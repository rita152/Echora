//! Stable application boundary for coding-agent backends.
//! Domain modules depend on each other explicitly; concrete protocols stay in adapters.

mod account;
mod activity;
mod apps;
mod auto_approval;
mod backend;
mod catalog;
mod codex;
mod config;
mod events;
mod file_search;
mod mcp;
mod message;
mod plugins;
mod requests;
mod runtime;
mod skills;
mod status;
mod thread;

pub use account::{
    AgentAccount, AgentAccountAuthMode, AgentAccountLoginPhase, AgentAccountLoginState,
    AgentAccountPlanType, AgentAccountPresence, AgentAccountRateLimitsState, AgentAccountSnapshot,
    AgentAccountState, AgentAccountUpdate, AgentCreditsSnapshot, AgentLoginCancelOutcome,
    AgentLoginChallenge, AgentLoginCompletion, AgentLoginStart, AgentLogoutOutcome,
    AgentRateLimitPatch, AgentRateLimitReachedType, AgentRateLimitResetCredit,
    AgentRateLimitResetCreditStatus, AgentRateLimitResetCredits, AgentRateLimitResetType,
    AgentRateLimitWindow, AgentRateLimitsRead, AgentSpendControlLimit,
};

// Construction helpers the account tests use to build protocol snapshots; the
// application itself only reads these values while reducing connection events.
#[cfg(test)]
pub use account::{AGENT_DEFAULT_RATE_LIMIT_ID, AgentRateLimitBucket};
pub use activity::{
    AgentActivityStatus, AgentCollaboration, AgentCollaborationStatus, AgentCollaborationTool,
    AgentCollaboratorState, AgentCollaboratorStatus, AgentContextCompaction, AgentDynamicToolCall,
    AgentDynamicToolCallContentItem, AgentDynamicToolCallStatus, AgentFileChange,
    AgentFileChangeEntry, AgentFileChangeKind, AgentFileChangeStatus, AgentFunctionCallOutput,
    AgentFunctionCallOutputBody, AgentFunctionCallOutputContentItem, AgentImageDetail,
    AgentImageGeneration, AgentImageGenerationFailure, AgentImageGenerationStatus, AgentImageView,
    AgentMcpToolCall, AgentMcpToolCallStatus, AgentPlan, AgentPlanStep, AgentPlanStepStatus,
    AgentReasoning, AgentReviewMode, AgentSleep, AgentTurnPlan, AgentWebSearch, CommandExecution,
    CommandExecutionAction, CommandExecutionStatus, LegacySubAgentActivityKind,
};
pub use apps::{
    AgentAppBranding, AgentAppInfo, AgentAppMetadata, AgentAppMetadataEntry, AgentAppReview,
    AgentAppScreenshot, AgentAppToolSummary, AgentAppsError, AgentAppsErrorKind,
    AgentAppsInstalledRequest, AgentAppsListRequest, AgentAppsPage, AgentAppsReadRequest,
    AgentAppsReadResult, AgentInstalledApp, AgentInstalledApps,
};
pub use auto_approval::{
    AgentAutoApprovalReview, AgentAutoApprovalReviewAction, AgentAutoApprovalReviewKey,
    AgentAutoApprovalReviewStatus, AgentGuardianWarning, AgentStrictReviewRequirement,
};
pub(crate) use backend::AgentInterruptControl;
pub use backend::{
    AgentBackend, AgentCapabilities, AgentCapability, AgentInputFile, AgentInterruptHandle,
    AgentInterruptOutcome, AgentPromptContext, AgentRequest, AgentRun, AgentSteerRequest,
    AgentTurnIdentity, SideConversationRequest, WorkspaceError, WorkspaceResult,
};
pub use catalog::{
    AgentActivePermissionProfile, AgentEffectivePermissions, AgentModel, AgentModelCatalog,
    AgentPermissionMode, AgentPermissionProfile, AgentReasoningEffort, AgentServiceTier,
    AgentThreadPermissionResult, AgentThreadPermissionUpdate, AgentThreadSettings,
    AgentThreadSettingsSnapshot,
};
pub use codex::{CodexAppServerBackend, CodexAppServerManager};
pub use config::{
    AgentConfigChoiceSet, AgentConfigEdit, AgentConfigError, AgentConfigErrorKind,
    AgentConfigLayer, AgentConfigReceipt, AgentConfigRequirements, AgentConfigSaveResult,
    AgentConfigSnapshot, AgentConfigSource, AgentConfigWrite, config_value,
};
pub use events::{AgentConnectionEvent, AgentEvent};
pub(crate) use file_search::AgentFileSearchSessionControl;
pub use file_search::{
    AgentFileMatchType, AgentFileSearchRequest, AgentFileSearchResult, AgentFileSearchSession,
    AgentFileSearchSessionCompleted, AgentFileSearchSessionEvent, AgentFileSearchSessionUpdate,
};
pub use mcp::{
    AgentMcpAuthStatus, AgentMcpError, AgentMcpErrorKind, AgentMcpOauthClientRegistration,
    AgentMcpOauthCompletion, AgentMcpOauthCompletionStatus, AgentMcpOauthLogin,
    AgentMcpOauthLoginRequest, AgentMcpReloadOutcome, AgentMcpReloadRequest, AgentMcpReloadResult,
    AgentMcpResource, AgentMcpResourceTemplate, AgentMcpServerConnectionStatus, AgentMcpServerInfo,
    AgentMcpServerPage, AgentMcpServerStatus, AgentMcpServerStatusRequest,
    AgentMcpStartupStatusUpdated, AgentMcpStatusDetail, AgentMcpTool,
};
pub use message::normalize_user_message_for_display;
pub(crate) use message::user_message_context_files;
pub use plugins::{
    AgentAppTemplateUnavailableReason, AgentMarketplaceAddReceipt, AgentMarketplaceAddRequest,
    AgentMarketplaceAddResult, AgentMarketplaceInterface, AgentMarketplaceLoadError,
    AgentMarketplaceRemoveReceipt, AgentMarketplaceRemoveRequest, AgentMarketplaceRemoveResult,
    AgentMarketplaceUpgradeError, AgentMarketplaceUpgradeReceipt, AgentMarketplaceUpgradeRequest,
    AgentMarketplaceUpgradeResult, AgentPluginAppSummary, AgentPluginAppTemplateSummary,
    AgentPluginAuthPolicy, AgentPluginAvailability, AgentPluginCatalog, AgentPluginCatalogRequest,
    AgentPluginDetail, AgentPluginDisabledReason, AgentPluginHookSummary, AgentPluginInstallPolicy,
    AgentPluginInstallPolicySource, AgentPluginInstallReceipt, AgentPluginInstallRequest,
    AgentPluginInstallResult, AgentPluginInstalledRequest, AgentPluginInterface,
    AgentPluginMarketplace, AgentPluginMarketplaceKind, AgentPluginOperationOutcome,
    AgentPluginReadRequest, AgentPluginReconcileChangedPlugin, AgentPluginReconcileReceipt,
    AgentPluginReconcileRequest, AgentPluginSearchPage, AgentPluginSearchRequest,
    AgentPluginSearchResult, AgentPluginShareContext, AgentPluginShareDeleteRequest,
    AgentPluginShareDeleteResult, AgentPluginShareDiscoverability, AgentPluginShareList,
    AgentPluginShareListEntry, AgentPluginSharePrincipal, AgentPluginSharePrincipalType,
    AgentPluginShareRole, AgentPluginShareSaveReceipt, AgentPluginShareSaveRequest,
    AgentPluginShareSaveResult, AgentPluginShareTarget, AgentPluginShareUpdateTargetsReceipt,
    AgentPluginShareUpdateTargetsRequest, AgentPluginShareUpdateTargetsResult,
    AgentPluginSkillContent, AgentPluginSkillReadRequest, AgentPluginSkillSummary,
    AgentPluginSource, AgentPluginSummary, AgentPluginUninstallRequest, AgentPluginUninstallResult,
    AgentPluginsError, AgentPluginsErrorKind,
};
pub use requests::{
    AgentAdditionalFileSystemPermissions, AgentAdditionalNetworkPermissions, AgentApprovalHandle,
    AgentCommandApprovalChoice, AgentCommandApprovalKind, AgentCommandApprovalRequest,
    AgentFileApprovalChoice, AgentFileApprovalHandle, AgentFileApprovalRequest,
    AgentFileSystemAccess, AgentFileSystemPath, AgentFileSystemPermissionEntry,
    AgentFileSystemSpecialPath, AgentMcpElicitationAction, AgentMcpElicitationContent,
    AgentMcpElicitationField, AgentMcpElicitationFieldKind, AgentMcpElicitationFieldValue,
    AgentMcpElicitationForm, AgentMcpElicitationHandle, AgentMcpElicitationIdentity,
    AgentMcpElicitationMode, AgentMcpElicitationOption, AgentMcpElicitationRequest,
    AgentMcpElicitationResponse, AgentMcpElicitationStringFormat, AgentMcpElicitationUrl,
    AgentMcpElicitationValue, AgentNetworkApprovalContext, AgentNetworkApprovalProtocol,
    AgentNetworkPolicyAction, AgentNetworkPolicyAmendment, AgentOptionalField,
    AgentPermissionRequestProfile, AgentPermissionsApprovalChoice, AgentPermissionsApprovalHandle,
    AgentPermissionsApprovalRequest, AgentServerRequestFailureKind, AgentServerRequestId,
    AgentServerRequestKind, AgentServerRequestMetadata, AgentUserInputAnswer, AgentUserInputHandle,
    AgentUserInputOption, AgentUserInputQuestion, AgentUserInputRequest, AgentUserInputResponse,
};
pub(crate) use requests::{
    AgentApprovalControl, AgentFileApprovalControl, AgentMcpElicitationControl,
    AgentPermissionsApprovalControl, AgentUserInputControl,
};
pub use runtime::{
    AgentAuthRecovery, AgentDeprecationNotice, AgentHookOutput, AgentHookPrompt,
    AgentHookPromptFragment, AgentHookRun, AgentHookStatus, AgentLocalClosure, AgentRuntimeEvent,
    AgentRuntimeObservation, AgentRuntimeState, AgentScopedHookPrompt,
};
pub use skills::{
    AgentSkill, AgentSkillDependency, AgentSkillInterface, AgentSkillLoadError, AgentSkillScope,
    AgentSkillSelector, AgentSkillWriteReceipt, AgentSkillWriteRequest, AgentSkillsEntry,
    AgentSkillsError, AgentSkillsErrorKind, AgentSkillsLoadRequest, AgentSkillsSnapshot,
};
pub use status::{
    AgentConfigWarning, AgentMcpServerStartupFailureReason, AgentMcpServerStartupState,
    AgentMcpServerStartupStatus, AgentThreadActiveFlag, AgentThreadStatus, AgentThreadStatusState,
    AgentThreadTokenUsage, AgentTokenUsageBreakdown,
};
pub use thread::{
    AgentThreadRevert, AgentThreadRevertOutcome, CreateProject, FilterValue, HistoryItemDetail,
    HistoryTurnStatus, Page, PageRequest, Project, ProjectChange, ProjectId, SortDirection,
    ThreadActivity, ThreadHistory, ThreadHistoryItem, ThreadHistoryItemEntry, ThreadId,
    ThreadListRequest, ThreadMetadataUpdate, ThreadSearchResult, ThreadSection,
    ThreadSectionAppearance, ThreadSectionId, ThreadSortKey, ThreadSummary, ThreadTurn,
    UpdateProject, UserMessageAttachment,
};
