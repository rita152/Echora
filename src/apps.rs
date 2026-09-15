//! App (connector) directory state. The directory read and the installed
//! runtime snapshot are separate caches: they are different server reads with
//! different freshness, and the UI never folds one into the other.

use crate::agent::{
    AgentAppInfo, AgentAppsError, AgentAppsPage, AgentAppsReadResult, AgentInstalledApp,
    AgentInstalledApps,
};

#[derive(Default)]
pub struct AppsDirectory {
    /// Monotonic read cycle; a response for an older cycle is discarded.
    pub cycle: u64,
    pub generation: u64,
    pub page: Option<AgentAppsPage>,
    pub installed: Option<AgentInstalledApps>,
    pub loading: bool,
    pub error: Option<AgentAppsError>,
    /// True when `app/list/updated` arrived after the last successful read.
    pub stale: bool,
    pub detail: Option<AgentAppsReadResult>,
    pub detail_error: Option<AgentAppsError>,
    /// App whose detail read is in flight.
    pub opening: Option<String>,
}

impl AppsDirectory {
    pub fn begin_load(&mut self, generation: u64) -> u64 {
        self.cycle = self.cycle.wrapping_add(1);
        self.generation = generation;
        self.loading = true;
        self.cycle
    }

    pub fn apply_page(&mut self, cycle: u64, page: AgentAppsPage) -> bool {
        if cycle != self.cycle {
            return false;
        }
        self.loading = false;
        self.error = None;
        self.stale = false;
        self.generation = page.generation;
        self.page = Some(page);
        true
    }

    pub fn apply_installed(&mut self, cycle: u64, installed: AgentInstalledApps) -> bool {
        if cycle != self.cycle {
            return false;
        }
        self.generation = installed.generation;
        self.installed = Some(installed);
        true
    }

    pub fn apply_error(&mut self, cycle: u64, error: AgentAppsError) -> bool {
        if cycle != self.cycle {
            return false;
        }
        self.loading = false;
        self.error = Some(error);
        true
    }

    /// `app/list/updated` only invalidates the cache. It never clears the
    /// entries on screen, which stay until the directory is re-read.
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    pub fn begin_detail(&mut self, app_id: &str) {
        self.opening = Some(app_id.to_owned());
        self.detail = None;
        self.detail_error = None;
    }

    pub fn apply_detail(&mut self, app_id: &str, result: AgentAppsReadResult) -> bool {
        if self.opening.as_deref() != Some(app_id) {
            return false;
        }
        self.opening = None;
        self.detail = Some(result);
        true
    }

    pub fn apply_detail_error(&mut self, app_id: &str, error: AgentAppsError) -> bool {
        if self.opening.as_deref() != Some(app_id) {
            return false;
        }
        self.opening = None;
        self.detail_error = Some(error);
        true
    }

    pub fn clear_detail(&mut self) {
        self.opening = None;
        self.detail = None;
        self.detail_error = None;
    }

    pub fn entries(&self) -> &[AgentAppInfo] {
        self.page
            .as_ref()
            .map(|page| page.apps.as_slice())
            .unwrap_or_default()
    }

    /// The runtime snapshot entry for one app, when the server reported one.
    pub fn installed_entry(&self, app_id: &str) -> Option<&AgentInstalledApp> {
        self.installed
            .as_ref()
            .and_then(|installed| installed.apps.iter().find(|app| app.id == app_id))
    }

    /// Whether the directory has finished its first read, successfully or not.
    pub fn resolved(&self) -> bool {
        !self.loading && (self.page.is_some() || self.error.is_some())
    }
}
