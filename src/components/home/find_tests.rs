//! Find in chat through real window events against a scripted backend: ⌘F
//! only inside the conversation, the request, paging past the first page,
//! the jump and the local fallback when the server cannot search.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, IntoElement, Render, TestApp, TestAppWindow,
    Window, WindowBounds, WindowOptions, div, point, prelude::*, px, size,
};

use super::HomeView;
use crate::{
    agent::{
        AgentBackend, AgentConnectionEvent, AgentModelCatalog, AgentPermissionProfile,
        AgentThreadOccurrence, AgentThreadOccurrencePage, AgentThreadOccurrenceRequest,
        AgentThreadSearchError, HistoryItemDetail, HistoryTurnStatus, ThreadActivity,
        ThreadHistory, ThreadHistoryItem, ThreadSummary, ThreadTurn,
    },
    theme::ThemeMode,
};

type SearchReply = async_channel::Sender<Result<AgentThreadOccurrencePage, AgentThreadSearchError>>;

#[derive(Default)]
struct Backend {
    searches: Mutex<Vec<(AgentThreadOccurrenceRequest, SearchReply)>>,
}

impl AgentBackend for Backend {
    fn subscribe_connection_events(&self) -> async_channel::Receiver<AgentConnectionEvent> {
        async_channel::unbounded().1
    }
    fn load_model_catalog(&self) -> async_channel::Receiver<Result<AgentModelCatalog, String>> {
        async_channel::bounded(1).1
    }
    fn load_permission_profiles(
        &self,
        _cwd: PathBuf,
    ) -> async_channel::Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        async_channel::bounded(1).1
    }
    fn update_thread_permissions(
        &self,
        _request: crate::agent::AgentThreadPermissionUpdate,
    ) -> async_channel::Receiver<Result<crate::agent::AgentThreadPermissionResult, String>> {
        async_channel::bounded(1).1
    }
    fn run_prompt(&self, _request: crate::agent::AgentRequest) -> crate::agent::AgentRun {
        crate::agent::AgentRun::new(async_channel::unbounded().1, None)
    }
    fn search_thread_occurrences(
        &self,
        request: AgentThreadOccurrenceRequest,
    ) -> async_channel::Receiver<Result<AgentThreadOccurrencePage, AgentThreadSearchError>> {
        let (reply, receiver) = async_channel::bounded(1);
        self.searches.lock().unwrap().push((request, reply));
        receiver
    }
}

fn history() -> ThreadHistory {
    let turn = |index: usize, prompt: &str, answer: &str| ThreadTurn {
        turn_id: format!("turn-{index}"),
        status: HistoryTurnStatus::Completed,
        items_view: HistoryItemDetail::Full,
        items: vec![
            ThreadHistoryItem::UserMessage {
                client_message_id: None,
                images: Vec::new(),
                item_id: format!("user-{index}"),
                text: prompt.into(),
            },
            ThreadHistoryItem::AssistantMessage {
                item_id: format!("assistant-{index}"),
                text: answer.into(),
                phase: Some("final_answer".into()),
            },
        ],
        started_at: Some(1_000),
        completed_at: Some(2_000),
        duration_ms: Some(1_000),
        error: None,
    };
    ThreadHistory {
        thread: ThreadSummary {
            thread_id: "find-thread".to_owned(),
            title: "Find".to_owned(),
            preview: String::new(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            section: None,
            created_at: 1,
            updated_at: 2,
            recency_at: Some(2),
            activity: ThreadActivity::Idle,
            git: Default::default(),
        },
        turns: vec![
            turn(0, "hello", "Hello! I'm here."),
            turn(1, "你好🙂", "No match in this answer"),
            turn(2, "last prompt", "The end."),
        ],
        next_turn_cursor: None,
        backwards_turn_cursor: None,
    }
}

/// A conversation beside another focusable pane, like the file editor.
struct Shell {
    home: Entity<HomeView>,
    pane: FocusHandle,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .child(div().flex_1().child(self.home.clone()))
            .child(
                div()
                    .id("pane")
                    .w(px(200.0))
                    .h_full()
                    .track_focus(&self.pane),
            )
    }
}

struct Harness {
    app: TestApp,
    window: TestAppWindow<Shell>,
    backend: Arc<Backend>,
}

impl Drop for Harness {
    /// Lets every timer run out, so no task outlives the test (as the rail
    /// harness does).
    fn drop(&mut self) {
        // The focused field's input handler holds its view; release it.
        self.window
            .update(|shell, window, cx| window.focus(&shell.pane, cx));
        for _ in 0..4 {
            self.app.advance_clock(Duration::from_secs(1));
            self.app.run_until_parked();
            self.window.draw();
        }
    }
}

impl Harness {
    fn open() -> Self {
        let mut app = TestApp::new();
        app.update(|cx| {
            super::init_find_keyboard(cx);
            // The app's own field binding (main.rs): Enter submits.
            cx.bind_keys([gpui::KeyBinding::new(
                "enter",
                crate::components::prompt_input::Submit,
                Some("PromptInput"),
            )]);
        });
        let backend = Arc::new(Backend::default());
        let source: Arc<dyn AgentBackend> = backend.clone();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(gpui::Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(1400.0), px(900.0)),
                })),
                ..Default::default()
            },
            move |_, cx| Shell {
                home: cx.new(|cx| HomeView::new_with_backend(ThemeMode::Dark, source.clone(), cx)),
                pane: cx.focus_handle(),
            },
        );
        window.update(|shell, window, cx| {
            window.activate_window();
            shell
                .home
                .read(cx)
                .composer_entity()
                .update(cx, |composer, cx| composer.hydrate_history(history(), cx));
        });
        let mut harness = Self {
            app,
            window,
            backend,
        };
        harness.settle();
        harness
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.window.draw();
            self.app.run_until_parked();
        }
    }

    fn focus_composer(&mut self) {
        self.window.update(|shell, window, cx| {
            let focus = shell
                .home
                .read(cx)
                .composer_entity()
                .read(cx)
                .prompt_focus_handle(cx);
            window.focus(&focus, cx);
        });
        self.settle();
    }

    fn home<R>(&self, read: impl FnOnce(&HomeView, &gpui::App) -> R) -> R {
        self.window.read(|shell, cx| read(shell.home.read(cx), cx))
    }

    fn type_query(&mut self, query: &str) {
        self.window.update(|shell, _, cx| {
            let input = shell.home.read(cx).find.input.clone().expect("find field");
            input.update(cx, |input, cx| input.set_text(query.to_owned(), cx));
        });
        self.app.advance_clock(Duration::from_millis(200));
        self.settle();
    }

    fn answer(
        &mut self,
        page: Result<AgentThreadOccurrencePage, AgentThreadSearchError>,
    ) -> AgentThreadOccurrenceRequest {
        let (request, reply) = self.backend.searches.lock().unwrap().remove(0);
        reply.send_blocking(page).unwrap();
        self.settle();
        request
    }
}

fn occurrence(
    turn: usize,
    item: &str,
    snippet: &str,
    range: std::ops::Range<usize>,
) -> AgentThreadOccurrence {
    AgentThreadOccurrence {
        item_id: item.into(),
        turn_id: format!("turn-{turn}"),
        turn_cursor: format!("turn-cursor-{turn}"),
        snippet: snippet.into(),
        snippet_match: range,
    }
}

#[test]
fn cmd_f_opens_the_bar_only_from_the_conversation() {
    let mut harness = Harness::open();
    // Another pane keeps its own ⌘F.
    harness
        .window
        .update(|shell, window, cx| window.focus(&shell.pane, cx));
    harness.window.simulate_keystrokes("cmd-f");
    assert!(!harness.home(|home, _| home.find.state.open));
    harness.focus_composer();
    harness.window.simulate_keystrokes("cmd-f");
    harness.settle();
    assert!(harness.home(|home, _| home.find.state.open));
    harness.window.simulate_keystrokes("escape");
    harness.settle();
    assert!(!harness.home(|home, _| home.find.state.open));
}

#[test]
fn a_search_pages_on_enter_and_marks_the_active_row() {
    let mut harness = Harness::open();
    harness.focus_composer();
    harness.window.simulate_keystrokes("cmd-f");
    harness.settle();
    harness.type_query("Hello");
    let request = harness.answer(Ok(AgentThreadOccurrencePage {
        generation: 1,
        occurrences: vec![
            occurrence(0, "user-0", "hello", 0..5),
            occurrence(0, "assistant-0", "Hello! I'm here.", 0..5),
        ],
        next_cursor: Some("page-2".into()),
    }));
    assert_eq!(
        (
            request.thread_id.as_str(),
            request.search_term.as_str(),
            request.limit,
            request.cursor
        ),
        ("find-thread", "Hello", 250, None)
    );
    let label = crate::i18n::format!("1 / 2+ 个结果" => "1 / 2+ results");
    assert_eq!(
        harness.home(|home, _| home.find.state.count_label()),
        Some(label)
    );
    let first = harness
        .home(|home, _| home.find.target)
        .expect("the user message row");
    harness.window.simulate_keystrokes("enter");
    harness.settle();
    let second = harness
        .home(|home, _| home.find.target)
        .expect("the answer row");
    assert!(second.row > first.row);
    // Past the last loaded match, Enter reads the next page with its cursor.
    harness.window.simulate_keystrokes("enter");
    harness.settle();
    let (request, reply) = harness.backend.searches.lock().unwrap().remove(0);
    assert_eq!(request.cursor.as_deref(), Some("page-2"));
    reply
        .send_blocking(Ok(AgentThreadOccurrencePage {
            generation: 1,
            occurrences: vec![occurrence(2, "user-2", "last prompt", 0..4)],
            next_cursor: None,
        }))
        .unwrap();
    harness.settle();
    assert_eq!(harness.home(|home, _| home.find.state.active), Some(2));
    let third = harness
        .home(|home, _| home.find.target)
        .expect("the third turn's prompt");
    assert!(third.row > second.row);
    // After the last page the next step wraps to the first match.
    harness.window.simulate_keystrokes("enter");
    harness.settle();
    assert_eq!(harness.home(|home, _| home.find.state.active), Some(0));
    assert_eq!(harness.home(|home, _| home.find.target), Some(first));
}

#[test]
fn a_thread_the_server_cannot_search_is_searched_locally() {
    let mut harness = Harness::open();
    harness.focus_composer();
    harness.window.simulate_keystrokes("cmd-f");
    harness.settle();
    harness.type_query("你好");
    harness.answer(Err(AgentThreadSearchError::Unsupported(
        r#"{"code":-32601,"message":"thread/searchOccurrences is not supported yet"}"#.into(),
    )));
    assert!(harness.home(|home, _| home.find.state.local));
    assert_eq!(harness.home(|home, _| home.find.state.matches.len()), 1);
    assert!(harness.home(|home, _| home.find.target.is_some()));
}

#[test]
fn a_match_in_a_turn_not_loaded_rereads_history_once_then_says_so() {
    let mut harness = Harness::open();
    harness.focus_composer();
    harness.window.simulate_keystrokes("cmd-f");
    harness.settle();
    harness.type_query("later");
    harness.answer(Ok(AgentThreadOccurrencePage {
        generation: 1,
        occurrences: vec![occurrence(9, "user-9", "a later prompt", 2..7)],
        next_cursor: None,
    }));
    // The turn is not in the loaded conversation: history is re-read through
    // the ordinary loader (its stale flag), not an index of our own.
    assert!(harness.window.read(|shell, cx| {
        shell
            .home
            .read(cx)
            .composer_entity()
            .read(cx)
            .history_needs_reload()
    }));
    assert!(!harness.home(|home, _| home.find.unreachable));
    // The reload brought the same turns back: the match is reported.
    harness.window.update(|shell, _, cx| {
        let composer = shell.home.read(cx).composer_entity();
        composer.update(cx, |composer, cx| {
            composer.clear_history_stale();
            composer.hydrate_history(history(), cx);
        });
    });
    harness.settle();
    assert!(harness.home(|home, _| home.find.unreachable && home.find.target.is_none()));
}
