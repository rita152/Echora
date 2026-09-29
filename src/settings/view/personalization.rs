//! Personalization settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn personalization_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let instruction = &page.sections[0].rows[0];
        let personality = &page.sections[2].rows[0];
        let link_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x339cffff),
            ThemeMode::Dark => gpui::rgba(0x99ceffff),
        };
        let textarea_border = match self.mode {
            ThemeMode::Light => gpui::rgba(0x1a1c1f1f),
            ThemeMode::Dark => gpui::rgba(0xffffff1f),
        };
        let warning_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xfffcfbff),
            ThemeMode::Dark => gpui::rgba(0x1c1613ff),
        };

        let memory_card = self.memory_card(theme, cx);

        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(66.0))
            .pb(px(80.0))
            .child(
                div()
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(crate::i18n::text(page.label)),
            )
            .child(
                div()
                    .mt(px(32.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(crate::i18n::format!("Codex 记忆" => "Codex memory")),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(theme.settings_description)
                            .child(crate::i18n::format!(
                                "配置 Codex 在本地上管理记忆的方式。" =>
                                "Configure how Codex manages memory for Local."
                            ))
                            .child(
                                div()
                                    .text_color(link_color)
                                    .child(crate::i18n::text("了解更多")),
                            ),
                    ),
            )
            .child(div().mt(px(12.0)).child(memory_card))
            .child(
                div()
                    .mt(px(39.0))
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .line_height(px(24.875))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(crate::i18n::text(instruction.title)),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .text_color(theme.settings_description)
                                    .child(crate::i18n::text(
                                        "向 ChatGPT 提供适用于此主机上所有聊天的额外说明和上下文。",
                                    ))
                                    .child(
                                        div()
                                            .text_color(link_color)
                                            .child(crate::i18n::text("了解更多")),
                                    ),
                            ),
                    )
                    .child(div().opacity(0.4).child(self.reference_button(
                        crate::i18n::text("保存"),
                        46.0,
                        None,
                        false,
                        theme,
                    ))),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .h(px(147.9375))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(textarea_border)
                    .px(px(10.0))
                    .py(px(8.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(theme.text_tertiary)
                    .child(crate::i18n::text("添加自定义指令…")),
            )
            .child(
                div()
                    .mt(px(39.0))
                    .h(px(37.125))
                    .px(px(12.0))
                    .rounded(px(20.0))
                    .bg(warning_fill)
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        svg()
                            .path("icons/settings-warning.svg")
                            .size(px(20.0))
                            .flex_none()
                            .text_color(theme.warning),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(theme.text)
                            .child(crate::i18n::text(page.sections[2].subtitle)),
                    ),
            )
            .child(div().mt(px(6.0)).child(self.agent_card(
                vec![self.agent_row(
                    personality.title,
                    personality.subtitle,
                    self.config_control("personality", theme, cx),
                    true,
                    theme,
                )],
                theme,
            )))
            .into_any_element()
    }
}
