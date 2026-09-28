//! Collaboration presets, the server queue, thread goals, and approving an
//! auto-review denial, driven through the scripted transport.

use super::*;
use crate::agent::{
    AgentAutoApprovalReviewStatus, AgentCollaborationModeKind, AgentPromptContext,
    AgentQueueAddRequest, AgentQueueReorderRequest, AgentQueueTarget, AgentQueueUpdateRequest,
    AgentThreadGoalStatus, AgentThreadGoalUpdate,
};

fn goal(thread: &str, status: &str, updated_at: i64) -> Value {
    json!({"threadId":thread,"objective":"Say ok","status":status,"tokenBudget":null,
        "tokensUsed":0,"timeUsedSeconds":0,"createdAt":1,"updatedAt":updated_at})
}

fn queued(id: &str, client: &str) -> Value {
    json!({"id":id,"clientUserMessageId":client,"input":[{"type":"text","text":id,"text_elements":[]}]})
}

fn next_event(
    events: &async_channel::Receiver<AgentConnectionEvent>,
    matches: impl Fn(&AgentConnectionEvent) -> bool,
) -> AgentConnectionEvent {
    let deadline = Instant::now() + WAIT;
    loop {
        match events.try_recv() {
            Ok(event) if matches(&event) => return event,
            Ok(_) => {}
            Err(TryRecvError::Empty) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("expected connection event: {error:?}"),
        }
    }
}

/// Opens generation 1 with a goal read, the cheapest request that needs a
/// connection, and answers it.
fn open(manager: &CodexAppServerManager, spawner: &FakeSpawner, thread: &str) -> FakeEndpoint {
    let read = manager.read_thread_goal(thread.into());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let get = endpoint.recv();
    assert_eq!(get["method"], "thread/goal/get");
    assert_eq!(get["params"], json!({"threadId":thread}));
    endpoint.respond(&get, json!({"goal":null}));
    let read = wait_value(&read).unwrap();
    assert_eq!((read.generation, read.goal), (1, None));
    endpoint
}

#[test]
fn server_started_turns_are_adopted_and_streamed_without_failing_the_connection() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let endpoint = open(&manager, &spawner, "t");
    // A goal continuation: the server starts a turn nobody asked for.
    endpoint.send(json!({"method":"turn/started","params":{"threadId":"t","turn":{"id":"goal-turn","items":[],"status":"inProgress","startedAt":5}}}));
    let AgentConnectionEvent::TurnStarted {
        generation,
        thread_id,
        turn_id,
        run,
    } = next_event(&events, |event| {
        matches!(event, AgentConnectionEvent::TurnStarted { .. })
    })
    else {
        unreachable!()
    };
    assert_eq!(
        (generation, thread_id.as_str(), turn_id.as_str()),
        (1, "t", "goal-turn")
    );
    let (stream, interrupt) = run.take().expect("first taker gets the run").into_parts();
    assert!(run.take().is_none(), "the run is handed out once");
    endpoint.send(json!({"method":"item/agentMessage/delta","params":{"threadId":"t","turnId":"goal-turn","itemId":"m","delta":"done"}}));
    complete(&endpoint, "t", "goal-turn", "completed");
    let streamed = collect_terminal(&stream);
    assert!(
        matches!(&streamed[0], AgentEvent::TurnReady(identity) if identity.turn_id == "goal-turn")
    );
    assert!(streamed.contains(&AgentEvent::TextDelta {
        item_id: "m".into(),
        delta: "done".into()
    }));
    assert_eq!(streamed.last(), Some(&AgentEvent::Completed));
    // Dropping the observer's handle never interrupts server work.
    drop(interrupt);
    // A second server-started turn (the next queued follow-up) is adopted too,
    // and late messages of the finished one stay inert.
    complete(&endpoint, "t", "goal-turn", "completed");
    endpoint.send(json!({"method":"turn/started","params":{"threadId":"t","turn":{"id":"queued-turn","items":[],"status":"inProgress"}}}));
    let AgentConnectionEvent::TurnStarted { run, .. } = next_event(&events, |event| {
        matches!(event, AgentConnectionEvent::TurnStarted { .. })
    }) else {
        unreachable!()
    };
    let (stream, interrupt) = run.take().unwrap().into_parts();
    // An explicit stop still interrupts it.
    assert_eq!(
        interrupt.as_ref().unwrap().interrupt(),
        Ok(AgentInterruptOutcome::Requested)
    );
    let mut endpoint = endpoint;
    let interrupt_request = endpoint.recv();
    assert_eq!(interrupt_request["method"], "turn/interrupt");
    assert_eq!(
        interrupt_request["params"],
        json!({"threadId":"t","turnId":"queued-turn"})
    );
    endpoint.respond(&interrupt_request, json!({}));
    complete(&endpoint, "t", "queued-turn", "interrupted");
    assert_eq!(
        collect_terminal(&stream).last(),
        Some(&AgentEvent::Interrupted)
    );
    // The generation is still usable.
    let read = manager.read_thread_goal("t".into());
    let get = endpoint.recv();
    endpoint.respond(&get, json!({"goal":goal("t","paused",3)}));
    assert_eq!(
        wait_value(&read).unwrap().goal.unwrap().status,
        AgentThreadGoalStatus::Paused
    );
    manager.shutdown();
}

#[test]
fn only_turn_started_can_introduce_an_unowned_turn() {
    let (manager, spawner) = manager_with_fake();
    let endpoint = open(&manager, &spawner, "t");
    endpoint.send(json!({"method":"item/agentMessage/delta","params":{"threadId":"t","turnId":"ghost","itemId":"m","delta":"x"}}));
    wait_for_process(&spawner.process(0));
    manager.shutdown();
}

#[test]
fn goal_writes_are_bound_to_their_generation_and_load_the_thread_first() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let mut endpoint = open(&manager, &spawner, "t");
    let stale = manager.update_thread_goal(AgentThreadGoalUpdate {
        generation: 7,
        thread_id: "t".into(),
        objective: Some("Say ok".into()),
        status: Some(AgentThreadGoalStatus::Active),
        token_budget: AgentOptionalField::Unspecified,
    });
    assert!(wait_value(&stale).unwrap_err().contains("连接已变化"));
    let set = manager.update_thread_goal(AgentThreadGoalUpdate {
        generation: 1,
        thread_id: "t".into(),
        objective: Some("Say ok".into()),
        status: Some(AgentThreadGoalStatus::Active),
        token_budget: AgentOptionalField::Unspecified,
    });
    let resume = endpoint.recv();
    assert_eq!(resume["method"], "thread/resume");
    endpoint.respond(&resume, json!({"thread":{"id":"t"}}));
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/goal/set");
    assert_eq!(
        request["params"],
        json!({"threadId":"t","objective":"Say ok","status":"active"})
    );
    // The notification may precede the response; both are delivered.
    endpoint.send(json!({"method":"thread/goal/updated","params":{"threadId":"t","turnId":null,"goal":goal("t","active",2)}}));
    endpoint.respond(&request, json!({"goal":goal("t","active",2)}));
    assert_eq!(wait_value(&set).unwrap().goal.unwrap().updated_at, 2);
    assert!(matches!(
        next_event(&events, |event| matches!(
            event,
            AgentConnectionEvent::ThreadGoalUpdated { .. }
        )),
        AgentConnectionEvent::ThreadGoalUpdated {
            generation: 1,
            turn_id: None,
            ..
        }
    ));
    let clear = manager.clear_thread_goal("t".into(), 1);
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/goal/clear");
    endpoint.send(json!({"method":"thread/goal/cleared","params":{"threadId":"t"}}));
    endpoint.respond(&request, json!({"cleared":true}));
    assert!(wait_value(&clear).unwrap());
    next_event(&events, |event| {
        matches!(event, AgentConnectionEvent::ThreadGoalCleared { .. })
    });
    // A malformed goal answer is an error, not a silent success.
    let read = manager.read_thread_goal("t".into());
    let get = endpoint.recv();
    endpoint.respond(&get, json!({"goal":goal("other","active",2)}));
    assert!(wait_value(&read).is_err());
    manager.shutdown();
}

#[test]
fn queue_requests_encode_their_input_and_validate_answers() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let mut endpoint = open(&manager, &spawner, "t");
    let add = manager.add_queued_submission(AgentQueueAddRequest {
        generation: 1,
        thread_id: "t".into(),
        client_message_id: "c1".into(),
        prompt: "Reply A".into(),
        context: AgentPromptContext::default(),
    });
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/queue/add");
    assert_eq!(
        request["params"],
        json!({"threadId":"t","clientUserMessageId":"c1","input":[{"type":"text","text":"Reply A"}]})
    );
    endpoint.respond(&request, json!({"queuedSubmission":queued("q1","c1")}));
    endpoint
        .send(json!({"method":"thread/queue/changed","params":{"threadId":"t"},"emittedAtMs":1}));
    assert_eq!(wait_value(&add).unwrap().id, "q1");
    assert!(matches!(
        next_event(&events, |event| matches!(event, AgentConnectionEvent::ThreadQueueChanged { .. })),
        AgentConnectionEvent::ThreadQueueChanged { generation: 1, ref thread_id } if thread_id == "t"
    ));

    let update = manager.update_queued_submission(AgentQueueUpdateRequest {
        generation: 1,
        thread_id: "t".into(),
        queued_submission_id: "q1".into(),
        prompt: "Reply A2".into(),
        context: AgentPromptContext::default(),
    });
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/queue/update");
    assert_eq!(request["params"]["queuedSubmissionId"], "q1");
    endpoint.respond(&request, json!({"queuedSubmission":queued("q2","c1")}));
    assert!(
        wait_value(&update).is_err(),
        "an answer for another row is rejected"
    );

    let reorder = manager.reorder_queued_submissions(AgentQueueReorderRequest {
        generation: 1,
        thread_id: "t".into(),
        queued_submission_ids: vec!["q2".into(), "q1".into()],
    });
    let request = endpoint.recv();
    assert_eq!(
        request["params"]["queuedSubmissionIds"],
        json!(["q2", "q1"])
    );
    endpoint.send(json!({"id":request["id"].clone(),"error":{"code":-32600,"message":"queue reorder must include every queued submission exactly once"}}));
    assert!(wait_value(&reorder).unwrap_err().contains("exactly once"));

    let delete = manager.delete_queued_submission(AgentQueueTarget {
        generation: 1,
        thread_id: "t".into(),
        queued_submission_id: Some("q1".into()),
    });
    let request = endpoint.recv();
    assert_eq!(
        request["params"],
        json!({"threadId":"t","queuedSubmissionId":"q1"})
    );
    endpoint.respond(&request, json!({"deleted":false}));
    assert!(
        !wait_value(&delete).unwrap(),
        "deleted=false is reported as-is"
    );

    let start = manager.start_queued_submission(AgentQueueTarget {
        generation: 1,
        thread_id: "t".into(),
        queued_submission_id: None,
    });
    let resume = endpoint.recv();
    assert_eq!(resume["method"], "thread/resume");
    endpoint.respond(&resume, json!({"thread":{"id":"t"}}));
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/queue/start");
    assert_eq!(
        request["params"],
        json!({"threadId":"t","queuedSubmissionId":null})
    );
    endpoint.respond(
        &request,
        json!({"turn":{"id":"started","items":[],"status":"inProgress"}}),
    );
    assert_eq!(wait_value(&start).unwrap(), "started");

    let stale = manager.delete_queued_submission(AgentQueueTarget {
        generation: 2,
        thread_id: "t".into(),
        queued_submission_id: Some("q1".into()),
    });
    assert!(wait_value(&stale).is_err());
    manager.shutdown();
}

#[test]
fn queue_list_follows_cursors_and_rejects_a_repeated_cursor() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open(&manager, &spawner, "t");
    let list = manager.list_thread_queue("t".into());
    let first = endpoint.recv();
    assert_eq!(first["params"], json!({"threadId":"t","cursor":null}));
    endpoint.respond(&first, json!({"data":[queued("q1","c1")],"nextCursor":"1"}));
    let second = endpoint.recv();
    assert_eq!(second["params"], json!({"threadId":"t","cursor":"1"}));
    endpoint.respond(
        &second,
        json!({"data":[queued("q2","c2")],"nextCursor":null}),
    );
    let queue = wait_value(&list).unwrap();
    assert_eq!(
        queue
            .submissions
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["q1", "q2"]
    );
    let looping = manager.list_thread_queue("t".into());
    let first = endpoint.recv();
    endpoint.respond(&first, json!({"data":[],"nextCursor":"1"}));
    let second = endpoint.recv();
    endpoint.respond(&second, json!({"data":[],"nextCursor":"1"}));
    assert!(wait_value(&looping).unwrap_err().contains("重复的游标"));
    manager.shutdown();
}

#[test]
fn collaboration_presets_are_read_once_per_generation_and_shape_turn_start() {
    let (manager, spawner) = manager_with_fake();
    let mut plan = request("plan it", Some("t"));
    plan.context.plan_mode = Some(true);
    let (first, _first_handle) = manager.run_prompt(plan.clone()).into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    endpoint.respond(&resume, json!({"thread":{"id":"t"}}));
    let list = endpoint.recv();
    assert_eq!(list["method"], "collaborationMode/list");
    assert_eq!(list["params"], json!({}));
    endpoint.respond(
        &list,
        json!({"data":[
            {"name":"Plan","mode":"plan","model":null,"reasoning_effort":"medium"},
            {"name":"Default","mode":"default","model":null,"reasoning_effort":null}
        ]}),
    );
    let start = endpoint.recv();
    assert_eq!(start["method"], "turn/start");
    assert_eq!(
        start["params"]["collaborationMode"],
        json!({"mode":"plan","settings":{"model":"gpt-test","reasoning_effort":"medium","developer_instructions":null}})
    );
    assert!(start["params"]["model"].is_null() && start["params"]["effort"].is_null());
    endpoint.respond(&start, json!({"turn":{"id":"one"}}));
    complete(&endpoint, "t", "one", "completed");
    collect_terminal(&first);

    plan.context.plan_mode = Some(false);
    let (second, _second_handle) = manager.run_prompt(plan).into_parts();
    let start = endpoint.recv();
    assert_eq!(
        start["method"], "turn/start",
        "the cached presets are reused"
    );
    assert_eq!(start["params"]["collaborationMode"]["mode"], "default");
    endpoint.respond(&start, json!({"turn":{"id":"two"}}));
    complete(&endpoint, "t", "two", "completed");
    collect_terminal(&second);

    let modes = wait_value(&manager.load_collaboration_modes()).unwrap();
    assert_eq!(modes.generation, 1);
    assert!(modes.preset(AgentCollaborationModeKind::Plan).is_some());
    assert_eq!(
        endpoint
            .methods()
            .iter()
            .filter(|m| **m == "collaborationMode/list")
            .count(),
        1
    );
    manager.shutdown();
}

#[test]
fn a_failed_preset_read_still_sends_default_but_refuses_plan() {
    let (manager, spawner) = manager_with_fake();
    let mut plan = request("plan it", Some("t"));
    plan.context.plan_mode = Some(true);
    let (first, _handle) = manager.run_prompt(plan).into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    endpoint.respond(&resume, json!({"thread":{"id":"t"}}));
    let list = endpoint.recv();
    endpoint.send(json!({"id":list["id"].clone(),"error":{"code":-32603,"message":"boom"}}));
    let events = collect_terminal(&first);
    assert!(matches!(events.last(), Some(AgentEvent::Failed(message)) if message.contains("Plan")));
    manager.shutdown();
}

#[test]
fn approving_a_denial_sends_the_derived_event_once() {
    let (manager, spawner) = manager_with_fake();
    let mut endpoint = open(&manager, &spawner, "t");
    let params = json!({
        "threadId":"t","turnId":"turn","reviewId":"review","targetItemId":"call",
        "startedAtMs":1,"completedAtMs":2,"decisionSource":"agent",
        "review":{"status":"denied","riskLevel":"high","userAuthorization":"low","rationale":"risky"},
        "action":{"type":"command","source":"shell","command":"rm -rf /tmp/x","cwd":"/tmp"}
    });
    let review = super::super::super::auto_approval::parse_review(
        &json!({"method":"item/autoApprovalReview/completed","params":params}),
    )
    .unwrap();
    assert_eq!(review.status, AgentAutoApprovalReviewStatus::Denied);
    let approval = crate::agent::AgentAutoReviewApproval {
        generation: 1,
        review: review.clone(),
    };
    let first = manager.approve_auto_review_denial(approval.clone());
    let request = endpoint.recv();
    assert_eq!(request["method"], "thread/approveGuardianDeniedAction");
    assert_eq!(request["params"]["threadId"], "t");
    assert_eq!(request["params"]["event"]["id"], "review");
    assert_eq!(request["params"]["event"]["action"]["type"], "command");
    // A second approval while the first is in flight is refused locally.
    let concurrent = manager.approve_auto_review_denial(approval.clone());
    assert!(wait_value(&concurrent).unwrap_err().contains("正在记录"));
    endpoint.respond(&request, json!({}));
    wait_value(&first).unwrap();
    // An approved review is never sent again in this generation.
    let again = manager.approve_auto_review_denial(approval.clone());
    assert!(wait_value(&again).unwrap_err().contains("已记录"));
    // A click from an older generation is inert.
    let stale = manager.approve_auto_review_denial(crate::agent::AgentAutoReviewApproval {
        generation: 9,
        review: crate::agent::AgentAutoApprovalReview {
            key: crate::agent::AgentAutoApprovalReviewKey {
                review_id: "other".into(),
                ..review.key.clone()
            },
            ..review
        },
    });
    assert!(wait_value(&stale).is_err());
    assert_eq!(
        endpoint
            .methods()
            .iter()
            .filter(|m| **m == "thread/approveGuardianDeniedAction")
            .count(),
        1
    );
    manager.shutdown();
}

fn turn_started(thread: &str, turn: &str) -> Value {
    json!({"method":"turn/started","params":{"threadId":thread,"turn":{"id":turn,"items":[],"status":"inProgress"}}})
}

fn delta(thread: &str, turn: &str, text: &str) -> Value {
    json!({"method":"item/agentMessage/delta","params":{"threadId":thread,"turnId":turn,"itemId":format!("m-{turn}"),"delta":text}})
}

#[test]
fn a_server_turn_that_races_a_pending_turn_start_is_streamed_as_its_own_turn() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let (ours, _handle) = manager.run_prompt(request("mine", Some("t"))).into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    endpoint.respond(&resume, json!({"thread":{"id":"t"}}));
    let start = endpoint.recv();
    assert_eq!(start["method"], "turn/start");
    // The server advanced its queue before it answered our turn/start.
    endpoint.send(turn_started("t", "queued"));
    endpoint.send(delta("t", "queued", "queued answer"));
    complete(&endpoint, "t", "queued", "completed");
    endpoint.send(turn_started("t", "mine"));
    endpoint.respond(&start, json!({"turn":{"id":"mine"}}));
    endpoint.send(delta("t", "mine", "my answer"));
    complete(&endpoint, "t", "mine", "completed");
    let mine = collect_terminal(&ours);
    assert!(matches!(&mine[0], AgentEvent::TurnReady(identity) if identity.turn_id == "mine"));
    assert!(
        mine.iter()
            .any(|e| matches!(e, AgentEvent::TextDelta { delta, .. } if delta == "my answer"))
    );
    assert!(
        !mine
            .iter()
            .any(|e| matches!(e, AgentEvent::TextDelta { delta, .. } if delta == "queued answer"))
    );
    let AgentConnectionEvent::TurnStarted { turn_id, run, .. } = next_event(&events, |event| {
        matches!(event, AgentConnectionEvent::TurnStarted { .. })
    }) else {
        unreachable!()
    };
    assert_eq!(turn_id, "queued");
    let (queued, _) = run.take().unwrap().into_parts();
    let queued = collect_terminal(&queued);
    assert!(
        queued
            .iter()
            .any(|e| matches!(e, AgentEvent::TextDelta { delta, .. } if delta == "queued answer"))
    );
    assert_eq!(queued.last(), Some(&AgentEvent::Completed));
    assert!(endpoint.process.is_alive());
    manager.shutdown();
}

#[test]
fn a_rejected_turn_start_hands_the_early_server_turn_over() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let (ours, _handle) = manager.run_prompt(request("mine", Some("t"))).into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    endpoint.respond(&resume, json!({"thread":{"id":"t"}}));
    let start = endpoint.recv();
    endpoint.send(turn_started("t", "goal"));
    endpoint.send(delta("t", "goal", "continuing"));
    endpoint.send(json!({"id":start["id"].clone(),"error":{"code":-32600,"message":"thread already has an active turn"}}));
    assert!(
        matches!(collect_terminal(&ours).last(), Some(AgentEvent::Failed(message)) if message.contains("active turn"))
    );
    let AgentConnectionEvent::TurnStarted { run, .. } = next_event(&events, |event| {
        matches!(event, AgentConnectionEvent::TurnStarted { .. })
    }) else {
        unreachable!()
    };
    complete(&endpoint, "t", "goal", "completed");
    let (goal, _) = run.take().unwrap().into_parts();
    let goal = collect_terminal(&goal);
    assert!(
        goal.iter()
            .any(|e| matches!(e, AgentEvent::TextDelta { delta, .. } if delta == "continuing"))
    );
    assert!(endpoint.process.is_alive());
    manager.shutdown();
}

#[test]
fn an_unclaimed_server_turn_can_be_claimed_later_with_its_whole_stream() {
    let (manager, spawner) = manager_with_fake();
    let endpoint = open(&manager, &spawner, "t");
    endpoint.send(turn_started("t", "goal"));
    endpoint.send(delta("t", "goal", "before open"));
    let deadline = Instant::now() + WAIT;
    let (turn_id, run) = loop {
        if let Some(claimed) = manager.take_server_turn("t") {
            break claimed;
        }
        assert!(Instant::now() < deadline, "server turn was not registered");
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(turn_id, "goal");
    assert!(manager.take_server_turn("t").is_none(), "claimed once");
    complete(&endpoint, "t", "goal", "completed");
    let (stream, _) = run.into_parts();
    let events = collect_terminal(&stream);
    assert!(matches!(&events[0], AgentEvent::TurnReady(identity) if identity.turn_id == "goal"));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::TextDelta { delta, .. } if delta == "before open"))
    );
    assert!(
        manager.take_server_turn("t").is_none(),
        "finished turns are forgotten"
    );
    manager.shutdown();
}
