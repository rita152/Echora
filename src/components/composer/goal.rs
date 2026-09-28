//! Thread goals in the composer: the typed `/goal` command, the Goal chip,
//! the summary above the composer, and pausing a goal before a user stop.

use std::time::Duration;

use gpui::{AppContext as _, Context};

use super::{ComposerView, ConversationChanged};
use crate::{
    agent::{
        AgentOptionalField, AgentThreadGoalRead, AgentThreadGoalStatus, AgentThreadGoalUpdate,
        GOAL_OBJECTIVE_LIMIT, load_objective, objective_file, prepare_objective,
    },
    conversation::GoalOperation,
};

/// The reference's composer command. The slash menu's Goal turns the chip
/// on; typing `/goal <objective>` in full still submits it directly.
pub(crate) const GOAL_COMMAND: &str = "/goal";

/// How long a user stop waits for the goal pause before interrupting anyway,
/// as the reference bounds its critical pause request.
const PAUSE_BEFORE_STOP_TIMEOUT: Duration = Duration::from_millis(500);

/// Work to run once a goal status change is answered or timed out.
type AfterGoalUpdate = Box<dyn FnOnce(&mut ComposerView, &mut Context<ComposerView>) + 'static>;

/// Asks the shell to open the "Edit goal" tab for this goal, with the
/// objective as the user wrote it (read back from its file when long).
pub struct OpenGoalEditor {
    pub goal: crate::agent::AgentThreadGoal,
    pub text: String,
}
impl gpui::EventEmitter<OpenGoalEditor> for ComposerView {}

impl ComposerView {
    /// Turns the Goal chip on or off. While it is on, submitting sets the
    /// text as a new objective (after the replace confirmation when a goal
    /// already exists).
    pub(super) fn set_goal_draft(&mut self, draft: bool, cx: &mut Context<Self>) {
        self.goal_draft = draft;
        let placeholder = if self.goal_draft {
            crate::i18n::format!("描述你的目标，定义可衡量的成果，以获得最佳效果" => "Describe your goal, define measurable outcomes for best results")
        } else {
            String::new()
        };
        self.prompt_editor.update(cx, |editor, cx| {
            if placeholder.is_empty() {
                // The composer's own default, a catalog key localized at paint.
                editor.set_placeholder("随心输入", cx);
            } else {
                editor.set_placeholder(placeholder, cx);
            }
        });
        cx.notify();
    }

    pub(super) fn load_goal(&mut self, thread_id: String, cx: &mut Context<Self>) {
        let receiver = self.backend.read_thread_goal(thread_id);
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::i18n::format!("目标读取在返回前中断" => "The goal read ended before it returned"))
            });
            let _ = this.update(cx, |this, cx| {
                if this.conversation.goal.resolve_backfill(result) {
                    this.after_goal_change(cx);
                    cx.emit(ConversationChanged);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A typed `/goal` turns the chip on; `/goal <objective>` submits it.
    /// Returns whether the text was a goal command.
    pub(super) fn handle_goal_command(&mut self, raw: &str, cx: &mut Context<Self>) -> bool {
        let trimmed = raw.trim();
        let Some(rest) = trimmed.strip_prefix(GOAL_COMMAND) else {
            return false;
        };
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return false;
        }
        let objective = rest.trim();
        if objective.is_empty() {
            self.clear_prompt(cx);
            self.set_goal_draft(true, cx);
            self.focus_prompt_pending = true;
            return true;
        }
        self.submit_goal(objective.to_owned(), cx);
        true
    }

    /// Submits the composer text as a goal objective.
    pub(super) fn submit_goal(&mut self, objective: String, cx: &mut Context<Self>) {
        if objective.trim().is_empty() {
            return;
        }
        if self.conversation.goal.goal.is_some() {
            self.dialog = Some(super::dialogs::ComposerDialog::ReplaceGoal { objective });
            self.dialog_focus_pending = true;
            cx.notify();
            return;
        }
        self.set_goal(objective, cx);
    }

    /// Writes a new objective, set active, and the server starts its turn.
    /// On a new chat the objective is sent as the first prompt, and the goal
    /// is set once that turn is accepted, as in the reference.
    pub(super) fn set_goal(&mut self, objective: String, cx: &mut Context<Self>) {
        let Some(thread_id) = self.conversation.thread_id.clone() else {
            self.goal_after_first_turn = Some(objective.clone());
            self.set_goal_draft(false, cx);
            self.submit_prompt_as(format!("{GOAL_COMMAND} {objective}"), false, cx);
            return;
        };
        let Some(generation) = self.conversation.goal.generation.or_else(|| {
            self.conversation
                .turn_identity
                .as_ref()
                .map(|identity| identity.generation)
        }) else {
            self.submission_error = Some(
                crate::i18n::format!("目标状态尚未读取，请稍后重试。输入已保留。" => "The goal has not been read yet. Try again shortly; your input is kept."),
            );
            cx.notify();
            return;
        };
        let Some(issued) = self.conversation.goal.begin(GoalOperation::Set) else {
            return;
        };
        self.clear_prompt(cx);
        self.set_goal_draft(false, cx);
        // The server starts the goal's turn by itself; show the objective as
        // that turn's request ("Sent as goal").
        self.pending_goal_bubble = Some(objective.clone());
        self.send_goal_objective(
            generation,
            thread_id,
            objective.clone(),
            move |this, result, cx| {
                match result {
                    Ok(read) => {
                        this.conversation.goal.resolve_read(Some(issued), read);
                        this.after_goal_change(cx);
                    }
                    Err(error) => {
                        this.pending_goal_bubble = None;
                        let message = crate::i18n::format!("无法设置目标：{error}" => "Failed to set goal: {error}");
                        this.conversation.goal.fail(issued, message.clone());
                        this.submission_error = Some(message);
                        if this.draft_is_empty(cx) {
                            this.prompt_editor
                                .update(cx, |editor, cx| editor.set_text_silently(&objective, cx));
                            this.set_goal_draft(true, cx);
                        }
                    }
                }
                cx.emit(ConversationChanged);
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    /// Sends an objective (set active). A long one is first written to its
    /// attachment file off the main thread, as the reference does, and the
    /// full text is remembered for the Edit goal tab.
    fn send_goal_objective(
        &mut self,
        generation: u64,
        thread_id: String,
        objective: String,
        done: impl FnOnce(&mut Self, Result<AgentThreadGoalRead, String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let text = objective.trim().to_owned();
        let backend = self.backend.clone();
        let update = move |objective: String| {
            backend.update_thread_goal(AgentThreadGoalUpdate {
                generation,
                thread_id,
                objective: Some(objective),
                status: Some(AgentThreadGoalStatus::Active),
                token_budget: AgentOptionalField::Unspecified,
            })
        };
        let ended = || crate::i18n::format!("更新目标的响应在返回前中断" => "Updating the goal ended before it returned");
        if text.chars().count() <= GOAL_OBJECTIVE_LIMIT {
            let receiver = update(text);
            cx.spawn(async move |this, cx| {
                let result = receiver.recv().await.unwrap_or_else(|_| Err(ended()));
                let _ = this.update(cx, |this, cx| done(this, result, cx));
            })
            .detach();
            return;
        }
        let home = self.codex_home.clone();
        let written = text.clone();
        let prepared = cx.background_spawn(async move {
            let home = home.ok_or_else(|| "CODEX_HOME".to_owned())?;
            prepare_objective(&written, &home).map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = match prepared.await {
                Ok(pointer) => {
                    let _ = this.update(cx, |this, _| {
                        this.goal_texts.insert(pointer.clone(), text);
                    });
                    update(pointer).recv().await.unwrap_or_else(|_| Err(ended()))
                }
                Err(error) => Err(
                    crate::i18n::format!("未能加载目标附件：{error}" => "Failed to prepare goal attachments: {error}"),
                ),
            };
            let _ = this.update(cx, |this, cx| done(this, result, cx));
        })
        .detach();
    }

    /// The objective as the user wrote it, if known: the objective itself, or
    /// the text behind a pointer this client wrote or already read.
    fn goal_text(&self, goal: &crate::agent::AgentThreadGoal) -> Option<String> {
        match self
            .codex_home
            .as_deref()
            .and_then(|home| objective_file(&goal.objective, home))
        {
            Some(_) => self.goal_texts.get(&goal.objective).cloned(),
            None => Some(goal.objective.clone()),
        }
    }

    /// Reads a pointer objective's file into the cache, then runs `then`.
    fn load_goal_text(
        &mut self,
        objective: String,
        then: impl FnOnce(&mut Self, Result<String, String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let home = self.codex_home.clone();
        let pointer = objective.clone();
        let read = cx.background_spawn(async move {
            let home = home.ok_or_else(|| "CODEX_HOME".to_owned())?;
            load_objective(&pointer, &home).map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(text) = &result {
                    this.goal_texts.insert(objective, text.clone());
                }
                then(this, result, cx);
            });
        })
        .detach();
    }

    /// The pause/resume toggle: active pauses; paused, blocked and
    /// usage-limited resume. Budget-limited and complete goals have no toggle.
    pub(crate) fn toggle_goal_pause(&mut self, cx: &mut Context<Self>) {
        let target = match self.conversation.goal.status() {
            Some(AgentThreadGoalStatus::Active) => AgentThreadGoalStatus::Paused,
            Some(
                AgentThreadGoalStatus::Paused
                | AgentThreadGoalStatus::Blocked
                | AgentThreadGoalStatus::UsageLimited,
            ) => AgentThreadGoalStatus::Active,
            _ => return,
        };
        self.update_goal_status(target, None, cx);
    }

    /// Sends a status change. `then` runs after the answer (or the timeout),
    /// whether or not it succeeded.
    fn update_goal_status(
        &mut self,
        status: AgentThreadGoalStatus,
        then: Option<AfterGoalUpdate>,
        cx: &mut Context<Self>,
    ) {
        let (Some(thread_id), Some(generation)) = (
            self.conversation.goal.thread_id.clone(),
            self.conversation.goal.generation,
        ) else {
            if let Some(then) = then {
                then(self, cx);
            }
            return;
        };
        let operation = if status == AgentThreadGoalStatus::Active {
            GoalOperation::Resume
        } else {
            GoalOperation::Pause
        };
        let Some(issued) = self.conversation.goal.begin(operation) else {
            if let Some(then) = then {
                then(self, cx);
            }
            return;
        };
        let receiver = self.backend.update_thread_goal(AgentThreadGoalUpdate {
            generation,
            thread_id,
            objective: None,
            status: Some(status),
            token_budget: AgentOptionalField::Unspecified,
        });
        // The answer and, for a stop, the timeout race on one channel; the
        // first message decides. `None` means the timeout won.
        let (race, first) = async_channel::bounded::<Option<Result<_, String>>>(2);
        let answer_race = race.clone();
        cx.background_spawn(async move {
            let answer = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::i18n::format!("更新目标的响应在返回前中断" => "Updating the goal ended before it returned"))
            });
            let _ = answer_race.send(Some(answer)).await;
        })
        .detach();
        if then.is_some() {
            let timer = cx.background_executor().timer(PAUSE_BEFORE_STOP_TIMEOUT);
            cx.background_spawn(async move {
                timer.await;
                let _ = race.send(None).await;
            })
            .detach();
        }
        cx.spawn(async move |this, cx| {
            let result = first.recv().await.ok().flatten();
            let _ = this.update(cx, |this, cx| {
                match result {
                    Some(Ok(read)) => {
                        this.conversation.goal.resolve_read(Some(issued), read);
                    }
                    Some(Err(error)) => this.conversation.goal.fail(
                        issued,
                        crate::i18n::format!("无法更新目标：{error}" => "Failed to update goal: {error}"),
                    ),
                    None => this.conversation.goal.fail(
                        issued,
                        crate::i18n::format!("暂停目标未在时限内确认" => "Pausing the goal was not confirmed in time"),
                    ),
                }
                if let Some(then) = then {
                    then(this, cx);
                }
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn clear_goal(&mut self, cx: &mut Context<Self>) {
        let (Some(thread_id), Some(generation)) = (
            self.conversation.goal.thread_id.clone(),
            self.conversation.goal.generation,
        ) else {
            return;
        };
        let Some(issued) = self.conversation.goal.begin(GoalOperation::Clear) else {
            return;
        };
        let receiver = self.backend.clear_thread_goal(thread_id, generation);
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::i18n::format!("清除目标的响应在返回前中断" => "Clearing the goal ended before it returned"))
            });
            let _ = this.update(cx, |this, cx| {
                this.conversation.goal.resolve_clear(
                    issued,
                    result.map_err(|error| {
                        crate::i18n::format!("无法清除目标：{error}" => "Failed to clear goal: {error}")
                    }),
                );
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Opens the saved objective in the "Edit goal" tab beside the chat.
    pub(crate) fn edit_goal(&mut self, cx: &mut Context<Self>) {
        let Some(goal) = self.conversation.goal.goal.clone() else {
            return;
        };
        if let Some(text) = self.goal_text(&goal) {
            cx.emit(OpenGoalEditor { goal, text });
            return;
        }
        self.load_goal_text(
            goal.objective.clone(),
            move |this, result, cx| match result {
                Ok(text) => cx.emit(OpenGoalEditor { goal, text }),
                Err(_) => this.show_toast(
                    super::toast::ToastKind::Danger,
                    crate::i18n::format!("未成功加载目标" => "Failed to load goal objective"),
                    cx,
                ),
            },
            cx,
        );
    }

    /// Saves an edited objective from the "Edit goal" tab. As in the
    /// reference, the save also sets the goal active (resuming a paused one)
    /// and appends no transcript item.
    pub(crate) fn save_goal_objective(&mut self, objective: String, cx: &mut Context<Self>) {
        let (Some(thread_id), Some(generation)) = (
            self.conversation.goal.thread_id.clone(),
            self.conversation.goal.generation,
        ) else {
            return;
        };
        let Some(issued) = self.conversation.goal.begin(GoalOperation::Set) else {
            return;
        };
        self.send_goal_objective(
            generation,
            thread_id,
            objective,
            move |this, result, cx| {
                match result {
                    Ok(read) => {
                        this.conversation.goal.resolve_read(Some(issued), read);
                    }
                    Err(error) => this.conversation.goal.fail(
                        issued,
                        crate::i18n::format!("未成功保存目标：{error}" => "Failed to save goal objective: {error}"),
                    ),
                }
                cx.emit(ConversationChanged);
                cx.notify();
            },
            cx,
        );
        cx.emit(ConversationChanged);
        cx.notify();
    }

    /// What an open "Edit goal" tab shows for this conversation's goal.
    pub fn goal_tab_sync(
        &self,
    ) -> Option<(String, crate::components::file_panel::GoalTabSync<'_>)> {
        let goal = &self.conversation.goal;
        Some((
            goal.thread_id.clone()?,
            crate::components::file_panel::GoalTabSync {
                goal: goal.goal.as_ref(),
                text: goal.goal.as_ref().and_then(|goal| self.goal_text(goal)),
                saving: matches!(goal.pending, Some((GoalOperation::Set, _))),
                error: goal.error.clone(),
            },
        ))
    }

    /// Reacts to a new goal snapshot: a newly complete goal is cleared right
    /// away, as the reference clears it on the completing update.
    pub(super) fn after_goal_change(&mut self, cx: &mut Context<Self>) {
        // An open Edit goal tab compares the objective as written; read a
        // pointer set elsewhere so the tab can tell it changed.
        if let Some(goal) = self.conversation.goal.goal.clone()
            && self.goal_text(&goal).is_none()
        {
            self.load_goal_text(
                goal.objective,
                |_, _, cx| {
                    cx.emit(ConversationChanged);
                    cx.notify();
                },
                cx,
            );
        }
        let complete = self
            .conversation
            .goal
            .goal
            .as_ref()
            .filter(|goal| goal.status == AgentThreadGoalStatus::Complete)
            .map(|goal| goal.updated_at);
        if let Some(updated_at) = complete
            && self.auto_cleared_goal != Some(updated_at)
        {
            self.auto_cleared_goal = Some(updated_at);
            self.clear_goal(cx);
        }
    }

    /// A user stop with an active goal pauses the goal first, then interrupts;
    /// without the pause the server would start the next continuation turn.
    pub(super) fn stop_generation_with_goal(&mut self, cx: &mut Context<Self>) -> bool {
        if self.conversation.goal.status() != Some(AgentThreadGoalStatus::Active) {
            return false;
        }
        self.update_goal_status(
            AgentThreadGoalStatus::Paused,
            Some(Box::new(|this: &mut Self, cx: &mut Context<Self>| {
                if this.conversation.stop_generation() {
                    cx.emit(ConversationChanged);
                    cx.notify();
                }
            })),
            cx,
        );
        true
    }

    /// Called once the first turn of a new chat is accepted: a goal submitted
    /// before the thread existed is set now.
    pub(super) fn apply_goal_after_first_turn(&mut self, cx: &mut Context<Self>) {
        if self.conversation.thread_id.is_none() || self.conversation.turn_identity.is_none() {
            return;
        }
        let Some(objective) = self.goal_after_first_turn.take() else {
            return;
        };
        let generation = self
            .conversation
            .turn_identity
            .as_ref()
            .map(|identity| identity.generation);
        if self.conversation.goal.generation.is_none() {
            self.conversation.goal.generation = generation;
        }
        self.pending_goal_bubble = None;
        self.set_goal(objective, cx);
        // The first turn already shows `/goal <objective>` as its request.
        self.pending_goal_bubble = None;
    }
}
