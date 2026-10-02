//! The browser toolbar (`@container/browser-toolbar`, 48 px): the Back /
//! Next / Reload capsule, the centred address pill, the actions capsule and
//! the options button, with the loading bar along its bottom edge. Also the
//! address field's behaviour: inline completion, the suggestion dropdown and
//! its keyboard navigation.

use gpui::{
    Animation, AnimationExt, AnyElement, Context, Div, Focusable, FontWeight, HighlightStyle,
    MouseButton, Role, Stateful, StyledText, Window, div, prelude::*, px,
};

use super::{
    ADDRESS_CONTEXT, AddressDown, AddressEscape, AddressUp, BrowserPanel, BrowserTheme, PanelMenu,
};
use crate::{
    browser::{address, history::Suggestion},
    components::icons::icon,
};

/// `max-w-[770px]` on the address pill.
const ADDRESS_MAX_WIDTH: f32 = 770.0;
/// `@container browser-toolbar` breakpoints: `sm` (24rem) drops the address
/// area's padding.
const TOOLBAR_SMALL: f32 = 384.0;
/// Back, Next, the divider and Reload: 2 + 28 + 28 + 4 + 1 + 4 + 28 + 2.
const LEADING_WIDTH: f32 = 97.0;
/// The Downloads capsule, the gap and the options button.
const TRAILING_WIDTH: f32 = 32.0 + 6.0 + 32.0;
/// `container-xs` (20rem) the centring keeps free, plus three gaps.
const CENTERING_RESERVE: f32 = 320.0 + 12.0;

impl BrowserPanel {
    pub(super) fn render_toolbar(
        &self,
        theme: BrowserTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let tab = self.active_tab();
        let has_page = tab.is_some_and(|tab| tab.view.is_some() || tab.pending_url.is_some());
        let can_go_back = tab.is_some_and(|tab| tab.can_go_back());
        let can_go_forward = tab.is_some_and(|tab| tab.can_go_forward && tab.error.is_none());
        let loading = tab.is_some_and(|tab| tab.loading);
        let width = self.panel_width.get();
        // `_address_q37cr_2`: margins that centre the pill on the toolbar
        // rather than between its neighbours, while room allows.
        let centering_space = (width - LEADING_WIDTH - TRAILING_WIDTH - CENTERING_RESERVE).max(0.);
        let margin_start = (TRAILING_WIDTH - LEADING_WIDTH).clamp(0., centering_space);
        let margin_end = (LEADING_WIDTH - TRAILING_WIDTH).clamp(0., centering_space);

        let nav_button = |id: &'static str,
                          glyph: &'static str,
                          label: String,
                          enabled: bool,
                          color: gpui::Rgba| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .size(px(28.))
                .flex_none()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .when(!enabled, |button| button.opacity(0.4))
                .when(enabled, |button| {
                    button
                        .cursor_pointer()
                        .hover(move |button| button.bg(theme.ghost_hover))
                })
                .child(icon(glyph, color.into()).size(px(16.)))
        };
        let navigation = div()
            .id("browser-navigation")
            .role(Role::Group)
            .aria_label(crate::i18n::format!("导航" => "Navigation"))
            .h(px(32.))
            .p(px(2.))
            .flex_none()
            .rounded_full()
            .bg(theme.control)
            .shadow(theme.control_shadow())
            .flex()
            .items_center()
            .child(
                nav_button(
                    "browser-back",
                    "browser-back",
                    crate::i18n::format!("返回" => "Back"),
                    can_go_back,
                    theme.text_tertiary,
                )
                .when(can_go_back, |button| {
                    button.on_click(cx.listener(|panel, _, _, cx| panel.go_back(cx)))
                }),
            )
            .child(
                nav_button(
                    "browser-forward",
                    "browser-forward",
                    crate::i18n::format!("前进" => "Next"),
                    can_go_forward,
                    theme.text_tertiary,
                )
                .when(can_go_forward, |button| {
                    button.on_click(cx.listener(|panel, _, _, cx| panel.go_forward(cx)))
                }),
            )
            .child(
                div()
                    .mx(px(4.))
                    .w(px(1.))
                    .h(px(16.))
                    .flex_none()
                    .bg(theme.border),
            )
            .child(
                nav_button(
                    "browser-reload",
                    "browser-reload",
                    crate::i18n::format!("重新加载页面" => "Reload page"),
                    has_page,
                    theme.text,
                )
                .when(has_page, |button| {
                    button.on_click(cx.listener(|panel, _, _, cx| panel.reload(cx)))
                }),
            );

        let downloads_open = self.menu == Some(PanelMenu::Downloads);
        let actions = div()
            .id("browser-actions")
            .role(Role::Group)
            .aria_label(crate::i18n::format!("浏览器操作" => "Browser actions"))
            .size(px(32.))
            .flex_none()
            .rounded_full()
            .bg(theme.control)
            .shadow(theme.control_shadow())
            .child(
                div()
                    .id("browser-downloads")
                    .role(Role::Button)
                    .aria_label(crate::i18n::format!("下载" => "Downloads"))
                    .aria_expanded(downloads_open)
                    .relative()
                    .size_full()
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(downloads_open, |button| button.bg(theme.ghost_hover))
                    .hover(move |button| button.bg(theme.ghost_hover))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(
                        cx.listener(|panel, _, _, cx| panel.toggle_menu(PanelMenu::Downloads, cx)),
                    )
                    .child(icon("browser-downloads", theme.text.into()).size(px(16.)))
                    .child(self.anchor("downloads")),
            );
        let options_open = matches!(self.menu, Some(PanelMenu::Options | PanelMenu::ClearData));
        let options = div()
            .id("browser-options")
            .role(Role::Button)
            .aria_label(crate::i18n::format!("浏览器选项" => "Browser options"))
            .aria_expanded(options_open)
            .relative()
            .size(px(32.))
            .flex_none()
            .rounded_full()
            .bg(theme.control)
            .shadow(theme.control_shadow())
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_full()
                    .when(options_open, |layer| layer.bg(theme.ghost_hover))
                    .hover(move |layer| layer.bg(theme.ghost_hover)),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|panel, _, _, cx| panel.toggle_menu(PanelMenu::Options, cx)))
            .child(icon("browser-more", theme.text.into()).size(px(16.)))
            .child(self.anchor("options"));

        div()
            .id("browser-toolbar")
            .relative()
            .h(px(48.))
            .w_full()
            .flex_none()
            .bg(theme.toolbar)
            .px(px(8.))
            .py(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(navigation)
            .child(
                div()
                    .id("browser-address-area")
                    .min_w(px(0.))
                    .flex_1()
                    .when(width >= TOOLBAR_SMALL, |area| area.px(px(4.)))
                    .ml(px(margin_start))
                    .mr(px(margin_end))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(self.render_address_pill(theme, window, cx)),
            )
            .child(actions)
            .child(options)
            .when(loading, |toolbar| {
                toolbar.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(2.))
                        .bg(theme.info_soft)
                        .with_animation(
                            "browser-loading",
                            Animation::new(std::time::Duration::from_millis(2_000)).repeat(),
                            // Tailwind's `animate-pulse`: opacity 1 → 0.5 → 1.
                            |bar, delta| bar.opacity(1. - 0.5 * (1. - (2. * delta - 1.).abs())),
                        ),
                )
            })
    }

    fn render_address_pill(
        &self,
        theme: BrowserTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let focused = self.address.read(cx).focus_handle(cx).is_focused(window);
        let tab = self.active_tab();
        let url = tab.map(|tab| tab.url.clone()).unwrap_or_default();
        let resting = tab
            .and_then(|tab| tab.draft.clone())
            .unwrap_or_else(|| address::display_text(&url));
        // The pill's "Open in external browser" opens what the field would
        // load: the typed address while editing, else the page.
        let typed = self.address.read(cx).text().trim().to_owned();
        let target = if focused && !typed.is_empty() && typed != url {
            address::navigation_url(&typed)
        } else {
            url.clone()
        };
        let external = address::is_web_url(&target);
        let pill = div()
            .id("browser-address")
            .group("browser-address")
            .relative()
            .w_full()
            .max_w(px(ADDRESS_MAX_WIDTH))
            .min_w(px(0.))
            .h(px(32.))
            .p(px(2.))
            .rounded_full()
            .bg(theme.control)
            .shadow(theme.control_shadow())
            .border_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .overflow_hidden()
            .cursor_text()
            // `_inputSurface`: the ghost-hover wash while hovered or focused.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_full()
                    .when(focused, |wash| wash.bg(theme.ghost_hover))
                    .group_hover("browser-address", move |wash| wash.bg(theme.ghost_hover)),
            )
            .child(self.anchor("address"));
        if focused {
            pill.key_context(ADDRESS_CONTEXT)
                .on_action(cx.listener(|panel, _: &AddressUp, _, cx| panel.move_highlight(-1, cx)))
                .on_action(cx.listener(|panel, _: &AddressDown, _, cx| panel.move_highlight(1, cx)))
                .on_action(cx.listener(|panel, _: &AddressEscape, _, cx| panel.address_escape(cx)))
                .child(
                    div()
                        .relative()
                        .min_w(px(0.))
                        .flex_1()
                        .child(self.address.clone()),
                )
                .when(external, |pill| {
                    let url = target.clone();
                    pill.child(
                        div()
                            .id("browser-address-external")
                            .role(Role::Button)
                            .aria_label(crate::i18n::format!(
                                "在外部浏览器中打开" => "Open in external browser"
                            ))
                            .relative()
                            .size(px(28.))
                            .flex_none()
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(move |button| button.bg(theme.text.alpha(0.05)))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(
                                cx.listener(move |panel, _, _, cx| panel.open_external(&url, cx)),
                            )
                            .child(icon("browser-external", theme.text.into()).size(px(16.))),
                    )
                })
        } else {
            let empty = resting.is_empty();
            pill.on_mouse_down(
                MouseButton::Left,
                cx.listener(|panel, _, window, cx| {
                    let handle = panel.address.read(cx).focus_handle(cx);
                    window.focus(&handle, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                // `_addressText`: the page's short address, centred.
                div()
                    .relative()
                    .min_w(px(0.))
                    .flex_1()
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .min_w(px(0.))
                            .truncate()
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .text_color(if empty {
                                theme.text_tertiary
                            } else {
                                theme.text
                            })
                            .child(if empty {
                                crate::i18n::format!("搜索或输入网址" => "Search or enter a URL")
                            } else {
                                resting
                            }),
                    ),
            )
        }
    }

    /// Typing: refresh the suggestions and, when the text grew at its end,
    /// append the default page's remaining address, selected.
    pub(super) fn address_changed(&mut self, cx: &mut Context<Self>) {
        let (text, selection, composing) = {
            let input = self.address.read(cx);
            (
                input.text().to_owned(),
                input.selected_range(),
                input.is_composing(),
            )
        };
        let previous = std::mem::replace(&mut self.address_state.typed, text.clone());
        let grew = text.len() > previous.len()
            && text.starts_with(previous.as_str())
            && selection.start == text.len();
        let suggestions = self.store.read(cx).history.suggestions(&text);
        self.address_state.open = !text.trim().is_empty() && !suggestions.rows.is_empty();
        self.address_state.highlighted = self.address_state.open.then_some(0);
        self.address_state.hovered = None;
        if grew
            && !composing
            && let Some(completion) = suggestions.inline_completion.clone()
        {
            let completed = format!("{text}{completion}");
            let start = text.len();
            let end = completed.len();
            self.address.update(cx, |input, cx| {
                input.set_text_with_selection(completed, start..end, cx)
            });
        }
        self.address_state.suggestions = suggestions;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.draft = (!text.trim().is_empty()).then_some(text);
        }
        cx.notify();
    }

    pub(super) fn submit_address(&mut self, cx: &mut Context<Self>) {
        let text = self.address.read(cx).text().to_owned();
        let state = &self.address_state;
        let url = state
            .open
            .then_some(state.highlighted)
            .flatten()
            .and_then(|index| state.suggestions.rows.get(index))
            .map(|row| row.url().to_owned())
            .unwrap_or_else(|| address::navigation_url(&text));
        if url.is_empty() {
            return;
        }
        self.address_state.open = false;
        self.navigate(url, cx);
        self.focus_page_pending = true;
        cx.notify();
    }

    fn move_highlight(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.address_state.suggestions.rows.len();
        if !self.address_state.open || count == 0 {
            return;
        }
        let current = self
            .address_state
            .highlighted
            .map_or(-1, |index| index as isize);
        let next = (current + delta).rem_euclid(count as isize) as usize;
        self.address_state.highlighted = Some(next);
        let fill = self.address_state.suggestions.rows[next].fill_text();
        let end = fill.len();
        self.address.update(cx, |input, cx| {
            input.set_text_with_selection(fill, end..end, cx)
        });
        cx.notify();
    }

    fn address_escape(&mut self, cx: &mut Context<Self>) {
        if self.address_state.open {
            self.address_state.open = false;
            let typed = self.address_state.typed.clone();
            let end = typed.len();
            self.address.update(cx, |input, cx| {
                input.set_text_with_selection(typed, end..end, cx)
            });
        } else {
            let url = self
                .active_tab()
                .map(|tab| tab.url.clone())
                .unwrap_or_default();
            let current = self.address.read(cx).text().to_owned();
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.draft = None;
            }
            if current == url {
                // A second Escape gives the keyboard back to the page.
                self.focus_page_pending = self.showing_page();
            }
            let end = url.len();
            self.address.update(cx, |input, cx| {
                input.set_text_with_selection(url, 0..end, cx)
            });
        }
        cx.notify();
    }

    /// Focus selects the whole address (or the unsent draft).
    pub(super) fn address_focused(&mut self, cx: &mut Context<Self>) {
        let text = self
            .active_tab()
            .map(|tab| tab.draft.clone().unwrap_or_else(|| tab.url.clone()))
            .unwrap_or_default();
        self.address_state.typed = text.clone();
        self.address_state.open = false;
        self.menu = None;
        let end = text.len();
        self.address.update(cx, |input, cx| {
            input.set_text_with_selection(text, 0..end, cx)
        });
        cx.notify();
    }

    pub(super) fn address_blurred(&mut self, cx: &mut Context<Self>) {
        self.address_state.open = false;
        let text = self.address.read(cx).text().to_owned();
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.draft = (!text.trim().is_empty() && text != tab.url).then_some(text);
        }
        cx.notify();
    }

    /// Shows the active tab's address (or draft) in the field.
    pub(super) fn sync_address_text(&mut self, cx: &mut Context<Self>) {
        let text = self
            .active_tab()
            .map(|tab| tab.draft.clone().unwrap_or_else(|| tab.url.clone()))
            .unwrap_or_default();
        self.address_state.typed = text.clone();
        let end = text.len();
        let selection = if self.address_state.focused {
            0..end
        } else {
            end..end
        };
        self.address.update(cx, |input, cx| {
            input.set_text_with_selection(text, selection, cx)
        });
    }

    pub(super) fn render_address_dropdown(
        &self,
        theme: BrowserTheme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.address_state.open || !self.address_state.focused {
            return None;
        }
        let bounds = self.anchor_bounds("address")?;
        let rows = &self.address_state.suggestions.rows;
        let typed = self.address_state.typed.clone();
        let mut list = div()
            .id("browser-address-suggestions")
            .role(Role::ListBox)
            .aria_label(crate::i18n::format!("地址建议" => "Address suggestions"))
            .relative()
            .w(bounds.size.width)
            .p(px(4.))
            .rounded(px(15.))
            .border_1()
            .border_color(theme.dropdown_border)
            .bg(theme.dropdown)
            .shadow(theme.dropdown_shadow())
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (index, row) in rows.iter().enumerate() {
            list = list.child(self.render_suggestion(index, row, &typed, theme, cx));
        }
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(bounds.bottom_left())
                    .snap_to_window_with_margin(px(8.))
                    .child(list.child(self.overlay_hole(15.))),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    fn render_suggestion(
        &self,
        index: usize,
        row: &Suggestion,
        typed: &str,
        theme: BrowserTheme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let highlighted = self.address_state.highlighted == Some(index);
        let hovered = self.address_state.hovered == Some(index);
        let url = row.url().to_owned();
        let base = div()
            .id(("browser-suggestion", index))
            .group("browser-suggestion")
            .role(Role::ListBoxOption)
            .aria_selected(highlighted)
            .relative()
            .h(px(36.))
            .w_full()
            .flex_none()
            .px(px(10.))
            .pr(px(36.))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .gap(px(10.))
            .text_size(px(13.))
            .line_height(px(19.5))
            .cursor_pointer()
            .when(highlighted, |row| row.bg(theme.ghost_hover))
            .when(!highlighted && hovered, |row| {
                row.bg(theme.ghost_hover.alpha(theme.ghost_hover.a * 0.6))
            })
            .on_hover(cx.listener(move |panel, hovered: &bool, _, cx| {
                let next = hovered.then_some(index);
                if panel.address_state.hovered != next
                    && (*hovered || panel.address_state.hovered == Some(index))
                {
                    panel.address_state.hovered = next;
                    cx.notify();
                }
            }))
            .on_click(cx.listener({
                let url = url.clone();
                move |panel, _, _, cx| {
                    panel.address_state.open = false;
                    panel.navigate(url.clone(), cx);
                    panel.focus_page_pending = true;
                }
            }));
        match row {
            Suggestion::History { title, url, .. } => {
                let shown_url = address::completion_text(url);
                let title = if title.trim().is_empty() {
                    shown_url.clone()
                } else {
                    title.clone()
                };
                let label =
                    crate::i18n::format!("移除 {title} 建议" => "Remove suggestion for {title}");
                base.child(
                    icon("browser-history-globe", theme.text_tertiary.into())
                        .size(px(16.))
                        .flex_none(),
                )
                .child(
                    div()
                        .min_w(px(0.))
                        .flex_1()
                        .flex()
                        .items_baseline()
                        .gap(px(4.))
                        .child(
                            div()
                                .max_w(gpui::relative(0.6))
                                .flex_none()
                                .truncate()
                                .text_color(theme.text)
                                .child(bold_match(&title, typed)),
                        )
                        .child(div().flex_none().text_color(theme.text_tertiary).child("—"))
                        .child(
                            div()
                                .min_w(px(0.))
                                .flex_1()
                                .truncate()
                                .text_color(theme.text_tertiary)
                                .child(bold_match(&shown_url, typed)),
                        ),
                )
                .when(highlighted || hovered, |row| {
                    let url = url.clone();
                    row.child(
                        div()
                            .id(("browser-suggestion-remove", index))
                            .role(Role::Button)
                            .aria_label(label)
                            .absolute()
                            .right(px(4.))
                            .top(px(4.))
                            .size(px(28.))
                            .rounded(px(12.5))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(move |button| button.bg(theme.ghost_hover))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                cx.stop_propagation();
                                panel.remove_suggestion(url.clone(), cx);
                            }))
                            .child(
                                icon("browser-dismiss", theme.text_tertiary.into()).size(px(16.)),
                            ),
                    )
                })
            }
            Suggestion::Address { text, .. } => base
                .child(
                    icon("browser-history-globe", theme.text_tertiary.into())
                        .size(px(16.))
                        .flex_none(),
                )
                .child(
                    div()
                        .min_w(px(0.))
                        .flex_1()
                        .truncate()
                        .text_color(theme.text)
                        .child(text.clone()),
                ),
            Suggestion::Search { query, .. } => base
                .aria_label(
                    crate::i18n::format!("在网上搜索“{query}”" => "Search the web for ‘{query}’"),
                )
                .child(
                    icon("browser-search", theme.text_tertiary.into())
                        .size(px(16.))
                        .flex_none(),
                )
                .child(
                    div()
                        .min_w(px(0.))
                        .flex_1()
                        .flex()
                        .items_baseline()
                        .gap(px(4.))
                        .child(
                            div()
                                .min_w(px(0.))
                                .flex_1()
                                .truncate()
                                .text_color(theme.text)
                                .child(query.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(theme.text_tertiary)
                                .child(crate::i18n::format!("搜索网页" => "Search the web")),
                        ),
                ),
        }
    }

    fn remove_suggestion(&mut self, url: String, cx: &mut Context<Self>) {
        self.store
            .update(cx, |store, cx| store.remove_history(&url, cx));
        let typed = self.address_state.typed.clone();
        let suggestions = self.store.read(cx).history.suggestions(&typed);
        self.address_state.open = !suggestions.rows.is_empty();
        self.address_state.highlighted = self.address_state.open.then_some(0);
        self.address_state.suggestions = suggestions;
        cx.notify();
    }
}

/// The reference bolds (`font-semibold`) the typed text where it starts the
/// title or address.
fn bold_match(text: &str, typed: &str) -> StyledText {
    let typed = typed.trim();
    let lower = text.to_lowercase();
    let needle = typed.to_lowercase();
    let range = (!needle.is_empty() && lower.len() == text.len())
        .then(|| lower.find(&needle))
        .flatten()
        .map(|start| start..start + needle.len())
        .filter(|range| text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
    let styled = StyledText::new(text.to_owned());
    match range {
        Some(range) => styled.with_highlights([(
            range,
            HighlightStyle {
                font_weight: Some(FontWeight::SEMIBOLD),
                ..Default::default()
            },
        )]),
        None => styled,
    }
}
