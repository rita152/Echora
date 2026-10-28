//! Durable conversation forks use the shared lifecycle registry.
use super::{
    CodexAppServerManager,
    connection::{PendingThreadLifecycle, ThreadLifecycleKind},
};
use crate::agent::{ThreadId, WorkspaceResult};
use anyhow::{Context as _, anyhow, bail};
use async_channel::Receiver;
use serde_json::{Value, json};

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn fork_conversation(
        &self,
        thread_id: ThreadId,
        cwd: std::path::PathBuf,
        last_turn_id: Option<String>,
    ) -> Receiver<WorkspaceResult<ThreadId>> {
        self.workspace_call(move |manager| {
            if last_turn_id.as_ref().is_some_and(|id| id.trim().is_empty()) {
                bail!("A fork requires a completed turn id");
            }
            let connection = manager.inner.ensure_connection()?;
            let _lifecycle = connection.lifecycle_lock.lock()
                .map_err(|_| anyhow!("Codex thread lifecycle lock poisoned"))?;
            {
                let mut state = connection.state.lock()
                    .map_err(|_| anyhow!("Codex connection state lock poisoned"))?;
                if state.pending_thread_lifecycle.is_some() {
                    bail!("Codex thread lifecycle registry occupied");
                }
                state.pending_thread_lifecycle = Some(PendingThreadLifecycle {
                    kind: ThreadLifecycleKind::Fork,
                    observed_thread_id: None,
                });
            }
            let mut params = json!({
                "threadId": thread_id, "cwd": cwd, "ephemeral": false,
                "excludeTurns": true, "threadSource": "user",
                // Copy a saved goal without allowing it to start work before
                // the user has chosen a new instruction in the fork.
                "deferGoalContinuation": true,
            });
            if let Some(id) = &last_turn_id {
                params["lastTurnId"] = json!(id);
            }
            let response = match connection.request("thread/fork", params) {
                Ok(response) => response,
                Err(error) => {
                    if let Ok(mut state) = connection.state.lock() {
                        state.pending_thread_lifecycle = None;
                    }
                    return Err(error);
                }
            };
            let parsed = (|| {
                let id = response.pointer("/result/thread/id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty() && *id != thread_id)
                    .context("thread/fork missing a new thread id")?;
                if response.pointer("/result/thread/ephemeral").and_then(Value::as_bool) != Some(false) {
                    bail!("thread/fork did not create a durable chat");
                }
                let mut state = connection.state.lock()
                    .map_err(|_| anyhow!("Codex connection state lock poisoned"))?;
                let pending = state.pending_thread_lifecycle.take()
                    .context("thread/fork lifecycle registry empty")?;
                if pending.observed_thread_id.as_deref().is_some_and(|observed| observed != id) {
                    bail!("thread/fork response disagrees with thread/started");
                }
                if let Some(settings) = super::super::notifications::lifecycle_settings(&response)? {
                    state.thread_settings.insert(id.to_owned(), settings);
                }
                state.loaded_threads.insert(id.to_owned());
                Ok(id.to_owned())
            })();
            let id = super::workspace::validate_workspace_response(&connection, "thread/fork", parsed)?;
            if let Some(last_turn_id) = last_turn_id {
                // Older servers can ignore unknown fields. Never publish a
                // successful bounded fork until its stored tail is confirmed.
                let verified = (|| {
                    if response.pointer("/result/thread/forkedFromId").and_then(Value::as_str)
                        != Some(thread_id.as_str()) {
                        bail!("The fork did not preserve its source chat identity");
                    }
                    let tail = connection.request("thread/turns/list", json!({
                        "threadId": id, "limit": 1, "sortDirection": "desc", "itemsView": "notLoaded",
                    }))?;
                    if tail.pointer("/result/data/0/id").and_then(Value::as_str) != Some(last_turn_id.as_str()) {
                        bail!("The server did not fork through the selected reply; update Codex and retry");
                    }
                    Ok(())
                })();
                if let Err(error) = verified {
                    // This is a fresh, unused child owned by this operation.
                    // Archive it rather than presenting an incorrect branch.
                    let cleanup = connection.request("thread/archive", json!({"threadId": id}));
                    return Err(error).context(if cleanup.is_ok() {
                        "Fork verification failed; the unused fork was archived"
                    } else {
                        "Fork verification failed; unable to archive the unused fork"
                    });
                }
            }
            Ok(id)
        })
    }
}
