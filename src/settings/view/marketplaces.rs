//! Configured marketplace management lives in its own settings tab. Bundled
//! catalogs supply plugins but are managed by the host application.
use gpui::{Context, IntoElement, Role, div, prelude::*, px};
use serde_json::{Map, Value};

use super::{SettingsView, plugins_catalog::PluginConfirmation};
use crate::{agent::AgentPluginMarketplace, components::icons::icon, theme::Theme};

fn app_managed_marketplace(name: &str) -> bool {
    let normalized = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "codex-official"
            | "openai-bundled"
            | "openai-primary-runtime"
            | "openai-curated"
            | "openai-curated-remote"
    )
}

impl SettingsView {
    fn marketplace_config(&self) -> Option<&Map<String, Value>> {
        self.config_editor
            .snapshot
            .as_ref()?
            .effective
            .get("marketplaces")?
            .as_object()
    }

    pub(super) fn configured_marketplace_count(&self) -> usize {
        self.marketplace_config().map_or(0, |sources| {
            sources
                .keys()
                .filter(|name| !app_managed_marketplace(name))
                .count()
        })
    }

    pub(super) fn configured_marketplaces(&self) -> Vec<&AgentPluginMarketplace> {
        let Some(configured) = self.marketplace_config() else {
            return Vec::new();
        };
        let mut rows = self
            .plugins_catalog
            .directory
            .catalog
            .as_ref()
            .map(|catalog| {
                catalog
                    .marketplaces
                    .iter()
                    .filter(|source| {
                        !app_managed_marketplace(&source.name)
                            && configured.contains_key(&source.name)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        rows.sort_by_key(|source| source.display_name().to_lowercase());
        rows
    }

    pub(super) fn can_upgrade_marketplace(&self, name: &str) -> bool {
        !app_managed_marketplace(name)
            && self
                .marketplace_config()
                .and_then(|sources| sources.get(name))
                .is_some_and(|source| {
                    source.get("source_type").and_then(Value::as_str) == Some("git")
                        && source
                            .get("source")
                            .and_then(Value::as_str)
                            .is_some_and(|source| !source.is_empty())
                })
    }

    pub(super) fn confirm_marketplace_remove(&mut self, name: String, cx: &mut Context<Self>) {
        if self.directory_operation_blocked()
            || !self
                .configured_marketplaces()
                .iter()
                .any(|source| source.name == name)
        {
            return;
        }
        self.plugins_catalog.confirmation = Some(PluginConfirmation::MarketplaceRemove {
            marketplace_name: name,
        });
        cx.notify();
    }

    pub(super) fn marketplaces_content(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let directory = &self.plugins_catalog.directory;
        let mut list = div().w_full().mt(px(40.)).flex().flex_col().gap(px(12.));
        if let Some(confirmation) = &self.plugins_catalog.confirmation {
            list = list.child(self.confirmation_card(confirmation, theme, cx));
        }
        for operation in directory.operations() {
            if let Some(message) = operation.phase.message() {
                list = list.child(self.catalog_state_card(message, None, theme));
            }
        }
        if directory.loading && directory.catalog.is_none() {
            return list
                .child(self.catalog_state_card(crate::i18n::text("正在读取插件目录…"), None, theme))
                .into_any_element();
        }
        if let Some(error) = &directory.error {
            list = list.child(self.catalog_state_card(
                &error.user_message(),
                Some(&error.message),
                theme,
            ));
        }
        let query = directory.search_term.trim().to_lowercase();
        let rows = self.configured_marketplaces();
        let mut visible = 0;
        let busy = self.directory_operation_blocked();
        for (index, source) in rows.iter().enumerate() {
            if !query.is_empty()
                && ![
                    source.name.as_str(),
                    source.display_name(),
                    source.path.as_deref().unwrap_or(""),
                ]
                .iter()
                .any(|text| text.to_lowercase().contains(&query))
            {
                continue;
            }
            visible += 1;
            let upgrade = self.can_upgrade_marketplace(&source.name) && !busy;
            let upgrade_name = source.name.clone();
            let remove_name = source.name.clone();
            let upgrade_key = source.name.clone();
            let remove_key = source.name.clone();
            list = list.child(div()
                .id(("marketplace-row", index))
                .debug_selector(move || format!("marketplace-row-{index}"))
                .w_full().min_h(px(76.)).p(px(8.)).rounded(px(20.))
                .hover(move |style| style.bg(theme.sidebar_hover))
                .flex().items_center().gap(px(12.))
                .child(div().size(px(40.)).flex_none().rounded(px(8.)).bg(theme.settings_panel)
                    .flex().items_center().justify_center()
                    .child(icon("marketplace", theme.text_tertiary.into()).size(px(20.))))
                .child(div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(2.))
                    .child(div().text_size(px(crate::typography::ui_size(14.))).line_height(px(21.)).font_weight(gpui::FontWeight(500.))
                        .text_color(theme.text).child(source.display_name().to_owned()))
                    .child(div().text_size(px(crate::typography::ui_size(14.))).line_height(px(18.375))
                        .text_color(theme.text_tertiary)
                        .child(crate::i18n::format!("{} 个插件" => "{} plugins", source.plugins.len())))
                    .when_some(source.path.clone(), |column, path| column.child(div()
                        .truncate().text_size(px(crate::typography::ui_size(14.))).line_height(px(18.375))
                        .text_color(theme.text_tertiary).child(path))))
                .child(div().flex_none().flex().items_center().gap(px(8.))
                    .child(div().id(("marketplace-upgrade", index))
                        .debug_selector(move || format!("marketplace-upgrade-{index}"))
                        .role(Role::Button).aria_label(crate::i18n::format!("更新市场 {}" => "Upgrade marketplace {}", source.display_name()))
                        .aria_disabled(!upgrade).focusable().tab_stop(upgrade)
                        .h(px(24.)).px(px(8.)).rounded(px(12.))
                        .border_1().border_color(theme.border).bg(theme.settings_button)
                        .flex().items_center().justify_center()
                        .text_size(px(crate::typography::ui_size(13.))).text_color(theme.text)
                        .opacity(if upgrade { 1. } else { 0.4 })
                        .when(upgrade, |button| button.cursor_pointer().hover(move |style| style.bg(theme.sidebar_hover)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if upgrade && !this.directory_operation_blocked() {
                                this.confirm_marketplace_upgrade(upgrade_name.clone(), cx);
                            }
                        }))
                        .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                this.confirm_marketplace_upgrade(upgrade_key.clone(), cx);
                                cx.stop_propagation();
                            }
                        }))
                        .child(crate::i18n::text("升级")))
                    .child(div().id(("marketplace-remove", index))
                        .debug_selector(move || format!("marketplace-remove-{index}"))
                        .role(Role::Button).aria_label(crate::i18n::format!("移除市场 {}" => "Remove marketplace {}", source.display_name()))
                        .aria_disabled(busy).focusable().tab_stop(!busy)
                        .size(px(24.)).rounded(px(8.))
                        .flex().items_center().justify_center()
                        .when(!busy, |button| button.cursor_pointer().hover(move |style| style.bg(theme.sidebar_hover)))
                        .opacity(if busy { 0.4 } else { 1. })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.directory_operation_blocked() {
                                this.confirm_marketplace_remove(remove_name.clone(), cx);
                            }
                        }))
                        .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                this.confirm_marketplace_remove(remove_key.clone(), cx);
                                cx.stop_propagation();
                            }
                        }))
                        .child(icon("queue-delete", theme.text_tertiary.into()).size(px(16.))))));
        }
        if visible == 0 && !directory.loading && directory.error.is_none() {
            list = list.child(
                div()
                    .py(px(32.))
                    .text_center()
                    .text_size(px(crate::typography::ui_size(13.)))
                    .text_color(theme.text_tertiary)
                    .child(crate::i18n::text("未找到任何市场")),
            );
        }
        list.into_any_element()
    }
}
