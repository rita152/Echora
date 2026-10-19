//! GitHub's inert HTML subset, combined with the surrounding CommonMark tree.
//! Tokenization handles quoted attributes and entities; HTML never executes.
use std::{collections::BTreeMap, path::Path};

use html5gum::{Token, Tokenizer};

use super::*;

struct Frame {
    name: String,
    attributes: BTreeMap<String, String>,
    children: Vec<RawNode>,
}

fn push(roots: &mut Vec<RawNode>, stack: &mut [Frame], node: RawNode) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        roots.push(node);
    }
}

fn close(roots: &mut Vec<RawNode>, stack: &mut Vec<Frame>) {
    if let Some(frame) = stack.pop() {
        push(
            roots,
            stack,
            RawNode::HtmlElement {
                name: frame.name,
                attributes: frame.attributes,
                children: frame.children,
            },
        );
    }
}

pub(super) fn normalize(nodes: Vec<RawNode>) -> Vec<RawNode> {
    fn visit(node: RawNode, roots: &mut Vec<RawNode>, stack: &mut Vec<Frame>) {
        match node {
            // pulldown's HTML blocks end at blank lines. The HTML element may
            // continue around later Markdown blocks (notably <details>).
            RawNode::Element {
                tag: Tag::HtmlBlock,
                children,
            } => {
                for child in children {
                    visit(child, roots, stack);
                }
            }
            RawNode::Html(source) => {
                for token in Tokenizer::new(source.as_str()).flatten() {
                    match token {
                        Token::StartTag(tag) => {
                            let name = String::from_utf8_lossy(&tag.name).into_owned();
                            let attributes = tag
                                .attributes
                                .into_iter()
                                .map(|(key, value)| {
                                    (
                                        String::from_utf8_lossy(&key).into_owned(),
                                        String::from_utf8_lossy(&value).into_owned(),
                                    )
                                })
                                .collect();
                            let void = tag.self_closing
                                || matches!(
                                    name.as_str(),
                                    "br" | "hr"
                                        | "img"
                                        | "input"
                                        | "wbr"
                                        | "meta"
                                        | "link"
                                        | "source"
                                        | "area"
                                        | "base"
                                        | "embed"
                                        | "param"
                                        | "track"
                                        | "col"
                                );
                            stack.push(Frame {
                                name,
                                attributes,
                                children: Vec::new(),
                            });
                            if void {
                                close(roots, stack);
                            }
                        }
                        Token::EndTag(tag) => {
                            let name = String::from_utf8_lossy(&tag.name);
                            if let Some(index) = stack.iter().rposition(|frame| frame.name == name)
                            {
                                while stack.len() > index {
                                    close(roots, stack);
                                }
                            }
                        }
                        Token::String(value) => push(
                            roots,
                            stack,
                            RawNode::Text(String::from_utf8_lossy(&value).into_owned()),
                        ),
                        _ => {}
                    }
                }
            }
            RawNode::Element { tag, children } => push(
                roots,
                stack,
                RawNode::Element {
                    tag,
                    children: normalize(children),
                },
            ),
            other => push(roots, stack, other),
        }
    }
    let (mut roots, mut stack) = (Vec::new(), Vec::new());
    for node in nodes {
        visit(node, &mut roots, &mut stack);
    }
    while !stack.is_empty() {
        close(&mut roots, &mut stack);
    }
    roots
}

fn hidden(name: &str) -> bool {
    matches!(
        name,
        "script" | "style" | "iframe" | "object" | "embed" | "template" | "noscript" | "head"
    )
}

pub(super) fn is_block(name: &str) -> bool {
    hidden(name)
        || matches!(
            name,
            "p" | "div"
                | "section"
                | "article"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "details"
                | "summary"
                | "blockquote"
                | "pre"
                | "hr"
                | "img"
                | "ul"
                | "ol"
        )
}

fn safe_destination(value: &str) -> bool {
    let value = value.trim();
    if value.chars().any(char::is_control) {
        return false;
    }
    match url::Url::parse(value) {
        Ok(url) => matches!(url.scheme(), "http" | "https" | "mailto"),
        Err(url::ParseError::RelativeUrlWithoutBase) => true,
        Err(_) => false,
    }
}

pub(super) fn inlines(
    name: &str,
    attributes: &BTreeMap<String, String>,
    children: Vec<RawNode>,
) -> Vec<MarkdownInline> {
    if hidden(name) {
        return Vec::new();
    }
    if matches!(name, "code" | "kbd" | "samp") {
        return vec![MarkdownInline::Code(collect_raw_text(children))];
    }
    let content = raw_nodes_to_inlines(children);
    match name {
        "strong" | "b" => vec![MarkdownInline::Strong(content)],
        "em" | "i" => vec![MarkdownInline::Emphasis(content)],
        "del" | "s" | "strike" => vec![MarkdownInline::Strikethrough(content)],
        "br" => vec![MarkdownInline::HardBreak],
        "a" => {
            if let Some(destination) = attributes
                .get("href")
                .filter(|value| safe_destination(value))
            {
                vec![MarkdownInline::Link {
                    destination: destination.trim().to_owned(),
                    title: attributes.get("title").cloned().unwrap_or_default(),
                    content,
                }]
            } else {
                content
            }
        }
        _ => content,
    }
}

pub(super) fn append_block(
    name: String,
    attributes: BTreeMap<String, String>,
    mut children: Vec<RawNode>,
    output: &mut Vec<MarkdownBlock>,
) {
    if hidden(&name) {
        return;
    }
    let mut blocks = match name.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => vec![MarkdownBlock::Heading {
            level: name.as_bytes()[1] - b'0',
            content: raw_nodes_to_inlines(children),
        }],
        "p" | "summary" => vec![MarkdownBlock::Paragraph(raw_nodes_to_inlines(children))],
        "details" => {
            let summary = children
                .iter()
                .position(
                    |node| matches!(node, RawNode::HtmlElement { name, .. } if name == "summary"),
                )
                .map(|index| match children.remove(index) {
                    RawNode::HtmlElement { children, .. } => raw_nodes_to_inlines(children),
                    _ => unreachable!(),
                })
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| {
                    vec![MarkdownInline::Text(
                        crate::i18n::format!("详情" => "Details"),
                    )]
                });
            vec![MarkdownBlock::Details {
                summary,
                blocks: raw_nodes_to_blocks(children),
                open: attributes.contains_key("open"),
            }]
        }
        "blockquote" => vec![MarkdownBlock::BlockQuote(raw_nodes_to_blocks(children))],
        "pre" => vec![MarkdownBlock::CodeBlock {
            language: None,
            code: collect_raw_text(children),
            fenced: true,
        }],
        "hr" => vec![MarkdownBlock::HorizontalRule],
        "img" => attributes
            .get("src")
            .filter(|value| safe_destination(value))
            .map(|destination| {
                vec![MarkdownBlock::Image {
                    destination: destination.clone(),
                    alt: attributes.get("alt").cloned().unwrap_or_default(),
                    dimensions: markdown_image_dimensions(destination),
                }]
            })
            .unwrap_or_default(),
        "ul" | "ol" => vec![MarkdownBlock::List {
            start: (name == "ol").then(|| {
                attributes
                    .get("start")
                    .and_then(|start| start.parse().ok())
                    .unwrap_or(1)
            }),
            items: children
                .into_iter()
                .filter_map(|node| match node {
                    RawNode::HtmlElement { name, children, .. } if name == "li" => {
                        Some(MarkdownListItem {
                            checked: None,
                            blocks: raw_nodes_to_blocks(children),
                        })
                    }
                    _ => None,
                })
                .collect(),
        }],
        _ => raw_nodes_to_blocks(children),
    };
    let alignment = match attributes
        .get("align")
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("center") => MarkdownAlignment::Center,
        Some("right") => MarkdownAlignment::Right,
        Some("left") => MarkdownAlignment::Left,
        _ => MarkdownAlignment::None,
    };
    if alignment != MarkdownAlignment::None && !blocks.is_empty() {
        output.push(MarkdownBlock::Aligned { alignment, blocks });
    } else {
        output.append(&mut blocks);
    }
}

/// Resolve README-style relative resources against the file's directory.
pub fn parse_markdown_file(source: &str, path: &Path) -> MarkdownDocument {
    fn destination(value: &mut String, base: &url::Url, image: bool) {
        if !value.starts_with('#')
            && url::Url::parse(value).is_err()
            && let Ok(url) = base.join(value)
            && let Ok(path) = url.to_file_path()
        {
            *value = path.to_string_lossy().into_owned();
            if !image && let Some(fragment) = url.fragment() {
                value.push('#');
                value.push_str(fragment);
            }
        }
    }
    fn inlines(values: &mut [MarkdownInline], base: &url::Url) {
        for value in values {
            match value {
                MarkdownInline::Link {
                    destination: target,
                    content,
                    ..
                } => {
                    destination(target, base, false);
                    inlines(content, base);
                }
                MarkdownInline::Strong(children)
                | MarkdownInline::Emphasis(children)
                | MarkdownInline::Strikethrough(children) => inlines(children, base),
                _ => {}
            }
        }
    }
    fn blocks(values: &mut [MarkdownBlock], base: &url::Url) {
        for value in values {
            match value {
                MarkdownBlock::Image {
                    destination: target,
                    dimensions,
                    ..
                } => {
                    destination(target, base, true);
                    *dimensions = markdown_image_dimensions(target);
                }
                MarkdownBlock::Paragraph(content) | MarkdownBlock::Heading { content, .. } => {
                    inlines(content, base)
                }
                MarkdownBlock::Aligned {
                    blocks: children, ..
                }
                | MarkdownBlock::BlockQuote(children) => blocks(children, base),
                MarkdownBlock::Details {
                    summary,
                    blocks: children,
                    ..
                } => {
                    inlines(summary, base);
                    blocks(children, base);
                }
                MarkdownBlock::List { items, .. } => {
                    for item in items {
                        blocks(&mut item.blocks, base);
                    }
                }
                MarkdownBlock::Table { header, rows, .. } => {
                    for cell in header.iter_mut().chain(rows.iter_mut().flatten()) {
                        inlines(&mut cell.content, base);
                    }
                }
                _ => {}
            }
        }
    }
    let mut document = parse_markdown(source);
    if let Some(parent) = path.parent()
        && let Ok(base) = url::Url::from_directory_path(parent)
    {
        blocks(&mut document.blocks, &base);
    }
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_html_keeps_heading_alignment_emphasis_links_and_disclosures() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("README.zh-CN.md");
        let document = parse_markdown_file(include_str!("../../../README.zh-CN.md"), &path);
        let MarkdownBlock::Aligned { alignment, blocks } = &document.blocks[0] else {
            panic!("missing HTML alignment")
        };
        assert_eq!(*alignment, MarkdownAlignment::Center);
        assert!(
            matches!(&blocks[0], MarkdownBlock::Heading { level: 1, content } if content == &[MarkdownInline::Text("Echora".into())])
        );
        let MarkdownBlock::Aligned { blocks, .. } = &document.blocks[1] else {
            panic!()
        };
        assert!(
            matches!(&blocks[0], MarkdownBlock::Paragraph(content) if matches!(content.first(), Some(MarkdownInline::Strong(_))))
        );
        let MarkdownBlock::Aligned { blocks, .. } = &document.blocks[3] else {
            panic!()
        };
        let MarkdownBlock::Paragraph(content) = &blocks[0] else {
            panic!()
        };
        assert!(content.iter().any(|inline| matches!(inline, MarkdownInline::Link { destination, .. } if destination == &path.with_file_name("README.md").to_string_lossy())));
        let (summary, blocks, open) = document
            .blocks
            .iter()
            .find_map(|block| match block {
                MarkdownBlock::Details {
                    summary,
                    blocks,
                    open,
                } => Some((summary, blocks, open)),
                _ => None,
            })
            .expect("README disclosure");
        assert_eq!(summary, &[MarkdownInline::Text("浅色主题".into())]);
        assert!(!open);
        assert!(blocks.iter().any(|block| matches!(block, MarkdownBlock::Image { destination, .. } if destination == &path.parent().unwrap().join("docs/images/workspace-light.png").to_string_lossy())));
    }

    #[test]
    fn mixed_inline_html_and_nested_details_preserve_markdown() {
        let source = "A <STRONG>bold *and italic*</STRONG> <a href='README.md' title='a > b'>English &amp; 中文</a>.\n\n<details open><summary><strong>Outer</strong></summary>\n\n- Markdown item\n\n<details><summary>Inner</summary>\n\n`<code>`\n\n</details>\n\n</details>";
        let document = parse_markdown(source);
        let MarkdownBlock::Paragraph(content) = &document.blocks[0] else {
            panic!()
        };
        assert!(content.iter().any(|inline| matches!(inline, MarkdownInline::Strong(children) if children.iter().any(|child| matches!(child, MarkdownInline::Emphasis(_))))));
        assert!(content.iter().any(|inline| matches!(inline, MarkdownInline::Link { title, content, .. } if title == "a > b" && content == &[MarkdownInline::Text("English & 中文".into())])));
        let MarkdownBlock::Details { blocks, open, .. } = &document.blocks[1] else {
            panic!()
        };
        assert!(open);
        assert!(matches!(blocks.first(), Some(MarkdownBlock::List { .. })));
        assert!(matches!(
            blocks.last(),
            Some(MarkdownBlock::Details { open: false, .. })
        ));
    }

    #[test]
    fn html_is_inert_but_code_and_escaped_tags_remain_literal() {
        let source = "<!-- hidden -->\n<script>alert('hidden')</script>\n\n<a href='javascript:alert(1)' onclick='bad()'>Safe text</a>\n\n```html\n<details><summary>Code</summary></details>\n```\n\n&lt;h1&gt;literal&lt;/h1&gt;";
        let document = parse_markdown(source);
        assert_eq!(
            document.blocks[0],
            MarkdownBlock::Paragraph(vec![MarkdownInline::Text("Safe text".into())])
        );
        assert!(
            matches!(&document.blocks[1], MarkdownBlock::CodeBlock { code, .. } if code == "<details><summary>Code</summary></details>\n")
        );
        assert_eq!(
            document.blocks[2],
            MarkdownBlock::Paragraph(vec![MarkdownInline::Text("<h1>literal</h1>".into())])
        );
    }
}
