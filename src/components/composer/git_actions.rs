//! Git follow-ups of a conversation, as the reference performs them: after
//! an agent command completes, a `git push` or `git checkout`/`switch` it
//! contained is reported to the app (which refreshes pull-request lookups,
//! the checkout's Git metadata and the thread's branch), and after an
//! accepted message the thread's branch is synced with its checkout.

use std::path::PathBuf;

use gpui::Context;

use super::ComposerView;
use crate::{
    agent::{
        AgentDynamicToolCallContentItem, AgentDynamicToolCallStatus, AgentEvent,
        CommandExecutionSource, CommandExecutionStatus,
    },
    pull_requests::detection::{GitAction, dynamic_exec_command, git_actions, output_looks_failed},
};

/// Git actions an agent command of this thread performed.
pub struct GitActionsDetected {
    pub thread_id: String,
    /// The thread's own working directory.
    pub thread_cwd: PathBuf,
    pub actions: Vec<GitAction>,
}
impl gpui::EventEmitter<GitActionsDetected> for ComposerView {}

/// A message was accepted in an existing thread: its branch follows the
/// checkout (the reference's `U()` after a send).
pub struct SyncThreadGitBranch {
    pub thread_id: String,
    pub cwd: PathBuf,
}
impl gpui::EventEmitter<SyncThreadGitBranch> for ComposerView {}

/// The actions of one completed item, with the reference's filters: a
/// successful non-shell `commandExecution`, or a successful `exec` /
/// `exec_command` dynamic tool call whose output does not read as a failure.
pub(super) fn item_git_actions(event: &AgentEvent, thread_cwd: &std::path::Path) -> Vec<GitAction> {
    match event {
        AgentEvent::CommandCompleted(command)
            if command.status == CommandExecutionStatus::Completed
                && command.exit_code == Some(0)
                && command.source != CommandExecutionSource::UserShell =>
        {
            let cwd = if command.cwd.is_empty() {
                thread_cwd.to_owned()
            } else {
                PathBuf::from(&command.cwd)
            };
            git_actions(&command.command, &cwd)
        }
        AgentEvent::DynamicToolCallUpdated(call)
            if call.completed
                && call.status == AgentDynamicToolCallStatus::Completed
                && call.success == Some(true)
                && matches!(call.namespace.as_deref(), None | Some("functions"))
                && matches!(call.tool.as_str(), "exec" | "exec_command") =>
        {
            let Some((command, directory)) = dynamic_exec_command(&call.arguments) else {
                return Vec::new();
            };
            let output = call
                .content_items
                .iter()
                .flatten()
                .filter_map(|item| match item {
                    AgentDynamicToolCallContentItem::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if output_looks_failed(&output) {
                return Vec::new();
            }
            let cwd = match directory {
                Some(directory) => thread_cwd.join(directory),
                None => thread_cwd.to_owned(),
            };
            git_actions(&command, &cwd)
        }
        _ => Vec::new(),
    }
}

impl ComposerView {
    pub(super) fn report_git_actions(&mut self, events: &[AgentEvent], cx: &mut Context<Self>) {
        let Some(thread_id) = self.conversation.thread_id.clone() else {
            return;
        };
        let thread_cwd = self.conversation.cwd.clone();
        let actions: Vec<GitAction> = events
            .iter()
            .flat_map(|event| item_git_actions(event, &thread_cwd))
            .collect();
        if !actions.is_empty() {
            cx.emit(GitActionsDetected {
                thread_id,
                thread_cwd,
                actions,
            });
        }
    }

    pub(super) fn sync_git_branch_after_send(&mut self, cx: &mut Context<Self>) {
        if let Some(thread_id) = self.conversation.thread_id.clone() {
            cx.emit(SyncThreadGitBranch {
                thread_id,
                cwd: self.conversation.cwd.clone(),
            });
        }
    }
}
