//! Render behavior and presentation for the prompt composer.

use gpui::{
    Animation, AnimationExt, Bounds, BoxShadow, ContentMask, Context, Div, Hsla, MouseButton,
    Pixels, Render, SharedString, TextAlign, TextRun, Window, canvas, deferred, div, hsla, point,
    prelude::*, px, rgba, size,
};

use super::{COMPOSER_CORNER_RADIUS, ComposerView, DictationState, MODEL_PICKER_TRIGGER_GAP};
use crate::{
    components::icons::icon,
    conversation::ConversationPhase,
    theme::{Theme, ThemeMode},
};

impl ComposerView {
    pub(super) fn render_composer(
        &self,
        viewport_width: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let (permission_label, permission_icon, permission_color) = self.permission_label();
        let effective_model_label = self.effective_model_label();
        let model_status_active = self.conversation.model_status.is_some();
        let effort_or_status_label = self
            .conversation
            .model_status
            .clone()
            .unwrap_or_else(|| self.selected_effort_label());
        let fast_tier_selected = self.conversation.selected_service_tier.is_some();
        let composer_width = self.available_width.unwrap_or(viewport_width);
        let compact = composer_width < 430.0;
        let model_trigger_bounds = self.model_trigger_bounds.clone();
        let context_trigger_bounds = self.context_catalog.trigger_bounds.clone();
        let trigger_label = div()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(MODEL_PICKER_TRIGGER_GAP))
            .child(
                div()
                    .min_w(px(0.0))
                    .when(compact, |d| {
                        d.max_w(px((composer_width - 202.0).max(42.0))).truncate()
                    })
                    .text_color(theme.text)
                    .child(if self.menu_open && !self.picker_shows_model_list() {
                        crate::i18n::text(if self.conversation.model_explicit {
                            "选择推理强度"
                        } else {
                            "选择模型"
                        })
                        .to_owned()
                    } else {
                        effective_model_label
                    }),
            )
            .when(
                !compact && (!self.menu_open || self.picker_shows_model_list()),
                |d| {
                    d.child(
                        div()
                            .text_color(if model_status_active {
                                theme.effort
                            } else {
                                theme.text_tertiary
                            })
                            .child(effort_or_status_label),
                    )
                },
            );
        let trigger_value = div()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .when(fast_tier_selected, |value| {
                value
                    .gap(px(MODEL_PICKER_TRIGGER_GAP))
                    .child(icon("model-fast", theme.text.into()).size(px(14.0)))
            })
            .child(trigger_label);
        let conversation_started = self.conversation.phase != ConversationPhase::Empty
            || !self.conversation.activities.is_empty();
        let send_action = self.send_action(cx);
        div()
            .w_full()
            .relative()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && this.context_menu_open {
                    this.context_menu_open = false;
                    this.prompt_focus_handle(cx).focus(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                } else {cx.propagate();}
            }))
            // ⌘Z undoes the composer text first, then restores the last
            // removed queued message, as the reference's app undo does.
            .capture_action(cx.listener(|this, _: &crate::components::file_editor::EditorTab, _, cx| {
                if this.context_menu_key("tab", false, cx) || this.slash_menu_key("tab", false, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &crate::app::DismissPermissionUi, _, cx| {
                if this.context_menu_key("escape", false, cx) || this.slash_menu_key("escape", false, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &crate::components::file_editor::EditorUndo, _, cx| {
                let text_undo = this.prompt_editor.read(cx).can_undo();
                if !text_undo && this.undo_queue_removal(cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &crate::components::file_editor::EditorRedo, _, cx| {
                let text_redo = this.prompt_editor.read(cx).can_redo();
                if !text_redo && this.redo_queue_removal(cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let modifiers = event.keystroke.modifiers;
                let ctrl_only = modifiers.control && !modifiers.platform && !modifiers.alt && !modifiers.shift;
                // macOS can mark synthetic and hardware navigation keys with
                // the Fn flag. It does not change slash-menu navigation.
                let plain = !modifiers.control && !modifiers.platform && !modifiers.alt && !modifiers.shift;
                if (plain || ctrl_only)
                    && (this.context_menu_key(event.keystroke.key.as_str(), ctrl_only, cx) || this.slash_menu_key(event.keystroke.key.as_str(), ctrl_only, cx))
                {
                    cx.stop_propagation();
                    return;
                }
                if event.keystroke.key == "up"
                    && plain
                    && this.prompt_focus_handle(cx).is_focused(window)
                    && this.queue_supported()
                    && this.edit_last_queued(cx)
                {
                    cx.stop_propagation();
                }
            }))
            .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| this.attach_paths(paths.paths().to_vec(), cx)))
            .flex()
            .flex_col()
            .gap(px(0.0))
            .children(self.render_command_status(theme,cx))
            .when(!conversation_started && !self.side_chat, |composer| {
                composer.child(context_toolbar(self, theme, cx))
            })
            .child(self.submission_feedback(theme, cx))
            .when(self.tray_height() > 0.0, |composer| {
                composer.child(div().h(px(self.tray_height())).flex_none())
            })
            .children(self.render_slash_menu(
                theme,
                if self.review_comments.is_empty() {
                    self.composer_body_height(cx)
                } else {
                    self.composer_body_height(cx) + 32.0
                },
                cx,
            ))
            .children(self.render_tray(
                theme,
                if self.review_comments.is_empty() {
                    self.composer_body_height(cx)
                } else {
                    self.composer_body_height(cx) + 32.0
                },
                cx,
            ))
            .child(
                div()
                    .h(px(if self.review_comments.is_empty() {
                        self.composer_body_height(cx)
                    } else {
                        self.composer_body_height(cx) + 32.0
                    }))
                    .w_full()
                    .rounded(px(COMPOSER_CORNER_RADIUS))
                    .bg(theme.control_soft)
                    .shadow(vec![
                        BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                            .spread_radius(px(0.5)),
                        BoxShadow::new(px(0.0), px(3.0), hsla(0.0, 0.0, 0.0, 0.04))
                            .blur_radius(px(7.5)),
                        BoxShadow::new(px(0.0), px(0.0), hsla(0.0, 0.0, 0.0, 0.05))
                            .blur_radius(px(20.0)),
                        BoxShadow::new(px(0.0), px(0.0), theme.surface.into())
                            .spread_radius(px(0.5)),
                    ])
                    .flex()
                    .flex_col()
                    // Composer labels are uniformly system-ui 400 in the
                    // reference (placeholder 14/20; controls 13/18).
                    .font_weight(gpui::FontWeight::NORMAL)
                    .px(px(8.0))
                    .py(px(12.0))
                    .pt(px(if self.prompt_context.files.iter().any(|file| file.image) { 8.0 } else { 14.0 })).pb(px(8.0))
                    .when(!self.prompt_context.files.is_empty(), |d| d.child(self.render_attachments(theme, cx)))
                    .when(!self.review_comments.is_empty(), |d| {
                        d.child(
                            div()
                                .id("composer-review-comments")
                                .role(gpui::Role::Button)
                                .aria_label(crate::i18n::text("查看审查评论"))
                                .focusable()
                                .tab_stop(true)
                                .h(px(28.))
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .px(px(8.))
                                .mb(px(4.))
                                .rounded(px(8.))
                                .bg(theme.text.alpha(0.05))
                                .cursor_pointer()
                                .on_click(
                                    cx.listener(|_, _, _, cx| cx.emit(super::OpenReviewComments)),
                                )
                                .on_key_down(cx.listener(|_, e: &gpui::KeyDownEvent, _, cx| {
                                    if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                        cx.emit(super::OpenReviewComments);
                                        cx.stop_propagation();
                                    }
                                }))
                                .child(
                                    icon("panel-review", theme.text_secondary.into()).size(px(14.)),
                                )
                                .child(
                                    div()
                                        .text_size(px(crate::typography::ui_size(12.)))
                                        .text_color(theme.text_secondary)
                                        .child(crate::i18n::format!("{} 个评论" => "{} comments", self.review_comments.len())),
                                ),
                        )
                    })
                    .child(div().h(px(self.prompt_editor.read(cx).composer_height())).flex_none().mx(px(4.0)).child(self.prompt_editor.clone()))
                    .when(self.dictation_state == DictationState::Idle, |composer| {
                        composer.child(
                            div()
                                .h(px(28.0))
                                .flex_none()
                                .mt(px(4.0))
                                .relative()
                                .top(px(0.0))
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .child(
                                            div()
                                                .id("composer-add-context")
                                                .relative()
                                                .flex_none()
                                                .role(gpui::Role::Button)
                                                .aria_label(crate::i18n::text("添加文件等内容"))
                                                .focusable()
                                                .tab_stop(true)
                                                .on_click(cx.listener(|this, _, window, cx| { this.toggle_context(window, cx); cx.stop_propagation(); }))
                                                    .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, window, cx| { if matches!(e.keystroke.key.as_str(), "enter" | "space" | "down") { this.toggle_context(window, cx); cx.stop_propagation(); } }))
                                                .size(px(28.0))
                                                .rounded_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor_pointer()
                                                .hover(move |style| style.bg(theme.sidebar_hover))
                                                .active(move |style| style.bg(theme.sidebar_hover))
                                                .when(self.context_menu_open, |d| d.bg(theme.sidebar_hover))
                                                .child(
                                                    icon("add", theme.text.into()).size(px(16.0)),
                                                )
                                                .child(canvas(move |bounds,_,_| context_trigger_bounds.set(Some(bounds)),|_,(),_,_|{}).absolute().inset_0().size_full()),
                                        )
                                        .when(self.permission_ui_enabled, |controls| {
                                            controls.child(
                                                div()
                                                    .id("composer-permissions")
                                                    .role(gpui::Role::Button)
                                                    .aria_label(crate::i18n::text("更改权限"))
                                                    .flex_none()
                                                    .h(px(28.0))
                                                    .px(px(8.0))
                                                    .rounded_full()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(4.0))
                                                    .text_size(px(crate::typography::ui_size(13.0)))
                                                    .line_height(px(18.0))
                                                    .text_color(permission_color)
                                                    .cursor_pointer()
                                                    .when(!self.permission_menu_open, |button| {
                                                        button.track_focus(
                                                            &self.permission_menu_focus,
                                                        )
                                                    })
                                                    .on_key_down(
                                                        cx.listener(
                                                            Self::handle_permission_menu_key,
                                                        ),
                                                    )
                                                    .when(self.permission_menu_open, |button| {
                                                        button.bg(theme.sidebar_hover)
                                                    })
                                                    .hover(move |style| {
                                                        style.bg(theme.sidebar_hover)
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _, window, cx| {
                                                            cx.stop_propagation();
                                                            this.menu_open = false;
                                                            this.model_list_open = false;
                                                            this.permission_menu_open =
                                                                !this.permission_menu_open;
                                                            this.permission_menu_keyboard_focus =
                                                                false;
                                                            if this.permission_menu_open {
                                                                window.focus(
                                                                    &this.permission_menu_focus,
                                                                    cx,
                                                                );
                                                            }
                                                            cx.notify();
                                                        }),
                                                    )
                                                    .on_click(cx.listener(|_, _, _, cx| {
                                                        cx.stop_propagation();
                                                    }))
                                                    .child(
                                                        div()
                                                            .size(px(16.0))
                                                            .flex_none()
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .child(icon(
                                                                permission_icon,
                                                                permission_color.into(),
                                                            )),
                                                    )
                                                    .when(!compact, |button| button.child(permission_label))
                                                    .when(self.prompt_context.plan_mode == Some(true) && !compact, |button| button.child(crate::i18n::text(" · 计划"))),
                                            )
                                        })
                                        .children(self.render_goal_chip(theme, cx))
                                        .children(self.render_shell_chip(theme, cx)),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .min_w(px(0.0))
                                        .child(
                                            div()
                                                .id("composer-model-picker")
                                                .debug_selector(|| "MODEL_PICKER_TRIGGER".to_owned())
                                                .relative()
                                                .role(gpui::Role::Button)
                                                .aria_label(crate::i18n::text("选择模型和思考强度"))
                                                .track_focus(&self.model_trigger_focus)
                                                .tab_stop(true)
                                                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                                                    if !this.menu_open && matches!(event.keystroke.key.as_str(), "enter" | "space" | "down") {
                                                        this.menu_open = true;
                                                        this.model_list_open = false;
                                                        this.permission_menu_open = false;
                                                        this.model_menu_focused_item = 0;
                                                        this.model_menu_keyboard_focus = true;
                                                        this.model_menu_focus.focus(window, cx);
                                                        cx.notify();
                                                        cx.stop_propagation();
                                                    }
                                                }))
                                                .h(px(28.0))
                                                .px(px(8.0))
                                                .rounded_full()
                                                .flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .text_size(px(crate::typography::ui_size(13.0)))
                                                .line_height(px(18.0))
                                                .cursor_pointer()
                                                .when(self.menu_open, |button| {
                                                    button.bg(theme.sidebar_hover)
                                                })
                                                .hover(move |style| style.bg(theme.sidebar_hover))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    cx.stop_propagation();
                                                    this.permission_menu_open = false;
                                                    this.permission_menu_keyboard_focus = false;
                                                    this.menu_open = !this.menu_open;
                                                    if !this.menu_open {
                                                        this.model_list_open = false;
                                                        this.model_trigger_focus.focus(window, cx);
                                                    } else {
                                                        this.model_menu_keyboard_focus = false;
                                                        this.model_list_open = false;
                                                        window.focus(&this.model_menu_focus, cx);
                                                    }
                                                    cx.notify();
                                                }))
                                                .child(trigger_value)
                                                .child(canvas(move |bounds, _, _| model_trigger_bounds.set(Some(bounds)), |_, (), _, _| {})
                                                    .absolute().inset_0().size_full())
                                                .child(
                                                    icon(
                                                        "chevron-down",
                                                        theme.text_tertiary.into(),
                                                    )
                                                    .size(px(12.0)),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    div()
                                                        .id("composer-dictation")
                                                        .flex_none()
                                                        .size(px(28.0))
                                                        .rounded_full()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .cursor_pointer()
                                                        .hover(move |style| {
                                                            style.bg(theme.sidebar_hover)
                                                        })
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.start_dictation(cx)
                                                        }))
                                                        .child(
                                                            icon("dictation", theme.text.into())
                                                                .size(px(16.0)),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .id("composer-send")
                                                        .role(gpui::Role::Button)
                                                        .accessibility_id("composer-send")
                                                        .debug_selector(|| "composer-send".to_owned())
                                                        .aria_label(send_action.label())
                                                        .aria_disabled(send_action.busy())
                                                        .aria_busy(send_action.busy())
                                                        .when(send_action.busy(), |button| button.aria_description(crate::i18n::text("正在处理")))
                                                        .focusable()
                                                        .tab_stop(send_action.enabled())
                                                        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                                                this.activate_send_action(cx);
                                                                cx.stop_propagation();
                                                            }
                                                        }))
                                                        .size(px(28.0))
                                                        .flex_none()
                                                        .rounded_full()
                                                        .bg(theme.button)
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .opacity(self.send_opacity.value(cx.background_executor().now()))
                                                        .focus_visible(|style| style.shadow(vec![BoxShadow::new(px(0.0), px(0.0), theme.button.into()).spread_radius(px(2.0))]))
                                                        .when(send_action.enabled(), |button| {
                                                            button.on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.activate_send_action(cx);
                                                                },
                                                            ))
                                                        })
                                                        .when(!send_action.busy(), |button| button.child(
                                                            icon(
                                                                if send_action == super::send::SendAction::Resume {
                                                                    "composer-resume"
                                                                } else if send_action == super::send::SendAction::Stop {
                                                                    "composer-stop"
                                                                } else {
                                                                    "dictation-send"
                                                                },
                                                                theme.button_text.into(),
                                                            )
                                                            .size(px(16.0))
                                                                .debug_selector(|| "composer-send-glyph".to_owned()),
                                                        ))
                                                        .when(send_action.busy(), |button| button.child(
                                                            icon("dictation-spinner", theme.button_text.into())
                                                                .size(px(16.0))
                                                                .debug_selector(|| "composer-send-glyph".to_owned())
                                                                .with_animation("composer-send-busy", Animation::new(std::time::Duration::from_millis(1000)).repeat(), |spinner, progress| {
                                                                    spinner.with_transformation(gpui::Transformation::rotate(gpui::radians(progress * std::f32::consts::TAU)))
                                                                })
                                                        )),
                                                ),
                                        ),
                                ),
                        )
                    })
                    .when(self.dictation_state != DictationState::Idle, |composer| {
                        composer.child(self.dictation_footer(theme, cx))
                    }),
            )
            .when(self.menu_open, |composer| {
                composer.child(deferred(self.model_menu(viewport_width, theme, cx)))
            })
            .when(self.context_menu_open, |composer| composer.child(self.render_context_menu(theme, cx)))
            .when(
                self.permission_ui_enabled && self.permission_menu_open,
                |composer| composer.child(deferred(self.permission_menu(theme, cx))),
            )
            .when(
                self.approval_resolved_capture && self.mode == ThemeMode::Dark,
                |composer| {
                    // CDP 12 was captured while the real model trigger's
                    // tooltip was visible. Keep that interaction state in the
                    // resolved-only fixture instead of changing the live
                    // composer or reusing a synthetic blank crop.
                    composer.child(
                        div()
                            .absolute()
                            .left(px(539.0))
                            .top(px(26.0))
                            .w(px(122.796875))
                            .h(px(32.5625))
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(12.5))
                            .border_1()
                            .border_color(rgba(0xdfdfdfff))
                            .bg(rgba(0xdfdfdfff))
                            .font_family(".SystemUIFont")
                            .text_size(px(crate::typography::ui_size(13.0)))
                            .line_height(px(crate::typography::ui_leading(13.0, 18.5714)))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .text_color(rgba(0x2d2d2dff))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(div().flex_none().child(crate::i18n::text("选择模型")))
                            .child(
                                div()
                                    .w(px(42.0))
                                    .h(px(16.0))
                                    .flex_none()
                                    .rounded(px(6.0))
                                    .bg(rgba(0x2d2d2d1a))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(crate::typography::ui_size(12.0)))
                                    .line_height(px(12.0))
                                    .child("⌃⇧M"),
                            ),
                    )
                },
            )
    }
}

/// The reference's `max-w-40` on the branch value in the home placement.
const BRANCH_VALUE_MAX_WIDTH: f32 = 160.0;
/// `mask-image: linear-gradient(90deg, #000 calc(100% - 1rem), transparent)`,
/// applied only while the value overflows.
const BRANCH_VALUE_FADE: f32 = 16.0;

/// Project control on the home utility bar. A projectless conversation shows
/// the reference's `Choose project` value in the tertiary foreground and has
/// no project to clear.
pub(super) fn project_utility(label: Option<SharedString>, theme: Theme) -> gpui::Stateful<Div> {
    let group = "composer-project-hover";
    let hover_fill = theme.text.alpha(0.05);
    let has_project = label.is_some();
    let foreground = if has_project {
        theme.text
    } else {
        theme.text_tertiary
    };

    div()
        .id("composer-project")
        .debug_selector(|| "composer-project".to_owned())
        .group(group)
        .relative()
        .h(px(28.0))
        .rounded_full()
        .flex_none()
        .child(
            div()
                .h(px(28.0))
                .px(px(9.0))
                .rounded_full()
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_size(px(crate::typography::ui_size(13.0)))
                .line_height(px(18.0))
                .font_weight(gpui::FontWeight::NORMAL)
                .text_color(foreground)
                .cursor_pointer()
                .group_hover(group, move |style| style.bg(hover_fill))
                .child(
                    icon("utility-folder", foreground.into())
                        .size(px(16.0))
                        .when(has_project, |folder| {
                            folder.group_hover(group, |style| style.invisible())
                        }),
                )
                .child(label.unwrap_or_else(|| crate::i18n::text("选择项目").into())),
        )
}

/// Branch control on the home utility bar. The reference renders it only while
/// the working directory's `HEAD` names a branch.
pub(super) fn branch_utility(branch: SharedString, theme: Theme) -> gpui::Stateful<Div> {
    let hover_fill = theme.text.alpha(0.05);

    div()
        .id("composer-branch")
        .debug_selector(|| "composer-branch".to_owned())
        .h(px(28.0))
        .px(px(9.0))
        .rounded_full()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.0))
        .text_size(px(crate::typography::ui_size(13.0)))
        .line_height(px(18.0))
        .font_weight(gpui::FontWeight::NORMAL)
        .text_color(theme.text)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_fill))
        .child(icon("branch", theme.text.into()).size(px(16.0)))
        .child(faded_value(branch, theme.text.into()))
}

/// A single-line value clipped to the branch value's width. GPUI has no
/// `mask-image`, so an overflowing value paints its last 16px as 1px slices of
/// falling opacity, the way the sidebar fades its task titles. The invisible
/// copy gives the box the text's own width up to the clamp.
fn faded_value(text: SharedString, color: Hsla) -> impl IntoElement {
    div()
        .relative()
        .min_w(px(0.0))
        .max_w(px(BRANCH_VALUE_MAX_WIDTH))
        .overflow_hidden()
        .whitespace_nowrap()
        .child(div().invisible().child(text.clone()))
        .child(
            canvas(
                move |bounds, window, _| {
                    let style = window.text_style();
                    let font_size = style.font_size.to_pixels(window.rem_size());
                    let line_height = style.line_height_in_pixels(window.rem_size());
                    let font = style.font();
                    let shape = |alpha: f32| {
                        window.text_system().shape_line(
                            text.clone(),
                            font_size,
                            &[TextRun {
                                len: text.len(),
                                font: font.clone(),
                                color: color.alpha(color.a * alpha),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        )
                    };
                    let opaque = shape(1.0);
                    let fade = (opaque.width > bounds.size.width).then(|| {
                        (0..BRANCH_VALUE_FADE as usize)
                            .map(|index| shape(1.0 - (index as f32 + 0.5) / BRANCH_VALUE_FADE))
                            .collect::<Vec<_>>()
                    });
                    (opaque, fade, line_height)
                },
                move |bounds, (opaque, fade, line_height), window, cx| {
                    let paint = |line: &gpui::ShapedLine,
                                 mask: Bounds<Pixels>,
                                 window: &mut Window,
                                 cx: &mut gpui::App| {
                        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                            line.paint(
                                bounds.origin,
                                line_height,
                                TextAlign::Left,
                                None,
                                window,
                                cx,
                            )
                            .expect("branch value glyphs should paint")
                        });
                    };
                    let Some(fade) = fade else {
                        paint(&opaque, bounds, window, cx);
                        return;
                    };
                    let fade_start = bounds.right() - px(BRANCH_VALUE_FADE);
                    paint(
                        &opaque,
                        Bounds::from_corners(bounds.origin, point(fade_start, bounds.bottom())),
                        window,
                        cx,
                    );
                    for (index, line) in fade.iter().enumerate() {
                        let x = fade_start + px(index as f32);
                        paint(
                            line,
                            Bounds::new(
                                point(x, bounds.origin.y),
                                size(px(1.0), bounds.size.height),
                            ),
                            window,
                            cx,
                        );
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
}

pub(super) fn context_toolbar(
    composer: &ComposerView,
    theme: Theme,
    cx: &mut Context<ComposerView>,
) -> Div {
    let branch = composer
        .workspace
        .checkout
        .as_ref()
        .and_then(|checkout| checkout.branch())
        .map(|branch| SharedString::from(branch.to_owned()));
    let branch = if composer.workspace_controls.worktree_mode
        && composer.workspace_controls.worktree_cwd.is_none()
    {
        composer
            .workspace_controls
            .base
            .clone()
            .map(SharedString::from)
            .or(branch)
    } else {
        branch
    };
    div()
        .relative()
        .debug_selector(|| "composer-toolbar".to_owned())
        .h(px(38.0))
        .mx(px(13.0))
        // The reference's action bar is 42px tall: its 38px row (`py-row`,
        // 5px above and below a 28px control) ends where the Composer begins,
        // and the last 4px run underneath it. Keeping the extension in a
        // separate background layer leaves the row itself centred on the 38px
        // that stays visible, so a control's hover plate has 5px on both
        // sides; shifting the bar and the row down together left 34px visible
        // and pushed the plate against the Composer.
        .child(
            div()
                .absolute()
                .debug_selector(|| "composer-toolbar-strip".to_owned())
                .top_0()
                .left_0()
                .w_full()
                .h(px(42.0))
                // The bottom edge joins the input; only the exposed top corners round.
                .rounded_t(px(16.0))
                .bg(theme.surface_under),
        )
        .child(
            div()
                .relative()
                .h(px(38.0))
                .w_full()
                .px(px(6.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(composer.project_trigger(theme, cx))
                .child(composer.location_trigger(theme, cx))
                // Although the HTML includes a trailing `px-0` class, the
                // home-placement `px-2` rule is emitted later in the bundled
                // stylesheet and wins the cascade. The computed button has
                // 8px inline padding plus a 1px transparent border per side.
                .when_some(branch, |toolbar, branch| {
                    toolbar.child(composer.branch_trigger(branch, theme, cx))
                }),
        )
}

impl Render for ComposerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let action = self.send_action(cx);
        if self
            .send_opacity
            .update(action, cx.background_executor().now(), cx.reduce_motion())
        {
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }
        if self.menu_open && self.model_ultra_warning_visible() {
            window.request_animation_frame();
        }
        if self.focus_prompt_pending {
            self.prompt_focus_handle(cx).focus(window, cx);
            self.focus_prompt_pending = false;
        }
        self.render_composer(
            f32::from(window.viewport_size().width),
            Theme::for_mode(self.mode),
            cx,
        )
    }
}
