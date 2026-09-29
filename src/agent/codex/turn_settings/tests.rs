//! Payloads follow the baseline CLI's answers recorded by
//! `scripts/batch2_app_server_probe.py` (artifacts/batch2-baseline-*/turn_settings.wire.json).

use serde_json::json;

use super::*;

#[test]
fn reviewer_update_sends_only_the_reviewer() {
    assert_eq!(
        reviewer_params("thread", "turn", "auto_review"),
        json!({"threadId": "thread", "turnId": "turn", "approvalsReviewer": "auto_review"})
    );
}

#[test]
fn both_statuses_decode_and_anything_else_is_rejected() {
    let response = |status: serde_json::Value| json!({"id": 7, "result": {"status": status}});
    assert_eq!(
        parse_response(&response(json!("applied"))).unwrap(),
        AgentTurnSettingsStatus::Applied
    );
    assert_eq!(
        parse_response(&response(json!("targetUnavailable"))).unwrap(),
        AgentTurnSettingsStatus::TargetUnavailable
    );
    assert!(parse_response(&response(json!("unavailable"))).is_err());
    assert!(parse_response(&response(json!(null))).is_err());
    assert!(parse_response(&json!({"id": 7, "result": {}})).is_err());
    assert!(parse_response(&json!({"id": 7})).is_err());
}
