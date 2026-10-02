//! The right panel's Browser in the application shell: one panel per chat,
//! the New tab page's tools, and the native pages' visibility.

use std::rc::Rc;

use gpui::{Context, Entity, prelude::*};

use super::{
    ChatApp, ConversationKey,
    panel_tabs::{PanelTab, TabPlacement},
    state::RightPanelMode,
};
use crate::components::{
    browser::{BrowserPanel, BrowserPanelEvent, BrowserTool},
    composer::{ToastAction, ToastKind},
};

impl ChatApp {
    /// The active chat's browser, created (with its saved pages) on first
    /// use. Its tabs reach the right panel's strip through
    /// [`ChatApp::reconcile_panel_tabs`].
    pub(super) fn browser_panel(&mut self, cx: &mut Context<Self>) -> Entity<BrowserPanel> {
        let key = self.active_conversation.clone();
        if !self.browser_panels.contains_key(&key) {
            let chat = match &key {
                ConversationKey::Thread(thread) => Some(thread.to_string()),
                ConversationKey::Draft(_) => None,
            };
            let store = self.browser_store.clone();
            let mode = self.mode;
            let panel = cx.new(|cx| BrowserPanel::new(mode, store, chat, cx));
            cx.subscribe(&panel, |this, _, event: &BrowserPanelEvent, cx| {
                this.handle_browser_event(event.clone(), cx)
            })
            .detach();
            cx.observe(&panel, |this, _, cx| this.reconcile_panel_tabs(None, cx))
                .detach();
            self.browser_panels.insert(key.clone(), panel);
        }
        let side_chat_available = self
            .conversation_hosts
            .get(&key)
            .is_some_and(|host| host.composer.read(cx).side_chat_configuration().is_some());
        let panel = self.browser_panels[&key].clone();
        panel.update(cx, |panel, cx| {
            panel.set_side_chat_available(side_chat_available, cx)
        });
        panel
    }

    /// ⌘T: a New tab; a chat link: a page in the selected New tab, or in a
    /// new tab after the others.
    pub(super) fn open_browser_tab(&mut self, url: Option<String>, cx: &mut Context<Self>) {
        self.right_panel.open = true;
        let panel = self.browser_panel(cx);
        let reuse = match self.active_panel_tab() {
            Some(PanelTab::Browser(id)) if panel.read(cx).is_new_tab(id) => Some(id),
            _ => None,
        };
        match (url, reuse) {
            (Some(url), Some(id)) => {
                panel.update(cx, |panel, cx| {
                    panel.select_tab_id(id, cx);
                    panel.open_url(url, cx);
                });
                self.reconcile_panel_tabs(None, cx);
                self.show_panel_tab(PanelTab::Browser(id), cx);
            }
            (url, _) => {
                let id = panel.update(cx, |panel, cx| panel.new_tab(url, cx));
                self.place_panel_tab(PanelTab::Browser(id), TabPlacement::Append, cx);
            }
        }
        cx.notify();
    }

    fn handle_browser_event(&mut self, event: BrowserPanelEvent, cx: &mut Context<Self>) {
        match event {
            // A tool picked on a New tab takes that tab's place.
            BrowserPanelEvent::OpenTool(tool) => {
                let mode = match tool {
                    BrowserTool::Review => RightPanelMode::Review,
                    BrowserTool::Terminal => RightPanelMode::Terminal,
                    BrowserTool::SideChat => RightPanelMode::SideChat,
                    BrowserTool::Files => RightPanelMode::Files,
                };
                let placement = self
                    .panel_tabs
                    .get(&self.active_conversation)
                    .filter(|state| matches!(state.active_tab(), Some(PanelTab::Browser(_))))
                    .map_or(TabPlacement::Append, |state| {
                        TabPlacement::Replace(state.active)
                    });
                self.open_panel_tool(mode, placement, cx);
            }
            BrowserPanelEvent::Toast {
                danger,
                text,
                undo_site,
            } => {
                if let Some(host) = self.conversation_hosts.get(&self.active_conversation) {
                    let kind = if danger {
                        ToastKind::Danger
                    } else {
                        ToastKind::Success
                    };
                    let action = undo_site.map(|url| {
                        let store = self.browser_store.clone();
                        let undo: ToastAction = Rc::new(move |_, cx| {
                            store.update(cx, |store, cx| store.restore_top_site(&url, cx));
                        });
                        (crate::i18n::format!("撤消" => "Undo"), undo)
                    });
                    host.composer.update(cx, |composer, cx| {
                        composer.show_panel_toast(kind, text, action, cx)
                    });
                }
            }
        }
        cx.notify();
    }

    /// Native pages sit above GPUI's view, so every frame hides the pages
    /// of browsers that are not on screen, and covers the shown one while a
    /// window-level dialog is open.
    pub(super) fn sync_browser_pages(&mut self, cx: &mut Context<Self>) {
        let shown = self.right_panel.open
            && self.right_panel.mode == Some(RightPanelMode::Browser)
            && self.right_panel.subagent.is_none()
            && self.right_panel.diff_review.is_none()
            && !self.showing_settings
            && !self.showing_pull_requests;
        let occluded = self.chat_search.read(cx).is_open()
            || self.image_preview.path.is_some()
            || self.permission_confirmation_open
            || self.project_creation.open
            || self.account.dialog.is_some()
            || self.sidebar.read(cx).thread_rename().is_some()
            || self
                .sidebar
                .read(cx)
                .activity_archive_confirmation()
                .is_some();
        let active = self.active_conversation.clone();
        let panels: Vec<_> = self
            .browser_panels
            .iter()
            .map(|(key, panel)| (*key == active, panel.clone()))
            .collect();
        for (is_active, panel) in panels {
            if !(shown && is_active) {
                panel.read(cx).hide_pages();
            }
            let covered = shown && is_active && occluded;
            if panel.read(cx).is_occluded() != covered {
                panel.update(cx, |panel, cx| panel.set_occluded(covered, cx));
            }
        }
    }
}
