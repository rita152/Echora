use std::sync::{Arc, Mutex};

use super::*;
use crate::{
    agent::{
        AgentConnectionEvent, AgentEvent, AgentMcpElicitationAction, AgentMcpElicitationControl,
        AgentMcpElicitationField, AgentMcpElicitationFieldKind, AgentMcpElicitationForm,
        AgentMcpElicitationMode, AgentMcpElicitationRequest, AgentMcpElicitationResponse,
        AgentOptionalField, AgentRuntimeEvent, AgentRuntimeObservation, AgentServerRequestId,
    },
    components::mcp_elicitation::McpElicitationStatus,
};

#[derive(Default)]
struct Replies(Mutex<Vec<AgentMcpElicitationResponse>>);

impl AgentMcpElicitationControl for Replies {
    fn respond(
        &self,
        _: &crate::agent::AgentMcpElicitationIdentity,
        response: AgentMcpElicitationResponse,
    ) -> Result<(), String> {
        self.0.lock().unwrap().push(response);
        Ok(())
    }
}

fn form_request(
    generation: u64,
    id: AgentServerRequestId,
    thread_id: &str,
) -> AgentMcpElicitationRequest {
    AgentMcpElicitationRequest {
        generation,
        request_id: id,
        server_name: "fixture-mcp".into(),
        thread_id: thread_id.into(),
        turn_id: AgentOptionalField::Unspecified,
        mode: AgentMcpElicitationMode::Form(AgentMcpElicitationForm {
            message: "请填写部署信息".into(),
            fields: vec![AgentMcpElicitationField {
                name: "name".into(),
                title: Some("名称".into()),
                description: None,
                required: true,
                kind: AgentMcpElicitationFieldKind::String {
                    format: None,
                    min_length: None,
                    max_length: None,
                },
                default: None,
            }],
        }),
    }
}

fn request_event(
    generation: u64,
    id: AgentServerRequestId,
    thread_id: &str,
    control: &Arc<Replies>,
) -> (
    AgentMcpElicitationRequest,
    AgentMcpElicitationHandle,
    AgentConnectionEvent,
) {
    let request = form_request(generation, id.clone(), thread_id);
    let control: Arc<dyn AgentMcpElicitationControl> = control.clone();
    let responder = AgentMcpElicitationHandle::new(request.identity(), control);
    let event = AgentConnectionEvent::McpElicitationRequested {
        request: request.clone(),
        responder: responder.clone(),
    };
    (request, responder, event)
}

fn card(state: &ConversationState) -> &McpElicitationPresentation {
    state
        .activities
        .iter()
        .find_map(|activity| match activity {
            ConversationActivity::McpElicitation(model) => Some(model.as_ref()),
            _ => None,
        })
        .expect("pending elicitation card")
}

fn scoped_state(thread_id: &str) -> ConversationState {
    let mut state = ConversationState::default();
    state.set_workspace_context(
        std::path::PathBuf::from("/tmp/project"),
        None,
        Some(thread_id.to_owned()),
    );
    state
}

#[test]
fn elicitation_stays_thread_scoped_and_never_moves_the_phase() {
    let mut state = scoped_state("thr_a");
    let control = Arc::new(Replies::default());
    let (_, _, event) = request_event(1, AgentServerRequestId::Number(1), "thr_a", &control);
    assert!(state.apply_connection_event(event));
    let phase = state.phase;
    assert_eq!(card(&state).request_id, "generation:1-number:1");
    assert_eq!(state.mcp_elicitation_responders.len(), 1);

    let (_, _, foreign) = request_event(1, AgentServerRequestId::Number(2), "thr_b", &control);
    assert!(!state.apply_connection_event(foreign));
    assert_eq!(state.mcp_elicitation_contexts.len(), 1);
    // An elicitation is not a turn event: the conversation phase is untouched.
    assert_eq!(state.phase, phase);
}

#[test]
fn duplicate_delivery_and_retired_generations_never_register_twice() {
    let mut state = scoped_state("thr_a");
    let control = Arc::new(Replies::default());
    let (_, _, event) = request_event(
        4,
        AgentServerRequestId::String("dup".into()),
        "thr_a",
        &control,
    );
    assert!(state.apply_connection_event(event.clone()));
    // A repeated delivery of the same request changes nothing and must never
    // register a second responder.
    assert!(!state.apply_connection_event(event));
    assert_eq!(state.mcp_elicitation_contexts.len(), 1);
    assert_eq!(state.mcp_elicitation_responders.len(), 1);

    // A newer generation retires old cards; a late request must not come back.
    state.runtime = Default::default();
    let _ = state.runtime.apply(AgentRuntimeEvent {
        generation: 5,
        observation: AgentRuntimeObservation::GenerationStarted,
    });
    let (_, _, stale) = request_event(
        3,
        AgentServerRequestId::String("stale".into()),
        "thr_a",
        &control,
    );
    assert!(!state.apply_connection_event(stale));
    assert_eq!(state.mcp_elicitation_contexts.len(), 1);
}

#[test]
fn resolution_requires_the_matching_identity_and_releases_the_responder() {
    let mut state = scoped_state("thr_a");
    let control = Arc::new(Replies::default());
    let (_, responder, event) =
        request_event(2, AgentServerRequestId::Number(9), "thr_a", &control);
    state.apply_connection_event(event);
    // The Composer writes the response and records the user's explicit action;
    // only the server resolution reports the outcome.
    responder
        .respond(AgentMcpElicitationResponse::decline())
        .unwrap();
    find_mcp_elicitation_mut(&mut state.activities, "generation:2-number:9")
        .expect("pending card")
        .mark_submitted(AgentMcpElicitationAction::Decline);

    // A resolution for a different generation must not release this card.
    let stale = AgentConnectionEvent::McpElicitationResolved {
        identity: crate::agent::AgentMcpElicitationIdentity {
            generation: 1,
            request_id: AgentServerRequestId::Number(9),
        },
        thread_id: "thr_a".into(),
    };
    assert!(!state.apply_connection_event(stale));
    assert_eq!(state.mcp_elicitation_contexts.len(), 1);
    assert!(card(&state).status.is_overlay_visible());

    let resolved = AgentConnectionEvent::McpElicitationResolved {
        identity: crate::agent::AgentMcpElicitationIdentity {
            generation: 2,
            request_id: AgentServerRequestId::Number(9),
        },
        thread_id: "thr_a".into(),
    };
    assert!(state.apply_connection_event(resolved));
    assert!(state.mcp_elicitation_contexts.is_empty());
    assert!(state.mcp_elicitation_responders.is_empty());
    // The user's explicit action is only reported as finished by the server.
    assert_eq!(card(&state).status, McpElicitationStatus::Declined);
    assert_eq!(
        card(&state).last_action,
        Some(AgentMcpElicitationAction::Decline)
    );
}

#[test]
fn failure_marks_the_card_invalid_or_cancelled() {
    for (kind, expected) in [
        (
            AgentServerRequestFailureKind::Cancelled,
            McpElicitationStatus::Cancelled,
        ),
        (
            AgentServerRequestFailureKind::Failed,
            McpElicitationStatus::Invalid,
        ),
    ] {
        let mut state = scoped_state("thr_a");
        let control = Arc::new(Replies::default());
        let (request, responder, event) =
            request_event(1, AgentServerRequestId::Number(3), "thr_a", &control);
        state.apply_connection_event(event);
        let failed = AgentConnectionEvent::McpElicitationFailed {
            identity: request.identity(),
            thread_id: "thr_a".into(),
            kind,
            message: "连接已断开".into(),
        };
        assert!(state.apply_connection_event(failed));
        assert!(state.mcp_elicitation_responders.is_empty());
        let model = card(&state);
        assert_eq!(model.status, expected);
        assert_eq!(model.failure_message.as_deref(), Some("连接已断开"));
        assert!(!model.status.is_overlay_visible());
        assert!(
            responder
                .respond(AgentMcpElicitationResponse::decline())
                .is_ok()
        );
    }
}

#[test]
fn pending_elicitations_outlive_turn_boundaries_but_never_enter_history() {
    let mut state = scoped_state("thr_a");
    state.begin_prompt("第一轮");
    let control = Arc::new(Replies::default());
    let (_, _, event) = request_event(1, AgentServerRequestId::Number(5), "thr_a", &control);
    state.apply_connection_event(event);
    assert_eq!(state.mcp_elicitation_responders.len(), 1);

    // A completed turn must not clear a request the server still waits on.
    state.apply_agent_event_batch(vec![AgentEvent::Completed]);
    assert_eq!(state.mcp_elicitation_responders.len(), 1);
    assert!(card(&state).status.is_overlay_visible());

    state.begin_prompt("第二轮");
    assert_eq!(state.mcp_elicitation_contexts.len(), 1);
    assert_eq!(state.mcp_elicitation_responders.len(), 1);
    let committed: Vec<_> = state
        .transcript
        .iter()
        .flat_map(|turn| turn.activities.iter())
        .filter(|activity| activity.is_mcp_elicitation())
        .collect();
    assert!(
        committed.is_empty(),
        "a connection-owned card is not part of a turn snapshot"
    );
    assert_eq!(card(&state).request_id, "generation:1-number:5");
}

#[test]
fn requested_card_keeps_its_form_contract() {
    let mut state = scoped_state("thr_a");
    let control = Arc::new(Replies::default());
    let (_, _, event) = request_event(1, AgentServerRequestId::Number(6), "thr_a", &control);
    state.apply_connection_event(event);
    let model = card(&state);
    assert_eq!(model.server_name, "fixture-mcp");
    assert_eq!(model.message(), "请填写部署信息");
    assert_eq!(model.fields().len(), 1);
    assert!(model.fields()[0].required);
    assert!(model.is_interactive());
}
