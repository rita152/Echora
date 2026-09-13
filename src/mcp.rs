//! MCP management state: server inventory, startup layering, reload outcome,
//! and OAuth logins this client started.

use std::collections::{BTreeMap, HashSet};

use crate::agent::{
    AgentMcpError, AgentMcpOauthCompletion, AgentMcpOauthCompletionStatus, AgentMcpReloadResult,
    AgentMcpServerPage, AgentMcpServerStartupStatus, AgentMcpServerStatus,
};

/// Scope key for a startup observation: the application scope when no thread is
/// involved, otherwise that thread.
pub type McpStartupScope = (Option<String>, String);

fn scope_key(status: &AgentMcpServerStartupStatus) -> McpStartupScope {
    (status.thread_id.clone(), status.name.clone())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpLoginState {
    pub login_id: u64,
    pub generation: u64,
    pub server_name: String,
    pub thread_id: Option<String>,
    pub authorization_url: String,
    pub phase: McpLoginPhase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpLoginPhase {
    /// The authorization URL is available; the browser step is up to the user.
    Waiting,
    Succeeded,
    Failed(String),
    Cancelled,
    Interrupted(String),
}

impl McpLoginPhase {
    pub fn busy(&self) -> bool {
        matches!(self, Self::Waiting)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Waiting => "等待授权",
            Self::Succeeded => "登录成功",
            Self::Failed(_) => "登录失败",
            Self::Cancelled => "已取消",
            Self::Interrupted(_) => "连接已断开",
        }
    }
}

#[derive(Default)]
pub struct McpDirectory {
    pub generation: u64,
    /// Servers merged from every page that has been read, in arrival order.
    pub order: Vec<String>,
    pub servers: BTreeMap<String, AgentMcpServerStatus>,
    /// Cursor for the next page; `None` means the list is complete for now.
    pub next_cursor: Option<String>,
    pub loading: bool,
    pub error: Option<AgentMcpError>,
    pub reload: Option<AgentMcpReloadResult>,
    pub reloading: bool,
    /// Startup observations, layered by scope and server name.
    pub startup: BTreeMap<McpStartupScope, AgentMcpServerStartupStatus>,
    pub logins: BTreeMap<u64, McpLoginState>,
    cursors: HashSet<String>,
}

impl McpDirectory {
    pub fn begin_refresh(&mut self) {
        self.loading = true;
        self.error = None;
        self.order.clear();
        self.servers.clear();
        self.startup.clear();
        self.next_cursor = None;
        self.cursors.clear();
    }

    /// Applies one page. A repeated or looping cursor ends the walk instead of
    /// requesting forever; the duplicate page is not merged twice.
    ///
    /// `partial` marks a `toolsAndAuthOnly` read: the server sends status and
    /// authentication but no inventories, so a previously loaded tool catalog
    /// is kept instead of being replaced by an empty one.
    pub fn accept_page(&mut self, page: AgentMcpServerPage, partial: bool) -> bool {
        if page.generation != self.generation {
            return false;
        }
        self.loading = false;
        self.error = None;
        for server in page.servers {
            let server = if partial {
                match self.servers.get(&server.name) {
                    Some(previous) if server.tools.is_empty() => AgentMcpServerStatus {
                        tools: previous.tools.clone(),
                        resources: previous.resources.clone(),
                        resource_templates: previous.resource_templates.clone(),
                        server_info: server
                            .server_info
                            .clone()
                            .or_else(|| previous.server_info.clone()),
                        ..server
                    },
                    _ => server,
                }
            } else {
                server
            };
            let name = server.name.clone();
            let is_new = self.servers.insert(name.clone(), server).is_none();
            if is_new && !self.order.contains(&name) {
                self.order.push(name);
            }
        }
        self.next_cursor = match page.next_cursor {
            Some(cursor) => {
                if !self.cursors.insert(cursor.clone()) {
                    // Cyclic cursor: stop walking and surface an explicit error
                    // rather than repeating the same page.
                    self.error = Some(AgentMcpError {
                        kind: crate::agent::AgentMcpErrorKind::Protocol,
                        message: format!("mcpServerStatus/list 返回了重复的 cursor `{cursor}`"),
                        data: None,
                        outcome_unknown: false,
                    });
                    None
                } else {
                    Some(cursor)
                }
            }
            None => None,
        };
        true
    }

    pub fn accept_reload(&mut self, result: AgentMcpReloadResult) {
        if result.generation != self.generation {
            return;
        }
        self.reloading = false;
        self.reload = Some(result);
    }

    /// Startup observations are stored per scope; an observation from a retired
    /// generation never overwrites a newer one.
    pub fn apply_startup(&mut self, generation: u64, status: AgentMcpServerStartupStatus) -> bool {
        if generation != self.generation {
            return false;
        }
        let key = scope_key(&status);
        if let Some(existing) = self.startup.get(&key)
            && existing.state == status.state
            && existing.error == status.error
        {
            return false;
        }
        self.startup.insert(key, status);
        true
    }

    pub fn startup_for(
        &self,
        thread_id: Option<&str>,
        name: &str,
    ) -> Option<&AgentMcpServerStartupStatus> {
        self.startup
            .get(&(thread_id.map(str::to_owned), name.to_owned()))
            .or_else(|| {
                // Fall back to the application scope when a thread scoped
                // observation has not arrived yet.
                self.startup.get(&(None, name.to_owned()))
            })
    }

    pub fn register_login(&mut self, login: McpLoginState) {
        self.logins.insert(login.login_id, login);
    }

    pub fn accept_login_completion(&mut self, completion: &AgentMcpOauthCompletion) -> bool {
        let Some(login) = self.logins.get_mut(&completion.login_id) else {
            // A completion for a login this client never started, or one that
            // was already retired, must not change any visible state.
            return false;
        };
        if !login.phase.busy() {
            return false;
        }
        login.phase = match &completion.status {
            AgentMcpOauthCompletionStatus::Succeeded => McpLoginPhase::Succeeded,
            AgentMcpOauthCompletionStatus::Failed(error) => McpLoginPhase::Failed(error.clone()),
            AgentMcpOauthCompletionStatus::Cancelled => McpLoginPhase::Cancelled,
            AgentMcpOauthCompletionStatus::Interrupted(error) => {
                McpLoginPhase::Interrupted(error.clone())
            }
        };
        true
    }

    pub fn cancel_login(&mut self, login_id: u64) -> bool {
        match self.logins.get_mut(&login_id) {
            Some(login) if login.phase.busy() => {
                login.phase = McpLoginPhase::Cancelled;
                true
            }
            _ => false,
        }
    }

    /// A generation change abandons every in-flight operation for the retired
    /// connection; nothing from the old generation may update the UI.
    pub fn reset_for_generation(&mut self, generation: u64) {
        if self.generation == generation {
            return;
        }
        let previous = self.generation;
        self.generation = generation;
        self.servers.clear();
        self.order.clear();
        self.next_cursor = None;
        self.cursors.clear();
        self.loading = false;
        self.reloading = false;
        for login in self.logins.values_mut() {
            if login.generation == previous && login.phase.busy() {
                login.phase = McpLoginPhase::Interrupted("连接已重建".into());
            }
        }
    }

    pub fn server_count(&self) -> usize {
        self.servers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{
        AgentMcpAuthStatus, AgentMcpServerConnectionStatus, AgentMcpServerStartupState,
    };

    fn server(name: &str) -> AgentMcpServerStatus {
        AgentMcpServerStatus {
            name: name.into(),
            plugin_id: None,
            auth_status: AgentMcpAuthStatus::Unsupported,
            runtime_status: None,
            server_info: None,
            tools: Vec::new(),
            resources: Vec::new(),
            resource_templates: Vec::new(),
            tools_error: None,
            extra: Default::default(),
        }
    }

    fn page(generation: u64, names: &[&str], next: Option<&str>) -> AgentMcpServerPage {
        AgentMcpServerPage {
            generation,
            cursor: None,
            servers: names.iter().map(|name| server(name)).collect(),
            next_cursor: next.map(str::to_owned),
            extra: Default::default(),
        }
    }

    fn startup(
        name: &str,
        thread: Option<&str>,
        state: AgentMcpServerStartupState,
    ) -> AgentMcpServerStartupStatus {
        AgentMcpServerStartupStatus {
            thread_id: thread.map(str::to_owned),
            name: name.into(),
            state,
            error: None,
            failure_reason: None,
        }
    }

    #[test]
    fn pages_merge_and_cyclic_cursor_stops_walk() {
        let mut directory = McpDirectory {
            generation: 1,
            ..Default::default()
        };
        directory.begin_refresh();
        assert!(directory.accept_page(page(1, &["alpha"], Some("cursor-1")), false));
        assert_eq!(directory.order, vec!["alpha".to_owned()]);
        assert_eq!(directory.next_cursor.as_deref(), Some("cursor-1"));
        assert!(directory.accept_page(page(1, &["beta"], Some("cursor-1")), false));
        assert!(directory.next_cursor.is_none());
        assert!(directory.error.is_some());
        assert_eq!(directory.order, vec!["alpha".to_owned(), "beta".to_owned()]);
    }

    #[test]
    fn startup_layers_by_scope_and_matches_thread_first() {
        let mut directory = McpDirectory {
            generation: 4,
            ..Default::default()
        };
        assert!(directory.apply_startup(
            4,
            startup("alpha", None, AgentMcpServerStartupState::Starting)
        ));
        assert!(directory.apply_startup(
            4,
            startup("alpha", Some("t1"), AgentMcpServerStartupState::Ready)
        ));
        assert_eq!(
            directory.startup_for(Some("t1"), "alpha").unwrap().state,
            AgentMcpServerStartupState::Ready
        );
        assert_eq!(
            directory.startup_for(Some("t2"), "alpha").unwrap().state,
            AgentMcpServerStartupState::Starting
        );
        assert!(!directory.apply_startup(
            3,
            startup("alpha", None, AgentMcpServerStartupState::Failed)
        ));
    }

    #[test]
    fn late_login_completion_is_ignored_after_cancel() {
        let mut directory = McpDirectory::default();
        directory.register_login(McpLoginState {
            login_id: 7,
            generation: 1,
            server_name: "alpha".into(),
            thread_id: None,
            authorization_url: "https://example.invalid/authorize".into(),
            phase: McpLoginPhase::Waiting,
        });
        assert!(directory.cancel_login(7));
        let completion = AgentMcpOauthCompletion {
            login_id: 7,
            generation: 1,
            server_name: "alpha".into(),
            thread_id: None,
            status: AgentMcpOauthCompletionStatus::Succeeded,
            extra: Default::default(),
        };
        assert!(!directory.accept_login_completion(&completion));
        assert_eq!(directory.logins[&7].phase, McpLoginPhase::Cancelled);
    }

    #[test]
    fn generation_change_interrupts_visible_logins() {
        let mut directory = McpDirectory {
            generation: 1,
            ..Default::default()
        };
        directory.register_login(McpLoginState {
            login_id: 3,
            generation: 1,
            server_name: "alpha".into(),
            thread_id: Some("t1".into()),
            authorization_url: "https://example.invalid/authorize".into(),
            phase: McpLoginPhase::Waiting,
        });
        directory.reset_for_generation(2);
        assert!(matches!(
            directory.logins[&3].phase,
            McpLoginPhase::Interrupted(_)
        ));
    }

    #[test]
    fn tools_and_auth_only_reads_keep_the_loaded_catalog() {
        let mut directory = McpDirectory {
            generation: 1,
            ..Default::default()
        };
        let mut full = page(1, &["echo-tools"], None);
        full.servers[0].tools.push(crate::agent::AgentMcpTool {
            name: "echo".into(),
            title: None,
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
            extra: Default::default(),
        });
        directory.begin_refresh();
        assert!(directory.accept_page(full, false));
        assert_eq!(directory.servers["echo-tools"].tools.len(), 1);

        // A cheap status/auth read carries no inventories; the catalog stays.
        let mut partial = page(1, &["echo-tools"], None);
        partial.servers[0].runtime_status = Some(AgentMcpServerConnectionStatus::Connected);
        assert!(directory.accept_page(partial, true));
        assert_eq!(directory.servers["echo-tools"].tools.len(), 1);
        assert_eq!(
            directory.servers["echo-tools"].runtime_status,
            Some(AgentMcpServerConnectionStatus::Connected)
        );
    }
}
