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

/// Surface identity is independent of whether a call returned an image.
/// Browser tool surfaces never select all windows of the user's browser.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Surface {
    Native(Option<Target>),
    Ended,
    Browser {
        backend: String,
        browser_id: Option<String>,
    },
}

fn surface(call: &AgentMcpToolCall) -> Option<Surface> {
    let value = call
        .result
        .as_ref()?
        .get("_meta")?
        .get("codex/toolSurface")?;
    match value.get("kind")?.as_str()? {
        "computerUse" => Some(Surface::Native(target(call))),
        "browserUse"
            if value
                .get("sessionEnded")
                .and_then(serde_json::Value::as_bool)
                == Some(true) =>
        {
            Some(Surface::Ended)
        }
        "browserUse" => Some(Surface::Browser {
            backend: value
                .get("backend")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .into(),
            browser_id: value
                .get("browserId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        }),
        _ => None,
    }
}

#[derive(Clone)]
pub(super) struct ToolFrame {
    pub key: String,
    pub data: String,
    pub mime: Option<String>,
}

pub(super) struct Snapshot {
    pub browser: bool,
    pub target: Option<Target>,
    pub targets: Vec<Target>,
    pub label: String,
    pub title: String,
    pub status: AgentMcpToolCallStatus,
    pub progress: Option<String>,
    pub error: Option<String>,
    pub frame: Option<ToolFrame>,
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
        let active_surface = calls.iter().rev().find_map(|call| surface(call));
        // Tool startup/inventory has not identified a surface yet. The
        // reference shows no PiP until there is something to preview.
        active_surface.as_ref()?;
        let active_target = match &active_surface {
            Some(Surface::Native(target)) => target.clone(),
            _ => None,
        };
        let browser = matches!(
            active_surface,
            Some(Surface::Browser { .. } | Surface::Ended)
        );
        let label = if browser {
            crate::i18n::text("浏览器").into()
        } else {
            active_target
                .as_ref()
                .map_or_else(|| "Computer Use".into(), |t| t.label().to_owned())
        };
        let mut frame = None;
        for call in calls.iter().rev() {
            if matches!(active_surface, Some(Surface::Ended)) {
                break;
            }
            let Some(call_surface) = surface(call) else {
                continue;
            };
            // Stop at a changed or explicitly cleared surface, even if an
            // older operation happened to select the same app/browser.
            if Some(&call_surface) != active_surface.as_ref() {
                break;
            }
            let Some(result) = &call.result else { continue };
            if browser {
                let screenshot = result.pointer("/_meta/codex~1toolSurface/screenshot");
                if let Some(url) = screenshot
                    .and_then(|s| s.get("url"))
                    .and_then(serde_json::Value::as_str)
                {
                    frame = browser_frame(&call.id, url);
                    // A new screenshot replaces the previous page, including
                    // an invalid/unsupported image. Never show the stale page.
                    break;
                }
            } else if let Some(parts) = result.get("content").and_then(serde_json::Value::as_array)
            {
                frame = parts.iter().enumerate().rev().find_map(|(index, part)| {
                    if part.get("type")?.as_str()? != "image" {
                        return None;
                    }
                    Some(tool_frame(
                        format!("{}-{index}", call.id),
                        part.get("data")?.as_str()?,
                        part.get("mimeType").and_then(serde_json::Value::as_str),
                    ))
                });
                if frame.is_some() {
                    break;
                }
            }
        }
        let mut targets = Vec::new();
        for call in &calls {
            if let Some(surface) = call
                .result
                .as_ref()
                .and_then(|r| r.get("_meta"))
                .and_then(|m| m.get("codex/toolSurface"))
                && surface.get("kind").and_then(serde_json::Value::as_str) == Some("computerUse")
            {
                match target(call) {
                    Some(target) if !targets.contains(&target) => targets.push(target),
                    None => targets.clear(),
                    _ => {}
                }
            }
        }
        if browser {
            targets.clear();
        }
        Some(Self {
            browser,
            target: active_target,
            targets,
            label,
            title: mcp_tool_call_label(latest),
            status: latest.status,
            progress: latest.progress.last().cloned(),
            error: latest.error.clone(),
            frame,
        })
    }
}

fn tool_frame(id: String, data: &str, mime: Option<&str>) -> ToolFrame {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hash);
    ToolFrame {
        key: format!("{id}-{}", hash.finish()),
        data: data.into(),
        mime: mime.map(str::to_owned),
    }
}

fn browser_frame(id: &str, url: &str) -> Option<ToolFrame> {
    let (header, data) = url.strip_prefix("data:")?.split_once(',')?;
    let mime = header.strip_suffix(";base64")?;
    if !matches!(mime, "image/png" | "image/jpeg" | "image/webp") || data.is_empty() {
        return None;
    }
    Some(tool_frame(format!("{id}-browser"), data, Some(mime)))
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
        assert!(Snapshot::from_activities(&[pending.clone()]).is_none());
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
    #[test]
    fn applications_accumulate_within_a_turn_and_explicit_null_clears_them() {
        let model = Snapshot::from_activities(&[
            call("a", Some("First"), false),
            call("b", Some("Second"), false),
            call("c", Some("First"), false),
        ])
        .unwrap();
        assert_eq!(
            model.targets,
            vec![
                Target::DisplayName("First".into()),
                Target::DisplayName("Second".into())
            ]
        );
        let model = Snapshot::from_activities(&[
            call("a", Some("First"), false),
            call("clear", None, false),
            call("b", Some("Second"), false),
        ])
        .unwrap();
        assert_eq!(model.targets, vec![Target::DisplayName("Second".into())]);
    }
    fn browser_call(id: &str, browser: &str, screenshot: Option<&str>) -> ConversationActivity {
        let mut activity = call(id, None, false);
        let ConversationActivity::McpToolCall(ref mut c) = activity else {
            unreachable!()
        };
        c.result = Some(json!({"content": [], "_meta": {"codex/toolSurface": {
            "kind": "browserUse", "backend": "chrome", "browserId": browser,
            "screenshot": screenshot.map(|url| json!({"url":url,"tabId":"123","pageUrl":"https://www.bilibili.com/"}))
        }}}));
        activity
    }

    #[test]
    fn chrome_tab_metadata_supplies_the_preview_and_survives_a_following_call_without_an_image() {
        let mut screenshot = browser_call("open", "1", Some("data:image/jpeg;base64,Zmlyc3Q="));
        let ConversationActivity::McpToolCall(ref mut call) = screenshot else {
            unreachable!()
        };
        // Chrome capture metadata identifies the extension, but a following
        // markDeliverable result only repeats backend and browserId.
        call.result.as_mut().unwrap()["_meta"]["codex/toolSurface"]["extensionInstanceId"] =
            json!("chrome-extension");
        let next = browser_call("deliver", "1", None);
        let snapshot = Snapshot::from_activities(&[screenshot, next]).unwrap();
        assert!(snapshot.browser);
        assert!(snapshot.targets.is_empty());
        assert!(snapshot.target.is_none());
        let frame = snapshot.frame.unwrap();
        assert_eq!(frame.data, "Zmlyc3Q=");
        assert_eq!(frame.mime.as_deref(), Some("image/jpeg"));
        assert!(frame.key.starts_with("open-browser-"));
    }

    #[test]
    fn browser_preview_updates_and_never_inherits_another_surface_or_remote_url() {
        let old = browser_call("old", "1", Some("data:image/png;base64,b2xk"));
        let new = browser_call("new", "1", Some("data:image/png;base64,bmV3"));
        let snapshot = Snapshot::from_activities(&[old.clone(), new]).unwrap();
        assert_eq!(snapshot.frame.unwrap().data, "bmV3");
        for next in [
            browser_call("other-browser", "2", None),
            browser_call(
                "remote-image",
                "1",
                Some("https://example.invalid/image.png"),
            ),
            call("native-app", Some("Preview"), false),
            call("cleared", None, false),
        ] {
            assert!(
                Snapshot::from_activities(&[old.clone(), next])
                    .unwrap()
                    .frame
                    .is_none()
            );
        }
        // Switching away and then back cannot resurrect an old screenshot.
        let snapshot = Snapshot::from_activities(&[
            old,
            call("native", Some("Preview"), false),
            browser_call("back", "1", None),
        ])
        .unwrap();
        assert!(snapshot.frame.is_none());
    }

    #[test]
    fn ended_browser_session_drops_the_old_frame() {
        let old = browser_call("old", "1", Some("data:image/png;base64,b2xk"));
        let mut ended = browser_call("ended", "1", None);
        let ConversationActivity::McpToolCall(ref mut call) = ended else {
            unreachable!()
        };
        call.result.as_mut().unwrap()["_meta"]["codex/toolSurface"]["sessionEnded"] = json!(true);
        let model = Snapshot::from_activities(&[old.clone(), ended.clone()]).unwrap();
        assert!(model.browser);
        assert!(model.frame.is_none());
        assert!(model.targets.is_empty());
        let reopened = browser_call("reopened", "1", None);
        assert!(
            Snapshot::from_activities(&[old, ended, reopened])
                .unwrap()
                .frame
                .is_none()
        );
    }
}
