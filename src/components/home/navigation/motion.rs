//! Motion of the user-message rail, read from the reference build.
//!
//! The numbers come from the shipped stylesheet and scripts of the ChatGPT
//! app (`thread-user-message-navigation-rail-app`, `app-shared`) and were
//! confirmed frame by frame over CDP with
//! `scripts/cdp_probe_user_message_rail_motion.mjs`; the recordings live in
//! `artifacts/user-message-rail-motion/reference/`.

use std::time::{Duration, Instant};

use gpui::{Bounds, Pixels, Point};

use crate::components::home::animation::cubic_bezier_ease;

/// `transition-duration: .16s` on `_MarkerLine_` and `_BookmarkDot_`.
pub(super) const MARKER_TRANSITION: Duration = Duration::from_millis(160);

/// The marker's `transition-timing-function: linear(0, .398 10%, …, 1)`.
const MARKER_EASING: [(f32, f32); 10] = [
    (0.0, 0.0),
    (0.1, 0.398),
    (0.2, 0.682),
    (0.3, 0.843),
    (0.4, 0.925),
    (0.5, 0.972),
    (0.6, 1.004),
    (0.7, 1.008),
    (0.8, 1.003),
    (1.0, 1.0),
];

/// The tooltip's `delayOpen` delay, measured from entering the rail.
pub(super) const CARD_OPEN_DELAY: Duration = Duration::from_millis(250);
/// The tooltip provider's `skipDelayDuration`: re-entering the rail this soon
/// after the card closed opens it without the delay.
pub(super) const CARD_SKIP_DELAY: Duration = Duration::from_millis(300);
/// The interactive tooltip's close grace after the pointer leaves the rail
/// or the card.
pub(super) const CARD_CLOSE_GRACE: Duration = Duration::from_millis(100);
/// The safe triangle toward the card reaches this far past its edges.
const SAFE_TRIANGLE_OVERHANG: f32 = 8.0;
/// floating-ui's `shift({ padding: 8 })` keeps the card inside the window.
pub(super) const CARD_WINDOW_PADDING: f32 = 8.0;

/// `scroll-mt-4` on the transcript unit a marker jumps to.
pub(super) const JUMP_SCROLL_MARGIN: f32 = 16.0;
/// Chromium's delta-based programmatic smooth scroll lasts `sqrt(|Δ|)`
/// frames at 60 Hz, capped at 180 frames.
const SMOOTH_SCROLL_MAX_FRAMES: f32 = 180.0;
/// The reference renders the visible turns plus this many on either side;
/// jumping to a turn outside that window reveals it instantly.
pub(super) const TRANSCRIPT_OVERSCAN_TURNS: usize = 2;

/// The bubble highlight after a jump: `color-mix(text 14%)` held until 35%,
/// easing back to the bubble's own `color-mix(text 5%)` over 1400 ms.
pub(super) const JUMP_HIGHLIGHT: Duration = Duration::from_millis(1400);
const JUMP_HIGHLIGHT_PEAK: f32 = 0.14;
const JUMP_HIGHLIGHT_REST: f32 = 0.05;
const JUMP_HIGHLIGHT_HOLD: f32 = 0.35;

/// `[--edge-fade-distance:2.5rem]` on the rail list.
const RAIL_EDGE_FADE: f32 = 40.0;
/// The `edge-fade` keyframes hold their ends between 0–1% and 99–100%.
const RAIL_EDGE_FADE_HOLD: f32 = 0.01;

/// The rail's opacity animation when it mounts: 150 ms,
/// `cubic-bezier(.23, 1, .32, 1)`.
pub(super) const RAIL_FADE_IN: Duration = Duration::from_millis(150);

pub(super) fn marker_ease(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    for pair in MARKER_EASING.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if progress <= end.0 {
            let span = end.0 - start.0;
            return start.1 + (end.1 - start.1) * (progress - start.0) / span;
        }
    }
    1.0
}

pub(super) fn ease_enter_snappy(progress: f32) -> f32 {
    cubic_bezier_ease(progress, 0.23, 1.0, 0.32, 1.0)
}

/// One marker's `--marker-progress` transition, retargeted the way CSS
/// Transitions Level 1 does: a change back toward where a running transition
/// started is shortened by how far that transition had come.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MarkerTransition {
    start: f32,
    end: f32,
    reversing_adjusted_start: f32,
    shortening_factor: f32,
    started_at: Option<Instant>,
    duration: Duration,
}

impl MarkerTransition {
    pub(super) fn settled(value: f32) -> Self {
        Self {
            start: value,
            end: value,
            reversing_adjusted_start: value,
            shortening_factor: 1.0,
            started_at: None,
            duration: Duration::ZERO,
        }
    }

    fn eased_progress(&self, now: Instant) -> Option<f32> {
        let started_at = self.started_at?;
        if self.duration.is_zero() {
            return None;
        }
        let elapsed = now.saturating_duration_since(started_at);
        if elapsed >= self.duration {
            return None;
        }
        Some(marker_ease(
            elapsed.as_secs_f32() / self.duration.as_secs_f32(),
        ))
    }

    pub(super) fn value(&self, now: Instant) -> f32 {
        match self.eased_progress(now) {
            Some(eased) => self.start + (self.end - self.start) * eased,
            None => self.end,
        }
    }

    pub(super) fn is_running(&self, now: Instant) -> bool {
        self.eased_progress(now).is_some()
    }

    /// Point the marker at `end`. `animate` is false while scrubbing (the
    /// reference sets `transition-duration: 0s` then) and under reduced motion.
    pub(super) fn retarget(&mut self, end: f32, now: Instant, animate: bool) {
        if (end - self.end).abs() < f32::EPSILON {
            return;
        }
        let current = self.value(now);
        if !animate || (current - end).abs() < f32::EPSILON {
            *self = Self::settled(end);
            return;
        }
        let running = self.eased_progress(now);
        if let Some(eased) = running
            && (end - self.reversing_adjusted_start).abs() < f32::EPSILON
        {
            let factor = (eased * self.shortening_factor + (1.0 - self.shortening_factor))
                .abs()
                .clamp(0.0, 1.0);
            *self = Self {
                start: current,
                end,
                reversing_adjusted_start: self.end,
                shortening_factor: factor,
                started_at: Some(now),
                duration: MARKER_TRANSITION.mul_f32(factor),
            };
            return;
        }
        *self = Self {
            start: current,
            end,
            reversing_adjusted_start: current,
            shortening_factor: 1.0,
            started_at: Some(now),
            duration: MARKER_TRANSITION,
        };
    }
}

pub(super) fn smooth_scroll_duration(distance: f32) -> Duration {
    Duration::from_secs_f32(distance.abs().sqrt().min(SMOOTH_SCROLL_MAX_FRAMES) / 60.0)
}

/// The curve Chromium's programmatic smooth scroll follows in the reference
/// (fitted to 570 samples from 40 to 6400 px; RMS error 1.1%).
pub(super) fn smooth_scroll_ease(progress: f32) -> f32 {
    cubic_bezier_ease(progress, 0.4, 0.0, 0.0, 1.0)
}

/// A smooth transcript scroll, in the list's pixel offsets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TranscriptScroll {
    pub(super) from: f32,
    pub(super) to: f32,
    pub(super) started_at: Instant,
    pub(super) duration: Duration,
}

impl TranscriptScroll {
    pub(super) fn new(from: f32, to: f32, now: Instant) -> Self {
        Self {
            from,
            to,
            started_at: now,
            duration: smooth_scroll_duration(to - from),
        }
    }

    pub(super) fn offset(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let progress = now.saturating_duration_since(self.started_at).as_secs_f32()
            / self.duration.as_secs_f32();
        self.from + (self.to - self.from) * smooth_scroll_ease(progress)
    }

    pub(super) fn is_finished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started_at) >= self.duration
    }
}

/// The highlight's `color-mix` share of the text colour `elapsed` after a
/// jump, or `None` once the animation has finished.
pub(super) fn jump_highlight_mix(elapsed: Duration) -> Option<f32> {
    if elapsed >= JUMP_HIGHLIGHT {
        return None;
    }
    let eased = ease_enter_snappy(elapsed.as_secs_f32() / JUMP_HIGHLIGHT.as_secs_f32());
    if eased <= JUMP_HIGHLIGHT_HOLD {
        return Some(JUMP_HIGHLIGHT_PEAK);
    }
    let settle = (eased - JUMP_HIGHLIGHT_HOLD) / (1.0 - JUMP_HIGHLIGHT_HOLD);
    Some(JUMP_HIGHLIGHT_PEAK + (JUMP_HIGHLIGHT_REST - JUMP_HIGHLIGHT_PEAK) * settle)
}

/// Opacity of a text-coloured layer painted over the resting bubble so the
/// two together print the highlight's share of the text colour: the
/// reference swaps the bubble's own `text 5%` fill, and this build's bubble
/// keeps its calibrated surface underneath.
pub(super) fn jump_highlight_overlay(mix: f32) -> f32 {
    (1.0 - (1.0 - mix) / (1.0 - JUMP_HIGHLIGHT_REST)).max(0.0)
}

/// `--top-fade` and `--bottom-fade` of the rail's `vertical-scroll-fade-mask`.
/// The scroll-driven animation only runs while the list can scroll, and the
/// registered lengths interpolate linearly between the held ends.
pub(super) fn rail_edge_fades(scroll_top: f32, max_scroll: f32) -> (f32, f32) {
    if max_scroll <= 0.0 {
        return (0.0, 0.0);
    }
    let progress = (scroll_top / max_scroll).clamp(0.0, 1.0);
    let fade =
        ((progress - RAIL_EDGE_FADE_HOLD) / (1.0 - 2.0 * RAIL_EDGE_FADE_HOLD)).clamp(0.0, 1.0);
    (RAIL_EDGE_FADE * fade, RAIL_EDGE_FADE * (1.0 - fade))
}

/// The mask's alpha at `y` inside a rail list `height` tall.
pub(super) fn rail_mask_alpha(y: f32, height: f32, (top, bottom): (f32, f32)) -> f32 {
    let from_top = if top > 0.0 {
        (y / top).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let from_bottom = if bottom > 0.0 {
        ((height - y) / bottom).clamp(0.0, 1.0)
    } else {
        1.0
    };
    from_top.min(from_bottom)
}

/// The reference keeps the first current marker inside the rail list with
/// the smallest scroll, landing one pixel past the row when it scrolls down.
pub(super) fn rail_follow_scroll_top(
    scroll_top: f32,
    client_height: f32,
    row_top: f32,
    row_height: f32,
) -> f32 {
    if row_top < scroll_top {
        row_top
    } else if row_top + row_height > scroll_top + client_height {
        row_top + row_height - client_height + 1.0
    } else {
        scroll_top
    }
}

/// The card is centred on its marker and shifted to stay inside the window.
pub(super) fn card_top(marker_centre: f32, card_height: f32, window_height: f32) -> f32 {
    let centred = marker_centre - card_height * 0.5;
    centred
        .min(window_height - CARD_WINDOW_PADDING - card_height)
        .max(CARD_WINDOW_PADDING)
}

/// The corridor the pointer may cross from where it left one surface to the
/// facing edge of the other without closing the card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SafeTriangle {
    start: Point<Pixels>,
    end_a: Point<Pixels>,
    end_b: Point<Pixels>,
}

impl SafeTriangle {
    /// Toward a destination on the right (the card) or on the left (the
    /// rail), from the point where the pointer left the other surface.
    pub(super) fn toward(
        start: Point<Pixels>,
        destination: Bounds<Pixels>,
        destination_on_right: bool,
    ) -> Option<Self> {
        if destination.size.width <= Pixels::ZERO || destination.size.height <= Pixels::ZERO {
            return None;
        }
        let edge = if destination_on_right {
            destination.left()
        } else {
            destination.right()
        };
        let overhang = gpui::px(SAFE_TRIANGLE_OVERHANG);
        Some(Self {
            start,
            end_a: gpui::point(edge, destination.top() - overhang),
            end_b: gpui::point(edge, destination.bottom() + overhang),
        })
    }

    pub(super) fn contains(&self, point: Point<Pixels>) -> bool {
        let side = |a: Point<Pixels>, b: Point<Pixels>| {
            let (px, py) = (f32::from(point.x), f32::from(point.y));
            let (ax, ay) = (f32::from(a.x), f32::from(a.y));
            let (bx, by) = (f32::from(b.x), f32::from(b.y));
            (px - bx) * (ay - by) - (ax - bx) * (py - by)
        };
        let first = side(self.start, self.end_a);
        let second = side(self.end_a, self.end_b);
        let third = side(self.end_b, self.start);
        let negative = first < 0.0 || second < 0.0 || third < 0.0;
        let positive = first > 0.0 || second > 0.0 || third > 0.0;
        !(negative && positive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    #[test]
    fn marker_easing_follows_the_reference_linear_curve() {
        assert_eq!(marker_ease(0.0), 0.0);
        assert!((marker_ease(0.1) - 0.398).abs() < 1e-6);
        assert!((marker_ease(0.25) - 0.7625).abs() < 1e-6);
        // The spring overshoots by 0.8% at 70% before settling.
        assert!((marker_ease(0.7) - 1.008).abs() < 1e-6);
        assert_eq!(marker_ease(1.0), 1.0);
    }

    #[test]
    fn marker_transition_reaches_its_target_in_160ms() {
        let start = Instant::now();
        let mut marker = MarkerTransition::settled(0.0);
        marker.retarget(1.0, start, true);
        assert!(marker.is_running(start + Duration::from_millis(80)));
        let halfway = marker.value(start + Duration::from_millis(80));
        assert!((halfway - 0.972).abs() < 1e-3, "{halfway}");
        assert_eq!(marker.value(start + MARKER_TRANSITION), 1.0);
        assert!(!marker.is_running(start + MARKER_TRANSITION));
    }

    /// Hovering off a marker 40 ms into its widening reverses over the part of
    /// the transition that had played, as Chromium does.
    #[test]
    fn reversing_a_marker_shortens_the_transition() {
        let start = Instant::now();
        let mut marker = MarkerTransition::settled(0.0);
        marker.retarget(1.0, start, true);
        let reversed_at = start + Duration::from_millis(40);
        let played = marker_ease(0.25);
        marker.retarget(0.0, reversed_at, true);
        let expected = MARKER_TRANSITION.mul_f32(played);
        assert!(marker.is_running(reversed_at + expected - Duration::from_millis(1)));
        assert!(!marker.is_running(reversed_at + expected + Duration::from_millis(1)));
        // A change to a new value plays the full duration again.
        let mut other = MarkerTransition::settled(0.0);
        other.retarget(1.0, start, true);
        other.retarget(0.7, reversed_at, true);
        assert!(other.is_running(reversed_at + MARKER_TRANSITION - Duration::from_millis(1)));
    }

    #[test]
    fn scrubbing_snaps_markers_without_a_transition() {
        let now = Instant::now();
        let mut marker = MarkerTransition::settled(0.2);
        marker.retarget(1.0, now, false);
        assert_eq!(marker.value(now), 1.0);
        assert!(!marker.is_running(now));
    }

    #[test]
    fn smooth_scroll_matches_chromium_programmatic_scrolls() {
        // sqrt(|Δ|) frames at 60 Hz: 800 px takes ~471 ms, 6400 px ~1333 ms.
        assert_eq!(smooth_scroll_duration(800.0).as_millis(), 471);
        assert_eq!(smooth_scroll_duration(-6400.0).as_millis(), 1333);
        assert_eq!(smooth_scroll_duration(1.0e9).as_secs(), 3);
        // Recorded on an 800 px scrollBy: 3% at 10%, ~36% at 25%, ~92% at 60%.
        assert!((smooth_scroll_ease(0.1) - 0.03).abs() < 0.02);
        assert!((smooth_scroll_ease(0.25) - 0.36).abs() < 0.06);
        assert!((smooth_scroll_ease(0.6) - 0.92).abs() < 0.02);
    }

    #[test]
    fn jump_highlight_holds_then_settles_to_the_bubble_fill() {
        assert_eq!(jump_highlight_mix(Duration::ZERO), Some(0.14));
        assert_eq!(jump_highlight_mix(Duration::from_millis(100)), Some(0.14));
        let late = jump_highlight_mix(Duration::from_millis(1300)).unwrap();
        assert!(late > 0.05 && late < 0.051, "{late}");
        assert_eq!(jump_highlight_mix(JUMP_HIGHLIGHT), None);
        assert!((jump_highlight_overlay(0.14) - 0.0947).abs() < 1e-3);
        assert_eq!(jump_highlight_overlay(0.05), 0.0);
    }

    #[test]
    fn rail_edge_fade_tracks_scroll_progress() {
        assert_eq!(rail_edge_fades(0.0, 0.0), (0.0, 0.0));
        assert_eq!(rail_edge_fades(0.0, 29.0), (0.0, 40.0));
        assert_eq!(rail_edge_fades(29.0, 29.0), (40.0, 0.0));
        // Recorded: scrollTop 10 of 19 printed --top-fade 21.0741px.
        let (top, bottom) = rail_edge_fades(10.0, 19.0);
        assert!((top - 21.0741).abs() < 1e-3 && (bottom - 18.9259).abs() < 1e-3);
        assert_eq!(rail_mask_alpha(10.0, 160.0, (20.0, 20.0)), 0.5);
        assert_eq!(rail_mask_alpha(80.0, 160.0, (20.0, 20.0)), 1.0);
        assert_eq!(rail_mask_alpha(155.0, 160.0, (0.0, 40.0)), 0.125);
    }

    #[test]
    fn rail_follow_keeps_the_first_current_marker_in_view() {
        // Recorded: marker 17 of 18 in a 161 px list scrolled to 10.
        assert_eq!(rail_follow_scroll_top(0.0, 161.0, 160.0, 10.0), 10.0);
        assert_eq!(rail_follow_scroll_top(29.0, 161.0, 0.0, 10.0), 0.0);
        assert_eq!(rail_follow_scroll_top(10.0, 161.0, 50.0, 10.0), 10.0);
    }

    #[test]
    fn card_is_centred_on_its_marker_and_kept_inside_the_window() {
        // Recorded at 1800x250: markers at 60 and 190 with a 116 px card.
        assert_eq!(card_top(60.0, 116.0, 250.0), 8.0);
        assert_eq!(card_top(190.0, 116.0, 250.0), 126.0);
        assert_eq!(card_top(465.0, 116.0, 1000.0), 407.0);
    }

    #[test]
    fn safe_triangle_spans_from_the_exit_point_to_the_card_edge() {
        let card = Bounds::new(point(px(293.0), px(407.0)), size(px(320.0), px(116.0)));
        let triangle = SafeTriangle::toward(point(px(293.0), px(465.0)), card, true).unwrap();
        assert!(triangle.contains(point(px(293.0), px(465.0))));
        assert!(!triangle.contains(point(px(270.0), px(465.0))));
        let wide = SafeTriangle::toward(point(px(250.0), px(465.0)), card, true).unwrap();
        assert!(wide.contains(point(px(290.0), px(420.0))));
        assert!(!wide.contains(point(px(290.0), px(300.0))));
    }
}
