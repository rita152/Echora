//! Detection, import and import history for configuration owned by other
//! coding agents. The server reports every migrated item explicitly; the client
//! never invents a source, an item type, or a result.

use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

/// The item kinds the migration protocol defines. The vocabulary is closed by
/// the schema, so an unknown value is a protocol error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AgentExternalAgentItemType {
    AgentsMd,
    Config,
    Skills,
    Plugins,
    McpServerConfig,
    Subagents,
    Hooks,
    Commands,
    Memory,
    Sessions,
}

impl AgentExternalAgentItemType {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "AGENTS_MD" => Some(Self::AgentsMd),
            "CONFIG" => Some(Self::Config),
            "SKILLS" => Some(Self::Skills),
            "PLUGINS" => Some(Self::Plugins),
            "MCP_SERVER_CONFIG" => Some(Self::McpServerConfig),
            "SUBAGENTS" => Some(Self::Subagents),
            "HOOKS" => Some(Self::Hooks),
            "COMMANDS" => Some(Self::Commands),
            "MEMORY" => Some(Self::Memory),
            "SESSIONS" => Some(Self::Sessions),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentsMd => "AGENTS_MD",
            Self::Config => "CONFIG",
            Self::Skills => "SKILLS",
            Self::Plugins => "PLUGINS",
            Self::McpServerConfig => "MCP_SERVER_CONFIG",
            Self::Subagents => "SUBAGENTS",
            Self::Hooks => "HOOKS",
            Self::Commands => "COMMANDS",
            Self::Memory => "MEMORY",
            Self::Sessions => "SESSIONS",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentExternalAgentConnectorSource {
    RemoteMcpServersConfig,
    SessionToolUse,
}

impl AgentExternalAgentConnectorSource {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "remoteMcpServersConfig" => Some(Self::RemoteMcpServersConfig),
            "sessionToolUse" => Some(Self::SessionToolUse),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentExternalAgentImportedConnectorSource {
    RemoteMcpServersConfig,
}

impl AgentExternalAgentImportedConnectorSource {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "remoteMcpServersConfig" => Some(Self::RemoteMcpServersConfig),
            _ => None,
        }
    }
}

/// One migration candidate. `details` is a schema union the client stores
/// verbatim: it is what the server offered to move, and never a value this
/// client derives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentMigrationItem {
    pub item_type: AgentExternalAgentItemType,
    pub description: String,
    pub cwd: Option<String>,
    pub details: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentDetectedConnector {
    pub name: String,
    pub session_count: i64,
    pub source: AgentExternalAgentConnectorSource,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentDetectResult {
    pub generation: u64,
    /// The exact request that produced this answer; a newer read supersedes it.
    pub request: AgentExternalAgentDetectRequest,
    pub items: Vec<AgentExternalAgentMigrationItem>,
    /// `None` when the server omitted `connectors` entirely, which is not the
    /// same as an empty list.
    pub connectors: Option<Vec<AgentExternalAgentDetectedConnector>>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AgentExternalAgentDetectRequest {
    /// Working directories to inspect. `None` sends no `cwds` key at all.
    pub cwds: Option<Vec<PathBuf>>,
    pub include_home: bool,
    pub max_session_age_days: Option<i64>,
    pub max_sessions: Option<i64>,
    /// Present only for the provider whose detection goes through this field;
    /// the reference client sends it for Cursor alone.
    pub migration_source: Option<String>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentImportRequest {
    pub migration_items: Vec<AgentExternalAgentMigrationItem>,
    pub migration_source: Option<String>,
    pub provider_id: Option<String>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentImportReceipt {
    pub import_id: String,
}

/// One successfully migrated item, exactly as the server reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentSuccess {
    pub item_type: AgentExternalAgentItemType,
    pub cwd: Option<String>,
    pub source: Option<String>,
    pub target: Option<String>,
    pub title: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentFailure {
    pub item_type: AgentExternalAgentItemType,
    /// Always present in a failure; the client renders it as reported.
    pub failure_stage: String,
    pub message: String,
    pub cwd: Option<String>,
    pub source: Option<String>,
    pub error_type: Option<String>,
    pub sub_error_type: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentTypeResult {
    pub item_type: AgentExternalAgentItemType,
    pub successes: Vec<AgentExternalAgentSuccess>,
    pub failures: Vec<AgentExternalAgentFailure>,
    pub extra: BTreeMap<String, Value>,
}

/// Progress and completion carry the same payload; only the method tells them
/// apart, and the client keeps that distinction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentImportStatus {
    pub generation: u64,
    pub import_id: String,
    pub completed: bool,
    pub item_type_results: Vec<AgentExternalAgentTypeResult>,
    pub extra: BTreeMap<String, Value>,
}

impl AgentExternalAgentImportStatus {
    pub fn successful_item_count(&self) -> usize {
        self.item_type_results
            .iter()
            .map(|result| result.successes.len())
            .sum()
    }

    pub fn failed_item_count(&self) -> usize {
        self.item_type_results
            .iter()
            .map(|result| result.failures.len())
            .sum()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentImportHistory {
    pub import_id: String,
    pub provider_id: Option<String>,
    pub completed_at_ms: i64,
    pub successes: Vec<AgentExternalAgentSuccess>,
    pub failures: Vec<AgentExternalAgentFailure>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentImportedConnector {
    pub name: String,
    pub session_count: i64,
    pub source: AgentExternalAgentImportedConnectorSource,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentImportHistories {
    pub generation: u64,
    pub histories: Vec<AgentExternalAgentImportHistory>,
    pub connectors: Vec<AgentExternalAgentImportedConnector>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentHistoryRecordRequest {
    pub provider_id: String,
    pub item_type_results: Vec<AgentExternalAgentTypeResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentExternalAgentConfigErrorKind {
    Unsupported,
    Protocol,
    Connection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExternalAgentConfigError {
    pub kind: AgentExternalAgentConfigErrorKind,
    pub message: String,
    pub data: Option<Value>,
    pub outcome_unknown: bool,
}

impl AgentExternalAgentConfigError {
    pub fn user_message(&self) -> String {
        match self.kind {
            AgentExternalAgentConfigErrorKind::Unsupported => {
                "当前 coding agent 不支持从其他应用导入".to_owned()
            }
            AgentExternalAgentConfigErrorKind::Connection => {
                "与 coding agent 的连接已断开".to_owned()
            }
            AgentExternalAgentConfigErrorKind::Protocol => "导入请求失败".to_owned(),
        }
    }
}
