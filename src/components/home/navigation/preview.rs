//! The hover card's response preview, laid out the way the reference prints
//! it: the reply's Markdown in `_preview` style, clamped to three lines.
//!
//! The reference renders the preview with its Markdown component and a few
//! overrides (`a`, `code` and `img` print as bare text), so a fenced code
//! block becomes whitespace-collapsed text directly in the root, between the
//! paragraphs. Paragraph spacing follows the root's rules: 13 px between
//! paragraphs, 4 px under a Han paragraph whose next sibling paragraph is Han
//! too, and sibling selectors skip that bare text.

use gpui::{FontStyle, FontWeight, SharedString, TextRun, Window, px};

use super::{
    CARD_PREVIEW_LINE_HEIGHT, CARD_PREVIEW_LINES, CARD_PREVIEW_SIZE, ascii_run_start,
    hover_card_font,
};
use crate::components::markdown::{MarkdownBlock, MarkdownInline, parse_markdown};

/// `--markdown-space` at the preview's 13 px font.
const SPACE: f32 = 13.0 / 4.0;
/// `padding-inline-start: var(--markdown-line-height)` on lists.
const LIST_INDENT: f32 = 21.0;
/// `[&:has(table)]` drops the line clamp for `max-height: 3 lines`.
const TABLE_MAX_HEIGHT: f32 = CARD_PREVIEW_LINE_HEIGHT * 3.0;
/// The table component's `text-sm leading-5`.
const TABLE_LINE_HEIGHT: f32 = 20.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::components::home) struct Emphasis {
    pub(in crate::components::home) bold: bool,
    pub(in crate::components::home) italic: bool,
}

/// One printed block of the preview.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::components::home) struct PreviewRow {
    pub(in crate::components::home) text: SharedString,
    /// Contiguous byte runs covering `text`.
    pub(in crate::components::home) runs: Vec<(usize, Emphasis)>,
    pub(in crate::components::home) lines: usize,
    pub(in crate::components::home) margin_top: f32,
    pub(in crate::components::home) font_size: f32,
    pub(in crate::components::home) line_height: f32,
    pub(in crate::components::home) weight: FontWeight,
    pub(in crate::components::home) indent: f32,
    pub(in crate::components::home) marker: Option<SharedString>,
}

/// The preview's rows, and the height its container clips them to.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::components::home) struct Preview {
    pub(in crate::components::home) rows: Vec<PreviewRow>,
    pub(in crate::components::home) height: f32,
}

impl Preview {
    pub(in crate::components::home) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Text with per-character emphasis, before whitespace is collapsed.
#[derive(Clone, Debug, Default)]
struct Styled(Vec<(char, Emphasis)>);

/// A `<br>` survives whitespace collapsing.
const HARD_BREAK: char = '\u{2028}';

impl Styled {
    fn push_str(&mut self, text: &str, emphasis: Emphasis) {
        self.0
            .extend(text.chars().map(|character| (character, emphasis)));
    }

    fn push_inlines(&mut self, inlines: &[MarkdownInline], emphasis: Emphasis) {
        for inline in inlines {
            match inline {
                MarkdownInline::Text(text) | MarkdownInline::Code(text) => {
                    self.push_str(text, emphasis)
                }
                MarkdownInline::SoftBreak => self.0.push(('\n', emphasis)),
                MarkdownInline::HardBreak => self.0.push((HARD_BREAK, emphasis)),
                MarkdownInline::Strong(children) => self.push_inlines(
                    children,
                    Emphasis {
                        bold: true,
                        ..emphasis
                    },
                ),
                MarkdownInline::Emphasis(children) => self.push_inlines(
                    children,
                    Emphasis {
                        italic: true,
                        ..emphasis
                    },
                ),
                MarkdownInline::Strikethrough(children)
                | MarkdownInline::Link {
                    content: children, ..
                } => self.push_inlines(children, emphasis),
            }
        }
    }

    fn from_inlines(inlines: &[MarkdownInline]) -> Self {
        let mut styled = Self::default();
        styled.push_inlines(inlines, Emphasis::default());
        styled
    }

    fn plain(text: &str) -> Self {
        let mut styled = Self::default();
        styled.push_str(text, Emphasis::default());
        styled
    }

    /// `white-space: normal`: runs of spaces and segment breaks become one
    /// space, a segment break between two wide characters disappears, and the
    /// block is trimmed.
    fn collapsed(&self) -> Self {
        let mut out: Vec<(char, Emphasis)> = Vec::with_capacity(self.0.len());
        let mut pending: Option<(bool, Emphasis)> = None; // (saw a segment break, emphasis)
        for &(character, emphasis) in &self.0 {
            if character == HARD_BREAK {
                pending = None;
                out.push(('\n', emphasis));
                continue;
            }
            if character.is_whitespace() {
                let segment_break = character == '\n' || character == '\r';
                pending = Some(match pending {
                    Some((seen, first)) => (seen || segment_break, first),
                    None => (segment_break, emphasis),
                });
                continue;
            }
            if let Some((segment_break, space_emphasis)) = pending.take() {
                let previous = out.last().map(|(previous, _)| *previous);
                let only_breaks_between_wide =
                    segment_break && previous.is_some_and(is_wide) && is_wide(character);
                if previous.is_some_and(|previous| previous != '\n') && !only_breaks_between_wide {
                    out.push((' ', space_emphasis));
                }
            }
            out.push((character, emphasis));
        }
        Self(out)
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn has_han(&self) -> bool {
        self.0.iter().any(|(character, _)| is_han(*character))
    }

    fn text(&self) -> String {
        self.0.iter().map(|(character, _)| *character).collect()
    }

    fn runs(&self) -> Vec<(usize, Emphasis)> {
        let mut runs: Vec<(usize, Emphasis)> = Vec::new();
        for &(character, emphasis) in &self.0 {
            match runs.last_mut() {
                Some((len, last)) if *last == emphasis => *len += character.len_utf8(),
                _ => runs.push((character.len_utf8(), emphasis)),
            }
        }
        runs
    }

    fn joined(mut self, other: &Styled) -> Self {
        if !self.0.is_empty() && !other.0.is_empty() {
            self.0.push((' ', Emphasis::default()));
        }
        self.0.extend(other.0.iter().copied());
        self
    }
}

fn is_wide(character: char) -> bool {
    matches!(character as u32,
        0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}

/// `/\p{Script=Han}/u`, which decides `data-markdown-han-text`.
fn is_han(character: char) -> bool {
    matches!(character as u32,
        0x2E80..=0x2FDF | 0x3005 | 0x3007 | 0x3021..=0x3029 | 0x3038..=0x303B
        | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3FFFF)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ElementKind {
    Paragraph { han: bool },
    Heading(u8),
    List,
    Quote,
    Table,
    Rule,
}

/// A child of the Markdown root: an element, or bare text.
#[derive(Clone, Debug)]
enum Node {
    Element(ElementKind, Vec<(Styled, Option<String>, usize)>),
    Text(Styled),
}

fn list_entries(
    items: &[crate::components::markdown::MarkdownListItem],
    start: Option<u64>,
    depth: usize,
    out: &mut Vec<(Styled, Option<String>, usize)>,
) {
    for (position, item) in items.iter().enumerate() {
        let marker = match start {
            Some(start) => format!("{}.", start + position as u64),
            None => match depth {
                1 => "•".to_owned(),
                2 => "◦".to_owned(),
                _ => "▪".to_owned(),
            },
        };
        let mut text = Styled::default();
        let mut nested = Vec::new();
        for block in &item.blocks {
            match block {
                MarkdownBlock::List { start, items } => {
                    list_entries(items, *start, depth + 1, &mut nested)
                }
                block => text = text.joined(&block_text(block)),
            }
        }
        out.push((text.collapsed(), Some(marker), depth));
        out.extend(nested);
    }
}

fn block_text(block: &MarkdownBlock) -> Styled {
    match block {
        MarkdownBlock::Paragraph(inlines)
        | MarkdownBlock::Heading {
            content: inlines, ..
        } => Styled::from_inlines(inlines),
        MarkdownBlock::CodeBlock { code, .. } => Styled::plain(code),
        MarkdownBlock::Image { alt, .. } => Styled::plain(alt),
        MarkdownBlock::BlockQuote(blocks) => {
            blocks.iter().fold(Styled::default(), |text, block| {
                text.joined(&block_text(block))
            })
        }
        MarkdownBlock::List { items, .. } => items.iter().fold(Styled::default(), |text, item| {
            item.blocks
                .iter()
                .fold(text, |text, block| text.joined(&block_text(block)))
        }),
        MarkdownBlock::Table { .. } | MarkdownBlock::HorizontalRule => Styled::default(),
    }
}

fn table_row(cells: &[crate::components::markdown::MarkdownTableCell]) -> Styled {
    cells.iter().fold(Styled::default(), |row, cell| {
        row.joined(&Styled::from_inlines(&cell.content))
    })
}

fn root_nodes(source: &str) -> Vec<Node> {
    let mut nodes: Vec<Node> = Vec::new();
    for block in parse_markdown(source).blocks {
        let node = match &block {
            MarkdownBlock::Paragraph(inlines) => {
                let text = Styled::from_inlines(inlines).collapsed();
                if text.is_empty() {
                    continue;
                }
                let han = text.has_han();
                Node::Element(ElementKind::Paragraph { han }, vec![(text, None, 0)])
            }
            // A media paragraph prints the image's alt text and carries no
            // `data-markdown-han-text`.
            MarkdownBlock::Image { alt, .. } => Node::Element(
                ElementKind::Paragraph { han: false },
                vec![(Styled::plain(alt).collapsed(), None, 0)],
            ),
            MarkdownBlock::Heading { level, content } => Node::Element(
                ElementKind::Heading(*level),
                vec![(Styled::from_inlines(content).collapsed(), None, 0)],
            ),
            MarkdownBlock::List { start, items } => {
                let mut entries = Vec::new();
                list_entries(items, *start, 1, &mut entries);
                Node::Element(ElementKind::List, entries)
            }
            MarkdownBlock::BlockQuote(blocks) => Node::Element(
                ElementKind::Quote,
                blocks
                    .iter()
                    .map(|block| (block_text(block).collapsed(), None, 0))
                    .filter(|(text, ..)| !text.is_empty())
                    .collect(),
            ),
            MarkdownBlock::Table { header, rows, .. } => Node::Element(
                ElementKind::Table,
                std::iter::once(header.as_slice())
                    .chain(rows.iter().map(Vec::as_slice))
                    .map(|cells| (table_row(cells).collapsed(), None, 0))
                    .collect(),
            ),
            MarkdownBlock::HorizontalRule => Node::Element(ElementKind::Rule, Vec::new()),
            // `code` prints its children bare, so the block's text sits in the
            // root and runs together with any bare text next to it.
            MarkdownBlock::CodeBlock { code, .. } => {
                let text = Styled::plain(code);
                if let Some(Node::Text(previous)) = nodes.last_mut() {
                    *previous = std::mem::take(previous).joined(&text);
                    continue;
                }
                Node::Text(text)
            }
        };
        nodes.push(node);
    }
    for node in &mut nodes {
        if let Node::Text(text) = node {
            *text = text.collapsed();
        }
    }
    nodes.retain(|node| !matches!(node, Node::Text(text) if text.is_empty()));
    nodes
}

/// `(margin-top, margin-bottom)` of each root element, from the Markdown
/// root's rules. Bare text has none, and sibling selectors skip it.
fn element_margins(kinds: &[ElementKind], index: usize) -> (f32, f32) {
    let kind = kinds[index];
    let first = index == 0;
    let last = index + 1 == kinds.len();
    let previous = index.checked_sub(1).map(|index| kinds[index]);
    let next = kinds.get(index + 1).copied();
    let after_paragraph = matches!(previous, Some(ElementKind::Paragraph { .. }));
    match kind {
        ElementKind::Paragraph { han } => {
            let top = if first {
                0.0
            } else if after_paragraph {
                SPACE * 4.0
            } else {
                SPACE * 2.0
            };
            let bottom = if han && next == Some(ElementKind::Paragraph { han: true }) {
                4.0
            } else if after_paragraph {
                SPACE * 4.0
            } else if last {
                0.0
            } else {
                SPACE
            };
            (top, bottom)
        }
        ElementKind::Heading(level) => {
            let (top, bottom) = match level {
                1 => (0.0, SPACE * 2.0),
                2 | 3 => (SPACE * 4.0, SPACE),
                4 => (SPACE * 4.0, 0.0),
                _ => (0.0, 0.0),
            };
            let top = if first {
                0.0
            } else if previous == Some(ElementKind::List) {
                SPACE * 4.0
            } else {
                top
            };
            (top, if last { 0.0 } else { bottom })
        }
        ElementKind::Quote => (0.0, if last { 0.0 } else { SPACE * 2.0 }),
        ElementKind::List | ElementKind::Table | ElementKind::Rule => (0.0, 0.0),
    }
}

/// Size, line height and weight of a row printed by `kind`. Sizes scale from
/// the 13.4 px body, which stands in for the reference's 13 px so GPUI wraps
/// where Chromium does.
fn row_metrics(kind: Option<ElementKind>) -> (f32, f32, FontWeight) {
    match kind {
        Some(ElementKind::Heading(level)) => {
            let (scale, line_height) = match level {
                1 => (1.5, SPACE * 8.0),
                2 => (1.25, SPACE * 7.0),
                3 => (1.125, SPACE * 7.0),
                4 => (1.0, SPACE * 6.0),
                _ => (1.0, CARD_PREVIEW_LINE_HEIGHT),
            };
            (CARD_PREVIEW_SIZE * scale, line_height, FontWeight::SEMIBOLD)
        }
        Some(ElementKind::Quote) => (
            CARD_PREVIEW_SIZE,
            SPACE * 6.0,
            crate::theme::UI_BODY_FONT_WEIGHT,
        ),
        Some(ElementKind::Table) => (
            CARD_PREVIEW_SIZE,
            TABLE_LINE_HEIGHT,
            crate::theme::UI_BODY_FONT_WEIGHT,
        ),
        _ => (
            CARD_PREVIEW_SIZE,
            CARD_PREVIEW_LINE_HEIGHT,
            crate::theme::UI_BODY_FONT_WEIGHT,
        ),
    }
}

fn text_runs(text: &str, runs: &[(usize, Emphasis)], weight: FontWeight) -> Vec<TextRun> {
    let mut out = Vec::with_capacity(runs.len());
    let mut used = 0;
    for &(len, emphasis) in runs {
        let len = len.min(text.len() - used);
        if len == 0 {
            continue;
        }
        used += len;
        let mut font = hover_card_font(if emphasis.bold {
            FontWeight::SEMIBOLD
        } else {
            weight
        });
        if emphasis.italic {
            font.style = FontStyle::Italic;
        }
        out.push(TextRun {
            len,
            font,
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }
    out
}

pub(in crate::components::home) fn row_text_runs(
    row: &PreviewRow,
    color: gpui::Hsla,
) -> Vec<TextRun> {
    text_runs(&row.text, &row.runs, row.weight)
        .into_iter()
        .map(|run| TextRun { color, ..run })
        .collect()
}

fn line_count(
    styled: &Styled,
    font_size: f32,
    weight: FontWeight,
    width: f32,
    window: &Window,
) -> usize {
    let text = styled.text();
    if text.is_empty() {
        return 0;
    }
    let runs = text_runs(&text, &styled.runs(), weight);
    window
        .text_system()
        .shape_text(text.into(), px(font_size), &runs, Some(px(width)), None)
        // `shape_text` reports one entry per source line; the wrapped lines
        // inside it are the boundaries the renderer will actually break at.
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.wrap_boundaries.len() + 1)
                .sum()
        })
        .unwrap_or(1)
        .max(1)
}

/// The longest prefix that still fits `lines` with an ellipsis, cut back to
/// the last break opportunity the way `-webkit-line-clamp` truncates.
fn clamp_styled(
    styled: &Styled,
    lines: usize,
    font_size: f32,
    weight: FontWeight,
    width: f32,
    window: &Window,
) -> Styled {
    let ellipsis = |prefix: &[(char, Emphasis)]| {
        let emphasis = prefix
            .last()
            .map(|(_, emphasis)| *emphasis)
            .unwrap_or_default();
        let mut chars = prefix.to_vec();
        chars.push(('…', emphasis));
        Styled(chars)
    };
    let (mut low, mut high) = (0, styled.0.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        let candidate = ellipsis(&styled.0[..middle]);
        if line_count(&candidate, font_size, weight, width, window) <= lines {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let prefix = &styled.0[..low];
    let text: String = prefix.iter().map(|(character, _)| *character).collect();
    let cut = text[..ascii_run_start(&text)].chars().count();
    ellipsis(&prefix[..cut])
}

/// Lays the reply out as the card prints it: rows with their collapsed
/// margins, clamped to three lines (or to three lines' height when the reply
/// holds a table), the last visible row ending in an ellipsis.
pub(in crate::components::home) fn layout_preview(
    source: &str,
    width: f32,
    window: &Window,
) -> Preview {
    let nodes = root_nodes(source);
    let kinds = nodes
        .iter()
        .filter_map(|node| match node {
            Node::Element(kind, _) => Some(*kind),
            Node::Text(_) => None,
        })
        .collect::<Vec<_>>();
    let has_table = kinds.contains(&ElementKind::Table);

    let mut rows: Vec<PreviewRow> = Vec::new();
    let mut remaining = CARD_PREVIEW_LINES;
    let mut height = 0.0_f32;
    let mut pending_margin = 0.0_f32;
    let mut element_index = 0;
    'nodes: for node in &nodes {
        let (kind, entries, (top, bottom)) = match node {
            Node::Element(kind, entries) => {
                let margins = element_margins(&kinds, element_index);
                element_index += 1;
                (Some(*kind), entries.clone(), margins)
            }
            Node::Text(text) => (None, vec![(text.clone(), None, 0)], (0.0, 0.0)),
        };
        let (font_size, line_height, weight) = row_metrics(kind);
        let quote_padding = if kind == Some(ElementKind::Quote) {
            SPACE * 2.0
        } else {
            0.0
        };
        for (position, (text, marker, depth)) in entries.iter().enumerate() {
            if text.is_empty() {
                continue;
            }
            if !has_table && remaining == 0 {
                break 'nodes;
            }
            let indent = match kind {
                Some(ElementKind::Quote) => SPACE * 6.0,
                _ => *depth as f32 * LIST_INDENT,
            };
            let row_width = (width - indent).max(1.0);
            let margin_top = if position == 0 {
                if rows.is_empty() {
                    quote_padding
                } else {
                    pending_margin.max(top) + quote_padding
                }
            } else {
                0.0
            };
            let mut lines = line_count(text, font_size, weight, row_width, window);
            let mut printed = text.clone();
            if !has_table && lines > remaining {
                printed = clamp_styled(text, remaining, font_size, weight, row_width, window);
                lines = remaining;
            }
            if !has_table {
                remaining -= lines;
            }
            height += margin_top + lines as f32 * line_height;
            rows.push(PreviewRow {
                text: printed.text().into(),
                runs: printed.runs(),
                lines,
                margin_top,
                font_size,
                line_height,
                weight,
                indent,
                marker: marker.clone().map(Into::into),
            });
        }
        pending_margin = bottom;
    }
    if has_table {
        height = height.min(TABLE_MAX_HEIGHT);
    }
    Preview { rows, height }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<String> {
        root_nodes(source)
            .iter()
            .map(|node| match node {
                Node::Element(kind, entries) => format!(
                    "{kind:?}:{}",
                    entries
                        .iter()
                        .map(|(text, ..)| text.text())
                        .collect::<Vec<_>>()
                        .join("|")
                ),
                Node::Text(text) => format!("Text:{}", text.text()),
            })
            .collect()
    }

    /// Recorded: a fenced block between two Han paragraphs prints bare, 4 px
    /// under the first paragraph, its lines run together.
    #[test]
    fn a_code_block_prints_as_bare_collapsed_text() {
        let source = "这是一段说明。\n\n```text\n你是建模手，\n本轮只做问题一。\n\nfirst line\nsecond\n```\n\n几点说明。";
        assert_eq!(
            kinds(source),
            vec![
                "Paragraph { han: true }:这是一段说明。".to_owned(),
                "Text:你是建模手，本轮只做问题一。 first line second".to_owned(),
                "Paragraph { han: true }:几点说明。".to_owned(),
            ]
        );
        let kinds = [
            ElementKind::Paragraph { han: true },
            ElementKind::Paragraph { han: true },
        ];
        // The first paragraph's next sibling element is the Han paragraph
        // after the bare text: 4 px under it, 13 px above the second.
        assert_eq!(element_margins(&kinds, 0), (0.0, 4.0));
        assert_eq!(element_margins(&kinds, 1).0, 13.0);
    }

    #[test]
    fn paragraph_margins_follow_the_markdown_root() {
        let latin = ElementKind::Paragraph { han: false };
        let han = ElementKind::Paragraph { han: true };
        assert_eq!(element_margins(&[latin, latin], 0), (0.0, SPACE));
        assert_eq!(element_margins(&[latin, latin], 1), (13.0, 13.0));
        assert_eq!(element_margins(&[han, han, latin], 1), (13.0, 13.0));
        assert_eq!(
            element_margins(&[han, latin, ElementKind::List], 1),
            (13.0, 13.0)
        );
        assert_eq!(
            element_margins(&[ElementKind::List, latin], 1),
            (SPACE * 2.0, 0.0)
        );
    }

    #[test]
    fn segment_breaks_between_wide_characters_disappear() {
        assert_eq!(Styled::plain("甲\n乙 c\n d").collapsed().text(), "甲乙 c d");
        assert_eq!(Styled::plain("  a  \n\n b ").collapsed().text(), "a b");
    }

    #[test]
    fn strong_runs_keep_their_emphasis() {
        let text = Styled::from_inlines(&[
            MarkdownInline::Text("a ".into()),
            MarkdownInline::Strong(vec![MarkdownInline::Text("bold".into())]),
        ]);
        assert_eq!(
            text.runs(),
            vec![
                (2, Emphasis::default()),
                (
                    4,
                    Emphasis {
                        bold: true,
                        italic: false
                    }
                )
            ]
        );
    }
}
