//! GitHub writes: single-flight dispatch, retained drafts, and authoritative readback.
use super::PullRequestsView;
use crate::pull_requests::{GhClient, NewReviewComment, PullRequestStatus, PullRequestSummary};
use gpui::Context;

impl PullRequestsView {
    pub(super) fn mutate(
        &mut self,
        summary: PullRequestSummary,
        message: &'static str,
        operation: impl FnOnce(&GhClient, &PullRequestSummary) -> anyhow::Result<()> + Send + 'static,
        on_success: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.mutation_pending {
            return;
        }
        self.mutation_pending = true;
        let generation = self.detail_generation;
        let client = self.client.clone();
        self.show_notice("Saving to GitHub…".into(), cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    operation(&client, &summary).map_err(|error| format!("{error:#}"))?;
                    Ok::<_, String>(
                        client
                            .detail(&summary.repository, summary.number)
                            .map_err(|error| format!("{error:#}")),
                    )
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                let refresh_list = result.is_ok();
                view.finish_mutation(generation, message, result, on_success, cx);
                if refresh_list {
                    view.reload(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn finish_mutation(
        &mut self,
        generation: u64,
        message: &str,
        result: Result<Result<crate::pull_requests::PullRequestDetail, String>, String>,
        on_success: impl FnOnce(&mut Self, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) {
        self.mutation_pending = false;
        // A response may refresh the list, but it must never overwrite a
        // different selection or clear a newer draft.
        let same_selection = self.detail_generation == generation;
        match result {
            Ok(readback) => {
                if same_selection {
                    on_success(self, cx);
                }
                match readback {
                    Ok(detail) => {
                        if same_selection {
                            self.selected = Some(detail.summary.clone());
                            self.detail = Some(detail);
                        }
                        self.show_notice(message.into(), cx);
                    }
                    Err(error) => {
                        self.show_notice(format!("Saved to GitHub; refresh failed: {error}"), cx)
                    }
                }
            }
            Err(error) => self.show_notice(format!("GitHub: {error}"), cx),
        }
        cx.notify();
    }

    pub fn save_title(&mut self, cx: &mut Context<Self>) {
        let (Some(input), Some(summary)) = (self.title_edit.clone(), self.selected.clone()) else {
            return;
        };
        let title = input.read(cx).text().trim().to_string();
        if title.is_empty() {
            return;
        }
        let submitted = title.clone();
        self.mutate(
            summary,
            "Title saved",
            move |client, pr| client.edit_title(&pr.repository, pr.number, &title),
            move |view, cx| {
                if input.read(cx).text().trim() == submitted {
                    view.title_edit = None;
                }
            },
            cx,
        );
    }

    pub fn save_description(&mut self, cx: &mut Context<Self>) {
        let (Some(editor), Some(summary)) = (self.description_edit.clone(), self.selected.clone())
        else {
            return;
        };
        let body = editor.read(cx).text().to_string();
        let submitted = body.clone();
        self.mutate(
            summary,
            "Description saved",
            move |client, pr| client.edit_body(&pr.repository, pr.number, &body),
            move |view, cx| {
                if editor.read(cx).text() == submitted {
                    view.description_edit = None;
                }
            },
            cx,
        );
    }

    pub fn set_status(&mut self, status: PullRequestStatus, cx: &mut Context<Self>) {
        let Some(summary) = self.selected.clone() else {
            return;
        };
        let current = summary.status;
        self.status_menu = false;
        self.mutate(
            summary,
            "Status updated",
            move |client, pr| client.set_status(&pr.repository, pr.number, current, status),
            |_, _| {},
            cx,
        );
    }

    pub fn merge(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        if detail.summary.status != PullRequestStatus::Open {
            return;
        }
        let summary = detail.summary.clone();
        let head = detail.head_sha.clone();
        self.mutate(
            summary,
            "Merge request completed",
            move |client, pr| client.merge(&pr.repository, pr.number, &head),
            |_, _| {},
            cx,
        );
    }

    pub fn submit_reviewers(&mut self, cx: &mut Context<Self>) {
        let Some(summary) = self.selected.clone() else {
            return;
        };
        let logins: Vec<_> = self.reviewers_selected.iter().cloned().collect();
        if logins.is_empty() {
            return;
        }
        self.mutate(
            summary,
            "Review requested",
            move |client, pr| client.request_reviewers(&pr.repository, pr.number, &logins),
            |view, cx| view.close_reviewers(cx),
            cx,
        );
    }

    pub fn post_comment(&mut self, cx: &mut Context<Self>) {
        let Some(summary) = self.selected.clone() else {
            return;
        };
        let body = self.comment_box.read(cx).text().trim().to_string();
        if body.is_empty() {
            return;
        }
        let submitted = body.clone();
        self.mutate(
            summary,
            "Comment posted",
            move |client, pr| client.comment(&pr.repository, pr.number, &body),
            move |view, cx| {
                if view.comment_box.read(cx).text().trim() == submitted {
                    view.comment_box
                        .update(cx, |editor, cx| editor.reload(String::new(), cx));
                }
            },
            cx,
        );
    }

    pub fn save_comment_edit(&mut self, cx: &mut Context<Self>) {
        let (Some((id, editor)), Some(summary)) =
            (self.comment_edit.clone(), self.selected.clone())
        else {
            return;
        };
        let Some(comment) = self
            .detail
            .as_ref()
            .and_then(|d| d.comment(&id))
            .filter(|c| c.can_edit)
            .cloned()
        else {
            return;
        };
        let body = editor.read(cx).text().trim().to_string();
        if body.is_empty() {
            return;
        }
        let submitted = body.clone();
        self.mutate(
            summary,
            "Comment updated",
            move |client, _| client.edit_comment(&comment, &body),
            move |view, cx| {
                if editor.read(cx).text().trim() == submitted {
                    view.comment_edit = None;
                }
            },
            cx,
        );
    }

    pub fn delete_comment(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(summary) = self.selected.clone() else {
            return;
        };
        let Some(comment) = self
            .detail
            .as_ref()
            .and_then(|d| d.comment(&id))
            .filter(|c| c.can_delete)
            .cloned()
        else {
            return;
        };
        self.comment_menu = None;
        self.mutate(
            summary,
            "Comment deleted",
            move |client, _| client.delete_comment(&comment),
            |_, _| {},
            cx,
        );
    }

    pub fn post_reply(&mut self, cx: &mut Context<Self>) {
        let (Some((Some(thread), editor)), Some(summary)) =
            (self.reply.clone(), self.selected.clone())
        else {
            return;
        };
        let body = editor.read(cx).text().trim().to_string();
        if body.is_empty() {
            return;
        }
        let submitted = body.clone();
        self.mutate(
            summary,
            "Reply posted",
            move |client, _| client.reply_to_thread(&thread, &body),
            move |view, cx| {
                if editor.read(cx).text().trim() == submitted {
                    view.cancel_reply(cx);
                }
            },
            cx,
        );
    }

    pub fn resolve_thread(&mut self, thread: String, cx: &mut Context<Self>) {
        let Some(summary) = self.selected.clone() else {
            return;
        };
        self.mutate(
            summary,
            "Conversation resolved",
            move |client, _| client.resolve_thread(&thread),
            |_, _| {},
            cx,
        );
    }

    pub fn submit_inline_comment(&mut self, cx: &mut Context<Self>) {
        let (Some(target), Some(editor), Some(summary)) = (
            self.inline_comment.clone(),
            self.inline_editor.clone(),
            self.selected.clone(),
        ) else {
            return;
        };
        let body = editor.read(cx).text().trim().to_string();
        if body.is_empty() {
            return;
        }
        let submitted = body.clone();
        let comment = NewReviewComment {
            path: target.path,
            line: target.line,
            old: target.old,
            commit: self.content_sha(),
            body,
        };
        self.mutate(
            summary,
            "Comment added",
            move |client, pr| client.add_review_comment(&pr.repository, pr.number, &comment),
            move |view, cx| {
                if editor.read(cx).text().trim() == submitted {
                    view.cancel_inline_comment(cx);
                }
            },
            cx,
        );
    }
}
