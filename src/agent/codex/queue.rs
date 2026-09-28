//! `thread/queue/*` requests and `thread/queue/changed` for the baseline CLI
//! schema. Queued input reuses the turn input encoder, and decoding reuses the
//! userMessage content rules so a queued submission and the userMessage the
//! server later starts from it read the same way.

use std::collections::HashSet;

use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};

use super::json::{array, object, params, required_string};
use crate::agent::{AgentQueuedSubmission, ThreadHistoryItem};

pub(super) const QUEUE_CHANGED_METHOD: &str = "thread/queue/changed";
/// Upper bound on list pages; a server that keeps returning cursors past this
/// is treated like a cursor loop.
pub(super) const QUEUE_LIST_PAGE_LIMIT: usize = 64;

pub(super) fn parse_submission(value: &Value) -> Result<AgentQueuedSubmission> {
    let submission = object(value, "QueuedSubmission")?;
    let id = required_string(submission, "id", "QueuedSubmission")?;
    let client_message_id = required_string(submission, "clientUserMessageId", "QueuedSubmission")?;
    let input = array(
        submission
            .get("input")
            .context("QueuedSubmission 缺少 input")?,
        "QueuedSubmission.input",
    )?;
    // The queued input has the same UserInput vocabulary as a userMessage's
    // content, so it is validated and decoded through that exact path.
    let item = json!({
        "type": "userMessage",
        "id": id,
        "clientId": client_message_id,
        "content": input,
    });
    super::items::validate_user_message(item.as_object().expect("literal object"))
        .context("QueuedSubmission.input 不符合 UserInput schema")?;
    let ThreadHistoryItem::UserMessage { text, images, .. } =
        super::workspace_protocol::parse_history_item(&item)?
    else {
        bail!("QueuedSubmission.input 未解码为用户消息");
    };
    Ok(AgentQueuedSubmission {
        id,
        client_message_id,
        text,
        attachments: images,
    })
}

fn submission_result(response: &Value, method: &str) -> Result<AgentQueuedSubmission> {
    let result = object(
        response
            .get("result")
            .with_context(|| format!("{method} 响应缺少 result"))?,
        &format!("{method} result"),
    )?;
    parse_submission(
        result
            .get("queuedSubmission")
            .with_context(|| format!("{method} 响应缺少 queuedSubmission"))?,
    )
}

pub(super) fn parse_add_response(
    response: &Value,
    client_message_id: &str,
) -> Result<AgentQueuedSubmission> {
    let submission = submission_result(response, "thread/queue/add")?;
    if submission.client_message_id != client_message_id {
        bail!(
            "thread/queue/add 返回的 clientUserMessageId `{}` 与请求的 `{client_message_id}` 不一致",
            submission.client_message_id
        );
    }
    Ok(submission)
}

pub(super) fn parse_update_response(
    response: &Value,
    queued_submission_id: &str,
) -> Result<AgentQueuedSubmission> {
    let submission = submission_result(response, "thread/queue/update")?;
    if submission.id != queued_submission_id {
        bail!(
            "thread/queue/update 返回的 id `{}` 与请求的 `{queued_submission_id}` 不一致",
            submission.id
        );
    }
    Ok(submission)
}

pub(super) fn parse_delete_response(response: &Value) -> Result<bool> {
    response
        .pointer("/result/deleted")
        .and_then(Value::as_bool)
        .context("thread/queue/delete 响应缺少布尔字段 deleted")
}

pub(super) fn parse_reorder_response(response: &Value) -> Result<()> {
    object(
        response
            .get("result")
            .context("thread/queue/reorder 响应缺少 result")?,
        "thread/queue/reorder result",
    )
    .map(|_| ())
}

/// `thread/queue/start` returns the started turn; only its id is consumed.
pub(super) fn parse_start_response(response: &Value) -> Result<String> {
    response
        .pointer("/result/turn/id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("thread/queue/start 响应缺少 result.turn.id")
}

pub(super) fn list_params(thread_id: &str, cursor: Option<&str>) -> Value {
    json!({ "threadId": thread_id, "cursor": cursor })
}

/// One list page: submissions plus the next cursor.
pub(super) fn parse_list_page(
    response: &Value,
) -> Result<(Vec<AgentQueuedSubmission>, Option<String>)> {
    let result = object(
        response
            .get("result")
            .context("thread/queue/list 响应缺少 result")?,
        "thread/queue/list result",
    )?;
    let data = array(
        result
            .get("data")
            .context("thread/queue/list 响应缺少 data")?,
        "thread/queue/list data",
    )?
    .iter()
    .map(parse_submission)
    .collect::<Result<Vec<_>>>()?;
    let next_cursor = match result.get("nextCursor") {
        None | Some(Value::Null) => None,
        Some(Value::String(cursor)) => Some(cursor.clone()),
        Some(_) => bail!("thread/queue/list 的 nextCursor 必须是字符串或 null"),
    };
    Ok((data, next_cursor))
}

/// Accumulates list pages, rejecting repeated cursors and duplicate ids.
#[derive(Default)]
pub(super) struct QueueListAccumulator {
    seen_cursors: HashSet<String>,
    seen_ids: HashSet<String>,
    pages: usize,
    pub(super) submissions: Vec<AgentQueuedSubmission>,
}

impl QueueListAccumulator {
    /// Adds one page and returns the cursor to request next, if any.
    pub(super) fn push(
        &mut self,
        page: Vec<AgentQueuedSubmission>,
        next_cursor: Option<String>,
    ) -> Result<Option<String>> {
        self.pages += 1;
        for submission in page {
            if !self.seen_ids.insert(submission.id.clone()) {
                bail!(
                    "thread/queue/list 返回重复的 queuedSubmission id `{}`",
                    submission.id
                );
            }
            self.submissions.push(submission);
        }
        let Some(cursor) = next_cursor else {
            return Ok(None);
        };
        if !self.seen_cursors.insert(cursor.clone()) {
            bail!("thread/queue/list 返回重复的游标 `{cursor}`");
        }
        if self.pages >= QUEUE_LIST_PAGE_LIMIT {
            bail!("thread/queue/list 超过 {QUEUE_LIST_PAGE_LIMIT} 页仍未结束");
        }
        Ok(Some(cursor))
    }
}

pub(super) fn parse_changed(message: &Value) -> Result<String> {
    if message.get("id").is_some() {
        bail!("{QUEUE_CHANGED_METHOD} 必须是通知，不能包含 id");
    }
    required_string(
        params(message, QUEUE_CHANGED_METHOD)?,
        "threadId",
        QUEUE_CHANGED_METHOD,
    )
}

#[cfg(test)]
mod tests;
