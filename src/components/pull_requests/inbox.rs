//! Code review inbox geometry and interactions, measured in the local Synara app.
use super::{
    CodeReviewPinsChanged, FilterSubmenu, InboxKind, ListMenu, PullRequestsView,
    render::TooltipPlacement, theme::MENU_CHEVRON_SIZE,
};
use crate::{
    components::icons::icon,
    pull_requests::{GroupKind, ListTab, PullRequestSummary, StatusFilter},
};
use gpui::{Div, SharedString, div, prelude::*, px};

impl PullRequestsView {
    pub(super) fn inbox_theme(&self) -> super::PrTheme {
        let mut theme = self.theme();
        if self.mode == crate::theme::ThemeMode::Light {
            theme.text = gpui::rgba(0x0d0d0dff);
            theme.text_muted = gpui::rgba(0x0d0d0d98);
            theme.control = gpui::rgba(0x0d0d0d08);
            theme.control_hover = gpui::rgba(0x0d0d0d08);
            theme.row_selected = gpui::rgba(0x0d0d0d12);
            theme.row_hover = gpui::rgba(0x0d0d0d08);
            theme.filter_badge = gpui::rgba(0xc74614ff);
            theme.field_surface = gpui::transparent_black().into();
        } else {
            theme.row_selected = gpui::rgba(0xffffff12);
        }
        theme.purple = gpui::rgba(0x6366f1ff);
        theme
    }
    pub fn set_pins(
        &mut self,
        pins: std::collections::BTreeSet<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.pinned_items = pins;
        cx.notify();
    }
    fn pinned(&self, row: &PullRequestSummary) -> bool {
        self.pinned_items.contains(&row.url)
    }
    pub(super) fn toggle_pin(&mut self, row: &PullRequestSummary, cx: &mut gpui::Context<Self>) {
        if !self.pinned_items.remove(&row.url) {
            self.pinned_items.insert(row.url.clone());
        }
        cx.emit(CodeReviewPinsChanged {
            pins: self.pinned_items.clone(),
        });
        cx.notify();
    }
    fn reset_inbox(&mut self, cx: &mut gpui::Context<Self>) {
        self.filter = Default::default();
        self.tab = ListTab::All;
        self.kind = InboxKind::All;
        self.involvement.clear();
        self.label_filters.clear();
        self.query.clear();
        self.visible_rows = 10;
        self.search
            .update(cx, |input, cx| input.set_text_silently("", cx));
        self.loaded_key = None;
        self.dismiss_menus(cx);
        self.reload(cx);
    }
    fn toggle_inbox_menu(&mut self, menu: ListMenu, cx: &mut gpui::Context<Self>) {
        self.list_menu = if self.list_menu.as_ref() == Some(&menu) {
            None
        } else {
            Some(menu)
        };
        self.filter_submenu = None;
        cx.notify();
    }
    pub(super) fn inbox_button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
    ) -> gpui::Stateful<Div> {
        let theme = self.inbox_theme();
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .relative()
            .flex_none()
            .h(px(28.0))
            .px(px(10.0))
            .rounded(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .text_size(px(crate::typography::ui_size(12.0)))
            .line_height(px(18.0))
            .text_color(theme.text_muted)
            .cursor_pointer()
            .whitespace_nowrap()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .hover(move |s| s.bg(theme.control_hover).text_color(theme.text))
    }
    pub(super) fn list_pane(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.inbox_theme();
        let sort = cx.entity();
        let filter = cx.entity();
        let more = cx.entity();
        let sort_label = if self.newest {
            "Sort: Newest"
        } else {
            "Sort: Recently updated"
        };
        let mut tabs = div()
            .id("pr-kind-group")
            .role(gpui::Role::Group)
            .aria_label("Kind")
            .flex()
            .items_center()
            .gap(px(2.0));
        for (kind, label) in [
            (InboxKind::All, "All"),
            (InboxKind::PullRequests, "PRs"),
            (InboxKind::Issues, "Issues"),
        ] {
            let count = self
                .groups
                .iter()
                .flat_map(|g| &g.items)
                .filter(|r| kind.matches(r))
                .count();
            let selected = self.kind == kind;
            let view = cx.entity();
            tabs = tabs.child(
                self.inbox_button(
                    SharedString::from(format!("pr-kind-{label}")),
                    SharedString::from(format!("{label}, {count}")),
                )
                .debug_selector(move || format!("pr-kind-{label}"))
                .role(gpui::Role::RadioButton)
                .aria_toggled(if selected {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .h(px(20.0))
                .px(px(6.0))
                .gap(px(4.0))
                .rounded(px(8.0))
                .when(selected, |s| s.bg(theme.control).text_color(theme.text))
                .on_click(move |_, window, cx| {
                    view.update(cx, |v, cx| {
                        v.kind = kind;
                        v.visible_rows = 10;
                        v.list_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
                        v.focus.focus(window, cx);
                        cx.notify();
                    })
                })
                .child(label)
                .child(div().text_color(theme.text_muted).child(count.to_string())),
            );
        }
        let mut header = div()
            .flex_none()
            .pt(px(44.0))
            .child(
                div()
                    .h(px(50.0))
                    .pt(px(10.0))
                    .pl(px(24.0))
                    .pr(px(18.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(crate::typography::ui_size(16.0)))
                            .line_height(px(24.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("Code review"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                self.inbox_button("pr-sort", sort_label)
                                    .size(px(24.0))
                                    .px(px(0.0))
                                    .child(self.control_anchor("pr-sort"))
                                    .child(
                                        icon("review-sort", theme.text_muted.into()).size(px(14.0)),
                                    )
                                    .on_click(move |_, _, cx| {
                                        sort.update(cx, |v, cx| {
                                            v.toggle_inbox_menu(ListMenu::Sort, cx)
                                        })
                                    }),
                            )
                            .child(
                                self.inbox_button("pr-filter", "Filter")
                                    .size(px(24.0))
                                    .px(px(0.0))
                                    .child(self.control_anchor("pr-filter"))
                                    .child(
                                        icon("review-filter", theme.text_muted.into())
                                            .size(px(14.0)),
                                    )
                                    .when(
                                        self.filter.is_active()
                                            || self.tab != ListTab::All
                                            || !self.involvement.is_empty()
                                            || !self.label_filters.is_empty(),
                                        |s| {
                                            s.child(
                                                div()
                                                    .absolute()
                                                    .top(px(1.0))
                                                    .right(px(1.0))
                                                    .size(px(6.0))
                                                    .rounded_full()
                                                    .bg(theme.filter_badge),
                                            )
                                        },
                                    )
                                    .on_click(move |_, _, cx| {
                                        filter.update(cx, |v, cx| v.toggle_filter_menu(cx))
                                    }),
                            )
                            .child(
                                self.inbox_button("pr-more", "More code review actions")
                                    .size(px(24.0))
                                    .px(px(0.0))
                                    .child(self.control_anchor("pr-more"))
                                    .child(
                                        icon("review-more", theme.text_muted.into()).size(px(14.0)),
                                    )
                                    .on_click(move |_, _, cx| {
                                        more.update(cx, |v, cx| {
                                            v.toggle_inbox_menu(ListMenu::More, cx)
                                        })
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .px(px(24.0))
                    .pt(px(4.0))
                    .pb(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(self.inbox_search(cx))
                    .child(tabs),
            );
        if self.filter.is_active()
            || self.tab != ListTab::All
            || !self.involvement.is_empty()
            || !self.label_filters.is_empty()
        {
            let clear = cx.entity();
            let mut chips = div().flex().flex_wrap().gap(px(6.0)).items_center();
            let mut labels = Vec::new();
            if self.filter.status != StatusFilter::Open {
                labels.push(self.filter.status.label().to_string());
            }
            if let Some(repo) = &self.filter.repository {
                labels.push(repo.clone());
            }
            if self.tab != ListTab::All {
                labels.push(self.tab.label().to_string());
            }
            if !self.involvement.is_empty() {
                labels.push(
                    if self.involvement == "assignee:@me" {
                        "Assigned to me"
                    } else {
                        "Involving me"
                    }
                    .into(),
                );
            }
            labels.extend(self.label_filters.clone());
            for (index, label) in labels.into_iter().enumerate() {
                let remove = cx.entity();
                let choice = label.clone();
                chips = chips.child(
                    div()
                        .rounded_full()
                        .pl(px(7.0))
                        .pr(px(2.0))
                        .py(px(1.0))
                        .bg(theme.control)
                        .text_color(theme.text_muted)
                        .when(label == self.filter.status.label(), |s| {
                            s.bg(gpui::rgba(0xc746140d)).text_color(theme.filter_badge)
                        })
                        .text_size(px(crate::typography::ui_size(11.0)))
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        .child(label.clone())
                        .child(
                            self.inbox_button(
                                gpui::ElementId::Name(format!("pr-remove-filter-{index}").into()),
                                SharedString::from(format!("Remove filter: {label}")),
                            )
                            .size(px(16.0))
                            .px(px(0.0))
                            .child(icon("pr-close", theme.text_muted.into()).size(px(10.0)))
                            .on_click(move |_, _, cx| {
                                remove.update(cx, |v, cx| {
                                    if choice == v.filter.status.label() {
                                        v.filter.status = StatusFilter::Open;
                                    } else if v.filter.repository.as_deref() == Some(&choice) {
                                        v.filter.repository = None;
                                    } else if v.label_filters.contains(&choice) {
                                        v.label_filters.retain(|l| l != &choice);
                                    } else {
                                        v.tab = ListTab::All;
                                        v.involvement.clear();
                                    }
                                    v.loaded_key = None;
                                    v.reload(cx);
                                })
                            }),
                        ),
                );
            }
            header = header.child(
                div().px(px(24.0)).pt(px(2.0)).pb(px(8.0)).child(
                    chips.child(
                        self.inbox_button("pr-clear-filters", "Clear filters")
                            .h(px(20.0))
                            .px(px(4.0))
                            .child("Clear")
                            .on_click(move |_, _, cx| clear.update(cx, |v, cx| v.reset_inbox(cx))),
                    ),
                ),
            );
        }
        div()
            .w(px(self.list_width))
            .min_w(px(self.list_width))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .font_weight(gpui::FontWeight::NORMAL)
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(
                        div()
                            .id("pr-list-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.list_scroll)
                            .child(self.inbox_rows(cx)),
                    )
                    .children(self.scrollbar_thumb(&self.list_scroll, cx)),
            )
    }
    fn inbox_search(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.inbox_theme();
        let clear = cx.entity();
        div()
            .h(px(28.0))
            .w_full()
            .bg(theme.field_surface)
            .px(px(10.0))
            .rounded(px(10.0))
            .border(px(1.0))
            .border_color(theme.field_border)
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                icon("search", theme.text_muted.into())
                    .flex_none()
                    .size(px(12.0)),
            )
            .child(div().flex_1().min_w(px(0.0)).child(self.search.clone()))
            .when(!self.query.is_empty(), |s| {
                s.child(
                    self.inbox_button("pr-clear-search", "Clear search")
                        .size(px(18.0))
                        .px(px(0.0))
                        .child(icon("pr-close", theme.text_muted.into()).size(px(12.0)))
                        .on_click(move |_, _, cx| clear.update(cx, |v, cx| v.clear_search(cx))),
                )
            })
    }
    fn inbox_rows(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.inbox_theme();
        let mut body = div()
            .px(px(12.0))
            .pt(px(8.0))
            .pb(px(16.0))
            .flex()
            .flex_col()
            .gap(px(2.0));
        if let Some(error) = &self.list_error {
            let retry = cx.entity();
            return body
                .child(
                    div()
                        .px(px(12.0))
                        .py(px(24.0))
                        .text_size(px(13.0))
                        .text_color(theme.warning)
                        .child(error.clone()),
                )
                .child(
                    self.inbox_button("pr-retry-list", "Try again")
                        .child("Try again")
                        .on_click(move |_, _, cx| retry.update(cx, |v, cx| v.reload(cx))),
                );
        }
        if self.list_loading && self.groups.is_empty() {
            for (index, group) in GroupKind::loading_sections(self.tab, &self.query)
                .iter()
                .enumerate()
            {
                body = body.child(
                    div()
                        .id(("pr-loading", index))
                        .role(gpui::Role::Status)
                        .aria_label("Loading code review")
                        .aria_description(group.label())
                        .px(px(12.0))
                        .py(px(12.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(Self::spinner(theme.text_muted, 14.0))
                        .child(
                            div()
                                .h(px(12.0))
                                .w(px(150.0))
                                .rounded(px(4.0))
                                .bg(theme.skeleton),
                        ),
                );
            }
            return body;
        }

        let mut effective = self.filter.clone();
        if self
            .groups
            .iter()
            .flat_map(|g| &g.items)
            .any(|r| r.matches_link(&self.query))
        {
            effective.status = StatusFilter::All;
            effective.repository = None;
        }
        let filtered = crate::pull_requests::filter_groups(&self.groups, &effective, "");
        let rows: Vec<_> = filtered
            .iter()
            .flat_map(|g| &g.items)
            .filter(|r| self.kind.matches(r))
            .collect();
        if rows.is_empty() {
            let clear = cx.entity();
            return body.child(
                div()
                    .py(px(32.0))
                    .px(px(12.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(13.0))
                    .text_color(theme.text_muted)
                    .child("No pull requests and issues found")
                    .child(
                        div()
                            .text_size(px(12.0))
                            .child("Try another filter, or clear them."),
                    )
                    .child(
                        self.inbox_button("pr-empty-clear", "Clear filters")
                            .child("Clear filters")
                            .on_click(move |_, _, cx| clear.update(cx, |v, cx| v.reset_inbox(cx))),
                    ),
            );
        }
        let has_pins = rows.iter().any(|r| self.pinned(r));
        for pinned in [true, false] {
            let section: Vec<_> = rows
                .iter()
                .filter(|r| self.pinned(r) == pinned)
                .copied()
                .collect();
            if section.is_empty() {
                continue;
            }
            if has_pins {
                let collapse = cx.entity();
                body = body.child(
                    self.inbox_button(
                        SharedString::from(format!("pr-section-{pinned}")),
                        if pinned { "Pinned" } else { "All" },
                    )
                    .justify_start()
                    .h(px(24.0))
                    .px(px(12.0))
                    .child(if pinned { "Pinned" } else { "All" })
                    .on_click(move |_, _, cx| {
                        collapse.update(cx, |v, cx| {
                            v.toggle_group(
                                if pinned {
                                    GroupKind::Authored
                                } else {
                                    GroupKind::Results
                                },
                                cx,
                            )
                        })
                    }),
                );
            }
            let collapsed = has_pins
                && self.collapsed_groups.contains(&if pinned {
                    GroupKind::Authored
                } else {
                    GroupKind::Results
                });
            if !collapsed {
                for row in section.iter().take(self.visible_rows) {
                    body = body.child(self.inbox_row(row, cx));
                }
                if section.len() > self.visible_rows {
                    let more = cx.entity();
                    body = body.child(
                        self.inbox_button(
                            SharedString::from(format!("pr-show-more-{pinned}")),
                            "Show more",
                        )
                        .justify_start()
                        .child("Show more")
                        .on_click(move |_, _, cx| {
                            more.update(cx, |v, cx| {
                                v.visible_rows += 10;
                                cx.notify();
                            })
                        }),
                    );
                }
            }
        }
        if self.visible_rows > 10 {
            let less = cx.entity();
            body = body.child(
                self.inbox_button("pr-show-less", "Show less")
                    .justify_start()
                    .child("Show less")
                    .on_click(move |_, _, cx| {
                        less.update(cx, |v, cx| {
                            v.visible_rows = 10;
                            cx.notify();
                        })
                    }),
            );
        }
        body
    }
    fn inbox_row(&self, row: &PullRequestSummary, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.inbox_theme();
        let selected = self.selected.as_ref().is_some_and(|s| s.url == row.url);
        let pinned = self.pinned(row);
        let select = cx.entity();
        let item = row.clone();
        let pin = cx.entity();
        let pin_row = row.clone();
        let label = format!(
            "{} {} #{}",
            if pinned { "Unpin" } else { "Pin" },
            if row.is_issue() {
                "issue"
            } else {
                "pull request"
            },
            row.number
        );
        let glyph = if row.is_issue() {
            icon(
                if row.status == crate::pull_requests::PullRequestStatus::Closed {
                    "review-issue-closed"
                } else {
                    "review-issue"
                },
                if row.status == crate::pull_requests::PullRequestStatus::Closed {
                    theme.purple
                } else {
                    theme.chart_green
                }
                .into(),
            )
            .size(px(14.0))
            .into_any_element()
        } else {
            let mut status_theme = theme;
            status_theme.chart_red = theme.text_muted;
            if row.status == crate::pull_requests::PullRequestStatus::Open {
                status_theme.text_muted = theme.chart_green;
            }
            Self::state_glyph(row.status, status_theme, 14.0).into_any_element()
        };
        let repositories = self
            .groups
            .iter()
            .flat_map(|g| &g.items)
            .map(|r| &r.repository)
            .collect::<std::collections::HashSet<_>>()
            .len();
        div()
            .w_full()
            .rounded(px(10.0))
            .flex()
            .items_stretch()
            .group("code-review-row")
            .when(selected, |s| s.bg(theme.row_selected))
            .when(!selected, |s| s.hover(move |s| s.bg(theme.row_hover)))
            .child(
                div()
                    .id(SharedString::from(format!(
                        "pr-row-{}-{}",
                        row.repository, row.number
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(row.title.clone()))
                    .aria_selected(selected)
                    .flex_1()
                    .min_w(px(0.0))
                    .py(px(8.0))
                    .pl(px(12.0))
                    .pr(px(4.0))
                    .rounded(px(10.0))
                    .cursor_pointer()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .on_click(move |_, window, cx| {
                        select.update(cx, |v, cx| {
                            v.select(item.clone(), cx);
                            v.focus.focus(window, cx);
                        })
                    })
                    .child(
                        div()
                            .text_size(px(crate::typography::ui_size(14.0)))
                            .line_height(px(19.25))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .line_clamp(2)
                            .child(row.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(crate::typography::ui_size(12.0)))
                            .line_height(px(18.0))
                            .text_color(if self.mode == crate::theme::ThemeMode::Light {
                                gpui::rgba(0x0d0d0d6a)
                            } else {
                                let mut muted = theme.text_muted;
                                muted.a *= 0.7;
                                muted
                            })
                            .child(glyph)
                            .when(repositories > 1, |s| {
                                s.child(div().max_w(px(96.0)).truncate().child(
                                    row.repository.rsplit('/').next().unwrap_or("").to_owned(),
                                ))
                            })
                            .child(self.avatar(row.author_avatar_url.as_deref(), 14.0))
                            .child(div().truncate().child(row.author.clone()))
                            .child("·")
                            .child(row.age.clone())
                            .child(div().flex_1())
                            .child(format!("#{}", row.number)),
                    ),
            )
            .child(
                self.with_tooltip(
                    self.inbox_button(
                        SharedString::from(format!("pr-pin-row-{}-{}", row.repository, row.number)),
                        SharedString::from(label.clone()),
                    )
                    .size(px(28.0))
                    .px(px(0.0))
                    .mt(px(4.0))
                    .mr(px(4.0))
                    .opacity(if pinned { 1.0 } else { 0.0 })
                    .group_hover("code-review-row", |s| s.opacity(1.0))
                    .child(
                        icon(
                            if pinned {
                                "review-pin-filled"
                            } else {
                                "review-pin"
                            },
                            theme.text_muted.into(),
                        )
                        .size(px(14.0)),
                    )
                    .on_click(move |_, _, cx| pin.update(cx, |v, cx| v.toggle_pin(&pin_row, cx))),
                    &format!("pr-pin-{}-{}", row.repository, row.number),
                    label,
                    TooltipPlacement::Above,
                    cx,
                ),
            )
    }
    fn inbox_menu_row(
        id: impl Into<gpui::ElementId>,
        theme: super::PrTheme,
        highlighted: bool,
        disabled: bool,
    ) -> gpui::Stateful<Div> {
        Self::menu_row(id, theme, highlighted, disabled)
            .font_weight(gpui::FontWeight::NORMAL)
            .h(px(26.0))
            .rounded(px(6.0))
    }
    pub(super) fn filter_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.inbox_theme();
        let mut menu = Self::solid_menu_surface("pr-filter-menu", theme)
            .w(px(192.0))
            .p(px(4.0))
            .rounded(px(14.0));
        menu = menu.child(
            div()
                .px(px(8.0))
                .py(px(6.0))
                .text_size(px(12.0))
                .line_height(px(18.0))
                .text_color(theme.text_muted)
                .child("Status"),
        );
        for status in [
            StatusFilter::Open,
            StatusFilter::Closed,
            StatusFilter::Merged,
        ] {
            let v = cx.entity();
            menu = menu.child(
                Self::inbox_menu_row(
                    SharedString::from(format!("pr-status-filter-{status}")),
                    theme,
                    false,
                    false,
                )
                .rounded(px(8.0))
                .aria_label(status.label())
                .child(Self::state_glyph(
                    match status {
                        StatusFilter::Open => crate::pull_requests::PullRequestStatus::Open,
                        StatusFilter::Merged => crate::pull_requests::PullRequestStatus::Merged,
                        _ => crate::pull_requests::PullRequestStatus::Closed,
                    },
                    theme,
                    14.0,
                ))
                .child(status.label())
                .child(div().flex_1())
                .child(Self::menu_check_slot(self.filter.status == status, theme))
                .on_click(move |_, _, cx| {
                    v.update(cx, |v, cx| {
                        let choice = if v.filter.status == status {
                            StatusFilter::All
                        } else {
                            status
                        };
                        v.apply_status_filter(choice, cx)
                    })
                }),
            );
        }
        menu = menu.child(Self::menu_rule(theme)).child(
            div()
                .px(px(8.0))
                .py(px(6.0))
                .text_size(px(12.0))
                .line_height(px(18.0))
                .text_color(theme.text_muted)
                .child("Involvement"),
        );
        for (label, tab, relation) in [
            ("Anyone", ListTab::All, ""),
            ("Involving me", ListTab::All, "involves:@me"),
            ("Review requested", ListTab::Reviewing, ""),
            ("Authored by me", ListTab::Authored, ""),
            ("Assigned to me", ListTab::All, "assignee:@me"),
        ] {
            let v = cx.entity();
            let selected = self.tab == tab && self.involvement == relation;
            menu = menu.child(
                Self::inbox_menu_row(
                    SharedString::from(format!("pr-involvement-{label}")),
                    theme,
                    false,
                    false,
                )
                .rounded(px(8.0))
                .aria_label(label)
                .child(label)
                .child(div().flex_1())
                .child(Self::menu_check_slot(selected, theme))
                .on_click(move |_, _, cx| {
                    v.update(cx, |v, cx| {
                        v.involvement = relation.into();
                        if v.tab == tab {
                            v.loaded_key = None;
                            v.reload(cx);
                        } else {
                            v.select_tab(tab, cx);
                        }
                        v.dismiss_menus(cx);
                    })
                }),
            );
        }
        menu = menu.child(Self::menu_rule(theme));
        for (sub, label) in [
            (FilterSubmenu::Repository, "Projects"),
            (FilterSubmenu::Labels, "Labels"),
        ] {
            let v = cx.entity();
            let click = v.clone();
            menu = menu.child(
                Self::inbox_menu_row(
                    SharedString::from(format!("pr-filter-{label}")),
                    theme,
                    self.filter_submenu == Some(sub),
                    false,
                )
                .relative()
                .child(self.control_anchor(format!("pr-filter-{label}")))
                .rounded(px(8.0))
                .aria_label(label)
                .child(
                    icon(
                        if sub == FilterSubmenu::Repository {
                            "pr-menu-repository"
                        } else {
                            "review-label"
                        },
                        theme.text_muted.into(),
                    )
                    .size(px(14.0)),
                )
                .child(label)
                .child(div().flex_1())
                .child(
                    icon("pr-chevron-right", theme.text_muted.into()).size(px(MENU_CHEVRON_SIZE)),
                )
                .on_mouse_move(move |_, _, cx| {
                    v.update(cx, |v, cx| v.hover_filter_submenu(Some(sub), cx))
                })
                .on_click(move |_, _, cx| {
                    click.update(cx, |v, cx| v.hover_filter_submenu(Some(sub), cx))
                }),
            );
        }
        let clear = cx.entity();
        menu.child(Self::menu_rule(theme)).child(
            Self::inbox_menu_row("pr-menu-clear", theme, false, false)
                .aria_label("Clear filters")
                .child("Clear filters")
                .on_click(move |_, _, cx| clear.update(cx, |v, cx| v.reset_inbox(cx))),
        )
    }
    pub(super) fn filter_submenu(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Stateful<Div>> {
        let sub = self.filter_submenu?;
        let theme = self.inbox_theme();
        let mut menu = Self::solid_menu_surface("pr-filter-submenu", theme)
            .w(px(240.0))
            .max_h(px(400.0))
            .overflow_y_scroll();
        let rows: Vec<(String, bool)> = match sub {
            FilterSubmenu::Status => StatusFilter::ALL
                .iter()
                .map(|s| (s.label().into(), self.filter.status == *s))
                .collect(),
            FilterSubmenu::Repository => {
                std::iter::once(("All projects".into(), self.filter.repository.is_none()))
                    .chain(
                        self.repositories
                            .iter()
                            .map(|r| (r.clone(), self.filter.repository.as_ref() == Some(r))),
                    )
                    .collect()
            }
            FilterSubmenu::Labels => {
                let mut labels = self
                    .groups
                    .iter()
                    .flat_map(|g| &g.items)
                    .flat_map(|r| r.labels.iter().map(|l| l.0.clone()))
                    .collect::<Vec<_>>();
                labels.extend(self.label_filters.clone());
                labels.sort();
                labels.dedup();
                labels
                    .into_iter()
                    .map(|l| {
                        let selected = self.label_filters.contains(&l);
                        (l, selected)
                    })
                    .collect()
            }
        };
        if rows.is_empty() {
            menu = menu.child(
                div()
                    .px(px(8.0))
                    .py(px(6.0))
                    .text_color(theme.text_muted)
                    .child("No labels on the loaded items"),
            );
        }
        for (index, (label, checked)) in rows.into_iter().enumerate() {
            let v = cx.entity();
            let choice = label.clone();
            menu = menu.child(
                Self::inbox_menu_row(
                    SharedString::from(format!("pr-sub-{index}")),
                    theme,
                    false,
                    false,
                )
                .aria_label(SharedString::from(label.clone()))
                .child(Self::menu_check_slot(checked, theme))
                .child(div().min_w(px(0.0)).truncate().child(label))
                .on_click(move |_, _, cx| {
                    v.update(cx, |v, cx| match sub {
                        FilterSubmenu::Status => {
                            v.apply_status_filter(StatusFilter::ALL[index], cx)
                        }
                        FilterSubmenu::Repository => {
                            v.apply_repository_filter((index > 0).then(|| choice.clone()), cx)
                        }
                        FilterSubmenu::Labels => {
                            if checked {
                                v.label_filters.retain(|l| l != &choice);
                            } else {
                                v.label_filters.push(choice.clone());
                            }
                            v.loaded_key = None;
                            v.dismiss_menus(cx);
                            v.reload(cx);
                        }
                    })
                }),
            );
        }
        Some(menu)
    }
    pub(super) fn inbox_aux_menu(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Stateful<Div>> {
        let theme = self.inbox_theme();
        let mut menu = Self::solid_menu_surface("pr-inbox-menu", theme)
            .w(px(190.0))
            .rounded(px(12.0));
        match self.list_menu.as_ref()? {
            ListMenu::Sort => {
                for (newest, label) in [(true, "Newest"), (false, "Recently updated")] {
                    let v = cx.entity();
                    menu = menu.child(
                        Self::inbox_menu_row(
                            SharedString::from(format!("pr-sort-{newest}")),
                            theme,
                            false,
                            false,
                        )
                        .aria_label(label)
                        .child(Self::menu_check_slot(self.newest == newest, theme))
                        .child(label)
                        .on_click(move |_, _, cx| {
                            v.update(cx, |v, cx| {
                                v.newest = newest;
                                v.loaded_key = None;
                                v.dismiss_menus(cx);
                                v.reload(cx);
                            })
                        }),
                    )
                }
            }
            ListMenu::More => {
                let refresh = cx.entity();
                let clear = cx.entity();
                menu = menu
                    .child(
                        Self::inbox_menu_row("pr-refresh", theme, false, self.list_loading)
                            .aria_label("Refresh")
                            .child("Refresh")
                            .on_click(move |_, _, cx| {
                                refresh.update(cx, |v, cx| {
                                    v.dismiss_menus(cx);
                                    v.refresh(cx);
                                })
                            }),
                    )
                    .child(
                        Self::inbox_menu_row("pr-more-clear", theme, false, false)
                            .aria_label("Clear filters")
                            .child("Clear filters")
                            .on_click(move |_, _, cx| clear.update(cx, |v, cx| v.reset_inbox(cx))),
                    );
            }
            ListMenu::Filter => return None,
        }
        Some(menu)
    }
}
