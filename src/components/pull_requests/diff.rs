//! `Code` and review tabs: toolbar, file headers, hunks, and the file tree.

mod viewport;
pub(super) use viewport::DiffViewport;

use gpui::{Div, SharedString, div, prelude::*, px};

use super::{DetailTab, PullRequestsView, ReviewScope, theme::LIST_PANE_WIDTH};
use crate::components::icons::icon;
use crate::git_review::{FileDiff, LineKind};
use crate::theme::UI_MONOSPACE_FONT_FAMILY;

const GUTTER_WIDTH: f32 = 53.0;
/// Row height of a single diff line, and the line box inside it (the reference
/// viewer uses a 21.5px line box, so a two-line row measures 43px).
const LINE_HEIGHT: f32 = 22.0;
const CODE_LINE_HEIGHT: f32 = 21.5;
/// The reference file tree panel: x 1080 → 1440 at a 1440px window, i.e. a
/// 360px column that reflows the diff into the remaining 286px.
const TREE_WIDTH: f32 = 360.0;
/// File-tree row height and radius measured from the reference rows.
const TREE_ROW_HEIGHT: f32 = 29.0;
const TREE_ROW_RADIUS: f32 = 6.0;
/// Each tree level indents its chevron by this much (1102 → 1119.5 → 1137).
const TREE_INDENT: f32 = 17.5;

/// The status a folder row advertises: `A` when every changed descendant is
/// added, otherwise `M`.
fn tree_status(files: &[&FileDiff]) -> (char, bool) {
    let all_added = !files.is_empty() && files.iter().all(|file| file.status == 'A');
    if all_added { ('A', true) } else { ('M', false) }
}

/// Splits the files below `prefix` into "has direct files" and the distinct
/// subfolders directly under it, in first-seen order.
fn tree_children(prefix: &str, files: &[&FileDiff]) -> (bool, Vec<String>) {
    let mut direct = false;
    let mut folders: Vec<String> = Vec::new();
    for file in files {
        let remainder = file.path.strip_prefix(prefix).unwrap_or(&file.path);
        if remainder.is_empty() {
            continue;
        }
        match remainder.split_once('/') {
            Some((folder, _)) => {
                let path = format!("{prefix}{folder}");
                if !folders.contains(&path) {
                    folders.push(path);
                }
            }
            None => direct = true,
        }
    }
    (direct, folders)
}

impl PullRequestsView {
    /// Bounded, diff-local syntax runs. The shared Markdown cache only retains
    /// sixteen blocks and is not a suitable per-line scrolling working set.
    fn code_runs(
        &self,
        text: &str,
        language: Option<&'static str>,
    ) -> std::rc::Rc<Vec<gpui::TextRun>> {
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
                .into_iter()
                .map(|(_, run)| run)
                .collect()
        })
    }

    fn code_text(&self, text: &str, language: Option<&'static str>) -> gpui::StyledText {
        gpui::StyledText::new(text.to_owned())
            .with_runs(self.code_runs(text, language).as_ref().clone())
    }

    fn highlighted_line(
        &self,
        file_index: usize,
        file: &FileDiff,
        hunk: usize,
        index: usize,
    ) -> gpui::StyledText {
        let line = &file.hunks[hunk].lines[index];
        let language = Self::language_for(&file.path);
        let pair = self
            .words
            .then(|| self.diff_viewport.partner(file_index, hunk, index))
            .flatten();
        let Some(pair) = pair else {
            return self.code_text(&line.text, language);
        };
        let span = changed_span(&line.text, &file.hunks[hunk].lines[pair].text);
        let deleted = line.kind == LineKind::Deleted;
        let theme = self.theme();
        let color = if deleted {
            theme.diff_deleted_emphasis
        } else {
            theme.diff_added_emphasis
        };
        let runs = self.diff_viewport.syntax_runs(
            &line.text,
            language,
            Some((span.start, span.end, deleted)),
            || {
                let mut runs = Vec::new();
                for (range, run) in crate::components::markdown::file_editor_runs(
                    &line.text,
                    language,
                    crate::theme::Theme::for_mode(self.mode),
                ) {
                    let mut cuts = vec![range.start, range.end];
                    cuts.extend(
                        [span.start, span.end]
                            .into_iter()
                            .filter(|cut| *cut > range.start && *cut < range.end),
                    );
                    cuts.sort_unstable();
                    cuts.dedup();
                    for cut in cuts.windows(2) {
                        let mut run = run.clone();
                        run.len = cut[1] - cut[0];
                        if span.contains(&cut[0]) {
                            run.background_color = Some(color.into());
                        }
                        runs.push(run);
                    }
                }
                runs
            },
        );
        gpui::StyledText::new(line.text.clone()).with_runs(runs.as_ref().clone())
    }

    /// Language name for the syntax highlighter, derived from the file suffix.
    fn language_for(path: &str) -> Option<&'static str> {
        let extension = path.rsplit('.').next().unwrap_or_default();
        Some(match extension {
            "rs" => "rs",
            "toml" => "toml",
            "json" => "json",
            "md" => "markdown",
            "py" => "python",
            "js" | "mjs" | "cjs" => "javascript",
            "ts" => "typescript",
            "sh" | "zsh" | "bash" => "bash",
            "yml" | "yaml" => "yaml",
            "c" | "h" => "c",
            "cpp" | "cc" | "hpp" => "cpp",
            "go" => "go",
            "html" => "html",
            "css" => "css",
            _ => return None,
        })
    }

    /// The diff code column: pane width minus the file tree, the 53px gutter,
    /// and the 14px padding on each side of the code.
    pub(super) fn measure_code_width(&mut self, window: &gpui::Window) {
        let viewport = f32::from(window.viewport_size().width);
        let page = if self.is_fullscreen() {
            viewport
        } else {
            viewport - crate::components::sidebar::SIDEBAR_WIDTH
        };
        let compact = page < 850.0;
        self.list_width = if compact {
            if self.selected.is_none() { page } else { 0.0 }
        } else {
            LIST_PANE_WIDTH.min(page * 0.445)
        };
        self.pane_width = if self.fullscreen || (compact && self.selected.is_some()) {
            page
        } else {
            page - self.list_width
        };
        let tree = if self.file_tree_open {
            self.tree_width()
        } else {
            0.0
        };
        let cell = (self.pane_width - tree) / if self.split { 2.0 } else { 1.0 };
        self.set_code_width((cell - GUTTER_WIDTH).max(40.0));
    }

    fn tree_width(&self) -> f32 {
        TREE_WIDTH.min(self.pane_width * 0.42)
    }

    /// File navigation addresses a header in the flattened virtual row index,
    /// not the original file index (which no longer denotes a scroll child).
    pub(super) fn apply_pending_file_scroll(&mut self, _cx: &mut gpui::Context<Self>) {
        let Some(path) = self.scrolled_to_file.as_ref() else {
            return;
        };
        if (self.detail_tab != DetailTab::Code && self.review_tab.is_none()) || self.diff_loading {
            return;
        }
        let row = self
            .diff
            .iter()
            .position(|file| &file.path == path)
            .and_then(|file| self.diff_viewport.file_row(file));
        match row {
            Some(item_ix) => {
                self.diff_viewport.scroll.scroll_to(gpui::ListOffset {
                    item_ix,
                    offset_in_item: px(0.0),
                });
                self.scrolled_to_file = None;
            }
            None if !self.diff.is_empty() => self.scrolled_to_file = None,
            None => {}
        }
    }

    pub(super) fn diff_surface(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        // The diff toolbar spans the whole pane; the file tree sits beside the
        // scrolling diff, not below it, so opening it narrows the diff column.
        let diff_column = div()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("pr-diff-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .min_w(px(0.0))
                    .overflow_x_scroll()
                    .track_scroll(&self.diff_scroll)
                    .children(self.diff_body(cx)),
            );
        let mut body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(diff_column);
        if self.file_tree_open {
            body = body.child(self.file_tree(cx));
        }
        let surface = div()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .child(self.diff_toolbar(cx))
            .child(body);
        let _ = theme;
        surface
    }

    fn diff_toolbar(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let (head, base) = self
            .detail
            .as_ref()
            .map(|detail| {
                (
                    detail.summary.head_branch.clone(),
                    detail.summary.base_branch.clone(),
                )
            })
            .unwrap_or_default();
        div()
            .flex_none()
            .h(px(40.0))
            .px(px(16.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b(px(1.0))
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(13.0))
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis_middle()
                            .child(head),
                    )
                    .child(">")
                    .child(
                        div()
                            .max_w(px(120.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(base),
                    ),
            )
            .child(self.diff_toolbar_button(
                "pr-review-options",
                "more-horizontal",
                "Review options",
                cx,
            ))
            .child(self.diff_toolbar_button(
                "pr-split-toggle",
                "pr-split-diff",
                if self.split {
                    "Switch to unified diff"
                } else {
                    "Switch to split diff"
                },
                cx,
            ))
            .child(
                self.diff_toolbar_button(
                    "pr-collapse-all",
                    "review-collapse",
                    if self
                        .diff
                        .iter()
                        .all(|file| self.collapsed_files.contains(&file.path))
                    {
                        "Expand all diffs"
                    } else {
                        "Collapse all diffs"
                    },
                    cx,
                ),
            )
            .child(self.diff_toolbar_button(
                "pr-file-tree",
                "pr-file-tree",
                if self.file_tree_open {
                    "Hide file tree"
                } else {
                    "Show file tree"
                },
                cx,
            ))
    }

    fn diff_toolbar_button(
        &self,
        id: &'static str,
        glyph: &'static str,
        label: &'static str,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let open = (id == "pr-review-options" && self.review_options_open)
            || (id == "pr-file-tree" && self.file_tree_open);
        div()
            .id(id)
            .relative()
            .child(self.control_anchor(id))
            .flex_none()
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.5))
            .cursor_pointer()
            .when(open, |button| button.bg(theme.control_hover))
            .hover(move |style| style.bg(theme.control_hover))
            .role(gpui::Role::Button)
            .aria_label(label)
            .on_click(move |_, _, cx| match id {
                "pr-review-options" => view.update(cx, |view, cx| view.toggle_review_options(cx)),
                "pr-collapse-all" => view.update(cx, |view, cx| view.collapse_all(cx)),
                "pr-split-toggle" => view.update(cx, |view, cx| view.toggle_split(cx)),
                "pr-file-tree" => view.update(cx, |view, cx| view.toggle_file_tree(cx)),
                _ => {}
            })
            .child(icon(glyph, theme.text.into()).size(px(18.0)))
    }

    /// Loading/error/empty states remain regular elements; actual diff rows
    /// are built on demand by a single variable-height virtual list.
    fn diff_body(&self, cx: &mut gpui::Context<Self>) -> Vec<gpui::AnyElement> {
        let theme = self.theme();
        if self.diff_loading {
            return vec![
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(theme.text_muted)
                    .child("Loading pull request changes")
                    .into_any_element(),
            ];
        }
        if let Some(error) = self.diff_error.clone() {
            return vec![
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(16.0))
                    .text_size(px(13.0))
                    .text_color(theme.warning)
                    .child(error)
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .id("pr-retry-diff.rs")
                            .role(gpui::Role::Button)
                            .aria_label("Retry loading diff")
                            .cursor_pointer()
                            .child("Retry")
                            .on_click({
                                let view = cx.entity();
                                move |_, _, cx| {
                                    view.update(cx, |view, cx| {
                                        view.load_diff(cx);
                                    });
                                }
                            }),
                    )
                    .into_any_element(),
            ];
        }
        if self.diff_viewport.rows.is_empty() {
            return vec![
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(theme.text_muted)
                    .child("No changed files")
                    .into_any_element(),
            ];
        }
        vec![self.virtual_diff_list(cx)]
    }

    fn file_header(&self, index: usize, file: &FileDiff, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let collapsed = self.collapsed_files.contains(&file.path);
        let selected = self.selected_file.as_deref() == Some(file.path.as_str());
        let path = file.path.clone();
        let view = cx.entity();
        div().flex_none().flex().flex_col().child(
            div()
                .id(SharedString::from(format!("pr-file-{}", file.path)))
                .group("pr-file-header")
                .h(px(34.0))
                .px(px(14.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .bg(theme.diff_header_surface)
                .when(selected, |header| header.bg(theme.control))
                .child(
                    div()
                        .id(SharedString::from(format!("pr-file-name-{}", file.path)))
                        .h_full()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .cursor_pointer()
                        .role(gpui::Role::Button)
                        .aria_label(SharedString::from(file.path.clone()))
                        .on_click({
                            let view = view.clone();
                            let path = path.clone();
                            move |_, _, cx| {
                                view.update(cx, |view, cx| view.toggle_file(path.clone(), cx));
                            }
                        })
                        .child(icon("pr-open-file", theme.text_muted.into()).size(px(16.0)))
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(px(13.0))
                                .text_color(theme.text)
                                .child(file.path.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .text_size(px(12.0))
                                .child(
                                    div()
                                        .text_color(theme.additions_text)
                                        .child(format!("+{}", file.additions)),
                                )
                                .child(
                                    div()
                                        .text_color(theme.deletions_text)
                                        .child(format!("-{}", file.deletions)),
                                ),
                        ),
                )
                .child(self.file_header_actions(index, &path, collapsed, cx)),
        )
    }

    fn file_preview(&self, file: &FileDiff, cx: &mut gpui::Context<Self>) -> Div {
        div()
            .p(px(16.0))
            .child(match self.context_lines(&file.path) {
                Some(lines) => crate::components::markdown::render_pull_request_markdown(
                    &lines.join("\n"),
                    crate::theme::Theme::for_mode(self.mode),
                    &format!("pr-preview-{}", file.path),
                ),
                None => div()
                    .child(
                        self.file_errors
                            .get(&file.path)
                            .cloned()
                            .unwrap_or_else(|| "Loading Markdown preview…".into()),
                    )
                    .when(self.file_errors.contains_key(&file.path), |body| {
                        body.child(
                            div()
                                .id(SharedString::from(format!(
                                    "pr-preview-retry-{}",
                                    file.path
                                )))
                                .role(gpui::Role::Button)
                                .aria_label("Retry Markdown preview")
                                .cursor_pointer()
                                .child("Retry")
                                .on_click({
                                    let view = cx.entity();
                                    let path = file.path.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |view, cx| {
                                            view.load_file_lines(path.clone(), cx)
                                        });
                                    }
                                }),
                        )
                    }),
            })
    }

    /// The three nested buttons of a file header: `Copy path`,
    /// `Toggle file diff`, and `Open file`. The reference reveals them when the
    /// header is hovered.
    fn file_header_actions(
        &self,
        _index: usize,
        path: &str,
        _collapsed: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .opacity(0.0)
            .group_hover("pr-file-header", |style| style.opacity(1.0));
        for (id, glyph, label) in [
            ("pr-copy-path", "pr-copy-path", "Copy path"),
            ("pr-toggle-file", "pr-toggle-file", "Toggle file diff"),
            ("pr-open-file", "pr-open-file", "Open file"),
        ] {
            let path = path.to_string();
            let view = view.clone();
            row = row.child(
                div()
                    .id(SharedString::from(format!("{id}-{path}")))
                    .size(px(22.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(11.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.control_hover))
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(label.to_string()))
                    .on_click(move |_, _, cx| match id {
                        "pr-copy-path" => {
                            view.update(cx, |view, cx| view.copy_path(path.clone(), cx))
                        }
                        "pr-toggle-file" => {
                            view.update(cx, |view, cx| view.toggle_file(path.clone(), cx))
                        }
                        _ => view.update(cx, |view, cx| view.open_file(path.clone(), None, cx)),
                    })
                    .child(icon(glyph, theme.text_muted.into()).size(px(16.0))),
            );
        }
        row
    }

    fn hunk_expander(
        &self,
        file_index: usize,
        file: &FileDiff,
        hunk_index: usize,
        gap: u32,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let key = format!("{}:{hunk_index}", file.path);
        let view = cx.entity();
        div()
            .id(SharedString::from(format!("pr-expander-{key}")))
            .h(px(32.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .bg(theme.diff_expander_surface)
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!("{gap} unmodified lines")))
            .on_click(move |_, _, cx| {
                view.update(cx, |view, cx| {
                    view.expand_context(file_index, key.clone(), cx)
                });
            })
            .child(div().w(px(GUTTER_WIDTH)).flex_none())
            .child(
                div()
                    .pl(px(14.0))
                    .text_size(px(12.0))
                    .text_color(theme.diff_gutter_text)
                    .child(format!("{gap} unmodified lines")),
            )
    }

    /// A real file line revealed by an expander.
    fn context_line(
        &self,
        key: &str,
        number: u32,
        text: &str,
        _cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let theme = self.theme();
        div()
            .id(SharedString::from(format!("pr-context-{key}-{number}")))
            .h(px(LINE_HEIGHT))
            .flex()
            .items_center()
            .bg(theme.surface)
            .child(
                div()
                    .w(px(GUTTER_WIDTH))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .pr(px(10.0))
                    .text_size(px(12.0))
                    .font_family(UI_MONOSPACE_FONT_FAMILY)
                    .text_color(theme.diff_gutter_text)
                    .child(number.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .font_family(UI_MONOSPACE_FONT_FAMILY)
                    .text_size(px(12.0))
                    .line_height(px(CODE_LINE_HEIGHT))
                    .text_color(theme.diff_context_text)
                    .child(self.code_text(text, Self::language_for(key))),
            )
    }

    fn diff_line(
        &self,
        file_index: usize,
        file: &FileDiff,
        hunk_index: usize,
        line_index: usize,
        old: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let line = &file.hunks[hunk_index].lines[line_index];
        let theme = self.theme();
        let selected = self.inline_comment.as_ref().is_some_and(|inline| {
            inline.path == file.path
                && inline.old == old
                && inline.line == if old { line.old } else { line.new }.unwrap_or(0)
        });
        let view = cx.entity();
        let path = file.path.clone();
        // Review comments anchor to the new-file line, and to the old-file line
        // for deletions; a row without either number offers no comment button.
        let anchor = if old { line.old } else { line.new };
        let line_number = anchor.unwrap_or(0);
        let row =
            div()
                .min_h(px(LINE_HEIGHT))
                .min_w(px(0.0))
                .flex()
                .items_stretch()
                .bg(match line.kind {
                    LineKind::Added => theme.diff_added_surface,
                    LineKind::Deleted => theme.diff_deleted_surface,
                    LineKind::Context => theme.surface,
                })
                .child(
                    div()
                        .w(px(GUTTER_WIDTH))
                        .flex_none()
                        .flex()
                        .justify_end()
                        .pr(px(10.0))
                        .bg(match line.kind {
                            LineKind::Added => theme.diff_added_emphasis,
                            LineKind::Deleted => theme.diff_deleted_emphasis,
                            LineKind::Context => theme.surface,
                        })
                        .text_size(px(12.0))
                        .font_family(UI_MONOSPACE_FONT_FAMILY)
                        .text_color(match line.kind {
                            LineKind::Added => theme.diff_added_text,
                            LineKind::Deleted => theme.diff_deleted_text,
                            LineKind::Context => theme.diff_gutter_text,
                        })
                        .when(anchor.is_some(), |gutter| {
                            gutter.child(
                                div().h(px(LINE_HEIGHT)).flex().items_center().child(
                                    anchor.map(|number| number.to_string()).unwrap_or_default(),
                                ),
                            )
                        }),
                )
                .child(
                    // Wrapping needs a definite width on a block that directly
                    // holds the text; a flex child would be measured unconstrained.
                    div()
                        .min_w(px(0.0))
                        .px(px(14.0))
                        .font_family(UI_MONOSPACE_FONT_FAMILY)
                        .text_size(px(12.0))
                        .text_color(theme.diff_context_text)
                        .line_height(px(CODE_LINE_HEIGHT))
                        .when(self.wrap, |code| {
                            code.w(px(self.code_width())).whitespace_normal()
                        })
                        .when(!self.wrap, |code| code.whitespace_nowrap())
                        .child(self.highlighted_line(file_index, file, hunk_index, line_index)),
                );
        div()
            .relative()
            .group("pr-line")
            .child(row)
            .child(
                div()
                    // Unique per row: a deleted and an added line can carry the
                    // same new-file number, and duplicate ids abort the a11y
                    // tree in debug builds.
                    .id(SharedString::from(format!(
                        "pr-line-add-{}-{hunk_index}-{line_index}-{old}",
                        file.path
                    )))
                    .absolute()
                    .left(px(2.0))
                    .top(px(0.0))
                    .size(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .bg(theme.surface)
                    .cursor_pointer()
                    .opacity(0.0)
                    .group_hover("pr-line", |style| style.opacity(1.0))
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(format!(
                        "Add comment on line {line_number}"
                    )))
                    .on_click({
                        let view = view.clone();
                        let path = path.clone();
                        move |_, _, cx| {
                            view.update(cx, |view, cx| {
                                view.begin_inline_comment(
                                    file_index,
                                    path.clone(),
                                    line_number,
                                    old,
                                    cx,
                                );
                            });
                        }
                    })
                    .child(icon("add", theme.text_muted.into()).size(px(14.0))),
            )
            .when(selected, |container| {
                container.child(self.inline_comment_box(cx))
            })
            .when(
                self.inline_comment.is_none() && hunk_index == 0 && line_index == 0,
                |container| container,
            )
    }

    fn inline_comment_box(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let has_text = !self.mutation_pending
            && self
                .inline_editor
                .as_ref()
                .is_some_and(|editor| !editor.read(cx).text().trim().is_empty());
        let cancel = view.clone();
        div()
            .p(px(8.0))
            .pl(px(GUTTER_WIDTH))
            .bg(theme.surface)
            .child(
                div()
                    .p(px(8.0))
                    .rounded(px(16.0))
                    .bg(theme.field_surface)
                    .border(px(1.0))
                    .border_color(theme.field_border)
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(
                        self.inline_editor.clone().map(|editor| {
                            Self::editor_frame(editor, 120.0, "pr-inline-editor-frame")
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .id("pr-inline-cancel")
                                    .h(px(28.0))
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(12.5))
                                    .text_size(px(13.0))
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.control_hover))
                                    .role(gpui::Role::Button)
                                    .aria_label("Cancel")
                                    .on_click(move |_, _, cx| {
                                        cancel
                                            .update(cx, |view, cx| view.cancel_inline_comment(cx));
                                    })
                                    .child("Cancel"),
                            )
                            .child(
                                div()
                                    .id("pr-inline-comment")
                                    .h(px(28.0))
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(12.5))
                                    .text_size(px(13.0))
                                    .when(has_text, |button| {
                                        button
                                            .bg(theme.inverted_surface)
                                            .text_color(theme.inverted_text)
                                            .cursor_pointer()
                                            .on_click(move |_, _, cx| {
                                                view.update(cx, |view, cx| {
                                                    view.submit_inline_comment(cx)
                                                });
                                            })
                                    })
                                    .when(!has_text, |button| {
                                        button
                                            .bg(theme.control)
                                            .text_color(theme.text_muted)
                                            .opacity(0.6)
                                    })
                                    .role(gpui::Role::Button)
                                    .aria_label("Comment")
                                    .child("Comment"),
                            ),
                    ),
            )
    }

    /// Right-hand file tree with git status badges and a filter field.
    fn file_tree(&self, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let filter = self.tree_filter.read(cx).text().trim().to_lowercase();
        let files: Vec<&FileDiff> = self
            .diff
            .iter()
            .filter(|file| filter.is_empty() || file.path.to_lowercase().contains(&filter))
            .collect();
        let rows = self.tree_rows("", &files, 0, cx);
        div()
            .flex_none()
            .w(px(self.tree_width()))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .pt(px(8.0))
            .pl(px(8.0))
            .pr(px(16.0))
            .child(
                div().pb(px(4.0)).child(
                    div()
                        .h(px(28.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .rounded(px(9999.0))
                        .bg(theme.field_surface)
                        .border(px(1.0))
                        .border_color(theme.field_border)
                        .child(icon("search", theme.icon_muted.into()).size(px(14.0)))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(self.tree_filter.clone()),
                        ),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .pl(px(8.0))
                    .pr(px(8.0))
                    .child(rows),
            )
    }

    /// Scope dropdown of the review tab: all changes or a single commit.
    pub(super) fn scope_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        let scope = self
            .review_tab
            .as_ref()
            .map(|tab| tab.scope.clone())
            .unwrap_or(ReviewScope::AllChanges);
        let commits = self
            .review_tab
            .as_ref()
            .map(|tab| tab.commits.clone())
            .unwrap_or_default();
        let (additions, deletions) = self
            .detail
            .as_ref()
            .map(|detail| (detail.additions, detail.deletions))
            .unwrap_or_default();
        let mut menu = div()
            .id("pr-scope-menu")
            .p(px(4.0))
            .w(px(300.0))
            .rounded(px(20.0))
            .bg(theme.menu_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .text_size(px(13.0))
            .text_color(theme.text);
        let all_selected = matches!(scope, ReviewScope::AllChanges);
        menu = menu.child(
            div()
                .id("pr-scope-all")
                .h(px(28.5))
                .px(px(8.0))
                .rounded(px(15.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .cursor_pointer()
                .hover(move |style| style.bg(theme.menu_hover))
                .role(gpui::Role::MenuItem)
                .aria_label("All PR changes")
                .on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |view, cx| {
                            view.set_review_scope(ReviewScope::AllChanges, cx)
                        });
                    }
                })
                .child(div().flex_1().child("All PR changes"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .text_size(px(12.0))
                        .child(
                            div()
                                .text_color(theme.additions_text)
                                .child(format!("+{additions}")),
                        )
                        .child(
                            div()
                                .text_color(theme.deletions_text)
                                .child(format!("-{deletions}")),
                        ),
                )
                .when(all_selected, |row| {
                    row.child(icon("check", theme.text.into()).size(px(14.0)))
                }),
        );
        menu = menu.child(
            div()
                .px(px(8.0))
                .py(px(6.0))
                .text_color(theme.text_muted)
                .child("Commits"),
        );
        let mut children = div()
            .id("pr-scope-commit-list")
            .max_h(px(300.0))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        for (sha, subject) in &commits {
            let sha = sha.clone();
            let sha_label = sha.clone();
            let select_view = view.clone();
            let selected = matches!(&scope, ReviewScope::Commit(current) if current == &sha);
            children = children.child(
                div()
                    .id(SharedString::from(format!("pr-scope-commit-{sha}")))
                    .flex_none()
                    .aria_label(format!("Commit {}: {subject}", &sha[..7.min(sha.len())]))
                    .h(px(28.5))
                    .px(px(8.0))
                    .rounded(px(15.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.menu_hover))
                    .role(gpui::Role::MenuItem)
                    .on_click(move |_, _, cx| {
                        select_view.update(cx, |view, cx| {
                            view.set_review_scope(ReviewScope::Commit(sha.clone()), cx)
                        });
                    })
                    .child(
                        div()
                            .font_family(UI_MONOSPACE_FONT_FAMILY)
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(sha_label.get(..7).unwrap_or(sha_label.as_str()).to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(subject.clone()),
                    )
                    .when(selected, |row| {
                        row.child(icon("check", theme.text.into()).size(px(14.0)))
                    }),
            );
        }
        menu = menu.child(children);
        menu
    }

    /// `Review options` menu of the diff toolbar.
    pub(super) fn review_options_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        let entries = [
            (
                if self.wrap {
                    "Disable word wrap"
                } else {
                    "Enable word wrap"
                },
                "wrap",
                self.wrap,
            ),
            (
                if self.rich {
                    "Disable Markdown preview"
                } else {
                    "Enable Markdown preview"
                },
                "rich",
                self.rich,
            ),
            (
                if self.words {
                    "Disable word diffs"
                } else {
                    "Enable word diffs"
                },
                "words",
                self.words,
            ),
        ];
        let mut menu = div()
            .id("pr-review-options-menu")
            .p(px(4.0))
            .w(px(220.0))
            .rounded(px(20.0))
            .bg(theme.menu_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .text_size(px(13.0))
            .text_color(theme.text);
        for (label, action, checked) in entries {
            if action == "rich"
                && !self
                    .diff
                    .iter()
                    .any(|file| file.path.ends_with(".md") && file.status != 'D')
            {
                continue;
            }
            let view = view.clone();
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("pr-review-option-{action}")))
                    .h(px(28.5))
                    .px(px(8.0))
                    .rounded(px(15.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.menu_hover))
                    .role(gpui::Role::MenuItem)
                    .aria_label(label)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| match action {
                            "wrap" => view.toggle_wrap(cx),
                            "rich" => view.toggle_rich(cx),
                            "words" => view.toggle_words(cx),
                            _ => {}
                        });
                    })
                    .child(div().flex_1().child(label))
                    .when(checked, |row| {
                        row.child(icon("check", theme.text.into()).size(px(14.0)))
                    }),
            );
        }
        menu
    }

    /// The `Status` submenu of the detail header.
    pub(super) fn status_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        let current = self
            .detail
            .as_ref()
            .map(|detail| detail.summary.status)
            .unwrap_or(crate::pull_requests::PullRequestStatus::Open);
        let mut menu = div()
            .id("pr-status-menu")
            .p(px(4.0))
            .w(px(200.0))
            .rounded(px(20.0))
            .bg(theme.menu_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .text_size(px(13.0))
            .text_color(theme.text);
        for status in crate::pull_requests::PullRequestStatus::selectable() {
            let disabled = status == current
                || current == crate::pull_requests::PullRequestStatus::Merged
                || self.mutation_pending;
            let view = view.clone();
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("pr-status-{}", status.label())))
                    .h(px(28.5))
                    .px(px(8.0))
                    .rounded(px(15.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(disabled, |row| row.text_color(theme.text_muted))
                    .when(!disabled, |row| {
                        row.cursor_pointer()
                            .hover(move |style| style.bg(theme.menu_hover))
                    })
                    .role(gpui::Role::MenuItem)
                    .aria_label(SharedString::from(status.label()))
                    .when(!disabled, |row| {
                        row.on_click(move |_, _, cx| {
                            view.update(cx, |view, cx| view.set_status(status, cx));
                        })
                    })
                    .child(div().flex_1().child(status.label()))
                    .when(disabled, |row| {
                        row.child(icon("check", theme.text_muted.into()).size(px(14.0)))
                    }),
            );
        }
        menu
    }

    /// `Description actions` menu.
    pub(super) fn description_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        let entries = ["Edit description", "Draft description in chat"];
        let mut menu = div()
            .id("pr-description-menu")
            .p(px(4.0))
            .w(px(200.0))
            .rounded(px(20.0))
            .bg(theme.menu_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .text_size(px(13.0))
            .text_color(theme.text);
        for entry in entries {
            let view = view.clone();
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("pr-description-{entry}")))
                    .h(px(28.5))
                    .px(px(8.0))
                    .rounded(px(15.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.menu_hover))
                    .role(gpui::Role::MenuItem)
                    .aria_label(entry)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| {
                            if entry.starts_with("Edit") {
                                view.begin_description_edit(cx);
                            } else {
                                view.description_menu = false;
                                if let Some(summary) = &view.selected {
                                    cx.emit(super::OpenChatForPullRequest { prompt: format!("Draft a pull request description for {}. Read the changes and summarize their behavior and validation. Return the draft for me to review.", summary.url) });
                                }
                            }
                        });
                    })
                    .child(entry),
            );
        }
        menu
    }
}

/// First old-file line of a `@@ -a,b +c,d @@` header.
fn hunk_new_start(header: &str) -> Option<u32> {
    let rest = header.split("@@").nth(1)?;
    let old = rest.split_whitespace().nth(1)?.trim_start_matches('+');
    old.split(',').next()?.parse().ok()
}

/// Last old-file line of a hunk header.
fn hunk_new_end(header: &str) -> Option<u32> {
    let rest = header.split("@@").nth(1)?;
    let old = rest.split_whitespace().nth(1)?.trim_start_matches('+');
    let mut parts = old.split(',');
    let start: u32 = parts.next()?.parse().ok()?;
    match parts.next().and_then(|count| count.parse::<u32>().ok()) {
        Some(count) => Some(start + count.saturating_sub(1)),
        None => Some(start),
    }
}

pub(super) fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

impl PullRequestsView {
    /// One level of the file tree: folders first, then the files directly under
    /// `prefix`. Clicking a folder row collapses or expands its subtree.
    ///
    /// A folder whose only child is another folder renders as a single joined
    /// row (`src/agent/codex`), which is how the reference compresses paths.
    fn tree_rows(
        &self,
        prefix: &str,
        files: &[&FileDiff],
        depth: usize,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let theme = self.theme();
        let mut rows = div().flex().flex_col();
        let mut folders: Vec<String> = Vec::new();
        for file in files {
            let remainder = file.path.strip_prefix(prefix).unwrap_or(&file.path);
            if remainder.is_empty() {
                continue;
            }
            if let Some((folder, _)) = remainder.split_once('/') {
                let folder_path = format!("{prefix}{folder}");
                if !folders.contains(&folder_path) {
                    folders.push(folder_path);
                }
            }
        }
        for folder_path in folders {
            let folder_prefix = format!("{folder_path}/");
            let collapsed = self.collapsed_folders.contains(&folder_path);
            let view = cx.entity();
            // Join single-child folder chains into one row and descend past the
            // joined levels, so `src/agent/codex` is one row rather than three.
            let mut label = folder_path
                .rsplit('/')
                .next()
                .unwrap_or(&folder_path)
                .to_string();
            let mut deepest = folder_prefix.clone();
            loop {
                let nested: Vec<&FileDiff> = files
                    .iter()
                    .copied()
                    .filter(|file| file.path.starts_with(&deepest))
                    .collect();
                let (direct, subfolders) = tree_children(&deepest, &nested);
                if direct || subfolders.len() != 1 {
                    break;
                }
                let next = subfolders[0].clone();
                label.push('/');
                label.push_str(next.rsplit('/').next().unwrap_or(&next));
                deepest = format!("{next}/");
            }
            let children: Vec<&FileDiff> = files
                .iter()
                .copied()
                .filter(|file| file.path.starts_with(&deepest))
                .collect();
            let toggle = folder_path.clone();
            let name = label.clone();
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("pr-tree-folder-{folder_path}")))
                    .h(px(TREE_ROW_HEIGHT))
                    .pl(px(6.0 + depth as f32 * TREE_INDENT))
                    .pr(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(TREE_ROW_RADIUS))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.control_hover))
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(name.clone()))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.toggle_folder(toggle.clone(), cx));
                    })
                    .child(
                        icon("section-chevron", theme.text_muted.into())
                            .size(px(16.0))
                            .when(collapsed, |chevron| {
                                chevron.with_transformation(gpui::Transformation::rotate(
                                    gpui::radians(-std::f32::consts::FRAC_PI_2),
                                ))
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(13.0))
                            .text_color(theme.text)
                            .child(name),
                    )
                    .child(
                        // The reference marks a folder that contains changes
                        // with a 6px dot in the status colour at half opacity.
                        div().w(px(20.0)).flex().justify_center().child(
                            div().size(px(6.0)).rounded(px(9999.0)).bg({
                                let (status, _) = tree_status(&children);
                                let base = if status == 'A' {
                                    theme.status_added
                                } else {
                                    theme.status_modified
                                };
                                gpui::Rgba { a: 0.5, ..base }
                            }),
                        ),
                    ),
            );
            if !collapsed && !children.is_empty() {
                rows = rows.child(self.tree_rows(&deepest, &children, depth + 1, cx));
            }
        }
        for file in files {
            let remainder = file.path.strip_prefix(prefix).unwrap_or(&file.path);
            if remainder.is_empty() || remainder.contains('/') {
                continue;
            }
            let selected = self.selected_file.as_deref() == Some(file.path.as_str());
            let path = file.path.clone();
            let view = cx.entity();
            let status = file.status;
            let name = file_name(&file.path);
            let full_path = file.path.clone();
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("pr-tree-{}", file.path)))
                    .h(px(TREE_ROW_HEIGHT))
                    .pl(px(6.0 + depth as f32 * TREE_INDENT))
                    .pr(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(TREE_ROW_RADIUS))
                    .cursor_pointer()
                    .when(selected, |row| row.bg(theme.control))
                    .hover(move |style| style.bg(theme.control_hover))
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(full_path.clone()))
                    .on_click({
                        let path = path.clone();
                        move |_, _, cx| {
                            view.update(cx, |view, cx| view.scroll_to_file(path.clone(), cx));
                        }
                    })
                    .child(icon("markdown-file-rust", theme.status_modified.into()).size(px(16.0)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(13.0))
                            .text_color(theme.text)
                            .child(name),
                    )
                    .child(self.tree_status_badge(status, cx)),
            );
        }
        rows
    }

    /// The reference draws the row status as an 18px rounded square outline
    /// with a centred dot (`M`) or plus (`A`).
    fn tree_status_badge(&self, status: char, _cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let (color, added) = if status == 'A' {
            (theme.status_added, true)
        } else {
            (theme.status_modified, false)
        };
        div().w(px(20.0)).flex().justify_center().child(
            div()
                .size(px(18.0))
                .rounded(px(6.0))
                .border(px(1.5))
                .border_color(color)
                .flex()
                .items_center()
                .justify_center()
                .child(if added {
                    div()
                        .size(px(9.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon("add", color.into()).size(px(9.0)))
                        .into_any_element()
                } else {
                    div()
                        .size(px(6.0))
                        .rounded(px(9999.0))
                        .bg(color)
                        .into_any_element()
                }),
        )
    }
}

/// Align each contiguous deletion/addition block without pairing across context.
pub(super) fn split_pairs(
    lines: &[crate::git_review::Line],
) -> Vec<(Option<usize>, Option<usize>)> {
    let mut rows = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind == LineKind::Context {
            rows.push((Some(i), Some(i)));
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && lines[i].kind == LineKind::Deleted {
            i += 1;
        }
        let added = i;
        while i < lines.len() && lines[i].kind == LineKind::Added {
            i += 1;
        }
        for offset in 0..(added - start).max(i - added) {
            rows.push((
                (start + offset < added).then_some(start + offset),
                (added + offset < i).then_some(added + offset),
            ));
        }
    }
    rows
}

pub(super) fn changed_span(a: &str, b: &str) -> std::ops::Range<usize> {
    let prefix: usize = a
        .chars()
        .zip(b.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let suffix: usize = a[prefix..]
        .chars()
        .rev()
        .zip(b[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    prefix..a.len() - suffix
}
