//! Workspace panels behavior and presentation for the application shell.

use std::time::Duration;

use gpui::{Context, Entity, Window, prelude::*};

use super::ChatApp;
use crate::components::{file_panel::FilePanel, terminal::TerminalPanel};

impl ChatApp {
    /// The active chat's terminals, created on first use without one.
    pub(super) fn terminal_panel(&mut self, cx: &mut Context<Self>) -> Entity<TerminalPanel> {
        let key = self.active_conversation.clone();
        if !self.terminal_panels.contains_key(&key) {
            let cwd = self
                .conversation_hosts
                .get(&key)
                .map(|host| host.cwd.clone())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let panel = cx.new(|cx| TerminalPanel::new(cwd, self.mode, cx));
            cx.observe(&panel, |this, _, cx| this.reconcile_panel_tabs(None, cx))
                .detach();
            self.terminal_panels.insert(key.clone(), panel);
        }
        self.terminal_panels[&key].clone()
    }
    pub fn request_window_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self
            .file_panels
            .values()
            .any(|p| p.read(cx).has_unsaved(cx))
        {
            return true;
        }
        if self.file_close_prompt_open {
            return false;
        }
        self.file_close_prompt_open = true;
        for panel in self.file_panels.values() {
            panel.update(cx, |p, cx| p.save_all(cx));
        }
        let mut window_cx = window.to_async(cx);
        cx.spawn(async move |this, cx| {
            // Finish queued automatic saves before deciding whether closing needs a prompt.
            for _ in 0..40 {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let pending = this
                    .read_with(cx, |s, cx| {
                        s.file_panels.values().any(|p| p.read(cx).has_unsaved(cx))
                    })
                    .unwrap_or(false);
                if !pending {
                    let _ = window_cx.update(|w, _| w.remove_window());
                    return;
                }
            }
            let answer = window_cx.update(|w, cx| {
                w.prompt(
                    gpui::PromptLevel::Warning,
                    crate::i18n::text("文件仍有未保存的编辑"),
                    Some(crate::i18n::text(
                        "自动保存未完成或遇到冲突。返回编辑以保留当前内容。",
                    )),
                    &[
                        crate::i18n::text("返回编辑"),
                        crate::i18n::text("放弃未保存的编辑并关闭"),
                    ],
                    cx,
                )
            });
            if let Ok(answer) = answer
                && answer.await.ok() == Some(1)
            {
                let _ = window_cx.update(|w, _| w.remove_window());
            } else {
                let _ = this.update(cx, |s, _| s.file_close_prompt_open = false);
            }
        })
        .detach();
        false
    }
    /// The active chat's files, created on first use without a document.
    pub(super) fn file_panel(&mut self, cx: &mut Context<Self>) -> Entity<FilePanel> {
        let key = self.active_conversation.clone();
        if !self.file_panels.contains_key(&key) {
            let cwd = self
                .conversation_hosts
                .get(&key)
                .map(|h| h.cwd.clone())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let panel = cx.new(|cx| FilePanel::new(cwd, self.mode, cx));
            cx.observe(&panel, |this, panel, cx| {
                if this.right_panel.open
                    && this.right_panel.mode == Some(super::state::RightPanelMode::Files)
                    && this.file_panels.get(&this.active_conversation) == Some(&panel)
                {
                    let item_id = panel.read(cx).active_plan_id();
                    this.home
                        .update(cx, |home, cx| home.set_plan_panel_for_view(item_id, cx));
                }
                this.reconcile_panel_tabs(None, cx);
            })
            .detach();
            self.watch_goal_saves(&panel, cx);
            self.file_panels.insert(key.clone(), panel);
        }
        self.file_panels[&key].clone()
    }

    /// Opens a document in the chat's files: `open` adds it to the file
    /// panel, and its tab joins the strip, selected.
    pub(super) fn open_in_files(
        &mut self,
        open: impl FnOnce(&mut FilePanel, &mut Context<FilePanel>),
        cx: &mut Context<Self>,
    ) {
        self.right_panel.open = true;
        let panel = self.file_panel(cx);
        panel.update(cx, open);
        self.reconcile_panel_tabs(None, cx);
        if let Some(id) = panel.read(cx).active_document() {
            self.show_panel_tab(super::panel_tabs::PanelTab::File(id), cx);
        }
    }
    /// ⌘P: the `Open file` tab with the tree's filter focused.
    pub(super) fn open_files(&mut self, cx: &mut Context<Self>) {
        use super::panel_tabs::{PanelTab, TabPlacement};
        self.right_panel.open = true;
        self.file_panel(cx);
        let picker = self
            .panel_tabs
            .get(&self.active_conversation)
            .and_then(|state| {
                state
                    .tabs
                    .iter()
                    .position(|tab| *tab == PanelTab::FilePicker)
            });
        match picker {
            Some(index) => self.activate_panel_tab(index, cx),
            None => self.place_panel_tab(PanelTab::FilePicker, TabPlacement::Append, cx),
        }
    }

    /// Opens one file matched inside the chat search dialog in the existing
    /// file panel. Directory matches reveal the picker instead, because the
    /// panel owns the tree and the search only reports the path.
    pub(super) fn open_matched_file(
        &mut self,
        path: String,
        is_directory: bool,
        cx: &mut Context<Self>,
    ) {
        if is_directory {
            self.open_files(cx);
            return;
        }
        self.open_in_files(
            |panel, cx| panel.open_path(std::path::PathBuf::from(path), None, cx),
            cx,
        );
        cx.notify();
    }
}
