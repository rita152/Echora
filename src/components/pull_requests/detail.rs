//! Right pane: header tabs, the `Summary` surface, and the comment composer.

use gpui::{Div, SharedString, div, prelude::*, px};

use super::{DetailTab, PullRequestsView, ReviewScope, theme::*};
use crate::components::icons::icon;
use crate::pull_requests::{CheckState, PullRequestStatus};
use crate::theme::Theme;

/// Which collapsible section a header row toggles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SectionKind {
    Checks,
    Activity,
}

impl PullRequestsView {
    pub(super) fn detail_pane(
        &self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let theme = self.theme();
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .relative()
            .child(self.detail_header(cx))
            .child(self.detail_body(cx))
            .children(self.detail_overlays(cx))
    }

    fn detail_header(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        if self.review_tab.is_some() && self.selected.is_some() {
            return self.review_tab_strip(cx).into_any_element();
        }
        let theme = self.theme();
        let has_detail = self.selected.is_some();
        let mut tabs = div()
            .id("pr-detail-tabs")
            .role(gpui::Role::TabList)
            .aria_label("Pull request view")
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(2.0));
        if has_detail {
            for (tab, label) in [(DetailTab::Summary, "Summary"), (DetailTab::Code, "Code")] {
                let selected = self.review_tab.is_none() && self.detail_tab == tab;
                let view = cx.entity();
                tabs = tabs.child(
                    Self::toolbar_button(
                        SharedString::from(format!("pr-detail-tab-{label}")),
                        theme,
                        selected,
                    )
                    .role(gpui::Role::Tab)
                    .aria_label(SharedString::from(label))
                    .aria_selected(selected)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.set_detail_tab(tab, cx));
                    })
                    .child(label),
                );
            }
        }

        let draft = self
            .detail
            .as_ref()
            .is_some_and(|detail| detail.summary.status == PullRequestStatus::Draft);
        let status = self.selected.as_ref().map(|pr| pr.status);
        let view = cx.entity();
        let mut actions = div().flex_none().flex().items_center().gap(px(4.0));
        if has_detail {
            let browser_view = view.clone();
            actions = actions.child(
                Self::toolbar_button("pr-open-browser", theme, false)
                    .w(px(28.0))
                    .px(px(0.0))
                    .justify_center()
                    .aria_label("Open in browser")
                    .on_click(move |_, _, cx| {
                        browser_view.update(cx, |view, cx| view.open_in_browser(cx));
                    })
                    .child(icon("pr-open-browser", theme.text.into()).size(px(16.0))),
            );
            let chat_view = view.clone();
            let chat_label = if self.chat_thread.is_some() {
                "Open chat"
            } else {
                "Chat"
            };
            actions = actions.child(
                Self::toolbar_button("pr-chat", theme, true)
                    .aria_label(chat_label)
                    .on_click(move |_, _, cx| {
                        chat_view.update(cx, |view, cx| view.open_chat(cx));
                    })
                    .child(chat_label),
            );
            // Merging only exists for open pull requests; a draft keeps the
            // button but disables it.
            if matches!(
                status,
                Some(PullRequestStatus::Open | PullRequestStatus::Draft)
            ) {
                let merge_view = view.clone();
                let enabled = !draft && self.detail.is_some() && !self.mutation_pending;
                actions = actions.child(
                    div()
                        .id("pr-merge")
                        .h(px(28.0))
                        .px(px(9.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .rounded(px(12.5))
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .bg(theme.inverted_surface)
                        .text_color(theme.inverted_text)
                        .when(enabled, |button| {
                            button.on_click(move |_, _, cx| {
                                merge_view.update(cx, |view, cx| view.merge(cx));
                            })
                        })
                        .when(!enabled, |button| button.opacity(0.5))
                        .role(gpui::Role::Button)
                        .aria_label(if self.mutation_pending {
                            "Saving to GitHub"
                        } else if self.detail.is_none() {
                            "Merge unavailable: details are loading"
                        } else if draft {
                            "Merge unavailable: Mark as \"Ready for review\" to merge"
                        } else {
                            "Merge"
                        })
                        .child(icon("pr-merge", theme.inverted_text.into()).size(px(16.0)))
                        .child("Merge"),
                );
            }
            let fullscreen_view = view.clone();
            let fullscreen = self.fullscreen;
            actions = actions.child(
                Self::toolbar_button("pr-fullscreen", theme, false)
                    .w(px(28.0))
                    .px(px(0.0))
                    .justify_center()
                    .aria_label(if fullscreen {
                        "Exit full screen"
                    } else {
                        "Enter full screen"
                    })
                    .on_click(move |_, _, cx| {
                        fullscreen_view.update(cx, |view, cx| view.toggle_fullscreen(cx));
                    })
                    .child(
                        icon(
                            if fullscreen {
                                "pr-exit-fullscreen"
                            } else {
                                "pr-fullscreen"
                            },
                            theme.text_muted.into(),
                        )
                        .size(px(16.0)),
                    ),
            );
        }

        // `grid h-toolbar grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] gap-3`:
        // a detail panel 900px or wider (full screen, say) centers the tabs
        // between the state glyph with the truncated title and the actions.
        if has_detail && !self.compact() && self.pane_width >= DETAIL_WIDE_HEADER {
            let title = self
                .detail
                .as_ref()
                .map(|detail| detail.summary.title.clone())
                .or_else(|| self.selected.as_ref().map(|pr| pr.title.clone()))
                .unwrap_or_default();
            return div()
                .flex_none()
                .h(px(TOOLBAR_HEIGHT))
                .px(px(16.0))
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
                        .when_some(status, |left, status| {
                            left.child(Self::state_glyph(status, theme, 18.0))
                        })
                        .child(
                            div()
                                .min_w(px(0.0))
                                .max_w(px(200.0))
                                .truncate()
                                .text_size(px(13.0))
                                .line_height(px(18.5714))
                                .text_color(theme.text)
                                .child(title),
                        ),
                )
                .child(tabs.flex_none())
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .justify_end()
                        .child(actions),
                )
                .into_any_element();
        }
        // `grid h-toolbar px-toolbar`: at this panel width the state glyph,
        // the tab list, and the actions sit in one row 4px apart.
        div()
            .flex_none()
            .h(px(TOOLBAR_HEIGHT))
            .px(px(16.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .when_some(status.filter(|_| has_detail), |header, status| {
                header.child(Self::state_glyph(status, theme, 18.0))
            })
            .when(self.compact(), |header| {
                header.child(
                    Self::toolbar_button("pr-back-list", theme, false)
                        .aria_label("Back to pull requests")
                        .child("Back")
                        .on_click({
                            let view = cx.entity();
                            move |_, _, cx| {
                                view.update(cx, |view, cx| {
                                    if let Some(pr) = view.selected.clone() {
                                        view.select(pr, cx);
                                    }
                                });
                            }
                        }),
                )
            })
            .child(tabs)
            .child(actions)
            .into_any_element()
    }

    /// A review opens as an app-shell task tab: the 46px strip (`ps-2 pe-1.5`,
    /// 6px gaps) holds its 240px tab (`rounded-lg`, 8% fill, 0.5px border)
    /// with the review glyph, the title, and the close button, then the full
    /// screen toggle. The pull request tabs and actions give way to it.
    fn review_tab_strip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        // The tab keeps the pull request's title whatever its scope.
        let title = self
            .detail
            .as_ref()
            .map(|detail| detail.summary.title.clone())
            .unwrap_or_default();
        let close_view = view.clone();
        let fullscreen = self.fullscreen;
        // The selected tab's surface, which the title fades into.
        let tab_fill = theme.tab_selected_surface;
        div()
            .flex_none()
            .h(px(TOOLBAR_HEIGHT))
            .pl(px(8.0))
            .pr(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div().flex_1().min_w(px(0.0)).flex().items_center().child(
                    div()
                        .id("pr-review-tab")
                        .relative()
                        .flex_none()
                        .w(px(238.0))
                        .h(px(32.0))
                        .pl(px(10.0))
                        .pr(px(27.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(12.5))
                        .bg(tab_fill)
                        .border(px(0.5))
                        .border_color(theme.border)
                        // `shadow-sm`
                        .shadow(vec![
                            gpui::BoxShadow::new(px(0.0), px(1.0), gpui::rgba(0x00000014).into())
                                .blur_radius(px(2.0))
                                .spread_radius(px(-1.0)),
                        ])
                        .role(gpui::Role::Tab)
                        .aria_selected(true)
                        .aria_label(SharedString::from(format!("{title} tab")))
                        .text_size(px(13.0))
                        .line_height(px(18.5714))
                        .text_color(theme.text)
                        .child(
                            icon("panel-review", theme.text.into())
                                .flex_none()
                                .size(px(16.0)),
                        )
                        // `-ms-1 text-fade-truncate`: the title sits 4px from
                        // the glyph and fades out instead of an ellipsis.
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .min_w(px(0.0))
                                .ml(px(-4.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(title.clone())
                                .child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .right_0()
                                        .w(px(24.0))
                                        .bg(gpui::linear_gradient(
                                            90.0,
                                            gpui::linear_color_stop(
                                                gpui::Rgba { a: 0.0, ..tab_fill },
                                                0.0,
                                            ),
                                            gpui::linear_color_stop(tab_fill, 1.0),
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .id("pr-review-tab-close")
                                .absolute()
                                .right(px(7.0))
                                .top(px(6.0))
                                .size(px(20.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .role(gpui::Role::Button)
                                .aria_label(SharedString::from(format!("Close {title} tab")))
                                .hover(move |style| style.bg(theme.control_hover))
                                .on_click(move |_, _, cx| {
                                    close_view.update(cx, |view, cx| view.close_review_tab(cx));
                                })
                                .child(icon("pr-close", theme.text_muted.into()).size(px(14.0))),
                        ),
                ),
            )
            .child(
                Self::toolbar_button("pr-fullscreen", theme, false)
                    .w(px(28.0))
                    .px(px(0.0))
                    .justify_center()
                    .aria_label(if fullscreen {
                        "Exit full screen"
                    } else {
                        "Enter full screen"
                    })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.toggle_fullscreen(cx));
                    })
                    .child(
                        icon(
                            if fullscreen {
                                "pr-exit-fullscreen"
                            } else {
                                "pr-fullscreen"
                            },
                            theme.text_muted.into(),
                        )
                        .size(px(16.0)),
                    ),
            )
    }

    /// The plain state glyph (`PullRequestStatusIcon`): draft and open in
    /// `text-codex-description`, merged in `text-purple`, closed in
    /// `text-chart-red`.
    pub(super) fn state_glyph(
        status: PullRequestStatus,
        theme: PrTheme,
        size: f32,
    ) -> impl IntoElement {
        let (name, color) = match status {
            PullRequestStatus::Draft => ("pr-status-draft", theme.text_muted),
            PullRequestStatus::Open => ("pr-status-open", theme.text_muted),
            PullRequestStatus::Merged => ("pr-status-merged", theme.purple),
            PullRequestStatus::Closed => ("pr-status-closed", theme.chart_red),
        };
        icon(name, color.into()).flex_none().size(px(size))
    }

    /// Title and body edits exist only for the author of an open pull request.
    pub(super) fn can_edit(&self) -> bool {
        self.detail.as_ref().is_some_and(|detail| {
            detail.author.is_self
                && matches!(
                    detail.summary.status,
                    PullRequestStatus::Open | PullRequestStatus::Draft
                )
        })
    }

    fn detail_body(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let theme = self.theme();
        if self.selected.is_none() {
            return div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.0))
                .text_color(theme.text)
                .child("Select pull request to view")
                .into_any_element();
        }
        if self.detail_loading {
            return div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(theme.text_muted)
                .child("Loading pull request details")
                .into_any_element();
        }
        if let Some(error) = self.detail_error.clone() {
            return div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(theme.warning)
                .child(error)
                .flex_col()
                .gap(px(12.0))
                .child(
                    div()
                        .id("pr-retry-detail.rs")
                        .role(gpui::Role::Button)
                        .aria_label("Retry loading pull request")
                        .cursor_pointer()
                        .child("Retry")
                        .on_click({
                            let view = cx.entity();
                            move |_, _, cx| {
                                view.update(cx, |view, cx| {
                                    if let Some(pr) = view.selected.clone() {
                                        view.load_detail(pr.repository, pr.number, cx);
                                    }
                                });
                            }
                        }),
                )
                .into_any_element();
        }
        if self.detail_tab == DetailTab::Code || self.review_tab.is_some() {
            self.diff_surface(cx).into_any_element()
        } else {
            self.summary_surface(cx).into_any_element()
        }
    }

    fn summary_surface(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let theme = self.theme();
        let Some(detail) = self.detail.as_ref() else {
            return div().into_any_element();
        };
        let markdown = crate::components::markdown::render_pull_request_markdown(
            &detail.body,
            Theme::for_mode(self.mode),
            "pull-request-description",
        );
        // The page column (`mx-auto max-w-[var(--thread-content-max-width)]`,
        // 768px, `gap-[var(--detail-page-section-gap)]` 24px) inside
        // `main.px-5.pb-5` with the 11px scrollbar gutter.
        let mut column = div()
            .w_full()
            .max_w(px(DETAIL_CONTENT_MAX_WIDTH))
            .mx_auto()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .child(self.title_block(cx))
            .child(self.meta_rows(cx));

        let description_view = cx.entity();
        let mut description = div().flex().flex_col().gap(px(16.0)).child(
            Self::section_summary(
                "pr-description-toggle".into(),
                theme,
                "Description".into(),
                None,
                !self.description_collapsed,
            )
            .on_click(move |_, _, cx| {
                description_view.update(cx, |view, cx| view.toggle_description(cx));
            })
            .when(self.can_edit(), |summary| {
                summary.child(self.description_actions(cx))
            }),
        );
        if !self.description_collapsed {
            description = description.child(if let Some(editor) = self.description_edit.clone() {
                div()
                    .px(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(Self::editor_frame(
                        editor,
                        280.0,
                        "pr-description-editor-frame",
                    ))
                    .child(self.description_edit_actions(cx))
            } else if detail.body.trim().is_empty() {
                div()
                    .px(px(8.0))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(theme.text_muted)
                    .child("No description provided")
            } else {
                div().px(px(8.0)).child(markdown)
            });
        }
        column = column.child(description);
        column = column.child(self.checks_section(cx));
        column = column.child(self.activity_section(cx));
        column = column.child(div().px(px(8.0)).child(self.comment_composer(cx)));
        // The reference scrolls the whole tab body so long descriptions reach
        // the Checks / Activity / commits sections.
        let capture_offset = (self.capture_offset > 0.0).then_some(self.capture_offset);
        div()
            .id("pr-detail-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&self.detail_scroll)
            .child(
                div()
                    .when_some(capture_offset, |wrapper, offset| {
                        wrapper.relative().top(px(-offset))
                    })
                    .pl(px(PANE_PADDING))
                    .pr(px(PANE_PADDING + SCROLLBAR_GUTTER))
                    .pb(px(PANE_PADDING))
                    .child(column),
            )
            .into_any_element()
    }

    /// `header.flex.flex-col.gap-4.px-2` under `pt-4`: the 24px title (1.2
    /// line box) and, 6px below it, the author line in `text-secondary`. The
    /// edit pencil exists only when the viewer may edit the title.
    fn title_block(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let Some(detail) = self.detail.as_ref() else {
            return div();
        };
        let title = detail.summary.title.clone();
        let author = detail.author.login.clone();
        let age = detail.created_age.clone();
        let view = cx.entity();
        let mut title_row = div().flex().items_start().justify_between().gap(px(16.0));
        if let Some(editor) = self.title_edit.clone() {
            let save_view = view.clone();
            let cancel_view = view.clone();
            title_row = title_row
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .h(px(36.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(12.5))
                        .border(px(1.0))
                        .border_color(theme.focus_ring)
                        .child(div().flex_1().min_w(px(0.0)).child(editor)),
                )
                .child(
                    Self::toolbar_button("pr-title-cancel", theme, false)
                        .w(px(28.0))
                        .px(px(0.0))
                        .justify_center()
                        .aria_label("Cancel title editing")
                        .on_click(move |_, window, cx| {
                            cancel_view.update(cx, |view, cx| view.cancel_title_edit(window, cx));
                        })
                        .child(icon("close-dialog", theme.text_muted.into()).size(px(16.0))),
                )
                .child(
                    Self::toolbar_button("pr-title-save", theme, false)
                        .w(px(28.0))
                        .px(px(0.0))
                        .justify_center()
                        .aria_label("Save title")
                        .on_click(move |_, _, cx| {
                            save_view.update(cx, |view, cx| view.save_title(cx));
                        })
                        .child(icon("check", theme.text.into()).size(px(16.0))),
                );
        } else {
            title_row = title_row
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(24.0))
                                .line_height(px(28.8))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(title),
                        )
                        .child(
                            div()
                                .mt(px(1.0))
                                .h(px(21.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .text_color(theme.icon_muted)
                                .child(self.avatar(detail.author.avatar_url.as_deref(), 18.0))
                                .child(author)
                                .when(!age.is_empty(), |line| line.child("·").child(age)),
                        ),
                )
                .when(self.can_edit(), |row| {
                    row.child(
                        Self::toolbar_button("pr-edit-title", theme, false)
                            .w(px(28.0))
                            .px(px(0.0))
                            .justify_center()
                            .aria_label("Edit title")
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |view, cx| view.begin_title_edit(cx));
                                }
                            })
                            .child(icon("pr-edit-title", theme.text.into()).size(px(16.0))),
                    )
                });
        }
        div().pt(px(16.0)).child(div().px(px(8.0)).child(title_row))
    }

    /// The overview `dl`: `px-2 pb-2`, rows of `grid gap-x-3` with a 120px
    /// label column, `py-row-y` (5px), and 14/20 type.
    fn meta_rows(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let Some(detail) = self.detail.as_ref() else {
            return div();
        };
        let editable = self.can_edit();
        let mut list = div().flex().flex_col().px(px(8.0)).pb(px(8.0));

        let (additions, deletions) = (detail.summary.additions, detail.summary.deletions);
        let branch = div()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(detail.summary.head_branch.clone()),
                    )
                    .child(
                        icon("pr-chevron-right", theme.text_muted.into())
                            .flex_none()
                            .size(px(14.0)),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(detail.summary.base_branch.clone()),
                    ),
            )
            .child({
                let view = cx.entity();
                Self::toolbar_button("pr-review-changes", theme, false)
                    .aria_label("Review pull request changes")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.open_review_tab(cx));
                    })
                    .child(Self::diff_stats(
                        additions,
                        deletions,
                        theme.additions_text,
                        theme.deletions_text,
                        13.0,
                    ))
            });
        list = list.child(self.meta_row(
            "Branch",
            Self::meta_label_icon("pr-meta-branch", theme),
            theme,
            branch,
        ));

        let view = cx.entity();
        let mut reviewers = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(8.0));
        let requested = &detail.requested_reviewers;
        if requested.is_empty() && !editable {
            reviewers = reviewers.child(div().text_color(theme.text_muted).child("No reviewers"));
        }
        for user in requested {
            reviewers = reviewers.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.avatar(user.avatar_url.as_deref(), 18.0))
                    .child(user.login.clone()),
            );
        }
        if editable {
            reviewers = reviewers.child(
                div()
                    .flex_none()
                    .when(requested.is_empty(), |slot| slot.ml(px(-8.0)))
                    .child(
                        Self::toolbar_button("pr-request-reviewers", theme, false)
                            .relative()
                            .child(self.control_anchor("pr-request-reviewers"))
                            .text_color(theme.text)
                            .aria_label(if requested.is_empty() {
                                "Request reviewers"
                            } else {
                                "Manage reviewers"
                            })
                            .on_click(move |_, _, cx| {
                                view.update(cx, |view, cx| view.open_reviewers(cx));
                            })
                            .child(icon("pr-plus", theme.text.into()).size(px(14.0)))
                            .when(requested.is_empty(), |button| {
                                button.child(div().pr(px(4.0)).child("Request"))
                            }),
                    ),
            );
        }
        list = list.child(self.meta_row(
            "Reviewers",
            Self::meta_label_icon("pr-meta-reviewers", theme),
            theme,
            reviewers,
        ));

        let count = detail.comment_count();
        let comments = div().child(match count {
            0 => "No comments".to_string(),
            1 => "1 comment".to_string(),
            count => format!("{count} comments"),
        });
        list = list.child(self.meta_row(
            "Comments",
            Self::meta_label_icon("pr-meta-comments", theme),
            theme,
            comments,
        ));

        let checks = div().child(if detail.checks.is_empty() {
            "No CI checks".to_string()
        } else if detail
            .checks
            .iter()
            .any(|check| check.state == CheckState::Failed)
        {
            "Failing".to_string()
        } else if detail
            .checks
            .iter()
            .any(|check| check.state == CheckState::Pending)
        {
            "Pending".to_string()
        } else {
            "Successful".to_string()
        });
        list = list.child(self.meta_row(
            "Checks",
            Self::meta_label_icon("pr-meta-checks", theme),
            theme,
            checks,
        ));

        let status = detail.summary.status;
        let status_menu_view = cx.entity();
        let status_value: gpui::AnyElement = if status.is_merged() || !detail.author.is_self {
            div().child(status.label()).into_any_element()
        } else {
            Self::toolbar_button("pr-status", theme, false)
                .relative()
                .child(self.control_anchor("pr-status"))
                .ml(px(-9.0))
                .text_color(theme.text)
                .text_size(px(14.0))
                .aria_label("Change pull request status")
                .on_click(move |_, _, cx| {
                    status_menu_view.update(cx, |view, cx| view.toggle_status_menu(cx));
                })
                .child(status.label())
                .child(icon("section-chevron", theme.text_muted.into()).size(px(14.0)))
                .into_any_element()
        };
        list = list.child(self.meta_row(
            "Status",
            Self::state_glyph(status, theme, 18.0).into_any_element(),
            theme,
            status_value,
        ));
        list
    }

    fn meta_label_icon(name: &'static str, theme: PrTheme) -> gpui::AnyElement {
        icon(name, theme.text_muted.into())
            .flex_none()
            .size(px(18.0))
            .into_any_element()
    }

    fn meta_row(
        &self,
        label: &'static str,
        glyph: gpui::AnyElement,
        theme: PrTheme,
        content: impl IntoElement,
    ) -> Div {
        div()
            .min_h(px(30.0))
            .py(px(5.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .child(
                div()
                    .w(px(120.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_color(theme.text_muted)
                    .child(glyph)
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .text_color(theme.text)
                    .child(content),
            )
    }

    fn description_actions(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        div()
            .id("pr-description-actions")
            .relative()
            .child(self.control_anchor("pr-description-actions"))
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.5))
            .cursor_pointer()
            .hover(move |style| style.bg(theme.control_hover))
            .role(gpui::Role::Button)
            .aria_label("Description actions")
            .on_click(move |_, _, cx| {
                view.update(cx, |view, cx| view.toggle_description_menu(cx));
            })
            .child(icon("pr-description-actions", theme.text_muted.into()).size(px(18.0)))
    }

    fn description_edit_actions(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let cancel = view.clone();
        div()
            .flex()
            .justify_end()
            .gap(px(4.0))
            .child(
                div()
                    .id("pr-description-cancel")
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(12.5))
                    .text_size(px(13.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.control_hover))
                    .role(gpui::Role::Button)
                    .aria_label("Cancel")
                    .on_click(move |_, _, cx| {
                        cancel.update(cx, |view, cx| view.cancel_description_edit(cx));
                    })
                    .child("Cancel"),
            )
            .child(
                div()
                    .id("pr-description-save")
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(12.5))
                    .text_size(px(13.0))
                    .bg(theme.inverted_surface)
                    .text_color(theme.inverted_text)
                    .cursor_pointer()
                    .role(gpui::Role::Button)
                    .aria_label("Save")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.save_description(cx));
                    })
                    .child("Save"),
            )
    }

    /// `Checks`: without checks, one centered tertiary `No CI checks` row.
    fn checks_section(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let Some(detail) = self.detail.as_ref() else {
            return div();
        };
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.section_header("Checks", self.checks_expanded, SectionKind::Checks, cx));
        if self.checks_expanded {
            let mut rows = div().flex().flex_col().gap(px(4.0)).px(px(8.0));
            if detail.checks.is_empty() {
                rows = rows.child(
                    div()
                        .min_h(px(30.0))
                        .py(px(5.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(8.0))
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(theme.text_muted)
                        .child("No CI checks"),
                );
            } else {
                for check in &detail.checks {
                    let color = match check.state {
                        CheckState::Passed => theme.chart_green,
                        CheckState::Failed => theme.chart_red,
                        _ => theme.text_muted,
                    };
                    rows = rows.child(
                        div()
                            .min_h(px(30.0))
                            .py(px(5.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .child(icon("check", color.into()).size(px(16.0)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(check.name.clone()),
                            )
                            .child(
                                div()
                                    .text_color(theme.text_muted)
                                    .child(check.state.label()),
                            ),
                    );
                }
            }
            section = section.child(rows);
        }
        section
    }

    fn section_header(
        &self,
        label: &str,
        expanded: bool,
        section: SectionKind,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        self.section_header_owned(label.to_string(), None, expanded, section, cx)
    }

    /// A `details > summary` section header: `ps-2 pe-0.5 pb-2` over a
    /// `border-subtle` rule, the 16/24 medium label, the 14px chevron in the
    /// label's color (turned right while closed), and an optional tertiary
    /// count after it (`Activity ⌄ 7`).
    pub(super) fn section_header_owned(
        &self,
        label: String,
        count: Option<usize>,
        expanded: bool,
        section: SectionKind,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        Self::section_summary(
            SharedString::from(format!("pr-section-{label}")),
            theme,
            label.clone(),
            count,
            expanded,
        )
        .on_click(move |_, _, cx| {
            view.update(cx, |view, cx| match section {
                SectionKind::Checks => view.toggle_checks(cx),
                SectionKind::Activity => view.toggle_activity(cx),
            });
        })
    }

    pub(super) fn section_summary(
        id: SharedString,
        theme: PrTheme,
        label: String,
        count: Option<usize>,
        expanded: bool,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .flex_none()
            .pl(px(8.0))
            .pr(px(2.0))
            .pb(px(8.0))
            .border_b(px(1.0))
            .border_color(theme.border_subtle)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(label.clone()))
            .aria_expanded(expanded)
            .child(
                div()
                    .min_h(px(28.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(16.0))
                    .line_height(px(24.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(label)
                    .child(
                        icon("section-chevron", theme.text.into())
                            .size(px(14.0))
                            .when(!expanded, |chevron| {
                                chevron.with_transformation(gpui::Transformation::rotate(
                                    gpui::radians(-std::f32::consts::FRAC_PI_2),
                                ))
                            }),
                    )
                    .when_some(count, |row, count| {
                        row.child(
                            div()
                                .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                                .text_color(theme.text_muted)
                                .child(count.to_string()),
                        )
                    }),
            )
    }

    /// The page's footer composer (`Pull request comment`).
    fn comment_composer(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let has_text =
            !self.mutation_pending && !self.comment_box.read(cx).text().trim().is_empty();
        self.composer(
            self.comment_box.clone(),
            "pr-comment-composer-frame",
            true,
            super::render::ComposerActions {
                cancel: None,
                post_label: "Post comment",
                post_enabled: has_text,
                post: Box::new(move |_, _, cx: &mut gpui::App| {
                    view.update(cx, |view, cx| view.post_comment(cx));
                }),
            },
            cx,
        )
    }

    /// The scope pill's label: `All PR changes`, or just `Commit` while one
    /// commit is chosen (its menu names which).
    pub(super) fn summary_scope_label(&self) -> String {
        match self
            .review_tab
            .as_ref()
            .map(|tab| &tab.scope)
            .unwrap_or(&ReviewScope::AllChanges)
        {
            ReviewScope::AllChanges => "All PR changes".to_string(),
            ReviewScope::Commit(_) => "Commit".to_string(),
        }
    }
}
