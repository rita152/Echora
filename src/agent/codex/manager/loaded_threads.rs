//! `thread/loaded/list`: what the server process actually holds in memory.
//!
//! `ConnectionState::loaded_threads` records the threads this generation
//! started, resumed or forked, and so subscribed to. The server's list is a
//! superset (an unsubscribed thread stays loaded), so it can never stand in for
//! a resume, which is also what subscribes and returns the thread's settings.
//! It is used the other way round: when a thread opens, a local "loaded" that
//! the server no longer confirms is dropped, and the thread is resumed instead
//! of being addressed on a guess.

use std::{collections::HashSet, sync::Arc};

use anyhow::{Result, anyhow, bail};

use super::{
    super::loaded_threads::{LOADED_LIST_METHOD, MAX_LOADED_PAGES, list_params, parse_page},
    CodexAppServerManager,
    connection::Connection,
    workspace::validate_workspace_response,
};

impl CodexAppServerManager {
    /// Every thread id the server reports loaded, across all pages. A repeated
    /// cursor, a duplicate id or a malformed page is a protocol error.
    pub(super) fn server_loaded_threads(
        &self,
        connection: &Arc<Connection>,
    ) -> Result<HashSet<String>> {
        let mut loaded = HashSet::new();
        let mut cursors = HashSet::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_LOADED_PAGES {
            let response =
                connection.request(LOADED_LIST_METHOD, list_params(cursor.as_deref()))?;
            let (ids, next) =
                validate_workspace_response(connection, LOADED_LIST_METHOD, parse_page(&response))?;
            for id in ids {
                if !loaded.insert(id.clone()) {
                    let message = format!("thread/loaded/list 重复返回线程 `{id}`");
                    connection.fail_protocol(message.clone());
                    bail!(message);
                }
            }
            match next {
                None => return Ok(loaded),
                Some(next) if !cursors.insert(next.clone()) => {
                    let message = format!("thread/loaded/list 返回了重复的游标 `{next}`");
                    connection.fail_protocol(message.clone());
                    bail!(message);
                }
                Some(next) => cursor = Some(next),
            }
        }
        let message = format!("thread/loaded/list 超过 {MAX_LOADED_PAGES} 页仍未结束");
        connection.fail_protocol(message.clone());
        bail!(message)
    }

    /// Checks a locally loaded thread against the server before it is used
    /// without a resume. Returns whether the local record still holds; when it
    /// does not, the record and the settings it carried are dropped so the
    /// caller resumes. A failed read keeps the record (the caller proceeds as
    /// before) and is reported, never retried.
    pub(super) fn confirm_loaded_thread(
        &self,
        connection: &Arc<Connection>,
        thread_id: &str,
    ) -> Result<bool> {
        let loaded = match self.server_loaded_threads(connection) {
            Ok(loaded) => loaded,
            Err(error) if connection.failed.load(std::sync::atomic::Ordering::Acquire) => {
                return Err(error);
            }
            Err(error) => {
                eprintln!("thread/loaded/list 读取失败，沿用本地加载记录：{error:#}");
                return Ok(true);
            }
        };
        if loaded.contains(thread_id) {
            return Ok(true);
        }
        let mut state = connection
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        state.loaded_threads.remove(thread_id);
        state.thread_settings.remove(thread_id);
        Ok(false)
    }
}
