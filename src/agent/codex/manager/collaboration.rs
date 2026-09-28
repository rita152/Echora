//! Collaboration mode presets, read once per connection generation.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_channel::Receiver;
use serde_json::json;

use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{AgentCollaborationModePreset, AgentCollaborationModes};

impl CodexAppServerManager {
    /// Presets of this generation. The first successful answer is cached for
    /// the generation's lifetime, as the reference caches its own read; a failed
    /// read is not cached, so the next caller asks again.
    pub(super) fn collaboration_presets(
        &self,
        connection: &Arc<Connection>,
    ) -> Result<Vec<AgentCollaborationModePreset>> {
        if let Some(presets) = connection
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?
            .collaboration_modes
            .clone()
        {
            return Ok(presets);
        }
        let response = connection.request(super::super::collaboration::METHOD, json!({}))?;
        let presets = super::super::collaboration::parse_list_response(&response)?;
        let mut state = connection
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        Ok(state.collaboration_modes.get_or_insert(presets).clone())
    }

    pub(in crate::agent::codex) fn load_collaboration_modes(
        &self,
    ) -> Receiver<Result<AgentCollaborationModes, String>> {
        self.spawn_call(|manager| {
            let connection = manager.inner.ensure_connection()?;
            Ok(AgentCollaborationModes {
                generation: connection.generation,
                presets: manager.collaboration_presets(&connection)?,
            })
        })
    }
}
