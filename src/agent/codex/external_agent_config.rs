//! `externalAgentConfig/*` codecs, including the progress and completed
//! notifications that close an import.

use anyhow::Result;
use serde_json::{Map, Value};

use super::json::{
    array, array_or_empty, object as as_object, optional_string, params, required_enum,
    required_i64, required_string, unknown_fields,
};
use crate::agent::{
    AgentExternalAgentDetectRequest, AgentExternalAgentDetectResult,
    AgentExternalAgentDetectedConnector, AgentExternalAgentFailure,
    AgentExternalAgentHistoryRecordRequest, AgentExternalAgentImportHistories,
    AgentExternalAgentImportHistory, AgentExternalAgentImportReceipt,
    AgentExternalAgentImportRequest, AgentExternalAgentImportStatus,
    AgentExternalAgentImportedConnector, AgentExternalAgentImportedConnectorSource,
    AgentExternalAgentItemType, AgentExternalAgentMigrationItem, AgentExternalAgentSuccess,
    AgentExternalAgentTypeResult,
};

const METHOD_DETECT: &str = "externalAgentConfig/detect";
const METHOD_IMPORT: &str = "externalAgentConfig/import";
const METHOD_HISTORIES: &str = "externalAgentConfig/import/readHistories";
const METHOD_RECORD: &str = "externalAgentConfig/import/recordHistory";
const METHOD_PROGRESS: &str = "externalAgentConfig/import/progress";
const METHOD_COMPLETED: &str = "externalAgentConfig/import/completed";

fn paths_value(paths: &[std::path::PathBuf]) -> Value {
    Value::Array(
        paths
            .iter()
            .map(|path| Value::String(path.display().to_string()))
            .collect(),
    )
}

pub(super) fn detect_params(request: &AgentExternalAgentDetectRequest) -> Value {
    let mut params = Map::new();
    params.insert("includeHome".into(), Value::Bool(request.include_home));
    if let Some(cwds) = &request.cwds {
        params.insert("cwds".into(), paths_value(cwds));
    }
    if let Some(days) = request.max_session_age_days {
        params.insert("maxSessionAgeDays".into(), Value::from(days));
    }
    if let Some(sessions) = request.max_sessions {
        params.insert("maxSessions".into(), Value::from(sessions));
    }
    if let Some(source) = &request.migration_source {
        params.insert("migrationSource".into(), Value::String(source.clone()));
    }
    if let Some(source) = &request.source {
        params.insert("source".into(), Value::String(source.clone()));
    }
    Value::Object(params)
}

fn migration_item_value(item: &AgentExternalAgentMigrationItem) -> Value {
    let mut entry = Map::new();
    entry.insert(
        "itemType".into(),
        Value::String(item.item_type.as_str().to_owned()),
    );
    entry.insert(
        "description".into(),
        Value::String(item.description.clone()),
    );
    entry.insert(
        "cwd".into(),
        match &item.cwd {
            Some(cwd) => Value::String(cwd.clone()),
            None => Value::Null,
        },
    );
    entry.insert(
        "details".into(),
        match &item.details {
            Some(details) => details.clone(),
            None => Value::Null,
        },
    );
    Value::Object(entry)
}

pub(super) fn import_params(request: &AgentExternalAgentImportRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "migrationItems".into(),
        Value::Array(
            request
                .migration_items
                .iter()
                .map(migration_item_value)
                .collect(),
        ),
    );
    if let Some(source) = &request.migration_source {
        params.insert("migrationSource".into(), Value::String(source.clone()));
    }
    if let Some(provider_id) = &request.provider_id {
        params.insert("providerId".into(), Value::String(provider_id.clone()));
    }
    if let Some(source) = &request.source {
        params.insert("source".into(), Value::String(source.clone()));
    }
    Value::Object(params)
}

pub(super) fn history_record_params(request: &AgentExternalAgentHistoryRecordRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "providerId".into(),
        Value::String(request.provider_id.clone()),
    );
    params.insert(
        "itemTypeResults".into(),
        Value::Array(
            request
                .item_type_results
                .iter()
                .map(|result| {
                    let mut entry = Map::new();
                    entry.insert(
                        "itemType".into(),
                        Value::String(result.item_type.as_str().to_owned()),
                    );
                    entry.insert(
                        "successes".into(),
                        Value::Array(result.successes.iter().map(success_value).collect()),
                    );
                    entry.insert(
                        "failures".into(),
                        Value::Array(result.failures.iter().map(failure_value).collect()),
                    );
                    Value::Object(entry)
                })
                .collect(),
        ),
    );
    Value::Object(params)
}

fn success_value(success: &AgentExternalAgentSuccess) -> Value {
    let mut entry = Map::new();
    entry.insert(
        "itemType".into(),
        Value::String(success.item_type.as_str().to_owned()),
    );
    entry.insert("cwd".into(), optional_string_value(&success.cwd));
    entry.insert("source".into(), optional_string_value(&success.source));
    entry.insert("target".into(), optional_string_value(&success.target));
    entry.insert("title".into(), optional_string_value(&success.title));
    for (key, value) in &success.extra {
        entry.insert(key.clone(), value.clone());
    }
    Value::Object(entry)
}

fn failure_value(failure: &AgentExternalAgentFailure) -> Value {
    let mut entry = Map::new();
    entry.insert(
        "itemType".into(),
        Value::String(failure.item_type.as_str().to_owned()),
    );
    entry.insert(
        "failureStage".into(),
        Value::String(failure.failure_stage.clone()),
    );
    entry.insert("message".into(), Value::String(failure.message.clone()));
    entry.insert("cwd".into(), optional_string_value(&failure.cwd));
    entry.insert("source".into(), optional_string_value(&failure.source));
    entry.insert(
        "errorType".into(),
        optional_string_value(&failure.error_type),
    );
    entry.insert(
        "subErrorType".into(),
        optional_string_value(&failure.sub_error_type),
    );
    for (key, value) in &failure.extra {
        entry.insert(key.clone(), value.clone());
    }
    Value::Object(entry)
}

fn optional_string_value(value: &Option<String>) -> Value {
    match value {
        Some(value) => Value::String(value.clone()),
        None => Value::Null,
    }
}

fn migration_item(value: &Value) -> Result<AgentExternalAgentMigrationItem> {
    let object = as_object(value, &format!("{METHOD_DETECT} 的 item"))?;
    Ok(AgentExternalAgentMigrationItem {
        item_type: required_enum(
            object,
            "itemType",
            "migration item",
            AgentExternalAgentItemType::parse,
        )?,
        description: required_string(object, "description", "migration item")?,
        cwd: optional_string(object, "cwd", "migration item")?,
        details: super::json::optional_value(object, "details"),
    })
}

fn detected_connector(value: &Value) -> Result<AgentExternalAgentDetectedConnector> {
    const KNOWN: &[&str] = &["name", "sessionCount", "source"];
    let object = as_object(value, &format!("{METHOD_DETECT} 的 connector"))?;
    Ok(AgentExternalAgentDetectedConnector {
        name: required_string(object, "name", "detected connector")?,
        session_count: required_i64(object, "sessionCount", "detected connector")?,
        source: required_enum(
            object,
            "source",
            "detected connector",
            crate::agent::AgentExternalAgentConnectorSource::parse,
        )?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_detect(
    generation: u64,
    request: &AgentExternalAgentDetectRequest,
    value: &Value,
) -> Result<AgentExternalAgentDetectResult> {
    let object = as_object(value, &format!("{METHOD_DETECT} 响应"))?;
    let items = array_or_empty(object, "items", "detect 响应的 items")?;
    let connectors = match object.get("connectors") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            array(value, "detect 响应的 connectors")?
                .iter()
                .map(detected_connector)
                .collect::<Result<Vec<_>>>()?,
        ),
    };
    Ok(AgentExternalAgentDetectResult {
        generation,
        request: request.clone(),
        items: items
            .iter()
            .map(migration_item)
            .collect::<Result<Vec<_>>>()?,
        connectors,
        extra: unknown_fields(object, &["items", "connectors"]),
    })
}

pub(super) fn decode_import_receipt(value: &Value) -> Result<AgentExternalAgentImportReceipt> {
    let object = as_object(value, &format!("{METHOD_IMPORT} 响应"))?;
    Ok(AgentExternalAgentImportReceipt {
        import_id: required_string(object, "importId", "import 响应")?,
    })
}

fn success(value: &Value) -> Result<AgentExternalAgentSuccess> {
    const KNOWN: &[&str] = &["itemType", "cwd", "source", "target", "title"];
    let object = as_object(value, "import success")?;
    Ok(AgentExternalAgentSuccess {
        item_type: required_enum(
            object,
            "itemType",
            "import success",
            AgentExternalAgentItemType::parse,
        )?,
        cwd: optional_string(object, "cwd", "import success")?,
        source: optional_string(object, "source", "import success")?,
        target: optional_string(object, "target", "import success")?,
        title: optional_string(object, "title", "import success")?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn failure(value: &Value) -> Result<AgentExternalAgentFailure> {
    const KNOWN: &[&str] = &[
        "itemType",
        "failureStage",
        "message",
        "cwd",
        "source",
        "errorType",
        "subErrorType",
    ];
    let object = as_object(value, "import failure")?;
    Ok(AgentExternalAgentFailure {
        item_type: required_enum(
            object,
            "itemType",
            "import failure",
            AgentExternalAgentItemType::parse,
        )?,
        failure_stage: required_string(object, "failureStage", "import failure")?,
        message: required_string(object, "message", "import failure")?,
        cwd: optional_string(object, "cwd", "import failure")?,
        source: optional_string(object, "source", "import failure")?,
        error_type: optional_string(object, "errorType", "import failure")?,
        sub_error_type: optional_string(object, "subErrorType", "import failure")?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn type_result(value: &Value) -> Result<AgentExternalAgentTypeResult> {
    let object = as_object(value, "import itemTypeResult")?;
    let successes = array_or_empty(object, "successes", "itemTypeResult 的 successes")?;
    let failures = array_or_empty(object, "failures", "itemTypeResult 的 failures")?;
    Ok(AgentExternalAgentTypeResult {
        item_type: required_enum(
            object,
            "itemType",
            "import itemTypeResult",
            AgentExternalAgentItemType::parse,
        )?,
        successes: successes.iter().map(success).collect::<Result<Vec<_>>>()?,
        failures: failures.iter().map(failure).collect::<Result<Vec<_>>>()?,
        extra: unknown_fields(object, &["itemType", "successes", "failures"]),
    })
}

fn import_status(
    method: &str,
    generation: u64,
    message: &Value,
    completed: bool,
) -> Result<AgentExternalAgentImportStatus> {
    let params = params(message, method)?;
    let results = array_or_empty(params, "itemTypeResults", "import 通知的 itemTypeResults")?;
    Ok(AgentExternalAgentImportStatus {
        generation,
        import_id: required_string(params, "importId", method)?,
        completed,
        item_type_results: results
            .iter()
            .map(type_result)
            .collect::<Result<Vec<_>>>()?,
        extra: unknown_fields(params, &["importId", "itemTypeResults"]),
    })
}

pub(super) fn decode_progress(
    generation: u64,
    message: &Value,
) -> Result<AgentExternalAgentImportStatus> {
    import_status(METHOD_PROGRESS, generation, message, false)
}

pub(super) fn decode_completed(
    generation: u64,
    message: &Value,
) -> Result<AgentExternalAgentImportStatus> {
    import_status(METHOD_COMPLETED, generation, message, true)
}

fn history(value: &Value) -> Result<AgentExternalAgentImportHistory> {
    let object = as_object(value, "import history")?;
    let successes = array_or_empty(object, "successes", "import history 的 successes")?;
    let failures = array_or_empty(object, "failures", "import history 的 failures")?;
    Ok(AgentExternalAgentImportHistory {
        import_id: required_string(object, "importId", "import history")?,
        provider_id: optional_string(object, "providerId", "import history")?,
        completed_at_ms: required_i64(object, "completedAtMs", "import history")?,
        successes: successes.iter().map(success).collect::<Result<Vec<_>>>()?,
        failures: failures.iter().map(failure).collect::<Result<Vec<_>>>()?,
        extra: unknown_fields(
            object,
            &[
                "importId",
                "providerId",
                "completedAtMs",
                "successes",
                "failures",
            ],
        ),
    })
}

fn imported_connector(value: &Value) -> Result<AgentExternalAgentImportedConnector> {
    const KNOWN: &[&str] = &["name", "sessionCount", "source"];
    let object = as_object(value, "imported connector")?;
    Ok(AgentExternalAgentImportedConnector {
        name: required_string(object, "name", "imported connector")?,
        session_count: required_i64(object, "sessionCount", "imported connector")?,
        source: required_enum(
            object,
            "source",
            "imported connector",
            AgentExternalAgentImportedConnectorSource::parse,
        )?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_histories(
    generation: u64,
    value: &Value,
) -> Result<AgentExternalAgentImportHistories> {
    let object = as_object(value, &format!("{METHOD_HISTORIES} 响应"))?;
    let data = array_or_empty(object, "data", "readHistories 响应的 data")?;
    let connectors = array_or_empty(object, "connectors", "readHistories 响应的 connectors")?;
    Ok(AgentExternalAgentImportHistories {
        generation,
        histories: data.iter().map(history).collect::<Result<Vec<_>>>()?,
        connectors: connectors
            .iter()
            .map(imported_connector)
            .collect::<Result<Vec<_>>>()?,
        extra: unknown_fields(object, &["data", "connectors"]),
    })
}

pub(super) fn decode_history_record_receipt(
    value: &Value,
) -> Result<AgentExternalAgentImportReceipt> {
    let object = as_object(value, &format!("{METHOD_RECORD} 响应"))?;
    Ok(AgentExternalAgentImportReceipt {
        import_id: required_string(object, "importId", "recordHistory 响应")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentExternalAgentDetectRequest, AgentExternalAgentImportRequest};
    use serde_json::json;

    #[test]
    fn detect_and_import_params_match_the_reference_client() {
        // includeHome is always sent; migrationSource only for the provider
        // whose detection route needs it.
        assert_eq!(
            detect_params(&AgentExternalAgentDetectRequest {
                cwds: None,
                include_home: true,
                max_session_age_days: None,
                max_sessions: None,
                migration_source: Some("cursor".into()),
                source: None,
            }),
            json!({"includeHome": true, "migrationSource": "cursor"})
        );
        assert_eq!(
            detect_params(&AgentExternalAgentDetectRequest {
                cwds: Some(vec!["/tmp/project".into()]),
                include_home: false,
                max_session_age_days: Some(30),
                max_sessions: Some(5),
                migration_source: None,
                source: None,
            }),
            json!({
                "includeHome": false,
                "cwds": ["/tmp/project"],
                "maxSessionAgeDays": 30,
                "maxSessions": 5
            })
        );
        let item = super::migration_item(&json!({
            "itemType": "SESSIONS",
            "description": "Migrate recent sessions",
            "cwd": null,
            "details": {"sessions": [{"title": "t"}]}
        }))
        .unwrap();
        assert_eq!(item.cwd, None);
        assert_eq!(
            import_params(&AgentExternalAgentImportRequest {
                migration_items: vec![item],
                migration_source: Some("cursor".into()),
                provider_id: Some("cursor".into()),
                source: Some("app".into()),
            }),
            json!({
                "migrationItems": [{
                    "itemType": "SESSIONS",
                    "description": "Migrate recent sessions",
                    "cwd": null,
                    "details": {"sessions": [{"title": "t"}]}
                }],
                "migrationSource": "cursor",
                "providerId": "cursor",
                "source": "app"
            })
        );
    }

    #[test]
    fn detect_decode_keeps_unknown_fields_and_connector_absence() {
        let request = AgentExternalAgentDetectRequest {
            include_home: true,
            ..Default::default()
        };
        let result = decode_detect(
            2,
            &request,
            &json!({
                "items": [{
                    "itemType": "CONFIG",
                    "description": "Migrate settings",
                    "details": null,
                    "vendorExtension": {"v": 1}
                }],
                "pageExtension": true
            }),
        )
        .unwrap();
        // A missing connectors key is not the same answer as an empty list.
        assert_eq!(result.connectors, None);
        assert_eq!(
            result.items[0].item_type,
            AgentExternalAgentItemType::Config
        );
        assert_eq!(result.extra["pageExtension"], json!(true));

        let empty = decode_detect(2, &request, &json!({"items": [], "connectors": []})).unwrap();
        assert!(empty.connectors.as_ref().unwrap().is_empty());
    }

    #[test]
    fn unknown_item_types_and_incomplete_failures_are_protocol_errors() {
        let request = AgentExternalAgentDetectRequest::default();
        assert!(
            decode_detect(
                1,
                &request,
                &json!({"items": [{"itemType": "TELEPATHY", "description": "x"}], "connectors": []})
            )
            .is_err()
        );
        assert!(
            decode_completed(
                1,
                &json!({
                    "method": "externalAgentConfig/import/completed",
                    "params": {
                        "importId": "i",
                        "itemTypeResults": [{
                            "itemType": "SKILLS",
                            "successes": [],
                            "failures": [{"itemType": "SKILLS", "message": "no stage"}]
                        }]
                    }
                })
            )
            .is_err()
        );
    }
}
