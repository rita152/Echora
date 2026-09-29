//! Personalization → Codex memory: the memory switches (user config writes)
//! and "Delete Codex memories" (`memory/reset` after a confirmation).

use gpui::{
    AnyElement, Context, IntoElement, MouseButton, Role, SharedString, div, prelude::*, px, rgba,
};

use super::{
    SettingsView,
    dynamic::{card, danger_color, dialog_surface, row, switch},
};
use crate::{
    agent::{AgentMemoryConfig, memory_enable_edits, memory_tool_assisted_edits},
    configuration::ConfigOperation,
    theme::Theme,
};

/// Deleting memories: asked, then sent once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum MemoryReset {
    #[default]
    Idle,
    Confirming,
    Resetting,
}

const ENABLE_SWITCH: &str = "memories.enable";

const TOOL_ASSISTED_SWITCH: &str = "memories.tool_assisted";

impl SettingsView {
    fn memory_config(&self) -> Option<AgentMemoryConfig> {
        self.config_editor
            .snapshot
            .as_ref()
            .map(|snapshot| AgentMemoryConfig::from_effective(&snapshot.effective))
    }

    /// Reads whether consolidated memory is ready. A failure hides the row
    /// and is logged; it is not retried until the page is shown again.
    pub(super) fn refresh_memory_status(&mut self, cx: &mut Context<Self>) {
        self.memory_status_cycle = self.memory_status_cycle.wrapping_add(1);
        let cycle = self.memory_status_cycle;
        let receiver = self
            .backend
            .read_memory_status(crate::agent::MEMORY_V2_REQUIRED_THREADS);
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await;
            let _ = this.update(cx, |this, cx| {
                if this.memory_status_cycle != cycle {
                    return;
                }
                this.memory_status = match result {
                    Ok(Ok(status)) => Some(status),
                    Ok(Err(error)) => {
                        eprintln!("memory/status 读取失败：{error}");
                        None
                    }
                    Err(_) => None,
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn reset_memories(&mut self, cx: &mut Context<Self>) {
        if self.memory_reset != MemoryReset::Confirming {
            return;
        }
        self.memory_reset = MemoryReset::Resetting;
        let receiver = self.backend.reset_memories();
        cx.spawn(async move |this, cx| {
            let result = receiver
                .recv()
                .await
                .unwrap_or_else(|_| Err(crate::i18n::text("记忆删除连接已关闭").into()));
            let _ = this.update(cx, |this, cx| {
                this.memory_reset = MemoryReset::Idle;
                // The reference refreshes nothing after a reset: the switches
                // are config, and the memories themselves are not listed here.
                match result {
                    Ok(()) => this.show_toast(
                        crate::components::composer::ToastKind::Success,
                        crate::i18n::format!("Codex 记忆已删除" => "Codex memories deleted"),
                        cx,
                    ),
                    Err(error) => {
                        eprintln!("memory/reset 失败：{error}");
                        this.show_toast(
                            crate::components::composer::ToastKind::Danger,
                            crate::i18n::format!("无法删除 Codex 记忆" => "Unable to delete Codex memories"),
                            cx,
                        )
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn memory_card(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let config = self.memory_config();
        let read_failed = matches!(self.config_editor.operation, ConfigOperation::ReadFailed(_))
            || (self.features.list.is_none() && self.features.error.is_some());
        let loading = !read_failed && (config.is_none() || self.features.list.is_none());
        if read_failed {
            let retry = div()
                .id("memory-retry")
                .role(Role::Button)
                .h(px(28.0))
                .px(px(10.0))
                .rounded(px(10.0))
                .bg(theme.text.alpha(0.06))
                .flex()
                .items_center()
                .text_size(px(13.0))
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.reload_config(cx);
                    this.refresh_features(cx);
                }))
                .child(crate::i18n::format!("重试" => "Retry"));
            return card(theme)
                .child(row(
                    crate::i18n::format!("无法加载 Codex 记忆设置" => "Unable to load Codex memory settings"),
                    None,
                    Some(retry.into_any_element()),
                    true,
                    theme,
                ))
                .into_any_element();
        }
        if loading {
            return card(theme)
                .child(row(
                    crate::i18n::format!("正在加载…" => "Loading…"),
                    None,
                    None,
                    true,
                    theme,
                ))
                .into_any_element();
        }
        let Some(feature) = self.features.memories() else {
            return card(theme)
                .child(row(
                    crate::i18n::format!("这台电脑不支持 Codex 记忆功能" => "Codex memory isn't available on this computer"),
                    None,
                    None,
                    true,
                    theme,
                ))
                .into_any_element();
        };
        let config = config.expect("loaded above");
        let writing = self.features.writing() || self.memory_reset == MemoryReset::Resetting;
        let enabled = self.features.displayed(
            ENABLE_SWITCH,
            feature.enabled && config.generate_memories && config.use_memories,
        );
        let tool_assisted = self
            .features
            .displayed(TOOL_ASSISTED_SWITCH, !config.disable_on_external_context);
        let failure = |target: &str| {
            self.features.failure_for(target).map(|write| {
                if write.overridden {
                    crate::i18n::format!("已写入，但被更高优先级配置覆盖" => "Saved, but a higher-priority config layer overrides it")
                } else {
                    write.failure.clone().unwrap_or_default()
                }
            })
        };
        let enable_switch = switch("memory-enable", enabled, writing, theme)
            .aria_label(crate::i18n::format!("启用 Codex 记忆" => "Enable Codex memories"))
            .when(!writing, |toggle| {
                toggle.on_click(cx.listener(move |this, _, _, cx| {
                    this.write_switch(
                        ENABLE_SWITCH.into(),
                        !enabled,
                        memory_enable_edits(!enabled),
                        false,
                        cx,
                    )
                }))
            });
        let tool_locked = writing || !feature.enabled;
        let tool_switch = switch("memory-tool-assisted", tool_assisted, tool_locked, theme)
            .aria_label(crate::i18n::format!("允许从使用工具的聊天中生成记忆" => "Allow memories from tool-assisted chats"))
            .when(!tool_locked, |toggle| {
                toggle.on_click(cx.listener(move |this, _, _, cx| {
                    this.write_switch(
                        TOOL_ASSISTED_SWITCH.into(),
                        !tool_assisted,
                        memory_tool_assisted_edits(!tool_assisted),
                        false,
                        cx,
                    )
                }))
            });
        let danger = danger_color(self.mode);
        let resetting = self.memory_reset == MemoryReset::Resetting;
        let delete = div()
            .id("memory-delete")
            .role(Role::Button)
            .aria_label(crate::i18n::format!("删除" => "Delete"))
            .h(px(28.0))
            .px(px(12.0))
            .flex_none()
            .rounded(px(10.0))
            .bg(gpui::Rgba { a: 0.1, ..danger })
            .flex()
            .items_center()
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(danger)
            .when(resetting, |button| button.opacity(0.5))
            .when(!resetting, |button| {
                button
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.memory_reset = MemoryReset::Confirming;
                        cx.notify();
                    }))
            })
            .child(crate::i18n::format!("删除" => "Delete"));
        card(theme)
            .child(row(
                crate::i18n::format!("启用 Codex 记忆功能" => "Enable Codex memories"),
                Some(failure(ENABLE_SWITCH).unwrap_or_else(|| {
                    crate::i18n::format!(
                        "根据本地上的聊天创建记忆，并使用这些记忆为今后在本地上的聊天提供个性化体验" =>
                        "Create memories from chats on Local and use them to personalize future chats on Local"
                    )
                })),
                Some(enable_switch.into_any_element()),
                false,
                theme,
            ))
            .child(row(
                crate::i18n::format!("允许从使用工具的聊天中生成记忆" => "Allow memories from tool-assisted chats"),
                Some(failure(TOOL_ASSISTED_SWITCH).unwrap_or_else(|| {
                    crate::i18n::format!("从使用过 MCP 工具或网页搜索的聊天生成记忆" => "Generate memories from chats that used MCP tools or web search")
                })),
                Some(tool_switch.into_any_element()),
                false,
                theme,
            ))
            .when_some(
                self.memory_status
                    .filter(|_| enabled)
                    .map(crate::conversation::memory_status_line),
                |card, subtitle| {
                    card.child(row(
                        crate::i18n::format!("记忆整合" => "Memory consolidation"),
                        Some(subtitle),
                        None,
                        false,
                        theme,
                    ))
                },
            )
            .child(row(
                crate::i18n::format!("删除 Codex 记忆" => "Delete Codex memories"),
                Some(crate::i18n::format!("删除本地上的全部 Codex 记忆" => "Delete all Codex memories for Local")),
                Some(delete.into_any_element()),
                true,
                theme,
            ))
            .into_any_element()
    }

    pub(super) fn dismiss_memory_dialog(&mut self, cx: &mut Context<Self>) -> bool {
        if self.memory_reset != MemoryReset::Confirming {
            return false;
        }
        self.memory_reset = MemoryReset::Idle;
        cx.notify();
        true
    }

    /// "Delete all Codex memories?", 420 px wide like the reference.
    pub(super) fn memory_reset_overlay(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.memory_reset != MemoryReset::Confirming {
            return None;
        }
        let (surface, edge) = dialog_surface(self.mode);
        let danger = danger_color(self.mode);
        let button = |id: &'static str, label: String, destructive: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(SharedString::from(label.clone()))
                .h(px(32.0))
                .px(px(16.0))
                .rounded(px(12.5))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.0))
                .cursor_pointer()
                // Delete is tinted, Cancel a text button, as the reference.
                .when(destructive, |button| {
                    button
                        .bg(gpui::Rgba { a: 0.1, ..danger })
                        .text_color(danger)
                })
                .when(!destructive, |button| {
                    button.text_color(theme.text.alpha(0.5))
                })
                .child(label)
        };
        Some(
            div()
                .id("memory-reset-overlay")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000021))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.dismiss_memory_dialog(cx);
                }))
                .child(
                    div()
                        .id("memory-reset-dialog")
                        .relative()
                        .role(Role::Dialog)
                        .aria_label(crate::i18n::format!("要删除全部 Codex 记忆吗？" => "Delete all Codex memories?"))
                        .w(px(420.0))
                        .p(px(20.0))
                        .rounded(px(25.0))
                        .bg(surface)
                        .border_1()
                        .border_color(edge)
                        .text_color(theme.text)
                        .flex()
                        .flex_col()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(20.0))
                                        .line_height(px(28.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(crate::i18n::format!("要删除全部 Codex 记忆吗？" => "Delete all Codex memories?")),
                                )
                                .child(
                                    div()
                                        .id("memory-reset-close")
                                    // The reference pins it 16 px from the corner.
                                    .absolute()
                                    .top(px(16.0))
                                    .right(px(16.0))
                                        .role(Role::Button)
                                        .aria_label(crate::i18n::format!("关闭对话框" => "Close dialog"))
                                        .size(px(24.0))
                                        .flex_none()
                                        .rounded(px(6.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(move |button| button.bg(theme.sidebar_hover))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.dismiss_memory_dialog(cx);
                                        }))
                                        .child(
                                            gpui::svg()
                                                .path("icons/close-dialog.svg")
                                                .size(px(16.0))
                                                .text_color(theme.text_tertiary),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .text_color(theme.text.alpha(0.5))
                                .child(crate::i18n::format!("此操作将删除本地的所有 Codex 记忆" => "This deletes all Codex memories for Local")),
                        )
                        .child(
                            div()
                                .mt(px(12.0))
                                .flex()
                                .justify_end()
                                .gap(px(12.0))
                                .child(
                                    button("memory-reset-cancel", crate::i18n::format!("取消" => "Cancel"), false)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.dismiss_memory_dialog(cx);
                                        })),
                                )
                                .child(
                                    button("memory-reset-confirm", crate::i18n::format!("删除" => "Delete"), true)
                                        .on_click(cx.listener(|this, _, _, cx| this.reset_memories(cx))),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(feature = "screenshot")]
impl SettingsView {
    /// Capture-only memory settings states. The feature list comes from the
    /// features fixture; the switches read the loaded configuration.
    /// Batch three: the memory card with its consolidation row, `ready` or
    /// `pending`. A read the page started is dropped.
    #[cfg(feature = "screenshot")]
    pub fn apply_memory_status_capture_fixture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.apply_memories_capture_fixture("settings-on", cx);
        self.memory_status_cycle = self.memory_status_cycle.wrapping_add(1);
        self.memory_status = Some(crate::agent::AgentMemoryStatus {
            generation: 1,
            v2_ready: state == "ready",
            consolidated_threads: if state == "ready" { 25 } else { 3 },
            required_threads: crate::agent::MEMORY_V2_REQUIRED_THREADS,
        });
        cx.notify();
    }

    pub fn apply_memories_capture_fixture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.apply_features_capture_fixture(
            if state == "settings-unavailable" {
                "empty"
            } else {
                "list"
            },
            cx,
        );
        if let Some(list) = &mut self.features.list
            && let Some(memories) = list
                .features
                .iter_mut()
                .find(|feature| feature.name == "memories")
        {
            memories.enabled = state == "settings-on";
        }
        match state {
            "delete-confirm" => self.memory_reset = MemoryReset::Confirming,
            "deleted" => self.show_toast(
                crate::components::composer::ToastKind::Success,
                crate::i18n::format!("Codex 记忆已删除" => "Codex memories deleted"),
                cx,
            ),
            "delete-failed" => self.show_toast(
                crate::components::composer::ToastKind::Danger,
                crate::i18n::format!("无法删除 Codex 记忆" => "Unable to delete Codex memories"),
                cx,
            ),
            _ => {}
        }
        cx.notify();
    }
}
