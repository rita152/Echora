//! Configuration → "Experimental features (Beta)" and the switch writes it
//! shares with Personalization → Codex memory.

use gpui::{AnyElement, Context, IntoElement, Role, SharedString, div, prelude::*, px};

use super::{
    SettingsView,
    dynamic::{card, danger_color, heading, row, switch},
};
use crate::{
    agent::{AgentConfigEdit, AgentConfigError, AgentConfigErrorKind},
    configuration::ImmediateWriteOutcome,
    theme::Theme,
};

impl SettingsView {
    /// Reads every page of `experimentalFeature/list`.
    pub(super) fn refresh_features(&mut self, cx: &mut Context<Self>) {
        if self.features.capture_fixture {
            return;
        }
        let cycle = self.features.begin_refresh();
        let receiver = self.backend.list_experimental_features(None);
        cx.spawn(async move |this, cx| {
            let result = receiver
                .recv()
                .await
                .unwrap_or_else(|_| Err(crate::i18n::text("实验性功能读取连接已关闭").into()));
            let _ = this.update(cx, |this, cx| {
                if this.features.accept(cycle, result) {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// A switch that saves at once: only its own edits, at the user layer's
    /// version, then a readback; the feature list is re-read afterwards. A
    /// failure is shown and the switch returns to the served value.
    pub(super) fn write_switch(
        &mut self,
        target: String,
        intended: bool,
        edits: Vec<AgentConfigEdit>,
        feature_change: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.features.begin_write(target, intended) {
            return;
        }
        let write = match self.config_editor.prepare_immediate_write(edits) {
            Ok(write) => write,
            Err(message) => {
                if let Some(state) = &mut self.features.write {
                    state.in_flight = false;
                    state.failure = Some(message);
                }
                cx.notify();
                return;
            }
        };
        let generation = write.generation;
        let receiver = self.backend.write_config(write.clone());
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(AgentConfigError {
                    kind: AgentConfigErrorKind::Connection,
                    message: crate::i18n::text("保存连接已关闭").into(),
                    data: None,
                    outcome_unknown: true,
                })
            });
            let _ = this.update(cx, |this, cx| {
                let outcome = this.config_editor.accept_immediate_save(&write, result);
                // A conflict or unknown outcome needs a fresh version before
                // the next explicit change.
                if outcome.is_err() {
                    this.reload_config(cx);
                }
                if let Some(state) = &mut this.features.write {
                    state.in_flight = false;
                    match outcome {
                        Ok(ImmediateWriteOutcome::Saved) => {}
                        Ok(ImmediateWriteOutcome::Overridden(_)) => state.overridden = true,
                        Ok(ImmediateWriteOutcome::Differs(keys)) => {
                            state.failure = Some(crate::i18n::format!(
                                "回读与写入不同：{}" => "Readback differs from the write: {}",
                                keys.join("、")
                            ))
                        }
                        Err(error) => state.failure = Some(error.user_message()),
                    }
                    let saved = state.failure.is_none();
                    if saved && feature_change {
                        this.features.changed_in_generation = Some(generation);
                    }
                }
                this.refresh_features(cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn experimental_features_section(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let directory = &self.features;
        let note = directory.changed_in_generation.is_some().then(|| {
            div()
                .mb(px(8.0))
                .text_size(px(14.0))
                .line_height(px(21.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(danger_color(self.mode))
                .child(crate::i18n::format!(
                    "重启 Echora（新建 Codex 连接）以应用实验性功能更改" =>
                    "Restart Echora (a new Codex connection) to apply experimental feature changes"
                ))
                .into_any_element()
        });
        let rows = directory.settings_rows();
        let mut list = card(theme);
        if directory.list.is_none() && directory.loading {
            list = list.child(row(
                crate::i18n::format!("正在加载实验性质功能…" => "Loading experimental features…"),
                None,
                None,
                true,
                theme,
            ));
        } else if let (None, Some(error)) = (&directory.list, &directory.error) {
            list = list.child(row(
                crate::i18n::format!("无法加载实验性功能" => "Could not load experimental features"),
                Some(error.clone()),
                None,
                true,
                theme,
            ));
        } else if rows.is_empty() {
            list = list.child(row(
                crate::i18n::format!("没有可用的测试版实验性功能" => "No beta experimental features available"),
                None,
                None,
                true,
                theme,
            ));
        }
        let writing = directory.writing();
        for (index, feature) in rows.iter().enumerate() {
            let name = feature.name.clone();
            let checked = directory.displayed(&name, feature.enabled);
            let label = feature.label().to_owned();
            let toggle = switch(
                SharedString::from(format!("feature-{name}")),
                checked,
                writing,
                theme,
            )
            .aria_label(crate::i18n::format!("切换 {label}" => "Toggle {label}"))
            .when(!writing, |toggle| {
                let edit = feature.edit(!checked);
                toggle.on_click(cx.listener(move |this, _, _, cx| {
                    this.write_switch(name.clone(), !checked, vec![edit.clone()], true, cx)
                }))
            });
            let failure = directory.failure_for(&feature.name).map(|write| {
                if write.overridden {
                    crate::i18n::format!("已写入，但被更高优先级配置覆盖" => "Saved, but a higher-priority config layer overrides it")
                } else {
                    write.failure.clone().unwrap_or_default()
                }
            });
            let description = match (&feature.description, failure) {
                (_, Some(failure)) => Some(failure),
                (Some(description), None) => Some(description.clone()),
                (None, None) => None,
            };
            list = list.child(row(
                label,
                description,
                Some(toggle.into_any_element()),
                index + 1 == rows.len(),
                theme,
            ));
        }
        div()
            .id("experimental-features")
            .role(Role::Group)
            .mt(px(40.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(heading(
                crate::i18n::format!("实验性质功能（测试版）" => "Experimental features (Beta)"),
                note,
                theme,
            ))
            .child(list)
            .into_any_element()
    }
}

#[cfg(feature = "screenshot")]
impl SettingsView {
    /// Capture-only: the reference's three beta flags (plus flags the list
    /// hides) in a named state, with backend reads suppressed.
    pub fn apply_features_capture_fixture(&mut self, state: &str, cx: &mut Context<Self>) {
        use crate::agent::{
            AgentExperimentalFeature, AgentExperimentalFeatureStage, AgentExperimentalFeatures,
        };
        let feature =
            |name: &str, label: Option<&str>, description: Option<&str>, stage, enabled| {
                AgentExperimentalFeature {
                    name: name.into(),
                    stage,
                    display_name: label.map(Into::into),
                    description: description.map(Into::into),
                    announcement: None,
                    enabled,
                    default_enabled: false,
                }
            };
        use AgentExperimentalFeatureStage::*;
        let enabled = state == "restart";
        let list = AgentExperimentalFeatures {
            generation: 1,
            features: vec![
                feature(
                    "analytics_plan_history",
                    Some("Analytics plan history"),
                    Some(
                        "Preview five-hour and weekly allowance history for consumer accounts in /analytics.",
                    ),
                    Beta,
                    enabled,
                ),
                feature("memories", None, None, Stable, false),
                feature(
                    "network_proxy",
                    Some("Network proxy"),
                    Some(
                        "Apply network proxy restrictions to sandboxed sessions that already have network access.",
                    ),
                    Beta,
                    false,
                ),
                feature(
                    "prevent_idle_sleep",
                    Some("Prevent sleep while running"),
                    Some("Keep your computer awake while Codex is running a thread."),
                    Beta,
                    false,
                ),
                feature("undo", None, None, Removed, false),
            ],
        };
        self.features = crate::features::FeatureDirectory {
            list: (!matches!(state, "loading" | "empty" | "error")).then_some(list),
            loading: state == "loading",
            error: (state == "error").then(|| "experimentalFeature/list failed".to_owned()),
            changed_in_generation: (state == "restart").then_some(1),
            capture_fixture: true,
            ..Default::default()
        };
        if state == "empty" {
            self.features.list = Some(AgentExperimentalFeatures {
                generation: 1,
                features: Vec::new(),
            });
        }
        cx.notify();
    }
}
