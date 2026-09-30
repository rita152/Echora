//! The summary panel as rendered: hovering a background process reveals its
//! stop button, a click stops every background terminal once (the button is
//! disabled while the clean runs, and a failure is a toast that is not
//! retried), the row opens the terminal's output, Delete stops from the
//! keyboard, a header folds its section, and a pull request row's menu removes
//! the attachment.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use async_channel::{Receiver, Sender};
use gpui::{AppContext, Bounds, Entity, Modifiers, Pixels, TestAppContext, VisualTestContext};

use super::{SummaryPanel, SummaryPanelEvent};
use crate::{
    agent::{
        AgentAttachmentContent, AgentAttachmentError, AgentAttachmentRemoveRequest, AgentBackend,
        AgentConnectionEvent, AgentModelCatalog, AgentPermissionProfile,
        AgentPullRequestAttachment, AgentPullRequestRef, AgentRequest, AgentRun,
        AgentThreadAttachment, AgentThreadAttachments, CommandExecution, CommandExecutionSource,
        CommandExecutionStatus, ThreadId,
    },
    components::composer::{ComposerView, ToastKind},
    conversation::BackgroundCleanState,
    theme::ThemeMode,
    workspace::WorkspaceStore,
};

const PULL_REQUEST_URL: &str = "https://github.com/openai/codex/pull/7";

#[derive(Default)]
struct Script {
    calls: Vec<String>,
    cleans: Vec<Sender<Result<(), String>>>,
}

struct Backend {
    events: Receiver<AgentConnectionEvent>,
    _publish: Sender<AgentConnectionEvent>,
    attachments: Vec<AgentThreadAttachment>,
    script: Mutex<Script>,
}

impl Backend {
    fn new(attachments: Vec<AgentThreadAttachment>) -> Arc<Self> {
        let (publish, events) = async_channel::unbounded();
        Arc::new(Self {
            events,
            _publish: publish,
            attachments,
            script: Mutex::default(),
        })
    }

    fn calls(&self) -> Vec<String> {
        self.script.lock().unwrap().calls.clone()
    }

    fn clean_count(&self) -> usize {
        self.calls()
            .iter()
            .filter(|call| call.starts_with("clean:"))
            .count()
    }

    fn answer_clean(&self, result: Result<(), String>) {
        let reply = self.script.lock().unwrap().cleans.remove(0);
        reply.send_blocking(result).unwrap();
    }
}

fn answer<T>(value: T) -> Receiver<T> {
    let (sender, receiver) = async_channel::bounded(1);
    sender.send_blocking(value).unwrap();
    receiver
}

impl AgentBackend for Backend {
    fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
        self.events.clone()
    }
    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
        async_channel::bounded(1).1
    }
    fn load_permission_profiles(
        &self,
        _cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        async_channel::bounded(1).1
    }
    fn update_thread_permissions(
        &self,
        _request: crate::agent::AgentThreadPermissionUpdate,
    ) -> Receiver<Result<crate::agent::AgentThreadPermissionResult, String>> {
        async_channel::bounded(1).1
    }
    fn run_prompt(&self, _request: AgentRequest) -> AgentRun {
        AgentRun::new(async_channel::unbounded().1, None)
    }
    fn list_thread_attachments(
        &self,
        thread_id: ThreadId,
        _force: bool,
    ) -> Receiver<Result<AgentThreadAttachments, AgentAttachmentError>> {
        answer(Ok(AgentThreadAttachments {
            generation: 1,
            thread_id,
            attachments: self.attachments.clone(),
        }))
    }
    fn remove_thread_attachment(
        &self,
        request: AgentAttachmentRemoveRequest,
    ) -> Receiver<Result<(), AgentAttachmentError>> {
        self.script.lock().unwrap().calls.push(format!(
            "remove:{}:{}:{}",
            request.thread_id, request.attachment_type, request.identity_key
        ));
        answer(Ok(()))
    }
    fn clean_background_terminals(
        &self,
        thread_id: ThreadId,
        generation: u64,
    ) -> Receiver<Result<(), String>> {
        let (reply, receiver) = async_channel::bounded(1);
        let mut script = self.script.lock().unwrap();
        script.calls.push(format!("clean:{thread_id}:{generation}"));
        script.cleans.push(reply);
        receiver
    }
}

fn running(id: &str, command: &str) -> CommandExecution {
    CommandExecution {
        id: id.into(),
        command: command.into(),
        actions: Vec::new(),
        cwd: "/repo".into(),
        output: format!("{id}-start\r\n"),
        terminal_process_id: Some(format!("pid-{id}")),
        status: CommandExecutionStatus::InProgress,
        exit_code: None,
        source: CommandExecutionSource::UnifiedExecStartup,
        timed_out: false,
    }
}

fn pull_request_attachment() -> AgentThreadAttachment {
    AgentThreadAttachment {
        id: "attachment-7".into(),
        identity_key: AgentPullRequestRef::parse(PULL_REQUEST_URL)
            .unwrap()
            .identity_key(),
        created_at: 1_790_700_007,
        content: AgentAttachmentContent::PullRequest(AgentPullRequestAttachment {
            url: PULL_REQUEST_URL.into(),
            root: None,
            head_branch: None,
        }),
    }
}

fn preferences_path() -> PathBuf {
    static SERIAL: AtomicU64 = AtomicU64::new(1);
    std::env::temp_dir()
        .join(format!(
            "gpui-summary-panel-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ))
        .join("preferences.json")
}

/// Field order is drop order: the window's context goes before the app.
struct Fixture {
    visual: VisualTestContext,
    composer: Entity<ComposerView>,
    backend: Arc<Backend>,
    store: Arc<WorkspaceStore>,
    panel: Entity<SummaryPanel>,
    events: Arc<Mutex<Vec<String>>>,
    path: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.path.parent().unwrap());
    }
}

impl Fixture {
    fn new(cx: &mut TestAppContext, attachments: Vec<AgentThreadAttachment>) -> Self {
        // The store reads attachments and saves preferences on worker threads.
        cx.executor().allow_parking();
        let backend = Backend::new(attachments);
        let source: Arc<dyn AgentBackend> = backend.clone();
        let path = preferences_path();
        let store = WorkspaceStore::with_preferences_path(source.clone(), path.clone());
        let composer_slot = Arc::new(Mutex::new(None));
        let slot = composer_slot.clone();
        let panel_store = store.clone();
        let window = cx.add_window(move |_, cx| {
            let composer = cx.new(|cx| {
                let mut composer = ComposerView::new_with_backend(ThemeMode::Dark, source, cx);
                composer.seed_background_turn_for_test(
                    "t",
                    vec![
                        running("bg1", "npm run dev"),
                        running("bg2", "tail -f server.log"),
                    ],
                );
                composer
            });
            *slot.lock().unwrap() = Some(composer.clone());
            let mut panel = SummaryPanel::new(ThemeMode::Dark, panel_store, cx);
            panel.set_conversation(Some(composer), Some("t".into()), None, cx);
            panel
        });
        let composer = composer_slot.lock().unwrap().take().unwrap();
        let panel = window.root(cx).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorded = events.clone();
        cx.update(|cx| {
            cx.subscribe(&panel, move |_, event: &SummaryPanelEvent, _| {
                let text = match event {
                    SummaryPanelEvent::OpenBackgroundTerminal {
                        thread_id,
                        item_id,
                        output,
                        ..
                    } => format!("terminal:{thread_id}:{item_id}:{}", output.trim()),
                    SummaryPanelEvent::ViewPullRequest(summary) => {
                        format!("view:{}", summary.url)
                    }
                    SummaryPanelEvent::OpenReview => "review".to_owned(),
                };
                recorded.lock().unwrap().push(text);
            })
            .detach();
        });
        let visual = VisualTestContext::from_window(window.into(), cx);
        let mut fixture = Self {
            visual,
            composer,
            backend,
            store,
            panel,
            events,
            path,
        };
        // The attachments are read on a worker thread when the chat is shown.
        fixture.wait_until(|fixture| {
            fixture
                .store
                .snapshot()
                .pull_requests
                .threads
                .get("t")
                .is_some_and(|entry| entry.is_settled())
        });
        fixture.draw();
        fixture
    }

    fn draw(&mut self) {
        self.visual.run_until_parked();
        self.visual.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn wait_until(&mut self, mut done: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.visual.run_until_parked();
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "condition never held");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn bounds(&mut self, selector: &str) -> Option<Bounds<Pixels>> {
        let selector: &'static str = Box::leak(selector.to_owned().into_boxed_str());
        self.visual.debug_bounds(selector)
    }

    fn hover(&mut self, selector: &str) {
        let bounds = self.bounds(selector).expect(selector);
        self.visual
            .simulate_mouse_move(bounds.center(), None, Modifiers::none());
        self.draw();
    }

    fn click(&mut self, selector: &str) {
        let bounds = self.bounds(selector).expect(selector);
        self.visual
            .simulate_mouse_move(bounds.center(), None, Modifiers::none());
        self.visual
            .simulate_click(bounds.center(), Modifiers::none());
        self.draw();
    }

    fn clean_state(&mut self) -> BackgroundCleanState {
        let composer = self.composer.clone();
        self.visual
            .update(|_, cx| composer.read(cx).background_clean_state().clone())
    }

    fn danger_toasts(&mut self) -> usize {
        let composer = self.composer.clone();
        self.visual.update(|_, cx| {
            composer
                .read(cx)
                .toasts()
                .iter()
                .filter(|toast| toast.kind == ToastKind::Danger)
                .count()
        })
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

#[gpui::test]
fn the_stop_button_cleans_once_and_reports_a_failure_without_retrying(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, Vec::new());
    assert!(fixture.bounds("THREAD_SUMMARY_PANEL").is_some());

    // Hovering a row reveals its stop button; pressing it stops every
    // background terminal of the thread with one request.
    fixture.hover("summary-background-bg1-trigger");
    fixture.click("summary-background-stop-bg1");
    assert_eq!(fixture.backend.calls(), ["clean:t:1"]);
    assert_eq!(
        fixture.clean_state(),
        BackgroundCleanState::InFlight {
            clicked_item_id: Some("bg1".into())
        }
    );
    // The pressed row shows the spinner instead of its button, and the other
    // row's button is disabled until the server answers.
    assert!(fixture.bounds("summary-background-stop-bg1").is_none());
    fixture.hover("summary-background-bg2-trigger");
    fixture.click("summary-background-stop-bg2");
    assert_eq!(fixture.backend.clean_count(), 1);
    // No row opened for clicks on the stop buttons.
    assert!(fixture.events().is_empty());

    // A failure is one toast; nothing is retried by itself.
    fixture.backend.answer_clean(Err("boom".into()));
    fixture.draw();
    assert_eq!(fixture.danger_toasts(), 1);
    assert_eq!(fixture.clean_state(), BackgroundCleanState::Idle);
    assert_eq!(fixture.backend.clean_count(), 1);

    // A new click is a new request; its success settles the state, and the
    // commands stay listed until the server completes them.
    fixture.hover("summary-background-bg2-trigger");
    fixture.click("summary-background-stop-bg2");
    assert_eq!(fixture.backend.clean_count(), 2);
    fixture.backend.answer_clean(Ok(()));
    fixture.draw();
    assert_eq!(fixture.clean_state(), BackgroundCleanState::Succeeded);
    assert_eq!(fixture.danger_toasts(), 1);
    assert!(fixture.bounds("summary-background-bg1-trigger").is_some());
}

#[gpui::test]
fn a_row_opens_its_terminal_and_delete_stops_from_the_keyboard(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, Vec::new());
    fixture.click("summary-background-bg2-trigger");
    assert_eq!(fixture.events(), ["terminal:t:bg2:bg2-start"]);
    assert_eq!(fixture.backend.clean_count(), 0);

    // The clicked row keeps keyboard focus, which reveals its stop button
    // like hover; Delete presses it.
    let panel = fixture.panel.clone();
    fixture.visual.simulate_mouse_move(
        gpui::point(gpui::px(1.0), gpui::px(1.0)),
        None,
        Modifiers::none(),
    );
    fixture.draw();
    assert_eq!(
        fixture
            .visual
            .update(|_, cx| panel.read(cx).focused_row.clone()),
        Some("summary-background-bg2".into())
    );
    // Focused by a click, the row shows no focus ring; a key press shows it.
    assert!(!fixture.visual.update(|_, cx| panel.read(cx).focus_ring));
    fixture.visual.simulate_keystrokes("delete");
    assert_eq!(fixture.backend.calls(), ["clean:t:1"]);
    fixture.draw();
    assert!(fixture.visual.update(|_, cx| panel.read(cx).focus_ring));
    // While the clean runs the key does nothing more.
    fixture.draw();
    fixture.visual.simulate_keystrokes("backspace");
    assert_eq!(fixture.backend.clean_count(), 1);
    fixture.backend.answer_clean(Ok(()));
    fixture.draw();
    assert_eq!(fixture.danger_toasts(), 0);
}

#[gpui::test]
fn headers_fold_their_section_and_the_pull_request_menu_removes_it(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, vec![pull_request_attachment()]);
    let row = format!(
        "summary-pr-{}",
        AgentPullRequestRef::parse(PULL_REQUEST_URL)
            .unwrap()
            .identity_key()
    );
    assert!(fixture.bounds(&format!("{row}-trigger")).is_some());

    // Folding a section hides its rows and is remembered.
    fixture.click("summary-section-toggle-background-tasks");
    assert!(fixture.bounds("summary-background-bg1-trigger").is_none());
    fixture.wait_until(|fixture| {
        fixture
            .store
            .snapshot()
            .preferences
            .summary_section_expanded
            .get("background-tasks")
            == Some(&false)
    });
    fixture.draw();
    assert!(fixture.bounds("summary-background-bg1-trigger").is_none());
    fixture.click("summary-section-toggle-background-tasks");
    assert!(fixture.bounds("summary-background-bg1-trigger").is_some());

    // The row's "…" opens its actions; "Remove PR from task" removes the
    // attachment by its reference identity.
    fixture.hover(&format!("{row}-trigger"));
    fixture.click(&format!("{row}-actions"));
    assert!(fixture.events().is_empty());
    assert!(fixture.bounds("summary-pr-remove").is_some());
    fixture.click("summary-pr-remove");
    fixture.wait_until(|fixture| {
        fixture
            .backend
            .calls()
            .iter()
            .any(|call| call.starts_with("remove:"))
    });
    let identity = AgentPullRequestRef::parse(PULL_REQUEST_URL)
        .unwrap()
        .identity_key();
    assert!(
        fixture
            .backend
            .calls()
            .contains(&format!("remove:t:pull_request:{identity}"))
    );
    fixture.draw();
    assert!(fixture.bounds("summary-pr-remove").is_none());
}
