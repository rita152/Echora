//! Task-board presentation and durable unsent work. No UI or adapter ownership.
use crate::{
    agent::{AgentPermissionMode, AgentPromptContext, AgentThreadActiveFlag, ThreadActivity},
    workspace::WorkspaceSnapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskView {
    List,
    #[default]
    Kanban,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoardMode {
    #[default]
    Attention,
    Classic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Draft,
    InProgress,
    AwaitingYou,
    Done,
}
impl Column {
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::InProgress => "In Progress",
            Self::AwaitingYou => "Awaiting you",
            Self::Done => "Done",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskInput {
    pub text: String,
    pub context: AgentPromptContext,
    pub model: String,
    pub effort: String,
    pub service_tier: Option<String>,
    pub permission: Option<AgentPermissionMode>,
    pub worktree: bool,
    pub base: Option<String>,
    pub include_local_changes: bool,
    pub goal: bool,
}
impl TaskInput {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.context.files.is_empty()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDraft {
    pub id: String,
    pub project_id: String,
    pub cwd: PathBuf,
    pub thread_id: Option<String>,
    #[serde(default)]
    pub client_message_id: Option<String>,
    pub created_at: i64,
    pub input: TaskInput,
}
impl TaskDraft {
    pub fn title(&self) -> String {
        let text = self.input.text.trim();
        if text.is_empty() {
            return "Attached references".into();
        }
        let first = text.lines().next().unwrap_or(text);
        let title: String = first.chars().take(80).collect();
        if first.chars().count() > 80 {
            format!("{title}…")
        } else {
            title
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Todo {
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub priority: Priority,
    #[serde(default)]
    pub completed_at: Option<i64>,
    pub id: String,
    pub title: String,
    pub completed: bool,
    pub thread_id: Option<String>,
    pub created_at: i64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    #[default]
    None,
    Low,
    Medium,
    High,
    Urgent,
}
impl Priority {
    pub const ALL: [Self; 5] = [
        Self::Urgent,
        Self::High,
        Self::Medium,
        Self::Low,
        Self::None,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No priority",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Urgent => "Urgent",
        }
    }
}
/// A server-derived latest-turn observation, refreshed after activity changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskRun {
    pub has_turn: bool,
    pub reply: String,
    pub failed: bool,
    pub last_activity: i64,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskPreferences {
    pub view: TaskView,
    pub mode: BoardMode,
    pub needs_review: bool,
    pub expanded: bool,
    pub drafts: Vec<TaskDraft>,
    pub draft_order: BTreeMap<String, Vec<String>>,
    pub todos: Vec<Todo>,
}

/// Recover the acceptance window after a restart without resending a task.
/// Only a matching server echo acknowledges a dispatch, never an unrelated
/// message or a merely-created thread.
pub fn reconcile_dispatch(
    prefs: &mut TaskPreferences,
    thread: &str,
    turns: &[crate::agent::ThreadTurn],
) -> bool {
    let accepted: Vec<_> = prefs.drafts.iter().filter(|draft| {
        draft.thread_id.as_deref() == Some(thread)
            && draft.client_message_id.as_ref().is_some_and(|client| turns.iter().any(|turn| turn.items.iter().any(|item| matches!(item,
                crate::agent::ThreadHistoryItem::UserMessage { client_message_id: Some(id), .. } if id == client))))
    }).map(|d| d.id.clone()).collect();
    if accepted.is_empty() {
        return false;
    }
    prefs.drafts.retain(|d| !accepted.contains(&d.id));
    for todo in &mut prefs.todos {
        if todo
            .thread_id
            .as_ref()
            .is_some_and(|id| accepted.contains(id))
        {
            todo.thread_id = Some(thread.to_owned());
        }
    }
    for order in prefs.draft_order.values_mut() {
        order.retain(|id| !accepted.iter().any(|draft| id == &format!("draft:{draft}")));
    }
    true
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub id: String,
    pub thread_id: Option<String>,
    pub draft_id: Option<String>,
    pub project_id: String,
    pub title: String,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub timestamp: i64,
    pub column: Column,
    pub attention: Option<&'static str>,
    pub needs_review: bool,
    pub pinned: bool,
}
#[derive(Clone, Debug)]
pub struct ProjectBoard {
    pub id: String,
    pub name: String,
    pub cards: Vec<Card>,
}

pub fn boards(snapshot: &WorkspaceSnapshot, now: i64) -> Vec<ProjectBoard> {
    let prefs = &snapshot.preferences.tasks;
    let mut projects: Vec<ProjectBoard> = snapshot
        .projects
        .iter()
        .map(|p| ProjectBoard {
            id: p.project_id.clone(),
            name: p.name.clone(),
            cards: Vec::new(),
        })
        .collect();
    let mut seen = HashSet::new();
    for thread in snapshot
        .pinned_threads
        .iter()
        .chain(&snapshot.recent_threads)
    {
        if prefs
            .drafts
            .iter()
            .any(|d| d.thread_id.as_deref() == Some(thread.thread_id.as_str()))
        {
            continue;
        }
        if !seen.insert(&thread.thread_id) {
            continue;
        }
        let project_id = crate::workspace::project_id_for_thread(thread, &snapshot.projects);
        let Some(project) = projects
            .iter_mut()
            .find(|p| Some(p.id.as_str()) == project_id.as_deref())
        else {
            continue;
        };
        let review = matches!(snapshot.thread_pull_request_chip(thread), crate::workspace::ChipPullRequest::Show { state, .. } if state.summary.status == crate::pull_requests::PullRequestStatus::Open);
        let (running, mut attention) = match &thread.activity {
            ThreadActivity::SystemError => (false, Some("Failed")),
            ThreadActivity::Active { flags } => (
                true,
                if flags.contains(&AgentThreadActiveFlag::WaitingOnApproval) {
                    Some("Awaiting approval")
                } else if flags.contains(&AgentThreadActiveFlag::WaitingOnUserInput) {
                    Some("Awaiting your input")
                } else {
                    None
                },
            ),
            _ => (false, None),
        };
        let observed = snapshot.task_runs.get(&thread.thread_id);
        let last_activity = observed.map_or(thread.updated_at, |r| {
            r.last_activity.max(thread.updated_at)
        });
        if !running && observed.is_some_and(|r| r.failed) {
            attention = Some("Failed");
        }
        let stuck_age = now.saturating_sub(last_activity);
        let stuck = running && attention.is_none() && stuck_age >= 40 * 60;
        if stuck {
            attention = Some("Stuck");
        }
        let column = if prefs.mode == BoardMode::Attention && attention.is_some() {
            Column::AwaitingYou
        } else if running {
            Column::InProgress
        } else if observed.is_some_and(|r| !r.has_turn && r.error.is_none()) {
            Column::Draft
        } else {
            Column::Done
        };
        if prefs.mode == BoardMode::Attention && prefs.needs_review && !review {
            continue;
        }
        project.cards.push(Card {
            id: format!("thread:{}", thread.thread_id),
            thread_id: Some(thread.thread_id.clone()),
            draft_id: None,
            project_id: project.id.clone(),
            title: thread.title.clone(),
            cwd: thread.cwd.clone(),
            branch: thread.git.branch.clone(),
            timestamp: thread
                .recency_at
                .unwrap_or(thread.updated_at)
                .max(last_activity),
            column,
            attention: attention.or(if observed.is_some_and(|r| r.error.is_some()) {
                Some("Status unavailable")
            } else if running && stuck_age >= 20 * 60 {
                Some("Stuck")
            } else {
                None
            }),
            needs_review: review,
            pinned: snapshot
                .pinned_threads
                .iter()
                .any(|t| t.thread_id == thread.thread_id),
        });
    }
    if !(prefs.mode == BoardMode::Attention && prefs.needs_review) {
        for draft in &prefs.drafts {
            if draft.input.is_empty() {
                continue;
            }
            if let Some(project) = projects.iter_mut().find(|p| p.id == draft.project_id) {
                project.cards.push(Card {
                    id: format!("draft:{}", draft.id),
                    thread_id: draft.thread_id.clone(),
                    draft_id: Some(draft.id.clone()),
                    project_id: draft.project_id.clone(),
                    title: draft.title(),
                    cwd: draft.cwd.clone(),
                    branch: draft.input.base.clone(),
                    timestamp: draft.created_at,
                    column: Column::Draft,
                    attention: None,
                    needs_review: false,
                    pinned: false,
                });
            }
        }
    }
    for project in &mut projects {
        let order = prefs.draft_order.get(&project.id);
        project.cards.sort_by(|a, b| {
            let rank = |card: &Card| match card.column {
                Column::InProgress => 0,
                Column::AwaitingYou => 1,
                Column::Draft => 2,
                Column::Done => 3,
            };
            rank(a).cmp(&rank(b)).then_with(|| {
                if a.column == Column::Draft && b.column == Column::Draft {
                    let pos = |id: &str| {
                        order
                            .and_then(|o| o.iter().position(|k| k == id))
                            .unwrap_or(usize::MAX)
                    };
                    let cmp = pos(&a.id).cmp(&pos(&b.id));
                    if !cmp.is_eq() {
                        return cmp;
                    }
                }
                b.timestamp.cmp(&a.timestamp).then_with(|| a.id.cmp(&b.id))
            })
        });
    }
    projects
}

pub fn reorder(order: &mut Vec<String>, visible: &[String], from: &str, to: &str) -> bool {
    let (Some(a), Some(b)) = (
        visible.iter().position(|s| s == from),
        visible.iter().position(|s| s == to),
    ) else {
        return false;
    };
    if a == b {
        return false;
    }
    let mut changed = visible.to_vec();
    let moved = changed.remove(a);
    changed.insert(b, moved);
    // Replace only visible slots; filtering must not move hidden cards.
    let visible_set: HashSet<_> = visible.iter().collect();
    let mut replacements = changed.into_iter();
    let mut merged = Vec::new();
    let mut used = HashSet::new();
    for id in order.iter() {
        let id = if visible_set.contains(id) {
            replacements.next().unwrap_or_else(|| id.clone())
        } else {
            id.clone()
        };
        if used.insert(id.clone()) {
            merged.push(id);
        }
    }
    merged.extend(replacements.filter(|id| used.insert(id.clone())));
    *order = merged;
    true
}

pub fn relative_time(timestamp: i64, now: i64) -> String {
    let age = now.saturating_sub(timestamp).max(0);
    if age < 60 {
        "now".into()
    } else if age < 3600 {
        format!("{}m", age / 60)
    } else if age < 86400 {
        format!("{}h", age / 3600)
    } else {
        format!("{}d", age / 86400)
    }
}
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "task-{time:x}-{:x}-{:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests;
