//! `!` shell mode: a composer line that starts with `!` runs the rest as a
//! shell command in the chat's thread (`thread/shellCommand`).
//!
//! The reference has no entry point for this method; the mode follows the
//! Codex CLI's `!` prefix. The server evaluates the command with the thread's
//! shell outside the sandbox, so the composer says so while the mode is on.
//! The command's output arrives as the thread's own turn: a new one on an idle
//! thread (the server starts it and the hub offers it here), or inside the
//! running turn.

use gpui::{AnyElement, Context, Role, div, prelude::*, px};

use super::{ComposerView, ConversationChanged, ConversationThreadCreated, toast::ToastKind};
use crate::{
    agent::{AgentEvent, AgentShellCommandRequest, AgentThreadTarget},
    components::icons::icon,
    theme::Theme,
};

/// The command a composer text runs in shell mode, when it is one.
pub(crate) fn shell_command(text: &str) -> Option<&str> {
    text.trim_start().strip_prefix('!').map(str::trim)
}

impl ComposerView {
    /// Shell mode: not in a side chat, which has no workspace of its own.
    pub(super) fn shell_mode(&self, cx: &gpui::App) -> bool {
        !self.side_chat && shell_command(self.prompt_text(cx)).is_some()
    }

    pub(super) fn render_shell_chip(&self, theme: Theme, cx: &gpui::App) -> Option<AnyElement> {
        if !self.shell_mode(cx) {
            return None;
        }
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                .child(
                    div()
                        .id("composer-shell-chip")
                        .debug_selector(|| "COMPOSER_SHELL_MODE".to_owned())
                        .role(Role::Status)
                        .aria_label(crate::i18n::format!(
                            "Shell 命令，在沙盒外运行" => "Shell command, runs outside the sandbox"
                        ))
                        .h(px(28.0))
                        .px(px(8.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(theme.warning)
                        .child(icon("panel-terminal", theme.warning.into()).size(px(16.0)))
                        .child(crate::i18n::format!("Shell · 在沙盒外运行" => "Shell · runs outside the sandbox")),
                )
                .into_any_element(),
        )
    }

    /// Runs the composer's `!` command. An empty command is not sent, and the
    /// text stays. A rejected command is reported and its text restored.
    pub(super) fn submit_shell_command(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(command) = shell_command(&text).map(str::to_owned) else {
            return;
        };
        if command.is_empty() {
            return;
        }
        if self.conversation.thread_id.is_none()
            && (self.permission_catalog_loading || self.permission_catalog_error.is_some())
        {
            self.submission_error =
                Some(self.permission_catalog_error.clone().unwrap_or_else(|| {
                    crate::i18n::text("正在读取新会话配置，输入已保留。").into()
                }));
            cx.notify();
            return;
        }
        self.submission_error = None;
        self.clear_prompt(cx);
        self.focus_prompt_pending = true;
        let receiver = self.backend.run_shell_command(AgentShellCommandRequest {
            thread: AgentThreadTarget {
                thread_id: self.conversation.thread_id.clone(),
                cwd: self.conversation.cwd.clone(),
                project_id: self.conversation.project_id.clone(),
                model: self.conversation.selected_model.clone(),
                service_tier: self.conversation.selected_service_tier.clone(),
                permission_mode: self.selected_agent_permission_mode(),
            },
            command,
            timeout_ms: None,
        });
        let cycle = self.conversation.cycle;
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::i18n::format!("Shell 命令连接已关闭" => "The shell command connection closed").to_owned())
            });
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(started) => {
                        if started.created_thread && this.conversation.thread_id.is_none() {
                            this.apply_agent_event_batch(vec![AgentEvent::ThreadCreated {
                                thread_id: started.thread_id.clone(),
                            }]);
                            cx.emit(ConversationThreadCreated {
                                thread_id: started.thread_id.clone(),
                            });
                            this.sync_thread_scoped_state(cx);
                        }
                        // The command's turn may have been offered before
                        // this chat knew its thread.
                        if this.conversation.thread_id.as_deref() == Some(started.thread_id.as_str())
                            && let Some((_, run)) = this.backend.take_server_turn(&started.thread_id)
                        {
                            this.pending_external_turns.push_back(run);
                            this.attach_pending_external_turn(cx);
                        }
                    }
                    Err(error) => {
                        if this.conversation.cycle == cycle && this.draft_is_empty(cx) {
                            this.prompt_editor
                                .update(cx, |editor, cx| editor.set_text_silently(&text, cx));
                        }
                        this.show_toast(
                            ToastKind::Danger,
                            crate::i18n::format!("无法运行 Shell 命令：{error}" => "Couldn't run the shell command: {error}"),
                            cx,
                        );
                    }
                }
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
        cx.emit(ConversationChanged);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::shell_command;

    #[test]
    fn a_leading_bang_is_the_shell_prefix() {
        assert_eq!(shell_command("!ls -la"), Some("ls -la"));
        assert_eq!(shell_command("  ! git status "), Some("git status"));
        assert_eq!(shell_command("!"), Some(""));
        assert_eq!(shell_command("hello !ls"), None);
        assert_eq!(shell_command(""), None);
    }
}
