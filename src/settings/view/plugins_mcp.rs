//! MCP server management inside the plugins settings page: inventory, status,
//! reload, detail, and the OAuth login lifecycle.

use std::collections::BTreeMap;

use gpui::{Context, div, prelude::*, px, svg};

/// Registration strategies offered for the next OAuth login, in the schema's
/// own vocabulary.
const REGISTRATION_MODES: [(&str, &str); 3] = [("auto", "自动"), ("cimd", "CIMD"), ("dcr", "DCR")];

use super::{PluginSegment, SettingsView};
use crate::{
    agent::{
        AgentMcpAuthStatus, AgentMcpError, AgentMcpErrorKind, AgentMcpOauthClientRegistration,
        AgentMcpOauthLoginRequest, AgentMcpReloadOutcome, AgentMcpReloadRequest,
        AgentMcpServerConnectionStatus, AgentMcpServerInfo, AgentMcpServerStatus,
        AgentMcpServerStatusRequest, AgentMcpStartupStatusUpdated, AgentMcpStatusDetail,
        AgentMcpTool,
    },
    mcp::{McpDirectory, McpLoginPhase},
    theme::Theme,
};

/// Bound on the cursor walk so a misbehaving server cannot spin the UI.
const MAX_MCP_PAGES: usize = 32;
const MCP_PAGE_LIMIT: u32 = 100;

fn capture_mcp_server(
    name: &str,
    plugin_id: Option<&str>,
    auth_status: AgentMcpAuthStatus,
    runtime_status: AgentMcpServerConnectionStatus,
) -> AgentMcpServerStatus {
    let info = AgentMcpServerInfo {
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        title: None,
        description: None,
        website_url: None,
        icons: None,
        extra: BTreeMap::new(),
    };
    let tools = if name == "computer-use" {
        vec![AgentMcpTool {
            name: "computer_screenshot".to_owned(),
            title: Some("computer_screenshot".to_owned()),
            description: Some("Capture the current desktop state.".to_owned()),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
            extra: BTreeMap::new(),
        }]
    } else {
        Vec::new()
    };
    AgentMcpServerStatus {
        name: name.to_owned(),
        plugin_id: plugin_id.map(str::to_owned),
        auth_status,
        runtime_status: Some(runtime_status),
        server_info: Some(info),
        tools,
        resources: Vec::new(),
        resource_templates: Vec::new(),
        tools_error: None,
        extra: BTreeMap::new(),
    }
}

#[derive(Default)]
pub(super) struct McpPanel {
    pub directory: McpDirectory,
    /// Server whose detail panel is open.
    pub detail: Option<String>,
    /// Registration strategy for the next login. The schema documents this as a
    /// per-login choice; `auto` keeps provider discovery.
    pub registration: AgentMcpOauthClientRegistration,
    /// Capture-only hovered row.
    pub hover_row: Option<String>,
    pub query: String,
    /// The app-server currently exposes status and lifecycle methods, but no
    /// persistent MCP enable/disable method.  Keep the switch optimistic in
    /// the view until such a method is available; a fresh inventory replaces
    /// these transient overrides.
    pub enabled_overrides: BTreeMap<String, bool>,
}

impl McpPanel {
    pub(super) fn visible_servers(&self) -> Vec<&AgentMcpServerStatus> {
        let query = self.query.trim().to_lowercase();
        self.directory
            .order
            .iter()
            .filter_map(|name| self.directory.servers.get(name))
            .filter(|server| {
                query.is_empty()
                    || server.name.to_lowercase().contains(&query)
                    || server
                        .server_info
                        .as_ref()
                        .and_then(|info| info.title.as_deref())
                        .is_some_and(|title| title.to_lowercase().contains(&query))
            })
            .collect()
    }

    pub(super) fn login_for(&self, server: &str) -> Option<&crate::mcp::McpLoginState> {
        self.directory
            .logins
            .values()
            .filter(|login| login.phase.busy())
            .max_by_key(|login| login.login_id)
            .filter(|_| false)
            .or_else(|| {
                self.directory
                    .logins
                    .values()
                    .filter(|login| login.server_name == server)
                    .max_by_key(|login| login.login_id)
            })
    }
}

impl SettingsView {
    /// Installs the same small, stable inventory used by the reference
    /// client's management-page captures.  This path is reachable only when
    /// an explicit capture flag is supplied; production launches always read
    /// the live app-server inventory below.
    pub(super) fn install_mcp_capture_fixture(&mut self, detail: Option<&str>) {
        let mut servers = vec![
            capture_mcp_server(
                "computer-use",
                None,
                AgentMcpAuthStatus::Unsupported,
                AgentMcpServerConnectionStatus::Disabled,
            ),
            capture_mcp_server(
                "node_repl",
                None,
                AgentMcpAuthStatus::Unsupported,
                AgentMcpServerConnectionStatus::Connected,
            ),
            capture_mcp_server(
                "openaiDeveloperDocs",
                None,
                AgentMcpAuthStatus::Unsupported,
                AgentMcpServerConnectionStatus::Connected,
            ),
            capture_mcp_server(
                "codex_apps",
                Some("codex_apps"),
                AgentMcpAuthStatus::Unsupported,
                AgentMcpServerConnectionStatus::Connected,
            ),
        ];
        if let Some(name) = detail
            && !servers.iter().any(|server| server.name == name)
        {
            servers.push(capture_mcp_server(
                name,
                None,
                AgentMcpAuthStatus::NotLoggedIn,
                AgentMcpServerConnectionStatus::NotStarted,
            ));
        }

        self.mcp_generation = 1;
        self.mcp.directory.generation = 1;
        self.mcp.directory.order = servers.iter().map(|server| server.name.clone()).collect();
        self.mcp.directory.servers = servers
            .into_iter()
            .map(|server| (server.name.clone(), server))
            .collect();
        self.mcp.directory.next_cursor = None;
        self.mcp.directory.loading = false;
        self.mcp.directory.error = None;
        self.mcp.directory.reload = None;
        self.mcp.directory.reloading = false;
        self.mcp.enabled_overrides.clear();
    }

    /// Loads the MCP inventory. Cursors are followed with an explicit cycle
    /// guard; a repeated cursor ends the walk with a visible error.
    /// Refreshes the inventory. `detail` follows the schema: `full` reads tool
    /// catalogs, `toolsAndAuthOnly` is the cheap status/auth read used after a
    /// reload or a completed login.
    pub(super) fn refresh_mcp_servers(
        &mut self,
        detail: AgentMcpStatusDetail,
        cx: &mut Context<Self>,
    ) {
        self.mcp.directory.generation = self.mcp_generation;
        self.mcp.directory.begin_refresh();
        cx.notify();
        let backend = self.backend.clone();
        let thread_id = self.mcp_thread_id.clone();
        cx.spawn(async move |this, cx| {
            let mut cursor: Option<String> = None;
            for _ in 0..MAX_MCP_PAGES {
                let request = AgentMcpServerStatusRequest {
                    cursor: cursor.clone(),
                    limit: Some(MCP_PAGE_LIMIT),
                    detail: Some(detail),
                    thread_id: thread_id.clone(),
                };
                let received = backend.list_mcp_servers(request).recv().await;
                let page = match received {
                    Ok(Ok(page)) => page,
                    Ok(Err(error)) => {
                        let _ = this.update(cx, |this, cx| {
                            this.mcp.directory.loading = false;
                            this.mcp.directory.error = Some(error);
                            cx.notify();
                        });
                        return;
                    }
                    Err(_) => {
                        let _ = this.update(cx, |this, cx| {
                            this.mcp.directory.loading = false;
                            this.mcp.directory.error = Some(AgentMcpError {
                                kind: AgentMcpErrorKind::Connection,
                                message: "MCP 列表连接已关闭".into(),
                                data: None,
                                outcome_unknown: false,
                            });
                            cx.notify();
                        });
                        return;
                    }
                };
                let mut next: Option<String> = None;
                let _ = this.update(cx, |this, cx| {
                    // A page from a different generation belongs to a rebuilt
                    // connection; the old inventory is abandoned, not merged.
                    if this.mcp.directory.generation != page.generation {
                        this.mcp.directory.reset_for_generation(page.generation);
                    }
                    this.mcp_generation = page.generation;
                    let partial = detail != AgentMcpStatusDetail::Full;
                    if this.mcp.directory.accept_page(page, partial) {
                        next = this.mcp.directory.next_cursor.clone();
                    }
                    cx.notify();
                });
                match next {
                    Some(next) => cursor = Some(next),
                    None => return,
                }
            }
            let _ = this.update(cx, |this, cx| {
                this.mcp.directory.loading = false;
                this.mcp.directory.error = Some(AgentMcpError {
                    kind: AgentMcpErrorKind::Protocol,
                    message: format!("mcpServerStatus/list 分页超过 {MAX_MCP_PAGES} 页，已中止"),
                    data: None,
                    outcome_unknown: false,
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn reload_mcp_servers(&mut self, cx: &mut Context<Self>) {
        if self.mcp.directory.reloading {
            return;
        }
        self.mcp.directory.reloading = true;
        self.mcp.directory.reload = None;
        cx.notify();
        let backend = self.backend.clone();
        let request = AgentMcpReloadRequest {
            cwd: self.config_cwd.clone(),
            generation: self.mcp.directory.generation,
        };
        cx.spawn(async move |this, cx| {
            let result = backend.reload_mcp_servers(request).recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(result) => {
                        let reloaded = matches!(result.outcome, AgentMcpReloadOutcome::Reloaded);
                        this.mcp.directory.accept_reload(result);
                        if reloaded {
                            // Reload restarts servers; the list is re-read so
                            // status changes come from the server, not from us.
                            this.refresh_mcp_servers(AgentMcpStatusDetail::ToolsAndAuthOnly, cx);
                        }
                    }
                    Err(_) => {
                        this.mcp.directory.reloading = false;
                        this.mcp.directory.error = Some(AgentMcpError {
                            kind: AgentMcpErrorKind::Connection,
                            message: "重新加载连接已关闭，结果未确认".into(),
                            data: None,
                            outcome_unknown: true,
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn start_mcp_login(&mut self, server_name: String, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let request = AgentMcpOauthLoginRequest {
            generation: self.mcp.directory.generation,
            server_name,
            thread_id: self.mcp_thread_id.clone(),
            scopes: None,
            client_registration: Some(self.mcp.registration),
            timeout_secs: None,
        };
        cx.spawn(async move |this, cx| {
            let result = backend.start_mcp_oauth_login(request).recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(login)) => {
                        this.mcp
                            .directory
                            .register_login(crate::mcp::McpLoginState {
                                login_id: login.login_id,
                                generation: login.generation,
                                server_name: login.server_name.clone(),
                                thread_id: login.thread_id.clone(),
                                authorization_url: login.authorization_url.clone(),
                                phase: McpLoginPhase::Waiting,
                            });
                        // The client opens the authorization page; completion
                        // still arrives as a server notification.
                        cx.open_url(&login.authorization_url);
                    }
                    Ok(Err(error)) => this.mcp.directory.error = Some(error),
                    Err(_) => {
                        this.mcp.directory.error = Some(AgentMcpError {
                            kind: AgentMcpErrorKind::Connection,
                            message: "登录连接已关闭".into(),
                            data: None,
                            outcome_unknown: false,
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn cancel_mcp_login(&mut self, login_id: u64, cx: &mut Context<Self>) {
        // Cancellation is local and immediate: the login is retired so a late
        // completion notification can never reopen the dialog.
        self.mcp.directory.cancel_login(login_id);
        cx.notify();
        let backend = self.backend.clone();
        cx.spawn(async move |this, cx| {
            let result = backend.cancel_mcp_oauth_login(login_id).recv().await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(Err(error)) = result {
                    this.mcp.directory.error = Some(error);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn apply_mcp_startup(
        &mut self,
        updated: &AgentMcpStartupStatusUpdated,
        cx: &mut Context<Self>,
    ) {
        if self.mcp.directory.generation == 0 {
            self.mcp.directory.generation = updated.generation;
        }
        self.mcp_generation = updated.generation;
        if self
            .mcp
            .directory
            .apply_startup(updated.generation, updated.status.clone())
        {
            cx.notify();
        }
    }

    pub(super) fn apply_mcp_login_completion(
        &mut self,
        completion: &crate::agent::AgentMcpOauthCompletion,
        cx: &mut Context<Self>,
    ) {
        self.mcp_generation = completion.generation;
        if self.mcp.directory.accept_login_completion(completion) {
            if completion.status.is_success() {
                self.refresh_mcp_servers(AgentMcpStatusDetail::ToolsAndAuthOnly, cx);
            }
            cx.notify();
        }
    }

    /// Escape closes the management surface: a waiting login is cancelled (the
    /// user abandoned it) and the detail panel returns to the list. Confirmed
    /// server operations are never cancelled from here.
    pub(super) fn dismiss_manage_overlays(&mut self, cx: &mut Context<Self>) -> bool {
        let waiting = self
            .mcp
            .directory
            .logins
            .values()
            .filter(|login| login.phase.busy())
            .map(|login| login.login_id)
            .collect::<Vec<_>>();
        if !waiting.is_empty() {
            for login_id in waiting {
                self.cancel_mcp_login(login_id, cx);
            }
            return true;
        }
        if self.mcp.detail.take().is_some() {
            cx.notify();
            return true;
        }
        false
    }

    /// Capture-only fixture installer. Deterministic screenshots need states
    /// that a single run cannot always produce (a failed reload, a login in
    /// flight); these fixtures only exist behind explicit CLI flags.
    pub fn apply_manage_capture_fixtures(
        &mut self,
        segment: Option<&str>,
        detail: Option<&str>,
        login_state: Option<&str>,
        reload_state: Option<&str>,
        hover_rows: (Option<&str>, Option<&str>),
        cx: &mut Context<Self>,
    ) {
        let (mcp_hover_row, skills_hover_row) = hover_rows;
        if let Some(segment) = segment {
            let target = match segment {
                "plugins" => PluginSegment::Plugins,
                "apps" => PluginSegment::Apps,
                "mcp" => PluginSegment::Mcp,
                "skills" => PluginSegment::Skills,
                _ => PluginSegment::Plugins,
            };
            self.plugins_segment = target;
        }
        if let Some(name) = detail {
            self.mcp.detail = Some(name.to_owned());
        }
        self.mcp.hover_row = mcp_hover_row.map(str::to_owned);
        self.skills.hover_row = skills_hover_row.map(str::to_owned);
        if let Some(state) = login_state {
            let server = detail
                .map(str::to_owned)
                .or_else(|| self.mcp.directory.order.first().cloned())
                .unwrap_or_else(|| "notes-oauth".to_owned());
            let phase = match state {
                "waiting" => McpLoginPhase::Waiting,
                "success" => McpLoginPhase::Succeeded,
                "failure" => McpLoginPhase::Failed("OAuth 提供方返回 access_denied".into()),
                "cancelled" => McpLoginPhase::Cancelled,
                "interrupted" => McpLoginPhase::Interrupted("连接已断开".into()),
                _ => McpLoginPhase::Waiting,
            };
            self.mcp
                .directory
                .register_login(crate::mcp::McpLoginState {
                    login_id: 9_000,
                    generation: self.mcp.directory.generation,
                    server_name: server,
                    thread_id: self.mcp_thread_id.clone(),
                    authorization_url: "https://mcp.example.invalid/authorize?state=capture".into(),
                    phase,
                });
        }
        if let Some(state) = reload_state {
            let outcome = match state {
                "ok" => AgentMcpReloadOutcome::Reloaded,
                "failed" => AgentMcpReloadOutcome::Failed {
                    message: "MCP 服务器重新加载失败".into(),
                    data: None,
                },
                "timeout" => AgentMcpReloadOutcome::TimedOut {
                    message: "`config/mcpServer/reload` 等待响应超时".into(),
                },
                _ => AgentMcpReloadOutcome::Unknown {
                    message: "重新加载结果未知".into(),
                },
            };
            self.mcp.directory.reload = Some(crate::agent::AgentMcpReloadResult {
                generation: self.mcp.directory.generation,
                cwd: self.config_cwd.clone(),
                outcome,
            });
        }
        // Capture flags deliberately use a deterministic inventory so the
        // screenshot is not held hostage by a slow or unavailable local
        // coding-agent process.  A normal settings navigation still follows
        // the live read path exactly as a user click does.
        let capture_mode = segment.is_some()
            || detail.is_some()
            || login_state.is_some()
            || reload_state.is_some()
            || mcp_hover_row.is_some()
            || skills_hover_row.is_some();
        if capture_mode {
            match self.plugins_segment {
                // The segment strip reports all four inventories even while
                // one segment is selected. Install both deterministic
                // management fixtures so MCP captures retain the reference
                // "技能 2" badge and Skills captures retain "MCP 3".
                PluginSegment::Mcp => {
                    self.install_mcp_capture_fixture(detail);
                    self.install_skills_capture_fixture();
                }
                PluginSegment::Skills => {
                    self.install_skills_capture_fixture();
                    self.install_mcp_capture_fixture(None);
                }
                PluginSegment::Plugins | PluginSegment::Apps => {}
            }
        } else {
            self.ensure_plugins_segment_loaded(cx);
        }
        cx.notify();
    }

    pub(super) fn mcp_segment_content(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if let Some(name) = self.mcp.detail.clone() {
            return self.mcp_detail_content(&name, theme, cx);
        }
        // The reference gives the list a 40px breathing space below the
        // segment strip and 20px between sections.
        let mut body = div().mt(px(40.0)).flex().flex_col().gap(px(20.0));
        // The healthy list in ChatGPT is intentionally quiet: refresh and
        // reload actions appear in an error/result state or on the detail
        // page, not above every list capture.
        if self.mcp.directory.reload.is_some()
            || self.mcp.directory.error.is_some()
            || self.mcp.directory.reloading
        {
            body = body.child(self.mcp_toolbar(theme, cx));
        }

        if let Some(result) = &self.mcp.directory.reload {
            let (message, failed) = match &result.outcome {
                AgentMcpReloadOutcome::Reloaded => (result.outcome.user_message(), false),
                _ => (result.outcome.user_message(), true),
            };
            let unknown = result.outcome.outcome_unknown();
            body = body.child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .text_color(if failed {
                        theme.warning
                    } else {
                        theme.text_tertiary
                    })
                    .child(if unknown {
                        format!("{message}（结果未知）")
                    } else {
                        message.to_owned()
                    }),
            );
        }
        if let Some(error) = self.mcp.directory.error.clone() {
            body = body.child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(19.0))
                            .text_color(theme.warning)
                            .child(error.user_message()),
                    )
                    .child(
                        div()
                            .id("mcp-retry")
                            .flex_none()
                            .h(px(28.0))
                            .px(px(12.0))
                            .rounded(px(12.5))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.settings_button)
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_mcp_servers(AgentMcpStatusDetail::Full, cx)
                            }))
                            .child("重试"),
                    ),
            );
        }

        let servers = self.mcp.visible_servers();
        if self.mcp.directory.loading && self.mcp.directory.servers.is_empty() {
            body = body.child(self.manage_state_card("正在读取 MCP 服务器…", theme));
        } else if servers.is_empty() && self.mcp.directory.error.is_none() {
            let message = if self.mcp.query.trim().is_empty() {
                "没有配置 MCP 服务器"
            } else {
                "没有匹配的 MCP 服务器"
            };
            body = body.child(self.manage_state_card(message, theme));
        }

        let (configured, plugin_owned): (Vec<_>, Vec<_>) = servers
            .into_iter()
            .partition(|server| server.plugin_id.is_none());

        if !configured.is_empty() {
            body = body.child(self.mcp_section("服务器", &configured, theme, cx));
        }
        if !plugin_owned.is_empty() {
            body = body.child(self.mcp_section("来自插件", &plugin_owned, theme, cx));
        }
        body.into_any_element()
    }

    fn mcp_toolbar(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .id("mcp-refresh")
                    .h(px(28.0))
                    .px(px(10.0))
                    .rounded(px(12.5))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_button)
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.refresh_mcp_servers(AgentMcpStatusDetail::Full, cx)
                    }))
                    .child(svg().path("icons/settings-refresh.svg").size(px(14.0)))
                    .child(if self.mcp.directory.loading {
                        "刷新中…"
                    } else {
                        "刷新"
                    }),
            )
            .child(
                div()
                    .id("mcp-reload")
                    .h(px(28.0))
                    .px(px(10.0))
                    .rounded(px(12.5))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_button)
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .when(self.mcp.directory.reloading, |button| button.opacity(0.5))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.reload_mcp_servers(cx)))
                    .child(if self.mcp.directory.reloading {
                        "重新加载中…"
                    } else {
                        "重新加载"
                    }),
            )
    }

    fn mcp_section(
        &self,
        title: &str,
        servers: &[&AgentMcpServerStatus],
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Chromium resolves the dark management outline one quantization
        // step brighter than the general shell divider (#35 over #23).
        let section_border = if theme.surface == gpui::rgba(0x181818ff) {
            gpui::rgba(0xffffff15)
        } else {
            theme.border
        };
        let mut rows = div()
            .relative()
            .left(px(1.0))
            // GPUI's border rasterizer needs a 16px logical radius to match
            // Chromium's 20px CSS corner at DPR 1 (the straight side and
            // top-row coverage then land on the same pixels).
            .rounded(px(16.0))
            .border_1()
            .border_color(section_border)
            .bg(theme.settings_panel)
            .overflow_hidden()
            .flex()
            .flex_col();
        for (index, server) in servers.iter().enumerate() {
            let name = server.name.clone();
            let status = server.runtime_status;
            let startup = self
                .mcp
                .directory
                .startup_for(self.mcp_thread_id.as_deref(), &server.name);
            // A runtime status is only reported once a thread has started the
            // server; the lifecycle notification covers the window before the
            // list reflects it.
            let status = status
                .or_else(|| startup.map(|status| status.state.connection_status()))
                .unwrap_or(AgentMcpServerConnectionStatus::NotStarted);
            let error = startup.and_then(|status| status.error.clone());
            let auth = server.auth_status;
            // The list keeps the reference's row layout; a status line only
            // appears when the server reports something other than healthy, so
            // failures stay visible instead of being hidden behind a detail
            // panel the user may never open.
            let status_line = match status {
                AgentMcpServerConnectionStatus::Connected
                | AgentMcpServerConnectionStatus::NotStarted
                | AgentMcpServerConnectionStatus::Disabled => None,
                other => Some(format!(
                    "{} · {} 个工具 · {}",
                    other.label(),
                    server.tools.len(),
                    auth.label()
                )),
            };
            let hovered = self.mcp.hover_row.as_deref() == Some(name.as_str());
            let enabled = self
                .mcp
                .enabled_overrides
                .get(&name)
                .copied()
                .unwrap_or(!matches!(status, AgentMcpServerConnectionStatus::Disabled));
            let configured = server.plugin_id.is_none();
            let primary_text = if theme.surface == gpui::rgba(0x181818ff) {
                gpui::rgba(0xffffffff)
            } else {
                theme.text
            };
            let row_height = if configured { 52.0 } else { 42.0 };
            rows = rows.child(
                div().id(("mcp-row", index)).flex().flex_col().child(
                    div()
                        .h(px(row_height))
                        .px(px(16.0))
                        .flex()
                        .items_center()
                        // The controls sit eight pixels apart in the
                        // reference row; keeping this gap at 8px also puts
                        // the 28px settings button on the same x-grid.
                        .gap(px(8.0))
                        .relative()
                        .when(index > 0, |row| {
                            // Draw separators as an overlay so they do
                            // not consume a pixel of the 52px row, just as
                            // the reference's absolutely-positioned rule.
                            row.child(
                                div()
                                    .absolute()
                                    // The web client insets its row rules by
                                    // the same 16px cell padding and places
                                    // them on the preceding row's baseline.
                                    .top(px(-1.0))
                                    .left(px(16.0))
                                    .right(px(16.0))
                                    .h(px(1.0))
                                    .bg(section_border),
                            )
                        })
                        .when(hovered, |row| row.bg(theme.settings_control))
                        // A capture must be independent of the OS cursor's
                        // last position. Explicit `--mcp-hover-row` still
                        // exercises the hover state when it is requested.
                        .when(!cfg!(feature = "screenshot"), |row| {
                            row.hover(|row| row.bg(theme.settings_control))
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                // The browser's text box starts on the
                                // half-pixel before GPUI's flex cell; shift
                                // only the text, leaving controls on-grid.
                                .relative()
                                .left(px(-1.0))
                                .flex()
                                .flex_col()
                                .gap(px(1.0))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .line_height(px(18.5714))
                                        .font_weight(gpui::FontWeight(500.0))
                                        .text_color(primary_text)
                                        .child(server.display_name()),
                                )
                                .when_some(status_line, |block, line| {
                                    block.child(
                                        div()
                                            .text_size(px(12.0))
                                            .line_height(px(18.0))
                                            .text_color(theme.settings_description)
                                            .child(line),
                                    )
                                }),
                        )
                        .when_some(error.clone(), |row, error| {
                            row.child(
                                div()
                                    .max_w(px(240.0))
                                    .text_size(px(12.0))
                                    .line_height(px(18.0))
                                    .text_color(theme.warning)
                                    .child(error),
                            )
                        })
                        .when(configured, |row| {
                            row.child(
                                div()
                                    .id(("mcp-detail", index))
                                    .role(gpui::Role::Button)
                                    .aria_label("设置")
                                    .size(px(28.0))
                                    .flex_none()
                                    .rounded(px(14.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|button| button.bg(theme.settings_button))
                                    .on_click(cx.listener({
                                        let name = name.clone();
                                        move |this, _, _, cx| {
                                            this.mcp.detail = Some(name.clone());
                                            cx.notify();
                                        }
                                    }))
                                    .child(
                                        svg()
                                            .path("icons/settings-mcp.svg")
                                            .size(px(16.0))
                                            .text_color(theme.text_tertiary),
                                    ),
                            )
                            .child(self.mcp_switch(index, &name, enabled, theme, cx))
                        }),
                ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(0.0))
            .child(
                div()
                    .h(px(46.0))
                    .pb(px(6.0))
                    .flex()
                    .items_center()
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(title.to_owned()),
            )
            .child(rows)
    }

    fn mcp_switch(
        &self,
        index: usize,
        name: &str,
        checked: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let name = name.to_owned();
        div()
            .id(("mcp-switch", index))
            .role(gpui::Role::Switch)
            .aria_label(if checked { "停用" } else { "启用" })
            .aria_toggled(if checked {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .w(px(32.0))
            .h(px(20.0))
            .p(px(2.0))
            .rounded_full()
            .flex()
            .items_center()
            .when(checked, |track| {
                // This is the opaque blue used by the reference switch
                // (rgb(58, 131, 247)), rather than the lighter settings
                // accent used by the catalog's generic controls.
                // The native Chromium capture quantizes this blue to
                // #4e82ef at DPR 1; use the same raster value for parity.
                track.justify_end().bg(gpui::rgba(0x4e82efff))
            })
            .when(!checked, |track| {
                track.justify_start().bg(theme.settings_switch_off)
            })
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                let current = this
                    .mcp
                    .enabled_overrides
                    .get(&name)
                    .copied()
                    .unwrap_or(checked);
                this.mcp.enabled_overrides.insert(name.clone(), !current);
                cx.notify();
            }))
            .child(
                div()
                    .size(px(16.0))
                    .rounded_full()
                    .bg(gpui::white())
                    .border_1()
                    .border_color(gpui::white()),
            )
    }

    fn mcp_detail_content(
        &self,
        name: &str,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(server) = self.mcp.directory.servers.get(name) else {
            return div()
                .mt(px(44.0))
                .child(self.manage_state_card("该服务器已不在列表中", theme))
                .into_any_element();
        };
        let startup = self
            .mcp
            .directory
            .startup_for(self.mcp_thread_id.as_deref(), name);
        let status = server
            .runtime_status
            .or_else(|| startup.map(|status| status.state.connection_status()))
            .unwrap_or(AgentMcpServerConnectionStatus::NotStarted);
        let login = self.mcp.login_for(name);

        let mut rows = div().flex().flex_col().gap(px(6.0));
        rows = rows.child(self.mcp_field("状态", status.label(), theme));
        rows = rows.child(self.mcp_field("认证", server.auth_status.label(), theme));
        if let Some(info) = &server.server_info {
            rows = rows.child(self.mcp_field(
                "服务",
                &format!(
                    "{} {}",
                    info.title.clone().unwrap_or_else(|| info.name.clone()),
                    info.version
                ),
                theme,
            ));
            if let Some(website) = &info.website_url {
                rows = rows.child(self.mcp_field("网站", website, theme));
            }
        }
        if let Some(startup) = startup
            && let Some(error) = &startup.error
        {
            rows = rows.child(self.mcp_field("错误", error, theme));
        }
        if let Some(plugin) = &server.plugin_id {
            rows = rows.child(self.mcp_field("来自插件", plugin, theme));
        }
        if let Some(tools_error) = &server.tools_error {
            rows = rows.child(self.mcp_field("工具发现", tools_error, theme));
        }

        let mut tools = div().flex().flex_col().gap(px(4.0));
        for tool in &server.tools {
            tools = tools.child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(12.0))
                    .bg(theme.settings_control)
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(19.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(tool.title.clone().unwrap_or_else(|| tool.name.clone())),
                    )
                    .when_some(tool.description.clone(), |card, description| {
                        card.child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(18.0))
                                .text_color(theme.settings_description)
                                .child(description),
                        )
                    }),
            );
        }

        let mut resources = div().flex().flex_col().gap(px(4.0));
        for resource in &server.resources {
            resources = resources.child(self.mcp_field(&resource.name, &resource.uri, theme));
        }
        for template in &server.resource_templates {
            resources =
                resources.child(self.mcp_field(&template.name, &template.uri_template, theme));
        }

        // Unknown extension fields stay visible instead of being dropped.
        let mut extensions = div().flex().flex_col().gap(px(4.0));
        for (key, value) in &server.extra {
            extensions = extensions.child(self.mcp_field(key, &value.to_string(), theme));
        }

        let mut actions = div().flex().items_center().gap(px(8.0));
        actions = actions.child(
            div()
                .id("mcp-detail-reload")
                .h(px(28.0))
                .px(px(12.0))
                .rounded(px(12.5))
                .border_1()
                .border_color(theme.border)
                .bg(theme.settings_button)
                .flex()
                .items_center()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| this.reload_mcp_servers(cx)))
                .child("重新加载"),
        );
        if server.auth_status.can_start_login()
            || matches!(server.auth_status, AgentMcpAuthStatus::OAuth)
            || login.is_some()
        {
            let mut modes = div().flex().items_center().gap(px(2.0));
            for (index, (raw, label)) in REGISTRATION_MODES.into_iter().enumerate() {
                let mode = AgentMcpOauthClientRegistration::parse(raw)
                    .unwrap_or(AgentMcpOauthClientRegistration::Auto);
                let selected = self.mcp.registration == mode;
                modes = modes.child(
                    div()
                        .id(("mcp-registration", index))
                        .h(px(24.0))
                        .px(px(8.0))
                        .rounded(px(12.0))
                        .flex()
                        .items_center()
                        .text_size(px(12.0))
                        .line_height(px(18.0))
                        .when(selected, |chip| chip.bg(theme.settings_button))
                        .when(!selected, |chip| chip.text_color(theme.text_tertiary))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.mcp.registration = mode;
                            cx.notify();
                        }))
                        .child(label),
                );
            }
            actions = actions.child(modes);
            let name = name.to_owned();
            actions = actions.child(
                div()
                    .id("mcp-detail-login")
                    .h(px(28.0))
                    .px(px(12.0))
                    .rounded(px(12.5))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_button)
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.start_mcp_login(name.clone(), cx)),
                    )
                    .child(if matches!(server.auth_status, AgentMcpAuthStatus::OAuth) {
                        "重新登录"
                    } else {
                        "登录"
                    }),
            );
        }

        div()
            .id("mcp-detail-scroll")
            .mt(px(44.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("mcp-detail-back")
                            .h(px(28.0))
                            .px(px(10.0))
                            .rounded(px(12.5))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .cursor_pointer()
                            .hover(|button| button.bg(theme.settings_button))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.mcp.detail = None;
                                cx.notify();
                            }))
                            .child(svg().path("icons/chevron-left.svg").size(px(14.0)))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .child("返回"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(server.display_name()),
                    ),
            )
            .child(actions)
            .child(rows)
            .when(!server.tools.is_empty(), |detail| {
                detail
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .child("工具"),
                    )
                    .child(tools)
            })
            .when(
                !server.resources.is_empty() || !server.resource_templates.is_empty(),
                |detail| {
                    detail
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .font_weight(gpui::FontWeight(500.0))
                                .child("资源"),
                        )
                        .child(resources)
                },
            )
            .when(!server.extra.is_empty(), |detail| {
                detail
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .child("服务端扩展字段"),
                    )
                    .child(extensions)
            })
            .into_any_element()
    }

    fn mcp_field(&self, label: &str, value: &str, theme: &Theme) -> impl IntoElement {
        div()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(12.0))
            .bg(theme.settings_control)
            .flex()
            .items_start()
            .gap(px(12.0))
            .child(
                div()
                    .w(px(120.0))
                    .flex_none()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(theme.text_tertiary)
                    .child(label.to_owned()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .child(value.to_owned()),
            )
    }

    /// The OAuth dialog is rendered above the settings shell so it is reachable
    /// from every segment, and Escape closes it by cancelling the login.
    pub(super) fn mcp_login_overlay(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let login = self
            .mcp
            .directory
            .logins
            .values()
            .max_by_key(|login| login.login_id)?;
        let login_id = login.login_id;
        let server_name = login.server_name.clone();
        let url = login.authorization_url.clone();
        let (title, detail, tone) = match &login.phase {
            McpLoginPhase::Waiting => (
                format!("连接 {server_name}"),
                "请在浏览器中完成授权，然后返回这里。".to_owned(),
                theme.text_tertiary,
            ),
            McpLoginPhase::Succeeded => (
                format!("{server_name} 已连接"),
                "授权已完成，服务器状态正在刷新。".to_owned(),
                theme.text_tertiary,
            ),
            McpLoginPhase::Failed(error) => {
                (login.phase.label().to_owned(), error.clone(), theme.warning)
            }
            McpLoginPhase::Cancelled => (
                "已取消登录".to_owned(),
                "未完成授权，服务器保持未登录状态。".to_owned(),
                theme.text_tertiary,
            ),
            McpLoginPhase::Interrupted(error) => {
                (login.phase.label().to_owned(), error.clone(), theme.warning)
            }
        };
        let waiting = login.phase.busy();
        Some(
            div()
                .id("mcp-login-overlay")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::rgba(0x00000038))
                .child(
                    div()
                        .w(px(420.0))
                        .p(px(20.0))
                        .rounded(px(20.0))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.settings_panel)
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .child(
                            div()
                                .text_size(px(15.0))
                                .line_height(px(22.0))
                                .font_weight(gpui::FontWeight(500.0))
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(19.0))
                                .text_color(tone)
                                .child(detail),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(18.0))
                                .text_color(theme.text_tertiary)
                                .child(url.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .id("mcp-login-open")
                                        .h(px(28.0))
                                        .px(px(12.0))
                                        .rounded(px(12.5))
                                        .border_1()
                                        .border_color(theme.border)
                                        .bg(theme.settings_button)
                                        .flex()
                                        .items_center()
                                        .text_size(px(13.0))
                                        .line_height(px(18.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |_, _, _, cx| {
                                            cx.open_url(&url);
                                        }))
                                        .child("打开授权地址"),
                                )
                                .child(
                                    div()
                                        .id("mcp-login-close")
                                        .h(px(28.0))
                                        .px(px(12.0))
                                        .rounded(px(12.5))
                                        .flex()
                                        .items_center()
                                        .text_size(px(13.0))
                                        .line_height(px(18.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.cancel_mcp_login(login_id, cx)
                                        }))
                                        .child(if waiting { "取消登录" } else { "关闭" }),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
