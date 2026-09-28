//! Editing and undoing queued messages, as the reference does it.
//!
//! "Edit message" removes the row from the server queue right away and loads
//! its input into the composer; resubmitting queues a new message at the old
//! position (next neighbour, else previous neighbour + 1, else the end). A
//! deleted or edited message can be put back with ⌘Z (60 s after a delete,
//! 30 min after an edit): it is re-added with its original id and input and
//! moved back into place, and a toast confirms it. ⌘⇧Z then redoes the
//! removal (delete again, or reopen the edit) within a fresh window; a new
//! delete or edit drops the redo.

use std::time::{Duration, Instant};

use gpui::Context;

use super::{ComposerView, ConversationChanged, toast::ToastKind};
use crate::{
    agent::{AgentQueueAddRequest, AgentQueueUpdateRequest, AgentQueuedSubmission},
    conversation::{ConversationPhase, QueueRowOperation, SubmissionDraft},
};

const DELETE_UNDO_WINDOW: Duration = Duration::from_secs(60);
const EDIT_UNDO_WINDOW: Duration = Duration::from_secs(30 * 60);

/// Where a removed row sat, by its neighbours at removal time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct QueueSlot {
    previous: Option<String>,
    next: Option<String>,
}

impl QueueSlot {
    /// The index to put a message back at in `order` (which excludes it).
    fn index_in(&self, order: &[String]) -> usize {
        let find = |id: &Option<String>| {
            id.as_ref()
                .and_then(|id| order.iter().position(|other| other == id))
        };
        find(&self.next)
            .or_else(|| find(&self.previous).map(|index| index + 1))
            .unwrap_or(order.len())
    }
}

/// A queued message loaded into the composer; its row is normally gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct QueueEdit {
    pub(super) slot: QueueSlot,
    /// The removed row. If a later list shows it again (restored elsewhere),
    /// resubmitting updates it in place, as the reference does.
    pub(super) queued_submission_id: String,
    pub(super) client_message_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Removal {
    Deleted,
    Edited,
}

/// Asks the shell to open a new side chat that sends this message.
pub struct OpenQueuedInSideChat {
    pub draft: SubmissionDraft,
    pub prompt: String,
    pub removed: RemovedQueuedMessage,
}
impl gpui::EventEmitter<OpenQueuedInSideChat> for ComposerView {}

/// The last queued message this client removed, kept for ⌘Z.
#[derive(Clone, Debug)]
pub struct RemovedQueuedMessage {
    removal: Removal,
    submission: AgentQueuedSubmission,
    draft: SubmissionDraft,
    prompt: String,
    slot: QueueSlot,
    expires_at: Instant,
}

impl ComposerView {
    fn queue_slot(&self, id: &str) -> Option<QueueSlot> {
        let order = self.conversation.queue.order();
        let index = order.iter().position(|other| other == id)?;
        Some(QueueSlot {
            previous: index.checked_sub(1).map(|i| order[i].clone()),
            next: order.get(index + 1).cloned(),
        })
    }

    /// The input a row can be restored or edited from.
    fn row_draft(&self, submission: &AgentQueuedSubmission) -> (SubmissionDraft, String) {
        if let Some(draft) = self
            .conversation
            .queue
            .drafts
            .get(&submission.client_message_id)
        {
            return draft.clone();
        }
        let (prompt, context) = super::followup::row_input(submission, None);
        (
            SubmissionDraft {
                text: submission.text.clone(),
                context,
                comments: Vec::new(),
            },
            prompt,
        )
    }

    /// Captures what ⌘Z needs before a row is deleted.
    pub(super) fn removal_record(&self, id: &str, edited: bool) -> Option<RemovedQueuedMessage> {
        let submission = self
            .conversation
            .queue
            .rows
            .iter()
            .find(|row| row.submission.id == id)?
            .submission
            .clone();
        let (draft, prompt) = self.row_draft(&submission);
        let (removal, window) = if edited {
            (Removal::Edited, EDIT_UNDO_WINDOW)
        } else {
            (Removal::Deleted, DELETE_UNDO_WINDOW)
        };
        Some(RemovedQueuedMessage {
            removal,
            slot: self.queue_slot(id)?,
            submission,
            draft,
            prompt,
            expires_at: Instant::now() + window,
        })
    }

    /// "Edit message": removes the row from the server queue, then loads its
    /// input into the composer. If the user typed meanwhile, the row is put
    /// back instead.
    pub(crate) fn edit_queued(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.queue_edit.is_some() {
            return;
        }
        if !self.draft_is_empty(cx) {
            self.submission_error = Some(
                crate::i18n::format!("请先发送或清空当前草稿，再编辑排队的消息。" => "Send or clear the current draft before editing a queued message."),
            );
            cx.notify();
            return;
        }
        let Some(record) = self.removal_record(id, true) else {
            return;
        };
        self.queue_menu = None;
        self.queue_redo = None;
        let revision = self.draft_revision;
        self.remove_queued(
            id,
            Some(record),
            move |this, record, cx| {
                if this.draft_revision != revision || !this.draft_is_empty(cx) {
                    this.put_back(record, None, cx);
                    return;
                }
                this.queue_edit = Some(QueueEdit {
                    slot: record.slot.clone(),
                    queued_submission_id: record.submission.id.clone(),
                    client_message_id: record.submission.client_message_id.clone(),
                });
                this.restore_draft(record.draft.clone(), cx);
                this.queue_removal = Some(record);
            },
            cx,
        );
    }

    /// Resubmits an edited message: queued at its old position while a turn
    /// runs or other messages wait, otherwise sent now as a new turn.
    /// Returns false when the caller should send it as an ordinary prompt.
    pub(super) fn submit_queue_edit(
        &mut self,
        edit: QueueEdit,
        prompt: String,
        draft: SubmissionDraft,
        cx: &mut Context<Self>,
    ) -> bool {
        self.queue_edit = None;
        self.queue_removal = None;
        if self
            .conversation
            .queue
            .row_mut(&edit.queued_submission_id)
            .is_some()
        {
            self.clear_prompt(cx);
            self.prompt_context.files.clear();
            self.update_queued(edit, prompt, draft, cx);
            return true;
        }
        let queue = self.is_running()
            || self.conversation.phase == ConversationPhase::Starting
            || !self.conversation.queue.is_empty();
        if !queue {
            return false;
        }
        self.clear_prompt(cx);
        self.prompt_context.files.clear();
        self.add_at_slot(None, prompt, draft, edit.slot, None, cx);
        true
    }

    /// Replaces a row's input in place. On failure the edit stays open.
    fn update_queued(
        &mut self,
        edit: QueueEdit,
        prompt: String,
        draft: SubmissionDraft,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.queue_target(Some(edit.queued_submission_id.clone())) else {
            return;
        };
        if !self
            .conversation
            .queue
            .begin_row(&edit.queued_submission_id, QueueRowOperation::Updating)
        {
            return;
        }
        let receiver = self
            .backend
            .update_queued_submission(AgentQueueUpdateRequest {
                generation: target.generation,
                thread_id: target.thread_id,
                queued_submission_id: edit.queued_submission_id.clone(),
                prompt: prompt.clone(),
                context: draft.context.clone(),
            });
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::i18n::format!("更新排队消息的响应在返回前中断" => "The queue update ended before it returned"))
            });
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(submission) => {
                        this.conversation.queue.finish_row(&edit.queued_submission_id, None);
                        this.conversation
                            .queue
                            .drafts
                            .insert(edit.client_message_id.clone(), (draft, prompt));
                        this.conversation.queue.replace_submission(submission);
                    }
                    Err(error) => {
                        this.conversation
                            .queue
                            .finish_row(&edit.queued_submission_id, Some(error.clone()));
                        this.submission_error = Some(error);
                        if this.draft_is_empty(cx) {
                            this.queue_edit = Some(edit);
                            this.restore_draft(draft, cx);
                        }
                    }
                }
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
    }

    /// Adds a message and moves it to `slot`. `restore` names the original
    /// submission when this puts a removed row back; then its id and input are
    /// reused and a toast confirms the restore.
    fn add_at_slot(
        &mut self,
        client_message_id: Option<String>,
        prompt: String,
        draft: SubmissionDraft,
        slot: QueueSlot,
        restored: Option<Removal>,
        cx: &mut Context<Self>,
    ) {
        let (Some(thread_id), Some(generation)) = (
            self.conversation.queue.thread_id.clone(),
            self.queue_generation(),
        ) else {
            return;
        };
        let client_message_id =
            client_message_id.unwrap_or_else(crate::conversation::new_client_message_id);
        let receiver = self.backend.add_queued_submission(AgentQueueAddRequest {
            generation,
            thread_id: thread_id.clone(),
            client_message_id,
            prompt: prompt.clone(),
            context: draft.context.clone(),
        });
        self.conversation.queue.adding += 1;
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::i18n::format!("排队请求在返回前中断，消息未加入队列" => "The queue request ended before it returned; the message was not queued"))
            });
            let _ = this.update(cx, |this, cx| {
                this.conversation.queue.adding = this.conversation.queue.adding.saturating_sub(1);
                if this.conversation.queue.thread_id.as_deref() != Some(thread_id.as_str()) {
                    return;
                }
                match result {
                    Ok(submission) => {
                        let id = submission.id.clone();
                        this.conversation.queue.insert_added(submission, draft, prompt);
                        let order = this
                            .conversation
                            .queue
                            .order()
                            .into_iter()
                            .filter(|other| other != &id)
                            .collect::<Vec<_>>();
                        let index = slot.index_in(&order);
                        if index < order.len() {
                            this.reorder_queued(&id, index, cx);
                        }
                        if let Some(removal) = restored {
                            this.queue_redo = this.removal_record(&id, removal == Removal::Edited);
                            let text = match removal {
                                Removal::Deleted => crate::i18n::format!("已恢复队列中的消息" => "Queued message restored"),
                                Removal::Edited => crate::i18n::format!("已恢复排队的消息" => "Queued message restored"),
                            };
                            this.show_toast(ToastKind::Success, text, cx);
                        }
                    }
                    Err(error) => {
                        this.submission_error = Some(error);
                        if restored.is_none() && this.draft_is_empty(cx) {
                            this.restore_draft(draft, cx);
                        }
                    }
                }
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Re-adds a removed message with its original id and input.
    fn put_back(
        &mut self,
        record: RemovedQueuedMessage,
        toast: Option<Removal>,
        cx: &mut Context<Self>,
    ) {
        self.add_at_slot(
            Some(record.submission.client_message_id.clone()),
            record.prompt,
            record.draft,
            record.slot,
            toast,
            cx,
        );
    }

    /// ⌘Z outside a text undo: restores the last deleted or edited queued
    /// message while its undo window lasts. Returns whether it acted.
    pub(super) fn undo_queue_removal(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(record) = self.queue_removal.take() else {
            return false;
        };
        if Instant::now() > record.expires_at {
            return false;
        }
        if record.removal == Removal::Edited && self.queue_edit.take().is_some() {
            self.clear_prompt(cx);
            self.prompt_context.files.clear();
        }
        let removal = record.removal;
        self.put_back(record, Some(removal), cx);
        true
    }

    /// ⌘⇧Z after an undo: removes the restored message again. Returns
    /// whether it acted. No toast, as in the reference.
    pub(super) fn redo_queue_removal(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(record) = self.queue_redo.take() else {
            return false;
        };
        if Instant::now() > record.expires_at
            || self
                .conversation
                .queue
                .row_mut(&record.submission.id)
                .is_none()
        {
            return false;
        }
        match record.removal {
            Removal::Deleted => self.delete_queued(&record.submission.id, cx),
            Removal::Edited => self.edit_queued(&record.submission.id, cx),
        }
        true
    }

    /// Deletes a row; `record` is kept for ⌘Z once the delete lands, and
    /// `then` runs with it after a successful delete.
    pub(super) fn remove_queued(
        &mut self,
        id: &str,
        record: Option<RemovedQueuedMessage>,
        then: impl FnOnce(&mut Self, RemovedQueuedMessage, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.queue_target(Some(id.to_owned())) else {
            return;
        };
        if !self
            .conversation
            .queue
            .begin_row(id, QueueRowOperation::Deleting)
        {
            return;
        }
        let receiver = self.backend.delete_queued_submission(target);
        let id = id.to_owned();
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::i18n::format!("删除排队消息的响应在返回前中断" => "The queue delete ended before it returned"))
            });
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(deleted) => {
                        this.conversation.queue.remove(&id);
                        // `deleted=false`: already gone (started or removed
                        // elsewhere); there is nothing to undo or edit.
                        if deleted && let Some(record) = record {
                            then(this, record, cx);
                        }
                    }
                    Err(error) => this.conversation.queue.finish_row(&id, Some(error)),
                }
                this.conversation.queue.wanted += 1;
                this.refresh_queue(cx);
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// The composer mid-edit for deterministic captures: the row is removed
    /// locally and its input loaded, without the server round trip.
    #[cfg(feature = "screenshot")]
    pub(super) fn open_queue_edit_for_capture(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(record) = self.removal_record(id, true) else {
            return;
        };
        self.conversation.queue.remove(id);
        self.queue_edit = Some(QueueEdit {
            slot: record.slot.clone(),
            queued_submission_id: record.submission.id.clone(),
            client_message_id: record.submission.client_message_id.clone(),
        });
        self.restore_draft(record.draft, cx);
    }

    /// "Open in side chat": the row leaves the queue and is sent as a new
    /// side chat's first message. The shell answers with
    /// [`Self::restore_queued_from_side_chat`] when no side chat could start.
    pub(crate) fn open_queued_in_side_chat(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(record) = self.removal_record(id, false) else {
            return;
        };
        self.queue_menu = None;
        self.remove_queued(
            id,
            Some(record),
            |_, record, cx| {
                cx.emit(OpenQueuedInSideChat {
                    draft: record.draft.clone(),
                    prompt: record.prompt.clone(),
                    removed: record,
                })
            },
            cx,
        );
    }

    /// Puts a message back that could not be opened in a side chat.
    pub fn restore_queued_from_side_chat(
        &mut self,
        removed: RemovedQueuedMessage,
        cx: &mut Context<Self>,
    ) {
        self.put_back(removed, None, cx);
    }

    /// Sends a message moved from the parent's queue as this side chat's
    /// first turn.
    pub fn send_moved_queued_message(
        &mut self,
        draft: SubmissionDraft,
        prompt: String,
        cx: &mut Context<Self>,
    ) {
        self.restore_draft(draft, cx);
        self.submit_prompt(prompt, cx);
    }

    /// ArrowUp in an empty composer edits the last queued message.
    pub(super) fn edit_last_queued(&mut self, cx: &mut Context<Self>) -> bool {
        if self.queue_edit.is_some() || !self.draft_is_empty(cx) {
            return false;
        }
        let Some(id) = self
            .conversation
            .queue
            .rows
            .last()
            .filter(|row| row.operation.is_none())
            .map(|row| row.submission.id.clone())
        else {
            return false;
        };
        self.edit_queued(&id, cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::QueueSlot;

    fn slot(previous: Option<&str>, next: Option<&str>) -> QueueSlot {
        QueueSlot {
            previous: previous.map(str::to_owned),
            next: next.map(str::to_owned),
        }
    }

    #[test]
    fn a_removed_message_goes_before_its_next_neighbour_then_after_its_previous() {
        let order = ["a", "c"].map(str::to_owned);
        assert_eq!(slot(Some("a"), Some("c")).index_in(&order), 1);
        assert_eq!(slot(Some("a"), Some("gone")).index_in(&order), 1);
        assert_eq!(slot(Some("gone"), Some("a")).index_in(&order), 0);
        assert_eq!(slot(Some("gone"), Some("gone")).index_in(&order), 2);
        assert_eq!(slot(None, None).index_in(&[]), 0);
    }
}
