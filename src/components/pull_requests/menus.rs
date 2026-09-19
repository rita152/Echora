//! Detail-pane overlays: the activity timeline with comment actions, and the
//! menus anchored to the status, description, and comment rows.

use gpui::{Div, SharedString, div, prelude::*, px};

use super::{PullRequestsView, diff::file_name};
use crate::components::icons::icon;
use crate::pull_requests::{Comment, PullRequestDetail, TimelineEntry, TimelineKind};
use crate::theme::Theme;

/// Finds a comment by id across the issue comments and the review threads.
fn find_comment<'a>(detail: &'a PullRequestDetail, id: &str) -> Option<&'a Comment> {
    detail
        .comments
        .iter()
        .find(|comment| comment.id == id)
        .or_else(|| {
            detail
                .review_threads
                .iter()
                .flat_map(|thread| thread.comments.iter())
                .find(|comment| comment.id == id)
        })
}

impl PullRequestsView {
    /// Timeline cards for the inline review comments of every review thread.
    fn review_thread_entries(&self, detail: &PullRequestDetail) -> Vec<TimelineEntry> {
        detail
            .review_threads
            .iter()
            .flat_map(|thread| thread.comments.iter())
            .map(|comment| TimelineEntry {
                kind: TimelineKind::Comment,
                at: comment.at.clone(),
                actor: comment.author.clone(),
                age: comment.age.clone(),
                comment_id: Some(comment.id.clone()),
                commit_sha: None,
                commit_subject: None,
            })
            .collect()
    }

    /// Activity timeline: the reference lists the `opened` event and every
    /// comment as its own bordered card.
    pub(super) fn activity_timeline(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let Some(detail) = self.detail.as_ref() else {
            return div();
        };
        let mut timeline = div().flex().flex_col().gap(px(12.0)).px(px(8.0));
        // Without a timeline payload (older data, or a failed query) the feed
        // falls back to the comments alone, prefixed by the `opened` event.
        let entries: Vec<TimelineEntry> = if detail.timeline.is_empty() {
            let mut entries = vec![TimelineEntry {
                kind: TimelineKind::Opened,
                at: String::new(),
                actor: detail.author.login.clone(),
                age: detail.created_age.clone(),
                comment_id: None,
                commit_sha: None,
                commit_subject: None,
            }];
            entries.extend(detail.comments.iter().map(|comment| TimelineEntry {
                kind: TimelineKind::Comment,
                at: String::new(),
                actor: comment.author.clone(),
                age: comment.age.clone(),
                comment_id: Some(comment.id.clone()),
                commit_sha: None,
                commit_subject: None,
            }));
            entries.extend(self.review_thread_entries(detail));
            entries
        } else {
            let mut entries = detail.timeline.clone();
            // Review-thread comments are part of the feed but not of the
            // timeline payload; their timestamps put them back in order.
            entries.extend(self.review_thread_entries(detail));
            entries.sort_by(|left, right| left.sort_key().cmp(right.sort_key()));
            entries
        };
        for entry in &entries {
            if entry.kind != TimelineKind::Comment {
                timeline = timeline.child(self.timeline_event_card(detail, entry, cx));
                continue;
            }
            let Some(comment) = entry
                .comment_id
                .as_deref()
                .and_then(|id| find_comment(detail, id))
            else {
                continue;
            };
            let id = comment.id.clone();
            let collapsed = self.collapsed_comments.contains(&id);
            let menu_open = self.comment_menu.as_deref() == Some(id.as_str());
            let editing = self
                .comment_edit
                .as_ref()
                .is_some_and(|(editing_id, _)| editing_id == &id);
            let view = cx.entity();
            let markdown = crate::components::markdown::render_pull_request_markdown(
                &comment.body,
                Theme::for_mode(self.mode),
                &format!("pull-request-comment-{id}"),
            );
            let mut entry = div()
                .id(SharedString::from(format!("pr-comment-{id}")))
                .rounded(px(16.0))
                .border(px(1.0))
                .border_color(theme.border)
                .bg(theme.card_surface)
                // Measured card metrics: a 12px inset all around (so a
                // header-only card is 48px tall), a 13px side padding, and a
                // 24px header row. Children that are not full-bleed carry the
                // side padding themselves.
                .pb(px(12.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(13.0))
                        .pt(px(12.0))
                        .h(px(36.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .id(SharedString::from(format!("pr-comment-author-{id}")))
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .cursor_pointer()
                                .role(gpui::Role::Button)
                                .aria_label(SharedString::from(if collapsed {
                                    format!("Expand comment by {}", comment.author)
                                } else {
                                    format!("Collapse comment by {}", comment.author)
                                }))
                                .on_click({
                                    let view = view.clone();
                                    let id = id.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |view, cx| {
                                            view.toggle_comment_collapsed(id.clone(), cx)
                                        });
                                    }
                                })
                                .child(
                                    div()
                                        .size(px(24.0))
                                        .rounded(px(9999.0))
                                        .bg(theme.control)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(12.0))
                                        .text_color(theme.text)
                                        .child(
                                            comment
                                                .author
                                                .chars()
                                                .next()
                                                .map(|letter| letter.to_uppercase().to_string())
                                                .unwrap_or_default(),
                                        ),
                                )
                                .child(
                                    div()
                                        // The reference renders the author name
                                        // in the same secondary tone as the age.
                                        .text_size(px(12.0))
                                        .text_color(theme.text_muted)
                                        .child(comment.author.clone()),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme.text_muted)
                                .child(comment.age.clone()),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .id(SharedString::from(format!("pr-permalink-{id}")))
                                .size(px(16.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(9999.0))
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.control_hover))
                                .role(gpui::Role::Link)
                                .aria_label("Copy link to comment")
                                .on_click({
                                    let url = format!(
                                        "{}#issuecomment-{}",
                                        detail.summary.url, comment.id
                                    );
                                    move |_, _, cx| cx.open_url(&url)
                                })
                                .child(
                                    icon("markdown-link", theme.text_muted.into()).size(px(16.0)),
                                ),
                        )
                        .child(self.comment_actions(&id, menu_open, cx)),
                );
            if !collapsed {
                if editing {
                    let (_, editor) = self.comment_edit.clone().unwrap();
                    entry = entry.child(editor).child(self.comment_edit_actions(cx));
                } else {
                    if comment.is_review
                        && let Some(path) = comment.path.clone()
                    {
                        entry = entry.child(self.thread_code_preview(&path, &id, cx));
                    }
                    // The reference renders no body for a comment whose markdown
                    // opens with an HTML comment (the Codex review summaries):
                    // both observed review-summary comments show the header card
                    // alone, so mirror that instead of drawing the raw payload.
                    if !comment.body.trim_start().starts_with("<!--") {
                        entry = entry
                            .child(div().pt(px(12.0)).pl(px(47.0)).pr(px(13.0)).child(markdown));
                    }
                    // `Reply`/`Resolve` belong to review threads; a review
                    // summary comment has no thread, and the reference shows
                    // only its action menu.
                    if comment.thread_id.is_some() {
                        entry = entry.child(self.comment_reply_actions(
                            &id,
                            comment.thread_id.clone(),
                            cx,
                        ));
                    }
                }
            }
            timeline = timeline.child(entry);
        }
        timeline
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
            .w(px(18.0))
            .h(px(26.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.5))
            .cursor_pointer()
            .when(open, |button| button.bg(theme.control_hover))
            .hover(move |style| style.bg(theme.control_hover))
            .role(gpui::Role::Button)
            .aria_label("Comment actions")
            .on_click({
                let id = id.to_string();
                move |_, _, cx| {
                    view.update(cx, |view, cx| view.toggle_comment_menu(id.clone(), cx));
                }
            })
            .child(icon("more-horizontal", theme.text_muted.into()).size(px(18.0)))
    }

    /// An activity card that is not a comment: the `opened`/`merged` state
    /// changes and the pull request's commits.
    fn timeline_event_card(
        &self,
        detail: &PullRequestDetail,
        entry: &TimelineEntry,
        _cx: &mut gpui::Context<Self>,
    ) -> Div {
        self.timeline_event_card_inner(detail, entry)
    }

    /// The code a review comment is anchored to: the reference shows the file's
    /// diff above the comment, with the added-line gutter and a `Open <file> in
    /// Code` header.
    fn thread_code_preview(
        &self,
        path: &str,
        comment_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let theme = self.theme();
        let view = cx.entity();
        let open_path = path.to_string();
        let name = file_name(path);
        let mut container = div()
            .w_full()
            .border(px(1.0))
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(
                div().h(px(37.0)).pl(px(46.0)).flex().items_center().child(
                    div()
                        .id(SharedString::from(format!("pr-comment-open-{comment_id}")))
                        .h(px(20.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(13.0))
                        .text_color(theme.text_muted)
                        .cursor_pointer()
                        .role(gpui::Role::Button)
                        .aria_label(format!("Open {name} in Code"))
                        .on_click(move |_, _, cx| {
                            let path = open_path.clone();
                            view.update(cx, |view, cx| {
                                view.set_detail_tab(super::DetailTab::Code, cx);
                                view.scroll_to_file(path, cx);
                            });
                        })
                        .child(icon("review-open", theme.text_muted.into()).size(px(14.0)))
                        .child(name),
                ),
            );
        if let Some(lines) = self.context_lines(path) {
            let mut body = div().flex().flex_col();
            for (index, line) in lines.iter().enumerate() {
                body = body.child(
                    div()
                        .h(px(21.6))
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .w(px(78.1))
                                .h_full()
                                .pr(px(10.0))
                                .flex()
                                .justify_end()
                                .items_center()
                                .bg(theme.diff_added_emphasis)
                                .font_family(crate::theme::UI_MONOSPACE_FONT_FAMILY)
                                .text_size(px(12.0))
                                .text_color(theme.diff_added_text)
                                .child(format!("{}", index + 1)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .h_full()
                                .pl(px(8.0))
                                .flex()
                                .items_center()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .bg(theme.diff_added_surface)
                                .font_family(crate::theme::UI_MONOSPACE_FONT_FAMILY)
                                .text_size(px(12.0))
                                .text_color(theme.diff_context_text)
                                .child(line.clone()),
                        ),
                );
            }
            container = container.child(body);
        }
        container
    }

    fn timeline_event_card_inner(&self, detail: &PullRequestDetail, entry: &TimelineEntry) -> Div {
        let theme = self.theme();
        let (icon_name, color, label): (&'static str, gpui::Rgba, String) = match entry.kind {
            TimelineKind::Commit => (
                "pr-meta-branch",
                theme.text_muted,
                entry.commit_subject.clone().unwrap_or_default(),
            ),
            TimelineKind::Merged => (
                "pr-merge",
                theme.merged_accent,
                format!("{} merged this pull request", entry.actor),
            ),
            TimelineKind::Closed => (
                "pr-merge",
                theme.deletions_text,
                format!("{} closed this pull request", entry.actor),
            ),
            TimelineKind::Reopened => (
                "pr-meta-branch",
                theme.status_added,
                format!("{} reopened this pull request", entry.actor),
            ),
            _ => (
                "pr-meta-branch",
                theme.status_added,
                format!("{} opened this pull request", entry.actor),
            ),
        };
        let mut card = div()
            .rounded(px(16.0))
            .border(px(1.0))
            .border_color(theme.border)
            .bg(theme.card_surface)
            .py(px(13.0))
            .px(px(16.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .line_height(px(20.0))
            .child(
                div()
                    .w(px(20.0))
                    .flex()
                    .justify_center()
                    .child(icon(icon_name, color.into()).size(px(16.0))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(label),
            );
        if let Some(sha) = entry.commit_sha.as_deref() {
            let short = sha.get(..7).unwrap_or(sha).to_string();
            let url = format!(
                "https://github.com/{}/commit/{}",
                detail.summary.repository, sha
            );
            card = card.child(
                div()
                    .id(SharedString::from(format!("pr-timeline-commit-{sha}")))
                    .font_family(crate::theme::UI_MONOSPACE_FONT_FAMILY)
                    .text_size(px(13.0))
                    .text_color(theme.text_muted)
                    .cursor_pointer()
                    .role(gpui::Role::Link)
                    .aria_label(format!("Commit {short}"))
                    .on_click(move |_, _, cx| cx.open_url(&url))
                    .child(short),
            );
        }
        card.child(
            div()
                .text_size(px(13.0))
                .text_color(theme.text_muted)
                .child(entry.age.clone()),
        )
    }

    fn comment_edit_actions(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let cancel = view.clone();
        div()
            .flex()
            .justify_end()
            .gap(px(4.0))
            .child(
                div()
                    .id("pr-comment-edit-cancel")
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
                        cancel.update(cx, |view, cx| view.cancel_comment_edit(cx));
                    })
                    .child("Cancel"),
            )
            .child(
                div()
                    .id("pr-comment-edit-save")
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
                    .aria_label("Save changes")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.save_comment_edit(cx));
                    })
                    .child("Save changes"),
            )
    }

    fn comment_reply_actions(
        &self,
        comment_id: &str,
        thread: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let reply_open = self
            .reply_target
            .as_deref()
            .is_some_and(|target| Some(target) == thread.as_deref() || target == comment_id);
        // The reference indents the thread actions to the comment body.
        let mut actions = div()
            .pl(px(39.0))
            .pb(px(12.0))
            .flex()
            .items_center()
            .gap(px(4.0));
        let reply_view = view.clone();
        let reply_thread = thread.clone();
        actions = actions.child(
            div()
                .id(SharedString::from(format!("pr-reply-{comment_id}")))
                .h(px(24.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(9999.0))
                .text_size(px(13.0))
                .text_color(theme.text_muted)
                .cursor_pointer()
                .hover(move |style| style.bg(theme.control_hover))
                .role(gpui::Role::Button)
                .aria_label("Reply")
                .on_click(move |_, _, cx| {
                    let thread = reply_thread.clone();
                    reply_view.update(cx, |view, cx| view.begin_reply(thread.clone(), None, cx));
                })
                .child("Reply"),
        );
        if let Some(thread_id) = thread.clone() {
            let resolve_view = view.clone();
            actions = actions.child(
                div()
                    .id(SharedString::from(format!("pr-resolve-{comment_id}")))
                    .h(px(24.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(9999.0))
                    .text_size(px(13.0))
                    .text_color(theme.text_muted)
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.control_hover))
                    .role(gpui::Role::Button)
                    .aria_label("Resolve")
                    .on_click(move |_, _, cx| {
                        resolve_view
                            .update(cx, |view, cx| view.resolve_thread(thread_id.clone(), cx));
                    })
                    .child(icon("check", theme.text_muted.into()).size(px(14.0)))
                    .child("Resolve"),
            );
        }
        if reply_open {
            let cancel_view = view.clone();
            let post_view = view.clone();
            let has_text = self
                .reply
                .as_ref()
                .is_some_and(|(_, editor)| !editor.read(cx).text().trim().is_empty());
            actions = div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .p(px(8.0))
                        .rounded(px(16.0))
                        .bg(theme.field_surface)
                        .border(px(1.0))
                        .border_color(theme.field_border)
                        .children(self.reply.as_ref().map(|(_, editor)| editor.clone())),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(4.0))
                        .child(
                            div()
                                .id("pr-reply-cancel")
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
                                    cancel_view.update(cx, |view, cx| view.cancel_reply(cx));
                                })
                                .child("Cancel"),
                        )
                        .child(
                            div()
                                .id("pr-reply-post")
                                .h(px(28.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .rounded(px(12.5))
                                .text_size(px(13.0))
                                .when(has_text, |button| {
                                    button
                                        .bg(theme.inverted_surface)
                                        .text_color(theme.inverted_text)
                                        .cursor_pointer()
                                        .on_click(move |_, _, cx| {
                                            post_view.update(cx, |view, cx| view.post_reply(cx));
                                        })
                                })
                                .when(!has_text, |button| {
                                    button
                                        .bg(theme.control)
                                        .text_color(theme.text_muted)
                                        .opacity(0.6)
                                })
                                .role(gpui::Role::Button)
                                .aria_label("Post reply")
                                .child("Post reply"),
                        ),
                );
        }
        actions
    }

    /// Overlays anchored inside the detail pane.
    pub(super) fn detail_overlays(&self, cx: &mut gpui::Context<Self>) -> Vec<Div> {
        let mut overlays = Vec::new();
        if self.status_menu {
            overlays.push(
                div()
                    .absolute()
                    .left(px(150.0))
                    .top(px(310.0))
                    .child(self.status_menu(cx)),
            );
        }
        if self.description_menu {
            overlays.push(
                div()
                    .absolute()
                    .right(px(24.0))
                    .top(px(380.0))
                    .child(self.description_menu(cx)),
            );
        }
        if self.review_options_open {
            overlays.push(
                div()
                    .absolute()
                    .right(px(120.0))
                    .top(px(44.0))
                    .child(self.review_options_menu(cx)),
            );
        }
        if self.scope_menu_open {
            overlays.push(
                div()
                    .absolute()
                    .left(px(120.0))
                    .top(px(44.0))
                    .child(self.scope_menu(cx)),
            );
        }
        if let Some(id) = self.comment_menu.clone() {
            let has_comment = self
                .detail
                .as_ref()
                .is_some_and(|detail| detail.comments.iter().any(|comment| comment.id == id));
            if has_comment {
                overlays.push(self.comment_menu_overlay(id, cx));
            }
        }
        overlays
    }

    fn comment_menu_overlay(&self, id: String, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let view = cx.entity();
        let mut menu = div()
            .id("pr-comment-menu")
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
            .text_color(theme.text);
        let entries = ["Edit", "Quote reply", "Delete"];
        for entry in entries {
            let view = view.clone();
            let id = id.clone();
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("pr-comment-action-{entry}")))
                    .h(px(28.5))
                    .px(px(8.0))
                    .rounded(px(15.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.menu_hover))
                    .role(gpui::Role::MenuItem)
                    .aria_label(entry)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| match entry {
                            "Edit" => view.begin_comment_edit(id.clone(), cx),
                            "Quote reply" => {
                                let comment = view.detail.as_ref().and_then(|detail| {
                                    detail
                                        .comments
                                        .iter()
                                        .find(|comment| comment.id == id)
                                        .cloned()
                                });
                                let quote = comment
                                    .as_ref()
                                    .map(|comment| {
                                        comment
                                            .body
                                            .lines()
                                            .map(|line| format!("> {line}"))
                                            .collect::<Vec<_>>()
                                            .join("\n")
                                    })
                                    .unwrap_or_default();
                                let thread = comment
                                    .as_ref()
                                    .and_then(|comment| comment.thread_id.clone());
                                view.begin_reply(thread, Some(format!("{quote}\n\n")), cx);
                            }
                            "Delete" => view.delete_comment(id.clone(), cx),
                            _ => {}
                        });
                    })
                    .child(entry),
            );
        }
        div().absolute().right(px(24.0)).top(px(120.0)).child(menu)
    }
}
