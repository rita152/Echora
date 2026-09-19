mod agent;
mod app;
mod apps;
mod components;
mod configuration;
mod conversation;
mod git_review;
mod i18n;
mod mcp;
mod media;
mod plugins;
mod pull_requests;
mod settings;
mod skills;
#[cfg(feature = "screenshot")]
mod stream_capture;
mod theme;
mod typography;
mod workspace;

use std::{borrow::Cow, fs, path::PathBuf};

#[cfg(feature = "screenshot")]
use std::path::Path;
#[cfg(feature = "screenshot")]
use std::time::{Duration, Instant};

#[cfg(feature = "screenshot")]
const RESUMED_THREAD_STABLE_FRAMES: usize = 3;
/// The chat search dialog reads real backend data, so its capture waits for a
/// few painted frames after the list settles.
#[cfg(feature = "screenshot")]
const CHAT_SEARCH_STABLE_FRAMES: usize = 4;

use anyhow::Result;
use gpui::{
    App, AppContext, AssetSource, Bounds, SharedString, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, px, size,
};
use gpui_platform::application;

use app::ChatApp;
use components::prompt_input::{
    Backspace, Copy, Cut, Delete, End, Home, Left, Paste, Right, SelectAll, SelectLeft,
    SelectRight, Submit,
};
use theme::ThemeMode;

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)] // objc 0.2 macros probe the legacy cargo-clippy cfg.
fn configure_native_blur_sampling(window: &mut gpui::Window) {
    fn schedule(window: &mut gpui::Window, attempts_remaining: usize) {
        window.on_next_frame(move |window, _| {
            let configured = unsafe { apply() };
            if !configured && attempts_remaining > 1 {
                schedule(window, attempts_remaining - 1);
            }
        });
    }

    #[allow(unexpected_cfgs)]
    unsafe fn apply() -> bool {
        use cocoa::{
            appkit::{
                NSView, NSViewHeightSizable, NSViewWidthSizable, NSVisualEffectBlendingMode,
                NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
            },
            base::{id, nil},
        };
        use objc::{class, msg_send, runtime::Object, sel, sel_impl};

        let application: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let mut native_window: *mut Object = msg_send![application, keyWindow];
        if native_window.is_null() {
            let windows: *mut Object = msg_send![application, windows];
            let count: usize = msg_send![windows, count];
            if count == 0 {
                return false;
            }
            native_window = msg_send![windows, objectAtIndex: 0usize];
        }

        let content_view: *mut Object = msg_send![native_window, contentView];
        let subviews: *mut Object = msg_send![content_view, subviews];
        let count: usize = msg_send![subviews, count];
        for index in 0..count {
            let view: *mut Object = msg_send![subviews, objectAtIndex: index];
            let is_visual_effect: bool = msg_send![view, isKindOfClass: class!(NSVisualEffectView)];
            if is_visual_effect {
                // Electron's primary ChatGPT window is transparent and calls
                // setVibrancy("menu"). GPUI's built-in BlurredView instead
                // uses Selection material and strips AppKit's tint and
                // saturation filters in updateLayer, so changing its material
                // in place still produces a visibly different light sidebar.
                // Keep GPUI's owned view alive but hidden, then install a plain
                // NSVisualEffectView with Electron's exact semantic material.
                unsafe {
                    let frame = NSView::bounds(content_view);
                    let menu_view: id =
                        NSVisualEffectView::initWithFrame_(NSVisualEffectView::alloc(nil), frame);
                    menu_view.setMaterial_(NSVisualEffectMaterial::Menu);
                    menu_view.setBlendingMode_(NSVisualEffectBlendingMode::BehindWindow);
                    menu_view.setState_(NSVisualEffectState::Active);
                    menu_view.setAutoresizingMask_(NSViewWidthSizable | NSViewHeightSizable);
                    let _: () = msg_send![
                        content_view,
                        addSubview: menu_view
                        positioned: -1isize
                        relativeTo: nil
                    ];
                    let _: id = msg_send![menu_view, autorelease];
                    let _: () = msg_send![view, setHidden: true];
                }
                return true;
            }
        }
        false
    }

    schedule(window, 8);
}

#[cfg(not(target_os = "macos"))]
fn configure_native_blur_sampling(_window: &mut gpui::Window) {}

struct Assets {
    base: PathBuf,
}

#[cfg(feature = "screenshot")]
fn save_screenshot(window: &gpui::Window, path: &str) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    window.render_to_image()?.save(path)?;
    Ok(())
}

#[cfg(feature = "screenshot")]
fn capture_frame(window: &mut gpui::Window, path: String, frames: usize) {
    window.on_next_frame(move |window, cx| {
        if frames>1 {window.refresh();capture_frame(window,path,frames-1);} else {
            match save_screenshot(window,&path) {
                Ok(())=>{
                    let metadata=serde_json::json!({"viewportWidth":f32::from(window.viewport_size().width),"viewportHeight":f32::from(window.viewport_size().height),"dpr":window.scale_factor(),"source":"GPUI render_to_image; no resizing or alignment","keyContexts":format!("{:?}",window.context_stack()),"approvalActionAvailable":window.is_action_available(&components::approval::ApprovalShortcut("escape"),cx)});
                    let _=std::fs::write(format!("{path}.render.json"),serde_json::to_vec_pretty(&metadata).unwrap());
                    println!("capture-frame: {path}");
                }
                Err(error)=>eprintln!("capture-frame failed: {error:#}"),
            }
        }
    });
    window.refresh();
}

#[cfg(feature = "screenshot")]
fn schedule_screenshot(window: &mut gpui::Window, path: String, frames: usize) {
    window.on_next_frame(move |window, cx| {
        if frames > 1 {
            // `on_next_frame` callbacks run before GPUI paints that frame.
            // Dirty each counted frame so the delay represents real draws.
            window.refresh();
            schedule_screenshot(window, path, frames - 1);
        } else {
            match save_screenshot(window, &path) {
                Ok(()) => println!("{path}"),
                Err(error) => {
                    eprintln!("failed to save screenshot: {error:#}");
                    std::process::exit(1);
                }
            }
            cx.quit();
        }
    });
}

/// Account surfaces wait for the real account read before capturing, so the
/// image shows the backend's account and quota rather than a pending state.
#[cfg(feature = "screenshot")]
fn schedule_account_screenshot(
    window: &mut gpui::Window,
    app: gpui::Entity<ChatApp>,
    path: String,
    deadline: Instant,
    stable: usize,
) {
    window.on_next_frame(
        move |window, cx| match app.read(cx).account_capture_ready() {
            true if stable == 0 => {
                match save_screenshot(window, &path) {
                    Ok(()) => println!("{path}"),
                    Err(error) => {
                        eprintln!("failed to save screenshot: {error:#}");
                        std::process::exit(1);
                    }
                }
                cx.quit();
            }
            ready if Instant::now() < deadline => {
                if ready {
                    window.refresh();
                }
                window.refresh();
                schedule_account_screenshot(
                    window,
                    app.clone(),
                    path.clone(),
                    deadline,
                    if ready { stable - 1 } else { stable },
                );
            }
            _ => {
                eprintln!("account surface did not resolve before the capture deadline");
                std::process::exit(1);
            }
        },
    );
}

/// Waits until the plugins management segments have loaded their backend data,
/// then saves the frame. A capture must never encode an empty list that only
/// reflects "not loaded yet".
#[cfg(feature = "screenshot")]
fn schedule_manage_screenshot(
    window: &mut gpui::Window,
    app: gpui::Entity<ChatApp>,
    path: String,
    deadline: Instant,
    stable_frames_remaining: usize,
) {
    window.on_next_frame(move |window, cx| {
        let ready = app.read(cx).manage_capture_ready(cx);
        if ready {
            if stable_frames_remaining > 0 {
                window.refresh();
                schedule_manage_screenshot(
                    window,
                    app,
                    path,
                    deadline,
                    stable_frames_remaining - 1,
                );
                return;
            }
            match save_screenshot(window, &path) {
                Ok(()) => println!("{path}"),
                Err(error) => {
                    eprintln!("failed to save screenshot: {error:#}");
                    std::process::exit(1);
                }
            }
            cx.quit();
            return;
        }
        if Instant::now() >= deadline {
            // Save what is on screen and report the incomplete wait instead of
            // leaving an unfinished capture behind.
            eprintln!("settings screenshot deadline reached; saving current frame");
            match save_screenshot(window, &path) {
                Ok(()) => println!("{path}"),
                Err(error) => {
                    eprintln!("failed to save screenshot: {error:#}");
                    std::process::exit(1);
                }
            }
            cx.quit();
            return;
        }
        window.refresh();
        schedule_manage_screenshot(window, app, path, deadline, stable_frames_remaining);
    });
}

#[cfg(feature = "screenshot")]
fn schedule_review_screenshot(
    window: &mut gpui::Window,
    app: gpui::Entity<ChatApp>,
    path: String,
    deadline: Instant,
    stable: usize,
) {
    window.on_next_frame(
        move |window, cx| match app.read(cx).review_capture_ready(cx) {
            Ok(true) if stable == 0 => {
                if let Err(error) = save_screenshot(window, &path) {
                    eprintln!("review screenshot failed: {error}");
                    std::process::exit(1);
                }
                println!("{path}");
                cx.quit();
            }
            Ok(ready) if Instant::now() < deadline => {
                window.refresh();
                schedule_review_screenshot(
                    window,
                    app,
                    path,
                    deadline,
                    if ready { stable.saturating_sub(1) } else { 3 },
                );
            }
            result => {
                eprintln!("review screenshot did not become ready: {result:?}");
                std::process::exit(1);
            }
        },
    );
}

/// Ready frames the Pull Requests capture waits for before saving.
#[cfg(feature = "screenshot")]
const PULL_REQUESTS_STABLE_FRAMES: usize = 90;

/// How often the capture re-activates its window while waiting for the page.
/// Activating on every frame fights whatever app the user is working in, so it
/// only happens while the window is not yet key.
#[cfg(feature = "screenshot")]
const PULL_REQUESTS_ACTIVATE_EVERY: usize = 30;

/// Keeps waiting for the Pull Requests page after a probe rejected the current
/// frame, until the capture deadline runs out.
#[cfg(feature = "screenshot")]
fn retry_pull_requests_screenshot(
    window: &mut gpui::Window,
    app: gpui::Entity<ChatApp>,
    path: String,
    deadline: Instant,
    frame: usize,
) {
    if Instant::now() < deadline {
        window.refresh();
        schedule_pull_requests_screenshot(
            window,
            app,
            path,
            deadline,
            PULL_REQUESTS_STABLE_FRAMES,
            frame + 1,
        );
    } else {
        eprintln!("pull requests window never painted the page");
        std::process::exit(1);
    }
}

#[cfg(feature = "screenshot")]
/// Waits until the Pull Requests page has finished loading, then saves the
/// window and exits. Used for the reference/GPUI pixel comparisons so captures
/// never depend on pointer position or timing.
#[cfg(feature = "screenshot")]
fn schedule_pull_requests_screenshot(
    window: &mut gpui::Window,
    app: gpui::Entity<ChatApp>,
    path: String,
    deadline: Instant,
    stable: usize,
    frame: usize,
) {
    if frame == 0 {
        // A window that never becomes visible stops delivering frames, so the
        // frame-driven deadline cannot fire. Watch the clock from a thread.
        let watchdog_deadline = deadline;
        let watchdog_path = path.clone();
        std::thread::spawn(move || {
            while Instant::now() < watchdog_deadline {
                std::thread::sleep(Duration::from_millis(250));
            }
            eprintln!("pull requests screenshot timed out: {watchdog_path}");
            std::process::exit(1);
        });
    }
    window.on_next_frame(move |window, cx| {
        if std::env::var("GPUI_PR_DEBUG").is_ok() {
            eprintln!(
                "PR-DEBUG frame={frame} active={} ready={} {}",
                window.is_window_active(),
                app.read(cx).pull_requests_capture_ready(cx),
                app.read(cx).pull_requests_diagnostics(cx)
            );
        }

        // A background launch can leave the window unpresented, and macOS only
        // makes a window key when the application itself is active. The capture
        // therefore counts ready frames whether or not the window is key, and
        // validates the rendered pixels below instead of trusting activation:
        // an unpresented window renders the flat grey startup view, which the
        // probe refuses to save.
        if app.read(cx).pull_requests_capture_ready(cx) {
            if stable == 0 {
                match window.render_to_image() {
                    Ok(image) => {
                        if let Ok(dump) = std::env::var("GPUI_PR_DUMP") {
                            let _ = image.save(format!("{dump}.probe.png"));
                        }
                        // An unpresented window renders a flat fill, so sample
                        // points across the whole frame and refuse to save a
                        // capture whose samples are all the same colour. A real
                        // page always differs between its panes.
                        let flat = {
                            let mut samples = Vec::new();
                            for (fx, fy) in
                                [(0.1, 0.1), (0.3, 0.5), (0.5, 0.25), (0.7, 0.7), (0.9, 0.9)]
                            {
                                let x = ((image.width() as f32) * fx) as u32;
                                let y = ((image.height() as f32) * fy) as u32;
                                samples.push(image.get_pixel(
                                    x.min(image.width() - 1),
                                    y.min(image.height() - 1),
                                ));
                            }
                            samples.windows(2).all(|pair| {
                                (0..3)
                                    .all(|channel| pair[0][channel].abs_diff(pair[1][channel]) <= 2)
                            })
                        };
                        if flat {
                            eprintln!("pull requests capture saw the startup view; waiting");
                            retry_pull_requests_screenshot(window, app, path, deadline, frame);
                            return;
                        }
                    }
                    Err(error) => {
                        eprintln!("pull requests render probe failed: {error}; waiting");
                        retry_pull_requests_screenshot(window, app, path, deadline, frame);
                        return;
                    }
                }
                if let Err(error) = save_screenshot(window, &path) {
                    eprintln!("pull requests screenshot failed: {error}");
                    std::process::exit(1);
                }
                let audit = serde_json::json!({
                    "viewportWidth": f32::from(window.viewport_size().width),
                    "viewportHeight": f32::from(window.viewport_size().height),
                    "dpr": window.scale_factor(),
                    "source": "GPUI render_to_image; no resizing or alignment",
                    "diagnostics": app.read(cx).pull_requests_diagnostics(cx),
                });
                let _ = std::fs::write(
                    format!("{path}.render.json"),
                    serde_json::to_vec_pretty(&audit).unwrap(),
                );
                println!("{path}");
                cx.quit();
            } else if Instant::now() < deadline {
                // Require the page to be ready on `stable + 1` consecutive
                // frames so the capture never saves a frame the window rendered
                // before the startup gate cleared.
                window.refresh();
                schedule_pull_requests_screenshot(
                    window,
                    app,
                    path,
                    deadline,
                    stable - 1,
                    frame + 1,
                );
            } else {
                eprintln!("pull requests screenshot did not stabilize");
                std::process::exit(1);
            }
            return;
        }
        if Instant::now() >= deadline {
            eprintln!("pull requests page did not become ready");
            std::process::exit(1);
        }
        // `render_to_image` rasterizes the last drawn scene, so the window must
        // actually be on screen and drawing. Bring it forward until it is key:
        // an occluded window never draws, and `render_to_image` only rasterizes
        // the last drawn scene, so the capture would otherwise save the startup
        // view. Re-activating every frame would keep stealing focus from
        // whatever the user is working in, so this backs off once the window is
        // key and only repeats periodically.
        let activate = !window.is_window_active();
        if activate && (frame.is_multiple_of(PULL_REQUESTS_ACTIVATE_EVERY) || frame < 2) {
            window.activate_window();
            cx.activate(true);
        }
        window.refresh();
        schedule_pull_requests_screenshot(
            window,
            app,
            path,
            deadline,
            stable,
            frame.wrapping_add(1),
        );
    });
}

#[cfg(feature = "screenshot")]
fn schedule_message_edit_screenshot(
    window: &mut gpui::Window,
    app: gpui::Entity<ChatApp>,
    path: String,
    phase: usize,
    stable_frames_remaining: usize,
) {
    window.on_next_frame(move |window, cx| {
        if phase == 0 {
            app.update(cx, |app, cx| app.capture_message_edit(cx));
            window.refresh();
            schedule_message_edit_screenshot(window, app, path, 1, stable_frames_remaining);
            return;
        }
        if stable_frames_remaining > 0 {
            window.refresh();
            schedule_message_edit_screenshot(window, app, path, phase, stable_frames_remaining - 1);
            return;
        }
        match save_screenshot(window, &path) {
            Ok(()) => println!("{path}"),
            Err(error) => {
                eprintln!("failed to save screenshot: {error:#}");
                std::process::exit(1);
            }
        }
        cx.quit();
    });
}

#[cfg(feature = "screenshot")]
#[allow(clippy::too_many_arguments)]
fn schedule_chat_search_screenshot(
    window: &mut gpui::Window,
    app: gpui::Entity<ChatApp>,
    path: String,
    state: String,
    query: Option<String>,
    index: Option<usize>,
    deadline: Instant,
    phase: usize,
    stable_frames_remaining: usize,
) {
    window.on_next_frame(move |window, cx| {
        if phase == 0 {
            match app.read(cx).chat_search_capture_ready(cx) {
                Err(error) => {
                    eprintln!("chat search capture failed: {error}");
                    std::process::exit(1);
                }
                Ok(true) => {
                    let state = state.clone();
                    let query = query.clone();
                    app.update(cx, |app, cx| {
                        app.capture_chat_search(&state, query.as_deref(), index, cx)
                    });
                    window.refresh();
                    schedule_chat_search_screenshot(
                        window, app, path, state, query, index, deadline, 1, 0,
                    );
                    return;
                }
                Ok(false) if Instant::now() < deadline => {
                    schedule_chat_search_screenshot(
                        window,
                        app,
                        path,
                        state,
                        query,
                        index,
                        deadline,
                        0,
                        stable_frames_remaining,
                    );
                    return;
                }
                Ok(false) => {
                    eprintln!("timed out waiting for chat search data");
                    std::process::exit(1);
                }
            }
        }
        if phase == 1 {
            // The query issued by the capture state is an asynchronous read;
            // wait for the dialog to leave its loading state before counting
            // stable frames.
            if app.read(cx).chat_search_capture_ready(cx).unwrap_or(false) {
                schedule_chat_search_screenshot(
                    window,
                    app,
                    path,
                    state,
                    query,
                    index,
                    deadline,
                    2,
                    CHAT_SEARCH_STABLE_FRAMES,
                );
            } else if Instant::now() < deadline {
                schedule_chat_search_screenshot(
                    window,
                    app,
                    path,
                    state,
                    query,
                    index,
                    deadline,
                    1,
                    stable_frames_remaining,
                );
            } else {
                eprintln!("timed out waiting for chat search results");
                std::process::exit(1);
            }
            return;
        }
        if stable_frames_remaining > 0 {
            window.refresh();
            schedule_chat_search_screenshot(
                window,
                app,
                path,
                state,
                query,
                index,
                deadline,
                2,
                stable_frames_remaining - 1,
            );
            return;
        }
        match save_screenshot(window, &path) {
            Ok(()) => println!("{path}"),
            Err(error) => {
                eprintln!("failed to save screenshot: {error:#}");
                std::process::exit(1);
            }
        }
        cx.quit();
    });
}

#[cfg(feature = "screenshot")]
fn schedule_resumed_thread_screenshot(
    window: &mut gpui::Window,
    app: gpui::Entity<ChatApp>,
    path: String,
    thread_id: String,
    deadline: Instant,
    scroll_from_bottom: Option<f32>,
    stable_frames_remaining: usize,
) {
    window.on_next_frame(move |window, cx| {
        let readiness = app.read(cx).resumed_thread_ready(&thread_id, cx);
        match readiness {
            Err(error) => {
                eprintln!("failed to load resumed thread {thread_id}: {error}");
                std::process::exit(1);
            }
            Ok(true) if scroll_from_bottom.is_some() => {
                let distance = scroll_from_bottom.expect("guarded capture scroll distance");
                app.update(cx, |app, cx| {
                    app.set_conversation_scroll_from_bottom_for_capture(distance, cx)
                });
                window.refresh();
                schedule_resumed_thread_screenshot(
                    window,
                    app,
                    path,
                    thread_id,
                    deadline,
                    None,
                    RESUMED_THREAD_STABLE_FRAMES,
                );
            }
            Ok(true) if stable_frames_remaining > 0 => {
                // Readiness can flip in this callback, before that frame is
                // painted. Force a draw for every counted stable frame.
                window.refresh();
                schedule_resumed_thread_screenshot(
                    window,
                    app,
                    path,
                    thread_id,
                    deadline,
                    scroll_from_bottom,
                    stable_frames_remaining - 1,
                );
            }
            Ok(true) => {
                match save_screenshot(window, &path) {
                    Ok(()) => println!("{path}"),
                    Err(error) => {
                        eprintln!("failed to save screenshot: {error:#}");
                        std::process::exit(1);
                    }
                }
                let audit = app.read(cx).resumed_render_audit(&thread_id, cx);
                if let Err(error) = std::fs::write(
                    format!("{path}.render.json"),
                    serde_json::to_vec_pretty(&audit).expect("capture audit is JSON"),
                ) {
                    eprintln!("failed to save resumed render audit: {error}");
                    std::process::exit(1);
                }
                cx.quit();
            }
            Ok(false) if Instant::now() < deadline => schedule_resumed_thread_screenshot(
                window,
                app,
                path,
                thread_id,
                deadline,
                scroll_from_bottom,
                RESUMED_THREAD_STABLE_FRAMES,
            ),
            Ok(false) => {
                eprintln!("timed out waiting for resumed thread {thread_id}");
                std::process::exit(1);
            }
        }
    });
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        fs::read(self.base.join(path))
            .map(Cow::Owned)
            .map(Some)
            .map_err(Into::into)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(fs::read_dir(self.base.join(path))?
            .filter_map(|entry| {
                entry
                    .ok()?
                    .file_name()
                    .into_string()
                    .ok()
                    .map(SharedString::from)
            })
            .collect())
    }
}

fn normalize_resume_thread_id(value: &str) -> String {
    value.strip_prefix("local:").unwrap_or(value).to_owned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let language = args.iter().find_map(|arg| arg.strip_prefix("--language="));
    let language = match language {
        Some(value) => i18n::Language::from_name(value).unwrap_or_else(|| {
            eprintln!("Unsupported language '{value}'. Use auto, en, or zh-CN.");
            std::process::exit(2);
        }),
        None => workspace::preferred_language(),
    };
    i18n::set_language(language);
    let mode = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--theme=").map(ThemeMode::from_name))
        .unwrap_or(ThemeMode::Dark);
    let screenshot_path = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--screenshot=").map(ToOwned::to_owned));
    let screenshot_frames = args
        .iter()
        .find_map(|arg| {
            arg.strip_prefix("--screenshot-frames=")?
                .parse::<usize>()
                .ok()
        })
        .unwrap_or(2);
    let resume_thread = args.iter().find_map(|arg| {
        arg.strip_prefix("--resume-thread=")
            .map(normalize_resume_thread_id)
    });
    let resume_scroll_from_bottom = args.iter().find_map(|arg| {
        arg.strip_prefix("--resume-scroll-from-bottom=")?
            .parse::<f32>()
            .ok()
    });
    let chat_search_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--chat-search-state=")
            .map(ToOwned::to_owned)
    });
    let chat_search_query = args.iter().find_map(|arg| {
        arg.strip_prefix("--chat-search-query=")
            .map(ToOwned::to_owned)
    });
    let chat_search_index = args.iter().find_map(|arg| {
        arg.strip_prefix("--chat-search-index=")
            .and_then(|value| value.parse::<usize>().ok())
    });
    #[cfg(not(feature = "screenshot"))]
    let _ = (chat_search_state, chat_search_query, chat_search_index);
    #[cfg(not(feature = "screenshot"))]
    let _ = (screenshot_frames, resume_scroll_from_bottom);
    let submit_prompt = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--submit-prompt=").map(ToOwned::to_owned));
    let command_tool_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--command-tool-state=")
            .map(ToOwned::to_owned)
    });
    let command_tool_expanded = args.iter().any(|arg| arg == "--command-tool-expanded");
    let tool_group_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--tool-group-state=")
            .map(ToOwned::to_owned)
    });
    let tool_group_expanded = args.iter().any(|arg| arg == "--tool-group-expanded");
    let reasoning_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--reasoning-ui-state=")
            .map(ToOwned::to_owned)
    });
    let reasoning_ui_expanded = args.iter().any(|arg| arg == "--reasoning-ui-expanded");
    let approval_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--approval-ui-state=")
            .map(ToOwned::to_owned)
    });
    #[cfg(feature = "screenshot")]
    let approval_replay = args.iter().find_map(|arg| {
        arg.strip_prefix("--approval-replay=")
            .map(std::path::PathBuf::from)
    });
    let user_input_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--user-input-ui-state=")
            .map(ToOwned::to_owned)
    });
    let mcp_elicitation_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--mcp-elicitation-ui-state=")
            .map(ToOwned::to_owned)
    });
    let file_approval_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--file-approval-ui-state=")
            .map(ToOwned::to_owned)
    });
    let permissions_approval_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--permissions-approval-ui-state=")
            .map(ToOwned::to_owned)
    });
    let permissions_approval_ui_kind = args
        .iter()
        .find_map(|arg| {
            arg.strip_prefix("--permissions-approval-ui-kind=")
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "network".to_owned());
    let file_change_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--file-change-ui-state=")
            .map(ToOwned::to_owned)
    });
    let turn_diff_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--turn-diff-ui-state=")
            .map(ToOwned::to_owned)
    });
    let context_compaction_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--context-compaction-ui-state=")
            .map(ToOwned::to_owned)
    });
    let message_edit_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--message-edit-state=")
            .map(ToOwned::to_owned)
    });
    #[cfg(not(feature = "screenshot"))]
    let _ = message_edit_state;
    let collaboration_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--collaboration-ui-state=")
            .map(ToOwned::to_owned)
    });
    let mcp_tool_call_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--mcp-tool-call-ui-state=")
            .map(ToOwned::to_owned)
    });
    let dynamic_tool_call_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--dynamic-tool-call-ui-state=")
            .map(ToOwned::to_owned)
    });
    #[cfg(feature = "screenshot")]
    let progress_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--progress-ui-state=")
            .map(ToOwned::to_owned)
    });
    #[cfg(feature = "screenshot")]
    let runtime_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--runtime-ui-state=")
            .map(ToOwned::to_owned)
    });
    let image_generation_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--image-generation-ui-state=")
            .map(ToOwned::to_owned)
    });
    let image_generation_path = args.iter().find_map(|arg| {
        arg.strip_prefix("--image-generation-path=")
            .map(PathBuf::from)
    });
    let approval_ui_kind = args
        .iter()
        .find_map(|arg| {
            arg.strip_prefix("--approval-ui-kind=")
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "command".to_owned());
    let user_message_actions_visible = args
        .iter()
        .any(|arg| arg == "--user-message-actions-visible");
    #[cfg(not(feature = "screenshot"))]
    if screenshot_path.is_some() {
        eprintln!(
            "--screenshot requires the screenshot feature; rebuild with \
             `cargo run --release --features screenshot -- --screenshot=<path>`"
        );
        std::process::exit(2);
    }
    let start_maximized = args.iter().any(|arg| arg == "--maximized");
    let maximize_after_open = args.iter().any(|arg| arg == "--maximize-after-open");
    let window_width = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--window-width=")?.parse::<f32>().ok())
        .unwrap_or(1440.0);
    let window_height = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--window-height=")?.parse::<f32>().ok())
        .unwrap_or(900.0);
    let sidebar_bottom = args.iter().any(|arg| arg == "--sidebar-bottom");
    let profile_menu_open = args.iter().any(|arg| arg == "--profile-menu-open");
    let account_dialog = args.iter().find_map(|arg| {
        arg.strip_prefix("--account-dialog=")
            .map(|value| value == "login")
    });
    let bottom_panel_open = args.iter().any(|arg| arg == "--bottom-panel-open");
    let bottom_panel_menu_open = args.iter().any(|arg| arg == "--bottom-panel-menu-open");
    let bottom_panel_append = args.iter().find_map(|arg| {
        arg.strip_prefix("--bottom-panel-append=")
            .map(ToOwned::to_owned)
    });
    let right_panel_open = args.iter().any(|arg| arg == "--right-panel-open");
    let projects_menu_open = args.iter().any(|arg| arg == "--projects-menu-open");
    let project_menu_open = args.iter().find_map(|arg| {
        arg.strip_prefix("--project-menu-open=")
            .map(ToOwned::to_owned)
    });
    let project_create_open = args.iter().any(|arg| arg == "--project-create-open");
    let project_create_remote = args.iter().any(|arg| arg == "--project-create-remote");
    let activity_open = args.iter().any(|arg| arg == "--activity-open");
    let activity_scroll = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--activity-scroll=")?.parse::<f32>().ok());
    let activity_hover_recent = args.iter().find_map(|arg| {
        arg.strip_prefix("--activity-hover-recent=")
            .map(ToOwned::to_owned)
    });
    let model_picker_open = args.iter().any(|arg| arg == "--model-picker-open");
    let model_picker_submenu = args.iter().find_map(|arg| {
        arg.strip_prefix("--model-picker-submenu=")
            .map(ToOwned::to_owned)
    });
    let model_picker_slider_index = args.iter().find_map(|arg| {
        arg.strip_prefix("--model-picker-slider-index=")?
            .parse::<usize>()
            .ok()
    });
    let model_picker_slider_fast = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--model-picker-slider-speed="))
        .is_some_and(|speed| speed == "fast");
    let dictation_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--dictation-state=")
            .map(ToOwned::to_owned)
    });
    let permission_mode = args.iter().find_map(|arg| {
        arg.strip_prefix("--permission-mode=")
            .map(ToOwned::to_owned)
    });
    let permission_menu_open = args.iter().any(|arg| arg == "--permission-menu-open");
    let permission_menu_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--permission-menu-state=")
            .map(ToOwned::to_owned)
    });
    let permission_confirmation_open = args
        .iter()
        .any(|arg| arg == "--permission-confirmation-open");
    // Permission controls are visible in ordinary product launches. These
    // flags only select deterministic modes/states for visual capture; pixel
    // similarity is not a production visibility gate.
    let permission_ui_capture = screenshot_path.is_some()
        || permission_mode.is_some()
        || permission_menu_open
        || permission_menu_state.is_some()
        || permission_confirmation_open;
    let settings_open = args.iter().any(|arg| arg == "--settings-open");
    // Deterministic entry for the Pull Requests page: `--pull-requests` opens the
    // page on launch, and `--pull-requests-select=N` selects the Nth visible row
    // so captures do not depend on pointer races.
    let pull_requests_open = args.iter().any(|arg| arg == "--pull-requests");
    let pull_requests_select = args.iter().find_map(|arg| {
        arg.strip_prefix("--pull-requests-select=")
            .and_then(|value| value.parse::<usize>().ok())
    });
    let pull_requests_title = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--pull-requests-title="))
        .map(ToOwned::to_owned);
    let pull_requests_tab = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--pull-requests-tab="))
        .map(ToOwned::to_owned);
    let pull_requests_file_tree = args.iter().any(|arg| arg == "--pull-requests-file-tree");
    let pull_requests_status = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--pull-requests-status="))
        .map(ToOwned::to_owned);
    let pull_requests_list_tab = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--pull-requests-list-tab="))
        .map(ToOwned::to_owned);
    let pull_requests_search = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--pull-requests-search="))
        .map(ToOwned::to_owned);
    let pull_requests_collapsed_group = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--pull-requests-collapse-group="))
        .map(ToOwned::to_owned);
    let pull_requests_action = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--pull-requests-action="))
        .map(ToOwned::to_owned);
    let pull_requests_comment_menu = args.iter().any(|arg| arg == "--pull-requests-comment-menu");
    let pull_requests_scroll = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--pull-requests-scroll="))
        .and_then(|value| value.parse::<f32>().ok());
    let mut account_ready_capture = false;
    let settings_page = args.iter().find_map(|arg| {
        arg.strip_prefix("--settings-page=")
            .map(|slug| Box::leak(slug.to_owned().into_boxed_str()) as &'static str)
    });
    // Capture-only selectors for the plugins page segments and management
    // states. They render deterministic fixtures for visual verification.
    let plugins_segment = args.iter().find_map(|arg| {
        arg.strip_prefix("--plugins-segment=")
            .map(ToOwned::to_owned)
    });
    let mcp_detail = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--mcp-detail=").map(ToOwned::to_owned));
    let mcp_login_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--mcp-login-state=")
            .map(ToOwned::to_owned)
    });
    let mcp_reload_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--mcp-reload-state=")
            .map(ToOwned::to_owned)
    });
    let mcp_hover_row = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--mcp-hover-row=").map(ToOwned::to_owned));
    let skills_hover_row = args.iter().find_map(|arg| {
        arg.strip_prefix("--skills-hover-row=")
            .map(ToOwned::to_owned)
    });

    #[cfg(feature = "screenshot")]
    if components::auto_approval::capture_auto_approval(&args) {
        return;
    }
    #[cfg(feature = "screenshot")]
    if components::markdown::capture_markdown(&args) {
        return;
    }
    #[cfg(feature = "screenshot")]
    if typography::capture_specimen(&args) {
        return;
    }
    typography::configure();
    application()
        .with_assets(Assets {
            base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        })
        .run(move |cx: &mut App| {
            typography::initialize_fonts(cx);
            #[cfg(feature = "screenshot")]
            cx.bind_keys([gpui::KeyBinding::new(
                "cmd-shift-f12",
                app::CaptureFrame,
                None,
            )]);
            cx.bind_keys([
                gpui::KeyBinding::new("ctrl-`", app::ToggleTerminal, None),
                gpui::KeyBinding::new("ctrl-shift-g", app::ToggleReview, None),
                gpui::KeyBinding::new("cmd-alt-s", app::OpenSideChat, None),
                gpui::KeyBinding::new("cmd-,", app::OpenSettingsPage, None),
                gpui::KeyBinding::new("tab", app::NextSettingsControl, Some("Settings")),
                gpui::KeyBinding::new("shift-tab", app::PreviousSettingsControl, Some("Settings")),
            ]);
            cx.set_window_appearance(Some(match mode {
                ThemeMode::Light => WindowAppearance::Light,
                ThemeMode::Dark => WindowAppearance::Dark,
            }));
            cx.bind_keys([gpui::KeyBinding::new(
                "escape",
                app::DismissPermissionUi,
                None,
            )]);
            cx.bind_keys([
                gpui::KeyBinding::new("backspace", Backspace, Some("PromptInput")),
                gpui::KeyBinding::new("delete", Delete, Some("PromptInput")),
                gpui::KeyBinding::new("left", Left, Some("PromptInput")),
                gpui::KeyBinding::new("right", Right, Some("PromptInput")),
                gpui::KeyBinding::new("shift-left", SelectLeft, Some("PromptInput")),
                gpui::KeyBinding::new("shift-right", SelectRight, Some("PromptInput")),
                gpui::KeyBinding::new("cmd-a", SelectAll, Some("PromptInput")),
                gpui::KeyBinding::new("cmd-v", Paste, Some("PromptInput")),
                gpui::KeyBinding::new("cmd-c", Copy, Some("PromptInput")),
                gpui::KeyBinding::new("cmd-x", Cut, Some("PromptInput")),
                gpui::KeyBinding::new("home", Home, Some("PromptInput")),
                gpui::KeyBinding::new("end", End, Some("PromptInput")),
                gpui::KeyBinding::new("enter", Submit, Some("PromptInput")),
            ]);
            // Register terminal bindings after the application-wide Escape fallback.
            components::terminal::init(cx);
            components::file_editor::init(cx);
            components::auto_approval::init(cx);
            components::side_chat::init(cx);
            // GPUI resolves equal keystrokes in reverse registration order.
            components::approval::init(cx);
            components::home::init_runtime_keyboard(cx);
            cx.bind_keys([gpui::KeyBinding::new("cmd-p", app::OpenFiles, None)]);
            let bounds = Bounds::centered(None, size(px(window_width), px(window_height)), cx);
            let initial_bounds = if start_maximized {
                WindowBounds::Maximized(bounds)
            } else {
                WindowBounds::Windowed(bounds)
            };
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(initial_bounds),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Codex".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(gpui::point(px(18.0), px(18.0))),
                    }),
                    // Let the native macOS visual-effect layer participate in
                    // the translucent sidebar composition. Opaque main-pane
                    // content still masks the material on the right.
                    window_background: WindowBackgroundAppearance::Blurred,
                    window_min_size: Some(size(px(960.0), px(620.0))),
                    ..Default::default()
                },
                move |window, cx| {
                    configure_native_blur_sampling(window);
                    if maximize_after_open {
                        window.on_next_frame(|window, _| window.zoom_window());
                    }
                    let app = cx.new(|cx| {
                        cx.observe_window_activation(window, |_, _, cx| cx.notify())
                            .detach();
                        cx.observe_window_bounds(window, |_, _, cx| cx.notify())
                            .detach();
                        let mut app = ChatApp::new(mode, sidebar_bottom, cx);
                        if permission_ui_capture {
                            app.enable_permission_ui_for_capture(cx);
                        }
                        if approval_ui_state.is_some()
                            || tool_group_state.is_some()
                            || reasoning_ui_state.is_some()
                            || user_input_ui_state.is_some()
                            || file_approval_ui_state.is_some()
                            || permissions_approval_ui_state.is_some()
                            || file_change_ui_state.is_some()
                            || turn_diff_ui_state.is_some()
                            || context_compaction_ui_state.is_some()
                            || collaboration_ui_state.is_some()
                            || mcp_tool_call_ui_state.is_some()
                            || dynamic_tool_call_ui_state.is_some()
                            || mcp_elicitation_ui_state.is_some()
                            || plugins_segment.is_some()
                            || mcp_detail.is_some()
                            || mcp_login_state.is_some()
                            || mcp_reload_state.is_some()
                            || image_generation_ui_state.is_some()
                            || permission_mode.is_some()
                            || permission_menu_open
                            || permission_menu_state.is_some()
                            || permission_confirmation_open
                            || settings_page.is_some()
                        {
                            app.complete_startup_for_capture(cx);
                        }
                        if profile_menu_open {
                            // The account surfaces are a capture state too: the
                            // window must not still be showing the startup gate.
                            app.complete_startup_for_capture(cx);
                            app.open_profile_menu(cx);
                        }
                        if let Some(login) = account_dialog {
                            eprintln!("DEBUG account-dialog capture: login={login}");
                            app.complete_startup_for_capture(cx);
                            account_ready_capture = true;
                            app.open_account_dialog_for_capture(
                                if login {
                                    crate::components::account::AccountDialog::Login
                                } else {
                                    crate::components::account::AccountDialog::Logout
                                },
                                cx,
                            );
                        }
                        if bottom_panel_open {
                            app.open_bottom_panel(cx);
                        }
                        if bottom_panel_menu_open {
                            app.open_bottom_panel_menu(cx);
                        }
                        if let Some(name) = bottom_panel_append.as_deref() {
                            app.append_bottom_panel_item_for_capture(name, cx);
                        }
                        #[cfg(feature = "screenshot")]
                        if let Some(root) = args
                            .iter()
                            .find_map(|a| a.strip_prefix("--file-panel-root="))
                        {
                            let path = args
                                .iter()
                                .find_map(|a| a.strip_prefix("--open-file="))
                                .map(PathBuf::from);
                            app.capture_files(PathBuf::from(root), path, cx);
                        }
                        if right_panel_open {
                            app.open_right_panel(cx);
                        }
                        if projects_menu_open {
                            app.open_projects_section_menu(cx);
                        }
                        if let Some(project_id) = project_menu_open {
                            app.open_project_menu_for_capture(project_id, cx);
                        }
                        if project_create_open {
                            app.open_project_creation(cx);
                        }
                        if project_create_remote {
                            app.open_project_creation_remote_for_capture(cx);
                        }
                        if activity_open {
                            app.open_activity(cx);
                        }
                        if let Some(offset) = activity_scroll {
                            app.open_activity(cx);
                            app.set_activity_scroll_for_capture(offset, cx);
                        }
                        if let Some(thread_id) = activity_hover_recent {
                            app.open_activity(cx);
                            app.set_activity_hovered_thread_for_capture(thread_id, cx);
                        }
                        if model_picker_open {
                            app.open_model_picker(cx);
                        }
                        if let Some(submenu) = model_picker_submenu.as_deref() {
                            app.open_model_picker_submenu(submenu, cx);
                        }
                        if let Some(index) = model_picker_slider_index {
                            app.open_model_picker_slider_at(index, model_picker_slider_fast, cx);
                        }
                        if let Some(state) = dictation_state.as_deref() {
                            app.set_dictation_state_for_capture(state, cx);
                        }
                        if let Some(mode) = permission_mode.as_deref() {
                            app.set_permission_mode_for_capture(mode, cx);
                        }
                        if let Some(state) = permission_menu_state.as_deref() {
                            app.set_permission_menu_capture_state(state, cx);
                        } else if permission_menu_open {
                            app.open_permission_menu_for_capture(cx);
                        }
                        if permission_confirmation_open {
                            app.open_permission_confirmation_for_capture(cx);
                        }
                        if let Some(prompt) = submit_prompt.as_deref() {
                            app.submit_prompt_for_capture(prompt, cx);
                        }
                        if user_message_actions_visible {
                            app.show_user_message_actions_for_capture(cx);
                        }
                        if let Some(state) = command_tool_state.as_deref() {
                            app.set_command_tool_for_capture(
                                state.eq_ignore_ascii_case("running"),
                                command_tool_expanded,
                                cx,
                            );
                        }
                        if let Some(state) = context_compaction_ui_state.as_deref() {
                            app.set_context_compaction_for_capture(
                                state.eq_ignore_ascii_case("running"),
                                cx,
                            );
                        }
                        if let Some(state) = collaboration_ui_state.as_deref() {
                            app.set_collaboration_for_capture(state, cx);
                        }
                        if let Some(state) = mcp_tool_call_ui_state.as_deref() {
                            app.set_mcp_tool_call_for_capture(state, cx);
                        }
                        if let Some(state) = dynamic_tool_call_ui_state.as_deref() {
                            app.set_dynamic_tool_call_for_capture(state, cx);
                        }
                        #[cfg(feature = "screenshot")]
                        if let Some(state) = progress_ui_state.as_deref() {
                            app.complete_startup_for_capture(cx);
                            app.set_progress_for_capture(state, cx);
                        }
                        #[cfg(feature = "screenshot")]
                        if let Some(state) = runtime_ui_state.as_deref() {
                            app.complete_startup_for_capture(cx);
                            app.set_runtime_for_capture(state, cx);
                        }
                        if let Some(state) = image_generation_ui_state.as_deref() {
                            app.set_image_generation_for_capture(
                                state,
                                image_generation_path.clone(),
                                cx,
                            );
                        }
                        if let Some(state) = tool_group_state.as_deref() {
                            app.set_tool_group_for_capture(
                                state.eq_ignore_ascii_case("running"),
                                tool_group_expanded,
                                cx,
                            );
                        }
                        if let Some(state) = reasoning_ui_state.as_deref() {
                            app.set_reasoning_for_capture(state, reasoning_ui_expanded, cx);
                        }
                        if let Some(state) = approval_ui_state.as_deref() {
                            app.set_approval_for_capture(&approval_ui_kind, state, cx);
                        }
                        #[cfg(feature = "screenshot")]
                        if let Some(path) = &approval_replay {
                            app.replay_approvals(path, cx);
                        }
                        if let Some(state) = user_input_ui_state.as_deref() {
                            app.set_user_input_for_capture(state, cx);
                        }
                        if let Some(state) = mcp_elicitation_ui_state.as_deref() {
                            app.set_mcp_elicitation_for_capture(state, cx);
                        }
                        if let Some(state) = file_approval_ui_state.as_deref() {
                            app.set_file_approval_for_capture(state, cx);
                        }
                        if let Some(state) = permissions_approval_ui_state.as_deref() {
                            app.set_permissions_approval_for_capture(
                                &permissions_approval_ui_kind,
                                state,
                                cx,
                            );
                        }
                        if let Some(state) = file_change_ui_state.as_deref() {
                            app.set_file_change_for_capture(state, cx);
                        }
                        if let Some(state) = turn_diff_ui_state.as_deref() {
                            app.set_turn_diff_for_capture(state, cx);
                        }
                        if let Some(slug) = settings_page {
                            app.open_settings_page(slug, cx);
                        } else if settings_open {
                            app.open_settings(cx);
                        }
                        if pull_requests_open {
                            app.complete_startup_for_capture(cx);
                            app.open_pull_requests_for_capture(cx);
                            if let Some(status) = pull_requests_status.as_deref() {
                                let filter = match status {
                                    "merged" => crate::pull_requests::StatusFilter::Merged,
                                    "closed" => crate::pull_requests::StatusFilter::Closed,
                                    "all" => crate::pull_requests::StatusFilter::All,
                                    _ => crate::pull_requests::StatusFilter::Open,
                                };
                                app.set_pull_requests_status_filter(filter, cx);
                            }
                            if let Some(tab) = pull_requests_list_tab.as_deref() {
                                app.set_pull_requests_list_tab(tab, cx);
                            }
                            if let Some(query) = pull_requests_search.as_deref() {
                                app.set_pull_requests_search(query, cx);
                            }
                            if let Some(group) = pull_requests_collapsed_group.as_deref() {
                                app.collapse_pull_requests_group(group, cx);
                            }
                            if let Some(tab) = pull_requests_tab.clone() {
                                app.open_pull_requests_tab(&tab, cx);
                            }
                            if let Some(offset) = pull_requests_scroll {
                                app.scroll_pull_requests_detail(offset, cx);
                            }
                            if pull_requests_comment_menu {
                                app.open_pull_requests_action("comment-menu", cx);
                            }
                            if let Some(action) = pull_requests_action.as_deref() {
                                app.open_pull_requests_action(action, cx);
                            }
                            if pull_requests_file_tree {
                                app.set_pull_requests_file_tree(true, cx);
                            }
                            if let Some(index) = pull_requests_select {
                                app.select_pull_request_index(index, cx);
                            }
                            if let Some(title) = pull_requests_title.as_deref() {
                                app.select_pull_request_title(title, cx);
                            }
                        }
                        app.apply_manage_capture_flags(
                            plugins_segment.as_deref(),
                            mcp_detail.as_deref(),
                            mcp_login_state.as_deref(),
                            mcp_reload_state.as_deref(),
                            mcp_hover_row.as_deref(),
                            skills_hover_row.as_deref(),
                            cx,
                        );
                        if let Some(thread_id) = resume_thread.as_deref() {
                            app.resume_thread_for_capture(thread_id.to_owned(), cx);
                        }
                        #[cfg(feature = "screenshot")]
                        if let Some(root) =
                            args.iter().find_map(|a| a.strip_prefix("--review-root="))
                        {
                            app.capture_review(PathBuf::from(root), cx);
                        }
                        #[cfg(feature = "screenshot")]
                        if let Some(query) =
                            args.iter().find_map(|a| a.strip_prefix("--review-filter="))
                        {
                            app.capture_review_filter(query, cx);
                        }
                        app
                    });
                    #[cfg(feature = "screenshot")]
                    if let Some(path) = args
                        .iter()
                        .find_map(|arg| arg.strip_prefix("--capture-live-turn="))
                    {
                        crate::stream_capture::schedule(window, app.clone(), path.to_owned(), cx);
                    }
                    let closing_app = app.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        closing_app
                            .update(cx, |app, cx| app.request_window_close(window, cx))
                            .unwrap_or(true)
                    });
                    #[cfg(feature = "screenshot")]
                    if let Some(path) = screenshot_path.clone() {
                        if plugins_segment.is_some() {
                            schedule_manage_screenshot(
                                window,
                                app.clone(),
                                path,
                                Instant::now() + Duration::from_secs(45),
                                RESUMED_THREAD_STABLE_FRAMES,
                            );
                        } else if args.iter().any(|a| a.starts_with("--review-root=")) {
                            schedule_review_screenshot(
                                window,
                                app.clone(),
                                path,
                                Instant::now() + Duration::from_secs(60),
                                3,
                            );
                        } else if profile_menu_open || account_ready_capture {
                            schedule_account_screenshot(
                                window,
                                app.clone(),
                                path,
                                Instant::now() + Duration::from_secs(45),
                                3,
                            );
                        } else if let Some(state) = chat_search_state.clone() {
                            schedule_chat_search_screenshot(
                                window,
                                app.clone(),
                                path,
                                state,
                                chat_search_query.clone(),
                                chat_search_index,
                                Instant::now() + Duration::from_secs(60),
                                0,
                                CHAT_SEARCH_STABLE_FRAMES,
                            );
                        } else if message_edit_state.is_some() {
                            schedule_message_edit_screenshot(
                                window,
                                app.clone(),
                                path,
                                0,
                                CHAT_SEARCH_STABLE_FRAMES,
                            );
                        } else if pull_requests_open {
                            // The page can be model-ready before the window has
                            // painted its first real frame; hold the capture for
                            // about a second of ready frames.
                            schedule_pull_requests_screenshot(
                                window,
                                app.clone(),
                                path,
                                Instant::now() + Duration::from_secs(60),
                                PULL_REQUESTS_STABLE_FRAMES,
                                0,
                            );
                        } else if let Some(thread_id) = resume_thread.clone() {
                            schedule_resumed_thread_screenshot(
                                window,
                                app.clone(),
                                path,
                                thread_id,
                                Instant::now() + Duration::from_secs(60),
                                resume_scroll_from_bottom,
                                RESUMED_THREAD_STABLE_FRAMES,
                            );
                        } else {
                            schedule_screenshot(
                                window,
                                path,
                                if maximize_after_open {
                                    screenshot_frames.max(90)
                                } else {
                                    screenshot_frames
                                },
                            );
                        }
                    }
                    app
                },
            )
            .expect("failed to open Codex window");
            cx.activate(true);
        });
}

#[cfg(test)]
mod tests {
    use super::normalize_resume_thread_id;

    #[test]
    fn resume_thread_id_accepts_chatgpt_sidebar_identity() {
        assert_eq!(
            normalize_resume_thread_id("local:01a06013-458c-7733-8aea-4f36df979feb"),
            "01a06013-458c-7733-8aea-4f36df979feb"
        );
        assert_eq!(
            normalize_resume_thread_id("01a06013-458c-7733-8aea-4f36df979feb"),
            "01a06013-458c-7733-8aea-4f36df979feb"
        );
    }
}
