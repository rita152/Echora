use super::render::{addition_color, deletion_color};
use super::*;
use crate::{
    components::{icons::icon, viewer_header},
    theme::Theme,
};
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
    Jump(usize),
    Toggle(usize),
    Copy(String),
    Open(usize),
    OpenExternal(usize),
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

/// Header menus open at the bottom of their 32px capsule: the header's 8px
/// padding and the capsule.
const HEADER_MENU_TOP: f32 = 8. + 32.;

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
        if matches!(&a,Action::Open(i)|Action::OpenExternal(i)|Action::Viewed(i)|Action::Toggle(i)|Action::Jump(i)|Action::Context(i)|Action::Menu(Menu::File(i)) if *i>=self.snapshot.files.len())
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
            Action::Jump(i) => self.jump_to(i, cx),
            Action::Toggle(i) => {
                self.menu = None;
                self.toggle_file(i, cx)
            }
            Action::Copy(s) => {
                cx.write_to_clipboard(ClipboardItem::new_string(s));
                self.menu = None;
                self.show_notice(crate::i18n::text("已复制").into(), cx);
            }
            Action::Viewed(i) => {
                self.menu = None;
                let f = &self.snapshot.files[i];
                if self.viewed.remove(&f.path).is_some() {
                    self.collapsed.remove(&f.path);
                } else {
                    self.viewed.insert(f.path.clone(), f.patch.clone());
                    self.collapsed.insert(f.path.clone());
                }
                self.rebuild(cx);
            }
            Action::Open(i) => {
                let file = &self.snapshot.files[i];
                cx.emit(ReviewEvent::OpenFile {
                    path: self.snapshot.root.join(&file.path).to_string_lossy().into(),
                    line: None,
                });
                self.menu = None;
            }
            Action::OpenExternal(i) => {
                cx.open_with_system(&self.snapshot.root.join(&self.snapshot.files[i].path));
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

    /// The shared hit target of every review control: its accessible name,
    /// focus ring, tooltip, and mouse and keyboard activation.
    pub(super) fn control(
        &self,
        id: impl Into<SharedString>,
        label: SharedString,
        a: Action,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let hint = label.clone();
        let key_action = a.clone();
        div()
            .id(id.into())
            .role(Role::Button)
            .aria_label(label)
            .focusable()
            .tab_stop(true)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
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
        self.control(id, label.clone(), a, cx)
            .h(px(28.))
            // Callers also use 20px icon buttons. Padding must not squeeze
            // their 16px glyphs; keep the default toolbar hit area at 28px.
            .px(px(if glyph.is_some() { 0. } else { 8. }))
            .when(glyph.is_some(), |b| b.w(px(28.)))
            .gap(px(4.))
            .rounded(px(12.5))
            .text_size(px(13.))
            .line_height(px(18.))
            .text_color(t.text_secondary)
            .hover(move |s| s.bg(t.sidebar_hover).text_color(t.text))
            .when_some(glyph, |b, g| {
                b.child(icon(g, t.text_secondary.into()).size(px(16.)).flex_none())
            })
            .when(glyph.is_none(), |b| b.child(label))
    }

    /// A 28px round control inside a header capsule: a tertiary glyph, the
    /// ghost hover, and the accent wash while its option is on or its menu
    /// is open.
    pub(super) fn capsule_button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        glyph: impl IntoElement,
        pressed: bool,
        a: Action,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let (_, pressed_bg) = viewer_header::pressed(self.mode);
        let open = matches!(&a, Action::Menu(menu) if self.menu.as_ref() == Some(menu));
        self.control(id, label.into(), a, cx)
            .size(px(viewer_header::CONTROL_SIZE))
            .rounded_full()
            .when(pressed, |b| b.bg(pressed_bg))
            .when(!pressed, |b| {
                b.when(open, |b| b.bg(t.sidebar_hover))
                    .hover(move |s| s.bg(t.sidebar_hover))
            })
            .child(glyph)
    }

    /// The glyph of a capsule control, in the pressed accent when on.
    pub(super) fn capsule_glyph(&self, name: &'static str, pressed: bool) -> gpui::Svg {
        let t = Theme::for_mode(self.mode);
        let (accent, _) = viewer_header::pressed(self.mode);
        icon(name, if pressed { accent } else { t.text_tertiary }.into())
            .size(px(16.))
            .flex_none()
    }

    /// The comparison source trigger: `px-3 pe-1.5` 13px text and its chevron.
    fn source_button(
        &self,
        id: &'static str,
        label: String,
        menu: Menu,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let open = self.menu.as_ref() == Some(&menu);
        self.control(id, label.clone().into(), Action::Menu(menu), cx)
            .debug_selector(move || id.to_owned())
            .h(px(28.))
            .min_w(px(0.))
            .pl(px(12.))
            .pr(px(6.))
            .gap(px(4.))
            .rounded_full()
            .text_size(px(13.))
            .line_height(px(20.))
            .text_color(t.text)
            .when(open, |b| b.bg(t.sidebar_hover))
            .hover(move |s| s.bg(t.sidebar_hover))
            .child(div().min_w(px(0.)).truncate().child(label))
            .child(
                icon("chevron-down", t.text.into())
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
            // The reference's `Changes options`. A narrow header folds its
            // Refresh, Word wrap, diff layout and Collapse controls in here.
            // Echora keeps its commit and pull request flows below them; the
            // reference reaches those from the task summary instead.
            Menu::View => {
                let check = |on: bool, entry: MenuEntry| {
                    if on { entry.trailing("check") } else { entry }
                };
                let mut entries = Vec::new();
                if !self.wide_header() {
                    entries.extend([
                        MenuEntry::new(crate::i18n::text("刷新"), Action::Refresh)
                            .leading("review-refresh"),
                        check(
                            self.wrap,
                            MenuEntry::new(crate::i18n::text("自动换行"), Action::Wrap)
                                .leading("review-word-wrap"),
                        ),
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
                            if self.all_collapsed() {
                                crate::i18n::text("展开全部差异")
                            } else {
                                crate::i18n::text("折叠全部差异")
                            },
                            Action::Collapse,
                        )
                        .leading("review-collapse-all"),
                    ]);
                }
                let options = [
                    check(
                        self.load_files,
                        MenuEntry::new(crate::i18n::text("加载完整文件"), Action::LoadFiles)
                            .leading("review-load-full-files"),
                    ),
                    check(
                        self.rich,
                        MenuEntry::new(crate::i18n::text("渲染预览"), Action::Rich)
                            .leading("review-rich-preview"),
                    ),
                    check(
                        self.words,
                        MenuEntry::new(crate::i18n::text("词级差异"), Action::Words)
                            .leading("browser-tool-review"),
                    ),
                    check(
                        self.whitespace,
                        MenuEntry::new(crate::i18n::text("隐藏空白字符"), Action::Whitespace)
                            .leading("review-white-space"),
                    ),
                    MenuEntry::new(crate::i18n::text("复制 git apply 命令"), Action::CopyPatch)
                        .leading("review-copy-apply"),
                ];
                let separated = !entries.is_empty();
                for (index, entry) in options.into_iter().enumerate() {
                    entries.push(if index == 0 && separated {
                        entry.separated()
                    } else {
                        entry
                    });
                }
                entries.push(
                    MenuEntry::new(crate::i18n::text("提交或推送"), Action::Commit)
                        .leading("review-commit")
                        .separated(),
                );
                entries.push(
                    MenuEntry::new(crate::i18n::text("创建 Pull Request"), Action::PullRequest)
                        .leading("pr-open-browser"),
                );
                if self.scope.editable() && !self.snapshot.files.is_empty() {
                    let staged = self.scope == Scope::Staged;
                    entries.push(
                        MenuEntry::new(
                            if staged {
                                crate::i18n::text("对全部取消暂存")
                            } else {
                                crate::i18n::text("暂存全部")
                            },
                            Action::Mutation(if staged {
                                Mutation::Unstage(None)
                            } else {
                                Mutation::Stage(None)
                            }),
                        )
                        .leading(if staged {
                            "review-minus"
                        } else {
                            "review-plus"
                        }),
                    );
                    entries.push(
                        MenuEntry::new(
                            crate::i18n::text("还原全部"),
                            Action::Confirm(Mutation::DiscardAll),
                        )
                        .leading("review-restore"),
                    );
                }
                entries
            }
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
            // The reference's `File actions`; staging and reverting a file
            // stay below them for the scopes that allow it.
            Menu::File(i) => {
                let file = &self.snapshot.files[*i];
                let mut a = vec![
                    MenuEntry::new(
                        crate::i18n::text("复制路径"),
                        Action::Copy(file.path.clone()),
                    ),
                    MenuEntry::new(crate::i18n::text("在标签页中打开文件"), Action::Open(*i)),
                    MenuEntry::new(
                        if self.collapsed.contains(&file.path) {
                            crate::i18n::text("展开文件")
                        } else {
                            crate::i18n::text("折叠文件")
                        },
                        Action::Toggle(*i),
                    ),
                ];
                if matches!(self.scope, Scope::Branch(_) | Scope::Commit(_)) {
                    a.push(MenuEntry::new(
                        if self.viewed.get(&file.path) == Some(&file.patch) {
                            crate::i18n::text("标记为未查看")
                        } else {
                            crate::i18n::text("标记为已查看")
                        },
                        Action::Viewed(*i),
                    ));
                }
                if self.scope.editable() {
                    a.push(
                        MenuEntry::new(
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
                        )
                        .separated(),
                    );
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

    pub(super) fn popup(&self, m: Menu, cx: &Context<Self>) -> gpui::AnyElement {
        if m == Menu::Branch {
            return div()
                .id("review-popup")
                .debug_selector(|| "review-popup".into())
                .absolute()
                .top(px(HEADER_MENU_TOP))
                .left(px(8.))
                .w(px(branch_picker::WIDTH))
                .max_w(px((self.panel_width - 16.).max(0.)))
                .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                    s.menu = None;
                    cx.notify();
                }))
                .child(self.branch_picker.clone())
                .into_any_element();
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
                } else {
                    HEADER_MENU_TOP
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
            Menu::Scope | Menu::Branch | Menu::Commits => menu.left(px(8.)),
            Menu::View | Menu::Jump => menu.right(px(self.header_menu_right(&m))),
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
        let menu = menu.child(items);
        // A file's menu opens 4px under its `File actions` button, right
        // aligned with it, wherever the header is in the list.
        if let Menu::File(i) = m
            && let Some(bounds) = self.menu_anchors.borrow().get(&i).copied()
        {
            let mut menu = menu;
            menu.style().position = None;
            menu.style().inset.top = None;
            menu.style().inset.right = None;
            return gpui::deferred(
                gpui::anchored()
                    .anchor(gpui::Anchor::TopRight)
                    .position(bounds.bottom_right() + gpui::point(px(0.), px(4.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            )
            .with_priority(2)
            .into_any_element();
        }
        menu.into_any_element()
    }

    /// The Changes header (`review-header`): the comparison source and its
    /// totals in one capsule, with the base branch or commit beside it, and
    /// the diff controls in another at the end.
    pub(super) fn toolbar(&self, cx: &Context<Self>) -> Div {
        let t = Theme::for_mode(self.mode);
        let (adds, dels) = self.counts();
        let source = viewer_header::capsule(self.mode)
            .min_w(px(0.))
            .child(self.source_button(
                "review-scope",
                self.scope.label().to_owned(),
                Menu::Scope,
                cx,
            ))
            .when(adds + dels > 0, |capsule| {
                capsule.child(
                    div()
                        .flex_none()
                        .mr(px(4.))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .text_size(px(14.))
                        .line_height(px(14.))
                        .child(
                            div()
                                .text_color(addition_color(self.mode))
                                .child(format!("+{adds}")),
                        )
                        .child(
                            div()
                                .text_color(deletion_color(self.mode))
                                .child(format!("-{dels}")),
                        ),
                )
            });
        let details = match &self.scope {
            Scope::Branch(base) => Some((
                crate::i18n::format!(
                    "{} → {base}" => "{} → {base}",
                    self.snapshot.branch
                ),
                Menu::Branch,
            )),
            Scope::Commit(sha) => Some((sha[..8.min(sha.len())].to_owned(), Menu::Commits)),
            _ => None,
        };
        let wide = self.wide_header();
        let collapse_label = if self.all_collapsed() {
            crate::i18n::text("展开全部差异")
        } else {
            crate::i18n::text("折叠全部差异")
        };
        let layout = self.layout_glyph();
        let controls = viewer_header::capsule(self.mode)
            .child(self.capsule_button(
                "review-options",
                crate::i18n::text("“变更”选项"),
                self.capsule_glyph("review-options", false),
                false,
                Action::Menu(Menu::View),
                cx,
            ))
            .child(self.capsule_button(
                "review-jump",
                crate::i18n::text("跳转到文件"),
                self.capsule_glyph("review-jump", false),
                false,
                Action::Menu(Menu::Jump),
                cx,
            ))
            .when(wide, |capsule| {
                capsule.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(self.capsule_button(
                            "review-refresh",
                            crate::i18n::text("刷新"),
                            self.capsule_glyph("review-refresh", false),
                            false,
                            Action::Refresh,
                            cx,
                        ))
                        .child(self.capsule_button(
                            "review-wrap",
                            crate::i18n::text("自动换行"),
                            self.capsule_glyph("review-word-wrap", self.wrap),
                            self.wrap,
                            Action::Wrap,
                            cx,
                        ))
                        .child(self.capsule_button(
                            "review-collapse",
                            collapse_label,
                            self.capsule_glyph("review-collapse", false),
                            false,
                            Action::Collapse,
                            cx,
                        ))
                        .child(self.capsule_button(
                            "review-split",
                            if self.split {
                                crate::i18n::text("切换到统一差异视图")
                            } else {
                                crate::i18n::text("切换到拆分差异视图")
                            },
                            layout,
                            false,
                            Action::Split,
                            cx,
                        )),
                )
            })
            .when(!self.compact(), |capsule| {
                capsule.child(self.capsule_button(
                    "review-tree",
                    if self.tree_open {
                        crate::i18n::text("隐藏文件")
                    } else {
                        crate::i18n::text("显示文件")
                    },
                    self.capsule_glyph("pr-file-tree", self.tree_open),
                    self.tree_open,
                    Action::Tree,
                    cx,
                ))
            });
        div()
            .w_full()
            .h(px(viewer_header::HEIGHT + 1.))
            .flex_none()
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(t.border)
            .child(
                div()
                    .min_w(px(0.))
                    .flex_shrink(1.)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(source)
                    .when_some(details, |row, (label, menu)| {
                        row.child(
                            viewer_header::capsule(self.mode)
                                .min_w(px(0.))
                                .child(self.source_button("review-base", label, menu, cx)),
                        )
                    }),
            )
            .child(div().flex_1().min_w(px(0.)))
            .child(controls)
    }

    /// The diff layout glyph: the frame with the current layout's red and
    /// green rows, as the reference draws `Switch to split diff`.
    fn layout_glyph(&self) -> Div {
        let t = Theme::for_mode(self.mode);
        let (deleted, added) = if self.split {
            ("pr-view-split-deleted", "pr-view-split-added")
        } else {
            ("pr-view-unified-deleted", "pr-view-unified-added")
        };
        let layer = |name: &'static str, color: gpui::Rgba| {
            icon(name, color.into())
                .absolute()
                .top_0()
                .left_0()
                .size(px(16.))
        };
        div()
            .relative()
            .flex_none()
            .size(px(16.))
            .child(layer("pr-view-frame", t.text_tertiary))
            .child(layer(deleted, gpui::rgb(0xf84e63)))
            .child(layer(added, gpui::rgb(0x36d958)))
    }

    /// How far the right edge of `m`'s trigger sits from the panel's right
    /// edge: the header's 8px padding, the capsule's 2px inset, and the 28px
    /// controls (2px apart, 6px inside the wide group) after the trigger.
    fn header_menu_right(&self, m: &Menu) -> f32 {
        let tree = if self.compact() { 0. } else { 30. };
        let group = if self.wide_header() { 132. } else { 0. };
        let jump = if *m == Menu::View { 30. } else { 0. };
        8. + 2. + tree + group + jump
    }

    pub(super) fn wide_header(&self) -> bool {
        self.panel_width >= viewer_header::WIDE_MIN_WIDTH
    }

    pub(super) fn all_collapsed(&self) -> bool {
        !self.snapshot.files.is_empty() && self.collapsed.len() == self.snapshot.files.len()
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
