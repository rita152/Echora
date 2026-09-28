//! Goal state of the conversation's thread, reduced from reads, writes and
//! notifications without GPUI.
//!
//! Server snapshots are ordered by `updatedAt`; a response is applied only if
//! no newer observation arrived while it was in flight, so a late read or a
//! duplicate notification never rolls a goal back.

use crate::agent::{AgentThreadGoal, AgentThreadGoalRead, AgentThreadGoalStatus};

/// The local operation waiting for its RPC, if any. One at a time per thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GoalOperation {
    Set,
    Pause,
    Resume,
    Clear,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConversationGoal {
    pub(crate) thread_id: Option<String>,
    pub(crate) generation: Option<u64>,
    pub(crate) goal: Option<AgentThreadGoal>,
    /// Bumped by every applied observation; responses carry the value they
    /// were issued at.
    pub(crate) revision: u64,
    pub(crate) pending: Option<(GoalOperation, u64)>,
    pub(crate) error: Option<String>,
}

impl ConversationGoal {
    /// Resets the state for another thread; nothing carries across threads.
    pub(crate) fn reset(&mut self, thread_id: Option<String>) {
        if self.thread_id != thread_id {
            *self = Self {
                thread_id,
                ..Self::default()
            };
        }
    }

    fn accepts(&self, generation: u64) -> bool {
        self.generation.is_none_or(|current| generation >= current)
    }

    fn adopt_generation(&mut self, generation: u64) {
        if self.generation.is_none_or(|current| generation > current) {
            self.generation = Some(generation);
        }
    }

    /// A server snapshot (notification). Older `updatedAt` values are ignored.
    pub(crate) fn observe(&mut self, generation: u64, goal: AgentThreadGoal) -> bool {
        if self.thread_id.as_deref() != Some(goal.thread_id.as_str()) || !self.accepts(generation) {
            return false;
        }
        if self
            .goal
            .as_ref()
            .is_some_and(|current| current == &goal || current.updated_at > goal.updated_at)
        {
            return false;
        }
        self.adopt_generation(generation);
        self.goal = Some(goal);
        self.revision += 1;
        true
    }

    pub(crate) fn observe_cleared(&mut self, generation: u64, thread_id: &str) -> bool {
        if self.thread_id.as_deref() != Some(thread_id) || !self.accepts(generation) {
            return false;
        }
        self.adopt_generation(generation);
        let changed = self.goal.take().is_some();
        self.revision += 1;
        changed
    }

    /// Starts one local operation and returns the revision it was issued at.
    pub(crate) fn begin(&mut self, operation: GoalOperation) -> Option<u64> {
        if self.pending.is_some() {
            return None;
        }
        self.pending = Some((operation, self.revision));
        self.error = None;
        Some(self.revision)
    }

    /// Applies a read or write answer. It replaces the snapshot only when no
    /// newer observation arrived meanwhile, or when it is itself newer.
    pub(crate) fn resolve_read(
        &mut self,
        issued_at: Option<u64>,
        read: AgentThreadGoalRead,
    ) -> bool {
        if self.thread_id.as_deref() != Some(read.thread_id.as_str())
            || !self.accepts(read.generation)
        {
            return false;
        }
        if let Some(issued) = issued_at
            && self
                .pending
                .as_ref()
                .is_some_and(|(_, revision)| *revision == issued)
        {
            self.pending = None;
        }
        let stale = issued_at.is_some_and(|issued| issued != self.revision);
        let newer = match (&self.goal, &read.goal) {
            (Some(current), Some(read)) => read.updated_at >= current.updated_at,
            _ => false,
        };
        if stale && !newer {
            return true;
        }
        self.adopt_generation(read.generation);
        if self.goal != read.goal {
            self.goal = read.goal;
            self.revision += 1;
        }
        true
    }

    /// A first read (open or resume). Failures are logged, not shown: the
    /// reference only logs a failed goal backfill.
    pub(crate) fn resolve_backfill(&mut self, read: Result<AgentThreadGoalRead, String>) -> bool {
        match read {
            Ok(read) => {
                let revision = self.revision;
                self.resolve_read(Some(revision), read)
            }
            Err(error) => {
                eprintln!("thread/goal/get 回填失败：{error}");
                false
            }
        }
    }

    pub(crate) fn resolve_clear(&mut self, issued_at: u64, result: Result<bool, String>) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, revision)| *revision == issued_at)
        {
            self.pending = None;
        }
        match result {
            Ok(_) => {
                if issued_at == self.revision && self.goal.take().is_some() {
                    self.revision += 1;
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn fail(&mut self, issued_at: u64, error: String) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, revision)| *revision == issued_at)
        {
            self.pending = None;
            self.error = Some(error);
        }
    }

    pub(crate) fn status(&self) -> Option<AgentThreadGoalStatus> {
        self.goal.as_ref().map(|goal| goal.status)
    }
}

#[cfg(test)]
mod tests;
