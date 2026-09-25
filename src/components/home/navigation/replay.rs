//! Capture-only replay of the rail's pointer scenarios.
//!
//! `--user-message-rail-motion` drives the packaged app through the same
//! pointer script `scripts/cdp_probe_user_message_rail_motion.mjs` sends the
//! reference, dispatching real window events, and records what every painted
//! frame showed: marker widths and paint, the card, `aria-current`, the rail
//! and transcript offsets, and the jump highlight.

use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration, time::Instant};

use gpui::{
    AnyWindowHandle, AppContext as _, AsyncApp, Context, Entity, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput, Point, ScrollDelta,
    ScrollWheelEvent, TouchPhase, Window, point, px,
};
use serde_json::{Value, json};

use super::super::{RAIL_ITEM_HEIGHT, RAIL_WIDTH, marker_dash_width, marker_paint};
use crate::components::home::HomeView;

/// What the last render of the rail resolved, for the recorder.
#[derive(Clone)]
pub(super) struct RenderedRail {
    pub(super) at: Instant,
    pub(super) progress: Vec<f32>,
    pub(super) focus: Option<usize>,
    pub(super) muted: bool,
    pub(super) current: Vec<bool>,
    pub(super) bookmarks: Vec<bool>,
    pub(super) scroll_top: f32,
    pub(super) opacity: f32,
}

#[derive(Default)]
struct Recording {
    started: Option<Instant>,
    frames: Vec<Value>,
    marks: Vec<Value>,
    running: bool,
    flash_row: Option<usize>,
}

impl Recording {
    fn elapsed_ms(&self, at: Instant) -> f64 {
        let started = self.started.unwrap_or(at);
        (at.saturating_duration_since(started).as_secs_f64() * 1000.0 * 10.0).round() / 10.0
    }
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

fn sample(home: &Entity<HomeView>, recording: &Rc<RefCell<Recording>>, cx: &gpui::App) {
    let home = home.read(cx);
    let rail = &home.navigation_rail;
    let now = Instant::now();
    let mut record = recording.borrow_mut();
    if !record.running {
        return;
    }
    let Some(rendered) = rail.rendered.as_ref() else {
        return;
    };
    let theme = crate::theme::Theme::for_mode(home.mode);
    let colours = rendered
        .progress
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let (colour, opacity) = marker_paint(
                theme,
                rendered.current.get(index).copied().unwrap_or(false),
                rendered.focus == Some(index),
                rendered.muted,
                rendered.bookmarks.get(index).copied().unwrap_or(false),
            );
            let kind = if colour == theme.text { "text" } else { "rest" };
            format!("{kind}@{}", round(opacity))
        })
        .collect::<Vec<_>>();
    let frame = rail.frame.borrow();
    let card = frame.card.as_ref().map(|hitbox| {
        let bounds = hitbox.bounds;
        json!({
            "rect": [round(f32::from(bounds.origin.x)), round(f32::from(bounds.origin.y)),
                round(f32::from(bounds.size.width)), round(f32::from(bounds.size.height))],
            "label": rail.anchor.and_then(|index| home.user_message_navigation.get(index))
                .map(|item| item.label.chars().take(40).collect::<String>())
                .unwrap_or_default(),
        })
    });
    let flash = record
        .flash_row
        .and_then(|row| home.user_message_highlight(row, cx));
    let value = json!({
        "t": record.elapsed_ms(now),
        "rendered": record.elapsed_ms(rendered.at),
        "widths": rendered.progress.iter().map(|progress| round(marker_dash_width(*progress))).collect::<Vec<_>>(),
        "colours": colours,
        "current": rendered.current.iter().map(|current| if *current { '1' } else { '0' }).collect::<String>(),
        "scrubTarget": rail.scrub.map_or(-1, |scrub| scrub.target as i64),
        "scrubbing": rail.scrub.is_some(),
        "card": card,
        "paneScrollTop": round(f32::from(home.conversation_list.scroll_offset())),
        "listScrollTop": round(rendered.scroll_top),
        "opacity": round(rendered.opacity),
        // Full precision: the highlight's tail settles within 1e-3 of rest.
        "flash": flash.map(|alpha| (f64::from(alpha) * 1.0e6).round() / 1.0e6),
    });
    drop(frame);
    record.frames.push(value);
}

fn schedule_sample(home: Entity<HomeView>, recording: Rc<RefCell<Recording>>, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        sample(&home, &recording, cx);
        if recording.borrow().running {
            schedule_sample(home, recording, window);
        }
    });
}

struct Driver {
    window: AnyWindowHandle,
    home: Entity<HomeView>,
    recording: Rc<RefCell<Recording>>,
}

impl Driver {
    fn dispatch(&self, cx: &mut AsyncApp, input: PlatformInput) {
        let _ = cx.update_window(self.window, |_, window, cx| {
            window.dispatch_event(input, cx);
        });
    }

    fn move_to(&self, cx: &mut AsyncApp, position: Point<Pixels>, pressed: bool) {
        self.dispatch(
            cx,
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: pressed.then_some(MouseButton::Left),
                modifiers: Modifiers::default(),
            }),
        );
    }

    fn press(&self, cx: &mut AsyncApp, position: Point<Pixels>) {
        self.dispatch(
            cx,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
        );
    }

    fn release(&self, cx: &mut AsyncApp, position: Point<Pixels>) {
        self.dispatch(
            cx,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
            }),
        );
    }

    fn click(&self, cx: &mut AsyncApp, position: Point<Pixels>) {
        self.move_to(cx, position, false);
        self.press(cx, position);
        self.release(cx, position);
    }

    async fn wait(&self, cx: &mut AsyncApp, milliseconds: u64) {
        cx.background_executor()
            .timer(Duration::from_millis(milliseconds))
            .await;
    }

    fn mark(&self, name: &str) {
        let mut record = self.recording.borrow_mut();
        let t = record.elapsed_ms(Instant::now());
        record.marks.push(json!({ "name": name, "t": t }));
    }

    fn start(&self, cx: &mut AsyncApp) {
        {
            let mut record = self.recording.borrow_mut();
            record.frames.clear();
            record.marks.clear();
            record.started = Some(Instant::now());
            record.running = true;
        }
        let (home, recording) = (self.home.clone(), self.recording.clone());
        let _ = cx.update_window(self.window, |_, window, _| {
            schedule_sample(home, recording, window)
        });
    }

    fn stop(&self) -> Value {
        let mut record = self.recording.borrow_mut();
        record.running = false;
        json!({ "frames": std::mem::take(&mut record.frames), "marks": std::mem::take(&mut record.marks) })
    }

    fn centres(&self, cx: &mut AsyncApp) -> Vec<Point<Pixels>> {
        cx.update(|cx| {
            let home = self.home.read(cx);
            let Some(bounds) = home.navigation_rail.frame.borrow().rail_bounds() else {
                return Vec::new();
            };
            let scroll_top = home.rail_scroll_top();
            (0..home.user_message_navigation.len())
                .map(|index| {
                    point(
                        bounds.left() + px(RAIL_WIDTH * 0.5),
                        bounds.top()
                            + px(index as f32 * RAIL_ITEM_HEIGHT + RAIL_ITEM_HEIGHT * 0.5
                                - scroll_top),
                    )
                })
                .collect()
        })
    }

    fn card_bounds(&self, cx: &mut AsyncApp) -> Option<gpui::Bounds<Pixels>> {
        cx.update(|cx| {
            self.home
                .read(cx)
                .navigation_rail
                .frame
                .borrow()
                .card_bounds()
        })
    }

    fn transcript_offset(&self, cx: &mut AsyncApp) -> f32 {
        cx.update(|cx| f32::from(self.home.read(cx).conversation_list.scroll_offset()))
    }

    fn key(&self, cx: &mut AsyncApp, keystroke: &str) {
        let keystroke = gpui::Keystroke::parse(keystroke).expect("valid keystroke");
        self.dispatch(
            cx,
            PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }),
        );
        self.dispatch(cx, PlatformInput::KeyUp(gpui::KeyUpEvent { keystroke }));
    }

    /// Where each measured prompt's bubble sits relative to the transcript's
    /// top, in rail order.
    fn prompt_tops(&self, cx: &mut AsyncApp) -> Vec<Option<f64>> {
        cx.update(|cx| {
            let home = self.home.read(cx);
            let top = home.conversation_list.viewport_bounds().top();
            home.user_message_navigation
                .iter()
                .map(|item| {
                    super::super::row_span(&home.conversation_list, item.row_index)
                        .map(|(row_top, _)| round(f32::from(row_top - top)))
                })
                .collect()
        })
    }

    async fn park(&self, cx: &mut AsyncApp) {
        self.move_to(cx, point(px(900.0), px(500.0)), false);
        self.wait(cx, 900).await;
    }
}

fn shifted(position: Point<Pixels>, dx: f32, dy: f32) -> Point<Pixels> {
    point(position.x + px(dx), position.y + px(dy))
}

async fn run(driver: Driver, output: PathBuf, cx: &mut AsyncApp) {
    let mut scenarios = serde_json::Map::new();
    let centres = driver.centres(cx);
    if centres.len() < 10 {
        eprintln!(
            "the rail has {} markers; the replay needs 10",
            centres.len()
        );
        std::process::exit(1);
    }
    let c = |index: usize| centres[index];

    // hoverEnter
    driver.park(cx).await;
    driver.move_to(cx, shifted(c(3), 60.0, 0.0), false);
    driver.wait(cx, 200).await;
    driver.start(cx);
    driver.wait(cx, 80).await;
    driver.mark("enter-marker-4");
    driver.move_to(cx, c(3), false);
    driver.wait(cx, 700).await;
    scenarios.insert("hoverEnter".into(), driver.stop());

    // hoverStep
    driver.park(cx).await;
    driver.move_to(cx, c(3), false);
    driver.wait(cx, 600).await;
    driver.start(cx);
    driver.wait(cx, 80).await;
    driver.mark("step-to-marker-7");
    driver.move_to(cx, c(6), false);
    driver.wait(cx, 500).await;
    scenarios.insert("hoverStep".into(), driver.stop());

    // hoverSweep
    driver.park(cx).await;
    driver.move_to(cx, shifted(c(1), 60.0, 0.0), false);
    driver.wait(cx, 200).await;
    driver.start(cx);
    driver.wait(cx, 80).await;
    driver.mark("enter-marker-2");
    for index in 1..=9 {
        driver.move_to(cx, c(index), false);
        driver.wait(cx, 40).await;
    }
    driver.mark("sweep-done-marker-10");
    driver.wait(cx, 600).await;
    scenarios.insert("hoverSweep".into(), driver.stop());

    // hoverLeaveAndReturn
    driver.park(cx).await;
    driver.move_to(cx, c(3), false);
    driver.wait(cx, 600).await;
    driver.start(cx);
    driver.wait(cx, 80).await;
    driver.mark("leave-left");
    driver.move_to(cx, shifted(c(3), -30.0, 0.0), false);
    driver.wait(cx, 250).await;
    driver.mark("reenter-after-250ms");
    driver.move_to(cx, c(3), false);
    driver.wait(cx, 500).await;
    driver.mark("leave-left-again");
    driver.move_to(cx, shifted(c(3), -30.0, 0.0), false);
    driver.wait(cx, 700).await;
    driver.mark("reenter-after-700ms");
    driver.move_to(cx, c(3), false);
    driver.wait(cx, 600).await;
    scenarios.insert("hoverLeaveAndReturn".into(), driver.stop());

    // hoverToCard
    driver.park(cx).await;
    driver.move_to(cx, c(3), false);
    driver.wait(cx, 600).await;
    if let Some(card) = driver.card_bounds(cx) {
        driver.start(cx);
        driver.wait(cx, 80).await;
        driver.mark("travel-to-card");
        let target = point(card.left() + px(40.0), card.center().y);
        let (from_x, from_y) = (f32::from(c(3).x), f32::from(c(3).y));
        let (to_x, to_y) = (f32::from(target.x), f32::from(target.y));
        for step in 1..=6 {
            let t = step as f32 / 6.0;
            driver.move_to(
                cx,
                point(
                    px(from_x + (to_x - from_x) * t),
                    px(from_y + (to_y - from_y) * t),
                ),
                false,
            );
            driver.wait(cx, 16).await;
        }
        driver.mark("on-card");
        driver.wait(cx, 400).await;
        driver.mark("leave-card-right");
        driver.move_to(cx, point(card.right() + px(40.0), target.y), false);
        driver.wait(cx, 500).await;
        scenarios.insert("hoverToCard".into(), driver.stop());
    }

    // hoverStopInGap
    driver.park(cx).await;
    driver.move_to(cx, c(3), false);
    driver.wait(cx, 600).await;
    driver.start(cx);
    driver.wait(cx, 80).await;
    driver.mark("leave-right-into-gap");
    driver.move_to(cx, shifted(c(3), 22.0, 200.0), false);
    driver.wait(cx, 600).await;
    scenarios.insert("hoverStopInGap".into(), driver.stop());

    // cardSweep: every marker's card, for the preview layout.
    driver.park(cx).await;
    driver.move_to(cx, c(0), false);
    driver.wait(cx, 450).await;
    driver.start(cx);
    for index in 0..centres.len() {
        driver.mark(&format!("card-{}", index + 1));
        driver.move_to(cx, c(index), false);
        driver.wait(cx, 120).await;
    }
    scenarios.insert("cardSweep".into(), driver.stop());

    // clickNear
    let mut clicks = Vec::new();
    for (from, to) in [(1, 2), (2, 4), (4, 1)] {
        driver.park(cx).await;
        driver.click(cx, c(from));
        driver.wait(cx, 1600).await;
        driver.park(cx).await;
        let row = cx.update(|cx| self_row(&driver.home, to, cx));
        driver.recording.borrow_mut().flash_row = row;
        let before = driver.transcript_offset(cx);
        driver.start(cx);
        driver.wait(cx, 80).await;
        driver.mark(&format!("click-{}-to-{}", from + 1, to + 1));
        driver.click(cx, c(to));
        driver.wait(cx, 1800).await;
        let mut record = driver.stop();
        record["from"] = json!(from);
        record["to"] = json!(to);
        record["offsetBefore"] = json!(before);
        record["offsetAfter"] = json!(driver.transcript_offset(cx));
        clicks.push(record);
    }
    driver.recording.borrow_mut().flash_row = None;
    scenarios.insert("clickNear".into(), Value::Array(clicks));

    // scrub
    driver.park(cx).await;
    driver.start(cx);
    driver.wait(cx, 80).await;
    driver.move_to(cx, c(1), false);
    driver.mark("press-marker-2");
    driver.press(cx, c(1));
    driver.wait(cx, 120).await;
    for step in 1..=8 {
        driver.mark(&format!("drag-to-{}", step + 2));
        driver.move_to(cx, point(c(1).x, c(1 + step).y), true);
        driver.wait(cx, 90).await;
    }
    let last = *centres.last().expect("markers");
    driver.mark("drag-below-rail");
    let below = shifted(point(c(1).x, last.y), 80.0, 120.0);
    driver.move_to(cx, below, true);
    driver.wait(cx, 200).await;
    driver.mark("release");
    driver.release(cx, below);
    driver.wait(cx, 600).await;
    scenarios.insert("scrub".into(), driver.stop());

    // altArrows: the keyboard steps the reference binds on the document.
    driver.park(cx).await;
    driver.click(cx, c(0));
    driver.wait(cx, 1600).await;
    driver.park(cx).await;
    let mut steps = vec![json!({ "key": "start", "tops": driver.prompt_tops(cx) })];
    for key in ["alt-down", "alt-down", "alt-down", "alt-up"] {
        driver.key(cx, key);
        driver.wait(cx, 900).await;
        steps.push(json!({ "key": key, "tops": driver.prompt_tops(cx) }));
    }
    scenarios.insert("altArrows".into(), Value::Array(steps));

    // wheelOverRail
    driver.park(cx).await;
    driver.click(cx, c(4));
    driver.wait(cx, 1600).await;
    driver.park(cx).await;
    let before = driver.transcript_offset(cx);
    driver.move_to(cx, c(5), false);
    driver.dispatch(
        cx,
        PlatformInput::ScrollWheel(ScrollWheelEvent {
            position: c(5),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-240.0))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        }),
    );
    driver.wait(cx, 700).await;
    scenarios.insert(
        "wheelOverRail".into(),
        json!({ "paneBefore": before, "paneAfter": driver.transcript_offset(cx) }),
    );
    driver.park(cx).await;

    let report = json!({
        "centres": centres.iter().map(|centre| [round(f32::from(centre.x)), round(f32::from(centre.y))]).collect::<Vec<_>>(),
        "scenarios": scenarios,
    });
    if let Err(error) = std::fs::write(
        &output,
        serde_json::to_vec(&report).expect("motion report is JSON"),
    ) {
        eprintln!("failed to save {}: {error}", output.display());
        std::process::exit(1);
    }
    println!("{}", output.display());
    cx.update(|cx| cx.quit());
}

fn self_row(home: &Entity<HomeView>, index: usize, cx: &gpui::App) -> Option<usize> {
    home.read(cx)
        .user_message_navigation
        .get(index)
        .map(|item| item.row_index)
}

impl HomeView {
    /// Capture hook: replay the reference probe's pointer scenarios and save
    /// what each frame painted to `output`, then quit.
    pub(crate) fn record_user_message_rail_motion(
        &mut self,
        output: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation_rail.pinned_for_capture = false;
        let driver = Driver {
            window: window.window_handle(),
            home: cx.entity(),
            recording: Rc::new(RefCell::new(Recording::default())),
        };
        cx.spawn(async move |_, cx| run(driver, output, cx).await)
            .detach();
    }
}
