//! The composer-width add picker shares the reference's 320px suggestion panel.
use super::{ComposerView, context_catalog::ContextAction};
use crate::{
    components::icons::icon,
    theme::{Theme, ThemeMode},
};
use gpui::{AnyElement, Context, MouseMoveEvent, Role, div, prelude::*, px, rgba};
impl ComposerView {
    pub(super) fn render_context_menu(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.mode == ThemeMode::Dark;
        let surface = if dark {
            rgba(0x2d2d2dff)
        } else {
            rgba(0xffffffff)
        };
        let highlight = theme.text.alpha(if dark { 0.08 } else { 0.05 });
        let rows = self.context_rows(cx);
        let mut children = Vec::new();
        let mut group = None;
        for (index, row) in rows.iter().enumerate() {
            if group != Some(row.group) {
                group = Some(row.group);
                children.push(
                    div()
                        .px(px(8.))
                        .py(px(4.))
                        .text_size(px(crate::typography::ui_size(13.)))
                        .line_height(px(18.57))
                        .text_color(theme.text_tertiary)
                        .child(row.group)
                        .into_any_element(),
                );
            }
            let selected = index
                == self
                    .context_catalog
                    .highlighted
                    .min(rows.len().saturating_sub(1));
            let check = matches!(
                row.action,
                ContextAction::Command(super::SlashCommand::PlanMode)
            ) && self.prompt_context.plan_mode == Some(true);
            let artwork = row.artwork.clone();
            let glyph = row.icon;
            children.push(
                div()
                    .id(("context-row", index))
                    .role(Role::MenuItem)
                    .aria_label(row.title.clone())
                    .w_full()
                    .h(px(28.57))
                    .flex_none()
                    .px(px(8.))
                    .py(px(5.))
                    .rounded(px(15.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(crate::typography::ui_size(13.)))
                    .line_height(px(18.57))
                    .text_color(theme.text)
                    .opacity(if selected { 1. } else { 0.75 })
                    .when(selected, |d| d.bg(highlight))
                    .cursor_pointer()
                    .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                        if this.context_catalog.highlighted != index {
                            this.context_catalog.highlighted = index;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_context_row(index, cx);
                        cx.stop_propagation();
                    }))
                    .child(if let Some(path) = artwork {
                        crate::components::icons::package_icon(path, theme.text.into())
                    } else {
                        icon(glyph, theme.text.into())
                            .size(px(16.))
                            .flex_none()
                            .into_any_element()
                    })
                    .child(
                        div()
                            .flex_none()
                            .max_w(gpui::relative(if row.description.is_empty() {
                                0.9
                            } else {
                                0.6
                            }))
                            .truncate()
                            .child(row.title.clone()),
                    )
                    .when(!row.description.is_empty(), |d| {
                        d.child(
                            div()
                                .min_w(px(0.))
                                .flex_1()
                                .truncate()
                                .text_color(theme.text_tertiary)
                                .child(row.description.clone()),
                        )
                    })
                    .when(check, |d| {
                        d.child(icon("check", theme.text.into()).size(px(14.)))
                    })
                    .into_any_element(),
            );
        }
        if self.context_catalog.loading || self.command_catalog.skills_loading {
            children.push(
                div()
                    .px(px(8.))
                    .py(px(5.))
                    .text_size(px(crate::typography::ui_size(13.)))
                    .text_color(theme.text_tertiary)
                    .child(crate::i18n::text("正在加载…"))
                    .into_any_element(),
            );
        }
        if rows.is_empty() && !self.context_catalog.loading && !self.context_catalog.searching {
            children.push(
                div()
                    .p(px(8.))
                    .text_size(px(crate::typography::ui_size(13.)))
                    .text_color(theme.text_tertiary)
                    .child(crate::i18n::text("无结果"))
                    .into_any_element(),
            );
        }
        if !self.context_catalog.project && self.context_catalog.query.trim().is_empty() {
            children.push(
                div()
                    .px(px(8.))
                    .pt(px(8.))
                    .pb(px(4.))
                    .text_size(px(crate::typography::ui_size(13.)))
                    .text_color(theme.text_tertiary)
                    .child(crate::i18n::text("文件和聊天"))
                    .into_any_element(),
            );
            children.push(
                div()
                    .px(px(8.))
                    .py(px(5.))
                    .text_size(px(crate::typography::ui_size(13.)))
                    .text_color(theme.text_tertiary)
                    .child(crate::i18n::text("输入内容以搜索文件或聊天"))
                    .into_any_element(),
            );
        } else if self.context_catalog.searching {
            children.push(
                div()
                    .px(px(8.))
                    .py(px(5.))
                    .text_size(px(crate::typography::ui_size(13.)))
                    .text_color(theme.text_tertiary)
                    .child(crate::i18n::text("正在搜索…"))
                    .into_any_element(),
            );
        }
        gpui::deferred(
            div()
                .id("side-chat-context-menu")
                .role(Role::Menu)
                .aria_label(crate::i18n::text("添加"))
                .absolute()
                .bottom(px(self.composer_body_height(cx) + self.tray_height() + 8.))
                .left_0()
                .w_full()
                .max_h(px(320.))
                .rounded(px(20.))
                .p(px(4.))
                .bg(surface)
                .border_1()
                .border_color(theme.text.alpha(0.08))
                .shadow_sm()
                .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    if !this
                        .context_catalog
                        .trigger_bounds
                        .get()
                        .is_some_and(|b| b.contains(&event.position))
                    {
                        this.dismiss_context(cx);
                        this.focus_prompt_pending = false;
                    }
                }))
                .child(
                    div()
                        .id("context-scroll")
                        .w_full()
                        .max_h(px(310.))
                        .overflow_y_scroll()
                        .track_scroll(&self.context_catalog.scroll)
                        .flex()
                        .flex_col()
                        .children(children),
                ),
        )
        .with_priority(21)
        .into_any_element()
    }
}
