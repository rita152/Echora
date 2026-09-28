//! `collaborationMode/list` decoding and the `turn/start` collaborationMode
//! object built from a preset.

use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};

use super::json::{array, object, required_string};
use crate::agent::{AgentCollaborationModeKind, AgentCollaborationModePreset};

pub(super) const METHOD: &str = "collaborationMode/list";

fn mode_wire(mode: AgentCollaborationModeKind) -> &'static str {
    match mode {
        AgentCollaborationModeKind::Plan => "plan",
        AgentCollaborationModeKind::Default => "default",
    }
}

/// Decodes the preset list. Presets without a mode are not selectable and are
/// dropped; the first preset of each mode wins; the result is ordered plan,
/// default, the order the reference shows them in.
pub(super) fn parse_list_response(response: &Value) -> Result<Vec<AgentCollaborationModePreset>> {
    let result = object(
        response
            .get("result")
            .context("collaborationMode/list 响应缺少 result")?,
        "collaborationMode/list result",
    )?;
    let data = array(
        result
            .get("data")
            .context("collaborationMode/list 响应缺少 data")?,
        "collaborationMode/list data",
    )?;
    let mut presets: Vec<AgentCollaborationModePreset> = Vec::new();
    for entry in data {
        let entry = object(entry, "CollaborationModeMask")?;
        let name = required_string(entry, "name", "CollaborationModeMask")?;
        let mode = match entry.get("mode") {
            None | Some(Value::Null) => None,
            Some(Value::String(mode)) => Some(match mode.as_str() {
                "plan" => AgentCollaborationModeKind::Plan,
                "default" => AgentCollaborationModeKind::Default,
                other => bail!("CollaborationModeMask 的 mode 为未知值 `{other}`"),
            }),
            Some(_) => bail!("CollaborationModeMask 的 mode 必须是字符串或 null"),
        };
        let model = match entry.get("model") {
            None | Some(Value::Null) => None,
            Some(Value::String(model)) => Some(model.clone()),
            Some(_) => bail!("CollaborationModeMask 的 model 必须是字符串或 null"),
        };
        let reasoning_effort = match entry.get("reasoning_effort") {
            None | Some(Value::Null) => None,
            Some(Value::String(effort)) if !effort.is_empty() => Some(effort.clone()),
            Some(_) => bail!("CollaborationModeMask 的 reasoning_effort 必须是非空字符串或 null"),
        };
        let Some(mode) = mode else {
            continue;
        };
        if presets.iter().any(|preset| preset.mode == mode) {
            continue;
        }
        presets.push(AgentCollaborationModePreset {
            name,
            mode,
            model,
            reasoning_effort,
        });
    }
    presets.sort_by_key(|preset| match preset.mode {
        AgentCollaborationModeKind::Plan => 0,
        AgentCollaborationModeKind::Default => 1,
    });
    Ok(presets)
}

/// The `turn/start` collaborationMode for a mode. The reference keeps only
/// the preset's mode: the user's model and effort always win and developer
/// instructions are never sent. When presets are unavailable the default mode
/// is still sent, as the reference falls back to it; plan needs its preset.
pub(super) fn turn_collaboration_mode(
    presets: &[AgentCollaborationModePreset],
    mode: AgentCollaborationModeKind,
    model: &str,
    effort: &str,
) -> Result<Value> {
    if mode == AgentCollaborationModeKind::Plan && !presets.iter().any(|preset| preset.mode == mode)
    {
        bail!("当前连接没有提供 Plan 协作模式，无法以计划模式发送。输入已保留。");
    }
    Ok(json!({
        "mode": mode_wire(mode),
        "settings": {
            "model": model,
            "reasoning_effort": effort,
            "developer_instructions": null,
        }
    }))
}

#[cfg(test)]
mod tests;
