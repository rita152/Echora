//! Background terminals: command items that keep running after their turn.
//!
//! A unified-exec command whose tool call yielded stays `inProgress` when its
//! turn completes; its later output and its end arrive as ordinary item
//! messages still carrying the finished turn's id. The list the summary panel
//! shows is derived from the items, as the reference derives it: every running
//! command item outside the turn that is itself still in progress.
//! `thread/backgroundTerminals/clean` stops all of them; their final state is
//! whatever the server reports next, never rewritten here.

use std::collections::HashSet;

use super::{
    activity::{ConversationActivity, find_command_activity_mut, upsert_command_activity},
    state::ConversationState,
    transcript::ConversationPhase,
};
use crate::agent::{AgentEvent, CommandExecution, CommandExecutionStatus};

/// How a background terminal's card reads (the reference's
/// `isBackgroundTerminalRunning` / `isBackgroundTerminalFinished`, and the
/// interrupted state a clean leaves until the server reports the end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackgroundMark {
    Running,
    Stopped,
    Finished,
}

/// One running background terminal, newest turn first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BackgroundTerminal {
    pub item_id: String,
    /// The command text the row shows; empty when the item has none.
    pub command: String,
}

/// The one `clean` request a thread may have in flight, and how the last one
/// ended. A failure is shown once and never retried automatically.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum BackgroundCleanState {
    #[default]
    Idle,
    /// Started from the row whose stop button was clicked (none for the stop
    /// fallback); every stop button is disabled meanwhile.
    InFlight {
        clicked_item_id: Option<String>,
    },
    Succeeded,
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConversationBackground {
    pub clean: BackgroundCleanState,
    /// Items that were running when a clean succeeded. Only their label says
    /// "stopped" until the server's `item/completed` arrives; their status is
    /// untouched.
    pub stop_requested: HashSet<String>,
}

fn phase_in_progress(phase: ConversationPhase) -> bool {
    matches!(
        phase,
        ConversationPhase::Starting
            | ConversationPhase::Thinking
            | ConversationPhase::Streaming
            | ConversationPhase::Stopping
    )
}

fn running_commands(
    activities: &[ConversationActivity],
) -> impl Iterator<Item = &CommandExecution> {
    activities.iter().filter_map(|activity| match activity {
        ConversationActivity::Command(command)
            if command.status == CommandExecutionStatus::InProgress =>
        {
            Some(command)
        }
        _ => None,
    })
}

impl ConversationState {
    /// Running command items outside the turn in progress, newest turn first.
    pub(crate) fn background_terminals(&self) -> Vec<BackgroundTerminal> {
        let mut terminals = Vec::new();
        if !phase_in_progress(self.phase) {
            terminals.extend(running_commands(&self.activities).map(terminal));
        }
        for turn in self.transcript.iter().rev() {
            terminals.extend(running_commands(&turn.activities).map(terminal));
        }
        terminals
    }

    /// Every command item that runs (or ran) in the background, by item id.
    pub(crate) fn background_marks(&self) -> std::collections::HashMap<String, BackgroundMark> {
        let current = (!phase_in_progress(self.phase)).then_some(&self.activities);
        current
            .into_iter()
            .chain(self.transcript.iter().map(|turn| &turn.activities))
            .flat_map(|activities| activities.iter())
            .filter_map(|activity| match activity {
                ConversationActivity::Command(command) if command.terminal_process_id.is_some() => {
                    let mark = if command.status != CommandExecutionStatus::InProgress {
                        BackgroundMark::Finished
                    } else if self.background.stop_requested.contains(&command.id) {
                        BackgroundMark::Stopped
                    } else {
                        BackgroundMark::Running
                    };
                    Some((command.id.clone(), mark))
                }
                _ => None,
            })
            .collect()
    }

    /// A command message of a turn that already ended. Returns whether the
    /// conversation changed.
    pub(crate) fn apply_background_command_event(
        &mut self,
        turn_id: &str,
        event: AgentEvent,
    ) -> bool {
        let item_id = match &event {
            AgentEvent::CommandOutputDelta { item_id, .. }
            | AgentEvent::CommandTerminalInteraction { item_id, .. } => item_id.clone(),
            AgentEvent::CommandCompleted(command) => command.id.clone(),
            _ => return false,
        };
        let current = self.turn_id.as_deref() == Some(turn_id)
            || find_command_activity_mut(&mut self.activities, &item_id).is_some();
        let activities = if current {
            &mut self.activities
        } else if let Some(turn) = self.transcript.iter_mut().find(|turn| {
            turn.turn_id.as_deref() == Some(turn_id)
                || turn.activities.iter().any(|activity| {
                    matches!(activity, ConversationActivity::Command(command) if command.id == item_id)
                })
        }) {
            &mut turn.activities
        } else {
            // Not a command this conversation has shown (history reloads omit
            // items that are still running); the next history read has it.
            return false;
        };
        match event {
            AgentEvent::CommandOutputDelta { delta, .. } => {
                let Some(command) = find_command_activity_mut(activities, &item_id) else {
                    return false;
                };
                command.output.push_str(&delta);
            }
            AgentEvent::CommandTerminalInteraction { process_id, .. } => {
                let Some(command) = find_command_activity_mut(activities, &item_id) else {
                    return false;
                };
                command.terminal_process_id = Some(process_id);
            }
            AgentEvent::CommandCompleted(command) => {
                upsert_command_activity(activities, command);
                self.background.stop_requested.remove(&item_id);
            }
            _ => return false,
        }
        true
    }

    /// Starts the thread's single clean request. False while one is in flight.
    pub(crate) fn begin_background_clean(&mut self, clicked_item_id: Option<String>) -> bool {
        if matches!(self.background.clean, BackgroundCleanState::InFlight { .. }) {
            return false;
        }
        self.background.clean = BackgroundCleanState::InFlight { clicked_item_id };
        true
    }

    pub(crate) fn finish_background_clean(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                let running: Vec<String> = self
                    .background_terminals()
                    .into_iter()
                    .map(|terminal| terminal.item_id)
                    .collect();
                self.background.stop_requested.extend(running);
                self.background.clean = BackgroundCleanState::Succeeded;
            }
            Err(message) => self.background.clean = BackgroundCleanState::Failed { message },
        }
    }

    /// The failure toast was shown; the state returns to idle.
    pub(crate) fn acknowledge_background_clean_failure(&mut self) {
        if matches!(self.background.clean, BackgroundCleanState::Failed { .. }) {
            self.background.clean = BackgroundCleanState::Idle;
        }
    }
}

fn terminal(command: &CommandExecution) -> BackgroundTerminal {
    BackgroundTerminal {
        item_id: command.id.clone(),
        command: command.command.trim().to_owned(),
    }
}

#[cfg(test)]
mod tests;
