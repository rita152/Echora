//! The "Edit goal" tab: the thread goal's objective as an editable document,
//! as the reference opens it beside the chat. Save writes only the objective;
//! Revert restores the saved text. The composer owns the goal and reports its
//! state back through [`FilePanel::sync_goal`].

use std::path::PathBuf;

use gpui::{Context, Div, EventEmitter, Role, div, prelude::*, px};

use super::{Document, FilePanel};
use crate::{
    agent::AgentThreadGoal,
    components::file_editor::{EditorEvent, FileEditor},
    theme::Theme,
};

/// Asks the thread's composer to save the edited objective.
pub struct SaveGoalObjective {
    pub thread_id: String,
    pub objective: String,
}

impl EventEmitter<SaveGoalObjective> for FilePanel {}

pub(super) struct GoalTab {
    thread_id: String,
    saved: String,
    updated_at: i64,
    saving: bool,
    error: Option<String>,
}

/// The composer's view of the goal, pushed to an open tab.
pub struct GoalTabSync<'a> {
    pub goal: Option<&'a AgentThreadGoal>,
    /// The objective as written, when known (a long one is a file pointer
    /// whose text may still be loading).
    pub text: Option<String>,
    pub saving: bool,
    pub error: Option<String>,
}

/// "Updated just now" under a minute, then whole minutes.
fn updated_label(updated_at: i64, now: i64) -> String {
    let minutes = (now - updated_at).max(0) / 60;
    if minutes == 0 {
        crate::i18n::format!("刚刚更新" => "Updated just now")
    } else {
        crate::i18n::format!("{minutes} 分钟前更新" => "Updated {minutes} min ago")
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

impl Document {
    pub(super) fn goal_dirty(&self, cx: &gpui::App) -> bool {
        self.goal
            .as_ref()
            .zip(self.editor.as_ref())
            .is_some_and(|(goal, editor)| editor.read(cx).text() != goal.saved)
    }
}

impl FilePanel {
    fn goal_document(&self, thread_id: &str) -> Option<u64> {
        self.documents
            .iter()
            .find(|d| d.goal.as_ref().is_some_and(|g| g.thread_id == thread_id))
            .map(|d| d.id)
    }

    /// Opens (or focuses) the thread's goal tab. When `toggle` is set and the
    /// tab is already the visible one, it closes instead, as the reference's
    /// edit control toggles the tab.
    pub fn open_goal(
        &mut self,
        goal: &AgentThreadGoal,
        text: &str,
        toggle: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.goal_document(&goal.thread_id) {
            if toggle && self.active == Some(id) {
                self.remove_document(id, cx);
                return;
            }
            self.active = Some(id);
            self.focus_editor = true;
            cx.notify();
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        let editor = cx.new(|cx| {
            let mut editor = FileEditor::prose(self.mode, "目标", cx);
            editor.set_text_metrics(14., 21., cx);
            editor.set_text_silently(text, cx);
            editor
        });
        cx.subscribe(&editor, move |panel, _, event: &EditorEvent, cx| {
            match event {
                EditorEvent::Save => panel.save_goal(id, cx),
                EditorEvent::Changed => {}
                EditorEvent::Submit | EditorEvent::SubmitInverted => {}
            }
            cx.notify();
        })
        .detach();
        self.documents.push(Document {
            id,
            path: PathBuf::from(crate::i18n::format!("编辑目标" => "Edit goal")),
            plan: None,
            terminal: None,
            goal: Some(GoalTab {
                thread_id: goal.thread_id.clone(),
                saved: text.to_owned(),
                updated_at: goal.updated_at,
                saving: false,
                error: None,
            }),
            editor: Some(editor),
            saved: None,
            error: None,
            loading: false,
            saving: false,
            revision: 0,
            image: false,
            preview: false,
            markdown: None,
            markdown_revision: None,
            markdown_pending_revision: None,
        });
        self.active = Some(id);
        self.focus_editor = true;
        cx.notify();
    }

    /// Follows the goal. The tab closes when the goal is cleared or complete,
    /// or when its objective changes to something other than this tab's own
    /// save, as the reference closes it.
    pub fn sync_goal(&mut self, thread_id: &str, sync: GoalTabSync, cx: &mut Context<Self>) {
        let Some(id) = self.goal_document(thread_id) else {
            return;
        };
        let Some(goal) = sync
            .goal
            .filter(|goal| goal.status != crate::agent::AgentThreadGoalStatus::Complete)
        else {
            self.remove_document(id, cx);
            return;
        };
        let Some(document) = self.documents.iter_mut().find(|d| d.id == id) else {
            return;
        };
        let (Some(tab), Some(editor)) = (document.goal.as_mut(), document.editor.as_ref()) else {
            return;
        };
        if let Some(text) = sync.text
            && tab.saved != text
        {
            if editor.read(cx).text().trim() != text {
                self.remove_document(id, cx);
                return;
            }
            tab.saved = text.clone();
            editor.update(cx, |editor, cx| editor.reload(text, cx));
        }
        tab.updated_at = goal.updated_at;
        tab.saving = sync.saving;
        tab.error = sync.error;
        cx.notify();
    }

    fn save_goal(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(document) = self.documents.iter_mut().find(|d| d.id == id) else {
            return;
        };
        let dirty = document.goal_dirty(cx);
        let (Some(tab), Some(editor)) = (document.goal.as_mut(), document.editor.as_ref()) else {
            return;
        };
        let objective = editor.read(cx).text().trim().to_owned();
        if tab.saving || !dirty || objective.is_empty() {
            return;
        }
        tab.saving = true;
        tab.error = None;
        let thread_id = tab.thread_id.clone();
        cx.emit(SaveGoalObjective {
            thread_id,
            objective,
        });
        cx.notify();
    }

    fn revert_goal(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(document) = self.documents.iter().find(|d| d.id == id) else {
            return;
        };
        if let (Some(tab), Some(editor)) = (&document.goal, &document.editor) {
            let saved = tab.saved.clone();
            editor.update(cx, |editor, cx| editor.reload(saved, cx));
        }
        cx.notify();
    }

    /// The header under the tab strip: when the goal was last updated, then
    /// Revert and Save, both only live while the text differs.
    pub(super) fn goal_toolbar(
        &self,
        document: &Document,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let tab = document.goal.as_ref()?;
        let id = document.id;
        let dirty = document.goal_dirty(cx) && !tab.saving;
        let savable = dirty
            && document
                .editor
                .as_ref()
                .is_some_and(|editor| !editor.read(cx).text().trim().is_empty());
        let status = tab.error.clone().unwrap_or_else(|| {
            if tab.saving {
                crate::i18n::format!("保存中…" => "Saving…")
            } else {
                updated_label(tab.updated_at, unix_now())
            }
        });
        Some(
            div()
                .h(px(40.))
                .flex_none()
                .px(px(8.))
                .border_b_1()
                .border_color(theme.border)
                .flex()
                .items_center()
                .gap(px(4.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .truncate()
                        .text_size(px(13.))
                        .line_height(px(18.))
                        .text_color(if tab.error.is_some() {
                            theme.warning
                        } else {
                            theme.text_tertiary
                        })
                        .debug_selector(|| "GOAL_TAB_UPDATED".to_owned())
                        .child(status),
                )
                .child(
                    self.control(
                        ("goal-revert", id),
                        &crate::i18n::format!("还原" => "Revert"),
                        "goal-revert",
                        theme,
                    )
                    .rounded(px(10.))
                    .opacity(if dirty { 1. } else { 0.4 })
                    .debug_selector(|| "GOAL_TAB_REVERT".to_owned())
                    .when(dirty, |button| {
                        button
                            .on_click(cx.listener(move |panel, _, _, cx| panel.revert_goal(id, cx)))
                    }),
                )
                .child(
                    // The reference's primary toolbar button: a solid fill in
                    // the text colour, 80% on hover, 40% when disabled.
                    div()
                        .id(("goal-save", id))
                        .role(Role::Button)
                        .aria_label(crate::i18n::format!("保存" => "Save"))
                        .tab_stop(true)
                        .h(px(28.))
                        .px(px(8.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(theme.border)
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .line_height(px(18.))
                        .bg(theme.text)
                        .text_color(theme.surface)
                        .opacity(if savable { 1. } else { 0.4 })
                        .debug_selector(|| "GOAL_TAB_SAVE".to_owned())
                        .when(savable, |button| {
                            button
                                .cursor_pointer()
                                .hover(move |button| button.bg(theme.text.alpha(0.8)))
                                .on_click(
                                    cx.listener(move |panel, _, _, cx| panel.save_goal(id, cx)),
                                )
                        })
                        .child(crate::i18n::format!("保存" => "Save")),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::{FilePanel, GoalTabSync, SaveGoalObjective, updated_label};
    use crate::{
        agent::{AgentThreadGoal, AgentThreadGoalStatus},
        theme::ThemeMode,
    };

    fn goal(objective: &str, updated_at: i64) -> AgentThreadGoal {
        AgentThreadGoal {
            thread_id: "thread".into(),
            objective: objective.into(),
            status: AgentThreadGoalStatus::Paused,
            token_budget: None,
            tokens_used: 0,
            time_used_seconds: 26,
            created_at: 1,
            updated_at,
        }
    }

    #[test]
    fn updated_label_counts_whole_minutes() {
        assert_eq!(updated_label(100, 159), updated_label(100, 100));
        assert_ne!(updated_label(100, 160), updated_label(100, 100));
        assert!(updated_label(100, 100 + 180).contains('3'));
        // A clock behind the server never shows a negative age.
        assert_eq!(updated_label(100, 40), updated_label(100, 100));
    }

    #[test]
    fn the_goal_tab_saves_only_edits_reverts_and_follows_the_goal() {
        let root = std::env::temp_dir().join(format!("gpui-goal-tab-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut app = gpui::TestApp::new();
        app.update(super::super::super::file_editor::init);
        let mut window = app.open_window_with_options(gpui::WindowOptions::default(), |_, cx| {
            FilePanel::new(root.clone(), ThemeMode::Dark, cx)
        });
        let saves = Rc::new(RefCell::new(Vec::<(String, String)>::new()));
        let seen = saves.clone();
        window.update(move |p, _, cx| {
            cx.subscribe(&cx.entity(), move |_, _, save: &SaveGoalObjective, _| {
                seen.borrow_mut()
                    .push((save.thread_id.clone(), save.objective.clone()))
            })
            .detach();
            p.open_goal(&goal("Run sleep 45", 10), "Run sleep 45", false, cx);
            p.open_goal(&goal("Run sleep 45", 10), "Run sleep 45", false, cx);
        });
        app.run_until_parked();
        window.draw();
        window.update(|p, _, cx| {
            assert_eq!(p.documents.len(), 1, "one tab per thread goal");
            assert!(!p.has_unsaved(cx), "a goal edit never blocks closing");
            assert!(p.open_documents().is_empty(), "not a file for context");
            let id = p.active.unwrap();
            // Unchanged text does not save.
            p.save_goal(id, cx);
            let editor = p.current().unwrap().editor.clone().unwrap();
            editor.update(cx, |e, cx| e.reload("Run sleep 5".into(), cx));
            p.save_goal(id, cx);
            p.save_goal(id, cx);
        });
        assert_eq!(
            saves.borrow().as_slice(),
            &[("thread".to_owned(), "Run sleep 5".to_owned())],
            "one save while it is in flight"
        );
        window.update(|p, _, cx| {
            // A failed save keeps the edit and shows the error.
            p.sync_goal(
                "thread",
                GoalTabSync {
                    goal: Some(&goal("Run sleep 45", 10)),
                    text: Some("Run sleep 45".into()),
                    saving: false,
                    error: Some("Failed to save goal objective".into()),
                },
                cx,
            );
            let document = p.current().unwrap();
            assert!(document.goal_dirty(cx));
            // The retried save lands: the tab stays and is clean.
            p.sync_goal(
                "thread",
                GoalTabSync {
                    goal: Some(&goal("Run sleep 5", 20)),
                    text: Some("Run sleep 5".into()),
                    saving: false,
                    error: None,
                },
                cx,
            );
            let document = p.current().unwrap();
            assert_eq!(
                document.editor.as_ref().unwrap().read(cx).text(),
                "Run sleep 5"
            );
            assert!(!document.goal_dirty(cx));
            let id = document.id;
            let editor = document.editor.clone().unwrap();
            editor.update(cx, |e, cx| e.reload("draft".into(), cx));
            p.revert_goal(id, cx);
            assert!(
                !p.current().unwrap().goal_dirty(cx),
                "revert restores the saved text"
            );
            // An objective changed elsewhere closes the tab.
            p.sync_goal(
                "thread",
                GoalTabSync {
                    goal: Some(&goal("Changed elsewhere", 30)),
                    text: Some("Changed elsewhere".into()),
                    saving: false,
                    error: None,
                },
                cx,
            );
            assert!(p.documents.is_empty());
            // A complete goal closes it too, and a second edit click toggles.
            p.open_goal(&goal("Run sleep 5", 20), "Run sleep 5", true, cx);
            let mut complete = goal("Run sleep 5", 40);
            complete.status = AgentThreadGoalStatus::Complete;
            p.sync_goal(
                "thread",
                GoalTabSync {
                    goal: Some(&complete),
                    text: Some("Run sleep 5".into()),
                    saving: false,
                    error: None,
                },
                cx,
            );
            assert!(p.documents.is_empty());
            p.open_goal(&goal("Run sleep 5", 20), "Run sleep 5", true, cx);
            assert_eq!(p.documents.len(), 1);
            p.open_goal(&goal("Run sleep 5", 20), "Run sleep 5", true, cx);
            assert!(
                p.documents.is_empty(),
                "the visible goal tab toggles closed"
            );
        });
        app.run_until_parked();
        std::fs::remove_dir_all(root).unwrap();
    }
}
