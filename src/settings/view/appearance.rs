//! Appearance settings presentation.

use gpui::{Context, IntoElement, WindowAppearance, div, prelude::*, px};

use super::{ChangeTheme, SettingsView};
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
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
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
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
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(crate::i18n::text(page.label)),
            )
            .child(
                div()
                    .mt(px(41.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(crate::i18n::text("主题")),
            )
            .child(previews)
            .into_any_element()
    }
}
