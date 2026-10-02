//! Review behavior and presentation for the application shell.

use gpui::{Context, prelude::*};

use super::{ChatApp, image_preview::finder_reveal_command};
use crate::components::file_change::{
    DiffFileVisualState, DiffReviewEvent, DiffReviewPresentation,
};

impl ChatApp {
    /// ⌃⇧G and the summary's changes: the chat's Changes tab, opened after
    /// the others when it has none.
    pub(super) fn open_review(&mut self, cx: &mut Context<Self>) {
        self.show_or_open_panel_tool(super::state::RightPanelMode::Review, cx);
    }
    pub(super) fn deactivate_review(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.review_panels.get(&self.active_conversation) {
            panel.update(cx, |p, _| p.deactivate());
        }
    }
    pub(super) fn ensure_review(&mut self, cx: &mut Context<Self>) {
        use crate::components::review_panel::{ReviewEvent, ReviewPanel};
        let key = self.active_conversation.clone();
        if !self.review_panels.contains_key(&key) {
            let cwd = self
                .conversation_hosts
                .get(&key)
                .map(|h| h.cwd.clone())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let panel = cx.new(|cx| ReviewPanel::new(cwd, self.mode, cx));
            panel.update(cx, |p, cx| {
                p.apply_preferences(self.workspace_store.snapshot().preferences.review, cx)
            });
            cx.observe(&panel, |_, _, cx| cx.notify()).detach();
            let composer = self
                .conversation_hosts
                .get(&key)
                .map(|h| h.composer.clone());
            cx.subscribe(&panel, move |s, _, event: &ReviewEvent, cx| match event {
                ReviewEvent::OpenFile { path, line } => {
                    let (path, line) = (std::path::PathBuf::from(path), *line);
                    s.open_in_files(|panel, cx| panel.open_path(path, line, cx), cx);
                    cx.notify();
                }
                ReviewEvent::CommentsChanged(comments) => {
                    if let Some(composer) = &composer {
                        composer.update(cx, |c, cx| c.set_review_comments(comments.clone(), cx));
                    }
                }
                ReviewEvent::PreferencesChanged(p) => {
                    s.workspace_store.set_review_preferences(p.clone())
                }
                ReviewEvent::PullRequestCreated {
                    url,
                    root,
                    head_branch,
                } => s.attach_created_pull_request(
                    composer.clone(),
                    url.clone(),
                    root.clone(),
                    head_branch.clone(),
                    cx,
                ),
            })
            .detach();
            if let Some(host) = self.conversation_hosts.get(&key) {
                let panel = panel.downgrade();
                let changed_panel = panel.clone();
                let restored_panel = panel.clone();
                cx.subscribe(
                    &host.composer,
                    move |_, composer, _: &crate::components::composer::ConversationChanged, cx| {
                        if let Some(review) = composer.read(cx).latest_review() {
                            let _ = changed_panel
                                .update(cx, |p, cx| p.set_last_turn(review, false, cx));
                        }
                    },
                )
                .detach();
                cx.subscribe(
                    &host.composer,
                    move |_, _, _: &crate::components::composer::ReviewCommentsSubmitted, cx| {
                        let _ = panel.update(cx, |p, cx| p.clear_comments(cx));
                    },
                )
                .detach();
                cx.subscribe(
                    &host.composer,
                    move |_, _, event: &crate::components::composer::ReviewCommentsRestored, cx| {
                        let _ = restored_panel
                            .update(cx, |panel, cx| panel.restore_comments(event.0.clone(), cx));
                    },
                )
                .detach();
                let composer = host.composer.clone();
                cx.subscribe(
                    &composer,
                    |s, composer, _: &crate::components::composer::OpenReviewComments, cx| {
                        let owner = s
                            .conversation_hosts
                            .iter()
                            .find(|(_, host)| host.composer == composer)
                            .map(|(key, _)| key.clone());
                        if let Some(key) = owner {
                            if key != s.active_conversation {
                                s.switch_home_to(key, cx);
                            }
                            s.open_review(cx);
                            if let Some(panel) = s.review_panels.get(&s.active_conversation) {
                                panel.update(cx, |p, cx| p.show_comments(cx));
                            }
                        }
                    },
                )
                .detach();
            }
            self.review_panels.insert(key.clone(), panel);
        }
        if let Some(host) = self.conversation_hosts.get(&key)
            && let Some(review) = host.composer.read(cx).latest_review()
        {
            self.review_panels[&key].update(cx, |p, cx| p.set_last_turn(review, false, cx));
        }
        self.review_panels[&key].update(cx, |p, cx| p.focus(cx));
        self.right_panel.focus_pending = false;
    }
    pub(super) fn open_diff_review(
        &mut self,
        review: DiffReviewPresentation,
        cx: &mut Context<Self>,
    ) {
        // The reference shows a turn's changes in the Changes tab.
        self.open_review(cx);
        if let Some(panel) = self.review_panels.get(&self.active_conversation) {
            panel.update(cx, |p, cx| p.set_last_turn(review, true, cx));
        }
        cx.notify();
    }
    pub(super) fn handle_diff_review_event(
        &mut self,
        event: DiffReviewEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            DiffReviewEvent::Close => {
                self.right_panel.diff_review = None;
                self.right_panel.open = false;
            }
            DiffReviewEvent::ToggleFile(index) => {
                let Some(file) = self
                    .right_panel
                    .diff_review
                    .as_mut()
                    .and_then(|review| review.files.get_mut(index))
                else {
                    return;
                };
                file.visual_state = if file.visual_state.is_expanded() {
                    DiffFileVisualState::Collapsed
                } else {
                    DiffFileVisualState::Expanded
                };
            }
            DiffReviewEvent::HeaderHoverChanged { index, hovered } => {
                let Some(file) = self
                    .right_panel
                    .diff_review
                    .as_mut()
                    .and_then(|review| review.files.get_mut(index))
                else {
                    return;
                };
                if file.visual_state.is_expanded() {
                    file.visual_state = if hovered {
                        DiffFileVisualState::HeaderHovered
                    } else {
                        DiffFileVisualState::Expanded
                    };
                }
            }
            DiffReviewEvent::CopyPath(path) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(path));
            }
            DiffReviewEvent::OpenLocation(path) => {
                // Keep this as a native OS action. `Command::arg` passes the
                // path as data (rather than through a shell), so spaces and
                // other filename characters cannot be interpreted as code.
                let _ = finder_reveal_command(&path).spawn();
            }
        }
        cx.notify();
    }
}
