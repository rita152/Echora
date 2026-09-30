//! Short confirmations and errors over the conversation, as the reference's
//! success and danger toasts:
//! top-centre, 8px from the top, at most three stacked 8px apart, and gone
//! after five seconds.

use std::time::Duration;

use gpui::{AnyElement, Context, Role, div, prelude::*, px, rgba};

use super::ComposerView;
use crate::{components::icons::icon, theme::ThemeMode};

pub(crate) const TOAST_DURATION: Duration = Duration::from_secs(5);
pub(crate) const VISIBLE_TOASTS: usize = 3;

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
    /// The label of the toast's action button (the reference's "Undo").
    pub(crate) action: Option<String>,
}

/// What a toast's action button does; run once, then the toast closes.
pub(crate) type ToastAction = std::rc::Rc<dyn Fn(&mut gpui::Window, &mut gpui::App)>;

impl ComposerView {
    pub(super) fn show_toast(&mut self, kind: ToastKind, text: String, cx: &mut Context<Self>) {
        self.push_toast(kind, text, None, cx);
    }

    /// A toast shown for another surface of this conversation (the summary
    /// panel), optionally with an action button such as "Undo".
    pub(crate) fn show_panel_toast(
        &mut self,
        kind: ToastKind,
        text: String,
        action: Option<(String, ToastAction)>,
        cx: &mut Context<Self>,
    ) {
        self.push_toast(kind, text, action, cx);
    }

    fn push_toast(
        &mut self,
        kind: ToastKind,
        text: String,
        action: Option<(String, ToastAction)>,
        cx: &mut Context<Self>,
    ) {
        self.next_toast_id += 1;
        let id = self.next_toast_id;
        let label = action.as_ref().map(|(label, _)| label.clone());
        if let Some((_, handler)) = action {
            self.toast_actions.insert(id, handler);
        }
        self.toasts.push(ComposerToast {
            id,
            kind,
            text,
            action: label,
        });
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
        self.toast_actions.remove(&id);
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
        let toasts: Vec<_> = self
            .toasts()
            .iter()
            .map(|toast| {
                let id = toast.id;
                let card = toast_card(
                    id,
                    toast.kind,
                    &toast.text,
                    dark,
                    cx.listener(move |this, _, _, cx| this.dismiss_toast(id, cx)),
                );
                match (&toast.action, self.toast_actions.get(&id).cloned()) {
                    (Some(label), Some(handler)) => card.child(toast_action_button(
                        id,
                        label,
                        toast.kind,
                        dark,
                        cx.listener(move |this, _, window, cx| {
                            handler(window, cx);
                            this.dismiss_toast(id, cx);
                        }),
                    )),
                    _ => card,
                }
            })
            .collect();
        Some(toast_stack(toasts))
    }
}

/// One toast in the reference's rich colours (default Electron theme). Shared
/// by every surface that shows toasts; each owns its own stack and timers.
pub(crate) fn toast_card(
    id: u64,
    kind: ToastKind,
    label: &str,
    dark: bool,
    on_close: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let (text, background, border, glyph) = match (kind, dark) {
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
    div()
        .id(("composer-toast", id))
        .role(Role::Status)
        .aria_label(label.to_owned())
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
                .child(label.to_owned()),
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
                .on_click(on_close)
                .child(icon("close-dialog", text.into()).size(px(16.))),
        )
}

/// The toast's action, a text button after its label.
fn toast_action_button(
    id: u64,
    label: &str,
    kind: ToastKind,
    dark: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let text = match (kind, dark) {
        (ToastKind::Success, false) => rgba(0x00a240ff),
        (ToastKind::Success, true) => rgba(0x40c977ff),
        (ToastKind::Danger, false) => rgba(0xe02e2aff),
        (ToastKind::Danger, true) => rgba(0xff6764ff),
    };
    div()
        .id(("composer-toast-action", id))
        .role(Role::Button)
        .aria_label(label.to_owned())
        .h(px(24.))
        .px(px(8.))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .text_color(text)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(move |button| button.bg(gpui::Rgba { a: 0.08, ..text }))
        .on_click(on_click)
        .child(label.to_owned())
}

/// Top-centre, 8px from the top, toasts stacked 8px apart.
pub(crate) fn toast_stack(toasts: impl IntoIterator<Item = impl IntoElement>) -> AnyElement {
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
        .into_any_element()
}
