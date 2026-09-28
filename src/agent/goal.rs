//! Thread goals: a server-owned objective that can keep a thread running.

/// Lifecycle of a thread goal. `Active` goals let the server start turns on its
/// own while the thread is idle; every other status stops that continuation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentThreadGoalStatus {
    Active,
    Paused,
    Blocked,
    UsageLimited,
    BudgetLimited,
    Complete,
}

impl AgentThreadGoalStatus {
    pub const ALL: [Self; 6] = [
        Self::Active,
        Self::Paused,
        Self::Blocked,
        Self::UsageLimited,
        Self::BudgetLimited,
        Self::Complete,
    ];
}

/// One goal snapshot exactly as the server reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadGoal {
    pub thread_id: String,
    pub objective: String,
    pub status: AgentThreadGoalStatus,
    pub token_budget: Option<i64>,
    pub tokens_used: i64,
    pub time_used_seconds: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A partial goal update. Omitted fields stay as the server has them; a token
/// budget can additionally be cleared with an explicit null.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadGoalUpdate {
    pub generation: u64,
    pub thread_id: String,
    pub objective: Option<String>,
    pub status: Option<AgentThreadGoalStatus>,
    pub token_budget: super::AgentOptionalField<i64>,
}

/// Goal answers carry the generation and thread they were read for, so a late
/// answer for another connection or thread can never overwrite newer state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadGoalRead {
    pub generation: u64,
    pub thread_id: String,
    pub goal: Option<AgentThreadGoal>,
}
