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
        author_avatar_url: None,
        url: "https://github.com/test/repository/pull/1".into(),
        can_merge: false,
        has_conflicts: false,
        ci_status: crate::pull_requests::CiStatus::None,
    }
}
fn detail(title: &str) -> PullRequestDetail {
    PullRequestDetail {
        viewer: None,
        summary: summary(title),
        body: String::new(),
        requested_reviewers: vec![],
        reviewers: vec![],
        comments: vec![],
        review_threads: vec![],
        activity: vec![],
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
    // The reference composer grows 28px per line from a single empty line.
    assert_eq!(
        visual
            .debug_bounds("pr-comment-composer-frame")
            .unwrap()
            .size
            .height,
        px(28.0)
    );
}

/// The review scope menu, with the detail pane taking `ratio` of a 1440px
/// window and one commit titled `subject`, its `Commits` flyout open:
/// (row, flyout) bounds.
fn scope_flyout(
    cx: &mut gpui::TestAppContext,
    ratio: f32,
    subject: &str,
) -> (gpui::Bounds<gpui::Pixels>, gpui::Bounds<gpui::Pixels>) {
    use gpui::{VisualTestContext, px, size};
    let subject = subject.to_string();
    let handle = cx.open_window(size(px(1440.0), px(900.0)), move |_, cx| {
        let mut view = PullRequestsView::build(ThemeMode::Light, None, false, cx);
        view.list_loading = false;
        view.selected = Some(summary("Scope"));
        view.detail = Some(detail("Scope"));
        view.detail_ratio = Some(ratio);
        // No diff request: the header and its menu are what is measured.
        view.diff_loading = true;
        view.review_tab = Some(ReviewTab {
            scope: ReviewScope::AllChanges,
            commits: vec![("b".repeat(40), subject)],
        });
        view.scope_menu_open = true;
        view.scope_commits_open = true;
        view
    });
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    // The flyout places itself from the row bounds of the previous frame.
    for _ in 0..2 {
        visual.update(|window, cx| window.draw(cx).clear(cx));
    }
    (
        visual.debug_bounds("pr-scope-commits").unwrap(),
        visual.debug_bounds("pr-scope-commits-menu").unwrap(),
    )
}

/// `anchored` places popups on whole pixels.
fn near(a: gpui::Pixels, b: gpui::Pixels) -> bool {
    (f32::from(a) - f32::from(b)).abs() <= 0.5
}

#[gpui::test]
fn scope_commits_flyout_opens_beside_its_row(cx: &mut gpui::TestAppContext) {
    use gpui::px;
    // Room on the right: 5px past the menu edge, level with its padding.
    let (row, flyout) = scope_flyout(cx, 0.6, "Short");
    assert!(
        near(flyout.left(), row.right() + px(9.0)),
        "{row:?} {flyout:?}"
    );
    assert!(
        near(flyout.top(), row.top() - px(4.0)),
        "{row:?} {flyout:?}"
    );
}

#[gpui::test]
fn scope_commits_flyout_flips_left_without_room(cx: &mut gpui::TestAppContext) {
    use gpui::px;
    let (row, flyout) = scope_flyout(
        cx,
        0.3,
        "Reconcile the app-server integration table with the implemented protocol",
    );
    assert!(
        near(flyout.right(), row.left() - px(9.0)),
        "{row:?} {flyout:?}"
    );
    assert!(
        near(flyout.top(), row.top() - px(4.0)),
        "{row:?} {flyout:?}"
    );
}
