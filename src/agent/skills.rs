//! Skills inventory, selectors and configuration receipts.
//! The backend owns discovery, ordering and persistence; this module keeps the
//! agent-neutral shape, including fields this client does not interpret yet.

use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

/// Where a skill was discovered. Unknown scopes are a protocol error rather
/// than a silently dropped skill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgentSkillScope {
    User,
    Repo,
    System,
    Admin,
}

impl AgentSkillScope {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "user" => Some(Self::User),
            "repo" => Some(Self::Repo),
            "system" => Some(Self::System),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::User => crate::i18n::text("个人"),
            Self::Repo => crate::i18n::text("项目"),
            Self::System => crate::i18n::text("系统"),
            Self::Admin => crate::i18n::text("组织"),
        }
    }
}

/// Presentation metadata from `SKILL.json`; every field is optional and new
/// keys are retained in `extra`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentSkillInterface {
    pub display_name: Option<String>,
    pub short_description: Option<String>,
    pub default_prompt: Option<String>,
    pub brand_color: Option<String>,
    pub icon_small: Option<PathBuf>,
    pub icon_large: Option<PathBuf>,
    pub icon_small_url: Option<String>,
    pub icon_large_url: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkillDependency {
    pub tool_type: String,
    pub value: String,
    pub command: Option<String>,
    pub description: Option<String>,
    pub transport: Option<String>,
    pub url: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub scope: AgentSkillScope,
    pub enabled: bool,
    pub short_description: Option<String>,
    pub plugin_id: Option<String>,
    pub interface: Option<AgentSkillInterface>,
    pub dependencies: Option<Vec<AgentSkillDependency>>,
    pub extra: BTreeMap<String, Value>,
}

impl AgentSkill {
    /// Prefers the newer `interface.displayName`, then the legacy
    /// `shortDescription`, then the directory name.
    pub fn display_name(&self) -> String {
        self.interface
            .as_ref()
            .and_then(|interface| interface.display_name.clone())
            .unwrap_or_else(|| self.name.clone())
    }

    pub fn summary(&self) -> String {
        self.interface
            .as_ref()
            .and_then(|interface| interface.short_description.clone())
            .or_else(|| self.short_description.clone())
            .filter(|summary| !summary.trim().is_empty())
            .unwrap_or_else(|| self.description.clone())
    }

    pub fn icon_path(&self) -> Option<&PathBuf> {
        self.interface.as_ref().and_then(|interface| {
            interface
                .icon_small
                .as_ref()
                .or(interface.icon_large.as_ref())
        })
    }

    pub fn selector(&self) -> AgentSkillSelector {
        // Plugin skills are addressed by their namespaced name; local skills by
        // the file the server reported so a rename cannot retarget the write.
        if self.name.contains(':') || self.plugin_id.is_some() {
            AgentSkillSelector::Name(self.name.clone())
        } else {
            AgentSkillSelector::Path(self.path.clone())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkillLoadError {
    pub path: PathBuf,
    pub message: String,
    pub extra: BTreeMap<String, Value>,
}

/// One working directory's worth of discovery results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkillsEntry {
    pub cwd: PathBuf,
    pub skills: Vec<AgentSkill>,
    pub errors: Vec<AgentSkillLoadError>,
    pub extra: BTreeMap<String, Value>,
}

/// A `skills/list` result. The schema defines no cursor for this method, but a
/// future server may add one; `next_cursor` is retained and validated so a
/// repeated or looping cursor can never build an unbounded page walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkillsSnapshot {
    pub generation: u64,
    pub entries: Vec<AgentSkillsEntry>,
    pub next_cursor: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

impl AgentSkillsSnapshot {
    pub fn skills(&self) -> impl Iterator<Item = &AgentSkill> {
        self.entries.iter().flat_map(|entry| entry.skills.iter())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkillsLoadRequest {
    pub cwds: Vec<PathBuf>,
    pub force_reload: bool,
}

/// Only the selector the user acted on is submitted. `name` addresses plugin
/// skills, `path` addresses local skills.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentSkillSelector {
    Name(String),
    Path(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkillWriteRequest {
    pub generation: u64,
    pub selector: AgentSkillSelector,
    pub enabled: bool,
}

/// The server decides the effective value; the client never predicts it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkillWriteReceipt {
    pub effective_enabled: bool,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentSkillsErrorKind {
    Unsupported,
    Protocol,
    Connection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkillsError {
    pub kind: AgentSkillsErrorKind,
    pub message: String,
    /// Original JSON-RPC error payload, including provider specific extensions.
    pub data: Option<Value>,
    /// True when a write may or may not have been applied server side.
    pub outcome_unknown: bool,
}

impl AgentSkillsError {
    pub fn user_message(&self) -> String {
        match self.kind {
            AgentSkillsErrorKind::Unsupported => {
                crate::i18n::text("当前 coding agent 不支持技能管理").to_owned()
            }
            AgentSkillsErrorKind::Connection => {
                crate::i18n::text("与 coding agent 的连接已断开").to_owned()
            }
            AgentSkillsErrorKind::Protocol => crate::i18n::text("技能请求失败").to_owned(),
        }
    }
}
