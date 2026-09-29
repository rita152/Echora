//! `hooks/list` reads. Trust and enable changes are ordinary user config
//! writes (see `manager/config.rs`); only the inventory lives here.

use std::path::PathBuf;

use async_channel::Receiver;

use super::CodexAppServerManager;
use crate::agent::AgentHooksSnapshot;

impl CodexAppServerManager {
    /// Lists hooks for the given working directories on the current
    /// connection; the snapshot names the generation it was read from.
    pub(in crate::agent::codex) fn list_hooks(
        &self,
        cwds: Vec<PathBuf>,
    ) -> Receiver<Result<AgentHooksSnapshot, String>> {
        self.spawn_call(move |manager| {
            use super::super::hooks::{HOOKS_LIST_METHOD, list_params, parse_list_response};
            let connection = manager.inner.ensure_connection()?;
            let response = connection.request(HOOKS_LIST_METHOD, list_params(&cwds))?;
            parse_list_response(connection.generation, &cwds, &response)
        })
    }
}
