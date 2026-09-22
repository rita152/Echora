use super::render::{addition_color, deletion_color};
use super::*;
use crate::{components::icons::icon, theme::Theme};
use gpui::{
    ClipboardItem, Div, MouseButton, Render, Role, SharedString, Stateful, div, prelude::*, rgba,
};

#[cfg(test)]
mod tests;

/// One row of a review popup. ChatGPT draws a leading glyph for the diff
/// controls, a trailing glyph for the submenu entry and the active comparison
/// source, and a hairline rule above the first row of a group.
#[derive(Clone)]
pub(super) struct MenuEntry {
    pub(super) label: String,
    pub(super) action: Action,
    pub(super) leading: Option<&'static str>,
    pub(super) trailing: Option<&'static str>,
    pub(super) separator_before: bool,
}

impl MenuEntry {
    pub(super) fn new(label: impl Into<String>, action: Action) -> Self {
        Self {
            label: label.into(),
            action,
            leading: None,
            trailing: None,
            separator_before: false,
        }
    }

    pub(super) fn leading(mut self, glyph: &'static str) -> Self {
        self.leading = Some(glyph);
        self
    }

    pub(super) fn trailing(mut self, glyph: &'static str) -> Self {
        self.trailing = Some(glyph);
        self
    }

    pub(super) fn separated(mut self) -> Self {
        self.separator_before = true;
        self
    }
}

#[derive(Clone)]
pub(super) enum Action {
    Menu(Menu),
    Scope(Scope),
    Refresh,
    Wrap,
    Context(usize),
    LoadFiles,
    Rich,
    Words,
    Whitespace,
    CopyPatch,
    Collapse,
    Split,
    Tree,
    Close,
    AddTab,
    Fullscreen,
    Jump(usize),
    Toggle(usize),
    Copy(String),
    Open(usize),
    OpenTab(String),
    Reveal(usize),
    Mutation(Mutation),
    Confirm(Mutation),
    Commit,
    SaveCommit,
    SaveComment,
    EditComment(u64),
    CancelComment,
    DeleteComment(u64),
    CancelDialog,
    CommitAll,
    CommitAndPush,
    PullRequest,
    CreatePullRequest(bool),
    PrBase(String),
    OpenPr(String),
    Viewed(usize),
    NewBranch(bool),
}

/// ChatGPT paints these popups with `bg-surface-elevated-secondary/90` over the
/// review pane, which composites to `surface-elevated` at 90% — the same value
/// the theme already carries for "elevated overlay over the app shell".
pub(super) fn menu_surface(t: Theme) -> gpui::Rgba {
    t.control
}

/// The 1px group rule inside a review popup: a full-width hairline with 4px of
/// vertical and 8px of horizontal breathing room, exactly like the reference's
/// `px-row-x py-1` wrapper.
fn menu_separator(t: Theme) -> Div {
    div()
        .w_full()
        .flex_none()
        .px(px(8.))
        .py(px(4.))
        .child(div().h(px(1.)).w_full().bg(t.border))
}

impl ReviewPanel {
    pub(super) fn action(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        if matches!(&a,Action::Open(i)|Action::Reveal(i)|Action::Viewed(i)|Action::Toggle(i)|Action::Jump(i)|Action::Context(i)|Action::Menu(Menu::File(i)) if *i>=self.snapshot.files.len())
        {
            return;
        }
        let save = matches!(
            a,
            Action::Wrap
                | Action::LoadFiles
                | Action::Rich
                | Action::Words
                | Action::Whitespace
                | Action::Split
                | Action::Tree
        );
        match a {
            Action::Menu(m) => {
                let jump = m == Menu::Jump;
                self.toggle_menu(m, cx);
                if jump {
                    self.jump.read(cx).focus_handle(cx).focus(w, cx);
                    self.focus_pending = false;
                }
            }
            Action::Scope(s) => {
                self.history_pinned = false;
                self.last_turn = self.latest_turn.clone();
                self.change_scope(s, cx);
            }
            Action::Refresh => {
                self.operation_error = None;
                self.menu = None;
                self.refresh(cx);
            }
            Action::Wrap => {
                self.wrap = !self.wrap;
                self.menu = None;
                self.scroll.remeasure();
            }
            Action::Context(file) => {
                let path = self.snapshot.files[file].path.clone();
                if !self.expanded_files.remove(&path) {
                    self.expanded_files.insert(path);
                }
                self.menu = None;
                self.generation += 1;
                self.loading = false;
                self.refresh(cx);
            }
            Action::LoadFiles => {
                self.load_files = !self.load_files;
                self.menu = None;
                self.generation += 1;
                self.loading = false;
                self.refresh(cx);
            }
            Action::Rich => {
                self.rich = !self.rich;
                self.menu = None;
                self.rebuild(cx);
            }
            Action::Words => {
                self.words = !self.words;
                self.menu = None;
            }
            Action::Whitespace => {
                self.whitespace = !self.whitespace;
                self.menu = None;
                self.refresh(cx);
            }
            Action::CopyPatch => {
                cx.write_to_clipboard(ClipboardItem::new_string(git_review::apply_command(
                    &self.snapshot.files,
                )));
                self.menu = None;
                self.show_notice(crate::i18n::text("已复制 git apply 命令").into(), cx);
            }
            Action::Collapse => self.toggle_all(cx),
            Action::Split => {
                self.split = !self.split;
                self.rebuild(cx);
            }
            Action::Tree => self.tree_open = !self.tree_open,
            Action::Close => cx.emit(ReviewEvent::Close),
            Action::AddTab => cx.emit(ReviewEvent::AddTab),
            Action::Fullscreen => cx.emit(ReviewEvent::Fullscreen),
            Action::Jump(i) => self.jump_to(i, cx),
            Action::Toggle(i) => self.toggle_file(i, cx),
            Action::Copy(s) => {
                cx.write_to_clipboard(ClipboardItem::new_string(s));
                self.menu = None;
                self.show_notice(crate::i18n::text("已复制").into(), cx);
            }
            Action::Viewed(i) => {
                let f = &self.snapshot.files[i];
                if self.viewed.remove(&f.path).is_some() {
                    self.collapsed.remove(&f.path);
                } else {
                    self.viewed.insert(f.path.clone(), f.patch.clone());
                    self.collapsed.insert(f.path.clone());
                }
                self.rebuild(cx);
            }
            Action::OpenTab(path) => cx.emit(ReviewEvent::OpenFile { path, line: None }),
            Action::Open(i) => {
                let file = &self.snapshot.files[i];
                cx.emit(ReviewEvent::OpenFile {
                    path: self.snapshot.root.join(&file.path).to_string_lossy().into(),
                    line: None,
                });
                self.menu = None;
            }
            Action::Reveal(i) => {
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(self.snapshot.root.join(&self.snapshot.files[i].path))
                    .spawn();
                self.menu = None;
            }
            Action::Mutation(op) => self.mutate(op, cx),
            Action::Confirm(op) => {
                self.pr_open = false;
                self.menu = None;
                self.confirm = Some(op);
            }
            Action::Commit => self.open_commit(cx),
            Action::SaveCommit => {
                if self.busy
                    || (self.new_branch && self.branch_input.read(cx).text().trim() == "codex/")
                {
                    return;
                }
                let message = self.commit_input.read(cx).text().to_owned();
                self.mutate(
                    Mutation::Commit {
                        message,
                        stage_all: self.commit_all,
                        branch: self
                            .new_branch
                            .then(|| self.branch_input.read(cx).text().trim().to_owned()),
                    },
                    cx,
                );
            }
            Action::SaveComment => self.save_comment(cx),
            Action::EditComment(id) => self.edit_comment(id, cx),
            Action::CancelComment => {
                self.draft = None;
                self.editing_comment = None;
                self.rebuild(cx);
                self.focus_pending = true;
            }
            Action::DeleteComment(id) => {
                if self.editing_comment == Some(id) {
                    self.editing_comment = None;
                }
                self.comments.retain(|c| c.id != id);
                self.emit_comments(cx);
                self.rebuild(cx);
            }
            Action::CancelDialog => {
                self.pr_open = false;
                self.confirm = None;
                self.commit_open = false;
                self.focus_pending = true;
            }
            Action::CommitAll => self.commit_all = !self.commit_all,
            Action::CommitAndPush => {
                if self.busy
                    || (self.new_branch && self.branch_input.read(cx).text().trim() == "codex/")
                {
                    return;
                }
                self.push_after_commit = true;
                self.action(Action::SaveCommit, w, cx);
            }
            Action::NewBranch(new) => {
                self.new_branch = new;
                self.menu = None;
            }
            Action::PullRequest => self.open_pull_request(cx),
            Action::PrBase(base) => {
                self.pr_base = base;
                self.menu = None;
            }
            Action::OpenPr(url) => cx.open_url(&url),
            Action::CreatePullRequest(draft) => {
                if self.busy
                    || (self.new_branch && self.branch_input.read(cx).text().trim() == "codex/")
                {
                    return;
                }
                if let Some(url) = self.existing_pr_for_head() {
                    cx.open_url(url);
                    return;
                }
                self.mutate(
                    Mutation::PullRequest(git_review::PullRequestOptions {
                        title: self.pr_title.read(cx).text().into(),
                        body: self.commit_input.read(cx).text().into(),
                        base: self.pr_base.clone(),
                        branch: self
                            .new_branch
                            .then(|| self.branch_input.read(cx).text().trim().into()),
                        include_local: self.commit_all,
                        draft,
                    }),
                    cx,
                );
            }
        }
        if save {
            self.save_preferences(cx);
        }
        cx.notify();
    }

    pub(super) fn button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        glyph: Option<&'static str>,
        a: Action,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let label = label.into();
        let hint = label.clone();
        let key_action = a.clone();
        div()
            .id(id.into())
            .role(Role::Button)
            .aria_label(label.clone())
            .focusable()
            .tab_stop(true)
            .h(px(28.))
            // Callers also use 20px icon buttons. Padding must not squeeze
            // their 16px glyphs; keep the default toolbar hit area at 28px.
            .px(px(if glyph.is_some() { 0. } else { 8. }))
            .when(glyph.is_some(), |b| b.w(px(28.)))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .rounded(px(12.5))
            .text_size(px(13.))
            .line_height(px(18.))
            .cursor_pointer()
            .text_color(t.text_secondary)
            .hover(move |s| s.bg(t.sidebar_hover).text_color(t.text))
            .focus_visible(move |s| s.border_1().border_color(t.accent))
            .tooltip(move |_, cx| cx.new(|_| ReviewTooltip(hint.clone())).into())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |s, _, w, cx| {
                cx.stop_propagation();
                s.action(a.clone(), w, cx);
            }))
            .on_key_down(cx.listener(move |s, e: &KeyDownEvent, w, cx| {
                if e.keystroke.key == "enter" || e.keystroke.key == "space" {
                    if s.menu.is_some() && matches!(key_action, Action::Menu(_)) {
                        s.key_down(e, w, cx);
                    } else {
                        cx.stop_propagation();
                        s.action(key_action.clone(), w, cx);
                    }
                }
            }))
            .when_some(glyph, |b, g| {
                b.child(icon(g, t.text_secondary.into()).size(px(16.)).flex_none())
            })
            .when(glyph.is_none(), |b| b.child(label))
    }

    pub(super) fn dropdown_button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        menu: Menu,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let id = id.into();
        let debug = id.to_string();
        // Keep the chevron off the text baseline and inside the same mouse /
        // keyboard hit target as the label, in both toolbar rows.
        self.button(id, label, None, Action::Menu(menu), cx)
            .debug_selector(move || debug.clone())
            .child(
                icon("chevron-down", t.text_secondary.into())
                    .size(px(12.))
                    .flex_none(),
            )
    }

    pub(super) fn menu_actions(&self, menu: &Menu) -> Vec<(String, Action)> {
        self.menu_entries(menu)
            .into_iter()
            .map(|entry| (entry.label, entry.action))
            .collect()
    }

    pub(super) fn menu_entries(&self, menu: &Menu) -> Vec<MenuEntry> {
        let toggle =
            |on: bool, yes: &str, no: &str| -> String { if on { yes.into() } else { no.into() } };
        match menu {
            // ChatGPT divides the comparison sources into "turn" / "working
            // tree" / "history" groups and ticks the active one.
            Menu::Scope => {
                let branch = self.snapshot.upstream.clone().unwrap_or_else(|| {
                    self.snapshot
                        .branches
                        .iter()
                        .find(|s| s.as_str() == "main")
                        .cloned()
                        .unwrap_or_else(|| "HEAD".into())
                });
                let mut entries = vec![
                    MenuEntry::new(crate::i18n::text("上一轮"), Action::Scope(Scope::LastTurn)),
                    MenuEntry::new(
                        crate::i18n::text("未提交"),
                        Action::Scope(Scope::Uncommitted),
                    )
                    .separated(),
                    MenuEntry::new(crate::i18n::text("未暂存"), Action::Scope(Scope::Unstaged)),
                    MenuEntry::new(crate::i18n::text("已暂存"), Action::Scope(Scope::Staged)),
                    MenuEntry::new(crate::i18n::text("已提交"), Action::Menu(Menu::Commits))
                        .trailing("review-chevron-right")
                        .separated(),
                    MenuEntry::new(
                        crate::i18n::text("分支"),
                        Action::Scope(Scope::Branch(branch)),
                    ),
                ];
                // The active comparison source carries the reference's tick.
                let current = match &self.scope {
                    Scope::LastTurn => 0,
                    Scope::Uncommitted => 1,
                    Scope::Unstaged => 2,
                    Scope::Staged => 3,
                    Scope::Commit(_) => 4,
                    Scope::Branch(_) => 5,
                };
                entries[current].trailing = Some("check");
                entries
            }
            Menu::View => vec![
                MenuEntry::new(crate::i18n::text("刷新"), Action::Refresh)
                    .leading("review-refresh"),
                MenuEntry::new(
                    toggle(
                        self.wrap,
                        crate::i18n::text("禁用自动换行"),
                        crate::i18n::text("启用自动换行"),
                    ),
                    Action::Wrap,
                )
                .leading("review-word-wrap"),
                MenuEntry::new(
                    if self.split {
                        crate::i18n::text("切换到统一差异视图")
                    } else {
                        crate::i18n::text("切换到拆分差异视图")
                    },
                    Action::Split,
                )
                .leading("review-split-diff"),
                MenuEntry::new(
                    if self.collapsed.len() == self.snapshot.files.len()
                        && !self.snapshot.files.is_empty()
                    {
                        crate::i18n::text("展开全部差异")
                    } else {
                        crate::i18n::text("折叠全部差异")
                    },
                    Action::Collapse,
                )
                .leading("review-collapse-all"),
                MenuEntry::new(
                    toggle(
                        self.load_files,
                        crate::i18n::text("不加载完整文件"),
                        crate::i18n::text("加载完整文件"),
                    ),
                    Action::LoadFiles,
                )
                .leading("review-load-full-files")
                .separated(),
                MenuEntry::new(
                    toggle(
                        self.rich,
                        crate::i18n::text("禁用富文本预览"),
                        crate::i18n::text("启用富文本预览"),
                    ),
                    Action::Rich,
                )
                .leading("review-rich-preview"),
                MenuEntry::new(
                    toggle(
                        self.words,
                        crate::i18n::text("禁用文字差异"),
                        crate::i18n::text("启用文字差异"),
                    ),
                    Action::Words,
                )
                .leading("review-word-diffs"),
                MenuEntry::new(
                    toggle(
                        self.whitespace,
                        crate::i18n::text("显示空白字符"),
                        crate::i18n::text("隐藏空白字符"),
                    ),
                    Action::Whitespace,
                )
                .leading("review-white-space"),
                MenuEntry::new(crate::i18n::text("复制 git apply 命令"), Action::CopyPatch)
                    .leading("review-copy-apply"),
            ],
            Menu::Git => vec![
                MenuEntry::new(crate::i18n::text("提交或推送"), Action::Commit),
                MenuEntry::new(crate::i18n::text("创建 Pull Request"), Action::PullRequest),
            ],
            Menu::CommitBranch => vec![
                MenuEntry::new(self.snapshot.branch.clone(), Action::NewBranch(false)),
                MenuEntry::new(crate::i18n::text("新分支"), Action::NewBranch(true)),
            ],
            Menu::PullRequestBase => {
                let mut names = self
                    .snapshot
                    .branches
                    .iter()
                    .map(|s| s.trim_start_matches("origin/").to_owned())
                    .collect::<Vec<_>>();
                names.sort();
                names.dedup();
                names
                    .into_iter()
                    .map(|s| MenuEntry::new(s.clone(), Action::PrBase(s)))
                    .collect()
            }
            Menu::Branch => self
                .snapshot
                .branches
                .iter()
                .map(|s| MenuEntry::new(s.clone(), Action::Scope(Scope::Branch(s.clone()))))
                .collect(),
            Menu::Commits => self
                .snapshot
                .commits
                .iter()
                .map(|(sha, title)| {
                    MenuEntry::new(
                        format!("{}  {title}", &sha[..7.min(sha.len())]),
                        Action::Scope(Scope::Commit(sha.clone())),
                    )
                })
                .collect(),
            Menu::Jump => self
                .matching_files(&self.jump_query)
                .into_iter()
                .map(|i| MenuEntry::new(self.snapshot.files[i].path.clone(), Action::Jump(i)))
                .collect(),
            Menu::File(i) => {
                let file = &self.snapshot.files[*i];
                let mut a = vec![
                    MenuEntry::new(crate::i18n::text("打开文件"), Action::Open(*i)),
                    MenuEntry::new(crate::i18n::text("在访达中显示"), Action::Reveal(*i)),
                    MenuEntry::new(
                        crate::i18n::text("复制路径"),
                        Action::Copy(file.path.clone()),
                    ),
                    MenuEntry::new(
                        crate::i18n::text("复制绝对路径"),
                        Action::Copy(self.snapshot.root.join(&file.path).to_string_lossy().into()),
                    ),
                ];
                if self.scope.editable() {
                    a.push(MenuEntry::new(
                        if self.scope == Scope::Staged {
                            crate::i18n::text("取消暂存")
                        } else {
                            crate::i18n::text("暂存更改")
                        },
                        Action::Mutation(if self.scope == Scope::Staged {
                            Mutation::Unstage(Some(file.path.clone()))
                        } else {
                            Mutation::Stage(Some(file.path.clone()))
                        }),
                    ));
                    a.push(MenuEntry::new(
                        crate::i18n::text("撤销更改…"),
                        Action::Confirm(Mutation::Discard(file.path.clone())),
                    ));
                }
                a
            }
        }
    }
    pub(super) fn activate_menu(
        &mut self,
        m: Menu,
        index: usize,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, a)) = self.menu_actions(&m).get(index) {
            self.action(a.clone(), w, cx);
        }
    }

    pub(super) fn popup(&self, m: Menu, cx: &Context<Self>) -> Stateful<Div> {
        if m == Menu::Branch {
            return div()
                .id("review-popup")
                .debug_selector(|| "review-popup".into())
                .absolute()
                .top(px(112.))
                .left(px(8.))
                .w(px(branch_picker::WIDTH))
                .max_w(px((self.panel_width - 16.).max(0.)))
                .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                    s.menu = None;
                    cx.notify();
                }))
                .child(self.branch_picker.clone());
        }
        let t = Theme::for_mode(self.mode);
        // ChatGPT sizes these menus with `menuBounded` (min 200px, max 320px):
        // the short comparison labels fit 200px, the diff controls need 220px.
        let width = if m == Menu::Jump || m == Menu::Commits {
            360.
        } else if m == Menu::Scope {
            200.
        } else {
            220.
        };
        let mut menu = div()
            .id("review-popup")
            .debug_selector(|| "review-popup".into())
            .role(Role::Menu)
            .absolute()
            .top(px(
                if matches!(m, Menu::CommitBranch | Menu::PullRequestBase) {
                    36.
                } else if m == Menu::Branch {
                    112.
                } else {
                    80.
                },
            ))
            .w(px(width))
            .max_w_full()
            // ChatGPT's review menus: a 4px inset, 20px corners, a half-pixel
            // hairline ring, and a 16px blur shadow offset 8px down.
            .p(px(4.))
            .rounded(px(20.))
            .bg(menu_surface(t))
            .border(px(0.5))
            .border_color(t.border)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(8.0), t.profile_menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .flex()
            .flex_col()
            .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                s.menu = None;
                cx.notify();
            }));
        menu = match m {
            Menu::Scope | Menu::Branch => menu.left(px(8.)),
            _ => menu.right(px(8.)),
        };
        if m == Menu::Jump {
            menu = menu.child(div().h(px(34.)).px(px(8.)).child(self.jump.clone()));
        }
        let actions = self.menu_entries(&m);
        let mut items = div()
            .id("review-menu-scroll")
            .max_h(px(360.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if actions.is_empty() {
            items = items.child(
                div()
                    .p(px(12.))
                    .text_color(t.text_tertiary)
                    .text_size(px(13.))
                    .child(if m == Menu::Commits {
                        crate::i18n::text("分支上暂无提交记录")
                    } else {
                        crate::i18n::text("没有匹配的文件")
                    }),
            );
        }
        for (i, entry) in actions.into_iter().enumerate() {
            let MenuEntry {
                label,
                action: a,
                leading,
                trailing,
                separator_before,
            } = entry;
            let key_a = a.clone();
            let selected = self.menu_keyboard_selected && self.menu_selected == i;
            if separator_before {
                items = items.child(menu_separator(t));
            }
            items = items.child(
                div()
                    .id(("review-menu-item", i))
                    .debug_selector(move || format!("review-menu-item-{i}"))
                    .role(Role::MenuItem)
                    .aria_label(label.clone())
                    .aria_selected(selected)
                    .focusable()
                    .tab_stop(true)
                    .min_h(px(28.5625))
                    .px(px(8.))
                    // ChatGPT's rows are 28.5625px: 13px text on an 18.5625px
                    // line box plus 5px of padding. GPUI's text layout
                    // quantizes that line box, so a row lands up to 0.2px
                    // taller; the geometry test pins the totals to 1px.
                    .py(px(5.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(15.))
                    .text_size(px(13.))
                    .line_height(px(18.5625))
                    .text_color(t.text)
                    .cursor_pointer()
                    .when(selected, |b| b.bg(t.sidebar_hover))
                    .hover(move |b| b.bg(t.sidebar_hover))
                    .on_click(cx.listener(move |s, _, w, cx| {
                        cx.stop_propagation();
                        s.action(a.clone(), w, cx);
                    }))
                    .on_key_down(cx.listener(move |s, e: &KeyDownEvent, w, cx| {
                        if e.keystroke.key == "enter" || e.keystroke.key == "space" {
                            cx.stop_propagation();
                            s.action(key_a.clone(), w, cx);
                        }
                    }))
                    .when_some(leading, |row, glyph| {
                        row.child(
                            icon(glyph, t.text.into())
                                .size(px(16.))
                                .opacity(0.75)
                                .flex_none(),
                        )
                    })
                    .child(
                        // The reference right-aligns a trailing glyph and lets
                        // the label fill the row; without one the label keeps
                        // its natural width.
                        div()
                            .when(trailing.is_some(), |d| d.flex_1().min_w(px(0.)))
                            .truncate()
                            .child(label),
                    )
                    .when_some(trailing, |row, glyph| {
                        // The submenu chevron is tertiary; the tick that marks
                        // the active source uses the body colour, as in the app.
                        let color = if glyph == "review-chevron-right" {
                            t.text_tertiary
                        } else {
                            t.text
                        };
                        row.child(
                            icon(glyph, color.into())
                                .size(px(16.))
                                .opacity(0.75)
                                .flex_none(),
                        )
                    }),
            );
        }
        menu.child(items)
    }

    pub(super) fn toolbar(&self, cx: &Context<Self>) -> Div {
        let t = Theme::for_mode(self.mode);
        let (adds, dels) = self.counts();
        div()
            .w_full()
            .min_h(px(40.))
            .flex_none()
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(3.))
            .border_b_1()
            .border_color(t.border)
            .child(
                self.dropdown_button("review-scope", self.scope.label(), Menu::Scope, cx)
                    .text_size(px(14.)),
            )
            .child(div().flex().gap(px(4.)).text_size(px(13.)).when(
                adds + dels > 0 && self.panel_width > 440.,
                |d| {
                    d.child(
                        div()
                            .text_color(addition_color(self.mode))
                            .child(format!("+{adds}")),
                    )
                    .child(
                        div()
                            .text_color(deletion_color(self.mode))
                            .child(format!("-{dels}")),
                    )
                },
            ))
            .child(div().flex_1().min_w(px(0.)))
            .child(self.button(
                "review-options",
                crate::i18n::text("查看选项"),
                Some("review-options"),
                Action::Menu(Menu::View),
                cx,
            ))
            .child(self.button(
                "review-collapse",
                if self.collapsed.len() == self.snapshot.files.len() {
                    crate::i18n::text("展开全部差异")
                } else {
                    crate::i18n::text("折叠全部差异")
                },
                Some("review-collapse"),
                Action::Collapse,
                cx,
            ))
            .child(self.button(
                "review-jump",
                crate::i18n::text("跳转到文件"),
                Some("review-jump"),
                Action::Menu(Menu::Jump),
                cx,
            ))
            .child(
                self.button(
                    "review-split",
                    if self.split {
                        crate::i18n::text("切换到统一差异视图")
                    } else {
                        crate::i18n::text("切换到拆分差异视图")
                    },
                    Some("review-split"),
                    Action::Split,
                    cx,
                )
                .when(self.split, |b| b.bg(t.sidebar_hover)),
            )
            .when(!self.compact(), |d| {
                d.child(
                    self.button(
                        "review-tree",
                        if self.tree_open {
                            crate::i18n::text("隐藏文件")
                        } else {
                            crate::i18n::text("显示文件")
                        },
                        Some("review-tree"),
                        Action::Tree,
                        cx,
                    )
                    .when(self.tree_open, |b| b.bg(t.sidebar_hover)),
                )
            })
            .child(
                self.button(
                    "review-git",
                    crate::i18n::text("提交或推送"),
                    Some("review-commit"),
                    Action::Commit,
                    cx,
                )
                .border_1()
                .border_color(t.border),
            )
            .child(self.button(
                "review-more-git",
                crate::i18n::text("更多 Git 操作"),
                Some("chevron-down"),
                Action::Menu(Menu::Git),
                cx,
            ))
    }
    pub(super) fn counts(&self) -> (usize, usize) {
        self.snapshot
            .files
            .iter()
            .fold((0, 0), |(a, d), f| (a + f.additions, d + f.deletions))
    }
}

pub(super) struct ReviewTooltip(pub(super) SharedString);
impl Render for ReviewTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(5.))
            .rounded(px(6.))
            .bg(rgba(0x333333ff))
            .text_color(rgba(0xffffffff))
            .text_size(px(12.))
            .child(self.0.clone())
    }
}
