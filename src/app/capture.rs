//! Capture behavior and presentation for the application shell.

use std::{path::PathBuf, time::Duration};

#[cfg(feature = "screenshot")]
use super::{ConversationKey, state::RightPanelMode};
#[cfg(feature = "screenshot")]
use crate::components::file_panel::FilePanel;
#[cfg(feature = "screenshot")]
use gpui::AppContext;
use gpui::Context;

use super::{
    ChatApp,
    state::{ProjectCreationKind, ProjectCreationStep},
};
use crate::{
    agent::{ProjectId, ThreadId},
    components::file_change::captured_diff_review_fixture,
};

/// Capture-only: the browser state to show, applied again to whichever chat's
/// browser becomes current (a resumed thread arrives after startup).
#[cfg(feature = "screenshot")]
pub(super) struct BrowserCapture {
    state: String,
    url: Option<String>,
    snapshot: Option<PathBuf>,
    applied_to: Option<ConversationKey>,
}

impl ChatApp {
    #[cfg(feature = "screenshot")]
    pub(crate) fn live_capture_status(
        &self,
        cx: &gpui::App,
    ) -> (crate::conversation::ConversationPhase, Option<String>) {
        let composer = self.conversation_hosts[&self.active_conversation]
            .composer
            .read(cx);
        (
            composer.conversation_phase(),
            composer.thread_id().map(str::to_owned),
        )
    }

    #[cfg(feature = "screenshot")]
    pub fn replay_approvals(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        match crate::agent::CodexAppServerBackend::replay_approvals(path) {
            Ok(capture) => {
                self.complete_startup_for_capture(cx);
                self.home.update(cx, |home, cx| {
                    home.replay_approvals(
                        capture.run,
                        &capture.user_message,
                        &capture.assistant_message,
                        capture.cwd,
                        cx,
                    )
                });
            }
            Err(error) => {
                eprintln!("审批回放失败：{error:#}");
                cx.quit();
            }
        }
    }
    /// Opens the Browser in one of its capture states (see
    /// `BrowserPanel::capture_state`).
    #[cfg(feature = "screenshot")]
    pub fn capture_browser(
        &mut self,
        state: &str,
        url: Option<String>,
        snapshot: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.complete_startup_for_capture(cx);
        self.browser_capture = Some(BrowserCapture {
            state: state.to_owned(),
            url,
            snapshot,
            applied_to: None,
        });
        self.right_panel.open = true;
        self.select_right_panel_item(1, cx);
        cx.notify();
    }

    /// Shows the capture's browser state in the current chat's browser once.
    #[cfg(feature = "screenshot")]
    pub(super) fn apply_browser_capture(&mut self, cx: &mut Context<Self>) {
        let key = self.active_conversation.clone();
        let Some(capture) = self.browser_capture.as_mut() else {
            return;
        };
        if capture.applied_to.as_ref() == Some(&key) {
            return;
        }
        capture.applied_to = Some(key.clone());
        let (state, url, snapshot) = (
            capture.state.clone(),
            capture.url.clone(),
            capture.snapshot.clone(),
        );
        if let Some(panel) = self.browser_panels.get(&key) {
            panel.update(cx, |panel, cx| {
                panel.capture_state(&state, url, snapshot, cx)
            });
        }
    }
    #[cfg(feature = "screenshot")]
    pub fn capture_review(&mut self, cwd: PathBuf, cx: &mut Context<Self>) {
        if let Some(host) = self.conversation_hosts.get_mut(&self.active_conversation) {
            host.cwd = cwd.clone();
            host.composer.update(cx, |composer, cx| {
                composer.set_workspace_context(
                    cwd,
                    None,
                    composer.thread_id().map(str::to_owned),
                    cx,
                )
            });
        }
        self.review_panels.remove(&self.active_conversation);
        self.complete_startup_for_capture(cx);
        self.right_panel.open = true;
        self.select_right_panel_item(4, cx);
    }
    #[cfg(feature = "screenshot")]
    pub fn review_capture_ready(&self, cx: &gpui::App) -> Result<bool, String> {
        if let ConversationKey::Thread(thread) = &self.active_conversation
            && !self.resumed_thread_ready(thread, cx)?
        {
            return Ok(false);
        }
        self.review_panels
            .get(&self.active_conversation)
            .map_or(Ok(false), |p| p.read(cx).capture_ready())
    }
    #[cfg(feature = "screenshot")]
    pub fn capture_review_filter(&mut self, query: &str, cx: &mut Context<Self>) {
        if let Some(panel) = self.review_panels.get(&self.active_conversation) {
            panel.update(cx, |panel, cx| panel.capture_filter(query, cx));
        }
    }
    /// Opens one review popup (`scope`, `options`, `branch`) for capture.
    #[cfg(feature = "screenshot")]
    pub fn capture_review_menu(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(panel) = self.review_panels.get(&self.active_conversation) {
            panel.update(cx, |panel, cx| panel.capture_menu(name, cx));
        }
    }
    /// Opens a persisted thread without requiring the sidebar to finish loading first.
    ///
    /// This is used by the deterministic Markdown capture path. It deliberately
    /// follows the same history-loading path as a real sidebar selection.
    pub fn resume_thread_for_capture(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.select_thread_for_capture(thread_id.clone(), cx)
        });
        self.select_conversation(thread_id, cx);
    }
    /// Returns whether the requested thread has finished hydrating, or its load error.
    #[cfg(feature = "screenshot")]
    pub fn resumed_thread_ready(&self, thread_id: &str, cx: &gpui::App) -> Result<bool, String> {
        let sidebar_ready = self
            .sidebar
            .read(cx)
            .resumed_thread_ready_for_capture(thread_id)?;
        if !(self.startup_model_catalog_resolved
            && self.startup_sidebar_resolved
            && self.startup_minimum_duration_elapsed
            && sidebar_ready)
        {
            return Ok(false);
        }
        let key = ConversationKey::Thread(thread_id.to_owned());
        let Some(host) = self.conversation_hosts.get(&key) else {
            return Ok(false);
        };
        let composer = host.composer.read(cx);
        if let Some(error) = composer.history_error() {
            return Err(error.to_owned());
        }
        Ok(!composer.history_loading()
            && composer.thread_id() == Some(thread_id)
            && composer.model_catalog_ready_for_capture()?
            // The rail fades in and its markers ease; a still frame waits for
            // them, as the reference capture waits for its rail to settle.
            && !self.home.read(cx).user_message_rail_animating_for_capture(cx))
    }
    #[cfg(feature = "screenshot")]
    pub fn resumed_render_audit(&self, thread_id: &str, cx: &gpui::App) -> serde_json::Value {
        let composer = self.conversation_hosts[&ConversationKey::Thread(thread_id.to_owned())]
            .composer
            .read(cx);
        let mut turns = composer
            .transcript_render_snapshot()
            .into_iter()
            .map(|turn| {
                serde_json::json!({
                    "id":turn.resumed.map(|r|r.id), "user_message":turn.user_message,
                    "units":crate::components::home::resumed_activity_audit(&turn.activities)
                })
            })
            .collect::<Vec<_>>();
        let (_, user, _, _, _, activities) = composer.conversation_render_snapshot();
        turns.push(
            serde_json::json!({"id":composer.resumed_turn().map(|r|r.id), "user_message":user,
            "units":crate::components::home::resumed_activity_audit(&activities)}),
        );
        let (pane_left, pane_top) = self.home.read(cx).conversation_pane_origin();
        serde_json::json!({"thread_id":thread_id,"turns":turns,
            "conversationPaneOrigin":[pane_left,pane_top]})
    }
    #[cfg(feature = "screenshot")]
    pub fn set_conversation_scroll_from_bottom_for_capture(
        &mut self,
        distance: f32,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_conversation_scroll_from_bottom_for_capture(distance, cx)
        });
    }
    /// Capture-only rail state: the marker the pointer would rest on.
    #[cfg(feature = "screenshot")]
    pub fn set_user_message_navigation_hover_for_capture(
        &mut self,
        index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_user_message_navigation_hover_for_capture(index, cx)
        });
    }
    /// Capture-only replay of the rail's pointer scenarios.
    #[cfg(feature = "screenshot")]
    pub fn record_user_message_rail_motion(
        &mut self,
        output: std::path::PathBuf,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.record_user_message_rail_motion(output, window, cx)
        });
    }
    /// Capture-only rail jump, so both builds record the same transcript.
    #[cfg(feature = "screenshot")]
    pub fn reveal_user_message_for_capture(&mut self, index: usize, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.reveal_user_message_for_capture(index, cx)
        });
    }
    pub fn complete_startup_for_capture(&mut self, cx: &mut Context<Self>) {
        self.startup_model_catalog_resolved = true;
        self.startup_sidebar_resolved = true;
        self.startup_minimum_duration_elapsed = true;
        cx.notify();
    }
    /// True once the account surfaces have an answer to render, so an account
    /// capture receives the real account and quota instead of a pending state.
    #[cfg(feature = "screenshot")]
    pub fn account_capture_ready(&self) -> bool {
        !matches!(
            self.account.status,
            crate::components::account::AccountLoadStatus::Idle
                | crate::components::account::AccountLoadStatus::Loading
        )
    }

    /// Opens the chat search dialog in a deterministic state for capture.
    #[cfg(feature = "screenshot")]
    pub fn capture_chat_search(
        &mut self,
        state: &str,
        query: Option<&str>,
        index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.chat_search.update(cx, |search, cx| {
            search.open(cx);
            search.apply_capture_state(state, query, index, cx);
        });
        cx.notify();
    }

    /// Ready once the workspace data the dialog reads has settled.
    #[cfg(feature = "screenshot")]
    pub fn chat_search_capture_ready(&self, cx: &gpui::App) -> Result<bool, String> {
        self.chat_search.read(cx).capture_ready()
    }

    /// Opens an account dialog for capture. Log out only becomes reachable
    /// after the confirmation the capture shows.
    pub fn open_account_dialog_for_capture(
        &mut self,
        dialog: crate::components::account::AccountDialog,
        cx: &mut Context<Self>,
    ) {
        self.account.dialog = Some(dialog);
        self.account_focus_pending = true;
        self.sync_account_view(cx);
    }

    pub fn open_profile_menu(&mut self, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_profile_menu_open(true, cx));
    }
    pub fn open_projects_section_menu(&mut self, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_projects_section_menu_for_capture(cx)
        });
    }
    pub fn open_project_menu_for_capture(&mut self, project_id: ProjectId, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_project_menu_for_capture(project_id, cx)
        });
    }
    /// Renders the sidebar at a persisted width, as the reference app does
    /// when the user has dragged it away from the default.
    /// The right panel's width as if its divider had been dragged there.
    #[cfg(feature = "screenshot")]
    pub fn set_right_panel_width_for_capture(&mut self, width: f32, cx: &mut Context<Self>) {
        self.right_panel.width = Some(width);
        cx.notify();
    }
    pub fn set_sidebar_width_for_capture(&mut self, width: f32, cx: &mut Context<Self>) {
        let width = width.clamp(super::SIDEBAR_MIN_WIDTH, super::SIDEBAR_MAX_WIDTH);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_width(width, cx));
    }
    /// Starts with the sidebar closed and its transition settled, as after a
    /// click on the titlebar trigger.
    pub fn collapse_sidebar_for_capture(&mut self, cx: &mut Context<Self>) {
        let layout = &mut self.sidebar_layout;
        layout.collapsed = true;
        layout.reveal = 0.0;
        layout.animation_from = 0.0;
        layout.animation_to = 0.0;
        layout.animation_started_at = None;
        layout.animation_running = false;
        cx.notify();
    }
    /// Opens the sidebar project hover card for the screenshot path.
    pub fn open_project_hover_card_for_capture(&mut self, project: &str, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_project_hover_card_for_capture(project, cx)
        });
    }
    /// Starts a chat in a sidebar project for the screenshot path.
    pub fn start_new_conversation_for_capture(&mut self, project: &str, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.start_new_conversation_for_capture(project, cx)
        });
    }
    /// Opens the sidebar task hover card for the screenshot path.
    pub fn open_thread_hover_card_for_capture(&mut self, thread: &str, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_thread_hover_card_for_capture(thread, cx)
        });
    }
    /// Opens the task rename panel for the screenshot path.
    pub fn open_thread_rename_for_capture(&mut self, thread: &str, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_thread_rename_for_capture(thread, cx)
        });
    }
    /// Cancel, the close button, Escape, and the scrim all leave the task's
    /// name untouched, exactly like the reference dialog.
    pub fn dismiss_thread_rename(&mut self, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.dismiss_thread_rename(cx));
    }
    /// The panel's Save button submits the same field the Enter key does.
    pub fn submit_thread_rename(&mut self, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.submit_thread_rename(cx));
    }
    /// Opens the activity view in a capture state once the workspace loads.
    pub fn capture_activity(
        &mut self,
        request: crate::components::sidebar::ActivityCaptureRequest,
        cx: &mut Context<Self>,
    ) {
        self.complete_startup_for_capture(cx);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.capture_activity(request, cx));
    }
    #[cfg(feature = "screenshot")]
    pub fn activity_capture_ready(&self, cx: &gpui::App) -> bool {
        self.sidebar.read(cx).activity_capture_ready() && self.account_capture_ready()
    }
    pub fn open_model_picker(&mut self, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| home.open_model_picker(cx));
    }
    pub fn open_model_picker_submenu(&mut self, name: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.open_model_picker_submenu(name, cx));
    }
    pub fn open_model_picker_slider_at(
        &mut self,
        index: usize,
        fast: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.open_model_picker_slider_at(index, fast, cx)
        });
    }
    pub fn set_dictation_state_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_dictation_state_for_capture(state, cx)
        });
    }
    pub fn submit_prompt_for_capture(&mut self, prompt: &str, cx: &mut Context<Self>) {
        if self.startup_model_catalog_resolved {
            self.home
                .update(cx, |home, cx| home.submit_prompt_for_capture(prompt, cx));
            return;
        }

        let prompt = prompt.to_owned();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(25))
                    .await;
                match this.update(cx, |this, cx| {
                    if !this.startup_model_catalog_resolved {
                        return false;
                    }
                    this.home
                        .update(cx, |home, cx| home.submit_prompt_for_capture(&prompt, cx));
                    true
                }) {
                    Ok(true) | Err(_) => return,
                    Ok(false) => {}
                }
            }
        })
        .detach();
    }
    pub fn show_user_message_actions_for_capture(&mut self, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.show_user_message_actions_for_capture(cx)
        });
    }
    pub fn set_command_tool_for_capture(
        &mut self,
        running: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_command_tool_for_capture(running, expanded, cx)
        });
    }
    pub fn set_context_compaction_for_capture(&mut self, running: bool, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_context_compaction_for_capture(running, cx)
        });
    }

    #[cfg(feature = "screenshot")]
    /// Opens the reference's inline message rewrite form over a deterministic
    /// conversation, without any app-server request.
    pub fn capture_message_edit(&mut self, cx: &mut Context<Self>) {
        let text = "Reply with exactly: p0 stage two".to_owned();
        self.complete_startup_for_capture(cx);
        self.home.update(cx, |home, cx| {
            home.seed_message_rewrite_for_capture(&text, cx)
        });
        cx.notify();
    }
    pub fn set_collaboration_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_collaboration_for_capture(state, cx));
    }
    pub fn set_mcp_tool_call_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_mcp_tool_call_for_capture(state, cx));
    }
    pub fn set_dynamic_tool_call_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_dynamic_tool_call_for_capture(state, cx)
        });
    }
    pub fn set_tool_group_for_capture(
        &mut self,
        running: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_tool_group_for_capture(running, expanded, cx)
        });
    }
    pub fn set_reasoning_for_capture(
        &mut self,
        state: &str,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_reasoning_for_capture(state, expanded, cx)
        });
    }
    pub fn set_approval_for_capture(&mut self, kind: &str, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_approval_for_capture(kind, state, cx)
        });
    }
    pub fn set_user_input_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_user_input_for_capture(state, cx));
    }
    pub fn set_mcp_elicitation_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_mcp_elicitation_for_capture(state, cx)
        });
    }
    pub fn set_file_approval_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_file_approval_for_capture(state, cx));
    }
    pub fn set_permissions_approval_for_capture(
        &mut self,
        kind: &str,
        state: &str,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_permissions_approval_for_capture(kind, state, cx)
        });
    }
    pub fn set_file_change_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_file_change_for_capture(state, cx));
    }
    pub fn set_turn_diff_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_file_change_for_capture("completed", cx)
        });
        self.open_diff_review(captured_diff_review_fixture(state), cx);
    }
    pub fn set_image_generation_for_capture(
        &mut self,
        state: &str,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_image_generation_for_capture(state, path, cx)
        });
        cx.notify();
    }
    pub fn enable_permission_ui_for_capture(&mut self, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.enable_permission_ui_for_capture(cx));
    }
    pub fn set_permission_mode_for_capture(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_permission_mode_for_capture(mode, cx)
        });
    }
    pub fn open_permission_menu_for_capture(&mut self, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.open_permission_menu_for_capture(cx));
    }
    pub fn set_permission_menu_capture_state(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_permission_menu_capture_state(state, cx)
        });
    }
    pub fn open_permission_confirmation_for_capture(&mut self, cx: &mut Context<Self>) {
        self.enable_permission_ui_for_capture(cx);
        self.open_permission_confirmation(self.home.read(cx).composer_entity(), cx);
        cx.notify();
    }
    pub fn open_project_creation_remote_for_capture(&mut self, cx: &mut Context<Self>) {
        self.open_project_creation(cx);
        self.project_creation.kind = ProjectCreationKind::Remote;
        self.project_creation.step = ProjectCreationStep::Remote;
        cx.notify();
    }
    /// `--right-panel-tool=side-chat|browser|terminal|files|review` opens
    /// that tool in the current chat's right panel; a comma-separated list
    /// opens one tab per entry, in order, leaving the last selected
    /// (`selected:` before an entry selects that one instead). `files=<path>`
    /// opens that file, and `review=unstaged|staged` shows that list.
    #[cfg(feature = "screenshot")]
    pub fn capture_right_panel_tool(&mut self, tools: &str, cx: &mut Context<Self>) {
        use super::panel_tabs::TabPlacement;
        self.complete_startup_for_capture(cx);
        self.right_panel.open = true;
        let mut selected = None;
        for entry in tools.split(',') {
            let (keep, entry) = match entry.strip_prefix("selected:") {
                Some(entry) => (true, entry),
                None => (false, entry),
            };
            let (tool, argument) = match entry.split_once('=') {
                Some((tool, argument)) => (tool, Some(argument)),
                None => (entry, None),
            };
            let mode = match tool {
                "side-chat" => RightPanelMode::SideChat,
                "browser" => RightPanelMode::Browser,
                "terminal" => RightPanelMode::Terminal,
                "files" => RightPanelMode::Files,
                "review" => RightPanelMode::Review,
                _ => {
                    eprintln!("unknown --right-panel-tool={tool}");
                    continue;
                }
            };
            match (mode, argument) {
                (RightPanelMode::Files, Some(path)) => {
                    let path = PathBuf::from(path);
                    self.open_in_files(|panel, cx| panel.open_path(path, None, cx), cx);
                }
                _ => self.open_panel_tool(mode, TabPlacement::Append, cx),
            }
            if mode == RightPanelMode::Review
                && let Some(scope) = argument
                && let Some(panel) = self.review_panels.get(&self.active_conversation)
            {
                let scope = match scope {
                    "unstaged" => crate::git_review::Scope::Unstaged,
                    "staged" => crate::git_review::Scope::Staged,
                    _ => crate::git_review::Scope::Uncommitted,
                };
                panel.update(cx, |panel, cx| panel.show_scope(scope, cx));
            }
            if keep {
                selected = self
                    .panel_tabs
                    .get(&self.active_conversation)
                    .map(|state| state.active);
            }
        }
        if let Some(index) = selected {
            self.activate_panel_tab(index, cx);
        }
    }
    #[cfg(feature = "screenshot")]
    pub fn capture_files(&mut self, root: PathBuf, path: Option<PathBuf>, cx: &mut Context<Self>) {
        let panel = cx.new(|cx| FilePanel::new(root, self.mode, cx));
        if let Some(path) = path {
            panel.update(cx, |p, cx| p.open_path(path, None, cx));
        }
        self.file_panels
            .insert(self.active_conversation.clone(), panel);
        self.right_panel.open = true;
        self.select_right_panel_item(3, cx);
    }
}

#[cfg(feature = "screenshot")]
impl ChatApp {
    pub fn set_progress_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |view, cx| view.set_progress_for_capture(state, cx));
        cx.notify();
    }
    pub fn set_runtime_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |view, cx| view.set_runtime_for_capture(state, cx));
        cx.notify();
    }
    /// `--hooks-settings-state`, `--experimental-features-state`,
    /// `--memories-state` and `--find-bar-state`.
    pub fn set_batch2_for_capture(
        &mut self,
        kind: &str,
        state: &str,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        match kind {
            "find-bar" => self
                .home
                .update(cx, |home, cx| home.set_find_for_capture(state, window, cx)),
            "hooks-settings" => self.settings.update(cx, |settings, cx| {
                settings.apply_hooks_capture_fixture(state, cx)
            }),
            "experimental-features" => self.settings.update(cx, |settings, cx| {
                settings.apply_features_capture_fixture(state, cx)
            }),
            "memories" if state.starts_with("settings") || state.starts_with("delete") => {
                self.settings.update(cx, |settings, cx| {
                    settings.apply_memories_capture_fixture(state, cx)
                })
            }
            "memories" => {
                let composer = self.home.read(cx).composer_entity();
                composer.update(cx, |composer, cx| {
                    composer.set_memories_for_capture(state, cx)
                });
                self.home
                    .update(cx, |home, cx| home.refresh_conversation_for_capture(cx));
            }
            _ => {}
        }
        cx.notify();
    }
    /// Batch three: `--review-menu-state`, `--review-turn-state`,
    /// `--review-delivery-state`, `--shell-mode-state`,
    /// `--memory-status-state`, `--capabilities-state`, `--sections-state`.
    pub fn set_batch3_for_capture(&mut self, kind: &str, state: &str, cx: &mut Context<Self>) {
        let composer = self.home.read(cx).composer_entity();
        match kind {
            "review-menu" => composer.update(cx, |composer, cx| {
                composer.set_review_menu_for_capture(state, cx)
            }),
            "review-turn" => composer.update(cx, |composer, cx| {
                composer.set_review_turn_for_capture(state, cx)
            }),
            "shell-mode" => composer.update(cx, |composer, cx| {
                composer.set_shell_mode_for_capture(state, cx)
            }),
            "memory-status" if state.starts_with("settings") => {
                let status = state.trim_start_matches("settings-").to_owned();
                self.settings.update(cx, |settings, cx| {
                    settings.apply_memory_status_capture_fixture(&status, cx)
                });
            }
            "memory-status" => composer.update(cx, |composer, cx| {
                composer.set_memory_status_for_capture(state, cx)
            }),
            "capabilities" => self.settings.update(cx, |settings, cx| {
                settings.apply_capabilities_capture_fixture(state, cx)
            }),
            "review-delivery" => {
                let delivery = if state == "detached" {
                    crate::workspace::ReviewDelivery::Detached
                } else {
                    crate::workspace::ReviewDelivery::Inline
                };
                self.settings.update(cx, |settings, cx| {
                    settings.set_review_delivery(delivery, cx)
                });
            }
            "sections" => {
                self.workspace_store.seed_custom_sections_for_capture();
                self.sidebar.update(cx, |sidebar, cx| {
                    sidebar.set_sections_for_capture(state, cx)
                });
            }
            _ => {}
        }
        self.home
            .update(cx, |home, cx| home.refresh_conversation_for_capture(cx));
        cx.notify();
    }
    /// `--queue-ui-state`, `--goal-ui-state`, `--auto-review-denial-state`.
    #[cfg(feature = "screenshot")]
    pub fn set_batch1_for_capture(&mut self, kind: &str, state: &str, cx: &mut Context<Self>) {
        let composer = self.home.read(cx).composer_entity();
        composer.update(cx, |composer, cx| match kind {
            "queue" => composer.set_queue_for_capture(state, cx),
            "goal" => composer.set_goal_for_capture(state, cx),
            "slash" => composer.set_slash_menu_for_capture(state, cx),
            _ => composer.set_auto_review_denial_for_capture(state, cx),
        });
        if kind == "goal"
            && state == "edit-tab"
            && let Some(goal) = composer.read(cx).capture_goal()
        {
            let text = goal.objective.clone();
            self.open_goal_editor(goal, text, cx);
        }
        self.home
            .update(cx, |home, cx| home.refresh_conversation_for_capture(cx));
        cx.notify();
    }
    /// `--batch4-state=<topic>:<state>` on a resumed fixture thread:
    /// `chips:hover-failing|hover-merged` (a hovered row and its card),
    /// `panel:closed|reopened|section-hover|pr-row-hover|pr-actions-menu|
    /// unmatched-pr-hover`, and `background:section|row-hover|row-focus|
    /// card-running|terminal-tab|stopping|stop-failed|card-stopped|
    /// card-finished`.
    #[cfg(feature = "screenshot")]
    pub fn set_batch4_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        let (topic, state) = state.split_once(':').unwrap_or(("", state));
        match topic {
            "chips" => {
                let title = match state {
                    "hover-failing" => Some("Fixture PR failing"),
                    "hover-merged" => Some("Fixture PR merged"),
                    _ => None,
                };
                if let Some(title) = title {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.hover_thread_for_capture(title, cx)
                    });
                }
            }
            "panel" => match state {
                // The header button flips the global pin, as a click would.
                "closed" => self.workspace_store.set_summary_panel_pinned(false),
                "open" | "reopened" => self.workspace_store.set_summary_panel_pinned(true),
                state => self
                    .summary_panel
                    .update(cx, |panel, cx| panel.set_capture_state(state, cx)),
            },
            "background" => {
                let composer = self
                    .conversation_hosts
                    .get(&self.active_conversation)
                    .map(|host| host.composer.clone());
                let fixture = match state {
                    "stopping" | "stop-failed" => state,
                    "card-stopped" => "stopped",
                    "card-finished" => "finished",
                    _ => "running",
                };
                if let Some(composer) = composer {
                    composer.update(cx, |composer, cx| {
                        composer.set_background_for_capture(fixture, cx)
                    });
                }
                if matches!(state, "row-hover" | "row-focus" | "terminal-tab") {
                    self.summary_panel
                        .update(cx, |panel, cx| panel.set_capture_state(state, cx));
                }
                self.home
                    .update(cx, |home, cx| home.refresh_conversation_for_capture(cx));
            }
            _ => {}
        }
        cx.notify();
    }
    /// The batch-four state is drawn: its row exists and no pull request,
    /// branch lookup or checkout is still loading.
    #[cfg(feature = "screenshot")]
    pub fn batch4_capture_ready(&self, cx: &gpui::App) -> bool {
        self.summary_panel.read(cx).capture_ready()
            && self.workspace_store.snapshot().pull_requests.is_idle()
    }
    pub fn set_streaming_reply_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |view, cx| {
            view.set_streaming_reply_for_capture(state, cx)
        });
        cx.notify();
    }
}
