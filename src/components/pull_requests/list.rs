//! Shared status glyphs and diff statistics for code review.
use super::{PullRequestsView, theme::*};
use crate::components::icons::icon;
use crate::pull_requests::{StatusIcon, format_count};
use gpui::{Div, div, prelude::*, px};
impl PullRequestsView {
    pub(crate) fn status_glyph(status: StatusIcon, theme: PrTheme, size: f32) -> Div {
        let (name, color, dot) = match status {
            StatusIcon::Merged => ("pr-status-merged", theme.purple, None),
            StatusIcon::Closed => ("pr-status-closed", theme.chart_red, None),
            StatusIcon::Draft => ("pr-status-draft", theme.text_muted, None),
            StatusIcon::Failing => ("pr-status-checks", theme.text_muted, Some(theme.chart_red)),
            StatusIcon::InProgress => (
                "pr-status-checks",
                theme.text_muted,
                Some(theme.chart_yellow),
            ),
            StatusIcon::Ready | StatusIcon::Successful => (
                "pr-status-checks",
                theme.text_muted,
                Some(theme.chart_green),
            ),
        };
        div()
            .relative()
            .flex_none()
            .size(px(size))
            .child(icon(name, color.into()).size(px(size)))
            .when_some(dot, |glyph, dot| {
                glyph.child(
                    icon("pr-status-dot", dot.into())
                        .absolute()
                        .top_0()
                        .left_0()
                        .size(px(size)),
                )
            })
    }

    /// `+x -y` in the reference's tabular, tightly tracked digits.
    pub(crate) fn diff_stats(
        additions: u64,
        deletions: u64,
        added: gpui::Rgba,
        deleted: gpui::Rgba,
        size: f32,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(size))
            .line_height(px(size))
            .font_features(stats_font_features())
            .child(
                div()
                    .flex_none()
                    .text_color(added)
                    .child(format!("+{}", format_count(additions))),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(deleted)
                    .child(format!("-{}", format_count(deletions))),
            )
    }
}
pub(crate) fn stats_font_features() -> gpui::FontFeatures {
    gpui::FontFeatures(std::sync::Arc::new(vec![
        ("tnum".into(), 1),
        ("cv01".into(), 1),
        ("cv02".into(), 1),
    ]))
    .with_letter_spacing(-0.025)
}
