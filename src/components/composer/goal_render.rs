//! Goal presentation: the status summary row in the tray and the Goal chip in
//! the composer footer.

use gpui::{AnyElement, Context, Role, SharedString, div, prelude::*, px};

use super::{
    ComposerView,
    followup_render::{TrayColors, tray_icon, tray_icon_button, tray_row},
};
use crate::{agent::AgentThreadGoalStatus, components::icons::icon, theme::Theme};

pub(crate) fn status_label(status: AgentThreadGoalStatus) -> String {
    match status {
        AgentThreadGoalStatus::Active => crate::i18n::format!("进行中的目标" => "Pursuing goal"),
        AgentThreadGoalStatus::Paused => crate::i18n::format!("已暂停的目标" => "Paused goal"),
        AgentThreadGoalStatus::Blocked => crate::i18n::format!("目标已停滞" => "Goal stalled"),
        AgentThreadGoalStatus::UsageLimited => {
            crate::i18n::format!("目标使用受限" => "Goal usage limited")
        }
        AgentThreadGoalStatus::BudgetLimited => crate::i18n::format!("目标受限" => "Goal limited"),
        AgentThreadGoalStatus::Complete => crate::i18n::format!("已达成目标" => "Goal achieved"),
    }
}

/// The reference's compact duration: `26s`, `3m 4s`, `1h 0m 5s`.
pub(crate) fn duration_label(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (hours, minutes, rest) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if crate::i18n::is_english() {
        if hours > 0 {
            format!("{hours}h {minutes}m {rest}s")
        } else if minutes > 0 {
            format!("{minutes}m {rest}s")
        } else {
            format!("{rest}s")
        }
    } else if hours > 0 {
        format!("{hours}小时 {minutes}分钟 {rest}秒")
    } else if minutes > 0 {
        format!("{minutes}分钟 {rest}秒")
    } else {
        format!("{rest}秒")
    }
}

/// The reference's "Goal achieved in {totalTime}": whole seconds with zero
/// units dropped, always in English units ("0s", "3m 12s", "5m", "1h 5m").
pub(crate) fn achieved_duration_label(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (days, hours, minutes, rest) = (
        seconds / 86_400,
        seconds % 86_400 / 3600,
        seconds % 3600 / 60,
        seconds % 60,
    );
    if seconds < 3600 {
        return match (minutes, rest) {
            (0, rest) => format!("{rest}s"),
            (minutes, 0) => format!("{minutes}m"),
            (minutes, rest) => format!("{minutes}m {rest}s"),
        };
    }
    [(days, "d"), (hours, "h"), (minutes, "m"), (rest, "s")]
        .into_iter()
        .filter(|(value, _)| *value > 0)
        .map(|(value, unit)| format!("{value}{unit}"))
        .collect::<Vec<_>>()
        .join(" ")
}

impl ComposerView {
    /// The reference mounts the goal row only for a goal that is not
    /// complete; a completing goal leaves the tray at once, and "Goal
    /// achieved" shows on the turn that achieved it instead.
    pub(super) fn goal_row_visible(&self) -> bool {
        self.conversation
            .goal
            .goal
            .as_ref()
            .is_some_and(|goal| goal.status != AgentThreadGoalStatus::Complete)
    }

    pub(super) fn goal_summary_row(
        &self,
        colors: &TrayColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.goal_row_visible() {
            return None;
        }
        let goal = self.conversation.goal.goal.as_ref()?;
        let status = goal.status;
        let busy = self.conversation.goal.pending.is_some();
        let toggle = match status {
            AgentThreadGoalStatus::Active => Some((
                "goal-pause",
                crate::i18n::format!("暂停目标" => "Pause goal"),
            )),
            AgentThreadGoalStatus::Paused
            | AgentThreadGoalStatus::Blocked
            | AgentThreadGoalStatus::UsageLimited => Some((
                "goal-resume",
                crate::i18n::format!("恢复目标" => "Resume goal"),
            )),
            AgentThreadGoalStatus::BudgetLimited | AgentThreadGoalStatus::Complete => None,
        };
        let mut meta = duration_label(goal.time_used_seconds);
        if let Some(budget) = goal.token_budget {
            meta = format!("{} / {budget} · {meta}", goal.tokens_used);
        }
        let error = self.conversation.goal.error.clone();
        let label = status_label(status);
        let objective = goal.objective.lines().next().unwrap_or_default().to_owned();
        let summary = format!("{label} {objective} • {meta}");
        let row = tray_row("goal-summary".into(), colors)
            .role(Role::Group)
            .aria_label(SharedString::from(summary))
            .child(tray_icon("goal-summary", colors))
            .child(
                div()
                    .id("goal-summary-open")
                    .flex_1()
                    .min_w(px(0.0))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(div().flex_none().text_color(colors.primary).child(label))
                    .child(div().min_w(px(0.0)).truncate().child(objective))
                    .child(div().flex_none().child("•"))
                    .child(div().flex_none().child(meta)),
            )
            .when_some(error, |row, error| {
                row.child(
                    div()
                        .id("goal-error")
                        .flex_none()
                        .max_w(px(200.0))
                        .truncate()
                        .text_color(gpui::rgba(0xe02e2aff))
                        .role(Role::Alert)
                        .child(error),
                )
            })
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        tray_icon_button(
                            "goal-clear".into(),
                            "queue-delete",
                            crate::i18n::format!("清除目标" => "Clear goal"),
                            colors,
                        )
                        .when(!busy, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.clear_goal(cx)))
                        }),
                    )
                    .when_some(toggle, |actions, (glyph, label)| {
                        actions.child(
                            tray_icon_button("goal-toggle".into(), glyph, label, colors).when(
                                !busy,
                                |button| {
                                    button.on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_goal_pause(cx)),
                                    )
                                },
                            ),
                        )
                    })
                    .child(
                        tray_icon_button(
                            "goal-edit".into(),
                            "goal-edit",
                            crate::i18n::format!("编辑目标" => "Edit goal"),
                            colors,
                        )
                        .when(!busy, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.edit_goal(cx)))
                        }),
                    ),
            );
        Some(row.into_any_element())
    }

    /// The footer chip shown while the composer holds a goal objective. Its
    /// icon turns into a clear mark on hover, as in the reference.
    pub(super) fn render_goal_chip(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.goal_draft {
            return None;
        }
        let muted: gpui::Hsla = theme.text.alpha(0.5).into();
        let label = crate::i18n::format!("目标" => "Goal");
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                .child(
                    div()
                        .id("composer-goal-chip")
                        .group("composer-goal-chip")
                        .role(Role::Button)
                        .aria_label(crate::i18n::format!("清除目标" => "Clear goal"))
                        .focusable()
                        .tab_stop(true)
                        .h(px(28.0))
                        .px(px(8.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(muted)
                        .cursor_pointer()
                        .hover(move |s| s.bg(theme.sidebar_hover))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_goal_draft(false, cx);
                            this.focus_prompt_pending = true;
                        }))
                        .child(
                            div()
                                .size(px(16.0))
                                .relative()
                                .child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .group_hover("composer-goal-chip", |s| s.invisible())
                                        .child(icon("goal-chip", muted).size(px(16.0))),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .invisible()
                                        .group_hover("composer-goal-chip", |s| s.visible())
                                        .child(icon("goal-chip-clear", muted).size(px(16.0))),
                                ),
                        )
                        .child(label),
                )
                .into_any_element(),
        )
    }
}
