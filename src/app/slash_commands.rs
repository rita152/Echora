//! Slash commands reuse the same shell workflows as sidebar and panel actions.
use super::{ChatApp, panel_tabs::TabPlacement, state::RightPanelMode};
use crate::components::composer::{ComposerView, SlashChatAction, SlashCommand, ToastKind};
use gpui::{Context, Entity};

impl ChatApp {
    pub(super) fn wire_slash_commands(
        composer: &Entity<ComposerView>,
        recent: Vec<crate::workspace::RecentModelConfiguration>,
        cx: &mut Context<Self>,
    ) {
        Self::wire_response_fork(composer, cx);
        composer.update(cx, |composer, cx| {
            composer.set_recent_command_models(recent, cx)
        });
        cx.subscribe(
            composer,
            |this, composer, _: &crate::components::composer::ConversationChanged, cx| {
                let Some(configuration) = composer.read(cx).selected_command_configuration() else {
                    return;
                };
                if this
                    .workspace_store
                    .record_model_configuration(configuration)
                {
                    let recent = this
                        .workspace_store
                        .snapshot()
                        .preferences
                        .recent_model_configurations;
                    for host in this.conversation_hosts.values() {
                        host.composer.update(cx, |composer, cx| {
                            composer.set_recent_command_models(recent.clone(), cx)
                        });
                    }
                }
            },
        )
        .detach();
        cx.subscribe(
            composer,
            |this, composer, _: &crate::components::composer::ToggleDesktopPet, cx| {
                if let Some(handle) = this.desktop_pet.take()
                    && handle
                        .update(cx, |_, window, _| window.remove_window())
                        .is_ok()
                {
                    return;
                }
                match crate::components::composer::DesktopPet::open(composer.downgrade(), cx) {
                    Ok(handle) => this.desktop_pet = Some(handle),
                    Err(error) => composer.update(cx, |composer, cx| {
                        composer.show_panel_toast(ToastKind::Danger, error.to_string(), None, cx)
                    }),
                }
            },
        )
        .detach();
        cx.subscribe(
            composer,
            |this, composer, event: &crate::components::composer::InspectInputReference, cx| {
                use crate::agent::AgentInputReferenceKind;
                match event.0.kind {
                    AgentInputReferenceKind::Plugin => {
                        if let Some(id) = event.0.path.strip_prefix("plugin://") {
                            this.open_settings(cx);
                            this.settings
                                .update(cx, |s, cx| s.inspect_plugin_reference(id.to_owned(), cx));
                        }
                    }
                    AgentInputReferenceKind::Thread => {
                        if let Some(id) = event
                            .0
                            .path
                            .strip_prefix("thread://")
                            .and_then(|s| s.split('?').next())
                        {
                            this.select_conversation(id.to_owned(), cx);
                        }
                    }
                    AgentInputReferenceKind::Skill | AgentInputReferenceKind::Mention => {
                        let path = std::path::PathBuf::from(&event.0.path);
                        if path.is_absolute() {
                            composer.update(cx, |_, cx| {
                                cx.emit(crate::components::composer::OpenAttachmentPreview(path))
                            });
                        }
                    }
                }
            },
        )
        .detach();
        cx.subscribe(
            composer,
            |this, composer, _: &crate::components::composer::ContextSourcesRequested, cx| {
                let tabs = this
                    .browser_panels
                    .get(&this.active_conversation)
                    .map(|panel| panel.read(cx).context_tabs())
                    .unwrap_or_default();
                composer.update(cx, |composer, cx| {
                    composer.set_context_browser_tabs(tabs, cx)
                });
            },
        )
        .detach();
        cx.subscribe(
            composer,
            |this, composer, event: &crate::components::composer::AttachBrowserContext, cx| {
                let Some(panel) = this.browser_panels.get(&this.active_conversation) else {
                    return;
                };
                let cwd = composer.read(cx).conversation_cwd();
                let thread = composer.read(cx).thread_id().map(str::to_owned);
                let draft = composer.read(cx).draft_identity();
                let weak = composer.downgrade();
                let mut async_cx = cx.to_async();
                panel.read(cx).collect_context(event.0, move |result| {
                    let result = result.and_then(|text| {
                        let root = std::env::var_os("CODEX_HOME")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| {
                                std::path::PathBuf::from(
                                    std::env::var_os("HOME").unwrap_or_default(),
                                )
                                .join(".codex")
                            })
                            .join("attachments");
                        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                        let path = root.join(format!(
                            "browser-context-{}.json",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos()
                        ));
                        std::fs::write(&path, text).map_err(|e| e.to_string())?;
                        Ok(path)
                    });
                    let _ = weak.update(&mut async_cx, |composer, cx| {
                        if composer.conversation_cwd() == cwd
                            && composer.draft_identity() == draft
                            && composer.thread_id() == thread.as_deref()
                        {
                            composer.attach_browser_context_result(result, cx);
                        }
                    });
                });
            },
        )
        .detach();
        cx.subscribe(
            composer,
            |this, composer, _: &crate::components::composer::FeedbackContextRequested, cx| {
                let diagnostics = this
                    .browser_panels
                    .values()
                    .map(|panel| panel.read(cx).feedback_diagnostics())
                    .collect();
                composer.update(cx, |composer, cx| {
                    composer.set_feedback_browser_diagnostics(diagnostics, cx)
                });
            },
        )
        .detach();
        cx.subscribe(composer, |this, composer, event: &SlashChatAction, cx| {
            if !this
                .conversation_hosts
                .get(&this.active_conversation)
                .is_some_and(|host| host.composer == composer)
            {
                return;
            }
            let cwd = composer.read(cx).conversation_cwd();
            let project = composer.read(cx).conversation_project_id();
            let id = composer.read(cx).thread_id().map(str::to_owned);
            match event.0 {
                SlashCommand::New => this.start_draft(project, cwd, cx),
                SlashCommand::Side => {
                    this.open_panel_tool(RightPanelMode::SideChat, TabPlacement::Append, cx)
                }
                SlashCommand::Archive => {
                    if let Some(id) = id {
                        this.workspace_store.archive_thread(id);
                        this.start_draft(project, cwd, cx);
                    }
                }
                SlashCommand::Pin => {
                    if let Some(id) = id {
                        let pinned = this
                            .workspace_store
                            .snapshot()
                            .pinned_threads
                            .iter()
                            .any(|t| t.thread_id == id);
                        this.workspace_store.set_thread_pinned(id, !pinned);
                    }
                }
                SlashCommand::Rename => {
                    if let Some(id) = id {
                        let snapshot = this.workspace_store.snapshot();
                        let title = snapshot
                            .recent_threads
                            .iter()
                            .chain(&snapshot.pinned_threads)
                            .chain(snapshot.custom_sections.iter().flat_map(|s| &s.threads))
                            .find(|t| t.thread_id == id)
                            .map(|t| t.title.clone())
                            .unwrap_or_default();
                        this.sidebar
                            .update(cx, |sidebar, cx| sidebar.open_thread_rename(id, title, cx));
                    }
                }
                SlashCommand::Fork | SlashCommand::ForkWorktree => {
                    if let Some(id) = id {
                        let origin = this.active_conversation.clone();
                        let backend = this.agent_backend.clone();
                        let worktree = event.0 == SlashCommand::ForkWorktree;
                        let root = std::env::var_os("CODEX_HOME")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| {
                                std::env::var_os("HOME")
                                    .map(std::path::PathBuf::from)
                                    .unwrap_or_default()
                                    .join(".codex")
                            })
                            .join("worktrees");
                        cx.spawn(async move |this, cx| {
                            let result = if worktree {
                                match cx
                                    .background_executor()
                                    .spawn(async move {
                                        crate::git_review::workspace::create_worktree(
                                            &cwd, "HEAD", true, &root,
                                        )
                                    })
                                    .await
                                {
                                    Ok(worktree) => {
                                        backend.fork_conversation(id, worktree.cwd).recv().await
                                    }
                                    Err(error) => Ok(Err(crate::agent::WorkspaceError::backend(
                                        error.to_string(),
                                    ))),
                                }
                            } else {
                                backend.fork_conversation(id, cwd).recv().await
                            };
                            let _ = this.update(cx, |this, cx| {
                                composer
                                    .update(cx, |composer, cx| composer.settle_command_fork(cx));
                                match result {
                                    Ok(Ok(id)) => {
                                        this.workspace_store.refresh_all();
                                        if this.active_conversation == origin {
                                            this.select_conversation(id, cx);
                                        }
                                    }
                                    error => {
                                        let message = match error {
                                            Ok(Err(e)) => {
                                                e.user_message(crate::i18n::text("创建聊天分支"))
                                            }
                                            Err(e) => e.to_string(),
                                            _ => unreachable!(),
                                        };
                                        composer.update(cx, |composer, cx| {
                                            composer.show_panel_toast(
                                                ToastKind::Danger,
                                                message,
                                                None,
                                                cx,
                                            )
                                        });
                                    }
                                }
                            });
                        })
                        .detach();
                    }
                }
                _ => {}
            }
            cx.notify();
        })
        .detach();
    }
}
