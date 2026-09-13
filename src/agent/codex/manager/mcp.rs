//! MCP server inventory, reload, and OAuth login lifecycle.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use anyhow::Result;
use async_channel::Receiver;
use serde_json::Value;

use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{
    AgentMcpError, AgentMcpErrorKind, AgentMcpOauthCompletion, AgentMcpOauthCompletionStatus,
    AgentMcpOauthLogin, AgentMcpOauthLoginRequest, AgentMcpReloadOutcome, AgentMcpReloadRequest,
    AgentMcpReloadResult, AgentMcpServerPage, AgentMcpServerStatusRequest,
};

/// The reload request has no params and no progress reporting, so a request
/// that produces no answer inside this window must be reported as unconfirmed
/// instead of leaving the entry pending forever.
const RELOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Login ids are client generated and monotonic. The protocol carries no login
/// identifier, so this is the only way to tell a late completion for a
/// replaced login apart from the completion of the current one.
static NEXT_LOGIN_ID: AtomicU64 = AtomicU64::new(1);

fn connection_error(error: anyhow::Error) -> AgentMcpError {
    AgentMcpError {
        kind: AgentMcpErrorKind::Connection,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown: false,
    }
}

fn protocol_error(error: anyhow::Error) -> AgentMcpError {
    AgentMcpError {
        kind: AgentMcpErrorKind::Protocol,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown: false,
    }
}

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn list_mcp_servers(
        &self,
        request: AgentMcpServerStatusRequest,
    ) -> Receiver<Result<AgentMcpServerPage, AgentMcpError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(connection_error)
                .and_then(|connection| Self::list_mcp_servers_on(&connection, &request));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn list_mcp_servers_on(
        connection: &Arc<Connection>,
        request: &AgentMcpServerStatusRequest,
    ) -> Result<AgentMcpServerPage, AgentMcpError> {
        let params = super::super::mcp::status_params(request);
        let response = connection
            .request("mcpServerStatus/list", params)
            .map_err(connection_error)?;
        let result = response
            .get("result")
            .cloned()
            .ok_or_else(|| AgentMcpError {
                kind: AgentMcpErrorKind::Protocol,
                message: "mcpServerStatus/list 响应缺少 result".into(),
                data: response.get("error").cloned(),
                outcome_unknown: false,
            })?;
        super::super::mcp::decode_page(connection.generation, request.cursor.clone(), &result)
            .map_err(protocol_error)
    }

    pub(in crate::agent::codex) fn reload_mcp_servers(
        &self,
        request: AgentMcpReloadRequest,
    ) -> Receiver<AgentMcpReloadResult> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let outcome = manager.reload_mcp_servers_blocking(&request);
            let _ = sender.send_blocking(AgentMcpReloadResult {
                generation: request.generation,
                cwd: request.cwd,
                outcome,
            });
        });
        receiver
    }

    fn reload_mcp_servers_blocking(
        &self,
        request: &AgentMcpReloadRequest,
    ) -> AgentMcpReloadOutcome {
        let connection = match self.inner.ensure_connection() {
            Ok(connection) => connection,
            Err(error) => {
                return AgentMcpReloadOutcome::Failed {
                    message: format!("无法连接 coding agent：{error:#}"),
                    data: None,
                };
            }
        };
        if connection.generation != request.generation {
            return AgentMcpReloadOutcome::Unknown {
                message: "连接已重建，重新加载未在新连接上执行".into(),
            };
        }
        match connection.request_with_timeout("config/mcpServer/reload", None, RELOAD_TIMEOUT) {
            Ok(response) => match (response.get("result"), response.get("error")) {
                (Some(_), None) => AgentMcpReloadOutcome::Reloaded,
                (None, Some(error)) => AgentMcpReloadOutcome::Failed {
                    message: reload_error_message(error),
                    data: Some(error.clone()),
                },
                _ => AgentMcpReloadOutcome::Unknown {
                    message: "重新加载响应同时缺少 result 与 error".into(),
                },
            },
            Err(error) => {
                let message = format!("{error:#}");
                if message.contains("超时") {
                    // The generation is already failed by the transport; the
                    // next explicit action rebuilds it.
                    AgentMcpReloadOutcome::TimedOut { message }
                } else {
                    AgentMcpReloadOutcome::Failed {
                        message,
                        data: None,
                    }
                }
            }
        }
    }

    pub(in crate::agent::codex) fn start_mcp_oauth_login(
        &self,
        request: AgentMcpOauthLoginRequest,
    ) -> Receiver<Result<AgentMcpOauthLogin, AgentMcpError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let connection = manager
                    .inner
                    .ensure_connection()
                    .map_err(connection_error)?;
                if connection.generation != request.generation {
                    return Err(AgentMcpError {
                        kind: AgentMcpErrorKind::Connection,
                        message: "连接已重建，请重新读取 MCP 服务器后再登录".into(),
                        data: None,
                        outcome_unknown: false,
                    });
                }
                Self::start_mcp_oauth_login_on(&connection, &request)
            })();
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn start_mcp_oauth_login_on(
        connection: &Arc<Connection>,
        request: &AgentMcpOauthLoginRequest,
    ) -> Result<AgentMcpOauthLogin, AgentMcpError> {
        let login_id = NEXT_LOGIN_ID.fetch_add(1, Ordering::Relaxed);
        let scope = (request.thread_id.clone(), request.server_name.clone());
        // A newer login for the same server supersedes the previous one; the
        // superseded id becomes a tombstone so its late completion stays inert.
        let superseded = connection
            .register_oauth_login(scope.clone(), login_id)
            .map_err(|error| AgentMcpError {
                kind: AgentMcpErrorKind::Connection,
                message: format!("{error:#}"),
                data: None,
                outcome_unknown: false,
            })?;
        if let Some(superseded) = superseded {
            connection.publish_oauth_completion(AgentMcpOauthCompletion {
                login_id: superseded,
                generation: connection.generation,
                server_name: request.server_name.clone(),
                thread_id: request.thread_id.clone(),
                status: AgentMcpOauthCompletionStatus::Cancelled,
                extra: Default::default(),
            });
        }
        let params = super::super::mcp::oauth_login_params(
            &request.server_name,
            request.thread_id.as_deref(),
            request.scopes.as_ref(),
            request.client_registration,
            request.timeout_secs,
        );
        let response = connection.request("mcpServer/oauth/login", params);
        match response {
            Ok(response) => match response.get("result") {
                Some(result) => match super::super::mcp::decode_oauth_authorization_url(result) {
                    Ok(authorization_url) => Ok(AgentMcpOauthLogin {
                        login_id,
                        generation: connection.generation,
                        server_name: request.server_name.clone(),
                        thread_id: request.thread_id.clone(),
                        authorization_url,
                    }),
                    Err(error) => {
                        connection.remove_oauth_login(login_id);
                        Err(protocol_error(error))
                    }
                },
                None => {
                    connection.remove_oauth_login(login_id);
                    Err(AgentMcpError {
                        kind: AgentMcpErrorKind::Protocol,
                        message: "mcpServer/oauth/login 响应缺少 result".into(),
                        data: response.get("error").cloned(),
                        outcome_unknown: false,
                    })
                }
            },
            Err(error) => {
                connection.remove_oauth_login(login_id);
                Err(connection_error(error))
            }
        }
    }

    /// Cancellation is local: the schema defines no cancel request. The login
    /// is retired so any later completion notification is ignored.
    pub(in crate::agent::codex) fn cancel_mcp_oauth_login(
        &self,
        login_id: u64,
    ) -> Receiver<Result<(), AgentMcpError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let cancelled = self.inner.current_connection().map(|connection| {
            if let Some(login) = connection.cancel_oauth_login(login_id) {
                connection.publish_oauth_completion(AgentMcpOauthCompletion {
                    login_id,
                    generation: connection.generation,
                    server_name: login.1,
                    thread_id: login.0,
                    status: AgentMcpOauthCompletionStatus::Cancelled,
                    extra: Default::default(),
                });
            }
        });
        let result = match cancelled {
            Some(()) | None => Ok(()),
        };
        let _ = sender.send_blocking(result);
        receiver
    }
}

fn reload_error_message(error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("重新加载 MCP 服务器失败");
    match error.get("code").and_then(Value::as_i64) {
        Some(code) => format!("{message}（{code}）"),
        None => message.to_owned(),
    }
}

/// Correlates `mcpServer/oauthLogin/completed` with the login this client
/// started. `Ok(None)` means the notification belongs to a retired login or an
/// unknown one, and must not change UI state.
pub(super) fn correlate_oauth_completion(
    connection: &Arc<Connection>,
    notification: &super::super::mcp::OauthCompletedNotification,
) -> Result<Option<AgentMcpOauthCompletion>> {
    let scope = (
        notification.thread_id.clone(),
        notification.server_name.clone(),
    );
    let Some(login_id) = connection.take_oauth_login(&scope) else {
        return Ok(None);
    };
    let status = if notification.success {
        AgentMcpOauthCompletionStatus::Succeeded
    } else {
        AgentMcpOauthCompletionStatus::Failed(
            notification
                .error
                .clone()
                .unwrap_or_else(|| "OAuth 登录未完成".to_owned()),
        )
    };
    Ok(Some(AgentMcpOauthCompletion {
        login_id,
        generation: connection.generation,
        server_name: notification.server_name.clone(),
        thread_id: notification.thread_id.clone(),
        status,
        extra: Default::default(),
    }))
}
