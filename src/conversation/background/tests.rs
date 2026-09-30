use super::*;
use crate::agent::{AgentConnectionEvent, CommandExecutionSource};
use crate::conversation::{ConversationPhase, ConversationTranscriptTurn};

fn command(id: &str, status: CommandExecutionStatus, process: Option<&str>) -> CommandExecution {
    CommandExecution {
        id: id.into(),
        command: format!("sleep 30 # {id}"),
        actions: Vec::new(),
        cwd: "/repo".into(),
        output: "bg-start\r\n".into(),
        terminal_process_id: process.map(str::to_owned),
        status,
        exit_code: None,
        source: CommandExecutionSource::UnifiedExecStartup,
        timed_out: false,
    }
}

fn turn(id: &str, activities: Vec<ConversationActivity>) -> ConversationTranscriptTurn {
    ConversationTranscriptTurn {
        turn_id: Some(id.into()),
        phase: ConversationPhase::Complete,
        user_message: "BGTERM".into(),
        user_images: Vec::new(),
        user_message_time: None,
        assistant_message: "ok".into(),
        assistant_message_time: None,
        activities,
        resumed: None,
        goal: Default::default(),
    }
}

fn conversation() -> ConversationState {
    let mut state = ConversationState {
        thread_id: Some("thread".into()),
        ..Default::default()
    };
    state.transcript.push(turn(
        "turn-1",
        vec![ConversationActivity::Command(command(
            "old",
            CommandExecutionStatus::InProgress,
            Some("11"),
        ))],
    ));
    state.turn_id = Some("turn-2".into());
    state.phase = ConversationPhase::Complete;
    state.activities = vec![
        ConversationActivity::Command(command(
            "done",
            CommandExecutionStatus::Completed,
            Some("12"),
        )),
        ConversationActivity::Command(command(
            "new",
            CommandExecutionStatus::InProgress,
            Some("13"),
        )),
    ];
    state
}

fn ids(state: &ConversationState) -> Vec<String> {
    state
        .background_terminals()
        .into_iter()
        .map(|t| t.item_id)
        .collect()
}

#[test]
fn running_items_outside_the_turn_in_progress_are_background_terminals() {
    let mut state = conversation();
    assert_eq!(ids(&state), ["new", "old"]);
    // While the newest turn runs, its own running command is not listed.
    state.phase = ConversationPhase::Streaming;
    assert_eq!(ids(&state), ["old"]);
    // A command without a process is an ordinary one.
}

#[test]
fn late_output_and_completion_update_the_finished_turn_in_place() {
    let mut state = conversation();
    assert!(
        state.apply_connection_event(AgentConnectionEvent::BackgroundCommandUpdated {
            generation: 1,
            thread_id: "thread".into(),
            turn_id: "turn-1".into(),
            event: AgentEvent::CommandOutputDelta {
                item_id: "old".into(),
                delta: "bg-mid\r\n".into()
            },
        })
    );
    let mut completed = command("old", CommandExecutionStatus::Failed, Some("11"));
    completed.exit_code = Some(-1);
    completed.output = "bg-start\r\nbg-mid\r\n".into();
    assert!(
        state.apply_connection_event(AgentConnectionEvent::BackgroundCommandUpdated {
            generation: 1,
            thread_id: "thread".into(),
            turn_id: "turn-1".into(),
            event: AgentEvent::CommandCompleted(completed),
        })
    );
    let ConversationActivity::Command(old) = &state.transcript[0].activities[0] else {
        panic!("command activity");
    };
    assert_eq!(old.status, CommandExecutionStatus::Failed);
    assert_eq!(old.exit_code, Some(-1));
    assert_eq!(ids(&state), ["new"]);
    // Another thread's update and an unknown item change nothing.
    assert!(
        !state.apply_connection_event(AgentConnectionEvent::BackgroundCommandUpdated {
            generation: 1,
            thread_id: "other".into(),
            turn_id: "turn-1".into(),
            event: AgentEvent::CommandOutputDelta {
                item_id: "new".into(),
                delta: "x".into()
            },
        })
    );
    assert!(!state.apply_background_command_event(
        "turn-9",
        AgentEvent::CommandOutputDelta {
            item_id: "missing".into(),
            delta: "x".into()
        },
    ));
}

#[test]
fn one_clean_at_a_time_and_the_server_decides_the_final_state() {
    let mut state = conversation();
    assert!(state.begin_background_clean(Some("old".into())));
    assert!(!state.begin_background_clean(None));
    state.finish_background_clean(Ok(()));
    assert_eq!(state.background.clean, BackgroundCleanState::Succeeded);
    // Stopping marks the label only; the items still run until the server says so.
    assert_eq!(ids(&state), ["new", "old"]);
    assert!(state.background.stop_requested.contains("old"));
    let mut completed = command("old", CommandExecutionStatus::Failed, Some("11"));
    completed.exit_code = Some(-1);
    assert!(
        state.apply_background_command_event("turn-1", AgentEvent::CommandCompleted(completed))
    );
    assert!(!state.background.stop_requested.contains("old"));
    // A failure is reported once and needs a new click to retry.
    assert!(state.begin_background_clean(None));
    state.finish_background_clean(Err("boom".into()));
    assert!(matches!(
        state.background.clean,
        BackgroundCleanState::Failed { .. }
    ));
    state.acknowledge_background_clean_failure();
    assert_eq!(state.background.clean, BackgroundCleanState::Idle);
}
