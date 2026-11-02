//! Model catalog selection and effective model lookup.

use super::state::ConversationState;
use crate::agent::{AgentModel, AgentModelCatalog};

/// The power steps ChatGPT offers for a configured model the catalog does not
/// list: it labels such a model "Custom" and still lets the effort change.
pub(crate) const FALLBACK_REASONING_EFFORTS: [&str; 6] =
    ["minimal", "low", "medium", "high", "xhigh", "max"];

/// Effort assumed for a model outside the catalog when nothing names one.
const FALLBACK_DEFAULT_EFFORT: &str = "medium";

impl ConversationState {
    pub(crate) fn apply_config_defaults(&mut self, config: &crate::agent::AgentConfigSnapshot) {
        if self.thread_id.is_some() || self.model_user_selected || self.user_message.is_some() {
            return;
        }
        let requirements = config.requirements.as_ref();
        let string = |key: &str| {
            requirements
                .and_then(|requirements| requirements.enforced.get(key))
                .filter(|value| !value.is_null())
                .or_else(|| config.effective.get(key))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        let configured_model = string("model");
        self.model_explicit = configured_model.is_some();
        self.selected_model = configured_model
            .or_else(|| {
                self.models
                    .iter()
                    .find(|model| model.is_default)
                    .or_else(|| self.models.first())
                    .map(|model| model.model.clone())
            })
            .unwrap_or_default();
        self.selected_effort = self.resolve_effort_for_selection(string("model_reasoning_effort"));
        self.selected_service_tier = string("service_tier");
        self.plan_default_effort = config
            .effective
            .get("plan_mode_reasoning_effort")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        self.sync_slider_index();
    }

    pub(crate) fn apply_model_catalog(&mut self, catalog: AgentModelCatalog) {
        let previous_model = self.selected_model.clone();
        let previous_effort = self.selected_effort.clone();
        let previous_service_tier = self.selected_service_tier.clone();

        self.models = catalog.models;
        self.model_catalog_error = None;
        if self.models.is_empty() {
            self.set_model_catalog_error(crate::i18n::text("Codex 未返回可用模型").to_owned());
            return;
        }

        if let Some(index) = self
            .models
            .iter()
            .position(|model| model.model == previous_model || model.id == previous_model)
        {
            self.apply_model_selection(index, Some(previous_effort), previous_service_tier, true);
        } else if !previous_model.is_empty() {
            // A configured or resumed model the catalog does not list stays
            // selected, as ChatGPT keeps it ("Custom") instead of silently
            // switching the request to another model.
            if self.selected_effort.is_empty() {
                self.selected_effort = self.default_effort_for_selection();
            }
            self.sync_slider_index();
        } else {
            let index = self
                .models
                .iter()
                .position(|model| model.is_default)
                .unwrap_or(0);
            self.apply_model_selection(index, None, None, false);
        }
    }

    /// Reasoning efforts the slider offers for the current selection: the
    /// catalog entry's own list, or ChatGPT's fallback steps for a model the
    /// catalog does not list.
    pub(crate) fn effort_ids(&self) -> Vec<String> {
        match self.selected_model_entry() {
            Some(model) => model
                .supported_reasoning_efforts
                .iter()
                .map(|effort| effort.id.clone())
                .collect(),
            None if !self.selected_model.is_empty() => FALLBACK_REASONING_EFFORTS
                .iter()
                .map(|effort| (*effort).to_owned())
                .collect(),
            None => Vec::new(),
        }
    }

    /// `effort_ids().len()` without building the list.
    pub(crate) fn effort_count(&self) -> usize {
        match self.selected_model_entry() {
            Some(model) => model.supported_reasoning_efforts.len(),
            None if !self.selected_model.is_empty() => FALLBACK_REASONING_EFFORTS.len(),
            None => 0,
        }
    }

    pub(crate) fn sync_slider_index(&mut self) {
        self.slider_index = self
            .effort_ids()
            .iter()
            .position(|effort| *effort == self.selected_effort)
            .unwrap_or(0);
    }

    pub(crate) fn default_effort_for_selection(&self) -> String {
        match self.selected_model_entry() {
            Some(model) => model
                .supported_reasoning_efforts
                .iter()
                .find(|effort| effort.id == model.default_reasoning_effort)
                .or_else(|| model.supported_reasoning_efforts.first())
                .map(|effort| effort.id.clone())
                .unwrap_or_else(|| model.default_reasoning_effort.clone()),
            None if !self.selected_model.is_empty() => FALLBACK_DEFAULT_EFFORT.to_owned(),
            None => String::new(),
        }
    }

    /// A catalog model must use one of its advertised efforts. Configuration
    /// and restored settings may name a step that a different model supported.
    /// Keep an unknown model's configured effort until its capabilities arrive.
    pub(crate) fn resolve_effort_for_selection(&self, preferred: Option<String>) -> String {
        preferred
            .filter(|effort| {
                !effort.is_empty()
                    && self.selected_model_entry().is_none_or(|model| {
                        model
                            .supported_reasoning_efforts
                            .iter()
                            .any(|option| option.id == *effort)
                    })
            })
            .unwrap_or_else(|| self.default_effort_for_selection())
    }
    pub(crate) fn set_model_catalog_error(&mut self, error: String) {
        self.models.clear();
        self.model_catalog_error = Some(error);
        // A failed catalog read must not replace an existing thread/config
        // selection. It can still use the configured model and fallback steps.
        self.sync_slider_index();
    }
    pub(crate) fn apply_model_selection(
        &mut self,
        index: usize,
        preferred_effort: Option<String>,
        preferred_service_tier: Option<String>,
        preserve_standard_tier: bool,
    ) {
        let Some(model) = self.models.get(index).cloned() else {
            return;
        };
        self.selected_model = model.model;
        self.selected_effort = self.resolve_effort_for_selection(preferred_effort);

        self.selected_service_tier = if preserve_standard_tier && preferred_service_tier.is_none() {
            None
        } else {
            preferred_service_tier
                .filter(|tier| model.service_tiers.iter().any(|option| option.id == *tier))
                .or_else(|| {
                    model
                        .default_service_tier
                        .filter(|tier| model.service_tiers.iter().any(|option| option.id == *tier))
                })
        };
        self.slider_index = model
            .supported_reasoning_efforts
            .iter()
            .position(|option| option.id == self.selected_effort)
            .unwrap_or(0);
        self.actual_model = None;
        self.model_status = None;
        self.safety_buffering = false;
    }
    pub(crate) fn selected_model_entry(&self) -> Option<&AgentModel> {
        self.models
            .iter()
            .find(|model| model.model == self.selected_model || model.id == self.selected_model)
    }
    pub(crate) fn model_display_name<'a>(&'a self, model_name: &'a str) -> &'a str {
        self.models
            .iter()
            .find(|model| model.model == model_name || model.id == model_name)
            .map(|model| model.display_name.as_str())
            .unwrap_or(model_name)
    }
}

#[cfg(test)]
mod config_default_tests {
    use super::*;
    #[test]
    fn session_static_defaults_apply_to_new_untouched_drafts_only() {
        let mut state = ConversationState::default();
        let mut config = crate::agent::AgentConfigSnapshot {
            generation: 1,
            cwd: "/work".into(),
            effective: serde_json::json!({"model":"model-a","model_reasoning_effort":"ultra","plan_mode_reasoning_effort":"max","service_tier":"priority"}),
            origins: Default::default(),
            layers: None,
            requirements: None,
            value_aliases: Default::default(),
            value_defaults: Default::default(),
            profile_parents: Default::default(),
            required_fields: Default::default(),
        };
        state.apply_config_defaults(&config);
        assert_eq!(state.selected_model, "model-a");
        assert_eq!(state.selected_effort, "ultra");
        assert_eq!(state.plan_default_effort.as_deref(), Some("max"));
        assert_eq!(state.selected_service_tier.as_deref(), Some("priority"));
        state.thread_id = Some("existing".into());
        config.effective["model"] = serde_json::json!("model-b");
        state.apply_config_defaults(&config);
        assert_eq!(state.selected_model, "model-a");
        state.thread_id = None;
        state.model_user_selected = true;
        state.apply_config_defaults(&config);
        assert_eq!(state.selected_model, "model-a");
    }

    #[test]
    fn clearing_config_defaults_restores_catalog_model_and_effort() {
        let model = |name: &str, is_default| AgentModel {
            id: name.into(),
            model: name.into(),
            display_name: name.into(),
            description: String::new(),
            supported_reasoning_efforts: vec![crate::agent::AgentReasoningEffort {
                id: "medium".into(),
                description: String::new(),
            }],
            default_reasoning_effort: "medium".into(),
            service_tiers: Vec::new(),
            default_service_tier: None,
            is_default,
        };
        let mut state = ConversationState::default();
        state.apply_model_catalog(AgentModelCatalog {
            models: vec![model("catalog-default", true), model("configured", false)],
        });
        let mut config = crate::agent::AgentConfigSnapshot {
            generation: 1,
            cwd: "/work".into(),
            effective: serde_json::json!({"model":"configured","model_reasoning_effort":"ultra"}),
            origins: Default::default(),
            layers: None,
            requirements: None,
            value_aliases: Default::default(),
            value_defaults: Default::default(),
            profile_parents: Default::default(),
            required_fields: Default::default(),
        };
        state.apply_config_defaults(&config);
        assert_eq!(state.selected_model, "configured");
        assert_eq!(state.selected_effort, "medium");
        assert_eq!(state.slider_index, 0);
        config.effective = serde_json::json!({"model":null,"model_reasoning_effort":null});
        state.apply_config_defaults(&config);
        assert_eq!(state.selected_model, "catalog-default");
        assert_eq!(state.selected_effort, "medium");
        config.effective = serde_json::json!({"model":"configured"});
        state.apply_config_defaults(&config);
        assert_eq!(state.selected_model, "configured");
        assert_eq!(state.selected_effort, "medium");
    }
}
