use super::*;
use crate::agent::{
    AgentDynamicToolCall, AgentDynamicToolCallContentItem, AgentDynamicToolCallStatus, AgentEvent,
    AgentFunctionCallOutput, AgentFunctionCallOutputBody, AgentReviewMode,
};
use crate::conversation::{ConversationActivity, ConversationPhase, ConversationState};
use serde_json::json;

/// Mirrors what the dispatch layer emits for an `item/started` notification.
fn started(id: &str, status: AgentDynamicToolCallStatus) -> AgentDynamicToolCall {
    AgentDynamicToolCall {
        id: id.into(),
        tool: "exec".into(),
        namespace: Some("functions".into()),
        arguments: json!({"cmd": "pwd"}),
        status,
        success: None,
        content_items: None,
        duration_ms: None,
        completed: false,
    }
}

/// Mirrors what the dispatch layer emits for an `item/completed` notification.
fn completed(id: &str, status: AgentDynamicToolCallStatus) -> AgentDynamicToolCall {
    AgentDynamicToolCall {
        id: id.into(),
        tool: "exec".into(),
        namespace: Some("functions".into()),
        arguments: json!({"cmd": "pwd"}),
        status,
        success: Some(status == AgentDynamicToolCallStatus::Completed),
        content_items: Some(vec![AgentDynamicToolCallContentItem::Text {
            text: "ok".into(),
        }]),
        duration_ms: Some(1535),
        completed: true,
    }
}

fn first_tool_call(state: &ConversationState) -> &AgentDynamicToolCall {
    state
        .activities
        .iter()
        .find_map(|activity| match activity {
            ConversationActivity::DynamicToolCall(call) => Some(call.as_ref()),
            _ => None,
        })
        .expect("expected a dynamic tool call activity")
}

fn only_tool_call(activities: &mut [ConversationActivity]) -> &AgentDynamicToolCall {
    let index = activities
        .iter()
        .position(|activity| matches!(activity, ConversationActivity::DynamicToolCall(_)))
        .expect("expected a dynamic tool call activity");
    match &activities[index] {
        ConversationActivity::DynamicToolCall(call) => call.as_ref(),
        _ => unreachable!(),
    }
}

#[test]
fn dynamic_tool_call_events_upsert_into_one_activity() {
    let mut activities = Vec::new();
    upsert_dynamic_tool_call_activity(
        &mut activities,
        started("dtc_1", AgentDynamicToolCallStatus::InProgress),
    );
    assert_eq!(activities.len(), 1);
    assert!(!only_tool_call(&mut activities).completed);
    assert_eq!(
        only_tool_call(&mut activities).status,
        AgentDynamicToolCallStatus::InProgress
    );

    upsert_dynamic_tool_call_activity(
        &mut activities,
        completed("dtc_1", AgentDynamicToolCallStatus::Completed),
    );
    assert_eq!(activities.len(), 1, "a completed item must not add a row");
    let call = only_tool_call(&mut activities);
    assert!(call.completed);
    assert_eq!(call.status, AgentDynamicToolCallStatus::Completed);
    assert_eq!(call.success, Some(true));
    assert_eq!(call.duration_ms, Some(1535));
    assert_eq!(call.arguments, json!({"cmd": "pwd"}));
}

#[test]
fn repeated_completion_is_idempotent() {
    let mut activities = Vec::new();
    upsert_dynamic_tool_call_activity(
        &mut activities,
        completed("dtc_1", AgentDynamicToolCallStatus::Completed),
    );
    upsert_dynamic_tool_call_activity(
        &mut activities,
        completed("dtc_1", AgentDynamicToolCallStatus::Completed),
    );
    upsert_dynamic_tool_call_activity(
        &mut activities,
        completed("dtc_1", AgentDynamicToolCallStatus::Completed),
    );
    assert_eq!(activities.len(), 1);
    assert!(only_tool_call(&mut activities).completed);
}

#[test]
fn a_late_started_event_never_reopens_a_completed_item() {
    let mut activities = Vec::new();
    upsert_dynamic_tool_call_activity(
        &mut activities,
        completed("dtc_1", AgentDynamicToolCallStatus::Completed),
    );
    upsert_dynamic_tool_call_activity(
        &mut activities,
        started("dtc_1", AgentDynamicToolCallStatus::InProgress),
    );
    assert_eq!(activities.len(), 1);
    let call = only_tool_call(&mut activities);
    assert!(call.completed, "completed must stay settled");
    assert_eq!(
        call.status,
        AgentDynamicToolCallStatus::Completed,
        "a late start must not reset the status"
    );
    assert_eq!(call.arguments, json!({"cmd": "pwd"}));
    assert_eq!(call.duration_ms, Some(1535));
}

#[test]
fn a_failed_completion_stays_failed_after_a_late_start() {
    let mut activities = Vec::new();
    upsert_dynamic_tool_call_activity(
        &mut activities,
        completed("dtc_1", AgentDynamicToolCallStatus::Failed),
    );
    upsert_dynamic_tool_call_activity(
        &mut activities,
        started("dtc_1", AgentDynamicToolCallStatus::InProgress),
    );
    let call = only_tool_call(&mut activities);
    assert_eq!(call.status, AgentDynamicToolCallStatus::Failed);
    assert_eq!(call.success, Some(false));
}

#[test]
fn interleaved_events_route_by_item_id() {
    let mut activities = Vec::new();
    upsert_dynamic_tool_call_activity(
        &mut activities,
        started("dtc_a", AgentDynamicToolCallStatus::InProgress),
    );
    upsert_dynamic_tool_call_activity(
        &mut activities,
        started("dtc_b", AgentDynamicToolCallStatus::InProgress),
    );
    upsert_dynamic_tool_call_activity(
        &mut activities,
        completed("dtc_a", AgentDynamicToolCallStatus::Completed),
    );
    upsert_dynamic_tool_call_activity(
        &mut activities,
        started("dtc_a", AgentDynamicToolCallStatus::InProgress),
    );
    upsert_dynamic_tool_call_activity(
        &mut activities,
        completed("dtc_b", AgentDynamicToolCallStatus::Failed),
    );
    let ids = activities
        .iter()
        .filter_map(|activity| match activity {
            ConversationActivity::DynamicToolCall(call) => Some(call.id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["dtc_a", "dtc_b"]);
    assert_eq!(activities.len(), 2, "each item keeps exactly one activity");
}

#[test]
fn tool_call_state_is_scoped_to_its_own_turn() {
    let mut turn_a = Vec::new();
    let mut turn_b = Vec::new();
    upsert_dynamic_tool_call_activity(
        &mut turn_a,
        started("dtc_a", AgentDynamicToolCallStatus::InProgress),
    );
    upsert_dynamic_tool_call_activity(
        &mut turn_b,
        started("dtc_b", AgentDynamicToolCallStatus::InProgress),
    );
    // A fresh session owns its own registry, so events from one turn cannot
    // reach the other even when their item ids collide.
    upsert_dynamic_tool_call_activity(
        &mut turn_a,
        completed("dtc_a", AgentDynamicToolCallStatus::Failed),
    );
    assert_eq!(turn_a.len(), 1);
    let call_a = match &turn_a[0] {
        ConversationActivity::DynamicToolCall(call) => call,
        other => panic!("unexpected activity {other:?}"),
    };
    assert_eq!(call_a.status, AgentDynamicToolCallStatus::Failed);
    let call_b = match &turn_b[0] {
        ConversationActivity::DynamicToolCall(call) => call,
        other => panic!("unexpected activity {other:?}"),
    };
    assert_eq!(call_b.status, AgentDynamicToolCallStatus::InProgress);
    assert!(!call_b.completed);
}

#[test]
fn reducer_keeps_the_turn_running_until_turn_completed() {
    let mut state = ConversationState {
        phase: ConversationPhase::Starting,
        ..Default::default()
    };
    state.apply_agent_event_batch(vec![AgentEvent::DynamicToolCallUpdated(completed(
        "dtc_1",
        AgentDynamicToolCallStatus::Completed,
    ))]);
    assert_eq!(
        state.phase,
        ConversationPhase::Streaming,
        "a finished item must not end the turn"
    );
    state.apply_agent_event_batch(vec![AgentEvent::Completed]);
    assert_eq!(state.phase, ConversationPhase::Complete);
}

#[test]
fn function_call_output_and_review_mode_stay_out_of_the_activity_stream() {
    let mut state = ConversationState {
        phase: ConversationPhase::Starting,
        ..Default::default()
    };
    state.apply_agent_event_batch(vec![
        AgentEvent::FunctionCallOutputUpdated(AgentFunctionCallOutput {
            id: "fco_1".into(),
            name: "shell".into(),
            namespace: Some("functions".into()),
            output: AgentFunctionCallOutputBody::Text("total 0\n".into()),
            completed: false,
        }),
        AgentEvent::FunctionCallOutputUpdated(AgentFunctionCallOutput {
            id: "fco_1".into(),
            name: "shell".into(),
            namespace: Some("functions".into()),
            output: AgentFunctionCallOutputBody::Text("total 0\n".into()),
            completed: true,
        }),
        AgentEvent::ReviewModeUpdated(AgentReviewMode {
            id: "review_1".into(),
            review: "code".into(),
            entered: true,
            completed: false,
        }),
        AgentEvent::ReviewModeUpdated(AgentReviewMode {
            id: "review_1".into(),
            review: "code".into(),
            entered: false,
            completed: true,
        }),
    ]);
    assert!(
        state.activities.is_empty(),
        "the reference client folds these items into turn activity"
    );
    assert_eq!(state.phase, ConversationPhase::Starting);
}

#[test]
fn dynamic_tool_call_keeps_a_single_activity_through_the_state_reducer() {
    let mut state = ConversationState {
        phase: ConversationPhase::Starting,
        ..Default::default()
    };
    for event in [
        AgentEvent::DynamicToolCallUpdated(started(
            "dtc_1",
            AgentDynamicToolCallStatus::InProgress,
        )),
        AgentEvent::DynamicToolCallUpdated(started(
            "dtc_1",
            AgentDynamicToolCallStatus::InProgress,
        )),
        AgentEvent::DynamicToolCallUpdated(completed(
            "dtc_1",
            AgentDynamicToolCallStatus::Completed,
        )),
    ] {
        state.apply_agent_event_batch(vec![event]);
    }
    let calls = state
        .activities
        .iter()
        .filter(|activity| matches!(activity, ConversationActivity::DynamicToolCall(_)))
        .count();
    assert_eq!(calls, 1, "a repeated item id must not create a second row");
    assert!(first_tool_call(&state).completed);
}
