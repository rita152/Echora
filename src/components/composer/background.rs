//! Background terminals of the conversation: the summary panel's stop button
//! and the stop fallback clean them through `thread/backgroundTerminals/clean`.
//! Only one clean runs at a time; a failure is shown once and never retried.
//! The commands' final state comes from the server.

use gpui::Context;

use super::{ComposerView, toast::ToastKind};
use crate::{
    agent::CommandExecution,
    conversation::{BackgroundCleanState, BackgroundTerminal, ConversationActivity},
};

impl ComposerView {
    pub(crate) fn background_terminals(&self) -> Vec<BackgroundTerminal> {
        self.conversation.background_terminals()
    }

    pub(crate) fn background_clean_state(&self) -> &BackgroundCleanState {
        &self.conversation.background.clean
    }

    /// The command item behind a background terminal, wherever its turn is.
    pub(crate) fn background_command(&self, item_id: &str) -> Option<CommandExecution> {
        self.conversation
            .activities
            .iter()
            .chain(
                self.conversation
                    .transcript
                    .iter()
                    .flat_map(|turn| turn.activities.iter()),
            )
            .find_map(|activity| match activity {
                ConversationActivity::Command(command) if command.id == item_id => {
                    Some(command.clone())
                }
                _ => None,
            })
    }

    /// Stops every background terminal of the thread. `clicked_item_id` is the
    /// row whose stop button was pressed (it shows the spinner); failures of a
    /// click are reported with the reference's toast, failures of the stop
    /// fallback only logged, as the reference does.
    pub(crate) fn clean_background_terminals(
        &mut self,
        clicked_item_id: Option<String>,
        report_failure: bool,
        cx: &mut Context<Self>,
    ) {
        let (Some(thread_id), Some(generation)) = (
            self.conversation.thread_id.clone(),
            self.conversation
                .turn_identity
                .as_ref()
                .map(|identity| identity.generation),
        ) else {
            return;
        };
        if !self.conversation.begin_background_clean(clicked_item_id) {
            return;
        }
        cx.notify();
        let receiver = self
            .backend
            .clean_background_terminals(thread_id, generation);
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::i18n::text("后台终端清理的响应通道提前关闭").to_owned())
            });
            let _ = this.update(cx, |this, cx| {
                let failed = result.is_err();
                if let Err(error) = &result {
                    eprintln!("thread/backgroundTerminals/clean failed: {error}");
                }
                this.conversation.finish_background_clean(result);
                if failed {
                    if report_failure {
                        this.show_toast(
                            ToastKind::Danger,
                            crate::i18n::format!("无法停止后台终端" => "Unable to stop background terminals"),
                            cx,
                        );
                    }
                    this.conversation.acknowledge_background_clean_failure();
                }
                cx.emit(super::ConversationChanged);
                cx.notify();
            });
        })
        .detach();
    }

    /// Command cards that read as background terminals, by item id.
    pub(crate) fn background_marks(
        &self,
    ) -> std::collections::HashMap<String, crate::conversation::BackgroundMark> {
        self.conversation.background_marks()
    }
}

#[cfg(any(test, feature = "screenshot"))]
impl ComposerView {
    /// Appends a finished turn that left these commands behind, after the
    /// chat's current turn; a chat without a thread gets `thread_id`.
    pub(crate) fn push_background_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        prompt: &str,
        commands: Vec<CommandExecution>,
    ) {
        use crate::conversation::{ConversationPhase, ConversationTranscriptTurn};
        if self.conversation.thread_id.is_none() {
            self.conversation.thread_id = Some(thread_id.to_owned());
        }
        if self.conversation.turn_identity.is_none() {
            self.conversation.turn_identity = Some(crate::agent::AgentTurnIdentity {
                generation: 1,
                thread_id: thread_id.to_owned(),
                turn_id: turn_id.to_owned(),
            });
        }
        self.conversation.commit_current_turn();
        self.conversation
            .transcript
            .push(ConversationTranscriptTurn {
                turn_id: Some(turn_id.to_owned()),
                phase: ConversationPhase::Complete,
                user_message: prompt.to_owned(),
                user_images: Vec::new(),
                user_message_time: None,
                assistant_message: "ok".to_owned(),
                assistant_message_time: None,
                activities: commands
                    .into_iter()
                    .map(ConversationActivity::Command)
                    .collect(),
                resumed: None,
                goal: Default::default(),
            });
        self.conversation.phase = ConversationPhase::Complete;
    }
}

#[cfg(test)]
impl ComposerView {
    /// A thread whose finished turn left these commands behind.
    pub(crate) fn seed_background_turn_for_test(
        &mut self,
        thread_id: &str,
        commands: Vec<CommandExecution>,
    ) {
        self.push_background_turn(thread_id, "turn-1", "start", commands);
    }
}

#[cfg(feature = "screenshot")]
impl ComposerView {
    /// `--batch4-state=background:<state>`: the reference's `BGTERM-LONG`
    /// turn (a command still running in a background terminal), stopping,
    /// a failed stop, stopped, or the three turns of the finished capture.
    pub(crate) fn set_background_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        use crate::agent::{AgentEvent, CommandExecutionSource, CommandExecutionStatus};
        const LONG: &str = "echo bg-start; sleep 1800; echo bg-end";
        const SHORT: &str = "echo bg-start; sleep 2; echo bg-mid; sleep 1; echo bg-end";
        let Some(thread_id) = self.conversation.thread_id.clone() else {
            return;
        };
        let cwd = self.conversation.cwd.to_string_lossy().into_owned();
        let command = |id: &str, text: &str, output: &str, status, exit_code| CommandExecution {
            id: id.to_owned(),
            command: text.to_owned(),
            actions: Vec::new(),
            cwd: cwd.clone(),
            output: output.to_owned(),
            terminal_process_id: Some(format!("{}", 40_000 + id.len())),
            status,
            exit_code,
            source: CommandExecutionSource::UnifiedExecStartup,
            timed_out: false,
        };
        // A tty exec streams nothing until it ends: the reference's tab read
        // "No output yet" while the command ran.
        let running = |id: &str| command(id, LONG, "", CommandExecutionStatus::InProgress, None);
        let stopped = |id: &str| {
            command(
                id,
                LONG,
                "bg-start\n",
                CommandExecutionStatus::Failed,
                Some(-1),
            )
        };
        match state {
            "finished" => {
                self.push_background_turn(
                    &thread_id,
                    "capture-long-1",
                    "BGTERM-LONG start",
                    vec![stopped("capture-bg-1")],
                );
                self.push_background_turn(
                    &thread_id,
                    "capture-long-2",
                    "BGTERM-LONG start",
                    vec![stopped("capture-bg-2")],
                );
                self.push_background_turn(
                    &thread_id,
                    "capture-short",
                    "BGTERM-SHORT start",
                    vec![command(
                        "capture-bg-3",
                        SHORT,
                        "bg-start\nbg-mid\nbg-end\n",
                        CommandExecutionStatus::Completed,
                        Some(0),
                    )],
                );
            }
            "stopped" => {
                self.push_background_turn(
                    &thread_id,
                    "capture-long-1",
                    "BGTERM-LONG start",
                    vec![running("capture-bg-1")],
                );
                // The real sequence: the clean succeeds, then the server
                // completes the item with exit code -1.
                self.conversation
                    .begin_background_clean(Some("capture-bg-1".to_owned()));
                self.conversation.finish_background_clean(Ok(()));
                self.conversation.apply_background_command_event(
                    "capture-long-1",
                    AgentEvent::CommandCompleted(stopped("capture-bg-1")),
                );
            }
            _ => {
                self.push_background_turn(
                    &thread_id,
                    "capture-long-1",
                    "BGTERM-LONG start",
                    vec![running("capture-bg-1")],
                );
                match state {
                    "stopping" => {
                        self.conversation.begin_background_clean(Some("capture-bg-1".to_owned()));
                    }
                    "stop-failed" => self.show_toast(
                        ToastKind::Danger,
                        crate::i18n::format!("无法停止后台终端" => "Unable to stop background terminals"),
                        cx,
                    ),
                    _ => {}
                }
            }
        }
        cx.emit(super::ConversationChanged);
        cx.notify();
    }
}
