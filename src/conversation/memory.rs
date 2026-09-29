//! A conversation's memory switches: the new-chat choice sent with
//! `thread/start`, and the started thread's generation mode with an
//! optimistic change that rolls back on failure.

use crate::agent::AgentMemoryPreferences;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingMemoryMode {
    pub(crate) operation: u64,
    pub(crate) thread_id: String,
    pub(crate) generation: u64,
    /// The value shown before the optimistic change, restored on failure.
    pub(crate) previous: Option<bool>,
}

/// What settling a mode change did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MemoryModeSettled {
    /// Late, duplicate or for another thread/generation: nothing changed.
    Ignored,
    Applied,
    RolledBack(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConversationMemory {
    pub(crate) thread_id: Option<String>,
    /// Chosen before the chat started; sent with `thread/start`.
    pub(crate) new_chat: Option<AgentMemoryPreferences>,
    /// Values known for the started thread in this session. The server has
    /// no read for them, so `None` falls back to the configured default.
    pub(crate) use_memories: Option<bool>,
    pub(crate) generate_memories: Option<bool>,
    pub(crate) pending: Option<PendingMemoryMode>,
    next_operation: u64,
}

impl ConversationMemory {
    /// Follows the conversation's thread. The first thread of a new chat
    /// inherits the choice it was started with; any other change starts over.
    pub(crate) fn reset(&mut self, thread_id: Option<String>) {
        if self.thread_id == thread_id {
            return;
        }
        let started = self.thread_id.is_none() && thread_id.is_some();
        let chosen = self.new_chat.take().filter(|_| started);
        *self = Self {
            thread_id,
            use_memories: chosen.map(|choice| choice.use_memories),
            generate_memories: chosen.map(|choice| choice.generate_memories),
            next_operation: self.next_operation,
            ..Self::default()
        };
    }

    /// Shows the new value at once and records what to restore. Refused
    /// while another change of this chat is in flight.
    pub(crate) fn begin_generate(
        &mut self,
        thread_id: &str,
        generation: u64,
        value: bool,
        shown: bool,
    ) -> Option<u64> {
        if self.pending.is_some() || self.thread_id.as_deref() != Some(thread_id) {
            return None;
        }
        self.next_operation += 1;
        self.pending = Some(PendingMemoryMode {
            operation: self.next_operation,
            thread_id: thread_id.to_owned(),
            generation,
            previous: self.generate_memories.or(Some(shown)),
        });
        self.generate_memories = Some(value);
        Some(self.next_operation)
    }

    pub(crate) fn settle_generate(
        &mut self,
        operation: u64,
        thread_id: &str,
        generation: u64,
        result: Result<(), String>,
    ) -> MemoryModeSettled {
        let matches = self.pending.as_ref().is_some_and(|pending| {
            pending.operation == operation
                && pending.thread_id == thread_id
                && pending.generation == generation
        });
        if !matches || self.thread_id.as_deref() != Some(thread_id) {
            return MemoryModeSettled::Ignored;
        }
        let pending = self.pending.take().expect("matched above");
        match result {
            Ok(()) => MemoryModeSettled::Applied,
            Err(error) => {
                self.generate_memories = pending.previous;
                MemoryModeSettled::RolledBack(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_chat_carries_its_choice_into_the_started_thread_only() {
        let mut memory = ConversationMemory {
            new_chat: Some(AgentMemoryPreferences {
                use_memories: false,
                generate_memories: true,
            }),
            ..Default::default()
        };
        memory.reset(Some("t".into()));
        assert_eq!(
            (memory.use_memories, memory.generate_memories),
            (Some(false), Some(true))
        );
        assert_eq!(memory.new_chat, None);
        // Opening another chat never inherits it.
        memory.reset(Some("other".into()));
        assert_eq!(
            (memory.use_memories, memory.generate_memories),
            (None, None)
        );
    }

    #[test]
    fn a_failed_change_rolls_back_and_stale_answers_change_nothing() {
        let mut memory = ConversationMemory::default();
        memory.reset(Some("t".into()));
        let operation = memory.begin_generate("t", 3, false, true).unwrap();
        assert_eq!(memory.generate_memories, Some(false));
        assert_eq!(
            memory.begin_generate("t", 3, true, false),
            None,
            "one at a time"
        );
        // A reply for another generation or operation is ignored.
        assert_eq!(
            memory.settle_generate(operation, "t", 2, Ok(())),
            MemoryModeSettled::Ignored
        );
        assert_eq!(
            memory.settle_generate(operation + 1, "t", 3, Ok(())),
            MemoryModeSettled::Ignored
        );
        assert_eq!(
            memory.settle_generate(operation, "t", 3, Err("no rollout".into())),
            MemoryModeSettled::RolledBack("no rollout".into())
        );
        assert_eq!(memory.generate_memories, Some(true));
        // A duplicate of the same answer is inert.
        assert_eq!(
            memory.settle_generate(operation, "t", 3, Ok(())),
            MemoryModeSettled::Ignored
        );
        let operation = memory.begin_generate("t", 3, false, true).unwrap();
        assert_eq!(
            memory.settle_generate(operation, "t", 3, Ok(())),
            MemoryModeSettled::Applied
        );
        assert_eq!(memory.generate_memories, Some(false));
        // Switching chats while a change is in flight retires it.
        let operation = memory.begin_generate("t", 3, true, false).unwrap();
        memory.reset(Some("other".into()));
        assert_eq!(
            memory.settle_generate(operation, "t", 3, Ok(())),
            MemoryModeSettled::Ignored
        );
    }
}
