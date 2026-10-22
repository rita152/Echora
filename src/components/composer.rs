#[cfg(test)]
use crate::agent::CodexAppServerBackend;

mod auto_review;
mod background;
mod capture;
mod context;
mod context_catalog;
mod context_menu;
mod context_thread;
mod mentions;
pub use context::OpenAttachmentPreview;
pub use context_catalog::{AttachBrowserContext, ContextBrowserTab, ContextSourcesRequested};
pub use mentions::InspectInputReference;
mod dialogs;
mod dictation;
mod followup;
mod followup_edit;
mod followup_render;
mod git_actions;
pub use git_actions::{GitActionsDetected, SyncThreadGitBranch};
mod command_catalog;
mod command_panels;
mod goal;
mod goal_render;
mod layout;
mod memories;
#[cfg(test)]
mod memories_tests;
mod permissions;
mod pet;
pub(crate) use pet::DesktopPet;
mod picker;
mod project_picker;
mod render;
mod requests;
mod review;
mod runtime;
mod send;
mod shell;
mod side_chat;
mod sketch;
mod sketch_image;
mod slash_menu;
mod slash_menu_render;
mod submissions;
mod task;
mod toast;
pub(crate) use task::SubmitTaskEditor;
mod workspace;
mod workspace_controls;
mod workspace_controls_render;
#[cfg(test)]
mod workspace_controls_tests;
pub(crate) use project_picker::projectless_cwd;
pub use project_picker::{DraftProjectSelected, OpenProjectCreation};
pub use workspace::CheckoutChanged;
pub use workspace_controls::{DraftWorkspaceChanged, OpenWorkspaceGitSettings};

#[cfg(feature = "screenshot")]
pub(crate) use capture::capture_find;
pub use followup::FollowUpModeToggled;
pub use followup_edit::OpenQueuedInSideChat;
pub(crate) use followup_render::TrayTooltip;
pub use goal::OpenGoalEditor;
pub(crate) use goal_render::achieved_duration_label;
pub use review::{CodeReviewStarted, StartDetachedReview};
pub(crate) use toast::{
    TOAST_DURATION, ToastAction, ToastKind, VISIBLE_TOASTS, toast_card, toast_stack,
};
pub use workspace::WorkspacePresentation;

use std::{cell::Cell, path::PathBuf, rc::Rc, sync::Arc};

use gpui::{Context, Entity, FocusHandle, Focusable, prelude::*};

use crate::{
    agent::{
        AgentBackend, AgentConnectionEvent, AgentEvent, AgentPermissionMode, ProjectId,
        ThreadHistory,
    },
    components::{
        prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
        user_input_request::UserInputRequestEvent,
    },
    conversation::{
        ConversationActivity, ConversationPhase, ConversationState, ConversationTranscriptTurn,
        ResumedTurnPresentation,
    },
    theme::ThemeMode,
};

pub(crate) const COMPOSER_CORNER_RADIUS: f32 = 24.0;

/// The reference exposes manual context compaction as the "Compact" slash
/// command. The composer accepts its typed form.
pub(crate) const COMPACT_COMMAND: &str = "/compact";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DictationState {
    #[default]
    Idle,
    Recording,
    Transcribing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermissionMode {
    Request,
    Assist,
    Full,
}

impl PermissionMode {
    /// The three permission modes shared by the trigger and menu.
    const MENU: [Self; 3] = [Self::Request, Self::Assist, Self::Full];

    const fn agent_mode(self) -> AgentPermissionMode {
        match self {
            Self::Request => AgentPermissionMode::Request,
            Self::Assist => AgentPermissionMode::Assist,
            Self::Full => AgentPermissionMode::Full,
        }
    }
}

/// Opens the native full-access confirmation dialog.
///
/// The original composer remains the target while the confirmation is open.
pub struct RequestFullAccessConfirmation;
impl gpui::EventEmitter<RequestFullAccessConfirmation> for ComposerView {}

pub struct ModelCatalogLoadFinished;
impl gpui::EventEmitter<ModelCatalogLoadFinished> for ComposerView {}

pub struct ToggleDesktopPet;
impl gpui::EventEmitter<ToggleDesktopPet> for ComposerView {}
pub struct FeedbackContextRequested;
impl gpui::EventEmitter<FeedbackContextRequested> for ComposerView {}

pub struct SlashChatAction(pub(crate) slash_menu::SlashCommand);
impl gpui::EventEmitter<SlashChatAction> for ComposerView {}
pub(crate) use slash_menu::SlashCommand;

pub struct PermissionSelectionChanged(pub(crate) crate::agent::AgentPermissionMode);
impl gpui::EventEmitter<PermissionSelectionChanged> for ComposerView {}

pub struct ConversationChanged;
impl gpui::EventEmitter<ConversationChanged> for ComposerView {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationThreadCreated {
    pub thread_id: String,
}
impl gpui::EventEmitter<ConversationThreadCreated> for ComposerView {}
pub struct OpenReviewComments;
impl gpui::EventEmitter<OpenReviewComments> for ComposerView {}

pub struct ReviewCommentsSubmitted;
impl gpui::EventEmitter<ReviewCommentsSubmitted> for ComposerView {}
pub struct ReviewCommentsRestored(pub Vec<crate::git_review::ReviewComment>);
impl gpui::EventEmitter<ReviewCommentsRestored> for ComposerView {}

gpui::actions!(
    composer,
    [
        DismissModelPicker,
        DismissProjectPicker,
        DismissWorkspaceMenu
    ]
);

const MODEL_PICKER_WIDTH: f32 = 254.0;
const MODEL_PICKER_MENU_GAP: f32 = 10.0;
const MODEL_PICKER_TRIGGER_GAP: f32 = 4.0;
const MODEL_PICKER_RIGHT_INSET: f32 = 72.0;
const PARTICLE_TIMELINE_MS: f32 = 120_000.0;
pub struct ComposerView {
    task_editor: bool,
    task_options_open: bool,
    conversation: ConversationState,
    backend: Arc<dyn AgentBackend>,
    mode: ThemeMode,
    prompt_editor: Entity<crate::components::file_editor::FileEditor>,
    side_chat: bool,
    side_ready: bool,
    send_opacity: send::SendOpacity,
    available_width: Option<f32>,
    trailing_margin: Option<f32>,
    prompt_context: crate::agent::AgentPromptContext,
    context_menu_open: bool,
    context_catalog: context_catalog::ContextCatalog,
    focus_prompt_pending: bool,
    review_comments: Vec<crate::git_review::ReviewComment>,
    draft_revision: u64,
    submission_error: Option<String>,
    user_input_other_input: Entity<PromptInput>,
    /// Shared inline editor for the focused MCP elicitation text field. The
    /// card renders whichever field currently owns logical focus.
    mcp_elicitation_input: Entity<PromptInput>,
    model_menu_focus: FocusHandle,
    model_trigger_focus: FocusHandle,
    model_trigger_bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    model_menu_focused_item: usize,
    model_menu_keyboard_focus: bool,
    menu_open: bool,
    model_list_open: bool,
    model_list_scroll: gpui::ScrollHandle,
    slider_dragging: bool,
    model_ultra_warning_until: Option<std::time::Instant>,
    dictation_state: DictationState,
    /// The permission selector is part of the normal Composer UI. Capture
    /// helpers still use this flag to make fixture setup explicit, but product
    /// launches enable it by default; visual similarity is no longer a
    /// visibility gate.
    permission_ui_enabled: bool,
    connection_event_task: Option<gpui::Task<()>>,
    permission_mode: PermissionMode,
    permission_update_cycle: u64,
    /// The backend selection can inherit defaults or retain a named profile;
    /// its presentation always uses one of the three permission modes.
    permission_selection: AgentPermissionMode,
    permission_confirmation_selection: Option<AgentPermissionMode>,
    permission_catalog_cycle: u64,
    permission_catalog_loading: bool,
    permission_effective_loading: bool,
    permission_read_cycle: u64,
    permission_catalog_error: Option<String>,
    permission_config: Option<crate::agent::AgentConfigSnapshot>,
    /// Whether the `memories` feature is enabled, for the generation read.
    memories_feature: Option<(u64, bool)>,
    memories_feature_loading: bool,
    permission_profiles: Vec<crate::agent::AgentPermissionProfile>,
    permission_menu_focus: FocusHandle,
    permission_menu_focused_item: usize,
    permission_menu_scroll: gpui::ScrollHandle,
    permission_menu_keyboard_focus: bool,
    permission_menu_open: bool,
    approval_resolved_capture: bool,
    workspace: WorkspacePresentation,
    checkout_cycle: u64,
    project_picker: project_picker::ProjectPicker,
    workspace_controls: workspace_controls::WorkspaceControls,
    follow_up_mode: crate::workspace::FollowUpMode,
    review_delivery: crate::workspace::ReviewDelivery,
    /// The `/review` submenu's base branches, and the read they came from.
    review_branches: review::ReviewBranches,
    review_branches_cycle: u64,
    /// The cycle of a review whose turn has not been accepted yet.
    review_start_cycle: Option<u64>,
    /// A review waiting for this conversation's settings to load.
    pending_review: Option<crate::agent::AgentReviewTarget>,
    /// What the configured model provider supports; `None` while unknown,
    /// which gates nothing.
    provider_capabilities: Option<crate::agent::AgentProviderCapabilities>,
    /// `memory/status`, read when the memories dialog opens.
    memory_status: Option<crate::agent::AgentMemoryStatus>,
    /// A queued row loaded into the composer for editing.
    queue_edit: Option<followup_edit::QueueEdit>,
    queue_removal: Option<followup_edit::RemovedQueuedMessage>,
    queue_redo: Option<followup_edit::RemovedQueuedMessage>,
    /// Row whose actions menu is open.
    queue_menu: Option<String>,
    queue_drag: Option<followup_render::QueueDrag>,
    /// The user resumed or sent after stopping, so the stopped turn no longer
    /// pauses the queue.
    queue_resumed: bool,
    /// Server-started turns waiting for the current run to end.
    pending_external_turns: std::collections::VecDeque<crate::agent::AgentRun>,
    goal_draft: bool,
    /// Full text behind long goal objectives sent as file pointers.
    goal_texts: std::collections::HashMap<String, String>,
    /// Where long goal objectives are written, resolved once.
    codex_home: Option<std::path::PathBuf>,
    slash_menu: Option<slash_menu::SlashMenu>,
    command_catalog: command_catalog::CommandCatalog,
    command_panels: command_panels::CommandPanels,
    sketch: sketch::SketchState,
    slash_scroll: gpui::ScrollHandle,
    toasts: Vec<toast::ComposerToast>,
    toast_actions: std::collections::HashMap<u64, toast::ToastAction>,
    next_toast_id: u64,
    /// Objective shown as the request of the turn the server starts for it.
    pending_goal_bubble: Option<String>,
    /// A goal submitted from a new chat, set once its first turn is accepted.
    goal_after_first_turn: Option<String>,
    /// `updatedAt` of the complete goal this composer already cleared.
    auto_cleared_goal: Option<i64>,
    dialog: Option<dialogs::ComposerDialog>,
    /// `collaborationMode/list` of the connection; plan is offered only when
    /// it lists a plan preset, as the reference hides plan otherwise.
    collaboration_modes: Option<crate::agent::AgentCollaborationModes>,
    dialog_focus: FocusHandle,
    dialog_focus_pending: bool,
}

impl ComposerView {
    #[cfg(test)]
    pub fn new(mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let backend: Arc<dyn AgentBackend> = Arc::new(CodexAppServerBackend::new());
        Self::new_with_backend(mode, backend, cx)
    }

    pub fn new_with_backend(
        mode: ThemeMode,
        backend: Arc<dyn AgentBackend>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.bind_keys([gpui::KeyBinding::new(
            "escape",
            DismissModelPicker,
            Some("ComposerModelPicker"),
        )]);
        cx.bind_keys([gpui::KeyBinding::new(
            "escape",
            DismissProjectPicker,
            Some("ComposerProjectPicker"),
        )]);
        cx.bind_keys([
            gpui::KeyBinding::new(
                "escape",
                DismissWorkspaceMenu,
                Some("ComposerWorkspaceMenu"),
            ),
            // Context-free bindings match at the deepest focus node too.
            gpui::KeyBinding::new(
                "escape",
                DismissWorkspaceMenu,
                Some("ComposerWorkspaceMenu > PromptInput"),
            ),
        ]);
        let project_picker = project_picker::ProjectPicker::new(mode, cx);
        let workspace_controls = workspace_controls::WorkspaceControls::new(mode, cx);
        let connection_events = backend.subscribe_connection_events();
        let prompt_editor = cx.new(|cx| {
            let mut editor = crate::components::file_editor::FileEditor::composer(mode, cx);
            editor.set_accessible_name("聊天输入框");
            editor
        });
        let user_input_other_input = cx.new(|cx| {
            PromptInput::inline_other(mode, "否，并告诉 ChatGPT 应该如何做得不同", false, cx)
        });
        let mcp_elicitation_input = cx.new(|cx| PromptInput::inline_other(mode, "", false, cx));
        cx.subscribe(
            &prompt_editor,
            |this, editor, event: &crate::components::file_editor::EditorEvent, cx| {
                match event {
                    crate::components::file_editor::EditorEvent::Changed => {
                        this.draft_revision = this.draft_revision.wrapping_add(1);
                        this.prompt_context.references = editor.read(cx).references().to_vec();
                        this.update_mentions(false, cx);
                        this.update_slash_menu(cx);
                    }
                    crate::components::file_editor::EditorEvent::Submit => {
                        // Enter picks the highlighted slash command instead.
                        if !this.context_menu_key("enter", false, cx) && !this.slash_menu_enter(cx)
                        {
                            this.submit_prompt(editor.read(cx).text().to_owned(), cx);
                        }
                    }
                    crate::components::file_editor::EditorEvent::SubmitInverted => {
                        if !this.context_menu_key("enter", false, cx) && !this.slash_menu_enter(cx)
                        {
                            this.submit_prompt_as(editor.read(cx).text().to_owned(), true, cx);
                        }
                    }
                    crate::components::file_editor::EditorEvent::SelectionChanged => {
                        this.update_mentions(true, cx);
                        this.update_slash_menu(cx)
                    }
                    crate::components::file_editor::EditorEvent::Save
                    | crate::components::file_editor::EditorEvent::HeightChanged => {}
                }
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(
            &prompt_editor,
            |_, _, event: &crate::components::file_editor::ReferenceActivated, cx| {
                cx.emit(InspectInputReference(event.0.clone()));
            },
        )
        .detach();
        cx.subscribe(
            &user_input_other_input,
            |this, input, _: &PromptChanged, cx| {
                let answer = input.read(cx).text().to_owned();
                if let Some(model) = this
                    .conversation
                    .activities
                    .iter_mut()
                    .find_map(|activity| {
                        let ConversationActivity::UserInput(model) = activity else {
                            return None;
                        };
                        model.is_interactive().then_some(model)
                    })
                {
                    model.save_other_answer(answer);
                    model.focus_other_answer();
                    cx.emit(ConversationChanged);
                }
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(
            &user_input_other_input,
            |this, _, event: &PromptSubmitted, cx| {
                let pending = this.conversation.activities.iter().find_map(|activity| {
                    let ConversationActivity::UserInput(model) = activity else {
                        return None;
                    };
                    let question = model.current_question()?;
                    model.is_interactive().then(|| {
                        (
                            model.request_id.clone(),
                            question.id.clone(),
                            event.0.clone(),
                        )
                    })
                });
                if let Some((request_id, question_id, answer)) = pending {
                    this.handle_user_input_request_event(
                        &request_id,
                        UserInputRequestEvent::SubmitOtherAnswer {
                            question_id,
                            answer,
                        },
                        cx,
                    );
                }
            },
        )
        .detach();
        cx.subscribe(
            &mcp_elicitation_input,
            |this, input, _: &PromptChanged, cx| {
                let text = input.read(cx).text().to_owned();
                if this.set_focused_mcp_elicitation_text(text) {
                    cx.emit(ConversationChanged);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(
            &mcp_elicitation_input,
            |this, _, _: &PromptSubmitted, cx| {
                let Some(request_id) = this.focused_mcp_elicitation_request_id() else {
                    return;
                };
                this.handle_mcp_elicitation_event(
                    &request_id,
                    crate::components::mcp_elicitation::McpElicitationEvent::Accept,
                    cx,
                );
            },
        )
        .detach();
        let mut view = Self {
            task_editor: false,
            task_options_open: false,
            conversation: ConversationState::default(),
            backend,
            mode,
            prompt_editor,
            side_chat: false,
            side_ready: true,
            send_opacity: Default::default(),
            available_width: None,
            trailing_margin: None,
            prompt_context: Default::default(),
            context_menu_open: false,
            context_catalog: context_catalog::ContextCatalog::default(),
            focus_prompt_pending: false,
            review_comments: Vec::new(),
            draft_revision: 0,
            submission_error: None,
            user_input_other_input,
            mcp_elicitation_input,
            model_menu_focus: cx.focus_handle(),
            model_trigger_focus: cx.focus_handle().tab_stop(true),
            model_trigger_bounds: Rc::new(Cell::new(None)),
            model_menu_focused_item: 0,
            model_menu_keyboard_focus: false,
            menu_open: false,
            model_list_open: false,
            model_list_scroll: gpui::ScrollHandle::new(),
            slider_dragging: false,
            model_ultra_warning_until: None,
            dictation_state: DictationState::Idle,
            permission_ui_enabled: true,
            connection_event_task: None,
            permission_mode: PermissionMode::Request,
            permission_update_cycle: 0,
            permission_selection: AgentPermissionMode::Custom,
            permission_confirmation_selection: None,
            permission_catalog_cycle: 0,
            permission_catalog_loading: false,
            permission_effective_loading: false,
            permission_read_cycle: 0,
            permission_catalog_error: None,
            permission_config: None,
            memories_feature: None,
            memories_feature_loading: false,
            permission_profiles: Vec::new(),
            permission_menu_focus: cx.focus_handle().tab_stop(true),
            permission_menu_focused_item: 0,
            permission_menu_scroll: gpui::ScrollHandle::new(),
            permission_menu_keyboard_focus: false,
            permission_menu_open: false,
            approval_resolved_capture: false,
            workspace: WorkspacePresentation::default(),
            checkout_cycle: 0,
            project_picker,
            workspace_controls,
            follow_up_mode: Default::default(),
            review_delivery: Default::default(),
            review_branches: Default::default(),
            review_branches_cycle: 0,
            review_start_cycle: None,
            pending_review: None,
            provider_capabilities: None,
            memory_status: None,
            queue_edit: None,
            queue_removal: None,
            queue_redo: None,
            queue_menu: None,
            queue_drag: None,
            queue_resumed: false,
            pending_external_turns: Default::default(),
            goal_draft: false,
            goal_texts: std::collections::HashMap::new(),
            codex_home: crate::agent::codex_home(),
            slash_menu: None,
            command_catalog: Default::default(),
            command_panels: command_panels::CommandPanels::new(cx),
            sketch: sketch::SketchState::default(),
            slash_scroll: gpui::ScrollHandle::new(),
            toasts: Vec::new(),
            toast_actions: Default::default(),
            next_toast_id: 0,
            pending_goal_bubble: None,
            goal_after_first_turn: None,
            auto_cleared_goal: None,
            dialog: None,
            collaboration_modes: None,
            dialog_focus: cx.focus_handle(),
            dialog_focus_pending: false,
        };
        view.consume_connection_events(connection_events, cx);
        #[cfg(not(test))]
        {
            view.load_model_catalog(cx);
            view.load_permission_catalog(cx);
            view.load_collaboration_modes(cx);
        }
        view
    }

    #[cfg(test)]
    pub fn conversation_snapshot(
        &self,
    ) -> (
        ConversationPhase,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
    ) {
        self.conversation.conversation_snapshot()
    }

    #[cfg(test)]
    pub fn conversation_activity_snapshot(&self) -> Vec<ConversationActivity> {
        self.conversation.activity_snapshot()
    }

    pub fn conversation_phase(&self) -> ConversationPhase {
        self.conversation.phase()
    }

    pub fn has_active_context_compaction(&self) -> bool {
        self.conversation.has_active_context_compaction()
    }

    pub fn has_active_plan(&self) -> bool {
        self.conversation.has_active_plan()
    }
    pub fn has_active_image_generation(&self) -> bool {
        self.conversation.has_active_image_generation()
    }

    /// The assistant message still receiving deltas, if any.
    pub fn streaming_assistant_message_id(&self) -> Option<String> {
        self.conversation
            .streaming_assistant_message_id()
            .map(ToOwned::to_owned)
    }

    pub fn transcript_render_snapshot(&self) -> Vec<ConversationTranscriptTurn> {
        self.conversation.transcript_render_snapshot()
    }

    pub fn thread_id(&self) -> Option<&str> {
        self.conversation.thread_id()
    }

    /// The agent this conversation talks to.
    pub(crate) fn agent_backend(&self) -> Arc<dyn AgentBackend> {
        self.backend.clone()
    }

    /// The current turn's id, once the server named it.
    pub fn current_turn_id(&self) -> Option<&str> {
        self.conversation.turn_id.as_deref()
    }

    /// Re-reads this conversation's history through the ordinary loader, as
    /// a revert this client did not make would.
    pub fn request_history_reload(&mut self, cx: &mut Context<Self>) {
        if let Some(thread_id) = self.conversation.thread_id.clone()
            && self.conversation.mark_history_stale(&thread_id)
        {
            cx.emit(ConversationChanged);
        }
    }

    pub fn history_needs_retry(&self) -> bool {
        self.conversation.history_needs_retry()
    }

    /// Whether a revert changed this conversation's durable history without a
    /// local request, so the host has to reload the turns.
    pub fn history_needs_reload(&self) -> bool {
        self.conversation.history_needs_reload()
    }

    pub fn clear_history_stale(&mut self) {
        self.conversation.clear_history_stale();
    }

    /// Writes text into the composer without sending it; the Pull Requests page
    /// uses this when `Chat` starts a conversation for a pull request.
    pub fn set_draft_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.context_menu_open = false;
        self.context_catalog.cancel();
        self.context_catalog.mention_start = None;
        self.context_catalog.mention_dismissed = None;
        self.prompt_editor
            .update(cx, |editor, cx| editor.set_text_silently(text, cx));
        self.prompt_context.references.clear();
        self.update_slash_menu(cx);
        self.draft_revision = self.draft_revision.wrapping_add(1);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn pending_input_for_test(
        &self,
        cx: &gpui::App,
    ) -> (String, crate::agent::AgentPromptContext) {
        (
            self.prompt_editor.read(cx).text().to_owned(),
            self.prompt_context.clone(),
        )
    }

    /// Enters the reference's rewrite mode for the newest user message and
    /// returns its text for the transcript's inline editor. The transcript owns
    /// the editor; the composer only owns the revert that submitting it runs.
    pub fn begin_message_edit(&mut self, cx: &mut Context<Self>) -> Option<String> {
        if self.conversation.message_edit_turn_id.is_some() {
            return None;
        }
        let (turn_id, text) = self.conversation.editable_user_turn()?;
        self.conversation.begin_message_edit(turn_id);
        cx.emit(ConversationChanged);
        cx.notify();
        Some(text)
    }

    pub fn cancel_message_edit(&mut self, cx: &mut Context<Self>) {
        if self.conversation.message_edit_turn_id.is_none() {
            return;
        }
        self.conversation.cancel_message_edit();
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub fn message_edit_active(&self) -> bool {
        self.conversation.message_edit_turn_id.is_some()
    }

    /// Whether the newest user message can be rewritten right now.
    pub fn message_edit_available(&self) -> bool {
        self.conversation.message_edit_turn_id.is_some()
            || self.conversation.editable_user_turn().is_some()
    }

    pub fn history_loading(&self) -> bool {
        self.conversation.history_loading()
    }

    #[cfg(feature = "screenshot")]
    pub fn history_error(&self) -> Option<&str> {
        self.conversation.history_error()
    }

    pub fn set_workspace_context(
        &mut self,
        cwd: PathBuf,
        project_id: Option<ProjectId>,
        thread_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.conversation.cwd != cwd
            || self.conversation.thread_id != thread_id
            || self.conversation.project_id != project_id
        {
            self.context_menu_open = false;
            self.context_catalog.cancel();
            self.context_catalog.mention_start = None;
            self.context_catalog.mention_dismissed = None;
            self.permission_update_cycle = self.permission_update_cycle.wrapping_add(1);
            self.conversation.permission_change = None;
            self.permission_confirmation_selection = None;
        }
        let changed_cwd = self.conversation.cwd != cwd;
        let changed_project = self.conversation.project_id != project_id;
        if changed_project || (changed_cwd && !self.workspace_controls.owns_cwd(&cwd)) {
            self.workspace_controls.reset();
        }
        self.conversation
            .set_workspace_context(cwd, project_id, thread_id);
        self.sync_thread_scoped_state(cx);
        if changed_cwd {
            self.command_catalog.invalidate();
            self.slash_menu = None;
            self.permission_config = None;
            self.load_permission_catalog(cx);
        }
        self.refresh_checkout(changed_cwd || changed_project, cx);
        cx.notify();
    }

    pub fn set_history_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.conversation.set_history_loading(loading);
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub fn set_history_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.conversation.set_history_error(error);
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub fn hydrate_history(&mut self, history: ThreadHistory, cx: &mut Context<Self>) {
        let thread_id = history.thread.thread_id.clone();
        // Checked before hydrating: the history marks its in-progress turn as
        // running, but only a stream this view already consumes blocks a claim.
        let streaming_locally = self.is_running();
        self.conversation.hydrate_history(history);
        self.load_effective_permissions(cx);
        // A turn the server started before this chat was opened keeps
        // streaming; claim it and let its replayed events rebuild the turn.
        if !streaming_locally
            && let Some((turn_id, run)) = self.backend.take_server_turn(&thread_id)
        {
            self.conversation.drop_history_turn(&turn_id);
            self.attach_external_turn(run, cx);
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub fn user_images(&self) -> Vec<crate::agent::UserMessageAttachment> {
        self.conversation.user_images()
    }

    pub fn current_goal_marks(&self) -> crate::conversation::TurnGoalMarks {
        crate::conversation::TurnGoalMarks {
            sent_as_goal: self.conversation.user_message_goal,
            achieved_seconds: self.conversation.goal_achieved_seconds,
            review_request: self.conversation.user_message_review,
        }
    }

    pub fn resumed_turn(&self) -> Option<ResumedTurnPresentation> {
        self.conversation.turn_presentation()
    }

    pub fn conversation_render_snapshot(
        &self,
    ) -> (
        ConversationPhase,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Vec<ConversationActivity>,
    ) {
        self.conversation.conversation_render_snapshot()
    }

    fn apply_connection_event(
        &mut self,
        event: AgentConnectionEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        match &event {
            AgentConnectionEvent::Runtime(runtime)
                if runtime.generation > self.conversation.runtime.generation
                    || (runtime.generation == self.conversation.runtime.generation
                        && matches!(
                            runtime.observation,
                            crate::agent::AgentRuntimeObservation::Disconnected
                        )) =>
            {
                self.command_catalog.invalidate();
                self.context_catalog.cancel();
                self.context_catalog.mention_start = None;
                self.context_catalog.mention_dismissed = None;
                self.context_menu_open = false;
                self.slash_menu = None;
            }
            AgentConnectionEvent::SkillsChanged { generation }
                if *generation >= self.conversation.runtime.generation =>
            {
                self.command_catalog.invalidate();
                if self.slash_menu_open(cx) || self.context_menu_open {
                    self.ensure_command_skills(cx);
                }
            }
            AgentConnectionEvent::TurnStarted {
                generation,
                thread_id,
                run,
                ..
            } => {
                self.offer_external_turn(*generation, thread_id, run, cx);
                return true;
            }
            AgentConnectionEvent::ThreadQueueChanged {
                generation,
                thread_id,
            } => {
                self.queue_changed(*generation, thread_id, cx);
                return false;
            }
            _ => {}
        }
        if let AgentConnectionEvent::BackgroundCommandUpdated {
            thread_id,
            event: completed,
            ..
        } = &event
            && self.conversation.thread_id.as_ref() == Some(thread_id)
        {
            self.report_git_actions(std::slice::from_ref(completed), cx);
        }
        let refresh_permissions = !self.side_chat
            && matches!(&event, AgentConnectionEvent::Runtime(runtime)
                if matches!(runtime.observation, crate::agent::AgentRuntimeObservation::GenerationStarted)
                    && self.permission_config.as_ref().is_some_and(|config| config.generation < runtime.generation))
            && !self.permission_catalog_loading;
        let goal_event = matches!(
            event,
            AgentConnectionEvent::ThreadGoalUpdated { .. }
                | AgentConnectionEvent::ThreadGoalCleared { .. }
        );
        let changed = self.apply_scoped_connection_event(event);
        if refresh_permissions {
            self.load_permission_catalog(cx);
        }
        if goal_event && changed {
            self.after_goal_change(cx);
        }
        changed
    }

    fn apply_scoped_connection_event(&mut self, event: AgentConnectionEvent) -> bool {
        if let AgentConnectionEvent::ThreadSettingsUpdated { generation, .. } = &event
            && (*generation < self.conversation.runtime.generation
                || self
                    .permission_config
                    .as_ref()
                    .is_some_and(|config| config.generation > *generation))
        {
            return false;
        }
        if let AgentConnectionEvent::ThreadSettingsUpdated {
            thread_id,
            settings,
            ..
        } = &event
            && self.conversation.thread_id.as_ref() == Some(thread_id)
            && self.conversation.permission_change.is_none()
            && let Some(permissions) = &settings.permissions
        {
            self.sync_permission_selection(permissions);
        }
        self.conversation.apply_connection_event(event)
    }

    fn apply_agent_event_batch(&mut self, events: Vec<AgentEvent>) -> bool {
        self.conversation.apply_agent_event_batch(events)
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.project_picker
            .input
            .update(cx, |input, cx| input.set_mode(mode, cx));
        self.prompt_editor
            .update(cx, |editor, cx| editor.set_mode(mode, cx));
        self.user_input_other_input
            .update(cx, |input, cx| input.set_mode(mode, cx));
        cx.notify();
    }

    pub fn set_review_comments(
        &mut self,
        comments: Vec<crate::git_review::ReviewComment>,
        cx: &mut Context<Self>,
    ) {
        if self.review_comments != comments {
            self.draft_revision = self.draft_revision.wrapping_add(1);
            self.review_comments = comments;
        }
        cx.notify();
    }

    pub fn latest_review(&self) -> Option<crate::components::file_change::DiffReviewPresentation> {
        let groups = std::iter::once(&self.conversation.activities).chain(
            self.conversation
                .transcript
                .iter()
                .rev()
                .map(|t| &t.activities),
        );
        for activities in groups {
            if let Some(review) = activities.iter().rev().find_map(|a| {
                if let crate::conversation::ConversationActivity::FileChange(c) = a {
                    c.review
                        .review_id
                        .starts_with("turn-diff-")
                        .then(|| c.review.clone())
                } else {
                    None
                }
            }) {
                return Some(review);
            }
            let mut raw = String::new();
            let mut files = Vec::new();
            for activity in activities {
                if let crate::conversation::ConversationActivity::FileChange(change) = activity
                    && change.status == crate::agent::AgentFileChangeStatus::Completed
                {
                    files.extend(change.review.files.iter().cloned());
                    if let Some(patch) = &change.review.raw_diff {
                        raw.push_str(patch);
                    }
                }
            }
            if !files.is_empty() {
                let mut review = crate::components::file_change::DiffReviewPresentation::new(
                    format!(
                        "latest-{}-{}",
                        self.conversation.thread_id.as_deref().unwrap_or("draft"),
                        self.conversation.cycle
                    ),
                    crate::i18n::text("上一轮"),
                    files,
                );
                review.raw_diff = (!raw.is_empty()).then_some(raw.into());
                return Some(review);
            }
        }
        None
    }

    pub fn prompt_focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.prompt_editor.read(cx).focus_handle(cx)
    }

    pub fn user_input_other_entity(&self) -> Entity<PromptInput> {
        self.user_input_other_input.clone()
    }

    pub fn user_input_other_focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.user_input_other_input.read(cx).focus_handle(cx)
    }

    pub fn mcp_elicitation_focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.mcp_elicitation_input.read(cx).focus_handle(cx)
    }

    pub fn mcp_elicitation_input_entity(&self) -> Entity<PromptInput> {
        self.mcp_elicitation_input.clone()
    }
}

impl ComposerView {}

#[cfg(test)]
mod approval_tests;
#[cfg(test)]
mod batch3_tests;
#[cfg(test)]
mod elicitation_tests;
#[cfg(test)]
mod followup_tests;
#[cfg(test)]
mod tests;
