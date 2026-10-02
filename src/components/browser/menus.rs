//! Menus the panel opens over the page: Browser options (with its Clear
//! browsing data flyout), a tab's context menu, the Downloads popover, and
//! the find-in-page bar. Each cuts a hole into the page it covers.

use gpui::{
    AnyElement, Context, Div, FontWeight, MouseButton, Role, Stateful, div, point, prelude::*, px,
};

use super::{BrowserPanel, BrowserTheme, DownloadState, FIND_CONTEXT, FindEscape, PanelMenu};
use crate::{
    browser::{address, webview::ClearData},
    components::icons::icon,
};

/// `w-[240px]`, `p-1`, 20px radius.
const MENU_WIDTH: f32 = 240.0;
const MENU_RADIUS: f32 = 20.0;
/// The Downloads popover.
const DOWNLOADS_WIDTH: f32 = 320.0;

impl BrowserPanel {
    pub(super) fn render_menu(
        &self,
        theme: BrowserTheme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.menu?;
        let (content, anchor) = match menu {
            PanelMenu::Options | PanelMenu::ClearData => {
                let bounds = self.anchor_bounds("options")?;
                (
                    div().child(self.options_menu(theme, cx)),
                    (
                        bounds.bottom_right() + point(px(-1.), px(2.)),
                        gpui::Anchor::TopRight,
                    ),
                )
            }
            PanelMenu::Downloads => {
                let bounds = self.anchor_bounds("downloads")?;
                (
                    div().child(self.downloads_popover(theme, cx)),
                    (
                        bounds.bottom_right() + point(px(0.), px(4.)),
                        gpui::Anchor::TopRight,
                    ),
                )
            }
            PanelMenu::Tab(id) => {
                let position = self.menu_anchor?;
                (
                    div().child(self.tab_menu(id, theme, cx)?),
                    (position, gpui::Anchor::TopLeft),
                )
            }
        };
        let (position, corner) = anchor;
        // The Clear browsing data flyout opens beside its row, towards the
        // window: the options menu sits at the panel's right edge.
        let flyout = (menu == PanelMenu::ClearData)
            .then(|| self.anchor_bounds("clear-data-row"))
            .flatten()
            .map(|row| {
                gpui::deferred(
                    gpui::anchored()
                        .anchor(gpui::Anchor::TopRight)
                        .position(row.origin + point(px(-8.), px(-4.)))
                        .snap_to_window_with_margin(px(8.))
                        .child(self.clear_data_menu(theme, cx)),
                )
                .with_priority(3)
            });
        Some(
            div()
                .child(
                    gpui::deferred(
                        gpui::anchored()
                            .anchor(corner)
                            .position(position)
                            .snap_to_window_with_margin(px(8.))
                            .child(
                                div()
                                    .id("browser-menu-layer")
                                    .on_mouse_down_out(cx.listener(
                                        |panel, event: &gpui::MouseDownEvent, _, cx| {
                                            // A press in the flyout is still inside the menu.
                                            let in_flyout =
                                                panel.anchor_bounds("clear-data-menu").is_some_and(
                                                    |bounds| bounds.contains(&event.position),
                                                );
                                            if !in_flyout {
                                                panel.menu = None;
                                                cx.notify();
                                            }
                                        },
                                    ))
                                    .child(content),
                            ),
                    )
                    .with_priority(2),
                )
                .children(flyout)
                .into_any_element(),
        )
    }

    fn menu_surface(&self, id: &'static str, width: f32, theme: BrowserTheme) -> Stateful<Div> {
        div()
            .id(id)
            .role(Role::Menu)
            .relative()
            .w(px(width))
            .p(px(4.))
            .rounded(px(MENU_RADIUS))
            .bg(theme.menu)
            .shadow(theme.menu_shadow())
            .flex()
            .flex_col()
            .text_size(px(13.))
            .line_height(px(18.5714))
            .text_color(theme.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(self.overlay_hole(MENU_RADIUS))
    }

    fn options_menu(&self, theme: BrowserTheme, cx: &mut Context<Self>) -> Stateful<Div> {
        let page = self.showing_page();
        let zoom = self.active_tab().map(|tab| tab.zoom).unwrap_or(1.);
        let percent = (zoom * 100.).round() as i64;
        let zoom_button = |id: &'static str, glyph: &'static str, label: String, enabled: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .when(!enabled, |button| button.opacity(0.4))
                .when(enabled, |button| {
                    button
                        .cursor_pointer()
                        .hover(move |button| button.bg(theme.ghost_hover))
                })
                .child(icon(glyph, theme.text_tertiary.into()).size(px(16.)))
        };
        let zoom_row = div()
            .id("browser-menu-zoom")
            .role(Role::Group)
            .aria_label(crate::i18n::format!("缩放" => "Zoom"))
            .px(px(8.))
            .py(px(2.))
            .flex()
            .items_center()
            .gap(px(4.))
            .when(!page, |row| row.opacity(0.5))
            .child(
                div()
                    .min_w(px(0.))
                    .flex_1()
                    .truncate()
                    .child(crate::i18n::format!("缩放" => "Zoom")),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.text.alpha(0.05))
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .child(
                        zoom_button(
                            "browser-menu-zoom-out",
                            "browser-zoom-out",
                            crate::i18n::format!("缩小" => "Zoom out"),
                            page,
                        )
                        .when(page, |button| {
                            button.on_click(cx.listener(|panel, _, _, cx| panel.step_zoom(-1, cx)))
                        }),
                    )
                    .child(
                        div()
                            .w(px(44.))
                            .py(px(2.))
                            .border_l_1()
                            .border_r_1()
                            .border_color(theme.border)
                            .text_center()
                            .child(format!("{percent}%")),
                    )
                    .child(
                        zoom_button(
                            "browser-menu-zoom-in",
                            "browser-zoom-in",
                            crate::i18n::format!("放大" => "Zoom in"),
                            page,
                        )
                        .when(page, |button| {
                            button.on_click(cx.listener(|panel, _, _, cx| panel.step_zoom(1, cx)))
                        }),
                    ),
            )
            .child(
                zoom_button(
                    "browser-menu-zoom-reset",
                    "browser-reload",
                    crate::i18n::format!("重置" => "Reset"),
                    page && percent != 100,
                )
                .rounded(px(10.))
                .when(page && percent != 100, |button| {
                    button.on_click(cx.listener(|panel, _, _, cx| panel.step_zoom(0, cx)))
                }),
            );
        let clear_open = self.menu == Some(PanelMenu::ClearData);
        self.menu_surface("browser-options-menu", MENU_WIDTH, theme)
            .child(
                menu_item(
                    "browser-menu-find",
                    crate::i18n::format!("在页面中查找" => "Find in page"),
                    !page,
                    theme,
                )
                .when(page, |item| {
                    item.on_click(cx.listener(|panel, _, window, cx| panel.open_find(window, cx)))
                }),
            )
            .child(
                menu_item(
                    "browser-menu-print",
                    crate::i18n::format!("打印" => "Print"),
                    !page,
                    theme,
                )
                .when(page, |item| {
                    item.on_click(cx.listener(|panel, _, _, cx| {
                        panel.menu = None;
                        if let Some(view) = panel.active_view() {
                            view.print();
                        }
                        cx.notify();
                    }))
                }),
            )
            .child(separator(theme))
            .child(zoom_row)
            .child(separator(theme))
            .child(
                menu_item(
                    "browser-menu-screenshot",
                    crate::i18n::format!("截取屏幕截图" => "Take a screenshot"),
                    !page,
                    theme,
                )
                .when(page, |item| {
                    item.on_click(cx.listener(|panel, _, _, cx| {
                        panel.menu = None;
                        panel.copy_screenshot();
                        cx.notify();
                    }))
                }),
            )
            .child(separator(theme))
            .child(
                menu_item(
                    "browser-menu-downloads",
                    crate::i18n::format!("下载" => "Downloads"),
                    false,
                    theme,
                )
                .on_click(cx.listener(|panel, _, _, cx| {
                    panel.menu = Some(PanelMenu::Downloads);
                    cx.notify();
                })),
            )
            .child(
                menu_item(
                    "browser-menu-clear",
                    crate::i18n::format!("清除浏览数据" => "Clear browsing data"),
                    false,
                    theme,
                )
                .aria_expanded(clear_open)
                .when(clear_open, |item| item.bg(theme.ghost_hover))
                .on_hover(cx.listener(|panel, hovered: &bool, _, cx| {
                    if *hovered && panel.menu == Some(PanelMenu::Options) {
                        panel.menu = Some(PanelMenu::ClearData);
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(|panel, _, _, cx| {
                    panel.menu = Some(PanelMenu::ClearData);
                    cx.notify();
                }))
                .relative()
                .child(
                    icon("browser-chevron-right", theme.text_tertiary.into())
                        .size(px(12.))
                        .opacity(0.75),
                )
                .child(self.anchor("clear-data-row")),
            )
    }

    fn clear_data_menu(&self, theme: BrowserTheme, cx: &mut Context<Self>) -> Stateful<Div> {
        self.menu_surface("browser-clear-data-menu", MENU_WIDTH, theme)
            .child(self.anchor("clear-data-menu"))
            .child(
                menu_item(
                    "browser-clear-cookies",
                    crate::i18n::format!("清除 Cookie" => "Clear cookies"),
                    false,
                    theme,
                )
                .on_click(cx.listener(|panel, _, _, cx| panel.clear_data(ClearData::Cookies, cx))),
            )
            .child(
                menu_item(
                    "browser-clear-cache",
                    crate::i18n::format!("清除缓存" => "Clear cache"),
                    false,
                    theme,
                )
                .on_click(cx.listener(|panel, _, _, cx| panel.clear_data(ClearData::Cache, cx))),
            )
            .child(
                menu_item(
                    "browser-clear-downloads",
                    crate::i18n::format!("删除下载历史记录" => "Delete download history"),
                    false,
                    theme,
                )
                .on_click(cx.listener(|panel, _, _, cx| panel.clear_download_history(cx))),
            )
    }

    /// `Oti`: New tab to the right, Reload, Duplicate, Copy URL, Open in
    /// external browser, then Rename.
    fn tab_menu(
        &self,
        id: u64,
        theme: BrowserTheme,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let index = self.tab_index(id)?;
        let url = self.tabs[index].url.clone();
        let web = address::is_web_url(&url);
        let mut menu = self
            .menu_surface("browser-tab-menu", MENU_WIDTH, theme)
            .child(
                menu_item(
                    "browser-tab-menu-new-right",
                    crate::i18n::format!("在右侧打开新标签页" => "New tab to the right"),
                    false,
                    theme,
                )
                .on_click(cx.listener(move |panel, _, _, cx| {
                    if let Some(index) = panel.tab_index(id) {
                        panel.open_tab_after(index, None, cx);
                    }
                })),
            )
            .child(
                menu_item(
                    "browser-tab-menu-reload",
                    crate::i18n::format!("重新加载" => "Reload"),
                    false,
                    theme,
                )
                .on_click(cx.listener(move |panel, _, _, cx| {
                    if let Some(index) = panel.tab_index(id) {
                        panel.select_tab(index, cx);
                        panel.reload(cx);
                    }
                    panel.menu = None;
                })),
            )
            .child(
                menu_item(
                    "browser-tab-menu-duplicate",
                    crate::i18n::format!("复制标签页" => "Duplicate"),
                    false,
                    theme,
                )
                .on_click(cx.listener(move |panel, _, _, cx| {
                    if let Some(index) = panel.tab_index(id) {
                        panel.duplicate_tab(index, cx);
                    }
                })),
            );
        if web {
            let copy = url.clone();
            let external = url.clone();
            menu = menu
                .child(
                    menu_item(
                        "browser-tab-menu-copy-url",
                        crate::i18n::format!("复制 URL" => "Copy URL"),
                        false,
                        theme,
                    )
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        panel.menu = None;
                        panel.copy_url(copy.clone(), cx);
                        cx.notify();
                    })),
                )
                .child(
                    menu_item(
                        "browser-tab-menu-external",
                        crate::i18n::format!("在外部浏览器中打开" => "Open in external browser"),
                        false,
                        theme,
                    )
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        panel.menu = None;
                        panel.open_external(&external, cx);
                        cx.notify();
                    })),
                );
        }
        Some(
            menu.child(separator(theme)).child(
                menu_item(
                    "browser-tab-menu-rename",
                    crate::i18n::format!("重命名" => "Rename"),
                    false,
                    theme,
                )
                .on_click(cx.listener(move |panel, _, _, cx| panel.rename_tab(id, cx))),
            ),
        )
    }

    fn downloads_popover(&self, theme: BrowserTheme, cx: &mut Context<Self>) -> Stateful<Div> {
        let downloads = self.store.read(cx).downloads().to_vec();
        let mut popover = self
            .menu_surface("browser-downloads-popover", DOWNLOADS_WIDTH, theme)
            .child(
                div()
                    .px(px(8.))
                    .pt(px(4.))
                    .pb(px(4.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(crate::i18n::format!("下载" => "Downloads")),
            );
        if downloads.is_empty() {
            return popover.child(
                div()
                    .px(px(8.))
                    .py(px(12.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(4.))
                    .text_center()
                    .child(crate::i18n::format!("暂无下载内容" => "No downloads yet"))
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .text_color(theme.text_tertiary)
                            .child(crate::i18n::format!(
                                "从内置浏览器下载的文件将显示在这里" =>
                                "Files downloaded from the built-in browser will appear here"
                            )),
                    ),
            );
        }
        for download in downloads {
            let status = match &download.state {
                DownloadState::InProgress(fraction) if *fraction > 0. => {
                    format!("{}%", (fraction * 100.).round() as i64)
                }
                DownloadState::InProgress(_) => crate::i18n::format!("正在开始" => "Starting"),
                DownloadState::Finished => crate::i18n::format!("已下载" => "Downloaded"),
                DownloadState::Failed(_) => crate::i18n::format!("下载未成功" => "Download failed"),
            };
            let finished = download.state == DownloadState::Finished;
            let open_path = download.path.clone();
            let reveal_path = download.path.clone();
            let id = download.id;
            popover = popover.child(
                div()
                    .id(("browser-download", id))
                    .role(Role::MenuItem)
                    .aria_label(crate::i18n::format!("打开 {}" => "Open {}", download.filename))
                    .min_h(px(44.))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(15.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(finished, |row| {
                        row.cursor_pointer()
                            .hover(move |row| row.bg(theme.ghost_hover))
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                if let Some(path) = &open_path {
                                    cx.open_with_system(path);
                                }
                                panel.menu = None;
                                cx.notify();
                            }))
                    })
                    .child(
                        icon("browser-downloads", theme.text_secondary.into())
                            .size(px(16.))
                            .flex_none(),
                    )
                    .child(
                        div()
                            .min_w(px(0.))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(div().truncate().child(download.filename.clone()))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .text_color(theme.text_tertiary)
                                    .child(status),
                            ),
                    )
                    .when(finished, |row| {
                        row.child(
                            div()
                                .id(("browser-download-reveal", id))
                                .role(Role::Button)
                                .aria_label(
                                    crate::i18n::format!("在访达中显示" => "Show in Finder"),
                                )
                                .size(px(28.))
                                .flex_none()
                                .rounded(px(12.5))
                                .flex()
                                .items_center()
                                .justify_center()
                                .hover(move |button| button.bg(theme.ghost_hover))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(path) = &reveal_path {
                                        cx.reveal_path(path);
                                    }
                                }))
                                .child(
                                    icon("browser-tool-files", theme.text_tertiary.into())
                                        .size(px(16.)),
                                ),
                        )
                    })
                    .child(
                        div()
                            .id(("browser-download-remove", id))
                            .role(Role::Button)
                            .aria_label(crate::i18n::format!("从列表中移除" => "Remove from list"))
                            .size(px(28.))
                            .flex_none()
                            .rounded(px(12.5))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(move |button| button.bg(theme.ghost_hover))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                cx.stop_propagation();
                                panel
                                    .store
                                    .update(cx, |store, cx| store.remove_download(id, cx));
                            }))
                            .child(
                                icon("browser-dismiss", theme.text_tertiary.into()).size(px(16.)),
                            ),
                    ),
            );
        }
        popover
    }

    /// Echora's find bar (the reference's floating find panel), here for the
    /// page: 8px inside the card's top right, over the tab strip.
    pub(super) fn render_find_bar(
        &self,
        theme: BrowserTheme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.find.open {
            return None;
        }
        let input = self.find.input.clone()?;
        let result = self.active_tab().map(|tab| tab.find).unwrap_or_default();
        let expanded = !self.find.query.is_empty();
        let has_matches = result.matches > 0;
        let nav = |id: &'static str, label: String, backwards: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .size(px(16.))
                .rounded(px(10.))
                .flex()
                .items_center()
                .justify_center()
                .when(!has_matches, |button| button.opacity(0.4))
                .when(has_matches, |button| {
                    button
                        .cursor_pointer()
                        .hover(move |button| button.bg(theme.ghost_hover))
                        .on_click(cx.listener(move |panel, _, _, _| panel.find_step(backwards)))
                })
                .child(
                    icon("find-previous", theme.text_secondary.into())
                        .size(px(16.))
                        .when(!backwards, |arrow| {
                            arrow.with_transformation(gpui::Transformation::rotate(gpui::radians(
                                std::f32::consts::PI,
                            )))
                        }),
                )
        };
        let status = if has_matches {
            crate::i18n::format!(
                "{} 个结果，共 {} 个结果" => "{} / {} results",
                result.active,
                result.matches
            )
        } else {
            crate::i18n::format!("0 个结果" => "0 results")
        };
        Some(
            div()
                .id("browser-find")
                .role(Role::Search)
                .aria_label(crate::i18n::format!("在页面中查找" => "Find in page"))
                .key_context(FIND_CONTEXT)
                .on_action(cx.listener(|panel, _: &FindEscape, _, cx| panel.close_find(cx)))
                .on_key_down(cx.listener(|panel, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "enter" && event.keystroke.modifiers.shift {
                        panel.find_step(true);
                        cx.stop_propagation();
                    }
                }))
                .absolute()
                .top(px(8.))
                .right(px(8.))
                .w(px(340.))
                .max_w(gpui::relative(0.9))
                .rounded(px(20.))
                .overflow_hidden()
                .bg(theme.find)
                .border(px(0.5))
                .border_color(theme.border)
                .shadow(vec![
                    gpui::BoxShadow::new(px(0.), px(4.), gpui::rgba(0x0000001a).into())
                        .blur_radius(px(12.)),
                ])
                .text_color(theme.text)
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .h(px(45.))
                        .pl(px(16.))
                        .pr(px(14.))
                        .flex()
                        .items_center()
                        .when(expanded, |row| row.border_b_1().border_color(theme.border))
                        .child(icon("find-search", theme.text_secondary.into()).size(px(16.)))
                        .child(div().ml(px(-2.)).min_w(px(0.)).flex_1().child(input))
                        .child(div().w(px(1.)).h(px(16.)).bg(theme.border))
                        .child(
                            div()
                                .id("browser-find-close")
                                .ml(px(6.))
                                .role(Role::Button)
                                .aria_label(crate::i18n::format!("关闭查找" => "Close find"))
                                .size(px(24.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(move |button| button.bg(theme.ghost_hover))
                                .on_click(cx.listener(|panel, _, _, cx| panel.close_find(cx)))
                                .child(
                                    icon("close-dialog", theme.text_secondary.into()).size(px(16.)),
                                ),
                        ),
                )
                .when(expanded, |bar| {
                    bar.child(
                        div()
                            .h(px(35.))
                            .px(px(16.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .child(nav(
                                        "browser-find-previous",
                                        crate::i18n::format!("上一个结果" => "Previous result"),
                                        true,
                                    ))
                                    .child(nav(
                                        "browser-find-next",
                                        crate::i18n::format!("下一个结果" => "Next result"),
                                        false,
                                    )),
                            )
                            .child(
                                div()
                                    .id("browser-find-count")
                                    .role(Role::Status)
                                    .text_size(px(14.))
                                    .line_height(px(24.))
                                    .text_color(theme.text.alpha(0.5))
                                    .child(status),
                            ),
                    )
                })
                .child(self.overlay_hole(20.))
                .into_any_element(),
        )
    }
}

fn menu_item(
    id: &'static str,
    label: String,
    disabled: bool,
    theme: BrowserTheme,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::MenuItem)
        .aria_label(label.clone())
        .min_h(px(28.5714))
        .px(px(8.))
        .py(px(5.))
        .rounded(px(15.))
        .flex()
        .items_center()
        .gap(px(6.))
        .when(disabled, |item| item.opacity(0.5))
        .when(!disabled, |item| {
            item.cursor_pointer()
                .hover(move |item| item.bg(theme.ghost_hover))
        })
        .child(div().min_w(px(0.)).flex_1().truncate().child(label))
}

fn separator(theme: BrowserTheme) -> Div {
    div()
        .w_full()
        .flex_none()
        .px(px(8.))
        .py(px(4.))
        .child(div().h(px(1.)).w_full().bg(theme.border))
}
