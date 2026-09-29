//! Batch 2: running-turn reviewer, hooks, experimental features, memories and
//! find in chat. Payload shapes follow the baseline CLI recordings in
//! artifacts/batch2-baseline-*.
use super::*;
use crate::agent::{
    AgentActiveTurnReviewerUpdate, AgentThreadPermissionResult, AgentThreadPermissionUpdate,
};

fn assist(thread: &str, operation: u64) -> AgentThreadPermissionUpdate {
    AgentThreadPermissionUpdate {
        thread_id: thread.into(),
        cwd: "/tmp/project".into(),
        mode: AgentPermissionMode::Assist,
        expected_generation: Some(1),
        operation_id: operation,
    }
}

fn settings_notification(thread: &str, reviewer: &str, profile: &str) -> Value {
    json!({"method":"thread/settings/updated","params":{"threadId":thread,"threadSettings":{
        "model":"gpt-test","effort":"medium","serviceTier":null,"cwd":"/tmp/project",
        "approvalPolicy":"on-request","approvalsReviewer":reviewer,"sandboxPolicy":{"type":"workspaceWrite"},
        "activePermissionProfile":{"id":profile,"extends":null}
    }}})
}

/// Starts turn `r` on thread `t`. The run is returned with the endpoint:
/// dropping its handle would interrupt the turn.
fn running_turn(
    manager: &CodexAppServerManager,
    spawner: &FakeSpawner,
) -> (FakeEndpoint, crate::agent::AgentRun) {
    let run = manager.run_prompt(request("initial", Some("t")));
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "t", "r");
    (endpoint, run)
}

/// Answers the thread settings request and its confirmation notification.
fn confirm_thread_update(endpoint: &mut FakeEndpoint, reviewer: &str) -> Value {
    let update = loop {
        let message = endpoint.recv();
        match message["method"].as_str().unwrap() {
            "permissionProfile/list" => endpoint.respond(
                &message,
                json!({"data":[{"id":":workspace","allowed":true}]}),
            ),
            "thread/settings/update" => break message,
            other => panic!("unexpected {other}"),
        }
    };
    endpoint.send(settings_notification("t", reviewer, ":workspace"));
    endpoint.respond(&update, json!({}));
    update
}

fn permission_result(
    receiver: &async_channel::Receiver<Result<AgentThreadPermissionResult, String>>,
) -> AgentThreadPermissionResult {
    wait_value(receiver).unwrap()
}

#[test]
fn a_reviewer_change_reaches_the_running_turn_after_the_thread_update() {
    let (manager, spawner) = manager_with_fake();
    let (mut endpoint, _run) = running_turn(&manager, &spawner);
    let result = manager.update_thread_permissions(assist("t", 1));
    let update = confirm_thread_update(&mut endpoint, "auto_review");
    assert_eq!(update["params"]["approvalsReviewer"], "auto_review");
    let turn = endpoint.recv();
    assert_eq!(turn["method"], "turn/settings/update");
    // Only the reviewer: the other permission fields have no per-turn form.
    assert_eq!(
        turn["params"],
        json!({"threadId":"t","turnId":"r","approvalsReviewer":"auto_review"})
    );
    // The permission result waits for the running-turn answer.
    std::thread::sleep(Duration::from_millis(20));
    assert!(matches!(result.try_recv(), Err(TryRecvError::Empty)));
    endpoint.respond(&turn, json!({"status":"applied"}));
    assert_eq!(
        permission_result(&result).active_turn_reviewer,
        Some(AgentActiveTurnReviewerUpdate::Applied {
            turn_id: "r".into()
        })
    );
    manager.shutdown();
}

#[test]
fn a_queued_permission_change_waits_for_the_previous_turn_reviewer_update() {
    let (manager, spawner) = manager_with_fake();
    let (mut endpoint, _run) = running_turn(&manager, &spawner);
    let first = manager.update_thread_permissions(assist("t", 1));
    let mut second = assist("t", 2);
    second.mode = AgentPermissionMode::Request;
    let second = manager.update_thread_permissions(second);
    confirm_thread_update(&mut endpoint, "auto_review");
    let turn = endpoint.recv();
    assert_eq!(turn["method"], "turn/settings/update");
    // Nothing else is written until the running-turn update is answered.
    std::thread::sleep(Duration::from_millis(30));
    assert!(endpoint.from_client.try_recv().is_err());
    endpoint.respond(&turn, json!({"status":"targetUnavailable"}));
    assert_eq!(
        permission_result(&first).active_turn_reviewer,
        Some(AgentActiveTurnReviewerUpdate::TargetUnavailable {
            turn_id: "r".into()
        })
    );
    let update = confirm_thread_update(&mut endpoint, "user");
    assert_eq!(update["params"]["approvalsReviewer"], "user");
    let turn = endpoint.recv();
    assert_eq!(turn["params"]["approvalsReviewer"], "user");
    endpoint.respond(&turn, json!({"status":"applied"}));
    assert!(matches!(
        permission_result(&second).active_turn_reviewer,
        Some(AgentActiveTurnReviewerUpdate::Applied { .. })
    ));
    manager.shutdown();
}

#[test]
fn a_failed_or_malformed_turn_update_keeps_the_confirmed_thread_settings() {
    let (manager, spawner) = manager_with_fake();
    let (mut endpoint, _run) = running_turn(&manager, &spawner);
    let result = manager.update_thread_permissions(assist("t", 1));
    confirm_thread_update(&mut endpoint, "auto_review");
    let turn = endpoint.recv();
    endpoint.send(json!({"id":turn["id"],"error":{"code":-32600,"message":"no live turn"}}));
    let confirmed = permission_result(&result);
    assert_eq!(
        confirmed
            .settings
            .permissions
            .as_ref()
            .unwrap()
            .approvals_reviewer,
        "auto_review"
    );
    assert!(matches!(
        confirmed.active_turn_reviewer,
        Some(AgentActiveTurnReviewerUpdate::Failed { ref turn_id, .. }) if turn_id == "r"
    ));
    // An unknown status is not taken as applied.
    let mut next = assist("t", 2);
    next.mode = AgentPermissionMode::Request;
    let result = manager.update_thread_permissions(next);
    confirm_thread_update(&mut endpoint, "user");
    let turn = endpoint.recv();
    endpoint.respond(&turn, json!({"status":"queued"}));
    assert!(matches!(
        permission_result(&result).active_turn_reviewer,
        Some(AgentActiveTurnReviewerUpdate::Failed { .. })
    ));
    manager.shutdown();
}

#[test]
fn no_turn_update_without_a_running_turn_or_without_a_reviewer() {
    let (manager, spawner) = manager_with_fake();
    let (mut endpoint, _run) = running_turn(&manager, &spawner);
    // A named profile carries no reviewer, so the running turn is left alone.
    let mut profile = assist("t", 1);
    profile.mode = AgentPermissionMode::Profile(":workspace".into());
    let result = manager.update_thread_permissions(profile);
    loop {
        let message = endpoint.recv();
        match message["method"].as_str().unwrap() {
            "permissionProfile/list" => endpoint.respond(
                &message,
                json!({"data":[{"id":":workspace","allowed":true}]}),
            ),
            "thread/settings/update" => {
                assert!(message["params"].get("approvalsReviewer").is_none());
                endpoint.send(settings_notification("t", "user", ":workspace"));
                endpoint.respond(&message, json!({}));
                break;
            }
            other => panic!("unexpected {other}"),
        }
    }
    assert_eq!(permission_result(&result).active_turn_reviewer, None);
    // After the turn ends there is no target, so nothing is sent for it.
    complete(&endpoint, "t", "r", "completed");
    std::thread::sleep(Duration::from_millis(30));
    let result = manager.update_thread_permissions(assist("t", 2));
    confirm_thread_update(&mut endpoint, "auto_review");
    assert_eq!(permission_result(&result).active_turn_reviewer, None);
    std::thread::sleep(Duration::from_millis(20));
    assert!(endpoint.from_client.try_recv().is_err());
    manager.shutdown();
}

fn open_connection(manager: &CodexAppServerManager, spawner: &FakeSpawner) -> FakeEndpoint {
    // A cheap read opens the connection; its answer is not under test.
    let features = manager.list_experimental_features(None);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let list = endpoint.recv();
    endpoint.respond(&list, json!({"data": [], "nextCursor": null}));
    wait_value(&features).unwrap();
    endpoint
}

#[test]
fn b_hooks_list_names_the_cwds_and_reports_its_generation() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_connection(&manager, &spawner);
    let result = manager.list_hooks(vec!["/work/a".into(), "/work/b".into()]);
    let request = endpoint.recv();
    assert_eq!(request["method"], "hooks/list");
    assert_eq!(request["params"], json!({"cwds": ["/work/a", "/work/b"]}));
    endpoint.respond(
        &request,
        json!({"data": [{"cwd": "/work/a", "hooks": [], "warnings": ["w"], "errors": []}]}),
    );
    let snapshot = wait_value(&result).unwrap();
    assert_eq!(snapshot.generation, 1);
    assert_eq!(snapshot.entries[0].warnings, ["w"]);
    // A malformed answer is an error, never an empty list.
    let result = manager.list_hooks(vec!["/work/a".into()]);
    let request = endpoint.recv();
    endpoint.respond(&request, json!({"data": [{"cwd": "/work/a", "hooks": []}]}));
    assert!(wait_value(&result).is_err());
    manager.shutdown();
}

#[test]
fn c_feature_pages_are_followed_and_loops_are_rejected() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_connection(&manager, &spawner);
    let feature = |name: &str| json!({"name": name, "stage": "beta", "enabled": false, "defaultEnabled": false});
    let result = manager.list_experimental_features(Some("t".into()));
    let first = endpoint.recv();
    assert_eq!(
        first["params"],
        json!({"cursor": null, "limit": 100, "threadId": "t"})
    );
    endpoint.respond(&first, json!({"data": [feature("a")], "nextCursor": "100"}));
    let second = endpoint.recv();
    assert_eq!(second["params"]["cursor"], "100");
    endpoint.respond(&second, json!({"data": [feature("b")], "nextCursor": null}));
    let features = wait_value(&result).unwrap();
    assert_eq!(
        features
            .features
            .iter()
            .map(|feature| feature.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    // A cursor that comes back again, or a flag listed twice, stops the walk.
    let result = manager.list_experimental_features(None);
    let first = endpoint.recv();
    endpoint.respond(&first, json!({"data": [feature("a")], "nextCursor": "x"}));
    let second = endpoint.recv();
    endpoint.respond(&second, json!({"data": [feature("b")], "nextCursor": "x"}));
    assert!(wait_value(&result).unwrap_err().contains("重复的游标"));
    let result = manager.list_experimental_features(None);
    let first = endpoint.recv();
    endpoint.respond(
        &first,
        json!({"data": [feature("a"), feature("a")], "nextCursor": null}),
    );
    assert!(wait_value(&result).is_err());
    manager.shutdown();
}

#[test]
fn d_memory_mode_is_bound_to_its_generation_and_reset_sends_no_params() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_connection(&manager, &spawner);
    let result = manager.set_thread_memory_mode(
        "t".into(),
        1,
        crate::agent::AgentThreadMemoryMode::Disabled,
    );
    let resume = endpoint.recv();
    assert_eq!(resume["method"], "thread/resume");
    endpoint.respond(&resume, json!({"thread": {"id": "t"}}));
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/memoryMode/set");
    assert_eq!(
        request["params"],
        json!({"threadId": "t", "mode": "disabled"})
    );
    endpoint.respond(&request, json!({}));
    assert!(wait_value(&result).is_ok());
    // A failure is reported, never taken as success.
    let result =
        manager.set_thread_memory_mode("t".into(), 1, crate::agent::AgentThreadMemoryMode::Enabled);
    let request = endpoint.recv();
    endpoint.send(
        json!({"id": request["id"], "error": {"code": -32600, "message": "no rollout found"}}),
    );
    assert!(wait_value(&result).is_err());
    // An older generation's click never reaches this connection.
    let stale =
        manager.set_thread_memory_mode("t".into(), 0, crate::agent::AgentThreadMemoryMode::Enabled);
    assert!(wait_value(&stale).unwrap_err().contains("连接已变化"));
    let reset = manager.reset_memories();
    let request = endpoint.recv();
    assert_eq!(request["method"], "memory/reset");
    assert!(request.get("params").is_none());
    endpoint.respond(&request, json!({}));
    assert!(wait_value(&reset).is_ok());
    std::thread::sleep(Duration::from_millis(20));
    assert!(endpoint.from_client.try_recv().is_err());
    manager.shutdown();
}

#[test]
fn d_a_new_chat_sends_its_memory_choice_as_thread_start_config() {
    let (manager, spawner) = manager_with_fake();
    let mut chosen = request("hello", None);
    chosen.context.memory = Some(crate::agent::AgentMemoryPreferences {
        use_memories: false,
        generate_memories: true,
    });
    let _run = manager.run_prompt(chosen);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let start = endpoint.recv();
    assert_eq!(start["method"], "thread/start");
    assert_eq!(
        start["params"]["config"],
        json!({"memories.generate_memories": true, "memories.use_memories": false})
    );
    manager.shutdown();

    // Without a choice the configured defaults apply: no override is sent.
    let (manager, spawner) = manager_with_fake();
    let _run = manager.run_prompt(request("hello", None));
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let start = endpoint.recv();
    assert_eq!(start["method"], "thread/start");
    assert!(start["params"].get("config").is_none());
    manager.shutdown();
}

#[test]
fn e_search_pages_carry_the_cursor_and_unsupported_threads_are_told_apart() {
    use crate::agent::{AgentThreadOccurrenceRequest, AgentThreadSearchError};
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_connection(&manager, &spawner);
    let request = |cursor: Option<&str>| AgentThreadOccurrenceRequest {
        thread_id: "t".into(),
        search_term: "hello".into(),
        cursor: cursor.map(Into::into),
        limit: 250,
    };
    let result = manager.search_thread_occurrences(request(Some("c1")));
    let sent = endpoint.recv();
    assert_eq!(sent["method"], "thread/searchOccurrences");
    assert_eq!(
        sent["params"],
        json!({"threadId":"t","searchTerm":"hello","cursor":"c1","limit":250})
    );
    endpoint.respond(
        &sent,
        json!({"data":[{"turnId":"turn","itemId":"item","snippet":"say 🙂 hello",
                        "snippetMatchRange":{"start":7,"end":12},"turnCursor":"tc"}],
               "nextCursor":null}),
    );
    let page = wait_value(&result).unwrap();
    assert_eq!(page.occurrences[0].matched_text(), "hello");
    assert_eq!(page.generation, 1);
    // `-32601` (an ephemeral side chat) is "cannot search here"; anything
    // else is a failure.
    let result = manager.search_thread_occurrences(request(None));
    let sent = endpoint.recv();
    assert!(sent["params"].get("cursor").is_none());
    endpoint.send(json!({"id":sent["id"],"error":{"code":-32601,"message":"thread/searchOccurrences is not supported yet"}}));
    assert!(matches!(
        wait_value(&result),
        Err(AgentThreadSearchError::Unsupported(_))
    ));
    let result = manager.search_thread_occurrences(request(Some("stale")));
    let sent = endpoint.recv();
    endpoint
        .send(json!({"id":sent["id"],"error":{"code":-32600,"message":"invalid cursor: stale"}}));
    assert!(matches!(
        wait_value(&result),
        Err(AgentThreadSearchError::Failed(_))
    ));
    manager.shutdown();
}
