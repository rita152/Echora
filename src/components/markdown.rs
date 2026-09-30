use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    ops::Range,
    path::Path,
    str::FromStr,
    sync::OnceLock,
};

use gpui::{
    ClipboardItem, Div, FontStyle, FontWeight, Rgba, SharedString, StrikethroughStyle, StyledText,
    TextAlign, TextRun, UnderlineStyle, div, prelude::*, px,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag};
use two_face::{
    re_exports::syntect::{
        easy::ScopeRegionIterator,
        highlighting::ScopeSelectors,
        parsing::{MatchPower, ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet},
        util::LinesWithEndings,
    },
    syntax::extra_newlines,
};

use crate::{
    components::icons::icon,
    theme::{Theme, UI_MONOSPACE_FONT_FAMILY, ui_font},
};

mod fade;
pub(crate) mod find;
mod highlight;
use highlight::highlighted_code_spans;
mod preview;
mod repair;
pub(crate) use fade::cubic_bezier_ease;
use fade::fade_segments;
pub use fade::{MarkdownFadeHandle, STREAMING_BLOCK_FADE_DURATION, STREAMING_TEXT_FADE_DURATION};
mod selection;
pub use preview::MarkdownPreview;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownDocument {
    pub blocks: Vec<MarkdownBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MarkdownBlock {
    Image {
        destination: String,
        alt: String,
        dimensions: Option<(u32, u32)>,
    },
    Paragraph(Vec<MarkdownInline>),
    Heading {
        level: u8,
        content: Vec<MarkdownInline>,
    },
    List {
        start: Option<u64>,
        items: Vec<MarkdownListItem>,
    },
    BlockQuote(Vec<MarkdownBlock>),
    HorizontalRule,
    Table {
        alignments: Vec<MarkdownAlignment>,
        header: Vec<MarkdownTableCell>,
        rows: Vec<Vec<MarkdownTableCell>>,
    },
    CodeBlock {
        language: Option<String>,
        code: String,
        fenced: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<MarkdownBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownTableCell {
    pub content: Vec<MarkdownInline>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkdownAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MarkdownInline {
    Text(String),
    Strong(Vec<MarkdownInline>),
    Emphasis(Vec<MarkdownInline>),
    Strikethrough(Vec<MarkdownInline>),
    Code(String),
    Link {
        destination: String,
        title: String,
        content: Vec<MarkdownInline>,
    },
    SoftBreak,
    HardBreak,
}

struct RawFrame {
    tag: Tag<'static>,
    children: Vec<RawNode>,
}

enum RawNode {
    Element {
        tag: Tag<'static>,
        children: Vec<RawNode>,
    },
    Text(String),
    Code(String),
    SoftBreak,
    HardBreak,
    Rule,
    TaskListMarker(bool),
}

pub fn parse_markdown(source: &str) -> MarkdownDocument {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let mut roots = Vec::new();
    let mut stack: Vec<RawFrame> = Vec::new();

    for event in Parser::new_ext(source, options).map(Event::into_static) {
        match event {
            Event::Start(tag) => stack.push(RawFrame {
                tag,
                children: Vec::new(),
            }),
            Event::End(end) => {
                if let Some(frame) = stack.pop() {
                    debug_assert_eq!(frame.tag.to_end(), end);
                    push_raw(
                        &mut roots,
                        &mut stack,
                        RawNode::Element {
                            tag: frame.tag,
                            children: frame.children,
                        },
                    );
                }
            }
            Event::Text(text)
            | Event::Html(text)
            | Event::InlineHtml(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text)
            | Event::FootnoteReference(text) => {
                push_raw(&mut roots, &mut stack, RawNode::Text(text.into_string()));
            }
            Event::Code(code) => {
                push_raw(&mut roots, &mut stack, RawNode::Code(code.into_string()));
            }
            Event::SoftBreak => push_raw(&mut roots, &mut stack, RawNode::SoftBreak),
            Event::HardBreak => push_raw(&mut roots, &mut stack, RawNode::HardBreak),
            Event::Rule => push_raw(&mut roots, &mut stack, RawNode::Rule),
            Event::TaskListMarker(checked) => {
                push_raw(&mut roots, &mut stack, RawNode::TaskListMarker(checked));
            }
        }
    }

    while let Some(frame) = stack.pop() {
        push_raw(
            &mut roots,
            &mut stack,
            RawNode::Element {
                tag: frame.tag,
                children: frame.children,
            },
        );
    }

    MarkdownDocument {
        blocks: raw_nodes_to_blocks(roots),
    }
}

fn push_raw(roots: &mut Vec<RawNode>, stack: &mut [RawFrame], node: RawNode) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        roots.push(node);
    }
}

fn raw_nodes_to_blocks(nodes: Vec<RawNode>) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let mut pending_inline = Vec::new();

    for node in nodes {
        if is_block_node(&node) {
            flush_pending_paragraph(&mut pending_inline, &mut blocks);
            append_block(node, &mut blocks);
        } else {
            pending_inline.push(node);
        }
    }
    flush_pending_paragraph(&mut pending_inline, &mut blocks);
    blocks
}

fn flush_pending_paragraph(pending: &mut Vec<RawNode>, blocks: &mut Vec<MarkdownBlock>) {
    if !pending.is_empty() {
        let content = raw_nodes_to_inlines(std::mem::take(pending));
        if !content.is_empty() {
            blocks.push(MarkdownBlock::Paragraph(content));
        }
    }
}

fn is_block_node(node: &RawNode) -> bool {
    match node {
        RawNode::Rule => true,
        RawNode::Element { tag, .. } => matches!(
            tag,
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::BlockQuote(_)
                | Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::List(_)
                | Tag::Table(_)
                | Tag::FootnoteDefinition(_)
                | Tag::DefinitionList
                | Tag::DefinitionListTitle
                | Tag::DefinitionListDefinition
                | Tag::MetadataBlock(_)
        ),
        _ => false,
    }
}

fn append_block(node: RawNode, blocks: &mut Vec<MarkdownBlock>) {
    match node {
        RawNode::Rule => blocks.push(MarkdownBlock::HorizontalRule),
        RawNode::Element { tag, children } => match tag {
            Tag::Paragraph => {
                let mut pending = Vec::new();
                for child in children {
                    if let RawNode::Element {
                        tag: Tag::Image { dest_url, .. },
                        children,
                    } = child
                    {
                        flush_pending_paragraph(&mut pending, blocks);
                        let destination = dest_url.into_string();
                        let dimensions = markdown_image_dimensions(&destination);
                        blocks.push(MarkdownBlock::Image {
                            destination,
                            dimensions,
                            alt: collect_raw_text(children),
                        });
                    } else {
                        pending.push(child);
                    }
                }
                flush_pending_paragraph(&mut pending, blocks);
            }
            Tag::Heading { level, .. } => blocks.push(MarkdownBlock::Heading {
                level: level as u8,
                content: raw_nodes_to_inlines(children),
            }),
            Tag::BlockQuote(_) => {
                blocks.push(MarkdownBlock::BlockQuote(raw_nodes_to_blocks(children)))
            }
            Tag::CodeBlock(kind) => {
                let (language, fenced) = match kind {
                    CodeBlockKind::Indented => (None, false),
                    CodeBlockKind::Fenced(info) => (
                        info.split_whitespace().next().and_then(|language| {
                            (!language.is_empty()).then(|| language.to_owned())
                        }),
                        true,
                    ),
                };
                blocks.push(MarkdownBlock::CodeBlock {
                    language,
                    code: collect_raw_text(children),
                    fenced,
                });
            }
            Tag::List(start) => {
                let items = children
                    .into_iter()
                    .filter_map(|child| match child {
                        RawNode::Element {
                            tag: Tag::Item,
                            mut children,
                        } => {
                            let checked = take_task_marker(&mut children);
                            Some(MarkdownListItem {
                                checked,
                                blocks: raw_nodes_to_blocks(children),
                            })
                        }
                        _ => None,
                    })
                    .collect();
                blocks.push(MarkdownBlock::List { start, items });
            }
            Tag::Table(alignments) => {
                let alignments = alignments.into_iter().map(map_alignment).collect();
                let mut header = Vec::new();
                let mut rows = Vec::new();
                for child in children {
                    if let RawNode::Element { tag, children } = child {
                        match tag {
                            Tag::TableHead => header = table_cells(children),
                            Tag::TableRow => rows.push(table_cells(children)),
                            _ => {}
                        }
                    }
                }
                blocks.push(MarkdownBlock::Table {
                    alignments,
                    header,
                    rows,
                });
            }
            _ => blocks.extend(raw_nodes_to_blocks(children)),
        },
        other => {
            let content = raw_nodes_to_inlines(vec![other]);
            if !content.is_empty() {
                blocks.push(MarkdownBlock::Paragraph(content));
            }
        }
    }
}

// Markdown is reparsed during virtual-row rendering. Cache decoded dimensions
// by file version so scrolling never rereads entire image files each frame.
fn markdown_image_dimensions(destination: &str) -> Option<(u32, u32)> {
    type Version = (Option<std::time::SystemTime>, u64);
    type Cache = std::collections::HashMap<String, (Version, Option<(u32, u32)>)>;
    static DIMENSIONS: OnceLock<std::sync::Mutex<Cache>> = OnceLock::new();
    let path = Path::new(destination);
    if !path.is_absolute() {
        return None;
    }
    let metadata = path.metadata().ok()?;
    let version = (metadata.modified().ok(), metadata.len());
    let cache = DIMENSIONS.get_or_init(Default::default);
    if let Some((cached_version, dimensions)) = cache.lock().ok()?.get(destination)
        && *cached_version == version
    {
        return *dimensions;
    }
    let dimensions = crate::media::read_image_dimensions(path).ok().flatten();
    let mut cache = cache.lock().ok()?;
    if cache.len() >= 128 {
        cache.clear();
    }
    cache.insert(destination.to_owned(), (version, dimensions));
    dimensions
}

fn map_alignment(alignment: Alignment) -> MarkdownAlignment {
    match alignment {
        Alignment::None => MarkdownAlignment::None,
        Alignment::Left => MarkdownAlignment::Left,
        Alignment::Center => MarkdownAlignment::Center,
        Alignment::Right => MarkdownAlignment::Right,
    }
}

fn table_cells(nodes: Vec<RawNode>) -> Vec<MarkdownTableCell> {
    nodes
        .into_iter()
        .filter_map(|node| match node {
            RawNode::Element {
                tag: Tag::TableCell,
                children,
            } => Some(MarkdownTableCell {
                content: raw_nodes_to_inlines(children),
            }),
            _ => None,
        })
        .collect()
}

fn take_task_marker(nodes: &mut Vec<RawNode>) -> Option<bool> {
    let mut index = 0;
    while index < nodes.len() {
        if matches!(nodes[index], RawNode::TaskListMarker(_))
            && let RawNode::TaskListMarker(checked) = nodes.remove(index)
        {
            return Some(checked);
        }
        if let RawNode::Element { tag, children } = &mut nodes[index]
            && matches!(
                tag,
                Tag::Paragraph
                    | Tag::Strong
                    | Tag::Emphasis
                    | Tag::Strikethrough
                    | Tag::Link { .. }
            )
            && let Some(checked) = take_task_marker(children)
        {
            return Some(checked);
        }
        index += 1;
    }
    None
}

fn collect_raw_text(nodes: Vec<RawNode>) -> String {
    let mut text = String::new();
    for node in nodes {
        match node {
            RawNode::Text(value) | RawNode::Code(value) => text.push_str(&value),
            RawNode::SoftBreak | RawNode::HardBreak => text.push('\n'),
            RawNode::Element { children, .. } => text.push_str(&collect_raw_text(children)),
            RawNode::Rule => text.push_str("---"),
            RawNode::TaskListMarker(checked) => {
                text.push_str(if checked { "[x] " } else { "[ ] " });
            }
        }
    }
    text
}

fn raw_nodes_to_inlines(nodes: Vec<RawNode>) -> Vec<MarkdownInline> {
    let mut inlines = Vec::new();
    for node in nodes {
        match node {
            RawNode::Text(text) => push_inline(&mut inlines, MarkdownInline::Text(text)),
            RawNode::Code(code) => inlines.push(MarkdownInline::Code(code)),
            RawNode::SoftBreak => inlines.push(MarkdownInline::SoftBreak),
            RawNode::HardBreak => inlines.push(MarkdownInline::HardBreak),
            RawNode::Rule => push_inline(&mut inlines, MarkdownInline::Text("—".to_owned())),
            RawNode::TaskListMarker(checked) => push_inline(
                &mut inlines,
                MarkdownInline::Text(if checked { "[x] " } else { "[ ] " }.to_owned()),
            ),
            RawNode::Element { tag, children } => match tag {
                Tag::Strong => inlines.push(MarkdownInline::Strong(raw_nodes_to_inlines(children))),
                Tag::Emphasis => {
                    inlines.push(MarkdownInline::Emphasis(raw_nodes_to_inlines(children)))
                }
                Tag::Strikethrough => inlines.push(MarkdownInline::Strikethrough(
                    raw_nodes_to_inlines(children),
                )),
                Tag::Link {
                    dest_url, title, ..
                } => inlines.push(MarkdownInline::Link {
                    destination: dest_url.into_string(),
                    title: title.into_string(),
                    content: raw_nodes_to_inlines(children),
                }),
                Tag::Image {
                    dest_url, title, ..
                } => inlines.push(MarkdownInline::Link {
                    destination: dest_url.into_string(),
                    title: title.into_string(),
                    content: raw_nodes_to_inlines(children),
                }),
                _ => {
                    for inline in raw_nodes_to_inlines(children) {
                        push_inline(&mut inlines, inline);
                    }
                }
            },
        }
    }
    inlines
}

fn push_inline(inlines: &mut Vec<MarkdownInline>, inline: MarkdownInline) {
    if let MarkdownInline::Text(text) = inline {
        if let Some(MarkdownInline::Text(previous)) = inlines.last_mut() {
            previous.push_str(&text);
        } else if !text.is_empty() {
            inlines.push(MarkdownInline::Text(text));
        }
    } else {
        inlines.push(inline);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MarkdownLayout {
    base_size: f32,
    base_line_height: f32,
    /// `--markdown-space` (a quarter of the font size): the unit of every
    /// block margin and heading line box.
    markdown_space: f32,
    paragraph_space: f32,
    heading_top: f32,
    list_padding: f32,
    list_item_padding: f32,
    quote_bottom: f32,
    quote_padding_y: f32,
    quote_padding_left: f32,
    quote_line_height: f32,
    quote_bar_width: f32,
    rule_margin: f32,
    inline_code_size: f32,
    inline_code_flow_height: f32,
    inline_code_line_height: f32,
    inline_code_padding_x: f32,
    inline_code_padding_y: f32,
    inline_code_radius: f32,
    code_margin: f32,
    code_radius: f32,
    code_header_size: f32,
    code_header_line_height: f32,
    code_header_padding_x: f32,
    code_header_padding_right: f32,
    code_header_padding_y: f32,
    code_body_padding_x: f32,
    code_body_padding_bottom: f32,
    code_size: f32,
    code_line_height: f32,
    table_size: f32,
    table_line_height: f32,
    table_header_line_height: f32,
    table_header_padding_y: f32,
    table_cell_padding_y: f32,
    table_cell_padding_right: f32,
    table_header_last_padding_right: f32,
    table_body_last_padding_bottom: f32,
    table_min_width: f32,
    table_cell_max_width: f32,
}

/// ChatGPT's assistant Markdown root resolves `font-weight: 430`. On macOS
/// Chromium selects PingFangSC-Medium for Chinese at that weight, so using
/// GPUI's 400-weight `NORMAL` produces visibly lighter paragraphs even when
/// the font family, size, and line height all match.
const CHATGPT_MARKDOWN_BODY_WEIGHT: FontWeight = crate::theme::UI_BODY_FONT_WEIGHT;

const CHATGPT_MARKDOWN_LAYOUT: MarkdownLayout = MarkdownLayout {
    base_size: 14.0,
    base_line_height: 22.75,
    markdown_space: 3.5,
    paragraph_space: 4.0,
    heading_top: 14.0,
    list_padding: 22.75,
    list_item_padding: 5.25,
    quote_bottom: 7.0,
    quote_padding_y: 7.0,
    quote_padding_left: 21.0,
    quote_line_height: 21.0,
    quote_bar_width: 3.5,
    rule_margin: 24.5,
    inline_code_size: 12.88,
    // CDP 2026-09-05: the 22.75px inherited line box becomes 23.75px
    // whenever inline code or an inline mention contributes its 1px vertical
    // padding. Its painted inline box is only 17px tall, so model the flow
    // height separately instead of stretching the gray capsule.
    inline_code_flow_height: 23.75,
    inline_code_line_height: 15.0,
    inline_code_padding_x: 6.0,
    inline_code_padding_y: 1.0,
    inline_code_radius: 6.0,
    code_margin: 17.5,
    code_radius: 20.0,
    code_header_size: 13.0,
    code_header_line_height: 18.5714,
    code_header_padding_x: 20.0,
    code_header_padding_right: 6.0,
    code_header_padding_y: 6.0,
    code_body_padding_x: 20.0,
    code_body_padding_bottom: 12.0,
    code_size: 12.0,
    code_line_height: 20.0,
    table_size: 12.25,
    table_line_height: 22.75,
    table_header_line_height: 14.0,
    table_header_padding_y: 7.0,
    table_cell_padding_y: 8.75,
    table_cell_padding_right: 21.0,
    table_header_last_padding_right: 35.0,
    table_body_last_padding_bottom: 21.0,
    table_min_width: 736.0,
    table_cell_max_width: 576.0,
};

#[derive(Clone, Copy, Debug, PartialEq)]
struct MarkdownPalette {
    text: Rgba,
    link: Rgba,
    file_link: Rgba,
    inline_code_text: Rgba,
    inline_code_surface: Rgba,
    code_surface: Rgba,
    code_header_surface: Rgba,
    code_border: Rgba,
    syntax_comment: Rgba,
    syntax_keyword: Rgba,
    syntax_literal: Rgba,
    syntax_string: Rgba,
    syntax_variable: Rgba,
    syntax_attribute: Rgba,
    syntax_name: Rgba,
    syntax_error: Rgba,
    action_hover: Rgba,
    blockquote_border: Rgba,
    table_border_strong: Rgba,
    table_border_subtle: Rgba,
    table_header_surface: Rgba,
    rule: Rgba,
}

/// `-webkit-line-clamp` over a root block sequence: the blocks after `block`
/// are dropped, and that block, when it is a paragraph of plain prose, ends
/// its `lines`-th line with an ellipsis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkdownLineClamp {
    pub block: usize,
    pub lines: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MarkdownRenderStyle {
    selectable: bool,
    layout: MarkdownLayout,
    /// Body text weight (430 for chat and pull request prose).
    body_weight: FontWeight,
    palette: MarkdownPalette,
    line_clamp: Option<MarkdownLineClamp>,
    /// Lines a plain paragraph shows before its ellipsis.
    paragraph_lines: Option<usize>,
    /// Root prose keeps the chat's readable measure, centered; pull request
    /// prose fills its column.
    centered_measure: bool,
}

impl MarkdownRenderStyle {
    fn new(theme: Theme) -> Self {
        Self {
            selectable: false,
            body_weight: CHATGPT_MARKDOWN_BODY_WEIGHT,
            layout: CHATGPT_MARKDOWN_LAYOUT,
            line_clamp: None,
            paragraph_lines: None,
            centered_measure: true,
            palette: MarkdownPalette {
                text: theme.markdown_text,
                link: theme.markdown_link,
                file_link: theme.markdown_file_link,
                inline_code_text: theme.markdown_inline_code_text,
                inline_code_surface: theme.markdown_inline_code_surface,
                code_surface: theme.markdown_code_surface,
                code_header_surface: theme.markdown_code_header_surface,
                code_border: theme.markdown_code_border,
                syntax_comment: theme.markdown_syntax_comment,
                syntax_keyword: theme.markdown_syntax_keyword,
                syntax_literal: theme.markdown_syntax_literal,
                syntax_string: theme.markdown_syntax_string,
                syntax_variable: theme.markdown_syntax_variable,
                syntax_attribute: theme.markdown_syntax_attribute,
                syntax_name: theme.markdown_syntax_name,
                syntax_error: theme.markdown_syntax_error,
                action_hover: theme.markdown_action_hover,
                blockquote_border: theme.markdown_blockquote_border,
                table_border_strong: theme.markdown_table_border_strong,
                table_border_subtle: theme.markdown_table_border_subtle,
                table_header_surface: theme.markdown_table_header_surface,
                rule: theme.markdown_rule,
            },
        }
    }
}

#[derive(Clone, Copy)]
enum SequenceContext {
    Root,
    BlockQuote,
    ListItem,
}

/// Flatten a document into the plain paragraphs the navigation rail's hover
/// card prints. The card repeats at most three lines of the response, so the
/// preview carries the text of each block in order; block-level decoration
/// (list markers, tables, code gutters) stays out of it.
pub fn render_assistant_markdown(source: &str, theme: Theme, message_scope: &str) -> Div {
    let document = parse_markdown(source);
    render_markdown_document(
        &document,
        theme,
        markdown_hash(message_scope),
        &MarkdownFadeHandle::none(),
    )
}

/// The assistant message that is still arriving. Every newly revealed word,
/// decoration and block fades in through `fade`, as ChatGPT's animated
/// markdown root does until the message completes.
pub fn render_streaming_assistant_markdown(
    source: &str,
    theme: Theme,
    message_scope: &str,
    fade: &MarkdownFadeHandle,
) -> Div {
    let document = parse_markdown(&repair::repair_streaming_markdown(source));
    render_markdown_document(&document, theme, markdown_hash(message_scope), fade)
}

/// Pull request descriptions and review comments: the reference renders this
/// surface with a 21px body line box instead of the chat view's 22.75px.
pub fn render_pull_request_markdown(source: &str, theme: Theme, scope: &str) -> Div {
    // The reference hides HTML comments in pull request prose: the Codex review
    // summaries embed machine-readable `<!-- ... -->` blocks that must not
    // reach the rendered body.
    let source = strip_html_comments(source);
    let document = parse_markdown(&source);
    let mut style = MarkdownRenderStyle::new(theme);
    style.layout.base_line_height = PULL_REQUEST_BODY_LINE_HEIGHT;
    // `p { margin-bottom: var(--markdown-space) }` before a list or code.
    style.layout.paragraph_space = style.layout.markdown_space;
    style.centered_measure = false;
    style.body_weight = PULL_REQUEST_BODY_WEIGHT;
    style.layout.quote_line_height = PULL_REQUEST_BODY_LINE_HEIGHT;
    render_block_sequence(
        &document.blocks,
        style,
        0,
        SequenceContext::Root,
        markdown_hash(scope),
        &MarkdownFadeHandle::none(),
    )
    .w_full()
    .min_w(px(0.0))
    .text_size(px(style.layout.base_size))
    .line_height(px(PULL_REQUEST_BODY_LINE_HEIGHT))
    .font(pull_request_font())
    .font_weight(PULL_REQUEST_BODY_WEIGHT)
    .text_color(style.palette.text)
}

/// Pull request comment bodies (`textStyle: {kind: "small"}`): 13px type on a
/// 21.125px line box, 1em between paragraphs, and 11.96px inline code.
/// `line_clamp` applies the collapsed body's `line-clamp-6`.
pub fn render_pull_request_comment_markdown(
    source: &str,
    theme: Theme,
    scope: &str,
    line_clamp: Option<MarkdownLineClamp>,
) -> Div {
    let source = strip_html_comments(source);
    let mut style = small_markdown_style(theme);
    style.line_clamp = line_clamp;
    render_small_markdown(&source, style, scope)
}

/// Settings notices (config warnings) use the same small text style, in the
/// banner's text color: their inline code inherits it instead of the
/// assistant's code color. Every newline is a line break, as the reference
/// renders the multi-line summary.
pub fn render_notice_markdown(source: &str, theme: Theme, scope: &str, text: Rgba) -> Div {
    let mut style = small_markdown_style(theme);
    style.palette.text = text;
    style.palette.inline_code_text = text;
    render_small_markdown(&source.replace('\n', "\\\n"), style, scope)
}

fn small_markdown_style(theme: Theme) -> MarkdownRenderStyle {
    let mut style = MarkdownRenderStyle::new(theme);
    style.centered_measure = false;
    // Every block metric derives from the 13px `--markdown-space` (3.25px)
    // and `--markdown-line-height` (1.625em).
    let space = PULL_REQUEST_COMMENT_SIZE / 4.0;
    style.layout.base_size = PULL_REQUEST_COMMENT_SIZE;
    style.layout.base_line_height = PULL_REQUEST_COMMENT_LINE_HEIGHT;
    style.layout.markdown_space = space;
    style.layout.paragraph_space = space;
    style.layout.heading_top = space * 4.0;
    style.layout.list_padding = PULL_REQUEST_COMMENT_LINE_HEIGHT;
    style.layout.list_item_padding = space * 1.5;
    style.layout.quote_bottom = space * 2.0;
    style.layout.quote_padding_y = space * 2.0;
    style.layout.quote_padding_left = space * 6.0;
    style.layout.quote_bar_width = space;
    style.layout.quote_line_height = PULL_REQUEST_COMMENT_LINE_HEIGHT;
    style.layout.rule_margin = space * 7.0;
    style.layout.code_margin = space * 5.0;
    style.layout.inline_code_size = 11.96;
    style.layout.inline_code_flow_height = PULL_REQUEST_COMMENT_LINE_HEIGHT + 1.0;
    style.layout.inline_code_line_height = 14.0;
    style.body_weight = PULL_REQUEST_BODY_WEIGHT;
    style
}

fn render_small_markdown(source: &str, style: MarkdownRenderStyle, scope: &str) -> Div {
    let document = parse_markdown(source);
    render_block_sequence(
        &document.blocks,
        style,
        0,
        SequenceContext::Root,
        markdown_hash(scope),
        &MarkdownFadeHandle::none(),
    )
    .w_full()
    .min_w(px(0.0))
    .text_size(px(PULL_REQUEST_COMMENT_SIZE))
    .line_height(px(PULL_REQUEST_COMMENT_LINE_HEIGHT))
    .font(pull_request_font())
    .font_weight(PULL_REQUEST_BODY_WEIGHT)
    .text_color(style.palette.text)
}

pub const PULL_REQUEST_COMMENT_SIZE: f32 = 13.0;
pub const PULL_REQUEST_COMMENT_LINE_HEIGHT: f32 = 21.125;

/// Font for pull request prose. The reference resolves Chinese runs through the
/// system cascade to `.PingFangUI…` (a 0.9587em ideograph), while an explicit
/// `PingFang SC` fallback resolves the text variant at a full 1em advance and
/// makes every mixed CJK line 3–4% too wide. Dropping the explicit fallback
/// lets CoreText's own cascade pick the face the reference uses.
fn pull_request_font() -> gpui::Font {
    let mut font = ui_font();
    font.fallbacks = None;
    font
}

/// Reference body line box for pull request prose. The 2026-09-18 CDP capture
/// of both the description (`div`/`p`/`li` nodes) and the activity comment
/// paragraphs resolves `font-size: 14px; line-height: 22.75px`, the same
/// inherited line box the chat surface uses.
pub const PULL_REQUEST_BODY_LINE_HEIGHT: f32 = 22.75;
/// Removes `<!-- ... -->` blocks, which the reference never renders.
fn strip_html_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find("<!--") {
        output.push_str(&rest[..start]);
        match rest[start + 4..].find("-->") {
            Some(end) => rest = &rest[start + 4 + end + 3..],
            None => return output,
        }
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod strip_html_comments_tests {
    use super::strip_html_comments;

    #[test]
    fn removes_comment_blocks_and_keeps_surrounding_prose() {
        assert_eq!(
            strip_html_comments("before\n<!-- hidden\nspanning -->\nafter"),
            "before\n\nafter"
        );
        assert_eq!(strip_html_comments("no comments"), "no comments");
        assert_eq!(strip_html_comments("open <!-- never closed"), "open ");
    }
}

/// Pull request prose weight: the reference's `font-weight: 430`, the same
/// `.SF NS` instance (and `.PingFangUIDisplaySC-Regular` for Chinese) the chat
/// body uses. A lighter instance narrows Latin text enough to change where
/// review comments wrap.
pub const PULL_REQUEST_BODY_WEIGHT: gpui::FontWeight = crate::theme::UI_BODY_FONT_WEIGHT;

pub fn render_selectable_plan(source: &str, theme: Theme, scope: &str) -> Div {
    let document = parse_markdown(source);
    render_selectable_markdown_document(&document, theme, scope)
}

pub fn render_selectable_markdown_document(
    document: &MarkdownDocument,
    theme: Theme,
    scope: &str,
) -> Div {
    let mut style = MarkdownRenderStyle::new(theme);
    style.selectable = true;
    render_block_sequence(
        &document.blocks,
        style,
        0,
        SequenceContext::Root,
        markdown_hash(scope),
        &MarkdownFadeHandle::none(),
    )
    .w_full()
    .min_w(px(0.0))
    .text_size(px(style.layout.base_size))
    .line_height(px(style.layout.base_line_height))
    .font(ui_font())
    .font_weight(CHATGPT_MARKDOWN_BODY_WEIGHT)
    .text_color(style.palette.text)
}

fn render_markdown_document(
    document: &MarkdownDocument,
    theme: Theme,
    identity_seed: u64,
    fade: &MarkdownFadeHandle,
) -> Div {
    let style = MarkdownRenderStyle::new(theme);
    render_block_sequence(
        &document.blocks,
        style,
        0,
        SequenceContext::Root,
        identity_seed,
        fade,
    )
    .w_full()
    .min_w(px(0.0))
    .text_size(px(style.layout.base_size))
    .line_height(px(style.layout.base_line_height))
    .font(ui_font())
    .font_weight(CHATGPT_MARKDOWN_BODY_WEIGHT)
    .text_color(style.palette.text)
}

fn render_block_sequence(
    blocks: &[MarkdownBlock],
    style: MarkdownRenderStyle,
    list_depth: usize,
    context: SequenceContext,
    identity_seed: u64,
    fade: &MarkdownFadeHandle,
) -> Div {
    let mut sequence = div().w_full().min_w(px(0.0)).flex().flex_col();
    let mut previous: Option<&MarkdownBlock> = None;
    let mut previous_bottom = 0.0_f32;

    let line_clamp = style
        .line_clamp
        .filter(|_| matches!(context, SequenceContext::Root));
    for (index, block) in blocks.iter().enumerate() {
        if line_clamp.is_some_and(|clamp| index > clamp.block) {
            break;
        }
        let mut block_style = style;
        block_style.line_clamp = None;
        block_style.paragraph_lines = line_clamp
            .filter(|clamp| clamp.block == index && matches!(block, MarkdownBlock::Paragraph(_)))
            .map(|clamp| clamp.lines);
        let style = block_style;
        let block_identity = markdown_hash(&(identity_seed, index));
        let (top, bottom) = block_margins(block, previous, index == 0, style.layout, context);
        let collapsed_gap = if index == 0 {
            0.0
        } else {
            previous_bottom.max(top)
        };
        sequence = sequence.child(
            div()
                .debug_selector(move || format!("markdown-block-{block_identity}"))
                .w_full()
                .min_w(px(0.0))
                // Prose keeps its readable measure while root tables can use
                // the surrounding conversation width, as in the desktop app.
                .when(
                    style.centered_measure
                        && matches!(context, SequenceContext::Root)
                        && !matches!(block, MarkdownBlock::Table { .. }),
                    |element| element.max_w(px(style.layout.table_min_width)).mx_auto(),
                )
                .when(collapsed_gap > 0.0, |element| element.mt(px(collapsed_gap)))
                .child(render_block(
                    block,
                    style,
                    list_depth,
                    context,
                    block_identity,
                    fade,
                )),
        );
        previous = Some(block);
        previous_bottom = bottom;
    }
    sequence
}

fn block_margins(
    block: &MarkdownBlock,
    previous: Option<&MarkdownBlock>,
    is_first: bool,
    layout: MarkdownLayout,
    context: SequenceContext,
) -> (f32, f32) {
    let default = match block {
        MarkdownBlock::Image { .. } => (12.0, 12.0),
        MarkdownBlock::Paragraph(_) => (0.0, layout.paragraph_space),
        MarkdownBlock::Heading { level: 1, .. } => (0.0, layout.markdown_space * 2.0),
        MarkdownBlock::Heading { level: 2 | 3, .. } => (layout.heading_top, layout.markdown_space),
        MarkdownBlock::Heading { level: 4, .. } => (layout.heading_top, 0.0),
        MarkdownBlock::Heading { .. } => (0.0, 0.0),
        MarkdownBlock::List { .. } | MarkdownBlock::Table { .. } => (0.0, 0.0),
        MarkdownBlock::BlockQuote(_) => (0.0, layout.quote_bottom),
        MarkdownBlock::HorizontalRule => (layout.rule_margin, layout.rule_margin),
        MarkdownBlock::CodeBlock { .. } => (layout.code_margin, layout.code_margin),
    };

    let (mut top, mut bottom) = match context {
        SequenceContext::Root => match block {
            MarkdownBlock::Paragraph(_)
                if matches!(previous, Some(MarkdownBlock::Paragraph(_))) =>
            {
                (layout.markdown_space * 4.0, layout.markdown_space * 4.0)
            }
            // `h1`–`h3 + p` and `h4 + p` reset the paragraph's top margin,
            // leaving only the heading's own bottom margin.
            MarkdownBlock::Paragraph(_)
                if matches!(previous, Some(MarkdownBlock::Heading { level: 1..=4, .. })) =>
            {
                (0.0, default.1)
            }
            MarkdownBlock::Paragraph(_) if !is_first => (layout.markdown_space * 2.0, default.1),
            _ => default,
        },
        SequenceContext::BlockQuote => match block {
            MarkdownBlock::Paragraph(_) => (0.0, 0.0),
            _ => default,
        },
        SequenceContext::ListItem => match block {
            MarkdownBlock::Paragraph(_)
                if matches!(previous, Some(MarkdownBlock::Paragraph(_))) =>
            {
                (layout.markdown_space * 4.0, 0.0)
            }
            MarkdownBlock::Paragraph(_) | MarkdownBlock::List { .. } => (0.0, 0.0),
            _ => default,
        },
    };
    if is_first {
        top = 0.0;
    }
    if !bottom.is_finite() {
        bottom = 0.0;
    }
    (top, bottom)
}

fn render_block(
    block: &MarkdownBlock,
    style: MarkdownRenderStyle,
    list_depth: usize,
    context: SequenceContext,
    block_identity: u64,
    fade: &MarkdownFadeHandle,
) -> Div {
    match block {
        MarkdownBlock::Image {
            destination,
            alt,
            dimensions,
        } => {
            let path = std::path::PathBuf::from(destination);
            // Local transcript artifacts use the native image cache; remote
            // references keep the normal URL loading behavior.
            let source = if path.is_absolute() {
                gpui::ImageSource::from(path.clone())
            } else {
                gpui::ImageSource::from(SharedString::from(destination.clone()))
            };
            let destination = destination.clone();
            div().child(
                div()
                    .id(markdown_element_id("markdown-image", &block_identity))
                    .max_w_full()
                    .flex()
                    .items_start()
                    .cursor_pointer()
                    .role(gpui::Role::Button)
                    .aria_label(crate::i18n::format!("打开图片：{alt}" => "Open image: {alt}"))
                    .on_click(move |_, _, cx| {
                        if path.is_absolute() {
                            cx.open_with_system(&path);
                        } else {
                            cx.open_url(&destination);
                        }
                    })
                    .child(
                        gpui::img(source)
                            .when_some(*dimensions, |image, (width, height)| {
                                let scale = (160.0 / height.max(1) as f32).min(1.0);
                                image
                                    .w(px(width as f32 * scale))
                                    .h(px(height as f32 * scale))
                            })
                            .max_w_full()
                            .max_h(px(160.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(style.palette.table_border_strong)
                            .object_fit(gpui::ObjectFit::Contain),
                    ),
            )
        }
        MarkdownBlock::Paragraph(content) => render_inline_block(
            content,
            style,
            style.layout.base_size,
            if matches!(context, SequenceContext::BlockQuote) {
                style.layout.quote_line_height
            } else {
                style.layout.base_line_height
            },
            style.body_weight,
            block_identity,
            fade,
        ),
        MarkdownBlock::Heading { level, content } => {
            // `h1`–`h4` scale the body size (1.5, 1.25, 1.125, 1em) on line
            // boxes of 8, 7, 7 and 6 `--markdown-space`.
            let (base, space) = (style.layout.base_size, style.layout.markdown_space);
            let (size, line_height) = match level {
                1 => (base * 1.5, space * 8.0),
                2 => (base * 1.25, space * 7.0),
                3 => (base * 1.125, space * 7.0),
                4 => (base, space * 6.0),
                _ => (base, style.layout.base_line_height),
            };
            render_inline_block(
                content,
                style,
                size,
                line_height,
                FontWeight::SEMIBOLD,
                block_identity,
                fade,
            )
        }
        MarkdownBlock::List { start, items } => {
            render_list(*start, items, style, list_depth, block_identity, fade)
        }
        MarkdownBlock::BlockQuote(blocks) => div()
            .relative()
            .w_full()
            .min_w(px(0.0))
            .when_some(
                fade.block_alpha(block_identity, STREAMING_BLOCK_FADE_DURATION),
                |quote, alpha| quote.opacity(alpha),
            )
            .pl(px(style.layout.quote_padding_left))
            .py(px(style.layout.quote_padding_y))
            .line_height(px(style.layout.quote_line_height))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(style.layout.quote_padding_y))
                    .bottom(px(style.layout.quote_padding_y))
                    .w(px(style.layout.quote_bar_width))
                    .rounded(px(style.layout.quote_bar_width / 2.0))
                    .bg(style.palette.blockquote_border),
            )
            .child(render_block_sequence(
                blocks,
                style,
                list_depth,
                SequenceContext::BlockQuote,
                block_identity,
                fade,
            )),
        MarkdownBlock::HorizontalRule => div()
            .w_full()
            .h_0()
            .border_t_1()
            .border_color(style.palette.rule)
            .when_some(
                fade.block_alpha(block_identity, STREAMING_BLOCK_FADE_DURATION),
                |rule, alpha| rule.opacity(alpha),
            ),
        MarkdownBlock::CodeBlock { language, code, .. } => {
            render_code_block(language.as_deref(), code, style, block_identity)
        }
        MarkdownBlock::Table {
            alignments,
            header,
            rows,
        } => render_table(alignments, header, rows, style, block_identity, fade),
    }
}

fn render_list(
    start: Option<u64>,
    items: &[MarkdownListItem],
    style: MarkdownRenderStyle,
    depth: usize,
    block_identity: u64,
    fade: &MarkdownFadeHandle,
) -> Div {
    let is_task_list = list_uses_task_layout(start, items);
    let mut list = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .when(!is_task_list, |list| list.pl(px(style.layout.list_padding)));

    for (index, item) in items.iter().enumerate() {
        let marker = if let Some(checked) = item.checked {
            if checked {
                "☑".to_owned()
            } else {
                "☐".to_owned()
            }
        } else if let Some(start) = start {
            format!("{}.", start.saturating_add(index as u64))
        } else {
            match depth % 3 {
                0 => "•".to_owned(),
                1 => "◦".to_owned(),
                _ => "▪".to_owned(),
            }
        };
        let marker_left = if is_task_list {
            0.0
        } else {
            -style.layout.list_padding
        };
        let content_padding = if is_task_list {
            style.layout.list_padding
        } else {
            style.layout.list_item_padding
        };
        let item_identity = markdown_hash(&(block_identity, index));
        // ChatGPT fades a streaming list item in as a block over the basic
        // transition, while its marker follows the slower text fade.
        list = list.child(
            div()
                .relative()
                .w_full()
                .min_w(px(0.0))
                .pl(px(content_padding))
                .when_some(
                    fade.block_alpha(item_identity, STREAMING_BLOCK_FADE_DURATION),
                    |item, alpha| item.opacity(alpha),
                )
                .child(
                    div()
                        .absolute()
                        .when_some(
                            fade.block_alpha(item_identity, STREAMING_TEXT_FADE_DURATION),
                            |marker, alpha| marker.opacity(alpha),
                        )
                        .left(px(marker_left))
                        .top(px(if item.checked.is_some() {
                            style.layout.paragraph_space
                        } else {
                            0.0
                        }))
                        .w(px(style.layout.list_padding))
                        .pr(px(style.layout.list_item_padding))
                        .text_right()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(style.layout.base_size))
                        .line_height(px(style.layout.base_line_height))
                        .child(marker),
                )
                .child(render_block_sequence(
                    &item.blocks,
                    style,
                    depth + 1,
                    SequenceContext::ListItem,
                    item_identity,
                    fade,
                )),
        );
    }
    list
}

fn list_uses_task_layout(start: Option<u64>, items: &[MarkdownListItem]) -> bool {
    if start.is_none() {
        items.iter().any(|item| item.checked.is_some())
    } else {
        !items.is_empty() && items.iter().all(|item| item.checked.is_some())
    }
}

fn render_inline_block(
    content: &[MarkdownInline],
    style: MarkdownRenderStyle,
    font_size: f32,
    line_height: f32,
    font_weight: FontWeight,
    inline_identity: u64,
    fade: &MarkdownFadeHandle,
) -> Div {
    fade.begin_inline(inline_identity);
    if requires_inline_boxes(content) {
        render_inline_boxes(
            content,
            style,
            font_size,
            line_height,
            font_weight,
            inline_identity,
            fade,
        )
    } else {
        div()
            .w_full()
            .min_w(px(0.0))
            .text_size(px(font_size))
            .line_height(px(line_height))
            .font_weight(font_weight)
            .when_some(style.paragraph_lines, |paragraph, lines| {
                paragraph.line_clamp(lines).text_ellipsis()
            })
            .child(if style.selectable {
                selection::selectable(
                    inline_identity,
                    render_styled_text(content, style, font_weight, fade),
                )
                .into_any_element()
            } else {
                render_styled_text(content, style, font_weight, fade).into_any_element()
            })
    }
}

fn requires_inline_boxes(inlines: &[MarkdownInline]) -> bool {
    inlines.iter().any(|inline| match inline {
        MarkdownInline::Code(_) | MarkdownInline::Link { .. } => true,
        MarkdownInline::Strong(children)
        | MarkdownInline::Emphasis(children)
        | MarkdownInline::Strikethrough(children) => requires_inline_boxes(children),
        _ => false,
    })
}

#[derive(Clone, Copy, Default)]
struct InlineState {
    strong: bool,
    emphasis: bool,
    strikethrough: bool,
    code: bool,
    link: bool,
}

fn render_styled_text(
    inlines: &[MarkdownInline],
    style: MarkdownRenderStyle,
    base_weight: FontWeight,
    fade: &MarkdownFadeHandle,
) -> StyledText {
    render_styled_text_with_state(inlines, style, base_weight, InlineState::default(), fade)
}

fn render_styled_text_with_state(
    inlines: &[MarkdownInline],
    style: MarkdownRenderStyle,
    base_weight: FontWeight,
    state: InlineState,
    fade: &MarkdownFadeHandle,
) -> StyledText {
    let mut text = String::new();
    let mut runs = Vec::new();
    append_inline_runs(
        inlines,
        state,
        style,
        base_weight,
        &mut text,
        &mut runs,
        fade,
    );
    let runs = find::highlight_runs(&text, runs);
    StyledText::new(text).with_runs(runs)
}

fn append_inline_runs(
    inlines: &[MarkdownInline],
    state: InlineState,
    style: MarkdownRenderStyle,
    base_weight: FontWeight,
    text: &mut String,
    runs: &mut Vec<TextRun>,
    fade: &MarkdownFadeHandle,
) {
    for inline in inlines {
        match inline {
            MarkdownInline::Text(value) => {
                append_text_run(value, state, style, base_weight, text, runs, fade)
            }
            MarkdownInline::Code(value) => {
                let mut next = state;
                next.code = true;
                append_text_run(value, next, style, base_weight, text, runs, fade);
            }
            MarkdownInline::SoftBreak => {
                append_text_run(" ", state, style, base_weight, text, runs, fade)
            }
            MarkdownInline::HardBreak => {
                append_text_run("\n", state, style, base_weight, text, runs, fade)
            }
            MarkdownInline::Strong(children) => {
                let mut next = state;
                next.strong = true;
                append_inline_runs(children, next, style, base_weight, text, runs, fade);
            }
            MarkdownInline::Emphasis(children) => {
                let mut next = state;
                next.emphasis = true;
                append_inline_runs(children, next, style, base_weight, text, runs, fade);
            }
            MarkdownInline::Strikethrough(children) => {
                let mut next = state;
                next.strikethrough = true;
                append_inline_runs(children, next, style, base_weight, text, runs, fade);
            }
            MarkdownInline::Link { content, .. } => {
                let mut next = state;
                next.link = true;
                append_inline_runs(content, next, style, base_weight, text, runs, fade);
            }
        }
    }
}

fn append_text_run(
    value: &str,
    state: InlineState,
    style: MarkdownRenderStyle,
    base_weight: FontWeight,
    text: &mut String,
    runs: &mut Vec<TextRun>,
    fade: &MarkdownFadeHandle,
) {
    if value.is_empty() {
        return;
    }
    text.push_str(value);
    let mut font = ui_font();
    if state.code {
        font.family = UI_MONOSPACE_FONT_FAMILY.into();
    }
    font.weight = if state.strong {
        FontWeight::SEMIBOLD
    } else {
        base_weight
    };
    font.style = if state.emphasis {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    let color = if state.link {
        style.palette.link
    } else if state.code {
        style.palette.inline_code_text
    } else {
        style.palette.text
    };
    let run = |len: usize, color: Rgba| TextRun {
        len,
        font: font.clone(),
        color: color.into(),
        background_color: state.code.then(|| style.palette.inline_code_surface.into()),
        underline: None,
        strikethrough: state.strikethrough.then_some(StrikethroughStyle {
            thickness: px(1.0),
            color: None,
        }),
    };
    // Breaks and other whitespace-only runs join the word before them in the
    // reference, so they neither count as a segment nor fade on their own.
    if fade.is_active() && !value.chars().all(char::is_whitespace) {
        for segment in fade_segments(value) {
            let alpha = fade.segment_alpha(segment).unwrap_or(1.0);
            runs.push(run(segment.len(), faded(color, alpha)));
        }
    } else {
        runs.push(run(value.len(), color));
    }
}

fn faded(color: Rgba, alpha: f32) -> Rgba {
    Rgba {
        a: color.a * alpha,
        ..color
    }
}

/// Plain text of an inline-box fragment while the message streams: one run
/// per fade segment, carrying the fragment's own weight, slant and color so
/// the runs paint exactly like the element's inherited text style.
fn fading_fragment_text(
    label: String,
    state: InlineState,
    weight: FontWeight,
    color: Rgba,
    fade: &MarkdownFadeHandle,
) -> StyledText {
    let mut font = ui_font();
    font.weight = weight;
    font.style = if state.emphasis {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    let runs = fade_segments(&label)
        .into_iter()
        .map(|segment| TextRun {
            len: segment.len(),
            font: font.clone(),
            color: faded(color, fade.segment_alpha(segment).unwrap_or(1.0)).into(),
            background_color: None,
            underline: None,
            strikethrough: state.strikethrough.then_some(StrikethroughStyle {
                thickness: px(1.0),
                color: None,
            }),
        })
        .collect::<Vec<_>>();
    let runs = find::highlight_runs(&label, runs);
    StyledText::new(label).with_runs(runs)
}

struct InlineFragment {
    text: String,
    state: InlineState,
    hard_break: bool,
    trailing_space: bool,
    link_destination: Option<String>,
    link_content: Option<Vec<MarkdownInline>>,
    /// Whether this piece of an inline code span starts or ends the span.
    /// Blink breaks inside a code span, slicing its padding and radius.
    code_opens: bool,
    code_closes: bool,
}

fn render_inline_boxes(
    inlines: &[MarkdownInline],
    style: MarkdownRenderStyle,
    font_size: f32,
    line_height: f32,
    base_weight: FontWeight,
    inline_identity: u64,
    fade: &MarkdownFadeHandle,
) -> Div {
    // ChatGPT uses .92em here; table cells inherit a smaller font than prose.
    let scale = font_size / style.layout.base_size;
    let code_size = style.layout.inline_code_size * scale;
    let code_paint_height = (style.layout.inline_code_line_height * scale).round();
    let mut fragments = Vec::new();
    append_inline_fragments(inlines, InlineState::default(), &mut fragments);
    let glued = (0..fragments.len())
        .map(|index| index > 0 && glued_to_previous(&fragments[index - 1], &fragments[index]))
        .collect::<Vec<_>>();
    // The pieces of one code span fade in as one unit.
    let mut code_alpha = None;
    let mut render = |index: usize, fragment: InlineFragment| -> gpui::AnyElement {
        if fragment.hard_break {
            return div().w_full().h_0().into_any_element();
        }
        let file_reference = fragment
            .link_destination
            .as_deref()
            .and_then(markdown_file_reference_path);
        let github_reference = fragment
            .link_destination
            .as_deref()
            .is_some_and(|url| url.starts_with("https://github.com/"));
        let has_icon = file_reference.is_some() || github_reference;
        let linked_inline_code = matches!(
            fragment.link_content.as_deref(),
            Some([MarkdownInline::Code(_)])
        );
        let expands_line_box = fragment.state.code || has_icon || linked_inline_code;
        let color = if file_reference.is_some() {
            style.palette.file_link
        } else if fragment.state.link {
            style.palette.link
        } else if fragment.state.code {
            style.palette.inline_code_text
        } else {
            style.palette.text
        };
        let weight = if fragment.state.strong {
            FontWeight::SEMIBOLD
        } else if fragment.state.link {
            FontWeight::MEDIUM
        } else {
            base_weight
        };
        // ChatGPT fades a code span or link in as one unit and every word
        // of plain text on its own.
        let is_decoration =
            fragment.state.code || linked_inline_code || fragment.link_destination.is_some();
        let decoration_alpha = if fragment.state.code && !fragment.code_opens {
            code_alpha
        } else if is_decoration {
            fade.next_segment_alpha()
        } else {
            None
        };
        if fragment.state.code {
            code_alpha = decoration_alpha;
        }
        let mut element = div()
            .flex_none()
            .max_w_full()
            .when_some(decoration_alpha, |element, alpha| element.opacity(alpha))
            .font_weight(weight)
            .text_color(color)
            .when(fragment.state.emphasis, |element| element.italic())
            .when(fragment.state.strikethrough, |element| {
                element.line_through()
            })
            .when(expands_line_box, |element| {
                element.flex().items_center().child(line_box_strut(
                    line_height + style.layout.inline_code_flow_height
                        - style.layout.base_line_height,
                ))
            })
            .when(!fragment.state.code, |element| {
                element
                    .text_size(px(font_size))
                    .line_height(px(line_height))
            })
            .when(has_icon, |element| {
                element.px(px(2.0)).flex().items_center()
            })
            .when(fragment.trailing_space, |element| {
                element.mr(px(font_size * 0.25))
            });
        if has_icon {
            element = element.child(
                icon(
                    file_reference
                        .map(markdown_file_reference_icon)
                        .unwrap_or("markdown-github"),
                    color.into(),
                )
                .size(px(16.0))
                .flex_none()
                .mr(px(3.0)),
            );
        }
        let text = if let Some(link_content) = fragment.link_content.as_deref()
            && file_reference.is_none()
        {
            render_styled_text_with_state(
                link_content,
                style,
                FontWeight::MEDIUM,
                fragment.state,
                &MarkdownFadeHandle::none(),
            )
        } else {
            let label = if file_reference.is_some() {
                markdown_file_reference_label(
                    &fragment.text,
                    fragment.link_destination.as_deref().unwrap_or_default(),
                )
            } else {
                fragment.text.clone()
            };
            if fade.is_active() && !is_decoration {
                fading_fragment_text(label, fragment.state, weight, color, fade)
            } else {
                StyledText::new(label)
            }
        };
        let element = if fragment.state.code || linked_inline_code {
            let (opens, closes) = if linked_inline_code {
                (true, true)
            } else {
                (fragment.code_opens, fragment.code_closes)
            };
            let code = match fragment.link_content.as_deref() {
                Some([MarkdownInline::Code(code)]) => code.clone(),
                _ => fragment.text,
            };
            element.child(
                div()
                    .min_w(px(0.0))
                    .max_w_full()
                    .whitespace_normal()
                    .font_family(UI_MONOSPACE_FONT_FAMILY)
                    .font_weight(weight)
                    .text_color(color)
                    .text_size(px(code_size))
                    .line_height(px(code_paint_height))
                    .when(opens, |piece| {
                        piece
                            .pl(px(style.layout.inline_code_padding_x))
                            .rounded_l(px(style.layout.inline_code_radius))
                    })
                    .when(closes, |piece| {
                        piece
                            .pr(px(style.layout.inline_code_padding_x))
                            .rounded_r(px(style.layout.inline_code_radius))
                    })
                    .py(px(style.layout.inline_code_padding_y))
                    .bg(style.palette.inline_code_surface)
                    .child(code),
            )
        } else {
            element.child(
                div()
                    .min_w(px(0.0))
                    .when(has_icon, |e| e.flex_1())
                    .child(text),
            )
        };
        if let Some(destination) = fragment.link_destination {
            let link_id = markdown_element_id(
                "markdown-link",
                &(inline_identity, index, destination.as_str()),
            );
            let file_reference = markdown_file_reference_path(&destination).map(str::to_owned);
            element
                .id(link_id)
                .cursor_pointer()
                .hover(|element| element.underline())
                .on_click(move |_, window, cx| {
                    if let Some(file_reference) = &file_reference {
                        let line = destination
                            .rsplit_once(":")
                            .and_then(|(_, n)| n.parse().ok())
                            .or_else(|| {
                                destination
                                    .rsplit_once("#L")
                                    .and_then(|(_, n)| n.parse().ok())
                            });
                        window.dispatch_action(
                            Box::new(super::file_panel::OpenWorkspaceFile {
                                path: file_reference.clone(),
                                line,
                            }),
                            cx,
                        );
                    } else {
                        cx.open_url(&destination);
                    }
                })
                .into_any_element()
        } else {
            element.into_any_element()
        }
    };
    let mut groups: Vec<Vec<gpui::AnyElement>> = Vec::new();
    for (index, fragment) in fragments.into_iter().enumerate() {
        let element = render(index, fragment);
        match groups.last_mut() {
            Some(group) if glued[index] => group.push(element),
            _ => groups.push(vec![element]),
        }
    }
    // A code span grows its line box downwards (23.75px for 22.75px prose):
    // Blink keeps the line's baseline where plain text puts it, so fragments
    // hang from the top of the line instead of centering in it.
    groups.into_iter().fold(
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start(),
        |line, mut group| {
            if group.len() == 1
                && let Some(element) = group.pop()
            {
                return line.child(element);
            }
            // Fragments without a break opportunity between them wrap as one.
            line.child(
                div()
                    .flex_none()
                    .max_w_full()
                    .flex()
                    .flex_row()
                    .items_start()
                    .children(group),
            )
        },
    )
}

/// A zero-width text line that gives an inline box its taller line box. An
/// authored `min_h` would be snapped to the device-pixel grid before layout
/// (23.75px becomes 23.5px at 2x) and pull every following line up, while a
/// measured text height keeps its fractional size.
fn line_box_strut(height: f32) -> Div {
    div()
        .flex_none()
        .w(px(0.0))
        .overflow_hidden()
        .line_height(px(height))
        .child("\u{200B}")
}

/// Whether Blink keeps `next` on the line of `previous`: nothing separates
/// them and their junction is no line-break opportunity. An inline code span
/// (`unicode-bidi: isolate`) never takes a break before it, and the pieces of
/// one span were already split at its opportunities; other junctions follow
/// [`line_break_offsets`] across the two fragments, so closing punctuation
/// stays with the code span or emphasis it follows.
fn glued_to_previous(previous: &InlineFragment, next: &InlineFragment) -> bool {
    if previous.hard_break || next.hard_break || previous.trailing_space {
        return false;
    }
    if next.state.code {
        return next.code_opens;
    }
    let (Some(last), Some(first)) = (previous.text.chars().last(), next.text.chars().next()) else {
        return false;
    };
    let junction = format!("{last}{first}");
    !line_break_offsets(&junction).contains(&last.len_utf8())
}

fn code_piece(text: &str, state: InlineState, opens: bool, closes: bool) -> InlineFragment {
    InlineFragment {
        text: text.to_owned(),
        state,
        hard_break: false,
        trailing_space: false,
        link_destination: None,
        link_content: None,
        code_opens: opens,
        code_closes: closes,
    }
}

/// Line-break opportunities as Blink finds them (see
/// [`gpui::line_break_opportunities`]), shared with GPUI's text wrapping.
fn line_break_offsets(text: &str) -> Vec<usize> {
    gpui::line_break_opportunities(text)
}

fn append_inline_fragments(
    inlines: &[MarkdownInline],
    state: InlineState,
    fragments: &mut Vec<InlineFragment>,
) {
    for inline in inlines {
        match inline {
            MarkdownInline::Text(text) => {
                // Word boundaries allow closing CJK punctuation to start a
                // line. Use Blink's line-break opportunities so punctuation
                // stays with its preceding text.
                let mut start = 0;
                for end in line_break_offsets(text) {
                    let segment = &text[start..end];
                    start = end;
                    let word = segment.trim_end_matches(char::is_whitespace);
                    if word.chars().all(char::is_whitespace) {
                        if let Some(previous) = fragments.last_mut() {
                            previous.trailing_space = true;
                        }
                    } else {
                        fragments.push(InlineFragment {
                            text: word.to_owned(),
                            state,
                            hard_break: false,
                            trailing_space: word.len() < segment.len(),
                            link_destination: None,
                            link_content: None,
                            code_opens: false,
                            code_closes: false,
                        });
                    }
                }
            }
            MarkdownInline::Code(text) => {
                let mut next = state;
                next.code = true;
                let breaks = line_break_offsets(text);
                let mut start = 0;
                for end in breaks.into_iter().filter(|&end| end < text.len()) {
                    fragments.push(code_piece(&text[start..end], next, start == 0, false));
                    start = end;
                }
                fragments.push(code_piece(&text[start..], next, start == 0, true));
            }
            MarkdownInline::SoftBreak => {
                if let Some(previous) = fragments.last_mut() {
                    previous.trailing_space = true;
                }
            }
            MarkdownInline::HardBreak => fragments.push(InlineFragment {
                text: String::new(),
                state,
                hard_break: true,
                trailing_space: false,
                link_destination: None,
                link_content: None,
                code_opens: false,
                code_closes: false,
            }),
            MarkdownInline::Strong(children) => {
                let mut next = state;
                next.strong = true;
                append_inline_fragments(children, next, fragments);
            }
            MarkdownInline::Emphasis(children) => {
                let mut next = state;
                next.emphasis = true;
                append_inline_fragments(children, next, fragments);
            }
            MarkdownInline::Strikethrough(children) => {
                let mut next = state;
                next.strikethrough = true;
                append_inline_fragments(children, next, fragments);
            }
            MarkdownInline::Link {
                destination,
                content,
                ..
            } => {
                let mut next = state;
                next.link = true;
                fragments.push(InlineFragment {
                    text: inline_plain_text(content),
                    state: next,
                    hard_break: false,
                    trailing_space: false,
                    link_destination: Some(destination.clone()),
                    link_content: Some(content.clone()),
                    code_opens: false,
                    code_closes: false,
                });
            }
        }
    }
}

pub(crate) fn inline_plain_text(inlines: &[MarkdownInline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        match inline {
            MarkdownInline::Text(value) | MarkdownInline::Code(value) => text.push_str(value),
            MarkdownInline::SoftBreak => text.push(' '),
            MarkdownInline::HardBreak => text.push('\n'),
            MarkdownInline::Strong(children)
            | MarkdownInline::Emphasis(children)
            | MarkdownInline::Strikethrough(children)
            | MarkdownInline::Link {
                content: children, ..
            } => text.push_str(&inline_plain_text(children)),
        }
    }
    text
}

fn markdown_file_reference_path(destination: &str) -> Option<&str> {
    if !Path::new(destination).is_absolute() {
        return None;
    }
    let destination = destination
        .rsplit_once("#L")
        .filter(|(_, line)| !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()))
        .map_or(destination, |(path, _)| path);
    Some(
        destination
            .rsplit_once(':')
            .filter(|(_, line)| line.bytes().all(|byte| byte.is_ascii_digit()))
            .map_or(destination, |(path, _)| path),
    )
}

fn markdown_file_reference_label(label: &str, destination: &str) -> String {
    match destination.rsplit_once(':') {
        Some((_, line))
            if !line.is_empty()
                && line.bytes().all(|byte| byte.is_ascii_digit())
                && !label.contains("(line ") =>
        {
            format!("{label} (line {line})")
        }
        _ => label.to_owned(),
    }
}

fn markdown_file_reference_icon(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("py" | "pyi" | "pyw") => "markdown-file-python",
        Some("rs") => "markdown-file-rust",
        Some("json" | "jsonl") => "markdown-file-json",
        _ => "markdown-file-document",
    }
}

pub(super) fn render_tool_text(text: &str, theme: Theme, identity: &str) -> Div {
    render_code_block(
        None,
        text,
        MarkdownRenderStyle::new(theme),
        markdown_hash(&identity),
    )
}

fn render_code_block(
    language: Option<&str>,
    code: &str,
    style: MarkdownRenderStyle,
    block_identity: u64,
) -> Div {
    let code = code.strip_suffix('\n').unwrap_or(code).to_owned();
    let code_for_copy = code.clone();
    let code_text = highlighted_code_text(&code, language, style);
    let scroll_id = markdown_element_id("markdown-code-scroll", &block_identity);
    let copy_id = markdown_element_id("markdown-code-copy", &block_identity);
    let wrap_id = markdown_element_id("markdown-code-wrap", &block_identity);
    let language_label = code_language_label(language);
    let border_width = 1.0;
    let header_radius = (style.layout.code_radius - border_width).max(0.0);

    div()
        .w_full()
        .min_w(px(0.0))
        .overflow_hidden()
        .rounded(px(style.layout.code_radius))
        .border(px(border_width))
        .border_color(style.palette.code_border)
        .bg(style.palette.code_surface)
        .child(
            div()
                .w_full()
                .min_h(px(48.0))
                .relative()
                .pl(px(style.layout.code_header_padding_x))
                .pr(px(style.layout.code_header_padding_right))
                .py(px(style.layout.code_header_padding_y))
                // GPUI overflow masks are rectangular. Round both header
                // backgrounds explicitly to follow the outer border's inset.
                .rounded_t(px(header_radius))
                .bg(style.palette.code_header_surface)
                .flex()
                .items_center()
                .justify_between()
                .font_weight(FontWeight::MEDIUM)
                .text_size(px(style.layout.code_header_size))
                .line_height(px(style.layout.code_header_line_height))
                // The live header paints a solid theme surface plus the same
                // translucent gradient used by the code body. Modeling both
                // layers avoids the misleading computed background-color and
                // reproduces the final #f4f4f4 / #242424 pixels.
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded_t(px(header_radius))
                        .bg(style.palette.code_surface),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(icon("markdown-code", style.palette.text.into()).size(px(20.0)))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(language_label),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(1.0))
                        // ChatGPT always reserves this 36px action before copy.
                        // Wrapping is intentionally left disabled because this
                        // stateless display node mirrors its default CDP state.
                        .child(
                            div()
                                .id(wrap_id)
                                .size(px(36.0))
                                .rounded(px(10.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .hover(move |button| button.bg(style.palette.action_hover))
                                .child(
                                    icon("markdown-wrap", style.palette.text.into()).size(px(20.0)),
                                ),
                        )
                        .child(
                            div()
                                .id(copy_id)
                                .size(px(36.0))
                                .rounded(px(10.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(move |button| button.bg(style.palette.action_hover))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        code_for_copy.clone(),
                                    ));
                                })
                                .child(
                                    icon("markdown-copy", style.palette.text.into()).size(px(20.0)),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .id(scroll_id)
                .w_full()
                .min_w(px(0.0))
                .px(px(style.layout.code_body_padding_x))
                .pb(px(style.layout.code_body_padding_bottom))
                .flex()
                .overflow_x_scroll()
                .restrict_scroll_to_axis()
                .scrollbar_width(px(0.0))
                .text_size(px(style.layout.code_size))
                .line_height(px(style.layout.code_line_height))
                .font_family(UI_MONOSPACE_FONT_FAMILY)
                .whitespace_nowrap()
                .child(div().flex_none().child(code_text)),
        )
}

fn code_language(language: Option<&str>) -> Option<String> {
    language
        .and_then(|language| language.split_ascii_whitespace().next())
        .map(str::trim)
        .filter(|language| !language.is_empty())
        .map(|language| language.to_ascii_lowercase())
}

fn code_language_label(language: Option<&str>) -> String {
    let Some(raw_language) = language
        .and_then(|language| language.split_ascii_whitespace().next())
        .map(str::trim)
        .filter(|language| !language.is_empty())
    else {
        return crate::i18n::text("纯文本").to_owned();
    };
    match raw_language.to_ascii_lowercase().as_str() {
        "text" | "txt" | "plaintext" | "plain" => crate::i18n::text("纯文本").to_owned(),
        "bash" | "sh" | "zsh" => "Bash".to_owned(),
        "fish" => "Fish".to_owned(),
        "arduino" => "Arduino".to_owned(),
        "c" => "C".to_owned(),
        "cpp" | "c++" => "C++".to_owned(),
        "csharp" | "c#" | "cs" => "C#".to_owned(),
        "diff" => "Diff".to_owned(),
        "dart" => "Dart".to_owned(),
        "dockerfile" | "docker" => "Dockerfile".to_owned(),
        "elixir" => "Elixir".to_owned(),
        "erlang" => "Erlang".to_owned(),
        "go" | "golang" => "Go".to_owned(),
        "graphql" => "GraphQL".to_owned(),
        "haskell" => "Haskell".to_owned(),
        "ini" => "INI".to_owned(),
        "java" => "Java".to_owned(),
        "js" | "javascript" | "jsx" => "JavaScript".to_owned(),
        "ts" | "typescript" | "tsx" => "TypeScript".to_owned(),
        "kotlin" | "kt" => "Kotlin".to_owned(),
        "latex" | "tex" => "LaTeX".to_owned(),
        "less" => "Less".to_owned(),
        "lua" => "Lua".to_owned(),
        "makefile" | "make" => "Makefile".to_owned(),
        "objectivec" | "objective-c" | "objc" => "Objective-C".to_owned(),
        "perl" => "Perl".to_owned(),
        "php" => "PHP".to_owned(),
        "php-template" => "PHP template".to_owned(),
        "powershell" | "ps1" => "PowerShell".to_owned(),
        "py" | "python" => "Python".to_owned(),
        "python-repl" | "pycon" => "Python REPL".to_owned(),
        "r" => "R".to_owned(),
        "rb" | "ruby" => "Ruby".to_owned(),
        "rs" | "rust" => "Rust".to_owned(),
        "scala" => "Scala".to_owned(),
        "scss" => "SCSS".to_owned(),
        "shell" => "Shell".to_owned(),
        "swift" => "Swift".to_owned(),
        "json" => "JSON".to_owned(),
        "yaml" | "yml" => "YAML".to_owned(),
        "toml" => "TOML".to_owned(),
        "css" => "CSS".to_owned(),
        "sql" => "SQL".to_owned(),
        "html" | "xml" => "XML".to_owned(),
        "md" | "markdown" => "Markdown".to_owned(),
        "vbnet" | "vb" => "Visual Basic .NET".to_owned(),
        "wasm" | "webassembly" => "WebAssembly".to_owned(),
        _ => raw_language.to_owned(),
    }
}

const MAX_HIGHLIGHTED_CODE_BYTES: usize = 256 * 1024;
const MAX_HIGHLIGHTED_LINE_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodeSyntaxToken {
    Plain,
    Comment,
    Keyword,
    Literal,
    String,
    Variable,
    Attribute,
    Name,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CodeSyntaxStyle {
    token: CodeSyntaxToken,
    italic: bool,
    bold: bool,
    underline: bool,
}

impl CodeSyntaxStyle {
    const PLAIN: Self = Self {
        token: CodeSyntaxToken::Plain,
        italic: false,
        bold: false,
        underline: false,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodeHighlightSpan {
    range: Range<usize>,
    style: CodeSyntaxStyle,
}

struct CodeScopeClassifiers {
    tokens: Vec<(ScopeSelectors, CodeSyntaxToken)>,
    shell_plain: [Scope; 2],
    italic: ScopeSelectors,
    bold: ScopeSelectors,
    underline: ScopeSelectors,
}

fn code_syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(extra_newlines)
}

fn code_scope_classifiers() -> &'static CodeScopeClassifiers {
    static CLASSIFIERS: OnceLock<CodeScopeClassifiers> = OnceLock::new();
    CLASSIFIERS.get_or_init(|| CodeScopeClassifiers {
        // Sublime/TextMate scopes do not have highlight.js's exact class names.
        // These groups preserve ChatGPT's eight semantic theme buckets while
        // allowing the grammar's more-specific nested scope to win.
        tokens: [
            ("comment", CodeSyntaxToken::Comment),
            (
                "keyword, storage, punctuation.definition.keyword",
                CodeSyntaxToken::Keyword,
            ),
            (
                "constant.numeric, constant.language, support.function, support.class, support.type, entity.name.type.class",
                CodeSyntaxToken::Literal,
            ),
            ("string, regexp, markup.inserted", CodeSyntaxToken::String),
            (
                "variable, support.variable, entity.name.function, entity.name.class, entity.other.inherited-class",
                CodeSyntaxToken::Variable,
            ),
            (
                "entity.other.attribute-name, support.type.property-name, markup.heading",
                CodeSyntaxToken::Attribute,
            ),
            (
                "entity.name.tag, constant.other.symbol, markup.list, meta.preprocessor",
                CodeSyntaxToken::Name,
            ),
            ("invalid, markup.deleted", CodeSyntaxToken::Error),
        ]
        .into_iter()
        .map(|(selector, token)| {
            (
                ScopeSelectors::from_str(selector).expect("valid Markdown syntax selector"),
                token,
            )
        })
        .collect(),
        // TextMate treats every shell command name and option as a variable.
        // highlight.js (and the live ChatGPT Bash block) leaves ordinary
        // commands such as `ssh -t` in the base foreground instead.
        shell_plain: [
            Scope::new("variable.function.shell").expect("valid shell command scope"),
            Scope::new("variable.parameter.option.shell").expect("valid shell option scope"),
        ],
        italic: ScopeSelectors::from_str("comment, markup.italic")
            .expect("valid Markdown italic selector"),
        bold: ScopeSelectors::from_str("markup.bold").expect("valid Markdown bold selector"),
        underline: ScopeSelectors::from_str("markup.underline.link")
            .expect("valid Markdown underline selector"),
    })
}

fn code_syntax(language: Option<&str>) -> Option<&'static SyntaxReference> {
    let language = code_language(language)?;
    let token = match language.as_str() {
        "text" | "txt" | "plaintext" | "plain" => return None,
        "arduino" => "cpp",
        "bash" | "sh" | "zsh" => "sh",
        "c#" | "csharp" => "cs",
        "c++" => "cpp",
        "docker" => "Dockerfile",
        "gql" => "graphql",
        "golang" => "go",
        "html" => "xml",
        "javascript" => "js",
        "kt" => "kotlin",
        "make" => "Makefile",
        "markdown" => "md",
        "objectivec" | "objective-c" | "objc" => "Objective-C",
        "patch" => "diff",
        "php-template" => "php",
        "python-repl" | "pycon" => "python",
        "shell" => "Shell-Unix-Generic",
        "typescript" => "ts",
        "vb" | "vbnet" | "wasm" | "webassembly" => return None,
        "yml" => "yaml",
        _ => language.as_str(),
    };
    code_syntax_set().find_syntax_by_token(token)
}

fn code_scope_style(stack: &ScopeStack) -> CodeSyntaxStyle {
    let classifiers = code_scope_classifiers();
    let mut strongest: Option<(MatchPower, usize, CodeSyntaxToken)> = None;
    for (index, (selector, token)) in classifiers.tokens.iter().enumerate() {
        let Some(power) = selector.does_match(stack.as_slice()) else {
            continue;
        };
        if strongest
            .as_ref()
            .is_none_or(|(best_power, best_index, _)| {
                power > *best_power || (power == *best_power && index > *best_index)
            })
        {
            strongest = Some((power, index, *token));
        }
    }

    let token = if stack.as_slice().iter().any(|scope| {
        classifiers
            .shell_plain
            .iter()
            .any(|plain| plain.is_prefix_of(*scope))
    }) {
        CodeSyntaxToken::Plain
    } else {
        strongest
            .map(|(_, _, token)| token)
            .unwrap_or(CodeSyntaxToken::Plain)
    };
    CodeSyntaxStyle {
        token,
        italic: token == CodeSyntaxToken::Comment
            || classifiers.italic.does_match(stack.as_slice()).is_some(),
        bold: classifiers.bold.does_match(stack.as_slice()).is_some(),
        underline: classifiers.underline.does_match(stack.as_slice()).is_some(),
    }
}

fn push_code_span(spans: &mut Vec<CodeHighlightSpan>, range: Range<usize>, style: CodeSyntaxStyle) {
    if range.is_empty() {
        return;
    }
    if let Some(previous) = spans.last_mut()
        && previous.range.end == range.start
        && previous.style == style
    {
        previous.range.end = range.end;
    } else {
        spans.push(CodeHighlightSpan { range, style });
    }
}

fn highlighted_code_text(
    code: &str,
    language: Option<&str>,
    style: MarkdownRenderStyle,
) -> StyledText {
    let mut base_font = ui_font();
    base_font.family = UI_MONOSPACE_FONT_FAMILY.into();
    base_font.weight = CHATGPT_MARKDOWN_BODY_WEIGHT;
    let spans = highlighted_code_spans(code, language).unwrap_or_else(|| {
        (!code.is_empty())
            .then_some(CodeHighlightSpan {
                range: 0..code.len(),
                style: CodeSyntaxStyle::PLAIN,
            })
            .into_iter()
            .collect()
    });
    let runs = spans
        .into_iter()
        .map(|span| {
            code_text_run(
                span.range.len(),
                base_font.clone(),
                span.style,
                style.palette,
            )
        })
        .collect();
    let runs = find::highlight_runs(code, runs);
    StyledText::new(code.to_owned()).with_runs(runs)
}

// Share the same syntax classifier and semantic palette with the native file editor.
/// The scope stack of every region of `code` (one parse, no cache), for
/// callers that style code from a TextMate theme instead of the app palette.
pub(crate) fn code_scope_regions(
    code: &str,
    language: Option<&str>,
) -> Option<Vec<(Range<usize>, ScopeStack)>> {
    if code.len() > MAX_HIGHLIGHTED_CODE_BYTES {
        return None;
    }
    let syntax = code_syntax(language)?;
    let mut parse_state = ParseState::new(syntax);
    let mut scope_stack = ScopeStack::new();
    let mut regions = Vec::new();
    let mut line_base = 0;
    for line in LinesWithEndings::from(code) {
        if line.len() > MAX_HIGHLIGHTED_LINE_BYTES {
            return None;
        }
        let operations = parse_state.parse_line(line, code_syntax_set()).ok()?;
        let mut cursor = 0;
        for (segment, operation) in ScopeRegionIterator::new(&operations, line) {
            scope_stack.apply(operation).ok()?;
            if !segment.is_empty() {
                regions.push((
                    line_base + cursor..line_base + cursor + segment.len(),
                    scope_stack.clone(),
                ));
            }
            cursor += segment.len();
        }
        line_base += line.len();
    }
    Some(regions)
}

pub fn file_editor_runs(
    code: &str,
    language: Option<&str>,
    theme: Theme,
) -> Vec<(Range<usize>, TextRun)> {
    let mut font = ui_font();
    font.family = UI_MONOSPACE_FONT_FAMILY.into();
    let mut palette = MarkdownRenderStyle::new(theme).palette;
    palette.text = theme.file_editor_text;
    highlighted_code_spans(code, language)
        .unwrap_or_else(|| {
            vec![CodeHighlightSpan {
                range: 0..code.len(),
                style: CodeSyntaxStyle::PLAIN,
            }]
        })
        .into_iter()
        .map(|s| {
            (
                s.range.clone(),
                code_text_run(s.range.len(), font.clone(), s.style, palette),
            )
        })
        .collect()
}

fn code_syntax_color(token: CodeSyntaxToken, palette: MarkdownPalette) -> Rgba {
    match token {
        CodeSyntaxToken::Plain => palette.text,
        CodeSyntaxToken::Comment => palette.syntax_comment,
        CodeSyntaxToken::Keyword => palette.syntax_keyword,
        CodeSyntaxToken::Literal => palette.syntax_literal,
        CodeSyntaxToken::String => palette.syntax_string,
        CodeSyntaxToken::Variable => palette.syntax_variable,
        CodeSyntaxToken::Attribute => palette.syntax_attribute,
        CodeSyntaxToken::Name => palette.syntax_name,
        CodeSyntaxToken::Error => palette.syntax_error,
    }
}

fn code_text_run(
    len: usize,
    mut font: gpui::Font,
    style: CodeSyntaxStyle,
    palette: MarkdownPalette,
) -> TextRun {
    let color = code_syntax_color(style.token, palette);
    if style.italic {
        font.style = FontStyle::Italic;
    }
    if style.bold {
        font.weight = FontWeight::BOLD;
    }
    TextRun {
        len,
        font,
        color: color.into(),
        background_color: None,
        underline: style.underline.then(|| UnderlineStyle {
            thickness: px(1.0),
            color: Some(color.into()),
            wavy: false,
        }),
        strikethrough: None,
    }
}

fn render_table(
    alignments: &[MarkdownAlignment],
    header: &[MarkdownTableCell],
    rows: &[Vec<MarkdownTableCell>],
    style: MarkdownRenderStyle,
    block_identity: u64,
    fade: &MarkdownFadeHandle,
) -> Div {
    div().w_full().min_w(px(0.0)).child(MarkdownTable {
        alignments: alignments.to_vec(),
        header: header.to_vec(),
        rows: rows.to_vec(),
        style,
        block_identity,
        fade: fade.clone(),
    })
}

#[derive(IntoElement)]
struct MarkdownTable {
    alignments: Vec<MarkdownAlignment>,
    header: Vec<MarkdownTableCell>,
    rows: Vec<Vec<MarkdownTableCell>>,
    style: MarkdownRenderStyle,
    block_identity: u64,
    fade: MarkdownFadeHandle,
}

impl gpui::RenderOnce for MarkdownTable {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let Self {
            alignments,
            header,
            rows,
            style,
            block_identity,
            fade,
        } = self;
        let widths = table_column_widths(
            &alignments,
            &header,
            &rows,
            style,
            block_identity,
            window,
            cx,
        );
        let column_count = widths.len();
        let scroll_id = markdown_element_id("markdown-table-scroll", &block_identity);
        let table_width = widths.iter().sum::<f32>().max(style.layout.table_min_width);
        let mut table = div()
            .debug_selector(move || format!("markdown-table-{block_identity}"))
            .w(px(table_width))
            .flex_none()
            .grid()
            .grid_cols_auto(column_count as u16)
            .text_size(px(style.layout.table_size))
            .line_height(px(style.layout.table_line_height));

        if !header.is_empty() {
            table = append_table_row(
                table,
                &header,
                style,
                TableRowPresentation {
                    is_header: true,
                    is_last_row: false,
                    row_identity: markdown_hash(&(block_identity, "header")),
                },
                column_count,
                &widths,
                &fade,
            );
        }
        for (index, row) in rows.iter().enumerate() {
            table = append_table_row(
                table,
                row,
                style,
                TableRowPresentation {
                    is_header: false,
                    is_last_row: index + 1 == rows.len(),
                    row_identity: markdown_hash(&(block_identity, index)),
                },
                column_count,
                &widths,
                &fade,
            );
        }

        // Keep the scroll viewport inside the available column. A fixed-width
        // scroller clips both ends on narrow windows and makes columns unreachable.
        div().w_full().min_w(px(0.0)).flex().justify_center().child(
            div()
                .id(scroll_id)
                .debug_selector(move || format!("markdown-table-viewport-{block_identity}"))
                .w(px(table_width))
                .max_w_full()
                .min_w(px(0.0))
                .flex_none()
                .overflow_x_scroll()
                .restrict_scroll_to_axis()
                .scrollbar_width(px(0.0))
                .child(table),
        )
    }
}

fn table_column_widths(
    alignments: &[MarkdownAlignment],
    header: &[MarkdownTableCell],
    rows: &[Vec<MarkdownTableCell>],
    style: MarkdownRenderStyle,
    block_identity: u64,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> Vec<f32> {
    let column_count = alignments
        .len()
        .max(header.len())
        .max(rows.iter().map(Vec::len).max().unwrap_or(0))
        .max(1)
        .min(u16::MAX as usize);
    let mut widths = vec![0.0_f32; column_count];
    for (row_index, cells) in std::iter::once(header)
        .chain(rows.iter().map(Vec::as_slice))
        .enumerate()
    {
        for (index, cell) in cells.iter().enumerate().take(column_count) {
            let header = row_index == 0;
            let padding = if index + 1 < column_count {
                style.layout.table_cell_padding_right
            } else if header {
                style.layout.table_header_last_padding_right
            } else {
                0.0
            };
            let mut content = render_inline_block(
                &cell.content,
                style,
                style.layout.table_size,
                if header {
                    style.layout.table_header_line_height
                } else {
                    style.layout.table_line_height
                },
                if header {
                    FontWeight::SEMIBOLD
                } else {
                    CHATGPT_MARKDOWN_BODY_WEIGHT
                },
                markdown_hash(&(block_identity, "measure", row_index, index)),
                &MarkdownFadeHandle::none(),
            )
            .w_auto()
            .font(ui_font())
            .into_any_element();
            let measured = content.layout_as_root(
                gpui::size(
                    gpui::AvailableSpace::MaxContent,
                    gpui::AvailableSpace::MaxContent,
                ),
                window,
                cx,
            );
            widths[index] = widths[index]
                .max((f32::from(measured.width) + padding).min(style.layout.table_cell_max_width));
        }
    }
    // CSS automatic table layout apportions spare width by intrinsic column
    // width. Grid's auto tracks add an equal amount to every column instead.
    let total: f32 = widths.iter().sum();
    if total > 0.0 && total < style.layout.table_min_width {
        for width in &mut widths {
            *width *= style.layout.table_min_width / total;
        }
    }
    widths
}

fn append_table_row(
    mut table: Div,
    cells: &[MarkdownTableCell],
    style: MarkdownRenderStyle,
    presentation: TableRowPresentation,
    column_count: usize,
    widths: &[f32],
    fade: &MarkdownFadeHandle,
) -> Div {
    let TableRowPresentation {
        is_header,
        is_last_row,
        row_identity,
    } = presentation;
    // The grid has no row element; every cell of a streaming row fades with
    // the row's own start time.
    let row_alpha = fade.block_alpha(row_identity, STREAMING_BLOCK_FADE_DURATION);
    for (index, width) in widths[..column_count].iter().copied().enumerate() {
        let cell = cells.get(index);
        let mut element = div()
            .debug_selector(move || format!("markdown-table-cell-{row_identity}-{index}"))
            .when_some(row_alpha, |cell, alpha| cell.opacity(alpha))
            .w(px(width))
            .min_w(px(0.0))
            .h_full()
            .pr(px(if index + 1 == column_count {
                if is_header {
                    style.layout.table_header_last_padding_right
                } else {
                    0.0
                }
            } else {
                style.layout.table_cell_padding_right
            }))
            .bg(if is_header {
                style.palette.table_header_surface
            } else {
                rgba_transparent()
            })
            .when(is_header, |element| {
                element
                    .py(px(style.layout.table_header_padding_y))
                    .border_b_1()
                    .border_color(style.palette.table_border_strong)
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(px(style.layout.table_header_line_height))
            })
            .when(!is_header, |element| {
                element
                    .pt(px(style.layout.table_cell_padding_y))
                    .pb(px(if is_last_row {
                        style.layout.table_body_last_padding_bottom
                    } else {
                        style.layout.table_cell_padding_y
                    }))
                    .when(!is_last_row, |element| {
                        element
                            .border_b_1()
                            .border_color(style.palette.table_border_subtle)
                    })
            });
        // The desktop reference left-aligns headers and values even when the
        // Markdown delimiter row contains center/right alignment markers.
        element = element.text_align(TextAlign::Left);
        if let Some(cell) = cell {
            let content = render_inline_block(
                &cell.content,
                style,
                style.layout.table_size,
                if is_header {
                    style.layout.table_header_line_height
                } else {
                    style.layout.table_line_height
                },
                if is_header {
                    FontWeight::SEMIBOLD
                } else {
                    CHATGPT_MARKDOWN_BODY_WEIGHT
                },
                markdown_hash(&(row_identity, index)),
                fade,
            );
            element = element.child(content.justify_start());
        }
        table = table.child(element);
    }
    table
}

fn rgba_transparent() -> Rgba {
    Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }
}

/// Runs the production Markdown renderer without requiring an app-server session.
#[cfg(feature = "screenshot")]
pub fn capture_markdown(args: &[String]) -> bool {
    use gpui::{
        App, AppContext, Bounds, Context, Render, Window, WindowBounds, WindowOptions, size,
    };
    let Some(path) = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--markdown-file="))
    else {
        return false;
    };
    let source = std::fs::read_to_string(path).expect("read Markdown capture source");
    let dark = args.iter().any(|arg| arg == "--theme=dark");
    let width = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--window-width=")?.parse::<f32>().ok())
        .unwrap_or(1000.0);
    let output = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--screenshot=").map(str::to_owned));
    struct Capture {
        source: String,
        dark: bool,
    }
    impl Render for Capture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let theme = Theme::for_mode(if self.dark {
                crate::theme::ThemeMode::Dark
            } else {
                crate::theme::ThemeMode::Light
            });
            div()
                .size_full()
                .bg(if self.dark {
                    gpui::rgb(0x181818)
                } else {
                    gpui::rgb(0xffffff)
                })
                .font(ui_font())
                .p(px(32.0))
                .child(
                    div()
                        .id("markdown-capture-scroll")
                        .size_full()
                        .overflow_y_scroll()
                        .restrict_scroll_to_axis()
                        .child(div().w_full().child(render_assistant_markdown(
                            &self.source,
                            theme,
                            "markdown-capture",
                        ))),
                )
        }
    }
    crate::typography::configure();
    let asset_status = crate::assets::status();
    if asset_status.is_missing() {
        eprintln!("{}", crate::assets::missing_warning(asset_status));
    }
    gpui_platform::application()
        .with_assets(crate::assets::Assets::load_from(asset_status))
        .run(move |cx: &mut App| {
            crate::typography::initialize_fonts(cx);
            let bounds = Bounds::centered(None, size(px(width), px(700.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    if let Some(output) = output {
                        crate::schedule_screenshot(window, output, 4);
                    }
                    cx.new(|_| Capture { source, dark })
                },
            )
            .expect("open Markdown capture");
            cx.activate(true);
        });
    true
}

fn markdown_element_id(prefix: &str, value: &impl Hash) -> SharedString {
    format!("{prefix}-{:016x}", markdown_hash(value)).into()
}

fn markdown_hash(value: &(impl Hash + ?Sized)) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    pub(super) struct InlineCodeLineBoxes;
    impl gpui::Render for InlineCodeLineBoxes {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let theme = Theme::for_mode(ThemeMode::Light);
            div()
                .flex()
                .flex_col()
                .items_start()
                .child(
                    render_assistant_markdown("plain", theme, "plain-line")
                        .debug_selector(|| "plain-line".to_owned()),
                )
                .child(
                    render_assistant_markdown("`code`", theme, "code-line")
                        .debug_selector(|| "code-line".to_owned()),
                )
                .child(
                    render_assistant_markdown("[src](/tmp/src.rs)", theme, "mention-line")
                        .debug_selector(|| "mention-line".to_owned()),
                )
        }
    }

    struct NoticeLines;
    impl gpui::Render for NoticeLines {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let theme = Theme::for_mode(ThemeMode::Dark);
            div().w(px(706.0)).child(
                render_notice_markdown(
                    "Codex is ignoring 1 setting.\n  user (config.toml): `key` is ignored.",
                    theme,
                    "notice",
                    theme.text,
                )
                .debug_selector(|| "notice".to_owned()),
            )
        }
    }

    /// The config warning summary breaks at its newline, as the reference's
    /// `<br>`, and its inline-code line is 1px taller (CDP: 64.375px for the
    /// three-line settings banner).
    #[gpui::test]
    fn notice_markdown_breaks_lines_at_newlines(cx: &mut gpui::TestAppContext) {
        let window = cx.add_window(|_, _| NoticeLines);
        let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let notice = visual.debug_bounds("notice").unwrap();
        let height = f32::from(notice.size.height);
        assert!(
            (height - (PULL_REQUEST_COMMENT_LINE_HEIGHT * 2.0 + 1.0)).abs() <= 0.5,
            "notice={notice:?}"
        );
    }

    #[gpui::test]
    fn inline_code_and_mentions_contribute_chatgpt_line_box_height(cx: &mut gpui::TestAppContext) {
        assert_eq!(CHATGPT_MARKDOWN_LAYOUT.base_line_height, 22.75);
        assert_eq!(CHATGPT_MARKDOWN_LAYOUT.inline_code_flow_height, 23.75);
        assert_eq!(
            CHATGPT_MARKDOWN_LAYOUT.inline_code_line_height
                + CHATGPT_MARKDOWN_LAYOUT.inline_code_padding_y * 2.0,
            17.0
        );

        let window = cx.add_window(|_, _| InlineCodeLineBoxes);
        let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));

        let plain = visual.debug_bounds("plain-line").unwrap();
        let code = visual.debug_bounds("code-line").unwrap();
        let mention = visual.debug_bounds("mention-line").unwrap();
        // Line boxes keep their fractional heights (22.75, 23.75, 23.75), so
        // only the snapped edges land on the device-pixel grid and the lines
        // below them do not drift.
        let top = plain.origin.y;
        for (bounds, expected) in [(plain, 22.75), (code, 46.5), (mention, 70.25)] {
            assert!(
                (f32::from(bounds.bottom() - top) - expected).abs() <= 0.5,
                "plain={plain:?} code={code:?} mention={mention:?}"
            );
        }
    }

    pub(super) struct FractionalInlineFlow;
    impl gpui::Render for FractionalInlineFlow {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            div()
                .flex()
                .flex_col()
                .items_start()
                .text_size(px(13.25))
                .child(
                    div()
                        .debug_selector(|| "fragmented-text".to_owned())
                        .flex()
                        .children((0..8).map(|_| div().child("a"))),
                )
                .child(
                    div()
                        .debug_selector(|| "continuous-text".to_owned())
                        .child("aaaaaaaa"),
                )
        }
    }
    #[gpui::test]
    fn fragmented_text_does_not_accumulate_per_character_pixel_rounding(
        cx: &mut gpui::TestAppContext,
    ) {
        let window = cx.add_window(|_, _| FractionalInlineFlow);
        let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let fragmented = visual.debug_bounds("fragmented-text").unwrap();
        let continuous = visual.debug_bounds("continuous-text").unwrap();
        assert!(
            (f32::from(fragmented.size.width - continuous.size.width)).abs() <= 1.0,
            "fragmented={fragmented:?}, continuous={continuous:?}"
        );
    }

    struct NarrowMarkdownTable;
    impl gpui::Render for NarrowMarkdownTable {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            let document = parse_markdown(
                "| Key | Description |\n| --- | --- |\n| `id` | A substantially longer description |\n| next | Text |",
            );
            let MarkdownBlock::Table {
                alignments,
                header,
                rows,
            } = &document.blocks[0]
            else {
                unreachable!()
            };
            div().w(px(360.0)).child(render_table(
                alignments,
                header,
                rows,
                MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Light)),
                42,
                &MarkdownFadeHandle::none(),
            ))
        }
    }

    #[gpui::test]
    fn table_keeps_its_scroll_viewport_inside_a_narrow_parent(cx: &mut gpui::TestAppContext) {
        let window = cx.add_window(|_, _| NarrowMarkdownTable);
        let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let viewport = visual.debug_bounds("markdown-table-viewport-42").unwrap();
        let table = visual.debug_bounds("markdown-table-42").unwrap();
        assert_eq!(viewport.size.width, px(360.0));
        assert!((f32::from(table.size.width) - 736.0).abs() <= 1.0);
        assert_eq!(viewport.origin.x, table.origin.x);
        let header_id = markdown_hash(&(42_u64, "header"));
        let a = visual
            .debug_bounds(Box::leak(
                format!("markdown-table-cell-{header_id}-0").into_boxed_str(),
            ))
            .unwrap();
        let b = visual
            .debug_bounds(Box::leak(
                format!("markdown-table-cell-{header_id}-1").into_boxed_str(),
            ))
            .unwrap();
        assert!(b.size.width > a.size.width * 2.0, "short={a:?}, long={b:?}");
        assert!((f32::from(a.size.width + b.size.width - table.size.width)).abs() <= 1.0);
        visual.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(180.0), px(40.0)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(-10000.0), px(0.0))),
            touch_phase: gpui::TouchPhase::Moved,
            modifiers: Default::default(),
        });
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let scrolled = visual.debug_bounds("markdown-table-42").unwrap();
        assert!(
            scrolled.origin.x < table.origin.x,
            "before={table:?}, after={scrolled:?}"
        );
        assert!((f32::from(scrolled.right() - viewport.right())).abs() <= 1.0);
        visual.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(180.0), px(40.0)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(10000.0), px(0.0))),
            touch_phase: gpui::TouchPhase::Moved,
            modifiers: Default::default(),
        });
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let restored = visual.debug_bounds("markdown-table-42").unwrap();
        assert_eq!(restored.origin.x, viewport.origin.x);
    }

    struct WideMarkdownTable;
    impl gpui::Render for WideMarkdownTable {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            let document = parse_markdown(
                "正文保持原有宽度。\n\n| 范围 | 当前实现 | 是否属于 GPUI 绘制 |\n| --- | --- | --- |\n| 主界面、侧栏、会话、设置页、菜单和审批卡片等 | Rust 中通过 GPUI 的 `Render`、`div()`、布局和样式 API 组成 | 是 |\n| 图标、图片、部分设置页文字覆盖层 | 用 GPUI 的 `svg()`／`img()` 显示已有资源 | 由 GPUI 渲染，但内容属于静态素材 |\n| ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz | ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz | ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz |\n\n后续正文也保持居中。",
            );
            div().w(px(1200.0)).child(render_markdown_document(
                &document,
                Theme::for_mode(ThemeMode::Dark),
                42,
                &MarkdownFadeHandle::none(),
            ))
        }
    }

    #[gpui::test]
    fn wide_table_exposes_last_header_without_widening_prose(cx: &mut gpui::TestAppContext) {
        let window = cx.add_window(|_, _| WideMarkdownTable);
        let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = |visual: &mut gpui::VisualTestContext, selector: String| {
            visual
                .debug_bounds(Box::leak(selector.into_boxed_str()))
                .unwrap()
        };
        let table_id = markdown_hash(&(42_u64, 1_usize));
        let table = bounds(&mut visual, format!("markdown-table-{table_id}"));
        let viewport = bounds(&mut visual, format!("markdown-table-viewport-{table_id}"));
        assert!(table.size.width > px(736.0), "{table:?}");
        assert_eq!(viewport.size.width, table.size.width);
        assert!((f32::from(table.center().x) - 600.0).abs() <= 1.0);
        let header_id = markdown_hash(&(table_id, "header"));
        let last_header = bounds(&mut visual, format!("markdown-table-cell-{header_id}-2"));
        assert!(last_header.right() <= viewport.right() + px(1.0));
        assert!(last_header.left() >= viewport.left());
        for index in [0_usize, 2] {
            let block_id = markdown_hash(&(42_u64, index));
            let prose = bounds(&mut visual, format!("markdown-block-{block_id}"));
            assert_eq!(prose.size.width, px(736.0));
            assert!((f32::from(prose.center().x) - 600.0).abs() <= 1.0);
        }
    }

    struct StretchedTableGrid;
    impl gpui::Render for StretchedTableGrid {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            div()
                .w(px(736.0))
                .grid()
                .grid_cols_auto(2)
                .child(
                    div()
                        .debug_selector(|| "first-column".to_owned())
                        .child(div().w(px(308.0)).h(px(20.0))),
                )
                .child(
                    div()
                        .debug_selector(|| "second-column".to_owned())
                        .child(div().w(px(305.0)).h(px(20.0))),
                )
        }
    }
    #[gpui::test]
    fn table_columns_share_unused_width_without_equalizing_intrinsic_sizes(
        cx: &mut gpui::TestAppContext,
    ) {
        let window = cx.add_window(|_, _| StretchedTableGrid);
        let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let a = visual.debug_bounds("first-column").unwrap();
        let b = visual.debug_bounds("second-column").unwrap();
        assert!((f32::from(a.size.width) - 369.5).abs() <= 1.0);
        assert!((f32::from(b.size.width) - 366.5).abs() <= 1.0);
        assert_eq!(a.size.width + b.size.width, px(736.0));
    }

    #[test]
    fn standalone_images_preserve_media_and_neighboring_text() {
        let doc = super::parse_markdown("before ![sample](/tmp/sample.png) after");
        assert_eq!(doc.blocks.len(), 3);
        assert!(
            matches!(&doc.blocks[1], super::MarkdownBlock::Image { destination, alt, .. } if destination == "/tmp/sample.png" && alt == "sample")
        );
        assert_eq!(
            super::markdown_file_reference_label("Renderer", "/tmp/main.rs:42"),
            "Renderer (line 42)"
        );
        assert_eq!(
            super::markdown_file_reference_label("Renderer (line 42)", "/tmp/main.rs:42"),
            "Renderer (line 42)"
        );
    }

    use super::*;
    use crate::theme::ThemeMode;
    use gpui::rgba;

    /// The pieces Blink wraps as units, probed in the reference renderer with
    /// `overflow-wrap: normal` at a 1px width.
    fn blink_pieces(text: &str) -> Vec<&str> {
        let mut start = 0;
        line_break_offsets(text)
            .into_iter()
            .map(|end| {
                let piece = &text[start..end];
                start = end;
                piece
            })
            .collect()
    }

    #[test]
    fn ascii_breaks_follow_blinks_pair_table() {
        assert_eq!(
            blink_pieces("artifacts/app-server-schema-20260913"),
            ["artifacts/app-", "server-", "schema-", "20260913"]
        );
        assert_eq!(blink_pieces("--schema-root"), ["-", "-", "schema-", "root"]);
        assert_eq!(blink_pieces("-32601"), ["-32601"]);
        assert_eq!(blink_pieces("x-1.2"), ["x-", "1.2"]);
        assert_eq!(
            blink_pieces("src/agent/codex/runtime.rs"),
            ["src/agent/codex/runtime.rs"]
        );
        assert_eq!(
            blink_pieces("codex-cli 0.154.0"),
            ["codex-", "cli ", "0.154.0"]
        );
        assert_eq!(blink_pieces("ab／cd"), ["ab", "／", "cd"]);
        assert_eq!(blink_pieces("README），统"), ["README），", "统"]);
    }

    #[test]
    fn code_spans_glue_to_the_text_before_them_and_closing_punctuation_after() {
        let blocks = parse_markdown("回 `-32601`」这 词`x`，`y`的 `a/b`／`c`");
        let MarkdownBlock::Paragraph(inlines) = &blocks.blocks[0] else {
            panic!("expected a paragraph");
        };
        let mut fragments = Vec::new();
        append_inline_fragments(inlines, InlineState::default(), &mut fragments);
        let mut units: Vec<String> = Vec::new();
        for (index, fragment) in fragments.iter().enumerate() {
            let glued = index > 0 && glued_to_previous(&fragments[index - 1], fragment);
            match units.last_mut() {
                Some(unit) if glued => unit.push_str(&fragment.text),
                _ => units.push(fragment.text.clone()),
            }
        }
        assert_eq!(
            units,
            ["回", "-32601」", "这", "词x，y", "的", "a/b", "／c"]
        );
    }

    #[test]
    fn parses_commonmark_and_gfm_nodes() {
        let markdown = r#"# H1
## H2
### H3
#### H4
##### H5
###### H6

Text **strong** *emphasis* ~~strike~~ `inline` [link](https://example.com "title")
soft
line\
hard

> quote **body**

---

1. ordered
   - nested
2. next

| Left | Right |
| :--- | ---: |
| a | b |

```rust
fn main() {}
```
"#;
        let document = parse_markdown(markdown);

        let levels = document
            .blocks
            .iter()
            .filter_map(|block| match block {
                MarkdownBlock::Heading { level, .. } => Some(*level),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(levels, vec![1, 2, 3, 4, 5, 6]);
        assert!(
            document
                .blocks
                .iter()
                .any(|block| matches!(block, MarkdownBlock::BlockQuote(_)))
        );
        assert!(
            document
                .blocks
                .iter()
                .any(|block| matches!(block, MarkdownBlock::HorizontalRule))
        );
        assert!(document.blocks.iter().any(|block| matches!(
            block,
            MarkdownBlock::Table { header, rows, .. }
                if header.len() == 2 && rows.len() == 1
        )));
        assert!(document.blocks.iter().any(|block| matches!(
            block,
            MarkdownBlock::CodeBlock { language, fenced: true, code }
                if language.as_deref() == Some("rust") && code.contains("fn main")
        )));

        let rich_paragraph = document.blocks.iter().find_map(|block| match block {
            MarkdownBlock::Paragraph(content)
                if content
                    .iter()
                    .any(|inline| matches!(inline, MarkdownInline::Strong(_))) =>
            {
                Some(content)
            }
            _ => None,
        });
        let rich_paragraph = rich_paragraph.expect("rich paragraph");
        assert!(
            rich_paragraph
                .iter()
                .any(|inline| matches!(inline, MarkdownInline::Emphasis(_)))
        );
        assert!(
            rich_paragraph
                .iter()
                .any(|inline| matches!(inline, MarkdownInline::Strikethrough(_)))
        );
        assert!(
            rich_paragraph
                .iter()
                .any(|inline| matches!(inline, MarkdownInline::Code(code) if code == "inline"))
        );
        assert!(rich_paragraph.iter().any(|inline| matches!(
            inline,
            MarkdownInline::Link { destination, .. } if destination == "https://example.com"
        )));
        assert!(
            rich_paragraph
                .iter()
                .any(|inline| matches!(inline, MarkdownInline::SoftBreak))
        );
        assert!(
            rich_paragraph
                .iter()
                .any(|inline| matches!(inline, MarkdownInline::HardBreak))
        );
    }

    #[test]
    fn preserves_ordered_start_nested_lists_and_tasks() {
        let document = parse_markdown("3. outer\n   - [x] nested task\n4. next\n");
        let MarkdownBlock::List { start, items } = &document.blocks[0] else {
            panic!("expected list");
        };
        assert_eq!(*start, Some(3));
        assert_eq!(items.len(), 2);
        let nested = items[0]
            .blocks
            .iter()
            .find_map(|block| match block {
                MarkdownBlock::List { items, .. } => Some(items),
                _ => None,
            })
            .expect("nested list");
        assert_eq!(nested[0].checked, Some(true));
    }

    #[test]
    fn incomplete_streaming_markdown_stays_renderable() {
        let document = parse_markdown("before **open\n\n```rust\nfn main(");
        assert!(!document.blocks.is_empty());
        assert!(document.blocks.iter().any(|block| matches!(
            block,
            MarkdownBlock::CodeBlock { fenced: true, code, .. } if code.contains("fn main(")
        )));
    }

    #[test]
    fn light_and_dark_share_structure_and_geometry() {
        let source = "## Title\n\n- **item** with `code`\n\n| a | b |\n|---|---|\n| 1 | 2 |";
        let light_document = parse_markdown(source);
        let dark_document = parse_markdown(source);
        assert_eq!(light_document, dark_document);

        let light = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Light));
        let dark = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Dark));
        assert_eq!(light.layout, dark.layout);
        assert_ne!(light.palette, dark.palette);
        assert_eq!(light.palette.link, dark.palette.link);
    }

    #[test]
    fn assistant_markdown_body_weight_matches_chatgpt_cdp() {
        assert_eq!(CHATGPT_MARKDOWN_BODY_WEIGHT, FontWeight(430.0));
    }

    #[test]
    fn markdown_link_is_one_contiguous_interaction_fragment() {
        let document = parse_markdown("前缀 [用户**气泡**像素报告](/Users/example/report.md) 后缀");
        let MarkdownBlock::Paragraph(inlines) = &document.blocks[0] else {
            panic!("expected paragraph");
        };
        let mut fragments = Vec::new();
        append_inline_fragments(inlines, InlineState::default(), &mut fragments);

        let links = fragments
            .iter()
            .filter(|fragment| fragment.link_destination.is_some())
            .collect::<Vec<_>>();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].text, "用户气泡像素报告");
        assert_eq!(
            links[0].link_destination.as_deref(),
            Some("/Users/example/report.md")
        );
        assert!(links[0].link_content.is_some());
        assert!(links[0].state.link);
    }

    #[test]
    fn code_toolbar_labels_match_chatgpt_language_aliases() {
        assert_eq!(code_language_label(None), "纯文本");
        assert_eq!(code_language_label(Some("text")), "纯文本");
        assert_eq!(code_language_label(Some("bash")), "Bash");
        assert_eq!(code_language_label(Some("sh")), "Bash");
        assert_eq!(code_language_label(Some("zsh")), "Bash");
        assert_eq!(code_language_label(Some("jsx")), "JavaScript");
        assert_eq!(code_language_label(Some("tsx")), "TypeScript");
        assert_eq!(code_language_label(Some("html")), "XML");
        assert_eq!(code_language_label(Some("rust title=sample")), "Rust");
    }

    #[test]
    fn absolute_markdown_links_use_chatgpt_file_mentions() {
        assert_eq!(
            markdown_file_reference_path("/Users/example/source.py:18"),
            Some("/Users/example/source.py")
        );
        assert_eq!(
            markdown_file_reference_path("/Users/example/README.md"),
            Some("/Users/example/README.md")
        );
        assert_eq!(
            markdown_file_reference_path("https://example.com/source.py"),
            None
        );
        assert_eq!(
            markdown_file_reference_icon("/Users/example/source.py"),
            "markdown-file-python"
        );
        assert_eq!(
            markdown_file_reference_icon("/Users/example/README.md"),
            "markdown-file-document"
        );

        let light = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Light)).palette;
        let dark = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Dark)).palette;
        assert_eq!(light.file_link, rgba(0x2858a4ff));
        assert_eq!(dark.file_link, rgba(0x5685d1ff));
    }

    fn assert_code_span_coverage(code: &str, spans: &[CodeHighlightSpan]) {
        if code.is_empty() {
            assert!(spans.is_empty());
            return;
        }
        assert_eq!(spans.first().map(|span| span.range.start), Some(0));
        assert_eq!(spans.last().map(|span| span.range.end), Some(code.len()));
        assert!(
            spans
                .windows(2)
                .all(|spans| spans[0].range.end == spans[1].range.start)
        );
        assert!(spans.iter().all(|span| {
            span.range.start < span.range.end
                && code.is_char_boundary(span.range.start)
                && code.is_char_boundary(span.range.end)
        }));
        assert_eq!(
            spans.iter().map(|span| span.range.len()).sum::<usize>(),
            code.len()
        );
    }

    fn text_for_token<'a>(
        code: &'a str,
        spans: &'a [CodeHighlightSpan],
        token: CodeSyntaxToken,
    ) -> Vec<&'a str> {
        spans
            .iter()
            .filter(|span| span.style.token == token)
            .map(|span| &code[span.range.clone()])
            .collect()
    }

    #[test]
    fn syntax_highlighting_covers_utf8_bytes_and_multiline_scopes() {
        let rust = "fn greet() {\n    let value = \"中文🙂\"; // comment\n}\n";
        let rust_spans = highlighted_code_spans(rust, Some("rs")).expect("Rust syntax");
        assert_code_span_coverage(rust, &rust_spans);
        assert!(
            text_for_token(rust, &rust_spans, CodeSyntaxToken::Keyword)
                .iter()
                .any(|text| text.contains("fn"))
        );
        assert!(
            text_for_token(rust, &rust_spans, CodeSyntaxToken::String)
                .iter()
                .any(|text| text.contains("中文🙂"))
        );
        assert!(
            text_for_token(rust, &rust_spans, CodeSyntaxToken::Comment)
                .iter()
                .any(|text| text.contains("comment"))
        );
        assert!(
            rust_spans
                .iter()
                .filter(|span| span.style.token == CodeSyntaxToken::Comment)
                .all(|span| span.style.italic)
        );

        let python = "message = \"\"\"first\n第二行🙂\nthird\"\"\"\nprint(message)";
        let python_spans = highlighted_code_spans(python, Some("python")).expect("Python syntax");
        assert_code_span_coverage(python, &python_spans);
        let highlighted_strings = text_for_token(python, &python_spans, CodeSyntaxToken::String)
            .into_iter()
            .collect::<String>();
        assert!(highlighted_strings.contains("第二行🙂"));

        let shell = "ssh -t host \"value\" # a \"quote\" in a comment\n";
        let shell_spans = highlighted_code_spans(shell, Some("bash")).expect("Bash syntax");
        assert_code_span_coverage(shell, &shell_spans);
        assert!(
            shell_spans
                .iter()
                .any(|span| span.style.token == CodeSyntaxToken::Comment
                    && shell[span.range.clone()].contains("quote"))
        );
        assert!(shell_spans.iter().any(|span| {
            span.style.token == CodeSyntaxToken::Plain && shell[span.range.clone()].contains("ssh")
        }));
        assert!(!shell_spans.iter().any(|span| {
            span.style.token == CodeSyntaxToken::Variable
                && (shell[span.range.clone()].contains("ssh")
                    || shell[span.range.clone()].contains(" -"))
        }));
    }

    #[test]
    fn syntax_highlighting_falls_back_for_unknown_plaintext_and_limits() {
        assert!(highlighted_code_spans("text", None).is_none());
        assert!(highlighted_code_spans("text", Some("plaintext")).is_none());
        assert!(highlighted_code_spans("text", Some("not-a-real-language")).is_none());
        assert!(highlighted_code_spans("", Some("rust")).unwrap().is_empty());

        let oversized_code = "x".repeat(MAX_HIGHLIGHTED_CODE_BYTES + 1);
        assert!(highlighted_code_spans(&oversized_code, Some("rust")).is_none());
        let oversized_line = "x".repeat(MAX_HIGHLIGHTED_LINE_BYTES + 1);
        assert!(highlighted_code_spans(&oversized_line, Some("rust")).is_none());
    }

    #[test]
    fn syntax_lookup_covers_chatgpt_languages_with_explicit_fallbacks() {
        for language in [
            "arduino",
            "bash",
            "c",
            "cpp",
            "csharp",
            "css",
            "diff",
            "go",
            "graphql",
            "ini",
            "java",
            "javascript",
            "json",
            "kotlin",
            "less",
            "lua",
            "makefile",
            "markdown",
            "objectivec",
            "perl",
            "php",
            "php-template",
            "python",
            "python-repl",
            "r",
            "ruby",
            "rust",
            "scss",
            "shell",
            "sql",
            "swift",
            "typescript",
            "xml",
            "yaml",
        ] {
            assert!(code_syntax(Some(language)).is_some(), "missing {language}");
        }
        for language in ["plaintext", "vbnet", "wasm"] {
            assert!(
                code_syntax(Some(language)).is_none(),
                "{language} should use the safe plaintext fallback"
            );
        }
    }

    #[test]
    fn chatgpt_code_theme_exposes_all_semantic_tokens() {
        let light = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Light)).palette;
        assert_eq!(light.syntax_comment, rgba(0x4f4f4fff));
        assert_eq!(light.syntax_keyword, rgba(0xab4f7aff));
        assert_eq!(light.syntax_literal, rgba(0xac4f23ff));
        assert_eq!(light.syntax_string, rgba(0x3a843fff));
        assert_eq!(light.syntax_variable, rgba(0x643caeff));
        assert_eq!(light.syntax_attribute, rgba(0xb8802bff));
        assert_eq!(light.syntax_name, rgba(0x1f4e94ff));
        assert_eq!(light.syntax_error, rgba(0xba2623ff));

        let dark = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Dark)).palette;
        assert_eq!(dark.syntax_comment, rgba(0xb9b9b9ff));
        assert_eq!(dark.syntax_keyword, rgba(0xf8a6c8ff));
        assert_eq!(dark.syntax_literal, rgba(0xf1a275ff));
        assert_eq!(dark.syntax_string, rgba(0x83d197ff));
        assert_eq!(dark.syntax_variable, rgba(0xb897f4ff));
        assert_eq!(dark.syntax_attribute, rgba(0xf9dc78ff));
        assert_eq!(dark.syntax_name, rgba(0x63a8f8ff));
        assert_eq!(dark.syntax_error, rgba(0xff8583ff));
    }
}

#[cfg(test)]
mod inline_line_break_regressions {
    use super::*;
    #[test]
    fn chinese_closing_punctuation_is_not_a_standalone_flex_word() {
        let mut fragments = Vec::new();
        append_inline_fragments(
            &[MarkdownInline::Text("仍需逐项做完整页面对照。".into())],
            InlineState::default(),
            &mut fragments,
        );
        assert_eq!(fragments.last().unwrap().text, "照。");
        assert_eq!(
            fragments
                .iter()
                .map(|f| f.text.as_str())
                .collect::<String>(),
            "仍需逐项做完整页面对照。"
        );
    }
    #[gpui::test]
    fn intrinsic_code_width_tolerates_float_subtraction_error(cx: &mut gpui::TestAppContext) {
        let window = cx.add_window(|_, _| super::tests::FractionalInlineFlow);
        window
            .update(cx, |_, window, _| {
                for text in ["2277a4b", "22.75px"] {
                    let run = gpui::TextRun {
                        len: text.len(),
                        font: gpui::font(UI_MONOSPACE_FONT_FAMILY),
                        color: gpui::black(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let width = window
                        .text_system()
                        .shape_line(text.into(), px(12.88), std::slice::from_ref(&run), None)
                        .width();
                    let lines = window
                        .text_system()
                        .shape_text(
                            text.into(),
                            px(12.88),
                            &[run],
                            Some(width - px(0.00001)),
                            None,
                        )
                        .unwrap();
                    assert!(
                        lines[0].wrap_boundaries().is_empty(),
                        "{text} must fit its intrinsic width"
                    );
                }
            })
            .unwrap();
    }

    #[gpui::test]
    fn shaped_paragraphs_respect_cjk_line_breaks(cx: &mut gpui::TestAppContext) {
        let window = cx.add_window(|_, _| super::tests::FractionalInlineFlow);
        window
            .update(cx, |_, window, _| {
                let run = |len| gpui::TextRun {
                    len,
                    font: ui_font(),
                    color: gpui::black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let width = window
                    .text_system()
                    .shape_line("对照".into(), px(14.0), &[run(6)], None)
                    .width();
                let lines = window
                    .text_system()
                    .shape_text(
                        "对照。".into(),
                        px(14.0),
                        &[run(9)],
                        Some(width + px(0.01)),
                        None,
                    )
                    .unwrap();
                let boundary = lines[0].wrap_boundaries()[0];
                assert_eq!(
                    lines[0].runs()[boundary.run_ix].glyphs[boundary.glyph_ix].index,
                    3
                );
            })
            .unwrap();
    }
}

#[derive(Clone, Copy)]
pub(super) struct TableRowPresentation {
    pub(super) is_header: bool,
    pub(super) is_last_row: bool,
    pub(super) row_identity: u64,
}

#[cfg(test)]
mod streaming_fade_tests {
    use super::*;
    use crate::theme::ThemeMode;
    use std::time::{Duration, Instant};

    /// Renders one paragraph through the streaming path and returns how many
    /// distinct fade segments the timeline holds afterwards.
    fn render_paragraph(source: &str, fade: &MarkdownFadeHandle) -> usize {
        let _ = render_streaming_assistant_markdown(
            source,
            Theme::for_mode(ThemeMode::Dark),
            "paragraph",
            fade,
        );
        fade.segment_count()
    }

    #[test]
    fn a_paragraph_switching_to_inline_boxes_keeps_its_word_keys() {
        let start = Instant::now();
        let fade = MarkdownFadeHandle::new(start);
        // Plain text renders as one styled run; punctuation after spaces
        // attaches differently than in the inline-box path.
        let plain = "Use the (optional) flag, \"quoted\" — and 中文（括号）说明 `cargo";
        let before = render_paragraph(plain, &fade);
        fade.set_now(start + Duration::from_secs(2));
        // Closing the code span switches the paragraph to inline boxes; only
        // the code span itself may register a new segment.
        let after = render_paragraph(&format!("{plain}`"), &fade);
        assert_eq!(after, before, "no word may restart its fade");
        let later = render_paragraph(&format!("{plain}` then"), &fade);
        assert_eq!(later, before + 1);
    }

    #[test]
    fn streaming_render_hides_an_unfinished_link() {
        let document = parse_markdown(&repair::repair_streaming_markdown(
            "see [GPUI](https://github.com/zed",
        ));
        assert_eq!(
            document.blocks,
            vec![MarkdownBlock::Paragraph(vec![MarkdownInline::Text(
                "see".into()
            )])]
        );
    }
}
