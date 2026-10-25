//! Right pane: header tabs, the `Summary` surface, and the comment composer.

use gpui::{Div, SharedString, div, prelude::*, px};

use super::render::TooltipPlacement;
use super::{DetailTab, PullRequestsView, ReviewScope, theme::*};
use crate::components::icons::icon;
use crate::pull_requests::{CheckState, PullRequestStatus};
use crate::theme::Theme;

fn status_color(status: PullRequestStatus, issue: bool, theme: PrTheme) -> gpui::Rgba {
    match status {
        PullRequestStatus::Merged => theme.purple,
        PullRequestStatus::Closed if issue => theme.purple,
        PullRequestStatus::Closed => theme.chart_red,
        PullRequestStatus::Open => theme.chart_green,
        PullRequestStatus::Draft => theme.text_muted,
    }
}

fn item_state_glyph(
    summary: &crate::pull_requests::PullRequestSummary,
    theme: PrTheme,
) -> gpui::AnyElement {
    if summary.is_issue() {
        icon(
            if summary.status == PullRequestStatus::Closed {
                "review-issue-closed"
            } else {
                "review-issue"
            },
            status_color(summary.status, true, theme).into(),
        )
        .size(px(14.0))
        .into_any_element()
    } else {
        PullRequestsView::state_glyph(summary.status, theme, 14.0).into_any_element()
    }
}

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
            .when(
                self.detail.is_some()
                    && !self.detail_loading
                    && self.detail_tab != DetailTab::Code
                    && self.review_tab.is_none(),
                |s| s.child(self.question_composer(cx)),
            )
            .children(self.detail_overlays(cx))
    }

    fn question_composer(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let ready = !self.question.read(cx).text().trim().is_empty();
        let view = cx.entity();
        div()
            .flex_none()
            .px(px(16.0))
            .pb(px(12.0))
            .flex()
            .justify_end()
            .child(
                div()
                    .w_full()
                    .max_w(px(640.0))
                    .p(px(12.0))
                    .rounded(px(20.0))
                    .bg(theme.composer_surface)
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(icon("folder", theme.text_muted.into()).size(px(14.0)))
                            .child(
                                self.selected
                                    .as_ref()
                                    .map(|s| {
                                        s.repository.rsplit('/').next().unwrap_or("").to_owned()
                                    })
                                    .unwrap_or_default(),
                            )
                            .child(
                                icon("local", theme.text_muted.into())
                                    .ml(px(8.0))
                                    .size(px(14.0)),
                            )
                            .child("Local"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(div().min_w(px(0.0)).flex_1().child(self.question.clone()))
                            .child(
                                self.inbox_button("pr-ask-agent", "Ask in chat")
                                    .size(px(28.0))
                                    .px(px(0.0))
                                    .rounded_full()
                                    .aria_disabled(!ready)
                                    .tab_stop(ready)
                                    .bg(theme.inverted_surface)
                                    .when(!ready, |s| s.opacity(0.3))
                                    .child(
                                        icon("pr-arrow-up", theme.inverted_text.into())
                                            .size(px(16.0)),
                                    )
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |v, cx| v.ask_about_item(cx))
                                    }),
                            ),
                    ),
            )
    }

    fn detail_header(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        if self.review_tab.is_some() && self.selected.is_some() {
            return self.review_tab_strip(cx).into_any_element();
        }
        let theme = self.theme();
        let mut tabs = div()
            .id("pr-detail-tabs")
            .role(gpui::Role::TabList)
            .aria_label("Pull request detail tabs")
            .overflow_x_scroll()
            .flex()
            .min_w(px(0.0))
            .items_center()
            .gap(px(2.0));
        if self.compact() {
            let back = cx.entity();
            tabs = tabs.child(
                self.inbox_button("pr-back-list", "Back to code review list")
                    .size(px(28.0))
                    .px(px(0.0))
                    .child(
                        icon("pr-chevron-right", theme.text_muted.into())
                            .size(px(16.0))
                            .with_transformation(gpui::Transformation::rotate(gpui::radians(
                                std::f32::consts::PI,
                            ))),
                    )
                    .on_click(move |_, _, cx| {
                        back.update(cx, |v, cx| {
                            v.selected = None;
                            v.detail = None;
                            v.detail_generation += 1;
                            cx.notify();
                        })
                    }),
            );
        }
        if let Some(item) = &self.selected {
            for (tab, label) in [
                (DetailTab::Summary, "Summary"),
                (DetailTab::Code, "Changes"),
                (DetailTab::Timeline, "Timeline"),
            ] {
                if item.is_issue() && tab == DetailTab::Code {
                    continue;
                }
                let view = cx.entity();
                let selected = self.detail_tab == tab;
                tabs = tabs.child(
                    self.inbox_button(SharedString::from(format!("pr-detail-tab-{label}")), label)
                        .role(gpui::Role::Tab)
                        .aria_selected(selected)
                        .when(selected, |s| s.bg(theme.control).text_color(theme.text))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |v, cx| v.set_detail_tab(tab, cx))
                        })
                        .child(label)
                        .when(tab == DetailTab::Code && self.pane_width >= 800.0, |s| {
                            s.child(Self::diff_stats(
                                item.additions,
                                item.deletions,
                                theme.additions_text,
                                theme.deletions_text,
                                11.0,
                            ))
                        }),
                );
            }
        }
        let mut actions = div().flex_none().flex().items_center().gap(px(6.0));
        if let Some(item) = &self.selected {
            let pinned = self.pinned_items.contains(&item.url);
            for (id, label, glyph) in [
                (
                    "pr-pin-detail",
                    format!(
                        "{} {} #{}",
                        if pinned { "Unpin" } else { "Pin" },
                        if item.is_issue() {
                            "issue"
                        } else {
                            "pull request"
                        },
                        item.number
                    ),
                    if pinned {
                        "review-pin-filled"
                    } else {
                        "review-pin"
                    },
                ),
                ("pr-copy-link", "Copy link".into(), "review-link"),
                (
                    "pr-open-browser",
                    "Open on GitHub".into(),
                    "review-open-browser",
                ),
            ] {
                let view = cx.entity();
                let row = item.clone();
                let button = self
                    .inbox_button(id, SharedString::from(label.clone()))
                    .size(px(28.0))
                    .px(px(0.0))
                    .child(icon(glyph, theme.text_muted.into()).size(px(16.0)))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |v, cx| match id {
                            "pr-pin-detail" => v.toggle_pin(&row, cx),
                            "pr-copy-link" => {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    row.url.clone(),
                                ));
                                v.show_notice("Link copied".into(), cx);
                            }
                            _ => v.open_in_browser(cx),
                        })
                    });
                actions = actions.child(self.with_tooltip(
                    button,
                    id,
                    label,
                    TooltipPlacement::Below,
                    cx,
                ));
            }
            if !item.is_issue()
                && matches!(
                    item.status,
                    PullRequestStatus::Open | PullRequestStatus::Draft
                )
            {
                let view = cx.entity();
                let draft = item.status == PullRequestStatus::Draft;
                let enabled = self.detail.as_ref().is_some_and(|d| d.mergeable)
                    && !self.mutation_pending
                    && !draft;
                let label = if draft { "Ready for review" } else { "Merge" };
                actions = actions.child(
                    self.inbox_button("pr-merge", label)
                        .bg(theme.inverted_surface)
                        .text_color(theme.inverted_text)
                        .aria_disabled(!enabled && !draft)
                        .when(!enabled && !draft, |s| s.opacity(0.5))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |v, cx| {
                                if draft {
                                    v.set_status(PullRequestStatus::Open, cx);
                                } else if enabled {
                                    v.merge(cx);
                                }
                            })
                        })
                        .child(icon("pr-merge", theme.inverted_text.into()).size(px(14.0)))
                        .when(self.pane_width >= 700.0, |s| s.child(label)),
                );
            }
            let send = cx.entity();
            let more = cx.entity();
            actions = actions
                .child(
                    self.inbox_button("pr-chat", "Send to agent")
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .gap(px(8.0))
                        .border(px(1.0))
                        .border_color(theme.border)
                        .aria_disabled(self.detail.is_none())
                        .when(self.detail.is_none(), |s| s.opacity(0.5))
                        .child(icon("review-send", theme.text.into()).size(px(16.0)))
                        .when(self.pane_width >= 736.0, |s| s.child("Send to agent"))
                        .on_click(move |_, _, cx| {
                            send.update(cx, |v, cx| {
                                if v.detail.is_some() {
                                    v.open_chat(cx);
                                }
                            })
                        }),
                )
                .child(
                    self.with_tooltip(
                        self.inbox_button("pr-detail-more", "More actions")
                            .relative()
                            .child(self.control_anchor("pr-detail-more"))
                            .w(px(30.0))
                            .px(px(0.0))
                            .child(icon("review-more", theme.text_muted.into()).size(px(16.0)))
                            .on_click(move |_, _, cx| {
                                more.update(cx, |v, cx| {
                                    v.detail_actions_open = !v.detail_actions_open;
                                    cx.notify();
                                })
                            }),
                        "pr-detail-more",
                        "More actions",
                        TooltipPlacement::Below,
                        cx,
                    ),
                );
        }
        div()
            .flex_none()
            .pt(px(44.0))
            .child(
                div()
                    .h(px(64.0))
                    .px(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(self.titlebar_spacer())
                    .child(tabs)
                    .child(div().flex_1())
                    .child(actions),
            )
            .into_any_element()
    }

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
        // The task tab names its source: `host/owner/repo #N — title (local)`.
        let tab_tooltip = self
            .detail
            .as_ref()
            .map(|detail| {
                format!(
                    "github.com/{} #{} \u{2014} {title} (local)",
                    detail.summary.repository.to_lowercase(),
                    detail.summary.number
                )
            })
            .unwrap_or_else(|| title.clone());
        div()
            .flex_none()
            .h(px(TOOLBAR_HEIGHT))
            .pl(px(8.0))
            .pr(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .children(self.titlebar_spacer())
            .child(
                div().flex_1().min_w(px(0.0)).flex().items_center().child(
                    div()
                        .id("pr-review-tab")
                        .debug_selector(|| "pr-review-tab".into())
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
                        .text_size(px(crate::typography::ui_size(13.0)))
                        .line_height(px(crate::typography::ui_leading(13.0, 18.5714)))
                        .text_color(theme.text)
                        .child(
                            icon("panel-review", theme.text.into())
                                .flex_none()
                                .size(px(16.0)),
                        )
                        // `-ms-1 text-fade-truncate`: the title sits 4px from
                        // the glyph and fades out instead of an ellipsis.
                        .child(
                            self.with_tooltip(
                                div()
                                    .id("pr-review-tab-title")
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
                                "pr-review-tab-title",
                                tab_tooltip,
                                TooltipPlacement::Below,
                                cx,
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
                self.fullscreen_button(
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
                    cx,
                ),
            )
    }

    /// The full screen toggle with its tooltip below.
    fn fullscreen_button(
        &self,
        button: gpui::Stateful<Div>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let label = if self.fullscreen {
            "Exit full screen"
        } else {
            "Enter full screen"
        };
        self.with_tooltip(button, "pr-fullscreen", label, TooltipPlacement::Below, cx)
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
                .text_size(px(crate::typography::ui_size(14.0)))
                .text_color(theme.text)
                .flex_col()
                .gap(px(12.0))
                .child(icon("pr-status-open", theme.text_muted.into()).size(px(24.0)))
                .child("Select a pull request or issue")
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text_muted)
                        .child("Choose one from the sidebar to review it"),
                )
                .into_any_element();
        }
        if self.detail_loading {
            return div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(crate::typography::ui_size(13.0)))
                .text_color(theme.text_muted)
                .child("Loading item details…")
                .into_any_element();
        }
        if let Some(error) = self.detail_error.clone() {
            return div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(crate::typography::ui_size(13.0)))
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
        let body_width =
            self.pane_width - 1.0 - SUMMARY_OUTER_PADDING * 2.0 - self.scrollbar_gutter();
        let columns = body_width >= SUMMARY_COLUMNS_MIN_WIDTH;
        // The main column takes the space left by Synara's fixed metadata
        // width; cap the complete frame so both columns stay together.
        let mut column = div()
            .id("pr-summary-content")
            .role(gpui::Role::Group)
            .aria_label("Code review content")
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.title_block(cx));

        if self.detail_tab == DetailTab::Timeline {
            column = column.child(self.activity_section(cx));
        }
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
                    .text_size(px(crate::typography::ui_size(14.0)))
                    .line_height(px(crate::typography::ui_leading(14.0, 20.0)))
                    .text_color(theme.text_muted)
                    .child("No description provided")
            } else {
                div().px(px(8.0)).child(markdown)
            });
        }
        if self.detail_tab != DetailTab::Timeline {
            column = column.child(description);
            if !detail.summary.is_issue() {
                column = column.child(self.checks_section(cx));
            }
            column = column.child(self.activity_section(cx));
        }
        column = column.child(div().px(px(8.0)).child(self.comment_composer(cx)));
        let metadata = div()
            .id("pr-summary-metadata")
            .role(gpui::Role::Group)
            .aria_label("Code review metadata")
            .flex_none()
            .when(columns, |s| {
                s.w(px(SUMMARY_INFO_WIDTH)).h_full().overflow_y_scroll()
            })
            .when(!columns, |s| s.w_full())
            .pt(px(16.0))
            .border_l(px(if columns { 1.0 } else { 0.0 }))
            .border_color(theme.border)
            .pl(px(16.0))
            .child(self.meta_rows(cx));
        let capture_offset = (self.capture_offset > 0.0).then_some(self.capture_offset);
        let mut content = div()
            .when_some(capture_offset, |s, offset| s.relative().top(px(-offset)))
            .pb(px(PANE_PADDING))
            .child(column);
        let mut metadata = Some(metadata);
        if !columns {
            content = content.children(metadata.take());
        }
        div()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .w_full()
            .max_w(px(SUMMARY_MAX_WIDTH))
            .mx_auto()
            .px(px(SUMMARY_OUTER_PADDING + SUMMARY_INNER_PADDING))
            .flex()
            .gap(px(SUMMARY_COLUMN_GAP))
            .child(
                div()
                    .id("pr-detail-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .min_w(px(0.0))
                    .h_full()
                    .pr(px(self.scrollbar_gutter()))
                    .overflow_y_scroll()
                    .track_scroll(&self.detail_scroll)
                    .child(content),
            )
            .children(metadata)
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
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(px(crate::typography::ui_size(12.0)))
                                .line_height(px(18.0))
                                .text_color(theme.text_muted)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded_full()
                                        .bg(status_color(
                                            detail.summary.status,
                                            detail.summary.is_issue(),
                                            theme,
                                        )
                                        .alpha(0.10))
                                        .text_color(status_color(
                                            detail.summary.status,
                                            detail.summary.is_issue(),
                                            theme,
                                        ))
                                        .child(item_state_glyph(&detail.summary, theme))
                                        .child(detail.summary.status.label()),
                                )
                                .child(format!(
                                    "{} #{}",
                                    detail.summary.repository, detail.summary.number
                                )),
                        )
                        .child(
                            div()
                                .text_size(px(crate::typography::ui_size(24.0)))
                                .line_height(px(crate::typography::ui_leading(24.0, 28.8)))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
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
                                .text_size(px(crate::typography::ui_size(12.0)))
                                .line_height(px(crate::typography::ui_leading(14.0, 21.0)))
                                .text_color(theme.icon_muted)
                                .child(self.avatar(detail.author.avatar_url.as_deref(), 16.0))
                                .child(author)
                                .when(!age.is_empty(), |line| {
                                    line.child("·").child(format!("{age} ago"))
                                })
                                .when(!detail.summary.is_issue(), |line| {
                                    line.child("·")
                                        .child(
                                            div()
                                                .min_w(px(0.0))
                                                .truncate()
                                                .child(detail.summary.head_branch.clone()),
                                        )
                                        .child("→")
                                        .child(
                                            div()
                                                .min_w(px(0.0))
                                                .truncate()
                                                .child(detail.summary.base_branch.clone()),
                                        )
                                }),
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
        let mut list = div()
            .id("pr-information-group")
            .role(gpui::Role::Group)
            .aria_label("Details")
            .flex()
            .flex_col()
            .gap(px(16.0))
            .pb(px(8.0));
        let mut assignees = div().flex().flex_col().gap(px(6.0));
        for user in &detail.assignees {
            assignees = assignees.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.avatar(user.avatar_url.as_deref(), 18.0))
                    .child(user.login.clone()),
            );
        }
        if detail.assignees.is_empty() {
            assignees = assignees.child("Unassigned");
        }
        list = list.child(self.meta_row(
            "Assignees",
            Self::meta_label_icon("pr-meta-reviewers", theme),
            theme,
            assignees,
        ));
        let mut labels = div().flex().flex_wrap().gap(px(4.0));
        for (name, color) in &detail.labels {
            let rgb = u32::from_str_radix(color, 16).unwrap_or(0x808080);
            labels = labels.child(
                div()
                    .px(px(7.0))
                    .py(px(2.0))
                    .rounded_full()
                    .bg(gpui::rgba((rgb << 8) | 0x22))
                    .text_color(theme.text)
                    .child(name.clone()),
            );
        }
        if detail.labels.is_empty() {
            labels = labels.child("None");
        }
        list = list.child(self.meta_row(
            "Labels",
            Self::meta_label_icon("pr-menu-status", theme),
            theme,
            labels,
        ));

        let (additions, deletions) = (detail.summary.additions, detail.summary.deletions);
        let changes = cx.entity();
        let branch = div()
            .min_w(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .id("pr-branch-names")
                    .w_full()
                    .overflow_x_scroll()
                    .text_size(px(crate::typography::ui_size(12.0)))
                    .child(format!(
                        "{} → {}",
                        detail.summary.head_branch, detail.summary.base_branch
                    )),
            )
            .child(
                Self::toolbar_button("pr-review-changes", theme, false)
                    .aria_label("Review pull request changes")
                    .on_click(move |_, _, cx| changes.update(cx, |v, cx| v.open_review_tab(cx)))
                    .child(Self::diff_stats(
                        additions,
                        deletions,
                        theme.additions_text,
                        theme.deletions_text,
                        13.0,
                    )),
            );
        if !detail.summary.is_issue() {
            list = list.child(self.meta_row(
                "Branch",
                Self::meta_label_icon("pr-meta-branch", theme),
                theme,
                branch,
            ));
        }

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
        if !detail.summary.is_issue() {
            list = list.child(self.meta_row(
                "Reviewers",
                Self::meta_label_icon("pr-meta-reviewers", theme),
                theme,
                reviewers,
            ));
        }

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
        if !detail.summary.is_issue() {
            list = list.child(self.meta_row(
                "Checks",
                Self::meta_label_icon("pr-meta-checks", theme),
                theme,
                checks,
            ));
        }

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
                .text_size(px(crate::typography::ui_size(14.0)))
                .aria_label(if detail.summary.is_issue() {
                    "Change issue status"
                } else {
                    "Change pull request status"
                })
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
        div().child(list)
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
            .flex()
            .flex_col()
            .gap(px(6.0))
            .text_size(px(crate::typography::ui_size(14.0)))
            .line_height(px(crate::typography::ui_leading(14.0, 20.0)))
            .child(
                div()
                    .w_full()
                    .text_size(px(crate::typography::ui_size(12.0)))
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
                    .text_size(px(crate::typography::ui_size(13.0)))
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
                    .text_size(px(crate::typography::ui_size(13.0)))
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
                        .text_size(px(crate::typography::ui_size(14.0)))
                        .line_height(px(crate::typography::ui_leading(14.0, 20.0)))
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
                            .text_size(px(crate::typography::ui_size(14.0)))
                            .line_height(px(crate::typography::ui_leading(14.0, 20.0)))
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
                    .text_size(px(crate::typography::ui_size(16.0)))
                    .line_height(px(crate::typography::ui_leading(16.0, 24.0)))
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
