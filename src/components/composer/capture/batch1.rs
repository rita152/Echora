//! Deterministic follow-up queue, goal and auto-review denial states. The
//! fixtures write the conversation state directly and never call the backend,
//! so no queue, goal, approval or model request is sent. Texts match the
//! reference captures in artifacts/batch1-*.

use super::{ComposerView, ConversationChanged};
use crate::{
    agent::*,
    conversation::{ConversationPhase, QueueRowOperation},
};
use gpui::Context;

const THREAD: &str = "capture-batch1";
const TURN: &str = "capture-batch1-turn";
const GENERATION: u64 = 1_000_000;

fn queued(id: &str, text: &str) -> AgentQueuedSubmission {
    AgentQueuedSubmission {
        id: id.into(),
        client_message_id: format!("client-{id}"),
        text: text.into(),
        attachments: Vec::new(),
    }
}

impl ComposerView {
    /// A synthetic thread with one turn, detached from every backend read.
    fn begin_batch1_capture(&mut self, prompt: &str, running: bool) {
        self.permission_catalog_cycle = self.permission_catalog_cycle.wrapping_add(1);
        self.permission_read_cycle = self.permission_read_cycle.wrapping_add(1);
        self.permission_catalog_loading = false;
        self.permission_effective_loading = false;
        self.permission_catalog_error = None;
        self.conversation.thread_id = Some(THREAD.into());
        self.conversation.queue.reset(Some(THREAD.into()));
        self.conversation.goal.reset(Some(THREAD.into()));
        self.conversation.queue.generation = Some(GENERATION);
        self.conversation.goal.generation = Some(GENERATION);
        self.conversation.begin_prompt(prompt);
        self.conversation.turn_id = Some(TURN.into());
        self.conversation.user_message_time = None;
        self.conversation.turn_identity = Some(AgentTurnIdentity {
            generation: GENERATION,
            thread_id: THREAD.into(),
            turn_id: TURN.into(),
        });
        self.conversation.phase = if running {
            ConversationPhase::Thinking
        } else {
            ConversationPhase::Complete
        };
    }

    fn set_queue_rows(&mut self, rows: &[(&str, &str)]) {
        let queue = &mut self.conversation.queue;
        queue.wanted = 1;
        let revision = queue.begin_list().expect("fixture list");
        queue.resolve_list(
            revision,
            Ok(AgentThreadQueue {
                generation: GENERATION,
                thread_id: THREAD.into(),
                submissions: rows.iter().map(|(id, text)| queued(id, text)).collect(),
            }),
        );
    }

    /// `queued` (running, three rows), `paused` (stopped, one row), `failed`
    /// (a row whose send failed), `menu` (row actions open), `confirm` (send
    /// while paused), `editing` (a row loaded into the composer).
    pub fn set_queue_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.follow_up_mode = crate::workspace::FollowUpMode::Queue;
        match state {
            "paused" | "confirm" => {
                self.begin_batch1_capture(
                    "Run `sleep 30` in the shell, then reply with exactly: x",
                    false,
                );
                self.conversation.phase = ConversationPhase::Stopped;
                self.set_queue_rows(&[("q1", "Reply with exactly: y")]);
                if state == "confirm" {
                    self.dialog = Some(super::super::dialogs::ComposerDialog::SendWhilePaused {
                        text: "Reply with exactly: z".into(),
                        inverted: false,
                    });
                }
            }
            "failed" => {
                self.begin_batch1_capture(
                    "Run `sleep 40` in the shell, then reply with exactly: first",
                    true,
                );
                self.set_queue_rows(&[
                    ("q1", "Reply with exactly: A"),
                    ("q2", "Reply with exactly: B"),
                ]);
                self.conversation.queue.finish_row(
                    "q1",
                    Some(crate::i18n::format!("连接已变化" => "The connection changed")),
                );
            }
            "sending" => {
                self.begin_batch1_capture(
                    "Run `sleep 40` in the shell, then reply with exactly: first",
                    true,
                );
                self.set_queue_rows(&[
                    ("q1", "Reply with exactly: A"),
                    ("q2", "Reply with exactly: B"),
                ]);
                self.conversation
                    .queue
                    .begin_row("q1", QueueRowOperation::Steering);
            }
            _ => {
                self.begin_batch1_capture(
                    "Run `sleep 40` in the shell, then reply with exactly: first",
                    true,
                );
                self.set_queue_rows(&[
                    ("q1", "Reply with exactly: A"),
                    ("q2", "Reply with exactly: B"),
                    ("q3", "Reply with exactly: C"),
                ]);
                if state == "menu" {
                    self.queue_menu = Some("q1".into());
                }
                if state == "editing" {
                    self.open_queue_edit_for_capture("q2", cx);
                }
                if state == "restored" {
                    self.show_toast(
                        super::super::toast::ToastKind::Success,
                        crate::i18n::format!("已恢复队列中的消息" => "Queued message restored"),
                        cx,
                    );
                }
            }
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    /// `active`, `paused`, `blocked`, `usage-limited`, `budget-limited` show
    /// the summary; `chip` the composer's Goal mode; `replace` the replace
    /// confirmation; `complete` the
    /// finished goal turn ("Sent as goal", "Goal achieved in 3s"); `edit-tab`
    /// a paused goal whose Edit goal tab the shell opens.
    pub fn set_goal_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        let objective = "Run `sleep 45` in the shell, then reply with exactly: finished";
        let status = match state {
            "paused" | "replace" | "edit-tab" => Some(AgentThreadGoalStatus::Paused),
            "blocked" => Some(AgentThreadGoalStatus::Blocked),
            "usage-limited" => Some(AgentThreadGoalStatus::UsageLimited),
            "budget-limited" => Some(AgentThreadGoalStatus::BudgetLimited),
            "chip" | "complete" => None,
            _ => Some(AgentThreadGoalStatus::Active),
        };
        self.begin_batch1_capture(objective, status == Some(AgentThreadGoalStatus::Active));
        self.conversation.user_message_goal = true;
        if let Some(status) = status {
            self.conversation.goal.observe(
                GENERATION,
                AgentThreadGoal {
                    thread_id: THREAD.into(),
                    objective: objective.into(),
                    status,
                    token_budget: (status == AgentThreadGoalStatus::BudgetLimited)
                        .then_some(20_000),
                    tokens_used: if status == AgentThreadGoalStatus::BudgetLimited {
                        20_000
                    } else {
                        0
                    },
                    time_used_seconds: if status == AgentThreadGoalStatus::Active {
                        6
                    } else {
                        26
                    },
                    created_at: 1,
                    // The Edit goal tab counts minutes from here.
                    updated_at: if state == "edit-tab" {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(2, |elapsed| elapsed.as_secs() as i64)
                    } else {
                        2
                    },
                },
            );
        }
        match state {
            "chip" => self.set_goal_draft(true, cx),
            "replace" => {
                self.set_goal_draft(true, cx);
                self.dialog = Some(super::super::dialogs::ComposerDialog::ReplaceGoal {
                    objective: "Reply with: replaced".into(),
                });
            }
            "complete" => {
                self.begin_batch1_capture(
                    "Reply with exactly the word done. That reply fully achieves this goal.",
                    false,
                );
                self.conversation.transcript.clear();
                self.conversation.user_message_goal = true;
                self.conversation.assistant_message = "done".into();
                self.conversation.goal_achieved_seconds = Some(3);
            }
            _ => {}
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    /// A denied auto-review in a finished turn: `denied`, `approving`,
    /// `approved`, `failed`.
    pub fn set_auto_review_denial_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.begin_batch1_capture("Run exactly `git push --force origin main`.", false);
        let key = AgentAutoApprovalReviewKey {
            thread_id: THREAD.into(),
            turn_id: TURN.into(),
            review_id: "capture-review".into(),
        };
        self.conversation
            .apply_auto_approval_review(AgentAutoApprovalReview {
                key: key.clone(),
                target_item_id: None,
                action: AgentAutoApprovalReviewAction::Command {
                    command: "git push --force origin main".into(),
                    cwd: "/tmp/fixture-project".into(),
                    source: "unifiedExec".into(),
                },
                status: AgentAutoApprovalReviewStatus::Denied,
                rationale: Some("Force-pushing to the default branch is destructive.".into()),
                risk_level: Some("high".into()),
                user_authorization: Some("low".into()),
                started_at_ms: 1,
                completed_at_ms: Some(2),
                decision_source: Some("agent".into()),
                source: serde_json::json!({"reviewId": "capture-review"}),
            });
        self.conversation.runtime.generation = GENERATION;
        match state {
            "approving" => {
                self.conversation.begin_review_approval(&key);
            }
            "approved" => {
                self.conversation.begin_review_approval(&key);
                self.conversation.finish_review_approval(&key, Ok(()));
                self.show_toast(
                    super::super::toast::ToastKind::Success,
                    crate::i18n::format!("已记录批准" => "Approval recorded"),
                    cx,
                );
            }
            "failed" => {
                self.conversation.begin_review_approval(&key);
                self.conversation.finish_review_approval(
                    &key,
                    Err(crate::i18n::format!("连接已变化" => "The connection changed")),
                );
            }
            _ => {}
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    /// The goal the capture set, for the shell's Edit goal tab.
    pub fn capture_goal(&self) -> Option<AgentThreadGoal> {
        self.conversation.goal.goal.clone()
    }

    /// The slash menu: `menu` (all commands), `query` (`/go`), `approve` (the
    /// auto-review denial submenu with two denials), `compact-busy` (Compact
    /// chosen while a turn runs: the danger notice).
    pub fn set_slash_menu_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.begin_batch1_capture(
            "Run exactly `git push --force origin main`.",
            state == "compact-busy",
        );
        if state == "approve" {
            for (id, command, at) in [
                ("capture-older", "git push --force origin main", 2),
                ("capture-newer", "rm -rf build", 3),
            ] {
                self.conversation
                    .apply_auto_approval_review(AgentAutoApprovalReview {
                        key: AgentAutoApprovalReviewKey {
                            thread_id: THREAD.into(),
                            turn_id: TURN.into(),
                            review_id: id.into(),
                        },
                        target_item_id: None,
                        action: AgentAutoApprovalReviewAction::Command {
                            command: command.into(),
                            cwd: "/tmp/fixture-project".into(),
                            source: "unifiedExec".into(),
                        },
                        status: AgentAutoApprovalReviewStatus::Denied,
                        rationale: (id == "capture-older")
                            .then(|| "Force-pushing to the default branch is destructive.".into()),
                        risk_level: Some("high".into()),
                        user_authorization: Some("low".into()),
                        started_at_ms: 1,
                        completed_at_ms: Some(at),
                        decision_source: Some("agent".into()),
                        source: serde_json::json!({"reviewId": id}),
                    });
            }
        }
        let text = match state {
            "query" => "/go",
            "compact-busy" => "/compact",
            _ => "/",
        };
        self.prompt_editor.update(cx, |editor, cx| {
            let len = editor.text().len();
            editor.replace_range(0..len, text, cx);
        });
        self.update_slash_menu(cx);
        if state == "approve" {
            self.select_slash_command(super::super::slash_menu::SlashCommand::Approve, cx);
        }
        if state == "compact-busy" {
            self.select_slash_command(super::super::slash_menu::SlashCommand::Compact, cx);
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
}
