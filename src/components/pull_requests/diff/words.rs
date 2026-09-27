//! Word-level highlights inside a changed line pair, as the reference's diff
//! viewer computes them (`lineDiffType: "word-alt"`).
//!
//! The reference diffs a deletion against its paired addition with jsdiff's
//! `diffWordsWithSpace` (Myers over word, whitespace-run and single-character
//! tokens), then walks the changes per side: runs of the same kind merge, and
//! a one-character unchanged token right after a change joins that change, so
//! `a b` changes read as one span. Each side's changed segments are its spans.

use std::ops::Range;

/// Lines longer than this (in UTF-16 units) get no word highlights.
const MAX_LINE_DIFF_LENGTH: usize = 1000;

/// jsdiff's `extendedWordChars` plus the ASCII word characters.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || c == '_'
        || matches!(
            c,
            '\u{AD}'
                | '\u{C0}'..='\u{D6}'
                | '\u{D8}'..='\u{F6}'
                | '\u{F8}'..='\u{2C6}'
                | '\u{2C8}'..='\u{2D7}'
                | '\u{2DE}'..='\u{2FF}'
                | '\u{1E00}'..='\u{1EFF}'
        )
}

fn is_inline_space(c: char) -> bool {
    c.is_whitespace() && c != '\n' && c != '\r'
}

/// `(\r?\n)|[word]+|[^\S\n\r]+|[^word]`, as byte ranges.
fn tokenize(text: &str) -> Vec<Range<usize>> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let mut end = start + c.len_utf8();
        if c == '\r' && chars.peek().is_some_and(|&(_, next)| next == '\n') {
            let (index, next) = chars.next().unwrap_or((end, '\n'));
            end = index + next.len_utf8();
        } else if is_word_char(c) || is_inline_space(c) {
            let same = if is_word_char(c) {
                is_word_char
            } else {
                is_inline_space
            };
            while let Some(&(index, next)) = chars.peek() {
                if !same(next) {
                    break;
                }
                end = index + next.len_utf8();
                chars.next();
            }
        }
        tokens.push(start..end);
    }
    tokens
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    Common,
    Added,
    Removed,
}

#[derive(Clone)]
struct Component {
    count: usize,
    change: Change,
    previous: Option<std::rc::Rc<Component>>,
}

#[derive(Clone)]
struct Path {
    old_pos: isize,
    last: Option<std::rc::Rc<Component>>,
}

/// jsdiff's `Diff.diffWithOptionsObj`: the change components, in order, as
/// `(change, token count)`.
fn myers(old: &[&str], new: &[&str]) -> Vec<(Change, usize)> {
    use std::rc::Rc;

    let (old_len, new_len) = (old.len() as isize, new.len() as isize);
    let add_to_path = |path: &Path, change: Change, old_inc: isize| -> Path {
        match &path.last {
            Some(last) if last.change == change => Path {
                old_pos: path.old_pos + old_inc,
                last: Some(Rc::new(Component {
                    count: last.count + 1,
                    change,
                    previous: last.previous.clone(),
                })),
            },
            last => Path {
                old_pos: path.old_pos + old_inc,
                last: Some(Rc::new(Component {
                    count: 1,
                    change,
                    previous: last.clone(),
                })),
            },
        }
    };
    let extract_common = |path: &mut Path, diagonal: isize| -> isize {
        let mut old_pos = path.old_pos;
        let mut new_pos = old_pos - diagonal;
        let mut common = 0;
        while new_pos + 1 < new_len
            && old_pos + 1 < old_len
            && old[(old_pos + 1) as usize] == new[(new_pos + 1) as usize]
        {
            new_pos += 1;
            old_pos += 1;
            common += 1;
        }
        if common > 0 {
            path.last = Some(Rc::new(Component {
                count: common,
                change: Change::Common,
                previous: path.last.take(),
            }));
        }
        path.old_pos = old_pos;
        new_pos
    };
    let build = |last: Option<Rc<Component>>| -> Vec<(Change, usize)> {
        let mut components = Vec::new();
        let mut next = last;
        while let Some(component) = next {
            components.push((component.change, component.count));
            next = component.previous.clone();
        }
        components.reverse();
        components
    };

    // `bestPath` is indexed by diagonal, which ranges over ±(old + new).
    let offset = old_len + new_len + 1;
    let mut best: Vec<Option<Path>> = vec![None; (2 * offset + 1) as usize];
    let slot = |diagonal: isize| (diagonal + offset) as usize;
    let mut first = Path {
        old_pos: -1,
        last: None,
    };
    let new_pos = extract_common(&mut first, 0);
    if first.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
        return build(first.last);
    }
    best[slot(0)] = Some(first);
    let (mut min_diagonal, mut max_diagonal) = (isize::MIN, isize::MAX);
    for edit_length in 1..=old_len + new_len {
        let mut diagonal = min_diagonal.max(-edit_length);
        while diagonal <= max_diagonal.min(edit_length) {
            let remove_path = best[slot(diagonal - 1)].take();
            let add_path = best[slot(diagonal + 1)].clone();
            let can_add = add_path.as_ref().is_some_and(|path| {
                let new_pos = path.old_pos - diagonal;
                0 <= new_pos && new_pos < new_len
            });
            let can_remove = remove_path
                .as_ref()
                .is_some_and(|path| path.old_pos + 1 < old_len);
            if !can_add && !can_remove {
                best[slot(diagonal)] = None;
                diagonal += 2;
                continue;
            }
            // `!canRemove || (canAdd && removePath.oldPos < addPath.oldPos)`.
            let use_add = match (&remove_path, &add_path) {
                (Some(remove), Some(add)) => {
                    !can_remove || (can_add && remove.old_pos < add.old_pos)
                }
                (None, _) => true,
                (Some(_), None) => false,
            };
            let mut path = match (use_add, &remove_path, &add_path) {
                (true, _, Some(add)) => add_to_path(add, Change::Added, 0),
                (false, Some(remove), _) => add_to_path(remove, Change::Removed, 1),
                _ => {
                    best[slot(diagonal)] = None;
                    diagonal += 2;
                    continue;
                }
            };
            let new_pos = extract_common(&mut path, diagonal);
            if path.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
                return build(path.last);
            }
            if path.old_pos + 1 >= old_len {
                max_diagonal = max_diagonal.min(diagonal - 1);
            }
            if new_pos + 1 >= new_len {
                min_diagonal = min_diagonal.max(diagonal + 1);
            }
            best[slot(diagonal)] = Some(path);
            diagonal += 2;
        }
    }
    vec![(Change::Removed, old.len()), (Change::Added, new.len())]
}

/// The changed spans (byte ranges) of `deleted` and of `added`.
pub(super) fn changed_spans(deleted: &str, added: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let deleted = deleted.strip_suffix('\n').unwrap_or(deleted);
    let added = added.strip_suffix('\n').unwrap_or(added);
    let utf16 = |text: &str| text.encode_utf16().count();
    if utf16(deleted) > MAX_LINE_DIFF_LENGTH || utf16(added) > MAX_LINE_DIFF_LENGTH {
        return (Vec::new(), Vec::new());
    }
    let old_tokens = tokenize(deleted);
    let new_tokens = tokenize(added);
    let old: Vec<&str> = old_tokens
        .iter()
        .map(|range| &deleted[range.clone()])
        .collect();
    let new: Vec<&str> = new_tokens
        .iter()
        .map(|range| &added[range.clone()])
        .collect();
    let components = myers(&old, &new);

    // `O2e`: each side's segments, `(changed, byte range)`.
    let mut old_segments: Vec<(bool, Range<usize>)> = Vec::new();
    let mut new_segments: Vec<(bool, Range<usize>)> = Vec::new();
    let push = |segments: &mut Vec<(bool, Range<usize>)>,
                changed: bool,
                range: Range<usize>,
                last: bool,
                text: &str| {
        let neutral = !changed;
        match segments.last_mut() {
            Some((previous_changed, previous)) if !last => {
                let previous_neutral = !*previous_changed;
                let one_char = text[range.clone()].chars().count() == 1;
                if neutral == previous_neutral || (neutral && one_char && !previous_neutral) {
                    previous.end = range.end;
                    return;
                }
                segments.push((changed, range));
            }
            _ => segments.push((changed, range)),
        }
    };
    let (mut old_token, mut new_token) = (0usize, 0usize);
    let span = |tokens: &[Range<usize>], start: usize, count: usize| {
        tokens[start].start..tokens[start + count - 1].end
    };
    let last_index = components.len().saturating_sub(1);
    for (index, &(change, count)) in components.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let last = index == last_index;
        match change {
            Change::Common => {
                push(
                    &mut old_segments,
                    false,
                    span(&old_tokens, old_token, count),
                    last,
                    deleted,
                );
                push(
                    &mut new_segments,
                    false,
                    span(&new_tokens, new_token, count),
                    last,
                    added,
                );
                old_token += count;
                new_token += count;
            }
            Change::Removed => {
                push(
                    &mut old_segments,
                    true,
                    span(&old_tokens, old_token, count),
                    last,
                    deleted,
                );
                old_token += count;
            }
            Change::Added => {
                push(
                    &mut new_segments,
                    true,
                    span(&new_tokens, new_token, count),
                    last,
                    added,
                );
                new_token += count;
            }
        }
    }
    let changed = |segments: Vec<(bool, Range<usize>)>| {
        segments
            .into_iter()
            .filter_map(|(changed, range)| changed.then_some(range))
            .collect()
    };
    (changed(old_segments), changed(new_segments))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(deleted: &str, added: &str) -> (Vec<String>, Vec<String>) {
        let (old, new) = changed_spans(deleted, added);
        (
            old.into_iter()
                .map(|range| deleted[range].to_owned())
                .collect(),
            new.into_iter()
                .map(|range| added[range].to_owned())
                .collect(),
        )
    }

    #[test]
    fn spans_match_the_reference_word_alt_highlights() {
        // Pairs and highlights read from the reference diff viewer.
        assert_eq!(
            texts(
                "baseline is `codex-cli 0.153.0`; see",
                "baseline is `codex-cli 0.154.0`; see"
            ),
            (vec!["153".to_owned()], vec!["154".to_owned()])
        );
        assert_eq!(
            texts(
                "| 合计 | 248 | 233 已声明，15 缺口 | 20260914 |",
                "| 合计 | 252 | 238 已声明，14 缺口 | 20260918 |"
            ),
            (
                vec!["248".into(), "233".into(), "15".into(), "20260914".into()],
                vec!["252".into(), "238".into(), "14".into(), "20260918".into()]
            )
        );
        assert_eq!(texts("same", "same"), (vec![], vec![]));
    }

    #[test]
    fn spans_keep_unicode_boundaries() {
        assert_eq!(texts("let 名 = 1;", "let 名 = 2;").0, vec!["1".to_owned()]);
        assert_eq!(texts("a😀c", "a🦀c").0, vec!["😀".to_owned()]);
        assert_eq!(texts("相同🙂", "相同🙂"), (vec![], vec![]));
        assert_eq!(texts("abc", "ab").0, vec!["abc".to_owned()]);
    }

    #[test]
    fn a_single_unchanged_character_joins_neighbouring_changes() {
        // The space joins `one`/`uno` to what follows; the diff's final item
        // (`dos`) is always pushed on its own, as `O2e` does for `isLastItem`.
        let (old, new) = texts("one two", "uno dos");
        assert_eq!(old, vec!["one two".to_owned()]);
        assert_eq!(new, vec!["uno ".to_owned(), "dos".to_owned()]);
    }
}
