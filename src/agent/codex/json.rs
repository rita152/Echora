//! Shared decoding helpers for the management codecs. Every helper keeps a
//! missing field apart from an explicit null, and every failure carries the
//! method and field that produced it.

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(super) fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .with_context(|| format!("{context} 必须是 JSON 对象"))
}

pub(super) fn params<'a>(message: &'a Value, method: &str) -> Result<&'a Map<String, Value>> {
    object(
        message
            .get("params")
            .with_context(|| format!("{method} 缺少 params"))?,
        &format!("{method} 的 params"),
    )
}

pub(super) fn array<'a>(value: &'a Value, context: &str) -> Result<&'a Vec<Value>> {
    value
        .as_array()
        .with_context(|| format!("{context} 必须是数组"))
}

/// A list field the schema declares required. A missing or null field decodes
/// to an empty list, which is how the server describes "nothing here"; the
/// caller never substitutes entries of its own.
pub(super) fn array_or_empty<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<&'a [Value]> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(&[]),
        Some(value) => Ok(array(value, context)?.as_slice()),
    }
}

/// A field that the schema marks required.
pub(super) fn required_string(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("{context} 缺少字符串字段 {field}"))
}

pub(super) fn required_bool(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<bool> {
    object
        .get(field)
        .and_then(Value::as_bool)
        .with_context(|| format!("{context} 缺少布尔字段 {field}"))
}

/// An optional field where a missing key and an explicit null are the same
/// absence, as the schema's `type: [X, null]` declares.
pub(super) fn optional_string(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<String>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("{context} 的 {field} 必须是字符串或 null"),
    }
}

pub(super) fn optional_bool(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<bool>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => bail!("{context} 的 {field} 必须是布尔值或 null"),
    }
}

pub(super) fn optional_i64(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<i64>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .with_context(|| format!("{context} 的 {field} 必须是整数")),
        Some(_) => bail!("{context} 的 {field} 必须是整数或 null"),
    }
}

/// A defaulted field: absent means the schema default, an explicit null is a
/// protocol error because the schema declares a concrete type.
pub(super) fn defaulted_bool(
    object: &Map<String, Value>,
    field: &str,
    default: bool,
    context: &str,
) -> Result<bool> {
    match object.get(field) {
        None => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => bail!("{context} 的 {field} 必须是布尔值"),
    }
}

pub(super) fn defaulted_string_list(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Vec<String>> {
    match object.get(field) {
        None => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .with_context(|| format!("{context} 的 {field} 必须是字符串数组"))
            })
            .collect(),
        Some(_) => bail!("{context} 的 {field} 必须是字符串数组"),
    }
}

pub(super) fn optional_string_list(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<Vec<String>>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(_)) => defaulted_string_list(object, field, context).map(Some),
        Some(_) => bail!("{context} 的 {field} 必须是字符串数组或 null"),
    }
}

pub(super) fn optional_string_map(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<BTreeMap<String, String>>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(entries)) => {
            let mut parsed = BTreeMap::new();
            for (key, value) in entries {
                let value = value
                    .as_str()
                    .with_context(|| format!("{context} 的 {field}.{key} 必须是字符串"))?;
                parsed.insert(key.clone(), value.to_owned());
            }
            Ok(Some(parsed))
        }
        Some(_) => bail!("{context} 的 {field} 必须是字符串映射或 null"),
    }
}

pub(super) fn required_path(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<PathBuf> {
    let raw = required_string(object, field, context)?;
    let path = PathBuf::from(&raw);
    if !path.is_absolute() {
        bail!("{context} 的 {field} 必须是绝对路径：{raw}");
    }
    Ok(path)
}

pub(super) fn optional_path(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<PathBuf>> {
    Ok(optional_string(object, field, context)?.map(PathBuf::from))
}

pub(super) fn path_list(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Vec<PathBuf>> {
    Ok(defaulted_string_list(object, field, context)?
        .into_iter()
        .map(PathBuf::from)
        .collect())
}

pub(super) fn unknown_fields(
    object: &Map<String, Value>,
    known: &[&str],
) -> BTreeMap<String, Value> {
    object
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// An untyped field: None for a missing key or an explicit null, the raw
/// value otherwise. Used for schema unions the client stores verbatim.
pub(super) fn optional_value(object: &Map<String, Value>, field: &str) -> Option<Value> {
    match object.get(field) {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.clone()),
    }
}

/// Decodes a required enum field with the schema's own vocabulary.
pub(super) fn required_enum<T>(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<T> {
    let raw = required_string(object, field, context)?;
    parse(&raw).with_context(|| format!("{context} 的 {field} 为未知值 `{raw}`"))
}

pub(super) fn optional_enum<T>(
    object: &Map<String, Value>,
    field: &str,
    context: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Option<T>> {
    match optional_string(object, field, context)? {
        None => Ok(None),
        Some(raw) => parse(&raw)
            .map(Some)
            .with_context(|| format!("{context} 的 {field} 为未知值 `{raw}`")),
    }
}
