//! State and focus ownership for independent application panels.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use gpui::{AppContext, Context, Entity, FocusHandle};

use super::ChatApp;
use crate::{
    agent::ThreadId,
    components::{file_change::DiffReviewPresentation, home::HomeView},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RightPanelMode {
    Review,
    Files,
    SideChat,
    Browser,
    Terminal,
}

#[derive(Clone)]
pub(super) struct SubagentPanel {
    pub(super) thread_id: ThreadId,
    pub(super) name: String,
    pub(super) home: Entity<HomeView>,
}

pub(super) struct SidebarLayoutState {
    pub(super) collapsed: bool,
    pub(super) resize_hovered: bool,
    pub(super) resize_dragging: bool,
    pub(super) resize_pointer_offset: f32,
    pub(super) reveal: f32,
    pub(super) animation_from: f32,
    pub(super) animation_to: f32,
    pub(super) animation_started_at: Option<Instant>,
    pub(super) animation_duration: Duration,
    pub(super) animation_running: bool,
}

impl Default for SidebarLayoutState {
    fn default() -> Self {
        Self {
            collapsed: false,
            resize_hovered: false,
            resize_dragging: false,
            resize_pointer_offset: 0.0,
            reveal: 1.0,
            animation_from: 1.0,
            animation_to: 1.0,
            animation_started_at: None,
            animation_duration: Duration::ZERO,
            animation_running: false,
        }
    }
}

pub(super) struct RightPanelState {
    pub(super) open: bool,
    pub(super) mode: Option<RightPanelMode>,
    pub(super) focused_item: usize,
    pub(super) keyboard_focus: bool,
    pub(super) focus: FocusHandle,
    pub(super) focus_pending: bool,
    pub(super) width: Option<f32>,
    pub(super) fullscreen: bool,
    pub(super) resize_dragging: bool,
    pub(super) resize_pointer_offset: f32,
    pub(super) subagent: Option<SubagentPanel>,
    pub(super) subagent_menu_open: bool,
    pub(super) diff_review: Option<DiffReviewPresentation>,
}

impl RightPanelState {
    pub(super) fn new(cx: &mut Context<ChatApp>) -> Self {
        Self {
            open: false,
            mode: None,
            focused_item: 0,
            keyboard_focus: false,
            focus: cx.focus_handle().tab_stop(true),
            focus_pending: false,
            width: None,
            fullscreen: false,
            resize_dragging: false,
            resize_pointer_offset: 0.0,
            subagent: None,
            subagent_menu_open: false,
            diff_review: None,
        }
    }
}

pub(super) struct ImagePreviewState {
    pub(super) composer: Option<gpui::WeakEntity<crate::components::composer::ComposerView>>,
    pub(super) path: Option<PathBuf>,
    pub(super) focus: FocusHandle,
    pub(super) focus_active: bool,
    pub(super) previous_focus: Option<FocusHandle>,
    pub(super) dimensions: Option<(u32, u32)>,
    pub(super) zoom: f32,
}

impl ImagePreviewState {
    pub(super) fn new(cx: &mut Context<ChatApp>) -> Self {
        Self {
            path: None,
            composer: None,
            focus: cx.focus_handle(),
            focus_active: false,
            previous_focus: None,
            dimensions: None,
            zoom: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum ProjectConfirmation {
    Remove {
        project_id: crate::agent::ProjectId,
        name: String,
    },
    Archive {
        project_id: crate::agent::ProjectId,
        name: String,
    },
}

pub(super) struct ProjectCreationState {
    pub(super) open: bool,
    pub(super) name: Entity<crate::components::prompt_input::PromptInput>,
    pub(super) roots: Vec<PathBuf>,
    pub(super) editing: Option<crate::agent::ProjectId>,
    pub(super) confirmation: Option<ProjectConfirmation>,
    pub(super) creating: bool,
    pub(super) error: Option<String>,
    pub(super) device_menu: bool,
    pub(super) marker_menu: bool,
    pub(super) marker: Option<crate::workspace::ProjectMarker>,
    pub(super) marker_search: Entity<crate::components::prompt_input::PromptInput>,
    pub(super) marker_emoji: bool,
    pub(super) marker_focused: Option<usize>,
    pub(super) marker_category: Option<usize>,
    pub(super) marker_scroll: gpui::UniformListScrollHandle,
    pub(super) cycle: u64,
    pub(super) previous_focus: Option<FocusHandle>,
    pub(super) focus: FocusHandle,
    pub(super) focus_pending: bool,
    pub(super) close_focus: FocusHandle,
    pub(super) confirmation_cancel_focus: FocusHandle,
    pub(super) device_focus: FocusHandle,
}

impl ProjectCreationState {
    pub(super) fn new(cx: &mut Context<ChatApp>) -> Self {
        Self {
            open: false,
            name: cx.new(|cx| {
                crate::components::prompt_input::PromptInput::project_name(
                    crate::theme::ThemeMode::Dark,
                    "项目名称",
                    cx,
                )
            }),
            roots: Vec::new(),
            editing: None,
            confirmation: None,
            creating: false,
            error: None,
            device_menu: false,
            marker_menu: false,
            marker: None,
            marker_search: cx.new(|cx| {
                crate::components::prompt_input::PromptInput::project_name(
                    crate::theme::ThemeMode::Dark,
                    crate::i18n::format!("搜索图标" => "Search icons"),
                    cx,
                )
            }),
            marker_emoji: false,
            marker_focused: None,
            marker_category: None,
            marker_scroll: gpui::UniformListScrollHandle::new(),
            cycle: 0,
            previous_focus: None,
            focus: cx.focus_handle().tab_stop(true),
            focus_pending: false,
            close_focus: cx.focus_handle().tab_stop(true),
            confirmation_cancel_focus: cx.focus_handle().tab_stop(true),
            device_focus: cx.focus_handle().tab_stop(true),
        }
    }
}
