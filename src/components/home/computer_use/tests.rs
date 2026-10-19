use super::*;
use gpui::{AppContext, Bounds, MouseButton, TestApp, WindowBounds, WindowOptions, point, size};

fn snapshot() -> Snapshot {
    Snapshot {
        target: None,
        label: "Verification target".into(),
        title: "Observe window".into(),
        status: AgentMcpToolCallStatus::InProgress,
        progress: None,
        error: None,
        frame: None,
        operations: vec![("Observe window".into(), AgentMcpToolCallStatus::InProgress)],
    }
}

struct PreviewHost(Entity<Preview>);
impl Render for PreviewHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full().child(self.0.clone())
    }
}

#[test]
fn preview_full_header_and_key_release_activate_once() {
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
            let mut preview = Preview::new(ThemeMode::Dark);
            preview.snapshot = Some(snapshot());
            preview.phase = ConversationPhase::Streaming;
            preview.seen_running = true;
            PreviewHost(cx.new(|_| preview))
        },
    );
    window.draw();
    window.simulate_click(point(px(760.), px(82.)), MouseButton::Left);
    window.draw();
    window.read(|host, cx| assert!(!host.0.read(cx).expanded));
    for (key, expected) in [("enter", true), ("space", false)] {
        window.simulate_keystroke(key);
        window.update(|_, window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::KeyUp(gpui::KeyUpEvent {
                    keystroke: gpui::Keystroke::parse(key).unwrap(),
                }),
                cx,
            );
        });
        window.draw();
        window.read(|host, cx| assert_eq!(host.0.read(cx).expanded, expected));
    }
}

#[test]
fn stale_native_callback_cannot_restore_a_previous_conversations_window() {
    let mut app = TestApp::new();
    app.update(|cx| {
        let preview = cx.new(|_| Preview::new(ThemeMode::Dark));
        preview.update(cx, |preview, cx| {
            let old = preview.generation;
            preview.cancel_capture();
            let frame = || computer_use::Frame {
                png: vec![],
                title: "Previous window".into(),
                application: "Previous app".into(),
            };
            assert!(preview.accept_capture(old, Ok(frame()), cx).is_none());
            assert!(preview.native_frame.is_none());
            assert!(preview.window_title.is_empty());
            assert!(
                preview
                    .accept_capture(preview.generation, Ok(frame()), cx)
                    .is_some()
            );
            assert_eq!(preview.window_title, "Previous window");
            preview.accept_capture(
                preview.generation,
                Err(CaptureError::PermissionRequired),
                cx,
            );
            assert_eq!(
                preview.capture_error,
                Some(CaptureError::PermissionRequired)
            );
            assert!(preview.capture_task.is_none());
        });
    });
}

#[test]
fn rebinding_another_composer_clears_pending_takeover_and_frames() {
    let mut app = TestApp::new();
    app.update(|cx| {
        let first = cx.new(|cx| ComposerView::new(ThemeMode::Dark, cx));
        let second = cx.new(|cx| ComposerView::new(ThemeMode::Dark, cx));
        let preview = cx.new(|_| Preview::new(ThemeMode::Dark));
        preview.update(cx, |preview, cx| {
            preview.sync(
                first,
                &[],
                ConversationPhase::Streaming,
                ThemeMode::Dark,
                cx,
            );
            preview.takeover_pending = true;
            preview.native_frame = Some(Arc::new(Image::from_bytes(ImageFormat::Png, vec![])));
            preview.sync(
                second,
                &[],
                ConversationPhase::Complete,
                ThemeMode::Dark,
                cx,
            );
            assert!(!preview.takeover_pending);
            assert!(preview.native_frame.is_none());
            assert!(!preview.seen_running);
        });
    });
}

#[test]
fn failed_interrupt_never_confirms_a_takeover() {
    let mut app = TestApp::new();
    app.update(|cx| {
        let owner = cx.new(|cx| ComposerView::new(ThemeMode::Dark, cx));
        let preview = cx.new(|_| Preview::new(ThemeMode::Dark));
        preview.update(cx, |preview, cx| {
            preview.sync(
                owner.clone(),
                &[],
                ConversationPhase::Streaming,
                ThemeMode::Dark,
                cx,
            );
            preview.takeover_pending = true;
            preview.sync(owner, &[], ConversationPhase::Failed, ThemeMode::Dark, cx);
            assert!(!preview.takeover_pending);
            assert!(preview.intervention_error.is_some());
        });
    });
}
