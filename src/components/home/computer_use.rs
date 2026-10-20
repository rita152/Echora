//! Continuous native window previews, stacked inside the conversation's top right.
mod model;
#[cfg(test)]
mod tests;

use crate::{
    agent::AgentMcpToolCallStatus,
    components::{composer::ComposerView, icons::icon},
    computer_use::{self, CaptureError, WindowId, WindowInfo},
    conversation::{ConversationActivity, ConversationPhase},
    theme::{Theme, ThemeMode},
};
use base64::Engine as _;
use gpui::{
    Context, Entity, Image, IntoElement, ObjectFit, Render, Role, Window, div, prelude::*, px,
};
use model::Snapshot;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

struct WindowPreview {
    info: WindowInfo,
    #[cfg(target_os = "macos")]
    stream: Option<computer_use::Stream>,
    #[cfg(target_os = "macos")]
    pending: Option<async_channel::Receiver<Result<computer_use::Stream, CaptureError>>>,
    #[cfg(target_os = "macos")]
    frame: Option<core_video::pixel_buffer::CVPixelBuffer>,
    error: Option<CaptureError>,
}
impl WindowPreview {
    fn new(info: WindowInfo) -> Self {
        Self {
            info,
            #[cfg(target_os = "macos")]
            stream: None,
            #[cfg(target_os = "macos")]
            pending: None,
            #[cfg(target_os = "macos")]
            frame: None,
            error: None,
        }
    }
    fn stop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            self.stream = None;
            self.pending = None;
        }
    }
}

pub(super) struct Preview {
    owner: Option<(Entity<ComposerView>, u64)>,
    snapshot: Option<Snapshot>,
    phase: ConversationPhase,
    pub(super) mode: ThemeMode,
    expanded: bool,
    toggle_focus: Option<gpui::FocusHandle>,
    generation: u64,
    capture_task: Option<gpui::Task<()>>,
    inventory: Option<async_channel::Receiver<Result<Vec<WindowInfo>, CaptureError>>>,
    inventory_started: Instant,
    windows: Vec<WindowPreview>,
    selected_window: Option<WindowId>,
    tool_frame: Option<Arc<Image>>,
    tool_frame_key: Option<String>,
    capture_error: Option<CaptureError>,
    takeover_pending: Option<WindowInfo>,
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
            toggle_focus: None,
            generation: 0,
            capture_task: None,
            inventory: None,
            inventory_started: Instant::now() - Duration::from_secs(10),
            windows: vec![],
            selected_window: None,
            tool_frame: None,
            tool_frame_key: None,
            capture_error: None,
            takeover_pending: None,
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
            self.takeover_pending = None;
            self.intervention_error = None;
            self.seen_running = false;
        }
        self.mode = mode;
        self.phase = phase;
        self.seen_running |= running(phase);
        let next = Snapshot::from_activities(activities);
        if self.snapshot.as_ref().map(|s| &s.targets) != next.as_ref().map(|s| &s.targets) {
            self.stop_streams();
            self.inventory = None;
            self.inventory_started = Instant::now() - Duration::from_secs(10);
            let targets = next
                .as_ref()
                .map(|s| s.targets.as_slice())
                .unwrap_or_default();
            self.windows.retain(|w| targets.contains(&w.info.target));
            self.capture_error = None;
        }
        let key = next
            .as_ref()
            .and_then(|s| s.frame.as_ref())
            .map(|f| f.key.clone());
        if self.tool_frame_key != key {
            self.tool_frame = next.as_ref().and_then(|s| s.frame.as_ref()).and_then(|f| {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(&f.data)
                    .ok()?;
                crate::media::encoded_image_dimensions(&bytes)?;
                Some(Arc::new(Image::from_bytes(
                    super::media::tool_image_format(&bytes, f.mime.as_deref()),
                    bytes,
                )))
            });
            self.tool_frame_key = key;
        }
        self.snapshot = next;
        if !running(phase) {
            self.stop_streams();
            self.capture_task = None;
            self.inventory = None;
            self.generation = self.generation.wrapping_add(1);
            if phase == ConversationPhase::Failed && self.takeover_pending.take().is_some() {
                self.intervention_error = Some(
                    crate::i18n::format!("未确认 agent 已停止，请检查执行错误后再切换应用。" => "The agent has not confirmed it stopped. Check the execution error before switching apps."),
                );
            }
            if matches!(
                phase,
                ConversationPhase::Stopped | ConversationPhase::Complete
            ) && let Some(window) = self.takeover_pending.take()
            {
                self.focus(&window);
            }
            self.windows.clear();
            self.selected_window = None;
        }
    }
    fn stop_streams(&mut self) {
        for window in &mut self.windows {
            window.stop();
        }
    }
    fn cancel_capture(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.capture_task = None;
        self.inventory = None;
        self.windows.clear();
        self.selected_window = None;
        self.tool_frame = None;
        self.tool_frame_key = None;
        self.capture_error = None;
    }
    fn start_capture(&mut self, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.capture_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let keep = this
                    .update(cx, |this, cx| {
                        if this.generation != generation {
                            return false;
                        }
                        if !this.expanded
                            || !running(this.phase)
                            || this.rendered_at.elapsed() > Duration::from_secs(2)
                        {
                            this.stop_streams();
                            this.inventory = None;
                            this.capture_task = None;
                            return false;
                        }
                        this.tick(generation, cx);
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    return;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
            }
        }));
    }
    fn accept_windows(
        &mut self,
        generation: u64,
        result: Result<Vec<WindowInfo>, CaptureError>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            return;
        }
        match result {
            Ok(infos) => {
                let mut previous = std::mem::take(&mut self.windows);
                self.windows = infos
                    .into_iter()
                    .map(|info| {
                        if let Some(index) = previous.iter().position(|w| w.info.id == info.id) {
                            let mut w = previous.remove(index);
                            if w.info.width != info.width || w.info.height != info.height {
                                w.stop();
                                #[cfg(target_os = "macos")]
                                {
                                    w.frame = None;
                                }
                            }
                            w.info = info;
                            if !matches!(
                                w.error,
                                Some(
                                    CaptureError::PermissionRequired
                                        | CaptureError::AccessibilityRequired
                                        | CaptureError::Unsupported
                                )
                            ) {
                                w.error = None;
                            }
                            w
                        } else {
                            WindowPreview::new(info)
                        }
                    })
                    .collect();
                if let Some(selected) = self.selected_window {
                    if let Some(index) = self.windows.iter().position(|w| w.info.id == selected) {
                        let selected = self.windows.remove(index);
                        self.windows.insert(0, selected);
                    } else {
                        self.selected_window = None;
                    }
                }
                self.capture_error = if self.windows.is_empty() {
                    Some(CaptureError::WindowUnavailable)
                } else {
                    None
                };
            }
            Err(error) => {
                self.capture_error = Some(error);
                self.stop_streams();
            }
        }
        cx.notify();
    }
    fn tick(&mut self, generation: u64, cx: &mut Context<Self>) {
        if let Some(receiver) = &self.inventory {
            match receiver.try_recv() {
                Ok(result) => {
                    self.inventory = None;
                    self.accept_windows(generation, result, cx);
                }
                Err(async_channel::TryRecvError::Closed) => {
                    self.inventory = None;
                    self.capture_error = Some(CaptureError::CaptureFailed);
                    cx.notify();
                }
                Err(_) if self.inventory_started.elapsed() > Duration::from_secs(5) => {
                    self.inventory = None;
                    self.capture_error = Some(CaptureError::CaptureFailed);
                    cx.notify();
                }
                _ => {}
            }
        }
        if self.inventory.is_none()
            && self.inventory_started.elapsed() > Duration::from_secs(1)
            && !matches!(
                self.capture_error,
                Some(
                    CaptureError::PermissionRequired
                        | CaptureError::AccessibilityRequired
                        | CaptureError::Unsupported
                )
            )
        {
            let targets = self
                .snapshot
                .as_ref()
                .map(|s| s.targets.clone())
                .unwrap_or_default();
            if !targets.is_empty() {
                self.inventory = Some(computer_use::windows(targets));
                self.inventory_started = Instant::now();
            }
        }
        #[cfg(target_os = "macos")]
        for window in &mut self.windows {
            if let Some(pending) = &window.pending {
                match pending.try_recv() {
                    Ok(Ok(stream)) => {
                        window.stream = Some(stream);
                        window.pending = None;
                        window.error = None;
                    }
                    Ok(Err(error)) => {
                        window.error = Some(error);
                        window.pending = None;
                        cx.notify();
                    }
                    Err(async_channel::TryRecvError::Closed) => {
                        window.error = Some(CaptureError::CaptureFailed);
                        window.pending = None;
                        cx.notify();
                    }
                    _ => {}
                }
            }
            if let Some(stream) = &window.stream {
                match stream.take_frame() {
                    Ok(Some(frame)) => {
                        window.frame = Some(frame);
                        window.error = None;
                        cx.notify();
                    }
                    Err(error) => {
                        window.error = Some(error);
                        window.stream = None;
                        cx.notify();
                    }
                    _ => {}
                }
            }
            if window.stream.is_none() && window.pending.is_none() && window.error.is_none() {
                window.pending = Some(computer_use::Stream::start(window.info.clone()));
            }
        }
    }
    fn toggle(&mut self, cx: &mut Context<Self>) {
        self.expanded = !self.expanded;
        if !self.expanded {
            self.stop_streams();
            self.capture_task = None;
            self.generation = self.generation.wrapping_add(1);
        } else {
            self.capture_error = None;
            for w in &mut self.windows {
                w.error = None;
            }
        }
        cx.notify();
    }
    fn focus(&mut self, window: &WindowInfo) {
        if !computer_use::focus(window) {
            self.intervention_error = Some(
                crate::i18n::format!("无法打开目标窗口，请手动切换。" => "Could not focus the target window. Switch to it manually."),
            );
        }
    }
    fn take_over(&mut self, id: WindowId, cx: &mut Context<Self>) {
        if self.takeover_pending.is_some() || self.phase == ConversationPhase::Stopping {
            return;
        }
        let Some(window) = self
            .windows
            .iter()
            .find(|w| w.info.id == id)
            .map(|w| w.info.clone())
        else {
            return;
        };
        if running(self.phase) {
            if let Some((owner, _)) = &self.owner {
                self.takeover_pending = Some(window);
                owner.update(cx, |owner, cx| owner.stop_for_computer_use(cx));
            }
        } else {
            self.focus(&window);
        }
        cx.notify();
    }
    fn interact(&mut self, id: WindowId, cx: &mut Context<Self>) {
        let Some(index) = self.windows.iter().position(|w| w.info.id == id) else {
            return;
        };
        if index == 0 {
            self.take_over(id, cx);
        } else {
            // Match the reference: a rear preview is selected first. Taking
            // over the front preview is a separate click, so browsing the
            // stack does not interrupt an active task.
            self.selected_window = Some(id);
            let selected = self.windows.remove(index);
            self.windows.insert(0, selected);
            cx.notify();
        }
    }
    fn status(&self, snapshot: &Snapshot) -> String {
        if self.takeover_pending.is_some() || self.phase == ConversationPhase::Stopping {
            crate::i18n::format!("正在停止…" => "Stopping…")
        } else if self.phase == ConversationPhase::Failed
            || snapshot.status == AgentMcpToolCallStatus::Failed
        {
            crate::i18n::format!("操作失败" => "Action failed")
        } else if self.phase == ConversationPhase::Stopped {
            crate::i18n::format!("已停止" => "Stopped")
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
            Some(CaptureError::PermissionRequired) => {
                crate::i18n::format!("实时预览需要 macOS 屏幕录制权限。" => "Live preview needs macOS Screen Recording access.")
            }
            Some(CaptureError::AccessibilityRequired) => {
                crate::i18n::format!("识别应用窗口需要 macOS 辅助功能权限。" => "Identifying application windows needs macOS Accessibility access.")
            }
            Some(CaptureError::WindowUnavailable) => {
                crate::i18n::format!("目标应用当前没有可预览的窗口。" => "The target application has no visible window.")
            }
            Some(CaptureError::Unsupported) => {
                crate::i18n::format!("此系统不支持实时窗口预览。" => "Live window preview is unavailable on this system.")
            }
            Some(CaptureError::CaptureFailed) => {
                crate::i18n::format!("窗口预览暂不可用。" => "Window preview is unavailable.")
            }
            None => crate::i18n::format!("等待应用窗口…" => "Waiting for an application window…"),
        }
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.rendered_at = Instant::now();
        if self.expanded
            && running(self.phase)
            && self.capture_task.is_none()
            && self
                .snapshot
                .as_ref()
                .is_some_and(|s| !s.targets.is_empty())
        {
            self.start_capture(cx);
        }
        let Some(snapshot) = self
            .snapshot
            .as_ref()
            .filter(|_| self.seen_running && running(self.phase))
        else {
            return div().into_any_element();
        };
        let toggle_focus = self
            .toggle_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let theme = Theme::for_mode(self.mode);
        let status = self.status(snapshot);
        let available = (f32::from(window.viewport_size().width) - 72.).max(96.);
        let width = 200f32.min(available);
        let available_height = (f32::from(window.viewport_size().height) - 200.).max(80.);
        let indices = self
            .windows
            .iter()
            .enumerate()
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let count = indices.len();
        let offset = if count > 1 {
            28f32.min((available - width) / (count - 1) as f32)
        } else {
            0.
        };
        let step = if count > 1 {
            22f32.min((available_height - 80.).max(0.) / (count - 1) as f32)
        } else {
            0.
        };
        let mut root = div()
            .id("computer-use-preview")
            .debug_selector(|| "computer-use-preview".into())
            .absolute()
            // Home already sits inside the conversation's 24 pt trailing
            // gutter. Keep the video below the title bar, matching the native
            // reference's 70 pt top anchor. Hover controls overlay the video.
            .top(px(70.))
            .right(px(0.))
            .text_color(theme.text);
        let toggle_label = if self.expanded {
            crate::i18n::format!("收起 Computer Use 预览" => "Collapse Computer Use preview")
        } else {
            crate::i18n::format!("展开 Computer Use 预览" => "Expand Computer Use preview")
        };
        let toggle = div()
            .id("computer-use-preview-toggle")
            .role(Role::Button)
            .aria_label(toggle_label)
            .aria_expanded(self.expanded)
            .track_focus(&toggle_focus)
            .tab_stop(true)
            .cursor_pointer()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(|this, _, _, cx| this.toggle(cx)));
        if !self.expanded {
            return root
                .child(
                    toggle
                        .size(px(32.))
                        .rounded(px(8.))
                        .bg(theme.control)
                        .child(
                            icon("activity-native-app", theme.text_secondary.into()).size(px(18.)),
                        ),
                )
                .into_any_element();
        }
        let header = div()
            .id("computer-use-preview-controls")
            .absolute()
            .top(px(4.))
            .left(px(offset * count.saturating_sub(1) as f32 + 4.))
            .h(px(24.))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                toggle
                    .size(px(20.))
                    .rounded(px(6.))
                    .bg(theme.control)
                    .child(icon("close-dialog", theme.text_secondary.into()).size(px(14.))),
            )
            .when(count == 0 || self.capture_error.is_some(), |view| {
                view.child(
                    div()
                        .id("computer-use-preview-retry")
                        .role(Role::Button)
                        .aria_label(crate::i18n::format!("重试窗口预览" => "Retry window preview"))
                        .focusable()
                        .tab_stop(true)
                        .cursor_pointer()
                        .text_size(px(11.))
                        .child(crate::i18n::text("重试"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.capture_error = None;
                            this.inventory_started = Instant::now() - Duration::from_secs(10);
                            for window in &mut this.windows {
                                window.error = None;
                            }
                            cx.notify();
                        })),
                )
            });
        root = root
            .group("computer-use-stack")
            .w(px(width + offset * count.saturating_sub(1) as f32));
        let mut stack = div().relative().w_full();
        let mut height = 80f32;
        for (index, source) in indices.into_iter().enumerate().rev() {
            let w = &self.windows[source];
            let id = w.info.id;
            let h = (width * w.info.height as f32 / w.info.width.max(1.) as f32)
                .min(available_height - step * count.saturating_sub(1) as f32)
                .max(48.);
            height = height.max(h + step * index as f32);
            let label = format!(
                "{} · {} · {} · {}{}",
                w.info.application,
                w.info.title,
                status,
                snapshot.title,
                snapshot
                    .progress
                    .as_ref()
                    .map(|p| format!(" · {p}"))
                    .unwrap_or_default()
            );
            let mut frame = div()
                .id(("computer-use-window", u64::from(id.window)))
                .role(Role::Button)
                .aria_label(label)
                .focusable()
                .tab_stop(true)
                .cursor_pointer()
                .occlude()
                .absolute()
                .top(px(step * index as f32))
                .right(px(offset * index as f32))
                .w(px(width))
                .h(px(h))
                .rounded(px(6.))
                .shadow_md()
                .on_click(cx.listener(move |this, _, _, cx| this.interact(id, cx)));
            #[cfg(target_os = "macos")]
            if let Some(buffer) = &w.frame {
                frame = frame.child(
                    gpui::surface(buffer.clone())
                        .size_full()
                        .rounded(px(6.))
                        .object_fit(ObjectFit::Contain),
                );
            }
            #[cfg(target_os = "macos")]
            let missing = w.frame.is_none();
            #[cfg(not(target_os = "macos"))]
            let missing = true;
            if missing {
                frame = frame
                    .bg(theme.control)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().px(px(12.)).text_size(px(11.)).text_center().child(
                        if w.error.is_some() {
                            crate::i18n::format!("窗口预览暂不可用" => "Window preview unavailable")
                        } else {
                            w.info.title.clone()
                        },
                    ));
            }
            stack = stack.child(frame);
        }
        if count == 0 {
            let target_label = snapshot
                .target
                .as_ref()
                .map_or(snapshot.label.as_str(), |t| t.label());
            let text = self
                .intervention_error
                .clone()
                .or_else(|| snapshot.error.clone())
                .unwrap_or_else(|| self.capture_message());
            stack = stack.child(
                div()
                    .id("computer-use-waiting")
                    .aria_label(target_label.to_owned())
                    .w(px(width))
                    .min_h(px(80.))
                    .p(px(12.))
                    .rounded(px(8.))
                    .bg(theme.control)
                    .text_size(px(11.))
                    .child(text)
                    .when_some(self.tool_frame.clone(), |v, image| {
                        v.child(
                            div().pt(px(6.)).child(
                                gpui::img(image)
                                    .w_full()
                                    .h(px(110.))
                                    .object_fit(ObjectFit::Contain),
                            ),
                        )
                        .child(crate::i18n::format!("最近的工具截图" => "Latest tool capture"))
                    }),
            );
        }
        root.child(stack.h(px(height)))
            .child(
                header
                    .opacity(0.)
                    .group_hover("computer-use-stack", |s| s.opacity(1.)),
            )
            .when_some(self.intervention_error.clone(), |v, e| {
                v.child(div().text_size(px(11.)).text_color(theme.warning).child(e))
            })
            .into_any_element()
    }
}
