//! Attach temporary chat panels to the parent conversation's UI lifetime.

use super::ChatApp;
use crate::{
    components::{
        composer::{ComposerView, OpenQueuedInSideChat},
        side_chat::{SideChatDestination, SideChatEvent, SideChatPanel},
    },
    media::read_image_dimensions,
};
use gpui::{Context, Entity, prelude::*};

impl ChatApp {
    /// "Open in side chat" on a queued message: a new side chat sends it as
    /// its first message; without one the message goes back to the queue.
    pub(super) fn open_queued_in_side_chat(
        &mut self,
        parent: Entity<ComposerView>,
        event: &OpenQueuedInSideChat,
        cx: &mut Context<Self>,
    ) {
        self.right_panel.open = true;
        // Selecting the side chat item starts a new side chat tab.
        self.select_right_panel_item(0, cx);
        let composer = self
            .side_chat_panels
            .get(&self.active_conversation)
            .and_then(|panel| panel.read(cx).active_composer());
        match composer {
            Some(composer) => composer.update(cx, |composer, cx| {
                composer.send_moved_queued_message(event.draft.clone(), event.prompt.clone(), cx)
            }),
            None => parent.update(cx, |parent, cx| {
                parent.restore_queued_from_side_chat(event.removed.clone(), cx)
            }),
        }
        cx.notify();
    }

    pub(super) fn deactivate_side_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.side_chat_panels.get(&self.active_conversation) {
            panel.update(cx, |panel, cx| panel.set_visible(false, cx));
        }
    }

    pub(super) fn ensure_side_chat(&mut self, new: bool, cx: &mut Context<Self>) {
        let key = self.active_conversation.clone();
        let Some(parent) = self
            .conversation_hosts
            .get(&key)
            .map(|host| host.composer.clone())
        else {
            return;
        };
        if parent.read(cx).side_chat_configuration().is_none() {
            self.right_panel.mode = None;
            cx.notify();
            return;
        }
        self.deactivate_review(cx);
        if !self.side_chat_panels.contains_key(&key) {
            let backend = self.agent_backend.clone();
            let skip = self
                .workspace_store
                .snapshot()
                .preferences
                .skip_side_chat_close_confirmation;
            let panel = cx.new(|cx| SideChatPanel::new(parent, backend, self.mode, skip, cx));
            cx.observe(&panel, |_, _, cx| cx.notify()).detach();
            cx.subscribe(&panel, |this, _, event: &SideChatEvent, cx| {
                match event {
                    SideChatEvent::Empty => {
                        this.right_panel.mode = None;
                        this.right_panel.fullscreen = false;
                        this.right_panel.focus_pending = true;
                    }
                    SideChatEvent::Fullscreen => {
                        this.right_panel.fullscreen = !this.right_panel.fullscreen;
                        if let Some(panel) = this.side_chat_panels.get(&this.active_conversation) {
                            panel.update(cx, |panel, cx| {
                                panel.set_fullscreen(this.right_panel.fullscreen, cx)
                            });
                        }
                    }
                    SideChatEvent::OpenPanel(destination) => {
                        let index = match destination {
                            SideChatDestination::Review => 4,
                            SideChatDestination::Terminal => 2,
                            SideChatDestination::Browser => 1,
                            SideChatDestination::Files => 3,
                        };
                        this.select_right_panel_item(index, cx);
                    }
                    SideChatEvent::OpenDiff(review) => {
                        this.deactivate_side_chat(cx);
                        this.open_diff_review(review.clone(), cx);
                    }
                    SideChatEvent::OpenImage(path) => {
                        this.image_preview.path = Some(path.clone());
                        this.image_preview.dimensions = read_image_dimensions(path).ok().flatten();
                        this.image_preview.zoom = 1.0;
                    }
                    SideChatEvent::OpenHookSettings => {
                        this.open_settings_page("hooks-settings", cx);
                    }
                    SideChatEvent::FullAccess(composer) => {
                        this.open_permission_confirmation(composer.clone(), cx);
                    }
                    SideChatEvent::SkipCloseConfirmation(skip) => {
                        this.workspace_store
                            .set_skip_side_chat_close_confirmation(*skip);
                        for panel in this.side_chat_panels.values() {
                            panel.update(cx, |panel, _| panel.set_skip_confirmation(*skip));
                        }
                    }
                }
                cx.notify();
            })
            .detach();
            self.side_chat_panels.insert(key.clone(), panel);
        }
        self.side_chat_panels[&key].update(cx, |panel, cx| {
            panel.set_fullscreen(self.right_panel.fullscreen, cx);
            if new || panel.is_empty() {
                panel.new_chat(cx);
            }
            panel.set_visible(true, cx);
            panel.focus(cx);
        });
        self.right_panel.focus_pending = false;
        self.terminal_return_focus_pending = false;
        cx.notify();
    }
}
