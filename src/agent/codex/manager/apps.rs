//! App (connector) directory reads.

use std::sync::Arc;

use async_channel::Receiver;
use serde_json::Value;

use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{
    AgentAppsError, AgentAppsErrorKind, AgentAppsInstalledRequest, AgentAppsListRequest,
    AgentAppsPage, AgentAppsReadRequest, AgentAppsReadResult, AgentInstalledApps,
};

/// Bounds the cursor walk so a server that keeps answering with a fresh cursor
/// cannot spin the client.
const MAX_APP_PAGES: usize = 64;
const APP_PAGE_LIMIT: u32 = 100;

fn connection_error(error: anyhow::Error) -> AgentAppsError {
    AgentAppsError {
        kind: AgentAppsErrorKind::Connection,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown: false,
    }
}

fn protocol_error(error: anyhow::Error) -> AgentAppsError {
    AgentAppsError {
        kind: AgentAppsErrorKind::Protocol,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown: false,
    }
}

fn decode_result<T>(
    method: &str,
    response: Value,
    decode: impl FnOnce(&Value) -> Result<T, anyhow::Error>,
) -> Result<T, AgentAppsError> {
    let result = response
        .get("result")
        .cloned()
        .ok_or_else(|| AgentAppsError {
            kind: AgentAppsErrorKind::Protocol,
            message: format!("{method} 响应缺少 result"),
            data: response.get("error").cloned(),
            outcome_unknown: false,
        })?;
    decode(&result).map_err(protocol_error)
}

impl CodexAppServerManager {
    /// Walks every page of the directory. A repeated cursor is a protocol
    /// error rather than a page to follow twice.
    pub(in crate::agent::codex) fn load_apps(
        &self,
        request: AgentAppsListRequest,
    ) -> Receiver<Result<AgentAppsPage, AgentAppsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(connection_error)
                .and_then(|connection| Self::load_apps_on(&connection, &request));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn load_apps_on(
        connection: &Arc<Connection>,
        request: &AgentAppsListRequest,
    ) -> Result<AgentAppsPage, AgentAppsError> {
        let mut current = AgentAppsListRequest {
            limit: Some(request.limit.unwrap_or(APP_PAGE_LIMIT)),
            ..request.clone()
        };
        let mut pages = 0usize;
        let mut seen = std::collections::HashSet::new();
        let mut combined: Option<AgentAppsPage> = None;
        loop {
            if pages >= MAX_APP_PAGES {
                return Err(AgentAppsError {
                    kind: AgentAppsErrorKind::Protocol,
                    message: format!("app/list 分页超过 {MAX_APP_PAGES} 页，已中止"),
                    data: None,
                    outcome_unknown: false,
                });
            }
            pages += 1;
            let params = super::super::apps::list_params(&current);
            let response = connection
                .request("app/list", params)
                .map_err(connection_error)?;
            let page = decode_result("app/list", response, |result| {
                super::super::apps::decode_page(
                    connection.generation,
                    current.cursor.clone(),
                    result,
                )
            })?;
            let next = page.next_cursor.clone();
            match &mut combined {
                None => combined = Some(page),
                Some(combined) => combined.apps.extend(page.apps),
            }
            match next {
                None => break,
                Some(cursor) => {
                    if !seen.insert(cursor.clone()) {
                        return Err(AgentAppsError {
                            kind: AgentAppsErrorKind::Protocol,
                            message: format!("app/list 返回了重复的 nextCursor `{cursor}`"),
                            data: None,
                            outcome_unknown: false,
                        });
                    }
                    current.cursor = Some(cursor);
                }
            }
        }
        Ok(combined.unwrap_or(AgentAppsPage {
            generation: connection.generation,
            cursor: request.cursor.clone(),
            apps: Vec::new(),
            next_cursor: None,
            extra: Default::default(),
        }))
    }

    pub(in crate::agent::codex) fn load_installed_apps(
        &self,
        request: AgentAppsInstalledRequest,
    ) -> Receiver<Result<AgentInstalledApps, AgentAppsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(connection_error)
                .and_then(|connection| Self::load_installed_apps_on(&connection, &request));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn load_installed_apps_on(
        connection: &Arc<Connection>,
        request: &AgentAppsInstalledRequest,
    ) -> Result<AgentInstalledApps, AgentAppsError> {
        let params = super::super::apps::installed_params(request);
        let response = connection
            .request("app/installed", params)
            .map_err(connection_error)?;
        decode_result("app/installed", response, |result| {
            super::super::apps::decode_installed(connection.generation, result)
        })
    }

    pub(in crate::agent::codex) fn read_apps(
        &self,
        request: AgentAppsReadRequest,
    ) -> Receiver<Result<AgentAppsReadResult, AgentAppsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(connection_error)
                .and_then(|connection| Self::read_apps_on(&connection, &request));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn read_apps_on(
        connection: &Arc<Connection>,
        request: &AgentAppsReadRequest,
    ) -> Result<AgentAppsReadResult, AgentAppsError> {
        let params = super::super::apps::read_params(request);
        let response = connection
            .request("app/read", params)
            .map_err(connection_error)?;
        decode_result("app/read", response, |result| {
            super::super::apps::decode_read(connection.generation, result)
        })
    }
}
