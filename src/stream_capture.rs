//! Opt-in native capture of a real live turn, before any history reload.
use std::time::{Duration, Instant};

use crate::{app::ChatApp, conversation::ConversationPhase};
use gpui::{Entity, Window};

pub(crate) fn schedule(window: &mut Window, app: Entity<ChatApp>, path: String, cx: &gpui::App) {
    window.spawn(cx, async move |cx| {
        let deadline = Instant::now() + Duration::from_secs(300);
        let mut stable = 0;
        loop {
            cx.background_executor().timer(Duration::from_millis(16)).await;
            let finished = cx.update(|window, cx| {
                let (phase, thread_id) = app.read(cx).live_capture_status(cx);
                if phase == ConversationPhase::Complete { stable += 1; } else { stable = 0; }
                if stable >= 3 {
                    let Some(thread_id) = thread_id else { eprintln!("live capture completed without thread identity"); std::process::exit(1); };
                    // macOS may pause display-link callbacks for an occluded
                    // verification window. Render the current native scene at
                    // its original size; do not rely on a stale presented frame.
                    window.refresh();
                    let arena = window.draw(cx);
                    arena.clear(cx);
                    if let Err(error) = crate::save_screenshot(window, &path) {
                        eprintln!("live capture failed: {error:#}"); std::process::exit(1);
                    }
                    let metadata = serde_json::json!({
                        "source": "live app-server turn; GPUI render_to_image; no history reload or image transforms",
                        "viewportWidth": f32::from(window.viewport_size().width),
                        "viewportHeight": f32::from(window.viewport_size().height), "dpr": window.scale_factor(),
                        "audit": app.read(cx).resumed_render_audit(&thread_id, cx),
                    });
                    std::fs::write(format!("{path}.json"), serde_json::to_vec_pretty(&metadata).unwrap()).expect("write live capture metadata");
                    println!("{path}");
                    cx.quit();
                    true
                } else if Instant::now() >= deadline || matches!(phase, ConversationPhase::Failed | ConversationPhase::Stopped) {
                    eprintln!("live capture did not complete: {phase:?}");
                    std::process::exit(1);
                } else { false }
            }).unwrap_or(true);
            if finished { return; }
        }
    }).detach();
}
