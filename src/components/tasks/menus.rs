use super::{TaskAction, TasksView};
use crate::{
    tasks::{Card, Column},
    theme::Theme,
};
use gpui::{Context, Focusable, MouseButton, Role, SharedString, Window, div, prelude::*, px};

#[derive(Clone, Copy, PartialEq)]
enum Command {
    Rename,
    Pin,
    Copy,
    CopyPath,
    CopyId,
    Archive,
    Goal,
    SendGoal,
    Send,
    Delete,
    TodoOpen,
    TodoUnlink,
    TodoDelegate,
    TodoToggle,
}
impl TasksView {
    fn commands(&self, card: &Card) -> Vec<(Command, String)> {
        if card
            .draft_id
            .as_deref()
            .is_some_and(|id| self.is_launching(id))
        {
            return vec![(Command::Copy, "Copy  ›".into())];
        }
        let mut items = Vec::new();
        if let Some(id) = card.id.strip_prefix("todo:") {
            if let Some(todo) = self
                .store
                .snapshot()
                .preferences
                .tasks
                .todos
                .iter()
                .find(|t| t.id == id)
            {
                items.push((Command::Rename, "Rename".into()));
                if todo.thread_id.is_some() {
                    items.push((Command::TodoOpen, "Open chat".into()));
                    items.push((Command::TodoUnlink, "Unlink chat".into()));
                } else {
                    items.push((Command::TodoDelegate, "Delegate…".into()));
                }
                items.push((
                    Command::TodoToggle,
                    if todo.completed {
                        "Mark as not done"
                    } else {
                        "Mark as done"
                    }
                    .into(),
                ));
                items.push((Command::Delete, "Delete".into()));
            }
            return items;
        }

        if card.draft_id.is_none() && card.thread_id.is_some() {
            items.push((Command::Rename, "Rename thread".into()));
            items.push((
                Command::Pin,
                if card.pinned {
                    "Unpin thread"
                } else {
                    "Pin thread"
                }
                .into(),
            ));
        }
        if card.column == Column::Draft && card.draft_id.is_some() {
            items.push((Command::Send, "Move to In Progress".into()));
        }
        items.push((Command::Copy, "Copy  ›".into()));
        if card.draft_id.is_none() && card.thread_id.is_some() {
            items.push((Command::Archive, "Archive".into()));
            items.push((Command::Goal, "Set as goal".into()));
        }
        if card.draft_id.is_some() {
            items.push((Command::SendGoal, "Send as goal".into()));
        }
        items.push((
            Command::Delete,
            if card.draft_id.is_some() {
                "Delete draft"
            } else {
                "Delete"
            }
            .into(),
        ));
        items
    }
    fn run_command(
        &mut self,
        card: Card,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let menu_position = self
            .menu
            .as_ref()
            .map(|(_, p)| *p)
            .unwrap_or(self.page_origin.get() + gpui::point(px(20.), px(80.)));
        self.menu = None;
        self.copy_menu = false;
        match command {
            Command::Rename => self.start_rename(card, window, cx),
            Command::TodoOpen => {
                if let Some(t) = self
                    .store
                    .snapshot()
                    .preferences
                    .tasks
                    .todos
                    .iter()
                    .find(|t| card.id == format!("todo:{}", t.id))
                {
                    self.open_todo(t, cx);
                }
            }
            Command::TodoUnlink => {
                self.store.update_tasks(|p| {
                    if let Some(t) = p
                        .todos
                        .iter_mut()
                        .find(|t| card.id == format!("todo:{}", t.id))
                    {
                        t.thread_id = None;
                    }
                });
            }
            Command::TodoDelegate => {
                self.handoff_todo(card.id.strip_prefix("todo:").map(str::to_owned), cx)
            }
            Command::TodoToggle => {
                if let Some(id) = card.id.strip_prefix("todo:") {
                    self.toggle_todo(id);
                }
            }

            Command::Pin => {
                if let Some(id) = card.thread_id {
                    self.store.set_thread_pinned(id, !card.pinned);
                }
            }
            Command::Copy => {
                self.menu = Some((card, menu_position));
                self.copy_menu = true;
                self.menu_index = 0;
                self.menu_focus.focus(window, cx);
            }
            Command::CopyPath => cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                card.cwd.to_string_lossy().into_owned(),
            )),
            Command::CopyId => {
                if let Some(id) = card.thread_id {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(id));
                }
            }
            Command::Archive => {
                if let Some(id) = card.thread_id {
                    self.store.archive_thread(id);
                }
            }
            Command::Goal => {
                if let Some(thread) = card.thread_id {
                    cx.emit(TaskAction::SetGoal {
                        thread,
                        objective: card.title,
                    });
                }
            }
            Command::Send => self.move_card(&card, Column::InProgress, None, cx),
            Command::SendGoal => {
                if let Some(id) = &card.draft_id {
                    self.store.update_tasks(|p| {
                        if let Some(d) = p.drafts.iter_mut().find(|d| &d.id == id) {
                            d.input.goal = true;
                        }
                    });
                    self.move_card(&card, Column::InProgress, None, cx);
                }
            }
            Command::Delete => {
                self.confirmation = Some(card);
                self.menu_focus.focus(window, cx);
            }
        }
        cx.notify();
    }
    pub(super) fn start_rename(&mut self, card: Card, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = card.id.strip_prefix("todo:") {
            self.todo_rename = Some(id.into());
        }
        if card.draft_id.is_some() {
            self.open_card(&card, cx);
            return;
        }
        self.rename_input.update(cx, |input, cx| {
            input.set_text_silently(card.title.clone(), cx)
        });
        if !card.id.starts_with("todo:") {
            self.rename = Some(card);
        }
        self.menu = None;
        self.rename_input
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
        cx.notify();
    }
    pub(super) fn commit_rename(&mut self, text: String, cx: &mut Context<Self>) {
        let name = text.trim();
        if name.is_empty() {
            return;
        }
        if let Some(id) = self.todo_rename.take() {
            self.store.update_tasks(|p| {
                if let Some(t) = p.todos.iter_mut().find(|t| t.id == id) {
                    t.title = name.into();
                }
            });
            if self.selected.as_deref() == Some(format!("todo:{id}").as_str()) {
                self.todo_title
                    .update(cx, |i, cx| i.set_text_silently(name, cx));
            }
        } else if let Some(card) = self.rename.take()
            && let Some(id) = card.thread_id
        {
            self.store.rename_thread(id, name.into());
        }
        self.focus_pending = true;
        cx.notify();
    }
    pub(super) fn menu_overlay(
        &self,
        window: &Window,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let (card, position) = self.menu.clone()?;
        let commands = self.commands(&card);
        let copy_commands = {
            let mut c = vec![(Command::CopyPath, "Path".into())];
            if card.thread_id.is_some() {
                c.push((Command::CopyId, "Thread ID".into()));
            }
            c
        };
        if commands.is_empty() {
            return None;
        }
        let key_commands = if self.copy_menu {
            copy_commands.clone()
        } else {
            commands.clone()
        };
        let key_card = card.clone();
        let relative = position - self.page_origin.get();
        let max_x =
            (f32::from(window.viewport_size().width - self.page_origin.get().x) - 222.).max(4.);
        let max_y =
            (f32::from(window.viewport_size().height) - commands.len() as f32 * 28. - 20.).max(4.);
        let left = f32::from(relative.x).clamp(4., max_x);
        let top = f32::from(relative.y).clamp(4., max_y);
        let copy_index = commands
            .iter()
            .position(|(c, _)| *c == Command::Copy)
            .unwrap_or(0);
        let mut menu = div()
            .id("task-context-menu")
            .role(Role::Menu)
            .track_focus(&self.menu_focus)
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(210.))
            .p(px(5.))
            .rounded(px(12.))
            .bg(theme.model_picker_surface)
            .border(px(0.5))
            .border_color(theme.border)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(8.), theme.text.alpha(0.12).into())
                    .blur_radius(px(20.)),
            ])
            .flex()
            .flex_col()
            .on_click(|_, _, cx| cx.stop_propagation())
            .on_key_down(
                cx.listener(move |this, e: &gpui::KeyDownEvent, window, cx| {
                    match e.keystroke.key.as_str() {
                        "escape" => {
                            if this.copy_menu {
                                this.copy_menu = false;
                            } else {
                                this.menu = None;
                                this.focus.focus(window, cx);
                            }
                        }
                        "down" => this.menu_index = (this.menu_index + 1) % key_commands.len(),
                        "up" => {
                            this.menu_index =
                                (this.menu_index + key_commands.len() - 1) % key_commands.len()
                        }
                        "enter" | "space" => {
                            if let Some((command, _)) = key_commands.get(this.menu_index) {
                                this.run_command(key_card.clone(), *command, window, cx);
                            }
                        }
                        "right" => {
                            if key_commands
                                .get(this.menu_index)
                                .is_some_and(|(c, _)| *c == Command::Copy)
                            {
                                this.copy_menu = true;
                                this.menu_index = 0;
                            }
                        }
                        "left" => {
                            this.copy_menu = false;
                            this.menu_index = 0;
                        }
                        _ => {
                            cx.propagate();
                            return;
                        }
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            );
        for (index, (command, label)) in commands.into_iter().enumerate() {
            let action_card = card.clone();
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("task-menu-{index}")))
                    .role(Role::MenuItem)
                    .aria_label(label.clone())
                    .h(px(28.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .text_size(px(13.))
                    .cursor_pointer()
                    .when(
                        (!self.copy_menu && self.menu_index == index)
                            || (self.copy_menu && command == Command::Copy),
                        |s| s.bg(theme.sidebar_hover),
                    )
                    .text_color(if matches!(command, Command::Delete) {
                        theme.warning
                    } else {
                        theme.text
                    })
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.run_command(action_card.clone(), command, window, cx);
                        cx.stop_propagation();
                    })),
            );
        }
        if self.copy_menu {
            let available = f32::from(window.viewport_size().width - self.page_origin.get().x);
            let submenu_left = if left + 394. <= available {
                left + 210.
            } else {
                (left - 184.).max(0.)
            };
            let mut submenu = div()
                .id("task-copy-submenu")
                .role(Role::Menu)
                .absolute()
                .left(px(submenu_left - left))
                .top(px((copy_index as f32 * 28.)
                    .min(f32::from(window.viewport_size().height) - top - 70.)
                    .max(0.)))
                .w(px(184.))
                .p(px(5.))
                .rounded(px(12.))
                .bg(theme.model_picker_surface)
                .border(px(0.5))
                .border_color(theme.border)
                .shadow(vec![
                    gpui::BoxShadow::new(px(0.), px(8.), theme.text.alpha(0.12).into())
                        .blur_radius(px(20.)),
                ])
                .flex()
                .flex_col()
                .on_click(|_, _, cx| cx.stop_propagation());
            for (index, (command, label)) in copy_commands.into_iter().enumerate() {
                let card = card.clone();
                submenu = submenu.child(
                    div()
                        .id(SharedString::from(format!("task-copy-{index}")))
                        .role(Role::MenuItem)
                        .aria_label(label.clone())
                        .h(px(28.))
                        .px(px(8.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .cursor_pointer()
                        .when(self.menu_index == index, |s| s.bg(theme.sidebar_hover))
                        .hover(move |s| s.bg(theme.sidebar_hover))
                        .child(label)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_command(card.clone(), command, window, cx);
                            cx.stop_propagation();
                        })),
                );
            }
            menu = menu.child(submenu);
        }
        Some(
            gpui::deferred(
                div()
                    .id("task-menu-dismiss")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.menu = None;
                            this.copy_menu = false;
                            this.focus.focus(window, cx);
                            cx.notify();
                        }),
                    )
                    .child(menu),
            )
            .into_any_element(),
        )
    }
    pub(super) fn rename_overlay(
        &self,
        _: &mut Window,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        self.rename.as_ref()?;
        let enabled = !self.rename_input.read(cx).text().trim().is_empty();
        let title = if self.todo_rename.is_some() {
            "Rename task"
        } else {
            "Rename thread"
        };
        Some(
            div()
                .id("task-rename-overlay")
                .absolute()
                .inset_0()
                .bg(theme.chat_search_overlay)
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .child(
                    div()
                        .id("task-rename-dialog")
                        .role(Role::Dialog)
                        .aria_label(title)
                        .track_focus(&self.menu_focus)
                        .w(px(420.))
                        .p(px(20.))
                        .rounded(px(20.))
                        .bg(theme.surface)
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .capture_key_down(cx.listener(
                            |this, e: &gpui::KeyDownEvent, window, cx| {
                                if e.keystroke.key == "tab" {
                                    this.trap_dialog_tab(e, window, cx);
                                    return;
                                }
                                if e.keystroke.key == "escape" {
                                    this.rename = None;
                                    this.todo_rename = None;
                                    this.focus_pending = true;
                                    cx.notify();
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .child(title)
                        .child(self.rename_input.clone())
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(12.))
                                .child(
                                    div()
                                        .id("task-rename-cancel")
                                        .role(Role::Button)
                                        .aria_label("Cancel")
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .child("Cancel")
                                        .on_key_down(cx.listener(
                                            |this, e: &gpui::KeyDownEvent, _, cx| {
                                                if matches!(
                                                    e.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    this.rename = None;
                                                    this.todo_rename = None;
                                                    this.focus_pending = true;
                                                    cx.notify();
                                                    cx.stop_propagation();
                                                }
                                            },
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.rename = None;
                                            this.todo_rename = None;
                                            this.focus_pending = true;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .id("task-rename-save")
                                        .role(Role::Button)
                                        .aria_label("Save")
                                        .aria_disabled(!enabled)
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .child("Save")
                                        .on_key_down(cx.listener(
                                            |this, e: &gpui::KeyDownEvent, _, cx| {
                                                if matches!(
                                                    e.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    this.commit_rename(
                                                        this.rename_input
                                                            .read(cx)
                                                            .text()
                                                            .to_owned(),
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }
                                            },
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.commit_rename(
                                                this.rename_input.read(cx).text().to_owned(),
                                                cx,
                                            );
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
    pub(super) fn confirmation_overlay(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let card = self.confirmation.clone()?;
        Some(
            div()
                .id("task-delete-overlay")
                .absolute()
                .inset_0()
                .bg(theme.chat_search_overlay)
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .child(
                    div()
                        .id("task-delete-dialog")
                        .role(Role::Dialog)
                        .aria_label("Delete task")
                        .track_focus(&self.menu_focus)
                        .w(px(420.))
                        .p(px(20.))
                        .rounded(px(20.))
                        .bg(theme.surface)
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .capture_key_down(cx.listener(
                            |this, e: &gpui::KeyDownEvent, window, cx| {
                                if e.keystroke.key == "tab" {
                                    this.trap_dialog_tab(e, window, cx);
                                    return;
                                }
                                if e.keystroke.key == "escape" {
                                    this.confirmation = None;
                                    this.focus_pending = true;
                                    cx.notify();
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .child(format!("Delete “{}”?", card.title))
                        .child(if card.id.starts_with("todo:") {
                            "This removes the local task. Its linked chat is kept."
                        } else if card.draft_id.is_some() {
                            "This removes the unsent prompt and its attachments."
                        } else {
                            "This permanently clears conversation history for this thread."
                        })
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(16.))
                                .child(
                                    div()
                                        .id("task-delete-cancel")
                                        .role(Role::Button)
                                        .aria_label("Cancel")
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .child("Cancel")
                                        .on_key_down(cx.listener(
                                            |this, e: &gpui::KeyDownEvent, _, cx| {
                                                if matches!(
                                                    e.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    this.confirmation = None;
                                                    this.focus_pending = true;
                                                    cx.notify();
                                                    cx.stop_propagation();
                                                }
                                            },
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.confirmation = None;
                                            this.focus_pending = true;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .id("task-delete-confirm")
                                        .role(Role::Button)
                                        .aria_label("Delete")
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .text_color(theme.warning)
                                        .child("Delete")
                                        .on_key_down(cx.listener(
                                            |this, e: &gpui::KeyDownEvent, _, cx| {
                                                if matches!(
                                                    e.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    this.confirm_delete(cx);
                                                    cx.stop_propagation();
                                                }
                                            },
                                        ))
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.confirm_delete(cx)),
                                        ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
    fn trap_dialog_tab(&self, e: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if e.keystroke.modifiers.shift {
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }
        if !self.menu_focus.contains_focused(window, cx) {
            self.menu_focus.focus(window, cx);
            window.focus_next(cx);
        }
        cx.stop_propagation();
    }
    pub(super) fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(card) = self.confirmation.take() else {
            return;
        };
        if let Some(id) = &card.draft_id {
            if self.is_launching(id) {
                return;
            }
            self.store.update_tasks(|p| {
                p.drafts.retain(|d| &d.id != id);
                for order in p.draft_order.values_mut() {
                    order.retain(|v| v != &card.id);
                }
                for todo in &mut p.todos {
                    if todo.thread_id.as_ref() == Some(id) {
                        todo.thread_id = None;
                    }
                }
            });
        } else if let Some(id) = card.id.strip_prefix("todo:") {
            self.store.update_tasks(|p| p.todos.retain(|t| t.id != id));
            if self.selected.as_deref() == Some(card.id.as_str()) {
                self.selected = None;
            }
        } else if let Some(id) = card.thread_id {
            self.store.delete_thread(id);
        }
        self.focus_pending = true;
        cx.notify();
    }
}
