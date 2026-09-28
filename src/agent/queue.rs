//! Server-side follow-up queue of one thread.
//!
//! The server owns the order and starts the next submission itself when a
//! turn completes; clients only add, edit, delete, reorder, or start one early.

use super::{AgentPromptContext, UserMessageAttachment};

/// One queued submission. `client_message_id` is the id the later userMessage
/// item carries as its `clientId` when the server starts the submission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentQueuedSubmission {
    pub id: String,
    pub client_message_id: String,
    /// Display text with any attachment envelope removed.
    pub text: String,
    pub attachments: Vec<UserMessageAttachment>,
}

/// The complete queue of one thread as read in one generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadQueue {
    pub generation: u64,
    pub thread_id: String,
    pub submissions: Vec<AgentQueuedSubmission>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentQueueAddRequest {
    pub generation: u64,
    pub thread_id: String,
    pub client_message_id: String,
    pub prompt: String,
    pub context: AgentPromptContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentQueueUpdateRequest {
    pub generation: u64,
    pub thread_id: String,
    pub queued_submission_id: String,
    pub prompt: String,
    pub context: AgentPromptContext,
}

/// Targets one submission (delete, start) or, for start, the queue head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentQueueTarget {
    pub generation: u64,
    pub thread_id: String,
    pub queued_submission_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentQueueReorderRequest {
    pub generation: u64,
    pub thread_id: String,
    /// The complete new order; the server rejects a partial list.
    pub queued_submission_ids: Vec<String>,
}
