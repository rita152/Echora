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

impl SettingsView {
    pub(super) fn plugins_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let plugin_hover = match self.mode {
            ThemeMode::Dark => gpui::rgba(0x242424ff),
            ThemeMode::Light => gpui::rgba(0xe8e8e9ff),
        };
        let search_border = match self.mode {
            ThemeMode::Dark => gpui::rgba(0xffffff29),
            ThemeMode::Light => gpui::rgba(0x1a1c1f1a),
        };
        let body = self.catalog_body(self.plugins_segment, &theme, cx);
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
                                    .child(crate::i18n::text(page.label)),
                            )
                            .child(
                                div()
                                    .relative()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .text_color(theme.text_tertiary)
                                    .child(crate::i18n::text(page.sections[0].title)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id("plugins-browse-directory")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_plugins(true, cx);
                                    }))
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
                                    .child(crate::i18n::text("浏览目录")),
                            )
                            .child(
                                div()
                                    .id("plugins-add-marketplace")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_marketplace_add(cx);
                                    }))
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
                                    .child(crate::i18n::text("添加"))
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
                            .child(self.plugin_search_input.clone()),
                    ),
            )
            .child(body)
            .into_any_element()
    }

    /// Segment labels, counts and targets in the reference order. Plugin and
    /// app counts come from the reference catalog; MCP and skills report what
    /// the backend actually returned.
    /// Segment labels, counts and targets. Every count is a live value: the
    /// plugin and app badges report what the directories actually returned.
    fn plugins_segment_counts(&self) -> [(&'static str, usize, PluginSegment); 4] {
        [
            (
                crate::i18n::text("插件"),
                self.live_plugin_count(),
                PluginSegment::Plugins,
            ),
            (
                crate::i18n::text("应用"),
                self.live_app_count(),
                PluginSegment::Apps,
            ),
            // Plugin-provided servers are shown in their own section and are
            // not included in the MCP badge.  The live directory keeps
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
            (
                crate::i18n::text("技能"),
                self.skills.skill_count(),
                PluginSegment::Skills,
            ),
        ]
    }
}
