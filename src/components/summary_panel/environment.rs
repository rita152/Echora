//! Synara's Environment island and its anchored, keyboard-operable menus.
use super::*;
use crate::{
    components::prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
    git_review::environment::Environment,
    theme::Theme,
};
use gpui::{Anchor, AppContext, Focusable, ScrollHandle};
use std::{cell::Cell, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Menu {
    Location,
    Branch,
    Git,
    Servers,
    Editor,
    CreateBranch,
}

pub(super) struct EnvironmentUi {
    data: Option<Environment>,
    cwd: Option<PathBuf>,
    loading: bool,
    busy: bool,
    generation: u64,
    refreshed: Option<Instant>,
    error: Option<String>,
    operation_error: Option<String>,
    return_focus_pending: bool,
    menu: Option<Menu>,
    return_row: usize,
    selected: usize,
    keyboard: bool,
    focus: FocusHandle,
    input: Entity<PromptInput>,
    focus_input: bool,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    row_bounds: Rc<RefCell<HashMap<usize, Bounds<Pixels>>>>,
    scroll: ScrollHandle,
}

impl EnvironmentUi {
    pub(super) fn new(mode: ThemeMode, cx: &mut Context<SummaryPanel>) -> Self {
        let input = cx.new(|cx| PromptInput::chat_search(mode, "Search branches".to_owned(), cx));
        cx.subscribe(&input, |this, _, _: &PromptChanged, cx| {
            this.environment.selected = 0;
            this.environment.scroll.set_offset(point(px(0.), px(0.)));
            cx.notify();
        })
        .detach();
        cx.subscribe(&input, |this, _, event: &PromptSubmitted, cx| {
            match this.environment.menu {
                Some(Menu::CreateBranch) => {
                    let name = event.0.trim();
                    if !name.is_empty() && name != "codex/" {
                        this.switch_environment_branch(name.to_owned(), true, cx);
                    }
                }
                Some(Menu::Branch) => {
                    if let Some((_, choice, true)) = this.choices(cx).get(this.environment.selected)
                    {
                        match choice.clone() {
                            Choice::Branch(branch) => {
                                this.switch_environment_branch(branch, false, cx)
                            }
                            Choice::CreateBranch => this.begin_environment_branch_creation(cx),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        })
        .detach();
        Self {
            data: None,
            cwd: None,
            loading: false,
            busy: false,
            generation: 0,
            refreshed: None,
            error: None,
            operation_error: None,
            return_focus_pending: false,
            menu: None,
            return_row: 0,
            selected: 0,
            keyboard: false,
            focus: cx.focus_handle(),
            input,
            focus_input: false,
            bounds: Rc::default(),
            row_bounds: Rc::default(),
            scroll: ScrollHandle::new(),
        }
    }
}

#[derive(Clone)]
enum Choice {
    Local,
    Worktree,
    Branch(String),
    CreateBranch,
    Commit,
    CommitPush,
    Pull,
    Push,
    PullRequest,
    Editor(String),
    Server(u16),
    Refresh,
}

impl SummaryPanel {
    pub fn return_environment_focus(&mut self, cx: &mut Context<Self>) {
        self.environment.return_row = 3;
        self.environment.return_focus_pending = true;
        cx.notify();
    }

    pub fn focus_environment(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.environment_focus(0, cx).focus(window, cx);
    }

    fn environment_focus(&self, index: usize, cx: &mut Context<Self>) -> FocusHandle {
        self.row_focus
            .borrow_mut()
            .entry(format!("environment-{index}").into())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }

    pub fn has_environment_menu(&self) -> bool {
        self.environment.menu.is_some()
    }

    pub fn dismiss_environment_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.environment.busy && self.environment.menu == Some(Menu::CreateBranch) {
            return true;
        }
        if self.environment.menu.take().is_none() {
            return false;
        }
        self.environment_focus(self.environment.return_row, cx)
            .focus(window, cx);
        cx.notify();
        true
    }

    pub fn reset_environment(&mut self, cx: &mut Context<Self>) {
        self.environment.generation += 1;
        self.environment.cwd = None;
        self.environment.data = None;
        self.environment.loading = false;
        self.environment.busy = false;
        self.environment.refreshed = None;
        self.environment.menu = None;
        self.environment.error = None;
        self.environment.operation_error = None;
        self.environment.return_focus_pending = false;
        cx.notify();
    }

    fn refresh_environment(&mut self, force: bool, cx: &mut Context<Self>) {
        let cwd = self.composer.as_ref().and_then(|c| {
            let (cwd, project, _) = c.read(cx).environment_context();
            project.then_some(cwd)
        });
        if self.environment.cwd != cwd {
            self.reset_environment(cx);
            self.environment.cwd = cwd.clone();
        }
        let Some(cwd) = cwd else {
            return;
        };
        if self.environment.loading
            || self.environment.busy
            || (!force
                && self
                    .environment
                    .refreshed
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(5)))
        {
            return;
        }
        self.environment.loading = true;
        let generation = self.environment.generation;
        cx.spawn(async move |this, cx| {
            let lookup = cwd.clone();
            let result = cx
                .background_executor()
                .spawn(async move { crate::git_review::environment::read(&lookup) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.environment.generation != generation
                    || this.environment.cwd.as_ref() != Some(&cwd)
                {
                    return;
                }
                this.environment.loading = false;
                this.environment.refreshed = Some(Instant::now());
                match result {
                    Ok(data) => {
                        this.environment.data = Some(data);
                        this.environment.error = None;
                    }
                    Err(error) => {
                        this.environment.data = None;
                        this.environment.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    fn environment_row(
        &self,
        index: usize,
        glyph: &'static str,
        content: (String, String),
        action: (bool, Option<Menu>),
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (label, trailing) = content;
        let (enabled, menu) = action;
        let focus = self.environment_focus(index, cx);
        let bounds = self.environment.row_bounds.clone();
        let chevron = menu.is_some();
        div()
            .id(("environment-row", index))
            .debug_selector(move || format!("environment-row-{index}"))
            .role(Role::Button)
            .aria_label(label.clone())
            .aria_description(if enabled { "" } else { "Unavailable" })
            .track_focus(&focus)
            .tab_stop(enabled)
            .h(px(28.))
            .w_full()
            .flex_none()
            .px(px(8.))
            .rounded(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .line_height(px(20.))
            .text_color(theme.text)
            .when(!enabled, |row| row.opacity(0.5))
            .when(enabled, |row| {
                row.cursor_pointer()
                    .hover(move |s| s.bg(theme.text.alpha(0.04)))
                    .active(move |s| s.bg(theme.text.alpha(0.08)))
            })
            .when(
                self.focused_row
                    .as_ref()
                    .is_some_and(|id| id.as_ref() == format!("environment-{index}"))
                    && self.focus_ring,
                |row| row.bg(theme.text.alpha(0.06)),
            )
            .child(
                canvas(
                    move |b, _, _| {
                        bounds.borrow_mut().insert(index, b);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_click(cx.listener(move |this, _, w, cx| {
                if enabled {
                    this.environment_activate_row(index, menu, w, cx);
                }
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, w, cx| {
                if enabled && matches!(e.keystroke.key.as_str(), "enter" | "space") {
                    this.environment_activate_row(index, menu, w, cx);
                    cx.stop_propagation();
                }
            }))
            .child(icon(glyph, theme.text.into()).size(px(16.)).flex_none())
            .when(index != 8, |row| {
                row.child(div().flex_1().min_w(px(0.)).text_ellipsis().child(label))
            })
            .when(!trailing.is_empty(), |row| {
                if index == 5 {
                    row.child(icon("settings-external", theme.text.into()).size(px(16.)))
                } else if index == 0 {
                    let (added, deleted) = trailing.split_once(' ').unwrap_or((&trailing, ""));
                    let dark = self.mode == ThemeMode::Dark;
                    row.child(
                        div()
                            .flex()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_color(gpui::rgba(if dark {
                                        0x40c977ff
                                    } else {
                                        0x00a240ff
                                    }))
                                    .child(added.to_owned()),
                            )
                            .child(
                                div()
                                    .text_color(gpui::rgba(if dark {
                                        0xfa423eff
                                    } else {
                                        0xe02e2aff
                                    }))
                                    .child(deleted.to_owned()),
                            ),
                    )
                } else {
                    row.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .when(index == 4 && trailing != "0", |badge| {
                                badge.child(
                                    div().size(px(6.)).rounded_full().bg(gpui::rgba(0x40c977ff)),
                                )
                            })
                            .child(div().text_color(theme.text_secondary).child(trailing)),
                    )
                }
            })
            .when(chevron, |row| {
                row.child(icon("chevron-down", theme.text_secondary.into()).size(px(12.)))
            })
    }

    fn environment_activate_row(
        &mut self,
        index: usize,
        menu: Option<Menu>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.environment_focus(index, cx).focus(w, cx);
        if let Some(menu) = menu {
            if self.environment.menu == Some(menu) {
                self.dismiss_environment_menu(w, cx);
                return;
            }
            self.environment.menu = Some(menu);
            self.environment.return_row = index;
            self.environment.selected = 0;
            self.environment.keyboard = false;
            self.environment.error = None;
            self.environment
                .bounds
                .set(self.environment.row_bounds.borrow().get(&index).copied());
            self.environment.scroll.set_offset(point(px(0.), px(0.)));
            if menu == Menu::Branch {
                self.environment.input.update(cx, |i, cx| {
                    i.set_placeholder("Search branches", cx);
                    i.set_accessible_name("Search branches");
                    i.set_text_silently("", cx);
                });
                self.environment.focus_input = true;
            } else {
                self.environment.focus.focus(w, cx);
            }
            self.refresh_environment(true, cx);
        } else {
            match index {
                0 => cx.emit(SummaryPanelEvent::OpenReview),
                5 => {
                    if let Some(repository) = self
                        .environment
                        .data
                        .as_ref()
                        .and_then(|d| d.repository.as_ref())
                    {
                        cx.open_url(&format!("https://github.com/{repository}"));
                    }
                }
                6 => cx.emit(SummaryPanelEvent::OpenEditorView),
                8 => cx.emit(SummaryPanelEvent::EnvironmentSettings),
                _ => {}
            }
        }
        cx.notify();
    }

    fn choices(&self, cx: &gpui::App) -> Vec<(String, Choice, bool)> {
        let data = self.environment.data.as_ref();
        let enabled = !self.environment.busy;
        match self.environment.menu {
            Some(Menu::Location) => {
                let draft = self
                    .composer
                    .as_ref()
                    .is_some_and(|c| c.read(cx).thread_id().is_none());
                vec![
                    ("Local project".into(), Choice::Local, enabled && draft),
                    (
                        "New worktree".into(),
                        Choice::Worktree,
                        enabled && draft && data.is_some(),
                    ),
                ]
            }
            Some(Menu::Branch) => {
                let query = self.environment.input.read(cx).text().trim().to_lowercase();
                let mut choices: Vec<_> = data
                    .into_iter()
                    .flat_map(|d| &d.branches)
                    .filter(|b| b.to_lowercase().contains(&query))
                    .map(|b| (b.clone(), Choice::Branch(b.clone()), enabled))
                    .collect();
                choices.push(("Create Branch".into(), Choice::CreateBranch, enabled));
                choices
            }
            Some(Menu::Git) => {
                let dirty = data.is_some_and(|d| d.changed > 0);
                let upstream = data.is_some_and(|d| d.upstream);
                vec![
                    ("Commit".into(), Choice::Commit, enabled && dirty),
                    (
                        "Commit & push".into(),
                        Choice::CommitPush,
                        enabled
                            && data.is_some_and(|d| {
                                d.changed > 0 || d.ahead > 0 || (!d.upstream && d.branch.is_some())
                            }),
                    ),
                    ("Pull".into(), Choice::Pull, enabled && upstream),
                    (
                        "Push".into(),
                        Choice::Push,
                        enabled
                            && data.is_some_and(|d| {
                                d.ahead > 0 || (!d.upstream && d.branch.is_some())
                            }),
                    ),
                    (
                        "Create PR".into(),
                        Choice::PullRequest,
                        enabled && data.is_some_and(|d| d.can_create_pr),
                    ),
                    ("Create Branch".into(), Choice::CreateBranch, enabled),
                ]
            }
            Some(Menu::Servers) => {
                let mut choices: Vec<_> = data
                    .into_iter()
                    .flat_map(|d| &d.servers)
                    .map(|&p| (format!("http://localhost:{p}"), Choice::Server(p), true))
                    .collect();
                choices.push((
                    "Refresh local servers".into(),
                    Choice::Refresh,
                    !self.environment.loading,
                ));
                choices
            }
            Some(Menu::Editor) => available_editors()
                .into_iter()
                .map(|editor| (editor.to_owned(), Choice::Editor(editor.to_owned()), true))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn begin_environment_branch_creation(&mut self, cx: &mut Context<Self>) {
        self.environment.menu = Some(Menu::CreateBranch);
        self.environment.operation_error = None;
        self.environment.input.update(cx, |i, cx| {
            i.set_placeholder("feature/my-change", cx);
            i.set_accessible_name("Branch name");
            i.set_text_silently("", cx);
        });
        self.environment.focus_input = true;
        cx.notify();
    }

    fn choose_environment(&mut self, choice: Choice, w: &mut Window, cx: &mut Context<Self>) {
        match choice {
            Choice::CreateBranch => self.begin_environment_branch_creation(cx),
            Choice::Branch(branch) => self.switch_environment_branch(branch, false, cx),
            Choice::Local | Choice::Worktree => {
                let worktree = matches!(choice, Choice::Worktree);
                if let Some(composer) = &self.composer {
                    composer.update(cx, |c, cx| c.select_environment_location(worktree, cx));
                }
                self.dismiss_environment_menu(w, cx);
            }
            Choice::Commit | Choice::CommitPush | Choice::PullRequest => {
                let action = match choice {
                    Choice::CommitPush => EnvironmentGitAction::CommitPush,
                    Choice::PullRequest => EnvironmentGitAction::PullRequest,
                    _ => EnvironmentGitAction::Commit,
                };
                self.dismiss_environment_menu(w, cx);
                cx.emit(SummaryPanelEvent::EnvironmentGit(action));
            }
            Choice::Pull => self.sync_environment_remote(false, cx),
            Choice::Push => self.sync_environment_remote(true, cx),
            Choice::Editor(editor) => {
                self.store.set_environment_editor(editor.clone());
                if let Some(cwd) = self.environment.cwd.clone() {
                    self.environment.busy = true;
                    let generation = self.environment.generation;
                    cx.spawn(async move |this, cx| {
                        let result = cx
                            .background_executor()
                            .spawn(async move {
                                let mut command = std::process::Command::new("open");
                                command.args(["-a", &editor]).arg(cwd);
                                crate::git_review::process::run(
                                    &mut command,
                                    None,
                                    Duration::from_secs(10),
                                )
                                .and_then(|o| {
                                    if o.status.success() {
                                        Ok(())
                                    } else {
                                        anyhow::bail!(
                                            String::from_utf8_lossy(&o.stderr).into_owned()
                                        )
                                    }
                                })
                            })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            if this.environment.generation != generation {
                                return;
                            }
                            this.environment.busy = false;
                            this.environment.operation_error = result.err().map(|e| e.to_string());
                            cx.notify();
                        });
                    })
                    .detach();
                }
                self.dismiss_environment_menu(w, cx);
            }
            Choice::Server(port) => {
                cx.open_url(&format!("http://localhost:{port}"));
                self.dismiss_environment_menu(w, cx);
            }
            Choice::Refresh => self.refresh_environment(true, cx),
        }
        cx.notify();
    }

    fn switch_environment_branch(&mut self, branch: String, create: bool, cx: &mut Context<Self>) {
        let Some(cwd) = self.environment.cwd.clone() else {
            return;
        };
        if self.environment.busy {
            return;
        }
        self.environment.busy = true;
        self.environment.operation_error = None;
        self.environment.generation += 1;
        self.environment.loading = false;
        let generation = self.environment.generation;
        cx.spawn(async move |this, cx| {
            let lookup = cwd.clone();
            let result = cx
                .background_executor()
                .spawn(
                    async move { crate::git_review::workspace::switch(&lookup, &branch, create) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.environment.generation != generation {
                    return;
                }
                this.environment.busy = false;
                match result {
                    Ok(checkout) => {
                        if let Some(composer) = &this.composer {
                            composer.update(cx, |c, cx| c.set_checkout(checkout, cx));
                        }
                        if let Some(thread) = this.thread_id.clone() {
                            this.store.sync_thread_git_branch(thread, cwd.clone());
                        }
                        this.store.refresh_checkout(&cwd);
                        this.environment.menu = None;
                        this.environment.return_focus_pending = true;
                        this.environment.refreshed = None;
                        this.refresh_environment(true, cx);
                    }
                    Err(e) => this.environment.operation_error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn sync_environment_remote(&mut self, push: bool, cx: &mut Context<Self>) {
        let Some(cwd) = self.environment.cwd.clone() else {
            return;
        };
        if self.environment.busy {
            return;
        }
        self.environment.busy = true;
        self.environment.operation_error = None;
        self.environment.generation += 1;
        self.environment.loading = false;
        let generation = self.environment.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if push {
                        let snapshot = crate::git_review::load(
                            &cwd,
                            &crate::git_review::Scope::Uncommitted,
                            false,
                            false,
                        )?;
                        crate::git_review::apply(
                            &snapshot,
                            &crate::git_review::Scope::Uncommitted,
                            &crate::git_review::Mutation::Push,
                        )
                        .map(|_| ())
                    } else {
                        crate::git_review::environment::pull(&cwd)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.environment.generation != generation {
                    return;
                }
                this.environment.busy = false;
                this.environment.operation_error = result.err().map(|e| e.to_string());
                this.environment.refreshed = None;
                this.refresh_environment(true, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn environment_branch_dialog(&mut self, w: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::for_mode(self.mode);
        let name = self.environment.input.read(cx).text().trim();
        let enabled = !self.environment.busy && !name.is_empty() && name != "codex/";
        let branch = self
            .environment
            .data
            .as_ref()
            .and_then(|d| d.branch.as_deref())
            .unwrap_or("HEAD");
        let button = |id: &'static str, label: &'static str, submit: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(if id == "environment-branch-close" {
                    "Close"
                } else {
                    label
                })
                .focusable()
                .tab_stop(!submit || enabled)
                .px(px(10.))
                .h(px(28.))
                .rounded(px(9.))
                .border_1()
                .border_color(theme.border)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .when(submit, |s| s.bg(theme.text).text_color(theme.surface))
                .when(submit && !enabled, |s| s.opacity(0.4))
                .hover(move |s| s.opacity(0.8))
                .on_click(cx.listener(move |this, _, w, cx| {
                    if submit && enabled {
                        let name = this.environment.input.read(cx).text().trim().to_owned();
                        this.switch_environment_branch(name, true, cx);
                    } else if !submit && !this.environment.busy {
                        this.dismiss_environment_menu(w, cx);
                    }
                }))
                .on_key_down(cx.listener(move |this, e: &KeyDownEvent, w, cx| {
                    if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                        if submit && enabled {
                            let name = this.environment.input.read(cx).text().trim().to_owned();
                            this.switch_environment_branch(name, true, cx);
                        } else if !submit && !this.environment.busy {
                            this.dismiss_environment_menu(w, cx);
                        }
                        cx.stop_propagation();
                    }
                }))
                .child(label)
        };
        let card = div()
            .id("environment-branch-dialog")
            .role(Role::Dialog)
            .aria_label("Create Branch")
            .w(px(448.))
            .max_w_full()
            .m(px(16.))
            .p(px(16.))
            .rounded(px(22.))
            .bg(theme.model_picker_surface)
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(8.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(div().flex_1().text_size(px(18.)).child("Create Branch"))
                    .child(button("environment-branch-close", "×", false)),
            )
            .child(
                div()
                    .text_color(theme.text_secondary)
                    .child(format!("Create and switch to a new branch from {branch}.")),
            )
            .child("Branch name")
            .child(
                div()
                    .h(px(32.))
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(9.))
                    .px(px(8.))
                    .child(self.environment.input.clone()),
            )
            .when_some(self.environment.operation_error.clone(), |s, error| {
                s.child(div().text_color(theme.text_secondary).child(error))
            })
            .child(
                div()
                    .mt(px(16.))
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(button("environment-branch-cancel", "Cancel", false))
                    .child(button(
                        "environment-branch-create",
                        if self.environment.busy {
                            "Creating…"
                        } else {
                            "Create and switch"
                        },
                        true,
                    )),
            );
        let overlay = div()
            .id("environment-branch-overlay")
            .w(w.viewport_size().width)
            .h(w.viewport_size().height)
            .bg(gpui::rgba(0x00000099))
            .text_size(px(13.))
            .text_color(theme.text)
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.environment.focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, w, cx| {
                    if !this.environment.busy {
                        this.dismiss_environment_menu(w, cx);
                    }
                }),
            )
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, w, cx| {
                match e.keystroke.key.as_str() {
                    "escape" if !this.environment.busy => {
                        this.dismiss_environment_menu(w, cx);
                    }
                    "enter" if enabled => {
                        let name = this.environment.input.read(cx).text().trim().to_owned();
                        this.switch_environment_branch(name, true, cx);
                    }
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(card);
        deferred(
            anchored()
                .anchor(Anchor::TopLeft)
                .position(point(px(0.), px(0.)))
                .child(overlay),
        )
        .with_priority(3)
        .into_any_element()
    }

    fn environment_menu(&mut self, w: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let kind = self.environment.menu?;
        let bounds = self.environment.bounds.get()?;
        let theme = Theme::for_mode(self.mode);
        if self.environment.focus_input {
            self.environment.focus_input = false;
            self.environment
                .input
                .read(cx)
                .focus_handle(cx)
                .focus(w, cx);
        }
        if kind == Menu::CreateBranch {
            return Some(self.environment_branch_dialog(w, cx));
        }
        let choices = self.choices(cx);
        let title = match kind {
            Menu::Location => "Work in",
            Menu::Branch => "Branches",
            Menu::Git => "Git actions",
            Menu::Servers => "Local Servers",
            Menu::Editor => "Open in",
            Menu::CreateBranch => "Create Branch",
        };
        let mut menu = div()
            .id("environment-menu")
            .debug_selector(|| "environment-menu".to_owned())
            .role(Role::Menu)
            .aria_label(title)
            .track_focus(&self.environment.focus)
            .w(px(if kind == Menu::Branch { 320. } else { 240. }))
            .max_h((w.viewport_size().height - px(80.)).max(px(100.)))
            .overflow_y_scroll()
            .p(px(4.))
            .rounded(px(16.))
            .bg(theme.model_picker_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(0.), theme.border.into()).spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.), px(6.), theme.text.alpha(0.10).into())
                    .blur_radius(px(18.)),
            ])
            .text_size(px(13.))
            .text_color(theme.text)
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(move |this, e: &gpui::MouseDownEvent, w, cx| {
                if !bounds.contains(&e.position) {
                    this.dismiss_environment_menu(w, cx);
                }
            }))
            .capture_key_down(cx.listener(|this, e: &KeyDownEvent, w, cx| {
                let choices = this.choices(cx);
                match e.keystroke.key.as_str() {
                    "escape" => {
                        this.dismiss_environment_menu(w, cx);
                    }
                    "down" | "up" | "tab" if !choices.is_empty() => {
                        let backwards = e.keystroke.key == "up"
                            || (e.keystroke.key == "tab" && e.keystroke.modifiers.shift);
                        let mut i = this.environment.selected;
                        for _ in 0..choices.len() {
                            i = if !this.environment.keyboard {
                                if backwards { choices.len() - 1 } else { 0 }
                            } else if backwards {
                                (i + choices.len() - 1) % choices.len()
                            } else {
                                (i + 1) % choices.len()
                            };
                            this.environment.keyboard = true;
                            if choices[i].2 {
                                break;
                            }
                        }
                        this.environment.selected = i;
                        this.environment.scroll.scroll_to_item(i);
                    }
                    "enter" | "space"
                        if e.keystroke.key == "enter"
                            || !matches!(
                                this.environment.menu,
                                Some(Menu::Branch | Menu::CreateBranch)
                            ) =>
                    {
                        if this.environment.menu == Some(Menu::CreateBranch) {
                            let name = this.environment.input.read(cx).text().trim().to_owned();
                            if !name.is_empty() && name != "codex/" {
                                this.switch_environment_branch(name, true, cx);
                            }
                        } else if let Some((_, choice, true)) =
                            choices.get(this.environment.selected)
                        {
                            this.choose_environment(choice.clone(), w, cx);
                        }
                    }
                    _ => return,
                }
                cx.notify();
                cx.stop_propagation();
            }))
            .when(kind != Menu::Branch, |menu| {
                menu.child(
                    div()
                        .px(px(8.))
                        .py(px(6.))
                        .text_color(theme.text_tertiary)
                        .child(title),
                )
            });
        if matches!(kind, Menu::Branch | Menu::CreateBranch) {
            menu = menu.child(
                div()
                    .h(px(30.))
                    .px(px(8.))
                    .child(self.environment.input.clone()),
            );
        }
        if kind == Menu::Servers
            && self
                .environment
                .data
                .as_ref()
                .is_none_or(|d| d.servers.is_empty())
        {
            menu = menu.child(
                div()
                    .p(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child("No servers running")
                    .child(
                        div()
                            .text_color(theme.text_tertiary)
                            .child("Local dev servers will appear here."),
                    ),
            );
        }
        let mut list = div()
            .id("environment-menu-items")
            .max_h(px(240.))
            .overflow_y_scroll()
            .track_scroll(&self.environment.scroll);
        let mut branch_footer = None;
        for (i, (mut label, choice, enabled)) in choices.into_iter().enumerate() {
            if kind == Menu::Branch && matches!(choice, Choice::CreateBranch) {
                label = "Create and checkout new branch...".into();
            }
            let selected = self.environment.keyboard && self.environment.selected == i;
            let checked = match &choice {
                Choice::Branch(b) => {
                    self.environment
                        .data
                        .as_ref()
                        .and_then(|d| d.branch.as_ref())
                        == Some(b)
                }
                Choice::Editor(e) => {
                    self.snapshot
                        .preferences
                        .environment_editor
                        .as_deref()
                        .unwrap_or_else(|| available_editors().first().copied().unwrap_or("Finder"))
                        == e
                }
                Choice::Local => !self
                    .composer
                    .as_ref()
                    .is_some_and(|c| c.read(cx).environment_context().2),
                Choice::Worktree => self
                    .composer
                    .as_ref()
                    .is_some_and(|c| c.read(cx).environment_context().2),
                _ => false,
            };
            let glyph = match &choice {
                Choice::Local => "project-local",
                Choice::Worktree => "project-worktree",
                Choice::Branch(_) | Choice::CreateBranch => "branch",
                Choice::Commit => "review-commit",
                Choice::CommitPush | Choice::Push => "environment-push",
                Choice::Pull => "environment-pull",
                Choice::PullRequest => "pr-status-open",
                Choice::Editor(editor) if editor == "Cursor" => "environment-cursor",
                Choice::Editor(_) => "environment-editor",
                Choice::Server(_) => "approval-link-globe",
                Choice::Refresh => "review-refresh",
            };
            let footer = kind == Menu::Branch && matches!(choice, Choice::CreateBranch);
            let branch_row = kind == Menu::Branch && !footer;
            let dirty = branch_row
                && checked
                && self
                    .environment
                    .data
                    .as_ref()
                    .is_some_and(|d| d.changed > 0);
            let detail = self.environment.data.as_ref().filter(|_| dirty).map(|d| {
                format!(
                    "Uncommitted: {} files  +{} −{}",
                    d.changed, d.additions, d.deletions
                )
            });
            let badge = if branch_row && checked {
                "current"
            } else if branch_row && matches!(label.as_str(), "main" | "master") {
                "default"
            } else {
                ""
            };
            let row = div()
                .id(("environment-menu-choice", i))
                .debug_selector(move || format!("environment-menu-choice-{i}"))
                .role(Role::MenuItem)
                .aria_label(label.clone())
                .aria_description(if enabled { "" } else { "Unavailable" })
                .h(px(if dirty {
                    44.
                } else if branch_row {
                    24.
                } else {
                    26.
                }))
                .px(px(8.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .when(!enabled, |s| s.opacity(0.4))
                .when(selected, |s| s.bg(theme.text.alpha(0.06)))
                .when(enabled, |s| {
                    s.cursor_pointer()
                        .hover(move |s| s.bg(theme.text.alpha(0.04)))
                })
                .on_click(cx.listener(move |this, _, w, cx| {
                    if enabled {
                        this.choose_environment(choice.clone(), w, cx);
                    }
                    cx.stop_propagation();
                }))
                .when(!branch_row, |s| {
                    s.child(
                        icon(if footer { "add" } else { glyph }, theme.text.into())
                            .size(px(14.))
                            .flex_none(),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(div().text_ellipsis().child(label))
                        .when_some(detail, |s, detail| {
                            s.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme.text_secondary)
                                    .child(detail),
                            )
                        }),
                )
                .when(!badge.is_empty(), |s| {
                    s.child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme.text_tertiary)
                            .child(badge),
                    )
                })
                .when(checked && !branch_row, |s| {
                    s.child(icon("check", theme.text.into()).size(px(14.)))
                });
            if footer {
                branch_footer = Some(row);
            } else {
                list = list.child(row);
            }
        }
        menu = menu.child(list).when_some(branch_footer, |menu, footer| {
            menu.child(environment_separator(theme)).child(footer)
        });
        if let Some(error) = self
            .environment
            .operation_error
            .clone()
            .or_else(|| self.environment.error.clone())
        {
            menu = menu.child(div().p(px(8.)).text_color(theme.text_tertiary).child(error));
        }
        if self.environment.busy {
            menu = menu.child(div().p(px(8.)).child("Working…"));
        }
        Some(
            deferred(
                anchored()
                    .anchor(Anchor::TopLeft)
                    .position(bounds.bottom_left() + point(px(0.), px(4.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }

    pub(super) fn environment_card(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.environment.return_focus_pending {
            self.environment.return_focus_pending = false;
            self.environment_focus(self.environment.return_row, cx)
                .focus(w, cx);
        }
        self.refresh_environment(false, cx);
        let theme = Theme::for_mode(self.mode);
        let data = self.environment.data.clone();
        let repository = data.as_ref().and_then(|d| d.repository.clone());
        let available = data.is_some();
        let busy = self.environment.busy;
        let worktree = self
            .composer
            .as_ref()
            .is_some_and(|c| c.read(cx).environment_context().2)
            || data.as_ref().is_some_and(|d| d.worktree);
        let branch = data
            .as_ref()
            .and_then(|d| d.branch.clone())
            .unwrap_or_else(|| {
                if available {
                    "Detached HEAD".into()
                } else {
                    "No repository".into()
                }
            });
        let stats = data
            .as_ref()
            .filter(|d| d.changed > 0)
            .map(|d| format!("+{} −{}", d.additions, d.deletions))
            .unwrap_or_default();
        let servers = data.as_ref().map_or(0, |d| d.servers.len()).to_string();
        let editor = self
            .snapshot
            .preferences
            .environment_editor
            .clone()
            .filter(|e| available_editors().contains(&e.as_str()))
            .unwrap_or_else(|| available_editors().first().unwrap_or(&"Finder").to_string());
        let mut card = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .h(px(28.))
                    .pl(px(8.))
                    .pr(px(3.))
                    .flex()
                    .items_center()
                    .text_size(px(12.))
                    .text_color(theme.text.alpha(0.32))
                    .child(div().flex_1().child("Environment"))
                    .child(
                        self.environment_row(
                            8,
                            "profile-settings",
                            ("Panel sections".into(), String::new()),
                            (true, None),
                            theme,
                            cx,
                        )
                        .w(px(24.))
                        .px(px(4.))
                        .h(px(24.)),
                    ),
            )
            .child(self.environment_row(
                0,
                "environment-changes",
                ("Changes".into(), stats),
                (available && !busy, None),
                theme,
                cx,
            ))
            .child(self.environment_row(
                1,
                if worktree {
                    "project-worktree"
                } else {
                    "project-local"
                },
                (
                    if worktree { "Worktree" } else { "Local" }.into(),
                    String::new(),
                ),
                (!busy, Some(Menu::Location)),
                theme,
                cx,
            ))
            .child(self.environment_row(
                2,
                "branch",
                (branch, String::new()),
                (available && !busy, Some(Menu::Branch)),
                theme,
                cx,
            ))
            .child(
                self.environment_row(
                    3,
                    "environment-push",
                    (
                        if data.as_ref().is_some_and(|d| d.changed == 0 && d.ahead > 0) {
                            "Push"
                        } else {
                            "Commit & push"
                        }
                        .into(),
                        String::new(),
                    ),
                    (available && !busy, Some(Menu::Git)),
                    theme,
                    cx,
                ),
            )
            .child(self.environment_row(
                4,
                "approval-link-globe",
                ("Local Servers".into(), servers),
                (available, Some(Menu::Servers)),
                theme,
                cx,
            ));
        if !self
            .snapshot
            .preferences
            .environment_hidden_sections
            .iter()
            .any(|s| s == "repository")
            && let Some(repository) = repository
        {
            card = card
                .child(environment_separator(theme))
                .child(environment_label("Repository", theme))
                .child(self.environment_row(
                    5,
                    "markdown-github",
                    (repository, "↗".into()),
                    (true, None),
                    theme,
                    cx,
                ));
        }
        if !self
            .snapshot
            .preferences
            .environment_hidden_sections
            .iter()
            .any(|s| s == "editor")
        {
            card = card
                .child(environment_separator(theme))
                .child(environment_label("Editor", theme))
                .child(self.environment_row(
                    6,
                    "environment-editor",
                    ("Editor view".into(), String::new()),
                    (self.environment.cwd.is_some(), None),
                    theme,
                    cx,
                ))
                .child(self.environment_row(
                    7,
                    if editor == "Cursor" {
                        "environment-cursor"
                    } else {
                        "environment-editor"
                    },
                    (format!("Open in {editor}"), String::new()),
                    (self.environment.cwd.is_some() && !busy, Some(Menu::Editor)),
                    theme,
                    cx,
                ));
        }
        if let Some(error) = self
            .environment
            .operation_error
            .clone()
            .or_else(|| self.environment.error.clone())
            .filter(|_| self.environment.menu.is_none())
        {
            card = card.child(
                div()
                    .px(px(8.))
                    .py(px(4.))
                    .text_size(px(12.))
                    .text_color(theme.text_tertiary)
                    .child(error),
            );
        }
        if let Some(menu) = self.environment_menu(w, cx) {
            card = card.child(menu);
        }
        card.into_any_element()
    }
}

fn environment_label(label: &'static str, theme: Theme) -> Div {
    div()
        .h(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .text_size(px(12.))
        .text_color(theme.text.alpha(0.32))
        .child(label)
}
fn environment_separator(theme: Theme) -> Div {
    div()
        .h(px(1.))
        .mt(px(3.))
        .mb(px(2.))
        .bg(theme.text.alpha(0.07))
}
fn available_editors() -> Vec<&'static str> {
    [
        "Cursor",
        "Visual Studio Code",
        "Zed",
        "Ghostty",
        "Terminal",
        "Xcode",
        "Finder",
    ]
    .into_iter()
    .filter(|name| {
        *name == "Finder"
            || *name == "Terminal"
            || std::path::Path::new("/Applications")
                .join(format!("{name}.app"))
                .exists()
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::super::tests::Fixture;
    use super::*;
    use gpui::{Modifiers, TestAppContext};

    #[gpui::test]
    fn environment_entire_row_opens_and_escape_returns_focus(cx: &mut TestAppContext) {
        let mut fixture = Fixture::new(cx, Vec::new());
        let row = fixture.bounds("environment-row-1").unwrap();
        // The edge of the row, beyond the icon and label, is clickable.
        fixture.visual.simulate_click(
            row.bottom_right() - point(px(2.), px(2.)),
            Modifiers::none(),
        );
        fixture.draw();
        assert!(fixture.bounds("environment-menu").is_some());
        fixture.visual.simulate_keystrokes("escape");
        fixture.draw();
        let panel = fixture.panel.clone();
        fixture.visual.update(|window, cx| {
            panel.update(cx, |panel, cx| {
                assert!(panel.environment.menu.is_none());
                assert!(panel.environment_focus(1, cx).is_focused(window));
            })
        });
    }

    #[gpui::test]
    fn environment_search_preserves_long_branch_identities_and_disabled_git_actions(
        cx: &mut TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx, Vec::new());
        let panel = fixture.panel.clone();
        fixture.visual.update(|_, cx| {
            panel.update(cx, |panel, cx| {
                let long = format!("feature/{}", "long".repeat(50));
                panel.environment.data = Some(Environment {
                    branch: Some("main".into()),
                    upstream: true,
                    branches: vec!["main".into(), long.clone()],
                    ..Environment::default()
                });
                panel.environment.menu = Some(Menu::Branch);
                panel
                    .environment
                    .input
                    .update(cx, |i, cx| i.set_text_silently("FEATURE/", cx));
                let choices = panel.choices(cx);
                assert_eq!(choices[0].0, long);
                assert_eq!(choices.len(), 2); // plus Create Branch
                panel.environment.menu = Some(Menu::Git);
                let choices = panel.choices(cx);
                assert!(!choices[0].2, "a clean checkout cannot commit");
                assert!(choices[2].2, "a checkout with upstream can pull");
                assert!(
                    !choices[1].2,
                    "a clean checkout with no ahead commits cannot commit and push"
                );
                panel.environment.busy = true;
                assert!(panel.choices(cx).iter().all(|choice| !choice.2));
                panel.reset_environment(cx);
                assert!(panel.environment.data.is_none());
                assert!(!panel.environment.busy);
                assert!(panel.environment.menu.is_none());
            })
        });
    }
}
