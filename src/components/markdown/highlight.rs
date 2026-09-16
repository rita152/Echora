//! Reuse syntax state up to the last complete line while a code block streams.
//! A partial line is always reparsed, so the result is identical to a full parse.
use std::{cell::RefCell, collections::VecDeque};

use super::*;

const CACHE_BUDGET: usize = 4 * 1024 * 1024;
const CACHE_ENTRIES: usize = 16;

struct HighlightedCode {
    syntax: &'static str,
    code: String,
    spans: Vec<CodeHighlightSpan>,
    stable_end: usize,
    parse_state: ParseState,
    scope_stack: ScopeStack,
}

impl HighlightedCode {
    fn weight(&self) -> usize {
        self.code.len() + self.spans.len() * std::mem::size_of::<CodeHighlightSpan>()
    }
}

#[derive(Default)]
struct HighlightCache {
    entries: VecDeque<HighlightedCode>,
    bytes: usize,
}

impl HighlightCache {
    fn spans(&mut self, code: &str, language: Option<&str>) -> Option<Vec<CodeHighlightSpan>> {
        if code.len() > MAX_HIGHLIGHTED_CODE_BYTES {
            return None;
        }
        let syntax = code_syntax(language)?;
        let exact = self
            .entries
            .iter()
            .position(|entry| entry.syntax == syntax.name && entry.code == code);
        if let Some(index) = exact {
            let entry = self.entries.remove(index)?;
            let spans = entry.spans.clone();
            self.entries.push_back(entry);
            return Some(spans);
        }
        // Only append operations may reuse parser state. Replacements and
        // language changes use a fresh parse, including authoritative snapshots.
        let previous = self
            .entries
            .iter()
            .rposition(|entry| entry.syntax == syntax.name && code.starts_with(&entry.code));
        let entry = previous.and_then(|index| self.entries.remove(index));
        if let Some(entry) = &entry {
            self.bytes -= entry.weight();
        }
        let highlighted = parse_code(code, syntax, entry)?;
        let spans = highlighted.spans.clone();
        let weight = highlighted.weight();
        if weight <= CACHE_BUDGET {
            while self.entries.len() >= CACHE_ENTRIES || self.bytes + weight > CACHE_BUDGET {
                self.bytes -= self.entries.pop_front()?.weight();
            }
            self.bytes += weight;
            self.entries.push_back(highlighted);
        }
        Some(spans)
    }
}

thread_local! {
    static CACHE: RefCell<HighlightCache> = RefCell::new(HighlightCache::default());
}

pub(super) fn highlighted_code_spans(
    code: &str,
    language: Option<&str>,
) -> Option<Vec<CodeHighlightSpan>> {
    CACHE.with_borrow_mut(|cache| cache.spans(code, language))
}

fn parse_code(
    code: &str,
    syntax: &'static SyntaxReference,
    previous: Option<HighlightedCode>,
) -> Option<HighlightedCode> {
    let (mut spans, mut line_base, mut parse_state, mut scope_stack) =
        if let Some(mut entry) = previous {
            entry
                .spans
                .retain(|span| span.range.start < entry.stable_end);
            if let Some(last) = entry.spans.last_mut() {
                last.range.end = last.range.end.min(entry.stable_end);
            }
            (
                entry.spans,
                entry.stable_end,
                entry.parse_state,
                entry.scope_stack,
            )
        } else {
            (Vec::new(), 0, ParseState::new(syntax), ScopeStack::new())
        };
    let mut stable_end = line_base;
    let mut stable_parse = parse_state.clone();
    let mut stable_scope = scope_stack.clone();
    for line in LinesWithEndings::from(&code[line_base..]) {
        if line.len() > MAX_HIGHLIGHTED_LINE_BYTES {
            return None;
        }
        let operations = parse_state.parse_line(line, code_syntax_set()).ok()?;
        let mut line_cursor = 0;
        for (segment, operation) in ScopeRegionIterator::new(&operations, line) {
            scope_stack.apply(operation).ok()?;
            let start = line_base + line_cursor;
            line_cursor += segment.len();
            push_code_span(
                &mut spans,
                start..line_base + line_cursor,
                code_scope_style(&scope_stack),
            );
        }
        if line_cursor != line.len() {
            return None;
        }
        line_base += line.len();
        if line.ends_with('\n') {
            stable_end = line_base;
            stable_parse = parse_state.clone();
            stable_scope = scope_stack.clone();
        }
    }
    if line_base != code.len()
        || spans.iter().any(|span| {
            !code.is_char_boundary(span.range.start) || !code.is_char_boundary(span.range.end)
        })
        || spans
            .windows(2)
            .any(|spans| spans[0].range.end != spans[1].range.start)
        || spans.first().is_some_and(|span| span.range.start != 0)
        || spans
            .last()
            .is_some_and(|span| span.range.end != code.len())
    {
        return None;
    }
    Some(HighlightedCode {
        syntax: &syntax.name,
        code: code.to_owned(),
        spans,
        stable_end,
        parse_state: stable_parse,
        scope_stack: stable_scope,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_highlighting_matches_full_parse_at_every_unicode_boundary() {
        for (language, code) in [
            (
                "rust",
                "fn main() {\n /* multi\nline */ let 值 = r#\"你好\nworld\"#;\n println!(\"{值}\");\n}\n",
            ),
            (
                "python",
                "def hello():\n    text = \"\"\"first\nsecond\"\"\"\n    print('你好', text)\n",
            ),
            (
                "javascript",
                "/* first\n second */\nconst x = `hello\n${1 + 2}`;\n",
            ),
            ("bash", "cat <<'EOF'\nhello\nEOF\nprintf '%s\\n' ok\n"),
        ] {
            let mut cache = HighlightCache::default();
            for end in code.char_indices().map(|(i, _)| i).chain([code.len()]) {
                let partial = &code[..end];
                let actual = cache.spans(partial, Some(language)).unwrap();
                let expected =
                    parse_code(partial, code_syntax(Some(language)).unwrap(), None).unwrap();
                assert_eq!(actual, expected.spans, "{language} at {end}");
            }
            let changed = code.replace("hello", "replacement");
            assert_eq!(
                cache.spans(&changed, Some(language)).unwrap(),
                parse_code(&changed, code_syntax(Some(language)).unwrap(), None)
                    .unwrap()
                    .spans
            );
        }
    }

    #[test]
    fn highlight_cache_is_bounded_and_keeps_language_identity() {
        let mut cache = HighlightCache::default();
        for n in 0..40 {
            let code = format!("// {n}\n{}", "let value = 42;\n".repeat(1000));
            cache.spans(&code, Some("rust")).unwrap();
            assert!(cache.bytes <= CACHE_BUDGET);
            assert!(cache.entries.len() <= CACHE_ENTRIES);
        }
        let code = "# comment\nprint('hello')\n";
        for lang in ["python", "rust", "python"] {
            assert_eq!(
                cache.spans(code, Some(lang)).unwrap(),
                parse_code(code, code_syntax(Some(lang)).unwrap(), None)
                    .unwrap()
                    .spans
            );
        }
    }

    #[test]
    #[ignore = "manual streaming syntax timing benchmark"]
    fn streaming_highlight_timings() {
        use std::time::Instant;
        let mut cache = HighlightCache::default();
        let syntax = code_syntax(Some("rust")).unwrap();
        let mut code = "fn main() {\n".to_owned();
        let mut full = Vec::new();
        let mut incremental = Vec::new();
        for index in 0..500 {
            code.push_str(&format!(
                "    let value_{index} = \"你好 streamed value {index}\";\n"
            ));
            let start = Instant::now();
            let expected = parse_code(&code, syntax, None).unwrap();
            full.push(start.elapsed().as_secs_f64() * 1000.);
            let start = Instant::now();
            let actual = cache.spans(&code, Some("rust")).unwrap();
            incremental.push(start.elapsed().as_secs_f64() * 1000.);
            assert_eq!(actual, expected.spans);
        }
        for (label, mut times) in [("full", full), ("incremental", incremental)] {
            times.sort_by(f64::total_cmp);
            eprintln!(
                "stream_highlight {label}: samples={} p50_ms={:.3} p95_ms={:.3} max_ms={:.3}",
                times.len(),
                times[times.len() / 2],
                times[times.len() * 95 / 100],
                times[times.len() - 1]
            );
        }
    }
}
