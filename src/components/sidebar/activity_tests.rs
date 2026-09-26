//! The activity view driven like a user: real clicks on the bell, rows and
//! menu items, and real connection events through the workspace store.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use async_channel::{Receiver, Sender};
use gpui::{
    Bounds, MouseButton, TestApp, TestAppWindow, WindowBounds, WindowOptions, point, px, size,
};

use super::super::SidebarView;
use crate::{
    agent::{
        AgentBackend, AgentCapabilities, AgentCapability, AgentConnectionEvent, AgentEvent,
        AgentModelCatalog, AgentPermissionProfile, AgentRequest, AgentRun, AgentThreadStatus,
        AgentThreadStatusState, Page, PageRequest, Project, ThreadActivity, ThreadId,
        ThreadListRequest, ThreadSection, ThreadSummary, WorkspaceResult,
    },
    theme::ThemeMode,
    workspace::{
        WorkspaceStore,
        activity::{ActivitySectionKind, Attention},
    },
};

fn response<T: Send + 'static>(value: T) -> Receiver<T> {
    let (sender, receiver) = async_channel::bounded(1);
    let _ = sender.send_blocking(value);
    receiver
}

struct ActivityBackend {
    events: Sender<AgentConnectionEvent>,
    event_receiver: Receiver<AgentConnectionEvent>,
    calls: Mutex<Vec<String>>,
}

impl ActivityBackend {
    fn new() -> Arc<Self> {
        let (events, event_receiver) = async_channel::unbounded();
        Arc::new(Self {
            events,
            event_receiver,
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("call log").clone()
    }

    fn thread(
        id: &str,
        minutes_ago: i64,
        activity: ThreadActivity,
        project: bool,
    ) -> ThreadSummary {
        // Anchored to today's local noon so every chat falls in one day group
        // whenever the test runs.
        let noon =
            crate::workspace::activity::local_day_start_ms(chrono::Utc::now().timestamp_millis())
                / 1_000
                + 12 * 3_600;
        let at = noon - minutes_ago * 60;
        ThreadSummary {
            thread_id: id.to_owned(),
            title: format!("Chat {id}"),
            preview: String::new(),
            cwd: PathBuf::from(if project {
                "/tmp/activity-project"
            } else {
                "/tmp/loose"
            }),
            project_id: project.then(|| "activity-project".to_owned()),
            section: None,
            created_at: at - 60,
            updated_at: at,
            recency_at: Some(at),
            activity,
        }
    }
}

impl AgentBackend for ActivityBackend {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::new([
            AgentCapability::ProjectList,
            AgentCapability::ThreadList,
            AgentCapability::ThreadArchive,
            AgentCapability::ThreadSectionList,
            AgentCapability::ThreadSectionCreate,
            AgentCapability::ThreadSectionMove,
        ])
    }

    fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
        self.event_receiver.clone()
    }

    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
        response(Err("not used".to_owned()))
    }

    fn load_permission_profiles(
        &self,
        _cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        response(Err("not used".to_owned()))
    }

    fn update_thread_permissions(
        &self,
        _request: crate::agent::AgentThreadPermissionUpdate,
    ) -> Receiver<Result<crate::agent::AgentThreadPermissionResult, String>> {
        response(Err("not used".to_owned()))
    }

    fn list_projects(&self, _page: PageRequest) -> Receiver<WorkspaceResult<Page<Project>>> {
        response(Ok(Page::single(vec![Project {
            project_id: "activity-project".to_owned(),
            name: "Activity project".to_owned(),
            roots: vec![PathBuf::from("/tmp/activity-project")],
            created_at: 1,
            updated_at: 2,
            recency_at: Some(3),
            position: 0,
        }])))
    }

    fn list_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
        let threads = if request.archived {
            Vec::new()
        } else {
            vec![
                Self::thread(
                    "running",
                    5,
                    ThreadActivity::Active { flags: Vec::new() },
                    true,
                ),
                Self::thread("idle-project", 30, ThreadActivity::Idle, true),
                Self::thread("idle-loose", 60, ThreadActivity::Idle, false),
            ]
        };
        response(Ok(Page::single(threads)))
    }

    fn list_thread_sections(
        &self,
        _page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSection>>> {
        response(Ok(Page::single(Vec::new())))
    }

    fn archive_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        self.calls
            .lock()
            .expect("call log")
            .push(format!("thread/archive:{thread_id}"));
        response(Ok(()))
    }

    fn run_prompt(&self, _request: AgentRequest) -> AgentRun {
        let (sender, receiver) = async_channel::unbounded::<AgentEvent>();
        drop(sender);
        AgentRun::new(receiver, None)
    }
}

struct Fixture {
    app: TestApp,
    window: TestAppWindow<SidebarView>,
    backend: Arc<ActivityBackend>,
    store: Arc<WorkspaceStore>,
    preferences: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let preferences = std::env::temp_dir()
            .join(format!(
                "gpui-sidebar-activity-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ))
            .join("preferences.json");
        let backend = ActivityBackend::new();
        let store = WorkspaceStore::with_preferences_path(backend.clone(), preferences.clone());
        store.refresh_all();
        let deadline = Instant::now() + Duration::from_secs(3);
        while store.snapshot().loading.recent || store.snapshot().loading.projects {
            assert!(Instant::now() < deadline, "activity fixture did not load");
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut app = TestApp::new();
        let view_store = store.clone();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            move |_, cx| SidebarView::new(ThemeMode::Dark, false, view_store, cx),
        );
        window.update(|sidebar, _, cx| sidebar.set_width(240.0, cx));
        window.draw();
        Self {
            app,
            window,
            backend,
            store,
            preferences,
        }
    }

    /// Lets store updates reach the view and redraws it.
    fn settle(&mut self, until: impl Fn(&SidebarView) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            self.app.run_until_parked();
            self.window.draw();
            if self.window.read(|sidebar, _| until(sidebar)) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the sidebar never reached the expected state"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn click(&mut self, x: f32, y: f32) {
        self.window
            .simulate_click(point(px(x), px(y)), MouseButton::Left);
        self.window.draw();
    }

    fn sections(&self) -> Vec<(String, Vec<(String, Attention)>)> {
        self.window.read(|sidebar, _| {
            let inputs = sidebar.activity_inputs();
            let Some(session) = &sidebar.activity else {
                return Vec::new();
            };
            session
                .layout(&inputs)
                .sections
                .into_iter()
                .map(|section| {
                    let name = match section.kind {
                        ActivitySectionKind::Priority => "priority".to_owned(),
                        ActivitySectionKind::Pinned => "pinned".to_owned(),
                        ActivitySectionKind::Day { .. } => "day".to_owned(),
                    };
                    let threads = section
                        .threads
                        .into_iter()
                        .map(|thread_id| {
                            let attention = inputs
                                .candidates
                                .iter()
                                .find(|candidate| candidate.thread_id == thread_id)
                                .map_or(Attention::Idle, |candidate| candidate.attention);
                            (thread_id, attention)
                        })
                        .collect();
                    (name, threads)
                })
                .collect()
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(parent) = self.preferences.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}

// Geometry at a 240 px sidebar, measured from the reference: the bell is the
// 24 px button at x 204..228, y 50.9..74.9. The fixture's list is too short to
// overflow, so it reserves no scrollbar gutter and the Priority heading's last
// button sits at x 206..230 (y 226..250, a pixel under its half-pixel box):
// the `…`, or `Clear read chats` once that joins it. A Priority row fills
// y 255..309 and the first day heading starts 16 px later.
const BELL: (f32, f32) = (216.0, 63.0);
const OPTIONS: (f32, f32) = (218.0, 238.0);

#[test]
fn the_bell_opens_the_activity_view_and_closes_it_again() {
    let mut fixture = Fixture::new();
    assert!(!fixture.window.read(|sidebar, _| sidebar.activity_is_open()));
    fixture.click(BELL.0, BELL.1);
    assert!(fixture.window.read(|sidebar, _| sidebar.activity_is_open()));
    assert_eq!(
        fixture.sections(),
        vec![
            (
                "priority".to_owned(),
                vec![("running".to_owned(), Attention::Active)]
            ),
            (
                "day".to_owned(),
                vec![
                    ("idle-project".to_owned(), Attention::Idle),
                    ("idle-loose".to_owned(), Attention::Idle)
                ]
            ),
        ]
    );
    fixture.click(BELL.0, BELL.1);
    assert!(!fixture.window.read(|sidebar, _| sidebar.activity_is_open()));
}

#[test]
fn a_row_opens_its_chat_and_the_view_stays_open() {
    let mut fixture = Fixture::new();
    fixture.click(BELL.0, BELL.1);
    // The first day row: Priority (heading 225..255, one row), 16 px, then
    // the day heading at 325 and its first row at 355..409.
    fixture.click(80.0, 380.0);
    fixture.window.read(|sidebar, _| {
        assert_eq!(sidebar.selected_thread_id.as_deref(), Some("idle-project"));
        assert!(sidebar.activity_is_open());
    });
}

#[test]
fn a_finished_turn_stays_in_priority_until_read_chats_are_cleared() {
    let mut fixture = Fixture::new();
    fixture.click(BELL.0, BELL.1);

    // The running chat finishes while another chat is on screen: the store
    // marks it unread and it stays in Priority.
    fixture
        .backend
        .events
        .send_blocking(AgentConnectionEvent::ThreadStatusChanged(
            AgentThreadStatus {
                thread_id: "running".to_owned(),
                state: AgentThreadStatusState::Idle,
            },
        ))
        .unwrap();
    fixture.settle(|sidebar| {
        sidebar
            .snapshot
            .preferences
            .unread_thread_ids
            .contains("running")
    });
    assert_eq!(
        fixture.sections()[0],
        (
            "priority".to_owned(),
            vec![("running".to_owned(), Attention::Unread)]
        )
    );
    assert!(fixture.window.read(|sidebar, _| {
        let inputs = sidebar.activity_inputs();
        sidebar.activity.as_ref().unwrap().needs_attention(&inputs)
    }));

    // `…` → Mark all as read. The menu sits under the button (y 252) with
    // its `Show` label, three options and a separator above the action.
    fixture.click(OPTIONS.0, OPTIONS.1);
    assert!(fixture.window.read(|sidebar, _| sidebar.activity_menu_open));
    fixture.click(250.0, 391.0);
    fixture.settle(|sidebar| sidebar.snapshot.preferences.unread_thread_ids.is_empty());
    assert_eq!(
        fixture.sections()[0],
        (
            "priority".to_owned(),
            vec![("running".to_owned(), Attention::Idle)]
        )
    );

    // A read chat in Priority brings in `Clear read chats`, which takes the
    // `…` button's place at the heading's end.
    fixture.click(OPTIONS.0, OPTIONS.1);
    fixture.settle(|sidebar| {
        let inputs = sidebar.activity_inputs();
        sidebar
            .activity
            .as_ref()
            .unwrap()
            .priority_threads(&inputs)
            .is_empty()
    });
    assert_eq!(
        fixture.sections(),
        vec![
            ("priority".to_owned(), Vec::new()),
            (
                "day".to_owned(),
                vec![
                    ("running".to_owned(), Attention::Idle),
                    ("idle-project".to_owned(), Attention::Idle),
                    ("idle-loose".to_owned(), Attention::Idle)
                ]
            ),
        ]
    );
}

#[test]
fn hiding_the_priority_section_is_a_saved_preference() {
    let mut fixture = Fixture::new();
    fixture.click(BELL.0, BELL.1);
    fixture.click(OPTIONS.0, OPTIONS.1);
    // `Priority section` is the first option under `Show` (y 278.5..307).
    fixture.click(250.0, 292.0);
    fixture.settle(|sidebar| !sidebar.snapshot.preferences.activity.show_priority);
    assert_eq!(
        fixture.sections(),
        vec![(
            "day".to_owned(),
            vec![
                ("running".to_owned(), Attention::Active),
                ("idle-project".to_owned(), Attention::Idle),
                ("idle-loose".to_owned(), Attention::Idle)
            ]
        )]
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&fixture.preferences).unwrap()).unwrap();
    assert_eq!(saved["activity"]["show_priority"], false);
}

#[test]
fn archive_chats_asks_first_then_archives_the_priority_chats() {
    let mut fixture = Fixture::new();
    fixture.click(BELL.0, BELL.1);
    fixture.click(OPTIONS.0, OPTIONS.1);
    // `Archive chats`, the last row.
    fixture.click(250.0, 420.0);
    let confirmation = fixture
        .window
        .read(|sidebar, _| sidebar.activity_archive_confirmation())
        .expect("the confirmation opens");
    assert_eq!(confirmation.thread_ids, vec!["running".to_owned()]);
    assert!(
        confirmation.running,
        "a running chat asks to stop and archive"
    );
    assert!(
        fixture.backend.calls().is_empty(),
        "nothing is archived before confirming"
    );
    fixture
        .window
        .update(|sidebar, _, cx| sidebar.confirm_activity_archive(cx));
    let store = fixture.store.clone();
    fixture.settle(move |sidebar| {
        sidebar.activity_archive_confirmation().is_none()
            && store.snapshot().thread("running").is_none()
    });
    assert_eq!(
        fixture.backend.calls(),
        vec!["thread/archive:running".to_owned()]
    );
    assert_eq!(fixture.sections()[0], ("priority".to_owned(), Vec::new()));
}
