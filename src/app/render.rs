//! Render behavior and presentation for the application shell.

use std::path::PathBuf;

use gpui::{
    Animation, AnimationExt, BoxShadow, Context, Div, Focusable, IntoElement, KeyDownEvent,
    MouseButton, ObjectFit, Render, Role, StyleRefinement, Transformation, Window, canvas, div,
    hsla, linear_color_stop, linear_gradient, point, prelude::*, px, radians, rgba,
};
use unicode_segmentation::UnicodeSegmentation;

use super::{right_panel::clamp_right_panel_width, state::RightPanelMode};
#[cfg(not(test))]
use crate::workspace::WorkspaceSnapshot;
use crate::{
    agent::{AgentAccountLoginPhase, AgentLoginChallenge},
    components::{account::AccountDialog, file_panel::OpenWorkspaceFile, icons::icon},
    theme::{CHAT_CONTENT_HORIZONTAL_GUTTER, Theme, ThemeMode, ui_font},
};

use super::{
    ChatApp, ConversationKey, DismissPermissionUi, LEADING_TITLEBAR_CONTROLS_TOP, OpenFiles,
    OpenSideChat, RIGHT_PANEL_MIN_WIDTH, STARTUP_LOADING_BLINK_DURATION, STARTUP_LOADING_LOGO_SIZE,
    ToggleReview, ToggleTerminal,
};

pub(super) fn panel_resize_handle(
    id: &'static str,
    left: f32,
    line_visible: bool,
    theme: Theme,
    input_layer: impl IntoElement,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(left))
        .w(px(16.0))
        .cursor_col_resize()
        .child(input_layer)
        .when(line_visible, |handle| {
            handle.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(7.5))
                    .w(px(1.0))
                    .flex()
                    .flex_col()
                    .child(div().flex_1().w_full().bg(linear_gradient(
                        0.0,
                        linear_color_stop(theme.text.alpha(0.0), 0.0),
                        linear_color_stop(theme.text.alpha(0.25), 1.0),
                    )))
                    .child(div().flex_1().w_full().bg(linear_gradient(
                        0.0,
                        linear_color_stop(theme.text.alpha(0.25), 0.0),
                        linear_color_stop(theme.text.alpha(0.0), 1.0),
                    ))),
            )
        })
}

pub(super) fn titlebar_interaction_area() -> impl IntoElement {
    div()
        .id("titlebar-interaction-area")
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(px(46.0))
        .on_click(|event, window, _| {
            if event.click_count() == 2 {
                window.zoom_window();
            }
        })
}

pub(super) fn startup_loading_logo_opacity(progress: f32) -> f32 {
    let blink = ((progress.clamp(0.0, 1.0) * std::f32::consts::TAU).cos() + 1.0) * 0.5;
    0.32 + blink * 0.68
}

#[cfg(not(test))]
pub(super) fn startup_sidebar_resolved(snapshot: &WorkspaceSnapshot) -> bool {
    !snapshot.loading.projects && !snapshot.loading.recent && !snapshot.loading.pinned
}

pub(super) fn startup_loading_view(theme: Theme) -> impl IntoElement {
    let logo = icon("home-mark", theme.home_mark.into())
        .size(px(STARTUP_LOADING_LOGO_SIZE))
        .with_animation(
            "startup-loading-logo-blink",
            Animation::new(STARTUP_LOADING_BLINK_DURATION).repeat(),
            |logo, progress| logo.opacity(startup_loading_logo_opacity(progress)),
        );

    div()
        .id("startup-loading-screen")
        .role(Role::ProgressIndicator)
        .aria_label(crate::i18n::text("GPUI 正在加载"))
        .size_full()
        .relative()
        .child(
            div()
                .size_full()
                .bg(theme.sidebar_surface)
                .flex()
                .items_center()
                .justify_center()
                .child(logo),
        )
        .child(titlebar_interaction_area())
}

pub(super) fn titlebar_icon_button(
    name: &'static str,
    disabled: bool,
    active: bool,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    let glyph = icon(name, theme.text_tertiary.into())
        .size(px(16.0))
        .when(name == "right-sidebar", |glyph| {
            glyph.with_transformation(Transformation::rotate(radians(std::f32::consts::PI)))
        });

    div()
        .id(name)
        .size(px(28.0))
        .flex_none()
        .rounded(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .when(active, |button| button.bg(theme.text.alpha(0.05)))
        .when(disabled, |button| button.opacity(0.4).cursor_default())
        .when(!disabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(theme.sidebar_hover))
                .active(move |style| style.bg(theme.sidebar_hover))
        })
        // The reference SVG declares 20x20, but its `icon-xs` class wins in
        // computed style and renders the glyph at 16x16.
        .child(glyph)
}

pub(super) fn permission_risk_row(
    icon_name: &'static str,
    title: &'static str,
    detail: &'static str,
    separated: bool,
    theme: Theme,
) -> Div {
    div()
        .h(px(51.0))
        .mx(px(16.0))
        .when(separated, |row| row.border_t_1().border_color(theme.border))
        .flex()
        .items_center()
        // Preserve the reference's authored multicolor fills.
        .child(gpui::img(format!("icons/{icon_name}.svg")).size(px(24.0)))
        .child(
            div()
                .ml(px(12.0))
                .flex_1()
                .flex()
                .flex_col()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .child(div().text_color(theme.markdown_text).child(title))
                .child(div().text_color(theme.text_tertiary).child(detail)),
        )
}

impl Render for ChatApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.account_focus_pending {
            if self.account.dialog.is_some() {
                self.account_focus.focus(window, cx);
            }
            self.account_focus_pending = false;
        }
        if self.permission_confirmation_focus_pending {
            if self.permission_confirmation_open {
                self.permission_confirmation_focus.focus(window, cx);
            } else {
                self.root_focus.focus(window, cx);
            }
            self.permission_confirmation_focus_pending = false;
        }
        let review_overlay =
            if self.right_panel.open && self.right_panel.mode == Some(RightPanelMode::Review) {
                self.review_panels
                    .get(&self.active_conversation)
                    .and_then(|p| p.update(cx, |p, cx| p.render_overlay(cx)))
            } else {
                None
            };
        let side_chat_overlay = if self.right_panel.open
            && self.right_panel.mode == Some(RightPanelMode::SideChat)
        {
            self.side_chat_panels
                .get(&self.active_conversation)
                .and_then(|panel| panel.update(cx, |panel, cx| panel.render_overlay(window, cx)))
        } else {
            None
        };
        if self.terminal_return_focus_pending {
            if let Some(host) = self.conversation_hosts.get(&self.active_conversation) {
                host.composer
                    .read(cx)
                    .prompt_focus_handle(cx)
                    .focus(window, cx);
            }
            self.terminal_return_focus_pending = false;
        }
        let viewport = window.viewport_size();
        if window.focused(cx).is_none() {
            self.root_focus.focus(window, cx);
        }
        let theme = Theme::for_window(
            self.mode,
            window.is_window_active(),
            f32::from(viewport.width),
            f32::from(viewport.height),
            window.scale_factor(),
        );
        if !(self.startup_model_catalog_resolved
            && self.startup_sidebar_resolved
            && self.startup_minimum_duration_elapsed)
        {
            return startup_loading_view(theme).into_any_element();
        }
        if self.image_preview.path.is_some() && !self.image_preview.focus_active {
            self.image_preview.previous_focus = window.focused(cx);
            self.image_preview.focus.focus(window, cx);
            self.image_preview.focus_active = true;
        } else if self.image_preview.path.is_none() && self.image_preview.focus_active {
            if let Some(previous) = self.image_preview.previous_focus.take() {
                previous.focus(window, cx);
            }
            self.image_preview.focus_active = false;
        }
        if self.project_creation.open && self.project_creation.focus_pending {
            self.project_creation.focus.focus(window, cx);
            self.project_creation.focus_pending = false;
        }
        if self.right_panel.open && self.right_panel.focus_pending {
            self.right_panel.focus.focus(window, cx);
            self.right_panel.focus_pending = false;
        }
        let sidebar_width = self.sidebar.read(cx).width();
        // The rename panel is a window-level dialog: the sidebar owns the task
        // it belongs to, the shell paints the scrim and the card.
        let thread_rename_panel = self.sidebar.read(cx).thread_rename();
        let thread_rename_input = self.sidebar.read(cx).thread_rename_input();
        let rename_field_focused = thread_rename_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let sidebar_reveal = self.sidebar_layout.reveal.clamp(0.0, 1.0);
        let revealed_sidebar_width = sidebar_width * sidebar_reveal;
        let resumed_title = match &self.active_conversation {
            ConversationKey::Thread(id) if !self.showing_settings => {
                self.workspace_store.snapshot().thread(id).map(|thread| {
                    (
                        thread.title.clone(),
                        crate::workspace::project_id_for_thread(
                            thread,
                            &self.workspace_store.snapshot().projects,
                        )
                        .is_some(),
                    )
                })
            }
            _ => None,
        };
        // CDP at both 2560×1410 and the project's 1440×900 target showed a
        // persisted 1418.21875 px panel, clamped to leave the main thread at
        // its measured 773.09375 px right edge on narrower windows.
        let viewport_width = f32::from(window.viewport_size().width);
        let default_right_panel_width = (window.viewport_size().width
            - px(if self.right_panel.mode == Some(RightPanelMode::Review) {
                759.66406
            } else {
                773.09375
            }))
        .min(px(1_418.218_8))
        .max(px(RIGHT_PANEL_MIN_WIDTH));
        let review_fullscreen = self.right_panel.open
            && matches!(
                self.right_panel.mode,
                Some(RightPanelMode::Review | RightPanelMode::SideChat)
            )
            && self.right_panel.fullscreen;
        let right_panel_width = if review_fullscreen {
            px(viewport_width - revealed_sidebar_width)
        } else {
            px(clamp_right_panel_width(
                self.right_panel
                    .width
                    .unwrap_or(f32::from(default_right_panel_width)),
                viewport_width,
                revealed_sidebar_width,
            ))
        };
        div()
            .id(if self.showing_settings {
                "app-shell-settings"
            } else {
                "app-shell"
            })
            .size_full()
            .track_focus(&self.root_focus)
            .key_context(if self.showing_settings {"Settings"} else {"ChatApp"})
            .on_action(cx.listener(|this,_:&super::NextSettingsControl,window,cx|{this.settings.update(cx,|settings,cx|settings.advance_focus(false,window,cx));cx.stop_propagation();}))
            .on_action(cx.listener(|this,_:&super::PreviousSettingsControl,window,cx|{this.settings.update(cx,|settings,cx|settings.advance_focus(true,window,cx));cx.stop_propagation();}))
            .relative()
            .flex()
            .font(ui_font())
            .on_click(cx.listener(|this, _, _, cx| {
                this.home.update(cx, |home, cx| home.close_model_picker(cx));
                this.sidebar
                    .update(cx, |sidebar, cx| sidebar.close_transient_menus(cx));
                if this.right_panel.subagent_menu_open {
                    this.right_panel.subagent_menu_open = false;
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &OpenFiles, _, cx| {
                this.open_files(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this,_:&crate::components::file_panel::OpenWorkspaceReview,_,cx|{this.right_panel.open=true;this.select_right_panel_item(4,cx);cx.stop_propagation();}))
            .on_action(cx.listener(|this, event: &OpenWorkspaceFile, _, cx| {
                this.open_files(cx);
                this.file_panels[&this.active_conversation].update(cx, |p,cx| p.open_path(PathBuf::from(&event.path),event.line,cx));
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ToggleTerminal, _, cx| {
                if this.right_panel.open && this.right_panel.mode == Some(RightPanelMode::Terminal) { this.close_right_panel(cx); }
                else { this.right_panel.open = true; this.select_right_panel_item(2, cx); }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ToggleReview, _, cx| {
                if this.right_panel.open && this.right_panel.mode == Some(RightPanelMode::Review) { this.close_right_panel(cx); }
                else { this.open_review(cx); }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &OpenSideChat, _, cx| {
                this.right_panel.open = true;
                this.select_right_panel_item(0, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::components::side_chat::RestoreSideChat, _, cx| {
                this.deactivate_review(cx);
                this.right_panel.open = true;
                this.right_panel.mode = Some(RightPanelMode::SideChat);
                this.right_panel.fullscreen = false;
                this.right_panel.diff_review = None;
                this.ensure_side_chat(false, cx);
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(Self::handle_project_creation_key))
            .on_action(cx.listener(|this,_:&super::OpenSettingsPage,_,cx|{this.open_settings(cx);cx.stop_propagation();}))
            .on_action(cx.listener(|_,_:&super::CaptureFrame,_window,_cx| {
                #[cfg(feature = "screenshot")]
                if let Ok(path) = std::env::var("GPUI_CAPTURE_OUTPUT") { crate::capture_frame(_window,path,3); }
            }))
            .on_action(cx.listener(|this, _: &DismissPermissionUi, window, cx| {
                // The rename dialog is modal and owns Escape while it is up:
                // the global binding is what actually receives the key, so the
                // dialog is dismissed here rather than through its own
                // `ThreadRename` context.
                if this.sidebar.read(cx).thread_rename().is_some() {
                    this.dismiss_thread_rename(cx);
                    cx.stop_propagation();
                    return;
                }
                if this.showing_pull_requests {
                    let fullscreen = this.pull_requests.read(cx).is_fullscreen();
                    if fullscreen {
                        this.pull_requests.update(cx, |view, cx| view.set_fullscreen(false, cx));
                        cx.stop_propagation();
                        return;
                    }
                    if this.pull_requests.update(cx, |view, cx| view.dismiss_menus(cx)) {
                        cx.stop_propagation();
                        return;
                    }
                }
                if this.showing_settings {
                    this.settings.update(cx, |settings, cx| settings.dismiss_transient(window, cx));
                    cx.stop_propagation(); return;
                }
                if this.permission_confirmation_open {
                    this.resolve_permission_confirmation(false, window, cx);
                    cx.stop_propagation(); return;
                }
                if this.right_panel.open && this.right_panel.mode == Some(RightPanelMode::SideChat)
                    && let Some(panel) = this.side_chat_panels.get(&this.active_conversation)
                    && panel.update(cx, |panel, cx| panel.dismiss_transient(cx)) {
                    cx.stop_propagation(); return;
                }
                if this.right_panel.open&&this.right_panel.mode==Some(RightPanelMode::Review)
                    &&let Some(panel)=this.review_panels.get(&this.active_conversation)
                    &&panel.update(cx,|p,cx|p.dismiss_transient(window,cx)) {
                    cx.stop_propagation();return;
                }
                if this.image_preview.path.is_some() {
                    this.image_preview.path = None;
                    this.image_preview.dimensions = None;
                    this.image_preview.zoom = 1.0;
                    cx.stop_propagation();
                    cx.notify();
                } else if this.project_creation.open {
                    this.close_project_creation(cx);
                } else {
                    this.home.update(cx, |home, cx| { home.close_model_picker(cx); home.dismiss_plan_popovers(cx); home.dismiss_hook_tooltips(cx); });
                }
            }))
            .when(self.showing_settings, |shell| {
                shell.child(self.settings.clone())
            })
            .when(!self.showing_settings, |shell| {
                shell
                    .child(
                        div()
                            .w(px(revealed_sidebar_width))
                            .min_w(px(revealed_sidebar_width))
                            .h_full()
                            .flex_none()
                            .overflow_hidden()
                            // Electron paints the translucent surface on the
                            // outer aside; only its inner contents fade while
                            // the panel collapses.
                            .bg(theme.sidebar_surface)
                            .child(
                                div()
                                    .w(px(sidebar_width))
                                    .min_w(px(sidebar_width))
                                    .h_full()
                                    // Avoid putting the settled sidebar foreground
                                    // through an opacity context. On a translucent
                                    // window, text already uses grayscale AA; an
                                    // additional alpha blend makes glyph and SVG
                                    // edges look soft over bright backgrounds.
                                    .when(sidebar_reveal < 1.0, |content| {
                                        content.opacity(sidebar_reveal)
                                    })
                                    .child(self.sidebar.clone()),
                            ),
                    )
                    // Sidebar scrolling dirties its ancestor view by design. Keep the
                    // much larger, static home/composer subtree cached so a wheel or
                    // trackpad frame does not rebuild and repaint the main pane.
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .w_full()
                                    .min_h(px(0.0))
                                    .flex_1()
                                    .flex()
                                    .when(!review_fullscreen && self.showing_pull_requests, |row| {
                                        // The Pull Requests page owns the whole content
                                        // area: the reference keeps its list/detail
                                        // panes flush with the workspace edges.
                                        row.child(
                                            div()
                                                .flex_1()
                                                .min_w(px(0.0))
                                                .h_full()
                                                .bg(theme.surface)
                                                .child(self.pull_requests.clone()),
                                        )
                                    })
                                    .when(!review_fullscreen && !self.showing_pull_requests,|row|row.child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .h_full()
                                            // Keep the conversation surface away from both
                                            // workspace edges when a side panel narrows the main
                                            // column. Max-width content remains unchanged on wide
                                            // windows because HomeView still centers it internally.
                                            .px(px(CHAT_CONTENT_HORIZONTAL_GUTTER))
                                            .bg(theme.surface)
                                            .child(
                                                if self.home.read(cx).needs_live_interaction_render(cx) {
                                                    // Interactive activity focus, AX nodes and text selection
                                                    // must remain registered on every frame.
                                                    self.home.clone().into_any_element()
                                                } else {
                                                    self.home.clone().cached(
                                                        StyleRefinement::default().size_full(),
                                                    ).into_any_element()
                                                },
                                            ),
                                    ))
                                    .when(self.right_panel.open, |row| {
                                        row.child(self.right_panel(
                                            right_panel_width,
                                            theme,
                                            cx,
                                        ))
                                    }),
                            ),
                    )
            })
            .when_some(review_overlay,|shell,overlay|shell.child(overlay))
            .when_some(side_chat_overlay,|shell,overlay|shell.child(overlay))
            .when(self.permission_confirmation_open, |shell| {
                shell.child(
                    div()
                        .id("permission-confirmation-overlay")
                        .absolute()
                        .inset_0()
                        // Electron computed style: #00000022.
                        .bg(rgba(0x00000022))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                        .child(
                            div()
                                .id("permission-confirmation-dialog")
                                .role(gpui::Role::Dialog).aria_label(crate::i18n::text("要开启完整访问权限吗？"))
                                .track_focus(&self.permission_confirmation_focus)
                                .on_key_down(cx.listener(Self::permission_confirmation_key))
                                .w(px(520.0))
                                .h(px(376.6875))
                                .rounded(px(25.0))
                                .border(px(0.5))
                                .border_color(theme.border)
                                .bg(theme.model_picker_surface)
                                .shadow(vec![
                                    BoxShadow::new(px(0.0), px(4.0), hsla(0.0, 0.0, 0.0, 0.10))
                                        .blur_radius(px(8.0))
                                        .spread_radius(px(-2.0)),
                                ])
                                .p(px(20.0))
                                .flex()
                                .flex_col()
                                .text_color(theme.markdown_text)
                                .child(
                                    div()
                                        .h(px(28.0))
                                        .flex()
                                        .items_start()
                                        .gap(px(8.0))
                                        .child(icon("permission-warning", theme.markdown_text.into()).size(px(20.0)))
                                        .child(
                                            div()
                                                .text_size(px(20.0))
                                                .line_height(px(24.0))
                                                .font_weight(gpui::FontWeight(600.0))
                                                .child(crate::i18n::text("要开启完整访问权限吗？")),
                                        ),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .text_size(px(14.0))
                                        .line_height(px(21.0))
                                        .text_color(theme.text_tertiary)
                                        .child(crate::i18n::text("Codex 将能够在未经您许可的情况下，在这台计算机上的任何位置运行命令、使用互联网，以及创建和编辑文件。这包括但不限于：")),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .h(px(162.0))
                                        .rounded(px(17.0))
                                        .bg(theme.elevated)
                                        .child(permission_risk_row("permission-dialog-folder", crate::i18n::text("文件和文件夹"), crate::i18n::text("读取、创建、修改、上传或删除此计算机上任意位置的文件"), false, theme))
                                        .child(permission_risk_row("permission-dialog-terminal", crate::i18n::text("终端命令"), crate::i18n::text("运行命令、安装软件和更改系统设置"), true, theme))
                                        .child(permission_risk_row("permission-dialog-internet", crate::i18n::text("互联网和已连接的应用"), crate::i18n::text("访问网站、发送数据并使用已启用的插件"), true, theme)),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .h(px(21.0))
                                        .flex()
                                        .items_center()
                                        .text_size(px(14.0))
                                        .line_height(px(21.0))
                                        .text_color(theme.text_tertiary)
                                        .child(div().flex_1().child(crate::i18n::text("这会带来敏感数据丢失或泄露、提示注入等风险。你可以将其关闭。")))
                                        .child(div().text_color(theme.accent).child(crate::i18n::text("了解更多"))),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .h(px(36.0))
                                        .flex()
                                        .justify_end()
                                        .gap(px(12.0))
                                        .child(
                                            div()
                                                .id("permission-confirmation-cancel")
                                                .role(gpui::Role::Button).aria_label(crate::i18n::text("取消"))
                                                .when(self.permission_confirmation_keyboard&&self.permission_confirmation_choice==0,|button|button.aria_active_descendant().shadow(vec![BoxShadow::new(px(0.),px(0.),theme.accent.into()).spread_radius(px(2.))]))
                                                .h(px(36.0))
                                                .px(px(20.0))
                                                .rounded_full()
                                                .bg(theme.text.alpha(0.05))
                                                .flex()
                                                .items_center()
                                                .text_size(px(14.0))
                                                .cursor_pointer()
                                                .hover(move |style| style.bg(theme.text.alpha(0.10)))
                                                .on_click(cx.listener(|this,_,window,cx|this.resolve_permission_confirmation(false,window,cx)))
                                                .child(crate::i18n::text("取消")),
                                        )
                                        .child(
                                            div()
                                                .id("permission-confirmation-confirm")
                                                .role(gpui::Role::Button).aria_label(crate::i18n::text("确认"))
                                                .when(self.permission_confirmation_keyboard&&self.permission_confirmation_choice==1,|button|button.aria_active_descendant().shadow(vec![BoxShadow::new(px(0.),px(0.),theme.accent.into()).spread_radius(px(2.))]))
                                                .h(px(36.0))
                                                .px(px(20.0))
                                                .rounded_full()
                                                .bg(rgba(0xff67641a))
                                                .flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .text_size(px(14.0))
                                                .text_color(rgba(0xff6764ff))
                                                .cursor_pointer()
                                                .hover(|style| style.bg(rgba(0xff676433)))
                                                .on_click(cx.listener(|this,_,window,cx|this.resolve_permission_confirmation(true,window,cx)))
                                                .child(icon("permission-warning", rgba(0xff6764ff).into()).size(px(16.0)))
                                                .child(crate::i18n::text("确认")),
                                        ),
                                ),
                        ),
                )
            })
            .when(self.account.dialog == Some(AccountDialog::Logout), |shell| {
                shell.child(account_dialog_overlay(
                    self.account_logout_overlay(theme, cx),
                ))
            })
            .when(self.account.dialog == Some(AccountDialog::Login), |shell| {
                shell.child(account_dialog_overlay(
                    self.account_login_overlay(theme, cx),
                ))
            })
            .when_some(thread_rename_panel, |shell, _panel| {
                shell.child(thread_rename_overlay(
                    thread_rename_input,
                    self.mode,
                    rename_field_focused,
                    theme,
                    cx,
                ))
            })
            .when_some(resumed_title.filter(|_| !review_fullscreen), |shell, (title, in_project)| {
                // The resumed thread has its own opaque sticky header. Paint
                // it over the virtual list's overdraw band, just as Electron
                // masks scrolling Markdown beneath its 46px titlebar.
                shell.child(div()
                    .id("resumed-thread-header")
                    .absolute().top_0().left(px(revealed_sidebar_width))
                    .right(if self.right_panel.open { right_panel_width } else { px(0.0) })
                    .h(px(46.0)).bg(theme.surface).border_b_1().border_color(theme.border)
                    .pl(px(if sidebar_reveal < 0.5 { 184.0 } else { 14.0 })).pr(px(100.0))
                    .flex().items_center().gap(px(12.0))
                        .text_size(px(14.0)).line_height(px(20.0)).font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                    .when(in_project, |header| header.child(icon("folder", theme.text.into()).size(px(16.0)).flex_none()))
                    .child(div().min_w(px(0.0)).truncate().child(title)))
            })
            .when(!self.showing_settings && sidebar_reveal == 1.0, |shell| {
                shell.child(self.sidebar_resize_handle(theme, revealed_sidebar_width, cx))
            })
            // Keep the draggable titlebar behind its interactive controls so
            // their 28px hover hit areas receive pointer events.
            .child(titlebar_interaction_area())
            .when(!self.showing_settings, |shell| {
                shell
                    .child(
                        div()
                            .absolute()
                            .top(px(LEADING_TITLEBAR_CONTROLS_TOP))
                            .left(px(88.0))
                            .flex()
                            .gap(px(4.0))
                            .child(
                                titlebar_icon_button("sidebar-toggle", false, false, theme).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.toggle_sidebar(window, cx);
                                    }),
                                ),
                            )
                            .child(titlebar_icon_button("back", false, false, theme))
                            // The captured reference has no forward history, so this
                            // control is intentionally disabled and 40% opaque.
                            .child(titlebar_icon_button("forward", true, false, theme)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(9.0))
                            .right(px(8.0))
                            .flex()
                            .gap(px(6.0))
                            // The reference Pull Requests page owns the whole
                            // content area and shows no panel controls there.
                            .when(!self.showing_pull_requests, |controls| {
                                controls
                            .child(
                                titlebar_icon_button(
                                    "right-sidebar",
                                    false,
                                    self.right_panel.open,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation()
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_right_panel(cx);
                                })),
                            )
                            }),
                    )
            })
            .when(self.project_creation.open, |shell| {
                shell.child(self.project_creation_overlay(theme, cx))
            })
            .when(self.chat_search.read(cx).is_open(), |shell| {
                shell.child(self.chat_search.clone())
            })
            .when_some(self.image_preview.path.clone(), |shell, path| {
                let viewport = window.viewport_size();
                let zoom = self.image_preview.zoom;
                let image_width = (f32::from(viewport.width) - 64.0).max(160.0) * zoom;
                let image_height = (f32::from(viewport.height) - 128.0).max(120.0) * zoom;
                let percentage = format!("{}%", (zoom * 100.0).round() as i32);
                let preview_dimensions = self.image_preview.dimensions;
                shell.child(
                    div()
                        .id("image-preview-dialog")
                        .track_focus(&self.image_preview.focus)
                        .role(Role::Dialog)
                        .aria_label(crate::i18n::text("图片预览"))
                        .absolute()
                        .inset_0()
                        .bg(theme.surface)
                        .overflow_hidden()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.image_preview.path = None;
                            this.image_preview.dimensions = None;
                            this.image_preview.zoom = 1.0;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .id("image-preview-image-scroll")
                                .absolute()
                                .inset_0()
                                .pt(px(48.0))
                                .pb(px(80.0))
                                .px(px(32.0))
                                .overflow_scroll()
                                .flex()
                                .items_center()
                                .justify_center()
                                .on_click(|_, _, cx| cx.stop_propagation())
                                .child(
                                    gpui::img(path.clone())
                                        .w(px(image_width))
                                        .h(px(image_height))
                                        .flex_none()
                                        .rounded(px(12.5))
                                        .object_fit(ObjectFit::Contain),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(12.0))
                                .right(px(12.0))
                                .flex()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .id("image-preview-open-original")
                                        .h(px(40.0))
                                        .min_w(px(40.0))
                                        .px(px(12.0))
                                        .rounded_full()
                                        .bg(theme.model_picker_surface.alpha(0.95))
                                        .shadow(vec![
                                            BoxShadow::new(
                                                px(0.0),
                                                px(2.0),
                                                rgba(0x00000014).into(),
                                            )
                                            .blur_radius(px(4.0))
                                            .spread_radius(px(-1.0)),
                                        ])
                                        .role(Role::Button)
                                        .aria_label(crate::i18n::text("下载图片"))
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(move |button| button.bg(theme.elevated))
                                        .on_click({
                                            let path = path.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.download_preview_image(path.clone(), cx);
                                                cx.stop_propagation();
                                            })
                                        })
                                        .on_key_down({
                                            let path = path.clone();
                                            cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                                if matches!(
                                                    event.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    this.download_preview_image(path.clone(), cx);
                                                    cx.stop_propagation();
                                                }
                                            })
                                        })
                                        .child(
                                            icon("image-download", theme.text.into()).size(px(20.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("image-preview-close")
                                        .h(px(40.0))
                                        .min_w(px(40.0))
                                        .px(px(12.0))
                                        .rounded_full()
                                        .bg(theme.model_picker_surface.alpha(0.95))
                                        .shadow(vec![
                                            BoxShadow::new(
                                                px(0.0),
                                                px(2.0),
                                                rgba(0x00000014).into(),
                                            )
                                            .blur_radius(px(4.0))
                                            .spread_radius(px(-1.0)),
                                        ])
                                        .role(Role::Button)
                                        .aria_label(crate::i18n::text("关闭图片预览"))
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(move |button| button.bg(theme.elevated))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.image_preview.path = None;
                                            this.image_preview.dimensions = None;
                                            this.image_preview.zoom = 1.0;
                                            cx.stop_propagation();
                                            cx.notify();
                                        }))
                                        .on_key_down(cx.listener(
                                            |this, event: &KeyDownEvent, _, cx| {
                                                if matches!(
                                                    event.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    this.image_preview.path = None;
                                                    this.image_preview.dimensions = None;
                                                    this.image_preview.zoom = 1.0;
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                }
                                            },
                                        ))
                                        .child(icon("close-dialog", theme.text.into()).size(px(21.0))),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .bottom(px(32.0))
                                .left_0()
                                .right_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .id("image-preview-zoom-controls")
                                        .h(px(36.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .on_click(|_, _, cx| cx.stop_propagation())
                                        .when_some(preview_dimensions, |controls, (width, height)| {
                                            controls.child(
                                                div()
                                                    .px(px(10.0))
                                                    .text_size(px(13.0))
                                                    .text_color(theme.text_secondary)
                                                    .child(format!("{width} × {height}")),
                                            )
                                        })
                                        .child(
                                            div()
                                                .id("image-preview-zoom-out")
                                                .size(px(36.0))
                                                .rounded_full()
                                                .bg(theme.text.alpha(0.10))
                                                .role(Role::Button)
                                                .aria_label(crate::i18n::text("缩小图片"))
                                                .focusable()
                                                .tab_stop(true)
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(20.0))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.image_preview.zoom =
                                                        (this.image_preview.zoom - 0.25).max(0.5);
                                                    cx.notify();
                                                }))
                                                .child("−"),
                                        )
                                        .child(
                                            div()
                                                .w(px(56.0))
                                                .text_center()
                                                .text_size(px(13.0))
                                                .text_color(theme.text)
                                                .child(percentage),
                                        )
                                        .child(
                                            div()
                                                .id("image-preview-zoom-in")
                                                .size(px(36.0))
                                                .rounded_full()
                                                .bg(theme.text.alpha(0.10))
                                                .role(Role::Button)
                                                .aria_label(crate::i18n::text("放大图片"))
                                                .focusable()
                                                .tab_stop(true)
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(20.0))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.image_preview.zoom =
                                                        (this.image_preview.zoom + 0.25).min(3.0);
                                                    cx.notify();
                                                }))
                                                .child("+"),
                                        ),
                                ),
                        ),
                )
            })
            .when_some(self.plan_export_error.clone(), |root, error| root.child(div().id("plan-export-error").role(Role::Alert).absolute().bottom(px(24.0)).right(px(24.0)).max_w(px(400.0)).p(px(12.0)).rounded(px(12.0)).bg(theme.surface).border_1().border_color(theme.border).text_color(theme.text).child(error)))
            .when(crate::assets::status().is_missing(), |root| {
                // A missing asset base blanks every icon without failing the
                // render. Say so on screen: a screenshot has to show it, not
                // only the log.
                let status = crate::assets::status();
                root.child(
                    div()
                        .id("assets-missing-banner")
                        .role(Role::Alert)
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(28.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .bg(rgba(0xb3261eff))
                        .text_size(px(12.0))
                        .text_color(rgba(0xffffffff))
                        .child(crate::i18n::format!(
                            "assets 缺失，图标不会渲染：{detail}" =>
                                "assets missing; icons will not render: {detail}",
                            detail = status.tried_summary()
                        )),
                )
            })
            .into_any_element()
    }
}

/// Dimmed surface that centers an account dialog. Clicking the scrim is not a
/// dismissal: a destructive confirmation still requires an explicit answer.
fn account_dialog_overlay(dialog: impl IntoElement) -> impl IntoElement {
    let mut overlay = div()
        .id("account-dialog-overlay")
        .absolute()
        .inset_0()
        .bg(rgba(0x00000022))
        .flex()
        .items_center()
        .justify_center()
        .on_click(|_, _, cx: &mut gpui::App| cx.stop_propagation())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    overlay = overlay.child(dialog);
    overlay
}

/// The reference's `border-primary-outline` ring on the title field, and the
/// `border-ring` it switches to while the field has focus (CDP, both themes).
fn rename_field_border(mode: ThemeMode, focused: bool) -> gpui::Rgba {
    match (mode, focused) {
        (ThemeMode::Light, true) => rgba(0x339cffff),
        (ThemeMode::Light, false) => rgba(0x1a1c1f1e),
        (ThemeMode::Dark, true) => rgba(0x83c3ffc2),
        (ThemeMode::Dark, false) => rgba(0xffffff28),
    }
}

/// `ring-border` on the reference dialog card: rgba(255,255,255,0.082) in
/// dark and rgba(26,28,31,0.078) in light.
fn rename_dialog_ring(mode: ThemeMode) -> gpui::Rgba {
    match mode {
        ThemeMode::Light => rgba(0x1a1c1f14),
        ThemeMode::Dark => rgba(0xffffff15),
    }
}

/// The dialog's `bg-surface-elevated-secondary/90`. The reference blurs what
/// is behind it, so the card shows the backdrop's *local average*; GPUI has no
/// backdrop filter, and painting 90% alpha over the live transcript would show
/// sharp text the reference never draws. The card therefore carries the
/// resolved colour, the same way the sidebar hover cards do.
fn rename_card_surface(theme: Theme) -> gpui::Rgba {
    let over = theme.project_hover_surface;
    // The card sits on top of the scrim, not on the bare pane: the backdrop it
    // would sample is the pane already dimmed by `chat_search_overlay`.
    let scrim = theme.chat_search_overlay;
    let under = gpui::Rgba {
        r: theme.surface.r * (1.0 - scrim.a) + scrim.r * scrim.a,
        g: theme.surface.g * (1.0 - scrim.a) + scrim.g * scrim.a,
        b: theme.surface.b * (1.0 - scrim.a) + scrim.b * scrim.a,
        a: 1.0,
    };
    let (alpha, rest) = (over.a, 1.0 - over.a);
    gpui::Rgba {
        r: over.r * alpha + under.r * rest,
        g: over.g * alpha + under.g * rest,
        b: over.b * alpha + under.b * rest,
        a: 1.0,
    }
}

/// The reference's dialog title carries `letter-spacing: -0.36px`
/// (measured at 20px: `-0.018em`). GPUI's text system has no letter spacing,
/// so the title is shaped once and painted one grapheme at a time, each shifted
/// by the tracking the reference applies before it.
fn tracked_title(
    text: &'static str,
    size: f32,
    line_height: f32,
    color: gpui::Hsla,
) -> impl IntoElement {
    let text: gpui::SharedString = crate::i18n::text(text).to_owned().into();
    let tracking = -0.018 * size;
    canvas(
        move |_, window, _| {
            let mut font = window.text_style().font();
            font.weight = gpui::FontWeight::SEMIBOLD;
            // One shaped line per grapheme: the reference places every glyph at
            // its own advance minus the tracking, which a single shaped run
            // cannot express.
            text.as_ref()
                .graphemes(true)
                .map(|grapheme| {
                    let grapheme = gpui::SharedString::from(grapheme.to_owned());
                    window.text_system().shape_line(
                        grapheme.clone(),
                        px(size),
                        &[gpui::TextRun {
                            len: grapheme.len(),
                            font: font.clone(),
                            color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                })
                .collect::<Vec<_>>()
        },
        move |bounds, lines, window, cx| {
            let mut x = f32::from(bounds.origin.x);
            for (index, line) in lines.iter().enumerate() {
                let left = if index == 0 {
                    x
                } else {
                    x + tracking * index as f32
                };
                line.paint(
                    point(px(left), bounds.origin.y),
                    px(line_height),
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .expect("rename title glyphs should paint");
                x += f32::from(line.width());
            }
        },
    )
    .w_full()
    .h_full()
}

/// The task rename dialog measured from the live ChatGPT desktop app: a scrim
/// over the whole window and a centred 420x185 card that wears
/// `bg-surface-elevated-secondary/90`, the 0.5px `ring-border`, and
/// `shadow-lg`. Every control inside it is a real commit or a real dismissal.
fn thread_rename_overlay(
    input: gpui::Entity<crate::components::prompt_input::PromptInput>,
    mode: ThemeMode,
    focused: bool,
    theme: Theme,
    cx: &mut Context<ChatApp>,
) -> impl IntoElement {
    div()
        .id("thread-rename-overlay")
        .absolute()
        .inset_0()
        .bg(theme.chat_search_overlay)
        .flex()
        .items_center()
        .justify_center()
        .on_click(cx.listener(|this, _, _, cx| this.dismiss_thread_rename(cx)))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("thread-rename-dialog")
                .role(Role::Dialog)
                .aria_label(crate::i18n::text("重命名聊天"))
                .w(px(420.0))
                .h(px(185.0))
                .rounded(px(25.0))
                .bg(rename_card_surface(theme))
                .shadow(vec![
                    // The reference draws its ring outside the 420px box.
                    BoxShadow::new(px(0.0), px(0.0), rename_dialog_ring(mode).into())
                        .blur_radius(px(0.0))
                        .spread_radius(px(0.5)),
                    BoxShadow::new(px(0.0), px(4.0), rgba(0x0000001a).into())
                        .blur_radius(px(8.0))
                        .spread_radius(px(-2.0)),
                ])
                .on_click(|_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .key_context("ThreadRename")
                .on_action(cx.listener(|this, _: &super::DismissThreadRename, _, cx| {
                    this.dismiss_thread_rename(cx);
                    cx.stop_propagation();
                }))
                .p(px(20.0))
                .flex()
                .flex_col()
                .text_color(theme.text)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(div().h(px(28.0)).w_full().child(tracked_title(
                            "重命名聊天",
                            20.0,
                            28.0,
                            theme.text.into(),
                        )))
                        .child(
                            div()
                                .h(px(21.0))
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .text_color(theme.chat_search_description)
                                .child(crate::i18n::text("保持简短且易于识别")),
                        ),
                )
                .child(
                    div().pt(px(12.0)).child(
                        div()
                            .id("thread-rename-input")
                            .h(px(36.0))
                            .w_full()
                            .rounded(px(10.0))
                            .border(px(1.0))
                            .border_color(rename_field_border(mode, focused))
                            .bg(theme.control)
                            .flex()
                            .items_center()
                            .child(input),
                    ),
                )
                .child(
                    div()
                        .pt(px(12.0))
                        .flex()
                        .justify_end()
                        .gap(px(12.0))
                        .child(
                            div()
                                .id("thread-rename-cancel")
                                .role(Role::Button)
                                .aria_label(crate::i18n::text("取消"))
                                .h(px(32.0))
                                .px(px(16.0))
                                .py(px(6.0))
                                .rounded(px(12.5))
                                .border(px(1.0))
                                .border_color(theme.chat_search_border)
                                .bg(theme.edit_button_surface)
                                .flex()
                                .items_center()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.sidebar_hover))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.dismiss_thread_rename(cx)),
                                )
                                .child(crate::i18n::text("取消")),
                        )
                        .child(
                            div()
                                .id("thread-rename-save")
                                .role(Role::Button)
                                .aria_label(crate::i18n::text("保存"))
                                .h(px(32.0))
                                .px(px(16.0))
                                .py(px(6.0))
                                .rounded(px(12.5))
                                .border(px(1.0))
                                .border_color(theme.chat_search_border)
                                .bg(theme.button)
                                .flex()
                                .items_center()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .text_color(theme.button_text)
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.button.alpha(0.8)))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.submit_thread_rename(cx)),
                                )
                                .child(crate::i18n::text("保存")),
                        ),
                )
                .child(
                    div()
                        .id("thread-rename-close")
                        .role(Role::Button)
                        .aria_label(crate::i18n::text("关闭对话框"))
                        .absolute()
                        .top(px(16.0))
                        .right(px(16.0))
                        .size(px(24.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.sidebar_hover))
                        .on_click(cx.listener(|this, _, _, cx| this.dismiss_thread_rename(cx)))
                        .child(icon("close-dialog", theme.text.alpha(0.8).into()).size(px(16.0))),
                ),
        )
}

impl ChatApp {
    /// Logout confirmation measured from the live ChatGPT desktop app: a
    /// centered 380x170 dialog with a 20px inset, a 20px title, and two 32px
    /// buttons. The destructive action only runs after this confirmation.
    pub(super) fn account_logout_overlay(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let email = self.account.account_email().map(str::to_owned);
        div()
            .id("account-logout-dialog")
            .role(gpui::Role::Dialog)
            .aria_label(crate::i18n::text("要退出登录？"))
            .track_focus(&self.account_focus)
            .on_key_down(cx.listener(Self::account_dialog_key))
            .w(px(380.0))
            .h(px(170.0))
            .relative()
            .rounded(px(25.0))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(8.0), theme.profile_menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .p(px(20.0))
            .flex()
            .flex_col()
            .text_color(theme.markdown_text)
            .child(
                div()
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .text_size(px(20.0))
                    .line_height(px(28.0))
                    .font_weight(gpui::FontWeight(600.0))
                    .child(crate::i18n::text("要退出登录？")),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child(match &email {
                        Some(email) => {
                            crate::i18n::format!("已以 {email} 身份登录" => "Signed in as {email}")
                        }
                        None => crate::i18n::text("已登录 ChatGPT 账户").to_owned(),
                    }),
            )
            .child(
                div()
                    .mt(px(11.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .child(crate::i18n::text("你需要重新登录才能继续使用 ChatGPT")),
            )
            .child(
                div()
                    .id("account-dialog-close")
                    .role(gpui::Role::Button)
                    .aria_label(crate::i18n::text("关闭对话框"))
                    .absolute()
                    .top(px(16.0))
                    .right(px(16.0))
                    .size(px(24.0))
                    .rounded(px(4.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.text.alpha(0.06)))
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss_account_dialog(cx)))
                    .child(icon("close-dialog", theme.markdown_text.into()).size(px(16.0))),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(20.0))
                    .left(px(20.0))
                    .right(px(20.0))
                    .flex()
                    .justify_end()
                    .gap(px(12.0))
                    .child(
                        div()
                            .id("account-logout-cancel")
                            .role(gpui::Role::Button)
                            .aria_label(crate::i18n::text("取消"))
                            .when(self.account_choice == 0, |button| {
                                button.aria_active_descendant().shadow(vec![
                                    BoxShadow::new(px(0.0), px(0.0), theme.accent.into())
                                        .spread_radius(px(2.0)),
                                ])
                            })
                            .h(px(32.0))
                            .px(px(16.0))
                            .py(px(6.0))
                            .rounded(px(12.5))
                            .flex()
                            .items_center()
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.text.alpha(0.05)))
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss_account_dialog(cx)))
                            .child(crate::i18n::text("取消")),
                    )
                    .child(
                        div()
                            .id("account-logout-confirm")
                            .role(gpui::Role::Button)
                            .aria_label(crate::i18n::text("退出登录"))
                            .when(self.account_choice == 1, |button| {
                                button.aria_active_descendant().shadow(vec![
                                    BoxShadow::new(px(0.0), px(0.0), theme.accent.into())
                                        .spread_radius(px(2.0)),
                                ])
                            })
                            .h(px(32.0))
                            .px(px(16.0))
                            .py(px(6.0))
                            .rounded(px(12.5))
                            .bg(rgba(0xe02e2a1a))
                            .flex()
                            .items_center()
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(rgba(0xe02e2aff))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgba(0xe02e2a33)))
                            .on_click(cx.listener(|this, _, _, cx| this.confirm_logout(cx)))
                            .child(crate::i18n::text("退出登录")),
                    ),
            )
    }

    /// Login progress. The challenge is exactly what the backend returned: an
    /// authorization URL, a device code, or a pending state until either shows
    /// up. Nothing here reports success before the completion notification.
    pub(super) fn account_login_overlay(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let challenge = self.account.challenge().cloned();
        let url = challenge.as_ref().map(|challenge| match challenge {
            AgentLoginChallenge::AuthUrl { auth_url } => auth_url.clone(),
            AgentLoginChallenge::DeviceCode {
                verification_url, ..
            } => verification_url.clone(),
        });
        let user_code = match &challenge {
            Some(AgentLoginChallenge::DeviceCode { user_code, .. }) => Some(user_code.clone()),
            _ => None,
        };
        let phase = self.account.state.login.phase;
        let mut dialog = div()
            .id("account-login-dialog")
            .role(gpui::Role::Dialog)
            .aria_label(crate::i18n::text("登录 ChatGPT"))
            .track_focus(&self.account_focus)
            .on_key_down(cx.listener(Self::account_dialog_key))
            .w(px(420.0))
            .relative()
            .rounded(px(25.0))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(8.0), theme.profile_menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_color(theme.markdown_text)
            .child(
                div()
                    .text_size(px(20.0))
                    .line_height(px(28.0))
                    .font_weight(gpui::FontWeight(600.0))
                    .child(crate::i18n::text("登录 ChatGPT")),
            );
        let mut actions = div().mt(px(8.0)).flex().justify_end().gap(px(12.0));
        if let Some(error) = self.account.login_error().map(str::to_owned) {
            dialog = dialog.child(div().text_size(px(14.0)).line_height(px(21.0)).child(error));
        } else if let Some(challenge) = &challenge {
            dialog = dialog.child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child(match challenge {
                        AgentLoginChallenge::AuthUrl { .. } => {
                            crate::i18n::text("请在浏览器中完成授权。完成后此窗口会自动更新。")
                        }
                        AgentLoginChallenge::DeviceCode { .. } => crate::i18n::text(
                            "请打开下面的地址，并输入一次性代码。完成后此窗口会自动更新。",
                        ),
                    }),
            );
            if let Some(code) = &user_code {
                dialog = dialog.child(
                    div()
                        .h(px(40.0))
                        .rounded(px(12.5))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.elevated)
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_family(crate::theme::UI_MONOSPACE_FONT_FAMILY)
                        .text_size(px(18.0))
                        .child(code.clone()),
                );
            }
            if let Some(url) = &url {
                dialog = dialog.child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(theme.accent)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(url.clone()),
                );
            }
        } else {
            dialog = dialog.child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child(crate::i18n::text(
                        "已请求登录，正在等待服务端返回授权信息。",
                    )),
            );
        }
        if let Some(url) = url {
            actions = actions.child(
                div()
                    .id("account-login-open")
                    .role(gpui::Role::Button)
                    .aria_label(crate::i18n::text("打开浏览器"))
                    .h(px(32.0))
                    .px(px(16.0))
                    .py(px(6.0))
                    .rounded(px(12.5))
                    .bg(theme.text.alpha(0.05))
                    .flex()
                    .items_center()
                    .text_size(px(14.0))
                    .line_height(px(18.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.text.alpha(0.10)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.handle_account_intent(
                            crate::components::sidebar::AccountIntent::OpenExternalUrl(url.clone()),
                            cx,
                        );
                    }))
                    .child(crate::i18n::text("打开浏览器")),
            );
        }
        let retry = phase == AgentAccountLoginPhase::Failed;
        actions = actions.child(
            div()
                .id("account-login-cancel")
                .role(gpui::Role::Button)
                .aria_label(if retry {
                    crate::i18n::text("重试")
                } else {
                    crate::i18n::text("取消登录")
                })
                .h(px(32.0))
                .px(px(16.0))
                .py(px(6.0))
                .rounded(px(12.5))
                .flex()
                .items_center()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .cursor_pointer()
                .hover(move |style| style.bg(theme.text.alpha(0.05)))
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.account.state.login.phase == AgentAccountLoginPhase::Failed {
                        this.start_login(cx);
                    } else if let Some(login_id) = this.account.state.login.login_id.clone() {
                        this.cancel_login(login_id, cx);
                    } else {
                        this.dismiss_account_dialog(cx);
                    }
                }))
                .child(if retry {
                    crate::i18n::text("重试")
                } else {
                    crate::i18n::text("取消登录")
                }),
        );
        dialog.child(actions)
    }

    pub(super) fn account_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.account.dialog.is_none() {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.dismiss_account_dialog(cx),
            "tab" | "up" | "down" | "left" | "right" => {
                self.account_choice = if self.account_choice == 0 { 1 } else { 0 };
                cx.notify();
            }
            "enter" => match self.account.dialog {
                Some(AccountDialog::Logout) => {
                    if self.account_choice == 1 {
                        self.confirm_logout(cx);
                    } else {
                        self.dismiss_account_dialog(cx);
                    }
                }
                Some(AccountDialog::Login) => {
                    if self.account.state.login.phase == AgentAccountLoginPhase::Failed {
                        self.start_login(cx);
                    } else if let Some(login_id) = self.account.state.login.login_id.clone() {
                        self.cancel_login(login_id, cx);
                    }
                }
                None => {}
            },
            _ => {}
        }
    }
}
