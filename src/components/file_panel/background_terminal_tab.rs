//! A background terminal's output tab, as the reference opens it from the
//! summary panel's "Background processes" row: the command item's live
//! output, and it stays open with the final output after the process ends.

use std::path::PathBuf;

use gpui::{Context, Div, ScrollHandle, div, prelude::*, px};

use super::{Document, FilePanel};
use crate::theme::{Theme, UI_MONOSPACE_FONT_FAMILY};

pub(super) struct BackgroundTerminalTab {
    pub(super) thread_id: String,
    pub(super) item_id: String,
    pub(super) output: String,
    pub(super) scroll: ScrollHandle,
}

impl FilePanel {
    fn terminal_document(&self, thread_id: &str, item_id: &str) -> Option<u64> {
        self.documents
            .iter()
            .find(|document| {
                document
                    .terminal
                    .as_ref()
                    .is_some_and(|tab| tab.thread_id == thread_id && tab.item_id == item_id)
            })
            .map(|document| document.id)
    }

    /// Opens (or focuses) the tab of one background terminal. `title` is the
    /// command, or "Background terminal" when it has none.
    pub fn open_background_terminal(
        &mut self,
        thread_id: &str,
        item_id: &str,
        title: String,
        output: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.terminal_document(thread_id, item_id) {
            self.active = Some(id);
            cx.notify();
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.documents.push(Document {
            id,
            path: PathBuf::from(title),
            plan: None,
            goal: None,
            terminal: Some(BackgroundTerminalTab {
                thread_id: thread_id.to_owned(),
                item_id: item_id.to_owned(),
                output,
                scroll: ScrollHandle::new(),
            }),
            editor: None,
            saved: None,
            error: None,
            loading: false,
            saving: false,
            revision: 0,
            image: false,
            preview: false,
            markdown: None,
            markdown_revision: None,
            markdown_pending_revision: None,
        });
        self.active = Some(id);
        cx.notify();
    }

    /// The command's output changed; open tabs follow it.
    pub fn sync_background_terminal(
        &mut self,
        thread_id: &str,
        item_id: &str,
        output: &str,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for document in &mut self.documents {
            if let Some(tab) = document.terminal.as_mut()
                && tab.thread_id == thread_id
                && tab.item_id == item_id
                && tab.output != output
            {
                tab.output = output.to_owned();
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }

    /// Items with an open terminal tab, for the app to push output to.
    pub fn background_terminal_items(&self) -> Vec<(String, String)> {
        self.documents
            .iter()
            .filter_map(|document| document.terminal.as_ref())
            .map(|tab| (tab.thread_id.clone(), tab.item_id.clone()))
            .collect()
    }
}

/// The tab body: the output in the code font, or "No output yet".
pub(super) fn body(tab: &BackgroundTerminalTab, theme: Theme) -> Div {
    let text = tab
        .output
        .trim_end_matches(['\r', '\n'])
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    div()
        .flex_1()
        .min_w(px(0.))
        .min_h(px(0.))
        .bg(theme.surface)
        .child(if text.is_empty() {
            div()
                .p(px(16.))
                .font_family(UI_MONOSPACE_FONT_FAMILY)
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(theme.text_tertiary)
                .child(crate::i18n::format!("暂无输出" => "No output yet"))
        } else {
            div().size_full().child(
                div()
                    .id("background-terminal-output")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&tab.scroll)
                    .p(px(16.))
                    .font_family(UI_MONOSPACE_FONT_FAMILY)
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(theme.text)
                    .whitespace_normal()
                    .children(
                        text.lines()
                            .map(|line| div().child(line.to_owned()))
                            .collect::<Vec<_>>(),
                    ),
            )
        })
}
