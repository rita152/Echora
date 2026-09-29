//! Payloads are the baseline CLI's `hooks/list` answers recorded by
//! `scripts/batch2_app_server_probe.py --scenario hooks`
//! (artifacts/batch2-baseline-*/hooks.wire.json), with the probe's temporary
//! root shortened to `/probe`. `managed` cannot be provoked without
//! /etc/codex/requirements.toml, so that hook follows the schema.

use serde_json::{Value, json};

use super::*;
use crate::agent::AgentHookStateChange;

pub(crate) fn recorded_entry() -> Value {
    json!({
        "cwd": "/probe/project",
        "hooks": [
            {"key": "/probe/home/config.toml:pre_tool_use:0:0", "eventName": "preToolUse", "handlerType": "command",
             "command": "echo trusted-pre-tool", "async": false, "matcher": "Bash", "timeoutSec": 30,
             "statusMessage": "Checking the command", "additionalContextLimit": null,
             "sourcePath": "/probe/home/config.toml", "source": "user", "pluginId": null, "displayOrder": 0,
             "enabled": true, "isManaged": false, "currentHash": "sha256:aa", "trustStatus": "trusted"},
            {"key": "/probe/home/config.toml:post_tool_use:0:0", "eventName": "postToolUse", "handlerType": "mcpTool",
             "server": "audit", "tool": "record", "matcher": null, "timeoutSec": 600, "statusMessage": null,
             "additionalContextLimit": null, "sourcePath": "/probe/home/config.toml", "source": "user",
             "pluginId": null, "displayOrder": 1, "enabled": true, "isManaged": false,
             "currentHash": "sha256:bb", "trustStatus": "untrusted"},
            {"key": "/probe/home/config.toml:stop:0:0", "eventName": "stop", "handlerType": "command",
             "command": "echo modified-stop --changed", "async": true, "matcher": null, "timeoutSec": 600,
             "statusMessage": null, "additionalContextLimit": null, "sourcePath": "/probe/home/config.toml",
             "source": "user", "pluginId": null, "displayOrder": 3, "enabled": false, "isManaged": false,
             "currentHash": "sha256:cc", "trustStatus": "modified"},
            {"key": "/probe/project/.codex/config.toml:session_start:0:0", "eventName": "sessionStart",
             "handlerType": "command", "command": "echo project-session-start", "async": false, "matcher": null,
             "timeoutSec": 600, "statusMessage": null, "additionalContextLimit": null,
             "sourcePath": "/probe/project/.codex/config.toml", "source": "project", "pluginId": null,
             "displayOrder": 4, "enabled": true, "isManaged": false, "currentHash": "sha256:dd",
             "trustStatus": "untrusted"},
            {"key": "/etc/codex/requirements.toml:interrupt:0:0", "eventName": "interrupt", "handlerType": "command",
             "command": "echo managed-interrupt", "matcher": null, "timeoutSec": 600, "statusMessage": null,
             "sourcePath": "/etc/codex/requirements.toml", "source": "system", "displayOrder": 5,
             "enabled": true, "isManaged": true, "currentHash": "sha256:ee", "trustStatus": "managed"}
        ],
        "warnings": [
            "failed to parse hooks config /probe/home/hooks.json: missing field `command` at line 1 column 51",
            "invalid matcher \"(unclosed\" in /probe/home/config.toml: regex parse error:\n    (unclosed\n    ^\nerror: unclosed group"
        ],
        "errors": [{"path": "/probe/plugins/audit/hooks.json", "message": "failed to read plugin hooks config"}]
    })
}

fn response(entry: Value) -> Value {
    json!({"id": 3, "result": {"data": [entry]}})
}

#[test]
fn list_params_name_every_cwd() {
    assert_eq!(
        list_params(&["/a".into(), "/b".into()]),
        json!({"cwds": ["/a", "/b"]})
    );
}

#[test]
fn every_trust_status_handler_and_load_issue_decodes() {
    let snapshot =
        parse_list_response(4, &["/probe/project".into()], &response(recorded_entry())).unwrap();
    assert_eq!(snapshot.generation, 4);
    let entry = &snapshot.entries[0];
    let trust = entry
        .hooks
        .iter()
        .map(|hook| hook.trust_status)
        .collect::<Vec<_>>();
    assert_eq!(
        trust,
        [
            AgentHookTrustStatus::Trusted,
            AgentHookTrustStatus::Untrusted,
            AgentHookTrustStatus::Modified,
            AgentHookTrustStatus::Untrusted,
            AgentHookTrustStatus::Managed,
        ]
    );
    assert_eq!(
        entry.hooks[0].handler,
        AgentHookHandler::Command {
            command: "echo trusted-pre-tool".into(),
            is_async: false
        }
    );
    assert_eq!(
        entry.hooks[1].handler,
        AgentHookHandler::McpTool {
            server: "audit".into(),
            tool: "record".into()
        }
    );
    assert_eq!(entry.hooks[0].matcher.as_deref(), Some("Bash"));
    assert_eq!(entry.hooks[0].timeout_sec, 30);
    assert_eq!(entry.hooks[3].source, AgentHookSource::Project);
    // `async` defaults to false, `additionalContextLimit` to null.
    assert!(entry.hooks[4].is_managed);
    assert_eq!(entry.hooks[4].additional_context_limit, None);
    assert_eq!(entry.warnings.len(), 2);
    assert_eq!(entry.errors[0].path, "/probe/plugins/audit/hooks.json");
    assert!(entry.hooks[2].needs_review() && !entry.hooks[0].needs_review());
    assert!(!entry.hooks[4].needs_review());
}

#[test]
fn unknown_enums_missing_fields_and_repeated_keys_are_rejected() {
    for (field, value) in [
        ("trustStatus", json!("maybe")),
        ("eventName", json!("beforeEverything")),
        ("source", json!("elsewhere")),
        ("handlerType", json!("script")),
        ("timeoutSec", json!(-1)),
        ("sourcePath", json!("relative/config.toml")),
        ("enabled", json!(null)),
    ] {
        let mut entry = recorded_entry();
        entry["hooks"][0][field] = value;
        assert!(
            parse_list_response(1, &[], &response(entry)).is_err(),
            "{field} should be rejected"
        );
    }
    let mut entry = recorded_entry();
    entry["hooks"][0].as_object_mut().unwrap().remove("command");
    assert!(parse_list_response(1, &[], &response(entry)).is_err());
    let mut entry = recorded_entry();
    let first = entry["hooks"][0].clone();
    entry["hooks"].as_array_mut().unwrap().push(first);
    assert!(parse_list_response(1, &[], &response(entry)).is_err());
    let mut entry = recorded_entry();
    entry.as_object_mut().unwrap().remove("warnings");
    assert!(parse_list_response(1, &[], &response(entry)).is_err());
}

#[test]
fn state_changes_write_quoted_key_paths() {
    let change = AgentHookStateChange {
        key: "/probe/home/config.toml:stop:0:0".into(),
        enabled: Some(false),
        trusted_hash: Some("sha256:cc".into()),
    };
    let edits = change.edits();
    assert_eq!(
        edits[0].key,
        r#"hooks.state."/probe/home/config.toml:stop:0:0".enabled"#
    );
    assert_eq!(edits[0].value, json!(false));
    assert_eq!(
        edits[1].key,
        r#"hooks.state."/probe/home/config.toml:stop:0:0".trusted_hash"#
    );
    assert_eq!(edits[1].value, json!("sha256:cc"));
    let enable_only = AgentHookStateChange {
        key: "k".into(),
        enabled: Some(true),
        trusted_hash: None,
    };
    assert_eq!(enable_only.edits().len(), 1);
}
