//! `Code` and review tabs: toolbar, file headers, hunks, and the file tree.

mod file_icons;
mod syntax;
mod viewport;
mod words;
pub(super) use viewport::DiffViewport;

use gpui::{Div, SharedString, div, prelude::*, px};

use super::{
    DetailTab, PullRequestsView, ReviewScope,
    theme::{DETAIL_MIN_WIDTH, MENU_SUBMENU_WIDTH},
};
use crate::components::icons::icon;
use crate::git_review::{FileDiff, LineKind};
use crate::theme::UI_MONOSPACE_FONT_FAMILY;

/// The reference number column: `ps` 2ch, a 3ch-minimum cell that measures
/// 28.9px, `pe` 1ch, and a 2px surface border, where 1ch is 7.22461px of 12px
/// Menlo.
const GUTTER_WIDTH: f32 = 52.5625;
const CELL_PADDING: f32 = 7.22461;
/// The diff line box, `calc(12px * 1.8)`. Rows take it from their text rather
/// than an authored height, which layout would snap to 21.5px and drift.
pub(super) const LINE_HEIGHT: f32 = 21.6;
/// The sticky file header block: the 32px header and 2px below it.
const STICKY_HEADER_HEIGHT: f32 = 34.0;
/// A hunk separator row (`[data-separator="line-info"]`).
const SEPARATOR_HEIGHT: f32 = 32.0;
/// The deleted-line bar: `linear-gradient(0deg, <row> 50%, <red> 50%)` tiled
/// every 1.96364px, a red stripe over each tile's top half.
const DELETED_BAR_TILE: f32 = 1.96364;
/// The reference file tree panel: x 1080 → 1440 at a 1440px window, i.e. a
/// 360px column that reflows the diff into the remaining 286px.
const TREE_WIDTH: f32 = 360.0;
/// File-tree row height and radius measured from the reference rows.
const TREE_ROW_HEIGHT: f32 = 29.0;
const TREE_ROW_RADIUS: f32 = 6.0;
/// Each tree level indents its row by this much (`spacing` 7.5px plus the
/// 5px gap: icons at 1095 → 1107.5).
const TREE_INDENT: f32 = 12.5;

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
            syntax::code_runs(text, language, self.mode)
        })
    }

    pub(super) fn code_text(&self, text: &str, language: Option<&'static str>) -> gpui::StyledText {
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
        let deleted = line.kind == LineKind::Deleted;
        let partner = &file.hunks[hunk].lines[pair].text;
        let (old_spans, new_spans) = if deleted {
            words::changed_spans(&line.text, partner)
        } else {
            words::changed_spans(partner, &line.text)
        };
        let spans = if deleted { old_spans } else { new_spans };
        if spans.is_empty() {
            return self.code_text(&line.text, language);
        }
        let theme = self.theme();
        let color = if deleted {
            theme.diff_deleted_word
        } else {
            theme.diff_added_word
        };
        let runs = self.diff_viewport.syntax_runs(
            &line.text,
            language,
            Some((partner.clone(), deleted)),
            || {
                let mut cuts: Vec<usize> = spans
                    .iter()
                    .flat_map(|span| [span.start, span.end])
                    .collect();
                let mut runs = Vec::new();
                let mut start = 0;
                for run in syntax::code_runs(&line.text, language, self.mode) {
                    let end = start + run.len;
                    cuts.retain(|cut| *cut > start);
                    let mut pieces = vec![start];
                    pieces.extend(cuts.iter().copied().filter(|cut| *cut < end));
                    pieces.push(end);
                    for piece in pieces.windows(2) {
                        let mut run = run.clone();
                        run.len = piece[1] - piece[0];
                        if spans.iter().any(|span| span.contains(&piece[0])) {
                            run.background_color = Some(color.into());
                        }
                        runs.push(run);
                    }
                    start = end;
                }
                runs
            },
        );
        gpui::StyledText::new(line.text.clone()).with_runs(runs.as_ref().clone())
    }

    /// Language name for the syntax highlighter, derived from the file suffix.
    pub(super) fn language_for(path: &str) -> Option<&'static str> {
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

    /// Lays out the two panes for this frame, then the diff code column: pane
    /// width minus the file tree, the 53px gutter, and the 14px padding on
    /// each side of the code.
    pub(super) fn measure_code_width(&mut self, window: &gpui::Window) {
        let viewport = f32::from(window.viewport_size().width);
        // Full screen still leaves the sidebar: the detail covers the main
        // area, whose width the host reports.
        let page = if self.page_width > 0.0 {
            self.page_width
        } else {
            viewport - crate::components::sidebar::SIDEBAR_WIDTH
        };
        let height = if self.page_height > 0.0 {
            self.page_height
        } else {
            f32::from(window.viewport_size().height)
        };
        let detail = super::detail_panel_width(page, height, self.detail_ratio);
        let compact = page - detail < DETAIL_MIN_WIDTH;
        self.compact_layout = compact;
        self.list_width = if compact {
            if self.selected.is_none() { page } else { 0.0 }
        } else {
            page - detail
        };
        self.pane_width = if self.fullscreen || (compact && self.selected.is_some()) {
            page
        } else {
            page - self.list_width
        };
        // The file tree floats over the diff, which keeps its width.
        let tree = 0.0;
        let gutter = if self.classic_scrollbars {
            super::theme::SCROLLBAR_GUTTER
        } else {
            0.0
        };
        let cell = self.pane_width - tree - gutter;
        let code = |cell: f32| (cell - GUTTER_WIDTH - CELL_PADDING * 2.0).max(40.0);
        // A split half gives 1px to the 2px gap between the halves.
        self.set_code_width(code(cell), code(cell / 2.0 - 1.0));
    }

    fn tree_width(&self) -> f32 {
        TREE_WIDTH.min(self.pane_width)
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
            .relative()
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
            )
            .children(self.sticky_file_header(cx))
            .children({
                let list = &self.diff_viewport.scroll;
                self.scrollbar_thumb_for(
                    f32::from(list.viewport_bounds().size.height),
                    f32::from(list.max_offset_for_scrollbar().y),
                    -f32::from(list.scroll_px_offset_for_scrollbar().y),
                    cx,
                )
            });
        let mut body = div()
            .relative()
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
            .child(if self.review_tab.is_some() {
                self.review_header(cx).into_any_element()
            } else {
                self.diff_toolbar(cx).into_any_element()
            })
            .child(body);
        let _ = theme;
        surface
    }

    /// The Code tab toolbar (`h-toolbar-pane px-2`, 40px over a rule): the
    /// `head › base` breadcrumb in 13px tertiary type, then `Review options`,
    /// `Collapse all diffs` and the view toggle as 32px round buttons and
    /// `Show file tree` as a 28px tertiary one.
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
        let all_collapsed = self
            .diff
            .iter()
            .all(|file| self.collapsed_files.contains(&file.path));
        div()
            .flex_none()
            .h(px(41.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .overflow_hidden()
            .border_b(px(1.0))
            .border_color(theme.border)
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .text_color(theme.text_muted)
                    .child(div().min_w(px(0.0)).truncate().child(head))
                    .child(
                        icon("pr-chevron-right", theme.text_muted.into())
                            .flex_none()
                            .size(px(14.0)),
                    )
                    .child(div().min_w(px(0.0)).truncate().child(base)),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        self.diff_toolbar_button(
                            "pr-review-options",
                            "Review options",
                            icon("more-horizontal", theme.text.into())
                                .size(px(16.0))
                                .into_any_element(),
                            cx,
                        ),
                    )
                    .child(
                        self.diff_toolbar_button(
                            "pr-collapse-all",
                            if all_collapsed {
                                "Expand all diffs"
                            } else {
                                "Collapse all diffs"
                            },
                            icon("review-collapse", theme.text.into())
                                .size(px(16.0))
                                .into_any_element(),
                            cx,
                        ),
                    )
                    .child(self.diff_toolbar_button(
                        "pr-split-toggle",
                        match self.diff_layout {
                            super::DiffLayout::Unified => "Switch to split diff",
                            super::DiffLayout::Split => "Switch to Auto diff",
                            super::DiffLayout::Auto => "Auto diff: switch to unified diff",
                        },
                        view_mode_glyph(self.diff_layout, theme).into_any_element(),
                        cx,
                    ))
                    .child(
                        self.diff_toolbar_button(
                            "pr-file-tree",
                            if self.file_tree_open {
                                "Hide file tree"
                            } else {
                                "Show file tree"
                            },
                            icon("pr-file-tree", theme.text_muted.into())
                                .size(px(16.0))
                                .into_any_element(),
                            cx,
                        ),
                    ),
            )
    }

    /// The review tab's header (`py-2 ps-2 pe-2`, 48px): the scope pill with
    /// its counts and chevron, and at the far end the `Review controls group`
    /// pill of 28px round buttons. Both pills sit on the composer surface at
    /// 96% with a 1px shadow ring.
    fn review_header(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let (additions, deletions) = self.diff.iter().fold((0, 0), |(a, d), file| {
            (a + file.additions, d + file.deletions)
        });
        let pill = |element: gpui::Stateful<Div>| {
            element
                .rounded(px(9999.0))
                .bg(gpui::Rgba {
                    a: 0.96,
                    ..theme.composer_surface
                })
                .shadow(vec![
                    gpui::BoxShadow::new(px(0.0), px(0.0), gpui::rgba(0x0000000d).into())
                        .spread_radius(px(1.0)),
                    gpui::BoxShadow::new(px(0.0), px(4.0), gpui::rgba(0x0000000d).into())
                        .blur_radius(px(16.0)),
                ])
        };
        let all_collapsed = self
            .diff
            .iter()
            .all(|file| self.collapsed_files.contains(&file.path));
        let round = |button: gpui::Stateful<Div>| button.size(px(28.0)).rounded(px(14.0));
        div()
            .flex_none()
            .h(px(48.0))
            .p(px(8.0))
            .flex()
            .items_center()
            .text_size(px(13.0))
            .child(
                pill(
                    div()
                        .id("pr-review-tab-scope")
                        .relative()
                        .child(self.control_anchor("pr-review-tab-scope"))
                        .h(px(32.0))
                        .pl(px(12.0))
                        .pr(px(6.0))
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .cursor_pointer()
                        .role(gpui::Role::Button)
                        .aria_label("Pull request changes scope")
                        .on_click(move |_, _, cx| {
                            view.update(cx, |view, cx| view.toggle_scope_menu(cx));
                        }),
                )
                .line_height(px(20.0))
                .text_color(theme.text)
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .child(self.summary_scope_label()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .mr(px(4.0))
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .line_height(px(13.0))
                                .font_features(super::list::stats_font_features())
                                .child(div().text_color(theme.additions_text).child(format!(
                                    "+{}",
                                    crate::pull_requests::format_count(additions as u64)
                                )))
                                .child(div().text_color(theme.deletions_text).child(format!(
                                    "-{}",
                                    crate::pull_requests::format_count(deletions as u64)
                                ))),
                        ),
                )
                .child(
                    icon("pr-tree-chevron", theme.text.into())
                        .flex_none()
                        .size(px(12.0)),
                ),
            )
            .child(
                div().ml_auto().flex_none().child(
                    pill(div().id("pr-review-controls"))
                        .role(gpui::Role::Group)
                        .aria_label("Review controls group")
                        .p(px(2.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_color(theme.text)
                        .child(round(
                            self.diff_toolbar_button(
                                "pr-review-options",
                                "Review options",
                                icon("more-horizontal", theme.text.into())
                                    .size(px(16.0))
                                    .into_any_element(),
                                cx,
                            ),
                        ))
                        .child(round(
                            self.diff_toolbar_button(
                                "pr-collapse-all",
                                if all_collapsed {
                                    "Expand all diffs"
                                } else {
                                    "Collapse all diffs"
                                },
                                icon("review-collapse", theme.text.into())
                                    .size(px(16.0))
                                    .into_any_element(),
                                cx,
                            ),
                        ))
                        .child(round(self.diff_toolbar_button(
                            "pr-split-toggle",
                            match self.diff_layout {
                                super::DiffLayout::Unified => "Switch to split diff",
                                super::DiffLayout::Split => "Switch to Auto diff",
                                super::DiffLayout::Auto => "Auto diff: switch to unified diff",
                            },
                            view_mode_glyph(self.diff_layout, theme).into_any_element(),
                            cx,
                        )))
                        .child(round(
                            self.diff_toolbar_button(
                                "pr-file-tree",
                                if self.file_tree_open {
                                    "Hide file tree"
                                } else {
                                    "Show file tree"
                                },
                                icon("pr-file-tree", theme.text.into())
                                    .size(px(16.0))
                                    .into_any_element(),
                                cx,
                            ),
                        )),
                ),
            )
    }

    fn diff_toolbar_button(
        &self,
        id: &'static str,
        label: &'static str,
        glyph: gpui::AnyElement,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        // An open `Review options` menu leaves its trigger unfilled.
        let open = id == "pr-file-tree" && self.file_tree_open;
        let (size, radius) = if id == "pr-file-tree" {
            (28.0, 12.5)
        } else {
            (32.0, 16.0)
        };
        div()
            .id(id)
            .relative()
            .child(self.control_anchor(id))
            .flex_none()
            .size(px(size))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius))
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
            .child(glyph)
    }

    /// The header of the file whose rows fill the top of the diff, pinned
    /// there (`sticky top-0`) and pushed up by the next file's header.
    fn sticky_file_header(&self, cx: &mut gpui::Context<Self>) -> Option<Div> {
        let list = &self.diff_viewport.scroll;
        let top = list.logical_scroll_top();
        let row = self.diff_viewport.rows.get(top.item_ix)?;
        if row.kind == viewport::RowKind::Header && top.offset_in_item <= px(0.0) {
            return None;
        }
        let file = self.diff.get(row.file)?;
        let viewport_top = list.viewport_bounds().origin.y;
        let push = self
            .diff
            .get(row.file + 1)
            .and_then(|_| self.diff_viewport.file_row(row.file + 1))
            .and_then(|next| list.bounds_for_item(next))
            .map_or(0.0, |next| {
                (f32::from(next.origin.y - viewport_top) - STICKY_HEADER_HEIGHT).min(0.0)
            });
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(push))
                .child(self.file_header(row.file, file, true, cx)),
        )
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

    /// A file's header row (`group/diff-header`, 32px, `py-1 ps-3 pe-2`, 14/21):
    /// the file-type glyph, the path with its directory in tertiary type,
    /// the `+x -y` counts, and the hover-only `Copy path`, `Toggle file diff`
    /// and `Open file` buttons right after them.
    /// `sticky` renders the copy pinned over the top of the diff, with its
    /// own element ids and without the gap above the file.
    pub(super) fn file_header(
        &self,
        index: usize,
        file: &FileDiff,
        sticky: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let prefix = if sticky { "pr-sticky" } else { "pr" };
        let theme = self.theme();
        let collapsed = self.collapsed_files.contains(&file.path);
        let path = file.path.clone();
        let view = cx.entity();
        let (asset, glyph_color) = file_icons::file_icon(&file.path, self.mode);
        let split = file.path.rfind('/').map_or(0, |slash| slash + 1);
        let label = gpui::StyledText::new(file.path.clone()).with_highlights([(
            0..split,
            gpui::HighlightStyle {
                color: Some(theme.text_muted.into()),
                ..Default::default()
            },
        )]);
        // The sticky header block is 34px (the 32px header and 2px below it),
        // and every file after the first follows the previous file's `pb-0.5`.
        div()
            .flex_none()
            .flex()
            .flex_col()
            .when(index > 0 && !sticky, |block| block.pt(px(2.0)))
            .pb(px(2.0))
            .bg(theme.diff_header_surface)
            .child(
                div()
                    .id(SharedString::from(format!("{prefix}-file-{}", file.path)))
                    .group("pr-file-header")
                    .h(px(32.0))
                    .pl(px(12.0))
                    .pr(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .bg(theme.diff_header_surface)
                    // `hover:bg-primary-ghost-hover`
                    .hover(move |style| style.bg(theme.control_hover))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "{prefix}-file-name-{}",
                                        file.path
                                    )))
                                    .min_w(px(0.0))
                                    .pl(px(4.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .cursor_pointer()
                                    .role(gpui::Role::Button)
                                    .aria_label(SharedString::from(file.path.clone()))
                                    .on_click({
                                        let view = view.clone();
                                        let path = path.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |view, cx| {
                                                view.toggle_file(path.clone(), cx)
                                            });
                                        }
                                    })
                                    .child(
                                        gpui::svg()
                                            .path(format!("icons/{asset}.svg"))
                                            .flex_none()
                                            .size(px(16.0))
                                            .text_color(glyph_color),
                                    )
                                    .child(
                                        div()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis_start()
                                            .text_color(theme.text)
                                            .child(label),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .mx(px(4.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .line_height(px(14.0))
                                    .font_features(super::list::stats_font_features())
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
                            )
                            .child(self.file_header_actions(prefix, &path, collapsed, cx)),
                    ),
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
        prefix: &str,
        path: &str,
        collapsed: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let mut row = div().flex_none().flex().items_center().gap(px(2.0));
        // `Copy path` is 24px, the others 20px; all show on header hover with
        // 14px (`icon-2xs`) tertiary glyphs.
        for (id, glyph, label, size) in [
            ("pr-copy-path", "pr-copy-path", "Copy path", 24.0),
            ("pr-toggle-file", "pr-toggle-file", "Toggle file diff", 20.0),
            ("pr-open-file", "pr-open-file", "Open file", 20.0),
        ] {
            let path = path.to_string();
            let view = view.clone();
            let glyph = icon(glyph, theme.text_muted.into()).size(px(14.0));
            let glyph = if id == "pr-toggle-file" && !collapsed {
                glyph.with_transformation(gpui::Transformation::rotate(gpui::radians(
                    std::f32::consts::FRAC_PI_2,
                )))
            } else {
                glyph
            };
            row = row.child(
                div()
                    .id(SharedString::from(if prefix == "pr" {
                        format!("{id}-{path}")
                    } else {
                        format!("{prefix}-{id}-{path}")
                    }))
                    .flex_none()
                    .size(px(size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.0))
                    .cursor_pointer()
                    .opacity(0.0)
                    .group_hover("pr-file-header", |style| style.opacity(1.0))
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
                    .child(glyph),
            );
        }
        row
    }

    /// A hunk separator: a 32px row whose `6px 8px 8px 6px` box, inset 2px,
    /// reads `N unmodified lines` in 12px system type. The Code tab cannot
    /// load the file there, so its row is static; a review tab puts 53px
    /// expand buttons over the gutter: one above the first hunk, and between
    /// hunks a top half (below the previous hunk) and a bottom half (above
    /// this one).
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
        let label = if gap == 1 {
            "1 unmodified line".to_owned()
        } else {
            format!("{gap} unmodified lines")
        };
        let review = self.review_tab.is_some();
        if self.file_splits(file) && !review {
            return self.split_hunk_expander(key, label);
        }
        let button = |id: String, from_start: bool, height: f32| {
            let view = cx.entity();
            let glyph = icon("pr-diff-expand", theme.diff_gutter_text.into()).size(px(16.0));
            div()
                .id(SharedString::from(id))
                .w(px(53.0))
                .h(px(height))
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .bg(theme.diff_expander_surface)
                .cursor_pointer()
                .role(gpui::Role::Button)
                .aria_label(if from_start {
                    "Expand lines below"
                } else {
                    "Expand lines above"
                })
                .on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| {
                        view.expand_gap(file_index, hunk_index, from_start, cx)
                    });
                })
                .child(if from_start {
                    glyph
                } else {
                    glyph.with_transformation(gpui::Transformation::rotate(gpui::radians(
                        std::f32::consts::PI,
                    )))
                })
        };
        let buttons = review.then(|| {
            // The first gap only reveals upwards, the one after the last hunk
            // only downwards, and those between both ways.
            if hunk_index == 0 || hunk_index == file.hunks.len() {
                let below = hunk_index > 0;
                div()
                    .flex_none()
                    .rounded_l(px(8.0))
                    .overflow_hidden()
                    .child(button(
                        format!("pr-expand-{}-{key}", if below { "below" } else { "above" }),
                        below,
                        SEPARATOR_HEIGHT,
                    ))
            } else {
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .rounded_l(px(8.0))
                    .overflow_hidden()
                    .child(button(
                        format!("pr-expand-below-{key}"),
                        true,
                        SEPARATOR_HEIGHT / 2.0,
                    ))
                    .child(button(
                        format!("pr-expand-above-{key}"),
                        false,
                        SEPARATOR_HEIGHT / 2.0,
                    ))
            }
        });
        div()
            .id(SharedString::from(format!("pr-expander-{key}")))
            .h(px(SEPARATOR_HEIGHT))
            .px(px(2.0))
            .flex()
            .bg(theme.surface)
            .aria_label(SharedString::from(label.clone()))
            .children(buttons)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .px(px(7.6242))
                    .when(!review, |content| content.rounded_l(px(6.0)))
                    .rounded_r(px(8.0))
                    .bg(theme.diff_expander_surface)
                    .text_size(px(12.0))
                    .line_height(px(LINE_HEIGHT))
                    .text_color(theme.diff_gutter_text)
                    .child(div().truncate().child(label)),
            )
    }

    /// A file line a review tab revealed, drawn like a context row.
    fn context_line(&self, file: &FileDiff, number: u32, text: &str) -> Div {
        let theme = self.theme();
        let split = self.file_splits(file);
        div()
            .min_w(px(0.0))
            .flex()
            .items_stretch()
            .bg(theme.surface)
            .child(self.gutter_cell(LineKind::Context, Some(number)))
            .child(
                div()
                    .min_w(px(0.0))
                    .px(px(CELL_PADDING))
                    .font_family(UI_MONOSPACE_FONT_FAMILY)
                    .text_size(px(12.0))
                    .text_color(theme.diff_context_text)
                    .line_height(px(LINE_HEIGHT))
                    .when(self.wrap, |code| {
                        code.w(px(self.code_width(split) + CELL_PADDING * 2.0))
                            .whitespace_normal()
                    })
                    .when(!self.wrap, |code| code.whitespace_nowrap())
                    .child(self.code_text(text, Self::language_for(&file.path))),
            )
    }

    /// A hunk separator in split view: each half is a filled band (the old
    /// side from the edge, the new side ending 8px short with 8px corners)
    /// across the 2px gap, and only the old side reads `N unmodified lines`.
    fn split_hunk_expander(&self, key: String, label: String) -> gpui::Stateful<Div> {
        let theme = self.theme();
        div()
            .id(SharedString::from(format!("pr-expander-{key}")))
            .h(px(SEPARATOR_HEIGHT))
            .flex()
            .bg(theme.surface)
            .aria_label(SharedString::from(label.clone()))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .mr(px(1.0))
                    .flex()
                    .bg(theme.diff_expander_surface)
                    .child(div().flex_none().w(px(GUTTER_WIDTH)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .px(px(7.6242))
                            .text_size(px(12.0))
                            .line_height(px(LINE_HEIGHT))
                            .text_color(theme.diff_gutter_text)
                            .child(div().truncate().child(label)),
                    ),
            )
            .child(
                div().flex_1().min_w(px(0.0)).ml(px(1.0)).pr(px(8.0)).child(
                    div()
                        .size_full()
                        .rounded_r(px(8.0))
                        .bg(theme.diff_expander_surface),
                ),
            )
    }

    /// The number cell of a diff row: right-aligned in `pe` 1ch, beside a 2px
    /// surface border, with the 4px change bar of changed rows at its edge.
    fn gutter_cell(&self, kind: LineKind, number: Option<u32>) -> Div {
        let theme = self.theme();
        let (surface, color) = match kind {
            LineKind::Added => (theme.diff_added_gutter, theme.diff_added_text),
            LineKind::Deleted => (theme.diff_deleted_gutter, theme.diff_deleted_text),
            LineKind::Context => (theme.surface, theme.diff_gutter_text),
        };
        div()
            .relative()
            .w(px(GUTTER_WIDTH))
            .flex_none()
            .flex()
            .justify_end()
            .pr(px(CELL_PADDING))
            .border_r(px(2.0))
            .border_color(theme.surface)
            .bg(surface)
            .text_size(px(12.0))
            .line_height(px(LINE_HEIGHT))
            .font_family(UI_MONOSPACE_FONT_FAMILY)
            .text_color(color)
            .when(kind == LineKind::Added, |cell| {
                cell.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(4.0))
                        .bg(theme.diff_added_text),
                )
            })
            .when(kind == LineKind::Deleted, |cell| {
                cell.child(deleted_bar(
                    theme.diff_deleted_text,
                    theme.diff_deleted_surface,
                ))
            })
            .when_some(number, |cell, number| {
                cell.child(div().child(number.to_string()))
            })
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
        let row = div()
            .min_w(px(0.0))
            .flex()
            .items_stretch()
            .bg(match line.kind {
                LineKind::Added => theme.diff_added_surface,
                LineKind::Deleted => theme.diff_deleted_surface,
                LineKind::Context => theme.surface,
            })
            .child(self.gutter_cell(line.kind, anchor))
            .child(
                // Wrapping needs a definite width on a block that directly
                // holds the text; a flex child would be measured unconstrained.
                div()
                    .min_w(px(0.0))
                    .px(px(CELL_PADDING))
                    .font_family(UI_MONOSPACE_FONT_FAMILY)
                    .text_size(px(12.0))
                    .text_color(theme.diff_context_text)
                    .line_height(px(LINE_HEIGHT))
                    .when(self.wrap, |code| {
                        code.w(px(
                            self.code_width(self.file_splits(file)) + CELL_PADDING * 2.0
                        ))
                        .whitespace_normal()
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
                    // `[data-utility-button]`: a 1lh square in the text color
                    // over the number cell, 29px from the gutter's edge.
                    .absolute()
                    .left(px(28.96875))
                    .top(px(0.0))
                    .size(px(LINE_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .bg(theme.text)
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
                    .child(icon("pr-diff-plus", theme.surface.into()).size(px(16.0))),
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
    /// The file tree (`absolute inset-y-0`, 360px, `ps-2 pe-4`): a panel over
    /// the right of the diff, which keeps its width underneath, with the
    /// `Filter files…` field above the rows.
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
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(self.tree_width()))
            .flex()
            .flex_col()
            .bg(theme.surface)
            .pt(px(8.0))
            .pl(px(8.0))
            .pr(px(16.0))
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.tree_panel_ring.into())
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(3.0), gpui::rgba(0x0000000a).into())
                    .blur_radius(px(7.5)),
                gpui::BoxShadow::new(px(0.0), px(0.0), gpui::rgba(0x0000000d).into())
                    .blur_radius(px(20.0)),
            ])
            .child(
                div().flex_none().pb(px(4.0)).child(
                    div()
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .rounded(px(12.5))
                        .bg(theme.soft_alpha)
                        .border(px(1.0))
                        .border_color(theme.border)
                        .child(
                            icon("pr-search", theme.text_muted.into())
                                .flex_none()
                                .ml(px(8.0))
                                .size(px(16.0)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .pr(px(6.0))
                                .child(self.tree_filter.clone()),
                        ),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .px(px(4.0))
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(rows),
            )
    }

    /// A tree row (`[data-type=item]`): 29px, `px-[3px]`, 5px gaps, 6px radius,
    /// indented 12.5px per level, filled while selected or hovered. A hovered
    /// row reads in the default color and, after a rest, names itself in a
    /// tooltip below it.
    fn tree_row(
        &self,
        key: String,
        name: String,
        depth: usize,
        selected: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        let hover_key = key.clone();
        let tooltip = (self.tree_tooltip.as_deref() == Some(key.as_str()))
            .then(|| self.tree_tooltip_overlay(&key, name));
        div()
            .id(SharedString::from(key.clone()))
            .relative()
            .flex_none()
            .h(px(TREE_ROW_HEIGHT))
            .px(px(3.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .rounded(px(TREE_ROW_RADIUS))
            .cursor_pointer()
            .role(gpui::Role::TreeItem)
            .child(self.control_anchor(key))
            .children(tooltip)
            .when(depth > 0, |row| {
                row.child(div().flex_none().w(px(depth as f32 * TREE_INDENT - 5.0)))
            })
            .when(selected, |row| row.bg(theme.row_selected))
            // Hover fills 8% (`primary-ghost-hover`), selection 5%.
            .hover(move |style| style.bg(theme.control_hover).text_color(theme.text))
            .on_hover(move |hovered, _, cx| {
                let key = hover_key.clone();
                view.update(cx, |view, cx| view.set_tree_hover(key, *hovered, cx));
            })
    }

    /// The tree row tooltip (`role=tooltip`, `data-side=bottom`): 13/18 type in
    /// a 20px-radius popover 2px under the row's left edge.
    fn tree_tooltip_overlay(&self, key: &str, name: String) -> gpui::AnyElement {
        let theme = self.theme();
        let mut anchored = gpui::anchored().snap_to_window_with_margin(px(8.0));
        if let Some(bounds) = self.control_bounds.borrow().get(key) {
            anchored = anchored.position(bounds.bottom_left() + gpui::point(px(0.0), px(2.0)));
        }
        gpui::deferred(
            anchored.child(
                div()
                    .max_w(px(320.0))
                    .px(px(12.0))
                    .py(px(5.0))
                    .rounded(px(20.0))
                    .border(px(1.0))
                    .border_color(gpui::Rgba {
                        a: 0.05,
                        ..theme.text
                    })
                    .bg(theme.popover_surface)
                    .shadow(vec![
                        gpui::BoxShadow::new(px(0.0), px(8.0), gpui::rgba(0x0f172a33).into())
                            .blur_radius(px(18.0)),
                    ])
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                    .text_color(theme.text)
                    .child(name),
            ),
        )
        .with_priority(2)
        .into_any_element()
    }

    /// The review tab's scope dropdown (`menuWide`, 240px, opaque): `All PR
    /// changes` with the pull request's counts, a rule, then the `Commits`
    /// row whose flyout lists each commit. The chosen scope carries the
    /// leading check.
    pub(super) fn scope_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        let all_selected = self
            .review_tab
            .as_ref()
            .is_none_or(|tab| tab.scope == ReviewScope::AllChanges);
        let (additions, deletions) = self
            .detail
            .as_ref()
            .map(|detail| (detail.additions, detail.deletions))
            .unwrap_or_default();
        let all_view = view.clone();
        let leave_view = view.clone();
        let hover_view = view.clone();
        let click_view = view;
        Self::solid_menu_surface("pr-scope-menu", theme)
            .w(px(240.0))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Self::menu_row("pr-scope-all", theme, false, false)
                    .aria_label("All PR changes")
                    .aria_selected(all_selected)
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            leave_view
                                .update(cx, |view, cx| view.set_scope_commits_open(false, cx));
                        }
                    })
                    .on_click(move |_, _, cx| {
                        all_view.update(cx, |view, cx| {
                            view.set_review_scope(ReviewScope::AllChanges, cx)
                        });
                    })
                    .child(Self::menu_check_slot(all_selected, theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .child("All PR changes"),
                    )
                    .child(Self::menu_diff_stats(additions, deletions, theme)),
            )
            .child(Self::menu_rule(theme))
            .child(
                Self::menu_row("pr-scope-commits", theme, self.scope_commits_open, false)
                    .debug_selector(|| "pr-scope-commits".into())
                    .relative()
                    .child(self.control_anchor("pr-scope-commits"))
                    .aria_label("Commits")
                    .aria_expanded(self.scope_commits_open)
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            hover_view.update(cx, |view, cx| view.set_scope_commits_open(true, cx));
                        }
                    })
                    .on_click(move |_, _, cx| {
                        click_view.update(cx, |view, cx| view.set_scope_commits_open(true, cx));
                    })
                    .child(Self::menu_check_slot(false, theme))
                    .child(div().flex_1().min_w(px(0.0)).truncate().child("Commits"))
                    .child(Self::tinted_menu_icon("pr-chevron-right", theme.menu_icon)),
            )
    }

    /// The `Commits` flyout: each commit as `<short sha> <title>` in a
    /// `max-h-80` scroller, on the translucent submenu surface.
    pub(super) fn scope_commits_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        let (scope, commits) = self
            .review_tab
            .as_ref()
            .map(|tab| (tab.scope.clone(), tab.commits.clone()))
            .unwrap_or((ReviewScope::AllChanges, Vec::new()));
        let mut list = div()
            .id("pr-scope-commit-list")
            .max_h(px(320.0))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        for (sha, subject) in commits {
            let selected = matches!(&scope, ReviewScope::Commit(current) if *current == sha);
            let label = format!("{} {subject}", sha.get(..7).unwrap_or(&sha));
            let select_view = view.clone();
            list = list.child(
                Self::menu_row(
                    SharedString::from(format!("pr-scope-commit-{sha}")),
                    theme,
                    false,
                    false,
                )
                .aria_label(SharedString::from(label.clone()))
                .aria_selected(selected)
                .on_click(move |_, _, cx| {
                    select_view.update(cx, |view, cx| {
                        view.set_review_scope(ReviewScope::Commit(sha.clone()), cx)
                    });
                })
                .child(Self::menu_check_slot(selected, theme))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(label),
                ),
            );
        }
        Self::menu_surface("pr-scope-commits-menu", theme)
            .debug_selector(|| "pr-scope-commits-menu".into())
            .min_w(px(MENU_SUBMENU_WIDTH))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(list)
    }

    /// `Review options` menu of the diff toolbar.
    /// `Review options`: word wrap, then (below a rule) rich preview when a
    /// Markdown file can preview, and word diffs, each with its 16px glyph.
    pub(super) fn review_options_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let view = cx.entity();
        let previewable = self
            .diff
            .iter()
            .any(|file| file.path.ends_with(".md") && file.status != 'D');
        let entries = [
            (
                if self.wrap {
                    "Disable word wrap"
                } else {
                    "Enable word wrap"
                },
                "wrap",
                "pr-menu-wrap",
                true,
            ),
            (
                if self.rich {
                    "Disable rich preview"
                } else {
                    "Enable rich preview"
                },
                "rich",
                "pr-menu-rich-preview",
                previewable,
            ),
            (
                if self.words {
                    "Disable word diffs"
                } else {
                    "Enable word diffs"
                },
                "words",
                "pr-menu-word-diffs",
                true,
            ),
        ];
        // This menu is opaque (`rgb(45,45,45)` / white), unlike the inbox's.
        let mut menu = Self::solid_menu_surface("pr-review-options-menu", theme)
            .w(px(220.0))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (index, (label, action, glyph, shown)) in entries.into_iter().enumerate() {
            if !shown {
                continue;
            }
            let view = view.clone();
            menu = menu.child(
                Self::menu_row(
                    SharedString::from(format!("pr-review-option-{action}")),
                    theme,
                    false,
                    false,
                )
                .aria_label(label)
                .on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| match action {
                        "wrap" => view.toggle_wrap(cx),
                        "rich" => view.toggle_rich(cx),
                        "words" => view.toggle_words(cx),
                        _ => {}
                    });
                })
                .child(Self::menu_icon(glyph, theme))
                .child(div().flex_1().min_w(px(0.0)).truncate().child(label)),
            );
            if index == 0 {
                menu = menu.child(Self::menu_rule(theme));
            }
        }
        div().id("pr-review-options-popover").child(menu)
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
            let chevron = icon("pr-tree-chevron", TREE_CHEVRON.into()).size(px(12.0));
            let chevron = if collapsed {
                chevron.with_transformation(gpui::Transformation::rotate(gpui::radians(
                    -std::f32::consts::FRAC_PI_2,
                )))
            } else {
                chevron
            };
            rows = rows.child(
                self.tree_row(
                    format!("pr-tree-folder-{folder_path}"),
                    label.clone(),
                    depth,
                    false,
                    cx,
                )
                .aria_label(SharedString::from(label.clone()))
                .aria_expanded(!collapsed)
                .text_color(theme.text)
                .on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| view.toggle_folder(toggle.clone(), cx));
                })
                .child(
                    div()
                        .flex_none()
                        .w(px(16.0))
                        .flex()
                        .justify_center()
                        .child(chevron),
                )
                .child(tree_label(&label, false))
                // A folder that holds changes carries a 6px dot in the
                // modified color at half opacity (`[data-item-section=git]`).
                .child(
                    div()
                        .flex_none()
                        .w(px(20.0))
                        .flex()
                        .justify_center()
                        .opacity(0.5)
                        .child(icon("pr-tree-dot", theme.status_modified.into()).size(px(6.0))),
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
            let (asset, glyph_color) = file_icons::file_icon(&file.path, self.mode);
            let threads = self.detail.as_ref().map_or(0, |detail| {
                detail
                    .review_threads
                    .iter()
                    .filter(|thread| thread.path == file.path)
                    .count()
            });
            let (badge, badge_color) = match file.status {
                'A' => ("pr-tree-status-added", theme.status_added),
                'D' => ("pr-tree-status-modified", theme.diff_deleted_text),
                _ => ("pr-tree-status-modified", theme.status_modified),
            };
            rows = rows.child(
                self.tree_row(
                    format!("pr-tree-{}", file.path),
                    file_name(&file.path),
                    depth,
                    selected,
                    cx,
                )
                .aria_label(SharedString::from(file.path.clone()))
                .aria_selected(selected)
                // Unselected files read in tertiary type.
                .text_color(if selected {
                    theme.text
                } else {
                    theme.text_muted
                })
                .on_click({
                    let path = path.clone();
                    move |_, _, cx| {
                        view.update(cx, |view, cx| view.scroll_to_file(path.clone(), cx));
                    }
                })
                .child(
                    gpui::svg()
                        .path(format!("icons/{asset}.svg"))
                        .flex_none()
                        .size(px(16.0))
                        .text_color(glyph_color),
                )
                .child(tree_label(&file_name(&file.path), true))
                .when(threads > 0, |row| {
                    row.child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .text_color(TREE_CHEVRON)
                            .text_size(px(12.0))
                            .child(icon("pr-tree-comment", TREE_CHEVRON.into()).size(px(18.0)))
                            // `review-file-tree-comment-N` sets the count at
                            // x 22 of its 29px glyph.
                            .child(div().ml(px(4.0)).child(threads.to_string())),
                    )
                })
                .child(
                    div()
                        .flex_none()
                        .w(px(20.0))
                        .flex()
                        .justify_center()
                        .child(icon(badge, badge_color.into()).size(px(20.0))),
                ),
            );
        }
        rows
    }
}

/// The reference tree's chevron and comment glyph color (`#84848a`).
const TREE_CHEVRON: gpui::Rgba = gpui::Rgba {
    r: 0x84 as f32 / 255.0,
    g: 0x84 as f32 / 255.0,
    b: 0x8a as f32 / 255.0,
    a: 1.0,
};

/// A tree row's name: the stem truncates while a file keeps its extension.
fn tree_label(name: &str, file: bool) -> Div {
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if file && dot > 0 => (&name[..=dot], &name[dot + 1..]),
        _ => (name, ""),
    };
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .whitespace_nowrap()
        .child(div().min_w(px(0.0)).truncate().child(stem.to_owned()))
        .when(!extension.is_empty(), |label| {
            label.child(div().flex_none().child(extension.to_owned()))
        })
}

/// The view toggle's glyph: the current layout (`rectangle-view-unified`,
/// `…-split`, or the diagonal auto glyph), its frame in the text color and its
/// two panes in the fixed `#F84E63` / `#36D958` at half opacity.
fn view_mode_glyph(layout: super::DiffLayout, theme: super::theme::PrTheme) -> Div {
    let (deleted, added) = match layout {
        super::DiffLayout::Unified => ("pr-view-unified-deleted", "pr-view-unified-added"),
        super::DiffLayout::Split => ("pr-view-split-deleted", "pr-view-split-added"),
        super::DiffLayout::Auto => ("pr-view-auto-deleted", "pr-view-auto-added"),
    };
    let layer = |name: &'static str, color: gpui::Rgba| {
        icon(name, color.into())
            .absolute()
            .top_0()
            .left_0()
            .size(px(16.0))
    };
    div()
        .relative()
        .size(px(16.0))
        .child(layer("pr-view-frame", theme.text))
        .child(layer(deleted, gpui::rgb(0xf84e63)))
        .child(layer(added, gpui::rgb(0x36d958)))
}

/// The deleted-line change bar: red stripes over the row color.
fn deleted_bar(stripe: gpui::Rgba, row: gpui::Rgba) -> Div {
    div()
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(px(4.0))
        .child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    window.paint_quad(gpui::fill(bounds, row));
                    let height = f32::from(bounds.size.height);
                    let mut top = 0.0;
                    while top < height {
                        let stripe_bounds = gpui::Bounds::new(
                            gpui::point(bounds.origin.x, bounds.origin.y + px(top)),
                            gpui::size(
                                bounds.size.width,
                                px((DELETED_BAR_TILE / 2.0).min(height - top)),
                            ),
                        );
                        window.paint_quad(gpui::fill(stripe_bounds, stripe));
                        top += DELETED_BAR_TILE;
                    }
                },
            )
            .size_full(),
        )
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
