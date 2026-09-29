//! Occurrences of a search term inside one thread
//! (`thread/searchOccurrences`), and the UTF-16 ranges they carry.

use std::ops::Range;

/// One match in a visible user message or final assistant answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadOccurrence {
    pub item_id: String,
    pub turn_id: String,
    /// Opaque inclusive `thread/turns/list` cursor for this turn.
    pub turn_cursor: String,
    pub snippet: String,
    /// The match inside `snippet`, already converted from UTF-16 code units
    /// to byte offsets.
    pub snippet_match: Range<usize>,
}

#[cfg(test)]
impl AgentThreadOccurrence {
    /// The matched text as the message spells it.
    pub fn matched_text(&self) -> &str {
        &self.snippet[self.snippet_match.clone()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadOccurrenceRequest {
    pub thread_id: String,
    pub search_term: String,
    pub cursor: Option<String>,
    pub limit: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadOccurrencePage {
    pub generation: u64,
    pub occurrences: Vec<AgentThreadOccurrence>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentThreadSearchError {
    /// The server cannot search this thread (`-32601`, e.g. an ephemeral side
    /// chat, or an older server): the caller may fall back to local text.
    Unsupported(String),
    Failed(String),
}

impl std::fmt::Display for AgentThreadSearchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(message) | Self::Failed(message) => formatter.write_str(message),
        }
    }
}

/// Converts a UTF-16 code-unit range of `text` to byte offsets. `None` when
/// the range is reversed, past the end, or splits a surrogate pair.
pub fn utf16_range_to_bytes(text: &str, start: usize, end: usize) -> Option<Range<usize>> {
    if start > end {
        return None;
    }
    let mut units = 0usize;
    let mut byte_start = None;
    let mut byte_end = None;
    for (offset, character) in text.char_indices() {
        if units == start {
            byte_start = Some(offset);
        }
        if units == end {
            byte_end = Some(offset);
            break;
        }
        units += character.len_utf16();
        if units > end && byte_end.is_none() {
            // `end` falls inside this character's surrogate pair.
            return None;
        }
    }
    if units == start && byte_start.is_none() {
        byte_start = Some(text.len());
    }
    if units == end && byte_end.is_none() {
        byte_end = Some(text.len());
    }
    Some(byte_start?..byte_end?)
}

#[cfg(test)]
mod tests {
    use super::utf16_range_to_bytes;

    #[test]
    fn utf16_ranges_map_to_byte_offsets_through_chinese_and_emoji() {
        // Recorded by the baseline probe: "HELLO from the assistant 🙂 hello".
        let text = "HELLO from the assistant 🙂 hello";
        let emoji = utf16_range_to_bytes(text, 25, 27).unwrap();
        assert_eq!(&text[emoji], "🙂");
        let last = utf16_range_to_bytes(text, 28, 33).unwrap();
        assert_eq!(&text[last], "hello");
        let chinese = "你好🙂世界，你好 REPLY2";
        assert_eq!(
            &chinese[utf16_range_to_bytes(chinese, 0, 2).unwrap()],
            "你好"
        );
        assert_eq!(
            &chinese[utf16_range_to_bytes(chinese, 7, 9).unwrap()],
            "你好"
        );
        assert_eq!(&chinese[utf16_range_to_bytes(chinese, 2, 4).unwrap()], "🙂");
        let mixed = "回答：你好 😀 hello";
        assert_eq!(
            &mixed[utf16_range_to_bytes(mixed, 6, 14).unwrap()],
            "😀 hello"
        );
        // Half of a surrogate pair, a reversed range and one past the end are rejected.
        assert_eq!(utf16_range_to_bytes(text, 25, 26), None);
        assert_eq!(utf16_range_to_bytes(text, 26, 27), None);
        assert_eq!(utf16_range_to_bytes(text, 5, 3), None);
        assert_eq!(utf16_range_to_bytes(text, 30, 40), None);
        assert_eq!(utf16_range_to_bytes("", 0, 0), Some(0..0));
    }
}
