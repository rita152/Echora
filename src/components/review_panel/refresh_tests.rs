//! Deterministic refresh regressions: complete requests explicitly so neither
//! timer timing nor a developer's working tree can affect the assertions.

use super::*;
use gpui::TestApp;

fn fixture(cx: &mut Context<ReviewPanel>) -> ReviewPanel {
    let mut panel = ReviewPanel::new(std::env::temp_dir(), ThemeMode::Light, cx);
    // Invalidate the constructor's asynchronous request and stop polling.
    panel.generation += 1;
    panel.loading = false;
    panel.active = false;
    panel
}

fn empty_snapshot() -> Snapshot {
    Snapshot {
        root: std::env::temp_dir(),
        branch: "main".into(),
        ..Default::default()
    }
}

fn changed_snapshot() -> Snapshot {
    Snapshot {
        files: git_review::parse_unified(
            "diff --git a/demo.rs b/demo.rs\n--- a/demo.rs\n+++ b/demo.rs\n@@ -1 +1 @@\n-old\n+new\n",
        ),
        ..empty_snapshot()
    }
}

#[test]
fn empty_snapshot_stays_visible_during_repeated_refreshes() {
    let mut app = TestApp::new();
    let panel = app.new_entity(fixture);
    app.update_entity(&panel, |panel, cx| {
        panel.refresh(cx);
        assert!(panel.show_initial_loading());
        panel.finish_refresh(panel.generation, Ok(empty_snapshot()), cx);
        assert!(panel.snapshot_loaded);
        assert!(!panel.loading);
        assert!(!panel.show_initial_loading());
        assert!(panel.rows.is_empty());
        let snapshot = panel.snapshot.clone();
        let rows = panel.rows.clone();

        for _ in 0..3 {
            panel.refresh(cx);
            let generation = panel.generation;
            assert!(panel.loading);
            assert!(!panel.show_initial_loading());
            // A second poll or a manual refresh must not overlap this request.
            panel.refresh(cx);
            assert_eq!(panel.generation, generation);
            panel.finish_refresh(generation, Ok(empty_snapshot()), cx);
            assert!(!panel.loading);
            assert!(!panel.show_initial_loading());
            assert!(Arc::ptr_eq(&panel.snapshot, &snapshot));
            assert!(Arc::ptr_eq(&panel.rows, &rows));
        }
    });
}

#[test]
fn changed_and_filtered_snapshots_refresh_without_a_loading_placeholder() {
    let mut app = TestApp::new();
    let panel = app.new_entity(fixture);
    app.update_entity(&panel, |panel, cx| {
        panel.refresh(cx);
        panel.finish_refresh(panel.generation, Ok(changed_snapshot()), cx);
        assert!(!panel.rows.is_empty());
        panel.query = "no-matching-file".into();
        panel.rebuild(cx);
        assert!(panel.rows.is_empty());
        let snapshot = panel.snapshot.clone();
        let rows = panel.rows.clone();

        panel.refresh(cx);
        assert!(!panel.show_initial_loading());
        panel.finish_refresh(panel.generation, Ok(changed_snapshot()), cx);
        assert!(Arc::ptr_eq(&panel.snapshot, &snapshot));
        assert!(Arc::ptr_eq(&panel.rows, &rows));

        // The quiet refresh still installs actual additions/removals.
        panel.query.clear();
        panel.rebuild(cx);
        panel.refresh(cx);
        panel.finish_refresh(panel.generation, Ok(empty_snapshot()), cx);
        assert!(panel.snapshot.files.is_empty());
        assert!(panel.rows.is_empty());
        panel.refresh(cx);
        assert!(!panel.show_initial_loading());
        panel.finish_refresh(panel.generation, Ok(changed_snapshot()), cx);
        assert_eq!(panel.snapshot.files.len(), 1);
        assert!(!panel.rows.is_empty());
    });
}

#[test]
fn failed_initial_load_keeps_its_error_visible_during_retry() {
    let mut app = TestApp::new();
    let panel = app.new_entity(fixture);
    app.update_entity(&panel, |panel, cx| {
        panel.refresh(cx);
        panel.finish_refresh(panel.generation, Err("git unavailable".into()), cx);
        assert!(!panel.snapshot_loaded);
        assert!(!panel.loading);
        assert_eq!(panel.error.as_deref(), Some("git unavailable"));

        panel.refresh(cx);
        assert!(panel.loading);
        assert!(!panel.show_initial_loading());
        assert_eq!(panel.error.as_deref(), Some("git unavailable"));
        panel.finish_refresh(panel.generation, Ok(empty_snapshot()), cx);
        assert!(panel.snapshot_loaded);
        assert!(panel.error.is_none());
        assert!(!panel.show_initial_loading());
    });
}

#[test]
fn background_errors_do_not_discard_a_loaded_snapshot() {
    let mut app = TestApp::new();
    let panel = app.new_entity(fixture);
    app.update_entity(&panel, |panel, cx| {
        panel.refresh(cx);
        panel.finish_refresh(panel.generation, Ok(changed_snapshot()), cx);
        let snapshot = panel.snapshot.clone();
        let rows = panel.rows.clone();
        panel.refresh(cx);
        panel.finish_refresh(panel.generation, Err("temporary git error".into()), cx);
        assert!(panel.snapshot_loaded);
        assert!(Arc::ptr_eq(&panel.snapshot, &snapshot));
        assert!(Arc::ptr_eq(&panel.rows, &rows));
        panel.refresh(cx);
        assert!(!panel.show_initial_loading());
        panel.finish_refresh(panel.generation, Ok(changed_snapshot()), cx);
        assert!(panel.error.is_none());
        assert!(Arc::ptr_eq(&panel.snapshot, &snapshot));
    });
}

#[test]
fn scope_changes_reset_loading_and_ignore_stale_completions() {
    let mut app = TestApp::new();
    let panel = app.new_entity(fixture);
    app.update_entity(&panel, |panel, cx| {
        panel.refresh(cx);
        panel.finish_refresh(panel.generation, Ok(empty_snapshot()), cx);
        for scope in [
            Scope::Unstaged,
            Scope::Staged,
            Scope::Branch("origin/main".into()),
            Scope::Commit("12345678".into()),
            Scope::Uncommitted,
        ] {
            panel.refresh(cx);
            let stale_generation = panel.generation;
            panel.error = Some("previous scope error".into());
            panel.change_scope(scope, cx);
            let generation = panel.generation;
            assert!(!panel.snapshot_loaded);
            assert!(panel.error.is_none());
            assert!(panel.show_initial_loading());

            panel.finish_refresh(stale_generation, Ok(changed_snapshot()), cx);
            panel.finish_refresh(stale_generation, Err("stale failure".into()), cx);
            assert_eq!(panel.generation, generation);
            assert!(panel.loading);
            assert!(!panel.snapshot_loaded);
            assert!(panel.snapshot.files.is_empty());
            assert!(panel.error.is_none());

            panel.finish_refresh(generation, Ok(empty_snapshot()), cx);
            assert!(panel.snapshot_loaded);
            assert!(!panel.show_initial_loading());
        }
    });
}

#[test]
fn last_turn_content_is_ready_without_waiting_for_git_metadata() {
    let mut app = TestApp::new();
    let panel = app.new_entity(fixture);
    app.update_entity(&panel, |panel, cx| {
        for files in [Vec::new(), changed_snapshot().files] {
            panel.last_turn = files.clone();
            panel.change_scope(Scope::LastTurn, cx);
            assert!(panel.loading);
            assert!(panel.snapshot_loaded);
            assert!(!panel.show_initial_loading());
            assert!(panel.snapshot.files == files);
            panel.finish_refresh(panel.generation, Ok(empty_snapshot()), cx);
            assert!(panel.snapshot.files == files);
        }
    });
}

#[test]
fn reopening_a_loaded_panel_keeps_its_empty_state() {
    let mut app = TestApp::new();
    let panel = app.new_entity(fixture);
    app.update_entity(&panel, |panel, cx| {
        panel.refresh(cx);
        panel.finish_refresh(panel.generation, Ok(empty_snapshot()), cx);
        panel.deactivate();
        panel.focus(cx);
        assert!(panel.active);
        assert!(panel.loading);
        assert!(!panel.show_initial_loading());
        let generation = panel.generation;
        panel.focus(cx);
        assert_eq!(panel.generation, generation);
        panel.finish_refresh(generation, Ok(empty_snapshot()), cx);
        assert!(!panel.show_initial_loading());
    });
}
