//! Controlled replies for server requests that have no interactive responder.
//!
//! The early adapter used a deliberate guardrail: answer `-32601` and let the
//! generation die, so every uncovered JSON-RPC method became visible. That
//! guardrail now costs live turns and shared pending RPCs, so each request this
//! client does not implement as an interactive request is answered under its
//! original id and recorded instead:
//!
//! * dynamic tool calls (`item/tool/call`),
//! * the legacy approval protocols (auto-denied, no card),
//! * `currentTime/read` (a real local clock read),
//! * methods this client deliberately does not integrate,
//! * everything outside the schema this client was built against.
//!
//! No branch fabricates a token, a tool success, or a time, and no branch
//! answers silently: every reply carries a diagnostic record.

use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};

use super::{
    approvals::{
        APPLY_PATCH_APPROVAL_METHOD, EXEC_COMMAND_APPROVAL_METHOD, legacy_approval_denied_result,
        parse_legacy_approval_request,
    },
    client_tools::{
        ClientToolExecution, ClientToolRegistry, TOOL_CALL_METHOD, parse_dynamic_tool_call_request,
    },
    methods::summarize_json,
    requests::{optional_request_string, request_id_from_value, required_request_string},
};
use crate::agent::AgentServerRequestId;

pub(super) const CURRENT_TIME_READ_METHOD: &str = "currentTime/read";
pub(super) const AUTH_TOKENS_REFRESH_METHOD: &str = "account/chatgptAuthTokens/refresh";
pub(super) const ATTESTATION_GENERATE_METHOD: &str = "attestation/generate";

/// How one server request was answered. Recorded for every controlled reply so
/// the connection keeps an observable trace instead of dying to raise attention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ServerRequestDisposition {
    /// Legacy approval protocols: answered with an explicit denial.
    LegacyApprovalDenied,
    /// A dynamic tool call this client executed with real content items.
    ToolCallSucceeded,
    /// A known client tool this client cannot run on this host.
    ToolCallUnavailable,
    /// A dynamic tool call for a tool this client does not know.
    ToolCallUnknown,
    /// The client clock was read and answered.
    CurrentTimeRead,
    /// A method this client deliberately does not integrate.
    UnsupportedMethod,
    /// A method outside the schema this client was built against.
    UnknownMethod,
    /// The payload failed validation; answered `-32602`.
    InvalidParams,
}

/// One generation-scoped diagnostic record. It is the non-silent replacement
/// for the old "disconnect to raise attention" guardrail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ServerRequestDiagnostic {
    pub(super) method: String,
    pub(super) request_id: AgentServerRequestId,
    pub(super) thread_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) disposition: ServerRequestDisposition,
    /// Short, already-safe explanation when the reply has one (for example the
    /// capability boundary that made a tool unavailable). It never carries
    /// request payload content.
    pub(super) detail: String,
    /// Redacted, capped summary of `params`: identity strings stay readable so a
    /// reader can correlate the request, and every value that can carry content
    /// is replaced by its shape.
    pub(super) params: String,
}

/// The JSON-RPC body written under the original request id.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum ServerRequestReplyBody {
    Result(Value),
    Error { code: i64, message: String },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ControlledServerRequestReply {
    pub(super) diagnostic: ServerRequestDiagnostic,
    pub(super) body: ServerRequestReplyBody,
}

/// The JSON-RPC message for one controlled reply: the body written under the
/// request's original id, with the string/number distinction preserved.
pub(super) fn controlled_reply_message(reply: &ControlledServerRequestReply) -> Value {
    let id = super::requests::request_id_value(&reply.diagnostic.request_id);
    match &reply.body {
        ServerRequestReplyBody::Result(result) => json!({ "id": id, "result": result }),
        ServerRequestReplyBody::Error { code, message } => {
            json!({ "id": id, "error": { "code": code, "message": message } })
        }
    }
}

/// Answers one server request that has no interactive responder. Every branch
/// keeps the connection; `Err` means the payload could not be decoded for a
/// method this client answers itself, which the caller reports as `-32602`.
pub(super) fn reply_to_controlled_server_request(
    method: &str,
    message: &Value,
    tools: &ClientToolRegistry,
) -> Result<ControlledServerRequestReply> {
    match method {
        TOOL_CALL_METHOD => {
            let call = parse_dynamic_tool_call_request(message)?;
            Ok(reply_to_dynamic_tool_call(&call, tools, message))
        }
        APPLY_PATCH_APPROVAL_METHOD | EXEC_COMMAND_APPROVAL_METHOD => {
            let request = parse_legacy_approval_request(method, message)?;
            Ok(ControlledServerRequestReply {
                diagnostic: diagnostic(
                    method,
                    request.request_id.clone(),
                    Some(request.conversation_id.clone()),
                    None,
                    ServerRequestDisposition::LegacyApprovalDenied,
                    String::new(),
                    message,
                ),
                body: ServerRequestReplyBody::Result(legacy_approval_denied_result(&request)),
            })
        }
        CURRENT_TIME_READ_METHOD => {
            let (request_id, thread_id) = parse_current_time_read(message)?;
            // The schema types `threadId` but does not require the thread to be
            // loaded, and the answer is this client's own clock: refusing an
            // unknown thread id would invent a validation boundary the protocol
            // does not have.
            Ok(ControlledServerRequestReply {
                diagnostic: diagnostic(
                    method,
                    request_id,
                    Some(thread_id),
                    None,
                    ServerRequestDisposition::CurrentTimeRead,
                    String::new(),
                    message,
                ),
                body: ServerRequestReplyBody::Result(
                    json!({ "currentTimeAt": current_time_unix_seconds() }),
                ),
            })
        }
        AUTH_TOKENS_REFRESH_METHOD => {
            let (request_id, thread_id) = parse_auth_tokens_refresh(message)?;
            Ok(ControlledServerRequestReply {
                diagnostic: diagnostic(
                    method,
                    request_id,
                    thread_id,
                    None,
                    ServerRequestDisposition::UnsupportedMethod,
                    String::new(),
                    message,
                ),
                // The result type requires an access token and an account id.
                // This client only supports Codex-managed ChatGPT login and
                // holds no external token it could honestly refresh.
                body: ServerRequestReplyBody::Error {
                    code: -32601,
                    message: "This client cannot refresh ChatGPT auth tokens; it only supports Codex-managed ChatGPT login".to_owned(),
                },
            })
        }
        ATTESTATION_GENERATE_METHOD => {
            let request_id = parse_attestation_generate(message)?;
            Ok(ControlledServerRequestReply {
                diagnostic: diagnostic(
                    method,
                    request_id,
                    param_string(message, "threadId"),
                    None,
                    ServerRequestDisposition::UnsupportedMethod,
                    String::new(),
                    message,
                ),
                // `initialize` sends requestAttestation=false, so this request
                // should not arrive; if it does, the client says so instead of
                // fabricating an attestation token.
                body: ServerRequestReplyBody::Error {
                    code: -32601,
                    message: "This client does not generate attestation tokens (requestAttestation=false)".to_owned(),
                },
            })
        }
        _ => unknown_server_request_reply(method, message),
    }
}

/// The reply for one decoded dynamic tool call. A tool this client cannot run
/// answers `success=false` with no content items, which is schema-legal and lets
/// the model continue its turn; the reason is recorded in the diagnostic instead
/// of being invented as content the client does not own.
pub(super) fn reply_to_dynamic_tool_call(
    call: &super::client_tools::DynamicToolCallRequest,
    tools: &ClientToolRegistry,
    message: &Value,
) -> ControlledServerRequestReply {
    let execution = tools.execute(call);
    let (disposition, detail) = match &execution {
        ClientToolExecution::Succeeded(_) => {
            (ServerRequestDisposition::ToolCallSucceeded, String::new())
        }
        ClientToolExecution::Unavailable { reason } => (
            ServerRequestDisposition::ToolCallUnavailable,
            reason.reason().to_owned(),
        ),
        ClientToolExecution::Unknown => (
            ServerRequestDisposition::ToolCallUnknown,
            format!("未注册的客户端工具 `{}`", call.tool),
        ),
    };
    ControlledServerRequestReply {
        diagnostic: diagnostic(
            TOOL_CALL_METHOD,
            call.request_id.clone(),
            Some(call.thread_id.clone()),
            Some(call.turn_id.clone()),
            disposition,
            detail,
            message,
        ),
        body: ServerRequestReplyBody::Result(execution.result().to_value()),
    }
}

/// The fallback tier: a method this client was not built against. The original
/// id is answered and the connection stays alive, so a CLI upgrade cannot kill an
/// active turn.
fn unknown_server_request_reply(
    method: &str,
    message: &Value,
) -> Result<ControlledServerRequestReply> {
    let request_id = request_id_from_value(
        message
            .get("id")
            .context("server request 缺少 JSON-RPC id")?,
    )?;
    Ok(ControlledServerRequestReply {
        diagnostic: diagnostic(
            method,
            request_id,
            param_string(message, "threadId"),
            param_string(message, "turnId"),
            ServerRequestDisposition::UnknownMethod,
            String::new(),
            message,
        ),
        body: ServerRequestReplyBody::Error {
            code: -32601,
            message: "This client does not implement this server-initiated request".to_owned(),
        },
    })
}

/// The reply for a controlled method whose payload failed validation. It keeps
/// the connection: unlike the interactive methods, a malformed payload for an
/// auto-answered method is a request-level error, not a fatal protocol error.
pub(super) fn invalid_params_reply(
    method: &str,
    message: &Value,
    request_id: AgentServerRequestId,
) -> ControlledServerRequestReply {
    ControlledServerRequestReply {
        diagnostic: diagnostic(
            method,
            request_id,
            param_string(message, "threadId").or_else(|| param_string(message, "conversationId")),
            param_string(message, "turnId"),
            ServerRequestDisposition::InvalidParams,
            format!("{method} params 校验失败，已按原 id 回复 -32602"),
            message,
        ),
        body: ServerRequestReplyBody::Error {
            code: -32602,
            message: format!("Invalid {method} params"),
        },
    }
}

/// Whole Unix seconds from this machine's clock.
fn current_time_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

fn parse_current_time_read(message: &Value) -> Result<(AgentServerRequestId, String)> {
    const METHOD: &str = CURRENT_TIME_READ_METHOD;
    let request_id = request_id_from_value(
        message
            .get("id")
            .context("currentTime/read 缺少 JSON-RPC id")?,
    )?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("currentTime/read 缺少对象 params")?;
    Ok((
        request_id,
        required_request_string(params, METHOD, "threadId")?,
    ))
}

fn parse_auth_tokens_refresh(message: &Value) -> Result<(AgentServerRequestId, Option<String>)> {
    const METHOD: &str = AUTH_TOKENS_REFRESH_METHOD;
    let request_id = request_id_from_value(
        message
            .get("id")
            .context("account/chatgptAuthTokens/refresh 缺少 JSON-RPC id")?,
    )?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("account/chatgptAuthTokens/refresh 缺少对象 params")?;
    let previous_account_id = optional_request_string(params, METHOD, "previousAccountId")?;
    let reason = required_request_string(params, METHOD, "reason")?;
    if reason != "unauthorized" {
        bail!("{METHOD} params.reason 包含未知值 `{reason}`");
    }
    Ok((request_id, previous_account_id))
}

fn parse_attestation_generate(message: &Value) -> Result<AgentServerRequestId> {
    let request_id = request_id_from_value(
        message
            .get("id")
            .context("attestation/generate 缺少 JSON-RPC id")?,
    )?;
    // `AttestationGenerateParams` is an empty object and stays required: a
    // payload this client cannot decode is answered `-32602`, never treated as
    // understood.
    if !matches!(message.get("params"), Some(Value::Object(_))) {
        bail!("attestation/generate params 必须是对象");
    }
    Ok(request_id)
}

fn diagnostic(
    method: &str,
    request_id: AgentServerRequestId,
    thread_id: Option<String>,
    turn_id: Option<String>,
    disposition: ServerRequestDisposition,
    detail: String,
    message: &Value,
) -> ServerRequestDiagnostic {
    ServerRequestDiagnostic {
        method: method.to_owned(),
        request_id,
        thread_id,
        turn_id,
        disposition,
        detail,
        params: summarize_request_params(message.get("params")),
    }
}

fn param_string(message: &Value, field: &str) -> Option<String> {
    message
        .pointer(&format!("/params/{field}"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Keys whose values identify a request instead of carrying content.
const IDENTIFIER_PARAMS: &[&str] = &[
    "threadId",
    "turnId",
    "callId",
    "conversationId",
    "itemId",
    "approvalId",
    "environmentId",
    "tool",
    "namespace",
];
const IDENTIFIER_VALUE_LIMIT: usize = 96;
const PARAMS_SUMMARY_LIMIT: usize = 512;

/// Redacted summary of one request's `params` for the diagnostic record.
/// Identity strings stay readable so a reader can correlate the request with a
/// thread or a tool; every other value is replaced by its shape, so patch
/// contents, commands, tool arguments, and any secret they carry never enter the
/// record.
fn summarize_request_params(params: Option<&Value>) -> String {
    let Some(Value::Object(params)) = params else {
        return params
            .map(summarize_json)
            .unwrap_or_else(|| "null".to_owned());
    };
    let mut keys = params.keys().collect::<Vec<_>>();
    keys.sort();
    let rendered = keys
        .into_iter()
        .map(|key| {
            let value = &params[key];
            let rendered = match value {
                Value::String(text) if IDENTIFIER_PARAMS.contains(&key.as_str()) => {
                    format!("\"{}\"", truncate(text, IDENTIFIER_VALUE_LIMIT))
                }
                value => shape(value),
            };
            format!("{key}={rendered}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    truncate(&rendered, PARAMS_SUMMARY_LIMIT)
}

fn shape(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(_) => "bool".to_owned(),
        Value::Number(_) => "number".to_owned(),
        Value::String(text) => format!("string({})", text.chars().count()),
        Value::Array(items) => format!("array({})", items.len()),
        Value::Object(object) => format!("object({})", object.len()),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    let mut characters = text.chars();
    let mut rendered: String = characters.by_ref().take(limit).collect();
    if characters.next().is_some() {
        rendered.push('\u{2026}');
    }
    rendered
}
