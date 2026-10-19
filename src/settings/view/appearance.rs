//! Appearance settings presentation.

use gpui::{Context, Focusable, IntoElement, Window, WindowAppearance, div, prelude::*, px};

use super::{ChangeTheme, SettingsView};
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn observe_font_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.font_focus_subscriptions.is_empty() {
            return;
        }
        for index in 0..2 {
            let focus = self.font_inputs[index].focus_handle(cx);
            let subscription = cx.on_blur(&focus, window, move |this, _, cx| {
                let draft = this.font_inputs[index].read(cx).text().to_owned();
                this.commit_font_size(index, &draft, cx);
            });
            self.font_focus_subscriptions.push(subscription);
        }
    }

    pub(super) fn commit_font_size(&mut self, index: usize, draft: &str, cx: &mut Context<Self>) {
        let valid = draft
            .trim()
            .parse::<f32>()
            .ok()
            .and_then(|value| self.font_sizes.with_value(index == 1, value));
        let sizes = valid.unwrap_or(self.font_sizes);
        self.font_inputs[index].update(cx, |input, cx| {
            input.set_text_silently(
                if index == 0 { sizes.ui() } else { sizes.code() }.to_string(),
                cx,
            );
        });
        if sizes != self.font_sizes {
            self.font_sizes = sizes;
            cx.emit(super::ChangeFontSizes(sizes));
            cx.notify();
        }
    }

    fn font_size_row(
        &self,
        index: usize,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (label, description) = if index == 0 {
            (
                crate::i18n::format!("界面字号" => "UI font size"),
                crate::i18n::format!("调整 Echora 的基础字号" => "Adjust the base font size for Echora"),
            )
        } else {
            (
                crate::i18n::format!("代码字体大小" => "Code font size"),
                crate::i18n::format!("调整聊天和差异视图中代码使用的基础字号" => "Adjust the base size for code in chats and diffs"),
            )
        };
        div()
            .w_full()
            .py(px(10.0))
            .flex()
            .items_center()
            .gap(px(24.0))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(0.0))
                    .child(
                        div()
                            .text_size(px(crate::typography::ui_size(14.0)))
                            .line_height(px(crate::typography::ui_leading(14.0, 21.0)))
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(crate::typography::ui_size(13.0)))
                            .line_height(px(crate::typography::ui_leading(13.0, 18.5714)))
                            .text_color(theme.text_secondary)
                            .child(description),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id(("appearance-font-size", index))
                            .w(px(64.0))
                            .h(px(28.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.control_soft)
                            .on_key_down(cx.listener(
                                move |this, event: &gpui::KeyDownEvent, _, cx| {
                                    let direction = match event.keystroke.key.as_str() {
                                        "up" => 1.0,
                                        "down" => -1.0,
                                        _ => return,
                                    };
                                    let range = if index == 0 {
                                        crate::workspace::FontSizes::UI_RANGE
                                    } else {
                                        crate::workspace::FontSizes::CODE_RANGE
                                    };
                                    let draft = this.font_inputs[index]
                                        .read(cx)
                                        .text()
                                        .parse::<f32>()
                                        .ok()
                                        .filter(|v| v.is_finite());
                                    let value = draft
                                        .map(|v| {
                                            if direction > 0.0 {
                                                v.floor() + 1.0
                                            } else {
                                                v.ceil() - 1.0
                                            }
                                        })
                                        .unwrap_or(*range.start())
                                        .clamp(*range.start(), *range.end());
                                    this.font_inputs[index].update(cx, |input, cx| {
                                        input.set_text_silently(value.to_string(), cx)
                                    });
                                    cx.stop_propagation();
                                },
                            ))
                            .child(self.font_inputs[index].clone()),
                    )
                    .child(
                        div()
                            .text_size(px(crate::typography::ui_size(13.0)))
                            .text_color(theme.text_secondary)
                            .child("px"),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn appearance_preview(
        &self,
        mode: usize,
        selected: bool,
        theme: Theme,
    ) -> impl IntoElement {
        let (shell, asset) = match mode {
            1 => (gpui::rgba(0xf3f3f3ff), "icons/settings-theme-light.svg"),
            2 => (gpui::rgba(0x5d5d5dff), "icons/settings-theme-dark.svg"),
            _ => (gpui::rgba(0x9f9f9fff), "icons/settings-theme-system.svg"),
        };
        let mut preview = div()
            .w_full()
            .aspect_ratio(17.0 / 12.0)
            .rounded(px(12.5))
            .overflow_hidden()
            .when(selected, |node| node.border_2().border_color(theme.text))
            .when(!selected, |node| node.border_1().border_color(theme.border))
            .bg(shell)
            .relative();

        if mode == 0 {
            preview = preview.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .child(
                        div()
                            .h_full()
                            .flex_1()
                            .rounded_l(px(12.5))
                            .bg(gpui::rgba(0x9f9f9fff)),
                    )
                    .child(
                        div()
                            .h_full()
                            .flex_1()
                            .rounded_r(px(12.5))
                            .bg(gpui::rgba(0x5d5d5dff)),
                    ),
            );
        }
        preview.child(gpui::img(asset).size_full().rounded(px(12.5)))
    }
    pub(super) fn appearance_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let labels = [
            crate::i18n::text("系统"),
            crate::i18n::text("浅色"),
            crate::i18n::text("深色"),
        ];
        let mut previews = div().w_full().mt(px(18.5)).flex().gap(px(12.0));
        for (index, label) in labels.iter().enumerate() {
            let selected = self.appearance_theme == index;
            previews = previews.child(
                div()
                    .id(("appearance-theme", index))
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(crate::typography::ui_size(13.0)))
                    .line_height(px(crate::typography::ui_leading(13.0, 18.5714)))
                    .text_color(if selected {
                        theme.text
                    } else {
                        theme.text_secondary
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let mode = match index {
                            1 => ThemeMode::Light,
                            2 => ThemeMode::Dark,
                            _ => match window.appearance() {
                                WindowAppearance::Light | WindowAppearance::VibrantLight => {
                                    ThemeMode::Light
                                }
                                WindowAppearance::Dark | WindowAppearance::VibrantDark => {
                                    ThemeMode::Dark
                                }
                            },
                        };
                        this.mode = mode;
                        this.branch_prefix_input
                            .update(cx, |input, cx| input.set_mode(mode, cx));
                        for input in &this.font_inputs {
                            input.update(cx, |input, cx| input.set_mode(mode, cx));
                        }
                        this.appearance_theme = index;
                        cx.emit(ChangeTheme(mode));
                        cx.notify();
                    }))
                    .child(self.appearance_preview(index, selected, theme))
                    .child(crate::i18n::text(label)),
            );
        }

        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(64.0))
            .pb(px(80.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(crate::typography::ui_size(24.0)))
                    .line_height(px(crate::typography::ui_leading(24.0, 28.8)))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(crate::i18n::text(page.label)),
            )
            .child(
                div()
                    .mt(px(41.5))
                    .text_size(px(crate::typography::ui_size(14.0)))
                    .line_height(px(crate::typography::ui_leading(14.0, 21.0)))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(crate::i18n::text("主题")),
            )
            .child(previews)
            .child(
                div()
                    .mt(px(28.0))
                    .children((0..2).map(|index| self.font_size_row(index, theme, cx))),
            )
            .into_any_element()
    }
}
