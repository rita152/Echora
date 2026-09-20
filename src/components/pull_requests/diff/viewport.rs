//! Row-level virtualization for the PR diff. No code elements or syntax runs
//! are built for off-screen rows during scrolling.

use super::super::InlineComment;
use super::*;
use crate::theme::ThemeMode;
use gpui::{ListAlignment, ListOffset, ListState, TextRun};
use std::{cell::RefCell, collections::HashMap, collections::VecDeque, rc::Rc};

const SYNTAX_BYTES: usize = 4 * 1024 * 1024;
const SYNTAX_LINES: usize = 1024;
type SyntaxKey = (String, Option<&'static str>, Option<(usize, usize, bool)>);

#[derive(Default)]
struct SyntaxCache {
    entries: HashMap<SyntaxKey, Rc<Vec<TextRun>>>,
    order: VecDeque<(SyntaxKey, usize)>,
    bytes: usize,
}
impl SyntaxCache {
    fn get_or_insert(
        &mut self,
        key: SyntaxKey,
        build: impl FnOnce() -> Vec<TextRun>,
    ) -> Rc<Vec<TextRun>> {
        if let Some(runs) = self.entries.get(&key) {
            return runs.clone();
        }
        let runs = Rc::new(build());
        let bytes = key.0.len() * 2
            + std::mem::size_of::<SyntaxKey>() * 2
            + runs.len() * std::mem::size_of::<TextRun>();
        // One exceptional line must not evict the entire working set.
        if bytes > SYNTAX_BYTES {
            return runs;
        }
        while self.bytes + bytes > SYNTAX_BYTES || self.entries.len() >= SYNTAX_LINES {
            let Some((old, bytes)) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&old);
            self.bytes -= bytes;
        }
        self.bytes += bytes;
        self.order.push_back((key.clone(), bytes));
        self.entries.insert(key, runs.clone());
        runs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowKind {
    Header,
    Preview,
    Binary,
    Gap {
        hunk: usize,
        count: u32,
    },
    Context {
        hunk: usize,
        number: u32,
    },
    Code {
        hunk: usize,
        left: Option<usize>,
        right: Option<usize>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Row {
    pub(super) file: usize,
    pub(super) kind: RowKind,
}
impl Row {
    fn same_anchor(self, other: Self) -> bool {
        if self.file != other.file {
            return false;
        }
        match (self.kind, other.kind) {
            (
                RowKind::Code { hunk, left, right },
                RowKind::Code {
                    hunk: other_hunk,
                    left: other_left,
                    right: other_right,
                },
            ) => {
                let line = right.or(left);
                hunk == other_hunk && line.is_some() && (line == other_left || line == other_right)
            }
            _ => self.kind == other.kind,
        }
    }
}
#[derive(PartialEq, Eq)]
struct RowsKey {
    generation: u64,
    count: usize,
    loading: bool,
    filter: String,
    collapsed: std::collections::HashSet<String>,
    expanded: std::collections::HashSet<String>,
    split: bool,
    rich: bool,
    contexts: HashMap<String, usize>,
    errors: HashMap<String, String>,
    inline: Option<InlineComment>,
}

pub(in crate::components::pull_requests) struct DiffViewport {
    key: Option<RowsKey>,
    pub(super) rows: Vec<Row>,
    pub(super) scroll: ListState,
    syntax: RefCell<SyntaxCache>,
    mode: Option<ThemeMode>,
    data: Option<(u64, usize)>,
    partners: Vec<Vec<Vec<Option<usize>>>>,
    width: f32,
    wrap: bool,
    max_line_width: Option<f32>,
}
impl Default for DiffViewport {
    fn default() -> Self {
        Self {
            key: None,
            rows: Vec::new(),
            scroll: ListState::new(0, ListAlignment::Top, px(LINE_HEIGHT * 10.0)),
            syntax: RefCell::new(SyntaxCache::default()),
            mode: None,
            data: None,
            partners: Vec::new(),
            width: 0.0,
            wrap: true,
            max_line_width: None,
        }
    }
}
impl DiffViewport {
    pub(super) fn syntax_runs(
        &self,
        text: &str,
        language: Option<&'static str>,
        emphasis: Option<(usize, usize, bool)>,
        build: impl FnOnce() -> Vec<TextRun>,
    ) -> Rc<Vec<TextRun>> {
        self.syntax
            .borrow_mut()
            .get_or_insert((text.to_owned(), language, emphasis), build)
    }
    pub(super) fn partner(&self, file: usize, hunk: usize, line: usize) -> Option<usize> {
        self.partners
            .get(file)?
            .get(hunk)?
            .get(line)
            .copied()
            .flatten()
    }
    pub(super) fn file_row(&self, file: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.file == file && row.kind == RowKind::Header)
    }
    fn restore(&self, anchor: Option<Row>, offset: gpui::Pixels) {
        let exact =
            anchor.and_then(|anchor| self.rows.iter().position(|row| anchor.same_anchor(*row)));
        let fallback = anchor.and_then(|anchor| self.file_row(anchor.file));
        self.scroll.scroll_to(ListOffset {
            item_ix: exact.or(fallback).unwrap_or(0),
            offset_in_item: if exact.is_some() { offset } else { px(0.0) },
        });
    }
}

impl PullRequestsView {
    /// Only metadata is compared on a redraw. The row index and deletion/addition
    /// pairing are rebuilt on data changes, not inside the per-line callback.
    pub(in crate::components::pull_requests) fn prepare_diff_viewport(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.detail_tab != DetailTab::Code && self.review_tab.is_none() {
            return;
        }
        let key = RowsKey {
            generation: self.diff_generation,
            count: self.diff.len(),
            loading: self.diff_loading,
            filter: self.tree_filter.read(cx).text().trim().to_lowercase(),
            collapsed: self.collapsed_files.clone(),
            expanded: self.expanded_context.clone(),
            split: self.split,
            rich: self.rich,
            contexts: self
                .file_lines
                .iter()
                .map(|(path, lines)| (path.clone(), lines.len()))
                .collect(),
            errors: self.file_errors.clone(),
            inline: self.inline_comment.clone(),
        };
        let data = (self.diff_generation, self.diff.len());
        if self.diff_viewport.data != Some(data) {
            self.diff_viewport.data = Some(data);
            self.diff_viewport.partners = self
                .diff
                .iter()
                .map(|file| {
                    file.hunks
                        .iter()
                        .map(|hunk| {
                            let mut partners = vec![None; hunk.lines.len()];
                            for (left, right) in split_pairs(&hunk.lines) {
                                if let (Some(left), Some(right)) = (left, right)
                                    && left != right
                                {
                                    partners[left] = Some(right);
                                    partners[right] = Some(left);
                                }
                            }
                            partners
                        })
                        .collect()
                })
                .collect();
            *self.diff_viewport.syntax.borrow_mut() = SyntaxCache::default();
        }
        let mode_changed = self.diff_viewport.mode != Some(self.mode);
        if mode_changed {
            self.diff_viewport.mode = Some(self.mode);
            *self.diff_viewport.syntax.borrow_mut() = SyntaxCache::default();
            self.diff_viewport.max_line_width = None;
        }
        let changed = self.diff_viewport.key.as_ref() != Some(&key);
        if changed {
            let top = self.diff_viewport.scroll.logical_scroll_top();
            let anchor = self.diff_viewport.rows.get(top.item_ix).copied();
            let rows = self.build_diff_rows(&key);
            let focus = self
                .inline_editor
                .as_ref()
                .map(|editor| gpui::Focusable::focus_handle(editor.read(cx), cx));
            let handles = rows
                .iter()
                .map(|row| self.row_has_inline(*row).then(|| focus.clone()).flatten())
                .collect::<Vec<_>>();
            let old_count = self.diff_viewport.scroll.item_count();
            self.diff_viewport
                .scroll
                .splice_focusable(0..old_count, handles);
            self.diff_viewport.rows = rows;
            // Source indices from another PR/scope are not valid anchors.
            let same_data = self
                .diff_viewport
                .key
                .as_ref()
                .is_some_and(|old| old.generation == key.generation && old.count == key.count);
            self.diff_viewport
                .restore(if same_data { anchor } else { None }, top.offset_in_item);
            self.diff_viewport.key = Some(key);
            self.diff_viewport.max_line_width = None;
        }
        let width = self.diff_view_width();
        if !changed
            && (self.diff_viewport.width != width
                || self.diff_viewport.wrap != self.wrap
                || mode_changed)
        {
            self.diff_viewport.scroll.remeasure();
        }
        self.diff_viewport.width = width;
        self.diff_viewport.wrap = self.wrap;
        if self.wrap {
            self.diff_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
        } else if self.diff_viewport.max_line_width.is_none() {
            // Keep native horizontal scrolling stable. Measure the no-wrap width
            // once, with the actual syntax font/runs, including fallback glyphs.
            let mut maximum = 0.0_f32;
            for row in &self.diff_viewport.rows {
                let file = &self.diff[row.file];
                let language = Self::language_for(&file.path);
                let mut measure = |text: &str| {
                    let runs = self.code_runs(text, language);
                    let line = window.text_system().shape_line(
                        text.to_owned().into(),
                        px(12.0),
                        runs.as_slice(),
                        None,
                    );
                    maximum = maximum.max(f32::from(line.width()));
                };
                match row.kind {
                    RowKind::Code { hunk, left, right } => {
                        for index in [left, right].into_iter().flatten() {
                            measure(&file.hunks[hunk].lines[index].text);
                        }
                    }
                    RowKind::Context { number, .. } => {
                        if let Some(text) = self
                            .file_lines
                            .get(&file.path)
                            .and_then(|lines| lines.get(number.saturating_sub(1) as usize))
                        {
                            measure(text);
                        }
                    }
                    _ => {}
                }
            }
            self.diff_viewport.max_line_width = Some(maximum + GUTTER_WIDTH + 28.0);
        }
    }

    fn build_diff_rows(&self, key: &RowsKey) -> Vec<Row> {
        let mut rows = Vec::new();
        for (file_index, file) in self.diff.iter().enumerate() {
            if !key.filter.is_empty() && !file.path.to_lowercase().contains(&key.filter) {
                continue;
            }
            let mut push = |kind| {
                rows.push(Row {
                    file: file_index,
                    kind,
                })
            };
            push(RowKind::Header);
            if key.collapsed.contains(&file.path) {
                continue;
            }
            if key.rich && file.path.ends_with(".md") && file.status != 'D' {
                push(RowKind::Preview);
                continue;
            }
            if file.binary {
                push(RowKind::Binary);
                continue;
            }
            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                let start = if hunk_index == 0 {
                    1
                } else {
                    hunk_new_end(&file.hunks[hunk_index - 1].header)
                        .map(|end| end.saturating_add(1))
                        .unwrap_or(1)
                };
                let gap = hunk_new_start(&hunk.header)
                    .unwrap_or(1)
                    .saturating_sub(start);
                let context_key = format!("{}:{hunk_index}", file.path);
                if key.expanded.contains(&context_key) {
                    if let Some(lines) = self.file_lines.get(&file.path) {
                        for offset in 0..(gap as usize)
                            .min(lines.len().saturating_sub(start.saturating_sub(1) as usize))
                        {
                            push(RowKind::Context {
                                hunk: hunk_index,
                                number: start + offset as u32,
                            });
                        }
                    }
                } else if gap > 0 {
                    push(RowKind::Gap {
                        hunk: hunk_index,
                        count: gap,
                    });
                }
                if key.split {
                    for (left, right) in split_pairs(&hunk.lines) {
                        push(RowKind::Code {
                            hunk: hunk_index,
                            left,
                            right,
                        });
                    }
                } else {
                    for (index, line) in hunk.lines.iter().enumerate() {
                        let old = line.kind == LineKind::Deleted;
                        push(RowKind::Code {
                            hunk: hunk_index,
                            left: old.then_some(index),
                            right: (!old).then_some(index),
                        });
                    }
                }
            }
        }
        rows
    }

    fn row_has_inline(&self, row: Row) -> bool {
        let Some(inline) = &self.inline_comment else {
            return false;
        };
        if self.diff[row.file].path != inline.path {
            return false;
        }
        let RowKind::Code { hunk, left, right } = row.kind else {
            return false;
        };
        let index = if inline.old { left } else { right };
        index.is_some_and(|index| {
            let line = &self.diff[row.file].hunks[hunk].lines[index];
            (if inline.old { line.old } else { line.new }) == Some(inline.line)
        })
    }
    fn diff_view_width(&self) -> f32 {
        (self.pane_width
            - if self.file_tree_open {
                self.tree_width()
            } else {
                0.0
            })
        .max(1.0)
    }
    fn diff_content_width(&self) -> f32 {
        if self.wrap {
            return self.diff_view_width();
        }
        let cell = self.diff_viewport.max_line_width.unwrap_or(0.0);
        (cell * if self.split { 2.0 } else { 1.0 } + if self.split { 1.0 } else { 0.0 })
            .max(self.diff_view_width())
    }
    pub(super) fn virtual_diff_list(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let view = cx.entity();
        gpui::list(self.diff_viewport.scroll.clone(), move |index, _, cx| {
            view.update(cx, |view, cx| view.render_diff_row(index, cx))
        })
        .w(px(self.diff_content_width()))
        .h_full()
        .into_any_element()
    }
    fn render_diff_row(&self, index: usize, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let Some(row) = self.diff_viewport.rows.get(index).copied() else {
            return div().into_any_element();
        };
        let file = &self.diff[row.file];
        let element = match row.kind {
            RowKind::Header => self.file_header(row.file, file, cx).into_any_element(),
            RowKind::Preview => self.file_preview(file, cx).into_any_element(),
            RowKind::Binary => div()
                .p(px(16.0))
                .child("Binary file changed. Open file to view it on GitHub.")
                .into_any_element(),
            RowKind::Gap { hunk, count } => self
                .hunk_expander(row.file, file, hunk, count, cx)
                .into_any_element(),
            RowKind::Context { hunk, number } => {
                let text = &self.file_lines[&file.path][number.saturating_sub(1) as usize];
                self.context_line(&format!("{}:{hunk}", file.path), number, text, cx)
                    .into_any_element()
            }
            RowKind::Code { hunk, left, right } if self.split => {
                let mut pair = div().w_full().flex().items_stretch();
                for (line, old) in [(left, true), (right, false)] {
                    let mut cell = div().flex_1().min_w(px(0.0));
                    if let Some(line) = line {
                        cell = cell.child(self.diff_line(row.file, file, hunk, line, old, cx));
                    }
                    pair = pair.child(cell);
                    if old {
                        pair = pair.child(div().flex_none().w(px(1.0)).bg(self.theme().border));
                    }
                }
                pair.into_any_element()
            }
            RowKind::Code { hunk, left, right } => {
                let line = right.or(left).expect("code rows contain a source line");
                self.diff_line(row.file, file, hunk, line, left.is_some(), cx)
                    .into_any_element()
            }
        };
        // Intercept horizontal/shift-wheel before List consumes Y. Vertical
        // scrolling stays on GPUI's native variable-height list path.
        div()
            .id(("pr-virtual-row", index))
            .w_full()
            .child(element)
            .on_scroll_wheel(cx.listener(|view, event: &gpui::ScrollWheelEvent, _, cx| {
                let delta = event.delta.pixel_delta(px(LINE_HEIGHT));
                if delta.x.abs() > delta.y.abs() || event.modifiers.shift {
                    if !view.wrap {
                        let horizontal = if event.modifiers.shift && delta.y != px(0.0) {
                            delta.y
                        } else {
                            delta.x
                        };
                        let maximum = (view.diff_content_width() - view.diff_view_width()).max(0.0);
                        let x = f32::from(view.diff_scroll.offset().x + horizontal)
                            .clamp(-maximum, 0.0);
                        view.diff_scroll.set_offset(gpui::point(px(x), px(0.0)));
                        cx.notify();
                    }
                    cx.stop_propagation();
                }
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    #[test]
    fn syntax_cache_reuses_more_than_the_shared_sixteen_line_cache() {
        let mut cache = SyntaxCache::default();
        for i in 0..100 {
            cache.get_or_insert((format!("let line_{i} = {i};"), Some("rs"), None), Vec::new);
        }
        for i in 0..100 {
            cache.get_or_insert((format!("let line_{i} = {i};"), Some("rs"), None), || {
                panic!("reparsed a warm line")
            });
        }
        for i in 0..2000 {
            cache.get_or_insert((format!("{i}{}", "x".repeat(8192)), None, None), Vec::new);
        }
        assert!(cache.bytes <= SYNTAX_BYTES);
        assert!(cache.entries.len() <= SYNTAX_LINES);
        assert_eq!(cache.entries.len(), cache.order.len());
        let count = cache.entries.len();
        cache.get_or_insert(("x".repeat(SYNTAX_BYTES), None, None), Vec::new);
        assert_eq!(cache.entries.len(), count);
    }
    #[test]
    fn split_and_unified_rows_share_source_anchors() {
        let deleted = Row {
            file: 2,
            kind: RowKind::Code {
                hunk: 3,
                left: Some(4),
                right: None,
            },
        };
        let split = Row {
            file: 2,
            kind: RowKind::Code {
                hunk: 3,
                left: Some(4),
                right: Some(5),
            },
        };
        assert!(deleted.same_anchor(split));
        assert!(!deleted.same_anchor(Row { file: 7, ..split }));
        let mut viewport = DiffViewport::default();
        viewport.rows = vec![Row {
            file: 2,
            kind: RowKind::Header,
        }];
        viewport.scroll.reset(1);
        viewport.restore(Some(deleted), px(11.0));
        let top = viewport.scroll.logical_scroll_top();
        assert_eq!(top.item_ix, 0);
        assert_eq!(top.offset_in_item, px(0.0));
    }
    #[gpui::test]
    fn row_index_filters_collapses_and_preserves_original_file_indices(cx: &mut TestAppContext) {
        let view = cx.new(|cx| PullRequestsView::build(ThemeMode::Dark, None, false, cx));
        view.update(cx, |view, _| {
            view.diff = crate::git_review::parse_unified("diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\ndiff --git a/b.rs b/b.rs\n--- a/b.rs\n+++ b/b.rs\n@@ -4 +4 @@\n-before\n+after\n");
            let mut key = RowsKey {
                generation: 1, count: 2, loading: false, filter: String::new(), collapsed: Default::default(), expanded: Default::default(), split: false,
                rich: false, contexts: Default::default(), errors: Default::default(), inline: None,
            };
            let rows = view.build_diff_rows(&key);
            assert_eq!(rows.len(), 7);
            assert_eq!(rows[3], Row { file: 1, kind: RowKind::Header });
            assert_eq!(rows[4].kind, RowKind::Gap { hunk: 0, count: 3 });
            key.filter = "b.rs".into();
            assert_eq!(view.build_diff_rows(&key)[0].file, 1);
            key.collapsed.insert("b.rs".into());
            assert_eq!(view.build_diff_rows(&key), vec![Row { file: 1, kind: RowKind::Header }]);
            key.collapsed.clear();
            key.expanded.insert("b.rs:0".into());
            view.file_lines.insert("b.rs".into(), vec!["one".into(), "two".into(), "three".into(), "after".into()]);
            key.split = true;
            let rows = view.build_diff_rows(&key);
            assert_eq!(rows.len(), 5);
            assert_eq!(rows[3].kind, RowKind::Context { hunk: 0, number: 3 });
            assert_eq!(rows[4].kind, RowKind::Code { hunk: 0, left: Some(0), right: Some(1) });
        });
    }
}

#[cfg(test)]
mod render_tests;
