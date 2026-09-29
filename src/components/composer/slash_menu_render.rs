//! The slash menu's panel: 8px above the composer (and its tray) at full
//! composer width, 16px radius, 4px padding, at most 320px tall. Rows are
//! 4×8 padded and 28px tall in the reference window, 12px radius, 75%
//! opaque until highlighted.
//!
//! The Code review submenu follows the reference's own list (DOM capture
//! `artifacts/batch3-review-*`): 28.6px rows padded 5×8 with a 15px radius
//! and no gap, full-strength text, and a muted section title padded 8/8/4.

use gpui::{
    AnyElement, Context, HighlightStyle, MouseMoveEvent, Role, SharedString, StyledText, div,
    prelude::*, px, rgba,
};

use super::{
    ComposerView,
    review::{ReviewBranches, ReviewRow},
    slash_menu::{DenialItem, SlashItem},
};
use crate::{components::icons::icon, theme::Theme, theme::ThemeMode};

const MENU_GAP: f32 = 8.0;
const MENU_MAX_HEIGHT: f32 = 320.0;

struct MenuColors {
    surface: gpui::Hsla,
    border: gpui::Hsla,
    text: gpui::Hsla,
    muted: gpui::Hsla,
    highlight: gpui::Hsla,
    /// The review submenu's rows are highlighted a shade lighter.
    review_highlight: gpui::Hsla,
    info: gpui::Hsla,
}

fn menu_colors(theme: Theme, mode: ThemeMode) -> MenuColors {
    let text: gpui::Hsla = theme.text.into();
    match mode {
        // Sampled from the reference in the chat window: the menu sits on
        // the composer's elevated surface, not the page background.
        ThemeMode::Dark => MenuColors {
            surface: rgba(0x2d2d2dff).into(),
            border: rgba(0x363636ff).into(),
            text,
            muted: rgba(0xffffff80).into(),
            highlight: rgba(0xffffff14).into(),
            review_highlight: rgba(0xffffff14).into(),
            info: rgba(0x339cffff).into(),
        },
        ThemeMode::Light => MenuColors {
            surface: rgba(0xffffffff).into(),
            border: rgba(0x1a1c1f14).into(),
            text,
            muted: rgba(0x1a1c1f80).into(),
            highlight: rgba(0x1a1c1f0d).into(),
            review_highlight: rgba(0x1a1c1f0e).into(),
            info: rgba(0x0285ffff).into(),
        },
    }
}

/// The title with its unmatched parts dimmed while a query is typed.
fn title_text(item: &SlashItem, colors: &MenuColors) -> StyledText {
    let mut dimmed = Vec::new();
    if !item.matched.is_empty() {
        let mut at = 0;
        for range in &item.matched {
            if range.start > at {
                dimmed.push(at..range.start);
            }
            at = range.end;
        }
        if at < item.title.len() {
            dimmed.push(at..item.title.len());
        }
    }
    StyledText::new(item.title.clone()).with_highlights(dimmed.into_iter().map(|range| {
        (
            range,
            HighlightStyle {
                color: Some(colors.muted),
                ..Default::default()
            },
        )
    }))
}

impl ComposerView {
    fn menu_row(
        &self,
        index: usize,
        highlighted: bool,
        colors: &MenuColors,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(("slash-row", index))
            .role(Role::MenuItem)
            .w_full()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(13.0))
            .line_height(px(20.0))
            .text_color(colors.text)
            .cursor_pointer()
            .opacity(if highlighted { 1.0 } else { 0.75 })
            .when(highlighted, |row| row.bg(colors.highlight))
            .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                if let Some(menu) = this.slash_menu.as_mut()
                    && menu.highlighted != index
                {
                    menu.highlighted = index;
                    cx.notify();
                }
            }))
    }

    fn command_row(
        &self,
        index: usize,
        item: SlashItem,
        highlighted: bool,
        colors: &MenuColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let command = item.command;
        let has_description = !item.description.is_empty();
        self.menu_row(index, highlighted, colors, cx)
            .aria_label(SharedString::from(item.title.clone()))
            .debug_selector(move || format!("SLASH_{}", command.title().to_uppercase()))
            .on_click(cx.listener(move |this, _, _, cx| this.select_slash_command(command, cx)))
            .child(
                div()
                    .size(px(16.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(command.icon(), colors.text).size(px(16.0))),
            )
            .child(
                div()
                    .flex_none()
                    .when(has_description, |title| title.max_w(gpui::relative(0.6)))
                    .truncate()
                    .child(title_text(&item, colors)),
            )
            .when(has_description, |row| {
                row.child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .truncate()
                        .text_color(colors.muted)
                        .child(item.description),
                )
            })
            .when(command.opens_submenu(), |row| {
                row.when(!has_description, |row| row.child(div().flex_1()))
                    .child(
                        icon("slash-submenu-chevron", colors.muted)
                            .size(px(16.0))
                            .flex_none(),
                    )
            })
            .into_any_element()
    }

    fn review_row(
        &self,
        index: usize,
        row: ReviewRow,
        highlighted: bool,
        colors: &MenuColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = row.title();
        let selector = match &row {
            ReviewRow::Uncommitted => "SLASH_REVIEW_UNCOMMITTED".to_owned(),
            ReviewRow::Branch(_) => format!("SLASH_REVIEW_BRANCH_{index}"),
        };
        div()
            .id(("slash-review-row", index))
            .role(Role::MenuItem)
            .aria_label(SharedString::from(title.clone()))
            .debug_selector(move || selector.clone())
            .w_full()
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(15.0))
            .flex()
            .items_center()
            .text_size(px(13.0))
            .line_height(px(18.57))
            .text_color(colors.text)
            .cursor_pointer()
            .when(highlighted, |item| item.bg(colors.highlight))
            .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                if let Some(menu) = this.slash_menu.as_mut()
                    && menu.highlighted != index
                {
                    menu.highlighted = index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.select_review_row(index, cx)))
            .child(div().min_w(px(0.0)).flex_1().truncate().child(title))
            .into_any_element()
    }

    /// Uncommitted changes, then the base-branch section with its branches
    /// or its loading and error states.
    fn review_rows_elements(
        &self,
        highlighted: usize,
        colors: &MenuColors,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let rows = self.review_rows(cx);
        let mut elements = Vec::new();
        let mut index = 0;
        let mut rows = rows.into_iter().peekable();
        if rows.peek() == Some(&ReviewRow::Uncommitted) {
            let row = rows.next().expect("peeked");
            elements.push(self.review_row(index, row, highlighted == index, colors, cx));
            index += 1;
        }
        elements.push(
            div()
                .w_full()
                .pt(px(8.0))
                .px(px(8.0))
                .pb(px(4.0))
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(colors.muted)
                .child(crate::i18n::format!("与基准分支比较" => "Review against a base branch"))
                .into_any_element(),
        );
        match &self.review_branches {
            ReviewBranches::Loading => elements.push(
                div()
                    .w_full()
                    .py(px(8.0))
                    .flex()
                    .justify_center()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(colors.muted)
                    .child(crate::i18n::format!("正在加载分支…" => "Loading branches…"))
                    .into_any_element(),
            ),
            ReviewBranches::Failed => elements.push(
                div()
                    .w_full()
                    .py(px(8.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(colors.muted)
                            .child(
                                crate::i18n::format!("无法加载分支" => "Unable to load branches"),
                            ),
                    )
                    .child(
                        div()
                            .id("slash-review-retry")
                            .role(Role::Button)
                            .debug_selector(|| "SLASH_REVIEW_RETRY".to_owned())
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(colors.info)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| this.load_review_branches(cx)))
                            .child(crate::i18n::format!("重试" => "Retry")),
                    )
                    .into_any_element(),
            ),
            ReviewBranches::Loaded(_) => {
                for row in rows {
                    elements.push(self.review_row(index, row, highlighted == index, colors, cx));
                    index += 1;
                }
            }
        }
        elements
    }

    fn denial_row(
        &self,
        index: usize,
        item: DenialItem,
        highlighted: bool,
        busy: bool,
        colors: &MenuColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.menu_row(index, highlighted, colors, cx)
            .aria_label(SharedString::from(item.title.clone()))
            .debug_selector(move || format!("SLASH_DENIAL_{index}"))
            .when(busy, |row| row.opacity(0.5).cursor_default())
            .when(!busy, |row| {
                row.on_click(cx.listener(move |this, _, _, cx| this.select_denial(index, cx)))
            })
            .child(
                div()
                    .size(px(16.0))
                    .flex_none()
                    .child(icon("auto-review-shield", colors.text).size(px(16.0))),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(div().truncate().child(item.title))
                    .child(
                        div()
                            .truncate()
                            .pt(px(2.0))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(colors.muted)
                            .child(item.detail),
                    ),
            )
            .into_any_element()
    }

    /// The menu, positioned by the caller inside the composer.
    pub(super) fn render_slash_menu(
        &self,
        theme: Theme,
        body_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.slash_menu_open(cx) {
            return None;
        }
        let menu = self.slash_menu.clone()?;
        let colors = menu_colors(theme, self.mode);
        let mut rows: Vec<AnyElement> = Vec::new();
        let review = menu.review && !menu.denials;
        if review {
            let colors = MenuColors {
                highlight: colors.review_highlight,
                ..menu_colors(theme, self.mode)
            };
            rows = self.review_rows_elements(menu.highlighted, &colors, cx);
        } else if menu.denials {
            rows.push(
                div()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_size(px(13.0))
                    .line_height(px(18.57))
                    .text_color(colors.muted)
                    .child(crate::i18n::format!(
                        "选择一条自动审查驳回记录，以批准一次重试。此次重试仍会经过自动审查。"
                            => "Select an auto-review denial to approve one retry. The retry will still go through auto-review."
                    ))
                    .into_any_element(),
            );
            let busy = self.denial_approval_in_flight();
            for (index, item) in self.denial_items().into_iter().enumerate() {
                rows.push(self.denial_row(
                    index,
                    item,
                    index == menu.highlighted,
                    busy,
                    &colors,
                    cx,
                ));
            }
        } else {
            let items = self.slash_items(cx);
            if items.is_empty() {
                rows.push(
                    div()
                        .px(px(8.0))
                        .py(px(4.0))
                        .text_size(px(13.0))
                        .text_color(colors.muted)
                        .child(crate::i18n::format!("无命令" => "No commands"))
                        .into_any_element(),
                );
            }
            for (index, item) in items.into_iter().enumerate() {
                rows.push(self.command_row(index, item, index == menu.highlighted, &colors, cx));
            }
        }
        Some(
            gpui::deferred(
                div()
                    .id("slash-menu")
                    .debug_selector(|| "SLASH_MENU".to_owned())
                    .role(Role::Menu)
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(body_height + self.tray_height() + MENU_GAP))
                    .max_h(px(MENU_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .p(px(4.0))
                    .rounded(px(16.0))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.surface)
                    .flex()
                    .flex_col()
                    .when(!review, |panel| panel.gap(px(4.0)))
                    .children(rows),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
