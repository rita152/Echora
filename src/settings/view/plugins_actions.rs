//! Backend driving for the plugins and apps segments: reads, the confirmation
//! gate in front of every state change, and the outcome handling that keeps a
//! failed intent available for an explicit retry.

use gpui::Context;

use super::plugins_catalog::PluginConfirmation;
use super::{PluginSegment, SettingsView};
use crate::{
    agent::{
        AgentAppsInstalledRequest, AgentAppsListRequest, AgentAppsReadRequest,
        AgentMarketplaceAddRequest, AgentMarketplaceRemoveRequest, AgentMarketplaceUpgradeRequest,
        AgentPluginCatalogRequest, AgentPluginInstallRequest, AgentPluginInstalledRequest,
        AgentPluginReadRequest, AgentPluginSearchRequest, AgentPluginSkillReadRequest,
        AgentPluginUninstallRequest,
    },
    plugins::{PluginIntent, PluginOperationPhase},
};

impl SettingsView {
    /// Reads the plugin catalog and the installed subset. Both answers belong to
    /// the same cycle, so a superseded read cannot overwrite a newer one.
    pub(super) fn refresh_plugins(&mut self, force_refetch: bool, cx: &mut Context<Self>) {
        let generation = self.plugins_generation;
        let cycle = self.plugins_catalog.directory.begin_load(generation);
        let reader = self.backend.load_plugin_catalog(AgentPluginCatalogRequest {
            cwds: self.plugins_cwd.clone().map(|cwd| vec![cwd]),
            force_refetch,
            // Browsing the directory asks for every marketplace kind the account
            // can see; an ordinary load keeps the server's own default set.
            marketplace_kinds: force_refetch.then(|| {
                vec![
                    crate::agent::AgentPluginMarketplaceKind::Local,
                    crate::agent::AgentPluginMarketplaceKind::Vertical,
                    crate::agent::AgentPluginMarketplaceKind::WorkspaceDirectory,
                    crate::agent::AgentPluginMarketplaceKind::SharedWithMe,
                    crate::agent::AgentPluginMarketplaceKind::CreatedByMeRemote,
                ]
            }),
        });
        let installed_reader = self
            .backend
            .load_installed_plugins(AgentPluginInstalledRequest {
                cwds: self.plugins_cwd.clone().map(|cwd| vec![cwd]),
                install_suggestion_plugin_names: None,
            });
        cx.spawn(async move |this, cx| {
            let catalog = reader.recv().await;
            let installed = installed_reader.recv().await;
            let _ = this.update(cx, |this, cx| {
                match catalog {
                    Ok(Ok(catalog)) => {
                        if this.plugins_catalog.directory.apply_catalog(cycle, catalog) {
                            this.plugins_catalog.directory.clear_settled_operations();
                        }
                    }
                    Ok(Err(error)) => {
                        this.plugins_catalog.directory.apply_error(cycle, error);
                    }
                    Err(_) => {
                        this.plugins_catalog
                            .directory
                            .apply_error(cycle, disconnected_plugins_error());
                    }
                }
                if let Ok(Ok(installed)) = installed {
                    this.plugins_catalog
                        .directory
                        .apply_installed(cycle, installed);
                }
                cx.notify();
            });
        })
        .detach();
        // Startup reconcile: once per connection generation, the server is
        // asked to bring installed plugins in line with configuration, and its
        // receipt is the only evidence of what changed.
        if self.plugins_catalog.directory.reconciled_generation != Some(generation) {
            self.plugins_catalog.directory.reconciled_generation = Some(generation);
            let reader =
                self.backend
                    .reconcile_plugins(crate::agent::AgentPluginReconcileRequest {
                        reason: Some("settings-open".to_owned()),
                    });
            cx.spawn(async move |this, cx| {
                let result = reader.recv().await;
                let _ = this.update(cx, |this, cx| {
                    if let Ok(Ok(receipt)) = result {
                        this.plugins_catalog.directory.reconcile = Some(receipt);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    /// Reads the app directory and the installed connector snapshot.
    pub(super) fn refresh_apps(&mut self, cx: &mut Context<Self>) {
        let generation = self.apps_generation;
        let cycle = self.apps.directory.begin_load(generation);
        let page_reader = self.backend.load_apps(AgentAppsListRequest {
            cursor: None,
            limit: None,
            force_refetch: false,
            thread_id: None,
        });
        let installed_reader = self
            .backend
            .load_installed_apps(AgentAppsInstalledRequest::default());
        cx.spawn(async move |this, cx| {
            let page = page_reader.recv().await;
            let installed = installed_reader.recv().await;
            let _ = this.update(cx, |this, cx| {
                match page {
                    Ok(Ok(page)) => {
                        this.apps.directory.apply_page(cycle, page);
                    }
                    Ok(Err(error)) => {
                        this.apps.directory.apply_error(cycle, error);
                    }
                    Err(_) => {
                        this.apps
                            .directory
                            .apply_error(cycle, disconnected_apps_error());
                    }
                }
                if let Ok(Ok(installed)) = installed {
                    this.apps.directory.apply_installed(cycle, installed);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Loads one plugin's detail through `plugin/read`.
    pub(super) fn open_plugin_detail(&mut self, plugin_id: String, cx: &mut Context<Self>) {
        let Some(row) = self
            .plugins_catalog
            .directory
            .visible_plugins()
            .into_iter()
            .find(|row| row.plugin.id == plugin_id)
            .map(|row| {
                (
                    row.plugin.name.clone(),
                    row.marketplace_path.map(str::to_owned),
                )
            })
        else {
            return;
        };
        let (plugin_name, marketplace_path) = row;
        self.plugins_catalog.open_plugin = Some(plugin_id);
        let reader = self.backend.read_plugin(AgentPluginReadRequest {
            marketplace_path,
            remote_marketplace_name: None,
            plugin_name,
        });
        cx.spawn(async move |this, cx| {
            let result = reader.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(detail)) => {
                        this.plugins_catalog.directory.detail = Some(detail);
                        this.plugins_catalog.directory.detail_error = None;
                    }
                    Ok(Err(error)) => {
                        this.plugins_catalog.directory.detail_error = Some(error);
                    }
                    Err(_) => {
                        this.plugins_catalog.directory.detail_error =
                            Some(disconnected_plugins_error());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Loads one app's metadata through `app/read`.
    pub(super) fn apps_open_detail(&mut self, app_id: String, cx: &mut Context<Self>) {
        self.apps.directory.begin_detail(&app_id);
        let reader = self.backend.read_apps(AgentAppsReadRequest {
            app_ids: vec![app_id.clone()],
            include_tools: true,
            thread_id: None,
        });
        cx.spawn(async move |this, cx| {
            let result = reader.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(result)) => {
                        this.apps.directory.apply_detail(&app_id, result);
                    }
                    Ok(Err(error)) => {
                        this.apps.directory.apply_detail_error(&app_id, error);
                    }
                    Err(_) => {
                        this.apps
                            .directory
                            .apply_detail_error(&app_id, disconnected_apps_error());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn dismiss_catalog_confirmation(&mut self, cx: &mut Context<Self>) {
        if self.plugins_catalog.confirmation.take().is_some() {
            cx.notify();
        }
    }

    /// Runs the confirmed operation. The intent is tracked by its own sequence
    /// so a result that arrives after a newer attempt is ignored.
    pub(super) fn confirm_plugin_operation(&mut self, cx: &mut Context<Self>) {
        let Some(confirmation) = self.plugins_catalog.confirmation.take() else {
            return;
        };
        let generation = self.plugins_generation;
        match confirmation {
            PluginConfirmation::Install {
                plugin_name,
                marketplace_path,
                ..
            } => {
                let intent = PluginIntent::Install {
                    plugin_name: plugin_name.clone(),
                    marketplace_path: marketplace_path.clone(),
                    remote_marketplace_name: None,
                };
                let sequence = self
                    .plugins_catalog
                    .directory
                    .start_operation(intent.clone());
                let reader = self.backend.install_plugin(AgentPluginInstallRequest {
                    generation,
                    plugin_name,
                    marketplace_path,
                    remote_marketplace_name: None,
                    install_attempt_id: None,
                });
                cx.spawn(async move |this, cx| {
                    let result = reader.recv().await;
                    let _ = this.update(cx, |this, cx| {
                        let phase = match result {
                            Ok(result) => {
                                SettingsView::operation_phase(&result.outcome, "已安装插件")
                            }
                            Err(_) => PluginOperationPhase::Unknown {
                                message: "安装连接在返回结果前关闭".to_owned(),
                            },
                        };
                        let target = intent.target();
                        let settled = matches!(
                            phase,
                            PluginOperationPhase::Succeeded { .. }
                                | PluginOperationPhase::Failed { .. }
                        );
                        this.plugins_catalog
                            .directory
                            .apply_operation(&target, sequence, phase);
                        cx.notify();
                        if settled {
                            this.refresh_plugins(false, cx);
                        }
                    });
                })
                .detach();
            }
            PluginConfirmation::Uninstall {
                plugin_id,
                display_name: _,
            } => {
                let intent = PluginIntent::Uninstall {
                    plugin_id: plugin_id.clone(),
                };
                let sequence = self
                    .plugins_catalog
                    .directory
                    .start_operation(intent.clone());
                let reader = self.backend.uninstall_plugin(AgentPluginUninstallRequest {
                    generation,
                    plugin_id,
                });
                cx.spawn(async move |this, cx| {
                    let result = reader.recv().await;
                    let _ = this.update(cx, |this, cx| {
                        let phase = match result {
                            Ok(result) => {
                                SettingsView::operation_phase(&result.outcome, "已卸载插件")
                            }
                            Err(_) => PluginOperationPhase::Unknown {
                                message: "卸载连接在返回结果前关闭".to_owned(),
                            },
                        };
                        let settled = matches!(
                            phase,
                            PluginOperationPhase::Succeeded { .. }
                                | PluginOperationPhase::Failed { .. }
                        );
                        let target = intent.target();
                        this.plugins_catalog
                            .directory
                            .apply_operation(&target, sequence, phase);
                        cx.notify();
                        if settled {
                            this.refresh_plugins(false, cx);
                        }
                    });
                })
                .detach();
            }
            PluginConfirmation::MarketplaceUpgrade { marketplace_name } => {
                let intent = PluginIntent::MarketplaceUpgrade {
                    marketplace_name: marketplace_name.clone(),
                };
                let sequence = self
                    .plugins_catalog
                    .directory
                    .start_operation(intent.clone());
                let reader = self
                    .backend
                    .upgrade_marketplaces(AgentMarketplaceUpgradeRequest {
                        generation,
                        marketplace_name,
                    });
                cx.spawn(async move |this, cx| {
                    let result = reader.recv().await;
                    let _ = this.update(cx, |this, cx| {
                        let phase = match result {
                            Ok(result) => {
                                SettingsView::operation_phase(&result.outcome, "已更新插件目录")
                            }
                            Err(_) => PluginOperationPhase::Unknown {
                                message: "更新连接在返回结果前关闭".to_owned(),
                            },
                        };
                        let settled = matches!(
                            phase,
                            PluginOperationPhase::Succeeded { .. }
                                | PluginOperationPhase::Failed { .. }
                        );
                        let target = intent.target();
                        this.plugins_catalog
                            .directory
                            .apply_operation(&target, sequence, phase);
                        cx.notify();
                        if settled {
                            this.refresh_plugins(true, cx);
                        }
                    });
                })
                .detach();
            }
            PluginConfirmation::MarketplaceRemove { marketplace_name } => {
                let intent = PluginIntent::MarketplaceRemove {
                    marketplace_name: marketplace_name.clone(),
                };
                let sequence = self
                    .plugins_catalog
                    .directory
                    .start_operation(intent.clone());
                let reader = self
                    .backend
                    .remove_marketplace(AgentMarketplaceRemoveRequest {
                        generation,
                        marketplace_name,
                    });
                cx.spawn(async move |this, cx| {
                    let result = reader.recv().await;
                    let _ = this.update(cx, |this, cx| {
                        let phase = match result {
                            Ok(result) => {
                                SettingsView::operation_phase(&result.outcome, "已移除 marketplace")
                            }
                            Err(_) => PluginOperationPhase::Unknown {
                                message: "移除连接在返回结果前关闭".to_owned(),
                            },
                        };
                        let settled = matches!(
                            phase,
                            PluginOperationPhase::Succeeded { .. }
                                | PluginOperationPhase::Failed { .. }
                        );
                        let target = intent.target();
                        this.plugins_catalog
                            .directory
                            .apply_operation(&target, sequence, phase);
                        cx.notify();
                        if settled {
                            this.refresh_plugins(true, cx);
                        }
                    });
                })
                .detach();
            }
            PluginConfirmation::MarketplaceAdd { source } => {
                let intent = PluginIntent::MarketplaceAdd {
                    source: source.clone(),
                };
                let sequence = self
                    .plugins_catalog
                    .directory
                    .start_operation(intent.clone());
                let reader = self.backend.add_marketplace(AgentMarketplaceAddRequest {
                    generation,
                    source,
                    ref_name: None,
                    sparse_paths: None,
                });
                cx.spawn(async move |this, cx| {
                    let result = reader.recv().await;
                    let _ = this.update(cx, |this, cx| {
                        let phase = match result {
                            Ok(result) => {
                                SettingsView::operation_phase(&result.outcome, "已添加 marketplace")
                            }
                            Err(_) => PluginOperationPhase::Unknown {
                                message: "添加连接在返回结果前关闭".to_owned(),
                            },
                        };
                        let settled = matches!(
                            phase,
                            PluginOperationPhase::Succeeded { .. }
                                | PluginOperationPhase::Failed { .. }
                        );
                        let target = intent.target();
                        this.plugins_catalog
                            .directory
                            .apply_operation(&target, sequence, phase);
                        cx.notify();
                        if settled {
                            this.refresh_plugins(true, cx);
                        }
                    });
                })
                .detach();
            }
            PluginConfirmation::ShareSave {
                plugin_path,
                remote_plugin_id,
                ..
            } => {
                // `plugin/share/save` addresses a local package by path; a
                // plugin the server only knows remotely has none, and the call
                // is refused locally with the server's own reason shape.
                let Some(plugin_path) = plugin_path.map(std::path::PathBuf::from) else {
                    self.plugins_catalog.directory.shares_error =
                        Some(crate::agent::AgentPluginsError {
                            kind: crate::agent::AgentPluginsErrorKind::Protocol,
                            message: "该插件没有本地路径，plugin/share/save 无法定位它".to_owned(),
                            data: None,
                            outcome_unknown: false,
                        });
                    cx.notify();
                    return;
                };
                let reader =
                    self.backend
                        .save_plugin_share(crate::agent::AgentPluginShareSaveRequest {
                            generation,
                            plugin_path,
                            remote_plugin_id,
                            discoverability: None,
                            share_targets: None,
                        });
                cx.spawn(async move |this, cx| {
                    let result = reader.recv().await;
                    let _ = this.update(cx, |this, cx| {
                        this.plugins_catalog.directory.shares_error = match result {
                            Ok(result) => match result.outcome.succeeded() {
                                Some(receipt) => {
                                    this.plugins_catalog.directory.share_url =
                                        Some(receipt.share_url.clone());
                                    None
                                }
                                None => Some(crate::agent::AgentPluginsError {
                                    kind: crate::agent::AgentPluginsErrorKind::Protocol,
                                    message: result.outcome.user_message("已共享插件"),
                                    data: None,
                                    outcome_unknown: result.outcome.outcome_unknown(),
                                }),
                            },
                            Err(_) => Some(crate::agent::AgentPluginsError {
                                kind: crate::agent::AgentPluginsErrorKind::Connection,
                                message: "共享请求连接在返回结果前关闭".to_owned(),
                                data: None,
                                outcome_unknown: false,
                            }),
                        };
                        cx.notify();
                    });
                })
                .detach();
            }
            PluginConfirmation::ShareUpdateTargets {
                remote_plugin_id,
                discoverability,
                targets,
                blocked,
            } => {
                if let Some(reason) = blocked {
                    self.plugins_catalog.directory.shares_error =
                        Some(crate::agent::AgentPluginsError {
                            kind: crate::agent::AgentPluginsErrorKind::Protocol,
                            message: reason,
                            data: None,
                            outcome_unknown: false,
                        });
                    cx.notify();
                    return;
                }
                let reader = self.backend.update_plugin_share_targets(
                    crate::agent::AgentPluginShareUpdateTargetsRequest {
                        generation,
                        remote_plugin_id,
                        discoverability,
                        share_targets: targets,
                    },
                );
                cx.spawn(async move |this, cx| {
                    let result = reader.recv().await;
                    let _ = this.update(cx, |this, cx| {
                        this.plugins_catalog.directory.shares_error = match result {
                            Ok(result) => match result.outcome {
                                crate::agent::AgentPluginOperationOutcome::Succeeded(_) => None,
                                other => Some(crate::agent::AgentPluginsError {
                                    kind: crate::agent::AgentPluginsErrorKind::Protocol,
                                    message: other.user_message("已更新共享范围"),
                                    data: None,
                                    outcome_unknown: other.outcome_unknown(),
                                }),
                            },
                            Err(_) => Some(crate::agent::AgentPluginsError {
                                kind: crate::agent::AgentPluginsErrorKind::Connection,
                                message: "共享请求连接在返回结果前关闭".to_owned(),
                                data: None,
                                outcome_unknown: false,
                            }),
                        };
                        cx.notify();
                    });
                })
                .detach();
            }
            PluginConfirmation::ShareDelete { remote_plugin_id } => {
                let reader =
                    self.backend
                        .delete_plugin_share(crate::agent::AgentPluginShareDeleteRequest {
                            generation,
                            remote_plugin_id,
                        });
                cx.spawn(async move |this, cx| {
                    let result = reader.recv().await;
                    let _ = this.update(cx, |this, cx| {
                        this.plugins_catalog.directory.shares_error = match result {
                            Ok(result) => match result.outcome {
                                crate::agent::AgentPluginOperationOutcome::Succeeded(_) => None,
                                other => Some(crate::agent::AgentPluginsError {
                                    kind: crate::agent::AgentPluginsErrorKind::Protocol,
                                    message: other.user_message("已取消共享"),
                                    data: None,
                                    outcome_unknown: other.outcome_unknown(),
                                }),
                            },
                            Err(_) => Some(crate::agent::AgentPluginsError {
                                kind: crate::agent::AgentPluginsErrorKind::Connection,
                                message: "共享请求连接在返回结果前关闭".to_owned(),
                                data: None,
                                outcome_unknown: false,
                            }),
                        };
                        cx.notify();
                    });
                })
                .detach();
            }
        }
    }
}

impl SettingsView {
    /// Runs a search with the current term. An empty term returns to the full
    /// catalog instead of querying for nothing.
    pub(super) fn submit_plugin_search(&mut self, cx: &mut Context<Self>) {
        let term = self.plugins_catalog.directory.search_term.clone();
        if term.trim().is_empty() {
            self.plugins_catalog.directory.clear_search();
            cx.notify();
            return;
        }
        self.plugins_catalog.directory.searching = true;
        let reader = self.backend.search_plugins(AgentPluginSearchRequest {
            cursor: None,
            limit: None,
            // The directory search runs across every marketplace the account
            // can see, which is what the global scope means.
            scope: Some("global"),
            search_term: term.clone(),
            cwds: self.plugins_cwd.clone().map(|cwd| vec![cwd]),
        });
        cx.spawn(async move |this, cx| {
            let result = reader.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(page)) => {
                        this.plugins_catalog.directory.apply_search(page);
                    }
                    Ok(Err(error)) => {
                        this.plugins_catalog
                            .directory
                            .apply_search_error(&term, error);
                    }
                    Err(_) => {
                        this.plugins_catalog
                            .directory
                            .apply_search_error(&term, disconnected_plugins_error());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Opens the update confirmation for one marketplace.
    pub(super) fn confirm_marketplace_upgrade(
        &mut self,
        marketplace_name: String,
        cx: &mut Context<Self>,
    ) {
        self.plugins_catalog.confirmation = Some(PluginConfirmation::MarketplaceUpgrade {
            marketplace_name: Some(marketplace_name),
        });
        cx.notify();
    }

    /// Opens the add sheet with a submitted source. The source is exactly the
    /// text the user typed; nothing is derived from it.
    pub(super) fn confirm_marketplace_add(&mut self, source: String, cx: &mut Context<Self>) {
        let source = source.trim().to_owned();
        if source.is_empty() {
            return;
        }
        self.plugins_catalog.confirmation = Some(PluginConfirmation::MarketplaceAdd { source });
        cx.notify();
    }

    /// Opens the marketplace add sheet: the header action the reference labels
    /// 添加.
    pub(super) fn open_marketplace_add(&mut self, cx: &mut Context<Self>) {
        self.plugins_catalog.confirmation = Some(PluginConfirmation::MarketplaceAdd {
            source: self.marketplace_source_input.read(cx).text().to_owned(),
        });
        cx.notify();
    }

    /// Applying an `app/list/updated` notification never replaces what is on
    /// screen: it marks the cache stale and re-reads only while the apps segment
    /// is visible.
    pub(super) fn apply_app_list_updated(&mut self, cx: &mut Context<Self>) {
        self.apps.directory.mark_stale();
        if self.plugins_segment == PluginSegment::Apps {
            self.refresh_apps(cx);
        }
        cx.notify();
    }

    /// Opens the plugin detail read for a row that is currently visible.
    pub(super) fn open_visible_plugin(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(plugin_id) = self
            .plugins_catalog
            .directory
            .visible_plugins()
            .get(index)
            .map(|row| row.plugin.id.clone())
        else {
            return;
        };
        self.open_plugin_detail(plugin_id, cx);
    }

    /// Reads one skill's contents. The protocol serves this for shared (remote)
    /// plugins only, so a plugin without a remote id is answered locally with
    /// the reason instead of a request that cannot succeed.
    pub(super) fn open_plugin_skill(
        &mut self,
        marketplace_name: String,
        remote_plugin_id: Option<String>,
        skill_name: String,
        cx: &mut Context<Self>,
    ) {
        let key = format!("{marketplace_name}:{skill_name}");
        let Some(remote_plugin_id) = remote_plugin_id else {
            self.plugins_catalog.directory.begin_skill(key.clone());
            self.plugins_catalog.directory.apply_skill_error(
                &key,
                crate::agent::AgentPluginsError {
                    kind: crate::agent::AgentPluginsErrorKind::Protocol,
                    message: "该插件没有远端 id，服务端不提供技能内容".to_owned(),
                    data: None,
                    outcome_unknown: false,
                },
            );
            cx.notify();
            return;
        };
        self.plugins_catalog.directory.begin_skill(key.clone());
        let reader = self.backend.read_plugin_skill(AgentPluginSkillReadRequest {
            remote_marketplace_name: marketplace_name,
            remote_plugin_id,
            skill_name,
        });
        cx.spawn(async move |this, cx| {
            let result = reader.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(content)) => {
                        this.plugins_catalog.directory.apply_skill(&key, content);
                    }
                    Ok(Err(error)) => {
                        this.plugins_catalog
                            .directory
                            .apply_skill_error(&key, error);
                    }
                    Err(_) => {
                        this.plugins_catalog
                            .directory
                            .apply_skill_error(&key, disconnected_plugins_error());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Reads the account's plugin shares through `plugin/share/list`.
    pub(super) fn refresh_plugin_shares(&mut self, cx: &mut Context<Self>) {
        self.plugins_catalog.directory.begin_shares();
        let reader = self.backend.plugin_share_list();
        cx.spawn(async move |this, cx| {
            let result = reader.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(shares)) => {
                        this.plugins_catalog.directory.apply_shares(shares);
                    }
                    Ok(Err(error)) => {
                        this.plugins_catalog.directory.apply_shares_error(error);
                    }
                    Err(_) => {
                        this.plugins_catalog
                            .directory
                            .apply_shares_error(disconnected_plugins_error());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn disconnected_plugins_error() -> crate::agent::AgentPluginsError {
    crate::agent::AgentPluginsError {
        kind: crate::agent::AgentPluginsErrorKind::Connection,
        message: "插件请求连接在返回结果前关闭".to_owned(),
        data: None,
        outcome_unknown: false,
    }
}

fn disconnected_apps_error() -> crate::agent::AgentAppsError {
    crate::agent::AgentAppsError {
        kind: crate::agent::AgentAppsErrorKind::Connection,
        message: "应用请求连接在返回结果前关闭".to_owned(),
        data: None,
        outcome_unknown: false,
    }
}
