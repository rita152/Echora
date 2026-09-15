//! File and command approval codecs for the installed app-server schema.

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};

use super::requests::{
    optional_request_string, parse_additional_file_system_permissions,
    parse_additional_network_permissions, parse_optional_field, request_id_from_value,
    required_request_string,
};
use crate::agent::{
    AgentCommandApprovalChoice, AgentCommandApprovalKind, AgentCommandApprovalRequest,
    AgentFileApprovalRequest, AgentNetworkApprovalContext, AgentNetworkApprovalProtocol,
    AgentNetworkPolicyAction, AgentNetworkPolicyAmendment, AgentPermissionRequestProfile,
    AgentServerRequestId,
};

pub(super) const APPLY_PATCH_APPROVAL_METHOD: &str = "applyPatchApproval";
pub(super) const EXEC_COMMAND_APPROVAL_METHOD: &str = "execCommandApproval";

pub(super) const LEGACY_APPROVAL_METHODS: &[&str] =
    &[APPLY_PATCH_APPROVAL_METHOD, EXEC_COMMAND_APPROVAL_METHOD];

fn string_list(value: &Value) -> Result<Vec<String>> {
    value
        .as_array()
        .context("必须是字符串数组")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("数组元素必须是字符串")
        })
        .collect()
}

fn optional_list<T>(
    params: &Map<String, Value>,
    field: &str,
    parse: impl Fn(&Value) -> Result<T>,
) -> Result<Vec<T>> {
    match params.get(field) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) => values.iter().map(parse).collect(),
        Some(_) => bail!("approval params.{field} 必须是数组或 null"),
    }
}

fn network_amendment(value: &Value) -> Result<AgentNetworkPolicyAmendment> {
    let object = value
        .as_object()
        .context("network policy amendment 必须是对象")?;
    let host = required_request_string(object, "network policy amendment", "host")?;
    let action = match object.get("action").and_then(Value::as_str) {
        Some("allow") => AgentNetworkPolicyAction::Allow,
        Some("deny") => AgentNetworkPolicyAction::Deny,
        _ => bail!("network policy amendment.action 必须是 allow 或 deny"),
    };
    Ok(AgentNetworkPolicyAmendment { host, action })
}

fn command_choice(value: &Value) -> Result<AgentCommandApprovalChoice> {
    use AgentCommandApprovalChoice as Choice;
    match value.as_str() {
        Some("accept") => return Ok(Choice::Accept),
        Some("acceptForSession") => return Ok(Choice::AcceptForSession),
        Some("decline") => return Ok(Choice::Decline),
        Some("cancel") => return Ok(Choice::Cancel),
        _ => {}
    }
    let object = value
        .as_object()
        .context("未知 command approval decision")?;
    if object.len() != 1 {
        bail!("command approval decision 必须且只能包含一个决策");
    }
    if let Some(amendment) = object.get("acceptWithExecpolicyAmendment") {
        return Ok(Choice::AcceptWithExecpolicyAmendment(string_list(
            amendment
                .get("execpolicy_amendment")
                .context("decision 缺少 execpolicy_amendment")?,
        )?));
    }
    if let Some(amendment) = object.get("applyNetworkPolicyAmendment") {
        return Ok(Choice::ApplyNetworkPolicyAmendment(network_amendment(
            amendment
                .get("network_policy_amendment")
                .context("decision 缺少 network_policy_amendment")?,
        )?));
    }
    bail!("未知 command approval decision")
}

pub(super) fn command_choice_value(choice: &AgentCommandApprovalChoice) -> Value {
    use AgentCommandApprovalChoice as Choice;
    match choice {
        Choice::Accept => json!("accept"),
        Choice::AcceptForSession => json!("acceptForSession"),
        Choice::Decline => json!("decline"),
        Choice::Cancel => json!("cancel"),
        Choice::AcceptWithExecpolicyAmendment(amendment) => json!({
            "acceptWithExecpolicyAmendment": {"execpolicy_amendment": amendment}
        }),
        Choice::ApplyNetworkPolicyAmendment(amendment) => json!({
            "applyNetworkPolicyAmendment": {"network_policy_amendment": {
                "host": amendment.host,
                "action": match amendment.action { AgentNetworkPolicyAction::Allow => "allow", AgentNetworkPolicyAction::Deny => "deny" }
            }}
        }),
    }
}

pub(super) fn parse_command_approval_request(
    message: &Value,
) -> Result<(
    AgentServerRequestId,
    AgentCommandApprovalRequest,
    Value,
    Vec<Value>,
)> {
    const METHOD: &str = "item/commandExecution/requestApproval";
    let request_id = request_id_from_value(message.get("id").context("command approval 缺少 id")?)?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("command approval 缺少对象 params")?;
    let kind = match params.get("kind") {
        None => AgentCommandApprovalKind::Command,
        Some(Value::String(kind)) if kind == "command" => AgentCommandApprovalKind::Command,
        Some(Value::String(kind)) if kind == "writeStdin" => AgentCommandApprovalKind::WriteStdin,
        _ => bail!("command approval params.kind 必须是 command 或 writeStdin"),
    };
    let network = match params.get("networkApprovalContext") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let object = value
                .as_object()
                .context("networkApprovalContext 必须是对象或 null")?;
            let host = required_request_string(object, METHOD, "host")?;
            let protocol = match object.get("protocol").and_then(Value::as_str) {
                Some("http") => AgentNetworkApprovalProtocol::Http,
                Some("https") => AgentNetworkApprovalProtocol::Https,
                Some("socks5Tcp") => AgentNetworkApprovalProtocol::Socks5Tcp,
                Some("socks5Udp") => AgentNetworkApprovalProtocol::Socks5Udp,
                _ => bail!("networkApprovalContext.protocol 未定义或缺失"),
            };
            Some(AgentNetworkApprovalContext { host, protocol })
        }
    };
    let additional_permissions = parse_optional_field(params, "additionalPermissions", |value| {
        let object = value
            .as_object()
            .context("additionalPermissions 必须是对象或 null")?;
        if let Some(field) = object
            .keys()
            .find(|field| !matches!(field.as_str(), "fileSystem" | "network"))
        {
            bail!("additionalPermissions 包含未定义权限字段 `{field}`");
        }
        Ok(AgentPermissionRequestProfile {
            file_system: parse_optional_field(
                object,
                "fileSystem",
                parse_additional_file_system_permissions,
            )?,
            network: parse_optional_field(object, "network", parse_additional_network_permissions)?,
        })
    })?;
    let proposed_exec = match params.get("proposedExecpolicyAmendment") {
        None | Some(Value::Null) => None,
        Some(value) => Some(string_list(value).context("proposedExecpolicyAmendment 无效")?),
    };
    let proposed_network =
        optional_list(params, "proposedNetworkPolicyAmendments", network_amendment)?;
    let decisions = match params.get("availableDecisions") {
        None | Some(Value::Null) => {
            use AgentCommandApprovalChoice as Choice;
            let mut choices = vec![Choice::Accept, Choice::AcceptForSession];
            if let Some(amendment) = proposed_exec {
                choices.push(Choice::AcceptWithExecpolicyAmendment(amendment));
            }
            choices.extend(
                proposed_network
                    .into_iter()
                    .map(Choice::ApplyNetworkPolicyAmendment),
            );
            choices.extend([Choice::Decline, Choice::Cancel]);
            choices
        }
        Some(Value::Array(values)) => values
            .iter()
            .map(command_choice)
            .collect::<Result<Vec<_>>>()?,
        Some(_) => bail!("command approval params.availableDecisions 必须是数组或 null"),
    };
    let actions = optional_list(params, "commandActions", |value| {
        let object = value.as_object().context("commandActions 元素必须是对象")?;
        let command = required_request_string(object, METHOD, "command")?;
        match object.get("type").and_then(Value::as_str) {
            Some("read") => {
                required_request_string(object, METHOD, "name")?;
                required_request_string(object, METHOD, "path")?;
            }
            Some("listFiles") => {
                optional_request_string(object, METHOD, "path")?;
            }
            Some("search") => {
                optional_request_string(object, METHOD, "path")?;
                optional_request_string(object, METHOD, "query")?;
            }
            Some("unknown") => {}
            _ => bail!("commandActions.type 未定义或缺失"),
        }
        Ok(command)
    })?;
    let command = optional_request_string(params, METHOD, "command")?
        .or_else(|| actions.into_iter().next())
        .unwrap_or_default();
    let request = AgentCommandApprovalRequest {
        request_id: request_id.clone(),
        thread_id: required_request_string(params, METHOD, "threadId")?,
        turn_id: required_request_string(params, METHOD, "turnId")?,
        item_id: required_request_string(params, METHOD, "itemId")?,
        approval_id: optional_request_string(params, METHOD, "approvalId")?,
        kind,
        environment_id: optional_request_string(params, METHOD, "environmentId")?,
        started_at_ms: params
            .get("startedAtMs")
            .and_then(Value::as_i64)
            .context("command approval params.startedAtMs 必须是 int64")?,
        cwd: optional_request_string(params, METHOD, "cwd")?,
        command,
        reason: optional_request_string(params, METHOD, "reason")?,
        network,
        additional_permissions,
        available_decisions: decisions.clone(),
    };
    Ok((
        request_id,
        request,
        Value::Object(params.clone()),
        decisions.iter().map(command_choice_value).collect(),
    ))
}

pub(super) fn parse_file_approval_request(message: &Value) -> Result<AgentFileApprovalRequest> {
    const METHOD: &str = "item/fileChange/requestApproval";
    let request_id = request_id_from_value(message.get("id").context("file approval 缺少 id")?)?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("file approval 缺少对象 params")?;
    Ok(AgentFileApprovalRequest {
        request_id,
        thread_id: required_request_string(params, METHOD, "threadId")?,
        turn_id: required_request_string(params, METHOD, "turnId")?,
        item_id: required_request_string(params, METHOD, "itemId")?,
        started_at_ms: params
            .get("startedAtMs")
            .and_then(Value::as_i64)
            .context("file approval params.startedAtMs 必须是 int64")?,
        reason: optional_request_string(params, METHOD, "reason")?,
        grant_root: optional_request_string(params, METHOD, "grantRoot")?,
    })
}

/// A parsed legacy approval request. Both legacy methods answer with the same
/// automatic denial, so only the identity that shapes the reply and the
/// connection diagnostic is kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LegacyApprovalRequest {
    pub(super) method: &'static str,
    pub(super) request_id: AgentServerRequestId,
    pub(super) call_id: String,
    /// `conversationId` on the wire. The v2 API calls the same identity
    /// `threadId`; the legacy protocol has no turn identity.
    pub(super) conversation_id: String,
}

pub(super) fn parse_legacy_approval_request(
    method: &str,
    message: &Value,
) -> Result<LegacyApprovalRequest> {
    match method {
        APPLY_PATCH_APPROVAL_METHOD => parse_legacy_apply_patch_approval(message),
        EXEC_COMMAND_APPROVAL_METHOD => parse_legacy_exec_command_approval(message),
        method => bail!("`{method}` 不是旧版审批请求"),
    }
}

fn parse_legacy_apply_patch_approval(message: &Value) -> Result<LegacyApprovalRequest> {
    const METHOD: &str = APPLY_PATCH_APPROVAL_METHOD;
    let request_id =
        request_id_from_value(message.get("id").context("applyPatchApproval 缺少 id")?)?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("applyPatchApproval 缺少对象 params")?;
    let call_id = required_request_string(params, METHOD, "callId")?;
    let conversation_id = required_request_string(params, METHOD, "conversationId")?;
    // File changes are validated by variant even though every legacy request is
    // denied: a payload this client cannot decode must not be answered as if it
    // had been understood.
    let file_changes = params
        .get("fileChanges")
        .and_then(Value::as_object)
        .context("applyPatchApproval params.fileChanges 必须是对象")?;
    for (path, change) in file_changes {
        validate_legacy_file_change(path, change)?;
    }
    optional_request_string(params, METHOD, "grantRoot")?;
    optional_request_string(params, METHOD, "reason")?;
    Ok(LegacyApprovalRequest {
        method: METHOD,
        request_id,
        call_id,
        conversation_id,
    })
}

fn validate_legacy_file_change(path: &str, value: &Value) -> Result<()> {
    let change = value
        .as_object()
        .with_context(|| format!("applyPatchApproval params.fileChanges[`{path}`] 必须是对象"))?;
    match change.get("type").and_then(Value::as_str) {
        Some("add" | "delete") => {
            if change.get("content").and_then(Value::as_str).is_none() {
                bail!("applyPatchApproval params.fileChanges[`{path}`].content 必须是字符串");
            }
        }
        Some("update") => {
            if change.get("unified_diff").and_then(Value::as_str).is_none() {
                bail!("applyPatchApproval params.fileChanges[`{path}`].unified_diff 必须是字符串");
            }
            match change.get("move_path") {
                None | Some(Value::Null) | Some(Value::String(_)) => {}
                Some(_) => bail!(
                    "applyPatchApproval params.fileChanges[`{path}`].move_path 必须是字符串或 null"
                ),
            }
        }
        Some(other) => {
            bail!("applyPatchApproval params.fileChanges[`{path}`].type 包含未知值 `{other}`")
        }
        None => bail!("applyPatchApproval params.fileChanges[`{path}`].type 必须是字符串"),
    }
    Ok(())
}

fn parse_legacy_exec_command_approval(message: &Value) -> Result<LegacyApprovalRequest> {
    const METHOD: &str = EXEC_COMMAND_APPROVAL_METHOD;
    let request_id =
        request_id_from_value(message.get("id").context("execCommandApproval 缺少 id")?)?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("execCommandApproval 缺少对象 params")?;
    let call_id = required_request_string(params, METHOD, "callId")?;
    let conversation_id = required_request_string(params, METHOD, "conversationId")?;
    optional_request_string(params, METHOD, "approvalId")?;
    required_request_string(params, METHOD, "cwd")?;
    optional_request_string(params, METHOD, "reason")?;
    let command = params
        .get("command")
        .and_then(Value::as_array)
        .context("execCommandApproval params.command 必须是字符串数组")?;
    for (index, value) in command.iter().enumerate() {
        if value.as_str().is_none() {
            bail!("execCommandApproval params.command[{index}] 必须是字符串");
        }
    }
    let parsed = params
        .get("parsedCmd")
        .and_then(Value::as_array)
        .context("execCommandApproval params.parsedCmd 必须是数组")?;
    for (index, value) in parsed.iter().enumerate() {
        validate_parsed_command(index, value)?;
    }
    Ok(LegacyApprovalRequest {
        method: METHOD,
        request_id,
        call_id,
        conversation_id,
    })
}

fn validate_parsed_command(index: usize, value: &Value) -> Result<()> {
    let command = value
        .as_object()
        .with_context(|| format!("execCommandApproval params.parsedCmd[{index}] 必须是对象"))?;
    match command.get("type").and_then(Value::as_str) {
        Some("read") => {
            for field in ["cmd", "name", "path"] {
                if command.get(field).and_then(Value::as_str).is_none() {
                    bail!("execCommandApproval params.parsedCmd[{index}].{field} 必须是字符串");
                }
            }
        }
        Some("list_files" | "search") => {
            if command.get("cmd").and_then(Value::as_str).is_none() {
                bail!("execCommandApproval params.parsedCmd[{index}].cmd 必须是字符串");
            }
            for field in ["path", "query"] {
                match command.get(field) {
                    None | Some(Value::Null) | Some(Value::String(_)) => {}
                    Some(_) => bail!(
                        "execCommandApproval params.parsedCmd[{index}].{field} 必须是字符串或 null"
                    ),
                }
            }
        }
        Some("unknown") => {
            if command.get("cmd").and_then(Value::as_str).is_none() {
                bail!("execCommandApproval params.parsedCmd[{index}].cmd 必须是字符串");
            }
        }
        Some(other) => {
            bail!("execCommandApproval params.parsedCmd[{index}].type 包含未知值 `{other}`")
        }
        None => bail!("execCommandApproval params.parsedCmd[{index}].type 必须是字符串"),
    }
    Ok(())
}

/// The controlled decision for a legacy approval. These methods belong to turns
/// this client never starts, and the reference client shows no card for them:
/// denying is the safe default, and the text tells the model why instead of
/// pretending the request was approved or silently dropping it.
pub(super) fn legacy_approval_denied_result(request: &LegacyApprovalRequest) -> Value {
    let subject = match request.method {
        EXEC_COMMAND_APPROVAL_METHOD => "command",
        _ => "file change",
    };
    json!({
        "decision": {
            "denied": {
                "rejection": format!(
                    "This client provides no approval UI for the legacy `{}` protocol, so the {subject} was automatically denied. Continue the turn without it.",
                    request.method
                )
            }
        }
    })
}
