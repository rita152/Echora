//! Hooks settings state: the `hooks/list` snapshot, the reference's grouping
//! of hooks by source, and per-hook write intents. Independent of GPUI.

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

use crate::agent::{
    AgentHook, AgentHookEventName, AgentHookListEntry, AgentHookLoadError, AgentHookSourceGroup,
    AgentHookStateChange, AgentHooksSnapshot,
};

/// Which source's hooks the detail dialog shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HookSourceSelection {
    /// User, admin, session-flag and unknown hooks apply to all projects.
    Shared(AgentHookSourceGroup),
    Project(PathBuf),
    /// `None` collects plugin hooks without a plugin id.
    Plugin(Option<String>),
}

/// Hooks of one source, deduplicated across the listed working directories,
/// with the load issues of the entries that hold them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookSource {
    pub selection: HookSourceSelection,
    pub hooks: Vec<AgentHook>,
    pub warnings: Vec<String>,
    pub errors: Vec<AgentHookLoadError>,
}

impl HookSource {
    pub fn issue_count(&self) -> usize {
        self.warnings.len() + self.errors.len()
    }

    pub fn needs_review(&self) -> usize {
        self.hooks.iter().filter(|hook| hook.needs_review()).count()
    }

    /// Hooks "Trust all" would approve: new or changed, and not managed.
    pub fn trustable(&self) -> Vec<&AgentHook> {
        self.hooks
            .iter()
            .filter(|hook| !hook.is_managed && hook.needs_review())
            .collect()
    }

    /// Events in first-listed order, each with its hooks by display order.
    pub fn events(&self) -> Vec<(AgentHookEventName, Vec<&AgentHook>)> {
        let mut order = Vec::new();
        for hook in &self.hooks {
            if !order.contains(&hook.event_name) {
                order.push(hook.event_name);
            }
        }
        order
            .into_iter()
            .map(|event| {
                let mut hooks = self
                    .hooks
                    .iter()
                    .filter(|hook| hook.event_name == event)
                    .collect::<Vec<_>>();
                hooks.sort_by_key(|hook| hook.display_order);
                (event, hooks)
            })
            .collect()
    }
}

/// The overview's groups, in the reference's order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HookSourceGroups {
    /// "From config": user, then admin.
    pub config: Vec<HookSource>,
    pub plugins: Vec<HookSource>,
    pub projects: Vec<HookSource>,
    /// "Other sources": session flags, then unknown.
    pub other: Vec<HookSource>,
}

impl HookSourceGroups {
    pub fn is_empty(&self) -> bool {
        self.config.is_empty()
            && self.plugins.is_empty()
            && self.projects.is_empty()
            && self.other.is_empty()
    }

    pub fn find(&self, selection: &HookSourceSelection) -> Option<&HookSource> {
        self.config
            .iter()
            .chain(&self.plugins)
            .chain(&self.projects)
            .chain(&self.other)
            .find(|source| &source.selection == selection)
    }
}

fn merged(
    entries: &[AgentHookListEntry],
    selection: HookSourceSelection,
    hooks: Vec<AgentHook>,
    extra: &[&AgentHookListEntry],
) -> HookSource {
    let mut seen = HashSet::new();
    let hooks = hooks
        .into_iter()
        .filter(|hook| seen.insert(hook.key.clone()))
        .collect::<Vec<_>>();
    let holders = entries
        .iter()
        .filter(|entry| entry.hooks.iter().any(|hook| seen.contains(&hook.key)))
        .chain(extra.iter().copied());
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    for entry in holders {
        for warning in &entry.warnings {
            if !warnings.contains(warning) {
                warnings.push(warning.clone());
            }
        }
        for error in &entry.errors {
            if !errors.contains(error) {
                errors.push(error.clone());
            }
        }
    }
    HookSource {
        selection,
        hooks,
        warnings,
        errors,
    }
}

/// Folds `hooks/list` entries into the overview, as the reference does: shared
/// sources merge every cwd's hooks by key; a project entry keeps only its own
/// project hooks; entries with issues but no hooks count as unknown.
pub fn group_sources(entries: &[AgentHookListEntry]) -> HookSourceGroups {
    let shared = |group: AgentHookSourceGroup| {
        let hooks = entries
            .iter()
            .flat_map(|entry| &entry.hooks)
            .filter(|hook| hook.source.group() == group)
            .cloned()
            .collect::<Vec<_>>();
        let issues_only = if group == AgentHookSourceGroup::Unknown {
            entries
                .iter()
                .filter(|entry| {
                    entry.hooks.is_empty()
                        && (!entry.warnings.is_empty() || !entry.errors.is_empty())
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        (!hooks.is_empty() || !issues_only.is_empty()).then(|| {
            merged(
                entries,
                HookSourceSelection::Shared(group),
                hooks,
                &issues_only,
            )
        })
    };
    let projects = entries
        .iter()
        .filter_map(|entry| {
            let hooks = entry
                .hooks
                .iter()
                .filter(|hook| hook.source.group() == AgentHookSourceGroup::Project)
                .cloned()
                .collect::<Vec<_>>();
            (!hooks.is_empty()).then(|| HookSource {
                selection: HookSourceSelection::Project(entry.cwd.clone()),
                hooks,
                warnings: entry.warnings.clone(),
                errors: entry.errors.clone(),
            })
        })
        .collect();
    let mut by_plugin = BTreeMap::<Option<String>, Vec<AgentHook>>::new();
    for hook in entries.iter().flat_map(|entry| &entry.hooks) {
        if hook.source.group() == AgentHookSourceGroup::Plugin {
            by_plugin
                .entry(hook.plugin_id.clone())
                .or_default()
                .push(hook.clone());
        }
    }
    // Named plugins in id order, the unnamed bucket last.
    let mut plugins = by_plugin
        .into_iter()
        .map(|(plugin, hooks)| merged(entries, HookSourceSelection::Plugin(plugin), hooks, &[]))
        .collect::<Vec<_>>();
    plugins.sort_by_key(|source| match &source.selection {
        HookSourceSelection::Plugin(None) => (1, String::new()),
        HookSourceSelection::Plugin(Some(id)) => (0, id.clone()),
        _ => (2, String::new()),
    });
    HookSourceGroups {
        config: [AgentHookSourceGroup::User, AgentHookSourceGroup::Admin]
            .into_iter()
            .filter_map(shared)
            .collect(),
        plugins,
        projects,
        other: [
            AgentHookSourceGroup::SessionFlags,
            AgentHookSourceGroup::Unknown,
        ]
        .into_iter()
        .filter_map(shared)
        .collect(),
    }
}

/// `hooks/list` cwds: the selected project's roots first, then every other
/// known project root in order, as the reference sends them.
pub fn list_cwds(selected: &[PathBuf], all: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut cwds = selected.to_vec();
    let mut others = all
        .into_iter()
        .filter(|root| !selected.contains(root))
        .collect::<Vec<_>>();
    others.sort();
    others.dedup();
    cwds.extend(others);
    cwds
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookWriteFailure {
    /// Another config layer decides this hook's state.
    Overridden,
    Failed {
        message: String,
        outcome_unknown: bool,
    },
}

/// The last trust or enable write of the open source, kept until the next
/// one so its failure stays visible; a failure is never retried by itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookWrite {
    pub changes: Vec<AgentHookStateChange>,
    pub failure: Option<HookWriteFailure>,
    pub in_flight: bool,
}

#[derive(Default)]
pub struct HooksDirectory {
    pub cwds: Vec<PathBuf>,
    /// Monotonic read cycle; a response for an older cycle is discarded.
    pub cycle: u64,
    pub snapshot: Option<AgentHooksSnapshot>,
    pub loading: bool,
    /// A manual reload, which reports success with a toast.
    pub reloading: bool,
    pub error: Option<String>,
    pub open: Option<HookSourceSelection>,
    pub expanded: Option<String>,
    /// The dialog's load-issue list is open.
    pub issues_expanded: bool,
    pub write: Option<HookWrite>,
    /// A capture fixture is shown: backend reads must not replace it.
    pub capture_fixture: bool,
}

impl HooksDirectory {
    pub fn begin_refresh(&mut self, cwds: Vec<PathBuf>) -> u64 {
        self.cycle += 1;
        if self.cwds != cwds {
            self.snapshot = None;
        }
        self.cwds = cwds;
        self.loading = true;
        self.cycle
    }

    /// Applies the newest read only. Returns whether it was applied.
    pub fn accept(&mut self, cycle: u64, result: Result<AgentHooksSnapshot, String>) -> bool {
        if cycle != self.cycle {
            return false;
        }
        self.loading = false;
        match result {
            Ok(snapshot) => {
                self.error = None;
                self.snapshot = Some(snapshot);
            }
            Err(error) => self.error = Some(error),
        }
        true
    }

    pub fn groups(&self) -> HookSourceGroups {
        self.snapshot
            .as_ref()
            .map(|snapshot| group_sources(&snapshot.entries))
            .unwrap_or_default()
    }

    pub fn open_source(&self) -> Option<HookSource> {
        let selection = self.open.as_ref()?;
        self.groups().find(selection).cloned()
    }

    /// Records a write about to be sent. Refused while another is in flight.
    pub fn begin_write(&mut self, changes: Vec<AgentHookStateChange>) -> bool {
        if self.write.as_ref().is_some_and(|write| write.in_flight) || changes.is_empty() {
            return false;
        }
        self.write = Some(HookWrite {
            changes,
            failure: None,
            in_flight: true,
        });
        true
    }

    pub fn finish_write(&mut self, failure: Option<HookWriteFailure>) {
        if let Some(write) = &mut self.write {
            write.in_flight = false;
            write.failure = failure;
        }
    }

    pub fn writing(&self) -> bool {
        self.write.as_ref().is_some_and(|write| write.in_flight)
    }

    /// The value a hook's switch shows while its write is in flight.
    pub fn displayed_enabled(&self, hook: &AgentHook) -> bool {
        self.write
            .as_ref()
            .filter(|write| write.in_flight)
            .and_then(|write| write.changes.iter().find(|change| change.key == hook.key))
            .and_then(|change| change.enabled)
            .unwrap_or(hook.enabled)
    }
}

/// Where to open a hook's definition: the file itself when it exists here.
pub fn local_source_file(hook: &AgentHook) -> Option<&Path> {
    let path = hook.source_path.as_path();
    (path.is_absolute() && path.is_file()).then_some(path)
}

#[cfg(test)]
mod tests;
