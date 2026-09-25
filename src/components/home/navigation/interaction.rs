//! How the user-message rail answers the pointer, and the transcript jumps it
//! drives.
//!
//! This follows the reference's rail component and the interactive tooltip it
//! wraps. The marker taper follows `:hover` at once and eases over 160 ms. The
//! card opens 250 ms after the pointer enters the rail (at once within 300 ms
//! of closing), moves to whichever marker is hovered, and closes 100 ms after
//! the pointer leaves both the rail and the card unless it keeps travelling
//! through the safe triangle between them. Pressing a marker opens the card;
//! dragging along the rail scrubs the transcript instantly, and a plain click
//! scrolls smoothly to a mounted turn. Every jump flashes the prompt's bubble.

use std::{cell::RefCell, rc::Rc, time::Instant};

use gpui::{Bounds, Context, Hitbox, ListOffset, Pixels, Point, ScrollHandle, Window, point, px};

use super::{
    RAIL_ITEM_HEIGHT, RailCard, RailRender, card_height, current_span, marker_progress,
    motion::{
        CARD_CLOSE_GRACE, CARD_OPEN_DELAY, CARD_SKIP_DELAY, JUMP_HIGHLIGHT, JUMP_SCROLL_MARGIN,
        MarkerTransition, RAIL_FADE_IN, SafeTriangle, TRANSCRIPT_OVERSCAN_TURNS, TranscriptScroll,
        card_top, ease_enter_snappy, jump_highlight_mix, jump_highlight_overlay, rail_edge_fades,
        rail_follow_scroll_top,
    },
    rail_height, row_span, turns_in_view,
};
use crate::components::home::{CONVERSATION_TOP_INSET, HomeView};

/// The reference's `IntersectionObserver` crops the transcript's top 16 px
/// before a turn counts as current.
const CURRENT_TOP_CROP: f32 = 16.0;

/// Hit targets of the last painted frame, shared with the window-level
/// pointer listeners the overlay registers.
#[derive(Default)]
pub(in crate::components::home) struct RailFrame {
    pub(super) rail: Option<Hitbox>,
    pub(super) card: Option<Hitbox>,
}

impl RailFrame {
    fn rail_bounds(&self) -> Option<Bounds<Pixels>> {
        self.rail.as_ref().map(|hitbox| hitbox.bounds)
    }

    fn card_bounds(&self) -> Option<Bounds<Pixels>> {
        self.card.as_ref().map(|hitbox| hitbox.bounds)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RailScrub {
    pressed: usize,
    target: usize,
    moved: bool,
}

#[derive(Clone, Copy, Debug)]
struct CloseGrace {
    generation: u64,
    point: Point<Pixels>,
    triangle: Option<SafeTriangle>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::components::home) enum JumpMotion {
    /// A marker click: smooth for a turn the reference keeps mounted.
    Smooth,
    /// A scrub step or a capture jump.
    Instant,
    /// Alt+↑/↓: smooth like a click, without the bubble highlight.
    Keyboard,
}

/// Alt+↑/↓ steps through the prompts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::components::home) enum MessageStep {
    Previous,
    Next,
}

/// Alt+↑/↓ treats a prompt within 24 px of the transcript's top as the one
/// it is on, and checks again 350 ms after scrolling.
const KEYBOARD_STEP_TOLERANCE: f32 = 24.0;
const KEYBOARD_STEP_CORRECTION: std::time::Duration = std::time::Duration::from_millis(350);

pub(in crate::components::home) struct UserMessageRail {
    /// The pointer is over the rail list (`[data-floating-navigation-rail-list]:hover`).
    rail_hovered: bool,
    /// The marker under the pointer (`button:hover`).
    hovered: Option<usize>,
    card_hovered: bool,
    /// The marker the card describes and is positioned against.
    anchor: Option<usize>,
    card_open: bool,
    scrub: Option<RailScrub>,
    open_pending: Option<u64>,
    grace: Option<CloseGrace>,
    generation: u64,
    last_closed_at: Option<Instant>,
    markers: Vec<MarkerTransition>,
    /// `aria-current`: kept while no turn intersects the viewport.
    current: Vec<bool>,
    /// The first current marker and rail height the list last scrolled for.
    followed: Option<(usize, i32)>,
    scroll: ScrollHandle,
    /// A running smooth scroll and the offset it last applied.
    transcript_scroll: Option<(TranscriptScroll, f32)>,
    highlights: Vec<(usize, Instant)>,
    visible: bool,
    shown_at: Option<Instant>,
    /// The window's activation last frame, to see it blur.
    window_active: bool,
    /// The Alt+↑/↓ step whose landing is checked again after 350 ms.
    keyboard_step: Option<u64>,
    frame_scheduled: bool,
    frame: Rc<RefCell<RailFrame>>,
    /// Capture-only: render the card and taper settled, as if hovered.
    pinned_for_capture: bool,
    /// Capture-only: what the last render resolved, for the motion replay.
    #[cfg(feature = "screenshot")]
    rendered: Option<replay::RenderedRail>,
}

impl Default for UserMessageRail {
    fn default() -> Self {
        Self {
            rail_hovered: false,
            hovered: None,
            card_hovered: false,
            anchor: None,
            card_open: false,
            scrub: None,
            open_pending: None,
            grace: None,
            generation: 0,
            last_closed_at: None,
            markers: Vec::new(),
            current: Vec::new(),
            followed: None,
            scroll: ScrollHandle::new(),
            transcript_scroll: None,
            highlights: Vec::new(),
            visible: false,
            shown_at: None,
            window_active: false,
            keyboard_step: None,
            frame_scheduled: false,
            frame: Rc::new(RefCell::new(RailFrame::default())),
            pinned_for_capture: false,
            #[cfg(feature = "screenshot")]
            rendered: None,
        }
    }
}

impl UserMessageRail {
    /// The marker the taper centres on: the scrub target, else the hovered one.
    fn focus(&self) -> Option<usize> {
        self.scrub
            .map(|scrub| scrub.target)
            .or(self.rail_hovered.then_some(self.hovered).flatten())
    }

    /// While the pointer is on the rail or scrubbing, the current markers fall
    /// back to the resting colour.
    fn muted(&self) -> bool {
        self.scrub.is_some() || self.rail_hovered
    }

    fn animating(&self, now: Instant) -> bool {
        self.markers.iter().any(|marker| marker.is_running(now))
            || self.transcript_scroll.is_some()
            || !self.highlights.is_empty()
            || self
                .shown_at
                .is_some_and(|shown| now.saturating_duration_since(shown) < RAIL_FADE_IN)
    }

    fn close_card(&mut self, now: Instant) {
        if self.card_open {
            self.last_closed_at = Some(now);
        }
        self.card_open = false;
        self.card_hovered = false;
        self.open_pending = None;
        self.grace = None;
        if self.scrub.is_none() {
            self.anchor = None;
        }
    }

    /// A different task or a rail that left the screen starts over.
    fn reset(&mut self, items: usize) {
        let frame = self.frame.clone();
        let scroll = self.scroll.clone();
        *self = Self {
            frame,
            scroll,
            ..Self::default()
        };
        self.markers = vec![MarkerTransition::settled(0.0); items];
        self.current = last_only(items);
        self.scroll.set_offset(point(px(0.0), px(0.0)));
    }
}

fn last_only(items: usize) -> Vec<bool> {
    (0..items).map(|index| index + 1 == items).collect()
}

impl HomeView {
    fn rail_now(&self, cx: &Context<Self>) -> Instant {
        cx.background_executor().now()
    }

    /// Clears the rail for a newly opened task.
    pub(in crate::components::home) fn reset_user_message_rail(&mut self) {
        let items = self.user_message_navigation.len();
        self.navigation_rail.reset(items);
    }

    fn rail_scroll_top(&self) -> f32 {
        -f32::from(self.navigation_rail.scroll.offset().y)
    }

    /// The marker at window-space `y`; a scrub clamps `y` to the rail first,
    /// the way the reference probes `elementFromPoint` at the list's centre.
    fn rail_marker_at(&self, y: Pixels, clamp: bool) -> Option<usize> {
        let bounds = self.navigation_rail.frame.borrow().rail_bounds()?;
        let mut y = f32::from(y);
        let (top, bottom) = (f32::from(bounds.top()), f32::from(bounds.bottom()));
        if clamp {
            y = y.clamp(top, (bottom - 1.0).max(top));
        } else if y < top || y >= bottom {
            return None;
        }
        let index = ((y - top + self.rail_scroll_top()) / RAIL_ITEM_HEIGHT).floor();
        let count = self.user_message_navigation.len();
        (index >= 0.0 && (index as usize) < count).then_some(index as usize)
    }

    fn retarget_rail_markers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.rail_now(cx);
        let count = self.user_message_navigation.len();
        let rail = &mut self.navigation_rail;
        rail.markers.resize(count, MarkerTransition::settled(0.0));
        let focus = rail.focus();
        let animate = rail.scrub.is_none() && !rail.pinned_for_capture && !cx.reduce_motion();
        for (index, marker) in rail.markers.iter_mut().enumerate() {
            marker.retarget(marker_progress(index, focus), now, animate);
        }
        self.ensure_rail_frames(window, cx);
    }

    fn ensure_rail_frames(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.rail_now(cx);
        if self.navigation_rail.frame_scheduled || !self.navigation_rail.animating(now) {
            return;
        }
        self.navigation_rail.frame_scheduled = true;
        cx.on_next_frame(window, |home, window, cx| {
            home.advance_rail_frame(window, cx)
        });
    }

    fn advance_rail_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.navigation_rail.frame_scheduled = false;
        let now = self.rail_now(cx);
        if let Some((scroll, applied)) = self.navigation_rail.transcript_scroll {
            // A wheel or scrollbar gesture takes over from the jump, as it does
            // from Chromium's programmatic smooth scroll.
            if (self.transcript_offset() - applied).abs() > 0.5 {
                self.navigation_rail.transcript_scroll = None;
            } else {
                let applied = self.set_transcript_offset(scroll.offset(now));
                self.navigation_rail.transcript_scroll =
                    (!scroll.is_finished(now)).then_some((scroll, applied));
            }
        }
        self.navigation_rail
            .highlights
            .retain(|(_, started)| now.saturating_duration_since(*started) < JUMP_HIGHLIGHT);
        cx.notify();
        self.ensure_rail_frames(window, cx);
    }

    fn arm_rail_card(&mut self, now: Instant, cx: &mut Context<Self>) {
        if self.navigation_rail.card_open || self.navigation_rail.open_pending.is_some() {
            return;
        }
        let skip = self
            .navigation_rail
            .last_closed_at
            .is_some_and(|closed| now.saturating_duration_since(closed) < CARD_SKIP_DELAY);
        if skip {
            self.open_rail_card();
            return;
        }
        let rail = &mut self.navigation_rail;
        rail.generation = rail.generation.wrapping_add(1);
        let generation = rail.generation;
        rail.open_pending = Some(generation);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CARD_OPEN_DELAY).await;
            let _ = this.update(cx, |home, cx| {
                if home.navigation_rail.open_pending == Some(generation) {
                    home.open_rail_card();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn open_rail_card(&mut self) {
        let rail = &mut self.navigation_rail;
        rail.open_pending = None;
        rail.grace = None;
        rail.card_open = true;
        rail.anchor = rail.anchor.or(rail.hovered).or(Some(0));
    }

    /// The pointer left the rail or the card while the card is open.
    fn start_rail_grace(
        &mut self,
        point: Point<Pixels>,
        triangle: Option<SafeTriangle>,
        cx: &mut Context<Self>,
    ) {
        if !self.navigation_rail.card_open {
            return;
        }
        self.restart_rail_grace(point, triangle, cx);
    }

    fn restart_rail_grace(
        &mut self,
        point: Point<Pixels>,
        triangle: Option<SafeTriangle>,
        cx: &mut Context<Self>,
    ) {
        let rail = &mut self.navigation_rail;
        rail.generation = rail.generation.wrapping_add(1);
        let generation = rail.generation;
        rail.grace = Some(CloseGrace {
            generation,
            point,
            triangle,
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CARD_CLOSE_GRACE).await;
            let _ = this.update(cx, |home, cx| {
                let Some(grace) = home.navigation_rail.grace else {
                    return;
                };
                if grace.generation != generation {
                    return;
                }
                let frame = home.navigation_rail.frame.borrow();
                let inside = frame
                    .rail_bounds()
                    .is_some_and(|bounds| bounds.contains(&grace.point))
                    || frame
                        .card_bounds()
                        .is_some_and(|bounds| bounds.contains(&grace.point));
                drop(frame);
                if inside {
                    home.navigation_rail.grace = None;
                    return;
                }
                let now = home.rail_now(cx);
                home.navigation_rail.close_card(now);
                cx.notify();
            });
        })
        .detach();
    }

    /// Every pointer move in the window while the rail is on screen.
    pub(in crate::components::home) fn rail_pointer_moved(
        &mut self,
        position: Point<Pixels>,
        pressed: bool,
        over_rail: bool,
        over_card: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.navigation_rail.pinned_for_capture {
            return;
        }
        let now = self.rail_now(cx);
        if let Some(mut scrub) = self.navigation_rail.scrub {
            if !pressed {
                self.rail_pointer_released(position, over_rail, window, cx);
                return;
            }
            if let Some(target) = self.rail_marker_at(position.y, true)
                && target != scrub.target
            {
                scrub.target = target;
                scrub.moved = true;
                self.navigation_rail.scrub = Some(scrub);
                self.navigation_rail.anchor = Some(target);
                self.navigation_rail.hovered = Some(target);
                self.jump_to_user_message(target, JumpMotion::Instant, cx);
                self.retarget_rail_markers(window, cx);
                cx.notify();
            }
            return;
        }

        let before = (
            self.navigation_rail.rail_hovered,
            self.navigation_rail.hovered,
            self.navigation_rail.card_hovered,
            self.navigation_rail.anchor,
            self.navigation_rail.card_open,
        );
        let mut crossed = false;
        if over_rail {
            let marker = self.rail_marker_at(position.y, false);
            let rail = &mut self.navigation_rail;
            if !rail.rail_hovered {
                rail.rail_hovered = true;
                rail.grace = None;
                crossed = true;
            }
            rail.hovered = marker;
            if marker.is_some() {
                rail.anchor = marker;
            }
            self.arm_rail_card(now, cx);
        } else if self.navigation_rail.rail_hovered {
            let rail = &mut self.navigation_rail;
            rail.rail_hovered = false;
            rail.hovered = None;
            rail.open_pending = None;
            crossed = true;
            let triangle = rail
                .frame
                .borrow()
                .card_bounds()
                .and_then(|card| SafeTriangle::toward(position, card, true));
            self.start_rail_grace(position, triangle, cx);
        }

        if over_card {
            if !self.navigation_rail.card_hovered {
                self.navigation_rail.card_hovered = true;
                self.navigation_rail.grace = None;
                crossed = true;
            }
        } else if self.navigation_rail.card_hovered {
            self.navigation_rail.card_hovered = false;
            crossed = true;
            let triangle = self
                .navigation_rail
                .frame
                .borrow()
                .rail_bounds()
                .and_then(|rail| SafeTriangle::toward(position, rail, false));
            self.start_rail_grace(position, triangle, cx);
        }

        if !crossed
            && !over_rail
            && !over_card
            && let Some(grace) = self.navigation_rail.grace
            && let Some(triangle) = grace.triangle
        {
            if triangle.contains(position) {
                self.restart_rail_grace(position, Some(triangle), cx);
            } else {
                self.navigation_rail.close_card(now);
            }
        }

        let after = (
            self.navigation_rail.rail_hovered,
            self.navigation_rail.hovered,
            self.navigation_rail.card_hovered,
            self.navigation_rail.anchor,
            self.navigation_rail.card_open,
        );
        if before != after {
            self.retarget_rail_markers(window, cx);
            cx.notify();
        }
    }

    /// A left press on the rail: the card opens at once and a scrub begins.
    pub(in crate::components::home) fn rail_pointer_pressed(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.rail_marker_at(position.y, false) else {
            return;
        };
        self.navigation_rail.pinned_for_capture = false;
        let rail = &mut self.navigation_rail;
        rail.open_pending = None;
        rail.grace = None;
        rail.rail_hovered = true;
        rail.hovered = Some(index);
        rail.scrub = Some(RailScrub {
            pressed: index,
            target: index,
            moved: false,
        });
        rail.anchor = Some(index);
        rail.card_open = true;
        self.retarget_rail_markers(window, cx);
        cx.notify();
    }

    /// The press ends. Without a scrub it is the reference's marker click.
    pub(in crate::components::home) fn rail_pointer_released(
        &mut self,
        position: Point<Pixels>,
        over_rail: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(scrub) = self.navigation_rail.scrub.take() else {
            return;
        };
        // The list scrolls to keep the first current marker in view again.
        self.navigation_rail.followed = None;
        if over_rail {
            self.navigation_rail.hovered = self.rail_marker_at(position.y, false);
        } else {
            // The list's pointerleave follows the release of its capture.
            self.navigation_rail.rail_hovered = false;
            self.navigation_rail.hovered = None;
            let triangle = self
                .navigation_rail
                .frame
                .borrow()
                .card_bounds()
                .and_then(|card| SafeTriangle::toward(position, card, true));
            self.start_rail_grace(position, triangle, cx);
        }
        if !scrub.moved {
            self.navigation_rail.anchor = Some(scrub.pressed);
            self.navigation_rail.card_open = true;
            self.jump_to_user_message(scrub.pressed, JumpMotion::Smooth, cx);
        }
        self.retarget_rail_markers(window, cx);
        cx.notify();
    }

    /// A keyboard activation of a marker: the card opens and the transcript
    /// scrolls to the message.
    pub(in crate::components::home) fn activate_user_message_marker(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation_rail.pinned_for_capture = false;
        self.navigation_rail.open_pending = None;
        self.navigation_rail.anchor = Some(index);
        self.navigation_rail.card_open = true;
        self.jump_to_user_message(index, JumpMotion::Smooth, cx);
        self.ensure_rail_frames(window, cx);
    }

    /// Escape dismisses the card, as the reference's tooltip provider does.
    pub(in crate::components::home) fn dismiss_user_message_card(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.navigation_rail.card_open {
            return;
        }
        let now = self.rail_now(cx);
        self.navigation_rail.close_card(now);
        cx.notify();
    }

    fn transcript_offset(&self) -> f32 {
        f32::from(self.conversation_list.scroll_offset())
    }

    /// Scrolls the transcript to a pixel offset and returns the offset the
    /// list actually took after clamping.
    fn set_transcript_offset(&mut self, offset: f32) -> f32 {
        self.conversation_list
            .set_offset_from_scrollbar(point(px(0.0), px(-offset.max(0.0))));
        self.transcript_offset()
    }

    /// The turns the reference keeps mounted: the visible ones and two more
    /// on either side.
    fn mounted_turns(&self) -> Option<(usize, usize)> {
        let items = &self.user_message_navigation;
        let viewport = self.conversation_list.viewport_bounds();
        let flags = turns_in_view(
            &self.conversation_list,
            items,
            viewport.top(),
            viewport.bottom(),
        );
        let mut visible = flags
            .iter()
            .zip(items.iter())
            .filter(|(visible, _)| **visible)
            .map(|(_, item)| item.turn);
        let first = visible.next()?;
        let last = visible.next_back().unwrap_or(first);
        Some((
            first.saturating_sub(TRANSCRIPT_OVERSCAN_TURNS),
            last + TRANSCRIPT_OVERSCAN_TURNS,
        ))
    }

    /// Only a mounted turn scrolls smoothly.
    fn user_message_is_mounted(&self, index: usize) -> bool {
        let Some((first, last)) = self.mounted_turns() else {
            return false;
        };
        self.user_message_navigation
            .get(index)
            .is_some_and(|item| item.turn >= first && item.turn <= last)
    }

    /// A turn's first prompt carries `scroll-mt-4` and lands 16 px below the
    /// top; a steering prompt inside the turn has none and lands at the top.
    fn user_message_scroll_margin(&self, index: usize) -> f32 {
        let items = &self.user_message_navigation;
        let first_of_turn = index == 0
            || items
                .get(index)
                .zip(items.get(index - 1))
                .is_none_or(|(item, previous)| item.turn != previous.turn);
        if first_of_turn {
            JUMP_SCROLL_MARGIN
        } else {
            0.0
        }
    }

    /// Scrolls the message to the top of the transcript
    /// (`scrollIntoView({ block: 'start' })`, honouring its scroll margin) and,
    /// for a rail jump, flashes its bubble. The frame loop that animates the
    /// scroll and the highlight starts on the next render, so this also runs
    /// where no window is at hand.
    pub(in crate::components::home) fn jump_to_user_message(
        &mut self,
        index: usize,
        motion: JumpMotion,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self
            .user_message_navigation
            .get(index)
            .map(|item| item.row_index)
        else {
            return;
        };
        let now = self.rail_now(cx);
        let landing = CONVERSATION_TOP_INSET - self.user_message_scroll_margin(index);
        let target = self
            .conversation_list
            .item_offset(row)
            .map(|offset| f32::from(offset) + landing);
        self.navigation_rail.transcript_scroll = None;
        let smooth = motion != JumpMotion::Instant
            && !cx.reduce_motion()
            && target.is_some()
            && self.user_message_is_mounted(index);
        match target {
            Some(target) if smooth => {
                let from = self.transcript_offset();
                // Resolve the clamped destination, then start from where the
                // list is now.
                let to = self.set_transcript_offset(target);
                let from = self.set_transcript_offset(from);
                if (to - from).abs() > 0.5 {
                    self.navigation_rail.transcript_scroll =
                        Some((TranscriptScroll::new(from, to, now), from));
                }
            }
            Some(target) => {
                self.set_transcript_offset(target);
            }
            None => self.conversation_list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(landing),
            }),
        }
        if motion != JumpMotion::Keyboard
            && !cx.reduce_motion()
            && !self.navigation_rail.pinned_for_capture
        {
            self.navigation_rail
                .highlights
                .retain(|(highlighted, _)| *highlighted != row);
            self.navigation_rail.highlights.push((row, now));
        }
        cx.notify();
    }

    /// The prompt Alt+↑/↓ moves to, following the reference: among the
    /// mounted prompts, "next" is the one after the last prompt that does not
    /// start below the top (plus 24 px); "previous" is the prompt before the
    /// one at the top, or the last one above it.
    fn user_message_step_target(&self, step: MessageStep) -> Option<usize> {
        let items = &self.user_message_navigation;
        if items.is_empty() {
            return None;
        }
        let top = f32::from(self.conversation_list.viewport_bounds().top());
        let mounted_turns = self.mounted_turns();
        let mounted = items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                mounted_turns.is_some_and(|(first, last)| item.turn >= first && item.turn <= last)
            })
            .filter_map(|(index, item)| {
                row_span(&self.conversation_list, item.row_index)
                    .map(|(row_top, _)| (index, f32::from(row_top)))
            })
            .collect::<Vec<_>>();
        let Some(&(first_mounted, _)) = mounted.first() else {
            return match step {
                MessageStep::Next => Some(0),
                MessageStep::Previous => Some(items.len() - 1),
            };
        };
        match step {
            MessageStep::Next => {
                let before = match mounted
                    .iter()
                    .position(|(_, row_top)| *row_top > top + KEYBOARD_STEP_TOLERANCE)
                {
                    Some(0) => None,
                    Some(position) => Some(mounted[position - 1].0),
                    None => mounted.last().map(|(index, _)| *index),
                };
                let next = before.map_or(0, |index| index + 1);
                (next < items.len()).then_some(next)
            }
            MessageStep::Previous => {
                for &(index, row_top) in mounted.iter().rev() {
                    if (row_top - top).abs() <= KEYBOARD_STEP_TOLERANCE {
                        return index.checked_sub(1);
                    }
                    if row_top < top {
                        return Some(index);
                    }
                }
                Some(first_mounted)
            }
        }
    }

    /// Alt+↑/↓: scroll to the previous or next prompt. Returns whether there
    /// was one to go to.
    pub(in crate::components::home) fn step_user_message(
        &mut self,
        step: MessageStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.user_message_step_target(step) else {
            return false;
        };
        self.jump_to_user_message(target, JumpMotion::Keyboard, cx);
        self.ensure_rail_frames(window, cx);
        // The reference checks the landing once more after 350 ms and scrolls
        // again if the prompt is still more than 24 px from the top.
        let rail = &mut self.navigation_rail;
        rail.generation = rail.generation.wrapping_add(1);
        let generation = rail.generation;
        rail.keyboard_step = Some(generation);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(KEYBOARD_STEP_CORRECTION)
                .await;
            let _ = this.update_in(cx, |home, window, cx| {
                if home.navigation_rail.keyboard_step != Some(generation) {
                    return;
                }
                home.navigation_rail.keyboard_step = None;
                let Some(row) = home
                    .user_message_navigation
                    .get(target)
                    .map(|item| item.row_index)
                else {
                    return;
                };
                let top = home.conversation_list.viewport_bounds().top();
                let off = row_span(&home.conversation_list, row).is_none_or(|(row_top, _)| {
                    f32::from(row_top - top).abs() > KEYBOARD_STEP_TOLERANCE
                });
                if off {
                    home.jump_to_user_message(target, JumpMotion::Keyboard, cx);
                    home.ensure_rail_frames(window, cx);
                }
            });
        })
        .detach();
        true
    }

    /// The highlight layer over one transcript row's bubble, if it is
    /// flashing after a jump.
    pub(in crate::components::home) fn user_message_highlight(
        &self,
        row: usize,
        cx: &gpui::App,
    ) -> Option<f32> {
        let now = cx.background_executor().now();
        let (_, started) = self
            .navigation_rail
            .highlights
            .iter()
            .find(|(highlighted, _)| *highlighted == row)?;
        jump_highlight_mix(now.saturating_duration_since(*started))
            .map(jump_highlight_overlay)
            .filter(|alpha| *alpha > 0.0)
    }

    /// Everything the overlay needs for this frame, or `None` while the rail
    /// is off screen.
    pub(in crate::components::home) fn user_message_rail_render(
        &mut self,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<RailRender> {
        let now = self.rail_now(cx);
        let count = self.user_message_navigation.len();
        if !visible {
            if self.navigation_rail.visible {
                self.navigation_rail.reset(count);
            }
            return None;
        }
        if !self.navigation_rail.visible {
            // The rail mounts: it fades in, and a capture's pinned hover
            // survives the fresh start.
            let pinned = self.navigation_rail.pinned_for_capture;
            let anchor = self.navigation_rail.anchor;
            self.navigation_rail.reset(count);
            self.navigation_rail.visible = true;
            if pinned {
                self.pin_user_message_rail_for_capture(anchor);
            } else if !cx.reduce_motion() {
                self.navigation_rail.shown_at = Some(now);
            }
        }
        if self.navigation_rail.markers.len() != count {
            // A message was sent or history arrived: the rail keeps its state.
            let rail = &mut self.navigation_rail;
            if rail.current.len() != count {
                rail.current = last_only(count);
            }
            for index in [&mut rail.anchor, &mut rail.hovered] {
                if index.is_some_and(|index| index >= count) {
                    *index = None;
                }
            }
            if rail.scrub.is_some_and(|scrub| scrub.target >= count) {
                rail.scrub = None;
            }
            self.retarget_rail_markers(window, cx);
        }
        // `closeOnWindowBlur`: the card closes when the window loses focus,
        // not merely because it is in the background.
        let active = window.is_window_active();
        if self.navigation_rail.window_active
            && !active
            && self.navigation_rail.card_open
            && !self.navigation_rail.pinned_for_capture
        {
            self.navigation_rail.close_card(now);
        }
        self.navigation_rail.window_active = active;

        let window_height = f32::from(window.viewport_size().height);
        let viewport = self.conversation_list.viewport_bounds();
        let flags = turns_in_view(
            &self.conversation_list,
            &self.user_message_navigation,
            viewport.top() + px(CURRENT_TOP_CROP),
            viewport.bottom(),
        );
        if let Some(current) = current_span(&flags) {
            self.navigation_rail.current = current;
        }

        let height = rail_height(count, window_height);
        let max_scroll = (count as f32 * RAIL_ITEM_HEIGHT - height).max(0.0);
        let first_current = self
            .navigation_rail
            .current
            .iter()
            .position(|current| *current)
            .unwrap_or(count.saturating_sub(1));
        let follow = (first_current, height.round() as i32);
        if self.navigation_rail.scrub.is_none() && self.navigation_rail.followed != Some(follow) {
            self.navigation_rail.followed = Some(follow);
            let scroll_top = self.rail_scroll_top();
            let followed = rail_follow_scroll_top(
                scroll_top,
                height,
                first_current as f32 * RAIL_ITEM_HEIGHT,
                RAIL_ITEM_HEIGHT,
            )
            .clamp(0.0, max_scroll);
            if (followed - scroll_top).abs() > f32::EPSILON {
                self.navigation_rail
                    .scroll
                    .set_offset(point(px(0.0), px(-followed)));
            }
        }
        let scroll_top = self.rail_scroll_top().clamp(0.0, max_scroll);

        let rail_top_window = (window_height - height) * 0.5;
        let card = self
            .navigation_rail
            .card_open
            .then_some(self.navigation_rail.anchor)
            .flatten()
            .and_then(|index| {
                let item = self.user_message_navigation.get(index)?;
                let preview = super::preview::layout_preview(
                    &item.preview,
                    super::CARD_WIDTH - super::CARD_PADDING * 2.0,
                    window,
                );
                let max_height = window_height - super::CARD_VIEWPORT_MARGIN;
                let height = card_height(&preview).min(max_height);
                let centre =
                    rail_top_window + index as f32 * RAIL_ITEM_HEIGHT + RAIL_ITEM_HEIGHT * 0.5
                        - scroll_top;
                Some(RailCard {
                    index,
                    top: card_top(centre, height, window_height) - self.content_top,
                    max_height,
                    bookmarked: self.user_message_navigation_bookmarked(index),
                    preview: Rc::new(preview),
                })
            });

        let opacity = self.navigation_rail.shown_at.map_or(1.0, |shown| {
            ease_enter_snappy(
                now.saturating_duration_since(shown).as_secs_f32() / RAIL_FADE_IN.as_secs_f32(),
            )
        });
        let progress: Vec<f32> = self
            .navigation_rail
            .markers
            .iter()
            .map(|marker| marker.value(now))
            .collect();
        let bookmarks: Vec<bool> = (0..count)
            .map(|index| self.user_message_navigation_bookmarked(index))
            .collect();
        self.ensure_rail_frames(window, cx);
        #[cfg(feature = "screenshot")]
        {
            self.navigation_rail.rendered = Some(replay::RenderedRail {
                at: std::time::Instant::now(),
                progress: progress.clone(),
                focus: self.navigation_rail.focus(),
                muted: self.navigation_rail.muted(),
                current: self.navigation_rail.current.clone(),
                bookmarks: bookmarks.clone(),
                scroll_top,
                opacity,
            });
        }
        Some(RailRender {
            items: self.user_message_navigation.clone(),
            current: Rc::new(self.navigation_rail.current.clone()),
            progress: Rc::new(progress),
            focus: self.navigation_rail.focus(),
            muted: self.navigation_rail.muted(),
            bookmarks: Rc::new(bookmarks),
            card,
            opacity,
            scroll: self.navigation_rail.scroll.clone(),
            frame: self.navigation_rail.frame.clone(),
            top: rail_top_window - self.content_top,
            height,
            scroll_top,
            fades: rail_edge_fades(scroll_top, max_scroll),
        })
    }

    /// Capture hook: whether a rail animation or jump is still running.
    #[cfg(feature = "screenshot")]
    pub(crate) fn user_message_rail_animating_for_capture(&self, cx: &gpui::App) -> bool {
        self.navigation_rail
            .animating(cx.background_executor().now())
    }

    /// Capture hook: hold one marker hovered with its card open, settled.
    pub(in crate::components::home) fn pin_user_message_rail_for_capture(
        &mut self,
        index: Option<usize>,
    ) {
        let rail = &mut self.navigation_rail;
        rail.pinned_for_capture = index.is_some();
        rail.rail_hovered = index.is_some();
        rail.hovered = index;
        rail.anchor = index;
        rail.card_open = index.is_some();
        rail.open_pending = None;
        rail.grace = None;
        let focus = rail.focus();
        for (position, marker) in rail.markers.iter_mut().enumerate() {
            *marker = MarkerTransition::settled(marker_progress(position, focus));
        }
    }

    /// Capture hook: jump the way a scrub step does, without the highlight
    /// or a card, so both builds record the same transcript.
    pub(in crate::components::home) fn jump_to_user_message_for_capture(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let pinned = self.navigation_rail.pinned_for_capture;
        self.navigation_rail.pinned_for_capture = true;
        self.jump_to_user_message(index, JumpMotion::Instant, cx);
        self.navigation_rail.pinned_for_capture = pinned;
    }
}

#[cfg(feature = "screenshot")]
#[path = "replay.rs"]
mod replay;

#[cfg(test)]
#[path = "interaction_tests.rs"]
mod tests;
