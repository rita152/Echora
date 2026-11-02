//! Sources and selection for the add-context picker. Every async result belongs
//! to one opening, working directory and connection generation.
use super::{
    ComposerView, ConversationChanged,
    slash_menu::{SlashCommand, fuzzy},
    toast::ToastKind,
};
use crate::agent::{
    AgentFileSearchResult, AgentFileSearchSession, AgentFileSearchSessionEvent,
    AgentInputReference, AgentInputReferenceKind, AgentPluginInstalledRequest, AgentPluginSummary,
    ProjectId, ThreadListRequest, ThreadSearchResult,
};
use gpui::{Context, ScrollHandle, point, px};
use std::{ops::Range, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextBrowserTab {
    pub id: u64,
    pub title: String,
    pub url: String,
}
pub struct ContextSourcesRequested;
pub struct AttachBrowserContext(pub u64);
impl gpui::EventEmitter<ContextSourcesRequested> for ComposerView {}
impl gpui::EventEmitter<AttachBrowserContext> for ComposerView {}

#[derive(Default)]
pub(super) struct ContextCatalog {
    pub mention_start: Option<usize>,
    pub mention_dismissed: Option<usize>,
    pub cycle: u64,
    pub start: usize,
    pub query: String,
    pub highlighted: usize,
    pub project: bool,
    pub plugins: Vec<AgentPluginSummary>,
    pub loading: bool,
    pub error: Option<String>,
    pub browser: Vec<ContextBrowserTab>,
    pub files: Vec<AgentFileSearchResult>,
    pub threads: Vec<ThreadSearchResult>,
    pub search_cycle: u64,
    pub searching: bool,
    pub search_error: Option<String>,
    pub session: Option<AgentFileSearchSession>,
    pub scroll: ScrollHandle,
    pub trigger_bounds: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
}
impl ContextCatalog {
    pub fn cancel(&mut self) {
        self.cycle = self.cycle.wrapping_add(1);
        self.search_cycle = self.search_cycle.wrapping_add(1);
        if let Some(session) = self.session.take() {
            let _ = session.stop();
        }
        self.searching = false;
    }
}
impl Drop for ContextCatalog {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Clone)]
pub(super) enum ContextAction {
    Files,
    Command(SlashCommand),
    Project(Option<ProjectId>),
    Plugin(usize, u64),
    Skill(usize, u64),
    Browser(u64),
    File(PathBuf),
    Thread(String, String),
    Retry,
}
#[derive(Clone)]
pub(super) struct ContextRow {
    pub action: ContextAction,
    pub title: String,
    pub description: String,
    pub icon: &'static str,
    pub artwork: Option<String>,
    pub group: &'static str,
}
impl ComposerView {
    pub(crate) fn draft_identity(&self) -> (u64, u64, u64) {
        (
            self.draft_revision,
            self.conversation.runtime.generation,
            self.context_catalog.cycle,
        )
    }
    pub(crate) fn set_context_browser_tabs(
        &mut self,
        tabs: Vec<ContextBrowserTab>,
        cx: &mut Context<Self>,
    ) {
        self.context_catalog.browser = tabs;
        cx.notify();
    }
    pub(super) fn dismiss_context(&mut self, cx: &mut Context<Self>) {
        self.context_catalog.mention_dismissed = self.context_catalog.mention_start;
        self.context_menu_open = false;
        self.context_catalog.cancel();
        self.focus_prompt_pending = true;
        cx.notify();
    }
    /// Closes the context menu and forgets the `@` mention it belonged to.
    pub(super) fn reset_context_menu(&mut self) {
        self.context_menu_open = false;
        self.context_catalog.cancel();
        self.context_catalog.mention_start = None;
        self.context_catalog.mention_dismissed = None;
    }
    pub(super) fn load_context_catalog(&mut self, cx: &mut Context<Self>) {
        self.context_catalog.cancel();
        self.context_catalog.mention_start = None;
        self.context_catalog.plugins.clear();
        self.context_catalog.loading = true;
        self.context_catalog.error = None;
        self.context_catalog.files.clear();
        self.context_catalog.threads.clear();
        self.context_catalog.search_error = None;
        self.context_catalog.project = false;
        self.context_catalog.highlighted = 0;
        self.context_catalog.query.clear();
        self.context_catalog.start = self.prompt_editor.read(cx).cursor();
        self.context_catalog
            .scroll
            .set_offset(point(px(0.), px(0.)));
        self.ensure_command_skills(cx);
        cx.emit(ContextSourcesRequested);
        let cycle = self.context_catalog.cycle;
        let cwd = self.conversation.cwd.clone();
        let generation = self.conversation.runtime.generation;
        let response = self
            .backend
            .load_installed_plugins(AgentPluginInstalledRequest {
                cwds: Some(vec![cwd.clone()]),
                install_suggestion_plugin_names: None,
            });
        cx.spawn(async move |this,cx| {
            let result = response.recv().await;
            let _ = this.update(cx, |this,cx| {
                if !this.context_menu_open || this.context_catalog.cycle != cycle || this.conversation.cwd != cwd || this.conversation.runtime.generation != generation { return; }
                this.context_catalog.loading = false;
                match result {
                    Ok(Ok(catalog)) if catalog.generation >= generation => {
                        let mut seen = std::collections::HashSet::new();
                        this.context_catalog.plugins = catalog.plugins().filter(|(_,p)| p.installed && p.enabled && p.disabled_reason.is_none() && seen.insert(p.id.clone())).map(|(_,p)|p.clone()).collect();
                        this.context_catalog.error = (!catalog.marketplace_load_errors.is_empty()).then(||catalog.marketplace_load_errors.iter().map(|e|e.message.clone()).collect::<Vec<_>>().join("\n"));
                    }
                    Ok(Ok(_)) => this.context_catalog.error = Some(crate::i18n::format!("插件目录已过期，请重试"=>"Plugin catalog expired; retry")),
                    Ok(Err(e)) => this.context_catalog.error = Some(e.message),
                    Err(e) => this.context_catalog.error = Some(e.to_string()),
                }
                cx.notify();
            });
        }).detach();
    }
    pub(super) fn context_range(&self, cx: &gpui::App) -> Option<Range<usize>> {
        let editor = self.prompt_editor.read(cx);
        let range = self
            .context_catalog
            .mention_start
            .unwrap_or(self.context_catalog.start)..editor.cursor();
        editor
            .text()
            .get(range.clone())
            .filter(|s| !s.contains('\n'))
            .map(|_| range)
    }
    pub(super) fn update_context_query(&mut self, cx: &mut Context<Self>) {
        if !self.context_menu_open {
            return;
        }
        let Some(range) = self.context_range(cx) else {
            self.dismiss_context(cx);
            return;
        };
        let query_start = range.start + usize::from(self.context_catalog.mention_start.is_some());
        let Some(query) = self
            .prompt_editor
            .read(cx)
            .text()
            .get(query_start..range.end)
            .map(str::to_owned)
        else {
            self.dismiss_context(cx);
            return;
        };
        if query == self.context_catalog.query {
            return;
        }
        self.context_catalog.query = query;
        self.context_catalog.highlighted = 0;
        self.context_catalog
            .scroll
            .set_offset(point(px(0.), px(0.)));
        self.search_context(cx);
    }
    fn search_context(&mut self, cx: &mut Context<Self>) {
        self.context_catalog.search_cycle = self.context_catalog.search_cycle.wrapping_add(1);
        let search_cycle = self.context_catalog.search_cycle;
        self.context_catalog.files.clear();
        self.context_catalog.threads.clear();
        self.context_catalog.search_error = None;
        if let Some(session) = self.context_catalog.session.take() {
            let _ = session.stop();
        }
        let query = self.context_catalog.query.trim().to_owned();
        self.context_catalog.searching = !query.is_empty() && !self.context_catalog.project;
        if !self.context_catalog.searching {
            return;
        }
        let cwd = self.conversation.cwd.clone();
        let cycle = self.context_catalog.cycle;
        let request = self.backend.search_threads(ThreadListRequest {
            search_term: Some(query.clone()),
            page: crate::agent::PageRequest {
                limit: 8,
                ..Default::default()
            },
            ..Default::default()
        });
        cx.spawn(async move |this, cx| {
            let result = request.recv().await;
            let _ = this.update(cx, |this, cx| {
                if !this.context_menu_open
                    || this.context_catalog.cycle != cycle
                    || this.context_catalog.search_cycle != search_cycle
                {
                    return;
                }
                match result {
                    Ok(Ok(page)) => this.context_catalog.threads = page.data,
                    Ok(Err(e)) => {
                        this.context_catalog.search_error =
                            Some(e.user_message(crate::i18n::text("聊天搜索")))
                    }
                    Err(e) => this.context_catalog.search_error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        let response = self
            .backend
            .open_file_search_session(vec![cwd.display().to_string()]);
        cx.spawn(async move |this, cx| {
            let session = match response
                .recv()
                .await
                .map_err(|e| e.to_string())
                .and_then(|session| session)
            {
                Ok(session) => session,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        if this.context_catalog.cycle == cycle
                            && this.context_catalog.search_cycle == search_cycle
                        {
                            this.context_catalog.searching = false;
                            this.context_catalog.search_error = Some(error);
                            cx.notify();
                        }
                    });
                    return;
                }
            };
            let events = session.updates();
            let stored = session.clone();
            let keep = this
                .update(cx, |this, cx| {
                    if !this.context_menu_open
                        || this.context_catalog.cycle != cycle
                        || this.context_catalog.search_cycle != search_cycle
                        || this.conversation.cwd != cwd
                    {
                        return false;
                    }
                    if let Err(error) = stored.update_query(query) {
                        this.context_catalog.search_error = Some(error);
                        this.context_catalog.searching = false;
                        cx.notify();
                        return false;
                    }
                    this.context_catalog.session = Some(stored);
                    true
                })
                .unwrap_or(false);
            if !keep {
                let _ = session.stop();
                return;
            }
            while let Ok(event) = events.recv().await {
                if !this
                    .update(cx, |this, cx| {
                        if !this.context_menu_open
                            || this.context_catalog.cycle != cycle
                            || this.context_catalog.search_cycle != search_cycle
                            || this.conversation.cwd != cwd
                        {
                            return false;
                        }
                        match event {
                            AgentFileSearchSessionEvent::Updated(update)
                                if update.query == this.context_catalog.query.trim() =>
                            {
                                this.context_catalog.files = update.files
                            }
                            AgentFileSearchSessionEvent::Failed(error) => {
                                this.context_catalog.search_error = Some(error);
                                this.context_catalog.searching = false;
                            }
                            AgentFileSearchSessionEvent::Completed(_) => {
                                this.context_catalog.searching = false
                            }
                            _ => {}
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
            let _ = session.stop();
        })
        .detach();
    }
    pub(super) fn context_rows(&self, _cx: &gpui::App) -> Vec<ContextRow> {
        let add = crate::i18n::text("添加");
        let normalized_query = self.context_catalog.query.trim().to_lowercase();
        let query = normalized_query.as_str();
        let mut rows = Vec::new();
        if self.context_catalog.project {
            rows.push(ContextRow {
                action: ContextAction::Project(None),
                title: crate::i18n::format!("无"=>"None"),
                description: crate::i18n::format!("不在任何项目中工作"=>"Don't work in a project"),
                icon: "utility-folder",
                artwork: None,
                group: crate::i18n::text("项目"),
            });
            rows.extend(self.project_picker.projects.iter().map(|p| {
                ContextRow {
                    action: ContextAction::Project(Some(p.project_id.clone())),
                    title: p.name.clone(),
                    description: p
                        .roots
                        .first()
                        .map_or_else(String::new, |p| p.display().to_string()),
                    icon: "utility-folder",
                    artwork: None,
                    group: crate::i18n::text("项目"),
                }
            }));
        } else {
            rows.push(ContextRow {
                action: ContextAction::Files,
                title: crate::i18n::format!("文件和文件夹"=>"Files and folders"),
                description: String::new(),
                icon: "paperclip",
                artwork: None,
                group: add,
            });
            for command in [
                SlashCommand::Project,
                SlashCommand::Goal,
                SlashCommand::PlanMode,
                SlashCommand::Sketch,
            ] {
                if self.command_available(command) {
                    rows.push(ContextRow {
                        action: ContextAction::Command(command),
                        title: command.title(),
                        description: self.command_description(command),
                        icon: command.icon(),
                        artwork: None,
                        group: add,
                    });
                }
            }
            for (index, plugin) in self.context_catalog.plugins.iter().enumerate() {
                rows.push(ContextRow {
                    action: ContextAction::Plugin(index, self.context_catalog.cycle),
                    title: plugin.display_name().into(),
                    description: plugin.description().unwrap_or_default().into(),
                    icon: "command-init",
                    artwork: plugin.interface.as_ref().and_then(|i| {
                        i.composer_icon
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .or_else(|| i.composer_icon_url.clone())
                            .or_else(|| {
                                i.logo_source(self.mode == crate::theme::ThemeMode::Dark)
                                    .map(str::to_owned)
                            })
                    }),
                    group: crate::i18n::text("插件"),
                });
            }
            for (index, skill) in self
                .command_catalog
                .skills
                .iter()
                .enumerate()
                .filter(|(_, s)| s.plugin_id.is_none())
            {
                rows.push(ContextRow {
                    action: ContextAction::Skill(index, self.command_catalog.cycle),
                    title: skill.display_name(),
                    description: skill.summary(),
                    icon: "command-init",
                    artwork: skill.icon_path().map(|p| p.display().to_string()),
                    group: crate::i18n::text("技能"),
                });
            }
            for tab in &self.context_catalog.browser {
                rows.push(ContextRow {
                    action: ContextAction::Browser(tab.id),
                    title: tab.title.clone(),
                    description: tab.url.clone(),
                    icon: "panel-browser",
                    artwork: None,
                    group: crate::i18n::text("标签页"),
                });
            }
        }
        // `fuzzy` matches everything for an empty query.
        rows.retain(|row| {
            fuzzy(&row.title, query).is_some()
                || fuzzy(&row.description, query).is_some()
                || match &row.action {
                    ContextAction::Command(command) => fuzzy(command.id(), query).is_some(),
                    ContextAction::Files => fuzzy("files folders", query).is_some(),
                    ContextAction::Skill(index, _) => self
                        .command_catalog
                        .skills
                        .get(*index)
                        .is_some_and(|skill| fuzzy(&skill.name, query).is_some()),
                    ContextAction::Plugin(index, _) => self
                        .context_catalog
                        .plugins
                        .get(*index)
                        .is_some_and(|plugin| fuzzy(&plugin.name, query).is_some()),
                    _ => false,
                }
        });
        if !self.context_catalog.project && !query.is_empty() {
            rows.extend(
                self.context_catalog
                    .files
                    .iter()
                    .take(8)
                    .map(|file| ContextRow {
                        action: ContextAction::File(PathBuf::from(&file.root).join(&file.path)),
                        title: file.file_name.clone(),
                        description: file.path.clone(),
                        icon: "utility-folder",
                        artwork: None,
                        group: crate::i18n::text("文件和聊天"),
                    }),
            );
            rows.extend(
                self.context_catalog
                    .threads
                    .iter()
                    .take(8)
                    .map(|r| ContextRow {
                        action: ContextAction::Thread(
                            r.thread.thread_id.clone(),
                            r.thread.title.clone(),
                        ),
                        title: r.thread.title.clone(),
                        description: r.snippet.clone(),
                        icon: "command-chat",
                        artwork: None,
                        group: crate::i18n::text("文件和聊天"),
                    }),
            );
        }
        if self.context_catalog.error.is_some()
            || self.command_catalog.skills_error.is_some()
            || self.context_catalog.search_error.is_some()
        {
            rows.push(ContextRow {
                action: ContextAction::Retry,
                title: crate::i18n::format!("重试"=>"Retry"),
                description: self
                    .context_catalog
                    .error
                    .as_ref()
                    .or(self.command_catalog.skills_error.as_ref())
                    .or(self.context_catalog.search_error.as_ref())
                    .cloned()
                    .unwrap_or_default(),
                icon: "model-reset",
                artwork: None,
                group: crate::i18n::text("加载失败"),
            });
        }
        rows
    }
    fn remove_context_query(&mut self, cx: &mut Context<Self>) {
        if let Some(range) = self.context_range(cx).filter(|range| !range.is_empty()) {
            self.prompt_editor
                .update(cx, |e, cx| e.replace_range(range, "", cx));
        }
    }
    fn insert_context_reference(&mut self, reference: AgentInputReference, cx: &mut Context<Self>) {
        let Some(range) = self.context_range(cx) else {
            return;
        };
        self.dismiss_context(cx);
        self.prompt_editor
            .update(cx, |e, cx| e.insert_reference(range, reference, cx));
        self.prompt_context.references = self.prompt_editor.read(cx).references().to_vec();
    }
    pub(super) fn select_context_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.context_rows(cx).into_iter().nth(index) else {
            return;
        };
        match row.action {
            ContextAction::Retry => {
                let start = self.context_catalog.start;
                let mention = self.context_catalog.mention_start;
                self.retry_command_skills(cx);
                self.load_context_catalog(cx);
                self.context_catalog.mention_start = mention;
                self.context_catalog.start = start;
                self.update_context_query(cx);
            }
            ContextAction::Files => {
                self.remove_context_query(cx);
                self.dismiss_context(cx);
                // Drain the query edit's Changed event before recording the
                // draft revision used to reject late native picker results.
                let weak = cx.entity().downgrade();
                cx.defer(move |cx| {
                    let _ = weak.update(cx, |this, cx| this.attach_files(cx));
                });
            }
            ContextAction::Command(SlashCommand::Project) => {
                self.remove_context_query(cx);
                self.context_catalog.mention_start = None;
                self.context_catalog.start = self.prompt_editor.read(cx).cursor();
                self.context_catalog.project = true;
                self.context_catalog.highlighted = 0;
                self.context_catalog.query.clear();
                self.context_catalog
                    .scroll
                    .set_offset(point(px(0.), px(0.)));
            }
            ContextAction::Project(project) => {
                self.remove_context_query(cx);
                self.dismiss_context(cx);
                cx.emit(super::DraftProjectSelected(project));
            }
            ContextAction::Command(command) => {
                self.remove_context_query(cx);
                self.dismiss_context(cx);
                match command {
                    SlashCommand::Goal => {
                        if self.prompt_context.plan_mode == Some(true) {
                            self.prompt_context.plan_mode = Some(false);
                        }
                        self.set_goal_draft(true, cx);
                    }
                    SlashCommand::PlanMode => {
                        self.prompt_context.plan_mode =
                            Some(self.prompt_context.plan_mode != Some(true));
                        cx.emit(ConversationChanged);
                    }
                    SlashCommand::Sketch => self.open_sketch(cx),
                    _ => {}
                }
            }
            ContextAction::Plugin(index, cycle) => {
                if cycle != self.context_catalog.cycle {
                    return;
                }
                if let Some(plugin) = self.context_catalog.plugins.get(index) {
                    self.insert_context_reference(
                        AgentInputReference {
                            range: 0..0,
                            label: plugin.display_name().into(),
                            name: plugin.display_name().into(),
                            path: format!("plugin://{}", plugin.id),
                            kind: AgentInputReferenceKind::Plugin,
                            icon: plugin.interface.as_ref().and_then(|i| {
                                i.composer_icon
                                    .as_ref()
                                    .map(|p| p.display().to_string())
                                    .or_else(|| i.composer_icon_url.clone())
                                    .or_else(|| {
                                        i.logo_source(self.mode == crate::theme::ThemeMode::Dark)
                                            .map(str::to_owned)
                                    })
                            }),
                            brand_color: plugin
                                .interface
                                .as_ref()
                                .and_then(|i| i.brand_color.clone()),
                        },
                        cx,
                    );
                }
            }
            ContextAction::Skill(index, cycle) => {
                if cycle != self.command_catalog.cycle {
                    return;
                }
                if let Some(skill) = self.command_catalog.skills.get(index) {
                    self.insert_context_reference(
                        AgentInputReference {
                            range: 0..0,
                            label: skill.display_name(),
                            name: skill.name.clone(),
                            path: skill.path.display().to_string(),
                            kind: AgentInputReferenceKind::Skill,
                            icon: skill.icon_path().map(|p| p.display().to_string()),
                            brand_color: skill
                                .interface
                                .as_ref()
                                .and_then(|i| i.brand_color.clone()),
                        },
                        cx,
                    );
                }
            }
            ContextAction::File(path) => {
                self.remove_context_query(cx);
                self.dismiss_context(cx);
                self.attach_paths(vec![path], cx);
            }
            ContextAction::Browser(id) => {
                self.remove_context_query(cx);
                self.dismiss_context(cx);
                cx.emit(AttachBrowserContext(id));
            }
            ContextAction::Thread(id, title) => self.attach_thread_context(id, title, cx),
        }
        cx.notify();
    }
    pub(super) fn context_menu_key(
        &mut self,
        key: &str,
        ctrl: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.context_menu_open || self.prompt_editor.read(cx).composing() {
            return false;
        }
        let count = self.context_rows(cx).len();
        match (key, ctrl) {
            ("escape", false) => self.dismiss_context(cx),
            ("left", false) if self.context_catalog.project => {
                self.context_catalog.project = false;
                self.context_catalog.highlighted = 0;
            }
            ("down", false) | ("n", true) if count > 0 => {
                self.context_catalog.highlighted = (self.context_catalog.highlighted + 1) % count
            }
            ("up", false) | ("p", true) if count > 0 => {
                self.context_catalog.highlighted =
                    (self.context_catalog.highlighted + count - 1) % count
            }
            ("enter" | "tab", false) => self.select_context_row(
                self.context_catalog
                    .highlighted
                    .min(count.saturating_sub(1)),
                cx,
            ),
            _ => return false,
        }
        let rows = self.context_rows(cx);
        let mut previous = None;
        let mut headers = 0;
        for row in rows.iter().take(self.context_catalog.highlighted + 1) {
            if previous != Some(row.group) {
                previous = Some(row.group);
                headers += 1;
            }
        }
        self.context_catalog
            .scroll
            .scroll_to_item(self.context_catalog.highlighted + headers);
        cx.notify();
        true
    }
    pub(crate) fn attach_browser_context_result(
        &mut self,
        result: Result<PathBuf, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(path) => self.attach_paths(vec![path], cx),
            Err(error) => self.show_toast(ToastKind::Danger, error, cx),
        }
    }
}
