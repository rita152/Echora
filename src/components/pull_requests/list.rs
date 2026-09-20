//! Left pane: tabs, search, the filter menu trigger, groups, and rows.

use gpui::{Div, SharedString, div, prelude::*, px};

use super::{FilterSubmenu, PullRequestsView, theme::*};
use crate::components::icons::icon;
use crate::pull_requests::{GroupKind, ListTab, PullRequestSummary, StatusFilter};

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
            .child(self.list_search_row(cx))
            .child(self.list_body(cx))
    }

    /// `h-toolbar px-2 gap-2` with the `All`/`Reviewing`/`Authored` pills.
    fn list_toolbar(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let mut tabs = div().flex().items_center().gap(px(2.0));
        for tab in ListTab::ALL {
            let selected = self.tab == tab;
            let view = cx.entity();
            tabs = tabs.child(
                div()
                    .id(SharedString::from(format!("pr-tab-{}", tab.label())))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .h(px(28.0))
                    .px(px(8.0))
                    .rounded(px(12.5))
                    .text_size(px(13.0))
                    .cursor_pointer()
                    .when(selected, |tab| tab.bg(theme.control).text_color(theme.text))
                    .when(!selected, |tab| tab.text_color(theme.text_muted))
                    .when(!selected, |tab| {
                        tab.hover(move |style| style.bg(theme.control_hover).text_color(theme.text))
                    })
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(tab.label()))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.select_tab(tab, cx));
                    })
                    .child(tab.label()),
            );
        }
        div()
            .flex_none()
            .h(px(TOOLBAR_HEIGHT))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            // The reference keeps a 1px `sr-only` heading ahead of the tab
            // group, which offsets the first tab by one pixel plus the gap.
            .child(div().w(px(1.0)).h(px(1.0)))
            .child(tabs)
            .child(div().flex_1())
    }

    /// `px-5 pb-2 pt-5` search row with the pill field and the filter trigger.
    fn list_search_row(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let has_query = !self.query.is_empty();
        let view = cx.entity();
        let clear = view.clone();
        div()
            .flex_none()
            .pl(px(PANE_PADDING))
            .pr(px(PANE_PADDING + SCROLLBAR_GUTTER))
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
                    .child(icon("search", theme.icon_muted.into()).size(px(18.0)))
                    .child(div().flex_1().min_w(px(0.0)).child(self.search.clone()))
                    .when(has_query, |field| {
                        field.child(
                            div()
                                .id("pr-clear-search")
                                .size(px(20.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(9999.0))
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.control))
                                .role(gpui::Role::Button)
                                .aria_label("Clear search")
                                .on_click(move |_, _, cx| {
                                    clear.update(cx, |view, cx| view.clear_search(cx));
                                })
                                .child(
                                    icon("close-dialog", theme.icon_muted.into()).size(px(14.0)),
                                ),
                        )
                    }),
            )
            .child(self.filter_button(cx))
    }

    fn filter_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let active = self.filter.is_active() || self.filter_loading;
        let open = self.list_menu.is_some();
        div()
            .id("pr-filter")
            .relative()
            .child(self.control_anchor("pr-filter"))
            .flex_none()
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.5))
            .cursor_pointer()
            .bg(if active || open {
                theme.control_hover
            } else {
                theme.control
            })
            .hover(move |style| style.bg(theme.control_hover))
            .role(gpui::Role::Button)
            .aria_label("Filter pull requests")
            .on_click(move |_, _, cx| {
                view.update(cx, |view, cx| view.toggle_filter_menu(cx));
            })
            .child(icon("settings-filter", theme.text.into()).size(px(18.0)))
    }

    fn list_body(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        if self.list_error.is_some() || (self.list_loading && self.groups.is_empty()) {
            let message = if self.list_error.is_some() {
                "Could not load pull requests"
            } else {
                "Loading pull requests"
            };
            return div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(theme.text_muted)
                .flex_col()
                .gap(px(12.0))
                .px(px(16.0))
                .child(message)
                .children(
                    self.list_error
                        .clone()
                        .map(|error| div().text_size(px(12.0)).child(error)),
                )
                .when(self.list_error.is_some(), |body| {
                    body.child(
                        div()
                            .id("pr-retry-list")
                            .role(gpui::Role::Button)
                            .aria_label("Retry loading pull requests")
                            .cursor_pointer()
                            .child("Retry")
                            .on_click({
                                let view = cx.entity();
                                move |_, _, cx| {
                                    view.update(cx, |view, cx| view.reload(cx));
                                }
                            }),
                    )
                });
        }
        let groups = crate::pull_requests::filter_groups(&self.groups, &self.filter, &self.query);
        if groups.is_empty() {
            let message = if !self.query.is_empty() {
                "No pull requests match this search"
            } else if self.tab == ListTab::Reviewing {
                "You're all caught up"
            } else {
                "No pull requests"
            };
            return div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(theme.text_muted)
                .child(message);
        }
        let mut column = div()
            .flex_1()
            .min_h(px(0.0))
            .pl(px(PANE_PADDING))
            .pr(px(PANE_PADDING + SCROLLBAR_GUTTER))
            .py(px(PANE_PADDING))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .overflow_hidden();
        for group in groups {
            column = column.child(self.group_section(group.kind, &group.items, cx));
        }
        column
    }

    fn group_section(
        &self,
        kind: GroupKind,
        items: &[PullRequestSummary],
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        let collapsed = self.collapsed_groups.contains(&kind);
        let view = cx.entity();
        let mut rows = div().flex().flex_col().gap(px(2.0));
        for item in items {
            rows = rows.child(self.row(kind, item, cx));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .id(SharedString::from(format!("pr-group-{}", kind.label())))
                    .h(px(20.0))
                    .px(px(12.0))
                    .pb(px(4.0))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(kind.label()))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.toggle_group(kind, cx));
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(kind.label()),
                            )
                            .child(
                                icon("section-chevron", theme.text_muted.into())
                                    .size(px(14.0))
                                    .when(collapsed, |chevron| {
                                        chevron.with_transformation(gpui::Transformation::rotate(
                                            gpui::radians(-std::f32::consts::FRAC_PI_2),
                                        ))
                                    }),
                            ),
                    ),
            )
            .when(!collapsed, |section| section.child(rows))
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
        div()
            // A pull request can sit in both groups (authored and previously
            // reviewed); the id must include the group or the accessibility
            // tree aborts on a duplicate node id.
            .id(SharedString::from(format!(
                "pr-row-{group}-{}-{}",
                item.repository, item.number
            )))
            .h(px(ROW_HEIGHT))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(ROW_RADIUS))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(item.title.clone()))
            .when(selected, |row| row.bg(theme.row_hover))
            .when(!selected, |row| {
                row.hover(move |style| style.bg(theme.row_hover))
            })
            .on_click(move |_, _, cx| {
                view.update(cx, |view, cx| view.select(summary.clone(), cx));
            })
            .child(
                div()
                    .flex_none()
                    .size(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon("pull-request", theme.text_muted.into()).size(px(20.0))),
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
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .text_size(px(14.0))
                                            .text_color(theme.text)
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .child(item.title.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .h(px(20.0))
                                    .text_size(px(12.0))
                                    .text_color(theme.text_muted)
                                    .child(item.age.clone()),
                            ),
                    )
                    .child(
                        div()
                            .h(px(16.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(12.0))
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
                                    .child(div().flex_none().child(item.repository.clone()))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .child(item.head_branch.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .text_color(theme.text_muted)
                                    .child(format!("+{}", item.additions))
                                    .child(format!("-{}", item.deletions)),
                            ),
                    ),
            )
    }

    /// The funnel popover with its two submenu rows.
    pub(super) fn filter_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        let mut menu = div()
            .id("pr-filter-menu")
            .w(px(208.0))
            .p(px(4.0))
            .rounded(px(20.0))
            .bg(theme.menu_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .text_size(px(13.0))
            .text_color(theme.text)
            // Clicks inside the popover must not reach the page's dismiss
            // handler, or the menu closes before a submenu item is chosen.
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (index, (submenu, label)) in [
            (FilterSubmenu::Status, "Status"),
            (FilterSubmenu::Repository, "Repository"),
        ]
        .into_iter()
        .enumerate()
        {
            let hovered = self.filter_submenu == Some(submenu);
            let hover_view = view.clone();
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("pr-filter-{label}")))
                    .relative()
                    .child(self.control_anchor(format!("pr-filter-{label}")))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |view, cx| {
                                view.hover_filter_submenu(Some(submenu), cx)
                            });
                        }
                    })
                    .h(px(28.5))
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(15.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .when(hovered, |row| row.bg(theme.menu_hover))
                    .hover(move |style| style.bg(theme.menu_hover))
                    .on_hover(move |hovered, _, cx| {
                        hover_view.update(cx, |view, cx| {
                            if *hovered {
                                view.hover_filter_submenu(Some(submenu), cx);
                            }
                        });
                    })
                    .role(gpui::Role::MenuItem)
                    .aria_label(SharedString::from(label))
                    .child(div().flex_1().child(label))
                    .child(icon("settings-chevron-next", theme.text_muted.into()).size(px(14.0)))
                    .when(index == 0, |row| row),
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
        let mut menu = div()
            .id("pr-filter-submenu")
            .p(px(4.0))
            .w(px(180.0))
            .rounded(px(20.0))
            .bg(theme.menu_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .text_size(px(13.0))
            .text_color(theme.text)
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
        let all_repositories = self.filter.repository_label().to_string();
        for (label, checked) in entries {
            let select_view = view.clone();
            let all_repositories = all_repositories.clone();
            let repository = match submenu {
                FilterSubmenu::Status => None,
                FilterSubmenu::Repository => Some(label.clone()),
            };
            let status = match submenu {
                FilterSubmenu::Status => StatusFilter::ALL
                    .into_iter()
                    .find(|status| status.label() == label),
                FilterSubmenu::Repository => None,
            };
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("pr-filter-option-{label}")))
                    .h(px(28.5))
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(15.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.menu_hover))
                    .role(gpui::Role::MenuItem)
                    .aria_label(SharedString::from(label.clone()))
                    .on_click(move |_, _, cx| {
                        select_view.update(cx, |view, cx| match (status, &repository) {
                            (Some(status), _) => view.apply_status_filter(status, cx),
                            (None, Some(repository)) if repository == &all_repositories => {
                                view.apply_repository_filter(None, cx)
                            }
                            (None, Some(repository)) => {
                                view.apply_repository_filter(Some(repository.clone()), cx)
                            }
                            _ => {}
                        });
                    })
                    .child(div().flex_1().child(label))
                    .when(checked, |row| {
                        row.child(icon("check", theme.text.into()).size(px(14.0)))
                    }),
            );
        }
        if submenu == FilterSubmenu::Status {
            // `All states` carries the current-status affordance in the
            // reference; the check mark above already marks the active row.
        }
        Some(menu)
    }
}
