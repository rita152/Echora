//! Confirmation dialogs the composer raises: sending while the follow-up
//! queue is paused, and replacing the saved goal. Rendered by the app as an
//! overlay above the whole window, like the reference's modal dialogs.

use gpui::{
    AnyElement, BoxShadow, Context, FontWeight, MouseButton, Role, SharedString, Window, div,
    prelude::*, px, rgba,
};

use super::{ComposerView, ConversationChanged};
use crate::{components::icons::icon, theme::Theme};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ComposerDialog {
    /// The user sent a message while the queue is paused.
    SendWhilePaused { text: String, inverted: bool },
    /// A new objective while a goal is saved.
    ReplaceGoal { objective: String },
    /// `/memories`: the chat memories switches.
    Memories,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DialogChoice {
    /// "Clear queue" / "Cancel": the first, secondary button.
    Secondary,
    /// "Send message" / "Replace goal".
    Primary,
    Dismiss,
}

impl ComposerView {
    pub(crate) fn choose_dialog(&mut self, choice: DialogChoice, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        self.focus_prompt_pending = true;
        match (dialog, choice) {
            (_, DialogChoice::Dismiss) => {}
            (ComposerDialog::SendWhilePaused { text, inverted }, choice) => {
                // Both choices send; "Clear queue" then deletes the queued
                // messages, "Send message" lets the queue continue after it.
                self.queue_resumed = true;
                self.submit_prompt_as(text, inverted, cx);
                if choice == DialogChoice::Secondary {
                    self.clear_queue(cx);
                }
            }
            (ComposerDialog::ReplaceGoal { objective }, DialogChoice::Primary) => {
                self.set_goal(objective, cx);
            }
            (ComposerDialog::Memories, _) => {}
            (ComposerDialog::ReplaceGoal { .. }, DialogChoice::Secondary) => {
                // Cancel keeps the composer text and the Goal chip.
                if !self.goal_draft {
                    self.set_goal_draft(true, cx);
                }
            }
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub fn render_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.dialog.clone()?;
        if self.dialog_focus_pending {
            self.dialog_focus_pending = false;
            self.dialog_focus.focus(window, cx);
        }
        if dialog == ComposerDialog::Memories {
            return Some(self.render_memories_dialog(cx));
        }
        let theme = Theme::for_mode(self.mode);
        let danger = match self.mode {
            crate::theme::ThemeMode::Dark => rgba(0xff6764ff),
            crate::theme::ThemeMode::Light => rgba(0xe02e2aff),
        };
        let (title, body, quote, secondary, primary, secondary_danger) = match &dialog {
            ComposerDialog::SendWhilePaused { .. } => {
                let count = self.conversation.queue.rows.len();
                (
                    crate::i18n::format!("发送消息？" => "Send message?"),
                    if count == 1 {
                        crate::i18n::format!("你即将发送一条消息。要清除之前已排队的 1 条消息吗？" => "You are about to send a message. Do you want to clear the 1 message previously queued?")
                    } else {
                        crate::i18n::format!("你即将发送一条消息。要清除之前已排队的 {count} 条消息吗？" => "You are about to send a message. Do you want to clear the {count} messages previously queued?")
                    },
                    None,
                    crate::i18n::format!("清空队列" => "Clear queue"),
                    crate::i18n::format!("发送消息" => "Send message"),
                    true,
                )
            }
            ComposerDialog::Memories => unreachable!("rendered above"),
            ComposerDialog::ReplaceGoal { objective } => (
                crate::i18n::format!("替换当前目标吗？" => "Replace current goal?"),
                crate::i18n::format!("这会保留聊天，但会用你当前在输入框中的文本替换已保存的目标" => "This will keep the chat but replace the saved goal with your current composer text"),
                Some(objective.clone()),
                crate::i18n::format!("取消" => "Cancel"),
                crate::i18n::format!("替换目标" => "Replace goal"),
                false,
            ),
        };
        let danger_color = danger;
        // The reference's two dialogs differ: the queue one is 520 wide with
        // 36 px pill buttons, the goal one 420 wide with 32 px rounded ones and
        // its title directly above the body.
        let replace = matches!(dialog, ComposerDialog::ReplaceGoal { .. });
        let (width, button_height, button_radius, button_padding, title_gap) = if replace {
            (420.0, 32.0, 12.5, 16.0, 4.0)
        } else {
            (520.0, 36.0, 9999.0, 20.0, 12.0)
        };
        let (surface, edge) = match self.mode {
            crate::theme::ThemeMode::Dark => (rgba(0x2b2a2aff), rgba(0xffffff0d)),
            crate::theme::ThemeMode::Light => (rgba(0xfdfcfcff), rgba(0x0000000f)),
        };
        let button = |id: &'static str, label: String, primary: bool, danger: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(SharedString::from(label.clone()))
                .focusable()
                .tab_stop(true)
                .h(px(button_height))
                .px(px(button_padding))
                .rounded(px(button_radius))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.0))
                .cursor_pointer()
                .when(primary, |d| {
                    d.bg(theme.button).text_color(theme.button_text)
                })
                .when(!primary && danger, |d| {
                    d.bg(gpui::Rgba {
                        a: 0.1,
                        ..danger_color
                    })
                    .text_color(danger_color)
                })
                .when(!primary && !danger, |d| {
                    d.bg(theme.text.alpha(0.06)).text_color(theme.text)
                })
                .child(label)
        };
        Some(
            div()
                .id("composer-dialog-overlay")
                .absolute()
                .inset_0()
                // The reference dims the window by about 13% in both themes.
                .bg(rgba(0x00000021))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.choose_dialog(DialogChoice::Dismiss, cx)
                }))
                .child(
                    div()
                        .id("composer-dialog")
                        .track_focus(&self.dialog_focus)
                        .role(Role::Dialog)
                        .aria_label(SharedString::from(title.clone()))
                        .w(px(width))
                        .max_w_full()
                        .p(px(20.0))
                        .rounded(px(25.0))
                        .bg(surface)
                        .border_1()
                        .border_color(edge)
                        .text_color(theme.text)
                        .shadow(vec![
                            BoxShadow::new(px(0.0), px(8.0), rgba(0x0000001f).into())
                                .blur_radius(px(24.0)),
                        ])
                        .flex()
                        .flex_col()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                            if event.keystroke.key == "escape" {
                                this.choose_dialog(DialogChoice::Dismiss, cx);
                                cx.stop_propagation();
                            }
                        }))
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(20.0))
                                        .line_height(px(28.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(title),
                                )
                                .when(matches!(dialog, ComposerDialog::SendWhilePaused { .. }), |row| {
                                    row.child(
                                        div()
                                            .id("composer-dialog-close")
                                            .role(Role::Button)
                                            .aria_label(crate::i18n::format!("关闭对话框" => "Close dialog"))
                                            .focusable()
                                            .tab_stop(true)
                                            .size(px(24.0))
                                            .rounded(px(6.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .hover(move |s| s.bg(theme.sidebar_hover))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.choose_dialog(DialogChoice::Dismiss, cx)
                                            }))
                                            .child(icon("close-dialog", theme.text_tertiary.into()).size(px(12.0))),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .mt(px(title_gap))
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .text_color(theme.text.alpha(0.5))
                                .child(body),
                        )
                        .when_some(quote, |d, quote| {
                            d.child(
                                div()
                                    .mt(px(12.0))
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .rounded(px(12.5))
                                    .bg(theme.text.alpha(0.04))
                                    .text_size(px(13.0))
                                    .line_height(px(18.57))
                                    .max_h(px(120.0))
                                    .overflow_hidden()
                                    .child(quote),
                            )
                        })
                        .child(
                            div()
                                .mt(px(12.0))
                                .flex()
                                .justify_end()
                                .gap(px(12.0))
                                .child(
                                    button("composer-dialog-secondary", secondary, false, secondary_danger)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.choose_dialog(DialogChoice::Secondary, cx)
                                        })),
                                )
                                .child(
                                    button("composer-dialog-primary", primary, true, false)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.choose_dialog(DialogChoice::Primary, cx)
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
