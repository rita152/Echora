//! Fade-in of the assistant message that is still arriving.
//!
//! ChatGPT renders a streaming message with `data-markdown-animated`: every
//! text segment (a word together with the punctuation and space that follow
//! it), each inline decoration (code span, link), and every list item, table
//! row, blockquote and rule mounts at opacity 0 and fades in with
//! `cubic-bezier(.37, .55, .86, .88)`. Text segments and list markers take
//! `--animation-duration-streaming-text` (0.7 s); block containers take
//! `--transition-duration-basic` (0.15 s). Segments keep their identity while
//! the message grows, so a word that is still being revealed never restarts
//! its fade, and the whole message re-renders without animation once it
//! completes. `prefers-reduced-motion` disables the fade entirely.

use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

use unicode_segmentation::UnicodeSegmentation;

/// `--animation-duration-streaming-text` in the ChatGPT desktop bundle.
pub const STREAMING_TEXT_FADE_DURATION: Duration = Duration::from_millis(700);
/// `--transition-duration-basic`, used by list items, table rows, blockquotes
/// and rules while the message streams.
pub const STREAMING_BLOCK_FADE_DURATION: Duration = Duration::from_millis(150);
/// The `--fade-easing` default of the animated markdown root.
const STREAMING_FADE_BEZIER: (f32, f32, f32, f32) = (0.37, 0.55, 0.86, 0.88);

/// Per-message fade timeline. Segment keys are the enclosing inline block's
/// identity plus the segment's ordinal inside that block; block keys are the
/// block identity. Both are index paths, which stay stable while a streaming
/// message only appends text.
#[derive(Debug)]
pub struct MarkdownFade {
    now: Instant,
    current_inline: u64,
    counters: HashMap<u64, usize>,
    segments: HashMap<(u64, usize), Instant>,
    blocks: HashMap<u64, Instant>,
    /// Newest moment something started to fade: a newly revealed segment or
    /// block, or a change the view reported before this frame laid it out.
    last_started: Instant,
}

impl MarkdownFade {
    fn new(now: Instant) -> Self {
        Self {
            now,
            current_inline: 0,
            counters: HashMap::new(),
            segments: HashMap::new(),
            blocks: HashMap::new(),
            last_started: now,
        }
    }

    fn alpha(&self, start: Instant, duration: Duration) -> f32 {
        let elapsed = self.now.saturating_duration_since(start);
        if elapsed >= duration {
            1.0
        } else {
            let (x1, y1, x2, y2) = STREAMING_FADE_BEZIER;
            cubic_bezier_ease(
                elapsed.as_secs_f32() / duration.as_secs_f32(),
                x1,
                y1,
                x2,
                y2,
            )
        }
    }

    /// Alpha of the segment consumed last in the current inline block, or
    /// of a new one when the block has none yet.
    fn continuation_alpha(&mut self) -> f32 {
        match self.counters.get(&self.current_inline).copied() {
            Some(ordinal) if ordinal > 0 => {
                let start = self.segments[&(self.current_inline, ordinal - 1)];
                self.alpha(start, STREAMING_TEXT_FADE_DURATION)
            }
            _ => self.next_segment_alpha(),
        }
    }

    fn next_segment_alpha(&mut self) -> f32 {
        let ordinal = self.counters.entry(self.current_inline).or_insert(0);
        let key = (self.current_inline, *ordinal);
        *ordinal += 1;
        let now = self.now;
        let start = *self.segments.entry(key).or_insert_with(|| {
            self.last_started = self.last_started.max(now);
            now
        });
        self.alpha(start, STREAMING_TEXT_FADE_DURATION)
    }

    fn block_alpha(&mut self, identity: u64, duration: Duration) -> f32 {
        let now = self.now;
        let start = *self.blocks.entry(identity).or_insert_with(|| {
            self.last_started = self.last_started.max(now);
            now
        });
        self.alpha(start, duration)
    }
}

/// Shared handle the renderer consults while it walks the streaming message.
/// The default handle is inert: every alpha query answers `None`, so settled
/// messages take the exact same paint path as before.
#[derive(Clone, Default)]
pub struct MarkdownFadeHandle(Option<Rc<RefCell<MarkdownFade>>>);

impl MarkdownFadeHandle {
    pub fn none() -> Self {
        Self(None)
    }

    pub fn new(now: Instant) -> Self {
        Self(Some(Rc::new(RefCell::new(MarkdownFade::new(now)))))
    }

    pub fn is_active(&self) -> bool {
        self.0.is_some()
    }

    /// The frame time every alpha of this render pass is evaluated at.
    pub fn set_now(&self, now: Instant) {
        if let Some(fade) = &self.0 {
            fade.borrow_mut().now = now;
        }
    }

    /// Records that the revealed text changed at `now`. Its new segments are
    /// only registered while the next frame lays out, so the view reports the
    /// change up front to keep frames coming for the whole fade.
    pub fn note_change(&self, now: Instant) {
        if let Some(fade) = &self.0 {
            let mut fade = fade.borrow_mut();
            fade.last_started = fade.last_started.max(now);
        }
    }

    /// Whether a fade may still be in progress, so the view needs another
    /// frame. False while the model pauses and every word has settled.
    pub fn is_animating(&self) -> bool {
        self.0.as_ref().is_some_and(|fade| {
            let fade = fade.borrow();
            fade.now < fade.last_started + STREAMING_TEXT_FADE_DURATION
        })
    }

    /// Starts counting segments of an inline block. Rendering the same block
    /// again within a frame restarts at ordinal 0, so repeated layout passes
    /// look up the same keys.
    pub(super) fn begin_inline(&self, identity: u64) {
        if let Some(fade) = &self.0 {
            let mut fade = fade.borrow_mut();
            fade.current_inline = identity;
            fade.counters.insert(identity, 0);
        }
    }

    /// Alpha of the next decoration (inline code span or link) of the
    /// current inline block; `None` when the handle is inert.
    pub(super) fn next_segment_alpha(&self) -> Option<f32> {
        self.0
            .as_ref()
            .map(|fade| fade.borrow_mut().next_segment_alpha())
    }

    /// Alpha of a text segment. A segment that starts with punctuation or
    /// whitespace continues the word before it, as the reference attaches
    /// such characters to the preceding segment. The two inline layout paths
    /// split text at different points around punctuation, so counting only
    /// word-led segments keeps each word's key when a paragraph switches
    /// paths (its first code span or link arrives) and no word refades.
    pub(super) fn segment_alpha(&self, segment: &str) -> Option<f32> {
        let word_led = segment.chars().next().is_some_and(char::is_alphanumeric);
        self.0.as_ref().map(|fade| {
            let mut fade = fade.borrow_mut();
            if word_led {
                fade.next_segment_alpha()
            } else {
                fade.continuation_alpha()
            }
        })
    }

    #[cfg(test)]
    pub(super) fn segment_count(&self) -> usize {
        self.0
            .as_ref()
            .map_or(0, |fade| fade.borrow().segments.len())
    }

    /// Alpha of a block-level container; `None` when the handle is inert.
    pub(super) fn block_alpha(&self, identity: u64, duration: Duration) -> Option<f32> {
        self.0
            .as_ref()
            .map(|fade| fade.borrow_mut().block_alpha(identity, duration))
    }
}

/// Splits a text run the way ChatGPT's animated markdown does: every run of
/// letters and digits starts a segment, and the punctuation and whitespace
/// that follow stay attached to it. ASCII text uses the bundle's fast path;
/// anything else follows `Intl.Segmenter` word boundaries, where a segment
/// that contains no letter, digit or ideograph is not word-like and joins the
/// segment before it.
pub fn fade_segments(text: &str) -> Vec<&str> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    if text.is_ascii() {
        let bytes = text.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index].is_ascii_alphanumeric() {
                let start = index;
                while index < bytes.len() && bytes[index].is_ascii_alphanumeric() {
                    index += 1;
                }
                ranges.push((start, index));
            } else {
                extend_last(&mut ranges, index, index + 1);
                index += 1;
            }
        }
    } else {
        for (offset, word) in text.split_word_bound_indices() {
            let word_like = word.chars().any(char::is_alphanumeric);
            if word_like {
                ranges.push((offset, offset + word.len()));
            } else {
                extend_last(&mut ranges, offset, offset + word.len());
            }
        }
    }
    ranges
        .into_iter()
        .map(|(start, end)| &text[start..end])
        .collect()
}

fn extend_last(ranges: &mut Vec<(usize, usize)>, start: usize, end: usize) {
    match ranges.last_mut() {
        Some((_, last_end)) if *last_end == start => *last_end = end,
        _ => ranges.push((start, end)),
    }
}

/// Evaluates a CSS `cubic-bezier(x1, y1, x2, y2)` timing function at
/// `progress`, inverting x by bisection before reading y.
pub(crate) fn cubic_bezier_ease(progress: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..10 {
        let parameter = (lower + upper) * 0.5;
        let inverse = 1.0 - parameter;
        let x = 3.0 * inverse * inverse * parameter * x1
            + 3.0 * inverse * parameter * parameter * x2
            + parameter * parameter * parameter;
        if x < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    let parameter = (lower + upper) * 0.5;
    let inverse = 1.0 - parameter;
    3.0 * inverse * inverse * parameter * y1
        + 3.0 * inverse * parameter * parameter * y2
        + parameter * parameter * parameter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_segments_start_at_alphanumeric_runs_and_keep_trailing_punctuation() {
        assert_eq!(
            fade_segments("Hello! What would you"),
            ["Hello! ", "What ", "would ", "you"]
        );
        assert_eq!(fade_segments("**bold"), ["**", "bold"]);
        assert_eq!(fade_segments("e.g. foo"), ["e.", "g. ", "foo"]);
        assert_eq!(fade_segments("don't"), ["don'", "t"]);
        assert_eq!(fade_segments("   "), ["   "]);
        assert!(fade_segments("").is_empty());
    }

    #[test]
    fn unicode_segments_follow_word_boundaries_and_attach_punctuation() {
        assert_eq!(
            fade_segments("你好，世界 hello"),
            ["你", "好，", "世", "界 ", "hello"]
        );
        assert_eq!(fade_segments("«quoted» word"), ["«", "quoted» ", "word"]);
        assert_eq!(fade_segments("naïve test"), ["naïve ", "test"]);
    }

    #[test]
    fn segments_cover_the_whole_text_in_order() {
        for text in [
            "a b  c",
            "多语言 mixed text!",
            "trailing space ",
            " leading",
        ] {
            assert_eq!(fade_segments(text).concat(), text);
        }
    }

    #[test]
    fn segment_alpha_rises_from_zero_to_one_and_keeps_its_start() {
        let start = Instant::now();
        let fade = MarkdownFadeHandle::new(start);
        fade.begin_inline(7);
        assert_eq!(fade.next_segment_alpha(), Some(0.0));

        fade.set_now(start + Duration::from_millis(350));
        fade.begin_inline(7);
        let midway = fade.next_segment_alpha().unwrap();
        assert!(midway > 0.0 && midway < 1.0, "{midway}");
        // A segment that appears later starts its own fade from zero.
        assert_eq!(fade.next_segment_alpha(), Some(0.0));

        fade.set_now(start + STREAMING_TEXT_FADE_DURATION);
        fade.begin_inline(7);
        assert_eq!(fade.next_segment_alpha(), Some(1.0));
        let second = fade.next_segment_alpha().unwrap();
        assert!(second > 0.0 && second < 1.0, "{second}");
        assert!(fade.is_animating());

        // Once every segment has settled, no more frames are needed until
        // the view reports new text.
        fade.set_now(start + STREAMING_TEXT_FADE_DURATION * 3);
        assert!(!fade.is_animating());
        fade.note_change(start + STREAMING_TEXT_FADE_DURATION * 3);
        assert!(fade.is_animating());
    }

    #[test]
    fn punctuation_led_segments_share_the_previous_word_alpha() {
        let start = Instant::now();
        let fade = MarkdownFadeHandle::new(start);
        fade.begin_inline(3);
        assert_eq!(fade.segment_alpha("(leading"), Some(0.0));
        assert_eq!(fade.segment_count(), 1);
        fade.set_now(start + Duration::from_millis(350));
        let word = fade.segment_alpha("word").unwrap();
        assert_eq!(word, 0.0);
        // "(" continues "word" instead of starting a fade of its own.
        assert_eq!(fade.segment_alpha("("), Some(word));
        assert_eq!(fade.segment_count(), 2);
    }

    #[test]
    fn block_alpha_uses_the_block_duration_and_is_inert_without_a_timeline() {
        let start = Instant::now();
        let fade = MarkdownFadeHandle::new(start);
        assert_eq!(
            fade.block_alpha(1, STREAMING_BLOCK_FADE_DURATION),
            Some(0.0)
        );
        fade.set_now(start + STREAMING_BLOCK_FADE_DURATION);
        assert_eq!(
            fade.block_alpha(1, STREAMING_BLOCK_FADE_DURATION),
            Some(1.0)
        );
        let marker = fade.block_alpha(1, STREAMING_TEXT_FADE_DURATION).unwrap();
        assert!(marker > 0.0 && marker < 1.0, "{marker}");

        let inert = MarkdownFadeHandle::none();
        assert!(!inert.is_active());
        assert_eq!(inert.next_segment_alpha(), None);
        assert_eq!(inert.block_alpha(1, STREAMING_BLOCK_FADE_DURATION), None);
        inert.note_change(start);
        assert!(!inert.is_animating());
    }

    #[test]
    fn fade_easing_matches_the_reference_curve_endpoints_and_shape() {
        let (x1, y1, x2, y2) = STREAMING_FADE_BEZIER;
        assert_eq!(cubic_bezier_ease(0.0, x1, y1, x2, y2), 0.0);
        assert_eq!(cubic_bezier_ease(1.0, x1, y1, x2, y2), 1.0);
        let quarter = cubic_bezier_ease(0.25, x1, y1, x2, y2);
        let half = cubic_bezier_ease(0.5, x1, y1, x2, y2);
        let three_quarters = cubic_bezier_ease(0.75, x1, y1, x2, y2);
        assert!(quarter < half && half < three_quarters);
        // The curve is close to linear with a slightly slow start.
        assert!((half - 0.55).abs() < 0.05, "{half}");
    }
}
