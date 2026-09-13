//! Skills and MCP management behaviour, driven through scripted app-servers.

use super::*;
use crate::agent::{
    AgentMcpOauthCompletionStatus, AgentMcpOauthLoginRequest, AgentMcpReloadOutcome,
    AgentMcpReloadRequest, AgentMcpServerStatusRequest, AgentSkillWriteRequest,
    AgentSkillsLoadRequest, AgentSkillsSnapshot,
};

fn skills_request() -> AgentSkillsLoadRequest {
    AgentSkillsLoadRequest {
        cwds: vec!["/tmp/project".into()],
        force_reload: false,
    }
}

fn skills_page(enabled: bool) -> Value {
    json!({
        "data": [{
            "cwd": "/tmp/project",
            "skills": [{
                "name": "release-notes",
                "description": "Draft release notes",
                "path": "/skills/release-notes/SKILL.md",
                "scope": "user",
                "enabled": enabled,
                "pluginId": null
            }],
            "errors": []
        }]
    })
}

/// Connection events also carry generation lifecycle observations; these tests
/// only care about the management events they trigger.
fn wait_for_event(
    events: &async_channel::Receiver<AgentConnectionEvent>,
    predicate: impl Fn(&AgentConnectionEvent) -> bool,
) -> AgentConnectionEvent {
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

#[test]
fn skills_list_and_write_share_one_connection_and_one_receipt() {
    let (manager, spawner) = manager_with_fake();
    let loaded = manager.load_skills(skills_request());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let list = endpoint.recv();
    assert_eq!(list["method"], "skills/list");
    assert_eq!(list["params"], json!({"cwds": ["/tmp/project"]}));
    endpoint.respond(&list, skills_page(true));
    let snapshot: AgentSkillsSnapshot = wait_value(&loaded).unwrap();
    assert_eq!(snapshot.generation, 1);
    assert_eq!(snapshot.next_cursor, None);
    assert_eq!(snapshot.skills().count(), 1);

    let written = manager.write_skill_config(AgentSkillWriteRequest {
        generation: 1,
        selector: crate::agent::AgentSkillSelector::Path("/skills/release-notes/SKILL.md".into()),
        enabled: false,
    });
    let write = endpoint.recv();
    assert_eq!(write["method"], "skills/config/write");
    // Only the user's change is submitted; the selector is the acted-on path.
    assert_eq!(
        write["params"],
        json!({"enabled": false, "path": "/skills/release-notes/SKILL.md"})
    );
    endpoint.respond(&write, json!({"effectiveEnabled": false}));
    let receipt = wait_value(&written).unwrap();
    assert!(!receipt.effective_enabled);
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}

#[test]
fn skill_write_after_generation_change_is_rejected_without_a_request() {
    let (manager, spawner) = manager_with_fake();
    let loaded = manager.load_skills(skills_request());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let list = endpoint.recv();
    endpoint.respond(&list, skills_page(true));
    let snapshot: AgentSkillsSnapshot = wait_value(&loaded).unwrap();
    manager
        .inner
        .fail_generation(snapshot.generation, "connection lost".into());
    let written = manager.write_skill_config(AgentSkillWriteRequest {
        generation: snapshot.generation,
        selector: crate::agent::AgentSkillSelector::Name("release-notes".into()),
        enabled: false,
    });
    // The next explicit operation rebuilds the connection; the stale write is
    // then rejected against the new generation instead of being replayed.
    let mut replacement = spawner.next_endpoint();
    handshake(&mut replacement);
    let error = wait_value(&written).unwrap_err();
    assert_eq!(error.kind, crate::agent::AgentSkillsErrorKind::Connection);
    assert!(!replacement.methods().contains(&"skills/config/write"));
    manager.shutdown();
}

#[test]
fn skills_changed_notification_is_published_with_its_generation() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let loaded = manager.load_skills(skills_request());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let list = endpoint.recv();
    endpoint.respond(&list, skills_page(true));
    let _: AgentSkillsSnapshot = wait_value(&loaded).unwrap();
    endpoint.send(json!({"method": "skills/changed", "params": {}}));
    let event = wait_for_event(&events, |event| {
        matches!(event, AgentConnectionEvent::SkillsChanged { .. })
    });
    assert!(matches!(
        event,
        AgentConnectionEvent::SkillsChanged { generation: 1 }
    ));
    manager.shutdown();
}

#[test]
fn mcp_status_list_preserves_server_extensions_across_pages() {
    let (manager, spawner) = manager_with_fake();
    let page = manager.list_mcp_servers(AgentMcpServerStatusRequest {
        cursor: None,
        limit: Some(1),
        detail: None,
        thread_id: None,
    });
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    assert_eq!(request["method"], "mcpServerStatus/list");
    assert_eq!(request["params"], json!({"limit": 1}));
    endpoint.respond(
        &request,
        json!({
            "data": [{
                "name": "echo-tools",
                "pluginId": null,
                "authStatus": "unsupported",
                "runtimeStatus": "connected",
                "serverInfo": {"name": "echora-echo", "version": "1.4.0"},
                "tools": {"echo": {"name": "echo", "inputSchema": {"type": "object"}}},
                "resources": [],
                "resourceTemplates": [],
                "vendorExtension": 5
            }],
            "nextCursor": "page-2"
        }),
    );
    let page = wait_value(&page).unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("page-2"));
    assert_eq!(page.servers[0].extra["vendorExtension"], json!(5));
    assert_eq!(
        page.servers[0].runtime_status,
        Some(crate::agent::AgentMcpServerConnectionStatus::Connected)
    );
    manager.shutdown();
}

#[test]
fn mcp_reload_reports_server_errors_without_retrying() {
    let (manager, spawner) = manager_with_fake();
    let reloaded = manager.reload_mcp_servers(AgentMcpReloadRequest {
        cwd: "/tmp/project".into(),
        generation: 1,
    });
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    assert_eq!(request["method"], "config/mcpServer/reload");
    assert!(request.get("params").is_none());
    endpoint.send(json!({
        "id": request["id"],
        "error": {"code": -32603, "message": "reload failed"}
    }));
    let result = wait_value(&reloaded);
    assert!(matches!(
        result.outcome,
        AgentMcpReloadOutcome::Failed { .. }
    ));
    assert!(!result.outcome.outcome_unknown());
    assert!(endpoint.from_client.try_recv().is_err());
    manager.shutdown();
}

/// Answers the pending `mcpServer/oauth/login` request on the endpoint that the
/// manager just created and returns the client-generated login id.
fn complete_login(
    endpoint: &mut FakeEndpoint,
    login: async_channel::Receiver<
        Result<crate::agent::AgentMcpOauthLogin, crate::agent::AgentMcpError>,
    >,
) -> u64 {
    let request = endpoint.recv();
    assert_eq!(request["method"], "mcpServer/oauth/login");
    assert_eq!(request["params"], json!({"name": "notes-oauth"}));
    endpoint.respond(
        &request,
        json!({"authorizationUrl": "https://auth.example.invalid/authorize"}),
    );
    let login = wait_value(&login).unwrap();
    assert_eq!(
        login.authorization_url,
        "https://auth.example.invalid/authorize"
    );
    login.login_id
}

#[test]
fn oauth_completion_correlates_with_the_started_login_and_ignores_repeats() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let login = manager.start_mcp_oauth_login(AgentMcpOauthLoginRequest {
        generation: 1,
        server_name: "notes-oauth".into(),
        thread_id: None,
        scopes: None,
        client_registration: None,
        timeout_secs: None,
    });
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let login_id = complete_login(&mut endpoint, login);
    endpoint.send(json!({
        "method": "mcpServer/oauthLogin/completed",
        "params": {"name": "notes-oauth", "threadId": null, "success": true}
    }));
    let event = wait_for_event(&events, |event| {
        matches!(event, AgentConnectionEvent::McpOauthLoginCompleted(_))
    });
    let AgentConnectionEvent::McpOauthLoginCompleted(completion) = event else {
        panic!("expected an oauth completion event");
    };
    assert_eq!(completion.login_id, login_id);
    assert_eq!(completion.status, AgentMcpOauthCompletionStatus::Succeeded);
    // A duplicate or late completion for the same login is inert.
    endpoint.send(json!({
        "method": "mcpServer/oauthLogin/completed",
        "params": {"name": "notes-oauth", "threadId": null, "success": false, "error": "late"}
    }));
    assert!(events.try_recv().is_err());
    manager.shutdown();
}

#[test]
fn cancelled_login_reports_once_and_ignores_a_late_success() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let login = manager.start_mcp_oauth_login(AgentMcpOauthLoginRequest {
        generation: 1,
        server_name: "notes-oauth".into(),
        thread_id: None,
        scopes: None,
        client_registration: None,
        timeout_secs: None,
    });
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let login_id = complete_login(&mut endpoint, login);
    wait_value(&manager.cancel_mcp_oauth_login(login_id)).unwrap();
    let cancelled = wait_for_event(&events, |event| {
        matches!(event, AgentConnectionEvent::McpOauthLoginCompleted(_))
    });
    let AgentConnectionEvent::McpOauthLoginCompleted(completion) = cancelled else {
        panic!("expected a cancellation event");
    };
    assert_eq!(completion.login_id, login_id);
    assert_eq!(completion.status, AgentMcpOauthCompletionStatus::Cancelled);
    endpoint.send(json!({
        "method": "mcpServer/oauthLogin/completed",
        "params": {"name": "notes-oauth", "threadId": null, "success": true}
    }));
    assert!(events.try_recv().is_err());
    manager.shutdown();
}

#[test]
fn generation_loss_reports_the_pending_login_as_interrupted() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let login = manager.start_mcp_oauth_login(AgentMcpOauthLoginRequest {
        generation: 1,
        server_name: "notes-oauth".into(),
        thread_id: None,
        scopes: None,
        client_registration: None,
        timeout_secs: None,
    });
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let login_id = complete_login(&mut endpoint, login);
    manager.inner.fail_generation(1, "connection lost".into());
    let mut interrupted = None;
    while let Ok(event) = events.try_recv() {
        if let AgentConnectionEvent::McpOauthLoginCompleted(completion) = event
            && completion.login_id == login_id
        {
            interrupted = Some(completion.status);
        }
    }
    assert!(matches!(
        interrupted,
        Some(AgentMcpOauthCompletionStatus::Interrupted(_))
    ));
    manager.shutdown();
}

#[test]
fn startup_status_notification_carries_generation_and_thread_scope() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let listed = manager.list_mcp_servers(AgentMcpServerStatusRequest::default());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let list = endpoint.recv();
    endpoint.respond(&list, json!({"data": []}));
    wait_value(&listed).unwrap();
    endpoint.send(json!({
        "method": "mcpServer/startupStatus/updated",
        "params": {"threadId": "thread-1", "name": "broken", "status": "failed", "error": "boom", "failureReason": "reauthenticationRequired"}
    }));
    let event = wait_for_event(&events, |event| {
        matches!(
            event,
            AgentConnectionEvent::McpServerStartupStatusUpdated(_)
        )
    });
    let AgentConnectionEvent::McpServerStartupStatusUpdated(updated) = event else {
        panic!("expected a startup status event");
    };
    assert_eq!(updated.generation, 1);
    assert_eq!(updated.status.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(
        updated.status.failure_reason,
        Some(crate::agent::AgentMcpServerStartupFailureReason::ReauthenticationRequired)
    );
    manager.shutdown();
}
