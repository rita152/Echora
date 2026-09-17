//! Connection-scoped and turn-scoped events delivered by agent backends.

use super::{
    activity::{
        AgentCollaboration, AgentContextCompaction, AgentFileChange, AgentFileChangeEntry,
        AgentImageGeneration, AgentImageView, AgentMcpToolCall, AgentReasoning, CommandExecution,
    },
    auto_approval::{AgentAutoApprovalReview, AgentGuardianWarning, AgentStrictReviewRequirement},
    catalog::AgentThreadSettings,
    requests::{
        AgentApprovalHandle, AgentCommandApprovalRequest, AgentFileApprovalHandle,
        AgentFileApprovalRequest, AgentMcpElicitationHandle, AgentMcpElicitationIdentity,
        AgentMcpElicitationRequest, AgentPermissionsApprovalHandle,
        AgentPermissionsApprovalRequest, AgentServerRequestFailureKind, AgentServerRequestMetadata,
        AgentUserInputHandle, AgentUserInputRequest,
    },
    status::{
        AgentConfigWarning, AgentMcpServerStartupStatus, AgentThreadStatus, AgentThreadTokenUsage,
    },
    thread::{ProjectChange, ProjectId, ThreadId},
};

/// Agent-neutral events whose lifetime belongs to a backend connection or a
/// loaded thread rather than to one particular turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentConnectionEvent {
    DeprecationNotice(super::runtime::AgentDeprecationNotice),
    Runtime(super::runtime::AgentRuntimeEvent),
    AutoApprovalReviewUpdated(Box<AgentAutoApprovalReview>),
    StrictReviewRequired(AgentStrictReviewRequirement),
    GuardianWarning(AgentGuardianWarning),
    Warning {
        thread_id: Option<String>,
        message: String,
    },
    ConfigWarning(AgentConfigWarning),
    McpServerStartupStatusUpdated(super::mcp::AgentMcpStartupStatusUpdated),
    /// A watched skill file changed. Treated as an invalidation signal, never
    /// as authoritative data.
    SkillsChanged {
        generation: u64,
    },
    /// `app/list/updated` arrived. The payload is schema-checked on the way in,
    /// but the notification is only an invalidation signal: the directory is
    /// re-read with `app/list`, so a value the user is looking at is never
    /// replaced by a notification payload.
    AppListUpdated {
        generation: u64,
    },
    /// Result of an OAuth login this client started, correlated to the client's
    /// own login id.
    McpOauthLoginCompleted(Box<super::mcp::AgentMcpOauthCompletion>),
    ThreadStatusChanged(AgentThreadStatus),
    ThreadTokenUsageUpdated(AgentThreadTokenUsage),
    ThreadSettingsUpdated {
        thread_id: String,
        generation: u64,
        settings: AgentThreadSettings,
    },
    ProjectChanged {
        project_id: ProjectId,
        change: ProjectChange,
    },
    ThreadArchived {
        thread_id: ThreadId,
    },
    ThreadUnarchived {
        thread_id: ThreadId,
    },
    ThreadDeleted {
        thread_id: ThreadId,
    },
    ThreadNameUpdated {
        thread_id: ThreadId,
        name: Option<String>,
    },
    ThreadClosed {
        thread_id: ThreadId,
    },
    /// The durable history of one thread was replaced with the prefix before a
    /// turn. Published when a revert was not requested by this client, or when
    /// the request failed after the server had already confirmed it.
    ThreadReverted {
        thread_id: ThreadId,
    },
    ThreadProjectUpdated {
        thread_id: ThreadId,
        project_id: Option<ProjectId>,
    },
    /// Account answer for this connection generation. Nullable fields and a
    /// missing account stay distinguishable, and the event replays to new
    /// subscribers.
    AccountUpdated(super::account::AgentAccountSnapshot),
    /// Login request, cancellation, and completion state for this generation.
    AccountLoginUpdated(super::account::AgentAccountLoginState),
    /// Complete quota snapshot keyed by accountId + limitId.
    AccountRateLimitsUpdated(super::account::AgentAccountRateLimitsState),
    /// A standalone MCP elicitation for one thread. It is not turn-scoped and
    /// stays answerable while no turn is active, so it is published on the
    /// connection hub instead of a turn event channel.
    McpElicitationRequested {
        request: AgentMcpElicitationRequest,
        responder: AgentMcpElicitationHandle,
    },
    McpElicitationResolved {
        identity: AgentMcpElicitationIdentity,
        thread_id: String,
    },
    McpElicitationFailed {
        identity: AgentMcpElicitationIdentity,
        thread_id: String,
        kind: AgentServerRequestFailureKind,
        message: String,
    },
}

/// Agent-neutral output consumed by the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEvent {
    HookPromptUpdated(super::runtime::AgentHookPrompt),
    ThreadCreated {
        thread_id: String,
    },
    Started,
    AutoApprovalReviewUpdated(Box<AgentAutoApprovalReview>),
    StrictReviewRequired(AgentStrictReviewRequirement),
    GuardianWarning(AgentGuardianWarning),
    TurnReady(super::backend::AgentTurnIdentity),
    UserMessage {
        item_id: String,
        client_message_id: Option<String>,
        text: String,
        images: Vec<super::thread::UserMessageAttachment>,
    },
    Error {
        message: String,
        details: Option<String>,
        will_retry: bool,
    },
    ThreadSettingsUpdated(AgentThreadSettings),
    Warning {
        message: String,
    },
    ConfigWarning(AgentConfigWarning),
    McpServerStartupStatusUpdated(AgentMcpServerStartupStatus),
    ThreadStatusChanged(AgentThreadStatus),
    ThreadTokenUsageUpdated(AgentThreadTokenUsage),
    AssistantMessageStarted {
        item_id: String,
        phase: Option<String>,
    },
    TextDelta {
        item_id: String,
        delta: String,
    },
    AssistantMessageCompleted {
        item_id: String,
        text: String,
        phase: Option<String>,
    },
    TurnTimingUpdated {
        started_at: Option<i64>,
        completed_at: Option<i64>,
        duration_ms: Option<i64>,
    },
    ReasoningStarted {
        reasoning: AgentReasoning,
        started_at_ms: i64,
    },
    ReasoningSummaryPartAdded {
        item_id: String,
        summary_index: usize,
    },
    ReasoningSummaryTextDelta {
        item_id: String,
        summary_index: usize,
        delta: String,
    },
    ReasoningTextDelta {
        item_id: String,
        content_index: usize,
        delta: String,
    },
    ReasoningCompleted {
        reasoning: AgentReasoning,
        completed_at_ms: i64,
    },
    CommandStarted(CommandExecution),
    CommandOutputDelta {
        item_id: String,
        delta: String,
    },
    CommandTerminalInteraction {
        item_id: String,
        process_id: String,
        /// Preserve the interaction semantic without retaining possibly
        /// sensitive terminal input in the UI model.
        wrote_stdin: bool,
    },
    CommandCompleted(CommandExecution),
    FileChangeUpdated(AgentFileChange),
    ImageViewed(AgentImageView),
    FunctionCallOutputUpdated(super::activity::AgentFunctionCallOutput),
    DynamicToolCallUpdated(super::activity::AgentDynamicToolCall),
    ReviewModeUpdated(super::activity::AgentReviewMode),
    ImageGenerationUpdated(AgentImageGeneration),
    PlanUpdated(super::activity::AgentPlan),
    PlanDelta {
        item_id: String,
        delta: String,
    },
    TurnPlanUpdated(super::activity::AgentTurnPlan),
    WebSearchUpdated(super::activity::AgentWebSearch),
    SleepUpdated(super::activity::AgentSleep),
    ContextCompactionUpdated(AgentContextCompaction),
    CollaborationUpdated(AgentCollaboration),
    McpToolCallUpdated(AgentMcpToolCall),
    McpToolCallProgress {
        item_id: String,
        message: String,
    },
    FileChangePatchUpdated {
        item_id: String,
        changes: Vec<AgentFileChangeEntry>,
    },
    TurnDiffUpdated {
        diff: String,
    },
    CommandApprovalRequested {
        request: AgentCommandApprovalRequest,
        responder: AgentApprovalHandle,
    },
    FileApprovalRequested {
        request: AgentFileApprovalRequest,
        responder: AgentFileApprovalHandle,
    },
    UserInputRequested {
        request: AgentUserInputRequest,
        responder: AgentUserInputHandle,
    },
    PermissionsApprovalRequested {
        request: AgentPermissionsApprovalRequest,
        responder: AgentPermissionsApprovalHandle,
    },
    ServerRequestResolved {
        request: AgentServerRequestMetadata,
    },
    ServerRequestFailed {
        request: AgentServerRequestMetadata,
        kind: AgentServerRequestFailureKind,
        message: String,
    },
    ModelRerouted {
        from_model: String,
        to_model: String,
        reason: String,
    },
    ModelVerificationRequired {
        verifications: Vec<String>,
    },
    ModelSafetyBufferingUpdated {
        model: String,
        use_cases: Vec<String>,
        reasons: Vec<String>,
        show_buffering_ui: bool,
        faster_model: Option<String>,
    },
    Completed,
    Interrupted,
    Failed(String),
}
