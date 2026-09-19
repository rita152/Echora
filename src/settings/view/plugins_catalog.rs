//! Plugins and apps directory segments. Every value on screen comes from the
//! coding agent: names, descriptions, artwork, counts, install state and
//! failure text. The client renders an empty, loading or failed state exactly
//! as the backend reported it and never substitutes sample entries.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::{PluginSegment, SettingsView};
use crate::{
    agent::{AgentPluginOperationOutcome, AgentPluginSummary},
    plugins::{PluginOperationPhase, PluginsDirectory},
    theme::{Theme, ThemeMode},
};

/// A catalog entry the server reported without artwork this client can draw.
const PLUGIN_ICON_FALLBACK: &str = "icons/settings-plugin-broken.svg";

/// One state-changing action waiting for the user's confirmation. The reference
/// confirms installs, uninstalls and marketplace changes before they run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PluginConfirmation {
    Install {
        plugin_name: String,
        marketplace_path: Option<String>,
        marketplace_name: String,
        display_name: String,
    },
    Uninstall {
        plugin_id: String,
        display_name: String,
    },
    MarketplaceUpgrade {
        marketplace_name: Option<String>,
    },
    MarketplaceRemove {
        marketplace_name: String,
    },
    MarketplaceAdd {
        source: String,
    },
    ShareSave {
        plugin_path: Option<String>,
        remote_plugin_id: Option<String>,
        display_name: String,
    },
    ShareUpdateTargets {
        remote_plugin_id: String,
        discoverability: crate::agent::AgentPluginShareDiscoverability,
        /// Targets exactly as the server reported them, minus roles the update
        /// API cannot express.
        targets: Vec<crate::agent::AgentPluginShareTarget>,
        /// Set when the server's own share list contains a role this client
        /// cannot send back; the action is then refused instead of silently
        /// dropping access.
        blocked: Option<String>,
    },
    ShareDelete {
        remote_plugin_id: String,
    },
}

impl PluginConfirmation {
    fn title(&self) -> String {
        match self {
            Self::Install { display_name, .. } => {
                crate::i18n::format!("安装 {display_name}" => "Install {display_name}")
            }
            Self::Uninstall { display_name, .. } => {
                crate::i18n::format!("卸载 {display_name}" => "Uninstall {display_name}")
            }
            Self::MarketplaceUpgrade { .. } => crate::i18n::text("更新插件目录").to_owned(),
            Self::MarketplaceRemove { marketplace_name } => {
                crate::i18n::format!("移除 {marketplace_name}" => "Remove {marketplace_name}")
            }
            Self::MarketplaceAdd { source } => {
                crate::i18n::format!("添加 {source}" => "Add {source}")
            }
            Self::ShareSave { display_name, .. } => {
                crate::i18n::format!("共享 {display_name}" => "Share {display_name}")
            }
            Self::ShareUpdateTargets { .. } => crate::i18n::text("更新共享范围").to_owned(),
            Self::ShareDelete { remote_plugin_id } => {
                crate::i18n::format!("取消共享 {remote_plugin_id}" => "Unshare {remote_plugin_id}")
            }
        }
    }

    fn body(&self) -> String {
        match self {
            Self::Install {
                plugin_name,
                marketplace_name,
                ..
            } => {
                crate::i18n::format!("将从 {marketplace_name} 安装插件 {plugin_name}。" => "Install plugin {plugin_name} from {marketplace_name}.")
            }
            Self::Uninstall { plugin_id, .. } => {
                crate::i18n::format!("将卸载插件 {plugin_id}；它的技能与 MCP 服务器会同时移除。" => "Uninstall plugin {plugin_id}, including its skills and MCP servers.")
            }
            Self::MarketplaceUpgrade { marketplace_name } => match marketplace_name {
                Some(name) => {
                    crate::i18n::format!("将重新拉取 {name} 的插件目录。" => "Refresh the plugin catalog for {name}.")
                }
                None => crate::i18n::text("将重新拉取全部插件目录。").to_owned(),
            },
            Self::MarketplaceRemove { marketplace_name } => {
                crate::i18n::format!("将移除 marketplace {marketplace_name}。" => "Remove marketplace {marketplace_name}.")
            }
            Self::MarketplaceAdd { source } => {
                crate::i18n::format!("将从 {source} 添加一个 marketplace。" => "Add a marketplace from {source}.")
            }
            Self::ShareSave { .. } => {
                crate::i18n::text("将把这个插件发布到账号的插件服务，并保留服务端返回的共享链接。")
                    .to_owned()
            }
            Self::ShareUpdateTargets {
                discoverability,
                targets,
                blocked,
                ..
            } => match blocked {
                Some(reason) => reason.clone(),
                None => crate::i18n::format!(
                    "将共享范围改为 {discoverability:?}，共 {} 个目标。" => "Change sharing scope to {discoverability:?}, with {} targets.",
                    targets.len()
                ),
            },
            Self::ShareDelete { .. } => crate::i18n::text("将删除这个插件的共享记录。").to_owned(),
        }
    }

    fn confirm_label(&self) -> &'static str {
        match self {
            Self::Install { .. } => crate::i18n::text("安装"),
            Self::Uninstall { .. } => crate::i18n::text("卸载"),
            Self::MarketplaceUpgrade { .. } => crate::i18n::text("更新"),
            Self::MarketplaceRemove { .. } => crate::i18n::text("移除"),
            Self::MarketplaceAdd { .. } => crate::i18n::text("添加"),
            Self::ShareSave { .. } => crate::i18n::text("共享"),
            Self::ShareUpdateTargets { .. } => crate::i18n::text("更新"),
            Self::ShareDelete { .. } => crate::i18n::text("取消共享"),
        }
    }
}

/// App directory state and its capture-only hover row.
#[derive(Default)]
pub(super) struct AppsPanel {
    pub directory: crate::apps::AppsDirectory,
    pub hover_row: Option<String>,
}

#[derive(Default)]
pub(super) struct PluginsPanel {
    pub directory: PluginsDirectory,
    /// Row the pointer is over; capture-only, like the MCP panel.
    pub hover_row: Option<String>,
    /// Plugin whose detail read is on screen.
    pub open_plugin: Option<String>,
    /// Pending confirmation, if any.
    pub confirmation: Option<PluginConfirmation>,
}

impl SettingsView {
    /// Total plugin count for the segment badge: what the catalog actually
    /// returned, never a reference constant.
    pub(super) fn live_plugin_count(&self) -> usize {
        self.plugins_catalog
            .directory
            .catalog
            .as_ref()
            .map(|catalog| catalog.plugins().count())
            .unwrap_or(0)
    }

    /// App count for the segment badge: the directory page the server answered
    /// with. An unreachable directory reports zero rather than a guess.
    pub(super) fn live_app_count(&self) -> usize {
        self.apps.directory.entries().len()
    }

    fn catalog_state_card(
        &self,
        message: &str,
        detail: Option<&str>,
        theme: &Theme,
    ) -> gpui::AnyElement {
        div()
            .px(px(16.0))
            .py(px(12.0))
            .rounded(px(20.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .flex()
            .flex_col()
            .gap(px(2.0))
            .text_size(px(13.0))
            .line_height(px(19.0))
            .text_color(theme.settings_description)
            .child(message.to_owned())
            .when_some(detail.map(str::to_owned), |card, detail| {
                card.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(17.0))
                        .text_color(theme.text_tertiary)
                        .child(detail),
                )
            })
            .into_any_element()
    }

    /// The plugin and app rows share one geometry so the two segments line up
    /// with the reference at the same viewport.
    fn catalog_row(
        &self,
        icon: gpui::AnyElement,
        title: String,
        subtitle: Option<String>,
        trailing: gpui::AnyElement,
    ) -> gpui::AnyElement {
        let subtitle_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x1a1c1fa6),
            ThemeMode::Dark => gpui::rgba(0xffffffa6),
        };
        div()
            .h(px(68.0625))
            .pl(px(9.0))
            .pr(px(8.0))
            .rounded(px(12.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .size(px(40.0))
                    .rounded(px(10.0))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .items_start()
                    .child(icon),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .relative()
                    .top(px(1.0))
                    .flex()
                    .flex_col()
                    .font_family(".SystemUIFont")
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(title),
                    )
                    .when_some(subtitle, |column, subtitle| {
                        column.child(
                            div()
                                .relative()
                                .top(px(-1.0))
                                .text_size(px(13.0))
                                .line_height(px(21.125))
                                .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                                .text_color(subtitle_color)
                                .child(subtitle),
                        )
                    }),
            )
            .child(trailing)
            .into_any_element()
    }

    fn plugin_icon(
        &self,
        plugin: &AgentPluginSummary,
        theme: &Theme,
        index: usize,
    ) -> gpui::AnyElement {
        let dark = self.mode == ThemeMode::Dark;
        match plugin
            .interface
            .as_ref()
            .and_then(|interface| interface.logo_source(dark))
        {
            Some(source) => div()
                .size(px(40.0))
                .child(gpui::img(source.to_owned()).size(px(40.0)))
                .into_any_element(),
            // The server shipped no artwork this client can load. The row keeps
            // the same footprint with the placeholder the reference uses for a
            // plugin whose assets are missing.
            None => div()
                .id(("plugin-icon-fallback", index))
                .size(px(40.0))
                .border_1()
                .border_color(theme.border)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(PLUGIN_ICON_FALLBACK)
                        .size(px(16.0))
                        .text_color(theme.text_tertiary),
                )
                .into_any_element(),
        }
    }

    /// A switch that performs an operation instead of only toggling a local
    /// override: the value shown is the server's install state.
    fn plugin_switch(
        &self,
        installed: bool,
        confirmation: PluginConfirmation,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .id(("plugin-switch", confirmation.title().len()))
            .w(px(32.0))
            .h(px(20.0))
            .p(px(2.0))
            .rounded_full()
            .flex()
            .items_center()
            .when(installed, |track| {
                track.justify_end().bg(theme.settings_accent)
            })
            .when(!installed, |track| {
                track.justify_start().bg(theme.settings_switch_off)
            })
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.plugins_catalog.confirmation = Some(confirmation.clone());
                cx.notify();
            }))
            .child(
                div()
                    .size(px(16.0))
                    .rounded_full()
                    .bg(gpui::white())
                    .border_1()
                    .border_color(gpui::rgba(0x00000012)),
            )
            .into_any_element()
    }

    /// The plugin rows of the current catalog (or of the search answer).
    pub(super) fn plugins_list(&self, theme: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let directory = &self.plugins_catalog.directory;
        let mut list = div().mt(px(36.0)).flex().flex_col().gap(px(8.0));
        if directory.loading && directory.catalog.is_none() {
            list = list.child(self.catalog_state_card(
                crate::i18n::text("正在读取插件目录…"),
                None,
                theme,
            ));
        }
        if let Some(catalog) = &directory.catalog {
            list = list.child(self.catalog_state_card(
                &crate::i18n::format!(
                    "已安装 {} / 共 {} 个插件" => "{} installed / {} plugins total",
                    catalog.installed_plugin_count(),
                    catalog.plugins().count()
                ),
                None,
                theme,
            ));
        }
        if let Some(error) = &directory.error {
            list = list.child(self.catalog_state_card(
                &error.user_message(),
                Some(&error.message),
                theme,
            ));
        }
        if directory.stale {
            list = list.child(self.catalog_state_card(
                crate::i18n::text("插件目录已在后端更新"),
                Some(crate::i18n::text("重新打开插件页或刷新目录以读取最新状态")),
                theme,
            ));
        }
        for operation in directory.operations() {
            if let Some(message) = operation.phase.message() {
                list = list.child(self.catalog_state_card(message, None, theme));
            }
        }
        if directory.busy() {
            list = list.child(self.catalog_state_card(
                crate::i18n::text("正在处理插件操作…"),
                None,
                theme,
            ));
        }
        if let Some(confirmation) = &self.plugins_catalog.confirmation {
            list = list.child(self.confirmation_card(confirmation, theme, cx));
        }
        if let Some(detail) = &directory.detail {
            list = list.child(self.plugin_detail_card(detail, theme, cx));
        }
        if let Some(error) = &directory.detail_error {
            list = list.child(self.catalog_state_card(
                &error.user_message(),
                Some(&error.message),
                theme,
            ));
        }
        if let Some(reconcile) = &directory.reconcile {
            let changed = reconcile.changed_plugins.len();
            let failed = reconcile.failed_remote_plugin_ids.len()
                + reconcile.failed_materialization_remote_plugin_ids.len();
            if changed > 0 || failed > 0 {
                list = list.child(self.catalog_state_card(
                    &crate::i18n::format!("启动核对：{changed} 个插件已更新，{failed} 个失败" => "Startup check: {changed} plugins updated, {failed} failed"),
                    None,
                    theme,
                ));
            }
        }
        list = list.child(self.marketplace_management_card(theme, cx));
        let rows = directory.visible_plugins();
        if rows.is_empty() && !directory.loading && directory.error.is_none() {
            let message = if directory.search_term.trim().is_empty() {
                crate::i18n::text("插件目录中没有任何条目")
            } else {
                crate::i18n::text("没有匹配的插件")
            };
            list = list.child(self.catalog_state_card(message, None, theme));
        }
        for (index, row) in rows.iter().enumerate() {
            let plugin = row.plugin;
            let installed =
                directory.displayed_installed(&plugin.name, &plugin.id, plugin.installed);
            let confirmation = if installed {
                PluginConfirmation::Uninstall {
                    plugin_id: plugin.id.clone(),
                    display_name: plugin.display_name().to_owned(),
                }
            } else {
                PluginConfirmation::Install {
                    plugin_name: plugin.name.clone(),
                    marketplace_path: row.marketplace_path.map(str::to_owned),
                    marketplace_name: row.marketplace_name.to_owned(),
                    display_name: plugin.display_name().to_owned(),
                }
            };
            let icon = self.plugin_icon(plugin, theme, index);
            let trailing = if installed || plugin.installable() {
                self.plugin_switch(installed, confirmation, theme, cx)
            } else {
                self.app_state_switch(false, false, theme)
            };
            let hovered = self.plugins_catalog.hover_row.as_deref() == Some(plugin.id.as_str());
            list = list.child(
                div()
                    .id(("plugin-row", index))
                    .when(hovered, |row| row.bg(theme.settings_button))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.open_visible_plugin(index, cx)),
                    )
                    .child(
                        self.catalog_row(
                            icon,
                            plugin.display_name().to_owned(),
                            plugin
                                .disabled_reason
                                .map(|reason| reason.label().to_owned())
                                .or_else(|| plugin.description().map(str::to_owned)),
                            trailing,
                        ),
                    ),
            );
        }
        list.into_any_element()
    }

    pub(super) fn apps_list(&self, theme: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let directory = &self.apps.directory;
        let mut list = div().mt(px(36.0)).flex().flex_col().gap(px(8.0));
        // Keep an explicit pending state until the first directory response
        // arrives. A disconnected app-server can leave the request unresolved
        // for a while; rendering an empty panel would look like a valid empty
        // catalog and diverge from the reference loading treatment.
        if directory.page.is_none() && !directory.resolved() {
            list = list.child(self.catalog_state_card(
                crate::i18n::text("正在读取应用目录…"),
                None,
                theme,
            ));
        }
        if let Some(error) = &directory.error {
            list = list.child(self.catalog_state_card(
                &error.user_message(),
                Some(&error.message),
                theme,
            ));
        }
        if directory.stale {
            list = list.child(self.catalog_state_card(
                crate::i18n::text("应用目录已在后端更新"),
                Some(crate::i18n::text("重新打开应用页或刷新目录以读取最新状态")),
                theme,
            ));
        }
        let entries = directory.entries();
        if entries.is_empty() && directory.resolved() && directory.error.is_none() {
            list = list.child(self.catalog_state_card(
                crate::i18n::text("尚未连接任何应用"),
                None,
                theme,
            ));
        }
        for (index, app) in entries.iter().enumerate() {
            let app_id = app.id.clone();
            let subtitle = app.description.clone();
            let enabled = directory
                .installed_entry(&app.id)
                .map(|installed| installed.enabled)
                .unwrap_or(app.is_enabled);
            let icon = match app.logo_url.as_deref() {
                // The directory reports remote artwork for hosted apps; a local
                // asset is drawn when the server sent one instead.
                Some(_url) => div()
                    .id(("app-icon", index))
                    .size(px(40.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.0))
                    .text_color(theme.text_tertiary)
                    .child(app.name.chars().next().unwrap_or('?').to_string())
                    .into_any_element(),
                None => div()
                    .id(("app-icon-blank", index))
                    .size(px(40.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(theme.border)
                    .into_any_element(),
            };
            let trailing = self.app_state_switch(enabled, app.is_accessible, theme);
            let app_hovered = self.apps.hover_row.as_deref() == Some(app.id.as_str());
            list =
                list.child(
                    div()
                        .id(("app-row", index))
                        .when(app_hovered, |row| row.bg(theme.settings_button))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.apps_open_detail(app_id.clone(), cx)
                        }))
                        .child(self.catalog_row(
                            icon,
                            app.display_name().to_owned(),
                            subtitle,
                            trailing,
                        )),
                );
        }
        list.into_any_element()
    }

    /// App enabled state as the server reported it. No app enable/disable
    /// method exists in the protocol surface this client integrates, so the
    /// control renders state and never pretends to write it.
    fn app_state_switch(&self, enabled: bool, accessible: bool, theme: &Theme) -> gpui::AnyElement {
        div()
            .w(px(32.0))
            .h(px(20.0))
            .p(px(2.0))
            .rounded_full()
            .flex()
            .items_center()
            .when(enabled, |track| {
                track.justify_end().bg(theme.settings_accent)
            })
            .when(!enabled, |track| {
                track.justify_start().bg(theme.settings_switch_off)
            })
            .when(!accessible, |track| track.opacity(0.5))
            .child(
                div()
                    .size(px(16.0))
                    .rounded_full()
                    .bg(gpui::white())
                    .border_1()
                    .border_color(gpui::rgba(0x00000012)),
            )
            .into_any_element()
    }

    /// Turns one operation outcome into the phase the panel renders. A success
    /// keeps the server's own confirmation path; a failure keeps its message so
    /// the user can retry the intent deliberately.
    pub(super) fn operation_phase<T>(
        outcome: &AgentPluginOperationOutcome<T>,
        success: &str,
    ) -> PluginOperationPhase {
        match outcome {
            AgentPluginOperationOutcome::Succeeded(_) => PluginOperationPhase::Succeeded {
                message: success.to_owned(),
            },
            AgentPluginOperationOutcome::Failed { message, .. } => PluginOperationPhase::Failed {
                message: message.clone(),
                outcome_unknown: false,
            },
            AgentPluginOperationOutcome::TimedOut { message } => PluginOperationPhase::TimedOut {
                message: message.clone(),
            },
            AgentPluginOperationOutcome::Unknown { message } => PluginOperationPhase::Unknown {
                message: message.clone(),
            },
        }
    }

    /// Convenience for the segment body switch.
    pub(super) fn catalog_body(
        &self,
        segment: PluginSegment,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match segment {
            PluginSegment::Plugins => self.plugins_list(theme, cx),
            PluginSegment::Apps => self.apps_list(theme, cx),
            PluginSegment::Mcp => self.mcp_segment_content(theme, cx),
            PluginSegment::Skills => self.skills_segment_content(theme, cx),
        }
    }

    /// The confirmation every state-changing catalog action passes through.
    /// The marketplaces the server reported, with the two actions the plugin
    /// protocol offers for them. Names and paths are the server's own.
    fn marketplace_management_card(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let marketplaces = self
            .plugins_catalog
            .directory
            .catalog
            .as_ref()
            .map(|catalog| {
                catalog
                    .marketplaces
                    .iter()
                    .map(|marketplace| {
                        (
                            marketplace.name.clone(),
                            marketplace.display_name().to_owned(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut card = div().w_full().flex().flex_col().gap(px(4.0));
        for (index, (name, display_name)) in marketplaces.into_iter().enumerate() {
            let upgrade_name = name.clone();
            let remove = PluginConfirmation::MarketplaceRemove {
                marketplace_name: name.clone(),
            };
            card = card.child(
                div()
                    .id(("marketplace-row", index))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(theme.settings_description)
                            .child(display_name),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id(("marketplace-upgrade", index))
                                    .h(px(24.0))
                                    .px(px(10.0))
                                    .rounded(px(12.0))
                                    .bg(theme.settings_button)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.text)
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let name = upgrade_name.clone();
                                        this.confirm_marketplace_upgrade(name, cx);
                                    }))
                                    .child(crate::i18n::text("更新")),
                            )
                            .child(
                                div()
                                    .id(("marketplace-remove", index))
                                    .h(px(24.0))
                                    .px(px(10.0))
                                    .rounded(px(12.0))
                                    .bg(theme.settings_button)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.text)
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.plugins_catalog.confirmation = Some(remove.clone());
                                        cx.notify();
                                    }))
                                    .child(crate::i18n::text("移除")),
                            ),
                    ),
            );
        }
        card.into_any_element()
    }

    fn confirmation_card(
        &self,
        confirmation: &PluginConfirmation,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let add_source = matches!(confirmation, PluginConfirmation::MarketplaceAdd { .. })
            .then(|| self.marketplace_source_input.clone());
        div()
            .w_full()
            .rounded(px(20.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .px(px(16.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(confirmation.title()),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.settings_description)
                    .child(confirmation.body()),
            )
            .when_some(add_source, |card, input| card.child(input))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("catalog-confirm")
                            .h(px(28.0))
                            .px(px(12.0))
                            .rounded(px(12.5))
                            .bg(theme.settings_accent)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(gpui::white())
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_plugin_operation(cx);
                            }))
                            .child(confirmation.confirm_label()),
                    )
                    .child(
                        div()
                            .id("catalog-cancel")
                            .h(px(28.0))
                            .px(px(12.0))
                            .rounded(px(12.5))
                            .bg(theme.settings_button)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_catalog_confirmation(cx);
                            }))
                            .child(crate::i18n::text("取消")),
                    ),
            )
            .into_any_element()
    }

    /// Detail read through `plugin/read`: the server's own description, the
    /// skills it ships, its MCP servers, and the share actions that the account
    /// is allowed to perform.
    fn plugin_detail_card(
        &self,
        detail: &crate::agent::AgentPluginDetail,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut card = div()
            .w_full()
            .rounded(px(20.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .px(px(16.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(detail.summary.display_name().to_owned()),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.settings_description)
                    .child(crate::i18n::format!(
                        "来源：{} · 市场：{}" => "Source: {} · Marketplace: {}",
                        detail.summary.source.kind(),
                        detail.marketplace_name
                    ))
                    .child(match &detail.description {
                        Some(description) => description.clone(),
                        None => format!("marketplace：{}", detail.marketplace_name),
                    }),
            );
        let remote_plugin_id = detail.summary.remote_plugin_id.clone().or_else(|| {
            detail
                .summary
                .share_context
                .as_ref()
                .map(|context| context.remote_plugin_id.clone())
        });
        let marketplace_name = detail.marketplace_name.clone();
        for (index, skill) in detail.skills.iter().enumerate() {
            let skill_name = skill.name.clone();
            let marketplace = marketplace_name.clone();
            let remote = remote_plugin_id.clone();
            // The protocol serves skill contents for shared (remote) plugins
            // only; the row stays visible either way and the server's answer is
            // what decides whether it can be read.
            card = card.child(
                div()
                    .id(("plugin-skill", index))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.text_tertiary)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_plugin_skill(
                            marketplace.clone(),
                            remote.clone(),
                            skill_name.clone(),
                            cx,
                        );
                    }))
                    .child(crate::i18n::format!("技能：{}" => "Skill: {}", skill.name)),
            );
        }
        for server in &detail.mcp_servers {
            card = card.child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.text_tertiary)
                    .child(format!("MCP：{server}")),
            );
        }
        if let Some(content) = &self.plugins_catalog.directory.skill {
            card = card.child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.text_tertiary)
                    .child(match &content.contents {
                        Some(contents) => contents.chars().take(400).collect::<String>(),
                        None => crate::i18n::text("服务端未返回该技能内容").to_owned(),
                    }),
            );
        }
        if let Some(error) = &self.plugins_catalog.directory.skill_error {
            card = card.child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.settings_description)
                    .child(error.message.clone()),
            );
        }
        let local_path = match &detail.summary.source {
            crate::agent::AgentPluginSource::Local { path } => Some(path.display().to_string()),
            _ => None,
        };
        let display_name = detail.summary.display_name().to_owned();
        let share_save = PluginConfirmation::ShareSave {
            plugin_path: local_path,
            remote_plugin_id: remote_plugin_id.clone(),
            display_name,
        };
        let mut share_actions = div().flex().items_center().gap(px(8.0)).child(
            div()
                .id("plugin-share-save")
                .h(px(28.0))
                .px(px(12.0))
                .rounded(px(12.5))
                .bg(theme.settings_button)
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(theme.text)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.plugins_catalog.confirmation = Some(share_save.clone());
                    cx.notify();
                }))
                .child(crate::i18n::text("共享")),
        );
        if let Some(remote_plugin_id) = remote_plugin_id {
            let discoverability = detail
                .summary
                .share_context
                .as_ref()
                .and_then(|context| context.discoverability)
                .unwrap_or(crate::agent::AgentPluginShareDiscoverability::Private);
            // The targets are the server's own principals. The update API
            // accepts reader and editor only, so an owner entry makes the call
            // unrepresentable and the action is refused instead of narrowing
            // access silently.
            let principals = detail
                .summary
                .share_context
                .as_ref()
                .and_then(|context| context.share_principals.clone())
                .unwrap_or_default();
            let mut targets = Vec::new();
            let mut blocked = None;
            for principal in &principals {
                match principal.role {
                    crate::agent::AgentPluginShareRole::Reader
                    | crate::agent::AgentPluginShareRole::Editor => {
                        targets.push(crate::agent::AgentPluginShareTarget {
                            principal_id: principal.principal_id.clone(),
                            principal_type: principal.principal_type,
                            role: principal.role,
                            extra: Default::default(),
                        });
                    }
                    crate::agent::AgentPluginShareRole::Owner => {
                        blocked = Some(crate::i18n::format!(
                            "服务端共享列表包含 owner 角色（{}），更新接口无法表达该角色" => "The server sharing list includes an owner role ({}) that the update interface cannot represent",
                            principal.name
                        ));
                    }
                }
            }
            let update = PluginConfirmation::ShareUpdateTargets {
                remote_plugin_id: remote_plugin_id.clone(),
                discoverability,
                targets,
                blocked,
            };
            let delete = PluginConfirmation::ShareDelete {
                remote_plugin_id: remote_plugin_id.clone(),
            };
            share_actions = share_actions
                .child(
                    div()
                        .id("plugin-share-update")
                        .h(px(28.0))
                        .px(px(12.0))
                        .rounded(px(12.5))
                        .bg(theme.settings_button)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(theme.text)
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.plugins_catalog.confirmation = Some(update.clone());
                            cx.notify();
                        }))
                        .child(crate::i18n::text("更新共享范围")),
                )
                .child(
                    div()
                        .id("plugin-share-delete")
                        .h(px(28.0))
                        .px(px(12.0))
                        .rounded(px(12.5))
                        .bg(theme.settings_button)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(theme.text)
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.plugins_catalog.confirmation = Some(delete.clone());
                            cx.notify();
                        }))
                        .child(crate::i18n::text("取消共享")),
                );
        }
        card = card.child(share_actions);
        if let Some(error) = &self.plugins_catalog.directory.shares_error {
            card = card.child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.settings_description)
                    .child(error.message.clone()),
            );
        }
        if let Some(shares) = &self.plugins_catalog.directory.shares {
            for entry in shares.entries.iter().take(5) {
                card = card.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(theme.text_tertiary)
                        .child(crate::i18n::format!(
                            "已共享：{}（{}）" => "Shared: {} ({})",
                            entry.plugin.display_name(),
                            entry.plugin.remote_plugin_id.as_deref().unwrap_or("—")
                        )),
                );
            }
        }
        card = card.child(
            div()
                .id("plugin-share-list")
                .h(px(24.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(theme.settings_description)
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| this.refresh_plugin_shares(cx)))
                .child(crate::i18n::text("读取已共享的插件")),
        );
        card.into_any_element()
    }
}
