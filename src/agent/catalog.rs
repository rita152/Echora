//! Model catalog and effective thread configuration.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentReasoningEffort {
    pub id: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentServiceTier {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentModel {
    pub id: String,
    pub model: String,
    pub display_name: String,
    pub description: String,
    pub supported_reasoning_efforts: Vec<AgentReasoningEffort>,
    pub default_reasoning_effort: String,
    pub service_tiers: Vec<AgentServiceTier>,
    pub default_service_tier: Option<String>,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentModelCatalog {
    pub models: Vec<AgentModel>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentPermissionMode {
    Request,
    Assist,
    Full,
    Custom,
    Profile(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentActivePermissionProfile {
    pub id: String,
    pub extends: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentEffectivePermissions {
    pub approval_policy: Value,
    pub approvals_reviewer: String,
    pub sandbox_policy: Option<Value>,
    pub active_permission_profile: Option<AgentActivePermissionProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPermissionProfile {
    pub id: String,
    pub description: Option<String>,
    pub allowed: bool,
    pub extends: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadSettings {
    pub model: String,
    pub effort: Option<String>,
    pub service_tier: Option<String>,
    pub cwd: String,
    pub permissions: Option<AgentEffectivePermissions>,
}

/// A permission mutation is scoped to the original view operation and connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadPermissionUpdate {
    pub thread_id: String,
    pub cwd: std::path::PathBuf,
    pub mode: AgentPermissionMode,
    pub expected_generation: Option<u64>,
    pub operation_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadPermissionResult {
    pub thread_id: String,
    pub generation: u64,
    pub operation_id: u64,
    pub settings: AgentThreadSettings,
    /// Set when the confirmed change named a reviewer while a turn of the
    /// thread was running: the reviewer is then also published to that turn.
    pub active_turn_reviewer: Option<AgentActiveTurnReviewerUpdate>,
}

/// How `turn/settings/update` answered for the reviewer of the running turn.
/// Steps the turn already captured and pending approvals keep their reviewer
/// either way; the other permission fields only apply from the next turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentActiveTurnReviewerUpdate {
    /// The running turn's later captures use the new reviewer.
    Applied { turn_id: String },
    /// No live task remained: the turn ended first. Not a failure.
    TargetUnavailable { turn_id: String },
    /// The request failed; the running turn keeps its original reviewer.
    Failed { turn_id: String, message: String },
}

/// `TurnSettingsUpdateResponse.status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentTurnSettingsStatus {
    Applied,
    TargetUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadSettingsSnapshot {
    pub thread_id: String,
    pub generation: u64,
    pub settings: AgentThreadSettings,
}
