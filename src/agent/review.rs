//! Agent-neutral requests for work that starts a turn without a prompt: a code
//! review of the workspace and a shell command typed into the composer.

use std::path::PathBuf;

use super::{catalog::AgentPermissionMode, thread::ProjectId};

/// What a code review looks at: the two targets the reference's Code review
/// menu offers. (The schema also has `commit` and `custom`, which no entry
/// point here starts.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentReviewTarget {
    /// Staged, unstaged and untracked changes in the working tree.
    UncommittedChanges,
    /// The current branch against `branch`, from their merge base.
    BaseBranch { branch: String },
}

/// The thread a prompt-less request runs in: an existing thread, or a new one
/// started with these settings (the composer's draft settings).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadTarget {
    pub thread_id: Option<String>,
    pub cwd: PathBuf,
    pub project_id: Option<ProjectId>,
    pub model: String,
    pub service_tier: Option<String>,
    pub permission_mode: AgentPermissionMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentReviewRequest {
    pub thread: AgentThreadTarget,
    pub target: AgentReviewTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentShellCommandRequest {
    pub thread: AgentThreadTarget,
    /// Evaluated by the thread's shell with its own syntax (pipes, quoting).
    pub command: String,
    /// `None` leaves the server's one-hour default.
    pub timeout_ms: Option<u64>,
}

/// A shell command the server accepted. Its output arrives as the thread's
/// own turn (a new one on an idle thread, the running one otherwise).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentShellCommandStarted {
    pub generation: u64,
    pub thread_id: String,
    /// Whether the thread was started for this command.
    pub created_thread: bool,
}
