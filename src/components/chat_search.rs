//! Chat search dialog: the native reconstruction of the ChatGPT desktop app's
//! command menu in its chat-search form, opened from the sidebar search button.
//!
//! Geometry, type, colours, and interaction states come from live CDP captures
//! of the reference build (see `artifacts/chat-search/`): a 520px panel centred
//! horizontally with `top = max(16, (windowHeight - 504) / 2)`, a 4px inset, a
//! 33px borderless input, and 31px rows that grow to 49px when the backend
//! returns a match snippet. Data comes from the workspace store, which reads
//! app-server `threadSection/list` and `thread/list` (empty query) or
//! `thread/search` (non-empty query).

use std::{sync::Arc, time::Duration};

use gpui::{
    Animation, AnimationExt, BoxShadow, Context, Entity, FocusHandle, Focusable, IntoElement,
    KeyDownEvent, MouseButton, ScrollHandle, SharedString, StyledText, TextRun, Window, div, px,
    radians,
};
use gpui::{prelude::*, rgba};

use crate::{
    agent::ThreadId,
    components::prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
    theme::{Theme, ThemeMode},
    workspace::{ChatSearchEntry, WorkspaceSnapshot, WorkspaceStore},
};

// Reference measurements (`getBoundingClientRect` / computed styles).
const PANEL_WIDTH: f32 = 520.0;
const PANEL_RADIUS: f32 = 20.0;
const PANEL_PADDING: f32 = 4.0;
const PANEL_GAP: f32 = 4.0;
const LIST_MAX_HEIGHT: f32 = 440.0;
const PANEL_MAX_HEIGHT: f32 = LIST_MAX_HEIGHT + 64.0;
const MIN_PANEL_TOP: f32 = 16.0;
const HEADER_HEIGHT: f32 = 26.5714;
const ROW_HEIGHT: f32 = 31.0;
const ROW_HEIGHT_WITH_SNIPPET: f32 = 49.0;
const EMPTY_ROW_HEIGHT: f32 = 58.0;
const ROW_RADIUS: f32 = 12.5;
const ROW_PADDING_X: f32 = 8.0;
const ROW_PADDING_Y: f32 = 5.0;
const ROW_GAP: f32 = 8.0;
/// Chat rows reserve the leading icon slot the reference renders empty.
const CHAT_ICON_GUTTER: f32 = 20.0;
const QUICK_ACTION_ICON: f32 = 16.0;
const PROJECT_LABEL_WIDTH: f32 = 96.0;
const HEADER_FONT_SIZE: f32 = 13.0;
const HEADER_LINE_HEIGHT: f32 = 18.5714;
/// Row type from `[cmdk-item]`: 14px over a 21px line box.
const ROW_FONT_SIZE: f32 = 14.0;
const ROW_LINE_HEIGHT: f32 = 21.0;
const SNIPPET_FONT_SIZE: f32 = 12.0;
const SNIPPET_LINE_HEIGHT: f32 = 16.0;
const SNIPPET_TOP_PADDING: f32 = 2.0;
const SPINNER_SIZE: f32 = 12.0;
const HINT_FONT_SIZE: f32 = 12.0;
const HINT_LINE_HEIGHT: f32 = 12.0;
const HINT_RADIUS: f32 = 10.0;
/// The reference caps the chat list at nine rows so ⌘1…⌘9 stay stable.
const MAX_CHAT_ROWS: usize = 9;

/// A quick action the dialog can run for real. Every variant maps to an
/// existing application flow; none of them are placeholders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuickAction {
    NewChat,
    OpenFolder,
    SearchFiles,
}

impl QuickAction {
    fn label(self) -> &'static str {
        match self {
            Self::NewChat => "New chat",
            Self::OpenFolder => "Open folder",
            Self::SearchFiles => "Search files",
        }
    }

    fn shortcut(self) -> &'static str {
        match self {
            Self::NewChat => "⌘N",
            Self::OpenFolder => "⌘O",
            Self::SearchFiles => "⌘P",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::NewChat => "command-new-chat",
            Self::OpenFolder => "command-open-folder",
            Self::SearchFiles => "command-search-files",
        }
    }

    fn all() -> [Self; 3] {
        [Self::NewChat, Self::OpenFolder, Self::SearchFiles]
    }
}

/// Selects a thread from the dialog. The host owns the actual navigation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectChat(pub ThreadId);

/// Starts a new conversation from the dialog's quick actions. The host resolves
/// the project and working directory from its own current selection.
pub struct StartNewChat;

/// Runs the "Open folder" quick action.
pub struct OpenFolder;

/// Runs the "Search files" quick action.
pub struct SearchFiles;

impl gpui::EventEmitter<SelectChat> for ChatSearchView {}
impl gpui::EventEmitter<StartNewChat> for ChatSearchView {}
impl gpui::EventEmitter<OpenFolder> for ChatSearchView {}
impl gpui::EventEmitter<SearchFiles> for ChatSearchView {}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    Chat {
        thread_id: ThreadId,
        title: String,
        project: Option<String>,
        snippet: Option<String>,
    },
    Quick(QuickAction),
}

pub struct ChatSearchView {
    mode: ThemeMode,
    store: Arc<WorkspaceStore>,
    snapshot: WorkspaceSnapshot,
    input: Entity<PromptInput>,
    focus_handle: FocusHandle,
    scroll: ScrollHandle,
    open: bool,
    focus_pending: bool,
    selected: usize,
    hovered: Option<usize>,
    /// Hover drives selection in the real dialog. Scripted captures pin an
    /// explicit state, so they disable the pointer path unless the state under
    /// test is the hover state itself.
    #[cfg(feature = "screenshot")]
    hover_enabled: bool,
}

impl ChatSearchView {
    pub fn new(mode: ThemeMode, store: Arc<WorkspaceStore>, cx: &mut Context<Self>) -> Self {
        let snapshot = store.snapshot();
        let input = cx.new(|cx| PromptInput::chat_search(mode, "Search chats", cx));
        cx.subscribe(&input, |this, input, _: &PromptChanged, cx| {
            let query = input.read(cx).text().to_owned();
            this.selected = 0;
            this.hovered = None;
            this.scroll.set_offset(gpui::point(px(0.0), px(0.0)));
            this.store.search(query);
            cx.notify();
        })
        .detach();
        cx.subscribe(&input, |this, _, _: &PromptSubmitted, cx| {
            this.activate_selected(cx);
        })
        .detach();
        let receiver = store.subscribe();
        cx.spawn(async move |this, cx| {
            while let Ok(snapshot) = receiver.recv().await {
                let _ = this.update(cx, |this, cx| {
                    this.snapshot = snapshot;
                    cx.notify();
                });
            }
        })
        .detach();
        Self {
            mode,
            store,
            snapshot,
            input,
            focus_handle: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            open: false,
            focus_pending: false,
            selected: 0,
            hovered: None,
            #[cfg(feature = "screenshot")]
            hover_enabled: true,
        }
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.input.update(cx, |input, cx| input.set_mode(mode, cx));
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the dialog with a clean query, matching the reference's behaviour
    /// when the sidebar search button is pressed.
    pub fn open(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        self.selected = 0;
        self.hovered = None;
        self.scroll.set_offset(gpui::point(px(0.0), px(0.0)));
        self.input.update(cx, |input, cx| {
            input.set_text_silently("", cx);
        });
        self.store.search(String::new());
        self.focus_pending = true;
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.focus_pending = false;
        self.input.update(cx, |input, cx| input.clear(cx));
        self.store.search(String::new());
        cx.notify();
    }

    /// Rows in render order: the Chats group first, then Quick actions.
    fn rows(&self) -> Vec<Row> {
        let query = self.snapshot.search_query.trim().to_lowercase();
        let mut rows = Vec::new();
        for entry in self.snapshot.chat_search_entries(MAX_CHAT_ROWS) {
            rows.push(Row::Chat {
                thread_id: entry.thread.thread_id.clone(),
                title: chat_title(&entry),
                project: chat_project_label(&entry),
                snippet: entry.snippet.clone(),
            });
        }
        for action in QuickAction::all() {
            if query.is_empty() || fuzzy_match(action.label(), &query).is_some() {
                rows.push(Row::Quick(action));
            }
        }
        rows
    }

    fn activate_selected(&mut self, cx: &mut Context<Self>) {
        let rows = self.rows();
        let Some(row) = rows.get(self.selected).cloned() else {
            return;
        };
        self.activate(row, cx);
    }

    fn activate(&mut self, row: Row, cx: &mut Context<Self>) {
        match row {
            Row::Chat { thread_id, .. } => {
                self.close(cx);
                cx.emit(SelectChat(thread_id));
            }
            Row::Quick(QuickAction::NewChat) => {
                self.close(cx);
                cx.emit(StartNewChat);
            }
            Row::Quick(QuickAction::OpenFolder) => {
                self.close(cx);
                cx.emit(OpenFolder);
            }
            Row::Quick(QuickAction::SearchFiles) => {
                self.close(cx);
                cx.emit(SearchFiles);
            }
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.rows().len();
        if count == 0 {
            return;
        }
        let current = self.selected.min(count - 1) as isize;
        let next = (current + delta).clamp(0, count as isize - 1);
        self.selected = next as usize;
        self.hovered = None;
        self.scroll_to_selected();
        cx.notify();
    }

    fn scroll_to_selected(&self) {
        // Rows are 31px or 49px tall; scroll the selection into view using the
        // measured row rectangles collected during the last render.
        let (top, height) = self.selected_span();
        let offset = f32::from(self.scroll.offset().y);
        let viewport = LIST_MAX_HEIGHT;
        if top < offset {
            self.scroll
                .set_offset(gpui::point(px(0.0), px(top.max(0.0))));
        } else if top + height > offset + viewport {
            self.scroll
                .set_offset(gpui::point(px(0.0), px(top + height - viewport)));
        }
    }

    fn selected_span(&self) -> (f32, f32) {
        let mut top = 0.0;
        let mut index = 0;
        let entries = self.snapshot.chat_search_entries(MAX_CHAT_ROWS);
        let query = self.snapshot.search_query.trim().to_lowercase();
        if !entries.is_empty() || !query.is_empty() {
            top += HEADER_HEIGHT + ROW_GAP;
        }
        for entry in &entries {
            let height = row_height(&chat_title(entry), entry.snippet.as_deref(), &query);
            if index == self.selected {
                return (top, height);
            }
            top += height;
            index += 1;
        }
        let quick_actions: Vec<QuickAction> = QuickAction::all()
            .into_iter()
            .filter(|action| query.is_empty() || fuzzy_match(action.label(), &query).is_some())
            .collect();
        if !quick_actions.is_empty() {
            top += HEADER_HEIGHT + ROW_GAP;
        }
        for action in quick_actions {
            if index == self.selected {
                return (top, ROW_HEIGHT);
            }
            let _ = action;
            top += ROW_HEIGHT;
            index += 1;
        }
        (top, ROW_HEIGHT)
    }

    fn number_shortcut(&mut self, index: usize, cx: &mut Context<Self>) {
        let rows = self.rows();
        if let Some(Row::Chat { .. }) = rows.get(index) {
            self.selected = index;
            self.activate_selected(cx);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform
            && let Some(index) = digit_index(key)
        {
            self.number_shortcut(index, cx);
            cx.stop_propagation();
            return;
        }
        match key {
            "escape" => {
                self.close(cx);
                cx.stop_propagation();
            }
            "up" => {
                self.move_selection(-1, cx);
                cx.stop_propagation();
            }
            "down" => {
                self.move_selection(1, cx);
                cx.stop_propagation();
            }
            "enter" => {
                self.activate_selected(cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }
}

impl Focusable for ChatSearchView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(feature = "screenshot")]
impl ChatSearchView {
    /// Deterministic capture states. Every state is produced through the same
    /// code path the real interaction uses; only the input source differs.
    pub fn apply_capture_state(
        &mut self,
        state: &str,
        query: Option<&str>,
        selected: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.hover_enabled = state == "hover";
        if state != "hover" {
            self.hovered = None;
        }
        if let Some(query) = query {
            self.input
                .update(cx, |input, cx| input.set_text_silently(query, cx));
            let query = self.input.read(cx).text().to_owned();
            self.selected = 0;
            self.store.search(query);
        }
        match state {
            "selected" | "hover" => {
                let count = self.rows().len().max(1);
                self.selected = selected.unwrap_or(0).min(count - 1);
            }
            "scroll" => {
                let offset = selected.unwrap_or(0) as f32;
                self.scroll.set_offset(gpui::point(px(0.0), px(-offset)));
            }
            _ => {}
        }
        cx.notify();
    }

    /// Ready when the workspace has settled for the current query.
    pub fn capture_ready(&self) -> Result<bool, String> {
        if let Some(error) = &self.snapshot.error {
            return Err(format!("侧栏数据加载失败：{error}"));
        }
        let loading = self.snapshot.loading;
        if loading.projects || loading.recent || loading.pinned || loading.search {
            return Ok(false);
        }
        if self.snapshot.search_query.trim().is_empty() {
            return Ok(!self.snapshot.recent_threads.is_empty()
                || !self.snapshot.pinned_threads.is_empty());
        }
        Ok(true)
    }
}

fn digit_index(key: &str) -> Option<usize> {
    match key {
        "1" => Some(0),
        "2" => Some(1),
        "3" => Some(2),
        "4" => Some(3),
        "5" => Some(4),
        "6" => Some(5),
        "7" => Some(6),
        "8" => Some(7),
        "9" => Some(8),
        _ => None,
    }
}

/// Threads without a server title show their first message preview, matching
/// the reference's fallback for unnamed conversations.
fn chat_title(entry: &ChatSearchEntry) -> String {
    let title = entry.thread.title.trim();
    if !title.is_empty() {
        return title.to_owned();
    }
    let preview = entry.thread.preview.trim();
    if !preview.is_empty() {
        return preview.lines().next().unwrap_or_default().trim().to_owned();
    }
    String::new()
}

/// The reference labels each chat with its workspace folder name.
fn chat_project_label(entry: &ChatSearchEntry) -> Option<String> {
    entry
        .thread
        .cwd
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

fn row_height(title: &str, snippet: Option<&str>, query: &str) -> f32 {
    if snippet_shown(title, snippet, query) {
        ROW_HEIGHT_WITH_SNIPPET
    } else {
        ROW_HEIGHT
    }
}

/// The reference renders the backend snippet on a second line unless it only
/// repeats text the title already shows. The app-server always returns a
/// snippet, so the decision is the client's: a snapshot whose match is the same
/// message the title came from stays a single line.
fn snippet_shown(title: &str, snippet: Option<&str>, query: &str) -> bool {
    if query.trim().is_empty() {
        return false;
    }
    let Some(snippet) = snippet else {
        return false;
    };
    let snippet = snippet
        .trim()
        .trim_start_matches('…')
        .trim_start_matches('.')
        .trim();
    if snippet.is_empty() {
        return false;
    }
    let title = title.trim();
    !title.contains(snippet) && !snippet.contains(title)
}

/// Case-insensitive subsequence match; `None` means the query is not present.
/// Returns the matched byte ranges of `text` in ascending order.
fn fuzzy_match(text: &str, query: &str) -> Option<Vec<std::ops::Range<usize>>> {
    if query.is_empty() {
        return Some(Vec::new());
    }
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut query_chars = query.chars().flat_map(char::to_lowercase);
    let mut wanted = query_chars.next()?;
    for (offset, character) in text.char_indices() {
        let lowered = character.to_lowercase().next().unwrap_or(character);
        if lowered != wanted {
            continue;
        }
        let end = offset + character.len_utf8();
        match ranges.last_mut() {
            Some(range) if range.end == offset => range.end = end,
            _ => ranges.push(offset..end),
        }
        match query_chars.next() {
            Some(next) => wanted = next,
            None => return Some(ranges),
        }
    }
    None
}

impl Render for ChatSearchView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        if std::mem::take(&mut self.focus_pending) {
            self.input.focus_handle(cx).focus(window, cx);
        }
        let theme = Theme::for_mode(self.mode);
        let rows = self.rows();
        let query = self.snapshot.search_query.trim().to_lowercase();
        let loading = self.snapshot.loading.search;
        let viewport = window.viewport_size();
        let top = ((f32::from(viewport.height) - PANEL_MAX_HEIGHT) / 2.0).max(MIN_PANEL_TOP);
        let left = ((f32::from(viewport.width) - PANEL_WIDTH) / 2.0).max(0.0);

        let chat_rows: Vec<(usize, &Row)> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, Row::Chat { .. }))
            .collect();
        let quick_rows: Vec<(usize, &Row)> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, Row::Quick(_)))
            .collect();
        let chats_empty = chat_rows.is_empty() && !query.is_empty() && !loading;
        let show_chats_group = !query.is_empty() || !chat_rows.is_empty();

        let mut list = div()
            .id("chat-search-list")
            .max_h(px(LIST_MAX_HEIGHT))
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        // The reference command menu has more groups behind the first nine
        // chats, so an empty query keeps the list viewport at its 440px cap
        // even though this native surface only exposes the history and quick
        // action groups. Query results, by contrast, size the list to their
        // actual content just like the reference.
        if query.is_empty() && !loading {
            list = list.h(px(LIST_MAX_HEIGHT));
        }

        if show_chats_group {
            let mut group = div().flex().flex_col().gap(px(PANEL_GAP)).child(
                div()
                    .h(px(HEADER_HEIGHT))
                    .px(px(8.0))
                    .pt(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(ROW_GAP))
                    .text_size(px(HEADER_FONT_SIZE))
                    .line_height(px(HEADER_LINE_HEIGHT))
                    .text_color(theme.chat_search_description)
                    .child("Chats")
                    .when(loading, |header| {
                        header.child(
                            div().w(px(SPINNER_SIZE)).h(px(SPINNER_SIZE)).child(
                                gpui::svg()
                                    .path("icons/search-spinner.svg")
                                    .size(px(SPINNER_SIZE))
                                    .text_color(theme.chat_search_description)
                                    .with_animation(
                                        "chat-search-spinner",
                                        Animation::new(Duration::from_millis(1000)).repeat(),
                                        |spinner, progress| {
                                            spinner.with_transformation(
                                                gpui::Transformation::rotate(radians(
                                                    progress * 2.0 * std::f32::consts::PI,
                                                )),
                                            )
                                        },
                                    ),
                            ),
                        )
                    }),
            );
            let mut body = div().flex().flex_col();
            for (index, row) in &chat_rows {
                body = body.child(self.chat_row(*index, row, &query, theme, cx));
            }
            if chats_empty {
                body = body.child(
                    div()
                        .id("chat-search-empty")
                        .h(px(EMPTY_ROW_HEIGHT))
                        .px(px(ROW_PADDING_X))
                        .py(px(ROW_PADDING_Y))
                        .rounded(px(ROW_RADIUS))
                        .opacity(0.25)
                        .flex()
                        .items_center()
                        .text_color(theme.chat_search_text)
                        .child(
                            div()
                                .h(px(48.0))
                                .px(px(16.0))
                                .flex()
                                .items_center()
                                .text_size(px(HEADER_FONT_SIZE))
                                .line_height(px(HEADER_LINE_HEIGHT))
                                .text_color(theme.chat_search_description)
                                .child("No matches"),
                        ),
                );
            }
            group = group.child(body);
            list = list.child(group);
        }

        if !quick_rows.is_empty() {
            let mut group = div().flex().flex_col().gap(px(PANEL_GAP)).child(
                div()
                    .h(px(HEADER_HEIGHT))
                    .px(px(8.0))
                    .pt(px(8.0))
                    .flex()
                    .items_center()
                    .text_size(px(HEADER_FONT_SIZE))
                    .line_height(px(HEADER_LINE_HEIGHT))
                    .text_color(theme.chat_search_description)
                    .child("Quick actions"),
            );
            let mut body = div().flex().flex_col();
            for (index, row) in &quick_rows {
                body = body.child(self.quick_row(*index, row, theme, cx));
            }
            group = group.child(body);
            list = list.child(group);
        }

        div()
            .id("chat-search-overlay")
            .absolute()
            .inset_0()
            .bg(theme.chat_search_overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.close(cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .id("chat-search-panel")
                    .role(gpui::Role::Dialog)
                    .aria_label("Command menu")
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(PANEL_WIDTH))
                    .max_h(px(PANEL_MAX_HEIGHT))
                    .rounded(px(PANEL_RADIUS))
                    .border_1()
                    .border_color(theme.chat_search_border)
                    .bg(theme.chat_search_surface)
                    .shadow(vec![
                        BoxShadow::new(px(0.0), px(16.0), rgba(0x00000030).into())
                            .blur_radius(px(32.0))
                            .spread_radius(px(-8.0)),
                    ])
                    .overflow_hidden()
                    .p(px(PANEL_PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(PANEL_GAP))
                    .track_focus(&self.focus_handle)
                    .on_key_down(cx.listener(Self::on_key_down))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .text_color(theme.chat_search_text)
                    .child(self.input.clone())
                    .child(list),
            )
            .into_any_element()
    }
}

impl ChatSearchView {
    fn chat_row(
        &self,
        index: usize,
        row: &Row,
        query: &str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let Row::Chat {
            thread_id,
            title,
            project,
            snippet,
        } = row
        else {
            unreachable!("chat_row called with a non-chat row")
        };
        let with_snippet = snippet_shown(title, snippet.as_deref(), query);
        let height = if with_snippet {
            ROW_HEIGHT_WITH_SNIPPET
        } else {
            ROW_HEIGHT
        };
        let selected = self.selected == index;
        let mut element = div()
            .id(SharedString::from(format!("chat-search-row-{index}")))
            .h(px(height))
            .px(px(ROW_PADDING_X))
            .py(px(ROW_PADDING_Y))
            .rounded(px(ROW_RADIUS))
            .flex()
            .gap(px(ROW_GAP))
            .text_size(px(ROW_FONT_SIZE))
            .line_height(px(ROW_LINE_HEIGHT))
            .cursor_pointer()
            .when(selected, |row| {
                row.bg(theme.chat_search_row_hover).opacity(1.0)
            })
            .when(!selected, |row| row.opacity(0.75))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                #[cfg(feature = "screenshot")]
                if !this.hover_enabled {
                    return;
                }
                if *hovered {
                    this.hovered = Some(index);
                    this.selected = index;
                } else if this.hovered == Some(index) {
                    this.hovered = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener({
                let thread_id = thread_id.clone();
                move |this, _, _, cx| {
                    this.close(cx);
                    cx.emit(SelectChat(thread_id.clone()));
                }
            }));
        element = element
            .when(with_snippet, |row| row.items_start())
            .when(!with_snippet, |row| row.items_center())
            .child(
                div()
                    .w(px(CHAT_ICON_GUTTER))
                    .h(px(16.0))
                    .flex_none()
                    .flex()
                    .items_center(),
            );
        let mut body = div().min_w(px(0.0)).flex_1().flex().flex_col().child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(ROW_GAP))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        // GPUI's text rasterizer centers the 21px line box
                        // half a logical pixel lower than Chromium at this
                        // DPR. Keep layout geometry unchanged and nudge only
                        // the title glyph run to the measured baseline.
                        .relative()
                        .top(px(-0.5))
                        .truncate()
                        .child(title_element(title, query, theme)),
                )
                .when_some(project.clone(), |row, project| {
                    row.child(
                        div()
                            .w(px(PROJECT_LABEL_WIDTH))
                            .flex_none()
                            .truncate()
                            .text_size(px(HEADER_FONT_SIZE))
                            .line_height(px(HEADER_LINE_HEIGHT))
                            .text_right()
                            .text_color(theme.chat_search_description)
                            .child(project),
                    )
                })
                .child(shortcut_hint(format!("⌘{}", index + 1), theme)),
        );
        if with_snippet && let Some(snippet) = snippet.clone() {
            body = body.child(
                div()
                    .w_full()
                    .pt(px(SNIPPET_TOP_PADDING))
                    .truncate()
                    .text_size(px(SNIPPET_FONT_SIZE))
                    .line_height(px(SNIPPET_LINE_HEIGHT))
                    .text_color(theme.chat_search_description)
                    .child(snippet),
            );
        }
        element.child(body)
    }

    fn quick_row(
        &self,
        index: usize,
        row: &Row,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let Row::Quick(action) = row else {
            unreachable!("quick_row called with a non-quick row")
        };
        let selected = self.selected == index;
        let action = *action;
        div()
            .id(SharedString::from(format!("chat-search-action-{index}")))
            .h(px(ROW_HEIGHT))
            .px(px(ROW_PADDING_X))
            .py(px(ROW_PADDING_Y))
            .rounded(px(ROW_RADIUS))
            .flex()
            .items_center()
            .gap(px(ROW_GAP))
            .text_size(px(ROW_FONT_SIZE))
            .line_height(px(ROW_LINE_HEIGHT))
            .cursor_pointer()
            .when(selected, |row| {
                row.bg(theme.chat_search_row_hover).opacity(1.0)
            })
            .when(!selected, |row| row.opacity(0.75))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                #[cfg(feature = "screenshot")]
                if !this.hover_enabled {
                    return;
                }
                if *hovered {
                    this.hovered = Some(index);
                    this.selected = index;
                } else if this.hovered == Some(index) {
                    this.hovered = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.activate(Row::Quick(action), cx);
            }))
            .child(
                gpui::svg()
                    .path(format!("icons/{}.svg", action.icon()))
                    .size(px(QUICK_ACTION_ICON))
                    .flex_none()
                    .text_color(theme.chat_search_text),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(ROW_GAP))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .child(action.label().to_owned()),
                    )
                    .child(shortcut_hint(action.shortcut().to_owned(), theme)),
            )
    }
}

/// The reference dims every title character that the query did not match, and
/// renders the title undimmed when the query never matched it at all.
fn title_element(title: &str, query: &str, theme: Theme) -> gpui::AnyElement {
    let Some(ranges) = fuzzy_match(title, query) else {
        return div().child(title.to_owned()).into_any_element();
    };
    if ranges.is_empty() {
        return div().child(title.to_owned()).into_any_element();
    }
    let mut runs = Vec::new();
    let mut cursor = 0;
    for range in ranges {
        if cursor < range.start {
            runs.push(TextRun {
                len: range.start - cursor,
                color: theme.chat_search_description.into(),
                ..Default::default()
            });
        }
        runs.push(TextRun {
            len: range.end - range.start,
            color: theme.chat_search_text.into(),
            ..Default::default()
        });
        cursor = range.end;
    }
    if cursor < title.len() {
        runs.push(TextRun {
            len: title.len() - cursor,
            color: theme.chat_search_description.into(),
            ..Default::default()
        });
    }
    StyledText::new(title.to_owned())
        .with_runs(runs)
        .into_any_element()
}

/// Shortcut capsule: 2px/6px padding, 16px tall, 10px radius, `currentColor`
/// at 10% alpha, at 80% container opacity.
fn shortcut_hint(label: String, theme: Theme) -> impl IntoElement {
    div().flex_none().opacity(0.8).flex().items_center().child(
        div()
            .h(px(16.0))
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(HINT_RADIUS))
            .bg(theme.chat_search_hint_surface)
            .flex()
            .items_center()
            .text_size(px(HINT_FONT_SIZE))
            .line_height(px(HINT_LINE_HEIGHT))
            .text_color(theme.chat_search_text)
            .child(label),
    )
}

#[cfg(test)]
mod tests {
    use super::{ROW_HEIGHT, ROW_HEIGHT_WITH_SNIPPET, fuzzy_match, row_height, snippet_shown};

    #[test]
    fn fuzzy_match_is_case_insensitive_and_ordered() {
        let ranges = fuzzy_match("GPUI优势与内存占用", "gpui").expect("subsequence matches");
        assert_eq!(ranges, vec![0..4]);
        let ranges = fuzzy_match("补充 TypeScript Map 用法", "map").expect("case insensitive");
        assert_eq!(&"补充 TypeScript Map 用法"[ranges[0].clone()], "Map");
        assert!(fuzzy_match("绘制鹈鹕骑单车SVG", "svg").is_some());
    }

    #[test]
    fn fuzzy_match_reports_missing_queries() {
        assert!(fuzzy_match("实现历史会话搜索弹窗", "zzz").is_none());
        // A subsequence that skips characters is still a match, matching the
        // reference's highlight semantics.
        assert!(fuzzy_match("Draw pelican bicycle SVG", "dsvg").is_some());
    }

    #[test]
    fn snippet_hidden_when_it_repeats_the_title() {
        let title = "Tailscale是通过什么原理连接的？";
        assert!(!snippet_shown(title, Some(title), "Tailscale"));
        assert!(!snippet_shown(
            title,
            Some("… Tailscale是通过什么原理连接的？ "),
            "Tailscale"
        ));
        assert!(snippet_shown(
            title,
            Some("请你绘制一个鹈鹕骑单车的svg图片"),
            "svg"
        ));
        assert!(!snippet_shown(title, None, "svg"));
        assert!(!snippet_shown(title, Some(title), ""));
    }

    #[test]
    fn row_height_follows_snippet_visibility() {
        assert_eq!(row_height("实现历史会话搜索弹窗", None, ""), ROW_HEIGHT);
        assert_eq!(
            row_height(
                "实现历史会话搜索弹窗",
                Some("项目背景 Echora 是基于"),
                "弹窗"
            ),
            ROW_HEIGHT_WITH_SNIPPET
        );
    }
}
