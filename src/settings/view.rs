mod agent;
mod appearance;
mod appshots;
mod artwork;
mod browser;
mod chronicle;
mod computer_use;
mod configuration;
mod connections;
mod controls;
mod data_controls;
mod environments;
mod git;
mod hooks;
mod keyboard;
mod navigation;
mod personalization;
mod plugins;
mod plugins_actions;
mod plugins_catalog;
mod plugins_mcp;
mod plugins_skills;
mod profile;
mod worktrees;

use std::collections::HashMap;

use gpui::{
    Context, EventEmitter, IntoElement, Render, ScrollHandle, Window, div, point, prelude::*, px,
    svg,
};

use super::{PageKind, PageSpec, page, pages};
use crate::theme::{Theme, ThemeMode, UI_FONT_FAMILY};

pub struct CloseSettings;
pub struct ChangeTheme(pub ThemeMode);
pub struct ConfigSaveFinished;

/// Segment of the plugins settings page. Plugins and apps keep the reference
/// catalog rendering; MCP servers and skills are backed by the backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PluginSegment {
    Plugins,
    Apps,
    Mcp,
    Skills,
}

pub struct SettingsView {
    mode: ThemeMode,
    selected: &'static str,
    nav_scroll: ScrollHandle,
    content_scroll: ScrollHandle,
    switch_overrides: HashMap<(&'static str, usize, usize), bool>,
    appearance_theme: usize,
    backend: std::sync::Arc<dyn crate::agent::AgentBackend>,
    config_cwd: std::path::PathBuf,
    config_editor: crate::configuration::ConfigEditor,
    config_drafts: configuration::ConfigDrafts,
    config_choices: Vec<crate::agent::AgentConfigChoiceSet>,
    config_profiles: Vec<crate::agent::AgentPermissionProfile>,
    config_profiles_error: Option<String>,
    config_models: Vec<crate::agent::AgentModel>,
    config_menu: Option<String>,
    config_menu_index: usize,
    config_menu_scroll: ScrollHandle,
    config_control_bounds:
        std::rc::Rc<std::cell::RefCell<HashMap<String, gpui::Bounds<gpui::Pixels>>>>,
    config_menu_focus: gpui::FocusHandle,
    config_menu_focus_pending: bool,
    config_field_focus: HashMap<String, gpui::FocusHandle>,
    config_return_focus: Option<String>,
    config_sources_open: bool,
    config_advanced_open: bool,
    config_custom_key: Option<String>,
    config_input: gpui::Entity<crate::components::prompt_input::PromptInput>,
    plugins_segment: PluginSegment,
    /// Catalog and install lifecycle of the plugins segment.
    plugins_catalog: plugins_catalog::PluginsPanel,
    /// App directory and installed connector snapshot.
    apps: plugins_catalog::AppsPanel,
    /// Working directory the directory reads are scoped to.
    plugins_cwd: Option<std::path::PathBuf>,
    /// Search field of the plugins segment; its submitted text is the query.
    plugin_search_input: gpui::Entity<crate::components::prompt_input::PromptInput>,
    /// Marketplace source field of the add sheet.
    marketplace_source_input: gpui::Entity<crate::components::prompt_input::PromptInput>,
    /// Connection generation the catalog and app reads belong to.
    plugins_generation: u64,
    apps_generation: u64,
    mcp: plugins_mcp::McpPanel,
    skills: plugins_skills::SkillsPanel,
    /// Last connection generation reported for skills and MCP.
    skills_generation: u64,
    mcp_generation: u64,
    /// Active conversation thread, used for thread scoped MCP runtime status.
    mcp_thread_id: Option<String>,
}

impl EventEmitter<CloseSettings> for SettingsView {}
impl EventEmitter<ChangeTheme> for SettingsView {}
impl EventEmitter<ConfigSaveFinished> for SettingsView {}

impl SettingsView {
    pub fn new(
        mode: ThemeMode,
        backend: std::sync::Arc<dyn crate::agent::AgentBackend>,
        cx: &mut Context<Self>,
    ) -> Self {
        let config_choices = backend.config_choices();
        let config_field_focus = config_choices
            .iter()
            .map(|field| field.key.clone())
            .chain(std::iter::once("source".into()))
            .map(|key| (key, cx.focus_handle().tab_stop(true)))
            .collect();
        let config_input = cx.new(|cx| {
            let mut input = crate::components::prompt_input::PromptInput::inline_other(
                mode,
                "输入配置值",
                false,
                cx,
            );
            input.set_accessible_name("配置值");
            input
        });
        let plugin_search_input = cx.new(|cx| {
            let mut input = crate::components::prompt_input::PromptInput::inline_other(
                mode,
                "搜索插件",
                false,
                cx,
            );
            input.set_accessible_name("搜索插件");
            input
        });
        cx.subscribe(
            &plugin_search_input,
            |this, _, event: &crate::components::prompt_input::PromptSubmitted, cx| {
                let term = event.0.clone();
                this.plugins_catalog.directory.set_search_term(term);
                this.submit_plugin_search(cx);
            },
        )
        .detach();
        let marketplace_source_input = cx.new(|cx| {
            let mut input = crate::components::prompt_input::PromptInput::inline_other(
                mode,
                "git URL 或本地路径",
                false,
                cx,
            );
            input.set_accessible_name("marketplace 来源");
            input
        });
        cx.subscribe(
            &marketplace_source_input,
            |this, _, event: &crate::components::prompt_input::PromptSubmitted, cx| {
                this.confirm_marketplace_add(event.0.clone(), cx);
            },
        )
        .detach();
        cx.subscribe(
            &config_input,
            |this, _, event: &crate::components::prompt_input::PromptSubmitted, cx| {
                if let Some(key) = this.config_custom_key.clone() {
                    let value = event.0.trim();
                    if !value.is_empty() {
                        match this.config_editor.edit(&key, serde_json::json!(value)) {
                            Ok(()) => {
                                this.config_custom_key = None;
                                this.config_return_focus = Some(key.clone());
                            }
                            Err(error) => this.config_editor.feedback = Some(error),
                        }
                        cx.notify();
                    }
                }
            },
        )
        .detach();
        // Skills invalidation, MCP startup status and OAuth completions are
        // connection scoped. Each is stamped with its generation so a retired
        // connection can never update this surface.
        let connection_events = backend.subscribe_connection_events();
        cx.spawn(async move |this, cx| {
            while let Ok(event) = connection_events.recv().await {
                let keep_going = this
                    .update(cx, |this, cx| match &event {
                        crate::agent::AgentConnectionEvent::SkillsChanged { generation } => {
                            this.apply_skills_changed(*generation, cx);
                            true
                        }
                        crate::agent::AgentConnectionEvent::McpServerStartupStatusUpdated(
                            updated,
                        ) => {
                            this.apply_mcp_startup(updated, cx);
                            true
                        }
                        crate::agent::AgentConnectionEvent::McpOauthLoginCompleted(completion) => {
                            this.apply_mcp_login_completion(completion, cx);
                            true
                        }
                        // The app catalog notification is an invalidation
                        // signal: it marks the cached directory stale and
                        // re-reads only while the apps segment is visible.
                        crate::agent::AgentConnectionEvent::AppListUpdated { generation } => {
                            this.apps_generation = *generation;
                            this.apply_app_list_updated(cx);
                            true
                        }
                        _ => true,
                    })
                    .is_ok();
                if !keep_going {
                    return;
                }
            }
        })
        .detach();
        Self {
            config_choices,
            config_field_focus,
            config_return_focus: None,
            backend,
            config_cwd: std::env::current_dir().unwrap_or_default(),
            config_editor: Default::default(),
            config_drafts: Default::default(),
            config_profiles: Vec::new(),
            config_profiles_error: None,
            config_models: Vec::new(),
            config_menu: None,
            config_menu_index: 0,
            config_menu_scroll: ScrollHandle::new(),
            config_control_bounds: Default::default(),
            config_menu_focus: cx.focus_handle(),
            config_menu_focus_pending: false,
            config_sources_open: false,
            config_advanced_open: false,
            config_custom_key: None,
            config_input,
            plugins_catalog: Default::default(),
            apps: Default::default(),
            plugins_cwd: None,
            plugins_generation: 0,
            plugin_search_input,
            marketplace_source_input,
            apps_generation: 0,
            plugins_segment: PluginSegment::Plugins,
            mcp: plugins_mcp::McpPanel::default(),
            skills: plugins_skills::SkillsPanel::default(),
            skills_generation: 0,
            mcp_generation: 0,
            mcp_thread_id: None,
            mode,
            selected: "general-settings",
            nav_scroll: ScrollHandle::new(),
            content_scroll: ScrollHandle::new(),
            switch_overrides: HashMap::new(),
            appearance_theme: if mode == ThemeMode::Dark { 2 } else { 1 },
        }
    }

    pub fn select(&mut self, slug: &'static str, cx: &mut Context<Self>) {
        self.selected = slug;
        self.config_menu = None;
        if matches!(slug, "agent" | "personalization") && self.config_editor.snapshot.is_none() {
            self.set_config_context(self.config_cwd.clone(), cx);
        }
        self.content_scroll.set_offset(point(px(0.0), px(0.0)));
        self.ensure_plugins_segment_loaded(cx);
        cx.notify();
    }

    /// Passes the active conversation so thread scoped MCP status can be shown
    /// without pretending the application scope is the same thing.
    pub fn set_manage_context(&mut self, thread_id: Option<String>, cx: &mut Context<Self>) {
        if self.mcp_thread_id == thread_id {
            return;
        }
        self.mcp_thread_id = thread_id;
        self.mcp.directory.startup.clear();
        cx.notify();
    }

    /// Loads whatever the selected plugins segment needs. Called when the page
    /// becomes visible so the segments never show stale data silently.
    pub fn ensure_plugins_segment_loaded(&mut self, cx: &mut Context<Self>) {
        match self.plugins_segment {
            PluginSegment::Mcp => {
                if self.mcp.directory.servers.is_empty() && !self.mcp.directory.loading {
                    self.refresh_mcp_servers(crate::agent::AgentMcpStatusDetail::Full, cx);
                }
            }
            PluginSegment::Skills => {
                if self.skills.directory.snapshot.is_none() && !self.skills.directory.loading {
                    self.refresh_skills(false, cx);
                }
            }
            PluginSegment::Plugins => {
                if self.plugins_catalog.directory.catalog.is_none()
                    && !self.plugins_catalog.directory.loading
                {
                    self.refresh_plugins(false, cx);
                }
            }
            PluginSegment::Apps => {
                if self.apps.directory.page.is_none() && !self.apps.directory.loading {
                    self.refresh_apps(cx);
                }
            }
        }
    }

    /// True once the visible plugins segment has finished its first backend
    /// read (or reported an error it can display).
    #[cfg(feature = "screenshot")]
    pub fn plugins_capture_ready(&self) -> bool {
        match self.plugins_segment {
            PluginSegment::Mcp => {
                !self.mcp.directory.loading
                    && (!self.mcp.directory.servers.is_empty()
                        || self.mcp.directory.error.is_some())
            }
            PluginSegment::Skills => {
                !self.skills.directory.loading
                    && (self.skills.directory.snapshot.is_some()
                        || self.skills.directory.error.is_some())
            }
            PluginSegment::Plugins => {
                self.plugins_catalog.directory.catalog.is_some()
                    || self.plugins_catalog.directory.error.is_some()
            }
            PluginSegment::Apps => self.apps.directory.resolved(),
        }
    }

    pub(super) fn select_plugins_segment(
        &mut self,
        segment: PluginSegment,
        cx: &mut Context<Self>,
    ) {
        if self.plugins_segment == segment {
            return;
        }
        self.plugins_segment = segment;
        // Leaving a segment drops the detail it was showing: the read belongs to
        // the list the user was looking at, not to the new one.
        if segment != PluginSegment::Plugins {
            self.plugins_catalog.directory.clear_detail();
            self.plugins_catalog.open_plugin = None;
        }
        if segment != PluginSegment::Apps {
            self.apps.directory.clear_detail();
        }
        self.ensure_plugins_segment_loaded(cx);
        cx.notify();
    }

    fn content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match page.kind {
            PageKind::Profile => self.profile_content(theme, viewport_width),
            PageKind::KeyboardShortcuts => self.keyboard_content(page, theme, cx),
            _ if page.slug == "appearance" => self.appearance_content(page, theme, cx),
            _ if page.slug == "appshots" => self.appshots_content(page, theme, cx),
            _ if page.slug == "computer-use" => self.computer_use_content(page, theme, cx),
            _ if page.slug == "personalization" => self.personalization_content(page, theme, cx),
            _ if page.slug == "chronicle" => self.chronicle_content(page, theme),
            _ if page.slug == "plugins-settings" => self.plugins_content(page, theme, cx),
            _ if page.slug == "hooks-settings" => self.hooks_content(page, theme),
            _ if page.slug == "connections" => self.connections_content(page, theme, cx),
            _ if page.slug == "browser-use" => self.browser_content(page, theme, cx),
            _ if page.slug == "agent" => self.agent_content(page, theme, cx),
            _ if page.slug == "git-settings" => self.git_content(page, theme, cx),
            _ if page.slug == "local-environments" => self.local_environments_content(page, theme),
            _ if page.slug == "worktrees" => self.worktrees_content(page, theme, cx),
            _ if page.slug == "data-controls" => self.data_controls_content(page, theme),
            PageKind::Standard => self.standard_content(page, theme, cx).into_any_element(),
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.config_menu_focus_pending {
            if self.config_menu.is_some() {
                self.config_menu_focus.focus(window, cx);
            }
            self.config_menu_focus_pending = false;
        }
        if let Some(key) = self.config_return_focus.take()
            && let Some(focus) = self.config_field_focus.get(&key)
        {
            focus.focus(window, cx);
        }
        let viewport = window.viewport_size();
        let theme = Theme::for_window(
            self.mode,
            window.is_window_active(),
            f32::from(viewport.width),
            f32::from(viewport.height),
            window.scale_factor(),
        );
        let viewport_width = f32::from(window.viewport_size().width);
        let selected =
            page(self.selected).unwrap_or_else(|| pages().next().expect("settings pages"));
        let nav_scroll = self.nav_scroll.clone();
        let content_scroll = self.content_scroll.clone();

        div()
            .id("settings-shell")
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab" {
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                } else if event.keystroke.modifiers.platform && event.keystroke.key == "s" {
                    this.save_config(cx);
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" {
                    this.config_menu = None;
                    this.config_custom_key = None;
                    if !this.dismiss_manage_overlays(cx) {
                        cx.notify();
                    }
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .size_full()
            .bg(theme.surface)
            .font_family(UI_FONT_FAMILY)
            .text_color(theme.markdown_text)
            .relative()
            .flex()
            .child(
                div()
                    .id("settings-sidebar")
                    .w(px(240.0))
                    .h_full()
                    .flex_none()
                    .relative()
                    // ChatGPT's settings shell resolves its translucent
                    // material to these opaque surfaces in the reference
                    // capture. Keep the conversation sidebar theme separate.
                    .bg(match self.mode {
                        ThemeMode::Light => gpui::rgba(0xfdfdfdff),
                        ThemeMode::Dark => gpui::rgba(0x222222ff),
                    })
                    .border_r_1()
                    // The reference shell has no contrasting divider at the
                    // settings rail edge; keep the box but paint it with the
                    // same opaque rail surface.
                    .border_color(match self.mode {
                        ThemeMode::Light => gpui::rgba(0xfdfdfdff),
                        ThemeMode::Dark => gpui::rgba(0x222222ff),
                    })
                    .flex()
                    .flex_col()
                    .child(div().h(px(46.0)).flex_none())
                    .child(
                        div()
                            .id("settings-back")
                            .focus_visible(move |style| {
                                style.bg(theme.sidebar_hover).shadow(vec![
                                    gpui::BoxShadow::new(px(0.), px(0.), theme.accent.into())
                                        .spread_radius(px(2.)),
                                ])
                            })
                            .role(gpui::Role::Button)
                            .aria_label("返回应用")
                            .focusable()
                            .tab_stop(true)
                            .on_key_down(cx.listener(|_, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    cx.emit(CloseSettings);
                                    cx.stop_propagation();
                                } else {
                                    cx.propagate();
                                }
                            }))
                            .mx(px(8.0))
                            .mb(px(8.0))
                            .h(px(31.0))
                            .px(px(8.0))
                            .rounded(px(12.5))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .text_color(theme.settings_description)
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(CloseSettings)))
                            .child(
                                svg()
                                    .path("icons/back.svg")
                                    .size(px(16.0))
                                    .text_color(theme.settings_description),
                            )
                            .child("返回应用"),
                    )
                    .child(
                        div()
                            .mx(px(8.0))
                            // The reference leaves 16px between the search
                            // field and the first navigation group.
                            .mb(px(16.0))
                            .h(px(29.0))
                            .px(px(8.0))
                            .rounded(px(12.5))
                            .bg(theme.settings_search)
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .child(
                                svg()
                                    .path("icons/search.svg")
                                    .size(px(18.0))
                                    .text_color(theme.text_tertiary),
                            )
                            .child("搜索设置…"),
                    )
                    .child(
                        div()
                            .id("settings-nav-scroll")
                            .min_h(px(0.0))
                            .flex_1()
                            .overflow_y_scroll()
                            .scrollbar_width(px(0.0))
                            .track_scroll(&nav_scroll)
                            .pl(px(8.0))
                            // Reserve the 15px rail occupied by ChatGPT's
                            // slim native scrollbar. It is mostly hidden in
                            // the screenshot, but it reduces each row's hit
                            // rectangle from 224px to 209px.
                            .pr(px(23.0))
                            .pt(px(1.0))
                            .pb(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .child(self.nav_group(
                                "个人",
                                &[
                                    "general-settings",
                                    "profile",
                                    "appearance",
                                    "voice",
                                    "agent",
                                    "personalization",
                                    "keyboard-shortcuts",
                                ],
                                theme,
                                cx,
                            ))
                            .child(self.nav_group(
                                "集成",
                                &[
                                    "computer-use",
                                    "chronicle",
                                    "appshots",
                                    "plugins-settings",
                                    "browser-use",
                                ],
                                theme,
                                cx,
                            ))
                            .child(self.nav_group(
                                "编码",
                                &[
                                    "hooks-settings",
                                    "connections",
                                    "git-settings",
                                    "local-environments",
                                    "worktrees",
                                ],
                                theme,
                                cx,
                            ))
                            .child(self.nav_group("已归档", &["data-controls"], theme, cx)),
                    )
                    .child(Self::sidebar_edge_shade(theme, &nav_scroll)),
            )
            .child(
                div()
                    .id("settings-content-scroll")
                    .min_w(px(0.0))
                    .h_full()
                    .flex_1()
                    .bg(theme.surface)
                    .overflow_y_scroll()
                    .track_scroll(&content_scroll)
                    .pl(px(40.0))
                    .pr(px(55.0))
                    .child(self.content(selected, theme, viewport_width, cx)),
            )
            .children(
                (selected.slug == "plugins-settings")
                    .then(|| self.mcp_login_overlay(&theme, cx))
                    .flatten(),
            )
    }
}
