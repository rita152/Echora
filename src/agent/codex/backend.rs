//! AgentBackend adapter delegating to the application-owned manager.

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use async_channel::Receiver;

use super::manager::CodexAppServerManager;
use crate::agent::{
    AgentAccountSnapshot, AgentBackend, AgentConnectionEvent, AgentLoginCancelOutcome,
    AgentLoginStart, AgentLogoutOutcome, AgentModelCatalog, AgentPermissionProfile,
    AgentRateLimitsRead, AgentRequest, AgentRun, CreateProject, HistoryItemDetail, Page,
    PageRequest, Project, ProjectId, SideConversationRequest, ThreadHistoryItemEntry, ThreadId,
    ThreadListRequest, ThreadMetadataUpdate, ThreadSearchResult, ThreadSection,
    ThreadSectionAppearance, ThreadSectionId, ThreadSummary, ThreadTurn, UpdateProject,
    WorkspaceResult,
};

/// Codex CLI adapter. JSON-RPC details intentionally stay inside this module.
#[derive(Clone)]
pub struct CodexAppServerBackend {
    pub(super) manager: Arc<CodexAppServerManager>,
}

impl CodexAppServerBackend {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(CodexAppServerManager::new()),
        }
    }

    pub fn with_manager(manager: Arc<CodexAppServerManager>) -> Self {
        Self { manager }
    }
}

impl Default for CodexAppServerBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentBackend for CodexAppServerBackend {
    fn capabilities(&self) -> crate::agent::AgentCapabilities {
        self.manager.capabilities()
    }

    fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
        self.manager.subscribe_connection_events()
    }

    fn open_side_conversation(
        &self,
        request: SideConversationRequest,
    ) -> Receiver<WorkspaceResult<ThreadId>> {
        self.manager.open_side_conversation(request)
    }

    fn close_side_conversation(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        self.manager.close_side_conversation(thread_id)
    }

    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
        self.manager.load_model_catalog()
    }

    fn load_permission_profiles(
        &self,
        cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        self.manager.load_permission_profiles(cwd)
    }

    fn update_thread_permissions(
        &self,
        request: crate::agent::AgentThreadPermissionUpdate,
    ) -> Receiver<Result<crate::agent::AgentThreadPermissionResult, String>> {
        self.manager.update_thread_permissions(request)
    }

    fn load_thread_settings(
        &self,
        thread_id: String,
        generation: u64,
    ) -> Receiver<Result<crate::agent::AgentThreadSettingsSnapshot, String>> {
        self.manager.load_thread_settings(thread_id, generation)
    }

    fn config_choices(&self) -> Vec<crate::agent::AgentConfigChoiceSet> {
        super::config::config_choices()
    }

    fn read_account(&self) -> Receiver<Result<AgentAccountSnapshot, String>> {
        self.manager.read_account()
    }

    fn read_rate_limits(&self) -> Receiver<Result<AgentRateLimitsRead, String>> {
        self.manager.read_rate_limits()
    }

    fn start_chatgpt_login(&self) -> Receiver<Result<AgentLoginStart, String>> {
        self.manager
            .start_login(super::manager::CHATGPT_LOGIN_TYPE.to_owned())
    }

    fn cancel_login(&self, login_id: String) -> Receiver<Result<AgentLoginCancelOutcome, String>> {
        self.manager.cancel_login(login_id)
    }

    fn logout_account(&self) -> Receiver<Result<AgentLogoutOutcome, String>> {
        self.manager.logout()
    }

    fn read_config(
        &self,
        cwd: PathBuf,
    ) -> Receiver<Result<crate::agent::AgentConfigSnapshot, crate::agent::AgentConfigError>> {
        self.manager.read_config(cwd)
    }
    fn write_config(
        &self,
        write: crate::agent::AgentConfigWrite,
    ) -> Receiver<Result<crate::agent::AgentConfigSaveResult, crate::agent::AgentConfigError>> {
        self.manager.write_config(write)
    }

    fn load_skills(
        &self,
        request: crate::agent::AgentSkillsLoadRequest,
    ) -> Receiver<Result<crate::agent::AgentSkillsSnapshot, crate::agent::AgentSkillsError>> {
        self.manager.load_skills(request)
    }

    fn write_skill_config(
        &self,
        request: crate::agent::AgentSkillWriteRequest,
    ) -> Receiver<Result<crate::agent::AgentSkillWriteReceipt, crate::agent::AgentSkillsError>>
    {
        self.manager.write_skill_config(request)
    }

    fn list_mcp_servers(
        &self,
        request: crate::agent::AgentMcpServerStatusRequest,
    ) -> Receiver<Result<crate::agent::AgentMcpServerPage, crate::agent::AgentMcpError>> {
        self.manager.list_mcp_servers(request)
    }

    fn reload_mcp_servers(
        &self,
        request: crate::agent::AgentMcpReloadRequest,
    ) -> Receiver<crate::agent::AgentMcpReloadResult> {
        self.manager.reload_mcp_servers(request)
    }

    fn start_mcp_oauth_login(
        &self,
        request: crate::agent::AgentMcpOauthLoginRequest,
    ) -> Receiver<Result<crate::agent::AgentMcpOauthLogin, crate::agent::AgentMcpError>> {
        self.manager.start_mcp_oauth_login(request)
    }

    fn cancel_mcp_oauth_login(
        &self,
        login_id: u64,
    ) -> Receiver<Result<(), crate::agent::AgentMcpError>> {
        self.manager.cancel_mcp_oauth_login(login_id)
    }

    fn load_apps(
        &self,
        request: crate::agent::AgentAppsListRequest,
    ) -> Receiver<Result<crate::agent::AgentAppsPage, crate::agent::AgentAppsError>> {
        self.manager.load_apps(request)
    }

    fn load_installed_apps(
        &self,
        request: crate::agent::AgentAppsInstalledRequest,
    ) -> Receiver<Result<crate::agent::AgentInstalledApps, crate::agent::AgentAppsError>> {
        self.manager.load_installed_apps(request)
    }

    fn read_apps(
        &self,
        request: crate::agent::AgentAppsReadRequest,
    ) -> Receiver<Result<crate::agent::AgentAppsReadResult, crate::agent::AgentAppsError>> {
        self.manager.read_apps(request)
    }

    fn load_plugin_catalog(
        &self,
        request: crate::agent::AgentPluginCatalogRequest,
    ) -> Receiver<Result<crate::agent::AgentPluginCatalog, crate::agent::AgentPluginsError>> {
        self.manager.load_plugin_catalog(request)
    }

    fn load_installed_plugins(
        &self,
        request: crate::agent::AgentPluginInstalledRequest,
    ) -> Receiver<Result<crate::agent::AgentPluginCatalog, crate::agent::AgentPluginsError>> {
        self.manager.load_installed_plugins(request)
    }

    fn read_plugin(
        &self,
        request: crate::agent::AgentPluginReadRequest,
    ) -> Receiver<Result<crate::agent::AgentPluginDetail, crate::agent::AgentPluginsError>> {
        self.manager.read_plugin(request)
    }

    fn search_plugins(
        &self,
        request: crate::agent::AgentPluginSearchRequest,
    ) -> Receiver<Result<crate::agent::AgentPluginSearchPage, crate::agent::AgentPluginsError>>
    {
        self.manager.search_plugins(request)
    }

    fn read_plugin_skill(
        &self,
        request: crate::agent::AgentPluginSkillReadRequest,
    ) -> Receiver<Result<crate::agent::AgentPluginSkillContent, crate::agent::AgentPluginsError>>
    {
        self.manager.read_plugin_skill(request)
    }

    fn reconcile_plugins(
        &self,
        request: crate::agent::AgentPluginReconcileRequest,
    ) -> Receiver<Result<crate::agent::AgentPluginReconcileReceipt, crate::agent::AgentPluginsError>>
    {
        self.manager.reconcile_plugins(request)
    }

    fn install_plugin(
        &self,
        request: crate::agent::AgentPluginInstallRequest,
    ) -> Receiver<crate::agent::AgentPluginInstallResult> {
        self.manager.install_plugin(request)
    }

    fn uninstall_plugin(
        &self,
        request: crate::agent::AgentPluginUninstallRequest,
    ) -> Receiver<crate::agent::AgentPluginUninstallResult> {
        self.manager.uninstall_plugin(request)
    }

    fn plugin_share_list(
        &self,
    ) -> Receiver<Result<crate::agent::AgentPluginShareList, crate::agent::AgentPluginsError>> {
        self.manager.plugin_share_list()
    }

    fn save_plugin_share(
        &self,
        request: crate::agent::AgentPluginShareSaveRequest,
    ) -> Receiver<crate::agent::AgentPluginShareSaveResult> {
        self.manager.save_plugin_share(request)
    }

    fn update_plugin_share_targets(
        &self,
        request: crate::agent::AgentPluginShareUpdateTargetsRequest,
    ) -> Receiver<crate::agent::AgentPluginShareUpdateTargetsResult> {
        self.manager.update_plugin_share_targets(request)
    }

    fn delete_plugin_share(
        &self,
        request: crate::agent::AgentPluginShareDeleteRequest,
    ) -> Receiver<crate::agent::AgentPluginShareDeleteResult> {
        self.manager.delete_plugin_share(request)
    }

    fn add_marketplace(
        &self,
        request: crate::agent::AgentMarketplaceAddRequest,
    ) -> Receiver<crate::agent::AgentMarketplaceAddResult> {
        self.manager.add_marketplace(request)
    }

    fn remove_marketplace(
        &self,
        request: crate::agent::AgentMarketplaceRemoveRequest,
    ) -> Receiver<crate::agent::AgentMarketplaceRemoveResult> {
        self.manager.remove_marketplace(request)
    }

    fn upgrade_marketplaces(
        &self,
        request: crate::agent::AgentMarketplaceUpgradeRequest,
    ) -> Receiver<crate::agent::AgentMarketplaceUpgradeResult> {
        self.manager.upgrade_marketplaces(request)
    }

    fn list_projects(&self, page: PageRequest) -> Receiver<WorkspaceResult<Page<Project>>> {
        self.manager.list_projects(page)
    }

    fn create_project(&self, project: CreateProject) -> Receiver<WorkspaceResult<Project>> {
        self.manager.create_project(project)
    }

    fn update_project(
        &self,
        project_id: ProjectId,
        update: UpdateProject,
    ) -> Receiver<WorkspaceResult<Project>> {
        self.manager.update_project(project_id, update)
    }

    fn delete_project(&self, project_id: ProjectId) -> Receiver<WorkspaceResult<()>> {
        self.manager.delete_project(project_id)
    }

    fn move_project(
        &self,
        project_id: ProjectId,
        before_project_id: Option<ProjectId>,
    ) -> Receiver<WorkspaceResult<()>> {
        self.manager.move_project(project_id, before_project_id)
    }

    fn list_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
        self.manager.list_threads(request)
    }

    fn search_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSearchResult>>> {
        self.manager.search_threads(request)
    }

    fn read_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.manager.read_thread(thread_id)
    }

    fn list_thread_turns(
        &self,
        thread_id: ThreadId,
        page: PageRequest,
        detail: HistoryItemDetail,
    ) -> Receiver<WorkspaceResult<Page<ThreadTurn>>> {
        self.manager.list_thread_turns(thread_id, page, detail)
    }

    fn list_thread_items(
        &self,
        thread_id: ThreadId,
        turn_id: Option<String>,
        page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadHistoryItemEntry>>> {
        self.manager.list_thread_items(thread_id, turn_id, page)
    }

    fn revert_thread(
        &self,
        request: crate::agent::AgentThreadRevert,
    ) -> Receiver<WorkspaceResult<crate::agent::AgentThreadRevertOutcome>> {
        self.manager.revert_thread(request)
    }

    fn start_thread_compaction(&self, thread_id: ThreadId) -> Receiver<Result<(), String>> {
        self.manager.start_thread_compaction(thread_id)
    }

    fn open_file_search_session(
        &self,
        roots: Vec<String>,
    ) -> Receiver<Result<crate::agent::AgentFileSearchSession, String>> {
        self.manager.open_file_search_session(roots)
    }

    fn set_thread_name(&self, thread_id: ThreadId, name: String) -> Receiver<WorkspaceResult<()>> {
        self.manager.set_thread_name(thread_id, name)
    }

    fn archive_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        self.manager.archive_thread(thread_id)
    }

    fn unarchive_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.manager.unarchive_thread(thread_id)
    }

    fn delete_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        self.manager.delete_thread(thread_id)
    }

    fn update_thread_metadata(
        &self,
        thread_id: ThreadId,
        update: ThreadMetadataUpdate,
    ) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.manager.update_thread_metadata(thread_id, update)
    }

    fn list_thread_sections(
        &self,
        page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSection>>> {
        self.manager.list_thread_sections(page)
    }

    fn create_thread_section(
        &self,
        name: String,
        appearance: Option<ThreadSectionAppearance>,
    ) -> Receiver<WorkspaceResult<ThreadSection>> {
        self.manager.create_thread_section(name, appearance)
    }

    fn move_thread_to_section(
        &self,
        thread_id: ThreadId,
        section_id: Option<ThreadSectionId>,
        before_thread_id: Option<ThreadId>,
    ) -> Receiver<WorkspaceResult<()>> {
        self.manager
            .move_thread_to_section(thread_id, section_id, before_thread_id)
    }

    fn steer_turn(&self, request: crate::agent::AgentSteerRequest) -> Receiver<Result<(), String>> {
        self.manager.steer_turn(request)
    }

    fn run_prompt(&self, request: AgentRequest) -> AgentRun {
        self.manager.run_prompt(request)
    }
}
