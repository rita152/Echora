//! Import page state: which external agents this client can actually detect,
//! the items the server offered, the selection the user made, and the import
//! that is currently running.

use std::collections::BTreeSet;

use crate::agent::{
    AgentExternalAgentConfigError, AgentExternalAgentDetectResult,
    AgentExternalAgentImportHistories, AgentExternalAgentImportStatus, AgentExternalAgentItemType,
    AgentExternalAgentMigrationItem,
};

/// The sources this client can detect through the app-server API. Claude
/// Cowork is absent on purpose: the reference client detects it with its own
/// local provider adapter, not through an app-server method, so this client has
/// no call that would answer for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportSource {
    ClaudeCode,
    Cursor,
}

impl ImportSource {
    pub const ALL: [Self; 2] = [Self::ClaudeCode, Self::Cursor];

    /// The provider id the protocol uses, sent as `providerId` and recorded in
    /// import history.
    pub fn provider_id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Cursor => "cursor",
        }
    }

    /// `migrationSource` is sent only for the provider whose detection route
    /// depends on it; the reference client sends it for Cursor alone.
    pub fn migration_source(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => None,
            Self::Cursor => Some("cursor"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Cursor => "Cursor",
        }
    }
}

pub struct ImportSourceState {
    pub source: ImportSource,
    pub loading: bool,
    pub result: Option<AgentExternalAgentDetectResult>,
    pub error: Option<AgentExternalAgentConfigError>,
    /// Indices into the detected item list that the server reported and the
    /// user selected. A source starts fully selected, like the reference.
    pub selected: BTreeSet<usize>,
}

impl ImportSourceState {
    pub fn new(source: ImportSource) -> Self {
        Self {
            source,
            loading: false,
            result: None,
            error: None,
            selected: BTreeSet::new(),
        }
    }

    pub fn items(&self) -> &[AgentExternalAgentMigrationItem] {
        self.result
            .as_ref()
            .map(|result| result.items.as_slice())
            .unwrap_or_default()
    }

    /// Sessions and connectors the server reported for this source.
    pub fn session_count(&self) -> i64 {
        let from_items: i64 = self
            .items()
            .iter()
            .filter(|item| item.item_type == AgentExternalAgentItemType::Sessions)
            .filter_map(|item| {
                item.details
                    .as_ref()
                    .and_then(|details| details.get("sessions"))
                    .and_then(|sessions| sessions.as_array())
                    .map(|sessions| sessions.len() as i64)
            })
            .sum();
        let from_connectors: i64 = self
            .result
            .as_ref()
            .and_then(|result| result.connectors.as_ref())
            .map(|connectors| connectors.iter().map(|c| c.session_count).sum())
            .unwrap_or(0);
        from_items + from_connectors
    }

    pub fn selected_items(&self) -> Vec<AgentExternalAgentMigrationItem> {
        self.items()
            .iter()
            .enumerate()
            .filter(|(index, _)| self.selected.contains(index))
            .map(|(_, item)| item.clone())
            .collect()
    }

    pub fn resolved(&self) -> bool {
        !self.loading && (self.result.is_some() || self.error.is_some())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportPhase {
    /// The import request is on its way.
    Requesting,
    /// The server accepted the import and reports progress by notification.
    Running { import_id: String },
    /// The server reported the import completed.
    Completed {
        import_id: String,
        succeeded: usize,
        failed: usize,
    },
    /// The request itself failed; nothing was imported.
    Failed { message: String },
}

#[derive(Clone, Debug)]
pub struct ActiveImport {
    pub source: ImportSource,
    pub phase: ImportPhase,
    pub status: Option<AgentExternalAgentImportStatus>,
}

impl ActiveImport {
    pub fn busy(&self) -> bool {
        matches!(
            self.phase,
            ImportPhase::Requesting | ImportPhase::Running { .. }
        )
    }
}

pub struct ExternalAgentImportState {
    pub generation: u64,
    /// Read cycle for the detection sweep; a response for an older cycle is
    /// discarded.
    pub cycle: u64,
    pub sources: Vec<ImportSourceState>,
    pub histories: Option<AgentExternalAgentImportHistories>,
    pub histories_loading: bool,
    pub histories_error: Option<AgentExternalAgentConfigError>,
    pub active: Option<ActiveImport>,
}

impl Default for ExternalAgentImportState {
    fn default() -> Self {
        Self {
            generation: 0,
            cycle: 0,
            sources: ImportSource::ALL
                .into_iter()
                .map(ImportSourceState::new)
                .collect(),
            histories: None,
            histories_loading: false,
            histories_error: None,
            active: None,
        }
    }
}

impl ExternalAgentImportState {
    pub fn begin_detect(&mut self, generation: u64) -> u64 {
        self.cycle = self.cycle.wrapping_add(1);
        self.generation = generation;
        for source in &mut self.sources {
            source.loading = true;
        }
        self.cycle
    }

    pub fn source_mut(&mut self, source: ImportSource) -> Option<&mut ImportSourceState> {
        self.sources.iter_mut().find(|state| state.source == source)
    }

    pub fn source(&self, source: ImportSource) -> Option<&ImportSourceState> {
        self.sources.iter().find(|state| state.source == source)
    }

    pub fn apply_detect(
        &mut self,
        cycle: u64,
        source: ImportSource,
        result: AgentExternalAgentDetectResult,
    ) -> bool {
        if cycle != self.cycle {
            return false;
        }
        let Some(state) = self.source_mut(source) else {
            return false;
        };
        state.loading = false;
        state.error = None;
        state.selected = (0..result.items.len()).collect();
        state.result = Some(result);
        true
    }

    pub fn apply_detect_error(
        &mut self,
        cycle: u64,
        source: ImportSource,
        error: AgentExternalAgentConfigError,
    ) -> bool {
        if cycle != self.cycle {
            return false;
        }
        let Some(state) = self.source_mut(source) else {
            return false;
        };
        state.loading = false;
        state.error = Some(error);
        true
    }

    /// Starts an import for one source. Refuses to overlap with a running one:
    /// the protocol correlates notifications by import id, and a second
    /// concurrent import would make the UI show progress it cannot attribute.
    pub fn begin_import(&mut self, source: ImportSource) -> bool {
        if self.active.as_ref().is_some_and(ActiveImport::busy) {
            return false;
        }
        self.active = Some(ActiveImport {
            source,
            phase: ImportPhase::Requesting,
            status: None,
        });
        true
    }

    pub fn apply_import_started(&mut self, source: ImportSource, import_id: String) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        if active.source != source {
            return false;
        }
        active.phase = ImportPhase::Running { import_id };
        true
    }

    pub fn apply_import_failed(&mut self, source: ImportSource, message: String) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        if active.source != source {
            return false;
        }
        active.phase = ImportPhase::Failed { message };
        true
    }

    /// Applies a progress or completed notification. A notification whose
    /// import id belongs to another import (or to none) is inert.
    pub fn apply_status(&mut self, status: &AgentExternalAgentImportStatus) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        let current = match &active.phase {
            ImportPhase::Running { import_id } => Some(import_id.as_str()),
            ImportPhase::Completed { import_id, .. } => Some(import_id.as_str()),
            _ => None,
        };
        if current != Some(status.import_id.as_str()) {
            return false;
        }
        if !status.completed {
            active.status = Some(status.clone());
            return true;
        }
        active.phase = ImportPhase::Completed {
            import_id: status.import_id.clone(),
            succeeded: status.successful_item_count(),
            failed: status.failed_item_count(),
        };
        active.status = Some(status.clone());
        true
    }

    pub fn dismiss_import(&mut self) {
        self.active = None;
    }

    pub fn begin_histories(&mut self) {
        self.histories_loading = true;
        self.histories_error = None;
    }

    pub fn apply_histories(&mut self, histories: AgentExternalAgentImportHistories) -> bool {
        self.histories_loading = false;
        self.histories_error = None;
        self.generation = histories.generation;
        self.histories = Some(histories);
        true
    }

    pub fn apply_histories_error(&mut self, error: AgentExternalAgentConfigError) -> bool {
        self.histories_loading = false;
        self.histories_error = Some(error);
        true
    }

    /// The detection sweep finished for every source.
    pub fn detected(&self) -> bool {
        self.sources.iter().all(ImportSourceState::resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{
        AgentExternalAgentDetectRequest, AgentExternalAgentDetectResult, AgentExternalAgentFailure,
        AgentExternalAgentImportStatus, AgentExternalAgentItemType, AgentExternalAgentSuccess,
        AgentExternalAgentTypeResult,
    };

    fn status(
        import_id: &str,
        completed: bool,
        successes: usize,
        failures: usize,
    ) -> AgentExternalAgentImportStatus {
        AgentExternalAgentImportStatus {
            generation: 1,
            import_id: import_id.to_owned(),
            completed,
            item_type_results: vec![AgentExternalAgentTypeResult {
                item_type: AgentExternalAgentItemType::Skills,
                successes: (0..successes)
                    .map(|index| AgentExternalAgentSuccess {
                        item_type: AgentExternalAgentItemType::Skills,
                        cwd: None,
                        source: None,
                        target: Some(format!("skill-{index}")),
                        title: None,
                        extra: Default::default(),
                    })
                    .collect(),
                failures: (0..failures)
                    .map(|index| AgentExternalAgentFailure {
                        item_type: AgentExternalAgentItemType::Skills,
                        failure_stage: "copy".to_owned(),
                        message: format!("failure-{index}"),
                        cwd: None,
                        source: None,
                        error_type: None,
                        sub_error_type: None,
                        extra: Default::default(),
                    })
                    .collect(),
                extra: Default::default(),
            }],
            extra: Default::default(),
        }
    }

    #[test]
    fn a_status_for_another_import_is_inert() {
        let mut state = ExternalAgentImportState::default();
        assert!(state.begin_import(ImportSource::ClaudeCode));
        assert!(state.apply_import_started(ImportSource::ClaudeCode, "import-1".to_owned()));
        // A notification for a different import never touches this one.
        assert!(!state.apply_status(&status("import-2", true, 1, 0)));
        assert!(
            !state
                .apply_status(&status("import-1", true, 1, 0))
                .eq(&false)
        );
        let phase = state.active.as_ref().unwrap().phase.clone();
        match phase {
            ImportPhase::Completed {
                import_id,
                succeeded,
                failed,
            } => {
                assert_eq!(import_id, "import-1");
                assert_eq!(succeeded, 1);
                assert_eq!(failed, 0);
            }
            other => panic!("unexpected phase {other:?}"),
        }
    }

    #[test]
    fn progress_is_kept_until_completion_arrives() {
        let mut state = ExternalAgentImportState::default();
        assert!(state.begin_import(ImportSource::Cursor));
        assert!(state.apply_import_started(ImportSource::Cursor, "import-9".to_owned()));
        assert!(state.apply_status(&status("import-9", false, 2, 1)));
        let active = state.active.as_ref().unwrap();
        assert!(active.busy());
        assert_eq!(active.status.as_ref().unwrap().successful_item_count(), 2);
        assert_eq!(active.status.as_ref().unwrap().failed_item_count(), 1);
    }

    #[test]
    fn a_second_import_is_refused_while_one_is_running() {
        let mut state = ExternalAgentImportState::default();
        assert!(state.begin_import(ImportSource::ClaudeCode));
        assert!(state.apply_import_started(ImportSource::ClaudeCode, "a".to_owned()));
        assert!(!state.begin_import(ImportSource::Cursor));
        state.dismiss_import();
        assert!(state.begin_import(ImportSource::Cursor));
    }

    #[test]
    fn detection_cycles_discard_superseded_answers() {
        let mut state = ExternalAgentImportState::default();
        let stale = state.begin_detect(1);
        let current = state.begin_detect(1);
        let result = AgentExternalAgentDetectResult {
            generation: 1,
            request: AgentExternalAgentDetectRequest::default(),
            items: vec![AgentExternalAgentMigrationItem {
                item_type: AgentExternalAgentItemType::Config,
                description: "Migrate settings".to_owned(),
                cwd: None,
                details: None,
            }],
            connectors: None,
            extra: Default::default(),
        };
        assert!(!state.apply_detect(stale, ImportSource::Cursor, result.clone()));
        assert!(state.apply_detect(current, ImportSource::Cursor, result));
        let cursor = state.source(ImportSource::Cursor).unwrap();
        // Detection selects everything it found; nothing is pre-deselected.
        assert_eq!(cursor.selected.len(), 1);
        assert!(cursor.resolved());
    }
}
