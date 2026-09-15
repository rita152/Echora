//! `plugin/*` and `marketplace/*` codecs.

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value};
use std::path::PathBuf;

use super::json::{
    array, array_or_empty, defaulted_string_list, object as as_object, optional_bool,
    optional_enum, optional_i64, optional_path, optional_string, optional_string_list,
    optional_value, path_list, required_bool, required_enum, required_path, required_string,
    unknown_fields,
};
use crate::agent::{
    AgentAppTemplateUnavailableReason, AgentMarketplaceAddReceipt, AgentMarketplaceAddRequest,
    AgentMarketplaceInterface, AgentMarketplaceLoadError, AgentMarketplaceRemoveReceipt,
    AgentMarketplaceRemoveRequest, AgentMarketplaceUpgradeError, AgentMarketplaceUpgradeReceipt,
    AgentMarketplaceUpgradeRequest, AgentPluginAppSummary, AgentPluginAppTemplateSummary,
    AgentPluginAuthPolicy, AgentPluginAvailability, AgentPluginCatalog, AgentPluginCatalogRequest,
    AgentPluginDetail, AgentPluginDisabledReason, AgentPluginHookSummary, AgentPluginInstallPolicy,
    AgentPluginInstallPolicySource, AgentPluginInstallReceipt, AgentPluginInstallRequest,
    AgentPluginInstalledRequest, AgentPluginInterface, AgentPluginMarketplace,
    AgentPluginReadRequest, AgentPluginReconcileChangedPlugin, AgentPluginReconcileReceipt,
    AgentPluginReconcileRequest, AgentPluginSearchPage, AgentPluginSearchRequest,
    AgentPluginSearchResult, AgentPluginShareContext, AgentPluginShareDeleteRequest,
    AgentPluginShareDiscoverability, AgentPluginShareList, AgentPluginShareListEntry,
    AgentPluginSharePrincipal, AgentPluginSharePrincipalType, AgentPluginShareRole,
    AgentPluginShareSaveReceipt, AgentPluginShareSaveRequest, AgentPluginShareTarget,
    AgentPluginShareUpdateTargetsReceipt, AgentPluginShareUpdateTargetsRequest,
    AgentPluginSkillContent, AgentPluginSkillReadRequest, AgentPluginSkillSummary,
    AgentPluginSource, AgentPluginSummary, AgentPluginUninstallRequest,
};

const METHOD_LIST: &str = "plugin/list";
const METHOD_READ: &str = "plugin/read";
const METHOD_INSTALL: &str = "plugin/install";
const METHOD_RECONCILE: &str = "plugin/reconcile";
const METHOD_SEARCH: &str = "plugin/search";
const METHOD_SKILL_READ: &str = "plugin/skill/read";
const METHOD_SHARE_SAVE: &str = "plugin/share/save";
const METHOD_SHARE_UPDATE_TARGETS: &str = "plugin/share/updateTargets";
const METHOD_SHARE_LIST: &str = "plugin/share/list";
const METHOD_MARKETPLACE_ADD: &str = "marketplace/add";
const METHOD_MARKETPLACE_REMOVE: &str = "marketplace/remove";
const METHOD_MARKETPLACE_UPGRADE: &str = "marketplace/upgrade";

fn path_list_value(paths: &[PathBuf]) -> Value {
    Value::Array(
        paths
            .iter()
            .map(|path| Value::String(path.display().to_string()))
            .collect(),
    )
}

pub(super) fn list_params(request: &AgentPluginCatalogRequest) -> Value {
    let mut params = Map::new();
    if let Some(cwds) = &request.cwds {
        params.insert("cwds".into(), path_list_value(cwds));
    }
    if request.force_refetch {
        params.insert("forceRefetch".into(), Value::Bool(true));
    }
    if let Some(kinds) = &request.marketplace_kinds {
        params.insert(
            "marketplaceKinds".into(),
            Value::Array(
                kinds
                    .iter()
                    .map(|kind| Value::String(kind.as_str().to_owned()))
                    .collect(),
            ),
        );
    }
    Value::Object(params)
}

pub(super) fn installed_params(request: &AgentPluginInstalledRequest) -> Value {
    let mut params = Map::new();
    if let Some(cwds) = &request.cwds {
        params.insert("cwds".into(), path_list_value(cwds));
    }
    if let Some(names) = &request.install_suggestion_plugin_names {
        params.insert(
            "installSuggestionPluginNames".into(),
            Value::Array(names.iter().cloned().map(Value::String).collect()),
        );
    }
    Value::Object(params)
}

fn optional_selector(
    params: &mut Map<String, Value>,
    marketplace_path: &Option<String>,
    remote_marketplace_name: &Option<String>,
) {
    // Both selectors are nullable on the wire and mutually exclusive by
    // convention: a local marketplace is addressed by path, a remote catalog by
    // name. The client sends the one it has and an explicit null otherwise, so
    // the server never has to guess.
    params.insert(
        "marketplacePath".into(),
        match marketplace_path {
            Some(path) => Value::String(path.clone()),
            None => Value::Null,
        },
    );
    params.insert(
        "remoteMarketplaceName".into(),
        match remote_marketplace_name {
            Some(name) => Value::String(name.clone()),
            None => Value::Null,
        },
    );
}

pub(super) fn read_params(request: &AgentPluginReadRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "pluginName".into(),
        Value::String(request.plugin_name.clone()),
    );
    optional_selector(
        &mut params,
        &request.marketplace_path,
        &request.remote_marketplace_name,
    );
    Value::Object(params)
}

pub(super) fn install_params(request: &AgentPluginInstallRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "pluginName".into(),
        Value::String(request.plugin_name.clone()),
    );
    optional_selector(
        &mut params,
        &request.marketplace_path,
        &request.remote_marketplace_name,
    );
    if let Some(attempt_id) = &request.install_attempt_id {
        params.insert("installAttemptId".into(), Value::String(attempt_id.clone()));
    }
    Value::Object(params)
}

pub(super) fn uninstall_params(request: &AgentPluginUninstallRequest) -> Value {
    let mut params = Map::new();
    params.insert("pluginId".into(), Value::String(request.plugin_id.clone()));
    Value::Object(params)
}

pub(super) fn reconcile_params(request: &AgentPluginReconcileRequest) -> Value {
    let mut params = Map::new();
    if let Some(reason) = &request.reason {
        params.insert("reason".into(), Value::String(reason.clone()));
    }
    Value::Object(params)
}

pub(super) fn search_params(request: &AgentPluginSearchRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "searchTerm".into(),
        Value::String(request.search_term.clone()),
    );
    if let Some(cursor) = &request.cursor {
        params.insert("cursor".into(), Value::String(cursor.clone()));
    }
    if let Some(limit) = request.limit {
        params.insert("limit".into(), Value::from(limit));
    }
    if let Some(scope) = request.scope {
        params.insert("scope".into(), Value::String(scope.to_owned()));
    }
    if let Some(cwds) = &request.cwds {
        params.insert("cwds".into(), path_list_value(cwds));
    }
    Value::Object(params)
}

pub(super) fn skill_read_params(request: &AgentPluginSkillReadRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "remoteMarketplaceName".into(),
        Value::String(request.remote_marketplace_name.clone()),
    );
    params.insert(
        "remotePluginId".into(),
        Value::String(request.remote_plugin_id.clone()),
    );
    params.insert(
        "skillName".into(),
        Value::String(request.skill_name.clone()),
    );
    Value::Object(params)
}

fn share_targets_value(targets: &[AgentPluginShareTarget]) -> Value {
    Value::Array(
        targets
            .iter()
            .map(|target| {
                let mut entry = Map::new();
                entry.insert(
                    "principalId".into(),
                    Value::String(target.principal_id.clone()),
                );
                entry.insert(
                    "principalType".into(),
                    Value::String(target.principal_type.as_str().to_owned()),
                );
                entry.insert(
                    "role".into(),
                    Value::String(target.role.as_str().to_owned()),
                );
                for (key, value) in &target.extra {
                    entry.insert(key.clone(), value.clone());
                }
                Value::Object(entry)
            })
            .collect(),
    )
}

pub(super) fn share_save_params(request: &AgentPluginShareSaveRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "pluginPath".into(),
        Value::String(request.plugin_path.display().to_string()),
    );
    if let Some(remote_plugin_id) = &request.remote_plugin_id {
        params.insert(
            "remotePluginId".into(),
            Value::String(remote_plugin_id.clone()),
        );
    }
    if let Some(discoverability) = request.discoverability {
        params.insert(
            "discoverability".into(),
            Value::String(discoverability.as_str().to_owned()),
        );
    }
    if let Some(targets) = &request.share_targets {
        params.insert("shareTargets".into(), share_targets_value(targets));
    }
    Value::Object(params)
}

pub(super) fn share_update_targets_params(request: &AgentPluginShareUpdateTargetsRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "remotePluginId".into(),
        Value::String(request.remote_plugin_id.clone()),
    );
    params.insert(
        "discoverability".into(),
        Value::String(request.discoverability.as_str().to_owned()),
    );
    params.insert(
        "shareTargets".into(),
        share_targets_value(&request.share_targets),
    );
    Value::Object(params)
}

pub(super) fn share_list_params() -> Value {
    Value::Object(Map::new())
}

pub(super) fn share_delete_params(request: &AgentPluginShareDeleteRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "remotePluginId".into(),
        Value::String(request.remote_plugin_id.clone()),
    );
    Value::Object(params)
}

pub(super) fn marketplace_add_params(request: &AgentMarketplaceAddRequest) -> Value {
    let mut params = Map::new();
    params.insert("source".into(), Value::String(request.source.clone()));
    if let Some(ref_name) = &request.ref_name {
        params.insert("refName".into(), Value::String(ref_name.clone()));
    }
    if let Some(sparse_paths) = &request.sparse_paths {
        params.insert(
            "sparsePaths".into(),
            Value::Array(sparse_paths.iter().cloned().map(Value::String).collect()),
        );
    }
    Value::Object(params)
}

pub(super) fn marketplace_remove_params(request: &AgentMarketplaceRemoveRequest) -> Value {
    let mut params = Map::new();
    params.insert(
        "marketplaceName".into(),
        Value::String(request.marketplace_name.clone()),
    );
    Value::Object(params)
}

pub(super) fn marketplace_upgrade_params(request: &AgentMarketplaceUpgradeRequest) -> Value {
    let mut params = Map::new();
    if let Some(name) = &request.marketplace_name {
        params.insert("marketplaceName".into(), Value::String(name.clone()));
    }
    Value::Object(params)
}

fn plugin_source(value: &Value) -> Result<AgentPluginSource> {
    let object = as_object(value, "plugin source")?;
    match required_string(object, "type", "plugin source")?.as_str() {
        "local" => Ok(AgentPluginSource::Local {
            path: required_path(object, "path", "local plugin source")?,
        }),
        "git" => Ok(AgentPluginSource::Git {
            url: required_string(object, "url", "git plugin source")?,
            ref_name: optional_string(object, "refName", "git plugin source")?,
            sha: optional_string(object, "sha", "git plugin source")?,
            path: optional_string(object, "path", "git plugin source")?,
        }),
        "npm" => Ok(AgentPluginSource::Npm {
            package: required_string(object, "package", "npm plugin source")?,
            registry: optional_string(object, "registry", "npm plugin source")?,
            version: optional_string(object, "version", "npm plugin source")?,
        }),
        "remote" => Ok(AgentPluginSource::Remote),
        other => bail!("plugin source 的 type 为未知值 `{other}`"),
    }
}

fn plugin_interface(value: &Value) -> Result<AgentPluginInterface> {
    const KNOWN: &[&str] = &[
        "displayName",
        "shortDescription",
        "longDescription",
        "developerName",
        "category",
        "brandColor",
        "capabilities",
        "composerIcon",
        "composerIconUrl",
        "logo",
        "logoDark",
        "logoUrl",
        "logoUrlDark",
        "screenshots",
        "screenshotUrls",
        "websiteUrl",
        "privacyPolicyUrl",
        "termsOfServiceUrl",
        "defaultPrompt",
    ];
    let object = as_object(value, "plugin interface")?;
    Ok(AgentPluginInterface {
        display_name: optional_string(object, "displayName", "plugin interface")?,
        short_description: optional_string(object, "shortDescription", "plugin interface")?,
        long_description: optional_string(object, "longDescription", "plugin interface")?,
        developer_name: optional_string(object, "developerName", "plugin interface")?,
        category: optional_string(object, "category", "plugin interface")?,
        brand_color: optional_string(object, "brandColor", "plugin interface")?,
        capabilities: defaulted_string_list(object, "capabilities", "plugin interface")?,
        composer_icon: optional_path(object, "composerIcon", "plugin interface")?,
        composer_icon_url: optional_string(object, "composerIconUrl", "plugin interface")?,
        logo: optional_path(object, "logo", "plugin interface")?,
        logo_dark: optional_path(object, "logoDark", "plugin interface")?,
        logo_url: optional_string(object, "logoUrl", "plugin interface")?,
        logo_url_dark: optional_string(object, "logoUrlDark", "plugin interface")?,
        screenshots: path_list(object, "screenshots", "plugin interface")?,
        screenshot_urls: defaulted_string_list(object, "screenshotUrls", "plugin interface")?,
        website_url: optional_string(object, "websiteUrl", "plugin interface")?,
        privacy_policy_url: optional_string(object, "privacyPolicyUrl", "plugin interface")?,
        terms_of_service_url: optional_string(object, "termsOfServiceUrl", "plugin interface")?,
        default_prompt: optional_string_list(object, "defaultPrompt", "plugin interface")?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn share_principal(value: &Value) -> Result<AgentPluginSharePrincipal> {
    const KNOWN: &[&str] = &["name", "principalId", "principalType", "role"];
    let object = as_object(value, "share principal")?;
    Ok(AgentPluginSharePrincipal {
        name: required_string(object, "name", "share principal")?,
        principal_id: required_string(object, "principalId", "share principal")?,
        principal_type: required_enum(
            object,
            "principalType",
            "share principal",
            AgentPluginSharePrincipalType::parse,
        )?,
        role: required_enum(
            object,
            "role",
            "share principal",
            AgentPluginShareRole::parse,
        )?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn share_context(value: &Value) -> Result<AgentPluginShareContext> {
    const KNOWN: &[&str] = &[
        "remotePluginId",
        "remoteVersion",
        "creatorAccountUserId",
        "creatorName",
        "canPublishToWorkspace",
        "discoverability",
        "sharePrincipals",
        "shareUrl",
    ];
    let object = as_object(value, "plugin shareContext")?;
    Ok(AgentPluginShareContext {
        remote_plugin_id: required_string(object, "remotePluginId", "shareContext")?,
        remote_version: optional_string(object, "remoteVersion", "shareContext")?,
        creator_account_user_id: optional_string(object, "creatorAccountUserId", "shareContext")?,
        creator_name: optional_string(object, "creatorName", "shareContext")?,
        can_publish_to_workspace: optional_bool(object, "canPublishToWorkspace", "shareContext")?,
        discoverability: optional_enum(
            object,
            "discoverability",
            "shareContext",
            AgentPluginShareDiscoverability::parse,
        )?,
        share_principals: match object.get("sharePrincipals") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                array(value, "shareContext 的 sharePrincipals")?
                    .iter()
                    .map(share_principal)
                    .collect::<Result<Vec<_>>>()?,
            ),
        },
        share_url: optional_string(object, "shareUrl", "shareContext")?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn plugin_summary(value: &Value) -> Result<AgentPluginSummary> {
    const KNOWN: &[&str] = &[
        "id",
        "name",
        "version",
        "localVersion",
        "enabled",
        "installed",
        "installedAt",
        "installPolicy",
        "installPolicySource",
        "authPolicy",
        "availability",
        "disabledReason",
        "eligiblePlanTypes",
        "interface",
        "keywords",
        "mustShowInstallationInterstitial",
        "remotePluginId",
        "shareContext",
        "source",
    ];
    let object = as_object(value, "plugin summary")?;
    Ok(AgentPluginSummary {
        id: required_string(object, "id", "plugin summary")?,
        name: required_string(object, "name", "plugin summary")?,
        version: optional_string(object, "version", "plugin summary")?,
        local_version: optional_string(object, "localVersion", "plugin summary")?,
        enabled: required_bool(object, "enabled", "plugin summary")?,
        installed: required_bool(object, "installed", "plugin summary")?,
        installed_at: optional_i64(object, "installedAt", "plugin summary")?,
        install_policy: required_enum(
            object,
            "installPolicy",
            "plugin summary",
            AgentPluginInstallPolicy::parse,
        )?,
        install_policy_source: optional_enum(
            object,
            "installPolicySource",
            "plugin summary",
            AgentPluginInstallPolicySource::parse,
        )?,
        auth_policy: required_enum(
            object,
            "authPolicy",
            "plugin summary",
            AgentPluginAuthPolicy::parse,
        )?,
        availability: optional_enum(
            object,
            "availability",
            "plugin summary",
            AgentPluginAvailability::parse,
        )?,
        disabled_reason: optional_enum(
            object,
            "disabledReason",
            "plugin summary",
            AgentPluginDisabledReason::parse,
        )?,
        eligible_plan_types: optional_string_list(object, "eligiblePlanTypes", "plugin summary")?,
        interface: match object.get("interface") {
            None | Some(Value::Null) => None,
            Some(value) => Some(plugin_interface(value)?),
        },
        keywords: optional_string_list(object, "keywords", "plugin summary")?,
        must_show_installation_interstitial: optional_bool(
            object,
            "mustShowInstallationInterstitial",
            "plugin summary",
        )?,
        remote_plugin_id: optional_string(object, "remotePluginId", "plugin summary")?,
        share_context: match object.get("shareContext") {
            None | Some(Value::Null) => None,
            Some(value) => Some(share_context(value)?),
        },
        source: plugin_source(
            object
                .get("source")
                .with_context(|| format!("{METHOD_LIST} 的 plugin 缺少 source"))?,
        )?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn marketplace(value: &Value) -> Result<AgentPluginMarketplace> {
    const KNOWN: &[&str] = &["name", "path", "interface", "plugins"];
    let object = as_object(value, "plugin marketplace")?;
    let plugins = array_or_empty(object, "plugins", "marketplace 的 plugins")?;
    Ok(AgentPluginMarketplace {
        name: required_string(object, "name", "marketplace")?,
        path: optional_string(object, "path", "marketplace")?,
        interface: match object.get("interface") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let interface = as_object(value, "marketplace interface")?;
                Some(AgentMarketplaceInterface {
                    display_name: optional_string(
                        interface,
                        "displayName",
                        "marketplace interface",
                    )?,
                    extra: unknown_fields(interface, &["displayName"]),
                })
            }
        },
        plugins: plugins
            .iter()
            .map(plugin_summary)
            .collect::<Result<Vec<_>>>()?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn marketplace_load_error(value: &Value) -> Result<AgentMarketplaceLoadError> {
    const KNOWN: &[&str] = &["marketplacePath", "message"];
    let object = as_object(value, "marketplaceLoadError")?;
    Ok(AgentMarketplaceLoadError {
        marketplace_path: required_path(object, "marketplacePath", "marketplaceLoadError")?,
        message: required_string(object, "message", "marketplaceLoadError")?,
        extra: unknown_fields(object, KNOWN),
    })
}

/// `plugin/list` and `plugin/installed` answer with the same shape.
pub(super) fn decode_catalog(generation: u64, value: &Value) -> Result<AgentPluginCatalog> {
    let object = as_object(value, "plugin catalog 响应")?;
    let marketplaces = array_or_empty(object, "marketplaces", "plugin catalog 的 marketplaces")?;
    let load_errors = match object.get("marketplaceLoadErrors") {
        None | Some(Value::Null) => Vec::new(),
        Some(value) => array(value, "plugin catalog 的 marketplaceLoadErrors")?
            .iter()
            .map(marketplace_load_error)
            .collect::<Result<Vec<_>>>()?,
    };
    Ok(AgentPluginCatalog {
        generation,
        marketplaces: marketplaces
            .iter()
            .map(marketplace)
            .collect::<Result<Vec<_>>>()?,
        featured_plugin_ids: defaulted_string_list(object, "featuredPluginIds", "plugin catalog")?,
        marketplace_load_errors: load_errors,
        extra: unknown_fields(
            object,
            &["marketplaces", "featuredPluginIds", "marketplaceLoadErrors"],
        ),
    })
}

fn app_summary(value: &Value) -> Result<AgentPluginAppSummary> {
    const KNOWN: &[&str] = &["id", "name", "description", "category", "installUrl"];
    let object = as_object(value, "plugin app")?;
    Ok(AgentPluginAppSummary {
        id: required_string(object, "id", "plugin app")?,
        name: required_string(object, "name", "plugin app")?,
        description: optional_string(object, "description", "plugin app")?,
        category: optional_string(object, "category", "plugin app")?,
        install_url: optional_string(object, "installUrl", "plugin app")?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn app_template_summary(value: &Value) -> Result<AgentPluginAppTemplateSummary> {
    const KNOWN: &[&str] = &[
        "templateId",
        "name",
        "description",
        "canonicalConnectorId",
        "category",
        "logoUrl",
        "logoUrlDark",
        "materializedAppIds",
        "reason",
    ];
    let object = as_object(value, "plugin appTemplate")?;
    let reason = match optional_string(object, "reason", "appTemplate")? {
        None => None,
        Some(raw) => Some(
            AgentAppTemplateUnavailableReason::parse(&raw)
                .with_context(|| format!("appTemplate 的 reason 为未知值 `{raw}`"))?,
        ),
    };
    Ok(AgentPluginAppTemplateSummary {
        template_id: required_string(object, "templateId", "appTemplate")?,
        name: required_string(object, "name", "appTemplate")?,
        description: optional_string(object, "description", "appTemplate")?,
        canonical_connector_id: optional_string(object, "canonicalConnectorId", "appTemplate")?,
        category: optional_string(object, "category", "appTemplate")?,
        logo_url: optional_string(object, "logoUrl", "appTemplate")?,
        logo_url_dark: optional_string(object, "logoUrlDark", "appTemplate")?,
        materialized_app_ids: defaulted_string_list(object, "materializedAppIds", "appTemplate")?,
        reason,
        extra: unknown_fields(object, KNOWN),
    })
}

fn hook_summary(value: &Value) -> Result<AgentPluginHookSummary> {
    const KNOWN: &[&str] = &["key", "eventName"];
    let object = as_object(value, "plugin hook")?;
    Ok(AgentPluginHookSummary {
        key: required_string(object, "key", "plugin hook")?,
        event_name: required_string(object, "eventName", "plugin hook")?,
        extra: unknown_fields(object, KNOWN),
    })
}

fn skill_summary(value: &Value) -> Result<AgentPluginSkillSummary> {
    const KNOWN: &[&str] = &[
        "name",
        "description",
        "shortDescription",
        "enabled",
        "path",
        "interface",
    ];
    let object = as_object(value, "plugin skill")?;
    Ok(AgentPluginSkillSummary {
        name: required_string(object, "name", "plugin skill")?,
        description: required_string(object, "description", "plugin skill")?,
        short_description: optional_string(object, "shortDescription", "plugin skill")?,
        enabled: required_bool(object, "enabled", "plugin skill")?,
        path: optional_string(object, "path", "plugin skill")?,
        interface: optional_value(object, "interface"),
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_detail(value: &Value) -> Result<AgentPluginDetail> {
    const KNOWN: &[&str] = &[
        "summary",
        "description",
        "marketplaceName",
        "marketplacePath",
        "apps",
        "appTemplates",
        "hooks",
        "mcpServers",
        "scheduledTasks",
        "shareUrl",
        "skills",
    ];
    let root = as_object(value, &format!("{METHOD_READ} 响应"))?;
    let object = as_object(
        root.get("plugin").context("plugin/read 响应缺少 plugin")?,
        "plugin/read 的 plugin",
    )?;
    let apps = array_or_empty(object, "apps", "plugin 的 apps")?;
    let app_templates = array_or_empty(object, "appTemplates", "plugin 的 appTemplates")?;
    let hooks = array_or_empty(object, "hooks", "plugin 的 hooks")?;
    let skills = array_or_empty(object, "skills", "plugin 的 skills")?;
    Ok(AgentPluginDetail {
        summary: plugin_summary(object.get("summary").context("plugin/read 缺少 summary")?)?,
        description: optional_string(object, "description", "plugin")?,
        marketplace_name: required_string(object, "marketplaceName", "plugin")?,
        marketplace_path: optional_string(object, "marketplacePath", "plugin")?,
        apps: apps.iter().map(app_summary).collect::<Result<Vec<_>>>()?,
        app_templates: app_templates
            .iter()
            .map(app_template_summary)
            .collect::<Result<Vec<_>>>()?,
        hooks: hooks.iter().map(hook_summary).collect::<Result<Vec<_>>>()?,
        mcp_servers: defaulted_string_list(object, "mcpServers", "plugin")?,
        scheduled_tasks: optional_value(object, "scheduledTasks"),
        share_url: optional_string(object, "shareUrl", "plugin")?,
        skills: skills
            .iter()
            .map(skill_summary)
            .collect::<Result<Vec<_>>>()?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_search_page(
    generation: u64,
    cursor: Option<String>,
    search_term: String,
    value: &Value,
) -> Result<AgentPluginSearchPage> {
    let object = as_object(value, &format!("{METHOD_SEARCH} 响应"))?;
    let data = array_or_empty(object, "data", "plugin/search 响应的 data")?;
    let mut results = Vec::with_capacity(data.len());
    for value in data {
        let entry = as_object(value, "plugin/search 结果")?;
        results.push(AgentPluginSearchResult {
            marketplace_name: required_string(entry, "marketplaceName", "search result")?,
            marketplace_path: optional_string(entry, "marketplacePath", "search result")?,
            plugin: plugin_summary(entry.get("plugin").context("search result 缺少 plugin")?)?,
        });
    }
    Ok(AgentPluginSearchPage {
        generation,
        cursor,
        search_term,
        results,
        next_cursor: optional_string(object, "nextCursor", METHOD_SEARCH)?,
        extra: unknown_fields(object, &["data", "nextCursor"]),
    })
}

pub(super) fn decode_install_receipt(value: &Value) -> Result<AgentPluginInstallReceipt> {
    let object = as_object(value, &format!("{METHOD_INSTALL} 响应"))?;
    let apps = array_or_empty(object, "appsNeedingAuth", "install 响应的 appsNeedingAuth")?;
    Ok(AgentPluginInstallReceipt {
        auth_policy: required_enum(
            object,
            "authPolicy",
            "plugin/install 响应",
            AgentPluginAuthPolicy::parse,
        )?,
        apps_needing_auth: apps.iter().map(app_summary).collect::<Result<Vec<_>>>()?,
    })
}

fn reconcile_changed_plugin(value: &Value) -> Result<AgentPluginReconcileChangedPlugin> {
    const KNOWN: &[&str] = &["id", "hasApps", "hasHooks", "hasMcps", "hasSkills"];
    let object = as_object(value, "plugin/reconcile changedPlugin")?;
    Ok(AgentPluginReconcileChangedPlugin {
        id: required_string(object, "id", "changedPlugin")?,
        has_apps: required_bool(object, "hasApps", "changedPlugin")?,
        has_hooks: required_bool(object, "hasHooks", "changedPlugin")?,
        has_mcps: required_bool(object, "hasMcps", "changedPlugin")?,
        has_skills: required_bool(object, "hasSkills", "changedPlugin")?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_reconcile_receipt(
    generation: u64,
    value: &Value,
) -> Result<AgentPluginReconcileReceipt> {
    let object = as_object(value, &format!("{METHOD_RECONCILE} 响应"))?;
    let changed = array_or_empty(object, "changedPlugins", "reconcile 响应的 changedPlugins")?;
    Ok(AgentPluginReconcileReceipt {
        generation,
        changed_plugins: changed
            .iter()
            .map(reconcile_changed_plugin)
            .collect::<Result<Vec<_>>>()?,
        failed_remote_plugin_ids: defaulted_string_list(
            object,
            "failedRemotePluginIds",
            METHOD_RECONCILE,
        )?,
        failed_materialization_remote_plugin_ids: defaulted_string_list(
            object,
            "failedMaterializationRemotePluginIds",
            METHOD_RECONCILE,
        )?,
        extra: unknown_fields(
            object,
            &[
                "changedPlugins",
                "failedRemotePluginIds",
                "failedMaterializationRemotePluginIds",
            ],
        ),
    })
}

pub(super) fn decode_share_save_receipt(value: &Value) -> Result<AgentPluginShareSaveReceipt> {
    const KNOWN: &[&str] = &["remotePluginId", "shareUrl", "canPublishToWorkspace"];
    let object = as_object(value, &format!("{METHOD_SHARE_SAVE} 响应"))?;
    Ok(AgentPluginShareSaveReceipt {
        remote_plugin_id: required_string(object, "remotePluginId", "share save 响应")?,
        share_url: required_string(object, "shareUrl", "share save 响应")?,
        can_publish_to_workspace: optional_bool(
            object,
            "canPublishToWorkspace",
            "share save 响应",
        )?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_share_update_targets_receipt(
    value: &Value,
) -> Result<AgentPluginShareUpdateTargetsReceipt> {
    let object = as_object(value, &format!("{METHOD_SHARE_UPDATE_TARGETS} 响应"))?;
    let principals = array_or_empty(
        object,
        "principals",
        "share updateTargets 响应的 principals",
    )?;
    Ok(AgentPluginShareUpdateTargetsReceipt {
        discoverability: required_enum(
            object,
            "discoverability",
            "share updateTargets 响应",
            AgentPluginShareDiscoverability::parse,
        )?,
        principals: principals
            .iter()
            .map(share_principal)
            .collect::<Result<Vec<_>>>()?,
        extra: unknown_fields(object, &["discoverability", "principals"]),
    })
}

pub(super) fn decode_share_list(generation: u64, value: &Value) -> Result<AgentPluginShareList> {
    let object = as_object(value, &format!("{METHOD_SHARE_LIST} 响应"))?;
    let data = array_or_empty(object, "data", "plugin/share/list 响应的 data")?;
    let mut entries = Vec::with_capacity(data.len());
    for value in data {
        const KNOWN: &[&str] = &["plugin", "localPluginPath"];
        let entry = as_object(value, "plugin/share/list 条目")?;
        entries.push(AgentPluginShareListEntry {
            plugin: plugin_summary(entry.get("plugin").context("share list 条目缺少 plugin")?)?,
            local_plugin_path: optional_string(entry, "localPluginPath", "share list 条目")?,
            extra: unknown_fields(entry, KNOWN),
        });
    }
    Ok(AgentPluginShareList {
        generation,
        entries,
        extra: unknown_fields(object, &["data"]),
    })
}

pub(super) fn decode_skill_content(
    generation: u64,
    value: &Value,
) -> Result<AgentPluginSkillContent> {
    let object = as_object(value, &format!("{METHOD_SKILL_READ} 响应"))?;
    Ok(AgentPluginSkillContent {
        generation,
        contents: optional_string(object, "contents", METHOD_SKILL_READ)?,
    })
}

pub(super) fn decode_marketplace_add(value: &Value) -> Result<AgentMarketplaceAddReceipt> {
    const KNOWN: &[&str] = &["marketplaceName", "installedRoot", "alreadyAdded"];
    let object = as_object(value, &format!("{METHOD_MARKETPLACE_ADD} 响应"))?;
    Ok(AgentMarketplaceAddReceipt {
        marketplace_name: required_string(object, "marketplaceName", "marketplace/add 响应")?,
        installed_root: required_path(object, "installedRoot", "marketplace/add 响应")?,
        already_added: required_bool(object, "alreadyAdded", "marketplace/add 响应")?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_marketplace_remove(value: &Value) -> Result<AgentMarketplaceRemoveReceipt> {
    const KNOWN: &[&str] = &["marketplaceName", "installedRoot"];
    let object = as_object(value, &format!("{METHOD_MARKETPLACE_REMOVE} 响应"))?;
    Ok(AgentMarketplaceRemoveReceipt {
        marketplace_name: required_string(object, "marketplaceName", "marketplace/remove 响应")?,
        installed_root: optional_string(object, "installedRoot", "marketplace/remove 响应")?,
        extra: unknown_fields(object, KNOWN),
    })
}

pub(super) fn decode_marketplace_upgrade(value: &Value) -> Result<AgentMarketplaceUpgradeReceipt> {
    let object = as_object(value, &format!("{METHOD_MARKETPLACE_UPGRADE} 响应"))?;
    let errors = array_or_empty(object, "errors", "marketplace/upgrade 响应的 errors")?;
    let mut parsed_errors = Vec::with_capacity(errors.len());
    for value in errors {
        const KNOWN: &[&str] = &["marketplaceName", "message"];
        let entry = as_object(value, "marketplace/upgrade error")?;
        parsed_errors.push(AgentMarketplaceUpgradeError {
            marketplace_name: required_string(entry, "marketplaceName", "upgrade error")?,
            message: required_string(entry, "message", "upgrade error")?,
            extra: unknown_fields(entry, KNOWN),
        });
    }
    Ok(AgentMarketplaceUpgradeReceipt {
        selected_marketplaces: defaulted_string_list(
            object,
            "selectedMarketplaces",
            METHOD_MARKETPLACE_UPGRADE,
        )?,
        upgraded_roots: path_list(object, "upgradedRoots", METHOD_MARKETPLACE_UPGRADE)?,
        errors: parsed_errors,
        extra: unknown_fields(object, &["selectedMarketplaces", "upgradedRoots", "errors"]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{
        AgentMarketplaceUpgradeRequest, AgentPluginCatalogRequest, AgentPluginInstallRequest,
        AgentPluginMarketplaceKind, AgentPluginReadRequest, AgentPluginSearchRequest,
        AgentPluginShareDeleteRequest, AgentPluginShareSaveRequest,
    };
    use serde_json::json;

    #[test]
    fn catalog_and_read_params_follow_the_schema() {
        assert_eq!(
            list_params(&AgentPluginCatalogRequest::default()),
            json!({})
        );
        assert_eq!(
            list_params(&AgentPluginCatalogRequest {
                cwds: Some(vec!["/tmp/project".into()]),
                force_refetch: true,
                marketplace_kinds: Some(vec![
                    AgentPluginMarketplaceKind::Local,
                    AgentPluginMarketplaceKind::CreatedByMeRemote,
                ]),
            }),
            json!({
                "cwds": ["/tmp/project"],
                "forceRefetch": true,
                "marketplaceKinds": ["local", "created-by-me-remote"]
            })
        );
        // Both selectors are always present; the unused one is an explicit null.
        assert_eq!(
            read_params(&AgentPluginReadRequest {
                marketplace_path: Some("/marketplace.json".into()),
                remote_marketplace_name: None,
                plugin_name: "documents".into(),
            }),
            json!({
                "pluginName": "documents",
                "marketplacePath": "/marketplace.json",
                "remoteMarketplaceName": null
            })
        );
        assert_eq!(
            install_params(&AgentPluginInstallRequest {
                generation: 1,
                plugin_name: "documents".into(),
                marketplace_path: None,
                remote_marketplace_name: Some("codex-official".into()),
                install_attempt_id: Some("attempt-1".into()),
            }),
            json!({
                "pluginName": "documents",
                "marketplacePath": null,
                "remoteMarketplaceName": "codex-official",
                "installAttemptId": "attempt-1"
            })
        );
        assert_eq!(
            search_params(&AgentPluginSearchRequest {
                cursor: Some("c1".into()),
                limit: Some(10),
                scope: Some("global"),
                search_term: "doc".into(),
                cwds: None,
            }),
            json!({"searchTerm": "doc", "cursor": "c1", "limit": 10, "scope": "global"})
        );
        assert_eq!(
            share_save_params(&AgentPluginShareSaveRequest {
                generation: 1,
                plugin_path: "/plugins/doc".into(),
                remote_plugin_id: Some("remote-1".into()),
                discoverability: Some(crate::agent::AgentPluginShareDiscoverability::Unlisted),
                share_targets: Some(vec![crate::agent::AgentPluginShareTarget {
                    principal_id: "u1".into(),
                    principal_type: crate::agent::AgentPluginSharePrincipalType::User,
                    role: crate::agent::AgentPluginShareRole::Editor,
                    extra: Default::default(),
                }]),
            }),
            json!({
                "pluginPath": "/plugins/doc",
                "remotePluginId": "remote-1",
                "discoverability": "UNLISTED",
                "shareTargets": [{"principalId": "u1", "principalType": "user", "role": "editor"}]
            })
        );
        assert_eq!(
            share_delete_params(&AgentPluginShareDeleteRequest {
                generation: 1,
                remote_plugin_id: "remote-1".into(),
            }),
            json!({"remotePluginId": "remote-1"})
        );
        // An upgrade without a name selects every marketplace server-side.
        assert_eq!(
            marketplace_upgrade_params(&AgentMarketplaceUpgradeRequest {
                generation: 1,
                marketplace_name: None,
            }),
            json!({})
        );
    }

    #[test]
    fn catalog_decode_keeps_nulls_unknown_fields_and_source_variants() {
        let catalog = decode_catalog(
            3,
            &json!({
                "marketplaces": [{
                    "name": "openai-bundled",
                    "path": null,
                    "interface": {"displayName": "OpenAI Bundled", "future": 1},
                    "plugins": [{
                        "id": "chrome@openai-bundled",
                        "name": "chrome",
                        "installed": true,
                        "enabled": true,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "availability": "ENABLED",
                        "disabledReason": null,
                        "source": {"type": "git", "url": "https://example.invalid/plugin.git"},
                        "extension": {"tier": "beta"}
                    }]
                }],
                "featuredPluginIds": ["chrome@openai-bundled"],
                "catalogExtension": true
            }),
        )
        .unwrap();
        let marketplace = &catalog.marketplaces[0];
        assert_eq!(marketplace.display_name(), "OpenAI Bundled");
        assert_eq!(
            marketplace.interface.as_ref().unwrap().extra["future"],
            json!(1)
        );
        let plugin = &marketplace.plugins[0];
        // The upstream alias for an available plugin is accepted verbatim.
        assert_eq!(
            plugin.availability,
            Some(crate::agent::AgentPluginAvailability::Available)
        );
        assert_eq!(plugin.disabled_reason, None);
        assert_eq!(plugin.source.kind(), "git");
        assert_eq!(plugin.extra["extension"]["tier"], json!("beta"));
        assert_eq!(catalog.extra["catalogExtension"], json!(true));
    }

    #[test]
    fn unknown_enum_values_and_unreadable_sources_are_protocol_errors() {
        let base = json!({
            "marketplaces": [{
                "name": "m",
                "plugins": [{
                    "id": "p@m", "name": "p", "installed": false, "enabled": false,
                    "installPolicy": "AVAILABLE", "authPolicy": "ON_USE",
                    "source": {"type": "local", "path": "/plugins/p"}
                }]
            }]
        });
        assert!(decode_catalog(1, &base).is_ok());
        let mut bad_policy = base.clone();
        bad_policy["marketplaces"][0]["plugins"][0]["installPolicy"] = json!("SOMETIMES");
        assert!(decode_catalog(1, &bad_policy).is_err());
        let mut bad_source = base.clone();
        bad_source["marketplaces"][0]["plugins"][0]["source"] = json!({"type": "tarball"});
        assert!(decode_catalog(1, &bad_source).is_err());
        let mut relative_path = base.clone();
        relative_path["marketplaces"][0]["plugins"][0]["source"] =
            json!({"type": "local", "path": "plugins/p"});
        assert!(decode_catalog(1, &relative_path).is_err());
    }
}
