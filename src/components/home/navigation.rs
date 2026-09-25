//! The conversation's user-message navigation rail and its hover preview.
//!
//! Every number here comes from the live ChatGPT capture in
//! `artifacts/user-message-rail/reference/report.json`: a 36 x 10 marker
//! button per user message in a column 16 px from the transcript pane's left
//! edge, a 30 x 2 track whose 26 x 2 line is scaled by the hovered distance,
//! and a 320 px hover card that repeats the prompt above the response preview.
//! How the rail moves and answers the pointer lives in `interaction` and
//! `motion`.

mod interaction;
mod motion;
mod preview;

use std::{cell::RefCell, rc::Rc};

use gpui::{
    AnyElement, BoxShadow, ClickEvent, DispatchPhase, Entity, FontWeight, HitboxBehavior,
    IntoElement, KeyDownEvent, ListState, MouseButton, MouseExitEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Rgba, Role, ScrollHandle, ScrollWheelEvent, Window, canvas, div,
    prelude::*, px, rgba,
};

pub(super) use interaction::{MessageStep, RailFrame, UserMessageRail};

gpui::actions!(
    user_message_navigation,
    [
        /// Alt+↑: scroll to the previous user prompt.
        PreviousUserMessage,
        /// Alt+↓: scroll to the next user prompt.
        NextUserMessage
    ]
);

/// The reference listens for Alt+↑/↓ on the whole document while a thread is
/// open, so the keys work from the composer as well.
pub(crate) fn init_keyboard(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("alt-up", PreviousUserMessage, Some("ChatApp")),
        gpui::KeyBinding::new("alt-down", NextUserMessage, Some("ChatApp")),
    ]);
}

use super::{CONVERSATION_TOP_INSET, HomeView};
use crate::{
    components::icons::icon,
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
pub(super) const RAIL_ITEM_HEIGHT: f32 = 10.0;
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
/// Room for an outside list marker and the space that ends it.
const LIST_MARKER_BOX: f32 = 21.0;
const LIST_MARKER_GAP: f32 = 4.0;
const CARD_BOOKMARK_SIZE: f32 = 16.0;
/// `[&>svg]:icon-2xs` leaves the 18 px glyph overflowing the 16 px button.
const CARD_BOOKMARK_GLYPH: f32 = 18.0;
const CARD_BOOKMARK_RADIUS: f32 = 10.0;
/// Radix clamps the floating card to `calc(100vh - 16px)`.
pub(super) const CARD_VIEWPORT_MARGIN: f32 = 16.0;

/// One user message of the loaded task, in transcript order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserMessageNavigationItem {
    /// Row of the message in the conversation list, used to jump to it.
    pub(super) row_index: usize,
    /// Last row this message is observed through: the whole turn for its
    /// first prompt, the prompt's own row for a later one in the same turn.
    pub(super) turn_end_row: usize,
    /// Ordinal of the transcript turn the message belongs to.
    pub(super) turn: usize,
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

/// Marker colour and opacity for one rail row. The hovered or scrubbed
/// marker prints the theme's foreground at full strength; the current
/// markers print it at 0.6 until the pointer rests on the rail or a scrub
/// runs, and a bookmark lifts either resting opacity to one. Colour and
/// opacity switch without a transition, as in the reference.
pub(super) fn marker_paint(
    theme: Theme,
    current: bool,
    focused: bool,
    muted: bool,
    bookmarked: bool,
) -> (Rgba, f32) {
    if focused {
        return (theme.text, 1.0);
    }
    if current && !muted {
        let opacity = if bookmarked {
            1.0
        } else {
            MARKER_CURRENT_OPACITY
        };
        return (theme.text, opacity);
    }
    let opacity = if bookmarked { 1.0 } else { MARKER_REST_OPACITY };
    (theme.navigation_rail_marker, opacity)
}

/// Window-space top and bottom of a measured transcript row. The list prints
/// content offset `o` at its top inset, so rows above its scroll anchor still
/// have a place: the browser paints them in that inset, and the reference's
/// observers see them there.
pub(super) fn row_span(list: &ListState, row: usize) -> Option<(Pixels, Pixels)> {
    let top = list.item_offset(row)?;
    let bottom = list.item_offset(row + 1)?;
    let origin = list.viewport_bounds().top() + px(CONVERSATION_TOP_INSET) - list.scroll_offset();
    Some((origin + top, origin + bottom))
}

/// Whether the turn spanning `first..=last` overlaps the viewport between
/// `top` and `bottom`. Rows the list has not measured fall back to what the
/// list knows about their side of the viewport.
fn turn_intersects(
    list: &ListState,
    first: usize,
    last: usize,
    top: Pixels,
    bottom: Pixels,
) -> bool {
    if let (Some((first_top, _)), Some((_, last_bottom))) =
        (row_span(list, first), row_span(list, last))
    {
        return last_bottom > top && first_top < bottom;
    }
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

/// Which turns intersect the transcript between `top` and `bottom`.
pub(super) fn turns_in_view(
    list: &ListState,
    items: &[UserMessageNavigationItem],
    top: Pixels,
    bottom: Pixels,
) -> Vec<bool> {
    items
        .iter()
        .map(|item| turn_intersects(list, item.row_index, item.turn_end_row, top, bottom))
        .collect()
}

/// `aria-current`: every turn from the first to the last one that intersects
/// the viewport (the reference's `findIndex` .. `findLastIndex`), or `None`
/// when nothing does, in which case the reference keeps its previous set.
pub(super) fn current_span(flags: &[bool]) -> Option<Vec<bool>> {
    let first = flags.iter().position(|visible| *visible)?;
    let last = flags.iter().rposition(|visible| *visible)?;
    Some(
        (0..flags.len())
            .map(|index| index >= first && index <= last)
            .collect(),
    )
}

pub(super) fn rail_visible(items: usize, pane_width: f32) -> bool {
    items >= RAIL_MINIMUM_ITEMS && pane_width >= RAIL_MINIMUM_PANE_WIDTH
}

pub(super) fn rail_max_height(pane_height: f32) -> f32 {
    (pane_height * RAIL_MAX_HEIGHT_RATIO).min(RAIL_MAX_HEIGHT)
}

/// The rail list's height: every marker until `max-h-[min(70vh,40rem)]`.
pub(super) fn rail_height(items: usize, pane_height: f32) -> f32 {
    (items as f32 * RAIL_ITEM_HEIGHT).min(rail_max_height(pane_height))
}

/// The card's height as `user_message_card` lays it out, so it can be placed
/// before it is painted.
pub(super) fn card_height(preview: &preview::Preview) -> f32 {
    let preview_height = if preview.is_empty() {
        0.0
    } else {
        CARD_PREVIEW_GAP + preview.height
    };
    CARD_PADDING * 2.0 + CARD_HEADER_HEIGHT + preview_height
}

/// Byte index where the trailing ASCII word run starts, so a clamped line never
/// ends in the middle of a Latin word.
pub(super) fn ascii_run_start(text: &str) -> usize {
    for (index, character) in text.char_indices().rev() {
        let in_word =
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | '+');
        if !in_word {
            return index + character.len_utf8();
        }
    }
    0
}

/// One frame of the rail, resolved by `HomeView::user_message_rail_render`.
pub(super) struct RailRender {
    pub(super) items: Rc<Vec<UserMessageNavigationItem>>,
    pub(super) current: Rc<Vec<bool>>,
    /// Each marker's animated `--marker-progress`.
    pub(super) progress: Rc<Vec<f32>>,
    /// The hovered or scrubbed marker.
    pub(super) focus: Option<usize>,
    /// The pointer is on the rail or scrubbing.
    pub(super) muted: bool,
    pub(super) bookmarks: Rc<Vec<bool>>,
    pub(super) card: Option<RailCard>,
    /// The mount fade-in.
    pub(super) opacity: f32,
    pub(super) scroll: ScrollHandle,
    pub(super) frame: Rc<RefCell<RailFrame>>,
    /// Top of the rail list, relative to the conversation view.
    pub(super) top: f32,
    pub(super) height: f32,
    pub(super) scroll_top: f32,
    /// `--top-fade` and `--bottom-fade` of the list's edge mask.
    pub(super) fades: (f32, f32),
}

pub(super) struct RailCard {
    pub(super) index: usize,
    /// Top of the card, relative to the conversation view.
    pub(super) top: f32,
    /// `max-height: calc(100vh - 16px)`.
    pub(super) max_height: f32,
    pub(super) bookmarked: bool,
    pub(super) preview: Rc<preview::Preview>,
}

fn marker_button(
    index: usize,
    render: &RailRender,
    theme: Theme,
    home: Entity<HomeView>,
) -> impl IntoElement {
    let progress = render.progress.get(index).copied().unwrap_or(0.0);
    let is_bookmarked = render.bookmarks.get(index).copied().unwrap_or(false);
    let (colour, opacity) = marker_paint(
        theme,
        render.current.get(index).copied().unwrap_or(false),
        render.focus == Some(index),
        render.muted,
        is_bookmarked,
    );
    // The list's `vertical-scroll-fade-mask`, sampled on the marker's line.
    let line_centre = index as f32 * RAIL_ITEM_HEIGHT + RAIL_ITEM_HEIGHT * 0.5 - render.scroll_top;
    let opacity = opacity * motion::rail_mask_alpha(line_centre, render.height, render.fades);
    let dash_width = marker_dash_width(progress);
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
        // Pointer presses, including the ones accessibility clicks synthesize,
        // go through the rail's press/scrub handling; only a keyboard click
        // arrives here.
        .on_click(move |event, window, cx| {
            if matches!(event, ClickEvent::Keyboard(_)) {
                home.update(cx, |home, cx| {
                    home.activate_user_message_marker(index, window, cx)
                });
            }
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
                                    .left(px(BOOKMARK_DOT_LEFT + dash_width))
                                    .top(px(0.0))
                                    .size(px(BOOKMARK_DOT_SIZE))
                                    .rounded(px(BOOKMARK_DOT_SIZE / 2.0))
                                    .bg(colour)
                                    .opacity(opacity),
                            )
                        }),
                ),
        )
}

/// Registers the window-level pointer and key listeners that drive the rail:
/// the reference tracks `:hover`, pointer capture while scrubbing, and the
/// safe triangle from document-level events, so these see every event.
fn register_rail_listeners(
    frame: Rc<RefCell<RailFrame>>,
    home: Entity<HomeView>,
    window: &mut Window,
) {
    fn pointer(
        frame: &RefCell<RailFrame>,
        home: &Entity<HomeView>,
        position: Point<Pixels>,
        pressed: bool,
        inside_window: bool,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let (over_rail, over_card) = {
            let frame = frame.borrow();
            let hovered = |hitbox: &Option<gpui::Hitbox>| {
                inside_window
                    && hitbox
                        .as_ref()
                        .is_some_and(|hitbox| hitbox.is_hovered(window))
            };
            (hovered(&frame.rail), hovered(&frame.card))
        };
        home.update(cx, |home, cx| {
            home.rail_pointer_moved(position, pressed, over_rail, over_card, window, cx)
        });
    }

    window.on_mouse_event({
        let (frame, home) = (frame.clone(), home.clone());
        move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture {
                let pressed = event.pressed_button == Some(MouseButton::Left);
                pointer(&frame, &home, event.position, pressed, true, window, cx);
            }
        }
    });
    window.on_mouse_event({
        let (frame, home) = (frame.clone(), home.clone());
        move |event: &MouseExitEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture {
                let pressed = event.pressed_button == Some(MouseButton::Left);
                pointer(&frame, &home, event.position, pressed, false, window, cx);
            }
        }
    });
    window.on_mouse_event({
        let (frame, home) = (frame.clone(), home.clone());
        move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                return;
            }
            let over_rail = frame
                .borrow()
                .rail
                .as_ref()
                .is_some_and(|hitbox| hitbox.is_hovered(window));
            home.update(cx, |home, cx| {
                home.rail_pointer_released(event.position, over_rail, window, cx)
            });
        }
    });
    window.on_mouse_event({
        let (frame, home) = (frame.clone(), home.clone());
        move |_: &ScrollWheelEvent, phase, window, _cx| {
            if phase != DispatchPhase::Capture {
                return;
            }
            // Scrolling the rail moves other markers under a still pointer;
            // re-read hover once the new offset is painted.
            let (frame, home) = (frame.clone(), home.clone());
            window.on_next_frame(move |window, cx| {
                let position = window.mouse_position();
                pointer(&frame, &home, position, false, true, window, cx);
            });
        }
    });
    window.on_key_event(move |event: &KeyDownEvent, phase, _window, cx| {
        if phase == DispatchPhase::Capture && event.keystroke.key == "escape" {
            home.update(cx, |home, cx| home.dismiss_user_message_card(cx));
        }
    });
}

/// The rail plus its hover card, positioned in the conversation view. The
/// rail sits 16 px from the transcript pane's left edge, centred in the
/// window, and floats above the transcript: it takes the pointer and the
/// wheel from whatever it covers, as the reference's portalled rail does.
pub(super) fn user_message_navigation_overlay(
    render: RailRender,
    theme: Theme,
    home: Entity<HomeView>,
) -> AnyElement {
    let mut markers = div().flex().flex_col();
    for index in 0..render.items.len() {
        markers = markers.child(marker_button(index, &render, theme, home.clone()));
    }
    let press_home = home.clone();
    let rail_frame = render.frame.clone();
    let listener_frame = render.frame.clone();
    let listener_home = home.clone();
    let rail = div()
        .absolute()
        .left(px(RAIL_LEFT_IN_VIEW))
        .top(px(render.top))
        .w(px(RAIL_WIDTH))
        .h(px(render.height))
        .opacity(render.opacity)
        .child(
            div()
                .id("user-message-navigation-rail")
                .size_full()
                .occlude()
                .overflow_y_scroll()
                .scrollbar_width(px(0.0))
                .track_scroll(&render.scroll)
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    cx.stop_propagation();
                    press_home.update(cx, |home, cx| {
                        home.rail_pointer_pressed(event.position, window, cx)
                    });
                })
                .child(markers),
        )
        .child(
            canvas(
                move |bounds, window, _| {
                    let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                    let mut frame = rail_frame.borrow_mut();
                    frame.rail = Some(hitbox);
                    // The card, if any, is laid out after the rail.
                    frame.card = None;
                },
                move |_, _, window, _| {
                    register_rail_listeners(listener_frame, listener_home, window)
                },
            )
            .absolute()
            .inset_0(),
        );

    let mut overlay = div().absolute().top_0().left_0().child(rail);
    if let Some(card) = render.card
        && let Some(item) = render.items.get(card.index)
    {
        overlay = overlay.child(
            div()
                .absolute()
                .left(px(CARD_LEFT - CHAT_CONTENT_HORIZONTAL_GUTTER))
                .top(px(card.top))
                .w(px(CARD_WIDTH))
                .child(user_message_card(
                    item,
                    &card,
                    theme,
                    render.frame.clone(),
                    home,
                )),
        );
    }
    overlay.into_any_element()
}

/// The hover card: prompt row, bookmark control, then the response preview.
pub(super) fn user_message_card(
    item: &UserMessageNavigationItem,
    card: &RailCard,
    theme: Theme,
    frame: Rc<RefCell<RailFrame>>,
    home: Entity<HomeView>,
) -> AnyElement {
    let index = card.index;
    let bookmarked = card.bookmarked;
    let preview = card.preview.as_ref();
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

    let text_color: gpui::Hsla = theme.navigation_rail_marker.into();
    let preview_column = preview.rows.iter().fold(
        div()
            .w_full()
            .h(px(preview.height))
            .overflow_hidden()
            .flex()
            .flex_col(),
        |column, row| {
            column.child(
                div()
                    .relative()
                    .w_full()
                    .flex_none()
                    .mt(px(row.margin_top))
                    .pl(px(row.indent))
                    .text_size(px(row.font_size))
                    .line_height(px(row.line_height))
                    .font(hover_card_font(row.weight))
                    .text_color(text_color)
                    .line_clamp(row.lines)
                    .when_some(row.marker.clone(), |line, marker| {
                        // `list-style-position: outside`: the marker ends one
                        // space before the item's text.
                        line.child(
                            div()
                                .absolute()
                                .top_0()
                                .left(px(row.indent - LIST_MARKER_BOX))
                                .w(px(LIST_MARKER_BOX - LIST_MARKER_GAP))
                                .flex()
                                .justify_end()
                                .child(marker),
                        )
                    })
                    .child(
                        gpui::StyledText::new(row.text.clone())
                            .with_runs(preview::row_text_runs(row, text_color)),
                    ),
            )
        },
    );

    div()
        .id(("user-message-navigation-card", index))
        .relative()
        .w(px(CARD_WIDTH))
        .max_h(px(card.max_height))
        .p(px(CARD_PADDING))
        .flex()
        .flex_col()
        .occlude()
        .rounded(px(CARD_RADIUS))
        .bg(theme.navigation_rail_surface)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(0.0), theme.border.into()).spread_radius(px(CARD_RING)),
            BoxShadow::new(px(0.0), px(8.0), theme.profile_menu_shadow.into())
                .blur_radius(px(16.0))
                .spread_radius(px(-4.0)),
        ])
        .overflow_hidden()
        .child(header)
        .when(!preview.is_empty(), |card| {
            card.child(div().mt(px(CARD_PREVIEW_GAP)).child(preview_column))
        })
        .child(
            canvas(
                move |bounds, window, _| {
                    let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                    frame.borrow_mut().card = Some(hitbox);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
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
            // (current, focused, muted, bookmarked)
            let resting = marker_paint(theme, false, false, false, false);
            assert_eq!(resting.0, theme.navigation_rail_marker);
            assert_eq!(resting.1, 0.4);

            let current = marker_paint(theme, true, false, false, false);
            assert_eq!(current.0, theme.text);
            assert_eq!(current.1, 0.6);

            let hovered = marker_paint(theme, false, true, true, false);
            assert_eq!(hovered.0, theme.text);
            assert_eq!(hovered.1, 1.0);

            // While the pointer rests on the rail or a scrub runs, a current
            // marker that is not the focused one falls back to the resting
            // colour.
            let muted = marker_paint(theme, true, false, true, false);
            assert_eq!(muted.0, theme.navigation_rail_marker);
            assert_eq!(muted.1, 0.4);

            // A bookmark lifts either resting opacity to one.
            assert_eq!(marker_paint(theme, false, false, false, true).1, 1.0);
            assert_eq!(marker_paint(theme, true, false, false, true).1, 1.0);
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
        assert_eq!(rail_height(14, 1000.0), 140.0);
        assert_eq!(rail_height(80, 1000.0), 640.0);
    }

    /// The reference marks every turn from the first to the last one that
    /// intersects the viewport, and keeps its set while none does.
    #[test]
    fn current_markers_span_the_first_to_the_last_visible_turn() {
        assert_eq!(current_span(&[false, false]), None);
        assert_eq!(
            current_span(&[true, true, false, true]),
            Some(vec![true, true, true, true])
        );
        assert_eq!(
            current_span(&[false, true, true, false]),
            Some(vec![false, true, true, false])
        );
    }

    /// A steering message shares its turn: the turn's first prompt is observed
    /// through the whole turn, the steering prompt through its own row.
    #[test]
    fn steering_prompts_share_the_turn_they_steered() {
        use super::super::timeline::{
            ActivityStreamUnit, ConversationListRow, user_message_navigation_items,
        };
        use crate::conversation::ConversationActivity;
        let user = |turn_index: usize| ConversationListRow::HistoricalUser {
            turn_index,
            message: format!("prompt {turn_index}"),
            images: Vec::new(),
            time: None,
        };
        let answer = |id: &str| ConversationListRow::AssistantMarkdown {
            id: id.to_owned(),
            text: id.to_owned(),
        };
        let steering = ConversationListRow::Activity {
            unit: ActivityStreamUnit::Standalone(ConversationActivity::UserMessage {
                item_id: "steer".to_owned(),
                text: "steer".to_owned(),
                images: Vec::new(),
            }),
            show_thinking_tail: false,
        };
        let rows = vec![
            user(0),
            answer("a"),
            steering,
            answer("b"),
            user(1),
            answer("c"),
        ];
        let assistant = |id: &str| ConversationActivity::AssistantMessage {
            item_id: id.to_owned(),
            text: id.to_owned(),
        };
        let turn = |activities: Vec<ConversationActivity>| {
            crate::conversation::ConversationTranscriptTurn {
                turn_id: None,
                phase: crate::conversation::ConversationPhase::Complete,
                user_message: String::new(),
                user_images: Vec::new(),
                user_message_time: None,
                assistant_message: "final answer".to_owned(),
                assistant_message_time: None,
                activities,
                resumed: None,
            }
        };
        let transcript = vec![
            turn(vec![
                assistant("commentary before the steer"),
                ConversationActivity::UserMessage {
                    item_id: "steer".to_owned(),
                    text: "steer".to_owned(),
                    images: Vec::new(),
                },
                assistant("progress"),
                assistant("answer after the steer"),
            ]),
            turn(Vec::new()),
        ];
        let items = user_message_navigation_items(&rows, &transcript, (&[], ""));
        let spans = items
            .iter()
            .map(|item| (item.row_index, item.turn_end_row, item.turn))
            .collect::<Vec<_>>();
        assert_eq!(spans, vec![(0, 3, 0), (2, 2, 0), (4, 5, 1)]);
        // Each prompt previews the last assistant message it received before
        // the next prompt; a turn without assistant activity falls back to
        // its answer.
        let previews = items
            .iter()
            .map(|item| item.preview.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            previews,
            vec![
                "commentary before the steer",
                "answer after the steer",
                "final answer"
            ]
        );
    }

    #[test]
    fn card_height_matches_the_card_layout() {
        assert_eq!(card_height(&preview::Preview::default()), 36.0);
        // A 76 px preview (one line, a 13 px gap, two lines): the recorded
        // 116 px card.
        let preview = preview::Preview {
            rows: Vec::new(),
            height: 76.0,
        };
        assert_eq!(card_height(&preview), 36.0);
        let row = preview::PreviewRow {
            text: "a".into(),
            runs: Vec::new(),
            lines: 1,
            margin_top: 0.0,
            font_size: CARD_PREVIEW_SIZE,
            line_height: CARD_PREVIEW_LINE_HEIGHT,
            weight: crate::theme::UI_BODY_FONT_WEIGHT,
            indent: 0.0,
            marker: None,
        };
        let preview = preview::Preview {
            rows: vec![row],
            height: 76.0,
        };
        assert_eq!(card_height(&preview), 116.0);
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
