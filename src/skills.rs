//! Skills management state: one cache per working directory, refresh cycles,
//! and local write intents that survive unrelated refreshes.

use std::{collections::BTreeMap, path::PathBuf};

use crate::agent::{
    AgentSkill, AgentSkillSelector, AgentSkillWriteReceipt, AgentSkillsError, AgentSkillsSnapshot,
};

/// Stable identity of a skill inside the current snapshot.
pub fn skill_key(skill: &AgentSkill) -> String {
    skill.path.display().to_string()
}

pub fn selector_key(selector: &AgentSkillSelector) -> String {
    match selector {
        AgentSkillSelector::Name(name) => format!("name:{name}"),
        AgentSkillSelector::Path(path) => format!("path:{}", path.display()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillWritePhase {
    /// The user changed the switch; the request is on its way.
    Saving,
    /// The server accepted the write and reported an effective value.
    Saved { effective_enabled: bool },
    /// The request failed. The intent is preserved for an explicit retry.
    Failed {
        message: String,
        outcome_unknown: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillWrite {
    pub key: String,
    pub selector: AgentSkillSelector,
    pub sequence: u64,
    pub intended_enabled: bool,
    pub phase: SkillWritePhase,
}

impl SkillWrite {
    /// Value the switch shows while a local operation is outstanding.
    pub fn displayed_enabled(&self) -> bool {
        match &self.phase {
            SkillWritePhase::Saved { effective_enabled } => *effective_enabled,
            SkillWritePhase::Saving | SkillWritePhase::Failed { .. } => self.intended_enabled,
        }
    }

    pub fn busy(&self) -> bool {
        matches!(self.phase, SkillWritePhase::Saving)
    }
}

#[derive(Default)]
pub struct SkillsDirectory {
    pub cwd: PathBuf,
    /// Monotonic read cycle; a response for an older cycle is discarded.
    pub cycle: u64,
    pub snapshot: Option<AgentSkillsSnapshot>,
    pub loading: bool,
    pub error: Option<AgentSkillsError>,
    /// True when `skills/changed` arrived after the last successful read.
    pub stale: bool,
    writes: BTreeMap<String, SkillWrite>,
    sequence: u64,
    applied: BTreeMap<String, u64>,
}

impl SkillsDirectory {
    pub fn for_cwd(cwd: PathBuf) -> Self {
        Self {
            cwd,
            ..Default::default()
        }
    }

    /// Starts a refresh and returns its cycle. Only the newest cycle may be
    /// applied, so an older response can never roll back newer data.
    pub fn begin_refresh(&mut self) -> u64 {
        self.cycle += 1;
        self.loading = true;
        self.cycle
    }

    pub fn accept_snapshot(&mut self, cycle: u64, snapshot: AgentSkillsSnapshot) {
        if cycle != self.cycle {
            return;
        }
        self.loading = false;
        self.error = None;
        self.stale = false;
        // A served value retires its own local write; pending and failed
        // operations keep the user's intent so nothing is edited twice.
        let confirmed = snapshot
            .skills()
            .filter_map(|skill| {
                let key = skill_key(skill);
                let write = self.writes.get(&key)?;
                match write.phase {
                    SkillWritePhase::Saved { effective_enabled }
                        if effective_enabled == skill.enabled =>
                    {
                        Some(key)
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        for key in confirmed {
            self.writes.remove(&key);
        }
        self.snapshot = Some(snapshot);
    }

    pub fn fail_refresh(&mut self, cycle: u64, error: AgentSkillsError) {
        if cycle != self.cycle {
            return;
        }
        self.loading = false;
        // A failed read keeps the previous snapshot visible, but the error is
        // explicit so the view can offer a retry.
        self.error = Some(error);
    }

    /// Records the user's intent and the sequence that will confirm it.
    pub fn begin_write(&mut self, selector: AgentSkillSelector, enabled: bool) -> u64 {
        self.sequence += 1;
        let key = selector_key(&selector);
        self.writes.insert(
            key.clone(),
            SkillWrite {
                key,
                selector,
                sequence: self.sequence,
                intended_enabled: enabled,
                phase: SkillWritePhase::Saving,
            },
        );
        self.sequence
    }

    pub fn accept_receipt(&mut self, sequence: u64, receipt: AgentSkillWriteReceipt) {
        let Some(write) = self
            .writes
            .values_mut()
            .find(|write| write.sequence == sequence)
        else {
            return;
        };
        write.phase = SkillWritePhase::Saved {
            effective_enabled: receipt.effective_enabled,
        };
        self.applied.insert(write.key.clone(), sequence);
    }

    pub fn fail_write(&mut self, sequence: u64, error: AgentSkillsError, intended_enabled: bool) {
        let Some(write) = self
            .writes
            .values_mut()
            .find(|write| write.sequence == sequence)
        else {
            return;
        };
        write.intended_enabled = intended_enabled;
        write.phase = SkillWritePhase::Failed {
            message: error.user_message(),
            outcome_unknown: error.outcome_unknown,
        };
    }

    /// `skills/changed` invalidates cached data without discarding pending or
    /// failed local operations.
    pub fn note_changed(&mut self) {
        self.stale = true;
    }

    pub fn write_for(&self, selector: &AgentSkillSelector) -> Option<&SkillWrite> {
        self.writes.get(&selector_key(selector))
    }

    pub fn display_enabled(&self, skill: &AgentSkill) -> bool {
        match self.write_for(&skill.selector()) {
            Some(write) => write.displayed_enabled(),
            None => skill.enabled,
        }
    }

    pub fn busy(&self) -> bool {
        self.writes.values().any(SkillWrite::busy)
    }

    pub fn retry_target(&self, selector: &AgentSkillSelector) -> Option<bool> {
        match self.write_for(selector)?.phase {
            SkillWritePhase::Failed { .. } => {
                Some(self.write_for(selector).unwrap().intended_enabled)
            }
            _ => None,
        }
    }

    pub fn skill_count(&self) -> usize {
        self.snapshot.as_ref().map_or(0, |snapshot| {
            snapshot
                .entries
                .iter()
                .map(|entry| entry.skills.len())
                .sum()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentSkillScope, AgentSkillsEntry};

    fn skill(name: &str, enabled: bool) -> AgentSkill {
        AgentSkill {
            name: name.into(),
            description: "d".into(),
            path: PathBuf::from(format!("/skills/{name}/SKILL.md")),
            scope: AgentSkillScope::User,
            enabled,
            short_description: None,
            plugin_id: None,
            interface: None,
            dependencies: None,
            extra: Default::default(),
        }
    }

    fn snapshot(generation: u64, skills: Vec<AgentSkill>) -> AgentSkillsSnapshot {
        AgentSkillsSnapshot {
            generation,
            entries: vec![AgentSkillsEntry {
                cwd: PathBuf::from("/project"),
                skills,
                errors: Vec::new(),
                extra: Default::default(),
            }],
            next_cursor: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn changed_notification_invalidates_without_dropping_pending_write() {
        let mut directory = SkillsDirectory::for_cwd("/project".into());
        let cycle = directory.begin_refresh();
        directory.accept_snapshot(cycle, snapshot(1, vec![skill("alpha", true)]));
        let sequence = directory.begin_write(skill("alpha", true).selector(), false);
        directory.note_changed();
        let refresh = directory.begin_refresh();
        directory.accept_snapshot(refresh, snapshot(1, vec![skill("alpha", true)]));
        let current = skill("alpha", true);
        assert!(!directory.display_enabled(&current));
        assert!(directory.write_for(&current.selector()).unwrap().busy());
        directory.accept_receipt(
            sequence,
            AgentSkillWriteReceipt {
                effective_enabled: false,
                extra: Default::default(),
            },
        );
        assert!(!directory.display_enabled(&current));
    }

    #[test]
    fn stale_read_cycle_cannot_roll_back_newer_data() {
        let mut directory = SkillsDirectory::for_cwd("/project".into());
        let first = directory.begin_refresh();
        let second = directory.begin_refresh();
        directory.accept_snapshot(second, snapshot(2, vec![skill("alpha", false)]));
        directory.accept_snapshot(first, snapshot(1, vec![skill("alpha", true)]));
        let current = &directory.snapshot.as_ref().unwrap().entries[0].skills[0];
        assert!(!current.enabled);
    }

    #[test]
    fn failed_write_keeps_intent_for_retry() {
        let mut directory = SkillsDirectory::for_cwd("/project".into());
        let cycle = directory.begin_refresh();
        directory.accept_snapshot(cycle, snapshot(1, vec![skill("alpha", true)]));
        let sequence = directory.begin_write(skill("alpha", true).selector(), false);
        directory.fail_write(
            sequence,
            AgentSkillsError {
                kind: crate::agent::AgentSkillsErrorKind::Protocol,
                message: "server said no".into(),
                data: None,
                outcome_unknown: false,
            },
            false,
        );
        assert_eq!(
            directory.retry_target(&skill("alpha", true).selector()),
            Some(false)
        );
        assert!(!directory.display_enabled(&skill("alpha", true)));
    }
}
