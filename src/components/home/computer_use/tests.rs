use super::*;
use gpui::{AppContext, Bounds, MouseButton, TestApp, WindowBounds, WindowOptions, point, size};
fn snapshot() -> Snapshot {
    Snapshot {
        target: None,
        targets: vec![],
        label: "Verification target".into(),
        title: "Observe window".into(),
        status: AgentMcpToolCallStatus::InProgress,
        progress: None,
        error: None,
        frame: None,
    }
}
fn info(id: u32) -> WindowInfo {
    WindowInfo {
        id: WindowId {
            process: 123,
            window: id,
        },
        target: computer_use::Target::AppId("com.test".into()),
        title: format!("Window {id}"),
        application: "Test".into(),
        width: 420.,
        height: 312.,
    }
}
struct PreviewHost(Entity<Preview>);
impl Render for PreviewHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full().child(self.0.clone())
    }
}
#[test]
fn collapse_and_keyboard_release_activate_once() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(800.), px(600.)),
            ))),
            ..Default::default()
        },
        |_, cx| {
            let mut p = Preview::new(ThemeMode::Dark);
            p.snapshot = Some(snapshot());
            p.phase = ConversationPhase::Streaming;
            p.seen_running = true;
            p.owner = Some((cx.new(|cx| ComposerView::new(ThemeMode::Dark, cx)), 0));
            p.windows = vec![WindowPreview::new(info(1)), WindowPreview::new(info(2))];
            PreviewHost(cx.new(|_| p))
        },
    );
    window.draw();
    window.simulate_click(point(px(614.), px(84.)), MouseButton::Left);
    window.draw();
    window.read(|h, cx| {
        assert!(!h.0.read(cx).expanded);
        assert!(h.0.read(cx).takeover_pending.is_none());
    });
    for (key, expected) in [("enter", true), ("space", false)] {
        window.simulate_keystroke(key);
        window.update(|_, window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::KeyUp(gpui::KeyUpEvent {
                    keystroke: gpui::Keystroke::parse(key).unwrap(),
                }),
                cx,
            )
        });
        window.draw();
        window.read(|h, cx| assert_eq!(h.0.read(cx).expanded, expected));
    }
}
#[test]
fn stale_inventory_cannot_restore_another_conversations_windows() {
    let mut app = TestApp::new();
    app.update(|cx| {
        let p = cx.new(|_| Preview::new(ThemeMode::Dark));
        p.update(cx, |p, cx| {
            let old = p.generation;
            p.cancel_capture();
            p.accept_windows(old, Ok(vec![info(1)]), cx);
            assert!(p.windows.is_empty());
            p.accept_windows(p.generation, Ok(vec![info(1), info(2)]), cx);
            assert_eq!(p.windows.len(), 2);
            p.accept_windows(p.generation, Ok(vec![info(2)]), cx);
            assert_eq!(p.windows.len(), 1);
            assert_eq!(p.windows[0].info.id.window, 2);
            p.accept_windows(p.generation, Ok(vec![]), cx);
            assert!(p.windows.is_empty());
            assert_eq!(p.capture_error, Some(CaptureError::WindowUnavailable));
        });
    });
}
#[test]
fn reordering_windows_preserves_their_own_state_and_closed_windows_are_removed() {
    let mut app = TestApp::new();
    app.update(|cx| {
        let p = cx.new(|_| Preview::new(ThemeMode::Dark));
        p.update(cx, |p, cx| {
            p.accept_windows(p.generation, Ok(vec![info(1), info(2)]), cx);
            p.windows[0].error = Some(CaptureError::PermissionRequired);
            p.accept_windows(p.generation, Ok(vec![info(2), info(1)]), cx);
            assert_eq!(p.windows[0].info.id.window, 2);
            assert_eq!(p.windows[0].error, None);
            assert_eq!(p.windows[1].error, Some(CaptureError::PermissionRequired));
            p.accept_windows(p.generation, Ok(vec![info(2)]), cx);
            assert_eq!(p.windows.len(), 1);
        });
    });
}
#[test]
fn selecting_a_rear_preview_preserves_the_running_task_and_survives_inventory_refresh() {
    let mut app = TestApp::new();
    app.update(|cx| {
        let p = cx.new(|_| Preview::new(ThemeMode::Dark));
        p.update(cx, |p, cx| {
            p.phase = ConversationPhase::Streaming;
            p.accept_windows(p.generation, Ok(vec![info(1), info(2)]), cx);
            p.interact(info(2).id, cx);
            assert_eq!(p.phase, ConversationPhase::Streaming);
            assert!(p.takeover_pending.is_none());
            assert_eq!(p.windows[0].info.id, info(2).id);
            p.accept_windows(p.generation, Ok(vec![info(1), info(2)]), cx);
            assert_eq!(p.windows[0].info.id, info(2).id);
            p.accept_windows(p.generation, Ok(vec![info(1)]), cx);
            assert_eq!(p.selected_window, None);
            assert_eq!(p.windows[0].info.id, info(1).id);
        });
    });
}

#[test]
fn rebinding_another_composer_clears_pending_takeover_and_windows() {
    let mut app = TestApp::new();
    app.update(|cx| {
        let first = cx.new(|cx| ComposerView::new(ThemeMode::Dark, cx));
        let second = cx.new(|cx| ComposerView::new(ThemeMode::Dark, cx));
        let p = cx.new(|_| Preview::new(ThemeMode::Dark));
        p.update(cx, |p, cx| {
            p.sync(
                first,
                &[],
                ConversationPhase::Streaming,
                ThemeMode::Dark,
                cx,
            );
            p.takeover_pending = Some(info(1));
            p.windows.push(WindowPreview::new(info(1)));
            p.sync(
                second,
                &[],
                ConversationPhase::Complete,
                ThemeMode::Dark,
                cx,
            );
            assert!(p.takeover_pending.is_none());
            assert!(p.windows.is_empty());
            assert!(!p.seen_running);
        });
    });
}
#[test]
fn failed_interrupt_never_confirms_a_takeover() {
    let mut app = TestApp::new();
    app.update(|cx| {
        let owner = cx.new(|cx| ComposerView::new(ThemeMode::Dark, cx));
        let p = cx.new(|_| Preview::new(ThemeMode::Dark));
        p.update(cx, |p, cx| {
            p.sync(
                owner.clone(),
                &[],
                ConversationPhase::Streaming,
                ThemeMode::Dark,
                cx,
            );
            p.takeover_pending = Some(info(2));
            p.sync(owner, &[], ConversationPhase::Failed, ThemeMode::Dark, cx);
            assert!(p.takeover_pending.is_none());
            assert!(p.intervention_error.is_some());
            assert!(p.windows.is_empty());
        });
    });
}

#[test]
fn resized_window_discards_the_old_video_frame() {
    let mut app = TestApp::new();
    app.update(|cx| {
        let p = cx.new(|_| Preview::new(ThemeMode::Dark));
        p.update(cx, |p, cx| {
            p.accept_windows(p.generation, Ok(vec![info(1)]), cx);
            #[cfg(target_os = "macos")]
            {
                use core_video::pixel_buffer::{CVPixelBuffer, kCVPixelFormatType_32BGRA};
                p.windows[0].frame =
                    Some(CVPixelBuffer::new(kCVPixelFormatType_32BGRA, 2, 2, None).unwrap());
            }
            let mut resized = info(1);
            resized.width = 600.;
            p.accept_windows(p.generation, Ok(vec![resized]), cx);
            assert_eq!(p.windows[0].info.width, 600.);
            #[cfg(target_os = "macos")]
            assert!(p.windows[0].frame.is_none());
        });
    });
}
