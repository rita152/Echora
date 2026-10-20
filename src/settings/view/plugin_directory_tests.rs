use super::*;
use crate::agent::AgentPluginMarketplaceKind as Kind;
use crate::agent::*;
use gpui::TestApp;
use std::sync::{Arc, Mutex};

type Reply<T> = async_channel::Sender<Result<T, AgentPluginsError>>;
#[derive(Default)]
struct Script {
    catalogs: Vec<(AgentPluginCatalogRequest, Reply<AgentPluginCatalog>)>,
    installed: Vec<Reply<AgentPluginCatalog>>,
    details: Vec<(AgentPluginReadRequest, Reply<AgentPluginDetail>)>,
    searches: Vec<(AgentPluginSearchRequest, Reply<AgentPluginSearchPage>)>,
    installs: Vec<(
        AgentPluginInstallRequest,
        async_channel::Sender<AgentPluginInstallResult>,
    )>,
}
#[derive(Default)]
struct Backend(Mutex<Script>);
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
        async_channel::unbounded().1
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
    fn load_plugin_catalog(
        &self,
        request: AgentPluginCatalogRequest,
    ) -> async_channel::Receiver<Result<AgentPluginCatalog, AgentPluginsError>> {
        let (tx, rx) = async_channel::unbounded();
        self.0.lock().unwrap().catalogs.push((request, tx));
        rx
    }
    fn load_installed_plugins(
        &self,
        _: AgentPluginInstalledRequest,
    ) -> async_channel::Receiver<Result<AgentPluginCatalog, AgentPluginsError>> {
        let (tx, rx) = async_channel::unbounded();
        self.0.lock().unwrap().installed.push(tx);
        rx
    }
    fn read_plugin(
        &self,
        request: AgentPluginReadRequest,
    ) -> async_channel::Receiver<Result<AgentPluginDetail, AgentPluginsError>> {
        let (tx, rx) = async_channel::unbounded();
        self.0.lock().unwrap().details.push((request, tx));
        rx
    }
    fn search_plugins(
        &self,
        request: AgentPluginSearchRequest,
    ) -> async_channel::Receiver<Result<AgentPluginSearchPage, AgentPluginsError>> {
        let (tx, rx) = async_channel::unbounded();
        self.0.lock().unwrap().searches.push((request, tx));
        rx
    }
    fn install_plugin(
        &self,
        request: AgentPluginInstallRequest,
    ) -> async_channel::Receiver<AgentPluginInstallResult> {
        let (tx, rx) = async_channel::unbounded();
        self.0.lock().unwrap().installs.push((request, tx));
        rx
    }
}

fn plugin(name: &str, installed: bool) -> AgentPluginSummary {
    AgentPluginSummary {
        id: format!("{name}@test-market"),
        name: name.into(),
        version: Some("1.0".into()),
        local_version: None,
        enabled: true,
        installed,
        installed_at: None,
        install_policy: AgentPluginInstallPolicy::Available,
        install_policy_source: None,
        auth_policy: AgentPluginAuthPolicy::OnUse,
        availability: Some(AgentPluginAvailability::Available),
        disabled_reason: None,
        eligible_plan_types: None,
        interface: None,
        keywords: None,
        must_show_installation_interstitial: None,
        remote_plugin_id: None,
        share_context: None,
        source: AgentPluginSource::Remote,
        extra: Default::default(),
    }
}
fn catalog(plugins: Vec<AgentPluginSummary>) -> AgentPluginCatalog {
    AgentPluginCatalog {
        generation: 7,
        marketplaces: vec![AgentPluginMarketplace {
            name: "test-market".into(),
            path: None,
            interface: None,
            plugins,
            extra: Default::default(),
        }],
        featured_plugin_ids: vec![],
        marketplace_load_errors: vec![],
        extra: Default::default(),
    }
}
fn error() -> AgentPluginsError {
    AgentPluginsError {
        kind: AgentPluginsErrorKind::Connection,
        message: "installed read failed".into(),
        data: None,
        outcome_unknown: false,
    }
}

struct Fixture {
    settings: gpui::Entity<SettingsView>,
    backend: Arc<Backend>,
    // TestApp checks for live entity handles when dropped, so it must be last.
    app: TestApp,
}
impl Fixture {
    fn new() -> Self {
        let mut app = TestApp::new();
        let backend = Arc::new(Backend::default());
        let settings = app
            .new_entity(|cx| SettingsView::new(crate::theme::ThemeMode::Dark, backend.clone(), cx));
        app.update_entity(&settings, |s, cx| {
            s.plugins_generation = 7;
            s.open_plugin_directory("/verification".into(), cx)
        });
        Self {
            app,
            settings,
            backend,
        }
    }
    fn with<R>(&mut self, f: impl FnOnce(&mut SettingsView, &mut Context<SettingsView>) -> R) -> R {
        self.app.update_entity(&self.settings, f)
    }
    fn reply(
        &mut self,
        index: usize,
        available: Vec<AgentPluginSummary>,
        installed: Vec<AgentPluginSummary>,
    ) {
        {
            let script = self.backend.0.lock().unwrap();
            script.catalogs[index * 5]
                .1
                .send_blocking(Ok(catalog(available)))
                .unwrap();
            for (_, reply) in script.catalogs.iter().skip(index * 5 + 1).take(4) {
                reply.send_blocking(Ok(catalog(vec![]))).unwrap();
            }
            script.installed[index]
                .send_blocking(Ok(catalog(installed)))
                .unwrap();
        }
        self.app.run_until_parked();
    }
}

#[test]
fn all_sources_are_isolated_and_switching_to_installed_discards_late_catalogs() {
    let mut f = Fixture::new();
    {
        let s = f.backend.0.lock().unwrap();
        assert_eq!(s.catalogs.len(), 5);
        assert_eq!(s.catalogs[0].0.marketplace_kinds, Some(vec![Kind::Local]));
        assert_eq!(
            s.catalogs[0].0.cwds,
            Some(vec![PathBuf::from("/verification")])
        );
        for (index, kind) in [
            Kind::Vertical,
            Kind::WorkspaceDirectory,
            Kind::SharedWithMe,
            Kind::CreatedByMeRemote,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(s.catalogs[index + 1].0.marketplace_kinds, Some(vec![kind]));
        }
    }
    f.with(|s, cx| s.select_directory_scope(Scope::Installed, cx));
    f.reply(0, vec![plugin("stale", false)], vec![]);
    {
        let script = f.backend.0.lock().unwrap();
        assert_eq!(
            script.catalogs.len(),
            5,
            "installed does not query unrelated catalogs"
        );
        script.installed[1]
            .send_blocking(Ok(catalog(vec![plugin("installed-only", true)])))
            .unwrap();
    }
    f.app.run_until_parked();
    f.with(|s, _| {
        assert!(s.plugins_catalog.directory.catalog.is_none());
        assert_eq!(s.directory_rows()[0].plugin.name, "installed-only");
        assert!(!s.plugins_catalog.directory.loading);
    });
}

#[test]
fn installed_failure_is_visible_and_cannot_become_a_successful_empty_inventory() {
    let mut f = Fixture::new();
    {
        let s = f.backend.0.lock().unwrap();
        s.catalogs[0].1.send_blocking(Ok(catalog(vec![]))).unwrap();
        s.installed[0].send_blocking(Err(error())).unwrap();
    }
    f.app.run_until_parked();
    f.with(|s, _| {
        assert!(s.plugins_catalog.directory.installed.is_none());
        assert!(s.plugins_catalog.directory.installed_error.is_some());
        assert!(!s.plugins_catalog.directory.installed_loading);
    });
}

#[test]
fn remote_details_and_confirmed_install_preserve_selector_and_generation() {
    let mut f = Fixture::new();
    f.reply(0, vec![plugin("remote", false)], vec![]);
    f.with(|s, cx| s.open_plugin_detail("remote@test-market".into(), cx));
    {
        let script = f.backend.0.lock().unwrap();
        let read = &script.details[0].0;
        assert_eq!(read.remote_marketplace_name.as_deref(), Some("test-market"));
        assert!(read.marketplace_path.is_none());
        assert!(script.installs.is_empty());
    }
    f.with(|s, cx| {
        s.plugins_catalog.confirmation = Some(PluginConfirmation::Install {
            plugin_name: "remote".into(),
            marketplace_path: None,
            marketplace_name: "test-market".into(),
            display_name: "Remote".into(),
        });
        s.confirm_plugin_operation(cx);
    });
    {
        let script = f.backend.0.lock().unwrap();
        assert_eq!(script.installs.len(), 1);
        assert_eq!(script.installs[0].0.generation, 7);
        assert_eq!(
            script.installs[0].0.remote_marketplace_name.as_deref(),
            Some("test-market")
        );
        script.installs[0]
            .1
            .send_blocking(AgentPluginInstallResult {
                generation: 7,
                plugin_name: "remote".into(),
                outcome: AgentPluginOperationOutcome::Unknown {
                    message: "connection replaced".into(),
                },
            })
            .unwrap();
    }
    f.app.run_until_parked();
    f.with(|s, _| {
        assert!(s.directory_operation_blocked());
        assert!(
            !s.plugins_catalog
                .directory
                .confirmed_installed(&plugin("remote", false))
        );
    });
}

#[test]
fn same_term_search_replies_from_an_older_request_are_ignored() {
    let mut f = Fixture::new();
    f.reply(0, vec![plugin("current", false)], vec![]);
    f.with(|s, cx| {
        s.directory_presentation = false;
        s.plugins_catalog.directory.set_search_term("same".into());
        s.submit_plugin_search(cx);
        s.submit_plugin_search(cx);
    });
    let page = |name| AgentPluginSearchPage {
        generation: 7,
        cursor: None,
        search_term: "same".into(),
        results: vec![AgentPluginSearchResult {
            marketplace_name: "test-market".into(),
            marketplace_path: None,
            plugin: plugin(name, false),
        }],
        next_cursor: None,
        extra: Default::default(),
    };
    f.backend.0.lock().unwrap().searches[1]
        .1
        .send_blocking(Ok(page("current")))
        .unwrap();
    f.app.run_until_parked();
    f.backend.0.lock().unwrap().searches[0]
        .1
        .send_blocking(Ok(page("stale")))
        .unwrap();
    f.app.run_until_parked();
    f.with(|s, _| {
        assert_eq!(
            s.plugins_catalog.directory.visible_plugins()[0].plugin.name,
            "current"
        )
    });
}

#[test]
fn closing_details_invalidates_the_in_flight_read_and_returns_to_the_same_catalog() {
    let mut f = Fixture::new();
    f.reply(0, vec![plugin("remote", false)], vec![]);
    f.with(|s, cx| {
        s.open_plugin_detail("remote@test-market".into(), cx);
        s.dismiss_plugin_directory(cx);
    });
    f.backend.0.lock().unwrap().details[0]
        .1
        .send_blocking(Err(error()))
        .unwrap();
    f.app.run_until_parked();
    f.with(|s, _| {
        assert!(s.plugins_catalog.open_plugin.is_none());
        assert!(s.plugins_catalog.directory.detail_error.is_none());
        assert_eq!(s.directory_rows().len(), 1);
    });
}

#[test]
fn successful_install_waits_for_the_installed_readback_before_showing_installed() {
    let mut f = Fixture::new();
    f.reply(0, vec![plugin("remote", false)], vec![]);
    f.with(|s, cx| {
        s.plugins_catalog.confirmation = Some(PluginConfirmation::Install {
            plugin_name: "remote".into(),
            marketplace_path: None,
            marketplace_name: "test-market".into(),
            display_name: "Remote".into(),
        });
        s.confirm_plugin_operation(cx);
        assert!(
            !s.plugins_catalog
                .directory
                .confirmed_installed(&plugin("remote", false))
        );
    });
    f.backend.0.lock().unwrap().installs[0]
        .1
        .send_blocking(AgentPluginInstallResult {
            generation: 7,
            plugin_name: "remote".into(),
            outcome: AgentPluginOperationOutcome::Succeeded(AgentPluginInstallReceipt {
                auth_policy: AgentPluginAuthPolicy::OnUse,
                apps_needing_auth: vec![],
            }),
        })
        .unwrap();
    f.app.run_until_parked();
    f.with(|s, _| {
        assert!(s.plugins_catalog.directory.installed_loading);
        assert!(
            !s.plugins_catalog
                .directory
                .confirmed_installed(&plugin("remote", false))
        );
    });
    f.reply(
        1,
        vec![plugin("remote", true)],
        vec![plugin("remote", true)],
    );
    f.with(|s, _| {
        assert!(
            s.plugins_catalog
                .directory
                .confirmed_installed(&plugin("remote", false))
        );
        assert!(!s.directory_operation_blocked());
    });
}

#[test]
fn directory_refresh_preserves_the_query_and_filters_new_sources_without_remote_search() {
    let mut f = Fixture::new();
    f.reply(0, vec![plugin("current", false)], vec![]);
    f.with(|s, cx| {
        s.plugins_catalog.directory.set_search_term("cur".into());
        s.submit_plugin_search(cx);
        s.refresh_plugins(true, cx);
    });
    f.reply(
        1,
        vec![plugin("current-new", false), plugin("other", false)],
        vec![],
    );
    f.with(|s, _| {
        assert!(!s.plugins_catalog.directory.searching);
        assert_eq!(s.directory_rows().len(), 1);
        assert_eq!(s.directory_rows()[0].plugin.name, "current-new");
    });
    assert!(f.backend.0.lock().unwrap().searches.is_empty());
}

#[test]
fn installed_search_uses_the_inventory_without_a_global_search_request() {
    let mut f = Fixture::new();
    f.reply(
        0,
        vec![],
        vec![plugin("installed-only", true), plugin("other", true)],
    );
    f.with(|s, cx| {
        s.directory_scope = Scope::Installed;
        s.directory_search_changed("installed".into(), cx);
        s.submit_plugin_search(cx);
        assert!(!s.plugins_catalog.directory.searching);
        assert_eq!(s.directory_rows().len(), 1);
        assert_eq!(s.directory_rows()[0].plugin.name, "installed-only");
    });
    assert!(f.backend.0.lock().unwrap().searches.is_empty());
}

#[test]
fn local_and_installed_results_publish_while_remote_sources_are_pending_or_rejected() {
    let mut f = Fixture::new();
    {
        let script = f.backend.0.lock().unwrap();
        script.catalogs[0]
            .1
            .send_blocking(Ok(catalog(vec![plugin("local", false)])))
            .unwrap();
        script.installed[0]
            .send_blocking(Ok(catalog(vec![plugin("installed", true)])))
            .unwrap();
    }
    f.app.run_until_parked();
    f.with(|s, _| {
        assert_eq!(s.directory_rows()[0].plugin.name, "local");
        assert_eq!(s.plugins_catalog.directory.installed_rows().len(), 1);
        assert!(!s.plugins_catalog.directory.installed_loading);
        assert!(s.plugins_catalog.directory.loading);
    });
    {
        let script = f.backend.0.lock().unwrap();
        script.catalogs[1]
            .1
            .send_blocking(Ok(catalog(vec![])))
            .unwrap();
        for (_, reply) in &script.catalogs[2..] {
            let mut e = error();
            e.kind = AgentPluginsErrorKind::AuthenticationRequired;
            reply.send_blocking(Err(e)).unwrap();
        }
    }
    f.app.run_until_parked();
    f.with(|s, _| {
        assert_eq!(s.directory_rows()[0].plugin.name, "local");
        assert_eq!(s.plugins_catalog.directory.installed_rows().len(), 1);
        assert!(s.plugins_catalog.directory.source_failed());
        assert!(!s.plugins_catalog.directory.loading);
    });
}

#[test]
fn reconnect_discards_old_source_and_installed_replies() {
    let mut f = Fixture::new();
    f.with(|s, cx| s.plugin_generation_started(8, cx));
    f.reply(0, vec![plugin("stale", false)], vec![plugin("stale", true)]);
    f.with(|s, _| {
        assert!(s.directory_rows().is_empty());
        assert!(s.plugins_catalog.directory.installed.is_none());
        assert_eq!(s.plugins_generation, 8);
    });
    {
        let script = f.backend.0.lock().unwrap();
        for (_, reply) in &script.catalogs[5..] {
            let mut c = catalog(vec![plugin("new", false)]);
            c.generation = 8;
            reply.send_blocking(Ok(c)).unwrap();
        }
        let mut c = catalog(vec![plugin("new", true)]);
        c.generation = 8;
        script.installed[1].send_blocking(Ok(c)).unwrap();
    }
    f.app.run_until_parked();
    f.with(|s, _| assert_eq!(s.directory_rows()[0].plugin.name, "new"));
}

#[test]
fn an_empty_catalog_is_distinct_from_all_sources_failing() {
    for failed in [false, true] {
        let mut f = Fixture::new();
        {
            let script = f.backend.0.lock().unwrap();
            for (_, reply) in &script.catalogs {
                reply
                    .send_blocking(if failed {
                        Err(error())
                    } else {
                        Ok(catalog(vec![]))
                    })
                    .unwrap();
            }
            script.installed[0]
                .send_blocking(Ok(catalog(vec![])))
                .unwrap();
        }
        f.app.run_until_parked();
        f.with(|s, _| {
            assert_eq!(s.plugins_catalog.directory.catalog.is_some(), !failed);
            assert_eq!(s.plugins_catalog.directory.source_failed(), failed);
            assert!(!s.plugins_catalog.directory.loading);
            assert!(s.directory_rows().is_empty());
            assert!(!s.plugins_catalog.directory.installed_loading);
        });
    }
}

#[test]
fn merged_sources_deduplicate_server_ids_and_keep_the_original_marketplace_selector() {
    let mut f = Fixture::new();
    {
        let script = f.backend.0.lock().unwrap();
        let mut local = catalog(vec![plugin("local", false)]);
        local.marketplaces[0].path = Some("/actual/local/catalog.json".into());
        script.catalogs[0]
            .1
            .send_blocking(Ok(local.clone()))
            .unwrap();
        local.marketplaces[0].plugins.push(plugin("remote", false));
        script.catalogs[1].1.send_blocking(Ok(local)).unwrap();
        for (_, reply) in &script.catalogs[2..] {
            reply.send_blocking(Err(error())).unwrap();
        }
        script.installed[0]
            .send_blocking(Ok(catalog(vec![])))
            .unwrap();
    }
    f.app.run_until_parked();
    f.with(|s, cx| {
        assert_eq!(s.directory_rows().len(), 2);
        s.open_plugin_detail("local@test-market".into(), cx);
    });
    let script = f.backend.0.lock().unwrap();
    assert_eq!(
        script.details[0].0.marketplace_path.as_deref(),
        Some("/actual/local/catalog.json")
    );
    assert!(script.details[0].0.remote_marketplace_name.is_none());
}

#[test]
fn unavailable_sources_do_not_hide_available_plugins_and_late_sources_publish_automatically() {
    let mut f = Fixture::new();
    {
        let script = f.backend.0.lock().unwrap();
        script.catalogs[0]
            .1
            .send_blocking(Ok(catalog(vec![plugin("local", false)])))
            .unwrap();
        script.catalogs[1]
            .1
            .send_blocking(Ok(catalog(vec![])))
            .unwrap();
        for index in [2, 3] {
            let mut denied = error();
            denied.kind = AgentPluginsErrorKind::AuthenticationRequired;
            script.catalogs[index].1.send_blocking(Err(denied)).unwrap();
        }
        script.installed[0]
            .send_blocking(Ok(catalog(vec![])))
            .unwrap();
    }
    f.app.run_until_parked();
    f.with(|s, _| {
        assert_eq!(s.directory_rows().len(), 1);
        assert_eq!(s.directory_rows()[0].plugin.name, "local");
        assert!(s.plugins_catalog.directory.loading);
        assert!(s.plugins_catalog.directory.error.is_none());
    });
    f.backend.0.lock().unwrap().catalogs[4]
        .1
        .send_blocking(Ok(catalog(vec![plugin("remote-later", false)])))
        .unwrap();
    f.app.run_until_parked();
    f.with(|s, _| {
        let names = s
            .directory_rows()
            .iter()
            .map(|row| row.plugin.name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"local"));
        assert!(names.contains(&"remote-later"));
        assert!(!s.plugins_catalog.directory.loading);
    });
    f.with(|s, cx| {
        assert!(
            s.directory_feedback(crate::theme::Theme::for_mode(s.mode), cx)
                .is_none()
        )
    });
}
