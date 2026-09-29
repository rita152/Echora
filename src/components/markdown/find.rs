//! Find-in-chat highlights for rendered message text.
//!
//! A row is rendered inside [`with_find_scope`]; every text run built during
//! that call marks the case-insensitive matches of the query, and the n-th
//! match of the row (counted in rendering order, as the reference counts its
//! DOM matches) gets the active colour. Rendering is synchronous, so the
//! scope is a thread-local set only for the duration of one row.

use std::{cell::RefCell, ops::Range};

use gpui::{HighlightStyle, Rgba, TextRun};

#[derive(Clone, Debug)]
pub(crate) struct FindScope {
    pub(crate) query: String,
    /// The row's match to mark as active, by position.
    pub(crate) active: Option<usize>,
    pub(crate) match_color: Rgba,
    pub(crate) active_color: Rgba,
}

struct ScopeState {
    scope: FindScope,
    seen: usize,
}

thread_local! {
    static SCOPE: RefCell<Option<ScopeState>> = const { RefCell::new(None) };
}

/// Renders with highlights for `scope`, or none when it is `None`.
pub(crate) fn with_find_scope<R>(scope: Option<FindScope>, render: impl FnOnce() -> R) -> R {
    let previous =
        SCOPE.with(|cell| cell.replace(scope.map(|scope| ScopeState { scope, seen: 0 })));
    let result = render();
    SCOPE.with(|cell| cell.replace(previous));
    result
}

/// The next matches of the current row inside `text`, with their colours.
fn take_matches(text: &str) -> Vec<(Range<usize>, Rgba)> {
    SCOPE.with(|cell| {
        let mut state = cell.borrow_mut();
        let Some(state) = state.as_mut() else {
            return Vec::new();
        };
        crate::conversation::case_insensitive_matches(text, state.scope.query.trim())
            .into_iter()
            .map(|range| {
                let active = state.scope.active == Some(state.seen);
                state.seen += 1;
                let color = if active {
                    state.scope.active_color
                } else {
                    state.scope.match_color
                };
                (range, color)
            })
            .collect()
    })
}

/// Splits `runs` at match boundaries and paints the matches' background.
pub(crate) fn highlight_runs(text: &str, runs: Vec<TextRun>) -> Vec<TextRun> {
    let matches = take_matches(text);
    if matches.is_empty() {
        return runs;
    }
    let mut output = Vec::with_capacity(runs.len() + matches.len() * 2);
    let mut start = 0usize;
    for run in runs {
        let end = start + run.len;
        let mut cursor = start;
        for (range, color) in &matches {
            let (from, to) = (range.start.max(cursor), range.end.min(end));
            if from >= to {
                continue;
            }
            if from > cursor {
                output.push(TextRun {
                    len: from - cursor,
                    ..run.clone()
                });
            }
            output.push(TextRun {
                len: to - from,
                background_color: Some((*color).into()),
                ..run.clone()
            });
            cursor = to;
        }
        if cursor < end {
            output.push(TextRun {
                len: end - cursor,
                ..run.clone()
            });
        }
        start = end;
    }
    output
}

/// Match highlights for plain text rendered with `StyledText::with_highlights`.
pub(crate) fn highlight_ranges(text: &str) -> Vec<(Range<usize>, HighlightStyle)> {
    take_matches(text)
        .into_iter()
        .map(|(range, color)| {
            (
                range,
                HighlightStyle {
                    background_color: Some(color.into()),
                    ..Default::default()
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(active: Option<usize>) -> FindScope {
        FindScope {
            query: "hello".into(),
            active,
            match_color: gpui::rgba(0xf8d45dff),
            active_color: gpui::rgba(0xea7339ff),
        }
    }

    fn run(len: usize) -> TextRun {
        TextRun {
            len,
            font: crate::theme::ui_font(),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    #[test]
    fn runs_split_at_matches_and_the_active_one_is_counted_across_calls() {
        let (first, second) = with_find_scope(Some(scope(Some(1))), || {
            let first = highlight_runs("Hello and ", vec![run(6), run(4)]);
            let second = highlight_ranges("say hello");
            (first, second)
        });
        assert_eq!(
            first.iter().map(|run| run.len).collect::<Vec<_>>(),
            [5, 1, 4]
        );
        assert_eq!(
            first[0].background_color,
            Some(gpui::rgba(0xf8d45dff).into())
        );
        assert_eq!(first[1].background_color, None);
        // The second match of the row is the active one.
        assert_eq!(second[0].0, 4..9);
        assert_eq!(
            second[0].1.background_color,
            Some(gpui::rgba(0xea7339ff).into())
        );
        // Outside a scope nothing is marked.
        assert!(highlight_ranges("hello").is_empty());
    }
}
