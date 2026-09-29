//! `hooks/list` for the baseline CLI schema.
//!
//! `HookMetadata` flattens its handler into the hook object: `handlerType`
//! selects which of `command`/`async` or `server`/`tool` are present.

use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};

use super::json::{array, object, optional_string, required_bool, required_enum, required_string};
use crate::agent::{
    AgentHook, AgentHookEventName, AgentHookHandler, AgentHookListEntry, AgentHookLoadError,
    AgentHookSource, AgentHookTrustStatus, AgentHooksSnapshot,
};

pub(super) const HOOKS_LIST_METHOD: &str = "hooks/list";

pub(super) fn list_params(cwds: &[PathBuf]) -> Value {
    json!({ "cwds": cwds.iter().map(|cwd| cwd.display().to_string()).collect::<Vec<_>>() })
}

fn event_wire(event: AgentHookEventName) -> &'static str {
    match event {
        AgentHookEventName::PreToolUse => "preToolUse",
        AgentHookEventName::PermissionRequest => "permissionRequest",
        AgentHookEventName::PostToolUse => "postToolUse",
        AgentHookEventName::PreCompact => "preCompact",
        AgentHookEventName::PostCompact => "postCompact",
        AgentHookEventName::SessionStart => "sessionStart",
        AgentHookEventName::SessionEnd => "sessionEnd",
        AgentHookEventName::UserPromptSubmit => "userPromptSubmit",
        AgentHookEventName::SubagentStart => "subagentStart",
        AgentHookEventName::SubagentStop => "subagentStop",
        AgentHookEventName::Stop => "stop",
        AgentHookEventName::Interrupt => "interrupt",
    }
}

fn source_wire(source: AgentHookSource) -> &'static str {
    match source {
        AgentHookSource::System => "system",
        AgentHookSource::User => "user",
        AgentHookSource::Project => "project",
        AgentHookSource::Mdm => "mdm",
        AgentHookSource::SessionFlags => "sessionFlags",
        AgentHookSource::Plugin => "plugin",
        AgentHookSource::CloudRequirements => "cloudRequirements",
        AgentHookSource::CloudManagedConfig => "cloudManagedConfig",
        AgentHookSource::LegacyManagedConfigFile => "legacyManagedConfigFile",
        AgentHookSource::LegacyManagedConfigMdm => "legacyManagedConfigMdm",
        AgentHookSource::Unknown => "unknown",
    }
}

fn parse_trust(raw: &str) -> Option<AgentHookTrustStatus> {
    match raw {
        "managed" => Some(AgentHookTrustStatus::Managed),
        "untrusted" => Some(AgentHookTrustStatus::Untrusted),
        "trusted" => Some(AgentHookTrustStatus::Trusted),
        "modified" => Some(AgentHookTrustStatus::Modified),
        _ => None,
    }
}

fn unsigned(object: &Map<String, Value>, field: &str, context: &str) -> Result<u64> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .with_context(|| format!("{context} 缺少非负整数字段 {field}"))
}

fn optional_unsigned(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<u64>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .with_context(|| format!("{context} 的 {field} 必须是非负整数或 null")),
    }
}

fn parse_handler(hook: &Map<String, Value>) -> Result<AgentHookHandler> {
    const CONTEXT: &str = "hooks/list hook";
    Ok(
        match required_string(hook, "handlerType", CONTEXT)?.as_str() {
            "command" => AgentHookHandler::Command {
                command: required_string(hook, "command", CONTEXT)?,
                // The schema defaults `async` to false.
                is_async: match hook.get("async") {
                    None => false,
                    Some(Value::Bool(value)) => *value,
                    Some(_) => bail!("{CONTEXT} 的 async 必须是布尔值"),
                },
            },
            "mcpTool" => AgentHookHandler::McpTool {
                server: required_string(hook, "server", CONTEXT)?,
                tool: required_string(hook, "tool", CONTEXT)?,
            },
            "prompt" => AgentHookHandler::Prompt,
            "agent" => AgentHookHandler::Agent,
            other => bail!("{CONTEXT} 的 handlerType 为未知值 `{other}`"),
        },
    )
}

fn parse_hook(value: &Value) -> Result<AgentHook> {
    const CONTEXT: &str = "hooks/list hook";
    let hook = object(value, CONTEXT)?;
    let source_path = PathBuf::from(required_string(hook, "sourcePath", CONTEXT)?);
    if !source_path.is_absolute() {
        bail!("{CONTEXT} 的 sourcePath 必须是绝对路径");
    }
    Ok(AgentHook {
        key: required_string(hook, "key", CONTEXT)?,
        event_name: required_enum(hook, "eventName", CONTEXT, |raw| {
            AgentHookEventName::ALL
                .into_iter()
                .find(|event| event_wire(*event) == raw)
        })?,
        handler: parse_handler(hook)?,
        matcher: optional_string(hook, "matcher", CONTEXT)?,
        timeout_sec: unsigned(hook, "timeoutSec", CONTEXT)?,
        status_message: optional_string(hook, "statusMessage", CONTEXT)?,
        source: required_enum(hook, "source", CONTEXT, |raw| {
            AgentHookSource::ALL
                .into_iter()
                .find(|source| source_wire(*source) == raw)
        })?,
        source_path,
        plugin_id: optional_string(hook, "pluginId", CONTEXT)?,
        display_order: hook
            .get("displayOrder")
            .and_then(Value::as_i64)
            .context("hooks/list hook 缺少整数字段 displayOrder")?,
        enabled: required_bool(hook, "enabled", CONTEXT)?,
        is_managed: required_bool(hook, "isManaged", CONTEXT)?,
        current_hash: required_string(hook, "currentHash", CONTEXT)?,
        trust_status: required_enum(hook, "trustStatus", CONTEXT, parse_trust)?,
        additional_context_limit: optional_unsigned(hook, "additionalContextLimit", CONTEXT)?,
    })
}

fn strings(entry: &Map<String, Value>, field: &str) -> Result<Vec<String>> {
    array(
        entry
            .get(field)
            .with_context(|| format!("hooks/list entry 缺少 {field}"))?,
        field,
    )?
    .iter()
    .map(|value| {
        value
            .as_str()
            .map(str::to_owned)
            .with_context(|| format!("hooks/list entry 的 {field} 必须是字符串数组"))
    })
    .collect()
}

fn parse_entry(value: &Value) -> Result<AgentHookListEntry> {
    const CONTEXT: &str = "hooks/list entry";
    let entry = object(value, CONTEXT)?;
    let hooks = array(
        entry.get("hooks").context("hooks/list entry 缺少 hooks")?,
        "hooks",
    )?
    .iter()
    .map(parse_hook)
    .collect::<Result<Vec<_>>>()?;
    let errors = array(
        entry
            .get("errors")
            .context("hooks/list entry 缺少 errors")?,
        "errors",
    )?
    .iter()
    .map(|error| {
        let error = object(error, "hooks/list error")?;
        Ok(AgentHookLoadError {
            path: required_string(error, "path", "hooks/list error")?,
            message: required_string(error, "message", "hooks/list error")?,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    Ok(AgentHookListEntry {
        cwd: PathBuf::from(required_string(entry, "cwd", CONTEXT)?),
        hooks,
        warnings: strings(entry, "warnings")?,
        errors,
    })
}

pub(super) fn parse_list_response(
    generation: u64,
    cwds: &[PathBuf],
    response: &Value,
) -> Result<AgentHooksSnapshot> {
    let result = object(
        response
            .get("result")
            .context("hooks/list 响应缺少 result")?,
        "hooks/list result",
    )?;
    let entries = array(
        result.get("data").context("hooks/list 响应缺少 data")?,
        "data",
    )?
    .iter()
    .map(parse_entry)
    .collect::<Result<Vec<_>>>()?;
    let mut keys = std::collections::HashSet::new();
    for entry in &entries {
        for hook in &entry.hooks {
            // The same user-layer hook is listed again for every cwd; only a
            // key repeated within one entry is inconsistent.
            if !keys.insert((entry.cwd.clone(), hook.key.clone())) {
                bail!(
                    "hooks/list 在 {} 中重复返回钩子 {}",
                    entry.cwd.display(),
                    hook.key
                );
            }
        }
    }
    Ok(AgentHooksSnapshot {
        generation,
        cwds: cwds.to_vec(),
        entries,
    })
}

#[cfg(test)]
mod tests;
