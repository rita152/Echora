use super::{TaskAction, TasksView};
use crate::{
    tasks::{Card, Column, Priority, TaskDraft, TaskInput, Todo},
    theme::Theme,
};
use gpui::{Context, Focusable, MouseButton, Role, SharedString, div, prelude::*, px};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum DetailMenu {
    Due,
    Project,
    Priority,
}
#[derive(Clone)]
pub(super) enum TodoAction {
    Close,
    Toggle(String),
    Open(String),
    Delegate(String),
    Stop(String),
    Menu(DetailMenu),
    Due(Option<String>),
    Project(Option<String>),
    Priority(Priority),
    SaveDate,
}

impl TasksView {
    pub(super) fn add_todo(&mut self, title: String, cx: &mut Context<Self>) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        let todo = Todo {
            id: crate::tasks::new_id(),
            title: title.into(),
            notes: String::new(),
            due_date: None,
            priority: Priority::None,
            project_id: None,
            completed: false,
            completed_at: None,
            thread_id: None,
            created_at: chrono::Utc::now().timestamp(),
        };
        self.store.update_tasks(|p| p.todos.push(todo));
        self.quick_add
            .update(cx, |i, cx| i.set_text_silently("", cx));
        cx.notify();
    }
    pub(super) fn save_todo_details(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .selected
            .as_deref()
            .and_then(|s| s.strip_prefix("todo:"))
        else {
            return;
        };
        let title = self.todo_title.read(cx).text().trim().to_owned();
        let notes = self.todo_notes.read(cx).text().to_owned();
        self.store.update_tasks(|p| {
            if let Some(t) = p.todos.iter_mut().find(|t| t.id == id) {
                if !title.is_empty() {
                    t.title = title;
                }
                t.notes = notes;
            }
        });
    }
    pub(super) fn schedule_todo_save(&mut self, cx: &mut Context<Self>) {
        self.todo_revision = self.todo_revision.wrapping_add(1);
        let revision = self.todo_revision;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(350))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.todo_revision == revision {
                    this.save_todo_details(cx);
                }
            });
        })
        .detach();
    }
    fn select_todo(&mut self, todo: &Todo, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(format!("todo:{}", todo.id).as_str()) {
            return;
        }
        if self.todo_rename.is_some() {
            self.commit_rename(self.rename_input.read(cx).text().to_owned(), cx);
        }
        self.save_todo_details(cx);
        self.todo_revision = self.todo_revision.wrapping_add(1);
        self.selected = Some(format!("todo:{}", todo.id));
        self.detail_menu = None;
        self.todo_title
            .update(cx, |i, cx| i.set_text_silently(todo.title.clone(), cx));
        self.todo_notes
            .update(cx, |i, cx| i.set_text_silently(&todo.notes, cx));
        self.todo_date.update(cx, |i, cx| {
            i.set_text_silently(todo.due_date.clone().unwrap_or_default(), cx)
        });
        cx.notify();
    }
    pub(super) fn handoff_todo(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.save_todo_details(cx);
        let snapshot = self.store.snapshot();
        let todo = id
            .as_ref()
            .and_then(|id| {
                snapshot
                    .preferences
                    .tasks
                    .todos
                    .iter()
                    .find(|t| &t.id == id)
            })
            .cloned();
        if let Some(link) = todo.as_ref().and_then(|t| t.thread_id.clone()) {
            if snapshot
                .preferences
                .tasks
                .drafts
                .iter()
                .any(|d| d.id == link)
            {
                cx.emit(TaskAction::OpenDraft(link));
            } else {
                cx.emit(TaskAction::OpenThread(link));
            }
            return;
        }
        let title = todo
            .as_ref()
            .map(|t| t.title.clone())
            .unwrap_or_else(|| self.quick_add.read(cx).text().to_owned());
        if title.trim().is_empty() {
            return;
        }
        let Some(project) = todo
            .as_ref()
            .and_then(|t| {
                snapshot
                    .projects
                    .iter()
                    .find(|p| Some(&p.project_id) == t.project_id.as_ref())
            })
            .or_else(|| snapshot.projects.first())
        else {
            self.show_message(
                "Add a project before handing a task to an agent.".into(),
                cx,
            );
            return;
        };
        let Some(cwd) = project.roots.first() else {
            return;
        };
        let prompt = todo
            .as_ref()
            .filter(|t| !t.notes.trim().is_empty())
            .map_or_else(|| title.clone(), |t| format!("{}\n\n{}", title, t.notes));
        let draft = TaskDraft {
            id: crate::tasks::new_id(),
            project_id: project.project_id.clone(),
            cwd: cwd.clone(),
            thread_id: None,
            client_message_id: None,
            created_at: chrono::Utc::now().timestamp(),
            input: TaskInput {
                text: prompt,
                ..Default::default()
            },
        };
        self.store.update_tasks(|p| {
            if let Some(id) = id {
                if let Some(t) = p.todos.iter_mut().find(|t| t.id == id) {
                    t.thread_id = Some(draft.id.clone());
                }
            } else {
                p.todos.push(Todo {
                    id: crate::tasks::new_id(),
                    title,
                    notes: String::new(),
                    due_date: None,
                    priority: Priority::None,
                    project_id: Some(project.project_id.clone()),
                    completed: false,
                    completed_at: None,
                    thread_id: Some(draft.id.clone()),
                    created_at: draft.created_at,
                });
            }
            p.drafts.push(draft.clone());
        });
        self.quick_add
            .update(cx, |i, cx| i.set_text_silently("", cx));
        cx.emit(TaskAction::OpenDraft(draft.id));
        cx.notify();
    }
    pub(super) fn todo_card(&self, todo: &Todo) -> Card {
        Card {
            id: format!("todo:{}", todo.id),
            thread_id: todo.thread_id.clone(),
            draft_id: None,
            project_id: todo.project_id.clone().unwrap_or_default(),
            title: todo.title.clone(),
            cwd: Default::default(),
            branch: None,
            timestamp: todo.created_at,
            column: if todo.completed {
                Column::Done
            } else {
                Column::Draft
            },
            attention: None,
            needs_review: false,
            pinned: false,
        }
    }
    pub(super) fn open_todo(&mut self, todo: &Todo, cx: &mut Context<Self>) {
        self.save_todo_details(cx);
        if let Some(id) = &todo.thread_id {
            if self
                .store
                .snapshot()
                .preferences
                .tasks
                .drafts
                .iter()
                .any(|d| &d.id == id)
            {
                cx.emit(TaskAction::OpenDraft(id.clone()));
            } else {
                cx.emit(TaskAction::OpenThread(id.clone()));
            }
        }
    }
    pub(super) fn toggle_todo(&mut self, id: &str) {
        self.store.update_tasks(|p| {
            if let Some(t) = p.todos.iter_mut().find(|t| t.id == id) {
                t.completed = !t.completed;
                t.completed_at = t.completed.then(|| chrono::Utc::now().timestamp());
            }
        });
    }
    pub(super) fn todo_action(&mut self, action: TodoAction, cx: &mut Context<Self>) {
        match action {
            TodoAction::Close => {
                self.save_todo_details(cx);
                self.selected = None;
                self.detail_menu = None;
            }
            TodoAction::Toggle(id) => self.toggle_todo(&id),
            TodoAction::Open(id) => {
                if let Some(t) = self
                    .store
                    .snapshot()
                    .preferences
                    .tasks
                    .todos
                    .iter()
                    .find(|t| t.id == id)
                {
                    self.open_todo(t, cx);
                }
            }
            TodoAction::Delegate(id) => self.handoff_todo(Some(id), cx),
            TodoAction::Stop(id) => cx.emit(TaskAction::StopThread(id)),
            TodoAction::Menu(menu) => {
                self.detail_menu = if self.detail_menu == Some(menu) {
                    None
                } else {
                    Some(menu)
                }
            }
            TodoAction::SaveDate => {
                let text = self.todo_date.read(cx).text().trim();
                let date = if text.is_empty() {
                    None
                } else if chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok() {
                    Some(text.to_owned())
                } else {
                    self.show_message("Use a valid date in YYYY-MM-DD format.".into(), cx);
                    return;
                };
                self.todo_action(TodoAction::Due(date), cx);
            }
            metadata => {
                if let Some(id) = self
                    .selected
                    .as_deref()
                    .and_then(|s| s.strip_prefix("todo:"))
                {
                    self.store.update_tasks(|p| {
                        if let Some(t) = p.todos.iter_mut().find(|t| t.id == id) {
                            match metadata {
                                TodoAction::Due(date) => t.due_date = date,
                                TodoAction::Project(id) => t.project_id = id,
                                TodoAction::Priority(priority) => t.priority = priority,
                                _ => {}
                            }
                        }
                    });
                }
                self.detail_menu = None;
            }
        }
        cx.notify();
    }
    fn todo_control(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        action: TodoAction,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let label = label.into();
        let key = action.clone();
        div()
            .id(id.into())
            .role(Role::Button)
            .aria_label(label.clone())
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .rounded(px(6.))
            .px(px(8.))
            .py(px(4.))
            .text_size(px(12.))
            .text_color(theme.text_secondary)
            .hover(move |s| s.bg(theme.sidebar_hover))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.todo_action(action.clone(), cx);
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(move |this, e: &gpui::KeyDownEvent, _, cx| {
                if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                    this.todo_action(key.clone(), cx);
                    cx.stop_propagation();
                }
            }))
    }
    pub(super) fn todo_details(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let snapshot = self.store.snapshot();
        let id = self.selected.as_deref()?.strip_prefix("todo:")?;
        let todo = snapshot
            .preferences
            .tasks
            .todos
            .iter()
            .find(|t| t.id == id)?;
        let thread = todo.thread_id.as_deref().and_then(|id| snapshot.thread(id));
        let running = thread
            .is_some_and(|t| matches!(t.activity, crate::agent::ThreadActivity::Active { .. }));
        let project = todo
            .project_id
            .as_ref()
            .and_then(|id| snapshot.projects.iter().find(|p| &p.project_id == id))
            .map_or("Project".to_owned(), |p| p.name.clone());
        let mut details = div()
            .id("todo-details")
            .role(Role::Group)
            .aria_label("Task details")
            .w(px(352.))
            .max_w_full()
            .max_h_full()
            .overflow_y_scroll()
            .flex_none()
            .m(px(16.))
            .p(px(16.))
            .rounded(px(24.))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.control_soft)
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(self.todo_title.clone())
                    .child(
                        self.todo_control("todo-close", "×", TodoAction::Close, theme, cx)
                            .aria_label("Close task details"),
                    ),
            )
            .child(
                div()
                    .h(px((self.todo_notes.read(cx).visual_line_count() as f32
                        * 20.)
                        .clamp(24., 140.)))
                    .child(self.todo_notes.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.))
                    .child(self.todo_control(
                        "todo-due",
                        todo.due_date.clone().unwrap_or_else(|| "Due day".into()),
                        TodoAction::Menu(DetailMenu::Due),
                        theme,
                        cx,
                    ))
                    .child(self.todo_control(
                        "todo-project",
                        project,
                        TodoAction::Menu(DetailMenu::Project),
                        theme,
                        cx,
                    ))
                    .child(self.todo_control(
                        "todo-priority",
                        todo.priority.label(),
                        TodoAction::Menu(DetailMenu::Priority),
                        theme,
                        cx,
                    )),
            );
        match self.detail_menu {
            Some(DetailMenu::Due) => {
                let today = chrono::Local::now().date_naive();
                let next_monday =
                    7 - chrono::Datelike::weekday(&today).num_days_from_monday() as i64;
                details = details
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .children(
                                [("Today", 0), ("Tomorrow", 1), ("Next week", next_monday)].map(
                                    |(label, days)| {
                                        self.todo_control(
                                            SharedString::from(format!("due-{label}")),
                                            label,
                                            TodoAction::Due(Some(
                                                (today + chrono::Duration::days(days)).to_string(),
                                            )),
                                            theme,
                                            cx,
                                        )
                                    },
                                ),
                            )
                            .child(self.todo_control(
                                "due-clear",
                                "Clear due day",
                                TodoAction::Due(None),
                                theme,
                                cx,
                            )),
                    )
                    .child(self.todo_date.clone())
                    .child(self.todo_control(
                        "due-save",
                        "Save date",
                        TodoAction::SaveDate,
                        theme,
                        cx,
                    ));
            }
            Some(DetailMenu::Priority) => {
                details = details.child(div().flex().flex_wrap().children(Priority::ALL.map(|p| {
                    self.todo_control(
                        SharedString::from(format!("priority-{}", p.label())),
                        p.label(),
                        TodoAction::Priority(p),
                        theme,
                        cx,
                    )
                })))
            }
            Some(DetailMenu::Project) => {
                details = details.child(
                    div()
                        .id("todo-project-menu")
                        .max_h(px(180.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .child(self.todo_control(
                            "todo-project-none",
                            "No project",
                            TodoAction::Project(None),
                            theme,
                            cx,
                        ))
                        .children(snapshot.projects.iter().map(|p| {
                            self.todo_control(
                                SharedString::from(format!("todo-project-{}", p.project_id)),
                                p.name.clone(),
                                TodoAction::Project(Some(p.project_id.clone())),
                                theme,
                                cx,
                            )
                        })),
                )
            }
            None => {}
        }
        if let Some(thread) = thread {
            details = details.child(
                div()
                    .id("todo-reply-preview")
                    .rounded(px(16.))
                    .p(px(12.))
                    .bg(theme.sidebar_hover)
                    .text_size(px(13.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_color(theme.text_secondary)
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                crate::components::icons::icon(
                                    "codex",
                                    theme.text_secondary.into(),
                                )
                                .size(px(14.)),
                            )
                            .child(if running {
                                "Working…".into()
                            } else {
                                format!(
                                    "{} · {}",
                                    todo_status(todo, &snapshot).1,
                                    crate::tasks::relative_time(
                                        snapshot
                                            .task_runs
                                            .get(&thread.thread_id)
                                            .map_or(thread.updated_at, |r| r
                                                .last_activity
                                                .max(thread.updated_at)),
                                        chrono::Utc::now().timestamp()
                                    )
                                )
                            }),
                    )
                    .when_some(
                        snapshot
                            .task_runs
                            .get(&thread.thread_id)
                            .filter(|r| !r.reply.is_empty()),
                        |s, run| {
                            s.child(
                                div()
                                    .id("todo-reply-text")
                                    .max_h(px(200.))
                                    .overflow_y_scroll()
                                    .child(run.reply.clone()),
                            )
                        },
                    ),
            );
            details = details.child(self.todo_reply.clone());
            details = details.child(div().flex().justify_end().when(running, |s| {
                s.child(self.todo_control(
                    "todo-stop",
                    "Stop",
                    TodoAction::Stop(thread.thread_id.clone()),
                    theme,
                    cx,
                ))
            }));
        } else if todo.thread_id.is_some() {
            details = details.child(self.todo_control(
                "todo-open-draft",
                "Open linked draft",
                TodoAction::Open(todo.id.clone()),
                theme,
                cx,
            ));
        } else {
            details = details.child(self.todo_control(
                "todo-delegate",
                "Hand it to an agent…",
                TodoAction::Delegate(todo.id.clone()),
                theme,
                cx,
            ));
        }
        Some(
            details
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .when(
                            todo.thread_id
                                .as_deref()
                                .and_then(|id| snapshot.thread(id))
                                .is_some(),
                            |s| {
                                s.child(self.todo_control(
                                    "todo-open-chat",
                                    "Open chat",
                                    TodoAction::Open(todo.id.clone()),
                                    theme,
                                    cx,
                                ))
                            },
                        )
                        .child(
                            self.todo_control(
                                "todo-complete",
                                if todo.completed {
                                    "Mark as not done"
                                } else {
                                    "Mark as done"
                                },
                                TodoAction::Toggle(todo.id.clone()),
                                theme,
                                cx,
                            )
                            .bg(theme.button)
                            .text_color(theme.button_text)
                            .rounded_full(),
                        ),
                )
                .into_any_element(),
        )
    }
    pub(super) fn list_view(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let snapshot = self.store.snapshot();
        let mut todos = snapshot.preferences.tasks.todos.iter().collect::<Vec<_>>();
        todos.sort_by(|a, b| {
            todo_status(a, &snapshot)
                .0
                .cmp(&todo_status(b, &snapshot).0)
                .then_with(|| {
                    if a.completed {
                        b.completed_at.cmp(&a.completed_at)
                    } else {
                        b.priority
                            .cmp(&a.priority)
                            .then_with(|| a.due_date.is_none().cmp(&b.due_date.is_none()))
                            .then_with(|| a.due_date.cmp(&b.due_date))
                            .then_with(|| a.created_at.cmp(&b.created_at))
                    }
                })
        });
        let needs = todos
            .iter()
            .filter(|t| todo_status(t, &snapshot).0 == 0)
            .count();
        let done = todos.iter().filter(|t| t.completed).count();
        let mut completed_shown = 0;
        let mut group = None;
        let mut rows = div()
            .id("todo-rows")
            .role(Role::List)
            .aria_label("To do")
            .flex()
            .flex_col()
            .gap(px(2.));
        let multiple_groups = todos
            .iter()
            .filter(|t| !t.completed)
            .map(|t| todo_status(t, &snapshot).0)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1;
        for todo in todos {
            let (rank, status) = todo_status(todo, &snapshot);
            if todo.completed {
                completed_shown += 1;
                if !self.show_completed && completed_shown > 2 {
                    continue;
                }
            }
            if multiple_groups && group != Some(rank) {
                group = Some(rank);
                rows = rows.child(
                    div()
                        .mt(px(16.))
                        .px(px(12.))
                        .text_size(px(12.))
                        .text_color(theme.text_tertiary)
                        .child(match rank {
                            0 => "Needs you",
                            1 => "Running",
                            2 => "To do",
                            _ => "Completed",
                        }),
                );
            }
            let id = todo.id.clone();
            let toggle = id.clone();
            let key_id = id.clone();
            let selected = todo.clone();
            let secondary = todo.clone();
            let open_task = todo.clone();
            let linked_task = todo.clone();
            let mut label = todo.title.clone();
            if let Some(date) = &todo.due_date {
                label.push_str(&format!(" · {date}"));
            }
            if todo.priority != Priority::None {
                label.push_str(&format!(" · {}", todo.priority.label()));
            }
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("todo-row-{id}")))
                    .min_h(px(42.))
                    .px(px(12.))
                    .rounded(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .focusable()
                    .tab_stop(true)
                    .role(Role::ListItem)
                    .aria_label(todo.title.clone())
                    .when(
                        self.selected.as_deref() == Some(format!("todo:{id}").as_str()),
                        |s| s.bg(theme.sidebar_hover),
                    )
                    .hover(move |s| s.bg(theme.text.alpha(0.03)))
                    .on_click(cx.listener(move |this, e: &gpui::ClickEvent, window, cx| {
                        this.select_todo(&selected, cx);
                        if e.click_count() == 2 {
                            this.todo_rename = Some(selected.id.clone());
                            this.rename_input.update(cx, |i, cx| {
                                i.set_text_silently(selected.title.clone(), cx)
                            });
                            this.rename_input
                                .read(cx)
                                .focus_handle(cx)
                                .focus(window, cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, e: &gpui::MouseDownEvent, window, cx| {
                            this.menu = Some((this.todo_card(&secondary), e.position));
                            this.menu_index = 0;
                            this.menu_focus.focus(window, cx);
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    )
                    .on_key_down(
                        cx.listener(move |this, e: &gpui::KeyDownEvent, window, cx| {
                            if this.todo_rename.as_ref() == Some(&key_id) {
                                cx.propagate();
                                return;
                            }
                            match e.keystroke.key.as_str() {
                                "space" => this.toggle_todo(&key_id),
                                "enter" => this.select_todo(&open_task, cx),
                                "f2" => {
                                    this.todo_rename = Some(key_id.clone());
                                    this.rename_input.update(cx, |i, cx| {
                                        i.set_text_silently(open_task.title.clone(), cx)
                                    });
                                    this.rename_input
                                        .read(cx)
                                        .focus_handle(cx)
                                        .focus(window, cx);
                                }
                                "backspace" | "delete" => {
                                    this.confirmation = Some(this.todo_card(&open_task));
                                    this.menu_focus.focus(window, cx);
                                }
                                "tab" if !e.keystroke.modifiers.shift => {
                                    this.handoff_todo(Some(key_id.clone()), cx)
                                }
                                _ => {
                                    cx.propagate();
                                    return;
                                }
                            }
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("todo-toggle-{id}")))
                            .role(Role::CheckBox)
                            .aria_label(format!(
                                "Mark as {}: {}",
                                if todo.completed { "not done" } else { "done" },
                                todo.title
                            ))
                            .aria_toggled(if todo.completed {
                                gpui::Toggled::True
                            } else {
                                gpui::Toggled::False
                            })
                            .size(px(18.))
                            .text_color(theme.text_tertiary)
                            .cursor_pointer()
                            .child(if todo.completed { "✓" } else { "◯" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_todo(&toggle);
                                cx.stop_propagation();
                            })),
                    )
                    .child(
                        div()
                            .min_w(px(0.))
                            .truncate()
                            .text_size(px(14.))
                            .text_color(if todo.completed {
                                theme.text_tertiary
                            } else {
                                theme.text
                            })
                            .when(self.todo_rename.as_ref() != Some(&id), |s| s.child(label))
                            .when(self.todo_rename.as_ref() == Some(&id), |s| {
                                s.child(
                                    div()
                                        .id("todo-inline-rename")
                                        .min_w(px(180.))
                                        .child(self.rename_input.clone())
                                        .on_click(|_, _, cx| cx.stop_propagation()),
                                )
                            }),
                    )
                    .when(!status.is_empty(), |s| {
                        s.child(
                            div()
                                .text_size(px(12.))
                                .text_color(theme.effort)
                                .child(status),
                        )
                    })
                    .child(div().flex_1())
                    .when(todo.thread_id.is_some(), |s| {
                        s.child(
                            div()
                                .id(SharedString::from(format!("todo-open-{id}")))
                                .role(Role::Button)
                                .aria_label("Open linked task")
                                .cursor_pointer()
                                .child(
                                    crate::components::icons::icon(
                                        "codex",
                                        theme.text_secondary.into(),
                                    )
                                    .size(px(14.)),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_todo(&linked_task, cx);
                                    cx.stop_propagation();
                                })),
                        )
                    }),
            );
        }
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .items_start()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .id("task-list-scroll")
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .w(px(540.))
                            .max_w_full()
                            .px(px(12.))
                            .pt(px(32.))
                            .flex()
                            .flex_col()
                            .gap(px(24.))
                            .child(
                                div()
                                    .px(px(12.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_size(px(24.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("Tasks"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(theme.text_tertiary)
                                            .child(if needs > 0 {
                                                format!(
                                                    "{needs} {} needs you",
                                                    if needs == 1 { "thing" } else { "things" }
                                                )
                                            } else if done > 0
                                                && snapshot
                                                    .preferences
                                                    .tasks
                                                    .todos
                                                    .iter()
                                                    .all(|t| t.completed)
                                            {
                                                "All done".into()
                                            } else {
                                                "Add anything you need to do".into()
                                            }),
                                    ),
                            )
                            .child(rows)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .px(px(12.))
                                    .child("◌")
                                    .child(self.quick_add.clone())
                                    .capture_key_down(cx.listener(
                                        |this, e: &gpui::KeyDownEvent, _, cx| {
                                            if e.keystroke.key == "tab"
                                                && !e.keystroke.modifiers.shift
                                                && !this.quick_add.read(cx).text().trim().is_empty()
                                            {
                                                this.handoff_todo(None, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    )),
                            )
                            .when(done > 2, |s| {
                                s.child(
                                    div()
                                        .id("todos-completed")
                                        .role(Role::Button)
                                        .aria_label("Show completed tasks")
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .text_size(px(13.))
                                        .text_color(theme.text_tertiary)
                                        .child(if self.show_completed {
                                            "Show fewer".into()
                                        } else {
                                            format!("Show {} completed", done - 2)
                                        })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.show_completed = !this.show_completed;
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
            )
            .children(self.todo_details(theme, cx))
            .into_any_element()
    }
}
fn todo_status(todo: &Todo, snapshot: &crate::workspace::WorkspaceSnapshot) -> (u8, &'static str) {
    if todo.completed {
        return (3, "");
    }
    if let Some(id) = &todo.thread_id {
        if let Some(thread) = snapshot.thread(id) {
            if !matches!(thread.activity, crate::agent::ThreadActivity::Active { .. })
                && snapshot.task_runs.get(id).is_some_and(|r| r.failed)
            {
                return (0, "Failed");
            }
            return match &thread.activity {
                crate::agent::ThreadActivity::Active { flags } if !flags.is_empty() => {
                    (0, "Needs you")
                }
                crate::agent::ThreadActivity::Active { .. } => (1, "Working"),
                crate::agent::ThreadActivity::SystemError => (0, "Failed"),
                _ => (0, "Ready for you"),
            };
        }
        if snapshot
            .preferences
            .tasks
            .drafts
            .iter()
            .any(|d| &d.id == id)
        {
            return (2, "Draft");
        }
        return (0, "Chat unavailable");
    }
    (2, "")
}
