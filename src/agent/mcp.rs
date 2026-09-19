//! MCP server inventory, startup state, reload outcomes, and OAuth logins.
//! Unknown JSON fields are retained at every level so server extensions and
//! newer CLI versions survive a round trip through this client.

use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

use super::AgentMcpServerStartupStatus;

/// A startup observation stamped with the connection generation that saw it.
/// Application state is layered as generation, then thread (or the application
/// scope when `thread_id` is absent), then server name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpStartupStatusUpdated {
    pub generation: u64,
    pub status: AgentMcpServerStartupStatus,
}

/// How much inventory the server should fetch for each entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpStatusDetail {
    Full,
    ToolsAndAuthOnly,
}

impl AgentMcpStatusDetail {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::ToolsAndAuthOnly => "toolsAndAuthOnly",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpServerStatusRequest {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    pub detail: Option<AgentMcpStatusDetail>,
    /// When present the server reports the connection state of that thread.
    pub thread_id: Option<String>,
}

impl Default for AgentMcpServerStatusRequest {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: None,
            detail: Some(AgentMcpStatusDetail::Full),
            thread_id: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpAuthStatus {
    Unknown,
    Unsupported,
    NotLoggedIn,
    BearerToken,
    OAuth,
}

impl AgentMcpAuthStatus {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "unknown" => Some(Self::Unknown),
            "unsupported" => Some(Self::Unsupported),
            "notLoggedIn" => Some(Self::NotLoggedIn),
            "bearerToken" => Some(Self::BearerToken),
            "oAuth" => Some(Self::OAuth),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => crate::i18n::text("认证状态未知"),
            Self::Unsupported => crate::i18n::text("无需认证"),
            Self::NotLoggedIn => crate::i18n::text("尚未登录"),
            Self::BearerToken => crate::i18n::text("已配置令牌"),
            Self::OAuth => crate::i18n::text("已登录"),
        }
    }

    /// Only servers that reported an OAuth capable, signed-out state can start
    /// an interactive login.
    pub fn can_start_login(self) -> bool {
        matches!(self, Self::NotLoggedIn | Self::Unknown)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpServerConnectionStatus {
    NotStarted,
    Starting,
    Connected,
    AuthenticationRequired,
    Failed,
    Cancelled,
    Disabled,
}

impl AgentMcpServerConnectionStatus {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "notStarted" => Some(Self::NotStarted),
            "starting" => Some(Self::Starting),
            "connected" => Some(Self::Connected),
            "authenticationRequired" => Some(Self::AuthenticationRequired),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NotStarted => crate::i18n::text("未连接"),
            Self::Starting => crate::i18n::text("连接中"),
            Self::Connected => crate::i18n::text("已连接"),
            Self::AuthenticationRequired => crate::i18n::text("需要登录"),
            Self::Failed => crate::i18n::text("连接失败"),
            Self::Cancelled => crate::i18n::text("已取消"),
            Self::Disabled => crate::i18n::text("已停用"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpServerInfo {
    pub name: String,
    pub version: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub website_url: Option<String>,
    /// Icon payloads are untyped in the schema and are kept verbatim.
    pub icons: Option<Value>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpTool {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
    pub annotations: Option<Value>,
    pub icons: Option<Value>,
    pub meta: Option<Value>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpResource {
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    pub annotations: Option<Value>,
    pub meta: Option<Value>,
    pub icons: Option<Value>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpResourceTemplate {
    pub uri_template: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub annotations: Option<Value>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpServerStatus {
    pub name: String,
    pub plugin_id: Option<String>,
    pub auth_status: AgentMcpAuthStatus,
    /// `null` when the runtime state is unavailable or configuration changed.
    pub runtime_status: Option<AgentMcpServerConnectionStatus>,
    pub server_info: Option<AgentMcpServerInfo>,
    pub tools: Vec<AgentMcpTool>,
    pub resources: Vec<AgentMcpResource>,
    pub resource_templates: Vec<AgentMcpResourceTemplate>,
    /// Set when tool discovery failed and no catalog was returned.
    pub tools_error: Option<String>,
    /// Reserved for newer CLI revisions that add trailing fields.
    pub extra: BTreeMap<String, Value>,
}

impl AgentMcpServerStatus {
    /// Prefers the server's advertised title, then the configuration name.
    pub fn display_name(&self) -> String {
        self.server_info
            .as_ref()
            .and_then(|info| info.title.clone())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| self.name.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpServerPage {
    pub generation: u64,
    pub cursor: Option<String>,
    pub servers: Vec<AgentMcpServerStatus>,
    pub next_cursor: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentMcpOauthClientRegistration {
    #[default]
    Auto,
    Cimd,
    Dcr,
}

impl AgentMcpOauthClientRegistration {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "auto" => Some(Self::Auto),
            "cimd" => Some(Self::Cimd),
            "dcr" => Some(Self::Dcr),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cimd => "cimd",
            Self::Dcr => "dcr",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpOauthLoginRequest {
    pub generation: u64,
    pub server_name: String,
    /// `None` performs an application scoped login; `Some` binds it to a thread.
    pub thread_id: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub client_registration: Option<AgentMcpOauthClientRegistration>,
    pub timeout_secs: Option<i64>,
}

/// A login the client started. `login_id` is client-generated because the
/// protocol carries no server-side login identifier; it is what makes a late
/// completion notification distinguishable from a newer login for the same
/// server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpOauthLogin {
    pub login_id: u64,
    pub generation: u64,
    pub server_name: String,
    pub thread_id: Option<String>,
    pub authorization_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentMcpOauthCompletionStatus {
    Succeeded,
    Failed(String),
    /// The client cancelled; kept so a late completion can be ignored.
    Cancelled,
    /// The connection was lost or replaced before the login finished.
    Interrupted(String),
}

impl AgentMcpOauthCompletionStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpOauthCompletion {
    pub login_id: u64,
    pub generation: u64,
    pub server_name: String,
    pub thread_id: Option<String>,
    pub status: AgentMcpOauthCompletionStatus,
    pub extra: BTreeMap<String, Value>,
}

/// Distinguishes a confirmed reload from a request that never produced an
/// answer we can trust.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentMcpReloadOutcome {
    Reloaded,
    Failed {
        message: String,
        data: Option<Value>,
    },
    TimedOut {
        message: String,
    },
    Unknown {
        message: String,
    },
}

impl AgentMcpReloadOutcome {
    /// True when the connection can no longer be trusted to answer.
    pub fn outcome_unknown(&self) -> bool {
        matches!(self, Self::TimedOut { .. } | Self::Unknown { .. })
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Reloaded => crate::i18n::text("已重新加载 MCP 服务器").to_owned(),
            Self::Failed { message, .. } => message.clone(),
            Self::TimedOut { .. } => {
                crate::i18n::text("重新加载超时，结果未确认；连接已重置，请重试").to_owned()
            }
            Self::Unknown { .. } => {
                crate::i18n::text("重新加载结果未知；请重新读取服务器列表").to_owned()
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpReloadRequest {
    /// The working directory the reload was issued for; a reload never applies
    /// to a different project than the one the user acted in.
    pub cwd: PathBuf,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpReloadResult {
    pub generation: u64,
    pub cwd: PathBuf,
    pub outcome: AgentMcpReloadOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentMcpErrorKind {
    Unsupported,
    Protocol,
    Connection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpError {
    pub kind: AgentMcpErrorKind,
    pub message: String,
    pub data: Option<Value>,
    pub outcome_unknown: bool,
}

impl AgentMcpError {
    pub fn user_message(&self) -> String {
        match self.kind {
            AgentMcpErrorKind::Unsupported => {
                crate::i18n::text("当前 coding agent 不支持 MCP 管理").to_owned()
            }
            AgentMcpErrorKind::Connection => {
                crate::i18n::text("与 coding agent 的连接已断开").to_owned()
            }
            AgentMcpErrorKind::Protocol => crate::i18n::text("MCP 请求失败").to_owned(),
        }
    }
}
