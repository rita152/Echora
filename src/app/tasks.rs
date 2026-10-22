//! Routes and conversation ownership for the task board.
use super::{ChatApp, ConversationHost, ConversationKey, DraftId};
use crate::{
    components::{
        composer::{ComposerView, ConversationThreadCreated, RequestFullAccessConfirmation},
        sidebar::SidebarView,
        tasks::{TaskAction, TaskRoute, TasksView},
    },
    tasks::TaskDraft,
};
use gpui::{Context, Entity};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PageRoute {
    Conversation(ConversationKey),
    Tasks(TaskRoute),
    Plugins,
    PullRequests,
    Settings,
}
impl ChatApp {
    pub(super) fn current_page(&self, cx: &gpui::App) -> PageRoute {
        if self.showing_tasks {
            PageRoute::Tasks(self.tasks.read(cx).route())
        } else if self.showing_settings {
            PageRoute::Settings
        } else if self.showing_plugins {
            PageRoute::Plugins
        } else if self.showing_pull_requests {
            PageRoute::PullRequests
        } else {
            PageRoute::Conversation(self.active_conversation.clone())
        }
    }
    pub(super) fn remember_task_route(&mut self, from: PageRoute, to: PageRoute) {
        if self.task_navigation || from == to {
            return;
        }
        self.task_back.push(from);
        self.task_forward.clear();
    }
    pub(super) fn wire_tasks(
        tasks: &Entity<TasksView>,
        sidebar: &Entity<SidebarView>,
        cx: &mut Context<Self>,
    ) {
        cx.observe(tasks, |_, _, cx| cx.notify()).detach();
        cx.subscribe(
            sidebar,
            |this, _, _: &crate::components::sidebar::OpenTasks, cx| this.open_tasks(None, cx),
        )
        .detach();
        cx.subscribe(tasks, |this, _, event: &TaskAction, cx| match event {
            TaskAction::New {
                project,
                draft,
                existing,
            } => {
                if let Some(id) = existing {
                    this.dispatch_saved_task(id, cx);
                } else {
                    this.new_task_editor(project.clone(), *draft, cx);
                }
            }
            TaskAction::OpenThread(id) => {
                this.select_conversation(id.clone(), cx);
            }
            TaskAction::ReplyThread { thread, text } => {
                let c = this.ensure_thread_conversation(thread.clone(), cx);
                let result = c.update(cx, |c, cx| c.reply_to_task(text, cx));
                match result {
                    Ok(()) => this.tasks.update(cx, |v, cx| v.reply_sent(text, cx)),
                    Err(e) => {
                        this.tasks.update(cx, |v, cx| v.show_message(e, cx));
                        this.select_conversation(thread.clone(), cx);
                    }
                }
            }
            TaskAction::StopThread(id) => {
                let c = this.ensure_thread_conversation(id.clone(), cx);
                c.update(cx, |c, cx| c.stop_task(cx));
            }
            TaskAction::OpenDraft(id) => this.edit_saved_task(id, cx),
            TaskAction::Launch {
                draft,
                composer,
                open,
            } => this.launch_board_task(*draft.clone(), composer.clone(), *open, cx),
            TaskAction::SetGoal { thread, objective } => {
                this.set_board_goal(thread.clone(), objective.clone(), cx)
            }
            TaskAction::RouteChanged { from, to } => this
                .remember_task_route(PageRoute::Tasks(from.clone()), PageRoute::Tasks(to.clone())),
        })
        .detach();
    }
    pub fn open_tasks(&mut self, route: Option<TaskRoute>, cx: &mut Context<Self>) {
        let to = route.unwrap_or_else(|| self.tasks.read(cx).route());
        let from = self.current_page(cx);
        self.remember_task_route(from, PageRoute::Tasks(to.clone()));
        self.showing_settings = false;
        self.showing_plugins = false;
        self.showing_pull_requests = false;
        self.showing_tasks = true;
        self.tasks.update(cx, |view, cx| view.set_route(to, cx));
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_pull_requests_open(false, cx);
            sidebar.set_plugins_open(false, cx);
            sidebar.set_tasks_open(true, cx);
        });
        self.sync_right_panel_page(cx);
        cx.notify();
    }
    pub(super) fn close_tasks(&mut self, cx: &mut Context<Self>) {
        if !self.showing_tasks {
            return;
        }
        self.tasks.update(cx, |v, cx| v.park(cx));
        self.showing_tasks = false;
        self.sidebar.update(cx, |s, cx| s.set_tasks_open(false, cx));
        self.sync_right_panel_page(cx);
        cx.notify();
    }
    pub(super) fn navigate_task_page(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let route = if forward {
            self.task_forward.pop()
        } else {
            self.task_back.pop()
        };
        let Some(route) = route else {
            return false;
        };
        let current = self.current_page(cx);
        if forward {
            self.task_back.push(current);
        } else {
            self.task_forward.push(current);
        }
        self.task_navigation = true;
        match route {
            PageRoute::Tasks(route) => self.open_tasks(Some(route), cx),
            PageRoute::Conversation(key) => {
                self.close_tasks(cx);
                self.showing_settings = false;
                self.showing_plugins = false;
                self.showing_pull_requests = false;
                self.switch_home_to(key, cx);
            }
            PageRoute::Plugins => self.open_plugins(cx),
            PageRoute::PullRequests => self.open_pull_requests(cx),
            PageRoute::Settings => self.open_settings(cx),
        }
        self.task_navigation = false;
        cx.notify();
        true
    }
    fn create_task_composer(
        &mut self,
        draft: &TaskDraft,
        cx: &mut Context<Self>,
    ) -> Entity<ComposerView> {
        let saved = self.workspace_store.snapshot().preferences.tasks.drafts;
        self.task_draft_composers.retain(|id, _| {
            saved.iter().any(|d| &d.id == id) || self.tasks.read(cx).is_launching(id)
        });
        if let Some(composer) = self.task_draft_composers.get(&draft.id) {
            return composer.clone();
        }
        let composer = if let Some(thread) = &draft.thread_id {
            self.ensure_thread_conversation(thread.clone(), cx)
        } else {
            self.new_host_composer(cx)
        };
        composer.update(cx, |c, cx| {
            c.set_workspace_context(
                draft.cwd.clone(),
                Some(draft.project_id.clone()),
                draft.thread_id.clone(),
                cx,
            );
            c.restore_task_input(&draft.input, cx);
            c.set_available_projects(&self.workspace_store.snapshot().projects, false, None, cx);
        });
        cx.subscribe(
            &composer,
            |this, composer, _: &RequestFullAccessConfirmation, cx| {
                this.open_permission_confirmation(composer, cx)
            },
        )
        .detach();
        cx.subscribe(
            &composer,
            |this, composer, event: &crate::components::composer::OpenAttachmentPreview, cx| {
                this.image_preview.path = Some(event.0.clone());
                this.image_preview.composer = Some(composer.downgrade());
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(
            &composer,
            |this, _, event: &ConversationThreadCreated, cx| {
                this.rekey_created_thread(event.thread_id.clone(), cx)
            },
        )
        .detach();
        self.task_draft_composers
            .insert(draft.id.clone(), composer.clone());
        composer
    }
    fn new_task_editor(
        &mut self,
        project: Option<String>,
        send_as_draft: bool,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.workspace_store.snapshot();
        let project = project
            .and_then(|id| snapshot.projects.iter().find(|p| p.project_id == id))
            .or_else(|| snapshot.projects.first());
        let Some(project) = project else {
            return;
        };
        let Some(cwd) = project.roots.first() else {
            return;
        };
        let draft = TaskDraft {
            id: crate::tasks::new_id(),
            project_id: project.project_id.clone(),
            cwd: cwd.clone(),
            thread_id: None,
            client_message_id: None,
            created_at: chrono::Utc::now().timestamp(),
            input: Default::default(),
        };
        let composer = self.create_task_composer(&draft, cx);
        self.tasks.update(cx, |v, cx| {
            v.show_editor(draft, composer, send_as_draft, cx)
        });
    }
    fn edit_saved_task(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.tasks.read(cx).is_launching(id) {
            return;
        }
        let Some(draft) = self
            .workspace_store
            .snapshot()
            .preferences
            .tasks
            .drafts
            .into_iter()
            .find(|d| d.id == id)
        else {
            return;
        };
        let composer = self.create_task_composer(&draft, cx);
        composer.update(cx, |c, cx| c.restore_task_input(&draft.input, cx));
        self.tasks
            .update(cx, |v, cx| v.show_editor(draft, composer, true, cx));
    }
    fn dispatch_saved_task(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.tasks.read(cx).is_launching(id) {
            return;
        }
        let Some(draft) = self
            .workspace_store
            .snapshot()
            .preferences
            .tasks
            .drafts
            .into_iter()
            .find(|d| d.id == id)
        else {
            return;
        };
        if draft.client_message_id.is_some()
            && let Some(thread) = &draft.thread_id
        {
            let snapshot = self.workspace_store.snapshot();
            if snapshot
                .task_runs
                .get(thread)
                .is_none_or(|r| r.error.is_some())
                || snapshot.thread(thread).is_some_and(|t| {
                    matches!(t.activity, crate::agent::ThreadActivity::Active { .. })
                })
            {
                if let Some(thread) = snapshot.thread(thread) {
                    self.workspace_store.request_task_runs(vec![thread.clone()]);
                }
                self.tasks.update(cx, |v, cx| v.show_message("Checking the previous dispatch before retrying. Open the linked chat to inspect it.".into(), cx));
                return;
            }
        }
        let composer = self.create_task_composer(&draft, cx);
        // Resolve catalog/permissions before dispatch; a retry uses the exact saved input.
        composer.update(cx, |c, cx| c.restore_task_input(&draft.input, cx));
        self.tasks
            .update(cx, |v, cx| v.launch(draft, composer, false, cx));
    }
    fn launch_board_task(
        &mut self,
        draft: TaskDraft,
        composer: Entity<ComposerView>,
        open: bool,
        cx: &mut Context<Self>,
    ) {
        let key = if let Some(thread) = &draft.thread_id {
            ConversationKey::Thread(thread.clone())
        } else {
            let id = DraftId(self.next_draft_id);
            self.next_draft_id = self.next_draft_id.wrapping_add(1).max(1);
            ConversationKey::Draft(id)
        };
        self.conversation_hosts.insert(
            key.clone(),
            ConversationHost {
                composer: composer.clone(),
                cwd: draft.cwd,
                project_id: Some(draft.project_id),
            },
        );
        if open {
            self.switch_home_to(key, cx);
            self.showing_settings = false;
            self.showing_plugins = false;
            self.showing_pull_requests = false;
        }
        let marker = composer.read(cx).task_marker();
        if composer.update(cx, |c, cx| c.launch_task(draft.input.goal, cx)) {
            let client = composer.read(cx).task_submission_id(marker);
            self.workspace_store.update_tasks(|p| {
                if let Some(saved) = p.drafts.iter_mut().find(|d| d.id == draft.id) {
                    saved.client_message_id = client;
                }
            });
        } else {
            self.tasks.update(cx, |v, cx| {
                v.launch_failed(
                    &draft.id,
                    "The agent is not ready. Draft kept; retry after the connection is ready."
                        .into(),
                    cx,
                )
            });
        }
    }
    fn set_board_goal(&mut self, thread: String, objective: String, cx: &mut Context<Self>) {
        let objective = self
            .conversation_hosts
            .get(&ConversationKey::Thread(thread.clone()))
            .map(|h| h.composer.read(cx).task_input(cx).text)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(objective);
        let backend = self.agent_backend.clone();
        let reader = backend.read_thread_goal(thread.clone());
        cx.spawn(async move |this, cx| {
            let result = async {
                let read = reader.recv().await.map_err(|e| e.to_string())??;
                backend
                    .update_thread_goal(crate::agent::AgentThreadGoalUpdate {
                        generation: read.generation,
                        thread_id: thread,
                        objective: Some(
                            crate::agent::prepare_objective(
                                &objective,
                                &crate::agent::codex_home().ok_or("CODEX_HOME is unavailable")?,
                            )
                            .map_err(|e| e.to_string())?,
                        ),
                        status: Some(crate::agent::AgentThreadGoalStatus::Paused),
                        token_budget: crate::agent::AgentOptionalField::Unspecified,
                    })
                    .recv()
                    .await
                    .map_err(|e| e.to_string())?
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.tasks.update(cx, |v, cx| {
                    v.show_message(
                        match result {
                            Ok(_) => {
                                "Goal saved paused. Open the chat and resume the goal to start it."
                                    .into()
                            }
                            Err(e) => format!("Could not set goal: {e}"),
                        },
                        cx,
                    )
                });
            });
        })
        .detach();
    }
}
