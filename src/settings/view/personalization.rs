//! Personalization settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px};

use super::SettingsView;
use crate::{settings::PageSpec, theme::Theme};

impl SettingsView {
    pub(super) fn personalization_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
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
                            )),
                    ),
            )
            .child(div().mt(px(12.0)).child(memory_card))
            .child(div().mt(px(39.0)).child(self.agent_card(
                vec![self.agent_row(
                    crate::i18n::text("个性"),
                    crate::i18n::text("选择 ChatGPT 回复的默认语气"),
                    self.config_control("personality", theme, cx),
                    true,
                    theme,
                )],
                theme,
            )))
            .into_any_element()
    }
}
