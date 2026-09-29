//! Payloads are the baseline CLI's answers recorded by
//! `scripts/batch2_app_server_probe.py --scenario search`
//! (artifacts/batch2-baseline-*/search.wire.json), cursors shortened.

use serde_json::json;

use super::*;

const CURSOR: &str =
    r#"{"requestedThreadId":"t","rolloutOrdinal":1,"includeAnchor":true,"scope":{"kind":"turns"}}"#;

fn occurrence(item: &str, snippet: &str, start: u64, end: u64) -> serde_json::Value {
    json!({"turnId": "turn-1", "itemId": item, "snippet": snippet,
           "snippetMatchRange": {"start": start, "end": end}, "turnCursor": CURSOR})
}

#[test]
fn params_carry_the_cursor_only_when_continuing() {
    let mut request = AgentThreadOccurrenceRequest {
        thread_id: "t".into(),
        search_term: "hello".into(),
        cursor: None,
        limit: 250,
    };
    assert_eq!(
        params(&request),
        json!({"threadId": "t", "searchTerm": "hello", "limit": 250})
    );
    request.cursor = Some("next".into());
    assert_eq!(params(&request)["cursor"], "next");
}

#[test]
fn every_occurrence_of_an_item_decodes_with_byte_ranges() {
    let response = json!({"id": 4, "result": {"data": [
        occurrence("u1", "Hello world, hello again REPLY1", 0, 5),
        occurrence("u1", "Hello world, hello again REPLY1", 13, 18),
        occurrence("msg_resp_1", "HELLO from the assistant 🙂 hello", 28, 33),
        occurrence("u2", "你好🙂世界，你好 REPLY2", 7, 9),
    ], "nextCursor": r#"{"threadId":"t","searchTerm":"hello","nextRolloutOrdinal":8,"nextOccurrenceIndex":0}"#}});
    let (occurrences, next) = parse_page(&response).unwrap();
    assert!(next.is_some());
    assert_eq!(
        occurrences
            .iter()
            .map(AgentThreadOccurrence::matched_text)
            .collect::<Vec<_>>(),
        ["Hello", "hello", "hello", "你好"]
    );
    assert_eq!(occurrences[0].turn_cursor, CURSOR);
    let (_, last) =
        parse_page(&json!({"id": 5, "result": {"data": [], "nextCursor": null}})).unwrap();
    assert_eq!(last, None);
}

#[test]
fn broken_ranges_and_missing_fields_are_rejected() {
    let page = |item: serde_json::Value| json!({"id": 1, "result": {"data": [item]}});
    // Half of the emoji's surrogate pair.
    assert!(parse_page(&page(occurrence("m", "a 🙂 b", 2, 3))).is_err());
    assert!(parse_page(&page(occurrence("m", "hello", 3, 2))).is_err());
    assert!(parse_page(&page(occurrence("m", "hello", 0, 9))).is_err());
    assert!(parse_page(&page(occurrence("m", "hello", 2, 2))).is_err());
    let mut missing = occurrence("m", "hello", 0, 5);
    missing.as_object_mut().unwrap().remove("turnCursor");
    assert!(parse_page(&page(missing)).is_err());
    assert!(parse_page(&json!({"id": 1, "result": {}})).is_err());
}
