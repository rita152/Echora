//! Custom sections in the rendered sidebar: where they sit, their heading's
//! hit areas and options menu, the rows' section menu items, the dialog, and
//! drag and drop — all through real pointer events on the drawn bounds.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use gpui::{Modifiers, MouseButton, Point, VisualTestContext, px};

use super::{NewChatInSection, SidebarView};
use crate::{
    theme::ThemeMode,
    workspace::{
        SectionItem, WorkspaceStore,
        sections_fake::{PINNED_ID, SectionsBackend},
    },
};

fn preferences_path() -> PathBuf {
    static SERIAL: AtomicU64 = AtomicU64::new(1);
    std::env::temp_dir()
        .join(format!(
            "gpui-sidebar-sections-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ))
        .join("preferences.json")
}

fn wait_for(
    store: &WorkspaceStore,
    condition: impl Fn(&crate::workspace::WorkspaceSnapshot) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition(&store.snapshot()) {
        assert!(Instant::now() < deadline, "timed out waiting for the store");
        std::thread::sleep(Duration::from_millis(5));
    }
}

struct Harness {
    backend: Arc<SectionsBackend>,
    store: Arc<WorkspaceStore>,
    visual: VisualTestContext,
    new_chats: Arc<Mutex<Vec<String>>>,
    path: PathBuf,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.path.parent().unwrap());
    }
}

impl Harness {
    fn open(cx: &mut gpui::TestAppContext) -> Self {
        // The store runs its reads on real threads, which then wake the
        // view's snapshot task.
        cx.executor().allow_parking();
        let backend = SectionsBackend::seeded();
        let path = preferences_path();
        let store = WorkspaceStore::with_preferences_path(backend.clone(), path.clone());
        store.refresh_all();
        wait_for(&store, |snapshot| {
            !snapshot.loading.recent
                && !snapshot.loading.sections
                && snapshot.custom_sections.len() == 2
        });
        let view_store = store.clone();
        let window =
            cx.add_window(|_, cx| SidebarView::new(ThemeMode::Dark, false, view_store, cx));
        let new_chats = Arc::new(Mutex::new(Vec::new()));
        let seen = new_chats.clone();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| {
            window.resize(gpui::size(px(240.0), px(900.0)));
            let sidebar = window.root::<SidebarView>().flatten().unwrap();
            cx.subscribe(&sidebar, move |_, event: &NewChatInSection, _| {
                seen.lock().unwrap().push(event.0.clone());
            })
            .detach();
        });
        let mut harness = Self {
            backend,
            store,
            visual,
            new_chats,
            path,
        };
        harness.draw();
        harness
    }

    /// Lets the view take the store's latest snapshot, then draws it.
    fn draw(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            self.visual.run_until_parked();
            let current = self.store.snapshot();
            let seen = self.visual.update(|window, cx| {
                let sidebar = window.root::<SidebarView>().flatten().unwrap();
                sidebar.read(cx).snapshot == current
            });
            if seen || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.visual.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn bounds(&mut self, selector: &'static str) -> gpui::Bounds<gpui::Pixels> {
        self.visual
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is not drawn"))
    }

    fn click(&mut self, selector: &'static str) {
        let center = self.bounds(selector).center();
        self.visual
            .simulate_mouse_move(center, None, Modifiers::default());
        self.visual.simulate_click(center, Modifiers::default());
        self.draw();
    }

    fn hover(&mut self, selector: &'static str) {
        let center = self.bounds(selector).center();
        self.visual
            .simulate_mouse_move(center, None, Modifiers::default());
        self.draw();
    }

    fn sidebar<R>(&mut self, read: impl FnOnce(&SidebarView) -> R) -> R {
        self.visual.update(|window, cx| {
            let sidebar = window.root::<SidebarView>().flatten().unwrap();
            read(sidebar.read(cx))
        })
    }

    fn update<R>(
        &mut self,
        update: impl FnOnce(&mut SidebarView, &mut gpui::Context<SidebarView>) -> R,
    ) -> R {
        self.visual.update(|window, cx| {
            let sidebar = window.root::<SidebarView>().flatten().unwrap();
            sidebar.update(cx, update)
        })
    }

    fn settle_store(&mut self, condition: impl Fn(&crate::workspace::WorkspaceSnapshot) -> bool) {
        wait_for(&self.store, condition);
        self.draw();
    }
}

#[gpui::test]
fn custom_sections_sit_between_pinned_and_projects_and_hold_their_items(
    cx: &mut gpui::TestAppContext,
) {
    let mut h = Harness::open(cx);
    let work = h.bounds("CUSTOM_SECTION_work");
    let later = h.bounds("CUSTOM_SECTION_later");
    assert!(work.origin.y < later.origin.y, "the preferred order");
    // Work holds chat `a`, which Recents therefore leaves out; Later is empty.
    let chat = h.bounds("THREAD_ROW_a");
    assert!(chat.origin.y > work.origin.y && chat.origin.y < later.origin.y);
    assert!(
        h.visual
            .debug_bounds("CUSTOM_SECTION_EMPTY_later")
            .is_some()
    );
    assert!(h.visual.debug_bounds("CUSTOM_SECTION_EMPTY_work").is_none());
    let recent_rows = h.sidebar(|sidebar| sidebar.sectioned_thread_ids().len());
    assert_eq!(recent_rows, 1);
    assert_eq!(
        h.store.snapshot().preferences.pinned_section_id.as_deref(),
        Some(PINNED_ID)
    );
}

#[gpui::test]
fn the_heading_collapses_on_its_title_and_opens_its_menu_on_hover(cx: &mut gpui::TestAppContext) {
    let mut h = Harness::open(cx);
    h.click("CUSTOM_SECTION_TOGGLE_work");
    h.settle_store(|snapshot| snapshot.preferences.collapsed_section_ids.contains("work"));
    assert!(h.visual.debug_bounds("THREAD_ROW_a").is_none(), "collapsed");
    h.click("CUSTOM_SECTION_TOGGLE_work");
    h.settle_store(|snapshot| snapshot.preferences.collapsed_section_ids.is_empty());
    assert!(h.visual.debug_bounds("THREAD_ROW_a").is_some());
    // The options button is on the heading's right edge, shown on hover.
    h.hover("CUSTOM_SECTION_TOGGLE_later");
    let heading = h.bounds("CUSTOM_SECTION_later");
    let options = h.bounds("CUSTOM_SECTION_OPTIONS_later");
    assert!(options.origin.x > heading.origin.x + heading.size.width / 2.0);
    h.click("CUSTOM_SECTION_OPTIONS_later");
    assert_eq!(
        h.sidebar(|sidebar| sidebar.section_menu_id.clone())
            .as_deref(),
        Some("later")
    );
    // "New chat in Later" asks the host for a draft in the section.
    h.click("CUSTOM_SECTION_MENU_NEW_CHAT");
    assert_eq!(h.new_chats.lock().unwrap().as_slice(), ["later"]);
    assert_eq!(h.sidebar(|sidebar| sidebar.section_menu_id.clone()), None);
}

#[gpui::test]
fn edit_opens_the_dialog_with_the_name_and_remove_deletes_the_section(
    cx: &mut gpui::TestAppContext,
) {
    let mut h = Harness::open(cx);
    h.hover("CUSTOM_SECTION_TOGGLE_work");
    h.click("CUSTOM_SECTION_OPTIONS_work");
    h.click("CUSTOM_SECTION_MENU_EDIT");
    let dialog = h.sidebar(|sidebar| sidebar.section_dialog()).unwrap();
    assert_eq!(dialog.editing.as_deref(), Some("work"));
    assert_eq!(dialog.name, "Work");
    h.update(|sidebar, cx| {
        assert!(sidebar.section_dialog_submittable(cx));
        sidebar
            .section_name_input
            .update(cx, |input, cx| input.set_rename_text("  ", cx));
        assert!(
            !sidebar.section_dialog_submittable(cx),
            "editing needs a name"
        );
        sidebar.commit_section_dialog("  ".into(), cx);
        assert!(
            sidebar.section_dialog().is_some(),
            "an empty rename is not sent"
        );
        sidebar.commit_section_dialog("Deep work".into(), cx);
        assert!(sidebar.section_dialog().is_none());
    });
    h.settle_store(|snapshot| snapshot.custom_sections[0].section.name == "Deep work");
    h.hover("CUSTOM_SECTION_TOGGLE_work");
    h.click("CUSTOM_SECTION_OPTIONS_work");
    h.click("CUSTOM_SECTION_MENU_REMOVE");
    h.settle_store(|snapshot| snapshot.custom_sections.len() == 1);
    assert!(
        h.backend
            .log()
            .contains(&"threadSection/delete:work".to_owned())
    );
    assert!(h.visual.debug_bounds("CUSTOM_SECTION_work").is_none());
}

#[gpui::test]
fn a_row_menu_moves_its_chat_into_a_section_or_starts_a_new_one(cx: &mut gpui::TestAppContext) {
    let mut h = Harness::open(cx);
    let row = h.bounds("THREAD_ROW_c").center();
    h.visual
        .simulate_mouse_move(row, None, Modifiers::default());
    h.visual
        .simulate_mouse_down(row, MouseButton::Right, Modifiers::default());
    h.visual
        .simulate_mouse_up(row, MouseButton::Right, Modifiers::default());
    h.draw();
    assert_eq!(
        h.sidebar(|s| s.thread_menu_id.clone()).as_deref(),
        Some("c")
    );
    h.click("SECTION_MOVE_later");
    h.settle_store(|snapshot| {
        snapshot.custom_section_of_thread("c").map(String::as_str) == Some("later")
    });
    assert!(
        h.backend
            .log()
            .contains(&"thread/section/move:c:Some(\"later\")".to_owned())
    );
    // "New section…" opens the dialog with the chat as its first item.
    let row = h.bounds("THREAD_ROW_c").center();
    h.visual
        .simulate_mouse_down(row, MouseButton::Right, Modifiers::default());
    h.visual
        .simulate_mouse_up(row, MouseButton::Right, Modifiers::default());
    h.draw();
    h.click("SECTION_MOVE_NEW");
    let dialog = h.sidebar(|s| s.section_dialog()).unwrap();
    assert_eq!(dialog.editing, None);
    assert_eq!(dialog.item, Some(SectionItem::Thread("c".into())));
}

#[gpui::test]
fn dragging_a_chat_onto_a_section_moves_it_and_onto_recents_takes_it_out(
    cx: &mut gpui::TestAppContext,
) {
    let mut h = Harness::open(cx);
    let from = h.bounds("THREAD_ROW_c").center();
    let to = h.bounds("CUSTOM_SECTION_EMPTY_later").center();
    drag(&mut h.visual, from, to);
    h.settle_store(|snapshot| {
        snapshot.custom_section_of_thread("c").map(String::as_str) == Some("later")
    });
    // Out again: dropped on the Recents area.
    let from = h.bounds("THREAD_ROW_c").center();
    let recent = h
        .visual
        .debug_bounds("THREAD_ROW_b")
        .map(|bounds| bounds.center())
        .unwrap_or_else(|| {
            let later = h.visual.debug_bounds("CUSTOM_SECTION_later").unwrap();
            gpui::point(
                later.center().x,
                later.origin.y + later.size.height + px(200.0),
            )
        });
    drag(&mut h.visual, from, recent);
    h.settle_store(|snapshot| snapshot.custom_section_of_thread("c").is_none());
    assert!(
        h.backend
            .log()
            .contains(&"thread/section/move:c:None".to_owned())
    );
}

fn drag(visual: &mut VisualTestContext, from: Point<gpui::Pixels>, to: Point<gpui::Pixels>) {
    visual.simulate_mouse_move(from, None, Modifiers::default());
    visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    let steps = 6;
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let point = gpui::point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        visual.simulate_mouse_move(point, Some(MouseButton::Left), Modifiers::default());
    }
    visual.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
}
