use super::*;
use crate::agent::{
    AgentMcpElicitationAction, AgentMcpElicitationContent, AgentMcpElicitationFieldValue,
    AgentMcpElicitationHandle, AgentMcpElicitationMode, AgentMcpElicitationRequest,
    AgentMcpElicitationResponse, AgentMcpElicitationValue, AgentServerRequestFailureKind,
};

fn form_elicitation(id: Value, thread_id: &str, turn_id: Option<Value>) -> Value {
    let mut params = json!({
        "serverName": "fixture-mcp",
        "threadId": thread_id,
        "mode": "form",
        "message": "请填写部署信息",
        "requestedSchema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "title": "名称", "minLength": 2 },
                "replicas": { "type": "integer", "minimum": 1, "maximum": 8, "default": 2 },
                "enabled": { "type": "boolean", "default": false },
                "region": { "type": "string", "enum": ["us", "eu"] },
                "features": {
                    "type": "array",
                    "items": { "type": "string", "enum": ["logs", "metrics"] }
                }
            },
            "required": ["name"]
        }
    });
    if let Some(turn_id) = turn_id {
        params["turnId"] = turn_id;
    }
    json!({
        "id": id,
        "method": "mcpServer/elicitation/request",
        "params": params
    })
}

fn url_elicitation(id: Value, thread_id: &str) -> Value {
    json!({
        "id": id,
        "method": "mcpServer/elicitation/request",
        "params": {
            "serverName": "fixture-mcp",
            "threadId": thread_id,
            "turnId": null,
            "mode": "url",
            "elicitationId": "elicit-url-1",
            "message": "请在浏览器完成登录",
            "url": "https://example.com/device"
        }
    })
}

fn resolved(thread_id: &str, request_id: Value) -> Value {
    json!({
        "method": "serverRequest/resolved",
        "params": { "threadId": thread_id, "requestId": request_id }
    })
}

fn accept_name(name: &str) -> AgentMcpElicitationResponse {
    AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
        fields: vec![AgentMcpElicitationFieldValue {
            name: "name".into(),
            value: AgentMcpElicitationValue::String(name.to_owned()),
        }],
    })
}

fn drain_events(receiver: &async_channel::Receiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        events.push(event);
    }
    events
}

/// Ordered connection-event buffer. Tests must never drop an unrelated
/// lifecycle event while waiting for a different one: two concurrent
/// elicitations can arrive in any order.
struct ConnectionEvents {
    receiver: async_channel::Receiver<AgentConnectionEvent>,
    buffered: Vec<AgentConnectionEvent>,
}

impl ConnectionEvents {
    fn new(manager: &CodexAppServerManager) -> Self {
        Self {
            receiver: manager.subscribe_connection_events(),
            buffered: Vec::new(),
        }
    }

    fn wait_for(
        &mut self,
        expected: impl Fn(&AgentConnectionEvent) -> bool,
    ) -> AgentConnectionEvent {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(index) = self.buffered.iter().position(&expected) {
                return self.buffered.remove(index);
            }
            match self.receiver.try_recv() {
                Ok(event) => self.buffered.push(event),
                Err(TryRecvError::Empty) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!(
                    "connection event stream did not produce the event: {error:?}; buffered={:?}",
                    self.buffered
                ),
            }
        }
    }

    fn elicitation(
        &mut self,
        request_id: &AgentServerRequestId,
    ) -> (AgentMcpElicitationRequest, AgentMcpElicitationHandle) {
        let event = self.wait_for(|event| {
            matches!(
                event,
                AgentConnectionEvent::McpElicitationRequested { request, .. }
                    if &request.request_id == request_id
            )
        });
        let AgentConnectionEvent::McpElicitationRequested { request, responder } = event else {
            unreachable!("event was matched above")
        };
        (request, responder)
    }

    fn resolved(&mut self, request_id: &AgentServerRequestId) {
        self.wait_for(|event| {
            matches!(
                event,
                AgentConnectionEvent::McpElicitationResolved { identity, .. }
                    if &identity.request_id == request_id
            )
        });
    }

    fn failed(
        &mut self,
        request_id: &AgentServerRequestId,
    ) -> (String, AgentServerRequestFailureKind) {
        let event = self.wait_for(|event| {
            matches!(
                event,
                AgentConnectionEvent::McpElicitationFailed { identity, .. }
                    if &identity.request_id == request_id
            )
        });
        let AgentConnectionEvent::McpElicitationFailed {
            thread_id, kind, ..
        } = event
        else {
            unreachable!("event was matched above")
        };
        (thread_id, kind)
    }
}

/// A connection with no turn at all must still deliver and answer an
/// elicitation, and the original JSON-RPC id type must survive the round trip.
#[test]
fn elicitation_without_a_turn_answers_with_the_original_id() {
    let (manager, spawner) = manager_with_fake();
    let mut events = ConnectionEvents::new(&manager);
    let catalog = manager.load_model_catalog();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let model_request = endpoint.recv();
    endpoint.respond(&model_request, model_page());
    assert!(wait_value(&catalog).is_ok());

    endpoint.send(form_elicitation(json!(11), "thr_idle", None));
    let (request, responder) = events.elicitation(&AgentServerRequestId::Number(11));
    assert_eq!(request.thread_id, "thr_idle");
    assert_eq!(
        request.turn_id,
        crate::agent::AgentOptionalField::Unspecified
    );
    assert_eq!(request.server_name, "fixture-mcp");
    let AgentMcpElicitationMode::Form(form) = &request.mode else {
        panic!("expected form mode");
    };
    assert_eq!(form.fields.len(), 5);

    responder
        .respond(AgentMcpElicitationResponse::accept(
            AgentMcpElicitationContent {
                fields: vec![
                    AgentMcpElicitationFieldValue {
                        name: "name".into(),
                        value: AgentMcpElicitationValue::String("echora".into()),
                    },
                    AgentMcpElicitationFieldValue {
                        name: "replicas".into(),
                        value: AgentMcpElicitationValue::Number(serde_json::Number::from(3)),
                    },
                ],
            },
        ))
        .unwrap();
    let response = endpoint.recv();
    assert_eq!(response["id"], json!(11));
    assert_eq!(
        response["result"],
        json!({ "action": "accept", "content": { "name": "echora", "replicas": 3 } })
    );
    // The request is answered exactly once.
    assert!(
        responder
            .respond(AgentMcpElicitationResponse::decline())
            .is_err()
    );
    assert!(endpoint.from_client.try_recv().is_err());

    endpoint.send(resolved("thr_idle", json!(11)));
    events.resolved(&AgentServerRequestId::Number(11));
    // Duplicate and late resolutions stay idempotent.
    endpoint.send(resolved("thr_idle", json!(11)));
    assert!(
        responder
            .respond(AgentMcpElicitationResponse::decline())
            .is_err()
    );
    let catalog = manager.load_model_catalog();
    let follow_up = endpoint.recv();
    assert_eq!(follow_up["method"], "model/list");
    endpoint.respond(&follow_up, model_page());
    assert!(wait_value(&catalog).is_ok());
    assert!(endpoint.process.is_alive());
    manager.shutdown();
}

/// turnId may be null or reference an already finished turn: the card stays
/// answerable and must not create, restart, or finish that turn.
#[test]
fn elicitation_survives_null_and_completed_turn_identities() {
    let (manager, spawner) = manager_with_fake();
    let mut events = ConnectionEvents::new(&manager);
    let run = manager.run_prompt(request("finish then elicit", Some("thr_done")));
    let (turn_events, interrupt) = run.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "thr_done", "turn_done");
    complete(&endpoint, "thr_done", "turn_done", "completed");
    assert!(matches!(
        collect_terminal(&turn_events).last(),
        Some(AgentEvent::Completed)
    ));

    endpoint.send(form_elicitation(
        json!("after-null"),
        "thr_done",
        Some(Value::Null),
    ));
    endpoint.send(form_elicitation(
        json!("after-completed"),
        "thr_done",
        Some(json!("turn_done")),
    ));
    let (null_turn, null_responder) =
        events.elicitation(&AgentServerRequestId::String("after-null".into()));
    assert_eq!(null_turn.turn_id, crate::agent::AgentOptionalField::Null);
    let (completed_turn, completed_responder) =
        events.elicitation(&AgentServerRequestId::String("after-completed".into()));
    assert_eq!(
        completed_turn.turn_id,
        crate::agent::AgentOptionalField::Value("turn_done".to_owned())
    );

    null_responder
        .respond(AgentMcpElicitationResponse::decline())
        .unwrap();
    let first = endpoint.recv();
    assert_eq!(first["id"], json!("after-null"));
    assert_eq!(first["result"], json!({ "action": "decline" }));
    assert!(first["result"].get("content").is_none());
    completed_responder
        .respond(AgentMcpElicitationResponse::cancel())
        .unwrap();
    let second = endpoint.recv();
    assert_eq!(second["id"], json!("after-completed"));
    assert_eq!(second["result"], json!({ "action": "cancel" }));

    endpoint.send(resolved("thr_done", json!("after-null")));
    endpoint.send(resolved("thr_done", json!("after-completed")));
    events.resolved(&AgentServerRequestId::String("after-completed".into()));
    events.resolved(&AgentServerRequestId::String("after-null".into()));
    // The finished turn is untouched: no further terminal or lifecycle events.
    assert!(turn_events.try_recv().is_err());
    let catalog = manager.load_model_catalog();
    let follow_up = endpoint.recv();
    endpoint.respond(&follow_up, model_page());
    assert!(wait_value(&catalog).is_ok());
    drop(interrupt);
    manager.shutdown();
}

/// Two threads with two live elicitations stay isolated, and neither one
/// disturbs its own turn.
#[test]
fn concurrent_elicitations_on_two_threads_stay_isolated() {
    let (manager, spawner) = manager_with_fake();
    let mut events = ConnectionEvents::new(&manager);
    let first = manager.run_prompt(request("a", Some("thr_a")));
    let second = manager.run_prompt(request("b", Some("thr_b")));
    let (events_a, interrupt_a) = first.into_parts();
    let (events_b, interrupt_b) = second.into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);

    let mut turn_requests = HashMap::new();
    while turn_requests.len() < 2 {
        let message = endpoint.recv();
        match message["method"].as_str().unwrap() {
            "thread/resume" => {
                let thread_id = message["params"]["threadId"].as_str().unwrap();
                endpoint.respond(&message, json!({ "thread": { "id": thread_id } }));
            }
            "turn/start" => {
                turn_requests.insert(
                    message["params"]["threadId"].as_str().unwrap().to_owned(),
                    message,
                );
            }
            method => panic!("unexpected method: {method}"),
        }
    }
    endpoint.respond(
        turn_requests.get("thr_a").unwrap(),
        json!({ "turn": { "id": "turn_a" } }),
    );
    endpoint.respond(
        turn_requests.get("thr_b").unwrap(),
        json!({ "turn": { "id": "turn_b" } }),
    );
    endpoint.send(form_elicitation(
        json!("elicit-a"),
        "thr_a",
        Some(json!("turn_a")),
    ));
    endpoint.send(form_elicitation(json!(202), "thr_b", Some(json!("turn_b"))));

    let (request_a, responder_a) =
        events.elicitation(&AgentServerRequestId::String("elicit-a".into()));
    let (request_b, responder_b) = events.elicitation(&AgentServerRequestId::Number(202));
    assert_eq!(request_a.thread_id, "thr_a");
    assert_eq!(request_b.thread_id, "thr_b");
    assert_eq!(request_a.generation, request_b.generation);

    responder_b
        .respond(AgentMcpElicitationResponse::decline())
        .unwrap();
    let response_b = endpoint.recv();
    assert_eq!(response_b["id"], json!(202));
    responder_a.respond(accept_name("echora")).unwrap();
    let response_a = endpoint.recv();
    assert_eq!(response_a["id"], json!("elicit-a"));
    assert_eq!(response_a["result"]["content"], json!({ "name": "echora" }));

    endpoint.send(resolved("thr_a", json!("elicit-a")));
    endpoint.send(resolved("thr_b", json!(202)));
    events.resolved(&AgentServerRequestId::Number(202));
    events.resolved(&AgentServerRequestId::String("elicit-a".into()));

    // Neither elicitation ended, restarted, or replaced its turn.
    for receiver in [&events_a, &events_b] {
        let produced = drain_events(receiver);
        assert!(
            !produced.iter().any(|event| matches!(
                event,
                AgentEvent::Completed
                    | AgentEvent::Interrupted
                    | AgentEvent::Failed(_)
                    | AgentEvent::ServerRequestFailed { .. }
            )),
            "{produced:?}"
        );
    }
    complete(&endpoint, "thr_a", "turn_a", "completed");
    complete(&endpoint, "thr_b", "turn_b", "completed");
    assert!(matches!(
        collect_terminal(&events_a).last(),
        Some(AgentEvent::Completed)
    ));
    assert!(matches!(
        collect_terminal(&events_b).last(),
        Some(AgentEvent::Completed)
    ));
    drop(interrupt_a);
    drop(interrupt_b);
    manager.shutdown();
}

#[test]
fn url_mode_round_trips_without_inventing_content() {
    let (manager, spawner) = manager_with_fake();
    let mut events = ConnectionEvents::new(&manager);
    let catalog = manager.load_model_catalog();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let model_request = endpoint.recv();
    endpoint.respond(&model_request, model_page());
    wait_value(&catalog).unwrap();

    endpoint.send(url_elicitation(json!(7), "thr_url"));
    let (request, responder) = events.elicitation(&AgentServerRequestId::Number(7));
    let AgentMcpElicitationMode::Url(url) = &request.mode else {
        panic!("expected url mode");
    };
    assert_eq!(url.elicitation_id, "elicit-url-1");
    assert_eq!(url.url, "https://example.com/device");
    assert_eq!(request.turn_id, crate::agent::AgentOptionalField::Null);

    // Accept mirrors the explicit protocol action and never carries content.
    responder
        .respond(AgentMcpElicitationResponse::accept(
            AgentMcpElicitationContent::default(),
        ))
        .unwrap();
    let response = endpoint.recv();
    assert_eq!(response["id"], json!(7));
    assert_eq!(response["result"], json!({ "action": "accept" }));
    endpoint.send(resolved("thr_url", json!(7)));
    events.resolved(&AgentServerRequestId::Number(7));
    assert!(endpoint.process.is_alive());
    manager.shutdown();
}

#[test]
fn invalid_accept_payload_keeps_the_request_answerable() {
    let (manager, spawner) = manager_with_fake();
    let mut events = ConnectionEvents::new(&manager);
    let catalog = manager.load_model_catalog();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let model_request = endpoint.recv();
    endpoint.respond(&model_request, model_page());
    wait_value(&catalog).unwrap();

    endpoint.send(form_elicitation(json!(31), "thr_validate", None));
    let (_, responder) = events.elicitation(&AgentServerRequestId::Number(31));

    // Missing required field, wrong type, and out-of-range values never write.
    for invalid in [
        AgentMcpElicitationResponse::accept(AgentMcpElicitationContent::default()),
        AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
            fields: vec![
                AgentMcpElicitationFieldValue {
                    name: "name".into(),
                    value: AgentMcpElicitationValue::String("ok".into()),
                },
                AgentMcpElicitationFieldValue {
                    name: "replicas".into(),
                    value: AgentMcpElicitationValue::Number(serde_json::Number::from(99)),
                },
            ],
        }),
        AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
            fields: vec![AgentMcpElicitationFieldValue {
                name: "name".into(),
                value: AgentMcpElicitationValue::Boolean(true),
            }],
        }),
        AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
            fields: vec![
                AgentMcpElicitationFieldValue {
                    name: "name".into(),
                    value: AgentMcpElicitationValue::String("ok".into()),
                },
                AgentMcpElicitationFieldValue {
                    name: "region".into(),
                    value: AgentMcpElicitationValue::String("mars".into()),
                },
            ],
        }),
        AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
            fields: vec![
                AgentMcpElicitationFieldValue {
                    name: "name".into(),
                    value: AgentMcpElicitationValue::String("ok".into()),
                },
                AgentMcpElicitationFieldValue {
                    name: "features".into(),
                    value: AgentMcpElicitationValue::StringArray(vec![
                        "logs".into(),
                        "logs".into(),
                    ]),
                },
            ],
        }),
        // decline/cancel are never turned into an accept payload
        AgentMcpElicitationResponse {
            action: AgentMcpElicitationAction::Cancel,
            content: Some(AgentMcpElicitationContent::default()),
        },
    ] {
        assert!(responder.respond(invalid).is_err());
        assert!(
            endpoint.from_client.try_recv().is_err(),
            "an invalid payload must not write a response"
        );
    }

    responder
        .respond(AgentMcpElicitationResponse::accept(
            AgentMcpElicitationContent {
                fields: vec![
                    AgentMcpElicitationFieldValue {
                        name: "name".into(),
                        value: AgentMcpElicitationValue::String("echora".into()),
                    },
                    AgentMcpElicitationFieldValue {
                        name: "enabled".into(),
                        value: AgentMcpElicitationValue::Boolean(true),
                    },
                    AgentMcpElicitationFieldValue {
                        name: "region".into(),
                        value: AgentMcpElicitationValue::String("eu".into()),
                    },
                    AgentMcpElicitationFieldValue {
                        name: "features".into(),
                        value: AgentMcpElicitationValue::StringArray(vec!["logs".into()]),
                    },
                ],
            },
        ))
        .unwrap();
    let response = endpoint.recv();
    assert_eq!(
        response["result"],
        json!({
            "action": "accept",
            "content": { "name": "echora", "enabled": true, "region": "eu", "features": ["logs"] }
        })
    );
    assert!(endpoint.from_client.try_recv().is_err());
    endpoint.send(resolved("thr_validate", json!(31)));
    events.resolved(&AgentServerRequestId::Number(31));
    manager.shutdown();
}

#[test]
fn duplicate_elicitation_request_id_fails_the_generation() {
    let (manager, spawner) = manager_with_fake();
    let mut events = ConnectionEvents::new(&manager);
    let catalog = manager.load_model_catalog();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let model_request = endpoint.recv();
    endpoint.respond(&model_request, model_page());
    wait_value(&catalog).unwrap();

    endpoint.send(form_elicitation(json!(41), "thr_dup", None));
    let (_, responder) = events.elicitation(&AgentServerRequestId::Number(41));
    endpoint.send(form_elicitation(json!(41), "thr_dup", None));

    wait_for_process(&endpoint.process);
    assert!(
        responder
            .respond(AgentMcpElicitationResponse::decline())
            .is_err()
    );
    assert!(endpoint.from_client.try_recv().is_err());
    manager.shutdown();
}

#[test]
fn openai_form_modes_reply_an_error_then_fail_the_generation() {
    for mode in ["openai/form", "openaiForm"] {
        let (manager, spawner) = manager_with_fake();
        let catalog = manager.load_model_catalog();
        let mut endpoint = spawner.next_endpoint();
        handshake(&mut endpoint);
        let model_request = endpoint.recv();
        endpoint.respond(&model_request, model_page());
        wait_value(&catalog).unwrap();

        let mut message = form_elicitation(json!(51), "thr_unsupported", None);
        message["params"]["mode"] = json!(mode);
        endpoint.send(message);
        let error = endpoint.recv();
        assert_eq!(error["id"], json!(51));
        assert_eq!(error["error"]["code"], json!(-32602));
        wait_for_process(&endpoint.process);
        manager.shutdown();
    }
}

#[test]
fn thread_closure_invalidates_pending_elicitations() {
    let (manager, spawner) = manager_with_fake();
    let mut events = ConnectionEvents::new(&manager);
    let catalog = manager.load_model_catalog();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let model_request = endpoint.recv();
    endpoint.respond(&model_request, model_page());
    wait_value(&catalog).unwrap();

    endpoint.send(form_elicitation(json!(61), "thr_closed", None));
    let (_, responder) = events.elicitation(&AgentServerRequestId::Number(61));
    endpoint.send(json!({
        "method": "thread/closed",
        "params": { "threadId": "thr_closed" }
    }));
    let (thread_id, kind) = events.failed(&AgentServerRequestId::Number(61));
    assert_eq!(thread_id, "thr_closed");
    assert_eq!(kind, AgentServerRequestFailureKind::Cancelled);
    assert!(
        responder
            .respond(AgentMcpElicitationResponse::decline())
            .is_err()
    );
    // A late resolution of an invalidated request stays idempotent.
    endpoint.send(resolved("thr_closed", json!(61)));
    let catalog = manager.load_model_catalog();
    let follow_up = endpoint.recv();
    endpoint.respond(&follow_up, model_page());
    assert!(wait_value(&catalog).is_ok());
    assert!(endpoint.process.is_alive());
    manager.shutdown();
}

#[test]
fn disconnect_invalidates_pending_elicitations_and_retires_the_generation() {
    let (manager, spawner) = manager_with_fake();
    let mut events = ConnectionEvents::new(&manager);
    let catalog = manager.load_model_catalog();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let model_request = endpoint.recv();
    endpoint.respond(&model_request, model_page());
    wait_value(&catalog).unwrap();

    endpoint.send(form_elicitation(json!(71), "thr_drop", None));
    let (_, responder) = events.elicitation(&AgentServerRequestId::Number(71));
    endpoint.close_stdout();
    let (_, kind) = events.failed(&AgentServerRequestId::Number(71));
    assert_eq!(kind, AgentServerRequestFailureKind::Failed);
    assert!(
        responder
            .respond(AgentMcpElicitationResponse::decline())
            .is_err()
    );
    wait_for_process(&endpoint.process);

    // The retired generation's responder never works against a new connection.
    let catalog = manager.load_model_catalog();
    let mut next = spawner.next_endpoint();
    handshake(&mut next);
    let model_request = next.recv();
    next.respond(&model_request, model_page());
    wait_value(&catalog).unwrap();
    assert!(
        responder
            .respond(AgentMcpElicitationResponse::decline())
            .is_err()
    );
    manager.shutdown();
}

#[test]
fn mismatched_resolved_thread_is_a_protocol_error() {
    let (manager, spawner) = manager_with_fake();
    let mut events = ConnectionEvents::new(&manager);
    let catalog = manager.load_model_catalog();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let model_request = endpoint.recv();
    endpoint.respond(&model_request, model_page());
    wait_value(&catalog).unwrap();

    endpoint.send(form_elicitation(json!(81), "thr_owner", None));
    let (_, responder) = events.elicitation(&AgentServerRequestId::Number(81));
    endpoint.send(resolved("thr_other", json!(81)));
    wait_for_process(&endpoint.process);
    assert!(
        responder
            .respond(AgentMcpElicitationResponse::decline())
            .is_err()
    );
    manager.shutdown();
}

/// A turn-scoped approval already owns id 91; an elicitation reusing that id
/// must not inherit or overwrite its responder.
#[test]
fn elicitation_request_id_never_reuses_a_turn_owner() {
    let (manager, spawner) = manager_with_fake();
    let catalog = manager.load_model_catalog();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let model_request = endpoint.recv();
    endpoint.respond(&model_request, model_page());
    wait_value(&catalog).unwrap();

    let run = manager.run_prompt(request("owner", Some("thr_owner")));
    let (turn_events, interrupt) = run.into_parts();
    start_known_turn(&mut endpoint, "thr_owner", "turn_owner");
    endpoint.send(command_approval(json!(91), "thr_owner", "turn_owner"));
    endpoint.send(form_elicitation(
        json!(91),
        "thr_owner",
        Some(json!("turn_owner")),
    ));
    wait_for_process(&endpoint.process);
    let terminal = collect_terminal(&turn_events);
    assert!(
        matches!(terminal.last(), Some(AgentEvent::Failed(_))),
        "{terminal:?}"
    );
    drop(interrupt);
    manager.shutdown();
}
