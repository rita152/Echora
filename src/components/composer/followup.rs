//! Server-side follow-up queue: sending while a turn runs, the queue tray's
//! row actions, and turns the server starts by itself.
//!
//! The server owns the queue and advances it when a turn completes; every
//! change is followed by a re-list, and the tray always shows the last list.

use gpui::Context;

use super::{ComposerView, ConversationChanged};
use crate::{
    agent::{
        AgentInputFile, AgentPromptContext, AgentQueueAddRequest, AgentQueueReorderRequest,
        AgentQueueTarget, AgentQueuedSubmission, AgentRun, UserMessageAttachment,
    },
    conversation::{ConversationPhase, QueueRowOperation, SubmissionDraft},
    workspace::FollowUpMode,
};

/// Emitted when the tray's menu turns queueing off or on; the app persists
/// the new default and applies it to every composer.
pub struct FollowUpModeToggled(pub FollowUpMode);
impl gpui::EventEmitter<FollowUpModeToggled> for ComposerView {}

/// The input a queued row was created from: this client's own draft when it
/// queued the row, otherwise the server's decoded input.
pub(super) fn row_input(
    submission: &AgentQueuedSubmission,
    draft: Option<&(SubmissionDraft, String)>,
) -> (String, AgentPromptContext) {
    if let Some((draft, prompt)) = draft {
        return (prompt.clone(), draft.context.clone());
    }
    let files = submission
        .attachments
        .iter()
        .filter_map(|attachment| match attachment {
            UserMessageAttachment::File(path) => Some(AgentInputFile {
                path: path.clone(),
                image: false,
            }),
            UserMessageAttachment::Local(path) => Some(AgentInputFile {
                path: path.clone(),
                image: true,
            }),
            UserMessageAttachment::Remote(_) | UserMessageAttachment::Unavailable(_) => None,
        })
        .collect();
    (
        submission.text.clone(),
        AgentPromptContext {
            files,
            plan_mode: None,
            memory: None,
        },
    )
}

impl ComposerView {
    pub fn set_follow_up_mode(&mut self, mode: FollowUpMode, cx: &mut Context<Self>) {
        if self.follow_up_mode != mode {
            self.follow_up_mode = mode;
            cx.notify();
        }
    }

    /// Side chats are ephemeral threads; the reference keeps their follow-ups
    /// out of the server queue, so they always steer.
    pub(super) fn queue_supported(&self) -> bool {
        !self.side_chat && self.conversation.thread_id.is_some()
    }

    /// The generation queue writes are bound to: the one the tray was listed
    /// in, else the running turn's.
    pub(super) fn queue_generation(&self) -> Option<u64> {
        self.conversation.queue.generation.or_else(|| {
            self.conversation
                .turn_identity
                .as_ref()
                .map(|identity| identity.generation)
        })
    }

    /// The tray shows the queue as paused after the user stopped a turn: the
    /// server keeps the remaining messages and does not advance them.
    pub(crate) fn queue_paused(&self) -> bool {
        self.conversation.phase == ConversationPhase::Stopped
            && !self.conversation.queue.is_empty()
            && !self.queue_resumed
    }

    /// Resets thread-scoped goal and queue state after the conversation's
    /// thread changed, then reads both for the new thread.
    pub(super) fn sync_thread_scoped_state(&mut self, cx: &mut Context<Self>) {
        let thread_id = self.conversation.thread_id.clone();
        if self.conversation.queue.thread_id == thread_id {
            return;
        }
        self.conversation.queue.reset(thread_id.clone());
        self.conversation.goal.reset(thread_id.clone());
        self.conversation.memory.reset(thread_id.clone());
        self.queue_edit = None;
        self.queue_removal = None;
        self.queue_redo = None;
        self.queue_menu = None;
        self.queue_drag = None;
        self.queue_resumed = false;
        self.pending_external_turns.clear();
        if let Some(thread_id) = thread_id {
            if !self.side_chat {
                self.conversation.queue.wanted = 1;
                self.refresh_queue(cx);
            }
            self.load_goal(thread_id, cx);
        }
    }

    /// Lists the queue if a change is outstanding; repeats until the answer
    /// covers the latest `thread/queue/changed`.
    pub(super) fn refresh_queue(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.conversation.queue.thread_id.clone() else {
            return;
        };
        let Some(revision) = self.conversation.queue.begin_list() else {
            return;
        };
        let receiver = self.backend.list_thread_queue(thread_id);
        cx.spawn(async move |this, cx| {
            let result = receiver
                .recv()
                .await
                .unwrap_or_else(|_| Err(crate::i18n::format!("队列读取在返回前中断" => "The queue read ended before it returned")));
            let _ = this.update(cx, |this, cx| {
                let again = this.conversation.queue.resolve_list(revision, result);
                if again {
                    this.refresh_queue(cx);
                }
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn queue_changed(
        &mut self,
        generation: u64,
        thread_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.side_chat || !self.conversation.queue.invalidate(generation, thread_id) {
            return;
        }
        self.refresh_queue(cx);
    }

    /// Queues a follow-up instead of steering. The draft snapshot is kept by
    /// its clientUserMessageId, so an edit restores attachments and comments.
    pub(super) fn submit_to_queue(
        &mut self,
        prompt: String,
        draft: SubmissionDraft,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some(thread_id), Some(generation)) =
            (self.conversation.thread_id.clone(), self.queue_generation())
        else {
            return false;
        };
        let client_message_id = crate::conversation::new_client_message_id();
        let receiver = self.backend.add_queued_submission(AgentQueueAddRequest {
            generation,
            thread_id: thread_id.clone(),
            client_message_id: client_message_id.clone(),
            prompt: prompt.clone(),
            context: draft.context.clone(),
        });
        self.conversation.queue.adding += 1;
        cx.spawn(async move |this, cx| {
            let result = receiver
                .recv()
                .await
                .unwrap_or_else(|_| Err(crate::i18n::format!("排队请求在返回前中断，消息未加入队列" => "The queue request ended before it returned; the message was not queued")));
            let _ = this.update(cx, |this, cx| {
                this.conversation.queue.adding = this.conversation.queue.adding.saturating_sub(1);
                if this.conversation.queue.thread_id.as_deref() != Some(thread_id.as_str()) {
                    return;
                }
                match result {
                    Ok(submission) => {
                        this.conversation
                            .queue
                            .insert_added(submission, draft, prompt);
                        this.queue_resumed = false;
                    }
                    Err(error) => {
                        // Never lose the user's input: put it back unless the
                        // composer already holds a newer draft.
                        this.submission_error = Some(error);
                        if this.draft_is_empty(cx) {
                            this.restore_draft(draft, cx);
                        }
                    }
                }
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
        true
    }

    pub(super) fn queue_target(&self, id: Option<String>) -> Option<AgentQueueTarget> {
        Some(AgentQueueTarget {
            generation: self.queue_generation()?,
            thread_id: self.conversation.queue.thread_id.clone()?,
            queued_submission_id: id,
        })
    }

    /// Sends one queued message now. While a turn runs it steers that turn
    /// with the row's clientUserMessageId and then deletes the row (a missing
    /// row is an error, as in the reference); on an idle thread it starts it.
    pub(crate) fn send_queued_now(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(row) = self
            .conversation
            .queue
            .row_mut(id)
            .map(|row| row.submission.clone())
        else {
            return;
        };
        let Some(target) = self.queue_target(Some(id.to_owned())) else {
            return;
        };
        let running = matches!(
            self.conversation.phase,
            ConversationPhase::Thinking | ConversationPhase::Streaming
        );
        let operation = if running {
            QueueRowOperation::Steering
        } else {
            QueueRowOperation::Starting
        };
        if !self.conversation.queue.begin_row(id, operation) {
            return;
        }
        let id = id.to_owned();
        if running {
            let Ok(turn) = self.conversation.steer_target() else {
                self.conversation.queue.finish_row(&id, None);
                return;
            };
            let draft = self
                .conversation
                .queue
                .drafts
                .get(&row.client_message_id)
                .cloned();
            let (prompt, context) = row_input(&row, draft.as_ref());
            let display = crate::agent::normalize_user_message_for_display(&prompt);
            self.conversation.record_submission_with_id(
                row.client_message_id.clone(),
                draft
                    .map(|(draft, _)| draft)
                    .unwrap_or_else(|| SubmissionDraft {
                        text: row.text.clone(),
                        context: context.clone(),
                        comments: Vec::new(),
                    }),
                display,
            );
            let steer = self.backend.steer_turn(crate::agent::AgentSteerRequest {
                target: turn,
                client_message_id: row.client_message_id.clone(),
                prompt,
                context,
            });
            let backend = self.backend.clone();
            let client_message_id = row.client_message_id.clone();
            cx.spawn(async move |this, cx| {
                let steered = steer.recv().await.unwrap_or_else(|_| {
                    Err(crate::i18n::format!("引导响应连接已关闭，接受状态未知" => "The steer response closed; acceptance is unknown"))
                });
                let result = match steered.clone() {
                    Ok(()) => backend
                        .delete_queued_submission(target)
                        .recv()
                        .await
                        .unwrap_or_else(|_| Err(crate::i18n::format!("删除排队消息的响应在返回前中断" => "The queue delete ended before it returned")))
                        .and_then(|deleted| {
                            deleted.then_some(()).ok_or_else(|| {
                                crate::i18n::format!("已引导，但服务端队列中找不到这条消息" => "Steered, but the server queue no longer had this message")
                            })
                        }),
                    Err(error) => Err(error),
                };
                let _ = this.update(cx, |this, cx| {
                    this.conversation.resolve_submission(&client_message_id, steered);
                    this.conversation.queue.finish_row(&id, result.err());
                    this.conversation.queue.wanted += 1;
                    this.refresh_queue(cx);
                    cx.emit(ConversationChanged);
                    cx.notify();
                });
            })
            .detach();
        } else {
            let receiver = self.backend.start_queued_submission(target);
            self.queue_resumed = true;
            cx.spawn(async move |this, cx| {
                let result = receiver.recv().await.unwrap_or_else(|_| {
                    Err(crate::i18n::format!("发送排队消息的响应在返回前中断" => "Starting the queued message ended before it returned"))
                });
                let _ = this.update(cx, |this, cx| {
                    let failed = result.is_err();
                    this.conversation.queue.finish_row(&id, result.err());
                    if failed {
                        this.queue_resumed = false;
                    }
                    this.conversation.queue.wanted += 1;
                    this.refresh_queue(cx);
                    cx.emit(ConversationChanged);
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// Resumes a paused queue by starting its first message.
    pub(crate) fn resume_queue(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self
            .conversation
            .queue
            .rows
            .first()
            .map(|row| row.submission.id.clone())
        {
            self.send_queued_now(&id, cx);
        }
    }

    /// "Delete": no toast; ⌘Z within a minute puts it back.
    pub(crate) fn delete_queued(&mut self, id: &str, cx: &mut Context<Self>) {
        let record = self.removal_record(id, false);
        self.queue_redo = None;
        self.remove_queued(
            id,
            record,
            |this, record, _| this.queue_removal = Some(record),
            cx,
        );
    }

    /// Deletes every queued message, for "Clear queue".
    pub(super) fn clear_queue(&mut self, cx: &mut Context<Self>) {
        let ids = self.conversation.queue.order();
        for id in ids {
            self.delete_queued(&id, cx);
        }
    }

    /// Moves a row and sends the complete new order. A failed reorder
    /// re-lists, so the tray never keeps an order the server rejected.
    pub(crate) fn reorder_queued(&mut self, id: &str, to: usize, cx: &mut Context<Self>) {
        let Some(generation) = self.queue_generation() else {
            return;
        };
        let Some(thread_id) = self.conversation.queue.thread_id.clone() else {
            return;
        };
        if self.conversation.queue.reorder_pending {
            return;
        }
        let Some(order) = self.conversation.queue.move_row(id, to) else {
            return;
        };
        self.conversation.queue.reorder_pending = true;
        let receiver = self
            .backend
            .reorder_queued_submissions(AgentQueueReorderRequest {
                generation,
                thread_id,
                queued_submission_ids: order,
            });
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::i18n::format!("排序请求在返回前中断" => "The reorder ended before it returned"))
            });
            let _ = this.update(cx, |this, cx| {
                this.conversation.queue.reorder_pending = false;
                if let Err(error) = result {
                    this.submission_error = Some(error);
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

    /// "Turn off queueing" / "Turn on queueing" from the row menu.
    pub(crate) fn toggle_queueing(&mut self, cx: &mut Context<Self>) {
        self.queue_menu = None;
        let mode = self.follow_up_mode.inverted();
        self.follow_up_mode = mode;
        cx.emit(FollowUpModeToggled(mode));
        cx.notify();
    }

    /// Offers a server-started turn to this conversation. It is attached when
    /// the composer is idle; otherwise it waits for the current run to end,
    /// because the two event streams are separate channels.
    pub(super) fn offer_external_turn(
        &mut self,
        generation: u64,
        thread_id: &str,
        run: &crate::agent::AgentExternalTurn,
        cx: &mut Context<Self>,
    ) {
        if self.conversation.thread_id.as_deref() != Some(thread_id)
            || generation < self.conversation.runtime.generation
        {
            return;
        }
        let Some(run) = run.take() else {
            return;
        };
        self.pending_external_turns.push_back(run);
        self.attach_pending_external_turn(cx);
    }

    pub(super) fn attach_pending_external_turn(&mut self, cx: &mut Context<Self>) {
        if self.is_running() {
            return;
        }
        let Some(run) = self.pending_external_turns.pop_front() else {
            return;
        };
        self.attach_external_turn(run, cx);
    }

    pub(super) fn attach_external_turn(&mut self, run: AgentRun, cx: &mut Context<Self>) {
        let objective = self.pending_goal_bubble.take();
        let cycle = self.conversation.begin_external_turn(objective);
        self.queue_resumed = true;
        let (receiver, interrupt) = run.into_parts();
        self.conversation.active_turn = interrupt;
        self.consume_agent_events(receiver, cycle, cx);
        cx.emit(ConversationChanged);
        cx.notify();
    }
}
