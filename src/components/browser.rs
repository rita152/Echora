//! The right panel's Browser (ChatGPT 26.924's in-app browser): a tab strip,
//! the toolbar with its address field, the New tab page, and native pages.
//!
//! Each chat has its own panel and tabs. A tab without a page shows the New
//! tab page (the reference's panel launcher: Tools and Suggested sites). A tab
//! with a page owns a [`WebView`] that the content area places over itself
//! every frame; GPUI overlays above the page (the address suggestions, menus,
//! the find bar, the zoom banner) cut holes into it so they stay visible and
//! clickable.

mod address_bar;
#[cfg(feature = "screenshot")]
mod capture;
mod menus;
mod new_tab;
mod page;
mod store;
mod tab_strip;
#[cfg(test)]
mod tests;
mod theme;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    App, Bounds, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable, Image,
    ImageFormat, KeyBinding, Pixels, Window, canvas, div, prelude::*,
};

pub use store::{BrowserStore, DownloadState};

use self::theme::BrowserTheme;
use crate::{
    browser::{
        address,
        error_page::{self, ErrorPage},
        history::Suggestions,
        session::{SavedSession, SavedTab},
        webview::{
            ClearData, ContextCommand, DownloadEvent, FindResult, MenuLabels, WebView,
            WebViewEvent, WebViewFrame, WebViewHole, WebViewHost,
        },
    },
    components::prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
    theme::ThemeMode,
};

gpui::actions!(
    browser,
    [
        /// ⌘T: a new tab (bound application-wide; the app opens the panel).
        NewBrowserTab,
        CloseBrowserTab,
        FocusAddress,
        ReloadPage,
        GoBack,
        GoForward,
        FindInPage,
        FindNext,
        FindPrevious,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        AddressUp,
        AddressDown,
        AddressEscape,
        FindEscape,
        MenuEscape
    ]
);

/// A web link from the app (chat markdown): the reference opens links in its
/// side panel browser by default.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(no_json)]
pub struct OpenLinkInBrowser {
    pub url: String,
}

/// Whether a link clicked in the app opens in the in-app browser: web pages
/// and local servers do, other schemes go to the system.
pub fn opens_in_browser(url: &str) -> bool {
    address::is_web_url(url) || address::is_local_url(url)
}

const PANEL_CONTEXT: &str = "BrowserPanel";
const ADDRESS_CONTEXT: &str = "BrowserAddress";
const FIND_CONTEXT: &str = "BrowserFind";
/// Chromium's zoom presets, which the reference steps through.
const ZOOM_LEVELS: [f64; 17] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0,
];
/// How long the zoom banner stays after a zoom command (`Ic`).
const ZOOM_BANNER_DURATION: Duration = Duration::from_millis(2_000);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-w", CloseBrowserTab, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-l", FocusAddress, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-r", ReloadPage, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-[", GoBack, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-]", GoForward, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-f", FindInPage, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-g", FindNext, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-shift-g", FindPrevious, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-=", ZoomIn, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-+", ZoomIn, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-shift-=", ZoomIn, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd--", ZoomOut, Some(PANEL_CONTEXT)),
        KeyBinding::new("cmd-0", ZoomReset, Some(PANEL_CONTEXT)),
        KeyBinding::new("up", AddressUp, Some(ADDRESS_CONTEXT)),
        KeyBinding::new("down", AddressDown, Some(ADDRESS_CONTEXT)),
        KeyBinding::new("escape", AddressEscape, Some(ADDRESS_CONTEXT)),
        KeyBinding::new("escape", FindEscape, Some(FIND_CONTEXT)),
        KeyBinding::new("escape", MenuEscape, Some(PANEL_CONTEXT)),
    ]);
}

/// Tools the New tab page opens in the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserTool {
    Review,
    Terminal,
    SideChat,
    Files,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserPanelEvent {
    OpenTool(BrowserTool),
    /// A confirmation or error for the app's toast stack. `undo_site` puts
    /// an Undo button on it that restores a dismissed suggested site.
    Toast {
        danger: bool,
        text: String,
        undo_site: Option<String>,
    },
    /// The last tab closed; the panel has a fresh New tab for next time.
    Closed,
}

/// A failed or crashed page, drawn by GPUI in the tab's content area.
#[derive(Clone, Debug)]
struct TabError {
    page: ErrorPage,
    /// What Reload loads again.
    url: String,
    details_open: bool,
}

struct BrowserTab {
    id: u64,
    view: Option<Rc<WebView>>,
    /// A URL to load once the panel knows its window (restored and new tabs).
    pending_url: Option<String>,
    url: String,
    title: String,
    custom_title: Option<String>,
    favicon: Option<Arc<Image>>,
    loading: bool,
    progress: f64,
    can_go_back: bool,
    can_go_forward: bool,
    error: Option<TabError>,
    zoom: f64,
    /// Text typed into the address field and not submitted.
    draft: Option<String>,
    find: FindResult,
}

impl BrowserTab {
    fn new(id: u64) -> Self {
        Self {
            id,
            view: None,
            pending_url: None,
            url: String::new(),
            title: String::new(),
            custom_title: None,
            favicon: None,
            loading: false,
            progress: 0.0,
            can_go_back: false,
            can_go_forward: false,
            error: None,
            zoom: 1.0,
            draft: None,
            find: FindResult::default(),
        }
    }

    /// No page yet: the New tab page.
    fn is_new_tab(&self) -> bool {
        self.url.is_empty() && self.pending_url.is_none() && self.view.is_none()
    }

    fn page_title(&self) -> String {
        if let Some(title) = &self.custom_title {
            return title.clone();
        }
        if !self.title.trim().is_empty() {
            return self.title.clone();
        }
        if self.is_new_tab() {
            return crate::i18n::format!("新标签页" => "New tab");
        }
        address::display_text(&self.url)
    }

    /// Back also leaves an error page for the page under it.
    fn can_go_back(&self) -> bool {
        self.can_go_back
            || (self.error.is_some()
                && self
                    .view
                    .as_ref()
                    .is_some_and(|view| !view.url().is_empty()))
    }
}

#[derive(Default)]
struct AddressState {
    focused: bool,
    /// What the user typed, without the inline completion.
    typed: String,
    suggestions: Suggestions,
    highlighted: Option<usize>,
    open: bool,
    hovered: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PanelMenu {
    Options,
    ClearData,
    Downloads,
    Tab(u64),
}

/// A change a native completion makes to the panel.
type PanelUpdate = Box<dyn FnOnce(&mut BrowserPanel, &mut Context<BrowserPanel>)>;

/// Work for the panel from native callbacks, applied on its own task so a
/// callback that runs synchronously (headless pages) never re-enters it.
enum PanelMessage {
    Event(u64, WebViewEvent),
    Apply(PanelUpdate),
}

struct FindBar {
    open: bool,
    input: Option<Entity<PromptInput>>,
    query: String,
}

pub struct BrowserPanel {
    mode: ThemeMode,
    store: Entity<BrowserStore>,
    /// The chat whose tabs are saved; drafts keep theirs in memory.
    chat: Option<String>,
    host: Option<WebViewHost>,
    tabs: Vec<BrowserTab>,
    active: usize,
    next_tab_id: u64,
    events: async_channel::Sender<PanelMessage>,
    address: Entity<PromptInput>,
    address_state: AddressState,
    menu: Option<PanelMenu>,
    /// Where a tab's context menu opens: the right click's position.
    menu_anchor: Option<gpui::Point<gpui::Pixels>>,
    find: FindBar,
    focus: FocusHandle,
    page_focus: FocusHandle,
    focus_page_pending: bool,
    focus_address_pending: bool,
    /// The app covers the panel with a dialog: the page is hidden.
    occluded: bool,
    side_chat_available: bool,
    /// Overlays over the page this frame, in window coordinates.
    holes: Rc<RefCell<Vec<WebViewHole>>>,
    /// The panel's width last frame, for the toolbar's container queries.
    panel_width: Rc<Cell<f32>>,
    /// Where the active page was placed last frame.
    page_frame: Rc<Cell<Option<WebViewFrame>>>,
    #[cfg(feature = "screenshot")]
    capture: capture::CaptureState,
    /// Window bounds of controls that menus open from.
    anchors: Rc<RefCell<HashMap<&'static str, Bounds<Pixels>>>>,
    zoom_banner: Option<Instant>,
    zoom_banner_hovered: bool,
    rename: Option<(u64, Entity<PromptInput>)>,
    hovered_tile: Option<String>,
    labels: MenuLabels,
}

impl EventEmitter<BrowserPanelEvent> for BrowserPanel {}

impl Focusable for BrowserPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BrowserPanel {
    pub fn new(
        mode: ThemeMode,
        store: Entity<BrowserStore>,
        chat: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (events, receiver) = async_channel::unbounded::<PanelMessage>();
        cx.spawn(async move |this, cx| {
            while let Ok(message) = receiver.recv().await {
                let applied = this.update(cx, |panel, cx| match message {
                    PanelMessage::Event(tab, event) => panel.handle_event(tab, event, cx),
                    PanelMessage::Apply(apply) => apply(panel, cx),
                });
                if applied.is_err() {
                    break;
                }
            }
        })
        .detach();
        let address = cx.new(|cx| {
            PromptInput::browser_address(
                mode,
                crate::i18n::format!("搜索或输入网址" => "Search or enter a URL"),
                cx,
            )
        });
        cx.subscribe(&address, |panel, _, _: &PromptChanged, cx| {
            panel.address_changed(cx)
        })
        .detach();
        cx.subscribe(&address, |panel, _, _: &PromptSubmitted, cx| {
            panel.submit_address(cx)
        })
        .detach();
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let mut panel = Self {
            mode,
            store,
            chat,
            host: None,
            tabs: Vec::new(),
            active: 0,
            next_tab_id: 1,
            events,
            address,
            address_state: AddressState::default(),
            menu: None,
            menu_anchor: None,
            find: FindBar {
                open: false,
                input: None,
                query: String::new(),
            },
            focus: cx.focus_handle(),
            page_focus: cx.focus_handle(),
            focus_page_pending: false,
            focus_address_pending: false,
            occluded: false,
            side_chat_available: false,
            holes: Rc::new(RefCell::new(Vec::new())),
            panel_width: Rc::new(Cell::new(0.)),
            page_frame: Rc::new(Cell::new(None)),
            #[cfg(feature = "screenshot")]
            capture: capture::CaptureState::default(),
            anchors: Rc::new(RefCell::new(HashMap::new())),
            zoom_banner: None,
            zoom_banner_hovered: false,
            rename: None,
            hovered_tile: None,
            labels: menu_labels(),
        };
        panel.restore_session(cx);
        if panel.tabs.is_empty() {
            panel.push_tab(BrowserTab::new(0));
        }
        panel
    }

    fn restore_session(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = &self.chat else {
            return;
        };
        let Some(session) = self.store.read(cx).session(chat) else {
            return;
        };
        for saved in session.tabs {
            let mut tab = BrowserTab::new(0);
            if !saved.url.is_empty() {
                tab.pending_url = Some(saved.url.clone());
                tab.url = saved.url;
            }
            tab.title = saved.title;
            tab.custom_title = saved.custom_title;
            self.push_tab(tab);
        }
        self.active = session.active.min(self.tabs.len().saturating_sub(1));
    }

    fn save_session(&self, cx: &mut Context<Self>) {
        let Some(chat) = self.chat.clone() else {
            return;
        };
        let session = SavedSession {
            tabs: self
                .tabs
                .iter()
                .map(|tab| SavedTab {
                    url: tab.url.clone(),
                    title: tab.title.clone(),
                    custom_title: tab.custom_title.clone(),
                })
                .collect(),
            active: self.active,
            updated_ms: chrono::Utc::now().timestamp_millis(),
        };
        self.store
            .update(cx, |store, _| store.save_session(&chat, session));
    }

    /// The chat this panel belongs to became a thread.
    pub fn set_chat(&mut self, chat: String, cx: &mut Context<Self>) {
        if self.chat.as_deref() != Some(chat.as_str()) {
            self.chat = Some(chat);
            self.save_session(cx);
        }
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        for tab in &self.tabs {
            if let Some(view) = &tab.view {
                view.set_dark_appearance(mode == ThemeMode::Dark);
            }
        }
        self.address
            .update(cx, |address, cx| address.set_mode(mode, cx));
        if let Some(input) = &self.find.input {
            input.update(cx, |input, cx| input.set_mode(mode, cx));
        }
        cx.notify();
    }

    pub fn set_side_chat_available(&mut self, available: bool, cx: &mut Context<Self>) {
        if self.side_chat_available != available {
            self.side_chat_available = available;
            cx.notify();
        }
    }

    /// A dialog of the app covers the window: hide the page so the dialog's
    /// backdrop is not painted under it.
    pub fn set_occluded(&mut self, occluded: bool, cx: &mut Context<Self>) {
        if self.occluded != occluded {
            self.occluded = occluded;
            if occluded {
                self.hide_pages();
            }
            cx.notify();
        }
    }

    pub fn is_occluded(&self) -> bool {
        self.occluded
    }

    /// The panel left the screen (another tool, another chat, a closed
    /// panel): hide every page. The content area shows the active one again
    /// when it paints.
    pub fn hide_pages(&self) {
        for tab in &self.tabs {
            if let Some(view) = &tab.view {
                view.set_visible(false);
            }
        }
    }

    fn push_tab(&mut self, mut tab: BrowserTab) -> usize {
        tab.id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(tab);
        self.tabs.len() - 1
    }

    fn insert_tab(&mut self, index: usize, mut tab: BrowserTab) -> usize {
        tab.id = self.next_tab_id;
        self.next_tab_id += 1;
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, tab);
        index
    }

    fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    fn active_tab(&self) -> Option<&BrowserTab> {
        self.tabs.get(self.active)
    }

    fn active_tab_mut(&mut self) -> Option<&mut BrowserTab> {
        self.tabs.get_mut(self.active)
    }

    fn active_view(&self) -> Option<Rc<WebView>> {
        self.active_tab().and_then(|tab| tab.view.clone())
    }

    /// The page area shows a native page (not the New tab page or an error).
    fn showing_page(&self) -> bool {
        self.active_tab()
            .is_some_and(|tab| tab.view.is_some() && tab.error.is_none())
    }

    /// GPUI is about to take the keyboard (⌘L, ⌘F, a new tab): the page
    /// gives up being AppKit's first responder, or keys would keep going to
    /// it.
    fn release_page_keyboard(&self) {
        if let Some(view) = self.active_view() {
            view.blur();
        }
    }

    /// Records a control's window bounds for the menu that opens from it.
    fn anchor(&self, key: &'static str) -> impl IntoElement {
        let anchors = self.anchors.clone();
        canvas(
            move |bounds, _, _| {
                anchors.borrow_mut().insert(key, bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    fn anchor_bounds(&self, key: &'static str) -> Option<Bounds<Pixels>> {
        self.anchors.borrow().get(key).copied()
    }

    /// Fills an overlay that sits over the page: at paint it hands the
    /// overlay's rounded rectangle to the page, which leaves it to GPUI.
    fn overlay_hole(&self, corner_radius: f32) -> impl IntoElement {
        let holes = self.holes.clone();
        let view = self.active_view().filter(|_| self.showing_page());
        canvas(
            |_, _, _| {},
            move |bounds, _, _, _| {
                let Some(view) = &view else {
                    return;
                };
                holes.borrow_mut().push(WebViewHole {
                    x: f32::from(bounds.origin.x),
                    y: f32::from(bounds.origin.y),
                    width: f32::from(bounds.size.width),
                    height: f32::from(bounds.size.height),
                    corner_radius,
                });
                view.set_holes(&holes.borrow());
            },
        )
        .absolute()
        .inset_0()
    }

    fn toggle_menu(&mut self, menu: PanelMenu, cx: &mut Context<Self>) {
        self.menu = if self.menu == Some(menu) {
            None
        } else {
            Some(menu)
        };
        self.address_state.open = false;
        cx.notify();
    }

    /// Sends work back to the panel from a native completion.
    fn reply(&self) -> impl Fn(PanelUpdate) + 'static {
        let events = self.events.clone();
        move |apply| {
            let _ = events.try_send(PanelMessage::Apply(apply));
        }
    }

    fn handler(&self, tab: u64) -> crate::browser::webview::EventHandler {
        let events = self.events.clone();
        Rc::new(move |event| {
            let _ = events.try_send(PanelMessage::Event(tab, event));
        })
    }

    /// Opens a new tab with `url` (or the New tab page), activates it and
    /// focuses its address field when it has no page.
    pub fn new_tab(&mut self, url: Option<String>, cx: &mut Context<Self>) {
        let index = self.insert_tab(self.tabs.len(), BrowserTab::new(0));
        self.activate(index, cx);
        match url {
            Some(url) => self.navigate(url, cx),
            None => self.focus_address_pending = true,
        }
        self.save_session(cx);
        cx.notify();
    }

    /// Shows the panel's current tab; a panel opened from the app without a
    /// tab gets the New tab page focused.
    pub fn focus_new_tab_address(&mut self, cx: &mut Context<Self>) {
        if self.active_tab().is_some_and(BrowserTab::is_new_tab) {
            self.focus_address_pending = true;
            cx.notify();
        }
    }

    /// Opens a link from the app (a chat link, a file) in a new tab, or in the
    /// current tab when it is still empty.
    pub fn open_url(&mut self, url: String, cx: &mut Context<Self>) {
        if self.active_tab().is_some_and(BrowserTab::is_new_tab) {
            self.navigate(url, cx);
            self.save_session(cx);
        } else {
            self.new_tab(Some(url), cx);
        }
    }

    fn open_tab_after(&mut self, after: usize, url: Option<String>, cx: &mut Context<Self>) {
        let index = self.insert_tab(after + 1, BrowserTab::new(0));
        self.activate(index, cx);
        match url {
            Some(url) => self.navigate(url, cx),
            None => self.focus_address_pending = true,
        }
        self.save_session(cx);
        cx.notify();
    }

    fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        if index != self.active {
            if let Some(view) = self.active_view() {
                view.set_visible(false);
            }
            self.close_find(cx);
        }
        self.active = index;
        self.menu = None;
        self.address_state.open = false;
        self.sync_address_text(cx);
        cx.notify();
    }

    pub fn select_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.activate(index, cx);
        self.save_session(cx);
    }

    pub fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        let closing_active = index == self.active;
        self.tabs.remove(index);
        if self
            .rename
            .as_ref()
            .is_some_and(|(id, _)| self.tab_index(*id).is_none())
        {
            self.rename = None;
        }
        if self.tabs.is_empty() {
            self.push_tab(BrowserTab::new(0));
            self.active = 0;
            self.menu = None;
            self.close_find(cx);
            self.sync_address_text(cx);
            self.save_session(cx);
            cx.emit(BrowserPanelEvent::Closed);
            cx.notify();
            return;
        }
        if index < self.active || (closing_active && self.active >= self.tabs.len()) {
            self.active = self.active.saturating_sub(1);
        }
        if closing_active {
            self.close_find(cx);
            self.sync_address_text(cx);
        }
        self.menu = None;
        self.save_session(cx);
        cx.notify();
    }

    /// Loads `url` in the active tab. The page is created on the next frame,
    /// once the window is known.
    fn navigate(&mut self, url: String, cx: &mut Context<Self>) {
        if url.trim().is_empty() {
            return;
        }
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        tab.error = None;
        tab.draft = None;
        tab.url = url.clone();
        tab.loading = true;
        tab.progress = 0.0;
        match &tab.view {
            Some(view) => view.load_url(&url),
            None => tab.pending_url = Some(url),
        }
        // As a browser does after a navigation, the page takes the keyboard.
        self.focus_address_pending = false;
        self.focus_page_pending = true;
        self.address_state.open = false;
        self.sync_address_text(cx);
        cx.notify();
    }

    /// Creates pages for the active tab once a window is known, and gives
    /// the page or the address field the focus they asked for.
    fn prepare_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = *self.host.get_or_insert_with(|| {
            use raw_window_handle::HasWindowHandle as _;
            WebViewHost::from_raw(window.window_handle().ok().map(|handle| handle.as_raw()))
        });
        let labels = self.labels.clone();
        let handler = self.active_tab().map(|tab| self.handler(tab.id));
        if let (Some(tab), Some(handler)) = (self.tabs.get_mut(self.active), handler)
            && let Some(url) = tab.pending_url.take()
        {
            let view = tab
                .view
                .get_or_insert_with(|| Rc::new(WebView::new(host, labels, handler)))
                .clone();
            view.set_zoom(tab.zoom);
            view.set_dark_appearance(self.mode == ThemeMode::Dark);
            view.load_url(&url);
            tab.loading = true;
        }
        if self.focus_address_pending {
            self.focus_address_pending = false;
            self.release_page_keyboard();
            let handle = self.address.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        }
        if self.focus_page_pending {
            self.focus_page_pending = false;
            if self.showing_page()
                && let Some(view) = self.active_view()
            {
                window.focus(&self.page_focus, cx);
                view.focus();
            }
        }
        let focused = self.address.read(cx).focus_handle(cx).is_focused(window);
        if focused != self.address_state.focused {
            self.address_state.focused = focused;
            if focused {
                self.address_focused(cx);
            } else {
                self.address_blurred(cx);
            }
        }
        #[cfg(feature = "screenshot")]
        self.apply_capture_frame(window, cx);
    }

    fn handle_event(&mut self, tab_id: u64, event: WebViewEvent, cx: &mut Context<Self>) {
        let Some(index) = self.tab_index(tab_id) else {
            return;
        };
        let active = index == self.active;
        match event {
            WebViewEvent::Title(title) => {
                let tab = &mut self.tabs[index];
                tab.title = title.clone();
                let url = tab.url.clone();
                self.store
                    .update(cx, |store, cx| store.update_page(&url, Some(&title), cx));
                self.save_session(cx);
            }
            WebViewEvent::Url(url) => {
                if !url.is_empty() && url != "about:blank" {
                    let tab = &mut self.tabs[index];
                    if tab.url != url {
                        tab.url = url;
                        tab.favicon = None;
                    }
                    if active && !self.address_state.focused {
                        self.sync_address_text(cx);
                    }
                }
            }
            WebViewEvent::Loading(loading) => {
                let tab = &mut self.tabs[index];
                tab.loading = loading;
                if loading && tab.progress >= 1.0 {
                    tab.progress = 0.0;
                }
            }
            WebViewEvent::Progress(progress) => self.tabs[index].progress = progress,
            WebViewEvent::History {
                can_go_back,
                can_go_forward,
            } => {
                let tab = &mut self.tabs[index];
                tab.can_go_back = can_go_back;
                tab.can_go_forward = can_go_forward;
            }
            WebViewEvent::Finished { url, title } => {
                let tab = &mut self.tabs[index];
                tab.error = None;
                tab.loading = false;
                if !url.is_empty() && url != "about:blank" {
                    tab.url = url.clone();
                    if !title.is_empty() {
                        tab.title = title.clone();
                    }
                    self.store
                        .update(cx, |store, cx| store.record_visit(&url, &title, cx));
                }
                self.save_session(cx);
                #[cfg(feature = "screenshot")]
                if active {
                    self.capture_page_loaded(cx);
                }
            }
            WebViewEvent::Failed(error) => {
                let tab = &mut self.tabs[index];
                let url = if error.url.is_empty() {
                    tab.url.clone()
                } else {
                    error.url.clone()
                };
                tab.loading = false;
                tab.url = url.clone();
                tab.error = Some(TabError {
                    page: error_page::load_error(&url, &error.code),
                    url,
                    details_open: false,
                });
                if active {
                    self.sync_address_text(cx);
                }
            }
            WebViewEvent::Crashed => {
                let tab = &mut self.tabs[index];
                tab.loading = false;
                let url = tab.url.clone();
                tab.error = Some(TabError {
                    page: error_page::crash(&url),
                    url,
                    details_open: false,
                });
            }
            WebViewEvent::Favicon(png) => {
                let image = png.map(|bytes| Arc::new(Image::from_bytes(ImageFormat::Png, bytes)));
                if let (Some(image), Some(host)) = (&image, address::host(&self.tabs[index].url)) {
                    let image = image.clone();
                    self.store
                        .update(cx, |store, _| store.set_favicon(host, image));
                }
                self.tabs[index].favicon = image;
            }
            WebViewEvent::OpenInNewTab(url) => self.open_tab_after(index, Some(url), cx),
            WebViewEvent::NewWindow(view) => {
                let position = self.insert_tab(index + 1, BrowserTab::new(0));
                let id = self.tabs[position].id;
                view.set_handler(self.handler(id));
                view.set_dark_appearance(self.mode == ThemeMode::Dark);
                let tab = &mut self.tabs[position];
                tab.url = view.url();
                tab.loading = true;
                tab.view = Some(Rc::new(view));
                self.activate(position, cx);
                self.save_session(cx);
            }
            WebViewEvent::CloseRequested => self.close_tab(index, cx),
            WebViewEvent::Focused(focused) => {
                if focused && active {
                    self.menu = None;
                    self.address_state.open = false;
                    self.focus_page_pending = true;
                }
            }
            WebViewEvent::Context(command) => match command {
                ContextCommand::Back => self.go_back(cx),
                ContextCommand::Forward => self.go_forward(cx),
                ContextCommand::Reload => self.reload(cx),
                ContextCommand::OpenExternal(url) => cx.open_url(&url),
            },
            WebViewEvent::Download(event) => self.download_event(event, cx),
        }
        cx.notify();
    }

    fn download_event(&mut self, event: DownloadEvent, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            match event {
                DownloadEvent::Started { id, url, filename } => {
                    store.downloads.insert(
                        0,
                        store::Download {
                            id,
                            url,
                            filename,
                            path: None,
                            state: DownloadState::InProgress(0.0),
                        },
                    );
                }
                DownloadEvent::Destination { id, path } => {
                    if let Some(download) = store.download_mut(id) {
                        if let Some(name) = path.file_name() {
                            download.filename = name.to_string_lossy().into_owned();
                        }
                        download.path = Some(path);
                    }
                }
                DownloadEvent::Progress { id, fraction } => {
                    if let Some(download) = store.download_mut(id)
                        && matches!(download.state, DownloadState::InProgress(_))
                    {
                        download.state = DownloadState::InProgress(fraction);
                    }
                }
                DownloadEvent::Finished { id } => {
                    if let Some(download) = store.download_mut(id) {
                        download.state = DownloadState::Finished;
                    }
                }
                DownloadEvent::Failed { id, message } => {
                    if let Some(download) = store.download_mut(id) {
                        download.state = DownloadState::Failed(message);
                    }
                }
            }
            cx.notify();
        });
        if self.menu.is_none() {
            self.menu = Some(PanelMenu::Downloads);
        }
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        if tab.error.is_some()
            && let Some(view) = tab.view.clone()
            && !view.url().is_empty()
        {
            tab.error = None;
            tab.url = view.url();
            tab.title = view.title();
        } else if let Some(view) = &tab.view {
            view.go_back();
        }
        self.sync_address_text(cx);
        cx.notify();
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.active_view() {
            view.go_forward();
        }
        cx.notify();
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        if let Some(error) = &tab.error {
            let url = error.url.clone();
            self.navigate(url, cx);
        } else if let Some(view) = &tab.view {
            view.reload();
            let tab = &mut self.tabs[self.active];
            tab.loading = true;
            tab.progress = 0.0;
        }
        cx.notify();
    }

    fn open_external(&mut self, url: &str, cx: &mut Context<Self>) {
        if address::is_web_url(url) || url.starts_with("file:") {
            cx.open_url(url);
        }
    }

    fn copy_url(&mut self, url: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(url));
        self.toast(
            false,
            crate::i18n::format!("URL 已复制到剪贴板" => "URL copied to clipboard"),
            cx,
        );
    }

    fn toast(&mut self, danger: bool, text: String, cx: &mut Context<Self>) {
        cx.emit(BrowserPanelEvent::Toast {
            danger,
            text,
            undo_site: None,
        });
    }

    fn step_zoom(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        if tab.view.is_none() {
            return;
        }
        let current = tab.zoom;
        let next = match delta.cmp(&0) {
            std::cmp::Ordering::Greater => ZOOM_LEVELS
                .iter()
                .copied()
                .find(|level| *level > current + 1e-6),
            std::cmp::Ordering::Less => ZOOM_LEVELS
                .iter()
                .rev()
                .copied()
                .find(|level| *level < current - 1e-6),
            std::cmp::Ordering::Equal => Some(1.0),
        };
        if let Some(next) = next {
            tab.zoom = next;
            if let Some(view) = &tab.view {
                view.set_zoom(next);
            }
        }
        self.show_zoom_banner(cx);
        cx.notify();
    }

    fn show_zoom_banner(&mut self, cx: &mut Context<Self>) {
        let shown = Instant::now();
        self.zoom_banner = Some(shown);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ZOOM_BANNER_DURATION).await;
            let _ = this.update(cx, |panel, cx| panel.expire_zoom_banner(shown, cx));
        })
        .detach();
    }

    fn expire_zoom_banner(&mut self, shown: Instant, cx: &mut Context<Self>) {
        if self.zoom_banner != Some(shown) {
            return;
        }
        if self.zoom_banner_hovered {
            // Hovering keeps the banner; leaving it starts the timer again.
            return;
        }
        self.zoom_banner = None;
        cx.notify();
    }

    fn copy_screenshot(&mut self) {
        let Some(view) = self.active_view() else {
            return;
        };
        let reply = self.reply();
        view.copy_snapshot(move |copied| {
            let text = if copied {
                crate::i18n::format!("截图已保存到剪贴板" => "Screenshot saved to clipboard")
            } else {
                crate::i18n::format!("无法截取屏幕截图" => "Unable to capture screenshot")
            };
            reply(Box::new(move |panel, cx| panel.toast(!copied, text, cx)));
        });
    }

    fn clear_data(&mut self, kind: ClearData, cx: &mut Context<Self>) {
        self.menu = None;
        let reply = self.reply();
        crate::browser::webview::clear_browsing_data(kind, move || {
            let text = match kind {
                ClearData::Cookies => {
                    crate::i18n::format!("已清除浏览器 Cookie" => "Browser cookies cleared")
                }
                _ => crate::i18n::format!("浏览器缓存已清除" => "Browser cache cleared"),
            };
            reply(Box::new(move |panel, cx| panel.toast(false, text, cx)));
        });
        cx.notify();
    }

    fn clear_download_history(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        self.store.update(cx, |store, cx| store.clear_downloads(cx));
        self.toast(
            false,
            crate::i18n::format!("浏览器下载历史记录已清除" => "Browser download history cleared"),
            cx,
        );
        cx.notify();
    }

    fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.showing_page() {
            return;
        }
        let input = self
            .find
            .input
            .get_or_insert_with(|| {
                let input = cx.new(|cx| {
                    let mut input = PromptInput::chat_search(
                        self.mode,
                        crate::i18n::format!("在页面中查找…" => "Find in page…"),
                        cx,
                    );
                    input.set_accessible_name(
                        crate::i18n::format!("在页面中查找" => "Find in page"),
                    );
                    input
                });
                cx.subscribe(&input, |panel, input, _: &PromptChanged, cx| {
                    let query = input.read(cx).text().to_owned();
                    panel.run_find(query, false);
                })
                .detach();
                cx.subscribe(&input, |panel, _, _: &PromptSubmitted, _| {
                    panel.find_step(false)
                })
                .detach();
                input
            })
            .clone();
        self.find.open = true;
        self.menu = None;
        self.release_page_keyboard();
        input.update(cx, |input, cx| input.select_all_text(cx));
        window.focus(&input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    fn close_find(&mut self, cx: &mut Context<Self>) {
        if !self.find.open {
            return;
        }
        self.find.open = false;
        self.find.query.clear();
        if let Some(input) = &self.find.input {
            input.update(cx, |input, cx| input.clear(cx));
        }
        if let Some(tab) = self.active_tab_mut() {
            tab.find = FindResult::default();
            if let Some(view) = &tab.view {
                view.clear_find();
            }
        }
        self.focus_page_pending = self.showing_page();
        cx.notify();
    }

    fn run_find(&mut self, query: String, backwards: bool) {
        self.find.query = query.clone();
        let Some(tab) = self.active_tab() else {
            return;
        };
        let Some(view) = tab.view.clone() else {
            return;
        };
        let tab_id = tab.id;
        let reply = self.reply();
        let search = query.clone();
        view.find(&search, backwards, move |result| {
            reply(Box::new(move |panel, cx| {
                if panel.find.query != query {
                    return;
                }
                if let Some(index) = panel.tab_index(tab_id) {
                    panel.tabs[index].find = result;
                    cx.notify();
                }
            }));
        });
    }

    fn find_step(&mut self, backwards: bool) {
        if self.find.open && !self.find.query.is_empty() {
            let query = self.find.query.clone();
            self.run_find(query, backwards);
        }
    }

    fn rename_tab(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(index) = self.tab_index(id) else {
            return;
        };
        let title = self.tabs[index].page_title();
        let input = cx.new(|cx| {
            let mut input = PromptInput::inline_other(self.mode, "", false, cx);
            input.set_rename_text(&title, cx);
            input
        });
        cx.subscribe(&input, move |panel, input, _: &PromptSubmitted, cx| {
            let text = input.read(cx).text().trim().to_owned();
            panel.finish_rename(id, Some(text), cx);
        })
        .detach();
        self.rename = Some((id, input));
        self.menu = None;
        cx.notify();
    }

    fn finish_rename(&mut self, id: u64, title: Option<String>, cx: &mut Context<Self>) {
        if self
            .rename
            .as_ref()
            .is_some_and(|(renaming, _)| *renaming == id)
        {
            self.rename = None;
        }
        if let (Some(index), Some(title)) = (self.tab_index(id), title) {
            self.tabs[index].custom_title = (!title.is_empty()).then_some(title);
            self.save_session(cx);
        }
        cx.notify();
    }

    fn duplicate_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let url = self
            .tabs
            .get(index)
            .map(|tab| tab.url.clone())
            .unwrap_or_default();
        self.open_tab_after(index, (!url.is_empty()).then_some(url), cx);
    }

    fn open_tool(&mut self, tool: BrowserTool, cx: &mut Context<Self>) {
        cx.emit(BrowserPanelEvent::OpenTool(tool));
    }

    fn dismiss_site(&mut self, url: String, cx: &mut Context<Self>) {
        self.store
            .update(cx, |store, cx| store.dismiss_top_site(&url, cx));
        self.hovered_tile = None;
        cx.emit(BrowserPanelEvent::Toast {
            danger: false,
            text: crate::i18n::format!("已忽略建议" => "Suggestion dismissed"),
            undo_site: Some(url),
        });
    }

    /// Captures and app tests: the panel's tabs as (title, url).
    #[cfg(test)]
    pub fn tab_summaries(&self) -> Vec<(String, String)> {
        self.tabs
            .iter()
            .map(|tab| (tab.page_title(), tab.url.clone()))
            .collect()
    }
}

fn menu_labels() -> MenuLabels {
    MenuLabels {
        open_link_in_new_tab: crate::i18n::format!("在新标签页中打开链接" => "Open link in new tab"),
        open_in_external_browser: crate::i18n::format!(
            "在外部浏览器中打开" => "Open in external browser"
        ),
        copy_link_address: crate::i18n::format!("复制链接地址" => "Copy link address"),
        back: crate::i18n::format!("返回" => "Back"),
        forward: crate::i18n::format!("前进" => "Forward"),
        reload: crate::i18n::format!("重新加载" => "Reload"),
        inspect: crate::i18n::format!("检查" => "Inspect"),
        ok: crate::i18n::format!("好" => "OK"),
        cancel: crate::i18n::format!("取消" => "Cancel"),
    }
}

impl Render for BrowserPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.prepare_frame(window, cx);
        let theme = BrowserTheme::for_mode(self.mode);
        // Pages of other tabs never show.
        for (index, tab) in self.tabs.iter().enumerate() {
            if index != self.active
                && let Some(view) = &tab.view
            {
                view.set_visible(false);
            }
        }
        if !self.showing_page()
            && let Some(view) = self.active_view()
        {
            view.set_visible(false);
        }
        self.holes.borrow_mut().clear();
        let width = self.panel_width.clone();
        let entity = cx.entity().downgrade();
        let measure = canvas(
            move |bounds, _, cx| {
                let next = f32::from(bounds.size.width);
                if (width.get() - next).abs() > 0.5 {
                    width.set(next);
                    if let Some(entity) = entity.upgrade() {
                        entity.update(cx, |_, cx| cx.notify());
                    }
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        div()
            .id("browser-panel")
            .child(measure)
            .key_context(PANEL_CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_action(
                cx.listener(|panel, _: &CloseBrowserTab, _, cx| panel.close_tab(panel.active, cx)),
            )
            .on_action(cx.listener(|panel, _: &FocusAddress, window, cx| {
                panel.release_page_keyboard();
                let handle = panel.address.read(cx).focus_handle(cx);
                window.focus(&handle, cx);
            }))
            .on_action(cx.listener(|panel, _: &ReloadPage, _, cx| panel.reload(cx)))
            .on_action(cx.listener(|panel, _: &GoBack, _, cx| panel.go_back(cx)))
            .on_action(cx.listener(|panel, _: &GoForward, _, cx| panel.go_forward(cx)))
            .on_action(cx.listener(|panel, _: &FindInPage, window, cx| panel.open_find(window, cx)))
            .on_action(cx.listener(|panel, _: &FindNext, _, _| panel.find_step(false)))
            .on_action(cx.listener(|panel, _: &FindPrevious, _, _| panel.find_step(true)))
            .on_action(cx.listener(|panel, _: &ZoomIn, _, cx| panel.step_zoom(1, cx)))
            .on_action(cx.listener(|panel, _: &ZoomOut, _, cx| panel.step_zoom(-1, cx)))
            .on_action(cx.listener(|panel, _: &ZoomReset, _, cx| panel.step_zoom(0, cx)))
            .on_action(cx.listener(|panel, _: &MenuEscape, _, cx| {
                if panel.menu.take().is_some() || panel.rename.take().is_some() {
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .child(self.render_tab_strip(theme, cx))
            .child(self.render_toolbar(theme, window, cx))
            .child(self.render_content(theme, window, cx))
            .children(self.render_find_bar(theme, cx))
            .children(self.render_address_dropdown(theme, cx))
            .children(self.render_menu(theme, cx))
    }
}
