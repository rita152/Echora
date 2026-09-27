//! Paced reveal of the assistant message that is still arriving.
//!
//! ChatGPT does not print a delta the moment it arrives. The streaming
//! markdown keeps the received text as its `source` and publishes a growing
//! prefix of it: every 50 ms it reveals `rate` characters per second, and the
//! rate follows the backlog, so a burst of tokens drains evenly over about a
//! second instead of jumping onto the screen. Text present when the message
//! first renders shows at once; only later deltas are paced. The moment the
//! message completes, the whole source is published.

use std::time::{Duration, Instant};

/// The reference's `setTimeout(…, 50)` drain cadence.
pub(super) const STREAMING_REVEAL_TICK: Duration = Duration::from_millis(50);
/// Length of one drain step, in seconds, used to count how many steps
/// elapsed between two updates.
const DRAIN_STEP_SECS: f32 = 0.05;
/// The desktop bundle passes 10 s here unless its `streaming-response` config
/// is on, which leaves the rate to the backlog controller below.
const ARRIVAL_INTERVAL_FLOOR_SECS: f32 = 10.0;

#[derive(Debug)]
pub(super) struct StreamingReveal {
    item_id: String,
    source: String,
    /// Byte length of the revealed prefix, always on a char boundary.
    revealed: usize,
    revealed_chars: usize,
    source_chars: usize,
    actively_streaming: bool,
    /// Characters per second.
    rate: f32,
    last_arrival: Option<Instant>,
    last_drain: Option<Instant>,
    drain_carry: f32,
    last_target_chars: usize,
}

impl StreamingReveal {
    /// Starts revealing `source`; everything present now is visible at once.
    pub(super) fn new(item_id: &str, source: &str, actively_streaming: bool) -> Self {
        let source_chars = source.chars().count();
        Self {
            item_id: item_id.to_owned(),
            source: source.to_owned(),
            revealed: source.len(),
            revealed_chars: source_chars,
            source_chars,
            actively_streaming,
            rate: 1.0,
            last_arrival: None,
            last_drain: None,
            drain_carry: 0.0,
            last_target_chars: 0,
        }
    }

    pub(super) fn item_id(&self) -> &str {
        &self.item_id
    }

    pub(super) fn revealed_text(&self) -> &str {
        &self.source[..self.revealed]
    }

    /// Whether a drain tick could still change what is shown.
    pub(super) fn should_update(&self) -> bool {
        self.revealed < self.source.len()
    }

    /// Feeds the latest received text. A different message, a source that no
    /// longer extends the previous one, or a message that starts streaming
    /// again resets the reveal so the current text shows immediately.
    pub(super) fn update(
        &mut self,
        item_id: &str,
        source: &str,
        actively_streaming: bool,
        now: Instant,
    ) {
        let reset = item_id != self.item_id
            || !source.starts_with(&self.source)
            || (!self.actively_streaming && actively_streaming);
        if reset {
            *self = Self::new(item_id, source, actively_streaming);
            return;
        }
        let unchanged =
            source.len() == self.source.len() && actively_streaming == self.actively_streaming;
        if unchanged {
            return;
        }
        if source.len() > self.source.len() {
            self.source_chars += source[self.source.len()..].chars().count();
            self.source = source.to_owned();
        }
        self.actively_streaming = actively_streaming;
        if actively_streaming {
            if self.should_update() {
                self.drain(now);
            }
        } else {
            self.reveal_all();
        }
    }

    /// One drain step. Returns whether the revealed text changed.
    pub(super) fn tick(&mut self, now: Instant) -> bool {
        if !self.actively_streaming || !self.should_update() {
            return false;
        }
        let before = self.revealed;
        self.drain(now);
        self.revealed != before
    }

    fn reveal_all(&mut self) {
        self.revealed = self.source.len();
        self.revealed_chars = self.source_chars;
    }

    fn drain(&mut self, now: Instant) {
        let arrived = self.source_chars.saturating_sub(self.last_target_chars);
        self.last_target_chars = self.source_chars;
        if arrived > 0 {
            match self.last_arrival {
                Some(last_arrival) => {
                    let interval = now
                        .saturating_duration_since(last_arrival)
                        .as_secs_f32()
                        .max(ARRIVAL_INTERVAL_FLOOR_SECS);
                    let arrival_rate = arrived as f32 / interval;
                    self.rate = (0.1 * arrival_rate + 0.9 * self.rate).max(1.0);
                }
                None => self.rate = arrived as f32,
            }
            self.last_arrival = Some(now);
        }
        let elapsed = self.last_drain.map_or(0.0, |last| {
            now.saturating_duration_since(last).as_secs_f32()
        });
        self.last_drain = Some(now);
        let backlog = (self.source_chars - self.revealed_chars) as f32;
        // The reference loops `for (e = 0; e < value / 0.05; e++)`, so a
        // timer that fires a little late runs two steps, not one.
        let steps = ((elapsed + self.drain_carry) / DRAIN_STEP_SECS).ceil() as usize;
        for _ in 0..steps {
            if backlog > 0.0 && backlog < self.rate * 1.5 {
                self.rate = (backlog + self.rate * 1.5) / 2.0;
            }
            if backlog > self.rate / 1.2 {
                self.rate = (backlog + self.rate / 1.2) / 2.0;
            }
        }
        self.rate = self.rate.max(1.0);
        let reveal = elapsed * self.rate + self.drain_carry;
        let whole = reveal.floor();
        self.drain_carry = reveal - whole;
        let whole = whole as usize;
        if whole >= 1 {
            self.advance_chars(whole);
        }
    }

    fn advance_chars(&mut self, count: usize) {
        let mut consumed = 0;
        let mut end = self.revealed;
        for (offset, character) in self.source[self.revealed..].char_indices() {
            if consumed == count {
                break;
            }
            consumed += 1;
            end = self.revealed + offset + character.len_utf8();
        }
        self.revealed = end;
        self.revealed_chars += consumed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn text_present_at_first_render_shows_at_once_and_later_deltas_are_paced() {
        let start = Instant::now();
        let mut reveal = StreamingReveal::new("m", "Hello", true);
        assert_eq!(reveal.revealed_text(), "Hello");
        assert!(!reveal.should_update());

        reveal.update(
            "m",
            "Hello! What would you like to work on?",
            true,
            at(start, 0),
        );
        assert_eq!(reveal.revealed_text(), "Hello");
        assert!(reveal.should_update());

        let mut lengths = Vec::new();
        for step in 1..=40 {
            reveal.tick(at(start, step * 50));
            lengths.push(reveal.revealed_text().chars().count());
        }
        assert!(
            lengths.windows(2).all(|pair| pair[0] <= pair[1]),
            "{lengths:?}"
        );
        assert!(lengths[3] > 5, "reveal never started: {lengths:?}");
        assert!(lengths[3] < 38, "reveal was not paced: {lengths:?}");
        assert_eq!(
            reveal.revealed_text(),
            "Hello! What would you like to work on?"
        );
        assert!(!reveal.should_update());
    }

    #[test]
    fn the_rate_follows_the_backlog_so_a_burst_drains_within_about_a_second() {
        let start = Instant::now();
        let mut reveal = StreamingReveal::new("m", "", true);
        let burst = "x".repeat(200);
        reveal.update("m", &burst, true, at(start, 0));
        let mut revealed_after = |millis: u64| {
            reveal.tick(at(start, millis));
            reveal.revealed_text().len()
        };
        let quarter = revealed_after(250);
        let half = revealed_after(500);
        let second = (0..10)
            .map(|step| revealed_after(550 + step * 50))
            .last()
            .unwrap();
        let two_seconds = (0..20)
            .map(|step| revealed_after(1050 + step * 50))
            .last()
            .unwrap();
        let four_seconds = (0..40)
            .map(|step| revealed_after(2050 + step * 50))
            .last()
            .unwrap();
        assert!(quarter > 0 && quarter < half, "{quarter} {half}");
        assert!(second > 100, "{second}");
        // The rate settles at twice the backlog, so the last characters ease
        // out instead of landing all at once.
        assert!(two_seconds >= 195, "{two_seconds}");
        assert_eq!(four_seconds, 200);
    }

    #[test]
    fn completion_publishes_the_whole_source() {
        let start = Instant::now();
        let mut reveal = StreamingReveal::new("m", "a", true);
        reveal.update(
            "m",
            "a long tail that is still buffered",
            true,
            at(start, 0),
        );
        reveal.tick(at(start, 50));
        assert!(reveal.should_update());
        reveal.update(
            "m",
            "a long tail that is still buffered",
            false,
            at(start, 60),
        );
        assert_eq!(reveal.revealed_text(), "a long tail that is still buffered");
        assert!(!reveal.should_update());
        assert!(!reveal.tick(at(start, 200)));
    }

    #[test]
    fn a_new_message_or_a_rewritten_source_resets_to_the_current_text() {
        let start = Instant::now();
        let mut reveal = StreamingReveal::new("m", "a", true);
        reveal.update("m", "abcdefgh", true, at(start, 0));
        assert_eq!(reveal.revealed_text(), "a");
        reveal.update("n", "next message", true, at(start, 10));
        assert_eq!(reveal.item_id(), "n");
        assert_eq!(reveal.revealed_text(), "next message");
        reveal.update("n", "next message continues", true, at(start, 20));
        reveal.update(
            "n",
            "rewritten by the completed snapshot",
            true,
            at(start, 30),
        );
        assert_eq!(
            reveal.revealed_text(),
            "rewritten by the completed snapshot"
        );
    }

    #[test]
    fn reveal_advances_by_characters_and_stays_on_char_boundaries() {
        let start = Instant::now();
        let mut reveal = StreamingReveal::new("m", "", true);
        reveal.update("m", "你好，世界！", true, at(start, 0));
        let mut seen = Vec::new();
        for step in 1..=60 {
            reveal.tick(at(start, step * 50));
            seen.push(reveal.revealed_text().to_owned());
        }
        assert!(
            seen.iter()
                .all(|text| "你好，世界！".starts_with(text.as_str()))
        );
        assert!(
            seen.iter()
                .any(|text| !text.is_empty() && text != "你好，世界！")
        );
        assert_eq!(seen.last().unwrap(), "你好，世界！");
    }
}
