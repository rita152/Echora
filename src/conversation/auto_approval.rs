//! Review identity, monotonic updates, and cleanup. No approval RPCs live here.

use super::{ConversationActivity, ConversationPhase, ConversationState};
use crate::agent::{
    AgentAutoApprovalReview, AgentAutoApprovalReviewStatus, AgentEvent, AgentGuardianWarning,
    AgentStrictReviewRequirement,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutoApprovalReviewPresentation {
    pub review: AgentAutoApprovalReview,
    /// Turn cleanup is local evidence, never an invented server decision/time.
    pub closed_locally: bool,
    pub attached_to_item: bool,
    /// The user's approval of a denied review.
    pub approval: ReviewApproval,
}

/// Approving a denial records it for one retry; it never runs the action.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum ReviewApproval {
    #[default]
    Idle,
    Approving,
    Approved,
    Failed(String),
}

impl AutoApprovalReviewPresentation {
    pub fn status(&self) -> AgentAutoApprovalReviewStatus {
        if self.closed_locally && self.review.status == AgentAutoApprovalReviewStatus::InProgress {
            AgentAutoApprovalReviewStatus::Aborted
        } else {
            self.review.status
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StrictReviewPresentation {
    pub requirement: AgentStrictReviewRequirement,
    pub turn_finished: bool,
}

fn terminal(phase: ConversationPhase) -> bool {
    matches!(
        phase,
        ConversationPhase::Complete | ConversationPhase::Stopped | ConversationPhase::Failed
    )
}

fn upsert(
    activities: &mut Vec<ConversationActivity>,
    review: AgentAutoApprovalReview,
    finished: bool,
) {
    if let Some(ConversationActivity::AutoApprovalReview(existing)) = activities.iter_mut().find(|a|
        matches!(a, ConversationActivity::AutoApprovalReview(existing) if existing.review.key == review.key)
    ) {
        // A repeated/late start cannot roll a completed review back to inProgress.
        // Completed messages carry the full action and server timestamps.
        if existing.review.completed_at_ms.is_some() && review.completed_at_ms.is_none() { return; }
        if existing.review.status != AgentAutoApprovalReviewStatus::InProgress
            && review.status == AgentAutoApprovalReviewStatus::InProgress { return; }
        if let (Some(old), Some(new)) = (existing.review.completed_at_ms, review.completed_at_ms)
            && new < old { return; }
        let mut review = review;
        if review.rationale.is_none() { review.rationale.clone_from(&existing.review.rationale); }
        if review.risk_level.is_none() { review.risk_level.clone_from(&existing.review.risk_level); }
        if review.user_authorization.is_none() { review.user_authorization.clone_from(&existing.review.user_authorization); }
        existing.review = review;
        existing.closed_locally |= finished;
    } else {
        activities.push(ConversationActivity::AutoApprovalReview(Box::new(AutoApprovalReviewPresentation { review, closed_locally: finished, attached_to_item: false, approval: ReviewApproval::Idle })));
    }
}

impl ConversationState {
    pub(crate) fn apply_auto_approval_review(&mut self, review: AgentAutoApprovalReview) {
        if self.thread_id.as_deref() != Some(review.key.thread_id.as_str()) {
            return;
        }
        if let Some(turn) = self
            .transcript
            .iter_mut()
            .find(|t| t.turn_id.as_deref() == Some(review.key.turn_id.as_str()))
        {
            upsert(&mut turn.activities, review, terminal(turn.phase));
        } else if self.turn_id.as_deref() == Some(review.key.turn_id.as_str()) {
            upsert(&mut self.activities, review, terminal(self.phase));
        } else {
            self.queue_review_event(
                review.key.turn_id.clone(),
                AgentEvent::AutoApprovalReviewUpdated(Box::new(review)),
            );
        }
    }

    pub(crate) fn apply_strict_review(&mut self, requirement: AgentStrictReviewRequirement) {
        if self.thread_id.as_deref() != Some(requirement.thread_id.as_str()) {
            return;
        }
        let (activities, finished) = if let Some(turn) = self
            .transcript
            .iter_mut()
            .find(|t| t.turn_id.as_deref() == Some(requirement.turn_id.as_str()))
        {
            (&mut turn.activities, terminal(turn.phase))
        } else if self.turn_id.as_deref() == Some(requirement.turn_id.as_str()) {
            (&mut self.activities, terminal(self.phase))
        } else {
            self.queue_review_event(
                requirement.turn_id.clone(),
                AgentEvent::StrictReviewRequired(requirement),
            );
            return;
        };
        // There is no reviewId here. Distinct server start times remain independent.
        if !activities.iter().any(|a| matches!(a, ConversationActivity::StrictReview(existing) if existing.requirement == requirement)) {
            activities.push(ConversationActivity::StrictReview(StrictReviewPresentation { requirement, turn_finished: finished }));
        }
    }

    pub(crate) fn apply_guardian_warning(&mut self, warning: AgentGuardianWarning) {
        if self.thread_id.as_deref() != Some(warning.thread_id.as_str()) {
            return;
        }
        if self.activities.iter()
            .any(|a| matches!(a, ConversationActivity::GuardianWarning(existing) if existing == &warning)) { return; }
        self.activities
            .push(ConversationActivity::GuardianWarning(warning));
    }

    pub(crate) fn close_auto_approval_reviews(&mut self) {
        for activity in &mut self.activities {
            match activity {
                ConversationActivity::AutoApprovalReview(review) => review.closed_locally = true,
                ConversationActivity::StrictReview(requirement) => requirement.turn_finished = true,
                _ => {}
            }
        }
    }

    fn review_presentation_mut(
        &mut self,
        key: &crate::agent::AgentAutoApprovalReviewKey,
    ) -> Option<&mut AutoApprovalReviewPresentation> {
        self.transcript
            .iter_mut()
            .flat_map(|turn| turn.activities.iter_mut())
            .chain(self.activities.iter_mut())
            .find_map(|activity| match activity {
                ConversationActivity::AutoApprovalReview(review) if &review.review.key == key => {
                    Some(review.as_mut())
                }
                _ => None,
            })
    }

    /// The newest denied reviews that can still be approved, for the
    /// `/autoreview` menu: at most `limit`, newest first.
    pub(crate) fn approvable_denials(&self, limit: usize) -> Vec<AutoApprovalReviewPresentation> {
        let mut denials = self
            .transcript
            .iter()
            .flat_map(|turn| turn.activities.iter())
            .chain(self.activities.iter())
            .filter_map(|activity| match activity {
                ConversationActivity::AutoApprovalReview(review)
                    if review.review.status == AgentAutoApprovalReviewStatus::Denied
                        && review.approval != ReviewApproval::Approved
                        && !self.approved_reviews.contains(&review.review.key.review_id) =>
                {
                    Some((**review).clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        denials.sort_by_key(|review| {
            std::cmp::Reverse(
                review
                    .review
                    .completed_at_ms
                    .unwrap_or(review.review.started_at_ms),
            )
        });
        denials.truncate(limit);
        denials
    }

    /// Starts approving one denied review. Refuses a review that is not
    /// denied, already approved, or while another approval of this
    /// conversation is in flight, so a repeated click sends nothing.
    pub(crate) fn begin_review_approval(
        &mut self,
        key: &crate::agent::AgentAutoApprovalReviewKey,
    ) -> Option<crate::agent::AgentAutoReviewApproval> {
        if self.approving_review.is_some()
            || self.approved_reviews.contains(&key.review_id)
            || self.thread_id.as_deref() != Some(key.thread_id.as_str())
        {
            return None;
        }
        let generation = self.runtime.generation;
        let presentation = self.review_presentation_mut(key)?;
        if presentation.review.status != AgentAutoApprovalReviewStatus::Denied
            || presentation.approval == ReviewApproval::Approved
        {
            return None;
        }
        presentation.approval = ReviewApproval::Approving;
        let review = presentation.review.clone();
        self.approving_review = Some(key.review_id.clone());
        Some(crate::agent::AgentAutoReviewApproval { generation, review })
    }

    /// Applies the answer. An answer for another thread (the user switched
    /// chats meanwhile) is dropped.
    pub(crate) fn finish_review_approval(
        &mut self,
        key: &crate::agent::AgentAutoApprovalReviewKey,
        result: Result<(), String>,
    ) -> bool {
        if self.thread_id.as_deref() != Some(key.thread_id.as_str())
            || self.approving_review.as_deref() != Some(key.review_id.as_str())
        {
            return false;
        }
        self.approving_review = None;
        let approved = result.is_ok();
        if approved {
            self.approved_reviews.insert(key.review_id.clone());
        }
        if let Some(presentation) = self.review_presentation_mut(key) {
            presentation.approval = match result {
                Ok(()) => ReviewApproval::Approved,
                Err(error) => ReviewApproval::Failed(error),
            };
        }
        true
    }

    fn queue_review_event(&mut self, turn_id: String, event: AgentEvent) {
        let pending = self.pending_review_events.entry(turn_id).or_default();
        if !pending.contains(&event) {
            pending.push(event);
        }
    }

    pub(crate) fn replay_pending_reviews(&mut self) {
        let ids = self
            .transcript
            .iter()
            .filter_map(|t| t.turn_id.clone())
            .chain(self.turn_id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            if let Some(events) = self.pending_review_events.remove(&id) {
                self.apply_agent_event_batch(events);
            }
        }
    }
}
