from pathlib import Path
import hashlib

root = Path('src/components')
parent = root / 'pull_requests.rs'
diff_path = root / 'pull_requests/diff.rs'
render = root / 'pull_requests/render.rs'

def read_checked(path, expected):
    data = path.read_bytes()
    digest = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
    if digest != expected:
        raise RuntimeError(f'Refusing to patch unexpected content in {path}: {digest}')
    return data.decode()

def once(text, before, after):
    if text.count(before) != 1:
        raise RuntimeError(f'Expected exactly one source anchor: {before[:100]!r}')
    return text.replace(before, after, 1)

if 'mod viewport;' in diff_path.read_text():
    print('Scrolling patch already applied.')
    raise SystemExit(0)

p = read_checked(parent, '572c6e853294b8e5416bd91ccd1b2a3d401fe65e')
d = read_checked(diff_path, '092742ae6d4743051447b18de59e59472c50ed26')
r = read_checked(render, '48c5630e3c1be0496634c24ef06cd88202a19d60')
p = once(p, '    diff_scroll: gpui::ScrollHandle,', '    diff_scroll: gpui::ScrollHandle,\n    diff_viewport: diff::DiffViewport,')
p = once(p, '            diff_scroll: gpui::ScrollHandle::new(),', '            diff_scroll: gpui::ScrollHandle::new(),\n            diff_viewport: Default::default(),')
p = once(p, '        self.diff.clear();', '        self.diff.clear();\n        self.diff_viewport = Default::default();')
r = once(r, '        self.apply_pending_file_scroll(cx);', '        self.prepare_diff_viewport(window, cx);\n        self.apply_pending_file_scroll(cx);')
d = once(d, 'use gpui::{Div, SharedString, div, prelude::*, px};', 'mod viewport;\npub(super) use viewport::DiffViewport;\n\nuse gpui::{Div, SharedString, div, prelude::*, px};')
start = d.index('    /// Syntax-highlighted diff text')
end = d.index('    /// Language name for the syntax highlighter', start)
d = d[:start] + '''    /// Bounded, diff-local syntax runs. The shared Markdown cache only retains
    /// sixteen blocks and is not a suitable per-line scrolling working set.
    fn code_runs(&self, text: &str, language: Option<&'static str>) -> std::rc::Rc<Vec<gpui::TextRun>> {
        self.diff_viewport.syntax_runs(text, language, None, || {
            let theme = self.theme();
            let mut base = crate::theme::Theme::for_mode(self.mode);
            base.file_editor_text = theme.syntax_plain;
            base.markdown_syntax_comment = theme.syntax_comment;
            base.markdown_syntax_keyword = theme.syntax_keyword;
            base.markdown_syntax_literal = theme.syntax_type;
            base.markdown_syntax_string = theme.syntax_string;
            base.markdown_syntax_variable = theme.syntax_type;
            base.markdown_syntax_attribute = theme.syntax_operator;
            base.markdown_syntax_name = theme.syntax_name;
            base.markdown_syntax_error = theme.syntax_error;
            crate::components::markdown::file_editor_runs(text, language, base)
                .into_iter().map(|(_, run)| run).collect()
        })
    }

    fn code_text(&self, text: &str, language: Option<&'static str>) -> gpui::StyledText {
        gpui::StyledText::new(text.to_owned()).with_runs(self.code_runs(text, language).as_ref().clone())
    }

    fn highlighted_line(&self, file_index: usize, file: &FileDiff, hunk: usize, index: usize) -> gpui::StyledText {
        let line = &file.hunks[hunk].lines[index];
        let language = Self::language_for(&file.path);
        let pair = self.words.then(|| self.diff_viewport.partner(file_index, hunk, index)).flatten();
        let Some(pair) = pair else { return self.code_text(&line.text, language); };
        let span = changed_span(&line.text, &file.hunks[hunk].lines[pair].text);
        let deleted = line.kind == LineKind::Deleted;
        let theme = self.theme();
        let color = if deleted { theme.diff_deleted_emphasis } else { theme.diff_added_emphasis };
        let runs = self.diff_viewport.syntax_runs(&line.text, language, Some((span.start, span.end, deleted)), || {
            let mut runs = Vec::new();
            for (range, run) in crate::components::markdown::file_editor_runs(
                &line.text, language, crate::theme::Theme::for_mode(self.mode),
            ) {
                let mut cuts = vec![range.start, range.end];
                cuts.extend([span.start, span.end].into_iter().filter(|cut| *cut > range.start && *cut < range.end));
                cuts.sort_unstable();
                cuts.dedup();
                for cut in cuts.windows(2) {
                    let mut run = run.clone();
                    run.len = cut[1] - cut[0];
                    if span.contains(&cut[0]) { run.background_color = Some(color.into()); }
                    runs.push(run);
                }
            }
            runs
        });
        gpui::StyledText::new(line.text.clone()).with_runs(runs.as_ref().clone())
    }

''' + d[end:]
start = d.index('    /// Reveals a file the user asked for:')
end = d.index('    pub(super) fn diff_surface', start)
d = d[:start] + '''    /// File navigation addresses a header in the flattened virtual row index,
    /// not the original file index (which no longer denotes a scroll child).
    pub(super) fn apply_pending_file_scroll(&mut self, _cx: &mut gpui::Context<Self>) {
        let Some(path) = self.scrolled_to_file.as_ref() else { return; };
        if (self.detail_tab != DetailTab::Code && self.review_tab.is_none()) || self.diff_loading {
            return;
        }
        let row = self.diff.iter().position(|file| &file.path == path)
            .and_then(|file| self.diff_viewport.file_row(file));
        match row {
            Some(item_ix) => {
                self.diff_viewport.scroll.scroll_to(gpui::ListOffset { item_ix, offset_in_item: px(0.0) });
                self.scrolled_to_file = None;
            }
            None if !self.diff.is_empty() => self.scrolled_to_file = None,
            None => {}
        }
    }

''' + d[end:]
d = once(d, '                    .overflow_scroll()\n                    .track_scroll(&self.diff_scroll)', '                    .min_w(px(0.0))\n                    .overflow_x_scroll()\n                    .track_scroll(&self.diff_scroll)')
d = once(d, '''    /// The diff column's direct children: one section per changed file, so the
    /// scroll handle can address them by index (`scroll_to_item`) when a review
    /// comment or the file tree asks to reveal one.''', '''    /// Loading/error/empty states remain regular elements; actual diff rows
    /// are built on demand by a single variable-height virtual list.''')
d = once(d, '''        files
            .into_iter()
            .map(|(index, file)| self.file_section(index, file, cx).into_any_element())
            .collect()''', '''        vec![self.virtual_diff_list(cx)]''')
start = d.index('    fn file_section(')
end = d.index('    /// The three nested buttons', start)
old = d[start:end]
header_end = old.index('        if !collapsed && self.rich')
header = old[:header_end].replace('fn file_section(', 'fn file_header(', 1).replace('let mut section =', 'let section =', 1)
preview_start = old.index('            section = section.child(') + len('            section = section.child(')
preview_end = old.index('            );\n        } else if !collapsed && file.binary', preview_start)
preview = old[preview_start:preview_end].strip()
d = d[:start] + header + '        section\n    }\n\n' + '''    fn file_preview(&self, file: &FileDiff, cx: &mut gpui::Context<Self>) -> Div {
''' + preview + '\n    }\n\n' + d[end:]
start = d.index('    fn hunk_section(')
end = d.index('    /// A real file line revealed', start)
d = d[:start] + '''    fn hunk_expander(
        &self, file_index: usize, file: &FileDiff, hunk_index: usize,
        gap: u32, cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let key = format!("{}:{hunk_index}", file.path);
        let view = cx.entity();
        div().id(SharedString::from(format!("pr-expander-{key}")))
            .h(px(32.0)).flex().items_center().cursor_pointer()
            .bg(theme.diff_expander_surface).role(gpui::Role::Button)
            .aria_label(SharedString::from(format!("{gap} unmodified lines")))
            .on_click(move |_, _, cx| {
                view.update(cx, |view, cx| view.expand_context(file_index, key.clone(), cx));
            })
            .child(div().w(px(GUTTER_WIDTH)).flex_none())
            .child(div().pl(px(14.0)).text_size(px(12.0))
                .text_color(theme.diff_gutter_text).child(format!("{gap} unmodified lines")))
    }

''' + d[end:]
d = once(d, '.child(self.highlighted_line(file, hunk_index, line_index))', '.child(self.highlighted_line(file_index, file, hunk_index, line_index))')
parent.write_text(p)
diff_path.write_text(d)
render.write_text(r)
print('Applied checked PR scrolling patch to three source files.')
