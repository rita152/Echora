//! Configured lifecycle hooks (`hooks/list`) and the per-hook state a user
//! can change: whether it is enabled and which definition hash is trusted.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AgentHookEventName {
    PreToolUse,
    PermissionRequest,
    PostToolUse,
    PreCompact,
    PostCompact,
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    SubagentStart,
    SubagentStop,
    Stop,
    Interrupt,
}

impl AgentHookEventName {
    pub const ALL: [Self; 12] = [
        Self::PreToolUse,
        Self::PermissionRequest,
        Self::PostToolUse,
        Self::PreCompact,
        Self::PostCompact,
        Self::SessionStart,
        Self::SessionEnd,
        Self::UserPromptSubmit,
        Self::SubagentStart,
        Self::SubagentStop,
        Self::Stop,
        Self::Interrupt,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentHookSource {
    System,
    User,
    Project,
    Mdm,
    SessionFlags,
    Plugin,
    CloudRequirements,
    CloudManagedConfig,
    LegacyManagedConfigFile,
    LegacyManagedConfigMdm,
    Unknown,
}

impl AgentHookSource {
    pub const ALL: [Self; 11] = [
        Self::System,
        Self::User,
        Self::Project,
        Self::Mdm,
        Self::SessionFlags,
        Self::Plugin,
        Self::CloudRequirements,
        Self::CloudManagedConfig,
        Self::LegacyManagedConfigFile,
        Self::LegacyManagedConfigMdm,
        Self::Unknown,
    ];

    /// The settings page's source group, as the reference folds sources.
    pub fn group(self) -> AgentHookSourceGroup {
        match self {
            Self::User => AgentHookSourceGroup::User,
            Self::Project => AgentHookSourceGroup::Project,
            Self::Plugin => AgentHookSourceGroup::Plugin,
            Self::SessionFlags => AgentHookSourceGroup::SessionFlags,
            Self::Unknown => AgentHookSourceGroup::Unknown,
            Self::System
            | Self::Mdm
            | Self::CloudRequirements
            | Self::CloudManagedConfig
            | Self::LegacyManagedConfigFile
            | Self::LegacyManagedConfigMdm => AgentHookSourceGroup::Admin,
        }
    }
}

/// Source groups in the order the settings page shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AgentHookSourceGroup {
    Plugin,
    User,
    Admin,
    Project,
    SessionFlags,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentHookHandler {
    Command { command: String, is_async: bool },
    McpTool { server: String, tool: String },
    Prompt,
    Agent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentHookTrustStatus {
    Managed,
    Untrusted,
    Trusted,
    Modified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHook {
    pub key: String,
    pub event_name: AgentHookEventName,
    pub handler: AgentHookHandler,
    pub matcher: Option<String>,
    pub timeout_sec: u64,
    pub status_message: Option<String>,
    pub source: AgentHookSource,
    pub source_path: PathBuf,
    pub plugin_id: Option<String>,
    pub display_order: i64,
    pub enabled: bool,
    pub is_managed: bool,
    pub current_hash: String,
    pub trust_status: AgentHookTrustStatus,
    pub additional_context_limit: Option<u64>,
}

impl AgentHook {
    /// New or changed since last trusted: it stays off until reviewed.
    pub fn needs_review(&self) -> bool {
        matches!(
            self.trust_status,
            AgentHookTrustStatus::Untrusted | AgentHookTrustStatus::Modified
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHookLoadError {
    pub path: String,
    pub message: String,
}

/// One `hooks/list` entry: everything loaded for one working directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHookListEntry {
    pub cwd: PathBuf,
    pub hooks: Vec<AgentHook>,
    pub warnings: Vec<String>,
    pub errors: Vec<AgentHookLoadError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHooksSnapshot {
    pub generation: u64,
    pub cwds: Vec<PathBuf>,
    pub entries: Vec<AgentHookListEntry>,
}

/// A change to one hook's user-owned state. Either field may be omitted; the
/// write only names what the user changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHookStateChange {
    pub key: String,
    pub enabled: Option<bool>,
    pub trusted_hash: Option<String>,
}

impl AgentHookStateChange {
    /// `hooks.state.<JSON-quoted key>.enabled` and `.trusted_hash` in the user
    /// config: the hook key holds dots and colons, so it is one quoted segment.
    pub fn edits(&self) -> Vec<super::AgentConfigEdit> {
        let prefix = format!(
            "hooks.state.{}",
            serde_json::Value::String(self.key.clone())
        );
        let mut edits = Vec::new();
        if let Some(enabled) = self.enabled {
            edits.push(super::AgentConfigEdit {
                key: format!("{prefix}.enabled"),
                value: serde_json::Value::Bool(enabled),
            });
        }
        if let Some(hash) = &self.trusted_hash {
            edits.push(super::AgentConfigEdit {
                key: format!("{prefix}.trusted_hash"),
                value: serde_json::Value::String(hash.clone()),
            });
        }
        edits
    }
}
