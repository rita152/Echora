//! Work-budget regressions: the file tree shares ReviewPanel's scroll redraws.
//! Wall-clock timings are deliberately kept in the ignored benchmark below.

use super::*;
use gpui::{Bounds, TestApp, WindowBounds, WindowOptions, point, size};
use std::cell::Cell;

thread_local! {
    static TREE_ROWS_RENDERED: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn record_row() {
    TREE_ROWS_RENDERED.with(|count| count.set(count.get() + 1));
}

fn take_row_count() -> usize {
    TREE_ROWS_RENDERED.with(|count| count.replace(0))
}

fn options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(0.), px(0.)),
            size(px(800.), px(500.)),
        ))),
        ..Default::default()
    }
}

fn fixture(file_count: usize, cx: &mut Context<ReviewPanel>) -> ReviewPanel {
    let mut panel = ReviewPanel::new(std::env::temp_dir(), ThemeMode::Dark, cx);
    panel.active = false;
    panel.generation += 1;
    panel.loading = true;
    panel.tree_open = true;
    let file = git_review::parse_unified(
        "diff --git a/demo.rs b/demo.rs\n--- a/demo.rs\n+++ b/demo.rs\n@@ -1,2 +1,2 @@\n context\n-let old = 1;\n+let new = 2;\n",
    )
    .remove(0);
    panel.snapshot = Arc::new(Snapshot {
        root: std::env::temp_dir(),
        files: (0..file_count)
            .map(|i| FileDiff {
                path: format!("src/group-{}/file-{i:05}.rs", i / 100),
                ..file.clone()
            })
            .collect(),
        ..Default::default()
    });
    panel.rebuild(cx);
    panel
}

#[test]
fn diff_scroll_builds_only_viewport_tree_rows_in_large_reviews() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(options(), |_, cx| fixture(2048, cx));
    window.draw();
    window.draw();
    for split in [false, true] {
        window.update(|panel, _, cx| {
            panel.split = split;
            panel.words = true;
            panel.rebuild(cx);
            panel.scroll.scroll_to(ListOffset {
                item_ix: panel.rows.len() / 2,
                offset_in_item: px(0.),
            });
        });
        window.draw();
        window.draw();
        for delta in [-60., -60., 60., 60.] {
            take_row_count();
            let before = window.read(|panel, _| panel.scroll.logical_scroll_top());
            window.simulate_scroll(point(px(180.), px(250.)), point(px(0.), px(delta)));
            window.draw();
            let rendered = take_row_count();
            let after = window.read(|panel, _| panel.scroll.logical_scroll_top());
            assert!(
                before.item_ix != after.item_ix || before.offset_in_item != after.offset_in_item,
                "the test must actually scroll the diff"
            );
            // A 500px window fits fewer than 18 tree rows. Allow measurement,
            // overscan and extra draws, but never construction of the whole tree.
            assert!(rendered > 0, "the tree must be visible in this regression");
            assert!(
                rendered < 96,
                "scroll built {rendered} tree rows (split={split}); off-screen rows must stay virtual"
            );
        }
    }
}

#[test]
fn file_tree_wheel_does_not_scroll_the_diff() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(options(), |_, cx| fixture(2048, cx));
    window.draw();
    window.draw();
    window.update(|panel, _, _| {
        panel.scroll.scroll_to(ListOffset {
            item_ix: 100,
            offset_in_item: px(7.),
        });
    });
    window.draw();
    let before = window.read(|panel, _| panel.scroll.logical_scroll_top());
    take_row_count();
    window.simulate_scroll(point(px(700.), px(250.)), point(px(0.), px(-900.)));
    window.draw();
    let after = window.read(|panel, _| panel.scroll.logical_scroll_top());
    assert_eq!(before.item_ix, after.item_ix);
    assert_eq!(before.offset_in_item, after.offset_in_item);
    let rendered = take_row_count();
    assert!(
        rendered > 0 && rendered < 96,
        "tree scroll built {rendered} rows"
    );
}

#[test]
fn long_single_file_scroll_preserves_rows_and_reverses_direction() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(options(), |_, cx| {
        let mut panel = fixture(1, cx);
        let mut patch =
            "diff --git a/long.rs b/long.rs\n--- a/long.rs\n+++ b/long.rs\n@@ -1,20000 +1,20000 @@\n".to_owned();
        for i in 0..20_000 {
            patch.push_str(&format!(" context line {i}\n"));
        }
        Arc::make_mut(&mut panel.snapshot).files = git_review::parse_unified(&patch);
        panel.rebuild(cx);
        panel.scroll.scroll_to(ListOffset {
            item_ix: 10_000,
            offset_in_item: px(0.),
        });
        panel
    });
    window.draw();
    window.draw();
    let rows = window.read(|panel, _| panel.rows.clone());
    let start = window.read(|panel, _| panel.scroll.logical_scroll_top().item_ix);
    window.simulate_scroll(point(px(180.), px(250.)), point(px(0.), px(-120.)));
    window.draw();
    let down = window.read(|panel, _| panel.scroll.logical_scroll_top().item_ix);
    assert!(down > start);
    window.simulate_scroll(point(px(180.), px(250.)), point(px(0.), px(120.)));
    window.draw();
    window.read(|panel, _| {
        assert!(panel.scroll.logical_scroll_top().item_ix < down);
        assert!(
            Arc::ptr_eq(&rows, &panel.rows),
            "wheel events must not rebuild diff rows"
        );
    });
}

#[test]
#[ignore = "manual file-tree-enabled scroll/layout benchmark, not display FPS"]
fn review_file_tree_scroll_timings() {
    for file_count in [32, 512, 4096] {
        for tree_open in [false, true] {
            let mut app = TestApp::new();
            let mut window = app.open_window_with_options(options(), |_, cx| {
                let mut panel = fixture(file_count, cx);
                panel.tree_open = tree_open;
                panel.scroll.scroll_to(ListOffset {
                    item_ix: panel.rows.len() / 2,
                    offset_in_item: px(0.),
                });
                panel
            });
            window.draw();
            window.draw();
            let mut samples = Vec::new();
            let mut max_tree_rows = 0;
            for frame in 0..100 {
                take_row_count();
                let start = std::time::Instant::now();
                window.simulate_scroll(
                    point(px(180.), px(250.)),
                    point(px(0.), px(if frame % 20 < 10 { -60. } else { 60. })),
                );
                window.draw();
                samples.push(start.elapsed().as_secs_f64() * 1000.);
                max_tree_rows = max_tree_rows.max(take_row_count());
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "{}",
                serde_json::json!({
                "files": file_count, "tree_open": tree_open, "samples": samples.len(),
                "median_ms": samples[50], "p95_ms": samples[94], "max_ms": samples[99],
                "max_tree_rows_per_scroll": max_tree_rows,
                })
            );
        }
    }
}
