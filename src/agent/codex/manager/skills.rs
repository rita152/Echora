//! Skills inventory and configuration writes.

use std::{collections::HashSet, sync::Arc};

use async_channel::Receiver;

use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{
    AgentSkillWriteReceipt, AgentSkillWriteRequest, AgentSkillsError, AgentSkillsErrorKind,
    AgentSkillsLoadRequest, AgentSkillsSnapshot,
};

/// A skills request never needs more than one server round trip per page. The
/// bound exists so a server that keeps returning cursors cannot block a caller
/// forever; a repeated or looping cursor is rejected instead of followed.
const MAX_SKILL_PAGES: usize = 64;

fn connection_error(error: anyhow::Error, outcome_unknown: bool) -> AgentSkillsError {
    AgentSkillsError {
        kind: AgentSkillsErrorKind::Connection,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown,
    }
}

fn protocol_error(error: anyhow::Error) -> AgentSkillsError {
    AgentSkillsError {
        kind: AgentSkillsErrorKind::Protocol,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown: false,
    }
}

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn load_skills(
        &self,
        request: AgentSkillsLoadRequest,
    ) -> Receiver<Result<AgentSkillsSnapshot, AgentSkillsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(|error| connection_error(error, false))
                .and_then(|connection| Self::load_skills_on(&connection, &request));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn load_skills_on(
        connection: &Arc<Connection>,
        request: &AgentSkillsLoadRequest,
    ) -> Result<AgentSkillsSnapshot, AgentSkillsError> {
        let params = super::super::skills::list_params(&request.cwds, request.force_reload);
        let response = connection
            .request("skills/list", params)
            .map_err(|error| connection_error(error, false))?;
        let result = response
            .get("result")
            .cloned()
            .ok_or_else(|| AgentSkillsError {
                kind: AgentSkillsErrorKind::Protocol,
                message: "skills/list 响应缺少 result".into(),
                data: response.get("error").cloned(),
                outcome_unknown: false,
            })?;
        let mut snapshot = super::super::skills::decode_snapshot(connection.generation, &result)
            .map_err(protocol_error)?;
        // The pinned schema defines no cursor for `skills/list`. If a future
        // server adds one, it is followed once per distinct value so a repeated
        // or looping cursor can never produce an unbounded walk.
        let mut seen = HashSet::new();
        let mut pages = 1usize;
        while let Some(cursor) = snapshot.next_cursor.clone() {
            if !seen.insert(cursor.clone()) {
                return Err(AgentSkillsError {
                    kind: AgentSkillsErrorKind::Protocol,
                    message: format!("skills/list 返回了重复的 nextCursor `{cursor}`"),
                    data: None,
                    outcome_unknown: false,
                });
            }
            if pages >= MAX_SKILL_PAGES {
                return Err(AgentSkillsError {
                    kind: AgentSkillsErrorKind::Protocol,
                    message: format!("skills/list 分页超过 {MAX_SKILL_PAGES} 页，已中止"),
                    data: None,
                    outcome_unknown: false,
                });
            }
            pages += 1;
            let params = super::super::skills::list_params(&request.cwds, request.force_reload);
            let response = connection
                .request("skills/list", params)
                .map_err(|error| connection_error(error, false))?;
            let result = response
                .get("result")
                .cloned()
                .ok_or_else(|| AgentSkillsError {
                    kind: AgentSkillsErrorKind::Protocol,
                    message: "skills/list 响应缺少 result".into(),
                    data: response.get("error").cloned(),
                    outcome_unknown: false,
                })?;
            let page = super::super::skills::decode_snapshot(connection.generation, &result)
                .map_err(protocol_error)?;
            snapshot.entries.extend(page.entries);
            snapshot.next_cursor = page.next_cursor;
            snapshot.extra.extend(page.extra);
        }
        Ok(snapshot)
    }

    pub(in crate::agent::codex) fn write_skill_config(
        &self,
        request: AgentSkillWriteRequest,
    ) -> Receiver<Result<AgentSkillWriteReceipt, AgentSkillsError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let connection = manager
                    .inner
                    .ensure_connection()
                    .map_err(|error| connection_error(error, false))?;
                if connection.generation != request.generation {
                    return Err(AgentSkillsError {
                        kind: AgentSkillsErrorKind::Connection,
                        message: "连接已重建，技能状态可能已变化，请重新读取后再保存".into(),
                        data: None,
                        outcome_unknown: false,
                    });
                }
                Self::write_skill_config_on(&connection, &request)
            })();
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn write_skill_config_on(
        connection: &Arc<Connection>,
        request: &AgentSkillWriteRequest,
    ) -> Result<AgentSkillWriteReceipt, AgentSkillsError> {
        let params = super::super::skills::write_params(&request.selector, request.enabled);
        let response = connection
            .request("skills/config/write", params)
            .map_err(|error| {
                let unknown = connection.failed.load(std::sync::atomic::Ordering::Acquire);
                connection_error(error, unknown)
            })?;
        let result = response
            .get("result")
            .cloned()
            .ok_or_else(|| AgentSkillsError {
                kind: AgentSkillsErrorKind::Protocol,
                message: "skills/config/write 响应缺少 result".into(),
                data: response.get("error").cloned(),
                outcome_unknown: true,
            })?;
        super::super::skills::decode_write_receipt(&result).map_err(|error| {
            // The write may have been applied even though its receipt could not
            // be read; the caller must not silently retry it.
            let mut error = protocol_error(error);
            error.outcome_unknown = true;
            error
        })
    }
}
