//! Turns the server starts on its own.
//!
//! An active thread goal continues on an idle thread, and a completed turn
//! makes the server start the next queued follow-up; `thread/queue/start`
//! likewise starts a turn this client did not create with `turn/start`. Such a
//! turn arrives as a `turn/started` with no local owner. It is adopted as an
//! ordinary managed turn (same routing, approvals, and cleanup) and its event
//! stream is offered to the UI through the connection hub.

use std::sync::{Arc, Weak};

use anyhow::{Result, anyhow, bail};

use super::{
    ManagerInner,
    connection::{Connection, TurnKey},
    turn::{ManagedTurn, PromptControl},
};
use crate::agent::{
    AgentConnectionEvent, AgentExternalTurn, AgentInterruptControl, AgentInterruptHandle,
    AgentInterruptOutcome, AgentRun,
};

/// The UI observes but does not own a server-started turn: dropping its handle
/// (closing the chat, switching threads) must not interrupt the server's work.
/// An explicit stop still interrupts it.
struct ExternalTurnControl {
    control: Weak<PromptControl>,
}

impl AgentInterruptControl for ExternalTurnControl {
    fn request_interrupt(&self) -> std::result::Result<AgentInterruptOutcome, String> {
        match self.control.upgrade() {
            Some(control) => control.request_interrupt(),
            None => Ok(AgentInterruptOutcome::AlreadyFinished),
        }
    }

    fn abandon(&self) {}
}

impl ManagerInner {
    /// Adopts one server-started turn. Only a `turn/started` may introduce it;
    /// any other message for an unowned turn remains a protocol error.
    pub(super) fn adopt_server_turn(
        &self,
        connection: &Arc<Connection>,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<Arc<ManagedTurn>> {
        let (events, receiver) = async_channel::unbounded();
        let keepalive = receiver.clone();
        let control = Arc::new(PromptControl::default());
        let turn = ManagedTurn::new(
            thread_id.to_owned(),
            connection,
            events,
            keepalive,
            control.clone(),
        );
        {
            let mut state = connection
                .state
                .lock()
                .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
            let key = TurnKey {
                thread_id: thread_id.to_owned(),
                turn_id: turn_id.to_owned(),
            };
            if state.turns.contains_key(&key) {
                bail!("turn `{thread_id}`/`{turn_id}` 已被接管");
            }
            // Another turn of the thread may still be registered while a
            // replayed server turn is adopted (it ran while turn/start was
            // pending); the server stays the authority on which one is live.
            state.turns.insert(
                TurnKey {
                    thread_id: thread_id.to_owned(),
                    turn_id: turn_id.to_owned(),
                },
                turn.clone(),
            );
        }
        control.attach(&turn);
        // No buffered messages exist yet, so accepting only binds the id and
        // announces the turn identity on its own channel.
        turn.accept(turn_id)?;
        let interrupt = AgentInterruptHandle::new(Arc::new(ExternalTurnControl {
            control: Arc::downgrade(&control),
        }));
        let run = AgentExternalTurn::new(AgentRun::new(receiver, Some(interrupt)));
        connection
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?
            .server_turns
            .insert(thread_id.to_owned(), (turn_id.to_owned(), run.clone()));
        self.publish_connection_event(AgentConnectionEvent::TurnStarted {
            generation: connection.generation,
            thread_id: thread_id.to_owned(),
            turn_id: turn_id.to_owned(),
            run,
        });
        Ok(turn)
    }
}

impl super::CodexAppServerManager {
    /// The stream of a server-started turn of this thread that no view has
    /// claimed yet, with its turn id. Its events replay from the turn's start.
    pub(in crate::agent::codex) fn take_server_turn(
        &self,
        thread_id: &str,
    ) -> Option<(String, AgentRun)> {
        let connection = self.inner.current_connection()?;
        let state = connection.state.lock().ok()?;
        let (turn_id, run) = state.server_turns.get(thread_id)?;
        Some((turn_id.clone(), run.take()?))
    }
}
