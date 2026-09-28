use super::*;
use crate::agent::{AgentQueuedSubmission, AgentThreadQueue};

fn submission(id: &str) -> AgentQueuedSubmission {
    AgentQueuedSubmission {
        id: id.into(),
        client_message_id: format!("client-{id}"),
        text: id.into(),
        attachments: Vec::new(),
    }
}

fn listed(ids: &[&str], generation: u64) -> Result<AgentThreadQueue, String> {
    Ok(AgentThreadQueue {
        generation,
        thread_id: "thread".into(),
        submissions: ids.iter().map(|id| submission(id)).collect(),
    })
}

fn queue() -> ConversationQueue {
    let mut queue = ConversationQueue::default();
    queue.reset(Some("thread".into()));
    queue
}

#[test]
fn changes_during_a_list_trigger_another_list_until_stable() {
    let mut queue = queue();
    assert!(queue.begin_list().is_none(), "nothing to list yet");
    assert!(queue.invalidate(1, "thread"));
    let first = queue.begin_list().unwrap();
    assert!(queue.begin_list().is_none(), "one list in flight");
    assert!(queue.invalidate(1, "thread"));
    assert!(
        queue.resolve_list(first, listed(&["a"], 1)),
        "stale answer asks again"
    );
    let second = queue.begin_list().unwrap();
    assert!(!queue.resolve_list(second, listed(&["a", "b"], 1)));
    assert_eq!(queue.order(), ["a", "b"]);
    assert!(!queue.invalidate(1, "other"));
}

#[test]
fn older_generations_and_other_threads_are_ignored() {
    let mut queue = queue();
    queue.invalidate(2, "thread");
    let revision = queue.begin_list().unwrap();
    queue.resolve_list(revision, listed(&["a"], 2));
    assert!(!queue.invalidate(1, "thread"));
    queue.wanted += 1;
    let revision = queue.begin_list().unwrap();
    queue.resolve_list(revision, listed(&["stale"], 1));
    assert_eq!(queue.order(), ["a"]);
    queue.wanted += 1;
    let revision = queue.begin_list().unwrap();
    queue.resolve_list(revision, Err("boom".into()));
    assert_eq!(queue.list_error.as_deref(), Some("boom"));
    assert_eq!(
        queue.order(),
        ["a"],
        "a failed list keeps the last known queue"
    );
}

#[test]
fn row_operations_refuse_a_second_click_and_survive_a_relist() {
    let mut queue = queue();
    queue.invalidate(1, "thread");
    let revision = queue.begin_list().unwrap();
    queue.resolve_list(revision, listed(&["a", "b"], 1));
    assert!(queue.begin_row("a", QueueRowOperation::Deleting));
    assert!(!queue.begin_row("a", QueueRowOperation::Steering));
    queue.wanted += 1;
    let revision = queue.begin_list().unwrap();
    queue.resolve_list(revision, listed(&["a", "b"], 1));
    assert_eq!(queue.rows[0].operation, Some(QueueRowOperation::Deleting));
    queue.finish_row("a", Some("failed".into()));
    assert_eq!(queue.rows[0].error.as_deref(), Some("failed"));
    assert!(
        queue.begin_row("a", QueueRowOperation::Steering),
        "retry clears the error"
    );
    assert!(queue.rows[0].error.is_none());
}

#[test]
fn moving_a_row_yields_the_complete_order() {
    let mut queue = queue();
    queue.invalidate(1, "thread");
    let revision = queue.begin_list().unwrap();
    queue.resolve_list(revision, listed(&["a", "b", "c"], 1));
    assert_eq!(queue.move_row("c", 0).unwrap(), ["c", "a", "b"]);
    assert!(queue.move_row("c", 0).is_none());
    assert_eq!(queue.move_row("c", 9).unwrap(), ["a", "b", "c"]);
    assert!(queue.move_row("missing", 0).is_none());
}

#[test]
fn added_rows_keep_their_draft_by_client_id() {
    let mut queue = queue();
    let draft = crate::conversation::SubmissionDraft {
        text: "with comments".into(),
        context: Default::default(),
        comments: Vec::new(),
    };
    queue.insert_added(submission("a"), draft.clone(), "prompt".into());
    queue.insert_added(submission("a"), draft.clone(), "prompt".into());
    assert_eq!(queue.rows.len(), 1);
    assert_eq!(queue.drafts["client-a"], (draft, "prompt".into()));
    assert!(queue.remove("a").is_some());
    assert!(queue.is_empty());
}
