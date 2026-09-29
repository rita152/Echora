//! Work that starts without a prompt: `review/start` and
//! `thread/shellCommand`.
//!
//! A review is a turn this client starts, so it is registered and bound like
//! `turn/start` (see `turn.rs`) and flagged so its alias `turn/started` is
//! never adopted as a server turn. A shell command is only acknowledged: its
//! output arrives in the thread's turn, which on an idle thread is a turn the
//! server starts and the connection hub offers to the view.

use std::sync::Arc;

use anyhow::Result;
use async_channel::{Receiver, Sender};

use super::{
    CodexAppServerManager, connection::Connection, turn::ManagedTurn, turn::PromptControl,
};
use crate::agent::{
    AgentEvent, AgentRequest, AgentReviewRequest, AgentRun, AgentShellCommandRequest,
    AgentShellCommandStarted, AgentThreadTarget,
};

/// `threadSource` of a thread started for a review, as the reference names it.
const REVIEW_THREAD_SOURCE: &str = "code_review";
const SHELL_THREAD_SOURCE: &str = "user";

/// The `thread/start` request for a new thread; nothing is prompted.
fn thread_start_request(target: &AgentThreadTarget) -> AgentRequest {
    AgentRequest {
        client_message_id: None,
        prompt: String::new(),
        cwd: target.cwd.clone(),
        project_id: target.project_id.clone(),
        thread_id: None,
        model: target.model.clone(),
        effort: String::new(),
        service_tier: target.service_tier.clone(),
        permission_mode: target.permission_mode.clone(),
        context: Default::default(),
    }
}

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn run_review(&self, request: AgentReviewRequest) -> AgentRun {
        self.spawn_turn(move |manager, events, keepalive, control| {
            manager.run_review_blocking(request, events, keepalive, control)
        })
    }

    fn run_review_blocking(
        &self,
        request: AgentReviewRequest,
        events: Sender<AgentEvent>,
        keepalive: Receiver<AgentEvent>,
        control: Arc<PromptControl>,
    ) -> Result<()> {
        use super::super::review::{REVIEW_START_METHOD, parse_review_turn_id, review_params};
        if control.is_abandoned() {
            let _ = events.send_blocking(AgentEvent::Interrupted);
            control.mark_terminal();
            return Ok(());
        }
        let connection = self.inner.ensure_connection()?;
        let thread_id = match request.thread.thread_id.as_deref() {
            Some(thread_id) => {
                connection.reserve_thread(thread_id)?;
                if let Err(error) = self.ensure_thread_loaded(&connection, Some(thread_id), None) {
                    connection.release_reservation(thread_id);
                    return Err(error);
                }
                thread_id.to_owned()
            }
            None => {
                let thread_id = self.start_prompt_less_thread(
                    &connection,
                    &thread_start_request(&request.thread),
                    REVIEW_THREAD_SOURCE,
                )?;
                connection.reserve_thread(&thread_id)?;
                if events
                    .send_blocking(AgentEvent::ThreadCreated {
                        thread_id: thread_id.clone(),
                    })
                    .is_err()
                {
                    connection.release_reservation(&thread_id);
                    control.mark_terminal();
                    return Ok(());
                }
                thread_id
            }
        };
        if control.is_abandoned() {
            connection.release_reservation(&thread_id);
            let _ = events.send_blocking(AgentEvent::Interrupted);
            control.mark_terminal();
            return Ok(());
        }
        let turn = ManagedTurn::new(
            thread_id.clone(),
            &connection,
            events,
            keepalive,
            control.clone(),
        );
        turn.review
            .store(true, std::sync::atomic::Ordering::Release);
        connection.register_starting_turn(turn.clone())?;
        control.attach(&turn);
        self.start_registered_turn(
            &connection,
            &turn,
            REVIEW_START_METHOD,
            review_params(&thread_id, &request.target),
            |response| parse_review_turn_id(response, &thread_id),
        );
        Ok(())
    }

    /// Asks the thread's shell to run a command. The answer only confirms the
    /// server took it; a rejected command (empty, bad timeout, unknown thread)
    /// is reported and never retried.
    pub(in crate::agent::codex) fn run_shell_command(
        &self,
        request: AgentShellCommandRequest,
    ) -> Receiver<Result<AgentShellCommandStarted, String>> {
        self.spawn_call(move |manager| {
            use super::super::shell::{SHELL_COMMAND_METHOD, parse_shell_ack, shell_params};
            let connection = manager.inner.ensure_connection()?;
            let (thread_id, created_thread) = match request.thread.thread_id.as_deref() {
                Some(thread_id) => (
                    manager.ensure_thread_loaded(&connection, Some(thread_id), None)?,
                    false,
                ),
                None => (
                    manager.start_prompt_less_thread(
                        &connection,
                        &thread_start_request(&request.thread),
                        SHELL_THREAD_SOURCE,
                    )?,
                    true,
                ),
            };
            let response = connection.request(
                SHELL_COMMAND_METHOD,
                shell_params(&thread_id, &request.command, request.timeout_ms),
            )?;
            if let Err(error) = parse_shell_ack(&response) {
                fail_on_mismatch(&connection, &error);
                return Err(error);
            }
            Ok(AgentShellCommandStarted {
                generation: connection.generation,
                thread_id,
                created_thread,
            })
        })
    }
}

fn fail_on_mismatch(connection: &Connection, error: &anyhow::Error) {
    connection.fail_protocol(format!(
        "无法解析 thread/shellCommand 响应；与 app-server schema 不匹配：{error:#}"
    ));
}
