//! The panel's tab strip, in the shape every Echora right-panel view uses
//! (156 px tabs on the card's 46 px top bar), with the reference's Browser
//! tab content: the page icon, its title and a close button, then "+".

use gpui::{Context, Div, MouseButton, MouseDownEvent, Role, Stateful, div, img, prelude::*, px};

use super::{BrowserPanel, BrowserTheme, PanelMenu};
use crate::components::icons::icon;

/// Room the card's top bar keeps for the titlebar's trailing controls.
const TRAILING_CONTROLS_WIDTH: f32 = 82.0;

impl BrowserPanel {
    pub(super) fn render_tab_strip(
        &self,
        theme: BrowserTheme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut tabs = div()
            .id("browser-tabs")
            .role(Role::TabList)
            .aria_label(crate::i18n::format!("浏览器标签页" => "Browser tabs"))
            .flex()
            .items_center()
            .min_w(px(0.))
            .overflow_x_scroll()
            .gap(px(4.));
        for (index, tab) in self.tabs.iter().enumerate() {
            let id = tab.id;
            let active = index == self.active;
            let title = tab.page_title();
            let renaming = self
                .rename
                .as_ref()
                .filter(|(renaming, _)| *renaming == id)
                .map(|(_, input)| input.clone());
            let close_label = crate::i18n::format!("关闭{}标签页" => "Close {} tab", title);
            let page_icon: gpui::AnyElement = match &tab.favicon {
                Some(favicon) => img(favicon.clone())
                    .size(px(16.))
                    .flex_none()
                    .into_any_element(),
                None => icon("panel-browser", theme.text.into())
                    .size(px(16.))
                    .flex_none()
                    .into_any_element(),
            };
            let mut row: Stateful<Div> = div()
                .id(("browser-tab", id))
                .group("browser-tab")
                .role(Role::Tab)
                .aria_label(title.clone())
                .aria_selected(active)
                .h(px(28.))
                .w(px(156.))
                .min_w(px(80.))
                .flex_none()
                .px(px(8.))
                .rounded(px(10.))
                .when(active, |row| row.bg(theme.text.alpha(0.05)))
                .hover(move |row| row.bg(theme.ghost_hover))
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .on_click(cx.listener(move |panel, _, _, cx| {
                    if let Some(index) = panel.tab_index(id) {
                        panel.select_tab(index, cx);
                    }
                }))
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(move |panel, _: &MouseDownEvent, _, cx| {
                        if let Some(index) = panel.tab_index(id) {
                            panel.close_tab(index, cx);
                        }
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |panel, event: &MouseDownEvent, _, cx| {
                        panel.menu = Some(PanelMenu::Tab(id));
                        panel.menu_anchor = Some(event.position);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(page_icon);
            row = match renaming {
                Some(input) => row.child(
                    div()
                        .id(("browser-tab-rename", id))
                        .min_w(px(0.))
                        .flex_1()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(input),
                ),
                None => row.child(
                    div()
                        .min_w(px(0.))
                        .flex_1()
                        .truncate()
                        .text_size(px(13.))
                        .line_height(px(18.5714))
                        .text_color(theme.text)
                        .child(title),
                ),
            };
            tabs = tabs.child(
                row.child(
                    div()
                        .id(("browser-tab-close", id))
                        .role(Role::Button)
                        .aria_label(close_label)
                        .size(px(20.))
                        .flex_none()
                        .rounded(px(5.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |button| button.bg(theme.ghost_hover))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |panel, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(index) = panel.tab_index(id) {
                                panel.close_tab(index, cx);
                            }
                        }))
                        .child(icon("close-dialog", theme.text_tertiary.into()).size(px(12.))),
                ),
            );
        }
        div()
            .id("browser-tab-strip")
            .h(px(46.))
            .w_full()
            .flex_none()
            .px(px(8.))
            .pr(px(TRAILING_CONTROLS_WIDTH))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(tabs.relative().child(self.anchor("tabs")))
            .child(
                div()
                    .id("browser-new-tab")
                    .role(Role::Button)
                    .aria_label(crate::i18n::format!("新标签页" => "New tab"))
                    .size(px(28.))
                    .flex_none()
                    .rounded(px(10.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |button| button.bg(theme.ghost_hover))
                    .on_click(cx.listener(|panel, _, _, cx| panel.new_tab(None, cx)))
                    .child(icon("browser-new-tab", theme.text_tertiary.into()).size(px(16.))),
            )
    }
}
