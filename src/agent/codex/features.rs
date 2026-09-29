//! `experimentalFeature/list`, `thread/memoryMode/set` and `memory/reset` for
//! the baseline CLI schema.

use anyhow::{Context as _, Result};
use serde_json::{Map, Value, json};

use super::json::{array, object, optional_string, required_bool, required_enum, required_string};
use crate::agent::{
    AgentExperimentalFeature, AgentExperimentalFeatureStage, AgentThreadMemoryMode,
};

pub(super) const FEATURE_LIST_METHOD: &str = "experimentalFeature/list";
pub(super) const MEMORY_MODE_SET_METHOD: &str = "thread/memoryMode/set";
pub(super) const MEMORY_RESET_METHOD: &str = "memory/reset";
/// The reference's page size.
pub(super) const FEATURE_PAGE_SIZE: u32 = 100;

pub(super) fn list_params(cursor: Option<&str>, thread_id: Option<&str>) -> Value {
    let mut params = Map::new();
    params.insert(
        "cursor".into(),
        cursor.map_or(Value::Null, |cursor| json!(cursor)),
    );
    params.insert("limit".into(), json!(FEATURE_PAGE_SIZE));
    if let Some(thread_id) = thread_id {
        params.insert("threadId".into(), json!(thread_id));
    }
    Value::Object(params)
}

fn parse_stage(raw: &str) -> Option<AgentExperimentalFeatureStage> {
    Some(match raw {
        "beta" => AgentExperimentalFeatureStage::Beta,
        "underDevelopment" => AgentExperimentalFeatureStage::UnderDevelopment,
        "stable" => AgentExperimentalFeatureStage::Stable,
        "deprecated" => AgentExperimentalFeatureStage::Deprecated,
        "removed" => AgentExperimentalFeatureStage::Removed,
        _ => return None,
    })
}

fn parse_feature(value: &Value) -> Result<AgentExperimentalFeature> {
    const CONTEXT: &str = "experimentalFeature";
    let feature = object(value, CONTEXT)?;
    let name = required_string(feature, "name", CONTEXT)?;
    // The name becomes a config key path segment.
    if name.is_empty()
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        anyhow::bail!("{CONTEXT} 的 name `{name}` 不是合法的配置键");
    }
    Ok(AgentExperimentalFeature {
        name,
        stage: required_enum(feature, "stage", CONTEXT, parse_stage)?,
        display_name: optional_string(feature, "displayName", CONTEXT)?,
        description: optional_string(feature, "description", CONTEXT)?,
        announcement: optional_string(feature, "announcement", CONTEXT)?,
        enabled: required_bool(feature, "enabled", CONTEXT)?,
        default_enabled: required_bool(feature, "defaultEnabled", CONTEXT)?,
    })
}

/// One page and its continuation cursor.
pub(super) fn parse_list_page(
    response: &Value,
) -> Result<(Vec<AgentExperimentalFeature>, Option<String>)> {
    let result = object(
        response
            .get("result")
            .context("experimentalFeature/list 响应缺少 result")?,
        "experimentalFeature/list result",
    )?;
    let features = array(
        result
            .get("data")
            .context("experimentalFeature/list 响应缺少 data")?,
        "data",
    )?
    .iter()
    .map(parse_feature)
    .collect::<Result<Vec<_>>>()?;
    let next = optional_string(result, "nextCursor", "experimentalFeature/list result")?;
    Ok((features, next))
}

pub(super) fn memory_mode_params(thread_id: &str, mode: AgentThreadMemoryMode) -> Value {
    json!({
        "threadId": thread_id,
        "mode": match mode {
            AgentThreadMemoryMode::Enabled => "enabled",
            AgentThreadMemoryMode::Disabled => "disabled",
        },
    })
}

/// Both `thread/memoryMode/set` and `memory/reset` answer an empty object.
pub(super) fn parse_empty_result(response: &Value, method: &str) -> Result<()> {
    object(
        response
            .get("result")
            .with_context(|| format!("{method} 响应缺少 result"))?,
        method,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests;
