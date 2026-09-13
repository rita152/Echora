//! Strict decoding and encoding for mcpServer/elicitation/request.
//!
//! Field names, optionality, and variant membership follow the schema emitted
//! by codex-cli 0.153.0 (artifacts/app-server-schema/0.153.0/
//! McpServerElicitationRequestParams.json). Only the standard form and url
//! modes are supported: openai/form and openaiForm describe an opaque schema
//! this client cannot render, so they stay protocol errors instead of being
//! silently accepted.

use std::collections::HashSet;

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Map, Number, Value, json};

use super::requests::{parse_optional_field, request_id_from_value, required_request_string};
use crate::agent::{
    AgentMcpElicitationAction, AgentMcpElicitationField, AgentMcpElicitationFieldKind,
    AgentMcpElicitationFieldValue, AgentMcpElicitationForm, AgentMcpElicitationMode,
    AgentMcpElicitationOption, AgentMcpElicitationRequest, AgentMcpElicitationResponse,
    AgentMcpElicitationStringFormat, AgentMcpElicitationUrl, AgentMcpElicitationValue,
};

pub(super) const ELICITATION_METHOD: &str = "mcpServer/elicitation/request";

/// Modes whose wire shape this client implements end to end.
const SUPPORTED_MODES: &[&str] = &["form", "url"];

/// Declared by the experimental schema but intentionally not implemented.
const UNSUPPORTED_MODES: &[&str] = &["openai/form", "openaiForm", "openai/userVerification"];

fn ensure_known_keys(object: &Map<String, Value>, path: &str, allowed: &[&str]) -> Result<()> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            bail!("{path} 包含 schema 未定义字段 {key:?}");
        }
    }
    Ok(())
}

fn required_string(object: &Map<String, Value>, path: &str, field: &str) -> Result<String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("{path}.{field} 必须是字符串"))
}

fn optional_string(object: &Map<String, Value>, path: &str, field: &str) -> Result<Option<String>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("{path}.{field} 必须是字符串或 null"),
    }
}

fn optional_number(object: &Map<String, Value>, path: &str, field: &str) -> Result<Option<Number>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("{path}.{field} 必须是数字或 null"),
    }
}

fn optional_unsigned(object: &Map<String, Value>, path: &str, field: &str) -> Result<Option<u64>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => value
            .as_u64()
            .map(Some)
            .with_context(|| format!("{path}.{field} 必须是非负整数或 null")),
        Some(_) => bail!("{path}.{field} 必须是非负整数或 null"),
    }
}

pub(super) fn parse_mcp_server_elicitation_request(
    message: &Value,
    generation: u64,
) -> Result<AgentMcpElicitationRequest> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .filter(|method| *method == ELICITATION_METHOD)
        .with_context(|| format!("{ELICITATION_METHOD} 缺少匹配的 method"))?;
    let request_id = request_id_from_value(
        message
            .get("id")
            .context("mcpServer/elicitation/request 缺少 JSON-RPC id")?,
    )?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .with_context(|| format!("{method} 缺少对象 params"))?;
    ensure_known_keys(
        params,
        "mcpServer/elicitation/request params",
        &[
            "serverName",
            "threadId",
            "turnId",
            "mode",
            "message",
            "requestedSchema",
            "elicitationId",
            "url",
            "_meta",
        ],
    )?;
    let server_name = required_request_string(params, method, "serverName")?;
    let thread_id = required_request_string(params, method, "threadId")?;
    let turn_id = parse_optional_field(params, "turnId", |value| {
        value
            .as_str()
            .map(str::to_owned)
            .context("mcpServer/elicitation/request params.turnId 必须是字符串或 null")
    })?;
    let Some(mode) = params.get("mode").and_then(Value::as_str) else {
        bail!("{method} params.mode 必须是字符串");
    };
    if UNSUPPORTED_MODES.contains(&mode) {
        bail!(
            "mcpServer/elicitation/request mode {mode:?} 尚未接入：该模式需要完整 schema/UI 支持，当前按协议错误终止连接"
        );
    }
    if !SUPPORTED_MODES.contains(&mode) {
        bail!("mcpServer/elicitation/request params.mode 包含未知值 {mode:?}");
    }
    let mode = match mode {
        "form" => AgentMcpElicitationMode::Form(parse_form_mode(params)?),
        "url" => AgentMcpElicitationMode::Url(parse_url_mode(params)?),
        other => bail!("mcpServer/elicitation/request mode {other:?} 未接入"),
    };
    Ok(AgentMcpElicitationRequest {
        generation,
        request_id,
        server_name,
        thread_id,
        turn_id,
        mode,
    })
}

fn parse_form_mode(params: &Map<String, Value>) -> Result<AgentMcpElicitationForm> {
    const PATH: &str = "mcpServer/elicitation/request form params";
    // The schema flattens the shared serverName/threadId/turnId keys and the
    // mode-specific variant into one params object, so both key sets are legal
    // here; the top-level check above already rejected anything else.
    ensure_known_keys(
        params,
        PATH,
        &[
            "serverName",
            "threadId",
            "turnId",
            "mode",
            "message",
            "requestedSchema",
            "_meta",
        ],
    )?;
    let message = required_string(params, PATH, "message")?;
    let schema = params
        .get("requestedSchema")
        .and_then(Value::as_object)
        .context("mcpServer/elicitation/request params.requestedSchema 必须是对象")?;
    let fields = parse_requested_schema(schema)?;
    Ok(AgentMcpElicitationForm { message, fields })
}

fn parse_url_mode(params: &Map<String, Value>) -> Result<AgentMcpElicitationUrl> {
    const PATH: &str = "mcpServer/elicitation/request url params";
    ensure_known_keys(
        params,
        PATH,
        &[
            "serverName",
            "threadId",
            "turnId",
            "mode",
            "elicitationId",
            "message",
            "url",
            "_meta",
        ],
    )?;
    let elicitation_id = required_string(params, PATH, "elicitationId")?;
    let message = required_string(params, PATH, "message")?;
    let url = required_string(params, PATH, "url")?;
    let parsed =
        url::Url::parse(&url).with_context(|| format!("{PATH}.url 不是合法 URL：{url}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("{PATH}.url 只支持 http/https，收到 {}", parsed.scheme());
    }
    if parsed.host_str().is_none() {
        bail!("{PATH}.url 缺少主机名：{url}");
    }
    Ok(AgentMcpElicitationUrl {
        elicitation_id,
        message,
        url,
    })
}

/// Parse the standard MCP requestedSchema object.
fn parse_requested_schema(schema: &Map<String, Value>) -> Result<Vec<AgentMcpElicitationField>> {
    const PATH: &str = "mcpServer/elicitation/request params.requestedSchema";
    ensure_known_keys(schema, PATH, &["$schema", "type", "properties", "required"])?;
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => {}
        Some(other) => bail!("{PATH}.type 必须是 object，收到 {other:?}"),
        None => bail!("{PATH}.type 必须是字符串 object"),
    }
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .with_context(|| format!("{PATH}.properties 必须是对象"))?;
    let required = match schema.get("required") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .with_context(|| format!("{PATH}.required[{index}] 必须是字符串"))
            })
            .collect::<Result<Vec<_>>>()?,
        Some(_) => bail!("{PATH}.required 必须是字符串数组或 null"),
    };
    let mut required_names = HashSet::new();
    for name in &required {
        if !properties.contains_key(name) {
            bail!("{PATH}.required 引用了未定义的属性 {name:?}");
        }
        if !required_names.insert(name.clone()) {
            bail!("{PATH}.required 包含重复属性 {name:?}");
        }
    }
    let mut fields = Vec::with_capacity(properties.len());
    for (name, property) in properties {
        let path = format!("{PATH}.properties.{name}");
        let property = property
            .as_object()
            .with_context(|| format!("{path} 必须是对象"))?;
        let kind = parse_primitive_schema(property, &path)?;
        fields.push(AgentMcpElicitationField {
            name: name.clone(),
            required: required_names.contains(name),
            title: optional_string(property, &path, "title")?,
            description: optional_string(property, &path, "description")?,
            default: default_value(&kind, property, &path)?,
            kind,
        });
    }
    Ok(fields)
}

fn parse_primitive_schema(
    property: &Map<String, Value>,
    path: &str,
) -> Result<AgentMcpElicitationFieldKind> {
    match property.get("type").and_then(Value::as_str) {
        Some("string") => parse_string_schema(property, path),
        Some("number") | Some("integer") => {
            ensure_known_keys(
                property,
                path,
                &[
                    "type",
                    "title",
                    "description",
                    "default",
                    "minimum",
                    "maximum",
                ],
            )?;
            let minimum = optional_number(property, path, "minimum")?;
            let maximum = optional_number(property, path, "maximum")?;
            if let (Some(minimum), Some(maximum)) = (&minimum, &maximum)
                && minimum.as_f64() > maximum.as_f64()
            {
                bail!("{path}.minimum 不能大于 maximum");
            }
            Ok(AgentMcpElicitationFieldKind::Number {
                integer: property.get("type").and_then(Value::as_str) == Some("integer"),
                minimum,
                maximum,
            })
        }
        Some("boolean") => {
            ensure_known_keys(property, path, &["type", "title", "description", "default"])?;
            Ok(AgentMcpElicitationFieldKind::Boolean)
        }
        Some("array") => parse_array_schema(property, path),
        Some(other) => bail!("{path}.type 包含不支持的 MCP primitive {other:?}"),
        None => bail!("{path}.type 必须是字符串"),
    }
}

fn parse_string_schema(
    property: &Map<String, Value>,
    path: &str,
) -> Result<AgentMcpElicitationFieldKind> {
    let has_enum = property.contains_key("enum");
    let has_one_of = property.contains_key("oneOf");
    if has_enum && has_one_of {
        bail!("{path} 不能同时包含 enum 与 oneOf");
    }
    if has_enum {
        ensure_known_keys(
            property,
            path,
            &[
                "type",
                "title",
                "description",
                "default",
                "enum",
                "enumNames",
            ],
        )?;
        let values = string_array(property, path, "enum")?;
        if values.is_empty() {
            bail!("{path}.enum 不能为空");
        }
        let titles = match property.get("enumNames") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(names)) => names
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .with_context(|| format!("{path}.enumNames[{index}] 必须是字符串"))
                })
                .collect::<Result<Vec<_>>>()?,
            Some(_) => bail!("{path}.enumNames 必须是字符串数组或 null"),
        };
        if !titles.is_empty() && titles.len() != values.len() {
            bail!("{path}.enumNames 长度必须与 enum 一致");
        }
        let options = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| AgentMcpElicitationOption {
                title: titles.get(index).cloned().unwrap_or_else(|| value.clone()),
                value,
            })
            .collect::<Vec<_>>();
        ensure_unique_options(&options, path)?;
        return Ok(AgentMcpElicitationFieldKind::SingleSelect { options });
    }
    if has_one_of {
        ensure_known_keys(
            property,
            path,
            &["type", "title", "description", "default", "oneOf"],
        )?;
        let entries = property
            .get("oneOf")
            .and_then(Value::as_array)
            .with_context(|| format!("{path}.oneOf 必须是数组"))?;
        if entries.is_empty() {
            bail!("{path}.oneOf 不能为空");
        }
        let mut options = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let entry_path = format!("{path}.oneOf[{index}]");
            let entry = entry
                .as_object()
                .with_context(|| format!("{entry_path} 必须是对象"))?;
            ensure_known_keys(entry, &entry_path, &["const", "title"])?;
            options.push(AgentMcpElicitationOption {
                value: required_string(entry, &entry_path, "const")?,
                title: required_string(entry, &entry_path, "title")?,
            });
        }
        ensure_unique_options(&options, path)?;
        return Ok(AgentMcpElicitationFieldKind::SingleSelect { options });
    }
    ensure_known_keys(
        property,
        path,
        &[
            "type",
            "title",
            "description",
            "default",
            "format",
            "minLength",
            "maxLength",
        ],
    )?;
    let format = match property.get("format") {
        None | Some(Value::Null) => None,
        Some(Value::String(format)) => Some(match format.as_str() {
            "email" => AgentMcpElicitationStringFormat::Email,
            "uri" => AgentMcpElicitationStringFormat::Uri,
            "date" => AgentMcpElicitationStringFormat::Date,
            "date-time" => AgentMcpElicitationStringFormat::DateTime,
            other => bail!("{path}.format 包含未知值 {other:?}"),
        }),
        Some(_) => bail!("{path}.format 必须是字符串或 null"),
    };
    let min_length = optional_unsigned(property, path, "minLength")?;
    let max_length = optional_unsigned(property, path, "maxLength")?;
    if let (Some(min), Some(max)) = (min_length, max_length)
        && min > max
    {
        bail!("{path}.minLength 不能大于 maxLength");
    }
    Ok(AgentMcpElicitationFieldKind::String {
        format,
        min_length,
        max_length,
    })
}

fn parse_array_schema(
    property: &Map<String, Value>,
    path: &str,
) -> Result<AgentMcpElicitationFieldKind> {
    ensure_known_keys(
        property,
        path,
        &[
            "type",
            "title",
            "description",
            "default",
            "items",
            "minItems",
            "maxItems",
        ],
    )?;
    let items_path = format!("{path}.items");
    let items = property
        .get("items")
        .and_then(Value::as_object)
        .with_context(|| format!("{items_path} 必须是对象"))?;
    let options = if items.contains_key("enum") {
        ensure_known_keys(items, &items_path, &["type", "enum"])?;
        match items.get("type").and_then(Value::as_str) {
            Some("string") => {}
            Some(other) => bail!("{items_path}.type 必须是 string，收到 {other:?}"),
            None => bail!("{items_path}.type 必须是字符串"),
        }
        string_array(items, &items_path, "enum")?
            .into_iter()
            .map(|value| AgentMcpElicitationOption {
                title: value.clone(),
                value,
            })
            .collect::<Vec<_>>()
    } else if items.contains_key("anyOf") {
        ensure_known_keys(items, &items_path, &["anyOf"])?;
        let entries = items
            .get("anyOf")
            .and_then(Value::as_array)
            .with_context(|| format!("{items_path}.anyOf 必须是数组"))?;
        entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let entry_path = format!("{items_path}.anyOf[{index}]");
                let entry = entry
                    .as_object()
                    .with_context(|| format!("{entry_path} 必须是对象"))?;
                ensure_known_keys(entry, &entry_path, &["const", "title"])?;
                Ok(AgentMcpElicitationOption {
                    value: required_string(entry, &entry_path, "const")?,
                    title: required_string(entry, &entry_path, "title")?,
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        bail!("{items_path} 必须包含 enum 或 anyOf");
    };
    if options.is_empty() {
        bail!("{path}.items 不能为空");
    }
    ensure_unique_options(&options, path)?;
    let min_items = optional_unsigned(property, path, "minItems")?;
    let max_items = optional_unsigned(property, path, "maxItems")?;
    if let (Some(min), Some(max)) = (min_items, max_items)
        && min > max
    {
        bail!("{path}.minItems 不能大于 maxItems");
    }
    Ok(AgentMcpElicitationFieldKind::MultiSelect {
        options,
        min_items,
        max_items,
    })
}

fn ensure_unique_options(options: &[AgentMcpElicitationOption], path: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for option in options {
        if !seen.insert(option.value.as_str()) {
            bail!("{path} 包含重复选项值 {:?}", option.value);
        }
    }
    Ok(())
}

fn string_array(object: &Map<String, Value>, path: &str, field: &str) -> Result<Vec<String>> {
    let values = object
        .get(field)
        .and_then(Value::as_array)
        .with_context(|| format!("{path}.{field} 必须是字符串数组"))?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("{path}.{field}[{index}] 必须是字符串"))
        })
        .collect()
}

fn default_value(
    kind: &AgentMcpElicitationFieldKind,
    property: &Map<String, Value>,
    path: &str,
) -> Result<Option<AgentMcpElicitationValue>> {
    let Some(default) = property.get("default") else {
        return Ok(None);
    };
    if default.is_null() {
        return Ok(None);
    }
    let value = match kind {
        AgentMcpElicitationFieldKind::String {
            min_length,
            max_length,
            ..
        } => {
            let value = default
                .as_str()
                .with_context(|| format!("{path}.default 必须是字符串或 null"))?;
            let length = value.chars().count() as u64;
            if min_length.is_some_and(|min| length < min) {
                bail!("{path}.default 短于 minLength");
            }
            if max_length.is_some_and(|max| length > max) {
                bail!("{path}.default 长于 maxLength");
            }
            AgentMcpElicitationValue::String(value.to_owned())
        }
        AgentMcpElicitationFieldKind::Number {
            integer,
            minimum,
            maximum,
        } => {
            let number = default
                .as_number()
                .with_context(|| format!("{path}.default 必须是数字或 null"))?;
            if *integer && number.as_i64().is_none() && number.as_u64().is_none() {
                bail!("{path}.default 必须是整数");
            }
            let numeric = number
                .as_f64()
                .with_context(|| format!("{path}.default 不是可用数字"))?;
            if minimum
                .as_ref()
                .is_some_and(|min| min.as_f64().is_some_and(|min| numeric < min))
            {
                bail!("{path}.default 小于 minimum");
            }
            if maximum
                .as_ref()
                .is_some_and(|max| max.as_f64().is_some_and(|max| numeric > max))
            {
                bail!("{path}.default 大于 maximum");
            }
            AgentMcpElicitationValue::Number(number.clone())
        }
        AgentMcpElicitationFieldKind::Boolean => AgentMcpElicitationValue::Boolean(
            default
                .as_bool()
                .with_context(|| format!("{path}.default 必须是布尔值或 null"))?,
        ),
        AgentMcpElicitationFieldKind::SingleSelect { options } => {
            let value = default
                .as_str()
                .with_context(|| format!("{path}.default 必须是字符串或 null"))?;
            if !options.iter().any(|option| option.value == value) {
                bail!("{path}.default 不在可选值内");
            }
            AgentMcpElicitationValue::String(value.to_owned())
        }
        AgentMcpElicitationFieldKind::MultiSelect {
            options,
            min_items,
            max_items,
        } => {
            let values = string_array(property, path, "default")?;
            let count = values.len() as u64;
            if min_items.is_some_and(|min| count < min) {
                bail!("{path}.default 少于 minItems");
            }
            if max_items.is_some_and(|max| count > max) {
                bail!("{path}.default 多于 maxItems");
            }
            let mut seen = HashSet::new();
            for value in &values {
                if !options.iter().any(|option| option.value == *value) {
                    bail!("{path}.default 包含不在 items 内的值 {value:?}");
                }
                if !seen.insert(value.as_str()) {
                    bail!("{path}.default 包含重复值 {value:?}");
                }
            }
            AgentMcpElicitationValue::StringArray(values)
        }
    };
    Ok(Some(value))
}

/// Validate a response against the decoded schema and produce the MCP result
/// object. Invalid input is rejected before anything is written, so the form
/// stays answerable and the request is never answered twice.
pub(super) fn elicitation_response_result(
    request: &AgentMcpElicitationRequest,
    response: &AgentMcpElicitationResponse,
) -> Result<Value> {
    let action = match response.action {
        AgentMcpElicitationAction::Accept => "accept",
        AgentMcpElicitationAction::Decline => "decline",
        AgentMcpElicitationAction::Cancel => "cancel",
    };
    if response.action != AgentMcpElicitationAction::Accept {
        if response.content.is_some() {
            bail!("MCP elicitation {action} 响应不能携带 content");
        }
        return Ok(json!({ "action": action }));
    }
    let content = response
        .content
        .as_ref()
        .context("MCP elicitation accept 响应缺少 content")?;
    let AgentMcpElicitationMode::Form(form) = &request.mode else {
        if !content.fields.is_empty() {
            bail!("MCP elicitation url 模式的 accept 响应不能携带 content");
        }
        return Ok(json!({ "action": action }));
    };
    let mut provided = Map::new();
    for AgentMcpElicitationFieldValue { name, value } in &content.fields {
        let field = form
            .fields
            .iter()
            .find(|field| &field.name == name)
            .with_context(|| format!("MCP elicitation content 包含未定义字段 {name:?}"))?;
        let encoded = validate_field_value(field, value)?;
        if provided.insert(name.clone(), encoded).is_some() {
            bail!("MCP elicitation content 包含重复字段 {name:?}");
        }
    }
    for field in &form.fields {
        if field.required && !provided.contains_key(&field.name) {
            bail!("MCP elicitation 必填字段 {:?} 缺少值", field.name);
        }
    }
    Ok(json!({ "action": action, "content": Value::Object(provided) }))
}

fn validate_field_value(
    field: &AgentMcpElicitationField,
    value: &AgentMcpElicitationValue,
) -> Result<Value> {
    let name = field.name.as_str();
    match (&field.kind, value) {
        (
            AgentMcpElicitationFieldKind::String {
                min_length,
                max_length,
                format,
            },
            AgentMcpElicitationValue::String(value),
        ) => {
            if value.trim().is_empty() && field.required {
                bail!("MCP elicitation 必填字段 {name:?} 不能为空");
            }
            let length = value.chars().count() as u64;
            if let Some(min) = min_length
                && length < *min
            {
                bail!("MCP elicitation 字段 {name:?} 至少需要 {min} 个字符");
            }
            if let Some(max) = max_length
                && length > *max
            {
                bail!("MCP elicitation 字段 {name:?} 最多允许 {max} 个字符");
            }
            if let Some(format) = format {
                validate_string_format(*format, value, name)?;
            }
            Ok(json!(value))
        }
        (
            AgentMcpElicitationFieldKind::Number {
                integer,
                minimum,
                maximum,
            },
            AgentMcpElicitationValue::Number(value),
        ) => {
            if *integer && value.as_i64().is_none() && value.as_u64().is_none() {
                bail!("MCP elicitation 字段 {name:?} 必须是整数");
            }
            let numeric = value
                .as_f64()
                .with_context(|| format!("MCP elicitation 字段 {name:?} 不是可用数字"))?;
            if minimum
                .as_ref()
                .is_some_and(|min| min.as_f64().is_some_and(|min| numeric < min))
            {
                bail!("MCP elicitation 字段 {name:?} 小于允许的最小值");
            }
            if maximum
                .as_ref()
                .is_some_and(|max| max.as_f64().is_some_and(|max| numeric > max))
            {
                bail!("MCP elicitation 字段 {name:?} 大于允许的最大值");
            }
            Ok(json!(value))
        }
        (AgentMcpElicitationFieldKind::Boolean, AgentMcpElicitationValue::Boolean(value)) => {
            Ok(json!(value))
        }
        (
            AgentMcpElicitationFieldKind::SingleSelect { options },
            AgentMcpElicitationValue::String(value),
        ) => {
            if !options.iter().any(|option| &option.value == value) {
                bail!("MCP elicitation 字段 {name:?} 的取值不在可选列表内");
            }
            Ok(json!(value))
        }
        (
            AgentMcpElicitationFieldKind::MultiSelect {
                options,
                min_items,
                max_items,
            },
            AgentMcpElicitationValue::StringArray(values),
        ) => {
            let count = values.len() as u64;
            if let Some(min) = min_items
                && count < *min
            {
                bail!("MCP elicitation 字段 {name:?} 至少需要 {min} 个选项");
            }
            if let Some(max) = max_items
                && count > *max
            {
                bail!("MCP elicitation 字段 {name:?} 最多允许 {max} 个选项");
            }
            let mut seen = HashSet::new();
            for value in values {
                if !options.iter().any(|option| option.value == *value) {
                    bail!("MCP elicitation 字段 {name:?} 的取值不在可选列表内");
                }
                if !seen.insert(value.as_str()) {
                    bail!("MCP elicitation 字段 {name:?} 包含重复选项 {value:?}");
                }
            }
            Ok(json!(values))
        }
        _ => Err(anyhow!(
            "MCP elicitation 字段 {name:?} 的响应类型与 requestedSchema 不一致"
        )),
    }
}

fn validate_string_format(
    format: AgentMcpElicitationStringFormat,
    value: &str,
    name: &str,
) -> Result<()> {
    if value.trim().is_empty() {
        return Ok(());
    }
    let valid = match format {
        AgentMcpElicitationStringFormat::Email => {
            let mut parts = value.split('@');
            matches!(
                (parts.next(), parts.next(), parts.next()),
                (Some(local), Some(domain), None)
                    if !local.is_empty()
                        && domain.contains('.')
                        && !domain.starts_with('.')
                        && !domain.ends_with('.')
            )
        }
        AgentMcpElicitationStringFormat::Uri => url::Url::parse(value).is_ok(),
        AgentMcpElicitationStringFormat::Date => is_date(value),
        AgentMcpElicitationStringFormat::DateTime => {
            value.len() > 10 && value.as_bytes().get(10) == Some(&b'T') && is_date(&value[..10])
        }
    };
    if !valid {
        bail!("MCP elicitation 字段 {name:?} 不符合 requestedSchema 的 format 约束");
    }
    Ok(())
}

fn is_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<u32> { value.get(range)?.parse().ok() };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    year >= 1 && (1..=12).contains(&month) && (1..=31).contains(&day)
}

#[cfg(test)]
mod tests;
