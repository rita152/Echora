use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A throwaway directory tree that cleans itself up.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "gpui-assets-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    /// Creates `assets/icons` below this directory so it counts as usable.
    fn with_assets(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.join("assets").join(ASSETS_PROBE)).unwrap();
        path
    }

    /// Creates `assets` without `icons`, the shape of a half-copied bundle.
    fn with_empty_assets(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.join("assets")).unwrap();
        path
    }

    fn empty(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn bundle_resources_win_over_the_build_worktree() {
    let scratch = Scratch::new();
    let bundle = scratch.with_assets("GPUI Capture.app/Contents/Resources");
    let executable = scratch
        .empty("GPUI Capture.app/Contents/MacOS")
        .join("gpui-chat-clone");
    let worktree = scratch.with_assets("worktree");

    let status = resolve_from(candidates_from(None, Some(&executable), &worktree, None));

    assert_eq!(status.origin, Some(AssetOrigin::BundleResources));
    assert_eq!(
        status.base.as_deref(),
        Some(
            std::fs::canonicalize(bundle.join("assets"))
                .unwrap()
                .as_path()
        )
    );
}

#[test]
fn executable_sibling_assets_are_used_without_a_bundle_layout() {
    let scratch = Scratch::new();
    let executable_directory = scratch.with_assets("dist");
    let executable = executable_directory.join("gpui-chat-clone");
    let worktree = scratch.with_assets("worktree");

    let status = resolve_from(candidates_from(None, Some(&executable), &worktree, None));

    assert_eq!(status.origin, Some(AssetOrigin::ExecutableDirectory));
    assert_eq!(
        status.base.as_deref(),
        Some(
            std::fs::canonicalize(executable_directory.join("assets"))
                .unwrap()
                .as_path()
        )
    );
}

#[test]
fn environment_override_wins_over_every_other_candidate() {
    let scratch = Scratch::new();
    let override_directory = scratch.with_assets("override");
    let bundle = scratch.with_assets("GPUI Capture.app/Contents/Resources");
    let executable = scratch
        .empty("GPUI Capture.app/Contents/MacOS")
        .join("gpui-chat-clone");
    let worktree = scratch.with_assets("worktree");
    let _ = bundle;

    // `GPUI_ASSETS_DIR` points at the assets directory itself, not at a parent.
    let status = resolve_from(candidates_from(
        Some(override_directory.join("assets")),
        Some(&executable),
        &worktree,
        None,
    ));

    assert_eq!(status.origin, Some(AssetOrigin::Environment));
    assert_eq!(
        status.base.as_deref(),
        Some(
            std::fs::canonicalize(override_directory.join("assets"))
                .unwrap()
                .as_path()
        )
    );
}

#[test]
fn cargo_run_falls_back_to_the_build_worktree() {
    let scratch = Scratch::new();
    let executable = scratch.empty("target/debug").join("gpui-chat-clone");
    let worktree = scratch.with_assets("worktree");
    let working_directory = scratch.empty("elsewhere");

    let status = resolve_from(candidates_from(
        None,
        Some(&executable),
        &worktree,
        Some(&working_directory),
    ));

    assert_eq!(status.origin, Some(AssetOrigin::CompiledWorktree));
    assert_eq!(
        status.base.as_deref(),
        Some(
            std::fs::canonicalize(worktree.join("assets"))
                .unwrap()
                .as_path()
        )
    );
}

#[test]
fn working_directory_is_the_last_resort() {
    let scratch = Scratch::new();
    let executable = scratch.empty("target/debug").join("gpui-chat-clone");
    let worktree = scratch.empty("worktree");
    let working_directory = scratch.with_assets("checkout");

    let status = resolve_from(candidates_from(
        None,
        Some(&executable),
        &worktree,
        Some(&working_directory),
    ));

    assert_eq!(status.origin, Some(AssetOrigin::WorkingDirectory));
}

#[test]
fn a_directory_without_icons_is_rejected() {
    let scratch = Scratch::new();
    let executable_directory = scratch.with_empty_assets("dist");
    let executable = executable_directory.join("gpui-chat-clone");
    let worktree = scratch.with_assets("worktree");

    let status = resolve_from(candidates_from(None, Some(&executable), &worktree, None));

    let rejected = status
        .candidates
        .iter()
        .find(|candidate| candidate.origin == AssetOrigin::ExecutableDirectory)
        .expect("the half-copied bundle was considered");
    assert!(
        !rejected.usable,
        "assets without icons must not count as usable"
    );
    assert_eq!(status.origin, Some(AssetOrigin::CompiledWorktree));
}

#[test]
fn a_missing_base_reports_every_candidate_instead_of_failing_silently() {
    let scratch = Scratch::new();
    let executable = scratch.empty("target/debug").join("gpui-chat-clone");
    let worktree = scratch.empty("worktree");
    let working_directory = scratch.empty("checkout");

    let status = resolve_from(candidates_from(
        None,
        Some(&executable),
        &worktree,
        Some(&working_directory),
    ));

    assert!(status.is_missing());
    assert_eq!(status.summary(), "assets directory not found");
    assert_eq!(status.candidates.len(), 4);
    assert!(status.candidates.iter().all(|candidate| !candidate.usable));
    let tried = status.tried_summary();
    for expected in [
        "Resources/assets",
        "target/debug/assets",
        "worktree/assets",
        "checkout/assets",
    ] {
        assert!(tried.contains(expected), "{tried} is missing {expected}");
    }
    let warning = missing_warning(&status);
    assert!(warning.contains("will not render"), "{warning}");
    assert!(warning.contains("worktree/assets"), "{warning}");
}

#[test]
fn a_missing_base_makes_the_loader_fail() {
    let scratch = Scratch::new();
    let executable = scratch.empty("target/debug").join("gpui-chat-clone");
    let worktree = scratch.empty("worktree");
    let status = resolve_from(candidates_from(None, Some(&executable), &worktree, None));

    let assets = Assets::load_from(&status);
    assert!(
        assets.load("icons/folder.svg").is_err(),
        "a missing base has no fallback: blank icons and the banner are one condition"
    );

    let worktree = scratch.with_assets("worktree");
    let status = resolve_from(candidates_from(None, Some(&executable), &worktree, None));
    let assets = Assets::load_from(&status);
    assert!(
        assets.load("icons/missing-icon.svg").is_err(),
        "a real base still fails for files that do not exist"
    );
    assert!(status.base.is_some());
}
