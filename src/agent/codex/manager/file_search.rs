//! Fuzzy file search sessions owned by one connection generation.
//!
//! The session form is the shape the reference desktop client drives; the
//! one-shot request stays available and is used automatically when a server
//! build reports that the session does not exist. Both shapes deliver the same
//! event stream to the owning UI.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::{Receiver, Sender};
use serde_json::Value;

use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{
    AgentFileSearchRequest, AgentFileSearchSession, AgentFileSearchSessionControl,
    AgentFileSearchSessionEvent,
};

/// Cancellation token of the one-shot fallback. The server may use it to drop
/// superseded searches; the value is opaque.
pub(super) const FILE_SEARCH_CANCELLATION_TOKEN: &str = "gpui-fuzzy-file-search";

struct FileSearchSessionControl {
    manager: Arc<CodexAppServerManager>,
    generation: u64,
    session_id: String,
    roots: Vec<String>,
    /// Cleared once the connection reports that sessions are unavailable.
    sessions_supported: AtomicBool,
    stopped: AtomicBool,
    events: Sender<AgentFileSearchSessionEvent>,
}

impl FileSearchSessionControl {
    fn connection(&self) -> Result<Arc<Connection>> {
        let connection = self
            .manager
            .inner
            .state
            .lock()
            .map_err(|_| anyhow!("连接状态不可用"))?
            .current
            .clone()
            .context("连接已断开，文件搜索会话已失效")?;
        if connection.generation != self.generation || connection.failed.load(Ordering::Acquire) {
            bail!("原连接的 generation 已失效，文件搜索会话已结束");
        }
        Ok(connection)
    }

    fn validate_session(&self, session_id: &str) -> Result<()> {
        if session_id != self.session_id {
            bail!("文件搜索会话 id 不匹配：{session_id}");
        }
        if self.stopped.load(Ordering::Acquire) {
            bail!("文件搜索会话已结束");
        }
        Ok(())
    }

    /// One-shot fallback: the search runs immediately and its result is
    /// published as the same update/completed pair a session would emit.
    fn search_once(&self, query: &str) -> Result<(), String> {
        let connection = self.connection().map_err(|error| format!("{error:#}"))?;
        let response = connection
            .request(
                "fuzzyFileSearch",
                super::super::file_search::build_file_search_params(&AgentFileSearchRequest {
                    query: query.to_owned(),
                    roots: self.roots.clone(),
                    cancellation_token: Some(FILE_SEARCH_CANCELLATION_TOKEN.to_owned()),
                }),
            )
            .map_err(|error| format!("{error:#}"))?;
        let files = super::super::file_search::parse_file_search_response(&response)
            .map_err(|error| format!("{error:#}"))?;
        let _ = self
            .events
            .send_blocking(AgentFileSearchSessionEvent::Updated(
                crate::agent::AgentFileSearchSessionUpdate {
                    session_id: self.session_id.clone(),
                    query: query.to_owned(),
                    files,
                },
            ));
        let _ = self
            .events
            .send_blocking(AgentFileSearchSessionEvent::Completed(
                crate::agent::AgentFileSearchSessionCompleted {
                    session_id: self.session_id.clone(),
                },
            ));
        Ok(())
    }
}

impl AgentFileSearchSessionControl for FileSearchSessionControl {
    fn update_query(&self, session_id: &str, query: &str) -> Result<(), String> {
        self.validate_session(session_id)
            .map_err(|error| format!("{error:#}"))?;
        if self.sessions_supported.load(Ordering::Acquire) {
            let connection = self.connection().map_err(|error| format!("{error:#}"))?;
            match connection.request(
                "fuzzyFileSearch/sessionUpdate",
                super::super::file_search::build_file_search_session_update(session_id, query),
            ) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    let message = format!("{error:#}");
                    if !super::super::file_search::is_unknown_file_search_session(&message) {
                        return Err(message);
                    }
                    // The server no longer knows this session; keep the dialog
                    // working through the one-shot request instead.
                    self.sessions_supported.store(false, Ordering::Release);
                }
            }
        }
        self.search_once(query)
    }

    fn stop(&self, session_id: &str) -> Result<(), String> {
        self.validate_session(session_id)
            .map_err(|error| format!("{error:#}"))?;
        self.stopped.store(true, Ordering::Release);
        let connection = self.connection().map_err(|error| format!("{error:#}"))?;
        connection.retire_file_search_session(session_id);
        if self.sessions_supported.load(Ordering::Acquire) {
            match connection.request(
                "fuzzyFileSearch/sessionStop",
                super::super::file_search::build_file_search_session_stop(session_id),
            ) {
                Ok(_) => {}
                Err(error) => {
                    let message = format!("{error:#}");
                    if !super::super::file_search::is_unknown_file_search_session(&message) {
                        return Err(message);
                    }
                }
            }
        }
        Ok(())
    }
}

impl CodexAppServerManager {
    /// Opens one fuzzy file search session for the given roots. The returned
    /// handle streams updates for this session only; closing the dialog must
    /// call [AgentFileSearchSession::stop].
    pub(in crate::agent::codex) fn open_file_search_session(
        &self,
        roots: Vec<String>,
    ) -> Receiver<Result<AgentFileSearchSession, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = Arc::new(self.clone());
        std::thread::spawn(move || {
            let result = (|| -> Result<AgentFileSearchSession> {
                if roots.is_empty() {
                    bail!("文件搜索需要至少一个工作区根目录");
                }
                let connection = manager.inner.ensure_connection()?;
                let generation = connection.generation;
                let session_id = connection.next_file_search_session_id();
                let (events, updates) = async_channel::unbounded();
                let control = Arc::new(FileSearchSessionControl {
                    manager: manager.clone(),
                    generation,
                    session_id: session_id.clone(),
                    roots,
                    sessions_supported: AtomicBool::new(true),
                    stopped: AtomicBool::new(false),
                    events,
                });
                // Register before the request: an immediate notification for
                // this session must never be rejected as unknown.
                connection.register_file_search_session(&session_id, control.events.clone())?;
                let supported = match connection.request(
                    "fuzzyFileSearch/sessionStart",
                    super::super::file_search::build_file_search_session_start(
                        &session_id,
                        &control.roots,
                    ),
                ) {
                    Ok(_) => true,
                    Err(error) => {
                        let message = format!("{error:#}");
                        if super::super::file_search::is_unknown_file_search_session(&message) {
                            false
                        } else {
                            connection.retire_file_search_session(&session_id);
                            return Err(error.context("打开文件搜索会话失败"));
                        }
                    }
                };
                control
                    .sessions_supported
                    .store(supported, Ordering::Release);
                Ok(AgentFileSearchSession::new(session_id, control, updates))
            })()
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }
}

impl super::ManagerInner {
    /// Routes the two session notifications to their owning dialog. A session
    /// id that was retired (or is already settled) is a late event and stays
    /// inert; an unknown one is a protocol error.
    pub(super) fn handle_file_search_notification(
        &self,
        connection: &Arc<Connection>,
        method: &str,
        message: &Value,
    ) -> Result<()> {
        let event = match method {
            "fuzzyFileSearch/sessionUpdated" => {
                let update = super::super::file_search::parse_session_updated(message)?;
                let session_id = update.session_id.clone();
                (AgentFileSearchSessionEvent::Updated(update), session_id)
            }
            "fuzzyFileSearch/sessionCompleted" => {
                let completed = super::super::file_search::parse_session_completed(message)?;
                let session_id = completed.session_id.clone();
                (
                    AgentFileSearchSessionEvent::Completed(completed),
                    session_id,
                )
            }
            other => bail!("fuzzy file search 通知方法未知：{other}"),
        };
        if let Some(sender) = connection.file_search_session_sender(&event.1)? {
            let _ = sender.send_blocking(event.0);
        }
        Ok(())
    }
}
