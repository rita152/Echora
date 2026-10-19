use super::*;
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
            script.catalogs[index]
                .1
                .send_blocking(Ok(catalog(available)))
                .unwrap();
            script.installed[index]
                .send_blocking(Ok(catalog(installed)))
                .unwrap();
        }
        self.app.run_until_parked();
    }
}

#[test]
fn public_personal_and_installed_reads_have_real_scopes_and_discard_late_responses() {
    let mut f = Fixture::new();
    {
        let s = f.backend.0.lock().unwrap();
        assert_eq!(
            s.catalogs[0].0.marketplace_kinds,
            Some(vec![Kind::Vertical])
        );
        assert_eq!(
            s.catalogs[0].0.cwds,
            Some(vec![PathBuf::from("/verification")])
        );
    }
    f.with(|s, cx| s.select_directory_scope(Scope::Personal, cx));
    f.reply(0, vec![plugin("public-old", false)], vec![]);
    f.with(|s, _| assert!(s.plugins_catalog.directory.catalog.is_none()));
    f.reply(
        1,
        vec![plugin("personal", false)],
        vec![plugin("installed-only", true)],
    );
    f.with(|s, _| {
        assert_eq!(s.plugins_generation, 7);
        assert_eq!(s.directory_rows()[0].plugin.name, "personal");
        s.directory_scope = Scope::Installed;
        assert_eq!(s.directory_rows()[0].plugin.name, "installed-only");
    });
    let s = f.backend.0.lock().unwrap();
    assert!(
        !s.catalogs[1]
            .0
            .marketplace_kinds
            .as_ref()
            .unwrap()
            .contains(&Kind::Vertical)
    );
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
    f.with(|s, _| assert_eq!(s.directory_rows()[0].plugin.name, "current"));
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
fn refresh_reissues_the_query_for_the_new_catalog_cycle() {
    let mut f = Fixture::new();
    f.reply(0, vec![plugin("current", false)], vec![]);
    f.with(|s, cx| {
        s.plugins_catalog.directory.set_search_term("same".into());
        s.submit_plugin_search(cx);
        s.refresh_plugins(true, cx);
    });
    f.reply(1, vec![plugin("current", false)], vec![]);
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
    {
        let script = f.backend.0.lock().unwrap();
        assert_eq!(script.searches.len(), 2);
        script.searches[0]
            .1
            .send_blocking(Ok(page("stale")))
            .unwrap();
        script.searches[1]
            .1
            .send_blocking(Ok(page("current")))
            .unwrap();
    }
    f.app.run_until_parked();
    f.with(|s, _| {
        assert!(!s.plugins_catalog.directory.searching);
        assert_eq!(s.directory_rows()[0].plugin.name, "current");
    });
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
