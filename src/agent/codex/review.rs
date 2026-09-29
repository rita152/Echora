//! `review/start` for the baseline CLI schema.
//!
//! Reviews are always requested `inline` on the thread they are given: a
//! paginated thread rejects `detached` delivery (-32600 "paginated threads do
//! not support detached review"), and the schema deprecates it in favour of
//! `thread/start` followed by an inline review, which is how a separate
//! review chat is made.
//!
//! The review's own `turn/started` carries a second turn id that nothing else
//! uses: every item and the final `turn/completed` name the id the response
//! returned. The manager treats that second id as an alias of the review turn.

use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};

use crate::agent::AgentReviewTarget;

pub(super) const REVIEW_START_METHOD: &str = "review/start";

pub(super) fn target_value(target: &AgentReviewTarget) -> Value {
    match target {
        AgentReviewTarget::UncommittedChanges => json!({ "type": "uncommittedChanges" }),
        AgentReviewTarget::BaseBranch { branch } => {
            json!({ "type": "baseBranch", "branch": branch })
        }
    }
}

pub(super) fn review_params(thread_id: &str, target: &AgentReviewTarget) -> Value {
    json!({
        "threadId": thread_id,
        "target": target_value(target),
        "delivery": "inline",
    })
}

/// The review turn's id. An inline review runs on the requested thread, so a
/// response naming another thread is a protocol error.
pub(super) fn parse_review_turn_id(response: &Value, thread_id: &str) -> Result<String> {
    let result = response
        .get("result")
        .and_then(Value::as_object)
        .context("review/start 响应缺少对象 result")?;
    let review_thread = result
        .get("reviewThreadId")
        .and_then(Value::as_str)
        .context("review/start 响应缺少字符串 reviewThreadId")?;
    if review_thread != thread_id {
        bail!("inline review/start 返回了其他线程 `{review_thread}`，请求的是 `{thread_id}`");
    }
    result
        .get("turn")
        .and_then(|turn| turn.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .context("review/start 响应缺少字符串 turn.id")
}
