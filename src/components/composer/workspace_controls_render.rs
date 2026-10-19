use gpui::{
    Anchor, AnyElement, BoxShadow, Context, Div, Focusable, KeyDownEvent, MouseButton, Role,
    SharedString, Window, canvas, div, hsla, point, prelude::*, px,
};

use super::{ComposerView, workspace_controls::WorkspaceMenu};
use crate::{components::icons::icon, git_review::Checkout, theme::Theme};

impl ComposerView {
    fn render_create_branch_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.workspace_controls.input_focus_pending {
            self.workspace_controls.input_focus_pending = false;
            self.workspace_controls
                .input
                .read(cx)
                .focus_handle(cx)
                .focus(window, cx);
        }
        let theme = Theme::for_mode(self.mode);
        let name = self.workspace_controls.input.read(cx).text().trim();
        let duplicate = self
            .workspace_controls
            .branches
            .as_ref()
            .is_some_and(|list| list.names.iter().any(|b| b == name));
        let validation = if name.ends_with('/') {
            Some(
                crate::i18n::format!("分支名称不能以“/”结尾。" => "Branch name cannot end with \"/\"."),
            )
        } else if duplicate {
            Some(crate::i18n::format!("该分支已存在。" => "This branch already exists."))
        } else {
            self.workspace_controls.error.clone()
        };
        let enabled = !self.workspace_controls.busy && !name.is_empty() && validation.is_none();
        let button = |id: &'static str, label: String, primary: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label.clone())
                .focusable()
                .tab_stop(true)
                .h(px(32.))
                .px(px(16.))
                .rounded(px(12.5))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(crate::typography::ui_size(14.)))
                .line_height(px(18.))
                .cursor_pointer()
                .bg(if primary {
                    theme.text
                } else {
                    theme.text.alpha(0.05)
                })
                .text_color(if primary { theme.surface } else { theme.text })
                .child(label)
        };
        gpui::deferred(div().id("workspace-branch-overlay").absolute().inset_0().bg(gpui::rgba(0x00000021))
            .flex().items_center().justify_center().occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| { if !this.workspace_controls.busy { this.close_workspace_menu(window, cx); } }))
            .child(div().id("workspace-create-branch").debug_selector(|| "workspace-create-branch".into())
                .role(Role::Dialog).aria_label(crate::i18n::format!("创建并切换分支" => "Create and checkout branch"))
                .track_focus(&self.workspace_controls.menu_focus)
                .key_context("ComposerWorkspaceMenu")
                .on_action(cx.listener(|this, _: &super::DismissWorkspaceMenu, window, cx| {
                    if !this.workspace_controls.busy { this.close_workspace_menu(window, cx); }
                }))
                .w(px(400.)).max_w_full().p(px(20.)).rounded(px(25.)).bg(theme.model_picker_surface)
                .border(px(0.5)).border_color(theme.border).font(crate::theme::ui_font())
                .text_color(theme.text).flex().flex_col()
                .on_click(|_, _, cx| cx.stop_propagation())
                .capture_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                    if e.keystroke.key == "escape" && !this.workspace_controls.busy {
                        this.close_workspace_menu(window, cx); cx.stop_propagation();
                    } else if e.keystroke.key == "tab" {
                        if e.keystroke.modifiers.shift { window.focus_prev(cx); } else { window.focus_next(cx); }
                        if !this.workspace_controls.menu_focus.contains_focused(window, cx) {
                            if e.keystroke.modifiers.shift {
                                this.workspace_controls.dialog_close_focus.focus(window, cx);
                            } else {
                                this.workspace_controls.input.read(cx).focus_handle(cx).focus(window, cx);
                            }
                        }
                        cx.stop_propagation();
                    }
                }))
                .child(div().relative().text_size(px(crate::typography::ui_size(20.))).line_height(px(28.)).font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(crate::i18n::format!("创建并切换分支" => "Create and checkout branch"))
                    .child(div().id("workspace-dialog-close").absolute().right(px(-4.)).top(px(-4.)).size(px(24.)).rounded(px(4.))
                        .role(Role::Button).aria_label(crate::i18n::format!("关闭对话框" => "Close dialog"))
                        .track_focus(&self.workspace_controls.dialog_close_focus).tab_stop(true).cursor_pointer()
                        .flex().items_center().justify_center().hover(move |s| s.bg(theme.sidebar_hover))
                        .child(icon("close-dialog", theme.text_tertiary.into()).size(px(16.)))
                        .on_click(cx.listener(|this, _, window, cx| { if !this.workspace_controls.busy { this.close_workspace_menu(window, cx); } }))
                        .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| { if matches!(e.keystroke.key.as_str(), "enter" | "space") && !this.workspace_controls.busy { this.close_workspace_menu(window, cx); cx.stop_propagation(); } }))))
                .child(div().mt(px(12.)).text_size(px(crate::typography::ui_size(13.))).line_height(px(18.57)).flex().justify_between()
                    .child(crate::i18n::format!("分支名称" => "Branch name"))
                    .child(div().id("workspace-set-prefix").role(Role::Button).aria_label(crate::i18n::format!("设置前缀" => "Set prefix"))
                        .focusable().tab_stop(true).cursor_pointer().text_color(theme.text_tertiary)
                        .child(crate::i18n::format!("设置前缀" => "Set prefix"))
                        .on_click(cx.listener(|this, _, _, cx| { this.workspace_controls.menu = None; cx.emit(super::workspace_controls::OpenWorkspaceGitSettings); cx.notify(); }))
                        .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| { if matches!(e.keystroke.key.as_str(), "enter" | "space") { this.workspace_controls.menu = None; cx.emit(super::workspace_controls::OpenWorkspaceGitSettings); cx.notify(); cx.stop_propagation(); } }))))
                .child(div().mt(px(8.)).h(px(36.)).px(px(10.)).rounded(px(10.)).bg(theme.text.alpha(0.04))
                    .flex().items_center().child(self.workspace_controls.input.clone()))
                .when_some(validation, |dialog, error| dialog.child(div().mt(px(6.)).text_size(px(crate::typography::ui_size(12.))).line_height(px(crate::typography::ui_leading(12., 18.))).text_color(theme.text_tertiary).child(error)))
                .child(div().mt(px(12.)).flex().justify_end().gap(px(12.))
                    .child(button("workspace-create-cancel", crate::i18n::text("关闭").to_owned(), false)
                        .on_click(cx.listener(|this, _, window, cx| { if !this.workspace_controls.busy { this.close_workspace_menu(window, cx); } }))
                        .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| { if matches!(e.keystroke.key.as_str(), "enter" | "space") && !this.workspace_controls.busy { this.close_workspace_menu(window, cx); cx.stop_propagation(); } })))
                    .child(button("workspace-create-confirm", crate::i18n::format!("创建并切换" => "Create and checkout"), true).when(!enabled, |s| s.opacity(0.5))
                        .on_click(cx.listener(move |this, _, _, cx| { if enabled { this.confirm_workspace_choice(cx); } }))
                        .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| { if enabled && matches!(e.keystroke.key.as_str(), "enter" | "space") { this.confirm_workspace_choice(cx); cx.stop_propagation(); } }))))))
            .into_any_element()
    }

    pub(super) fn location_trigger(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let label = if self.workspace_controls.worktree_mode {
            crate::i18n::format!("新建本地工作树" => "New local worktree")
        } else {
            crate::i18n::text("本地").to_owned()
        };
        let glyph = if self.workspace_controls.worktree_mode {
            "project-worktree"
        } else {
            "local"
        };
        let bounds = self.workspace_controls.location_bounds.clone();
        div()
            .id("composer-location")
            .accessibility_id("composer-location")
            .debug_selector(|| "composer-location".into())
            .role(Role::Button)
            .aria_label(crate::i18n::format!("选择聊天运行位置" => "Select where to run the chat"))
            .track_focus(&self.workspace_controls.location_focus)
            .tab_stop(true)
            .h(px(28.))
            .px(px(9.))
            .rounded_full()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .text_size(px(crate::typography::ui_size(13.)))
            .line_height(px(18.))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.text)
            .cursor_pointer()
            .hover(move |s| s.bg(theme.text.alpha(0.05)))
            .when(
                self.workspace_controls.menu == Some(WorkspaceMenu::Location),
                |s| s.bg(theme.text.alpha(0.05)),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.open_workspace_menu(WorkspaceMenu::Location, window, cx);
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                if matches!(e.keystroke.key.as_str(), "enter" | "space" | "down") {
                    this.open_workspace_menu(WorkspaceMenu::Location, window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(icon(glyph, theme.text.into()).size(px(16.)))
            .child(label)
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, (), _, _| {})
                    .absolute()
                    .inset_0(),
            )
    }

    pub(super) fn branch_trigger(
        &self,
        branch: SharedString,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let bounds = self.workspace_controls.branch_bounds.clone();
        super::render::branch_utility(branch, theme)
            .accessibility_id("composer-branch")
            .role(Role::Button)
            .aria_label(crate::i18n::format!("切换分支" => "Switch branch"))
            .track_focus(&self.workspace_controls.branch_focus)
            .tab_stop(true)
            .when(
                self.workspace_controls.menu == Some(WorkspaceMenu::Branch),
                |s| s.bg(theme.text.alpha(0.05)),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.open_workspace_menu(WorkspaceMenu::Branch, window, cx);
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                if matches!(e.keystroke.key.as_str(), "enter" | "space" | "down") {
                    this.open_workspace_menu(WorkspaceMenu::Branch, window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, (), _, _| {})
                    .absolute()
                    .inset_0(),
            )
    }

    fn workspace_row(
        &self,
        index: usize,
        label: String,
        description: Option<String>,
        glyph: &'static str,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let theme = Theme::for_mode(self.mode);
        let enabled = !self.workspace_controls.busy
            && (self.workspace_controls.menu != Some(WorkspaceMenu::Location)
                || index == 0
                || self
                    .workspace
                    .checkout
                    .as_ref()
                    .is_some_and(Checkout::is_repository));
        div()
            .id(format!("workspace-choice-{index}"))
            .debug_selector(move || format!("workspace-choice-{index}"))
            .role(Role::MenuItem)
            .aria_label(label.clone())
            .h(px(if description.is_some() { 44.57 } else { 28.57 }))
            .flex_none()
            .px(px(8.))
            .rounded(px(15.))
            .flex()
            .items_center()
            .gap(px(6.))
            .when(!enabled, |s| s.opacity(0.5))
            .when(enabled, |s| {
                s.cursor_pointer().hover(move |s| s.bg(theme.sidebar_hover))
            })
            .when(self.workspace_controls.focused == Some(index), |s| {
                s.bg(theme.sidebar_hover)
            })
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && enabled {
                    this.workspace_controls.focused = Some(index);
                    cx.notify();
                }
            }))
            .child(
                icon(glyph, theme.text.into())
                    .size(px(16.))
                    .flex_none()
                    .opacity(0.75),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .child(div().truncate().child(label))
                    .when_some(description, |s, text| {
                        s.child(
                            div()
                                .truncate()
                                .text_size(px(crate::typography::ui_size(12.)))
                                .line_height(px(16.))
                                .text_color(theme.text_tertiary)
                                .child(text),
                        )
                    }),
            )
            .when(selected, |s| {
                s.child(
                    icon("check", theme.text.into())
                        .size(px(17.))
                        .flex_none()
                        .opacity(0.75),
                )
            })
    }

    pub(super) fn render_workspace_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let kind = self.workspace_controls.menu?;
        if kind == WorkspaceMenu::CreateBranch {
            return Some(self.render_create_branch_dialog(window, cx));
        }
        let theme = Theme::for_mode(self.mode);
        let location = kind == WorkspaceMenu::Location;
        let bounds = if location {
            self.workspace_controls.location_bounds.get()
        } else {
            self.workspace_controls.branch_bounds.get()
        }?;
        let mut menu = div()
            .id("workspace-menu")
            .debug_selector(|| "workspace-menu".into())
            .role(if kind == WorkspaceMenu::CreateBranch {
                Role::Dialog
            } else {
                Role::Menu
            })
            .aria_label(if location {
                crate::i18n::format!("运行位置" => "Run location")
            } else {
                crate::i18n::format!("分支" => "Branches")
            })
            .track_focus(&self.workspace_controls.menu_focus)
            .key_context("ComposerWorkspaceMenu")
            .on_action(
                cx.listener(|this, _: &super::DismissWorkspaceMenu, window, cx| {
                    if !this.workspace_controls.busy {
                        this.close_workspace_menu(window, cx);
                    }
                }),
            )
            .w(px(if location { 216. } else { 296. }))
            .max_h((window.viewport_size().height - px(24.)).max(px(100.)))
            .overflow_y_scroll()
            .p(px(4.))
            .rounded(px(20.))
            .bg(theme.model_picker_surface)
            .shadow(vec![
                BoxShadow::new(px(0.), px(0.), theme.border.into()).spread_radius(px(0.5)),
                BoxShadow::new(px(0.), px(8.), hsla(0., 0., 0., 0.12))
                    .blur_radius(px(16.))
                    .spread_radius(px(-4.)),
            ])
            .font(crate::theme::ui_font())
            .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
            .text_size(px(crate::typography::ui_size(13.)))
            .line_height(px(crate::typography::ui_leading(13., 18.5714)))
            .text_color(theme.text)
            .occlude()
            .capture_key_down(cx.listener(Self::workspace_menu_key))
            .on_mouse_down_out(cx.listener(
                move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if event.button == MouseButton::Left
                        && !bounds.contains(&event.position)
                        && !this.workspace_controls.busy
                    {
                        this.close_workspace_menu(window, cx);
                    }
                },
            ))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation());
        let title = |label: String| {
            div()
                .h(px(26.57))
                .px(px(8.))
                .flex()
                .items_center()
                .text_color(theme.text_tertiary)
                .child(label)
        };
        let separator = || {
            div()
                .h(px(9.))
                .px(px(8.))
                .flex()
                .items_center()
                .child(div().w_full().h(px(1.)).bg(theme.border))
        };
        if location {
            menu = menu.child(title(crate::i18n::format!("工作位置" => "Work in")));
            for (index, label, glyph) in [
                (0, crate::i18n::text("本地").to_owned(), "local"),
                (
                    1,
                    crate::i18n::format!("新建本地工作树" => "New local worktree"),
                    "project-worktree",
                ),
            ] {
                let enabled = index == 0
                    || self
                        .workspace
                        .checkout
                        .as_ref()
                        .is_some_and(Checkout::is_repository);
                let selected = self.workspace_controls.worktree_mode == (index == 1);
                menu = menu.child(
                    self.workspace_row(index, label, None, glyph, selected, cx)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if enabled {
                                this.select_run_location(index == 1, cx);
                            }
                        })),
                );
            }
        } else {
            let choices = self.workspace_choices(cx);
            let selected = if self.workspace_controls.worktree_mode {
                self.workspace_controls.base.as_deref()
            } else {
                self.workspace.checkout.as_ref().and_then(Checkout::branch)
            };
            let mut list = div()
                .id("workspace-branches")
                .debug_selector(|| "workspace-branches".into())
                .h(px(200.))
                .overflow_y_scroll()
                .track_scroll(&self.workspace_controls.scroll)
                .child(title(if self.workspace_controls.worktree_mode {
                    crate::i18n::format!("本地文件状态" => "Local file state")
                } else {
                    crate::i18n::format!("分支" => "Branches")
                }))
                .child(div().h(px(6.)));
            if self.workspace_controls.loading {
                list = list.child(title(
                    crate::i18n::format!("正在加载分支…" => "Loading branches…"),
                ));
            } else {
                let mut local_heading = false;
                for (index, (name, include_changes)) in choices.iter().enumerate() {
                    let special = *include_changes
                        || self
                            .workspace_controls
                            .branches
                            .as_ref()
                            .and_then(|b| b.default_remote.as_ref())
                            == Some(name);
                    if self.workspace_controls.worktree_mode && !special && !local_heading {
                        list =
                            list.child(title(crate::i18n::format!("本地分支" => "Local branches")));
                        local_heading = true;
                    }
                    let branch = name.clone();
                    let include_changes = *include_changes;
                    let selected = selected == Some(name.as_str())
                        && (!self.workspace_controls.worktree_mode
                            || include_changes == self.workspace_controls.include_local_changes);
                    let description = if include_changes {
                        Some(crate::i18n::format!("包含本地代码修改" => "with local code changes"))
                    } else if !self.workspace_controls.worktree_mode && selected {
                        self.workspace_controls.branches.as_ref().filter(|b| b.dirty_files > 0).map(|b| {
                            if b.dirty_files == 1 {
                                crate::i18n::format!("未提交：1 个文件" => "Uncommitted: 1 file")
                            } else {
                                crate::i18n::format!("未提交：{} 个文件" => "Uncommitted: {} files", b.dirty_files)
                            }
                        })
                    } else {
                        None
                    };
                    list = list.child(
                        self.workspace_row(
                            index,
                            name.clone(),
                            description,
                            "branch",
                            selected,
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choose_workspace_branch(branch.clone(), include_changes, cx)
                        })),
                    );
                }
                if choices.is_empty() {
                    list = list.child(title(
                        crate::i18n::format!("未找到分支" => "No branches found"),
                    ));
                }
            }
            menu = menu
                .child(
                    div()
                        .h(px(28.57))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(icon("search", theme.text_tertiary.into()).size(px(16.)))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .child(self.workspace_controls.input.clone()),
                        ),
                )
                .child(div().h(px(6.)))
                .child(list);
            if !self.workspace_controls.worktree_mode {
                menu = menu.child(separator()).child(self.workspace_row(choices.len(), crate::i18n::format!("创建并切换新分支…" => "Create and checkout new branch…"), None, "add", false, cx)
                    .on_click(cx.listener(|this, _, _, cx| { if !this.workspace_controls.busy { this.open_create_branch(cx); } })));
            }
        }
        if self.workspace_controls.busy {
            menu = menu.child(title(
                crate::i18n::format!("正在切换分支…" => "Switching branch…"),
            ));
        }
        if let Some(error) = self.workspace_controls.error.clone() {
            menu = menu
                .child(
                    div()
                        .px(px(8.))
                        .py(px(6.))
                        .text_color(theme.text_tertiary)
                        .child(error),
                )
                .child(
                    self.workspace_row(
                        self.workspace_choices(cx).len()
                            + usize::from(!self.workspace_controls.worktree_mode),
                        crate::i18n::text("重试").to_owned(),
                        None,
                        "review-refresh",
                        false,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.load_workspace_branches(cx))),
                );
        }
        Some(
            gpui::deferred(
                gpui::anchored()
                    .anchor(Anchor::BottomLeft)
                    .position(bounds.origin + point(px(1.), px(-2.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            )
            .into_any_element(),
        )
    }
}
