//! `/memories` and the "Chat memories" dialog. Before a chat starts both
//! switches are this composer's choice, sent with `thread/start`; once it has
//! started, "Use memories" is fixed and "Generate memories" is changed with
//! `thread/memoryMode/set`, shown at once and rolled back if it fails.

use gpui::{AnyElement, Context, MouseButton, Role, SharedString, div, prelude::*, px, rgba};

use super::{ComposerView, ConversationChanged, dialogs::ComposerDialog, toast::ToastKind};
use crate::{
    agent::{AgentMemoryConfig, AgentMemoryPreferences, AgentThreadMemoryMode},
    components::icons::icon,
    conversation::MemoryModeSettled,
    theme::{Theme, ThemeMode},
};

impl ComposerView {
    /// `/memories` exists only while the `memories` feature is enabled, as in
    /// the reference.
    pub(super) fn memories_feature_enabled(&self) -> bool {
        self.memories_feature.is_some_and(|(generation, enabled)| {
            enabled && generation >= self.conversation.runtime.generation
        })
    }

    /// Reads the feature list once per connection generation.
    pub(super) fn ensure_memories_feature(&mut self, cx: &mut Context<Self>) {
        let known = self
            .memories_feature
            .is_some_and(|(generation, _)| generation >= self.conversation.runtime.generation);
        if known || self.memories_feature_loading || self.side_chat {
            return;
        }
        self.memories_feature_loading = true;
        let receiver = self.backend.list_experimental_features(None);
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await;
            let _ = this.update(cx, |this, cx| {
                this.memories_feature_loading = false;
                match result {
                    Ok(Ok(features)) => {
                        let enabled = features
                            .get("memories")
                            .is_some_and(|feature| feature.enabled);
                        this.memories_feature = Some((features.generation, enabled));
                        // The menu may already be open for this query.
                        cx.notify();
                    }
                    Ok(Err(error)) => eprintln!("experimentalFeature/list 读取失败：{error}"),
                    Err(_) => {}
                }
            });
        })
        .detach();
    }

    fn memory_defaults(&self) -> AgentMemoryConfig {
        self.permission_config
            .as_ref()
            .map(|config| AgentMemoryConfig::from_effective(&config.effective))
            .unwrap_or_else(|| AgentMemoryConfig::from_effective(&serde_json::Value::Null))
    }

    /// The switches as the dialog shows them.
    pub(crate) fn memory_switches(&self) -> AgentMemoryPreferences {
        let defaults = self.memory_defaults().preferences();
        let memory = &self.conversation.memory;
        if self.conversation.thread_id.is_none() {
            return memory.new_chat.unwrap_or(defaults);
        }
        AgentMemoryPreferences {
            use_memories: memory.use_memories.unwrap_or(defaults.use_memories),
            generate_memories: memory
                .generate_memories
                .unwrap_or(defaults.generate_memories),
        }
    }

    /// The choice a new chat sends with `thread/start`, once the user made it.
    pub(super) fn new_chat_memory(&self) -> Option<AgentMemoryPreferences> {
        self.conversation
            .thread_id
            .is_none()
            .then_some(self.conversation.memory.new_chat)
            .flatten()
    }

    pub(super) fn open_memories_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialog = Some(ComposerDialog::Memories);
        self.dialog_focus_pending = true;
        cx.notify();
    }

    pub(super) fn toggle_use_memories(&mut self, cx: &mut Context<Self>) {
        // Fixed once the chat has started.
        if self.conversation.thread_id.is_some() || self.is_running() {
            return;
        }
        let mut switches = self.memory_switches();
        switches.use_memories = !switches.use_memories;
        self.conversation.memory.new_chat = Some(switches);
        cx.notify();
    }

    pub(super) fn toggle_generate_memories(&mut self, cx: &mut Context<Self>) {
        let switches = self.memory_switches();
        let value = !switches.generate_memories;
        let Some(thread_id) = self.conversation.thread_id.clone() else {
            if self.is_running() {
                return;
            }
            self.conversation.memory.new_chat = Some(AgentMemoryPreferences {
                generate_memories: value,
                ..switches
            });
            cx.notify();
            return;
        };
        let generation = self.conversation.runtime.generation;
        let Some(operation) = self.conversation.memory.begin_generate(
            &thread_id,
            generation,
            value,
            switches.generate_memories,
        ) else {
            return;
        };
        let mode = if value {
            AgentThreadMemoryMode::Enabled
        } else {
            AgentThreadMemoryMode::Disabled
        };
        let receiver = self
            .backend
            .set_thread_memory_mode(thread_id.clone(), generation, mode);
        cx.spawn(async move |this, cx| {
            let result = receiver
                .recv()
                .await
                .unwrap_or_else(|_| Err(crate::i18n::text("记忆设置连接已关闭").into()));
            let _ = this.update(cx, |this, cx| {
                match this
                    .conversation
                    .memory
                    .settle_generate(operation, &thread_id, generation, result)
                {
                    MemoryModeSettled::RolledBack(error) => {
                        eprintln!("thread/memoryMode/set 失败：{error}");
                        this.show_toast(
                            ToastKind::Danger,
                            crate::i18n::format!("无法更新聊天记忆设置" => "Unable to update chat memory settings"),
                            cx,
                        );
                    }
                    MemoryModeSettled::Applied | MemoryModeSettled::Ignored => {}
                }
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn close_memories_dialog(&mut self, cx: &mut Context<Self>) {
        if self.dialog == Some(ComposerDialog::Memories) {
            self.dialog = None;
            self.focus_prompt_pending = true;
            cx.notify();
        }
    }

    /// 400 px wide: an icon tile, the title and scope line, two bordered
    /// switch rows and a full-width Done, as the reference measures.
    pub(super) fn render_memories_dialog(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::for_mode(self.mode);
        let (surface, edge) = match self.mode {
            ThemeMode::Dark => (rgba(0x2b2a2aff), rgba(0xffffff0d)),
            ThemeMode::Light => (rgba(0xfdfcfcff), rgba(0x0000000f)),
        };
        let started = self.conversation.thread_id.is_some();
        let locked_new_chat = !started && self.is_running();
        let switches = self.memory_switches();
        let generating = self.conversation.memory.pending.is_some();
        let switch = |id: &'static str, checked: bool, disabled: bool| {
            div()
                .id(id)
                .role(Role::Switch)
                .aria_toggled(if checked {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .w(px(32.0))
                .h(px(20.0))
                .flex_none()
                .p(px(2.0))
                .rounded_full()
                .flex()
                .items_center()
                .when(checked, |track| track.justify_end().bg(rgba(0x339cffff)))
                .when(!checked, |track| {
                    track.justify_start().bg(theme.settings_switch_off)
                })
                .when(disabled, |track| track.opacity(0.5))
                .when(!disabled, |track| track.cursor_pointer())
                .child(
                    div()
                        .size(px(16.0))
                        .rounded_full()
                        .bg(gpui::white())
                        .border_1()
                        .border_color(rgba(0x00000012)),
                )
        };
        let option = |label: String, description: String, control: gpui::Stateful<gpui::Div>| {
            div()
                .p(px(12.0))
                .rounded(px(12.5))
                .border_1()
                .border_color(theme.border)
                .flex()
                .items_start()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .text_size(px(13.0))
                        .line_height(px(18.5714))
                        .child(div().font_weight(gpui::FontWeight(500.0)).child(label))
                        .child(
                            div()
                                .text_color(theme.settings_description)
                                .child(description),
                        ),
                )
                .child(control)
        };
        let use_row = option(
            crate::i18n::format!("使用记忆" => "Use memories"),
            if started {
                crate::i18n::format!("对话开始后无法更改" => "Cannot be changed after conversation has started")
            } else {
                crate::i18n::format!("允许 ChatGPT 将现有记忆带入此聊天的上下文" => "Let ChatGPT bring existing memories into this chat's context")
            },
            switch(
                "memories-use",
                switches.use_memories,
                started || locked_new_chat,
            )
            .aria_label(crate::i18n::format!("使用记忆" => "Use memories"))
            .when(!started && !locked_new_chat, |toggle| {
                toggle.on_click(cx.listener(|this, _, _, cx| this.toggle_use_memories(cx)))
            }),
        );
        let generate_locked = generating || locked_new_chat;
        let generate_row = option(
            crate::i18n::format!("生成记忆" => "Generate memories"),
            crate::i18n::format!("允许 ChatGPT 日后创建新记忆时使用这次聊天" => "Allow ChatGPT to use this chat when creating new memories later"),
            switch(
                "memories-generate",
                switches.generate_memories,
                generate_locked,
            )
            .aria_label(crate::i18n::format!("生成记忆" => "Generate memories"))
            .when(!generate_locked, |toggle| {
                toggle.on_click(cx.listener(|this, _, _, cx| this.toggle_generate_memories(cx)))
            }),
        );
        let title = crate::i18n::format!("聊天记忆" => "Chat memories");
        div()
            .id("composer-dialog-overlay")
            .absolute()
            .inset_0()
            .bg(rgba(0x00000021))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.close_memories_dialog(cx)))
            .child(
                div()
                    .id("composer-memories-dialog")
                        .relative()
                    .track_focus(&self.dialog_focus)
                    .role(Role::Dialog)
                    .aria_label(SharedString::from(title.clone()))
                    .w(px(400.0))
                    .max_w_full()
                    .p(px(20.0))
                    .rounded(px(25.0))
                    .bg(surface)
                    .border_1()
                    .border_color(edge)
                    .text_color(theme.text)
                    .flex()
                    .flex_col()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "escape" {
                            this.close_memories_dialog(cx);
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
                                    .size(px(36.0))
                                    .rounded(px(15.0))
                                    .bg(theme.text.alpha(0.05))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon("slash-memories", theme.text.into()).size(px(18.0))),
                            )
                            .child(
                                div()
                                    .id("composer-memories-close")
                                    // The reference pins it 16 px from the corner.
                                    .absolute()
                                    .top(px(16.0))
                                    .right(px(16.0))
                                    .role(Role::Button)
                                    .aria_label(crate::i18n::format!("关闭对话框" => "Close dialog"))
                                    .size(px(24.0))
                                    .rounded(px(6.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(move |button| button.bg(theme.sidebar_hover))
                                    .on_click(cx.listener(|this, _, _, cx| this.close_memories_dialog(cx)))
                                    .child(icon("close-dialog", theme.text_tertiary.into()).size(px(16.0))),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(12.0))
                            .text_size(px(20.0))
                            .line_height(px(28.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .text_color(theme.text.alpha(0.5))
                            .child(if started {
                                crate::i18n::format!("这些开关适用于当前聊天" => "These switches apply to the current chat")
                            } else {
                                crate::i18n::format!("这些开关适用于从此输入框发起的聊天" => "These switches apply to the chat started from this composer")
                            }),
                    )
                    .child(
                        div()
                            .mt(px(32.0))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(use_row)
                            .child(generate_row),
                    )
                    .child(
                        div()
                            .id("composer-memories-done")
                            .role(Role::Button)
                            .aria_label(crate::i18n::format!("完成" => "Done"))
                            .mt(px(20.0))
                            .h(px(32.0))
                            .rounded(px(12.5))
                            .bg(theme.text.alpha(0.05))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(14.0))
                            .cursor_pointer()
                            .hover(move |button| button.bg(theme.text.alpha(0.08)))
                            .on_click(cx.listener(|this, _, _, cx| this.close_memories_dialog(cx)))
                            .child(crate::i18n::format!("完成" => "Done")),
                    ),
            )
            .into_any_element()
    }
}
