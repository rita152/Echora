use super::*;

/// The worked examples come from running the reference's own parser
/// (`bt`/`Fte`/`Hee`/`mRe` in ChatGPT 26.924) in node.
#[test]
fn pull_request_urls_normalize_and_key_like_the_reference() {
    let cases = [
        (
            "https://github.com/openai/codex/pull/1234",
            "https://github.com/openai/codex/pull/1234",
            r#"["github.com","openai","codex",1234]"#,
        ),
        (
            "https://GitHub.com/OpenAI/Codex/pull/1234/files?x=1#y",
            "https://github.com/OpenAI/Codex/pull/1234",
            r#"["github.com","openai","codex",1234]"#,
        ),
        (
            "https://gitlab.com/Group/Sub/Proj/-/merge_requests/7/diffs",
            "https://gitlab.com/Group/Sub/Proj/-/merge_requests/7",
            r#"["gitlab.com","group/sub","proj",7]"#,
        ),
        (
            "https://ghe.corp.example/o/r/pull/5",
            "https://ghe.corp.example/o/r/pull/5",
            r#"["ghe.corp.example","o","r",5]"#,
        ),
    ];
    for (input, canonical, key) in cases {
        let parsed = AgentPullRequestRef::parse(input).unwrap_or_else(|| panic!("{input}"));
        assert_eq!(parsed.canonical_url(), canonical, "{input}");
        assert_eq!(parsed.identity_key(), key, "{input}");
    }
    // The key the reference sent on the wire for the fixture PR.
    assert_eq!(
        AgentPullRequestRef::parse("https://github.com/openai/codex/pull/35882")
            .unwrap()
            .identity_key(),
        "[\"github.com\",\"openai\",\"codex\",35882]"
    );
}

#[test]
fn non_pull_request_urls_are_rejected() {
    for input in [
        "https://github.com/openai/codex/issues/1",
        "https://github.com/openai/codex/pull/abc",
        "https://github.com/openai/codex/pull/0",
        "ftp://github.com/openai/codex/pull/1",
        "https://github.com/openai/pull/1",
        "not a url",
        "https://gitlab.com/group/-/merge_requests/1",
        "http://gitlab.example/group/proj/-/merge_requests/1",
    ] {
        assert_eq!(AgentPullRequestRef::parse(input), None, "{input}");
    }
    // Self-hosted GitLab over https is recognised by its path.
    let gitlab =
        AgentPullRequestRef::parse("https://gitlab.example/a/b/-/merge_requests/3").unwrap();
    assert_eq!(gitlab.provider, AgentPullRequestProvider::GitLab);
    assert_eq!(gitlab.identity_key(), r#"["gitlab.example","a","b",3]"#);
}

#[test]
fn identity_comparison_ignores_case() {
    let a = AgentPullRequestRef::parse("https://github.com/OpenAI/Codex/pull/9").unwrap();
    let b = AgentPullRequestRef::parse("https://github.com/openai/codex/pull/9").unwrap();
    assert!(a.same_as(&b));
    assert_eq!(a.canonical_url(), "https://github.com/OpenAI/Codex/pull/9");
}
