//! Hooks, experimental features and memory settings against a scripted
//! backend: what each switch writes, the readback and the confirmation.

use serde_json::json;

use super::{
    SettingsView,
    test_backend::{Backend, USER_CONFIG},
};
use crate::{
    agent::{
        AgentBackend, AgentExperimentalFeature, AgentExperimentalFeatureStage,
        AgentExperimentalFeatures, AgentHook, AgentHookEventName, AgentHookHandler,
        AgentHookListEntry, AgentHookSource, AgentHookSourceGroup, AgentHookStateChange,
        AgentHookTrustStatus, AgentHooksSnapshot,
    },
    hooks::{HookSourceSelection, HookWriteFailure},
    theme::ThemeMode,
};

fn hook(key: &str, trust: AgentHookTrustStatus) -> AgentHook {
    AgentHook {
        key: format!("{USER_CONFIG}:{key}"),
        event_name: AgentHookEventName::Stop,
        handler: AgentHookHandler::Command {
            command: "echo".into(),
            is_async: false,
        },
        matcher: None,
        timeout_sec: 600,
        status_message: None,
        source: AgentHookSource::User,
        source_path: USER_CONFIG.into(),
        plugin_id: None,
        display_order: 0,
        enabled: true,
        is_managed: false,
        current_hash: format!("sha256:{key}"),
        trust_status: trust,
        additional_context_limit: None,
    }
}

fn features(memories: bool) -> AgentExperimentalFeatures {
    let feature = |name: &str, stage, enabled| AgentExperimentalFeature {
        name: name.into(),
        stage,
        display_name: Some(name.into()),
        description: None,
        announcement: None,
        enabled,
        default_enabled: false,
    };
    AgentExperimentalFeatures {
        generation: 1,
        features: vec![
            feature("network_proxy", AgentExperimentalFeatureStage::Beta, false),
            feature("memories", AgentExperimentalFeatureStage::Stable, memories),
        ],
    }
}

struct Fixture {
    settings: gpui::Entity<SettingsView>,
    backend: std::sync::Arc<Backend>,
    app: gpui::TestApp,
}

impl Fixture {
    fn new(page: &'static str) -> Self {
        let mut app = gpui::TestApp::new();
        let backend = std::sync::Arc::new(Backend::default());
        {
            let mut script = backend.script.lock().unwrap();
            script.config = json!({"model": "gpt-test"});
            script.hooks = Some(AgentHooksSnapshot {
                generation: 1,
                cwds: vec!["/work".into()],
                entries: vec![AgentHookListEntry {
                    cwd: "/work".into(),
                    hooks: vec![
                        hook("stop:0:0", AgentHookTrustStatus::Untrusted),
                        hook("stop:1:0", AgentHookTrustStatus::Modified),
                    ],
                    warnings: vec![],
                    errors: vec![],
                }],
            });
            script.features = Some(features(true));
        }
        let source: std::sync::Arc<dyn AgentBackend> = backend.clone();
        let settings = app.new_entity(|cx| SettingsView::new(ThemeMode::Dark, source, cx));
        app.update_entity(&settings, |settings, cx| {
            settings.set_config_context("/work".into(), cx);
            settings.set_hook_roots(
                vec!["/work".into()],
                vec!["/other".into(), "/work".into()],
                cx,
            );
            settings.select(page, cx);
        });
        app.run_until_parked();
        Self {
            settings,
            backend,
            app,
        }
    }

    fn with<R>(
        &mut self,
        f: impl FnOnce(&mut SettingsView, &mut gpui::Context<SettingsView>) -> R,
    ) -> R {
        self.app.update_entity(&self.settings, f)
    }
}

#[test]
fn trusting_hooks_writes_only_their_state_at_the_user_layer_version() {
    let mut f = Fixture::new("hooks-settings");
    assert_eq!(
        f.with(|s, _| s.hooks.cwds.clone()),
        [std::path::PathBuf::from("/work"), "/other".into()]
    );
    let user = HookSourceSelection::Shared(AgentHookSourceGroup::User);
    f.with(|s, cx| s.open_hook_source(Some(user.clone()), cx));
    let trustable = f.with(|s, _| s.hooks.open_source().unwrap().trustable().len());
    assert_eq!(trustable, 2);
    let changes = f.with(|s, _| {
        s.hooks
            .open_source()
            .unwrap()
            .trustable()
            .iter()
            .map(|hook| AgentHookStateChange {
                key: hook.key.clone(),
                enabled: None,
                trusted_hash: Some(hook.current_hash.clone()),
            })
            .collect::<Vec<_>>()
    });
    f.with(|s, cx| s.write_hook_state(changes, cx));
    f.app.run_until_parked();
    let reads_before = f.backend.script.lock().unwrap().hook_reads;
    let write = f.backend.script.lock().unwrap().answer_write("ok");
    assert_eq!(write.file_path, std::path::PathBuf::from(USER_CONFIG));
    assert_eq!(write.expected_version, "sha256:0");
    assert_eq!(
        write
            .edits
            .iter()
            .map(|edit| edit.key.as_str())
            .collect::<Vec<_>>(),
        [
            format!(r#"hooks.state."{USER_CONFIG}:stop:0:0".trusted_hash"#).as_str(),
            format!(r#"hooks.state."{USER_CONFIG}:stop:1:0".trusted_hash"#).as_str(),
        ]
    );
    f.app.run_until_parked();
    // The readback matched and the list is read again.
    assert_eq!(f.with(|s, _| s.hooks.write.clone().unwrap().failure), None);
    assert!(f.backend.script.lock().unwrap().hook_reads > reads_before);
    // An overridden answer is reported as such.
    let change = AgentHookStateChange {
        key: format!("{USER_CONFIG}:stop:0:0"),
        enabled: Some(false),
        trusted_hash: None,
    };
    f.with(|s, cx| s.write_hook_state(vec![change], cx));
    f.app.run_until_parked();
    let write = f
        .backend
        .script
        .lock()
        .unwrap()
        .answer_write("okOverridden");
    assert_eq!(
        write.expected_version, "sha256:1",
        "the new version after our own write"
    );
    assert_eq!(write.edits[0].value, json!(false));
    f.app.run_until_parked();
    assert_eq!(
        f.with(|s, _| s.hooks.write.clone().unwrap().failure),
        Some(HookWriteFailure::Overridden)
    );
}

#[test]
fn a_feature_switch_writes_its_flag_and_says_a_new_connection_applies_it() {
    let mut f = Fixture::new("agent");
    assert_eq!(f.with(|s, _| s.features.settings_rows().len()), 1);
    let edit = f.with(|s, _| s.features.settings_rows()[0].edit(true));
    f.with(|s, cx| s.write_switch("network_proxy".into(), true, vec![edit], true, cx));
    assert!(f.with(|s, _| s.features.displayed("network_proxy", false)));
    f.app.run_until_parked();
    let write = f.backend.script.lock().unwrap().answer_write("ok");
    assert_eq!(write.edits[0].key, "features.network_proxy");
    assert_eq!(write.edits[0].value, json!(true));
    f.app.run_until_parked();
    assert_eq!(f.with(|s, _| s.features.changed_in_generation), Some(1));
}

#[test]
fn memory_switches_write_their_keys_and_delete_waits_for_confirmation() {
    let mut f = Fixture::new("personalization");
    f.with(|s, cx| {
        s.write_switch(
            "memories.enable".into(),
            false,
            crate::agent::memory_enable_edits(false),
            false,
            cx,
        )
    });
    f.app.run_until_parked();
    let write = f.backend.script.lock().unwrap().answer_write("ok");
    assert_eq!(
        write
            .edits
            .iter()
            .map(|edit| (edit.key.as_str(), edit.value.clone()))
            .collect::<Vec<_>>(),
        [
            ("features.memories", json!(false)),
            ("memories.generate_memories", json!(false)),
            ("memories.use_memories", json!(false)),
        ]
    );
    f.app.run_until_parked();
    f.with(|s, cx| {
        s.write_switch(
            "memories.tool_assisted".into(),
            false,
            crate::agent::memory_tool_assisted_edits(false),
            false,
            cx,
        )
    });
    f.app.run_until_parked();
    let write = f.backend.script.lock().unwrap().answer_write("ok");
    assert_eq!(
        write.edits[1].value,
        serde_json::Value::Null,
        "the older key is removed"
    );
    f.app.run_until_parked();
    assert!(f.with(|s, _| s.features.write.clone().unwrap().failure.is_none()));
    // Delete only after the confirmation, once.
    f.with(|s, cx| s.reset_memories(cx));
    assert!(f.backend.script.lock().unwrap().resets.is_empty());
    f.with(|s, _| s.memory_reset = super::memories::MemoryReset::Confirming);
    f.with(|s, cx| s.reset_memories(cx));
    f.with(|s, cx| s.reset_memories(cx));
    assert_eq!(f.backend.script.lock().unwrap().resets.len(), 1);
    let reply = f.backend.script.lock().unwrap().resets.remove(0);
    reply.send_blocking(Ok(())).unwrap();
    f.app.run_until_parked();
    assert_eq!(f.with(|s, _| s.toasts.len()), 1);
    assert_eq!(
        f.with(|s, _| s.memory_reset),
        super::memories::MemoryReset::Idle
    );
}
