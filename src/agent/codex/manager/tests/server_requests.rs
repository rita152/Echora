//! Controlled replies for server requests this client does not implement as
//! interactive requests. The connection, its pending RPCs, and the active turns
//! must survive every one of them.

use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::agent::codex::client_tools::{
    ClientTool, ClientToolRegistry, ClientToolUnavailable, DynamicToolCallRequest, TOOL_CALL_METHOD,
};
use crate::agent::codex::server_requests::{ServerRequestDiagnostic, ServerRequestDisposition};
use crate::agent::{AgentDynamicToolCallContentItem, AgentServerRequestId};

fn tool_call(
    id: Value,
    thread_id: &str,
    turn_id: &str,
    tool: &str,
    namespace: Option<&str>,
    arguments: Value,
) -> Value {
    json!({
        "id": id,
        "method": TOOL_CALL_METHOD,
        "params": {
            "threadId": thread_id,
            "turnId": turn_id,
            "callId": "call_1",
            "tool": tool,
            "namespace": namespace,
            "arguments": arguments,
        }
    })
}

fn resolved(thread_id: &str, request_id: Value) -> Value {
    json!({
        "method": "serverRequest/resolved",
        "params": { "threadId": thread_id, "requestId": request_id }
    })
}

fn legacy_patch_approval(id: Value, conversation_id: &str) -> Value {
    json!({
        "id": id,
        "method": "applyPatchApproval",
        "params": {
            "callId": "patch_1",
            "conversationId": conversation_id,
            "fileChanges": {"src/main.rs": {"type": "update", "unified_diff": "@@ -1 +1 @@"}}
        }
    })
}

fn legacy_command_approval(id: Value, conversation_id: &str) -> Value {
    json!({
        "id": id,
        "method": "execCommandApproval",
        "params": {
            "callId": "exec_1",
            "command": ["/bin/zsh", "-lc", "git status"],
            "conversationId": conversation_id,
            "cwd": "/tmp/project",
            "parsedCmd": [{"type": "unknown", "cmd": "git status"}]
        }
    })
}

fn current_time_read(id: Value, thread_id: &str) -> Value {
    json!({"id": id, "method": "currentTime/read", "params": {"threadId": thread_id}})
}

fn auth_tokens_refresh(id: Value) -> Value {
    json!({
        "id": id,
        "method": "account/chatgptAuthTokens/refresh",
        "params": {"previousAccountId": null, "reason": "unauthorized"}
    })
}

fn attestation_generate(id: Value) -> Value {
    json!({"id": id, "method": "attestation/generate", "params": {}})
}

fn diagnostics(manager: &CodexAppServerManager) -> Vec<ServerRequestDiagnostic> {
    manager.inner.server_request_diagnostics()
}

/// A registry whose tool genuinely produces content items, used to cover the
/// success path of the same registry the built-in tools go through.
fn fixture_probe(
    _call: &DynamicToolCallRequest,
) -> std::result::Result<Vec<AgentDynamicToolCallContentItem>, ClientToolUnavailable> {
    Ok(vec![AgentDynamicToolCallContentItem::Text {
        text: "fixture runtime: /opt/echora/primary-runtime".to_owned(),
    }])
}

/// The registry keys an exact `namespace + tool`, so a tool that is reachable
/// with and without the `codex_app` namespace is registered for both.
fn fixture_probe_tools() -> Vec<ClientTool> {
    [None, Some("codex_app")]
        .into_iter()
        .map(|namespace| ClientTool::new(namespace, "fixture_probe", fixture_probe))
        .collect()
}

fn manager_with_client_tools(tools: Vec<ClientTool>) -> (CodexAppServerManager, Arc<FakeSpawner>) {
    let spawner = FakeSpawner::new();
    let manager = CodexAppServerManager::with_client_tools(
        spawner.clone(),
        ClientToolRegistry::from_tools(tools),
    );
    (manager, spawner)
}

#[test]
fn controlled_replies_use_each_method_shape_and_echo_the_original_id() {
    let (manager, spawner) = manager_with_fake();
    let run = manager.run_prompt(request("controlled replies", Some("thr_ctl")));
    let (events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_ctl", "turn_ctl");

    endpoint.send(json!({
        "id": 7,
        "method": "applyPatchApproval",
        "params": {
            "callId": "patch_1",
            "conversationId": "thr_ctl",
            "fileChanges": {
                "src/main.rs": {"type": "update", "unified_diff": "@@ -1 +1 @@", "move_path": null},
                "src/new.rs": {"type": "add", "content": "fn main() {}"},
                "src/old.rs": {"type": "delete", "content": ""}
            },
            "grantRoot": null,
            "reason": "probe"
        }
    }));
    let response = endpoint.recv();
    assert_eq!(response["id"], json!(7));
    assert!(response.get("error").is_none());
    let rejection = response["result"]["decision"]["denied"]["rejection"]
        .as_str()
        .expect("legacy patch approval must answer a denied decision");
    assert!(rejection.contains("applyPatchApproval"), "{rejection}");
    assert!(rejection.contains("automatically denied"), "{rejection}");

    endpoint.send(json!({
        "id": "exec-1",
        "method": "execCommandApproval",
        "params": {
            "approvalId": null,
            "callId": "exec_1",
            "command": ["/bin/zsh", "-lc", "git status"],
            "conversationId": "thr_ctl",
            "cwd": "/tmp/project",
            "parsedCmd": [{"type": "unknown", "cmd": "git status"}],
            "reason": null
        }
    }));
    let response = endpoint.recv();
    assert_eq!(response["id"], json!("exec-1"));
    let rejection = response["result"]["decision"]["denied"]["rejection"]
        .as_str()
        .expect("legacy command approval must answer a denied decision");
    assert!(rejection.contains("execCommandApproval"), "{rejection}");

    endpoint.send(json!({
        "id": "clock-1",
        "method": "currentTime/read",
        "params": {"threadId": "thr_ctl"}
    }));
    let response = endpoint.recv();
    assert_eq!(response["id"], json!("clock-1"));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let reported = response["result"]["currentTimeAt"]
        .as_i64()
        .expect("currentTime/read must answer whole Unix seconds");
    assert!((now - reported).abs() <= 2, "{reported} vs {now}");

    endpoint.send(json!({
        "id": 8,
        "method": "account/chatgptAuthTokens/refresh",
        "params": {"previousAccountId": null, "reason": "unauthorized"}
    }));
    let response = endpoint.recv();
    assert_eq!(response["id"], json!(8));
    assert_eq!(response["error"]["code"], -32601);
    assert!(response.get("result").is_none());
    assert!(!response.to_string().contains("accessToken"));

    endpoint.send(json!({"id": "att-1", "method": "attestation/generate", "params": {}}));
    let response = endpoint.recv();
    assert_eq!(response["id"], json!("att-1"));
    assert_eq!(response["error"]["code"], -32601);
    assert!(response.get("result").is_none());
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("requestAttestation")),
        "{response}"
    );

    complete(&endpoint, "thr_ctl", "turn_ctl", "completed");
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));

    let recorded = diagnostics(&manager);
    assert_eq!(recorded.len(), 5);
    assert_eq!(
        recorded[0].disposition,
        ServerRequestDisposition::LegacyApprovalDenied
    );
    assert_eq!(recorded[0].thread_id.as_deref(), Some("thr_ctl"));
    assert_eq!(recorded[0].turn_id, None);
    assert_eq!(recorded[1].method, "execCommandApproval");
    assert_eq!(
        recorded[2].disposition,
        ServerRequestDisposition::CurrentTimeRead
    );
    assert_eq!(
        recorded[3].disposition,
        ServerRequestDisposition::UnsupportedMethod
    );
    assert_eq!(recorded[4].method, "attestation/generate");
    assert_eq!(
        recorded[3].request_id,
        AgentServerRequestId::Number(8),
        "the numeric id must stay numeric in the record"
    );

    drop(interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}

#[test]
fn both_request_id_types_are_echoed_for_every_controlled_method() {
    let (manager, spawner) = manager_with_fake();
    let run = manager.run_prompt(request("id echo", Some("thr_echo")));
    let (events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_echo", "turn_echo");

    // Every controlled method answers with the same body for a numeric and a
    // string id, and echoes the id with its original type.
    let cases: [(Value, Value, Value); 5] = [
        (
            json!(10),
            json!("p10"),
            legacy_patch_approval(Value::Null, "thr_echo"),
        ),
        (
            json!(11),
            json!("c11"),
            legacy_command_approval(Value::Null, "thr_echo"),
        ),
        (
            json!(12),
            json!("t12"),
            current_time_read(Value::Null, "thr_echo"),
        ),
        (json!(13), json!("a13"), auth_tokens_refresh(Value::Null)),
        (json!(14), json!("g14"), attestation_generate(Value::Null)),
    ];
    for (number_id, string_id, template) in cases {
        let mut bodies = Vec::new();
        for id in [number_id.clone(), string_id.clone()] {
            let mut message = template.clone();
            message["id"] = id.clone();
            endpoint.send(message);
            let response = endpoint.recv();
            assert_eq!(response["id"], id, "{template}");
            let mut body = response.clone();
            body.as_object_mut().unwrap().remove("id");
            bodies.push(body);
        }
        let (number_body, string_body) = (&bodies[0], &bodies[1]);
        if template["method"] == json!("currentTime/read") {
            // The clock may advance between the two replies; both must be real.
            for body in [number_body, string_body] {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                let reported = body["result"]["currentTimeAt"].as_i64().unwrap();
                assert!((now - reported).abs() <= 2, "{reported} vs {now}");
            }
        } else {
            assert_eq!(number_body, string_body, "{template}");
        }
    }

    complete(&endpoint, "thr_echo", "turn_echo", "completed");
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));
    assert_eq!(diagnostics(&manager).len(), 10);

    drop(interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}

#[test]
fn legacy_approvals_never_enter_the_v2_approval_cards() {
    let (manager, spawner) = manager_with_fake();
    let run = manager.run_prompt(request("legacy approvals", Some("thr_legacy")));
    let (events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_legacy", "turn_legacy");

    endpoint.send(json!({
        "id": 21,
        "method": "applyPatchApproval",
        "params": {
            "callId": "patch_1",
            "conversationId": "thr_legacy",
            "fileChanges": {"src/main.rs": {"type": "update", "unified_diff": "@@"}}
        }
    }));
    assert_eq!(endpoint.recv()["id"], json!(21));
    endpoint.send(json!({
        "id": 22,
        "method": "execCommandApproval",
        "params": {
            "callId": "exec_1",
            "command": ["pwd"],
            "conversationId": "thr_legacy",
            "cwd": "/tmp/project",
            "parsedCmd": []
        }
    }));
    assert_eq!(endpoint.recv()["id"], json!(22));

    // The automatic denial is a wire reply only: no approval card, no approval
    // queue entry, and no pending server request for the turn.
    complete(&endpoint, "thr_legacy", "turn_legacy", "completed");
    let events = collect_terminal(&events);
    assert!(
        events.iter().all(|event| !matches!(
            event,
            AgentEvent::CommandApprovalRequested { .. }
                | AgentEvent::FileApprovalRequested { .. }
                | AgentEvent::PermissionsApprovalRequested { .. }
                | AgentEvent::UserInputRequested { .. }
        )),
        "legacy approvals must not create an interactive request"
    );
    assert!(matches!(events.last(), Some(AgentEvent::Completed)));

    drop(interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}

#[test]
fn dynamic_tool_calls_fail_honestly_instead_of_ending_the_turn() {
    let (manager, spawner) = manager_with_fake();
    let run = manager.run_prompt(request("tool calls", Some("thr_tool")));
    let (events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_tool", "turn_tool");

    let calls = [
        (json!(11), "load_workspace_dependencies", None),
        (json!("12"), "automation_update", None),
        (json!(13), "automation_update", Some("codex_app")),
        (json!("14"), "codex_app/unknown_tool", None),
    ];
    for (id, tool, namespace) in calls {
        endpoint.send(tool_call(
            id.clone(),
            "thr_tool",
            "turn_tool",
            tool,
            namespace,
            json!({"probe": true}),
        ));
        let response = endpoint.recv();
        assert_eq!(response["id"], id);
        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["success"], json!(false), "{response}");
        assert_eq!(response["result"]["contentItems"], json!([]), "{response}");
    }

    complete(&endpoint, "thr_tool", "turn_tool", "completed");
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));

    let recorded = diagnostics(&manager);
    assert_eq!(recorded.len(), 4);
    assert_eq!(
        recorded[0].disposition,
        ServerRequestDisposition::ToolCallUnavailable
    );
    assert_eq!(
        recorded[1].disposition,
        ServerRequestDisposition::ToolCallUnavailable
    );
    assert_eq!(
        recorded[2].disposition,
        ServerRequestDisposition::ToolCallUnavailable
    );
    assert_eq!(
        recorded[3].disposition,
        ServerRequestDisposition::ToolCallUnknown
    );
    assert_eq!(recorded[0].thread_id.as_deref(), Some("thr_tool"));
    assert_eq!(recorded[0].turn_id.as_deref(), Some("turn_tool"));

    drop(interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}

#[test]
fn registered_client_tool_success_is_written_as_a_real_result() {
    let (manager, spawner) = manager_with_client_tools(fixture_probe_tools());
    let run = manager.run_prompt(request("registered tool", Some("thr_reg")));
    let (events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_reg", "turn_reg");

    endpoint.send(tool_call(
        json!(31),
        "thr_reg",
        "turn_reg",
        "fixture_probe",
        Some("codex_app"),
        json!(null),
    ));
    let response = endpoint.recv();
    assert_eq!(response["id"], json!(31));
    assert_eq!(response["result"]["success"], json!(true));
    assert_eq!(
        response["result"]["contentItems"],
        json!([{"type": "inputText", "text": "fixture runtime: /opt/echora/primary-runtime"}])
    );

    complete(&endpoint, "thr_reg", "turn_reg", "completed");
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));
    assert_eq!(
        diagnostics(&manager)[0].disposition,
        ServerRequestDisposition::ToolCallSucceeded
    );

    drop(interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}

#[test]
fn resolved_notifications_release_tool_calls_idempotently() {
    let (manager, spawner) = manager_with_client_tools(fixture_probe_tools());
    let run = manager.run_prompt(request("resolved lifecycle", Some("thr_res")));
    let (events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_res", "turn_res");

    endpoint.send(tool_call(
        json!(41),
        "thr_res",
        "turn_res",
        "fixture_probe",
        None,
        json!({}),
    ));
    assert_eq!(endpoint.recv()["id"], json!(41));
    // The matching resolution releases the ownership record; a duplicate stays
    // inert instead of failing the generation.
    endpoint.send(resolved("thr_res", json!(41)));
    endpoint.send(resolved("thr_res", json!(41)));

    complete(&endpoint, "thr_res", "turn_res", "completed");
    // A resolution that arrives after the turn ended is inert as well.
    endpoint.send(resolved("thr_res", json!(41)));
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));

    // The same generation keeps serving requests.
    let catalog = manager.load_model_catalog();
    let model_list = endpoint.recv();
    assert_eq!(model_list["method"], "model/list");
    endpoint.respond(&model_list, model_page());
    assert!(wait_value(&catalog).is_ok());

    drop(interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}

#[test]
fn resolved_notifications_for_connection_scoped_replies_stay_inert() {
    let (manager, spawner) = manager_with_fake();
    let run = manager.run_prompt(request("resolved idle replies", Some("thr_idle")));
    let (events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_idle", "turn_idle");

    // A connection-scoped controlled reply has no turn owner; a resolution that
    // names it must not fail the generation, and neither must a duplicate.
    endpoint.send(json!({
        "id": 91,
        "method": "account/chatgptAuthTokens/refresh",
        "params": {"reason": "unauthorized"}
    }));
    assert_eq!(endpoint.recv()["id"], json!(91));
    endpoint.send(resolved("thr_idle", json!(91)));
    endpoint.send(resolved("thr_idle", json!(91)));

    complete(&endpoint, "thr_idle", "turn_idle", "completed");
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));

    drop(interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}

#[test]
fn invalid_controlled_params_answer_minus_32602_and_keep_the_connection() {
    let (manager, spawner) = manager_with_fake();
    let run = manager.run_prompt(request("invalid controlled params", Some("thr_bad")));
    let (_events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_bad", "turn_bad");

    let invalid = [
        json!({"id": 51, "method": "currentTime/read", "params": {"threadId": 5}}),
        json!({
            "id": "52",
            "method": "item/tool/call",
            "params": {
                "threadId": "thr_bad", "turnId": "turn_bad", "callId": "call_1", "tool": "automation_update"
            }
        }),
        json!({"id": 53, "method": "account/chatgptAuthTokens/refresh", "params": {"reason": "expired"}}),
        json!({"id": "54", "method": "attestation/generate", "params": "probe"}),
        json!({
            "id": 55,
            "method": "applyPatchApproval",
            "params": {
                "callId": "patch_1", "conversationId": "thr_bad", "fileChanges": {"a.rs": {"type": "replace"}}
            }
        }),
    ];
    for message in invalid {
        let id = message["id"].clone();
        endpoint.send(message);
        let response = endpoint.recv();
        assert_eq!(response["id"], id);
        assert_eq!(response["error"]["code"], -32602, "{response}");
        assert!(response.get("result").is_none());
    }
    let recorded = diagnostics(&manager);
    assert_eq!(recorded.len(), 5);
    assert!(
        recorded
            .iter()
            .all(|entry| entry.disposition == ServerRequestDisposition::InvalidParams)
    );

    // A resolution for a request answered `-32602` stays inert: the server may
    // resolve it even though the payload was rejected.
    endpoint.send(resolved("thr_bad", json!(51)));

    // The connection and its pending work continue on the same generation.
    let catalog = manager.load_model_catalog();
    let model_list = endpoint.recv();
    endpoint.respond(&model_list, model_page());
    assert!(wait_value(&catalog).is_ok());

    drop(interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}

#[test]
fn controlled_replies_leave_turns_approvals_and_config_usable() {
    let (manager, spawner) = manager_with_fake();
    let run = manager.run_prompt(request("after controlled replies", Some("thr_after")));
    let (events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_after", "turn_after");

    let controlled = [
        json!({"id": 61, "method": "item/futureTool/requestApproval", "params": {"probe": true}}),
        json!({
            "id": 62,
            "method": "applyPatchApproval",
            "params": {
                "callId": "patch_1", "conversationId": "thr_after",
                "fileChanges": {"src/main.rs": {"type": "update", "unified_diff": "@@"}}
            }
        }),
        json!({"id": 63, "method": "execCommandApproval", "params": {
            "callId": "exec_1", "command": ["pwd"], "conversationId": "thr_after",
            "cwd": "/tmp/project", "parsedCmd": []}}),
        json!({"id": 64, "method": "currentTime/read", "params": {"threadId": "thr_after"}}),
        json!({"id": 65, "method": "account/chatgptAuthTokens/refresh", "params": {"reason": "unauthorized"}}),
        json!({"id": 66, "method": "attestation/generate", "params": {}}),
        tool_call(
            json!(67),
            "thr_after",
            "turn_after",
            "automation_update",
            None,
            json!({}),
        ),
    ];
    for message in controlled {
        let id = message["id"].clone();
        endpoint.send(message);
        assert_eq!(endpoint.recv()["id"], id);
    }

    // A v2 command approval still reaches the composer and is answered once.
    endpoint.send(command_approval(json!(71), "thr_after", "turn_after"));
    let deadline = Instant::now() + WAIT;
    let mut approval = None;
    while approval.is_none() && Instant::now() < deadline {
        if let Ok(AgentEvent::CommandApprovalRequested { responder, .. }) = events.try_recv() {
            approval = Some(responder);
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    let approval = approval.expect("v2 command approval must still reach the composer");
    approval
        .respond(AgentCommandApprovalChoice::Accept)
        .expect("v2 approval response must be written");
    let response = endpoint.recv();
    assert_eq!(response["id"], json!(71));
    assert_eq!(response["result"]["decision"], json!("accept"));
    endpoint.send(resolved("thr_after", json!(71)));

    // Configuration reads and writes still complete on the same generation.
    let read = manager.read_config("/tmp/project".into());
    let config = endpoint.recv();
    assert_eq!(config["method"], "config/read");
    endpoint.respond(
        &config,
        json!({"config":{"model_verbosity":"low"},"origins":{},"layers":[]}),
    );
    let requirements = endpoint.recv();
    assert_eq!(requirements["method"], "configRequirements/read");
    endpoint.respond(&requirements, json!({"requirements":null}));
    assert_eq!(wait_value(&read).unwrap().generation, 1);

    complete(&endpoint, "thr_after", "turn_after", "completed");
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));

    // A later turn on the same generation still starts normally: the thread is
    // loaded, so the prompt goes straight to `turn/start`.
    let follow_up = manager.run_prompt(request("follow up", Some("thr_after")));
    let (follow_up_events, follow_up_interrupt) = follow_up.into_parts();
    let turn_start = endpoint.recv();
    assert_eq!(turn_start["method"], "turn/start");
    endpoint.respond(&turn_start, json!({"turn": {"id": "turn_after_2"}}));
    endpoint.send(json!({
        "method": "turn/started",
        "params": {"threadId": "thr_after", "turn": {"id": "turn_after_2", "items": [], "status": "inProgress"}}
    }));
    complete(&endpoint, "thr_after", "turn_after_2", "completed");
    assert!(matches!(
        collect_terminal(&follow_up_events).last(),
        Some(AgentEvent::Completed)
    ));
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);

    drop(interrupt);
    drop(follow_up_interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}

#[test]
fn diagnostics_keep_a_capped_generation_scoped_record() {
    let limit = super::super::connection::SERVER_REQUEST_DIAGNOSTIC_LIMIT;
    let total = limit + 44;
    let (manager, spawner) = manager_with_fake();
    let run = manager.run_prompt(request("diagnostic cap", Some("thr_cap")));
    let (events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_cap", "turn_cap");

    for id in 0..total {
        endpoint.send(json!({
            "id": id,
            "method": "protocol/futureRequest",
            "params": {"index": id}
        }));
    }
    for _ in 0..total {
        assert_eq!(endpoint.recv()["error"]["code"], -32601);
    }

    let recorded = diagnostics(&manager);
    assert_eq!(recorded.len(), limit);
    assert_eq!(
        recorded[0].request_id,
        AgentServerRequestId::Number((total - limit) as i64),
        "the oldest records are evicted first"
    );
    assert_eq!(
        recorded[limit - 1].request_id,
        AgentServerRequestId::Number((total - 1) as i64)
    );

    complete(&endpoint, "thr_cap", "turn_cap", "completed");
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));

    drop(interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}

#[test]
fn diagnostics_record_controlled_replies_without_their_payload_content() {
    let (manager, spawner) = manager_with_fake();
    let run = manager.run_prompt(request("diagnostics", Some("thr_diag")));
    let (events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_diag", "turn_diag");

    endpoint.send(tool_call(
        json!(81),
        "thr_diag",
        "turn_diag",
        "automation_update",
        None,
        json!({"token": "hunter2-secret", "prompt": "private"}),
    ));
    assert_eq!(endpoint.recv()["id"], json!(81));
    endpoint.send(json!({
        "id": "82",
        "method": "item/futureMethod/request",
        "params": {"threadId": "thr_diag", "turnId": "turn_diag", "secret": "hunter2-secret"}
    }));
    assert_eq!(endpoint.recv()["id"], json!("82"));

    let recorded = diagnostics(&manager);
    assert_eq!(recorded.len(), 2);
    let tool = &recorded[0];
    assert_eq!(tool.method, "item/tool/call");
    assert_eq!(tool.request_id, AgentServerRequestId::Number(81));
    assert_eq!(tool.thread_id.as_deref(), Some("thr_diag"));
    assert_eq!(tool.turn_id.as_deref(), Some("turn_diag"));
    assert_eq!(
        tool.disposition,
        ServerRequestDisposition::ToolCallUnavailable
    );
    assert!(
        tool.detail.contains("automation"),
        "the record must name the capability boundary: {}",
        tool.detail
    );
    assert!(
        tool.params.contains("tool=\"automation_update\""),
        "{}",
        tool.params
    );
    assert!(
        tool.params.contains("arguments=object(2)"),
        "{}",
        tool.params
    );
    assert!(!tool.params.contains("hunter2-secret"), "{}", tool.params);

    let unknown = &recorded[1];
    assert_eq!(unknown.method, "item/futureMethod/request");
    assert_eq!(
        unknown.request_id,
        AgentServerRequestId::String("82".to_owned())
    );
    assert_eq!(unknown.disposition, ServerRequestDisposition::UnknownMethod);
    assert!(
        !unknown.params.contains("hunter2-secret"),
        "{}",
        unknown.params
    );

    complete(&endpoint, "thr_diag", "turn_diag", "completed");
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));

    drop(interrupt);
    manager.shutdown();
    wait_for_process(&endpoint.process);
}
