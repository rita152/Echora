//! Custom sections in the workspace store: what loads, what each change
//! sends, and what the preferences keep.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use super::{
    SectionItem, WorkspaceStore,
    sections_fake::{PINNED_ID, SectionsBackend},
};

fn preferences_path() -> PathBuf {
    static SERIAL: AtomicU64 = AtomicU64::new(1);
    std::env::temp_dir()
        .join(format!(
            "gpui-sections-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ))
        .join("preferences.json")
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for the store");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn loaded(backend: std::sync::Arc<SectionsBackend>) -> (std::sync::Arc<WorkspaceStore>, PathBuf) {
    let path = preferences_path();
    let store = WorkspaceStore::with_preferences_path(backend, path.clone());
    store.refresh_all();
    wait_until(|| {
        let snapshot = store.snapshot();
        !snapshot.loading.recent
            && !snapshot.loading.pinned
            && !snapshot.loading.sections
            && snapshot.custom_sections.len() == 2
    });
    (store, path)
}

fn names(store: &WorkspaceStore) -> Vec<String> {
    store
        .snapshot()
        .custom_sections
        .iter()
        .map(|section| section.section.name.clone())
        .collect()
}

#[test]
fn custom_sections_are_every_section_but_pinned_with_their_chats() {
    let backend = SectionsBackend::seeded();
    let (store, path) = loaded(backend.clone());
    let snapshot = store.snapshot();
    assert_eq!(names(&store), ["Work", "Later"]);
    assert_eq!(
        snapshot.preferences.pinned_section_id.as_deref(),
        Some(PINNED_ID)
    );
    assert_eq!(
        snapshot.custom_sections[0]
            .threads
            .iter()
            .map(|thread| thread.thread_id.as_str())
            .collect::<Vec<_>>(),
        ["a"]
    );
    assert!(snapshot.custom_sections[1].threads.is_empty());
    assert_eq!(
        snapshot.custom_section_of_thread("a").map(String::as_str),
        Some("work")
    );
    assert_eq!(snapshot.custom_section_of_thread("c"), None);
    // The order is recorded, and a section the server no longer has drops
    // out of every preference on the next read.
    assert_eq!(snapshot.preferences.section_order, ["work", "later"]);
    store.set_custom_section_collapsed("later", true);
    backend
        .state
        .lock()
        .unwrap()
        .sections
        .retain(|section| section.section_id != "later");
    store.refresh_custom_sections();
    wait_until(|| store.snapshot().custom_sections.len() == 1);
    let snapshot = store.snapshot();
    assert_eq!(snapshot.preferences.section_order, ["work"]);
    assert!(snapshot.preferences.collapsed_section_ids.is_empty());
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_new_section_takes_its_first_chat_or_project_and_an_empty_name_is_the_default() {
    let backend = SectionsBackend::seeded();
    let (store, path) = loaded(backend.clone());
    store.create_custom_section("  Research ".into(), Some(SectionItem::Thread("c".into())));
    wait_until(|| {
        store
            .snapshot()
            .custom_sections
            .iter()
            .any(|section| section.threads.iter().any(|thread| thread.thread_id == "c"))
    });
    let log = backend.log();
    assert!(log.contains(&"threadSection/create:Research".to_owned()));
    assert!(log.contains(&"thread/section/move:c:Some(\"new-1\")".to_owned()));
    assert_eq!(
        names(&store),
        ["Work", "Later", "Research"],
        "new sections go last"
    );
    // A project joins in the preferences only.
    crate::i18n::set_language(crate::i18n::Language::English);
    store.create_custom_section("".into(), Some(SectionItem::Project("p".into())));
    wait_until(|| store.snapshot().custom_section_of_project("p").is_some());
    crate::i18n::set_language(crate::i18n::Language::SimplifiedChinese);
    let snapshot = store.snapshot();
    assert_eq!(snapshot.custom_sections[3].section.name, "New section");
    assert_eq!(snapshot.custom_sections[3].projects, ["p"]);
    assert_eq!(
        snapshot.custom_section_of_project("p").map(String::as_str),
        Some("new-2")
    );
    assert!(!backend.log().iter().any(|entry| entry.contains("move:p")));
    // Its "Archive chats" and "Mark all as read" reach the project's chats.
    assert_eq!(snapshot.custom_section_thread_ids("new-2"), ["b"]);
    // The preferences survive a restart (the save runs on the worker).
    wait_until(|| {
        WorkspaceStore::with_preferences_path(backend.clone(), path.clone())
            .snapshot()
            .preferences
            .section_projects
            .get("new-2")
            .cloned()
            == Some(vec!["p".to_owned()])
    });
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn rename_and_delete_reach_the_server_and_a_failure_is_reported() {
    let backend = SectionsBackend::seeded();
    let (store, path) = loaded(backend.clone());
    store.rename_custom_section("work".into(), " Work stuff ".into());
    wait_until(|| names(&store)[0] == "Work stuff");
    assert!(
        backend
            .log()
            .contains(&"threadSection/update:work:Work stuff".to_owned())
    );
    // An empty name is never sent.
    store.rename_custom_section("work".into(), "   ".into());
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        backend
            .log()
            .iter()
            .filter(|e| e.starts_with("threadSection/update"))
            .count(),
        1
    );
    backend.state.lock().unwrap().fail_next = Some("thread section not found: work".into());
    store.rename_custom_section("work".into(), "Other".into());
    wait_until(|| store.snapshot().error.is_some());
    assert_eq!(
        names(&store)[0],
        "Work stuff",
        "a failed rename keeps the name"
    );
    // Removing keeps the chats: they return to Recents.
    store.move_to_custom_section(SectionItem::Project("p".into()), Some("work".into()));
    store.set_custom_section_collapsed("work", true);
    store.delete_custom_section("work".into());
    wait_until(|| names(&store) == ["Later"]);
    assert!(
        backend
            .log()
            .contains(&"threadSection/delete:work".to_owned())
    );
    let snapshot = store.snapshot();
    assert!(
        !snapshot
            .preferences
            .section_order
            .contains(&"work".to_owned())
    );
    assert!(snapshot.preferences.section_projects.is_empty());
    assert!(snapshot.preferences.collapsed_section_ids.is_empty());
    wait_until(|| {
        store
            .snapshot()
            .recent_threads
            .iter()
            .any(|thread| thread.thread_id == "a" && thread.section.is_none())
    });
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn chats_move_on_the_server_projects_locally_and_sections_reorder() {
    let backend = SectionsBackend::seeded();
    let (store, path) = loaded(backend.clone());
    store.move_to_custom_section(SectionItem::Thread("a".into()), None);
    wait_until(|| store.snapshot().custom_sections[0].threads.is_empty());
    assert!(
        backend
            .log()
            .contains(&"thread/section/move:a:None".to_owned())
    );
    store.move_to_custom_section(SectionItem::Thread("c".into()), Some("later".into()));
    wait_until(|| {
        store
            .snapshot()
            .custom_section_of_thread("c")
            .map(String::as_str)
            == Some("later")
    });
    // A project is in at most one section.
    store.move_to_custom_section(SectionItem::Project("p".into()), Some("work".into()));
    store.move_to_custom_section(SectionItem::Project("p".into()), Some("later".into()));
    let snapshot = store.snapshot();
    assert!(snapshot.custom_sections[0].projects.is_empty());
    assert_eq!(snapshot.custom_sections[1].projects, ["p"]);
    store.move_to_custom_section(SectionItem::Project("p".into()), None);
    assert!(store.snapshot().preferences.section_projects.is_empty());
    // Reorder: "later" before "work", then back to the end.
    store.move_custom_section("later", Some("work"));
    assert_eq!(names(&store), ["Later", "Work"]);
    store.move_custom_section("later", None);
    assert_eq!(names(&store), ["Work", "Later"]);
    assert_eq!(
        store.snapshot().preferences.section_order,
        ["work", "later"]
    );
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
