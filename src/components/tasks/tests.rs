use super::*;
use crate::agent::*;
use gpui::TestApp;

struct Offline;
fn offline<T>() -> async_channel::Receiver<Result<T, String>> {
    let (tx, rx) = async_channel::bounded(1);
    tx.try_send(Err("offline".into())).ok();
    rx
}
impl AgentBackend for Offline {
    fn subscribe_connection_events(&self) -> async_channel::Receiver<AgentConnectionEvent> {
        async_channel::unbounded().1
    }
    fn load_model_catalog(&self) -> async_channel::Receiver<Result<AgentModelCatalog, String>> {
        offline()
    }
    fn load_permission_profiles(
        &self,
        _: std::path::PathBuf,
    ) -> async_channel::Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        offline()
    }
    fn update_thread_permissions(
        &self,
        _: AgentThreadPermissionUpdate,
    ) -> async_channel::Receiver<Result<AgentThreadPermissionResult, String>> {
        offline()
    }
    fn run_prompt(&self, _: AgentRequest) -> AgentRun {
        panic!("An offline draft must not execute")
    }
}
fn setup(app: &mut TestApp) -> (Arc<WorkspaceStore>, Entity<TasksView>, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("{}.json", crate::tasks::new_id()));
    let store = WorkspaceStore::with_preferences_path(Arc::new(Offline), path.clone());
    let view = app.new_entity(|cx| TasksView::new(ThemeMode::Light, store.clone(), cx));
    (store, view, path)
}
fn draft() -> TaskDraft {
    TaskDraft {
        id: "draft".into(),
        project_id: "project".into(),
        cwd: "/tmp".into(),
        thread_id: None,
        client_message_id: None,
        created_at: 1,
        input: crate::tasks::TaskInput {
            text: "preserve me".into(),
            ..Default::default()
        },
    }
}

#[test]
fn waiting_dispatch_has_one_owner_and_error_keeps_the_durable_input_for_retry() {
    let mut app = TestApp::new();
    let (store, view, path) = setup(&mut app);
    let draft = draft();
    store.update_tasks(|p| p.drafts.push(draft.clone()));
    let composer = app
        .new_entity(|cx| ComposerView::new_with_backend(ThemeMode::Light, Arc::new(Offline), cx));
    app.update_entity(&composer, |c, cx| c.restore_task_input(&draft.input, cx));
    app.update_entity(&view, |v, cx| {
        v.launch(draft.clone(), composer.clone(), false, cx);
        let attempt = v.launching[&draft.id].attempt.clone();
        v.launch(draft.clone(), composer.clone(), false, cx);
        assert_eq!(v.launching.len(), 1);
        assert_eq!(v.launching[&draft.id].attempt, attempt);
        v.launch_failed(&draft.id, "offline".into(), cx);
        assert!(!v.is_launching(&draft.id));
        assert_eq!(store.snapshot().preferences.tasks.drafts[0], draft);
        v.launch(draft.clone(), composer.clone(), false, cx);
        assert_ne!(v.launching[&draft.id].attempt, attempt);
    });
    let restored = WorkspaceStore::with_preferences_path(Arc::new(Offline), path.clone());
    assert_eq!(restored.snapshot().preferences.tasks.drafts, vec![draft]);
    std::fs::remove_file(path).ok();
}

#[test]
fn deleting_a_linked_draft_unlinks_the_todo_and_never_deletes_its_chat() {
    let mut app = TestApp::new();
    let (store, view, path) = setup(&mut app);
    store.update_tasks(|p| p.drafts.push(draft()));
    app.update_entity(&view, |v, cx| v.add_todo("a local task".into(), cx));
    store.update_tasks(|p| {
        p.todos[0].thread_id = Some("draft".into());
        p.draft_order
            .insert("project".into(), vec!["draft:draft".into()]);
    });
    app.update_entity(&view, |v, cx| {
        let mut card = v.todo_card(&store.snapshot().preferences.tasks.todos[0]);
        card.id = "draft:draft".into();
        card.draft_id = Some("draft".into());
        v.confirmation = Some(card);
        v.confirm_delete(cx);
    });
    let p = store.snapshot().preferences.tasks;
    assert!(p.drafts.is_empty());
    assert!(p.draft_order["project"].is_empty());
    assert_eq!(p.todos.len(), 1);
    assert!(p.todos[0].thread_id.is_none());
    app.update_entity(&view, |v, cx| {
        v.confirmation = Some(v.todo_card(&p.todos[0]));
        v.confirm_delete(cx);
    });
    assert!(store.snapshot().preferences.tasks.todos.is_empty());
    std::fs::remove_file(path).ok();
}

#[test]
fn renaming_a_selected_todo_updates_its_editor_before_the_next_autosave() {
    let mut app = TestApp::new();
    let (store, view, path) = setup(&mut app);
    app.update_entity(&view, |v, cx| {
        v.add_todo("old".into(), cx);
        let todo = store.snapshot().preferences.tasks.todos.remove(0);
        v.selected = Some(format!("todo:{}", todo.id));
        v.todo_title
            .update(cx, |i, cx| i.set_text_silently("old", cx));
        v.todo_rename = Some(todo.id);
        v.commit_rename("new".into(), cx);
        v.save_todo_details(cx);
    });
    assert_eq!(store.snapshot().preferences.tasks.todos[0].title, "new");
    std::fs::remove_file(path).ok();
}

#[gpui::test]
fn narrow_project_board_scroll_reaches_the_last_column(cx: &mut gpui::TestAppContext) {
    use gpui::{point, px, size};
    cx.executor().allow_parking();
    let path = std::env::temp_dir().join(format!("{}.json", crate::tasks::new_id()));
    let store = WorkspaceStore::with_preferences_path(Arc::new(Offline), path.clone());
    store.insert_project_for_test(Project {
        project_id: "project".into(),
        name: "Project".into(),
        roots: vec!["/tmp/project".into()],
        created_at: 1,
        updated_at: 1,
        recency_at: None,
        position: 0,
    });
    let handle = cx.add_window(|_, cx| {
        let mut view = TasksView::new(ThemeMode::Dark, store, cx);
        view.route.project = Some("project".into());
        view
    });
    let entity = handle.root(cx).unwrap();
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(720.), px(620.)));
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let scroll = visual.update(|_, cx| entity.read(cx).scroll.clone());
    assert!(scroll.bounds().size.width <= px(720.));
    assert!(scroll.max_offset().x > px(0.));
    let last = scroll.bounds_for_item(3).unwrap();
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: scroll.bounds().center(),
        delta: gpui::ScrollDelta::Pixels(point(px(-10_000.), px(0.))),
        touch_phase: gpui::TouchPhase::Moved,
        modifiers: Default::default(),
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(scroll.offset().x < px(0.));
    assert!(last.right() + scroll.offset().x <= scroll.bounds().right() + px(1.));
    visual.update(|_, cx| entity.update(cx, |view, cx| view.set_route(TaskRoute::default(), cx)));
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        scroll.offset(),
        point(px(0.), px(0.)),
        "The overview must not inherit a project's horizontal scroll"
    );
    std::fs::remove_file(path).ok();
}

#[gpui::test]
fn long_boards_scroll_to_the_end_before_and_after_expanding(cx: &mut gpui::TestAppContext) {
    use crate::tasks::BoardMode;
    use gpui::{point, px, size};
    cx.executor().allow_parking();
    let path = std::env::temp_dir().join(format!("{}.json", crate::tasks::new_id()));
    let store = WorkspaceStore::with_preferences_path(Arc::new(Offline), path.clone());
    store.insert_project_for_test(Project {
        project_id: "project".into(),
        name: "Project".into(),
        roots: vec!["/tmp/project".into()],
        created_at: 1,
        updated_at: 1,
        recency_at: None,
        position: 0,
    });
    store.update_tasks(|p| {
        p.drafts = (0..45)
            .map(|index| TaskDraft {
                id: format!("draft-{index}"),
                input: crate::tasks::TaskInput {
                    text: format!(
                        "历史任务 {index}: {}",
                        "Verify long mixed-language task titles. ".repeat(10)
                    ),
                    ..Default::default()
                },
                ..draft()
            })
            .collect();
    });
    let handle = cx.add_window(|_, cx| TasksView::new(ThemeMode::Dark, store.clone(), cx));
    let entity = handle.root(cx).unwrap();
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(720.), px(620.)));
    for mode in [BoardMode::Attention, BoardMode::Classic] {
        for project in [None, Some("project".to_owned())] {
            let mut collapsed_extent = px(0.);
            for expanded in [false, true] {
                store.update_tasks(|p| {
                    p.mode = mode;
                    p.expanded = expanded;
                });
                visual.update(|_, cx| {
                    entity.update(cx, |view, cx| {
                        view.set_route(
                            TaskRoute {
                                project: project.clone(),
                                list: false,
                            },
                            cx,
                        );
                        view.scroll.set_offset(point(px(0.), px(0.)));
                        cx.notify();
                    });
                });
                visual.run_until_parked();
                visual.update(|window, cx| window.draw(cx).clear(cx));
                let scroll_id = if project.is_some() {
                    "task-column-project-Draft"
                } else {
                    "task-project-cards-project"
                };
                let (scroll, board_scroll) = visual.update(|_, cx| {
                    let view = entity.read(cx);
                    (view.column_scrolls[scroll_id].clone(), view.scroll.clone())
                });
                scroll.set_offset(point(px(0.), px(0.)));
                let viewport = scroll.bounds();
                let section = board_scroll.bounds_for_item(0).unwrap();
                assert!(
                    viewport.top() > section.top(),
                    "Column header stays above the scrolling cards"
                );
                assert!(viewport.bottom() <= px(620.));
                let extent = scroll.max_offset().y;
                assert!(extent > px(0.), "{mode:?}, {project:?}, {expanded}");
                if expanded {
                    assert!(
                        extent > collapsed_extent,
                        "Expanded cards must extend the scroll range"
                    );
                } else {
                    collapsed_extent = extent;
                }
                visual.simulate_event(gpui::ScrollWheelEvent {
                    position: point(viewport.left() + px(140.), viewport.center().y),
                    delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
                    touch_phase: gpui::TouchPhase::Moved,
                    modifiers: Default::default(),
                });
                visual.update(|window, cx| window.draw(cx).clear(cx));
                assert!(scroll.offset().y < px(0.));
                let last = scroll
                    .bounds_for_item(if expanded {
                        44
                    } else if project.is_some() {
                        30
                    } else {
                        20
                    })
                    .unwrap();
                assert!(last.bottom() + scroll.offset().y <= viewport.bottom() + px(1.));
                assert_eq!(board_scroll.offset().y, px(0.));
                assert_eq!(board_scroll.bounds_for_item(0).unwrap(), section);
            }
        }
    }
    std::fs::remove_file(path).ok();
}

#[test]
fn leaving_list_details_flushes_pending_edits_before_clearing_selection() {
    let mut app = TestApp::new();
    let (store, view, path) = setup(&mut app);
    app.update_entity(&view, |v, cx| {
        v.add_todo("Original".into(), cx);
        v.route.list = true;
        let todo = store.snapshot().preferences.tasks.todos.remove(0);
        v.selected = Some(format!("todo:{}", todo.id));
        v.todo_title
            .update(cx, |i, cx| i.set_text_silently("Pending title", cx));
        v.set_route(TaskRoute::default(), cx);
        assert!(v.selected.is_none());
    });
    assert_eq!(
        store.snapshot().preferences.tasks.todos[0].title,
        "Pending title"
    );
    let restored = WorkspaceStore::with_preferences_path(Arc::new(Offline), path.clone());
    assert_eq!(
        restored.snapshot().preferences.tasks.todos[0].title,
        "Pending title"
    );
    std::fs::remove_file(path).ok();
}

struct ModalHost(Entity<TasksView>);
impl gpui::Render for ModalHost {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let overlay = self
            .0
            .update(cx, |view, cx| view.render_modal_overlay(window, cx));
        gpui::div()
            .size_full()
            .relative()
            .child(self.0.clone())
            .children(overlay)
    }
}

#[test]
fn escape_closes_project_and_task_submenus_before_discarding_the_editor() {
    let mut app = TestApp::new();
    let (_, view, path) = setup(&mut app);
    let composer = app
        .new_entity(|cx| ComposerView::new_with_backend(ThemeMode::Light, Arc::new(Offline), cx));
    let input = draft();
    app.update_entity(&composer, |c, cx| c.restore_task_input(&input.input, cx));
    app.update_entity(&view, |v, cx| {
        v.show_editor(input, composer.clone(), true, cx);
        v.editor.as_mut().unwrap().project_menu = true;
    });
    let mut window = app.open_window(|_, _| ModalHost(view.clone()));
    window.draw();
    window.simulate_keystrokes("escape");
    app.read_entity(&view, |v, cx| {
        assert!(!v.editor.as_ref().unwrap().project_menu);
        assert_eq!(
            v.editor
                .as_ref()
                .unwrap()
                .composer
                .read(cx)
                .task_input(cx)
                .text,
            "preserve me"
        );
    });
    window.simulate_keystrokes("tab enter");
    window.draw();
    window.simulate_keystrokes("escape");
    assert!(app.read_entity(&view, |v, _| v.editor.is_some()));
    window.simulate_keystrokes("escape");
    assert!(app.read_entity(&view, |v, _| v.editor.is_none()));
    std::fs::remove_file(path).ok();
}
