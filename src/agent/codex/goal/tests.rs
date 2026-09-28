//! Goal codec. Payloads follow the baseline CLI's answers recorded by
//! `scripts/batch1_app_server_probe.py` (artifacts/batch1-baseline-*/goal.wire.json).

use serde_json::json;

use super::*;
use crate::agent::{AgentOptionalField, AgentThreadGoalStatus, AgentThreadGoalUpdate};

fn goal(status: &str) -> serde_json::Value {
    json!({
        "threadId": "thread", "objective": "Say ok", "status": status,
        "tokenBudget": null, "tokensUsed": 858, "timeUsedSeconds": 1,
        "createdAt": 1790590446, "updatedAt": 1790590447
    })
}

#[test]
fn every_goal_status_decodes_and_unknown_status_is_rejected() {
    for (wire, status) in [
        ("active", AgentThreadGoalStatus::Active),
        ("paused", AgentThreadGoalStatus::Paused),
        ("blocked", AgentThreadGoalStatus::Blocked),
        ("usageLimited", AgentThreadGoalStatus::UsageLimited),
        ("budgetLimited", AgentThreadGoalStatus::BudgetLimited),
        ("complete", AgentThreadGoalStatus::Complete),
    ] {
        let parsed = parse_goal(&goal(wire)).unwrap();
        assert_eq!(parsed.status, status);
        assert_eq!(status_wire(status), wire);
    }
    assert!(parse_goal(&goal("done")).is_err());
    let mut missing = goal("active");
    missing.as_object_mut().unwrap().remove("tokensUsed");
    assert!(parse_goal(&missing).is_err());
    let mut budget = goal("paused");
    budget["tokenBudget"] = json!(1000);
    assert_eq!(parse_goal(&budget).unwrap().token_budget, Some(1000));
    budget["tokenBudget"] = json!("1000");
    assert!(parse_goal(&budget).is_err());
}

#[test]
fn set_params_send_only_the_fields_the_caller_changes() {
    let mut update = AgentThreadGoalUpdate {
        generation: 1,
        thread_id: "thread".into(),
        objective: Some("Say ok".into()),
        status: Some(AgentThreadGoalStatus::Active),
        token_budget: AgentOptionalField::Unspecified,
    };
    assert_eq!(
        set_params(&update),
        json!({"threadId":"thread","objective":"Say ok","status":"active"})
    );
    update.objective = None;
    update.status = Some(AgentThreadGoalStatus::Paused);
    assert_eq!(
        set_params(&update),
        json!({"threadId":"thread","status":"paused"})
    );
    update.status = None;
    update.token_budget = AgentOptionalField::Null;
    assert_eq!(
        set_params(&update),
        json!({"threadId":"thread","tokenBudget":null})
    );
    update.token_budget = AgentOptionalField::Value(1000);
    assert_eq!(set_params(&update)["tokenBudget"], 1000);
}

#[test]
fn responses_are_validated_against_the_requested_thread() {
    let set = json!({"id": 4, "result": {"goal": goal("active")}});
    assert_eq!(
        parse_set_response(&set, "thread").unwrap().objective,
        "Say ok"
    );
    assert!(parse_set_response(&set, "other").is_err());
    assert!(parse_set_response(&json!({"id":4,"result":{}}), "thread").is_err());
    assert_eq!(
        parse_get_response(&json!({"id":3,"result":{"goal":null}}), "thread").unwrap(),
        None
    );
    assert_eq!(
        parse_get_response(&json!({"id":3,"result":{}}), "thread").unwrap(),
        None
    );
    assert!(parse_get_response(&json!({"id":3,"result":{"goal":goal("paused")}}), "x").is_err());
    assert!(parse_clear_response(&json!({"id":10,"result":{"cleared":true}})).unwrap());
    assert!(!parse_clear_response(&json!({"id":11,"result":{"cleared":false}})).unwrap());
    assert!(parse_clear_response(&json!({"id":11,"result":{}})).is_err());
}

#[test]
fn notifications_keep_turn_identity_and_reject_mismatches() {
    let updated = json!({"method":"thread/goal/updated","params":{"threadId":"thread","turnId":null,"goal":goal("active")}});
    assert!(matches!(
        parse_notification(&updated).unwrap(),
        GoalNotification::Updated { turn_id: None, .. }
    ));
    let with_turn = json!({"method":"thread/goal/updated","params":{"threadId":"thread","turnId":"turn","goal":goal("complete")}});
    assert!(matches!(
        parse_notification(&with_turn).unwrap(),
        GoalNotification::Updated { turn_id: Some(turn), goal, .. } if turn == "turn" && goal.status == AgentThreadGoalStatus::Complete
    ));
    let bad_turn = json!({"method":"thread/goal/updated","params":{"threadId":"thread","turnId":7,"goal":goal("active")}});
    assert!(parse_notification(&bad_turn).is_err());
    let other_thread =
        json!({"method":"thread/goal/updated","params":{"threadId":"other","goal":goal("active")}});
    assert!(parse_notification(&other_thread).is_err());
    let cleared = json!({"method":"thread/goal/cleared","params":{"threadId":"thread"}});
    assert_eq!(
        parse_notification(&cleared).unwrap(),
        GoalNotification::Cleared {
            thread_id: "thread".into()
        }
    );
    assert!(
        parse_notification(
            &json!({"id":1,"method":"thread/goal/cleared","params":{"threadId":"thread"}})
        )
        .is_err()
    );
    assert!(parse_notification(&json!({"method":"thread/goal/cleared","params":{}})).is_err());
}
