//! The rail's pointer behaviour, driven through real window events against a
//! resumed task, with the timings recorded from the reference.

use std::{path::PathBuf, time::Duration};

use gpui::{
    AppContext as _, MouseButton, MouseMoveEvent, Pixels, Point, TestApp, TestAppWindow,
    WindowBounds, WindowOptions, point, px, size,
};

use super::super::{
    RAIL_ITEM_HEIGHT, RAIL_WIDTH,
    motion::{CARD_CLOSE_GRACE, CARD_OPEN_DELAY, MARKER_TRANSITION},
};
use crate::{
    agent::{
        HistoryItemDetail, HistoryTurnStatus, ThreadActivity, ThreadHistory, ThreadHistoryItem,
        ThreadSummary, ThreadTurn,
    },
    components::home::HomeView,
    theme::ThemeMode,
};

const TURNS: usize = 14;

fn history() -> ThreadHistory {
    history_with_steering(None)
}

/// Fourteen turns; `steered` adds a steering prompt inside that turn.
fn history_with_steering(steered: Option<usize>) -> ThreadHistory {
    let turn = |index: usize| ThreadTurn {
        turn_id: format!("turn-{index}"),
        status: HistoryTurnStatus::Completed,
        items_view: HistoryItemDetail::Full,
        items: {
            let mut items = vec![
                ThreadHistoryItem::UserMessage {
                    client_message_id: None,
                    images: Vec::new(),
                    item_id: format!("user-{index}"),
                    text: format!("prompt {index}"),
                },
                ThreadHistoryItem::AssistantMessage {
                    item_id: format!("commentary-{index}"),
                    text: format!("commentary {index}"),
                    phase: None,
                },
            ];
            if steered == Some(index) {
                items.push(ThreadHistoryItem::UserMessage {
                    client_message_id: None,
                    images: Vec::new(),
                    item_id: format!("steer-{index}"),
                    text: format!("steer {index}"),
                });
            }
            items.push(ThreadHistoryItem::AssistantMessage {
                item_id: format!("assistant-{index}"),
                text: format!("answer {index}\n\n{}", "line of the reply. ".repeat(40)),
                phase: None,
            });
            items
        },
        started_at: Some(1_000),
        completed_at: Some(2_000),
        duration_ms: Some(1_000),
        error: None,
    };
    ThreadHistory {
        thread: ThreadSummary {
            thread_id: "rail-thread".to_owned(),
            title: "Rail".to_owned(),
            preview: String::new(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            section: None,
            created_at: 1,
            updated_at: 2,
            recency_at: Some(2),
            activity: ThreadActivity::Idle,
        },
        turns: (0..TURNS).map(turn).collect(),
        next_turn_cursor: None,
        backwards_turn_cursor: None,
    }
}

struct Rail {
    app: TestApp,
    window: TestAppWindow<HomeView>,
}

impl Rail {
    fn open() -> Self {
        Self::open_history(history(), TURNS)
    }

    fn open_history(history: ThreadHistory, prompts: usize) -> Self {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(gpui::Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(1800.0), px(1000.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );
        window.update(|home, window, cx| {
            window.activate_window();
            home.composer_entity()
                .update(cx, |composer, cx| composer.hydrate_history(history, cx));
        });
        app.run_until_parked();
        let mut rail = Self { app, window };
        rail.settle();
        assert_eq!(
            rail.window
                .read(|home, _| home.user_message_navigation.len()),
            prompts
        );
        rail
    }

    /// Draws enough frames for layout, hitboxes, and running animations.
    fn settle(&mut self) {
        for _ in 0..3 {
            self.window.draw();
            self.app.run_until_parked();
        }
    }

    /// Advances the clock and runs the frame the rail's animations wait on.
    fn wait(&mut self, duration: Duration) {
        self.app.advance_clock(duration);
        self.app.run_until_parked();
        let handle = self.window.handle();
        self.app.update(|cx| {
            cx.update_window(handle.into(), |_, window, cx| {
                window.simulate_next_frame(cx)
            })
            .unwrap()
        });
        self.app.run_until_parked();
        self.window.draw();
    }

    /// Lets every running animation finish, so no frame callback outlives
    /// the test.
    fn finish(mut self) {
        for _ in 0..4 {
            self.wait(Duration::from_secs(1));
        }
    }

    fn rail_bounds(&self) -> gpui::Bounds<Pixels> {
        self.window.read(|home, _| {
            home.navigation_rail
                .frame
                .borrow()
                .rail
                .as_ref()
                .expect("the rail is painted")
                .bounds
        })
    }

    fn marker(&self, index: usize) -> Point<Pixels> {
        let bounds = self.rail_bounds();
        point(
            bounds.left() + px(RAIL_WIDTH * 0.5),
            bounds.top() + px(index as f32 * RAIL_ITEM_HEIGHT + RAIL_ITEM_HEIGHT * 0.5),
        )
    }

    fn move_to(&mut self, position: Point<Pixels>) {
        self.window.simulate_mouse_move(position);
        self.window.draw();
    }

    fn drag_to(&mut self, position: Point<Pixels>) {
        self.window.simulate_event(MouseMoveEvent {
            position,
            pressed_button: Some(MouseButton::Left),
            modifiers: Default::default(),
        });
        self.window.draw();
    }

    fn card(&self) -> Option<usize> {
        self.window.read(|home, _| {
            home.navigation_rail
                .card_open
                .then_some(home.navigation_rail.anchor)
                .flatten()
        })
    }

    fn transcript_offset(&self) -> f32 {
        self.window
            .read(|home, _| f32::from(home.conversation_list.scroll_offset()))
    }
}

#[test]
fn the_taper_follows_the_pointer_before_the_card_opens() {
    let mut rail = Rail::open();
    rail.move_to(rail.marker(3));
    rail.window.read(|home, cx| {
        let now = cx.background_executor().now();
        assert_eq!(home.navigation_rail.hovered, Some(3));
        assert!(home.navigation_rail.markers[3].is_running(now));
    });
    assert_eq!(rail.card(), None);
    rail.wait(MARKER_TRANSITION);
    // The widened marker has settled while the card is still waiting.
    rail.window.read(|home, cx| {
        let now = cx.background_executor().now();
        assert_eq!(home.navigation_rail.markers[3].value(now), 1.0);
        assert_eq!(home.navigation_rail.markers[4].value(now), 0.7);
        assert_eq!(home.navigation_rail.markers[6].value(now), 0.2);
    });
    assert_eq!(rail.card(), None);
    rail.finish();
}

#[test]
fn the_card_opens_250ms_after_entering_even_while_sweeping() {
    let mut rail = Rail::open();
    rail.move_to(rail.marker(1));
    for index in 2..=5 {
        rail.wait(Duration::from_millis(40));
        rail.move_to(rail.marker(index));
    }
    // 160 ms have passed; moving between markers does not restart the delay.
    rail.wait(CARD_OPEN_DELAY - Duration::from_millis(161));
    assert_eq!(rail.card(), None);
    rail.wait(Duration::from_millis(2));
    assert_eq!(rail.card(), Some(5));
    // Once open, the card follows the hovered marker at once.
    rail.move_to(rail.marker(8));
    assert_eq!(rail.card(), Some(8));
    rail.finish();
}

#[test]
fn leaving_the_rail_closes_the_card_after_the_grace_and_a_quick_return_skips_the_delay() {
    let mut rail = Rail::open();
    let marker = rail.marker(3);
    rail.move_to(marker);
    rail.wait(CARD_OPEN_DELAY + Duration::from_millis(1));
    assert_eq!(rail.card(), Some(3));

    // Away from the card, to the rail's left.
    rail.move_to(marker - point(px(30.0), px(0.0)));
    rail.window
        .read(|home, _| assert!(!home.navigation_rail.rail_hovered));
    rail.wait(CARD_CLOSE_GRACE - Duration::from_millis(1));
    assert_eq!(rail.card(), Some(3));
    rail.wait(Duration::from_millis(2));
    assert_eq!(rail.card(), None);

    // Back within the 300 ms skip window: no delay.
    rail.wait(Duration::from_millis(100));
    rail.move_to(marker);
    assert_eq!(rail.card(), Some(3));

    // Out and back after the window: the full delay again.
    rail.move_to(marker - point(px(30.0), px(0.0)));
    rail.wait(CARD_CLOSE_GRACE + Duration::from_millis(1));
    assert_eq!(rail.card(), None);
    rail.wait(Duration::from_millis(400));
    rail.move_to(marker);
    assert_eq!(rail.card(), None);
    rail.wait(CARD_OPEN_DELAY + Duration::from_millis(1));
    assert_eq!(rail.card(), Some(3));
    rail.finish();
}

#[test]
fn the_card_stays_open_on_the_way_to_it_and_closes_after_leaving_it() {
    let mut rail = Rail::open();
    let marker = rail.marker(3);
    rail.move_to(marker);
    rail.wait(CARD_OPEN_DELAY + Duration::from_millis(1));
    let card = rail.window.read(|home, _| {
        home.navigation_rail
            .frame
            .borrow()
            .card
            .as_ref()
            .expect("the card is painted")
            .bounds
    });
    // Just past the rail, inside the triangle toward the card.
    let on_card = point(card.left() + px(40.0), card.center().y);
    rail.move_to(point(marker.x + px(18.5), marker.y));
    rail.wait(Duration::from_millis(60));
    rail.move_to(on_card);
    rail.wait(CARD_CLOSE_GRACE * 3);
    assert_eq!(rail.card(), Some(3));

    // Out of the card to the right: closed after the grace.
    rail.move_to(point(card.right() + px(40.0), card.center().y));
    rail.wait(CARD_CLOSE_GRACE + Duration::from_millis(1));
    assert_eq!(rail.card(), None);

    // Out over the rail's top edge, clear of the card: a move that leaves
    // the triangle toward the card closes it at once.
    let first = rail.marker(0);
    rail.wait(Duration::from_millis(400));
    rail.move_to(first);
    rail.wait(CARD_OPEN_DELAY + Duration::from_millis(1));
    assert_eq!(rail.card(), Some(0));
    let top = rail.rail_bounds().top();
    rail.move_to(point(first.x, top - px(5.0)));
    assert_eq!(rail.card(), Some(0));
    rail.move_to(point(first.x - px(10.0), top - px(120.0)));
    assert_eq!(rail.card(), None);
    rail.finish();
}

#[test]
fn pressing_opens_the_card_and_scrubbing_jumps_without_transitions() {
    let mut rail = Rail::open();
    let press = rail.marker(1);
    rail.move_to(press);
    rail.window.simulate_mouse_down(press, MouseButton::Left);
    rail.window.draw();
    assert_eq!(rail.card(), Some(1));

    let before = rail.transcript_offset();
    rail.drag_to(rail.marker(6));
    assert_eq!(rail.card(), Some(6));
    rail.window.read(|home, cx| {
        let now = cx.background_executor().now();
        // A scrub snaps the taper; the transcript jumped instantly.
        assert_eq!(home.navigation_rail.markers[6].value(now), 1.0);
        assert_eq!(home.navigation_rail.markers[5].value(now), 0.7);
        assert!(home.navigation_rail.transcript_scroll.is_none());
    });
    assert_ne!(rail.transcript_offset(), before);

    // Dragging past the rail clamps to the last marker.
    let bounds = rail.rail_bounds();
    rail.drag_to(point(
        bounds.right() + px(80.0),
        bounds.bottom() + px(120.0),
    ));
    assert_eq!(rail.card(), Some(TURNS - 1));

    // Releasing outside the rail closes the card after the grace, and a
    // scrub is not followed by a click jump.
    let released = point(bounds.right() + px(80.0), bounds.bottom() + px(120.0));
    rail.window.simulate_mouse_up(released, MouseButton::Left);
    rail.window.draw();
    rail.window.read(|home, _| {
        assert!(home.navigation_rail.scrub.is_none());
        assert!(home.navigation_rail.transcript_scroll.is_none());
    });
    rail.wait(CARD_CLOSE_GRACE + Duration::from_millis(1));
    assert_eq!(rail.card(), None);
    rail.finish();
}

#[test]
fn a_click_scrolls_smoothly_to_a_mounted_turn_and_flashes_its_bubble() {
    let mut rail = Rail::open();
    // Land on turn 12 first, then click turn 11: one turn away is mounted.
    let far = rail.marker(12);
    rail.move_to(far);
    rail.window.simulate_click(far, MouseButton::Left);
    rail.settle();
    rail.wait(Duration::from_secs(2));

    let target = rail.marker(11);
    rail.move_to(target);
    rail.window.simulate_click(target, MouseButton::Left);
    rail.window.draw();
    let row = rail
        .window
        .read(|home, _| home.user_message_navigation[11].row_index);
    rail.window.read(|home, cx| {
        assert!(home.navigation_rail.transcript_scroll.is_some());
        assert!(home.user_message_highlight(row, cx).is_some());
    });
    assert_eq!(rail.card(), Some(11));

    // The scroll finishes with the prompt 16 px below the transcript's top.
    for _ in 0..40 {
        rail.wait(Duration::from_millis(16));
    }
    let landing = rail.window.read(|home, cx| {
        assert!(home.navigation_rail.transcript_scroll.is_none());
        // The highlight is still easing back.
        assert!(home.user_message_highlight(row, cx).is_some());
        // Content offset `o` prints at the list's 78 px inset, so the prompt
        // sits 16 px below the top at offset `o + 62`.
        f32::from(home.conversation_list.item_offset(row).expect("measured")) + 62.0
    });
    let offset = rail.transcript_offset();
    assert!(
        (offset - landing).abs() < 0.5,
        "offset {offset}, landing {landing}"
    );
    rail.wait(Duration::from_millis(900));
    rail.window
        .read(|home, cx| assert!(home.user_message_highlight(row, cx).is_none()));
    rail.finish();
}

#[test]
fn the_wheel_over_the_rail_never_scrolls_the_transcript() {
    let mut rail = Rail::open();
    let far = rail.marker(4);
    rail.window.simulate_click(far, MouseButton::Left);
    rail.settle();
    let before = rail.transcript_offset();
    rail.window
        .simulate_scroll(rail.marker(5), point(px(0.0), px(-240.0)));
    rail.settle();
    assert_eq!(rail.transcript_offset(), before);
    rail.finish();
}

/// Where a jump to prompt `index` leaves the list: content offset `o` prints
/// at the 78 px inset, so a 16 px scroll margin lands at `o + 62`.
fn landing(rail: &Rail, index: usize, margin: f32) -> f32 {
    rail.window.read(|home, _| {
        let row = home.user_message_navigation[index].row_index;
        f32::from(home.conversation_list.item_offset(row).expect("measured")) + 78.0 - margin
    })
}

#[test]
fn alt_arrows_step_between_prompts_without_the_highlight() {
    let mut rail = Rail::open();
    rail.window
        .update(|home, _, cx| home.jump_to_user_message_for_capture(6, cx));
    rail.settle();
    assert!((rail.transcript_offset() - landing(&rail, 6, 16.0)).abs() < 0.5);

    // The prompt at the top (16 px, inside the 24 px tolerance) is the current
    // one, so Alt+↑ goes to the prompt before it, smoothly.
    let stepped = rail.window.update(|home, window, cx| {
        home.step_user_message(super::MessageStep::Previous, window, cx)
    });
    assert!(stepped);
    rail.window.read(|home, cx| {
        assert!(home.navigation_rail.transcript_scroll.is_some());
        let row = home.user_message_navigation[5].row_index;
        assert!(home.user_message_highlight(row, cx).is_none());
        assert!(!home.navigation_rail.card_open);
    });
    for _ in 0..60 {
        rail.wait(Duration::from_millis(16));
    }
    assert!((rail.transcript_offset() - landing(&rail, 5, 16.0)).abs() < 0.5);

    // Alt+↓ goes back to the next prompt below the top.
    let target = rail
        .window
        .read(|home, _| home.user_message_step_target(super::MessageStep::Next));
    assert_eq!(target, Some(6));
    rail.window
        .update(|home, window, cx| home.step_user_message(super::MessageStep::Next, window, cx));
    for _ in 0..60 {
        rail.wait(Duration::from_millis(16));
    }
    let (offset, want) = (rail.transcript_offset(), landing(&rail, 6, 16.0));
    assert!(
        (offset - want).abs() < 0.5,
        "offset {offset}, landing {want}"
    );
    rail.finish();
}

#[test]
fn a_steering_prompt_lands_at_the_top_without_the_scroll_margin() {
    let mut rail = Rail::open_history(history_with_steering(Some(5)), TURNS + 1);
    let steer = rail.window.read(|home, _| {
        home.user_message_navigation
            .iter()
            .position(|item| item.label == "steer 5")
            .expect("steering prompt")
    });
    let first = steer - 1;
    rail.window
        .read(|home, _| assert_eq!(home.user_message_navigation[first].label, "prompt 5"));
    rail.window
        .update(|home, _, cx| home.jump_to_user_message_for_capture(steer, cx));
    rail.settle();
    assert!((rail.transcript_offset() - landing(&rail, steer, 0.0)).abs() < 0.5);
    rail.window
        .update(|home, _, cx| home.jump_to_user_message_for_capture(first, cx));
    rail.settle();
    assert!((rail.transcript_offset() - landing(&rail, first, 16.0)).abs() < 0.5);
    rail.finish();
}
