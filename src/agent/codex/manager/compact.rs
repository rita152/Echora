//! Manual context compaction.
//!
//! A manual compaction is a non-steerable turn on the server side: while it
//! runs, turn/steer fails with activeTurnNotSteerable{turnKind: "compact"},
//! and the completion arrives through the ordinary turn and item stream.

use super::CodexAppServerManager;
use anyhow::{Context as _, Result, bail};
use async_channel::Receiver;
use serde_json::json;

impl CodexAppServerManager {
    /// Starts one manual compaction for a thread. The empty response only
    /// acknowledges the request; progress and completion arrive as turn events.
    pub(in crate::agent::codex) fn start_thread_compaction(
        &self,
        thread_id: String,
    ) -> Receiver<Result<(), String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                if thread_id.trim().is_empty() {
                    bail!("thread/compact/start 需要 threadId");
                }
                let connection = manager.inner.ensure_connection()?;
                manager.validate_temporary_thread(&connection, &thread_id)?;
                let response =
                    connection.request("thread/compact/start", json!({ "threadId": thread_id }))?;
                let result = response
                    .get("result")
                    .context("thread/compact/start 响应缺少 result")?;
                if !result.is_object() {
                    bail!("thread/compact/start result 必须是对象");
                }
                Ok(())
            })()
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }
}
