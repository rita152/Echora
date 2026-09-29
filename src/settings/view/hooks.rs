//! Hooks settings: the `hooks/list` overview grouped by source, and a dialog
//! per source with trust and enable switches written to the user config.

use std::path::PathBuf;

use gpui::{
    AnyElement, Context, IntoElement, MouseButton, Role, SharedString, Transformation, div,
    prelude::*, px, radians, rgba, svg,
};

use super::{
    SettingsView,
    dynamic::{self, card, heading, outline_button, row, switch, warning_color},
};
use crate::{
    agent::{
        AgentHook, AgentHookEventName, AgentHookHandler, AgentHookSourceGroup,
        AgentHookStateChange, AgentHookTrustStatus,
    },
    configuration::ImmediateWriteOutcome,
    hooks::{HookSource, HookSourceSelection, HookWriteFailure},
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

/// Opens a hook's definition in Echora's file panel.
pub struct OpenSettingsFile(pub PathBuf);

fn event_label(event: AgentHookEventName) -> String {
    use AgentHookEventName::*;
    match event {
        PreToolUse => "PreToolUse".into(),
        PermissionRequest => "PermissionRequest".into(),
        PostToolUse => "PostToolUse".into(),
        PreCompact => "PreCompact".into(),
        PostCompact => "PostCompact".into(),
        SessionStart => "SessionStart".into(),
        SessionEnd => crate::i18n::format!("会话结束" => "SessionEnd"),
        UserPromptSubmit => "UserPromptSubmit".into(),
        SubagentStart => crate::i18n::format!("子智能体启动" => "SubagentStart"),
        SubagentStop => crate::i18n::format!("子智能体停止" => "SubagentStop"),
        Stop => "Stop".into(),
        Interrupt => crate::i18n::format!("中断" => "Interrupt"),
    }
}

fn event_description(event: AgentHookEventName) -> String {
    use AgentHookEventName::*;
    match event {
        PreToolUse => crate::i18n::format!("工具执行前" => "Before a tool executes"),
        PermissionRequest => crate::i18n::format!("当请求权限时" => "When permission is requested"),
        PostToolUse => crate::i18n::format!("工具执行后" => "After a tool executes"),
        PreCompact => {
            crate::i18n::format!("在 ChatGPT 压缩对话之前" => "Before ChatGPT compacts the conversation")
        }
        PostCompact => {
            crate::i18n::format!("在 ChatGPT 压缩对话之后" => "After ChatGPT compacts the conversation")
        }
        SessionStart => crate::i18n::format!("当新会话开始时" => "When a new session starts"),
        SessionEnd => crate::i18n::format!("会话结束时" => "When a session ends"),
        UserPromptSubmit => {
            crate::i18n::format!("当用户提交提示时" => "When the user submits a prompt")
        }
        SubagentStart => crate::i18n::format!("子智能体启动时" => "When a subagent starts"),
        SubagentStop => crate::i18n::format!("当子智能体停止时" => "When a subagent stops"),
        Stop => {
            crate::i18n::format!("在 ChatGPT 结束本轮响应之前" => "Right before ChatGPT ends its turn")
        }
        Interrupt => crate::i18n::format!("当某个轮次被中断时" => "When a turn is interrupted"),
    }
}

fn hook_title(hook: &AgentHook, index: usize) -> String {
    let index = index + 1;
    match hook.status_message.as_deref().map(str::trim) {
        Some(message) if !message.is_empty() => format!("{index} - {message}"),
        _ => crate::i18n::format!("钩子 {index}" => "Hook {index}"),
    }
}

fn group_label(group: AgentHookSourceGroup) -> String {
    match group {
        AgentHookSourceGroup::Plugin => crate::i18n::format!("插件" => "Plugin"),
        AgentHookSourceGroup::User => crate::i18n::format!("用户配置" => "User config"),
        AgentHookSourceGroup::Admin => crate::i18n::format!("管理员配置" => "Admin config"),
        AgentHookSourceGroup::Project => crate::i18n::format!("项目配置" => "Project config"),
        AgentHookSourceGroup::SessionFlags => crate::i18n::format!("会话标记" => "Session flags"),
        AgentHookSourceGroup::Unknown => crate::i18n::format!("未知来源" => "Unknown source"),
    }
}

fn source_label(selection: &HookSourceSelection) -> String {
    match selection {
        HookSourceSelection::Shared(group) => group_label(*group),
        HookSourceSelection::Project(root) => root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string()),
        // Plugin ids are `name@marketplace`; the name is the label.
        HookSourceSelection::Plugin(Some(id)) => id.split('@').next().unwrap_or(id).to_owned(),
        HookSourceSelection::Plugin(None) => crate::i18n::format!("未知插件" => "Unknown plugin"),
    }
}

fn source_icon(selection: &HookSourceSelection) -> &'static str {
    match selection {
        HookSourceSelection::Shared(AgentHookSourceGroup::User) => "icons/hooks-user-config.svg",
        HookSourceSelection::Shared(AgentHookSourceGroup::Admin) => {
            "icons/settings-data-controls.svg"
        }
        HookSourceSelection::Project(_) => "icons/hooks-project.svg",
        HookSourceSelection::Plugin(_) => "icons/plugins.svg",
        HookSourceSelection::Shared(_) => "icons/settings-hooks-settings.svg",
    }
}

/// `{count} {noun}` with the reference's English plural, one Chinese form.
pub(super) fn counted(count: usize, zh: &str, one: &str, other: &str) -> String {
    if crate::i18n::is_english() {
        format!("{count} {}", if count == 1 { one } else { other })
    } else {
        format!("{count} {zh}")
    }
}

fn hook_count(count: usize) -> String {
    counted(count, "个钩子", "hook", "hooks")
}

/// "1 issue · 3 need review", either part omitted when zero.
fn attention_summary(issues: usize, review: usize) -> String {
    let issues = (issues > 0).then(|| counted(issues, "个问题", "issue", "issues"));
    let review = (review > 0).then(|| counted(review, "项待审核", "needs review", "need review"));
    [issues, review]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The reference's warning banner surface (measured on its captures).
fn banner_color(mode: ThemeMode) -> gpui::Rgba {
    match mode {
        ThemeMode::Dark => rgba(0x1c1613ff),
        ThemeMode::Light => rgba(0xfffcfbff),
    }
}

fn chevron(size: f32, angle: f32, color: gpui::Rgba) -> gpui::Svg {
    svg()
        .path("icons/hooks-chevron-down.svg")
        .size(px(size))
        .flex_none()
        .text_color(color)
        .with_transformation(Transformation::rotate(radians(angle)))
}

impl SettingsView {
    /// The selected project's roots come first; every other known root follows.
    pub fn set_hook_roots(
        &mut self,
        selected: Vec<PathBuf>,
        all: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let cwds = crate::hooks::list_cwds(&selected, all);
        if self.hooks.capture_fixture {
            return;
        }
        if cwds != self.hooks.cwds || self.hooks.snapshot.is_none() {
            self.hooks.cwds = cwds;
            if self.selected == "hooks-settings" {
                self.refresh_hooks(false, cx);
            }
        }
    }

    /// Reads `hooks/list` for the known project roots. A manual reload shows
    /// the reference's "Refreshed hooks" toast once it succeeds.
    pub(super) fn refresh_hooks(&mut self, manual: bool, cx: &mut Context<Self>) {
        let cwds = self.hooks.cwds.clone();
        if cwds.is_empty() || self.hooks.capture_fixture {
            return;
        }
        let cycle = self.hooks.begin_refresh(cwds.clone());
        self.hooks.reloading = manual;
        let receiver = self.backend.list_hooks(cwds);
        cx.spawn(async move |this, cx| {
            let result = receiver
                .recv()
                .await
                .unwrap_or_else(|_| Err(crate::i18n::text("钩子读取连接已关闭").into()));
            let _ = this.update(cx, |this, cx| {
                let succeeded = result.is_ok();
                if this.hooks.accept(cycle, result) {
                    if manual && succeeded {
                        this.show_toast(
                            crate::components::composer::ToastKind::Success,
                            crate::i18n::format!("钩子已刷新" => "Refreshed hooks"),
                            cx,
                        );
                    }
                    this.hooks.reloading = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Trust and enable are user config writes of only the changed hooks, at
    /// the user layer's version; a conflict or failure is reported and the
    /// list re-read, never retried.
    pub(super) fn write_hook_state(
        &mut self,
        changes: Vec<AgentHookStateChange>,
        cx: &mut Context<Self>,
    ) {
        if !self.hooks.begin_write(changes.clone()) {
            return;
        }
        let edits = changes
            .iter()
            .flat_map(AgentHookStateChange::edits)
            .collect();
        let write = match self.config_editor.prepare_immediate_write(edits) {
            Ok(write) => write,
            Err(message) => {
                self.hooks.finish_write(Some(HookWriteFailure::Failed {
                    message,
                    outcome_unknown: false,
                }));
                cx.notify();
                return;
            }
        };
        let receiver = self.backend.write_config(write.clone());
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::agent::AgentConfigError {
                    kind: crate::agent::AgentConfigErrorKind::Connection,
                    message: crate::i18n::text("保存连接已关闭").into(),
                    data: None,
                    outcome_unknown: true,
                })
            });
            let _ = this.update(cx, |this, cx| {
                let failure = match this.config_editor.accept_immediate_save(&write, result) {
                    Ok(ImmediateWriteOutcome::Saved) => None,
                    Ok(ImmediateWriteOutcome::Overridden(_)) => Some(HookWriteFailure::Overridden),
                    Ok(ImmediateWriteOutcome::Differs(keys)) => Some(HookWriteFailure::Failed {
                        message: crate::i18n::format!("回读与写入不同：{}" => "Readback differs from the write: {}", keys.join("、")),
                        outcome_unknown: false,
                    }),
                    Err(error) => {
                        // A conflict or unknown outcome needs a fresh version
                        // before the next explicit change.
                        this.reload_config(cx);
                        Some(HookWriteFailure::Failed {
                            message: error.user_message(),
                            outcome_unknown: error.outcome_unknown,
                        })
                    }
                };
                this.hooks.finish_write(failure);
                this.refresh_hooks(false, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn open_hook_source(
        &mut self,
        selection: Option<HookSourceSelection>,
        cx: &mut Context<Self>,
    ) {
        if self.hooks.open != selection {
            self.hooks.write = None;
            self.hooks.expanded = None;
            self.hooks.issues_expanded = false;
        }
        self.hooks.open = selection;
        cx.notify();
    }

    pub(super) fn dismiss_hook_dialog(&mut self, cx: &mut Context<Self>) -> bool {
        if self.hooks.open.is_none() {
            return false;
        }
        self.open_hook_source(None, cx);
        true
    }

    pub(super) fn hooks_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let link_color = match self.mode {
            ThemeMode::Light => rgba(0x339cffff),
            ThemeMode::Dark => rgba(0x99ceffff),
        };
        let subtitle = div()
            .h(px(21.0))
            .flex()
            .items_center()
            .text_size(px(14.0))
            .line_height(px(21.0))
            .text_color(theme.settings_description)
            .child(crate::i18n::text(
                "通过配置和已启用的插件管理生命周期钩子。",
            ))
            .child(
                div()
                    .ml(px(8.0))
                    .text_color(link_color)
                    .child(crate::i18n::text("了解更多")),
            )
            .into_any_element();
        let disabled = self.hooks.cwds.is_empty() || self.hooks.loading;
        let reload_label = crate::i18n::format!("重新加载钩子" => "Reload hooks");
        let reload = div()
            .id("hooks-reload")
            .role(Role::Button)
            .aria_label(SharedString::from(reload_label))
            .size(px(26.0))
            .flex_none()
            .rounded(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .when(disabled, |button| button.opacity(0.5))
            .when(!disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| this.refresh_hooks(true, cx)))
            })
            .child(
                svg()
                    .path("icons/settings-hooks-refresh.svg")
                    .size(px(16.0))
                    .text_color(theme.text_tertiary),
            )
            .into_any_element();
        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            // The reference centres the column in a 4 px wider area.
            .relative()
            .left(px(2.0))
            .pt(px(66.0))
            .pb(px(80.0))
            .font_family(".SystemUIFont")
            .child(self.dynamic_page_header(
                crate::i18n::text(page.label).to_owned(),
                subtitle,
                Some(reload),
            ))
            .child(self.hooks_overview(theme, cx))
            .into_any_element()
    }

    fn hooks_notice_card(&self, label: String, description: String, theme: Theme) -> AnyElement {
        div()
            .mt(px(32.0))
            .child(card(theme).child(row(label, Some(description), None, true, theme)))
            .into_any_element()
    }

    fn hooks_overview(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let directory = &self.hooks;
        let empty = || {
            self.hooks_notice_card(
                crate::i18n::format!("未找到钩子" => "No hooks found"),
                crate::i18n::format!("已配置的钩子将显示在此处" => "Configured hooks will appear here"),
                theme,
            )
        };
        if directory.cwds.is_empty() {
            return empty();
        }
        let Some(snapshot) = &directory.snapshot else {
            return match &directory.error {
                Some(error) => self.hooks_notice_card(
                    crate::i18n::format!("无法加载钩子" => "Could not load hooks"),
                    error.clone(),
                    theme,
                ),
                None => self.hooks_notice_card(
                    crate::i18n::format!("正在加载钩子…" => "Loading hooks…"),
                    String::new(),
                    theme,
                ),
            };
        };
        let groups = crate::hooks::group_sources(&snapshot.entries);
        if groups.is_empty() {
            return empty();
        }
        let mut first = true;
        let mut section = |title: String, sources: &[HookSource], cx: &mut Context<Self>| {
            (!sources.is_empty()).then(|| {
                // The reference leaves 62 px below the page subtitle and 60 px
                // between a card and the next heading.
                let top = if std::mem::take(&mut first) {
                    36.0
                } else {
                    44.0
                };
                let mut list = card(theme);
                for (index, source) in sources.iter().enumerate() {
                    list = list.child(self.hook_source_row(
                        source,
                        index + 1 == sources.len(),
                        theme,
                        cx,
                    ));
                }
                div()
                    .mt(px(top))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(heading(title, None, theme))
                    .child(list)
            })
        };
        div()
            .children(section(
                crate::i18n::format!("来自配置" => "From Config"),
                &groups.config,
                cx,
            ))
            .children(section(
                crate::i18n::format!("来自插件" => "From Plugins"),
                &groups.plugins,
                cx,
            ))
            .children(section(
                crate::i18n::format!("来自项目配置文件" => "From Projects"),
                &groups.projects,
                cx,
            ))
            .children(section(
                crate::i18n::format!("其他来源" => "Other sources"),
                &groups.other,
                cx,
            ))
            .when_some(directory.error.clone(), |content, error| {
                content.child(
                    div()
                        .id("hooks-load-error")
                        .mt(px(12.0))
                        .role(Role::Alert)
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(dynamic::danger_color(self.mode))
                        .child(error),
                )
            })
            .into_any_element()
    }

    fn hook_source_row(
        &self,
        source: &HookSource,
        last: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selection = source.selection.clone();
        let summary = attention_summary(source.issue_count(), source.needs_review());
        let warning = warning_color(self.mode);
        let trailing = div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .when(!summary.is_empty(), |trailing| {
                trailing.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .text_size(px(13.0))
                        .line_height(px(18.5714))
                        .text_color(theme.text)
                        .whitespace_nowrap()
                        .child(
                            svg()
                                .path("icons/settings-warning.svg")
                                .size(px(14.0))
                                .text_color(warning),
                        )
                        .child(summary),
                )
            })
            .child(chevron(
                14.0,
                -std::f32::consts::FRAC_PI_2,
                theme.text_tertiary,
            ))
            .into_any_element();
        let label = div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                svg()
                    .path(source_icon(&selection))
                    .size(px(18.0))
                    .flex_none()
                    .text_color(theme.text_secondary),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(div().truncate().child(source_label(&selection)))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .text_color(theme.settings_description)
                            .child(hook_count(source.hooks.len())),
                    ),
            );
        let id = SharedString::from(format!("hook-source-{:?}", selection));
        div()
            .id(gpui::ElementId::Name(id))
            .role(Role::Button)
            .aria_label(SharedString::from(source_label(&selection)))
            .cursor_pointer()
            .hover(move |row| row.bg(theme.sidebar_hover.alpha(0.5)))
            .on_click(
                cx.listener(move |this, _, _, cx| {
                    this.open_hook_source(Some(selection.clone()), cx)
                }),
            )
            .child(row(label, None, Some(trailing), last, theme))
            .into_any_element()
    }

    /// The source dialog, mounted over the whole settings shell.
    pub(super) fn hook_source_overlay(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let source = self.hooks.open_source()?;
        let (surface, edge) = dynamic::dialog_surface(self.mode);
        let warning = warning_color(self.mode);
        let trustable = source.trustable();
        let writing = self.hooks.writing();
        let subtitle = match &source.selection {
            HookSourceSelection::Project(root) => root.display().to_string(),
            _ => crate::i18n::format!("所有项目" => "All projects"),
        };
        let header = div()
            .flex()
            .items_start()
            .justify_between()
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                svg()
                                    .path(source_icon(&source.selection))
                                    .size(px(18.0))
                                    .text_color(theme.text),
                            )
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .line_height(px(28.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(source_label(&source.selection)),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .text_color(theme.text.alpha(0.5))
                            .child(subtitle),
                    ),
            )
            .child(
                div()
                    .id("hook-dialog-close")
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
                    .on_click(cx.listener(|this, _, _, cx| this.open_hook_source(None, cx)))
                    .child(
                        svg()
                            .path("icons/close-dialog.svg")
                            .size(px(16.0))
                            .text_color(theme.text_tertiary),
                    ),
            );
        let trust_all = (!trustable.is_empty()).then(|| {
            let changes = trustable
                .iter()
                .map(|hook| AgentHookStateChange {
                    key: hook.key.clone(),
                    enabled: None,
                    trusted_hash: Some(hook.current_hash.clone()),
                })
                .collect::<Vec<_>>();
            div()
                .mt(px(16.0))
                .px(px(16.0))
                .py(px(16.0))
                .rounded(px(16.0))
                .bg(banner_color(self.mode))
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(svg().path("icons/settings-warning.svg").size(px(18.0)).flex_none().text_color(warning))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(13.0))
                        .line_height(px(21.125))
                        .child(crate::i18n::format!("Hook 在沙盒外运行，可能存在安全风险" => "Hooks run outside of the sandbox and may be unsafe")),
                )
                .child(
                    outline_button("hooks-trust-all", crate::i18n::format!("全部信任" => "Trust all"), None, theme)
                        .when(writing, |button| button.opacity(0.5))
                        .when(!writing, |button| {
                            button.on_click(cx.listener(move |this, _, _, cx| {
                                this.write_hook_state(changes.clone(), cx)
                            }))
                        }),
                )
        });
        let failure = self
            .hooks
            .write
            .as_ref()
            .and_then(|write| write.failure.clone())
            .map(|failure| {
                let text = match failure {
                    HookWriteFailure::Overridden => crate::i18n::format!(
                        "另一个配置层覆盖了这些 Hook 设置，必须先修改该层配置，批准才能生效" =>
                        "Another config layer overrides these hook settings and must be changed before approval can take effect"
                    ),
                    HookWriteFailure::Failed { message, .. } => crate::i18n::format!(
                        "无法将 Hook 设为可信：{message}" => "Could not trust hooks: {message}"
                    ),
                };
                div()
                    .id("hooks-write-error")
                    .mt(px(12.0))
                    .role(Role::Alert)
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(dynamic::danger_color(self.mode))
                    .child(text)
            });
        let issues = (source.issue_count() > 0).then(|| self.hook_issues(&source, theme, cx));
        let mut events = card(theme);
        let event_groups = source.events();
        for (position, (event, hooks)) in event_groups.iter().enumerate() {
            let review = hooks.iter().filter(|hook| hook.needs_review()).count();
            if position > 0 {
                // Event blocks are divided like settings rows: inset 16 px.
                events = events.child(div().mx(px(16.0)).h(px(1.0)).bg(theme.border));
            }
            events = events.child(
                div().border_b_1().border_color(theme.border).child(row(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            svg()
                                .path("icons/hooks-event.svg")
                                .size(px(16.0))
                                .flex_none()
                                .text_color(theme.text),
                        )
                        .child(
                            div().flex().flex_col().child(event_label(*event)).child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(theme.settings_description)
                                    .child(event_description(*event)),
                            ),
                        ),
                    None,
                    (review > 0).then(|| {
                        svg()
                            .path("icons/settings-warning.svg")
                            .size(px(14.0))
                            .text_color(warning)
                            .into_any_element()
                    }),
                    true,
                    theme,
                )),
            );
            let mut list = div().px(px(12.0)).flex().flex_col();
            for (index, hook) in hooks.iter().enumerate() {
                list = list.child(self.hook_row(hook, index, index + 1 == hooks.len(), theme, cx));
            }
            events = events.child(list);
        }
        Some(
            div()
                .id("hook-dialog-overlay")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000021))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| this.open_hook_source(None, cx)))
                .child(
                    div()
                        .id("hook-dialog")
                        .relative()
                        .role(Role::Dialog)
                        .aria_label(SharedString::from(source_label(&source.selection)))
                        .w(px(680.0))
                        .max_w_full()
                        .max_h(px(680.0))
                        .p(px(20.0))
                        .rounded(px(25.0))
                        .bg(surface)
                        .border_1()
                        .border_color(edge)
                        .text_color(theme.text)
                        .flex()
                        .flex_col()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(header)
                        .children(trust_all)
                        .children(failure)
                        .child(
                            div()
                                .id("hook-dialog-scroll")
                                .mt(px(12.0))
                                .min_h(px(0.0))
                                .flex_1()
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .children(issues)
                                .when(!event_groups.is_empty(), |content| content.child(events)),
                        ),
                )
                .into_any_element(),
        )
    }

    fn hook_issues(&self, source: &HookSource, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let warning = warning_color(self.mode);
        let expanded = self.hooks.issues_expanded;
        let count = source.issue_count();
        let details =
            source
                .warnings
                .iter()
                .cloned()
                .chain(source.errors.iter().map(
                    |error| crate::i18n::format!("{}：{}" => "{}: {}", error.path, error.message),
                ))
                .collect::<Vec<_>>();
        div()
            .rounded(px(12.5))
            .border_1()
            .border_color(gpui::Rgba { a: 0.3, ..warning })
            .overflow_hidden()
            .child(
                div()
                    .id("hook-issues-toggle")
                    .role(Role::Button)
                    .aria_expanded(expanded)
                    .px(px(12.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.hooks.issues_expanded = !this.hooks.issues_expanded;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                svg()
                                    .path("icons/settings-warning.svg")
                                    .size(px(14.0))
                                    .text_color(warning),
                            )
                            .child(div().text_size(px(13.0)).line_height(px(18.5714)).child(
                                if crate::i18n::is_english() {
                                    format!(
                                        "{} loading hooks for this source",
                                        counted(count, "", "issue", "issues")
                                    )
                                } else {
                                    format!("此源有 {count} 个钩子加载问题")
                                },
                            )),
                    )
                    .child(chevron(
                        14.0,
                        if expanded { std::f32::consts::PI } else { 0.0 },
                        theme.text,
                    )),
            )
            .when(expanded, |issues| {
                issues.child(
                    div()
                        .border_t_1()
                        .border_color(gpui::Rgba { a: 0.2, ..warning })
                        .px(px(12.0))
                        .py(px(8.0))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(theme.text_secondary)
                        .children(details),
                )
            })
            .into_any_element()
    }

    fn hook_row(
        &self,
        hook: &AgentHook,
        index: usize,
        last: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.hooks.expanded.as_deref() == Some(hook.key.as_str());
        let review = hook.needs_review();
        let writing = self.hooks.writing();
        let key = hook.key.clone();
        let title = hook_title(hook, index);
        let open_file = (!hook.is_managed)
            .then(|| crate::hooks::local_source_file(hook).map(std::path::Path::to_path_buf));
        let open_button = open_file.map(|path| {
            let missing = path.is_none();
            div()
                .id(SharedString::from(format!("hook-open-{key}")))
                .role(Role::Button)
                .aria_label(crate::i18n::format!("打开配置文件" => "Open config file"))
                .size(px(20.0))
                .flex_none()
                .rounded(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(move |button| button.bg(theme.sidebar_hover))
                .on_click(cx.listener({
                    let source = hook.source_path.clone();
                    move |this, _, _, cx| match &path {
                        Some(path) => cx.emit(OpenSettingsFile(path.clone())),
                        // The definition lives on another machine or was
                        // removed; say so instead of opening nothing.
                        None => this.show_toast(
                            crate::components::composer::ToastKind::Danger,
                            crate::i18n::format!("此电脑上找不到 {}" => "{} is not on this computer", source.display()),
                            cx,
                        ),
                    }
                }))
                .child(
                    svg()
                        .path("icons/hooks-open-config.svg")
                        .size(px(12.0))
                        .text_color(if missing { theme.text_tertiary.alpha(0.5) } else { theme.text_tertiary }),
                )
        });
        let trust = review.then(|| {
            let change = AgentHookStateChange {
                key: key.clone(),
                enabled: None,
                trusted_hash: Some(hook.current_hash.clone()),
            };
            outline_button(
                SharedString::from(format!("hook-trust-{key}")),
                crate::i18n::format!("信任" => "Trust"),
                Some("hooks-trust"),
                theme,
            )
            .tooltip({
                let text = if hook.trust_status == AgentHookTrustStatus::Modified {
                    crate::i18n::format!("钩子自上次标记为可信后已更改" => "Hook changed since last trusted")
                } else {
                    crate::i18n::format!("新钩子" => "New hook")
                };
                move |_, cx| cx.new(|_| crate::components::composer::TrayTooltip(text.clone().into())).into()
            })
            .when(writing, |button| button.opacity(0.5))
            .when(!writing, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| {
                    this.write_hook_state(vec![change.clone()], cx)
                }))
            })
        });
        let checked = hook.is_managed || (self.hooks.displayed_enabled(hook) && !review);
        let locked = hook.is_managed || review;
        let toggle = switch(SharedString::from(format!("hook-enabled-{key}")), checked, locked || writing, theme)
            .aria_label(SharedString::from(title.clone()))
            .when(locked, |toggle| {
                let text = if hook.is_managed {
                    crate::i18n::format!("受管理的钩子始终处于开启状态" => "Managed hooks are always on")
                } else {
                    crate::i18n::format!("钩子在标记为可信之前保持禁用" => "Disabled until hook is trusted")
                };
                toggle.tooltip(move |_, cx| cx.new(|_| crate::components::composer::TrayTooltip(text.clone().into())).into())
            })
            .when(!locked && !writing, |toggle| {
                let key = key.clone();
                toggle.on_click(cx.listener(move |this, _, _, cx| {
                    this.write_hook_state(
                        vec![AgentHookStateChange {
                            key: key.clone(),
                            enabled: Some(!checked),
                            trusted_hash: None,
                        }],
                        cx,
                    )
                }))
            });
        div()
            .when(!last, |hook_row| {
                hook_row.border_b_1().border_color(theme.border)
            })
            .when(expanded, |hook_row| hook_row.pb(px(8.0)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("hook-row-{key}")))
                            .role(Role::Button)
                            .aria_expanded(expanded)
                            .min_w(px(0.0))
                            .flex_1()
                            .py(px(8.0))
                            .pl(px(28.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .cursor_pointer()
                            .on_click(cx.listener({
                                let key = key.clone();
                                move |this, _, _, cx| {
                                    this.hooks.expanded = (!expanded).then(|| key.clone());
                                    cx.notify();
                                }
                            }))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .child(title),
                            )
                            .children(open_button)
                            .child(chevron(
                                14.0,
                                if expanded { std::f32::consts::PI } else { 0.0 },
                                theme.text_secondary,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .children(trust)
                            .child(toggle),
                    ),
            )
            .when(expanded, |hook_row| {
                hook_row.child(self.hook_details(hook, theme))
            })
            .into_any_element()
    }

    fn hook_details(&self, hook: &AgentHook, theme: Theme) -> AnyElement {
        let term = |label: String, value: AnyElement| {
            div()
                .flex()
                .gap(px(16.0))
                .child(
                    div()
                        .w(px(96.0))
                        .flex_none()
                        .text_color(theme.settings_description)
                        .child(label),
                )
                .child(div().min_w(px(0.0)).flex_1().child(value))
        };
        let code = |text: String| {
            div()
                .font_family(crate::theme::UI_MONOSPACE_FONT_FAMILY)
                .text_size(px(12.0))
                .child(text)
                .into_any_element()
        };
        let mut grid = div()
            .ml(px(28.0))
            .mt(px(8.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.border)
            .px(px(12.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .child(term(
                crate::i18n::format!("Hook 类型" => "Hook type"),
                div().child(event_label(hook.event_name)).into_any_element(),
            ));
        match &hook.handler {
            AgentHookHandler::Command { command, .. } => {
                grid = grid.child(term(
                    crate::i18n::format!("命令" => "Command"),
                    code(command.clone()),
                ));
            }
            AgentHookHandler::McpTool { server, tool } => {
                grid = grid
                    .child(term(
                        crate::i18n::format!("MCP 服务器" => "MCP server"),
                        code(server.clone()),
                    ))
                    .child(term(
                        crate::i18n::format!("工具" => "Tool"),
                        code(tool.clone()),
                    ));
            }
            AgentHookHandler::Prompt => {
                grid = grid.child(term(
                    crate::i18n::format!("处理程序" => "Handler"),
                    div()
                        .child(crate::i18n::format!("提示" => "Prompt"))
                        .into_any_element(),
                ));
            }
            AgentHookHandler::Agent => {
                grid = grid.child(term(
                    crate::i18n::format!("处理程序" => "Handler"),
                    div()
                        .child(crate::i18n::format!("智能体" => "Agent"))
                        .into_any_element(),
                ));
            }
        }
        if let Some(matcher) = &hook.matcher {
            grid = grid.child(term(
                crate::i18n::format!("匹配器" => "Matcher"),
                code(matcher.clone()),
            ));
        }
        grid.child(term(
            crate::i18n::format!("超时" => "Timeout"),
            div()
                .child(format!("{}s", hook.timeout_sec))
                .into_any_element(),
        ))
        .into_any_element()
    }
}

#[cfg(feature = "screenshot")]
impl SettingsView {
    /// Capture-only: the reference fixture (three user hooks, one project hook
    /// and a load warning) in a named state, with backend reads suppressed.
    pub fn apply_hooks_capture_fixture(&mut self, state: &str, cx: &mut Context<Self>) {
        use crate::agent::{AgentHookListEntry, AgentHookSource, AgentHooksSnapshot};
        let config = PathBuf::from("/Users/me/.codex/config.toml");
        let project = PathBuf::from("/Users/me/fixture-project");
        let hook = |key: &str, event, handler, source, order, trust| AgentHook {
            key: format!("{}:{key}", config.display()),
            event_name: event,
            handler,
            matcher: None,
            timeout_sec: 600,
            status_message: None,
            source,
            source_path: config.clone(),
            plugin_id: None,
            display_order: order,
            enabled: true,
            is_managed: false,
            current_hash: format!("sha256:{key}"),
            trust_status: trust,
            additional_context_limit: None,
        };
        let command = |text: &str, is_async| AgentHookHandler::Command {
            command: text.into(),
            is_async,
        };
        let untrusted = state != "trusted";
        let mut pre = hook(
            "pre_tool_use:0:0",
            AgentHookEventName::PreToolUse,
            command("echo trusted-pre-tool", false),
            AgentHookSource::User,
            0,
            if untrusted {
                AgentHookTrustStatus::Untrusted
            } else {
                AgentHookTrustStatus::Trusted
            },
        );
        pre.matcher = Some("Bash".into());
        pre.timeout_sec = 30;
        pre.status_message = Some("Checking the command".into());
        let post = hook(
            "post_tool_use:0:0",
            AgentHookEventName::PostToolUse,
            AgentHookHandler::McpTool {
                server: "audit".into(),
                tool: "record".into(),
            },
            AgentHookSource::User,
            1,
            AgentHookTrustStatus::Untrusted,
        );
        let stop = hook(
            "stop:0:0",
            AgentHookEventName::Stop,
            command("echo modified-stop", true),
            AgentHookSource::User,
            2,
            if untrusted {
                AgentHookTrustStatus::Untrusted
            } else {
                AgentHookTrustStatus::Modified
            },
        );
        let mut session = hook(
            "session_start:0:0",
            AgentHookEventName::SessionStart,
            command("echo project-session-start", false),
            AgentHookSource::Project,
            3,
            AgentHookTrustStatus::Untrusted,
        );
        session.key = format!("{}/.codex/config.toml:session_start:0:0", project.display());
        session.source_path = project.join(".codex/config.toml");
        let warning = format!(
            "invalid matcher \"(unclosed\" in {}: regex parse error:\n    (unclosed\n    ^\nerror: unclosed group",
            config.display()
        );
        let entries = match state {
            "empty" => Vec::new(),
            _ => vec![AgentHookListEntry {
                cwd: project.clone(),
                hooks: vec![pre, post, stop, session],
                warnings: vec![warning],
                errors: Vec::new(),
            }],
        };
        self.hooks = crate::hooks::HooksDirectory {
            cwds: vec![project.clone()],
            snapshot: (state != "loading" && state != "error").then(|| AgentHooksSnapshot {
                generation: 1,
                cwds: vec![project],
                entries,
            }),
            loading: state == "loading",
            error: (state == "error").then(|| "hooks/list failed: connection closed".to_owned()),
            capture_fixture: true,
            ..Default::default()
        };
        let user = HookSourceSelection::Shared(AgentHookSourceGroup::User);
        match state {
            "dialog" | "trusted" => self.hooks.open = Some(user),
            "expanded" => {
                self.hooks.open = Some(user);
                self.hooks.expanded = Some("/Users/me/.codex/config.toml:pre_tool_use:0:0".into());
            }
            "issues" => {
                self.hooks.open = Some(user);
                self.hooks.issues_expanded = true;
            }
            "overridden" => {
                self.hooks.open = Some(user);
                self.hooks.write = Some(crate::hooks::HookWrite {
                    changes: Vec::new(),
                    failure: Some(HookWriteFailure::Overridden),
                    in_flight: false,
                });
            }
            "refreshed" => self.show_toast(
                crate::components::composer::ToastKind::Success,
                crate::i18n::format!("钩子已刷新" => "Refreshed hooks"),
                cx,
            ),
            _ => {}
        }
        cx.notify();
    }
}
