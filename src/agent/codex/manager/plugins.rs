//! Plugin directory reads, install lifecycle, sharing, and marketplace
//! operations. Every state-changing call reports its own outcome: a timeout or
//! an unreadable answer is never folded into a failure, because neither may be
//! retried automatically.

use std::sync::Arc;

use async_channel::Receiver;
use serde_json::Value;

use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{
    AgentMarketplaceAddRequest, AgentMarketplaceAddResult, AgentMarketplaceRemoveRequest,
    AgentMarketplaceRemoveResult, AgentMarketplaceUpgradeRequest, AgentMarketplaceUpgradeResult,
    AgentPluginCatalog, AgentPluginCatalogRequest, AgentPluginDetail, AgentPluginInstallReceipt,
    AgentPluginInstallRequest, AgentPluginInstallResult, AgentPluginInstalledRequest,
    AgentPluginOperationOutcome, AgentPluginReadRequest, AgentPluginReconcileReceipt,
    AgentPluginReconcileRequest, AgentPluginSearchPage, AgentPluginSearchRequest,
    AgentPluginShareDeleteRequest, AgentPluginShareDeleteResult, AgentPluginShareList,
    AgentPluginShareSaveRequest, AgentPluginShareSaveResult, AgentPluginShareUpdateTargetsRequest,
    AgentPluginShareUpdateTargetsResult, AgentPluginSkillContent, AgentPluginSkillReadRequest,
    AgentPluginUninstallRequest, AgentPluginUninstallResult, AgentPluginsError,
    AgentPluginsErrorKind,
};

/// A plugin operation with no server-side progress reporting must not leave the
/// UI pending forever when the answer never arrives.
const PLUGIN_OPERATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// Bounds the cursor walk so a server that keeps answering with a new cursor
/// cannot spin the client.
const MAX_PLUGIN_SEARCH_PAGES: usize = 32;
const PLUGIN_SEARCH_PAGE_LIMIT: u32 = 100;

fn connection_error(error: anyhow::Error, outcome_unknown: bool) -> AgentPluginsError {
    AgentPluginsError {
        kind: AgentPluginsErrorKind::Connection,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown,
    }
}

fn protocol_error(error: anyhow::Error) -> AgentPluginsError {
    AgentPluginsError {
        kind: AgentPluginsErrorKind::Protocol,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown: false,
    }
}

fn missing_result(method: &str, response: &Value) -> AgentPluginsError {
    AgentPluginsError {
        kind: AgentPluginsErrorKind::Protocol,
        message: format!("{method} 响应缺少 result"),
        data: response.get("error").cloned(),
        outcome_unknown: false,
    }
}

/// Reads a response, requiring `result` and decoding it.
fn decode_result<T>(
    method: &str,
    response: Value,
    decode: impl FnOnce(&Value) -> Result<T, anyhow::Error>,
) -> Result<T, AgentPluginsError> {
    let result = response
        .get("result")
        .cloned()
        .ok_or_else(|| missing_result(method, &response))?;
    decode(&result).map_err(protocol_error)
}

/// Turns one state-changing request into an explicit outcome. The message text
/// of the server error is preserved verbatim; a transport failure that made the
/// connection unusable is reported as an unknown outcome.
fn operation_outcome<T>(
    connection: &Arc<Connection>,
    method: &str,
    response: Result<Value, anyhow::Error>,
    decode: impl FnOnce(&Value) -> Result<T, anyhow::Error>,
) -> AgentPluginOperationOutcome<T> {
    match response {
        Ok(response) => match (response.get("result"), response.get("error")) {
            (Some(result), None) => match decode(result) {
                Ok(value) => AgentPluginOperationOutcome::Succeeded(value),
                Err(error) => AgentPluginOperationOutcome::Unknown {
                    message: format!("{method} 的结果无法读取：{error:#}"),
                },
            },
            (None, Some(error)) => AgentPluginOperationOutcome::Failed {
                message: server_error_message(method, error),
                data: Some(error.clone()),
            },
            _ => AgentPluginOperationOutcome::Unknown {
                message: format!("{method} 响应同时缺少 result 与 error"),
            },
        },
        Err(error) => {
            let message = format!("{error:#}");
            if message.contains("超时") {
                AgentPluginOperationOutcome::TimedOut { message }
            } else if connection.failed.load(std::sync::atomic::Ordering::Acquire) {
                AgentPluginOperationOutcome::Unknown { message }
            } else {
                AgentPluginOperationOutcome::Failed {
                    message,
                    data: None,
                }
            }
        }
    }
}

fn server_error_message(method: &str, error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or(method);
    match error.get("code").and_then(Value::as_i64) {
        Some(code) => format!("{message}（{code}）"),
        None => message.to_owned(),
    }
}

/// Runs one state-changing request on a freshly ensured connection, refusing to
/// act when the generation was replaced while the user was deciding.
fn run_operation<T, D>(
    manager: CodexAppServerManager,
    method: &'static str,
    expected_generation: u64,
    params: Value,
    decode: D,
) -> Receiver<AgentPluginOperationOutcome<T>>
where
    T: Send + 'static,
    D: FnOnce(&Value) -> Result<T, anyhow::Error> + Send + 'static,
{
    let (sender, receiver) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let outcome = match manager.inner.ensure_connection() {
            Ok(connection) => {
                if connection.generation != expected_generation {
                    AgentPluginOperationOutcome::Unknown {
                        message: "连接已重建，操作未在新连接上执行；请重新读取后再试".to_owned(),
                    }
                } else {
                    let response = connection.request_with_timeout(
                        method,
                        Some(params),
                        PLUGIN_OPERATION_TIMEOUT,
                    );
                    operation_outcome(&connection, method, response, decode)
                }
            }
            Err(error) => AgentPluginOperationOutcome::Failed {
                message: format!("无法连接 coding agent：{error:#}"),
                data: None,
            },
        };
        let _ = sender.send_blocking(outcome);
    });
    receiver
}

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn load_plugin_catalog(
        &self,
        request: AgentPluginCatalogRequest,
    ) -> Receiver<Result<AgentPluginCatalog, AgentPluginsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(|error| connection_error(error, false))
                .and_then(|connection| Self::load_plugin_catalog_on(&connection, &request));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn load_plugin_catalog_on(
        connection: &Arc<Connection>,
        request: &AgentPluginCatalogRequest,
    ) -> Result<AgentPluginCatalog, AgentPluginsError> {
        let params = super::super::plugins::list_params(request);
        let response = connection
            .request("plugin/list", params)
            .map_err(|error| connection_error(error, false))?;
        decode_result("plugin/list", response, |result| {
            super::super::plugins::decode_catalog(connection.generation, result)
        })
    }

    pub(in crate::agent::codex) fn load_installed_plugins(
        &self,
        request: AgentPluginInstalledRequest,
    ) -> Receiver<Result<AgentPluginCatalog, AgentPluginsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(|error| connection_error(error, false))
                .and_then(|connection| Self::load_installed_plugins_on(&connection, &request));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn load_installed_plugins_on(
        connection: &Arc<Connection>,
        request: &AgentPluginInstalledRequest,
    ) -> Result<AgentPluginCatalog, AgentPluginsError> {
        let params = super::super::plugins::installed_params(request);
        let response = connection
            .request("plugin/installed", params)
            .map_err(|error| connection_error(error, false))?;
        decode_result("plugin/installed", response, |result| {
            super::super::plugins::decode_catalog(connection.generation, result)
        })
    }

    pub(in crate::agent::codex) fn read_plugin(
        &self,
        request: AgentPluginReadRequest,
    ) -> Receiver<Result<AgentPluginDetail, AgentPluginsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(|error| connection_error(error, false))
                .and_then(|connection| Self::read_plugin_on(&connection, &request));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn read_plugin_on(
        connection: &Arc<Connection>,
        request: &AgentPluginReadRequest,
    ) -> Result<AgentPluginDetail, AgentPluginsError> {
        let params = super::super::plugins::read_params(request);
        let response = connection
            .request("plugin/read", params)
            .map_err(|error| connection_error(error, false))?;
        decode_result("plugin/read", response, |result| {
            super::super::plugins::decode_detail(result)
        })
    }

    /// Searches the catalog. The cursor walk stops on a repeated cursor, which
    /// is a protocol error rather than a page to follow twice.
    pub(in crate::agent::codex) fn search_plugins(
        &self,
        request: AgentPluginSearchRequest,
    ) -> Receiver<Result<AgentPluginSearchPage, AgentPluginsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(|error| connection_error(error, false))
                .and_then(|connection| Self::search_plugins_on(&connection, &request));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn search_plugins_on(
        connection: &Arc<Connection>,
        request: &AgentPluginSearchRequest,
    ) -> Result<AgentPluginSearchPage, AgentPluginsError> {
        let mut current = AgentPluginSearchRequest {
            cursor: request.cursor.clone(),
            limit: Some(request.limit.unwrap_or(PLUGIN_SEARCH_PAGE_LIMIT)),
            scope: request.scope,
            search_term: request.search_term.clone(),
            cwds: request.cwds.clone(),
        };
        let mut pages = 0usize;
        let mut seen = std::collections::HashSet::new();
        let mut combined: Option<AgentPluginSearchPage> = None;
        loop {
            if pages >= MAX_PLUGIN_SEARCH_PAGES {
                return Err(AgentPluginsError {
                    kind: AgentPluginsErrorKind::Protocol,
                    message: format!("plugin/search 分页超过 {MAX_PLUGIN_SEARCH_PAGES} 页，已中止"),
                    data: None,
                    outcome_unknown: false,
                });
            }
            pages += 1;
            let params = super::super::plugins::search_params(&current);
            let response = connection
                .request("plugin/search", params)
                .map_err(|error| connection_error(error, false))?;
            let page = decode_result("plugin/search", response, |result| {
                super::super::plugins::decode_search_page(
                    connection.generation,
                    current.cursor.clone(),
                    current.search_term.clone(),
                    result,
                )
            })?;
            let next = page.next_cursor.clone();
            match &mut combined {
                None => combined = Some(page),
                Some(combined) => combined.results.extend(page.results),
            }
            match next {
                None => break,
                Some(cursor) => {
                    if !seen.insert(cursor.clone()) {
                        return Err(AgentPluginsError {
                            kind: AgentPluginsErrorKind::Protocol,
                            message: format!("plugin/search 返回了重复的 nextCursor `{cursor}`"),
                            data: None,
                            outcome_unknown: false,
                        });
                    }
                    current.cursor = Some(cursor);
                }
            }
        }
        combined
            .ok_or_else(|| AgentPluginsError {
                kind: AgentPluginsErrorKind::Protocol,
                message: "plugin/search 没有返回任何页".to_owned(),
                data: None,
                outcome_unknown: false,
            })
            .map(|page| AgentPluginSearchPage {
                cursor: request.cursor.clone(),
                ..page
            })
    }

    pub(in crate::agent::codex) fn read_plugin_skill(
        &self,
        request: AgentPluginSkillReadRequest,
    ) -> Receiver<Result<AgentPluginSkillContent, AgentPluginsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(|error| connection_error(error, false))
                .and_then(|connection| {
                    let params = super::super::plugins::skill_read_params(&request);
                    let response = connection
                        .request("plugin/skill/read", params)
                        .map_err(|error| connection_error(error, false))?;
                    decode_result("plugin/skill/read", response, |result| {
                        super::super::plugins::decode_skill_content(connection.generation, result)
                    })
                });
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    /// Reconciles installed plugins with configuration. Called once at startup;
    /// its receipt is the only evidence of what changed.
    pub(in crate::agent::codex) fn reconcile_plugins(
        &self,
        request: AgentPluginReconcileRequest,
    ) -> Receiver<Result<AgentPluginReconcileReceipt, AgentPluginsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(|error| connection_error(error, false))
                .and_then(|connection| {
                    let params = super::super::plugins::reconcile_params(&request);
                    let response = connection
                        .request("plugin/reconcile", params)
                        .map_err(|error| connection_error(error, false))?;
                    decode_result("plugin/reconcile", response, |result| {
                        super::super::plugins::decode_reconcile_receipt(
                            connection.generation,
                            result,
                        )
                    })
                });
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(in crate::agent::codex) fn plugin_share_list(
        &self,
    ) -> Receiver<Result<AgentPluginShareList, AgentPluginsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(|error| connection_error(error, false))
                .and_then(|connection| {
                    let response = connection
                        .request(
                            "plugin/share/list",
                            super::super::plugins::share_list_params(),
                        )
                        .map_err(|error| connection_error(error, false))?;
                    decode_result("plugin/share/list", response, |result| {
                        super::super::plugins::decode_share_list(connection.generation, result)
                    })
                });
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(in crate::agent::codex) fn install_plugin(
        &self,
        request: AgentPluginInstallRequest,
    ) -> Receiver<AgentPluginInstallResult> {
        let plugin_name = request.plugin_name.clone();
        let generation = request.generation;
        let outcome = run_operation(
            self.clone(),
            "plugin/install",
            generation,
            super::super::plugins::install_params(&request),
            super::super::plugins::decode_install_receipt,
        );
        relay(
            outcome,
            move |outcome: AgentPluginOperationOutcome<AgentPluginInstallReceipt>| {
                AgentPluginInstallResult {
                    generation,
                    plugin_name,
                    outcome,
                }
            },
        )
    }

    pub(in crate::agent::codex) fn uninstall_plugin(
        &self,
        request: AgentPluginUninstallRequest,
    ) -> Receiver<AgentPluginUninstallResult> {
        let plugin_id = request.plugin_id.clone();
        let generation = request.generation;
        let outcome = run_operation(
            self.clone(),
            "plugin/uninstall",
            generation,
            super::super::plugins::uninstall_params(&request),
            |_| Ok(()),
        );
        relay(outcome, move |outcome| AgentPluginUninstallResult {
            generation,
            plugin_id,
            outcome,
        })
    }

    pub(in crate::agent::codex) fn save_plugin_share(
        &self,
        request: AgentPluginShareSaveRequest,
    ) -> Receiver<AgentPluginShareSaveResult> {
        let path = request.plugin_path.clone();
        let generation = request.generation;
        let outcome = run_operation(
            self.clone(),
            "plugin/share/save",
            generation,
            super::super::plugins::share_save_params(&request),
            super::super::plugins::decode_share_save_receipt,
        );
        relay(outcome, move |outcome| AgentPluginShareSaveResult {
            generation,
            plugin_path: path,
            outcome,
        })
    }

    pub(in crate::agent::codex) fn update_plugin_share_targets(
        &self,
        request: AgentPluginShareUpdateTargetsRequest,
    ) -> Receiver<AgentPluginShareUpdateTargetsResult> {
        let remote_plugin_id = request.remote_plugin_id.clone();
        let generation = request.generation;
        let outcome = run_operation(
            self.clone(),
            "plugin/share/updateTargets",
            generation,
            super::super::plugins::share_update_targets_params(&request),
            super::super::plugins::decode_share_update_targets_receipt,
        );
        relay(outcome, move |outcome| {
            AgentPluginShareUpdateTargetsResult {
                generation,
                remote_plugin_id,
                outcome,
            }
        })
    }

    pub(in crate::agent::codex) fn delete_plugin_share(
        &self,
        request: AgentPluginShareDeleteRequest,
    ) -> Receiver<AgentPluginShareDeleteResult> {
        let remote_plugin_id = request.remote_plugin_id.clone();
        let generation = request.generation;
        let outcome = run_operation(
            self.clone(),
            "plugin/share/delete",
            generation,
            super::super::plugins::share_delete_params(&request),
            |_| Ok(()),
        );
        relay(outcome, move |outcome| AgentPluginShareDeleteResult {
            generation,
            remote_plugin_id,
            outcome,
        })
    }

    pub(in crate::agent::codex) fn add_marketplace(
        &self,
        request: AgentMarketplaceAddRequest,
    ) -> Receiver<AgentMarketplaceAddResult> {
        let source = request.source.clone();
        let generation = request.generation;
        let outcome = run_operation(
            self.clone(),
            "marketplace/add",
            generation,
            super::super::plugins::marketplace_add_params(&request),
            super::super::plugins::decode_marketplace_add,
        );
        relay(outcome, move |outcome| AgentMarketplaceAddResult {
            generation,
            source,
            outcome,
        })
    }

    pub(in crate::agent::codex) fn remove_marketplace(
        &self,
        request: AgentMarketplaceRemoveRequest,
    ) -> Receiver<AgentMarketplaceRemoveResult> {
        let marketplace_name = request.marketplace_name.clone();
        let generation = request.generation;
        let outcome = run_operation(
            self.clone(),
            "marketplace/remove",
            generation,
            super::super::plugins::marketplace_remove_params(&request),
            super::super::plugins::decode_marketplace_remove,
        );
        relay(outcome, move |outcome| AgentMarketplaceRemoveResult {
            generation,
            marketplace_name,
            outcome,
        })
    }

    pub(in crate::agent::codex) fn upgrade_marketplaces(
        &self,
        request: AgentMarketplaceUpgradeRequest,
    ) -> Receiver<AgentMarketplaceUpgradeResult> {
        let marketplace_name = request.marketplace_name.clone();
        let generation = request.generation;
        let outcome = run_operation(
            self.clone(),
            "marketplace/upgrade",
            generation,
            super::super::plugins::marketplace_upgrade_params(&request),
            super::super::plugins::decode_marketplace_upgrade,
        );
        relay(outcome, move |outcome| AgentMarketplaceUpgradeResult {
            generation,
            marketplace_name,
            outcome,
        })
    }
}

/// Moves the raw outcome into the operation-specific result type without
/// changing it.
fn relay<T, R>(
    outcome: Receiver<AgentPluginOperationOutcome<T>>,
    map: impl FnOnce(AgentPluginOperationOutcome<T>) -> R + Send + 'static,
) -> Receiver<R>
where
    T: Send + 'static,
    R: Send + 'static,
{
    let (sender, receiver) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let value = match outcome.recv_blocking() {
            Ok(value) => map(value),
            Err(_) => return,
        };
        let _ = sender.send_blocking(value);
    });
    receiver
}
