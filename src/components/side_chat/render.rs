//! The side chat's content and its CDP-measured close confirmation; the
//! right panel's tab strip lists the side chats.

use super::SideChatPanel;
use crate::{
    components::icons::icon,
    theme::{CHAT_CONTENT_HORIZONTAL_GUTTER, Theme},
};
use gpui::{
    AnyElement, BoxShadow, Context, FontWeight, MouseButton, Render, Role, Window, div, prelude::*,
    px, rgba,
};

impl SideChatPanel {
    pub fn render_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.close_confirmation?;
        if self.confirm_focus_pending {
            self.confirm_focus.focus(window, cx);
            self.confirm_focus_pending = false;
        }
        let theme = Theme::for_mode(self.mode);
        let checkbox = div()
            .id("side-chat-remember-close")
            .track_focus(&self.remember_focus)
            .role(Role::CheckBox)
            .aria_label(crate::i18n::text("不再询问"))
            .aria_toggled(if self.remember_close {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .focusable()
            .tab_stop(true)
            .h(px(28.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, window, cx| {
                this.remember_focus.focus(window, cx);
                this.remember_close = !this.remember_close;
                cx.notify();
            }))
            .child(
                div()
                    .size(px(16.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(theme.text_tertiary)
                    .when(self.remember_close, |d| {
                        d.bg(theme.button)
                            .child(icon("check", theme.button_text.into()).size(px(14.0)))
                    }),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .child(crate::i18n::text("不再询问")),
            );
        let action = |id: &'static str, label: &'static str, danger: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .focusable()
                .tab_stop(true)
                .h(px(36.0))
                .px(px(14.0))
                .rounded(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .bg(if danger {
                    rgba(0xe02e2aff)
                } else {
                    theme.text.alpha(0.08)
                })
                .text_color(if danger { rgba(0xffffffff) } else { theme.text })
                .child(label)
        };
        Some(
            div()
                .id("side-chat-close-overlay")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000066))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("side-chat-close-dialog")
                        .key_context("SideChatCloseDialog")
                        .role(Role::Dialog)
                        .aria_label(crate::i18n::text("关闭侧边聊天？"))
                        .w(px(400.0))
                        .max_w_full()
                        .p(px(24.0))
                        .rounded(px(20.0))
                        .bg(theme.model_picker_surface)
                        .border(px(0.5))
                        .border_color(theme.border)
                        .text_color(theme.text)
                        .shadow(vec![
                            BoxShadow::new(px(0.0), px(16.0), rgba(0x00000044).into())
                                .blur_radius(px(40.0)),
                        ])
                        .flex()
                        .flex_col()
                        .gap(px(16.0))
                        .on_action(cx.listener(|this, _: &super::CloseDialogNext, window, cx| {
                            this.move_dialog_focus(false, window, cx)
                        }))
                        .on_action(cx.listener(
                            |this, _: &super::CloseDialogPrevious, window, cx| {
                                this.move_dialog_focus(true, window, cx)
                            },
                        ))
                        .on_action(cx.listener(
                            |this, _: &super::CloseDialogActivate, window, cx| {
                                this.activate_dialog_control(window, cx)
                            },
                        ))
                        .on_action(cx.listener(|this, _: &super::CloseDialogCancel, _, cx| {
                            this.dismiss_transient(cx);
                        }))
                        .child(
                            div()
                                .text_size(px(18.0))
                                .line_height(px(25.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(crate::i18n::text("关闭侧边聊天？")),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(20.0))
                                .text_color(theme.text_secondary)
                                .child(crate::i18n::text(
                                    "此侧边聊天将消失且无法恢复。确定要关闭吗？",
                                )),
                        )
                        .child(checkbox)
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    action(
                                        "side-chat-close-cancel",
                                        crate::i18n::text("取消"),
                                        false,
                                    )
                                    .track_focus(&self.cancel_focus)
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.close_confirmation = None;
                                            this.focus_pending = true;
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(
                                    action(
                                        "side-chat-close-confirm",
                                        crate::i18n::text("关闭侧边聊天"),
                                        true,
                                    )
                                    .track_focus(&self.confirm_focus)
                                    .on_click(cx.listener(|this, _, _, cx| this.confirm_close(cx))),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

impl Render for SideChatPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        let active = self.tabs.iter().find(|tab| Some(tab.id) == self.active);
        if self.focus_pending && self.close_confirmation.is_none() {
            if let Some(tab) = active {
                tab.composer
                    .read(cx)
                    .prompt_focus_handle(cx)
                    .focus(window, cx);
            }
            self.focus_pending = false;
        }
        let mut root = div()
            .id("side-chat-panel")
            .size_full()
            .relative()
            .bg(theme.surface)
            .key_context("SideChatPanel")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::handle_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                for tab in &this.tabs {
                    tab.composer.update(cx, |c, cx| c.close_side_menus(cx));
                }
                cx.stop_propagation();
            }));
        if let Some(tab) = active {
            root = root.child(
                div()
                    .id("side-chat-content")
                    .debug_selector(|| "side-chat-content".to_owned())
                    .size_full()
                    .px(px(CHAT_CONTENT_HORIZONTAL_GUTTER))
                    .child(tab.home.clone()),
            );
            if tab.error.is_none()
                && tab.composer.read(cx).conversation_render_snapshot().0
                    == crate::conversation::ConversationPhase::Failed
            {
                let composer = tab.composer.clone();
                let key_composer = composer.clone();
                root = root.child(
                    div()
                        .id("side-chat-retry-turn")
                        .role(Role::Button)
                        .aria_label(crate::i18n::text("重试回复"))
                        .focusable()
                        .tab_stop(true)
                        .absolute()
                        .bottom(px(tab.composer.read(cx).side_composer_height(cx) + 24.0))
                        .right(px(24.0))
                        .px(px(12.0))
                        .h(px(28.0))
                        .rounded(px(8.0))
                        .bg(theme.control_soft)
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .text_size(px(13.0))
                        .text_color(theme.text)
                        .child(crate::i18n::text("重试回复"))
                        .on_click(move |_, _, cx| {
                            composer.update(cx, |composer, cx| composer.retry_side_prompt(cx))
                        })
                        .on_key_down(move |e: &gpui::KeyDownEvent, _, cx| {
                            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                key_composer
                                    .update(cx, |composer, cx| composer.retry_side_prompt(cx));
                                cx.stop_propagation();
                            }
                        }),
                );
            }
            if let Some(error) = &tab.error {
                let id = tab.id;
                root = root.child(
                    div()
                        .absolute()
                        .top(px(8.0))
                        .left(px(CHAT_CONTENT_HORIZONTAL_GUTTER))
                        .right(px(CHAT_CONTENT_HORIZONTAL_GUTTER))
                        .p(px(12.0))
                        .rounded(px(12.0))
                        .bg(theme.control_soft)
                        .text_size(px(13.0))
                        .text_color(theme.text_secondary)
                        .child(error.clone())
                        .when(tab.thread_id.is_none(), |d| {
                            d.child(
                                div()
                                    .id("side-chat-retry-open")
                                    .role(Role::Button)
                                    .aria_label(crate::i18n::text("重试"))
                                    .focusable()
                                    .tab_stop(true)
                                    .mt(px(8.0))
                                    .cursor_pointer()
                                    .text_color(theme.text)
                                    .child(crate::i18n::text("重试"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_tab(id, cx);
                                        cx.notify();
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, e: &gpui::KeyDownEvent, _, cx| {
                                            if matches!(e.keystroke.key.as_str(), "enter" | "space")
                                            {
                                                this.open_tab(id, cx);
                                                cx.notify();
                                                cx.stop_propagation();
                                            }
                                        },
                                    )),
                            )
                        }),
                )
            }
        }
        root
    }
}
