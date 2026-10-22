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
        let open = self.task_options_open
            || self.menu_open
            || self.permission_menu_open
            || self.context_menu_open
            || self.slash_menu.is_some()
            || self.workspace_controls.menu.is_some();
        if open {
            self.task_options_open = false;
            self.close_side_menus(cx);
            self.slash_menu = None;
            self.workspace_controls.menu = None;
            self.focus_prompt_pending = true;
        }
        open
    }
    pub(crate) fn render_task_utilities(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        use gpui::{Role, div, prelude::*, px};
        let theme = crate::theme::Theme::for_mode(self.mode);
        let branch = self.workspace_controls.base.clone().or_else(|| {
            self.workspace
                .checkout
                .as_ref()
                .and_then(crate::git_review::Checkout::branch)
                .map(str::to_owned)
        });
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .child(self.location_trigger(theme, cx))
            .when_some(branch, |s, branch| {
                s.child(self.branch_trigger(branch.into(), theme, cx))
            })
            .child(
                div()
                    .relative()
                    .child(
                        div()
                            .id("task-mode-options")
                            .role(Role::Button)
                            .aria_label("Task options")
                            .focusable()
                            .tab_stop(true)
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(move |s| s.bg(theme.sidebar_hover))
                            .child(if self.prompt_context.plan_mode == Some(true) {
                                "Plan · ⋯"
                            } else {
                                "Default · ⋯"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.task_options_open = !this.task_options_open;
                                cx.notify();
                            }))
                            .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, _, cx| {
                                if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                    this.task_options_open = !this.task_options_open;
                                    cx.notify();
                                    cx.stop_propagation();
                                }
                            })),
                    )
                    .when(self.task_options_open, |s| {
                        s.child(gpui::deferred(
                            div()
                                .id("task-mode-menu")
                                .role(Role::Menu)
                                .absolute()
                                .bottom(px(32.))
                                .left_0()
                                .w(px(205.))
                                .p(px(6.))
                                .rounded(px(10.))
                                .bg(theme.surface)
                                .border_1()
                                .border_color(theme.border)
                                .flex()
                                .flex_col()
                                .children(
                                    [
                                        ("Default", Some(false)),
                                        ("Plan", Some(true)),
                                        ("Debug (unavailable)", None),
                                    ]
                                    .into_iter()
                                    .map(|(label, plan)| {
                                        let enabled =
                                            plan.is_some_and(|p| !p || self.plan_mode_available());
                                        div()
                                            .id(gpui::SharedString::from(format!(
                                                "task-mode-{label}"
                                            )))
                                            .role(Role::MenuItem)
                                            .aria_label(label)
                                            .aria_disabled(!enabled)
                                            .focusable()
                                            .tab_stop(true)
                                            .px(px(8.))
                                            .py(px(6.))
                                            .rounded(px(6.))
                                            .text_size(px(13.))
                                            .text_color(if enabled {
                                                theme.text
                                            } else {
                                                theme.text_tertiary
                                            })
                                            .cursor_pointer()
                                            .hover(move |s| s.bg(theme.sidebar_hover))
                                            .child(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.select_task_mode(plan, cx)
                                            }))
                                            .on_key_down(cx.listener(
                                                move |this, e: &gpui::KeyDownEvent, _, cx| {
                                                    if matches!(
                                                        e.keystroke.key.as_str(),
                                                        "enter" | "space"
                                                    ) {
                                                        this.select_task_mode(plan, cx);
                                                        cx.stop_propagation();
                                                    }
                                                },
                                            ))
                                    }),
                                ),
                        ))
                    }),
            )
            .into_any_element()
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
        cx.notify();
    }
}
