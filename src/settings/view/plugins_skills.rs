//! Skills management inside the plugins settings page. The segment reads the
//! backend inventory, keeps pending writes across refreshes, and adopts the
//! server receipt instead of predicting it.

use gpui::{Context, div, prelude::*, px};

use std::hash::{Hash, Hasher};

/// Stable numeric identity for stateful elements keyed by a server string.
fn id_hash(value: &str) -> usize {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish() as usize
}

use super::SettingsView;
use crate::{
    agent::{AgentSkill, AgentSkillScope, AgentSkillsError, AgentSkillsLoadRequest},
    skills::{SkillWritePhase, SkillsDirectory},
    theme::Theme,
};

#[derive(Default)]
pub(super) struct SkillsPanel {
    pub directory: SkillsDirectory,
    /// Capture-only hovered row.
    pub hover_row: Option<String>,
    pub query: String,
}

impl SkillsPanel {
    pub(super) fn skill_count(&self) -> usize {
        self.directory.skill_count()
    }

    pub(super) fn visible_skills(&self) -> Vec<&AgentSkill> {
        let query = self.query.trim().to_lowercase();
        self.directory
            .snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .skills()
                    .filter(|skill| {
                        query.is_empty()
                            || skill.name.to_lowercase().contains(&query)
                            || skill.display_name().to_lowercase().contains(&query)
                            || skill.summary().to_lowercase().contains(&query)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl SettingsView {
    pub(super) fn refresh_skills(&mut self, force_reload: bool, cx: &mut Context<Self>) {
        let cycle = self.skills.directory.begin_refresh();
        cx.notify();
        let backend = self.backend.clone();
        // The directory owns the working directory its cache belongs to.
        let cwd = self.skills.directory.cwd.clone();
        cx.spawn(async move |this, cx| {
            let request = AgentSkillsLoadRequest {
                cwds: vec![cwd],
                force_reload,
            };
            let result = backend.load_skills(request).recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(snapshot)) => {
                        this.skills_generation = snapshot.generation;
                        this.skills.directory.accept_snapshot(cycle, snapshot);
                    }
                    Ok(Err(error)) => this.skills.directory.fail_refresh(cycle, error),
                    Err(_) => this.skills.directory.fail_refresh(
                        cycle,
                        AgentSkillsError {
                            kind: crate::agent::AgentSkillsErrorKind::Connection,
                            message: "技能列表连接已关闭".into(),
                            data: None,
                            outcome_unknown: false,
                        },
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Retries a failed write with the value the user originally asked for.
    /// The switch never flips to the opposite value as a side effect of a
    /// failure, so a retry cannot silently change intent.
    pub(super) fn retry_skill_write(&mut self, skill: &AgentSkill, cx: &mut Context<Self>) {
        let selector = skill.selector();
        let Some(target) = self.skills.directory.retry_target(&selector) else {
            return;
        };
        if self
            .skills
            .directory
            .write_for(&selector)
            .is_some_and(|write| write.busy())
        {
            return;
        }
        let sequence = self.skills.directory.begin_write(selector.clone(), target);
        cx.notify();
        let backend = self.backend.clone();
        let generation = self.skills_generation;
        cx.spawn(async move |this, cx| {
            let request = crate::agent::AgentSkillWriteRequest {
                generation,
                selector,
                enabled: target,
            };
            let result = backend.write_skill_config(request).recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(receipt)) => {
                        this.skills.directory.accept_receipt(sequence, receipt);
                        this.refresh_skills(true, cx);
                    }
                    Ok(Err(error)) => this.skills.directory.fail_write(sequence, error, target),
                    Err(_) => this.skills.directory.fail_write(
                        sequence,
                        AgentSkillsError {
                            kind: crate::agent::AgentSkillsErrorKind::Connection,
                            message: "重试连接已关闭".into(),
                            data: None,
                            outcome_unknown: true,
                        },
                        target,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Writes one skill's enabled flag. Only the selector the user acted on is
    /// submitted, and the switch keeps the user's intent until the server
    /// answers with its effective value.
    pub(super) fn toggle_skill(&mut self, skill: &AgentSkill, cx: &mut Context<Self>) {
        let selector = skill.selector();
        if self
            .skills
            .directory
            .write_for(&selector)
            .is_some_and(|write| write.busy())
        {
            return;
        }
        let intended = !self.skills.directory.display_enabled(skill);
        let sequence = self
            .skills
            .directory
            .begin_write(selector.clone(), intended);
        cx.notify();
        let backend = self.backend.clone();
        let generation = self.skills_generation;
        cx.spawn(async move |this, cx| {
            let request = crate::agent::AgentSkillWriteRequest {
                generation,
                selector,
                enabled: intended,
            };
            let result = backend.write_skill_config(request).recv().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(receipt)) => {
                        this.skills.directory.accept_receipt(sequence, receipt);
                        // The receipt is authoritative: re-read so the list
                        // shows server state rather than our intent.
                        this.refresh_skills(true, cx);
                    }
                    Ok(Err(error)) => this.skills.directory.fail_write(sequence, error, intended),
                    Err(_) => this.skills.directory.fail_write(
                        sequence,
                        AgentSkillsError {
                            kind: crate::agent::AgentSkillsErrorKind::Connection,
                            message: "保存连接已关闭".into(),
                            data: None,
                            outcome_unknown: true,
                        },
                        intended,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Applies a `skills/changed` notification: invalidate, then re-read.
    /// Cached data is dropped; a pending or failed local write is never.
    pub(super) fn apply_skills_changed(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.skills_generation = generation;
        self.skills.directory.note_changed();
        self.refresh_skills(true, cx);
    }

    pub(super) fn skills_segment_content(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let skills = self.skills.visible_skills();
        let mut list = div().mt(px(44.0)).flex().flex_col().gap(px(6.0));

        let load_errors = self
            .skills
            .directory
            .snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .entries
                    .iter()
                    .flat_map(|entry| entry.errors.iter())
                    .map(|error| format!("{}：{}", error.path.display(), error.message))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if self.skills.directory.loading && self.skills.directory.snapshot.is_none() {
            list = list.child(self.manage_state_card("正在读取技能…", theme));
        }
        if self.skills.directory.busy() {
            list = list.child(self.manage_state_card("正在保存技能设置…", theme));
        }
        if let Some(error) = self.skills.directory.error.clone() {
            list = list.child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(19.0))
                            .text_color(theme.warning)
                            .child(error.user_message()),
                    )
                    .child(
                        div()
                            .id("skills-retry")
                            .flex_none()
                            .h(px(28.0))
                            .px(px(12.0))
                            .rounded(px(12.5))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.settings_button)
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_skills(true, cx)))
                            .child("重试"),
                    ),
            );
        }
        if skills.is_empty()
            && !self.skills.directory.loading
            && self.skills.directory.error.is_none()
        {
            let message = if self.skills.query.trim().is_empty() {
                "还没有可管理的技能"
            } else {
                "没有匹配的技能"
            };
            list = list.child(self.manage_state_card(message, theme));
        }

        let mut rows = div()
            .rounded(px(20.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .overflow_hidden()
            .flex()
            .flex_col();
        for (index, skill) in skills.iter().enumerate() {
            let skill = (*skill).clone();
            let enabled = self.skills.directory.display_enabled(&skill);
            let write = self.skills.directory.write_for(&skill.selector());
            let busy = write.is_some_and(|write| write.busy());
            let failed =
                write.is_some_and(|write| matches!(write.phase, SkillWritePhase::Failed { .. }));
            let failure = write.and_then(|write| match &write.phase {
                SkillWritePhase::Failed {
                    message,
                    outcome_unknown,
                } => Some((message.clone(), *outcome_unknown)),
                _ => None,
            });
            let row_skill = skill.clone();
            let hovered = self.skills.hover_row.as_deref() == Some(skill.name.as_str());
            rows = rows.child(
                div()
                    .id(("skill-row", index))
                    .flex()
                    .flex_col()
                    .when(index > 0, |row| row.border_t_1().border_color(theme.border))
                    .child(
                        div()
                            .h(px(60.0))
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .when(hovered, |row| row.bg(theme.settings_control))
                            .hover(|row| row.bg(theme.settings_control))
                            .child(self.skill_icon(&skill, theme))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.0))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .line_height(px(19.0))
                                            .font_weight(gpui::FontWeight(500.0))
                                            .text_color(theme.text)
                                            .child(skill.display_name()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .line_height(px(19.0))
                                            .text_color(theme.settings_description)
                                            .child(skill.summary()),
                                    ),
                            )
                            .child(self.scope_tag(skill.scope, theme))
                            .child(self.skill_switch(&row_skill, enabled, busy, failed, theme, cx)),
                    )
                    .when_some(failure, |row, (message, outcome_unknown)| {
                        let row_skill = row_skill.clone();
                        row.child(
                            div()
                                .px(px(16.0))
                                .pb(px(10.0))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .line_height(px(18.0))
                                        .text_color(theme.warning)
                                        .child(if outcome_unknown {
                                            format!("{message}（结果未确认）")
                                        } else {
                                            message
                                        }),
                                )
                                .child(
                                    div()
                                        .id((
                                            "skill-retry",
                                            id_hash(&crate::skills::skill_key(&row_skill)),
                                        ))
                                        .flex_none()
                                        .h(px(24.0))
                                        .px(px(10.0))
                                        .rounded(px(12.0))
                                        .border_1()
                                        .border_color(theme.border)
                                        .bg(theme.settings_button)
                                        .text_size(px(12.0))
                                        .line_height(px(18.0))
                                        .flex()
                                        .items_center()
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.retry_skill_write(&row_skill, cx)
                                        }))
                                        .child("重试"),
                                ),
                        )
                    }),
            );
        }
        list = list.child(rows);

        for message in load_errors {
            list = list.child(self.manage_state_card(&message, theme));
        }
        list.into_any_element()
    }

    pub(super) fn manage_state_card(&self, message: &str, theme: &Theme) -> impl IntoElement {
        div()
            .px(px(16.0))
            .py(px(12.0))
            .rounded(px(20.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .text_size(px(13.0))
            .line_height(px(19.0))
            .text_color(theme.settings_description)
            .child(message.to_owned())
    }

    fn skill_icon(&self, skill: &AgentSkill, theme: &Theme) -> impl IntoElement {
        let icon = skill.icon_path().cloned();
        div()
            .size(px(40.0))
            .flex_none()
            .rounded(px(10.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_control)
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .when_some(icon, |row, path| row.child(gpui::img(path).size(px(24.0))))
    }

    fn scope_tag(&self, scope: AgentSkillScope, theme: &Theme) -> impl IntoElement {
        div()
            .flex_none()
            .h(px(20.0))
            .px(px(8.0))
            .rounded(px(10.0))
            .bg(theme.settings_control)
            .text_size(px(12.0))
            .line_height(px(18.0))
            .text_color(theme.text_tertiary)
            .flex()
            .items_center()
            .child(scope.label())
    }

    fn skill_switch(
        &self,
        skill: &AgentSkill,
        enabled: bool,
        busy: bool,
        failed: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let skill = skill.clone();
        div()
            .id(("skill-switch", id_hash(&crate::skills::skill_key(&skill))))
            .w(px(32.0))
            .h(px(20.0))
            .flex_none()
            .p(px(2.0))
            .rounded_full()
            .flex()
            .items_center()
            .when(enabled, |track| {
                track.justify_end().bg(theme.settings_accent)
            })
            .when(!enabled, |track| {
                track.justify_start().bg(theme.settings_switch_off)
            })
            .when(failed, |track| track.border_1().border_color(theme.warning))
            .when(busy, |track| track.opacity(0.6))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_skill(&skill, cx)))
            .child(
                div()
                    .size(px(16.0))
                    .rounded_full()
                    .bg(gpui::white())
                    .border_1()
                    .border_color(gpui::rgba(0x00000012)),
            )
    }
}
