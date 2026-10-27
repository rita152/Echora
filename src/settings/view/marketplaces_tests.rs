//! Settings marketplace visibility, hit areas and confirmed protocol actions.
use super::{
    PluginSegment, SettingsView, plugins_catalog::PluginConfirmation, test_backend::snapshot,
};
use crate::{agent::*, theme::ThemeMode};
use gpui::{TestAppContext, VisualTestContext, point, px, size};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Script {
    config: Value,
    config_reads: usize,
    searches: usize,
    upgrades: Vec<AgentMarketplaceUpgradeRequest>,
    removes: Vec<AgentMarketplaceRemoveRequest>,
    adds: Vec<AgentMarketplaceAddRequest>,
    upgrade_replies: Vec<async_channel::Sender<AgentMarketplaceUpgradeResult>>,
    remove_replies: Vec<async_channel::Sender<AgentMarketplaceRemoveResult>>,
}
#[derive(Default)]
struct Backend(Mutex<Script>);
fn catalog() -> AgentPluginCatalog {
    AgentPluginCatalog {
        generation: 1,
        marketplaces: [
            "openai-primary-runtime",
            "OpenAI Bundled",
            "Codex official",
            "local",
            "git",
            "unconfigured",
        ]
        .map(|name| AgentPluginMarketplace {
            name: name.into(),
            path: Some(format!("/fixtures/{name}/marketplace.json")),
            interface: None,
            plugins: vec![],
            extra: Default::default(),
        })
        .to_vec(),
        featured_plugin_ids: vec![],
        marketplace_load_errors: vec![],
        extra: Default::default(),
    }
}
fn ready<T>(value: T) -> async_channel::Receiver<T> {
    let (tx, rx) = async_channel::bounded(1);
    tx.send_blocking(value).ok();
    rx
}
impl AgentBackend for Backend {
    fn subscribe_connection_events(&self) -> async_channel::Receiver<AgentConnectionEvent> {
        async_channel::unbounded().1
    }
    fn load_model_catalog(&self) -> async_channel::Receiver<Result<AgentModelCatalog, String>> {
        async_channel::unbounded().1
    }
    fn load_permission_profiles(
        &self,
        _: PathBuf,
    ) -> async_channel::Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        ready(Ok(vec![]))
    }
    fn update_thread_permissions(
        &self,
        _: AgentThreadPermissionUpdate,
    ) -> async_channel::Receiver<Result<AgentThreadPermissionResult, String>> {
        async_channel::unbounded().1
    }
    fn run_prompt(&self, _: AgentRequest) -> AgentRun {
        AgentRun::new(async_channel::unbounded().1, None)
    }
    fn read_config(
        &self,
        _: PathBuf,
    ) -> async_channel::Receiver<Result<AgentConfigSnapshot, AgentConfigError>> {
        let mut s = self.0.lock().unwrap();
        s.config_reads += 1;
        ready(Ok(snapshot(&s.config, 1)))
    }
    fn load_plugin_catalog(
        &self,
        _: AgentPluginCatalogRequest,
    ) -> async_channel::Receiver<Result<AgentPluginCatalog, AgentPluginsError>> {
        ready(Ok(catalog()))
    }
    fn load_installed_plugins(
        &self,
        _: AgentPluginInstalledRequest,
    ) -> async_channel::Receiver<Result<AgentPluginCatalog, AgentPluginsError>> {
        ready(Ok(catalog()))
    }
    fn search_plugins(
        &self,
        _: AgentPluginSearchRequest,
    ) -> async_channel::Receiver<Result<AgentPluginSearchPage, AgentPluginsError>> {
        self.0.lock().unwrap().searches += 1;
        async_channel::unbounded().1
    }
    fn upgrade_marketplaces(
        &self,
        r: AgentMarketplaceUpgradeRequest,
    ) -> async_channel::Receiver<AgentMarketplaceUpgradeResult> {
        let (tx, rx) = async_channel::unbounded();
        let mut s = self.0.lock().unwrap();
        s.upgrades.push(r);
        s.upgrade_replies.push(tx);
        rx
    }
    fn add_marketplace(
        &self,
        request: AgentMarketplaceAddRequest,
    ) -> async_channel::Receiver<AgentMarketplaceAddResult> {
        self.0.lock().unwrap().adds.push(request);
        async_channel::unbounded().1
    }
    fn remove_marketplace(
        &self,
        r: AgentMarketplaceRemoveRequest,
    ) -> async_channel::Receiver<AgentMarketplaceRemoveResult> {
        let (tx, rx) = async_channel::unbounded();
        let mut s = self.0.lock().unwrap();
        s.removes.push(r);
        s.remove_replies.push(tx);
        rx
    }
}
fn fixture(
    cx: &mut TestAppContext,
) -> (gpui::Entity<SettingsView>, Arc<Backend>, VisualTestContext) {
    let backend = Arc::new(Backend::default());
    backend.0.lock().unwrap().config = json!({"marketplaces": {
        "openai-primary-runtime": {"source_type":"local"}, "OpenAI Bundled": {}, "Codex official": {},
        "local": {"source_type":"local","source":"/fixtures/local"},
        "git": {"source_type":"git","source":"https://example.test/plugins.git"}
    }});
    let source = backend.clone();
    let window = cx.open_window(size(px(1440.), px(900.)), |_, cx| {
        let mut s = SettingsView::new(ThemeMode::Dark, source, cx);
        s.select("plugins-settings", cx);
        s
    });
    let entity = cx.update(|cx| window.root(cx).unwrap());
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    visual.update(|w, cx| w.draw(cx).clear(cx));
    (entity, backend, visual)
}
fn draw(visual: &mut VisualTestContext) {
    visual.run_until_parked();
    visual.update(|w, cx| w.draw(cx).clear(cx));
}
#[gpui::test]
fn marketplaces_are_separate_configured_sources_and_search_locally(cx: &mut TestAppContext) {
    let (entity, backend, mut visual) = fixture(cx);
    assert!(
        visual.debug_bounds("marketplace-row-0").is_none(),
        "Plugins must not render source rows"
    );
    entity.update(&mut visual, |s, cx| {
        assert_eq!(s.configured_marketplace_count(), 2);
        assert_eq!(
            s.configured_marketplaces()
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            ["git", "local"]
        );
        assert!(s.can_upgrade_marketplace("git"));
        assert!(!s.can_upgrade_marketplace("local"));
        for name in [
            "openai-primary-runtime",
            "OpenAI Bundled",
            "Codex official",
            "unconfigured",
            "local",
        ] {
            s.confirm_marketplace_upgrade(name.into(), cx);
            assert!(s.plugins_catalog.confirmation.is_none());
        }
        s.confirm_marketplace_remove("unconfigured".into(), cx);
        assert!(s.plugins_catalog.confirmation.is_none());
        s.select_plugins_segment(PluginSegment::Marketplaces, cx);
    });
    draw(&mut visual);
    let row = visual.debug_bounds("marketplace-row-0").unwrap();
    let update = visual.debug_bounds("marketplace-upgrade-0").unwrap();
    let remove = visual.debug_bounds("marketplace-remove-0").unwrap();
    assert!(row.size.height >= px(76.));
    assert!(update.right() < remove.left());
    // Source actions must work across the whole button, including its edges.
    for x in [update.left() + px(1.), update.right() - px(1.)] {
        visual.simulate_click(point(x, update.center().y), Default::default());
        draw(&mut visual);
        entity.update(&mut visual, |s, cx| {
            assert_eq!(
                s.plugins_catalog.confirmation,
                Some(PluginConfirmation::MarketplaceUpgrade {
                    marketplace_name: Some("git".into())
                })
            );
            s.dismiss_catalog_confirmation(cx);
        });
        draw(&mut visual);
    }
    assert!(
        backend.0.lock().unwrap().upgrades.is_empty(),
        "Cancel must not write"
    );
    let local = visual.debug_bounds("marketplace-upgrade-1").unwrap();
    visual.simulate_click(local.center(), Default::default());
    entity.read_with(&visual, |s, _| {
        assert!(s.plugins_catalog.confirmation.is_none())
    });
    entity.update(&mut visual, |s, cx| {
        s.directory_search_changed("local".into(), cx);
        s.submit_plugin_search(cx);
    });
    draw(&mut visual);
    assert!(visual.debug_bounds("marketplace-row-0").is_none());
    assert!(visual.debug_bounds("marketplace-row-1").is_some());
    assert_eq!(backend.0.lock().unwrap().searches, 0);
    entity.update(&mut visual, |s, cx| {
        s.select_plugins_segment(PluginSegment::Plugins, cx)
    });
    entity.read_with(&visual, |s, _| {
        assert!(s.plugins_catalog.directory.search_term.is_empty())
    });
    // App-only sources do not expose a management tab.
    entity.update(&mut visual,|s,_|s.config_editor.snapshot=Some(snapshot(&json!({"marketplaces":{"OpenAI Bundled":{},"Codex official":{},"openai-primary-runtime":{},"openai-curated":{},"openai-curated-remote":{}}}),1)));
    draw(&mut visual);
    assert!(visual.debug_bounds("plugins-segment-4").is_none());
}
#[gpui::test]
fn marketplaces_confirm_once_block_while_busy_and_refresh_after_remove(cx: &mut TestAppContext) {
    let (entity, backend, mut visual) = fixture(cx);
    entity.update(&mut visual, |s, cx| {
        s.select_plugins_segment(PluginSegment::Marketplaces, cx)
    });
    draw(&mut visual);
    let button = visual.debug_bounds("marketplace-upgrade-0").unwrap();
    visual.simulate_click(button.center(), Default::default());
    draw(&mut visual);
    let confirm = visual.debug_bounds("catalog-confirm").unwrap();
    visual.simulate_click(confirm.center(), Default::default());
    draw(&mut visual);
    assert_eq!(
        backend.0.lock().unwrap().upgrades[0]
            .marketplace_name
            .as_deref(),
        Some("git")
    );
    entity.update(&mut visual, |s, cx| {
        s.confirm_marketplace_upgrade("git".into(), cx);
        s.confirm_marketplace_remove("local".into(), cx);
        assert!(s.plugins_catalog.confirmation.is_none());
    });
    backend.0.lock().unwrap().upgrade_replies[0]
        .send_blocking(AgentMarketplaceUpgradeResult {
            generation: 1,
            marketplace_name: Some("git".into()),
            outcome: AgentPluginOperationOutcome::Succeeded(AgentMarketplaceUpgradeReceipt {
                selected_marketplaces: vec!["git".into()],
                upgraded_roots: vec![],
                errors: vec![],
                extra: Default::default(),
            }),
        })
        .unwrap();
    draw(&mut visual);
    for edge in [-1., 1.] {
        let button = visual.debug_bounds("marketplace-remove-1").unwrap();
        visual.simulate_click(
            point(
                button.center().x + edge * (button.size.width / 2. - px(1.)),
                button.center().y,
            ),
            Default::default(),
        );
        draw(&mut visual);
        entity.update(&mut visual, |s, cx| {
            assert_eq!(
                s.plugins_catalog.confirmation,
                Some(PluginConfirmation::MarketplaceRemove {
                    marketplace_name: "local".into()
                })
            );
            s.dismiss_catalog_confirmation(cx);
        });
        draw(&mut visual);
    }
    entity.update(&mut visual, |s, cx| {
        s.confirm_marketplace_remove("local".into(), cx);
        s.confirm_plugin_operation(cx);
        s.confirm_plugin_operation(cx);
    });
    draw(&mut visual);
    let reads;
    {
        let mut b = backend.0.lock().unwrap();
        assert_eq!(b.removes.len(), 1);
        assert_eq!(b.removes[0].marketplace_name, "local");
        reads = b.config_reads;
        b.config["marketplaces"]
            .as_object_mut()
            .unwrap()
            .remove("local");
        b.remove_replies[0]
            .send_blocking(AgentMarketplaceRemoveResult {
                generation: 1,
                marketplace_name: "local".into(),
                outcome: AgentPluginOperationOutcome::Succeeded(AgentMarketplaceRemoveReceipt {
                    marketplace_name: "local".into(),
                    installed_root: None,
                    extra: Default::default(),
                }),
            })
            .unwrap();
    }
    draw(&mut visual);
    assert!(backend.0.lock().unwrap().config_reads > reads);
    entity.read_with(&visual, |s, _| {
        assert_eq!(s.configured_marketplace_count(), 1)
    });
    assert!(visual.debug_bounds("marketplace-row-1").is_none());
}

#[gpui::test]
fn marketplaces_last_removal_returns_to_a_visible_tab(cx: &mut TestAppContext) {
    let (entity, backend, mut visual) = fixture(cx);
    entity.update(&mut visual, |s, cx| {
        s.select_plugins_segment(PluginSegment::Marketplaces, cx)
    });
    backend.0.lock().unwrap().config = json!({"marketplaces": {"openai-primary-runtime": {}}});
    entity.update(&mut visual, |s, cx| s.refresh_plugins(true, cx));
    draw(&mut visual);
    entity.read_with(&visual, |s, _| {
        assert_eq!(s.plugins_segment, PluginSegment::Plugins)
    });
    assert!(visual.debug_bounds("plugins-segment-4").is_none());
}

#[gpui::test]
fn marketplaces_add_uses_current_input_without_an_enter_submission(cx: &mut TestAppContext) {
    let (entity, backend, mut visual) = fixture(cx);
    entity.update(&mut visual, |s, cx| {
        s.open_marketplace_add(cx);
        s.confirm_plugin_operation(cx);
        assert!(
            s.plugins_catalog.confirmation.is_some(),
            "Empty source stays editable"
        );
        let input = s.marketplace_source_input.clone();
        input.update(cx, |input, cx| {
            input.set_text_silently("  file:///fixtures/catalog  ", cx);
            cx.emit(crate::components::prompt_input::PromptChanged);
        });
    });
    draw(&mut visual);
    assert!(backend.0.lock().unwrap().adds.is_empty());
    let button = visual.debug_bounds("catalog-confirm").unwrap();
    visual.simulate_click(button.center(), Default::default());
    draw(&mut visual);
    let script = backend.0.lock().unwrap();
    assert_eq!(script.adds.len(), 1);
    assert_eq!(script.adds[0].source, "file:///fixtures/catalog");
}
