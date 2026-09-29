//! `/memories` and the chat memories dialog against a scripted backend.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use gpui::TestApp;

use super::{ComposerView, dialogs::ComposerDialog};
use crate::{
    agent::{
        AgentBackend, AgentConnectionEvent, AgentExperimentalFeature,
        AgentExperimentalFeatureStage, AgentExperimentalFeatures, AgentMemoryPreferences,
        AgentModelCatalog, AgentPermissionProfile, AgentRequest, AgentRun, AgentThreadMemoryMode,
    },
    theme::ThemeMode,
};

type Reply<T> = async_channel::Sender<Result<T, String>>;
/// One `thread/memoryMode/set` call: thread, generation, mode and its reply.
type ModeCall = (String, u64, AgentThreadMemoryMode, Reply<()>);

#[derive(Default)]
struct Backend {
    memories_enabled: bool,
    runs: Mutex<Vec<AgentRequest>>,
    modes: Mutex<Vec<ModeCall>>,
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
        let (reply, receiver) = async_channel::bounded(1);
        reply.send_blocking(Ok(Vec::new())).unwrap();
        receiver
    }
    fn read_config(
        &self,
        cwd: PathBuf,
    ) -> async_channel::Receiver<
        Result<crate::agent::AgentConfigSnapshot, crate::agent::AgentConfigError>,
    > {
        let (reply, receiver) = async_channel::bounded(1);
        reply
            .send_blocking(Ok(crate::agent::AgentConfigSnapshot {
                generation: 0,
                cwd,
                // Configured defaults: generate on, use off.
                effective: serde_json::json!({"memories": {"use_memories": false}}),
                origins: Default::default(),
                layers: None,
                requirements: None,
                value_aliases: Default::default(),
                value_defaults: Default::default(),
                profile_parents: Default::default(),
                required_fields: Default::default(),
            }))
            .unwrap();
        receiver
    }
    fn update_thread_permissions(
        &self,
        _request: crate::agent::AgentThreadPermissionUpdate,
    ) -> async_channel::Receiver<Result<crate::agent::AgentThreadPermissionResult, String>> {
        async_channel::bounded(1).1
    }
    fn run_prompt(&self, request: AgentRequest) -> AgentRun {
        self.runs.lock().unwrap().push(request);
        AgentRun::new(async_channel::unbounded().1, None)
    }
    fn list_experimental_features(
        &self,
        _thread_id: Option<String>,
    ) -> async_channel::Receiver<Result<AgentExperimentalFeatures, String>> {
        let (reply, receiver) = async_channel::bounded(1);
        reply
            .send_blocking(Ok(AgentExperimentalFeatures {
                generation: 0,
                features: vec![AgentExperimentalFeature {
                    name: "memories".into(),
                    stage: AgentExperimentalFeatureStage::Stable,
                    display_name: None,
                    description: None,
                    announcement: None,
                    enabled: self.memories_enabled,
                    default_enabled: false,
                }],
            }))
            .unwrap();
        receiver
    }
    fn set_thread_memory_mode(
        &self,
        thread_id: String,
        generation: u64,
        mode: AgentThreadMemoryMode,
    ) -> async_channel::Receiver<Result<(), String>> {
        let (reply, receiver) = async_channel::bounded(1);
        self.modes
            .lock()
            .unwrap()
            .push((thread_id, generation, mode, reply));
        receiver
    }
}

struct Fixture {
    composer: gpui::Entity<ComposerView>,
    backend: Arc<Backend>,
    app: TestApp,
}

impl Fixture {
    fn new(memories_enabled: bool, thread: Option<&str>) -> Self {
        let mut app = TestApp::new();
        let backend = Arc::new(Backend {
            memories_enabled,
            ..Default::default()
        });
        let source: Arc<dyn AgentBackend> = backend.clone();
        let composer =
            app.new_entity(|cx| ComposerView::new_with_backend(ThemeMode::Dark, source, cx));
        app.update_entity(&composer, |composer, cx| {
            composer.apply_model_catalog(AgentModelCatalog {
                models: vec![crate::agent::AgentModel {
                    id: "gpt-test".into(),
                    model: "gpt-test".into(),
                    display_name: "GPT Test".into(),
                    description: String::new(),
                    supported_reasoning_efforts: vec![crate::agent::AgentReasoningEffort {
                        id: "medium".into(),
                        description: String::new(),
                    }],
                    default_reasoning_effort: "medium".into(),
                    service_tiers: Vec::new(),
                    default_service_tier: None,
                    is_default: true,
                }],
            });
            composer.set_workspace_context(
                PathBuf::from("/tmp/p"),
                None,
                thread.map(Into::into),
                cx,
            );
            composer.ensure_memories_feature(cx);
        });
        app.run_until_parked();
        Self {
            composer,
            backend,
            app,
        }
    }

    fn with<R>(
        &mut self,
        f: impl FnOnce(&mut ComposerView, &mut gpui::Context<ComposerView>) -> R,
    ) -> R {
        self.app.update_entity(&self.composer, f)
    }
}

#[test]
fn memories_is_offered_only_while_the_feature_is_enabled() {
    let mut off = Fixture::new(false, None);
    assert!(!off.with(|composer, _| composer.memories_feature_enabled()));
    let mut on = Fixture::new(true, None);
    assert!(on.with(|composer, _| composer.memories_feature_enabled()));
    on.with(|composer, cx| composer.open_memories_dialog(cx));
    assert_eq!(
        on.with(|composer, _| composer.dialog.clone()),
        Some(ComposerDialog::Memories)
    );
}

#[test]
fn a_new_chat_sends_the_switches_it_was_given() {
    let mut f = Fixture::new(true, None);
    // Untouched switches show the configured defaults and leave them to the
    // server: nothing is sent for them.
    assert_eq!(
        f.with(|composer, _| composer.memory_switches()),
        AgentMemoryPreferences {
            use_memories: false,
            generate_memories: true
        }
    );
    assert_eq!(f.with(|composer, _| composer.new_chat_memory()), None);
    f.with(|composer, cx| composer.toggle_use_memories(cx));
    f.with(|composer, cx| composer.toggle_use_memories(cx));
    f.with(|composer, cx| composer.toggle_generate_memories(cx));
    f.with(|composer, cx| composer.submit_prompt("hello".into(), cx));
    f.app.run_until_parked();
    let runs = f.backend.runs.lock().unwrap();
    assert_eq!(
        runs[0].context.memory,
        Some(AgentMemoryPreferences {
            use_memories: false,
            generate_memories: false
        })
    );
}

#[test]
fn a_started_chat_changes_generation_at_once_and_rolls_back_on_failure() {
    let mut f = Fixture::new(true, Some("main"));
    // Use memories is fixed once the chat exists (configured off here).
    f.with(|composer, cx| composer.toggle_use_memories(cx));
    assert!(!f.with(|composer, _| composer.memory_switches().use_memories));
    f.with(|composer, cx| composer.toggle_generate_memories(cx));
    assert!(!f.with(|composer, _| composer.memory_switches().generate_memories));
    let (thread, _, mode, reply) = f.backend.modes.lock().unwrap().remove(0);
    assert_eq!(
        (thread.as_str(), mode),
        ("main", AgentThreadMemoryMode::Disabled)
    );
    // A second change waits for the first.
    f.with(|composer, cx| composer.toggle_generate_memories(cx));
    assert!(f.backend.modes.lock().unwrap().is_empty());
    reply.send_blocking(Err("no rollout found".into())).unwrap();
    f.app.run_until_parked();
    assert!(f.with(|composer, _| composer.memory_switches().generate_memories));
    assert_eq!(f.with(|composer, _| composer.toasts().len()), 1);
    // A reply arriving after the chat changed is ignored.
    f.with(|composer, cx| composer.toggle_generate_memories(cx));
    let (_, _, _, reply) = f.backend.modes.lock().unwrap().remove(0);
    f.with(|composer, cx| {
        composer.set_workspace_context(PathBuf::from("/tmp/p"), None, Some("other".into()), cx)
    });
    reply.send_blocking(Err("late".into())).unwrap();
    f.app.run_until_parked();
    assert_eq!(f.with(|composer, _| composer.toasts().len()), 1);
}
