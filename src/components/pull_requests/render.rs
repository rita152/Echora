//! Page shell: pane layout, overlays, and the shared page background.

use gpui::{Div, SharedString, div, prelude::*, px};

use super::{PullRequestsView, theme::*};
use crate::components::icons::icon;
use crate::theme::ui_font;

impl gpui::Render for PullRequestsView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        self.measure_code_width(window);
        self.take_capture_offset();
        self.prepare_diff_viewport(window, cx);
        self.apply_pending_file_scroll(cx);
        let fullscreen = self.fullscreen || (self.compact() && self.selected.is_some());
        let detail = if fullscreen {
            None
        } else {
            Some(self.list_pane(cx))
        };
        let view = cx.entity();
        let dismiss = view.clone();
        let mut page = div()
            .id("pull-requests-page")
            .relative()
            .size_full()
            .flex()
            .font(ui_font())
            .text_color(theme.text)
            .bg(theme.surface)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    view.dismiss_menus(cx);
                    view.cancel_inline_comment(cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                dismiss.update(cx, |view, cx| {
                    view.dismiss_menus(cx);
                });
            });
        if let Some(detail) = detail {
            // The reference layers its resize strip over the pane boundary, so the
            // two panes sit flush: list 518 + detail 646 at a 1440px window.
            page = page.child(detail);
        }
        if !self.compact() || self.selected.is_some() || self.fullscreen {
            page = page.child(self.detail_pane(window, cx));
        }
        if !fullscreen && !self.compact() {
            page = page.child(
                div()
                    .absolute()
                    .left(px(self.list_width - 0.5))
                    .top(px(0.0))
                    .w(px(1.0))
                    .h_full()
                    .bg(theme.border),
            );
        }
        if !fullscreen {
            page = page.children(self.filter_overlays(cx));
        }
        if self.reviewers_open {
            page = page.child(self.popup("pr-request-reviewers", self.reviewers_dialog(cx)));
        }
        if let Some(notice) = if self.mutation_pending {
            Some("Saving to GitHub…".to_string())
        } else {
            self.notice.clone()
        } {
            page = page.child(
                div()
                    .absolute()
                    .bottom(px(24.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .max_w(px((self.pane_width - 32.0).max(200.0)))
                            .px(px(12.0))
                            .py(px(6.0))
                            .rounded(px(12.0))
                            .bg(theme.menu_surface)
                            .text_size(px(13.0))
                            .text_color(theme.text)
                            .shadow(vec![
                                gpui::BoxShadow::new(px(0.0), px(8.0), theme.menu_shadow.into())
                                    .blur_radius(px(16.0))
                                    .spread_radius(px(-4.0)),
                            ])
                            .child(notice),
                    ),
            );
        }
        page
    }
}

impl PullRequestsView {
    /// Everything positioned over the page: the filter popover and its submenu,
    /// the status and description menus of the detail pane, and the diff
    /// toolbar menus.
    fn filter_overlays(&self, cx: &mut gpui::Context<Self>) -> Vec<gpui::AnyElement> {
        let mut overlays = Vec::new();
        if self.list_menu.is_some() {
            overlays.push(self.popup("pr-filter", self.filter_menu(cx)));
            if let Some(submenu) = self.filter_submenu(cx) {
                let key = match self.filter_submenu {
                    Some(super::FilterSubmenu::Status) => "pr-filter-Status",
                    _ => "pr-filter-Repository",
                };
                let bounds = self.control_bounds.borrow().get(key).copied();
                let mut popup = gpui::anchored()
                    .snap_to_window_with_margin(px(8.0))
                    .child(submenu);
                if let Some(bounds) = bounds {
                    popup = popup.position(bounds.top_right());
                }
                overlays.push(gpui::deferred(popup).into_any_element());
            }
        }
        overlays
    }

    pub(super) fn compact(&self) -> bool {
        self.pane_width + self.list_width < 850.0 && !self.fullscreen
    }

    pub(super) fn control_anchor<K: Into<String>>(&self, key: K) -> impl IntoElement + use<K> {
        let state = self.control_bounds.clone();
        let key = key.into();
        gpui::canvas(
            move |bounds, _, _| bounds,
            move |_, bounds, _, _| {
                state.borrow_mut().insert(key.clone(), bounds);
            },
        )
        .absolute()
        .size_full()
    }

    pub(super) fn popup(&self, key: &str, menu: impl IntoElement) -> gpui::AnyElement {
        let mut popup = gpui::anchored()
            .anchor(gpui::Anchor::TopRight)
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(menu),
            );
        if let Some(bounds) = self.control_bounds.borrow().get(key) {
            popup = popup.position(bounds.bottom_right() + gpui::point(px(0.0), px(4.0)));
        }
        gpui::deferred(popup).into_any_element()
    }

    /// The reference's reviewer picker: a popover anchored under the
    /// `Reviewers` meta row (x 751.5 → 1040, y 241.5 → 341.5 at the reference
    /// window size). It has no scrim, no title, and no submit buttons: the
    /// search field sits above a divider, and choosing a user requests it.
    pub(super) fn reviewers_dialog(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let query_is_empty = self
            .reviewers_query
            .as_ref()
            .is_none_or(|query| query.read(cx).text().trim().is_empty());
        let mut results = div()
            .id("pr-reviewer-results")
            .max_h(px(300.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px(px(14.0))
            .py(px(18.0));
        if self.reviewers_results.is_empty() {
            results = results.child(
                div()
                    .text_size(px(14.0))
                    .text_color(theme.text_muted)
                    .child(if let Some(error) = &self.reviewers_error {
                        error.clone()
                    } else if self.reviewers_loading {
                        "Searching GitHub…".into()
                    } else if query_is_empty {
                        "Search by name or GitHub username".into()
                    } else {
                        "No users found".into()
                    }),
            );
        }
        for user in &self.reviewers_results {
            let login = user.login.clone();
            let selected = self.reviewers_selected.contains(&login);
            let select_view = view.clone();
            results = results.child(
                div()
                    .id(SharedString::from(format!("pr-reviewer-{login}")))
                    .aria_label(login.clone())
                    .h(px(28.5))
                    .px(px(4.0))
                    .py(px(5.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .when(selected, |row| row.bg(theme.menu_hover))
                    .hover(move |style| style.bg(theme.menu_hover))
                    .role(gpui::Role::MenuItem)
                    .on_click(move |_, _, cx| {
                        select_view.update(cx, |view, cx| view.toggle_reviewer(login.clone(), cx));
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(user.login.clone()),
                    )
                    .when(selected, |row| {
                        row.child(icon("check", theme.text.into()).size(px(14.0)))
                    }),
            );
        }
        div()
            .w(px(288.5))
            .rounded(px(20.0))
            .bg(theme.popover_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(16.0), theme.menu_shadow.into())
                    .blur_radius(px(32.0))
                    .spread_radius(px(-8.0)),
            ])
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("pr-reviewers-dialog")
                    .h(px(47.0))
                    .px(px(13.5))
                    .flex()
                    .items_center()
                    .gap(px(3.5))
                    .child(icon("search", theme.icon_muted.into()).size(px(20.0)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .children(self.reviewers_query.clone()),
                    ),
            )
            .child(div().h(px(1.0)).bg(theme.border))
            .child(results)
    }

    pub(super) fn editor_frame(
        editor: gpui::Entity<super::FileEditor>,
        height: f32,
        key: &'static str,
    ) -> Div {
        // FileEditor fills its parent; auto height would collapse it in these
        // content-sized summary and inline-comment cards.
        div()
            .debug_selector(move || key.into())
            .w_full()
            .h(px(height))
            .flex_none()
            .child(editor)
    }

    /// Shared pill button used by the detail header (`h-7 rounded-full px-2`).
    pub(super) fn header_pill(
        id: &'static str,
        label: &'static str,
        theme: PrTheme,
        filled: bool,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .flex_none()
            .h(px(28.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(12.5))
            .text_size(px(13.0))
            .cursor_pointer()
            .when(filled, |button| {
                button
                    .bg(theme.inverted_surface)
                    .text_color(theme.inverted_text)
            })
            .when(!filled, |button| {
                button
                    .bg(theme.control)
                    .text_color(theme.text)
                    .hover(move |style| style.bg(theme.control_hover))
            })
            .child(label)
    }
}
