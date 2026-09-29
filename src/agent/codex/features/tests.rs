//! Payloads follow `scripts/batch2_app_server_probe.py --scenario features`
//! and `--scenario memory` (artifacts/batch2-baseline-*).

use serde_json::json;

use super::*;

#[test]
fn list_params_match_the_reference_page_request() {
    assert_eq!(
        list_params(None, None),
        json!({"cursor": null, "limit": 100})
    );
    assert_eq!(
        list_params(Some("100"), Some("t")),
        json!({"cursor": "100", "limit": 100, "threadId": "t"})
    );
}

#[test]
fn a_page_decodes_every_stage_and_its_cursor() {
    let response = json!({"id": 2, "result": {"data": [
        {"name": "network_proxy", "stage": "beta", "displayName": "Network proxy",
         "description": "Apply network proxy restrictions to sandboxed sessions that already have network access.",
         "announcement": "NEW: Network proxy can now be enabled from /experimental. Restart Codex after enabling it.",
         "enabled": false, "defaultEnabled": false},
        {"name": "memories", "stage": "stable", "displayName": null, "description": null, "announcement": null,
         "enabled": true, "defaultEnabled": false},
        {"name": "chronicle", "stage": "underDevelopment", "displayName": null, "description": null,
         "announcement": null, "enabled": false, "defaultEnabled": false},
        {"name": "transcript_v2", "stage": "deprecated", "displayName": null, "description": null,
         "announcement": null, "enabled": false, "defaultEnabled": false},
        {"name": "undo", "stage": "removed", "displayName": null, "description": null, "announcement": null,
         "enabled": false, "defaultEnabled": false}
    ], "nextCursor": "40"}});
    let (features, next) = parse_list_page(&response).unwrap();
    assert_eq!(next.as_deref(), Some("40"));
    assert_eq!(features.len(), 5);
    assert_eq!(features[0].label(), "Network proxy");
    assert!(features[0].listed_in_settings());
    assert!(
        features
            .iter()
            .skip(1)
            .all(|feature| !feature.listed_in_settings())
    );
    let (_, last) =
        parse_list_page(&json!({"id": 3, "result": {"data": [], "nextCursor": null}})).unwrap();
    assert_eq!(last, None);
}

#[test]
fn unknown_stage_bad_names_and_missing_flags_are_rejected() {
    let page = |feature: serde_json::Value| json!({"id": 1, "result": {"data": [feature]}});
    let base = json!({"name": "x", "stage": "beta", "enabled": true, "defaultEnabled": false});
    assert!(parse_list_page(&page(base.clone())).is_ok());
    for (field, value) in [
        ("stage", json!("alpha")),
        ("name", json!("a.b")),
        ("name", json!("")),
        ("enabled", json!("yes")),
    ] {
        let mut feature = base.clone();
        feature[field] = value;
        assert!(parse_list_page(&page(feature)).is_err(), "{field}");
    }
    let mut missing = base;
    missing.as_object_mut().unwrap().remove("defaultEnabled");
    assert!(parse_list_page(&page(missing)).is_err());
}

#[test]
fn memory_requests_and_empty_results() {
    assert_eq!(
        memory_mode_params("t", AgentThreadMemoryMode::Disabled),
        json!({"threadId": "t", "mode": "disabled"})
    );
    assert_eq!(
        memory_mode_params("t", AgentThreadMemoryMode::Enabled)["mode"],
        "enabled"
    );
    assert!(parse_empty_result(&json!({"id": 9, "result": {}}), MEMORY_RESET_METHOD).is_ok());
    assert!(parse_empty_result(&json!({"id": 9, "result": null}), MEMORY_RESET_METHOD).is_err());
    assert!(parse_empty_result(&json!({"id": 9}), MEMORY_MODE_SET_METHOD).is_err());
}
