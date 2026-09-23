mod capture;
mod conversations;
mod image_preview;
mod permissions;
mod plan;
mod project_creation;
mod render;
mod review;
mod right_panel;
mod side_chat;
mod sidebar;
mod state;
mod workspace_panels;

use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use gpui::{Context, Entity, WindowAppearance, prelude::*};

use state::{
    ImagePreviewState, ProjectCreationState, RightPanelMode, RightPanelState, SidebarLayoutState,
};

gpui::actions!(
    permission_ui,
    [
        DismissPermissionUi,
        /// Escape inside the task rename dialog. The dialog is modal, so the
        /// key needs its own context: the global `escape` binding belongs to
        /// permission prompts and would otherwise swallow it.
        DismissThreadRename,
        CaptureFrame,
        ToggleTerminal,
        ToggleReview,
        OpenFiles,
        OpenSideChat,
        OpenSettingsPage,
        NextSettingsControl,
        PreviousSettingsControl
    ]
);

use crate::{
    agent::{
        AgentAccountLoginPhase, AgentBackend, AgentConnectionEvent, CodexAppServerBackend,
        CodexAppServerManager, ProjectId, ThreadId,
    },
    components::{
        account::{AccountDialog, AccountLoadStatus, AccountView},
        chat_search::{ChatSearchView, OpenFile, OpenFolder, SelectChat, StartNewChat},
        composer::{
            ComposerView, ConversationThreadCreated, ModelCatalogLoadFinished,
            RequestFullAccessConfirmation,
        },
        file_panel::FilePanel,
        home::{
            HomeView, OpenDiffReview, OpenImagePreview, OpenSubAgentPanel, RetryImageGeneration,
        },
        pull_requests::{OpenChatForPullRequest, PullRequestsView},
        review_panel::ReviewPanel,
        side_chat::SideChatPanel,
        sidebar::{
            AccountAction, AccountIntent, NewConversation, OpenChatSearch, OpenProjectCreation,
            OpenSettings, SelectThread, SidebarView,
        },
        terminal::TerminalPanel,
    },
    media::read_image_dimensions,
    settings::{ChangeTheme, CloseSettings, SettingsView},
    theme::ThemeMode,
    workspace::WorkspaceStore,
};

pub struct ChatApp {
    codex_app_server: Arc<CodexAppServerManager>,
    agent_backend: Arc<dyn AgentBackend>,
    workspace_store: Arc<WorkspaceStore>,
    conversation_hosts: HashMap<ConversationKey, ConversationHost>,
    active_conversation: ConversationKey,
    next_draft_id: u64,
    mode: ThemeMode,
    root_focus: gpui::FocusHandle,
    startup_model_catalog_resolved: bool,
    startup_sidebar_resolved: bool,
    startup_minimum_duration_elapsed: bool,
    sidebar: Entity<SidebarView>,
    chat_search: Entity<ChatSearchView>,
    home: Entity<HomeView>,
    settings: Entity<SettingsView>,
    showing_settings: bool,
    pull_requests: Entity<PullRequestsView>,
    /// True while the main content area shows the Pull Requests page.
    showing_pull_requests: bool,
    /// Capture-only: keep the Pull Requests page open even if the startup
    /// sequence later selects a conversation.
    capture_pull_requests_locked: bool,
    sidebar_layout: SidebarLayoutState,
    terminal_panels: HashMap<ConversationKey, Entity<TerminalPanel>>,
    file_panels: HashMap<ConversationKey, Entity<FilePanel>>,
    review_panels: HashMap<ConversationKey, Entity<ReviewPanel>>,
    plan_export_error: Option<String>,
    side_chat_panels: HashMap<ConversationKey, Entity<SideChatPanel>>,
    file_close_prompt_open: bool,
    terminal_return_focus_pending: bool,
    right_panel: RightPanelState,
    image_preview: ImagePreviewState,
    permission_confirmation_open: bool,
    permission_confirmation_focus: gpui::FocusHandle,
    permission_confirmation_focus_pending: bool,
    permission_confirmation_choice: usize,
    permission_confirmation_keyboard: bool,
    permission_confirmation_target: Option<Entity<ComposerView>>,
    project_creation: ProjectCreationState,
    account: AccountView,
    account_focus: gpui::FocusHandle,
    account_focus_pending: bool,
    /// Keyboard focus inside the account dialog.
    account_choice: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct DraftId(u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ConversationKey {
    Draft(DraftId),
    Thread(ThreadId),
}

struct ConversationHost {
    composer: Entity<ComposerView>,
    cwd: PathBuf,
    project_id: Option<ProjectId>,
}

impl Drop for ChatApp {
    fn drop(&mut self) {
        #[cfg(not(test))]
        crate::git_review::shutdown();
        self.codex_app_server.shutdown();
    }
}

const RIGHT_PANEL_ITEMS: &[(RightPanelMode, &str, &str, &str)] = &[
    (RightPanelMode::SideChat, "侧边聊天", "⌥⌘S", "side-chat"),
    (RightPanelMode::Browser, "浏览器", "⌘T", "panel-browser"),
    (RightPanelMode::Terminal, "终端", "⌃`", "panel-terminal"),
    (RightPanelMode::Files, "文件", "⌘P", "panel-files"),
    (RightPanelMode::Review, "审查", "⌃⇧G", "panel-review"),
];
const SIDEBAR_MIN_WIDTH: f32 = 240.0;
const SIDEBAR_MAX_WIDTH: f32 = 480.0;
const RIGHT_PANEL_MIN_WIDTH: f32 = 320.0;
const RIGHT_PANEL_MAIN_MIN_WIDTH: f32 = 384.0;
const SUBAGENT_PANEL_DEFAULT_WIDTH: f32 = 603.0;
const SUBAGENT_PANEL_HEADER_HEIGHT: f32 = 48.0;
// The native 14px traffic lights start at y=18px, so their center is y=25px.
// Center the 28px leading titlebar controls on that same horizontal axis.
const LEADING_TITLEBAR_CONTROLS_TOP: f32 = 11.0;
const STARTUP_LOADING_LOGO_SIZE: f32 = 48.0;
const STARTUP_LOADING_BLINK_DURATION: Duration = Duration::from_millis(1_200);
const STARTUP_LOADING_MINIMUM_DURATION: Duration = Duration::from_secs(1);

const SIDEBAR_TRANSITION_DURATION: Duration = Duration::from_millis(400);

impl ChatApp {
    pub fn new(mode: ThemeMode, scroll_sidebar_to_bottom: bool, cx: &mut Context<Self>) -> Self {
        let codex_app_server = Arc::new(CodexAppServerManager::new());
        let agent_backend: Arc<dyn AgentBackend> = Arc::new(CodexAppServerBackend::with_manager(
            codex_app_server.clone(),
        ));
        let workspace_store = WorkspaceStore::new(agent_backend.clone());
        let sidebar = cx.new(|cx| {
            SidebarView::new(mode, scroll_sidebar_to_bottom, workspace_store.clone(), cx)
        });
        #[cfg(not(test))]
        workspace_store.refresh_all();
        #[cfg(not(test))]
        let workspace_receiver = workspace_store.subscribe();
        let settings = cx.new(|cx| SettingsView::new(mode, agent_backend.clone(), cx));
        let pull_requests = cx.new(|cx| PullRequestsView::new(mode, None, cx));
        let chat_search = cx.new(|cx| {
            ChatSearchView::new(mode, workspace_store.clone(), agent_backend.clone(), cx)
        });
        cx.subscribe(&sidebar, |this, _, _: &OpenChatSearch, cx| {
            this.update_chat_search_roots(cx);
            this.chat_search.update(cx, |search, cx| search.open(cx));
        })
        .detach();
        cx.subscribe(&chat_search, |this, _, event: &SelectChat, cx| {
            this.select_conversation(event.0.clone(), cx);
        })
        .detach();
        cx.subscribe(&chat_search, |this, _, _: &StartNewChat, cx| {
            let (project_id, cwd) = this
                .sidebar
                .update(cx, |sidebar, _| sidebar.new_conversation_target());
            this.start_draft(project_id, cwd, cx);
        })
        .detach();
        cx.subscribe(&chat_search, |this, _, _: &OpenFolder, cx| {
            this.open_project_creation(cx);
        })
        .detach();
        cx.subscribe(&chat_search, |this, _, event: &OpenFile, cx| {
            this.open_matched_file(event.path.clone(), event.is_directory, cx);
        })
        .detach();
        let home = cx.new(|cx| HomeView::new_with_backend(mode, agent_backend.clone(), cx));
        let initial_composer = home.read(cx).composer_entity();
        let initial_cwd = std::env::current_dir().unwrap_or_default();
        initial_composer.update(cx, |composer, cx| {
            composer.set_workspace_context(initial_cwd.clone(), None, None, cx);
        });
        let active_conversation = ConversationKey::Draft(DraftId(1));
        let mut conversation_hosts = HashMap::new();
        conversation_hosts.insert(
            active_conversation.clone(),
            ConversationHost {
                composer: initial_composer,
                cwd: initial_cwd,
                project_id: None,
            },
        );
        cx.subscribe(&sidebar, |this, _, _: &OpenSettings, cx| {
            this.open_settings(cx);
        })
        .detach();
        cx.subscribe(
            &sidebar,
            |this, _, _: &crate::components::sidebar::OpenPullRequests, cx| {
                this.open_pull_requests(cx);
            },
        )
        .detach();
        cx.subscribe(
            &pull_requests,
            |this, _, event: &OpenChatForPullRequest, cx| {
                let prompt = event.prompt.clone();
                let cwd = this
                    .conversation_hosts
                    .get(&this.active_conversation)
                    .map(|host| host.cwd.clone())
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                this.showing_pull_requests = false;
                this.start_draft(None, cwd, cx);
                let key = this.active_conversation.clone();
                if let Some(host) = this.conversation_hosts.get(&key) {
                    host.composer.update(cx, |composer, cx| {
                        composer.set_draft_text(&prompt, cx);
                    });
                }
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(&sidebar, |this, _, event: &AccountAction, cx| {
            this.handle_account_intent(event.0.clone(), cx);
        })
        .detach();
        // Account surfaces are connection-scoped: they follow the manager's
        // snapshot instead of any single conversation.
        let account_events = agent_backend.subscribe_connection_events();
        cx.spawn(async move |this, cx| {
            while let Ok(event) = account_events.recv().await {
                if this
                    .update(cx, |this, cx| this.apply_account_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.subscribe(
            &home,
            |this, _, _: &crate::components::home::OpenHookSettings, cx| {
                this.settings
                    .update(cx, |settings, cx| settings.select("hooks-settings", cx));
                this.showing_settings = true;
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(&sidebar, |this, _, _: &OpenProjectCreation, cx| {
            this.open_project_creation(cx);
        })
        .detach();
        cx.subscribe(&sidebar, |this, _, event: &SelectThread, cx| {
            this.close_pull_requests(cx);
            this.select_conversation(event.thread_id.clone(), cx);
        })
        .detach();
        cx.subscribe(&sidebar, |this, _, event: &NewConversation, cx| {
            this.close_pull_requests(cx);
            this.start_draft(event.project_id.clone(), event.cwd.clone(), cx);
        })
        .detach();
        cx.subscribe(&settings, |this, _, _: &CloseSettings, cx| {
            this.showing_settings = false;
            this.showing_pull_requests = false;
            if let Some(host) = this.conversation_hosts.get(&this.active_conversation) {
                host.composer
                    .update(cx, |composer, cx| composer.refresh_draft_defaults(cx));
            }
            cx.notify();
        })
        .detach();
        cx.subscribe(
            &settings,
            |this, _, _: &crate::settings::ConfigSaveFinished, cx| {
                for host in this.conversation_hosts.values() {
                    host.composer
                        .update(cx, |composer, cx| composer.refresh_draft_defaults(cx));
                }
            },
        )
        .detach();
        cx.subscribe(
            &settings,
            |this, _, event: &crate::settings::ChangeLanguage, cx| {
                crate::i18n::set_language(event.0);
                this.workspace_store.set_language(event.0);
                // Invalidate hidden views too: a window refresh only redraws
                // the settings subtree currently on screen.
                this.refresh_child_views(cx);
                cx.refresh_windows();
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(&settings, |this, _, event: &ChangeTheme, cx| {
            this.mode = event.0;
            cx.set_window_appearance(Some(match event.0 {
                ThemeMode::Light => WindowAppearance::Light,
                ThemeMode::Dark => WindowAppearance::Dark,
            }));
            this.refresh_child_views(cx);
            cx.notify();
        })
        .detach();
        cx.subscribe(&home, |this, _, _: &RequestFullAccessConfirmation, cx| {
            this.open_permission_confirmation(this.home.read(cx).composer_entity(), cx);
        })
        .detach();
        cx.subscribe(&home, |this, _, _: &ModelCatalogLoadFinished, cx| {
            this.startup_model_catalog_resolved = true;
            cx.notify();
        })
        .detach();
        cx.subscribe(
            &home,
            |this, _, event: &crate::components::home::OpenPlan, cx| {
                this.open_plan(event.0.clone(), cx);
            },
        )
        .detach();
        cx.subscribe(
            &home,
            |this, _, event: &crate::components::home::DownloadPlan, cx| {
                this.download_plan(event.0.clone(), cx);
            },
        )
        .detach();
        cx.subscribe(&home, |this, _, event: &OpenDiffReview, cx| {
            this.open_diff_review(event.0.clone(), cx);
        })
        .detach();
        cx.subscribe(&home, |this, _, event: &OpenImagePreview, cx| {
            this.image_preview.path = Some(event.0.clone());
            this.image_preview.dimensions = read_image_dimensions(&event.0).ok().flatten();
            this.image_preview.zoom = 1.0;
            cx.notify();
        })
        .detach();
        cx.subscribe(&home, |this, _, event: &OpenSubAgentPanel, cx| {
            this.open_subagent_panel(event.clone(), cx);
        })
        .detach();
        cx.subscribe(&home, |this, _, _: &RetryImageGeneration, cx| {
            if let Some(host) = this.conversation_hosts.get(&this.active_conversation) {
                host.composer
                    .update(cx, |composer, cx| composer.retry_image_generation(cx));
            }
        })
        .detach();
        cx.subscribe(&home, |this, _, event: &ConversationThreadCreated, cx| {
            this.rekey_created_thread(event.thread_id.clone(), cx);
        })
        .detach();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(STARTUP_LOADING_MINIMUM_DURATION)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.startup_minimum_duration_elapsed = true;
                cx.notify();
            });
        })
        .detach();
        #[cfg(not(test))]
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| this.refresh_account(cx));
        })
        .detach();
        #[cfg(not(test))]
        cx.spawn(async move |this, cx| {
            while let Ok(snapshot) = workspace_receiver.recv().await {
                if startup_sidebar_resolved(&snapshot) {
                    let _ = this.update(cx, |this, cx| {
                        this.startup_sidebar_resolved = true;
                        cx.notify();
                    });
                    break;
                }
            }
        })
        .detach();
        Self {
            codex_app_server,
            agent_backend,
            workspace_store,
            conversation_hosts,
            active_conversation,
            next_draft_id: 2,
            mode,
            root_focus: cx.focus_handle(),
            // Unit tests intentionally exercise the full shell without spawning
            // the external Codex model-catalog process.
            startup_model_catalog_resolved: cfg!(test),
            startup_sidebar_resolved: cfg!(test),
            startup_minimum_duration_elapsed: cfg!(test),
            sidebar,
            chat_search,
            home,
            settings,
            showing_settings: false,
            pull_requests,
            showing_pull_requests: false,
            capture_pull_requests_locked: false,
            sidebar_layout: SidebarLayoutState::default(),
            terminal_panels: HashMap::new(),
            file_panels: HashMap::new(),
            review_panels: HashMap::new(),
            plan_export_error: None,
            side_chat_panels: HashMap::new(),
            file_close_prompt_open: false,
            terminal_return_focus_pending: false,
            right_panel: RightPanelState::new(cx),
            image_preview: ImagePreviewState::new(cx),
            permission_confirmation_open: false,
            permission_confirmation_focus: cx.focus_handle(),
            permission_confirmation_focus_pending: false,
            permission_confirmation_choice: 0,
            permission_confirmation_keyboard: false,
            permission_confirmation_target: None,
            project_creation: ProjectCreationState::new(cx),
            account: AccountView::default(),
            account_focus: cx.focus_handle(),
            account_focus_pending: false,
            account_choice: 0,
        }
    }

    /// Reapply presentation to child entities, including cached inputs that
    /// are hidden while settings are open. Preserves drafts and session state.
    fn refresh_child_views(&mut self, cx: &mut Context<Self>) {
        for panel in self.file_panels.values() {
            panel.update(cx, |panel, cx| panel.set_mode(self.mode, cx));
        }
        for panel in self.review_panels.values() {
            panel.update(cx, |panel, cx| panel.set_mode(self.mode, cx));
        }
        for panel in self.terminal_panels.values() {
            panel.update(cx, |panel, cx| panel.set_mode(self.mode, cx));
        }
        for panel in self.side_chat_panels.values() {
            panel.update(cx, |panel, cx| panel.set_mode(self.mode, cx));
        }
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_mode(self.mode, cx);
        });
        self.chat_search.update(cx, |search, cx| {
            search.set_mode(self.mode, cx);
        });
        self.home.update(cx, |home, cx| {
            home.set_mode(self.mode, cx);
        });
        for host in self.conversation_hosts.values() {
            host.composer.update(cx, |composer, cx| {
                composer.set_mode(self.mode, cx);
            });
        }
        if let Some(panel) = &self.right_panel.subagent {
            panel
                .home
                .update(cx, |home, cx| home.set_mode(self.mode, cx));
        }
        self.pull_requests
            .update(cx, |view, cx| view.set_mode(self.mode, cx));
    }

    /// Reduces the account parts of a connection event. These events never
    /// touch a conversation or a turn.
    fn apply_account_event(&mut self, event: AgentConnectionEvent, cx: &mut Context<Self>) {
        let changed = match event {
            AgentConnectionEvent::AccountUpdated(snapshot) => {
                if self.account.state.account == snapshot {
                    false
                } else {
                    self.account.state.account = snapshot;
                    true
                }
            }
            AgentConnectionEvent::AccountLoginUpdated(login) => {
                if self.account.state.login == login {
                    false
                } else {
                    self.account.state.login = login;
                    true
                }
            }
            AgentConnectionEvent::AccountRateLimitsUpdated(rate_limits) => {
                if self.account.state.rate_limits == rate_limits {
                    false
                } else {
                    self.account.state.rate_limits = rate_limits;
                    true
                }
            }
            _ => false,
        };
        if !changed {
            return;
        }
        // A confirmed login ends the login dialog; a new one keeps it open.
        if self.account.state.login.phase == AgentAccountLoginPhase::SignedIn {
            self.account.dialog = None;
            self.account.action_error = None;
        }
        self.sync_account_view(cx);
    }

    fn sync_account_view(&mut self, cx: &mut Context<Self>) {
        let view = self.account.clone();
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_account_view(view.clone(), cx));
        cx.notify();
    }

    /// account/read followed by account/rateLimits/read. Both answers are also
    /// reduced into the connection snapshot other views observe.
    pub fn refresh_account(&mut self, cx: &mut Context<Self>) {
        if self.account.is_loading() {
            return;
        }
        self.account.status = AccountLoadStatus::Loading;
        self.sync_account_view(cx);
        let backend = self.agent_backend.clone();
        let account_reader = backend.read_account();
        cx.spawn(async move |this, cx| {
            let account_result = account_reader.recv().await;
            let limits_reader = backend.read_rate_limits();
            let limits_result = limits_reader.recv().await;
            let status = match (&account_result, &limits_result) {
                (Ok(Ok(_)), Ok(Ok(_))) => AccountLoadStatus::Loaded,
                (Ok(Err(error)), _) => AccountLoadStatus::Failed(error.clone()),
                (Err(_), _) => AccountLoadStatus::Failed(
                    crate::i18n::text("账户连接在返回结果前关闭").to_owned(),
                ),
                (_, Err(_)) => AccountLoadStatus::Failed(
                    crate::i18n::text("配额连接在返回结果前关闭").to_owned(),
                ),
                (_, Ok(Err(error))) => AccountLoadStatus::Failed(error.clone()),
            };
            let _ = this.update(cx, |this, cx| {
                this.account.status = status;
                this.sync_account_view(cx);
            });
        })
        .detach();
    }

    fn handle_account_intent(&mut self, intent: AccountIntent, cx: &mut Context<Self>) {
        match intent {
            AccountIntent::Refresh => self.refresh_account(cx),
            AccountIntent::StartLogin => self.start_login(cx),
            AccountIntent::CancelLogin(login_id) => self.cancel_login(login_id, cx),
            AccountIntent::RequestLogout => {
                self.account.dialog = Some(AccountDialog::Logout);
                self.account.action_error = None;
                self.account_choice = 0;
                self.account_focus_pending = true;
                self.sync_account_view(cx);
            }
            AccountIntent::OpenExternalUrl(url) => cx.open_url(&url),
        }
    }

    fn start_login(&mut self, cx: &mut Context<Self>) {
        self.account.action_error = None;
        let receiver = self.agent_backend.start_chatgpt_login();
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(_)) => {
                        // The login id and challenge arrive through the
                        // connection snapshot; nothing is assumed here.
                        this.account.dialog = Some(AccountDialog::Login);
                        this.account_focus_pending = true;
                    }
                    Ok(Err(error)) => {
                        // The login state carries the failure so the dialog can
                        // offer a retry, and the menu keeps the same error.
                        this.account.state.apply_login_failed(None, error.clone());
                        this.account.action_error = Some(error);
                        this.account.dialog = Some(AccountDialog::Login);
                    }
                    Err(_) => {
                        let error = crate::i18n::text("登录连接在返回结果前关闭").to_owned();
                        this.account.state.apply_login_failed(None, error.clone());
                        this.account.action_error = Some(error);
                        this.account.dialog = Some(AccountDialog::Login);
                    }
                }
                this.sync_account_view(cx);
            });
        })
        .detach();
    }

    fn cancel_login(&mut self, login_id: String, cx: &mut Context<Self>) {
        let receiver = self.agent_backend.cancel_login(login_id);
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(_)) => this.account.action_error = None,
                    Ok(Err(error)) => this.account.action_error = Some(error),
                    Err(_) => {
                        this.account.action_error =
                            Some(crate::i18n::text("取消登录连接在返回结果前关闭").to_owned())
                    }
                }
                this.sync_account_view(cx);
            });
        })
        .detach();
    }

    /// Logout runs only after the confirmation is accepted. The answer is the
    /// backend's response plus the account/quote reads that follow it.
    fn confirm_logout(&mut self, cx: &mut Context<Self>) {
        self.account.dialog = None;
        self.account.status = AccountLoadStatus::Loading;
        self.sync_account_view(cx);
        let receiver = self.agent_backend.logout_account();
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(outcome)) => {
                        this.account.status = AccountLoadStatus::Loaded;
                        this.account.action_error = outcome.confirmation_error;
                    }
                    Ok(Err(error)) => {
                        this.account.status = AccountLoadStatus::Failed(error.clone());
                        this.account.action_error = Some(error);
                    }
                    Err(_) => {
                        let error = crate::i18n::text("退出登录连接在返回结果前关闭").to_owned();
                        this.account.status = AccountLoadStatus::Failed(error.clone());
                        this.account.action_error = Some(error);
                    }
                }
                this.sync_account_view(cx);
            });
        })
        .detach();
    }

    fn dismiss_account_dialog(&mut self, cx: &mut Context<Self>) {
        self.account.dialog = None;
        self.sync_account_view(cx);
    }

    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        let cwd = self
            .conversation_hosts
            .get(&self.active_conversation)
            .map(|host| host.cwd.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        // MCP runtime status is thread scoped; the management surface needs the
        // conversation the user is actually in.
        let thread_id = self
            .conversation_hosts
            .get(&self.active_conversation)
            .and_then(|host| host.composer.read(cx).thread_id().map(str::to_owned));
        self.settings.update(cx, |settings, cx| {
            settings.set_config_context(cwd, cx);
            settings.set_manage_context(thread_id, cx);
            settings.ensure_plugins_segment_loaded(cx);
        });
        self.showing_settings = true;
        self.showing_pull_requests = false;
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_pull_requests_open(false, cx));
        cx.notify();
    }

    /// Capture-only: opens the Pull Requests page and keeps it open while the
    /// rest of the startup sequence runs.
    pub fn open_pull_requests_for_capture(&mut self, cx: &mut Context<Self>) {
        self.capture_pull_requests_locked = true;
        self.open_pull_requests(cx);
    }

    /// Opens the Pull Requests page in the main content area; the sidebar stays
    /// visible and highlights its own `Pull requests` row.
    pub fn open_pull_requests(&mut self, cx: &mut Context<Self>) {
        self.showing_settings = false;
        self.showing_pull_requests = true;
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_pull_requests_open(true, cx));
        self.pull_requests.update(cx, |view, cx| {
            view.set_mode(self.mode, cx);
            view.refresh(cx);
        });
        cx.notify();
    }

    /// Capture helper: selects the Nth row the list currently renders.
    #[cfg(feature = "screenshot")]
    pub fn select_pull_request_index(&mut self, index: usize, cx: &mut Context<Self>) {
        self.pull_requests
            .update(cx, |view, cx| view.select_index(index, cx));
    }

    #[cfg(not(feature = "screenshot"))]
    pub fn select_pull_request_index(&mut self, _index: usize, _cx: &mut Context<Self>) {}

    /// Capture helper: selects the row whose title contains `needle`.
    #[cfg(feature = "screenshot")]
    pub fn select_pull_request_title(&mut self, needle: &str, cx: &mut Context<Self>) {
        self.pull_requests
            .update(cx, |view, cx| view.select_title(needle, cx));
    }

    #[cfg(not(feature = "screenshot"))]
    pub fn select_pull_request_title(&mut self, _needle: &str, _cx: &mut Context<Self>) {}

    /// Capture helper: applies a status filter to the list before it loads.
    #[cfg(feature = "screenshot")]
    pub fn set_pull_requests_status_filter(
        &mut self,
        status: crate::pull_requests::StatusFilter,
        cx: &mut Context<Self>,
    ) {
        self.pull_requests.update(cx, |view, cx| {
            view.set_status_filter_for_capture(status, cx)
        });
    }

    #[cfg(not(feature = "screenshot"))]
    pub fn set_pull_requests_status_filter(
        &mut self,
        _status: crate::pull_requests::StatusFilter,
        _cx: &mut Context<Self>,
    ) {
    }

    /// Capture helper: picks the list tab before the list loads.
    #[cfg(feature = "screenshot")]
    pub fn set_pull_requests_list_tab(&mut self, tab: &str, cx: &mut Context<Self>) {
        use crate::pull_requests::ListTab;
        let tab = match tab {
            "reviewing" => ListTab::Reviewing,
            "authored" => ListTab::Authored,
            _ => ListTab::All,
        };
        self.pull_requests
            .update(cx, |view, cx| view.set_list_tab_for_capture(tab, cx));
    }

    #[cfg(not(feature = "screenshot"))]
    pub fn set_pull_requests_list_tab(&mut self, _tab: &str, _cx: &mut Context<Self>) {}

    /// Capture helper: types a query into the Pull Requests search field.
    #[cfg(feature = "screenshot")]
    pub fn set_pull_requests_search(&mut self, query: &str, cx: &mut Context<Self>) {
        self.pull_requests
            .update(cx, |view, cx| view.set_search_for_capture(query, cx));
    }

    #[cfg(not(feature = "screenshot"))]
    pub fn set_pull_requests_search(&mut self, _query: &str, _cx: &mut Context<Self>) {}

    /// Capture helper: collapses one list grouping header.
    #[cfg(feature = "screenshot")]
    pub fn collapse_pull_requests_group(&mut self, group: &str, cx: &mut Context<Self>) {
        use crate::pull_requests::GroupKind;
        let kind = match group {
            "previously-reviewed" | "reviewed" => GroupKind::PreviouslyReviewed,
            _ => GroupKind::Authored,
        };
        self.pull_requests
            .update(cx, |view, cx| view.collapse_group_for_capture(kind, cx));
    }

    #[cfg(not(feature = "screenshot"))]
    pub fn collapse_pull_requests_group(&mut self, _group: &str, _cx: &mut Context<Self>) {}

    /// Capture helper: scrolls the detail column to an offset.
    #[cfg(feature = "screenshot")]
    pub fn scroll_pull_requests_detail(&mut self, offset: f32, cx: &mut Context<Self>) {
        self.pull_requests
            .update(cx, |view, cx| view.scroll_detail_for_capture(offset, cx));
    }

    #[cfg(not(feature = "screenshot"))]
    pub fn scroll_pull_requests_detail(&mut self, _offset: f32, _cx: &mut Context<Self>) {}

    /// Capture helper: opens one Pull Requests interaction state.
    #[cfg(feature = "screenshot")]
    pub fn open_pull_requests_action(&mut self, action: &str, cx: &mut Context<Self>) {
        self.pull_requests
            .update(cx, |view, cx| view.open_action_for_capture(action, cx));
    }

    #[cfg(not(feature = "screenshot"))]
    pub fn open_pull_requests_action(&mut self, _action: &str, _cx: &mut Context<Self>) {}

    /// Capture helper: opens a detail tab (`summary`, `code`, `review`).
    #[cfg(feature = "screenshot")]
    pub fn open_pull_requests_tab(&mut self, tab: &str, cx: &mut Context<Self>) {
        self.pull_requests
            .update(cx, |view, cx| view.open_tab_for_capture(tab, cx));
    }

    #[cfg(not(feature = "screenshot"))]
    pub fn open_pull_requests_tab(&mut self, _tab: &str, _cx: &mut Context<Self>) {}

    /// Capture helper: shows or hides the diff file tree.
    #[cfg(feature = "screenshot")]
    pub fn set_pull_requests_file_tree(&mut self, open: bool, cx: &mut Context<Self>) {
        self.pull_requests
            .update(cx, |view, cx| view.set_file_tree_for_capture(open, cx));
    }

    #[cfg(not(feature = "screenshot"))]
    pub fn set_pull_requests_file_tree(&mut self, _open: bool, _cx: &mut Context<Self>) {}

    /// Capture diagnostics for the Pull Requests page.
    #[cfg(feature = "screenshot")]
    pub fn pull_requests_diagnostics(&self, cx: &gpui::App) -> String {
        self.pull_requests.read(cx).capture_diagnostics()
    }

    /// Capture helper: whether the Pull Requests page finished loading.
    #[cfg(feature = "screenshot")]
    pub fn pull_requests_capture_ready(&self, cx: &gpui::App) -> bool {
        // The startup gate must have cleared: the page can be fully loaded while
        // the window still paints the loading view.
        self.startup_model_catalog_resolved
            && self.startup_sidebar_resolved
            && self.startup_minimum_duration_elapsed
            && self.showing_pull_requests
            && self.pull_requests.read(cx).capture_ready()
    }

    pub fn close_pull_requests(&mut self, cx: &mut Context<Self>) {
        if self.capture_pull_requests_locked || !self.showing_pull_requests {
            return;
        }
        self.showing_pull_requests = false;
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_pull_requests_open(false, cx));
        cx.notify();
    }

    pub fn open_settings_page(&mut self, slug: &'static str, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.select(slug, cx));
        self.open_settings(cx);
    }

    /// Capture-only fixture selection for the plugins page. Every argument is
    /// an explicit CLI flag; nothing here runs for an ordinary launch.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_manage_capture_flags(
        &mut self,
        segment: Option<&str>,
        detail: Option<&str>,
        login_state: Option<&str>,
        reload_state: Option<&str>,
        mcp_hover_row: Option<&str>,
        skills_hover_row: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        if segment.is_none()
            && detail.is_none()
            && login_state.is_none()
            && reload_state.is_none()
            && mcp_hover_row.is_none()
            && skills_hover_row.is_none()
        {
            return;
        }
        self.settings.update(cx, |settings, cx| {
            settings.apply_manage_capture_fixtures(
                segment,
                detail,
                login_state,
                reload_state,
                (mcp_hover_row, skills_hover_row),
                cx,
            );
        });
    }

    /// Capture gate: the plugins segments have finished their first backend
    /// read, so a captured frame cannot show an unloaded list.
    #[cfg(feature = "screenshot")]
    pub fn manage_capture_ready(&self, cx: &gpui::App) -> bool {
        self.showing_settings && self.settings.read(cx).plugins_capture_ready()
    }
}

impl ChatApp {}

#[cfg(not(test))]
use render::startup_sidebar_resolved;

#[cfg(test)]
mod tests;
