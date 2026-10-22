//! Atomic edits of task preferences, using the workspace's serialized writer.
use super::WorkspaceStore;
use crate::tasks::TaskPreferences;
impl WorkspaceStore {
    pub fn update_tasks(&self, edit: impl FnOnce(&mut TaskPreferences)) {
        self.update(|snapshot| edit(&mut snapshot.preferences.tasks));
        self.save_preferences();
    }
}

impl WorkspaceStore {
    pub(super) fn observe_task_event(&self, event: &crate::agent::AgentConnectionEvent) {
        use crate::agent::{
            AgentConnectionEvent as E, AgentLocalClosure as C, AgentRuntimeObservation as O,
        };
        let (id, has_turn, failed) = match event {
            E::ThreadStatusChanged(s) => (
                &s.thread_id,
                matches!(s.state, crate::agent::AgentThreadStatusState::Active { .. }),
                None,
            ),
            E::ThreadTokenUsageUpdated(s) => (&s.thread_id, true, None),
            E::TurnStarted { thread_id, .. } => (thread_id, true, Some(false)),
            E::Runtime(e) => match &e.observation {
                O::TurnClosed {
                    thread_id, reason, ..
                } => (thread_id, true, Some(*reason == C::Failed)),
                _ => return,
            },
            _ => return,
        };
        self.update(|snapshot| {
            let run = snapshot.task_runs.entry(id.clone()).or_default();
            run.has_turn |= has_turn;
            if let Some(failed) = failed {
                run.failed = failed;
            }
            run.last_activity = chrono::Utc::now().timestamp();
        });
    }
    pub fn observe_task_reply(&self, thread_id: String, reply: String) {
        self.update(|snapshot| {
            let r = snapshot.task_runs.entry(thread_id).or_default();
            r.has_turn = true;
            if !reply.is_empty() && r.reply != reply {
                r.last_activity = chrono::Utc::now().timestamp();
                r.reply = reply;
            }
        });
    }
    /// Bounded callers load summaries, never whole transcripts or image payloads.
    pub fn request_task_runs(
        self: &std::sync::Arc<Self>,
        threads: Vec<crate::agent::ThreadSummary>,
    ) {
        let threads: Vec<_> = if let Ok(mut pending) = self.task_run_inflight.lock() {
            threads
                .into_iter()
                .filter(|t| pending.insert(t.thread_id.clone()))
                .collect()
        } else {
            return;
        };
        if threads.is_empty() {
            return;
        }
        let store = self.clone();
        std::thread::spawn(move || {
            for thread in threads {
                let mut cursor = None;
                let mut seen = std::collections::HashSet::new();
                let mut turns = Vec::new();
                let result = loop {
                    match super::receive(
                        store.backend.list_thread_turns(
                            thread.thread_id.clone(),
                            crate::agent::PageRequest { cursor, limit: 100 },
                            crate::agent::HistoryItemDetail::Summary,
                        ),
                        "Read task status",
                    ) {
                        Ok(page) => {
                            turns.extend(page.data);
                            if let Some(next) = page.next_cursor {
                                if !seen.insert(next.clone()) {
                                    break Err("Repeated task history cursor".to_owned());
                                }
                                cursor = Some(next);
                            } else {
                                break Ok(());
                            }
                        }
                        Err(e) => break Err(e.to_string()),
                    }
                };
                if let Ok(mut pending) = store.task_run_inflight.lock() {
                    pending.remove(&thread.thread_id);
                }
                let mut recovered = false;
                store.update(|snapshot| {
                    if !snapshot.thread(&thread.thread_id).is_some_and(|t| {
                        t.updated_at == thread.updated_at && t.activity == thread.activity
                    }) {
                        return;
                    }
                    let previous = snapshot
                        .task_runs
                        .get(&thread.thread_id)
                        .cloned()
                        .unwrap_or_default();
                    if result.is_ok() {
                        recovered = crate::tasks::reconcile_dispatch(
                            &mut snapshot.preferences.tasks,
                            &thread.thread_id,
                            &turns,
                        );
                    }
                    let latest = turns
                        .iter()
                        .enumerate()
                        .max_by_key(|(index, t)| (t.started_at.unwrap_or_default(), *index))
                        .map(|(_, t)| t);
                    snapshot.task_runs.insert(
                        thread.thread_id.clone(),
                        crate::tasks::TaskRun {
                            has_turn: if result.is_ok() {
                                !turns.is_empty()
                            } else {
                                previous.has_turn
                            },
                            reply: latest
                                .and_then(|t| {
                                    t.items.iter().rev().find_map(|i| match i {
                                        crate::agent::ThreadHistoryItem::AssistantMessage {
                                            text,
                                            ..
                                        } => Some(text.clone()),
                                        _ => None,
                                    })
                                })
                                .unwrap_or(previous.reply),
                            failed: if result.is_ok() {
                                latest.is_some_and(|t| {
                                    t.status == crate::agent::HistoryTurnStatus::Failed
                                })
                            } else {
                                previous.failed
                            },
                            last_activity: previous.last_activity.max(
                                latest
                                    .and_then(|t| t.completed_at.or(t.started_at))
                                    .map(|t| if t > 10_000_000_000 { t / 1000 } else { t })
                                    .unwrap_or(thread.updated_at),
                            ),
                            error: result.err(),
                        },
                    );
                });
                if recovered {
                    store.save_preferences();
                }
            }
        });
    }
}
