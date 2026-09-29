//! Find in chat: the query, the pages of `thread/searchOccurrences` read so
//! far, and the active match. Independent of GPUI.
//!
//! As in the reference, the first page is read with a limit of 250 and the
//! count shows `+` while more pages exist. Stepping past the last loaded match
//! reads the next page (a repeated cursor ends the walk); after the last page
//! the active match wraps around. When the server cannot search the thread
//! (`-32601`, e.g. an ephemeral side chat), the loaded transcript's visible
//! user messages and final answers are searched locally instead.

use crate::agent::{AgentThreadOccurrence, AgentThreadOccurrencePage};

pub(crate) const FIND_PAGE_SIZE: u32 = 250;

/// One match and its position among the matches of the same message, which
/// is how the view finds it inside the rendered text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FindMatch {
    pub(crate) occurrence: AgentThreadOccurrence,
    pub(crate) ordinal_in_item: usize,
}

/// What the view should do after a step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FindStep {
    /// Show the active match.
    Reveal,
    /// Read the next page, then step again.
    LoadMore(String),
    Nothing,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FindState {
    pub(crate) open: bool,
    pub(crate) query: String,
    pub(crate) thread_id: Option<String>,
    /// Connection generation of the pages read; a newer one invalidates them.
    pub(crate) generation: Option<u64>,
    pub(crate) cycle: u64,
    pub(crate) matches: Vec<FindMatch>,
    pub(crate) next_cursor: Option<String>,
    seen_cursors: Vec<String>,
    pub(crate) active: Option<usize>,
    pub(crate) loading: bool,
    /// A step past the loaded matches waits for the next page.
    step_after_load: bool,
    pub(crate) error: Option<String>,
    /// The server could not search this thread; results are local.
    pub(crate) local: bool,
}

impl FindState {
    pub(crate) fn open(&mut self, thread_id: Option<String>) {
        if self.thread_id != thread_id {
            *self = Self {
                cycle: self.cycle,
                ..Self::default()
            };
            self.thread_id = thread_id;
        }
        self.open = true;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.cycle += 1;
        self.loading = false;
    }

    /// A new query starts a new search; returns its cycle, or `None` when
    /// there is nothing to search (an empty or blank query clears results).
    pub(crate) fn set_query(&mut self, query: String) -> Option<u64> {
        self.query = query;
        self.cycle += 1;
        self.matches.clear();
        self.next_cursor = None;
        self.seen_cursors.clear();
        self.active = None;
        self.error = None;
        self.local = false;
        self.step_after_load = false;
        self.loading = !self.query.trim().is_empty() && self.thread_id.is_some();
        self.loading.then_some(self.cycle)
    }

    /// The trimmed term the server is asked for, as the reference sends it.
    pub(crate) fn search_term(&self) -> String {
        self.query.trim().to_owned()
    }

    /// Applies a page of the current search. A stale cycle, another thread
    /// or an older generation changes nothing.
    pub(crate) fn accept_page(
        &mut self,
        cycle: u64,
        thread_id: &str,
        page: AgentThreadOccurrencePage,
    ) -> FindStep {
        if cycle != self.cycle || self.thread_id.as_deref() != Some(thread_id) {
            return FindStep::Nothing;
        }
        if self
            .generation
            .is_some_and(|generation| page.generation < generation)
        {
            return FindStep::Nothing;
        }
        self.generation = Some(page.generation);
        self.loading = false;
        self.error = None;
        let first_page = self.matches.is_empty();
        for occurrence in page.occurrences {
            let ordinal_in_item = self
                .matches
                .iter()
                .filter(|existing| {
                    existing.occurrence.turn_id == occurrence.turn_id
                        && existing.occurrence.item_id == occurrence.item_id
                })
                .count();
            self.matches.push(FindMatch {
                occurrence,
                ordinal_in_item,
            });
        }
        self.next_cursor = match page.next_cursor {
            Some(cursor) if self.seen_cursors.contains(&cursor) => {
                self.error = Some(crate::i18n::text("查找结果返回了重复的游标，已停止读取").into());
                None
            }
            Some(cursor) => {
                self.seen_cursors.push(cursor.clone());
                Some(cursor)
            }
            None => None,
        };
        if first_page && !self.matches.is_empty() {
            self.active = Some(0);
            return FindStep::Reveal;
        }
        if std::mem::take(&mut self.step_after_load) {
            return self.next();
        }
        FindStep::Nothing
    }

    pub(crate) fn fail(&mut self, cycle: u64, error: String) {
        if cycle == self.cycle {
            self.loading = false;
            self.step_after_load = false;
            self.error = Some(error);
        }
    }

    /// Local results for a server that cannot search this thread.
    pub(crate) fn accept_local(
        &mut self,
        cycle: u64,
        texts: &[(String, String, String)],
    ) -> FindStep {
        if cycle != self.cycle {
            return FindStep::Nothing;
        }
        self.local = true;
        self.loading = false;
        self.error = None;
        let occurrences = local_occurrences(&self.search_term(), texts);
        let thread_id = self.thread_id.clone().unwrap_or_default();
        let generation = self.generation.unwrap_or_default();
        self.accept_page(
            cycle,
            &thread_id,
            AgentThreadOccurrencePage {
                generation,
                occurrences,
                next_cursor: None,
            },
        )
    }

    pub(crate) fn next(&mut self) -> FindStep {
        let Some(active) = self.active else {
            return FindStep::Nothing;
        };
        if active + 1 < self.matches.len() {
            self.active = Some(active + 1);
            return FindStep::Reveal;
        }
        if let Some(cursor) = self.next_cursor.clone() {
            if self.loading {
                return FindStep::Nothing;
            }
            self.loading = true;
            self.step_after_load = true;
            return FindStep::LoadMore(cursor);
        }
        // After the last page the search wraps to the first match.
        self.active = Some(0);
        FindStep::Reveal
    }

    pub(crate) fn previous(&mut self) -> FindStep {
        let Some(active) = self.active else {
            return FindStep::Nothing;
        };
        // Backwards from the first match goes to the last loaded one; pages
        // not read yet stay unread, as the count's `+` says.
        self.active = Some(if active == 0 {
            self.matches.len() - 1
        } else {
            active - 1
        });
        FindStep::Reveal
    }

    pub(crate) fn active_match(&self) -> Option<&FindMatch> {
        self.matches.get(self.active?)
    }

    /// "{current} / {total}", with `+` while more pages exist; 0 without matches.
    pub(crate) fn count_label(&self) -> Option<String> {
        if self.query.trim().is_empty() || (self.loading && self.matches.is_empty()) {
            return None;
        }
        let total = self.matches.len();
        if total == 0 {
            return Some(crate::i18n::format!("0 个结果" => "0 results"));
        }
        let active = self.active.map_or(0, |active| active + 1);
        let more = if self.next_cursor.is_some() { "+" } else { "" };
        Some(crate::i18n::format!(
            "{active} / {total}{more} 个结果" => "{active} / {total}{more} results"
        ))
    }
}

/// Case-insensitive literal matches in `(turn id, item id, text)` messages,
/// with the whole text as the snippet.
pub(crate) fn local_occurrences(
    term: &str,
    texts: &[(String, String, String)],
) -> Vec<AgentThreadOccurrence> {
    if term.is_empty() {
        return Vec::new();
    }
    texts
        .iter()
        .flat_map(|(turn_id, item_id, text)| {
            case_insensitive_matches(text, term)
                .into_iter()
                .map(move |range| AgentThreadOccurrence {
                    item_id: item_id.clone(),
                    turn_id: turn_id.clone(),
                    turn_cursor: String::new(),
                    snippet: text.clone(),
                    snippet_match: range,
                })
        })
        .collect()
}

/// Byte ranges of the non-overlapping case-insensitive matches of `term`.
pub(crate) fn case_insensitive_matches(text: &str, term: &str) -> Vec<std::ops::Range<usize>> {
    let needle = term
        .chars()
        .flat_map(char::to_lowercase)
        .collect::<Vec<_>>();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    let mut from = 0;
    while from < text.len() {
        let Some(end) = match_at(&text[from..], &needle) else {
            let step = text[from..].chars().next().map_or(1, char::len_utf8);
            from += step;
            continue;
        };
        matches.push(from..from + end);
        from += end.max(1);
    }
    matches
}

/// Byte length of `text`'s prefix that lowercases to `needle`, if any.
fn match_at(text: &str, needle: &[char]) -> Option<usize> {
    let mut index = 0;
    for (offset, character) in text.char_indices() {
        for lower in character.to_lowercase() {
            if needle.get(index) != Some(&lower) {
                return None;
            }
            index += 1;
        }
        if index == needle.len() {
            return Some(offset + character.len_utf8());
        }
    }
    None
}

#[cfg(test)]
mod tests;
