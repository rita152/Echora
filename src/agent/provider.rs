//! What the configured model provider supports, independent of the model.

/// `modelProvider/capabilities/read` for one generation. Entry points for a
/// capability the provider lacks are gated in the UI; the adapter only
/// reports what the server answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentProviderCapabilities {
    pub generation: u64,
    pub image_generation: bool,
    pub web_search: bool,
    pub namespace_tools: bool,
}
