//! Editing existing projects and confirming project-wide actions.
use super::*;
use crate::app::state::ProjectConfirmation;
use crate::{agent::UpdateProject, components::sidebar::ProjectAction};

impl ChatApp {
    pub(in crate::app) fn open_project_action(
        &mut self,
        action: ProjectAction,
        cx: &mut Context<Self>,
    ) {
        if self.project_creation.creating {
            return;
        }
        let id = match &action {
            ProjectAction::Edit(id) | ProjectAction::Archive(id) | ProjectAction::Remove(id) => id,
        };
        let snapshot = self.workspace_store.snapshot();
        let Some(project) = snapshot
            .projects
            .iter()
            .find(|p| &p.project_id == id)
            .cloned()
        else {
            return;
        };
        self.close_project_creation(cx);
        self.open_project_creation(cx);
        self.project_creation.previous_focus = self
            .sidebar
            .read(cx)
            .project_return_focus(&project.project_id);
        match action {
            ProjectAction::Edit(id) => {
                self.project_creation.editing = Some(id.clone());
                self.project_creation.roots = project.roots;
                self.project_creation.marker =
                    snapshot.preferences.project_markers.get(&id).cloned();
                self.project_creation
                    .name
                    .update(cx, |input, cx| input.set_text_silently(project.name, cx));
            }
            ProjectAction::Remove(id) => {
                self.project_creation.confirmation = Some(ProjectConfirmation::Remove {
                    project_id: id,
                    name: project.name,
                })
            }
            ProjectAction::Archive(id) => {
                self.project_creation.confirmation = Some(ProjectConfirmation::Archive {
                    project_id: id,
                    name: project.name,
                })
            }
        }
        cx.notify();
    }

    pub(super) fn remove_editing_project(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.project_creation.editing.clone() {
            self.open_project_action(ProjectAction::Remove(id), cx);
        }
    }

    pub(super) fn save_project_edit(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.project_creation.editing.clone() else {
            return;
        };
        let name = self.project_creation.name.read(cx).text().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let roots = self.project_creation.roots.clone();
        let marker = self.project_creation.marker.clone();
        let cycle = self.project_creation.cycle;
        let store = self.workspace_store.clone();
        self.project_creation.creating = true;
        self.project_creation.focus_pending = true;
        self.project_creation.marker_menu = false;
        self.project_creation.error = None;
        cx.spawn(async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { prepare_project_update(name, roots) })
                .await;
            let result = match prepared {
                Ok(update) => store
                    .update_project(id.clone(), update)
                    .recv()
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.user_message(crate::i18n::text("更新项目")))),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                if this.project_creation.cycle != cycle {
                    return;
                }
                this.project_creation.creating = false;
                match result {
                    Ok(project) => {
                        this.refresh_project_drafts(&id, Some(&project), cx);
                        store.set_project_marker(id, marker);
                        this.close_project_creation(cx);
                    }
                    Err(error) => {
                        this.project_creation.error = Some(error);
                        this.project_creation.focus_pending = true;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn confirm_project_action(&mut self, cx: &mut Context<Self>) {
        let Some(confirmation) = self.project_creation.confirmation.clone() else {
            return;
        };
        let removed_id = match &confirmation {
            ProjectConfirmation::Remove { project_id, .. } => Some(project_id.clone()),
            _ => None,
        };
        let store = self.workspace_store.clone();
        let cycle = self.project_creation.cycle;
        self.project_creation.creating = true;
        self.project_creation.focus_pending = true;
        self.project_creation.error = None;
        cx.spawn(async move |this,cx| {
            let result = match confirmation {
                ProjectConfirmation::Remove {project_id,..} => store.delete_project(project_id).recv().await
                    .map_err(|e|e.to_string()).and_then(|r|r.map_err(|e|e.user_message(crate::i18n::text("移除项目")))),
                ProjectConfirmation::Archive {project_id,..} => {
                    let ids = store.snapshot().project_thread_ids(&project_id);
                    match store.archive_threads(ids).recv().await {
                        Ok((_,0)) => Ok(()),
                        Ok((_,failed)) => Err(crate::i18n::format!("{failed} 个聊天归档失败，请重试。" => "{failed} chats could not be archived. Try again.")),
                        Err(e) => Err(e.to_string()),
                    }
                },
            };
            let _ = this.update(cx, |this,cx| {
                if this.project_creation.cycle != cycle { return; }
                this.project_creation.creating = false;
                match result {
                    Ok(()) => {
                        if let Some(id) = removed_id {
                            this.refresh_project_drafts(&id, None, cx);
                            this.project_creation.previous_focus = None;
                            this.terminal_return_focus_pending = true;
                        }
                        this.close_project_creation(cx);
                    },
                    Err(error) => {this.project_creation.error=Some(error);this.project_creation.focus_pending=true;cx.notify();},
                }
            });
        }).detach();
        cx.notify();
    }

    fn refresh_project_drafts(
        &mut self,
        id: &str,
        project: Option<&crate::agent::Project>,
        cx: &mut Context<Self>,
    ) {
        let project_id = project.map(|p| p.project_id.clone());
        let cwd = project
            .and_then(|p| p.roots.first())
            .cloned()
            .unwrap_or_else(crate::components::composer::projectless_cwd);
        for (key, host) in &mut self.conversation_hosts {
            if !matches!(key, super::super::ConversationKey::Draft(_))
                || host.project_id.as_deref() != Some(id)
            {
                continue;
            }
            host.project_id = project_id.clone();
            host.cwd = cwd.clone();
            host.composer.update(cx, |composer, cx| {
                composer.set_workspace_context(cwd.clone(), project_id.clone(), None, cx)
            });
            if let Some(panel) = self.file_panels.get(key) {
                panel.update(cx, |panel, cx| panel.set_workspace_root(cwd.clone(), cx));
            }
            if let Some(panel) = self.terminal_panels.get(key) {
                panel.update(cx, |panel, cx| panel.set_workspace_root(cwd.clone(), cx));
            }
            if let Some(panel) = self.review_panels.get(key) {
                panel.update(cx, |panel, cx| panel.set_workspace_root(cwd.clone(), cx));
            }
            if key == &self.active_conversation {
                self.sidebar.update(cx, |sidebar, cx| {
                    sidebar.set_draft_project(project_id.clone(), cx)
                });
            }
        }
    }

    pub(super) fn project_confirmation_overlay(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let confirmation = self.project_creation.confirmation.as_ref().unwrap();
        let (title, description, action) = match confirmation {
            ProjectConfirmation::Remove { name, .. } => (
                crate::i18n::format!("移除 {name}？" => "Remove {name}?"),
                crate::i18n::format!("这只会从应用中移除项目。电脑上的文件和现有聊天不会被删除。" => "This only removes the project from the app. Files on your computer and existing chats won't be deleted."),
                crate::i18n::text("移除项目").to_owned(),
            ),
            ProjectConfirmation::Archive { project_id, name } => {
                let count = self
                    .workspace_store
                    .snapshot()
                    .project_thread_ids(project_id)
                    .len();
                (
                    crate::i18n::format!("归档 {count} 个聊天？" => "Archive {count} chats?"),
                    crate::i18n::format!("这会归档 {name} 中的聊天。你可以稍后在已归档聊天中找到它们" => "This will archive the chats in {name}. You can find them later in your archived chats"),
                    crate::i18n::format!("全部归档" => "Archive all"),
                )
            }
        };
        let busy = self.project_creation.creating;
        let dialog = div()
            .id("project-confirm-dialog")
            .role(gpui::Role::Dialog)
            .aria_label(title.clone())
            .relative()
            .w(px(420.))
            .max_w(gpui::relative(0.92))
            .p(px(20.))
            .rounded(px(25.))
            .bg(theme.project_dialog_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(0.), theme.border.into()).spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.), px(4.), gpui::hsla(0., 0., 0., 0.10))
                    .blur_radius(px(8.))
                    .spread_radius(px(-2.)),
            ])
            .font(crate::theme::ui_font())
            .text_color(theme.text)
            .text_size(px(crate::typography::ui_size(14.)))
            .line_height(px(crate::typography::ui_leading(14., 21.)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(crate::typography::ui_size(20.)))
                    .line_height(px(28.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .pr(px(20.))
                    .child(title),
            )
            .child(
                div()
                    .mt(px(4.))
                    .text_color(theme.text_tertiary)
                    .child(description),
            )
            .when_some(self.project_creation.error.clone(), |d, error| {
                d.child(div().mt(px(12.)).text_color(theme.warning).child(error))
            })
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .justify_end()
                    .gap(px(12.))
                    .child(
                        self.project_button(
                            "project-confirm-cancel",
                            crate::i18n::text("取消").to_owned(),
                            theme,
                        )
                        .track_focus(&self.project_creation.confirmation_cancel_focus)
                        .text_color(theme.text_tertiary)
                        .on_click(cx.listener(|this, _, _, cx| this.close_project_creation(cx)))
                        .on_key_down(cx.listener(
                            |this, e: &KeyDownEvent, _, cx| {
                                if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                    this.close_project_creation(cx);
                                    cx.stop_propagation();
                                }
                            },
                        )),
                    )
                    .child(
                        self.project_button("project-confirm-submit", action, theme)
                            .bg(rgba(0xe02e2a1a))
                            .text_color(rgba(0xe02e2aff))
                            .when(busy, |b| b.opacity(0.4))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if !this.project_creation.creating {
                                    this.confirm_project_action(cx);
                                }
                            }))
                            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                                if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                    if !this.project_creation.creating {
                                        this.confirm_project_action(cx);
                                    }
                                    cx.stop_propagation();
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .id("project-confirm-close")
                    .role(gpui::Role::Button)
                    .aria_label(crate::i18n::text("关闭对话框"))
                    .track_focus(&self.project_creation.close_focus)
                    .tab_stop(true)
                    .absolute()
                    .top(px(16.))
                    .right(px(16.))
                    .size(px(24.))
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| this.close_project_creation(cx)))
                    .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            this.close_project_creation(cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(icon("close-dialog", theme.text.alpha(0.8).into()).size(px(16.))),
            );
        div()
            .id("project-confirm-overlay")
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x00000022))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.project_creation.focus)
            .key_context("ProjectCreation")
            .capture_key_down(cx.listener(Self::handle_project_creation_key))
            .on_action(
                cx.listener(|this, _: &super::super::DismissProjectCreation, _, cx| {
                    this.close_project_creation(cx);
                    cx.stop_propagation();
                }),
            )
            .on_click(cx.listener(|this, _, _, cx| this.close_project_creation(cx)))
            .child(dialog)
    }
}

fn prepare_project_update(name: String, roots: Vec<PathBuf>) -> Result<UpdateProject, String> {
    let mut prepared = Vec::new();
    for root in roots {
        let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
        if !root.is_dir() {
            return Err(crate::i18n::format!("源文件夹不存在" => "Source folder does not exist"));
        }
        if !prepared.contains(&root) {
            prepared.push(root);
        }
    }
    Ok(UpdateProject {
        name: Some(name),
        roots: Some(prepared),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_and_removing_a_project_refresh_unsent_draft_context() {
        let mut app = gpui::TestApp::new();
        let mut window =
            app.open_window(|_, cx| ChatApp::new(crate::theme::ThemeMode::Dark, false, cx));
        window.update(|chat, _, cx| {
            chat.start_draft(Some("fixture".into()), PathBuf::from("/tmp/old"), cx);
            let project = crate::agent::Project {
                project_id: "fixture".into(),
                name: "Edited".into(),
                roots: vec![PathBuf::from("/tmp/new")],
                created_at: 0,
                updated_at: 0,
                recency_at: None,
                position: 0,
            };
            chat.refresh_project_drafts("fixture", Some(&project), cx);
            let host = &chat.conversation_hosts[&chat.active_conversation];
            assert_eq!(host.cwd, PathBuf::from("/tmp/new"));
            assert_eq!(host.project_id.as_deref(), Some("fixture"));
            chat.refresh_project_drafts("fixture", None, cx);
            let host = &chat.conversation_hosts[&chat.active_conversation];
            assert!(host.project_id.is_none());
            assert_eq!(host.cwd, crate::components::composer::projectless_cwd());
        });
        window.draw();
    }

    #[test]
    fn editing_can_clear_roots_and_never_creates_a_workspace() {
        let update = prepare_project_update("Existing".into(), vec![]).unwrap();
        assert_eq!(update.name.as_deref(), Some("Existing"));
        assert_eq!(update.roots, Some(vec![]));
        let root = std::env::temp_dir().join(format!("echora-edit-sources-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let canonical = std::fs::canonicalize(&root).unwrap();
        let update =
            prepare_project_update("Existing".into(), vec![root.clone(), root.join(".")]).unwrap();
        assert_eq!(update.roots, Some(vec![canonical]));
        let file = root.join("file");
        std::fs::write(&file, "keep").unwrap();
        assert!(prepare_project_update("Existing".into(), vec![file]).is_err());
        assert!(prepare_project_update("Existing".into(), vec![root.join("missing")]).is_err());
        assert!(!root.join("missing").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
