from pathlib import Path

base = Path('src/components/pull_requests')
viewport = base / 'diff/viewport.rs'
tests = base / 'diff/viewport/render_tests.rs'
diff = base / 'diff.rs'

def replace_once(source, before, after):
    if after in source:
        return source
    if source.count(before) != 1:
        raise RuntimeError(f'Unexpected patch anchor: {before[:120]!r}')
    return source.replace(before, after, 1)

v = viewport.read_text()
if 'Seed off-screen rows without measuring them' not in v:
    v = replace_once(v,
'''            self.diff_viewport
                .scroll
                .splice_focusable(0..old_count, handles);''',
'''            self.diff_viewport
                .scroll
                .splice_focusable(0..old_count, handles);
            // Seed off-screen rows without measuring them. Otherwise GPUI's
            // pixel scrolling is limited to the small measured prefix. These
            // are estimates: wrapped lines/editors replace them on measurement.
            self.diff_viewport.scroll = self.diff_viewport.scroll.clone()
                .with_uniform_item_height(px(LINE_HEIGHT));''')
v = replace_once(v,
'''            .id(("pr-virtual-row", index))
            .w_full()
            .child(element)''',
'''            .id(("pr-virtual-row", index))
            .w_full()
            .flex()
            .flex_col()
            .child(element)''')
viewport.write_text(v)

d = diff.read_text()
start = d.index('    fn diff_body(')
a = d.find('        let filter = self.tree_filter.read(cx).text().trim().to_lowercase();', start)
b = d.index('    fn file_header(', start)
if a != -1 and a < b:
    end = d.index('        if files.is_empty() {', a) + len('        if files.is_empty() {')
    d = d[:a] + '        if self.diff_viewport.rows.is_empty() {' + d[end:]
diff.write_text(d)

t = tests.read_text()
if 'pixel scroll lost its off-screen estimates' not in t:
    a = t.index('    inner.update(&mut visual, |view, _| {\n        let cache')
    b = t.index('    visual.update(|window, cx| {', a)
    replacement = '''    inner.update(&mut visual, |view, cx| {
        let cache = view.diff_viewport.syntax.borrow();
        for (key, runs) in &warm {
            assert!(Rc::ptr_eq(runs, cache.entries.get(key).expect("warm line was evicted")));
        }
        drop(cache);
        // Exercise pixel-based movement as well as direct logical-row jumps.
        // With no off-screen height hints this seeks past the measured prefix
        // straight to the end, rather than approximately three thousand rows.
        view.diff_viewport.scroll.scroll_by(px(LINE_HEIGHT * 3000.0));
        let top = view.diff_viewport.scroll.logical_scroll_top().item_ix;
        assert!((2900..3100).contains(&top), "pixel scroll lost its off-screen estimates: {top}");
        cx.notify();
    });
    visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    inner.update(&mut visual, |view, cx| {
        assert!(view.diff_viewport.syntax.borrow().entries.len() < 600);
        view.diff_viewport.scroll.scroll_to(ListOffset { item_ix: 6000, offset_in_item: px(5.0) });
        cx.notify();
    });
'''
    t = t[:a] + replacement + t[b:]
if 'the distant viewport did not render' not in t:
    t = replace_once(t,
'''        assert!(view.diff_viewport.syntax.borrow().entries.len() < 600);
        view.scrolled_to_file''',
'''        let cache = view.diff_viewport.syntax.borrow();
        assert!(cache.entries.len() < 600);
        assert!(cache.entries.keys().any(|key| key.0.starts_with("let new_")), "the distant viewport did not render");
        drop(cache);
        view.scrolled_to_file''')
tests.write_text(t)
print('Applied height estimates, row stretching and pixel-scroll regression coverage.')
