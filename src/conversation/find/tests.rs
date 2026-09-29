//! Pages mirror `scripts/batch2_app_server_probe.py --scenario search`
//! (artifacts/batch2-baseline-*/search.wire.json): "hello" with limit 2
//! returned three pages of 2, 2 and 1 occurrences.

use super::*;

fn occurrence(item: &str, snippet: &str, range: std::ops::Range<usize>) -> AgentThreadOccurrence {
    AgentThreadOccurrence {
        item_id: item.into(),
        turn_id: "turn-1".into(),
        turn_cursor: "cursor".into(),
        snippet: snippet.into(),
        snippet_match: range,
    }
}

fn page(occurrences: Vec<AgentThreadOccurrence>, next: Option<&str>) -> AgentThreadOccurrencePage {
    AgentThreadOccurrencePage {
        generation: 1,
        occurrences,
        next_cursor: next.map(Into::into),
    }
}

fn searching() -> (FindState, u64) {
    let mut find = FindState::default();
    find.open(Some("t".into()));
    let cycle = find.set_query("hello ".into()).unwrap();
    assert_eq!(find.search_term(), "hello");
    (find, cycle)
}

#[test]
fn pages_are_read_on_demand_and_the_count_shows_more() {
    let (mut find, cycle) = searching();
    let user = "Hello world, hello again REPLY1";
    assert_eq!(
        find.accept_page(
            cycle,
            "t",
            page(
                vec![occurrence("u1", user, 0..5), occurrence("u1", user, 13..18)],
                Some("c1")
            )
        ),
        FindStep::Reveal
    );
    assert_eq!(find.active, Some(0));
    assert_eq!(find.matches[1].ordinal_in_item, 1);
    assert_eq!(
        find.count_label().unwrap(),
        crate::i18n::format!("1 / 2+ 个结果" => "1 / 2+ results")
    );
    assert_eq!(find.next(), FindStep::Reveal);
    // Past the last loaded match the next page is read, then the step lands.
    assert_eq!(find.next(), FindStep::LoadMore("c1".into()));
    assert_eq!(find.next(), FindStep::Nothing, "one read at a time");
    let reply = "HELLO from the assistant 🙂 hello";
    assert_eq!(
        find.accept_page(
            cycle,
            "t",
            page(
                vec![
                    occurrence("m1", reply, 0..5),
                    occurrence("m1", reply, 29..34)
                ],
                Some("c2")
            )
        ),
        FindStep::Reveal
    );
    assert_eq!(find.active, Some(2));
    assert_eq!(find.matches[3].ordinal_in_item, 1);
    find.next();
    assert_eq!(find.next(), FindStep::LoadMore("c2".into()));
    find.accept_page(
        cycle,
        "t",
        page(vec![occurrence("m3", "回答：你好 😀 hello", 15..20)], None),
    );
    assert_eq!(find.active, Some(4));
    assert_eq!(
        find.count_label().unwrap(),
        crate::i18n::format!("5 / 5 个结果" => "5 / 5 results")
    );
    // After the last page the search wraps around, both ways.
    assert_eq!(find.next(), FindStep::Reveal);
    assert_eq!(find.active, Some(0));
    assert_eq!(find.previous(), FindStep::Reveal);
    assert_eq!(find.active, Some(4));
}

#[test]
fn stale_pages_repeated_cursors_and_empty_queries_change_nothing() {
    let (mut find, cycle) = searching();
    let newer = find.set_query("world".into()).unwrap();
    assert_eq!(
        find.accept_page(
            cycle,
            "t",
            page(vec![occurrence("u1", "hello", 0..5)], None)
        ),
        FindStep::Nothing
    );
    assert!(find.matches.is_empty());
    assert_eq!(
        find.accept_page(
            newer,
            "other",
            page(vec![occurrence("u1", "world", 0..5)], None)
        ),
        FindStep::Nothing
    );
    find.accept_page(
        newer,
        "t",
        page(vec![occurrence("u1", "world", 0..5)], Some("c1")),
    );
    find.next();
    // A cursor seen before ends the walk instead of looping.
    find.accept_page(
        newer,
        "t",
        page(vec![occurrence("u2", "world", 0..5)], Some("c1")),
    );
    assert!(find.next_cursor.is_none() && find.error.is_some());
    // An older generation's late page is ignored.
    let mut late = page(vec![occurrence("u3", "world", 0..5)], None);
    late.generation = 0;
    assert_eq!(find.accept_page(newer, "t", late), FindStep::Nothing);
    assert_eq!(find.set_query("   ".into()), None);
    assert_eq!(find.count_label(), None);
    let zero = find.set_query("zzz".into()).unwrap();
    find.accept_page(zero, "t", page(vec![], None));
    assert_eq!(
        find.count_label().unwrap(),
        crate::i18n::format!("0 个结果" => "0 results")
    );
    // Switching threads starts over.
    find.open(Some("other".into()));
    assert!(find.query.is_empty() && find.matches.is_empty());
}

#[test]
fn local_matches_are_case_insensitive_and_keep_utf8_boundaries() {
    assert_eq!(
        case_insensitive_matches("Hello hELLo", "hello"),
        [0..5, 6..11]
    );
    let text = "你好🙂世界，你好";
    let ranges = case_insensitive_matches(text, "你好");
    assert_eq!(
        ranges
            .iter()
            .map(|range| &text[range.clone()])
            .collect::<Vec<_>>(),
        ["你好", "你好"]
    );
    assert_eq!(&text[case_insensitive_matches(text, "🙂")[0].clone()], "🙂");
    assert!(case_insensitive_matches("abc", "").is_empty());
    let (mut find, cycle) = searching();
    let texts = vec![
        (
            "turn-1".to_owned(),
            "u1".to_owned(),
            "Hello and hello".to_owned(),
        ),
        ("turn-1".to_owned(), "m1".to_owned(), "no match".to_owned()),
    ];
    assert_eq!(find.accept_local(cycle, &texts), FindStep::Reveal);
    assert!(find.local);
    assert_eq!(find.matches.len(), 2);
    assert_eq!(find.matches[1].occurrence.matched_text(), "hello");
}
