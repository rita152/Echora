//! Experimental feature flags (`experimentalFeature/list`) and memory
//! settings. Feature state is read from the server; a change is a user config
//! write of `features.<name>` that the running app-server applies only after
//! a new connection loads it.

use serde_json::Value;

use super::AgentConfigEdit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentExperimentalFeatureStage {
    Beta,
    UnderDevelopment,
    Stable,
    Deprecated,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExperimentalFeature {
    /// Stable key used in config.toml.
    pub name: String,
    pub stage: AgentExperimentalFeatureStage,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub announcement: Option<String>,
    pub enabled: bool,
    pub default_enabled: bool,
}

/// Beta flags the reference hides from its settings list because another page
/// owns them (memories, plugins, remote control…) or they are not user-facing.
const SETTINGS_EXCLUDED: [&str; 7] = [
    "memories",
    "multi_agent",
    "plugins",
    "plugin",
    "remote_control",
    "chronicle",
    "workspace_dependencies",
];

impl AgentExperimentalFeature {
    /// Shown in "Experimental features (Beta)": beta flags other pages do not own.
    pub fn listed_in_settings(&self) -> bool {
        self.stage == AgentExperimentalFeatureStage::Beta
            && !SETTINGS_EXCLUDED.contains(&self.name.as_str())
            && !self.name.starts_with("realtime_")
    }

    pub fn label(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }

    pub fn edit(&self, enabled: bool) -> AgentConfigEdit {
        feature_edit(&self.name, enabled)
    }
}

pub(super) fn feature_edit(name: &str, enabled: bool) -> AgentConfigEdit {
    AgentConfigEdit {
        key: format!("features.{name}"),
        value: Value::Bool(enabled),
    }
}

/// Every page of the list for one generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExperimentalFeatures {
    pub generation: u64,
    pub features: Vec<AgentExperimentalFeature>,
}

impl AgentExperimentalFeatures {
    pub fn get(&self, name: &str) -> Option<&AgentExperimentalFeature> {
        self.features.iter().find(|feature| feature.name == name)
    }
}

/// `thread/memoryMode/set` mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentThreadMemoryMode {
    Enabled,
    Disabled,
}

/// The two switches of a chat. A new chat sends them as `thread/start`
/// config overrides; a started chat can only change generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentMemoryPreferences {
    pub use_memories: bool,
    pub generate_memories: bool,
}

/// `memories.*` in the effective configuration, with the defaults the
/// reference applies when a key is absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentMemoryConfig {
    pub generate_memories: bool,
    pub use_memories: bool,
    /// `disable_on_external_context`, falling back to the older
    /// `no_memories_if_mcp_or_web_search`.
    pub disable_on_external_context: bool,
}

impl AgentMemoryConfig {
    pub fn from_effective(effective: &Value) -> Self {
        let memories = effective.get("memories");
        let flag = |key: &str| {
            memories
                .and_then(|table| table.get(key))
                .and_then(Value::as_bool)
        };
        Self {
            generate_memories: flag("generate_memories").unwrap_or(true),
            use_memories: flag("use_memories").unwrap_or(true),
            disable_on_external_context: flag("disable_on_external_context")
                .or_else(|| flag("no_memories_if_mcp_or_web_search"))
                .unwrap_or(false),
        }
    }

    pub fn preferences(self) -> AgentMemoryPreferences {
        AgentMemoryPreferences {
            use_memories: self.use_memories,
            generate_memories: self.generate_memories,
        }
    }
}

/// "Enable Codex memories": the feature flag and both memory switches.
pub fn memory_enable_edits(enabled: bool) -> Vec<AgentConfigEdit> {
    vec![
        feature_edit("memories", enabled),
        AgentConfigEdit {
            key: "memories.generate_memories".into(),
            value: Value::Bool(enabled),
        },
        AgentConfigEdit {
            key: "memories.use_memories".into(),
            value: Value::Bool(enabled),
        },
    ]
}

/// "Allow memories from tool-assisted chats". The older key is removed so it
/// cannot contradict the new one, as the reference does.
pub fn memory_tool_assisted_edits(allow: bool) -> Vec<AgentConfigEdit> {
    vec![
        AgentConfigEdit {
            key: "memories.disable_on_external_context".into(),
            value: Value::Bool(!allow),
        },
        AgentConfigEdit {
            key: "memories.no_memories_if_mcp_or_web_search".into(),
            value: Value::Null,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn memory_config_defaults_and_legacy_key() {
        assert_eq!(
            AgentMemoryConfig::from_effective(&json!({})),
            AgentMemoryConfig {
                generate_memories: true,
                use_memories: true,
                disable_on_external_context: false
            }
        );
        let legacy = json!({"memories": {"generate_memories": false, "no_memories_if_mcp_or_web_search": true}});
        let config = AgentMemoryConfig::from_effective(&legacy);
        assert!(
            !config.generate_memories && config.use_memories && config.disable_on_external_context
        );
        let both = json!({"memories": {"disable_on_external_context": false, "no_memories_if_mcp_or_web_search": true}});
        assert!(!AgentMemoryConfig::from_effective(&both).disable_on_external_context);
    }

    #[test]
    fn settings_list_keeps_only_unowned_beta_flags() {
        let feature = |name: &str, stage| AgentExperimentalFeature {
            name: name.into(),
            stage,
            display_name: None,
            description: None,
            announcement: None,
            enabled: false,
            default_enabled: false,
        };
        use AgentExperimentalFeatureStage::*;
        assert!(feature("network_proxy", Beta).listed_in_settings());
        assert!(!feature("network_proxy", Stable).listed_in_settings());
        assert!(!feature("memories", Beta).listed_in_settings());
        assert!(!feature("realtime_voice", Beta).listed_in_settings());
        assert_eq!(feature("x", Beta).label(), "x");
        assert_eq!(
            feature("prevent_idle_sleep", Beta).edit(true).key,
            "features.prevent_idle_sleep"
        );
    }
}
