//! Custom sidebar sections: every app-server thread section except Pinned.
//!
//! The reference groups chats and projects into sections it names, orders and
//! collapses in its own global state and mirrors to app-server
//! `threadSection/*`. Here the server is the authority for what it can hold:
//! the sections (`threadSection/list|create|update|delete`) and which chats
//! are in them (`thread/section/move`, `thread.section`). What app-server
//! sections cannot hold is kept in the UI preferences: the order of the
//! sections, the projects placed in them and which are collapsed.
//!
//! Sections load only when the backend can also rename and delete them;
//! otherwise the sidebar keeps its Pinned-only form.

use std::sync::{Arc, atomic::Ordering};

use super::{
    PINNED_SECTION_NAME, ThreadCollectionKind, WorkspaceOperation, WorkspaceSnapshot,
    WorkspaceStore, append_error, apply_thread_overlays,
    loaders::{load_all_sections, load_all_threads, receive},
};
use crate::agent::{
    AgentCapability, FilterValue, ProjectId, ThreadId, ThreadListRequest, ThreadSection,
    ThreadSectionId, ThreadSortKey, ThreadSummary,
};

/// One custom section as the sidebar shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomSection {
    pub section: ThreadSection,
    /// Its chats, in the server's section order.
    pub threads: Vec<ThreadSummary>,
    /// Projects placed in it, in the order they were added.
    pub projects: Vec<ProjectId>,
}

/// Something a section holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectionItem {
    Thread(ThreadId),
    Project(ProjectId),
}

impl WorkspaceSnapshot {
    pub fn supports_custom_sections(&self) -> bool {
        [
            AgentCapability::ThreadSectionList,
            AgentCapability::ThreadSectionCreate,
            AgentCapability::ThreadSectionUpdate,
            AgentCapability::ThreadSectionDelete,
            AgentCapability::ThreadSectionMove,
            AgentCapability::ThreadList,
        ]
        .into_iter()
        .all(|capability| self.capabilities.supports(capability))
    }

    pub fn custom_section(&self, section_id: &str) -> Option<&CustomSection> {
        self.custom_sections
            .iter()
            .find(|section| section.section.section_id == section_id)
    }

    /// The custom section a chat is in.
    pub fn custom_section_of_thread(&self, thread_id: &str) -> Option<&ThreadSectionId> {
        self.custom_sections
            .iter()
            .find(|section| {
                section
                    .threads
                    .iter()
                    .any(|thread| thread.thread_id == thread_id)
            })
            .map(|section| &section.section.section_id)
    }

    /// The custom section a project is in.
    pub fn custom_section_of_project(&self, project_id: &str) -> Option<&ThreadSectionId> {
        self.custom_sections
            .iter()
            .find(|section| section.projects.iter().any(|id| id == project_id))
            .map(|section| &section.section.section_id)
    }

    /// The chats "Archive chats" and "Mark all as read" act on: the section's
    /// own chats and the loaded chats of its projects, as the reference
    /// includes a section's project chats.
    pub fn custom_section_thread_ids(&self, section_id: &str) -> Vec<ThreadId> {
        let Some(section) = self.custom_section(section_id) else {
            return Vec::new();
        };
        let mut ids: Vec<ThreadId> = section
            .threads
            .iter()
            .map(|thread| thread.thread_id.clone())
            .collect();
        for thread in &self.recent_threads {
            if thread
                .project_id
                .as_ref()
                .is_some_and(|project| section.projects.contains(project))
                && !ids.contains(&thread.thread_id)
            {
                ids.push(thread.thread_id.clone());
            }
        }
        ids
    }

    /// Rebuilds the sections' project lists from the preferences.
    fn sync_section_projects(&mut self) {
        let projects = self.preferences.section_projects.clone();
        for section in &mut self.custom_sections {
            section.projects = projects
                .get(&section.section.section_id)
                .cloned()
                .unwrap_or_default();
        }
    }
}

/// The server's Pinned section: the one the preferences name, else the first
/// named `Pinned`.
fn is_pinned(section: &ThreadSection, pinned_id: Option<&str>, sections: &[ThreadSection]) -> bool {
    match pinned_id {
        Some(id) if sections.iter().any(|section| section.section_id == id) => {
            section.section_id == id
        }
        _ => sections
            .iter()
            .find(|section| section.name == PINNED_SECTION_NAME)
            .is_some_and(|pinned| pinned.section_id == section.section_id),
    }
}

impl WorkspaceStore {
    /// Lists the sections and each custom section's chats. A read that a
    /// newer one superseded is dropped.
    pub(super) fn refresh_custom_sections(self: &Arc<Self>) {
        if !self.snapshot().supports_custom_sections() {
            return;
        }
        let generation = self.sections_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.update(|snapshot| snapshot.loading.sections = true);
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let pinned_id = store.snapshot().preferences.pinned_section_id;
            let result = load_all_sections(store.backend.as_ref()).and_then(|sections| {
                let customs = sections
                    .iter()
                    .filter(|section| !is_pinned(section, pinned_id.as_deref(), &sections))
                    .cloned()
                    .collect::<Vec<_>>();
                let overlays = store.thread_overlays();
                customs
                    .into_iter()
                    .map(|section| {
                        let mut threads = load_all_threads(
                            store.backend.as_ref(),
                            ThreadListRequest {
                                section: FilterValue::Value(section.section_id.clone()),
                                sort_key: ThreadSortKey::SectionPosition,
                                ..ThreadListRequest::default()
                            },
                        )?;
                        apply_thread_overlays(
                            &mut threads,
                            &overlays,
                            ThreadCollectionKind::Pinned,
                        );
                        Ok(CustomSection {
                            section,
                            threads,
                            projects: Vec::new(),
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            });
            if store.sections_generation.load(Ordering::Acquire) != generation {
                return;
            }
            let mut preferences_changed = false;
            store.update(|snapshot| {
                // Checked again under the snapshot lock: a section created or
                // deleted since the check above bumps the generation in its
                // own update, and this older list must not prune it.
                if store.sections_generation.load(Ordering::Acquire) != generation {
                    return;
                }
                snapshot.loading.sections = false;
                match result {
                    Ok(mut sections) => {
                        let order = &mut snapshot.preferences.section_order;
                        let before = order.clone();
                        order.retain(|id| sections.iter().any(|s| &s.section.section_id == id));
                        for section in &sections {
                            if !order.contains(&section.section.section_id) {
                                order.push(section.section.section_id.clone());
                            }
                        }
                        sections.sort_by_key(|section| {
                            order
                                .iter()
                                .position(|id| id == &section.section.section_id)
                                .unwrap_or(usize::MAX)
                        });
                        let known = |id: &ThreadSectionId| {
                            sections
                                .iter()
                                .any(|section| &section.section.section_id == id)
                        };
                        let projects_before = snapshot.preferences.section_projects.len();
                        snapshot
                            .preferences
                            .section_projects
                            .retain(|id, _| known(id));
                        let collapsed_before = snapshot.preferences.collapsed_section_ids.len();
                        snapshot.preferences.collapsed_section_ids.retain(known);
                        preferences_changed = before != snapshot.preferences.section_order
                            || projects_before != snapshot.preferences.section_projects.len()
                            || collapsed_before != snapshot.preferences.collapsed_section_ids.len();
                        snapshot.custom_sections = sections;
                        snapshot.sync_section_projects();
                    }
                    Err(error) => append_error(
                        &mut snapshot.error,
                        error.user_message(crate::i18n::text("加载会话分区")),
                    ),
                }
            });
            if preferences_changed {
                store.save_preferences();
            }
        });
    }

    /// "New section": creates it (an empty name saves as "New section", as
    /// the reference) and moves `item` into it.
    pub fn create_custom_section(self: &Arc<Self>, name: String, item: Option<SectionItem>) {
        let name = match name.trim() {
            "" => crate::i18n::format!("新分区" => "New section"),
            name => name.to_owned(),
        };
        let operation = WorkspaceOperation::CreateSection(name.clone());
        self.begin(operation.clone());
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let created = receive(
                store.backend.create_thread_section(name, None),
                crate::i18n::format!("创建分区" => "Create section").as_str(),
            );
            match created {
                Ok(section) => {
                    let section_id = section.section_id.clone();
                    store.update(|snapshot| {
                        // A read already in flight predates this section and
                        // would prune it from the preferences.
                        store.sections_generation.fetch_add(1, Ordering::AcqRel);
                        snapshot.preferences.section_order.push(section_id.clone());
                        snapshot.custom_sections.push(CustomSection {
                            section,
                            threads: Vec::new(),
                            projects: Vec::new(),
                        });
                    });
                    store.save_preferences();
                    store.finish(&operation, None);
                    if let Some(item) = item {
                        store.move_to_custom_section(item, Some(section_id));
                    }
                    store.refresh_custom_sections();
                }
                Err(error) => store.finish(
                    &operation,
                    Some(error.user_message(crate::i18n::text("创建分区"))),
                ),
            }
        });
    }

    /// "Edit section": renames it. An empty name is not sent (the dialog
    /// disables Save for it).
    pub fn rename_custom_section(self: &Arc<Self>, section_id: ThreadSectionId, name: String) {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let operation = WorkspaceOperation::RenameSection(section_id.clone());
        self.begin(operation.clone());
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            match receive(
                store
                    .backend
                    .rename_thread_section(section_id.clone(), name),
                crate::i18n::text("重命名分区"),
            ) {
                Ok(section) => {
                    store.update(|snapshot| {
                        if let Some(existing) = snapshot
                            .custom_sections
                            .iter_mut()
                            .find(|existing| existing.section.section_id == section_id)
                        {
                            existing.section = section;
                        }
                    });
                    store.finish(&operation, None);
                }
                Err(error) => store.finish(
                    &operation,
                    Some(error.user_message(crate::i18n::text("重命名分区"))),
                ),
            }
        });
    }

    /// "Remove section": deletes it. Its chats return to their projects or
    /// Recents and its projects to Projects, as the reference preserves them.
    pub fn delete_custom_section(self: &Arc<Self>, section_id: ThreadSectionId) {
        let operation = WorkspaceOperation::DeleteSection(section_id.clone());
        self.begin(operation.clone());
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            match receive(
                store.backend.delete_thread_section(section_id.clone()),
                crate::i18n::text("删除分区"),
            ) {
                Ok(()) => {
                    store.update(|snapshot| {
                        // A read in flight may still list the deleted section.
                        store.sections_generation.fetch_add(1, Ordering::AcqRel);
                        snapshot
                            .custom_sections
                            .retain(|section| section.section.section_id != section_id);
                        snapshot
                            .preferences
                            .section_order
                            .retain(|id| id != &section_id);
                        snapshot.preferences.section_projects.remove(&section_id);
                        snapshot
                            .preferences
                            .collapsed_section_ids
                            .remove(&section_id);
                    });
                    store.save_preferences();
                    store.finish(&operation, None);
                    store.refresh_recent_and_pinned();
                }
                Err(error) => store.finish(
                    &operation,
                    Some(error.user_message(crate::i18n::text("删除分区"))),
                ),
            }
        });
    }

    /// Moves a chat or project into `section_id`, or out of its section with
    /// `None`. A chat moves on the server, which also takes it out of Pinned;
    /// a project moves in the preferences.
    pub fn move_to_custom_section(
        self: &Arc<Self>,
        item: SectionItem,
        section_id: Option<ThreadSectionId>,
    ) {
        match item {
            SectionItem::Project(project_id) => {
                self.update(|snapshot| {
                    for projects in snapshot.preferences.section_projects.values_mut() {
                        projects.retain(|id| id != &project_id);
                    }
                    snapshot
                        .preferences
                        .section_projects
                        .retain(|_, projects| !projects.is_empty());
                    if let Some(section_id) = &section_id {
                        snapshot
                            .preferences
                            .section_projects
                            .entry(section_id.clone())
                            .or_default()
                            .push(project_id.clone());
                    }
                    snapshot.sync_section_projects();
                });
                self.save_preferences();
            }
            SectionItem::Thread(thread_id) => {
                let operation = WorkspaceOperation::MoveThread(thread_id.clone());
                self.begin(operation.clone());
                let store = Arc::clone(self);
                std::thread::spawn(move || {
                    match receive(
                        store
                            .backend
                            .move_thread_to_section(thread_id, section_id, None),
                        crate::i18n::text("移动聊天"),
                    ) {
                        Ok(()) => {
                            store.finish(&operation, None);
                            store.refresh_recent_and_pinned();
                        }
                        Err(error) => store.finish(
                            &operation,
                            Some(error.user_message(crate::i18n::text("移动聊天"))),
                        ),
                    }
                });
            }
        }
    }

    pub fn set_custom_section_collapsed(&self, section_id: &str, collapsed: bool) {
        self.update(|snapshot| {
            let ids = &mut snapshot.preferences.collapsed_section_ids;
            if collapsed {
                ids.insert(section_id.to_owned());
            } else {
                ids.remove(section_id);
            }
        });
        self.save_preferences();
    }

    /// Reorders the sections: `section_id` goes before `before`, or last.
    pub fn move_custom_section(&self, section_id: &str, before: Option<&str>) {
        if before == Some(section_id) {
            return;
        }
        self.update(|snapshot| {
            let order = &mut snapshot.preferences.section_order;
            let Some(index) = order.iter().position(|id| id == section_id) else {
                return;
            };
            let moved = order.remove(index);
            let at = before
                .and_then(|before| order.iter().position(|id| id == before))
                .unwrap_or(order.len());
            order.insert(at, moved);
            let order = order.clone();
            snapshot.custom_sections.sort_by_key(|section| {
                order
                    .iter()
                    .position(|id| id == &section.section.section_id)
                    .unwrap_or(usize::MAX)
            });
        });
        self.save_preferences();
    }

    /// Screenshot fixture: "Research" with the first two chats and the first
    /// project of the capture's workspace, and an empty "Later", as the
    /// reference clone was seeded. Nothing is written to app-server or to the
    /// preferences file; an in-flight section read is dropped.
    #[cfg(feature = "screenshot")]
    pub fn seed_custom_sections_for_capture(self: &Arc<Self>) {
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            loop {
                let snapshot = store.snapshot();
                let loading = snapshot.loading;
                if (!loading.recent && !loading.projects && !loading.sections)
                    || std::time::Instant::now() > deadline
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            store.sections_generation.fetch_add(1, Ordering::AcqRel);
            store.update(|snapshot| {
                let pinned: std::collections::HashSet<_> = snapshot
                    .pinned_threads
                    .iter()
                    .map(|thread| thread.thread_id.clone())
                    .collect();
                let threads = snapshot
                    .recent_threads
                    .iter()
                    .filter(|thread| !pinned.contains(&thread.thread_id))
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>();
                let projects = snapshot
                    .projects
                    .iter()
                    .skip(1)
                    .take(1)
                    .map(|project| project.project_id.clone())
                    .collect::<Vec<_>>();
                let section = |id: &str, name: &str| ThreadSection {
                    section_id: id.into(),
                    name: name.into(),
                    appearance: None,
                };
                snapshot.custom_sections = vec![
                    CustomSection {
                        section: section("capture-research", "Research"),
                        threads,
                        projects: Vec::new(),
                    },
                    CustomSection {
                        section: section("capture-later", "Later"),
                        threads: Vec::new(),
                        projects: Vec::new(),
                    },
                ];
                snapshot.preferences.section_order =
                    vec!["capture-research".into(), "capture-later".into()];
                snapshot.preferences.section_projects = [("capture-research".to_owned(), projects)]
                    .into_iter()
                    .collect();
                snapshot.sync_section_projects();
            });
        });
    }
}
