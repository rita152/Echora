//! The New tab page, the reference's panel launcher: "Tools" (Review,
//! Terminal, Side chat when the chat has one, Files) in one or two columns,
//! then "Suggested", the most visited sites, which can be dismissed.

use gpui::{Context, Div, FontWeight, MouseButton, Role, Stateful, div, img, prelude::*, px};

use super::{BrowserPanel, BrowserTheme, BrowserTool};
use crate::{browser::address, components::icons::icon};

/// `px-panel` and `py-8` around the page.
const PAGE_PADDING_X: f32 = 20.0;
const PAGE_PADDING_Y: f32 = 32.0;
/// `max-w-3xl`.
const PAGE_MAX_WIDTH: f32 = 768.0;
/// `@md`: two tool columns once the page's content is 28rem wide.
const TWO_COLUMNS_FROM: f32 = 448.0;

impl BrowserPanel {
    pub(super) fn render_new_tab_page(
        &self,
        theme: BrowserTheme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let content_width = (self.panel_width.get() - 2. * PAGE_PADDING_X).min(PAGE_MAX_WIDTH);
        let mut tools = vec![
            (
                BrowserTool::Review,
                crate::i18n::format!("审查" => "Review"),
                "browser-tool-review",
                "⌃⇧G",
            ),
            (
                BrowserTool::Terminal,
                crate::i18n::format!("终端" => "Terminal"),
                "browser-tool-terminal",
                "⌃`",
            ),
        ];
        if self.side_chat_available {
            tools.push((
                BrowserTool::SideChat,
                crate::i18n::format!("侧边聊天" => "Side chat"),
                "browser-tool-side-chat",
                "⌥⌘S",
            ));
        }
        tools.push((
            BrowserTool::Files,
            crate::i18n::format!("文件" => "Files"),
            "browser-tool-files",
            "⌘P",
        ));
        let two_columns = content_width >= TWO_COLUMNS_FROM;
        let mut grid = div()
            .id("browser-new-tab-tools")
            .grid()
            .grid_cols(if two_columns { 2 } else { 1 })
            .gap_x(px(16.))
            .gap_y(px(4.));
        for (tool, label, glyph, shortcut) in tools {
            grid = grid.child(tool_row(tool, label, glyph, shortcut, theme, cx));
        }
        let sites = self
            .store
            .read(cx)
            .history
            .top_sites(crate::browser::history::TOP_SITE_LIMIT);
        let suggested = (!sites.is_empty()).then(|| {
            let mut row = div()
                .id("browser-new-tab-sites")
                .flex()
                .gap(px(12.))
                .overflow_x_scroll();
            for site in sites {
                row = row.child(self.site_tile(site, theme, cx));
            }
            section(crate::i18n::format!("推荐" => "Suggested"), theme).child(row)
        });
        div()
            .id("browser-new-tab")
            .size_full()
            .overflow_y_scroll()
            .bg(theme.canvas)
            .px(px(PAGE_PADDING_X))
            .py(px(PAGE_PADDING_Y))
            .child(
                div()
                    .mx_auto()
                    .w_full()
                    .max_w(px(PAGE_MAX_WIDTH))
                    .flex()
                    .flex_col()
                    .gap(px(40.))
                    .child(section(crate::i18n::format!("工具" => "Tools"), theme).child(grid))
                    .children(suggested),
            )
    }

    fn site_tile(
        &self,
        site: crate::browser::history::HistoryEntry,
        theme: BrowserTheme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let hovered = self.hovered_tile.as_deref() == Some(site.url.as_str());
        let title = if site.title.trim().is_empty() {
            address::display_text(&site.url)
        } else {
            site.title.clone()
        };
        let favicon = address::host(&site.url).and_then(|host| self.store.read(cx).favicon(&host));
        let glyph: gpui::AnyElement = match favicon {
            Some(image) => img(image).size(px(32.)).into_any_element(),
            None => icon("browser-site-globe", theme.text_secondary.into())
                .size(px(32.))
                .into_any_element(),
        };
        let url = site.url.clone();
        let hover_url = site.url.clone();
        let dismiss_url = site.url.clone();
        let dismiss_label = crate::i18n::format!("忽略 {title}" => "Dismiss {title}");
        div()
            .id(gpui::ElementId::Name(
                format!("browser-site-{}", site.url).into(),
            ))
            .role(Role::Link)
            .aria_label(title.clone())
            .relative()
            .min_w(px(128.))
            .flex_1()
            .rounded(px(15.))
            .cursor_pointer()
            .on_hover(cx.listener(move |panel, hovered: &bool, _, cx| {
                let next = hovered.then(|| hover_url.clone());
                if *hovered || panel.hovered_tile.as_deref() == Some(hover_url.as_str()) {
                    panel.hovered_tile = next;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |panel, _, _, cx| panel.navigate(url.clone(), cx)))
            .child(
                div()
                    .min_w(px(0.))
                    .p(px(12.))
                    .rounded(px(15.))
                    .when(hovered, |tile| tile.bg(theme.ghost_hover))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(48.))
                            .flex_none()
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(glyph),
                    )
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_center()
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(title),
                    ),
            )
            .when(hovered, |tile| {
                tile.child(
                    div()
                        .id("browser-site-dismiss")
                        .role(Role::Button)
                        .aria_label(dismiss_label)
                        .absolute()
                        .top_0()
                        .right_0()
                        .size(px(28.))
                        .rounded(px(12.5))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |button| button.bg(theme.ghost_hover))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |panel, _, _, cx| {
                            cx.stop_propagation();
                            panel.dismiss_site(dismiss_url.clone(), cx);
                        }))
                        .child(icon("browser-dismiss", theme.text_tertiary.into()).size(px(16.))),
                )
            })
    }
}

/// A heading (`text-sm font-medium`) with its content 16px below.
fn section(title: String, theme: BrowserTheme) -> Div {
    div().flex().flex_col().gap(px(16.)).child(
        div()
            .text_size(px(13.))
            .line_height(px(18.5714))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.text)
            .child(title),
    )
}

/// `gs`: a 40px row on `bg-primary-soft-alpha` with the tool's icon, name
/// and shortcut chip.
fn tool_row(
    tool: BrowserTool,
    label: String,
    glyph: &'static str,
    shortcut: &'static str,
    theme: BrowserTheme,
    cx: &mut Context<BrowserPanel>,
) -> Stateful<Div> {
    div()
        .id(gpui::ElementId::Name(
            format!("browser-tool-{glyph}").into(),
        ))
        .role(Role::Button)
        .aria_label(label.clone())
        .min_h(px(40.))
        .w_full()
        .min_w(px(0.))
        .px(px(10.))
        .py(px(8.))
        .rounded(px(10.))
        .bg(theme.tool_row)
        .hover(move |row| row.bg(theme.ghost_hover))
        .flex()
        .items_center()
        .gap(px(8.))
        .cursor_pointer()
        .on_click(cx.listener(move |panel, _, _, cx| panel.open_tool(tool, cx)))
        .child(
            icon(glyph, theme.text_secondary.into())
                .size(px(16.))
                .flex_none(),
        )
        .child(
            div()
                .min_w(px(0.))
                .flex_1()
                .truncate()
                .text_size(px(13.))
                .line_height(px(18.5714))
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.text)
                .child(label),
        )
        .child(
            div()
                .flex_none()
                .ml(px(8.))
                .h(px(16.))
                .px(px(6.))
                .py(px(2.))
                .rounded(px(10.))
                .bg(theme.kbd)
                .text_size(px(12.))
                .line_height(px(12.))
                .text_color(theme.text_secondary)
                .flex()
                .items_center()
                .child(shortcut),
        )
}
