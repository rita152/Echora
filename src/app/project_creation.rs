//! Native project setup: name, source folders, completion, and draft selection.

mod editing;
mod marker;

use super::ChatApp;
use crate::{
    agent::CreateProject,
    components::{
        icons::icon,
        prompt_input::{PromptChanged, PromptSubmitted},
    },
    theme::Theme,
};
use gpui::{
    Context, Div, Focusable, KeyDownEvent, MouseButton, PathPromptOptions, Window, div, prelude::*,
    px, rgba,
};
use std::path::PathBuf;

impl ChatApp {
    pub fn open_project_creation(&mut self, cx: &mut Context<Self>) {
        if self.project_creation.creating {
            return;
        }
        self.project_creation
            .name
            .update(cx, |input, cx| input.set_mode(self.mode, cx));
        self.project_creation.editing = None;
        self.project_creation.confirmation = None;
        self.project_creation.open = true;
        self.project_creation.error = None;
        self.project_creation.focus_pending = true;
        self.permission_confirmation_open = false;
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.close_transient_menus(cx));
        cx.notify();
    }

    pub(super) fn close_project_creation(&mut self, cx: &mut Context<Self>) {
        if self.project_creation.creating {
            return;
        }
        self.project_creation.open = false;
        self.project_creation.editing = None;
        self.project_creation.confirmation = None;
        self.project_creation.device_menu = false;
        self.project_creation.marker_menu = false;
        self.project_creation.focus_pending = false;
        self.project_creation.roots.clear();
        self.project_creation.marker = None;
        self.project_creation
            .marker_search
            .update(cx, |input, cx| input.set_text_silently("", cx));
        self.project_creation.marker_emoji = false;
        self.project_creation.marker_focused = None;
        self.project_creation.marker_category = None;
        self.project_creation
            .name
            .update(cx, |input, cx| input.set_text_silently("", cx));
        self.project_creation.error = None;
        self.project_creation.cycle = self.project_creation.cycle.wrapping_add(1);
        cx.notify();
    }

    pub(super) fn cancel_project_creation(&mut self, cx: &mut Context<Self>) {
        self.close_project_creation(cx);
    }

    pub(super) fn add_project_sources(&mut self, cx: &mut Context<Self>) {
        if self.project_creation.creating {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some(crate::i18n::text("添加").into()),
        });
        let cycle = self.project_creation.cycle;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if this.project_creation.cycle != cycle || !this.project_creation.open {
                    return;
                }
                for path in paths {
                    if !this.project_creation.roots.contains(&path) {
                        this.project_creation.roots.push(path);
                    }
                }
                this.project_creation.error = None;
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn advance_project_creation(&mut self, cx: &mut Context<Self>) {
        if self.project_creation.creating {
            return;
        }
        if self.project_creation.confirmation.is_some() {
            self.confirm_project_action(cx);
            return;
        }
        if self.project_creation.editing.is_some() {
            self.save_project_edit(cx);
            return;
        }
        let name = self.project_creation.name.read(cx).text().trim().to_owned();
        let roots = self.project_creation.roots.clone();
        let marker = self.project_creation.marker.clone();
        let cycle = self.project_creation.cycle;
        self.project_creation.creating = true;
        self.project_creation.focus_pending = true;
        self.project_creation.marker_menu = false;
        self.project_creation.device_menu = false;
        self.project_creation.error = None;
        let store = self.workspace_store.clone();
        cx.spawn(async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { prepare_project(name, roots) })
                .await;
            let result = match prepared {
                Ok(request) => {
                    let roots = request.roots.clone();
                    let _ = this.update(cx, |this, cx| {
                        if this.project_creation.cycle == cycle {
                            this.project_creation.roots = roots;
                            cx.notify();
                        }
                    });
                    store
                        .create_project(request)
                        .recv()
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r.map_err(|e| e.user_message(crate::i18n::text("创建项目"))))
                }
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                if this.project_creation.cycle != cycle {
                    return;
                }
                this.project_creation.creating = false;
                match result {
                    Ok(project) => {
                        this.workspace_store
                            .set_project_marker(project.project_id.clone(), marker);
                        let cwd = project
                            .roots
                            .first()
                            .cloned()
                            .unwrap_or_else(crate::components::composer::projectless_cwd);
                        this.sidebar.update(cx, |sidebar, cx| {
                            sidebar.set_draft_project(Some(project.project_id.clone()), cx)
                        });
                        this.project_creation.previous_focus = None;
                        this.start_draft(Some(project.project_id), cwd, cx);
                        this.close_pull_requests(cx);
                        this.cancel_project_creation(cx);
                    }
                    Err(error) => {
                        this.project_creation.error = Some(error);
                        this.project_creation.focus_pending = true;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn wire_project_creation(&mut self, cx: &mut Context<Self>) {
        let search = self.project_creation.marker_search.clone();
        cx.subscribe(&search, |this, _, _: &PromptChanged, cx| {
            this.project_creation.marker_focused = None;
            this.project_creation.marker_category = None;
            this.project_creation
                .marker_scroll
                .scroll_to_item(0, gpui::ScrollStrategy::Top);
            cx.notify();
        })
        .detach();
        cx.subscribe(&search, |this, _, _: &PromptSubmitted, cx| {
            if this.project_creation.open && this.project_creation.marker_menu {
                this.select_project_marker(this.project_creation.marker_focused.unwrap_or(0), cx);
            }
        })
        .detach();
        let input = self.project_creation.name.clone();
        cx.subscribe(&input, |this, _, _: &PromptChanged, cx| {
            this.project_creation.error = None;
            cx.notify();
        })
        .detach();
        cx.subscribe(&input, |this, _, _: &PromptSubmitted, cx| {
            if this.project_creation.open {
                this.advance_project_creation(cx);
            }
        })
        .detach();
    }

    pub(super) fn handle_project_creation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.project_creation.open {
            return;
        }
        if self.project_creation.creating {
            cx.stop_propagation();
            return;
        }
        if event.keystroke.key == "escape" {
            if self.project_creation.marker_menu {
                self.project_creation.marker_menu = false;
                self.project_creation.focus_pending = true;
            } else if self.project_creation.device_menu {
                self.project_creation.device_menu = false;
                self.project_creation.device_focus.focus(window, cx);
            } else {
                self.close_project_creation(cx);
            }
            cx.stop_propagation();
            cx.notify();
        } else if event.keystroke.key == "tab" {
            if event.keystroke.modifiers.shift {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            if !self.project_creation.focus.contains_focused(window, cx) {
                if event.keystroke.modifiers.shift {
                    self.project_creation.close_focus.focus(window, cx);
                } else if self.project_creation.confirmation.is_some() {
                    self.project_creation
                        .confirmation_cancel_focus
                        .focus(window, cx);
                } else {
                    self.project_creation
                        .name
                        .read(cx)
                        .focus_handle(cx)
                        .focus(window, cx);
                }
            }
            cx.stop_propagation();
        }
    }

    fn project_button(&self, id: &'static str, label: String, theme: Theme) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .role(gpui::Role::Button)
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
            .hover(move |s| s.bg(theme.sidebar_hover))
            .active(move |s| s.bg(theme.sidebar_hover))
            .child(label)
    }

    pub(super) fn project_creation_overlay(
        &self,
        theme: Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        if self.project_creation.confirmation.is_some() {
            return self.project_confirmation_overlay(theme, cx);
        }
        let editing = self.project_creation.editing.is_some();
        let creating = self.project_creation.creating;
        let title = crate::i18n::text(if editing {
            "编辑项目"
        } else {
            "创建项目"
        });
        let mut sources = div()
            .id("project-creation-sources")
            .w_full()
            .rounded(px(if editing { 8. } else { 12.5 }))
            .border_1()
            .border_color(theme.border)
            .bg(theme.control)
            .text_size(px(crate::typography::ui_size(13.)))
            .line_height(px(crate::typography::ui_leading(13., 18.5714)))
            .max_h(px(360.))
            .overflow_y_scroll();
        if self.project_creation.roots.is_empty() && editing {
            sources = sources.child(div().id("project-edit-choose-sources")
                .role(gpui::Role::Button).aria_label(crate::i18n::format!("选择源文件夹" => "Choose source folders"))
                .focusable().tab_stop(true).h(px(96.)).w_full().flex().flex_col().items_center().justify_center().gap(px(4.)).cursor_pointer()
                .text_color(theme.text_tertiary).hover(move |s|s.bg(theme.sidebar_hover))
                .child(icon("project-add-folder",theme.text_tertiary.into()).size(px(16.)))
                .child(crate::i18n::format!("添加 Codex 可以读取和编辑的文件夹" => "Add folders Codex can read and edit"))
                .on_click(cx.listener(Self::add_project_sources_click))
                .on_key_down(cx.listener(|this,e: &KeyDownEvent,_,cx| {if matches!(e.keystroke.key.as_str(),"enter"|"space") {this.add_project_sources(cx);cx.stop_propagation();}})));
        } else if self.project_creation.roots.is_empty() {
            sources = sources.child(div().h(px(102.5625)).p(px(24.)).flex().flex_col().items_center().gap(px(12.))
                .child(div().id("project-creation-device").role(gpui::Role::Button).aria_label(crate::i18n::format!("项目所在设备" => "Project device"))
                    .track_focus(&self.project_creation.device_focus).tab_stop(true).h(px(18.5625)).flex().items_center().gap(px(4.)).text_size(px(crate::typography::ui_size(13.))).line_height(px(crate::typography::ui_leading(13., 18.5714))).text_color(theme.text.alpha(0.65)).cursor_pointer()
                    .on_click(cx.listener(|this,_,_,cx|{ if !this.project_creation.creating {this.project_creation.device_menu = !this.project_creation.device_menu;} cx.notify(); }))
                    .on_key_down(cx.listener(|this,e: &KeyDownEvent,_,cx|{ if matches!(e.keystroke.key.as_str(),"enter"|"space"|"down") { this.project_creation.device_menu=true; cx.stop_propagation(); cx.notify(); }}))
                    .child(crate::i18n::format!("在此电脑上添加文件夹" => "Add a folder on this computer")).child(icon("chevron-down",theme.text.into()).size(px(16.))))
                .child(div().id("project-creation-add-source").role(gpui::Role::Button).aria_label(crate::i18n::text("添加")).focusable().tab_stop(true)
                    .h(px(24.)).px(px(8.)).rounded_full().bg(theme.text.alpha(0.05)).flex().items_center().gap(px(4.)).text_size(px(crate::typography::ui_size(13.))).line_height(px(18.)).cursor_pointer()
                    .hover(move |s|s.bg(theme.text.alpha(0.10))).on_click(cx.listener(Self::add_project_sources_click))
                    .on_key_down(cx.listener(|this,e: &KeyDownEvent,_,cx|{if matches!(e.keystroke.key.as_str(),"enter"|"space"){this.add_project_sources(cx);cx.stop_propagation();}}))
                    .child(icon("project-add-folder",theme.text.into()).size(px(16.))).child(crate::i18n::text("添加"))));
        } else {
            for (index, path) in self.project_creation.roots.iter().enumerate() {
                let full_path = path.display().to_string();
                let path = if editing {
                    path.file_name()
                        .unwrap_or(path.as_os_str())
                        .to_string_lossy()
                        .into_owned()
                } else {
                    full_path.clone()
                };
                sources = sources.child(
                    div()
                        .id(format!("project-source-{index}"))
                        .tooltip(move |_, cx| {
                            cx.new(|_| {
                                crate::components::composer::TrayTooltip(full_path.clone().into())
                            })
                            .into()
                        })
                        .h(px(48.))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .border_b_1()
                        .border_color(theme.border)
                        .child(icon("utility-folder", theme.text_tertiary.into()).size(px(16.)))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(path),
                        )
                        .child(
                            div()
                                .id(format!("project-source-remove-{index}"))
                                .role(gpui::Role::Button)
                                .aria_label(crate::i18n::format!("移除文件夹" => "Remove folder"))
                                .focusable()
                                .tab_stop(true)
                                .size(px(if editing { 24. } else { 28. }))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(move |s| s.bg(theme.sidebar_hover))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.project_creation.creating {
                                        this.project_creation.roots.remove(index);
                                        cx.notify();
                                    }
                                }))
                                .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                                    if matches!(e.keystroke.key.as_str(), "enter" | "space")
                                        && !this.project_creation.creating
                                    {
                                        this.project_creation.roots.remove(index);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    icon("close-dialog", theme.text_tertiary.into()).size(px(16.)),
                                ),
                        ),
                );
            }
            sources = sources.child(
                div()
                    .id("project-creation-add-more")
                    .role(gpui::Role::Button)
                    .aria_label(crate::i18n::format!("添加文件夹" => "Add folder"))
                    .focusable()
                    .tab_stop(true)
                    .h(px(if editing { 48. } else { 36. }))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .on_click(cx.listener(Self::add_project_sources_click))
                    .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            this.add_project_sources(cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        icon(
                            if editing { "project-add-folder" } else { "add" },
                            theme.text.into(),
                        )
                        .size(px(16.)),
                    )
                    .child(crate::i18n::format!("添加文件夹" => "Add folder")),
            );
        }
        let name = div()
            .h(px(40.))
            .w_full()
            .rounded(px(if editing { 12. } else { 15. }))
            .border_1()
            .border_color(if self.project_creation.error.is_some() {
                theme.warning
            } else if self
                .project_creation
                .name
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
            {
                theme.accent
            } else {
                theme.border
            })
            .bg(theme.control)
            .flex()
            .items_center()
            .gap(px(8.))
            .pr(px(12.))
            .overflow_hidden()
            .child(
                div()
                    .id("project-marker-trigger")
                    .role(gpui::Role::Button)
                    .aria_label(crate::i18n::format!("更改项目标记" => "Change project marker"))
                    .focusable()
                    .tab_stop(true)
                    .w(px(40.))
                    .h_full()
                    .border_r_1()
                    .border_color(theme.border)
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.project_creation.creating {
                            return;
                        }
                        this.project_creation.marker_menu = !this.project_creation.marker_menu;
                        this.project_creation
                            .marker_search
                            .update(cx, |input, cx| input.set_mode(this.mode, cx));
                        if this.project_creation.marker_menu {
                            this.project_creation
                                .marker_search
                                .read(cx)
                                .focus_handle(cx)
                                .focus(window, cx);
                        }
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space" | "down") {
                            this.project_creation.marker_menu = true;
                            this.project_creation
                                .marker_search
                                .read(cx)
                                .focus_handle(cx)
                                .focus(window, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .child(crate::components::project_marker::glyph(
                        self.project_creation.marker.as_ref(),
                        theme,
                        16.,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .when(!creating, |d| d.child(self.project_creation.name.clone()))
                    .when(creating, |d| {
                        d.opacity(0.4)
                            .child(self.project_creation.name.read(cx).text().to_owned())
                    }),
            );
        let dialog = div()
            .id("project-creation-dialog")
            .role(gpui::Role::Dialog)
            .aria_label(title)
            .relative()
            .w(px(520.))
            .max_w(gpui::relative(0.92))
            .p(px(20.))
            .rounded(px(25.))
            .bg(theme.project_dialog_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(0.), theme.border.into()).spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.), px(4.), gpui::hsla(0., 0., 0., 0.10))
                    .blur_radius(px(8.))
                    .spread_radius(px(-2.)),
            ])
            .font(crate::theme::ui_font())
            .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
            .text_size(px(crate::typography::ui_size(14.)))
            .line_height(px(crate::typography::ui_leading(14., 21.)))
            .text_color(theme.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h(px(28.))
                    .text_size(px(crate::typography::ui_size(20.)))
                    .line_height(px(28.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(div().mt(px(12.)).child(name))
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(crate::typography::ui_size(13.)))
                            .line_height(px(18.5625))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(crate::i18n::format!("源文件夹" => "Source folders")),
                    )
                    .child(sources),
            )
            .when_some(self.project_creation.error.clone(), |d, error| {
                d.child(div().mt(px(12.)).text_color(theme.warning).child(error))
            })
            .child(
                div()
                    .mt(px(if editing { 20. } else { 32. }))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(12.))
                    .when(editing, |row| {
                        row.child(
                            self.project_button(
                                "project-edit-remove",
                                crate::i18n::format!("移除本地项目" => "Remove local project"),
                                theme,
                            )
                            .bg(gpui::rgba(0xe02e2a1a))
                            .text_color(gpui::rgba(0xe02e2aff))
                            .on_click(cx.listener(|this, _, _, cx| this.remove_editing_project(cx)))
                            .on_key_down(cx.listener(
                                |this, e: &KeyDownEvent, _, cx| {
                                    if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                        this.remove_editing_project(cx);
                                        cx.stop_propagation();
                                    }
                                },
                            )),
                        )
                        .child(div().flex_1())
                    })
                    .child(
                        self.project_button(
                            "project-creation-cancel",
                            crate::i18n::text("取消").to_owned(),
                            theme,
                        )
                        .text_color(theme.text_tertiary)
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_project_creation(cx)))
                        .on_key_down(cx.listener(
                            |this, e: &KeyDownEvent, _, cx| {
                                if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                    this.cancel_project_creation(cx);
                                    cx.stop_propagation();
                                }
                            },
                        )),
                    )
                    .child(
                        self.project_button(
                            "project-creation-submit",
                            if creating {
                                if editing {
                                    crate::i18n::text("正在保存…")
                                } else {
                                    crate::i18n::text("正在创建项目…")
                                }
                            } else if editing {
                                crate::i18n::text("保存")
                            } else {
                                crate::i18n::text("创建项目")
                            }
                            .to_owned(),
                            theme,
                        )
                        .bg(theme.button)
                        .text_color(theme.button_text)
                        .when(
                            creating
                                || (editing
                                    && self
                                        .project_creation
                                        .name
                                        .read(cx)
                                        .text()
                                        .trim()
                                        .is_empty()),
                            |b| b.opacity(0.4).cursor_default(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.advance_project_creation(cx)))
                        .on_key_down(cx.listener(
                            |this, e: &KeyDownEvent, _, cx| {
                                if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                    this.advance_project_creation(cx);
                                    cx.stop_propagation();
                                }
                            },
                        )),
                    ),
            )
            .child(
                div()
                    .id("project-creation-close")
                    .role(gpui::Role::Button)
                    .aria_label(crate::i18n::text("关闭对话框"))
                    .track_focus(&self.project_creation.close_focus)
                    .tab_stop(true)
                    .absolute()
                    .top(px(16.))
                    .right(px(16.))
                    .size(px(24.))
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| this.close_project_creation(cx)))
                    .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            this.close_project_creation(cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(icon("close-dialog", theme.text.alpha(0.8).into()).size(px(16.))),
            )
            .when(self.project_creation.device_menu, |d| {
                d.child(self.project_device_menu(theme, cx))
            })
            .when(self.project_creation.marker_menu, |d| {
                d.child(self.project_marker_menu(theme, cx))
            });
        div()
            .id("project-creation-overlay")
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x00000022))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.project_creation.focus)
            .key_context("ProjectCreation")
            .capture_key_down(cx.listener(Self::handle_project_creation_key))
            .on_action(
                cx.listener(|this, _: &super::DismissProjectCreation, window, cx| {
                    if this.project_creation.marker_menu {
                        this.project_creation.marker_menu = false;
                        this.project_creation.focus_pending = true;
                    } else if this.project_creation.device_menu {
                        this.project_creation.device_menu = false;
                        this.project_creation.device_focus.focus(window, cx);
                    } else {
                        this.close_project_creation(cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_click(cx.listener(|this, _, _, cx| this.close_project_creation(cx)))
            .child(dialog)
    }

    fn add_project_sources_click(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_project_sources(cx);
    }

    fn project_device_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        div()
            .id("project-device-menu")
            .absolute()
            .top(px(184.5))
            .left(px(160.))
            .w(px(200.))
            .p(px(4.))
            .rounded(px(20.))
            .bg(theme.model_picker_surface)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(8.), gpui::hsla(0., 0., 0., 0.12))
                    .blur_radius(px(16.))
                    .spread_radius(px(-4.)),
                gpui::BoxShadow::new(px(0.), px(0.), theme.border.into()).spread_radius(px(0.5)),
            ])
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.project_creation.device_menu = false;
                cx.notify();
            }))
            .child(
                div()
                    .id("project-device-local")
                    .role(gpui::Role::Button)
                    .aria_label(crate::i18n::format!("此电脑" => "This computer"))
                    .focusable()
                    .tab_stop(true)
                    .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            this.project_creation.device_menu = false;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .h(px(28.5625))
                    .rounded(px(15.))
                    .text_size(px(crate::typography::ui_size(13.)))
                    .line_height(px(crate::typography::ui_leading(13., 18.5714)))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.project_creation.device_menu = false;
                        cx.notify();
                    }))
                    .child(icon("local", theme.text.into()).size(px(16.)))
                    .child(
                        div()
                            .flex_1()
                            .child(crate::i18n::format!("此电脑" => "This computer")),
                    )
                    .child(icon("check", theme.text.into()).size(px(16.))),
            )
    }
}

fn prepare_project(name: String, roots: Vec<PathBuf>) -> Result<CreateProject, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is unavailable")?;
    prepare_project_in(name, roots, home.join("Documents/Echora"))
}

fn prepare_project_in(
    name: String,
    mut roots: Vec<PathBuf>,
    parent: PathBuf,
) -> Result<CreateProject, String> {
    for root in &mut roots {
        *root = std::fs::canonicalize(&*root).map_err(|error| error.to_string())?;
        if !root.is_dir() {
            return Err(crate::i18n::format!("源文件夹不存在" => "Source folder does not exist"));
        }
    }
    let mut seen = std::collections::HashSet::new();
    roots.retain(|root| seen.insert(root.clone()));
    let name = if name.is_empty() {
        roots
            .first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| crate::i18n::format!("新建项目" => "New project"))
    } else {
        name
    };
    if roots.is_empty() {
        std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let folder = name
            .chars()
            .map(|c| {
                if c.is_control()
                    || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                {
                    '_'
                } else {
                    c
                }
            })
            .collect::<String>();
        let folder = folder.trim().trim_matches('.');
        let folder = if folder.is_empty() {
            "New project"
        } else {
            folder
        };
        for suffix in 1..10000 {
            let path = parent.join(if suffix == 1 {
                folder.to_owned()
            } else {
                format!("{folder} {suffix}")
            });
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    let _ = std::process::Command::new("git")
                        .arg("init")
                        .arg("--quiet")
                        .arg(&path)
                        .status();
                    roots.push(path);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        if roots.is_empty() {
            return Err("Could not allocate project folder".into());
        }
    }
    Ok(CreateProject { name, roots })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_sources_are_validated_canonicalized_and_deduplicated() {
        let root =
            std::env::temp_dir().join(format!("echora-project-source-{}", std::process::id()));
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        let request = prepare_project_in(
            String::new(),
            vec![root.join("a"), root.join("b"), root.join("a/../a")],
            root.join("unused"),
        )
        .unwrap();
        assert_eq!(request.name, "a");
        assert_eq!(request.roots.len(), 2);
        assert_eq!(
            request.roots[0],
            std::fs::canonicalize(root.join("a")).unwrap()
        );
        assert!(
            prepare_project_in(
                "bad".into(),
                vec![root.join("missing")],
                root.join("unused")
            )
            .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn project_without_sources_creates_unique_safe_git_workspaces() {
        let parent =
            std::env::temp_dir().join(format!("echora-project-default-{}", std::process::id()));
        let first = prepare_project_in("../Name: Test".into(), vec![], parent.clone()).unwrap();
        let second = prepare_project_in("../Name: Test".into(), vec![], parent.clone()).unwrap();
        assert_ne!(first.roots, second.roots);
        assert_eq!(first.name, "../Name: Test");
        assert_eq!(first.roots[0].parent(), Some(parent.as_path()));
        assert!(first.roots[0].join(".git").is_dir());
        std::fs::remove_dir_all(parent).unwrap();
    }
}
