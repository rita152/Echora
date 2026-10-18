//! The thread summary panel beside the active chat: which chat it summarises,
//! where it sits for the window width, its toggle, and what its rows open.

use gpui::{Context, Entity};

use super::{ChatApp, ConversationKey};
use crate::components::{
    composer::ComposerView,
    summary_panel::{SummaryPanel, SummaryPanelEvent},
};

/// Index of the Files and Review items in `RIGHT_PANEL_ITEMS`.
const FILES_ITEM: usize = 3;
const REVIEW_ITEM: usize = 4;

impl ChatApp {
    pub(super) fn wire_summary_panel(panel: &Entity<SummaryPanel>, cx: &mut Context<Self>) {
        cx.subscribe(
            panel,
            |this, _, event: &SummaryPanelEvent, cx| match event {
                SummaryPanelEvent::OpenBackgroundTerminal {
                    thread_id,
                    item_id,
                    title,
                    output,
                } => {
                    this.right_panel.open = true;
                    this.select_right_panel_item(FILES_ITEM, cx);
                    if let Some(panel) = this.file_panels.get(&this.active_conversation) {
                        panel.update(cx, |panel, cx| {
                            panel.open_background_terminal(
                                thread_id,
                                item_id,
                                title.clone(),
                                output.clone(),
                                cx,
                            )
                        });
                    }
                    cx.notify();
                }
                SummaryPanelEvent::ViewPullRequest(summary) => {
                    this.open_pull_requests(cx);
                    let summary = summary.clone();
                    this.pull_requests
                        .update(cx, |view, cx| view.select(summary, cx));
                }
                SummaryPanelEvent::EnvironmentSettings => {
                    this.open_settings_page("general-settings", cx);
                    this.settings
                        .update(cx, |settings, cx| settings.select_environment(cx));
                }
                SummaryPanelEvent::OpenEditorView => {
                    this.open_files(cx);
                }
                SummaryPanelEvent::EnvironmentGit(action) => {
                    this.ensure_review(cx);
                    if let Some(panel) = this.review_panels.get(&this.active_conversation) {
                        panel.update(cx, |panel, cx| panel.open_environment_git(*action, cx));
                    }
                }
                SummaryPanelEvent::OpenReview => {
                    this.right_panel.open = true;
                    this.select_right_panel_item(REVIEW_ITEM, cx);
                    cx.notify();
                }
            },
        )
        .detach();
    }

    /// Points the panel at the active chat.
    pub(super) fn sync_summary_panel(&mut self, cx: &mut Context<Self>) {
        let (composer, cwd) = self
            .conversation_hosts
            .get(&self.active_conversation)
            .map(|host| (Some(host.composer.clone()), Some(host.cwd.clone())))
            .unwrap_or_else(|| (Some(self.home.read(cx).composer_entity()), None));
        let thread_id = match &self.active_conversation {
            ConversationKey::Thread(id) => Some(id.clone()),
            _ => None,
        };
        self.summary_panel.update(cx, |panel, cx| {
            panel.set_conversation(composer, thread_id, cwd, cx)
        });
    }

    /// The header button shows or hides Environment and its reserved column.
    pub(super) fn toggle_summary_panel(&mut self, cx: &mut Context<Self>) {
        let pinned = !self
            .workspace_store
            .snapshot()
            .preferences
            .summary_panel_unpinned;
        self.workspace_store.set_summary_panel_pinned(!pinned);
        self.summary_panel
            .update(cx, |panel, cx| panel.reset_environment(cx));
        self.summary_reveal_serial += 1;
        cx.notify();
    }

    /// Whether Environment is enabled in the workspace preferences.
    pub(super) fn summary_panel_visible(&self) -> bool {
        !self
            .workspace_store
            .snapshot()
            .preferences
            .summary_panel_unpinned
    }

    /// Output of background terminals follows into their open tabs.
    pub(super) fn sync_background_terminal_tabs(
        &mut self,
        composer: &Entity<ComposerView>,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_id) = composer.read(cx).thread_id().map(str::to_owned) else {
            return;
        };
        let Some(panel) = self
            .file_panels
            .get(&ConversationKey::Thread(thread_id.clone()))
            .cloned()
        else {
            return;
        };
        let items = panel.read(cx).background_terminal_items();
        for (tab_thread, item_id) in items {
            if tab_thread != thread_id {
                continue;
            }
            let Some(command) = composer.read(cx).background_command(&item_id) else {
                continue;
            };
            panel.update(cx, |panel, cx| {
                panel.sync_background_terminal(&thread_id, &item_id, &command.output, cx)
            });
        }
    }

    /// The reference attaches a pull request created from the chat's Git
    /// actions to the chat (warning when that fails), then records the
    /// branch it was created from as the chat's branch.
    pub(super) fn attach_created_pull_request(
        &mut self,
        composer: Option<Entity<ComposerView>>,
        url: String,
        root: std::path::PathBuf,
        head_branch: String,
        cx: &mut Context<Self>,
    ) {
        let Some(composer) = composer else {
            return;
        };
        let Some(thread_id) = composer.read(cx).thread_id().map(str::to_owned) else {
            return;
        };
        if head_branch.trim().is_empty() {
            return;
        }
        let cwd = self
            .conversation_hosts
            .get(&ConversationKey::Thread(thread_id.clone()))
            .map(|host| host.cwd.clone())
            .unwrap_or_else(|| root.clone());
        let receiver = self.workspace_store.attach_pull_request(
            thread_id.clone(),
            url,
            Some(root.to_string_lossy().into_owned()),
            Some(head_branch.clone()),
        );
        let store = self.workspace_store.clone();
        cx.spawn(async move |_, cx| {
            let attached = receiver.recv().await.is_ok_and(|result| result.is_ok());
            if !attached {
                composer.update(cx, |composer, cx| {
                    composer.show_panel_toast(
                        crate::components::composer::ToastKind::Danger,
                        crate::i18n::format!(
                            "已创建拉取请求，但无法将其附加到此任务" =>
                            "Pull request created, but could not attach it to this task"
                        ),
                        None,
                        cx,
                    )
                });
            }
            store.update_thread_git_branch(thread_id, cwd, head_branch, root);
        })
        .detach();
    }
}
