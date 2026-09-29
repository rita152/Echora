//! Find in chat (⌘F while the conversation or its composer has focus): the
//! floating bar, `thread/searchOccurrences` reads, and jumping to and
//! highlighting the active match. Panels outside the conversation (file
//! editor, terminal, review, pull requests) keep their own ⌘F because the
//! binding lives in this view's key context.

use std::time::Duration;

use gpui::{
    AnyElement, Context, Entity, Focusable, IntoElement, KeyDownEvent, Role, SharedString, Window,
    div, prelude::*, px, rgba,
};

use super::{HomeView, timeline::ConversationListRow};
use crate::{
    agent::{AgentThreadOccurrenceRequest, AgentThreadSearchError},
    components::{
        icons::icon,
        markdown::find::FindScope,
        prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
    },
    conversation::{ConversationActivity, FIND_PAGE_SIZE, FindState, FindStep},
    theme::{Theme, ThemeMode},
};

gpui::actions!(home_find, [FindInChat, FindNext, FindPrevious]);

/// The conversation's key context: its descendants (the composer included)
/// resolve ⌘F to find in chat.
pub(super) const FIND_CONTEXT: &str = "ChatConversation";

pub(crate) fn init_keyboard(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-f", FindInChat, Some(FIND_CONTEXT)),
        gpui::KeyBinding::new("cmd-g", FindNext, Some(FIND_CONTEXT)),
        gpui::KeyBinding::new("cmd-shift-g", FindPrevious, Some(FIND_CONTEXT)),
    ]);
}

/// Debounce before a changed query is sent, so typing does not flood the
/// server with one request per key.
const QUERY_DEBOUNCE: Duration = Duration::from_millis(150);

/// Where the active match is shown: its row and position inside the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FindTarget {
    pub(super) row: usize,
    pub(super) ordinal: usize,
}

#[derive(Default)]
pub(super) struct FindView {
    pub(super) state: FindState,
    pub(super) input: Option<Entity<PromptInput>>,
    pub(super) target: Option<FindTarget>,
    /// The active match's turn was missing; history was re-read once for it.
    reload_requested: bool,
    /// The active match could not be shown in the loaded conversation.
    pub(super) unreachable: bool,
}

fn highlight_colors(mode: ThemeMode) -> (gpui::Rgba, gpui::Rgba) {
    // Measured on the reference's find highlights.
    match mode {
        ThemeMode::Dark => (rgba(0xf8d45dff), rgba(0xea7339ff)),
        ThemeMode::Light => (rgba(0xf6c543ff), rgba(0xd25e28ff)),
    }
}

impl HomeView {
    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let thread_id = self.composer.read(cx).thread_id().map(ToOwned::to_owned);
        let reopened = self.find.state.open;
        self.find.state.open(thread_id);
        let input = self.find.input.get_or_insert_with(|| {
            let input = cx.new(|cx| {
                let mut input = PromptInput::chat_search(
                    self.mode,
                    crate::i18n::format!("搜索聊天…" => "Search chat…"),
                    cx,
                );
                input.set_accessible_name(crate::i18n::format!("在聊天中查找" => "Find in chat"));
                input
            });
            cx.subscribe(&input, |home, _, _: &PromptChanged, cx| {
                home.find_query_changed(cx)
            })
            .detach();
            cx.subscribe(&input, |home, _, _: &PromptSubmitted, cx| {
                home.step_find(true, cx)
            })
            .detach();
            input
        });
        let input = input.clone();
        if self.find.state.query.is_empty() {
            input.update(cx, |input, cx| input.clear(cx));
        }
        window.focus(&input.read(cx).focus_handle(cx), cx);
        if !reopened {
            cx.notify();
        }
    }

    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find.state.close();
        self.find.target = None;
        self.find.unreachable = false;
        let prompt = self.composer.read(cx).prompt_focus_handle(cx);
        window.focus(&prompt, cx);
        self.conversation_cache_dirty = true;
        cx.notify();
    }

    fn find_query_changed(&mut self, cx: &mut Context<Self>) {
        let Some(input) = &self.find.input else {
            return;
        };
        let query = input.read(cx).text().to_owned();
        if query == self.find.state.query {
            return;
        }
        self.find.target = None;
        self.find.unreachable = false;
        self.find.reload_requested = false;
        let cycle = self.find.state.set_query(query);
        cx.notify();
        let Some(cycle) = cycle else {
            return;
        };
        let timer = cx.background_executor().timer(QUERY_DEBOUNCE);
        cx.spawn(async move |home, cx| {
            timer.await;
            let _ = home.update(cx, |home, cx| {
                if home.find.state.cycle == cycle {
                    home.read_find_page(cycle, None, cx);
                }
            });
        })
        .detach();
    }

    /// One `thread/searchOccurrences` page. A thread the server cannot search
    /// falls back to the loaded transcript's own text.
    fn read_find_page(&mut self, cycle: u64, cursor: Option<String>, cx: &mut Context<Self>) {
        let Some(thread_id) = self.find.state.thread_id.clone() else {
            return;
        };
        let receiver = self
            .composer
            .read(cx)
            .agent_backend()
            .search_thread_occurrences(AgentThreadOccurrenceRequest {
                thread_id: thread_id.clone(),
                search_term: self.find.state.search_term(),
                cursor,
                limit: FIND_PAGE_SIZE,
            });
        cx.spawn(async move |home, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(AgentThreadSearchError::Failed(
                    crate::i18n::text("查找连接已关闭").into(),
                ))
            });
            let _ = home.update(cx, |home, cx| {
                let step = match result {
                    Ok(page) => home.find.state.accept_page(cycle, &thread_id, page),
                    Err(AgentThreadSearchError::Unsupported(_)) => {
                        let texts = home.local_find_texts(cx);
                        home.find.state.accept_local(cycle, &texts)
                    }
                    Err(AgentThreadSearchError::Failed(error)) => {
                        home.find.state.fail(cycle, error);
                        FindStep::Nothing
                    }
                };
                home.apply_find_step(step, cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn step_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        let step = if forward {
            self.find.state.next()
        } else {
            self.find.state.previous()
        };
        self.apply_find_step(step, cx);
    }

    fn apply_find_step(&mut self, step: FindStep, cx: &mut Context<Self>) {
        match step {
            FindStep::Reveal => {
                self.find.reload_requested = false;
                self.reveal_find_match(cx);
            }
            FindStep::LoadMore(cursor) => {
                let cycle = self.find.state.cycle;
                self.read_find_page(cycle, Some(cursor), cx);
            }
            FindStep::Nothing => {}
        }
        cx.notify();
    }

    /// Visible user messages and final answers of the loaded transcript, as
    /// `(turn id, item id, text)`, for a server that cannot search.
    fn local_find_texts(&self, cx: &gpui::App) -> Vec<(String, String, String)> {
        let composer = self.composer.read(cx);
        let mut texts = Vec::new();
        for (index, turn) in composer.transcript_render_snapshot().iter().enumerate() {
            let turn_id = turn
                .turn_id
                .clone()
                .unwrap_or_else(|| format!("turn-{index}"));
            texts.push((
                turn_id.clone(),
                format!("user-{index}"),
                turn.user_message.clone(),
            ));
            texts.push((
                turn_id,
                format!("answer-{index}"),
                turn.assistant_message.clone(),
            ));
        }
        let (_, user, _, answer, _, _) = composer.conversation_render_snapshot();
        let current = composer.current_turn_id().unwrap_or("current").to_owned();
        texts.push((
            current.clone(),
            "user-current".into(),
            user.unwrap_or_default(),
        ));
        texts.push((current, "answer-current".into(), answer));
        texts
    }

    /// The row holding the active match. Final answers are found by item id
    /// (their activity row, or the turn's answer row); anything else of the
    /// turn is its user message.
    fn find_row(&self, cx: &gpui::App) -> Option<usize> {
        let active = self.find.state.active_match()?;
        let occurrence = &active.occurrence;
        let rows = self.conversation_rows.as_ref();
        if let Some(row) = rows.iter().position(|row| {
            matches!(row, ConversationListRow::Activity {
                unit: super::timeline::ActivityStreamUnit::Standalone(
                    ConversationActivity::AssistantMessage { item_id, .. }
                    | ConversationActivity::UserMessage { item_id, .. }
                ),
                ..
            } if *item_id == occurrence.item_id)
        }) {
            return Some(row);
        }
        let composer = self.composer.read(cx);
        let transcript = composer.transcript_render_snapshot();
        let current_turn = composer.current_turn_id();
        let local = self.find.state.local;
        let turn_index = transcript
            .iter()
            .position(|turn| turn.turn_id.as_deref() == Some(occurrence.turn_id.as_str()));
        let is_current = turn_index.is_none() && current_turn == Some(occurrence.turn_id.as_str())
            || (local && occurrence.item_id.ends_with("-current"));
        let answer = if local {
            occurrence.item_id.starts_with("answer-")
        } else {
            match turn_index {
                Some(index) => transcript[index]
                    .resumed
                    .as_ref()
                    .is_some_and(|resumed| resumed.final_message_ids.contains(&occurrence.item_id)),
                None => composer
                    .resumed_turn()
                    .is_some_and(|resumed| resumed.final_message_ids.contains(&occurrence.item_id)),
            }
        };
        let turn_index = if local && !is_current {
            occurrence
                .item_id
                .rsplit('-')
                .next()
                .and_then(|index| index.parse::<usize>().ok())
        } else {
            turn_index
        };
        rows.iter().position(|row| match row {
            ConversationListRow::HistoricalUser {
                turn_index: index, ..
            } if !answer => Some(*index) == turn_index,
            ConversationListRow::CurrentUser { .. } if !answer => is_current,
            ConversationListRow::AssistantMarkdown { id, .. } if answer => match turn_index {
                Some(index) if !is_current => *id == format!("historical-assistant-{index}"),
                _ => is_current && id == "current-assistant",
            },
            _ => false,
        })
    }

    /// Scrolls to the active match. A turn not in the loaded conversation is
    /// re-read once through the ordinary history path, and the jump retried
    /// when the rows are rebuilt; after that the match is reported as not
    /// reachable instead of guessing a position.
    pub(super) fn reveal_find_match(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.find.state.active_match() else {
            self.find.target = None;
            return;
        };
        let ordinal = active.ordinal_in_item;
        match self.find_row(cx) {
            Some(row) => {
                self.find.target = Some(FindTarget { row, ordinal });
                self.find.unreachable = false;
                self.conversation_list.scroll_to_reveal_item(row);
                self.conversation_cache_dirty = true;
            }
            // History is being (or about to be) read again: wait for it.
            None if self.composer.read(cx).history_loading()
                || (self.find.reload_requested
                    && self.composer.read(cx).history_needs_reload()) =>
            {
                self.find.target = None;
            }
            None if !self.find.reload_requested && !self.find.state.local => {
                self.find.reload_requested = true;
                self.find.target = None;
                self.composer
                    .update(cx, |composer, cx| composer.request_history_reload(cx));
            }
            None => {
                self.find.target = None;
                self.find.unreachable = true;
            }
        }
        cx.notify();
    }

    /// The rows were rebuilt: a pending jump is retried (history arrived);
    /// a shown match only follows its row, without scrolling the user away.
    pub(super) fn refresh_find_after_rows_changed(&mut self, cx: &mut Context<Self>) {
        if !self.find.state.open || self.find.state.active_match().is_none() {
            return;
        }
        match self.find.target {
            None if !self.find.unreachable => self.reveal_find_match(cx),
            Some(target) => {
                self.find.target = self.find_row(cx).map(|row| FindTarget { row, ..target });
            }
            None => {}
        }
    }

    /// Highlights for one row while the bar is open.
    pub(super) fn find_scope_for_row(&self, row: usize) -> Option<FindScope> {
        let state = &self.find.state;
        if !state.open || state.query.trim().is_empty() || state.matches.is_empty() {
            return None;
        }
        let (match_color, active_color) = highlight_colors(self.mode);
        Some(FindScope {
            query: state.search_term(),
            active: self
                .find
                .target
                .filter(|target| target.row == row)
                .map(|target| target.ordinal),
            match_color,
            active_color,
        })
    }

    fn handle_find_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_find(window, cx);
                cx.stop_propagation();
            }
            "enter" if event.keystroke.modifiers.shift => {
                self.step_find(false, cx);
                cx.stop_propagation();
            }
            _ => cx.propagate(),
        }
    }

    /// The reference's floating find panel at the conversation's top right:
    /// the field and a close button, then previous/next and the count.
    pub(super) fn render_find_bar(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.find.state.open {
            return None;
        }
        let input = self.find.input.clone()?;
        let state = &self.find.state;
        let has_matches = !state.matches.is_empty();
        // As in the reference, the navigation row appears with the first key.
        let expanded = !state.query.is_empty();
        // Measured on the reference panel.
        let surface = match self.mode {
            ThemeMode::Dark => rgba(0x141414ff),
            ThemeMode::Light => rgba(0xf6f6f6ff),
        };
        let nav = |id: &'static str, glyph: &'static str, label: String, forward: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(SharedString::from(label))
                .size(px(16.0))
                .rounded(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .when(!has_matches, |button| button.opacity(0.4))
                .when(has_matches, |button| {
                    button
                        .cursor_pointer()
                        .hover(move |button| button.bg(theme.sidebar_hover))
                        .on_click(cx.listener(move |home, _, _, cx| home.step_find(forward, cx)))
                })
                .child(
                    icon(glyph, theme.text_secondary.into())
                        .size(px(16.0))
                        .when(forward, |arrow| {
                            arrow.with_transformation(gpui::Transformation::rotate(gpui::radians(
                                std::f32::consts::PI,
                            )))
                        }),
                )
        };
        let status = if self.find.unreachable {
            Some(crate::i18n::format!("无法跳转到此结果" => "Could not show this result"))
        } else if let Some(error) = &state.error {
            Some(error.clone())
        } else {
            state.count_label()
        };
        Some(
            div()
                .id("find-in-chat")
                .role(Role::Search)
                .aria_label(crate::i18n::format!("在聊天中查找" => "Find in chat"))
                .absolute()
                .top(px(8.0))
                // 16 px from the window edge, as the reference; the view
                // itself sits inside the conversation gutter.
                .right(px(16.0 - crate::theme::CHAT_CONTENT_HORIZONTAL_GUTTER))
                .w(px(340.0))
                .rounded(px(20.0))
                .overflow_hidden()
                .bg(surface)
                .border(px(0.5))
                .border_color(theme.border)
                .shadow(vec![
                    gpui::BoxShadow::new(px(0.0), px(4.0), rgba(0x0000001a).into())
                        .blur_radius(px(12.0)),
                ])
                .text_color(theme.text)
                .flex()
                .flex_col()
                .on_key_down(cx.listener(Self::handle_find_key))
                .child(
                    div()
                        .h(px(45.0))
                        .pl(px(16.0))
                        .pr(px(14.0))
                        .flex()
                        .items_center()
                        .when(expanded, |row| row.border_b_1().border_color(theme.border))
                        .child(icon("find-search", theme.text_secondary.into()).size(px(16.0)))
                        .child(div().ml(px(-2.0)).min_w(px(0.0)).flex_1().child(input))
                        .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                        .child(
                            div()
                                .id("find-close")
                                .ml(px(6.0))
                                .role(Role::Button)
                                .aria_label(crate::i18n::format!("关闭查找" => "Close find"))
                                .size(px(24.0))
                                .rounded(px(6.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(move |button| button.bg(theme.sidebar_hover))
                                .on_click(
                                    cx.listener(|home, _, window, cx| home.close_find(window, cx)),
                                )
                                .child(
                                    icon("close-dialog", theme.text_secondary.into())
                                        .size(px(16.0)),
                                ),
                        ),
                )
                .when(expanded, |bar| {
                    bar.child(
                        div()
                            .h(px(35.0))
                            .pl(px(16.0))
                            .pr(px(16.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.0))
                                    .child(nav(
                                        "find-previous",
                                        "find-previous",
                                        crate::i18n::format!("上一个结果" => "Previous result"),
                                        false,
                                    ))
                                    .child(nav(
                                        "find-next",
                                        "find-previous",
                                        crate::i18n::format!("下一个结果" => "Next result"),
                                        true,
                                    )),
                            )
                            .when_some(status, |row, status| {
                                row.child(
                                    div()
                                        .id("find-count")
                                        .role(Role::Status)
                                        .aria_label(SharedString::from(status.clone()))
                                        .text_size(px(14.0))
                                        .line_height(px(24.0))
                                        .text_color(theme.text.alpha(0.5))
                                        .child(status),
                                )
                            }),
                    )
                })
                .into_any_element(),
        )
    }
}

#[cfg(feature = "screenshot")]
impl HomeView {
    /// Capture-only find bar states over the reference's "hello" chat:
    /// `open` (empty field), `results` (1 / 2, the user message active),
    /// `second` (2 / 2, the answer active), `capped` (1 / 2+), `none` (0).
    pub fn set_find_for_capture(
        &mut self,
        state: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::agent::AgentThreadOccurrencePage;
        use crate::components::composer::capture_find::{FIND_PROMPT, FIND_REPLY};
        self.composer.update(cx, |composer, cx| {
            composer.set_find_conversation_for_capture(cx)
        });
        self.refresh_conversation_cache(cx);
        self.open_find(window, cx);
        if state == "open" {
            return;
        }
        let query = if state == "none" { "zzz" } else { "hello" };
        if let Some(input) = &self.find.input {
            input.update(cx, |input, cx| input.set_text_silently(query, cx));
        }
        let cycle = self.find.state.set_query(query.into()).expect("query");
        let texts = vec![
            (
                "capture-batch2-turn".to_owned(),
                "user-current".to_owned(),
                FIND_PROMPT.to_owned(),
            ),
            (
                "capture-batch2-turn".to_owned(),
                "answer-current".to_owned(),
                FIND_REPLY.to_owned(),
            ),
        ];
        let mut occurrences = crate::conversation::find_local_occurrences(query, &texts);
        let next_cursor = (state == "capped").then(|| "capture-next".to_owned());
        if state == "capped" {
            occurrences.truncate(2);
        }
        let thread_id = self.find.state.thread_id.clone().unwrap_or_default();
        // Local matches, marked as such so the rows resolve by their ids.
        let step = self.find.state.accept_page(
            cycle,
            &thread_id,
            AgentThreadOccurrencePage {
                generation: 1,
                occurrences,
                next_cursor,
            },
        );
        self.find.state.local = true;
        self.apply_find_step(step, cx);
        if state == "second" {
            self.step_find(true, cx);
        }
        cx.notify();
    }
}
