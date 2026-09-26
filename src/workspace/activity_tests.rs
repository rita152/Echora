use std::collections::BTreeSet;

use chrono::{Local, TimeZone};

use super::*;
use crate::agent::{AgentThreadActiveFlag, ThreadActivity, ThreadSummary};

fn at(day: u32, hour: u32) -> i64 {
    Local
        .with_ymd_and_hms(2026, 9, day, hour, 0, 0)
        .single()
        .expect("unambiguous local time")
        .timestamp_millis()
}

fn candidate(id: &str, attention: Attention, recency_ms: i64) -> ActivityCandidate {
    ActivityCandidate {
        thread_id: id.to_owned(),
        attention,
        recency_ms,
        created_ms: recency_ms - 60_000,
        pinned: false,
    }
}

fn inputs(candidates: Vec<ActivityCandidate>, now_ms: i64) -> ActivityInputs {
    let mut candidates = candidates;
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.recency_ms));
    ActivityInputs {
        candidates,
        preferences: ActivityPreferences::default(),
        viewed_thread: None,
        today_start_ms: local_day_start_ms(now_ms),
    }
}

fn section_threads(layout: &ActivityLayout) -> Vec<(String, Vec<&str>)> {
    layout
        .sections
        .iter()
        .map(|section| {
            let name = match &section.kind {
                ActivitySectionKind::Priority => "priority".to_owned(),
                ActivitySectionKind::Pinned => "pinned".to_owned(),
                ActivitySectionKind::Day { relative, start_ms } => match relative {
                    RelativeDay::Today => "today".to_owned(),
                    RelativeDay::Yesterday => "yesterday".to_owned(),
                    RelativeDay::Weekday => Local
                        .timestamp_millis_opt(*start_ms)
                        .unwrap()
                        .format("%a")
                        .to_string(),
                },
            };
            (name, section.threads.iter().map(String::as_str).collect())
        })
        .collect()
}

fn thread(id: &str, activity: ThreadActivity) -> ThreadSummary {
    ThreadSummary {
        thread_id: id.to_owned(),
        title: id.to_owned(),
        preview: String::new(),
        cwd: Default::default(),
        project_id: None,
        section: None,
        created_at: 0,
        updated_at: 0,
        recency_at: None,
        activity,
    }
}

#[test]
fn attention_prefers_pending_requests_then_unread_then_running() {
    let unread: BTreeSet<String> = ["unread".to_owned(), "waiting".to_owned()].into();
    let waiting = thread(
        "waiting",
        ThreadActivity::Active {
            flags: vec![AgentThreadActiveFlag::WaitingOnApproval],
        },
    );
    let input = thread(
        "input",
        ThreadActivity::Active {
            flags: vec![AgentThreadActiveFlag::WaitingOnUserInput],
        },
    );
    let running = thread("running", ThreadActivity::Active { flags: Vec::new() });
    let mut unread_running = running.clone();
    unread_running.thread_id = "unread".to_owned();
    let idle = thread("idle", ThreadActivity::Idle);
    let failed = thread("failed", ThreadActivity::SystemError);
    assert_eq!(Attention::of(&waiting, &unread), Attention::Waiting);
    assert_eq!(Attention::of(&input, &unread), Attention::Waiting);
    assert_eq!(Attention::of(&unread_running, &unread), Attention::Unread);
    assert_eq!(Attention::of(&running, &unread), Attention::Active);
    assert_eq!(Attention::of(&idle, &unread), Attention::Idle);
    assert_eq!(Attention::of(&failed, &unread), Attention::Idle);
    assert!(Attention::Waiting < Attention::Unread && Attention::Unread < Attention::Active);
    assert!(Attention::Unread.needs_attention() && !Attention::Active.needs_attention());
}

#[test]
fn opening_snapshots_priority_and_groups_the_last_seven_days_by_day() {
    let now = at(25, 23);
    let inputs = inputs(
        vec![
            candidate("running", Attention::Active, at(25, 9)),
            candidate("waiting", Attention::Waiting, at(20, 9)),
            candidate("unread", Attention::Unread, at(24, 9)),
            candidate("today", Attention::Idle, at(25, 8)),
            candidate("yesterday-late", Attention::Idle, at(24, 20)),
            candidate("yesterday-early", Attention::Idle, at(24, 7)),
            candidate("wednesday", Attention::Idle, at(23, 7)),
            candidate("oldest-kept", Attention::Idle, at(19, 1)),
            candidate("too-old", Attention::Idle, at(18, 23)),
        ],
        now,
    );
    let session = ActivitySession::activate(&inputs, now);
    assert_eq!(
        section_threads(&session.layout(&inputs)),
        vec![
            ("priority".to_owned(), vec!["waiting", "unread", "running"]),
            ("today".to_owned(), vec!["today"]),
            (
                "yesterday".to_owned(),
                vec!["yesterday-late", "yesterday-early"]
            ),
            ("Wed".to_owned(), vec!["wednesday"]),
            ("Sat".to_owned(), vec!["oldest-kept"]),
        ]
    );
    assert!(!session.layout(&inputs).has_more);
    assert!(session.needs_attention(&inputs));
}

#[test]
fn priority_keeps_read_chats_until_they_are_cleared() {
    let now = at(25, 12);
    let mut current = inputs(
        vec![
            candidate("a", Attention::Unread, at(25, 10)),
            candidate("b", Attention::Active, at(25, 11)),
            candidate("c", Attention::Idle, at(25, 9)),
        ],
        now,
    );
    let mut session = ActivitySession::activate(&current, now);
    assert_eq!(
        session.priority_threads(&current),
        vec![
            ("a".to_owned(), Attention::Unread),
            ("b".to_owned(), Attention::Active)
        ]
    );

    // `a` is read and `b` finishes; both stay listed, now idle, and a chat
    // that starts running afterwards is appended.
    current.candidates = vec![
        candidate("d", Attention::Active, at(25, 12)),
        candidate("b", Attention::Idle, at(25, 11)),
        candidate("a", Attention::Idle, at(25, 10)),
        candidate("c", Attention::Idle, at(25, 9)),
    ];
    session.refresh(&current);
    assert_eq!(
        session.priority_threads(&current),
        vec![
            ("a".to_owned(), Attention::Idle),
            ("b".to_owned(), Attention::Idle),
            ("d".to_owned(), Attention::Active)
        ]
    );
    assert_eq!(
        section_threads(&session.layout(&current))[1],
        ("today".to_owned(), vec!["c"])
    );

    session.clear_read(&current);
    assert_eq!(
        section_threads(&session.layout(&current)),
        vec![
            ("priority".to_owned(), vec!["d"]),
            ("today".to_owned(), vec!["b", "a", "c"]),
        ]
    );
}

#[test]
fn archived_chats_leave_priority_and_rows_keep_their_first_recency() {
    let now = at(25, 12);
    let mut current = inputs(
        vec![
            candidate("gone", Attention::Unread, at(25, 11)),
            candidate("old", Attention::Idle, at(23, 9)),
            candidate("new", Attention::Idle, at(25, 10)),
        ],
        now,
    );
    let mut session = ActivitySession::activate(&current, now);
    // `old` moves to today while the view is open (another client used it),
    // and `gone` is archived.
    current.candidates = vec![
        candidate("old", Attention::Idle, at(25, 12)),
        candidate("new", Attention::Idle, at(25, 10)),
    ];
    session.refresh(&current);
    assert_eq!(
        section_threads(&session.layout(&current)),
        vec![
            ("priority".to_owned(), vec![]),
            ("today".to_owned(), vec!["new"]),
            ("Wed".to_owned(), vec!["old"]),
        ]
    );
}

#[test]
fn rows_are_revealed_one_page_at_a_time() {
    let now = at(25, 23);
    let rows: Vec<ActivityCandidate> = (0..23)
        .map(|index| {
            candidate(
                &format!("t{index:02}"),
                Attention::Idle,
                at(25, 22) - index as i64 * 3_600_000,
            )
        })
        .collect();
    let current = inputs(rows, now);
    let mut session = ActivitySession::activate(&current, now);
    let visible = |session: &ActivitySession| {
        session
            .layout(&current)
            .sections
            .iter()
            .map(|section| section.threads.len())
            .sum::<usize>()
    };
    let layout = session.layout(&current);
    assert_eq!(visible(&session), ACTIVITY_PAGE_SIZE);
    assert!(layout.has_more);
    // The empty Priority section is still rendered, as its empty state.
    assert_eq!(layout.sections[0].kind, ActivitySectionKind::Priority);
    session.load_more();
    assert_eq!(visible(&session), 20);
    session.load_more();
    assert_eq!(visible(&session), 23);
    assert!(!session.layout(&current).has_more);
}

#[test]
fn hiding_priority_lists_every_chat_by_recency() {
    let now = at(25, 12);
    let mut current = inputs(
        vec![
            candidate("running", Attention::Active, at(25, 11)),
            candidate("idle", Attention::Idle, at(25, 10)),
        ],
        now,
    );
    current.preferences.show_priority = false;
    let session = ActivitySession::activate(&current, now);
    assert_eq!(
        section_threads(&session.layout(&current)),
        vec![("today".to_owned(), vec!["running", "idle"])]
    );
    assert!(session.priority_threads(&current).is_empty());
}

#[test]
fn the_pinned_section_takes_pinned_chats_out_of_the_other_sections() {
    let now = at(25, 12);
    let mut pinned_running = candidate("pinned-running", Attention::Active, at(25, 11));
    pinned_running.pinned = true;
    let mut pinned_idle = candidate("pinned-idle", Attention::Idle, at(25, 10));
    pinned_idle.pinned = true;
    let mut current = inputs(
        vec![
            pinned_running,
            pinned_idle,
            candidate("idle", Attention::Idle, at(25, 9)),
        ],
        now,
    );
    current.preferences.show_pinned = true;
    let session = ActivitySession::activate(&current, now);
    assert_eq!(
        section_threads(&session.layout(&current)),
        vec![
            ("priority".to_owned(), vec![]),
            ("pinned".to_owned(), vec!["pinned-running", "pinned-idle"]),
            ("today".to_owned(), vec!["idle"]),
        ]
    );
}

#[test]
fn a_chat_started_while_the_view_is_open_leads_priority() {
    let now = at(25, 12);
    let mut current = inputs(
        vec![
            candidate("running", Attention::Active, at(25, 11)),
            candidate("idle", Attention::Idle, at(25, 10)),
        ],
        now,
    );
    let mut session = ActivitySession::activate(&current, now);
    // A new chat starts with its first turn running.
    let mut started = candidate("started", Attention::Active, now + 5_000);
    started.created_ms = now + 1_000;
    current.candidates.insert(0, started.clone());
    current.viewed_thread = Some("started".to_owned());
    session.refresh(&current);
    let listed = |session: &ActivitySession, current: &ActivityInputs| {
        session
            .priority_threads(current)
            .into_iter()
            .map(|(thread_id, _)| thread_id)
            .collect::<Vec<_>>()
    };
    assert_eq!(listed(&session, &current), vec!["started", "running"]);
    // The turn finishes while the chat is on screen: it stays, read, and is
    // kept even through a clear until its recency moves... unless cleared.
    started.attention = Attention::Idle;
    current.candidates[0] = started;
    session.refresh(&current);
    assert_eq!(listed(&session, &current), vec!["started", "running"]);
    session.clear_read(&current);
    session.refresh(&current);
    assert_eq!(listed(&session, &current), vec!["running"]);
    assert_eq!(
        section_threads(&session.layout(&current))[1],
        ("today".to_owned(), vec!["started", "idle"])
    );
}
