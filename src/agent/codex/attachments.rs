//! `thread/attachment/*` requests and `thread/attachment/updated` for the
//! baseline CLI schema.
//!
//! Payloads are opaque JSON on the wire; the two types this client shows are
//! decoded here with the reference's own acceptance rules, and anything else is
//! kept as it arrived.

use std::collections::HashSet;

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};

use super::json::{array, object, params, required_string};
use crate::agent::{
    AgentAttachmentAddOutcome, AgentAttachmentAddRequest, AgentAttachmentAdded,
    AgentAttachmentContent, AgentAttachmentOperation, AgentAttachmentRemoveRequest,
    AgentPullRequestAttachment, AgentPullRequestRef, AgentThreadAttachment,
    AgentWorktreeAttachment, PULL_REQUEST_ATTACHMENT_TYPE, WORKTREE_ATTACHMENT_TYPE,
};

pub(super) const ATTACHMENT_UPDATED_METHOD: &str = "thread/attachment/updated";
/// Page size for `thread/attachment/list`. The reference asks for 100, but the
/// 0.158 server caps a page at 100 and drops `nextCursor` whenever the limit is
/// 100 or more, so a thread with more attachments would be silently cut off;
/// 99 is the largest size that still pages.
pub(super) const ATTACHMENT_LIST_PAGE_SIZE: u32 = 99;
/// Upper bound on list pages; more is treated like a cursor loop.
pub(super) const ATTACHMENT_LIST_PAGE_LIMIT: usize = 64;

fn optional_nullable_string(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<String>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("{context}.{field} 必须是字符串或 null"),
    }
}

/// The reference keeps a `pull_request` payload only when `url` is a string
/// that parses as a pull request and `root`/`headBranch` are strings or null,
/// and a `worktree` payload only when both roots are non-empty strings. A known
/// type that does not match is kept raw, like an unknown type.
fn decode_content(attachment_type: &str, payload: &Value) -> AgentAttachmentContent {
    let typed = match attachment_type {
        PULL_REQUEST_ATTACHMENT_TYPE => payload.as_object().and_then(|payload| {
            let url = payload.get("url")?.as_str()?.to_owned();
            AgentPullRequestRef::parse(&url)?;
            let root = optional_nullable_string(payload, "root", "pull_request").ok()?;
            let head_branch =
                optional_nullable_string(payload, "headBranch", "pull_request").ok()?;
            Some(AgentAttachmentContent::PullRequest(
                AgentPullRequestAttachment {
                    url,
                    root,
                    head_branch,
                },
            ))
        }),
        WORKTREE_ATTACHMENT_TYPE => payload.as_object().and_then(|payload| {
            let root = payload
                .get("root")?
                .as_str()
                .filter(|root| !root.is_empty())?;
            let workspace_root = payload
                .get("workspaceRoot")?
                .as_str()
                .filter(|root| !root.is_empty())?;
            Some(AgentAttachmentContent::Worktree(AgentWorktreeAttachment {
                root: root.to_owned(),
                workspace_root: workspace_root.to_owned(),
            }))
        }),
        _ => None,
    };
    typed.unwrap_or_else(|| AgentAttachmentContent::Other {
        attachment_type: attachment_type.to_owned(),
        payload: payload.clone(),
    })
}

pub(super) fn parse_attachment(value: &Value) -> Result<AgentThreadAttachment> {
    let attachment = object(value, "ThreadAttachment")?;
    let attachment_type = required_string(attachment, "attachmentType", "ThreadAttachment")?;
    let payload = attachment
        .get("payload")
        .context("ThreadAttachment 缺少 payload")?;
    Ok(AgentThreadAttachment {
        id: required_string(attachment, "id", "ThreadAttachment")?,
        identity_key: required_string(attachment, "identityKey", "ThreadAttachment")?,
        created_at: attachment
            .get("createdAt")
            .and_then(Value::as_i64)
            .context("ThreadAttachment 缺少 int64 字段 createdAt")?,
        content: decode_content(&attachment_type, payload),
    })
}

pub(super) fn list_params(thread_id: &str, cursor: Option<&str>) -> Value {
    json!({ "threadId": thread_id, "cursor": cursor, "limit": ATTACHMENT_LIST_PAGE_SIZE })
}

/// One page and its cursor. Duplicate ids inside the page are a protocol
/// error; the caller also rejects ids already seen on earlier pages.
pub(super) fn parse_list_page(
    response: &Value,
) -> Result<(Vec<AgentThreadAttachment>, Option<String>)> {
    let result = object(
        response
            .get("result")
            .context("thread/attachment/list 响应缺少 result")?,
        "thread/attachment/list result",
    )?;
    let data = array(
        result
            .get("data")
            .context("thread/attachment/list 响应缺少 data")?,
        "thread/attachment/list data",
    )?;
    let mut seen = HashSet::new();
    let mut attachments = Vec::with_capacity(data.len());
    for value in data {
        let attachment = parse_attachment(value)?;
        if !seen.insert(attachment.id.clone()) {
            bail!(
                "thread/attachment/list 同一页出现重复附件 `{}`",
                attachment.id
            );
        }
        attachments.push(attachment);
    }
    let cursor = optional_nullable_string(result, "nextCursor", "thread/attachment/list")?;
    if cursor.as_deref() == Some("") {
        bail!("thread/attachment/list 返回了空的 nextCursor");
    }
    Ok((attachments, cursor))
}

pub(super) fn add_params(request: &AgentAttachmentAddRequest) -> Value {
    json!({
        "threadId": request.thread_id,
        "attachmentType": request.attachment_type,
        "identityKey": request.identity_key,
        "payload": request.payload,
    })
}

pub(super) fn parse_add_response(
    response: &Value,
    request: &AgentAttachmentAddRequest,
) -> Result<AgentAttachmentAdded> {
    let result = object(
        response
            .get("result")
            .context("thread/attachment/add 响应缺少 result")?,
        "thread/attachment/add result",
    )?;
    let outcome = match result.get("outcome").and_then(Value::as_str) {
        Some("created") => AgentAttachmentAddOutcome::Created,
        Some("existing") => AgentAttachmentAddOutcome::Existing,
        Some(other) => bail!("thread/attachment/add 的 outcome 为未知值 `{other}`"),
        None => bail!("thread/attachment/add 响应缺少字符串字段 outcome"),
    };
    let raw = result
        .get("attachment")
        .context("thread/attachment/add 响应缺少 attachment")?;
    let attachment = parse_attachment(raw)?;
    let returned_type = raw
        .get("attachmentType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if returned_type != request.attachment_type || attachment.identity_key != request.identity_key {
        bail!(
            "thread/attachment/add 返回的附件 `{returned_type}` / `{}` 与请求的 `{}` / `{}` 不一致",
            attachment.identity_key,
            request.attachment_type,
            request.identity_key
        );
    }
    Ok(AgentAttachmentAdded {
        outcome,
        attachment,
    })
}

pub(super) fn remove_params(request: &AgentAttachmentRemoveRequest) -> Value {
    json!({
        "threadId": request.thread_id,
        "attachmentType": request.attachment_type,
        "identityKey": request.identity_key,
    })
}

/// A successful removal carries no data; removing a pair that is not attached
/// answers the same way.
pub(super) fn parse_remove_response(response: &Value) -> Result<()> {
    object(
        response
            .get("result")
            .context("thread/attachment/remove 响应缺少 result")?,
        "thread/attachment/remove result",
    )
    .map(|_| ())
}

/// The decoded fields of `thread/attachment/updated`. Fields outside the schema
/// (the server adds `emittedAtMs` to every notification) are ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AttachmentUpdated {
    pub thread_id: String,
    pub attachment_id: String,
    pub attachment_type: String,
    pub identity_key: String,
    pub operation: AgentAttachmentOperation,
}

pub(super) fn parse_updated(message: &Value) -> Result<AttachmentUpdated> {
    if message.get("id").is_some() {
        bail!("{ATTACHMENT_UPDATED_METHOD} 通知不能包含 id");
    }
    let params = params(message, ATTACHMENT_UPDATED_METHOD)?;
    let operation = match params.get("operation").and_then(Value::as_str) {
        Some("created") => AgentAttachmentOperation::Created,
        Some("deleted") => AgentAttachmentOperation::Deleted,
        Some(other) => bail!("{ATTACHMENT_UPDATED_METHOD} 的 operation 为未知值 `{other}`"),
        None => bail!("{ATTACHMENT_UPDATED_METHOD} 缺少字符串字段 operation"),
    };
    Ok(AttachmentUpdated {
        thread_id: required_string(params, "threadId", ATTACHMENT_UPDATED_METHOD)?,
        attachment_id: required_string(params, "attachmentId", ATTACHMENT_UPDATED_METHOD)?,
        attachment_type: required_string(params, "attachmentType", ATTACHMENT_UPDATED_METHOD)?,
        identity_key: required_string(params, "identityKey", ATTACHMENT_UPDATED_METHOD)?,
        operation,
    })
}

/// Whether a failed request was the server not knowing the method at all (an
/// older CLI without thread attachments). The connection formats the JSON-RPC
/// error object into the message, so its code is the signal kept.
pub(super) fn is_method_not_found(error: &anyhow::Error) -> bool {
    format!("{error:#}").contains("\"code\":-32601")
}
