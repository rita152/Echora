//! General settings presentation: the two rows that read and write real state,
//! the interface language and the follow-up behavior.

use gpui::{Context, IntoElement, div, prelude::*, px};

use super::{
    SettingsView,
    dynamic::{card, heading, row},
};
use crate::{settings::PageSpec, theme::Theme};

/// Queue first, steer second, matching `FollowUpMode`.
const FOLLOW_UP_LABELS: [&str; 2] = ["排队", "引导"];

impl SettingsView {
    /// Follow-up behavior: a real two-option control backed by the local UI
    /// preference, pressed state exposed like the reference's toggle buttons.
    fn follow_up_control(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        use crate::workspace::FollowUpMode;
        let selected = match self.follow_up_mode {
            FollowUpMode::Queue => 0,
            FollowUpMode::Steer => 1,
        };
        let mut group = div()
            .id("follow-up-mode")
            .role(gpui::Role::Group)
            .aria_label(crate::i18n::text("跟进处理方式"))
            .flex()
            .items_center()
            .gap(px(2.0));
        for (index, label) in FOLLOW_UP_LABELS.into_iter().enumerate() {
            let mode = if index == 0 {
                FollowUpMode::Queue
            } else {
                FollowUpMode::Steer
            };
            group = group.child(
                div()
                    .id(("follow-up-mode-option", index))
                    .role(gpui::Role::Button)
                    .aria_label(crate::i18n::text(label))
                    .aria_toggled(if index == selected {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .focusable()
                    .tab_stop(true)
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded_full()
                    .text_size(px(13.0))
                    .cursor_pointer()
                    .text_color(if index == selected {
                        theme.text
                    } else {
                        theme.text_tertiary
                    })
                    .when(index == selected, |item| item.bg(theme.settings_button))
                    .hover(move |item| item.text_color(theme.text))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.choose_follow_up_mode(mode, cx)),
                    )
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.choose_follow_up_mode(mode, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(crate::i18n::text(label)),
            );
        }
        group.into_any_element()
    }

    fn choose_follow_up_mode(
        &mut self,
        mode: crate::workspace::FollowUpMode,
        cx: &mut Context<Self>,
    ) {
        if self.follow_up_mode != mode {
            self.follow_up_mode = mode;
            cx.emit(super::ChangeFollowUpMode(mode));
            cx.notify();
        }
    }

    pub(super) fn general_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let language = row(
            crate::i18n::text("语言").to_owned(),
            Some(crate::i18n::text("应用 UI 语言").to_owned()),
            Some(self.language_control(theme, cx)),
            true,
            theme,
        );
        let follow_up = row(
            crate::i18n::text("跟进处理方式").to_owned(),
            Some(
                crate::i18n::text(
                    "在 ChatGPT 运行时将后续消息加入队列，或引导当前运行。按 ⌘⏎ 可对单条消息执行相反操作",
                )
                .to_owned(),
            ),
            Some(self.follow_up_control(theme, cx)),
            true,
            theme,
        );
        let section = |title: &'static str, row: gpui::Div| {
            div()
                .w_full()
                .flex()
                .flex_col()
                .child(heading(crate::i18n::text(title).to_owned(), None, theme))
                .child(card(theme).child(row))
        };
        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(64.0))
            .pb(px(96.0))
            .flex()
            .flex_col()
            .gap(px(30.0))
            .child(
                div()
                    .relative()
                    .top(px(1.0))
                    .text_size(px(24.0))
                    .line_height(px(31.0))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text)
                    .child(crate::i18n::text(page.label)),
            )
            .child(section("常规", language))
            .child(section("编辑器", follow_up))
            .into_any_element()
    }
}
