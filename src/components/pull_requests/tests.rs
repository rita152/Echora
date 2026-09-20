use super::*;
use crate::git_review::{Line, LineKind};

#[test]
fn split_diff_pairs_replacements_without_crossing_context() {
    let lines: Vec<_> = [
        LineKind::Context,
        LineKind::Deleted,
        LineKind::Deleted,
        LineKind::Added,
        LineKind::Context,
        LineKind::Added,
    ]
    .into_iter()
    .map(|kind| Line {
        old: None,
        new: None,
        text: String::new(),
        kind,
    })
    .collect();
    assert_eq!(
        diff::split_pairs(&lines),
        vec![
            (Some(0), Some(0)),
            (Some(1), Some(3)),
            (Some(2), None),
            (Some(4), Some(4)),
            (None, Some(5))
        ]
    );
}

#[test]
fn word_highlights_keep_unicode_boundaries() {
    for (before, after, changed) in [
        ("let 名 = 1;", "let 名 = 2;", "1"),
        ("a😀c", "a🦀c", "😀"),
        ("same", "same", ""),
        ("abc", "ab", "c"),
    ] {
        assert_eq!(&before[diff::changed_span(before, after)], changed);
    }
}

fn summary(title: &str) -> crate::pull_requests::PullRequestSummary {
    crate::pull_requests::PullRequestSummary {
        number: 1,
        title: title.into(),
        repository: "test/repository".into(),
        head_branch: "topic".into(),
        base_branch: "main".into(),
        additions: 1,
        deletions: 1,
        status: crate::pull_requests::PullRequestStatus::Open,
        age: "now".into(),
        author: "test".into(),
        url: "https://github.com/test/repository/pull/1".into(),
    }
}
fn detail(title: &str) -> PullRequestDetail {
    PullRequestDetail {
        summary: summary(title),
        body: String::new(),
        requested_reviewers: vec![],
        reviewers: vec![],
        comments: vec![],
        review_threads: vec![],
        timeline: vec![],
        checks: vec![],
        commits: vec![],
        author: User {
            login: "test".into(),
            name: None,
            avatar_url: None,
            is_self: true,
        },
        mergeable: true,
        age: String::new(),
        created_age: String::new(),
        additions: 1,
        deletions: 1,
        head_sha: "a".repeat(40),
    }
}
fn setup() -> (gpui::TestApp, Entity<PullRequestsView>) {
    let mut app = gpui::TestApp::new();
    let view = app.new_entity(|cx| PullRequestsView::build(ThemeMode::Light, None, false, cx));
    app.update_entity(&view, |view, _| {
        view.list_loading = false;
        view.selected = Some(summary("original"));
        view.detail = Some(detail("original"));
    });
    (app, view)
}

#[test]
fn ordinary_quote_reply_prefills_the_visible_comment_composer() {
    let (mut app, view) = setup();
    app.update_entity(&view, |view, cx| {
        view.begin_reply(None, Some("> quoted\n\n".into()), cx);
        assert_eq!(view.comment_box.read(cx).text(), "> quoted\n\n");
        assert!(view.reply.is_none());
        assert!(f32::from(view.detail_scroll.offset().y) < 0.0);
    });
}

#[test]
fn outside_click_keeps_an_inline_draft_and_its_deletion_side() {
    let (mut app, view) = setup();
    app.update_entity(&view, |view, cx| {
        view.begin_inline_comment(0, "removed.rs".into(), 7, true, cx);
        view.inline_editor
            .as_ref()
            .unwrap()
            .update(cx, |editor, cx| {
                editor.set_text_silently("Please retain this", cx)
            });
        view.dismiss_menus(cx);
        assert!(view.inline_comment.as_ref().unwrap().old);
        assert_eq!(
            view.inline_editor.as_ref().unwrap().read(cx).text(),
            "Please retain this"
        );
    });
}

#[test]
fn failed_write_keeps_draft_and_displays_the_real_error() {
    let (mut app, view) = setup();
    app.update_entity(&view, |view, cx| {
        view.begin_title_edit(cx);
        view.mutation_pending = true;
        view.finish_mutation(
            view.detail_generation,
            "saved",
            Err("permission denied".into()),
            |_, _| panic!("failed write must not clear draft"),
            cx,
        );
        assert!(view.title_edit.is_some());
        assert_eq!(view.detail.as_ref().unwrap().summary.title, "original");
        assert_eq!(view.notice.as_deref(), Some("GitHub: permission denied"));
        assert!(!view.mutation_pending);
    });
}

#[test]
fn late_success_cannot_update_another_selection_or_clear_its_draft() {
    let (mut app, view) = setup();
    app.update_entity(&view, |view, cx| {
        view.detail_generation = 2;
        view.finish_mutation(
            1,
            "saved",
            Ok(Ok(detail("other PR"))),
            |_, _| panic!("stale completion must not touch the current editor"),
            cx,
        );
        assert_eq!(view.selected.as_ref().unwrap().title, "original");
        assert_eq!(view.detail.as_ref().unwrap().summary.title, "original");
    });
}

#[test]
fn successful_write_uses_readback_and_distinguishes_refresh_failure() {
    let (mut app, view) = setup();
    app.update_entity(&view, |view, cx| {
        view.finish_mutation(
            view.detail_generation,
            "saved",
            Ok(Ok(detail("server title"))),
            |_, _| {},
            cx,
        );
        assert_eq!(view.selected.as_ref().unwrap().title, "server title");
        view.finish_mutation(
            view.detail_generation,
            "saved",
            Ok(Err("offline".into())),
            |_, _| {},
            cx,
        );
        assert_eq!(
            view.notice.as_deref(),
            Some("Saved to GitHub; refresh failed: offline")
        );
    });
}

#[test]
fn changing_diff_invalidates_inflight_requests_and_cached_file_contents() {
    let (mut app, view) = setup();
    app.update_entity(&view, |view, cx| {
        let (diff_generation, file_generation) = (view.diff_generation, view.file_generation);
        view.diff_loading = true;
        view.file_lines
            .insert("same.rs".into(), vec!["old content".into()]);
        view.file_lines_loading.insert("same.rs".into());
        view.reset_diff(cx);
        assert!(view.diff_generation > diff_generation && view.file_generation > file_generation);
        assert!(!view.diff_loading);
        assert!(view.file_lines.is_empty() && view.file_lines_loading.is_empty());
    });
}

#[test]
fn duplicate_submissions_are_not_dispatched() {
    let (mut app, view) = setup();
    app.update_entity(&view, |view, cx| {
        view.mutation_pending = true;
        view.mutate(
            summary("test"),
            "saved",
            |_, _| panic!("must not send a duplicate write"),
            |_, _| {},
            cx,
        );
        assert!(view.mutation_pending);
    });
    app.run_until_parked();
}

#[gpui::test]
fn description_and_comment_editors_have_visible_layout_height(cx: &mut gpui::TestAppContext) {
    use gpui::{VisualTestContext, px};
    let handle = cx.add_window(|_, cx| {
        let mut view = PullRequestsView::build(ThemeMode::Light, None, false, cx);
        view.list_loading = false;
        view.selected = Some(summary("Editor layout"));
        view.detail = Some(detail("Editor layout"));
        view.begin_description_edit(cx);
        view
    });
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        visual
            .debug_bounds("pr-description-editor-frame")
            .unwrap()
            .size
            .height,
        px(280.0)
    );
    assert_eq!(
        visual
            .debug_bounds("pr-comment-composer-frame")
            .unwrap()
            .size
            .height,
        px(96.0)
    );
}
