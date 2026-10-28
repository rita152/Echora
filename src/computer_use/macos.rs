//! ScreenCaptureKit window streams. No screenshot encoding or permission prompts.

use super::{CaptureError, MAX_WINDOWS, Target, WindowId, WindowInfo};
use block::ConcreteBlock;
use cocoa::{
    base::{id, nil},
    foundation::{NSAutoreleasePool, NSRect},
};
use core_foundation::base::TCFType;
use core_video::pixel_buffer::{CVPixelBuffer, CVPixelBufferRef};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, c_void},
    sync::{Arc, Mutex, OnceLock},
};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGWindowListCopyWindowInfo(options: u32, relative: u32) -> id;
}
#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMSampleBufferGetImageBuffer(sample: *const c_void) -> CVPixelBufferRef;
    fn CMSampleBufferGetSampleAttachmentsArray(sample: *const c_void, create: bool) -> id;
}
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> *const c_void;
    fn AXUIElementCopyAttributeValue(element: *const c_void, name: id, value: *mut id) -> i32;
    fn AXUIElementPerformAction(element: *const c_void, action: id) -> i32;
    fn AXUIElementSetAttributeValue(element: *const c_void, name: id, value: id) -> i32;
}

fn preflight() -> Result<(), CaptureError> {
    static LOADED: OnceLock<bool> = OnceLock::new();
    let loaded = *LOADED.get_or_init(|| unsafe {
        !libc::dlopen(
            c"/System/Library/Frameworks/ScreenCaptureKit.framework/ScreenCaptureKit".as_ptr(),
            libc::RTLD_LAZY,
        )
        .is_null()
    });
    if !loaded || Class::get("SCStream").is_none() {
        return Err(CaptureError::Unsupported);
    }
    if !unsafe { CGPreflightScreenCaptureAccess() } {
        eprintln!("Computer Use preview: Screen Recording preflight denied (cause unknown)");
        return Err(CaptureError::PermissionRequired);
    }
    Ok(())
}

pub(super) fn permission_diagnostics() -> String {
    format!(
        "screenCapturePreflight: {}\naccessibilityTrusted: {}\nScreen capture preflight does not identify the reason for a denial.",
        unsafe { CGPreflightScreenCaptureAccess() },
        unsafe { AXIsProcessTrusted() }
    )
}

fn capture_error(domain: &str, code: i64) -> CaptureError {
    if domain == "com.apple.ScreenCaptureKit.SCStreamErrorDomain" && code == -3801 {
        CaptureError::PermissionRequired
    } else {
        CaptureError::CaptureFailed
    }
}

#[allow(unexpected_cfgs)]
unsafe fn native_error(error: id) -> CaptureError {
    if error == nil {
        return CaptureError::CaptureFailed;
    }
    let domain: id = unsafe { msg_send![error, domain] };
    let code: i64 = unsafe { msg_send![error, code] };
    let domain = unsafe { string(domain) };
    eprintln!("Computer Use preview: native capture error {domain} ({code})");
    capture_error(&domain, code)
}

#[allow(unexpected_cfgs)] // objc 0.2 macros probe the legacy cargo-clippy cfg.
unsafe fn string(value: id) -> String {
    if value == nil {
        return String::new();
    }
    let bytes: *const std::ffi::c_char = unsafe { msg_send![value, UTF8String] };
    if bytes.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(bytes) }
            .to_string_lossy()
            .into_owned()
    }
}

fn matches(target: &Target, bundle: &str, name: &str) -> bool {
    match target {
        Target::AppId(value) => value == bundle,
        Target::DisplayName(value) => value.eq_ignore_ascii_case(name),
    }
}

#[allow(unexpected_cfgs)]
unsafe fn info(window: id, targets: &[Target]) -> Option<WindowInfo> {
    unsafe {
        let owner: id = msg_send![window, owningApplication];
        if owner == nil {
            return None;
        }
        let bundle: id = msg_send![owner, bundleIdentifier];
        let name: id = msg_send![owner, applicationName];
        let bundle = string(bundle);
        let application = string(name);
        let target = targets.iter().find(|t| matches(t, &bundle, &application))?;
        let layer: i64 = msg_send![window, windowLayer];
        let on_screen: bool = msg_send![window, isOnScreen];
        let rect: NSRect = msg_send![window, frame];
        if !on_screen || layer != 0 || rect.size.width <= 1. || rect.size.height <= 1. {
            return None;
        }
        let title: id = msg_send![window, title];
        Some(WindowInfo {
            id: WindowId {
                process: msg_send![owner, processID],
                window: msg_send![window, windowID],
            },
            target: target.clone(),
            title: string(title),
            application,
            width: rect.size.width,
            height: rect.size.height,
        })
    }
}

/// AXWindows excludes implementation windows (for example titlebar helpers)
/// that WindowServer and ScreenCaptureKit also expose as on-screen layer 0.
#[allow(unexpected_cfgs)]
fn accessible_windows(process: i32) -> Result<HashSet<u32>, CaptureError> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err(CaptureError::AccessibilityRequired);
    }
    unsafe {
        let function = libc::dlsym(libc::RTLD_DEFAULT, c"_AXUIElementGetWindow".as_ptr());
        if function.is_null() {
            return Err(CaptureError::Unsupported);
        }
        let get_window: unsafe extern "C" fn(id, *mut u32) -> i32 = std::mem::transmute(function);
        let app = AXUIElementCreateApplication(process);
        let key: id = msg_send![class!(NSString), stringWithUTF8String:c"AXWindows".as_ptr()];
        let mut windows = nil;
        let status = AXUIElementCopyAttributeValue(app, key, &mut windows);
        core_foundation::base::CFRelease(app as _);
        if status != 0 || windows == nil {
            return Err(CaptureError::WindowUnavailable);
        }
        let count: usize = msg_send![windows, count];
        let mut ids = HashSet::new();
        for index in 0..count {
            let window: id = msg_send![windows, objectAtIndex:index];
            if !is_application_window(window) {
                continue;
            }
            let mut identity = 0;
            if get_window(window, &mut identity) == 0 {
                ids.insert(identity);
            }
        }
        core_foundation::base::CFRelease(windows as _);
        Ok(ids)
    }
}

fn previewable_role(subrole: &str, modal: bool, close_button: bool) -> bool {
    subrole == "AXStandardWindow" || (subrole == "AXDialog" && (modal || close_button))
}

#[allow(unexpected_cfgs)]
unsafe fn is_application_window(window: id) -> bool {
    unsafe {
        let mut subrole = nil;
        let mut modal = nil;
        let mut close = nil;
        for (name, value) in [
            (c"AXSubrole", &mut subrole),
            (c"AXModal", &mut modal),
            (c"AXCloseButton", &mut close),
        ] {
            let key: id = msg_send![class!(NSString), stringWithUTF8String:name.as_ptr()];
            AXUIElementCopyAttributeValue(window.cast(), key, value);
        }
        let is_modal = modal != nil && msg_send![modal, boolValue];
        let result = previewable_role(&string(subrole), is_modal, close != nil);
        for value in [subrole, modal, close] {
            if value != nil {
                core_foundation::base::CFRelease(value as _);
            }
        }
        result
    }
}

#[allow(unexpected_cfgs)]
pub(super) fn windows(
    targets: Vec<Target>,
) -> async_channel::Receiver<Result<Vec<WindowInfo>, CaptureError>> {
    let (sender, receiver) = async_channel::bounded(1);
    if let Err(error) = preflight() {
        let _ = sender.try_send(Err(error));
        return receiver;
    }
    unsafe {
        let handler = ConcreteBlock::new(move |content: id, error: id| {
            let pool = NSAutoreleasePool::new(nil);
            if content == nil || error != nil {
                let _ = sender.try_send(Err(native_error(error)));
            } else {
                let windows: id = msg_send![content, windows];
                let count: usize = msg_send![windows, count];
                let mut result = Vec::new();
                let mut accessible = HashMap::new();
                for i in 0..count {
                    let window: id = msg_send![windows, objectAtIndex:i];
                    if let Some(info) = info(window, &targets) {
                        let ids = accessible
                            .entry(info.id.process)
                            .or_insert_with(|| accessible_windows(info.id.process));
                        match ids {
                            Ok(ids) if ids.contains(&info.id.window) => result.push(info),
                            Err(CaptureError::AccessibilityRequired) => {
                                let _ = sender.try_send(Err(CaptureError::AccessibilityRequired));
                                pool.drain();
                                return;
                            }
                            _ => {}
                        }
                    }
                }
                // Query WindowServer explicitly; SCK does not guarantee ordering.
                let ordered = CGWindowListCopyWindowInfo(1, 0);
                if ordered != nil {
                    let count: usize = msg_send![ordered, count];
                    let key: id = msg_send![class!(NSString), stringWithUTF8String:c"kCGWindowNumber".as_ptr()];
                    let mut rank = HashMap::new();
                    for index in 0..count {
                        let entry: id = msg_send![ordered, objectAtIndex:index];
                        let number: id = msg_send![entry, objectForKey:key];
                        if number != nil {
                            let identity: u32 = msg_send![number, unsignedIntValue];
                            rank.insert(identity, index);
                        }
                    }
                    result.sort_by_key(|window| rank.get(&window.id.window).copied().unwrap_or(usize::MAX));
                    core_foundation::base::CFRelease(ordered as _);
                }
                result.truncate(MAX_WINDOWS);
                let _ = sender.try_send(Ok(result));
            }
            pool.drain();
        })
        .copy();
        let _: () = msg_send![class!(SCShareableContent), getShareableContentExcludingDesktopWindows:true onScreenWindowsOnly:true completionHandler:handler];
    }
    receiver
}

// SCK owns the storage and supplies a completed immutable frame. Retaining it
// prevents reuse while the UI reads it; the mutex transfers sole wrapper ownership.
struct VideoFrame(CVPixelBuffer);
unsafe impl Send for VideoFrame {}

#[derive(Default)]
struct VideoState {
    latest: Option<VideoFrame>,
    error: Option<CaptureError>,
}

/// Owns the native stream. The delegate owns a single latest frame, never a queue.
pub(crate) struct Stream {
    stream: id,
    output: id,
    state: Arc<Mutex<VideoState>>,
}
impl Stream {
    pub(crate) fn take_frame(&self) -> Result<Option<CVPixelBuffer>, CaptureError> {
        let mut state = self.state.lock().unwrap();
        if let Some(error) = state.error {
            return Err(error);
        }
        Ok(state.latest.take().map(|frame| frame.0))
    }

    #[allow(unexpected_cfgs)]
    pub(crate) fn start(window: WindowInfo) -> async_channel::Receiver<Result<Self, CaptureError>> {
        let (sender, receiver) = async_channel::bounded(1);
        if let Err(error) = preflight() {
            let _ = sender.try_send(Err(error));
            return receiver;
        }
        unsafe {
            let handler = ConcreteBlock::new(move |content: id, error: id| {
                let pool = NSAutoreleasePool::new(nil);
                if content == nil || error != nil { let _ = sender.try_send(Err(native_error(error))); pool.drain(); return; }
                let windows: id = msg_send![content, windows];
                let count: usize = msg_send![windows, count];
                let mut selected = nil;
                for i in 0..count {
                    let candidate: id = msg_send![windows, objectAtIndex:i];
                    if info(candidate, std::slice::from_ref(&window.target)).is_some_and(|v| v.id == window.id) { selected = candidate; break; }
                }
                if selected == nil { let _ = sender.try_send(Err(CaptureError::WindowUnavailable)); pool.drain(); return; }
                let state = Arc::new(Mutex::new(VideoState::default()));
                let output: id = msg_send![output_class(), new];
                (*output).set_ivar("state", Box::into_raw(Box::new(state.clone())) as *mut c_void);
                let filter: id = msg_send![class!(SCContentFilter), alloc];
                let filter: id = msg_send![filter, initWithDesktopIndependentWindow:selected];
                let config: id = msg_send![class!(SCStreamConfiguration), new];
                let scale = (800. / window.width.max(window.height)).min(2.);
                let _: () = msg_send![config, setWidth:(window.width*scale).round().max(2.) as usize];
                let _: () = msg_send![config, setHeight:(window.height*scale).round().max(2.) as usize];
                let _: () = msg_send![config, setPixelFormat:0x42475241u32]; // BGRA, no CPU conversion.
                let _: () = msg_send![config, setQueueDepth:3usize];
                let _: () = msg_send![config, setMinimumFrameInterval:CMTime { value:1, timescale:30, flags:1, epoch:0 }];
                let _: () = msg_send![config, setShowsCursor:false];
                if msg_send![config, respondsToSelector:sel!(setIgnoreShadowsSingleWindow:)] {
                    let _: () = msg_send![config, setIgnoreShadowsSingleWindow:true];
                }
                let stream: id = msg_send![class!(SCStream), alloc];
                let stream: id = msg_send![stream, initWithFilter:filter configuration:config delegate:output];
                let _: () = msg_send![filter, release]; let _: () = msg_send![config, release];
                let mut error: id = nil;
                let added: bool = msg_send![stream, addStreamOutput:output type:0i64 sampleHandlerQueue:std::ptr::null::<c_void>() error:&mut error];
                if !added || error != nil {
                    let _: () = msg_send![stream, release]; let _: () = msg_send![output, release];
                    let _ = sender.try_send(Err(native_error(error))); pool.drain(); return;
                }
                let sender = sender.clone();
                let complete = ConcreteBlock::new(move |error: id| {
                    let result = if error == nil { Ok(Stream { stream, output, state:state.clone() }) }
                    else {
                        let _: () = msg_send![stream, release]; let _: () = msg_send![output, release];
                        Err(native_error(error))
                    };
                    // A cancelled receiver drops the stream, stopping capture immediately.
                    let _ = sender.try_send(result);
                }).copy();
                let _: () = msg_send![stream, startCaptureWithCompletionHandler:complete];
                pool.drain();
            }).copy();
            let _: () = msg_send![class!(SCShareableContent), getShareableContentExcludingDesktopWindows:true onScreenWindowsOnly:true completionHandler:handler];
        }
        receiver
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}
unsafe impl objc::Encode for CMTime {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CMTime=qiIq}") }
    }
}

#[allow(unexpected_cfgs)]
impl Drop for Stream {
    fn drop(&mut self) {
        unsafe {
            let mut error: id = nil;
            let _: bool =
                msg_send![self.stream, removeStreamOutput:self.output type:0i64 error:&mut error];
            let complete = ConcreteBlock::new(|_: id| {}).copy();
            let _: () = msg_send![self.stream, stopCaptureWithCompletionHandler:complete];
            let _: () = msg_send![self.stream, release];
            let _: () = msg_send![self.output, release];
        }
    }
}

#[allow(unexpected_cfgs)]
fn output_class() -> &'static Class {
    static CLASS: OnceLock<&'static Class> = OnceLock::new();
    CLASS.get_or_init(|| {
        let mut c = ClassDecl::new("EchoraWindowStreamOutput", class!(NSObject)).unwrap();
        c.add_ivar::<*mut c_void>("state");
        unsafe {
            c.add_method(
                sel!(stream:didOutputSampleBuffer:ofType:),
                frame as extern "C" fn(&Object, Sel, id, *const c_void, i64),
            );
            c.add_method(
                sel!(stream:didStopWithError:),
                stopped as extern "C" fn(&Object, Sel, id, id),
            );
            c.add_method(sel!(dealloc), dealloc as extern "C" fn(&mut Object, Sel));
        }
        c.register()
    })
}

extern "C" fn frame(this: &Object, _: Sel, _: id, sample: *const c_void, kind: i64) {
    if kind != 0 {
        return;
    }
    unsafe {
        let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, false);
        if attachments != nil {
            let key =
                libc::dlsym(libc::RTLD_DEFAULT, c"SCStreamFrameInfoStatus".as_ptr()) as *const id;
            if !key.is_null() {
                #[allow(unexpected_cfgs)]
                let count: usize = msg_send![attachments, count];
                if count > 0 {
                    #[allow(unexpected_cfgs)]
                    let attachment: id = msg_send![attachments, objectAtIndex:0usize];
                    #[allow(unexpected_cfgs)]
                    let status: id = msg_send![attachment, objectForKey:*key];
                    if status != nil {
                        #[allow(unexpected_cfgs)]
                        let status: i64 = msg_send![status, integerValue];
                        if status != 0 {
                            return;
                        }
                    }
                }
            }
        }
        let buffer = CMSampleBufferGetImageBuffer(sample);
        if buffer.is_null() {
            return;
        }
        let state = &*(*this.get_ivar::<*mut c_void>("state") as *const Arc<Mutex<VideoState>>);
        state.lock().unwrap().latest = Some(VideoFrame(CVPixelBuffer::wrap_under_get_rule(buffer)));
    }
}
extern "C" fn stopped(this: &Object, _: Sel, _: id, error: id) {
    unsafe {
        let state = &*(*this.get_ivar::<*mut c_void>("state") as *const Arc<Mutex<VideoState>>);
        state.lock().unwrap().error = Some(native_error(error));
    }
}
#[allow(unexpected_cfgs)]
extern "C" fn dealloc(this: &mut Object, _: Sel) {
    unsafe {
        let ptr = *this.get_ivar::<*mut c_void>("state");
        if !ptr.is_null() {
            drop(Box::from_raw(ptr as *mut Arc<Mutex<VideoState>>));
        }
        let _: () = msg_send![super(this, class!(NSObject)), dealloc];
    }
}

#[allow(unexpected_cfgs)]
pub(super) fn focus(window: &WindowInfo) -> bool {
    if !unsafe { AXIsProcessTrusted() } {
        return false;
    }
    unsafe {
        let app: id = msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier:window.id.process];
        if app == nil {
            return false;
        }
        let bundle: id = msg_send![app, bundleIdentifier];
        let name: id = msg_send![app, localizedName];
        if !matches(&window.target, &string(bundle), &string(name)) {
            return false;
        }
        let ax = AXUIElementCreateApplication(window.id.process);
        let key: id = msg_send![class!(NSString), stringWithUTF8String:c"AXWindows".as_ptr()];
        let action: id = msg_send![class!(NSString), stringWithUTF8String:c"AXRaise".as_ptr()];
        let mut windows: id = nil;
        let mut raised = false;
        // Match native identities; never substitute the application's other window.
        let function = libc::dlsym(libc::RTLD_DEFAULT, c"_AXUIElementGetWindow".as_ptr());
        if !function.is_null() && AXUIElementCopyAttributeValue(ax, key, &mut windows) == 0 {
            let get_window: unsafe extern "C" fn(id, *mut u32) -> i32 =
                std::mem::transmute(function);
            let count: usize = msg_send![windows, count];
            for i in 0..count {
                let candidate: id = msg_send![windows,objectAtIndex:i];
                let mut id = 0;
                if get_window(candidate, &mut id) == 0 && id == window.id.window {
                    let activated: bool = msg_send![app, activateWithOptions: 2usize];
                    // AXRaise changes z-order but does not make a background
                    // window the keyboard target. Select it as the main window
                    // as well, then verify the app's focused native identity.
                    let main: id =
                        msg_send![class!(NSString), stringWithUTF8String:c"AXMain".as_ptr()];
                    let focused: id = msg_send![class!(NSString), stringWithUTF8String:c"AXFocusedWindow".as_ptr()];
                    if activated && AXUIElementPerformAction(candidate.cast(), action) == 0 {
                        AXUIElementSetAttributeValue(
                            candidate.cast(),
                            main,
                            core_foundation::boolean::CFBoolean::true_value().as_concrete_TypeRef()
                                as id,
                        );
                        let mut selected = nil;
                        if AXUIElementCopyAttributeValue(ax, focused, &mut selected) == 0
                            && selected != nil
                        {
                            let mut identity = 0;
                            raised = get_window(selected, &mut identity) == 0
                                && identity == window.id.window;
                            core_foundation::base::CFRelease(selected as _);
                        }
                    }
                    break;
                }
            }
            core_foundation::base::CFRelease(windows as _);
        }
        core_foundation::base::CFRelease(ax as _);
        raised
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_permission_errors_are_distinct_from_unrelated_native_failures() {
        assert_eq!(
            capture_error("com.apple.ScreenCaptureKit.SCStreamErrorDomain", -3801),
            CaptureError::PermissionRequired
        );
        assert_eq!(
            capture_error("com.apple.ScreenCaptureKit.SCStreamErrorDomain", -3802),
            CaptureError::CaptureFailed
        );
        assert_eq!(capture_error("other", -3801), CaptureError::CaptureFailed);
        assert_ne!(
            CaptureError::PermissionRequired.settings_url(),
            CaptureError::AccessibilityRequired.settings_url()
        );
        assert!(CaptureError::CaptureFailed.settings_url().is_none());
    }

    #[test]
    fn helper_panels_are_excluded_without_size_or_title_heuristics() {
        assert!(previewable_role("AXStandardWindow", false, false));
        assert!(previewable_role("AXDialog", true, false));
        assert!(previewable_role("AXDialog", false, true));
        assert!(!previewable_role("AXDialog", false, false));
        assert!(!previewable_role("AXFloatingWindow", false, false));
    }

    #[test]
    fn target_identity_never_falls_back_to_an_unrelated_application() {
        assert!(matches(
            &Target::AppId("com.test".into()),
            "com.test",
            "Test"
        ));
        assert!(!matches(
            &Target::AppId("com.test".into()),
            "com.other",
            "com.test"
        ));
        assert!(matches(
            &Target::DisplayName("Test".into()),
            "com.test",
            "test"
        ));
        assert!(!matches(
            &Target::DisplayName("Test".into()),
            "com.test",
            "Test Preview"
        ));
    }
}
