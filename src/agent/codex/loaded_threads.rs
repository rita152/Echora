//! `thread/loaded/list` for the baseline CLI schema: the ids of the threads the
//! server process currently holds in memory, one opaque-cursor page at a time.

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};

use super::json::{array, object, optional_string};

pub(super) const LOADED_LIST_METHOD: &str = "thread/loaded/list";
/// Stops a server that keeps answering with new cursors from looping forever.
pub(super) const MAX_LOADED_PAGES: usize = 64;

pub(super) fn list_params(cursor: Option<&str>) -> Value {
    let mut params = Map::new();
    if let Some(cursor) = cursor {
        params.insert("cursor".into(), json!(cursor));
    }
    Value::Object(params)
}

/// One page of thread ids and its continuation cursor.
pub(super) fn parse_page(response: &Value) -> Result<(Vec<String>, Option<String>)> {
    const CONTEXT: &str = "thread/loaded/list result";
    let result = object(
        response
            .get("result")
            .context("thread/loaded/list 响应缺少 result")?,
        CONTEXT,
    )?;
    let ids = array(
        result
            .get("data")
            .context("thread/loaded/list 响应缺少 data")?,
        "thread/loaded/list data",
    )?
    .iter()
    .map(|id| {
        id.as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .context("thread/loaded/list data 只能包含非空字符串")
    })
    .collect::<Result<Vec<_>>>()?;
    let next = optional_string(result, "nextCursor", CONTEXT)?;
    if next.is_some() && ids.is_empty() {
        bail!("thread/loaded/list 返回了空页却仍有 nextCursor");
    }
    Ok((ids, next))
}
