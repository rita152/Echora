//! Plugin directory, install lifecycle, sharing, and marketplace operations.
//! Every structure keeps the server's unknown fields and keeps a missing field
//! apart from an explicit null.

use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

/// Marketplace kinds accepted by `plugin/list`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPluginMarketplaceKind {
    Local,
    Vertical,
    WorkspaceDirectory,
    SharedWithMe,
    CreatedByMeRemote,
}

impl AgentPluginMarketplaceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Vertical => "vertical",
            Self::WorkspaceDirectory => "workspace-directory",
            Self::SharedWithMe => "shared-with-me",
            Self::CreatedByMeRemote => "created-by-me-remote",
        }
    }
}

/// Where a marketplace entry's plugin comes from. The protocol models this as a
/// tagged union; an unrecognised tag is a protocol error rather than an entry
/// the client would have to render without provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentPluginSource {
    Local {
        path: PathBuf,
    },
    Git {
        url: String,
        ref_name: Option<String>,
        sha: Option<String>,
        path: Option<String>,
    },
    Npm {
        package: String,
        registry: Option<String>,
        version: Option<String>,
    },
    Remote,
}

impl AgentPluginSource {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Local { .. } => "local",
            Self::Git { .. } => "git",
            Self::Npm { .. } => "npm",
            Self::Remote => "remote",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPluginAuthPolicy {
    OnInstall,
    OnUse,
}

impl AgentPluginAuthPolicy {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "ON_INSTALL" => Some(Self::OnInstall),
            "ON_USE" => Some(Self::OnUse),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPluginInstallPolicy {
    NotAvailable,
    Available,
    InstalledByDefault,
}

impl AgentPluginInstallPolicy {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "NOT_AVAILABLE" => Some(Self::NotAvailable),
            "AVAILABLE" => Some(Self::Available),
            "INSTALLED_BY_DEFAULT" => Some(Self::InstalledByDefault),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPluginInstallPolicySource {
    WorkspaceSetting,
    ImplicitCanonicalApp,
}

impl AgentPluginInstallPolicySource {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "WORKSPACE_SETTING" => Some(Self::WorkspaceSetting),
            "IMPLICIT_CANONICAL_APP" => Some(Self::ImplicitCanonicalApp),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPluginDisabledReason {
    DisabledByAdmin,
    PlanNotEligible,
    RequiredAppUnavailable,
    Unknown,
}

impl AgentPluginDisabledReason {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "disabled_by_admin" => Some(Self::DisabledByAdmin),
            "plan_not_eligible" => Some(Self::PlanNotEligible),
            "required_app_unavailable" => Some(Self::RequiredAppUnavailable),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// The reason text the directory shows for a disabled entry.
    pub fn label(self) -> &'static str {
        match self {
            Self::DisabledByAdmin => "管理员已停用",
            Self::PlanNotEligible => "当前方案不可用",
            Self::RequiredAppUnavailable => "所需应用不可用",
            Self::Unknown => "暂不可用",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPluginAvailability {
    DisabledByAdmin,
    Available,
}

impl AgentPluginAvailability {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            // Upstream reports \"ENABLED\" for an available remote plugin; the
            // app-server API documents \"AVAILABLE\" as the same state.
            "DISABLED_BY_ADMIN" => Some(Self::DisabledByAdmin),
            "AVAILABLE" | "ENABLED" => Some(Self::Available),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPluginShareDiscoverability {
    Listed,
    Unlisted,
    Private,
}

impl AgentPluginShareDiscoverability {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "LISTED" => Some(Self::Listed),
            "UNLISTED" => Some(Self::Unlisted),
            "PRIVATE" => Some(Self::Private),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Listed => "LISTED",
            Self::Unlisted => "UNLISTED",
            Self::Private => "PRIVATE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPluginSharePrincipalType {
    User,
    Group,
    Workspace,
}

impl AgentPluginSharePrincipalType {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "user" => Some(Self::User),
            "group" => Some(Self::Group),
            "workspace" => Some(Self::Workspace),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Group => "group",
            Self::Workspace => "workspace",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPluginShareRole {
    Reader,
    Editor,
    Owner,
}

impl AgentPluginShareRole {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "reader" => Some(Self::Reader),
            "editor" => Some(Self::Editor),
            "owner" => Some(Self::Owner),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reader => "reader",
            Self::Editor => "editor",
            Self::Owner => "owner",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginSharePrincipal {
    pub name: String,
    pub principal_id: String,
    pub principal_type: AgentPluginSharePrincipalType,
    pub role: AgentPluginShareRole,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareTarget {
    pub principal_id: String,
    pub principal_type: AgentPluginSharePrincipalType,
    /// `plugin/share/updateTargets` accepts reader and editor only.
    pub role: AgentPluginShareRole,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareContext {
    pub remote_plugin_id: String,
    pub remote_version: Option<String>,
    pub creator_account_user_id: Option<String>,
    pub creator_name: Option<String>,
    pub can_publish_to_workspace: Option<bool>,
    pub discoverability: Option<AgentPluginShareDiscoverability>,
    pub share_principals: Option<Vec<AgentPluginSharePrincipal>>,
    pub share_url: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

/// Installed plugin package resources, as reported by the server. Paths and URLs
/// are both preserved: a remote catalog entry has URLs, a local package has
/// paths, and the client renders whichever the server actually sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginInterface {
    pub display_name: Option<String>,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub developer_name: Option<String>,
    pub category: Option<String>,
    pub brand_color: Option<String>,
    pub capabilities: Vec<String>,
    pub composer_icon: Option<PathBuf>,
    pub composer_icon_url: Option<String>,
    pub logo: Option<PathBuf>,
    pub logo_dark: Option<PathBuf>,
    pub logo_url: Option<String>,
    pub logo_url_dark: Option<String>,
    pub screenshots: Vec<PathBuf>,
    pub screenshot_urls: Vec<String>,
    pub website_url: Option<String>,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub default_prompt: Option<Vec<String>>,
    pub extra: BTreeMap<String, Value>,
}

impl AgentPluginInterface {
    /// Thumbnail source the directory prefers: local package asset first, remote
    /// URL second. `None` keeps the placeholder the view draws for a plugin the
    /// server shipped without artwork.
    pub fn logo_source(&self, dark: bool) -> Option<&str> {
        let local = if dark {
            self.logo_dark.as_ref().or(self.logo.as_ref())
        } else {
            self.logo.as_ref()
        };
        match local.and_then(|path| path.to_str()) {
            Some(path) => Some(path),
            None => {
                if dark {
                    self.logo_url_dark.as_deref().or(self.logo_url.as_deref())
                } else {
                    self.logo_url.as_deref()
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginSummary {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub local_version: Option<String>,
    pub enabled: bool,
    pub installed: bool,
    pub installed_at: Option<i64>,
    pub install_policy: AgentPluginInstallPolicy,
    pub install_policy_source: Option<AgentPluginInstallPolicySource>,
    pub auth_policy: AgentPluginAuthPolicy,
    pub availability: Option<AgentPluginAvailability>,
    pub disabled_reason: Option<AgentPluginDisabledReason>,
    pub eligible_plan_types: Option<Vec<String>>,
    pub interface: Option<AgentPluginInterface>,
    pub keywords: Option<Vec<String>>,
    pub must_show_installation_interstitial: Option<bool>,
    pub remote_plugin_id: Option<String>,
    pub share_context: Option<AgentPluginShareContext>,
    pub source: AgentPluginSource,
    pub extra: BTreeMap<String, Value>,
}

impl AgentPluginSummary {
    /// Name shown in the directory: the interface label when the package
    /// provides one, otherwise the configured plugin name.
    pub fn display_name(&self) -> &str {
        self.interface
            .as_ref()
            .and_then(|interface| interface.display_name.as_deref())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.name)
    }

    pub fn description(&self) -> Option<&str> {
        self.interface
            .as_ref()
            .and_then(|interface| interface.short_description.as_deref())
    }

    /// Whether the server would accept an install for this entry. The client
    /// derives nothing beyond the two fields the server itself reports.
    pub fn installable(&self) -> bool {
        !self.installed && self.install_policy != AgentPluginInstallPolicy::NotAvailable
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceInterface {
    pub display_name: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginMarketplace {
    pub name: String,
    pub path: Option<String>,
    pub interface: Option<AgentMarketplaceInterface>,
    pub plugins: Vec<AgentPluginSummary>,
    pub extra: BTreeMap<String, Value>,
}

impl AgentPluginMarketplace {
    pub fn display_name(&self) -> &str {
        self.interface
            .as_ref()
            .and_then(|interface| interface.display_name.as_deref())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceLoadError {
    pub marketplace_path: PathBuf,
    pub message: String,
    pub extra: BTreeMap<String, Value>,
}

/// The shape both `plugin/list` and `plugin/installed` return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginCatalog {
    pub generation: u64,
    pub marketplaces: Vec<AgentPluginMarketplace>,
    pub featured_plugin_ids: Vec<String>,
    pub marketplace_load_errors: Vec<AgentMarketplaceLoadError>,
    pub extra: BTreeMap<String, Value>,
}

impl AgentPluginCatalog {
    /// Every plugin in every marketplace, in server order.
    pub fn plugins(&self) -> impl Iterator<Item = (&AgentPluginMarketplace, &AgentPluginSummary)> {
        self.marketplaces.iter().flat_map(|marketplace| {
            marketplace
                .plugins
                .iter()
                .map(move |plugin| (marketplace, plugin))
        })
    }

    pub fn installed_plugin_count(&self) -> usize {
        self.plugins()
            .filter(|(_, plugin)| plugin.installed)
            .count()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginAppSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub install_url: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAppTemplateUnavailableReason {
    NotConfiguredForWorkspace,
    NoActiveWorkspace,
}

impl AgentAppTemplateUnavailableReason {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "NOT_CONFIGURED_FOR_WORKSPACE" => Some(Self::NotConfiguredForWorkspace),
            "NO_ACTIVE_WORKSPACE" => Some(Self::NoActiveWorkspace),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginAppTemplateSummary {
    pub template_id: String,
    pub name: String,
    pub description: Option<String>,
    pub canonical_connector_id: Option<String>,
    pub category: Option<String>,
    pub logo_url: Option<String>,
    pub logo_url_dark: Option<String>,
    pub materialized_app_ids: Vec<String>,
    pub reason: Option<AgentAppTemplateUnavailableReason>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginHookSummary {
    pub key: String,
    pub event_name: String,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginSkillSummary {
    pub name: String,
    pub description: String,
    pub short_description: Option<String>,
    pub enabled: bool,
    pub path: Option<String>,
    pub interface: Option<Value>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginDetail {
    pub summary: AgentPluginSummary,
    pub description: Option<String>,
    pub marketplace_name: String,
    pub marketplace_path: Option<String>,
    pub apps: Vec<AgentPluginAppSummary>,
    pub app_templates: Vec<AgentPluginAppTemplateSummary>,
    pub hooks: Vec<AgentPluginHookSummary>,
    pub mcp_servers: Vec<String>,
    pub scheduled_tasks: Option<Value>,
    pub share_url: Option<String>,
    pub skills: Vec<AgentPluginSkillSummary>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginSearchResult {
    pub marketplace_name: String,
    pub marketplace_path: Option<String>,
    pub plugin: AgentPluginSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginSearchPage {
    pub generation: u64,
    pub cursor: Option<String>,
    pub search_term: String,
    pub results: Vec<AgentPluginSearchResult>,
    pub next_cursor: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

/// How an operation that changes server state ended. A timeout and an
/// unconfirmed answer are distinct from a failure: neither may be retried
/// automatically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentPluginOperationOutcome<T> {
    Succeeded(T),
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

impl<T> AgentPluginOperationOutcome<T> {
    /// True when the connection can no longer be trusted to answer.
    pub fn outcome_unknown(&self) -> bool {
        matches!(self, Self::TimedOut { .. } | Self::Unknown { .. })
    }

    pub fn succeeded(&self) -> Option<&T> {
        match self {
            Self::Succeeded(value) => Some(value),
            _ => None,
        }
    }

    pub fn user_message(&self, success: &str) -> String {
        match self {
            Self::Succeeded(_) => success.to_owned(),
            Self::Failed { message, .. } => message.clone(),
            Self::TimedOut { .. } => "操作超时，结果未确认；请重新读取后再试".to_owned(),
            Self::Unknown { .. } => "操作结果未知；请重新读取目录确认当前状态".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginInstallReceipt {
    pub auth_policy: AgentPluginAuthPolicy,
    pub apps_needing_auth: Vec<AgentPluginAppSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginInstallRequest {
    pub generation: u64,
    pub plugin_name: String,
    pub marketplace_path: Option<String>,
    pub remote_marketplace_name: Option<String>,
    /// Client-generated attempt id echoed by the server for this attempt only.
    pub install_attempt_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginInstallResult {
    pub generation: u64,
    pub plugin_name: String,
    pub outcome: AgentPluginOperationOutcome<AgentPluginInstallReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginUninstallRequest {
    pub generation: u64,
    pub plugin_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginUninstallResult {
    pub generation: u64,
    pub plugin_id: String,
    pub outcome: AgentPluginOperationOutcome<()>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginReconcileRequest {
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginReconcileChangedPlugin {
    pub id: String,
    pub has_apps: bool,
    pub has_hooks: bool,
    pub has_mcps: bool,
    pub has_skills: bool,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginReconcileReceipt {
    pub generation: u64,
    pub changed_plugins: Vec<AgentPluginReconcileChangedPlugin>,
    pub failed_remote_plugin_ids: Vec<String>,
    pub failed_materialization_remote_plugin_ids: Vec<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceAddRequest {
    pub generation: u64,
    pub source: String,
    pub ref_name: Option<String>,
    pub sparse_paths: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceAddReceipt {
    pub marketplace_name: String,
    pub installed_root: PathBuf,
    pub already_added: bool,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceAddResult {
    pub generation: u64,
    pub source: String,
    pub outcome: AgentPluginOperationOutcome<AgentMarketplaceAddReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceRemoveRequest {
    pub generation: u64,
    pub marketplace_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceRemoveReceipt {
    pub marketplace_name: String,
    pub installed_root: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceRemoveResult {
    pub generation: u64,
    pub marketplace_name: String,
    pub outcome: AgentPluginOperationOutcome<AgentMarketplaceRemoveReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceUpgradeRequest {
    pub generation: u64,
    /// `None` upgrades every marketplace the server selects.
    pub marketplace_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceUpgradeError {
    pub marketplace_name: String,
    pub message: String,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceUpgradeReceipt {
    pub selected_marketplaces: Vec<String>,
    pub upgraded_roots: Vec<PathBuf>,
    pub errors: Vec<AgentMarketplaceUpgradeError>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMarketplaceUpgradeResult {
    pub generation: u64,
    pub marketplace_name: Option<String>,
    pub outcome: AgentPluginOperationOutcome<AgentMarketplaceUpgradeReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareSaveRequest {
    pub generation: u64,
    pub plugin_path: PathBuf,
    pub remote_plugin_id: Option<String>,
    pub discoverability: Option<AgentPluginShareDiscoverability>,
    pub share_targets: Option<Vec<AgentPluginShareTarget>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareSaveReceipt {
    pub remote_plugin_id: String,
    pub share_url: String,
    pub can_publish_to_workspace: Option<bool>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareSaveResult {
    pub generation: u64,
    pub plugin_path: PathBuf,
    pub outcome: AgentPluginOperationOutcome<AgentPluginShareSaveReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareUpdateTargetsRequest {
    pub generation: u64,
    pub remote_plugin_id: String,
    pub discoverability: AgentPluginShareDiscoverability,
    pub share_targets: Vec<AgentPluginShareTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareUpdateTargetsReceipt {
    pub discoverability: AgentPluginShareDiscoverability,
    pub principals: Vec<AgentPluginSharePrincipal>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareUpdateTargetsResult {
    pub generation: u64,
    pub remote_plugin_id: String,
    pub outcome: AgentPluginOperationOutcome<AgentPluginShareUpdateTargetsReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareListEntry {
    pub plugin: AgentPluginSummary,
    pub local_plugin_path: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareList {
    pub generation: u64,
    pub entries: Vec<AgentPluginShareListEntry>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareDeleteRequest {
    pub generation: u64,
    pub remote_plugin_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginShareDeleteResult {
    pub generation: u64,
    pub remote_plugin_id: String,
    pub outcome: AgentPluginOperationOutcome<()>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginSkillReadRequest {
    pub remote_marketplace_name: String,
    pub remote_plugin_id: String,
    pub skill_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginSkillContent {
    pub generation: u64,
    /// `None` when the server answered with `null` contents, which is not the
    /// same as an empty skill body.
    pub contents: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginReadRequest {
    pub marketplace_path: Option<String>,
    pub remote_marketplace_name: Option<String>,
    pub plugin_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginSearchRequest {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    /// One of the schema's scope values; this client searches every
    /// marketplace, which the protocol calls `global`.
    pub scope: Option<&'static str>,
    pub search_term: String,
    pub cwds: Option<Vec<PathBuf>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AgentPluginCatalogRequest {
    /// Working directories used to discover repository marketplaces. `None`
    /// keeps the server's home-scoped default set.
    pub cwds: Option<Vec<PathBuf>>,
    pub force_refetch: bool,
    pub marketplace_kinds: Option<Vec<AgentPluginMarketplaceKind>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AgentPluginInstalledRequest {
    pub cwds: Option<Vec<PathBuf>>,
    pub install_suggestion_plugin_names: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentPluginsErrorKind {
    Unsupported,
    Protocol,
    Connection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPluginsError {
    pub kind: AgentPluginsErrorKind,
    pub message: String,
    pub data: Option<Value>,
    pub outcome_unknown: bool,
}

impl AgentPluginsError {
    pub fn user_message(&self) -> String {
        match self.kind {
            AgentPluginsErrorKind::Unsupported => "当前 coding agent 不支持插件管理".to_owned(),
            AgentPluginsErrorKind::Connection => "与 coding agent 的连接已断开".to_owned(),
            AgentPluginsErrorKind::Protocol => "插件请求失败".to_owned(),
        }
    }
}
