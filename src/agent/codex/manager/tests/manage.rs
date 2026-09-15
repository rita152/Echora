//! Plugin, app, and external-agent-import behaviour, driven through scripted
//! app-servers.

use super::*;
use crate::agent::{
    AgentAppsListRequest, AgentExternalAgentDetectRequest, AgentExternalAgentImportRequest,
    AgentExternalAgentItemType, AgentPluginCatalogRequest, AgentPluginInstallReceipt,
    AgentPluginInstallRequest, AgentPluginOperationOutcome, AgentPluginSearchRequest,
};

/// Waits for the first connection event the predicate accepts, ignoring the
/// generation lifecycle observations these tests do not assert on.
fn wait_for_event(
    events: &async_channel::Receiver<crate::agent::AgentConnectionEvent>,
    predicate: impl Fn(&crate::agent::AgentConnectionEvent) -> bool,
) -> crate::agent::AgentConnectionEvent {
    let deadline = Instant::now() + WAIT;
    loop {
        match events.try_recv() {
            Ok(event) if predicate(&event) => return event,
            Ok(_) => {}
            Err(TryRecvError::Empty) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("no matching connection event: {error:?}"),
        }
    }
}

fn app_entry(id: &str, name: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "description": format!("{name} description"),
        "branding": null,
        "appMetadata": null
    })
}

fn catalog_payload(installed: bool) -> Value {
    json!({
        "featuredPluginIds": [],
        "marketplaceLoadErrors": [],
        "marketplaces": [{
            "name": "openai-primary-runtime",
            "path": "/Users/example/.cache/marketplace.json",
            "interface": {"displayName": "OpenAI primary runtime"},
            "plugins": [{
                "id": "documents@openai-primary-runtime",
                "name": "documents",
                "localVersion": "26.909.12148",
                "installed": installed,
                "installedAt": null,
                "enabled": installed,
                "installPolicy": "AVAILABLE",
                "installPolicySource": null,
                "authPolicy": "ON_USE",
                "availability": "AVAILABLE",
                "disabledReason": null,
                "eligiblePlanTypes": null,
                "mustShowInstallationInterstitial": null,
                "remotePluginId": null,
                "shareContext": null,
                "source": {"type": "local", "path": "/Users/example/plugins/documents"},
                "interface": {
                    "displayName": "Documents",
                    "shortDescription": "Create and edit documents",
                    "capabilities": [],
                    "screenshots": [],
                    "screenshotUrls": []
                },
                "keywords": ["doc"]
            }]
        }]
    })
}

#[test]
fn app_list_walks_pages_and_refuses_a_repeated_cursor() {
    let (manager, spawner) = manager_with_fake();
    let loaded = manager.load_apps(AgentAppsListRequest::default());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);

    let first = endpoint.recv();
    assert_eq!(first["method"], "app/list");
    // A page size is always sent so the walk is bounded and reproducible.
    assert_eq!(first["params"], json!({"limit": 100}));
    endpoint.respond(
        &first,
        json!({"data": [app_entry("app-1", "First")], "nextCursor": "c2"}),
    );

    let second = endpoint.recv();
    assert_eq!(second["params"], json!({"limit": 100, "cursor": "c2"}));
    // The same cursor again is a protocol error, not a page to follow twice.
    endpoint.respond(
        &second,
        json!({"data": [app_entry("app-2", "Second")], "nextCursor": "c2"}),
    );

    let error = wait_value(&loaded).unwrap_err();
    assert!(error.message.contains("重复的 nextCursor"), "{error:?}");
}

#[test]
fn app_list_updated_is_only_an_invalidation_signal() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let loaded = manager.load_apps(AgentAppsListRequest::default());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let list = endpoint.recv();
    endpoint.respond(&list, json!({"data": [], "nextCursor": null}));
    assert!(wait_value(&loaded).unwrap().apps.is_empty());

    // The notification carries a payload, but the client treats it as a cache
    // invalidation: the event names the generation and no directory data.
    endpoint.send(json!({
        "method": "app/list/updated",
        "params": {"data": [app_entry("app-1", "First")]}
    }));
    let event = wait_for_event(&events, |event| {
        matches!(
            event,
            crate::agent::AgentConnectionEvent::AppListUpdated { .. }
        )
    });
    assert!(matches!(
        event,
        crate::agent::AgentConnectionEvent::AppListUpdated { generation: 1 }
    ));

    // A malformed payload still fails loudly instead of being ignored.
    endpoint.send(json!({"method": "app/list/updated", "params": {"data": 7}}));
    let failure = wait_for_event(&events, |event| {
        matches!(event, crate::agent::AgentConnectionEvent::Runtime(_))
    });
    assert!(matches!(
        failure,
        crate::agent::AgentConnectionEvent::Runtime(_)
    ));
}

#[test]
fn plugin_install_reports_success_failure_and_unknown_results_separately() {
    let (manager, spawner) = manager_with_fake();
    let first = manager.install_plugin(AgentPluginInstallRequest {
        generation: 1,
        plugin_name: "documents".to_owned(),
        marketplace_path: Some("/Users/example/marketplace.json".to_owned()),
        remote_marketplace_name: None,
        install_attempt_id: None,
    });
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    assert_eq!(request["method"], "plugin/install");
    // The local selector is a path and the remote one is an explicit null, which
    // is what the reference client sends.
    assert_eq!(
        request["params"],
        json!({
            "pluginName": "documents",
            "marketplacePath": "/Users/example/marketplace.json",
            "remoteMarketplaceName": null
        })
    );
    endpoint.respond(
        &request,
        json!({"authPolicy": "ON_USE", "appsNeedingAuth": []}),
    );
    let receipt: AgentPluginInstallReceipt = match wait_value(&first).outcome {
        AgentPluginOperationOutcome::Succeeded(receipt) => receipt,
        other => panic!("expected success, got {other:?}"),
    };
    assert_eq!(receipt.apps_needing_auth.len(), 0);

    // A refusal is a failure that keeps the server's own message and code.
    let refused = manager.install_plugin(AgentPluginInstallRequest {
        generation: 1,
        plugin_name: "missing".to_owned(),
        marketplace_path: None,
        remote_marketplace_name: None,
        install_attempt_id: None,
    });
    let request = endpoint.recv();
    endpoint.send(json!({
        "id": request["id"],
        "error": {"code": -32000, "message": "plugin not found"}
    }));
    match wait_value(&refused).outcome {
        AgentPluginOperationOutcome::Failed { message, .. } => {
            // The server's own text reaches the user unchanged, whether the
            // transport surfaced it as an error object or as text.
            assert!(message.contains("plugin not found"), "{message}");
        }
        other => panic!("expected failure, got {other:?}"),
    }

    // A receipt the client cannot read is an unknown result, never a success.
    let unreadable = manager.install_plugin(AgentPluginInstallRequest {
        generation: 1,
        plugin_name: "documents".to_owned(),
        marketplace_path: None,
        remote_marketplace_name: None,
        install_attempt_id: None,
    });
    let request = endpoint.recv();
    endpoint.respond(&request, json!({"authPolicy": "NOT_A_POLICY"}));
    let outcome = wait_value(&unreadable).outcome;
    assert!(outcome.outcome_unknown(), "{outcome:?}");
}

#[test]
fn plugin_catalog_and_search_keep_the_servers_own_rows() {
    let (manager, spawner) = manager_with_fake();
    let loaded = manager.load_plugin_catalog(AgentPluginCatalogRequest::default());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    assert_eq!(request["method"], "plugin/list");
    assert_eq!(request["params"], json!({}));
    endpoint.respond(&request, catalog_payload(true));
    let catalog = wait_value(&loaded).unwrap();
    let (marketplace, plugin) = catalog.plugins().next().unwrap();
    assert_eq!(marketplace.name, "openai-primary-runtime");
    assert_eq!(marketplace.display_name(), "OpenAI primary runtime");
    assert_eq!(plugin.display_name(), "Documents");
    assert!(plugin.installed && plugin.enabled);
    assert_eq!(catalog.installed_plugin_count(), 1);

    let searched = manager.search_plugins(AgentPluginSearchRequest {
        cursor: None,
        limit: None,
        scope: Some("global"),
        search_term: "doc".to_owned(),
        cwds: None,
    });
    let request = endpoint.recv();
    assert_eq!(request["method"], "plugin/search");
    assert_eq!(
        request["params"],
        json!({"searchTerm": "doc", "scope": "global", "limit": 100})
    );
    endpoint.respond(
        &request,
        json!({
            "data": [{
                "plugin": catalog_payload(true)["marketplaces"][0]["plugins"][0].clone(),
                "marketplaceName": "openai-primary-runtime",
                "marketplacePath": "/Users/example/marketplace.json"
            }],
            "nextCursor": null
        }),
    );
    let page = wait_value(&searched).unwrap();
    assert_eq!(page.search_term, "doc");
    assert_eq!(page.results.len(), 1);
    assert_eq!(page.results[0].marketplace_name, "openai-primary-runtime");
}

#[test]
fn external_agent_detect_and_import_carry_the_expected_parameters() {
    let (manager, spawner) = manager_with_fake();
    let detected = manager.detect_external_agent_config(AgentExternalAgentDetectRequest {
        cwds: None,
        include_home: true,
        max_session_age_days: None,
        max_sessions: None,
        migration_source: Some("cursor".to_owned()),
        source: None,
    });
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    assert_eq!(request["method"], "externalAgentConfig/detect");
    assert_eq!(
        request["params"],
        json!({"includeHome": true, "migrationSource": "cursor"})
    );
    endpoint.respond(
        &request,
        json!({
            "items": [{
                "itemType": "SESSIONS",
                "description": "Migrate recent sessions from /Users/example/.cursor/projects",
                "cwd": null,
                "details": {"sessions": [{"title": "one"}]}
            }],
            "connectors": [{"name": "cursor-app-control", "sessionCount": 2, "source": "sessionToolUse"}]
        }),
    );
    let result = wait_value(&detected).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(
        result.items[0].item_type,
        AgentExternalAgentItemType::Sessions
    );
    assert_eq!(result.items[0].cwd, None);
    assert_eq!(result.connectors.as_ref().unwrap()[0].session_count, 2);

    let started = manager.import_external_agent_config(AgentExternalAgentImportRequest {
        migration_items: result.items.clone(),
        migration_source: Some("cursor".to_owned()),
        provider_id: Some("cursor".to_owned()),
        source: Some("app".to_owned()),
    });
    let request = endpoint.recv();
    assert_eq!(request["method"], "externalAgentConfig/import");
    // The item the server sent is echoed unchanged, including its null cwd.
    assert_eq!(
        request["params"]["migrationItems"][0],
        json!({
            "itemType": "SESSIONS",
            "description": "Migrate recent sessions from /Users/example/.cursor/projects",
            "cwd": null,
            "details": {"sessions": [{"title": "one"}]}
        })
    );
    assert_eq!(request["params"]["providerId"], json!("cursor"));
    assert_eq!(request["params"]["source"], json!("app"));
    endpoint.respond(
        &request,
        json!({"importId": "5f1ead92-8d21-4c8f-a543-30b9fd1b5886"}),
    );
    assert_eq!(
        wait_value(&started).unwrap().import_id,
        "5f1ead92-8d21-4c8f-a543-30b9fd1b5886"
    );
}

#[test]
fn import_progress_and_completion_are_published_with_their_import_id() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let loaded = manager.load_apps(AgentAppsListRequest::default());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let list = endpoint.recv();
    endpoint.respond(&list, json!({"data": [], "nextCursor": null}));
    assert!(wait_value(&loaded).unwrap().apps.is_empty());

    endpoint.send(json!({
        "method": "externalAgentConfig/import/progress",
        "params": {
            "importId": "import-1",
            "itemTypeResults": [{
                "itemType": "SKILLS",
                "successes": [{"itemType": "SKILLS", "target": "release-notes"}],
                "failures": []
            }]
        }
    }));
    let progress = wait_for_event(&events, |event| {
        matches!(
            event,
            crate::agent::AgentConnectionEvent::ExternalAgentImportStatus(status)
                if !status.completed
        )
    });
    match progress {
        crate::agent::AgentConnectionEvent::ExternalAgentImportStatus(status) => {
            assert_eq!(status.import_id, "import-1");
            assert_eq!(status.successful_item_count(), 1);
            assert_eq!(status.failed_item_count(), 0);
        }
        other => panic!("unexpected event {other:?}"),
    }

    endpoint.send(json!({
        "method": "externalAgentConfig/import/completed",
        "params": {
            "importId": "import-1",
            "itemTypeResults": [{
                "itemType": "SESSIONS",
                "successes": [],
                "failures": [{
                    "itemType": "SESSIONS",
                    "failureStage": "copy",
                    "message": "permission denied"
                }]
            }]
        }
    }));
    let completed = wait_for_event(&events, |event| {
        matches!(
            event,
            crate::agent::AgentConnectionEvent::ExternalAgentImportStatus(status)
                if status.completed
        )
    });
    match completed {
        crate::agent::AgentConnectionEvent::ExternalAgentImportStatus(status) => {
            assert_eq!(status.failed_item_count(), 1);
            let failure = &status.item_type_results[0].failures[0];
            assert_eq!(failure.failure_stage, "copy");
            assert_eq!(failure.message, "permission denied");
        }
        other => panic!("unexpected event {other:?}"),
    }
}
