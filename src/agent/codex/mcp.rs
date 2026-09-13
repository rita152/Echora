//! `mcpServer/*`, `mcpServerStatus/list` and `config/mcpServer/reload` codecs.

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value};

use crate::agent::{
    AgentMcpAuthStatus, AgentMcpResource, AgentMcpResourceTemplate, AgentMcpServerConnectionStatus,
    AgentMcpServerInfo, AgentMcpServerPage, AgentMcpServerStatus, AgentMcpServerStatusRequest,
    AgentMcpTool,
};

const METHOD_STATUS: &str = "mcpServerStatus/list";

pub(super) fn status_params(request: &AgentMcpServerStatusRequest) -> Value {
    let mut params = Map::new();
    // `cursor`, `limit`, `threadId` and `detail` are nullable on the wire; only
    // values the caller actually wants are sent so defaults stay server-side.
    if let Some(cursor) = &request.cursor {
        params.insert("cursor".into(), Value::String(cursor.clone()));
    }
    if let Some(limit) = request.limit {
        params.insert("limit".into(), Value::from(limit));
    }
    if let Some(detail) = request.detail {
        params.insert("detail".into(), Value::String(detail.as_str().to_owned()));
    }
    if let Some(thread_id) = &request.thread_id {
        params.insert("threadId".into(), Value::String(thread_id.clone()));
    }
    Value::Object(params)
}

pub(super) fn oauth_login_params(
    server_name: &str,
    thread_id: Option<&str>,
    scopes: Option<&Vec<String>>,
    client_registration: Option<crate::agent::AgentMcpOauthClientRegistration>,
    timeout_secs: Option<i64>,
) -> Value {
    let mut params = Map::new();
    params.insert("name".into(), Value::String(server_name.to_owned()));
    if let Some(thread_id) = thread_id {
        params.insert("threadId".into(), Value::String(thread_id.to_owned()));
    }
    if let Some(scopes) = scopes {
        params.insert(
            "scopes".into(),
            Value::Array(scopes.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(client_registration) = client_registration {
        params.insert(
            "clientRegistration".into(),
            Value::String(client_registration.as_str().to_owned()),
        );
    }
    if let Some(timeout_secs) = timeout_secs {
        params.insert("timeoutSecs".into(), Value::from(timeout_secs));
    }
    Value::Object(params)
}

fn unknown_fields(object: &Map<String, Value>, known: &[&str]) -> Map<String, Value> {
    object
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .with_context(|| format!("{context} 必须是 JSON 对象"))
}

fn required_string(object: &Map<String, Value>, field: &str, context: &str) -> Result<String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("{context} 缺少字符串字段 {field}"))
}

fn optional_string(
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

fn optional_value(object: &Map<String, Value>, field: &str) -> Option<Value> {
    match object.get(field) {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.clone()),
    }
}

fn server_info(value: &Value) -> Result<AgentMcpServerInfo> {
    const KNOWN: &[&str] = &[
        "name",
        "version",
        "title",
        "description",
        "websiteUrl",
        "icons",
    ];
    let object = object(value, "mcpServerStatus/list 的 serverInfo")?;
    Ok(AgentMcpServerInfo {
        name: required_string(object, "name", "serverInfo")?,
        version: required_string(object, "version", "serverInfo")?,
        title: optional_string(object, "title", "serverInfo")?,
        description: optional_string(object, "description", "serverInfo")?,
        website_url: optional_string(object, "websiteUrl", "serverInfo")?,
        icons: optional_value(object, "icons"),
        extra: unknown_fields(object, KNOWN).into_iter().collect(),
    })
}

fn tool(value: &Value) -> Result<AgentMcpTool> {
    const KNOWN: &[&str] = &[
        "name",
        "title",
        "description",
        "inputSchema",
        "outputSchema",
        "annotations",
        "icons",
        "_meta",
    ];
    let object = object(value, "mcpServerStatus/list 的 tool")?;
    Ok(AgentMcpTool {
        name: required_string(object, "name", "tool")?,
        title: optional_string(object, "title", "tool")?,
        description: optional_string(object, "description", "tool")?,
        input_schema: object
            .get("inputSchema")
            .cloned()
            .context("tool 缺少 inputSchema")?,
        output_schema: optional_value(object, "outputSchema"),
        annotations: optional_value(object, "annotations"),
        icons: optional_value(object, "icons"),
        meta: optional_value(object, "_meta"),
        extra: unknown_fields(object, KNOWN).into_iter().collect(),
    })
}

fn resource(value: &Value) -> Result<AgentMcpResource> {
    const KNOWN: &[&str] = &[
        "uri",
        "name",
        "title",
        "description",
        "mimeType",
        "size",
        "annotations",
        "_meta",
        "icons",
    ];
    let object = object(value, "mcpServerStatus/list 的 resource")?;
    let size = match object.get("size") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.as_i64().context("resource.size 必须是整数")?),
    };
    Ok(AgentMcpResource {
        uri: required_string(object, "uri", "resource")?,
        name: required_string(object, "name", "resource")?,
        title: optional_string(object, "title", "resource")?,
        description: optional_string(object, "description", "resource")?,
        mime_type: optional_string(object, "mimeType", "resource")?,
        size,
        annotations: optional_value(object, "annotations"),
        meta: optional_value(object, "_meta"),
        icons: optional_value(object, "icons"),
        extra: unknown_fields(object, KNOWN).into_iter().collect(),
    })
}

fn resource_template(value: &Value) -> Result<AgentMcpResourceTemplate> {
    const KNOWN: &[&str] = &[
        "uriTemplate",
        "name",
        "title",
        "description",
        "mimeType",
        "annotations",
    ];
    let object = object(value, "mcpServerStatus/list 的 resourceTemplate")?;
    Ok(AgentMcpResourceTemplate {
        uri_template: required_string(object, "uriTemplate", "resourceTemplate")?,
        name: required_string(object, "name", "resourceTemplate")?,
        title: optional_string(object, "title", "resourceTemplate")?,
        description: optional_string(object, "description", "resourceTemplate")?,
        mime_type: optional_string(object, "mimeType", "resourceTemplate")?,
        annotations: optional_value(object, "annotations"),
        extra: unknown_fields(object, KNOWN).into_iter().collect(),
    })
}

fn server(value: &Value) -> Result<AgentMcpServerStatus> {
    const KNOWN: &[&str] = &[
        "name",
        "pluginId",
        "authStatus",
        "runtimeStatus",
        "serverInfo",
        "tools",
        "resources",
        "resourceTemplates",
        "toolsError",
    ];
    let object = object(value, &format!("{METHOD_STATUS} 的 server"))?;
    let raw_auth = required_string(object, "authStatus", "server")?;
    let auth_status = AgentMcpAuthStatus::parse(&raw_auth)
        .with_context(|| format!("{METHOD_STATUS} 的 authStatus 为未知值 `{raw_auth}`"))?;
    let runtime_status = {
        // `runtimeStatus` is nullable: null means "unavailable", which is not
        // the same as `notStarted`.
        match object.get("runtimeStatus") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let raw = value.as_str().with_context(|| {
                    format!("{METHOD_STATUS} 的 runtimeStatus 必须是字符串或 null")
                })?;
                let parsed = AgentMcpServerConnectionStatus::parse(raw).with_context(|| {
                    format!("{METHOD_STATUS} 的 runtimeStatus 为未知值 `{raw}`")
                })?;
                Some(parsed)
            }
        }
    };
    let tools = object
        .get("tools")
        .context("server 缺少 tools")?
        .as_object()
        .context("server.tools 必须是对象")?;
    let mut parsed_tools = Vec::with_capacity(tools.len());
    for value in tools.values() {
        parsed_tools.push(tool(value)?);
    }
    parsed_tools.sort_by(|left, right| left.name.cmp(&right.name));
    let resources = object
        .get("resources")
        .context("server 缺少 resources")?
        .as_array()
        .context("server.resources 必须是数组")?;
    let resource_templates = object
        .get("resourceTemplates")
        .context("server 缺少 resourceTemplates")?
        .as_array()
        .context("server.resourceTemplates 必须是数组")?;
    Ok(AgentMcpServerStatus {
        name: required_string(object, "name", "server")?,
        plugin_id: optional_string(object, "pluginId", "server")?,
        auth_status,
        runtime_status,
        server_info: match object.get("serverInfo") {
            None | Some(Value::Null) => None,
            Some(value) => Some(server_info(value)?),
        },
        tools: parsed_tools,
        resources: resources.iter().map(resource).collect::<Result<Vec<_>>>()?,
        resource_templates: resource_templates
            .iter()
            .map(resource_template)
            .collect::<Result<Vec<_>>>()?,
        tools_error: optional_string(object, "toolsError", "server")?,
        extra: unknown_fields(object, KNOWN).into_iter().collect(),
    })
}

pub(super) fn decode_page(
    generation: u64,
    cursor: Option<String>,
    value: &Value,
) -> Result<AgentMcpServerPage> {
    const KNOWN: &[&str] = &["data", "nextCursor"];
    let object = object(value, &format!("{METHOD_STATUS} 响应"))?;
    let data = object
        .get("data")
        .context("mcpServerStatus/list 响应缺少 data")?
        .as_array()
        .context("mcpServerStatus/list 响应的 data 必须是数组")?;
    Ok(AgentMcpServerPage {
        generation,
        cursor,
        servers: data.iter().map(server).collect::<Result<Vec<_>>>()?,
        next_cursor: optional_string(object, "nextCursor", METHOD_STATUS)?,
        extra: unknown_fields(object, KNOWN).into_iter().collect(),
    })
}

pub(super) fn decode_oauth_authorization_url(value: &Value) -> Result<String> {
    let object = object(value, "mcpServer/oauth/login 响应")?;
    required_string(object, "authorizationUrl", "mcpServer/oauth/login 响应")
}

/// Parsed `mcpServer/oauthLogin/completed` payload. The protocol carries no
/// login id, so the server name, optional thread scope, and the client's own
/// login registry are what make the notification correlatable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OauthCompletedNotification {
    pub server_name: String,
    pub thread_id: Option<String>,
    pub success: bool,
    pub error: Option<String>,
}

pub(super) fn parse_oauth_completed(message: &Value) -> Result<OauthCompletedNotification> {
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("mcpServer/oauthLogin/completed 缺少对象字段 params")?;
    let success = params
        .get("success")
        .and_then(Value::as_bool)
        .context("mcpServer/oauthLogin/completed 缺少布尔字段 params.success")?;
    Ok(OauthCompletedNotification {
        server_name: required_string(params, "name", "mcpServer/oauthLogin/completed")?,
        thread_id: optional_string(params, "threadId", "mcpServer/oauthLogin/completed")?,
        success,
        error: optional_string(params, "error", "mcpServer/oauthLogin/completed")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentMcpOauthClientRegistration, AgentMcpStatusDetail};
    use serde_json::json;

    #[test]
    fn status_params_only_send_requested_fields() {
        let request = AgentMcpServerStatusRequest {
            cursor: Some("c1".into()),
            limit: Some(2),
            detail: Some(AgentMcpStatusDetail::ToolsAndAuthOnly),
            thread_id: Some("t1".into()),
        };
        assert_eq!(
            status_params(&request),
            json!({"cursor":"c1","limit":2,"detail":"toolsAndAuthOnly","threadId":"t1"})
        );
        assert_eq!(
            status_params(&AgentMcpServerStatusRequest::default()),
            json!({"detail":"full"})
        );
    }

    #[test]
    fn oauth_login_params_follow_the_schema_enum() {
        for (raw, expected) in [
            ("auto", AgentMcpOauthClientRegistration::Auto),
            ("cimd", AgentMcpOauthClientRegistration::Cimd),
            ("dcr", AgentMcpOauthClientRegistration::Dcr),
        ] {
            assert_eq!(AgentMcpOauthClientRegistration::parse(raw), Some(expected));
            assert_eq!(expected.as_str(), raw);
        }
        assert_eq!(AgentMcpOauthClientRegistration::parse("other"), None);
        let scopes = vec!["notes.read".to_owned()];
        assert_eq!(
            oauth_login_params(
                "notes",
                Some("t1"),
                Some(&scopes),
                Some(AgentMcpOauthClientRegistration::Dcr),
                Some(30)
            ),
            json!({"name":"notes","threadId":"t1","scopes":["notes.read"],"clientRegistration":"dcr","timeoutSecs":30})
        );
        assert_eq!(
            oauth_login_params("notes", None, None, None, None),
            json!({"name":"notes"})
        );
    }

    #[test]
    fn page_decode_keeps_unknown_fields_and_nullable_runtime_status() {
        let page = decode_page(
            7,
            None,
            &json!({
                "data": [{
                    "name": "echo-tools",
                    "pluginId": null,
                    "authStatus": "unsupported",
                    "runtimeStatus": null,
                    "serverInfo": {"name": "echora-echo", "version": "1.4.0", "futureField": true},
                    "tools": {"echo": {"name": "echo", "inputSchema": {"type": "object"}, "extension": 1}},
                    "resources": [{"uri": "echo://a", "name": "a", "size": 12}],
                    "resourceTemplates": [{"uriTemplate": "echo://{id}", "name": "t"}],
                    "toolsError": "discovery failed",
                    "vendorExtension": {"tier": "beta"},
                }],
                "nextCursor": "c2",
                "pageExtension": 3,
            }),
        )
        .unwrap();
        assert_eq!(page.generation, 7);
        assert_eq!(page.next_cursor.as_deref(), Some("c2"));
        assert_eq!(page.extra["pageExtension"], json!(3));
        let server = &page.servers[0];
        assert_eq!(server.runtime_status, None);
        assert_eq!(server.tools_error.as_deref(), Some("discovery failed"));
        assert_eq!(server.extra["vendorExtension"], json!({"tier":"beta"}));
        assert_eq!(
            server.server_info.as_ref().unwrap().extra["futureField"],
            json!(true)
        );
        assert_eq!(server.tools[0].extra["extension"], json!(1));
        assert_eq!(server.resources[0].size, Some(12));
    }

    #[test]
    fn unknown_enum_values_are_protocol_errors() {
        let base = json!({
            "data": [{
                "name": "x", "authStatus": "mystery", "runtimeStatus": null,
                "serverInfo": null, "tools": {}, "resources": [], "resourceTemplates": []
            }]
        });
        assert!(decode_page(1, None, &base).is_err());
        let mut runtime = base.clone();
        runtime["data"][0]["authStatus"] = json!("unsupported");
        runtime["data"][0]["runtimeStatus"] = json!("mystery");
        assert!(decode_page(1, None, &runtime).is_err());
    }

    #[test]
    fn oauth_completion_requires_success_and_keeps_error() {
        let parsed = parse_oauth_completed(&json!({
            "method": "mcpServer/oauthLogin/completed",
            "params": {"name": "notes-oauth", "threadId": null, "success": false, "error": "access_denied"}
        }))
        .unwrap();
        assert_eq!(parsed.server_name, "notes-oauth");
        assert_eq!(parsed.thread_id, None);
        assert!(!parsed.success);
        assert_eq!(parsed.error.as_deref(), Some("access_denied"));
        assert!(parse_oauth_completed(&json!({"params": {"name": "x"}})).is_err());
    }
}
