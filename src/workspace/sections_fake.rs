//! A stateful fake app-server for custom sections, shared by the store and
//! sidebar tests: sections, their chats and the calls made, answered the way
//! the batch-three probe recorded (`artifacts/batch3-baseline-*/sections`).

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_channel::Receiver;

use crate::agent::{
    AgentBackend, AgentCapabilities, AgentCapability, AgentConnectionEvent, AgentModelCatalog,
    AgentPermissionProfile, AgentRequest, AgentRun, Page, PageRequest, Project, ThreadActivity,
    ThreadId, ThreadListRequest, ThreadSection, ThreadSectionAppearance, ThreadSectionId,
    ThreadSummary, WorkspaceError, WorkspaceResult,
};

/// The built-in Pinned section's fixed id on the baseline CLI.
pub(crate) const PINNED_ID: &str = "01984de2-8f74-7c91-a3b2-5c5e937cf318";

#[derive(Default)]
pub(crate) struct SectionsState {
    pub(crate) sections: Vec<ThreadSection>,
    pub(crate) threads: Vec<ThreadSummary>,
    pub(crate) projects: Vec<Project>,
    pub(crate) log: Vec<String>,
    next_id: u64,
    /// The next section write fails with this message (as the server's
    /// -32602 "thread section not found").
    pub(crate) fail_next: Option<String>,
}

pub(crate) struct SectionsBackend {
    pub(crate) state: Mutex<SectionsState>,
    events: Receiver<AgentConnectionEvent>,
    _publish: async_channel::Sender<AgentConnectionEvent>,
}

fn reply<T: Send + 'static>(value: T) -> Receiver<T> {
    let (sender, receiver) = async_channel::bounded(1);
    let _ = sender.send_blocking(value);
    receiver
}

pub(crate) fn section(id: &str, name: &str) -> ThreadSection {
    ThreadSection {
        section_id: id.into(),
        name: name.into(),
        appearance: None,
    }
}

pub(crate) fn thread(
    id: &str,
    project: Option<&str>,
    section: Option<ThreadSection>,
) -> ThreadSummary {
    ThreadSummary {
        thread_id: id.into(),
        title: format!("Chat {id}"),
        preview: String::new(),
        cwd: PathBuf::from("/work"),
        project_id: project.map(str::to_owned),
        section,
        created_at: 1,
        updated_at: 2,
        recency_at: Some(3),
        activity: ThreadActivity::Idle,
    }
}

pub(crate) fn project(id: &str) -> Project {
    Project {
        project_id: id.into(),
        name: format!("Project {id}"),
        roots: vec![PathBuf::from(format!("/work/{id}"))],
        created_at: 1,
        updated_at: 2,
        recency_at: Some(3),
        position: 0,
    }
}

impl SectionsBackend {
    /// Pinned, "Work" (chat `a`) and an empty "Later"; chats `a`, `b` (in
    /// project `p`) and `c`; project `p`.
    pub(crate) fn seeded() -> Arc<Self> {
        let (publish, events) = async_channel::unbounded();
        let work = section("work", "Work");
        let state = SectionsState {
            sections: vec![
                section(PINNED_ID, "Pinned"),
                work.clone(),
                section("later", "Later"),
            ],
            threads: vec![
                thread("a", None, Some(work)),
                thread("b", Some("p"), None),
                thread("c", None, None),
            ],
            projects: vec![project("p")],
            ..SectionsState::default()
        };
        Arc::new(Self {
            state: Mutex::new(state),
            events,
            _publish: publish,
        })
    }

    pub(crate) fn log(&self) -> Vec<String> {
        self.state.lock().unwrap().log.clone()
    }

    fn fail(&self) -> Option<WorkspaceError> {
        self.state
            .lock()
            .unwrap()
            .fail_next
            .take()
            .map(WorkspaceError::backend)
    }
}

impl AgentBackend for SectionsBackend {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::new([
            AgentCapability::ProjectList,
            AgentCapability::ThreadList,
            AgentCapability::ThreadArchive,
            AgentCapability::ThreadSectionList,
            AgentCapability::ThreadSectionCreate,
            AgentCapability::ThreadSectionUpdate,
            AgentCapability::ThreadSectionDelete,
            AgentCapability::ThreadSectionMove,
        ])
    }
    fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
        self.events.clone()
    }
    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
        reply(Err("not used".into()))
    }
    fn load_permission_profiles(
        &self,
        _cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        reply(Err("not used".into()))
    }
    fn update_thread_permissions(
        &self,
        _request: crate::agent::AgentThreadPermissionUpdate,
    ) -> Receiver<Result<crate::agent::AgentThreadPermissionResult, String>> {
        reply(Err("not used".into()))
    }
    fn run_prompt(&self, _request: AgentRequest) -> AgentRun {
        AgentRun::new(async_channel::unbounded().1, None)
    }
    fn list_projects(&self, _page: PageRequest) -> Receiver<WorkspaceResult<Page<Project>>> {
        reply(Ok(Page::single(
            self.state.lock().unwrap().projects.clone(),
        )))
    }
    fn list_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
        let state = self.state.lock().unwrap();
        let threads = if request.archived {
            Vec::new()
        } else {
            state
                .threads
                .iter()
                .filter(|thread| match &request.section {
                    crate::agent::FilterValue::Any => true,
                    crate::agent::FilterValue::None => thread.section.is_none(),
                    crate::agent::FilterValue::Value(id) => thread
                        .section
                        .as_ref()
                        .is_some_and(|section| &section.section_id == id),
                })
                .cloned()
                .collect()
        };
        reply(Ok(Page::single(threads)))
    }
    fn list_thread_sections(
        &self,
        _page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSection>>> {
        let mut state = self.state.lock().unwrap();
        state.log.push("threadSection/list".into());
        reply(Ok(Page::single(state.sections.clone())))
    }
    fn create_thread_section(
        &self,
        name: String,
        _appearance: Option<ThreadSectionAppearance>,
    ) -> Receiver<WorkspaceResult<ThreadSection>> {
        if let Some(error) = self.fail() {
            return reply(Err(error));
        }
        let mut state = self.state.lock().unwrap();
        state.next_id += 1;
        let created = section(&format!("new-{}", state.next_id), &name);
        state.log.push(format!("threadSection/create:{name}"));
        state.sections.push(created.clone());
        reply(Ok(created))
    }
    fn rename_thread_section(
        &self,
        section_id: ThreadSectionId,
        name: String,
    ) -> Receiver<WorkspaceResult<ThreadSection>> {
        if let Some(error) = self.fail() {
            return reply(Err(error));
        }
        let mut state = self.state.lock().unwrap();
        state
            .log
            .push(format!("threadSection/update:{section_id}:{name}"));
        let Some(existing) = state
            .sections
            .iter_mut()
            .find(|section| section.section_id == section_id)
        else {
            return reply(Err(WorkspaceError::backend("thread section not found")));
        };
        existing.name = name;
        let renamed = existing.clone();
        reply(Ok(renamed))
    }
    fn delete_thread_section(&self, section_id: ThreadSectionId) -> Receiver<WorkspaceResult<()>> {
        if let Some(error) = self.fail() {
            return reply(Err(error));
        }
        let mut state = self.state.lock().unwrap();
        state.log.push(format!("threadSection/delete:{section_id}"));
        state
            .sections
            .retain(|section| section.section_id != section_id);
        for thread in &mut state.threads {
            if thread
                .section
                .as_ref()
                .is_some_and(|section| section.section_id == section_id)
            {
                thread.section = None;
            }
        }
        reply(Ok(()))
    }
    fn move_thread_to_section(
        &self,
        thread_id: ThreadId,
        section_id: Option<ThreadSectionId>,
        _before_thread_id: Option<ThreadId>,
    ) -> Receiver<WorkspaceResult<()>> {
        let mut state = self.state.lock().unwrap();
        state
            .log
            .push(format!("thread/section/move:{thread_id}:{section_id:?}"));
        let target = section_id.and_then(|id| {
            state
                .sections
                .iter()
                .find(|section| section.section_id == id)
                .cloned()
        });
        if let Some(thread) = state
            .threads
            .iter_mut()
            .find(|thread| thread.thread_id == thread_id)
        {
            thread.section = target;
        }
        reply(Ok(()))
    }
    fn archive_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        let mut state = self.state.lock().unwrap();
        state.log.push(format!("thread/archive:{thread_id}"));
        state.threads.retain(|thread| thread.thread_id != thread_id);
        reply(Ok(()))
    }
}
