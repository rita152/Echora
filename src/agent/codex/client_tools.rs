//! Client-executed dynamic tool calls (`item/tool/call`).
//!
//! The app-server asks the client to run a tool it owns; the client answers
//! with a schema-legal `{success, contentItems}` result and keeps the
//! connection. A tool this client cannot honestly run answers `success=false`
//! with no content items instead of a fabricated success or a JSON-RPC error.

use anyhow::{Context as _, Result};
use serde_json::{Value, json};

use super::requests::{optional_request_string, request_id_from_value, required_request_string};
use crate::agent::{AgentDynamicToolCallContentItem, AgentServerRequestId};

pub(super) const TOOL_CALL_METHOD: &str = "item/tool/call";

/// The reference client routes an absent namespace and the `codex_app`
/// namespace to the same client tool surface, so the registry keys both.
const REFERENCE_TOOL_NAMESPACES: &[Option<&str>] = &[None, Some("codex_app")];

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DynamicToolCallRequest {
    pub(super) request_id: AgentServerRequestId,
    pub(super) thread_id: String,
    pub(super) turn_id: String,
    pub(super) call_id: String,
    pub(super) tool: String,
    pub(super) namespace: Option<String>,
    /// `arguments` is `true` in the schema, so any JSON value is legal,
    /// including an explicit `null`. Only a missing key is a violation, matching
    /// the `dynamicToolCall` item decoder.
    pub(super) arguments: Value,
}

pub(super) fn parse_dynamic_tool_call_request(message: &Value) -> Result<DynamicToolCallRequest> {
    const METHOD: &str = TOOL_CALL_METHOD;
    let request_id = request_id_from_value(
        message
            .get("id")
            .context("item/tool/call 缺少 JSON-RPC id")?,
    )?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("item/tool/call 缺少对象 params")?;
    Ok(DynamicToolCallRequest {
        request_id,
        thread_id: required_request_string(params, METHOD, "threadId")?,
        turn_id: required_request_string(params, METHOD, "turnId")?,
        call_id: required_request_string(params, METHOD, "callId")?,
        tool: required_request_string(params, METHOD, "tool")?,
        namespace: optional_request_string(params, METHOD, "namespace")?,
        arguments: params
            .get("arguments")
            .cloned()
            .context("item/tool/call params.arguments 缺失")?,
    })
}

/// Wire shape of one content item. The result uses camelCase type names
/// (`inputText`/`inputImage`/`inputAudio`), unlike the snake_case
/// `functionCallOutput` content items, and shares the item-side representation.
fn content_item_value(item: &AgentDynamicToolCallContentItem) -> Value {
    match item {
        AgentDynamicToolCallContentItem::Text { text } => {
            json!({ "type": "inputText", "text": text })
        }
        AgentDynamicToolCallContentItem::Image { image_url } => {
            json!({ "type": "inputImage", "imageUrl": image_url })
        }
        AgentDynamicToolCallContentItem::Audio { audio_url } => {
            json!({ "type": "inputAudio", "audioUrl": audio_url })
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DynamicToolCallResult {
    pub(super) success: bool,
    pub(super) content_items: Vec<AgentDynamicToolCallContentItem>,
}

impl DynamicToolCallResult {
    /// The honest failure result: schema-legal, empty, and never a claimed
    /// success. The reason for the failure is recorded as a connection-level
    /// diagnostic instead of being invented as content the client does not own.
    pub(super) fn failed() -> Self {
        Self {
            success: false,
            content_items: Vec::new(),
        }
    }

    pub(super) fn succeeded(content_items: Vec<AgentDynamicToolCallContentItem>) -> Self {
        Self {
            success: true,
            content_items,
        }
    }

    pub(super) fn to_value(&self) -> Value {
        json!({
            "success": self.success,
            "contentItems": self
                .content_items
                .iter()
                .map(content_item_value)
                .collect::<Vec<_>>(),
        })
    }
}

/// Why a known client tool cannot run on this client. These are capability
/// boundaries, not protocol errors: the model still receives a valid result and
/// keeps its turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClientToolUnavailable {
    /// The reference client answers this tool from the Node.js/Python document
    /// runtime it installs into its own resources. Echora is a native Rust
    /// application and does not bundle that runtime.
    DocumentRuntimeMissing,
    /// Automations belong to the reference app's scheduler and store. Echora has
    /// no automation product surface, so it cannot create, update, or delete one.
    AutomationStoreMissing,
}

impl ClientToolUnavailable {
    pub(super) fn reason(self) -> &'static str {
        match self {
            Self::DocumentRuntimeMissing => {
                "本客户端为原生 Rust 应用，未捆绑参考客户端安装的 Node.js／Python 文档运行时"
            }
            Self::AutomationStoreMissing => {
                "本客户端没有 automation 存储与调度器，无法执行 automation_update"
            }
        }
    }
}

/// Result of one tool handler. `Err` is the honest "this client cannot run that
/// tool" outcome; a handler only returns `Ok` for capability the client owns.
pub(super) type ClientToolHandler =
    fn(
        &DynamicToolCallRequest,
    ) -> std::result::Result<Vec<AgentDynamicToolCallContentItem>, ClientToolUnavailable>;

#[derive(Clone, Copy)]
pub(super) struct ClientTool {
    pub(super) namespace: Option<&'static str>,
    pub(super) name: &'static str,
    execute: ClientToolHandler,
}

impl ClientTool {
    pub(super) fn new(
        namespace: Option<&'static str>,
        name: &'static str,
        execute: ClientToolHandler,
    ) -> Self {
        Self {
            namespace,
            name,
            execute,
        }
    }

    fn matches(&self, call: &DynamicToolCallRequest) -> bool {
        self.name == call.tool && self.namespace == call.namespace.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ClientToolExecution {
    /// The client ran the tool and produced real content items.
    Succeeded(DynamicToolCallResult),
    /// A tool this client knows but cannot execute on this host.
    Unavailable { reason: ClientToolUnavailable },
    /// No tool is registered for this namespace and tool name.
    Unknown,
}

impl ClientToolExecution {
    pub(super) fn result(&self) -> DynamicToolCallResult {
        match self {
            Self::Succeeded(result) => result.clone(),
            Self::Unavailable { .. } | Self::Unknown => DynamicToolCallResult::failed(),
        }
    }
}

/// Client tool registry keyed by `namespace + tool`. Entries are looked up with
/// the exact namespace the request carried, so an unknown namespace stays
/// unknown instead of silently matching an unnamespaced tool.
#[derive(Clone, Default)]
pub(super) struct ClientToolRegistry {
    tools: Vec<ClientTool>,
}

impl ClientToolRegistry {
    pub(super) fn builtin() -> Self {
        Self::from_tools(
            REFERENCE_TOOL_NAMESPACES
                .iter()
                .flat_map(|namespace| {
                    [
                        ClientTool::new(
                            *namespace,
                            "load_workspace_dependencies",
                            execute_load_workspace_dependencies,
                        ),
                        ClientTool::new(*namespace, "automation_update", execute_automation_update),
                    ]
                })
                .collect(),
        )
    }

    pub(super) fn from_tools(tools: Vec<ClientTool>) -> Self {
        Self { tools }
    }

    pub(super) fn execute(&self, call: &DynamicToolCallRequest) -> ClientToolExecution {
        let Some(tool) = self.tools.iter().find(|tool| tool.matches(call)) else {
            return ClientToolExecution::Unknown;
        };
        match (tool.execute)(call) {
            Ok(content_items) => {
                ClientToolExecution::Succeeded(DynamicToolCallResult::succeeded(content_items))
            }
            Err(reason) => ClientToolExecution::Unavailable { reason },
        }
    }
}

/// The reference client resolves this tool from the bundled document runtime it
/// installs for sheets, slides, PDFs and documents. Echora has no such runtime,
/// so it answers honestly instead of returning paths it does not own.
fn execute_load_workspace_dependencies(
    _call: &DynamicToolCallRequest,
) -> std::result::Result<Vec<AgentDynamicToolCallContentItem>, ClientToolUnavailable> {
    Err(ClientToolUnavailable::DocumentRuntimeMissing)
}

/// Automations live in the reference app's `CODEX_HOME/automations` store and
/// are replayed by its scheduler. Echora does not maintain that product, so
/// creating, updating, or deleting an automation here would be a claim the
/// application cannot keep.
fn execute_automation_update(
    _call: &DynamicToolCallRequest,
) -> std::result::Result<Vec<AgentDynamicToolCallContentItem>, ClientToolUnavailable> {
    Err(ClientToolUnavailable::AutomationStoreMissing)
}
