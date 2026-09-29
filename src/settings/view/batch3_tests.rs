//! Git → Review delivery, the memory consolidation status and the web search
//! row that provider capabilities gate, against the scripted settings backend.

use serde_json::json;

use super::{SettingsView, test_backend::Backend};
use crate::{
    agent::{AgentBackend, AgentMemoryStatus, AgentProviderCapabilities},
    theme::ThemeMode,
    workspace::ReviewDelivery,
};

struct Fixture {
    settings: gpui::Entity<SettingsView>,
    backend: std::sync::Arc<Backend>,
    app: gpui::TestApp,
}

fn features() -> crate::agent::AgentExperimentalFeatures {
    crate::agent::AgentExperimentalFeatures {
        generation: 1,
        features: vec![crate::agent::AgentExperimentalFeature {
            name: "memories".into(),
            stage: crate::agent::AgentExperimentalFeatureStage::Stable,
            display_name: None,
            description: None,
            announcement: None,
            enabled: true,
            default_enabled: false,
        }],
    }
}

impl Fixture {
    fn new(page: &'static str, setup: impl FnOnce(&mut super::test_backend::Script)) -> Self {
        let mut app = gpui::TestApp::new();
        let backend = std::sync::Arc::new(Backend::default());
        {
            let mut script = backend.script.lock().unwrap();
            script.config = json!({"model": "gpt-test", "web_search": "live"});
            script.features = Some(features());
            setup(&mut script);
        }
        let source: std::sync::Arc<dyn AgentBackend> = backend.clone();
        let settings = app.new_entity(|cx| SettingsView::new(ThemeMode::Dark, source, cx));
        app.update_entity(&settings, |settings, cx| {
            settings.set_config_context("/work".into(), cx);
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
fn review_delivery_is_a_real_choice_that_reports_each_change_once() {
    let mut f = Fixture::new("git-settings", |_| {});
    let changes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = changes.clone();
    f.app.update(|cx| {
        cx.subscribe(
            &f.settings,
            move |_, event: &super::ChangeReviewDelivery, _| {
                seen.lock().unwrap().push(event.0);
            },
        )
        .detach();
    });
    assert_eq!(f.with(|s, _| s.review_delivery), ReviewDelivery::Inline);
    f.with(|s, cx| s.choose_review_delivery(ReviewDelivery::Detached, cx));
    f.with(|s, cx| s.choose_review_delivery(ReviewDelivery::Detached, cx));
    f.app.run_until_parked();
    assert_eq!(
        changes.lock().unwrap().as_slice(),
        [ReviewDelivery::Detached]
    );
    // The host's own value is applied without echoing a change back.
    f.with(|s, cx| s.set_review_delivery(ReviewDelivery::Inline, cx));
    f.app.run_until_parked();
    assert_eq!(changes.lock().unwrap().len(), 1);
    assert_eq!(f.with(|s, _| s.review_delivery), ReviewDelivery::Inline);
}

#[test]
fn the_memory_page_reads_the_consolidation_status_and_hides_a_failure() {
    let status = AgentMemoryStatus {
        generation: 1,
        v2_ready: false,
        consolidated_threads: 3,
        required_threads: 0,
    };
    let mut f = Fixture::new("personalization", |script| {
        script.memory_status = Some(status)
    });
    assert_eq!(f.backend.script.lock().unwrap().memory_status_reads, 1);
    let read = f.with(|s, _| s.memory_status).unwrap();
    assert_eq!(
        read.required_threads,
        crate::agent::MEMORY_V2_REQUIRED_THREADS
    );
    assert_eq!(
        crate::conversation::memory_status_line(read),
        "尚未就绪 · 已整合 3/20 个聊天"
    );
    assert_eq!(
        crate::conversation::memory_status_line(AgentMemoryStatus {
            v2_ready: true,
            consolidated_threads: 25,
            ..read
        }),
        "已就绪 · 已整合 25 个聊天"
    );
    // A failed read shows nothing, and other pages do not read it.
    let mut failed = Fixture::new("personalization", |_| {});
    assert_eq!(failed.with(|s, _| s.memory_status), None);
    let other = Fixture::new("git-settings", |_| {});
    assert_eq!(other.backend.script.lock().unwrap().memory_status_reads, 0);
}

#[test]
fn a_provider_without_web_search_gates_every_option_but_off() {
    let unsupported = AgentProviderCapabilities {
        generation: 1,
        image_generation: true,
        web_search: false,
        namespace_tools: true,
    };
    let mut f = Fixture::new("agent", |script| script.capabilities = Some(unsupported));
    assert!(f.backend.script.lock().unwrap().capability_reads >= 1);
    let gated = f.with(|s, _| {
        ["live", "cached", "indexed", "disabled"].map(|value| {
            s.provider_unsupported_reason("web_search", &json!(value))
                .is_some()
        })
    });
    assert_eq!(gated, [true, true, true, false]);
    assert_eq!(
        f.with(|s, _| s.provider_unsupported_reason("web_search", &serde_json::Value::Null)),
        None,
        "inheriting stays possible"
    );
    assert_eq!(
        f.with(|s, _| s.provider_unsupported_reason("sandbox_mode", &json!("read-only"))),
        None
    );
    // Unknown capabilities gate nothing.
    let mut unknown = Fixture::new("agent", |_| {});
    assert_eq!(
        unknown.with(|s, _| s.provider_unsupported_reason("web_search", &json!("live"))),
        None
    );
}
