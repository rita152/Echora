//! A native web view hosted beside the GPUI view in the app window.
//!
//! On macOS this is a `WKWebView` added to the window's content view above
//! GPUI's own view; the browser panel places it over its content area every
//! frame and hides it whenever that area is not on screen. Everywhere else,
//! and in tests (GPUI's test windows have no native handle), a headless
//! stand-in records commands and reports navigations as if they had loaded.

use std::rc::Rc;

#[cfg(target_os = "macos")]
mod macos;

/// The window a web view lives in, from GPUI's `raw-window-handle`.
#[derive(Clone, Copy, Debug)]
pub enum WebViewHost {
    /// AppKit's `NSView` of a GPUI window.
    AppKit(*mut std::ffi::c_void),
    Headless,
}

impl WebViewHost {
    pub fn from_raw(handle: Option<raw_window_handle::RawWindowHandle>) -> Self {
        match handle {
            #[cfg(target_os = "macos")]
            Some(raw_window_handle::RawWindowHandle::AppKit(handle)) => {
                Self::AppKit(handle.ns_view.as_ptr())
            }
            _ => Self::Headless,
        }
    }
}

/// A rectangle in the window's coordinates (points, origin at the top left).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WebViewFrame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Radius of the two bottom corners, so the page follows a rounded card.
    pub bottom_corner_radius: f32,
    /// A strip along the left edge where clicks reach GPUI instead of the
    /// page: the panel's resize handle straddles the card's edge.
    pub pointer_inset_left: f32,
}

/// A GPUI overlay over the page (a menu, the address suggestions, the find
/// bar): the native page leaves this rounded rectangle to GPUI, for painting
/// and for the pointer. Window coordinates, like [`WebViewFrame`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WebViewHole {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub corner_radius: f32,
}

/// Commands from the page's own context menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextCommand {
    Back,
    Forward,
    Reload,
    OpenExternal(String),
}

/// Titles the native page UI shows, in the app's language.
#[derive(Clone, Debug)]
pub struct MenuLabels {
    pub open_link_in_new_tab: String,
    pub open_in_external_browser: String,
    pub copy_link_address: String,
    pub back: String,
    pub forward: String,
    pub reload: String,
    pub inspect: String,
    /// Buttons of the page's `alert`, `confirm` and `prompt` sheets.
    pub ok: String,
    pub cancel: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindResult {
    pub matches: usize,
    /// 1-based index of the active match, 0 without matches.
    pub active: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationError {
    pub url: String,
    /// Chromium's error name for the failure (`ERR_NAME_NOT_RESOLVED`…), so
    /// the error page can use the reference's wording.
    pub code: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DownloadEvent {
    Started {
        id: u64,
        url: String,
        filename: String,
    },
    Destination {
        id: u64,
        path: std::path::PathBuf,
    },
    Progress {
        id: u64,
        fraction: f64,
    },
    Finished {
        id: u64,
    },
    Failed {
        id: u64,
        message: String,
    },
}

pub enum WebViewEvent {
    Title(String),
    Url(String),
    Loading(bool),
    Progress(f64),
    History {
        can_go_back: bool,
        can_go_forward: bool,
    },
    /// A main-frame navigation finished loading.
    Finished {
        url: String,
        title: String,
    },
    Failed(NavigationError),
    /// The page's web content process went away.
    Crashed,
    /// The page's icon, rendered to a 32×32 PNG, or `None` when it has none.
    Favicon(Option<Vec<u8>>),
    /// A link asked to open in a new tab (⌘-click, middle click).
    OpenInNewTab(String),
    /// `window.open` or a `target=_blank` link: the new page already exists
    /// and keeps its opener, so it becomes a tab as it is.
    NewWindow(WebView),
    /// The page called `window.close()`.
    CloseRequested,
    Focused(bool),
    Context(ContextCommand),
    Download(DownloadEvent),
}

pub type EventHandler = Rc<dyn Fn(WebViewEvent)>;

/// One page. Dropping it removes the native view from the window.
pub struct WebView {
    inner: Inner,
}

enum Inner {
    #[cfg(target_os = "macos")]
    Native(macos::NativeWebView),
    Headless(headless::HeadlessWebView),
}

macro_rules! forward {
    ($self:ident, $view:ident => $body:expr) => {
        match &$self.inner {
            #[cfg(target_os = "macos")]
            Inner::Native($view) => $body,
            Inner::Headless($view) => $body,
        }
    };
}

impl WebView {
    pub fn new(host: WebViewHost, labels: MenuLabels, handler: EventHandler) -> Self {
        let inner = match host {
            #[cfg(target_os = "macos")]
            WebViewHost::AppKit(view) => {
                Inner::Native(macos::NativeWebView::new(view.cast(), labels, handler))
            }
            _ => {
                let _ = labels;
                Inner::Headless(headless::HeadlessWebView::new(handler))
            }
        };
        Self { inner }
    }

    /// Routes this page's events to `handler`; a page created by `window.open`
    /// keeps its events until a tab adopts it.
    pub fn set_handler(&self, handler: EventHandler) {
        forward!(self, view => view.set_handler(handler))
    }

    pub fn load_url(&self, url: &str) {
        forward!(self, view => view.load_url(url))
    }

    pub fn go_back(&self) {
        forward!(self, view => view.go_back())
    }

    pub fn go_forward(&self) {
        forward!(self, view => view.go_forward())
    }

    pub fn reload(&self) {
        forward!(self, view => view.reload())
    }

    pub fn set_frame(&self, frame: WebViewFrame) {
        forward!(self, view => view.set_frame(frame))
    }

    pub fn set_visible(&self, visible: bool) {
        forward!(self, view => view.set_visible(visible))
    }

    /// Replaces the overlays the page leaves to GPUI this frame.
    pub fn set_holes(&self, holes: &[WebViewHole]) {
        forward!(self, view => view.set_holes(holes))
    }

    /// Gives the page the keyboard.
    pub fn focus(&self) {
        forward!(self, view => view.focus())
    }

    /// Takes the keyboard back for the GPUI view when the page has it.
    pub fn blur(&self) {
        forward!(self, view => view.blur())
    }

    pub fn set_zoom(&self, zoom: f64) {
        forward!(self, view => view.set_zoom(zoom))
    }

    /// Follows the app's theme: pages see it as `prefers-color-scheme`, and
    /// the page's sheets and menus are drawn in it.
    pub fn set_dark_appearance(&self, dark: bool) {
        forward!(self, view => view.set_dark_appearance(dark))
    }

    pub fn url(&self) -> String {
        forward!(self, view => view.url())
    }

    pub fn title(&self) -> String {
        forward!(self, view => view.title())
    }

    pub fn find(&self, query: &str, backwards: bool, done: impl FnOnce(FindResult) + 'static) {
        forward!(self, view => view.find(query, backwards, Box::new(done)))
    }

    pub fn clear_find(&self) {
        forward!(self, view => view.clear_find())
    }

    /// Copies what the page shows to the clipboard as an image.
    pub fn copy_snapshot(&self, done: impl FnOnce(bool) + 'static) {
        forward!(self, view => view.copy_snapshot(Box::new(done)))
    }

    pub fn print(&self) {
        forward!(self, view => view.print())
    }

    /// Captures only: a PNG of what the page shows.
    #[cfg(feature = "screenshot")]
    pub fn snapshot_png(&self, done: impl FnOnce(Option<Vec<u8>>) + 'static) {
        forward!(self, view => view.snapshot_png(Box::new(done)))
    }
}

/// Removes cookies, site data and caches from the browser's data store.
pub fn clear_browsing_data(kinds: ClearData, done: impl FnOnce() + 'static) {
    #[cfg(target_os = "macos")]
    {
        macos::clear_browsing_data(kinds, Box::new(done));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kinds;
        done();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearData {
    Cookies,
    Cache,
}

mod headless {
    use std::cell::RefCell;

    use super::*;

    /// Stands in for a native page where there is no window to host one. It
    /// "loads" every URL at once, keeping a back/forward list, so tab and
    /// history logic can run in tests.
    pub struct HeadlessWebView {
        handler: RefCell<EventHandler>,
        state: RefCell<State>,
    }

    #[derive(Default)]
    struct State {
        entries: Vec<String>,
        index: usize,
        visible: bool,
        zoom: f64,
    }

    impl HeadlessWebView {
        pub fn new(handler: EventHandler) -> Self {
            Self {
                handler: RefCell::new(handler),
                state: RefCell::new(State {
                    zoom: 1.0,
                    ..State::default()
                }),
            }
        }

        pub fn set_handler(&self, handler: EventHandler) {
            *self.handler.borrow_mut() = handler;
        }

        fn emit(&self, event: WebViewEvent) {
            let handler = self.handler.borrow().clone();
            handler(event);
        }

        fn arrive(&self) {
            let (url, can_go_back, can_go_forward) = {
                let state = self.state.borrow();
                (
                    state.entries.get(state.index).cloned().unwrap_or_default(),
                    state.index > 0,
                    state.index + 1 < state.entries.len(),
                )
            };
            self.emit(WebViewEvent::Loading(true));
            self.emit(WebViewEvent::Url(url.clone()));
            self.emit(WebViewEvent::History {
                can_go_back,
                can_go_forward,
            });
            self.emit(WebViewEvent::Title(url.clone()));
            self.emit(WebViewEvent::Progress(1.0));
            self.emit(WebViewEvent::Loading(false));
            self.emit(WebViewEvent::Finished {
                title: url.clone(),
                url,
            });
        }

        pub fn load_url(&self, url: &str) {
            {
                let mut state = self.state.borrow_mut();
                let keep = if state.entries.is_empty() {
                    0
                } else {
                    state.index + 1
                };
                state.entries.truncate(keep);
                state.entries.push(url.to_owned());
                state.index = state.entries.len() - 1;
            }
            self.arrive();
        }

        pub fn go_back(&self) {
            let moved = {
                let mut state = self.state.borrow_mut();
                let moved = state.index > 0;
                if moved {
                    state.index -= 1;
                }
                moved
            };
            if moved {
                self.arrive();
            }
        }

        pub fn go_forward(&self) {
            let moved = {
                let mut state = self.state.borrow_mut();
                let moved = state.index + 1 < state.entries.len();
                if moved {
                    state.index += 1;
                }
                moved
            };
            if moved {
                self.arrive();
            }
        }

        pub fn reload(&self) {
            if !self.state.borrow().entries.is_empty() {
                self.arrive();
            }
        }

        pub fn set_frame(&self, _: WebViewFrame) {}

        pub fn set_holes(&self, _: &[WebViewHole]) {}

        pub fn set_visible(&self, visible: bool) {
            self.state.borrow_mut().visible = visible;
        }

        pub fn focus(&self) {}

        pub fn blur(&self) {}

        pub fn set_zoom(&self, zoom: f64) {
            self.state.borrow_mut().zoom = zoom;
        }

        pub fn set_dark_appearance(&self, _: bool) {}

        pub fn url(&self) -> String {
            let state = self.state.borrow();
            state.entries.get(state.index).cloned().unwrap_or_default()
        }

        pub fn title(&self) -> String {
            self.url()
        }

        pub fn find(&self, query: &str, _: bool, done: Box<dyn FnOnce(FindResult)>) {
            let matches = usize::from(!query.is_empty() && self.url().contains(query));
            done(FindResult {
                matches,
                active: matches,
            });
        }

        pub fn clear_find(&self) {}

        pub fn copy_snapshot(&self, done: Box<dyn FnOnce(bool)>) {
            done(false);
        }

        pub fn print(&self) {}

        #[cfg(feature = "screenshot")]
        pub fn snapshot_png(&self, done: Box<dyn FnOnce(Option<Vec<u8>>)>) {
            done(None);
        }
    }
}
