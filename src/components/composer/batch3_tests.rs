//! `/review` (Code review submenu, review turns, delivery) and `!` shell mode
//! against a scripted backend, including the rendered submenu's hit areas.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use gpui::TestApp;

use super::{
    CodeReviewStarted, ComposerView, StartDetachedReview, review::ReviewBranches, toast::ToastKind,
};
use crate::{
    agent::{
        AgentBackend, AgentConnectionEvent, AgentEvent, AgentModel, AgentModelCatalog,
        AgentPermissionProfile, AgentReasoningEffort, AgentRequest, AgentReviewMode,
        AgentReviewRequest, AgentReviewTarget, AgentRun, AgentShellCommandRequest,
        AgentShellCommandStarted, AgentTurnIdentity,
    },
    git_review::{Checkout, Scope},
    theme::ThemeMode,
    workspace::ReviewDelivery,
};

type Reply<T> = async_channel::Sender<Result<T, String>>;

#[derive(Default)]
struct Script {
    log: Vec<String>,
    reviews: Vec<(AgentReviewRequest, async_channel::Sender<AgentEvent>)>,
    shells: Vec<(AgentShellCommandRequest, Reply<AgentShellCommandStarted>)>,
    server_turn: Option<(String, AgentRun)>,
}

struct Backend {
    events: async_channel::Receiver<AgentConnectionEvent>,
    _publish: async_channel::Sender<AgentConnectionEvent>,
    script: Arc<Mutex<Script>>,
}

impl Backend {
    fn new() -> Arc<Self> {
        let (publish, events) = async_channel::unbounded();
        Arc::new(Self {
            events,
            _publish: publish,
            script: Default::default(),
        })
    }

    fn log(&self) -> Vec<String> {
        self.script.lock().unwrap().log.clone()
    }
}

impl AgentBackend for Backend {
    fn subscribe_connection_events(&self) -> async_channel::Receiver<AgentConnectionEvent> {
        self.events.clone()
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
    fn run_prompt(&self, request: AgentRequest) -> AgentRun {
        self.script
            .lock()
            .unwrap()
            .log
            .push(format!("turn/start:{}", request.prompt));
        AgentRun::new(async_channel::unbounded().1, None)
    }
    fn run_review(&self, request: AgentReviewRequest) -> AgentRun {
        let (sender, receiver) = async_channel::unbounded();
        let mut script = self.script.lock().unwrap();
        script
            .log
            .push(format!("review/start:{:?}", request.target));
        script.reviews.push((request, sender));
        AgentRun::new(receiver, None)
    }
    fn run_shell_command(
        &self,
        request: AgentShellCommandRequest,
    ) -> async_channel::Receiver<Result<AgentShellCommandStarted, String>> {
        let (reply, receiver) = async_channel::bounded(1);
        let mut script = self.script.lock().unwrap();
        script
            .log
            .push(format!("thread/shellCommand:{}", request.command));
        script.shells.push((request, reply));
        receiver
    }
    fn take_server_turn(&self, thread_id: &str) -> Option<(String, AgentRun)> {
        let mut script = self.script.lock().unwrap();
        script.log.push(format!("take_server_turn:{thread_id}"));
        script.server_turn.take()
    }
}

fn catalog() -> AgentModelCatalog {
    AgentModelCatalog {
        models: vec![AgentModel {
            id: "gpt-test".into(),
            model: "gpt-test".into(),
            display_name: "GPT Test".into(),
            description: String::new(),
            supported_reasoning_efforts: vec![AgentReasoningEffort {
                id: "medium".into(),
                description: String::new(),
            }],
            default_reasoning_effort: "medium".into(),
            service_tiers: Vec::new(),
            default_service_tier: None,
            is_default: true,
        }],
    }
}

fn prepare(
    composer: &mut ComposerView,
    thread: Option<&str>,
    cx: &mut gpui::Context<ComposerView>,
) {
    composer.apply_model_catalog(catalog());
    composer.set_workspace_context(
        PathBuf::from("/tmp/repo"),
        Some("project".into()),
        thread.map(str::to_owned),
        cx,
    );
    composer.set_checkout(Checkout::Branch("feature".into()), cx);
    composer.permission_catalog_loading = false;
}

/// Field order is drop order: the entity handle goes before the app.
struct Fixture {
    composer: gpui::Entity<ComposerView>,
    backend: Arc<Backend>,
    events: Arc<Mutex<Vec<String>>>,
    app: TestApp,
}

impl Fixture {
    fn new(thread: Option<&str>) -> Self {
        let mut app = TestApp::new();
        let backend = Backend::new();
        let source: Arc<dyn AgentBackend> = backend.clone();
        let composer =
            app.new_entity(|cx| ComposerView::new_with_backend(ThemeMode::Dark, source, cx));
        app.update_entity(&composer, |c, cx| prepare(c, thread, cx));
        let events = Arc::new(Mutex::new(Vec::new()));
        app.update(|cx| {
            let started = events.clone();
            cx.subscribe(&composer, move |_, event: &CodeReviewStarted, _| {
                started
                    .lock()
                    .unwrap()
                    .push(format!("started:{:?}", event.0));
            })
            .detach();
            let detached = events.clone();
            cx.subscribe(&composer, move |_, event: &StartDetachedReview, _| {
                detached
                    .lock()
                    .unwrap()
                    .push(format!("detached:{:?}", event.0));
            })
            .detach();
        });
        app.run_until_parked();
        // The stub never answers the permission catalog; a new chat would
        // otherwise wait for it.
        app.update_entity(&composer, |c, _| {
            c.permission_catalog_loading = false;
            c.permission_catalog_error = None;
        });
        Self {
            composer,
            backend,
            events,
            app,
        }
    }

    fn with<R>(
        &mut self,
        f: impl FnOnce(&mut ComposerView, &mut gpui::Context<ComposerView>) -> R,
    ) -> R {
        self.app.update_entity(&self.composer, f)
    }

    fn type_text(&mut self, text: &str) {
        self.with(|c, cx| {
            c.prompt_editor.update(cx, |e, cx| {
                let len = e.text().len();
                e.replace_range(0..len, text, cx);
            });
        });
        self.settle();
    }

    fn settle(&mut self) {
        self.app.run_until_parked();
        self.app
            .advance_clock(crate::conversation::STREAM_UPDATE_INTERVAL);
        self.app.run_until_parked();
    }

    /// Opens the Code review submenu with these branches.
    fn open_review(&mut self, branches: &[&str]) {
        self.type_text("/review");
        let titles = self.with(|c, cx| {
            c.slash_items(cx)
                .into_iter()
                .map(|item| item.title)
                .collect::<Vec<_>>()
        });
        assert_eq!(titles, ["代码审查"]);
        self.with(|c, cx| assert!(c.slash_menu_enter(cx)));
        self.settle();
        let branches = branches.iter().map(|b| (*b).to_owned()).collect();
        self.with(|c, cx| c.set_review_branches(ReviewBranches::Loaded(branches), cx));
    }

    fn review_titles(&mut self) -> Vec<String> {
        self.with(|c, cx| c.review_rows(cx).iter().map(|row| row.title()).collect())
    }
}

#[test]
fn code_review_needs_a_repository_an_empty_composer_and_no_side_chat() {
    let mut f = Fixture::new(Some("main"));
    f.type_text("/rev");
    let titles = f.with(|c, cx| {
        c.slash_items(cx)
            .into_iter()
            .map(|i| i.title)
            .collect::<Vec<_>>()
    });
    assert_eq!(titles, ["代码审查"]);
    let description = f.with(|c, cx| c.slash_items(cx)[0].description.clone());
    assert_eq!(description, "审查未提交的更改，或与某个分支比较");
    // Other text in the composer hides it, as the reference requires an
    // empty composer.
    f.type_text("fix this /rev");
    assert!(f.with(|c, cx| c.slash_items(cx).is_empty()));
    // Outside a repository there is nothing to review.
    f.with(|c, cx| c.set_checkout(Checkout::NotRepository, cx));
    f.type_text("/rev");
    assert!(f.with(|c, cx| c.slash_items(cx).is_empty()));
}

#[test]
fn the_title_marks_what_the_query_matched_even_when_the_id_ranks_it() {
    crate::i18n::set_language(crate::i18n::Language::English);
    let mut f = Fixture::new(Some("main"));
    f.type_text("/review");
    let item = f.with(|c, cx| c.slash_items(cx).remove(0));
    crate::i18n::set_language(crate::i18n::Language::SimplifiedChinese);
    assert_eq!(item.title, "Code review");
    // "Code" dims and "review" stays bright, as in the reference.
    assert_eq!(item.matched, vec![5..11]);
}

#[test]
fn the_submenu_keeps_a_slash_filters_moves_and_escape_closes_it() {
    let mut f = Fixture::new(Some("main"));
    f.open_review(&["origin/main", "older"]);
    f.with(|c, cx| {
        assert_eq!(c.prompt_text(cx), "/", "the token is replaced by /");
        assert!(c.slash_menu_open(cx));
        assert!(c.slash_menu.as_ref().unwrap().review);
    });
    assert_eq!(
        f.review_titles(),
        ["审查未提交的更改", "origin/main", "older"]
    );
    // The keyboard walks the rows and wraps.
    f.with(|c, cx| {
        assert!(c.slash_menu_key("down", false, cx));
        assert_eq!(c.slash_menu.as_ref().unwrap().highlighted, 1);
        assert!(c.slash_menu_key("up", false, cx));
        assert!(c.slash_menu_key("up", false, cx));
        assert_eq!(c.slash_menu.as_ref().unwrap().highlighted, 2);
    });
    // Typing after the slash filters, and the submenu stays.
    f.type_text("/orig");
    assert_eq!(f.review_titles(), ["origin/main"]);
    assert!(f.with(|c, cx| c.slash_menu.as_ref().unwrap().review && c.slash_menu_open(cx)));
    // Escape closes the whole menu and leaves the slash text.
    f.with(|c, cx| {
        assert!(c.slash_menu_key("escape", false, cx));
        assert!(!c.slash_menu_open(cx));
        assert_eq!(c.prompt_text(cx), "/orig");
    });
    assert!(f.backend.log().is_empty(), "nothing was started");
}

#[test]
fn choosing_a_row_starts_the_review_in_this_chat_and_shows_the_request() {
    let mut f = Fixture::new(Some("main"));
    f.open_review(&["origin/main"]);
    f.with(|c, cx| {
        assert!(c.slash_menu_key("down", false, cx));
        assert!(c.slash_menu_enter(cx));
        assert!(!c.slash_menu_open(cx));
        assert!(c.prompt_text(cx).is_empty(), "the slash is removed");
        assert!(c.is_running());
        assert_eq!(
            c.conversation.user_message.as_deref(),
            Some("请审查 feature 相对 origin/main 的更改")
        );
        assert!(c.conversation.user_message_review);
    });
    let (request, events) = f.backend.script.lock().unwrap().reviews.remove(0);
    assert_eq!(
        request.target,
        AgentReviewTarget::BaseBranch {
            branch: "origin/main".into()
        }
    );
    assert_eq!(request.thread.thread_id.as_deref(), Some("main"));
    assert_eq!(request.thread.cwd, PathBuf::from("/tmp/repo"));
    assert_eq!(
        f.events.lock().unwrap().as_slice(),
        [format!("started:{:?}", Scope::Branch("origin/main".into()))]
    );
    // The server's review prompt item is not shown as the user's message;
    // the result arrives as the agent message.
    for event in [
        AgentEvent::TurnReady(AgentTurnIdentity {
            generation: 1,
            thread_id: "main".into(),
            turn_id: "R".into(),
        }),
        AgentEvent::ReviewModeUpdated(AgentReviewMode {
            id: "e".into(),
            review: "changes against 'origin/main'".into(),
            entered: true,
            completed: true,
        }),
        AgentEvent::UserMessage {
            item_id: "u".into(),
            client_message_id: None,
            text: "Review the code changes against the base branch".into(),
            images: Vec::new(),
        },
        AgentEvent::Completed,
    ] {
        events.send_blocking(event).unwrap();
    }
    f.settle();
    f.with(|c, _| {
        assert!(!c.is_running());
        let turn = c.conversation.transcript.last();
        let (message, review) = turn
            .map(|turn| (turn.user_message.clone(), turn.goal.review_request))
            .unwrap_or_else(|| {
                (
                    c.conversation.user_message.clone().unwrap_or_default(),
                    c.conversation.user_message_review,
                )
            });
        assert_eq!(message, "请审查 feature 相对 origin/main 的更改");
        assert!(review);
        assert!(c.toasts().is_empty());
    });
}

#[test]
fn a_review_that_never_starts_says_so_once() {
    let mut f = Fixture::new(Some("main"));
    f.open_review(&[]);
    f.with(|c, cx| assert!(c.slash_menu_enter(cx)));
    let (request, events) = f.backend.script.lock().unwrap().reviews.remove(0);
    assert_eq!(request.target, AgentReviewTarget::UncommittedChanges);
    assert_eq!(
        f.events.lock().unwrap().as_slice(),
        [format!("started:{:?}", Scope::Uncommitted)]
    );
    events
        .send_blocking(AgentEvent::Failed("review/start 失败".into()))
        .unwrap();
    f.settle();
    f.with(|c, _| {
        assert_eq!(
            c.toasts()
                .iter()
                .map(|toast| (toast.kind, toast.text.clone()))
                .collect::<Vec<_>>(),
            [(ToastKind::Danger, "无法开始代码审查".to_owned())]
        );
    });
}

#[test]
fn detached_delivery_hands_an_existing_chat_s_review_to_the_host() {
    let mut f = Fixture::new(Some("main"));
    f.with(|c, cx| c.set_review_delivery(ReviewDelivery::Detached, cx));
    f.open_review(&["origin/main"]);
    f.with(|c, cx| assert!(c.slash_menu_enter(cx)));
    f.settle();
    assert!(f.backend.script.lock().unwrap().reviews.is_empty());
    assert_eq!(
        f.events.lock().unwrap().as_slice(),
        [format!(
            "detached:{:?}",
            AgentReviewTarget::UncommittedChanges
        )]
    );
    // A new chat has nothing to detach from: it reviews in place, and a
    // review queued before its settings loaded starts once they have.
    let mut draft = Fixture::new(None);
    draft.with(|c, cx| {
        c.set_review_delivery(ReviewDelivery::Detached, cx);
        c.permission_catalog_loading = true;
        c.queue_code_review(AgentReviewTarget::UncommittedChanges, cx);
    });
    assert!(draft.backend.script.lock().unwrap().reviews.is_empty());
    draft.with(|c, cx| {
        c.permission_catalog_loading = false;
        c.permission_catalog_error = None;
        c.start_pending_review(cx);
    });
    let (request, _) = draft.backend.script.lock().unwrap().reviews.remove(0);
    assert_eq!(request.thread.thread_id, None);
    assert_eq!(request.thread.project_id.as_deref(), Some("project"));
}

#[test]
fn a_review_is_refused_while_a_turn_runs() {
    let mut f = Fixture::new(Some("main"));
    f.with(|c, cx| {
        c.submit_prompt("first".into(), cx);
        c.start_code_review(AgentReviewTarget::UncommittedChanges, cx);
        assert_eq!(
            c.toasts().last().map(|toast| toast.text.clone()).as_deref(),
            Some("聊天期间无法开始代码审查")
        );
    });
    assert!(f.backend.script.lock().unwrap().reviews.is_empty());
}

#[test]
fn a_bang_line_runs_in_the_thread_shell_and_a_failure_restores_it() {
    let mut f = Fixture::new(Some("main"));
    f.type_text("!ls | sort");
    assert!(f.with(|c, cx| c.shell_mode(cx)));
    f.with(|c, cx| c.submit_prompt("!ls | sort".into(), cx));
    f.with(|c, cx| assert!(c.prompt_text(cx).is_empty()));
    let (request, reply) = f.backend.script.lock().unwrap().shells.remove(0);
    assert_eq!(request.command, "ls | sort");
    assert_eq!(request.thread.thread_id.as_deref(), Some("main"));
    assert_eq!(request.timeout_ms, None);
    assert!(
        !f.backend
            .log()
            .iter()
            .any(|entry| entry.starts_with("turn/start"))
    );
    reply
        .send_blocking(Err("command must not be empty".into()))
        .unwrap();
    f.settle();
    f.with(|c, cx| {
        assert_eq!(c.prompt_text(cx), "!ls | sort", "the command is restored");
        assert!(c.toasts()[0].text.contains("command must not be empty"));
    });
    // A bare `!` is not sent.
    f.type_text("!");
    f.with(|c, cx| c.submit_prompt("!".into(), cx));
    assert!(f.backend.script.lock().unwrap().shells.is_empty());
}

#[test]
fn a_shell_command_in_a_new_chat_adopts_its_thread_and_claims_the_turn() {
    let mut f = Fixture::new(None);
    f.with(|c, cx| c.submit_prompt("!pwd".into(), cx));
    let (request, reply) = f.backend.script.lock().unwrap().shells.remove(0);
    assert_eq!(request.thread.thread_id, None);
    assert_eq!(request.thread.project_id.as_deref(), Some("project"));
    let (events, receiver) = async_channel::unbounded();
    f.backend.script.lock().unwrap().server_turn =
        Some(("sh".into(), AgentRun::new(receiver, None)));
    reply
        .send_blocking(Ok(AgentShellCommandStarted {
            generation: 1,
            thread_id: "new".into(),
            created_thread: true,
        }))
        .unwrap();
    f.settle();
    assert!(f.backend.log().contains(&"take_server_turn:new".to_owned()));
    f.with(|c, _| {
        assert_eq!(c.conversation.thread_id.as_deref(), Some("new"));
        assert!(c.is_running(), "the command's turn is attached");
    });
    events.send_blocking(AgentEvent::Completed).unwrap();
    f.settle();
    assert!(f.with(|c, _| !c.is_running()));
}

/// The composer as the app lays it out, 736 px wide like the reference's.
struct Harness {
    backend: Arc<Backend>,
}

impl Harness {
    fn open(cx: &mut gpui::TestAppContext) -> (Self, gpui::WindowHandle<ComposerView>) {
        let backend = Backend::new();
        let source: Arc<dyn AgentBackend> = backend.clone();
        let window = cx.add_window(|_, cx| {
            let mut composer = ComposerView::new_with_backend(ThemeMode::Dark, source, cx);
            prepare(&mut composer, Some("main"), cx);
            composer
        });
        (Self { backend }, window)
    }
}

#[gpui::test]
fn the_rendered_submenu_rows_hover_and_click_like_the_reference(cx: &mut gpui::TestAppContext) {
    let (harness, window) = Harness::open(cx);
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.update(|window, cx| {
        window.resize(gpui::size(gpui::px(900.0), gpui::px(700.0)));
        let composer = window.root::<ComposerView>().flatten().unwrap();
        composer.update(cx, |c, cx| {
            c.prompt_editor
                .update(cx, |e, cx| e.replace_range(0..0, "/review", cx));
        });
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        let composer = window.root::<ComposerView>().flatten().unwrap();
        composer.update(cx, |c, cx| {
            assert!(c.slash_menu_enter(cx));
            c.set_review_branches(
                ReviewBranches::Loaded(vec!["origin/main".into(), "older".into()]),
                cx,
            );
        });
        window.draw(cx).clear(cx);
    });
    let uncommitted = visual.debug_bounds("SLASH_REVIEW_UNCOMMITTED").unwrap();
    let branch = visual.debug_bounds("SLASH_REVIEW_BRANCH_2").unwrap();
    // 28.6 px rows with no gap between them, as the reference measures.
    assert!((f32::from(uncommitted.size.height) - 28.57).abs() < 0.5);
    assert!(f32::from(branch.origin.y) > f32::from(uncommitted.origin.y));
    // Hovering the last branch highlights it.
    visual.simulate_mouse_move(branch.center(), None, gpui::Modifiers::default());
    visual.run_until_parked();
    let highlighted = visual.update(|window, cx| {
        let composer = window.root::<ComposerView>().flatten().unwrap();
        composer.read(cx).slash_menu.as_ref().unwrap().highlighted
    });
    assert_eq!(highlighted, 2);
    // Clicking the uncommitted row starts that review.
    visual.simulate_click(uncommitted.center(), gpui::Modifiers::default());
    visual.run_until_parked();
    assert_eq!(
        harness.backend.log(),
        [format!(
            "review/start:{:?}",
            AgentReviewTarget::UncommittedChanges
        )]
    );
}

#[gpui::test]
fn a_branch_list_that_fails_to_load_offers_a_working_retry(cx: &mut gpui::TestAppContext) {
    let (_harness, window) = Harness::open(cx);
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.update(|window, cx| {
        window.resize(gpui::size(gpui::px(900.0), gpui::px(700.0)));
        let composer = window.root::<ComposerView>().flatten().unwrap();
        composer.update(cx, |c, cx| {
            c.prompt_editor
                .update(cx, |e, cx| e.replace_range(0..0, "/review", cx));
        });
    });
    visual.run_until_parked();
    let cycle = visual.update(|window, cx| {
        let composer = window.root::<ComposerView>().flatten().unwrap();
        let cycle = composer.update(cx, |c, cx| {
            assert!(c.slash_menu_enter(cx));
            c.set_review_branches(ReviewBranches::Failed, cx);
            c.review_branches_cycle
        });
        window.draw(cx).clear(cx);
        cycle
    });
    let retry = visual.debug_bounds("SLASH_REVIEW_RETRY").unwrap();
    visual.simulate_click(retry.center(), gpui::Modifiers::default());
    visual.run_until_parked();
    visual.update(|window, cx| {
        let composer = window.root::<ComposerView>().flatten().unwrap();
        let composer = composer.read(cx);
        assert_eq!(
            composer.review_branches_cycle,
            cycle + 1,
            "Retry reads again"
        );
        assert_eq!(composer.review_branches, ReviewBranches::Loading);
    });
}

#[gpui::test]
fn shell_mode_shows_its_sandbox_warning_while_the_line_starts_with_a_bang(
    cx: &mut gpui::TestAppContext,
) {
    let (_harness, window) = Harness::open(cx);
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    let set = |visual: &mut gpui::VisualTestContext, text: &'static str| {
        visual.update(|window, cx| {
            let composer = window.root::<ComposerView>().flatten().unwrap();
            composer.update(cx, |c, cx| {
                c.prompt_editor.update(cx, |e, cx| {
                    let len = e.text().len();
                    e.replace_range(0..len, text, cx);
                });
            });
        });
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear(cx));
    };
    set(&mut visual, "git status");
    assert!(visual.debug_bounds("COMPOSER_SHELL_MODE").is_none());
    set(&mut visual, "!git status");
    assert!(visual.debug_bounds("COMPOSER_SHELL_MODE").is_some());
}
