//! Account, login, and quota routing tests for the shared manager.

use serde_json::{Value, json};

use super::{FakeEndpoint, WAIT, handshake, manager_with_fake, wait_value};
use crate::agent::{
    AgentAccountLoginPhase, AgentAccountPlanType, AgentAccountPresence, AgentConnectionEvent,
    AgentLoginCancelOutcome,
};

/// Reads the next account connection event of a given kind.
fn wait_for_account_event(
    events: &async_channel::Receiver<AgentConnectionEvent>,
    select: impl Fn(&AgentConnectionEvent) -> bool,
) -> AgentConnectionEvent {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        while let Ok(event) = events.try_recv() {
            if select(&event) {
                return event;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "account connection event did not arrive"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// Answers the account and quota reads a test triggered, on whichever order
/// the manager issued them.
fn answer_account_reads(endpoint: &mut FakeEndpoint, account: Value, rate_limits: Value) {
    let mut answered = 0;
    while answered < 2 {
        let message = endpoint.recv();
        match message["method"].as_str().unwrap() {
            "account/read" => {
                endpoint.respond(&message, account.clone());
                answered += 1;
            }
            "account/rateLimits/read" => {
                endpoint.respond(&message, rate_limits.clone());
                answered += 1;
            }
            other => panic!("unexpected request during account reads: {other}"),
        }
    }
}

#[test]
fn account_read_and_quota_read_publish_snapshots_that_new_subscribers_replay() {
    let (manager, spawner) = manager_with_fake();
    let reader = manager.read_account();
    let limits = manager.read_rate_limits();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    answer_account_reads(
        &mut endpoint,
        json!({ "requiresOpenaiAuth": true, "account": {"type":"chatgpt","email":"rita@example.com","planType":"pro"} }),
        json!({
            "accountId": "acct_1",
            "ordinaryUsageAllowed": true,
            "rateLimits": {"limitId":"codex","primary":{"usedPercent":27,"windowDurationMins":10080,"resetsAt":1788752152}},
            "rateLimitsByLimitId": {
                "codex": {"limitId":"codex","primary":{"usedPercent":27,"windowDurationMins":10080,"resetsAt":1788752152}},
                "gpt-5-spark": {"limitId":"gpt-5-spark","limitName":"GPT-5 Spark","primary":{"usedPercent":0,"windowDurationMins":300}}
            }
        }),
    );
    wait_value(&reader).unwrap();
    wait_value(&limits).unwrap();

    // A subscriber that joins later must receive both current snapshots.
    let replay = manager.subscribe_connection_events();
    let mut saw_account = false;
    let mut saw_limits = false;
    while let Ok(event) = replay.try_recv() {
        match event {
            AgentConnectionEvent::AccountUpdated(snapshot) => {
                assert_eq!(snapshot.email(), Some("rita@example.com"));
                assert_eq!(snapshot.plan_type, None);
                saw_account = true;
            }
            AgentConnectionEvent::AccountRateLimitsUpdated(state) => {
                assert_eq!(state.account_id.as_deref(), Some("acct_1"));
                assert_eq!(state.buckets.len(), 2);
                assert_eq!(
                    state
                        .buckets
                        .get("gpt-5-spark")
                        .and_then(|bucket| bucket.limit_name.as_deref()),
                    Some("GPT-5 Spark")
                );
                saw_limits = true;
            }
            _ => {}
        }
    }
    assert!(
        saw_account && saw_limits,
        "new subscriber missed the account snapshot"
    );
    manager.shutdown();
}

#[test]
fn account_notifications_merge_per_bucket_and_keep_nullable_values() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let reader = manager.read_account();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let read = endpoint.recv();
    assert_eq!(read["method"], "account/read");
    endpoint.respond(
        &read,
        json!({ "requiresOpenaiAuth": true, "account": {"type":"chatgpt","email":"rita@example.com","planType":"pro"} }),
    );
    wait_value(&reader).unwrap();

    endpoint
        .send(json!({"method":"account/updated","params":{"authMode":"chatgpt","planType":"pro"}}));
    let updated = wait_for_account_event(
        &events,
        |event| matches!(event, AgentConnectionEvent::AccountUpdated(snapshot) if snapshot.auth_mode.is_some()),
    );
    let AgentConnectionEvent::AccountUpdated(snapshot) = updated else {
        unreachable!()
    };
    assert_eq!(snapshot.plan_type, Some(AgentAccountPlanType::Pro));
    assert_eq!(snapshot.email(), Some("rita@example.com"));

    endpoint.send(json!({
        "method":"account/rateLimits/updated",
        "params":{"rateLimits":{"limitId":"codex","limitName":"Codex","primary":{"usedPercent":25,"windowDurationMins":10080,"resetsAt":1788752152}}}
    }));
    wait_for_account_event(
        &events,
        |event| matches!(event, AgentConnectionEvent::AccountRateLimitsUpdated(state) if state.buckets.contains_key("codex")),
    );

    // A rolling update for another bucket must not touch the first one, and a
    // nullable field reports unavailability instead of clearing a value.
    endpoint.send(json!({
        "method":"account/rateLimits/updated",
        "params":{"rateLimits":{"limitId":"gpt-5-spark","limitName":null,"primary":{"usedPercent":40}}}
    }));
    let state = wait_for_account_event(
        &events,
        |event| matches!(event, AgentConnectionEvent::AccountRateLimitsUpdated(state) if state.buckets.contains_key("gpt-5-spark")),
    );
    let AgentConnectionEvent::AccountRateLimitsUpdated(state) = state else {
        unreachable!()
    };
    let codex = state.buckets.get("codex").expect("codex bucket");
    assert_eq!(codex.primary.as_ref().unwrap().used_percent, 25);
    assert_eq!(codex.limit_name.as_deref(), Some("Codex"));
    let spark = state.buckets.get("gpt-5-spark").expect("spark bucket");
    assert_eq!(spark.primary.as_ref().unwrap().used_percent, 40);
    assert!(spark.limit_name.is_none());
    assert_eq!(
        spark.primary.as_ref().unwrap().window_duration_mins,
        None,
        "a sparse window keeps only the fields it carried"
    );
    manager.shutdown();
}

#[test]
fn logout_clears_the_snapshot_and_confirms_with_fresh_reads() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let reader = manager.read_account();
    let limits = manager.read_rate_limits();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    answer_account_reads(
        &mut endpoint,
        json!({ "requiresOpenaiAuth": true, "account": {"type":"chatgpt","email":"rita@example.com","planType":"pro"} }),
        json!({"accountId":"acct_1","rateLimits":{"limitId":"codex","primary":{"usedPercent":27}}}),
    );
    wait_value(&reader).unwrap();
    wait_value(&limits).unwrap();

    let logout = manager.logout();
    let request = endpoint.recv();
    assert_eq!(request["method"], "account/logout");
    endpoint.respond(&request, json!({}));
    // Logout is followed by a fresh account and quota read.
    answer_account_reads(
        &mut endpoint,
        json!({ "requiresOpenaiAuth": true, "account": null }),
        json!({"rateLimits":{}}),
    );
    let outcome = wait_value(&logout).unwrap();
    assert_eq!(
        outcome.account.map(|snapshot| snapshot.account),
        Some(AgentAccountPresence::Null)
    );
    assert!(outcome.confirmation_error.is_none());

    // The cleared snapshot is what a new subscriber observes.
    let replay = manager.subscribe_connection_events();
    let mut rate_limits = None;
    while let Ok(event) = replay.try_recv() {
        if let AgentConnectionEvent::AccountRateLimitsUpdated(state) = event {
            rate_limits = Some(state);
        }
    }
    let state = rate_limits.expect("quota snapshot");
    assert!(state.account_id.is_none());
    assert!(state.buckets.is_empty());
    assert!(
        std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, AgentConnectionEvent::AccountUpdated(snapshot) if snapshot.email().is_none()))
    );
    manager.shutdown();
}

#[test]
fn login_completion_after_cancel_is_ignored_and_a_success_refreshes_the_account() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let start = manager.start_login("chatgpt".into());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    assert_eq!(request["method"], "account/login/start");
    assert_eq!(request["params"]["type"], "chatgpt");
    endpoint.respond(
        &request,
        json!({"type":"chatgpt","authUrl":"https://example.com/auth","loginId":"login_1"}),
    );
    let started = wait_value(&start).unwrap();
    assert_eq!(started.login_id, "login_1");
    let state = wait_for_account_event(
        &events,
        |event| matches!(event, AgentConnectionEvent::AccountLoginUpdated(login) if login.phase == AgentAccountLoginPhase::InProgress),
    );
    let AgentConnectionEvent::AccountLoginUpdated(login) = state else {
        unreachable!()
    };
    assert_eq!(login.login_id.as_deref(), Some("login_1"));
    assert!(login.challenge.is_some());

    // Cancelling only ends the matching login.
    let cancel = manager.cancel_login("login_1".into());
    let request = endpoint.recv();
    assert_eq!(request["method"], "account/login/cancel");
    assert_eq!(request["params"]["loginId"], "login_1");
    endpoint.respond(&request, json!({"status":"canceled"}));
    assert_eq!(
        wait_value(&cancel).unwrap(),
        AgentLoginCancelOutcome::Canceled
    );
    let state = wait_for_account_event(
        &events,
        |event| matches!(event, AgentConnectionEvent::AccountLoginUpdated(login) if login.phase == AgentAccountLoginPhase::Canceled),
    );
    assert!(matches!(
        state,
        AgentConnectionEvent::AccountLoginUpdated(_)
    ));

    // A late completion cannot revive the cancelled login, and it does not
    // start another account read.
    endpoint.send(json!({"method":"account/login/completed","params":{"success":true,"loginId":"login_1","error":null}}));
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(
        std::iter::from_fn(|| events.try_recv().ok()).all(|event| !matches!(
            event,
            AgentConnectionEvent::AccountLoginUpdated(login)
                if login.phase == AgentAccountLoginPhase::SignedIn
        )),
        "a late completion must not report a successful login"
    );
    manager.shutdown();
}

#[test]
fn successful_login_completion_refreshes_account_and_quota_state() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let start = manager.start_login("chatgpt".into());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = endpoint.recv();
    endpoint.respond(
        &request,
        json!({"type":"chatgptDeviceCode","loginId":"login_2","userCode":"ABCD-1234","verificationUrl":"https://example.com/device"}),
    );
    let started = wait_value(&start).unwrap();
    assert!(matches!(
        started.challenge,
        crate::agent::AgentLoginChallenge::DeviceCode { .. }
    ));

    endpoint.send(json!({"method":"account/login/completed","params":{"success":true,"loginId":"login_2","error":null,"onboardingEntrypoint":"life_sciences"}}));
    let refresh = endpoint.recv();
    assert_eq!(refresh["method"], "account/read");
    endpoint.respond(
        &refresh,
        json!({ "requiresOpenaiAuth": true, "account": {"type":"chatgpt","email":"rita@example.com","planType":"pro"} }),
    );
    let limits = endpoint.recv();
    assert_eq!(limits["method"], "account/rateLimits/read");
    endpoint.respond(
        &limits,
        json!({"accountId":"acct_1","rateLimits":{"limitId":"codex","primary":{"usedPercent":12}}}),
    );
    wait_for_account_event(
        &events,
        |event| matches!(event, AgentConnectionEvent::AccountUpdated(snapshot) if snapshot.email() == Some("rita@example.com")),
    );
    let state = wait_for_account_event(
        &events,
        |event| matches!(event, AgentConnectionEvent::AccountRateLimitsUpdated(state) if state.account_id.as_deref() == Some("acct_1")),
    );
    let AgentConnectionEvent::AccountRateLimitsUpdated(state) = state else {
        unreachable!()
    };
    assert_eq!(
        state
            .buckets
            .get("codex")
            .and_then(|bucket| bucket.primary.as_ref())
            .map(|window| window.used_percent),
        Some(12)
    );
    manager.shutdown();
}

#[test]
fn a_failed_generation_clears_account_snapshots_for_later_subscribers() {
    let (manager, spawner) = manager_with_fake();
    let reader = manager.read_account();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let read = endpoint.recv();
    endpoint.respond(
        &read,
        json!({ "requiresOpenaiAuth": true, "account": {"type":"chatgpt","email":"rita@example.com","planType":"pro"} }),
    );
    wait_value(&reader).unwrap();
    endpoint.close_stdout();
    std::thread::sleep(std::time::Duration::from_millis(200));

    let replay = manager.subscribe_connection_events();
    while let Ok(event) = replay.try_recv() {
        if let AgentConnectionEvent::AccountUpdated(snapshot) = event {
            assert!(
                snapshot.email().is_none(),
                "a retired generation must not replay its account"
            );
        }
    }
    manager.shutdown();
}
