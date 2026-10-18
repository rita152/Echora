//! One action decision shared by the send button's rendering and activation.
use super::ComposerView;
use crate::conversation::{ConversationPhase, SubmissionStatus};
use gpui::{App, Context};
use std::time::{Duration, Instant};

pub(super) struct SendOpacity {
    from: f32,
    target: f32,
    started: Option<Instant>,
}

impl Default for SendOpacity {
    fn default() -> Self {
        Self {
            from: 0.5,
            target: 0.5,
            started: None,
        }
    }
}

impl SendOpacity {
    const DURATION: Duration = Duration::from_millis(150);

    pub(super) fn value(&self, now: Instant) -> f32 {
        let Some(started) = self.started else {
            return self.target;
        };
        let progress = (now.saturating_duration_since(started).as_secs_f32()
            / Self::DURATION.as_secs_f32())
        .clamp(0.0, 1.0);
        let eased = crate::components::markdown::cubic_bezier_ease(progress, 0.4, 0.0, 0.2, 1.0);
        self.from + (self.target - self.from) * eased
    }

    pub(super) fn update(&mut self, action: SendAction, now: Instant, reduce_motion: bool) -> bool {
        let target = if action.enabled() || action == SendAction::Stopping {
            1.0
        } else {
            0.5
        };
        if target != self.target {
            self.from = self.value(now);
            self.target = target;
            self.started = Some(now);
        }
        if reduce_motion
            || self
                .started
                .is_some_and(|start| now.saturating_duration_since(start) >= Self::DURATION)
        {
            self.started = None;
        }
        self.started.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SendAction {
    Disabled,
    Send,
    Resume,
    Stop,
    Starting,
    Stopping,
    Sending,
}

impl SendAction {
    pub(super) fn enabled(self) -> bool {
        matches!(self, Self::Send | Self::Stop | Self::Resume)
    }

    pub(super) fn busy(self) -> bool {
        matches!(self, Self::Starting | Self::Stopping | Self::Sending)
    }

    pub(super) fn label(self) -> &'static str {
        crate::i18n::text(if self == Self::Resume {
            "继续"
        } else if matches!(self, Self::Stop | Self::Stopping) {
            "停止"
        } else {
            "发送"
        })
    }
}

impl ComposerView {
    pub(super) fn send_action(&self, cx: &App) -> SendAction {
        if self.workspace_controls.busy || self.workspace_controls.pending_send.is_some() {
            return SendAction::Starting;
        }
        if self.conversation.phase == ConversationPhase::Starting {
            return SendAction::Starting;
        }
        if self.conversation.phase == ConversationPhase::Stopping {
            return SendAction::Stopping;
        }
        let empty = self.prompt_text(cx).trim().is_empty()
            && self.review_comments.is_empty()
            && self.prompt_context.files.is_empty();
        if self.is_running() && empty {
            return SendAction::Stop;
        }
        if self.conversation.queue.adding > 0
            || self.conversation.submissions.iter().any(|submission| {
                submission.cycle == self.conversation.cycle
                    && submission.status == SubmissionStatus::Sending
            })
        {
            return SendAction::Sending;
        }
        if !self.side_ready
            || (self.conversation.thread_id.is_none()
                && (self.permission_catalog_loading || self.permission_catalog_error.is_some()))
            || (!self.is_running()
                && (self.conversation.selected_model.is_empty()
                    || self.conversation.permission_change.is_some()))
            || (self.is_running()
                && !self.queue_supported()
                && self.conversation.steer_target().is_err())
        {
            SendAction::Disabled
        } else if empty
            && self.conversation.phase == ConversationPhase::Stopped
            && self.conversation.thread_id.is_some()
        {
            SendAction::Resume
        } else if empty {
            SendAction::Disabled
        } else {
            SendAction::Send
        }
    }

    pub(super) fn activate_send_action(&mut self, cx: &mut Context<Self>) {
        match self.send_action(cx) {
            SendAction::Send => self.submit_current_prompt(cx),
            SendAction::Stop => self.stop_generation(cx),
            SendAction::Resume if self.queue_paused() => self.resume_queue(cx),
            SendAction::Resume => self.submit_current_prompt(cx),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_reverses_without_a_jump_and_respects_reduced_motion() {
        let now = Instant::now();
        let mut opacity = SendOpacity::default();
        assert!(!opacity.update(SendAction::Disabled, now, false));
        assert_eq!(opacity.value(now), 0.5);
        assert!(opacity.update(SendAction::Send, now, false));
        let middle = now + Duration::from_millis(75);
        let before = opacity.value(middle);
        assert!(before > 0.5 && before < 1.0);
        assert!(opacity.update(SendAction::Starting, middle, false));
        assert_eq!(opacity.value(middle), before);
        assert!(!opacity.update(SendAction::Starting, middle + SendOpacity::DURATION, false));
        assert_eq!(opacity.value(middle + SendOpacity::DURATION), 0.5);
        assert!(!opacity.update(SendAction::Stopping, middle, true));
        assert_eq!(opacity.value(middle), 1.0);
    }
}
