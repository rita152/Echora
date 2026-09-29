//! Experimental feature and memory settings state, independent of GPUI.
//!
//! The feature list is read once per page visit (all pages) and after each
//! write. The memories section and `/memories` read the `memories` entry from
//! the same list, as the reference does.

use crate::agent::{AgentExperimentalFeature, AgentExperimentalFeatures};

/// A switch write in flight or its result, kept until the next write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchWrite {
    /// Which switch: a feature name, or a memories control.
    pub target: String,
    pub intended: bool,
    pub in_flight: bool,
    pub failure: Option<String>,
    pub overridden: bool,
}

#[derive(Default)]
pub struct FeatureDirectory {
    pub cycle: u64,
    pub list: Option<AgentExperimentalFeatures>,
    pub loading: bool,
    pub error: Option<String>,
    pub write: Option<SwitchWrite>,
    /// Generation in which a feature was changed: the running app-server keeps
    /// the flags it started with, so a new connection is needed to apply it.
    pub changed_in_generation: Option<u64>,
    /// A capture fixture is shown: backend reads must not replace it.
    pub capture_fixture: bool,
}

impl FeatureDirectory {
    pub fn begin_refresh(&mut self) -> u64 {
        self.cycle += 1;
        self.loading = true;
        self.cycle
    }

    pub fn accept(
        &mut self,
        cycle: u64,
        result: Result<AgentExperimentalFeatures, String>,
    ) -> bool {
        if cycle != self.cycle {
            return false;
        }
        self.loading = false;
        match result {
            Ok(list) => {
                // A different connection has loaded the flags anew.
                if self
                    .changed_in_generation
                    .is_some_and(|generation| generation != list.generation)
                {
                    self.changed_in_generation = None;
                }
                self.error = None;
                self.list = Some(list);
            }
            Err(error) => self.error = Some(error),
        }
        true
    }

    /// The rows of "Experimental features (Beta)", in server order.
    pub fn settings_rows(&self) -> Vec<&AgentExperimentalFeature> {
        self.list
            .iter()
            .flat_map(|list| &list.features)
            .filter(|feature| feature.listed_in_settings())
            .collect()
    }

    pub fn memories(&self) -> Option<&AgentExperimentalFeature> {
        self.list.as_ref()?.get("memories")
    }

    pub fn begin_write(&mut self, target: String, intended: bool) -> bool {
        if self.writing() {
            return false;
        }
        self.write = Some(SwitchWrite {
            target,
            intended,
            in_flight: true,
            failure: None,
            overridden: false,
        });
        true
    }

    pub fn writing(&self) -> bool {
        self.write.as_ref().is_some_and(|write| write.in_flight)
    }

    /// The value a switch shows: the intent while its write is in flight.
    pub fn displayed(&self, target: &str, served: bool) -> bool {
        self.write
            .as_ref()
            .filter(|write| write.in_flight && write.target == target)
            .map_or(served, |write| write.intended)
    }

    pub fn failure_for(&self, target: &str) -> Option<&SwitchWrite> {
        self.write
            .as_ref()
            .filter(|write| write.target == target && (write.failure.is_some() || write.overridden))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentExperimentalFeatureStage;

    fn list(generation: u64, enabled: bool) -> AgentExperimentalFeatures {
        let feature = |name: &str, stage| AgentExperimentalFeature {
            name: name.into(),
            stage,
            display_name: Some(name.into()),
            description: None,
            announcement: None,
            enabled,
            default_enabled: false,
        };
        AgentExperimentalFeatures {
            generation,
            features: vec![
                feature("network_proxy", AgentExperimentalFeatureStage::Beta),
                feature("memories", AgentExperimentalFeatureStage::Stable),
                feature("undo", AgentExperimentalFeatureStage::Removed),
                feature("prevent_idle_sleep", AgentExperimentalFeatureStage::Beta),
            ],
        }
    }

    #[test]
    fn rows_are_beta_flags_and_restart_note_clears_on_a_new_connection() {
        let mut directory = FeatureDirectory::default();
        let stale = directory.begin_refresh();
        let current = directory.begin_refresh();
        assert!(!directory.accept(stale, Ok(list(1, true))));
        assert!(directory.accept(current, Ok(list(1, false))));
        assert_eq!(
            directory
                .settings_rows()
                .iter()
                .map(|feature| feature.name.as_str())
                .collect::<Vec<_>>(),
            ["network_proxy", "prevent_idle_sleep"]
        );
        assert!(directory.memories().is_some());
        directory.changed_in_generation = Some(1);
        let same = directory.begin_refresh();
        directory.accept(same, Ok(list(1, true)));
        assert_eq!(directory.changed_in_generation, Some(1));
        let next = directory.begin_refresh();
        directory.accept(next, Ok(list(2, true)));
        assert_eq!(directory.changed_in_generation, None);
    }

    #[test]
    fn one_write_at_a_time_and_its_intent_shows_until_settled() {
        let mut directory = FeatureDirectory::default();
        assert!(directory.begin_write("network_proxy".into(), true));
        assert!(!directory.begin_write("prevent_idle_sleep".into(), true));
        assert!(directory.displayed("network_proxy", false));
        assert!(!directory.displayed("prevent_idle_sleep", false));
        let write = directory.write.as_mut().unwrap();
        write.in_flight = false;
        write.failure = Some("conflict".into());
        assert!(!directory.displayed("network_proxy", false));
        assert!(directory.failure_for("network_proxy").is_some());
    }
}
