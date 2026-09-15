//! App (connector) directory, installed runtime snapshot, and metadata reads.
//! Unknown JSON fields are retained at every level so server extensions and
//! newer CLI revisions survive a round trip through this client.

use serde_json::Value;
use std::collections::BTreeMap;

/// One entry of `app/list`. Field names mirror the protocol; a missing field and
/// an explicit null are kept apart everywhere the schema distinguishes them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub distribution_channel: Option<String>,
    pub install_url: Option<String>,
    pub logo_url: Option<String>,
    pub logo_url_dark: Option<String>,
    /// Local icon assets keyed by the server's own size names.
    pub icon_assets: Option<BTreeMap<String, String>>,
    pub icon_dark_assets: Option<BTreeMap<String, String>>,
    /// Localized labels keyed by locale.
    pub labels: Option<BTreeMap<String, String>>,
    pub plugin_display_names: Vec<String>,
    pub is_accessible: bool,
    pub is_enabled: bool,
    pub branding: Option<AgentAppBranding>,
    pub metadata: Option<AgentAppMetadata>,
    pub extra: BTreeMap<String, Value>,
}

impl AgentAppInfo {
    /// The name the directory shows: the server's own `name` field. The client
    /// never derives a display name from an id or a URL.
    pub fn display_name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppBranding {
    pub category: Option<String>,
    pub developer: Option<String>,
    pub is_discoverable_app: bool,
    pub privacy_policy: Option<String>,
    pub terms_of_service: Option<String>,
    pub website: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppReview {
    pub status: String,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppScreenshot {
    pub file_id: Option<String>,
    pub url: Option<String>,
    pub user_prompt: String,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppMetadata {
    pub categories: Option<Vec<String>>,
    pub developer: Option<String>,
    pub first_party_requires_install: Option<bool>,
    pub review: Option<AgentAppReview>,
    pub screenshots: Option<Vec<AgentAppScreenshot>>,
    pub seo_description: Option<String>,
    pub show_in_composer_when_unlinked: Option<bool>,
    pub sub_categories: Option<Vec<String>>,
    pub version: Option<String>,
    pub version_id: Option<String>,
    pub version_notes: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppsPage {
    pub generation: u64,
    pub cursor: Option<String>,
    pub apps: Vec<AgentAppInfo>,
    pub next_cursor: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

/// `app/installed` entry: the effective state of a connector in the committed
/// runtime snapshot. This is not a duplicate of the directory entry, and the two
/// are never folded into one record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentInstalledApp {
    pub id: String,
    /// Effective enabled state after global, workspace, local and managed config.
    pub enabled: bool,
    pub callable: bool,
    pub runtime_name: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentInstalledApps {
    pub generation: u64,
    pub apps: Vec<AgentInstalledApp>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppToolSummary {
    pub name: String,
    pub title: Option<String>,
    pub description: String,
    pub disabled_reason: Option<String>,
    pub is_enabled: bool,
    pub is_read_only: bool,
    pub extra: BTreeMap<String, Value>,
}

/// `app/read` entry. `tool_summaries` stays `None` when the server omitted the
/// field and `Some` when it answered with an array, including an empty one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppMetadataEntry {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub distribution_channel: Option<String>,
    pub icon_url: Option<String>,
    pub icon_url_dark: Option<String>,
    pub install_url: Option<String>,
    pub plugin_display_names: Vec<String>,
    pub tool_summaries: Option<Vec<AgentAppToolSummary>>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppsReadResult {
    pub generation: u64,
    pub apps: Vec<AgentAppMetadataEntry>,
    /// Ids the server could not resolve; rendered as reported, never as an
    /// empty entry.
    pub missing_app_ids: Vec<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAppsListRequest {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    pub force_refetch: bool,
    pub thread_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AgentAppsInstalledRequest {
    pub force_refresh: bool,
    pub thread_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppsReadRequest {
    pub app_ids: Vec<String>,
    pub include_tools: bool,
    pub thread_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentAppsErrorKind {
    Unsupported,
    Protocol,
    Connection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAppsError {
    pub kind: AgentAppsErrorKind,
    pub message: String,
    pub data: Option<Value>,
    pub outcome_unknown: bool,
}

impl AgentAppsError {
    pub fn user_message(&self) -> String {
        match self.kind {
            AgentAppsErrorKind::Unsupported => "当前 coding agent 不支持应用目录".to_owned(),
            AgentAppsErrorKind::Connection => "与 coding agent 的连接已断开".to_owned(),
            AgentAppsErrorKind::Protocol => "应用目录请求失败".to_owned(),
        }
    }
}
