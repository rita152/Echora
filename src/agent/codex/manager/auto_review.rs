//! Approving an auto-review denial.
//!
//! One request per thread may be in flight, and a review approved in this
//! generation is never sent again. The request only records the approval; the
//! agent's retry still goes through auto-review.

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Receiver;
use serde_json::json;

use super::CodexAppServerManager;
use crate::agent::{AgentAutoApprovalReviewStatus, AgentAutoReviewApproval};

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn approve_auto_review_denial(
        &self,
        request: AgentAutoReviewApproval,
    ) -> Receiver<Result<(), String>> {
        self.spawn_call(move |manager| {
            let review = &request.review;
            if review.status != AgentAutoApprovalReviewStatus::Denied {
                bail!("只有被拒绝的自动审核可以批准");
            }
            let event = super::super::auto_approval::denial_event(&review.source)?;
            let connection = manager.connection_for_generation(request.generation)?;
            let thread_id = review.key.thread_id.clone();
            manager.validate_temporary_thread(&connection, &thread_id)?;
            {
                let mut state = connection
                    .state
                    .lock()
                    .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
                if state.approved_reviews.contains(&review.key.review_id) {
                    bail!("这条自动审核拒绝已记录批准");
                }
                if !state.approving_review_threads.insert(thread_id.clone()) {
                    bail!("正在记录另一条批准，请稍后再试");
                }
            }
            let result = connection
                .request(
                    "thread/approveGuardianDeniedAction",
                    json!({ "threadId": thread_id, "event": event }),
                )
                .and_then(|response| {
                    response
                        .get("result")
                        .filter(|result| result.is_object())
                        .map(|_| ())
                        .context("thread/approveGuardianDeniedAction 响应缺少 result 对象")
                });
            let mut state = connection
                .state
                .lock()
                .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
            state.approving_review_threads.remove(&thread_id);
            if result.is_ok() {
                state.approved_reviews.insert(review.key.review_id.clone());
            }
            result
        })
    }
}
