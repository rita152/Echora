//! Prompt preparation and interruption independently of UI focus and menus.

use super::{
    state::ConversationState,
    transcript::{ConversationPhase, current_local_time_label},
};
use crate::agent::{
    AgentEvent, AgentInterruptHandle, AgentInterruptOutcome, normalize_user_message_for_display,
};

impl ConversationState {
    pub(crate) fn begin_prompt(&mut self, prompt: &str) -> u64 {
        self.commit_current_turn();
        self.turn_id = None;
        self.resumed_turn = None;
        self.assistant_message_phases.clear();
        self.completed_assistant_messages.clear();
        self.turn_identity = None;
        self.seen_user_items.clear();
        self.history_loading = false;
        self.history_error = None;
        self.user_message = Some(normalize_user_message_for_display(prompt));
        self.user_message_goal = false;
        self.user_message_review = false;
        self.goal_achieved_seconds = None;
        self.external_turn = false;
        self.user_message_time = Some(current_local_time_label());
        self.assistant_message.clear();
        // Elicitation cards belong to the connection, not to the turn that
        // observed them: a new prompt must not drop a request the server is
        // still waiting on.
        self.retain_pending_mcp_elicitations();
        self.approval_responders.clear();
        self.command_approval_requests.clear();
        self.file_approval_responders.clear();
        self.file_changes.clear();
        self.user_input_responders.clear();
        self.permissions_approval_responders.clear();
        self.server_request_contexts.clear();
        self.assistant_message_time = None;
        self.phase = ConversationPhase::Starting;
        self.cycle = self.cycle.wrapping_add(1);
        self.cycle
    }

    /// A turn the server started on its own (goal continuation, queue
    /// advance, `thread/queue/start`). The previous turn is committed like for
    /// a prompt; the user message, if the turn has one, arrives as its item.
    /// `goal_objective` shows a goal's objective as the turn's request, as the
    /// reference inserts `/goal <objective>` for the turn it starts.
    pub(crate) fn begin_external_turn(&mut self, goal_objective: Option<String>) -> u64 {
        let cycle = self.begin_prompt("");
        // An empty message still commits the turn later; the view shows no
        // bubble for it, as it does for restored turns without a user item.
        self.user_message_goal = goal_objective.is_some();
        if goal_objective.is_none() {
            self.user_message_time = None;
        }
        self.user_message = Some(goal_objective.unwrap_or_default());
        self.external_turn = true;
        cycle
    }

    /// A code review this client starts: `request` is shown as the user's
    /// message with the "Review mode" mark.
    pub(crate) fn begin_review(&mut self, request: &str) -> u64 {
        let cycle = self.begin_prompt(request);
        self.user_message_review = true;
        cycle
    }

    /// Marks the turn that completed the goal ("Goal achieved in …"): the
    /// named turn, or the newest one when the update names none.
    pub(crate) fn mark_goal_achieved(&mut self, turn_id: Option<&str>, seconds: i64) {
        let current = turn_id.is_none_or(|id| self.turn_id.as_deref() == Some(id));
        if current {
            self.goal_achieved_seconds = Some(seconds);
        } else if let Some(turn) = self
            .transcript
            .iter_mut()
            .rev()
            .find(|turn| turn.turn_id.as_deref() == turn_id)
        {
            turn.goal.achieved_seconds = Some(seconds);
        }
    }

    pub(crate) fn clear_goal_achieved_marks(&mut self) {
        self.goal_achieved_seconds = None;
        for turn in &mut self.transcript {
            turn.goal.achieved_seconds = None;
        }
    }

    /// Removes a history turn that is still running on the server, so its
    /// live stream (replayed from the start) rebuilds it without a duplicate.
    pub(crate) fn drop_history_turn(&mut self, turn_id: &str) {
        self.transcript
            .retain(|turn| turn.turn_id.as_deref() != Some(turn_id));
        if self.turn_id.as_deref() != Some(turn_id) {
            return;
        }
        if let Some(last) = self.transcript.pop() {
            self.turn_id = last.turn_id;
            self.phase = last.phase;
            self.user_message = Some(last.user_message);
            self.user_images = last.user_images;
            self.user_message_time = last.user_message_time;
            self.assistant_message = last.assistant_message;
            self.assistant_message_time = last.assistant_message_time;
            self.activities = last.activities;
            self.resumed_turn = last.resumed;
            self.user_message_goal = last.goal.sent_as_goal;
            self.goal_achieved_seconds = last.goal.achieved_seconds;
            self.user_message_review = last.goal.review_request;
        } else {
            self.turn_id = None;
            self.phase = super::transcript::ConversationPhase::Empty;
            self.user_message = None;
            self.user_images.clear();
            self.user_message_time = None;
            self.assistant_message.clear();
            self.assistant_message_time = None;
            self.activities.clear();
            self.resumed_turn = None;
        }
    }

    pub(crate) fn stop_generation(&mut self) -> bool {
        if matches!(
            self.phase,
            ConversationPhase::Starting
                | ConversationPhase::Thinking
                | ConversationPhase::Streaming
                | ConversationPhase::Stopping
        ) {
            let result = self
                .active_turn
                .as_ref()
                .map(AgentInterruptHandle::interrupt)
                .unwrap_or_else(|| {
                    Err(crate::i18n::text("当前 Codex turn 没有可用的中断连接").to_owned())
                });
            match result {
                Ok(AgentInterruptOutcome::Requested | AgentInterruptOutcome::AlreadyRequested) => {
                    self.phase = ConversationPhase::Stopping;
                }
                Ok(AgentInterruptOutcome::AlreadyFinished) => {}
                Err(error) => {
                    self.apply_agent_event_batch(vec![AgentEvent::Failed(crate::i18n::format!(
                        "无法中断 Codex turn：{error}" => "Could not interrupt Codex turn: {error}"
                    ))]);
                }
            }
            true
        } else {
            false
        }
    }
}
