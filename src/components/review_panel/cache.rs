//! Snapshot-derived metrics and bounded syntax runs for the virtual diff viewport.
//! Lines take the reference's Codex code theme and its `word-alt` spans.
//!
//! Scrolling, selection, and hover may redraw the same line many times. Parsing
//! it again or searching the entire file for its word-diff partner is unnecessary.

use super::*;
use crate::components::{diff_syntax, diff_words};
use gpui::TextRun;
use std::{collections::VecDeque, ops::Range, rc::Rc, sync::Weak};

mod file_tree;
pub(super) use file_tree::TreeRow;

const MAX_SYNTAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_SYNTAX_LINES: usize = 1024;
pub(super) type SyntaxRuns = Vec<(Range<usize>, TextRun)>;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct LineKey {
    file: usize,
    old: Option<u32>,
    new: Option<u32>,
}
impl LineKey {
    fn new(file: usize, line: &Line) -> Self {
        Self {
            file,
            old: line.old,
            new: line.new,
        }
    }
}

struct CachedSyntax {
    text: String,
    runs: Rc<SyntaxRuns>,
    bytes: usize,
}

#[derive(Default)]
pub(super) struct RenderCache {
    snapshot: Weak<Snapshot>,
    pub(super) tree: file_tree::TreeCache,
    pub(super) tree_scroll: gpui::UniformListScrollHandle,
    mode: Option<ThemeMode>,
    languages: Vec<Option<&'static str>>,
    words: HashMap<LineKey, Vec<Range<usize>>>,
    syntax: HashMap<LineKey, CachedSyntax>,
    order: VecDeque<LineKey>,
    syntax_bytes: usize,
    pub(super) max_line_width: f32,
    pub(super) gutter_width: f32,
    #[cfg(test)]
    parses: usize,
}

impl RenderCache {
    pub(super) fn prepare(&mut self, snapshot: &Arc<Snapshot>, mode: ThemeMode) {
        if self.snapshot.as_ptr() != Arc::as_ptr(snapshot) {
            self.snapshot = Arc::downgrade(snapshot);
            self.tree.invalidate();
            self.languages.clear();
            self.words.clear();
            self.clear_syntax();
            self.max_line_width = 0.;
            let mut largest_line = 1;
            for (file_index, file) in snapshot.files.iter().enumerate() {
                self.languages.push(diff_syntax::language_for(&file.path));
                for hunk in &file.hunks {
                    for line in &hunk.lines {
                        self.max_line_width = self
                            .max_line_width
                            .max(line.text.chars().count() as f32 * 7.2246 + 20.);
                        largest_line = largest_line
                            .max(line.old.unwrap_or(0))
                            .max(line.new.unwrap_or(0));
                    }
                    let mut i = 0;
                    while i < hunk.lines.len() {
                        if hunk.lines[i].kind != LineKind::Deleted {
                            i += 1;
                            continue;
                        }
                        let start = i;
                        while i < hunk.lines.len() && hunk.lines[i].kind == LineKind::Deleted {
                            i += 1;
                        }
                        let added = i;
                        while i < hunk.lines.len() && hunk.lines[i].kind == LineKind::Added {
                            i += 1;
                        }
                        for offset in 0..(added - start).min(i - added) {
                            let old = &hunk.lines[start + offset];
                            let new = &hunk.lines[added + offset];
                            let (old_spans, new_spans) =
                                diff_words::changed_spans(&old.text, &new.text);
                            self.words.insert(LineKey::new(file_index, old), old_spans);
                            self.words.insert(LineKey::new(file_index, new), new_spans);
                        }
                    }
                }
            }
            // Include padding and the change marker. Five-digit source lines
            // must not wrap and double the virtual list's measured row height.
            let digits = largest_line.ilog10() + 1;
            self.gutter_width = 52.5625_f32.max((digits as f32 * 7.2246 + 19.).ceil());
        }
        if self.mode != Some(mode) {
            self.mode = Some(mode);
            self.clear_syntax();
        }
    }

    fn clear_syntax(&mut self) {
        self.syntax.clear();
        self.order.clear();
        self.syntax_bytes = 0;
    }

    pub(super) fn word_spans(&self, file: usize, line: &Line) -> Vec<Range<usize>> {
        self.words
            .get(&LineKey::new(file, line))
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn syntax_runs(&mut self, file: usize, line: &Line) -> Rc<SyntaxRuns> {
        let key = LineKey::new(file, line);
        if let Some(cached) = self.syntax.get(&key) {
            if cached.text == line.text {
                return cached.runs.clone();
            }
            // Defensive for overlapping history hunks that reuse a source line.
            self.syntax_bytes -= cached.bytes;
            self.syntax.remove(&key);
            self.order.retain(|k| *k != key);
        }
        #[cfg(test)]
        {
            self.parses += 1;
        }
        let runs = Rc::new(diff_syntax::code_spans(
            &line.text,
            self.languages.get(file).copied().flatten(),
            self.mode.unwrap_or(ThemeMode::Dark),
        ));
        let bytes = line.text.len() + runs.len() * std::mem::size_of::<(Range<usize>, TextRun)>();
        // Very large individual lines can be displayed, but do not evict the
        // entire working set to retain a single oversized entry.
        if bytes > MAX_SYNTAX_BYTES {
            return runs;
        }
        while self.syntax_bytes + bytes > MAX_SYNTAX_BYTES || self.syntax.len() >= MAX_SYNTAX_LINES
        {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.syntax.remove(&oldest) {
                self.syntax_bytes -= old.bytes;
            }
        }
        self.syntax_bytes += bytes;
        self.syntax.insert(
            key,
            CachedSyntax {
                text: line.text.clone(),
                runs: runs.clone(),
                bytes,
            },
        );
        self.order.push_back(key);
        runs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(patch: &str) -> Arc<Snapshot> {
        Arc::new(Snapshot {
            files: git_review::parse_unified(patch),
            ..Default::default()
        })
    }

    #[test]
    fn tree_cache_invalidates_with_snapshot_but_not_theme() {
        let original = snapshot(
            "diff --git a/before.rs b/before.rs\n--- a/before.rs\n+++ b/before.rs\n@@ -1 +1 @@\n-old\n+new\n",
        );
        let mut cache = RenderCache::default();
        let collapsed = HashSet::new();
        cache.prepare(&original, ThemeMode::Dark);
        let first = cache.tree.prepare(
            "",
            &collapsed,
            original.files.iter().map(|file| file.path.as_str()),
        );
        cache.prepare(&original, ThemeMode::Light);
        let recolored = cache.tree.prepare(
            "",
            &collapsed,
            original.files.iter().map(|file| file.path.as_str()),
        );
        assert!(Arc::ptr_eq(&first, &recolored));
        let changed = snapshot(
            "diff --git a/after.rs b/after.rs\n--- a/after.rs\n+++ b/after.rs\n@@ -1 +1 @@\n-old\n+new\n",
        );
        cache.prepare(&changed, ThemeMode::Light);
        let replaced = cache.tree.prepare(
            "",
            &collapsed,
            changed.files.iter().map(|file| file.path.as_str()),
        );
        assert!(!Arc::ptr_eq(&first, &replaced));
        assert!(matches!(&replaced[0], TreeRow::File { index: 0, name, .. } if name == "after.rs"));
    }

    #[test]
    fn redraw_reuses_syntax_and_invalidates_for_theme_and_snapshot_changes() {
        let original = snapshot(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 2;\n",
        );
        let mut cache = RenderCache::default();
        cache.prepare(&original, ThemeMode::Dark);
        let line = &original.files[0].hunks[0].lines[1];
        let first = cache.syntax_runs(0, line);
        for _ in 0..20 {
            cache.prepare(&original, ThemeMode::Dark);
            assert!(Rc::ptr_eq(&first, &cache.syntax_runs(0, line)));
        }
        assert_eq!(cache.parses, 1);
        assert_eq!(
            *first,
            diff_syntax::code_spans(&line.text, Some("rs"), ThemeMode::Dark)
        );
        cache.prepare(&original, ThemeMode::Light);
        let light = cache.syntax_runs(0, line);
        assert_ne!(*first, *light);
        let changed = snapshot(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-let old = 1;\n+let changed = 3;\n",
        );
        cache.prepare(&changed, ThemeMode::Dark);
        let line = &changed.files[0].hunks[0].lines[1];
        assert_eq!(
            *cache.syntax_runs(0, line),
            diff_syntax::code_spans(&line.text, Some("rs"), ThemeMode::Dark)
        );
        assert_eq!(cache.parses, 3);
    }

    #[test]
    fn syntax_working_set_is_bounded_when_traversing_long_files() {
        let source =
            snapshot("diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-x\n+y\n");
        let mut cache = RenderCache::default();
        cache.prepare(&source, ThemeMode::Dark);
        for n in 0..2000 {
            cache.syntax_runs(
                0,
                &Line {
                    old: None,
                    new: Some(n),
                    kind: LineKind::Added,
                    text: "x".repeat(8192),
                },
            );
        }
        assert!(cache.syntax_bytes <= MAX_SYNTAX_BYTES);
        assert!(cache.syntax.len() <= MAX_SYNTAX_LINES);
        assert_eq!(cache.syntax.len(), cache.order.len());
    }

    #[test]
    fn word_index_pairs_within_each_hunk_and_preserves_unicode_boundaries() {
        let source = snapshot(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,3 @@\n-old 中文🙂 end\n+new 中文🙂 end\n+unpaired\n context\n@@ -10000 +10001 @@\n-deep value\n+deep changed\n",
        );
        let mut cache = RenderCache::default();
        cache.prepare(&source, ThemeMode::Dark);
        let lines = &source.files[0].hunks[0].lines;
        assert_eq!(cache.word_spans(0, &lines[0]), vec![0..3]);
        assert_eq!(cache.word_spans(0, &lines[1]), vec![0..3]);
        assert!(cache.word_spans(0, &lines[2]).is_empty());
        assert!(cache.word_spans(0, &lines[3]).is_empty());
        let deep = &source.files[0].hunks[1].lines[1];
        assert_eq!(&deep.text[cache.word_spans(0, deep)[0].clone()], "changed");
        assert!(cache.gutter_width >= 5. * 7.2246 + 18.);
    }
}
