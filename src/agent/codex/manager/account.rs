//! Account operations: reads, login lifecycle, logout, and reduction.
//!
//! Account state is connection-scoped. Every mutation reduces into the current
//! generation and publishes the parts that changed, so a new subscriber replays
//! the account, login, and quota snapshots without asking for a thread or turn.

use std::sync::Arc;

use anyhow::Result;
use async_channel::Receiver;
use serde_json::{Value, json};

use super::{CodexAppServerManager, ManagerInner, connection::Connection};
use crate::agent::codex::account::{
    login_request, parse_account_rate_limits_updated, parse_account_response,
    parse_account_updated, parse_cancel_login_response, parse_login_completed,
    parse_login_response, parse_rate_limits_response,
};
use crate::agent::{
    AgentAccountLoginState, AgentAccountRateLimitsState, AgentAccountSnapshot, AgentAccountState,
    AgentLoginCancelOutcome, AgentLoginCompletion, AgentLoginStart, AgentLogoutOutcome,
    AgentRateLimitPatch, AgentRateLimitsRead,
};

/// Login type this phase requests. Codex-managed ChatGPT auth is the only
/// visible product entry point; other variants stay unreachable from the UI.
pub(in crate::agent::codex) const CHATGPT_LOGIN_TYPE: &str = "chatgpt";

impl ManagerInner {
    /// Applies one account mutation and publishes only the parts that changed.
    pub(super) fn mutate_account<F>(&self, connection: &Connection, mutate: F) -> bool
    where
        F: FnOnce(&mut AgentAccountState),
    {
        let (before, after) = {
            let mut state = match connection.state.lock() {
                Ok(state) => state,
                Err(_) => return false,
            };
            let before = state.account.clone();
            mutate(&mut state.account);
            state.account.generation = connection.generation;
            (before, state.account.clone())
        };
        let mut changed = false;
        if before.account != after.account {
            self.publish_connection_event(crate::agent::AgentConnectionEvent::AccountUpdated(
                after.account.clone(),
            ));
            changed = true;
        }
        if before.login != after.login {
            self.publish_connection_event(crate::agent::AgentConnectionEvent::AccountLoginUpdated(
                after.login.clone(),
            ));
            changed = true;
        }
        if before.rate_limits != after.rate_limits {
            self.publish_connection_event(
                crate::agent::AgentConnectionEvent::AccountRateLimitsUpdated(
                    after.rate_limits.clone(),
                ),
            );
            changed = true;
        }
        changed
    }

    /// Clears account state for a generation that is no longer usable, so a new
    /// subscriber or a late callback never observes the previous account.
    pub(super) fn clear_account_state(&self) {
        self.publish_connection_event(crate::agent::AgentConnectionEvent::AccountUpdated(
            AgentAccountSnapshot::default(),
        ));
        self.publish_connection_event(crate::agent::AgentConnectionEvent::AccountLoginUpdated(
            AgentAccountLoginState::default(),
        ));
        self.publish_connection_event(
            crate::agent::AgentConnectionEvent::AccountRateLimitsUpdated(
                AgentAccountRateLimitsState::default(),
            ),
        );
    }
}

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn read_account(
        &self,
    ) -> Receiver<Result<AgentAccountSnapshot, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .and_then(|connection| Self::read_account_on(&connection))
                .map_err(|error| format!("账户状态读取失败：{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(in crate::agent::codex) fn read_rate_limits(
        &self,
    ) -> Receiver<Result<AgentRateLimitsRead, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .and_then(|connection| Self::read_rate_limits_on(&connection))
                .map_err(|error| format!("配额读取失败：{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(in crate::agent::codex) fn start_login(
        &self,
        login_type: String,
    ) -> Receiver<Result<AgentLoginStart, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<AgentLoginStart> {
                let params = login_request(&login_type)?;
                let connection = manager.inner.ensure_connection()?;
                let response = connection.request("account/login/start", params)?;
                let start = parse_login_response(&response, &login_type)?;
                manager.inner.mutate_account(&connection, |state| {
                    state.apply_login_started(&start);
                });
                Ok(start)
            })()
            .map_err(|error| format!("登录请求失败：{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    /// Cancels the login this client is waiting on. A failed RPC keeps the
    /// pending login intact so the user can retry or wait for a completion.
    pub(in crate::agent::codex) fn cancel_login(
        &self,
        login_id: String,
    ) -> Receiver<Result<AgentLoginCancelOutcome, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<AgentLoginCancelOutcome> {
                let connection = manager.inner.ensure_connection()?;
                let response =
                    connection.request("account/login/cancel", json!({ "loginId": login_id }))?;
                let outcome = parse_cancel_login_response(&response)?;
                // Both answers end the local wait: the backend either cancelled
                // this login or does not know it any more.
                manager.inner.mutate_account(&connection, |state| {
                    state.apply_login_canceled(&login_id);
                });
                Ok(outcome)
            })()
            .map_err(|error| format!("取消登录失败：{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    /// Signs out, clears the local account surfaces, and then confirms the
    /// result with a fresh account read plus a quota read. The logout RPC alone
    /// is not treated as the server's final state.
    pub(in crate::agent::codex) fn logout(&self) -> Receiver<Result<AgentLogoutOutcome, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<AgentLogoutOutcome> {
                let connection = manager.inner.ensure_connection()?;
                connection.request("account/logout", json!({}))?;
                manager.inner.mutate_account(&connection, |state| {
                    state.clear_for_logout();
                });
                let mut outcome = AgentLogoutOutcome::default();
                match CodexAppServerManager::read_account_on(&connection) {
                    Ok(snapshot) => outcome.account = Some(snapshot),
                    Err(error) => outcome.confirmation_error = Some(format!("{error:#}")),
                }
                if let Err(error) = CodexAppServerManager::read_rate_limits_on(&connection) {
                    outcome
                        .confirmation_error
                        .get_or_insert_with(|| format!("退出登录后配额状态未确认：{error:#}"));
                }
                Ok(outcome)
            })()
            .map_err(|error| format!("退出登录失败：{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    fn read_account_on(connection: &Arc<Connection>) -> Result<AgentAccountSnapshot> {
        let response = connection.request("account/read", json!({}))?;
        let snapshot = parse_account_response(&response)?;
        if let Some(manager) = connection.manager.upgrade() {
            manager.mutate_account(connection, |state| {
                state.apply_account_read(snapshot.clone());
            });
        }
        Ok(snapshot)
    }

    fn read_rate_limits_on(connection: &Arc<Connection>) -> Result<AgentRateLimitsRead> {
        let response = connection.request("account/rateLimits/read", json!({}))?;
        let read = parse_rate_limits_response(&response)?;
        if let Some(manager) = connection.manager.upgrade() {
            manager.mutate_account(connection, |state| {
                state.apply_rate_limits_read(read.clone());
            });
        }
        Ok(read)
    }
}

impl ManagerInner {
    /// Reduces an account/updated notification, keeping nullable fields from
    /// clearing confirmed values.
    pub(super) fn handle_account_updated(
        &self,
        connection: &Connection,
        message: &Value,
    ) -> Result<()> {
        let update = parse_account_updated(message)?;
        self.mutate_account(connection, |state| {
            state.apply_account_update(update);
        });
        Ok(())
    }

    /// Reduces account/rateLimits/updated, which only ever carries one bucket:
    /// merging it cannot touch another limitId.
    pub(super) fn handle_rate_limits_updated(
        &self,
        connection: &Connection,
        message: &Value,
    ) -> Result<()> {
        let patch: AgentRateLimitPatch = parse_account_rate_limits_updated(message)?;
        self.mutate_account(connection, |state| {
            state.apply_rate_limit_patch(&patch);
        });
        Ok(())
    }

    /// Reduces account/login/completed. A successful completion refreshes the
    /// account and quota snapshots on a worker thread, because the reader
    /// thread must stay free to consume the responses it waits for.
    pub(super) fn handle_login_completed(
        &self,
        connection: &Arc<Connection>,
        message: &Value,
    ) -> Result<()> {
        let completion: AgentLoginCompletion = parse_login_completed(message)?;
        let success = completion.success;
        let applied = self.mutate_account(connection, |state| {
            state.apply_login_completed(completion);
        });
        if applied && success {
            self.spawn_account_refresh(connection.clone());
        }
        Ok(())
    }

    fn spawn_account_refresh(&self, connection: Arc<Connection>) {
        std::thread::spawn(move || {
            if connection.failed.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            let _ = CodexAppServerManager::read_account_on(&connection);
            let _ = CodexAppServerManager::read_rate_limits_on(&connection);
        });
    }
}
