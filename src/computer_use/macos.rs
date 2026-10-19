//! ScreenCaptureKit snapshots, sampled by the view while its task is visible.
//! Preflight never prompts; missing authorization is an ordinary UI state.

use std::{ffi::CStr, slice, sync::OnceLock};

use block::ConcreteBlock;
use cocoa::{
    base::{id, nil},
    foundation::{NSAutoreleasePool, NSRect},
};
use objc::{class, msg_send, sel, sel_impl};

use super::{CaptureError, Frame, Target};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
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

#[allow(unexpected_cfgs)] // objc 0.2 macros probe the legacy cargo-clippy cfg.
pub(super) fn capture(target: Target) -> async_channel::Receiver<Result<Frame, CaptureError>> {
    let (sender, receiver) = async_channel::bounded(1);
    // Echora also runs on macOS versions predating ScreenCaptureKit. Keep the
    // framework optional rather than making those releases fail at app launch.
    static FRAMEWORK_LOADED: OnceLock<bool> = OnceLock::new();
    let loaded = *FRAMEWORK_LOADED.get_or_init(|| unsafe {
        !libc::dlopen(
            c"/System/Library/Frameworks/ScreenCaptureKit.framework/ScreenCaptureKit".as_ptr(),
            libc::RTLD_LAZY,
        )
        .is_null()
    });
    if !loaded || objc::runtime::Class::get("SCScreenshotManager").is_none() {
        let _ = sender.try_send(Err(CaptureError::Unsupported));
        return receiver;
    }
    if !unsafe { CGPreflightScreenCaptureAccess() } {
        let _ = sender.try_send(Err(CaptureError::PermissionRequired));
        return receiver;
    }
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let handler = ConcreteBlock::new(move |content: id, error: id| {
            let pool = NSAutoreleasePool::new(nil);
            if content == nil || error != nil {
                let _ = sender.try_send(Err(CaptureError::CaptureFailed));
                pool.drain();
                return;
            }
            let windows: id = msg_send![content, windows];
            let count: usize = msg_send![windows, count];
            let mut selected = nil;
            let mut selected_area = 0.;
            for index in 0..count {
                let window: id = msg_send![windows, objectAtIndex: index];
                let owner: id = msg_send![window, owningApplication];
                if owner == nil {
                    continue;
                }
                let bundle: id = msg_send![owner, bundleIdentifier];
                let name: id = msg_send![owner, applicationName];
                let on_screen: bool = msg_send![window, isOnScreen];
                let layer: i64 = msg_send![window, windowLayer];
                let frame: NSRect = msg_send![window, frame];
                let area = frame.size.width * frame.size.height;
                if on_screen && layer == 0 && area > selected_area
                    && matches(&target, &string(bundle), &string(name))
                {
                    selected = window;
                    selected_area = area;
                }
            }
            if selected == nil {
                let _ = sender.try_send(Err(CaptureError::WindowUnavailable));
                pool.drain();
                return;
            }
            let title: id = msg_send![selected, title];
            let title = string(title);
            let owner: id = msg_send![selected, owningApplication];
            let application: id = msg_send![owner, applicationName];
            let application = string(application);
            let frame: NSRect = msg_send![selected, frame];
            let filter: id = msg_send![class!(SCContentFilter), alloc];
            let filter: id = msg_send![filter, initWithDesktopIndependentWindow: selected];
            let config: id = msg_send![class!(SCStreamConfiguration), new];
            let scale = (960. / frame.size.width.max(1.)).min(1.);
            let _: () = msg_send![config, setWidth: (frame.size.width * scale).max(1.) as usize];
            let _: () = msg_send![config, setHeight: (frame.size.height * scale).max(1.) as usize];
            let _: () = msg_send![config, setShowsCursor: false];
            let sender = sender.clone();
            let screenshot = ConcreteBlock::new(move |image: *const std::ffi::c_void, error: id| {
                let pool = NSAutoreleasePool::new(nil);
                let result = if image.is_null() || error != nil {
                    Err(CaptureError::CaptureFailed)
                } else {
                    let bitmap: id = msg_send![class!(NSBitmapImageRep), alloc];
                    let bitmap: id = msg_send![bitmap, initWithCGImage: image];
                    let properties: id = msg_send![class!(NSDictionary), dictionary];
                    let data: id = msg_send![bitmap, representationUsingType: 4usize properties: properties];
                    let result = if data == nil {
                        Err(CaptureError::CaptureFailed)
                    } else {
                        let length: usize = msg_send![data, length];
                        let bytes: *const u8 = msg_send![data, bytes];
                        if bytes.is_null() || length == 0 {
                            Err(CaptureError::CaptureFailed)
                        } else {
                            Ok(Frame {
                                png: slice::from_raw_parts(bytes, length).to_vec(),
                                title: title.clone(),
                                application: application.clone(),
                            })
                        }
                    };
                    let _: () = msg_send![bitmap, release];
                    result
                };
                let _ = sender.try_send(result);
                pool.drain();
            }).copy();
            let _: () = msg_send![class!(SCScreenshotManager), captureImageWithFilter: filter configuration: config completionHandler: screenshot];
            let _: () = msg_send![filter, release];
            let _: () = msg_send![config, release];
            pool.drain();
        }).copy();
        let _: () = msg_send![class!(SCShareableContent), getShareableContentExcludingDesktopWindows: true onScreenWindowsOnly: true completionHandler: handler];
        pool.drain();
    }
    receiver
}

#[allow(unexpected_cfgs)] // objc 0.2 macros probe the legacy cargo-clippy cfg.
pub(super) fn focus(target: &Target) -> bool {
    unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let apps: id = msg_send![workspace, runningApplications];
        let count: usize = msg_send![apps, count];
        for index in 0..count {
            let app: id = msg_send![apps, objectAtIndex: index];
            let bundle: id = msg_send![app, bundleIdentifier];
            let name: id = msg_send![app, localizedName];
            if matches(target, &string(bundle), &string(name)) {
                return msg_send![app, activateWithOptions: 3usize];
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_never_fall_back_to_an_unrelated_application() {
        assert!(matches(
            &Target::AppId("com.test.app".into()),
            "com.test.app",
            "Test"
        ));
        assert!(!matches(
            &Target::AppId("com.test.app".into()),
            "com.other",
            "com.test.app"
        ));
        assert!(matches(
            &Target::DisplayName("Test".into()),
            "com.test.app",
            "test"
        ));
        assert!(!matches(
            &Target::DisplayName("Test".into()),
            "com.test.app",
            "Test Preview"
        ));
    }

    #[test]
    fn missing_permission_is_reported_without_requesting_access() {
        if unsafe { CGPreflightScreenCaptureAccess() } {
            return;
        }
        let result = capture(Target::AppId("com.echora.verification.nonexistent".into()))
            .try_recv()
            .unwrap();
        assert!(matches!(
            result,
            Err(CaptureError::PermissionRequired | CaptureError::Unsupported)
        ));
    }
}
