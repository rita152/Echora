//! The right panel's Browser in the application shell: one panel per chat,
//! the New tab page's tools, and the native pages' visibility.

use std::rc::Rc;

use gpui::{Context, prelude::*};

use super::{ChatApp, ConversationKey, state::RightPanelMode};
use crate::components::{
    browser::{BrowserPanel, BrowserPanelEvent, BrowserTool},
    composer::{ToastAction, ToastKind},
};

impl ChatApp {
    /// Shows the active chat's browser, creating it (with its saved tabs) on
    /// first use. Opening it on a New tab puts the keyboard in the address
    /// field; switching chats with the browser shown leaves focus alone.
    pub(super) fn ensure_browser(&mut self, cx: &mut Context<Self>) {
        self.show_browser(true, cx);
    }

    pub(super) fn show_browser(&mut self, focus_address: bool, cx: &mut Context<Self>) {
        self.deactivate_review(cx);
        self.terminal_return_focus_pending = false;
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
            self.browser_panels.insert(key.clone(), panel);
        }
        let side_chat_available = self
            .conversation_hosts
            .get(&key)
            .is_some_and(|host| host.composer.read(cx).side_chat_configuration().is_some());
        self.browser_panels[&key].update(cx, |panel, cx| {
            panel.set_side_chat_available(side_chat_available, cx);
            if focus_address {
                panel.focus_new_tab_address(cx);
            }
        });
        self.right_panel.focus_pending = false;
        #[cfg(feature = "screenshot")]
        self.apply_browser_capture(cx);
    }

    /// ⌘T, the review tab strip's "+": a New tab in the browser.
    pub(super) fn open_browser_tab(&mut self, url: Option<String>, cx: &mut Context<Self>) {
        self.right_panel.open = true;
        self.select_right_panel_item(1, cx);
        let key = self.active_conversation.clone();
        if let Some(panel) = self.browser_panels.get(&key) {
            panel.update(cx, |panel, cx| match url {
                Some(url) => panel.open_url(url, cx),
                None => panel.new_tab(None, cx),
            });
        }
        cx.notify();
    }

    fn handle_browser_event(&mut self, event: BrowserPanelEvent, cx: &mut Context<Self>) {
        match event {
            BrowserPanelEvent::OpenTool(tool) => match tool {
                BrowserTool::Review => self.open_review(cx),
                BrowserTool::Terminal => self.select_right_panel_item(2, cx),
                BrowserTool::SideChat => self.select_right_panel_item(0, cx),
                BrowserTool::Files => self.select_right_panel_item(3, cx),
            },
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
            BrowserPanelEvent::Closed => {
                if self.right_panel.open && self.right_panel.mode == Some(RightPanelMode::Browser) {
                    self.close_right_panel(cx);
                }
            }
        }
        cx.notify();
    }

    /// An open panel with nothing selected shows the browser's New tab, as
    /// the reference's panel opens on its launcher tab.
    pub(super) fn normalize_right_panel_mode(&mut self, cx: &mut Context<Self>) {
        if self.right_panel.open
            && self.right_panel.mode.is_none()
            && self.right_panel.subagent.is_none()
            && self.right_panel.diff_review.is_none()
        {
            self.right_panel.mode = Some(RightPanelMode::Browser);
            self.ensure_browser(cx);
        }
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
