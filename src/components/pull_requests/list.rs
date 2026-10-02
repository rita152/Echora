//! Left pane: tabs, search, the filter menu trigger, sections, and rows.
//!
//! Geometry and colors are the reference inbox's computed styles (ChatGPT
//! 26.924, `pull-request-route`): a toolbar row, then one scroll container
//! with a stable 11px scrollbar gutter whose first child is the sticky search
//! row and its 32px surface fade.

use gpui::{AnimationExt, Div, SharedString, div, prelude::*, px};

use super::{FilterSubmenu, PullRequestsView, theme::*};
use crate::components::icons::icon;
use crate::pull_requests::{
    GroupKind, ListTab, PullRequestSummary, StatusFilter, StatusIcon, format_count,
};

impl PullRequestsView {
    pub(super) fn list_pane(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        div()
            .w(px(self.list_width))
            .min_w(px(self.list_width))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .child(self.list_toolbar(cx))
            .child(self.list_scroll_area(cx))
    }

    /// The app-shell header row: the toolbar sits 7px inside the pane with
    /// `px-2`, so the `All`/`Reviewing`/`Authored` switch starts 15px in.
    /// With the sidebar closed the header starts after the titlebar's
    /// leading area instead, `px-2` past it (tabs at x=144).
    fn list_toolbar(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let mut tabs = div()
            .id("pr-view-tabs")
            .role(gpui::Role::Group)
            .aria_label("Pull request view")
            .flex()
            .items_center()
            .gap(px(2.0));
        for tab in ListTab::ALL {
            let selected = self.tab == tab;
            let view = cx.entity();
            tabs = tabs.child(
                Self::toolbar_button(
                    SharedString::from(format!("pr-tab-{}", tab.label())),
                    theme,
                    selected,
                )
                .aria_label(SharedString::from(tab.label()))
                .debug_selector(move || format!("pr-tab-{}", tab.label()))
                .on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| view.select_tab(tab, cx));
                })
                .child(tab.label()),
            );
        }
        div()
            .flex_none()
            .h(px(TOOLBAR_HEIGHT))
            .ml(px(LIST_TOOLBAR_INSET_LEFT.max(self.titlebar_inset + 8.0)))
            .mr(px(LIST_TOOLBAR_INSET_RIGHT))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .child(tabs)
            .when(self.list_loading && !self.groups.is_empty(), |row| {
                // `Refreshing pull requests`: a 24px status slot with the
                // tertiary spinner while a loaded list refetches.
                row.child(
                    div()
                        .id("pr-list-refreshing")
                        .role(gpui::Role::Status)
                        .aria_label("Refreshing pull requests")
                        .w(px(24.0))
                        .h(px(28.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Self::spinner(theme.text_muted, 14.0)),
                )
            })
    }

    /// One scroll container for the sticky search row and the sections, with
    /// the reference's `[scrollbar-gutter:stable]` on the right (11px with
    /// classic scrollers, none with overlay ones).
    fn list_scroll_area(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        div()
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(
                div()
                    .id("pr-list-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.list_scroll)
                    .child(
                        div()
                            .w(px((self.list_width - self.scrollbar_gutter()).max(0.0)))
                            .min_h_full()
                            .flex()
                            .flex_col()
                            // Room for the sticky search row drawn above.
                            .child(div().flex_none().h(px(LIST_SEARCH_ROW_HEIGHT)))
                            .child(self.list_body(cx)),
                    ),
            )
            .child(
                // The sticky row lives outside the scrolled content so it stays
                // put; its `after:h-8` fade overlays the rows below it.
                div()
                    .absolute()
                    .top(px(LIST_SEARCH_ROW_HEIGHT))
                    .left_0()
                    .w(px((self.list_width - self.scrollbar_gutter()).max(0.0)))
                    .h(px(32.0))
                    .bg(gpui::linear_gradient(
                        180.0,
                        gpui::linear_color_stop(theme.surface, 0.0),
                        gpui::linear_color_stop(theme.surface.alpha(0.0), 1.0),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px((self.list_width - self.scrollbar_gutter()).max(0.0)))
                    .bg(theme.surface)
                    .child(self.list_search_row(cx)),
            )
            .children(self.scrollbar_thumb(&self.list_scroll, cx))
    }

    /// `pt-panel px-panel pb-2` search row: the pill field and the filter
    /// trigger.
    fn list_search_row(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let has_query = !self.query.is_empty();
        let clear = cx.entity();
        div()
            .flex_none()
            .h(px(LIST_SEARCH_ROW_HEIGHT))
            .px(px(PANE_PADDING))
            .pt(px(PANE_PADDING))
            .pb(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(32.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(9999.0))
                    .bg(theme.field_surface)
                    .border(px(1.0))
                    .border_color(theme.field_border)
                    .child(
                        div()
                            .flex_none()
                            .size(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("pr-search", theme.icon_muted.into()).size(px(18.0))),
                    )
                    .child(div().flex_1().min_w(px(0.0)).child(self.search.clone()))
                    .when(has_query, |field| {
                        // A bare 16px circled × in `text-secondary`.
                        field.child(
                            div()
                                .id("pr-clear-search")
                                .flex_none()
                                .size(px(16.0))
                                .role(gpui::Role::Button)
                                .aria_label("Clear search")
                                .on_click(move |_, _, cx| {
                                    clear.update(cx, |view, cx| view.clear_search(cx));
                                })
                                .child(
                                    icon("pr-clear-search", theme.icon_muted.into()).size(px(16.0)),
                                ),
                        )
                    }),
            )
            .child(self.filter_button(cx))
    }

    /// `Filter pull requests`: the funnel, or the badged lines while a status
    /// other than `Open` or a repository is chosen.
    fn filter_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let active = self.filter.is_active();
        let open = self.list_menu.is_some();
        Self::toolbar_button("pr-filter", theme, true)
            .w(px(28.0))
            .justify_center()
            .px(px(0.0))
            .relative()
            .child(self.control_anchor("pr-filter"))
            .when(open, |button| button.bg(theme.control_hover))
            .aria_label("Filter pull requests")
            .on_click(move |_, _, cx| {
                view.update(cx, |view, cx| view.toggle_filter_menu(cx));
            })
            .child(if active {
                div()
                    .relative()
                    .size(px(18.0))
                    .child(icon("pr-filter-active", theme.text.into()).size(px(18.0)))
                    .child(
                        icon("pr-filter-badge", theme.filter_badge.into())
                            .absolute()
                            .top_0()
                            .left_0()
                            .size(px(18.0)),
                    )
            } else {
                div()
                    .size(px(18.0))
                    .child(icon("pr-filter", theme.text.into()).size(px(18.0)))
            })
    }

    fn list_body(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let body = div()
            .flex_1()
            .flex()
            .flex_col()
            .px(px(PANE_PADDING))
            .pt(px(PANE_PADDING))
            .pb(px(PANE_PADDING));
        if let Some(error) = self.list_error.clone()
            && self.groups.is_empty()
        {
            let view = cx.entity();
            return body.child(
                Self::empty_state(theme, "Unable to load pull requests", false)
                    .child(
                        div()
                            .max_w(px(576.0))
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(theme.text_muted)
                            .text_center()
                            .child(error),
                    )
                    .child(
                        Self::toolbar_button("pr-retry-list", theme, true)
                            .aria_label("Try again")
                            .on_click(move |_, _, cx| {
                                view.update(cx, |view, cx| view.reload(cx));
                            })
                            .child("Try again"),
                    ),
            );
        }
        if self.list_loading && self.groups.is_empty() {
            // Every section of the tab shows its header over a compact
            // skeleton while its search runs.
            let mut column = div().flex().flex_col().gap(px(16.0));
            for (index, kind) in GroupKind::loading_sections(self.tab, &self.query)
                .iter()
                .copied()
                .enumerate()
            {
                column = column.child(self.group_section(kind, &[], index == 0, cx));
            }
            return body.child(column);
        }
        let groups = crate::pull_requests::filter_groups(&self.groups, &self.filter, "");
        if groups.is_empty() {
            let message = if !self.query.trim().is_empty() {
                "No pull requests match this search"
            } else if self.tab == ListTab::Reviewing {
                "You’re all caught up"
            } else {
                "No pull requests found"
            };
            return body.child(Self::empty_state(theme, message, true));
        }
        let mut column = div().flex().flex_col().gap(px(16.0));
        for group in groups {
            column = column.child(self.group_section(group.kind, &group.items, false, cx));
        }
        body.child(column)
    }

    /// `EmptyState` (`px-3 py-6 min-h-64 flex-1`): a 16/24 medium title
    /// centered in the body, at `opacity-60` for the faded tone.
    fn empty_state(theme: PrTheme, title: &'static str, faded: bool) -> Div {
        div()
            .flex_1()
            .min_h(px(256.0))
            .px(px(12.0))
            .py(px(24.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .max_w(px(576.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.0))
                    .text_center()
                    .when(faded, |content| content.opacity(0.6))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .line_height(px(24.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(title),
                    ),
            )
    }

    /// The compact list skeleton (`max-h-28` over five pulsing rows): each
    /// row carries a 16px title bar and a 12px detail bar at `bg-text/10`.
    fn skeleton_rows(theme: PrTheme, announce: bool) -> impl IntoElement {
        const BARS: [(f32, f32); 5] = [
            (2.0 / 3.0, 1.0 / 2.0),
            (1.0 / 2.0, 2.0 / 5.0),
            (3.0 / 4.0, 3.0 / 5.0),
            (5.0 / 12.0, 1.0 / 3.0),
            (7.0 / 12.0, 1.0 / 2.0),
        ];
        let bar = |height: f32, width: f32| {
            div()
                .h(px(height))
                .w(gpui::relative(width))
                .rounded(px(4.0))
                .bg(theme.skeleton)
        };
        let mut rows = div().flex().flex_col().gap(px(2.0));
        for (title, detail) in BARS {
            rows = rows.child(
                div()
                    .min_h(px(40.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(ROW_RADIUS))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .child(bar(16.0, title)),
                            )
                            .child(
                                div()
                                    .h(px(16.0))
                                    .flex()
                                    .items_center()
                                    .child(bar(12.0, detail)),
                            ),
                    ),
            );
        }
        div()
            .id(if announce {
                "pr-list-loading"
            } else {
                "pr-list-loading-section"
            })
            .when(announce, |skeleton| {
                skeleton
                    .role(gpui::Role::Status)
                    .aria_label("Loading pull requests")
            })
            .max_h(px(112.0))
            .overflow_hidden()
            .child(rows.with_animation(
                "pr-skeleton-pulse",
                gpui::Animation::new(std::time::Duration::from_secs(2)).repeat(),
                |rows, delta| {
                    // `animate-pulse`: opacity 1 → 0.5 → 1.
                    let phase = (delta * std::f32::consts::TAU).cos();
                    rows.opacity(0.75 + 0.25 * phase)
                },
            ))
    }

    /// A section: its collapse header, then its rows, or the skeleton while
    /// `items` is empty because the section is still loading.
    fn group_section(
        &self,
        kind: GroupKind,
        items: &[PullRequestSummary],
        announce_loading: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        let collapsed = self.collapsed_groups.contains(&kind);
        let view = cx.entity();
        let loading = items.is_empty();
        let mut rows = div().flex().flex_col().gap(px(2.0));
        for item in items {
            rows = rows.child(self.row(kind, item, cx));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                // `h2.px-3.pb-1` holding the collapse button with the label and
                // a 14px chevron that turns down while the section is open.
                div().h(px(20.0)).px(px(12.0)).pb(px(4.0)).flex().child(
                    div()
                        .id(SharedString::from(format!("pr-group-{}", kind.label())))
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .rounded(px(7.5))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text_muted)
                        .role(gpui::Role::Button)
                        .aria_label(SharedString::from(kind.label()))
                        .aria_expanded(!collapsed)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |view, cx| view.toggle_group(kind, cx));
                        })
                        .child(kind.label())
                        .child(
                            icon("pr-chevron-right", theme.text_muted.into())
                                .size(px(14.0))
                                .when(!collapsed, |chevron| {
                                    chevron.with_transformation(gpui::Transformation::rotate(
                                        gpui::radians(std::f32::consts::FRAC_PI_2),
                                    ))
                                }),
                        ),
                ),
            )
            .when(!collapsed && !loading, |section| section.child(rows))
            .when(!collapsed && loading, |section| {
                section.child(Self::skeleton_rows(theme, announce_loading))
            })
    }

    fn row(
        &self,
        kind: GroupKind,
        item: &PullRequestSummary,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let group = kind.label();
        let theme = self.theme();
        let selected = self.selected.as_ref().is_some_and(|current| {
            current.number == item.number && current.repository == item.repository
        });
        let view = cx.entity();
        let summary = item.clone();
        let show_repository = self.filter.repository.is_none();
        div()
            // A pull request can sit in both groups (authored and previously
            // reviewed); the id must include the group or the accessibility
            // tree aborts on a duplicate node id.
            .id(SharedString::from(format!(
                "pr-row-{group}-{}-{}",
                item.repository, item.number
            )))
            .w_full()
            .min_h(px(40.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(ROW_RADIUS))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(14.0))
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(item.title.clone()))
            .aria_selected(selected)
            .when(selected, |row| row.bg(theme.row_selected))
            .when(!selected, |row| {
                row.hover(move |style| style.bg(theme.row_hover))
            })
            .on_click(move |_, _, cx| {
                view.update(cx, |view, cx| view.select(summary.clone(), cx));
            })
            .child(
                div()
                    .flex_none()
                    .min_w(px(16.0))
                    .min_h(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::status_glyph(item.status_icon(), theme, 20.0)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .h(px(24.0))
                                    .line_height(px(24.0))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_color(theme.text)
                                    .child(item.title.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .min_h(px(20.0))
                                    .flex()
                                    .items_center()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.text_muted)
                                    .child(item.age.clone()),
                            ),
                    )
                    .child(
                        div()
                            .h(px(16.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(theme.text_muted)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .when(kind.shows_author_avatar(), |line| {
                                        line.child(
                                            self.avatar(item.author_avatar_url.as_deref(), 16.0),
                                        )
                                    })
                                    .when(show_repository, |line| {
                                        line.child(
                                            div()
                                                .flex_none()
                                                .max_w(gpui::relative(0.35))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .child(item.repository.clone()),
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(item.head_branch.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .min_h(px(16.0))
                                    .flex()
                                    .items_center()
                                    .child(Self::diff_stats(
                                        item.additions,
                                        item.deletions,
                                        theme.text_muted,
                                        theme.text_muted,
                                        12.0,
                                    )),
                            ),
                    ),
            )
    }

    /// The pull request state glyph at `size`, tinted like the reference's
    /// `text-purple` / `text-chart-red` / `text-codex-description`, with the
    /// merge-readiness dot drawn over open pull requests.
    pub(crate) fn status_glyph(status: StatusIcon, theme: PrTheme, size: f32) -> Div {
        let (name, color, dot) = match status {
            StatusIcon::Merged => ("pr-status-merged", theme.purple, None),
            StatusIcon::Closed => ("pr-status-closed", theme.chart_red, None),
            StatusIcon::Draft => ("pr-status-draft", theme.text_muted, None),
            StatusIcon::Failing => ("pr-status-checks", theme.text_muted, Some(theme.chart_red)),
            StatusIcon::InProgress => (
                "pr-status-checks",
                theme.text_muted,
                Some(theme.chart_yellow),
            ),
            StatusIcon::Ready | StatusIcon::Successful => (
                "pr-status-checks",
                theme.text_muted,
                Some(theme.chart_green),
            ),
        };
        div()
            .relative()
            .flex_none()
            .size(px(size))
            .child(icon(name, color.into()).size(px(size)))
            .when_some(dot, |glyph, dot| {
                glyph.child(
                    icon("pr-status-dot", dot.into())
                        .absolute()
                        .top_0()
                        .left_0()
                        .size(px(size)),
                )
            })
    }

    /// `+x -y` in the reference's tabular, tightly tracked digits.
    pub(crate) fn diff_stats(
        additions: u64,
        deletions: u64,
        added: gpui::Rgba,
        deleted: gpui::Rgba,
        size: f32,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(size))
            .line_height(px(size))
            .font_features(stats_font_features())
            .child(
                div()
                    .flex_none()
                    .text_color(added)
                    .child(format!("+{}", format_count(additions))),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(deleted)
                    .child(format!("-{}", format_count(deletions))),
            )
    }

    /// The funnel popover with its two submenu rows.
    pub(super) fn filter_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        let status_disabled = self.tab == ListTab::Reviewing;
        let mut menu = Self::menu_surface("pr-filter-menu", theme)
            .w(px(MENU_NARROW_WIDTH))
            // Clicks inside the popover must not reach the page's dismiss
            // handler, or the menu closes before a submenu item is chosen.
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (submenu, label, glyph) in [
            (FilterSubmenu::Status, "Status", "pr-menu-status"),
            (
                FilterSubmenu::Repository,
                "Repository",
                "pr-menu-repository",
            ),
        ] {
            let hovered = self.filter_submenu == Some(submenu);
            let disabled = submenu == FilterSubmenu::Status && status_disabled;
            let hover_view = view.clone();
            let click_view = view.clone();
            menu = menu.child(
                Self::menu_row(
                    SharedString::from(format!("pr-filter-{label}")),
                    theme,
                    hovered && !disabled,
                    disabled,
                )
                .relative()
                .child(self.control_anchor(format!("pr-filter-{label}")))
                .when(!disabled, |row| {
                    row.on_click(move |_, _, cx| {
                        click_view
                            .update(cx, |view, cx| view.hover_filter_submenu(Some(submenu), cx));
                    })
                    .on_hover(move |hovered, _, cx| {
                        hover_view.update(cx, |view, cx| {
                            if *hovered {
                                view.hover_filter_submenu(Some(submenu), cx);
                            }
                        });
                    })
                })
                .aria_label(SharedString::from(label))
                .aria_expanded(hovered)
                .child(Self::menu_icon(glyph, theme))
                .child(div().flex_1().min_w(px(0.0)).child(label))
                .child(
                    icon("pr-chevron-right", theme.menu_icon.into()).size(px(MENU_CHEVRON_SIZE)),
                ),
            );
        }
        menu
    }

    pub(super) fn filter_submenu(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Stateful<Div>> {
        let submenu = self.filter_submenu?;
        let theme = self.theme();
        let view = cx.entity();
        // `min-w-[180px]`, and the repository list caps at `max-w-80`.
        let mut menu = Self::menu_surface("pr-filter-submenu", theme)
            .min_w(px(MENU_SUBMENU_WIDTH))
            .max_w(px(320.0))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let entries: Vec<(String, bool)> = match submenu {
            FilterSubmenu::Status => StatusFilter::ALL
                .into_iter()
                .map(|status| (status.label().to_string(), self.filter.status == status))
                .collect(),
            FilterSubmenu::Repository => {
                let mut entries = vec![(
                    "All repositories".to_string(),
                    self.filter.repository.is_none(),
                )];
                entries.extend(self.repositories.iter().map(|repository| {
                    (
                        repository.clone(),
                        self.filter.repository.as_deref() == Some(repository.as_str()),
                    )
                }));
                entries
            }
        };
        for (index, (label, checked)) in entries.into_iter().enumerate() {
            let select_view = view.clone();
            let repository = match submenu {
                FilterSubmenu::Repository if index > 0 => Some(label.clone()),
                _ => None,
            };
            let status = match submenu {
                FilterSubmenu::Status => StatusFilter::ALL
                    .into_iter()
                    .find(|status| status.label() == label),
                FilterSubmenu::Repository => None,
            };
            menu = menu.child(
                Self::menu_row(
                    SharedString::from(format!("pr-filter-option-{label}")),
                    theme,
                    false,
                    false,
                )
                .aria_label(SharedString::from(label.clone()))
                .on_click(move |_, _, cx| {
                    select_view.update(cx, |view, cx| match status {
                        Some(status) => view.apply_status_filter(status, cx),
                        None => view.apply_repository_filter(repository.clone(), cx),
                    });
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
                .when(checked, |row| {
                    row.child(icon("check", theme.text.into()).size(px(MENU_ICON_SIZE)))
                }),
            );
        }
        Some(menu)
    }
}

/// `disambiguated-digits tabular-nums tracking-tight`: SF's alternate
/// digits, tabular figures, and -0.025em letter spacing.
pub(crate) fn stats_font_features() -> gpui::FontFeatures {
    gpui::FontFeatures(std::sync::Arc::new(vec![
        ("tnum".into(), 1),
        ("cv01".into(), 1),
        ("cv02".into(), 1),
    ]))
    .with_letter_spacing(-0.025)
}
