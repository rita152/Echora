//! Live, persisted UI language selection. Uses its own focus target so the
//! dropdown works independently of backend configuration controls.

use gpui::{Context, IntoElement, MouseButton, Role, deferred, div, prelude::*, px, svg};

use super::{ChangeLanguage, SettingsView};
use crate::{
    i18n::{self, Language},
    theme::Theme,
};

impl SettingsView {
    fn select_language(&mut self, language: Language, cx: &mut Context<Self>) {
        self.language_menu_open = false;
        for input in [
            &self.config_input,
            &self.plugin_search_input,
            &self.marketplace_source_input,
        ] {
            input.update(cx, |_, cx| cx.notify());
        }
        cx.emit(ChangeLanguage(language));
        cx.notify();
    }

    pub(super) fn language_control(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let current = i18n::language();
        let mut control = div()
            .id("settings-language")
            .role(Role::Button)
            .aria_label(i18n::text("语言"))
            .aria_value(current.label())
            .aria_expanded(self.language_menu_open)
            .track_focus(&self.language_focus)
            .relative()
            .min_w(px(132.0))
            .min_h(px(28.0))
            .px(px(12.0))
            .rounded(px(12.5))
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_control)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .text_size(px(14.0))
            .line_height(px(18.0))
            .text_color(theme.text)
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .focus_visible(move |style| style.border_color(theme.accent))
            .on_click(cx.listener(move |this, event, window, cx| {
                if matches!(event, gpui::ClickEvent::Keyboard(_)) && this.language_menu_open {
                    this.select_language(Language::ALL[this.language_menu_index], cx);
                    return;
                }
                this.language_focus.focus(window, cx);
                this.language_menu_open = !this.language_menu_open;
                this.language_menu_index = Language::ALL
                    .iter()
                    .position(|value| *value == current)
                    .unwrap_or(0);
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "down" | "up" if !this.language_menu_open => {
                        this.language_menu_open = true;
                        this.language_menu_index = Language::ALL
                            .iter()
                            .position(|value| *value == i18n::language())
                            .unwrap_or(0);
                    }
                    "down" if this.language_menu_open => {
                        this.language_menu_index =
                            (this.language_menu_index + 1) % Language::ALL.len();
                    }
                    "up" if this.language_menu_open => {
                        this.language_menu_index = (this.language_menu_index + Language::ALL.len()
                            - 1)
                            % Language::ALL.len();
                    }
                    "escape" => this.language_menu_open = false,
                    "tab" => {
                        this.language_menu_open = false;
                        cx.notify();
                        cx.propagate();
                        return;
                    }
                    _ => {
                        cx.propagate();
                        return;
                    }
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .child(current.label())
            .child(
                svg()
                    .path("icons/chevron-down.svg")
                    .size(px(12.0))
                    .text_color(theme.text_tertiary),
            );

        let bounds = self.language_bounds.clone();
        control = control.child(
            gpui::canvas(move |rect, _, _| bounds.set(Some(rect)), |_, (), _, _| {})
                .absolute()
                .size_full(),
        );
        if self.language_menu_open {
            let anchor_bounds = self.language_bounds.clone();
            let mut menu = div()
                .id("settings-language-menu")
                .role(Role::ListBox)
                .aria_label(i18n::text("语言"))
                .absolute()
                .top(px(34.0))
                .right_0()
                .w(px(196.0))
                .p(px(5.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(theme.border)
                .bg(theme.surface)
                .shadow_md()
                .flex()
                .flex_col()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down_out(cx.listener(
                    move |this, event: &gpui::MouseDownEvent, _, cx| {
                        if anchor_bounds
                            .get()
                            .is_some_and(|bounds| bounds.contains(&event.position))
                        {
                            return;
                        }
                        this.language_menu_open = false;
                        cx.notify();
                    },
                ));
            for (index, language) in Language::ALL.into_iter().enumerate() {
                menu = menu.child(
                    div()
                        .id(("settings-language-option", index))
                        .role(Role::ListBoxOption)
                        .aria_label(language.label())
                        .aria_selected(language == current)
                        .when(index == self.language_menu_index, |row| {
                            row.aria_active_descendant()
                        })
                        .min_h(px(32.0))
                        .px(px(10.0))
                        .rounded(px(7.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .when(index == self.language_menu_index, |row| {
                            row.bg(theme.sidebar_hover)
                        })
                        .hover(move |style| style.bg(theme.sidebar_hover))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.select_language(language, cx);
                        }))
                        .child(language.label())
                        .when(language == current, |row| row.child("✓")),
                );
            }
            control = control.child(deferred(menu));
        }
        control.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, TestApp, WindowBounds, WindowOptions, point, size};
    use std::sync::Arc;

    fn press(window: &mut gpui::TestAppWindow<SettingsView>, key: &str) {
        // simulate_keystroke dispatches only key-down; accessible buttons
        // activate on key-up, just as they do in the native application.
        window.simulate_keystroke(key);
        window.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse(key).unwrap(),
        });
        window.draw();
    }

    #[test]
    fn keyboard_selection_closes_once_and_shell_dismissal_keeps_choice() {
        let previous = i18n::language();
        i18n::set_language(Language::English);
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.0), px(0.0)),
                    size(px(1440.0), px(900.0)),
                ))),
                ..Default::default()
            },
            |_, cx| {
                SettingsView::new(
                    crate::theme::ThemeMode::Light,
                    Arc::new(crate::agent::CodexAppServerBackend::new()),
                    cx,
                )
            },
        );
        window.update(|settings, window, cx| {
            cx.subscribe(&cx.entity(), |_, _, event: &ChangeLanguage, cx| {
                i18n::set_language(event.0);
                cx.refresh_windows();
            })
            .detach();
            settings.language_focus.focus(window, cx);
        });
        window.draw();
        press(&mut window, "enter");
        assert!(window.read(|settings, _| settings.language_menu_open));
        press(&mut window, "down");
        press(&mut window, "enter");
        assert_eq!(i18n::language(), Language::SimplifiedChinese);
        assert!(!window.read(|settings, _| settings.language_menu_open));
        press(&mut window, "space");
        assert!(window.read(|settings, _| settings.language_menu_open));
        window.update(|settings, window, cx| settings.dismiss_transient(window, cx));
        assert!(!window.read(|settings, _| settings.language_menu_open));
        assert_eq!(i18n::language(), Language::SimplifiedChinese);
        press(&mut window, "enter");
        window.update(|settings, window, cx| settings.advance_focus(false, window, cx));
        assert!(!window.read(|settings, _| settings.language_menu_open));
        i18n::set_language(previous);
    }
}
