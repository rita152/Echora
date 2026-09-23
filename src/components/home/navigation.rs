//! The conversation's user-message navigation rail and its hover preview.
//!
//! Every number here comes from the live ChatGPT capture in
//! `artifacts/user-message-rail/reference/report.json`: a 36 x 10 marker
//! button per user message in a column 16 px from the transcript pane's left
//! edge, a 30 x 2 track whose 26 x 2 line is scaled by the hovered distance,
//! and a 320 px hover card that repeats the prompt above the response preview.

use gpui::{
    AnyElement, BoxShadow, Entity, FontWeight, IntoElement, ListState, Pixels, Rgba, Role, TextRun,
    Window, div, prelude::*, px, rgba,
};

use super::HomeView;
use crate::{
    components::{icons::icon, markdown::plain_text_blocks},
    theme::{CHAT_CONTENT_HORIZONTAL_GUTTER, Theme},
};

/// The card prints CJK through CoreText's cascade, which lands on the hidden
/// `.PingFang UI` face; naming the public `PingFang SC` instead makes every
/// line about 2% too wide for the reference's wrap.
const RAIL_CARD_CJK_FALLBACK: &str = ".PingFang UI";

fn hover_card_font(weight: gpui::FontWeight) -> gpui::Font {
    gpui::Font {
        family: crate::theme::UI_FONT_FAMILY.into(),
        features: Default::default(),
        fallbacks: Some(gpui::FontFallbacks::from_fonts(vec![
            RAIL_CARD_CJK_FALLBACK.to_owned(),
        ])),
        weight,
        style: gpui::FontStyle::Normal,
    }
}

/// Distance from the transcript pane's left edge (`electron:left-4`).
pub(super) const RAIL_LEFT: f32 = 16.0;
/// The conversation view is inset by the workspace's content gutter, so the
/// rail subtracts it to stay 16 px from the transcript surface itself.
const RAIL_LEFT_IN_VIEW: f32 = RAIL_LEFT - CHAT_CONTENT_HORIZONTAL_GUTTER;
/// `w-9`.
pub(super) const RAIL_WIDTH: f32 = 36.0;
/// `h-2.5`.
const RAIL_ITEM_HEIGHT: f32 = 10.0;
/// `max-h-[min(70vh,40rem)]`.
const RAIL_MAX_HEIGHT_RATIO: f32 = 0.7;
const RAIL_MAX_HEIGHT: f32 = 640.0;
/// The reference renders the rail only once a task has four user messages.
pub(super) const RAIL_MINIMUM_ITEMS: usize = 4;
/// The rail column is 52 px wide; the capture flips over exactly when the
/// 768 px transcript column still leaves that gutter on both sides.
pub(super) const RAIL_MINIMUM_PANE_WIDTH: f32 = 872.0;
/// `w-[30px] h-0.5`.
const MARKER_TRACK_WIDTH: f32 = 30.0;
const MARKER_TRACK_HEIGHT: f32 = 2.0;
/// `width: 26px`.
const MARKER_WIDTH: f32 = 26.0;
/// `scaleX(calc(.2308 + .7692 * progress))`.
const MARKER_REST_SCALE: f32 = 0.2308;
const MARKER_HOVER_SCALE: f32 = 0.7692;
const MARKER_REST_OPACITY: f32 = 0.4;
const MARKER_CURRENT_OPACITY: f32 = 0.6;
/// `_BookmarkDot_`: `left: calc(var(--spacing) * .5)` and `size-0.5`.
const BOOKMARK_DOT_LEFT: f32 = 2.0;
const BOOKMARK_DOT_SIZE: f32 = 2.0;

/// `left-4` + `w-9`: the card starts where the marker button ends.
pub(super) const CARD_LEFT: f32 = RAIL_LEFT + RAIL_WIDTH;
pub(super) const CARD_WIDTH: f32 = 320.0;
pub(super) const CARD_PADDING: f32 = 8.0;
const CARD_RADIUS: f32 = 15.0;
/// `ring-[0.5px]`: a painted ring rather than a border, so the card's text
/// column keeps the reference's exact 304 px.
const CARD_RING: f32 = 0.5;
const CARD_HEADER_HEIGHT: f32 = 20.0;
const CARD_HEADER_GAP: f32 = 6.0;
const CARD_PROMPT_SIZE: f32 = 13.0;
const CARD_PROMPT_WEIGHT: FontWeight = FontWeight(500.0);
/// The reference prints the preview at 13 px. GPUI resolves the same family
/// with about 3% narrower advances, which moves the clamped preview's line
/// breaks; 13.4 px restores the reference's own break positions, which is what
/// the pixel comparison scores.
const CARD_PREVIEW_SIZE: f32 = 13.4;
const CARD_PREVIEW_LINE_HEIGHT: f32 = 21.0;
/// `line-clamp-3`.
const CARD_PREVIEW_LINES: usize = 3;
/// `mt-1`.
const CARD_PREVIEW_GAP: f32 = 4.0;
/// Markdown block gap inside the card's small preview.
const CARD_PARAGRAPH_GAP: f32 = 13.0;
const CARD_BOOKMARK_SIZE: f32 = 16.0;
/// `[&>svg]:icon-2xs` leaves the 18 px glyph overflowing the 16 px button.
const CARD_BOOKMARK_GLYPH: f32 = 18.0;
const CARD_BOOKMARK_RADIUS: f32 = 10.0;
/// The card is centred on its marker; the band is the vertical slack GPUI
/// needs to centre content whose height it cannot measure up front.
pub(super) const CARD_BAND_HALF_HEIGHT: f32 = 160.0;
/// Radix clamps the floating card to `calc(100vh - 16px)`.
pub(super) const CARD_VIEWPORT_MARGIN: f32 = 16.0;

/// One user message of the loaded task, in transcript order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserMessageNavigationItem {
    /// Row of the message in the conversation list, used to jump to it.
    pub(super) row_index: usize,
    /// Last row of the turn that starts with this message, so the rail can tell
    /// whether the turn is on screen.
    pub(super) turn_end_row: usize,
    /// Stable identity of the turn inside the task, used for bookmarks.
    pub(super) bookmark_id: String,
    /// The prompt, trimmed exactly like the reference's `getLabel()`.
    pub(super) label: String,
    /// The assistant response of the same turn, shown under the prompt.
    pub(super) preview: String,
}

/// The reference's `--marker-progress` for a marker `distance` steps away from
/// the hovered one (`.7`, `.4`, `.2`, then rest).
pub(super) fn marker_progress(index: usize, hovered: Option<usize>) -> f32 {
    let Some(hovered) = hovered else {
        return 0.0;
    };
    match index.abs_diff(hovered) {
        0 => 1.0,
        1 => 0.7,
        2 => 0.4,
        3 => 0.2,
        _ => 0.0,
    }
}

pub(super) fn marker_dash_width(progress: f32) -> f32 {
    MARKER_WIDTH * (MARKER_REST_SCALE + MARKER_HOVER_SCALE * progress)
}

/// Marker colour and opacity for one rail row. The hovered marker prints the
/// theme's foreground at full strength; a bookmark lifts its resting opacity
/// to one, and while the pointer sits on the rail the current markers fall
/// back to the resting colour.
pub(super) fn marker_paint(
    theme: Theme,
    current: bool,
    progress: f32,
    rail_hovered: bool,
    bookmarked: bool,
) -> (Rgba, f32) {
    let hovered = progress >= 1.0;
    let current = (current && !rail_hovered) || hovered;
    let colour = if current {
        theme.text
    } else {
        theme.navigation_rail_marker
    };
    let opacity = if bookmarked || hovered {
        1.0
    } else if current {
        MARKER_CURRENT_OPACITY
    } else {
        MARKER_REST_OPACITY
    };
    (colour, opacity)
}

/// Whether the turn spanning `first..=last` overlaps the viewport between
/// `top` and `bottom`. Rows the list has already scrolled past have no bounds;
/// the turn is still on screen as long as its last row is inside the crop.
fn turn_intersects(
    list: &ListState,
    first: usize,
    last: usize,
    top: Pixels,
    bottom: Pixels,
) -> bool {
    let last_above = match list.bounds_for_item(last) {
        Some(bounds) => bounds.bottom() <= top,
        None => list.item_is_above_viewport(last).unwrap_or(false),
    };
    let first_below = match list.bounds_for_item(first) {
        Some(bounds) => bounds.top() >= bottom,
        None => list.item_is_below_viewport(first).unwrap_or(false),
    };
    !last_above && !first_below
}

/// `aria-current` set: the contiguous run of turns that intersect the viewport,
/// with the reference's 16 px top crop. Rows the list has not measured count as
/// off screen, which is what the reference's intersection observer reports too.
pub(super) fn current_items(list: &ListState, items: &[UserMessageNavigationItem]) -> Vec<bool> {
    let viewport = list.viewport_bounds();
    let top = viewport.top() + px(16.0);
    let bottom = viewport.bottom();
    let flags = items
        .iter()
        .map(|item| turn_intersects(list, item.row_index, item.turn_end_row, top, bottom))
        .collect::<Vec<_>>();
    contiguous_run(&flags)
}

/// The reference keeps only the run of visible turns that starts at the first
/// one, so a message far below the viewport never lights up its marker.
fn contiguous_run(flags: &[bool]) -> Vec<bool> {
    let Some(first) = flags.iter().position(|visible| *visible) else {
        return flags.to_vec();
    };
    let mut end = first;
    for (index, visible) in flags.iter().enumerate().skip(first) {
        if *visible {
            end = index;
        } else {
            break;
        }
    }
    flags
        .iter()
        .enumerate()
        .map(|(index, _)| index >= first && index <= end)
        .collect()
}

pub(super) fn rail_visible(items: usize, pane_width: f32) -> bool {
    items >= RAIL_MINIMUM_ITEMS && pane_width >= RAIL_MINIMUM_PANE_WIDTH
}

pub(super) fn rail_max_height(pane_height: f32) -> f32 {
    (pane_height * RAIL_MAX_HEIGHT_RATIO).min(RAIL_MAX_HEIGHT)
}

/// The response preview as the card prints it: at most three wrapped lines,
/// carrying the markdown paragraphs they were cut from.
/// Byte index where the trailing ASCII word run starts, so a clamped line never
/// ends in the middle of a Latin word.
fn ascii_run_start(text: &str) -> usize {
    for (index, character) in text.char_indices().rev() {
        let in_word =
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | '+');
        if !in_word {
            return index + character.len_utf8();
        }
    }
    0
}

pub(super) fn preview_paragraphs(
    source: &str,
    width: f32,
    window: &Window,
) -> Vec<(String, usize)> {
    let font = hover_card_font(crate::theme::UI_BODY_FONT_WEIGHT);
    let run = |text: &str| TextRun {
        len: text.len(),
        font: font.clone(),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line_count = |text: &str| -> usize {
        window
            .text_system()
            .shape_text(
                text.to_owned().into(),
                px(CARD_PREVIEW_SIZE),
                &[run(text)],
                Some(px(width)),
                None,
            )
            // `shape_text` reports one entry per source line; the wrapped lines
            // inside it are the boundaries the renderer will actually break at.
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| line.wrap_boundaries.len() + 1)
                    .sum()
            })
            .unwrap_or(1)
            .max(1)
    };
    let mut remaining = CARD_PREVIEW_LINES;
    let mut out = Vec::new();
    for text in plain_text_blocks(source) {
        if remaining == 0 {
            break;
        }
        let lines = line_count(&text);
        if lines <= remaining {
            remaining -= lines;
            out.push((text, lines));
            continue;
        }
        // The reference clamps the block with `-webkit-line-clamp`, which fills
        // the last visible line and marks the cut with an ellipsis. The clamp
        // is applied to the whole block, so the paragraph that overflows is cut
        // inside the last line the card prints.
        let shown = remaining;
        let mut low = 0;
        let mut high = text.chars().count();
        while low < high {
            let middle = (low + high).div_ceil(2);
            let candidate = text.chars().take(middle).collect::<String>() + "…";
            if line_count(&candidate) <= shown {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        let mut truncated = text.chars().take(low).collect::<String>();
        // Chrome truncates the clamped line at the last break opportunity that
        // fits, so a limit that landed inside a Latin run backs up to the run's
        // start instead of printing half a word.
        let cut = ascii_run_start(&truncated);
        truncated.truncate(cut);
        truncated.push('…');
        remaining = 0;
        out.push((truncated, shown));
    }
    out
}

/// The floating rail: one button per user message, centred in the pane.
pub(super) fn user_message_rail(
    items: &[UserMessageNavigationItem],
    current: &[bool],
    hovered: Option<usize>,
    bookmarked: &dyn Fn(usize) -> bool,
    pane_height: f32,
    theme: Theme,
    home: Entity<HomeView>,
) -> AnyElement {
    let mut column = div().flex().flex_col();
    for (index, _item) in items.iter().enumerate() {
        let progress = marker_progress(index, hovered);
        let is_current = current.get(index).copied().unwrap_or(false);
        let is_bookmarked = bookmarked(index);
        let (colour, opacity) = marker_paint(
            theme,
            is_current,
            progress,
            hovered.is_some(),
            is_bookmarked,
        );
        let dash_width = marker_dash_width(progress);
        let hover_home = home.clone();
        let click_home = home.clone();
        let label = if is_bookmarked {
            crate::i18n::format!(
                "跳转到第 {position} 条用户消息，已收藏" => "Jump to user message {position}, bookmarked turn",
                position = index + 1
            )
        } else {
            crate::i18n::format!(
                "跳转到第 {position} 条用户消息" => "Jump to user message {position}",
                position = index + 1
            )
        };
        column = column.child(
            div()
                .id(("user-message-navigation-item", index))
                .w(px(RAIL_WIDTH))
                .h(px(RAIL_ITEM_HEIGHT))
                .flex_none()
                .flex()
                .items_center()
                .role(Role::Button)
                .aria_label(label)
                .cursor_pointer()
                .on_hover(move |hovered: &bool, _window, cx| {
                    let hovered = *hovered;
                    hover_home.update(cx, |home, cx| {
                        home.set_user_message_navigation_hover(index, hovered, cx);
                    });
                })
                .on_click(move |_event, _window, cx| {
                    click_home.update(cx, |home, cx| home.reveal_user_message(index, cx));
                })
                .child(
                    div()
                        .w(px(MARKER_TRACK_WIDTH))
                        .h(px(MARKER_TRACK_HEIGHT))
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .relative()
                                .w(px(MARKER_WIDTH))
                                .h(px(MARKER_TRACK_HEIGHT))
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(0.0))
                                        .top(px(0.0))
                                        .w(px(dash_width))
                                        .h(px(MARKER_TRACK_HEIGHT))
                                        .bg(colour)
                                        .opacity(opacity),
                                )
                                .when(is_bookmarked, |marker| {
                                    marker.child(
                                        div()
                                            .absolute()
                                            .left(px(
                                                BOOKMARK_DOT_LEFT + marker_dash_width(progress)
                                            ))
                                            .top(px(0.0))
                                            .size(px(BOOKMARK_DOT_SIZE))
                                            .rounded(px(BOOKMARK_DOT_SIZE / 2.0))
                                            .bg(colour)
                                            .opacity(opacity),
                                    )
                                }),
                        ),
                ),
        );
    }
    div()
        .id("user-message-navigation-rail")
        .w(px(RAIL_WIDTH))
        .max_h(px(rail_max_height(pane_height)))
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .scrollbar_width(px(0.0))
        .child(column)
        .into_any_element()
}

/// The rail plus its hover card, positioned relative to the transcript pane.
/// The pane is the containing block, so the rail sits 16 px from its left edge
/// and stays centred no matter where the transcript has scrolled to.
pub(super) fn user_message_navigation_overlay(
    items: &[UserMessageNavigationItem],
    current: &[bool],
    hovered: Option<usize>,
    bookmarked: &[bool],
    preview: &[(String, usize)],
    view_top: f32,
    window_height: f32,
    theme: Theme,
    home: Entity<HomeView>,
) -> AnyElement {
    let is_bookmarked = |index: usize| bookmarked.get(index).copied().unwrap_or(false);
    let rail_height = (items.len() as f32 * RAIL_ITEM_HEIGHT).min(rail_max_height(window_height));
    // The reference centres the rail in the window, not in the scrolled pane.
    let rail_top_window = (window_height - rail_height) * 0.5;
    let rail_top = rail_top_window - view_top;
    let mut overlay = div()
        .absolute()
        .left(px(RAIL_LEFT_IN_VIEW))
        .top(px(rail_top))
        .w(px(RAIL_WIDTH))
        .h(px(rail_height))
        .flex()
        .flex_col()
        .child(user_message_rail(
            items,
            current,
            hovered,
            &is_bookmarked,
            window_height,
            theme,
            home.clone(),
        ));

    if let Some(index) = hovered
        && let Some(item) = items.get(index)
        && !preview.is_empty()
    {
        let item_centre =
            rail_top_window + index as f32 * RAIL_ITEM_HEIGHT + RAIL_ITEM_HEIGHT * 0.5;
        let band_height = CARD_BAND_HALF_HEIGHT * 2.0;
        let band_top = (item_centre - CARD_BAND_HALF_HEIGHT).clamp(
            CARD_VIEWPORT_MARGIN,
            (window_height - CARD_VIEWPORT_MARGIN - band_height).max(CARD_VIEWPORT_MARGIN),
        ) - rail_top_window;
        overlay = overlay.child(
            div()
                .absolute()
                .left(px(CARD_LEFT - RAIL_LEFT))
                .top(px(band_top))
                .w(px(CARD_WIDTH))
                .h(px(band_height))
                .flex()
                .flex_col()
                .items_start()
                .justify_center()
                .child(user_message_card(
                    item,
                    index,
                    is_bookmarked(index),
                    preview,
                    theme,
                    home,
                )),
        );
    }

    overlay.into_any_element()
}

/// The hover card: prompt row, bookmark control, then the response preview.
pub(super) fn user_message_card(
    item: &UserMessageNavigationItem,
    index: usize,
    bookmarked: bool,
    preview: &[(String, usize)],
    theme: Theme,
    home: Entity<HomeView>,
) -> AnyElement {
    let card_hover_home = home.clone();
    let bookmark_home = home.clone();
    let header = div()
        .w_full()
        .h(px(CARD_HEADER_HEIGHT))
        .flex()
        .items_center()
        .gap(px(CARD_HEADER_GAP))
        .font(hover_card_font(CARD_PROMPT_WEIGHT))
        .text_size(px(CARD_PROMPT_SIZE))
        .line_height(px(CARD_HEADER_HEIGHT))
        .text_color(theme.text)
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(item.label.clone()),
        )
        .child(
            div()
                .id(("user-message-navigation-bookmark", index))
                .flex_none()
                .size(px(CARD_BOOKMARK_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(CARD_BOOKMARK_RADIUS))
                .border_1()
                .border_color(rgba(0x00000000))
                .role(Role::Button)
                .aria_label(crate::i18n::text(if bookmarked {
                    "取消收藏该轮次"
                } else {
                    "收藏该轮次"
                }))
                .cursor_pointer()
                .text_color(theme.navigation_rail_marker)
                .hover(|style| style.text_color(theme.text))
                .on_click(move |_event, _window, cx| {
                    cx.stop_propagation();
                    bookmark_home.update(cx, |home, cx| {
                        home.toggle_user_message_bookmark(index, cx);
                    });
                })
                .child(
                    icon(
                        if bookmarked {
                            "bookmark-filled"
                        } else {
                            "bookmark"
                        },
                        theme.navigation_rail_marker.into(),
                    )
                    .size(px(CARD_BOOKMARK_GLYPH)),
                ),
        );

    let preview_column = preview.iter().enumerate().fold(
        div().w_full().flex().flex_col(),
        |column, (position, (text, lines))| {
            column.child(
                div()
                    .w_full()
                    .when(position > 0, |paragraph| {
                        paragraph.mt(px(CARD_PARAGRAPH_GAP))
                    })
                    .text_size(px(CARD_PREVIEW_SIZE))
                    .line_height(px(CARD_PREVIEW_LINE_HEIGHT))
                    .font(hover_card_font(crate::theme::UI_BODY_FONT_WEIGHT))
                    .text_color(theme.navigation_rail_marker)
                    .line_clamp(*lines)
                    .child(text.clone()),
            )
        },
    );

    div()
        .id(("user-message-navigation-card", index))
        .w(px(CARD_WIDTH))
        .p(px(CARD_PADDING))
        .flex()
        .flex_col()
        .rounded(px(CARD_RADIUS))
        .bg(theme.navigation_rail_surface)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(0.0), theme.border.into()).spread_radius(px(CARD_RING)),
            BoxShadow::new(px(0.0), px(8.0), theme.profile_menu_shadow.into())
                .blur_radius(px(16.0))
                .spread_radius(px(-4.0)),
        ])
        .overflow_hidden()
        .on_hover(move |hovered: &bool, _window, cx| {
            let hovered = *hovered;
            card_hover_home.update(cx, |home, cx| {
                home.set_user_message_card_hovered(index, hovered, cx);
            });
        })
        .child(header)
        .child(div().mt(px(CARD_PREVIEW_GAP)).child(preview_column))
        .into_any_element()
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Geometry and colours measured from the live reference: hovering one
    /// marker widens it to 26 px and tapers its neighbours to 20, 14, 10, 6.
    #[test]
    fn marker_widths_match_the_reference_taper() {
        let assert_widths = |hovered: Option<usize>, expected: &[f32]| {
            for (index, want) in expected.iter().enumerate() {
                let got = marker_dash_width(marker_progress(index, hovered));
                assert!(
                    (got - want).abs() < 0.01,
                    "marker {index} with hovered {hovered:?} was {got}px, expected {want}px"
                );
            }
        };
        assert_widths(None, &[6.0, 6.0, 6.0, 6.0, 6.0, 6.0]);
        assert_widths(Some(0), &[26.0, 20.0, 14.0, 10.0, 6.0, 6.0]);
        assert_widths(Some(3), &[10.0, 14.0, 20.0, 26.0, 20.0, 14.0]);
    }

    #[test]
    fn marker_paint_follows_the_hover_and_current_rules() {
        for mode in [
            crate::theme::ThemeMode::Light,
            crate::theme::ThemeMode::Dark,
        ] {
            let theme = Theme::for_mode(mode);
            let resting = marker_paint(theme, false, 0.0, false, false);
            assert_eq!(resting.0, theme.navigation_rail_marker);
            assert_eq!(resting.1, 0.4);

            let current = marker_paint(theme, true, 0.0, false, false);
            assert_eq!(current.0, theme.text);
            assert_eq!(current.1, 0.6);

            let hovered = marker_paint(theme, false, 1.0, true, false);
            assert_eq!(hovered.0, theme.text);
            assert_eq!(hovered.1, 1.0);

            // While the pointer rests on the rail, a current marker that is not
            // the hovered one falls back to the resting colour.
            let muted = marker_paint(theme, true, 0.0, true, false);
            assert_eq!(muted.0, theme.navigation_rail_marker);
            assert_eq!(muted.1, 0.4);

            // A bookmark lifts the resting marker to full opacity.
            let bookmarked = marker_paint(theme, false, 0.0, false, true);
            assert_eq!(bookmarked.1, 1.0);
        }
    }

    #[test]
    fn rail_needs_four_messages_and_a_wide_enough_pane() {
        assert!(!rail_visible(3, 2000.0));
        assert!(!rail_visible(4, RAIL_MINIMUM_PANE_WIDTH - 1.0));
        assert!(rail_visible(4, RAIL_MINIMUM_PANE_WIDTH));
        assert!(rail_visible(14, 1800.0));
        // `max-h-[min(70vh,40rem)]`.
        assert_eq!(rail_max_height(1000.0), 640.0);
        assert_eq!(rail_max_height(800.0), 560.0);
    }

    #[test]
    fn current_markers_keep_only_the_first_visible_run() {
        assert_eq!(contiguous_run(&[false, false]), vec![false, false]);
        assert_eq!(
            contiguous_run(&[true, true, false, true]),
            vec![true, true, false, false]
        );
        assert_eq!(
            contiguous_run(&[false, true, true, false]),
            vec![false, true, true, false]
        );
    }

    /// The clamp backs up over a Latin run so the last line never prints half a
    /// word, which is what the reference's `-webkit-line-clamp` does.
    #[test]
    fn clamped_lines_stop_at_the_last_break_opportunity() {
        assert_eq!(
            ascii_run_start("附件、AGENTS.md、gi"),
            "附件、AGENTS.md、".len()
        );
        assert_eq!(ascii_run_start("正文 + real_"), "正文 + ".len());
        assert_eq!(ascii_run_start("附件、"), "附件、".len());
        assert_eq!(ascii_run_start("gitignore"), 0);
    }
}
