use super::*;
use crate::{
    agent::{
        AgentCapabilities, AgentInputFile, AgentPermissionMode, Project, ThreadGitInfo,
        ThreadSummary,
    },
    workspace::{ThreadPullRequestsState, UiPreferences, WorkspaceLoading},
};
fn snapshot() -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        capabilities: AgentCapabilities::default(),
        projects: vec![Project {
            project_id: "project".into(),
            name: "Project".into(),
            roots: vec!["/tmp/project".into()],
            created_at: 1,
            updated_at: 1,
            recency_at: None,
            position: 0,
        }],
        recent_threads: vec![],
        archived_threads: vec![],
        pinned_threads: vec![],
        custom_sections: vec![],
        search_query: String::new(),
        search_results: vec![],
        loading: WorkspaceLoading::default(),
        error: None,
        preference_error: None,
        pending: Default::default(),
        preferences: UiPreferences::default(),
        pull_requests: ThreadPullRequestsState::default(),
        task_runs: Default::default(),
    }
}
fn thread(id: &str, activity: ThreadActivity) -> ThreadSummary {
    ThreadSummary {
        thread_id: id.into(),
        title: id.into(),
        preview: "prompt".into(),
        cwd: "/tmp/project".into(),
        project_id: Some("project".into()),
        section: None,
        created_at: 1,
        updated_at: 100,
        recency_at: None,
        activity,
        git: ThreadGitInfo::default(),
    }
}
fn draft(id: &str) -> TaskDraft {
    TaskDraft {
        id: id.into(),
        project_id: "project".into(),
        cwd: "/tmp/project".into(),
        thread_id: None,
        client_message_id: None,
        created_at: 10,
        input: TaskInput {
            text: format!("Task {id}"),
            ..Default::default()
        },
    }
}
#[test]
fn attention_and_classic_use_actual_activity_and_turn_history() {
    let mut s = snapshot();
    s.recent_threads = vec![
        thread(
            "waiting",
            ThreadActivity::Active {
                flags: vec![AgentThreadActiveFlag::WaitingOnApproval],
            },
        ),
        thread("failed", ThreadActivity::Idle),
        thread("empty", ThreadActivity::Idle),
        thread("done", ThreadActivity::Idle),
    ];
    s.task_runs.insert(
        "failed".into(),
        TaskRun {
            has_turn: true,
            failed: true,
            ..Default::default()
        },
    );
    s.task_runs.insert("empty".into(), TaskRun::default());
    let cols = |s: &WorkspaceSnapshot| {
        boards(s, 101)
            .remove(0)
            .cards
            .into_iter()
            .map(|c| (c.title, c.column))
            .collect::<BTreeMap<_, _>>()
    };
    let attention = cols(&s);
    assert_eq!(attention["waiting"], Column::AwaitingYou);
    assert_eq!(attention["failed"], Column::AwaitingYou);
    assert_eq!(attention["empty"], Column::Draft);
    assert_eq!(attention["done"], Column::Done);
    s.preferences.tasks.mode = BoardMode::Classic;
    let classic = cols(&s);
    assert_eq!(classic["waiting"], Column::InProgress);
    assert_eq!(classic["failed"], Column::Done);
}
#[test]
fn stuck_warning_at_twenty_minutes_moves_only_after_forty_and_respects_live_activity() {
    let mut s = snapshot();
    s.recent_threads
        .push(thread("live", ThreadActivity::Active { flags: vec![] }));
    let card = |s: &WorkspaceSnapshot, now| boards(s, now).remove(0).cards.remove(0);
    assert_eq!(card(&s, 100 + 20 * 60).column, Column::InProgress);
    assert_eq!(card(&s, 100 + 20 * 60).attention, Some("Stuck"));
    assert_eq!(card(&s, 100 + 40 * 60).column, Column::AwaitingYou);
    s.task_runs.insert(
        "live".into(),
        TaskRun {
            has_turn: true,
            last_activity: 2500,
            ..Default::default()
        },
    );
    assert_eq!(card(&s, 2501).column, Column::InProgress);
    assert_eq!(card(&s, 2501).attention, None);
    assert_eq!(card(&s, 2501).timestamp, 2500);
}
#[test]
fn pinned_threads_and_inflight_saved_drafts_do_not_duplicate_cards() {
    let mut s = snapshot();
    s.recent_threads.push(thread("one", ThreadActivity::Idle));
    s.pinned_threads = s.recent_threads.clone();
    assert_eq!(boards(&s, 101)[0].cards.len(), 1);
    let mut d = draft("saved");
    d.thread_id = Some("one".into());
    s.preferences.tasks.drafts.push(d);
    let cards = boards(&s, 101).remove(0).cards;
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].draft_id.as_deref(), Some("saved"));
}
#[test]
fn needs_review_is_open_pr_filter_and_does_not_treat_unread_as_review() {
    let mut s = snapshot();
    s.recent_threads
        .push(thread("unread", ThreadActivity::Idle));
    s.preferences.unread_thread_ids.insert("unread".into());
    s.preferences.tasks.drafts.push(draft("local"));
    s.preferences.tasks.needs_review = true;
    assert!(boards(&s, 101)[0].cards.is_empty());
    s.preferences.tasks.mode = BoardMode::Classic;
    assert_eq!(boards(&s, 101)[0].cards.len(), 2);
}
#[test]
fn filtered_reordering_preserves_hidden_slots_and_survives_serialization() {
    let mut order = vec!["a".into(), "hidden".into(), "b".into(), "c".into()];
    let visible = vec!["a".into(), "b".into(), "c".into()];
    assert!(reorder(&mut order, &visible, "c", "a"));
    assert_eq!(order, vec!["c", "hidden", "a", "b"]);
    assert!(!reorder(&mut order, &visible, "missing", "a"));
    let mut empty = vec![];
    assert!(reorder(&mut empty, &visible, "a", "c"));
    assert_eq!(empty, vec!["b", "c", "a"]);
    let mut p = TaskPreferences::default();
    p.draft_order.insert("p".into(), order);
    let saved = serde_json::to_string(&p).unwrap();
    assert_eq!(p, serde_json::from_str(&saved).unwrap());
}
#[test]
fn full_draft_and_todo_options_roundtrip_and_old_preferences_load() {
    let mut d = draft("saved");
    d.input.model = "model".into();
    d.input.effort = "high".into();
    d.input.service_tier = Some("fast".into());
    d.input.permission = Some(AgentPermissionMode::Request);
    d.input.worktree = true;
    d.input.base = Some("main".into());
    d.input.include_local_changes = true;
    d.input.goal = true;
    d.input.context.plan_mode = Some(true);
    d.input.context.files.push(AgentInputFile {
        path: "/tmp/picture.png".into(),
        image: true,
    });
    let mut p = TaskPreferences::default();
    p.drafts.push(d);
    p.todos.push(Todo {
        id: "todo".into(),
        title: "title".into(),
        notes: "notes\n中文".into(),
        priority: Priority::Urgent,
        due_date: Some("2026-10-24".into()),
        project_id: Some("project".into()),
        completed: false,
        completed_at: None,
        thread_id: Some("saved".into()),
        created_at: 1,
    });
    assert_eq!(
        p,
        serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap()
    );
    let old: UiPreferences = serde_json::from_str("{\"version\":1}").unwrap();
    assert_eq!(old.tasks, TaskPreferences::default());
}

#[test]
fn failed_status_reads_do_not_turn_completed_threads_into_drafts() {
    let mut s = snapshot();
    s.recent_threads
        .push(thread("unavailable", ThreadActivity::Idle));
    s.task_runs.insert(
        "unavailable".into(),
        TaskRun {
            error: Some("offline".into()),
            ..Default::default()
        },
    );
    let card = boards(&s, 101).remove(0).cards.remove(0);
    assert_eq!(card.column, Column::Done);
    assert_eq!(card.attention, Some("Status unavailable"));
}

#[test]
fn restart_recovery_requires_the_exact_dispatch_echo() {
    let mut p = TaskPreferences::default();
    let mut d = draft("recovery");
    d.thread_id = Some("thread".into());
    d.client_message_id = Some("exact-client-id".into());
    p.drafts.push(d);
    let turn = |client: &str| crate::agent::ThreadTurn {
        turn_id: "turn".into(),
        status: crate::agent::HistoryTurnStatus::Completed,
        items_view: crate::agent::HistoryItemDetail::Summary,
        items: vec![crate::agent::ThreadHistoryItem::UserMessage {
            item_id: "user".into(),
            client_message_id: Some(client.into()),
            text: "same text".into(),
            images: vec![],
        }],
        started_at: None,
        completed_at: None,
        duration_ms: None,
        error: None,
    };
    assert!(!reconcile_dispatch(&mut p, "thread", &[turn("unrelated")]));
    assert!(!reconcile_dispatch(
        &mut p,
        "other-thread",
        &[turn("exact-client-id")]
    ));
    assert_eq!(p.drafts.len(), 1);
    let saved = serde_json::to_string(&p).unwrap();
    let mut restored = serde_json::from_str(&saved).unwrap();
    assert!(reconcile_dispatch(
        &mut restored,
        "thread",
        &[turn("exact-client-id")]
    ));
    assert!(restored.drafts.is_empty());
    assert!(!reconcile_dispatch(
        &mut restored,
        "thread",
        &[turn("exact-client-id")]
    ));
}
