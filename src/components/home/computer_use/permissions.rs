//! Permission recovery never requests access or changes the macOS TCC database.
use super::*;
use gpui::{Div, KeyDownEvent};

#[derive(Clone, Copy)]
enum RecoveryAction {
    Settings(&'static str),
    Retry,
    Help,
}

impl Preview {
    pub(super) fn permission_error(&self) -> Option<CaptureError> {
        self.capture_error
            .into_iter()
            .chain(self.windows.iter().filter_map(|window| window.error))
            .find(|error| error.settings_url().is_some())
    }

    pub(super) fn retry_capture(&mut self, cx: &mut Context<Self>) {
        // Retire any old enumeration and stream before re-reading permissions.
        self.generation = self.generation.wrapping_add(1);
        self.capture_task = None;
        self.inventory = None;
        self.stop_streams();
        self.windows.clear();
        self.selected_window = None;
        self.capture_error = None;
        self.inventory_started = Instant::now() - Duration::from_secs(10);
        cx.notify();
    }

    fn recover(&mut self, action: RecoveryAction, cx: &mut Context<Self>) {
        match action {
            RecoveryAction::Settings(url) => cx.open_url(url),
            RecoveryAction::Retry => self.retry_capture(cx),
            RecoveryAction::Help => {
                self.capture_help_open = !self.capture_help_open;
                cx.notify();
            }
        }
    }

    fn recovery_button(
        &mut self,
        id: &'static str,
        label: String,
        action: RecoveryAction,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let focus = self
            .recovery_focus
            .entry(id)
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        div()
            .id(id)
            .debug_selector(move || id.into())
            .role(Role::Button)
            .aria_label(label.clone())
            .when(matches!(action, RecoveryAction::Help), |view| {
                view.aria_expanded(self.capture_help_open)
            })
            .track_focus(&focus)
            .tab_stop(true)
            .w_full()
            .px(px(8.))
            .py(px(6.))
            .rounded(px(6.))
            .cursor_pointer()
            .bg(theme.surface)
            .hover(|s| s.bg(theme.sidebar_hover))
            .focus_visible(|s| s.bg(theme.sidebar_hover))
            .text_color(theme.markdown_link)
            .on_click(cx.listener(move |this, _, window, cx| {
                window.focus(&focus, cx);
                this.recover(action, cx);
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.recover(action, cx);
                    cx.stop_propagation();
                } else if event.keystroke.key == "tab" {
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                }
            }))
            .child(label)
    }

    pub(super) fn render_permission_recovery(
        &mut self,
        error: CaptureError,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let screen = error == CaptureError::PermissionRequired;
        let settings_label = if screen {
            crate::i18n::format!("打开屏幕录制设置" => "Open Screen Recording settings")
        } else {
            crate::i18n::format!("打开辅助功能设置" => "Open Accessibility settings")
        };
        let help = if screen {
            crate::i18n::format!("升级后，旧授权可能仍绑定旧版本。请在系统设置的屏幕录制列表中移除旧 Echora 项目，重新添加当前 Echora.app 并开启权限；按系统提示重启应用后再重试。屏幕录制与辅助功能是两项独立权限。" => "After an upgrade, an old grant may still identify the previous build. Remove the old Echora entry from Screen Recording settings, add the current Echora.app, enable access, then restart the app as macOS requests and retry. Screen Recording and Accessibility are separate permissions.")
        } else {
            crate::i18n::format!("请在辅助功能列表中允许当前 Echora.app。若已经开启仍失败，可移除旧项目并重新添加当前应用，再按系统提示重启。此权限用于识别和聚焦应用窗口，屏幕录制权限需单独开启。" => "Allow the current Echora.app in Accessibility settings. If it is already enabled, remove the old entry, add the current app, and restart as macOS requests. This permission identifies and focuses application windows; Screen Recording access is granted separately.")
        };
        div()
            .id("computer-use-permission-recovery")
            .debug_selector(|| "computer-use-permission-recovery".into())
            .w_full()
            .p(px(12.))
            .pt(px(34.))
            .rounded(px(8.))
            .occlude()
            .bg(theme.control)
            .text_size(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(self.capture_message())
            .child(self.recovery_button(
                "computer-use-open-settings",
                settings_label,
                RecoveryAction::Settings(error.settings_url().expect("permission error")),
                theme,
                cx,
            ))
            .child(self.recovery_button(
                "computer-use-permission-retry",
                crate::i18n::format!("重试窗口预览" => "Retry window preview"),
                RecoveryAction::Retry,
                theme,
                cx,
            ))
            .child(self.recovery_button(
                "computer-use-permission-help",
                crate::i18n::format!("已开启仍不可用？" => "Already enabled but unavailable?"),
                RecoveryAction::Help,
                theme,
                cx,
            ))
            .when(self.capture_help_open, |view| {
                view.child(
                    div()
                        .id("computer-use-permission-instructions")
                        .role(Role::Status)
                        .child(help),
                )
            })
    }
}
