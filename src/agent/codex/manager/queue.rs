//! Server-side follow-up queue operations.
//!
//! The server advances the queue by itself when a turn completes; that turn is
//! routed like any other server-started turn. `thread/queue/changed` is only an
//! invalidation signal, so callers re-list after every change.

use anyhow::{Context as _, Result};
use async_channel::Receiver;
use serde_json::json;

use super::CodexAppServerManager;
use crate::agent::{
    AgentQueueAddRequest, AgentQueueReorderRequest, AgentQueueTarget, AgentQueueUpdateRequest,
    AgentQueuedSubmission, AgentThreadQueue,
};

impl CodexAppServerManager {
    /// Reads every page of one thread's queue, rejecting repeated cursors and
    /// duplicate ids. A read may open a connection; the generation it used is
    /// part of the answer.
    pub(in crate::agent::codex) fn list_thread_queue(
        &self,
        thread_id: String,
    ) -> Receiver<Result<AgentThreadQueue, String>> {
        self.spawn_call(move |manager| {
            let connection = manager.inner.ensure_connection()?;
            manager.validate_temporary_thread(&connection, &thread_id)?;
            let mut pages = super::super::queue::QueueListAccumulator::default();
            let mut cursor = None;
            loop {
                let response = connection.request(
                    "thread/queue/list",
                    super::super::queue::list_params(&thread_id, cursor.as_deref()),
                )?;
                let (page, next) = super::super::queue::parse_list_page(&response)?;
                match pages.push(page, next)? {
                    Some(next) => cursor = Some(next),
                    None => break,
                }
            }
            Ok(AgentThreadQueue {
                generation: connection.generation,
                thread_id,
                submissions: pages.submissions,
            })
        })
    }

    pub(in crate::agent::codex) fn add_queued_submission(
        &self,
        request: AgentQueueAddRequest,
    ) -> Receiver<Result<AgentQueuedSubmission, String>> {
        self.spawn_call(move |manager| {
            let connection = manager.connection_for_generation(request.generation)?;
            manager.validate_temporary_thread(&connection, &request.thread_id)?;
            let input = super::super::input::encode_input(&request.prompt, &request.context)?;
            let response = connection.request(
                "thread/queue/add",
                json!({
                    "threadId": request.thread_id,
                    "input": input,
                    "clientUserMessageId": request.client_message_id,
                }),
            )?;
            super::super::queue::parse_add_response(&response, &request.client_message_id)
        })
    }

    pub(in crate::agent::codex) fn update_queued_submission(
        &self,
        request: AgentQueueUpdateRequest,
    ) -> Receiver<Result<AgentQueuedSubmission, String>> {
        self.spawn_call(move |manager| {
            let connection = manager.connection_for_generation(request.generation)?;
            manager.validate_temporary_thread(&connection, &request.thread_id)?;
            let input = super::super::input::encode_input(&request.prompt, &request.context)?;
            let response = connection.request(
                "thread/queue/update",
                json!({
                    "threadId": request.thread_id,
                    "queuedSubmissionId": request.queued_submission_id,
                    "input": input,
                }),
            )?;
            super::super::queue::parse_update_response(&response, &request.queued_submission_id)
        })
    }

    /// `deleted=false` is returned as-is: the caller decides whether a missing
    /// submission is an error (it is, for "steer now").
    pub(in crate::agent::codex) fn delete_queued_submission(
        &self,
        target: AgentQueueTarget,
    ) -> Receiver<Result<bool, String>> {
        self.spawn_call(move |manager| {
            let connection = manager.connection_for_generation(target.generation)?;
            manager.validate_temporary_thread(&connection, &target.thread_id)?;
            let id = target
                .queued_submission_id
                .context("thread/queue/delete 需要 queuedSubmissionId")?;
            let response = connection.request(
                "thread/queue/delete",
                json!({ "threadId": target.thread_id, "queuedSubmissionId": id }),
            )?;
            super::super::queue::parse_delete_response(&response)
        })
    }

    pub(in crate::agent::codex) fn reorder_queued_submissions(
        &self,
        request: AgentQueueReorderRequest,
    ) -> Receiver<Result<(), String>> {
        self.spawn_call(move |manager| {
            let connection = manager.connection_for_generation(request.generation)?;
            manager.validate_temporary_thread(&connection, &request.thread_id)?;
            let response = connection.request(
                "thread/queue/reorder",
                json!({
                    "threadId": request.thread_id,
                    "queuedSubmissionIds": request.queued_submission_ids,
                }),
            )?;
            super::super::queue::parse_reorder_response(&response)
        })
    }

    /// Starts one queued submission (or the queue head) on an idle thread. The
    /// thread is loaded first; the started turn itself is delivered as a
    /// server-started turn, and the returned id only confirms the request.
    pub(in crate::agent::codex) fn start_queued_submission(
        &self,
        target: AgentQueueTarget,
    ) -> Receiver<Result<String, String>> {
        self.spawn_call(move |manager| {
            let connection = manager.connection_for_generation(target.generation)?;
            manager.ensure_thread_loaded(&connection, Some(&target.thread_id), None)?;
            let response = connection.request(
                "thread/queue/start",
                json!({
                    "threadId": target.thread_id,
                    "queuedSubmissionId": target.queued_submission_id,
                }),
            )?;
            super::super::queue::parse_start_response(&response)
        })
    }
}
