//! Derive the preview from real current-turn MCP activity, including calls
//! whose result has not arrived yet. A changed surface never inherits an image.

use super::super::mcp::{is_computer_use_call, mcp_tool_call_label};
use crate::{
    agent::{AgentMcpToolCall, AgentMcpToolCallStatus},
    computer_use::Target,
    conversation::ConversationActivity,
};
use std::hash::{Hash, Hasher};

pub(super) fn target(call: &AgentMcpToolCall) -> Option<Target> {
    let surface = call
        .result
        .as_ref()?
        .get("_meta")?
        .get("codex/toolSurface")?;
    if surface.get("kind").and_then(serde_json::Value::as_str) != Some("computerUse") {
        return None;
    }
    let app = surface.get("app")?;
    if let Some(value) = app
        .get("appId")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(Target::AppId(value.into()))
    } else {
        app.get("displayName")
            .or_else(|| app.get("name"))
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|value| Target::DisplayName(value.into()))
    }
}

#[derive(Clone)]
pub(super) struct ToolFrame {
    pub key: String,
    pub data: String,
    pub mime: Option<String>,
}

pub(super) struct Snapshot {
    pub target: Option<Target>,
    pub label: String,
    pub title: String,
    pub status: AgentMcpToolCallStatus,
    pub progress: Option<String>,
    pub error: Option<String>,
    pub frame: Option<ToolFrame>,
    pub operations: Vec<(String, AgentMcpToolCallStatus)>,
}

impl Snapshot {
    pub fn from_activities(activities: &[ConversationActivity]) -> Option<Self> {
        let calls = activities
            .iter()
            .filter_map(|activity| match activity {
                ConversationActivity::McpToolCall(call) if is_computer_use_call(call) => {
                    Some(call.as_ref())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let latest = *calls.last()?;
        // An unfinished call has no surface yet. Keep the last known surface
        // only until this call supplies its own metadata (including app:null).
        let surface = calls.iter().rev().find(|call| {
            call.result
                .as_ref()
                .and_then(|r| r.get("_meta"))
                .and_then(|m| m.get("codex/toolSurface"))
                .is_some()
        });
        let active_target = surface.and_then(|call| target(call));
        let label = active_target
            .as_ref()
            .map_or_else(|| "Computer Use".into(), |t| t.label().to_owned());
        let frame = calls.iter().rev().find_map(|call| {
            if target(call) != active_target {
                return None;
            }
            call.result
                .as_ref()?
                .get("content")?
                .as_array()?
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, part)| {
                    if part.get("type")?.as_str()? != "image" {
                        return None;
                    }
                    let data = part.get("data")?.as_str()?;
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    data.hash(&mut hash);
                    Some(ToolFrame {
                        key: format!("{}-{index}-{}", call.id, hash.finish()),
                        data: data.to_owned(),
                        mime: part
                            .get("mimeType")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned),
                    })
                })
        });
        Some(Self {
            target: active_target,
            label,
            title: mcp_tool_call_label(latest),
            status: latest.status,
            progress: latest.progress.last().cloned(),
            error: latest.error.clone(),
            frame,
            operations: calls
                .iter()
                .rev()
                .take(8)
                .rev()
                .map(|call| (mcp_tool_call_label(call), call.status))
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str, name: Option<&str>, image: bool) -> ConversationActivity {
        ConversationActivity::McpToolCall(Box::new(AgentMcpToolCall {
            id: id.into(),
            server: "cua_repl".into(),
            tool: "js".into(),
            status: AgentMcpToolCallStatus::Completed,
            arguments: json!({"title":id}),
            app_context: None,
            plugin_id: None,
            result: Some(
                json!({"_meta":{"codex/toolSurface":{"kind":"computerUse","app":name.map(|s|json!({"kind":"displayName","displayName":s}))}},"content":if image {vec![json!({"type":"image","data":"frame"})]} else {vec![]}}),
            ),
            error: None,
            legacy_resource_uri: None,
            read_only_hint: None,
            duration_ms: None,
            progress: vec![],
        }))
    }

    #[test]
    fn pending_operation_retains_the_known_target_and_its_last_frame() {
        let mut pending = call("click", None, false);
        let ConversationActivity::McpToolCall(ref mut c) = pending else {
            unreachable!()
        };
        c.result = None;
        c.status = AgentMcpToolCallStatus::InProgress;
        c.progress = vec!["Clicking".into()];
        let model =
            Snapshot::from_activities(&[call("observe", Some("Preview"), true), pending]).unwrap();
        assert_eq!(model.target, Some(Target::DisplayName("Preview".into())));
        assert!(model.frame.unwrap().key.starts_with("observe-0-"));
        assert_eq!(model.title, "click");
        assert_eq!(model.progress.as_deref(), Some("Clicking"));
        assert_eq!(model.status, AgentMcpToolCallStatus::InProgress);
    }

    #[test]
    fn changing_or_clearing_the_surface_drops_the_old_application_frame() {
        for next in [Some("Other"), None] {
            let model = Snapshot::from_activities(&[
                call("old", Some("Preview"), true),
                call("new", next, false),
            ])
            .unwrap();
            assert!(model.frame.is_none());
            assert_eq!(model.target, next.map(|s| Target::DisplayName(s.into())));
        }
        assert!(Snapshot::from_activities(&[]).is_none());
    }

    #[test]
    fn updated_image_in_the_same_item_is_not_treated_as_the_old_frame() {
        let initial = call("observe", Some("Preview"), true);
        let mut updated = initial.clone();
        let ConversationActivity::McpToolCall(ref mut c) = updated else {
            unreachable!()
        };
        c.result.as_mut().unwrap()["content"][0]["data"] = json!("new frame");
        assert_ne!(
            Snapshot::from_activities(&[initial])
                .unwrap()
                .frame
                .unwrap()
                .key,
            Snapshot::from_activities(&[updated])
                .unwrap()
                .frame
                .unwrap()
                .key
        );
    }
}
