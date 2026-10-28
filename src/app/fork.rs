//! Durable reply forks keep their creation state on the originating composer.
use gpui::{Context, Entity};

use super::ChatApp;
use crate::components::composer::{ComposerView, ForkConversationRequest, OpenForkSource};

impl ChatApp {
    pub(super) fn wire_response_fork(composer: &Entity<ComposerView>, cx: &mut Context<Self>) {
        cx.subscribe(composer, |this, _, event: &OpenForkSource, cx| {
            this.select_conversation(event.0.clone(), cx);
        })
        .detach();
        cx.subscribe(
            composer,
            |this, composer, request: &ForkConversationRequest, cx| {
                let request = request.clone();
                let backend = this.agent_backend.clone();
                let snapshot = this.workspace_store.snapshot();
                let source_title = snapshot
                    .thread(&request.thread_id)
                    .map(|thread| thread.title.clone());
                let fork_count = snapshot
                    .recent_threads
                    .iter()
                    .filter(|thread| thread.forked_from_id.as_deref() == Some(&request.thread_id))
                    .count();
                cx.spawn(async move |this, cx| {
                    let mut worktree_cwd = request.worktree_cwd;
                    let result = async {
                        let cwd = if request.worktree {
                            if let Some(cwd) = &worktree_cwd {
                                cwd.clone()
                            } else {
                                let root = std::env::var_os("CODEX_HOME")
                                    .map(std::path::PathBuf::from)
                                    .unwrap_or_else(|| {
                                        std::env::var_os("HOME")
                                            .map(std::path::PathBuf::from)
                                            .unwrap_or_default()
                                            .join(".codex")
                                    })
                                    .join("worktrees");
                                let cwd = request.cwd.clone();
                                let worktree = cx
                                    .background_executor()
                                    .spawn(async move {
                                        crate::git_review::workspace::create_worktree(
                                            &cwd, "HEAD", true, &root,
                                        )
                                    })
                                    .await
                                    .map_err(|error| error.to_string())?;
                                worktree_cwd = Some(worktree.cwd.clone());
                                worktree.cwd
                            }
                        } else {
                            request.cwd
                        };
                        let id = backend
                            .fork_conversation_at(request.thread_id, cwd, request.last_turn_id)
                            .recv()
                            .await
                            .map_err(|error| error.to_string())?
                            .map_err(|error| {
                                error.user_message(crate::i18n::text("创建聊天分支"))
                            })?;
                        // Naming also makes a freshly persisted fork discoverable
                        // before it has received its first new user message.
                        let title = format!(
                            "{} ({})",
                            source_title.unwrap_or_else(|| crate::i18n::text("新聊天").into()),
                            fork_count + 2
                        );
                        let finalized = async {
                            backend
                                .set_thread_name(id.clone(), title)
                                .recv()
                                .await
                                .map_err(|error| error.to_string())?
                                .map_err(|error| {
                                    error.user_message(crate::i18n::text("创建聊天分支"))
                                })?;
                            backend
                                .read_thread(id.clone())
                                .recv()
                                .await
                                .map_err(|error| error.to_string())?
                                .map_err(|error| {
                                    error.user_message(crate::i18n::text("读取聊天历史"))
                                })
                        }
                        .await;
                        if finalized.is_err() {
                            let _ = backend.archive_thread(id).recv().await;
                        }
                        finalized
                    }
                    .await;
                    let _ = this.update(cx, |this, cx| {
                        match result {
                            Ok(thread) => {
                                let id = thread.thread_id.clone();
                                composer.update(cx, |composer, cx| {
                                    composer.settle_response_fork(Ok(()), worktree_cwd, cx)
                                });
                                this.workspace_store.register_forked_thread(thread);
                                this.workspace_store.refresh_all();
                                if this
                                    .conversation_hosts
                                    .get(&this.active_conversation)
                                    .is_some_and(|host| host.composer == composer)
                                {
                                    this.select_conversation(id, cx);
                                    this.draft_focus_pending = true;
                                }
                            }
                            Err(error) => composer.update(cx, |composer, cx| {
                                composer.settle_response_fork(Err(error), worktree_cwd, cx)
                            }),
                        }
                        cx.notify();
                    });
                })
                .detach();
            },
        )
        .detach();
    }
}
