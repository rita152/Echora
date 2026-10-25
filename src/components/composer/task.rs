//! Durable task input uses the same editor and turn submission as a chat.
use super::ComposerView;
use crate::tasks::TaskInput;
use gpui::{App, Context};

pub struct SubmitTaskEditor;
impl gpui::EventEmitter<SubmitTaskEditor> for ComposerView {}
impl ComposerView {
    pub(crate) fn set_task_editor(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.task_editor = enabled;
        self.prompt_editor.update(cx, |editor, cx| {
            editor.set_placeholder(
                if enabled {
                    "Describe the task, @tag files/folders, paste images, or use / for skills"
                } else {
                    "随心输入"
                },
                cx,
            );
            editor.set_accessible_name(if enabled {
                "Task prompt"
            } else {
                "聊天输入框"
            });
        });
        self.focus_prompt_pending = true;
        cx.notify();
    }
    pub(crate) fn task_input(&self, cx: &App) -> TaskInput {
        TaskInput {
            text: self.prompt_text(cx).to_owned(),
            context: self.prompt_context.clone(),
            model: self.conversation.selected_model.clone(),
            effort: self.conversation.selected_effort.clone(),
            service_tier: self.conversation.selected_service_tier.clone(),
            permission: Some(self.permission_selection.clone()),
            worktree: self.workspace_controls.worktree_mode,
            base: self.workspace_controls.base.clone(),
            include_local_changes: self.workspace_controls.include_local_changes,
            goal: self.goal_draft,
        }
    }
    pub(crate) fn restore_task_input(&mut self, input: &TaskInput, cx: &mut Context<Self>) {
        self.submission_error = None;
        self.set_draft_text(&input.text, cx);
        self.prompt_context = input.context.clone();
        self.prompt_editor.update(cx, |editor, cx| {
            editor.set_references(input.context.references.clone(), cx)
        });
        if !input.model.is_empty() {
            self.conversation.selected_model = input.model.clone();
            self.conversation.selected_effort = input.effort.clone();
            self.conversation.selected_service_tier = input.service_tier.clone();
            self.conversation.model_user_selected = true;
            self.conversation.model_explicit = true;
        }
        if let Some(selection) = &input.permission {
            self.set_permission_selection(selection.clone());
        }
        self.workspace_controls.worktree_mode = input.worktree;
        self.workspace_controls.base = input.base.clone();
        self.workspace_controls.include_local_changes = input.include_local_changes;
        self.goal_draft = input.goal;
        cx.notify();
    }
    pub(crate) fn launch_task(&mut self, goal: bool, cx: &mut Context<Self>) -> bool {
        if !self.task_can_send(cx) {
            return false;
        }
        self.set_task_editor(false, cx);
        self.goal_draft = goal;
        self.submit_current_prompt(cx);
        true
    }
    pub(crate) fn reply_to_task(
        &mut self,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.draft_is_empty(cx) {
            return Err(
                "This chat already has unsent input. Open it to combine your reply.".into(),
            );
        }
        self.set_draft_text(text, cx);
        if !self.task_can_send(cx) {
            return Err("The chat is not ready. Your reply is kept in its composer.".into());
        }
        self.submit_current_prompt(cx);
        Ok(())
    }
    pub(crate) fn task_can_send(&self, cx: &App) -> bool {
        self.send_action(cx) == super::send::SendAction::Send
    }
    pub(crate) fn stop_task(&mut self, cx: &mut Context<Self>) {
        self.stop_generation(cx);
    }
    pub(crate) fn task_marker(&self) -> (usize, u64) {
        (
            self.conversation.submissions.len(),
            self.conversation.goal.revision,
        )
    }
    pub(crate) fn task_submission_id(&self, marker: (usize, u64)) -> Option<String> {
        self.conversation
            .submissions
            .get(marker.0)
            .map(|s| s.id.clone())
    }
    pub(crate) fn task_pending(&self, marker: (usize, u64)) -> bool {
        self.dialog.is_some()
            || self.workspace_controls.busy
            || self.workspace_controls.pending_send.is_some()
            || self.conversation.goal.pending.is_some()
            || self
                .conversation
                .submissions
                .iter()
                .skip(marker.0)
                .any(|s| s.status == crate::conversation::SubmissionStatus::Sending)
    }
    pub(crate) fn task_error(&self, marker: (usize, u64)) -> Option<String> {
        self.submission_error
            .clone()
            .or_else(|| self.conversation.model_catalog_error.clone())
            .or_else(|| self.permission_catalog_error.clone())
            .or_else(|| self.conversation.permission_error.clone())
            .or_else(|| {
                self.conversation
                    .submissions
                    .iter()
                    .skip(marker.0)
                    .find_map(|s| match &s.status {
                        crate::conversation::SubmissionStatus::Failed(e) => Some(e.clone()),
                        _ => None,
                    })
            })
    }
    pub(crate) fn task_accepted(&self, marker: (usize, u64)) -> bool {
        if self.conversation.goal.revision > marker.1
            && self.conversation.goal.pending.is_none()
            && self.conversation.goal.error.is_none()
            && self.conversation.goal.goal.is_some()
            && self.pending_goal_bubble.is_some()
        {
            return true;
        }
        self.conversation
            .submissions
            .iter()
            .skip(marker.0)
            .any(|s| s.status == crate::conversation::SubmissionStatus::Accepted)
    }
    pub(crate) fn dismiss_task_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if self.task_options_open && self.task_mode_menu {
            self.task_mode_menu = false;
            cx.notify();
            return true;
        }
        let open = self.task_options_open
            || self.menu_open
            || self.permission_menu_open
            || self.context_menu_open
            || self.slash_menu.is_some()
            || self.workspace_controls.menu.is_some();
        if open {
            self.task_options_open = false;
            self.task_mode_menu = false;
            self.close_side_menus(cx);
            self.slash_menu = None;
            self.workspace_controls.menu = None;
            self.focus_prompt_pending = true;
        }
        open
    }
    pub(super) fn toggle_task_options(&mut self, cx: &mut Context<Self>) {
        let open = !self.task_options_open;
        self.close_side_menus(cx);
        self.task_options_open = open;
        self.task_mode_menu = false;
        cx.notify();
    }
    pub(crate) fn render_task_footer(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        use gpui::{Role, div, prelude::*, px};
        let theme = crate::theme::Theme::for_mode(self.mode);
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .children(
                [
                    ("Attach files", "paperclip", true),
                    ("Voice input unavailable", "dictation", false),
                ]
                .into_iter()
                .map(|(label, glyph, attach)| {
                    div()
                        .id(if attach { "task-attach" } else { "task-voice" })
                        .role(Role::Button)
                        .aria_label(label)
                        .focusable()
                        .tab_stop(true)
                        .size(px(28.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |s| s.bg(theme.sidebar_hover))
                        .focus_visible(move |s| s.border_1().border_color(theme.text_secondary))
                        .child(
                            crate::components::icons::icon(glyph, theme.text_secondary.into())
                                .size(px(16.)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if attach {
                                this.attach_files(cx)
                            } else {
                                this.start_dictation(cx)
                            }
                            cx.stop_propagation();
                        }))
                        .on_key_down(cx.listener(move |this, e: &gpui::KeyDownEvent, _, cx| {
                            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                if attach {
                                    this.attach_files(cx)
                                } else {
                                    this.start_dictation(cx)
                                }
                                cx.stop_propagation();
                            }
                        }))
                }),
            )
            .into_any_element()
    }
    pub(crate) fn render_task_utilities(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        use crate::components::icons::icon;
        use gpui::{Role, div, prelude::*, px};
        let theme = crate::theme::Theme::for_mode(self.mode);
        let mut menu = div()
            .id("task-options-menu")
            .role(Role::Menu)
            .aria_label("Task options")
            .absolute()
            .top(px(32.))
            .left_0()
            .w(px(150.))
            .p(px(4.))
            .rounded(px(10.))
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .on_mouse_down_out(cx.listener(|this, e: &gpui::MouseDownEvent, _, cx| {
                if !this
                    .context_catalog
                    .trigger_bounds
                    .get()
                    .is_some_and(|b| b.contains(&e.position))
                {
                    this.task_options_open = false;
                    this.task_mode_menu = false;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("task-mode-options")
                    .role(Role::MenuItem)
                    .aria_label("Mode")
                    .focusable()
                    .tab_stop(true)
                    .h(px(30.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .child("Mode")
                    .child(div().flex_1())
                    .child(icon("pr-chevron-right", theme.text_secondary.into()).size(px(12.)))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.task_mode_menu = true;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.task_mode_menu = true;
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, _, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space" | "right") {
                            this.task_mode_menu = true;
                            cx.notify();
                            cx.stop_propagation();
                        }
                    })),
            );
        for (label, glyph, worktree) in [("Local", "local", false), ("Worktree", "branch", true)] {
            let enabled = !self.workspace_controls.busy
                && self.thread_id().is_none()
                && (!worktree
                    || self
                        .workspace
                        .checkout
                        .as_ref()
                        .is_some_and(crate::git_review::Checkout::is_repository));
            let selected = self.workspace_controls.worktree_mode == worktree;
            menu = menu.child(
                div()
                    .id(if worktree {
                        "task-run-worktree"
                    } else {
                        "task-run-local"
                    })
                    .role(Role::MenuItem)
                    .aria_label(label)
                    .aria_selected(selected)
                    .aria_disabled(!enabled)
                    .focusable()
                    .tab_stop(enabled)
                    .h(px(30.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .text_color(if enabled {
                        theme.text
                    } else {
                        theme.text_tertiary
                    })
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .child(icon(glyph, theme.text_secondary.into()).size(px(14.)))
                    .child(label)
                    .child(div().flex_1())
                    .when(selected, |s| {
                        s.child(icon("check", theme.text_secondary.into()).size(px(12.)))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if enabled {
                            this.select_run_location(worktree, cx);
                            this.task_options_open = false;
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(move |this, e: &gpui::KeyDownEvent, _, cx| {
                        if enabled && matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            this.select_run_location(worktree, cx);
                            this.task_options_open = false;
                            cx.notify();
                            cx.stop_propagation();
                        }
                    })),
            );
        }
        if self.workspace_controls.worktree_mode {
            let branch = self
                .workspace_controls
                .base
                .clone()
                .unwrap_or_else(|| "HEAD".into());
            menu = menu.child(self.branch_trigger(branch.into(), theme, cx));
        }
        if self.task_mode_menu {
            let mut modes = div()
                .id("task-mode-menu")
                .role(Role::Menu)
                .aria_label("Mode")
                .absolute()
                .top_0()
                .left(px(148.))
                .w(px(185.))
                .p(px(4.))
                .rounded(px(10.))
                .bg(theme.surface)
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col();
            for (label, plan) in [
                ("Default", Some(false)),
                ("Plan", Some(true)),
                ("Debug (unavailable)", None),
            ] {
                let enabled = plan.is_some_and(|p| !p || self.plan_mode_available());
                let selected = plan == self.prompt_context.plan_mode.or(Some(false));
                modes = modes.child(
                    div()
                        .id(gpui::SharedString::from(format!("task-mode-{label}")))
                        .role(Role::MenuItem)
                        .aria_label(label)
                        .aria_disabled(!enabled)
                        .aria_selected(selected)
                        .focusable()
                        .tab_stop(enabled)
                        .h(px(30.))
                        .px(px(8.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .cursor_pointer()
                        .text_color(if enabled {
                            theme.text
                        } else {
                            theme.text_tertiary
                        })
                        .hover(move |s| s.bg(theme.sidebar_hover))
                        .child(label)
                        .child(div().flex_1())
                        .when(selected, |s| {
                            s.child(icon("check", theme.text_secondary.into()).size(px(12.)))
                        })
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.select_task_mode(plan, cx)),
                        )
                        .on_key_down(cx.listener(move |this, e: &gpui::KeyDownEvent, _, cx| {
                            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                this.select_task_mode(plan, cx);
                                cx.stop_propagation();
                            }
                        })),
                );
            }
            menu = menu.child(modes);
        }
        gpui::deferred(menu).into_any_element()
    }
    fn select_task_mode(&mut self, plan: Option<bool>, cx: &mut Context<Self>) {
        if let Some(plan) = plan.filter(|p| !p || self.plan_mode_available()) {
            self.prompt_context.plan_mode = Some(plan);
        } else {
            self.show_toast(
                super::toast::ToastKind::Danger,
                "This agent does not provide this interaction mode.".into(),
                cx,
            );
        }
        self.task_options_open = false;
        self.task_mode_menu = false;
        cx.notify();
    }
}
