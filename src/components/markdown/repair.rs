//! Repair of the unfinished tail of a streaming assistant message.
//!
//! ChatGPT's streaming markdown rewrites the received text before parsing it,
//! so half-written syntax never flashes on screen. This is a port of that
//! pass: it drops an unterminated citation marker, a trailing image that is
//! still being written, and a link on the last line whose brackets or
//! destination have not closed yet, and it closes a dangling `*` or `**` on
//! the last line so the words render emphasized while they arrive. An open
//! code fence is left alone (its body renders as code), and an open inline
//! code span only has unfinished links trimmed, because the backtick itself
//! stays literal until it closes.

use std::borrow::Cow;

/// Start and end of the private-use citation markers Codex embeds in text.
const CITATION_START: char = '\u{E200}';
const CITATION_END: char = '\u{E201}';

/// Returns the text to parse for a message that is still streaming.
pub fn repair_streaming_markdown(source: &str) -> Cow<'_, str> {
    let text = strip_unterminated_citation(source);
    if text.is_empty() || (text.contains('`') && has_open_inline_code(text)) {
        return trim_unfinished_link(text).into();
    }
    if (text.contains("```") || text.contains("~~~")) && has_open_code_fence(text) {
        return text.into();
    }
    let text = if text.contains("![") {
        strip_partial_image(text)
    } else {
        text
    };
    let text = trim_unfinished_link(text);
    let text = close_emphasis(text.into(), "*");
    close_emphasis(text, "**")
}

fn strip_unterminated_citation(text: &str) -> &str {
    let search_from = text
        .rfind(CITATION_END)
        .map_or(0, |index| index + CITATION_END.len_utf8());
    match text[search_from..].find(CITATION_START) {
        Some(offset) => &text[..search_from + offset],
        None => text,
    }
}

/// Whether the byte at `index` is preceded by an odd number of backslashes.
fn is_escaped(bytes: &[u8], index: usize) -> bool {
    bytes[..index]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count()
        % 2
        == 1
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Fence {
    marker: u8,
    length: usize,
}

/// Tracks an open code fence across one line (already left-trimmed).
fn fence_state(state: Option<Fence>, line: &str) -> Option<Fence> {
    let bytes = line.as_bytes();
    let Some(&marker) = bytes.first().filter(|byte| matches!(byte, b'`' | b'~')) else {
        return state;
    };
    let length = bytes.iter().take_while(|byte| **byte == marker).count();
    if length < 3 {
        return state;
    }
    let rest = &line[length..];
    match state {
        None if marker == b'`' && rest.contains('`') => None,
        None => Some(Fence { marker, length }),
        Some(open) if marker == open.marker && length >= open.length && rest.trim().is_empty() => {
            None
        }
        open => open,
    }
}

fn has_open_code_fence(text: &str) -> bool {
    text.split('\n')
        .fold(None, |state, line| fence_state(state, line.trim_start()))
        .is_some()
}

/// Whether an inline code span outside fenced code is still open. The span
/// may cross soft line breaks, as CommonMark allows.
fn has_open_inline_code(text: &str) -> bool {
    let mut fence = None;
    let mut open_run: Option<usize> = None;
    for line in text.split('\n') {
        let was_fenced = fence.is_some();
        fence = fence_state(fence, line.trim_start());
        if was_fenced || fence.is_some() {
            continue;
        }
        let bytes = line.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != b'`' {
                index += 1;
                continue;
            }
            let end = index + bytes[index..].iter().take_while(|b| **b == b'`').count();
            let run = end - index;
            if open_run == Some(run) {
                open_run = None;
            } else if open_run.is_none() && !is_escaped(bytes, index) {
                open_run = Some(run);
            }
            index = end;
        }
    }
    open_run.is_some()
}

/// Drops a trailing line that is only an image still being written:
/// `![alt`, `![alt]`, or `![alt](partial-destination`.
fn strip_partial_image(text: &str) -> &str {
    let trimmed = text.trim_end();
    let line_start = trimmed.rfind('\n').map_or(0, |index| index + 1);
    let line = trimmed[line_start..].trim_start_matches(|c: char| c.is_whitespace() && c != '\n');
    let Some(rest) = line.strip_prefix("![") else {
        return text;
    };
    let partial = match rest.find(']') {
        None => true,
        Some(close) => {
            let after = &rest[close + 1..];
            after.is_empty()
                || after
                    .strip_prefix('(')
                    .is_some_and(|destination| !destination.contains(')'))
        }
    };
    if partial { &text[..line_start] } else { text }
}

/// Index of the delimiter that closes the one at `start`, honoring nesting,
/// escapes, code spans inside link text, and quoted or angle-bracketed parts
/// of a destination.
fn matching_close(text: &str, start: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0_i64;
    let mut quote: Option<u8> = None;
    let mut code_run: Option<usize> = None;
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if open == b'[' && byte == b'`' {
            let end = index + bytes[index..].iter().take_while(|b| **b == b'`').count();
            let run = end - index;
            if code_run == Some(run) {
                code_run = None;
            } else if code_run.is_none() {
                code_run = Some(run);
            }
            index = end;
            continue;
        }
        if code_run.is_some() {
        } else if byte == b'\\' {
            index += 1;
        } else if let Some(expected) = quote {
            if byte == expected {
                quote = None;
            }
        } else if open == b'('
            && (index == start + 1 || bytes[index - 1].is_ascii_whitespace())
            && matches!(byte, b'<' | b'"' | b'\'')
        {
            quote = Some(if byte == b'<' { b'>' } else { byte });
        } else if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

/// `/:[\w-]+$/`: the bracket opens a directive such as `:name[`.
fn ends_with_directive_name(prefix: &str) -> bool {
    let name_len = prefix
        .bytes()
        .rev()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        .count();
    name_len > 0
        && (prefix.len() - name_len)
            .checked_sub(1)
            .is_some_and(|colon| prefix.as_bytes()[colon] == b':')
}

/// `/^\s*(?:[-+*]|\d+[.)])\s+$/`: the text before the bracket is a list
/// marker, so `[ ]` or `[x]` is a task checkbox rather than a link.
fn is_list_marker(prefix: &str) -> bool {
    let rest = prefix.trim_start();
    let after_marker = if let Some(rest) = rest.strip_prefix(['-', '+', '*']) {
        rest
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        match rest[digits..].strip_prefix(['.', ')']) {
            Some(rest) if digits > 0 => rest,
            _ => return false,
        }
    };
    !after_marker.is_empty() && after_marker.trim_start().is_empty()
}

fn is_task_checkbox(candidate: &str) -> bool {
    matches!(candidate, "[ ]" | "[x]" | "[X]")
}

/// Cuts the last line at the `[` of a link that has not closed yet.
fn trim_unfinished_link(text: &str) -> &str {
    let bytes = text.as_bytes();
    let line_start = text.rfind('\n').map_or(0, |index| index + 1);
    let last_line = &text[line_start..];
    if last_line.starts_with("    ") || last_line.starts_with('\t') {
        return text;
    }
    let mut search = line_start;
    while let Some(offset) = text[search..].find('[') {
        let open = search + offset;
        search = open + 1;
        if is_escaped(bytes, open)
            || (open > 0 && bytes[open - 1] == b'!')
            || ends_with_directive_name(&text[line_start..open])
            || has_open_inline_code(&text[..open])
        {
            continue;
        }
        let close = matching_close(text, open, b'[', b']');
        if let Some(close) = close
            && is_list_marker(&text[line_start..open])
            && is_task_checkbox(&text[open..=close])
        {
            search = close + 1;
            continue;
        }
        let Some(close) = close.filter(|close| *close + 1 < bytes.len()) else {
            return &text[..open];
        };
        let next = bytes[close + 1];
        if next == b'(' || next == b'[' {
            let closing = if next == b'(' { b')' } else { b']' };
            match matching_close(text, close + 1, next, closing) {
                Some(end) => search = end + 1,
                None => return &text[..open],
            }
        } else {
            search = close + 1;
        }
    }
    text
}

/// A single-character marker next to the same character is part of a
/// longer run (`**`), not a marker of its own.
fn is_part_of_longer_run(bytes: &[u8], index: usize, marker: &str) -> bool {
    if marker.len() != 1 {
        return false;
    }
    let byte = marker.as_bytes()[0];
    (index > 0 && bytes[index - 1] == byte) || bytes.get(index + 1) == Some(&byte)
}

fn is_marker_at(text: &str, index: usize, marker: &str) -> bool {
    text[index..].starts_with(marker)
        && !is_escaped(text.as_bytes(), index)
        && !is_part_of_longer_run(text.as_bytes(), index, marker)
}

fn count_markers(text: &str, marker: &str) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index + marker.len() <= text.len() {
        if text.is_char_boundary(index) && is_marker_at(text, index, marker) {
            count += 1;
            index += marker.len();
        } else {
            index += 1;
        }
    }
    count
}

fn last_marker(text: &str, marker: &str) -> Option<usize> {
    (0..=text.len().checked_sub(marker.len())?)
        .rev()
        .find(|&index| text.is_char_boundary(index) && is_marker_at(text, index, marker))
}

/// Closes an odd `marker` whose emphasized words are still arriving on the
/// last line, keeping any trailing whitespace after the inserted closer.
fn close_emphasis<'a>(text: Cow<'a, str>, marker: &str) -> Cow<'a, str> {
    if !text.contains(marker) || count_markers(&text, marker).is_multiple_of(2) {
        return text;
    }
    let Some(open) = last_marker(&text, marker) else {
        return text;
    };
    let emphasized = &text[open + marker.len()..];
    if emphasized.is_empty()
        || emphasized.starts_with(char::is_whitespace)
        || emphasized.contains('\n')
        || has_open_inline_code(emphasized)
    {
        return text;
    }
    let body = text.trim_end();
    let closer = if marker == "**"
        && body.ends_with('*')
        && !is_escaped(body.as_bytes(), body.len() - 1)
        && !count_markers(body, "*").is_multiple_of(2)
    {
        "*"
    } else {
        marker
    };
    format!("{body}{closer}{}", &text[body.len()..]).into()
}

#[cfg(test)]
mod tests {
    use super::repair_streaming_markdown as repair;

    #[test]
    fn unfinished_links_are_hidden_until_they_close() {
        assert_eq!(repair("see [GPUI"), "see ");
        assert_eq!(repair("see [GPUI]"), "see ");
        assert_eq!(repair("see [GPUI](https://github.com/zed"), "see ");
        assert_eq!(repair("see [GPUI](<https://x y"), "see ");
        assert_eq!(repair("see [GPUI][ref"), "see ");
        assert_eq!(
            repair("see [GPUI](https://github.com/zed) now"),
            "see [GPUI](https://github.com/zed) now"
        );
        assert_eq!(repair("a [`x]y` label"), "a ");
        assert_eq!(repair("array[0] is set"), "array[0] is set");
    }

    #[test]
    fn links_are_only_trimmed_on_the_last_line_and_outside_code() {
        assert_eq!(repair("[open\nnext line"), "[open\nnext line");
        assert_eq!(repair("escaped \\[not a link"), "escaped \\[not a link");
        assert_eq!(repair("an image ![alt"), "an image ![alt");
        assert_eq!(repair("    indented [code"), "    indented [code");
        assert_eq!(repair("call :name[arg"), "call :name[arg");
        assert_eq!(repair("- [ ] task"), "- [ ] task");
        assert_eq!(repair("1. [x] done"), "1. [x] done");
    }

    #[test]
    fn dangling_emphasis_on_the_last_line_is_closed() {
        assert_eq!(repair("a **bold"), "a **bold**");
        assert_eq!(repair("a *it"), "a *it*");
        assert_eq!(repair("a **bold "), "a **bold** ");
        // The reference counts single stars only when they stand alone, so
        // it closes just the `**` run of a triple opener.
        assert_eq!(repair("a ***both"), "a ***both**");
        assert_eq!(repair("a ** spaced"), "a ** spaced");
        assert_eq!(repair("a **"), "a **");
        assert_eq!(repair("a **bold** done"), "a **bold** done");
        assert_eq!(repair("**open\nnext"), "**open\nnext");
        assert_eq!(repair("2 \\* 3 *it"), "2 \\* 3 *it*");
    }

    #[test]
    fn open_code_is_left_for_the_parser() {
        let fence = "```rust\nfn main() {\n    let v = [1, **2";
        assert_eq!(repair(fence), fence);
        let tilde = "~~~\n[x **y";
        assert_eq!(repair(tilde), tilde);
        assert_eq!(repair("run `cargo **b"), "run `cargo **b");
        assert_eq!(repair("run `cargo` then [docs"), "run `cargo` then ");
        assert_eq!(
            repair("```\nclosed\n```\n**bold"),
            "```\nclosed\n```\n**bold**"
        );
    }

    #[test]
    fn partial_images_and_citations_are_dropped() {
        assert_eq!(repair("text\n![alt](https://x/y"), "text\n");
        assert_eq!(repair("text\n  ![alt]"), "text\n");
        assert_eq!(repair("text\n![alt"), "text\n");
        assert_eq!(
            repair("text\n![alt](https://x/y.png)"),
            "text\n![alt](https://x/y.png)"
        );
        assert_eq!(repair("fact \u{E200}cite"), "fact ");
        assert_eq!(
            repair("fact \u{E200}cite\u{E201} more"),
            "fact \u{E200}cite\u{E201} more"
        );
    }

    #[test]
    fn repair_keeps_multibyte_text_intact() {
        assert_eq!(repair("中文 **强调"), "中文 **强调**");
        assert_eq!(repair("链接 [说明](https://例"), "链接 ");
        assert_eq!(repair("全角＊不是标记"), "全角＊不是标记");
    }
}
