use super::{TaskRoute, TasksView};
use crate::{
    components::icons::icon,
    tasks::{BoardMode, Card, Column, ProjectBoard},
    theme::Theme,
};
use gpui::{
    BoxShadow, Context, Div, Focusable, MouseButton, Render, Role, SharedString, Window, div,
    point, prelude::*, px, rgba,
};

#[derive(Clone)]
pub(super) struct CardDrag {
    pub card: Card,
    pub mode: crate::theme::ThemeMode,
}
impl Render for CardDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::for_mode(self.mode);
        div()
            .w(px(280.))
            .p(px(12.))
            .rounded(px(10.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .text_color(t.text)
            .child(self.card.title.clone())
    }
}
#[derive(Clone)]
pub(super) enum Control {
    New(Option<String>, bool),
    Route(TaskRoute),
    Mode(BoardMode),
    Review,
    Expand,
    DismissMessage,
    EditorDraft,
    EditorGoal,
    Submit,
    Close,
    ProjectMenu,
    Project(String),
    Retry,
}
impl TasksView {
    pub fn render_modal_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let theme = board_theme(self.mode);
        let dialog = self
            .editor_overlay(window, theme, cx)
            .or_else(|| self.rename_overlay(window, theme, cx))
            .or_else(|| self.confirmation_overlay(theme, cx))?;
        Some(
            div()
                .absolute()
                .inset_0()
                .text_color(theme.text)
                .child(dialog)
                .into_any_element(),
        )
    }
    pub(super) fn control(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        action: Control,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let label = label.into();
        let key_action = action.clone();
        div()
            .id(id.into())
            .role(Role::Button)
            .aria_label(label.clone())
            .focusable()
            .tab_stop(true)
            .h(px(28.))
            .px(px(10.))
            .rounded(px(6.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(5.))
            .text_size(px(crate::typography::ui_size(13.)))
            .cursor_pointer()
            .text_color(theme.text_secondary)
            .hover(move |s| s.bg(theme.sidebar_hover).text_color(theme.text))
            .focus_visible(move |s| s.border_2().border_color(theme.text.alpha(0.4)))
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.perform(action.clone(), window, cx);
                cx.stop_propagation();
            }))
            .on_key_down(
                cx.listener(move |this, e: &gpui::KeyDownEvent, window, cx| {
                    if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                        this.perform(key_action.clone(), window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
    }
    fn editor_switch(
        &self,
        id: &'static str,
        state: (&'static str, bool, bool),
        action: Control,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let (label, value, disabled) = state;
        self.control(id, "", action, theme, cx)
            .role(Role::Switch)
            .aria_label(label)
            .aria_disabled(disabled)
            .aria_toggled(if value {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .px(px(0.))
            .gap(px(8.))
            .opacity(if disabled { 0.5 } else { 1. })
            .child(
                div()
                    .w(px(30.))
                    .h(px(18.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme.text.alpha(0.12))
                    .bg(if value {
                        theme.text
                    } else {
                        theme.text.alpha(0.18)
                    })
                    .flex()
                    .items_center()
                    .px(px(2.))
                    .when(value, |s| s.justify_end())
                    .child(div().size(px(14.)).rounded_full().bg(theme.surface)),
            )
            .child(label)
    }
    fn perform(&mut self, action: Control, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Control::New(project, draft) => self.new_task(project, draft, cx),
            Control::Route(route) => self.navigate(route, cx),
            Control::Mode(mode) => self.store.update_tasks(|p| p.mode = mode),
            Control::Review => self
                .store
                .update_tasks(|p| p.needs_review = !p.needs_review),
            Control::Expand => self.store.update_tasks(|p| p.expanded = !p.expanded),
            Control::DismissMessage => self.message = None,
            Control::EditorDraft => {
                if let Some(e) = &mut self.editor {
                    e.send_as_draft = !e.send_as_draft;
                    if e.send_as_draft {
                        e.goal = false;
                    }
                }
            }
            Control::EditorGoal => {
                if let Some(e) = &mut self.editor
                    && !e.send_as_draft
                {
                    e.goal = !e.goal;
                }
            }
            Control::Submit => self.submit_editor(cx),
            Control::Close => self.close_editor(window, cx),
            Control::ProjectMenu => {
                if let Some(e) = &mut self.editor {
                    e.project_menu = !e.project_menu;
                    e.composer.update(cx, |c, cx| {
                        c.close_side_menus(cx);
                    });
                }
            }
            Control::Project(id) => {
                if let Some(project) = self
                    .store
                    .snapshot()
                    .projects
                    .iter()
                    .find(|p| p.project_id == id)
                    && let Some(editor) = &mut self.editor
                    && let Some(cwd) = project.roots.first()
                {
                    editor.draft.project_id = id.clone();
                    editor.draft.cwd = cwd.clone();
                    editor.project_menu = false;
                    editor.composer.update(cx, |c, cx| {
                        c.set_workspace_context(
                            cwd.clone(),
                            Some(id),
                            editor.draft.thread_id.clone(),
                            cx,
                        )
                    });
                }
            }
            Control::Retry => self.store.retry(),
        }
        cx.notify();
    }
    fn card(&self, card: &Card, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let open = card.clone();
        let key = card.clone();
        let menu = card.clone();
        let reorder = card.clone();
        let now = chrono::Utc::now().timestamp();
        let launching = card
            .draft_id
            .as_ref()
            .is_some_and(|id| self.launching.contains_key(id));
        let status = if launching {
            "Starting…"
        } else {
            card.column.label()
        };
        let pr = card
            .thread_id
            .as_deref()
            .and_then(|id| self.store.snapshot().thread(id).cloned())
            .and_then(
                |t| match self.store.snapshot().thread_pull_request_chip(&t) {
                    crate::workspace::ChipPullRequest::Show { url, state } => Some((
                        url,
                        format!("PR #{} · {:?}", state.summary.number, state.summary.status),
                    )),
                    _ => None,
                },
            );
        let color = match card.column {
            Column::Done => rgba(0x7068ccff),
            Column::InProgress => rgba(0xd8b333ff),
            Column::AwaitingYou => rgba(0xc98927ff),
            Column::Draft => theme.text_tertiary,
        };
        div()
            .id(SharedString::from(format!("task-card-{}", card.id)))
            .role(Role::Button)
            .aria_label(format!(
                "{} {} {} {}",
                card.title,
                card.branch.as_deref().unwrap_or(""),
                crate::tasks::relative_time(card.timestamp, now),
                status
            ))
            .aria_description("Open task. Right-click for actions. Alt+Up/Down reorders drafts.")
            .focusable()
            .tab_stop(true)
            .relative()
            .w_full()
            .min_w(px(0.))
            // All card rows are single-line. A definite height also keeps long
            // boards from repeatedly measuring every title for intrinsic size.
            .h(px(65.
                + if card.attention.is_some() { 22. } else { 0. }
                + if pr.is_some() { 22. } else { 0. }))
            .flex_none()
            .px(px(12.))
            .py(px(10.))
            .rounded(px(10.))
            .border(px(0.5))
            .border_color(theme.text.alpha(0.08))
            .bg(theme.control_soft)
            .shadow(vec![
                BoxShadow::new(px(0.), px(4.), theme.text.alpha(0.025).into()).blur_radius(px(6.)),
            ])
            .hover(move |s| {
                s.bg(theme.sidebar_hover)
                    .border_color(theme.text.alpha(0.13))
            })
            .focus_visible(move |s| s.border_2().border_color(theme.text.alpha(0.35)))
            .when(self.selected.as_deref() == Some(card.id.as_str()), |s| {
                s.border_color(theme.text.alpha(0.25))
            })
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(6.))
            .on_click(cx.listener(move |this, _, _, cx| this.open_card(&open, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &gpui::MouseDownEvent, window, cx| {
                    this.menu = Some((menu.clone(), e.position));
                    this.menu_index = 0;
                    this.menu_focus.focus(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .on_key_down(
                cx.listener(move |this, e: &gpui::KeyDownEvent, window, cx| {
                    match e.keystroke.key.as_str() {
                        "enter" | "space" => {
                            this.open_card(&key, cx);
                            cx.stop_propagation();
                        }
                        "up" | "down"
                            if e.keystroke.modifiers.alt && key.column == Column::Draft =>
                        {
                            this.reorder_keyboard(&key, e.keystroke.key == "down", cx);
                            cx.stop_propagation();
                        }
                        "f2" => {
                            this.start_rename(key.clone(), window, cx);
                            cx.stop_propagation();
                        }
                        "f10" if e.keystroke.modifiers.shift => {
                            this.menu = Some((key.clone(), point(px(80.), px(100.))));
                            this.menu_focus.focus(window, cx);
                            cx.notify();
                            cx.stop_propagation();
                        }
                        _ => cx.propagate(),
                    }
                }),
            )
            .when(card.column == Column::Draft && !launching, |s| {
                s.drag_threshold(px(6.))
                    .on_drag(
                        CardDrag {
                            card: card.clone(),
                            mode: self.mode,
                        },
                        |drag, _, _, cx| cx.new(|_| drag.clone()),
                    )
                    .on_drop(cx.listener(move |this, drag: &CardDrag, _, cx| {
                        if drag.card.project_id == reorder.project_id {
                            this.move_card(&drag.card, Column::Draft, Some(&reorder.id), cx);
                        }
                        cx.stop_propagation();
                    }))
            })
            .child(
                div()
                    .w_full()
                    .h(px(18.))
                    .flex_none()
                    .truncate()
                    .text_color(theme.text)
                    .text_size(px(crate::typography::ui_size(14.)))
                    .line_height(px(18.))
                    .child(card.title.clone()),
            )
            .child(
                div()
                    .h(px(16.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_size(px(crate::typography::ui_size(12.)))
                    .line_height(px(16.))
                    .text_color(theme.text_tertiary)
                    .child(icon("codex", theme.text_secondary.into()).size(px(14.)))
                    .when_some(card.branch.clone(), |row, branch| {
                        row.child(icon("branch", theme.text_tertiary.into()).size(px(12.)))
                            .child(div().max_w(px(110.)).truncate().child(branch))
                    })
                    .child(div().flex_1())
                    .child(crate::tasks::relative_time(card.timestamp, now))
                    .child(icon(column_icon(card.column), color.into()).size(px(12.)))
                    .child(status),
            )
            .when_some(card.attention, |s, attention| {
                s.child(
                    div()
                        .h(px(16.))
                        .flex_none()
                        .line_height(px(16.))
                        .text_size(px(11.))
                        .text_color(theme.warning)
                        .child(attention),
                )
            })
            .when_some(pr, |s, (url, label)| {
                s.child(
                    div()
                        .id(SharedString::from(format!("task-pr-{}", card.id)))
                        .role(Role::Link)
                        .aria_label(label.clone())
                        .h(px(16.))
                        .flex_none()
                        .line_height(px(16.))
                        .text_size(px(11.))
                        .text_color(theme.effort)
                        .cursor_pointer()
                        .child(label)
                        .on_click(move |_, _, cx| {
                            cx.open_url(&url);
                            cx.stop_propagation();
                        }),
                )
            })
    }
    fn column(
        &mut self,
        project: &ProjectBoard,
        column: Column,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let project_id = project.id.clone();
        let all: Vec<_> = project
            .cards
            .iter()
            .filter(|c| c.column == column)
            .collect();
        let cap = if self.store.snapshot().preferences.tasks.expanded {
            usize::MAX
        } else {
            30
        };
        let scroll_id = format!("task-column-{}-{}", project.id, column.label());
        let scroll = self
            .column_scrolls
            .entry(scroll_id.clone())
            .or_default()
            .clone();
        let mut list = div()
            .id(SharedString::from(scroll_id))
            .role(Role::List)
            .aria_label(format!("{} drop area", column.label()))
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .flex()
            .flex_col()
            .gap(px(8.))
            .drag_over::<CardDrag>(move |s, _, _, _| s.bg(theme.text.alpha(0.04)))
            .on_drop(cx.listener(move |this, drag: &CardDrag, _, cx| {
                if drag.card.project_id == project_id {
                    let snapshot = this.store.snapshot();
                    let last = crate::tasks::boards(&snapshot, chrono::Utc::now().timestamp())
                        .into_iter()
                        .find(|p| p.id == project_id)
                        .and_then(|p| p.cards.into_iter().rfind(|c| c.column == Column::Draft))
                        .map(|c| c.id);
                    this.move_card(&drag.card, column, last.as_deref(), cx);
                }
                cx.stop_propagation();
            }));
        let header = div()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(2.))
            .text_size(px(14.))
            .text_color(theme.text)
            .child(column.label())
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme.text_tertiary)
                    .child(all.len().to_string()),
            )
            .child(div().flex_1())
            .when(column == Column::Draft, |s| {
                s.child(
                    self.control(
                        SharedString::from(format!("new-task-{}", project.id)),
                        "+",
                        Control::New(Some(project.id.clone()), true),
                        theme,
                        cx,
                    )
                    .aria_label("New task in Draft"),
                )
            })
            .child(
                icon(
                    column_icon(column),
                    match column {
                        Column::Done => rgba(0x7068ccff),
                        Column::InProgress => rgba(0xd8b333ff),
                        Column::AwaitingYou => rgba(0xc98927ff),
                        Column::Draft => theme.text_tertiary,
                    }
                    .into(),
                )
                .size(px(14.)),
            );
        if all.is_empty() {
            list = list.child(
                div()
                    .h(px(50.))
                    .w_full()
                    .border(px(0.5))
                    .border_dashed()
                    .border_color(theme.text.alpha(0.06))
                    .rounded(px(10.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(theme.text_tertiary)
                    .child("No cards"),
            );
        }
        for card in all.iter().take(cap) {
            list = list.child(self.card(card, theme, cx));
        }
        if all.len() > cap {
            list = list.child(self.control(
                SharedString::from(format!("column-more-{}", column.label())),
                format!("Show {} more", all.len() - cap),
                Control::Expand,
                theme,
                cx,
            ));
        }
        div()
            .id(SharedString::from(format!(
                "task-section-{}-{}",
                project.id,
                column.label()
            )))
            .flex_1()
            .min_w(px(248.))
            .min_h(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(header)
            .child(list)
    }
    fn overview_project(
        &mut self,
        project: &ProjectBoard,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let prefs = &self.store.snapshot().preferences.tasks;
        let mut counts = [0; 4];
        let cap = if prefs.expanded { usize::MAX } else { 20 };
        let mut hidden = 0;
        let scroll_id = format!("task-project-cards-{}", project.id);
        let scroll = self
            .column_scrolls
            .entry(scroll_id.clone())
            .or_default()
            .clone();
        let mut list = div()
            .id(SharedString::from(scroll_id))
            .role(Role::List)
            .aria_label(format!("{} tasks", project.name))
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .flex()
            .flex_col()
            .gap(px(8.));
        let mut column = div()
            .id(SharedString::from(format!("task-project-{}", project.id)))
            .w(px(280.))
            .min_w(px(280.))
            .min_h(px(0.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .h(px(28.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(
                        self.control(
                            SharedString::from(format!("open-board-{}", project.id)),
                            "",
                            Control::Route(TaskRoute {
                                project: Some(project.id.clone()),
                                list: false,
                            }),
                            theme,
                            cx,
                        )
                        .flex_1()
                        .justify_start()
                        .min_w(px(0.))
                        .px(px(4.))
                        .aria_label(format!("{} {}", project.name, project.cards.len()))
                        .child(
                            div()
                                .min_w(px(0.))
                                .truncate()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(project.name.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(12.))
                                .text_color(theme.text_tertiary)
                                .child(project.cards.len().to_string()),
                        ),
                    )
                    .child(
                        self.control(
                            SharedString::from(format!("project-add-{}", project.id)),
                            "+",
                            Control::New(Some(project.id.clone()), false),
                            theme,
                            cx,
                        )
                        .aria_label(format!("New task in {}", project.name)),
                    ),
            );
        for card in &project.cards {
            if prefs.mode == BoardMode::Classic && card.column == Column::AwaitingYou {
                continue;
            }
            let index = match card.column {
                Column::Draft => 0,
                Column::InProgress => 1,
                Column::AwaitingYou => 2,
                Column::Done => 3,
            };
            counts[index] += 1;
            if counts[index] > cap {
                hidden += 1;
                continue;
            }
            list = list.child(self.card(card, theme, cx));
        }
        if hidden > 0 {
            list = list.child(self.control(
                SharedString::from(format!("project-more-{}", project.id)),
                format!("Show {hidden} more"),
                Control::Expand,
                theme,
                cx,
            ));
        }
        column = column.child(list);
        column
    }
    fn editor_overlay(
        &mut self,
        window: &mut Window,
        _theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let theme = Theme::for_mode(self.mode);
        let editor = self.editor.as_mut()?;
        if editor.previous_focus.is_none() {
            editor.previous_focus = window.focused(cx);
        }
        let composer = editor.composer.clone();
        let draft = editor.draft.clone();
        let save = editor.send_as_draft;
        let goal = editor.goal;
        let project_menu = editor.project_menu;
        let input = composer.read(cx).task_input(cx);
        let enabled = !input.is_empty() && (save || composer.read(cx).task_can_send(cx));
        let snapshot = self.store.snapshot();
        let project_name = snapshot
            .projects
            .iter()
            .find(|p| p.project_id == draft.project_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "No project".into());
        let picker_bounds =
            std::rc::Rc::new(std::cell::Cell::new(None::<gpui::Bounds<gpui::Pixels>>));
        let picker_canvas = picker_bounds.clone();
        let utilities = composer.update(cx, |c, cx| c.render_task_footer(cx));
        let child_overlay = composer.update(cx, |c, cx| c.render_overlay(window, cx));
        let toasts = composer.update(cx, |c, cx| c.render_toasts(cx));
        Some(
            div()
                .id("task-editor-overlay")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000099))
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .on_click(cx.listener(|this, _, window, cx| this.close_editor(window, cx)))
                .child(
                    div()
                        .id("task-editor-dialog")
                        .role(Role::Dialog)
                        .aria_label("New task")
                        .track_focus(&self.editor_focus)
                        .capture_action(cx.listener(
                            |this, _: &crate::components::file_editor::EditorTab, window, cx| {
                                window.focus_next(cx);
                                if !this.editor_focus.contains_focused(window, cx) {
                                    this.editor_focus.focus(window, cx);
                                    window.focus_next(cx);
                                }
                                cx.stop_propagation();
                            },
                        ))
                        .capture_action(cx.listener(
                            |this, _: &crate::app::DismissPermissionUi, window, cx| {
                                if let Some(e) = &mut this.editor {
                                    if e.project_menu {
                                        e.project_menu = false;
                                        cx.notify();
                                        cx.stop_propagation();
                                        return;
                                    }
                                    if e.composer.update(cx, |c, cx| c.dismiss_task_menu(cx)) {
                                        cx.stop_propagation();
                                        return;
                                    }
                                    if e.composer.read(cx).has_modal_overlay() {
                                        cx.propagate();
                                        return;
                                    }
                                }
                                this.close_editor(window, cx);
                                cx.stop_propagation();
                            },
                        ))
                        .w(px(768.))
                        .max_w((window.viewport_size().width - px(32.)).max(px(0.)))
                        .max_h((window.viewport_size().height - px(32.)).max(px(0.)))
                        .overflow_y_scroll()
                        .p(px(16.))
                        .rounded(px(22.))
                        .bg(theme.surface)
                        .text_color(theme.text)
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .relative()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .capture_key_down(cx.listener(
                            |this, e: &gpui::KeyDownEvent, window, cx| {
                                if e.keystroke.key == "tab" {
                                    if e.keystroke.modifiers.shift {
                                        window.focus_prev(cx);
                                    } else {
                                        window.focus_next(cx);
                                    }
                                    if !this.editor_focus.contains_focused(window, cx) {
                                        this.editor_focus.focus(window, cx);
                                        window.focus_next(cx);
                                    }
                                    cx.stop_propagation();
                                    return;
                                }
                                if e.keystroke.key == "escape"
                                    && let Some(editor) = &mut this.editor
                                {
                                    if editor.project_menu {
                                        editor.project_menu = false;
                                        cx.notify();
                                        cx.stop_propagation();
                                        return;
                                    }
                                    if editor.composer.read(cx).has_modal_overlay() {
                                        cx.propagate();
                                        return;
                                    }
                                    if editor.composer.update(cx, |c, cx| c.dismiss_task_menu(cx)) {
                                        cx.stop_propagation();
                                        return;
                                    }
                                    this.close_editor(window, cx);
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .child(
                                    self.control(
                                        "task-project-picker",
                                        "",
                                        Control::ProjectMenu,
                                        theme,
                                        cx,
                                    )
                                    .min_w(px(0.))
                                    .max_w(px(260.))
                                    .aria_label(format!("Choose project: {project_name}"))
                                    .child(div().min_w(px(0.)).truncate().child(project_name))
                                    .child(
                                        icon("chevron-down", theme.text_secondary.into())
                                            .size(px(12.)),
                                    )
                                    .relative()
                                    .child(
                                        gpui::canvas(
                                            move |bounds, _, _| picker_canvas.set(Some(bounds)),
                                            |_, (), _, _| {},
                                        )
                                        .absolute()
                                        .inset_0(),
                                    )
                                    .border(px(0.5))
                                    .border_color(theme.border),
                                )
                                .child("›")
                                .child(div().text_size(px(13.)).child("New task"))
                                .child(div().flex_1())
                                .child(
                                    self.control(
                                        "task-editor-close",
                                        "×",
                                        Control::Close,
                                        theme,
                                        cx,
                                    )
                                    .aria_label("Close task editor"),
                                ),
                        )
                        .when(project_menu, |s| {
                            s.child(gpui::deferred(
                                div()
                                    .id("task-project-options")
                                    .on_mouse_down_out(cx.listener(
                                        move |this, event: &gpui::MouseDownEvent, _, cx| {
                                            if !picker_bounds
                                                .get()
                                                .is_some_and(|b| b.contains(&event.position))
                                            {
                                                if let Some(editor) = &mut this.editor {
                                                    editor.project_menu = false;
                                                }
                                                cx.notify();
                                            }
                                        },
                                    ))
                                    .role(Role::Menu)
                                    .absolute()
                                    .top(px(46.))
                                    .left(px(16.))
                                    .w(px(280.))
                                    .max_w_full()
                                    .p(px(6.))
                                    .rounded(px(10.))
                                    .bg(theme.surface)
                                    .border_1()
                                    .border_color(theme.border)
                                    .max_h(px(200.))
                                    .overflow_y_scroll()
                                    .flex()
                                    .flex_col()
                                    .children(snapshot.projects.iter().map(|p| {
                                        self.control(
                                            SharedString::from(format!(
                                                "task-project-option-{}",
                                                p.project_id
                                            )),
                                            p.name.clone(),
                                            Control::Project(p.project_id.clone()),
                                            theme,
                                            cx,
                                        )
                                        .justify_start()
                                        .truncate()
                                    })),
                            ))
                        })
                        .child(composer.clone())
                        .child(
                            div()
                                .border_t_1()
                                .border_color(theme.text.alpha(0.06))
                                .pt(px(8.))
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap(px(12.))
                                .child(utilities)
                                .child(div().flex_1())
                                .child(self.editor_switch(
                                    "task-draft-toggle",
                                    ("Send as draft", save, false),
                                    Control::EditorDraft,
                                    theme,
                                    cx,
                                ))
                                .child(self.editor_switch(
                                    "task-goal-toggle",
                                    ("Send as goal", goal, save),
                                    Control::EditorGoal,
                                    theme,
                                    cx,
                                ))
                                .child(
                                    self.control(
                                        "task-create",
                                        "Create task",
                                        Control::Submit,
                                        theme,
                                        cx,
                                    )
                                    .bg(theme.button)
                                    .text_color(theme.button_text)
                                    .aria_disabled(!enabled)
                                    .opacity(if enabled {
                                        1.
                                    } else {
                                        0.5
                                    }),
                                ),
                        ),
                )
                .children(child_overlay)
                .children(toasts)
                .into_any_element(),
        )
    }
}
impl Render for TasksView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_pending {
            self.focus_pending = false;
            self.focus.focus(window, cx);
        }
        let theme = board_theme(self.mode);
        let snapshot = self.store.snapshot();
        let prefs = &snapshot.preferences.tasks;
        let mut boards = crate::tasks::boards(&snapshot, chrono::Utc::now().timestamp());
        for board in &mut boards {
            board.cards.retain(|card| {
                !card.thread_id.as_ref().is_some_and(|id| {
                    self.launching
                        .iter()
                        .any(|(_, c)| c.composer.read(cx).thread_id() == Some(id.as_str()))
                        && card.draft_id.is_none()
                })
            });
        }
        for board in &mut boards {
            for card in &mut board.cards {
                if card
                    .draft_id
                    .as_ref()
                    .is_some_and(|id| self.launching.contains_key(id))
                {
                    card.column = Column::InProgress;
                }
            }
        }
        let threads: Vec<_> = snapshot
            .pinned_threads
            .iter()
            .chain(&snapshot.recent_threads)
            .filter(|t| self.requested_prs.insert(t.thread_id.clone()))
            .cloned()
            .collect();
        if !threads.is_empty() {
            self.store.request_thread_pull_requests(&threads);
        }
        let runs: Vec<_> = snapshot
            .pinned_threads
            .iter()
            .chain(&snapshot.recent_threads)
            .filter(|t| {
                let signature = (t.updated_at, t.activity.clone());
                if self.requested_runs.get(&t.thread_id) == Some(&signature) {
                    false
                } else {
                    self.requested_runs.insert(t.thread_id.clone(), signature);
                    true
                }
            })
            .cloned()
            .collect();
        if !runs.is_empty() {
            self.store.request_task_runs(runs);
        }
        let project = boards
            .iter()
            .find(|p| Some(&p.id) == self.route.project.as_ref());
        let total = project.map_or_else(
            || boards.iter().map(|p| p.cards.len()).sum(),
            |p| p.cards.len(),
        );
        let mut header = div()
            .id("tasks-toolbar")
            .min_w(px(0.))
            .overflow_x_scroll()
            .h(px(46.))
            .flex_none()
            .pl(px(self.titlebar_inset.max(20.)))
            .pr(px(20.))
            .flex()
            .items_center()
            .gap(px(8.));
        if let Some(project) = project {
            header = header
                .child(
                    self.control(
                        "tasks-all-projects",
                        "←",
                        Control::Route(TaskRoute::default()),
                        theme,
                        cx,
                    )
                    .aria_label("Back to all projects"),
                )
                .child(
                    div()
                        .min_w(px(80.))
                        .max_w(px(260.))
                        .truncate()
                        .text_size(px(14.))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(project.name.clone()),
                );
        } else if !self.route.list {
            header = header.child(
                div()
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("Kanban"),
            );
        }
        if !self.route.list {
            header = header.child(
                div()
                    .text_color(theme.text_tertiary)
                    .text_size(px(13.))
                    .child(format!(
                        "{total} {}",
                        if total == 1 { "task" } else { "tasks" }
                    )),
            );
        }
        if self.route.project.is_none() {
            if self.route.list {
                header = header.child(div().flex_1());
            }
            for (label, list) in [("List", true), ("Kanban", false)] {
                header = header.child(
                    self.control(
                        SharedString::from(format!("tasks-view-{label}")),
                        label,
                        Control::Route(TaskRoute {
                            project: None,
                            list,
                        }),
                        theme,
                        cx,
                    )
                    .aria_selected(self.route.list == list)
                    .when(self.route.list == list, |s| {
                        s.bg(theme.sidebar_hover).text_color(theme.text)
                    }),
                );
            }
        }
        if !self.route.list {
            for (label, mode) in [
                ("Attention", BoardMode::Attention),
                ("Classic", BoardMode::Classic),
            ] {
                header = header.child(
                    self.control(
                        SharedString::from(format!("tasks-mode-{label}")),
                        label,
                        Control::Mode(mode),
                        theme,
                        cx,
                    )
                    .aria_selected(prefs.mode == mode)
                    .when(prefs.mode == mode, |s| {
                        s.bg(theme.sidebar_hover).text_color(theme.text)
                    }),
                );
            }
            header = header.child(div().flex_1()).child(
                self.control(
                    "tasks-new",
                    "+ New task",
                    Control::New(self.route.project.clone(), false),
                    theme,
                    cx,
                )
                .aria_disabled(snapshot.projects.is_empty()),
            );
        }
        let mut body = div()
            .id("tasks-body")
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .pt(px(0.));
        if self.route.list {
            body = body.child(self.list_view(theme, cx));
        } else {
            if prefs.mode == BoardMode::Attention {
                body = body.child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(16.))
                        .pb(px(8.))
                        .child(
                            self.control("tasks-needs-review", "", Control::Review, theme, cx)
                                .role(Role::CheckBox)
                                .aria_label("Needs review")
                                .px(px(4.))
                                .child(
                                    div()
                                        .size(px(15.))
                                        .flex_none()
                                        .rounded(px(4.))
                                        .border_1()
                                        .border_color(theme.text.alpha(0.10))
                                        .bg(if prefs.needs_review {
                                            theme.text
                                        } else {
                                            theme.control_soft
                                        })
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .when(prefs.needs_review, |s| {
                                            s.child(
                                                icon("check", theme.surface.into()).size(px(11.)),
                                            )
                                        }),
                                )
                                .child("Needs review")
                                .aria_toggled(if prefs.needs_review {
                                    gpui::Toggled::True
                                } else {
                                    gpui::Toggled::False
                                })
                                .text_size(px(12.)),
                        )
                        .when(prefs.expanded, |s| {
                            s.child(self.control(
                                "tasks-less",
                                "Show fewer",
                                Control::Expand,
                                theme,
                                cx,
                            ))
                        }),
                );
            }
            let mut content = div()
                .id("tasks-scroll")
                .flex_1()
                .min_h(px(0.))
                .overflow_x_scroll()
                .track_scroll(&self.scroll)
                .px(px(20.))
                .pb(px(20.))
                .flex()
                .gap(px(20.));
            if let Some(project) = project {
                for column in [
                    Column::Draft,
                    Column::InProgress,
                    Column::AwaitingYou,
                    Column::Done,
                ] {
                    if column == Column::AwaitingYou && prefs.mode == BoardMode::Classic {
                        continue;
                    }
                    content = content.child(self.column(project, column, theme, cx));
                }
            } else {
                for project in boards.iter().filter(|p| !p.cards.is_empty()) {
                    content = content.child(self.overview_project(project, theme, cx));
                }
                if total == 0 {
                    content = content.child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap(px(12.))
                            .text_color(theme.text_tertiary)
                            .child(div().text_size(px(14.)).text_color(theme.text).child(
                                if snapshot.loading.projects || snapshot.loading.recent {
                                    "Loading tasks…"
                                } else if prefs.needs_review {
                                    "No tasks need review"
                                } else {
                                    "Nothing on the board yet"
                                }
                            ))
                            .when(!snapshot.loading.projects && !snapshot.loading.recent, |s| {
                                s.child(div().max_w(px(400.)).text_size(px(13.)).text_center().text_color(theme.text_secondary).child(
                                    if prefs.needs_review {"Clear Needs review to see all tasks."}
                                    else {"Drafted prompts, running turns, and completed chats will show up here automatically."}
                                ))
                            })
                            .when(
                                snapshot.projects.is_empty() && !snapshot.loading.projects,
                                |s| {
                                    s.child("Add a local project from the sidebar to create tasks.")
                                },
                            ),
                    );
                }
            }
            body = body.child(content);
        }
        let error = snapshot.error.clone().or(snapshot.preference_error.clone());
        let origin = self.page_origin.clone();
        let mut root = div()
            .id("tasks-page")
            .size_full()
            .min_w(px(0.))
            .relative()
            .bg(theme.surface)
            .text_color(theme.text)
            .font(crate::theme::ui_font())
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, window, cx| {
                if e.keystroke.modifiers.alt
                    && e.keystroke.modifiers.platform
                    && e.keystroke.key == "t"
                {
                    if this.editor.is_none() {
                        if this.route.list {
                            this.quick_add.read(cx).focus_handle(cx).focus(window, cx);
                        } else {
                            this.new_task(this.route.project.clone(), false, cx);
                        }
                    }
                    cx.stop_propagation();
                }
            }))
            .child(
                gpui::canvas(
                    move |bounds, _, _| origin.set(bounds.origin),
                    |_, (), _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .capture_action(
                cx.listener(|this, _: &crate::app::DismissPermissionUi, window, cx| {
                    if this.editor.is_some() {
                        cx.propagate();
                        return;
                    }
                    if this.copy_menu {
                        this.copy_menu = false;
                    } else if this.menu.is_some() {
                        this.menu = None;
                    } else if this.rename.is_some() || this.todo_rename.is_some() {
                        this.rename = None;
                        this.todo_rename = None;
                    } else if this.confirmation.is_some() {
                        this.confirmation = None;
                    } else {
                        cx.propagate();
                        return;
                    }
                    this.focus.focus(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .child(header)
            .when_some(error, |s, error| {
                s.child(
                    div()
                        .id("tasks-error")
                        .role(Role::Alert)
                        .px(px(20.))
                        .py(px(8.))
                        .text_color(theme.warning)
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(error)
                        .child(self.control("tasks-retry", "Try again", Control::Retry, theme, cx)),
                )
            })
            .child(body);
        root = root.when_some(self.message.clone(), |s, message| {
            s.child(
                div()
                    .id("tasks-message")
                    .role(Role::Status)
                    .absolute()
                    .bottom(px(20.))
                    .right(px(20.))
                    .max_w(px(600.))
                    .rounded(px(12.))
                    .bg(theme.model_picker_surface)
                    .border(px(0.5))
                    .border_color(theme.border)
                    .shadow(vec![
                        BoxShadow::new(px(0.), px(5.), theme.text.alpha(0.1).into())
                            .blur_radius(px(16.)),
                    ])
                    .px(px(20.))
                    .py(px(6.))
                    .text_size(px(12.))
                    .flex()
                    .items_center()
                    .child(message)
                    .child(self.control(
                        "tasks-dismiss-message",
                        "×",
                        Control::DismissMessage,
                        theme,
                        cx,
                    )),
            )
        });
        if let Some(menu) = self.menu_overlay(window, theme, cx) {
            root = root.child(menu);
        }
        root
    }
}

/// Synara Tasks uses its neutral canvas and card tokens within Echora's shell.
fn board_theme(mode: crate::theme::ThemeMode) -> Theme {
    let mut theme = Theme::for_mode(mode);
    match mode {
        crate::theme::ThemeMode::Light => {
            theme.surface = rgba(0xf9f9f9ff);
            theme.control_soft = rgba(0xfcfcfcff);
            theme.sidebar_hover = rgba(0xf0f0f0ff);
            theme.text = rgba(0x171717ff);
            theme.text_secondary = rgba(0x737373ff);
            theme.text_tertiary = rgba(0x999999ff);
        }
        crate::theme::ThemeMode::Dark => {
            theme.surface = rgba(0x171717ff);
            theme.control_soft = rgba(0x1c1c1cff);
            theme.sidebar_hover = rgba(0x262626ff);
        }
    }
    theme
}

fn column_icon(column: Column) -> &'static str {
    match column {
        Column::Draft => "task-draft",
        Column::InProgress => "task-progress",
        Column::AwaitingYou => "task-attention",
        Column::Done => "task-done",
    }
}
