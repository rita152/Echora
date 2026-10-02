//! The in-app browser's own history: what the address bar suggests and which
//! sites the New tab page offers. It is kept apart from any system browser,
//! like the reference's built-in browser profile.

use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::address;

/// Pages kept, newest visit first; older ones fall off.
const HISTORY_LIMIT: usize = 5_000;
/// The reference's omnibox shows at most eight matches.
pub const SUGGESTION_LIMIT: usize = 8;
/// Sites on the New tab page.
pub const TOP_SITE_LIMIT: usize = 8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub favicon_url: Option<String>,
    pub visit_count: u32,
    pub last_visit_ms: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct HistoryFile {
    #[serde(default)]
    entries: Vec<HistoryEntry>,
    /// Sites removed from the New tab page; they stay in history.
    #[serde(default)]
    dismissed_top_sites: BTreeSet<String>,
}

/// One row of the address bar's dropdown.
#[derive(Clone, Debug, PartialEq)]
pub enum Suggestion {
    /// A page from history: its title, then its URL in the secondary color.
    History {
        url: String,
        title: String,
        favicon_url: Option<String>,
    },
    /// The typed text as an address ("url-what-you-typed").
    Address { url: String, text: String },
    /// A web search for the typed text.
    Search { query: String, url: String },
}

impl Suggestion {
    pub fn url(&self) -> &str {
        match self {
            Self::History { url, .. } | Self::Address { url, .. } | Self::Search { url, .. } => url,
        }
    }

    /// The text the field shows while the row is highlighted.
    pub fn fill_text(&self) -> String {
        match self {
            Self::History { url, .. } | Self::Address { url, .. } => address::completion_text(url),
            Self::Search { query, .. } => query.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Suggestions {
    /// Text appended (and selected) after what was typed, when the first row is
    /// a history page whose address starts with it.
    pub inline_completion: Option<String>,
    pub rows: Vec<Suggestion>,
}

#[derive(Debug)]
pub struct BrowserHistory {
    path: Option<PathBuf>,
    file: HistoryFile,
}

impl BrowserHistory {
    /// History kept only in memory (tests, captures).
    pub fn in_memory() -> Self {
        Self {
            path: None,
            file: HistoryFile::default(),
        }
    }

    pub fn load(path: PathBuf) -> Self {
        let file = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            file,
        }
    }

    #[cfg(test)]
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.file.entries
    }

    pub fn record_visit(&mut self, url: &str, title: &str, now_ms: i64) {
        if !is_recordable(url) {
            return;
        }
        if let Some(index) = self.file.entries.iter().position(|entry| entry.url == url) {
            let mut entry = self.file.entries.remove(index);
            entry.visit_count = entry.visit_count.saturating_add(1);
            entry.last_visit_ms = now_ms;
            if !title.trim().is_empty() {
                entry.title = title.to_owned();
            }
            self.file.entries.insert(0, entry);
        } else {
            self.file.entries.insert(
                0,
                HistoryEntry {
                    url: url.to_owned(),
                    title: title.to_owned(),
                    favicon_url: None,
                    visit_count: 1,
                    last_visit_ms: now_ms,
                },
            );
            self.file.entries.truncate(HISTORY_LIMIT);
        }
        self.save();
    }

    /// A page's title often arrives after it finished loading.
    pub fn update_page(&mut self, url: &str, title: Option<&str>, favicon_url: Option<&str>) {
        let Some(entry) = self.file.entries.iter_mut().find(|entry| entry.url == url) else {
            return;
        };
        let mut changed = false;
        if let Some(title) = title.filter(|title| !title.trim().is_empty())
            && entry.title != title
        {
            entry.title = title.to_owned();
            changed = true;
        }
        if let Some(favicon) = favicon_url
            && entry.favicon_url.as_deref() != Some(favicon)
        {
            entry.favicon_url = Some(favicon.to_owned());
            changed = true;
        }
        if changed {
            self.save();
        }
    }

    /// Removes a page from history (the dropdown row's remove button).
    pub fn remove(&mut self, url: &str) {
        let before = self.file.entries.len();
        self.file.entries.retain(|entry| entry.url != url);
        if self.file.entries.len() != before {
            self.save();
        }
    }

    pub fn dismiss_top_site(&mut self, url: &str) {
        if self.file.dismissed_top_sites.insert(url.to_owned()) {
            self.save();
        }
    }

    pub fn restore_top_site(&mut self, url: &str) {
        if self.file.dismissed_top_sites.remove(url) {
            self.save();
        }
    }

    /// The New tab page's suggested sites: the most visited web pages, most
    /// recent first among equals, without dismissed ones.
    pub fn top_sites(&self, limit: usize) -> Vec<HistoryEntry> {
        let mut sites: Vec<&HistoryEntry> = self
            .file
            .entries
            .iter()
            .filter(|entry| {
                address::is_web_url(&entry.url)
                    && !self.file.dismissed_top_sites.contains(&entry.url)
            })
            .collect();
        sites.sort_by(|a, b| {
            b.visit_count
                .cmp(&a.visit_count)
                .then(b.last_visit_ms.cmp(&a.last_visit_ms))
        });
        sites.into_iter().take(limit).cloned().collect()
    }

    /// Dropdown rows for typed text. A page whose address starts with the
    /// text becomes the default row and supplies the inline completion; the
    /// typed text itself (as an address or a search) follows, then other
    /// pages whose title or address contain it.
    pub fn suggestions(&self, typed: &str) -> Suggestions {
        let query = typed.trim_start();
        if query.trim().is_empty() {
            return Suggestions::default();
        }
        let needle = query.to_lowercase();
        let mut prefix: Vec<&HistoryEntry> = Vec::new();
        let mut contains: Vec<&HistoryEntry> = Vec::new();
        for entry in &self.file.entries {
            let text = address::completion_text(&entry.url).to_lowercase();
            let bare = text.strip_prefix("www.").unwrap_or(&text);
            if text.starts_with(&needle) || bare.starts_with(&needle) {
                prefix.push(entry);
            } else if text.contains(&needle) || entry.title.to_lowercase().contains(&needle) {
                contains.push(entry);
            }
        }
        let rank = |entries: &mut Vec<&HistoryEntry>| {
            entries.sort_by(|a, b| {
                b.visit_count
                    .cmp(&a.visit_count)
                    .then(b.last_visit_ms.cmp(&a.last_visit_ms))
            })
        };
        rank(&mut prefix);
        rank(&mut contains);
        // A shorter address wins the inline completion, as with the
        // reference's "example" → "example.net".
        let default = prefix
            .iter()
            .copied()
            .min_by_key(|entry| address::completion_text(&entry.url).len());

        let mut rows = Vec::new();
        let mut inline_completion = None;
        if let Some(entry) = default {
            let text = address::completion_text(&entry.url);
            let bare = text.strip_prefix("www.").unwrap_or(&text);
            let completion =
                suffix_after_prefix(&text, query).or_else(|| suffix_after_prefix(bare, query));
            if let Some(completion) = completion
                && !completion.is_empty()
                && !query.ends_with(char::is_whitespace)
            {
                inline_completion = Some(completion.to_owned());
            }
            rows.push(history_row(entry));
        }
        let trimmed = query.trim();
        let url = address::navigation_url(trimmed);
        if address::is_search(trimmed) {
            rows.push(Suggestion::Search {
                query: trimmed.to_owned(),
                url,
            });
        } else if !rows.iter().any(|row| row.url() == url) {
            rows.push(Suggestion::Address {
                url,
                text: trimmed.to_owned(),
            });
            rows.push(Suggestion::Search {
                query: trimmed.to_owned(),
                url: address::search_url(trimmed),
            });
        }
        for entry in prefix.iter().chain(contains.iter()) {
            if rows.len() >= SUGGESTION_LIMIT {
                break;
            }
            if default.is_some_and(|default| std::ptr::eq(default, *entry)) {
                continue;
            }
            rows.push(history_row(entry));
        }
        rows.truncate(SUGGESTION_LIMIT);
        Suggestions {
            inline_completion,
            rows,
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec(&self.file) else {
            return;
        };
        let _ = write_atomically(path, &bytes);
    }
}

fn history_row(entry: &HistoryEntry) -> Suggestion {
    Suggestion::History {
        url: entry.url.clone(),
        title: entry.title.clone(),
        favicon_url: entry.favicon_url.clone(),
    }
}

/// The rest of `text` after a case-insensitive `prefix`.
fn suffix_after_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let mut rest = text.char_indices();
    for expected in prefix.chars() {
        let (_, actual) = rest.next()?;
        if !actual.to_lowercase().eq(expected.to_lowercase()) {
            return None;
        }
    }
    Some(rest.next().map_or("", |(index, _)| &text[index..]))
}

fn is_recordable(url: &str) -> bool {
    address::is_web_url(url) || url.starts_with("file:")
}

pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(pages: &[(&str, &str, u32)]) -> BrowserHistory {
        let mut history = BrowserHistory::in_memory();
        for (index, (url, title, visits)) in pages.iter().enumerate().rev() {
            for visit in 0..*visits {
                history.record_visit(url, title, (index as i64) * 100 + visit as i64);
            }
        }
        history
    }

    #[test]
    fn typing_a_known_host_inline_completes_and_lists_the_search() {
        let history = history(&[
            ("https://example.net/", "Example Domain", 1),
            ("https://example.com/docs", "Docs", 1),
        ]);
        let suggestions = history.suggestions("example");
        assert_eq!(suggestions.inline_completion.as_deref(), Some(".net"));
        assert_eq!(
            suggestions.rows[0],
            Suggestion::History {
                url: "https://example.net/".into(),
                title: "Example Domain".into(),
                favicon_url: None,
            }
        );
        assert_eq!(
            suggestions.rows[1],
            Suggestion::Search {
                query: "example".into(),
                url: address::search_url("example"),
            }
        );
        assert_eq!(suggestions.rows[2].url(), "https://example.com/docs");
    }

    #[test]
    fn an_address_without_history_offers_itself_then_a_search() {
        let history = BrowserHistory::in_memory();
        let suggestions = history.suggestions("docs.rs");
        assert_eq!(suggestions.inline_completion, None);
        assert_eq!(
            suggestions.rows,
            vec![
                Suggestion::Address {
                    url: "https://docs.rs".into(),
                    text: "docs.rs".into()
                },
                Suggestion::Search {
                    query: "docs.rs".into(),
                    url: address::search_url("docs.rs")
                },
            ]
        );
        assert!(history.suggestions("  ").rows.is_empty());
    }

    #[test]
    fn www_is_skipped_for_completion_and_titles_match_too() {
        let history = history(&[
            ("https://www.rust-lang.org/learn", "Learn Rust", 2),
            ("https://crates.io/", "crates.io: Rust Package Registry", 1),
        ]);
        let suggestions = history.suggestions("rust-l");
        assert_eq!(
            suggestions.inline_completion.as_deref(),
            Some("ang.org/learn")
        );
        let titled = history.suggestions("package");
        assert_eq!(titled.inline_completion, None);
        assert!(
            titled
                .rows
                .iter()
                .any(|row| row.url() == "https://crates.io/")
        );
    }

    #[test]
    fn top_sites_rank_by_visits_and_skip_dismissed() {
        let mut history = history(&[
            ("https://a.example.com/", "A", 1),
            ("https://b.example.com/", "B", 3),
            ("file:///tmp/c.html", "C", 5),
        ]);
        let urls: Vec<_> = history.top_sites(8).into_iter().map(|e| e.url).collect();
        assert_eq!(urls, ["https://b.example.com/", "https://a.example.com/"]);
        history.dismiss_top_site("https://b.example.com/");
        assert_eq!(history.top_sites(8).len(), 1);
        history.restore_top_site("https://b.example.com/");
        assert_eq!(history.top_sites(8).len(), 2);
    }

    #[test]
    fn history_persists_and_drops_unrecordable_pages() {
        let directory = std::env::temp_dir().join(format!(
            "echora-browser-history-{}-{}",
            std::process::id(),
            line!()
        ));
        let path = directory.join("history.json");
        let mut history = BrowserHistory::load(path.clone());
        history.record_visit("about:blank", "", 1);
        history.record_visit("https://example.net/", "", 2);
        history.update_page(
            "https://example.net/",
            Some("Example"),
            Some("https://example.net/favicon.ico"),
        );
        let reloaded = BrowserHistory::load(path);
        assert_eq!(reloaded.entries().len(), 1);
        assert_eq!(reloaded.entries()[0].title, "Example");
        assert_eq!(
            reloaded.entries()[0].favicon_url.as_deref(),
            Some("https://example.net/favicon.ico")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
