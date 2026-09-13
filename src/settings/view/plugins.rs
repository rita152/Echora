//! Plugins settings presentation. The plugins and apps segments keep the
//! reference catalog rendering; the MCP and skills segments are backed by the
//! coding-agent protocol through `plugins_mcp` and `plugins_skills`.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::PluginSegment;
use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

/// Reference catalog sizes for the two segments this client does not read from
/// a backend (plugins and apps). MCP and skills counts are live values.
// Keep the catalog badges in step with the current ChatGPT desktop reference
// (the management surfaces are compared at the same viewport in capture
// runs).  MCP and skills remain live counts from their respective directories.
const PLUGIN_CATALOG_COUNT: usize = 16;
const APP_CATALOG_COUNT: usize = 9;

impl SettingsView {
    pub(super) fn plugins_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let rows = page.sections[1].rows;
        let plugin_hover = match self.mode {
            ThemeMode::Dark => gpui::rgba(0x242424ff),
            ThemeMode::Light => gpui::rgba(0xe8e8e9ff),
        };
        let search_border = match self.mode {
            ThemeMode::Dark => gpui::rgba(0xffffff29),
            ThemeMode::Light => gpui::rgba(0x1a1c1f1a),
        };
        let subtitle_weight = crate::theme::UI_BODY_FONT_WEIGHT;
        // ChatGPT uses a 65%-alpha secondary foreground for the subtitle in
        // both themes, rather than a precomposited gray.
        let subtitle_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x1a1c1fa6),
            ThemeMode::Dark => gpui::rgba(0xffffffa6),
        };
        let icon_paths = match self.mode {
            ThemeMode::Light => [
                "icons/settings-plugin-gmail-light.svg",
                "icons/settings-plugin-github-light.svg",
                "icons/settings-plugin-figma-light.svg",
                "icons/settings-plugin-zotero-light.svg",
                "icons/settings-plugin-templates-light.svg",
                "icons/settings-plugin-management-light.svg",
                "icons/settings-plugin-documents-light.svg",
                "icons/settings-plugin-pdf-light.svg",
                "icons/settings-plugin-spreadsheets-light.svg",
                "icons/settings-plugin-presentations-light.svg",
            ],
            ThemeMode::Dark => [
                "icons/settings-plugin-gmail-dark.svg",
                "icons/settings-plugin-github-dark.svg",
                "icons/settings-plugin-figma-dark.svg",
                "icons/settings-plugin-zotero-dark.svg",
                "icons/settings-plugin-templates-dark.svg",
                "icons/settings-plugin-management-dark.svg",
                "icons/settings-plugin-documents-dark.svg",
                "icons/settings-plugin-pdf-dark.svg",
                "icons/settings-plugin-spreadsheets-dark.svg",
                "icons/settings-plugin-presentations-dark.svg",
            ],
        };
        let mut list = div().mt(px(36.0)).flex().flex_col();
        for (index, row) in rows.iter().enumerate() {
            list = list.child(
                div()
                    .h(px(68.0625))
                    .pl(px(9.0))
                    .pr(px(8.0))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .rounded(px(10.0))
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .items_start()
                            .when((6..10).contains(&index), |icon| {
                                icon.relative().top(px(1.0))
                            })
                            .when(index < 10, |icon| {
                                icon.child(gpui::img(icon_paths[index]).size(px(40.0)))
                            })
                            .when(index >= 10, |icon| {
                                icon.border_1()
                                    .border_color(theme.border)
                                    .child(
                                        gpui::img("icons/settings-plugin-broken.svg")
                                            .size(px(16.0))
                                            .flex_none(),
                                    )
                                    .child(
                                        div()
                                            .h(px(20.0))
                                            .flex_none()
                                            .text_size(px(14.0))
                                            .line_height(px(20.0))
                                            .font_weight(gpui::FontWeight::NORMAL)
                                            .child(row.title),
                                    )
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .relative()
                            .top(px(1.0))
                            .flex()
                            .flex_col()
                            .font_family(".SystemUIFont")
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(row.title),
                            )
                            .child(
                                div()
                                    .relative()
                                    .top(px(-1.0))
                                    .text_size(px(13.0))
                                    .line_height(px(21.125))
                                    .font_weight(subtitle_weight)
                                    .text_color(subtitle_color)
                                    .child(row.subtitle),
                            ),
                    )
                    .child(self.switch_control(true, (page.slug, 1, index), theme, cx)),
            );
        }
        let body = match self.plugins_segment {
            PluginSegment::Plugins | PluginSegment::Apps => list.into_any_element(),
            PluginSegment::Mcp => self.mcp_segment_content(&theme, cx),
            PluginSegment::Skills => self.skills_segment_content(&theme, cx),
        };
        let segment = self.plugins_segment;
        let counts = self.plugins_segment_counts();
        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(64.0))
            .pb(px(80.0))
            .child(
                div()
                    .relative()
                    .top(px(2.0))
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .relative()
                                    .top(px(-1.0))
                                    .text_size(px(24.0))
                                    .line_height(px(31.0))
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .child(page.label),
                            )
                            .child(
                                div()
                                    .relative()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .text_color(theme.text_tertiary)
                                    .child(page.sections[0].title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(74.0))
                                    .h(px(28.0))
                                    .rounded(px(12.5))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.settings_button)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(14.0))
                                    .line_height(px(18.0))
                                    .child("浏览目录"),
                            )
                            .child(
                                div()
                                    .w(px(66.0))
                                    .h(px(28.0))
                                    .px(px(8.0))
                                    .rounded(px(12.5))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.text)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(4.0))
                                    .text_size(px(14.0))
                                    .line_height(px(18.0))
                                    .text_color(theme.surface)
                                    .child("添加")
                                    .child(
                                        svg()
                                            .path("icons/chevron-down.svg")
                                            .size(px(12.0))
                                            .text_color(theme.surface),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .relative()
                    .mt(px(33.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .children((0..4).map(|index| {
                                let (label, count, target) = counts[index];
                                let selected = segment == target;
                                div()
                                    .id(("plugins-segment", index))
                                    .h(px(28.0))
                                    .pl(px(9.0))
                                    .pr(px(7.0))
                                    .rounded(px(12.5))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .when(selected, |chip| {
                                        chip.bg(plugin_hover).text_color(theme.text)
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.select_plugins_segment(target, cx)
                                    }))
                                    .child(format!("{label} {count}"))
                            })),
                    )
                    .child(
                        div()
                            .w(px(224.0))
                            .h(px(32.0))
                            .px(px(10.0))
                            .rounded_full()
                            .border_1()
                            .border_color(search_border)
                            .when(self.mode == ThemeMode::Dark, |search| {
                                search.bg(gpui::rgba(0x2d2d2dff))
                            })
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .child(
                                svg()
                                    .path("icons/search.svg")
                                    .size(px(15.0))
                                    .text_color(theme.text_tertiary),
                            )
                            .child(self.plugins_segment_search_label()),
                    ),
            )
            .child(body)
            .into_any_element()
    }

    /// Segment labels, counts and targets in the reference order. Plugin and
    /// app counts come from the reference catalog; MCP and skills report what
    /// the backend actually returned.
    fn plugins_segment_counts(&self) -> [(&'static str, usize, PluginSegment); 4] {
        [
            ("插件", PLUGIN_CATALOG_COUNT, PluginSegment::Plugins),
            ("应用", APP_CATALOG_COUNT, PluginSegment::Apps),
            // Plugin-provided servers are shown in their own section and are
            // not included in ChatGPT's MCP badge.  The live directory keeps
            // both kinds of entries so the UI can preserve that grouping.
            (
                "MCP",
                self.mcp
                    .directory
                    .servers
                    .values()
                    .filter(|server| server.plugin_id.is_none())
                    .count(),
                PluginSegment::Mcp,
            ),
            ("技能", self.skills.skill_count(), PluginSegment::Skills),
        ]
    }

    fn plugins_segment_search_label(&self) -> &'static str {
        match self.plugins_segment {
            PluginSegment::Plugins => "搜索插件",
            PluginSegment::Apps => "搜索应用",
            PluginSegment::Mcp => "搜索 MCP 服务器",
            PluginSegment::Skills => "搜索技能",
        }
    }
}
