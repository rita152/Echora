//! Page shell: pane layout, overlays, and the shared page background.

use gpui::{AnimationExt, Div, SharedString, div, prelude::*, px};

use super::{PullRequestsView, theme::*};
use crate::components::icons::icon;
use crate::theme::ui_font;

type ClickHandler = Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App)>;

/// What a composer's footer buttons do.
pub(super) struct ComposerActions {
    pub cancel: Option<ClickHandler>,
    pub post_label: &'static str,
    pub post_enabled: bool,
    pub post: ClickHandler,
}

impl gpui::Render for PullRequestsView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        self.classic_scrollbars = !cx.should_auto_hide_scrollbars();
        self.measure_code_width(window);
        self.take_capture_offset();
        self.prepare_diff_viewport(window, cx);
        // Expand once the list has placed the hunk, so the anchor holds.
        if self.capture_expand_first_gap && self.hunk_offset(0, 0).is_some() {
            self.capture_expand_first_gap = false;
            self.expand_gap(0, 0, false, cx);
        }
        self.apply_expand_anchor(cx);
        // A tree opened before the diff arrived selects its top file now.
        if self.file_tree_open && self.selected_file.is_none() {
            self.selected_file = self.top_diff_file();
        }
        self.apply_capture_diff_offset(cx);
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
            page = page.child(detail);
        }
        let split = !fullscreen && !self.compact();
        if !self.compact() || self.selected.is_some() || self.fullscreen {
            // The app-shell detail panel: `border-l border-default` on the
            // surface, so its content starts one pixel in.
            page = page.child(
                div()
                    .flex_none()
                    .w(px(self.pane_width))
                    .h_full()
                    .flex()
                    .when(split, |pane| {
                        pane.border_l(px(1.0)).border_color(theme.border)
                    })
                    .child(self.detail_pane(window, cx)),
            );
        }
        if split {
            page = page
                // `shadow-[-8px_0_16px_-8px_rgb(0_0_0/0.18)]` on a 1px strip at
                // the panel's leading edge, cast back over the list.
                .child(
                    div()
                        .absolute()
                        .left(px(self.list_width))
                        .top_0()
                        .w(px(1.0))
                        .h_full()
                        .shadow(vec![
                            gpui::BoxShadow::new(px(-8.0), px(0.0), gpui::rgba(0x0000002e).into())
                                .blur_radius(px(16.0))
                                .spread_radius(px(-8.0)),
                        ]),
                )
                .child(self.pane_separator(cx));
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
            // Radix places the menu (`m-px`) 2px under the trigger, flush with
            // its end edge.
            overlays.push(self.popup_at(
                "pr-filter",
                gpui::point(px(-1.0), px(2.0)),
                self.filter_menu(cx),
            ));
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
                    // The sub-content opens 5px beyond the menu edge (the row
                    // sits 4px inside it), aligned with the menu's top.
                    popup = popup.position(bounds.top_right() + gpui::point(px(9.0), px(-4.0)));
                }
                overlays.push(gpui::deferred(popup).into_any_element());
            }
        }
        overlays
    }

    /// The reference keeps both panes while the list can hold 320px beside
    /// the detail panel; below that the page shows one pane at a time.
    pub(super) fn compact(&self) -> bool {
        !self.fullscreen && self.compact_layout
    }

    pub(super) fn control_anchor<K: Into<String>>(&self, key: K) -> impl IntoElement + use<K> {
        let state = self.control_bounds.clone();
        let key = key.into();
        // Pinned to the padding box, so the bounds are the whole control's
        // rather than starting after its leading padding.
        gpui::canvas(
            move |bounds, _, _| bounds,
            move |_, bounds, _, _| {
                state.borrow_mut().insert(key.clone(), bounds);
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    /// `Resize workspace panes`: a 16px strip centred on the pane boundary.
    /// Hovering shows its fading hairline; dragging resizes the detail panel
    /// between 320px and `main - 352`, and the host stores the new ratio.
    fn pane_separator(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let resizing = self.detail_resize.is_some();
        let start = cx.entity();
        let drag = cx.entity();
        let end = cx.entity();
        let end_out = cx.entity();
        let line = |alpha: f32| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(7.5))
                .w(px(1.0))
                .flex()
                .flex_col()
                .child(div().flex_1().w_full().bg(gpui::linear_gradient(
                    180.0,
                    gpui::linear_color_stop(theme.text.alpha(0.0), 0.0),
                    gpui::linear_color_stop(theme.text.alpha(alpha), 1.0),
                )))
                .child(div().flex_1().w_full().bg(gpui::linear_gradient(
                    180.0,
                    gpui::linear_color_stop(theme.text.alpha(alpha), 0.0),
                    gpui::linear_color_stop(theme.text.alpha(0.0), 1.0),
                )))
        };
        div()
            .id("pr-pane-separator")
            .group("pr-pane-separator")
            .absolute()
            .top_0()
            .left(px(self.list_width - 8.0))
            .w(px(16.0))
            .h_full()
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .role(gpui::Role::Splitter)
            .aria_label("Resize workspace panes")
            .on_mouse_down(gpui::MouseButton::Left, move |event, _, cx| {
                cx.stop_propagation();
                start.update(cx, |view, _| {
                    view.begin_detail_resize(f32::from(event.position.x))
                });
            })
            .on_mouse_move(move |event, _, cx| {
                if event.pressed_button == Some(gpui::MouseButton::Left) {
                    drag.update(cx, |view, cx| {
                        view.drag_detail_resize(f32::from(event.position.x), cx)
                    });
                }
            })
            .on_mouse_up(gpui::MouseButton::Left, move |_, _, cx| {
                end.update(cx, |view, cx| view.end_detail_resize(cx));
            })
            .on_mouse_up_out(gpui::MouseButton::Left, move |_, _, cx| {
                end_out.update(cx, |view, cx| view.end_detail_resize(cx));
            })
            .child(
                line(0.25)
                    .when(!resizing, |line| line.invisible())
                    .group_hover("pr-pane-separator", |style| style.visible()),
            )
    }

    /// The reference's `Button` at `size="toolbar"`: 28px tall, `px-2` inside
    /// a transparent 1px border, 12.5px radius, 13/18 type. `secondary` sits on
    /// the 5% soft fill (10% on hover); `ghost` is transparent with tertiary
    /// text and the 8% ghost hover.
    pub(super) fn toolbar_button(
        id: impl Into<gpui::ElementId>,
        theme: PrTheme,
        secondary: bool,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .role(gpui::Role::Button)
            .flex_none()
            .h(px(28.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(12.5))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .whitespace_nowrap()
            .when(secondary, |button| {
                button
                    .bg(theme.control)
                    .text_color(theme.text)
                    .hover(move |style| style.bg(theme.control_hover))
            })
            .when(!secondary, |button| {
                button
                    .text_color(theme.text_muted)
                    .hover(move |style| style.bg(theme.row_hover))
            })
    }

    /// A small rotating spinner for status slots.
    pub(super) fn spinner(color: gpui::Rgba, size: f32) -> impl IntoElement {
        icon("pr-spinner", color.into())
            .size(px(size))
            .with_animation(
                "pr-spinner",
                gpui::Animation::new(std::time::Duration::from_millis(800)).repeat(),
                |svg, delta| {
                    svg.with_transformation(gpui::Transformation::rotate(gpui::percentage(delta)))
                },
            )
    }

    /// Radix menu surface: `p-1`, 20px radius, 90% surface, a doubled 0.5px
    /// ring, and `0 8px 16px -4px` shadow.
    pub(super) fn menu_surface(id: &'static str, theme: PrTheme) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .role(gpui::Role::Menu)
            .p(px(4.0))
            .flex()
            .flex_col()
            .rounded(px(20.0))
            .bg(theme.menu_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .text_size(px(MENU_TEXT_SIZE))
            .line_height(px(MENU_LINE_HEIGHT))
            .text_color(theme.text)
    }

    /// A control with the app's Radix tooltip: `label` after
    /// `TOOLTIP_DELAY` of hover. Pressing the control closes it.
    pub(super) fn with_tooltip(
        &self,
        element: gpui::Stateful<Div>,
        key: &str,
        label: impl Into<SharedString>,
        placement: TooltipPlacement,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let key = format!("tip:{key}");
        let open = self.tooltip.as_deref() == Some(key.as_str());
        let tooltip = open.then(|| self.tooltip_overlay(&key, label.into(), placement));
        let hover_view = cx.entity();
        let press_view = cx.entity();
        let hover_key = key.clone();
        element
            .relative()
            .child(self.control_anchor(key))
            .children(tooltip)
            .on_hover(move |hovered, _, cx| {
                let key = hover_key.clone();
                hover_view.update(cx, |view, cx| {
                    view.set_tooltip_hover(key, *hovered, super::TOOLTIP_DELAY, cx)
                });
            })
            .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                press_view.update(cx, |view, cx| view.dismiss_tooltip(cx));
            })
    }

    /// The tooltip pill (`role=tooltip`): 13/18 type tracked -0.15px, `px-3 py-[5px]`, 20px
    /// radius, a 5% hairline and `0 8px 18px` shadow, 2px from the trigger
    /// and kept 8px inside the window. Radix tooltips center on the trigger
    /// and wrap at 512px; the file tree's sits under the row's start.
    pub(super) fn tooltip_overlay(
        &self,
        key: &str,
        label: SharedString,
        placement: TooltipPlacement,
    ) -> gpui::AnyElement {
        let theme = self.theme();
        let mut anchored = gpui::anchored().snap_to_window_with_margin(px(8.0));
        if let Some(bounds) = self.control_bounds.borrow().get(key) {
            let gap = px(2.0);
            anchored = match placement {
                TooltipPlacement::Above => anchored
                    .anchor(gpui::Anchor::BottomCenter)
                    .position(bounds.top_center() - gpui::point(px(0.0), gap)),
                TooltipPlacement::Below => anchored
                    .anchor(gpui::Anchor::TopCenter)
                    .position(bounds.bottom_center() + gpui::point(px(0.0), gap)),
                TooltipPlacement::BelowStart => {
                    anchored.position(bounds.bottom_left() + gpui::point(px(0.0), gap))
                }
            };
        }
        let tree = placement == TooltipPlacement::BelowStart;
        gpui::deferred(
            anchored.child(
                div()
                    .max_w(px(if tree { 320.0 } else { 512.0 }))
                    .px(px(12.0))
                    .py(px(5.0))
                    .rounded(px(20.0))
                    .border(px(1.0))
                    .border_color(gpui::Rgba {
                        a: 0.05,
                        ..theme.tooltip_text
                    })
                    .bg(theme.tooltip_surface)
                    .shadow(vec![
                        gpui::BoxShadow::new(px(0.0), px(8.0), gpui::rgba(0x0f172a33).into())
                            .blur_radius(px(18.0)),
                    ])
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                    // The tooltip text theme's `letter-spacing: -0.15px`.
                    .font_features(gpui::FontFeatures::default().with_letter_spacing(-0.15 / 13.0))
                    .text_color(theme.tooltip_text)
                    // `whitespace-normal break-words`, whatever the trigger sets.
                    .whitespace_normal()
                    .when(!tree, |tooltip| tooltip.text_center())
                    .child(label),
            ),
        )
        .with_priority(2)
        .into_any_element()
    }

    /// The opaque dropdown surface (`bg-surface-elevated-secondary`, white /
    /// `rgb(45,45,45)`) with a single 0.5px ring, as the review toolbar's
    /// menus draw it.
    pub(super) fn solid_menu_surface(id: &'static str, theme: PrTheme) -> gpui::Stateful<Div> {
        Self::menu_surface(id, theme)
            .bg(theme.popover_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
    }

    /// `Menu.Separator`: a 1px rule in a `py-1 px-2` block.
    pub(super) fn menu_rule(theme: PrTheme) -> Div {
        div()
            .flex_none()
            .py(px(4.0))
            .px(px(8.0))
            .child(div().h(px(1.0)).bg(theme.border))
    }

    /// A selectable item's `ItemIcon` slot (16px): the check while chosen,
    /// otherwise empty so labels stay aligned.
    pub(super) fn menu_check_slot(checked: bool, theme: PrTheme) -> gpui::AnyElement {
        if checked {
            Self::menu_icon("pr-menu-check-xs", theme).into_any_element()
        } else {
            div()
                .flex_none()
                .size(px(MENU_ICON_SIZE))
                .into_any_element()
        }
    }

    /// A menu item's trailing `+N -M` counts, or an em dash when both are 0.
    pub(super) fn menu_diff_stats(additions: u64, deletions: u64, theme: PrTheme) -> Div {
        let stats = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.0))
            .line_height(px(13.0))
            .font_features(super::list::stats_font_features());
        if additions == 0 && deletions == 0 {
            return stats.text_color(theme.text_muted).child("\u{2014}");
        }
        stats
            .child(div().text_color(theme.additions_text).child(format!(
                "+{}",
                crate::pull_requests::format_count(additions)
            )))
            .child(div().text_color(theme.deletions_text).child(format!(
                "-{}",
                crate::pull_requests::format_count(deletions)
            )))
    }

    /// One menu item: 28.56px, `px-2 py-[5px]`, 15px radius, 6px gap.
    /// `highlighted` keeps the fill of an open submenu's trigger.
    pub(super) fn menu_row(
        id: impl Into<gpui::ElementId>,
        theme: PrTheme,
        highlighted: bool,
        disabled: bool,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .group("pr-menu-row")
            .role(gpui::Role::MenuItem)
            .flex_none()
            .h(px(MENU_ROW_HEIGHT))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(15.0))
            .when(disabled, |row| row.opacity(0.5))
            .when(highlighted, |row| row.bg(theme.menu_hover))
            .when(!disabled, |row| {
                row.hover(move |style| style.bg(theme.menu_hover))
            })
    }

    /// A menu item's 16px leading glyph at `opacity-75`, fully opaque while
    /// its row is hovered (`group-hover:opacity-100`).
    pub(super) fn menu_icon(name: &'static str, theme: PrTheme) -> impl IntoElement {
        Self::tinted_menu_icon(name, theme.text)
    }

    /// [`Self::menu_icon`] in the row's own color, as a `danger` item draws it.
    pub(super) fn tinted_menu_icon(name: &'static str, color: gpui::Rgba) -> impl IntoElement {
        icon(name, color.into())
            .flex_none()
            .size(px(MENU_ICON_SIZE))
            .opacity(0.75)
            .group_hover("pr-menu-row", |style| style.opacity(1.0))
    }

    /// A GitHub avatar at `size`: `rounded-full bg-white`, the cached image
    /// once it has downloaded.
    pub(super) fn avatar(&self, url: Option<&str>, size: f32) -> Div {
        let path = url.and_then(|url| self.avatar_path(url)).cloned();
        div()
            .flex_none()
            .size(px(size))
            .rounded_full()
            .overflow_hidden()
            .bg(gpui::white())
            .when_some(path, |avatar, path| {
                avatar.child(
                    gpui::img(path)
                        .size(px(size))
                        .rounded_full()
                        .object_fit(gpui::ObjectFit::Cover),
                )
            })
    }

    /// A classic-scroller thumb for `scroll`, drawn when the system shows
    /// scroll bars and the content overflows.
    pub(super) fn scrollbar_thumb(
        &self,
        scroll: &gpui::ScrollHandle,
        cx: &gpui::App,
    ) -> Option<Div> {
        self.scrollbar_thumb_for(
            f32::from(scroll.bounds().size.height),
            f32::from(scroll.max_offset().y),
            -f32::from(scroll.offset().y),
            cx,
        )
    }

    /// The classic-scroller thumb for a `viewport` tall scroller scrolled
    /// `offset` of `max_offset`.
    pub(super) fn scrollbar_thumb_for(
        &self,
        viewport: f32,
        max_offset: f32,
        offset: f32,
        cx: &gpui::App,
    ) -> Option<Div> {
        if cx.should_auto_hide_scrollbars() {
            return None;
        }
        let max_offset = max_offset.max(0.0);
        if max_offset <= 0.5 || viewport <= 0.0 {
            return None;
        }
        let track = (viewport - SCROLLBAR_TRACK_INSET_TOP - SCROLLBAR_TRACK_INSET_BOTTOM).max(0.0);
        let thumb = (track * viewport / (viewport + max_offset))
            .max(SCROLLBAR_THUMB_MIN_LENGTH)
            .min(track);
        let progress = (offset / max_offset).clamp(0.0, 1.0);
        Some(
            div()
                .absolute()
                .top(px(SCROLLBAR_TRACK_INSET_TOP + (track - thumb) * progress))
                .right(px(SCROLLBAR_THUMB_INSET_RIGHT))
                .w(px(SCROLLBAR_THUMB_WIDTH))
                .h(px(thumb))
                .rounded_full()
                .bg(self.theme().scrollbar_thumb),
        )
    }

    /// A popover whose top-right corner sits at the control's bottom-right
    /// plus `offset`.
    pub(super) fn popup_at(
        &self,
        key: &str,
        offset: gpui::Point<gpui::Pixels>,
        menu: impl IntoElement,
    ) -> gpui::AnyElement {
        let mut popup = gpui::anchored()
            .anchor(gpui::Anchor::TopRight)
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(menu),
            );
        if let Some(bounds) = self.control_bounds.borrow().get(key) {
            popup = popup.position(bounds.bottom_right() + offset);
        }
        gpui::deferred(popup).into_any_element()
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

    /// The reference's `ComposerLayout` for comments and replies: the 14/28
    /// field (`pt-2.5 px-3`), then a 28px footer with the viewer's avatar, an
    /// optional ghost cancel, and the round submit arrow (`opacity-40` while
    /// the field is empty). The page composer sits on the composer surface
    /// inside a hairline; the reply composer is borderless inside its footer.
    pub(super) fn composer(
        &self,
        editor: gpui::Entity<super::FileEditor>,
        key: &'static str,
        surface: bool,
        actions: ComposerActions,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let ComposerActions {
            cancel: on_cancel,
            post_label,
            post_enabled,
            post: on_post,
        } = actions;
        let theme = self.theme();
        let lines = editor.read(cx).visual_line_count().clamp(1, 12);
        let viewer_avatar = self
            .detail
            .as_ref()
            .and_then(|detail| detail.viewer.as_ref())
            .and_then(|viewer| viewer.avatar_url.clone());
        div()
            .rounded(px(12.5))
            .overflow_hidden()
            .flex()
            .flex_col()
            .when(surface, |composer| {
                composer
                    .border(px(1.0))
                    .border_color(theme.border)
                    .bg(theme.composer_surface)
            })
            .child(div().px(px(12.0)).pt(px(10.0)).child(Self::editor_frame(
                editor,
                28.0 * lines as f32,
                key,
            )))
            .child(
                div()
                    .mt(px(4.0))
                    .mb(px(8.0))
                    .px(px(8.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .child(self.avatar(viewer_avatar.as_deref(), 24.0))
                    .child(
                        div().flex_1().min_w(px(0.0)).flex().justify_end().child(
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .when_some(on_cancel, |row, on_cancel| {
                                    row.child(
                                        Self::toolbar_button(
                                            SharedString::from(format!("{key}-cancel")),
                                            theme,
                                            false,
                                        )
                                        .w(px(28.0))
                                        .px(px(0.0))
                                        .justify_center()
                                        .aria_label("Cancel")
                                        .on_click(on_cancel)
                                        .child(icon("pr-close", theme.text.into()).size(px(16.0))),
                                    )
                                })
                                .child(
                                    div()
                                        .id(SharedString::from(format!("{key}-post")))
                                        .flex_none()
                                        .size(px(28.0))
                                        .rounded_full()
                                        .border(px(1.0))
                                        .border_color(theme.border)
                                        .bg(theme.inverted_surface)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .role(gpui::Role::Button)
                                        .aria_label(post_label)
                                        .when(!post_enabled, |button| button.opacity(0.4))
                                        .when(post_enabled, |button| button.on_click(on_post))
                                        .child(
                                            icon("pr-arrow-up", theme.inverted_text.into())
                                                .size(px(16.0)),
                                        ),
                                ),
                        ),
                    ),
            )
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
}

/// Where a tooltip opens against its trigger.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TooltipPlacement {
    Above,
    Below,
    /// Under the trigger's start edge, as the file tree places its tooltip.
    BelowStart,
}
