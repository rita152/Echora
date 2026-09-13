//! MCP elicitation lifecycle reduction for one conversation.
//!
//! Elicitations are connection-owned, so they are delivered as connection
//! events and scoped by their own thread id. They never move the conversation
//! phase, never commit or restart a turn, and stay answerable while no turn is
//! active. A card is released only by a matching serverRequest/resolved or by
//! an explicit invalidation (connection loss, closed thread, retired
//! generation).

use super::{activity::ConversationActivity, state::ConversationState};

#[cfg(test)]
mod tests;

use crate::{
    agent::{
        AgentMcpElicitationHandle, AgentMcpElicitationIdentity, AgentMcpElicitationRequest,
        AgentServerRequestFailureKind,
    },
    components::mcp_elicitation::McpElicitationPresentation,
};

pub(crate) fn find_mcp_elicitation_mut<'a>(
    activities: &'a mut [ConversationActivity],
    request_id: &str,
) -> Option<&'a mut McpElicitationPresentation> {
    activities.iter_mut().find_map(|activity| match activity {
        ConversationActivity::McpElicitation(model) if model.request_id == request_id => {
            Some(model.as_mut())
        }
        _ => None,
    })
}

impl ConversationState {
    pub(crate) fn mcp_elicitation_requested(
        &mut self,
        request: AgentMcpElicitationRequest,
        responder: AgentMcpElicitationHandle,
    ) -> bool {
        let identity = request.identity();
        if identity.generation < self.runtime.generation {
            // The generation that owned this request is already retired; its
            // own failure notification invalidated the card, so a late request
            // must not come back as an answerable surface.
            return false;
        }
        let key = identity.ui_key();
        if self.mcp_elicitation_contexts.contains_key(&key) {
            return false;
        }
        let model = McpElicitationPresentation::pending(key.clone(), &request);
        self.mcp_elicitation_contexts.insert(key.clone(), identity);
        self.mcp_elicitation_responders.insert(key, responder);
        self.activities
            .push(ConversationActivity::McpElicitation(Box::new(model)));
        true
    }

    pub(crate) fn mcp_elicitation_resolved(
        &mut self,
        identity: &AgentMcpElicitationIdentity,
    ) -> bool {
        let key = identity.ui_key();
        if self.mcp_elicitation_contexts.get(&key) != Some(identity) {
            return false;
        }
        self.mcp_elicitation_contexts.remove(&key);
        self.mcp_elicitation_responders.remove(&key);
        if let Some(model) = find_mcp_elicitation_mut(&mut self.activities, &key) {
            model.mark_resolved();
        }
        true
    }

    pub(crate) fn mcp_elicitation_failed(
        &mut self,
        identity: &AgentMcpElicitationIdentity,
        kind: AgentServerRequestFailureKind,
        message: String,
    ) -> bool {
        let key = identity.ui_key();
        if self.mcp_elicitation_contexts.get(&key) != Some(identity) {
            return false;
        }
        self.mcp_elicitation_contexts.remove(&key);
        self.mcp_elicitation_responders.remove(&key);
        if let Some(model) = find_mcp_elicitation_mut(&mut self.activities, &key) {
            match kind {
                AgentServerRequestFailureKind::Cancelled => model.mark_cancelled(message),
                AgentServerRequestFailureKind::Failed => model.mark_invalid(message),
            }
        }
        true
    }

    /// Elicitation cards outlive the turn that observed them, so a new prompt
    /// must not drop a request the server still waits on.
    pub(crate) fn retain_pending_mcp_elicitations(&mut self) {
        self.activities
            .retain(ConversationActivity::is_mcp_elicitation);
    }
}

impl ConversationActivity {
    pub(crate) fn is_mcp_elicitation(&self) -> bool {
        matches!(self, Self::McpElicitation(_))
    }
}
