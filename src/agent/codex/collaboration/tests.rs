use serde_json::json;

use super::*;

#[test]
fn presets_are_deduplicated_per_mode_and_ordered_plan_first() {
    // The baseline answer, reversed and with extras, still yields plan, default.
    let presets = parse_list_response(&json!({"id":1,"result":{"data":[
        {"name":"Default","mode":"default","model":null,"reasoning_effort":null},
        {"name":"Unscoped","mode":null},
        {"name":"Plan","mode":"plan","model":null,"reasoning_effort":"medium"},
        {"name":"Plan again","mode":"plan","model":"other"}
    ]}}))
    .unwrap();
    assert_eq!(
        presets
            .iter()
            .map(|preset| preset.name.as_str())
            .collect::<Vec<_>>(),
        ["Plan", "Default"]
    );
    assert_eq!(presets[0].reasoning_effort.as_deref(), Some("medium"));
    assert_eq!(presets[1].model, None);
}

#[test]
fn preset_fields_are_validated_strictly() {
    for bad in [
        json!({"id":1,"result":{}}),
        json!({"id":1,"result":{"data":[{"mode":"plan"}]}}),
        json!({"id":1,"result":{"data":[{"name":"X","mode":"pair"}]}}),
        json!({"id":1,"result":{"data":[{"name":"X","mode":"plan","reasoning_effort":""}]}}),
        json!({"id":1,"result":{"data":[{"name":"X","mode":"plan","model":3}]}}),
    ] {
        assert!(parse_list_response(&bad).is_err(), "{bad}");
    }
}

#[test]
fn turn_mode_keeps_the_users_model_and_effort() {
    let presets = parse_list_response(&json!({"id":1,"result":{"data":[
        {"name":"Plan","mode":"plan","model":"preset-model","reasoning_effort":"medium"},
        {"name":"Default","mode":"default"}
    ]}}))
    .unwrap();
    let plan = turn_collaboration_mode(
        &presets,
        AgentCollaborationModeKind::Plan,
        "user-model",
        "high",
    )
    .unwrap();
    assert_eq!(
        plan,
        json!({"mode":"plan","settings":{"model":"user-model","reasoning_effort":"high","developer_instructions":null}})
    );
    // Default is still sent without presets; plan needs its preset.
    assert_eq!(
        turn_collaboration_mode(&[], AgentCollaborationModeKind::Default, "m", "low").unwrap()["mode"],
        "default"
    );
    assert!(turn_collaboration_mode(&[], AgentCollaborationModeKind::Plan, "m", "low").is_err());
}
