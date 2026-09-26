//! The sidebar activity view: which chats need attention, the Priority list the
//! view keeps while it is open, and the day groups below it.
//!
//! Mirrors ChatGPT 26.917's priority threads model (`sidebarElectron.
//! priorityThreads`, read from `app.asar`). Opening the view snapshots the
//! chats that are waiting, unread or running; they stay in Priority until the
//! user clears read chats or reopens the view, and chats that start needing
//! attention later are appended. The day groups below freeze each chat's
//! recency the first time they show it, so rows do not jump while the view is
//! open. Nothing here depends on GPUI; the sidebar renders what it computes.

use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{Days, Local, NaiveTime, TimeZone};

use super::{ActivityPreferences, WorkspaceSnapshot};
use crate::agent::{AgentThreadActiveFlag, ThreadActivity, ThreadId, ThreadSummary};

/// Rows the view renders before its loader asks for more (the reference's
/// `visibleItemCount` step).
pub const ACTIVITY_PAGE_SIZE: usize = 10;
/// The day groups cover the activation day and the six days before it.
const ACTIVITY_HISTORY_DAYS: u64 = 7;

/// A chat's attention state, ordered the way the reference sorts Priority:
/// awaiting a response, then unread, then running, then idle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Attention {
    Waiting,
    Unread,
    Active,
    Idle,
}

impl Attention {
    /// A pending approval or user-input request wins, then an unread turn,
    /// then a running turn.
    pub fn of(thread: &ThreadSummary, unread: &BTreeSet<ThreadId>) -> Self {
        match &thread.activity {
            ThreadActivity::Active { flags }
                if flags.iter().any(|flag| {
                    matches!(
                        flag,
                        AgentThreadActiveFlag::WaitingOnApproval
                            | AgentThreadActiveFlag::WaitingOnUserInput
                    )
                }) =>
            {
                Self::Waiting
            }
            _ if unread.contains(&thread.thread_id) => Self::Unread,
            ThreadActivity::Active { .. } => Self::Active,
            _ => Self::Idle,
        }
    }

    /// Waiting and unread chats light the bell's attention badge.
    pub fn needs_attention(self) -> bool {
        matches!(self, Self::Waiting | Self::Unread)
    }
}

/// One chat as the activity view sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityCandidate {
    pub thread_id: ThreadId,
    pub attention: Attention,
    /// Recency in epoch milliseconds: the thread's recency, else its update.
    pub recency_ms: i64,
    pub created_ms: i64,
    pub pinned: bool,
}

/// Everything a session reads from the workspace on each update.
#[derive(Clone, Debug)]
pub struct ActivityInputs {
    /// Sidebar chats, newest first, each listed once.
    pub candidates: Vec<ActivityCandidate>,
    pub preferences: ActivityPreferences,
    /// The chat the main area shows, if any.
    pub viewed_thread: Option<ThreadId>,
    /// Local midnight of the current day, in epoch milliseconds.
    pub today_start_ms: i64,
}

impl ActivityInputs {
    pub fn from_snapshot(
        snapshot: &WorkspaceSnapshot,
        viewed_thread: Option<ThreadId>,
        now_ms: i64,
    ) -> Self {
        let unread = &snapshot.preferences.unread_thread_ids;
        let pinned: HashSet<&str> = snapshot
            .pinned_threads
            .iter()
            .map(|thread| thread.thread_id.as_str())
            .collect();
        let mut seen = HashSet::new();
        let mut candidates: Vec<ActivityCandidate> = snapshot
            .pinned_threads
            .iter()
            .chain(&snapshot.recent_threads)
            .filter(|thread| seen.insert(thread.thread_id.clone()))
            .map(|thread| ActivityCandidate {
                thread_id: thread.thread_id.clone(),
                attention: Attention::of(thread, unread),
                recency_ms: thread_recency_ms(thread),
                created_ms: thread.created_at * 1_000,
                pinned: pinned.contains(thread.thread_id.as_str()),
            })
            .collect();
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.recency_ms));
        Self {
            candidates,
            preferences: snapshot.preferences.activity.clone(),
            viewed_thread,
            today_start_ms: local_day_start_ms(now_ms),
        }
    }

    fn candidate(&self, thread_id: &str) -> Option<&ActivityCandidate> {
        self.candidates
            .iter()
            .find(|candidate| candidate.thread_id == thread_id)
    }
}

/// The recency the sidebar orders chats by, in milliseconds.
pub fn thread_recency_ms(thread: &ThreadSummary) -> i64 {
    thread.recency_at.unwrap_or(thread.updated_at) * 1_000
}

/// Local midnight of the day that contains `ms`.
pub fn local_day_start_ms(ms: i64) -> i64 {
    let Some(moment) = Local.timestamp_millis_opt(ms).single() else {
        return ms - ms.rem_euclid(86_400_000);
    };
    local_midnight_ms(moment.date_naive()).unwrap_or(ms)
}

fn local_midnight_ms(date: chrono::NaiveDate) -> Option<i64> {
    Local
        .from_local_datetime(&date.and_time(NaiveTime::MIN))
        .earliest()
        .map(|midnight| midnight.timestamp_millis())
}

/// How a day heading names its day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelativeDay {
    Today,
    Yesterday,
    /// Any other day, printed as its weekday name.
    Weekday,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivitySectionKind {
    Priority,
    Pinned,
    Day {
        start_ms: i64,
        relative: RelativeDay,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivitySection {
    pub kind: ActivitySectionKind,
    pub threads: Vec<ThreadId>,
}

/// What the list renders: its sections, trimmed to the visible row budget,
/// and whether the loader has more to reveal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivityLayout {
    pub sections: Vec<ActivitySection>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DayGroup {
    start_ms: i64,
    relative: RelativeDay,
    threads: Vec<ThreadId>,
}

/// State the view keeps from the moment it opens until it closes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivitySession {
    activated_at_ms: i64,
    /// Priority chats in display order; they stay until cleared.
    priority: Vec<ThreadId>,
    /// Pinned chats in display order, for the optional Pinned section.
    pinned: Vec<ThreadId>,
    /// Recency each day-group row had when the view first showed it.
    frozen_recency: HashMap<ThreadId, i64>,
    visible_count: usize,
    /// Chats started while the view is open, kept in Priority while their
    /// recency is unchanged (the reference's per-thread keep marker).
    kept: HashMap<ThreadId, i64>,
    /// Every chat the session has seen, to tell new chats apart.
    known: HashSet<ThreadId>,
    /// New chats not yet placed in Priority, oldest first.
    fresh: Vec<ThreadId>,
}

impl ActivitySession {
    pub fn activate(inputs: &ActivityInputs, now_ms: i64) -> Self {
        let mut session = Self {
            activated_at_ms: now_ms,
            priority: Vec::new(),
            pinned: Vec::new(),
            frozen_recency: HashMap::new(),
            visible_count: ACTIVITY_PAGE_SIZE,
            kept: HashMap::new(),
            known: inputs
                .candidates
                .iter()
                .map(|candidate| candidate.thread_id.clone())
                .collect(),
            fresh: Vec::new(),
        };
        session.priority = session
            .priority_candidates(inputs)
            .map(|candidate| candidate.thread_id.clone())
            .collect();
        session.pinned = inputs
            .candidates
            .iter()
            .filter(|candidate| candidate.pinned)
            .map(|candidate| candidate.thread_id.clone())
            .collect();
        // The first frame freezes the recency of every chat it groups by day.
        for group in session.day_groups(inputs, &session.priority, &HashMap::new()) {
            for thread_id in group.threads {
                if let Some(candidate) = inputs.candidate(&thread_id) {
                    session
                        .frozen_recency
                        .insert(thread_id, candidate.recency_ms);
                }
            }
        }
        session.refresh(inputs);
        session
    }

    /// Folds a workspace update into the session: existing Priority chats keep
    /// their place (and drop out once archived or deleted), chats that start
    /// needing attention are appended, and a chat started while the view is
    /// open joins Priority while it is the one on screen.
    pub fn refresh(&mut self, inputs: &ActivityInputs) {
        let activated_floor = self.activated_at_ms - self.activated_at_ms.rem_euclid(1_000);
        for candidate in &inputs.candidates {
            if !self.known.contains(&candidate.thread_id)
                && candidate.created_ms >= activated_floor
                && !self.fresh.contains(&candidate.thread_id)
            {
                self.fresh.push(candidate.thread_id.clone());
            }
            self.known.insert(candidate.thread_id.clone());
        }
        let newest_fresh = inputs
            .candidates
            .iter()
            .find(|candidate| self.fresh.contains(&candidate.thread_id))
            .cloned();
        if let Some(fresh) = &newest_fresh
            && fresh.attention == Attention::Idle
            && inputs
                .viewed_thread
                .as_ref()
                .is_none_or(|viewed| *viewed == fresh.thread_id)
        {
            self.kept.insert(fresh.thread_id.clone(), fresh.recency_ms);
        }

        let kept_priority: Vec<ThreadId> = self
            .priority
            .iter()
            .filter(|thread_id| inputs.candidate(thread_id).is_some())
            .cloned()
            .collect();
        let mut frozen = self.frozen_recency.clone();
        for group in self.day_groups(inputs, &kept_priority, &frozen) {
            for thread_id in group.threads {
                if let Some(candidate) = inputs.candidate(&thread_id) {
                    frozen.entry(thread_id).or_insert(candidate.recency_ms);
                }
            }
        }
        let candidates: Vec<ThreadId> = self
            .priority_candidates(inputs)
            .map(|candidate| candidate.thread_id.clone())
            .collect();
        let lead = newest_fresh
            .as_ref()
            .map(|fresh| fresh.thread_id.clone())
            .filter(|thread_id| candidates.contains(thread_id) && !frozen.contains_key(thread_id));
        let priority = unique(lead.into_iter().chain(kept_priority).chain(candidates));

        let preferences = &inputs.preferences;
        let priority_set: HashSet<&ThreadId> = priority.iter().collect();
        let previous_pinned: HashSet<&ThreadId> = self.pinned.iter().collect();
        let newly_pinned = inputs.candidates.iter().filter(|candidate| {
            candidate.pinned
                && (!preferences.show_pinned
                    || !preferences.show_priority
                    || !self.qualifies(candidate)
                    || !priority_set.contains(&candidate.thread_id)
                    || previous_pinned.contains(&candidate.thread_id))
        });
        let pinned = unique(
            self.pinned
                .iter()
                .cloned()
                .chain(newly_pinned.map(|candidate| candidate.thread_id.clone())),
        )
        .into_iter()
        .filter(|thread_id| {
            inputs
                .candidate(thread_id)
                .is_some_and(|candidate| candidate.pinned)
        })
        .collect();

        if let Some(fresh) = &newest_fresh
            && priority.contains(&fresh.thread_id)
        {
            self.fresh.retain(|thread_id| *thread_id != fresh.thread_id);
        }
        self.priority = priority;
        self.pinned = pinned;
        self.frozen_recency = frozen;
    }

    /// `Clear read chats`: Priority keeps only the chats that still need
    /// attention or are still running.
    pub fn clear_read(&mut self, inputs: &ActivityInputs) {
        self.kept.clear();
        self.priority.retain(|thread_id| {
            inputs
                .candidate(thread_id)
                .is_some_and(|candidate| candidate.attention != Attention::Idle)
        });
    }

    /// The loader came into view: reveal the next page of rows.
    pub fn load_more(&mut self) {
        self.visible_count += ACTIVITY_PAGE_SIZE;
    }

    pub fn visible_count(&self) -> usize {
        self.visible_count
    }

    /// Chats the Priority section shows, with their current attention.
    pub fn priority_threads(&self, inputs: &ActivityInputs) -> Vec<(ThreadId, Attention)> {
        let preferences = &inputs.preferences;
        if !preferences.show_priority {
            return Vec::new();
        }
        self.priority
            .iter()
            .filter(|thread_id| !preferences.show_pinned || !self.pinned.contains(thread_id))
            .filter_map(|thread_id| {
                inputs
                    .candidate(thread_id)
                    .map(|candidate| (thread_id.clone(), candidate.attention))
            })
            .collect()
    }

    /// The bell shows its attention badge while any chat waits for the user
    /// or has an unread turn.
    pub fn needs_attention(&self, inputs: &ActivityInputs) -> bool {
        self.priority_candidates(inputs)
            .any(|candidate| candidate.attention.needs_attention())
            || self
                .priority_threads(inputs)
                .iter()
                .any(|(_, attention)| attention.needs_attention())
    }

    pub fn layout(&self, inputs: &ActivityInputs) -> ActivityLayout {
        let preferences = &inputs.preferences;
        let mut sections = Vec::new();
        if preferences.show_priority {
            sections.push(ActivitySection {
                kind: ActivitySectionKind::Priority,
                threads: self
                    .priority_threads(inputs)
                    .into_iter()
                    .map(|(thread_id, _)| thread_id)
                    .collect(),
            });
        }
        if preferences.show_pinned {
            let pinned: Vec<ThreadId> = self
                .pinned
                .iter()
                .filter(|thread_id| {
                    inputs
                        .candidate(thread_id)
                        .is_some_and(|candidate| candidate.pinned)
                })
                .cloned()
                .collect();
            if !pinned.is_empty() {
                sections.push(ActivitySection {
                    kind: ActivitySectionKind::Pinned,
                    threads: pinned,
                });
            }
        }
        sections.extend(
            self.day_groups(inputs, &self.priority, &self.frozen_recency)
                .into_iter()
                .map(|group| ActivitySection {
                    kind: ActivitySectionKind::Day {
                        start_ms: group.start_ms,
                        relative: group.relative,
                    },
                    threads: group.threads,
                }),
        );
        let total: usize = sections.iter().map(|section| section.threads.len()).sum();
        let mut budget = self.visible_count;
        let sections = sections
            .into_iter()
            .filter_map(|mut section| {
                section.threads.truncate(budget);
                budget -= section.threads.len();
                (section.kind == ActivitySectionKind::Priority || !section.threads.is_empty())
                    .then_some(section)
            })
            .collect();
        ActivityLayout {
            sections,
            has_more: total > self.visible_count,
        }
    }

    /// A chat belongs in Priority while it needs attention or runs, or while
    /// it is the chat started (and kept) since the view opened.
    fn qualifies(&self, candidate: &ActivityCandidate) -> bool {
        candidate.attention != Attention::Idle
            || self.kept.get(&candidate.thread_id) == Some(&candidate.recency_ms)
    }

    fn priority_candidates<'a>(
        &'a self,
        inputs: &'a ActivityInputs,
    ) -> impl Iterator<Item = &'a ActivityCandidate> + 'a {
        let mut candidates: Vec<&ActivityCandidate> = inputs
            .candidates
            .iter()
            .filter(|candidate| self.qualifies(candidate))
            .collect();
        candidates.sort_by_key(|candidate| {
            (candidate.attention, std::cmp::Reverse(candidate.recency_ms))
        });
        candidates.into_iter()
    }

    /// Idle chats from the last seven days, newest first, one group per local
    /// day. With Priority shown, its chats are left out and each row keeps the
    /// recency it had when the view first showed it.
    fn day_groups(
        &self,
        inputs: &ActivityInputs,
        priority: &[ThreadId],
        frozen: &HashMap<ThreadId, i64>,
    ) -> Vec<DayGroup> {
        let preferences = &inputs.preferences;
        let show_priority = preferences.show_priority;
        let cutoff = self.history_cutoff_ms();
        let in_priority: HashSet<&ThreadId> = priority.iter().collect();
        let recency = |candidate: &ActivityCandidate| {
            if show_priority {
                frozen
                    .get(&candidate.thread_id)
                    .copied()
                    .unwrap_or(candidate.recency_ms)
            } else {
                candidate.recency_ms
            }
        };
        let mut rows: Vec<(&ActivityCandidate, i64)> = inputs
            .candidates
            .iter()
            .filter(|candidate| {
                let listed = in_priority.contains(&candidate.thread_id);
                (!show_priority
                    || candidate.attention == Attention::Idle
                    || frozen.contains_key(&candidate.thread_id))
                    && (recency(candidate) >= cutoff || (!show_priority && listed))
                    && (!show_priority || !listed)
                    && (!preferences.show_pinned || !candidate.pinned)
            })
            .map(|candidate| (candidate, recency(candidate)))
            .collect();
        rows.sort_by_key(|(_, recency)| std::cmp::Reverse(*recency));
        let today = inputs.today_start_ms;
        let yesterday = previous_day_start_ms(today);
        let mut groups: Vec<DayGroup> = Vec::new();
        for (candidate, recency) in rows {
            let start_ms = local_day_start_ms(recency);
            match groups.last_mut() {
                Some(group) if group.start_ms == start_ms => {
                    group.threads.push(candidate.thread_id.clone())
                }
                _ => groups.push(DayGroup {
                    start_ms,
                    relative: if start_ms == today {
                        RelativeDay::Today
                    } else if start_ms == yesterday {
                        RelativeDay::Yesterday
                    } else {
                        RelativeDay::Weekday
                    },
                    threads: vec![candidate.thread_id.clone()],
                }),
            }
        }
        groups
    }

    /// Local midnight six days before the activation day.
    fn history_cutoff_ms(&self) -> i64 {
        Local
            .timestamp_millis_opt(self.activated_at_ms)
            .single()
            .and_then(|moment| {
                moment
                    .date_naive()
                    .checked_sub_days(Days::new(ACTIVITY_HISTORY_DAYS - 1))
            })
            .and_then(local_midnight_ms)
            .unwrap_or(self.activated_at_ms - 6 * 86_400_000)
    }
}

fn previous_day_start_ms(day_start_ms: i64) -> i64 {
    Local
        .timestamp_millis_opt(day_start_ms)
        .single()
        .and_then(|moment| moment.date_naive().checked_sub_days(Days::new(1)))
        .and_then(local_midnight_ms)
        .unwrap_or(day_start_ms - 86_400_000)
}

fn unique(threads: impl IntoIterator<Item = ThreadId>) -> Vec<ThreadId> {
    let mut seen = HashSet::new();
    threads
        .into_iter()
        .filter(|thread_id| seen.insert(thread_id.clone()))
        .collect()
}

#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;
