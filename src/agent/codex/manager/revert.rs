//! Thread history revert: request encoding, response validation, and the
//! notification that may arrive before that response.
//!
//! Only persisted conversation history changes; local files are untouched. The
//! response is the authoritative source of the updated thread, so the caller
//! reloads turns through the normal pagination path afterwards.

use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{
    AgentConnectionEvent, AgentThreadRevert, AgentThreadRevertOutcome, WorkspaceResult,
};
use anyhow::{Context as _, Result, bail};
use async_channel::Receiver;
use serde_json::{Value, json};

use super::super::workspace_protocol::response_result;
use super::workspace::validate_workspace_response;

pub(super) fn build_revert_params(request: &AgentThreadRevert) -> Value {
    json!({
        "threadId": request.thread_id,
        "beforeTurnId": request.before_turn_id,
    })
}

/// Decodes the revert response. The thread is returned without turns; the
/// optional backwards cursors are kept so the caller can hydrate retained
/// history exactly like every other paginated read.
pub(super) fn parse_revert_response(response: &Value) -> Result<AgentThreadRevertOutcome> {
    let result = response_result(response, "thread/revert")?;
    let thread = super::super::workspace_protocol::parse_thread_summary(
        result
            .get("thread")
            .context("thread/revert result 缺少 thread")?,
    )?;
    let turns_backwards_cursor = backwards_cursor(result, "turnsBackwardsCursor")?;
    let items_backwards_cursor = backwards_cursor(result, "itemsBackwardsCursor")?;
    Ok(AgentThreadRevertOutcome {
        thread,
        turns_backwards_cursor,
        items_backwards_cursor,
    })
}

/// The response carries one opaque backwards cursor per history shape. They
/// are optional, but when present they must be strings.
fn backwards_cursor(result: &Value, field: &str) -> Result<Option<String>> {
    match result.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(cursor)) => Ok(Some(cursor.clone())),
        Some(_) => bail!("thread/revert result.{field} 必须是字符串或 null"),
    }
}

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn revert_thread(
        &self,
        request: AgentThreadRevert,
    ) -> Receiver<WorkspaceResult<AgentThreadRevertOutcome>> {
        self.workspace_call(move |manager| manager.revert_thread_blocking(request))
    }

    pub(super) fn revert_thread_blocking(
        &self,
        request: AgentThreadRevert,
    ) -> Result<AgentThreadRevertOutcome> {
        if request.before_turn_id.trim().is_empty() {
            bail!("thread/revert 需要 beforeTurnId");
        }
        let connection = self.inner.ensure_connection()?;
        self.validate_temporary_thread(&connection, &request.thread_id)?;
        let thread_id = request.thread_id.clone();
        connection.begin_revert(&thread_id)?;
        // The notification may already be produced by the time the response is
        // read; keep the expectation until both sides have been observed.
        let response = connection.request("thread/revert", build_revert_params(&request));
        let observed = connection.revert_observed(&thread_id);
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                connection.finish_revert(&thread_id, observed);
                if observed {
                    // The server confirmed the revert with a notification even
                    // though the request itself failed: the history on disk no
                    // longer matches what the client shows.
                    self.inner
                        .publish_connection_event(AgentConnectionEvent::ThreadReverted {
                            thread_id: thread_id.clone(),
                        });
                }
                return Err(error).context("thread/revert 请求失败");
            }
        };
        let outcome = validate_workspace_response(
            &connection,
            "thread/revert",
            parse_revert_response(&response),
        );
        // The response is authoritative; a later duplicate notification is
        // inert rather than a duplicate event.
        connection.finish_revert(&thread_id, true);
        if let Ok(outcome) = &outcome
            && outcome.thread.thread_id != thread_id
        {
            let message = format!(
                "thread/revert 响应返回了其他线程 {}",
                outcome.thread.thread_id
            );
            connection.fail_protocol(message.clone());
            bail!(message);
        }
        outcome
    }
}

impl super::ManagerInner {
    /// Handles one thread/reverted notification. While a revert request is in
    /// flight this is that request's confirmation; otherwise the history of a
    /// thread changed outside this client's request and the UI is told.
    pub(super) fn handle_thread_reverted(
        &self,
        connection: &std::sync::Arc<Connection>,
        message: &Value,
    ) -> Result<()> {
        let thread_id = super::super::notifications::parse_thread_reverted(message)?;
        if connection.observe_reverted(&thread_id)? {
            return Ok(());
        }
        self.publish_connection_event(AgentConnectionEvent::ThreadReverted { thread_id });
        Ok(())
    }
}
