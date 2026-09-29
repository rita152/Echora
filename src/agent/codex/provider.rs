//! `modelProvider/capabilities/read` for the baseline CLI schema. The answer
//! follows the configured default provider, so it is read per generation and
//! again after a user config reload.

use anyhow::{Context as _, Result};
use serde_json::{Value, json};

use super::json::{object, required_bool};
use crate::agent::AgentProviderCapabilities;

pub(super) const PROVIDER_CAPABILITIES_METHOD: &str = "modelProvider/capabilities/read";

/// The params object is required even though it has no fields; a null or
/// missing `params` is rejected as an invalid request.
pub(super) fn capabilities_params() -> Value {
    json!({})
}

pub(super) fn parse_capabilities(
    generation: u64,
    response: &Value,
) -> Result<AgentProviderCapabilities> {
    const CONTEXT: &str = "modelProvider/capabilities/read result";
    let result = object(
        response
            .get("result")
            .context("modelProvider/capabilities/read 响应缺少 result")?,
        CONTEXT,
    )?;
    Ok(AgentProviderCapabilities {
        generation,
        image_generation: required_bool(result, "imageGeneration", CONTEXT)?,
        web_search: required_bool(result, "webSearch", CONTEXT)?,
        namespace_tools: required_bool(result, "namespaceTools", CONTEXT)?,
    })
}
