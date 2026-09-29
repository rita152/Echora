//! Batch 3 codecs. Payloads follow `scripts/batch3_app_server_probe.py`
//! (artifacts/batch3-baseline-*).

use serde_json::json;

mod loaded {
    use super::*;
    use crate::agent::codex::loaded_threads::{list_params, parse_page};

    #[test]
    fn params_omit_an_absent_cursor() {
        assert_eq!(list_params(None), json!({}));
        assert_eq!(list_params(Some("abc")), json!({"cursor": "abc"}));
    }

    #[test]
    fn pages_decode_ids_and_the_cursor_as_the_probe_recorded() {
        let (ids, next) = parse_page(&json!({"id": 9, "result": {
            "data": ["01a0ebf9-514b-7a60-9569-d8ff1cbcf39d"],
            "nextCursor": "01a0ebf9-514b-7a60-9569-d8ff1cbcf39d"
        }}))
        .unwrap();
        assert_eq!(ids, ["01a0ebf9-514b-7a60-9569-d8ff1cbcf39d"]);
        assert_eq!(
            next.as_deref(),
            Some("01a0ebf9-514b-7a60-9569-d8ff1cbcf39d")
        );
        let (ids, next) = parse_page(&json!({"id": 1, "result": {"data": []}})).unwrap();
        assert!(ids.is_empty());
        assert_eq!(next, None);
        let (_, next) =
            parse_page(&json!({"id": 1, "result": {"data": [], "nextCursor": null}})).unwrap();
        assert_eq!(next, None);
    }

    #[test]
    fn malformed_pages_are_rejected() {
        for result in [
            json!({}),
            json!({"data": null}),
            json!({"data": "t"}),
            json!({"data": [1]}),
            json!({"data": [""]}),
            json!({"data": ["t"], "nextCursor": 3}),
            json!({"data": [], "nextCursor": "again"}),
        ] {
            assert!(
                parse_page(&json!({"id": 1, "result": result.clone()})).is_err(),
                "{result}"
            );
        }
        assert!(parse_page(&json!({"id": 1})).is_err());
    }
}

mod capabilities {
    use super::*;
    use crate::agent::AgentProviderCapabilities;
    use crate::agent::codex::provider::{capabilities_params, parse_capabilities};

    #[test]
    fn params_are_an_empty_object_never_null() {
        // A null or missing `params` is -32600 on the baseline CLI.
        assert_eq!(capabilities_params(), json!({}));
    }

    #[test]
    fn the_recorded_answers_decode() {
        let fake = json!({"id": 1, "result": {"namespaceTools": true, "imageGeneration": true, "webSearch": true}});
        assert_eq!(
            parse_capabilities(3, &fake).unwrap(),
            AgentProviderCapabilities {
                generation: 3,
                image_generation: true,
                web_search: true,
                namespace_tools: true
            }
        );
        let bedrock = json!({"id": 2, "result": {"namespaceTools": true, "imageGeneration": false, "webSearch": true}});
        assert!(!parse_capabilities(1, &bedrock).unwrap().image_generation);
    }

    #[test]
    fn missing_or_mistyped_flags_are_rejected() {
        let base = json!({"namespaceTools": true, "imageGeneration": true, "webSearch": true});
        for field in ["namespaceTools", "imageGeneration", "webSearch"] {
            let mut missing = base.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(parse_capabilities(1, &json!({"id": 1, "result": missing})).is_err());
            let mut null = base.clone();
            null[field] = Value::Null;
            assert!(parse_capabilities(1, &json!({"id": 1, "result": null})).is_err());
        }
        assert!(parse_capabilities(1, &json!({"id": 1})).is_err());
        assert!(parse_capabilities(1, &json!({"id": 1, "result": []})).is_err());
    }

    use serde_json::Value;
}

mod memory_status {
    use super::*;
    use crate::agent::AgentMemoryStatus;
    use crate::agent::codex::features::{memory_status_params, parse_memory_status};

    #[test]
    fn params_name_the_threshold() {
        assert_eq!(
            memory_status_params(20),
            json!({"minConsolidatedThreads": 20})
        );
    }

    #[test]
    fn the_recorded_answer_decodes() {
        let response = json!({"id": 1, "result": {"v2ConsolidatedThreads": 0, "v2Ready": false}});
        assert_eq!(
            parse_memory_status(2, 20, &response).unwrap(),
            AgentMemoryStatus {
                generation: 2,
                v2_ready: false,
                consolidated_threads: 0,
                required_threads: 20
            }
        );
        let ready = json!({"id": 1, "result": {"v2ConsolidatedThreads": 25, "v2Ready": true}});
        let status = parse_memory_status(1, 20, &ready).unwrap();
        assert!(status.v2_ready);
        assert_eq!(status.consolidated_threads, 25);
    }

    #[test]
    fn malformed_answers_are_rejected() {
        for result in [
            json!({"v2Ready": false}),
            json!({"v2ConsolidatedThreads": 0}),
            json!({"v2ConsolidatedThreads": -1, "v2Ready": false}),
            json!({"v2ConsolidatedThreads": 4294967296u64, "v2Ready": false}),
            json!({"v2ConsolidatedThreads": "3", "v2Ready": false}),
            json!({"v2ConsolidatedThreads": 3, "v2Ready": null}),
        ] {
            assert!(
                parse_memory_status(1, 20, &json!({"id": 1, "result": result.clone()})).is_err(),
                "{result}"
            );
        }
    }
}

mod review {
    use super::*;
    use crate::agent::AgentReviewTarget;
    use crate::agent::codex::review::{parse_review_turn_id, review_params, target_value};

    #[test]
    fn every_target_encodes_as_the_schema_names_it() {
        assert_eq!(
            target_value(&AgentReviewTarget::UncommittedChanges),
            json!({"type": "uncommittedChanges"})
        );
        assert_eq!(
            target_value(&AgentReviewTarget::BaseBranch {
                branch: "main".into()
            }),
            json!({"type": "baseBranch", "branch": "main"})
        );
    }

    #[test]
    fn delivery_is_always_inline() {
        // `detached` is rejected for paginated threads and deprecated.
        assert_eq!(
            review_params("t", &AgentReviewTarget::UncommittedChanges)["delivery"],
            "inline"
        );
    }

    #[test]
    fn the_recorded_response_names_the_review_turn() {
        let response = json!({"id": 3, "result": {"turn": {"id": "01a0ebfa-5986-7110-a183-2e4c30852ec7",
            "items": [{"type": "userMessage", "id": "01a0ebfa-5986-7110-a183-2e4c30852ec7", "clientId": null,
                "content": [{"type": "text", "text": "current changes", "text_elements": []}]}],
            "itemsView": "notLoaded", "status": "inProgress", "error": null, "startedAt": null,
            "completedAt": null, "durationMs": null},
            "reviewThreadId": "01a0ebfa-5970-7d62-92ab-e6ddc4522616"}});
        assert_eq!(
            parse_review_turn_id(&response, "01a0ebfa-5970-7d62-92ab-e6ddc4522616").unwrap(),
            "01a0ebfa-5986-7110-a183-2e4c30852ec7"
        );
    }

    #[test]
    fn another_thread_or_a_missing_turn_is_rejected() {
        let ok = json!({"turn": {"id": "R"}, "reviewThreadId": "t"});
        assert!(parse_review_turn_id(&json!({"result": ok}), "other").is_err());
        for result in [
            json!({"turn": {"id": "R"}}),
            json!({"reviewThreadId": "t"}),
            json!({"turn": {}, "reviewThreadId": "t"}),
            json!({"turn": {"id": ""}, "reviewThreadId": "t"}),
            json!(null),
        ] {
            assert!(
                parse_review_turn_id(&json!({"result": result.clone()}), "t").is_err(),
                "{result}"
            );
        }
    }
}

mod shell {
    use super::*;
    use crate::agent::codex::shell::{parse_shell_ack, shell_params};

    #[test]
    fn params_keep_shell_syntax_and_omit_an_absent_timeout() {
        assert_eq!(
            shell_params("t", "printf 'b\\na\\n' | sort", None),
            json!({"threadId": "t", "command": "printf 'b\\na\\n' | sort"})
        );
        assert_eq!(
            shell_params("t", "sleep 5", Some(0)),
            json!({"threadId": "t", "command": "sleep 5", "timeoutMs": 0})
        );
    }

    #[test]
    fn the_ack_is_an_empty_object() {
        assert!(parse_shell_ack(&json!({"id": 1, "result": {}})).is_ok());
        for bad in [
            json!({"id": 1}),
            json!({"id": 1, "result": null}),
            json!({"id": 1, "result": []}),
        ] {
            assert!(parse_shell_ack(&bad).is_err());
        }
    }
}

mod command_source {
    use super::*;
    use crate::agent::codex::items::parse_command_execution;
    use crate::agent::{CommandExecutionSource, CommandExecutionStatus};

    fn item(extra: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        let mut item = json!({"type": "commandExecution", "id": "c", "command": "/opt/homebrew/bin/zsh -lc 'echo x'",
            "cwd": "/p", "commandActions": [{"type": "unknown", "command": "echo x"}],
            "aggregatedOutput": "x\n", "exitCode": 0, "status": "completed"});
        for (key, value) in extra.as_object().unwrap() {
            item[key] = value.clone();
        }
        item.as_object().unwrap().clone()
    }

    #[test]
    fn source_defaults_to_agent_and_decodes_every_variant() {
        assert_eq!(
            parse_command_execution(&item(json!({}))).unwrap().source,
            CommandExecutionSource::Agent
        );
        for (wire, source) in [
            ("agent", CommandExecutionSource::Agent),
            ("userShell", CommandExecutionSource::UserShell),
            (
                "unifiedExecStartup",
                CommandExecutionSource::UnifiedExecStartup,
            ),
            (
                "unifiedExecInteraction",
                CommandExecutionSource::UnifiedExecInteraction,
            ),
        ] {
            assert_eq!(
                parse_command_execution(&item(json!({"source": wire})))
                    .unwrap()
                    .source,
                source
            );
        }
        assert!(parse_command_execution(&item(json!({"source": "robot"}))).is_err());
        assert!(parse_command_execution(&item(json!({"source": null}))).is_err());
    }

    #[test]
    fn the_recorded_timeout_and_failure_are_told_apart() {
        let timeout = parse_command_execution(&item(
            json!({"source": "userShell", "status": "failed",
            "exitCode": -1, "aggregatedOutput":
            "execution error: Sandbox(Timeout { output: ExecToolCallOutput { exit_code: 124 } })"}),
        ))
        .unwrap();
        assert_eq!(timeout.status, CommandExecutionStatus::Failed);
        assert!(timeout.timed_out);
        assert_eq!(
            timeout.output, "",
            "the executor's error text is not the command's output"
        );
        let failure =
            parse_command_execution(&item(json!({"source": "userShell", "status": "failed",
            "exitCode": 3, "aggregatedOutput": "to-stderr\n"})))
            .unwrap();
        assert!(!failure.timed_out);
        assert_eq!(failure.output, "to-stderr\n");
    }
}
