//! Batch 4 codecs. Payloads follow `scripts/batch4_app_server_probe.py`
//! (artifacts/batch4-baseline-*) and the reference's own wire log
//! (artifacts/batch4-reference-*/wire-excerpt.jsonl).

use serde_json::json;

mod attachments {
    use super::*;
    use crate::agent::codex::attachments::{
        ATTACHMENT_LIST_PAGE_SIZE, add_params, is_method_not_found, list_params,
        parse_add_response, parse_attachment, parse_list_page, parse_remove_response,
        parse_updated,
    };
    use crate::agent::{
        AgentAttachmentAddOutcome, AgentAttachmentAddRequest, AgentAttachmentContent,
        AgentAttachmentOperation, AgentAttachmentRemoveRequest, AgentPullRequestAttachment,
        AgentPullRequestRef, AgentWorktreeAttachment,
    };

    fn pull_request_add(key: &str) -> AgentAttachmentAddRequest {
        AgentAttachmentAddRequest {
            generation: 1,
            thread_id: "01a0eff7-7f9f-7f52-a8ae-06d16558c610".into(),
            attachment_type: "pull_request".into(),
            identity_key: key.into(),
            payload: json!({
                "url": "https://github.com/openai/codex/pull/35882",
                "root": "/fixture/codex",
                "headBranch": "dependabot/rust_toolchain/codex-rs/rust-toolchain-1.97.1"
            }),
        }
    }

    #[test]
    fn add_sends_the_reference_request_byte_for_byte() {
        // The reference's own request (wire-excerpt.jsonl), rebuilt from the
        // pull request URL the way both clients key it.
        let key = AgentPullRequestRef::parse("https://github.com/openai/codex/pull/35882")
            .unwrap()
            .identity_key();
        assert_eq!(key, "[\"github.com\",\"openai\",\"codex\",35882]");
        assert_eq!(
            add_params(&pull_request_add(&key)).to_string(),
            json!({
                "threadId": "01a0eff7-7f9f-7f52-a8ae-06d16558c610",
                "attachmentType": "pull_request",
                "identityKey": "[\"github.com\",\"openai\",\"codex\",35882]",
                "payload": {
                    "url": "https://github.com/openai/codex/pull/35882",
                    "root": "/fixture/codex",
                    "headBranch": "dependabot/rust_toolchain/codex-rs/rust-toolchain-1.97.1"
                }
            })
            .to_string()
        );
    }

    #[test]
    fn add_outcomes_decode_and_must_name_the_requested_pair() {
        let key = "[\"github.com\",\"openai\",\"codex\",35882]";
        let response = |outcome: &str, returned_key: &str| {
            json!({"id": 5, "result": {"outcome": outcome, "attachment": {
                "id": "01a0ee12-61e0-7811-997f-2925a838acdc",
                "attachmentType": "pull_request",
                "identityKey": returned_key,
                "payload": {"url": "https://github.com/openai/codex/pull/35882", "root": null, "headBranch": null},
                "createdAt": 1790700577
            }}})
        };
        let added = parse_add_response(&response("created", key), &pull_request_add(key)).unwrap();
        assert_eq!(added.outcome, AgentAttachmentAddOutcome::Created);
        assert_eq!(added.attachment.created_at, 1790700577);
        assert_eq!(
            added.attachment.content,
            AgentAttachmentContent::PullRequest(AgentPullRequestAttachment {
                url: "https://github.com/openai/codex/pull/35882".into(),
                root: None,
                head_branch: None,
            })
        );
        let existing =
            parse_add_response(&response("existing", key), &pull_request_add(key)).unwrap();
        assert_eq!(existing.outcome, AgentAttachmentAddOutcome::Existing);
        assert!(parse_add_response(&response("replaced", key), &pull_request_add(key)).is_err());
        assert!(parse_add_response(&response("created", "other"), &pull_request_add(key)).is_err());
        assert!(
            parse_add_response(&json!({"id": 5, "result": {}}), &pull_request_add(key)).is_err()
        );
    }

    #[test]
    fn payloads_decode_like_the_reference_and_everything_else_stays_raw() {
        let attachment = |kind: &str, payload: serde_json::Value| {
            parse_attachment(&json!({
                "id": "a", "attachmentType": kind, "identityKey": "k", "payload": payload, "createdAt": 1
            }))
            .unwrap()
            .content
        };
        assert_eq!(
            attachment(
                "worktree",
                json!({"root": "/w", "workspaceRoot": "/r", "sourceCwd": "/s"})
            ),
            AgentAttachmentContent::Worktree(AgentWorktreeAttachment {
                root: "/w".into(),
                workspace_root: "/r".into()
            })
        );
        // Known types that do not match the reference's schema are kept raw.
        for (kind, payload) in [
            ("worktree", json!({"root": "", "workspaceRoot": "/r"})),
            (
                "pull_request",
                json!({"url": "https://github.com/openai/codex/issues/1"}),
            ),
            (
                "pull_request",
                json!({"url": "https://github.com/o/r/pull/1", "root": 3}),
            ),
            ("pull_request", json!(null)),
            (
                "archived_worktree",
                json!({"worktree": {"root": "/w"}, "pullRequests": []}),
            ),
            ("custom_probe", json!([1, "two", {"three": [3]}])),
        ] {
            assert_eq!(
                attachment(kind, payload.clone()),
                AgentAttachmentContent::Other {
                    attachment_type: kind.into(),
                    payload
                }
            );
        }
        // Required fields are enforced.
        for missing in [
            "id",
            "attachmentType",
            "identityKey",
            "payload",
            "createdAt",
        ] {
            let mut value = json!({"id": "a", "attachmentType": "x", "identityKey": "k", "payload": {}, "createdAt": 1});
            value.as_object_mut().unwrap().remove(missing);
            assert!(parse_attachment(&value).is_err(), "{missing}");
        }
    }

    #[test]
    fn list_pages_under_the_server_cap_and_reject_bad_pages() {
        // 0.158 drops nextCursor once the limit reaches 100 (probe: 130
        // attachments, limit 100 → 100 rows and no cursor), so pages ask for 99.
        assert_eq!(ATTACHMENT_LIST_PAGE_SIZE, 99);
        assert_eq!(
            list_params("t", None),
            json!({"threadId": "t", "cursor": null, "limit": 99})
        );
        assert_eq!(
            list_params("t", Some("t|1790700577|a")),
            json!({"threadId": "t", "cursor": "t|1790700577|a", "limit": 99})
        );
        let row = |id: &str| json!({"id": id, "attachmentType": "pull_request", "identityKey": id, "payload": {}, "createdAt": 1});
        let (page, next) = parse_list_page(
            &json!({"id": 1, "result": {"data": [row("a"), row("b")], "nextCursor": "t|1|b"}}),
        )
        .unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(next.as_deref(), Some("t|1|b"));
        let (_, next) =
            parse_list_page(&json!({"id": 1, "result": {"data": [], "nextCursor": null}})).unwrap();
        assert_eq!(next, None);
        for result in [
            json!({}),
            json!({"data": null}),
            json!({"data": [row("a"), row("a")]}),
            json!({"data": [], "nextCursor": 3}),
            json!({"data": [], "nextCursor": ""}),
        ] {
            assert!(
                parse_list_page(&json!({"id": 1, "result": result.clone()})).is_err(),
                "{result}"
            );
        }
    }

    #[test]
    fn remove_answers_an_empty_object_and_unknown_methods_mean_unsupported() {
        assert!(parse_remove_response(&json!({"id": 52, "result": {}})).is_ok());
        assert!(parse_remove_response(&json!({"id": 52})).is_err());
        let request = AgentAttachmentRemoveRequest {
            generation: 1,
            thread_id: "t".into(),
            attachment_type: "pull_request".into(),
            identity_key: "k".into(),
        };
        assert_eq!(
            crate::agent::codex::attachments::remove_params(&request),
            json!({"threadId": "t", "attachmentType": "pull_request", "identityKey": "k"})
        );
        let method_not_found = anyhow::anyhow!(
            "{}",
            json!({"code": -32601, "message": "Method not found: thread/attachment/list"})
        );
        assert!(is_method_not_found(&method_not_found));
        let invalid = anyhow::anyhow!(
            "{}",
            json!({"code": -32602, "message": "thread not found: t"})
        );
        assert!(!is_method_not_found(&invalid));
    }

    #[test]
    fn updated_notifications_decode_with_the_extra_timestamp() {
        let note = |operation: &str| {
            json!({"method": "thread/attachment/updated", "params": {
                "threadId": "01a0ee12-5f5b-7460-a626-2339f436b173",
                "attachmentType": "pull_request",
                "identityKey": "github.com/openai/codex#42",
                "attachmentId": "01a0ee12-61e0-7811-997f-2925a838acdc",
                "operation": operation
            }, "emittedAtMs": 1790700577204u64})
        };
        let created = parse_updated(&note("created")).unwrap();
        assert_eq!(created.operation, AgentAttachmentOperation::Created);
        assert_eq!(created.thread_id, "01a0ee12-5f5b-7460-a626-2339f436b173");
        assert_eq!(
            parse_updated(&note("deleted")).unwrap().operation,
            AgentAttachmentOperation::Deleted
        );
        assert!(parse_updated(&note("moved")).is_err());
        let mut with_id = note("created");
        with_id["id"] = json!(3);
        assert!(parse_updated(&with_id).is_err());
        let mut missing = note("created");
        missing["params"]
            .as_object_mut()
            .unwrap()
            .remove("attachmentId");
        assert!(parse_updated(&missing).is_err());
    }

    #[test]
    fn the_notification_is_integrated_and_validated() {
        use crate::agent::codex::methods::{
            UNINTEGRATED_SERVER_NOTIFICATION_METHODS, ensure_server_method_is_defined,
            is_defined_server_method,
        };
        assert!(!UNINTEGRATED_SERVER_NOTIFICATION_METHODS.contains(&"thread/attachment/updated"));
        assert!(is_defined_server_method("thread/attachment/updated"));
        assert!(ensure_server_method_is_defined(&json!({"method": "thread/attachment/updated", "params": {
            "threadId": "t", "attachmentType": "worktree", "identityKey": "/w", "attachmentId": "a", "operation": "created"
        }})).is_ok());
        // A broken payload of an integrated notification is a protocol error.
        assert!(
            ensure_server_method_is_defined(
                &json!({"method": "thread/attachment/updated", "params": {"threadId": "t"}})
            )
            .is_err()
        );
    }
}

mod background_terminals {
    use super::*;
    use crate::agent::codex::dispatch::background_command_event;
    use crate::agent::codex::shell::{clean_params, parse_clean_ack};
    use crate::agent::{AgentEvent, CommandExecutionSource, CommandExecutionStatus};

    #[test]
    fn clean_sends_the_thread_and_accepts_an_empty_acknowledgement() {
        assert_eq!(clean_params("t"), json!({"threadId": "t"}));
        assert!(parse_clean_ack(&json!({"id": 6, "result": {}})).is_ok());
        assert!(parse_clean_ack(&json!({"id": 6, "result": null})).is_err());
    }

    #[test]
    fn late_command_messages_become_background_updates() {
        // After clean the server completes the item under its finished turn.
        let completed = background_command_event(&json!({"method": "item/completed", "params": {
            "threadId": "t", "turnId": "01a0ee14-14b6-7bc0-aad6-cfd2f406342c",
            "item": {
                "type": "commandExecution", "id": "call_resp_1", "pluginId": null, "scriptPath": null,
                "command": "/opt/homebrew/bin/zsh -lc 'echo bg-start; sleep 30; echo bg-end'",
                "cwd": "/repo", "processId": "60110", "source": "unifiedExecStartup",
                "status": "failed",
                "commandActions": [{"type": "unknown", "command": "echo bg-start; sleep 30; echo bg-end"}],
                "aggregatedOutput": "bg-start\r\n", "exitCode": -1, "durationMs": 1020
            }
        }}))
        .unwrap();
        let Some(AgentEvent::CommandCompleted(command)) = completed else {
            panic!("command completion");
        };
        assert_eq!(command.status, CommandExecutionStatus::Failed);
        assert_eq!(command.exit_code, Some(-1));
        assert_eq!(command.terminal_process_id.as_deref(), Some("60110"));
        assert_eq!(command.source, CommandExecutionSource::UnifiedExecStartup);
        assert_eq!(command.command, "echo bg-start; sleep 30; echo bg-end");
        let delta = background_command_event(
            &json!({"method": "item/commandExecution/outputDelta", "params": {
                "threadId": "t", "turnId": "u", "itemId": "call_resp_10", "delta": "bg-mid\r\n"
            }}),
        )
        .unwrap();
        assert_eq!(
            delta,
            Some(AgentEvent::CommandOutputDelta {
                item_id: "call_resp_10".into(),
                delta: "bg-mid\r\n".into()
            })
        );
        // Other late turn messages stay inert.
        assert_eq!(
            background_command_event(&json!({"method": "item/completed", "params": {
                "threadId": "t", "turnId": "u", "item": {"type": "agentMessage", "id": "m", "text": "ok"}
            }}))
            .unwrap(),
            None
        );
        assert_eq!(
            background_command_event(
                &json!({"method": "turn/completed", "params": {"threadId": "t"}})
            )
            .unwrap(),
            None
        );
    }
}

mod git_info {
    use super::*;
    use crate::agent::codex::workspace_protocol::parse_thread_summary;

    fn thread(git: serde_json::Value) -> serde_json::Value {
        json!({
            "id": "t", "preview": "hello", "name": null, "cwd": "/repo", "projectId": null,
            "createdAt": 1, "updatedAt": 2, "recencyAt": null, "status": {"type": "idle"},
            "gitInfo": git
        })
    }

    #[test]
    fn thread_git_info_is_decoded_with_nullable_fields() {
        let summary = parse_thread_summary(&thread(json!({
            "sha": "2506911e", "branch": "feat/x", "originUrl": "https://github.com/openai/codex.git"
        })))
        .unwrap();
        assert_eq!(summary.git.branch.as_deref(), Some("feat/x"));
        assert_eq!(
            summary.git.origin_url.as_deref(),
            Some("https://github.com/openai/codex.git")
        );
        let none = parse_thread_summary(&thread(json!(null))).unwrap();
        assert_eq!(none.git, Default::default());
        let partial =
            parse_thread_summary(&thread(json!({"branch": null, "originUrl": null}))).unwrap();
        assert_eq!(partial.git.branch, None);
        assert!(parse_thread_summary(&thread(json!("main"))).is_err());
        assert!(parse_thread_summary(&thread(json!({"branch": 3}))).is_err());
    }
}
