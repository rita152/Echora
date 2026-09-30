//! A scripted backend for the settings pages that read app-server data:
//! every read answers from fixed data and every write is kept for the test.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde_json::{Value, json};

use crate::agent::{
    AgentBackend, AgentConfigError, AgentConfigLayer, AgentConfigReceipt, AgentConfigSaveResult,
    AgentConfigSnapshot, AgentConfigSource, AgentConfigWrite, AgentConnectionEvent,
    AgentExperimentalFeatures, AgentHooksSnapshot, AgentModelCatalog, AgentPermissionProfile,
    AgentRequest, AgentRun,
};

type ConfigReply = async_channel::Sender<Result<AgentConfigSaveResult, AgentConfigError>>;

pub(super) const USER_CONFIG: &str = "/home/me/.codex/config.toml";

#[derive(Default)]
pub(super) struct Script {
    pub(super) config: Value,
    pub(super) version: u64,
    pub(super) hooks: Option<AgentHooksSnapshot>,
    pub(super) features: Option<AgentExperimentalFeatures>,
    pub(super) hook_reads: usize,
    pub(super) feature_reads: usize,
    pub(super) writes: Vec<(AgentConfigWrite, ConfigReply)>,
    pub(super) resets: Vec<async_channel::Sender<Result<(), String>>>,
    /// `memory/status` answers; `None` answers with an error.
    pub(super) memory_status: Option<crate::agent::AgentMemoryStatus>,
    pub(super) memory_status_reads: usize,
    pub(super) capabilities: Option<crate::agent::AgentProviderCapabilities>,
    pub(super) capability_reads: usize,
    /// Replayed to every connection event subscriber, like the hub snapshots.
    pub(super) connection_events: Vec<AgentConnectionEvent>,
}

#[derive(Default)]
pub(super) struct Backend {
    pub(super) script: Arc<Mutex<Script>>,
    /// Keeps each subscription open for the lifetime of the test.
    subscribers: Mutex<Vec<async_channel::Sender<AgentConnectionEvent>>>,
}

pub(super) fn snapshot(config: &Value, version: u64) -> AgentConfigSnapshot {
    let source = AgentConfigSource {
        metadata: json!({"type": "user", "file": USER_CONFIG}),
        kind: "user".into(),
        path: Some(USER_CONFIG.into()),
        name: None,
        profile: None,
        version: format!("sha256:{version}"),
    };
    AgentConfigSnapshot {
        generation: 1,
        cwd: "/work".into(),
        effective: config.clone(),
        origins: Default::default(),
        layers: Some(vec![AgentConfigLayer {
            source,
            config: config.clone(),
            disabled_reason: None,
        }]),
        requirements: None,
        value_aliases: Default::default(),
        value_defaults: Default::default(),
        profile_parents: Default::default(),
        required_fields: Default::default(),
    }
}

impl Script {
    /// Applies the oldest write to the stored config and answers it with the
    /// receipt and readback a server gives.
    pub(super) fn answer_write(&mut self, status: &str) -> AgentConfigWrite {
        let (write, reply) = self.writes.remove(0);
        for edit in &write.edits {
            let segments = crate::agent::key_path_segments(&edit.key);
            let mut node = &mut self.config;
            for segment in &segments[..segments.len() - 1] {
                if !node.get(segment).is_some_and(Value::is_object) {
                    node[segment] = json!({});
                }
                node = node.get_mut(segment).unwrap();
            }
            let last = segments.last().unwrap();
            if edit.value.is_null() {
                node.as_object_mut().unwrap().remove(last);
            } else {
                node[last] = edit.value.clone();
            }
        }
        self.version += 1;
        let receipt = AgentConfigReceipt {
            status: status.into(),
            version: format!("sha256:{}", self.version),
            file_path: USER_CONFIG.into(),
            overridden: None,
        };
        reply
            .send_blocking(Ok(AgentConfigSaveResult {
                receipt,
                readback: Ok(snapshot(&self.config, self.version)),
            }))
            .unwrap();
        write
    }
}

impl AgentBackend for Backend {
    fn subscribe_connection_events(&self) -> async_channel::Receiver<AgentConnectionEvent> {
        let (sender, receiver) = async_channel::unbounded();
        for event in &self.script.lock().unwrap().connection_events {
            let _ = sender.send_blocking(event.clone());
        }
        self.subscribers.lock().unwrap().push(sender);
        receiver
    }
    fn load_model_catalog(&self) -> async_channel::Receiver<Result<AgentModelCatalog, String>> {
        async_channel::bounded(1).1
    }
    fn load_permission_profiles(
        &self,
        _cwd: PathBuf,
    ) -> async_channel::Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        let (reply, receiver) = async_channel::bounded(1);
        reply.send_blocking(Ok(Vec::new())).unwrap();
        receiver
    }
    fn update_thread_permissions(
        &self,
        _request: crate::agent::AgentThreadPermissionUpdate,
    ) -> async_channel::Receiver<Result<crate::agent::AgentThreadPermissionResult, String>> {
        async_channel::bounded(1).1
    }
    fn run_prompt(&self, _request: AgentRequest) -> AgentRun {
        AgentRun::new(async_channel::unbounded().1, None)
    }
    fn read_config(
        &self,
        _cwd: PathBuf,
    ) -> async_channel::Receiver<Result<AgentConfigSnapshot, AgentConfigError>> {
        let script = self.script.lock().unwrap();
        let (reply, receiver) = async_channel::bounded(1);
        reply
            .send_blocking(Ok(snapshot(&script.config, script.version)))
            .unwrap();
        receiver
    }
    fn write_config(
        &self,
        write: AgentConfigWrite,
    ) -> async_channel::Receiver<Result<AgentConfigSaveResult, AgentConfigError>> {
        let (reply, receiver) = async_channel::bounded(1);
        self.script.lock().unwrap().writes.push((write, reply));
        receiver
    }
    fn list_hooks(
        &self,
        _cwds: Vec<PathBuf>,
    ) -> async_channel::Receiver<Result<AgentHooksSnapshot, String>> {
        let mut script = self.script.lock().unwrap();
        script.hook_reads += 1;
        let (reply, receiver) = async_channel::bounded(1);
        reply
            .send_blocking(script.hooks.clone().ok_or_else(|| "no hooks".to_owned()))
            .unwrap();
        receiver
    }
    fn list_experimental_features(
        &self,
        _thread_id: Option<String>,
    ) -> async_channel::Receiver<Result<AgentExperimentalFeatures, String>> {
        let mut script = self.script.lock().unwrap();
        script.feature_reads += 1;
        let (reply, receiver) = async_channel::bounded(1);
        reply
            .send_blocking(
                script
                    .features
                    .clone()
                    .ok_or_else(|| "no features".to_owned()),
            )
            .unwrap();
        receiver
    }
    fn reset_memories(&self) -> async_channel::Receiver<Result<(), String>> {
        let (reply, receiver) = async_channel::bounded(1);
        self.script.lock().unwrap().resets.push(reply);
        receiver
    }
    fn read_memory_status(
        &self,
        required_threads: u32,
    ) -> async_channel::Receiver<Result<crate::agent::AgentMemoryStatus, String>> {
        let mut script = self.script.lock().unwrap();
        script.memory_status_reads += 1;
        let (reply, receiver) = async_channel::bounded(1);
        reply
            .send_blocking(
                script
                    .memory_status
                    .map(|status| crate::agent::AgentMemoryStatus {
                        required_threads,
                        ..status
                    })
                    .ok_or_else(|| "memories unavailable".to_owned()),
            )
            .unwrap();
        receiver
    }
    fn read_provider_capabilities(
        &self,
    ) -> async_channel::Receiver<Result<crate::agent::AgentProviderCapabilities, String>> {
        let mut script = self.script.lock().unwrap();
        script.capability_reads += 1;
        let (reply, receiver) = async_channel::bounded(1);
        reply
            .send_blocking(script.capabilities.ok_or_else(|| "unknown".to_owned()))
            .unwrap();
        receiver
    }
}
