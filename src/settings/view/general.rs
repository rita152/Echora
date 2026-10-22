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
    fn close_behavior(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Div {
        let checked = self.keep_running_on_close;
        let control = super::dynamic::switch("keep-running-on-close", checked, false, theme)
            .aria_label(crate::i18n::text("关闭窗口后在后台保留 Echora"))
            .focusable()
            .tab_stop(true)
            .on_click(cx.listener(move |this, _, _, cx| this.choose_keep_running(!checked, cx)))
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.choose_keep_running(!checked, cx);
                    cx.stop_propagation();
                }
            }));
        row(
            crate::i18n::text("关闭窗口后在后台保留 Echora"),
            Some(
                crate::i18n::text(
                    "默认关闭：关窗即退出。开启后保留会话与任务，点击 Dock 图标继续；⌘Q 始终退出。",
                )
                .to_owned(),
            ),
            Some(control.into_any_element()),
            true,
            theme,
        )
    }

    fn choose_keep_running(&mut self, checked: bool, cx: &mut Context<Self>) {
        self.keep_running_on_close = checked;
        cx.emit(super::ChangeKeepRunningOnClose(checked));
        cx.notify();
    }

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
                    .text_size(px(crate::typography::ui_size(13.0)))
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

    fn environment_settings(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Div {
        let mut content = card(theme);
        for (key, label, detail) in [
            (
                "open",
                "Open by default",
                "Open the Environment panel automatically. Your last open/close also updates this preference.",
            ),
            (
                "repository",
                "Repository",
                "Show the GitHub repository link. The git block always stays visible.",
            ),
            (
                "pull-requests",
                "Pull request",
                "Show pull requests attached to this chat and their checks.",
            ),
            (
                "editor",
                "Editor",
                "Show Editor view and the Open in editor picker.",
            ),
        ] {
            let checked = if key == "open" {
                self.environment_open
            } else {
                !self.environment_hidden_sections.iter().any(|s| s == key)
            };
            let control = super::dynamic::switch(
                gpui::SharedString::from(format!("environment-setting-{key}")),
                checked,
                false,
                theme,
            )
            .aria_label(label)
            .focusable()
            .tab_stop(true)
            .on_click(
                cx.listener(move |this, _, _, cx| {
                    this.choose_environment_setting(key, !checked, cx)
                }),
            )
            .on_key_down(cx.listener(move |this, e: &gpui::KeyDownEvent, _, cx| {
                if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                    this.choose_environment_setting(key, !checked, cx);
                    cx.stop_propagation();
                }
            }));
            content = content.child(row(
                label,
                Some(detail.into()),
                Some(control.into_any_element()),
                key != "editor",
                theme,
            ));
        }
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(heading("Environment panel".into(), None, theme))
            .child(content)
    }

    fn choose_environment_setting(&mut self, key: &str, checked: bool, cx: &mut Context<Self>) {
        if key == "open" {
            self.environment_open = checked;
            cx.emit(super::ChangeEnvironmentOpen(checked));
        } else {
            self.environment_hidden_sections.retain(|s| s != key);
            if !checked {
                self.environment_hidden_sections.push(key.to_owned());
            }
            cx.emit(super::ChangeEnvironmentSection(key.to_owned(), checked));
        }
        cx.notify();
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
                    .text_size(px(crate::typography::ui_size(24.0)))
                    .line_height(px(31.0))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text)
                    .child(crate::i18n::text(page.label)),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(heading(crate::i18n::text("常规").to_owned(), None, theme))
                    .child(
                        card(theme)
                            .child(language)
                            .when(cfg!(target_os = "macos"), |card| {
                                card.child(self.close_behavior(theme, cx))
                            }),
                    ),
            )
            .child(section("编辑器", follow_up))
            .child(self.environment_settings(theme, cx))
            .into_any_element()
    }
}
