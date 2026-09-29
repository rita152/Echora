//! `turn/settings/update` for the baseline CLI schema (experimental).
//!
//! The reference client sends it once after a confirmed
//! `thread/settings/update` that named a reviewer, while the thread has a
//! running turn, and only with `approvalsReviewer`: the other permission fields
//! have no per-turn counterpart and keep applying from the next turn.

use anyhow::{Context as _, Result};
use serde_json::{Value, json};

use super::json::{object, required_enum};
use crate::agent::AgentTurnSettingsStatus;

pub(super) const TURN_SETTINGS_UPDATE_METHOD: &str = "turn/settings/update";

pub(super) fn reviewer_params(thread_id: &str, turn_id: &str, reviewer: &str) -> Value {
    json!({ "threadId": thread_id, "turnId": turn_id, "approvalsReviewer": reviewer })
}

fn parse_status(raw: &str) -> Option<AgentTurnSettingsStatus> {
    match raw {
        "applied" => Some(AgentTurnSettingsStatus::Applied),
        "targetUnavailable" => Some(AgentTurnSettingsStatus::TargetUnavailable),
        _ => None,
    }
}

pub(super) fn parse_response(response: &Value) -> Result<AgentTurnSettingsStatus> {
    let result = object(
        response
            .get("result")
            .context("turn/settings/update 响应缺少 result")?,
        "turn/settings/update result",
    )?;
    required_enum(
        result,
        "status",
        "turn/settings/update result",
        parse_status,
    )
}

#[cfg(test)]
mod tests;
