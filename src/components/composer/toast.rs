//! Short confirmations and errors over the conversation, as the reference's
//! success and danger toasts:
//! top-centre, 8px from the top, at most three stacked 8px apart, and gone
//! after five seconds.

use std::time::Duration;

use gpui::{AnyElement, Context, Role, div, prelude::*, px, rgba};

use super::ComposerView;
use crate::{components::icons::icon, theme::ThemeMode};

const TOAST_DURATION: Duration = Duration::from_secs(5);
const VISIBLE_TOASTS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Success,
    Danger,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComposerToast {
    id: u64,
    pub(crate) kind: ToastKind,
    pub(crate) text: String,
}

impl ComposerView {
    pub(super) fn show_toast(&mut self, kind: ToastKind, text: String, cx: &mut Context<Self>) {
        self.next_toast_id += 1;
        let id = self.next_toast_id;
        self.toasts.push(ComposerToast { id, kind, text });
        let overflow = self.toasts.len().saturating_sub(VISIBLE_TOASTS);
        self.toasts.drain(..overflow);
        let timer = cx.background_executor().timer(TOAST_DURATION);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| this.dismiss_toast(id, cx));
        })
        .detach();
        cx.notify();
    }

    fn dismiss_toast(&mut self, id: u64, cx: &mut Context<Self>) {
        self.toasts.retain(|toast| toast.id != id);
        cx.notify();
    }

    pub(crate) fn toasts(&self) -> &[ComposerToast] {
        &self.toasts
    }

    /// The toast stack, positioned by the caller over the conversation.
    pub fn render_toasts(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.toasts().is_empty() {
            return None;
        }
        let dark = self.mode == ThemeMode::Dark;
        let toasts = self.toasts().iter().map(|toast| {
            // The reference's rich toast colours (default Electron theme).
            let (text, background, border, glyph) = match (toast.kind, dark) {
                (ToastKind::Success, false) => (
                    rgba(0x00a240ff),
                    rgba(0xedfaf2ff),
                    rgba(0x00a24033),
                    "toast-success",
                ),
                (ToastKind::Success, true) => (
                    rgba(0x40c977ff),
                    rgba(0x011c0bff),
                    rgba(0x40c97733),
                    "toast-success",
                ),
                (ToastKind::Danger, false) => (
                    rgba(0xe02e2aff),
                    rgba(0xfff0f0ff),
                    rgba(0xe02e2a26),
                    "toast-danger",
                ),
                (ToastKind::Danger, true) => (
                    rgba(0xff6764ff),
                    rgba(0x280b0aff),
                    rgba(0xfa423e66),
                    "toast-danger",
                ),
            };
            let id = toast.id;
            div()
                .id(("composer-toast", id))
                .role(Role::Status)
                .aria_label(toast.text.clone())
                .max_w_full()
                .p(px(8.))
                .rounded(px(12.))
                .border_1()
                .border_color(border)
                .bg(background)
                .text_color(text)
                .text_size(px(14.))
                .line_height(px(19.6))
                .shadow(vec![gpui::BoxShadow {
                    color: rgba(0x0000001a).into(),
                    offset: gpui::point(px(0.), px(4.)),
                    blur_radius: px(12.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .flex()
                .items_start()
                .gap(px(4.))
                .debug_selector(|| "COMPOSER_TOAST".to_owned())
                .child(
                    div()
                        .size(px(24.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(glyph, text.into()).size(px(16.))),
                )
                .child(
                    div()
                        .min_h(px(24.))
                        .flex()
                        .items_center()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(toast.text.clone()),
                )
                .child(
                    div()
                        .id(("composer-toast-close", id))
                        .role(Role::Button)
                        .aria_label(crate::i18n::format!("关闭" => "Close"))
                        .size(px(24.))
                        .flex_none()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |button| button.bg(gpui::Rgba { a: 0.05, ..text }))
                        .on_click(cx.listener(move |this, _, _, cx| this.dismiss_toast(id, cx)))
                        .child(icon("close-dialog", text.into()).size(px(16.))),
                )
        });
        Some(
            div()
                .absolute()
                .top(px(8.))
                .left_0()
                .right_0()
                .px(px(8.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .children(toasts)
                .into_any_element(),
        )
    }
}
