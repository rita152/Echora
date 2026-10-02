//! `WKWebView` behind [`super::WebView`].
//!
//! Each page is an `EchoraWebView` (a `WKWebView` subclass) inside a plain
//! clipping view that rounds the card's bottom corners. Both live in the
//! window's content view above GPUI's view, so AppKit routes clicks, scrolling
//! and (once the page is first responder) keys straight to WebKit. One shared
//! delegate object serves every page; each callback finds its page's state
//! through the web view's `echoraState` ivar.

// objc 0.2 macros probe the legacy cargo-clippy cfg.
#![allow(unexpected_cfgs)]

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::{CStr, c_void},
    path::PathBuf,
    rc::{Rc, Weak},
    sync::OnceLock,
};

use block::{Block, ConcreteBlock};
use cocoa::{
    base::{BOOL, NO, YES, id, nil},
    foundation::{NSPoint, NSRect, NSSize},
};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Protocol, Sel},
    sel, sel_impl,
};

use super::{
    ClearData, ContextCommand, DownloadEvent, EventHandler, FindResult, MenuLabels,
    NavigationError, WebView, WebViewEvent, WebViewFrame, WebViewHole,
};
use crate::browser::scripts;

#[link(name = "WebKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPathCreateMutable() -> *mut c_void;
    fn CGPathAddRect(path: *mut c_void, transform: *const c_void, rect: NSRect);
    fn CGPathAddRoundedRect(
        path: *mut c_void,
        transform: *const c_void,
        rect: NSRect,
        corner_width: f64,
        corner_height: f64,
    );
    fn CGPathRelease(path: *mut c_void);
}

const STATE_IVAR: &str = "echoraState";
/// The clip view's pointer to its web view, for hit testing the holes.
const PAGE_IVAR: &str = "echoraPage";
const MESSAGE_HANDLER: &str = "echora";
const OBSERVED_KEYS: [&str; 6] = [
    "title",
    "URL",
    "estimatedProgress",
    "loading",
    "canGoBack",
    "canGoForward",
];
const UTF8_ENCODING: usize = 4;
const NS_WINDOW_ABOVE: isize = 1;
const COMMAND_FLAG: usize = 1 << 20;
const SHIFT_FLAG: usize = 1 << 17;
const OPTION_FLAG: usize = 1 << 19;
const CONTROL_FLAG: usize = 1 << 18;
const POLICY_CANCEL: isize = 0;
const POLICY_ALLOW: isize = 1;
const POLICY_DOWNLOAD: isize = 2;
/// `kCALayerMinXMinYCorner | kCALayerMaxXMinYCorner`: the bottom corners of a
/// layer in a non-flipped view.
const BOTTOM_CORNERS: usize = 1 | 2;
const PNG_FILE_TYPE: usize = 4;
const FAVICON_PIXELS: isize = 32;
/// Menu item tags for the commands the app adds to the page's context menu.
const TAG_BACK: isize = 0x0EC0_0001;
const TAG_FORWARD: isize = 0x0EC0_0002;
const TAG_RELOAD: isize = 0x0EC0_0003;
const TAG_OPEN_EXTERNAL: isize = 0x0EC0_0004;

/// Per-page state shared by the Rust handle and the native callbacks.
struct PageState {
    handler: RefCell<Option<EventHandler>>,
    /// Events from a `window.open` page until a tab adopts it.
    pending: RefCell<Vec<WebViewEvent>>,
    labels: MenuLabels,
    /// The link under the last right click, reported by the page script.
    context_link: RefCell<Option<String>>,
    favicon_href: RefCell<Option<String>>,
    favicon_serial: Cell<u64>,
    /// GPUI's view in the same window; pages opened from this one go above it.
    host_view: usize,
    /// Overlay rectangles in the clip view's coordinates, left to GPUI.
    holes: RefCell<Vec<NSRect>>,
    /// Width of the left strip whose clicks go to GPUI.
    pointer_inset_left: Cell<f64>,
}

impl PageState {
    fn emit(&self, event: WebViewEvent) {
        let handler = self.handler.borrow().clone();
        match handler {
            Some(handler) => handler(event),
            None => self.pending.borrow_mut().push(event),
        }
    }
}

pub struct NativeWebView {
    web_view: id,
    clip_view: id,
    host_view: id,
    state: Rc<PageState>,
}

impl NativeWebView {
    pub fn new(host_view: id, labels: MenuLabels, handler: EventHandler) -> Self {
        unsafe {
            let configuration: id = msg_send![class!(WKWebViewConfiguration), new];
            configure(configuration);
            let view = Self::with_configuration(host_view, configuration, labels, Some(handler));
            let _: () = msg_send![configuration, release];
            view
        }
    }

    /// Builds a page for `configuration`, which either comes from
    /// [`configure`] or is the copy WebKit hands over for `window.open`.
    unsafe fn with_configuration(
        host_view: id,
        configuration: id,
        labels: MenuLabels,
        handler: Option<EventHandler>,
    ) -> Self {
        unsafe {
            let zero = NSRect::new(NSPoint::new(0., 0.), NSSize::new(0., 0.));
            let clip_view: id = msg_send![clip_view_class(), alloc];
            let clip_view: id = msg_send![clip_view, initWithFrame: zero];
            let _: () = msg_send![clip_view, setWantsLayer: YES];
            let layer: id = msg_send![clip_view, layer];
            let _: () = msg_send![layer, setMasksToBounds: YES];
            let _: () = msg_send![clip_view, setHidden: YES];

            let web_view: id = msg_send![web_view_class(), alloc];
            let web_view: id =
                msg_send![web_view, initWithFrame: zero configuration: configuration];
            // Width and height sizable: the page always fills the clip view.
            let _: () = msg_send![web_view, setAutoresizingMask: 2usize | 16usize];
            let _: () = msg_send![web_view, setAllowsBackForwardNavigationGestures: YES];
            let _: () = msg_send![web_view, setAllowsMagnification: YES];
            let _: () = msg_send![web_view, setAllowsLinkPreview: YES];
            if responds(web_view, sel!(setInspectable:)) {
                let _: () = msg_send![web_view, setInspectable: YES];
            }
            // Capture windows sit behind other windows; an occluded page would
            // be throttled and miss the capture.
            #[cfg(feature = "screenshot")]
            if responds(web_view, sel!(_setWindowOcclusionDetectionEnabled:)) {
                let _: () = msg_send![web_view, _setWindowOcclusionDetectionEnabled: NO];
            }
            let delegate = delegate();
            let _: () = msg_send![web_view, setNavigationDelegate: delegate];
            let _: () = msg_send![web_view, setUIDelegate: delegate];
            for key in OBSERVED_KEYS {
                let _: () = msg_send![
                    web_view,
                    addObserver: delegate
                    forKeyPath: ns_string(key)
                    options: 1usize
                    context: std::ptr::null_mut::<c_void>()
                ];
            }

            let state = Rc::new(PageState {
                handler: RefCell::new(handler),
                pending: RefCell::new(Vec::new()),
                labels,
                context_link: RefCell::new(None),
                favicon_href: RefCell::new(None),
                favicon_serial: Cell::new(0),
                host_view: host_view as usize,
                holes: RefCell::new(Vec::new()),
                pointer_inset_left: Cell::new(0.),
            });
            let raw = Rc::into_raw(state.clone()) as *mut c_void;
            (*web_view).set_ivar::<*mut c_void>(STATE_IVAR, raw);

            (*clip_view).set_ivar::<*mut c_void>(PAGE_IVAR, web_view as *mut c_void);
            let _: () = msg_send![clip_view, addSubview: web_view];
            let superview: id = msg_send![host_view, superview];
            if superview != nil {
                let _: () = msg_send![
                    superview,
                    addSubview: clip_view
                    positioned: NS_WINDOW_ABOVE
                    relativeTo: host_view
                ];
            }
            Self {
                web_view,
                clip_view,
                host_view,
                state,
            }
        }
    }

    pub fn set_handler(&self, handler: EventHandler) {
        *self.state.handler.borrow_mut() = Some(handler.clone());
        let pending = std::mem::take(&mut *self.state.pending.borrow_mut());
        for event in pending {
            handler(event);
        }
    }

    pub fn load_url(&self, url: &str) {
        unsafe {
            let ns_url: id = msg_send![class!(NSURL), URLWithString: ns_string(url)];
            if ns_url == nil {
                return;
            }
            let is_file: BOOL = msg_send![ns_url, isFileURL];
            if is_file == YES {
                let root: id = msg_send![class!(NSURL), fileURLWithPath: ns_string("/")];
                let _: id =
                    msg_send![self.web_view, loadFileURL: ns_url allowingReadAccessToURL: root];
            } else {
                let request: id = msg_send![class!(NSURLRequest), requestWithURL: ns_url];
                let _: id = msg_send![self.web_view, loadRequest: request];
            }
        }
    }

    pub fn go_back(&self) {
        unsafe {
            let _: id = msg_send![self.web_view, goBack];
        }
    }

    pub fn go_forward(&self) {
        unsafe {
            let _: id = msg_send![self.web_view, goForward];
        }
    }

    pub fn reload(&self) {
        unsafe {
            let url: id = msg_send![self.web_view, URL];
            if url == nil {
                return;
            }
            let _: id = msg_send![self.web_view, reload];
        }
    }

    pub fn set_frame(&self, frame: WebViewFrame) {
        unsafe {
            let superview: id = msg_send![self.clip_view, superview];
            if superview == nil {
                return;
            }
            let bounds: NSRect = msg_send![superview, bounds];
            let flipped: BOOL = msg_send![superview, isFlipped];
            let y = if flipped == YES {
                frame.y as f64
            } else {
                bounds.size.height - (frame.y + frame.height) as f64
            };
            let rect = NSRect::new(
                NSPoint::new(frame.x as f64, y),
                NSSize::new(frame.width.max(0.) as f64, frame.height.max(0.) as f64),
            );
            let current: NSRect = msg_send![self.clip_view, frame];
            if current.origin.x != rect.origin.x
                || current.origin.y != rect.origin.y
                || current.size.width != rect.size.width
                || current.size.height != rect.size.height
            {
                let _: () = msg_send![self.clip_view, setFrame: rect];
                let inner = NSRect::new(NSPoint::new(0., 0.), rect.size);
                let _: () = msg_send![self.web_view, setFrame: inner];
            }
            self.state
                .pointer_inset_left
                .set(frame.pointer_inset_left.max(0.) as f64);
            let layer: id = msg_send![self.clip_view, layer];
            if layer != nil {
                let _: () = msg_send![layer, setCornerRadius: frame.bottom_corner_radius as f64];
                let _: () = msg_send![layer, setMaskedCorners: BOTTOM_CORNERS];
            }
        }
    }

    pub fn set_visible(&self, visible: bool) {
        unsafe {
            let hidden: BOOL = msg_send![self.clip_view, isHidden];
            let want = if visible { NO } else { YES };
            if hidden != want {
                if !visible && self.has_focus() {
                    self.blur();
                }
                let _: () = msg_send![self.clip_view, setHidden: want];
            }
        }
    }

    pub fn set_holes(&self, holes: &[WebViewHole]) {
        unsafe {
            let frame: NSRect = msg_send![self.clip_view, frame];
            let superview: id = msg_send![self.clip_view, superview];
            if superview == nil {
                return;
            }
            let bounds: NSRect = msg_send![superview, bounds];
            let flipped: BOOL = msg_send![superview, isFlipped];
            // The clip view is not flipped: its origin is the bottom left.
            let top = if flipped == YES {
                frame.origin.y
            } else {
                bounds.size.height - frame.origin.y - frame.size.height
            };
            let local: Vec<(NSRect, f64)> = holes
                .iter()
                .filter_map(|hole| {
                    let x = hole.x as f64 - frame.origin.x;
                    let y_from_top = hole.y as f64 - top;
                    let rect = NSRect::new(
                        NSPoint::new(x, frame.size.height - y_from_top - hole.height as f64),
                        NSSize::new(hole.width as f64, hole.height as f64),
                    );
                    let visible = rect.origin.x < frame.size.width
                        && rect.origin.y < frame.size.height
                        && rect.origin.x + rect.size.width > 0.
                        && rect.origin.y + rect.size.height > 0.;
                    visible.then_some((rect, hole.corner_radius as f64))
                })
                .collect();
            let rects: Vec<NSRect> = local.iter().map(|(rect, _)| *rect).collect();
            let same = {
                let current = self.state.holes.borrow();
                current.len() == rects.len()
                    && current.iter().zip(&rects).all(|(a, b)| {
                        a.origin.x == b.origin.x
                            && a.origin.y == b.origin.y
                            && a.size.width == b.size.width
                            && a.size.height == b.size.height
                    })
            };
            if same {
                return;
            }
            *self.state.holes.borrow_mut() = rects;
            let layer: id = msg_send![self.clip_view, layer];
            if layer == nil {
                return;
            }
            if local.is_empty() {
                let _: () = msg_send![layer, setMask: nil];
                return;
            }
            let mask: id = msg_send![class!(CAShapeLayer), layer];
            let full = NSRect::new(NSPoint::new(0., 0.), frame.size);
            let _: () = msg_send![mask, setFrame: full];
            let path = CGPathCreateMutable();
            CGPathAddRect(path, std::ptr::null(), full);
            for (rect, radius) in &local {
                let radius = radius
                    .min(rect.size.width / 2.)
                    .min(rect.size.height / 2.)
                    .max(0.);
                CGPathAddRoundedRect(path, std::ptr::null(), *rect, radius, radius);
            }
            let _: () = msg_send![mask, setPath: path];
            CGPathRelease(path);
            let _: () = msg_send![mask, setFillRule: ns_string("even-odd")];
            let _: () = msg_send![layer, setMask: mask];
        }
    }

    fn has_focus(&self) -> bool {
        unsafe { view_has_focus(self.web_view) }
    }

    pub fn focus(&self) {
        unsafe {
            let window: id = msg_send![self.web_view, window];
            if window != nil {
                let _: BOOL = msg_send![window, makeFirstResponder: self.web_view];
            }
        }
    }

    pub fn blur(&self) {
        unsafe {
            if !self.has_focus() {
                return;
            }
            let window: id = msg_send![self.web_view, window];
            if window != nil {
                let _: BOOL = msg_send![window, makeFirstResponder: self.host_view];
            }
        }
    }

    pub fn set_zoom(&self, zoom: f64) {
        unsafe {
            if responds(self.web_view, sel!(setPageZoom:)) {
                let _: () = msg_send![self.web_view, setPageZoom: zoom];
            }
        }
    }

    pub fn set_dark_appearance(&self, dark: bool) {
        unsafe {
            let name = if dark {
                "NSAppearanceNameDarkAqua"
            } else {
                "NSAppearanceNameAqua"
            };
            let appearance: id = msg_send![class!(NSAppearance), appearanceNamed: ns_string(name)];
            if appearance != nil {
                let _: () = msg_send![self.clip_view, setAppearance: appearance];
            }
        }
    }

    pub fn url(&self) -> String {
        unsafe { web_view_url(self.web_view) }
    }

    pub fn title(&self) -> String {
        unsafe {
            let title: id = msg_send![self.web_view, title];
            rust_string(title)
        }
    }

    pub fn find(&self, query: &str, backwards: bool, done: Box<dyn FnOnce(FindResult)>) {
        let script = if query.is_empty() {
            scripts::CLEAR_FIND_CALL.to_owned()
        } else {
            scripts::find_call(query, backwards)
        };
        unsafe {
            evaluate(self.web_view, &script, move |result| {
                done(scripts::parse_find_result(&result.unwrap_or_default()));
            });
        }
    }

    pub fn clear_find(&self) {
        unsafe {
            evaluate(self.web_view, scripts::CLEAR_FIND_CALL, |_| {});
        }
    }

    pub fn copy_snapshot(&self, done: Box<dyn FnOnce(bool)>) {
        unsafe {
            take_snapshot(self.web_view, move |image| {
                let copied = image.is_some_and(|image| {
                    let pasteboard: id = msg_send![class!(NSPasteboard), generalPasteboard];
                    let _: isize = msg_send![pasteboard, clearContents];
                    let objects: id = msg_send![class!(NSArray), arrayWithObject: image];
                    let written: BOOL = msg_send![pasteboard, writeObjects: objects];
                    written == YES
                });
                done(copied);
            });
        }
    }

    #[cfg(feature = "screenshot")]
    pub fn snapshot_png(&self, done: Box<dyn FnOnce(Option<Vec<u8>>)>) {
        unsafe {
            take_snapshot(self.web_view, move |image| {
                let png = image.and_then(|image| {
                    let tiff: id = msg_send![image, TIFFRepresentation];
                    if tiff == nil {
                        return None;
                    }
                    let rep: id = msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff];
                    if rep == nil { None } else { png_data(rep) }
                });
                done(png);
            });
        }
    }

    pub fn print(&self) {
        unsafe {
            let info: id = msg_send![class!(NSPrintInfo), sharedPrintInfo];
            let operation: id = msg_send![self.web_view, printOperationWithPrintInfo: info];
            if operation == nil {
                return;
            }
            // WebKit's print view has no size of its own.
            let view: id = msg_send![operation, view];
            let bounds: NSRect = msg_send![self.web_view, bounds];
            let _: () = msg_send![view, setFrame: bounds];
            let window: id = msg_send![self.web_view, window];
            let _: () = msg_send![
                operation,
                runOperationModalForWindow: window
                delegate: nil
                didRunSelector: std::ptr::null::<c_void>()
                contextInfo: std::ptr::null_mut::<c_void>()
            ];
        }
    }
}

impl Drop for NativeWebView {
    fn drop(&mut self) {
        unsafe {
            if self.has_focus() {
                self.blur();
            }
            let delegate = delegate();
            for key in OBSERVED_KEYS {
                let _: () =
                    msg_send![self.web_view, removeObserver: delegate forKeyPath: ns_string(key)];
            }
            let _: () = msg_send![self.web_view, stopLoading];
            let _: () = msg_send![self.web_view, setNavigationDelegate: nil];
            let _: () = msg_send![self.web_view, setUIDelegate: nil];
            let raw: *mut c_void = *(*self.web_view).get_ivar::<*mut c_void>(STATE_IVAR);
            (*self.web_view).set_ivar::<*mut c_void>(STATE_IVAR, std::ptr::null_mut());
            if !raw.is_null() {
                drop(Rc::from_raw(raw as *const PageState));
            }
            let _: () = msg_send![self.clip_view, removeFromSuperview];
            let _: () = msg_send![self.web_view, release];
            let _: () = msg_send![self.clip_view, release];
        }
    }
}

/// The browser's configuration: the shared website data store, the page
/// scripts, Safari's user agent suffix and Web Inspector.
unsafe fn configure(configuration: id) {
    unsafe {
        let store: id = msg_send![class!(WKWebsiteDataStore), defaultDataStore];
        let _: () = msg_send![configuration, setWebsiteDataStore: store];
        let _: () = msg_send![
            configuration,
            setApplicationNameForUserAgent: ns_string(&safari_user_agent_suffix())
        ];
        let preferences: id = msg_send![configuration, preferences];
        if responds(preferences, sel!(_setDeveloperExtrasEnabled:)) {
            let _: () = msg_send![preferences, _setDeveloperExtrasEnabled: YES];
        }
        if responds(preferences, sel!(setElementFullscreenEnabled:)) {
            let _: () = msg_send![preferences, setElementFullscreenEnabled: YES];
        }
        let controller: id = msg_send![configuration, userContentController];
        let _: () = msg_send![controller, addScriptMessageHandler: delegate() name: ns_string(MESSAGE_HANDLER)];
        for (source, injection_time, main_frame_only) in [
            (scripts::CONTEXT_SCRIPT, 0isize, NO),
            (scripts::PAGE_SCRIPT, 1isize, YES),
        ] {
            let script: id = msg_send![class!(WKUserScript), alloc];
            let script: id = msg_send![
                script,
                initWithSource: ns_string(source)
                injectionTime: injection_time
                forMainFrameOnly: main_frame_only
            ];
            let _: () = msg_send![controller, addUserScript: script];
            let _: () = msg_send![script, release];
        }
    }
}

/// `Version/<Safari version> Safari/605.1.15`, so sites serve their Safari
/// pages instead of a reduced WebView variant.
fn safari_user_agent_suffix() -> String {
    static SUFFIX: OnceLock<String> = OnceLock::new();
    SUFFIX
        .get_or_init(|| {
            let version = unsafe {
                let path = ns_string("/Applications/Safari.app/Contents/Info.plist");
                let info: id = msg_send![class!(NSDictionary), dictionaryWithContentsOfFile: path];
                if info == nil {
                    String::new()
                } else {
                    let value: id =
                        msg_send![info, objectForKey: ns_string("CFBundleShortVersionString")];
                    rust_string(value)
                }
            };
            let version = if version.is_empty() {
                "18.0".to_owned()
            } else {
                version
            };
            format!("Version/{version} Safari/605.1.15")
        })
        .clone()
}

/// The plain view that rounds the page's bottom corners and lets the pointer
/// through where GPUI overlays sit.
fn clip_view_class() -> &'static Class {
    static CLASS: OnceLock<usize> = OnceLock::new();
    let class = *CLASS.get_or_init(|| unsafe {
        let mut decl = ClassDecl::new("EchoraWebViewClip", class!(NSView))
            .expect("EchoraWebViewClip is registered once");
        decl.add_ivar::<*mut c_void>(PAGE_IVAR);
        decl.add_method(
            sel!(hitTest:),
            clip_hit_test as extern "C" fn(&Object, Sel, NSPoint) -> id,
        );
        decl.register() as *const Class as usize
    });
    unsafe { &*(class as *const Class) }
}

extern "C" fn clip_hit_test(this: &Object, _: Sel, point: NSPoint) -> id {
    unsafe {
        let this = this as *const Object as id;
        let superview: id = msg_send![this, superview];
        let local: NSPoint = msg_send![this, convertPoint: point fromView: superview];
        let page: *mut c_void = *(*this).get_ivar::<*mut c_void>(PAGE_IVAR);
        if let Some(state) = page_state(page as id) {
            if local.x < state.pointer_inset_left.get() {
                return nil;
            }
            let inside = state.holes.borrow().iter().any(|rect| {
                local.x >= rect.origin.x
                    && local.x < rect.origin.x + rect.size.width
                    && local.y >= rect.origin.y
                    && local.y < rect.origin.y + rect.size.height
            });
            if inside {
                return nil;
            }
        }
        msg_send![super(this, class!(NSView)), hitTest: point]
    }
}

fn web_view_class() -> &'static Class {
    static CLASS: OnceLock<usize> = OnceLock::new();
    let class = *CLASS.get_or_init(|| unsafe {
        let mut decl = ClassDecl::new("EchoraWebView", class!(WKWebView))
            .expect("EchoraWebView is registered once");
        decl.add_ivar::<*mut c_void>(STATE_IVAR);
        decl.add_method(
            sel!(performKeyEquivalent:),
            perform_key_equivalent as extern "C" fn(&Object, Sel, id) -> BOOL,
        );
        decl.add_method(
            sel!(willOpenMenu:withEvent:),
            will_open_menu as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(becomeFirstResponder),
            become_first_responder as extern "C" fn(&Object, Sel) -> BOOL,
        );
        decl.add_method(
            sel!(resignFirstResponder),
            resign_first_responder as extern "C" fn(&Object, Sel) -> BOOL,
        );
        decl.register() as *const Class as usize
    });
    unsafe { &*(class as *const Class) }
}

fn delegate() -> id {
    static DELEGATE: OnceLock<usize> = OnceLock::new();
    *DELEGATE.get_or_init(|| unsafe {
        let mut decl = ClassDecl::new("EchoraWebViewDelegate", class!(NSObject))
            .expect("EchoraWebViewDelegate is registered once");
        for protocol in [
            "WKNavigationDelegate",
            "WKUIDelegate",
            "WKScriptMessageHandler",
            "WKDownloadDelegate",
        ] {
            if let Some(protocol) = Protocol::get(protocol) {
                decl.add_protocol(protocol);
            }
        }
        decl.add_method(
            sel!(webView:didStartProvisionalNavigation:),
            did_start_navigation as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(webView:didCommitNavigation:),
            did_commit_navigation as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(webView:didFinishNavigation:),
            did_finish_navigation as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(webView:didFailProvisionalNavigation:withError:),
            did_fail_navigation as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(webView:didFailNavigation:withError:),
            did_fail_navigation as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(webView:decidePolicyForNavigationAction:decisionHandler:),
            decide_navigation_action as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(webView:decidePolicyForNavigationResponse:decisionHandler:),
            decide_navigation_response as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(webView:navigationAction:didBecomeDownload:),
            did_become_download as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(webView:navigationResponse:didBecomeDownload:),
            did_become_download as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(webViewWebContentProcessDidTerminate:),
            process_did_terminate as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:),
            create_web_view as extern "C" fn(&Object, Sel, id, id, id, id) -> id,
        );
        decl.add_method(
            sel!(webViewDidClose:),
            web_view_did_close as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(webView:runJavaScriptAlertPanelWithMessage:initiatedByFrame:completionHandler:),
            run_alert as extern "C" fn(&Object, Sel, id, id, id, id),
        );
        decl.add_method(
            sel!(webView:runJavaScriptConfirmPanelWithMessage:initiatedByFrame:completionHandler:),
            run_confirm as extern "C" fn(&Object, Sel, id, id, id, id),
        );
        decl.add_method(
            sel!(webView:runJavaScriptTextInputPanelWithPrompt:defaultText:initiatedByFrame:completionHandler:),
            run_prompt as extern "C" fn(&Object, Sel, id, id, id, id, id),
        );
        decl.add_method(
            sel!(webView:runOpenPanelWithParameters:initiatedByFrame:completionHandler:),
            run_open_panel as extern "C" fn(&Object, Sel, id, id, id, id),
        );
        decl.add_method(
            sel!(userContentController:didReceiveScriptMessage:),
            did_receive_script_message as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(observeValueForKeyPath:ofObject:change:context:),
            observe_value as extern "C" fn(&Object, Sel, id, id, id, *mut c_void),
        );
        decl.add_method(
            sel!(download:decideDestinationUsingResponse:suggestedFilename:completionHandler:),
            download_destination as extern "C" fn(&Object, Sel, id, id, id, id),
        );
        decl.add_method(
            sel!(downloadDidFinish:),
            download_did_finish as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(download:didFailWithError:resumeData:),
            download_did_fail as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(echoraMenuAction:),
            menu_action as extern "C" fn(&Object, Sel, id),
        );
        let class = decl.register();
        let delegate: id = msg_send![class, new];
        delegate as usize
    }) as id
}

/// The page state of a web view, if it still belongs to a live tab.
unsafe fn page_state(web_view: id) -> Option<Rc<PageState>> {
    unsafe {
        if web_view == nil {
            return None;
        }
        let is_page: BOOL = msg_send![web_view, isKindOfClass: web_view_class()];
        if is_page == NO {
            return None;
        }
        let raw: *mut c_void = *(*web_view).get_ivar::<*mut c_void>(STATE_IVAR);
        if raw.is_null() {
            return None;
        }
        let raw = raw as *const PageState;
        Rc::increment_strong_count(raw);
        Some(Rc::from_raw(raw))
    }
}

unsafe fn emit(web_view: id, event: WebViewEvent) {
    unsafe {
        if let Some(state) = page_state(web_view) {
            state.emit(event);
        }
    }
}

unsafe fn view_has_focus(view: id) -> bool {
    unsafe {
        let window: id = msg_send![view, window];
        if window == nil {
            return false;
        }
        let responder: id = msg_send![window, firstResponder];
        if responder == nil {
            return false;
        }
        let is_view: BOOL = msg_send![responder, isKindOfClass: class!(NSView)];
        if is_view == NO {
            return false;
        }
        let inside: BOOL = msg_send![responder, isDescendantOf: view];
        inside == YES
    }
}

extern "C" fn perform_key_equivalent(this: &Object, _: Sel, event: id) -> BOOL {
    unsafe {
        let this = this as *const Object as id;
        if view_has_focus(this) {
            let flags: usize = msg_send![event, modifierFlags];
            let modifiers = flags & (COMMAND_FLAG | SHIFT_FLAG | OPTION_FLAG | CONTROL_FLAG);
            let characters: id = msg_send![event, charactersIgnoringModifiers];
            let key = rust_string(characters).to_lowercase();
            // The app has no Edit menu for these, so the page needs them here.
            let action = match (modifiers, key.as_str()) {
                (COMMAND_FLAG, "c") => Some(sel!(copy:)),
                (COMMAND_FLAG, "x") => Some(sel!(cut:)),
                (COMMAND_FLAG, "v") => Some(sel!(paste:)),
                (COMMAND_FLAG, "a") => Some(sel!(selectAll:)),
                _ => None,
            };
            if let Some(action) = action {
                let done: BOOL = msg_send![this, tryToPerform: action with: nil];
                if done == YES {
                    return YES;
                }
            }
            if key == "z" && (modifiers == COMMAND_FLAG || modifiers == COMMAND_FLAG | SHIFT_FLAG) {
                let manager: id = msg_send![this, undoManager];
                if manager != nil {
                    if modifiers == COMMAND_FLAG {
                        let _: () = msg_send![manager, undo];
                    } else {
                        let _: () = msg_send![manager, redo];
                    }
                    return YES;
                }
            }
        }
        msg_send![super(this, class!(WKWebView)), performKeyEquivalent: event]
    }
}

extern "C" fn become_first_responder(this: &Object, _: Sel) -> BOOL {
    unsafe {
        let this = this as *const Object as id;
        let accepted: BOOL = msg_send![super(this, class!(WKWebView)), becomeFirstResponder];
        if accepted == YES {
            emit(this, WebViewEvent::Focused(true));
        }
        accepted
    }
}

extern "C" fn resign_first_responder(this: &Object, _: Sel) -> BOOL {
    unsafe {
        let this = this as *const Object as id;
        let resigned: BOOL = msg_send![super(this, class!(WKWebView)), resignFirstResponder];
        if resigned == YES {
            emit(this, WebViewEvent::Focused(false));
        }
        resigned
    }
}

/// Rebuilds WebKit's context menu in the reference's order: link actions,
/// then copy/edit actions, then Back/Forward/Reload on plain page areas, then
/// Inspect.
extern "C" fn will_open_menu(this: &Object, _: Sel, menu: id, event: id) {
    unsafe {
        let this = this as *const Object as id;
        let _: () = msg_send![super(this, class!(WKWebView)), willOpenMenu: menu withEvent: event];
        let Some(state) = page_state(this) else {
            return;
        };
        let labels = &state.labels;
        let items: id = msg_send![menu, itemArray];
        let count: usize = msg_send![items, count];
        let mut by_identifier: HashMap<String, id> = HashMap::new();
        let mut edit_items: Vec<id> = Vec::new();
        let (mut editable, mut selection) = (false, false);
        for index in 0..count {
            let item: id = msg_send![items, objectAtIndex: index];
            let identifier: id = msg_send![item, identifier];
            let identifier = rust_string(identifier);
            let action: Sel = msg_send![item, action];
            if action == sel!(paste:) {
                editable = true;
            }
            if action == sel!(cut:) || action == sel!(copy:) || action == sel!(paste:) {
                edit_items.push(item);
            }
            if identifier == "WKMenuItemIdentifierCopy" {
                selection = true;
            }
            if !identifier.is_empty() {
                by_identifier.insert(identifier, item);
            }
        }
        let item_for = |identifier: &str| by_identifier.get(identifier).copied();
        let open_link = item_for("WKMenuItemIdentifierOpenLinkInNewWindow");
        let copy_link = item_for("WKMenuItemIdentifierCopyLink");
        let copy_selection = item_for("WKMenuItemIdentifierCopy");
        let inspect = item_for("WKMenuItemIdentifierInspectElement");
        let has_media = item_for("WKMenuItemIdentifierCopyImage").is_some()
            || item_for("WKMenuItemIdentifierCopyMediaLink").is_some();
        let link = state
            .context_link
            .borrow()
            .clone()
            .filter(|link| !link.is_empty());
        let has_link = open_link.is_some() || copy_link.is_some();
        for item in [open_link, copy_link, copy_selection, inspect]
            .into_iter()
            .flatten()
            .chain(edit_items.iter().copied())
        {
            let _: id = msg_send![item, retain];
        }
        let _: () = msg_send![menu, removeAllItems];

        let mut sections: Vec<Vec<id>> = Vec::new();
        if has_link {
            let mut section = Vec::new();
            if let Some(item) = open_link {
                set_title(item, &labels.open_link_in_new_tab);
                section.push(item);
            }
            if let Some(link) = link
                .as_ref()
                .filter(|link| crate::browser::address::is_web_url(link))
            {
                let item = custom_item(&labels.open_in_external_browser, TAG_OPEN_EXTERNAL, this);
                let _: () = msg_send![item, setRepresentedObject: ns_string(link)];
                section.push(item);
            }
            sections.push(section);
        }
        if has_link || editable || selection {
            let mut section = Vec::new();
            if let Some(item) = copy_link {
                set_title(item, &labels.copy_link_address);
                section.push(item);
            }
            if editable {
                section.extend(edit_items.iter().copied());
            } else if let Some(item) = copy_selection {
                section.push(item);
            }
            sections.push(section);
        }
        if !has_link && !editable && !selection && !has_media {
            let can_go_back: BOOL = msg_send![this, canGoBack];
            let can_go_forward: BOOL = msg_send![this, canGoForward];
            let back = custom_item(&labels.back, TAG_BACK, this);
            let _: () = msg_send![back, setEnabled: can_go_back];
            let forward = custom_item(&labels.forward, TAG_FORWARD, this);
            let _: () = msg_send![forward, setEnabled: can_go_forward];
            let reload = custom_item(&labels.reload, TAG_RELOAD, this);
            sections.push(vec![back, forward, reload]);
        }
        if let Some(item) = inspect {
            set_title(item, &labels.inspect);
            sections.push(vec![item]);
        }
        let mut first = true;
        for section in sections.into_iter().filter(|section| !section.is_empty()) {
            if !first {
                let separator: id = msg_send![class!(NSMenuItem), separatorItem];
                let _: () = msg_send![menu, addItem: separator];
            }
            first = false;
            for item in section {
                let _: () = msg_send![menu, addItem: item];
            }
        }
        for item in [open_link, copy_link, copy_selection, inspect]
            .into_iter()
            .flatten()
            .chain(edit_items.iter().copied())
        {
            let _: () = msg_send![item, release];
        }
        let _: () = msg_send![menu, setAutoenablesItems: NO];
    }
}

unsafe fn set_title(item: id, title: &str) {
    unsafe {
        let _: () = msg_send![item, setTitle: ns_string(title)];
    }
}

/// A menu item for one of the app's commands; its represented object is the
/// link (when there is one) and the page is found through the tag's owner.
unsafe fn custom_item(title: &str, tag: isize, web_view: id) -> id {
    unsafe {
        let item: id = msg_send![class!(NSMenuItem), alloc];
        let item: id = msg_send![
            item,
            initWithTitle: ns_string(title)
            action: sel!(echoraMenuAction:)
            keyEquivalent: ns_string("")
        ];
        let _: () = msg_send![item, setTarget: delegate()];
        let _: () = msg_send![item, setTag: tag];
        let _: () = msg_send![item, setEnabled: YES];
        MENU_PAGE.with(|page| page.set(web_view as usize));
        let _: id = msg_send![item, autorelease];
        item
    }
}

thread_local! {
    /// The page whose context menu is open; menus are modal, so one at a time.
    static MENU_PAGE: Cell<usize> = const { Cell::new(0) };
    /// Live downloads: the `WKDownload` and its progress, to the page and id.
    static DOWNLOADS: RefCell<HashMap<usize, DownloadEntry>> = RefCell::new(HashMap::new());
    static NEXT_DOWNLOAD_ID: Cell<u64> = const { Cell::new(1) };
}

struct DownloadEntry {
    id: u64,
    page: Weak<PageState>,
    progress: id,
}

extern "C" fn menu_action(_: &Object, _: Sel, item: id) {
    unsafe {
        let page = MENU_PAGE.with(|page| page.get()) as id;
        if page == nil {
            return;
        }
        let tag: isize = msg_send![item, tag];
        let command = match tag {
            TAG_BACK => ContextCommand::Back,
            TAG_FORWARD => ContextCommand::Forward,
            TAG_RELOAD => ContextCommand::Reload,
            TAG_OPEN_EXTERNAL => {
                let link: id = msg_send![item, representedObject];
                ContextCommand::OpenExternal(rust_string(link))
            }
            _ => return,
        };
        emit(page, WebViewEvent::Context(command));
    }
}

extern "C" fn did_start_navigation(_: &Object, _: Sel, web_view: id, _: id) {
    unsafe {
        emit(web_view, WebViewEvent::Loading(true));
    }
}

extern "C" fn did_commit_navigation(_: &Object, _: Sel, web_view: id, _: id) {
    unsafe {
        emit(web_view, WebViewEvent::Url(web_view_url(web_view)));
    }
}

extern "C" fn did_finish_navigation(_: &Object, _: Sel, web_view: id, _: id) {
    unsafe {
        let title: id = msg_send![web_view, title];
        emit(
            web_view,
            WebViewEvent::Finished {
                url: web_view_url(web_view),
                title: rust_string(title),
            },
        );
    }
}

extern "C" fn did_fail_navigation(_: &Object, _: Sel, web_view: id, _: id, error: id) {
    unsafe {
        let domain: id = msg_send![error, domain];
        let domain = rust_string(domain);
        let code: isize = msg_send![error, code];
        // Cancelled loads, policy changes (downloads) and plug-in handled loads
        // are not failures the user should see.
        if (domain == "NSURLErrorDomain" && code == -999)
            || (domain == "WebKitErrorDomain" && matches!(code, 102 | 204))
        {
            return;
        }
        let info: id = msg_send![error, userInfo];
        let failing: id = msg_send![info, objectForKey: ns_string("NSErrorFailingURLStringKey")];
        let mut url = rust_string(failing);
        if url.is_empty() {
            let failing: id = msg_send![info, objectForKey: ns_string("NSErrorFailingURLKey")];
            if failing != nil {
                let string: id = msg_send![failing, absoluteString];
                url = rust_string(string);
            }
        }
        let description: id = msg_send![error, localizedDescription];
        emit(
            web_view,
            WebViewEvent::Failed(NavigationError {
                url,
                code: chromium_error_name(&domain, code),
                description: rust_string(description),
            }),
        );
    }
}

/// The Chromium error name the reference's error page keys its wording on.
fn chromium_error_name(domain: &str, code: isize) -> String {
    if domain != "NSURLErrorDomain" {
        return format!("ERR_FAILED ({code})");
    }
    match code {
        -1003 | -1006 => "ERR_NAME_NOT_RESOLVED",
        -1009 => "ERR_INTERNET_DISCONNECTED",
        -1004 => "ERR_CONNECTION_REFUSED",
        -1001 => "ERR_TIMED_OUT",
        -1005 => "ERR_CONNECTION_CLOSED",
        -1200 => "ERR_SSL_PROTOCOL_ERROR",
        -1201 | -1204 => "ERR_CERT_DATE_INVALID",
        -1202 | -1203 => "ERR_CERT_AUTHORITY_INVALID",
        -1205 | -1206 => "ERR_BAD_SSL_CLIENT_AUTH_CERT",
        -1100 => "ERR_FILE_NOT_FOUND",
        -1102 => "ERR_ACCESS_DENIED",
        -1002 => "ERR_UNKNOWN_URL_SCHEME",
        -1000 => "ERR_INVALID_URL",
        -1022 => "ERR_BLOCKED_BY_CLIENT",
        _ => return format!("ERR_FAILED ({code})"),
    }
    .to_owned()
}

extern "C" fn decide_navigation_action(_: &Object, _: Sel, web_view: id, action: id, handler: id) {
    unsafe {
        let decide = |policy: isize| {
            let block = &*(handler as *const Block<(isize,), ()>);
            block.call((policy,));
        };
        let request: id = msg_send![action, request];
        let url: id = msg_send![request, URL];
        let url_string = if url == nil {
            String::new()
        } else {
            let string: id = msg_send![url, absoluteString];
            rust_string(string)
        };
        let scheme = url_string
            .split_once(':')
            .map(|(scheme, _)| scheme.to_ascii_lowercase())
            .unwrap_or_default();
        if !scheme.is_empty()
            && !matches!(
                scheme.as_str(),
                "http" | "https" | "file" | "about" | "data" | "blob" | "javascript"
            )
        {
            // mailto:, tel:, app links: hand them to their app.
            let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
            let _: BOOL = msg_send![workspace, openURL: url];
            decide(POLICY_CANCEL);
            return;
        }
        if responds(action, sel!(shouldPerformDownload)) {
            let download: BOOL = msg_send![action, shouldPerformDownload];
            if download == YES {
                decide(POLICY_DOWNLOAD);
                return;
            }
        }
        let target: id = msg_send![action, targetFrame];
        let main_frame = target != nil && {
            let is_main: BOOL = msg_send![target, isMainFrame];
            is_main == YES
        };
        let navigation_type: isize = msg_send![action, navigationType];
        let flags: usize = msg_send![action, modifierFlags];
        let button: isize = msg_send![action, buttonNumber];
        // ⌘-click and middle click on a link open a new tab.
        if main_frame
            && navigation_type == 0
            && (flags & COMMAND_FLAG != 0 || button == 2)
            && crate::browser::address::is_web_url(&url_string)
        {
            emit(web_view, WebViewEvent::OpenInNewTab(url_string));
            decide(POLICY_CANCEL);
            return;
        }
        decide(POLICY_ALLOW);
    }
}

extern "C" fn decide_navigation_response(_: &Object, _: Sel, _: id, response: id, handler: id) {
    unsafe {
        let decide = |policy: isize| {
            let block = &*(handler as *const Block<(isize,), ()>);
            block.call((policy,));
        };
        let can_show: BOOL = msg_send![response, canShowMIMEType];
        let inner: id = msg_send![response, response];
        let is_http: BOOL = msg_send![inner, isKindOfClass: class!(NSHTTPURLResponse)];
        let attachment = is_http == YES && {
            let disposition: id =
                msg_send![inner, valueForHTTPHeaderField: ns_string("Content-Disposition")];
            rust_string(disposition)
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("attachment")
        };
        let for_main_frame: BOOL = msg_send![response, isForMainFrame];
        if for_main_frame == YES && (can_show == NO || attachment) {
            decide(POLICY_DOWNLOAD);
        } else {
            decide(POLICY_ALLOW);
        }
    }
}

extern "C" fn did_become_download(_: &Object, _: Sel, web_view: id, _: id, download: id) {
    unsafe {
        let Some(state) = page_state(web_view) else {
            return;
        };
        let _: () = msg_send![download, setDelegate: delegate()];
        let id = NEXT_DOWNLOAD_ID.with(|next| {
            let id = next.get();
            next.set(id + 1);
            id
        });
        let progress: id = msg_send![download, progress];
        let _: () = msg_send![
            progress,
            addObserver: delegate()
            forKeyPath: ns_string("fractionCompleted")
            options: 1usize
            context: download as *mut c_void
        ];
        let _: id = msg_send![download, retain];
        DOWNLOADS.with(|downloads| {
            downloads.borrow_mut().insert(
                download as usize,
                DownloadEntry {
                    id,
                    page: Rc::downgrade(&state),
                    progress,
                },
            );
        });
        let request: id = msg_send![download, originalRequest];
        let url: id = msg_send![request, URL];
        let url = if url == nil {
            String::new()
        } else {
            let string: id = msg_send![url, absoluteString];
            rust_string(string)
        };
        let filename = url
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or("download")
            .split(['?', '#'])
            .next()
            .unwrap_or("download")
            .to_owned();
        state.emit(WebViewEvent::Download(DownloadEvent::Started {
            id,
            url,
            filename,
        }));
    }
}

fn with_download(download: id, run: impl FnOnce(&DownloadEntry, Rc<PageState>)) {
    DOWNLOADS.with(|downloads| {
        let downloads = downloads.borrow();
        if let Some(entry) = downloads.get(&(download as usize))
            && let Some(page) = entry.page.upgrade()
        {
            run(entry, page);
        }
    });
}

fn finish_download(download: id) {
    let entry = DOWNLOADS.with(|downloads| downloads.borrow_mut().remove(&(download as usize)));
    if let Some(entry) = entry {
        unsafe {
            let _: () = msg_send![
                entry.progress,
                removeObserver: delegate()
                forKeyPath: ns_string("fractionCompleted")
            ];
            let _: () = msg_send![download, release];
        }
    }
}

extern "C" fn download_destination(
    _: &Object,
    _: Sel,
    download: id,
    _: id,
    suggested: id,
    handler: id,
) {
    unsafe {
        let path = unique_download_path(&rust_string(suggested));
        let url: id = msg_send![class!(NSURL), fileURLWithPath: ns_string(&path.to_string_lossy())];
        with_download(download, |entry, page| {
            page.emit(WebViewEvent::Download(DownloadEvent::Destination {
                id: entry.id,
                path: path.clone(),
            }));
        });
        let block = &*(handler as *const Block<(id,), ()>);
        block.call((url,));
    }
}

fn unique_download_path(suggested: &str) -> PathBuf {
    let name = std::path::Path::new(suggested)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "download".to_owned());
    let directory = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Downloads");
    let candidate = directory.join(&name);
    if !candidate.exists() {
        return candidate;
    }
    let path = std::path::Path::new(&name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|n| directory.join(format!("{stem} ({n}){extension}")))
        .find(|candidate| !candidate.exists())
        .expect("an unused download name exists")
}

extern "C" fn download_did_finish(_: &Object, _: Sel, download: id) {
    with_download(download, |entry, page| {
        page.emit(WebViewEvent::Download(DownloadEvent::Finished {
            id: entry.id,
        }));
    });
    finish_download(download);
}

extern "C" fn download_did_fail(_: &Object, _: Sel, download: id, error: id, _: id) {
    let message = unsafe {
        let description: id = msg_send![error, localizedDescription];
        rust_string(description)
    };
    with_download(download, |entry, page| {
        page.emit(WebViewEvent::Download(DownloadEvent::Failed {
            id: entry.id,
            message: message.clone(),
        }));
    });
    finish_download(download);
}

extern "C" fn process_did_terminate(_: &Object, _: Sel, web_view: id) {
    unsafe {
        emit(web_view, WebViewEvent::Crashed);
    }
}

extern "C" fn create_web_view(
    _: &Object,
    _: Sel,
    web_view: id,
    configuration: id,
    _: id,
    _: id,
) -> id {
    unsafe {
        let Some(state) = page_state(web_view) else {
            return nil;
        };
        let host_view = state.host_view as id;
        if host_view == nil {
            return nil;
        }
        let page =
            NativeWebView::with_configuration(host_view, configuration, state.labels.clone(), None);
        let created = page.web_view;
        state.emit(WebViewEvent::NewWindow(WebView {
            inner: super::Inner::Native(page),
        }));
        // WebKit expects a +0 reference; the new tab owns the page.
        created
    }
}

extern "C" fn web_view_did_close(_: &Object, _: Sel, web_view: id) {
    unsafe {
        emit(web_view, WebViewEvent::CloseRequested);
    }
}

unsafe fn page_host(frame: id) -> String {
    unsafe {
        let origin: id = msg_send![frame, securityOrigin];
        if origin == nil {
            return String::new();
        }
        let host: id = msg_send![origin, host];
        rust_string(host)
    }
}

unsafe fn alert(message: id, host: &str, buttons: &[&str]) -> id {
    unsafe {
        let alert: id = msg_send![class!(NSAlert), new];
        let title = if host.is_empty() {
            rust_string(message)
        } else {
            host.to_owned()
        };
        let _: () = msg_send![alert, setMessageText: ns_string(&title)];
        if !host.is_empty() {
            let _: () = msg_send![alert, setInformativeText: message];
        }
        for button in buttons {
            let _: id = msg_send![alert, addButtonWithTitle: ns_string(button)];
        }
        let _: id = msg_send![alert, autorelease];
        alert
    }
}

unsafe fn begin_sheet(alert: id, web_view: id, done: impl Fn(isize) + 'static) {
    unsafe {
        let window: id = msg_send![web_view, window];
        let block = ConcreteBlock::new(move |response: isize| done(response)).copy();
        if window == nil {
            let response: isize = msg_send![alert, runModal];
            block.call((response,));
        } else {
            let _: () =
                msg_send![alert, beginSheetModalForWindow: window completionHandler: &*block];
        }
    }
}

extern "C" fn run_alert(_: &Object, _: Sel, web_view: id, message: id, frame: id, handler: id) {
    unsafe {
        let Some(state) = page_state(web_view) else {
            let block = &*(handler as *const Block<(), ()>);
            block.call(());
            return;
        };
        let handler: id = msg_send![handler, copy];
        let alert = alert(message, &page_host(frame), &[&state.labels.ok]);
        begin_sheet(alert, web_view, move |_| {
            let block = &*(handler as *const Block<(), ()>);
            block.call(());
            let _: () = msg_send![handler, release];
        });
    }
}

extern "C" fn run_confirm(_: &Object, _: Sel, web_view: id, message: id, frame: id, handler: id) {
    unsafe {
        let Some(state) = page_state(web_view) else {
            let block = &*(handler as *const Block<(BOOL,), ()>);
            block.call((NO,));
            return;
        };
        let handler: id = msg_send![handler, copy];
        let alert = alert(
            message,
            &page_host(frame),
            &[&state.labels.ok, &state.labels.cancel],
        );
        begin_sheet(alert, web_view, move |response| {
            let block = &*(handler as *const Block<(BOOL,), ()>);
            block.call((if response == 1000 { YES } else { NO },));
            let _: () = msg_send![handler, release];
        });
    }
}

extern "C" fn run_prompt(
    _: &Object,
    _: Sel,
    web_view: id,
    prompt: id,
    default_text: id,
    frame: id,
    handler: id,
) {
    unsafe {
        let Some(state) = page_state(web_view) else {
            let block = &*(handler as *const Block<(id,), ()>);
            block.call((nil,));
            return;
        };
        let handler: id = msg_send![handler, copy];
        let alert = alert(
            prompt,
            &page_host(frame),
            &[&state.labels.ok, &state.labels.cancel],
        );
        let field: id = msg_send![class!(NSTextField), alloc];
        let field: id = msg_send![
            field,
            initWithFrame: NSRect::new(NSPoint::new(0., 0.), NSSize::new(260., 24.))
        ];
        if default_text != nil {
            let _: () = msg_send![field, setStringValue: default_text];
        }
        let _: () = msg_send![alert, setAccessoryView: field];
        let _: () = msg_send![field, release];
        begin_sheet(alert, web_view, move |response| {
            let value: id = if response == 1000 {
                msg_send![field, stringValue]
            } else {
                nil
            };
            let block = &*(handler as *const Block<(id,), ()>);
            block.call((value,));
            let _: () = msg_send![handler, release];
        });
    }
}

extern "C" fn run_open_panel(_: &Object, _: Sel, web_view: id, parameters: id, _: id, handler: id) {
    unsafe {
        let handler: id = msg_send![handler, copy];
        let panel: id = msg_send![class!(NSOpenPanel), openPanel];
        let multiple: BOOL = msg_send![parameters, allowsMultipleSelection];
        let directories: BOOL = msg_send![parameters, allowsDirectories];
        let _: () = msg_send![panel, setAllowsMultipleSelection: multiple];
        let _: () = msg_send![panel, setCanChooseDirectories: directories];
        let _: () = msg_send![panel, setCanChooseFiles: YES];
        let _: id = msg_send![panel, retain];
        let done = ConcreteBlock::new(move |response: isize| {
            let urls: id = if response == 1 {
                msg_send![panel, URLs]
            } else {
                nil
            };
            let block = &*(handler as *const Block<(id,), ()>);
            block.call((urls,));
            let _: () = msg_send![handler, release];
            let _: () = msg_send![panel, release];
        })
        .copy();
        let window: id = msg_send![web_view, window];
        if window == nil {
            let response: isize = msg_send![panel, runModal];
            done.call((response,));
        } else {
            let _: () =
                msg_send![panel, beginSheetModalForWindow: window completionHandler: &*done];
        }
    }
}

extern "C" fn did_receive_script_message(_: &Object, _: Sel, _: id, message: id) {
    unsafe {
        let web_view: id = msg_send![message, webView];
        let Some(state) = page_state(web_view) else {
            return;
        };
        let body: id = msg_send![message, body];
        let is_dictionary: BOOL = msg_send![body, isKindOfClass: class!(NSDictionary)];
        if is_dictionary == NO {
            return;
        }
        let field = |key: &str| -> String {
            let value: id = msg_send![body, objectForKey: ns_string(key)];
            if value == nil {
                return String::new();
            }
            let is_string: BOOL = msg_send![value, isKindOfClass: class!(NSString)];
            if is_string == YES {
                rust_string(value)
            } else {
                String::new()
            }
        };
        match field("type").as_str() {
            "contextmenu" => {
                *state.context_link.borrow_mut() = Some(field("link"));
            }
            "favicon" => {
                let href = field("href");
                if state.favicon_href.borrow().as_deref() == Some(href.as_str()) {
                    return;
                }
                *state.favicon_href.borrow_mut() = Some(href.clone());
                let serial = state.favicon_serial.get() + 1;
                state.favicon_serial.set(serial);
                if href.is_empty() {
                    state.emit(WebViewEvent::Favicon(None));
                    return;
                }
                let weak = Rc::downgrade(&state);
                fetch_favicon(&href, move |png| {
                    if let Some(state) = weak.upgrade()
                        && state.favicon_serial.get() == serial
                    {
                        state.emit(WebViewEvent::Favicon(png));
                    }
                });
            }
            _ => {}
        }
    }
}

/// Downloads an icon on the main queue and renders it to a 32×32 PNG, which
/// also turns `.ico`, SVG and WebP icons into one format.
fn fetch_favicon(href: &str, done: impl Fn(Option<Vec<u8>>) + 'static) {
    unsafe {
        let url: id = msg_send![class!(NSURL), URLWithString: ns_string(href)];
        if url == nil {
            done(None);
            return;
        }
        let session = favicon_session();
        let handler = ConcreteBlock::new(move |data: id, _: id, error: id| {
            if error != nil || data == nil {
                done(None);
                return;
            }
            done(render_icon(data));
        })
        .copy();
        let task: id = msg_send![session, dataTaskWithURL: url completionHandler: &*handler];
        let _: () = msg_send![task, resume];
    }
}

fn favicon_session() -> id {
    static SESSION: OnceLock<usize> = OnceLock::new();
    *SESSION.get_or_init(|| unsafe {
        let configuration: id = msg_send![
            class!(NSURLSessionConfiguration),
            defaultSessionConfiguration
        ];
        let queue: id = msg_send![class!(NSOperationQueue), mainQueue];
        let session: id = msg_send![
            class!(NSURLSession),
            sessionWithConfiguration: configuration
            delegate: nil
            delegateQueue: queue
        ];
        let _: id = msg_send![session, retain];
        session as usize
    }) as id
}

unsafe fn render_icon(data: id) -> Option<Vec<u8>> {
    unsafe {
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithData: data];
        if image == nil {
            return None;
        }
        let _: id = msg_send![image, autorelease];
        let valid: BOOL = msg_send![image, isValid];
        if valid == NO {
            return None;
        }
        let rep: id = msg_send![class!(NSBitmapImageRep), alloc];
        let rep: id = msg_send![
            rep,
            initWithBitmapDataPlanes: std::ptr::null_mut::<*mut u8>()
            pixelsWide: FAVICON_PIXELS
            pixelsHigh: FAVICON_PIXELS
            bitsPerSample: 8isize
            samplesPerPixel: 4isize
            hasAlpha: YES
            isPlanar: NO
            colorSpaceName: ns_string("NSCalibratedRGBColorSpace")
            bytesPerRow: 0isize
            bitsPerPixel: 0isize
        ];
        if rep == nil {
            return None;
        }
        let _: id = msg_send![rep, autorelease];
        let context: id =
            msg_send![class!(NSGraphicsContext), graphicsContextWithBitmapImageRep: rep];
        let _: () = msg_send![class!(NSGraphicsContext), saveGraphicsState];
        let _: () = msg_send![class!(NSGraphicsContext), setCurrentContext: context];
        let rect = NSRect::new(
            NSPoint::new(0., 0.),
            NSSize::new(FAVICON_PIXELS as f64, FAVICON_PIXELS as f64),
        );
        let zero = NSRect::new(NSPoint::new(0., 0.), NSSize::new(0., 0.));
        let _: () = msg_send![
            image,
            drawInRect: rect
            fromRect: zero
            operation: 2usize
            fraction: 1.0f64
        ];
        let _: () = msg_send![class!(NSGraphicsContext), restoreGraphicsState];
        png_data(rep)
    }
}

unsafe fn png_data(rep: id) -> Option<Vec<u8>> {
    unsafe {
        let properties: id = msg_send![class!(NSDictionary), dictionary];
        let png: id = msg_send![rep, representationUsingType: PNG_FILE_TYPE properties: properties];
        if png == nil {
            return None;
        }
        let length: usize = msg_send![png, length];
        let bytes: *const u8 = msg_send![png, bytes];
        (!bytes.is_null() && length > 0).then(|| std::slice::from_raw_parts(bytes, length).to_vec())
    }
}

unsafe fn take_snapshot(web_view: id, done: impl FnOnce(Option<id>) + 'static) {
    unsafe {
        let done = RefCell::new(Some(done));
        let handler = ConcreteBlock::new(move |image: id, _: id| {
            if let Some(done) = done.borrow_mut().take() {
                done((image != nil).then_some(image));
            }
        })
        .copy();
        let _: () = msg_send![
            web_view,
            takeSnapshotWithConfiguration: nil
            completionHandler: &*handler
        ];
    }
}

unsafe fn evaluate(web_view: id, script: &str, done: impl FnOnce(Option<String>) + 'static) {
    unsafe {
        let done = RefCell::new(Some(done));
        let handler = ConcreteBlock::new(move |result: id, _: id| {
            let Some(done) = done.borrow_mut().take() else {
                return;
            };
            let is_string = result != nil && {
                let is_string: BOOL = msg_send![result, isKindOfClass: class!(NSString)];
                is_string == YES
            };
            done(is_string.then(|| rust_string(result)));
        })
        .copy();
        let _: () = msg_send![
            web_view,
            evaluateJavaScript: ns_string(script)
            completionHandler: &*handler
        ];
    }
}

extern "C" fn observe_value(
    _: &Object,
    _: Sel,
    key_path: id,
    object: id,
    _: id,
    context: *mut c_void,
) {
    unsafe {
        let key = rust_string(key_path);
        if key == "fractionCompleted" {
            let download = context as id;
            let fraction: f64 = msg_send![object, fractionCompleted];
            with_download(download, |entry, page| {
                page.emit(WebViewEvent::Download(DownloadEvent::Progress {
                    id: entry.id,
                    fraction,
                }));
            });
            return;
        }
        let web_view = object;
        let event = match key.as_str() {
            "title" => {
                let title: id = msg_send![web_view, title];
                WebViewEvent::Title(rust_string(title))
            }
            "URL" => WebViewEvent::Url(web_view_url(web_view)),
            "estimatedProgress" => {
                let progress: f64 = msg_send![web_view, estimatedProgress];
                WebViewEvent::Progress(progress)
            }
            "loading" => {
                let loading: BOOL = msg_send![web_view, isLoading];
                WebViewEvent::Loading(loading == YES)
            }
            "canGoBack" | "canGoForward" => {
                let back: BOOL = msg_send![web_view, canGoBack];
                let forward: BOOL = msg_send![web_view, canGoForward];
                WebViewEvent::History {
                    can_go_back: back == YES,
                    can_go_forward: forward == YES,
                }
            }
            _ => return,
        };
        emit(web_view, event);
    }
}

/// Clears what "Clear browsing data" names in the shared data store.
pub fn clear_browsing_data(kinds: ClearData, done: Box<dyn FnOnce()>) {
    unsafe {
        let store: id = msg_send![class!(WKWebsiteDataStore), defaultDataStore];
        let types: id = match kinds {
            ClearData::Cookies => {
                let types = [
                    "WKWebsiteDataTypeCookies",
                    "WKWebsiteDataTypeLocalStorage",
                    "WKWebsiteDataTypeSessionStorage",
                    "WKWebsiteDataTypeIndexedDBDatabases",
                    "WKWebsiteDataTypeWebSQLDatabases",
                ];
                string_set(&types)
            }
            ClearData::Cache => {
                let types = [
                    "WKWebsiteDataTypeDiskCache",
                    "WKWebsiteDataTypeMemoryCache",
                    "WKWebsiteDataTypeOfflineWebApplicationCache",
                    "WKWebsiteDataTypeFetchCache",
                ];
                string_set(&types)
            }
        };
        let since: id = msg_send![class!(NSDate), distantPast];
        let done = RefCell::new(Some(done));
        let handler = ConcreteBlock::new(move || {
            if let Some(done) = done.borrow_mut().take() {
                done();
            }
        })
        .copy();
        let _: () = msg_send![
            store,
            removeDataOfTypes: types
            modifiedSince: since
            completionHandler: &*handler
        ];
    }
}

unsafe fn string_set(values: &[&str]) -> id {
    unsafe {
        let set: id = msg_send![class!(NSMutableSet), set];
        for value in values {
            let _: () = msg_send![set, addObject: ns_string(value)];
        }
        set
    }
}

unsafe fn web_view_url(web_view: id) -> String {
    unsafe {
        let url: id = msg_send![web_view, URL];
        if url == nil {
            return String::new();
        }
        let string: id = msg_send![url, absoluteString];
        rust_string(string)
    }
}

unsafe fn responds(object: id, selector: Sel) -> bool {
    unsafe {
        let responds: BOOL = msg_send![object, respondsToSelector: selector];
        responds == YES
    }
}

/// An autoreleased `NSString`.
fn ns_string(text: &str) -> id {
    unsafe {
        let string: id = msg_send![class!(NSString), alloc];
        let string: id = msg_send![
            string,
            initWithBytes: text.as_ptr() as *const c_void
            length: text.len()
            encoding: UTF8_ENCODING
        ];
        msg_send![string, autorelease]
    }
}

unsafe fn rust_string(string: id) -> String {
    unsafe {
        if string == nil {
            return String::new();
        }
        let is_string: BOOL = msg_send![string, isKindOfClass: class!(NSString)];
        if is_string == NO {
            return String::new();
        }
        let utf8: *const std::os::raw::c_char = msg_send![string, UTF8String];
        if utf8.is_null() {
            String::new()
        } else {
            CStr::from_ptr(utf8).to_string_lossy().into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webkit_errors_map_to_the_reference_error_names() {
        assert_eq!(
            chromium_error_name("NSURLErrorDomain", -1003),
            "ERR_NAME_NOT_RESOLVED"
        );
        assert_eq!(
            chromium_error_name("NSURLErrorDomain", -1004),
            "ERR_CONNECTION_REFUSED"
        );
        assert_eq!(
            chromium_error_name("NSURLErrorDomain", -1202),
            "ERR_CERT_AUTHORITY_INVALID"
        );
        assert_eq!(
            chromium_error_name("NSURLErrorDomain", -9),
            "ERR_FAILED (-9)"
        );
        assert_eq!(
            chromium_error_name("WebKitErrorDomain", 101),
            "ERR_FAILED (101)"
        );
    }

    #[test]
    fn downloads_never_overwrite_a_file() {
        let path = unique_download_path("../../etc/passwd");
        assert_eq!(path.file_name().unwrap(), "passwd");
        assert!(path.parent().unwrap().ends_with("Downloads"));
    }
}
