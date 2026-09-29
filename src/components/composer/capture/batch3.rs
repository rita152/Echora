//! Deterministic `/review`, review-turn, `!` shell mode and `/memories`
//! status states. The fixtures write the conversation state directly and
//! never call the backend. The chat matches the reference capture in
//! artifacts/batch3-review-* ("hello" in the GPUI project).

use super::super::{dialogs::ComposerDialog, review::ReviewBranches};
use super::{ComposerView, ConversationChanged};
use crate::{
    agent::{
        AgentMemoryStatus, AgentTurnIdentity, CommandExecution, CommandExecutionAction,
        CommandExecutionSource, CommandExecutionStatus,
    },
    conversation::{ConversationActivity, ConversationPhase},
    git_review::Checkout,
};
use gpui::Context;

const THREAD: &str = "capture-batch3";
const TURN: &str = "capture-batch3-turn";
const GENERATION: u64 = 1_000_000;
const PROMPT: &str = "hello";
const REPLY: &str = "Hey! I'm here and have the Echora repo context loaded. What are we working on today — a feature, a bug, UI polish, or something else?";
const REVIEW_RESULT: &str = "One small formatting regression.\n\nReview comment:\n\n- [P2] Greeting drops the trailing newline — hello.txt:1-1\n  `hello.txt` now ends without a newline, which breaks `cat` concatenation.";

impl ComposerView {
    fn begin_batch3_capture(&mut self, cx: &mut Context<Self>) {
        self.permission_catalog_cycle = self.permission_catalog_cycle.wrapping_add(1);
        self.permission_read_cycle = self.permission_read_cycle.wrapping_add(1);
        self.permission_catalog_loading = false;
        self.permission_effective_loading = false;
        self.memories_feature = Some((self.conversation.runtime.generation, true));
        self.set_checkout(Checkout::Branch("main".into()), cx);
        self.conversation.thread_id = Some(THREAD.into());
        self.conversation.queue.reset(Some(THREAD.into()));
        self.conversation.goal.reset(Some(THREAD.into()));
        self.conversation.memory.reset(Some(THREAD.into()));
        self.conversation.begin_prompt(PROMPT);
        self.conversation.turn_id = Some(TURN.into());
        self.conversation.user_message_time = None;
        self.conversation.turn_identity = Some(AgentTurnIdentity {
            generation: GENERATION,
            thread_id: THREAD.into(),
            turn_id: TURN.into(),
        });
        self.conversation.assistant_message = REPLY.into();
        self.conversation.phase = ConversationPhase::Complete;
    }

    fn type_for_capture(&mut self, text: &str, cx: &mut Context<Self>) {
        self.prompt_editor.update(cx, |editor, cx| {
            let len = editor.text().len();
            editor.replace_range(0..len, text, cx);
        });
        self.update_slash_menu(cx);
    }

    /// `slash` (`/review` with the Code review row), `submenu`,
    /// `submenu-branch` (the first branch highlighted), `loading`, `failed`,
    /// `escaped` (the menu closed, `/` left). Branches are this repository's
    /// own, like the reference's.
    pub fn set_review_menu_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.begin_batch3_capture(cx);
        self.type_for_capture("/review", cx);
        if state != "slash" {
            self.slash_menu_enter(cx);
            let branches = match state {
                "loading" => ReviewBranches::Loading,
                "failed" => ReviewBranches::Failed,
                _ => crate::git_review::review_branches(&self.conversation.cwd)
                    .map_or(ReviewBranches::Failed, ReviewBranches::Loaded),
            };
            // A read the menu started must not replace the fixture.
            self.review_branches_cycle = self.review_branches_cycle.wrapping_add(1);
            self.set_review_branches(branches, cx);
            match state {
                "submenu-branch" => {
                    self.slash_menu_key("down", false, cx);
                }
                "escaped" => {
                    self.slash_menu_key("escape", false, cx);
                }
                _ => {}
            }
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    /// `running` (the review request with its mark, still thinking) and
    /// `finished` (the review result as the answer).
    pub fn set_review_turn_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.begin_batch3_capture(cx);
        let request = crate::conversation::review_request_text(
            &crate::agent::AgentReviewTarget::UncommittedChanges,
            Some("main"),
        );
        self.conversation.begin_review(&request);
        self.conversation.user_message_time = None;
        self.conversation.turn_id = Some(format!("{TURN}-review"));
        self.conversation.turn_identity = Some(AgentTurnIdentity {
            generation: GENERATION,
            thread_id: THREAD.into(),
            turn_id: format!("{TURN}-review"),
        });
        if state == "finished" {
            self.conversation.activities = vec![ConversationActivity::AssistantMessage {
                item_id: "capture-review-result".into(),
                text: REVIEW_RESULT.into(),
            }];
            self.conversation.assistant_message = REVIEW_RESULT.into();
            self.conversation
                .completed_assistant_messages
                .insert("capture-review-result".into());
            self.conversation.phase = ConversationPhase::Complete;
        } else {
            self.conversation.phase = ConversationPhase::Thinking;
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    /// `typing` (`!git status` with the sandbox warning), `running`,
    /// `completed`, `failed`, `timeout`, `interrupted`: the command's own turn.
    pub fn set_shell_mode_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.begin_batch3_capture(cx);
        if state == "typing" {
            self.type_for_capture("!git status", cx);
            cx.emit(ConversationChanged);
            cx.notify();
            return;
        }
        self.conversation.begin_external_turn(None);
        self.conversation.turn_id = Some(format!("{TURN}-shell"));
        let (status, exit_code, output, timed_out, phase) = match state {
            "running" => (
                CommandExecutionStatus::InProgress,
                None,
                "tick 1\ntick 2\n",
                false,
                ConversationPhase::Streaming,
            ),
            "failed" => (
                CommandExecutionStatus::Failed,
                Some(3),
                "to-stderr\n",
                false,
                ConversationPhase::Complete,
            ),
            "timeout" => (
                CommandExecutionStatus::Failed,
                Some(-1),
                "before\n",
                true,
                ConversationPhase::Complete,
            ),
            "interrupted" => (
                CommandExecutionStatus::Failed,
                None,
                "started\n",
                false,
                ConversationPhase::Stopped,
            ),
            _ => (
                CommandExecutionStatus::Completed,
                Some(0),
                "On branch main\nnothing to commit, working tree clean\n",
                false,
                ConversationPhase::Complete,
            ),
        };
        let command = match state {
            "running" => "for i in 1 2 3; do echo tick $i; sleep 1; done",
            "failed" => "echo to-stderr >&2; exit 3",
            "timeout" => "echo before; sleep 5; echo never",
            "interrupted" => "echo started; sleep 20",
            _ => "git status",
        };
        self.conversation.activities = vec![ConversationActivity::Command(CommandExecution {
            id: "capture-shell".into(),
            command: command.into(),
            actions: vec![CommandExecutionAction::Unknown {
                command: command.into(),
            }],
            cwd: self.conversation.cwd.display().to_string(),
            output: output.into(),
            terminal_process_id: None,
            status,
            exit_code,
            source: CommandExecutionSource::UserShell,
            timed_out,
        })];
        self.conversation.phase = phase;
        cx.emit(ConversationChanged);
        cx.notify();
    }

    /// `/memories` for a started chat with the consolidation line: `ready`
    /// or `pending`.
    pub fn set_memory_status_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.begin_batch3_capture(cx);
        self.memory_status = Some(AgentMemoryStatus {
            generation: GENERATION,
            v2_ready: state == "ready",
            consolidated_threads: if state == "ready" { 25 } else { 3 },
            required_threads: crate::agent::MEMORY_V2_REQUIRED_THREADS,
        });
        self.dialog = Some(ComposerDialog::Memories);
        cx.emit(ConversationChanged);
        cx.notify();
    }
}
