//! The tab's content area: the New tab page, the native page placed over
//! the area every frame, or the reference's network-error page. The zoom
//! banner floats over the page's top right after a zoom command.

use gpui::{
    AnyElement, Context, Div, FontWeight, MouseButton, Role, Stateful, Window, canvas, div, img,
    prelude::*, px, rgba,
};

use super::{BrowserPanel, BrowserTheme, TabError};
use crate::{browser::webview::WebViewFrame, components::icons::icon};

/// The card's radius less its hairline, for the page's bottom corners.
const PAGE_CORNER_RADIUS: f32 = 11.0;
/// The right panel's 16 px resize handle is centred on the card's left edge;
/// its inner half lies over the page, less the 1 px the page is inset.
const RESIZE_HANDLE_OVERLAP: f32 = 7.0;

impl BrowserPanel {
    pub(super) fn render_content(
        &self,
        theme: BrowserTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let area = div()
            .id("browser-content")
            .relative()
            .flex_1()
            .min_h(px(0.))
            .w_full()
            .overflow_hidden();
        let Some(tab) = self.active_tab() else {
            return area;
        };
        if tab.is_new_tab() {
            return area.child(self.render_new_tab_page(theme, cx));
        }
        if let Some(error) = tab.error.clone() {
            return area.child(self.render_error_page(error, theme, window, cx));
        }
        let page_background = if theme.dark {
            rgba(0x282828ff)
        } else {
            rgba(0xffffffff)
        };
        let host = tab.view.clone().map(|view| {
            let holes = self.holes.clone();
            let occluded = self.occluded;
            let page_frame = self.page_frame.clone();
            canvas(
                |_, _, _| {},
                move |bounds, _, _, _| {
                    // Inside the card's hairline on the left, right and
                    // bottom, so the border stays visible around the page.
                    let frame = WebViewFrame {
                        x: f32::from(bounds.origin.x) + 1.,
                        y: f32::from(bounds.origin.y),
                        width: f32::from(bounds.size.width) - 2.,
                        height: f32::from(bounds.size.height) - 1.,
                        bottom_corner_radius: PAGE_CORNER_RADIUS,
                        pointer_inset_left: RESIZE_HANDLE_OVERLAP,
                    };
                    page_frame.set(Some(frame));
                    view.set_frame(frame);
                    view.set_holes(&holes.borrow());
                    view.set_visible(!occluded);
                },
            )
            .absolute()
            .inset_0()
        });
        area.bg(page_background)
            .track_focus(&self.page_focus)
            .children(host)
            .children(self.render_zoom_banner(theme, cx))
    }

    fn render_zoom_banner(
        &self,
        theme: BrowserTheme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.zoom_banner?;
        let zoom = self.active_tab()?.zoom;
        let percent = (zoom * 100.).round() as i64;
        let control = |id: &'static str, glyph: &'static str, label: String| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(move |button| button.bg(theme.ghost_hover))
                .child(icon(glyph, theme.text.into()).size(px(16.)))
        };
        Some(
            div()
                .absolute()
                .top_0()
                .right_0()
                .px(px(12.))
                .pt(px(12.))
                .child(
                    div()
                        .id("browser-zoom-banner")
                        .relative()
                        .h(px(44.))
                        .pl(px(16.))
                        .pr(px(8.))
                        .rounded(px(20.))
                        .bg(theme.menu)
                        .shadow(theme.menu_shadow())
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .text_color(theme.text)
                        .on_hover(cx.listener(|panel, hovered: &bool, _, cx| {
                            panel.zoom_banner_hovered = *hovered;
                            if !*hovered {
                                panel.show_zoom_banner(cx);
                            }
                        }))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(div().min_w(px(44.)).child(format!("{percent}%")))
                        .child(
                            div()
                                .ml(px(12.))
                                .flex()
                                .items_center()
                                .overflow_hidden()
                                .rounded(px(10.))
                                .bg(theme.text.alpha(0.05))
                                .child(
                                    control(
                                        "browser-zoom-out",
                                        "browser-zoom-out",
                                        crate::i18n::format!("缩小" => "Zoom out"),
                                    )
                                    .on_click(
                                        cx.listener(|panel, _, _, cx| panel.step_zoom(-1, cx)),
                                    ),
                                )
                                .child(div().w(px(1.)).h(px(16.)).bg(theme.border))
                                .child(
                                    control(
                                        "browser-zoom-in",
                                        "browser-zoom-in",
                                        crate::i18n::format!("放大" => "Zoom in"),
                                    )
                                    .on_click(
                                        cx.listener(|panel, _, _, cx| panel.step_zoom(1, cx)),
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .id("browser-zoom-reset")
                                .role(Role::Button)
                                .ml(px(8.))
                                .h(px(28.))
                                .px(px(8.))
                                .rounded(px(12.5))
                                .flex()
                                .items_center()
                                .when(percent == 100, |button| button.opacity(0.4))
                                .when(percent != 100, |button| {
                                    button
                                        .cursor_pointer()
                                        .hover(move |button| button.bg(theme.ghost_hover))
                                        .on_click(
                                            cx.listener(|panel, _, _, cx| panel.step_zoom(0, cx)),
                                        )
                                })
                                .child(crate::i18n::format!("重置" => "Reset")),
                        )
                        .child(self.overlay_hole(20.)),
                )
                .into_any_element(),
        )
    }

    /// Chromium's `neterror` interstitial from the reference, at its CSS:
    /// 14px text on a 22.4px line, `max-width: 600px` from 20vh down, and its
    /// narrow-width rules below 700px and 420px.
    fn render_error_page(
        &self,
        error: TabError,
        theme: BrowserTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let page = error.page;
        let dark = theme.dark;
        let (background, text, heading, link, button_fill, button_hover, button_text) = if dark {
            (
                rgba(0x202124ff),
                rgba(0x9aa0a6ff),
                rgba(0x9aa0a6ff),
                rgba(0x8ab4f8ff),
                rgba(0xffffff0d),
                rgba(0xffffff1a),
                rgba(0xffffffff),
            )
        } else {
            (
                rgba(0xffffffff),
                rgba(0x5f6368ff),
                rgba(0x202124ff),
                rgba(0x585858ff),
                rgba(0x1a1c1f0d),
                rgba(0x1a1c1f1a),
                rgba(0x1a1c1fff),
            )
        };
        let detail_body = if dark { text } else { rgba(0x777777ff) };
        let width = self.panel_width.get();
        let height = f32::from(window.viewport_size().height);
        let narrow = width <= 420.;
        let medium = !narrow && width <= 700.;
        let top = if narrow { height * 0.07 } else { height * 0.2 };
        let horizontal = if narrow {
            24.
        } else if medium {
            width * 0.1
        } else {
            0.
        };
        let button = |id: &'static str, label: String| {
            div()
                .id(id)
                .role(Role::Button)
                .min_h(px(28.))
                .px(px(8.))
                .rounded(px(8.))
                .border_1()
                .border_color(gpui::transparent_black())
                .bg(button_fill)
                .text_color(button_text)
                .text_size(px(13.))
                .line_height(px(18.))
                .font_weight(FontWeight::MEDIUM)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(move |button| button.bg(button_hover))
                .child(label)
        };
        let url = error.url.clone();
        let mut buttons = div()
            .mt(px(if narrow { 30. } else { 51. }))
            .flex()
            .gap(px(8.))
            .when(page.actions_at_end, |row| row.justify_end());
        if let Some(label) = page.open_external.clone() {
            buttons = buttons.child(
                button("browser-error-open-external", label)
                    .on_click(cx.listener(move |panel, _, _, cx| panel.open_external(&url, cx))),
            );
        }
        buttons = buttons.child(
            button("browser-error-reload", page.reload.clone())
                .on_click(cx.listener(|panel, _, _, cx| panel.reload(cx))),
        );
        let details_open = error.details_open;
        let suggestions = page.try_label.clone().map(|try_label| {
            let mut list = div().flex().flex_col();
            for suggestion in &page.suggestions {
                list = list.child(bullet(suggestion.clone(), None, text));
            }
            if let Some(details) = page.details_link.clone() {
                list = list.child(bullet(
                    details,
                    Some(
                        div()
                            .id("browser-error-details-link")
                            .role(Role::Link)
                            .aria_expanded(details_open)
                            .text_color(link)
                            .cursor_pointer()
                            .on_click(cx.listener(|panel, _, _, cx| {
                                if let Some(tab) = panel.tabs.get_mut(panel.active)
                                    && let Some(error) = &mut tab.error
                                {
                                    error.details_open = !error.details_open;
                                    cx.notify();
                                }
                            })),
                    ),
                    text,
                ));
            }
            div()
                .mt(px(18.))
                .child(div().mt(px(14.)).child(try_label))
                .child(list)
        });
        div()
            .id("browser-error-page")
            .size_full()
            .overflow_y_scroll()
            .bg(background)
            .text_color(text)
            .text_size(px(14.))
            .line_height(px(22.4))
            .child(
                div()
                    .mx_auto()
                    .mt(px(top))
                    .mb(px(if narrow { 12. } else { 0. }))
                    .px(px(horizontal))
                    .w_full()
                    .max_w(px(600. + 2. * horizontal))
                    .child(
                        img(gpui::ImageSource::Resource(gpui::Resource::Embedded(
                            "icons/browser-error.png".into(),
                        )))
                        .w(px(26.))
                        .h(px(28.))
                        .mb(px(24.)),
                    )
                    .child(
                        div()
                            .text_color(heading)
                            .text_size(px(if narrow { 21. } else { 16.8 }))
                            .line_height(px(if narrow { 25.2 } else { 20.16 }))
                            .font_weight(FontWeight::MEDIUM)
                            .mb(px(if narrow { 8. } else { 12. }))
                            .child(page.heading.clone()),
                    )
                    .child(div().child(page.summary.clone()))
                    .children(suggestions)
                    .children(page.error_code.clone().map(|code| {
                        div()
                            .mt(px(24.))
                            .text_size(px(11.2))
                            .line_height(px(17.92))
                            .child(code.to_uppercase())
                    }))
                    .child(buttons)
                    .when(details_open, |content| {
                        let mut details = div()
                            .mt(px(if narrow { 20. } else { 0. }))
                            .mb(px(if narrow { 20. } else { 50. }));
                        for (header, body) in &page.details {
                            details = details.child(
                                div()
                                    .mt(px(18.))
                                    .child(
                                        div()
                                            .mb(px(4.))
                                            .font_weight(FontWeight::BOLD)
                                            .child(header.clone()),
                                    )
                                    .child(div().text_color(detail_body).child(body.clone())),
                            );
                        }
                        content.child(details)
                    }),
            )
    }
}

/// A `<li>` of the error page's "Try:" list: a disc in the list's 40px
/// indent, then the text (or the details link).
fn bullet(text: String, link: Option<Stateful<Div>>, color: gpui::Rgba) -> Div {
    let content: AnyElement = match link {
        Some(link) => link.child(text).into_any_element(),
        None => div().child(text).into_any_element(),
    };
    div()
        .pl(px(40.))
        .relative()
        .child(
            div()
                .absolute()
                .left(px(24.))
                .top(px(9.))
                .size(px(5.))
                .rounded_full()
                .bg(color),
        )
        .child(content)
}
