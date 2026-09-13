use std::sync::{Arc, Mutex};

use gpui::TestApp;

use super::*;
use crate::{
    agent::{
        AgentConnectionEvent, AgentMcpElicitationAction, AgentMcpElicitationControl,
        AgentMcpElicitationField, AgentMcpElicitationFieldKind, AgentMcpElicitationFieldValue,
        AgentMcpElicitationForm, AgentMcpElicitationHandle, AgentMcpElicitationIdentity,
        AgentMcpElicitationMode, AgentMcpElicitationOption, AgentMcpElicitationRequest,
        AgentMcpElicitationResponse, AgentMcpElicitationUrl, AgentMcpElicitationValue,
        AgentOptionalField, AgentServerRequestFailureKind, AgentServerRequestId,
    },
    components::mcp_elicitation::{McpElicitationEvent, McpElicitationFocus, McpElicitationStatus},
    conversation::ConversationActivity,
};

#[derive(Default)]
struct Replies {
    responses: Mutex<Vec<AgentMcpElicitationResponse>>,
    failure: Option<String>,
}

impl AgentMcpElicitationControl for Replies {
    fn respond(
        &self,
        _: &AgentMcpElicitationIdentity,
        response: AgentMcpElicitationResponse,
    ) -> Result<(), String> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        self.responses.lock().unwrap().push(response);
        Ok(())
    }
}

fn form_fields() -> Vec<AgentMcpElicitationField> {
    vec![
        AgentMcpElicitationField {
            name: "name".into(),
            title: Some("名称".into()),
            description: Some("部署名称".into()),
            required: true,
            kind: AgentMcpElicitationFieldKind::String {
                format: None,
                min_length: None,
                max_length: None,
            },
            default: None,
        },
        AgentMcpElicitationField {
            name: "replicas".into(),
            title: Some("副本数".into()),
            description: None,
            required: false,
            kind: AgentMcpElicitationFieldKind::Number {
                integer: true,
                minimum: Some(serde_json::Number::from(1)),
                maximum: Some(serde_json::Number::from(8)),
            },
            default: Some(AgentMcpElicitationValue::Number(serde_json::Number::from(
                2,
            ))),
        },
        AgentMcpElicitationField {
            name: "enabled".into(),
            title: Some("启用".into()),
            description: None,
            required: false,
            kind: AgentMcpElicitationFieldKind::Boolean,
            default: None,
        },
        AgentMcpElicitationField {
            name: "region".into(),
            title: Some("区域".into()),
            description: None,
            required: true,
            kind: AgentMcpElicitationFieldKind::SingleSelect {
                options: vec![
                    AgentMcpElicitationOption {
                        value: "us".into(),
                        title: "美东".into(),
                    },
                    AgentMcpElicitationOption {
                        value: "eu".into(),
                        title: "西欧".into(),
                    },
                ],
            },
            default: None,
        },
        AgentMcpElicitationField {
            name: "features".into(),
            title: Some("功能".into()),
            description: None,
            required: false,
            kind: AgentMcpElicitationFieldKind::MultiSelect {
                options: vec![
                    AgentMcpElicitationOption {
                        value: "logs".into(),
                        title: "日志".into(),
                    },
                    AgentMcpElicitationOption {
                        value: "metrics".into(),
                        title: "指标".into(),
                    },
                ],
                min_items: None,
                max_items: Some(1),
            },
            default: None,
        },
    ]
}

fn form_request(id: AgentServerRequestId) -> AgentMcpElicitationRequest {
    AgentMcpElicitationRequest {
        generation: 1,
        request_id: id,
        server_name: "fixture-mcp".into(),
        thread_id: "thread-a".into(),
        turn_id: AgentOptionalField::Unspecified,
        mode: AgentMcpElicitationMode::Form(AgentMcpElicitationForm {
            message: "请填写部署信息".into(),
            fields: form_fields(),
        }),
    }
}

fn url_request(id: AgentServerRequestId) -> AgentMcpElicitationRequest {
    AgentMcpElicitationRequest {
        generation: 1,
        request_id: id,
        server_name: "fixture-mcp".into(),
        thread_id: "thread-a".into(),
        turn_id: AgentOptionalField::Null,
        mode: AgentMcpElicitationMode::Url(AgentMcpElicitationUrl {
            elicitation_id: "elicit-url-1".into(),
            message: "请在浏览器完成登录".into(),
            url: "https://example.com/device".into(),
        }),
    }
}

fn request_event(
    request: AgentMcpElicitationRequest,
    control: &Arc<Replies>,
) -> (String, AgentConnectionEvent) {
    let key = request.identity().ui_key();
    let control: Arc<dyn AgentMcpElicitationControl> = control.clone();
    let responder = AgentMcpElicitationHandle::new(request.identity(), control);
    (
        key,
        AgentConnectionEvent::McpElicitationRequested { request, responder },
    )
}

fn model<'a>(
    composer: &'a ComposerView,
    key: &str,
) -> &'a crate::components::mcp_elicitation::McpElicitationPresentation {
    composer
        .conversation
        .activities
        .iter()
        .find_map(|activity| match activity {
            ConversationActivity::McpElicitation(model) if model.request_id == key => {
                Some(model.as_ref())
            }
            _ => None,
        })
        .expect("elicitation card")
}

fn responses(control: &Arc<Replies>) -> Vec<AgentMcpElicitationResponse> {
    control.responses.lock().unwrap().clone()
}

#[test]
fn form_submission_validates_locally_and_waits_for_the_server_resolution() {
    let mut app = TestApp::new();
    let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
    let control = Arc::new(Replies::default());
    app.update_entity(&composer, |composer, cx| {
        composer.conversation.thread_id = Some("thread-a".into());
        let (key, event) = request_event(form_request(AgentServerRequestId::Number(11)), &control);
        assert!(composer.apply_connection_event(event));
        assert!(composer.focused_mcp_elicitation_request_id().as_deref() == Some(key.as_str()));

        // Empty required text, illegal number, and out-of-range values stay
        // pending with per-field errors and never write a response.
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Accept, cx);
        assert!(responses(&control).is_empty());
        assert_eq!(model(composer, &key).status, McpElicitationStatus::Pending);
        assert!(model(composer, &key).field(0).unwrap().error.is_some());

        composer.handle_mcp_elicitation_event(
            &key,
            McpElicitationEvent::Focus(McpElicitationFocus::Field(0)),
            cx,
        );
        let name = model(composer, &key).field(0).unwrap().name.clone();
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Accept, cx);
        assert!(responses(&control).is_empty());
        assert!(model(composer, &key).field(0).unwrap().error.is_some());
        assert_eq!(name, "name");

        composer.set_focused_mcp_elicitation_text("echora".into());
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Accept, cx);
        assert!(responses(&control).is_empty(), "region is required");
        assert!(model(composer, &key).field(3).unwrap().error.is_some());

        composer.handle_mcp_elicitation_event(
            &key,
            McpElicitationEvent::SelectOption {
                field: 3,
                option: 1,
            },
            cx,
        );
        composer.handle_mcp_elicitation_event(
            &key,
            McpElicitationEvent::ToggleBoolean { field: 2 },
            cx,
        );
        composer.handle_mcp_elicitation_event(
            &key,
            McpElicitationEvent::ToggleMultiOption {
                field: 4,
                option: 0,
            },
            cx,
        );
        composer.handle_mcp_elicitation_event(
            &key,
            McpElicitationEvent::ToggleMultiOption {
                field: 4,
                option: 1,
            },
            cx,
        );
        // maxItems=1 is a validation rule, not a silent truncation.
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Accept, cx);
        assert!(responses(&control).is_empty());
        assert!(model(composer, &key).field(4).unwrap().error.is_some());
        composer.handle_mcp_elicitation_event(
            &key,
            McpElicitationEvent::ToggleMultiOption {
                field: 4,
                option: 1,
            },
            cx,
        );
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Accept, cx);
        let sent = responses(&control);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].action, AgentMcpElicitationAction::Accept);
        let content = sent[0].content.as_ref().unwrap();
        assert_eq!(
            content.fields,
            vec![
                AgentMcpElicitationFieldValue {
                    name: "name".into(),
                    value: AgentMcpElicitationValue::String("echora".into()),
                },
                AgentMcpElicitationFieldValue {
                    name: "replicas".into(),
                    value: AgentMcpElicitationValue::Number(serde_json::Number::from(2)),
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
            ]
        );
        // Submitted but not yet resolved: the card must not claim completion.
        assert_eq!(
            model(composer, &key).status,
            McpElicitationStatus::Submitting
        );
        assert!(model(composer, &key).status.is_overlay_visible());
        // A second submit never writes twice.
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Accept, cx);
        assert_eq!(responses(&control).len(), 1);

        let identity = composer.conversation.mcp_elicitation_contexts[&key].clone();
        assert!(
            composer.apply_connection_event(AgentConnectionEvent::McpElicitationResolved {
                identity,
                thread_id: "thread-a".into(),
            })
        );
        assert_eq!(model(composer, &key).status, McpElicitationStatus::Accepted);
        assert!(!model(composer, &key).status.is_overlay_visible());
        assert!(composer.conversation.mcp_elicitation_responders.is_empty());
    });
}

#[test]
fn decline_and_cancel_send_distinct_protocol_actions() {
    let mut app = TestApp::new();
    let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
    let control = Arc::new(Replies::default());
    app.update_entity(&composer, |composer, cx| {
        composer.conversation.thread_id = Some("thread-a".into());
        let (decline_key, decline) =
            request_event(form_request(AgentServerRequestId::Number(21)), &control);
        let (cancel_key, cancel) =
            request_event(form_request(AgentServerRequestId::Number(22)), &control);
        composer.apply_connection_event(decline);
        composer.apply_connection_event(cancel);

        composer.handle_mcp_elicitation_event(&decline_key, McpElicitationEvent::Decline, cx);
        composer.handle_mcp_elicitation_event(&cancel_key, McpElicitationEvent::Cancel, cx);
        let sent = responses(&control);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].action, AgentMcpElicitationAction::Decline);
        assert!(sent[0].content.is_none());
        assert_eq!(sent[1].action, AgentMcpElicitationAction::Cancel);
        assert!(sent[1].content.is_none());
        assert_eq!(
            model(composer, &decline_key).status,
            McpElicitationStatus::Submitting
        );
        assert_eq!(
            model(composer, &cancel_key).status,
            McpElicitationStatus::Submitting
        );

        let decline_identity = composer.conversation.mcp_elicitation_contexts[&decline_key].clone();
        composer.apply_connection_event(AgentConnectionEvent::McpElicitationResolved {
            identity: decline_identity,
            thread_id: "thread-a".into(),
        });
        assert_eq!(
            model(composer, &decline_key).status,
            McpElicitationStatus::Declined
        );
        // The other card keeps waiting for its own resolution.
        assert_eq!(
            model(composer, &cancel_key).status,
            McpElicitationStatus::Submitting
        );
        let cancel_identity = composer.conversation.mcp_elicitation_contexts[&cancel_key].clone();
        composer.apply_connection_event(AgentConnectionEvent::McpElicitationResolved {
            identity: cancel_identity,
            thread_id: "thread-a".into(),
        });
        assert_eq!(
            model(composer, &cancel_key).status,
            McpElicitationStatus::Cancelled
        );
        assert_eq!(responses(&control).len(), 2);
    });
}

#[test]
fn url_cards_open_the_link_without_answering_and_keep_the_explicit_action() {
    let mut app = TestApp::new();
    let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
    let control = Arc::new(Replies::default());
    app.update_entity(&composer, |composer, cx| {
        composer.conversation.thread_id = Some("thread-a".into());
        let (key, event) = request_event(url_request(AgentServerRequestId::Number(31)), &control);
        composer.apply_connection_event(event);
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::OpenUrl, cx);
        assert!(
            responses(&control).is_empty(),
            "opening a link is not an answer"
        );
        let (_, url, opened) = model(composer, &key).url().unwrap();
        assert_eq!(url, "https://example.com/device");
        assert!(opened);
        assert_eq!(model(composer, &key).status, McpElicitationStatus::Pending);

        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Accept, cx);
        let sent = responses(&control);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].action, AgentMcpElicitationAction::Accept);
        assert_eq!(
            sent[0].content.as_ref().map(|content| content.fields.len()),
            Some(0)
        );
    });
}

#[test]
fn connection_failure_invalidates_the_card_and_blocks_further_submits() {
    let mut app = TestApp::new();
    let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
    let control = Arc::new(Replies::default());
    app.update_entity(&composer, |composer, cx| {
        composer.conversation.thread_id = Some("thread-a".into());
        let (key, event) = request_event(form_request(AgentServerRequestId::Number(41)), &control);
        composer.apply_connection_event(event);
        let identity = composer.conversation.mcp_elicitation_contexts[&key].clone();
        composer.apply_connection_event(AgentConnectionEvent::McpElicitationFailed {
            identity,
            thread_id: "thread-a".into(),
            kind: AgentServerRequestFailureKind::Failed,
            message: "连接已断开，等待中的 MCP elicitation 不再可回复".into(),
        });
        assert_eq!(model(composer, &key).status, McpElicitationStatus::Invalid);
        assert!(!model(composer, &key).status.is_overlay_visible());
        assert!(model(composer, &key).failure_message.is_some());
        assert!(composer.focused_mcp_elicitation_request_id().is_none());

        // Any later user action must not reach the retired responder.
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Accept, cx);
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Cancel, cx);
        assert!(responses(&control).is_empty());
        assert_eq!(model(composer, &key).status, McpElicitationStatus::Invalid);
    });
}

#[test]
fn write_failure_is_visible_and_never_retried() {
    let mut app = TestApp::new();
    let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
    let control = Arc::new(Replies {
        failure: Some("写入 MCP elicitation 的 JSON-RPC response 失败；请勿重复提交".into()),
        ..Default::default()
    });
    app.update_entity(&composer, |composer, cx| {
        composer.conversation.thread_id = Some("thread-a".into());
        let (key, event) = request_event(url_request(AgentServerRequestId::Number(51)), &control);
        composer.apply_connection_event(event);
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Decline, cx);
        let model = model(composer, &key);
        assert_eq!(model.status, McpElicitationStatus::Invalid);
        assert!(
            model
                .failure_message
                .as_deref()
                .is_some_and(|message| message.contains("请勿重复提交"))
        );
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Decline, cx);
        assert!(responses(&control).is_empty());
    });
}

#[test]
fn keyboard_order_covers_fields_and_buttons_and_escape_cancels() {
    let mut app = TestApp::new();
    let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
    let control = Arc::new(Replies::default());
    app.update_entity(&composer, |composer, cx| {
        composer.conversation.thread_id = Some("thread-a".into());
        let (key, event) = request_event(form_request(AgentServerRequestId::Number(61)), &control);
        composer.apply_connection_event(event);
        assert_eq!(
            model(composer, &key).keyboard_focus,
            Some(McpElicitationFocus::Field(0))
        );
        let targets = model(composer, &key).focus_targets();
        assert_eq!(targets.len(), 5 + 3);
        assert_eq!(targets[0], McpElicitationFocus::Field(0));
        assert_eq!(targets[4], McpElicitationFocus::Field(4));
        // Tab order follows the reference card: close (cancel) first, then the
        // footer's skip (decline) and continue (accept).
        assert_eq!(targets[5], McpElicitationFocus::Cancel);
        assert_eq!(targets[6], McpElicitationFocus::Decline);
        assert_eq!(targets[7], McpElicitationFocus::Accept);
        for expected in [
            McpElicitationFocus::Field(1),
            McpElicitationFocus::Field(2),
            McpElicitationFocus::Field(3),
            McpElicitationFocus::Field(4),
            McpElicitationFocus::Cancel,
            McpElicitationFocus::Decline,
            McpElicitationFocus::Accept,
            McpElicitationFocus::Field(0),
        ] {
            let next =
                model(composer, &key).next_focus(model(composer, &key).keyboard_focus, false);
            composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Focus(next), cx);
            assert_eq!(model(composer, &key).keyboard_focus, Some(expected));
        }

        // Escape is the explicit protocol cancel, not a decline.
        composer.handle_mcp_elicitation_event(&key, McpElicitationEvent::Cancel, cx);
        let sent = responses(&control);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].action, AgentMcpElicitationAction::Cancel);
    });
}
