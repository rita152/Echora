//! Interactive server requests and typed response handles.

use std::{fmt, sync::Arc};

use serde_json::Number;

/// JSON-RPC request ids are deliberately not normalized: a numeric `7` and a
/// string `"7"` identify different server requests and must be echoed with
/// their original type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentServerRequestId {
    Number(i64),
    String(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentServerRequestKind {
    CommandApproval,
    FileApproval,
    UserInput,
    PermissionsApproval,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentServerRequestMetadata {
    pub request_id: AgentServerRequestId,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub kind: AgentServerRequestKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AgentOptionalField<T> {
    #[default]
    Unspecified,
    Null,
    Value(T),
}

impl AgentServerRequestId {
    pub fn ui_key(&self) -> String {
        match self {
            Self::Number(id) => format!("number:{id}"),
            Self::String(id) => format!("string:{id}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCommandApprovalRequest {
    pub request_id: AgentServerRequestId,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub approval_id: Option<String>,
    pub kind: AgentCommandApprovalKind,
    pub environment_id: Option<String>,
    pub started_at_ms: i64,
    pub cwd: Option<String>,
    pub command: String,
    pub reason: Option<String>,
    pub network: Option<AgentNetworkApprovalContext>,
    pub additional_permissions: AgentOptionalField<AgentPermissionRequestProfile>,
    /// Ordered, validated choices. A missing/null wire list uses the protocol's
    /// legacy choices, with amendments only when the server proposes them.
    pub available_decisions: Vec<AgentCommandApprovalChoice>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCommandApprovalKind {
    Command,
    WriteStdin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentNetworkApprovalContext {
    pub host: String,
    pub protocol: AgentNetworkApprovalProtocol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentNetworkApprovalProtocol {
    Http,
    Https,
    Socks5Tcp,
    Socks5Udp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentNetworkPolicyAction {
    Allow,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentNetworkPolicyAmendment {
    pub host: String,
    pub action: AgentNetworkPolicyAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentCommandApprovalChoice {
    Accept,
    AcceptForSession,
    /// Reject this command item while allowing the active turn to continue.
    Decline,
    /// Reject the command and interrupt its turn. Never substitute `decline`.
    Cancel,
    AcceptWithExecpolicyAmendment(Vec<String>),
    ApplyNetworkPolicyAmendment(AgentNetworkPolicyAmendment),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFileApprovalRequest {
    pub request_id: AgentServerRequestId,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub started_at_ms: i64,
    pub reason: Option<String>,
    pub grant_root: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentFileApprovalChoice {
    Accept,
    AcceptForSession,
    Decline,
    Cancel,
}

pub(crate) trait AgentFileApprovalControl: Send + Sync {
    fn respond(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentFileApprovalChoice,
    ) -> Result<(), String>;
}

#[derive(Clone)]
pub struct AgentFileApprovalHandle {
    request_id: AgentServerRequestId,
    control: Arc<dyn AgentFileApprovalControl>,
}

impl AgentFileApprovalHandle {
    pub(crate) fn new(
        request_id: AgentServerRequestId,
        control: Arc<dyn AgentFileApprovalControl>,
    ) -> Self {
        Self {
            request_id,
            control,
        }
    }

    pub fn respond(&self, choice: AgentFileApprovalChoice) -> Result<(), String> {
        self.control.respond(&self.request_id, choice)
    }
}

impl fmt::Debug for AgentFileApprovalHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentFileApprovalHandle")
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AgentFileApprovalHandle {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id && Arc::ptr_eq(&self.control, &other.control)
    }
}

impl Eq for AgentFileApprovalHandle {}

pub(crate) trait AgentApprovalControl: Send + Sync {
    fn respond(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentCommandApprovalChoice,
    ) -> Result<(), String>;
}

#[derive(Clone)]
pub struct AgentApprovalHandle {
    request_id: AgentServerRequestId,
    control: Arc<dyn AgentApprovalControl>,
}

impl AgentApprovalHandle {
    pub(crate) fn new(
        request_id: AgentServerRequestId,
        control: Arc<dyn AgentApprovalControl>,
    ) -> Self {
        Self {
            request_id,
            control,
        }
    }

    pub fn respond(&self, choice: AgentCommandApprovalChoice) -> Result<(), String> {
        self.control.respond(&self.request_id, choice)
    }
}

impl fmt::Debug for AgentApprovalHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentApprovalHandle")
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AgentApprovalHandle {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id && Arc::ptr_eq(&self.control, &other.control)
    }
}

impl Eq for AgentApprovalHandle {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentUserInputOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentUserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<AgentUserInputOption>,
    pub allows_other: bool,
    pub is_secret: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentUserInputRequest {
    pub request_id: AgentServerRequestId,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub questions: Vec<AgentUserInputQuestion>,
    pub is_blocking: bool,
    pub auto_resolution_ms: Option<u64>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AgentUserInputAnswer {
    pub question_id: String,
    pub answers: Vec<String>,
}

impl fmt::Debug for AgentUserInputAnswer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentUserInputAnswer")
            .field("question_id", &self.question_id)
            .field("answer_count", &self.answers.len())
            .field("answers", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct AgentUserInputResponse {
    pub answers: Vec<AgentUserInputAnswer>,
}

impl fmt::Debug for AgentUserInputResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentUserInputResponse")
            .field("answers", &self.answers)
            .finish()
    }
}

pub(crate) trait AgentUserInputControl: Send + Sync {
    fn respond(
        &self,
        request_id: &AgentServerRequestId,
        response: AgentUserInputResponse,
    ) -> Result<(), String>;
}

#[derive(Clone)]
pub struct AgentUserInputHandle {
    request_id: AgentServerRequestId,
    control: Arc<dyn AgentUserInputControl>,
}

impl AgentUserInputHandle {
    pub(crate) fn new(
        request_id: AgentServerRequestId,
        control: Arc<dyn AgentUserInputControl>,
    ) -> Self {
        Self {
            request_id,
            control,
        }
    }

    pub fn respond(&self, response: AgentUserInputResponse) -> Result<(), String> {
        self.control.respond(&self.request_id, response)
    }
}

impl fmt::Debug for AgentUserInputHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentUserInputHandle")
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AgentUserInputHandle {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id && Arc::ptr_eq(&self.control, &other.control)
    }
}

impl Eq for AgentUserInputHandle {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentFileSystemAccess {
    Read,
    Write,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentFileSystemSpecialPath {
    Root,
    Minimal,
    ProjectRoots {
        subpath: AgentOptionalField<String>,
    },
    Tmpdir,
    SlashTmp,
    Unknown {
        path: String,
        subpath: AgentOptionalField<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentFileSystemPath {
    Path(String),
    GlobPattern(String),
    Special(AgentFileSystemSpecialPath),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFileSystemPermissionEntry {
    pub path: AgentFileSystemPath,
    pub access: AgentFileSystemAccess,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAdditionalFileSystemPermissions {
    pub read: AgentOptionalField<Vec<String>>,
    pub write: AgentOptionalField<Vec<String>>,
    pub glob_scan_max_depth: AgentOptionalField<u64>,
    pub entries: AgentOptionalField<Vec<AgentFileSystemPermissionEntry>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAdditionalNetworkPermissions {
    pub enabled: AgentOptionalField<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentPermissionRequestProfile {
    pub file_system: AgentOptionalField<AgentAdditionalFileSystemPermissions>,
    pub network: AgentOptionalField<AgentAdditionalNetworkPermissions>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPermissionsApprovalRequest {
    pub request_id: AgentServerRequestId,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub environment_id: Option<String>,
    pub started_at_ms: i64,
    pub cwd: String,
    pub reason: Option<String>,
    pub permissions: AgentPermissionRequestProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPermissionsApprovalChoice {
    AllowOnce,
    AllowForSession,
    Decline,
}

pub(crate) trait AgentPermissionsApprovalControl: Send + Sync {
    fn respond(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentPermissionsApprovalChoice,
    ) -> Result<(), String>;
}

#[derive(Clone)]
pub struct AgentPermissionsApprovalHandle {
    request_id: AgentServerRequestId,
    control: Arc<dyn AgentPermissionsApprovalControl>,
}

impl AgentPermissionsApprovalHandle {
    pub(crate) fn new(
        request_id: AgentServerRequestId,
        control: Arc<dyn AgentPermissionsApprovalControl>,
    ) -> Self {
        Self {
            request_id,
            control,
        }
    }

    pub fn respond(&self, choice: AgentPermissionsApprovalChoice) -> Result<(), String> {
        self.control.respond(&self.request_id, choice)
    }
}

impl fmt::Debug for AgentPermissionsApprovalHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentPermissionsApprovalHandle")
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AgentPermissionsApprovalHandle {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id && Arc::ptr_eq(&self.control, &other.control)
    }
}

impl Eq for AgentPermissionsApprovalHandle {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentServerRequestFailureKind {
    Cancelled,
    Failed,
}

/// Identity of one MCP elicitation. A connection generation owns its own
/// requests, so the original JSON-RPC id alone is never a global key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AgentMcpElicitationIdentity {
    pub generation: u64,
    pub request_id: AgentServerRequestId,
}

impl AgentMcpElicitationIdentity {
    pub fn ui_key(&self) -> String {
        format!(
            "generation:{}-{}",
            self.generation,
            self.request_id.ui_key()
        )
    }
}

/// Standard MCP requestedSchema primitives supported by the first version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpElicitationStringFormat {
    Email,
    Uri,
    Date,
    DateTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpElicitationOption {
    /// Wire value submitted for this option.
    pub value: String,
    /// Display title; defaults to the wire value when the schema has none.
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentMcpElicitationFieldKind {
    String {
        format: Option<AgentMcpElicitationStringFormat>,
        min_length: Option<u64>,
        max_length: Option<u64>,
    },
    Number {
        integer: bool,
        minimum: Option<Number>,
        maximum: Option<Number>,
    },
    Boolean,
    SingleSelect {
        options: Vec<AgentMcpElicitationOption>,
    },
    MultiSelect {
        options: Vec<AgentMcpElicitationOption>,
        min_items: Option<u64>,
        max_items: Option<u64>,
    },
}

/// One submitted value. Numbers keep their original JSON representation so an
/// integer schema never receives a lossy float.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentMcpElicitationValue {
    String(String),
    Number(Number),
    Boolean(bool),
    StringArray(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpElicitationField {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub required: bool,
    pub kind: AgentMcpElicitationFieldKind,
    pub default: Option<AgentMcpElicitationValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpElicitationForm {
    pub message: String,
    pub fields: Vec<AgentMcpElicitationField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpElicitationUrl {
    pub elicitation_id: String,
    pub message: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentMcpElicitationMode {
    Form(AgentMcpElicitationForm),
    Url(AgentMcpElicitationUrl),
}

/// A standalone server-to-client MCP elicitation. It is not a turn-scoped
/// approval: turn_id may be missing or null, and the request must stay
/// answerable without an active turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpElicitationRequest {
    pub generation: u64,
    pub request_id: AgentServerRequestId,
    pub server_name: String,
    pub thread_id: String,
    pub turn_id: AgentOptionalField<String>,
    pub mode: AgentMcpElicitationMode,
}

impl AgentMcpElicitationRequest {
    pub fn identity(&self) -> AgentMcpElicitationIdentity {
        AgentMcpElicitationIdentity {
            generation: self.generation,
            request_id: self.request_id.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpElicitationAction {
    Accept,
    Decline,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpElicitationFieldValue {
    pub name: String,
    pub value: AgentMcpElicitationValue,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct AgentMcpElicitationContent {
    pub fields: Vec<AgentMcpElicitationFieldValue>,
}

/// Response payload mirroring the MCP CreateElicitationResult shape.
/// content is only ever present for accept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpElicitationResponse {
    pub action: AgentMcpElicitationAction,
    pub content: Option<AgentMcpElicitationContent>,
}

impl AgentMcpElicitationResponse {
    pub fn accept(content: AgentMcpElicitationContent) -> Self {
        Self {
            action: AgentMcpElicitationAction::Accept,
            content: Some(content),
        }
    }

    pub fn decline() -> Self {
        Self {
            action: AgentMcpElicitationAction::Decline,
            content: None,
        }
    }

    pub fn cancel() -> Self {
        Self {
            action: AgentMcpElicitationAction::Cancel,
            content: None,
        }
    }
}

pub(crate) trait AgentMcpElicitationControl: Send + Sync {
    fn respond(
        &self,
        identity: &AgentMcpElicitationIdentity,
        response: AgentMcpElicitationResponse,
    ) -> Result<(), String>;
}

#[derive(Clone)]
pub struct AgentMcpElicitationHandle {
    identity: AgentMcpElicitationIdentity,
    control: Arc<dyn AgentMcpElicitationControl>,
}

impl AgentMcpElicitationHandle {
    pub(crate) fn new(
        identity: AgentMcpElicitationIdentity,
        control: Arc<dyn AgentMcpElicitationControl>,
    ) -> Self {
        Self { identity, control }
    }

    pub fn respond(&self, response: AgentMcpElicitationResponse) -> Result<(), String> {
        self.control.respond(&self.identity, response)
    }
}

impl fmt::Debug for AgentMcpElicitationHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentMcpElicitationHandle")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AgentMcpElicitationHandle {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity && Arc::ptr_eq(&self.control, &other.control)
    }
}

impl Eq for AgentMcpElicitationHandle {}
