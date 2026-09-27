//! The Summary tab's `Activity` feed.
//!
//! Every card follows the reference's timeline components
//! (`pull-request-detail-query`, `pull-request-readonly-comment`): a
//! `rounded-lg border border-default bg-primary-soft-alpha` card per item,
//! commit groups that expand in place, `opened`/`merged`/review events, and
//! comment cards whose header toggles the body, whose permalink and chevron
//! appear on hover, and whose review threads show the commented diff above the
//! body with `Reply` and `Resolve` below it.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{Div, SharedString, div, prelude::*, px};

use super::{DetailTab, PullRequestsView, diff::file_name, theme::*};
use crate::components::icons::icon;
use crate::components::markdown::{MarkdownLineClamp, PULL_REQUEST_COMMENT_LINE_HEIGHT};
use crate::pull_requests::{
    ActivityEventKind, ActivityItem, Comment, Commit, PullRequestDetail, ReviewThread,
};
use crate::theme::{Theme, UI_MONOSPACE_FONT_FAMILY};

/// Lines a comment body shows before `Show more` (`line-clamp-6`).
const CLAMP_LINES: usize = 6;
/// Card radius (`rounded-lg`).
const CARD_RADIUS: f32 = 12.5;
/// The diff viewer inside a review thread: 12px code on 21.6px rows with a
/// 78px line-number gutter.
const PREVIEW_LINE_HEIGHT: f32 = 21.6;
const PREVIEW_GUTTER_WIDTH: f32 = 78.12;
const PREVIEW_CELL_PADDING: f32 = 7.22461;

/// Heights measured from the last unclamped frame, keyed by comment id: the
/// clamp needs the laid-out height of each top-level Markdown block.
pub(super) type BodyMeasurements = Rc<RefCell<HashMap<String, Vec<(f32, f32)>>>>;

impl PullRequestsView {
    pub(super) fn activity_section(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(detail) = self.detail.as_ref() else {
            return div();
        };
        let expanded = self.activity_expanded;
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.section_header_owned(
                "Activity".into(),
                Some(detail.activity.len()),
                expanded,
                super::detail::SectionKind::Activity,
                cx,
            ))
            .when(expanded, |section| {
                section.child(
                    div()
                        .px(px(8.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(self.activity_list(detail, cx)),
                )
            })
    }

    fn activity_list(&self, detail: &PullRequestDetail, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        if detail.activity.is_empty() {
            return div()
                .min_h(px(30.0))
                .py(px(5.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(theme.text_muted)
                .child("No activity");
        }
        let mut list = div().flex().flex_col().gap(px(12.0));
        for (index, item) in detail.activity.iter().enumerate() {
            list = list.child(match item {
                ActivityItem::Commits { commits } => {
                    self.commit_group(index, commits, &detail.summary.repository, cx)
                }
                ActivityItem::Event {
                    kind, actor, age, ..
                } => Self::event_card(*kind, actor, age, theme),
                ActivityItem::Comment { id, .. } => match detail.comment(id) {
                    Some(comment) => self.comment_card(comment, None, cx),
                    None => div(),
                },
                ActivityItem::Thread { id, .. } => match detail.thread(id) {
                    Some(thread) => match thread.comments.first() {
                        Some(first) => self.comment_card(first, Some(thread), cx),
                        None => div(),
                    },
                    None => div(),
                },
            });
        }
        list
    }

    fn card(theme: PrTheme) -> Div {
        div()
            .rounded(px(CARD_RADIUS))
            .border(px(1.0))
            .border_color(theme.border)
            .bg(theme.soft_alpha)
    }

    /// The 24px round well holding an 18px glyph.
    fn icon_well(glyph: impl IntoElement, theme: PrTheme) -> Div {
        div()
            .flex_none()
            .size(px(24.0))
            .rounded_full()
            .bg(theme.soft_alpha)
            .flex()
            .items_center()
            .justify_center()
            .child(glyph)
    }

    fn event_card(kind: ActivityEventKind, actor: &str, age: &str, theme: PrTheme) -> Div {
        let actor = if actor.is_empty() { "Someone" } else { actor };
        let (name, color, label) = match kind {
            ActivityEventKind::Opened => (
                "pr-status-open",
                theme.chart_green,
                format!("{actor} opened this pull request"),
            ),
            ActivityEventKind::Merged => (
                "pr-status-merged",
                theme.purple,
                format!("{actor} merged this pull request"),
            ),
            ActivityEventKind::Approved => (
                "pr-event-approved",
                theme.chart_green,
                format!("{actor} approved these changes"),
            ),
            ActivityEventKind::ChangesRequested => (
                "pr-event-changes-requested",
                theme.chart_red,
                format!("{actor} requested changes"),
            ),
        };
        Self::card(theme)
            .px(px(12.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(Self::icon_well(
                icon(name, color.into()).size(px(18.0)),
                theme,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(label),
            )
            .child(Self::age_label(age, theme))
    }

    fn age_label(age: &str, theme: PrTheme) -> Div {
        div()
            .flex_none()
            .text_size(px(13.0))
            .line_height(px(18.5714))
            .text_color(theme.text_muted)
            .child(age.to_string())
    }

    /// Consecutive commits: one commit is a standalone row, more fold into a
    /// `N commits` summary that expands the rows below a rule.
    fn commit_group(
        &self,
        index: usize,
        commits: &[Commit],
        repository: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let theme = self.theme();
        if let [commit] = commits {
            return self.commit_row(commit, repository, false, theme);
        }
        let open = self.expanded_commit_groups.contains(&index);
        let view = cx.entity();
        let age = commits
            .first()
            .map(|commit| commit.age.clone())
            .unwrap_or_default();
        let label = format!("{} commits", commits.len());
        let mut card = Self::card(theme).flex().flex_col().child(
            div()
                .id(SharedString::from(format!("pr-commit-group-{index}")))
                .group("pr-commit-group")
                .px(px(12.0))
                .py(px(10.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .role(gpui::Role::Button)
                .aria_label(SharedString::from(label.clone()))
                .aria_expanded(open)
                .on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| view.toggle_commit_group(index, cx));
                })
                .child(Self::icon_well(
                    icon("pr-commit", theme.icon_muted.into()).size(px(18.0)),
                    theme,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(label)
                        .child(
                            icon("pr-chevron-right", theme.text_muted.into())
                                .size(px(14.0))
                                .opacity(0.0)
                                .group_hover("pr-commit-group", |style| style.opacity(1.0))
                                .when(open, |chevron| {
                                    chevron.with_transformation(gpui::Transformation::rotate(
                                        gpui::radians(std::f32::consts::FRAC_PI_2),
                                    ))
                                }),
                        ),
                )
                .child(Self::age_label(&age, theme)),
        );
        if open {
            let mut rows = div()
                .flex()
                .flex_col()
                .border_t(px(1.0))
                .border_color(theme.border);
            for commit in commits {
                rows = rows.child(self.commit_row(commit, repository, true, theme));
            }
            card = card.child(rows);
        }
        card
    }

    fn commit_row(&self, commit: &Commit, repository: &str, grouped: bool, theme: PrTheme) -> Div {
        let url = format!("https://github.com/{repository}/commit/{}", commit.sha);
        let short = commit.short_sha().to_string();
        let avatar = commit.avatar_url();
        let row = if grouped { div() } else { Self::card(theme) };
        row.min_w(px(0.0))
            .px(px(12.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(Self::icon_well(
                icon("pr-commit", theme.icon_muted.into()).size(px(18.0)),
                theme,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(commit.subject.clone()),
            )
            .child(
                div()
                    .id(SharedString::from(format!("pr-commit-{}", commit.sha)))
                    .flex_none()
                    .font_family(UI_MONOSPACE_FONT_FAMILY)
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .text_color(theme.icon_muted)
                    .role(gpui::Role::Link)
                    .aria_label(SharedString::from(format!("Commit {short}")))
                    .hover(|style| style.underline())
                    .on_click(move |_, _, cx| cx.open_url(&url))
                    .child(short),
            )
            .when(!commit.author.is_empty(), |row| {
                row.child(
                    div()
                        .flex_none()
                        .size(px(20.0))
                        .rounded_full()
                        .overflow_hidden()
                        .bg(theme.surface_secondary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .map(|well| match avatar.as_deref() {
                            Some(url) => well.child(self.avatar(Some(url), 20.0)),
                            None => well.child(
                                commit
                                    .author
                                    .chars()
                                    .next()
                                    .map(|letter| letter.to_uppercase().to_string())
                                    .unwrap_or_default(),
                            ),
                        }),
                )
            })
            .child(
                div()
                    .flex_none()
                    .w(px(36.0))
                    .flex()
                    .justify_end()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .font_features(super::list::stats_font_features())
                    .text_color(theme.text_muted)
                    .child(commit.age.clone()),
            )
    }

    /// A comment or the first comment of a review thread. Bots and resolved
    /// threads start collapsed; the header toggles the body.
    fn comment_card(
        &self,
        comment: &Comment,
        thread: Option<&ReviewThread>,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let theme = self.theme();
        let id = comment.id.clone();
        let collapsed = self.comment_collapsed(comment, thread);
        let editing = self
            .comment_edit
            .as_ref()
            .is_some_and(|(editing_id, _)| editing_id == &id);
        let mut card = Self::card(theme)
            .id(SharedString::from(format!("pr-comment-{id}")))
            .group("pr-comment")
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(self.comment_header(comment, collapsed, cx));
        if collapsed {
            return div().child(card);
        }
        if let Some(thread) = thread {
            card = card.child(self.thread_preview(thread, comment, cx));
        }
        if editing {
            let (_, editor) = self.comment_edit.clone().unwrap();
            card = card.child(
                div()
                    .px(px(12.0))
                    .pt(px(4.0))
                    .pb(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(Self::editor_frame(editor, 160.0, "pr-comment-editor-frame"))
                    .child(self.comment_edit_actions(cx)),
            );
        } else if !comment.body.trim().is_empty() {
            card = card.child(self.comment_body(comment, thread.is_none(), cx));
        }
        if let Some(thread) = thread {
            for reply in thread.comments.iter().skip(1) {
                card = card.child(self.thread_reply(reply, cx));
            }
            card = card.child(self.comment_reply_actions(&id, Some(thread.id.clone()), cx));
        }
        div().child(card)
    }

    /// Default collapse (`isResolved || authorType !== "User"`) flipped by the
    /// user's toggles.
    pub(super) fn comment_collapsed(
        &self,
        comment: &Comment,
        thread: Option<&ReviewThread>,
    ) -> bool {
        let default = comment.author_is_bot || thread.is_some_and(|thread| thread.resolved);
        default != self.collapsed_comments.contains(&comment.id)
    }

    /// `px-3 pt-2`: the avatar and the author button (the chevron shows on
    /// hover), then the permalink (hover only), age, and actions.
    fn comment_header(
        &self,
        comment: &Comment,
        collapsed: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        let id = comment.id.clone();
        let menu_open = self.comment_menu.as_deref() == Some(id.as_str());
        let view = cx.entity();
        let toggle_view = view.clone();
        let toggle_id = id.clone();
        let label = if collapsed {
            format!("Expand comment by {}", comment.author)
        } else {
            format!("Collapse comment by {}", comment.author)
        };
        div()
            .id(SharedString::from(format!("pr-comment-header-{id}")))
            .px(px(12.0))
            .pt(px(8.0))
            .pb(px(if collapsed { 8.0 } else { 2.0 }))
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .on_click(move |_, _, cx| {
                let id = toggle_id.clone();
                toggle_view.update(cx, |view, cx| view.toggle_comment_collapsed(id, cx));
            })
            .child(
                div()
                    .min_h(px(24.0))
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(self.avatar(comment.avatar_url.as_deref(), 24.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("pr-comment-author-{id}")))
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(theme.text_muted)
                            .role(gpui::Role::Button)
                            .aria_label(SharedString::from(label))
                            .aria_expanded(!collapsed)
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(comment.author.clone()),
                            )
                            .child(
                                icon("pr-chevron-right", theme.text_muted.into())
                                    .flex_none()
                                    .size(px(14.0))
                                    .opacity(0.0)
                                    .group_hover("pr-comment", |style| style.opacity(1.0))
                                    .when(!collapsed, |chevron| {
                                        chevron.with_transformation(gpui::Transformation::rotate(
                                            gpui::radians(std::f32::consts::FRAC_PI_2),
                                        ))
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("pr-permalink-{id}")))
                            .flex_none()
                            .size(px(16.0))
                            .opacity(0.0)
                            .group_hover("pr-comment", |style| style.opacity(1.0))
                            .role(gpui::Role::Link)
                            .aria_label("Open comment on GitHub")
                            .on_click({
                                let url = comment.url.clone();
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    cx.open_url(&url);
                                }
                            })
                            .child(icon("pr-permalink", theme.text_muted.into()).size(px(16.0))),
                    )
                    .child(Self::age_label(&comment.age, theme))
                    .child(self.comment_actions(&id, menu_open, cx)),
            )
    }

    fn comment_actions(
        &self,
        id: &str,
        open: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        div()
            .id(SharedString::from(format!("pr-comment-actions-{id}")))
            .relative()
            .child(self.control_anchor(format!("pr-comment-actions-{id}")))
            .flex_none()
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(9.375))
            .when(open, |button| button.bg(theme.row_hover))
            .hover(move |style| style.bg(theme.row_hover))
            .role(gpui::Role::Button)
            .aria_label("Comment actions")
            .on_click({
                let id = id.to_string();
                move |_, _, cx| {
                    cx.stop_propagation();
                    view.update(cx, |view, cx| view.toggle_comment_menu(id.clone(), cx));
                }
            })
            .child(icon("pr-more", theme.text_muted.into()).size(px(16.0)))
    }

    /// `px-3 pt-1 pb-2` body. Top-level comments clamp at six lines with a
    /// `Show more` toggle; thread comments render in full.
    fn comment_body(&self, comment: &Comment, clamp: bool, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let id = comment.id.clone();
        let scope = format!("pull-request-comment-{id}");
        if !clamp {
            let markdown = crate::components::markdown::render_pull_request_comment_markdown(
                &comment.body,
                Theme::for_mode(self.mode),
                &scope,
                None,
            );
            return div().px(px(12.0)).pt(px(4.0)).pb(px(8.0)).child(markdown);
        }
        let expanded = self.expanded_bodies.contains(&id);
        let body_clamp =
            self.body_measurements.borrow().get(&id).and_then(|blocks| {
                body_clamp(blocks, PULL_REQUEST_COMMENT_LINE_HEIGHT, CLAMP_LINES)
            });
        let applied = body_clamp.filter(|_| !expanded);
        let markdown = crate::components::markdown::render_pull_request_comment_markdown(
            &comment.body,
            Theme::for_mode(self.mode),
            &scope,
            applied.map(|clamp| clamp.line_clamp),
        );
        let measurements = self.body_measurements.clone();
        let measure_id = id.clone();
        let markdown = markdown.on_children_prepainted(move |bounds, window, _| {
            let Some(first) = bounds.first() else {
                return;
            };
            let top = first.origin.y;
            let blocks: Vec<(f32, f32)> = bounds
                .iter()
                .map(|block| {
                    (
                        f32::from(block.origin.y - top),
                        f32::from(block.size.height),
                    )
                })
                .collect();
            let mut measured = measurements.borrow_mut();
            let Some(clamp) = applied else {
                if measured.get(&measure_id) != Some(&blocks) {
                    measured.insert(measure_id.clone(), blocks);
                    window.refresh();
                }
                return;
            };
            // A clamped frame lays out only the blocks up to the clamp. It
            // stays valid while the blocks before it keep their measured boxes
            // and the clamped block still fills its lines; otherwise (a new
            // width, say) the next frame measures the whole body again.
            let index = clamp.line_clamp.block;
            let still_valid = measured.get(&measure_id).is_some_and(|full| {
                blocks.len() == index + 1
                    && full.len() > index
                    && blocks[..index]
                        .iter()
                        .zip(&full[..index])
                        .all(|(a, b)| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01)
                    && blocks[index].1
                        >= clamp.line_clamp.lines as f32 * PULL_REQUEST_COMMENT_LINE_HEIGHT - 0.5
            });
            if !still_valid {
                measured.remove(&measure_id);
                window.refresh();
            }
        });
        let view = cx.entity();
        div()
            .px(px(12.0))
            .pt(px(4.0))
            .pb(px(8.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .overflow_hidden()
                    .when_some(applied, |body, clamp| body.max_h(px(clamp.bottom)))
                    .child(markdown),
            )
            .when(body_clamp.is_some(), |body| {
                body.child(
                    div()
                        .id(SharedString::from(format!("pr-comment-more-{id}")))
                        .mt(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .text_size(px(14.0))
                        .line_height(px(21.0))
                        .text_color(theme.text_muted)
                        .role(gpui::Role::Button)
                        .aria_label(if expanded { "Show less" } else { "Show more" })
                        .aria_expanded(expanded)
                        .on_click(move |_, _, cx| {
                            let id = id.clone();
                            view.update(cx, |view, cx| view.toggle_body_expanded(id, cx));
                        })
                        .child(if expanded { "Show less" } else { "Show more" })
                        .child(
                            icon("section-chevron", theme.text_muted.into())
                                .size(px(14.0))
                                .when(expanded, |chevron| {
                                    chevron.with_transformation(gpui::Transformation::rotate(
                                        gpui::radians(std::f32::consts::PI),
                                    ))
                                }),
                        ),
                )
            })
    }

    /// A reply inside a review thread: a rule, then the reply's own header and
    /// body.
    fn thread_reply(&self, reply: &Comment, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        div()
            .border_t(px(1.0))
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(self.comment_header(reply, false, cx))
            .when(!reply.body.trim().is_empty(), |reply_card| {
                reply_card.child(self.comment_body(reply, false, cx))
            })
    }

    /// The commented code of a review thread (`mt-2 border-y bg-surface`): the
    /// file row that opens the Code tab, then the thread's diff hunk.
    fn thread_preview(
        &self,
        thread: &ReviewThread,
        comment: &Comment,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let theme = self.theme();
        let view = cx.entity();
        let path = thread.path.clone();
        let name = file_name(&thread.path);
        let language = Self::language_for(&thread.path);
        let mut lines = div().flex().flex_col();
        for (index, line) in preview_lines(&comment.diff_hunk).into_iter().enumerate() {
            let (surface, gutter_surface, number_color) = match line.kind {
                PreviewKind::Added => (
                    theme.diff_added_surface,
                    theme.diff_added_gutter,
                    theme.diff_added_text,
                ),
                PreviewKind::Deleted => (
                    theme.diff_deleted_surface,
                    theme.diff_deleted_gutter,
                    theme.diff_deleted_text,
                ),
                PreviewKind::Context => (theme.surface, theme.surface, theme.diff_gutter_text),
            };
            lines = lines.child(
                div()
                    .h(px(PREVIEW_LINE_HEIGHT))
                    .flex()
                    .child(
                        div()
                            .relative()
                            .flex_none()
                            .w(px(PREVIEW_GUTTER_WIDTH))
                            .h_full()
                            .pr(px(PREVIEW_CELL_PADDING))
                            .border_r(px(2.0))
                            .border_color(theme.surface)
                            .bg(gutter_surface)
                            .flex()
                            .items_center()
                            .justify_end()
                            .text_color(number_color)
                            // `data-indicators="bars"`: a 4px bar in the
                            // number color marks changed lines.
                            .when(line.kind != PreviewKind::Context, |gutter| {
                                gutter.child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .bottom_0()
                                        .w(px(4.0))
                                        .bg(number_color),
                                )
                            })
                            .child(line.number.map(|n| n.to_string()).unwrap_or_default()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "pr-thread-line-{}-{index}",
                                thread.id
                            )))
                            .flex_1()
                            .min_w(px(0.0))
                            .h_full()
                            .px(px(PREVIEW_CELL_PADDING))
                            .bg(surface)
                            .flex()
                            .items_center()
                            .whitespace_nowrap()
                            .text_color(theme.diff_context_text)
                            .child(self.code_text(&line.text, language)),
                    ),
            );
        }
        div()
            .mt(px(8.0))
            .border_y(px(1.0))
            .border_color(theme.border)
            .bg(theme.surface)
            .flex()
            .flex_col()
            .child(
                div()
                    .py(px(8.0))
                    .pl(px(46.0))
                    .pr(px(12.0))
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .id(SharedString::from(format!("pr-comment-open-{}", thread.id)))
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.0))
                            .line_height(px(20.0))
                            .role(gpui::Role::Button)
                            .aria_label(SharedString::from(format!("Open {name} in Code")))
                            .hover(move |style| style.text_color(theme.text))
                            .on_click(move |_, _, cx| {
                                let path = path.clone();
                                view.update(cx, |view, cx| {
                                    view.set_detail_tab(DetailTab::Code, cx);
                                    view.scroll_to_file(path, cx);
                                });
                            })
                            .child(name),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("pr-thread-hunk-{}", thread.id)))
                    .overflow_x_scroll()
                    .font_family(UI_MONOSPACE_FONT_FAMILY)
                    .text_size(px(12.0))
                    .line_height(px(PREVIEW_LINE_HEIGHT))
                    .child(lines),
            )
    }
}

/// Where `line-clamp-6` cuts a body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BodyClamp {
    /// The block holding the last visible line, and how many of its lines show.
    line_clamp: MarkdownLineClamp,
    /// The bottom of the last visible line box.
    bottom: f32,
}

/// Where `line-clamp-6` cuts a body made of blocks `(top, height)`: after the
/// sixth line box, counting each block as `height / line_height` lines.
/// `None` when the body is not taller than that.
pub(super) fn body_clamp(
    blocks: &[(f32, f32)],
    line_height: f32,
    lines: usize,
) -> Option<BodyClamp> {
    let mut remaining = lines;
    for (index, &(top, height)) in blocks.iter().enumerate() {
        let block_lines = ((height / line_height).round() as usize).max(1);
        if block_lines >= remaining {
            let bottom = top + remaining as f32 * line_height;
            let content = blocks
                .last()
                .map_or(0.0, |&(last_top, last_height)| last_top + last_height);
            return (content > bottom + 0.5).then_some(BodyClamp {
                line_clamp: MarkdownLineClamp {
                    block: index,
                    lines: remaining,
                },
                bottom,
            });
        }
        remaining -= block_lines;
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewKind {
    Added,
    Deleted,
    Context,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreviewLine {
    kind: PreviewKind,
    number: Option<u32>,
    text: String,
}

/// Rows of a review comment's `diffHunk`, numbered like the reference's
/// unified view: the new-file number, or the old one for deletions.
fn preview_lines(hunk: &str) -> Vec<PreviewLine> {
    let mut lines = Vec::new();
    let (mut old, mut new) = (1u32, 1u32);
    for raw in hunk.lines() {
        if let Some(header) = raw.strip_prefix("@@") {
            let mut parts = header.split_whitespace();
            let parse = |part: Option<&str>, sign: char| {
                part.and_then(|part| part.strip_prefix(sign))
                    .and_then(|part| part.split(',').next())
                    .and_then(|start| start.parse::<u32>().ok())
            };
            old = parse(parts.next(), '-').unwrap_or(1).max(1);
            new = parse(parts.next(), '+').unwrap_or(1).max(1);
            continue;
        }
        let (kind, text) = match raw.chars().next() {
            Some('+') => (PreviewKind::Added, &raw[1..]),
            Some('-') => (PreviewKind::Deleted, &raw[1..]),
            Some('\\') => continue,
            Some(' ') => (PreviewKind::Context, &raw[1..]),
            _ => (PreviewKind::Context, raw),
        };
        let number = match kind {
            PreviewKind::Deleted => {
                old += 1;
                Some(old - 1)
            }
            PreviewKind::Added => {
                new += 1;
                Some(new - 1)
            }
            PreviewKind::Context => {
                old += 1;
                new += 1;
                Some(new - 1)
            }
        };
        lines.push(PreviewLine {
            kind,
            number,
            text: text.to_string(),
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_counts_lines_across_paragraphs() {
        // A five-line paragraph, a 13px gap, then single-line paragraphs: the
        // sixth line is the second paragraph's first line.
        let lh = 21.125;
        let blocks = [
            (0.0, 5.0 * lh),
            (5.0 * lh + 13.0, lh),
            (6.0 * lh + 26.0, lh),
        ];
        let clamp = body_clamp(&blocks, lh, 6).unwrap();
        assert!((clamp.bottom - (5.0 * lh + 13.0 + lh)).abs() < 0.01);
        assert_eq!(clamp.line_clamp, MarkdownLineClamp { block: 1, lines: 1 });
        // Six lines or fewer need no clamp.
        assert_eq!(body_clamp(&blocks[..2], lh, 6), None);
    }

    #[test]
    fn preview_numbers_follow_the_hunk_header() {
        let lines = preview_lines("@@ -10,3 +12,4 @@ fn x\n a\n-b\n+c\n+d\n e");
        let numbers: Vec<_> = lines.iter().map(|line| (line.kind, line.number)).collect();
        assert_eq!(
            numbers,
            vec![
                (PreviewKind::Context, Some(12)),
                (PreviewKind::Deleted, Some(11)),
                (PreviewKind::Added, Some(13)),
                (PreviewKind::Added, Some(14)),
                (PreviewKind::Context, Some(15)),
            ]
        );
    }
}
