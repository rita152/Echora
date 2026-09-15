//! Plugin directory state: one cache of the catalog plus the installed subset,
//! request cycles that discard superseded reads, and the install lifecycle the
//! settings page renders.

use std::collections::BTreeMap;

use crate::agent::{
    AgentPluginCatalog, AgentPluginDetail, AgentPluginSearchPage, AgentPluginsError,
};

/// What the user asked the server to do. The intent is kept verbatim so a
/// failed attempt can be retried without re-deriving it from the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginIntent {
    Install {
        plugin_name: String,
        marketplace_path: Option<String>,
        remote_marketplace_name: Option<String>,
    },
    Uninstall {
        plugin_id: String,
    },
    MarketplaceAdd {
        source: String,
    },
    MarketplaceRemove {
        marketplace_name: String,
    },
    MarketplaceUpgrade {
        marketplace_name: Option<String>,
    },
}

impl PluginIntent {
    /// The row an intent applies to. Operations on different plugins are
    /// tracked separately; the same plugin's newest intent supersedes the
    /// previous one.
    pub fn target(&self) -> String {
        match self {
            Self::Install { plugin_name, .. } => format!("install:{plugin_name}"),
            Self::Uninstall { plugin_id } => format!("uninstall:{plugin_id}"),
            Self::MarketplaceAdd { source } => format!("marketplace-add:{source}"),
            Self::MarketplaceRemove { marketplace_name } => {
                format!("marketplace-remove:{marketplace_name}")
            }
            Self::MarketplaceUpgrade { marketplace_name } => format!(
                "marketplace-upgrade:{}",
                marketplace_name.as_deref().unwrap_or("*")
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginOperationPhase {
    /// The request is on its way.
    Pending,
    /// The server confirmed the operation.
    Succeeded { message: String },
    /// The server refused it. The intent stays so the user can retry.
    Failed {
        message: String,
        outcome_unknown: bool,
    },
    /// No answer inside the operation window. Never retried automatically.
    TimedOut { message: String },
    /// The answer could not be read or the connection was replaced.
    Unknown { message: String },
}

impl PluginOperationPhase {
    pub fn busy(&self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Whether the user can explicitly retry this operation. A timeout or an
    /// unknown result is excluded: the server may already have applied it.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Failed {
                outcome_unknown: false,
                ..
            }
        )
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Pending => None,
            Self::Succeeded { message }
            | Self::Failed { message, .. }
            | Self::TimedOut { message }
            | Self::Unknown { message } => Some(message),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginOperation {
    pub sequence: u64,
    pub intent: PluginIntent,
    pub phase: PluginOperationPhase,
}

#[derive(Default)]
pub struct PluginsDirectory {
    /// Monotonic read cycle; a response for an older cycle is discarded.
    pub cycle: u64,
    pub generation: u64,
    pub catalog: Option<AgentPluginCatalog>,
    /// The installed subset from `plugin/installed`, kept apart from the full
    /// catalog because the two are different reads with different freshness.
    pub installed: Option<AgentPluginCatalog>,
    pub loading: bool,
    pub error: Option<AgentPluginsError>,
    /// True when a change signal arrived after the last successful read.
    pub stale: bool,
    pub detail: Option<AgentPluginDetail>,
    pub detail_error: Option<AgentPluginsError>,
    pub search: Option<AgentPluginSearchPage>,
    pub search_error: Option<AgentPluginsError>,
    pub search_term: String,
    pub searching: bool,
    /// Plugin shares the account reported through `plugin/share/list`.
    pub shares: Option<crate::agent::AgentPluginShareList>,
    pub shares_error: Option<AgentPluginsError>,
    pub shares_loading: bool,
    /// Share link the server returned for the last successful share save.
    pub share_url: Option<String>,
    /// Contents of one skill read through `plugin/skill/read`.
    pub skill: Option<crate::agent::AgentPluginSkillContent>,
    pub skill_error: Option<AgentPluginsError>,
    /// Skill whose contents are being read, as `plugin:skill`.
    pub skill_opening: Option<String>,
    /// Receipt of the startup reconcile for the current connection generation.
    pub reconcile: Option<crate::agent::AgentPluginReconcileReceipt>,
    /// Generation whose startup reconcile already ran.
    pub reconciled_generation: Option<u64>,
    operations: BTreeMap<String, PluginOperation>,
    sequence: u64,
}

impl PluginsDirectory {
    /// Starts a read cycle. The returned cycle must be handed back with the
    /// response so a superseded read cannot overwrite a newer one.
    pub fn begin_load(&mut self, generation: u64) -> u64 {
        self.cycle = self.cycle.wrapping_add(1);
        self.generation = generation;
        self.loading = true;
        self.cycle
    }

    pub fn apply_catalog(&mut self, cycle: u64, catalog: AgentPluginCatalog) -> bool {
        if cycle != self.cycle {
            return false;
        }
        self.loading = false;
        self.error = None;
        self.stale = false;
        self.generation = catalog.generation;
        self.catalog = Some(catalog);
        true
    }

    pub fn apply_installed(&mut self, cycle: u64, catalog: AgentPluginCatalog) -> bool {
        if cycle != self.cycle {
            return false;
        }
        self.generation = catalog.generation;
        self.installed = Some(catalog);
        true
    }

    pub fn apply_error(&mut self, cycle: u64, error: AgentPluginsError) -> bool {
        if cycle != self.cycle {
            return false;
        }
        self.loading = false;
        self.error = Some(error);
        true
    }

    pub fn start_operation(&mut self, intent: PluginIntent) -> u64 {
        self.sequence = self.sequence.wrapping_add(1);
        let sequence = self.sequence;
        self.operations.insert(
            intent.target(),
            PluginOperation {
                sequence,
                intent,
                phase: PluginOperationPhase::Pending,
            },
        );
        sequence
    }

    pub fn operation(&self, target: &str) -> Option<&PluginOperation> {
        self.operations.get(target)
    }

    pub fn operations(&self) -> impl Iterator<Item = &PluginOperation> {
        self.operations.values()
    }

    /// Applies one operation outcome. A late result for a superseded intent is
    /// ignored.
    pub fn apply_operation(
        &mut self,
        target: &str,
        sequence: u64,
        phase: PluginOperationPhase,
    ) -> bool {
        let Some(operation) = self.operations.get_mut(target) else {
            return false;
        };
        if operation.sequence != sequence {
            return false;
        }
        operation.phase = phase;
        true
    }

    /// Drops a finished operation once the directory has been re-read, so the
    /// banner does not outlive the state it described.
    pub fn clear_settled_operations(&mut self) {
        self.operations
            .retain(|_, operation| operation.phase.busy() || operation.phase.retryable());
    }

    pub fn busy(&self) -> bool {
        self.operations.values().any(|op| op.phase.busy())
    }

    pub fn apply_search(&mut self, search: AgentPluginSearchPage) -> bool {
        if search.search_term != self.search_term {
            return false;
        }
        self.searching = false;
        self.search_error = None;
        self.search = Some(search);
        true
    }

    pub fn apply_search_error(&mut self, search_term: &str, error: AgentPluginsError) -> bool {
        if search_term != self.search_term {
            return false;
        }
        self.searching = false;
        self.search_error = Some(error);
        true
    }

    pub fn set_search_term(&mut self, term: String) {
        if self.search_term == term {
            return;
        }
        self.search_term = term;
        // A new term invalidates the previous answer; it is never shown against
        // a query it did not answer.
        self.search = None;
        self.search_error = None;
    }

    pub fn clear_search(&mut self) {
        self.search_term.clear();
        self.search = None;
        self.search_error = None;
        self.searching = false;
    }

    pub fn clear_detail(&mut self) {
        self.detail = None;
        self.detail_error = None;
        self.skill = None;
        self.skill_error = None;
        self.skill_opening = None;
    }

    pub fn begin_skill(&mut self, key: String) {
        self.skill_opening = Some(key);
        self.skill = None;
        self.skill_error = None;
    }

    pub fn apply_skill(
        &mut self,
        key: &str,
        content: crate::agent::AgentPluginSkillContent,
    ) -> bool {
        if self.skill_opening.as_deref() != Some(key) {
            return false;
        }
        self.skill_opening = None;
        self.skill = Some(content);
        true
    }

    pub fn apply_skill_error(&mut self, key: &str, error: AgentPluginsError) -> bool {
        if self.skill_opening.as_deref() != Some(key) {
            return false;
        }
        self.skill_opening = None;
        self.skill_error = Some(error);
        true
    }

    pub fn begin_shares(&mut self) {
        self.shares_loading = true;
        self.shares_error = None;
    }

    pub fn apply_shares(&mut self, shares: crate::agent::AgentPluginShareList) -> bool {
        self.shares_loading = false;
        self.shares_error = None;
        self.shares = Some(shares);
        true
    }

    pub fn apply_shares_error(&mut self, error: AgentPluginsError) -> bool {
        self.shares_loading = false;
        self.shares_error = Some(error);
        true
    }

    /// The rows the directory shows: the search answer while a search is
    /// active, otherwise the full catalog. A search answer keeps its own
    /// marketplace identity, so no lookup is invented for it.
    pub fn visible_plugins(&self) -> Vec<PluginRow<'_>> {
        if !self.search_term.trim().is_empty() {
            return self
                .search
                .as_ref()
                .map(|search| {
                    search
                        .results
                        .iter()
                        .map(|result| PluginRow {
                            marketplace_name: result.marketplace_name.as_str(),
                            marketplace_path: result.marketplace_path.as_deref(),
                            plugin: &result.plugin,
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        self.catalog
            .as_ref()
            .map(|catalog| {
                catalog
                    .plugins()
                    .map(|(marketplace, plugin)| PluginRow {
                        marketplace_name: marketplace.name.as_str(),
                        marketplace_path: marketplace.path.as_deref(),
                        plugin,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The switch value for one plugin: the server's installed flag. A pending
    /// operation shows the value the user just asked for, which the server has
    /// not confirmed yet.
    pub fn displayed_installed(
        &self,
        plugin_name: &str,
        plugin_id: &str,
        server_installed: bool,
    ) -> bool {
        let uninstall = PluginIntent::Uninstall {
            plugin_id: plugin_id.to_owned(),
        };
        if let Some(operation) = self.operation(&uninstall.target())
            && operation.phase.busy()
        {
            return false;
        }
        let install = PluginIntent::Install {
            plugin_name: plugin_name.to_owned(),
            marketplace_path: None,
            remote_marketplace_name: None,
        };
        if let Some(operation) = self.operation(&install.target())
            && operation.phase.busy()
        {
            return true;
        }
        server_installed
    }
}

/// One directory row: the plugin plus the marketplace it was found in.
#[derive(Clone, Copy)]
pub struct PluginRow<'a> {
    pub marketplace_name: &'a str,
    pub marketplace_path: Option<&'a str>,
    pub plugin: &'a crate::agent::AgentPluginSummary,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentPluginSearchPage, AgentPluginsErrorKind};

    fn search_page(term: &str) -> AgentPluginSearchPage {
        AgentPluginSearchPage {
            generation: 1,
            cursor: None,
            search_term: term.to_owned(),
            results: Vec::new(),
            next_cursor: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn a_superseded_read_never_overwrites_a_newer_one() {
        let mut directory = PluginsDirectory::default();
        let stale = directory.begin_load(1);
        let current = directory.begin_load(1);
        assert!(
            !directory.apply_error(
                stale,
                AgentPluginsError {
                    kind: AgentPluginsErrorKind::Connection,
                    message: "old".into(),
                    data: None,
                    outcome_unknown: false,
                }
            ),
            "a response for a superseded cycle is discarded"
        );
        assert!(directory.apply_error(
            current,
            AgentPluginsError {
                kind: AgentPluginsErrorKind::Protocol,
                message: "new".into(),
                data: None,
                outcome_unknown: false,
            }
        ));
        assert_eq!(directory.error.unwrap().message, "new");
    }

    #[test]
    fn a_failure_keeps_the_intent_for_an_explicit_retry_only() {
        let mut directory = PluginsDirectory::default();
        let intent = PluginIntent::Install {
            plugin_name: "documents".into(),
            marketplace_path: None,
            remote_marketplace_name: None,
        };
        let sequence = directory.start_operation(intent.clone());
        let target = intent.target();
        assert!(directory.busy());
        directory.apply_operation(
            &target,
            sequence,
            PluginOperationPhase::Failed {
                message: "no network".into(),
                outcome_unknown: false,
            },
        );
        let operation = directory.operation(&target).unwrap();
        assert!(operation.phase.retryable());
        assert!(!directory.busy());

        // An unconfirmed result is never offered for retry.
        let sequence = directory.start_operation(intent.clone());
        directory.apply_operation(
            &target,
            sequence,
            PluginOperationPhase::TimedOut {
                message: "timeout".into(),
            },
        );
        assert!(!directory.operation(&target).unwrap().phase.retryable());
    }

    #[test]
    fn a_late_result_for_a_superseded_attempt_is_ignored() {
        let mut directory = PluginsDirectory::default();
        let intent = PluginIntent::Uninstall {
            plugin_id: "documents@marketplace".into(),
        };
        let first = directory.start_operation(intent.clone());
        let second = directory.start_operation(intent.clone());
        let target = intent.target();
        assert!(!directory.apply_operation(
            &target,
            first,
            PluginOperationPhase::Succeeded {
                message: "old".into(),
            }
        ));
        assert!(directory.apply_operation(
            &target,
            second,
            PluginOperationPhase::Succeeded {
                message: "new".into(),
            }
        ));
    }

    #[test]
    fn a_search_answer_or_error_for_another_term_is_inert() {
        let mut directory = PluginsDirectory::default();
        directory.set_search_term("doc".into());
        assert!(!directory.apply_search(search_page("pdf")));
        assert!(directory.apply_search(search_page("doc")));

        directory.set_search_term("table".into());
        assert!(!directory.apply_search_error("doc", disconnected_error()));
        assert!(directory.apply_search_error("table", disconnected_error()));
    }

    #[test]
    fn clearing_the_search_returns_to_the_catalog() {
        let mut directory = PluginsDirectory::default();
        directory.set_search_term("doc".into());
        directory.apply_search(search_page("doc"));
        directory.clear_search();
        assert!(directory.search.is_none());
        assert!(directory.visible_plugins().is_empty());
    }

    fn disconnected_error() -> AgentPluginsError {
        AgentPluginsError {
            kind: AgentPluginsErrorKind::Connection,
            message: "closed".into(),
            data: None,
            outcome_unknown: false,
        }
    }
}
