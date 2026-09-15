//! Import settings presentation. Every source, count, progress figure and
//! history entry comes from the coding agent: `externalAgentConfig/detect` for
//! what can be imported, the import request plus its progress/completed
//! notifications for a running import, and
//! `externalAgentConfig/import/readHistories` for what was imported before.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    agent::{
        AgentExternalAgentConfigErrorKind, AgentExternalAgentImportHistory,
        AgentExternalAgentItemType,
    },
    imports::{ActiveImport, ImportPhase, ImportSource, ImportSourceState},
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    /// One detected source row: the agent's own name, what the server reported
    /// for it, and the import action.
    pub(super) fn import_source_row(
        &self,
        state: &ImportSourceState,
        orange: bool,
        last: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let orange_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xd25f28ff),
            ThemeMode::Dark => gpui::rgba(0xea733aff),
        };
        let orange_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0xffffffff),
            ThemeMode::Dark => gpui::rgba(0x0d0d0dff),
        };
        let icon = match state.source {
            ImportSource::ClaudeCode => "icons/settings-import-code.svg",
            ImportSource::Cursor => "icons/settings-import-cursor-mark.svg",
        };
        let sessions = state.session_count();
        let selected = state.selected.len();
        div()
            .h(px(64.0))
            .flex_none()
            .px(px(16.0))
            .relative()
            .flex()
            .items_center()
            .justify_between()
            .when(!last, |row| {
                row.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left(px(16.0))
                        .right(px(16.0))
                        .h(px(0.5))
                        .bg(theme.border),
                )
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .flex_none()
                            .rounded(px(15.0))
                            .when(orange, |mark| mark.bg(orange_fill))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(svg().path(icon).size(px(40.0)).text_color(if orange {
                                orange_text
                            } else {
                                theme.text
                            })),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .relative()
                                    .left(px(1.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.5625))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(state.source.label()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.settings_description)
                                    .child(import_source_subtitle(state, sessions, selected)),
                            ),
                    ),
            )
            .child(self.import_action_button(state, theme, cx))
            .into_any_element()
    }

    /// The items the server offered for one source. Each row toggles exactly
    /// that item in the set the import request will carry.
    fn import_item_rows(
        &self,
        state: &ImportSourceState,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let source = state.source;
        state
            .items()
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let selected = state.selected.contains(&index);
                let label = item_type_label(item.item_type);
                div()
                    .id(("import-item", source.provider_id().len() + index))
                    .h(px(32.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_import_item(source, index, cx);
                    }))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(theme.settings_description)
                            .child(format!("{label}：{}", item.description)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(theme.text_tertiary)
                            .child(if selected { "已选择" } else { "未选择" }),
                    )
                    .into_any_element()
            })
            .collect()
    }

    /// The import action for one source. It stays in place but inert while the
    /// server has nothing selected or another import is already running.
    fn import_action_button(
        &self,
        state: &ImportSourceState,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let source = state.source;
        let enabled = !state.selected.is_empty()
            && !self.imports.active.as_ref().is_some_and(ActiveImport::busy);
        div()
            .id(("import-source", source.provider_id().len()))
            .w(px(46.0))
            .h(px(28.0))
            .flex_none()
            .px(px(8.0))
            .rounded(px(12.5))
            .bg(theme.settings_button)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .text_size(px(14.0))
            .line_height(px(18.0))
            .text_color(theme.text)
            .whitespace_nowrap()
            .when(enabled, |button| button.cursor_pointer())
            .when(!enabled, |button| button.opacity(0.5))
            .on_click(cx.listener(move |this, _, _, cx| {
                if enabled {
                    this.start_import(source, cx);
                }
            }))
            .child("导入")
            .into_any_element()
    }

    pub(super) fn import_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let sync = div()
            .relative()
            .top(px(1.0))
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .child(
                div()
                    .h(px(60.5625))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(24.0))
                    .child(self.reference_label(
                        "保持导入同步",
                        "自动同步已连接来源中的新增和更新内容",
                        theme,
                    ))
                    // No app-server method backs this preference: it is a client
                    // setting, so the switch reports the client's own value and
                    // never a value it invented for the server.
                    .child(self.reference_switch_control(false, (page.slug, 0, 0), theme, cx)),
            );
        let mut sources = div()
            .relative()
            .top(px(1.0))
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        let source_count = self.imports.sources.len();
        for (index, state) in self.imports.sources.iter().enumerate() {
            sources = sources.child(self.import_source_row(
                state,
                index % 2 == 0,
                index + 1 == source_count,
                theme,
                cx,
            ));
        }
        for state in &self.imports.sources {
            for row in self.import_item_rows(state, &theme, cx) {
                sources = sources.child(row);
            }
        }
        let history = self.import_history_panel(&theme, cx);
        let progress = self.import_progress_panel(&theme, cx);

        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(66.0))
            .pb(px(80.0))
            .font_family(".SystemUIFont")
            .child(
                div()
                    .relative()
                    .left(px(1.0))
                    .top(px(-2.0))
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(page.label),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.settings_description)
                    .child(page.intro),
            )
            .child(
                div()
                    .mt(px(41.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("开启自动同步"),
            )
            .child(div().mt(px(15.5)).child(sync))
            .child(
                div()
                    .mt(px(40.0))
                    .text_size(px(16.0))
                    .line_height(px(24.875))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("从其他 AI 应用导入"),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(theme.settings_description)
                    .child("检测到可添加到 ChatGPT 的配置"),
            )
            .child(div().mt(px(12.0)).child(sources))
            .when_some(progress, |page, progress| {
                page.child(div().mt(px(16.0)).child(progress))
            })
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("导入历史"),
            )
            .child(div().mt(px(15.5)).child(history))
            .into_any_element()
    }

    /// The running or just-finished import, with the counts the server itself
    /// reported. Clicking it dismisses a settled import; a running one is not
    /// dismissible, because its notifications are still expected.
    ///
    /// The running import, with the counts the server itself reported.
    fn import_progress_panel(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let active = self.imports.active.as_ref()?;
        let settled = !active.busy();
        let (title, detail) = match &active.phase {
            ImportPhase::Requesting => ("正在启动导入…".to_owned(), None),
            ImportPhase::Running { .. } => (
                "正在导入…".to_owned(),
                active.status.as_ref().map(|status| {
                    format!(
                        "已成功 {} 项，失败 {} 项",
                        status.successful_item_count(),
                        status.failed_item_count()
                    )
                }),
            ),
            ImportPhase::Completed {
                succeeded, failed, ..
            } => (
                format!("已导入 {succeeded} 项"),
                (*failed > 0).then(|| format!("{failed} 项未能导入")),
            ),
            ImportPhase::Failed { message } => (message.clone(), None),
        };
        Some(
            div()
                .id("import-progress")
                .when(settled, |card| {
                    card.cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.imports.dismiss_import();
                            cx.notify();
                        }))
                })
                .w_full()
                .rounded(px(20.0))
                .border_1()
                .border_color(theme.border)
                .bg(theme.settings_panel)
                .px(px(16.0))
                .py(px(12.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.5625))
                        .font_weight(gpui::FontWeight(500.0))
                        .child(title),
                )
                .when_some(detail, |card, detail| {
                    card.child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(theme.settings_description)
                            .child(detail),
                    )
                })
                .into_any_element(),
        )
    }

    /// Import history read from the server. An empty history renders the real
    /// empty state instead of a sample entry.
    fn import_history_panel(&self, theme: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let panel = div()
            .relative()
            .top(px(1.0))
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        if self.imports.histories_loading && self.imports.histories.is_none() {
            return panel
                .child(self.import_message_row("正在读取导入历史…", theme))
                .into_any_element();
        }
        if let Some(error) = &self.imports.histories_error {
            return panel
                .child(self.import_message_row(&error.user_message(), theme))
                .into_any_element();
        }
        let entries = self
            .imports
            .histories
            .as_ref()
            .map(|histories| histories.histories.as_slice())
            .unwrap_or_default();
        let Some(latest) = entries.iter().max_by_key(|entry| entry.completed_at_ms) else {
            return panel
                .child(self.import_message_row("尚无导入记录", theme))
                .into_any_element();
        };
        let open = self.import_history_open;
        let imported = latest.successes.len();
        let completed_at = format_completed_at(latest.completed_at_ms);
        let mut card = panel.child(
            div()
                .id("import-history-header")
                .h(px(64.0))
                .flex_none()
                .px(px(16.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(24.0))
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.import_history_open = !this.import_history_open;
                    cx.notify();
                }))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .size(px(40.0))
                                .flex_none()
                                .rounded(px(15.0))
                                .bg(theme.settings_button)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    svg()
                                        .path("icons/settings-import.svg")
                                        .size(px(20.0))
                                        .text_color(theme.settings_description),
                                ),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .line_height(px(18.5625))
                                        .font_weight(gpui::FontWeight(500.0))
                                        .child("导入"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .text_color(theme.settings_description)
                                        .child(completed_at)
                                        .child(format!("已导入 {imported} 项")),
                                ),
                        ),
                )
                .child(
                    svg()
                        .path(if open {
                            "icons/settings-chevron-up.svg"
                        } else {
                            "icons/settings-chevron-next.svg"
                        })
                        .size(px(20.0))
                        .text_color(theme.text_tertiary),
                ),
        );
        if open {
            let status_green = match self.mode {
                ThemeMode::Light => gpui::rgba(0x00a240ff),
                ThemeMode::Dark => gpui::rgba(0x40c977ff),
            };
            let status_red = match self.mode {
                ThemeMode::Light => gpui::rgba(0xe02e2aff),
                ThemeMode::Dark => gpui::rgba(0xff6764ff),
            };
            for (index, item_type) in history_item_types(latest).into_iter().enumerate() {
                let successes = latest
                    .successes
                    .iter()
                    .filter(|success| success.item_type == item_type)
                    .count();
                let failures = latest
                    .failures
                    .iter()
                    .filter(|failure| failure.item_type == item_type)
                    .count();
                let mut trailing = div().flex().items_center().gap(px(12.0));
                if successes > 0 {
                    trailing = trailing.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(div().size(px(8.0)).rounded_full().bg(status_green))
                            .child(format!("已导入 {successes} 项")),
                    );
                }
                if failures > 0 {
                    trailing = trailing.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(div().size(px(8.0)).rounded_full().bg(status_red))
                            .child(format!("失败 {failures} 项")),
                    );
                }
                card = card.child(
                    div()
                        .id(("import-history-row", index))
                        .h(px(40.0))
                        .flex_none()
                        .px(px(16.0))
                        .relative()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left(px(16.0))
                                .right(px(16.0))
                                .h(px(0.5))
                                .bg(theme.border),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(18.5625))
                                .child(item_type_label(item_type)),
                        )
                        .child(
                            trailing.child(
                                svg()
                                    .path("icons/settings-chevron-next.svg")
                                    .size(px(20.0))
                                    .text_color(theme.text_tertiary),
                            ),
                        ),
                );
            }
        }
        card.into_any_element()
    }

    pub(super) fn import_message_row(&self, message: &str, theme: &Theme) -> gpui::AnyElement {
        div()
            .px(px(16.0))
            .py(px(12.0))
            .text_size(px(13.0))
            .line_height(px(19.0))
            .text_color(theme.settings_description)
            .child(message.to_owned())
            .into_any_element()
    }
}

/// The subtitle of one source row, built only from what the backend reported.
fn import_source_subtitle(state: &ImportSourceState, sessions: i64, selected: usize) -> String {
    if state.loading {
        return "正在检测…".to_owned();
    }
    if let Some(error) = &state.error {
        return match error.kind {
            AgentExternalAgentConfigErrorKind::Unsupported => error.user_message(),
            AgentExternalAgentConfigErrorKind::Connection
            | AgentExternalAgentConfigErrorKind::Protocol => error.message.clone(),
        };
    }
    let items = state.items().len();
    if items == 0 {
        return "未检测到可导入的内容".to_owned();
    }
    if sessions > 0 {
        format!("检测到 {items} 项配置、{sessions} 个会话（已选 {selected} 项）")
    } else {
        format!("检测到 {items} 项配置（已选 {selected} 项）")
    }
}

/// Item types present in one history entry, in the protocol's own order.
fn history_item_types(
    history: &AgentExternalAgentImportHistory,
) -> Vec<AgentExternalAgentItemType> {
    let mut types: Vec<AgentExternalAgentItemType> = history
        .successes
        .iter()
        .map(|success| success.item_type)
        .chain(history.failures.iter().map(|failure| failure.item_type))
        .collect();
    types.sort();
    types.dedup();
    types
}

fn item_type_label(item_type: AgentExternalAgentItemType) -> String {
    match item_type {
        AgentExternalAgentItemType::AgentsMd => "指令文件",
        AgentExternalAgentItemType::Config => "设置",
        AgentExternalAgentItemType::Skills => "技能",
        AgentExternalAgentItemType::Plugins => "插件",
        AgentExternalAgentItemType::McpServerConfig => "MCP 服务器",
        AgentExternalAgentItemType::Subagents => "子代理",
        AgentExternalAgentItemType::Hooks => "钩子",
        AgentExternalAgentItemType::Commands => "命令",
        AgentExternalAgentItemType::Memory => "记忆",
        AgentExternalAgentItemType::Sessions => "会话",
    }
    .to_owned()
}

/// The server reports completion in Unix milliseconds; the page shows it in the
/// user's own timezone.
fn format_completed_at(completed_at_ms: i64) -> String {
    match chrono::DateTime::from_timestamp_millis(completed_at_ms) {
        Some(utc) => utc
            .with_timezone(&chrono::Local)
            .format("%Y年%-m月%-d日 %H:%M")
            .to_string(),
        None => completed_at_ms.to_string(),
    }
}
