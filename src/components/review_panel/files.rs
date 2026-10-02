use super::*;
use super::{
    controls::Action,
    render::{addition_color, deletion_color},
};
use crate::{
    components::{
        diff_marks, file_type_icons,
        icons::icon,
        markdown::{MarkdownPreview, parse_markdown},
        pull_requests::theme::PrTheme,
        viewer_header,
    },
    theme::{Theme, UI_MONOSPACE_FONT_FAMILY},
};

/// The diff header row (`h-9.5` with its bottom rule); 1px follows it.
pub(super) const FILE_HEADER_HEIGHT: f32 = 38.;
/// An unmodified-lines separator row (`[data-separator]`).
const GAP_HEIGHT: f32 = 32.;
use gpui::{
    AnyElement, Div, MouseButton, Role, SharedString, Stateful, StyledText, div, prelude::*, rgba,
};

#[cfg(test)]
mod tree_tests;

impl ReviewPanel {
    pub(super) fn tree(&mut self, cx: &Context<Self>) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let rows = self.render_cache.tree.prepare(
            &self.query,
            &self.folder_collapsed,
            self.snapshot.files.iter().map(|file| file.path.as_str()),
        );
        let entity = cx.entity();
        let content = if rows.is_empty() {
            div()
                .px(px(16.))
                .py(px(8.))
                .text_size(px(13.))
                .text_color(t.text_tertiary)
                .child(crate::i18n::text("没有匹配的文件"))
                .into_any_element()
        } else {
            // Diff wheel events redraw this panel too. Build only the tree's
            // visible range, not a GPUI element (and listeners) for every file.
            gpui::uniform_list("review-tree-scroll", rows.len(), move |range, _, cx| {
                entity.update(cx, |panel, cx| {
                    range
                        .map(|i| panel.tree_row(&rows[i], cx))
                        .collect::<Vec<_>>()
                })
            })
            .track_scroll(&self.render_cache.tree_scroll)
            .min_h(px(0.))
            .flex_1()
            .w_full()
            .px(px(8.))
            .into_any_element()
        };
        div()
            .id("review-file-tree")
            .role(Role::Tree)
            .aria_label(crate::i18n::text("审查文件"))
            .w(px(250.))
            .min_w(px(160.))
            .max_w(gpui::relative(0.4))
            .h_full()
            .flex_none()
            .border_l_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .child(
                // The reference's `Filter files…` field: 28px, 12.5px
                // corners, the hairline border and a 16px search glyph.
                div()
                    .mx(px(8.))
                    .mt(px(4.))
                    .mb(px(1.))
                    .h(px(28.))
                    .flex_none()
                    .pl(px(9.))
                    .rounded(px(12.5))
                    .border_1()
                    .border_color(t.border)
                    .when(self.mode == ThemeMode::Dark, |field| {
                        field.bg(rgba(0xffffff08))
                    })
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        icon("search", t.text_tertiary.into())
                            .size(px(16.))
                            .flex_none(),
                    )
                    .child(div().min_w(px(0.)).flex_1().child(self.filter.clone())),
            )
            .child(content)
    }

    /// One row of the changed-file tree (`file-tree-container`): 29px, 3px
    /// inset, a 7.5px indent guide per level, the folder chevron or the file
    /// type glyph, the name, and a file's non-zero `+N` / `-N`. File names
    /// are tertiary until selected.
    fn tree_row(&self, row: &cache::TreeRow, cx: &Context<Self>) -> Stateful<Div> {
        #[cfg(test)]
        tree_tests::record_row();
        let t = Theme::for_mode(self.mode);
        let pr = PrTheme::for_mode(self.mode);
        // A folder's guide shows while the selected file is inside it.
        let selected_path = self
            .snapshot
            .files
            .get(self.selected_file)
            .map(|file| file.path.clone())
            .unwrap_or_default();
        let guides = move |path: &str, depth: usize| {
            let parts: Vec<&str> = path.split('/').collect();
            (0..depth)
                .map(|level| {
                    let folder = parts[..(level + 1).min(parts.len())].join("/");
                    selected_path.starts_with(&format!("{folder}/"))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(move |visible| {
                    div()
                        .flex_none()
                        .w(px(7.5))
                        .h_full()
                        .flex()
                        .justify_end()
                        .child(
                            div()
                                .w(px(1.))
                                .h_full()
                                .when(visible, |line| line.bg(pr.diff_gutter_text.alpha(0.25))),
                        )
                })
        };
        let base = |id: gpui::ElementId, label: String, depth: usize| {
            let lines = guides(&label, depth);
            div()
                .id(id)
                .role(Role::TreeItem)
                .aria_label(label)
                .w_full()
                .h(px(29.))
                .px(px(3.))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(5.))
                .rounded(px(6.))
                .text_size(px(13.))
                .cursor_pointer()
                .hover(move |b| b.bg(pr.row_hover).text_color(t.text))
                .children(lines)
        };
        match row {
            cache::TreeRow::Folder {
                path,
                name,
                depth,
                collapsed,
            } => {
                let toggle = path.clone();
                base(
                    SharedString::from(format!("review-folder-{path}")).into(),
                    path.clone(),
                    *depth,
                )
                .text_color(t.text)
                .on_click(cx.listener(move |s, _, _, cx| {
                    if !s.folder_collapsed.remove(&toggle) {
                        s.folder_collapsed.insert(toggle.clone());
                    }
                    cx.notify();
                }))
                .child(
                    div().w(px(16.)).flex_none().flex().justify_center().child(
                        icon(
                            if *collapsed {
                                "settings-chevron-right"
                            } else {
                                "chevron-down"
                            },
                            file_type_icons::muted_color(self.mode).into(),
                        )
                        .size(px(12.)),
                    ),
                )
                .child(div().min_w(px(0.)).flex_1().truncate().child(name.clone()))
            }
            cache::TreeRow::File { index, name, depth } => {
                let i = *index;
                let file = &self.snapshot.files[i];
                let selected = i == self.selected_file;
                let (asset, glyph_color) = file_type_icons::file_icon(&file.path, self.mode);
                base(("review-tree-file", i).into(), file.path.clone(), *depth)
                    .focusable()
                    .tab_stop(true)
                    .text_color(if selected { t.text } else { t.text_tertiary })
                    .when(selected, |b| b.bg(pr.row_selected))
                    .on_click(cx.listener(move |s, _, _, cx| s.jump_to(i, cx)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |s, _, _, cx| {
                            s.toggle_menu(Menu::File(i), cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_key_down(cx.listener(move |s, e: &KeyDownEvent, _, cx| {
                        match e.keystroke.key.as_str() {
                            "enter" | "space" => s.jump_to(i, cx),
                            "down" => s.jump_to((i + 1).min(s.snapshot.files.len() - 1), cx),
                            "up" => s.jump_to(i.saturating_sub(1), cx),
                            _ => return,
                        }
                        cx.stop_propagation();
                    }))
                    .child(
                        gpui::svg()
                            .path(format!("icons/{asset}.svg"))
                            .flex_none()
                            .size(px(16.))
                            .text_color(glyph_color),
                    )
                    .child(div().min_w(px(0.)).flex_1().truncate().child(name.clone()))
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .text_size(px(12.))
                            .font_features(crate::components::pull_requests::stats_font_features())
                            .when(file.additions > 0, |d| {
                                d.child(
                                    div()
                                        .text_color(addition_color(self.mode))
                                        .child(format!("+{}", file.additions)),
                                )
                            })
                            .when(file.deletions > 0, |d| {
                                d.child(
                                    div()
                                        .text_color(deletion_color(self.mode))
                                        .child(format!("-{}", file.deletions)),
                                )
                            }),
                    )
            }
        }
    }

    /// A file's diff header (`group/diff-header`): its type glyph, the path
    /// with the directory in tertiary text, a fold chevron on hover, the
    /// totals, and `Open in`, `Open in editor` and `File actions`.
    pub(super) fn file_header(&self, i: usize, cx: &Context<Self>) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let file = &self.snapshot.files[i];
        let path = file.path.clone();
        let collapsed = self.collapsed.contains(&path);
        let (asset, glyph_color) = file_type_icons::file_icon(&path, self.mode);
        let split = path.rfind('/').map_or(0, |slash| slash + 1);
        let label = StyledText::new(path.clone()).with_highlights([(
            0..split,
            gpui::HighlightStyle {
                color: Some(t.text_tertiary.into()),
                ..Default::default()
            },
        )]);
        let group: SharedString = format!("review-file-header-{i}").into();
        let chevron = icon("pr-toggle-file", t.text_tertiary.into())
            .size(px(16.))
            .flex_none()
            .when(!collapsed, |glyph| {
                glyph.with_transformation(gpui::Transformation::rotate(gpui::radians(
                    std::f32::consts::FRAC_PI_2,
                )))
            })
            .opacity(0.)
            .group_hover(group.clone(), |s| s.opacity(1.));
        // `Open in`, `Open in editor` and `File actions` turn primary on hover.
        let small = |id: String, label: &str, glyph: &'static str, size: f32, a: Action| {
            let hover: SharedString = format!("{id}-hover").into();
            self.control(id, label.to_owned().into(), a, cx)
                .group(hover.clone())
                .relative()
                .size(px(20.))
                .rounded_full()
                .child(
                    icon(glyph, t.text_tertiary.into())
                        .size(px(size))
                        .flex_none()
                        .group_hover(hover, move |s| s.text_color(t.text)),
                )
        };
        let actions = self.file_actions_anchor(i);
        div()
            .id(("review-file-header", i))
            .w_full()
            .flex_none()
            .pb(px(1.))
            .bg(t.surface)
            .child(
                div()
                    .id(("review-file-header-row", i))
                    .role(Role::Group)
                    .aria_label(path.clone())
                    .group(group.clone())
                    .h(px(FILE_HEADER_HEIGHT))
                    .w_full()
                    .pl(px(12.))
                    .pr(px(8.))
                    .border_b_1()
                    .border_color(t.border)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |s, _, w, cx| s.action(Action::Toggle(i), w, cx)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |s, _, _, cx| s.toggle_menu(Menu::File(i), cx)),
                    )
                    .child(
                        div()
                            .min_w(px(0.))
                            .flex_1()
                            .flex()
                            .items_center()
                            .pl(px(4.))
                            .gap(px(8.))
                            .child(
                                gpui::svg()
                                    .path(format!("icons/{asset}.svg"))
                                    .flex_none()
                                    .size(px(16.))
                                    .text_color(glyph_color),
                            )
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis_start()
                                    .text_color(t.text)
                                    .child(label),
                            )
                            .child(chevron),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(
                                div()
                                    .flex_none()
                                    .mx(px(4.))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .line_height(px(14.))
                                    .font_features(
                                        crate::components::pull_requests::stats_font_features(),
                                    )
                                    .when(self.diff_width < 260., |d| d.hidden())
                                    .child(
                                        div()
                                            .text_color(addition_color(self.mode))
                                            .child(format!("+{}", file.additions)),
                                    )
                                    .child(
                                        div()
                                            .text_color(deletion_color(self.mode))
                                            .child(format!("-{}", file.deletions)),
                                    ),
                            )
                            .when(self.hunk_actions_available(), |row| {
                                let staged = self.scope == Scope::Staged;
                                row.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(2.))
                                        .opacity(0.)
                                        .group_hover(group.clone(), |s| s.opacity(1.))
                                        .when(self.diff_width < 320., |d| d.hidden())
                                        .when(!staged, |d| {
                                            d.child(small(
                                                format!("review-restore-{i}"),
                                                crate::i18n::text("还原文件"),
                                                "review-restore",
                                                14.,
                                                Action::Confirm(Mutation::Discard(path.clone())),
                                            ))
                                        })
                                        .child(small(
                                            format!("review-stage-{i}"),
                                            if staged {
                                                crate::i18n::text("对文件取消暂存")
                                            } else {
                                                crate::i18n::text("暂存文件")
                                            },
                                            if staged {
                                                "review-minus"
                                            } else {
                                                "review-plus"
                                            },
                                            14.,
                                            Action::Mutation(if staged {
                                                Mutation::Unstage(Some(path.clone()))
                                            } else {
                                                Mutation::Stage(Some(path.clone()))
                                            }),
                                        )),
                                )
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .when(self.diff_width < 280., |d| d.hidden())
                                    .child(small(
                                        format!("review-open-external-{i}"),
                                        crate::i18n::text("打开方式"),
                                        "review-open",
                                        14.,
                                        Action::OpenExternal(i),
                                    ))
                                    .child(small(
                                        format!("review-open-{i}"),
                                        crate::i18n::text("在编辑器中打开"),
                                        "review-open-in-editor",
                                        16.,
                                        Action::Open(i),
                                    )),
                            )
                            .child(
                                small(
                                    format!("review-file-actions-{i}"),
                                    crate::i18n::text("文件操作"),
                                    "review-options",
                                    16.,
                                    Action::Menu(Menu::File(i)),
                                )
                                .child(actions),
                            ),
                    ),
            )
    }

    /// Records where file `i`'s `File actions` button is, so its menu opens
    /// under it.
    fn file_actions_anchor(&self, i: usize) -> impl IntoElement {
        let anchors = self.menu_anchors.clone();
        gpui::canvas(
            move |bounds, _, _| {
                anchors.borrow_mut().insert(i, bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    /// Hunk staging is offered where the reference offers it: on the
    /// unstaged and staged lists of a modified file.
    fn hunk_actions_available(&self) -> bool {
        matches!(self.scope, Scope::Unstaged | Scope::Staged)
    }

    /// The capsule over a hovered hunk's last changed line: `Revert` and
    /// `Stage` on the unstaged list, `Unstage` on the staged one.
    fn hunk_actions(&self, file: usize, hunk: usize, cx: &Context<Self>) -> impl IntoElement {
        let diff = &self.snapshot.files[file];
        let path = diff.path.clone();
        let staged = self.scope == Scope::Staged;
        let capsule = viewer_header::capsule(self.mode)
            .when(!staged, |capsule| {
                capsule.child(self.capsule_button(
                    format!("review-hunk-revert-{file}-{hunk}"),
                    crate::i18n::text("还原"),
                    self.capsule_glyph("review-restore", false),
                    false,
                    Action::Confirm(Mutation::DiscardHunk {
                        path: path.clone(),
                        index: hunk,
                    }),
                    cx,
                ))
            })
            .child(self.capsule_button(
                format!("review-hunk-stage-{file}-{hunk}"),
                if staged {
                    crate::i18n::text("取消暂存")
                } else {
                    crate::i18n::text("暂存")
                },
                self.capsule_glyph(
                    if staged {
                        "review-minus"
                    } else {
                        "review-plus"
                    },
                    false,
                ),
                false,
                Action::Mutation(Mutation::Hunk {
                    path,
                    index: hunk,
                    reverse: staged,
                }),
                cx,
            ));
        gpui::deferred(
            div()
                .absolute()
                .right(px(4.))
                .top(px((21.6 - 32.) / 2.))
                .when(diff.status != 'M', |d| d.hidden())
                .child(capsule),
        )
        .with_priority(1)
    }

    /// An unchanged run of lines (`[data-separator]`): a 32px row whose
    /// `8px`-rounded band reads `N unmodified lines`, with the expand control
    /// over the gutter (upwards before the first hunk, downwards after the
    /// last, both ways between hunks).
    fn gap_row(&self, file: usize, hunk: usize, count: u32, cx: &Context<Self>) -> Stateful<Div> {
        let pr = PrTheme::for_mode(self.mode);
        let label = if count == 1 {
            crate::i18n::text("1 行未修改").to_owned()
        } else {
            crate::i18n::format!("{count} 行未修改" => "{count} unmodified lines")
        };
        let hunks = self.snapshot.files[file].hunks.len();
        let glyph = |up: bool| {
            let glyph = icon("pr-diff-expand", pr.diff_gutter_text.into()).size(px(16.));
            if up {
                glyph.with_transformation(gpui::Transformation::rotate(gpui::radians(
                    std::f32::consts::PI,
                )))
            } else {
                glyph
            }
        };
        let cell = |height: f32, up: bool| {
            div()
                .w(px(self.render_cache.gutter_width))
                .h(px(height))
                .flex()
                .items_center()
                .justify_center()
                .bg(pr.diff_expander_surface)
                .child(glyph(up))
        };
        let buttons = if hunk == 0 || hunk >= hunks {
            div().child(cell(GAP_HEIGHT, hunk == 0))
        } else {
            div()
                .flex()
                .flex_col()
                .child(cell(GAP_HEIGHT / 2., false))
                .child(cell(GAP_HEIGHT / 2., true))
        };
        let group: SharedString = format!("review-gap-{file}-{hunk}").into();
        div()
            .id(SharedString::from(format!("review-gap-{file}-{hunk}")))
            .group(group.clone())
            .role(Role::Button)
            .aria_label(label.clone())
            .w_full()
            .h(px(GAP_HEIGHT))
            .px(px(8.))
            .flex()
            .bg(pr.surface)
            .cursor_pointer()
            .on_scroll_wheel(cx.listener(Self::scroll_diff_wheel))
            .on_click(cx.listener(move |s, _, w, cx| s.action(Action::Context(file), w, cx)))
            .child(
                buttons
                    .flex_none()
                    .rounded_l(px(8.))
                    .overflow_hidden()
                    .border_r_1()
                    .border_color(pr.surface),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .items_center()
                    .px(px(7.6242))
                    .rounded_r(px(8.))
                    .bg(pr.diff_expander_surface)
                    .text_size(px(12.))
                    .line_height(px(21.6))
                    .text_color(pr.diff_gutter_text)
                    .child(
                        div()
                            .truncate()
                            .group_hover(group, |s| s.underline())
                            .child(label),
                    ),
            )
    }

    pub(super) fn code_cell(
        &mut self,
        index: usize,
        file: usize,
        line: Option<&Line>,
        old: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let pr = PrTheme::for_mode(self.mode);
        let Some(line) = line else {
            return div()
                .id(("review-empty-cell", index * 2 + old as usize))
                .on_scroll_wheel(cx.listener(Self::scroll_diff_wheel))
                .flex_1()
                .min_w(px(0.))
                .h(px(21.6))
                .bg(t.text.alpha(0.025));
        };
        let added = line.kind == LineKind::Added;
        let deleted = line.kind == LineKind::Deleted;
        let (bg, gutter, number_color) = if added {
            (
                pr.diff_added_surface,
                pr.diff_added_gutter,
                pr.diff_added_text,
            )
        } else if deleted {
            (
                pr.diff_deleted_surface,
                pr.diff_deleted_gutter,
                pr.diff_deleted_text,
            )
        } else {
            (t.surface, t.surface, pr.diff_gutter_text)
        };
        let number = if old { line.old } else { line.new.or(line.old) }.unwrap_or(0);
        let comment_old = old || line.kind == LineKind::Deleted;
        let draft = Draft {
            file,
            start: number,
            end: number,
            old: comment_old,
        };
        let move_draft = draft.clone();
        let selected = self.selection.filter(|s| s.old == old).and_then(|s| {
            let a = s.anchor.min(s.head);
            let b = s.anchor.max(s.head);
            (index >= a.row && index <= b.row).then_some(
                (if index == a.row {
                    a.byte.min(line.text.len())
                } else {
                    0
                })..(if index == b.row {
                    b.byte.min(line.text.len())
                } else {
                    line.text.len()
                }),
            )
        });
        let code = line.text.clone();
        let spans = self.render_cache.syntax_runs(file, line);
        let words = if self.words && (added || deleted) {
            self.render_cache.word_spans(file, line)
        } else {
            Vec::new()
        };
        let word_color = if deleted {
            pr.diff_deleted_word
        } else {
            pr.diff_added_word
        };
        let runs = decorate_runs(&spans, selected.map(|r| (r, t.accent.alpha(0.4).into())));
        let content = StyledText::new(code.clone()).with_runs(runs);
        let down_layout = content.layout().clone();
        let move_layout = down_layout.clone();
        let word_layout = down_layout.clone();
        let hover_group: SharedString = format!("review-line-{index}-{old}").into();
        div()
            .id(("review-code-cell", index * 2 + old as usize))
            .group(hover_group.clone())
            .relative()
            .on_scroll_wheel(cx.listener(Self::scroll_diff_wheel))
            .min_w(px(0.))
            .flex_1()
            .min_h(px(21.6))
            .flex()
            .items_stretch()
            .bg(bg)
            .font_family(UI_MONOSPACE_FONT_FAMILY)
            .text_size(px(12.))
            .line_height(px(21.6))
            .text_color(pr.diff_context_text)
            .child(
                div()
                    .id(("review-line-number", index * 2 + old as usize))
                    .role(Role::Button)
                    .aria_label(crate::i18n::format!(
                        "在 {} 第 {}{} 行添加评论" => "Add comment in {} at line {}{}",
                        self.snapshot.files[file].path,
                        if comment_old { "L" } else { "R" },
                        number
                    ))
                    // The number sits `pe` 1ch from a 2px surface rule, with
                    // the 4px change bar at the outer edge.
                    .relative()
                    .w(px(self.render_cache.gutter_width))
                    .flex_none()
                    .min_h(px(21.6))
                    .pr(px(7.22461))
                    .border_r(px(2.))
                    .border_color(t.surface)
                    .bg(gutter)
                    .text_right()
                    .whitespace_nowrap()
                    .text_color(number_color)
                    .cursor_pointer()
                    .when(added, |cell| {
                        cell.child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(4.))
                                .bg(pr.diff_added_text),
                        )
                    })
                    .when(deleted, |cell| {
                        cell.child(diff_marks::deleted_bar(
                            pr.diff_deleted_text,
                            pr.diff_deleted_surface,
                        ))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, _, _, cx| {
                            s.gutter_drag = Some(draft.clone());
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(move |s, e: &gpui::MouseMoveEvent, _, cx| {
                        if e.pressed_button == Some(MouseButton::Left)
                            && let Some(d) = s.gutter_drag.as_mut()
                            && d.file == move_draft.file
                            && d.old == move_draft.old
                        {
                            d.end = move_draft.end;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Some(d) = s.gutter_drag.take() {
                            s.begin_comment(d, cx);
                        }
                        cx.stop_propagation();
                    }))
                    .child(number.to_string()),
            )
            .child(
                div()
                    .id(("review-line-text", index * 2 + old as usize))
                    .min_w(px(0.))
                    .flex_1()
                    .overflow_hidden()
                    .px(px(7.2246))
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, e: &gpui::MouseDownEvent, w, cx| {
                            let byte = down_layout
                                .index_for_position(e.position)
                                .unwrap_or_else(|i| i)
                                .min(code.len());
                            let cursor = TextCursor { row: index, byte };
                            if e.modifiers.shift && s.selection.is_some_and(|v| v.old == old) {
                                s.selection.as_mut().unwrap().head = cursor;
                            } else {
                                s.selection = Some(Selection {
                                    anchor: cursor,
                                    head: cursor,
                                    old,
                                });
                            }
                            if e.click_count >= 3 {
                                s.selection = Some(Selection {
                                    anchor: TextCursor {
                                        row: index,
                                        byte: 0,
                                    },
                                    head: TextCursor {
                                        row: index,
                                        byte: code.len(),
                                    },
                                    old,
                                });
                            } else if e.click_count == 2 {
                                let r = word_at(&code, byte);
                                s.selection = Some(Selection {
                                    anchor: TextCursor {
                                        row: index,
                                        byte: r.start,
                                    },
                                    head: TextCursor {
                                        row: index,
                                        byte: r.end,
                                    },
                                    old,
                                });
                            }
                            s.selecting = true;
                            s.focus.focus(w, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(move |s, e: &gpui::MouseMoveEvent, _, cx| {
                        if s.selecting
                            && e.pressed_button == Some(MouseButton::Left)
                            && let Some(selection) = s.selection.as_mut()
                            && selection.old == old
                        {
                            selection.head = TextCursor {
                                row: index,
                                byte: move_layout
                                    .index_for_position(e.position)
                                    .unwrap_or_else(|i| i),
                            };
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .relative()
                            .when(self.wrap, |d| d.w_full().whitespace_normal())
                            .when(!self.wrap, |d| {
                                d.w(px(self.max_line_width))
                                    .left(px(-self.horizontal_offset))
                                    .whitespace_nowrap()
                            })
                            .when(!words.is_empty(), |d| {
                                d.child(diff_marks::word_boxes(word_layout, words, word_color))
                            })
                            .child(content),
                    ),
            )
            .child(
                // `[data-utility-button]`: the add-comment square over the
                // number while the line is hovered, above the cell's text.
                div()
                    .absolute()
                    .left(px(28.96875))
                    .top_0()
                    .size(px(21.6))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .bg(t.text)
                    .opacity(0.)
                    .group_hover(hover_group, |s| s.opacity(1.))
                    .child(icon("pr-diff-plus", pr.diff_utility_glyph.into()).size(px(16.))),
            )
    }

    pub(super) fn render_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = Theme::for_mode(self.mode);
        match self.rows[index].clone() {
            Row::Header(i) => self.file_header(i, cx).into_any_element(),
            Row::Gap { file, hunk, count } => {
                self.gap_row(file, hunk, count, cx).into_any_element()
            }
            Row::End(_) => div().w_full().h(px(17.)).into_any_element(),
            Row::Code {
                file,
                hunk,
                left,
                right,
            } => {
                let mut row = div()
                    .id(("review-code-row", index))
                    .relative()
                    .w_full()
                    .min_h(px(21.6))
                    .flex()
                    .items_stretch();
                if self.split {
                    row = row
                        .child(self.code_cell(index, file, left.as_ref(), true, cx))
                        .child(div().w(px(1.)).bg(t.border));
                }
                row = row.child(self.code_cell(index, file, right.as_ref(), false, cx));
                if let Some(hunk) = hunk.filter(|_| self.hunk_actions_available()) {
                    row = row.on_hover(cx.listener(move |s, hovered: &bool, _, cx| {
                        let key = Some((file, hunk));
                        if *hovered && s.hovered_hunk != key {
                            s.hovered_hunk = key;
                            cx.notify();
                        } else if !*hovered && s.hovered_hunk == key {
                            s.hovered_hunk = None;
                            cx.notify();
                        }
                    }));
                    if self.hunk_action_rows.get(&index) == Some(&(file, hunk))
                        && self.hovered_hunk == Some((file, hunk))
                    {
                        row = row.child(self.hunk_actions(file, hunk, cx));
                    }
                }
                row.into_any_element()
            }
            Row::Comment(id) => {
                if let Some(c) = self.comments.iter().find(|c| c.id == id) {
                    self.comment_card(c.clone(), self.editing_comment == Some(id), cx)
                        .into_any_element()
                } else {
                    div().into_any_element()
                }
            }
            Row::Draft => {
                let d = self.draft.as_ref().expect("draft row");
                self.comment_card(
                    Comment {
                        id: 0,
                        path: String::new(),
                        start: d.start.min(d.end),
                        end: d.start.max(d.end),
                        old: d.old,
                        text: String::new(),
                    },
                    true,
                    cx,
                )
                .into_any_element()
            }
            Row::Binary(i) => div()
                .p(px(28.))
                .text_size(px(13.))
                .text_color(t.text_tertiary)
                .child(crate::i18n::text("二进制文件内容已更改"))
                .child(self.button(
                    format!("review-binary-open-{i}"),
                    crate::i18n::text("打开文件"),
                    None,
                    Action::Open(i),
                    cx,
                ))
                .into_any_element(),
            Row::Empty(i) => div()
                .p(px(20.))
                .text_size(px(13.))
                .text_color(t.text_tertiary)
                .child(if self.snapshot.files[i].old_path.is_some() {
                    crate::i18n::text("文件已重命名，内容未更改")
                } else {
                    crate::i18n::text("没有文本差异")
                })
                .into_any_element(),
            Row::Preview(i) => {
                let file = &self.snapshot.files[i];
                let path = file.path.clone();
                let mode = self.mode;
                let text = file.new_text.clone().unwrap_or_default();
                let preview = self.previews.entry(path).or_insert_with(|| {
                    cx.new(|cx| MarkdownPreview::new(parse_markdown(&text), mode, cx))
                });
                div()
                    .w_full()
                    .h(px(440.))
                    .overflow_hidden()
                    .child(preview.clone())
                    .into_any_element()
            }
        }
    }
}

fn decorate_runs(
    spans: &[(std::ops::Range<usize>, gpui::TextRun)],
    selection: Option<(std::ops::Range<usize>, gpui::Hsla)>,
) -> Vec<gpui::TextRun> {
    let mut result = Vec::new();
    for (span, run) in spans {
        let mut cuts = vec![span.start, span.end];
        for (range, _) in [&selection].into_iter().flatten() {
            if range.start > span.start && range.start < span.end {
                cuts.push(range.start);
            }
            if range.end > span.start && range.end < span.end {
                cuts.push(range.end);
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for p in cuts.windows(2) {
            let mut r = run.clone();
            r.len = p[1] - p[0];
            for (range, color) in [&selection].into_iter().flatten() {
                if range.contains(&p[0]) {
                    r.background_color = Some(*color);
                }
            }
            result.push(r);
        }
    }
    result
}
fn word_at(text: &str, byte: usize) -> std::ops::Range<usize> {
    let mut a = byte.min(text.len());
    while !text.is_char_boundary(a) {
        a -= 1;
    }
    let mut b = a;
    while a > 0 {
        let (i, c) = text[..a].char_indices().next_back().unwrap();
        if !c.is_alphanumeric() && c != '_' {
            break;
        }
        a = i;
    }
    while b < text.len() {
        let c = text[b..].chars().next().unwrap();
        if !c.is_alphanumeric() && c != '_' {
            break;
        }
        b += c.len_utf8();
    }
    a..b
}
