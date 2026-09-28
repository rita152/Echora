//! The "Edit goal" tab: the composer asks for it, the file panel shows it,
//! and saving goes back to the composer that owns the thread's goal.
use super::{ChatApp, ConversationKey};
use crate::{
    agent::AgentThreadGoal,
    components::{
        composer::ComposerView,
        file_panel::{FilePanel, SaveGoalObjective},
    },
};
use gpui::{Context, Entity};

impl ChatApp {
    pub(super) fn open_goal_editor(
        &mut self,
        goal: AgentThreadGoal,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let visible = self.right_panel.open
            && self.right_panel.mode == Some(super::state::RightPanelMode::Files);
        self.right_panel.open = true;
        self.select_right_panel_item(3, cx);
        if let Some(panel) = self.file_panels.get(&self.active_conversation) {
            panel.update(cx, |panel, cx| panel.open_goal(&goal, &text, visible, cx));
        }
        cx.notify();
    }

    pub(super) fn watch_goal_saves(&mut self, panel: &Entity<FilePanel>, cx: &mut Context<Self>) {
        cx.subscribe(panel, |this, _, save: &SaveGoalObjective, cx| {
            let key = ConversationKey::Thread(save.thread_id.clone());
            if let Some(host) = this.conversation_hosts.get(&key) {
                let objective = save.objective.clone();
                host.composer.update(cx, |composer, cx| {
                    composer.save_goal_objective(objective, cx)
                });
            }
        })
        .detach();
    }

    /// Pushes the composer's goal to any open "Edit goal" tab of its thread.
    pub(super) fn sync_goal_tabs(
        &mut self,
        composer: &Entity<ComposerView>,
        cx: &mut Context<Self>,
    ) {
        let Some((thread_id, goal, text, saving, error)) =
            composer.read(cx).goal_tab_sync().map(|(thread_id, sync)| {
                (
                    thread_id,
                    sync.goal.cloned(),
                    sync.text,
                    sync.saving,
                    sync.error,
                )
            })
        else {
            return;
        };
        for panel in self.file_panels.values() {
            panel.update(cx, |panel, cx| {
                panel.sync_goal(
                    &thread_id,
                    crate::components::file_panel::GoalTabSync {
                        goal: goal.as_ref(),
                        text: text.clone(),
                        saving,
                        error: error.clone(),
                    },
                    cx,
                )
            });
        }
    }
}
