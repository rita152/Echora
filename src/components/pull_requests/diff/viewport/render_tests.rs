use super::*;
use gpui::{Context, Entity, Render, TestAppContext, VisualTestContext, Window};

struct DiffHarness(Entity<PullRequestsView>);
impl Render for DiffHarness {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |view, cx| {
            view.measure_code_width(window);
            view.prepare_diff_viewport(window, cx);
            view.apply_pending_file_scroll(cx);
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(view.diff_surface(cx))
        })
    }
}

#[gpui::test]
fn a_large_hunk_only_highlights_the_viewport_and_reuses_warm_runs(cx: &mut TestAppContext) {
    let mut patch = String::from(
        "diff --git a/large.rs b/large.rs\n--- a/large.rs\n+++ b/large.rs\n@@ -1,5000 +1,5000 @@\n",
    );
    for i in 0..5000 {
        patch.push_str(&format!("-let old_{i} = {i};\n"));
    }
    for i in 0..5000 {
        patch.push_str(&format!("+let new_{i} = {i};\n"));
    }
    patch.push_str(
        "diff --git a/last.rs b/last.rs\n--- a/last.rs\n+++ b/last.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let inner = cx.new(|cx| {
        let mut view = PullRequestsView::build(ThemeMode::Dark, None, false, cx);
        view.detail_tab = DetailTab::Code;
        view.fullscreen = true;
        view.words = true;
        view.diff = crate::git_review::parse_unified(&patch);
        view
    });
    let render_view = inner.clone();
    let handle = cx.add_window(move |_, _| DiffHarness(render_view));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let warm = inner.update(&mut visual, |view, _| {
        assert_eq!(view.diff_viewport.rows.len(), 10004);
        assert_eq!(view.diff_viewport.partner(0, 0, 0), Some(5000));
        assert_eq!(view.diff_viewport.file_row(1), Some(10001));
        let cache = view.diff_viewport.syntax.borrow();
        assert!(
            !cache.entries.is_empty(),
            "the viewport must have a nonzero layout height"
        );
        assert!(
            cache.entries.len() < 300,
            "off-screen lines were highlighted: {}",
            cache.entries.len()
        );
        cache.entries.clone()
    });
    visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    inner.update(&mut visual, |view, _| {
        let cache = view.diff_viewport.syntax.borrow();
        for (key, runs) in &warm {
            assert!(Rc::ptr_eq(
                runs,
                cache.entries.get(key).expect("warm line was evicted")
            ));
        }
        view.diff_viewport.scroll.scroll_to(ListOffset {
            item_ix: 6000,
            offset_in_item: px(5.0),
        });
    });
    visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    inner.update(&mut visual, |view, cx| {
        assert!(view.diff_viewport.syntax.borrow().entries.len() < 600);
        view.scrolled_to_file = Some("last.rs".into());
        view.apply_pending_file_scroll(cx);
        assert_eq!(
            view.diff_viewport.scroll.logical_scroll_top().item_ix,
            10001
        );
        assert!(view.scrolled_to_file.is_none());
        view.reset_diff(cx);
        assert_eq!(view.diff_viewport.scroll.item_count(), 0);
        assert!(view.diff_viewport.syntax.borrow().entries.is_empty());
    });
}
