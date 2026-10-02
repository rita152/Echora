//! Browser panel behaviour through GPUI test windows, with headless pages:
//! a test window has no native handle, so every page is the headless
//! stand-in that "loads" URLs at once and keeps a back/forward list.

use gpui::{
    Bounds, Entity, KeyBinding, TestApp, TestAppWindow, WindowBounds, WindowOptions, point, px,
    size,
};

use super::{BrowserPanel, BrowserStore, PanelMenu};
use crate::{
    browser::{
        session::{SavedSession, SavedTab},
        webview::{NavigationError, WebViewEvent},
    },
    components::prompt_input::{Backspace, Submit},
    theme::ThemeMode,
};

fn open_panel(
    app: &mut TestApp,
    chat: Option<&str>,
    store: Entity<BrowserStore>,
) -> TestAppWindow<BrowserPanel> {
    app.update(|cx| {
        super::init(cx);
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, Some("PromptInput")),
            KeyBinding::new("enter", Submit, Some("PromptInput")),
        ]);
    });
    let chat = chat.map(ToOwned::to_owned);
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(640.0), px(800.0)),
            })),
            ..Default::default()
        },
        move |_, cx| {
            // As the right panel opens a browser: on its saved pages, or on a
            // New tab when it has none.
            let mut panel = BrowserPanel::new(ThemeMode::Dark, store, chat, cx);
            if panel.tabs.is_empty() {
                panel.new_tab(None, cx);
            }
            panel
        },
    );
    window.draw();
    window
}

fn store_with_history(app: &mut TestApp) -> Entity<BrowserStore> {
    let store = app.new_entity(|_| BrowserStore::in_memory());
    app.update_entity(&store, |store, cx| {
        store.record_visit("https://example.net/", "Example Domain", cx);
        store.record_visit("https://example.com/docs", "Docs", cx);
    });
    store
}

fn settle(app: &mut TestApp, window: &mut TestAppWindow<BrowserPanel>) {
    for _ in 0..3 {
        window.draw();
        app.run_until_parked();
    }
}

fn focus_address(app: &mut TestApp, window: &mut TestAppWindow<BrowserPanel>) {
    window.update(|panel, _, cx| {
        panel.focus_address_pending = true;
        cx.notify();
    });
    settle(app, window);
    assert!(window.read(|panel, _| panel.address_state.focused));
}

#[test]
fn typing_completes_a_visited_host_and_enter_opens_it() {
    let mut app = TestApp::new();
    let store = store_with_history(&mut app);
    let mut window = open_panel(&mut app, None, store.clone());
    focus_address(&mut app, &mut window);

    window.simulate_input("exa");
    let (text, selection, open, rows) = window.read(|panel, cx| {
        let input = panel.address.read(cx);
        (
            input.text().to_owned(),
            input.selected_range(),
            panel.address_state.open,
            panel.address_state.suggestions.rows.len(),
        )
    });
    // The reference's "exa" → "exa|mple.net", the completion selected.
    assert_eq!(text, "example.net");
    assert_eq!(selection, 3..11);
    assert!(open);
    assert_eq!(rows, 3);

    window.simulate_keystroke("enter");
    settle(&mut app, &mut window);
    let (url, title, open) = window.read(|panel, _| {
        let tab = panel.active_tab().unwrap();
        (tab.url.clone(), tab.title.clone(), panel.address_state.open)
    });
    assert_eq!(url, "https://example.net/");
    assert_eq!(title, "https://example.net/");
    assert!(!open);
    let visits = app.read_entity(&store, |store, _| {
        store
            .history
            .top_sites(8)
            .into_iter()
            .find(|site| site.url == "https://example.net/")
            .map(|site| site.visit_count)
    });
    assert_eq!(visits, Some(2));
}

#[test]
fn deleting_does_not_complete_and_escape_restores_the_typed_text() {
    let mut app = TestApp::new();
    let store = store_with_history(&mut app);
    let mut window = open_panel(&mut app, None, store);
    focus_address(&mut app, &mut window);

    window.simulate_input("exa");
    // Backspace removes the selected completion; nothing is completed again.
    window.simulate_keystroke("backspace");
    assert_eq!(
        window.read(|panel, cx| panel.address.read(cx).text().to_owned()),
        "exa"
    );
    window.simulate_keystroke("backspace");
    assert_eq!(
        window.read(|panel, cx| panel.address.read(cx).text().to_owned()),
        "ex"
    );

    // Down highlights the next row and shows its text; Escape restores what
    // was typed and closes the list.
    window.simulate_keystroke("down");
    assert_eq!(
        window.read(|panel, _| panel.address_state.highlighted),
        Some(1)
    );
    window.simulate_keystroke("escape");
    let (text, open) = window.read(|panel, cx| {
        (
            panel.address.read(cx).text().to_owned(),
            panel.address_state.open,
        )
    });
    assert_eq!(text, "ex");
    assert!(!open);
}

#[test]
fn back_and_forward_follow_the_page_history() {
    let mut app = TestApp::new();
    let store = app.new_entity(|_| BrowserStore::in_memory());
    let mut window = open_panel(&mut app, None, store);
    window.update(|panel, _, cx| panel.open_url("https://a.example.com/".into(), cx));
    settle(&mut app, &mut window);
    window.update(|panel, _, cx| panel.navigate("https://b.example.com/".into(), cx));
    settle(&mut app, &mut window);
    assert!(window.read(|panel, _| panel.active_tab().unwrap().can_go_back()));

    window.update(|panel, _, cx| panel.go_back(cx));
    settle(&mut app, &mut window);
    let (url, forward) = window.read(|panel, _| {
        let tab = panel.active_tab().unwrap();
        (tab.url.clone(), tab.can_go_forward)
    });
    assert_eq!(url, "https://a.example.com/");
    assert!(forward);

    window.update(|panel, _, cx| panel.go_forward(cx));
    settle(&mut app, &mut window);
    assert_eq!(
        window.read(|panel, _| panel.active_tab().unwrap().url.clone()),
        "https://b.example.com/"
    );
}

#[test]
fn a_failed_load_shows_the_error_page_until_reload_succeeds() {
    let mut app = TestApp::new();
    let store = app.new_entity(|_| BrowserStore::in_memory());
    let mut window = open_panel(&mut app, None, store);
    window.update(|panel, _, cx| panel.open_url("https://nope.invalid/".into(), cx));
    settle(&mut app, &mut window);
    window.update(|panel, _, cx| {
        let id = panel.active_tab().unwrap().id;
        panel.handle_event(
            id,
            WebViewEvent::Failed(NavigationError {
                url: "https://nope.invalid/".into(),
                code: "ERR_NAME_NOT_RESOLVED".into(),
                description: String::new(),
            }),
            cx,
        );
    });
    window.draw();
    let page = window.read(|panel, _| panel.active_tab().unwrap().error.clone().map(|e| e.page));
    let page = page.expect("error page");
    assert_eq!(page.error_code.as_deref(), Some("ERR_NAME_NOT_RESOLVED"));
    assert!(!window.read(|panel, _| panel.showing_page()));

    // Reload loads the failed address again; the headless page succeeds.
    window.update(|panel, _, cx| panel.reload(cx));
    settle(&mut app, &mut window);
    assert!(window.read(|panel, _| panel.active_tab().unwrap().error.is_none()));
    assert!(window.read(|panel, _| panel.showing_page()));
}

/// The right panel's strip owns the tabs: closing the browser's last tab
/// leaves it without one, and a New tab can open again afterwards.
#[test]
fn closing_the_last_tab_leaves_no_tab_until_one_opens() {
    let mut app = TestApp::new();
    let store = app.new_entity(|_| BrowserStore::in_memory());
    let mut window = open_panel(&mut app, None, store);
    window.update(|panel, _, cx| panel.new_tab(Some("https://example.net/".into()), cx));
    settle(&mut app, &mut window);
    assert_eq!(window.read(|panel, _| panel.tabs.len()), 2);
    window.update(|panel, _, cx| panel.close_tab(1, cx));
    window.update(|panel, _, cx| panel.close_tab(0, cx));
    app.run_until_parked();
    assert!(window.read(|panel, _| panel.tabs.is_empty()));
    let id = window.update(|panel, _, cx| panel.new_tab(None, cx));
    assert!(window.read(|panel, _| panel.is_new_tab(id)));
    assert_eq!(window.read(|panel, _| panel.active_tab_id()), Some(id));
}

#[test]
fn a_chat_reopens_its_saved_tabs_cold() {
    let mut app = TestApp::new();
    let store = app.new_entity(|_| BrowserStore::in_memory());
    app.update_entity(&store, |store, _| {
        store.save_session(
            "thread-1",
            SavedSession {
                tabs: vec![
                    SavedTab {
                        url: "https://a.example.com/".into(),
                        title: "A".into(),
                        custom_title: None,
                    },
                    SavedTab {
                        url: "https://b.example.com/".into(),
                        title: "B".into(),
                        custom_title: Some("Renamed".into()),
                    },
                ],
                active: 1,
                updated_ms: 1,
            },
        )
    });
    let mut window = open_panel(&mut app, Some("thread-1"), store);
    let tabs = window.read(|panel, _| panel.tab_summaries());
    assert_eq!(
        tabs,
        vec![
            ("A".to_owned(), "https://a.example.com/".to_owned()),
            ("Renamed".to_owned(), "https://b.example.com/".to_owned()),
        ]
    );
    // Only the shown tab gets a page; the other stays cold until selected.
    settle(&mut app, &mut window);
    let pages = window.read(|panel, _| {
        panel
            .tabs
            .iter()
            .map(|tab| tab.view.is_some())
            .collect::<Vec<_>>()
    });
    assert_eq!(pages, vec![false, true]);
}

#[test]
fn zoom_steps_through_the_chromium_levels() {
    let mut app = TestApp::new();
    let store = app.new_entity(|_| BrowserStore::in_memory());
    let mut window = open_panel(&mut app, None, store);
    window.update(|panel, _, cx| panel.open_url("https://example.net/".into(), cx));
    settle(&mut app, &mut window);
    let zoom = |window: &mut TestAppWindow<BrowserPanel>, delta: i32| {
        window.update(|panel, _, cx| {
            panel.step_zoom(delta, cx);
            panel.active_tab().unwrap().zoom
        })
    };
    assert_eq!(zoom(&mut window, 1), 1.1);
    assert_eq!(zoom(&mut window, -1), 1.0);
    assert_eq!(zoom(&mut window, -1), 0.9);
    assert_eq!(zoom(&mut window, 0), 1.0);
    assert!(window.read(|panel, _| panel.zoom_banner.is_some()));
}

#[test]
fn find_reports_matches_and_menus_close_on_escape() {
    let mut app = TestApp::new();
    let store = app.new_entity(|_| BrowserStore::in_memory());
    let mut window = open_panel(&mut app, None, store);
    window.update(|panel, _, cx| panel.open_url("https://example.net/".into(), cx));
    settle(&mut app, &mut window);
    window.update(|panel, window, cx| {
        panel.open_find(window, cx);
        panel.run_find("example".into(), false);
    });
    settle(&mut app, &mut window);
    let result = window.read(|panel, _| panel.active_tab().unwrap().find);
    assert_eq!((result.matches, result.active), (1, 1));
    window.update(|panel, _, cx| panel.close_find(cx));
    assert!(!window.read(|panel, _| panel.find.open));

    window.update(|panel, window, cx| {
        panel.toggle_menu(PanelMenu::Options, cx);
        window.focus(&panel.focus, cx);
    });
    window.draw();
    assert_eq!(window.read(|panel, _| panel.menu), Some(PanelMenu::Options));
    window.simulate_keystroke("escape");
    assert_eq!(window.read(|panel, _| panel.menu), None);
}
