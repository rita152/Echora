//! Building blocks for settings content that comes from the backend (hooks,
//! experimental features, memories): the reference's section heading, card,
//! 60.56 px row, switch and dialog frame, without the static pages' nudges.

use gpui::{AnyElement, IntoElement, Role, SharedString, div, prelude::*, px, rgba, svg};

use super::SettingsView;
use crate::theme::{Theme, ThemeMode};

/// The reference switch: 32×20 track, 16 px white knob, blue when on.
pub(super) fn switch(
    id: impl Into<gpui::ElementId>,
    checked: bool,
    disabled: bool,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(Role::Switch)
        .aria_toggled(if checked {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .w(px(32.0))
        .h(px(20.0))
        .flex_none()
        .p(px(2.0))
        .rounded_full()
        .flex()
        .items_center()
        .when(checked, |track| track.justify_end().bg(rgba(0x339cffff)))
        .when(!checked, |track| {
            track.justify_start().bg(theme.settings_switch_off)
        })
        .when(disabled, |track| track.opacity(0.5))
        .when(!disabled, |track| track.cursor_pointer())
        .child(
            div()
                .size(px(16.0))
                .rounded_full()
                .bg(gpui::white())
                .border_1()
                .border_color(rgba(0x00000012)),
        )
}

/// A settings card: 20 px radius, hairline border, panel surface.
pub(super) fn card(theme: Theme) -> gpui::Div {
    div()
        .w_full()
        .rounded(px(20.0))
        .overflow_hidden()
        .border_1()
        .border_color(theme.border)
        .bg(theme.settings_panel)
        .flex()
        .flex_col()
}

/// 14 px medium section heading with an optional line below it.
pub(super) fn heading(title: String, subtitle: Option<AnyElement>, theme: Theme) -> gpui::Div {
    div()
        .min_h(px(32.0))
        .pb(px(6.0))
        .flex()
        .flex_col()
        .justify_end()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(21.0))
                .font_weight(gpui::FontWeight(500.0))
                .text_color(theme.text)
                .child(title),
        )
        .children(subtitle)
}

/// A card row: 13 px medium label over a 12 px description, a trailing
/// control, and the inset divider every row but the last draws.
pub(super) fn row(
    label: impl IntoElement,
    description: Option<String>,
    trailing: Option<AnyElement>,
    last: bool,
    theme: Theme,
) -> gpui::Div {
    div()
        .min_h(px(60.5625))
        .flex_none()
        .px(px(16.0))
        .py(px(12.0))
        .relative()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.0))
        .when(!last, |row| {
            row.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left(px(16.0))
                    .right(px(16.0))
                    .h(px(1.0))
                    .bg(theme.border),
            )
        })
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.5714))
                        .font_weight(gpui::FontWeight(500.0))
                        .text_color(theme.text)
                        .child(label),
                )
                .when_some(description, |column, description| {
                    column.child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(theme.settings_description)
                            .child(description),
                    )
                }),
        )
        .children(trailing)
}

/// Warning orange of the reference (`text-warning`).
pub(super) fn warning_color(mode: ThemeMode) -> gpui::Rgba {
    match mode {
        ThemeMode::Dark => rgba(0xff8549ff),
        ThemeMode::Light => rgba(0xd25e28ff),
    }
}

pub(super) fn danger_color(mode: ThemeMode) -> gpui::Rgba {
    match mode {
        ThemeMode::Dark => rgba(0xff6764ff),
        ThemeMode::Light => rgba(0xe02e2aff),
    }
}

/// The reference dialog surface over a 13% dim, as the composer dialogs.
pub(super) fn dialog_surface(mode: ThemeMode) -> (gpui::Rgba, gpui::Rgba) {
    match mode {
        ThemeMode::Dark => (rgba(0x2b2a2aff), rgba(0xffffff0d)),
        ThemeMode::Light => (rgba(0xfdfcfcff), rgba(0x0000000f)),
    }
}

/// An outlined pill button (the reference's `outline` / `composerSm`).
pub(super) fn outline_button(
    id: impl Into<gpui::ElementId>,
    label: String,
    icon: Option<&'static str>,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.clone()))
        .h(px(24.0))
        .px(px(8.0))
        .flex_none()
        .rounded_full()
        .border_1()
        .border_color(theme.border)
        .bg(theme.text.alpha(0.03))
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(px(13.0))
        .line_height(px(18.0))
        .text_color(theme.text)
        .cursor_pointer()
        .hover(move |button| button.bg(theme.sidebar_hover))
        .when_some(icon, |button, icon| {
            button.child(
                svg()
                    .path(SharedString::from(format!("icons/{icon}.svg")))
                    .size(px(14.0))
                    .text_color(theme.text),
            )
        })
        .child(label)
}

impl SettingsView {
    /// Page title and subtitle with an optional trailing action, as the
    /// reference's settings page header.
    pub(super) fn dynamic_page_header(
        &self,
        title: String,
        subtitle: AnyElement,
        action: Option<AnyElement>,
    ) -> gpui::Div {
        div()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .relative()
                            .top(px(-1.0))
                            .font_family(crate::theme::UI_FONT_FAMILY)
                            .text_size(px(24.0))
                            .line_height(px(31.0))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .child(title),
                    )
                    .child(div().relative().left(px(1.0)).mt(px(3.8)).child(subtitle)),
            )
            .children(action)
    }
}
