//! Sidebar directory presentation, sharing the settings controller and real
//! plugin lifecycle. No separate catalog, private service, or sample entries.
use std::{collections::BTreeMap, path::PathBuf};

use gpui::{Context, IntoElement, Role, div, point, prelude::*, px};

use super::{CloseSettings, PluginSegment, SettingsView, plugins_catalog::PluginConfirmation};
use crate::{
    components::{icons::icon, markdown::render_pull_request_markdown},
    plugins::{PluginOperationPhase, PluginRow},
    theme::{Theme, UI_FONT_FAMILY},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scope {
    All,
    Installed,
}

impl Scope {
    fn label(self) -> String {
        match self {
            Self::All => crate::i18n::format!("全部" => "All"),
            Self::Installed => crate::i18n::format!("已安装" => "Installed"),
        }
    }
}

impl SettingsView {
    fn directory_source_status(&self, theme: Theme) -> gpui::AnyElement {
        use crate::plugins::{PluginCatalogSource as Source, PluginCatalogState as State};
        div().flex().flex_col().gap(px(4.)).children(
            self.plugins_catalog.directory.sources.iter().map(|(source, state)| {
                let label = match source {
                    Source::Local => crate::i18n::format!("本地目录" => "Local catalogs"),
                    Source::Vertical => crate::i18n::format!("Vertical 目录" => "Vertical catalog"),
                    Source::WorkspaceDirectory => crate::i18n::format!("工作区目录" => "Workspace directory"),
                    Source::SharedWithMe => crate::i18n::format!("与我共享" => "Shared with me"),
                    Source::CreatedByMeRemote => crate::i18n::format!("我创建的远程插件" => "Remote plugins created by me"),
                };
                let status = match state {
                    State::Loading => crate::i18n::format!("正在加载…" => "Loading…"),
                    State::Loaded(catalog) if !catalog.marketplace_load_errors.is_empty() => crate::i18n::format!("目录部分加载失败" => "Some marketplaces failed to load"),
                    State::Loaded(catalog) if catalog.plugins().next().is_none() => crate::i18n::format!("目录为空" => "Catalog is empty"),
                    State::Loaded(catalog) => crate::i18n::format!("{} 个插件" => "{} plugins", catalog.plugins().count()),
                    State::Failed(error) if error.kind == crate::agent::AgentPluginsErrorKind::Unsupported => crate::i18n::format!("此目录类型不受支持" => "This catalog type is not supported"),
                    State::Failed(error) => error.user_message(),
                };
                let text = format!("{label}: {status}");
                div().id(format!("directory-source-{source:?}")).role(Role::Group).aria_label(text.clone()).text_size(px(12.))
                    .text_color(theme.text_secondary).child(text)
                    .when(matches!(state, State::Failed(error) if error.kind != crate::agent::AgentPluginsErrorKind::AuthenticationRequired), |row| {
                        let State::Failed(error) = state else { unreachable!() };
                        row.child(div().child(error.message.clone()))
                    })
            })
        ).into_any_element()
    }

    pub fn open_plugin_directory(&mut self, cwd: PathBuf, cx: &mut Context<Self>) {
        if self.plugins_cwd.as_ref() != Some(&cwd) {
            self.plugins_catalog.directory.installed = None;
            self.skills.directory = crate::skills::SkillsDirectory::for_cwd(cwd.clone());
        }
        self.plugins_cwd = Some(cwd);
        self.directory_presentation = true;
        self.plugins_segment = PluginSegment::Plugins;
        self.clear_directory_navigation(cx);
        self.refresh_plugins(false, cx);
        cx.notify();
    }

    pub fn show_settings_shell(&mut self, cx: &mut Context<Self>) {
        if self.directory_presentation {
            self.directory_presentation = false;
            self.clear_directory_navigation(cx);
            self.plugins_catalog.directory.cycle =
                self.plugins_catalog.directory.cycle.wrapping_add(1);
            self.plugins_catalog.directory.catalog = None;
            self.plugins_catalog.directory.loading = false;
        }
        cx.notify();
    }

    pub fn set_directory_titlebar_inset(&mut self, inset: f32) {
        self.directory_titlebar_inset = inset;
    }

    fn clear_directory_navigation(&mut self, cx: &mut Context<Self>) {
        self.plugins_catalog.open_plugin = None;
        self.plugins_catalog.confirmation = None;
        self.plugins_catalog.pending_reference = None;
        self.plugins_catalog.directory.clear_detail();
        self.plugins_catalog.directory.clear_search();
        self.skills.query.clear();
        self.plugin_search_input.update(cx, |input, cx| {
            input.set_text_silently("", cx);
            input.set_placeholder(crate::i18n::text("搜索插件"), cx);
            input.set_accessible_name(crate::i18n::text("搜索插件"));
        });
        self.content_scroll.set_offset(point(px(0.), px(0.)));
    }

    fn select_directory_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        if self.directory_scope == scope {
            return;
        }
        self.directory_scope = scope;
        self.clear_directory_navigation(cx);
        // Do not display another scope while its independent read is pending.
        self.plugins_catalog.directory.catalog = None;
        self.refresh_plugins(false, cx);
        cx.notify();
    }

    pub fn dismiss_plugin_directory(&mut self, cx: &mut Context<Self>) {
        if self.plugins_catalog.confirmation.take().is_some() {
            cx.notify();
        } else if self.plugins_catalog.open_plugin.take().is_some() {
            self.plugins_catalog.directory.clear_detail();
            self.content_scroll.set_offset(point(px(0.), px(0.)));
            cx.notify();
        } else {
            cx.emit(CloseSettings);
        }
    }

    pub(super) fn directory_search_changed(&mut self, text: String, cx: &mut Context<Self>) {
        if !self.directory_presentation {
            return;
        }
        if self.plugins_segment == PluginSegment::Skills {
            self.skills.query = text;
            cx.notify();
            return;
        }
        self.plugins_catalog.directory.set_search_term(text);
        self.plugins_catalog.directory.searching = false;
        cx.notify();
    }

    fn directory_rows(&self) -> Vec<PluginRow<'_>> {
        let directory = &self.plugins_catalog.directory;
        if self.directory_scope == Scope::Installed {
            let query = directory.search_term.trim().to_lowercase();
            return directory
                .installed_rows()
                .into_iter()
                .filter(|row| {
                    query.is_empty()
                        || row.plugin.display_name().to_lowercase().contains(&query)
                        || row.plugin.name.to_lowercase().contains(&query)
                        || row
                            .plugin
                            .description()
                            .is_some_and(|s| s.to_lowercase().contains(&query))
                })
                .collect();
        }
        let query = directory.search_term.trim().to_lowercase();
        directory
            .catalog
            .as_ref()
            .into_iter()
            .flat_map(|catalog| catalog.plugins())
            .filter(|(_, plugin)| {
                query.is_empty()
                    || plugin.display_name().to_lowercase().contains(&query)
                    || plugin.name.to_lowercase().contains(&query)
                    || plugin
                        .description()
                        .is_some_and(|s| s.to_lowercase().contains(&query))
                    || plugin.keywords.as_ref().is_some_and(|words| {
                        words
                            .iter()
                            .any(|word| word.to_lowercase().contains(&query))
                    })
            })
            .map(|(marketplace, plugin)| PluginRow {
                plugin,
                marketplace_name: &marketplace.name,
                marketplace_path: marketplace.path.as_deref(),
            })
            .collect()
    }

    fn directory_operation_blocked(&self) -> bool {
        self.plugins_catalog.directory.operations().any(|op| {
            op.phase.busy()
                || matches!(
                    op.phase,
                    PluginOperationPhase::Unknown { .. }
                        | PluginOperationPhase::TimedOut { .. }
                        | PluginOperationPhase::Failed {
                            outcome_unknown: true,
                            ..
                        }
                )
        })
    }

    fn directory_action(
        &self,
        row: PluginRow<'_>,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let plugin = row.plugin;
        let installed = self.plugins_catalog.directory.confirmed_installed(plugin);
        let blocked = self.directory_operation_blocked()
            || (!installed
                && (!plugin.installable()
                    || plugin.disabled_reason.is_some()
                    || plugin.availability
                        == Some(crate::agent::AgentPluginAvailability::DisabledByAdmin)))
            || self.plugins_catalog.directory.installed_loading
            || self.plugins_catalog.directory.installed_error.is_some();
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
        let label = if installed {
            crate::i18n::format!("卸载 {}" => "Uninstall {}", plugin.display_name())
        } else {
            crate::i18n::format!("安装 {}" => "Install {}", plugin.display_name())
        };
        div()
            .id(format!("directory-plugin-action-{}", plugin.id))
            .role(Role::Button)
            .aria_label(label)
            .aria_disabled(blocked)
            .focusable()
            .tab_stop(!blocked)
            .flex_none()
            .h(px(28.))
            .px(px(if compact { 7. } else { 14. }))
            .rounded(px(10.))
            .bg(theme.settings_button)
            .text_size(px(13.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .opacity(if blocked { 0.4 } else { 1. })
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if !blocked && !this.directory_operation_blocked() {
                    this.plugins_catalog.confirmation = Some(confirmation.clone());
                    cx.notify();
                }
            }))
            .when(compact, |button| {
                button.child(
                    icon(if installed { "check" } else { "add" }, theme.text.into()).size(px(14.)),
                )
            })
            .when(!compact, |button| {
                button.child(if installed {
                    crate::i18n::format!("卸载插件" => "Uninstall plugin")
                } else {
                    crate::i18n::format!("安装插件" => "Install plugin")
                })
            })
            .into_any_element()
    }

    fn directory_entry(
        &self,
        row: PluginRow<'_>,
        index: usize,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let plugin = row.plugin;
        let id = plugin.id.clone();
        let description = plugin
            .disabled_reason
            .map(|r| r.label().to_owned())
            .or_else(|| plugin.description().map(str::to_owned));
        div()
            .id(format!("directory-plugin-{}", plugin.id))
            .debug_selector(move || format!("directory-plugin-{index}"))
            .role(Role::Button)
            .aria_label(crate::i18n::format!("查看 {}" => "View {}", plugin.display_name()))
            .focusable()
            .tab_stop(true)
            .w_full()
            .min_w(px(0.))
            .h(px(76.))
            .px(px(12.))
            .rounded(px(14.))
            .hover(|s| s.bg(theme.sidebar_hover))
            .cursor_pointer()
            .flex()
            .items_center()
            .gap(px(12.))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_plugin_detail(id.clone(), cx);
                this.content_scroll.set_offset(point(px(0.), px(0.)));
                cx.stop_propagation();
            }))
            .child(self.plugin_icon(plugin, &theme, index))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .truncate()
                            .child(plugin.display_name().to_owned()),
                    )
                    .when_some(description, |v, description| {
                        v.child(
                            div()
                                .text_size(px(12.))
                                .text_color(theme.text_secondary)
                                .truncate()
                                .child(description),
                        )
                    }),
            )
            .child(self.directory_action(row, theme, true, cx))
            .into_any_element()
    }

    fn directory_feedback(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let directory = &self.plugins_catalog.directory;
        let mut feedback = div().w_full().flex().flex_col().gap(px(10.));
        if let Some(confirmation) = &self.plugins_catalog.confirmation {
            feedback = feedback.child(self.confirmation_card(confirmation, &theme, cx));
        }
        if directory.busy() {
            feedback = feedback.child(self.catalog_state_card(
                crate::i18n::text("正在处理插件操作…"),
                None,
                &theme,
            ));
        }
        for operation in directory.operations() {
            if let Some(message) = operation.phase.message() {
                feedback = feedback.child(self.catalog_state_card(message, None, &theme));
            }
        }
        for error in [
            directory.error.as_ref(),
            directory.installed_error.as_ref(),
            directory.search_error.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            feedback = feedback.child(self.catalog_state_card(
                &error.user_message(),
                Some(&error.message),
                &theme,
            ));
        }
        if self.directory_scope == Scope::All {
            feedback = feedback.child(self.directory_source_status(theme));
        }
        if let Some(catalog) = &directory.catalog {
            for error in &catalog.marketplace_load_errors {
                feedback = feedback.child(self.catalog_state_card(
                    &error.message,
                    Some(&error.marketplace_path.display().to_string()),
                    &theme,
                ));
            }
        }
        feedback.into_any_element()
    }

    fn directory_catalog(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let directory = &self.plugins_catalog.directory;
        let mut body = div().w_full().flex().flex_col().gap(px(24.));
        let pending = match self.directory_scope {
            Scope::All => directory.loading,
            Scope::Installed => directory.installed_loading,
        };
        if pending {
            body = body.child(self.catalog_state_card(
                crate::i18n::text("正在读取插件目录…"),
                None,
                &theme,
            ));
        }
        let rows = self.directory_rows();
        if rows.is_empty()
            && !pending
            && (self.directory_scope == Scope::Installed
                || !directory.source_failed()
                || !directory.search_term.trim().is_empty())
            && directory.error.is_none()
            && directory.search_error.is_none()
            && !(self.directory_scope == Scope::Installed && directory.installed_error.is_some())
        {
            body = body.child(self.catalog_state_card(
                &if !directory.search_term.trim().is_empty() {
                    crate::i18n::format!("已加载的插件中没有匹配结果" => "No matches in the loaded plugins")
                } else { crate::i18n::format!("未找到插件" => "No plugins found") },
                None,
                &theme,
            ));
        }
        let mut groups: BTreeMap<String, Vec<(usize, PluginRow<'_>)>> = BTreeMap::new();
        for (index, row) in rows.into_iter().enumerate() {
            let category = row.marketplace_name.to_owned();
            groups.entry(category).or_default().push((index, row));
        }
        for (category, rows) in groups {
            let entries = rows.into_iter().map(|(index, row)| {
                div()
                    .w(gpui::relative(0.5))
                    .min_w(px(250.))
                    .child(self.directory_entry(row, index, theme, cx))
            });
            body = body.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(category),
                    )
                    .child(div().w_full().flex().flex_wrap().children(entries)),
            );
        }
        body.into_any_element()
    }

    fn directory_detail(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let directory = &self.plugins_catalog.directory;
        let mut body = div().w_full().flex().flex_col().gap(px(24.));
        if let Some(detail) = &directory.detail {
            let plugin = &detail.summary;
            let row = PluginRow {
                plugin,
                marketplace_name: &detail.marketplace_name,
                marketplace_path: detail.marketplace_path.as_deref(),
            };
            let interface = plugin.interface.as_ref();
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(self.plugin_icon(plugin, &theme, 0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .child(
                                div()
                                    .text_size(px(24.))
                                    .child(plugin.display_name().to_owned()),
                            )
                            .when_some(
                                plugin.description().map(str::to_owned),
                                |v, description| {
                                    v.child(
                                        div()
                                            .text_size(px(14.))
                                            .text_color(theme.text_secondary)
                                            .child(description),
                                    )
                                },
                            ),
                    )
                    .child(self.directory_action(row, theme, false, cx)),
            );
            let description = interface
                .and_then(|i| i.long_description.as_deref())
                .or(detail.description.as_deref());
            if let Some(description) = description {
                body = body.child(render_pull_request_markdown(
                    description,
                    theme,
                    "plugin-directory-description",
                ));
            }
            if let Some(interface) = interface {
                let mut info = div().flex().flex_col().gap(px(10.));
                for (label, value) in [
                    (
                        crate::i18n::format!("开发者" => "Developer"),
                        interface.developer_name.clone(),
                    ),
                    (
                        crate::i18n::format!("类别" => "Category"),
                        interface.category.clone(),
                    ),
                    (
                        crate::i18n::format!("版本" => "Version"),
                        plugin.version.clone(),
                    ),
                    (
                        crate::i18n::format!("功能" => "Capabilities"),
                        (!interface.capabilities.is_empty())
                            .then(|| interface.capabilities.join(", ")),
                    ),
                ] {
                    if let Some(value) = value {
                        info = info.child(
                            div()
                                .text_size(px(13.))
                                .text_color(theme.text_secondary)
                                .child(format!("{label} · {value}")),
                        );
                    }
                }
                let mut links = div().flex().flex_wrap().gap(px(16.));
                for (index, (label, url)) in [
                    (
                        crate::i18n::format!("网站" => "Website"),
                        interface.website_url.as_ref(),
                    ),
                    (
                        crate::i18n::format!("隐私政策" => "Privacy policy"),
                        interface.privacy_policy_url.as_ref(),
                    ),
                    (
                        crate::i18n::format!("服务条款" => "Terms of service"),
                        interface.terms_of_service_url.as_ref(),
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    if let Some(url) =
                        url.filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                    {
                        let url = url.clone();
                        links = links.child(
                            div()
                                .id(("directory-plugin-link", index))
                                .role(Role::Link)
                                .aria_label(label.clone())
                                .focusable()
                                .tab_stop(true)
                                .cursor_pointer()
                                .text_size(px(13.))
                                .text_color(theme.accent)
                                .on_click(move |_, _, cx| cx.open_url(&url))
                                .child(label),
                        );
                    }
                }
                body = body.child(info.child(links));
                let images = interface
                    .screenshots
                    .iter()
                    .filter_map(|path| path.to_str().map(str::to_owned))
                    .chain(interface.screenshot_urls.clone())
                    .enumerate()
                    .map(|(index, source)| {
                        div()
                            .id(("directory-plugin-screenshot", index))
                            .h(px(260.))
                            .w_full()
                            .overflow_hidden()
                            .rounded(px(12.))
                            .child(
                                gpui::img(source)
                                    .size_full()
                                    .object_fit(gpui::ObjectFit::Contain),
                            )
                    });
                body = body.children(images);
            }
            // Skills, MCP servers and share operations use the same controller
            // and confirmation handling as Settings, rather than a second RPC path.
            body = body.child(self.plugin_detail_card(detail, &theme, cx));
        } else if let Some(error) = &directory.detail_error {
            body = body.child(self.catalog_state_card(
                &error.user_message(),
                Some(&error.message),
                &theme,
            ));
        } else {
            body = body.child(self.catalog_state_card(
                &crate::i18n::format!("正在读取插件详情…" => "Loading plugin details…"),
                None,
                &theme,
            ));
        }
        body.into_any_element()
    }

    pub(super) fn plugin_directory_page(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let skills = self.plugins_segment == PluginSegment::Skills;
        let detail = self.plugins_catalog.open_plugin.is_some() && !skills;
        let mut toolbar = div()
            .h(px(46.))
            .flex_none()
            .pl(px(self.directory_titlebar_inset + 12.))
            .pr(px(16.))
            .flex()
            .items_center()
            .gap(px(4.));
        if detail {
            toolbar = toolbar.child(
                div()
                    .id("directory-back")
                    .role(Role::Button)
                    .aria_label(crate::i18n::text("返回"))
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .h(px(32.))
                    .px(px(10.))
                    .rounded(px(10.))
                    .hover(|s| s.bg(theme.sidebar_hover))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon("back", theme.text.into()).size(px(16.)))
                    .child(crate::i18n::text("插件"))
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss_plugin_directory(cx))),
            );
        } else {
            for (index, (label, segment)) in [
                (crate::i18n::text("插件"), PluginSegment::Plugins),
                (crate::i18n::text("技能"), PluginSegment::Skills),
            ]
            .into_iter()
            .enumerate()
            {
                toolbar = toolbar.child(
                    div()
                        .id(("directory-tab", index))
                        .role(Role::Tab)
                        .aria_label(label)
                        .aria_selected(self.plugins_segment == segment)
                        .focusable()
                        .tab_stop(true)
                        .cursor_pointer()
                        .h(px(36.))
                        .px(px(14.))
                        .rounded(px(12.))
                        .when(self.plugins_segment == segment, |v| {
                            v.bg(theme.sidebar_hover)
                        })
                        .flex()
                        .items_center()
                        .text_size(px(14.))
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.clear_directory_navigation(cx);
                            this.select_plugins_segment(segment, cx);
                            if segment == PluginSegment::Skills {
                                this.plugin_search_input.update(cx, |input, cx| {
                                    input.set_placeholder(
                                        crate::i18n::format!("搜索技能" => "Search skills"),
                                        cx,
                                    );
                                    input.set_accessible_name(
                                        crate::i18n::format!("搜索技能" => "Search skills"),
                                    );
                                });
                            }
                        })),
                );
            }
        }
        toolbar = toolbar
            .child(div().flex_1())
            .child(
                div()
                    .id("directory-refresh")
                    .role(Role::Button)
                    .aria_label(crate::i18n::text("刷新"))
                    .focusable()
                    .tab_stop(true)
                    .size(px(28.))
                    .rounded(px(10.))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.sidebar_hover))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon("settings-refresh", theme.text_secondary.into()).size(px(16.)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.plugins_segment == PluginSegment::Skills {
                            this.refresh_skills(true, cx);
                        } else {
                            this.refresh_plugins(true, cx);
                        }
                    })),
            )
            .when(!skills, |v| {
                v.child(
                    div()
                        .id("directory-add")
                        .role(Role::Button)
                        .aria_label(crate::i18n::text("添加"))
                        .focusable()
                        .tab_stop(true)
                        .h(px(28.))
                        .px(px(14.))
                        .rounded(px(10.))
                        .cursor_pointer()
                        .bg(theme.settings_button)
                        .flex()
                        .items_center()
                        .child(crate::i18n::text("添加"))
                        .on_click(cx.listener(|this, _, _, cx| this.open_marketplace_add(cx))),
                )
            });

        let mut content = div()
            .w_full()
            .max_w(px(960.))
            .mx_auto()
            .px(px(20.))
            .pt(px(20.))
            .pb(px(64.))
            .flex()
            .flex_col()
            .gap(px(24.));
        if !detail {
            content = content.child(div().flex().flex_col().gap(px(8.))
                .child(div().text_size(px(24.)).child(if skills { crate::i18n::text("技能") } else { crate::i18n::text("插件") }))
                .child(div().text_size(px(14.)).text_color(theme.text_secondary).child(if skills {
                    crate::i18n::format!("管理 coding agent 可用的技能" => "Manage the skills available to your coding agent")
                } else { crate::i18n::format!("连接插件，让 Codex 在你的各类工具中协同工作" => "Connect plugins to work across your tools with Codex") })))
                .child(div().w_full().h(px(40.)).px(px(12.)).rounded(px(12.)).border_1().border_color(theme.border)
                    .flex().items_center().gap(px(8.)).child(icon("search", theme.text_secondary.into()).size(px(16.)))
                    .child(self.plugin_search_input.clone()));
            if !skills {
                content =
                    content.child(div().flex().gap(px(4.)).children(
                        [Scope::All, Scope::Installed].into_iter().enumerate().map(
                            |(index, scope)| {
                                div()
                                    .id(("directory-scope", index))
                                    .role(Role::Tab)
                                    .aria_label(scope.label())
                                    .aria_selected(self.directory_scope == scope)
                                    .focusable()
                                    .tab_stop(true)
                                    .h(px(36.))
                                    .px(px(14.))
                                    .rounded(px(12.))
                                    .cursor_pointer()
                                    .when(self.directory_scope == scope, |v| {
                                        v.bg(theme.sidebar_hover)
                                    })
                                    .flex()
                                    .items_center()
                                    .text_size(px(14.))
                                    .child(scope.label())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.select_directory_scope(scope, cx)
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, event: &gpui::KeyDownEvent, _, cx| {
                                            let target = match event.keystroke.key.as_str() {
                                                "enter" | "space" => Some(scope),
                                                _ => None,
                                            };
                                            if let Some(target) = target {
                                                this.select_directory_scope(target, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                            },
                        ),
                    ));
            }
        }
        content = content.child(self.directory_feedback(theme, cx));
        content = content.child(if skills {
            self.skills_segment_content(&theme, cx)
        } else if detail {
            self.directory_detail(theme, cx)
        } else {
            self.directory_catalog(theme, cx)
        });
        div()
            .id("plugin-directory-page")
            .debug_selector(|| "plugin-directory-page".into())
            .size_full()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .font_family(UI_FONT_FAMILY)
            .text_color(theme.text)
            .bg(theme.surface)
            .on_key_down(cx.listener(|_, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab" {
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                }
            }))
            .child(toolbar)
            .child(
                div()
                    .id("plugin-directory-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .track_scroll(&self.content_scroll)
                    .child(content),
            )
            .children(self.render_toasts(cx))
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "plugin_directory_tests.rs"]
mod tests;
