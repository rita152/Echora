//! Dynamic command data and command execution, scoped to a composer and cwd.
use std::{collections::HashSet, ops::Range};

use gpui::Context;

use super::{
    ComposerView, ConversationChanged,
    slash_menu::{SlashCommand, SlashItem, fuzzy},
    toast::ToastKind,
};
use crate::agent::{
    AgentInputReference, AgentInputReferenceKind, AgentMcpServerStatus,
    AgentMcpServerStatusRequest, AgentSkill, AgentSkillsLoadRequest, ProjectId,
};

#[derive(Default)]
pub(super) struct CommandCatalog {
    pub cycle: u64,
    pub fork_loading: bool,
    pub recent_models: Vec<crate::workspace::RecentModelConfiguration>,
    pub skills: Vec<AgentSkill>,
    pub skills_loaded: bool,
    pub skills_loading: bool,
    pub skills_error: Option<String>,
    pub mcp_cycle: u64,
    pub mcp: Vec<AgentMcpServerStatus>,
    pub mcp_loading: bool,
    pub mcp_error: Option<String>,
}

impl CommandCatalog {
    pub fn invalidate(&mut self) {
        self.cycle = self.cycle.wrapping_add(1);
        self.skills.clear();
        self.skills_loaded = false;
        self.skills_loading = false;
        self.skills_error = None;
        self.mcp_cycle = self.mcp_cycle.wrapping_add(1);
        self.mcp.clear();
        self.mcp_loading = false;
        self.mcp_error = None;
    }
}

impl ComposerView {
    pub(crate) fn settle_command_fork(&mut self, cx: &mut Context<Self>) {
        self.command_catalog.fork_loading = false;
        cx.notify();
    }
    pub(super) fn retry_command_skills(&mut self, cx: &mut Context<Self>) {
        self.command_catalog.skills_loaded = false;
        self.command_catalog.skills_error = None;
        self.ensure_command_skills(cx);
    }
    pub(super) fn ensure_command_skills(&mut self, cx: &mut Context<Self>) {
        if self.command_catalog.skills_loaded || self.command_catalog.skills_loading {
            return;
        }
        self.command_catalog.skills_loading = true;
        self.command_catalog.cycle = self.command_catalog.cycle.wrapping_add(1);
        let cycle = self.command_catalog.cycle;
        let cwd = self.conversation.cwd.clone();
        let response = self.backend.load_skills(AgentSkillsLoadRequest {
            cwds: vec![cwd.clone()],
            force_reload: true,
        });
        cx.spawn(async move |this, cx| {
            let result = response.recv().await;
            let _ = this.update(cx, |this, cx| {
                if this.conversation.cwd != cwd || this.command_catalog.cycle != cycle {
                    return;
                }
                this.command_catalog.skills_loading = false;
                this.command_catalog.skills_loaded = false;
                match result {
                    Ok(Ok(snapshot))
                        if snapshot.generation >= this.conversation.runtime.generation =>
                    {
                        this.command_catalog.skills_loaded = true;
                        let errors = snapshot
                            .entries
                            .iter()
                            .filter(|entry| entry.cwd == cwd)
                            .flat_map(|entry| &entry.errors)
                            .map(|error| format!("{}: {}", error.path.display(), error.message))
                            .collect::<Vec<_>>();
                        let mut seen = HashSet::new();
                        this.command_catalog.skills = snapshot
                            .entries
                            .into_iter()
                            .filter(|entry| entry.cwd == cwd)
                            .flat_map(|entry| entry.skills)
                            .filter(|skill| skill.enabled && seen.insert(skill.path.clone()))
                            .collect();
                        this.command_catalog
                            .skills
                            .sort_by_key(AgentSkill::display_name);
                        this.command_catalog.skills_error =
                            (!errors.is_empty()).then(|| errors.join("\n"));
                    }
                    Ok(Ok(_)) => {
                        this.command_catalog.skills_loaded = false;
                    }
                    Ok(Err(error)) => {
                        this.command_catalog.skills_error = Some(error.user_message());
                    }
                    Err(error) => {
                        this.command_catalog.skills_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn command_skill_items(&self, query: &str) -> Vec<SlashItem> {
        let mut rows = self
            .command_catalog
            .skills
            .iter()
            .enumerate()
            .filter_map(|(index, skill)| {
                let title = skill.display_name();
                let title_match = fuzzy(&title, query);
                let name_match = fuzzy(&skill.name, query);
                let summary_match = fuzzy(&skill.summary(), query);
                let score = title_match
                    .as_ref()
                    .map(|m| m.0)
                    .into_iter()
                    .chain(name_match.map(|m| m.0))
                    .chain(summary_match.map(|m| m.0))
                    .max()?;
                Some((
                    score,
                    SlashItem {
                        command: SlashCommand::Skill {
                            index,
                            cycle: self.command_catalog.cycle,
                        },
                        title,
                        description: skill.summary(),
                        matched: title_match.map_or_else(Vec::new, |m| m.1),
                    },
                ))
            })
            .collect::<Vec<_>>();
        rows.sort_by(|(a_score, a), (b_score, b)| {
            b_score.cmp(a_score).then_with(|| a.title.cmp(&b.title))
        });
        rows.into_iter().map(|(_, row)| row).collect()
    }

    pub(super) fn command_project_rows(
        &self,
        cx: &gpui::App,
    ) -> Vec<(Option<ProjectId>, String, String)> {
        let query = self.slash_query(cx).unwrap_or_default();
        let mut rows = vec![(
            None,
            crate::i18n::format!("无" => "None"),
            crate::i18n::format!("不在任何项目中工作" => "Don't work in a project"),
        )];
        rows.extend(self.project_picker.projects.iter().map(|project| {
            (
                Some(project.project_id.clone()),
                project.name.clone(),
                project
                    .roots
                    .first()
                    .map_or_else(String::new, |p| p.display().to_string()),
            )
        }));
        // `fuzzy` matches everything for an empty query.
        rows.retain(|(_, title, path)| {
            fuzzy(title, &query).is_some() || fuzzy(&path.to_lowercase(), &query).is_some()
        });
        rows
    }

    pub(super) fn select_command_project(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((project, _, _)) = self.command_project_rows(cx).into_iter().nth(index) else {
            return;
        };
        self.remove_slash_token(cx);
        self.slash_menu = None;
        self.focus_prompt_pending = true;
        cx.emit(super::DraftProjectSelected(project));
        cx.notify();
    }

    pub(super) fn command_fork_rows(&self, cx: &gpui::App) -> Vec<(SlashCommand, String, String)> {
        let mut rows = vec![(
            SlashCommand::Fork,
            crate::i18n::format!("创建聊天分支"=>"Fork chat"),
            crate::i18n::format!("在当前工作区创建此聊天的分支"=>"Fork this chat in the current workspace"),
        )];
        if self
            .workspace
            .checkout
            .as_ref()
            .is_some_and(crate::git_review::Checkout::is_repository)
        {
            rows.push((SlashCommand::ForkWorktree,crate::i18n::format!("在新工作树中创建聊天分支"=>"Fork chat in new worktree"),crate::i18n::format!("在隔离的工作树中创建此聊天的分支"=>"Fork this chat into an isolated worktree")));
        }
        let query = self.slash_query(cx).unwrap_or_default();
        rows.retain(|(_, title, _)| fuzzy(title, &query).is_some());
        rows
    }
    pub(super) fn select_command_fork(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((command, _, _)) = self.command_fork_rows(cx).into_iter().nth(index) else {
            return;
        };
        self.remove_slash_token(cx);
        self.slash_menu = None;
        self.command_catalog.fork_loading = true;
        cx.emit(super::SlashChatAction(command));
        cx.notify();
    }

    fn reset_command_query(&mut self, cx: &mut Context<Self>) {
        self.prompt_editor.update(cx, |editor, cx| {
            let caret = editor.cursor();
            let range = super::slash_menu::token_range(editor.text(), caret);
            if let Some(range) = range {
                editor.replace_range(range, "/", cx);
            }
        });
        self.slash_scroll
            .set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
    }

    /// Returns true when this module owns the command, including its submenu.
    pub(super) fn select_catalog_command(
        &mut self,
        command: SlashCommand,
        cx: &mut Context<Self>,
    ) -> bool {
        match command {
            SlashCommand::Skill { index, cycle } => {
                if cycle != self.command_catalog.cycle {
                    return true;
                }
                let Some(skill) = self.command_catalog.skills.get(index).cloned() else {
                    return true;
                };
                let Some((range, _)) = self.slash_token(cx) else {
                    return true;
                };
                self.slash_menu = None;
                self.prompt_editor.update(cx, |editor, cx| {
                    editor.insert_reference(
                        range,
                        AgentInputReference {
                            range: Range::default(),
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
                    )
                });
                self.prompt_context.references = self.prompt_editor.read(cx).references().to_vec();
                self.focus_prompt_pending = true;
                cx.notify();
            }
            SlashCommand::Init => {
                self.remove_slash_token(cx);
                self.slash_menu = None;
                self.initialize_repository(cx);
            }
            SlashCommand::Mcp
            | SlashCommand::Project
            | SlashCommand::Fork
            | SlashCommand::Model
            | SlashCommand::Reasoning => {
                self.reset_command_query(cx);
                let menu = self.slash_menu.get_or_insert_with(Default::default);
                menu.mcp = command == SlashCommand::Mcp;
                menu.project = command == SlashCommand::Project;
                menu.fork = command == SlashCommand::Fork;
                menu.models = command == SlashCommand::Model;
                menu.reasoning = command == SlashCommand::Reasoning;
                menu.review = false;
                menu.highlighted = 0;
                if command == SlashCommand::Mcp {
                    self.load_command_mcp(cx);
                }
                cx.notify();
            }
            _ => return false,
        }
        true
    }

    /// Model shortcuts are exposed only for providers that require account
    /// authentication, matching the desktop reference's `requiresAuth` gate.
    pub(super) fn command_model_available(&self) -> bool {
        let Some(config) = &self.permission_config else {
            return false;
        };
        let provider = config
            .effective
            .get("model_provider")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("openai");
        let requires_auth = config
            .effective
            .get("model_providers")
            .and_then(|p| p.get(provider))
            .and_then(|p| p.get("requires_openai_auth"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(provider == "openai");
        requires_auth
            && !self.side_chat
            && !self.conversation.models.is_empty()
            && self.conversation.model_catalog_error.is_none()
    }
    pub(super) fn command_model_rows(&self, cx: &gpui::App) -> Vec<(usize, String, String, bool)> {
        let query = self.slash_query(cx).unwrap_or_default();
        if self.slash_menu.as_ref().is_some_and(|m| m.reasoning) {
            return self
                .conversation
                .effort_ids()
                .into_iter()
                .enumerate()
                .filter_map(|(i, id)| {
                    let title = Self::effort_label(&id).to_owned();
                    (fuzzy(&title, &query).is_some() || fuzzy(&id, &query).is_some()).then_some((
                        i,
                        title,
                        String::new(),
                        id == self.conversation.selected_effort,
                    ))
                })
                .collect();
        }
        let recent = self.command_recent_models();
        let mut rows = recent
            .iter()
            .enumerate()
            .filter_map(|(i, configuration)| {
                let model = self
                    .conversation
                    .models
                    .iter()
                    .find(|m| m.model == configuration.model || m.id == configuration.model)?;
                let title = super::picker::short_model_label(&model.display_name);
                let description = Self::effort_label(&configuration.effort).to_owned();
                (fuzzy(&title, &query).is_some() || fuzzy(&model.model, &query).is_some())
                    .then_some((
                        usize::MAX - i,
                        title,
                        description,
                        configuration.model == self.conversation.selected_model
                            && configuration.effort == self.request_effort()
                            && configuration.service_tier
                                == self.conversation.selected_service_tier,
                    ))
            })
            .collect::<Vec<_>>();
        if !query.is_empty() {
            rows.extend(
                self.conversation
                    .models
                    .iter()
                    .enumerate()
                    .filter_map(|(i, m)| {
                        let title = super::picker::short_model_label(&m.display_name);
                        (fuzzy(&title, &query).is_some() || fuzzy(&m.model, &query).is_some())
                            .then_some((i, title, m.description.clone(), false))
                    }),
            );
        }
        rows
    }
    pub(super) fn select_command_model(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((index, _, _, _)) = self.command_model_rows(cx).into_iter().nth(index) else {
            return;
        };
        let reasoning = self.slash_menu.as_ref().is_some_and(|m| m.reasoning);
        self.remove_slash_token(cx);
        self.slash_menu = None;
        if reasoning {
            self.select_effort_at(index);
        } else if index > usize::MAX - 4 {
            if let Some(configuration) = self
                .command_recent_models()
                .get(usize::MAX - index)
                .cloned()
                && let Some(model) = self
                    .conversation
                    .models
                    .iter()
                    .position(|m| m.model == configuration.model || m.id == configuration.model)
            {
                self.apply_model_selection(
                    model,
                    Some(configuration.effort),
                    configuration.service_tier,
                    true,
                );
                self.conversation.model_explicit = true;
            }
        } else {
            self.select_model_at(index);
        }
        self.focus_prompt_pending = true;
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub(crate) fn set_recent_command_models(
        &mut self,
        recent: Vec<crate::workspace::RecentModelConfiguration>,
        cx: &mut Context<Self>,
    ) {
        self.command_catalog.recent_models = recent;
        cx.notify();
    }
    pub(crate) fn selected_command_configuration(
        &self,
    ) -> Option<crate::workspace::RecentModelConfiguration> {
        self.conversation
            .model_user_selected
            .then(|| crate::workspace::RecentModelConfiguration {
                model: self.conversation.selected_model.clone(),
                effort: self.request_effort(),
                service_tier: self.conversation.selected_service_tier.clone(),
            })
    }
    fn command_recent_models(&self) -> Vec<crate::workspace::RecentModelConfiguration> {
        if !self.command_catalog.recent_models.is_empty() {
            return self
                .command_catalog
                .recent_models
                .iter()
                .take(3)
                .cloned()
                .collect();
        }
        self.conversation
            .models
            .iter()
            .take(3)
            .map(|m| crate::workspace::RecentModelConfiguration {
                model: m.model.clone(),
                effort: m.default_reasoning_effort.clone(),
                service_tier: None,
            })
            .collect()
    }

    fn initialize_repository(&mut self, cx: &mut Context<Self>) {
        let cwd = self.conversation.cwd.clone();
        let draft = self.prompt_editor.read(cx).text().to_owned();
        let thread = self.conversation.thread_id.clone();
        cx.spawn(async move |this,cx| {
            let path = cwd.join("AGENTS.md");
            let exists = cx.background_executor().spawn(async move { path.try_exists() }).await;
            let _ = this.update(cx, |this,cx| {
                if this.conversation.cwd != cwd || this.conversation.thread_id != thread || this.prompt_editor.read(cx).text() != draft { return; }
                match exists {
                    Ok(true) => this.show_toast(ToastKind::Success,crate::i18n::format!("此处已存在 AGENTS.md。为避免覆盖，已跳过 /init" => "AGENTS.md already exists here. Skipping /init to avoid overwriting it"),cx),
                    Ok(false) => this.submit_prompt(INIT_PROMPT.to_owned(),cx),
                    Err(error) => this.show_toast(ToastKind::Danger,error.to_string(),cx),
                }
                this.focus_prompt_pending = true;
                cx.emit(ConversationChanged);
                cx.notify();
            });
        }).detach();
    }

    pub(super) fn load_command_mcp(&mut self, cx: &mut Context<Self>) {
        self.command_catalog.mcp_cycle = self.command_catalog.mcp_cycle.wrapping_add(1);
        let cycle = self.command_catalog.mcp_cycle;
        self.command_catalog.mcp_loading = true;
        self.command_catalog.mcp_error = None;
        self.command_catalog.mcp.clear();
        let backend = self.backend.clone();
        let cwd = self.conversation.cwd.clone();
        let thread_id = self.conversation.thread_id.clone();
        cx.spawn(async move |this,cx| {
            let mut cursor = None;
            let mut seen = HashSet::new();
            let mut servers = Vec::new();
            let mut generation = None;
            let result = loop {
                let response = backend.list_mcp_servers(AgentMcpServerStatusRequest { cursor: cursor.clone(), limit: Some(100), thread_id: thread_id.clone(), ..Default::default() });
                match response.recv().await {
                    Ok(Ok(page)) => {
                        if generation.is_some_and(|g| g != page.generation) { break Err(crate::i18n::format!("MCP 连接已更改，请重新打开菜单" => "MCP connection changed. Reopen the menu.")); }
                        generation = Some(page.generation);
                        servers.extend(page.servers);
                        match page.next_cursor {
                            Some(next) if seen.len() < 64 && seen.insert(next.clone()) => cursor = Some(next),
                            Some(_) => break Err(crate::i18n::format!("MCP 分页游标重复或页数超过限制" => "MCP pagination repeated or exceeded its limit")),
                            None => break Ok(servers),
                        }
                    }
                    Ok(Err(error)) => break Err(error.user_message()),
                    Err(error) => break Err(error.to_string()),
                }
            };
            let _ = this.update(cx, |this,cx| {
                if this.command_catalog.mcp_cycle != cycle || this.conversation.cwd != cwd || this.conversation.thread_id != thread_id { return; }
                this.command_catalog.mcp_loading = false;
                match result {
                    Ok(mut servers) if generation.is_none_or(|g| g >= this.conversation.runtime.generation) => {
                        servers.sort_by(|a,b| a.name.cmp(&b.name));
                        this.command_catalog.mcp = servers;
                    }
                    Ok(_) => this.command_catalog.mcp_error = Some(crate::i18n::format!("MCP 连接已更改，请重新打开菜单" => "MCP connection changed. Reopen the menu.")),
                    Err(error) => this.command_catalog.mcp_error = Some(error),
                }
                cx.notify();
            });
        }).detach();
    }
}

const INIT_PROMPT: &str = "Generate a file named AGENTS.md that serves as a contributor guide for this repository.\nYour goal is to produce a clear, concise, and well-structured document with descriptive headings and actionable explanations for each section.\nFollow the outline below, but adapt as needed — add sections if relevant, and omit those that do not apply to this project.\n\nDocument Requirements\n\n- Title the document \"Repository Guidelines\".\n- Use Markdown headings (#, ##, etc.) for structure.\n- Keep the document concise. 200-400 words is optimal.\n- Keep explanations short, direct, and specific to this repository.\n- Provide examples where helpful (commands, directory paths, naming patterns).\n- Maintain a professional, instructional tone.\n\nRecommended Sections\n\nProject Structure & Module Organization\n\n- Outline the project structure, including where the source code, tests, and assets are located.\n\nBuild, Test, and Development Commands\n\n- List key commands for building, testing, and running locally (e.g., npm test, make build).\n- Briefly explain what each command does.\n\nCoding Style & Naming Conventions\n\n- Specify indentation rules, language-specific style preferences, and naming patterns.\n- Include any formatting or linting tools used.\n\nTesting Guidelines\n\n- Identify testing frameworks and coverage requirements.\n- State test naming conventions and how to run tests.\n\nCommit & Pull Request Guidelines\n\n- Summarize commit message conventions found in the project’s Git history.\n- Outline pull request requirements (descriptions, linked issues, screenshots, etc.).\n\n(Optional) Add other sections if relevant, such as Security & Configuration Tips, Architecture Overview, or Agent-Specific Instructions.\n";
