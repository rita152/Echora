//! Project selection for an unsent conversation. The app owns the draft target;
//! this menu emits a selection instead of starting a second conversation.

use std::{cell::Cell, path::PathBuf, rc::Rc};

use gpui::{
    Anchor, AnyElement, Bounds, BoxShadow, Context, Div, Entity, FocusHandle, Focusable,
    KeyDownEvent, MouseButton, Pixels, Window, canvas, div, hsla, point, prelude::*, px,
};

use super::{ComposerView, DismissProjectPicker};
use crate::{
    agent::{Project, ProjectId},
    components::{
        icons::icon,
        prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
    },
    theme::{Theme, ThemeMode},
};

pub struct DraftProjectSelected(pub Option<ProjectId>);
pub struct OpenProjectCreation(pub FocusHandle);
impl gpui::EventEmitter<DraftProjectSelected> for ComposerView {}
impl gpui::EventEmitter<OpenProjectCreation> for ComposerView {}

pub(super) struct ProjectPicker {
    pub(super) open: bool,
    pub(super) projects: Vec<Project>,
    pub(super) loading: bool,
    pub(super) error: Option<String>,
    pub(super) input: Entity<PromptInput>,
    pub(super) trigger_focus: FocusHandle,
    pub(super) clear_focus: FocusHandle,
    pub(super) trigger_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    active_bounds: Option<Bounds<Pixels>>,
    previous_focus: Option<FocusHandle>,
    focused: usize,
    scroll: gpui::ScrollHandle,
}

impl ProjectPicker {
    pub(super) fn new(mode: ThemeMode, cx: &mut Context<ComposerView>) -> Self {
        let input = cx.new(|cx| {
            PromptInput::project_name(
                mode,
                crate::i18n::format!("搜索项目" => "Search projects"),
                cx,
            )
        });
        cx.subscribe(&input, |this, _, _: &PromptChanged, cx| {
            this.project_picker.focused = 0;
            this.project_picker.scroll.set_offset(point(px(0.), px(0.)));
            cx.notify();
        })
        .detach();
        cx.subscribe(&input, |this, _, _: &PromptSubmitted, cx| {
            if this.project_picker.open {
                this.confirm_project_picker(cx);
            }
        })
        .detach();
        Self {
            open: false,
            projects: Vec::new(),
            loading: false,
            error: None,
            input,
            trigger_focus: cx.focus_handle().tab_stop(true),
            clear_focus: cx.focus_handle().tab_stop(true),
            trigger_bounds: Rc::new(Cell::new(None)),
            active_bounds: None,
            previous_focus: None,
            focused: 0,
            scroll: gpui::ScrollHandle::new(),
        }
    }
}

#[derive(Clone)]
pub(super) enum ProjectChoice {
    Project(ProjectId, String),
    Create,
    Clear,
}

impl ComposerView {
    pub fn set_available_projects(
        &mut self,
        projects: &[Project],
        loading: bool,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.project_picker.projects != projects
            || self.project_picker.loading != loading
            || self.project_picker.error != error
        {
            self.project_picker.projects = projects.to_vec();
            self.project_picker.loading = loading;
            self.project_picker.error = error;
            self.project_picker.focused = self
                .project_picker
                .focused
                .min(self.project_choices(cx).len().saturating_sub(1));
            cx.notify();
        }
    }

    pub fn open_project_picker(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.thread_id().is_some() || self.workspace_controls.busy {
            return;
        }
        self.project_picker.open = true;
        self.workspace_controls.menu = None;
        self.project_picker.active_bounds =
            bounds.or_else(|| self.project_picker.trigger_bounds.get());
        self.project_picker.previous_focus = window.focused(cx);
        self.project_picker
            .input
            .update(cx, |input, cx| input.set_mode(self.mode, cx));
        self.project_picker.focused = 0;
        self.project_picker.scroll.set_offset(point(px(0.), px(0.)));
        self.project_picker
            .input
            .update(cx, |input, cx| input.set_text_silently("", cx));
        self.project_picker
            .input
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
        self.menu_open = false;
        self.permission_menu_open = false;
        self.context_menu_open = false;
        cx.notify();
    }

    pub(super) fn project_choices(&self, cx: &Context<Self>) -> Vec<ProjectChoice> {
        let query = self
            .project_picker
            .input
            .read(cx)
            .text()
            .trim()
            .to_lowercase();
        let mut choices = self
            .project_picker
            .projects
            .iter()
            .filter(|project| {
                let searchable = format!(
                    "{} {}",
                    project.name,
                    project
                        .roots
                        .iter()
                        .map(|p| p.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .to_lowercase();
                query
                    .split_whitespace()
                    .all(|word| searchable.contains(word))
            })
            .map(|p| ProjectChoice::Project(p.project_id.clone(), p.name.clone()))
            .collect::<Vec<_>>();
        choices.push(ProjectChoice::Create);
        if self.conversation.project_id.is_some() {
            choices.push(ProjectChoice::Clear);
        }
        choices
    }

    pub(super) fn apply_project_choice(&mut self, choice: ProjectChoice, cx: &mut Context<Self>) {
        self.project_picker.open = false;
        match choice {
            ProjectChoice::Project(id, _) => {
                self.focus_prompt_pending = true;
                cx.emit(DraftProjectSelected(Some(id)));
            }
            ProjectChoice::Clear => {
                self.focus_prompt_pending = true;
                cx.emit(DraftProjectSelected(None));
            }
            ProjectChoice::Create => cx.emit(OpenProjectCreation(
                self.project_picker.trigger_focus.clone(),
            )),
        }
        cx.notify();
    }

    pub(super) fn confirm_project_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(choice) = self
            .project_choices(cx)
            .get(self.project_picker.focused)
            .cloned()
        {
            self.apply_project_choice(choice, cx);
        }
    }

    fn project_picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let choices = self.project_choices(cx);
        let count = choices.len();
        let project_count = choices
            .iter()
            .filter(|c| matches!(c, ProjectChoice::Project(_, _)))
            .count();
        match event.keystroke.key.as_str() {
            "escape" => {
                self.project_picker.open = false;
                if let Some(focus) = self.project_picker.previous_focus.take() {
                    focus.focus(window, cx);
                }
            }
            "down" | "tab" if count > 0 => {
                self.project_picker.focused = if event.keystroke.modifiers.shift {
                    (self.project_picker.focused + count - 1) % count
                } else {
                    (self.project_picker.focused + 1) % count
                };
                if project_count > 0 {
                    self.project_picker
                        .scroll
                        .scroll_to_item(self.project_picker.focused.min(project_count - 1));
                }
            }
            "up" if count > 0 => {
                self.project_picker.focused = (self.project_picker.focused + count - 1) % count;
                if project_count > 0 {
                    self.project_picker
                        .scroll
                        .scroll_to_item(self.project_picker.focused.min(project_count - 1));
                }
            }
            "enter" => self.confirm_project_picker(cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn project_trigger(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let bounds = self.project_picker.trigger_bounds.clone();
        super::render::project_utility(self.workspace.project_label.clone(), theme)
            .role(gpui::Role::Button)
            .aria_label(crate::i18n::text("选择项目"))
            .track_focus(&self.project_picker.trigger_focus)
            .tab_stop(true)
            .when(self.project_picker.open, |button| {
                button.bg(theme.text.alpha(0.05))
            })
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                if matches!(e.keystroke.key.as_str(), "enter" | "space" | "down") {
                    this.open_project_picker(None, window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                if this.project_picker.open {
                    this.project_picker.open = false;
                    this.project_picker.trigger_focus.focus(window, cx);
                    cx.notify();
                } else {
                    this.open_project_picker(None, window, cx);
                }
            }))
            .when(self.workspace.project_label.is_some(), |button| {
                button.child(
                    div()
                        .id("composer-clear-project")
                        .debug_selector(|| "composer-clear-project".to_owned())
                        .role(gpui::Role::Button)
                        .aria_label(
                            crate::i18n::format!("不在项目中工作" => "Don't work in a project"),
                        )
                        .track_focus(&self.project_picker.clear_focus)
                        .tab_stop(true)
                        .absolute()
                        .top_0()
                        .left_0()
                        .size(px(28.))
                        .group("composer-clear-project-icon")
                        .invisible()
                        .group_hover("composer-project-hover", |s| s.visible())
                        .focus_visible(|s| s.visible())
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |s| s.bg(theme.text.alpha(0.05)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.apply_project_choice(ProjectChoice::Clear, cx);
                        }))
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                this.apply_project_choice(ProjectChoice::Clear, cx);
                            }
                        }))
                        .child(
                            icon("clear-project", theme.text_tertiary.into())
                                .size(px(16.))
                                .group_hover("composer-clear-project-icon", move |s| {
                                    s.text_color(theme.text)
                                }),
                        ),
                )
            })
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, (), _, _| {})
                    .absolute()
                    .inset_0()
                    .size_full(),
            )
    }

    pub(super) fn render_project_picker(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.project_picker.open {
            return None;
        }
        let theme = Theme::for_mode(self.mode);
        let choices = self.project_choices(cx);
        let mut projects = div()
            .id("project-picker-list")
            .max_h(px(142.8125))
            .overflow_y_scroll()
            .track_scroll(&self.project_picker.scroll);
        let mut actions = div().flex().flex_col();
        for (index, choice) in choices.iter().enumerate() {
            let (label, glyph, selected) = match choice {
                ProjectChoice::Project(id, name) => (
                    name.clone(),
                    "utility-folder",
                    self.conversation.project_id.as_ref() == Some(id),
                ),
                ProjectChoice::Create => (
                    crate::i18n::format!("新建项目" => "New project"),
                    "add",
                    false,
                ),
                ProjectChoice::Clear => (
                    crate::i18n::format!("不在项目中工作" => "Don't work in a project"),
                    "close-dialog",
                    false,
                ),
            };
            let choice = choice.clone();
            let row = div()
                .id(format!("project-picker-row-{index}"))
                .role(gpui::Role::Button)
                .aria_label(label.clone())
                .h(px(28.5625))
                .px(px(8.))
                .rounded(px(15.))
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .when(self.project_picker.focused == index, |r| {
                    r.bg(theme.sidebar_hover)
                })
                .hover(move |s| s.bg(theme.sidebar_hover))
                .active(move |s| s.bg(theme.sidebar_hover))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered {
                        this.project_picker.focused = index;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.apply_project_choice(choice.clone(), cx);
                }))
                .child(icon(glyph, theme.text.into()).size(px(16.)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(label),
                )
                .when(selected, |r| {
                    r.child(icon("check", theme.text.into()).size(px(16.)))
                });
            if matches!(choices[index], ProjectChoice::Project(_, _)) {
                projects = projects.child(row);
            } else {
                actions = actions.child(row);
            }
        }
        let has_projects = choices
            .iter()
            .any(|c| matches!(c, ProjectChoice::Project(_, _)));
        let has_actions = choices
            .iter()
            .any(|c| !matches!(c, ProjectChoice::Project(_, _)));
        let status = if self.project_picker.loading {
            Some(crate::i18n::format!("正在加载项目…" => "Loading projects…"))
        } else if let Some(error) = &self.project_picker.error {
            Some(error.clone())
        } else if !has_projects {
            Some(crate::i18n::format!("未找到项目" => "No projects found"))
        } else {
            None
        };
        let separator = || {
            div()
                .h(px(9.))
                .px(px(8.))
                .flex()
                .items_center()
                .child(div().h(px(1.)).w_full().bg(theme.border))
        };
        let menu = div()
            .id("project-picker-menu")
            .role(gpui::Role::Dialog)
            .aria_label(crate::i18n::format!("搜索项目" => "Search projects"))
            .key_context("ComposerProjectPicker")
            .w(px(260.))
            .max_h(px(350.))
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
            .capture_key_down(cx.listener(Self::project_picker_key))
            .on_action(cx.listener(|this, _: &DismissProjectPicker, window, cx| {
                this.project_picker.open = false;
                this.project_picker.trigger_focus.focus(window, cx);
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down_out(cx.listener(|this, e: &gpui::MouseDownEvent, window, cx| {
                if e.button == MouseButton::Left
                    && !this
                        .project_picker
                        .active_bounds
                        .is_some_and(|b| b.contains(&e.position))
                {
                    this.project_picker.open = false;
                    if let Some(focus) = this.project_picker.previous_focus.take() {
                        focus.focus(window, cx);
                    }
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h(px(28.5625))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon("search", theme.text_tertiary.into()).size(px(16.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(self.project_picker.input.clone()),
                    ),
            )
            .child(separator())
            .child(projects)
            .when_some(status, |menu, status| {
                menu.child(
                    div()
                        .px(px(8.))
                        .py(px(5.))
                        .text_color(theme.text_tertiary)
                        .child(status),
                )
            })
            .when(has_projects && has_actions, |m| m.child(separator()))
            .when(has_actions, |m| m.child(actions));
        let b = self.project_picker.active_bounds?;
        let popup = gpui::anchored()
            .anchor(Anchor::BottomLeft)
            .position(b.origin + point(px(1.), px(-2.)))
            .snap_to_window_with_margin(px(8.))
            .child(menu);
        let _ = window;
        Some(gpui::deferred(popup).into_any_element())
    }
}

pub(crate) fn projectless_cwd() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}
