//! `thread/searchOccurrences` for the baseline CLI schema (experimental).

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};

use super::json::{array, object, optional_string, required_string};
use crate::agent::{AgentThreadOccurrence, AgentThreadOccurrenceRequest, utf16_range_to_bytes};

pub(super) const SEARCH_OCCURRENCES_METHOD: &str = "thread/searchOccurrences";

pub(super) fn params(request: &AgentThreadOccurrenceRequest) -> Value {
    let mut params = Map::new();
    params.insert("threadId".into(), json!(request.thread_id));
    params.insert("searchTerm".into(), json!(request.search_term));
    if let Some(cursor) = &request.cursor {
        params.insert("cursor".into(), json!(cursor));
    }
    params.insert("limit".into(), json!(request.limit));
    Value::Object(params)
}

fn unit_offset(range: &Map<String, Value>, field: &str) -> Result<usize> {
    let value = range
        .get(field)
        .and_then(Value::as_u64)
        .with_context(|| format!("snippetMatchRange 缺少非负整数字段 {field}"))?;
    usize::try_from(value).context("snippetMatchRange 偏移超出范围")
}

fn parse_occurrence(value: &Value) -> Result<AgentThreadOccurrence> {
    const CONTEXT: &str = "thread/searchOccurrences occurrence";
    let occurrence = object(value, CONTEXT)?;
    let snippet = required_string(occurrence, "snippet", CONTEXT)?;
    let range = object(
        occurrence
            .get("snippetMatchRange")
            .context("occurrence 缺少 snippetMatchRange")?,
        "snippetMatchRange",
    )?;
    let (start, end) = (unit_offset(range, "start")?, unit_offset(range, "end")?);
    let Some(snippet_match) = utf16_range_to_bytes(&snippet, start, end) else {
        bail!("snippetMatchRange {start}..{end} 不是 snippet 内的完整 UTF-16 范围");
    };
    if snippet_match.is_empty() {
        bail!("snippetMatchRange 为空");
    }
    Ok(AgentThreadOccurrence {
        item_id: required_string(occurrence, "itemId", CONTEXT)?,
        turn_id: required_string(occurrence, "turnId", CONTEXT)?,
        turn_cursor: required_string(occurrence, "turnCursor", CONTEXT)?,
        snippet,
        snippet_match,
    })
}

pub(super) fn parse_page(response: &Value) -> Result<(Vec<AgentThreadOccurrence>, Option<String>)> {
    let result = object(
        response
            .get("result")
            .context("thread/searchOccurrences 响应缺少 result")?,
        "thread/searchOccurrences result",
    )?;
    let occurrences = array(
        result
            .get("data")
            .context("thread/searchOccurrences 响应缺少 data")?,
        "data",
    )?
    .iter()
    .map(parse_occurrence)
    .collect::<Result<Vec<_>>>()?;
    let next = optional_string(result, "nextCursor", "thread/searchOccurrences result")?;
    Ok((occurrences, next))
}

#[cfg(test)]
mod tests;
