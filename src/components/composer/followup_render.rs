//! The tray above the composer: goal summary, paused-queue banner, and the
//! queued follow-ups with their actions. Geometry follows the reference's
//! queued-message list: 32 px rows, 14/20 text, 24 px icon buttons, a 710 px
//! panel inset 13 px from the composer that tucks under its top edge.

use gpui::{
    AnyElement, Context, Div, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Role, SharedString, Stateful, div, prelude::*, px, rgba,
};

use super::ComposerView;
use crate::{
    components::icons::icon, conversation::QueueRowOperation, theme::Theme, workspace::FollowUpMode,
};

pub(crate) const TRAY_ROW_HEIGHT: f32 = 32.0;
const TRAY_INSET: f32 = 13.0;
/// The part of the tray hidden under the composer's rounded top edge.
const TRAY_TUCK: f32 = 12.0;
const DRAG_ACTIVATION: f32 = 6.0;

/// A row being dragged to a new position.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QueueDrag {
    pub(crate) id: String,
    pub(crate) from: usize,
    pub(crate) start_y: f32,
    pub(crate) current_y: f32,
    pub(crate) active: bool,
}

impl QueueDrag {
    pub(crate) fn target(&self, rows: usize) -> usize {
        let offset = ((self.current_y - self.start_y) / TRAY_ROW_HEIGHT).round() as isize;
        (self.from as isize + offset).clamp(0, rows.saturating_sub(1) as isize) as usize
    }
}

pub(crate) struct TrayColors {
    pub(crate) surface: Hsla,
    pub(crate) border: Hsla,
    pub(crate) divider: Hsla,
    pub(crate) primary: Hsla,
    pub(crate) secondary: Hsla,
    pub(crate) icon: Hsla,
    pub(crate) hover: Hsla,
}

pub(crate) fn tray_colors(theme: Theme, mode: crate::theme::ThemeMode) -> TrayColors {
    let text: Hsla = theme.text.into();
    match mode {
        crate::theme::ThemeMode::Dark => TrayColors {
            surface: rgba(0x2d2d2dff).into(),
            border: rgba(0x3b3b3bff).into(),
            divider: rgba(0x363636ff).into(),
            primary: text,
            secondary: text.opacity(0.5),
            icon: text.opacity(0.35),
            hover: rgba(0xffffff0f).into(),
        },
        crate::theme::ThemeMode::Light => TrayColors {
            surface: rgba(0xfefefeff).into(),
            border: rgba(0x1a1c1f14).into(),
            divider: rgba(0x1a1c1f0c).into(),
            primary: text,
            secondary: text.opacity(0.494),
            icon: text.opacity(0.346),
            hover: rgba(0x1a1c1f0d).into(),
        },
    }
}

/// A tray row: 14 px icon at 10 px, content at 32 px, actions on the right.
pub(crate) fn tray_row(id: SharedString, colors: &TrayColors) -> Stateful<Div> {
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .h(px(TRAY_ROW_HEIGHT))
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(10.0))
        .py(px(2.0))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(colors.secondary)
}

pub(crate) fn tray_icon(name: &'static str, colors: &TrayColors) -> impl IntoElement {
    div()
        .w(px(14.0))
        .h(px(16.0))
        .flex_none()
        .flex()
        .items_center()
        .child(icon(name, colors.icon).size(px(14.0)))
}

/// A 24 px round icon button with an accessible name.
pub(crate) fn tray_icon_button(
    id: SharedString,
    name: &'static str,
    label: String,
    colors: &TrayColors,
) -> Stateful<Div> {
    let hover = colors.hover;
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .role(Role::Button)
        .aria_label(SharedString::from(label))
        .focusable()
        .tab_stop(true)
        .size(px(24.0))
        .flex_none()
        .rounded(px(12.5))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(name, colors.secondary).size(px(14.0)))
}

/// A text action ("Steer", "Resume"): 13/18, 2×8 padding, pill.
pub(crate) fn tray_text_button(
    id: SharedString,
    name: &'static str,
    label: String,
    colors: &TrayColors,
) -> Stateful<Div> {
    let hover = colors.hover;
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .role(Role::Button)
        .aria_label(SharedString::from(label.clone()))
        .focusable()
        .tab_stop(true)
        .h(px(24.0))
        .flex_none()
        .px(px(8.0))
        .py(px(2.0))
        .rounded_full()
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(px(13.0))
        .line_height(px(18.0))
        .text_color(colors.secondary)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(name, colors.secondary).size(px(14.0)))
        .child(label)
}

fn activate_on_key(event: &gpui::KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space")
}

impl ComposerView {
    pub(super) fn tray_visible(&self) -> bool {
        !self.side_chat && (self.goal_row_visible() || !self.conversation.queue.is_empty())
    }

    /// The tray's contribution to the composer's height: its border, rows,
    /// dividers and 1 px gaps. The part tucked under the composer cancels out.
    pub(super) fn tray_height(&self) -> f32 {
        if !self.tray_visible() {
            return 0.0;
        }
        let mut children: Vec<f32> = Vec::new();
        if self.goal_row_visible() {
            children.push(TRAY_ROW_HEIGHT);
        }
        if !self.conversation.queue.is_empty() {
            if !children.is_empty() {
                children.push(1.0);
            }
            if self.queue_paused() {
                children.extend([TRAY_ROW_HEIGHT, 1.0]);
            }
            children.extend(std::iter::repeat_n(
                TRAY_ROW_HEIGHT,
                self.conversation.queue.rows.len(),
            ));
        }
        let gaps = children.len().saturating_sub(1) as f32;
        2.0 + children.iter().sum::<f32>() + gaps
    }

    /// The whole tray, or nothing when there is neither a goal nor a queue.
    /// Painted before the composer body and anchored to it, so its bottom
    /// `TRAY_TUCK` px sit under the body's rounded top edge; the composer
    /// reserves only the visible part with a spacer (negative margins do not
    /// lay out in this bottom-anchored overlay).
    pub(super) fn render_tray(
        &self,
        theme: Theme,
        body_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.tray_visible() {
            return None;
        }
        let colors = tray_colors(theme, self.mode);
        let mut rows: Vec<AnyElement> = Vec::new();
        if let Some(summary) = self.goal_summary_row(&colors, cx) {
            rows.push(summary);
        }
        if !self.conversation.queue.is_empty() {
            if !rows.is_empty() {
                rows.push(divider(&colors));
            }
            if self.queue_paused() {
                rows.push(self.paused_row(&colors, cx));
                rows.push(divider(&colors));
            }
            let count = self.conversation.queue.rows.len();
            for index in 0..count {
                rows.push(self.queue_row(index, &colors, cx));
            }
        }
        let tray = div()
            .id("composer-tray")
            .absolute()
            .left(px(TRAY_INSET))
            .right(px(TRAY_INSET))
            .bottom(px(body_height - TRAY_TUCK))
            .h(px(self.tray_height() + TRAY_TUCK))
            .pb(px(TRAY_TUCK))
            .overflow_hidden()
            .rounded_t(px(12.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.surface)
            .flex()
            .flex_col()
            .gap(px(1.0))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let Some(drag) = this.queue_drag.as_mut() else {
                    return;
                };
                if event.pressed_button != Some(MouseButton::Left) {
                    return;
                }
                drag.current_y = f32::from(event.position.y);
                if !drag.active && (drag.current_y - drag.start_y).abs() >= DRAG_ACTIVATION {
                    drag.active = true;
                }
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    let Some(drag) = this.queue_drag.take() else {
                        return;
                    };
                    if drag.active {
                        let target = drag.target(this.conversation.queue.rows.len());
                        this.reorder_queued(&drag.id, target, cx);
                    }
                    cx.notify();
                }),
            )
            .children(rows);
        Some(tray.into_any_element())
    }

    fn paused_row(&self, colors: &TrayColors, cx: &mut Context<Self>) -> AnyElement {
        let resume = crate::i18n::format!("继续" => "Resume");
        tray_row("queue-paused".into(), colors)
            .role(Role::Status)
            .child(tray_icon("queue-paused", colors))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .child(crate::i18n::format!("由于你中断了当前响应，队列已暂停" => "Queue paused because you interrupted")),
            )
            .child(
                tray_text_button("queue-resume".into(), "queue-resume", resume, colors)
                    .on_click(cx.listener(|this, _, _, cx| this.resume_queue(cx)))
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                        if activate_on_key(event) {
                            this.resume_queue(cx);
                            cx.stop_propagation();
                        }
                    })),
            )
            .into_any_element()
    }

    fn queue_row(&self, index: usize, colors: &TrayColors, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.conversation.queue.rows[index];
        let id = row.submission.id.clone();
        let busy = row.operation.clone();
        let failed = row.error.clone();

        let text: String = row
            .submission
            .text
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned();
        let drag = self
            .queue_drag
            .as_ref()
            .filter(|drag| drag.active && drag.id == id);
        let reorderable = self.conversation.queue.rows.len() > 1
            && self
                .conversation
                .queue
                .rows
                .iter()
                .all(|row| row.operation.is_none());
        let action_label = match (&busy, &failed) {
            (Some(QueueRowOperation::Steering | QueueRowOperation::Starting), _) => {
                crate::i18n::format!("正在发送" => "Sending")
            }
            (_, Some(_)) => crate::i18n::format!("重试" => "Retry"),
            _ => crate::i18n::format!("引导" => "Steer"),
        };
        let tooltip = match &failed {
            Some(error) => {
                crate::i18n::format!("这条排队中的消息未能发送：{error}。重试、编辑或删除该消息以继续发送排队的消息" => "This queued message could not be sent: {error}. Retry, edit, or delete it to continue the queue")
            }
            None => {
                crate::i18n::format!("提交，但不中断模型运行" => "Submit without interrupting the model")
            }
        };
        let menu_open = self.queue_menu.as_deref() == Some(id.as_str());
        let send_id = id.clone();
        let send_key_id = id.clone();
        let delete_id = id.clone();
        let delete_key_id = id.clone();
        let menu_id = id.clone();
        let drag_id = id.clone();
        let mut element = tray_row(SharedString::from(format!("queued-{id}")), colors)
            .relative()
            .aria_label(SharedString::from(text.clone()))
            .when(reorderable, |row| {
                row.cursor_grab().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, _| {
                        let y = f32::from(event.position.y);
                        this.queue_drag = Some(QueueDrag {
                            id: drag_id.clone(),
                            from: index,
                            start_y: y,
                            current_y: y,
                            active: false,
                        });
                    }),
                )
            })
            .when_some(drag, |row, drag| {
                row.relative()
                    .top(px(drag.current_y - drag.start_y))
                    .opacity(0.8)
                    .bg(colors.hover)
            })
            .child(tray_icon("queue-row", colors))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .py(px(4.0))
                    .truncate()
                    .text_color(colors.primary)
                    .child(text),
            )
            .when_some(failed.clone(), |row, _| {
                row.child(
                    div()
                        .flex_none()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(rgba(0xe02e2aff)),
                )
            })
            .child(
                tray_text_button(
                    SharedString::from(format!("queued-send-{id}")),
                    "queue-steer",
                    action_label,
                    colors,
                )
                .aria_description(SharedString::from(tooltip.clone()))
                .tooltip(move |_, cx| cx.new(|_| TrayTooltip(tooltip.clone().into())).into())
                .when(busy.is_none(), |button| {
                    button
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.send_queued_now(&send_id, cx)),
                        )
                        .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                            if activate_on_key(event) {
                                this.send_queued_now(&send_key_id, cx);
                                cx.stop_propagation();
                            }
                        }))
                }),
            )
            .child(
                tray_icon_button(
                    SharedString::from(format!("queued-delete-{id}")),
                    "queue-delete",
                    crate::i18n::format!("删除排队的消息" => "Delete queued message"),
                    colors,
                )
                .when(busy.is_none(), |button| {
                    button
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.delete_queued(&delete_id, cx)),
                        )
                        .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                            if activate_on_key(event) {
                                this.delete_queued(&delete_key_id, cx);
                                cx.stop_propagation();
                            }
                        }))
                }),
            )
            .child(
                tray_icon_button(
                    SharedString::from(format!("queued-menu-{id}")),
                    "queue-more",
                    crate::i18n::format!("排队消息操作" => "Queued message actions"),
                    colors,
                )
                .aria_expanded(menu_open)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.queue_menu = if this.queue_menu.as_deref() == Some(menu_id.as_str()) {
                        None
                    } else {
                        Some(menu_id.clone())
                    };
                    cx.notify();
                })),
            );
        if menu_open {
            element = element.child(self.queue_menu_popover(&id, cx));
        }
        element.into_any_element()
    }

    fn queue_menu_popover(&self, id: &str, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::for_mode(self.mode);
        let item = |key: &'static str, label: String| {
            div()
                .id(key)
                .debug_selector(move || key.to_owned())
                .role(Role::MenuItem)
                .aria_label(SharedString::from(label.clone()))
                .focusable()
                .tab_stop(true)
                .h(px(29.0))
                .px(px(8.0))
                .rounded(px(6.0))
                .flex()
                .items_center()
                .text_size(px(14.0))
                .text_color(theme.text)
                .cursor_pointer()
                .hover(move |s| s.bg(theme.sidebar_hover))
                .child(label)
        };
        let edit_id = id.to_owned();
        let side_id = id.to_owned();
        let side_chat = !self.side_chat && self.side_chat_configuration().is_some();
        let toggle = match self.follow_up_mode {
            FollowUpMode::Queue => crate::i18n::format!("关闭排队" => "Turn off queueing"),
            FollowUpMode::Steer => crate::i18n::format!("启用队列模式" => "Turn on queueing"),
        };
        gpui::deferred(
            div()
                .id("queued-menu")
                .role(Role::Menu)
                .absolute()
                .right(px(10.0))
                .top(px(TRAY_ROW_HEIGHT - 2.0))
                .w(px(154.0))
                .p(px(4.0))
                .rounded(px(10.0))
                .bg(theme.model_picker_surface)
                .border(px(0.5))
                .border_color(theme.border)
                .shadow(vec![
                    gpui::BoxShadow::new(px(0.0), px(8.0), rgba(0x00000033).into())
                        .blur_radius(px(24.0)),
                ])
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.queue_menu = None;
                    cx.notify();
                }))
                .child(
                    item(
                        "queued-menu-edit",
                        crate::i18n::format!("编辑消息" => "Edit message"),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.edit_queued(&edit_id, cx))),
                )
                .when(side_chat, |menu| {
                    menu.child(
                        item(
                            "queued-menu-side-chat",
                            crate::i18n::format!("在侧边聊天中打开" => "Open in side chat"),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_queued_in_side_chat(&side_id, cx)
                        })),
                    )
                })
                .child(
                    item("queued-menu-toggle", toggle)
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_queueing(cx))),
                ),
        )
        .with_priority(2)
        .into_any_element()
    }
}

fn divider(colors: &TrayColors) -> AnyElement {
    div()
        .h(px(1.0))
        .w_full()
        .flex_none()
        .bg(colors.divider)
        .into_any_element()
}

/// The reference's dark tooltip bubble for tray actions.
pub(crate) struct TrayTooltip(pub(crate) SharedString);

impl Render for TrayTooltip {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(280.0))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .bg(rgba(0x333333ff))
            .text_color(rgba(0xffffffff))
            .text_size(px(12.0))
            .line_height(px(16.0))
            .child(self.0.clone())
    }
}
