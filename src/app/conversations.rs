//! Conversations behavior and presentation for the application shell.

use std::path::PathBuf;

use gpui::{Context, Entity, SharedString};

use super::{ChatApp, ConversationHost, ConversationKey, DraftId, state::RightPanelMode};
use crate::{
    agent::{Project, ProjectId, ThreadId},
    components::composer::{ComposerView, ConversationChanged},
    workspace::{project_id_for_thread, project_label},
};

impl ChatApp {
    pub(super) fn switch_home_to(&mut self, key: ConversationKey, cx: &mut Context<Self>) {
        self.cancel_permission_confirmation(cx);
        let Some(host) = self.conversation_hosts.get(&key) else {
            return;
        };
        let composer = host.composer.clone();
        let cwd = host.cwd.clone();
        let project_id = host.project_id.clone();
        if self.active_conversation != key {
            self.conversation_back
                .push(self.active_conversation.clone());
            self.conversation_forward.clear();
        }
        let thread_id = match &key {
            ConversationKey::Draft(_) => None,
            ConversationKey::Thread(thread_id) => Some(thread_id.clone()),
        };
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(cwd, project_id.clone(), thread_id.clone(), cx);
        });
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_conversation_selection(project_id, thread_id, cx);
        });
        self.sync_project_labels(&self.workspace_store.snapshot().projects, cx);
        self.deactivate_review(cx);
        self.deactivate_side_chat(cx);
        // The right panel belongs to its chat: the one the window leaves keeps
        // its state, and the one it shows has its own. Behind the Pull
        // Requests page the panel is parked and that page's exit restores it.
        if !self.right_panel_parked {
            self.remember_right_panel();
        }
        self.active_conversation = key;
        self.update_chat_search_roots(cx);
        if !self.right_panel_parked {
            self.apply_right_panel_state(cx);
        }
        self.home
            .update(cx, |home, cx| home.set_composer(composer, cx));
        self.sync_summary_panel(cx);
        cx.notify();
    }
    /// Names every conversation after the project it targets, so the home
    /// heading and the composer's project control follow renames.
    pub(super) fn sync_project_labels(&mut self, projects: &[Project], cx: &mut Context<Self>) {
        let snapshot = self.workspace_store.snapshot();
        for host in self.conversation_hosts.values() {
            let label = project_label(host.project_id.as_deref(), &host.cwd, projects)
                .map(SharedString::from);
            host.composer.update(cx, |composer, cx| {
                composer.set_project_label(label, cx);
                composer.set_available_projects(
                    projects,
                    snapshot.loading.projects,
                    snapshot.error.clone(),
                    cx,
                );
            });
        }
    }

    pub(super) fn wire_project_selection(composer: &Entity<ComposerView>, cx: &mut Context<Self>) {
        cx.subscribe(
            composer,
            |this, _, _: &crate::components::composer::OpenWorkspaceGitSettings, cx| {
                this.open_settings_page("git-settings", cx);
            },
        )
        .detach();
        cx.subscribe(
            composer,
            |this, composer, event: &crate::components::composer::DraftWorkspaceChanged, cx| {
                let key = this
                    .conversation_hosts
                    .iter()
                    .find_map(|(key, host)| (host.composer == composer).then(|| key.clone()));
                let Some(key) = key else {
                    return;
                };
                let Some(host) = this.conversation_hosts.get_mut(&key) else {
                    return;
                };
                host.cwd = event.0.clone();
                if let Some(panel) = this.file_panels.get(&key) {
                    panel.update(cx, |panel, cx| {
                        panel.set_workspace_root(event.0.clone(), cx)
                    });
                }
                if let Some(panel) = this.terminal_panels.get(&key) {
                    panel.update(cx, |panel, cx| {
                        panel.set_workspace_root(event.0.clone(), cx)
                    });
                }
                if let Some(panel) = this.review_panels.get(&key) {
                    panel.update(cx, |panel, cx| {
                        panel.set_workspace_root(event.0.clone(), cx);
                    });
                }
                this.workspace_store.invalidate_branch_lookups();
                if key == this.active_conversation {
                    this.update_chat_search_roots(cx);
                    this.sync_summary_panel(cx);
                }
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(
            composer,
            |this, composer, event: &crate::components::composer::DraftProjectSelected, cx| {
                this.select_draft_project(&composer, event.0.clone(), cx);
            },
        )
        .detach();
        cx.subscribe(
            composer,
            |this, _, event: &crate::components::composer::OpenProjectCreation, cx| {
                this.project_creation.previous_focus = Some(event.0.clone());
                this.open_project_creation(cx)
            },
        )
        .detach();
    }

    pub(super) fn select_draft_project(
        &mut self,
        composer: &Entity<ComposerView>,
        project_id: Option<ProjectId>,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.active_conversation, ConversationKey::Draft(_)) {
            return;
        }
        let snapshot = self.workspace_store.snapshot();
        let cwd = match &project_id {
            Some(id) => {
                let Some(project) = snapshot.projects.iter().find(|p| &p.project_id == id) else {
                    return;
                };
                project
                    .roots
                    .first()
                    .cloned()
                    .unwrap_or_else(crate::components::composer::projectless_cwd)
            }
            None => crate::components::composer::projectless_cwd(),
        };
        let Some(host) = self.conversation_hosts.get_mut(&self.active_conversation) else {
            return;
        };
        if &host.composer != composer {
            return;
        }
        host.cwd = cwd.clone();
        host.project_id = project_id.clone();
        if let Some(panel) = self.file_panels.get(&self.active_conversation) {
            panel.update(cx, |panel, cx| panel.set_workspace_root(cwd.clone(), cx));
        }
        if let Some(panel) = self.terminal_panels.get(&self.active_conversation) {
            panel.update(cx, |panel, cx| panel.set_workspace_root(cwd.clone(), cx));
        }
        if let Some(panel) = self.review_panels.get(&self.active_conversation) {
            panel.update(cx, |panel, cx| panel.set_workspace_root(cwd.clone(), cx));
        }
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(cwd, project_id.clone(), None, cx)
        });
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_draft_project(project_id, cx));
        self.sync_project_labels(&snapshot.projects, cx);
        self.update_chat_search_roots(cx);
        self.sync_summary_panel(cx);
        cx.notify();
    }

    pub(super) fn start_draft(
        &mut self,
        project_id: Option<ProjectId>,
        cwd: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let draft_id = DraftId(self.next_draft_id);
        self.next_draft_id = self.next_draft_id.wrapping_add(1).max(1);
        let composer = self.new_host_composer(cx);
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(cwd.clone(), project_id.clone(), None, cx);
        });
        self.draft_focus_pending = true;
        let key = ConversationKey::Draft(draft_id);
        self.conversation_hosts.insert(
            key.clone(),
            ConversationHost {
                composer,
                cwd,
                project_id,
            },
        );
        self.switch_home_to(key, cx);
    }

    /// New-chat navigation returns to the unsent draft for this destination.
    /// Explicit prompt creation (PR chat, review, /new) still allocates a host.
    pub(super) fn resume_or_start_draft(
        &mut self,
        project_id: Option<ProjectId>,
        cwd: PathBuf,
        section: Option<crate::agent::ThreadSectionId>,
        cx: &mut Context<Self>,
    ) {
        let existing = self
            .conversation_hosts
            .iter()
            .filter_map(|(key, host)| {
                let ConversationKey::Draft(id) = key else {
                    return None;
                };
                (host.project_id == project_id
                    && host.cwd == cwd
                    && self.draft_sections.get(key) == section.as_ref())
                .then_some((id.0, key.clone()))
            })
            .max_by_key(|(id, _)| *id)
            .map(|(_, key)| key);
        self.draft_focus_pending = true;
        if let Some(key) = existing {
            self.switch_home_to(key, cx);
        } else {
            self.start_draft(project_id, cwd, cx);
            if let Some(section) = section {
                self.draft_sections
                    .insert(self.active_conversation.clone(), section);
            }
        }
    }

    pub(super) fn navigate_conversation(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.showing_plugins && !forward {
            self.close_plugins(cx);
            self.draft_focus_pending = true;
            return;
        }
        let (mut back, mut next) = (
            std::mem::take(&mut self.conversation_back),
            std::mem::take(&mut self.conversation_forward),
        );
        let (source, destination) = if forward {
            (&mut next, &mut back)
        } else {
            (&mut back, &mut next)
        };
        while let Some(key) = source.pop() {
            if key != self.active_conversation && self.conversation_hosts.contains_key(&key) {
                destination.push(self.active_conversation.clone());
                self.switch_home_to(key, cx);
                self.close_pull_requests(cx);
                self.draft_focus_pending = true;
                break;
            }
        }
        self.conversation_back = back;
        self.conversation_forward = next;
        cx.notify();
    }
    pub(super) fn ensure_thread_conversation(
        &mut self,
        thread_id: ThreadId,
        cx: &mut Context<Self>,
    ) -> Entity<ComposerView> {
        let key = ConversationKey::Thread(thread_id.clone());
        if let Some(host) = self.conversation_hosts.get(&key) {
            let composer = host.composer.clone();
            let composer_view = composer.read(cx);
            let retry_history =
                composer_view.history_needs_retry() || composer_view.history_needs_reload();
            if retry_history {
                composer.update(cx, |composer, cx| {
                    composer.clear_history_stale();
                    composer.set_history_loading(true, cx);
                });
                self.load_conversation_history(
                    ConversationKey::Thread(thread_id.clone()),
                    thread_id,
                    composer.clone(),
                    cx,
                );
            }
            return composer;
        }

        let snapshot = self.workspace_store.snapshot();
        let summary = snapshot.thread(&thread_id).cloned().or_else(|| {
            snapshot
                .search_results
                .iter()
                .find(|result| result.thread.thread_id == thread_id)
                .map(|result| result.thread.clone())
        });
        let cwd = summary
            .as_ref()
            .map(|thread| thread.cwd.clone())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let project_id = summary
            .as_ref()
            .and_then(|thread| project_id_for_thread(thread, &snapshot.projects));
        let composer = self.new_host_composer(cx);
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(
                cwd.clone(),
                project_id.clone(),
                Some(thread_id.clone()),
                cx,
            );
            composer.set_history_loading(true, cx);
        });
        self.conversation_hosts.insert(
            key.clone(),
            ConversationHost {
                composer: composer.clone(),
                cwd,
                project_id,
            },
        );
        self.watch_history_invalidation(key.clone(), thread_id.clone(), composer.clone(), cx);

        self.load_conversation_history(
            ConversationKey::Thread(thread_id.clone()),
            thread_id,
            composer.clone(),
            cx,
        );
        composer
    }

    /// A revert performed by another writer - or one whose request failed
    /// after the server had already confirmed it - leaves the locally reduced
    /// turns out of date. Reload them from app-server as soon as the
    /// conversation reports its history as stale, without fabricating the
    /// truncated turns locally.
    fn watch_history_invalidation(
        &mut self,
        key: ConversationKey,
        thread_id: ThreadId,
        composer: Entity<ComposerView>,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe(
            &composer,
            move |this, composer, _: &ConversationChanged, cx| {
                if !composer.read(cx).history_needs_reload() {
                    return;
                }
                composer.update(cx, |composer, cx| {
                    composer.clear_history_stale();
                    composer.set_history_loading(true, cx);
                });
                this.load_conversation_history(
                    key.clone(),
                    thread_id.clone(),
                    composer.clone(),
                    cx,
                );
            },
        )
        .detach();
    }

    /// The command menu searches the active conversation's working directory,
    /// which is also the root app-server reports matches against.
    pub(super) fn update_chat_search_roots(&mut self, cx: &mut Context<Self>) {
        let roots = self
            .conversation_hosts
            .get(&self.active_conversation)
            .map(|host| vec![host.cwd.to_string_lossy().into_owned()])
            .unwrap_or_default();
        self.chat_search
            .update(cx, |search, _| search.set_workspace_roots(roots));
    }
    pub(super) fn select_conversation(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        let key = ConversationKey::Thread(thread_id.clone());
        self.ensure_thread_conversation(thread_id, cx);
        self.switch_home_to(key, cx);
    }
    pub(super) fn load_conversation_history(
        &mut self,
        key: ConversationKey,
        thread_id: ThreadId,
        composer: Entity<ComposerView>,
        cx: &mut Context<Self>,
    ) {
        let receiver = self.workspace_store.load_history(thread_id);
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::agent::WorkspaceError::backend(crate::i18n::text(
                    "读取聊天历史的响应通道提前关闭",
                )))
            });
            let _ = this.update(cx, |this, cx| match result {
                Ok(history) => {
                    if let Some(host) = this.conversation_hosts.get_mut(&key)
                        && host.composer == composer
                    {
                        host.cwd = history.thread.cwd.clone();
                        host.project_id = history.thread.project_id.clone();
                    }
                    if this.active_conversation == key {
                        this.sync_summary_panel(cx);
                    }
                    composer.update(cx, |composer, cx| composer.hydrate_history(history, cx));
                    if this.active_conversation == key
                        && this.right_panel.mode == Some(RightPanelMode::Review)
                    {
                        this.ensure_review(cx);
                    }
                }
                Err(error) => composer.update(cx, |composer, cx| {
                    composer.set_history_error(
                        error.user_message(crate::i18n::text("读取聊天历史")),
                        cx,
                    )
                }),
            });
        })
        .detach();
    }
    pub(super) fn rekey_created_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        let draft_key = self.conversation_hosts.iter().find_map(|(key, host)| {
            matches!(key, ConversationKey::Draft(_))
                .then(|| {
                    (host.composer.read(cx).thread_id() == Some(thread_id.as_str()))
                        .then(|| key.clone())
                })
                .flatten()
        });
        let Some(draft_key) = draft_key else {
            return;
        };
        let Some(host) = self.conversation_hosts.remove(&draft_key) else {
            return;
        };
        let preview = host
            .composer
            .read(cx)
            .conversation_render_snapshot()
            .1
            .unwrap_or_default();
        self.workspace_store.register_created_thread(
            thread_id.clone(),
            host.cwd.clone(),
            host.project_id.clone(),
            preview,
        );
        if self.active_conversation == draft_key {
            self.sidebar.update(cx, |sidebar, cx| {
                sidebar.reveal_created_thread(thread_id.clone(), host.project_id.clone(), cx);
            });
        }
        if let Some(section_id) = self.draft_sections.remove(&draft_key) {
            self.workspace_store.move_to_custom_section(
                crate::workspace::SectionItem::Thread(thread_id.clone()),
                Some(section_id),
            );
        }
        let real_key = ConversationKey::Thread(thread_id.clone());
        for key in self
            .conversation_back
            .iter_mut()
            .chain(&mut self.conversation_forward)
        {
            if key == &draft_key {
                *key = real_key.clone();
            }
        }
        if self.active_conversation == draft_key {
            self.active_conversation = real_key.clone();
        }
        if self.right_panel_chats.remove(&draft_key) {
            self.right_panel_chats.insert(real_key.clone());
        }
        if let Some(panel) = self.file_panels.remove(&draft_key) {
            self.file_panels.insert(real_key.clone(), panel);
        }
        if let Some(panel) = self.terminal_panels.remove(&draft_key) {
            self.terminal_panels.insert(real_key.clone(), panel);
        }
        if let Some(panel) = self.browser_panels.remove(&draft_key) {
            // From now on the chat's tabs are saved with it.
            panel.update(cx, |panel, cx| panel.set_chat(thread_id.to_string(), cx));
            self.browser_panels.insert(real_key.clone(), panel);
        }
        if let Some(panel) = self.review_panels.remove(&draft_key) {
            self.review_panels.insert(real_key.clone(), panel);
        }
        let composer = host.composer.clone();
        self.conversation_hosts.insert(real_key, host);
        self.watch_history_invalidation(
            ConversationKey::Thread(thread_id.clone()),
            thread_id,
            composer,
            cx,
        );
        #[cfg(not(test))]
        self.workspace_store.refresh_all();
        self.sync_summary_panel(cx);
        cx.notify();
    }
}
