//! `app/list`, `app/installed`, `app/read` and `app/list/updated` codecs.

use anyhow::Result;
use serde_json::{Map, Value};

use super::json::{
    array, array_or_empty, defaulted_bool, object as as_object, optional_bool, optional_string,
    optional_string_list, optional_string_map, params, required_bool, required_string,
    unknown_fields,
};
use crate::agent::{
    AgentAppBranding, AgentAppInfo, AgentAppMetadata, AgentAppMetadataEntry, AgentAppReview,
    AgentAppScreenshot, AgentAppToolSummary, AgentAppsInstalledRequest, AgentAppsListRequest,
    AgentAppsPage, AgentAppsReadRequest, AgentAppsReadResult, AgentInstalledApp,
    AgentInstalledApps,
};

const METHOD_LIST: &str = "app/list";
const METHOD_INSTALLED: &str = "app/installed";
const METHOD_READ: &str = "app/read";
const METHOD_UPDATED: &str = "app/list/updated";

pub(super) fn list_params(request: &AgentAppsListRequest) -> Value {
    let mut params = Map::new();
    if let Some(cursor) = &request.cursor {
        params.insert("cursor".into(), Value::String(cursor.clone()));
    }
    if let Some(limit) = request.limit {
        params.insert("limit".into(), Value::from(limit));
    }
    if request.force_refetch {
        params.insert("forceRefetch".into(), Value::Bool(true));
    }
    if let Some(thread_id) = &request.thread_id {
        params.insert("threadId".into(), Value::String(thread_id.clone()));
    }
    Value::Object(params)
}

pub(super) fn installed_params(request: &AgentAppsInstalledRequest) -> Value {
    let mut params = Map::new();
    if request.force_refresh {
        params.insert("forceRefresh".into(), Value::Bool(true));
    }
    if let Some(thread_id) = &request.thread_id {
        params.insert("threadId".into(), Value::String(thread_id.clone()));
    }
    Value::Object(params)
}

pub(super) fn read_params(request: &AgentAppsReadRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "appIds".into(),
        Value::Array(request.app_ids.iter().cloned().map(Value::String).collect()),
    );
    if request.include_tools {
        params.insert("includeTools".into(), Value::Bool(true));
    }
    if let Some(thread_id) = &request.thread_id {
        params.insert("threadId".into(), Value::String(thread_id.clone()));
    }
    Value::Object(params)
}

fn branding(value: &Value) -> Result<AgentAppBranding> {
    const KNOWN: &[&str] = &[
        "category",
        "developer",
        "isDiscoverableApp",
        "privacyPolicy",
        "termsOfService",
        "website",
    ];
    let object = as_object(value, "app/list 的 branding")?;
    Ok(AgentAppBranding {
        category: optional_string(object, "category", "branding")?,
        developer: optional_string(object, "developer", "branding")?,
        is_discoverable_app: required_bool(object, "isDiscoverableApp", "branding")?,
        privacy_policy: optional_string(object, "privacyPolicy", "branding")?,
        terms_of_service: optional_string(object, "termsOfService", "branding")?,
        website: optional_string(object, "website", "branding")?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn screenshot(value: &Value) -> Result<AgentAppScreenshot> {
    const KNOWN: &[&str] = &["fileId", "url", "userPrompt"];
    let object = as_object(value, "appMetadata 的 screenshot")?;
    Ok(AgentAppScreenshot {
        file_id: optional_string(object, "fileId", "screenshot")?,
        url: optional_string(object, "url", "screenshot")?,
        user_prompt: required_string(object, "userPrompt", "screenshot")?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn metadata(value: &Value) -> Result<AgentAppMetadata> {
    const KNOWN: &[&str] = &[
        "categories",
        "developer",
        "firstPartyRequiresInstall",
        "review",
        "screenshots",
        "seoDescription",
        "showInComposerWhenUnlinked",
        "subCategories",
        "version",
        "versionId",
        "versionNotes",
    ];
    let object = as_object(value, "app/list 的 appMetadata")?;
    Ok(AgentAppMetadata {
        categories: optional_string_list(object, "categories", "appMetadata")?,
        developer: optional_string(object, "developer", "appMetadata")?,
        first_party_requires_install: optional_bool(
            object,
            "firstPartyRequiresInstall",
            "appMetadata",
        )?,
        review: match object.get("review") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let review = as_object(value, "appMetadata 的 review")?;
                Some(AgentAppReview {
                    status: required_string(review, "status", "review")?,
                    extra: unknown_fields(review, &["status"]),
                })
            }
        },
        screenshots: match object.get("screenshots") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                array(value, "appMetadata 的 screenshots")?
                    .iter()
                    .map(screenshot)
                    .collect::<Result<Vec<_>>>()?,
            ),
        },
        seo_description: optional_string(object, "seoDescription", "appMetadata")?,
        show_in_composer_when_unlinked: optional_bool(
            object,
            "showInComposerWhenUnlinked",
            "appMetadata",
        )?,
        sub_categories: optional_string_list(object, "subCategories", "appMetadata")?,
        version: optional_string(object, "version", "appMetadata")?,
        version_id: optional_string(object, "versionId", "appMetadata")?,
        version_notes: optional_string(object, "versionNotes", "appMetadata")?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn app_info(value: &Value) -> Result<AgentAppInfo> {
    const KNOWN: &[&str] = &[
        "id",
        "name",
        "description",
        "distributionChannel",
        "installUrl",
        "logoUrl",
        "logoUrlDark",
        "iconAssets",
        "iconDarkAssets",
        "labels",
        "pluginDisplayNames",
        "isAccessible",
        "isEnabled",
        "appMetadata",
        "branding",
    ];
    let object = as_object(value, &format!("{METHOD_LIST} 的 app"))?;
    Ok(AgentAppInfo {
        id: required_string(object, "id", "app")?,
        name: required_string(object, "name", "app")?,
        description: optional_string(object, "description", "app")?,
        distribution_channel: optional_string(object, "distributionChannel", "app")?,
        install_url: optional_string(object, "installUrl", "app")?,
        logo_url: optional_string(object, "logoUrl", "app")?,
        logo_url_dark: optional_string(object, "logoUrlDark", "app")?,
        icon_assets: optional_string_map(object, "iconAssets", "app")?,
        icon_dark_assets: optional_string_map(object, "iconDarkAssets", "app")?,
        labels: optional_string_map(object, "labels", "app")?,
        plugin_display_names: super::json::defaulted_string_list(
            object,
            "pluginDisplayNames",
            "app",
        )?,
        is_accessible: defaulted_bool(object, "isAccessible", false, "app")?,
        is_enabled: defaulted_bool(object, "isEnabled", true, "app")?,
        branding: match object.get("branding") {
            None | Some(Value::Null) => None,
            Some(value) => Some(branding(value)?),
        },
        metadata: match object.get("appMetadata") {
            None | Some(Value::Null) => None,
            Some(value) => Some(metadata(value)?),
        },
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_page(
    generation: u64,
    cursor: Option<String>,
    value: &Value,
) -> Result<AgentAppsPage> {
    const KNOWN: &[&str] = &["data", "nextCursor"];
    let object = as_object(value, &format!("{METHOD_LIST} 响应"))?;
    let data = array_or_empty(object, "data", "app/list 响应的 data")?;
    Ok(AgentAppsPage {
        generation,
        cursor,
        apps: data.iter().map(app_info).collect::<Result<Vec<_>>>()?,
        next_cursor: optional_string(object, "nextCursor", METHOD_LIST)?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn installed_app(value: &Value) -> Result<AgentInstalledApp> {
    const KNOWN: &[&str] = &["id", "enabled", "callable", "runtimeName"];
    let object = as_object(value, &format!("{METHOD_INSTALLED} 的 app"))?;
    Ok(AgentInstalledApp {
        id: required_string(object, "id", "installed app")?,
        enabled: required_bool(object, "enabled", "installed app")?,
        callable: required_bool(object, "callable", "installed app")?,
        runtime_name: optional_string(object, "runtimeName", "installed app")?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_installed(generation: u64, value: &Value) -> Result<AgentInstalledApps> {
    let object = as_object(value, &format!("{METHOD_INSTALLED} 响应"))?;
    let apps = array_or_empty(object, "apps", "app/installed 响应的 apps")?;
    Ok(AgentInstalledApps {
        generation,
        apps: apps.iter().map(installed_app).collect::<Result<Vec<_>>>()?,
        extra: unknown_fields(object, &["apps"]),
    })
}

fn tool_summary(value: &Value) -> Result<AgentAppToolSummary> {
    const KNOWN: &[&str] = &[
        "name",
        "title",
        "description",
        "disabledReason",
        "isEnabled",
        "isReadOnly",
    ];
    let object = as_object(value, "app/read 的 toolSummary")?;
    Ok(AgentAppToolSummary {
        name: required_string(object, "name", "toolSummary")?,
        title: optional_string(object, "title", "toolSummary")?,
        description: required_string(object, "description", "toolSummary")?,
        disabled_reason: optional_string(object, "disabledReason", "toolSummary")?,
        is_enabled: defaulted_bool(object, "isEnabled", true, "toolSummary")?,
        is_read_only: defaulted_bool(object, "isReadOnly", false, "toolSummary")?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn metadata_entry(value: &Value) -> Result<AgentAppMetadataEntry> {
    const KNOWN: &[&str] = &[
        "id",
        "name",
        "description",
        "distributionChannel",
        "iconUrl",
        "iconUrlDark",
        "installUrl",
        "pluginDisplayNames",
        "toolSummaries",
    ];
    let object = as_object(value, &format!("{METHOD_READ} 的 app"))?;
    Ok(AgentAppMetadataEntry {
        id: required_string(object, "id", "app/read app")?,
        name: required_string(object, "name", "app/read app")?,
        description: optional_string(object, "description", "app/read app")?,
        distribution_channel: optional_string(object, "distributionChannel", "app/read app")?,
        icon_url: optional_string(object, "iconUrl", "app/read app")?,
        icon_url_dark: optional_string(object, "iconUrlDark", "app/read app")?,
        install_url: optional_string(object, "installUrl", "app/read app")?,
        plugin_display_names: super::json::defaulted_string_list(
            object,
            "pluginDisplayNames",
            "app/read app",
        )?,
        tool_summaries: match object.get("toolSummaries") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                array(value, "app/read app 的 toolSummaries")?
                    .iter()
                    .map(tool_summary)
                    .collect::<Result<Vec<_>>>()?,
            ),
        },
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_read(generation: u64, value: &Value) -> Result<AgentAppsReadResult> {
    let object = as_object(value, &format!("{METHOD_READ} 响应"))?;
    let apps = array_or_empty(object, "apps", "app/read 响应的 apps")?;
    Ok(AgentAppsReadResult {
        generation,
        apps: apps
            .iter()
            .map(metadata_entry)
            .collect::<Result<Vec<_>>>()?,
        missing_app_ids: super::json::defaulted_string_list(object, "missingAppIds", METHOD_READ)?,
        extra: unknown_fields(object, &["apps", "missingAppIds"]),
    })
}

/// `app/list/updated` carries the changed directory, but this client treats it
/// as a cache invalidation signal only: the payload is still decoded, so a
/// shape change fails loudly, while the canonical list always comes from a
/// fresh `app/list` read the caller issues.
pub(super) fn validate_list_updated(message: &Value) -> Result<usize> {
    let params = params(message, METHOD_UPDATED)?;
    let data = array_or_empty(params, "data", "app/list/updated 的 data")?;
    let apps = data.iter().map(app_info).collect::<Result<Vec<_>>>()?;
    Ok(apps.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentAppsListRequest, AgentAppsReadRequest};
    use serde_json::json;

    #[test]
    fn list_params_only_send_requested_fields() {
        assert_eq!(list_params(&AgentAppsListRequest::default()), json!({}));
        assert_eq!(
            list_params(&AgentAppsListRequest {
                cursor: Some("c2".into()),
                limit: Some(100),
                force_refetch: true,
                thread_id: Some("t1".into()),
            }),
            json!({"cursor": "c2", "limit": 100, "forceRefetch": true, "threadId": "t1"})
        );
        assert_eq!(
            read_params(&AgentAppsReadRequest {
                app_ids: vec!["a".into(), "b".into()],
                include_tools: true,
                thread_id: None,
            }),
            json!({"appIds": ["a", "b"], "includeTools": true})
        );
    }

    #[test]
    fn page_decode_keeps_defaults_nulls_and_unknown_fields() {
        let page = decode_page(
            4,
            None,
            &json!({
                "data": [{
                    "id": "connector",
                    "name": "Connector",
                    "description": null,
                    "isAccessible": false,
                    "pluginDisplayNames": ["Documents"],
                    "iconAssets": {"512": "/icons/512.png"},
                    "vendorExtension": {"tier": "beta"}
                }],
                "nextCursor": null,
                "pageExtension": 2
            }),
        )
        .unwrap();
        let app = &page.apps[0];
        // isEnabled is absent, so the schema default applies; a null description
        // stays absent rather than becoming an empty string.
        assert!(app.is_enabled);
        assert!(!app.is_accessible);
        assert_eq!(app.description, None);
        assert_eq!(
            app.icon_assets.as_ref().unwrap()["512"],
            json!("/icons/512.png")
        );
        assert_eq!(app.extra["vendorExtension"], json!({"tier": "beta"}));
        assert_eq!(page.extra["pageExtension"], json!(2));
    }

    #[test]
    fn installed_and_read_distinguish_missing_from_null() {
        let installed = decode_installed(
            1,
            &json!({"apps": [{"id": "a", "enabled": true, "callable": false, "runtimeName": null}]}),
        )
        .unwrap();
        assert_eq!(installed.apps[0].runtime_name, None);

        let read = decode_read(
            1,
            &json!({
                "apps": [{"id": "a", "name": "A", "toolSummaries": []}],
                "missingAppIds": ["b"]
            }),
        )
        .unwrap();
        // An empty array and an omitted field are different answers.
        assert_eq!(read.apps[0].tool_summaries.as_ref().unwrap().len(), 0);
        assert_eq!(read.missing_app_ids, vec!["b".to_owned()]);
        let omitted = decode_read(
            1,
            &json!({"apps": [{"id": "a", "name": "A"}], "missingAppIds": []}),
        )
        .unwrap();
        assert!(omitted.apps[0].tool_summaries.is_none());
    }

    #[test]
    fn updated_notification_is_decoded_and_malformed_payloads_fail() {
        assert_eq!(
            validate_list_updated(&json!({
                "method": "app/list/updated",
                "params": {"data": [{"id": "a", "name": "A"}]}
            }))
            .unwrap(),
            1
        );
        assert!(
            validate_list_updated(&json!({"method": "app/list/updated", "params": {"data": 3}}))
                .is_err()
        );
    }
}
