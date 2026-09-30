//! Agent settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px};

use super::SettingsView;
use crate::{settings::PageSpec, theme::Theme};

/// The reference keeps the newest notices only.
const CONFIG_WARNING_LIMIT: usize = 20;
/// CDP 2026-09-30, Configuration → Agent defaults: the warning banner is
/// `rounded-[25px]` with `16px 12px 16px 20px` padding, an 18px icon 12px
/// before a 706px text column, and the actions 32px after it.
const CONFIG_WARNING_RADIUS: f32 = 25.0;
const CONFIG_WARNING_ICON_SIZE: f32 = 18.0;
const CONFIG_WARNING_ICON_GAP: f32 = 12.0;
const CONFIG_WARNING_ACTION_GAP: f32 = 32.0;
const CONFIG_WARNING_BUTTON_HEIGHT: f32 = 24.0;

impl SettingsView {
    /// A repeated warning moves to the end instead of being listed twice.
    pub(super) fn apply_config_warning(
        &mut self,
        warning: crate::agent::AgentConfigWarning,
        cx: &mut Context<Self>,
    ) {
        self.config_warnings.retain(|existing| existing != &warning);
        self.config_warnings.push(warning);
        let overflow = self
            .config_warnings
            .len()
            .saturating_sub(CONFIG_WARNING_LIMIT);
        self.config_warnings.drain(..overflow);
        cx.notify();
    }
    /// The reference's warning banner: `bg-surface` under a 30%
    /// `background-warning-surface` wash (resolved here to the painted
    /// color), a 0.5px ring and two soft shadows. The summary and details are
    /// small Markdown, so a backticked key renders as inline code.
    fn config_warning_card(
        &self,
        warning: &crate::agent::AgentConfigWarning,
        index: usize,
        theme: Theme,
    ) -> gpui::AnyElement {
        let dark = self.mode == crate::theme::ThemeMode::Dark;
        let (surface, ring) = if dark {
            (gpui::rgba(0x1c1613ff), gpui::rgba(0xffffff28))
        } else {
            (gpui::rgba(0xfefcfbff), gpui::rgba(0x1a1c1f1e))
        };
        let details = warning
            .details
            .as_deref()
            .filter(|details| !details.trim().is_empty());
        let file = warning.path.as_deref().map(|path| {
            let location = match (warning.line, warning.column) {
                (Some(line), Some(column)) => {
                    crate::i18n::format!("（第 {line} 行，第 {column} 列）" => " (line {line}, column {column})")
                }
                _ => String::new(),
            };
            crate::i18n::format!("文件：`{path}`{location}" => "File: `{path}`{location}")
        });
        let mut label = format!(
            "{}：{}",
            crate::i18n::text("Codex 配置警告"),
            warning.summary
        );
        for part in details.iter().copied().chain(file.as_deref()) {
            label.push_str(&format!("；{}", part.replace('`', "")));
        }
        let markdown = |part: &str, slot: &str| {
            crate::components::markdown::render_notice_markdown(
                part,
                theme,
                &format!("config-warning-{index}-{slot}"),
                theme.text,
            )
        };
        let text = div()
            .min_w(px(0.))
            .flex_1()
            .flex()
            .flex_col()
            .child(markdown(&warning.summary, "summary"))
            .when_some(details, |text, details| {
                text.child(markdown(details, "details"))
            })
            .when_some(file.as_deref(), |text, file| {
                text.child(markdown(file, "file"))
            });
        let open = warning.path.clone().map(|path| {
            let path = std::path::PathBuf::from(path);
            div()
                .id(("config-warning-open", index))
                .role(gpui::Role::Button)
                .aria_label(crate::i18n::format!("打开配置文件 {}" => "Open configuration file {}", path.display()))
                .h(px(CONFIG_WARNING_BUTTON_HEIGHT))
                .px(px(8.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(9999.))
                .border(px(1.))
                .border_color(theme.border)
                .bg(theme.command_surface)
                .text_size(px(13.))
                .line_height(px(18.))
                .text_color(theme.text)
                .cursor_pointer()
                .hover(|button| button.bg(theme.sidebar_hover))
                .on_click(move |_, _, cx| cx.open_with_system(&path))
                .child(crate::i18n::text("打开文件"))
        });
        div()
            .id(("config-warning", index))
            .role(gpui::Role::Alert)
            .aria_label(label)
            .w_full()
            .pt(px(16.))
            .pb(px(16.))
            .pl(px(20.))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(CONFIG_WARNING_ICON_GAP))
            .rounded(px(CONFIG_WARNING_RADIUS))
            .bg(surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(0.), ring.into()).spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.), px(0.), gpui::rgba(0x0000000d).into())
                    .blur_radius(px(2.)),
                gpui::BoxShadow::new(px(0.), px(4.), gpui::rgba(0x00000005).into())
                    .blur_radius(px(6.)),
            ])
            .child(
                // `pt-0.5` over the 18px glyph: a 20px column, centered.
                div().pt(px(2.)).flex_none().child(
                    crate::components::icons::icon("settings-warning", theme.warning.into())
                        .size(px(CONFIG_WARNING_ICON_SIZE)),
                ),
            )
            .child(
                div()
                    .min_w(px(0.))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(CONFIG_WARNING_ACTION_GAP))
                    .child(text)
                    .children(open),
            )
            .into_any_element()
    }
    pub(super) fn agent_row(
        &self,
        title: &'static str,
        subtitle: impl Into<gpui::SharedString>,
        right: gpui::AnyElement,
        last: bool,
        theme: Theme,
    ) -> gpui::AnyElement {
        let subtitle: gpui::SharedString = subtitle.into();
        let field_key = match title {
            "批准策略" => "approval_policy",
            "沙盒设置" => "sandbox_mode",
            "网页搜索" => "web_search",
            "输出详细程度" => "model_verbosity",
            "推理摘要" => "model_reasoning_summary",
            "批准方式" => "approvals_reviewer",
            "默认权限配置" => "default_permissions",
            "默认模型" => "model",
            "默认推理强度" => "model_reasoning_effort",
            "Plan 推理强度" => "plan_mode_reasoning_effort",
            "服务等级" => "service_tier",
            "个性默认值" => "personality",
            _ => "",
        };
        let field_error = if self.config_editor.edits.len() == 1
            && self.config_editor.edits.contains_key(field_key)
        {
            if let crate::configuration::ConfigOperation::Failed(error) =
                &self.config_editor.operation
            {
                Some(error.user_message())
            } else {
                None
            }
        } else {
            None
        };
        let label = div()
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
                    .text_color(theme.markdown_text)
                    .child(crate::i18n::text(title)),
            )
            .when(!subtitle.is_empty(), |column| {
                column.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(theme.settings_description)
                        .child(crate::i18n::text(&subtitle).to_owned()),
                )
            });
        let label = label.when_some(field_error, |column, error| {
            column.child(
                div()
                    .id(format!("config-field-error-{field_key}"))
                    .role(gpui::Role::Alert)
                    .aria_label(error.clone())
                    .mt(px(2.))
                    .text_size(px(13.))
                    .line_height(px(18.5625))
                    .text_color(if self.mode == crate::theme::ThemeMode::Dark {
                        gpui::rgba(0xff6764ff)
                    } else {
                        gpui::rgba(0xd62f2aff)
                    })
                    .child(error),
            )
        });
        div()
            .min_h(px(60.5625))
            .py(px(12.))
            .flex_none()
            .px(px(16.0))
            .relative()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(24.0))
            .when(!last, |row| {
                row.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left(px(16.0))
                        .right(px(16.0))
                        .h(px(0.5))
                        .bg(if theme.surface == gpui::rgba(0x181818ff) {
                            gpui::rgba(0x353535ff)
                        } else {
                            gpui::rgba(0xe9e9e9ff)
                        }),
                )
            })
            .child(label)
            .child(right)
            .into_any_element()
    }
    pub(super) fn agent_card(&self, rows: Vec<gpui::AnyElement>, theme: Theme) -> gpui::AnyElement {
        let mut card = div()
            .w_full()
            .rounded(px(20.0))
            .border_1()
            .border_color(if theme.surface == gpui::rgba(0x181818ff) {
                gpui::rgba(0x353535ff)
            } else {
                gpui::rgba(0xe9e9e9ff)
            })
            .bg(if self.mode == crate::theme::ThemeMode::Dark {
                gpui::rgba(0x232323ff)
            } else {
                theme.settings_panel
            });
        for row in rows {
            card = card.child(row);
        }
        card.into_any_element()
    }
    pub(super) fn agent_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use super::configuration::ConfigAction;
        use crate::configuration::ConfigOperation;
        let busy = self.config_editor.busy();
        let rows = [
            (
                "approval_policy",
                crate::i18n::text("批准策略"),
                crate::i18n::text("选择 ChatGPT 何时请求批准"),
            ),
            (
                "sandbox_mode",
                crate::i18n::text("沙盒设置"),
                crate::i18n::text("选择 ChatGPT 运行命令时的权限范围"),
            ),
            (
                "web_search",
                crate::i18n::text("网页搜索"),
                crate::i18n::text("选择 ChatGPT 访问网络的方式"),
            ),
            (
                "model_verbosity",
                crate::i18n::text("输出详细程度"),
                crate::i18n::text("选择 ChatGPT 回复包含细节的详细程度"),
            ),
            (
                "model_reasoning_summary",
                crate::i18n::text("推理摘要"),
                crate::i18n::text("选择 ChatGPT 总结其推理的方式"),
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (key, title, description))| {
            // A provider without web search says so under the row.
            let gated = (key == "web_search")
                .then(|| self.provider_unsupported_reason(key, &serde_json::json!("live")))
                .flatten();
            let description: gpui::SharedString =
                gated.map_or_else(|| description.into(), Into::into);
            self.agent_row(
                title,
                description,
                self.config_control(key, theme, cx),
                index == 4,
                theme,
            )
        })
        .collect();
        // Section content stacks with a 6px gap: the warnings sit 15.5px
        // under the header, and the source row keeps its own 6px top, so it
        // is 12px under the last warning and 21.5px under a bare header.
        let warnings = (!self.config_warnings.is_empty()).then(|| {
            div().mt(px(15.5)).flex().flex_col().gap(px(6.)).children(
                self.config_warnings
                    .iter()
                    .enumerate()
                    .map(|(index, warning)| self.config_warning_card(warning, index, theme)),
            )
        });
        let source_gap = if warnings.is_some() { 12. } else { 21.5 };
        let mut content = div()
            .w_full()
            .max_w(px(768.))
            .mx_auto()
            .pt(px(66.))
            .pb(px(80.))
            .child(
                div()
                    .text_size(px(24.))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(crate::i18n::text(page.label)),
            )
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .text_color(theme.settings_description)
                    .child(crate::i18n::text("配置新聊天的权限、网页访问和智能体回复")),
            )
            .child(
                div()
                    .mt(px(41.5))
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .font_weight(gpui::FontWeight(500.))
                    .child(crate::i18n::text("智能体默认设置")),
            )
            .children(warnings)
            .child(
                div()
                    .mt(px(source_gap))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.config_control("source", theme, cx))
                    .child(self.config_button(
                        "config-reload",
                        if self.config_editor.operation == ConfigOperation::Loading {
                            crate::i18n::text("读取中…")
                        } else {
                            crate::i18n::text("重新读取")
                        },
                        ConfigAction::Reload,
                        busy,
                        theme,
                        cx,
                    )),
            )
            .child(div().mt(px(12.)).child(self.agent_card(rows, theme)));
        if let ConfigOperation::Failed(error) | ConfigOperation::ReadFailed(error) =
            &self.config_editor.operation
            && (matches!(self.config_editor.operation, ConfigOperation::ReadFailed(_))
                || self.config_editor.edits.len() != 1)
        {
            let text = error.user_message();
            content = content.child(
                div()
                    .id("config-error")
                    .role(gpui::Role::Alert)
                    .aria_label(text.clone())
                    .mt(px(12.))
                    .text_size(px(13.))
                    .text_color(theme.warning)
                    .child(text),
            );
        }
        if let Some(feedback) = &self.config_editor.feedback {
            content = content.child(
                div()
                    .id("config-feedback")
                    .role(gpui::Role::Status)
                    .aria_label(feedback.clone())
                    .mt(px(12.))
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .child(feedback.clone()),
            );
        }
        if !self.config_editor.edits.is_empty()
            || matches!(self.config_editor.operation, ConfigOperation::Saving)
        {
            content = content.child(
                div()
                    .mt(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(theme.text_tertiary)
                            .child(crate::i18n::format!("{} 项未保存的修改" => "{} unsaved changes", self.config_editor.edits.len())),
                    )
                    .when(self.config_editor.needs_review, |row| {
                        row.child(self.config_button(
                            "config-review",
                            crate::i18n::text("确认已核对草稿"),
                            ConfigAction::Review,
                            busy || self.config_editor.operation != ConfigOperation::Ready,
                            theme,
                            cx,
                        ))
                    })
                    .child(self.config_button(
                        "config-discard",
                        crate::i18n::text("放弃修改"),
                        ConfigAction::Discard,
                        busy,
                        theme,
                        cx,
                    ))
                    .child(self.config_button(
                        "config-save",
                        if matches!(self.config_editor.operation, ConfigOperation::Saving) {
                            crate::i18n::text("保存中…")
                        } else {
                            crate::i18n::text("保存")
                        },
                        ConfigAction::Save,
                        busy || self.config_editor.needs_review
                            || matches!(
                                self.config_editor.operation,
                                ConfigOperation::ReadFailed(_)
                            ),
                        theme,
                        cx,
                    )),
            );
        }
        content = content.child(self.experimental_features_section(theme, cx));
        content = content.child(
            div()
                .mt(px(24.))
                .flex()
                .gap(px(8.))
                .child(self.config_button(
                    "config-advanced",
                    crate::i18n::text("权限与会话默认值"),
                    ConfigAction::Advanced,
                    false,
                    theme,
                    cx,
                ))
                .child(self.config_button(
                    "config-sources",
                    crate::i18n::text("配置来源与受管限制"),
                    ConfigAction::Sources,
                    false,
                    theme,
                    cx,
                )),
        );
        if self.config_advanced_open {
            let fields = [
                (
                    "approvals_reviewer",
                    crate::i18n::text("批准方式"),
                    crate::i18n::text("用户批准或服务端自动复核"),
                ),
                (
                    "default_permissions",
                    crate::i18n::text("默认权限配置"),
                    crate::i18n::text("仅可使用服务器允许的配置；新聊天继承此设置"),
                ),
                (
                    "model",
                    crate::i18n::text("默认模型"),
                    crate::i18n::text("仅用于新会话；已有会话可在模型菜单中修改"),
                ),
                (
                    "model_reasoning_effort",
                    crate::i18n::text("默认推理强度"),
                    crate::i18n::text("使用模型目录提供的强度"),
                ),
                (
                    "plan_mode_reasoning_effort",
                    crate::i18n::text("Plan 推理强度"),
                    crate::i18n::text("仅用于新会话的 Plan 默认值"),
                ),
                (
                    "service_tier",
                    crate::i18n::text("服务等级"),
                    crate::i18n::text("仅用于新会话；合法值由模型目录提供"),
                ),
                (
                    "personality",
                    crate::i18n::text("个性默认值"),
                    crate::i18n::text("仅用于新会话"),
                ),
            ];
            let rows = fields
                .into_iter()
                .enumerate()
                .map(|(index, (key, title, description))| {
                    self.agent_row(
                        title,
                        description,
                        self.config_control(key, theme, cx),
                        index == 6,
                        theme,
                    )
                })
                .collect();
            content = content.child(div().mt(px(12.)).child(self.agent_card(rows, theme)));
            if let Some(error) = &self.config_profiles_error {
                content = content.child(
                    div()
                        .mt(px(8.))
                        .text_size(px(13.))
                        .text_color(theme.warning)
                        .child(crate::i18n::format!("权限配置列表不可用：{error}" => "Permission profiles unavailable: {error}")),
                );
            }
        }
        if let Some(key) = &self.config_custom_key {
            content = content.child(
                div()
                    .mt(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().text_size(px(13.)).child(key.clone()))
                    .child(div().flex_1().child(self.config_input.clone()))
                    .child(self.config_button(
                        "config-apply-custom",
                        crate::i18n::text("应用到草稿"),
                        ConfigAction::ApplyCustom,
                        busy,
                        theme,
                        cx,
                    )),
            );
        }
        if self.config_sources_open {
            content = content.child(
                div()
                    .mt(px(16.))
                    .child(self.config_button(
                        "config-copy",
                        crate::i18n::text("复制配置来源与诊断"),
                        ConfigAction::Copy,
                        false,
                        theme,
                        cx,
                    ))
                    .child(
                        div()
                            .id("config-source-details")
                            .mt(px(12.))
                            .text_size(px(12.))
                            .line_height(px(18.))
                            .text_color(theme.text_tertiary)
                            .child(crate::components::markdown::render_selectable_plan(
                                &self.config_diagnostics(),
                                theme,
                                "config-source-details-text",
                            )),
                    ),
            );
        }
        content.child(div().mt(px(24.)).text_size(px(12.)).line_height(px(18.)).text_color(theme.settings_description)
            .child(crate::i18n::format!("工作目录：{}" => "Working directory: {}",self.config_cwd.display())))
            .child(div().mt(px(6.)).text_size(px(12.)).line_height(px(18.)).text_color(theme.settings_description)
                .child(crate::i18n::text("保存后会回读有效配置。配置默认值与当前线程权限分别管理；线程权限变更从下一轮起生效，复核者会同步到进行中的轮次。")))
            .into_any_element()
    }
}
