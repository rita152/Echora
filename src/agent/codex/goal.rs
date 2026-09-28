//! `thread/goal/*` requests and notifications for the baseline CLI schema.

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};

use super::json::{object, optional_i64, params, required_enum, required_string};
use crate::agent::{
    AgentOptionalField, AgentThreadGoal, AgentThreadGoalStatus, AgentThreadGoalUpdate,
};

pub(super) const GOAL_UPDATED_METHOD: &str = "thread/goal/updated";
pub(super) const GOAL_CLEARED_METHOD: &str = "thread/goal/cleared";

pub(super) fn status_wire(status: AgentThreadGoalStatus) -> &'static str {
    match status {
        AgentThreadGoalStatus::Active => "active",
        AgentThreadGoalStatus::Paused => "paused",
        AgentThreadGoalStatus::Blocked => "blocked",
        AgentThreadGoalStatus::UsageLimited => "usageLimited",
        AgentThreadGoalStatus::BudgetLimited => "budgetLimited",
        AgentThreadGoalStatus::Complete => "complete",
    }
}

fn parse_status(raw: &str) -> Option<AgentThreadGoalStatus> {
    AgentThreadGoalStatus::ALL
        .into_iter()
        .find(|status| status_wire(*status) == raw)
}

fn required_i64(goal: &Map<String, Value>, field: &str) -> Result<i64> {
    goal.get(field)
        .and_then(Value::as_i64)
        .with_context(|| format!("ThreadGoal 缺少 int64 字段 {field}"))
}

/// Decodes a ThreadGoal. Every required field is enforced; the optional token
/// budget keeps the schema's nullable meaning.
pub(super) fn parse_goal(value: &Value) -> Result<AgentThreadGoal> {
    let goal = object(value, "ThreadGoal")?;
    Ok(AgentThreadGoal {
        thread_id: required_string(goal, "threadId", "ThreadGoal")?,
        objective: required_string(goal, "objective", "ThreadGoal")?,
        status: required_enum(goal, "status", "ThreadGoal", parse_status)?,
        token_budget: optional_i64(goal, "tokenBudget", "ThreadGoal")?,
        tokens_used: required_i64(goal, "tokensUsed")?,
        time_used_seconds: required_i64(goal, "timeUsedSeconds")?,
        created_at: required_i64(goal, "createdAt")?,
        updated_at: required_i64(goal, "updatedAt")?,
    })
}

fn ensure_goal_thread(goal: &AgentThreadGoal, thread_id: &str, context: &str) -> Result<()> {
    if goal.thread_id != thread_id {
        bail!(
            "{context} 返回的 goal.threadId `{}` 与请求的 `{thread_id}` 不一致",
            goal.thread_id
        );
    }
    Ok(())
}

pub(super) fn set_params(update: &AgentThreadGoalUpdate) -> Value {
    let mut params = Map::new();
    params.insert("threadId".into(), json!(update.thread_id));
    if let Some(objective) = &update.objective {
        params.insert("objective".into(), json!(objective));
    }
    if let Some(status) = update.status {
        params.insert("status".into(), json!(status_wire(status)));
    }
    match &update.token_budget {
        AgentOptionalField::Unspecified => {}
        AgentOptionalField::Null => {
            params.insert("tokenBudget".into(), Value::Null);
        }
        AgentOptionalField::Value(budget) => {
            params.insert("tokenBudget".into(), json!(budget));
        }
    }
    Value::Object(params)
}

/// `thread/goal/set` returns the goal after the update.
pub(super) fn parse_set_response(response: &Value, thread_id: &str) -> Result<AgentThreadGoal> {
    let result = object(
        response
            .get("result")
            .context("thread/goal/set 响应缺少 result")?,
        "thread/goal/set result",
    )?;
    let goal = parse_goal(
        result
            .get("goal")
            .context("thread/goal/set 响应缺少 goal")?,
    )?;
    ensure_goal_thread(&goal, thread_id, "thread/goal/set")?;
    Ok(goal)
}

/// `thread/goal/get` returns the goal, or null (or no field) when none exists.
pub(super) fn parse_get_response(
    response: &Value,
    thread_id: &str,
) -> Result<Option<AgentThreadGoal>> {
    let result = object(
        response
            .get("result")
            .context("thread/goal/get 响应缺少 result")?,
        "thread/goal/get result",
    )?;
    match result.get("goal") {
        None | Some(Value::Null) => Ok(None),
        Some(goal) => {
            let goal = parse_goal(goal)?;
            ensure_goal_thread(&goal, thread_id, "thread/goal/get")?;
            Ok(Some(goal))
        }
    }
}

pub(super) fn parse_clear_response(response: &Value) -> Result<bool> {
    response
        .pointer("/result/cleared")
        .and_then(Value::as_bool)
        .context("thread/goal/clear 响应缺少布尔字段 cleared")
}

/// A decoded goal notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GoalNotification {
    Updated {
        thread_id: String,
        turn_id: Option<String>,
        goal: AgentThreadGoal,
    },
    Cleared {
        thread_id: String,
    },
}

pub(super) fn parse_notification(message: &Value) -> Result<GoalNotification> {
    if message.get("id").is_some() {
        bail!("thread/goal 通知不能包含 id");
    }
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .context("thread/goal 通知缺少 method")?;
    let params = params(message, method)?;
    let thread_id = required_string(params, "threadId", method)?;
    match method {
        GOAL_UPDATED_METHOD => {
            let turn_id = match params.get("turnId") {
                None | Some(Value::Null) => None,
                Some(Value::String(turn_id)) => Some(turn_id.clone()),
                Some(_) => bail!("{method} 的 turnId 必须是字符串或 null"),
            };
            let goal = parse_goal(
                params
                    .get("goal")
                    .context("thread/goal/updated 缺少 goal")?,
            )?;
            ensure_goal_thread(&goal, &thread_id, method)?;
            Ok(GoalNotification::Updated {
                thread_id,
                turn_id,
                goal,
            })
        }
        GOAL_CLEARED_METHOD => Ok(GoalNotification::Cleared { thread_id }),
        other => bail!("`{other}` 不是 thread/goal 通知"),
    }
}

#[cfg(test)]
mod tests;
