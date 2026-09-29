//! Batch 3: loaded-thread confirmation, provider capabilities, memory status,
//! shell commands, code review and custom thread sections. Payload shapes
//! follow the baseline CLI recordings in artifacts/batch3-baseline-*.
use super::*;

/// A `thread/resume` answer that carries the effective settings a thread
/// open needs.
fn resume_with_settings(thread: &str) -> Value {
    json!({"thread":{"id":thread},"model":"gpt-test","reasoningEffort":null,"serviceTier":null,
        "cwd":"/tmp/project","approvalPolicy":"on-request","approvalsReviewer":"user",
        "sandbox":{"type":"readOnly"},"activePermissionProfile":{"id":":read-only","extends":null}})
}

/// Opens thread `t` once so this generation holds it loaded.
fn open_thread(manager: &CodexAppServerManager, spawner: &FakeSpawner) -> FakeEndpoint {
    let loaded = manager.load_thread_settings("t".into(), 1);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    assert_eq!(resume["method"], "thread/resume");
    endpoint.respond(&resume, resume_with_settings("t"));
    wait_value(&loaded).unwrap();
    endpoint
}

fn loaded_page(endpoint: &mut FakeEndpoint, data: Value, next: Value) -> Value {
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/loaded/list");
    endpoint.respond(&request, json!({"data": data, "nextCursor": next}));
    request
}

#[test]
fn a_a_thread_the_server_no_longer_holds_is_resumed_again() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let reopened = manager.load_thread_settings("t".into(), 1);
    loaded_page(&mut endpoint, json!(["other"]), Value::Null);
    let resume = endpoint.recv();
    assert_eq!(resume["method"], "thread/resume");
    assert_eq!(resume["params"]["threadId"], "t");
    endpoint.respond(&resume, resume_with_settings("t"));
    assert_eq!(wait_value(&reopened).unwrap().thread_id, "t");
    manager.shutdown();
}

#[test]
fn a_loaded_pages_are_followed_and_a_confirmed_thread_is_not_resumed() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let reopened = manager.load_thread_settings("t".into(), 1);
    let first = loaded_page(&mut endpoint, json!(["a"]), json!("a"));
    assert_eq!(first["params"], json!({}));
    let second = loaded_page(&mut endpoint, json!(["t"]), Value::Null);
    assert_eq!(second["params"], json!({"cursor": "a"}));
    wait_value(&reopened).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    assert!(endpoint.from_client.try_recv().is_err());
    manager.shutdown();
}

#[test]
fn a_a_failed_loaded_read_keeps_the_record_and_is_not_retried() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let reopened = manager.load_thread_settings("t".into(), 1);
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/loaded/list");
    endpoint.send(json!({"id": request["id"], "error": {"code": -32600, "message": "busy"}}));
    wait_value(&reopened).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    assert!(endpoint.from_client.try_recv().is_err());
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}

#[test]
fn a_a_looping_or_malformed_loaded_list_fails_the_generation() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let reopened = manager.load_thread_settings("t".into(), 1);
    loaded_page(&mut endpoint, json!(["x"]), json!("x"));
    loaded_page(&mut endpoint, json!(["y"]), json!("x"));
    assert!(wait_value(&reopened).unwrap_err().contains("重复的游标"));
    wait_for_process(&spawner.process(0));

    // The rebuilt generation holds nothing yet, so it resumes without asking.
    let rebuilt = manager.load_thread_settings("t".into(), 2);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    assert_eq!(resume["method"], "thread/resume");
    endpoint.respond(&resume, resume_with_settings("t"));
    assert_eq!(wait_value(&rebuilt).unwrap().generation, 2);

    let reopened = manager.load_thread_settings("t".into(), 2);
    let request = endpoint.recv();
    endpoint.respond(&request, json!({"data": [7]}));
    assert!(wait_value(&reopened).is_err());
    wait_for_process(&spawner.process(1));
    manager.shutdown();
}

#[test]
fn a_prompts_on_a_loaded_thread_keep_the_fast_path() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let run = manager.run_prompt(request("next", Some("t")));
    let start = endpoint.recv();
    assert_eq!(start["method"], "turn/start");
    endpoint.respond(&start, json!({"turn": {"id": "r"}}));
    complete(&endpoint, "t", "r", "completed");
    let (events, _handle) = run.into_parts();
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));
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
fn b_capabilities_send_an_empty_object_and_name_the_generation() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_connection(&manager, &spawner);
    let read = manager.read_provider_capabilities();
    let request = endpoint.recv();
    assert_eq!(request["method"], "modelProvider/capabilities/read");
    assert_eq!(request["params"], json!({}));
    endpoint.respond(
        &request,
        json!({"namespaceTools": true, "imageGeneration": false, "webSearch": true}),
    );
    let capabilities = wait_value(&read).unwrap();
    assert_eq!(capabilities.generation, 1);
    assert!(!capabilities.image_generation && capabilities.web_search);
    // An error or a malformed answer is reported, never taken as "all off",
    // and neither costs the connection.
    let read = manager.read_provider_capabilities();
    let request = endpoint.recv();
    endpoint.send(json!({"id": request["id"], "error": {"code": -32603, "message": "boom"}}));
    assert!(wait_value(&read).is_err());
    let read = manager.read_provider_capabilities();
    let request = endpoint.recv();
    endpoint.respond(&request, json!({"webSearch": true}));
    assert!(wait_value(&read).is_err());
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}

#[test]
fn b_memory_status_names_its_threshold_and_reports_range_errors() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_connection(&manager, &spawner);
    let read = manager.read_memory_status(20);
    let request = endpoint.recv();
    assert_eq!(request["method"], "memory/status");
    assert_eq!(request["params"], json!({"minConsolidatedThreads": 20}));
    endpoint.respond(
        &request,
        json!({"v2ConsolidatedThreads": 3, "v2Ready": false}),
    );
    let status = wait_value(&read).unwrap();
    assert_eq!(
        (
            status.generation,
            status.consolidated_threads,
            status.required_threads
        ),
        (1, 3, 20)
    );
    let read = manager.read_memory_status(0);
    let request = endpoint.recv();
    endpoint.send(json!({"id": request["id"], "error": {"code": -32602,
        "message": "minConsolidatedThreads must be between 1 and 4096"}}));
    assert!(
        wait_value(&read)
            .unwrap_err()
            .contains("between 1 and 4096")
    );
    // A duplicate answer to a settled read is fatal like any unknown id.
    endpoint.respond(
        &request,
        json!({"v2ConsolidatedThreads": 3, "v2Ready": false}),
    );
    wait_for_process(&spawner.process(0));
    manager.shutdown();
}

#[test]
fn f_rename_keeps_the_appearance_and_checks_the_answering_section() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_connection(&manager, &spawner);
    let renamed = manager.rename_thread_section("s1".into(), "Work stuff".into());
    let request = endpoint.recv();
    assert_eq!(request["method"], "threadSection/update");
    // No `appearance` key: omitted means "keep" in the schema.
    assert_eq!(
        request["params"],
        json!({"sectionId": "s1", "name": "Work stuff"})
    );
    endpoint.respond(
        &request,
        json!({"section": {"id": "s1", "name": "Work stuff", "appearance": {"icon": "folder", "color": "blue"}}}),
    );
    let section = wait_value(&renamed).unwrap();
    assert_eq!(section.name, "Work stuff");
    assert_eq!(section.appearance.unwrap().icon.as_deref(), Some("folder"));
    // The server's validation errors are reported and the connection stays.
    let renamed = manager.rename_thread_section("s1".into(), " ".into());
    let request = endpoint.recv();
    endpoint.send(json!({"id": request["id"], "error": {"code": -32602, "message": "section name must not be empty"}}));
    assert!(wait_value(&renamed).is_err());
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    // An answer for another section is a protocol error.
    let renamed = manager.rename_thread_section("s1".into(), "x".into());
    let request = endpoint.recv();
    endpoint.respond(&request, json!({"section": {"id": "s2", "name": "x"}}));
    assert!(wait_value(&renamed).is_err());
    wait_for_process(&spawner.process(0));
    manager.shutdown();
}

#[test]
fn f_delete_sends_the_section_id_and_reports_a_missing_section() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_connection(&manager, &spawner);
    let deleted = manager.delete_thread_section("s1".into());
    let request = endpoint.recv();
    assert_eq!(request["method"], "threadSection/delete");
    assert_eq!(request["params"], json!({"sectionId": "s1"}));
    endpoint.respond(&request, json!({}));
    wait_value(&deleted).unwrap();
    let deleted = manager.delete_thread_section("s1".into());
    let request = endpoint.recv();
    endpoint.send(json!({"id": request["id"], "error": {"code": -32602,
        "message": "thread section not found: s1"}}));
    assert!(wait_value(&deleted).is_err());
    let deleted = manager.delete_thread_section("s1".into());
    let request = endpoint.recv();
    endpoint.respond(&request, json!(null));
    assert!(wait_value(&deleted).is_err());
    wait_for_process(&spawner.process(0));
    manager.shutdown();
}

fn thread_target(thread: Option<&str>) -> crate::agent::AgentThreadTarget {
    crate::agent::AgentThreadTarget {
        thread_id: thread.map(str::to_owned),
        cwd: "/tmp/project".into(),
        project_id: None,
        model: "gpt-test".into(),
        service_tier: None,
        permission_mode: AgentPermissionMode::Request,
    }
}

fn review(thread: Option<&str>, target: crate::agent::AgentReviewTarget) -> AgentRequestReview {
    crate::agent::AgentReviewRequest {
        thread: thread_target(thread),
        target,
    }
}
type AgentRequestReview = crate::agent::AgentReviewRequest;

fn next_event(
    events: &async_channel::Receiver<AgentConnectionEvent>,
    matches: impl Fn(&AgentConnectionEvent) -> bool,
) -> Option<AgentConnectionEvent> {
    let deadline = Instant::now() + Duration::from_millis(300);
    loop {
        match events.try_recv() {
            Ok(event) if matches(&event) => return Some(event),
            Ok(_) => {}
            Err(TryRecvError::Empty) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(_) => return None,
        }
    }
}

fn is_turn_started(event: &AgentConnectionEvent) -> bool {
    matches!(event, AgentConnectionEvent::TurnStarted { .. })
}

/// The probe's notification order after the `review/start` response: the
/// entered item under the response's id R, the alias `turn/started` S, the
/// live-only prompt item, an agent message that never completes, the result
/// and the completion under R.
fn review_stream(endpoint: &FakeEndpoint, thread: &str, turn: &str, alias: &str) {
    let item = |method: &str, item: Value| {
        json!({"method": method, "params": {"item": item, "threadId": thread, "turnId": turn,
            "startedAtMs": 1, "completedAtMs": 2}})
    };
    let entered = json!({"type": "enteredReviewMode", "id": "e", "review": "current changes"});
    endpoint.send(item("item/started", entered.clone()));
    endpoint.send(item("item/completed", entered));
    endpoint.send(
        json!({"method": "turn/started", "params": {"threadId": thread,
        "turn": {"id": alias, "items": [], "status": "inProgress"}}}),
    );
    let prompt = json!({"type": "userMessage", "id": "u", "clientId": null,
        "content": [{"type": "text", "text": "Review the current code changes", "text_elements": []}]});
    endpoint.send(item("item/started", prompt.clone()));
    endpoint.send(item("item/completed", prompt));
    endpoint.send(item(
        "item/started",
        json!({"type": "agentMessage", "id": "raw", "text": ""}),
    ));
    let exited = json!({"type": "exitedReviewMode", "id": "x", "review": "One small formatting regression."});
    endpoint.send(item("item/started", exited.clone()));
    endpoint.send(item("item/completed", exited));
    let message =
        json!({"type": "agentMessage", "id": "m", "text": "One small formatting regression."});
    endpoint.send(item("item/started", message.clone()));
    endpoint.send(item("item/completed", message));
    complete(endpoint, thread, turn, "completed");
}

#[test]
fn c_an_inline_review_is_one_managed_turn_and_its_alias_is_never_adopted() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let hub = manager.subscribe_connection_events();
    let (events, _handle) = manager
        .run_review(review(
            Some("t"),
            crate::agent::AgentReviewTarget::UncommittedChanges,
        ))
        .into_parts();
    let start = endpoint.recv();
    assert_eq!(start["method"], "review/start");
    assert_eq!(
        start["params"],
        json!({"threadId": "t", "target": {"type": "uncommittedChanges"}, "delivery": "inline"})
    );
    endpoint.respond(
        &start,
        json!({"turn": {"id": "R", "items": [], "status": "inProgress"}, "reviewThreadId": "t"}),
    );
    review_stream(&endpoint, "t", "R", "S");
    let received = collect_terminal(&events);
    assert!(matches!(received.last(), Some(AgentEvent::Completed)));
    assert!(received.iter().any(|event| matches!(event,
        AgentEvent::TurnReady(identity) if identity.turn_id == "R")));
    assert!(received.iter().any(|event| matches!(event,
        AgentEvent::ReviewModeUpdated(review) if review.entered && review.review == "current changes")));
    assert!(next_event(&hub, is_turn_started).is_none());
    // A late message under the alias stays inert.
    complete(&endpoint, "t", "S", "completed");
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    // The thread is free again for the next turn.
    let run = manager.run_prompt(request("next", Some("t")));
    let start = endpoint.recv();
    assert_eq!(start["method"], "turn/start");
    endpoint.respond(&start, json!({"turn": {"id": "next"}}));
    complete(&endpoint, "t", "next", "completed");
    let (events, _handle) = run.into_parts();
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Completed)
    ));
    manager.shutdown();
}

#[test]
fn c_an_alias_that_arrives_before_the_response_is_dropped_with_it() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let hub = manager.subscribe_connection_events();
    let (events, _handle) = manager
        .run_review(review(
            Some("t"),
            crate::agent::AgentReviewTarget::BaseBranch {
                branch: "main".into(),
            },
        ))
        .into_parts();
    let request = endpoint.recv();
    assert_eq!(
        request["params"]["target"],
        json!({"type": "baseBranch", "branch": "main"})
    );
    // Everything but the completion before the response.
    endpoint.send(json!({"method": "turn/started", "params": {"threadId": "t",
        "turn": {"id": "S", "items": [], "status": "inProgress"}}}));
    let entered =
        json!({"type": "enteredReviewMode", "id": "e", "review": "changes against 'main'"});
    endpoint.send(
        json!({"method": "item/started", "params": {"item": entered, "threadId": "t",
        "turnId": "R", "startedAtMs": 1}}),
    );
    std::thread::sleep(Duration::from_millis(30));
    endpoint.respond(
        &request,
        json!({"turn": {"id": "R", "items": [], "status": "inProgress"}, "reviewThreadId": "t"}),
    );
    complete(&endpoint, "t", "R", "completed");
    let received = collect_terminal(&events);
    assert!(matches!(received.last(), Some(AgentEvent::Completed)));
    assert!(next_event(&hub, is_turn_started).is_none());
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}

#[test]
fn c_a_review_in_a_new_chat_starts_the_thread_with_its_permissions() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_connection(&manager, &spawner);
    let (events, _handle) = manager
        .run_review(review(
            None,
            crate::agent::AgentReviewTarget::UncommittedChanges,
        ))
        .into_parts();
    let start = endpoint.recv();
    assert_eq!(start["method"], "thread/start");
    assert_eq!(start["params"]["threadSource"], "code_review");
    assert_eq!(start["params"]["approvalPolicy"], "on-request");
    assert_eq!(start["params"]["approvalsReviewer"], "user");
    assert_eq!(start["params"]["permissions"], ":workspace");
    assert_eq!(start["params"]["cwd"], "/tmp/project");
    endpoint.respond(&start, json!({"thread": {"id": "new"}}));
    let request = endpoint.recv();
    assert_eq!(request["method"], "review/start");
    assert_eq!(request["params"]["threadId"], "new");
    endpoint.respond(
        &request,
        json!({"turn": {"id": "R", "items": [], "status": "inProgress"}, "reviewThreadId": "new"}),
    );
    complete(&endpoint, "new", "R", "completed");
    let received = collect_terminal(&events);
    assert!(matches!(received.first(),
        Some(AgentEvent::ThreadCreated { thread_id }) if thread_id == "new"));
    manager.shutdown();
}

#[test]
fn c_a_rejected_review_ends_its_turn_and_a_foreign_thread_fails_the_generation() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let (events, _handle) = manager
        .run_review(review(
            Some("t"),
            crate::agent::AgentReviewTarget::BaseBranch { branch: "".into() },
        ))
        .into_parts();
    let request = endpoint.recv();
    endpoint.send(json!({"id": request["id"], "error": {"code": -32600,
        "message": "branch must not be empty"}}));
    let received = collect_terminal(&events);
    assert!(matches!(received.last(),
        Some(AgentEvent::Failed(message)) if message.contains("review/start 失败")));
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    // The thread reservation was released: another review may start.
    let (events, _handle) = manager
        .run_review(review(
            Some("t"),
            crate::agent::AgentReviewTarget::UncommittedChanges,
        ))
        .into_parts();
    let request = endpoint.recv();
    endpoint.respond(
        &request,
        json!({"turn": {"id": "R", "items": [], "status": "inProgress"}, "reviewThreadId": "other"}),
    );
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Failed(_))
    ));
    wait_for_process(&spawner.process(0));
    manager.shutdown();
}

#[test]
fn c_stopping_a_review_interrupts_the_response_turn_id() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let (events, handle) = manager
        .run_review(review(
            Some("t"),
            crate::agent::AgentReviewTarget::UncommittedChanges,
        ))
        .into_parts();
    let request = endpoint.recv();
    endpoint.respond(
        &request,
        json!({"turn": {"id": "R", "items": [], "status": "inProgress"}, "reviewThreadId": "t"}),
    );
    endpoint.send(json!({"method": "turn/started", "params": {"threadId": "t",
        "turn": {"id": "S", "items": [], "status": "inProgress"}}}));
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        handle.as_ref().unwrap().interrupt(),
        Ok(AgentInterruptOutcome::Requested)
    );
    let interrupt = endpoint.recv();
    assert_eq!(interrupt["method"], "turn/interrupt");
    assert_eq!(interrupt["params"], json!({"threadId": "t", "turnId": "R"}));
    endpoint.respond(&interrupt, json!({}));
    complete(&endpoint, "t", "R", "interrupted");
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Interrupted)
    ));
    manager.shutdown();
}

fn shell(
    thread: Option<&str>,
    command: &str,
    timeout: Option<u64>,
) -> crate::agent::AgentShellCommandRequest {
    crate::agent::AgentShellCommandRequest {
        thread: thread_target(thread),
        command: command.into(),
        timeout_ms: timeout,
    }
}

#[test]
fn e_a_shell_command_is_acknowledged_and_its_turn_arrives_as_a_server_turn() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let hub = manager.subscribe_connection_events();
    let started = manager.run_shell_command(shell(Some("t"), "ls | sort", None));
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/shellCommand");
    assert_eq!(
        request["params"],
        json!({"threadId": "t", "command": "ls | sort"})
    );
    endpoint.respond(&request, json!({}));
    let started = wait_value(&started).unwrap();
    assert_eq!(
        (
            started.generation,
            started.thread_id.as_str(),
            started.created_thread
        ),
        (1, "t", false)
    );
    endpoint.send(json!({"method": "turn/started", "params": {"threadId": "t",
        "turn": {"id": "sh", "items": [], "status": "inProgress"}}}));
    let Some(AgentConnectionEvent::TurnStarted { turn_id, run, .. }) =
        next_event(&hub, is_turn_started)
    else {
        panic!("the shell turn was not offered");
    };
    assert_eq!(turn_id, "sh");
    let (stream, _interrupt) = run.take().unwrap().into_parts();
    let item = json!({"type": "commandExecution", "id": "c", "command": "/bin/zsh -lc 'ls | sort'",
        "cwd": "/tmp/project", "processId": null, "source": "userShell", "status": "failed",
        "commandActions": [{"type": "unknown", "command": "ls | sort"}],
        "aggregatedOutput": "execution error: Sandbox(Timeout { .. })", "exitCode": -1, "durationMs": 0});
    endpoint.send(
        json!({"method": "item/completed", "params": {"item": item, "threadId": "t",
        "turnId": "sh", "completedAtMs": 1}}),
    );
    complete(&endpoint, "t", "sh", "completed");
    let received = collect_terminal(&stream);
    let command = received
        .iter()
        .find_map(|event| match event {
            AgentEvent::CommandCompleted(command) => Some(command.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        command.source,
        crate::agent::CommandExecutionSource::UserShell
    );
    assert_eq!(command.command, "ls | sort");
    assert!(command.timed_out);
    // A timeout is sent as given.
    let started = manager.run_shell_command(shell(Some("t"), "sleep 5", Some(300)));
    let request = endpoint.recv();
    assert_eq!(request["params"]["timeoutMs"], 300);
    endpoint.respond(&request, json!({}));
    wait_value(&started).unwrap();
    manager.shutdown();
}

#[test]
fn e_a_shell_command_in_a_new_chat_starts_the_thread_first() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_connection(&manager, &spawner);
    let started = manager.run_shell_command(shell(None, "pwd", None));
    let start = endpoint.recv();
    assert_eq!(start["method"], "thread/start");
    assert_eq!(start["params"]["threadSource"], "user");
    assert_eq!(start["params"]["permissions"], ":workspace");
    endpoint.respond(&start, json!({"thread": {"id": "new"}}));
    let request = endpoint.recv();
    assert_eq!(request["params"]["threadId"], "new");
    endpoint.respond(&request, json!({}));
    let started = wait_value(&started).unwrap();
    assert!(started.created_thread);
    assert_eq!(started.thread_id, "new");
    manager.shutdown();
}

#[test]
fn e_rejected_commands_are_reported_and_a_bad_ack_fails_the_generation() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open_thread(&manager, &spawner);
    let started = manager.run_shell_command(shell(Some("t"), " ", None));
    let request = endpoint.recv();
    endpoint.send(json!({"id": request["id"], "error": {"code": -32600,
        "message": "command must not be empty"}}));
    assert!(
        wait_value(&started)
            .unwrap_err()
            .contains("command must not be empty")
    );
    std::thread::sleep(Duration::from_millis(20));
    assert!(endpoint.from_client.try_recv().is_err());
    let started = manager.run_shell_command(shell(Some("t"), "ls", None));
    let request = endpoint.recv();
    endpoint.respond(&request, json!(true));
    assert!(wait_value(&started).is_err());
    wait_for_process(&spawner.process(0));
    manager.shutdown();
}
