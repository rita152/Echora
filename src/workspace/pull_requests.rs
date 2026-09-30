//! Pull requests attached to threads, for the sidebar chip, the row hover card
//! and the thread summary panel.
//!
//! The server's `pull_request` attachments are the source of truth; the
//! newest (largest touchedAt) is the thread's current pull request. When the
//! server has no attachments (an older CLI answers method-not-found) the local
//! mirror in the UI preferences stands in, as the reference's
//! `pull-request-attachment-records-v3` does. GitHub state comes from the
//! authenticated `gh` CLI, in the background, deduplicated and cached.
//!
//! Threads created before the backfill cutoff that have no attachment yet are
//! backfilled once per session from the pull request of their branch, exactly
//! like the reference; a thread whose pull request the user removed (in either
//! application) is never backfilled again.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

use async_channel::Receiver;

use super::{
    WorkspaceSnapshot, WorkspaceStore,
    preferences::{
        ATTACHMENT_RECORD_PER_THREAD_LIMIT, ATTACHMENT_RECORD_THREAD_LIMIT,
        BACKFILL_COMPLETED_LIMIT, PullRequestAttachmentRecord, RecordedPullRequest,
    },
};
use crate::{
    agent::{
        AgentAttachmentAddRequest, AgentAttachmentError, AgentAttachmentOperation,
        AgentAttachmentRemoveRequest, AgentAttachmentUpdate, AgentOptionalField,
        AgentPullRequestRef, AgentThreadAttachment, AgentWorktreeAttachment,
        PULL_REQUEST_ATTACHMENT_TYPE, ThreadId, ThreadMetadataUpdate, ThreadSummary,
    },
    pull_requests::{BranchPullRequest, PullRequestLiveState, PullRequestStatus, StatusIcon},
};

/// Reads answer from memory for this long, like the reference's staleTime.
const ATTACHMENT_STALE_AFTER: Duration = Duration::from_secs(60);
/// Open pull requests are re-read this often (the reference polls at least
/// once a minute while a pull request is open, every 15 s while checks run).
const OPEN_STATUS_STALE_AFTER: Duration = Duration::from_secs(60);
const PENDING_STATUS_STALE_AFTER: Duration = Duration::from_secs(15);
const BRANCH_LOOKUP_STALE_AFTER: Duration = Duration::from_secs(60);
const GIT_TIMEOUT: Duration = Duration::from_secs(10);

/// One pull request attached to a thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadPullRequest {
    pub url: String,
    pub identity_key: String,
    pub root: Option<String>,
    pub head_branch: Option<String>,
    /// Epoch milliseconds (the attachment's createdAt × 1000).
    pub touched_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachmentLoad {
    Loading,
    Loaded,
    /// The server has no attachments; the local records are shown.
    Unsupported,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadAttachmentEntry {
    pub load: AttachmentLoad,
    /// The generation the attachments were read on; writes name it.
    pub generation: Option<u64>,
    pub pull_requests: Vec<ThreadPullRequest>,
    pub worktrees: Vec<AgentWorktreeAttachment>,
    read_at: Option<Instant>,
    epoch: u64,
}

impl ThreadAttachmentEntry {
    fn loading(epoch: u64) -> Self {
        Self {
            load: AttachmentLoad::Loading,
            generation: None,
            pull_requests: Vec::new(),
            worktrees: Vec::new(),
            read_at: None,
            epoch,
        }
    }

    /// Whether the attachments have been read (the reference's defined
    /// query data), so an empty list means "none".
    pub fn is_settled(&self) -> bool {
        matches!(
            self.load,
            AttachmentLoad::Loaded | AttachmentLoad::Unsupported
        ) || self.read_at.is_some()
    }

    /// Newest first, the order every consumer uses.
    pub fn pull_requests_newest_first(&self) -> Vec<ThreadPullRequest> {
        let mut pull_requests = self.pull_requests.clone();
        pull_requests.sort_by_key(|pull_request| std::cmp::Reverse(pull_request.touched_at));
        pull_requests
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PullRequestStatusLoad {
    Loading,
    /// `None`: the pull request does not exist or cannot be seen.
    Ready(Option<Box<PullRequestLiveState>>),
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestStatusEntry {
    pub load: PullRequestStatusLoad,
    fetched_at: Instant,
}

impl PullRequestStatusEntry {
    pub fn state(&self) -> Option<&PullRequestLiveState> {
        match &self.load {
            PullRequestStatusLoad::Ready(state) => state.as_deref(),
            _ => None,
        }
    }

    fn is_stale(&self) -> bool {
        let age = self.fetched_at.elapsed();
        match &self.load {
            PullRequestStatusLoad::Loading => false,
            PullRequestStatusLoad::Failed => age >= OPEN_STATUS_STALE_AFTER,
            PullRequestStatusLoad::Ready(None) => false,
            PullRequestStatusLoad::Ready(Some(state)) => match state.summary.status {
                PullRequestStatus::Open | PullRequestStatus::Draft
                    if state.summary.ci_status == crate::pull_requests::CiStatus::Pending =>
                {
                    age >= PENDING_STATUS_STALE_AFTER
                }
                PullRequestStatus::Open | PullRequestStatus::Draft => {
                    age >= OPEN_STATUS_STALE_AFTER
                }
                PullRequestStatus::Merged | PullRequestStatus::Closed => false,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchLookup {
    Loading,
    Ready(Option<BranchPullRequest>),
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchLookupEntry {
    pub lookup: BranchLookup,
    fetched_at: Instant,
}

/// What `git` says about a thread's working directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCheckout {
    pub root: PathBuf,
    /// `None` on a detached HEAD.
    pub branch: Option<String>,
    pub origin_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitCheckoutLoad {
    Loading,
    /// `None`: not a Git checkout.
    Ready(Option<GitCheckout>),
}

/// Everything the pull-request surfaces read, part of the workspace snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadPullRequestsState {
    pub threads: HashMap<ThreadId, ThreadAttachmentEntry>,
    /// By attachment identity key.
    pub statuses: HashMap<String, PullRequestStatusEntry>,
    pub branch_lookups: HashMap<(PathBuf, String), BranchLookupEntry>,
    pub checkouts: HashMap<PathBuf, GitCheckoutLoad>,
    /// The backfill cutoff in epoch milliseconds: the ChatGPT app's, or the one
    /// recorded here when it has none.
    pub backfill_cutoff: Option<i64>,
    /// Threads the ChatGPT app finished backfilling (read once).
    pub chatgpt_backfill_completed: HashSet<ThreadId>,
    /// Threads backfill already ran for in this session.
    backfilled_this_session: HashSet<ThreadId>,
    next_epoch: u64,
}

/// What the sidebar chip shows for a thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChipPullRequest {
    /// Nothing to show (no pull request, or none that resolved).
    None,
    /// A pull request whose state is still loading; nothing is shown yet.
    Pending,
    Show {
        url: String,
        state: Box<PullRequestLiveState>,
    },
}

/// The chip's state, the reference's `N1i(M1i(...))`.
pub fn chip_icon(state: &PullRequestLiveState) -> StatusIcon {
    state.summary.status_icon()
}

fn is_open(state: &PullRequestLiveState) -> bool {
    matches!(
        state.summary.status,
        PullRequestStatus::Open | PullRequestStatus::Draft
    )
}

/// `owner/name` of a GitHub remote URL (https or ssh), lowercased.
pub fn remote_repository(remote: &str) -> Option<String> {
    let trimmed = remote.trim().trim_end_matches('/').trim_end_matches(".git");
    let path = if let Some(rest) = trimmed.split_once("://").map(|(_, rest)| rest) {
        rest.split_once('/')?.1.to_owned()
    } else {
        // scp-like `git@host:owner/name`
        trimmed.split_once(':')?.1.to_owned()
    };
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?;
    let name = parts.next()?;
    Some(format!("{}/{}", owner.to_lowercase(), name.to_lowercase()))
}

impl ThreadPullRequestsState {
    fn status(&self, pull_request: &ThreadPullRequest) -> Option<&PullRequestStatusEntry> {
        self.statuses.get(&pull_request.identity_key)
    }

    /// The reference's `W$i`: attachments decide the chip for threads created
    /// after the cutoff, and for older ones once they have an attachment
    /// record (a pull request, or finished backfill).
    pub fn attachments_authoritative(
        &self,
        thread: &ThreadSummary,
        completed: &HashSet<ThreadId>,
    ) -> bool {
        let Some(cutoff) = self.backfill_cutoff else {
            return true;
        };
        if thread.created_at.saturating_mul(1000) >= cutoff {
            return true;
        }
        self.has_record(&thread.thread_id, completed)
    }

    /// The reference's `fh(...) != null`: at least one pull request, or an
    /// "empty record" (backfill finished) once the attachments were read.
    fn has_record(&self, thread_id: &str, completed: &HashSet<ThreadId>) -> bool {
        let Some(entry) = self.threads.get(thread_id) else {
            return false;
        };
        if !entry.pull_requests.is_empty() {
            return true;
        }
        entry.is_settled()
            && (completed.contains(thread_id)
                || self.chatgpt_backfill_completed.contains(thread_id))
    }

    /// The reference's `F1i`: which attached pull request the chip shows, and
    /// which one to fetch next. Attachments are tried newest first; an open,
    /// ready pull request on the thread's branch and repository wins.
    pub fn chip_selection(
        &self,
        thread_id: &str,
        thread_branch: Option<&str>,
        origin: Option<&str>,
    ) -> (ChipPullRequest, Option<ThreadPullRequest>) {
        let Some(entry) = self.threads.get(thread_id) else {
            return (ChipPullRequest::None, None);
        };
        let candidates = entry.pull_requests_newest_first();
        if candidates.is_empty() {
            return (ChipPullRequest::None, None);
        }
        let origin = origin.and_then(remote_repository);
        let thread_branch = thread_branch
            .map(str::trim)
            .filter(|branch| !branch.is_empty());
        let data = |pull_request: &ThreadPullRequest| {
            self.status(pull_request)
                .and_then(PullRequestStatusEntry::state)
        };
        let fetched = |pull_request: &ThreadPullRequest| {
            self.status(pull_request)
                .is_some_and(|entry| !matches!(entry.load, PullRequestStatusLoad::Loading))
        };
        let failed = |pull_request: &ThreadPullRequest| {
            self.status(pull_request)
                .is_some_and(|entry| matches!(entry.load, PullRequestStatusLoad::Failed))
        };
        let strict: Vec<&ThreadPullRequest> = candidates
            .iter()
            .filter(|pull_request| {
                data(pull_request).is_some_and(|state| {
                    is_open(state)
                        && Some(state.summary.head_branch.as_str()) == thread_branch
                        && origin.is_some()
                        && state.head_repository.as_deref().map(str::to_lowercase) == origin
                })
            })
            .collect();
        let any_open = candidates
            .iter()
            .find(|pull_request| data(pull_request).is_some_and(is_open));
        let strict_ready = strict.iter().copied().find(|pull_request| {
            data(pull_request).is_some_and(|state| state.summary.status != PullRequestStatus::Draft)
        });
        let open_ready = candidates.iter().find(|pull_request| {
            data(pull_request).is_some_and(|state| {
                is_open(state) && state.summary.status != PullRequestStatus::Draft
            })
        });
        let fallback = strict_ready
            .or(open_ready)
            .or(strict.first().copied())
            .or(any_open)
            .or_else(|| {
                candidates
                    .iter()
                    .find(|pull_request| data(pull_request).is_some())
            });
        let decided = strict_ready.or(if origin.is_none() || thread_branch.is_none() {
            open_ready
        } else {
            None
        });
        let search_end = decided
            .and_then(|decided| candidates.iter().position(|candidate| candidate == decided))
            .unwrap_or(candidates.len());
        let next_fetch = candidates[..search_end]
            .iter()
            .find(|pull_request| !fetched(pull_request) && !failed(pull_request));
        let shown = decided
            .or(next_fetch)
            .or(fallback)
            .unwrap_or(&candidates[0]);
        let chip = match data(shown) {
            Some(state) => ChipPullRequest::Show {
                url: shown.url.clone(),
                state: Box::new(state.clone()),
            },
            None if !fetched(shown) => ChipPullRequest::Pending,
            None => ChipPullRequest::None,
        };
        let fetch = next_fetch
            .filter(|pull_request| self.status(pull_request).is_none())
            .cloned();
        (chip, fetch)
    }
}

fn to_thread_pull_request(attachment: &AgentThreadAttachment) -> Option<ThreadPullRequest> {
    let pull_request = attachment.pull_request()?;
    Some(ThreadPullRequest {
        url: pull_request.url.clone(),
        identity_key: attachment.identity_key.clone(),
        root: pull_request.root.clone(),
        head_branch: pull_request.head_branch.clone(),
        touched_at: attachment.created_at.saturating_mul(1000),
    })
}

fn identity_of(url: &str) -> Option<String> {
    AgentPullRequestRef::parse(url).map(|pull_request| pull_request.identity_key())
}

fn from_record(record: &RecordedPullRequest) -> Option<ThreadPullRequest> {
    let url = record.url.clone()?;
    Some(ThreadPullRequest {
        identity_key: identity_of(&url)?,
        url,
        root: record.root.clone(),
        head_branch: record.head_branch.clone(),
        touched_at: record.touched_at,
    })
}

fn to_record(pull_request: &ThreadPullRequest) -> RecordedPullRequest {
    RecordedPullRequest {
        url: Some(pull_request.url.clone()),
        root: pull_request.root.clone(),
        head_branch: pull_request.head_branch.clone(),
        touched_at: pull_request.touched_at,
    }
}

/// Writes one thread's record, most recent last, at most 100 threads. A new
/// thread is only recorded when it has a pull request; an existing record is
/// replaced even when it becomes empty.
pub(super) fn mirror_record(
    records: &mut Vec<PullRequestAttachmentRecord>,
    thread_id: &str,
    pull_requests: &[ThreadPullRequest],
    insert_empty: bool,
) -> bool {
    let mut recorded: Vec<RecordedPullRequest> = pull_requests.iter().map(to_record).collect();
    recorded.sort_by_key(|record| record.touched_at);
    if recorded.len() > ATTACHMENT_RECORD_PER_THREAD_LIMIT {
        recorded.drain(..recorded.len() - ATTACHMENT_RECORD_PER_THREAD_LIMIT);
    }
    let record = PullRequestAttachmentRecord {
        thread_id: thread_id.to_owned(),
        pull_requests: recorded,
    };
    match records
        .iter()
        .position(|existing| existing.thread_id == thread_id)
    {
        Some(index) if records[index] == record => false,
        Some(index) => {
            records[index] = record;
            true
        }
        None if record.pull_requests.is_empty() && !insert_empty => false,
        None => {
            records.push(record);
            if records.len() > ATTACHMENT_RECORD_THREAD_LIMIT {
                records.drain(..records.len() - ATTACHMENT_RECORD_THREAD_LIMIT);
            }
            true
        }
    }
}

pub(super) fn mark_backfill_completed(completed: &mut Vec<ThreadId>, thread_id: &str) -> bool {
    if completed.iter().any(|existing| existing == thread_id) {
        return false;
    }
    completed.push(thread_id.to_owned());
    if completed.len() > BACKFILL_COMPLETED_LIMIT {
        completed.drain(..completed.len() - BACKFILL_COMPLETED_LIMIT);
    }
    true
}

fn git_output(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = crate::git_review::process::run(
        Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0"),
        None,
        GIT_TIMEOUT,
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// The checkout a working directory belongs to.
pub fn read_git_checkout(cwd: &Path) -> Option<GitCheckout> {
    let root = PathBuf::from(git_output(cwd, &["rev-parse", "--show-toplevel"])?);
    Some(GitCheckout {
        branch: git_output(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"]),
        origin_url: git_output(&root, &["remote", "get-url", "origin"]),
        root,
    })
}

fn same_path(a: &str, b: &Path) -> bool {
    Path::new(a.trim_end_matches('/')) == b
}

/// Outcome of an attach or detach as the UI reports it.
pub type AttachmentWrite = Result<(), String>;

impl WorkspaceStore {
    fn completed_threads(snapshot: &WorkspaceSnapshot) -> HashSet<ThreadId> {
        snapshot
            .preferences
            .pull_request_backfill_completed
            .iter()
            .cloned()
            .collect()
    }

    /// Reads the backfill bookkeeping once: the ChatGPT app's cutoff (else the
    /// one recorded here, else now) and its completed threads.
    pub(super) fn initialize_pull_request_backfill(&self) {
        let chatgpt_cutoff = crate::pull_requests::associations::backfill_cutoff();
        let chatgpt_completed = crate::pull_requests::associations::backfill_completed();
        let mut record_cutoff = false;
        self.update(|snapshot| {
            let cutoff = chatgpt_cutoff
                .or(snapshot.preferences.pull_request_backfill_cutoff_at)
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
            if chatgpt_cutoff.is_none()
                && snapshot
                    .preferences
                    .pull_request_backfill_cutoff_at
                    .is_none()
            {
                snapshot.preferences.pull_request_backfill_cutoff_at = Some(cutoff);
                record_cutoff = true;
            }
            snapshot.pull_requests.backfill_cutoff = Some(cutoff);
            snapshot.pull_requests.chatgpt_backfill_completed =
                chatgpt_completed.into_iter().collect();
        });
        if record_cutoff {
            self.save_preferences();
        }
    }

    /// Makes sure everything the sidebar chip of these threads needs is loaded
    /// or loading: attachments, their pull requests' state, the checkout of
    /// each working directory, the branch lookup and backfill of old threads.
    /// Cheap when nothing is stale; the view calls it on every render.
    pub fn request_thread_pull_requests(self: &Arc<Self>, threads: &[ThreadSummary]) {
        for thread in threads {
            self.ensure_thread_attachments(&thread.thread_id, false);
            self.ensure_checkout(&thread.cwd);
            self.advance_thread_pull_request(thread);
        }
    }

    /// The current chat gets focus again: its attachments are re-read.
    pub fn refresh_thread_attachments(self: &Arc<Self>, thread_id: &str) {
        self.ensure_thread_attachments(thread_id, true);
    }

    fn ensure_thread_attachments(self: &Arc<Self>, thread_id: &str, force: bool) {
        let mut start = None;
        if let Ok(mut snapshot) = self.snapshot.lock() {
            let state = &mut snapshot.pull_requests;
            let stale = match state.threads.get(thread_id) {
                None => true,
                Some(entry) if entry.load == AttachmentLoad::Loading => false,
                Some(entry) => {
                    force
                        || entry
                            .read_at
                            .is_none_or(|read_at| read_at.elapsed() >= ATTACHMENT_STALE_AFTER)
                }
            };
            if stale {
                state.next_epoch += 1;
                let epoch = state.next_epoch;
                let previous = state.threads.remove(thread_id);
                let mut entry = ThreadAttachmentEntry::loading(epoch);
                if let Some(previous) = previous {
                    // Keep showing what was there while it is re-read.
                    entry.pull_requests = previous.pull_requests;
                    entry.worktrees = previous.worktrees;
                    entry.generation = previous.generation;
                    entry.read_at = previous.read_at;
                }
                state.threads.insert(thread_id.to_owned(), entry);
                start = Some(epoch);
            }
        }
        let Some(epoch) = start else {
            return;
        };
        self.publish();
        let receiver = self
            .backend
            .list_thread_attachments(thread_id.to_owned(), force);
        let store = Arc::clone(self);
        let thread_id = thread_id.to_owned();
        std::thread::spawn(move || {
            let result = receiver.recv_blocking().unwrap_or_else(|_| {
                Err(AgentAttachmentError::Failed(
                    crate::i18n::text("附件读取提前结束").to_owned(),
                ))
            });
            store.finish_attachment_read(&thread_id, epoch, result);
        });
    }

    fn finish_attachment_read(
        self: &Arc<Self>,
        thread_id: &str,
        epoch: u64,
        result: Result<crate::agent::AgentThreadAttachments, AgentAttachmentError>,
    ) {
        let mut save = false;
        let mut stale = false;
        self.update(|snapshot| {
            let current = snapshot
                .pull_requests
                .threads
                .get(thread_id)
                .is_some_and(|entry| entry.epoch == epoch);
            if !current {
                // A newer read (after a notification) owns the entry.
                stale = true;
                return;
            }
            let records = &mut snapshot.preferences.pull_request_attachment_records;
            let entry = snapshot
                .pull_requests
                .threads
                .get_mut(thread_id)
                .expect("checked above");
            match result {
                Ok(read) => {
                    entry.pull_requests = read
                        .attachments
                        .iter()
                        .filter_map(to_thread_pull_request)
                        .collect();
                    entry.worktrees = read
                        .attachments
                        .iter()
                        .filter_map(|attachment| attachment.worktree().cloned())
                        .collect();
                    entry.generation = Some(read.generation);
                    entry.load = AttachmentLoad::Loaded;
                    entry.read_at = Some(Instant::now());
                    save = mirror_record(records, thread_id, &entry.pull_requests, false);
                }
                Err(AgentAttachmentError::Unsupported) => {
                    entry.pull_requests = records
                        .iter()
                        .find(|record| record.thread_id == thread_id)
                        .map(|record| {
                            record
                                .pull_requests
                                .iter()
                                .filter_map(from_record)
                                .collect()
                        })
                        .unwrap_or_default();
                    entry.worktrees.clear();
                    entry.generation = None;
                    entry.load = AttachmentLoad::Unsupported;
                    entry.read_at = Some(Instant::now());
                }
                Err(AgentAttachmentError::Failed(message)) => {
                    entry.load = AttachmentLoad::Failed(message);
                }
            }
        });
        if save {
            self.save_preferences();
        }
        if !stale && let Some(thread) = self.snapshot().thread(thread_id).cloned() {
            self.advance_thread_pull_request(&thread);
        }
    }

    /// A notification: the thread's attachments changed somewhere.
    pub(super) fn attachment_updated(self: &Arc<Self>, update: AgentAttachmentUpdate) {
        if update.attachment_type != PULL_REQUEST_ATTACHMENT_TYPE
            && update.attachment_type != crate::agent::WORKTREE_ATTACHMENT_TYPE
        {
            return;
        }
        if update.attachment_type == PULL_REQUEST_ATTACHMENT_TYPE
            && update.operation == AgentAttachmentOperation::Deleted
        {
            self.record_backfill_completed(&update.thread_id);
        }
        let tracked = self
            .snapshot()
            .pull_requests
            .threads
            .contains_key(&update.thread_id);
        if tracked {
            self.ensure_thread_attachments(&update.thread_id, true);
        }
    }

    /// A deleted thread leaves no pull-request bookkeeping behind.
    pub(super) fn forget_thread_pull_requests(&self, thread_id: &str) {
        let mut changed = false;
        self.update(|snapshot| {
            snapshot.pull_requests.threads.remove(thread_id);
            let preferences = &mut snapshot.preferences;
            let records = preferences.pull_request_attachment_records.len();
            preferences
                .pull_request_attachment_records
                .retain(|record| record.thread_id != thread_id);
            let completed = preferences.pull_request_backfill_completed.len();
            preferences
                .pull_request_backfill_completed
                .retain(|existing| existing != thread_id);
            changed = records != preferences.pull_request_attachment_records.len()
                || completed != preferences.pull_request_backfill_completed.len();
        });
        if changed {
            self.save_preferences();
        }
    }

    fn record_backfill_completed(&self, thread_id: &str) {
        let mut changed = false;
        self.update(|snapshot| {
            changed = mark_backfill_completed(
                &mut snapshot.preferences.pull_request_backfill_completed,
                thread_id,
            );
        });
        if changed {
            self.save_preferences();
        }
    }

    pub fn ensure_checkout(self: &Arc<Self>, cwd: &Path) {
        if cwd.as_os_str().is_empty() {
            return;
        }
        let start = self
            .snapshot
            .lock()
            .map(|mut snapshot| {
                if snapshot.pull_requests.checkouts.contains_key(cwd) {
                    return false;
                }
                snapshot
                    .pull_requests
                    .checkouts
                    .insert(cwd.to_owned(), GitCheckoutLoad::Loading);
                true
            })
            .unwrap_or(false);
        if !start {
            return;
        }
        let store = Arc::clone(self);
        let cwd = cwd.to_owned();
        std::thread::spawn(move || {
            let checkout = read_git_checkout(&cwd);
            store.update(|snapshot| {
                snapshot
                    .pull_requests
                    .checkouts
                    .insert(cwd.clone(), GitCheckoutLoad::Ready(checkout));
            });
            if let Some(threads) = store.threads_in(&cwd) {
                for thread in threads {
                    store.advance_thread_pull_request(&thread);
                }
            }
        });
    }

    fn threads_in(&self, cwd: &Path) -> Option<Vec<ThreadSummary>> {
        let snapshot = self.snapshot();
        let threads: Vec<ThreadSummary> = snapshot
            .pinned_threads
            .iter()
            .chain(&snapshot.recent_threads)
            .filter(|thread| thread.cwd == cwd)
            .cloned()
            .collect();
        (!threads.is_empty()).then_some(threads)
    }

    /// Git metadata of a working directory changed (a push, a new branch).
    pub fn refresh_checkout(self: &Arc<Self>, cwd: &Path) {
        self.update(|snapshot| {
            snapshot.pull_requests.checkouts.remove(cwd);
        });
        self.ensure_checkout(cwd);
    }

    /// A push can change what a branch lookup finds.
    pub fn invalidate_branch_lookups(&self) {
        self.update(|snapshot| snapshot.pull_requests.branch_lookups.clear());
    }

    /// The checkout of a working directory, when it has been read.
    pub fn checkout_for(snapshot: &WorkspaceSnapshot, cwd: &Path) -> Option<GitCheckout> {
        match snapshot.pull_requests.checkouts.get(cwd)? {
            GitCheckoutLoad::Ready(checkout) => checkout.clone(),
            GitCheckoutLoad::Loading => None,
        }
    }

    /// Next step for one thread: fetch the pull-request state its chip needs,
    /// or (old threads without a record) the branch lookup and backfill.
    fn advance_thread_pull_request(self: &Arc<Self>, thread: &ThreadSummary) {
        let snapshot = self.snapshot();
        let state = &snapshot.pull_requests;
        let completed = Self::completed_threads(&snapshot);
        let Some(entry) = state.threads.get(&thread.thread_id) else {
            return;
        };
        let branch = thread
            .git
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty());
        if state.attachments_authoritative(thread, &completed) {
            let (_, fetch) =
                state.chip_selection(&thread.thread_id, branch, thread.git.origin_url.as_deref());
            if let Some(fetch) = fetch {
                self.ensure_status(&fetch.identity_key, &fetch.url);
            }
            // Refresh stale states of pull requests already shown.
            for pull_request in entry.pull_requests_newest_first() {
                if state
                    .statuses
                    .get(&pull_request.identity_key)
                    .is_some_and(PullRequestStatusEntry::is_stale)
                {
                    self.ensure_status(&pull_request.identity_key, &pull_request.url);
                }
            }
            return;
        }
        // Legacy chip and backfill: the pull request of the thread's branch.
        let Some(branch) = branch else {
            return;
        };
        let Some(checkout) = Self::checkout_for(&snapshot, &thread.cwd) else {
            return;
        };
        let key = (checkout.root.clone(), branch.to_owned());
        match state.branch_lookups.get(&key) {
            None => self.ensure_branch_lookup(checkout.root.clone(), branch.to_owned()),
            Some(entry)
                if entry.fetched_at.elapsed() >= BRANCH_LOOKUP_STALE_AFTER
                    && !matches!(entry.lookup, BranchLookup::Loading) =>
            {
                self.ensure_branch_lookup(checkout.root.clone(), branch.to_owned())
            }
            Some(BranchLookupEntry {
                lookup: BranchLookup::Ready(found),
                ..
            }) => {
                if let Some(found) = found
                    && let Some(identity) = identity_of(&found.url)
                {
                    match state.statuses.get(&identity) {
                        None => {
                            self.ensure_status(&identity, &found.url);
                            return;
                        }
                        Some(entry) if matches!(entry.load, PullRequestStatusLoad::Loading) => {
                            return;
                        }
                        Some(_) => {}
                    }
                }
                self.backfill(thread, &checkout.root, branch, found.clone());
            }
            Some(_) => {}
        }
    }

    fn ensure_status(self: &Arc<Self>, identity_key: &str, url: &str) {
        let start = self
            .snapshot
            .lock()
            .map(|mut snapshot| {
                let statuses = &mut snapshot.pull_requests.statuses;
                if statuses
                    .get(identity_key)
                    .is_some_and(|entry| !entry.is_stale())
                {
                    return false;
                }
                let previous = statuses.remove(identity_key);
                statuses.insert(
                    identity_key.to_owned(),
                    PullRequestStatusEntry {
                        // A refresh keeps the last state visible.
                        load: match previous {
                            Some(PullRequestStatusEntry {
                                load: load @ PullRequestStatusLoad::Ready(Some(_)),
                                ..
                            }) => load,
                            _ => PullRequestStatusLoad::Loading,
                        },
                        fetched_at: Instant::now(),
                    },
                );
                true
            })
            .unwrap_or(false);
        if !start {
            return;
        }
        let store = Arc::clone(self);
        let identity_key = identity_key.to_owned();
        let url = url.to_owned();
        std::thread::spawn(move || {
            let load = match AgentPullRequestRef::parse(&url) {
                Some(pull_request)
                    if pull_request.provider == crate::agent::AgentPullRequestProvider::GitHub
                        && pull_request.hostname == "github.com" =>
                {
                    match crate::pull_requests::pull_request_state(
                        &pull_request.owner,
                        &pull_request.repository,
                        pull_request.number,
                    ) {
                        Ok(state) => PullRequestStatusLoad::Ready(state.map(Box::new)),
                        Err(_) => PullRequestStatusLoad::Failed,
                    }
                }
                // GitHub Enterprise and GitLab states are not read here.
                _ => PullRequestStatusLoad::Ready(None),
            };
            store.update(|snapshot| {
                snapshot.pull_requests.statuses.insert(
                    identity_key.clone(),
                    PullRequestStatusEntry {
                        load,
                        fetched_at: Instant::now(),
                    },
                );
            });
            store.advance_threads_with(&identity_key);
        });
    }

    fn advance_threads_with(self: &Arc<Self>, identity_key: &str) {
        let snapshot = self.snapshot();
        let threads: Vec<ThreadSummary> = snapshot
            .pull_requests
            .threads
            .iter()
            .filter(|(_, entry)| {
                entry
                    .pull_requests
                    .iter()
                    .any(|pull_request| pull_request.identity_key == identity_key)
            })
            .filter_map(|(thread_id, _)| snapshot.thread(thread_id).cloned())
            .collect();
        for thread in threads {
            self.advance_thread_pull_request(&thread);
        }
        // Legacy chips wait on the state of their branch's pull request.
        let legacy: Vec<ThreadSummary> = snapshot
            .pinned_threads
            .iter()
            .chain(&snapshot.recent_threads)
            .filter(|thread| {
                !snapshot
                    .pull_requests
                    .attachments_authoritative(thread, &Self::completed_threads(&snapshot))
            })
            .cloned()
            .collect();
        for thread in legacy {
            self.advance_thread_pull_request(&thread);
        }
    }

    fn ensure_branch_lookup(self: &Arc<Self>, root: PathBuf, branch: String) {
        let key = (root.clone(), branch.clone());
        self.update(|snapshot| {
            snapshot.pull_requests.branch_lookups.insert(
                key.clone(),
                BranchLookupEntry {
                    lookup: BranchLookup::Loading,
                    fetched_at: Instant::now(),
                },
            );
        });
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let lookup = match crate::pull_requests::pull_request_for_branch(&root, &branch) {
                Ok(found) => BranchLookup::Ready(found),
                Err(_) => BranchLookup::Failed,
            };
            store.update(|snapshot| {
                snapshot.pull_requests.branch_lookups.insert(
                    key.clone(),
                    BranchLookupEntry {
                        lookup,
                        fetched_at: Instant::now(),
                    },
                );
            });
            if let Some(threads) = store.threads_on_branch(&root, &branch) {
                for thread in threads {
                    store.advance_thread_pull_request(&thread);
                }
            }
        });
    }

    fn threads_on_branch(&self, root: &Path, branch: &str) -> Option<Vec<ThreadSummary>> {
        let snapshot = self.snapshot();
        let threads: Vec<ThreadSummary> = snapshot
            .pinned_threads
            .iter()
            .chain(&snapshot.recent_threads)
            .filter(|thread| thread.git.branch.as_deref().map(str::trim) == Some(branch))
            .filter(|thread| {
                Self::checkout_for(&snapshot, &thread.cwd)
                    .is_some_and(|checkout| checkout.root == root)
            })
            .cloned()
            .collect();
        (!threads.is_empty()).then_some(threads)
    }

    /// The reference's `J$i`: an old thread whose attachments were read and
    /// hold no pull request gets the one its branch has, once per session.
    fn backfill(
        self: &Arc<Self>,
        thread: &ThreadSummary,
        root: &Path,
        branch: &str,
        found: Option<BranchPullRequest>,
    ) {
        let snapshot = self.snapshot();
        let state = &snapshot.pull_requests;
        let completed = Self::completed_threads(&snapshot);
        let Some(cutoff) = state.backfill_cutoff else {
            return;
        };
        let Some(entry) = state.threads.get(&thread.thread_id) else {
            return;
        };
        let eligible = thread.created_at.saturating_mul(1000) < cutoff
            && entry.is_settled()
            && entry.pull_requests.is_empty()
            && !completed.contains(&thread.thread_id)
            && !state.chatgpt_backfill_completed.contains(&thread.thread_id)
            && !state.backfilled_this_session.contains(&thread.thread_id);
        if !eligible {
            return;
        }
        self.update(|snapshot| {
            snapshot
                .pull_requests
                .backfilled_this_session
                .insert(thread.thread_id.clone());
        });
        match found {
            Some(found) => {
                let head_branch = Some(found.head_branch.trim().to_owned())
                    .filter(|head| !head.is_empty())
                    .unwrap_or_else(|| branch.to_owned());
                let _ = self.attach_pull_request(
                    thread.thread_id.clone(),
                    found.url,
                    Some(root.to_string_lossy().into_owned()),
                    Some(head_branch),
                );
            }
            None => self.record_empty(&thread.thread_id),
        }
    }

    /// The reference's `R$i`: remembers that backfill found nothing.
    fn record_empty(&self, thread_id: &str) {
        let unsupported = self
            .snapshot()
            .pull_requests
            .threads
            .get(thread_id)
            .is_some_and(|entry| entry.load == AttachmentLoad::Unsupported);
        if !unsupported {
            self.record_backfill_completed(thread_id);
            return;
        }
        let mut changed = false;
        self.update(|snapshot| {
            changed = mirror_record(
                &mut snapshot.preferences.pull_request_attachment_records,
                thread_id,
                &[],
                true,
            );
        });
        if changed {
            self.save_preferences();
        }
    }

    /// The reference's single write path (`z$i`/`M$i`): the URL is normalised
    /// and keyed exactly like the reference; the pull request shows at once and
    /// is rolled back if the server refuses. Nothing is written without a URL
    /// that parses as a pull request.
    pub fn attach_pull_request(
        self: &Arc<Self>,
        thread_id: ThreadId,
        url: String,
        root: Option<String>,
        head_branch: Option<String>,
    ) -> Receiver<AttachmentWrite> {
        let (sender, receiver) = async_channel::bounded(1);
        let head_branch = head_branch
            .map(|branch| branch.trim().to_owned())
            .filter(|b| !b.is_empty());
        let Some(pull_request) = AgentPullRequestRef::parse(&url) else {
            let _ = sender.send_blocking(Ok(()));
            return receiver;
        };
        let canonical = pull_request.canonical_url();
        let identity_key = pull_request.identity_key();
        let optimistic = ThreadPullRequest {
            url: canonical.clone(),
            identity_key: identity_key.clone(),
            root: root.clone(),
            head_branch: head_branch.clone(),
            touched_at: chrono::Utc::now().timestamp_millis(),
        };
        let mut before = None;
        self.update(|snapshot| {
            let entry = snapshot
                .pull_requests
                .threads
                .entry(thread_id.clone())
                .or_insert_with(|| ThreadAttachmentEntry::loading(0));
            before = Some(entry.pull_requests.clone());
            if !entry
                .pull_requests
                .iter()
                .any(|existing| existing.url.eq_ignore_ascii_case(&canonical))
            {
                entry.pull_requests.push(optimistic.clone());
            }
        });
        let snapshot = self.snapshot();
        let entry = snapshot.pull_requests.threads.get(&thread_id).cloned();
        let unsupported = entry
            .as_ref()
            .is_some_and(|entry| entry.load == AttachmentLoad::Unsupported);
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let result = if unsupported {
                store.write_local_attachment(&thread_id, optimistic);
                Ok(())
            } else {
                let generation = entry.and_then(|entry| entry.generation);
                let payload = serde_json::json!({
                    "url": canonical,
                    "root": root,
                    "headBranch": head_branch,
                });
                let receiver = match generation {
                    Some(generation) => {
                        store
                            .backend
                            .add_thread_attachment(AgentAttachmentAddRequest {
                                generation,
                                thread_id: thread_id.clone(),
                                attachment_type: PULL_REQUEST_ATTACHMENT_TYPE.to_owned(),
                                identity_key,
                                payload,
                            })
                    }
                    None => {
                        // Not read in any generation yet: read first, then write.
                        let read = store
                            .backend
                            .list_thread_attachments(thread_id.clone(), false)
                            .recv_blocking();
                        match read {
                            Ok(Ok(read)) => {
                                store
                                    .backend
                                    .add_thread_attachment(AgentAttachmentAddRequest {
                                        generation: read.generation,
                                        thread_id: thread_id.clone(),
                                        attachment_type: PULL_REQUEST_ATTACHMENT_TYPE.to_owned(),
                                        identity_key,
                                        payload,
                                    })
                            }
                            Ok(Err(error)) => {
                                let (sender, receiver) = async_channel::bounded(1);
                                let _ = sender.send_blocking(Err(error));
                                receiver
                            }
                            Err(_) => {
                                let (sender, receiver) = async_channel::bounded(1);
                                let _ = sender.send_blocking(Err(AgentAttachmentError::Failed(
                                    crate::i18n::text("附件读取提前结束").to_owned(),
                                )));
                                receiver
                            }
                        }
                    }
                };
                match receiver.recv_blocking() {
                    Ok(Ok(_)) => {
                        store.record_backfill_completed(&thread_id);
                        Ok(())
                    }
                    Ok(Err(AgentAttachmentError::Unsupported)) => {
                        store.write_local_attachment(&thread_id, optimistic);
                        Ok(())
                    }
                    Ok(Err(AgentAttachmentError::Failed(message))) => Err(message),
                    Err(_) => Err(crate::i18n::text("附件写入提前结束").to_owned()),
                }
            };
            if result.is_err() {
                store.update(|snapshot| {
                    if let (Some(entry), Some(before)) =
                        (snapshot.pull_requests.threads.get_mut(&thread_id), before)
                    {
                        entry.pull_requests = before;
                    }
                });
            }
            store.ensure_thread_attachments(&thread_id, true);
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    /// Local record write when the server has no attachments: an existing
    /// entry for the same pull request (or the same branch and root) is
    /// replaced, with a strictly newer touchedAt.
    fn write_local_attachment(&self, thread_id: &str, pull_request: ThreadPullRequest) {
        let mut changed = false;
        self.update(|snapshot| {
            let records = &mut snapshot.preferences.pull_request_attachment_records;
            let mut current: Vec<ThreadPullRequest> = records
                .iter()
                .find(|record| record.thread_id == thread_id)
                .map(|record| {
                    record
                        .pull_requests
                        .iter()
                        .filter_map(from_record)
                        .collect()
                })
                .unwrap_or_default();
            let newest = current
                .iter()
                .map(|existing| existing.touched_at)
                .max()
                .unwrap_or(0);
            current.retain(|existing| existing.identity_key != pull_request.identity_key);
            let mut pull_request = pull_request;
            pull_request.touched_at = pull_request.touched_at.max(newest + 1);
            current.push(pull_request);
            changed = mirror_record(records, thread_id, &current, true);
            if let Some(entry) = snapshot.pull_requests.threads.get_mut(thread_id) {
                entry.pull_requests = current;
            }
        });
        if changed {
            self.save_preferences();
        }
    }

    /// The reference's `B$i`/`N$i`: removes the thread's attachments of this
    /// pull request (all pull requests when `url` is `None`), optimistically,
    /// and records the thread as done so backfill never re-adds it.
    pub fn detach_pull_request(
        self: &Arc<Self>,
        thread_id: ThreadId,
        url: Option<String>,
    ) -> Receiver<AttachmentWrite> {
        let (sender, receiver) = async_channel::bounded(1);
        let target = url.as_deref().and_then(AgentPullRequestRef::parse);
        let matches = move |pull_request: &ThreadPullRequest| match &target {
            None => true,
            Some(target) => AgentPullRequestRef::parse(&pull_request.url)
                .is_some_and(|candidate| candidate.same_as(target)),
        };
        let mut removed = Vec::new();
        let mut before = None;
        self.update(|snapshot| {
            if let Some(entry) = snapshot.pull_requests.threads.get_mut(&thread_id) {
                before = Some(entry.pull_requests.clone());
                let (gone, kept): (Vec<_>, Vec<_>) = entry
                    .pull_requests
                    .drain(..)
                    .partition(|pull_request| matches(pull_request));
                entry.pull_requests = kept;
                removed = gone;
            }
        });
        let entry = self
            .snapshot()
            .pull_requests
            .threads
            .get(&thread_id)
            .cloned();
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let result = match entry.as_ref().map(|entry| (&entry.load, entry.generation)) {
                Some((AttachmentLoad::Unsupported, _)) | None => {
                    store.remove_local_attachments(&thread_id, &removed);
                    Ok(())
                }
                Some((_, Some(generation))) => {
                    let mut result = Ok(());
                    for pull_request in &removed {
                        let outcome = store
                            .backend
                            .remove_thread_attachment(AgentAttachmentRemoveRequest {
                                generation,
                                thread_id: thread_id.clone(),
                                attachment_type: PULL_REQUEST_ATTACHMENT_TYPE.to_owned(),
                                identity_key: pull_request.identity_key.clone(),
                            })
                            .recv_blocking();
                        match outcome {
                            Ok(Ok(())) => {}
                            Ok(Err(AgentAttachmentError::Unsupported)) => {
                                store.remove_local_attachments(
                                    &thread_id,
                                    std::slice::from_ref(pull_request),
                                );
                            }
                            Ok(Err(AgentAttachmentError::Failed(message))) => {
                                result = Err(message);
                                break;
                            }
                            Err(_) => {
                                result = Err(crate::i18n::text("附件写入提前结束").to_owned());
                                break;
                            }
                        }
                    }
                    if result.is_ok() {
                        store.record_backfill_completed(&thread_id);
                    }
                    result
                }
                Some((_, None)) => Err(crate::i18n::text("附件尚未读取，未发送").to_owned()),
            };
            if result.is_err() {
                store.update(|snapshot| {
                    if let (Some(entry), Some(before)) =
                        (snapshot.pull_requests.threads.get_mut(&thread_id), before)
                    {
                        entry.pull_requests = before;
                    }
                });
            }
            store.ensure_thread_attachments(&thread_id, true);
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    fn remove_local_attachments(&self, thread_id: &str, removed: &[ThreadPullRequest]) {
        let mut changed = false;
        self.update(|snapshot| {
            let records = &mut snapshot.preferences.pull_request_attachment_records;
            let current: Vec<ThreadPullRequest> = records
                .iter()
                .find(|record| record.thread_id == thread_id)
                .map(|record| {
                    record
                        .pull_requests
                        .iter()
                        .filter_map(from_record)
                        .collect()
                })
                .unwrap_or_default();
            let kept: Vec<ThreadPullRequest> = current
                .into_iter()
                .filter(|existing| {
                    !removed
                        .iter()
                        .any(|gone| gone.identity_key == existing.identity_key)
                })
                .collect();
            changed = mirror_record(records, thread_id, &kept, true);
        });
        if changed {
            self.save_preferences();
        }
    }

    /// The reference's `UMn`: records the thread's Git branch, but only when
    /// the branch belongs to the thread's own checkout (the root of its cwd).
    pub fn update_thread_git_branch(
        self: &Arc<Self>,
        thread_id: ThreadId,
        cwd: PathBuf,
        branch: String,
        branch_root: PathBuf,
    ) {
        let branch = branch.trim().to_owned();
        if branch.is_empty() {
            return;
        }
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let Some(checkout) = read_git_checkout(&cwd) else {
                return;
            };
            if !same_path(&checkout.root.to_string_lossy(), &branch_root) {
                return;
            }
            let receiver = store.backend.update_thread_metadata(
                thread_id,
                ThreadMetadataUpdate {
                    git_branch: AgentOptionalField::Value(branch),
                    ..Default::default()
                },
            );
            if let Ok(Ok(thread)) = receiver.recv_blocking() {
                store.update(|snapshot| {
                    for existing in snapshot
                        .pinned_threads
                        .iter_mut()
                        .chain(snapshot.recent_threads.iter_mut())
                        .chain(snapshot.archived_threads.iter_mut())
                    {
                        if existing.thread_id == thread.thread_id {
                            existing.git = thread.git.clone();
                        }
                    }
                });
            }
        });
    }

    /// The reference's `lFn` for actions an agent command performed: a push
    /// makes pull-request lookups stale; every action refreshes its checkout's
    /// Git metadata; a new branch becomes the thread's branch when it is the
    /// thread's own checkout.
    pub fn apply_git_actions(
        self: &Arc<Self>,
        thread_id: ThreadId,
        thread_cwd: PathBuf,
        actions: Vec<crate::pull_requests::detection::GitAction>,
    ) {
        use crate::pull_requests::detection::GitAction;
        if actions
            .iter()
            .any(|action| matches!(action, GitAction::Push { .. }))
        {
            self.invalidate_branch_lookups();
        }
        let mut refreshed = HashSet::new();
        for action in &actions {
            if refreshed.insert(action.cwd().to_owned()) {
                self.refresh_checkout(action.cwd());
            }
        }
        if refreshed.contains(&thread_cwd)
            || actions
                .iter()
                .any(|a| matches!(a, GitAction::CreateBranch { .. }))
        {
            self.refresh_checkout(&thread_cwd);
        }
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            // The last branch per checkout root wins, like the reference's map.
            let mut branches: Vec<(PathBuf, String)> = Vec::new();
            for action in actions {
                let GitAction::CreateBranch { cwd, branch } = action else {
                    continue;
                };
                let Some(checkout) = read_git_checkout(&cwd) else {
                    continue;
                };
                branches.retain(|(root, _)| *root != checkout.root);
                branches.push((checkout.root, branch));
            }
            for (root, branch) in branches {
                store.update_thread_git_branch(thread_id.clone(), thread_cwd.clone(), branch, root);
            }
        });
    }

    /// After a send: the thread's branch follows its checkout (the reference
    /// does this after every accepted message of a local chat).
    pub fn sync_thread_git_branch(self: &Arc<Self>, thread_id: ThreadId, cwd: PathBuf) {
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let Some(checkout) = read_git_checkout(&cwd) else {
                return;
            };
            let Some(branch) = checkout.branch.clone() else {
                return;
            };
            store.update(|snapshot| {
                snapshot
                    .pull_requests
                    .checkouts
                    .insert(cwd.clone(), GitCheckoutLoad::Ready(Some(checkout.clone())));
            });
            store.update_thread_git_branch(thread_id, cwd, branch, checkout.root);
        });
    }
}

#[cfg(feature = "screenshot")]
impl ThreadPullRequestsState {
    /// Nothing is being read or fetched.
    pub fn is_idle(&self) -> bool {
        self.threads
            .values()
            .all(|entry| entry.load != AttachmentLoad::Loading)
            && self
                .statuses
                .values()
                .all(|entry| entry.load != PullRequestStatusLoad::Loading)
            && self
                .branch_lookups
                .values()
                .all(|entry| entry.lookup != BranchLookup::Loading)
            && self
                .checkouts
                .values()
                .all(|load| *load != GitCheckoutLoad::Loading)
    }
}

#[cfg(test)]
impl WorkspaceStore {
    /// A chat whose attachments were just read with this pull request, whose
    /// state was just fetched: nothing is read or fetched again for a while.
    pub(crate) fn seed_thread_pull_request_for_test(
        &self,
        thread_id: &str,
        url: &str,
        live: PullRequestLiveState,
    ) {
        let identity_key = AgentPullRequestRef::parse(url)
            .expect("a pull request url")
            .identity_key();
        if let Ok(mut snapshot) = self.snapshot.lock() {
            let state = &mut snapshot.pull_requests;
            let mut entry = ThreadAttachmentEntry::loading(1);
            entry.load = AttachmentLoad::Loaded;
            entry.generation = Some(1);
            entry.read_at = Some(Instant::now());
            entry.pull_requests = vec![ThreadPullRequest {
                identity_key: identity_key.clone(),
                url: url.to_owned(),
                root: None,
                head_branch: None,
                touched_at: 1,
            }];
            state.threads.insert(thread_id.to_owned(), entry);
            state.statuses.insert(
                identity_key,
                PullRequestStatusEntry {
                    load: PullRequestStatusLoad::Ready(Some(Box::new(live))),
                    fetched_at: Instant::now(),
                },
            );
        }
        self.publish();
    }
}

#[cfg(test)]
#[path = "pull_requests_tests.rs"]
mod tests;

impl WorkspaceSnapshot {
    /// What the sidebar chip of this thread shows: the attached pull request
    /// the reference would pick, or, for an old thread without an attachment
    /// record, the pull request of its branch.
    pub fn thread_pull_request_chip(&self, thread: &ThreadSummary) -> ChipPullRequest {
        let state = &self.pull_requests;
        let completed = WorkspaceStore::completed_threads(self);
        let branch = thread
            .git
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty());
        if state.attachments_authoritative(thread, &completed) {
            return state
                .chip_selection(&thread.thread_id, branch, thread.git.origin_url.as_deref())
                .0;
        }
        let (Some(branch), Some(checkout)) =
            (branch, WorkspaceStore::checkout_for(self, &thread.cwd))
        else {
            return ChipPullRequest::None;
        };
        let Some(BranchLookupEntry {
            lookup: BranchLookup::Ready(Some(found)),
            ..
        }) = state
            .branch_lookups
            .get(&(checkout.root, branch.to_owned()))
        else {
            return ChipPullRequest::None;
        };
        let Some(identity) = identity_of(&found.url) else {
            return ChipPullRequest::None;
        };
        match state.statuses.get(&identity).map(|entry| &entry.load) {
            Some(PullRequestStatusLoad::Ready(Some(live))) => ChipPullRequest::Show {
                url: found.url.clone(),
                state: live.clone(),
            },
            Some(PullRequestStatusLoad::Loading) | None => ChipPullRequest::Pending,
            _ => ChipPullRequest::None,
        }
    }

    /// The thread's attachments entry, when it has been requested.
    pub fn thread_attachments(&self, thread_id: &str) -> Option<&ThreadAttachmentEntry> {
        self.pull_requests.threads.get(thread_id)
    }

    /// The live state of one attached pull request, when read.
    pub fn pull_request_live_state(&self, identity_key: &str) -> Option<&PullRequestLiveState> {
        self.pull_requests
            .statuses
            .get(identity_key)
            .and_then(PullRequestStatusEntry::state)
    }
}
