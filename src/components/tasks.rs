//! Native task board. Runtime work stays in the application's conversation hosts.
mod list;
mod menus;
mod render;
#[cfg(test)]
mod tests;

use super::{composer::ComposerView, prompt_input::PromptInput};
use crate::{
    tasks::{Card, Column, TaskDraft, TaskView},
    theme::ThemeMode,
    workspace::WorkspaceStore,
};
use gpui::{Context, Entity, FocusHandle, ScrollHandle, prelude::*};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskRoute {
    pub project: Option<String>,
    pub list: bool,
}
pub enum TaskAction {
    New {
        project: Option<String>,
        draft: bool,
        existing: Option<String>,
    },
    OpenThread(String),
    StopThread(String),
    ReplyThread {
        thread: String,
        text: String,
    },
    OpenDraft(String),
    Launch {
        draft: Box<TaskDraft>,
        composer: Entity<ComposerView>,
        open: bool,
    },
    SetGoal {
        thread: String,
        objective: String,
    },
    RouteChanged {
        from: TaskRoute,
        to: TaskRoute,
    },
}
pub struct TaskEditor {
    pub draft: TaskDraft,
    pub composer: Entity<ComposerView>,
    pub send_as_draft: bool,
    pub goal: bool,
    pub project_menu: bool,
    pub previous_focus: Option<FocusHandle>,
    _submission: gpui::Subscription,
    _observation: gpui::Subscription,
}
struct PendingTask {
    composer: Entity<ComposerView>,
    attempt: String,
    dispatched: bool,
    _subscription: Option<gpui::Subscription>,
    _readiness: Option<gpui::Task<()>>,
}

pub struct TasksView {
    store: Arc<WorkspaceStore>,
    mode: ThemeMode,
    route: TaskRoute,
    titlebar_inset: f32,
    scroll: ScrollHandle,
    column_scrolls: HashMap<String, ScrollHandle>,
    focus: FocusHandle,
    focus_pending: bool,
    pub editor: Option<TaskEditor>,
    menu: Option<(Card, gpui::Point<gpui::Pixels>)>,
    menu_focus: FocusHandle,
    menu_index: usize,
    copy_menu: bool,
    page_origin: std::rc::Rc<std::cell::Cell<gpui::Point<gpui::Pixels>>>,
    rename: Option<Card>,
    rename_input: Entity<PromptInput>,
    confirmation: Option<Card>,
    selected: Option<String>,
    message: Option<String>,
    launching: HashMap<String, PendingTask>,
    requested_runs: HashMap<String, (i64, crate::agent::ThreadActivity)>,
    editor_focus: FocusHandle,
    requested_prs: HashSet<String>,
    quick_add: Entity<PromptInput>,
    todo_rename: Option<String>,
    show_completed: bool,
    todo_title: Entity<PromptInput>,
    todo_notes: Entity<super::file_editor::FileEditor>,
    todo_date: Entity<PromptInput>,
    todo_revision: u64,
    todo_reply: Entity<PromptInput>,
    detail_menu: Option<list::DetailMenu>,
}
impl gpui::EventEmitter<TaskAction> for TasksView {}
impl TasksView {
    pub fn is_launching(&self, id: &str) -> bool {
        self.launching.contains_key(id)
    }
    pub fn show_message(&mut self, message: String, cx: &mut Context<Self>) {
        self.message = Some(message);
        cx.notify();
    }
    pub fn new(mode: ThemeMode, store: Arc<WorkspaceStore>, cx: &mut Context<Self>) -> Self {
        let preferences = store.snapshot().preferences.tasks;
        let route = TaskRoute {
            project: None,
            list: preferences.view == TaskView::List,
        };
        let receiver = store.subscribe();
        cx.spawn(async move |this, cx| {
            while receiver.recv().await.is_ok() {
                if this
                    .update(cx, |this, cx| {
                        if this.route.project.as_ref().is_some_and(|id| {
                            !this
                                .store
                                .snapshot()
                                .projects
                                .iter()
                                .any(|p| &p.project_id == id)
                        }) && !this.store.snapshot().loading.projects
                        {
                            this.route.project = None;
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(30))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        this.requested_prs.clear();
                        let snapshot = this.store.snapshot();
                        this.requested_runs.retain(|id, _| {
                            snapshot.thread(id).is_some_and(|t| {
                                !matches!(t.activity, crate::agent::ThreadActivity::Active { .. })
                            }) && snapshot.task_runs.get(id).is_none_or(|r| r.error.is_none())
                        });
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let rename_input = cx.new(|cx| PromptInput::rename_chat(mode, "", cx));
        cx.subscribe(
            &rename_input,
            |this, input, _: &super::prompt_input::PromptSubmitted, cx| {
                this.commit_rename(input.read(cx).text().to_owned(), cx);
            },
        )
        .detach();
        let quick_add = cx.new(|cx| {
            PromptInput::project_name(mode, "Add a task, or press Tab to hand it to an agent", cx)
        });
        cx.subscribe(
            &quick_add,
            |this, input, _: &super::prompt_input::PromptSubmitted, cx| {
                this.add_todo(input.read(cx).text().to_owned(), cx);
            },
        )
        .detach();
        let todo_title = cx.new(|cx| PromptInput::project_name(mode, "Task title", cx));
        let todo_notes = cx.new(|cx| super::file_editor::FileEditor::prose(mode, "Notes", cx));
        let todo_reply = cx.new(|cx| PromptInput::project_name(mode, "Reply to agent", cx));
        cx.subscribe(
            &todo_reply,
            |this, input, _: &super::prompt_input::PromptSubmitted, cx| {
                let text = input.read(cx).text().trim().to_owned();
                if text.is_empty() {
                    return;
                }
                if let Some(todo) = this
                    .selected
                    .as_deref()
                    .and_then(|s| s.strip_prefix("todo:"))
                    .and_then(|id| {
                        this.store
                            .snapshot()
                            .preferences
                            .tasks
                            .todos
                            .into_iter()
                            .find(|t| t.id == id)
                    })
                    && let Some(thread) = todo.thread_id
                {
                    cx.emit(TaskAction::ReplyThread { thread, text });
                }
            },
        )
        .detach();
        let todo_date = cx.new(|cx| PromptInput::project_name(mode, "YYYY-MM-DD", cx));
        cx.subscribe(
            &todo_date,
            |this, _, _: &super::prompt_input::PromptSubmitted, cx| {
                this.todo_action(list::TodoAction::SaveDate, cx)
            },
        )
        .detach();
        cx.subscribe(
            &todo_title,
            |this, _, _: &super::prompt_input::PromptChanged, cx| this.schedule_todo_save(cx),
        )
        .detach();
        cx.subscribe(
            &todo_notes,
            |this, _, event: &super::file_editor::EditorEvent, cx| {
                if matches!(event, super::file_editor::EditorEvent::Changed) {
                    this.schedule_todo_save(cx);
                }
            },
        )
        .detach();
        Self {
            store,
            mode,
            route,
            titlebar_inset: 0.,
            scroll: ScrollHandle::new(),
            column_scrolls: HashMap::new(),
            focus: cx.focus_handle().tab_stop(true),
            focus_pending: false,
            editor: None,
            menu: None,
            menu_focus: cx.focus_handle(),
            menu_index: 0,
            copy_menu: false,
            page_origin: Default::default(),
            rename: None,
            rename_input,
            confirmation: None,
            selected: None,
            message: None,
            launching: HashMap::new(),
            requested_runs: HashMap::new(),
            editor_focus: cx.focus_handle(),
            requested_prs: HashSet::new(),
            quick_add,
            todo_rename: None,
            show_completed: false,
            todo_title,
            todo_notes,
            todo_date,
            todo_revision: 0,
            todo_reply,
            detail_menu: None,
        }
    }
    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.todo_reply.update(cx, |i, cx| i.set_mode(mode, cx));
        self.todo_title.update(cx, |i, cx| i.set_mode(mode, cx));
        self.todo_notes.update(cx, |i, cx| i.set_mode(mode, cx));
        self.todo_date.update(cx, |i, cx| i.set_mode(mode, cx));
        self.rename_input.update(cx, |i, cx| i.set_mode(mode, cx));
        self.quick_add.update(cx, |i, cx| i.set_mode(mode, cx));
        if let Some(e) = &self.editor {
            e.composer.update(cx, |c, cx| c.set_mode(mode, cx));
        }
        cx.notify();
    }
    pub fn reply_sent(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.todo_reply.read(cx).text().trim() == text.trim() {
            self.todo_reply
                .update(cx, |i, cx| i.set_text_silently("", cx));
        }
        self.show_message("Reply sent".into(), cx);
    }
    pub fn park(&mut self, cx: &mut Context<Self>) {
        self.save_todo_details(cx);
        if let Some(e) = self.editor.take() {
            e.composer.update(cx, |c, cx| c.close_side_menus(cx));
        }
        self.menu = None;
        self.rename = None;
        self.confirmation = None;
        cx.notify();
    }
    pub fn route(&self) -> TaskRoute {
        self.route.clone()
    }
    pub fn set_route(&mut self, route: TaskRoute, cx: &mut Context<Self>) {
        self.route = route;
        self.menu = None;
        self.focus_pending = true;
        cx.notify();
    }
    pub fn set_titlebar_inset(&mut self, inset: f32) {
        self.titlebar_inset = inset.max(0.);
    }
    fn navigate(&mut self, route: TaskRoute, cx: &mut Context<Self>) {
        self.save_todo_details(cx);
        let from = self.route.clone();
        if from == route {
            return;
        }
        self.store.update_tasks(|p| {
            p.view = if route.list {
                TaskView::List
            } else {
                TaskView::Kanban
            }
        });
        self.set_route(route.clone(), cx);
        cx.emit(TaskAction::RouteChanged { from, to: route });
    }
    fn new_task(&mut self, project: Option<String>, draft: bool, cx: &mut Context<Self>) {
        cx.emit(TaskAction::New {
            project,
            draft,
            existing: None,
        });
    }
    pub fn show_editor(
        &mut self,
        draft: TaskDraft,
        composer: Entity<ComposerView>,
        send_as_draft: bool,
        cx: &mut Context<Self>,
    ) {
        composer.update(cx, |c, cx| c.set_task_editor(true, cx));
        let id = draft.id.clone();
        let submission = cx.subscribe(
            &composer,
            move |this, _, _: &super::composer::SubmitTaskEditor, cx| {
                if this.editor.as_ref().is_some_and(|e| e.draft.id == id) {
                    this.submit_editor(cx);
                }
            },
        );
        let goal = draft.input.goal;
        let observation = cx.observe(&composer, |_, _, cx| cx.notify());
        self.editor = Some(TaskEditor {
            draft,
            composer,
            send_as_draft,
            goal,
            project_menu: false,
            previous_focus: None,
            _submission: submission,
            _observation: observation,
        });
        self.menu = None;
        self.message = None;
        cx.notify();
    }
    fn close_editor(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        if let Some(e) = self.editor.take() {
            // Closing an edit keeps its saved card; cancelling a new editor creates no card.
            if let Some(focus) = e.previous_focus {
                focus.focus(window, cx);
            } else {
                self.focus.focus(window, cx);
            }
            e.composer.update(cx, |c, cx| c.close_side_menus(cx));
        }
        cx.notify();
    }
    fn submit_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let mut draft = editor.draft.clone();
        draft.input = editor.composer.read(cx).task_input(cx);
        draft.input.goal = editor.goal;
        if draft.input.is_empty()
            || (!editor.send_as_draft && !editor.composer.read(cx).task_can_send(cx))
        {
            return;
        }
        let composer = editor.composer.clone();
        let save_only = editor.send_as_draft;
        self.store.update_tasks(|p| {
            p.drafts.retain(|d| d.id != draft.id);
            p.drafts.push(draft.clone());
            for todo in &mut p.todos {
                if todo.thread_id.as_deref() == Some(draft.id.as_str()) {
                    todo.project_id = Some(draft.project_id.clone());
                }
            }
        });
        self.editor = None;
        self.focus_pending = true;
        if save_only {
            self.message = Some("Draft saved. Drag it to In Progress to send it.".into());
        } else {
            self.launch(draft, composer, true, cx);
        }
        cx.notify();
    }
    pub fn launch(
        &mut self,
        draft: TaskDraft,
        composer: Entity<ComposerView>,
        open: bool,
        cx: &mut Context<Self>,
    ) {
        if self.launching.contains_key(&draft.id) {
            return;
        }
        let marker = composer.read(cx).task_marker();
        let attempt = crate::tasks::new_id();
        self.launching.insert(
            draft.id.clone(),
            PendingTask {
                composer: composer.clone(),
                attempt: attempt.clone(),
                dispatched: false,
                _subscription: None,
                _readiness: None,
            },
        );
        let id = draft.id.clone();
        let observed_attempt = attempt.clone();
        let subscription = cx.subscribe(
            &composer,
            move |this, composer, _: &super::composer::ConversationChanged, cx| {
                if !this
                    .launching
                    .get(&id)
                    .is_some_and(|p| p.attempt == observed_attempt)
                {
                    return;
                }
                let thread = composer.read(cx).thread_id().map(str::to_owned);
                let client = composer.read(cx).task_submission_id(marker);
                let cwd = composer.read(cx).conversation_cwd();
                if let Some(thread) = &thread
                    && this
                        .store
                        .snapshot()
                        .preferences
                        .tasks
                        .drafts
                        .iter()
                        .any(|d| {
                            d.id == id
                                && (d.thread_id.as_ref() != Some(thread)
                                    || d.client_message_id != client
                                    || d.cwd != cwd)
                        })
                {
                    this.store.update_tasks(|p| {
                        if let Some(d) = p.drafts.iter_mut().find(|d| d.id == id) {
                            d.thread_id = Some(thread.clone());
                            d.client_message_id = client.clone();
                            d.cwd = cwd.clone();
                        }
                    });
                }
                if composer.read(cx).task_accepted(marker) {
                    this.store.update_tasks(|p| {
                        p.drafts.retain(|d| d.id != id);
                        for todo in &mut p.todos {
                            if todo.thread_id.as_deref() == Some(id.as_str()) {
                                todo.thread_id = thread.clone();
                            }
                        }
                    });
                    this.launching.remove(&id);
                    this.message = Some("Task started".into());
                } else if let Some(error) = composer.read(cx).task_error(marker) {
                    this.launch_failed(
                        &id,
                        format!(
                            "Could not send draft: {error}. Input kept; open the draft to retry."
                        ),
                        cx,
                    );
                }
                cx.notify();
            },
        );
        self.launching.get_mut(&draft.id).unwrap()._subscription = Some(subscription);
        // A restored draft can be dragged before its model/config requests finish.
        // Own the attempt while waiting, so repeated drops cannot enqueue another send.
        let readiness_id = draft.id.clone();
        let readiness = cx.spawn(async move |this, cx| {
            let mut waiting = 0;
            loop {
                let finished = this.update(cx, |this, cx| {
                    let Some(pending) = this.launching.get(&draft.id)
                        .filter(|p| p.attempt == attempt)
                    else {
                        return true;
                    };
                    // Keep entity ownership in PendingTask. Capturing an Entity in
                    // this task would keep it alive beyond the view's teardown.
                    let composer = pending.composer.clone();
                    let dispatched = pending.dispatched;
                    if let Some(error) = composer.read(cx).task_error(marker) {
                        this.launch_failed(
                            &draft.id,
                            format!("Could not send draft: {error}. Input kept."),
                            cx,
                        );
                        return true;
                    }
                    if dispatched {
                        if !composer.read(cx).task_pending(marker)
                            && !composer.read(cx).task_accepted(marker)
                        {
                            this.launch_failed(
                                &draft.id,
                                "Sending was cancelled. Draft kept; open it to edit or retry.".into(),
                                cx,
                            );
                            return true;
                        }
                    } else if composer.read(cx).task_can_send(cx) {
                        this.launching.get_mut(&draft.id).unwrap().dispatched = true;
                        cx.emit(TaskAction::Launch {
                            draft: Box::new(draft.clone()), composer, open,
                        });
                    } else {
                        waiting += 1;
                        if waiting >= 150 {
                            this.launch_failed(
                                &draft.id,
                                "The agent did not become ready. Draft kept; open it to inspect its connection and retry.".into(),
                                cx,
                            );
                            return true;
                        }
                    }
                    false
                }).unwrap_or(true);
                if finished {
                    return;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
            }
        });
        self.launching.get_mut(&readiness_id).unwrap()._readiness = Some(readiness);
        cx.notify();
    }
    pub fn launch_failed(&mut self, id: &str, error: String, cx: &mut Context<Self>) {
        self.launching.remove(id);
        self.message = Some(error);
        cx.notify();
    }
    fn open_card(&mut self, card: &Card, cx: &mut Context<Self>) {
        if let Some(id) = &card.draft_id
            && let Some(pending) = self.launching.get(id)
        {
            if let Some(thread) = pending.composer.read(cx).thread_id() {
                cx.emit(TaskAction::OpenThread(thread.to_owned()));
            } else {
                self.show_message("The task is starting. Its conversation will be available after the server creates it.".into(),cx);
            }
            return;
        }
        self.selected = Some(card.id.clone());
        self.menu = None;
        if let Some(draft) = &card.draft_id {
            cx.emit(TaskAction::OpenDraft(draft.clone()));
        } else if let Some(thread) = &card.thread_id {
            cx.emit(TaskAction::OpenThread(thread.clone()));
        }
        cx.notify();
    }
    fn move_card(
        &mut self,
        card: &Card,
        column: Column,
        target: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        if card.column != Column::Draft {
            return;
        }
        match column {
            Column::Draft => {
                let Some(to) = target else { return };
                let snapshot = self.store.snapshot();
                let cards = crate::tasks::boards(&snapshot, chrono::Utc::now().timestamp())
                    .into_iter()
                    .find(|p| p.id == card.project_id)
                    .map(|p| {
                        p.cards
                            .into_iter()
                            .filter(|c| c.column == Column::Draft)
                            .map(|c| c.id)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                self.store.update_tasks(|p| {
                    crate::tasks::reorder(
                        p.draft_order.entry(card.project_id.clone()).or_default(),
                        &cards,
                        &card.id,
                        to,
                    );
                });
            }
            Column::InProgress => {
                if let Some(id) = &card.draft_id {
                    cx.emit(TaskAction::New {
                        project: Some(card.project_id.clone()),
                        draft: false,
                        existing: Some(id.clone()),
                    });
                } else {
                    self.open_card(card, cx);
                }
            }
            Column::Done => {
                self.message = Some(
                    "Done is derived automatically. Cards move here when their runs complete."
                        .into(),
                );
            }
            Column::AwaitingYou => {
                self.message = Some(
                    "Awaiting you is derived automatically. Cards move here when they need approval, input, or a retry."
                        .into(),
                );
            }
        }
        cx.notify();
    }
    fn reorder_keyboard(&mut self, card: &Card, down: bool, cx: &mut Context<Self>) {
        let snapshot = self.store.snapshot();
        let cards = crate::tasks::boards(&snapshot, chrono::Utc::now().timestamp())
            .into_iter()
            .find(|p| p.id == card.project_id)
            .map(|p| {
                p.cards
                    .into_iter()
                    .filter(|c| c.column == Column::Draft)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(index) = cards.iter().position(|c| c.id == card.id) {
            let target = if down {
                cards.get(index + 1)
            } else {
                index.checked_sub(1).and_then(|i| cards.get(i))
            };
            if let Some(target) = target {
                self.move_card(card, Column::Draft, Some(&target.id), cx);
            }
        }
    }
}
