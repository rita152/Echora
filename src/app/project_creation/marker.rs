//! Project symbol picker presentation.
use crate::{app::ChatApp, theme::Theme};
use gpui::{Context, Div, Focusable, KeyDownEvent, MouseButton, Window, div, prelude::*, px};
impl ChatApp {
    fn project_marker_choices(
        &self,
        cx: &Context<Self>,
    ) -> Vec<(crate::workspace::ProjectMarker, String)> {
        use crate::components::project_marker::{
            EMOJI_CATEGORIES, ICONS, RECOMMENDED_EMOJIS, emojis,
        };
        let query = self
            .project_creation
            .marker_search
            .read(cx)
            .text()
            .trim()
            .to_lowercase();
        if self.project_creation.marker_emoji {
            let category = self.project_creation.marker_category;
            let recommended = RECOMMENDED_EMOJIS
                .iter()
                .filter_map(|symbol| emojis().iter().find(|e| e.emoji == *symbol));
            let catalog = emojis().iter().filter(|e| {
                category.is_none()
                    || category.is_some_and(|n| n > 0 && e.group == EMOJI_CATEGORIES[n].1)
            });
            recommended
                .filter(|_| query.is_empty() && (category.is_none() || category == Some(0)))
                .chain(catalog.filter(|_| category != Some(0)))
                .filter(|e| {
                    query.is_empty()
                        || format!("{} {} {}", e.name, e.emoji, e.group)
                            .to_lowercase()
                            .contains(&query)
                })
                .map(|e| {
                    (
                        crate::workspace::ProjectMarker {
                            emoji: Some(e.emoji.clone()),
                            ..Default::default()
                        },
                        e.name.clone(),
                    )
                })
                .collect()
        } else {
            ICONS
                .iter()
                .filter(|(id, zh, en)| {
                    query.is_empty() || format!("{id} {zh} {en}").to_lowercase().contains(&query)
                })
                .map(|(id, zh, en)| {
                    (
                        crate::workspace::ProjectMarker {
                            icon: (*id).into(),
                            color: self.project_creation.marker.as_ref().map_or(0, |m| m.color),
                            emoji: None,
                        },
                        if crate::i18n::is_english() {
                            (*en).into()
                        } else {
                            (*zh).into()
                        },
                    )
                })
                .collect()
        }
    }

    pub(super) fn select_project_marker(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.project_creation.creating {
            return;
        }
        if let Some((marker, _)) = self.project_marker_choices(cx).into_iter().nth(index) {
            self.project_creation.marker = Some(marker);
            self.project_creation.marker_menu = false;
            self.project_creation.focus_pending = true;
            cx.notify();
        }
    }

    fn navigate_project_marker(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let count = self.project_marker_choices(cx).len();
        if count == 0 {
            return false;
        }
        let current = self.project_creation.marker_focused;
        let index = match key {
            "down" => current.map_or(0, |n| (n + 8).min(count - 1)),
            "up" => current.map_or(count - 1, |n| n.saturating_sub(8)),
            "left" if current.is_some() => current.unwrap().saturating_sub(1),
            "right" if current.is_some() => (current.unwrap() + 1).min(count - 1),
            _ => return false,
        };
        self.project_creation.marker_focused = Some(index);
        self.project_creation
            .marker_scroll
            .scroll_to_item(index / 8, gpui::ScrollStrategy::Nearest);
        cx.notify();
        true
    }

    fn handle_project_marker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .project_creation
            .marker_search
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
            && self.navigate_project_marker(&event.keystroke.key, cx)
        {
            cx.stop_propagation();
        }
    }

    pub(super) fn project_marker_menu(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        use crate::components::project_marker::{COLOR_NAMES, EMOJI_CATEGORIES, color};
        let emoji = self.project_creation.marker_emoji;
        let mut menu = div()
            .id("project-marker-menu")
            .role(gpui::Role::Dialog)
            .aria_label(crate::i18n::format!("项目标记" => "Project marker"))
            .absolute()
            .top(px(104.))
            .left(px(-139.))
            .w(px(360.))
            .rounded(px(15.))
            .bg(theme.model_picker_surface)
            .capture_key_down(cx.listener(Self::handle_project_marker_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(0.), theme.border.into()).spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.), px(8.), gpui::hsla(0., 0., 0., 0.12))
                    .blur_radius(px(16.))
                    .spread_radius(px(-4.)),
            ])
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.project_creation.marker_menu = false;
                cx.notify();
            }));
        let mut tabs = div().p(px(8.)).flex().gap(px(4.));
        for (value, label) in [
            (true, crate::i18n::format!("表情符号" => "Emoji")),
            (false, crate::i18n::format!("图标" => "Icons")),
        ] {
            tabs = tabs.child(
                div()
                    .id(format!("project-marker-tab-{value}"))
                    .role(gpui::Role::Button)
                    .aria_label(label.clone())
                    .focusable()
                    .tab_stop(true)
                    .h(px(24.))
                    .px(px(8.))
                    .rounded_full()
                    .text_size(px(crate::typography::ui_size(12.)))
                    .line_height(px(16.))
                    .cursor_pointer()
                    .when(emoji == value, |d| d.bg(theme.text.alpha(0.05)))
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_project_marker_tab(value, cx);
                        this.project_creation
                            .marker_search
                            .read(cx)
                            .focus_handle(cx)
                            .focus(window, cx);
                    }))
                    .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            this.set_project_marker_tab(value, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(label),
            );
        }
        menu = menu.child(tabs).child(
            div().px(px(10.)).pb(px(5.)).child(
                div()
                    .h(px(28.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.control)
                    .flex()
                    .items_center()
                    .child(self.project_creation.marker_search.clone()),
            ),
        );
        if emoji {
            let mut categories = div().px(px(10.)).py(px(5.)).flex().justify_between();
            for (index, (zh, en)) in EMOJI_CATEGORIES.iter().enumerate() {
                let label = if crate::i18n::is_english() { *en } else { *zh };
                categories = categories.child(
                    div()
                        .id(format!("project-emoji-category-{index}"))
                        .role(gpui::Role::Button)
                        .aria_label(label)
                        .focusable()
                        .tab_stop(true)
                        .size(px(24.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .when(self.project_creation.marker_category == Some(index), |d| {
                            d.bg(theme.sidebar_hover)
                        })
                        .hover(move |s| s.bg(theme.sidebar_hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_project_marker_category(index, cx)
                        }))
                        .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                this.set_project_marker_category(index, cx);
                                cx.stop_propagation();
                            }
                        }))
                        .child(
                            gpui::svg()
                                .path(format!("icons/project-marker-category-{index}.svg"))
                                .size(px(16.))
                                .text_color(theme.text_tertiary),
                        ),
                );
            }
            menu = menu.child(categories);
        }
        if !emoji {
            let mut colors = div().px(px(10.)).py(px(5.)).flex().justify_between();
            for (index, (zh, en)) in COLOR_NAMES.iter().enumerate() {
                let label = if crate::i18n::is_english() {
                    format!("Use {en}")
                } else {
                    format!("使用{zh}")
                };
                colors = colors.child(
                    div()
                        .id(format!("project-marker-color-{index}"))
                        .role(gpui::Role::Button)
                        .aria_label(label)
                        .focusable()
                        .tab_stop(true)
                        .size(px(32.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .when(
                            self.project_creation.marker.as_ref().map_or(0, |m| m.color) == index,
                            |d| d.bg(theme.text.alpha(0.05)),
                        )
                        .hover(move |s| s.bg(theme.sidebar_hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.project_creation
                                .marker
                                .get_or_insert_with(Default::default)
                                .color = index;
                            cx.notify();
                        }))
                        .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                this.project_creation
                                    .marker
                                    .get_or_insert_with(Default::default)
                                    .color = index;
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }))
                        .child(
                            div()
                                .size(px(24.))
                                .rounded_full()
                                .bg(color(index, self.mode)),
                        ),
                );
            }
            menu = menu.child(colors);
        }
        let symbols = self.project_marker_choices(cx);
        let focused = self.project_creation.marker_focused;
        let rows = symbols.len().div_ceil(8);
        let entity = cx.entity();
        let grid = gpui::uniform_list("project-symbol-grid", rows, move |range, _, cx| {
            entity.update(cx, |_, cx| {
                range
                    .map(|row| {
                        let mut line = div().h(px(36.)).flex().gap(px(4.)).px(px(10.));
                        for (index, (marker, label)) in
                            symbols.iter().enumerate().skip(row * 8).take(8)
                        {
                            let marker = marker.clone();
                            let key_marker = marker.clone();
                            line = line.child(
                                div()
                                    .id(format!("project-symbol-{index}"))
                                    .role(gpui::Role::Button)
                                    .aria_label(label.clone())
                                    .focusable()
                                    .tab_stop(true)
                                    .w(px(39.))
                                    .h(px(32.))
                                    .rounded(px(12.5))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .when(focused == Some(index), |d| d.bg(theme.sidebar_hover))
                                    .hover(move |s| s.bg(theme.sidebar_hover))
                                    .focus_visible(move |s| s.bg(theme.sidebar_hover))
                                    .child(crate::components::project_marker::glyph(
                                        Some(&marker),
                                        theme,
                                        20.,
                                    ))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.project_creation.marker = Some(marker.clone());
                                        this.project_creation.marker_menu = false;
                                        this.project_creation.focus_pending = true;
                                        cx.notify();
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, e: &KeyDownEvent, window, cx| {
                                            this.project_creation.marker_focused = Some(index);
                                            if this.navigate_project_marker(&e.keystroke.key, cx) {
                                                this.project_creation
                                                    .marker_search
                                                    .read(cx)
                                                    .focus_handle(cx)
                                                    .focus(window, cx);
                                                cx.stop_propagation();
                                            } else if matches!(
                                                e.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                this.project_creation.marker =
                                                    Some(key_marker.clone());
                                                this.project_creation.marker_menu = false;
                                                this.project_creation.focus_pending = true;
                                                cx.stop_propagation();
                                                cx.notify();
                                            }
                                        },
                                    )),
                            );
                        }
                        line
                    })
                    .collect::<Vec<_>>()
            })
        })
        .track_scroll(&self.project_creation.marker_scroll)
        .h(px((rows as f32 * 36.).min(if emoji { 232. } else { 144. })))
        .w_full();
        menu.child(div().py(px(5.)).child(grid))
    }

    fn set_project_marker_category(&mut self, index: usize, cx: &mut Context<Self>) {
        self.project_creation.marker_category = Some(index);
        self.project_creation.marker_focused = None;
        self.project_creation
            .marker_search
            .update(cx, |input, cx| input.set_text_silently("", cx));
        self.project_creation
            .marker_scroll
            .scroll_to_item(0, gpui::ScrollStrategy::Top);
        cx.notify();
    }

    fn set_project_marker_tab(&mut self, emoji: bool, cx: &mut Context<Self>) {
        self.project_creation.marker_emoji = emoji;
        self.project_creation.marker_category = None;
        self.project_creation.marker_focused = None;
        self.project_creation.marker_search.update(cx, |input, cx| {
            input.set_placeholder(
                if emoji {
                    crate::i18n::format!("搜索表情符号" => "Search emoji")
                } else {
                    crate::i18n::format!("搜索图标" => "Search icons")
                },
                cx,
            );
            input.set_text_silently("", cx);
            input.set_mode(self.mode, cx);
        });
        self.project_creation
            .marker_scroll
            .scroll_to_item(0, gpui::ScrollStrategy::Top);
        cx.notify();
    }
}
