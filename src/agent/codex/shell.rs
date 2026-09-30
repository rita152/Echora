//! `thread/shellCommand` and `thread/backgroundTerminals/clean` for the
//! baseline CLI schema. Both responses are only acknowledgements: a shell
//! command's output arrives as a `commandExecution` item with
//! `source: userShell` in the thread's turn, and a cleaned background terminal
//! reports its end through its own `item/completed`.

use anyhow::{Context as _, Result};
use serde_json::{Map, Value, json};

pub(super) const SHELL_COMMAND_METHOD: &str = "thread/shellCommand";

pub(super) fn shell_params(thread_id: &str, command: &str, timeout_ms: Option<u64>) -> Value {
    let mut params = Map::new();
    params.insert("threadId".into(), json!(thread_id));
    params.insert("command".into(), json!(command));
    if let Some(timeout) = timeout_ms {
        params.insert("timeoutMs".into(), json!(timeout));
    }
    Value::Object(params)
}

pub(super) fn parse_shell_ack(response: &Value) -> Result<()> {
    response
        .get("result")
        .and_then(Value::as_object)
        .map(|_| ())
        .context("thread/shellCommand 响应缺少对象 result")
}

pub(super) const BACKGROUND_TERMINALS_CLEAN_METHOD: &str = "thread/backgroundTerminals/clean";

pub(super) fn clean_params(thread_id: &str) -> Value {
    json!({ "threadId": thread_id })
}

/// `thread/backgroundTerminals/clean` answers `{}` once the thread's background
/// terminals were told to stop; each command's final state arrives afterwards
/// as its own `item/completed`.
pub(super) fn parse_clean_ack(response: &Value) -> Result<()> {
    response
        .get("result")
        .and_then(Value::as_object)
        .map(|_| ())
        .context("thread/backgroundTerminals/clean 响应缺少对象 result")
}
