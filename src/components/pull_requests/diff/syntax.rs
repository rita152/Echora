//! Diff syntax colors from the reference's default code theme.
//!
//! The reference highlights diffs with Shiki and its bundled `Codex Light` /
//! `Codex Dark` themes (`appearanceLightCodeThemeId` / `…DarkCodeThemeId`
//! default to `codex`). Both are VS Code themes over TextMate scopes, so the
//! same rules resolve syntect's scope stacks here: each token takes the
//! foreground and font style of the most specific matching rule.

use std::{cell::RefCell, collections::HashMap, ops::Range, str::FromStr, sync::OnceLock};

use gpui::{FontStyle, FontWeight, Rgba, TextRun};
use serde::Deserialize;
use two_face::re_exports::syntect::{
    highlighting::{
        Color, FontStyle as ThemeFontStyle, Highlighter, ScopeSelectors, StyleModifier, Theme,
        ThemeItem, ThemeSettings,
    },
    parsing::{Scope, ScopeStack},
};

use crate::theme::{ThemeMode, UI_MONOSPACE_FONT_FAMILY, ui_font};

const CODEX_LIGHT: &str = include_str!("../../../../assets/code-themes/codex-light.json");
const CODEX_DARK: &str = include_str!("../../../../assets/code-themes/codex-dark.json");

#[derive(Deserialize)]
struct VsCodeTheme {
    colors: std::collections::HashMap<String, String>,
    #[serde(rename = "tokenColors")]
    token_colors: Vec<TokenColor>,
}

#[derive(Deserialize)]
struct TokenColor {
    #[serde(default)]
    scope: Option<Scopes>,
    settings: TokenSettings,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Scopes {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize)]
struct TokenSettings {
    foreground: Option<String>,
    #[serde(rename = "fontStyle")]
    font_style: Option<String>,
}

fn color(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#')?;
    let value = |range: Range<usize>| u8::from_str_radix(hex.get(range)?, 16).ok();
    let a = if hex.len() == 8 { value(6..8)? } else { 0xff };
    Some(Color {
        r: value(0..2)?,
        g: value(2..4)?,
        b: value(4..6)?,
        a,
    })
}

fn font_style(style: &str) -> ThemeFontStyle {
    style
        .split_whitespace()
        .fold(ThemeFontStyle::empty(), |styles, style| match style {
            "bold" => styles | ThemeFontStyle::BOLD,
            "italic" => styles | ThemeFontStyle::ITALIC,
            "underline" => styles | ThemeFontStyle::UNDERLINE,
            _ => styles,
        })
}

fn parse_theme(source: &str) -> Theme {
    let theme: VsCodeTheme = serde_json::from_str(source).expect("bundled code theme parses");
    let scopes = theme
        .token_colors
        .into_iter()
        .filter_map(|rule| {
            let selectors = match rule.scope? {
                Scopes::One(scope) => scope,
                Scopes::Many(scopes) => scopes.join(", "),
            };
            Some(ThemeItem {
                scope: ScopeSelectors::from_str(&selectors).ok()?,
                style: StyleModifier {
                    foreground: rule.settings.foreground.as_deref().and_then(color),
                    background: None,
                    font_style: rule.settings.font_style.as_deref().map(font_style),
                },
            })
        })
        .collect();
    Theme {
        name: None,
        author: None,
        settings: ThemeSettings {
            foreground: theme.colors.get("editor.foreground").and_then(|c| color(c)),
            background: theme.colors.get("editor.background").and_then(|c| color(c)),
            ..ThemeSettings::default()
        },
        scopes,
    }
}

/// Scopes that syntect's Sublime grammars name differently from the VS Code
/// grammars Shiki uses, rewritten so the theme's rules match the same tokens.
const SCOPE_ALIASES: &[(&str, &str)] = &[
    (
        "markup.raw.inline.markdown",
        "markup.inline.raw.string.markdown",
    ),
    (
        "punctuation.definition.raw.begin.markdown",
        "punctuation.definition.raw.markdown",
    ),
    (
        "punctuation.definition.raw.end.markdown",
        "punctuation.definition.raw.markdown",
    ),
    (
        "meta.link.inline.description.markdown",
        "string.other.link.title.markdown",
    ),
    (
        "meta.link.reference.description.markdown",
        "string.other.link.title.markdown",
    ),
    (
        "punctuation.definition.link.begin.markdown",
        "punctuation.definition.link.title.begin.markdown",
    ),
    (
        "punctuation.definition.link.end.markdown",
        "punctuation.definition.link.title.end.markdown",
    ),
    (
        "punctuation.definition.metadata.begin.markdown",
        "punctuation.definition.metadata.markdown",
    ),
    (
        "punctuation.definition.metadata.end.markdown",
        "punctuation.definition.metadata.markdown",
    ),
    (
        "punctuation.definition.bold.begin.markdown",
        "punctuation.definition.bold.markdown",
    ),
    (
        "punctuation.definition.bold.end.markdown",
        "punctuation.definition.bold.markdown",
    ),
    (
        "punctuation.definition.italic.begin.markdown",
        "punctuation.definition.italic.markdown",
    ),
    (
        "punctuation.definition.italic.end.markdown",
        "punctuation.definition.italic.markdown",
    ),
    (
        "punctuation.definition.heading.begin.markdown",
        "punctuation.definition.heading.markdown",
    ),
    (
        "punctuation.definition.list_item.markdown",
        "punctuation.definition.list.begin.markdown",
    ),
    (
        "punctuation.definition.raw.code-fence.begin.markdown",
        "punctuation.definition.markdown",
    ),
    (
        "punctuation.definition.raw.code-fence.end.markdown",
        "punctuation.definition.markdown",
    ),
    (
        "constant.other.language-name.markdown",
        "fenced_code.block.language.markdown",
    ),
];

/// `stack` with [`SCOPE_ALIASES`] applied.
fn vs_code_scopes(stack: &ScopeStack) -> Vec<Scope> {
    thread_local! {
        static ALIASES: RefCell<Option<HashMap<Scope, Scope>>> = const { RefCell::new(None) };
    }
    ALIASES.with_borrow_mut(|aliases| {
        let aliases = aliases.get_or_insert_with(|| {
            SCOPE_ALIASES
                .iter()
                .filter_map(|(from, to)| Some((Scope::new(from).ok()?, Scope::new(to).ok()?)))
                .collect()
        });
        stack
            .as_slice()
            .iter()
            .map(|scope| aliases.get(scope).copied().unwrap_or(*scope))
            .collect()
    })
}

fn theme(mode: ThemeMode) -> &'static Theme {
    static LIGHT: OnceLock<Theme> = OnceLock::new();
    static DARK: OnceLock<Theme> = OnceLock::new();
    match mode {
        ThemeMode::Light => LIGHT.get_or_init(|| parse_theme(CODEX_LIGHT)),
        ThemeMode::Dark => DARK.get_or_init(|| parse_theme(CODEX_DARK)),
    }
}

/// Text runs for one line of diff code in `mode`'s Codex theme. Unknown
/// languages (and code too long to parse) keep the theme's foreground.
pub(super) fn code_runs(text: &str, language: Option<&str>, mode: ThemeMode) -> Vec<TextRun> {
    let theme = theme(mode);
    let mut font = ui_font();
    font.family = UI_MONOSPACE_FONT_FAMILY.into();
    font.weight = FontWeight::NORMAL;
    let foreground = theme.settings.foreground.unwrap_or(Color::BLACK);
    let run = |len: usize, color: Color, style: ThemeFontStyle| {
        let mut font = font.clone();
        if style.contains(ThemeFontStyle::BOLD) {
            font.weight = FontWeight::BOLD;
        }
        if style.contains(ThemeFontStyle::ITALIC) {
            font.style = FontStyle::Italic;
        }
        let color = Rgba {
            r: f32::from(color.r) / 255.0,
            g: f32::from(color.g) / 255.0,
            b: f32::from(color.b) / 255.0,
            a: f32::from(color.a) / 255.0,
        };
        TextRun {
            len,
            font,
            color: color.into(),
            background_color: None,
            underline: style
                .contains(ThemeFontStyle::UNDERLINE)
                .then(|| gpui::UnderlineStyle {
                    thickness: gpui::px(1.0),
                    color: Some(color.into()),
                    wavy: false,
                }),
            strikethrough: None,
        }
    };
    let Some(regions) = crate::components::markdown::code_scope_regions(text, language) else {
        return vec![run(text.len(), foreground, ThemeFontStyle::empty())];
    };
    let highlighter = Highlighter::new(theme);
    let mut runs: Vec<TextRun> = Vec::with_capacity(regions.len());
    let mut covered = 0;
    for (range, stack) in regions {
        let style = highlighter.style_for_stack(&vs_code_scopes(&stack));
        if range.start > covered {
            runs.push(run(
                range.start - covered,
                foreground,
                ThemeFontStyle::empty(),
            ));
        }
        let next = run(range.len(), style.foreground, style.font_style);
        match runs.last_mut() {
            Some(last)
                if last.color == next.color
                    && last.font == next.font
                    && last.underline == next.underline =>
            {
                last.len += next.len
            }
            _ => runs.push(next),
        }
        covered = range.end;
    }
    if covered < text.len() {
        runs.push(run(
            text.len() - covered,
            foreground,
            ThemeFontStyle::empty(),
        ));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors(text: &str, language: &str, mode: ThemeMode) -> Vec<(String, String)> {
        let mut offset = 0;
        code_runs(text, Some(language), mode)
            .into_iter()
            .map(|run| {
                let piece = text[offset..offset + run.len].to_owned();
                offset += run.len;
                let c = Rgba::from(run.color);
                let hex = format!(
                    "#{:02x}{:02x}{:02x}",
                    (c.r * 255.0).round() as u8,
                    (c.g * 255.0).round() as u8,
                    (c.b * 255.0).round() as u8
                );
                (piece, hex)
            })
            .collect()
    }

    #[test]
    fn markdown_tokens_take_the_reference_codex_light_colors() {
        // Colors read from the reference diff's Shiki spans.
        let heading = colors("## Get started", "markdown", ThemeMode::Light);
        assert!(heading.iter().all(|(_, c)| c == "#d53538"), "{heading:?}");
        let line = colors(
            "is **macOS**. [rust-toolchain.toml](rust-toolchain.toml)",
            "markdown",
            ThemeMode::Light,
        );
        let color_of = |needle: &str| {
            line.iter()
                .find(|(piece, _)| piece.contains(needle))
                .map(|(_, c)| c.clone())
        };
        assert_eq!(color_of("macOS").as_deref(), Some("#bd5800"), "{line:?}");
        assert_eq!(
            color_of("rust-toolchain.toml]")
                .or(color_of("rust-toolchain"))
                .as_deref(),
            Some("#751ed9"),
            "{line:?}"
        );
    }
}
