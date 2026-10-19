//! Task-bound Computer Use picture-in-picture and intervention controls.
mod model;
#[cfg(test)]
mod tests;

use base64::Engine as _;
use gpui::{
    Context, Entity, Image, ImageFormat, IntoElement, ObjectFit, Render, Role, Window, div,
    prelude::*, px,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    agent::AgentMcpToolCallStatus,
    components::{composer::ComposerView, icons::icon},
    computer_use::{self, CaptureError, Target},
    conversation::{ConversationActivity, ConversationPhase},
    theme::{Theme, ThemeMode},
};
use model::Snapshot;

pub(super) struct Preview {
    owner: Option<(Entity<ComposerView>, u64)>,
    snapshot: Option<Snapshot>,
    phase: ConversationPhase,
    pub(super) mode: ThemeMode,
    expanded: bool,
    show_operations: bool,
    generation: u64,
    capture_task: Option<gpui::Task<()>>,
    native_frame: Option<Arc<Image>>,
    tool_frame: Option<Arc<Image>>,
    tool_frame_key: Option<String>,
    tool_frame_time: Option<Instant>,
    frame_time: Option<Instant>,
    window_title: String,
    application_name: String,
    capture_error: Option<CaptureError>,
    takeover_pending: bool,
    intervention_error: Option<String>,
    seen_running: bool,
    rendered_at: Instant,
}

fn running(phase: ConversationPhase) -> bool {
    matches!(
        phase,
        ConversationPhase::Starting
            | ConversationPhase::Thinking
            | ConversationPhase::Streaming
            | ConversationPhase::Stopping
    )
}

impl Preview {
    pub fn new(mode: ThemeMode) -> Self {
        Self {
            owner: None,
            snapshot: None,
            phase: ConversationPhase::Empty,
            mode,
            expanded: true,
            show_operations: false,
            generation: 0,
            capture_task: None,
            native_frame: None,
            tool_frame: None,
            tool_frame_key: None,
            tool_frame_time: None,
            frame_time: None,
            window_title: String::new(),
            application_name: String::new(),
            capture_error: None,
            takeover_pending: false,
            intervention_error: None,
            seen_running: false,
            rendered_at: Instant::now(),
        }
    }

    pub fn sync(
        &mut self,
        owner: Entity<ComposerView>,
        activities: &[ConversationActivity],
        phase: ConversationPhase,
        mode: ThemeMode,
        cx: &mut Context<Self>,
    ) {
        let identity = (owner.clone(), owner.read(cx).request_cycle());
        if self.owner.as_ref() != Some(&identity) {
            self.cancel_capture();
            self.owner = Some(identity);
            self.snapshot = None;
            self.expanded = true;
            self.show_operations = false;
            self.takeover_pending = false;
            self.intervention_error = None;
            self.seen_running = false;
        }
        self.mode = mode;
        self.phase = phase;
        self.seen_running |= running(phase);
        let next = Snapshot::from_activities(activities);
        let target = next.as_ref().and_then(|s| s.target.clone());
        if self.snapshot.as_ref().and_then(|s| s.target.as_ref()) != target.as_ref() {
            self.cancel_capture();
        }
        let tool_key = next
            .as_ref()
            .and_then(|s| s.frame.as_ref())
            .map(|f| f.key.clone());
        if self.tool_frame_key != tool_key {
            self.tool_frame = next
                .as_ref()
                .and_then(|s| s.frame.as_ref())
                .and_then(|frame| {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(&frame.data)
                        .ok()?;
                    crate::media::encoded_image_dimensions(&bytes)?;
                    let format = super::media::tool_image_format(&bytes, frame.mime.as_deref());
                    Some(Arc::new(Image::from_bytes(format, bytes)))
                });
            self.tool_frame_key = tool_key;
            self.tool_frame_time = self.tool_frame.as_ref().map(|_| Instant::now());
        }
        self.snapshot = next;
        if !running(phase) {
            self.capture_task = None;
            self.generation = self.generation.wrapping_add(1);
            if self.takeover_pending && phase == ConversationPhase::Failed {
                self.takeover_pending = false;
                self.intervention_error = Some(crate::i18n::format!(
                    "未确认 agent 已停止，请检查执行错误后再切换应用。" =>
                    "The agent has not confirmed it stopped. Check the execution error before switching apps."));
            }
            // Wait for a confirmed terminal event before bringing the target
            // forward. A failed interrupt must not masquerade as a takeover.
            if self.takeover_pending
                && matches!(
                    phase,
                    ConversationPhase::Stopped | ConversationPhase::Complete
                )
            {
                self.takeover_pending = false;
                if let Some(target) = target.as_ref()
                    && !computer_use::focus(target)
                {
                    self.intervention_error = Some(
                        crate::i18n::format!("无法打开目标应用，请手动切换。" => "Could not focus the target application. Switch to it manually."),
                    );
                }
            }
        }
    }

    fn cancel_capture(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.capture_task = None;
        self.native_frame = None;
        self.tool_frame = None;
        self.tool_frame_key = None;
        self.tool_frame_time = None;
        self.frame_time = None;
        self.window_title.clear();
        self.application_name.clear();
        self.capture_error = None;
    }

    fn start_capture(&mut self, target: Target, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.capture_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let visible = this
                    .update(cx, |this, _| {
                        let visible = this.generation == generation
                            && this.expanded
                            && running(this.phase)
                            && this.rendered_at.elapsed() < Duration::from_secs(2);
                        if !visible && this.generation == generation {
                            this.capture_task = None;
                        }
                        visible
                    })
                    .unwrap_or(false);
                if !visible {
                    return;
                }
                let receiver = cx.update(|_| computer_use::capture(target.clone()));
                // A native callback that never arrives must not retain a task
                // forever. Polling a bounded receiver also makes cancellation
                // on a thread/surface switch immediate.
                let deadline = Instant::now() + Duration::from_secs(5);
                let result = loop {
                    match receiver.try_recv() {
                        Ok(result) => break result,
                        Err(async_channel::TryRecvError::Closed) => {
                            break Err(CaptureError::CaptureFailed);
                        }
                        Err(async_channel::TryRecvError::Empty) if Instant::now() >= deadline => {
                            break Err(CaptureError::CaptureFailed);
                        }
                        Err(_) => {
                            cx.background_executor()
                                .timer(Duration::from_millis(40))
                                .await
                        }
                    }
                };
                let retry = this
                    .update(cx, |this, cx| this.accept_capture(generation, result, cx))
                    .ok()
                    .flatten();
                let Some(delay) = retry else {
                    return;
                };
                cx.background_executor().timer(delay).await;
            }
        }));
    }

    fn accept_capture(
        &mut self,
        generation: u64,
        result: Result<computer_use::Frame, CaptureError>,
        cx: &mut Context<Self>,
    ) -> Option<Duration> {
        if generation != self.generation {
            return None;
        }
        let delay = match result {
            Ok(frame) => {
                self.native_frame = Some(Arc::new(Image::from_bytes(ImageFormat::Png, frame.png)));
                self.window_title = frame.title;
                self.application_name = frame.application;
                self.frame_time = Some(Instant::now());
                self.capture_error = None;
                Duration::from_millis(400)
            }
            Err(error) => {
                self.capture_error = Some(error);
                if matches!(
                    error,
                    CaptureError::PermissionRequired | CaptureError::Unsupported
                ) {
                    self.capture_task = None;
                    cx.notify();
                    return None;
                }
                Duration::from_secs(2)
            }
        };
        cx.notify();
        Some(delay)
    }

    fn toggle(&mut self, cx: &mut Context<Self>) {
        self.expanded = !self.expanded;
        if !self.expanded {
            self.capture_task = None;
            self.generation = self.generation.wrapping_add(1);
        } else if running(self.phase)
            && let Some(target) = self.snapshot.as_ref().and_then(|s| s.target.clone())
        {
            self.start_capture(target, cx);
        }
        cx.notify();
    }

    fn take_over(&mut self, cx: &mut Context<Self>) {
        if self
            .snapshot
            .as_ref()
            .and_then(|s| s.target.as_ref())
            .is_none()
        {
            return;
        }
        if self.takeover_pending || self.phase == ConversationPhase::Stopping {
            return;
        }
        if running(self.phase) {
            self.takeover_pending = true;
            if let Some((owner, _)) = &self.owner {
                owner.update(cx, |owner, cx| owner.stop_for_computer_use(cx));
            }
        } else if let Some(target) = self.snapshot.as_ref().and_then(|s| s.target.as_ref())
            && !computer_use::focus(target)
        {
            self.intervention_error = Some(
                crate::i18n::format!("无法打开目标应用，请手动切换。" => "Could not focus the target application. Switch to it manually."),
            );
        }
        cx.notify();
    }
}

impl Preview {
    fn status_label(&self, snapshot: &Snapshot) -> String {
        if self.takeover_pending || self.phase == ConversationPhase::Stopping {
            crate::i18n::format!("正在停止…" => "Stopping…")
        } else if self.phase == ConversationPhase::Stopped {
            crate::i18n::format!("已停止 · 你可以接管" => "Stopped · You can take over")
        } else if self.phase == ConversationPhase::Failed
            || snapshot.status == AgentMcpToolCallStatus::Failed
        {
            crate::i18n::format!("操作失败" => "Action failed")
        } else if self.phase == ConversationPhase::Complete {
            crate::i18n::format!("已完成" => "Completed")
        } else if snapshot.status == AgentMcpToolCallStatus::InProgress {
            crate::i18n::format!("正在操作" => "Working")
        } else {
            crate::i18n::format!("等待下一步操作" => "Waiting for next action")
        }
    }

    fn capture_message(&self) -> String {
        match self.capture_error {
            Some(CaptureError::PermissionRequired) => crate::i18n::format!(
                "实时预览需要 macOS 屏幕录制权限；工具截图仍会随操作更新。" =>
                "Live preview needs macOS Screen Recording access. Tool captures still update with each action."),
            Some(CaptureError::WindowUnavailable) => crate::i18n::format!(
                "目标应用当前没有可预览的窗口。" => "The target application has no visible window."),
            Some(CaptureError::Unsupported) => crate::i18n::format!(
                "此系统不支持实时窗口预览。" => "Live window preview is unavailable on this system."),
            Some(CaptureError::CaptureFailed) => crate::i18n::format!(
                "窗口预览暂不可用，正在重试。" => "Window preview is unavailable. Retrying."),
            None => crate::i18n::format!(
                "等待应用窗口或工具截图…" => "Waiting for an application window or tool capture…"),
        }
    }
}

impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.rendered_at = Instant::now();
        if self.expanded
            && running(self.phase)
            && self.capture_task.is_none()
            && !matches!(
                self.capture_error,
                Some(CaptureError::PermissionRequired | CaptureError::Unsupported)
            )
            && let Some(target) = self.snapshot.as_ref().and_then(|s| s.target.clone())
        {
            self.start_capture(target, cx);
        }
        let Some(snapshot) = self.snapshot.as_ref().filter(|_| self.seen_running) else {
            return div().into_any_element();
        };
        let theme = Theme::for_mode(self.mode);
        let label = if self.application_name.is_empty() {
            snapshot.label.clone()
        } else {
            self.application_name.clone()
        };
        let status = self.status_label(snapshot);
        let live = running(self.phase)
            && self.capture_error.is_none()
            && self.native_frame.is_some()
            && self
                .frame_time
                .is_some_and(|time| time.elapsed() < Duration::from_secs(2));
        let native = self.native_frame.is_some()
            && (self.tool_frame.is_none() || self.frame_time >= self.tool_frame_time);
        let frame = if native {
            self.native_frame.clone()
        } else {
            self.tool_frame.clone()
        };
        let source = if native && live {
            crate::i18n::format!("实时窗口预览" => "Live window preview")
        } else if native {
            crate::i18n::format!("最近的窗口画面" => "Last window capture")
        } else {
            crate::i18n::format!("最近的工具截图" => "Latest tool capture")
        };
        let unavailable = self.capture_message();
        let target_available = snapshot.target.is_some();
        let error = self
            .intervention_error
            .clone()
            .or_else(|| snapshot.error.clone());
        let header = div()
            .id("computer-use-preview-toggle")
            .role(Role::Button)
            .aria_label(crate::i18n::format!(
                "{label}，{status}，{}预览" => "{label}, {status}, {} preview",
                if self.expanded { crate::i18n::text("折叠") } else { crate::i18n::text("展开") }))
            .aria_expanded(self.expanded)
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| this.toggle(cx)))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(icon("activity-native-app", theme.text_secondary.into()).size(px(16.)))
            .child(
                div()
                    .min_w(px(0.))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(crate::typography::ui_size(13.)))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(crate::typography::ui_size(11.)))
                            .text_color(theme.text_secondary)
                            .child(status),
                    ),
            )
            .child(
                icon(
                    if self.expanded {
                        "chevron-down"
                    } else {
                        "review-chevron-right"
                    },
                    theme.text_secondary.into(),
                )
                .size(px(14.)),
            );
        let image = div().px(px(8.)).child(
            div()
                .h(px(180.))
                .w_full()
                .rounded(px(8.))
                .bg(theme.surface)
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .when_some(frame.clone(), |v, image| {
                    v.child(gpui::img(image).size_full().object_fit(ObjectFit::Contain))
                })
                .when(frame.is_none(), |v| {
                    v.child(
                        div()
                            .px(px(16.))
                            .text_size(px(crate::typography::ui_size(12.)))
                            .text_center()
                            .text_color(theme.text_secondary)
                            .child(unavailable.clone()),
                    )
                }),
        );
        let information = div()
            .px(px(12.))
            .pt(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(crate::typography::ui_size(11.)))
                    .text_color(theme.text_secondary)
                    .child(source),
            )
            .when(!self.window_title.is_empty(), |v| {
                v.child(
                    div()
                        .text_size(px(crate::typography::ui_size(12.)))
                        .truncate()
                        .child(self.window_title.clone()),
                )
            })
            .child(
                div()
                    .text_size(px(crate::typography::ui_size(13.)))
                    .line_height(px(18.))
                    .child(snapshot.title.clone()),
            )
            .when_some(snapshot.progress.clone(), |v, p| {
                v.child(
                    div()
                        .text_size(px(crate::typography::ui_size(12.)))
                        .text_color(theme.text_secondary)
                        .child(p),
                )
            })
            .when(frame.is_some() && self.capture_error.is_some(), |v| {
                v.child(
                    div()
                        .text_size(px(crate::typography::ui_size(11.)))
                        .text_color(theme.text_secondary)
                        .child(unavailable),
                )
            })
            .when_some(error, |v, error| {
                v.child(
                    div()
                        .text_size(px(crate::typography::ui_size(12.)))
                        .text_color(theme.warning)
                        .child(error),
                )
            });
        let controls = div()
            .px(px(8.))
            .py(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .id("computer-use-operations")
                    .role(Role::Button)
                    .aria_label(crate::i18n::format!("操作过程" => "Operation history"))
                    .aria_expanded(self.show_operations)
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(8.))
                    .hover(|s| s.bg(theme.sidebar_hover))
                    .text_size(px(crate::typography::ui_size(12.)))
                    .child(crate::i18n::format!("操作过程" => "Operations"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_operations = !this.show_operations;
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("computer-use-takeover")
                    .role(Role::Button)
                    .aria_label(if running(self.phase) {
                        crate::i18n::format!("停止并接管" => "Stop and take over")
                    } else {
                        crate::i18n::format!("打开应用" => "Open app")
                    })
                    .aria_disabled(
                        !target_available
                            || self.takeover_pending
                            || self.phase == ConversationPhase::Stopping,
                    )
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(8.))
                    .bg(theme.sidebar_hover)
                    .text_size(px(crate::typography::ui_size(12.)))
                    .opacity(if target_available { 1. } else { 0.4 })
                    .child(if running(self.phase) {
                        crate::i18n::format!("接管" => "Take over")
                    } else {
                        crate::i18n::format!("打开应用" => "Open app")
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.take_over(cx))),
            );
        let operations = div()
            .id("computer-use-operation-list")
            .max_h(px(180.))
            .overflow_y_scroll()
            .px(px(12.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(snapshot.operations.iter().map(|(title, status)| {
                let mark = match status {
                    AgentMcpToolCallStatus::Completed => "✓",
                    AgentMcpToolCallStatus::Failed => "!",
                    AgentMcpToolCallStatus::InProgress => "…",
                };
                div()
                    .text_size(px(crate::typography::ui_size(12.)))
                    .text_color(if *status == AgentMcpToolCallStatus::Failed {
                        theme.warning
                    } else {
                        theme.text_secondary
                    })
                    .child(format!("{mark} {title}"))
            }));
        let retry = div()
            .id("computer-use-preview-retry")
            .role(Role::Button)
            .aria_label(crate::i18n::format!("重试窗口预览" => "Retry window preview"))
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .mx(px(12.))
            .mb(px(8.))
            .text_size(px(crate::typography::ui_size(12.)))
            .child(crate::i18n::format!("重试窗口预览" => "Retry window preview"))
            .on_click(cx.listener(|this, _, _, cx| {
                if running(this.phase)
                    && let Some(target) = this.snapshot.as_ref().and_then(|s| s.target.clone())
                {
                    this.capture_task = None;
                    this.start_capture(target, cx);
                }
            }));
        div()
            .id("computer-use-preview")
            .debug_selector(|| "computer-use-preview".into())
            .absolute()
            .top(px(58.))
            .right(px(16.))
            .w(px(320.))
            .max_w_full()
            .rounded(px(16.))
            .border_1()
            .border_color(theme.text.alpha(0.12))
            .bg(theme.control)
            .shadow_md()
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_color(theme.text)
            .child(header)
            .when(self.expanded, |root| {
                root.child(image)
                    .child(information)
                    .child(controls)
                    .when(self.show_operations, |v| v.child(operations))
                    .when(
                        self.capture_error == Some(CaptureError::PermissionRequired)
                            && running(self.phase),
                        |v| v.child(retry),
                    )
            })
            .into_any_element()
    }
}
