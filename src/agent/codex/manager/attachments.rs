//! Thread attachments and background-terminal cleanup, bound to one
//! connection generation.
//!
//! Attachments are cached per thread in the generation's state, so a new
//! generation starts empty. Every `thread/attachment/updated` bumps the
//! thread's revision: a list that was in flight across a change is discarded
//! and read again, so a late answer can never overwrite a newer notification.
//! A server without the methods (`-32601`) is remembered for the generation
//! and answered locally as unsupported.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow, bail};
use async_channel::Receiver;

use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{
    AgentAttachmentAddRequest, AgentAttachmentAdded, AgentAttachmentError,
    AgentAttachmentRemoveRequest, AgentThreadAttachment, AgentThreadAttachments,
};

/// How long a read answers from the cache, like the reference's one-minute
/// staleTime. A forced read always asks the server.
const ATTACHMENT_CACHE_TTL: Duration = Duration::from_secs(60);
/// Reads restarted because the thread changed while they were in flight.
const ATTACHMENT_READ_ATTEMPTS: usize = 4;

#[derive(Default)]
pub(super) struct AttachmentCache {
    revisions: HashMap<String, u64>,
    entries: HashMap<String, CachedAttachments>,
    unsupported: bool,
}

struct CachedAttachments {
    revision: u64,
    read_at: Instant,
    attachments: Vec<AgentThreadAttachment>,
}

impl AttachmentCache {
    fn revision(&self, thread_id: &str) -> u64 {
        self.revisions.get(thread_id).copied().unwrap_or(0)
    }

    /// Any change to the thread's attachments, seen or caused here.
    pub(super) fn invalidate(&mut self, thread_id: &str) {
        *self.revisions.entry(thread_id.to_owned()).or_default() += 1;
        self.entries.remove(thread_id);
    }
}

fn attachment_error(error: anyhow::Error) -> AgentAttachmentError {
    if super::super::attachments::is_method_not_found(&error) {
        AgentAttachmentError::Unsupported
    } else {
        AgentAttachmentError::Failed(format!("{error:#}"))
    }
}

/// Collects pages, rejecting a repeated cursor, an id already seen on an
/// earlier page, and runaway paging.
#[derive(Default)]
struct PageAccumulator {
    attachments: Vec<AgentThreadAttachment>,
    ids: HashSet<String>,
    cursors: HashSet<String>,
    pages: usize,
}

impl PageAccumulator {
    fn push(
        &mut self,
        page: Vec<AgentThreadAttachment>,
        next: Option<String>,
    ) -> Result<Option<String>> {
        self.pages += 1;
        for attachment in page {
            if !self.ids.insert(attachment.id.clone()) {
                bail!(
                    "thread/attachment/list 跨页出现重复附件 `{}`",
                    attachment.id
                );
            }
            self.attachments.push(attachment);
        }
        let Some(next) = next else {
            return Ok(None);
        };
        if !self.cursors.insert(next.clone()) {
            bail!("thread/attachment/list 返回了重复的 nextCursor `{next}`");
        }
        if self.pages >= super::super::attachments::ATTACHMENT_LIST_PAGE_LIMIT {
            bail!(
                "thread/attachment/list 超过 {} 页仍未结束",
                super::super::attachments::ATTACHMENT_LIST_PAGE_LIMIT
            );
        }
        Ok(Some(next))
    }
}

impl CodexAppServerManager {
    fn attachment_cache<T>(
        connection: &Connection,
        read: impl FnOnce(&mut AttachmentCache) -> T,
    ) -> Result<T> {
        let mut state = connection
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        Ok(read(&mut state.attachments))
    }

    fn read_all_attachments(
        connection: &Connection,
        thread_id: &str,
    ) -> Result<Vec<AgentThreadAttachment>> {
        use super::super::attachments::{list_params, parse_list_page};
        let mut pages = PageAccumulator::default();
        let mut cursor: Option<String> = None;
        loop {
            let response = connection.request(
                "thread/attachment/list",
                list_params(thread_id, cursor.as_deref()),
            )?;
            let (page, next) = parse_list_page(&response)?;
            match pages.push(page, next)? {
                Some(next) => cursor = Some(next),
                None => return Ok(pages.attachments),
            }
        }
    }

    /// Every attachment of one thread. Answers from this generation's cache
    /// unless `force` or the entry is older than a minute. A read may open a
    /// connection; the generation it used is part of the answer.
    pub(in crate::agent::codex) fn list_thread_attachments(
        &self,
        thread_id: String,
        force: bool,
    ) -> Receiver<Result<AgentThreadAttachments, AgentAttachmentError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let connection = manager
                    .inner
                    .ensure_connection()
                    .map_err(|error| AgentAttachmentError::Failed(format!("{error:#}")))?;
                let answer = |attachments| AgentThreadAttachments {
                    generation: connection.generation,
                    thread_id: thread_id.clone(),
                    attachments,
                };
                let cached = Self::attachment_cache(&connection, |cache| {
                    if cache.unsupported {
                        return Err(AgentAttachmentError::Unsupported);
                    }
                    let revision = cache.revision(&thread_id);
                    Ok(cache.entries.get(&thread_id).and_then(|entry| {
                        (!force
                            && entry.revision == revision
                            && entry.read_at.elapsed() < ATTACHMENT_CACHE_TTL)
                            .then(|| entry.attachments.clone())
                    }))
                })
                .map_err(attachment_error)??;
                if let Some(attachments) = cached {
                    return Ok(answer(attachments));
                }
                for _ in 0..ATTACHMENT_READ_ATTEMPTS {
                    let revision =
                        Self::attachment_cache(&connection, |cache| cache.revision(&thread_id))
                            .map_err(attachment_error)?;
                    let attachments = match Self::read_all_attachments(&connection, &thread_id) {
                        Ok(attachments) => attachments,
                        Err(error) => {
                            let error = attachment_error(error);
                            if error == AgentAttachmentError::Unsupported {
                                let _ = Self::attachment_cache(&connection, |cache| {
                                    cache.unsupported = true
                                });
                            }
                            return Err(error);
                        }
                    };
                    let stored = Self::attachment_cache(&connection, |cache| {
                        if cache.revision(&thread_id) != revision {
                            return false;
                        }
                        cache.entries.insert(
                            thread_id.clone(),
                            CachedAttachments {
                                revision,
                                read_at: Instant::now(),
                                attachments: attachments.clone(),
                            },
                        );
                        true
                    })
                    .map_err(attachment_error)?;
                    if stored {
                        return Ok(answer(attachments));
                    }
                }
                Err(AgentAttachmentError::Failed(format!(
                    "线程 `{thread_id}` 的附件在读取期间持续变化，已放弃本次读取"
                )))
            })();
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    /// Adds one attachment on the generation the caller read from. The thread
    /// is invalidated either way: the server's notification follows its
    /// response, and an existing pair sends none.
    pub(in crate::agent::codex) fn add_thread_attachment(
        &self,
        request: AgentAttachmentAddRequest,
    ) -> Receiver<Result<AgentAttachmentAdded, AgentAttachmentError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| {
                use super::super::attachments::{add_params, parse_add_response};
                let connection = manager
                    .connection_for_generation(request.generation)
                    .map_err(|error| AgentAttachmentError::Failed(format!("{error:#}")))?;
                if Self::attachment_cache(&connection, |cache| cache.unsupported).unwrap_or(false) {
                    return Err(AgentAttachmentError::Unsupported);
                }
                let response = connection.request("thread/attachment/add", add_params(&request));
                let _ = Self::attachment_cache(&connection, |cache| {
                    cache.invalidate(&request.thread_id)
                });
                parse_add_response(&response.map_err(attachment_error)?, &request)
                    .map_err(|error| AgentAttachmentError::Failed(format!("{error:#}")))
            })();
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(in crate::agent::codex) fn remove_thread_attachment(
        &self,
        request: AgentAttachmentRemoveRequest,
    ) -> Receiver<Result<(), AgentAttachmentError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| {
                use super::super::attachments::{parse_remove_response, remove_params};
                let connection = manager
                    .connection_for_generation(request.generation)
                    .map_err(|error| AgentAttachmentError::Failed(format!("{error:#}")))?;
                if Self::attachment_cache(&connection, |cache| cache.unsupported).unwrap_or(false) {
                    return Err(AgentAttachmentError::Unsupported);
                }
                let response =
                    connection.request("thread/attachment/remove", remove_params(&request));
                let _ = Self::attachment_cache(&connection, |cache| {
                    cache.invalidate(&request.thread_id)
                });
                parse_remove_response(&response.map_err(attachment_error)?)
                    .map_err(|error| AgentAttachmentError::Failed(format!("{error:#}")))
            })();
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    /// Stops every background terminal of the thread on the generation that
    /// ran them. Only the acknowledgement is reported: each command's end
    /// arrives as its own `item/completed`. Not retried.
    pub(in crate::agent::codex) fn clean_background_terminals(
        &self,
        thread_id: String,
        generation: u64,
    ) -> Receiver<Result<(), String>> {
        self.spawn_call(move |manager| {
            use super::super::shell::{
                BACKGROUND_TERMINALS_CLEAN_METHOD, clean_params, parse_clean_ack,
            };
            let connection = manager.connection_for_generation(generation)?;
            manager.validate_temporary_thread(&connection, &thread_id)?;
            let response =
                connection.request(BACKGROUND_TERMINALS_CLEAN_METHOD, clean_params(&thread_id))?;
            if let Err(error) = parse_clean_ack(&response) {
                connection.fail_protocol(format!(
                    "无法解析 {BACKGROUND_TERMINALS_CLEAN_METHOD} 响应；与 app-server schema 不匹配：{error:#}"
                ));
                return Err(error);
            }
            Ok(())
        })
    }
}
