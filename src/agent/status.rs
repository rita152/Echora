//! Connection, thread, and account status snapshots.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigWarning {
    pub summary: String,
    pub details: Option<String>,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub column: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpServerStartupState {
    Starting,
    Ready,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpServerStartupFailureReason {
    ReauthenticationRequired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpServerStartupStatus {
    pub thread_id: Option<String>,
    pub name: String,
    pub state: AgentMcpServerStartupState,
    pub error: Option<String>,
    pub failure_reason: Option<AgentMcpServerStartupFailureReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentThreadActiveFlag {
    WaitingOnApproval,
    WaitingOnUserInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentThreadStatusState {
    NotLoaded,
    Idle,
    SystemError,
    Active {
        active_flags: Vec<AgentThreadActiveFlag>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadStatus {
    pub thread_id: String,
    pub state: AgentThreadStatusState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTokenUsageBreakdown {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadTokenUsage {
    pub thread_id: String,
    pub turn_id: String,
    pub total: AgentTokenUsageBreakdown,
    pub last: AgentTokenUsageBreakdown,
    pub model_context_window: Option<i64>,
}
