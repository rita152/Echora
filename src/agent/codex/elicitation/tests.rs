use serde_json::{Value, json};

use super::{elicitation_response_result, parse_mcp_server_elicitation_request};
use crate::agent::{
    AgentMcpElicitationAction, AgentMcpElicitationContent, AgentMcpElicitationFieldKind,
    AgentMcpElicitationFieldValue, AgentMcpElicitationMode, AgentMcpElicitationResponse,
    AgentMcpElicitationStringFormat, AgentMcpElicitationValue, AgentOptionalField,
    AgentServerRequestId,
};

fn form_request(params: Value) -> Value {
    json!({
        "id": 11,
        "method": "mcpServer/elicitation/request",
        "params": params
    })
}

fn request_with_schema(schema: Value) -> Value {
    form_request(json!({
        "serverName": "fixture",
        "threadId": "thr-1",
        "mode": "form",
        "message": "请填写部署信息",
        "requestedSchema": schema
    }))
}

#[test]
fn form_mode_decodes_every_supported_primitive() {
    let request = parse_mcp_server_elicitation_request(
        &request_with_schema(json!({
            "type": "object",
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "properties": {
                "name": {
                    "type": "string",
                    "title": "名称",
                    "description": "部署名称",
                    "default": "echora",
                    "minLength": 2,
                    "maxLength": 40
                },
                "contact": { "type": "string", "format": "email" },
                "replicas": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 8,
                    "default": 3
                },
                "ratio": { "type": "number", "minimum": 0.5 },
                "enabled": { "type": "boolean", "default": true },
                "region": {
                    "type": "string",
                    "enum": ["us-east", "eu-west"],
                    "enumNames": ["美东", "西欧"],
                    "default": "eu-west"
                },
                "tier": {
                    "type": "string",
                    "oneOf": [
                        { "const": "basic", "title": "基础" },
                        { "const": "pro", "title": "专业" }
                    ]
                },
                "features": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 2,
                    "default": ["logs"],
                    "items": { "type": "string", "enum": ["logs", "metrics"] }
                },
                "channels": {
                    "type": "array",
                    "items": {
                        "anyOf": [
                            { "const": "email", "title": "邮件" },
                            { "const": "sms", "title": "短信" }
                        ]
                    }
                }
            },
            "required": ["name", "replicas"]
        })),
        7,
    )
    .unwrap();

    assert_eq!(request.generation, 7);
    assert_eq!(request.request_id, AgentServerRequestId::Number(11));
    assert_eq!(request.server_name, "fixture");
    assert_eq!(request.thread_id, "thr-1");
    assert_eq!(request.turn_id, AgentOptionalField::Unspecified);
    let AgentMcpElicitationMode::Form(form) = &request.mode else {
        panic!("expected form mode");
    };
    assert_eq!(form.message, "请填写部署信息");
    assert_eq!(form.fields.len(), 9);

    let name = &form.fields[0];
    assert_eq!(name.name, "name");
    assert!(name.required);
    assert_eq!(name.title.as_deref(), Some("名称"));
    assert_eq!(name.description.as_deref(), Some("部署名称"));
    assert_eq!(
        name.kind,
        AgentMcpElicitationFieldKind::String {
            format: None,
            min_length: Some(2),
            max_length: Some(40),
        }
    );
    assert_eq!(
        name.default,
        Some(AgentMcpElicitationValue::String("echora".into()))
    );

    assert_eq!(
        form.fields[1].kind,
        AgentMcpElicitationFieldKind::String {
            format: Some(AgentMcpElicitationStringFormat::Email),
            min_length: None,
            max_length: None,
        }
    );
    let AgentMcpElicitationFieldKind::Number {
        integer,
        minimum,
        maximum,
    } = &form.fields[2].kind
    else {
        panic!("expected integer field");
    };
    assert!(*integer);
    assert_eq!(minimum.as_ref().unwrap().as_u64(), Some(1));
    assert_eq!(maximum.as_ref().unwrap().as_u64(), Some(8));
    assert!(form.fields[2].required);

    let AgentMcpElicitationFieldKind::Number { integer, .. } = &form.fields[3].kind else {
        panic!("expected number field");
    };
    assert!(!*integer);
    assert_eq!(form.fields[4].kind, AgentMcpElicitationFieldKind::Boolean);
    assert_eq!(
        form.fields[4].default,
        Some(AgentMcpElicitationValue::Boolean(true))
    );

    let AgentMcpElicitationFieldKind::SingleSelect { options } = &form.fields[5].kind else {
        panic!("expected legacy enum single select");
    };
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].value, "us-east");
    assert_eq!(options[0].title, "美东");
    assert_eq!(
        form.fields[5].default,
        Some(AgentMcpElicitationValue::String("eu-west".into()))
    );

    let AgentMcpElicitationFieldKind::SingleSelect { options } = &form.fields[6].kind else {
        panic!("expected titled single select");
    };
    assert_eq!(options[1].value, "pro");
    assert_eq!(options[1].title, "专业");

    let AgentMcpElicitationFieldKind::MultiSelect {
        options,
        min_items,
        max_items,
    } = &form.fields[7].kind
    else {
        panic!("expected multi select");
    };
    assert_eq!(options.len(), 2);
    assert_eq!(*min_items, Some(1));
    assert_eq!(*max_items, Some(2));
    assert_eq!(
        form.fields[7].default,
        Some(AgentMcpElicitationValue::StringArray(vec!["logs".into()]))
    );

    let AgentMcpElicitationFieldKind::MultiSelect { options, .. } = &form.fields[8].kind else {
        panic!("expected titled multi select");
    };
    assert_eq!(options[0].title, "邮件");
}

#[test]
fn turn_id_missing_null_and_value_stay_distinct() {
    let mut params = json!({
        "serverName": "fixture",
        "threadId": "thr-1",
        "mode": "form",
        "message": "m",
        "requestedSchema": { "type": "object", "properties": {} }
    });
    let request = parse_mcp_server_elicitation_request(&form_request(params.clone()), 1).unwrap();
    assert_eq!(request.turn_id, AgentOptionalField::Unspecified);

    params["turnId"] = Value::Null;
    let request = parse_mcp_server_elicitation_request(&form_request(params.clone()), 1).unwrap();
    assert_eq!(request.turn_id, AgentOptionalField::Null);

    params["turnId"] = json!("turn-9");
    let request = parse_mcp_server_elicitation_request(&form_request(params.clone()), 1).unwrap();
    assert_eq!(
        request.turn_id,
        AgentOptionalField::Value("turn-9".to_owned())
    );

    params["turnId"] = json!(9);
    assert!(parse_mcp_server_elicitation_request(&form_request(params), 1).is_err());
}

#[test]
fn params_require_strings_and_keep_original_request_id_types() {
    let string_id = json!({
        "id": "elicitation-a",
        "method": "mcpServer/elicitation/request",
        "params": {
            "serverName": "fixture",
            "threadId": "thr-1",
            "mode": "url",
            "elicitationId": "url-1",
            "message": "请在浏览器完成授权",
            "url": "https://example.com/oauth?state=1"
        }
    });
    let request = parse_mcp_server_elicitation_request(&string_id, 3).unwrap();
    assert_eq!(
        request.request_id,
        AgentServerRequestId::String("elicitation-a".into())
    );
    assert_eq!(request.generation, 3);

    for (mutated, expectation) in [
        (json!(null), "missing serverName"),
        (json!(7), "numeric serverName"),
    ] {
        let mut message = string_id.clone();
        message["params"]["serverName"] = mutated;
        let error = parse_mcp_server_elicitation_request(&message, 3).unwrap_err();
        assert!(
            error.to_string().contains("serverName"),
            "{expectation}: {error:#}"
        );
    }
    let mut message = string_id.clone();
    message["params"]["threadId"] = json!([]);
    assert!(parse_mcp_server_elicitation_request(&message, 3).is_err());
    let mut message = string_id.clone();
    message["params"]["mode"] = json!(9);
    assert!(parse_mcp_server_elicitation_request(&message, 3).is_err());
    let mut message = string_id.clone();
    message["id"] = json!(1.5);
    assert!(parse_mcp_server_elicitation_request(&message, 3).is_err());
}

#[test]
fn url_mode_preserves_lifecycle_fields_and_rejects_non_http() {
    let message = json!({
        "id": 5,
        "method": "mcpServer/elicitation/request",
        "params": {
            "serverName": "remote-mcp",
            "threadId": "thr-2",
            "turnId": null,
            "mode": "url",
            "elicitationId": "elicit-7",
            "message": "登录后回来",
            "url": "https://auth.example.com/device?code=42",
            "_meta": { "anything": true }
        }
    });
    let request = parse_mcp_server_elicitation_request(&message, 4).unwrap();
    let AgentMcpElicitationMode::Url(url) = &request.mode else {
        panic!("expected url mode");
    };
    assert_eq!(url.elicitation_id, "elicit-7");
    assert_eq!(url.message, "登录后回来");
    assert_eq!(url.url, "https://auth.example.com/device?code=42");

    for invalid in ["javascript:alert(1)", "file:///etc/passwd", "not a url"] {
        let mut message = message.clone();
        message["params"]["url"] = json!(invalid);
        assert!(
            parse_mcp_server_elicitation_request(&message, 4).is_err(),
            "accepted {invalid}"
        );
    }
    let mut message = message.clone();
    message["params"]["elicitationId"] = json!(null);
    assert!(parse_mcp_server_elicitation_request(&message, 4).is_err());
}

#[test]
fn unsupported_and_unknown_modes_are_protocol_errors() {
    for mode in ["openai/form", "openaiForm", "openai/userVerification"] {
        let message = form_request(json!({
            "serverName": "fixture",
            "threadId": "thr-1",
            "mode": mode,
            "message": "m",
            "requestedSchema": { "type": "object", "properties": {} }
        }));
        let error = parse_mcp_server_elicitation_request(&message, 1).unwrap_err();
        assert!(
            error.to_string().contains("尚未接入"),
            "mode {mode}: {error:#}"
        );
    }
    let message = form_request(json!({
        "serverName": "fixture",
        "threadId": "thr-1",
        "mode": "graphics",
        "message": "m",
        "requestedSchema": { "type": "object", "properties": {} }
    }));
    assert!(parse_mcp_server_elicitation_request(&message, 1).is_err());
}

#[test]
fn requested_schema_rejects_malformed_or_unknown_members() {
    let cases = [
        json!({ "type": "array", "properties": {} }),
        json!({ "properties": {} }),
        json!({ "type": "object" }),
        json!({ "type": "object", "properties": {}, "extra": true }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "string" } },
            "required": ["missing"]
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "string", "minLength": 5, "maxLength": 2 } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "string", "default": "x", "minLength": 5 } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "string", "enum": [] } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "string", "enum": ["x", "x"] } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "string", "enum": ["x"], "enumNames": ["x", "y"] } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "string", "format": "ipv4" } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "integer", "default": 1.5 } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "integer", "minimum": 5, "maximum": 1 } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "array", "items": { "type": "string" } } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "object" } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "string", "enum": ["x"], "oneOf": [{"const": "y", "title": "y"}] } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "string", "unknownKeyword": 1 } }
        }),
        json!({
            "type": "object",
            "properties": { "a": { "type": "array", "items": { "type": "string", "enum": ["x"] }, "default": ["y"] } }
        }),
    ];
    for case in cases {
        let error = parse_mcp_server_elicitation_request(&request_with_schema(case.clone()), 1)
            .unwrap_err();
        assert!(
            error.to_string().contains("requestedSchema")
                || error.to_string().contains("properties")
                || error.to_string().contains("items"),
            "case {case}: {error:#}"
        );
    }
}

#[test]
fn decline_and_cancel_never_carry_content() {
    let request = parse_mcp_server_elicitation_request(
        &request_with_schema(json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"]
        })),
        1,
    )
    .unwrap();
    for response in [
        AgentMcpElicitationResponse::decline(),
        AgentMcpElicitationResponse::cancel(),
    ] {
        let result = elicitation_response_result(&request, &response).unwrap();
        assert!(result.get("content").is_none(), "{result}");
        let action = match response.action {
            AgentMcpElicitationAction::Decline => "decline",
            AgentMcpElicitationAction::Cancel => "cancel",
            AgentMcpElicitationAction::Accept => unreachable!(),
        };
        assert_eq!(result, json!({ "action": action }));
    }
    let forged = AgentMcpElicitationResponse {
        action: AgentMcpElicitationAction::Cancel,
        content: Some(AgentMcpElicitationContent::default()),
    };
    assert!(elicitation_response_result(&request, &forged).is_err());
}

#[test]
fn accept_validates_required_fields_types_and_ranges() {
    let request =
        parse_mcp_server_elicitation_request(&request_with_schema(json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 2 },
                "replicas": { "type": "integer", "minimum": 1, "maximum": 4 },
                "enabled": { "type": "boolean" },
                "region": { "type": "string", "enum": ["us", "eu"] },
                "features": { "type": "array", "minItems": 1, "maxItems": 2, "items": { "type": "string", "enum": ["logs", "metrics", "traces"] } }
            },
            "required": ["name", "replicas"]
        })), 1)
        .unwrap();

    let missing = AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
        fields: vec![AgentMcpElicitationFieldValue {
            name: "name".into(),
            value: AgentMcpElicitationValue::String("echora".into()),
        }],
    });
    let error = elicitation_response_result(&request, &missing).unwrap_err();
    assert!(error.to_string().contains("replicas"), "{error:#}");

    let accept = AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
        fields: vec![
            AgentMcpElicitationFieldValue {
                name: "name".into(),
                value: AgentMcpElicitationValue::String("echora".into()),
            },
            AgentMcpElicitationFieldValue {
                name: "replicas".into(),
                value: AgentMcpElicitationValue::Number(serde_json::Number::from(3)),
            },
            AgentMcpElicitationFieldValue {
                name: "enabled".into(),
                value: AgentMcpElicitationValue::Boolean(true),
            },
            AgentMcpElicitationFieldValue {
                name: "region".into(),
                value: AgentMcpElicitationValue::String("eu".into()),
            },
            AgentMcpElicitationFieldValue {
                name: "features".into(),
                value: AgentMcpElicitationValue::StringArray(vec!["logs".into()]),
            },
        ],
    });
    let result = elicitation_response_result(&request, &accept).unwrap();
    assert_eq!(
        result,
        json!({
            "action": "accept",
            "content": {
                "name": "echora",
                "replicas": 3,
                "enabled": true,
                "region": "eu",
                "features": ["logs"]
            }
        })
    );

    let rejected = [
        // wrong wire type for an integer field
        AgentMcpElicitationValue::String("3".into()),
        // integer schema rejects a fractional number
        AgentMcpElicitationValue::Number(serde_json::Number::from_f64(3.5).unwrap()),
        // outside minimum/maximum
        AgentMcpElicitationValue::Number(serde_json::Number::from(9)),
    ];
    for value in rejected {
        let response = AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
            fields: vec![
                AgentMcpElicitationFieldValue {
                    name: "name".into(),
                    value: AgentMcpElicitationValue::String("echora".into()),
                },
                AgentMcpElicitationFieldValue {
                    name: "replicas".into(),
                    value,
                },
            ],
        });
        assert!(elicitation_response_result(&request, &response).is_err());
    }

    let bad_option = AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
        fields: vec![
            AgentMcpElicitationFieldValue {
                name: "name".into(),
                value: AgentMcpElicitationValue::String("echora".into()),
            },
            AgentMcpElicitationFieldValue {
                name: "replicas".into(),
                value: AgentMcpElicitationValue::Number(serde_json::Number::from(2)),
            },
            AgentMcpElicitationFieldValue {
                name: "region".into(),
                value: AgentMcpElicitationValue::String("mars".into()),
            },
        ],
    });
    assert!(elicitation_response_result(&request, &bad_option).is_err());

    for features in [
        vec![],
        vec!["logs".to_owned(), "metrics".to_owned(), "traces".to_owned()],
        vec!["logs".to_owned(), "logs".to_owned()],
        vec!["unknown".to_owned()],
    ] {
        let response = AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
            fields: vec![
                AgentMcpElicitationFieldValue {
                    name: "name".into(),
                    value: AgentMcpElicitationValue::String("echora".into()),
                },
                AgentMcpElicitationFieldValue {
                    name: "replicas".into(),
                    value: AgentMcpElicitationValue::Number(serde_json::Number::from(2)),
                },
                AgentMcpElicitationFieldValue {
                    name: "features".into(),
                    value: AgentMcpElicitationValue::StringArray(features.clone()),
                },
            ],
        });
        assert!(
            elicitation_response_result(&request, &response).is_err(),
            "accepted features {features:?}"
        );
    }

    let unknown_field = AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
        fields: vec![AgentMcpElicitationFieldValue {
            name: "missing".into(),
            value: AgentMcpElicitationValue::Boolean(true),
        }],
    });
    assert!(elicitation_response_result(&request, &unknown_field).is_err());

    let empty_required = AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
        fields: vec![
            AgentMcpElicitationFieldValue {
                name: "name".into(),
                value: AgentMcpElicitationValue::String("   ".into()),
            },
            AgentMcpElicitationFieldValue {
                name: "replicas".into(),
                value: AgentMcpElicitationValue::Number(serde_json::Number::from(2)),
            },
        ],
    });
    assert!(elicitation_response_result(&request, &empty_required).is_err());
}

#[test]
fn url_accept_never_carries_form_content() {
    let message = json!({
        "id": 5,
        "method": "mcpServer/elicitation/request",
        "params": {
            "serverName": "remote-mcp",
            "threadId": "thr-2",
            "mode": "url",
            "elicitationId": "elicit-7",
            "message": "登录后回来",
            "url": "https://auth.example.com/device"
        }
    });
    let request = parse_mcp_server_elicitation_request(&message, 1).unwrap();
    let accept = AgentMcpElicitationResponse::accept(AgentMcpElicitationContent::default());
    assert_eq!(
        elicitation_response_result(&request, &accept).unwrap(),
        json!({ "action": "accept" })
    );
    let forged = AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
        fields: vec![AgentMcpElicitationFieldValue {
            name: "email".into(),
            value: AgentMcpElicitationValue::String("a@b.c".into()),
        }],
    });
    assert!(elicitation_response_result(&request, &forged).is_err());
}

#[test]
fn format_constraints_are_enforced_for_accept() {
    let request = parse_mcp_server_elicitation_request(
        &request_with_schema(json!({
            "type": "object",
            "properties": {
                "contact": { "type": "string", "format": "email" },
                "when": { "type": "string", "format": "date" },
                "stamp": { "type": "string", "format": "date-time" },
                "site": { "type": "string", "format": "uri" }
            },
            "required": ["contact"]
        })),
        1,
    )
    .unwrap();
    let response = |name: &str, value: &str| {
        AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
            fields: vec![
                AgentMcpElicitationFieldValue {
                    name: "contact".into(),
                    value: AgentMcpElicitationValue::String("dev@example.com".into()),
                },
                AgentMcpElicitationFieldValue {
                    name: name.into(),
                    value: AgentMcpElicitationValue::String(value.into()),
                },
            ],
        })
    };
    assert!(elicitation_response_result(&request, &response("when", "2026-09-13")).is_ok());
    assert!(elicitation_response_result(&request, &response("when", "13/09/2026")).is_err());
    assert!(
        elicitation_response_result(&request, &response("stamp", "2026-09-13T08:00:00Z")).is_ok()
    );
    assert!(elicitation_response_result(&request, &response("stamp", "2026-09-13")).is_err());
    assert!(elicitation_response_result(&request, &response("site", "https://echora.dev")).is_ok());
    assert!(elicitation_response_result(&request, &response("site", "echora")).is_err());
    let bad_email = AgentMcpElicitationResponse::accept(AgentMcpElicitationContent {
        fields: vec![AgentMcpElicitationFieldValue {
            name: "contact".into(),
            value: AgentMcpElicitationValue::String("not-an-email".into()),
        }],
    });
    assert!(elicitation_response_result(&request, &bad_email).is_err());
}

#[test]
fn top_level_params_and_form_members_reject_unknown_fields() {
    let message = form_request(json!({
        "serverName": "fixture",
        "threadId": "thr-1",
        "mode": "form",
        "message": "m",
        "requestedSchema": { "type": "object", "properties": {} },
        "extraField": 1
    }));
    assert!(parse_mcp_server_elicitation_request(&message, 1).is_err());

    let message = form_request(json!({
        "serverName": "fixture",
        "threadId": "thr-1",
        "mode": "form",
        "message": "m",
        "requestedSchema": { "type": "object", "properties": {} },
        "instructions": "nope"
    }));
    assert!(parse_mcp_server_elicitation_request(&message, 1).is_err());

    let message = form_request(json!({
        "serverName": "fixture",
        "threadId": "thr-1",
        "mode": "form",
        "requestedSchema": { "type": "object", "properties": {} }
    }));
    assert!(parse_mcp_server_elicitation_request(&message, 1).is_err());
}
