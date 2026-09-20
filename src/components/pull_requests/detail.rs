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
    Commits,
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

    fn detail_header(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let has_detail = self.selected.is_some();
        let mut tabs = div()
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
                    div()
                        .id(SharedString::from(format!("pr-detail-tab-{label}")))
                        .h(px(28.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .rounded(px(12.5))
                        .text_size(px(13.0))
                        .cursor_pointer()
                        .role(gpui::Role::Button)
                        .aria_label(SharedString::from(label))
                        .when(selected, |button| {
                            button.bg(theme.control).text_color(theme.text)
                        })
                        .when(!selected, |button| {
                            button.text_color(theme.text_muted).hover(move |style| {
                                style.bg(theme.control_hover).text_color(theme.text)
                            })
                        })
                        .on_click(move |_, _, cx| {
                            view.update(cx, |view, cx| view.set_detail_tab(tab, cx));
                        })
                        .child(label),
                );
            }
            if let Some(review) = self.review_tab.clone() {
                let view = cx.entity();
                let title = self
                    .detail
                    .as_ref()
                    .map(|detail| detail.summary.title.clone())
                    .unwrap_or_default();
                let label = match &review.scope {
                    ReviewScope::AllChanges => title.clone(),
                    ReviewScope::Commit(_) => self.summary_scope_label(),
                };
                tabs = tabs.child(
                    div()
                        .id("pr-review-tab")
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .h(px(28.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .rounded(px(12.5))
                        .bg(theme.control)
                        .text_color(theme.text)
                        .text_size(px(13.0))
                        .child(icon("panel-review", theme.text_muted.into()).size(px(14.0)))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .max_w(px(220.0))
                                .text_ellipsis()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(label.clone()),
                        )
                        .child(
                            div()
                                .id("pr-review-tab-scope")
                                .relative()
                                .child(self.control_anchor("pr-review-tab-scope"))
                                .size(px(18.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(9999.0))
                                .cursor_pointer()
                                .role(gpui::Role::Button)
                                .aria_label("Pull request changes scope")
                                .hover(move |style| style.bg(theme.control_hover))
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |view, cx| view.toggle_scope_menu(cx));
                                    }
                                })
                                .child(
                                    icon("section-chevron", theme.text_muted.into()).size(px(12.0)),
                                ),
                        )
                        .child(
                            div()
                                .id("pr-review-tab-close")
                                .size(px(18.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(9999.0))
                                .cursor_pointer()
                                .role(gpui::Role::Button)
                                .aria_label(format!("Close {label} tab"))
                                .hover(move |style| style.bg(theme.control_hover))
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |view, cx| view.close_review_tab(cx));
                                    }
                                })
                                .child(
                                    icon("close-dialog", theme.text_muted.into()).size(px(12.0)),
                                ),
                        ),
                );
            }
        }

        let draft = self
            .detail
            .as_ref()
            .is_some_and(|detail| detail.summary.status == PullRequestStatus::Draft);
        let view = cx.entity();
        let mut actions = div().flex_none().flex().items_center().gap(px(4.0));
        if has_detail {
            let browser_view = view.clone();
            actions = actions.child(
                div()
                    .id("pr-open-browser")
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.5))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.control_hover))
                    .role(gpui::Role::Button)
                    .aria_label("Open in browser")
                    .on_click(move |_, _, cx| {
                        browser_view.update(cx, |view, cx| view.open_in_browser(cx));
                    })
                    .child(icon("pr-open-browser", theme.text.into()).size(px(18.0))),
            );
            let chat_view = view.clone();
            actions = actions.child(Self::header_pill("pr-chat", "Chat", theme, false).on_click(
                move |_, _, cx| {
                    chat_view.update(cx, |view, cx| view.open_chat(cx));
                },
            ));
            let merge_view = view.clone();
            actions = actions.child(
                div()
                    .id("pr-merge")
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(12.5))
                    .text_size(px(13.0))
                    .bg(theme.inverted_surface)
                    .text_color(theme.inverted_text)
                    .when(
                        !draft
                            && self.detail.is_some()
                            && !self.mutation_pending
                            && self
                                .selected
                                .as_ref()
                                .is_some_and(|pr| pr.status == PullRequestStatus::Open),
                        |button| {
                            button.cursor_pointer().on_click(move |_, _, cx| {
                                merge_view.update(cx, |view, cx| view.merge(cx));
                            })
                        },
                    )
                    .when(
                        draft
                            || self.detail.is_none()
                            || self.mutation_pending
                            || self
                                .selected
                                .as_ref()
                                .is_some_and(|pr| pr.status != PullRequestStatus::Open),
                        |button| button.opacity(0.4),
                    )
                    .role(gpui::Role::Button)
                    .aria_label(if self.mutation_pending {
                        "Saving to GitHub"
                    } else if self.detail.is_none() {
                        "Merge unavailable: details are loading"
                    } else if self.selected.as_ref().is_some_and(|pr| {
                        matches!(
                            pr.status,
                            PullRequestStatus::Closed | PullRequestStatus::Merged
                        )
                    }) {
                        "Merge unavailable: pull request is closed"
                    } else if draft {
                        "Merge unavailable: Mark as \"Ready for review\" to merge"
                    } else {
                        "Merge"
                    })
                    .child(icon("pr-merge", theme.inverted_text.into()).size(px(18.0)))
                    .child("Merge"),
            );
        }
        if has_detail {
            let fullscreen_view = view.clone();
            let fullscreen = self.fullscreen;
            actions = actions.child(
                div()
                    .id("pr-fullscreen")
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.5))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.control_hover))
                    .role(gpui::Role::Button)
                    .aria_label(if fullscreen {
                        "Exit full screen"
                    } else {
                        "Enter full screen"
                    })
                    .on_click(move |_, _, cx| {
                        fullscreen_view.update(cx, |view, cx| view.toggle_fullscreen(cx));
                    })
                    .child(icon("pr-fullscreen", theme.text.into()).size(px(18.0))),
            );
        }

        div()
            .flex_none()
            .h(px(TOOLBAR_HEIGHT))
            .px(px(16.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .when(has_detail, |header| {
                header.child(icon("pull-request", theme.text_muted.into()).size(px(18.0)))
            })
            .when(self.compact(), |header| {
                header.child(
                    div()
                        .id("pr-back-list")
                        .flex_none()
                        .px(px(6.0))
                        .role(gpui::Role::Button)
                        .aria_label("Back to pull requests")
                        .cursor_pointer()
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
        let mut column = div()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            // The detail column reserves the reference's scrollbar gutter on the
            // right (20px padding + an 11px gutter), so the description body is
            // 578px wide at a 1440px window and breaks at the same words.
            // The reference insets the scroll body 2px inside the pane padding
            // (its content column starts at 816.9, not 814.9).
            .pl(px(PANE_PADDING + 2.0))
            .pr(px(PANE_PADDING + SCROLLBAR_GUTTER))
            .pb(px(PANE_PADDING))
            .flex()
            .flex_col()
            .gap(px(24.0))
            .child(self.title_block(cx))
            .child(self.meta_rows(cx));

        // Description.
        let description_toggle = {
            let view = cx.entity();
            div()
                .id("pr-description-toggle")
                .h(px(28.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor_pointer()
                .role(gpui::Role::Button)
                .aria_label("Description")
                .on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| view.toggle_description(cx));
                })
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child("Description"),
                )
                .child(
                    icon("section-chevron", theme.text_muted.into())
                        .size(px(14.0))
                        .when(self.description_collapsed, |chevron| {
                            chevron.with_transformation(gpui::Transformation::rotate(
                                gpui::radians(-std::f32::consts::FRAC_PI_2),
                            ))
                        }),
                )
        };
        let mut description = div().flex().flex_col().gap(px(16.0)).child(
            div()
                .h(px(37.0))
                .pl(px(8.0))
                .pr(px(2.0))
                .pb(px(8.0))
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(description_toggle)
                .child(div().flex_1())
                .child(self.description_actions(cx)),
        );
        if !self.description_collapsed {
            description = description.child(if let Some(editor) = self.description_edit.clone() {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(Self::editor_frame(
                        editor,
                        280.0,
                        "pr-description-editor-frame",
                    ))
                    .child(self.description_edit_actions(cx))
            } else {
                div().px(px(8.0)).child(markdown)
            });
        }
        column = column.child(description);
        column = column.child(self.checks_section(cx));
        column = column.child(self.activity_section(cx));
        column = column.child(self.commits_section(cx));
        column = column.child(self.comment_composer(cx));
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
                    .child(column),
            )
            .into_any_element()
    }

    fn title_block(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let Some(detail) = self.detail.as_ref() else {
            return div();
        };
        let title = detail.summary.title.clone();
        let author = detail.author.login.clone();
        let age = detail.age.clone();
        let view = cx.entity();
        let mut title_row = div().flex().items_center().gap(px(12.0));
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
                    div()
                        .id("pr-title-cancel")
                        .size(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(12.5))
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.control_hover))
                        .role(gpui::Role::Button)
                        .aria_label("Cancel title editing")
                        .on_click(move |_, window, cx| {
                            cancel_view.update(cx, |view, cx| view.cancel_title_edit(window, cx));
                        })
                        .child(icon("close-dialog", theme.text_muted.into()).size(px(18.0))),
                )
                .child(
                    div()
                        .id("pr-title-save")
                        .size(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(12.5))
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.control_hover))
                        .role(gpui::Role::Button)
                        .aria_label("Save title")
                        .on_click(move |_, _, cx| {
                            save_view.update(cx, |view, cx| view.save_title(cx));
                        })
                        .child(icon("check", theme.text.into()).size(px(18.0))),
                );
        } else {
            title_row = title_row
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        // Reference: 24px title on a 1.2 line box (28.8px), so the
                        // wrapped second line sits 29px below the first.
                        .text_size(px(24.0))
                        .line_height(px(28.8))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(title),
                )
                .child(
                    div()
                        .id("pr-edit-title")
                        .size(px(28.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(12.5))
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.control_hover))
                        .role(gpui::Role::Button)
                        .aria_label("Edit title")
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |view, cx| view.begin_title_edit(cx));
                            }
                        })
                        .child(icon("pr-edit-title", theme.text_muted.into()).size(px(18.0))),
                );
        }
        // Reference: the title block carries only `padding: 16px 0 0`; the 8px
        // bottom inset belongs to the meta list below it.
        div()
            .pt(px(16.0))
            .px(px(8.0))
            .pb(px(4.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(title_row)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(14.0))
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .size(px(20.0))
                            .rounded(px(9999.0))
                            .bg(theme.control)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.0))
                            .text_color(theme.text)
                            .child(
                                author
                                    .chars()
                                    .next()
                                    .map(|letter| letter.to_uppercase().to_string())
                                    .unwrap_or_default(),
                            ),
                    )
                    .child(div().text_color(theme.text).child(author))
                    .child("·")
                    .child(age),
            )
    }

    fn meta_rows(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let Some(detail) = self.detail.as_ref() else {
            return div();
        };
        let mut list = div().flex().flex_col().px(px(8.0)).pb(px(8.0));

        let branch = div()
            // The reference's branch row is 28px tall inside a 4px-padded row
            // (36px total); the other meta rows carry 24 or 20px content.
            .h(px(28.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .max_w(px(260.0))
                    .text_ellipsis()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(detail.summary.head_branch.clone()),
            )
            .child(icon("settings-chevron-next", theme.text_muted.into()).size(px(14.0)))
            .child(
                div()
                    .max_w(px(120.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(detail.summary.base_branch.clone()),
            )
            // The reference right-aligns the change counts inside the row.
            .child(div().flex_1())
            .child({
                let view = cx.entity();
                let (additions, deletions) = (detail.summary.additions, detail.summary.deletions);
                div()
                    .id("pr-review-changes")
                    .flex_none()
                    .h(px(24.0))
                    .pl(px(6.0))
                    .pr(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(9999.0))
                    .text_size(px(13.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.control_hover))
                    .role(gpui::Role::Button)
                    .aria_label("Review pull request changes")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.open_review_tab(cx));
                    })
                    .child(
                        div()
                            .text_color(theme.additions_text)
                            .child(format!("+{additions}")),
                    )
                    .child(
                        div()
                            .text_color(theme.deletions_text)
                            .child(format!("-{deletions}")),
                    )
            });
        list = list.child(self.meta_row("branch", "Branch", theme, branch));

        let view = cx.entity();
        let reviewers: gpui::AnyElement = if detail.requested_reviewers.is_empty() {
            div()
                .id("pr-request-reviewers")
                .relative()
                .child(self.control_anchor("pr-request-reviewers"))
                .h(px(24.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(9999.0))
                .text_size(px(13.0))
                .cursor_pointer()
                .hover(move |style| style.bg(theme.control_hover))
                .role(gpui::Role::Button)
                .aria_label("Request reviewers")
                .on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| view.open_reviewers(cx));
                })
                .child("+ Request")
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .children(detail.requested_reviewers.iter().map(|user| {
                    div()
                        .h(px(24.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(9999.0))
                        .bg(theme.control)
                        .text_size(px(13.0))
                        .child(user.login.clone())
                }))
                .into_any_element()
        };
        list = list.child(self.meta_row("reviewers", "Reviewers", theme, reviewers));

        let comments = div()
            .h(px(20.0))
            .flex()
            .items_center()
            .text_size(px(14.0))
            .child(if detail.comments.is_empty() {
                "No comments".to_string()
            } else {
                format!("{} comments", detail.comments.len())
            });
        list = list.child(self.meta_row("comments", "Comments", theme, comments));

        let checks = div()
            .h(px(20.0))
            .flex()
            .items_center()
            .text_size(px(14.0))
            .child(if detail.checks.is_empty() {
                "No CI checks".to_string()
            } else {
                format!("{} checks", detail.checks.len())
            });
        list = list.child(self.meta_row("checks", "Checks", theme, checks));

        let status = detail.summary.status;
        let status_menu_view = cx.entity();
        let status_row = div()
            .id("pr-status")
            .relative()
            .child(self.control_anchor("pr-status"))
            .h(px(24.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(9999.0))
            .text_size(px(13.0))
            .text_color(theme.text)
            .when(!status.is_merged(), |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.control_hover))
                    .role(gpui::Role::Button)
                    .aria_label("Change pull request status")
                    .on_click(move |_, _, cx| {
                        status_menu_view.update(cx, |view, cx| view.toggle_status_menu(cx));
                    })
            })
            .child(status.label())
            .when(!status.is_merged(), |button| {
                button.child(icon("section-chevron", theme.text_muted.into()).size(px(12.0)))
            });
        list = list.child(self.meta_row("status", "Status", theme, status_row));
        list
    }

    fn meta_row(
        &self,
        _id: &'static str,
        label: &'static str,
        theme: PrTheme,
        content: impl IntoElement,
    ) -> Div {
        div()
            // Measured row padding: the reference stacks `dt` rows with 36/32/
            // 28/28/32px pitch, which is the row content plus 4px on each side.
            .py(px(4.0))
            .flex()
            .items_start()
            .gap(px(12.0))
            .child(
                div()
                    .w(px(120.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(14.0))
                    .text_color(theme.text_muted)
                    .child(icon(Self::meta_icon(label), theme.text_muted.into()).size(px(18.0)))
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(14.0))
                    .text_color(theme.text)
                    .child(content),
            )
    }

    /// Row icons extracted from the reference DOM (`scripts/extract_pull_request_icons.mjs`).
    fn meta_icon(label: &str) -> &'static str {
        match label {
            "Branch" => "pr-meta-branch",
            "Reviewers" => "pr-meta-reviewers",
            "Comments" => "pr-meta-comments",
            "Checks" => "pr-meta-checks",
            _ => "pr-meta-status",
        }
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
            if detail.checks.is_empty() {
                section = section.child(
                    div()
                        .px(px(8.0))
                        .text_size(px(14.0))
                        .text_color(theme.text_muted)
                        .child("No CI checks"),
                );
            } else {
                let mut rows = div().flex().flex_col().gap(px(4.0)).px(px(8.0));
                for check in &detail.checks {
                    let color = match check.state {
                        CheckState::Passed => theme.additions_text,
                        CheckState::Failed => theme.deletions_text,
                        _ => theme.text_muted,
                    };
                    rows = rows.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .child(icon("check", color.into()).size(px(16.0)))
                            .child(check.name.clone())
                            .child(
                                div()
                                    .text_color(theme.text_muted)
                                    .child(check.state.label()),
                            ),
                    );
                }
                section = section.child(rows);
            }
        }
        section
    }

    fn activity_section(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(detail) = self.detail.as_ref() else {
            return div();
        };
        let label = format!("Activity {}", detail.comments.len());
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.section_header_owned(
                label,
                self.activity_expanded,
                SectionKind::Activity,
                cx,
            ))
            .when(self.activity_expanded, |section| {
                section.child(self.activity_timeline(cx))
            })
    }

    fn commits_section(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let Some(detail) = self.detail.as_ref() else {
            return div();
        };
        if detail.commits.is_empty() {
            return div();
        }
        let label = format!("{} commits", detail.commits.len());
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.section_header_owned(
                label,
                self.commits_expanded,
                SectionKind::Commits,
                cx,
            ));
        if self.commits_expanded {
            let mut rows = div().flex().flex_col().gap(px(6.0)).px(px(8.0));
            for commit in &detail.commits {
                let sha = commit.sha.clone();
                let url = format!(
                    "https://github.com/{}/commit/{}",
                    detail.summary.repository, commit.sha
                );
                rows = rows.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(14.0))
                        .child(
                            div()
                                .id(SharedString::from(format!("pr-commit-{sha}")))
                                .font_family(crate::theme::UI_MONOSPACE_FONT_FAMILY)
                                .text_size(px(13.0))
                                .text_color(theme.text)
                                .cursor_pointer()
                                .role(gpui::Role::Link)
                                .aria_label(format!("Commit {}", commit.short_sha()))
                                .on_click(move |_, _, cx| cx.open_url(&url))
                                .child(commit.short_sha().to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(commit.subject.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .child(commit.age.clone()),
                        ),
                );
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
        self.section_header_owned(label.to_string(), expanded, section, cx)
    }

    fn section_header_owned(
        &self,
        label: String,
        expanded: bool,
        section: SectionKind,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        div()
            .id(SharedString::from(format!("pr-section-{label}")))
            .h(px(28.0))
            .pl(px(8.0))
            .pr(px(2.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .cursor_pointer()
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(label.clone()))
            .on_click(move |_, _, cx| {
                view.update(cx, |view, cx| match section {
                    SectionKind::Checks => view.toggle_checks(cx),
                    SectionKind::Activity => view.toggle_activity(cx),
                    SectionKind::Commits => view.toggle_commits(cx),
                });
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(16.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(label.clone())
                    .child(
                        icon("section-chevron", theme.text_muted.into())
                            .size(px(14.0))
                            .when(!expanded, |chevron| {
                                chevron.with_transformation(gpui::Transformation::rotate(
                                    gpui::radians(-std::f32::consts::FRAC_PI_2),
                                ))
                            }),
                    ),
            )
    }

    fn comment_composer(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let has_text =
            !self.mutation_pending && !self.comment_box.read(cx).text().trim().is_empty();
        // The reference draws the composer as one bordered card: the editor on
        // top and a footer with the author avatar and a round send button.
        div()
            .rounded(px(16.0))
            .border(px(1.0))
            .border_color(theme.field_border)
            .bg(theme.surface)
            .pt(px(10.0))
            .px(px(12.0))
            .pb(px(12.0))
            .flex_col()
            .child(Self::editor_frame(
                self.comment_box.clone(),
                96.0,
                "pr-comment-composer-frame",
            ))
            .child(
                div()
                    .mt(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .size(px(28.0))
                            .rounded(px(9999.0))
                            .bg(theme.control)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(13.0))
                            .text_color(theme.text)
                            .child(
                                self.detail
                                    .as_ref()
                                    .and_then(|detail| detail.author.login.chars().next())
                                    .map(|letter| letter.to_uppercase().to_string())
                                    .unwrap_or_default(),
                            ),
                    )
                    .child(
                        div()
                            .id("pr-post-comment")
                            .h(px(28.0))
                            .w(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(9999.0))
                            .text_size(px(13.0))
                            .when(has_text, |button| {
                                button
                                    .bg(theme.inverted_surface)
                                    .text_color(theme.inverted_text)
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |view, cx| view.post_comment(cx));
                                    })
                            })
                            .when(!has_text, |button| {
                                button
                                    .bg(theme.control)
                                    .text_color(theme.text_muted)
                                    .opacity(0.6)
                            })
                            .role(gpui::Role::Button)
                            .aria_label("Post comment")
                            .child(icon("pr-send", theme.inverted_text.into()).size(px(16.0))),
                    ),
            )
    }

    pub(super) fn summary_scope_label(&self) -> String {
        match self
            .review_tab
            .as_ref()
            .map(|tab| &tab.scope)
            .unwrap_or(&ReviewScope::AllChanges)
        {
            ReviewScope::AllChanges => "All PR changes".to_string(),
            ReviewScope::Commit(sha) => format!("Commit {}", sha.get(..7).unwrap_or(sha.as_str())),
        }
    }
}
