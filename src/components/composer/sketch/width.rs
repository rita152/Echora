//! Pointer capture and accessible input for the shared sketch/markup width control.
use super::*;
use gpui::{AccessibleAction, DispatchPhase, Div, MouseMoveEvent, MouseUpEvent, Stateful};

const MIN: f32 = 2.;
const MAX: f32 = 48.;
const TRAVEL: f32 = 144.;
const INSET: f32 = 16.;

pub(super) fn width_at(y: f32) -> f32 {
    (MAX - (y - INSET) / TRAVEL * (MAX - MIN))
        .round()
        .clamp(MIN, MAX)
}

impl ComposerView {
    pub(super) fn set_sketch_width(&mut self, width: f32) {
        let width = width.round().clamp(MIN, MAX);
        self.sketch.width = Some(width);
        if let Some(mark) = self
            .sketch
            .selected
            .and_then(|i| self.sketch.marks.get_mut(i))
        {
            mark.width = width;
        }
    }

    fn step_sketch_width(&mut self, delta: f32) {
        if self.sketch.selected.is_some() {
            self.sketch.checkpoint();
        }
        self.set_sketch_width(self.sketch.width.unwrap_or(12.) + delta);
    }

    pub(super) fn sketch_width_slider(
        &self,
        top: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let bounds = self.sketch.width_bounds.clone();
        let entity = cx.entity();
        let focus = self.sketch.width_focus.as_ref().unwrap();
        let width = self.sketch.width.unwrap_or(12.);
        div()
            .id("sketch-width-slider")
            .role(Role::Slider)
            .aria_label(crate::i18n::format!("笔画宽度" => "Stroke width"))
            .aria_value(format!("{width}"))
            .aria_numeric_value(width as f64)
            .aria_min_numeric_value(MIN as f64)
            .aria_max_numeric_value(MAX as f64)
            .aria_numeric_value_step(1.)
            .track_focus(focus)
            .tab_stop(true)
            .focus_visible(|s| s.border_2().border_color(gpui::rgb(0x339cff)))
            .absolute()
            .left(px(8.))
            .top(px(top))
            .w(px(32.))
            .h(px(176.))
            .rounded_full()
            .bg(theme.elevated)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &gpui::MouseDownEvent, window, cx| {
                    if this.sketch.selected.is_some() {
                        this.sketch.checkpoint();
                    }
                    this.sketch.width_dragging = true;
                    this.sketch.width_focus.as_ref().unwrap().focus(window, cx);
                    this.set_sketch_width_at(e.position);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, _, cx| {
                let delta = match e.keystroke.key.as_str() {
                    "up" | "right" => 1.,
                    "down" | "left" => -1.,
                    "pageup" => 5.,
                    "pagedown" => -5.,
                    "home" => -MAX,
                    "end" => MAX,
                    _ => return,
                };
                this.step_sketch_width(delta);
                cx.stop_propagation();
                cx.notify();
            }))
            .on_scroll_wheel(cx.listener(|this, e: &gpui::ScrollWheelEvent, _, cx| {
                let delta = f32::from(e.delta.pixel_delta(px(20.)).y);
                if delta != 0. {
                    this.step_sketch_width(delta.signum());
                    cx.notify();
                }
                cx.stop_propagation();
            }))
            .on_a11y_action(AccessibleAction::Increment, {
                let view = cx.entity();
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.step_sketch_width(1.);
                        cx.notify();
                    })
                }
            })
            .on_a11y_action(AccessibleAction::Decrement, {
                let view = cx.entity();
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.step_sketch_width(-1.);
                        cx.notify();
                    })
                }
            })
            .child(
                canvas(
                    move |b, _, _| bounds.set(Some(b)),
                    move |b, _, window, _| {
                        // Capture the gesture for its entire lifetime, including outside
                        // the narrow rail. A drag entering the rail from the canvas is
                        // never a width gesture.
                        let moving = entity.clone();
                        window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            moving.update(cx, |this, cx| {
                                if this.sketch.width_dragging {
                                    if e.pressed_button == Some(MouseButton::Left) {
                                        this.set_sketch_width_at(e.position);
                                    } else {
                                        this.sketch.width_dragging = false;
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            });
                        });
                        let released = entity.clone();
                        window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                            if phase != DispatchPhase::Capture || e.button != MouseButton::Left {
                                return;
                            }
                            released.update(cx, |this, cx| {
                                if this.sketch.width_dragging {
                                    this.sketch.width_dragging = false;
                                    this.set_sketch_width_at(e.position);
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            });
                        });
                        let mut path = PathBuilder::fill();
                        path.move_to(b.origin + point(px(6.), px(8.)));
                        path.line_to(b.origin + point(px(26.), px(8.)));
                        path.line_to(b.origin + point(px(16.), px(168.)));
                        path.close();
                        if let Ok(path) = path.build() {
                            window.paint_path(path, theme.text.alpha(0.2));
                        }
                    },
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.))
                    .top(px((MAX - width) / (MAX - MIN) * TRAVEL + INSET - 8.))
                    .size(px(16.))
                    .rounded_full()
                    .bg(gpui::rgb(0xffffff))
                    .border_1()
                    .border_color(gpui::rgba(0x00000033)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Entity, Render, size};

    struct Harness(Entity<ComposerView>);
    impl Render for Harness {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .children(self.0.update(cx, |view, cx| view.render_sketch(window, cx)))
        }
    }

    #[test]
    fn width_gesture_survives_leaving_the_rail_and_controls_subsequent_strokes() {
        let mut app = gpui::TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        app.update_entity(&composer, |view, cx| view.open_sketch(cx));
        let mut window = app.open_window_with_options(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1000.), px(900.)),
                ))),
                ..Default::default()
            },
            |_, _| Harness(composer.clone()),
        );
        window.draw();
        let rail = window.read(|_, cx| composer.read(cx).sketch.width_bounds.get().unwrap());
        let start = rail.origin + point(px(16.), px(INSET));
        let outside = rail.origin + point(px(100.), px(220.));
        window.simulate_mouse_down(start, MouseButton::Left);
        window.update(|_, window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::MouseMove(MouseMoveEvent {
                    position: outside,
                    modifiers: gpui::Modifiers::default(),
                    pressed_button: Some(MouseButton::Left),
                }),
                cx,
            );
        });
        window.simulate_mouse_up(outside, MouseButton::Left);
        window.draw();
        window.read(|_, cx| {
            let state = &composer.read(cx).sketch;
            assert_eq!(state.width, Some(MIN));
            assert!(!state.width_dragging);
            assert!(
                state.marks.is_empty(),
                "width dragging must not draw on the canvas"
            );
        });
        window.simulate_keystrokes("up up");
        window.read(|_, cx| assert_eq!(composer.read(cx).sketch.width, Some(4.)));
        window.simulate_keystrokes("end");
        window.read(|_, cx| assert_eq!(composer.read(cx).sketch.width, Some(MAX)));
        window.simulate_keystrokes("home");
        window.read(|_, cx| assert_eq!(composer.read(cx).sketch.width, Some(MIN)));
        window.update(|_, _, cx| {
            composer.update(cx, |view, _| {
                view.sketch.down((100., 200.));
                view.sketch.drag((500., 200.));
                view.sketch.last = None;
                view.sketch.active = None;
                view.set_sketch_width(MAX);
                view.sketch.down((100., 400.));
                view.sketch.drag((500., 400.));
                assert_eq!(view.sketch.marks[0].width, MIN);
                assert_eq!(view.sketch.marks[1].width, MAX);
                let png = super::super::export_png(&view.sketch.marks, 0x000000, None).unwrap();
                let image = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
                let thickness = |y: u32| {
                    (y - 30..y + 30)
                        .filter(|&row| image.pixel(300, row).unwrap().red() > 200)
                        .count()
                };
                assert_eq!(thickness(200), MIN as usize);
                assert_eq!(thickness(400), MAX as usize);
            })
        });
    }
}
