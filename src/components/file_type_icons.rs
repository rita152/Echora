//! File-type glyphs for file trees, tabs and diff headers, resolved as the
//! reference's file tree resolves them (`set: "complete"`).
//!
//! `assets/file-icons/map.json` holds the reference's lookup tables: exact
//! file names first, then each dotted suffix of the lowercased name from the
//! longest down (`a.test.ts` tries `test.ts`, then `ts`), with the complete
//! set's overrides (`tsx` → `react`) ahead of the standard table. The glyphs
//! are `assets/icons/pr-file-<name>.svg`, drawn in each type's
//! `--trees-icon-*` color.

use std::{collections::HashMap, sync::OnceLock};

use gpui::Rgba;
use serde::Deserialize;

use crate::theme::ThemeMode;

const MAP: &str = include_str!("../../assets/file-icons/map.json");

#[derive(Deserialize)]
struct IconMap {
    icons: Vec<String>,
    /// `[light, dark]` hex colors per icon.
    colors: HashMap<String, [String; 2]>,
    #[serde(rename = "byFileName")]
    by_file_name: HashMap<String, String>,
    #[serde(rename = "byExtension")]
    by_extension: HashMap<String, String>,
    #[serde(rename = "completeExtension")]
    complete_extension: HashMap<String, String>,
}

fn map() -> &'static IconMap {
    static MAP_CELL: OnceLock<IconMap> = OnceLock::new();
    MAP_CELL.get_or_init(|| serde_json::from_str(MAP).expect("bundled file icon map parses"))
}

fn hex(color: &str) -> Rgba {
    let value = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0x84848a);
    gpui::rgb(value)
}

/// The icon token for `path` (`markdown`, `rust`, …, or `default`).
pub(crate) fn file_icon_token(path: &str) -> &'static str {
    let map = map();
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_lowercase();
    let known = |token: &String| map.icons.iter().any(|icon| icon == token);
    if let Some(token) = map.by_file_name.get(&lower).filter(|token| known(token)) {
        return token;
    }
    let parts: Vec<&str> = lower.split('.').collect();
    for start in 1..parts.len() {
        let suffix = parts[start..].join(".");
        if let Some(token) = map
            .complete_extension
            .get(&suffix)
            .or_else(|| map.by_extension.get(&suffix))
            .filter(|token| known(token))
        {
            return token;
        }
    }
    "default"
}

/// The asset name and color of `path`'s file-type glyph in `mode`.
pub(crate) fn file_icon(path: &str, mode: ThemeMode) -> (String, Rgba) {
    let token = file_icon_token(path);
    let color = map()
        .colors
        .get(token)
        .map(|[light, dark]| match mode {
            ThemeMode::Light => hex(light),
            ThemeMode::Dark => hex(dark),
        })
        .unwrap_or_else(|| hex("#84848a"));
    (format!("pr-file-{token}"), color)
}

/// The color of the reference's untyped glyphs, which its tree also uses for
/// folder chevrons.
pub(crate) fn muted_color(mode: ThemeMode) -> Rgba {
    file_icon("", mode).1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_like_the_reference_tree() {
        assert_eq!(file_icon_token("README.md"), "markdown");
        assert_eq!(file_icon_token("src/main.rs"), "rust");
        assert_eq!(
            file_icon_token("scripts/verify_integration_table.mjs"),
            "javascript"
        );
        assert_eq!(file_icon_token("web/App.tsx"), "react");
        assert_eq!(file_icon_token(".gitignore"), "git");
        assert_eq!(file_icon_token("Cargo.toml"), "default");
        let (asset, color) = file_icon("README.md", ThemeMode::Light);
        assert_eq!(asset, "pr-file-markdown");
        assert_eq!(color, gpui::rgb(0x199f43));
    }
}
