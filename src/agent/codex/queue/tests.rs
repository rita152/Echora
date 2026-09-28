//! Queue codec. Payloads follow the reference wire log
//! (artifacts/batch1-queue-*/wire) and the baseline probe.

use serde_json::json;

use super::*;
use crate::agent::UserMessageAttachment;

fn submission(id: &str, client: &str, text: &str) -> serde_json::Value {
    json!({"id": id, "clientUserMessageId": client, "input": [{"type":"text","text":text,"text_elements":[]}]})
}

#[test]
fn queued_input_decodes_like_the_user_message_it_becomes() {
    let parsed = parse_submission(&json!({
        "id": "q1", "clientUserMessageId": "c1",
        "input": [
            {"type":"text","text":"Reply with exactly: A\n","text_elements":[]},
            {"type":"localImage","path":"/tmp/capture.png"}
        ]
    }))
    .unwrap();
    assert_eq!(parsed.id, "q1");
    assert_eq!(parsed.client_message_id, "c1");
    assert_eq!(parsed.text.trim_end(), "Reply with exactly: A");
    assert_eq!(
        parsed.attachments,
        vec![UserMessageAttachment::Local("/tmp/capture.png".into())]
    );
    assert!(
        parse_submission(&json!({"id":"q","clientUserMessageId":"c","input":[{"type":"video"}]}))
            .is_err()
    );
    assert!(parse_submission(&json!({"id":"q","input":[]})).is_err());
    assert!(parse_submission(&json!({"id":"q","clientUserMessageId":"c","input":{}})).is_err());
}

#[test]
fn responses_are_bound_to_their_request() {
    let added = json!({"id":1,"result":{"queuedSubmission":submission("q1","c1","A")}});
    assert_eq!(parse_add_response(&added, "c1").unwrap().id, "q1");
    assert!(parse_add_response(&added, "c2").is_err());
    let updated = json!({"id":2,"result":{"queuedSubmission":submission("q1","c1","A edited")}});
    assert_eq!(
        parse_update_response(&updated, "q1").unwrap().text,
        "A edited"
    );
    assert!(parse_update_response(&updated, "q2").is_err());
    assert!(parse_delete_response(&json!({"id":3,"result":{"deleted":true}})).unwrap());
    assert!(!parse_delete_response(&json!({"id":3,"result":{"deleted":false}})).unwrap());
    assert!(parse_delete_response(&json!({"id":3,"result":{}})).is_err());
    assert!(parse_reorder_response(&json!({"id":4,"result":{}})).is_ok());
    assert!(parse_reorder_response(&json!({"id":4,"result":null})).is_err());
    assert_eq!(
        parse_start_response(
            &json!({"id":5,"result":{"turn":{"id":"t9","items":[],"status":"inProgress"}}})
        )
        .unwrap(),
        "t9"
    );
    assert!(parse_start_response(&json!({"id":5,"result":{}})).is_err());
}

#[test]
fn list_pages_follow_cursors_and_reject_loops_and_duplicates() {
    let page = |data: Vec<serde_json::Value>, next: serde_json::Value| {
        parse_list_page(&json!({"id":1,"result":{"data":data,"nextCursor":next}})).unwrap()
    };
    let mut pages = QueueListAccumulator::default();
    let (data, next) = page(vec![submission("q1", "c1", "A")], json!("1"));
    assert_eq!(pages.push(data, next).unwrap(), Some("1".into()));
    let (data, next) = page(vec![submission("q2", "c2", "B")], json!(null));
    assert_eq!(pages.push(data, next).unwrap(), None);
    assert_eq!(pages.submissions.len(), 2);

    let mut repeated = QueueListAccumulator::default();
    repeated.push(vec![], Some("1".into())).unwrap();
    assert!(repeated.push(vec![], Some("1".into())).is_err());

    let mut duplicate = QueueListAccumulator::default();
    let (data, _) = page(vec![submission("q1", "c1", "A")], json!(null));
    duplicate.push(data.clone(), Some("a".into())).unwrap();
    assert!(duplicate.push(data, None).is_err());

    assert!(parse_list_page(&json!({"id":1,"result":{"data":[],"nextCursor":1}})).is_err());
    assert!(parse_list_page(&json!({"id":1,"result":{}})).is_err());
    assert_eq!(
        list_params("thread", Some("1")),
        json!({"threadId":"thread","cursor":"1"})
    );
}

#[test]
fn changed_is_a_thread_scoped_notification() {
    assert_eq!(
        parse_changed(
            &json!({"method":"thread/queue/changed","params":{"threadId":"t"},"emittedAtMs":1})
        )
        .unwrap(),
        "t"
    );
    assert!(parse_changed(&json!({"method":"thread/queue/changed","params":{}})).is_err());
    assert!(
        parse_changed(&json!({"id":1,"method":"thread/queue/changed","params":{"threadId":"t"}}))
            .is_err()
    );
}
