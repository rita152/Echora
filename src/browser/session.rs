//! Browser tabs saved per chat, so a chat's tabs come back after a restart
//! as the reference restores them: the pages reload only when a tab is shown.

use std::{collections::BTreeMap, fs, path::PathBuf};

use serde::{Deserialize, Serialize};

/// Chats whose tabs are remembered; the least recently changed are dropped.
const SESSION_LIMIT: usize = 200;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedTab {
    /// Empty for a New tab page.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_title: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSession {
    pub tabs: Vec<SavedTab>,
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub updated_ms: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SessionFile {
    #[serde(default)]
    chats: BTreeMap<String, SavedSession>,
}

#[derive(Debug)]
pub struct BrowserSessions {
    path: Option<PathBuf>,
    file: SessionFile,
}

impl BrowserSessions {
    pub fn in_memory() -> Self {
        Self {
            path: None,
            file: SessionFile::default(),
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

    pub fn get(&self, chat: &str) -> Option<&SavedSession> {
        self.file.chats.get(chat)
    }

    /// Saves a chat's tabs; a chat left with only an empty New tab forgets
    /// them.
    pub fn save(&mut self, chat: &str, session: SavedSession) {
        let empty = session.tabs.iter().all(|tab| tab.url.is_empty());
        let changed = if empty {
            self.file.chats.remove(chat).is_some()
        } else {
            let unchanged =
                self.file.chats.get(chat).is_some_and(|saved| {
                    saved.tabs == session.tabs && saved.active == session.active
                });
            if !unchanged {
                self.file.chats.insert(chat.to_owned(), session);
            }
            !unchanged
        };
        if !changed {
            return;
        }
        while self.file.chats.len() > SESSION_LIMIT {
            let oldest = self
                .file
                .chats
                .iter()
                .min_by_key(|(_, session)| session.updated_ms)
                .map(|(chat, _)| chat.clone());
            match oldest {
                Some(chat) => {
                    self.file.chats.remove(&chat);
                }
                None => break,
            }
        }
        self.write();
    }

    fn write(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Ok(bytes) = serde_json::to_vec(&self.file) {
            let _ = super::history::write_atomically(path, &bytes);
        }
    }
}

/// Where the browser keeps its history and tabs: `GPUI_BROWSER_DATA_DIR`, or
/// a `browser` folder beside the UI preferences.
pub fn data_directory() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GPUI_BROWSER_DATA_DIR") {
        return Some(PathBuf::from(path));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return Some(PathBuf::from(home).join("Library/Application Support/GPUI/browser"));
    }
    std::env::var_os("XDG_CONFIG_HOME").map(|config| PathBuf::from(config).join("gpui/browser"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(url: &str) -> SavedTab {
        SavedTab {
            url: url.into(),
            title: url.into(),
            custom_title: None,
        }
    }

    #[test]
    fn sessions_round_trip_and_forget_empty_chats() {
        let directory = std::env::temp_dir().join(format!(
            "echora-browser-sessions-{}-{}",
            std::process::id(),
            line!()
        ));
        let path = directory.join("tabs.json");
        let mut sessions = BrowserSessions::load(path.clone());
        sessions.save(
            "thread-a",
            SavedSession {
                tabs: vec![tab("https://example.net/"), tab("")],
                active: 1,
                updated_ms: 1,
            },
        );
        let reloaded = BrowserSessions::load(path.clone());
        assert_eq!(reloaded.get("thread-a").unwrap().tabs.len(), 2);
        assert_eq!(reloaded.get("thread-a").unwrap().active, 1);

        sessions.save(
            "thread-a",
            SavedSession {
                tabs: vec![tab("")],
                active: 0,
                updated_ms: 2,
            },
        );
        assert!(BrowserSessions::load(path).get("thread-a").is_none());
        fs::remove_dir_all(directory).unwrap();
    }
}
