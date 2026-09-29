//! Experimental feature reads and the memory operations.

use std::collections::HashSet;

use anyhow::bail;
use async_channel::Receiver;

use super::CodexAppServerManager;
use crate::agent::{AgentExperimentalFeatures, AgentThreadMemoryMode};

/// The list is about 150 flags in pages of 100; the bound only stops a server
/// that never ends its cursor chain.
const MAX_FEATURE_PAGES: usize = 32;

impl CodexAppServerManager {
    /// Reads every page, as the reference does. A repeated cursor or a flag
    /// listed twice is a protocol error rather than a silently merged list.
    pub(in crate::agent::codex) fn list_experimental_features(
        &self,
        thread_id: Option<String>,
    ) -> Receiver<Result<AgentExperimentalFeatures, String>> {
        self.spawn_call(move |manager| {
            use super::super::features::{FEATURE_LIST_METHOD, list_params, parse_list_page};
            let connection = manager.inner.ensure_connection()?;
            let mut features = Vec::new();
            let mut names = HashSet::new();
            let mut cursors = HashSet::new();
            let mut cursor = None::<String>;
            for _ in 0..MAX_FEATURE_PAGES {
                let response = connection.request(
                    FEATURE_LIST_METHOD,
                    list_params(cursor.as_deref(), thread_id.as_deref()),
                )?;
                let (page, next) = parse_list_page(&response)?;
                for feature in page {
                    if !names.insert(feature.name.clone()) {
                        bail!("experimentalFeature/list 重复返回 {}", feature.name);
                    }
                    features.push(feature);
                }
                match next {
                    None => {
                        return Ok(AgentExperimentalFeatures {
                            generation: connection.generation,
                            features,
                        });
                    }
                    Some(next) if !cursors.insert(next.clone()) => {
                        bail!("experimentalFeature/list 返回了重复的游标 {next}")
                    }
                    Some(next) => cursor = Some(next),
                }
            }
            bail!("experimentalFeature/list 超过 {MAX_FEATURE_PAGES} 页，已停止读取")
        })
    }

    /// Changes whether a started chat may generate memories. Bound to the
    /// generation the caller showed; the thread must be loaded there.
    pub(in crate::agent::codex) fn set_thread_memory_mode(
        &self,
        thread_id: String,
        generation: u64,
        mode: AgentThreadMemoryMode,
    ) -> Receiver<Result<(), String>> {
        self.spawn_call(move |manager| {
            use super::super::features::{
                MEMORY_MODE_SET_METHOD, memory_mode_params, parse_empty_result,
            };
            let connection = manager.connection_for_generation(generation)?;
            manager.validate_temporary_thread(&connection, &thread_id)?;
            manager.ensure_thread_loaded(&connection, Some(&thread_id), None)?;
            let response =
                connection.request(MEMORY_MODE_SET_METHOD, memory_mode_params(&thread_id, mode))?;
            parse_empty_result(&response, MEMORY_MODE_SET_METHOD)
        })
    }

    /// Deletes every Codex memory. Sent without params, as the reference does.
    pub(in crate::agent::codex) fn reset_memories(&self) -> Receiver<Result<(), String>> {
        self.spawn_call(move |manager| {
            use super::super::features::{MEMORY_RESET_METHOD, parse_empty_result};
            let connection = manager.inner.ensure_connection()?;
            let receiver = connection.begin_request_with_params(MEMORY_RESET_METHOD, None)?;
            let response = receiver
                .recv_blocking()
                .map_err(|_| anyhow::anyhow!("memory/reset 连接在返回前关闭"))?
                .map_err(anyhow::Error::msg)?;
            parse_empty_result(&response, MEMORY_RESET_METHOD)
        })
    }
}
