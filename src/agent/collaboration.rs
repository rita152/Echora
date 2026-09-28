//! Collaboration mode presets (plan / default) advertised by the backend.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentCollaborationModeKind {
    Plan,
    Default,
}

/// One preset. `None` model or effort means the preset does not override the
/// user's own choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCollaborationModePreset {
    pub name: String,
    pub mode: AgentCollaborationModeKind,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

/// The presets of one connection generation, one per mode, plan first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCollaborationModes {
    pub generation: u64,
    pub presets: Vec<AgentCollaborationModePreset>,
}

impl AgentCollaborationModes {
    pub fn preset(
        &self,
        mode: AgentCollaborationModeKind,
    ) -> Option<&AgentCollaborationModePreset> {
        self.presets.iter().find(|preset| preset.mode == mode)
    }
}
