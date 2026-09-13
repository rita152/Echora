//! Application-facing backend contract, capabilities, errors, and run ownership.

use std::{collections::BTreeSet, fmt, path::PathBuf, sync::Arc};

use async_channel::Receiver;

use super::{
    account::{
        AgentAccountSnapshot, AgentLoginCancelOutcome, AgentLoginStart, AgentLogoutOutcome,
        AgentRateLimitsRead,
    },
    catalog::{AgentModelCatalog, AgentPermissionMode, AgentPermissionProfile},
    events::{AgentConnectionEvent, AgentEvent},
    thread::{
        CreateProject, HistoryItemDetail, Page, PageRequest, Project, ProjectId,
        ThreadHistoryItemEntry, ThreadId, ThreadListRequest, ThreadMetadataUpdate,
        ThreadSearchResult, ThreadSection, ThreadSectionAppearance, ThreadSectionId, ThreadSummary,
        ThreadTurn, UpdateProject,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgentCapability {
    AccountRead,
    AccountRateLimits,
    AccountLogin,
    AccountLogout,
    ProjectList,
    ProjectCreate,
    ProjectUpdate,
    ProjectDelete,
    ProjectMove,
    ThreadList,
    ThreadSearch,
    ThreadRead,
    ThreadTurnsList,
    ThreadItemsList,
    ThreadRename,
    ThreadArchive,
    ThreadUnarchive,
    ThreadDelete,
    ThreadMetadataUpdate,
    ThreadSectionList,
    ThreadSectionCreate,
    ThreadSectionMove,
    SideConversation,
    SkillsList,
    SkillConfigWrite,
    McpServerStatusList,
    McpServerReload,
    McpOauthLogin,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentCapabilities {
    supported: BTreeSet<AgentCapability>,
}

impl AgentCapabilities {
    pub fn new(capabilities: impl IntoIterator<Item = AgentCapability>) -> Self {
        Self {
            supported: capabilities.into_iter().collect(),
        }
    }

    pub fn supports(&self, capability: AgentCapability) -> bool {
        self.supported.contains(&capability)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsupported {
    pub capability: AgentCapability,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    Unsupported(Unsupported),
    Backend(String),
}

impl WorkspaceError {
    pub fn backend(message: impl Into<String>) -> Self {
        Self::Backend(message.into())
    }

    /// Produces an agent-neutral message suitable for product UI. Concrete
    /// protocol and transport details remain available in the error value for
    /// diagnostics, but must not cross into views.
    pub fn user_message(&self, action: &str) -> String {
        match self {
            Self::Unsupported(_) => format!("当前 coding agent 不支持{action}"),
            Self::Backend(_) => format!("{action}失败，请重试"),
        }
    }

    fn unsupported(capability: AgentCapability) -> Self {
        Self::Unsupported(Unsupported {
            capability,
            message: format!("当前 coding agent 不支持 {capability:?}"),
        })
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(unsupported) => formatter.write_str(&unsupported.message),
            Self::Backend(message) => formatter.write_str(message),
        }
    }
}

pub type WorkspaceResult<T> = Result<T, WorkspaceError>;

/// Agent-neutral input consumed by every coding-agent adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRequest {
    pub client_message_id: Option<String>,
    pub prompt: String,
    pub cwd: PathBuf,
    pub project_id: Option<ProjectId>,
    pub thread_id: Option<String>,
    pub model: String,
    pub effort: String,
    pub service_tier: Option<String>,
    pub permission_mode: AgentPermissionMode,
    pub context: AgentPromptContext,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentPromptContext {
    pub files: Vec<AgentInputFile>,
    /// None preserves the current conversation mode; false explicitly exits planning.
    pub plan_mode: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentInputFile {
    pub path: PathBuf,
    pub image: bool,
}

/// Identity of an accepted turn within exactly one connection generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTurnIdentity {
    pub generation: u64,
    pub thread_id: String,
    pub turn_id: String,
}

/// Appending input cannot override the running turn's configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSteerRequest {
    pub target: AgentTurnIdentity,
    pub client_message_id: String,
    pub prompt: String,
    pub context: AgentPromptContext,
}

/// An independent, temporary conversation using a parent's history as context.
/// Opening one must not submit a turn or change the parent conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideConversationRequest {
    pub parent_thread_id: ThreadId,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub service_tier: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentInterruptOutcome {
    Requested,
    AlreadyRequested,
    AlreadyFinished,
}

pub(crate) trait AgentInterruptControl: Send + Sync {
    fn request_interrupt(&self) -> Result<AgentInterruptOutcome, String>;
    fn abandon(&self);
}

pub struct AgentInterruptHandle {
    control: Arc<dyn AgentInterruptControl>,
}

impl AgentInterruptHandle {
    pub(crate) fn new(control: Arc<dyn AgentInterruptControl>) -> Self {
        Self { control }
    }

    pub fn interrupt(&self) -> Result<AgentInterruptOutcome, String> {
        self.control.request_interrupt()
    }
}

impl Drop for AgentInterruptHandle {
    fn drop(&mut self) {
        self.control.abandon();
    }
}

pub struct AgentRun {
    events: Receiver<AgentEvent>,
    interrupt: Option<AgentInterruptHandle>,
}

impl AgentRun {
    pub(crate) fn new(
        events: Receiver<AgentEvent>,
        interrupt: Option<AgentInterruptHandle>,
    ) -> Self {
        Self { events, interrupt }
    }

    pub fn into_parts(self) -> (Receiver<AgentEvent>, Option<AgentInterruptHandle>) {
        (self.events, self.interrupt)
    }
}

/// Boundary between the application and a concrete coding-agent protocol.
pub trait AgentBackend: Send + Sync {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::default()
    }

    fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent>;
    fn open_side_conversation(
        &self,
        _request: SideConversationRequest,
    ) -> Receiver<WorkspaceResult<ThreadId>> {
        unsupported_receiver(AgentCapability::SideConversation)
    }
    fn close_side_conversation(&self, _thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::SideConversation)
    }
    #[cfg_attr(test, allow(dead_code))]
    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>>;
    fn load_permission_profiles(
        &self,
        cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>>;
    fn update_thread_permissions(
        &self,
        request: super::AgentThreadPermissionUpdate,
    ) -> Receiver<Result<super::AgentThreadPermissionResult, String>>;

    fn load_thread_settings(
        &self,
        _thread_id: String,
        _generation: u64,
    ) -> Receiver<Result<super::AgentThreadSettingsSnapshot, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(Err("此后端不支持读取线程设置".into()));
        receiver
    }

    fn config_choices(&self) -> Vec<super::AgentConfigChoiceSet> {
        Vec::new()
    }

    /// Reads the account answer for the current connection. A missing or null
    /// account stays distinguishable, and the answer is also reduced into the
    /// connection snapshot other views observe.
    fn read_account(&self) -> Receiver<Result<AgentAccountSnapshot, String>> {
        unsupported_account_receiver("读取账户状态")
    }

    /// Reads the current quota snapshot for the connected account.
    fn read_rate_limits(&self) -> Receiver<Result<AgentRateLimitsRead, String>> {
        unsupported_account_receiver("读取配额")
    }

    /// Starts the Codex-managed ChatGPT login and returns the login id with the
    /// challenge the user must complete in a browser.
    fn start_chatgpt_login(&self) -> Receiver<Result<AgentLoginStart, String>> {
        unsupported_account_receiver("登录 ChatGPT 账户")
    }

    /// Cancels exactly the login named by this identifier.
    fn cancel_login(&self, _login_id: String) -> Receiver<Result<AgentLoginCancelOutcome, String>> {
        unsupported_account_receiver("取消登录")
    }

    /// Signs out and confirms the resulting account state.
    fn logout_account(&self) -> Receiver<Result<AgentLogoutOutcome, String>> {
        unsupported_account_receiver("退出登录")
    }

    /// Reads the skills inventory. `skills/changed` invalidates the caller's
    /// cache; this call is the only source of skill data.
    fn load_skills(
        &self,
        _request: super::AgentSkillsLoadRequest,
    ) -> Receiver<Result<super::AgentSkillsSnapshot, super::AgentSkillsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(Err(super::AgentSkillsError {
            kind: super::AgentSkillsErrorKind::Unsupported,
            message: "当前 coding agent 不支持技能管理".into(),
            data: None,
            outcome_unknown: false,
        }));
        receiver
    }

    /// Persists one skill's enabled flag. The returned receipt is the server's
    /// effective value; the client never predicts it.
    fn write_skill_config(
        &self,
        _request: super::AgentSkillWriteRequest,
    ) -> Receiver<Result<super::AgentSkillWriteReceipt, super::AgentSkillsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(Err(super::AgentSkillsError {
            kind: super::AgentSkillsErrorKind::Unsupported,
            message: "当前 coding agent 不支持技能管理".into(),
            data: None,
            outcome_unknown: false,
        }));
        receiver
    }

    fn list_mcp_servers(
        &self,
        _request: super::AgentMcpServerStatusRequest,
    ) -> Receiver<Result<super::AgentMcpServerPage, super::AgentMcpError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(Err(super::AgentMcpError {
            kind: super::AgentMcpErrorKind::Unsupported,
            message: "当前 coding agent 不支持 MCP 管理".into(),
            data: None,
            outcome_unknown: false,
        }));
        receiver
    }

    /// Reloads MCP server configuration. Always produces a result, including
    /// for a timeout or an unconfirmed outcome.
    fn reload_mcp_servers(
        &self,
        request: super::AgentMcpReloadRequest,
    ) -> Receiver<super::AgentMcpReloadResult> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(super::AgentMcpReloadResult {
            generation: request.generation,
            cwd: request.cwd,
            outcome: super::AgentMcpReloadOutcome::Failed {
                message: "当前 coding agent 不支持 MCP 管理".into(),
                data: None,
            },
        });
        receiver
    }

    /// Starts an interactive OAuth login. Cancellation is client-side: the
    /// protocol defines no cancel request, so the caller invalidates the login
    /// and ignores any later completion for it.
    fn start_mcp_oauth_login(
        &self,
        _request: super::AgentMcpOauthLoginRequest,
    ) -> Receiver<Result<super::AgentMcpOauthLogin, super::AgentMcpError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(Err(super::AgentMcpError {
            kind: super::AgentMcpErrorKind::Unsupported,
            message: "当前 coding agent 不支持 MCP 登录".into(),
            data: None,
            outcome_unknown: false,
        }));
        receiver
    }

    fn cancel_mcp_oauth_login(&self, _login_id: u64) -> Receiver<Result<(), super::AgentMcpError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(Ok(()));
        receiver
    }
    fn read_config(
        &self,
        _cwd: PathBuf,
    ) -> Receiver<Result<super::AgentConfigSnapshot, super::AgentConfigError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(Err(super::AgentConfigError::unavailable()));
        receiver
    }
    fn write_config(
        &self,
        _write: super::AgentConfigWrite,
    ) -> Receiver<Result<super::AgentConfigSaveResult, super::AgentConfigError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(Err(super::AgentConfigError::unavailable()));
        receiver
    }

    fn list_projects(&self, _page: PageRequest) -> Receiver<WorkspaceResult<Page<Project>>> {
        unsupported_receiver(AgentCapability::ProjectList)
    }

    fn create_project(&self, _project: CreateProject) -> Receiver<WorkspaceResult<Project>> {
        unsupported_receiver(AgentCapability::ProjectCreate)
    }

    fn update_project(
        &self,
        _project_id: ProjectId,
        _update: UpdateProject,
    ) -> Receiver<WorkspaceResult<Project>> {
        unsupported_receiver(AgentCapability::ProjectUpdate)
    }

    fn delete_project(&self, _project_id: ProjectId) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ProjectDelete)
    }

    fn move_project(
        &self,
        _project_id: ProjectId,
        _before_project_id: Option<ProjectId>,
    ) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ProjectMove)
    }

    fn list_threads(
        &self,
        _request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
        unsupported_receiver(AgentCapability::ThreadList)
    }

    fn search_threads(
        &self,
        _request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSearchResult>>> {
        unsupported_receiver(AgentCapability::ThreadSearch)
    }

    fn read_thread(&self, _thread_id: ThreadId) -> Receiver<WorkspaceResult<ThreadSummary>> {
        unsupported_receiver(AgentCapability::ThreadRead)
    }

    fn list_thread_turns(
        &self,
        _thread_id: ThreadId,
        _page: PageRequest,
        _detail: HistoryItemDetail,
    ) -> Receiver<WorkspaceResult<Page<ThreadTurn>>> {
        unsupported_receiver(AgentCapability::ThreadTurnsList)
    }

    fn list_thread_items(
        &self,
        _thread_id: ThreadId,
        _turn_id: Option<String>,
        _page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadHistoryItemEntry>>> {
        unsupported_receiver(AgentCapability::ThreadItemsList)
    }

    fn set_thread_name(
        &self,
        _thread_id: ThreadId,
        _name: String,
    ) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ThreadRename)
    }

    fn archive_thread(&self, _thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ThreadArchive)
    }

    fn unarchive_thread(&self, _thread_id: ThreadId) -> Receiver<WorkspaceResult<ThreadSummary>> {
        unsupported_receiver(AgentCapability::ThreadUnarchive)
    }

    fn delete_thread(&self, _thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ThreadDelete)
    }

    fn update_thread_metadata(
        &self,
        _thread_id: ThreadId,
        _update: ThreadMetadataUpdate,
    ) -> Receiver<WorkspaceResult<ThreadSummary>> {
        unsupported_receiver(AgentCapability::ThreadMetadataUpdate)
    }

    fn list_thread_sections(
        &self,
        _page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSection>>> {
        unsupported_receiver(AgentCapability::ThreadSectionList)
    }

    fn create_thread_section(
        &self,
        _name: String,
        _appearance: Option<ThreadSectionAppearance>,
    ) -> Receiver<WorkspaceResult<ThreadSection>> {
        unsupported_receiver(AgentCapability::ThreadSectionCreate)
    }

    fn move_thread_to_section(
        &self,
        _thread_id: ThreadId,
        _section_id: Option<ThreadSectionId>,
        _before_thread_id: Option<ThreadId>,
    ) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ThreadSectionMove)
    }

    fn steer_turn(&self, _request: AgentSteerRequest) -> Receiver<Result<(), String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.send_blocking(Err("当前 coding agent 不支持运行中追加输入。".into()));
        receiver
    }

    fn run_prompt(&self, request: AgentRequest) -> AgentRun;
}

fn unsupported_receiver<T: Send + 'static>(
    capability: AgentCapability,
) -> Receiver<WorkspaceResult<T>> {
    let (sender, receiver) = async_channel::bounded(1);
    let _ = sender.send_blocking(Err(WorkspaceError::unsupported(capability)));
    receiver
}

fn unsupported_account_receiver<T: Send + 'static>(action: &str) -> Receiver<Result<T, String>> {
    let (sender, receiver) = async_channel::bounded(1);
    let _ = sender.send_blocking(Err(format!("当前 coding agent 不支持{action}")));
    receiver
}
