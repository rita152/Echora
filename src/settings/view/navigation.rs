//! Navigation settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::{PageSpec, page},
    theme::Theme,
};

impl SettingsView {
    pub(super) fn nav_icon(slug: &'static str, theme: Theme) -> impl IntoElement {
        svg()
            .path(format!("icons/settings-{slug}.svg"))
            .size(px(16.0))
            .relative()
            .top(px(1.0))
            .text_color(theme.text)
    }
    pub(super) fn sidebar_edge_shade(
        theme: Theme,
        nav_scroll: &gpui::ScrollHandle,
    ) -> gpui::AnyElement {
        // GPUI's scrollbar is intentionally hidden on the scroll container so
        // it cannot reserve layout width. Recreate ChatGPT's slim native rail
        // as a real, scroll-aware thumb: 8px wide, 3px inset from the track,
        // with a 11px right gutter. Reading the handle here keeps the thumb
        // moving when the user scrolls instead of baking the capture position
        // into the settings shell.
        let bounds = nav_scroll.bounds();
        let viewport_top = f32::from(bounds.origin.y);
        let viewport_height = f32::from(bounds.size.height);
        if viewport_height <= 1.0 {
            return div()
                .absolute()
                .right(px(11.0))
                .top(px(133.0))
                .w(px(8.0))
                .h(px(680.0))
                .rounded_full()
                .bg(if theme.surface == gpui::rgba(0x181818ff) {
                    gpui::rgba(0x343434ff)
                } else {
                    gpui::rgba(0xebebebff)
                })
                .into_any_element();
        }
        let max_offset = f32::from(nav_scroll.max_offset().y).max(0.0);
        let track_inset = 3.0;
        let track_height = (viewport_height - track_inset * 2.0).max(1.0);
        let thumb_height = (track_height * viewport_height
            / (viewport_height + max_offset).max(1.0))
        .max(24.0)
        .min(track_height);
        let travel = (track_height - thumb_height).max(0.0);
        let progress = if max_offset > 0.0 {
            (-f32::from(nav_scroll.offset().y) / max_offset).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let thumb_top = viewport_top + track_inset + travel * progress;
        let thumb_color = if theme.surface == gpui::rgba(0x181818ff) {
            gpui::rgba(0x343434ff)
        } else {
            gpui::rgba(0xebebebff)
        };
        div()
            .absolute()
            .right(px(11.0))
            .top(px(thumb_top))
            .w(px(8.0))
            .h(px(thumb_height))
            .rounded_full()
            .bg(thumb_color)
            .into_any_element()
    }
    pub(super) fn nav_row(
        &self,
        item: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.selected == item.slug;
        let slug = item.slug;
        let nav_text = if theme.surface == gpui::rgba(0x181818ff) {
            gpui::rgba(0xb4b4b4ff)
        } else {
            gpui::rgba(0x363636ff)
        };
        div()
            .id(slug)
            .role(gpui::Role::Button)
            .aria_label(crate::i18n::text(item.label))
            .aria_selected(selected)
            .focusable()
            .tab_stop(true)
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "enter" | "space" => {
                            this.select(slug, cx);
                            cx.stop_propagation();
                        }
                        "up" => {
                            window.focus_prev(cx);
                            cx.stop_propagation();
                        }
                        "down" => {
                            window.focus_next(cx);
                            cx.stop_propagation();
                        }
                        _ => cx.propagate(),
                    }
                }),
            )
            .h(px(30.0))
            .flex_none()
            .px(px(8.0))
            .rounded(px(12.5))
            .focus_visible(move |style| {
                style.bg(theme.sidebar_hover).shadow(vec![
                    gpui::BoxShadow::new(px(0.), px(0.), theme.accent.into()).spread_radius(px(2.)),
                ])
            })
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(14.0))
            .line_height(px(21.0))
            .text_color(nav_text)
            .when(selected, |row| row.bg(theme.sidebar_hover))
            .cursor_pointer()
            .when(!cfg!(feature = "screenshot"), |row| {
                row.hover(move |style| style.bg(theme.sidebar_hover))
            })
            .on_click(cx.listener(move |this, _, _, cx| this.select(slug, cx)))
            .child(Self::nav_icon(slug, theme))
            .child(crate::i18n::text(item.label))
    }
    pub(super) fn nav_group(
        &self,
        title: &'static str,
        slugs: &'static [&'static str],
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // ChatGPT uses a 4px gap only between the section heading and its
        // first row; consecutive rows are separated by a 1px rhythm. Keep
        // that distinction explicit so the long settings nav does not drift
        // vertically as more entries are added.
        let mut group = div().flex_none().flex().flex_col().gap(px(1.0)).child(
            div()
                .h(px(21.0))
                .mb(px(3.0))
                .px(px(8.0))
                .relative()
                .top(px(1.0))
                .flex()
                .items_center()
                .text_size(px(14.0))
                .line_height(px(21.0))
                .font_weight(gpui::FontWeight(500.0))
                .text_color(theme.text_tertiary)
                .child(crate::i18n::text(title)),
        );
        for slug in slugs {
            if let Some(item) = page(slug) {
                group = group.child(self.nav_row(item, theme, cx));
            }
        }
        group
    }
}
