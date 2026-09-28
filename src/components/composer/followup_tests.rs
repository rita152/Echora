//! Composer flows for the follow-up queue, thread goals, server-started turns
//! and approving auto-review denials, against a scripted backend.

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use gpui::TestApp;

use super::{ComposerView, dialogs::ComposerDialog};
use crate::{
    agent::{
        AgentBackend, AgentConnectionEvent, AgentEvent, AgentExternalTurn, AgentInterruptControl,
        AgentInterruptHandle, AgentInterruptOutcome, AgentModel, AgentModelCatalog,
        AgentPermissionProfile, AgentQueueAddRequest, AgentQueueReorderRequest, AgentQueueTarget,
        AgentQueueUpdateRequest, AgentQueuedSubmission, AgentReasoningEffort, AgentRequest,
        AgentRun, AgentServiceTier, AgentThreadGoal, AgentThreadGoalRead, AgentThreadGoalStatus,
        AgentThreadGoalUpdate, AgentThreadQueue, AgentTurnIdentity,
    },
    conversation::ConversationPhase,
    theme::ThemeMode,
    workspace::FollowUpMode,
};

type Reply<T> = async_channel::Sender<Result<T, String>>;

/// Records every call in order and hands each reply channel to the test.
#[derive(Default)]
struct Script {
    log: Vec<String>,
    adds: Vec<(AgentQueueAddRequest, Reply<AgentQueuedSubmission>)>,
    updates: Vec<(AgentQueueUpdateRequest, Reply<AgentQueuedSubmission>)>,
    deletes: Vec<(AgentQueueTarget, Reply<bool>)>,
    starts: Vec<(AgentQueueTarget, Reply<String>)>,
    reorders: Vec<(AgentQueueReorderRequest, Reply<()>)>,
    lists: VecDeque<Reply<AgentThreadQueue>>,
    steers: Vec<(crate::agent::AgentSteerRequest, Reply<()>)>,
    runs: Vec<(AgentRequest, async_channel::Sender<AgentEvent>)>,
    goal_reads: Vec<Reply<AgentThreadGoalRead>>,
    goal_updates: Vec<(AgentThreadGoalUpdate, Reply<AgentThreadGoalRead>)>,
    goal_clears: Vec<Reply<bool>>,
    approvals: Vec<(crate::agent::AgentAutoReviewApproval, Reply<()>)>,
}

struct Backend {
    server_turn: Mutex<Option<(String, AgentRun)>>,
    modes: Mutex<Option<Result<crate::agent::AgentCollaborationModes, String>>>,
    events: async_channel::Receiver<AgentConnectionEvent>,
    publish: async_channel::Sender<AgentConnectionEvent>,
    script: Arc<Mutex<Script>>,
}

fn channel<T>() -> (Reply<T>, async_channel::Receiver<Result<T, String>>) {
    async_channel::bounded(1)
}

impl Backend {
    fn new() -> Arc<Self> {
        let (publish, events) = async_channel::unbounded();
        Arc::new(Self {
            server_turn: Mutex::new(None),
            modes: Mutex::new(None),
            events,
            publish,
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
    fn load_collaboration_modes(
        &self,
    ) -> async_channel::Receiver<Result<crate::agent::AgentCollaborationModes, String>> {
        let (reply, receiver) = channel();
        if let Some(answer) = self.modes.lock().unwrap().take() {
            reply.send_blocking(answer).unwrap();
        }
        receiver
    }
    fn run_prompt(&self, request: AgentRequest) -> AgentRun {
        let (sender, receiver) = async_channel::unbounded();
        let mut script = self.script.lock().unwrap();
        script.log.push(format!("turn/start:{}", request.prompt));
        script.runs.push((request, sender));
        let control: Arc<dyn AgentInterruptControl> = Arc::new(Interrupts(self.script.clone()));
        AgentRun::new(receiver, Some(AgentInterruptHandle::new(control)))
    }
    fn steer_turn(
        &self,
        request: crate::agent::AgentSteerRequest,
    ) -> async_channel::Receiver<Result<(), String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script
            .log
            .push(format!("turn/steer:{}", request.client_message_id));
        script.steers.push((request, reply));
        receiver
    }
    fn list_thread_queue(
        &self,
        _thread_id: String,
    ) -> async_channel::Receiver<Result<AgentThreadQueue, String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script.log.push("queue/list".into());
        script.lists.push_back(reply);
        receiver
    }
    fn add_queued_submission(
        &self,
        request: AgentQueueAddRequest,
    ) -> async_channel::Receiver<Result<AgentQueuedSubmission, String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script.log.push(format!("queue/add:{}", request.prompt));
        script.adds.push((request, reply));
        receiver
    }
    fn update_queued_submission(
        &self,
        request: AgentQueueUpdateRequest,
    ) -> async_channel::Receiver<Result<AgentQueuedSubmission, String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script
            .log
            .push(format!("queue/update:{}", request.queued_submission_id));
        script.updates.push((request, reply));
        receiver
    }
    fn delete_queued_submission(
        &self,
        target: AgentQueueTarget,
    ) -> async_channel::Receiver<Result<bool, String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script.log.push(format!(
            "queue/delete:{}",
            target.queued_submission_id.clone().unwrap_or_default()
        ));
        script.deletes.push((target, reply));
        receiver
    }
    fn reorder_queued_submissions(
        &self,
        request: AgentQueueReorderRequest,
    ) -> async_channel::Receiver<Result<(), String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script.log.push(format!(
            "queue/reorder:{}",
            request.queued_submission_ids.join(",")
        ));
        script.reorders.push((request, reply));
        receiver
    }
    fn start_queued_submission(
        &self,
        target: AgentQueueTarget,
    ) -> async_channel::Receiver<Result<String, String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script.log.push(format!(
            "queue/start:{}",
            target.queued_submission_id.clone().unwrap_or_default()
        ));
        script.starts.push((target, reply));
        receiver
    }
    fn read_thread_goal(
        &self,
        _thread_id: String,
    ) -> async_channel::Receiver<Result<AgentThreadGoalRead, String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script.log.push("goal/get".into());
        script.goal_reads.push(reply);
        receiver
    }
    fn update_thread_goal(
        &self,
        update: AgentThreadGoalUpdate,
    ) -> async_channel::Receiver<Result<AgentThreadGoalRead, String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script.log.push(format!(
            "goal/set:{:?}:{:?}",
            update.status,
            update.objective.as_deref()
        ));
        script.goal_updates.push((update, reply));
        receiver
    }
    fn clear_thread_goal(
        &self,
        _thread_id: String,
        _generation: u64,
    ) -> async_channel::Receiver<Result<bool, String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script.log.push("goal/clear".into());
        script.goal_clears.push(reply);
        receiver
    }
    fn take_server_turn(&self, thread_id: &str) -> Option<(String, AgentRun)> {
        self.script
            .lock()
            .unwrap()
            .log
            .push(format!("take_server_turn:{thread_id}"));
        self.server_turn.lock().unwrap().take()
    }

    fn approve_auto_review_denial(
        &self,
        request: crate::agent::AgentAutoReviewApproval,
    ) -> async_channel::Receiver<Result<(), String>> {
        let (reply, receiver) = channel();
        let mut script = self.script.lock().unwrap();
        script
            .log
            .push(format!("approve:{}", request.review.key.review_id));
        script.approvals.push((request, reply));
        receiver
    }
}

struct Interrupts(Arc<Mutex<Script>>);

impl AgentInterruptControl for Interrupts {
    fn request_interrupt(&self) -> Result<AgentInterruptOutcome, String> {
        self.0.lock().unwrap().log.push("turn/interrupt".into());
        Ok(AgentInterruptOutcome::Requested)
    }
    fn abandon(&self) {}
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
            service_tiers: Vec::<AgentServiceTier>::new(),
            default_service_tier: None,
            is_default: true,
        }],
    }
}

fn submission(id: &str, client: &str) -> AgentQueuedSubmission {
    AgentQueuedSubmission {
        id: id.into(),
        client_message_id: client.into(),
        text: id.into(),
        attachments: Vec::new(),
    }
}

fn goal(status: AgentThreadGoalStatus, updated_at: i64) -> AgentThreadGoal {
    AgentThreadGoal {
        thread_id: "main".into(),
        objective: "Reach done".into(),
        status,
        token_budget: None,
        tokens_used: 0,
        time_used_seconds: 26,
        created_at: 1,
        updated_at,
    }
}

/// Field order is drop order: the entity handle goes before the app.
struct Fixture {
    composer: gpui::Entity<ComposerView>,
    backend: Arc<Backend>,
    app: TestApp,
}

impl Fixture {
    /// A composer on thread `main` whose first queue list and goal read both
    /// answered empty in generation 1.
    fn new(mode: FollowUpMode) -> Self {
        let mut app = TestApp::new();
        let backend = Backend::new();
        let source: Arc<dyn AgentBackend> = backend.clone();
        let composer =
            app.new_entity(|cx| ComposerView::new_with_backend(ThemeMode::Dark, source, cx));
        app.update_entity(&composer, |c, cx| {
            c.apply_model_catalog(catalog());
            c.set_follow_up_mode(mode, cx);
            c.set_workspace_context(PathBuf::from("/tmp/p"), None, Some("main".into()), cx);
        });
        let fixture = Self {
            app,
            backend,
            composer,
        };
        fixture.answer_list(&[]);
        fixture.answer_goal_read(None);
        fixture.app.run_until_parked();
        fixture
    }

    fn answer_list(&self, ids: &[(&str, &str)]) {
        let reply = self
            .backend
            .script
            .lock()
            .unwrap()
            .lists
            .pop_front()
            .expect("a queue list was requested");
        reply
            .send_blocking(Ok(AgentThreadQueue {
                generation: 1,
                thread_id: "main".into(),
                submissions: ids
                    .iter()
                    .map(|(id, client)| submission(id, client))
                    .collect(),
            }))
            .unwrap();
    }

    fn answer_goal_read(&self, goal: Option<AgentThreadGoal>) {
        let reply = self
            .backend
            .script
            .lock()
            .unwrap()
            .goal_reads
            .pop()
            .unwrap();
        reply
            .send_blocking(Ok(AgentThreadGoalRead {
                generation: 1,
                thread_id: "main".into(),
                goal,
            }))
            .unwrap();
    }

    fn settle(&mut self) {
        self.app.run_until_parked();
        self.app
            .advance_clock(crate::conversation::STREAM_UPDATE_INTERVAL);
        self.app.run_until_parked();
    }

    /// Starts a local turn and makes it steerable.
    fn run_turn(&mut self) {
        self.app.update_entity(&self.composer, |c, cx| {
            c.submit_prompt("first".into(), cx);
            c.apply_agent_event_batch(vec![
                AgentEvent::TurnReady(AgentTurnIdentity {
                    generation: 1,
                    thread_id: "main".into(),
                    turn_id: "turn".into(),
                }),
                AgentEvent::Started,
            ]);
        });
    }

    fn with<R>(
        &mut self,
        f: impl FnOnce(&mut ComposerView, &mut gpui::Context<ComposerView>) -> R,
    ) -> R {
        self.app.update_entity(&self.composer, f)
    }
}

#[test]
fn queue_mode_queues_while_running_and_cmd_enter_steers_one_message() {
    let mut f = Fixture::new(FollowUpMode::Queue);
    f.run_turn();
    f.with(|c, cx| c.submit_prompt("second".into(), cx));
    f.with(|c, cx| c.submit_prompt_as("third".into(), true, cx));
    let log = f.backend.log();
    assert!(log.contains(&"queue/add:second".to_owned()), "{log:?}");
    assert!(
        log.iter().any(|entry| entry.starts_with("turn/steer:")),
        "{log:?}"
    );
    let (request, reply) = f.backend.script.lock().unwrap().adds.remove(0);
    assert_eq!(
        (request.generation, request.thread_id.as_str()),
        (1, "main")
    );
    reply
        .send_blocking(Ok(submission("q1", &request.client_message_id)))
        .unwrap();
    f.settle();
    f.with(|c, cx| {
        assert_eq!(c.conversation.queue.order(), ["q1"]);
        assert_eq!(
            c.conversation.queue.drafts[&request.client_message_id].1,
            "second"
        );
        assert!(c.prompt_text(cx).is_empty());
    });
}

#[test]
fn steer_mode_is_the_default_and_a_failed_add_restores_the_input() {
    let mut f = Fixture::new(FollowUpMode::default());
    f.run_turn();
    f.with(|c, cx| c.submit_prompt("steered".into(), cx));
    assert!(f.backend.log().iter().any(|e| e.starts_with("turn/steer:")));
    f.with(|c, cx| c.submit_prompt_as("queued".into(), true, cx));
    let (_, reply) = f.backend.script.lock().unwrap().adds.remove(0);
    reply.send_blocking(Err("queue rejected".into())).unwrap();
    f.settle();
    f.with(|c, cx| {
        assert_eq!(c.prompt_text(cx), "queued");
        assert_eq!(c.submission_error.as_deref(), Some("queue rejected"));
        assert!(c.conversation.queue.is_empty());
    });
}

#[test]
fn send_now_steers_a_running_turn_then_deletes_and_starts_an_idle_one() {
    let mut f = Fixture::new(FollowUpMode::Queue);
    f.run_turn();
    f.with(|c, cx| {
        c.conversation.queue.wanted += 1;
        c.refresh_queue(cx);
    });
    f.answer_list(&[("q1", "c1"), ("q2", "c2")]);
    f.settle();
    f.with(|c, cx| c.send_queued_now("q1", cx));
    f.with(|c, cx| c.send_queued_now("q1", cx));
    let steers = f
        .backend
        .log()
        .iter()
        .filter(|e| e.as_str() == "turn/steer:c1")
        .count();
    assert_eq!(steers, 1, "a busy row ignores a second click");
    let (steer, reply) = f.backend.script.lock().unwrap().steers.remove(0);
    assert_eq!(steer.client_message_id, "c1", "the queued id is reused");
    reply.send_blocking(Ok(())).unwrap();
    f.settle();
    let (target, reply) = f.backend.script.lock().unwrap().deletes.remove(0);
    assert_eq!(target.queued_submission_id.as_deref(), Some("q1"));
    // The server no longer had it: an error, as in the reference.
    reply.send_blocking(Ok(false)).unwrap();
    f.settle();
    f.with(|c, _| {
        let row = c.conversation.queue.row_mut("q1").unwrap();
        assert!(row.error.is_some() && row.operation.is_none());
    });
    // Idle: send now starts the queued submission instead.
    f.with(|c, _| {
        c.conversation
            .apply_agent_event_batch(vec![AgentEvent::Completed]);
        assert_eq!(c.conversation.phase, ConversationPhase::Complete);
    });
    f.with(|c, cx| c.send_queued_now("q2", cx));
    assert!(f.backend.log().contains(&"queue/start:q2".to_owned()));
}

#[test]
fn a_paused_queue_asks_before_sending_and_clear_queue_sends_then_deletes() {
    let mut f = Fixture::new(FollowUpMode::Queue);
    f.run_turn();
    f.with(|c, cx| {
        c.conversation.queue.wanted += 1;
        c.refresh_queue(cx);
    });
    f.answer_list(&[("q1", "c1")]);
    f.settle();
    f.with(|c, cx| {
        c.stop_generation(cx);
        c.conversation
            .apply_agent_event_batch(vec![AgentEvent::Interrupted]);
        assert!(c.queue_paused());
        c.submit_prompt("new".into(), cx);
        assert!(matches!(
            c.dialog,
            Some(ComposerDialog::SendWhilePaused { .. })
        ));
        c.choose_dialog(super::dialogs::DialogChoice::Secondary, cx);
    });
    let log = f.backend.log();
    let start = log.iter().position(|e| e == "turn/start:new").unwrap();
    let delete = log.iter().position(|e| e == "queue/delete:q1").unwrap();
    assert!(start < delete, "{log:?}");
}

#[test]
fn server_started_turns_attach_when_idle_and_wait_for_the_running_one() {
    let mut f = Fixture::new(FollowUpMode::Queue);
    f.run_turn();
    let (sender, receiver) = async_channel::unbounded();
    f.backend
        .publish
        .send_blocking(AgentConnectionEvent::TurnStarted {
            generation: 1,
            thread_id: "main".into(),
            turn_id: "queued".into(),
            run: AgentExternalTurn::new(AgentRun::new(receiver, None)),
        })
        .unwrap();
    f.settle();
    f.with(|c, _| {
        assert_eq!(c.pending_external_turns.len(), 1, "waits for the local run");
        assert_eq!(c.conversation.turn_id.as_deref(), Some("turn"));
    });
    // Another thread's turn is not taken.
    let (_other_sender, other) = async_channel::unbounded();
    let foreign = AgentExternalTurn::new(AgentRun::new(other, None));
    f.backend
        .publish
        .send_blocking(AgentConnectionEvent::TurnStarted {
            generation: 1,
            thread_id: "elsewhere".into(),
            turn_id: "x".into(),
            run: foreign.clone(),
        })
        .unwrap();
    f.settle();
    assert!(foreign.take().is_some());
    // The local run finishes; the server's turn takes over.
    let run = f.backend.script.lock().unwrap().runs[0].1.clone();
    run.send_blocking(AgentEvent::Completed).unwrap();
    f.settle();
    sender
        .send_blocking(AgentEvent::TurnReady(AgentTurnIdentity {
            generation: 1,
            thread_id: "main".into(),
            turn_id: "queued".into(),
        }))
        .unwrap();
    sender
        .send_blocking(AgentEvent::UserMessage {
            item_id: "u".into(),
            client_message_id: Some("c1".into()),
            text: "queued text".into(),
            images: Vec::new(),
        })
        .unwrap();
    f.settle();
    f.with(|c, _| {
        assert!(c.pending_external_turns.is_empty());
        assert_eq!(c.conversation.turn_id.as_deref(), Some("queued"));
        assert_eq!(c.conversation.user_message.as_deref(), Some("queued text"));
        assert_eq!(
            c.conversation.transcript.len(),
            1,
            "the first turn was committed"
        );
    });
    sender.send_blocking(AgentEvent::Completed).unwrap();
    f.settle();
    f.with(|c, _| assert_eq!(c.conversation.phase, ConversationPhase::Complete));
}

#[test]
fn goal_command_sets_an_active_goal_and_a_second_goal_asks_to_replace() {
    let mut f = Fixture::new(FollowUpMode::default());
    f.with(|c, cx| {
        c.submit_prompt("/goal".into(), cx);
        assert!(c.goal_draft, "the chip turns on");
        c.submit_prompt("Reach done".into(), cx);
    });
    let (update, reply) = f.backend.script.lock().unwrap().goal_updates.remove(0);
    assert_eq!(update.status, Some(AgentThreadGoalStatus::Active));
    assert_eq!(update.objective.as_deref(), Some("Reach done"));
    reply
        .send_blocking(Ok(AgentThreadGoalRead {
            generation: 1,
            thread_id: "main".into(),
            goal: Some(goal(AgentThreadGoalStatus::Active, 2)),
        }))
        .unwrap();
    f.settle();
    f.with(|c, cx| {
        assert_eq!(
            c.conversation.goal.status(),
            Some(AgentThreadGoalStatus::Active)
        );
        assert!(!c.goal_draft);
        c.submit_prompt("/goal Another".into(), cx);
        assert!(matches!(c.dialog, Some(ComposerDialog::ReplaceGoal { .. })));
        c.choose_dialog(super::dialogs::DialogChoice::Secondary, cx);
        assert!(c.dialog.is_none());
    });
    assert_eq!(
        f.backend.script.lock().unwrap().goal_updates.len(),
        0,
        "cancel sends nothing"
    );
}

#[test]
fn stopping_with_an_active_goal_pauses_it_before_interrupting() {
    let mut f = Fixture::new(FollowUpMode::default());
    f.run_turn();
    f.backend
        .publish
        .send_blocking(AgentConnectionEvent::ThreadGoalUpdated {
            generation: 1,
            thread_id: "main".into(),
            turn_id: None,
            goal: goal(AgentThreadGoalStatus::Active, 3),
        })
        .unwrap();
    f.settle();
    f.with(|c, cx| c.stop_generation(cx));
    assert!(!f.backend.log().contains(&"turn/interrupt".to_owned()));
    let (update, reply) = f.backend.script.lock().unwrap().goal_updates.remove(0);
    assert_eq!(update.status, Some(AgentThreadGoalStatus::Paused));
    assert_eq!(update.objective, None);
    reply
        .send_blocking(Ok(AgentThreadGoalRead {
            generation: 1,
            thread_id: "main".into(),
            goal: Some(goal(AgentThreadGoalStatus::Paused, 4)),
        }))
        .unwrap();
    f.settle();
    let log = f.backend.log();
    let pause = log
        .iter()
        .position(|e| e.starts_with("goal/set:Some(Paused)"))
        .unwrap();
    let interrupt = log.iter().position(|e| e == "turn/interrupt").unwrap();
    assert!(pause < interrupt, "{log:?}");
}

#[test]
fn a_completed_goal_is_cleared_once_and_every_status_has_a_summary() {
    let mut f = Fixture::new(FollowUpMode::default());
    for (status, updated_at) in [
        (AgentThreadGoalStatus::Active, 1),
        (AgentThreadGoalStatus::Paused, 2),
        (AgentThreadGoalStatus::Blocked, 3),
        (AgentThreadGoalStatus::UsageLimited, 4),
        (AgentThreadGoalStatus::BudgetLimited, 5),
    ] {
        f.backend
            .publish
            .send_blocking(AgentConnectionEvent::ThreadGoalUpdated {
                generation: 1,
                thread_id: "main".into(),
                turn_id: None,
                goal: goal(status, updated_at),
            })
            .unwrap();
        f.settle();
        f.with(|c, _| assert_eq!(c.conversation.goal.status(), Some(status)));
        assert!(!super::goal_render::status_label(status).is_empty());
    }
    for _ in 0..2 {
        f.backend
            .publish
            .send_blocking(AgentConnectionEvent::ThreadGoalUpdated {
                generation: 1,
                thread_id: "main".into(),
                turn_id: Some("turn".into()),
                goal: goal(AgentThreadGoalStatus::Complete, 9),
            })
            .unwrap();
        f.settle();
    }
    assert_eq!(
        f.backend
            .log()
            .iter()
            .filter(|e| e.as_str() == "goal/clear")
            .count(),
        1
    );
    f.backend
        .publish
        .send_blocking(AgentConnectionEvent::ThreadGoalCleared {
            generation: 1,
            thread_id: "main".into(),
        })
        .unwrap();
    f.settle();
    f.with(|c, _| {
        assert!(c.conversation.goal.goal.is_none());
        assert!(!c.tray_visible(), "a completed goal leaves the tray");
    });
}

#[test]
fn queue_invalidation_relists_only_for_this_thread() {
    let mut f = Fixture::new(FollowUpMode::Queue);
    let before = f
        .backend
        .log()
        .iter()
        .filter(|e| *e == "queue/list")
        .count();
    for thread in ["elsewhere", "main"] {
        f.backend
            .publish
            .send_blocking(AgentConnectionEvent::ThreadQueueChanged {
                generation: 1,
                thread_id: thread.into(),
            })
            .unwrap();
    }
    f.settle();
    let after = f
        .backend
        .log()
        .iter()
        .filter(|e| *e == "queue/list")
        .count();
    assert_eq!(after, before + 1);
}

#[test]
fn approving_a_denial_is_single_flight_and_bound_to_the_thread() {
    use crate::agent::{
        AgentAutoApprovalReview, AgentAutoApprovalReviewAction, AgentAutoApprovalReviewKey,
        AgentAutoApprovalReviewStatus,
    };
    let mut f = Fixture::new(FollowUpMode::default());
    f.run_turn();
    let key = AgentAutoApprovalReviewKey {
        thread_id: "main".into(),
        turn_id: "turn".into(),
        review_id: "review".into(),
    };
    let review = AgentAutoApprovalReview {
        key: key.clone(),
        target_item_id: None,
        action: AgentAutoApprovalReviewAction::Command {
            command: "rm -rf /tmp/x".into(),
            cwd: "/tmp".into(),
            source: "shell".into(),
        },
        status: AgentAutoApprovalReviewStatus::Denied,
        rationale: Some("risky".into()),
        risk_level: Some("high".into()),
        user_authorization: Some("low".into()),
        started_at_ms: 1,
        completed_at_ms: Some(2),
        decision_source: Some("agent".into()),
        source: serde_json::json!({"reviewId":"review"}),
    };
    f.with(|c, cx| {
        c.conversation.apply_auto_approval_review(review);
        c.approve_review(key.clone(), cx);
        c.approve_review(key.clone(), cx);
    });
    assert_eq!(
        f.backend
            .log()
            .iter()
            .filter(|e| e.as_str() == "approve:review")
            .count(),
        1
    );
    let (_, reply) = f.backend.script.lock().unwrap().approvals.remove(0);
    reply.send_blocking(Ok(())).unwrap();
    f.settle();
    f.with(|c, cx| {
        assert!(c.conversation.approved_reviews.contains("review"));
        assert_eq!(
            c.toasts()
                .iter()
                .map(|t| (t.kind, t.text.as_str()))
                .collect::<Vec<_>>(),
            [(super::toast::ToastKind::Success, "已记录批准")]
        );
        c.approve_review(key.clone(), cx);
    });
    assert_eq!(
        f.backend
            .log()
            .iter()
            .filter(|e| e.as_str() == "approve:review")
            .count(),
        1,
        "an approved review is never sent again"
    );
}

#[test]
fn plan_is_offered_only_when_the_connection_lists_a_plan_preset() {
    use crate::agent::{
        AgentCollaborationModeKind, AgentCollaborationModePreset, AgentCollaborationModes,
    };
    let mut f = Fixture::new(FollowUpMode::default());
    *f.backend.modes.lock().unwrap() = Some(Ok(AgentCollaborationModes {
        generation: 1,
        presets: vec![AgentCollaborationModePreset {
            name: "Plan".into(),
            mode: AgentCollaborationModeKind::Plan,
            model: None,
            reasoning_effort: Some("medium".into()),
        }],
    }));
    f.with(|c, cx| c.load_collaboration_modes(cx));
    f.settle();
    f.with(|c, _| {
        assert!(c.plan_mode_available());
        c.prompt_context.plan_mode = Some(true);
    });
    *f.backend.modes.lock().unwrap() = Some(Err("boom".into()));
    f.with(|c, cx| c.load_collaboration_modes(cx));
    f.settle();
    f.with(|c, _| {
        assert!(!c.plan_mode_available());
        assert_eq!(
            c.prompt_context.plan_mode,
            Some(false),
            "plan falls back to default"
        );
    });
}

/// The tray's real hit areas: every corner of the icon buttons, the row
/// menu, and dragging a row by more than the activation distance.
#[gpui::test]
fn tray_controls_answer_across_their_whole_hit_area_and_rows_drag_to_reorder(
    cx: &mut gpui::TestAppContext,
) {
    use gpui::{Modifiers, MouseButton, VisualTestContext, point, px, size};
    let backend = Backend::new();
    let source: Arc<dyn AgentBackend> = backend.clone();
    let handle = cx.add_window(|_, cx| ComposerView::new_with_backend(ThemeMode::Dark, source, cx));
    let composer = handle.root(cx).unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|_, cx| {
        composer.update(cx, |c, cx| {
            c.apply_model_catalog(catalog());
            c.set_follow_up_mode(FollowUpMode::Queue, cx);
            c.set_workspace_context(PathBuf::from("/tmp/p"), None, Some("main".into()), cx);
        })
    });
    let reply = backend.script.lock().unwrap().lists.pop_front().unwrap();
    reply
        .send_blocking(Ok(AgentThreadQueue {
            generation: 1,
            thread_id: "main".into(),
            submissions: vec![
                submission("q1", "c1"),
                submission("q2", "c2"),
                submission("q3", "c3"),
            ],
        }))
        .unwrap();
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let corners = |bounds: gpui::Bounds<gpui::Pixels>| {
        let inset = px(1.5);
        [
            point(bounds.left() + inset, bounds.top() + inset),
            point(bounds.right() - inset, bounds.top() + inset),
            point(bounds.left() + inset, bounds.bottom() - inset),
            point(bounds.right() - inset, bounds.bottom() - inset),
        ]
    };
    // Every corner of the delete button reaches the row action; the row is
    // busy after the first click, so the others send nothing.
    let delete = visual
        .debug_bounds("queued-delete-q2")
        .expect("delete button");
    assert_eq!(delete.size, size(px(24.0), px(24.0)));
    for corner in corners(delete) {
        visual.simulate_click(corner, Modifiers::none());
        visual.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert_eq!(
        backend
            .log()
            .iter()
            .filter(|e| e.as_str() == "queue/delete:q2")
            .count(),
        1
    );
    // The row menu opens from its button; its first item edits the row.
    let menu = visual.debug_bounds("queued-menu-q1").expect("menu button");
    visual.simulate_click(corners(menu)[3], Modifiers::none());
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let edit = visual.debug_bounds("queued-menu-edit").expect("menu item");
    visual.simulate_click(edit.center(), Modifiers::none());
    // Editing removes the row from the server queue first, as the reference.
    let reply = {
        let mut script = backend.script.lock().unwrap();
        let index = script
            .deletes
            .iter()
            .position(|(target, _)| target.queued_submission_id.as_deref() == Some("q1"))
            .expect("the edit deletes q1");
        script.deletes.remove(index).1
    };
    reply.send_blocking(Ok(true)).unwrap();
    visual.run_until_parked();
    visual.update(|_, cx| {
        let c = composer.read(cx);
        assert!(!c.conversation.queue.order().contains(&"q1".to_owned()));
        assert_eq!(c.prompt_text(cx), "q1");
        assert_eq!(
            c.queue_edit
                .as_ref()
                .map(|edit| edit.queued_submission_id.as_str()),
            Some("q1")
        );
    });
    // Dragging q3 up past q2 (q1 left for the edit) sends the complete new order.
    visual.update(|_, cx| {
        composer.update(cx, |c, cx| {
            c.queue_edit = None;
            c.clear_prompt(cx);
            c.conversation.queue.finish_row("q2", None);
        })
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let row = visual.debug_bounds("queued-q3").expect("row");
    let start = point(row.left() + px(200.0), row.center().y);
    visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(
        point(start.x, start.y - px(3.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    visual.simulate_mouse_move(
        point(start.x, start.y - px(36.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    visual.simulate_mouse_up(
        point(start.x, start.y - px(36.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert!(
        backend.log().contains(&"queue/reorder:q3,q2".to_owned()),
        "{:?}",
        backend.log()
    );
}

fn history_turn(id: &str, status: crate::agent::HistoryTurnStatus) -> crate::agent::ThreadTurn {
    crate::agent::ThreadTurn {
        turn_id: id.into(),
        status,
        items_view: crate::agent::HistoryItemDetail::Full,
        items: vec![crate::agent::ThreadHistoryItem::UserMessage {
            client_message_id: None,
            images: Vec::new(),
            item_id: format!("user-{id}"),
            text: format!("{id} request"),
        }],
        started_at: Some(1),
        completed_at: None,
        duration_ms: None,
        error: None,
    }
}

#[test]
fn opening_a_chat_claims_its_running_server_turn_and_rebuilds_it_from_the_stream() {
    use crate::agent::HistoryTurnStatus;
    let mut f = Fixture::new(FollowUpMode::Queue);
    let (sender, receiver) = async_channel::unbounded();
    *f.backend.server_turn.lock().unwrap() = Some(("live".into(), AgentRun::new(receiver, None)));
    // The stream replays from the turn's start.
    sender
        .send_blocking(AgentEvent::TurnReady(AgentTurnIdentity {
            generation: 1,
            thread_id: "main".into(),
            turn_id: "live".into(),
        }))
        .unwrap();
    sender
        .send_blocking(AgentEvent::UserMessage {
            item_id: "user-live".into(),
            client_message_id: None,
            text: "live request".into(),
            images: Vec::new(),
        })
        .unwrap();
    let history = crate::agent::ThreadHistory {
        thread: crate::agent::ThreadSummary {
            thread_id: "main".into(),
            title: "main".into(),
            preview: String::new(),
            cwd: PathBuf::from("/tmp/p"),
            project_id: None,
            section: None,
            created_at: 1,
            updated_at: 2,
            recency_at: Some(2),
            activity: crate::agent::ThreadActivity::Idle,
        },
        turns: vec![
            history_turn("old", HistoryTurnStatus::Completed),
            history_turn("live", HistoryTurnStatus::InProgress),
        ],
        next_turn_cursor: None,
        backwards_turn_cursor: None,
    };
    f.with(|c, cx| c.hydrate_history(history, cx));
    f.settle();
    assert!(
        f.backend
            .log()
            .contains(&"take_server_turn:main".to_owned()),
        "{:?}",
        f.backend.log()
    );
    f.with(|c, _| {
        assert!(c.is_running(), "the claimed turn drives the composer");
        assert_eq!(c.conversation.turn_id.as_deref(), Some("live"));
        assert_eq!(c.conversation.user_message.as_deref(), Some("live request"));
        let turns = c
            .conversation
            .transcript
            .iter()
            .map(|turn| turn.turn_id.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(turns, ["old"], "the history copy of the live turn is gone");
    });
    sender.send_blocking(AgentEvent::Completed).unwrap();
    f.settle();
    f.with(|c, _| assert_eq!(c.conversation.phase, ConversationPhase::Complete));
}

#[test]
fn a_chat_that_is_already_running_does_not_claim_a_server_turn() {
    let mut f = Fixture::new(FollowUpMode::Queue);
    f.run_turn();
    let (_sender, receiver) = async_channel::unbounded();
    *f.backend.server_turn.lock().unwrap() = Some(("live".into(), AgentRun::new(receiver, None)));
    let history = crate::agent::ThreadHistory {
        thread: crate::agent::ThreadSummary {
            thread_id: "main".into(),
            title: "main".into(),
            preview: String::new(),
            cwd: PathBuf::from("/tmp/p"),
            project_id: None,
            section: None,
            created_at: 1,
            updated_at: 2,
            recency_at: Some(2),
            activity: crate::agent::ThreadActivity::Idle,
        },
        turns: Vec::new(),
        next_turn_cursor: None,
        backwards_turn_cursor: None,
    };
    f.with(|c, cx| c.hydrate_history(history, cx));
    f.settle();
    assert!(f.backend.server_turn.lock().unwrap().is_some());
}

#[test]
fn goal_achieved_time_uses_the_references_english_units() {
    use super::achieved_duration_label as label;
    assert_eq!(label(0), "0s");
    assert_eq!(label(45), "45s");
    assert_eq!(label(192), "3m 12s");
    assert_eq!(label(300), "5m");
    assert_eq!(label(3900), "1h 5m");
    assert_eq!(label(90_061), "1d 1h 1m 1s");
}

impl Fixture {
    /// A running turn with q1..q3 queued.
    fn with_queue(mode: FollowUpMode) -> Self {
        let mut f = Self::new(mode);
        f.run_turn();
        f.with(|c, cx| {
            c.conversation.queue.wanted += 1;
            c.refresh_queue(cx);
        });
        f.answer_list(&[("q1", "c1"), ("q2", "c2"), ("q3", "c3")]);
        f.settle();
        f
    }

    fn answer_delete(&mut self, id: &str, deleted: bool) {
        let reply = {
            let mut script = self.backend.script.lock().unwrap();
            let index = script
                .deletes
                .iter()
                .position(|(target, _)| target.queued_submission_id.as_deref() == Some(id))
                .expect("a delete for the row");
            script.deletes.remove(index).1
        };
        reply.send_blocking(Ok(deleted)).unwrap();
        self.settle();
    }

    /// Answers the next add with `id` and returns the request.
    fn answer_add(&mut self, id: &str) -> AgentQueueAddRequest {
        let (request, reply) = self.backend.script.lock().unwrap().adds.remove(0);
        reply
            .send_blocking(Ok(AgentQueuedSubmission {
                id: id.into(),
                client_message_id: request.client_message_id.clone(),
                text: request.prompt.clone(),
                attachments: Vec::new(),
            }))
            .unwrap();
        self.settle();
        request
    }
}

#[test]
fn editing_a_queued_message_removes_it_and_resubmitting_queues_it_back_in_place() {
    let mut f = Fixture::with_queue(FollowUpMode::Queue);
    f.with(|c, cx| c.edit_queued("q2", cx));
    assert!(f.backend.log().contains(&"queue/delete:q2".to_owned()));
    f.answer_delete("q2", true);
    f.with(|c, cx| {
        assert_eq!(c.prompt_text(cx), "q2");
        assert_eq!(c.conversation.queue.order(), ["q1", "q3"]);
        c.prompt_editor
            .update(cx, |e, cx| e.set_text_silently("q2 edited", cx));
        c.submit_prompt("q2 edited".into(), cx);
    });
    let request = f.answer_add("q9");
    assert_ne!(
        request.client_message_id, "c2",
        "a resubmitted edit is a new message"
    );
    assert!(
        f.backend
            .log()
            .contains(&"queue/reorder:q1,q9,q3".to_owned()),
        "{:?}",
        f.backend.log()
    );
    f.with(|c, cx| {
        assert!(c.queue_edit.is_none());
        assert!(c.prompt_text(cx).is_empty());
    });
}

#[test]
fn an_edit_resubmitted_on_an_idle_empty_queue_is_sent_as_a_new_turn() {
    let mut f = Fixture::with_queue(FollowUpMode::Queue);
    f.with(|c, cx| {
        c.conversation.queue.remove("q1");
        c.conversation.queue.remove("q3");
        c.edit_queued("q2", cx);
    });
    f.answer_delete("q2", true);
    let run = f.backend.script.lock().unwrap().runs[0].1.clone();
    run.send_blocking(AgentEvent::Completed).unwrap();
    f.settle();
    f.with(|c, cx| c.submit_prompt("q2 now".into(), cx));
    let log = f.backend.log();
    assert!(log.contains(&"turn/start:q2 now".to_owned()), "{log:?}");
    assert!(!log.iter().any(|e| e == "queue/add:q2 now"), "{log:?}");
}

#[test]
fn undo_restores_a_deleted_or_edited_queued_message_at_its_position() {
    let mut f = Fixture::with_queue(FollowUpMode::Queue);
    f.with(|c, cx| c.delete_queued("q1", cx));
    f.answer_delete("q1", true);
    f.with(|c, cx| assert!(c.undo_queue_removal(cx)));
    let request = f.answer_add("q1b");
    assert_eq!(
        request.client_message_id, "c1",
        "the original id comes back"
    );
    assert!(
        f.backend
            .log()
            .contains(&"queue/reorder:q1b,q2,q3".to_owned())
    );
    f.with(|c, cx| {
        assert_eq!(
            c.toasts()
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>(),
            ["已恢复队列中的消息"]
        );
        assert!(!c.undo_queue_removal(cx), "one undo per removal");
        c.edit_queued("q3", cx);
    });
    f.answer_delete("q3", true);
    f.with(|c, cx| {
        assert_eq!(c.prompt_text(cx), "q3");
        assert!(c.undo_queue_removal(cx));
        assert!(c.queue_edit.is_none());
        assert!(c.prompt_text(cx).is_empty(), "the edit's draft is dropped");
    });
    let request = f.answer_add("q3b");
    assert_eq!(request.client_message_id, "c3");
    f.with(|c, _| {
        assert_eq!(c.toasts().last().unwrap().text, "已恢复排队的消息");
    });
}

#[test]
fn an_edit_whose_row_is_listed_again_updates_it_in_place() {
    let mut f = Fixture::with_queue(FollowUpMode::Queue);
    f.with(|c, cx| c.edit_queued("q2", cx));
    f.answer_delete("q2", true);
    // Another client put the row back before the resubmit.
    f.answer_list(&[("q1", "c1"), ("q2", "c2"), ("q3", "c3")]);
    f.settle();
    f.with(|c, cx| c.submit_prompt("q2 edited".into(), cx));
    assert!(
        f.backend.log().contains(&"queue/update:q2".to_owned()),
        "{:?}",
        f.backend.log()
    );
}

#[test]
fn arrow_up_in_an_empty_composer_edits_the_last_queued_message() {
    let mut f = Fixture::with_queue(FollowUpMode::Queue);
    f.with(|c, cx| assert!(c.edit_last_queued(cx)));
    assert!(f.backend.log().contains(&"queue/delete:q3".to_owned()));
    f.answer_delete("q3", true);
    f.with(|c, cx| {
        assert_eq!(c.prompt_text(cx), "q3");
        assert!(!c.edit_last_queued(cx), "not while an edit is open");
    });
}

#[test]
fn open_in_side_chat_moves_the_message_out_of_the_queue_or_puts_it_back() {
    let mut f = Fixture::with_queue(FollowUpMode::Queue);
    let moved = Arc::new(Mutex::new(Vec::new()));
    let seen = moved.clone();
    f.with(|c, cx| {
        cx.subscribe(
            &cx.entity(),
            move |_, _, event: &super::OpenQueuedInSideChat, _| {
                seen.lock().unwrap().push(event.removed.clone());
            },
        )
        .detach();
        c.open_queued_in_side_chat("q2", cx);
    });
    assert!(
        moved.lock().unwrap().is_empty(),
        "only after the delete lands"
    );
    f.answer_delete("q2", true);
    let removed = moved.lock().unwrap().pop().expect("the shell is asked");
    f.with(|c, cx| {
        assert_eq!(c.conversation.queue.order(), ["q1", "q3"]);
        assert!(!c.undo_queue_removal(cx), "moving is not undoable");
        c.restore_queued_from_side_chat(removed, cx);
    });
    let request = f.answer_add("q2b");
    assert_eq!(request.client_message_id, "c2");
    assert!(
        f.backend
            .log()
            .contains(&"queue/reorder:q1,q2b,q3".to_owned())
    );
}

fn denied_review(id: &str) -> crate::agent::AgentAutoApprovalReview {
    use crate::agent::{
        AgentAutoApprovalReview, AgentAutoApprovalReviewAction, AgentAutoApprovalReviewKey,
        AgentAutoApprovalReviewStatus,
    };
    AgentAutoApprovalReview {
        key: AgentAutoApprovalReviewKey {
            thread_id: "main".into(),
            turn_id: "turn".into(),
            review_id: id.into(),
        },
        target_item_id: None,
        action: AgentAutoApprovalReviewAction::Command {
            command: format!("echo {id}"),
            cwd: "/tmp".into(),
            source: "shell".into(),
        },
        status: AgentAutoApprovalReviewStatus::Denied,
        rationale: None,
        risk_level: Some("high".into()),
        user_authorization: Some("low".into()),
        started_at_ms: 1,
        completed_at_ms: Some(if id == "newer" { 5 } else { 2 }),
        decision_source: Some("agent".into()),
        source: serde_json::json!({"reviewId": id}),
    }
}

impl Fixture {
    /// Replaces the composer text as typing would: the caret ends up after
    /// it and the editor reports the change.
    fn type_text(&mut self, text: &str) {
        self.with(|c, cx| {
            c.prompt_editor.update(cx, |e, cx| {
                let len = e.text().len();
                e.replace_range(0..len, text, cx);
            });
        });
        self.settle();
    }

    fn slash_titles(&mut self) -> Vec<String> {
        self.with(|c, cx| {
            c.slash_items(cx)
                .into_iter()
                .map(|item| item.title)
                .collect()
        })
    }
}

#[test]
fn the_slash_menu_lists_available_commands_and_runs_the_selected_one() {
    let mut f = Fixture::new(FollowUpMode::Queue);
    f.type_text("/");
    assert!(f.with(|c, cx| c.slash_menu_open(cx)));
    // Localized title order; Approve and Plan mode are not available.
    assert_eq!(f.slash_titles(), ["压缩", "目标"]);
    f.type_text("/gooo");
    assert_eq!(f.slash_titles(), ["目标"]);
    f.type_text("/zzz");
    assert!(f.slash_titles().is_empty(), "the menu shows No commands");
    f.type_text("/goal now");
    assert!(
        !f.with(|c, cx| c.slash_menu_open(cx)),
        "a space ends the token"
    );
    f.type_text("/go");
    f.with(|c, cx| {
        assert!(c.slash_menu_enter(cx));
        assert!(c.goal_draft, "Goal turns the chip on");
        assert!(c.prompt_text(cx).is_empty(), "the /query text is removed");
        assert!(!c.slash_menu_open(cx));
    });
    // Escape closes the menu until the query changes.
    f.type_text("/co");
    f.with(|c, cx| {
        assert!(c.slash_menu_key("escape", false, cx));
        assert!(!c.slash_menu_open(cx));
    });
    f.type_text("/com");
    assert!(f.with(|c, cx| c.slash_menu_open(cx)));
    // Compact is refused while a turn runs.
    f.run_turn();
    f.type_text("/compact");
    f.with(|c, cx| {
        assert!(c.slash_menu_enter(cx));
        assert_eq!(
            c.toasts().last().map(|t| (t.kind, t.text.clone())),
            Some((
                super::toast::ToastKind::Danger,
                "聊天期间无法使用 Compact".to_owned()
            ))
        );
    });
}

#[test]
fn slash_approve_lists_the_newest_denials_and_closes_once_one_is_recorded() {
    let mut f = Fixture::new(FollowUpMode::Queue);
    f.run_turn();
    f.with(|c, _| {
        c.conversation
            .apply_auto_approval_review(denied_review("older"));
        c.conversation
            .apply_auto_approval_review(denied_review("newer"));
    });
    f.type_text("/");
    assert_eq!(f.slash_titles(), ["压缩", "批准", "目标"]);
    f.type_text("/autoreview");
    f.with(|c, cx| {
        assert!(c.slash_menu_enter(cx));
        let denials = c.denial_items();
        assert_eq!(
            denials.iter().map(|d| d.title.as_str()).collect::<Vec<_>>(),
            ["echo newer", "echo older"]
        );
        assert_eq!(denials[0].detail, "自动审查未提供理由");
        assert!(c.slash_menu_key("down", false, cx));
        assert!(c.slash_menu_enter(cx));
        assert_eq!(c.denial_items()[1].detail, "正在记录批准操作…");
        c.select_denial(0, cx);
    });
    let log = f.backend.log();
    assert_eq!(
        log.iter()
            .filter(|e| e.starts_with("approve:"))
            .collect::<Vec<_>>(),
        ["approve:older"],
        "one approval in flight"
    );
    let (_, reply) = f.backend.script.lock().unwrap().approvals.remove(0);
    reply.send_blocking(Ok(())).unwrap();
    f.settle();
    f.with(|c, cx| {
        assert!(
            !c.slash_menu_open(cx),
            "a recorded approval closes the menu"
        );
        assert!(c.prompt_text(cx).is_empty());
        assert_eq!(c.denial_items().len(), 1);
    });
}

#[test]
fn redo_removes_a_restored_message_again_and_a_new_removal_drops_it() {
    let mut f = Fixture::with_queue(FollowUpMode::Queue);
    f.with(|c, cx| c.delete_queued("q1", cx));
    f.answer_delete("q1", true);
    f.with(|c, cx| assert!(c.undo_queue_removal(cx)));
    f.answer_add("q1b");
    let toasts = f.with(|c, _| c.toasts().len());
    f.with(|c, cx| assert!(c.redo_queue_removal(cx)));
    assert!(f.backend.log().contains(&"queue/delete:q1b".to_owned()));
    f.answer_delete("q1b", true);
    f.with(|c, cx| {
        assert_eq!(c.toasts().len(), toasts, "redo shows no toast");
        assert!(!c.redo_queue_removal(cx), "one redo per undo");
        // The redo is undoable again.
        assert!(c.undo_queue_removal(cx));
    });
    f.answer_add("q1c");
    f.with(|c, cx| c.delete_queued("q2", cx));
    f.with(|c, cx| assert!(!c.redo_queue_removal(cx), "a new delete drops the redo"));
    // Redo of an edit reopens it.
    f.answer_delete("q2", true);
    f.with(|c, cx| c.edit_queued("q3", cx));
    f.answer_delete("q3", true);
    f.with(|c, cx| assert!(c.undo_queue_removal(cx)));
    f.answer_add("q3b");
    f.with(|c, cx| assert!(c.redo_queue_removal(cx)));
    f.answer_delete("q3b", true);
    f.with(|c, cx| {
        assert_eq!(c.prompt_text(cx), "q3");
        assert!(c.queue_edit.is_some());
    });
}

#[test]
fn a_slash_token_after_other_text_opens_the_menu_and_only_the_token_is_removed() {
    let mut f = Fixture::new(FollowUpMode::Queue);
    f.type_text("fix the tests /co");
    // Compact needs an otherwise empty composer.
    assert!(f.slash_titles().is_empty());
    f.type_text("fix the tests /go");
    assert_eq!(f.slash_titles(), ["目标"]);
    f.with(|c, cx| {
        assert!(c.slash_menu_enter(cx));
        assert_eq!(c.prompt_text(cx), "fix the tests ");
        assert!(c.goal_draft);
    });
    // The caret right after `/co`, more text after it on the same line: the
    // composer still holds only one `/…` line, so Compact stays.
    f.type_text(" soon");
    f.with(|c, cx| {
        c.prompt_editor
            .update(cx, |e, cx| e.replace_range(0..0, "/co", cx))
    });
    assert_eq!(
        f.slash_titles(),
        ["压缩"],
        "one /… line still counts as empty"
    );
    f.type_text("and/or");
    assert!(!f.with(|c, cx| c.slash_menu_open(cx)), "not after a letter");
}

#[test]
fn a_long_goal_objective_is_sent_as_a_file_pointer_and_edited_as_its_text() {
    let home = std::env::temp_dir().join(format!("gpui-long-goal-{}", std::process::id()));
    let mut f = Fixture::new(FollowUpMode::Queue);
    f.with(|c, _| c.codex_home = Some(home.clone()));
    let long = "x".repeat(crate::agent::GOAL_OBJECTIVE_LIMIT + 1);
    f.with(|c, cx| c.set_goal(long.clone(), cx));
    f.settle();
    let (update, reply) = f.backend.script.lock().unwrap().goal_updates.remove(0);
    let pointer = update.objective.clone().unwrap();
    assert!(
        pointer.starts_with("Read the Codex goal objective file at "),
        "{pointer}"
    );
    assert!(pointer.contains(&home.join("attachments").display().to_string()));
    let mut saved = goal(AgentThreadGoalStatus::Active, 5);
    saved.objective = pointer.clone();
    reply
        .send_blocking(Ok(AgentThreadGoalRead {
            generation: 1,
            thread_id: "main".into(),
            goal: Some(saved),
        }))
        .unwrap();
    f.settle();
    // The Edit goal tab gets the text, not the pointer.
    let opened = Arc::new(Mutex::new(None));
    let seen = opened.clone();
    f.with(|c, cx| {
        cx.subscribe(
            &cx.entity(),
            move |_, _, event: &super::OpenGoalEditor, _| {
                *seen.lock().unwrap() = Some(event.text.clone());
            },
        )
        .detach();
        c.edit_goal(cx);
        let (_, sync) = c.goal_tab_sync().unwrap();
        assert_eq!(sync.text.as_deref(), Some(long.as_str()));
    });
    assert_eq!(opened.lock().unwrap().as_deref(), Some(long.as_str()));
    // A pointer this client did not write is read back from its file.
    f.with(|c, _| c.goal_texts.clear());
    f.with(|c, cx| c.edit_goal(cx));
    f.settle();
    assert_eq!(opened.lock().unwrap().as_deref(), Some(long.as_str()));
    std::fs::remove_dir_all(home).unwrap();
}
