//! Git settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px};

use super::{
    SettingsView,
    dynamic::{card, row},
};
use crate::{settings::PageSpec, theme::Theme};

impl SettingsView {
    /// Review delivery: a real two-option control backed by the local UI
    /// preference, drawn like the reference's pill toggles.
    fn review_delivery_control(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        use crate::workspace::ReviewDelivery;
        let options = [
            (
                ReviewDelivery::Inline,
                crate::i18n::format!("内联" => "Inline"),
            ),
            (
                ReviewDelivery::Detached,
                crate::i18n::format!("单独" => "Detached"),
            ),
        ];
        let mut group = div()
            .id("review-delivery")
            .role(gpui::Role::Group)
            .aria_label(crate::i18n::format!("审查结果呈现方式" => "Review delivery"))
            .flex()
            .items_center()
            .gap(px(2.0));
        for (index, (delivery, label)) in options.into_iter().enumerate() {
            let selected = self.review_delivery == delivery;
            group =
                group.child(
                    div()
                        .id(("review-delivery-option", index))
                        .debug_selector(move || format!("REVIEW_DELIVERY_{index}"))
                        .role(gpui::Role::Button)
                        .aria_label(gpui::SharedString::from(label.clone()))
                        .aria_toggled(if selected {
                            gpui::Toggled::True
                        } else {
                            gpui::Toggled::False
                        })
                        .focusable()
                        .tab_stop(true)
                        .h(px(24.0))
                        .px(px(8.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .text_size(px(13.0))
                        .cursor_pointer()
                        .text_color(if selected {
                            theme.text
                        } else {
                            theme.text_tertiary
                        })
                        .when(selected, |item| item.bg(theme.sidebar_hover))
                        .hover(move |item| item.text_color(theme.text))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choose_review_delivery(delivery, cx)
                        }))
                        .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                this.choose_review_delivery(delivery, cx);
                                cx.stop_propagation();
                            }
                        }))
                        .child(label),
                );
        }
        group.into_any_element()
    }

    pub(super) fn choose_review_delivery(
        &mut self,
        delivery: crate::workspace::ReviewDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.review_delivery != delivery {
            self.review_delivery = delivery;
            cx.emit(super::ChangeReviewDelivery(delivery));
            cx.notify();
        }
    }

    pub(super) fn git_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let delivery = row(
            crate::i18n::text("审查结果呈现方式").to_owned(),
            Some(
                crate::i18n::text("尽可能在当前聊天中启动 /review，或启动单独的审查聊天")
                    .to_owned(),
            ),
            Some(self.review_delivery_control(theme, cx)),
            true,
            theme,
        );
        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(66.0))
            .pb(px(80.0))
            .child(
                div()
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text)
                    .child(crate::i18n::text(page.label)),
            )
            .child(div().mt(px(32.0)).child(card(theme).child(delivery)))
            .into_any_element()
    }
}
