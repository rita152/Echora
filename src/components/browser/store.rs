//! State every browser panel shares: the browser's history, each chat's
//! saved tabs, and the downloads list (the reference's global downloads
//! manager).

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use gpui::{Context, Image};

use crate::browser::{
    history::BrowserHistory,
    session::{BrowserSessions, SavedSession, data_directory},
};

#[derive(Clone, Debug, PartialEq)]
pub enum DownloadState {
    InProgress(f64),
    Finished,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Download {
    pub id: u64,
    pub url: String,
    pub filename: String,
    pub path: Option<PathBuf>,
    pub state: DownloadState,
}

pub struct BrowserStore {
    pub(super) history: BrowserHistory,
    sessions: BrowserSessions,
    pub(super) downloads: Vec<Download>,
    /// Page icons by host, from pages opened this session, for the New tab
    /// page's suggested sites.
    favicons: HashMap<String, Arc<Image>>,
}

impl BrowserStore {
    pub fn new(_: &mut Context<Self>) -> Self {
        let directory = if cfg!(test) { None } else { data_directory() };
        match directory {
            Some(directory) => Self {
                history: BrowserHistory::load(directory.join("history.json")),
                sessions: BrowserSessions::load(directory.join("tabs.json")),
                downloads: Vec::new(),
                favicons: HashMap::new(),
            },
            None => Self::in_memory(),
        }
    }

    pub fn in_memory() -> Self {
        Self {
            history: BrowserHistory::in_memory(),
            sessions: BrowserSessions::in_memory(),
            downloads: Vec::new(),
            favicons: HashMap::new(),
        }
    }

    pub fn session(&self, chat: &str) -> Option<SavedSession> {
        self.sessions.get(chat).cloned()
    }

    pub fn save_session(&mut self, chat: &str, session: SavedSession) {
        self.sessions.save(chat, session);
    }

    pub fn record_visit(&mut self, url: &str, title: &str, cx: &mut Context<Self>) {
        self.history
            .record_visit(url, title, chrono::Utc::now().timestamp_millis());
        cx.notify();
    }

    pub fn update_page(&mut self, url: &str, title: Option<&str>, cx: &mut Context<Self>) {
        self.history.update_page(url, title, None);
        cx.notify();
    }

    pub fn remove_history(&mut self, url: &str, cx: &mut Context<Self>) {
        self.history.remove(url);
        cx.notify();
    }

    pub fn dismiss_top_site(&mut self, url: &str, cx: &mut Context<Self>) {
        self.history.dismiss_top_site(url);
        cx.notify();
    }

    pub fn restore_top_site(&mut self, url: &str, cx: &mut Context<Self>) {
        self.history.restore_top_site(url);
        cx.notify();
    }

    pub fn favicon(&self, host: &str) -> Option<Arc<Image>> {
        self.favicons.get(host).cloned()
    }

    pub fn set_favicon(&mut self, host: String, image: Arc<Image>) {
        self.favicons.insert(host, image);
    }

    pub fn downloads(&self) -> &[Download] {
        &self.downloads
    }

    pub(super) fn download_mut(&mut self, id: u64) -> Option<&mut Download> {
        self.downloads.iter_mut().find(|download| download.id == id)
    }

    pub fn remove_download(&mut self, id: u64, cx: &mut Context<Self>) {
        self.downloads.retain(|download| download.id != id);
        cx.notify();
    }

    /// "Delete download history": forgets finished and failed downloads.
    pub fn clear_downloads(&mut self, cx: &mut Context<Self>) {
        self.downloads
            .retain(|download| matches!(download.state, DownloadState::InProgress(_)));
        cx.notify();
    }
}
