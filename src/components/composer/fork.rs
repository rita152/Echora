//! Reply-scoped durable forks. The shell owns backend work and navigation.
use std::path::PathBuf;

use gpui::{
    AnyElement, Context, IntoElement, MouseButton, Role, SharedString, Window, div, prelude::*, px,
    rgba,
};

use super::{ComposerView, ConversationChanged, dialogs::ComposerDialog};
use crate::{components::icons::icon, conversation::ConversationPhase, theme::Theme};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ForkDialogState {
    last_turn_id: String,
    loading: bool,
    error: Option<String>,
    worktree_cwd: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct ForkConversationRequest {
    pub thread_id: String,
    pub last_turn_id: String,
    pub cwd: PathBuf,
    pub worktree: bool,
    pub worktree_cwd: Option<PathBuf>,
}
impl gpui::EventEmitter<ForkConversationRequest> for ComposerView {}

pub(crate) struct OpenForkSource(pub String);
impl gpui::EventEmitter<OpenForkSource> for ComposerView {}

impl ComposerView {
    pub(crate) fn can_fork_reply(&self, turn_id: &str) -> bool {
        self.thread_id().is_some()
            && (self.conversation.turn_id.as_deref() == Some(turn_id)
                && matches!(
                    self.conversation.phase,
                    ConversationPhase::Complete | ConversationPhase::Stopped
                )
                || self.conversation.transcript.iter().any(|turn| {
                    turn.turn_id.as_deref() == Some(turn_id)
                        && matches!(
                            turn.phase,
                            ConversationPhase::Complete | ConversationPhase::Stopped
                        )
                }))
    }

    pub(crate) fn open_response_fork(&mut self, turn_id: String, cx: &mut Context<Self>) {
        if self.dialog.is_some() || !self.can_fork_reply(&turn_id) {
            return;
        }
        self.dialog = Some(ComposerDialog::Fork(ForkDialogState {
            last_turn_id: turn_id,
            loading: false,
            error: None,
            worktree_cwd: None,
        }));
        self.dialog_focus_pending = true;
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub(super) fn dismiss_fork_dialog(&mut self, cx: &mut Context<Self>) {
        if matches!(&self.dialog, Some(ComposerDialog::Fork(state)) if !state.loading) {
            self.dialog = None;
            self.focus_prompt_pending = true;
            cx.emit(ConversationChanged);
            cx.notify();
        }
    }

    pub(super) fn submit_response_fork(&mut self, worktree: bool, cx: &mut Context<Self>) {
        let Some(thread_id) = self.thread_id().map(ToOwned::to_owned) else {
            return;
        };
        let Some(ComposerDialog::Fork(state)) = self.dialog.as_ref() else {
            return;
        };
        if state.loading || !self.can_fork_reply(&state.last_turn_id) {
            return;
        }
        if worktree
            && !self
                .workspace
                .checkout
                .as_ref()
                .is_some_and(crate::git_review::Checkout::is_repository)
        {
            return;
        }
        let request = ForkConversationRequest {
            thread_id,
            last_turn_id: state.last_turn_id.clone(),
            cwd: self.conversation.cwd.clone(),
            worktree,
            worktree_cwd: state.worktree_cwd.clone(),
        };
        if let Some(ComposerDialog::Fork(state)) = self.dialog.as_mut() {
            state.loading = true;
            state.error = None;
        }
        cx.emit(request);
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub(crate) fn settle_response_fork(
        &mut self,
        result: Result<(), String>,
        worktree_cwd: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(ComposerDialog::Fork(state)) = self.dialog.as_mut() else {
            return;
        };
        match result {
            Ok(()) => {
                self.dialog = None;
                self.focus_prompt_pending = true;
            }
            Err(error) => {
                state.loading = false;
                state.error = Some(error);
                state.worktree_cwd = worktree_cwd;
            }
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub(crate) fn fork_origin(&self) -> Option<(String, Option<String>)> {
        Some((
            self.conversation.forked_from_id.clone()?,
            self.conversation.fork_boundary_turn_id.clone(),
        ))
    }

    pub(crate) fn set_fork_source_turns(
        &mut self,
        source_id: &str,
        ids: &[String],
        cx: &mut Context<Self>,
    ) {
        self.conversation.set_fork_source_turns(source_id, ids);
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub(super) fn render_fork_dialog(
        &mut self,
        state: ForkDialogState,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::for_mode(self.mode);
        let repository = self
            .workspace
            .checkout
            .as_ref()
            .is_some_and(crate::git_review::Checkout::is_repository);
        let title = crate::i18n::format!("从这里创建聊天分支" => "Fork chat from here");
        let mut choices = div().flex().flex_col().gap(px(4.0));
        for worktree in [false, true] {
            let enabled = !state.loading && (!worktree || repository);
            let (id, label, detail) = if worktree {
                (
                    "fork-new-worktree",
                    crate::i18n::format!("在新工作树中创建分支" => "Fork in new worktree"),
                    crate::i18n::format!("在新工作树中从此消息创建分支" => "Branch from this message in a new worktree"),
                )
            } else {
                (
                    "fork-current-workspace",
                    crate::i18n::format!("在此工作空间中创建分支" => "Fork in this workspace"),
                    crate::i18n::format!("在当前工作空间中从此消息创建分支" => "Branch from this message in the current workspace"),
                )
            };
            choices = choices.child(
                div()
                    .id(id)
                    .debug_selector(move || id.to_owned())
                    .role(Role::Button)
                    .aria_label(SharedString::from(label.clone()))
                    .aria_disabled(!enabled)
                    .focusable()
                    .tab_stop(enabled)
                    .rounded(px(10.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .flex()
                    .items_start()
                    .gap(px(12.0))
                    .when(!enabled, |d| d.opacity(0.45))
                    .when(enabled, |d| {
                        d.cursor_pointer().hover(|d| d.bg(theme.text.alpha(0.05)))
                    })
                    .focus_visible(|d| d.bg(theme.text.alpha(0.08)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if enabled {
                            this.submit_response_fork(worktree, cx);
                        }
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            if enabled {
                                this.submit_response_fork(worktree, cx);
                            }
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        icon("message-branch", theme.text.into())
                            .size(px(16.0))
                            .mt(px(2.0)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(crate::typography::ui_size(14.0)))
                                    .line_height(px(20.0))
                                    .child(label),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::typography::ui_size(12.0)))
                                    .line_height(px(16.0))
                                    .text_color(theme.text_tertiary)
                                    .child(detail),
                            ),
                    ),
            );
        }
        div().id("fork-dialog-overlay").absolute().inset_0().bg(rgba(0x00000021))
            .flex().items_center().justify_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.dismiss_fork_dialog(cx)))
            .child(div().id("fork-dialog").debug_selector(|| "fork-dialog".to_owned())
                .track_focus(&self.dialog_focus).role(Role::Dialog).aria_label(SharedString::from(title.clone()))
                .w(px(420.0)).max_w_full().p(px(20.0)).rounded(px(24.0)).bg(theme.surface).text_color(theme.text)
                .flex().flex_col().gap(px(12.0))
                .on_click(|_, _, cx| cx.stop_propagation())
                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "escape" => this.dismiss_fork_dialog(cx),
                        "tab" => if event.keystroke.modifiers.shift { window.focus_prev(cx); } else { window.focus_next(cx); },
                        "enter" | "space" => this.submit_response_fork(false, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .child(div().text_size(px(crate::typography::ui_size(20.0))).line_height(px(28.0)).font_weight(gpui::FontWeight::SEMIBOLD).child(title))
                .child(choices)
                .when(state.loading, |d| d.child(div().id("fork-loading").role(Role::Status).aria_label(crate::i18n::format!("正在创建聊天分支…" => "Creating chat branch…")).text_size(px(12.0)).text_color(theme.text_tertiary).child(crate::i18n::format!("正在创建聊天分支…" => "Creating chat branch…"))))
                .when_some(state.error, |d, error| d.child(div().id("fork-error").role(Role::Alert).aria_label(SharedString::from(error.clone())).text_size(px(12.0)).text_color(rgba(0xe05c5cff)).child(error))))
            .into_any_element()
    }
}
