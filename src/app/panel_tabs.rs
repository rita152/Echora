//! The right panel's tabs (ChatGPT 26.930's `Task tabs`): one ordered list
//! per chat over the items the tools hold (browser tabs, terminals, side
//! chats, open files) plus the Changes tab and an `Open file` tab. Each tool
//! shows the item its tab selects; this strip adds, closes, reorders and
//! switches them.
//!
//! The reference's rules, measured on 26.930:
//! - `+` appends a New tab; a tool picked on a New tab replaces it; a file
//!   opened from the `Open file` tab replaces that tab, any other new item is
//!   appended (a browser tab opened beside another follows it).
//! - Closing the selected tab selects its right neighbor (the left one at the
//!   end); closing the last tab hides the panel.
//! - Tabs share the strip up to 238px each and shrink to 88px, then scroll;
//!   after a close they keep their width until the pointer leaves the strip.
//! - The selected tab is a raised chip with its close button; the others are
//!   tertiary, show the button on hover, and a hairline divides two of them.
//!   Long titles fade out instead of ending in an ellipsis.

use std::sync::Arc;

use gpui::{
    AnyElement, BoxShadow, Context, Div, Image, MouseButton, MouseDownEvent, Render, Role,
    SharedString, Stateful, Window, div, img, linear_color_stop, linear_gradient, prelude::*, px,
    rgba,
};

use super::{ChatApp, ConversationKey, state::RightPanelMode};
use crate::{
    components::{icons::icon, prompt_input::PromptInput},
    theme::{Theme, ThemeMode, composite_over},
};

/// The strip's left padding, before the first tab.
const STRIP_LEFT: f32 = 8.0;
/// A full-screen strip's tabs start this far past the titlebar's leading
/// area: its 6px gap, the 1px border of the header that follows and the
/// strip's own 8px padding, less the card's 8px inset from the window edge.
const TITLEBAR_TABS_GAP: f32 = 7.0;
/// Room the strip keeps on its right for the titlebar's trailing controls.
const TRAILING_CONTROLS: f32 = 78.0;
/// A tab's slot: the 238px tab and its 2px gap, down to an 88px tab.
const TAB_SLOT_MAX: f32 = 240.0;
const TAB_SLOT_MIN: f32 = 90.0;
const TAB_GAP: f32 = 2.0;
/// Centred in the 34px strip, on the titlebar's row.
const TAB_HEIGHT: f32 = 26.0;
/// `+` and its 6px lead.
const NEW_TAB_AREA: f32 = 34.0;
/// The full view toggle, the hairline before it and their 6px gaps.
const FULL_VIEW_AREA: f32 = 41.0;

/// One tab of the right panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum PanelTab {
    Browser(u64),
    Terminal(usize),
    SideChat(u64),
    Changes,
    File(u64),
    /// The Files tool without a document: the workspace tree (`Open file`).
    FilePicker,
}

impl PanelTab {
    pub(super) fn mode(self) -> RightPanelMode {
        match self {
            Self::Browser(_) => RightPanelMode::Browser,
            Self::Terminal(_) => RightPanelMode::Terminal,
            Self::SideChat(_) => RightPanelMode::SideChat,
            Self::Changes => RightPanelMode::Review,
            Self::File(_) | Self::FilePicker => RightPanelMode::Files,
        }
    }

    fn key(self) -> SharedString {
        match self {
            Self::Browser(id) => format!("browser-{id}"),
            Self::Terminal(id) => format!("terminal-{id}"),
            Self::SideChat(id) => format!("side-chat-{id}"),
            Self::Changes => "changes".to_owned(),
            Self::File(id) => format!("file-{id}"),
            Self::FilePicker => "open-file".to_owned(),
        }
        .into()
    }
}

/// Where a new tab goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TabPlacement {
    /// After every other tab.
    Append,
    /// In place of the tab at this index (a New tab that picked a tool).
    Replace(usize),
}

/// A chat's tabs and the selected one.
#[derive(Default)]
pub(super) struct PanelTabs {
    pub(super) tabs: Vec<PanelTab>,
    pub(super) active: usize,
}

impl PanelTabs {
    pub(super) fn active_tab(&self) -> Option<PanelTab> {
        self.tabs.get(self.active).copied()
    }
}

/// What a tab shows before its title.
#[derive(Clone)]
enum TabGlyph {
    Icon(&'static str),
    Svg(SharedString),
    Image(Arc<Image>),
}

/// A tab being dragged to a new place in the strip.
#[derive(Clone)]
pub(super) struct PanelTabDrag {
    index: usize,
    title: SharedString,
    glyph: TabGlyph,
    mode: ThemeMode,
    width: f32,
}

impl Render for PanelTabDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        let (background, border) = active_chip(self.mode, theme);
        div()
            .w(px(self.width - TAB_GAP))
            .h(px(TAB_HEIGHT))
            .pl(px(10.))
            .pr(px(7.))
            .rounded(px(10.))
            .bg(background)
            .border_1()
            .border_color(border)
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .text_color(theme.text)
            .child(glyph(&self.glyph, theme.text))
            .child(div().min_w(px(0.)).truncate().child(self.title.clone()))
    }
}

/// The selected tab's chip: white (dark: 8% white) with the hairline.
fn active_chip(mode: ThemeMode, theme: Theme) -> (gpui::Rgba, gpui::Rgba) {
    match mode {
        ThemeMode::Light => (rgba(0xffffffff), theme.border),
        ThemeMode::Dark => (rgba(0xffffff14), theme.border),
    }
}

fn glyph(glyph: &TabGlyph, color: gpui::Rgba) -> AnyElement {
    match glyph {
        TabGlyph::Icon(name) => icon(name, color.into())
            .size(px(16.))
            .flex_none()
            .into_any_element(),
        TabGlyph::Svg(path) => gpui::svg()
            .path(path.clone())
            .size(px(16.))
            .flex_none()
            .text_color(color)
            .into_any_element(),
        TabGlyph::Image(image) => img(image.clone())
            .size(px(16.))
            .flex_none()
            .into_any_element(),
    }
}

impl ChatApp {
    fn panel_tabs_for(&self, key: &ConversationKey) -> Option<&PanelTabs> {
        self.panel_tabs.get(key)
    }

    pub(super) fn active_panel_tab(&self) -> Option<PanelTab> {
        self.panel_tabs_for(&self.active_conversation)
            .and_then(PanelTabs::active_tab)
    }

    pub(super) fn has_panel_tabs(&self) -> bool {
        self.panel_tabs_for(&self.active_conversation)
            .is_some_and(|state| !state.tabs.is_empty())
    }

    /// The items the chat's tools hold, in each tool's order, and the item
    /// each tool shows.
    fn tool_items(&self, key: &ConversationKey, cx: &gpui::App) -> (Vec<PanelTab>, Vec<PanelTab>) {
        let mut items = Vec::new();
        let mut shown = Vec::new();
        if let Some(panel) = self.browser_panels.get(key) {
            let panel = panel.read(cx);
            items.extend(panel.tab_ids().into_iter().map(PanelTab::Browser));
            shown.extend(panel.active_tab_id().map(PanelTab::Browser));
        }
        if let Some(panel) = self.terminal_panels.get(key) {
            let panel = panel.read(cx);
            items.extend(panel.tab_ids().into_iter().map(PanelTab::Terminal));
            shown.extend(panel.active_id().map(PanelTab::Terminal));
        }
        if let Some(panel) = self.side_chat_panels.get(key) {
            let panel = panel.read(cx);
            items.extend(panel.tab_ids().into_iter().map(PanelTab::SideChat));
            shown.extend(panel.active_id().map(PanelTab::SideChat));
        }
        if let Some(panel) = self.file_panels.get(key) {
            let panel = panel.read(cx);
            items.extend(panel.document_ids().into_iter().map(PanelTab::File));
            shown.push(
                panel
                    .active_document()
                    .map_or(PanelTab::FilePicker, PanelTab::File),
            );
        }
        (items, shown)
    }

    /// Brings the active chat's tabs in line with its tools: drops the tabs
    /// of closed items (selecting the right neighbor of a closed selected
    /// tab), adds the items the tools opened themselves, and follows a tool
    /// that switched to another of its items. `dismissed` is a tab the strip
    /// closed that no tool item backs (Changes, `Open file`).
    pub(super) fn reconcile_panel_tabs(
        &mut self,
        dismissed: Option<PanelTab>,
        cx: &mut Context<Self>,
    ) {
        let key = self.active_conversation.clone();
        let (items, shown) = self.tool_items(&key, cx);
        let review = self.review_panels.contains_key(&key);
        let files = self.file_panels.contains_key(&key);
        let state = self.panel_tabs.entry(key.clone()).or_default();
        let before = state.active_tab();
        let before_index = state.active;
        let mut kept_before_active = 0;
        let mut removed_active = false;
        let mut removed = false;
        let mut index = 0;
        state.tabs.retain(|tab| {
            let keep = Some(*tab) != dismissed
                && match tab {
                    PanelTab::Changes => review,
                    PanelTab::FilePicker => files,
                    other => items.contains(other),
                };
            if keep && index < before_index {
                kept_before_active += 1;
            }
            if !keep {
                removed = true;
                removed_active |= index == before_index;
            }
            index += 1;
            keep
        });
        let mut active = if removed_active {
            state
                .tabs
                .get(kept_before_active.min(state.tabs.len().saturating_sub(1)))
                .copied()
        } else {
            before
        };
        let mut added = false;
        for (position, item) in items.iter().enumerate() {
            if state.tabs.contains(item) {
                continue;
            }
            added = true;
            let picker = state
                .tabs
                .iter()
                .position(|tab| *tab == PanelTab::FilePicker);
            match (item, picker) {
                // A file opened from the `Open file` tab takes its place.
                (PanelTab::File(_), Some(picker)) if active == Some(PanelTab::FilePicker) => {
                    state.tabs[picker] = *item;
                }
                (PanelTab::Browser(_), _) => {
                    // A browser tab opened beside another follows it; one
                    // opened after them all goes at the end.
                    let follows = items[..position]
                        .iter()
                        .rev()
                        .find(|other| matches!(other, PanelTab::Browser(_)))
                        .copied();
                    let last = !items[position + 1..]
                        .iter()
                        .any(|other| matches!(other, PanelTab::Browser(_)));
                    match follows.and_then(|tab| state.tabs.iter().position(|t| *t == tab)) {
                        Some(after) if !last => state.tabs.insert(after + 1, *item),
                        _ => state.tabs.push(*item),
                    }
                }
                _ => state.tabs.push(*item),
            }
            // A tool that opened an item shows it.
            if shown.contains(item) {
                active = Some(*item);
            }
        }
        // A tool that switched to another of its own items (a file picked in
        // the tree, a side chat chosen with ⌃Tab) moves the selection along.
        // A closed tab's neighbor wins over whatever its tool shows next.
        if !removed_active
            && let Some(current) = active
            && let Some(own) = shown
                .iter()
                .find(|tab| tab.mode() == current.mode())
                .copied()
            && own != current
            && state.tabs.contains(&own)
        {
            active = Some(own);
        }
        state.active = active
            .and_then(|tab| state.tabs.iter().position(|t| *t == tab))
            .unwrap_or(0);
        let now = state.active_tab();
        let empty = state.tabs.is_empty();
        let has_changes = state.tabs.contains(&PanelTab::Changes);
        if added || removed {
            self.panel_tab_scroll.scroll_to_item(self.panel_tab_index());
        }
        if let Some(browser) = self.browser_panels.get(&key) {
            browser.update(cx, |panel, cx| panel.set_changes_open(has_changes, cx));
        }
        if empty && removed {
            // The reference hides the panel with its last tab.
            self.close_right_panel(cx);
        } else if let Some(tab) = now
            && self.right_panel.open
            && (now != before || self.right_panel.mode != Some(tab.mode()))
        {
            self.show_panel_tab(tab, cx);
        }
        // Tools notify often (a page's progress, a terminal's output); the
        // strip redraws only when its tabs or their labels change.
        let signature = self.panel_tab_signature(cx);
        if added || removed || now != before || signature != self.panel_tab_signature {
            self.panel_tab_signature = signature;
            cx.notify();
        }
    }

    /// A digest of what the strip draws: the tabs, the selection, and each
    /// tab's title and state.
    fn panel_tab_signature(&self, cx: &gpui::App) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        if let Some(state) = self.panel_tabs_for(&self.active_conversation) {
            state.active.hash(&mut hasher);
            for tab in &state.tabs {
                tab.hash(&mut hasher);
                let (title, glyph, unread, rename) = self.panel_tab_label(*tab, cx);
                title.hash(&mut hasher);
                unread.hash(&mut hasher);
                rename.is_some().hash(&mut hasher);
                match glyph {
                    TabGlyph::Icon(name) => name.hash(&mut hasher),
                    TabGlyph::Svg(path) => path.hash(&mut hasher),
                    TabGlyph::Image(image) => image.id().hash(&mut hasher),
                }
            }
        }
        hasher.finish()
    }

    fn panel_tab_index(&self) -> usize {
        self.panel_tabs_for(&self.active_conversation)
            .map_or(0, |state| state.active)
    }

    /// Puts `tab` in the strip and shows it.
    pub(super) fn place_panel_tab(
        &mut self,
        tab: PanelTab,
        placement: TabPlacement,
        cx: &mut Context<Self>,
    ) {
        let key = self.active_conversation.clone();
        let state = self.panel_tabs.entry(key).or_default();
        let mut placement = placement;
        if let Some(existing) = state.tabs.iter().position(|t| *t == tab) {
            state.tabs.remove(existing);
            if let TabPlacement::Replace(index) = placement
                && existing < index
            {
                placement = TabPlacement::Replace(index - 1);
            }
        }
        let index = match placement {
            TabPlacement::Replace(index) if index < state.tabs.len() => {
                state.tabs[index] = tab;
                index
            }
            _ => {
                state.tabs.push(tab);
                state.tabs.len() - 1
            }
        };
        state.active = index;
        self.panel_tab_frozen_width = None;
        self.right_panel.open = true;
        self.panel_tab_scroll.scroll_to_item(index);
        self.show_panel_tab(tab, cx);
        self.reconcile_panel_tabs(None, cx);
    }

    pub(super) fn activate_panel_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let key = self.active_conversation.clone();
        let Some(state) = self.panel_tabs.get_mut(&key) else {
            return;
        };
        let Some(tab) = state.tabs.get(index).copied() else {
            return;
        };
        state.active = index;
        self.panel_tab_scroll.scroll_to_item(index);
        self.show_panel_tab(tab, cx);
    }

    /// Closes the tab at `index` through its tool: side chats with messages
    /// and files with unsaved edits ask first, so they show while asking.
    pub(super) fn close_panel_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let key = self.active_conversation.clone();
        let Some(tab) = self
            .panel_tabs
            .get(&key)
            .and_then(|state| state.tabs.get(index).copied())
        else {
            return;
        };
        match tab {
            PanelTab::Browser(id) => {
                if let Some(panel) = self.browser_panels.get(&key) {
                    panel.update(cx, |panel, cx| panel.close_tab_id(id, cx));
                }
            }
            PanelTab::Terminal(id) => {
                if let Some(panel) = self.terminal_panels.get(&key) {
                    panel.update(cx, |panel, cx| panel.close(id, cx));
                }
            }
            PanelTab::SideChat(id) => {
                if self.active_panel_tab() != Some(tab) {
                    self.activate_panel_tab(index, cx);
                }
                if let Some(panel) = self.side_chat_panels.get(&key) {
                    panel.update(cx, |panel, cx| panel.request_close(id, cx));
                }
            }
            PanelTab::File(id) => {
                if self.active_panel_tab() != Some(tab) {
                    self.activate_panel_tab(index, cx);
                }
                if let Some(panel) = self.file_panels.get(&key) {
                    panel.update(cx, |panel, cx| panel.request_close(id, cx));
                }
            }
            PanelTab::Changes => self.deactivate_review(cx),
            PanelTab::FilePicker => {}
        }
        let dismissed = matches!(tab, PanelTab::Changes | PanelTab::FilePicker).then_some(tab);
        self.reconcile_panel_tabs(dismissed, cx);
    }

    pub(super) fn move_panel_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let key = self.active_conversation.clone();
        let Some(state) = self.panel_tabs.get_mut(&key) else {
            return;
        };
        if from >= state.tabs.len() || to >= state.tabs.len() || from == to {
            return;
        }
        let active = state.active_tab();
        let tab = state.tabs.remove(from);
        state.tabs.insert(to, tab);
        state.active = active
            .and_then(|tab| state.tabs.iter().position(|t| *t == tab))
            .unwrap_or(0);
        cx.notify();
    }

    /// Shows `tab`'s item in its tool and hides the other tools.
    pub(super) fn show_panel_tab(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        let key = self.active_conversation.clone();
        let mode = tab.mode();
        if mode != RightPanelMode::SideChat {
            self.deactivate_side_chat(cx);
        }
        if mode != RightPanelMode::Review {
            self.deactivate_review(cx);
        }
        self.right_panel.mode = Some(mode);
        self.right_panel.subagent = None;
        self.right_panel.subagent_menu_open = false;
        self.right_panel.diff_review = None;
        self.right_panel.keyboard_focus = false;
        self.terminal_return_focus_pending = false;
        match tab {
            PanelTab::Browser(id) => {
                let panel = self.browser_panel(cx);
                panel.update(cx, |panel, cx| {
                    panel.select_tab_id(id, cx);
                    if panel.is_new_tab(id) {
                        panel.focus_new_tab_address(cx);
                    }
                });
                #[cfg(feature = "screenshot")]
                self.apply_browser_capture(cx);
            }
            PanelTab::Terminal(id) => {
                let panel = self.terminal_panel(cx);
                panel.update(cx, |panel, cx| {
                    panel.select(id, cx);
                    panel.focus(cx);
                });
            }
            PanelTab::SideChat(id) => {
                if let Some(panel) = self.side_chat_panels.get(&key) {
                    panel.update(cx, |panel, cx| {
                        panel.activate(id, cx);
                        panel.set_visible(true, cx);
                        panel.focus(cx);
                    });
                }
            }
            PanelTab::Changes => self.ensure_review(cx),
            PanelTab::File(id) => {
                let panel = self.file_panel(cx);
                panel.update(cx, |panel, cx| {
                    panel.show_document(Some(id), cx);
                    panel.focus(cx);
                });
            }
            PanelTab::FilePicker => {
                let panel = self.file_panel(cx);
                panel.update(cx, |panel, cx| {
                    panel.show_document(None, cx);
                    panel.show_picker(cx);
                });
            }
        }
        if mode != RightPanelMode::Files {
            self.home
                .update(cx, |home, cx| home.set_plan_panel_for_view(None, cx));
        }
        self.right_panel.focus_pending = false;
        cx.notify();
    }

    /// `+`: a New tab after the others.
    pub(super) fn open_new_panel_tab(&mut self, cx: &mut Context<Self>) {
        let panel = self.browser_panel(cx);
        let id = panel.update(cx, |panel, cx| panel.new_tab(None, cx));
        self.place_panel_tab(PanelTab::Browser(id), TabPlacement::Append, cx);
    }

    /// Opens `mode`'s tool in a new tab at `placement`. Changes and the
    /// `Open file` tab exist once per chat: an open one is selected instead
    /// (taking the New tab's place when one picked it).
    pub(super) fn open_panel_tool(
        &mut self,
        mode: RightPanelMode,
        placement: TabPlacement,
        cx: &mut Context<Self>,
    ) {
        let key = self.active_conversation.clone();
        let replaced = match placement {
            TabPlacement::Replace(index) => self
                .panel_tabs
                .get(&key)
                .and_then(|state| state.tabs.get(index).copied()),
            TabPlacement::Append => None,
        };
        let tab = match mode {
            RightPanelMode::Browser => {
                let panel = self.browser_panel(cx);
                Some(PanelTab::Browser(
                    panel.update(cx, |panel, cx| panel.new_tab(None, cx)),
                ))
            }
            RightPanelMode::Terminal => {
                let panel = self.terminal_panel(cx);
                Some(PanelTab::Terminal(
                    panel.update(cx, |panel, cx| panel.add(cx)),
                ))
            }
            RightPanelMode::SideChat => self
                .side_chat_panel(cx)
                .and_then(|panel| panel.update(cx, |panel, cx| panel.new_chat(cx)))
                .map(PanelTab::SideChat),
            RightPanelMode::Review => Some(PanelTab::Changes),
            RightPanelMode::Files => {
                self.file_panel(cx);
                Some(PanelTab::FilePicker)
            }
        };
        let Some(tab) = tab else {
            return;
        };
        self.place_panel_tab(tab, placement, cx);
        // The New tab that picked the tool is gone.
        if let Some(PanelTab::Browser(id)) = replaced
            && Some(PanelTab::Browser(id)) != Some(tab)
            && let Some(panel) = self.browser_panels.get(&key)
        {
            panel.update(cx, |panel, cx| panel.close_tab_id(id, cx));
        }
        self.reconcile_panel_tabs(None, cx);
    }

    /// Selects the chat's first tab of `mode`, or opens one after the others.
    pub(super) fn show_or_open_panel_tool(&mut self, mode: RightPanelMode, cx: &mut Context<Self>) {
        self.right_panel.open = true;
        let existing = self
            .panel_tabs
            .get(&self.active_conversation)
            .and_then(|state| state.tabs.iter().position(|tab| tab.mode() == mode));
        match existing {
            Some(index) => self.activate_panel_tab(index, cx),
            None => self.open_panel_tool(mode, TabPlacement::Append, cx),
        }
    }

    fn panel_tab_label(
        &self,
        tab: PanelTab,
        cx: &gpui::App,
    ) -> (
        SharedString,
        TabGlyph,
        bool,
        Option<gpui::Entity<PromptInput>>,
    ) {
        let key = &self.active_conversation;
        match tab {
            PanelTab::Browser(id) => {
                let panel = self.browser_panels.get(key).map(|panel| panel.read(cx));
                let (title, favicon) = panel
                    .and_then(|panel| panel.tab_label(id))
                    .unwrap_or_else(|| (crate::i18n::format!("新标签页" => "New tab"), None));
                let glyph = favicon.map_or(TabGlyph::Icon("panel-browser"), TabGlyph::Image);
                (
                    title.into(),
                    glyph,
                    false,
                    panel.and_then(|panel| panel.renaming(id)),
                )
            }
            PanelTab::Terminal(id) => (
                self.terminal_panels
                    .get(key)
                    .map(|panel| panel.read(cx).title(id, cx))
                    .unwrap_or_default()
                    .into(),
                TabGlyph::Icon("panel-terminal"),
                false,
                None,
            ),
            PanelTab::SideChat(id) => {
                let (title, busy, unread) = self
                    .side_chat_panels
                    .get(key)
                    .and_then(|panel| panel.read(cx).tab_label(id))
                    .unwrap_or_else(|| (crate::i18n::text("侧边聊天").into(), false, false));
                (
                    title.into(),
                    TabGlyph::Icon(if busy {
                        "dictation-spinner"
                    } else {
                        "side-chat"
                    }),
                    unread,
                    None,
                )
            }
            PanelTab::Changes => (
                crate::i18n::text("变更").into(),
                TabGlyph::Icon("panel-review"),
                false,
                None,
            ),
            PanelTab::File(id) => {
                let (title, path) = self
                    .file_panels
                    .get(key)
                    .and_then(|panel| panel.read(cx).document_tab(id))
                    .unwrap_or_default();
                (title.into(), TabGlyph::Svg(path.into()), false, None)
            }
            PanelTab::FilePicker => (
                crate::i18n::text("打开文件").into(),
                TabGlyph::Icon("markdown-file-document"),
                false,
                None,
            ),
        }
    }

    /// The tab strip on the card's top bar: the tabs, `+`, and the full view
    /// toggle, left of the titlebar's trailing controls. A full-screen card
    /// under the titlebar's leading controls passes how far they reach into
    /// it as `titlebar_inset`; the tabs start after them.
    pub(super) fn panel_tab_strip(
        &self,
        card_width: f32,
        titlebar_inset: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mode = self.mode;
        let (tabs, active) = self
            .panel_tabs_for(&self.active_conversation)
            .map(|state| (state.tabs.clone(), state.active))
            .unwrap_or_default();
        // GPUI measures the strip's viewport only once it is laid out, so
        // the strip's first frame cannot scroll to the selected tab; ask
        // again on the next one.
        if !tabs.is_empty() && self.panel_tab_scroll.bounds_for_item(0).is_none() {
            let app = cx.weak_entity();
            cx.defer(move |cx| {
                app.update(cx, |this, cx| {
                    this.panel_tab_scroll.scroll_to_item(this.panel_tab_index());
                    cx.notify();
                })
                .ok();
            });
        }
        let strip_left = STRIP_LEFT.max(titlebar_inset + TITLEBAR_TABS_GAP);
        let available =
            (card_width - strip_left - NEW_TAB_AREA - FULL_VIEW_AREA - TRAILING_CONTROLS).max(0.);
        let slot = self.panel_tab_frozen_width.unwrap_or_else(|| {
            (available / tabs.len().max(1) as f32).clamp(TAB_SLOT_MIN, TAB_SLOT_MAX)
        });
        let (chip, chip_border) = active_chip(mode, theme);
        // The strip paints the card's tint itself, and the tabs paint
        // opaque colors over it, which their titles' fades end in.
        let strip_fill = composite_over(theme.side_panel_tint, theme.surface);
        let chip_fill = composite_over(chip, strip_fill);
        let hover_fill = composite_over(theme.sidebar_hover, strip_fill);
        let mut strip = div()
            .id("panel-tabs")
            .role(Role::TabList)
            .aria_label(crate::i18n::format!("任务标签页" => "Task tabs"))
            .min_w(px(0.))
            .h_full()
            .flex()
            .items_center()
            .overflow_x_scroll()
            .track_scroll(&self.panel_tab_scroll);
        for (index, tab) in tabs.iter().copied().enumerate() {
            let selected = index == active;
            let (title, tab_glyph, unread, rename) = self.panel_tab_label(tab, cx);
            let key = tab.key();
            let group: SharedString = format!("panel-tab-{key}").into();
            let color = if selected {
                theme.text
            } else {
                theme.text_tertiary
            };
            // The hairline between two unselected tabs, hidden while either
            // is hovered.
            let hovered = self.panel_tab_hovered;
            let divided = !selected
                && index + 1 < tabs.len()
                && index + 1 != active
                && hovered != Some(index)
                && hovered != Some(index + 1);
            let drag = PanelTabDrag {
                index,
                title: title.clone(),
                glyph: tab_glyph.clone(),
                mode,
                width: slot,
            };
            let close_label = crate::i18n::format!("关闭{}标签页" => "Close {} tab", title);
            let label: AnyElement = match rename {
                Some(input) => div()
                    .id(SharedString::from(format!("panel-tab-rename-{key}")))
                    .min_w(px(0.))
                    .flex_1()
                    .ml(px(-4.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(input)
                    .into_any_element(),
                None => {
                    // `text-fade-truncate`: a long title fades out over its
                    // last 16px; browser tabs only fade while selected or
                    // hovered and are cut off otherwise.
                    let fade = |background: gpui::Rgba| {
                        linear_gradient(
                            90.,
                            linear_color_stop(background.alpha(0.), 0.),
                            linear_color_stop(background, 1.),
                        )
                    };
                    let browser = matches!(tab, PanelTab::Browser(_));
                    let overlay = div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .size_full()
                        .max_w(px(16.));
                    let overlay = if selected {
                        overlay.bg(fade(chip_fill))
                    } else if browser {
                        overlay.group_hover(group.clone(), move |style| style.bg(fade(hover_fill)))
                    } else {
                        overlay
                            .bg(fade(strip_fill))
                            .group_hover(group.clone(), move |style| style.bg(fade(hover_fill)))
                    };
                    // `-ms-1`: titles sit 4px from their glyph.
                    div()
                        .relative()
                        .min_w(px(0.))
                        .flex_1()
                        .ml(px(-4.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(title.clone())
                        .child(overlay)
                        .into_any_element()
                }
            };
            let chip_tab = div()
                .id(SharedString::from(format!("panel-tab-{key}")))
                .group(group.clone())
                .role(Role::Tab)
                .aria_label(title.clone())
                .aria_selected(selected)
                .relative()
                .w(px(slot - TAB_GAP))
                .h(px(TAB_HEIGHT))
                .flex_none()
                // The reference's 10px and 7px, less the 1px border its
                // overlay draws outside the layout.
                .pl(px(9.))
                .pr(px(6.))
                .rounded(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(13.))
                .line_height(px(18.5714))
                .text_color(color)
                .cursor_pointer()
                .when(selected, |chip_tab| {
                    chip_tab
                        .pr(px(26.))
                        .bg(chip_fill)
                        .border_1()
                        .border_color(chip_border)
                        .shadow(vec![
                            BoxShadow::new(px(0.), px(1.), rgba(0x00000014).into())
                                .blur_radius(px(2.))
                                .spread_radius(px(-1.)),
                        ])
                })
                .when(!selected, move |chip_tab| {
                    chip_tab
                        .border_1()
                        .border_color(gpui::transparent_black())
                        // The close button takes room from the title only
                        // while it shows.
                        .hover(move |style| style.bg(hover_fill).pr(px(26.)))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.activate_panel_tab(index, cx);
                }))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    let next = match (*hovered, this.panel_tab_hovered) {
                        (true, _) => Some(index),
                        (false, Some(current)) if current == index => None,
                        (false, current) => current,
                    };
                    if this.panel_tab_hovered != next {
                        this.panel_tab_hovered = next;
                        cx.notify();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        this.panel_tab_frozen_width = Some(slot);
                        this.close_panel_tab(index, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        // Browser tabs keep the reference's context menu.
                        if let PanelTab::Browser(id) = tab {
                            cx.stop_propagation();
                            this.activate_panel_tab(index, cx);
                            if let Some(panel) = this.browser_panels.get(&this.active_conversation)
                            {
                                panel.update(cx, |panel, cx| {
                                    panel.open_tab_menu(id, event.position, cx)
                                });
                            }
                        }
                    }),
                )
                .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                .drag_over::<PanelTabDrag>(move |style, _, _, _| style.bg(theme.sidebar_hover))
                .on_drop(cx.listener(move |this, drag: &PanelTabDrag, _, cx| {
                    this.move_panel_tab(drag.index, index, cx);
                }))
                .child(glyph(&tab_glyph, color))
                .child(label)
                .when(unread, |chip_tab| {
                    chip_tab.child(
                        div()
                            .id(SharedString::from(format!("panel-tab-unread-{key}")))
                            .role(Role::Status)
                            .aria_label(crate::i18n::text("未读回复"))
                            .flex_none()
                            .size(px(5.))
                            .rounded_full()
                            .bg(theme.markdown_link),
                    )
                })
                .child(
                    div()
                        .id(SharedString::from(format!("panel-tab-close-{key}")))
                        .role(Role::Button)
                        .aria_label(close_label)
                        .absolute()
                        .right(px(6.))
                        .top(px(2.))
                        .size(px(20.))
                        .rounded(px(10.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .when(!selected, |button| {
                            button
                                .opacity(0.)
                                .group_hover(group.clone(), |style| style.opacity(1.))
                        })
                        .hover(move |style| style.bg(theme.sidebar_hover))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.panel_tab_frozen_width = Some(slot);
                            this.close_panel_tab(index, cx);
                        }))
                        .child(icon("close-dialog", theme.text_tertiary.into()).size(px(14.))),
                );
            strip = strip.child(
                div()
                    .relative()
                    .flex_none()
                    .w(px(slot))
                    .h(px(TAB_HEIGHT))
                    .pr(px(TAB_GAP))
                    .child(chip_tab)
                    .when(divided, |slot_div| {
                        slot_div.child(
                            div()
                                .absolute()
                                .right_0()
                                .top(px(7.))
                                .w(px(1.))
                                .h(px(12.))
                                .bg(theme.border),
                        )
                    }),
            );
        }
        let fullscreen = self.right_panel.fullscreen;
        let full_view_label = if fullscreen {
            crate::i18n::text("退出全屏")
        } else {
            crate::i18n::text("进入全屏")
        };
        div()
            .id("panel-tab-strip")
            .h(px(crate::components::PANEL_TAB_STRIP_HEIGHT))
            .w_full()
            .flex_none()
            .bg(strip_fill)
            .pl(px(strip_left))
            .pr(px(TRAILING_CONTROLS))
            .flex()
            .items_center()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if *hovered {
                    return;
                }
                let unfrozen = this.panel_tab_frozen_width.take().is_some();
                let unhovered = this.panel_tab_hovered.take().is_some();
                if unfrozen || unhovered {
                    cx.notify();
                }
            }))
            .child(strip)
            .child(
                div().flex_none().pl(px(6.)).child(
                    div()
                        .id("panel-new-tab")
                        .role(Role::Button)
                        .aria_label(crate::i18n::format!("新标签页" => "New tab"))
                        .size(px(28.))
                        .rounded(px(10.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.sidebar_hover))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.open_new_panel_tab(cx);
                        }))
                        .child(icon("browser-new-tab", theme.text_tertiary.into()).size(px(16.))),
                ),
            )
            .child(div().flex_1().min_w(px(0.)))
            .child(
                div()
                    .flex_none()
                    .ml(px(6.))
                    .w(px(1.))
                    .h(px(16.))
                    .bg(theme.border),
            )
            .child(
                div()
                    .id("panel-full-view")
                    .ml(px(6.))
                    .role(Role::Button)
                    .aria_label(full_view_label)
                    .size(px(28.))
                    .flex_none()
                    .rounded(px(10.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.right_panel.fullscreen = !this.right_panel.fullscreen;
                        cx.notify();
                    }))
                    .child(icon("panel-full-view", theme.text_tertiary.into()).size(px(16.))),
            )
    }
}
