//! Backend driving for the import page: one detection per source, the import
//! request whose notifications close it, and the history read that reports what
//! was imported before.

use gpui::Context;

use super::SettingsView;
use crate::{
    agent::{
        AgentExternalAgentConfigError, AgentExternalAgentConfigErrorKind,
        AgentExternalAgentDetectRequest, AgentExternalAgentImportRequest,
    },
    imports::ImportSource,
};

impl SettingsView {
    /// Detects every source this client can ask about and re-reads the history.
    pub(super) fn refresh_external_agent_imports(&mut self, cx: &mut Context<Self>) {
        let generation = self.imports.generation;
        let cycle = self.imports.begin_detect(generation);
        for source in ImportSource::ALL {
            self.detect_import_source(source, cycle, cx);
        }
        self.refresh_import_histories(cx);
        cx.notify();
    }

    fn detect_import_source(&mut self, source: ImportSource, cycle: u64, cx: &mut Context<Self>) {
        // The parameters are the ones the reference client sends: `includeHome`
        // always, `cwds` only when a workspace root exists, and
        // `migrationSource` only for the provider whose detection route needs
        // it.
        let request = AgentExternalAgentDetectRequest {
            cwds: self.plugins_cwd.clone().map(|cwd| vec![cwd]),
            include_home: true,
            max_session_age_days: None,
            max_sessions: None,
            migration_source: source.migration_source().map(str::to_owned),
            source: None,
        };
        let reader = self.backend.detect_external_agent_config(request);
        cx.spawn(async move |this, cx| {
            let result = reader.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(result)) => {
                        this.imports.apply_detect(cycle, source, result);
                    }
                    Ok(Err(error)) => {
                        this.imports.apply_detect_error(cycle, source, error);
                    }
                    Err(_) => {
                        this.imports
                            .apply_detect_error(cycle, source, disconnected_error());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn refresh_import_histories(&mut self, cx: &mut Context<Self>) {
        self.imports.begin_histories();
        let reader = self.backend.read_external_agent_import_histories();
        cx.spawn(async move |this, cx| {
            let result = reader.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(histories)) => {
                        this.imports.apply_histories(histories);
                    }
                    Ok(Err(error)) => {
                        this.imports.apply_histories_error(error);
                    }
                    Err(_) => {
                        this.imports.apply_histories_error(disconnected_error());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Selects or clears one detected item of a source. The selection is the
    /// exact set of items that will be sent in `migrationItems`.
    pub(super) fn toggle_import_item(
        &mut self,
        source: ImportSource,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.imports.source_mut(source) else {
            return;
        };
        if !state.selected.remove(&index) {
            state.selected.insert(index);
        }
        cx.notify();
    }

    /// Starts an import for one source with the items the user selected. The
    /// import id the server returns is what the progress and completed
    /// notifications are matched against.
    pub(super) fn start_import(&mut self, source: ImportSource, cx: &mut Context<Self>) {
        let Some(state) = self.imports.source(source) else {
            return;
        };
        let migration_items = state.selected_items();
        if migration_items.is_empty() {
            return;
        }
        if !self.imports.begin_import(source) {
            // Another import is still running; the protocol correlates
            // notifications by import id and cannot attribute a second one.
            cx.notify();
            return;
        }
        let reader = self
            .backend
            .import_external_agent_config(AgentExternalAgentImportRequest {
                migration_items,
                migration_source: source.migration_source().map(str::to_owned),
                provider_id: Some(source.provider_id().to_owned()),
                // The reference client marks imports it starts from the app.
                source: Some("app".to_owned()),
            });
        cx.spawn(async move |this, cx| {
            let result = reader.recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(receipt)) => {
                        this.imports.apply_import_started(source, receipt.import_id);
                    }
                    Ok(Err(error)) => {
                        this.imports
                            .apply_import_failed(source, error.user_message());
                    }
                    Err(_) => {
                        this.imports
                            .apply_import_failed(source, "导入连接在返回结果前关闭".to_owned());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Applies progress/completed notifications and refreshes the surfaces an
    /// import changes once it finished.
    pub(super) fn apply_import_status(
        &mut self,
        status: &crate::agent::AgentExternalAgentImportStatus,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.imports.apply_status(status) {
            return false;
        }
        if status.completed {
            // The reference client records a history entry only when nothing
            // else did. The server reports what it recorded through
            // readHistories, so a completed import that is already there is
            // left alone and one that is missing is recorded from the
            // server-s own item results.
            let already_recorded = self.imports.histories.as_ref().is_some_and(|histories| {
                histories
                    .histories
                    .iter()
                    .any(|entry| entry.import_id == status.import_id)
            });
            self.refresh_import_histories(cx);
            if !already_recorded {
                let provider_id = self
                    .imports
                    .active
                    .as_ref()
                    .map(|active| active.source.provider_id().to_owned());
                if let Some(provider_id) = provider_id {
                    let reader = self.backend.record_external_agent_import_history(
                        crate::agent::AgentExternalAgentHistoryRecordRequest {
                            provider_id,
                            item_type_results: status.item_type_results.clone(),
                        },
                    );
                    cx.spawn(async move |this, cx| {
                        let result = reader.recv().await;
                        let _ = this.update(cx, |this, cx| {
                            if let Ok(Ok(_)) = result {
                                this.refresh_import_histories(cx);
                            }
                            cx.notify();
                        });
                    })
                    .detach();
                }
            }
            // An import can install plugins and skills; those directories are
            // re-read rather than assumed.
            self.refresh_plugins(true, cx);
        }
        cx.notify();
        true
    }
}

fn disconnected_error() -> AgentExternalAgentConfigError {
    AgentExternalAgentConfigError {
        kind: AgentExternalAgentConfigErrorKind::Connection,
        message: "导入请求连接在返回结果前关闭".to_owned(),
        data: None,
        outcome_unknown: false,
    }
}
