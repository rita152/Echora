//! Thread goal reads and writes, bound to one connection generation.
//!
//! Reads may open a connection and report the generation they used; writes
//! must name the generation the caller read from, so a click made against an
//! older connection never reaches a newer one.

use std::sync::{Arc, atomic::Ordering};

use anyhow::{Result, anyhow, bail};
use async_channel::Receiver;
use serde_json::json;

use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{AgentThreadGoalRead, AgentThreadGoalUpdate};

impl CodexAppServerManager {
    /// The current connection, only when it is still the named generation.
    pub(super) fn connection_for_generation(&self, generation: u64) -> Result<Arc<Connection>> {
        let connection = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow!("Codex manager state 锁已损坏"))?
            .current
            .clone()
            .ok_or_else(|| anyhow!("连接已断开，操作未发送。请重新打开会话后重试。"))?;
        if connection.generation != generation || connection.failed.load(Ordering::Acquire) {
            bail!("会话所属的连接已变化，操作未发送。请刷新后重试。");
        }
        Ok(connection)
    }

    pub(super) fn spawn_call<T: Send + 'static>(
        &self,
        call: impl FnOnce(&CodexAppServerManager) -> Result<T> + Send + 'static,
    ) -> Receiver<Result<T, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let _ = sender.send_blocking(call(&manager).map_err(|error| format!("{error:#}")));
        });
        receiver
    }

    pub(in crate::agent::codex) fn read_thread_goal(
        &self,
        thread_id: String,
    ) -> Receiver<Result<AgentThreadGoalRead, String>> {
        self.spawn_call(move |manager| {
            let connection = manager.inner.ensure_connection()?;
            manager.validate_temporary_thread(&connection, &thread_id)?;
            let response =
                connection.request("thread/goal/get", json!({ "threadId": thread_id }))?;
            let goal = super::super::goal::parse_get_response(&response, &thread_id)?;
            Ok(AgentThreadGoalRead {
                generation: connection.generation,
                thread_id,
                goal,
            })
        })
    }

    /// Setting a goal can start a continuation turn right away, so the thread
    /// is loaded in this generation first; that turn then arrives as an
    /// ordinary server-started turn.
    pub(in crate::agent::codex) fn update_thread_goal(
        &self,
        update: AgentThreadGoalUpdate,
    ) -> Receiver<Result<AgentThreadGoalRead, String>> {
        self.spawn_call(move |manager| {
            let connection = manager.connection_for_generation(update.generation)?;
            manager.ensure_thread_loaded(&connection, Some(&update.thread_id), None)?;
            let response =
                connection.request("thread/goal/set", super::super::goal::set_params(&update))?;
            let goal = super::super::goal::parse_set_response(&response, &update.thread_id)?;
            Ok(AgentThreadGoalRead {
                generation: connection.generation,
                thread_id: update.thread_id,
                goal: Some(goal),
            })
        })
    }

    pub(in crate::agent::codex) fn clear_thread_goal(
        &self,
        thread_id: String,
        generation: u64,
    ) -> Receiver<Result<bool, String>> {
        self.spawn_call(move |manager| {
            let connection = manager.connection_for_generation(generation)?;
            manager.validate_temporary_thread(&connection, &thread_id)?;
            let response =
                connection.request("thread/goal/clear", json!({ "threadId": thread_id }))?;
            super::super::goal::parse_clear_response(&response)
        })
    }
}
