//! Agent-neutral fuzzy file search requests, results, and session handles.
//!
//! The Codex app-server exposes two shapes of the same capability: a one-shot
//! "fuzzyFileSearch" request and an experiment-gated session
//! ("fuzzyFileSearch/sessionStart|sessionUpdate|sessionStop" plus the
//! "sessionUpdated"/"sessionCompleted" notifications). Captured reference
//! traffic shows the desktop app driving the session form and falling back to
//! the one-shot request when a session cannot be created, so both shapes are
//! represented here and the adapter picks whichever the connection supports.

use std::{fmt, sync::Arc};

use async_channel::Receiver;

/// Whether a fuzzy match names a file or a directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentFileMatchType {
    File,
    Directory,
}

/// One fuzzy file match, in the order the backend produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFileSearchResult {
    pub file_name: String,
    pub match_type: AgentFileMatchType,
    /// Path relative to the entry's root, using the backend's separators.
    pub path: String,
    pub root: String,
    /// Backend sort weight; the protocol does not define a scale, only that
    /// higher scores are better matches.
    pub score: u32,
    /// Character indices of the matched characters inside the file name, used
    /// to highlight the match.
    pub indices: Option<Vec<u32>>,
}

/// One-shot "fuzzyFileSearch" parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFileSearchRequest {
    pub query: String,
    pub roots: Vec<String>,
    /// Optional client-chosen token the server may use to abandon superseded
    /// searches.
    pub cancellation_token: Option<String>,
}

/// A "fuzzyFileSearch/sessionUpdated" payload for one session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFileSearchSessionUpdate {
    pub session_id: String,
    pub query: String,
    pub files: Vec<AgentFileSearchResult>,
}

/// A "fuzzyFileSearch/sessionCompleted" payload for one session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFileSearchSessionCompleted {
    pub session_id: String,
}

/// Everything a file search session can deliver to its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentFileSearchSessionEvent {
    Updated(AgentFileSearchSessionUpdate),
    Completed(AgentFileSearchSessionCompleted),
    /// The connection generation ended or the session was invalidated.
    Failed(String),
}

pub(crate) trait AgentFileSearchSessionControl: Send + Sync {
    fn update_query(&self, session_id: &str, query: &str) -> Result<(), String>;
    fn stop(&self, session_id: &str) -> Result<(), String>;
}

/// Owner handle for one live fuzzy file search session. Dropping the handle
/// does not stop the session; call AgentFileSearchSession::stop for that.
#[derive(Clone)]
pub struct AgentFileSearchSession {
    session_id: String,
    control: Arc<dyn AgentFileSearchSessionControl>,
    events: Receiver<AgentFileSearchSessionEvent>,
}

impl AgentFileSearchSession {
    pub(crate) fn new(
        session_id: String,
        control: Arc<dyn AgentFileSearchSessionControl>,
        events: Receiver<AgentFileSearchSessionEvent>,
    ) -> Self {
        Self {
            session_id,
            control,
            events,
        }
    }

    /// Stream of updates for this session. Cloning the session clones the
    /// underlying receiver; the owner should keep exactly one consumer.
    pub fn updates(&self) -> Receiver<AgentFileSearchSessionEvent> {
        self.events.clone()
    }

    /// Replaces the query of a live session. The server answers with
    /// "sessionUpdated" notifications instead of returning results inline.
    pub fn update_query(&self, query: impl Into<String>) -> Result<(), String> {
        self.control.update_query(&self.session_id, &query.into())
    }

    /// Ends the session on the server. Safe to call more than once.
    pub fn stop(&self) -> Result<(), String> {
        self.control.stop(&self.session_id)
    }
}

impl fmt::Debug for AgentFileSearchSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentFileSearchSession")
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AgentFileSearchSession {
    fn eq(&self, other: &Self) -> bool {
        self.session_id == other.session_id && Arc::ptr_eq(&self.control, &other.control)
    }
}

impl Eq for AgentFileSearchSession {}
