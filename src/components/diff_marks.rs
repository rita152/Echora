//! Marks the reference's diff viewer paints over diff rows: the word-level
//! highlight boxes inside a changed line and the striped deleted-line bar.

use gpui::{Div, IntoElement, ParentElement, Styled, div, px};

use crate::theme::UI_MONOSPACE_FONT_FAMILY;

/// The deleted-line bar: `linear-gradient(0deg, <row> 50%, <red> 50%)` tiled
/// every 1.96364px, a red stripe over each tile's top half.
const DELETED_BAR_TILE: f32 = 1.96364;

/// Rounded boxes behind the changed word spans of a laid-out diff line.
pub(crate) fn word_boxes(
    layout: gpui::TextLayout,
    spans: Vec<std::ops::Range<usize>>,
    color: gpui::Rgba,
) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let text = layout.text();
            let Some(line) = layout.line_layout_for_index(0) else {
                return;
            };
            let unwrapped = &line.unwrapped_layout;
            let font = unwrapped
                .runs
                .first()
                .map(|run| run.font_id)
                .unwrap_or_else(|| {
                    window
                        .text_system()
                        .resolve_font(&gpui::font(UI_MONOSPACE_FONT_FAMILY))
                });
            let size = unwrapped.font_size;
            let content = window.text_system().ascent(font, size)
                + window.text_system().descent(font, size).abs();
            let inset = (layout.line_height() - content) / 2.0;
            for span in spans {
                let mut current: Option<gpui::Bounds<gpui::Pixels>> = None;
                let mut boxes = Vec::new();
                for (offset, ch) in text.get(span.clone()).unwrap_or_default().char_indices() {
                    let index = span.start + offset;
                    let Some(origin) = layout.position_for_index(index) else {
                        continue;
                    };
                    let advance =
                        unwrapped.x_for_index(index + ch.len_utf8()) - unwrapped.x_for_index(index);
                    match current.as_mut() {
                        Some(bounds) if bounds.origin.y == origin.y + inset => {
                            bounds.size.width = origin.x + advance - bounds.origin.x;
                        }
                        _ => {
                            boxes.extend(current.take());
                            current = Some(gpui::Bounds::new(
                                gpui::point(origin.x, origin.y + inset),
                                gpui::size(advance, content),
                            ));
                        }
                    }
                }
                boxes.extend(current);
                for bounds in boxes {
                    window.paint_quad(gpui::fill(bounds, color).corner_radii(px(3.0)));
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The deleted-line change bar: red stripes over the row color.
pub(crate) fn deleted_bar(stripe: gpui::Rgba, row: gpui::Rgba) -> Div {
    div()
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(px(4.0))
        .child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    window.paint_quad(gpui::fill(bounds, row));
                    let height = f32::from(bounds.size.height);
                    let mut top = 0.0;
                    while top < height {
                        let stripe_bounds = gpui::Bounds::new(
                            gpui::point(bounds.origin.x, bounds.origin.y + px(top)),
                            gpui::size(
                                bounds.size.width,
                                px((DELETED_BAR_TILE / 2.0).min(height - top)),
                            ),
                        );
                        window.paint_quad(gpui::fill(stripe_bounds, stripe));
                        top += DELETED_BAR_TILE;
                    }
                },
            )
            .size_full(),
        )
}
