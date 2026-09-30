use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use async_channel::{Receiver, Sender};

use super::*;
use crate::{
    agent::{
        AgentAttachmentAddOutcome, AgentAttachmentAddRequest, AgentAttachmentAdded,
        AgentAttachmentContent, AgentBackend, AgentCapabilities, AgentConnectionEvent,
        AgentModelCatalog, AgentPermissionProfile, AgentPullRequestAttachment, AgentRequest,
        AgentRun, AgentThreadAttachment, AgentThreadAttachments, ThreadActivity,
    },
    pull_requests::{CiStatus, PullRequestStatus, PullRequestSummary},
};

fn live(status: PullRequestStatus, head: &str, repository: &str) -> PullRequestLiveState {
    PullRequestLiveState {
        summary: PullRequestSummary {
            number: 1,
            title: format!("PR on {head}"),
            repository: repository.to_owned(),
            head_branch: head.to_owned(),
            base_branch: "main".to_owned(),
            additions: 1,
            deletions: 1,
            status,
            age: String::new(),
            author: "me".to_owned(),
            author_avatar_url: None,
            url: String::new(),
            can_merge: false,
            has_conflicts: false,
            ci_status: CiStatus::Passing,
        },
        head_repository: Some(repository.to_owned()),
        merged_at: None,
        closed_at: None,
    }
}

fn attached(number: u64, touched_at: i64) -> ThreadPullRequest {
    let url = format!("https://github.com/openai/codex/pull/{number}");
    ThreadPullRequest {
        identity_key: AgentPullRequestRef::parse(&url).unwrap().identity_key(),
        url,
        root: Some("/repo".into()),
        head_branch: None,
        touched_at,
    }
}

fn state_with(pull_requests: Vec<ThreadPullRequest>) -> ThreadPullRequestsState {
    let mut state = ThreadPullRequestsState::default();
    let mut entry = ThreadAttachmentEntry::loading(1);
    entry.load = AttachmentLoad::Loaded;
    entry.read_at = Some(Instant::now());
    entry.pull_requests = pull_requests;
    state.threads.insert("t".into(), entry);
    state
}

fn ready(
    state: &mut ThreadPullRequestsState,
    pull_request: &ThreadPullRequest,
    live: Option<PullRequestLiveState>,
) {
    state.statuses.insert(
        pull_request.identity_key.clone(),
        PullRequestStatusEntry {
            load: PullRequestStatusLoad::Ready(live.map(Box::new)),
            fetched_at: Instant::now(),
        },
    );
}

const ORIGIN: &str = "https://github.com/openai/codex.git";

#[test]
fn the_chip_fetches_newest_first_and_prefers_the_open_pull_request_of_the_branch() {
    let merged = attached(2, 2_000);
    let open = attached(1, 1_000);
    let mut state = state_with(vec![open.clone(), merged.clone()]);
    // Nothing fetched: the newest is fetched first and nothing shows yet.
    let (chip, fetch) = state.chip_selection("t", Some("feat"), Some(ORIGIN));
    assert_eq!(chip, ChipPullRequest::Pending);
    assert_eq!(fetch, Some(merged.clone()));
    // The newest is merged, so the next one is fetched before deciding.
    ready(
        &mut state,
        &merged,
        Some(live(PullRequestStatus::Merged, "feat", "openai/codex")),
    );
    let (chip, fetch) = state.chip_selection("t", Some("feat"), Some(ORIGIN));
    assert_eq!(chip, ChipPullRequest::Pending);
    assert_eq!(fetch, Some(open.clone()));
    // An open pull request on the thread's branch and repository wins.
    ready(
        &mut state,
        &open,
        Some(live(PullRequestStatus::Open, "feat", "openai/codex")),
    );
    let (chip, fetch) = state.chip_selection("t", Some("feat"), Some(ORIGIN));
    assert!(matches!(chip, ChipPullRequest::Show { url, .. } if url == open.url));
    assert_eq!(fetch, None);
}

#[test]
fn merged_and_closed_pull_requests_still_show_when_nothing_open_matches() {
    let closed = attached(3, 3_000);
    let mut state = state_with(vec![closed.clone()]);
    ready(
        &mut state,
        &closed,
        Some(live(PullRequestStatus::Closed, "feat", "openai/codex")),
    );
    let (chip, _) = state.chip_selection("t", Some("feat"), Some(ORIGIN));
    assert!(
        matches!(chip, ChipPullRequest::Show { state, .. } if state.summary.status == PullRequestStatus::Closed)
    );
    // An open pull request of a fork is not on the thread's repository, but
    // is still preferred over the closed one.
    let fork = attached(4, 1_000);
    let mut state = state_with(vec![closed.clone(), fork.clone()]);
    ready(
        &mut state,
        &closed,
        Some(live(PullRequestStatus::Closed, "feat", "openai/codex")),
    );
    ready(
        &mut state,
        &fork,
        Some(live(PullRequestStatus::Open, "feat", "someone/codex")),
    );
    let (chip, _) = state.chip_selection("t", Some("feat"), Some(ORIGIN));
    assert!(matches!(chip, ChipPullRequest::Show { url, .. } if url == fork.url));
    // A pull request that cannot be found shows nothing.
    let gone = attached(5, 1_000);
    let mut state = state_with(vec![gone.clone()]);
    ready(&mut state, &gone, None);
    assert_eq!(
        state.chip_selection("t", Some("feat"), Some(ORIGIN)).0,
        ChipPullRequest::None
    );
    assert_eq!(
        ThreadPullRequestsState::default()
            .chip_selection("t", None, None)
            .0,
        ChipPullRequest::None
    );
}

#[test]
fn attachments_decide_for_new_threads_and_old_ones_with_a_record() {
    let thread = |created_at: i64| ThreadSummary {
        thread_id: "t".into(),
        title: "t".into(),
        preview: String::new(),
        cwd: "/repo".into(),
        project_id: None,
        section: None,
        created_at,
        updated_at: created_at,
        recency_at: None,
        activity: ThreadActivity::Idle,
        git: Default::default(),
    };
    let mut state = state_with(Vec::new());
    state.backfill_cutoff = Some(2_000_000);
    let none = HashSet::new();
    assert!(state.attachments_authoritative(&thread(2_000), &none));
    assert!(!state.attachments_authoritative(&thread(1_000), &none));
    // Backfill finished here or in ChatGPT: an empty record counts.
    assert!(state.attachments_authoritative(&thread(1_000), &HashSet::from(["t".to_owned()])));
    state.chatgpt_backfill_completed.insert("t".into());
    assert!(state.attachments_authoritative(&thread(1_000), &none));
    // An attached pull request is a record too.
    let mut state = state_with(vec![attached(1, 1)]);
    state.backfill_cutoff = Some(2_000_000);
    assert!(state.attachments_authoritative(&thread(1_000), &none));
}

#[test]
fn records_and_completed_threads_keep_the_reference_limits() {
    let mut records = Vec::new();
    assert!(!mirror_record(&mut records, "empty", &[], false));
    assert!(mirror_record(&mut records, "empty", &[], true));
    for index in 0..ATTACHMENT_RECORD_THREAD_LIMIT + 5 {
        mirror_record(
            &mut records,
            &format!("t{index}"),
            &[attached(1, index as i64)],
            false,
        );
    }
    assert_eq!(records.len(), ATTACHMENT_RECORD_THREAD_LIMIT);
    assert_eq!(
        records.last().unwrap().thread_id,
        format!("t{}", ATTACHMENT_RECORD_THREAD_LIMIT + 4)
    );
    // An existing record is replaced in place, even with nothing left.
    let position = records
        .iter()
        .position(|record| record.thread_id == "t50")
        .unwrap();
    assert!(mirror_record(&mut records, "t50", &[], false));
    assert!(records[position].pull_requests.is_empty());
    assert!(!mirror_record(&mut records, "t50", &[], false));

    let mut completed = Vec::new();
    for index in 0..BACKFILL_COMPLETED_LIMIT + 3 {
        assert!(mark_backfill_completed(
            &mut completed,
            &format!("t{index}")
        ));
    }
    assert!(!mark_backfill_completed(&mut completed, "t10"));
    assert_eq!(completed.len(), BACKFILL_COMPLETED_LIMIT);
    assert_eq!(completed[0], "t3");
}

#[test]
fn remote_repositories_are_read_from_https_and_ssh_urls() {
    assert_eq!(remote_repository(ORIGIN).as_deref(), Some("openai/codex"));
    assert_eq!(
        remote_repository("git@github.com:OpenAI/Codex.git").as_deref(),
        Some("openai/codex")
    );
    assert_eq!(
        remote_repository("https://github.com/rita152/Echora/").as_deref(),
        Some("rita152/echora")
    );
    assert_eq!(remote_repository("not a remote"), None);
}

/// A backend that answers attachment calls from a script and records them.
struct AttachmentBackend {
    events_tx: Sender<AgentConnectionEvent>,
    events_rx: Receiver<AgentConnectionEvent>,
    list: Mutex<Result<Vec<AgentThreadAttachment>, AgentAttachmentError>>,
    add_fails: Mutex<bool>,
    calls: Mutex<Vec<String>>,
}

impl AttachmentBackend {
    fn new(list: Result<Vec<AgentThreadAttachment>, AgentAttachmentError>) -> Arc<Self> {
        let (events_tx, events_rx) = async_channel::unbounded();
        Arc::new(Self {
            events_tx,
            events_rx,
            list: Mutex::new(list),
            add_fails: Mutex::new(false),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

fn answer<T: Send + 'static>(value: T) -> Receiver<T> {
    let (sender, receiver) = async_channel::bounded(1);
    let _ = sender.send_blocking(value);
    receiver
}

impl AgentBackend for AttachmentBackend {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::new([])
    }
    fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
        self.events_rx.clone()
    }
    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
        answer(Err("unused".into()))
    }
    fn load_permission_profiles(
        &self,
        _cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        answer(Err("unused".into()))
    }
    fn update_thread_permissions(
        &self,
        _request: crate::agent::AgentThreadPermissionUpdate,
    ) -> Receiver<Result<crate::agent::AgentThreadPermissionResult, String>> {
        answer(Err("unused".into()))
    }
    fn run_prompt(&self, _request: AgentRequest) -> AgentRun {
        let (_sender, receiver) = async_channel::bounded(1);
        AgentRun::new(receiver, None)
    }
    fn list_thread_attachments(
        &self,
        thread_id: ThreadId,
        force: bool,
    ) -> Receiver<Result<AgentThreadAttachments, AgentAttachmentError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("list:{thread_id}:{force}"));
        answer(
            self.list
                .lock()
                .unwrap()
                .clone()
                .map(|attachments| AgentThreadAttachments {
                    generation: 3,
                    thread_id,
                    attachments,
                }),
        )
    }
    fn add_thread_attachment(
        &self,
        request: AgentAttachmentAddRequest,
    ) -> Receiver<Result<AgentAttachmentAdded, AgentAttachmentError>> {
        self.calls.lock().unwrap().push(format!(
            "add:{}:{}:{}:{}",
            request.generation, request.thread_id, request.identity_key, request.payload
        ));
        if *self.add_fails.lock().unwrap() {
            return answer(Err(AgentAttachmentError::Failed("boom".into())));
        }
        answer(Ok(AgentAttachmentAdded {
            outcome: AgentAttachmentAddOutcome::Created,
            attachment: AgentThreadAttachment {
                id: "x".into(),
                identity_key: request.identity_key,
                created_at: 1,
                content: AgentAttachmentContent::Other {
                    attachment_type: request.attachment_type,
                    payload: request.payload,
                },
            },
        }))
    }
    fn remove_thread_attachment(
        &self,
        request: crate::agent::AgentAttachmentRemoveRequest,
    ) -> Receiver<Result<(), AgentAttachmentError>> {
        self.calls.lock().unwrap().push(format!(
            "remove:{}:{}",
            request.generation, request.identity_key
        ));
        answer(Ok(()))
    }
}

fn server_attachment(number: u64) -> AgentThreadAttachment {
    let url = format!("https://github.com/openai/codex/pull/{number}");
    AgentThreadAttachment {
        id: format!("a{number}"),
        identity_key: AgentPullRequestRef::parse(&url).unwrap().identity_key(),
        created_at: 1_790_700_000 + number as i64,
        content: AgentAttachmentContent::PullRequest(AgentPullRequestAttachment {
            url,
            root: None,
            head_branch: None,
        }),
    }
}

fn preferences_path(label: &str) -> PathBuf {
    static SERIAL: AtomicU64 = AtomicU64::new(1);
    std::env::temp_dir()
        .join(format!(
            "gpui-pull-requests-{label}-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ))
        .join("preferences.json")
}

fn wait(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "condition never held");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn read(store: &Arc<WorkspaceStore>) {
    store.ensure_thread_attachments("t", true);
    wait(|| {
        store
            .snapshot()
            .pull_requests
            .threads
            .get("t")
            .is_some_and(ThreadAttachmentEntry::is_settled)
    });
}

#[test]
fn reads_mirror_pull_requests_and_attaching_sends_the_reference_key() {
    let backend = AttachmentBackend::new(Ok(vec![server_attachment(7)]));
    let store = WorkspaceStore::with_preferences_path(backend.clone(), preferences_path("attach"));
    read(&store);
    let records = store.snapshot().preferences.pull_request_attachment_records;
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].pull_requests[0].touched_at,
        (1_790_700_007) * 1000
    );

    let done = store.attach_pull_request(
        "t".into(),
        "https://GitHub.com/OpenAI/Codex/pull/42/files".into(),
        Some("/repo".into()),
        Some(" feat ".into()),
    );
    // Shown before the server answers.
    assert!(
        store.snapshot().pull_requests.threads["t"]
            .pull_requests
            .iter()
            .any(|pull_request| pull_request.url == "https://github.com/OpenAI/Codex/pull/42")
    );
    assert_eq!(done.recv_blocking().unwrap(), Ok(()));
    assert!(backend.calls().contains(&format!(
        "add:3:t:[\"github.com\",\"openai\",\"codex\",42]:{}",
        serde_json::json!({"url": "https://github.com/OpenAI/Codex/pull/42", "root": "/repo", "headBranch": "feat"})
    )));
    // A pull request attached (or detached) is never backfilled again.
    assert!(
        store
            .snapshot()
            .preferences
            .pull_request_backfill_completed
            .contains(&"t".to_owned())
    );
}

#[test]
fn a_refused_attach_is_rolled_back_and_a_url_that_is_not_a_pull_request_is_ignored() {
    let backend = AttachmentBackend::new(Ok(Vec::new()));
    let store =
        WorkspaceStore::with_preferences_path(backend.clone(), preferences_path("rollback"));
    read(&store);
    *backend.add_fails.lock().unwrap() = true;
    let done = store.attach_pull_request(
        "t".into(),
        "https://github.com/o/r/pull/1".into(),
        None,
        None,
    );
    assert_eq!(done.recv_blocking().unwrap(), Err("boom".into()));
    wait(|| {
        store.snapshot().pull_requests.threads["t"]
            .pull_requests
            .is_empty()
    });
    let ignored = store.attach_pull_request(
        "t".into(),
        "https://github.com/o/r/issues/1".into(),
        None,
        None,
    );
    assert_eq!(ignored.recv_blocking().unwrap(), Ok(()));
    assert_eq!(
        backend
            .calls()
            .iter()
            .filter(|call| call.starts_with("add:"))
            .count(),
        1
    );
}

#[test]
fn without_server_attachments_the_local_records_stand_in() {
    let backend = AttachmentBackend::new(Err(AgentAttachmentError::Unsupported));
    let store =
        WorkspaceStore::with_preferences_path(backend.clone(), preferences_path("unsupported"));
    read(&store);
    assert_eq!(
        store.snapshot().pull_requests.threads["t"].load,
        AttachmentLoad::Unsupported
    );
    let done = store.attach_pull_request(
        "t".into(),
        "https://github.com/o/r/pull/9".into(),
        None,
        Some("feat".into()),
    );
    assert_eq!(done.recv_blocking().unwrap(), Ok(()));
    let records = store.snapshot().preferences.pull_request_attachment_records;
    assert_eq!(
        records[0].pull_requests[0].url.as_deref(),
        Some("https://github.com/o/r/pull/9")
    );
    assert!(!backend.calls().iter().any(|call| call.starts_with("add:")));
    // Removing works on the records too.
    let removed =
        store.detach_pull_request("t".into(), Some("https://github.com/O/R/pull/9".into()));
    assert_eq!(removed.recv_blocking().unwrap(), Ok(()));
    assert!(
        store.snapshot().preferences.pull_request_attachment_records[0]
            .pull_requests
            .is_empty()
    );
}

#[test]
fn detaching_removes_each_matching_attachment_and_a_deleted_notice_ends_backfill() {
    let backend = AttachmentBackend::new(Ok(vec![server_attachment(7), server_attachment(8)]));
    let store = WorkspaceStore::with_preferences_path(backend.clone(), preferences_path("detach"));
    read(&store);
    let done = store.detach_pull_request("t".into(), None);
    assert_eq!(done.recv_blocking().unwrap(), Ok(()));
    let removes: Vec<String> = backend
        .calls()
        .into_iter()
        .filter(|call| call.starts_with("remove:"))
        .collect();
    assert_eq!(removes.len(), 2);
    assert!(removes.iter().all(|call| call.starts_with("remove:3:")));

    let other = AttachmentBackend::new(Ok(Vec::new()));
    let store = WorkspaceStore::with_preferences_path(other.clone(), preferences_path("notice"));
    store.attachment_updated(crate::agent::AgentAttachmentUpdate {
        generation: 1,
        thread_id: "gone".into(),
        attachment_id: "a".into(),
        attachment_type: "pull_request".into(),
        identity_key: "k".into(),
        operation: crate::agent::AgentAttachmentOperation::Deleted,
    });
    assert!(
        store
            .snapshot()
            .preferences
            .pull_request_backfill_completed
            .contains(&"gone".to_owned())
    );
    // Another attachment type does not touch backfill, and the thread was
    // not shown, so nothing is re-read.
    store.attachment_updated(crate::agent::AgentAttachmentUpdate {
        generation: 1,
        thread_id: "other".into(),
        attachment_id: "a".into(),
        attachment_type: "archived_worktree".into(),
        identity_key: "/w".into(),
        operation: crate::agent::AgentAttachmentOperation::Deleted,
    });
    assert!(
        !store
            .snapshot()
            .preferences
            .pull_request_backfill_completed
            .contains(&"other".to_owned())
    );
    assert!(other.calls().is_empty());
    drop(other.events_tx.clone());
}
