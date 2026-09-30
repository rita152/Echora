//! Batch 4: thread attachments (paging, cache, notifications, unsupported
//! servers, generation binding) and background terminals (late command
//! messages of finished turns, `thread/backgroundTerminals/clean`). Payloads
//! follow artifacts/batch4-baseline-*.
use super::*;
use crate::agent::{
    AgentAttachmentAddRequest, AgentAttachmentError, AgentAttachmentRemoveRequest,
    ThreadMetadataUpdate,
};

fn row(id: &str) -> Value {
    json!({"id": id, "attachmentType": "pull_request", "identityKey": format!("key-{id}"),
        "payload": {"url": format!("https://github.com/o/r/pull/{}", id.len()), "root": null, "headBranch": null},
        "createdAt": 1790700577})
}

fn list_page(endpoint: &mut FakeEndpoint, data: Value, next: Value) -> Value {
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/attachment/list");
    endpoint.respond(&request, json!({"data": data, "nextCursor": next}));
    request
}

fn attachment_events(
    events: &async_channel::Receiver<AgentConnectionEvent>,
) -> Vec<AgentConnectionEvent> {
    let deadline = Instant::now() + Duration::from_millis(300);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        match events.try_recv() {
            Ok(event @ AgentConnectionEvent::ThreadAttachmentUpdated(_))
            | Ok(event @ AgentConnectionEvent::BackgroundCommandUpdated { .. }) => seen.push(event),
            Ok(_) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(2)),
        }
    }
    seen
}

#[test]
fn attachment_pages_are_followed_then_answered_from_the_generation_cache() {
    let (manager, spawner) = manager_with_fake();
    let read = manager.list_thread_attachments("t".into(), false);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let first = list_page(&mut endpoint, json!([row("a")]), json!("t|1|a"));
    assert_eq!(
        first["params"],
        json!({"threadId": "t", "cursor": null, "limit": 99})
    );
    let second = list_page(&mut endpoint, json!([row("bb")]), Value::Null);
    assert_eq!(second["params"]["cursor"], "t|1|a");
    let read = wait_value(&read).unwrap();
    assert_eq!(read.generation, 1);
    assert_eq!(read.attachments.len(), 2);

    // Within a minute the same generation answers from memory…
    let cached = wait_value(&manager.list_thread_attachments("t".into(), false)).unwrap();
    assert_eq!(cached.attachments.len(), 2);
    std::thread::sleep(Duration::from_millis(20));
    assert!(endpoint.from_client.try_recv().is_err());
    // …and a forced read asks again.
    let forced = manager.list_thread_attachments("t".into(), true);
    list_page(&mut endpoint, json!([]), Value::Null);
    assert!(wait_value(&forced).unwrap().attachments.is_empty());
    manager.shutdown();
}

#[test]
fn looping_cursors_and_repeated_ids_fail_the_read_but_not_the_connection() {
    let (manager, spawner) = manager_with_fake();
    let read = manager.list_thread_attachments("t".into(), false);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    list_page(&mut endpoint, json!([row("a")]), json!("c1"));
    list_page(&mut endpoint, json!([row("bb")]), json!("c1"));
    let AgentAttachmentError::Failed(message) = wait_value(&read).unwrap_err() else {
        panic!("a failed read");
    };
    assert!(message.contains("重复的 nextCursor"), "{message}");

    let read = manager.list_thread_attachments("t".into(), true);
    list_page(&mut endpoint, json!([row("a")]), json!("c2"));
    list_page(&mut endpoint, json!([row("a")]), Value::Null);
    assert!(wait_value(&read).is_err());
    assert!(endpoint.process.is_alive());
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}

#[test]
fn a_notification_during_a_read_makes_the_late_answer_read_again() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let read = manager.list_thread_attachments("t".into(), false);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let stale = endpoint.recv();
    assert_eq!(stale["method"], "thread/attachment/list");
    // The server created an attachment before answering the older read.
    endpoint.send(json!({"method": "thread/attachment/updated", "params": {
        "threadId": "t", "attachmentType": "pull_request", "identityKey": "key-bb",
        "attachmentId": "bb", "operation": "created"
    }, "emittedAtMs": 1790700577204u64}));
    std::thread::sleep(Duration::from_millis(30));
    endpoint.respond(&stale, json!({"data": [row("a")], "nextCursor": null}));
    list_page(&mut endpoint, json!([row("a"), row("bb")]), Value::Null);
    assert_eq!(wait_value(&read).unwrap().attachments.len(), 2);
    let published = attachment_events(&events);
    assert!(matches!(
        published.as_slice(),
        [AgentConnectionEvent::ThreadAttachmentUpdated(update)]
            if update.thread_id == "t" && update.generation == 1
    ));
    manager.shutdown();
}

#[test]
fn an_older_cli_without_attachments_is_unsupported_for_the_generation() {
    let (manager, spawner) = manager_with_fake();
    let read = manager.list_thread_attachments("t".into(), false);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    endpoint.send(
        json!({"id": request["id"], "error": {"code": -32601, "message": "Method not found"}}),
    );
    assert_eq!(
        wait_value(&read).unwrap_err(),
        AgentAttachmentError::Unsupported
    );
    // Later calls are answered locally, without another request.
    assert_eq!(
        wait_value(&manager.list_thread_attachments("u".into(), true)).unwrap_err(),
        AgentAttachmentError::Unsupported
    );
    let add = manager.add_thread_attachment(AgentAttachmentAddRequest {
        generation: 1,
        thread_id: "t".into(),
        attachment_type: "pull_request".into(),
        identity_key: "k".into(),
        payload: json!({}),
    });
    assert_eq!(
        wait_value(&add).unwrap_err(),
        AgentAttachmentError::Unsupported
    );
    std::thread::sleep(Duration::from_millis(20));
    assert!(endpoint.from_client.try_recv().is_err());
    assert!(endpoint.process.is_alive());
    manager.shutdown();
}

#[test]
fn writes_are_bound_to_the_generation_they_were_read_on() {
    let (manager, spawner) = manager_with_fake();
    let read = manager.list_thread_attachments("t".into(), false);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    list_page(&mut endpoint, json!([]), Value::Null);
    assert_eq!(wait_value(&read).unwrap().generation, 1);

    let add = manager.add_thread_attachment(AgentAttachmentAddRequest {
        generation: 1,
        thread_id: "t".into(),
        attachment_type: "pull_request".into(),
        identity_key: "[\"github.com\",\"o\",\"r\",7]".into(),
        payload: json!({"url": "https://github.com/o/r/pull/7", "root": null, "headBranch": null}),
    });
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/attachment/add");
    assert_eq!(
        request["params"]["identityKey"],
        "[\"github.com\",\"o\",\"r\",7]"
    );
    endpoint.respond(&request, json!({"outcome": "created", "attachment": {
        "id": "x", "attachmentType": "pull_request", "identityKey": "[\"github.com\",\"o\",\"r\",7]",
        "payload": {"url": "https://github.com/o/r/pull/7"}, "createdAt": 5
    }}));
    assert!(wait_value(&add).is_ok());
    let remove = manager.remove_thread_attachment(AgentAttachmentRemoveRequest {
        generation: 1,
        thread_id: "t".into(),
        attachment_type: "pull_request".into(),
        identity_key: "[\"github.com\",\"o\",\"r\",7]".into(),
    });
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/attachment/remove");
    endpoint.respond(&request, json!({}));
    assert!(wait_value(&remove).is_ok());

    // The connection dies; a click made against generation 1 is not sent to 2.
    endpoint.close_stdout();
    wait_for_process(&endpoint.process);
    let read = manager.list_thread_attachments("t".into(), false);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    // The new generation starts without the old cache.
    list_page(&mut endpoint, json!([row("a")]), Value::Null);
    assert_eq!(wait_value(&read).unwrap().generation, 2);
    let stale = manager.remove_thread_attachment(AgentAttachmentRemoveRequest {
        generation: 1,
        thread_id: "t".into(),
        attachment_type: "pull_request".into(),
        identity_key: "key-a".into(),
    });
    assert!(wait_value(&stale).is_err());
    std::thread::sleep(Duration::from_millis(20));
    assert!(endpoint.from_client.try_recv().is_err());
    manager.shutdown();
}

#[test]
fn a_background_terminal_reports_after_its_turn_and_clean_is_acknowledged() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let run = manager.run_prompt(request("BGTERM start", Some("t")));
    let (turn_events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "t", "r");
    let command = json!({
        "type": "commandExecution", "id": "call_resp_1", "command": "/bin/zsh -lc 'sleep 30'",
        "cwd": "/repo", "processId": "60110", "source": "unifiedExecStartup", "status": "inProgress",
        "commandActions": [{"type": "unknown", "command": "sleep 30"}],
        "aggregatedOutput": null, "exitCode": null, "durationMs": null
    });
    endpoint.send(json!({"method": "item/started", "params": {"threadId": "t", "turnId": "r", "item": command}}));
    complete(&endpoint, "t", "r", "completed");
    let received = collect_terminal(&turn_events);
    assert!(received.iter().any(|event| matches!(
        event,
        AgentEvent::CommandStarted(started) if started.terminal_process_id.as_deref() == Some("60110")
    )));

    let clean = manager.clean_background_terminals("t".into(), 1);
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/backgroundTerminals/clean");
    assert_eq!(request["params"], json!({"threadId": "t"}));
    endpoint.respond(&request, json!({}));
    assert!(wait_value(&clean).is_ok());
    // The end arrives afterwards under the finished turn.
    let mut completed = command.clone();
    completed["status"] = json!("failed");
    completed["exitCode"] = json!(-1);
    completed["aggregatedOutput"] = json!("bg-start\r\n");
    endpoint.send(
        json!({"method": "item/commandExecution/outputDelta", "params": {
            "threadId": "t", "turnId": "r", "itemId": "call_resp_1", "delta": "bg-start\r\n"
        }}),
    );
    endpoint.send(json!({"method": "item/completed", "params": {"threadId": "t", "turnId": "r", "item": completed}}));
    let published = attachment_events(&events);
    assert!(matches!(
        published.as_slice(),
        [
            AgentConnectionEvent::BackgroundCommandUpdated { turn_id: first, event: AgentEvent::CommandOutputDelta { .. }, .. },
            AgentConnectionEvent::BackgroundCommandUpdated {
                turn_id: second,
                event: AgentEvent::CommandCompleted(done),
                ..
            },
        ] if first == "r" && second == "r" && done.exit_code == Some(-1)
    ));
    assert!(endpoint.process.is_alive());

    // A clean for another generation is refused locally.
    let stale = manager.clean_background_terminals("t".into(), 7);
    assert!(wait_value(&stale).is_err());
    drop(interrupt);
    manager.shutdown();
}

#[test]
fn a_thread_branch_is_recorded_through_metadata_update() {
    let (manager, spawner) = manager_with_fake();
    let update = manager.update_thread_metadata(
        "t".into(),
        ThreadMetadataUpdate {
            git_branch: crate::agent::AgentOptionalField::Value("feat/x".into()),
            ..Default::default()
        },
    );
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/metadata/update");
    // Only the branch of gitInfo is written, and the project is untouched.
    assert_eq!(
        request["params"],
        json!({"threadId": "t", "gitInfo": {"branch": "feat/x"}})
    );
    endpoint.respond(&request, json!({"thread": workspace_thread("t", None)}));
    assert!(wait_value(&update).is_ok());
    manager.shutdown();
}
