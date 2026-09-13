//! Tool activity shared by live events and restored conversation history.

use std::{collections::BTreeMap, path::PathBuf};

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCollaborationTool {
    SpawnAgent,
    SendInput,
    ResumeAgent,
    Wait,
    CloseAgent,
    SendMessage,
    FollowupTask,
    InterruptAgent,
    ListAgents,
    /// The persisted `subAgentActivity` item predates tool attribution.
    LegacyActivity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCollaborationStatus {
    InProgress,
    Completed,
    Failed,
    Interrupted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCollaboratorStatus {
    PendingInit,
    Running,
    Interrupted,
    Completed,
    Errored,
    Shutdown,
    NotFound,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCollaboratorState {
    pub status: AgentCollaboratorStatus,
    pub message: Option<String>,
    /// Optional product-facing label supplied by newer collaboration items.
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacySubAgentActivityKind {
    Started,
    Interacted,
    Interrupted,
    Completed,
}

/// Agent-neutral form shared by the public `collabToolCall`, the installed
/// desktop app's `collabAgentToolCall`, and persisted `subAgentActivity` items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCollaboration {
    pub id: String,
    pub tool: AgentCollaborationTool,
    pub status: AgentCollaborationStatus,
    pub sender_thread_id: String,
    pub receiver_thread_ids: Vec<String>,
    pub agents_states: BTreeMap<String, AgentCollaboratorState>,
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub legacy_agent_path: Option<String>,
    pub legacy_kind: Option<LegacySubAgentActivityKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentContextCompaction {
    pub id: String,
    pub completed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentImageView {
    pub id: String,
    pub path: PathBuf,
}

/// Agent-neutral representation of the app-server `functionCallOutput` thread
/// item, whose `output` field is either a plain string or a list of responses
/// API compatible content items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentFunctionCallOutputBody {
    Text(String),
    Items(Vec<AgentFunctionCallOutputContentItem>),
}

/// The subset of `ContentItem` the protocol accepts as a function call output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentFunctionCallOutputContentItem {
    Text {
        text: String,
    },
    Image {
        image_url: String,
        detail: Option<AgentImageDetail>,
    },
    Audio {
        audio_url: String,
    },
    Encrypted {
        encrypted_content: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentImageDetail {
    Auto,
    Low,
    High,
    Original,
}

/// Agent-neutral representation of the app-server `functionCallOutput` thread
/// item. `completed` reflects the item lifecycle rather than a protocol field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFunctionCallOutput {
    pub id: String,
    pub name: String,
    pub namespace: Option<String>,
    pub output: AgentFunctionCallOutputBody,
    pub completed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentDynamicToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

/// The subset of dynamic tool call output content the protocol defines, one
/// variant per `DynamicToolCallOutputContentItem` type (`inputText`,
/// `inputImage`, and `inputAudio` on the wire).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentDynamicToolCallContentItem {
    Text { text: String },
    Image { image_url: String },
    Audio { audio_url: String },
}

/// Agent-neutral representation of the app-server `dynamicToolCall` thread
/// item. `completed` reflects the item lifecycle, matching how the reference
/// client keeps an item row settled once either lifecycle event arrives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentDynamicToolCall {
    pub id: String,
    pub tool: String,
    pub namespace: Option<String>,
    pub arguments: Value,
    pub status: AgentDynamicToolCallStatus,
    pub success: Option<bool>,
    pub content_items: Option<Vec<AgentDynamicToolCallContentItem>>,
    pub duration_ms: Option<i64>,
    pub completed: bool,
}

/// Agent-neutral representation of the app-server `enteredReviewMode` and
/// `exitedReviewMode` thread items. The schema carries no fields beyond the
/// item id and the review label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentReviewMode {
    pub id: String,
    pub review: String,
    pub entered: bool,
    pub completed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentImageGenerationStatus {
    InProgress,
    Completed,
    Failed,
}

/// Agent-neutral representation of the app-server `mcpToolCall` thread item.
///
/// JSON-valued fields intentionally remain lossless: connector arguments,
/// results, and app context are owned by the selected MCP server and can gain
/// server-specific members independently of this client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpToolCall {
    pub id: String,
    pub server: String,
    pub tool: String,
    pub status: AgentMcpToolCallStatus,
    pub arguments: Value,
    pub app_context: Option<Value>,
    pub plugin_id: Option<String>,
    pub result: Option<Value>,
    pub error: Option<String>,
    /// Deprecated persisted metadata retained for histories written before
    /// `appContext.resourceUri` was introduced.
    pub legacy_resource_uri: Option<String>,
    pub read_only_hint: Option<bool>,
    pub duration_ms: Option<i64>,
    /// Live `item/mcpToolCall/progress` messages observed for this item.
    pub progress: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentImageGenerationFailure {
    UsageLimitExceeded {
        limit_id: String,
        resets_at: Option<i64>,
    },
}

/// Canonical, UI-ready representation of an app-server `imageGeneration` item.
///
/// The adapter resolves the potentially very large base64 `result` to a local
/// file before emitting this value. Keeping transport bytes out of the UI model
/// makes live upserts and paginated history hydration inexpensive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentImageGeneration {
    pub id: String,
    pub status: AgentImageGenerationStatus,
    pub revised_prompt: Option<String>,
    pub path: Option<PathBuf>,
    pub dimensions: Option<(u32, u32)>,
    pub transparent_background: Option<bool>,
    pub failure: Option<AgentImageGenerationFailure>,
    pub load_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentFileChangeKind {
    Add,
    Delete,
    Update { move_path: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFileChangeEntry {
    pub path: String,
    pub diff: String,
    pub kind: AgentFileChangeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentFileChangeStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFileChange {
    pub id: String,
    pub changes: Vec<AgentFileChangeEntry>,
    pub status: AgentFileChangeStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandExecutionStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandExecutionAction {
    Read {
        command: String,
        name: String,
        path: String,
    },
    ListFiles {
        command: String,
        path: Option<String>,
    },
    Search {
        command: String,
        path: Option<String>,
        query: Option<String>,
    },
    Unknown {
        command: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandExecution {
    pub id: String,
    pub command: String,
    pub actions: Vec<CommandExecutionAction>,
    pub cwd: String,
    pub output: String,
    /// Set by a terminal-interaction notification while a background process
    /// remains associated with this command item.
    pub terminal_process_id: Option<String>,
    pub status: CommandExecutionStatus,
    pub exit_code: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentReasoning {
    pub id: String,
    pub summary: Vec<String>,
    pub content: Vec<String>,
}

/// Item lifecycle is independent of the enclosing turn outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentActivityStatus {
    InProgress,
    Completed,
    Interrupted,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPlan {
    pub id: String,
    pub text: String,
    pub status: AgentActivityStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPlanStepStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPlanStep {
    pub step: String,
    pub status: AgentPlanStepStatus,
}

/// Turn-level progress does not contain or replace a proposed plan's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTurnPlan {
    pub turn_id: String,
    pub explanation: Option<String>,
    pub steps: Vec<AgentPlanStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentWebSearch {
    pub id: String,
    pub query: String,
    pub action: Value,
    pub results: Value,
    pub extra: BTreeMap<String, Value>,
    pub status: AgentActivityStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSleep {
    pub id: String,
    pub duration_ms: u64,
    pub status: AgentActivityStatus,
}
