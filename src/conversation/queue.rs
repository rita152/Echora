//! The thread's server-side follow-up queue as the composer shows it.
//!
//! The server list is authoritative. Local state only adds what the server
//! cannot know: the draft snapshot of submissions this client queued (so
//! attachments and review comments survive a restore), per-row operations in
//! flight, and failures. `thread/queue/changed` bumps the wanted revision and
//! the composer re-lists until the answer it has is for the latest signal.

use std::collections::HashMap;

use super::SubmissionDraft;
use crate::agent::{AgentQueuedSubmission, AgentThreadQueue};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum QueueRowOperation {
    Updating,
    Deleting,
    Steering,
    Starting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QueueRow {
    pub(crate) submission: AgentQueuedSubmission,
    pub(crate) operation: Option<QueueRowOperation>,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConversationQueue {
    pub(crate) thread_id: Option<String>,
    pub(crate) generation: Option<u64>,
    pub(crate) rows: Vec<QueueRow>,
    /// Drafts of submissions this client queued, by clientUserMessageId,
    /// with the exact prompt that was encoded for them.
    pub(crate) drafts: HashMap<String, (SubmissionDraft, String)>,
    /// `thread/queue/add` requests still in flight.
    pub(crate) adding: usize,
    /// The latest invalidation seen and the latest one a list answered.
    pub(crate) wanted: u64,
    pub(crate) listed: u64,
    pub(crate) listing: bool,
    pub(crate) list_error: Option<String>,
    pub(crate) reorder_pending: bool,
}

impl ConversationQueue {
    pub(crate) fn reset(&mut self, thread_id: Option<String>) {
        if self.thread_id != thread_id {
            *self = Self {
                thread_id,
                ..Self::default()
            };
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// A change signal (or the first open): another list is needed.
    pub(crate) fn invalidate(&mut self, generation: u64, thread_id: &str) -> bool {
        if self.thread_id.as_deref() != Some(thread_id)
            || self.generation.is_some_and(|current| generation < current)
        {
            return false;
        }
        self.wanted += 1;
        true
    }

    /// Starts a list if one is needed and none is in flight; returns the
    /// revision the list answers.
    pub(crate) fn begin_list(&mut self) -> Option<u64> {
        if self.listing || self.thread_id.is_none() || self.listed >= self.wanted {
            return None;
        }
        self.listing = true;
        Some(self.wanted)
    }

    /// Applies a list answer. Returns whether another list is needed because
    /// more changes arrived while this one was in flight.
    pub(crate) fn resolve_list(
        &mut self,
        revision: u64,
        result: Result<AgentThreadQueue, String>,
    ) -> bool {
        self.listing = false;
        match result {
            Ok(queue) => {
                if self.thread_id.as_deref() != Some(queue.thread_id.as_str())
                    || self
                        .generation
                        .is_some_and(|current| queue.generation < current)
                {
                    return false;
                }
                self.generation = Some(queue.generation);
                self.listed = self.listed.max(revision);
                self.list_error = None;
                let previous = std::mem::take(&mut self.rows);
                self.rows = queue
                    .submissions
                    .into_iter()
                    .map(|submission| {
                        let old = previous
                            .iter()
                            .find(|row| row.submission.id == submission.id);
                        QueueRow {
                            operation: old.and_then(|row| row.operation.clone()),
                            error: old.and_then(|row| row.error.clone()),
                            submission,
                        }
                    })
                    .collect();
            }
            Err(error) => {
                self.listed = self.listed.max(revision);
                self.list_error = Some(error);
            }
        }
        self.listed < self.wanted
    }

    pub(crate) fn row_mut(&mut self, id: &str) -> Option<&mut QueueRow> {
        self.rows.iter_mut().find(|row| row.submission.id == id)
    }

    /// Marks one row busy; refuses when it already is, so a repeated click
    /// cannot send a second request.
    pub(crate) fn begin_row(&mut self, id: &str, operation: QueueRowOperation) -> bool {
        let Some(row) = self.row_mut(id) else {
            return false;
        };
        if row.operation.is_some() {
            return false;
        }
        row.operation = Some(operation);
        row.error = None;
        true
    }

    pub(crate) fn finish_row(&mut self, id: &str, error: Option<String>) {
        if let Some(row) = self.row_mut(id) {
            row.operation = None;
            row.error = error;
        }
    }

    /// Records a submission this client just queued, before the list shows it.
    pub(crate) fn insert_added(
        &mut self,
        submission: AgentQueuedSubmission,
        draft: SubmissionDraft,
        prompt: String,
    ) {
        self.drafts
            .insert(submission.client_message_id.clone(), (draft, prompt));
        if self.row_mut(&submission.id).is_none() {
            self.rows.push(QueueRow {
                submission,
                operation: None,
                error: None,
            });
        }
    }

    pub(crate) fn replace_submission(&mut self, submission: AgentQueuedSubmission) {
        if let Some(row) = self.row_mut(&submission.id) {
            row.submission = submission;
        }
    }

    /// Removes a row after a confirmed delete; the list will agree.
    pub(crate) fn remove(&mut self, id: &str) -> Option<QueueRow> {
        let index = self.rows.iter().position(|row| row.submission.id == id)?;
        Some(self.rows.remove(index))
    }

    /// Moves one row locally for a drag, returning the complete new order the
    /// server requires, or None when nothing moved.
    pub(crate) fn move_row(&mut self, id: &str, to: usize) -> Option<Vec<String>> {
        let from = self.rows.iter().position(|row| row.submission.id == id)?;
        let to = to.min(self.rows.len().saturating_sub(1));
        if from == to {
            return None;
        }
        let row = self.rows.remove(from);
        self.rows.insert(to, row);
        Some(self.order())
    }

    pub(crate) fn order(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| row.submission.id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests;
