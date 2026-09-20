//! Layout and interaction regressions for the rendered picker, not estimates
//! based on branch-name length. These fixtures never mutate a Git repository.

use super::*;
use gpui::{Bounds, TestApp, WindowBounds, WindowOptions, size};

struct Harness {
    picker: Entity<BranchPicker>,
    events: Vec<BranchPickerEvent>,
    width: f32,
}

impl Harness {
    fn new(mode: ThemeMode, width: f32, names: &[String], cx: &mut Context<Self>) -> Self {
        let picker = cx.new(|cx| {
            let mut picker = BranchPicker::new(mode, cx);
            picker.prepare(names, "origin/main", cx);
            picker
        });
        cx.subscribe(&picker, |h, _, event: &BranchPickerEvent, cx| {
            h.events.push(event.clone());
            cx.notify();
        })
        .detach();
        Self {
            picker,
            events: Vec::new(),
            width,
        }
    }
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(self.width)).child(self.picker.clone())
    }
}

fn options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(0.), px(0.)),
            size(px(800.), px(650.)),
        ))),
        ..Default::default()
    }
}

fn names() -> Vec<String> {
    [
        "codex/command-approval-variants".into(),
        "codex/english-language-support".into(),
        "codex/integrate-approvals-20260909".into(),
        "main".into(),
        "origin/main".into(),
        "codex/修复-很长的分支名称-支持中文和省略号".into(),
        format!("codex/{}", "x".repeat(200)),
        "fix/review-panel-interactions-20260920".into(),
    ]
    .into()
}

#[test]
fn long_branch_rows_have_uniform_bounds_in_both_themes_and_widths() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for width in [224., WIDTH] {
            let mut app = TestApp::new();
            let mut window = app.open_window_with_options(options(), |_, cx| {
                Harness::new(mode, width, &names(), cx)
            });
            window.draw();
            window.read(|h, cx| {
                let picker = h.picker.read(cx);
                assert_eq!(picker.scroll.children_count(), 8);
                assert_eq!(picker.scroll.bounds().size.height, px(8. * ROW_HEIGHT));
                assert_eq!(picker.scroll.max_offset().height, px(0.));
                let first = picker.scroll.bounds_for_item(0).unwrap();
                for index in 0..8 {
                    let row = picker.scroll.bounds_for_item(index).unwrap();
                    assert_eq!(row.size.height, px(ROW_HEIGHT));
                    assert_eq!(row.top() - first.top(), px(index as f32 * ROW_HEIGHT));
                    assert!(row.size.width <= px(width - 12.));
                }
                assert_eq!(picker.current, "origin/main");
                assert_eq!(picker.branches[0], "origin/main");
                assert!(picker.branches.iter().any(|name| name == "main"));
            });
        }
    }
}

#[test]
fn the_entire_row_selects_the_complete_untruncated_ref() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(options(), |_, cx| {
        Harness::new(ThemeMode::Light, 224., &names(), cx)
    });
    window.draw();
    // Row 0 is the current base; row 1 is the long command-approval ref.
    for fraction in [0.03, 0.5, 0.97] {
        let position = window.read(|h, cx| {
            let viewport = h.picker.read(cx).scroll.bounds();
            point(
                viewport.left() + viewport.size.width * fraction,
                viewport.top() + px(ROW_HEIGHT * 1.5),
            )
        });
        window.simulate_click(position, MouseButton::Left);
        window.draw();
    }
    window.read(|h, _| {
        assert_eq!(
            h.events,
            vec![BranchPickerEvent::Selected("codex/command-approval-variants".into()); 3]
        );
    });
}

#[test]
fn searching_is_case_insensitive_and_keeps_unicode_and_ref_identity() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(options(), |_, cx| {
        Harness::new(ThemeMode::Light, WIDTH, &names(), cx)
    });
    window.draw();
    window.simulate_input("COMMAND-APPROVAL-VARIANTS");
    window.draw();
    window.read(|h, cx| assert_eq!(h.picker.read(cx).matches.len(), 1));
    window.simulate_keystrokes("enter");
    window.draw();
    window.read(|h, _| {
        assert_eq!(
            h.events,
            vec![BranchPickerEvent::Selected("codex/command-approval-variants".into())]
        );
    });
    window.update(|h, _, cx| {
        h.picker.update(cx, |p, cx| {
            p.input.update(cx, |input, cx| input.set_text("支持中文", cx));
        });
    });
    window.draw();
    window.read(|h, cx| {
        let picker = h.picker.read(cx);
        assert_eq!(picker.matches.len(), 1);
        assert_eq!(
            picker.branches[picker.matches[0]],
            "codex/修复-很长的分支名称-支持中文和省略号"
        );
    });
}

#[test]
fn keyboard_navigation_scrolls_without_changing_row_heights() {
    let branches = (0..40)
        .map(|index| format!("codex/integrate-approvals-{index:03}"))
        .collect::<Vec<_>>();
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(options(), |_, cx| {
        Harness::new(ThemeMode::Dark, WIDTH, &branches, cx)
    });
    window.draw();
    window.simulate_keystrokes("up");
    window.draw();
    window.read(|h, cx| {
        let picker = h.picker.read(cx);
        assert_eq!(picker.selected, 39);
        assert_eq!(picker.scroll.bounds().size.height, px(MAX_LIST_HEIGHT));
        assert!(picker.scroll.offset().y < px(0.));
        for index in 0..40 {
            assert_eq!(
                picker.scroll.bounds_for_item(index).unwrap().size.height,
                px(ROW_HEIGHT)
            );
        }
    });
    window.simulate_keystrokes("enter");
    window.draw();
    window.read(|h, _| {
        assert_eq!(
            h.events,
            vec![BranchPickerEvent::Selected(branches[39].clone())]
        );
    });
}

#[test]
fn empty_results_do_not_activate_and_reopening_resets_search_and_scroll() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(options(), |_, cx| {
        Harness::new(ThemeMode::Light, WIDTH, &names(), cx)
    });
    window.draw();
    window.simulate_input("no-such-branch");
    window.draw();
    for key in ["up", "down", "enter"] {
        window.simulate_keystrokes(key);
        window.draw();
    }
    window.read(|h, cx| {
        assert!(h.picker.read(cx).matches.is_empty());
        assert!(h.events.is_empty());
    });
    window.simulate_keystrokes("escape");
    window.draw();
    window.read(|h, _| assert_eq!(h.events, vec![BranchPickerEvent::Dismissed]));
    window.update(|h, _, cx| {
        h.picker.update(cx, |picker, cx| picker.prepare(&names(), "main", cx));
    });
    window.draw();
    window.read(|h, cx| {
        let picker = h.picker.read(cx);
        assert!(picker.input.read(cx).text().is_empty());
        assert_eq!(picker.matches.len(), 8);
        assert_eq!(picker.branches[0], "main");
        assert_eq!(picker.current, "main");
        assert_eq!(picker.scroll.offset(), point(px(0.), px(0.)));
    });
}

#[test]
fn review_branch_search_does_not_use_unfiltered_menu_indices_or_checkout_head() {
    use super::super::{Menu, ReviewPanel, Scope, Snapshot};
    use std::sync::Arc;

    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(options(), |_, cx| {
        let mut panel = ReviewPanel::new(std::env::temp_dir(), ThemeMode::Light, cx);
        // Ignore the constructor's asynchronous read and prevent further reads.
        panel.active = false;
        panel.generation += 1;
        panel.busy = true;
        panel.snapshot = Arc::new(Snapshot {
            branch: "main".into(),
            branches: names(),
            ..Default::default()
        });
        panel.scope = Scope::Branch("origin/main".into());
        panel.toggle_menu(Menu::Branch, cx);
        panel
    });
    window.draw();
    window.simulate_input("english-language-support");
    window.draw();
    window.simulate_keystrokes("space");
    window.draw();
    window.read(|panel, _| {
        assert_eq!(panel.menu, Some(Menu::Branch));
        assert_eq!(panel.scope, Scope::Branch("origin/main".into()));
    });
    window.simulate_keystrokes("enter");
    window.draw();
    window.read(|panel, _| {
        assert!(panel.menu.is_none());
        assert_eq!(
            panel.scope,
            Scope::Branch("codex/english-language-support".into())
        );
        assert_eq!(panel.snapshot.branch, "main");
    });
}
