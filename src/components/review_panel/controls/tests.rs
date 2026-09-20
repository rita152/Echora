//! Interaction regressions. Git data comes from disposable local repositories;
//! the existing-PR response is seeded explicitly, without contacting GitHub.

use super::*;
use gpui::{Bounds, TestApp, WindowBounds, WindowOptions, point, size};
use std::{fs, path::Path, process::Command, time::SystemTime};

struct TestRepo(PathBuf);

impl TestRepo {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "echora-review-interactions-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let repo = Self(root);
        repo.git(&["init", "-b", "main"]);
        repo.git(&["config", "user.name", "Review Test"]);
        repo.git(&["config", "user.email", "review-test@example.invalid"]);
        repo.git(&["config", "core.hooksPath", "/dev/null"]);
        for path in ["alpha.rs", "beta.rs"] {
            fs::write(repo.0.join(path), "fn before() {}\n").unwrap();
        }
        repo.git(&["add", "--", "alpha.rs", "beta.rs"]);
        repo.git(&["commit", "--no-gpg-sign", "-m", "Initial test files"]);
        for path in ["alpha.rs", "beta.rs"] {
            fs::write(repo.0.join(path), "fn after() {}\n").unwrap();
        }
        repo
    }

    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(&self.0)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(root: &Path, cx: &mut Context<ReviewPanel>) -> ReviewPanel {
    let mut panel = ReviewPanel::new(root.to_owned(), ThemeMode::Light, cx);
    // Freeze auto-refresh, not the data source: load the actual on-disk diff.
    panel.active = false;
    panel.generation += 1;
    panel.loading = true;
    panel.tree_open = false;
    panel.snapshot = Arc::new(git_review::load(root, &Scope::Uncommitted, false, false).unwrap());
    assert_eq!(panel.snapshot.files.len(), 2);
    panel.query = "alpha".into();
    panel
        .filter
        .update(cx, |input, cx| input.set_text_silently("alpha", cx));
    panel.rebuild(cx);
    panel
}

fn window_options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(0.), px(0.)),
            size(px(800.), px(500.)),
        ))),
        ..Default::default()
    }
}

#[test]
fn clicking_jump_result_reveals_a_filtered_out_file() {
    let repo = TestRepo::new();
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(window_options(), |_, cx| fixture(&repo.0, cx));
    window.draw();
    window.update(|panel, w, cx| {
        panel.jump_query = "beta".into();
        panel
            .jump
            .update(cx, |input, cx| input.set_text_silently("beta", cx));
        panel.action(Action::Menu(Menu::Jump), w, cx);
    });
    window.draw();
    // The first result is below the input in the right-anchored Jump popup.
    window.simulate_click(point(px(450.), px(135.)), MouseButton::Left);
    window.draw();
    window.read(|panel, cx| {
        let selected = panel.selected_file;
        assert_eq!(panel.snapshot.files[selected].path, "beta.rs");
        assert!(panel.query.is_empty());
        assert!(panel.filter.read(cx).text().is_empty());
        assert!(panel.menu.is_none());
        assert!(
            panel
                .rows
                .iter()
                .any(|row| matches!(row, Row::Header(i) if *i == selected))
        );
    });
}

#[test]
fn jump_input_submission_also_reveals_a_filtered_out_file() {
    let repo = TestRepo::new();
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(window_options(), |_, cx| fixture(&repo.0, cx));
    window.draw();
    window.update(|panel, w, cx| panel.action(Action::Menu(Menu::Jump), w, cx));
    window.draw();
    window.simulate_input("beta");
    window.simulate_keystrokes("enter");
    window.draw();
    window.read(|panel, cx| {
        assert_eq!(panel.snapshot.files[panel.selected_file].path, "beta.rs");
        assert!(panel.query.is_empty());
        assert!(panel.filter.read(cx).text().is_empty());
        assert!(panel.menu.is_none());
    });
}

#[test]
fn jump_preserves_a_matching_filter_and_ignores_an_invalid_target() {
    let repo = TestRepo::new();
    let mut app = TestApp::new();
    let panel = app.new_entity(|cx| fixture(&repo.0, cx));
    app.update_entity(&panel, |panel, cx| {
        let alpha = panel
            .snapshot
            .files
            .iter()
            .position(|file| file.path == "alpha.rs")
            .unwrap();
        panel.jump_to(alpha, cx);
        assert_eq!(panel.query, "alpha");
        assert_eq!(panel.filter.read(cx).text(), "alpha");
        panel.jump_to(usize::MAX, cx);
        assert_eq!(panel.selected_file, alpha);
        assert_eq!(panel.query, "alpha");
    });
}

#[test]
fn a_new_branch_never_reuses_the_checked_out_branch_pr() {
    let repo = TestRepo::new();
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(window_options(), |_, cx| fixture(&repo.0, cx));
    window.update(|panel, w, cx| {
        let url = "https://example.invalid/existing-pr";
        panel.pr_existing = Some(url.into());
        assert_eq!(panel.existing_pr_for_head(), Some(url));
        panel.action(Action::NewBranch(true), w, cx);
        assert_eq!(panel.existing_pr_for_head(), None);
        assert_eq!(panel.pr_existing.as_deref(), Some(url));
        panel.action(Action::NewBranch(false), w, cx);
        assert_eq!(panel.existing_pr_for_head(), Some(url));
    });
}

#[test]
fn a_lookup_finishing_after_new_branch_selection_stays_hidden() {
    let repo = TestRepo::new();
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(window_options(), |_, cx| fixture(&repo.0, cx));
    window.update(|panel, w, cx| {
        panel.action(Action::NewBranch(true), w, cx);
        // Model the asynchronous lookup result, not a new-branch PR.
        let url = "https://example.invalid/late-existing-pr";
        panel.pr_existing = Some(url.into());
        assert_eq!(panel.existing_pr_for_head(), None);
        panel.action(Action::NewBranch(false), w, cx);
        assert_eq!(panel.existing_pr_for_head(), Some(url));
    });
}
