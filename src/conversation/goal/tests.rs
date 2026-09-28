use super::*;
use crate::agent::{
    AgentConnectionEvent, AgentThreadGoal, AgentThreadGoalRead, AgentThreadGoalStatus,
};
use crate::conversation::ConversationState;

fn goal(status: AgentThreadGoalStatus, updated_at: i64) -> AgentThreadGoal {
    AgentThreadGoal {
        thread_id: "thread".into(),
        objective: "Say ok".into(),
        status,
        token_budget: None,
        tokens_used: 0,
        time_used_seconds: 3,
        created_at: 1,
        updated_at,
    }
}

fn state() -> ConversationGoal {
    let mut state = ConversationGoal::default();
    state.reset(Some("thread".into()));
    state
}

fn read(goal: Option<AgentThreadGoal>) -> AgentThreadGoalRead {
    AgentThreadGoalRead {
        generation: 1,
        thread_id: "thread".into(),
        goal,
    }
}

#[test]
fn notifications_never_roll_a_goal_back() {
    let mut state = state();
    assert!(state.observe(1, goal(AgentThreadGoalStatus::Active, 10)));
    // Duplicate and older snapshots are inert.
    assert!(!state.observe(1, goal(AgentThreadGoalStatus::Active, 10)));
    assert!(!state.observe(1, goal(AgentThreadGoalStatus::Paused, 9)));
    assert_eq!(state.status(), Some(AgentThreadGoalStatus::Active));
    assert!(state.observe(1, goal(AgentThreadGoalStatus::Paused, 11)));
    // Another thread or an older generation cannot touch this one.
    let mut other = goal(AgentThreadGoalStatus::Complete, 99);
    other.thread_id = "other".into();
    assert!(!state.observe(1, other));
    state.generation = Some(2);
    assert!(!state.observe(1, goal(AgentThreadGoalStatus::Complete, 99)));
}

#[test]
fn a_late_read_does_not_overwrite_a_newer_notification() {
    let mut state = state();
    let issued = state.begin(GoalOperation::Set).unwrap();
    assert!(
        state.begin(GoalOperation::Pause).is_none(),
        "one operation at a time"
    );
    assert!(state.observe(1, goal(AgentThreadGoalStatus::Paused, 20)));
    assert!(state.resolve_read(
        Some(issued),
        read(Some(goal(AgentThreadGoalStatus::Active, 15)))
    ));
    assert_eq!(state.status(), Some(AgentThreadGoalStatus::Paused));
    assert!(state.pending.is_none());
    // A newer answer still applies.
    let issued = state.begin(GoalOperation::Resume).unwrap();
    assert!(state.observe(1, goal(AgentThreadGoalStatus::Paused, 21)));
    state.resolve_read(
        Some(issued),
        read(Some(goal(AgentThreadGoalStatus::Active, 22))),
    );
    assert_eq!(state.status(), Some(AgentThreadGoalStatus::Active));
}

#[test]
fn backfill_failure_is_logged_only_and_cleared_state_is_kept() {
    let mut state = state();
    assert!(!state.resolve_backfill(Err("boom".into())));
    assert_eq!(state.error, None);
    assert!(state.resolve_backfill(Ok(read(Some(goal(AgentThreadGoalStatus::Blocked, 5))))));
    assert_eq!(state.generation, Some(1));
    assert!(state.observe_cleared(1, "thread"));
    assert!(state.goal.is_none());
    assert!(!state.observe_cleared(1, "other"));
}

#[test]
fn failed_operations_surface_their_error() {
    let mut state = state();
    let issued = state.begin(GoalOperation::Clear).unwrap();
    state.resolve_clear(issued, Err("no".into()));
    assert_eq!(state.error.as_deref(), Some("no"));
    let issued = state.begin(GoalOperation::Pause).unwrap();
    state.fail(issued, "late".into());
    assert!(state.pending.is_none());
    assert_eq!(state.error.as_deref(), Some("late"));
}

#[test]
fn switching_threads_resets_everything() {
    let mut state = state();
    state.observe(1, goal(AgentThreadGoalStatus::Active, 1));
    state.reset(Some("other".into()));
    assert!(state.goal.is_none() && state.generation.is_none());
}

fn goal_updated(turn_id: Option<&str>, status: AgentThreadGoalStatus) -> AgentConnectionEvent {
    AgentConnectionEvent::ThreadGoalUpdated {
        generation: 1,
        thread_id: "thread".into(),
        turn_id: turn_id.map(str::to_owned),
        goal: goal(status, 5),
    }
}

#[test]
fn the_completing_turn_is_marked_achieved_and_keeps_the_mark_in_the_transcript() {
    let mut conversation = ConversationState {
        thread_id: Some("thread".into()),
        ..Default::default()
    };
    conversation.goal.reset(Some("thread".into()));
    conversation.begin_external_turn(Some("Say ok".into()));
    conversation.turn_id = Some("turn-1".into());
    conversation.apply_connection_event(goal_updated(
        Some("turn-1"),
        AgentThreadGoalStatus::Complete,
    ));
    assert_eq!(conversation.goal_achieved_seconds, Some(3));

    conversation.begin_prompt("next");
    let turn = conversation.transcript.last().expect("committed goal turn");
    assert!(turn.goal.sent_as_goal);
    assert_eq!(turn.goal.achieved_seconds, Some(3));
    assert!(!conversation.user_message_goal);
    assert_eq!(conversation.goal_achieved_seconds, None);
}

#[test]
fn an_active_goal_update_marks_nothing() {
    let mut conversation = ConversationState {
        thread_id: Some("thread".into()),
        ..Default::default()
    };
    conversation.goal.reset(Some("thread".into()));
    conversation.begin_prompt("work");
    conversation.apply_connection_event(goal_updated(None, AgentThreadGoalStatus::Active));
    assert_eq!(conversation.goal_achieved_seconds, None);
}

#[test]
fn a_new_unfinished_goal_withdraws_the_achieved_mark() {
    let mut conversation = ConversationState {
        thread_id: Some("thread".into()),
        ..Default::default()
    };
    conversation.goal.reset(Some("thread".into()));
    conversation.begin_prompt("work");
    conversation.turn_id = Some("turn-1".into());
    conversation.apply_connection_event(goal_updated(
        Some("turn-1"),
        AgentThreadGoalStatus::Complete,
    ));
    conversation.begin_prompt("next");
    assert_eq!(
        conversation
            .transcript
            .last()
            .unwrap()
            .goal
            .achieved_seconds,
        Some(3)
    );
    let mut next = goal(AgentThreadGoalStatus::Active, 9);
    next.objective = "Another goal".into();
    conversation.apply_connection_event(AgentConnectionEvent::ThreadGoalUpdated {
        generation: 1,
        thread_id: "thread".into(),
        turn_id: None,
        goal: next,
    });
    assert!(
        conversation
            .transcript
            .iter()
            .all(|turn| turn.goal.achieved_seconds.is_none())
    );
}
