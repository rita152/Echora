//! The sidebar activity view the bell button toggles (ChatGPT 26.917's
//! `sidebarElectron.priorityThreads`): a Priority section of chats that need
//! attention, the last seven days grouped by day, row actions, the options
//! menu, and the bell itself. The data comes from the workspace snapshot and
//! the model in `crate::workspace::activity`; nothing here invents rows.
//!
//! Every length below was read from the reference over CDP on a dedicated
//! debug instance (`artifacts/activity-view-26917/reference/`).

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, Bounds, ClickEvent, Context, Div, Hsla, IntoElement, MouseButton,
    Pixels, SharedString, Transformation, Window, canvas, div, prelude::*, px, radians,
};

use super::{
    AccountMenuRhythm, ICON_BUTTON_RADIUS, ROW_GAP, ROW_HORIZONTAL_PADDING, ROW_RADIUS,
    SECTION_GAP, SECTION_TITLE_OPACITY, SelectThread, SidebarView, account_menu_ring,
    account_menu_row, account_menu_separator, account_menu_surface,
    sticky::{STICKY_HEADING_HEIGHT, sticky_section},
    thread_title_canvas,
};
use crate::{
    agent::{AgentCapability, ThreadId, ThreadSummary},
    components::icons::icon,
    theme::Theme,
    workspace::{
        ActivityPreferences,
        activity::{
            ActivityInputs, ActivityLayout, ActivitySection, ActivitySectionKind, ActivitySession,
            Attention, RelativeDay,
        },
        project_id_for_thread,
    },
};

/// Section titles: 14/21 px at weight 500 in the tertiary color, inside the
/// heading's 8 px start and 2 px end padding.
const HEADING_TITLE_LINE: f32 = 21.0;
const HEADING_PADDING_LEFT: f32 = 8.0;
const HEADING_PADDING_RIGHT: f32 = 2.0;
/// Priority and Pinned wrap their title in `py-0.5`, so their title row is
/// 25 px and the 24 px options button centres on it.
const WRAPPED_TITLE_PADDING_Y: f32 = 2.0;
/// `p-2 text-base opacity-50` empty state under Priority.
const EMPTY_STATE_PADDING: f32 = 8.0;
const EMPTY_STATE_OPACITY: f32 = 0.5;
/// Rows: `pt-1.5 pb-2 pe-row-y ps-row-x`, a 20 px title line, 2 px, then the
/// 12/16 px source line (18 px tall with its inline-flex baseline).
const ROW_PADDING_TOP: f32 = 6.0;
const ROW_PADDING_BOTTOM: f32 = 8.0;
const ROW_PADDING_RIGHT: f32 = 5.0;
const ROW_TITLE_LINE: f32 = 20.0;
const ROW_LINE_GAP: f32 = 2.0;
const ROW_DETAIL_HEIGHT: f32 = 18.0;
const ROW_DETAIL_LINE: f32 = 16.0;
const ROW_DETAIL_ICON: f32 = 12.0;
const ROW_DETAIL_GAP: f32 = 4.0;
/// The title line gives the hover rail `pe-14` while the row is hovered.
const ROW_HOVER_RAIL_RESERVE: f32 = 56.0;
/// Trailing status and environment icons: 20 px boxes, 8 px apart, 4 px after
/// the title (`gap-1`).
const ROW_TRAILING_ICON_BOX: f32 = 20.0;
const ROW_TRAILING_ICON_GAP: f32 = 8.0;
const ROW_TITLE_TRAILING_GAP: f32 = 4.0;
const ROW_ENV_ICON: f32 = 14.0;
/// Pin and Archive: `3xs` pill buttons (20 px, 14 px glyph), 8 px apart, at
/// `pt-1.5 pe-row-y` from the row's top-right corner.
const ROW_ACTION_BUTTON: f32 = 20.0;
const ROW_ACTION_ICON: f32 = 14.0;
const ROW_ACTION_GAP: f32 = 8.0;
/// Unread: an `icon-xs` (16 px) dot scaled by half, in `bg-info-solid`.
const UNREAD_DOT: f32 = 8.0;
/// Running: the 16 px spinner in `text-text/70`, one turn every 2 s.
const ROW_SPINNER: f32 = 16.0;
const ROW_SPINNER_PERIOD: Duration = Duration::from_millis(2_000);
/// The loader: `py-3` around a 16 px `text-secondary` spinner, 1 s per turn.
const LOADER_HEIGHT: f32 = 40.0;
const LOADER_SPINNER: f32 = 16.0;
const LOADER_PERIOD: Duration = Duration::from_millis(1_000);
/// Options menu: 172 px wide, aligned to the trigger's start. Radix places it
/// at round(trigger bottom + 1) and the content keeps its own 1 px margin, so
/// it lands at 252 under the reference's 225.5..249.5 trigger: 2 px below the
/// trigger as GPUI draws it (shifted a pixel down, see `activity_heading`).
const OPTIONS_MENU_WIDTH: f32 = 172.0;
const OPTIONS_MENU_OFFSET_Y: f32 = 2.0;
const OPTIONS_MENU_OFFSET_X: f32 = 1.0;
const OPTIONS_MENU_CHECK: f32 = 16.0;
const OPTIONS_MENU_LABEL_HEIGHT: f32 = 26.5625;
const OPTIONS_MENU_ITEM_HEIGHT: f32 = 28.5625;
/// Tooltips: `px-3 py-1.25` pill, 13/18 px, 2 px above the trigger, opening
/// 200 ms after the pointer arrives (Radix `delayDuration`).
pub(super) const TOOLTIP_DELAY: Duration = Duration::from_millis(200);
const TOOLTIP_OFFSET: f32 = 2.0;
const TOOLTIP_PADDING_X: f32 = 12.0;
const TOOLTIP_PADDING_Y: f32 = 5.0;
const TOOLTIP_RADIUS: f32 = 20.0;
const TOOLTIP_LINE: f32 = 18.0;
const TOOLTIP_SHORTCUT_GAP: f32 = 8.0;
/// The shortcut chip's `-me-1.5`.
const TOOLTIP_SHORTCUT_OVERHANG: f32 = 6.0;
pub(super) const ACTIVITY_SHORTCUT_LABEL: &str = "⌥⌘U";

/// Controls that print a tooltip after hovering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ActivityTooltipTarget {
    Bell,
    ClearRead,
    RestoreDefaults,
}

/// A deterministic activity-view state for the screenshot path, applied once
/// the workspace has listed its chats.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActivityCaptureRequest {
    pub scroll: Option<f32>,
    /// A row to show hovered, by title or thread id.
    pub hover: Option<String>,
    pub options_open: bool,
    /// `bell`, `clear-read` or `restore-defaults`.
    pub tooltip: Option<String>,
}

/// The `Archive chats` confirmation, rendered by the application shell as a
/// centred dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityArchiveConfirmation {
    pub thread_ids: Vec<ThreadId>,
    /// Any listed chat still runs: the dialog says archiving stops it.
    pub running: bool,
    pub archiving: bool,
}

impl ActivityArchiveConfirmation {
    pub fn title(&self) -> String {
        let count = self.thread_ids.len();
        if self.running {
            if count == 1 {
                crate::i18n::format!("停止并归档 {count} 个聊天？" => "Stop and archive {count} chat?")
            } else {
                crate::i18n::format!("停止并归档 {count} 个聊天？" => "Stop and archive {count} chats?")
            }
        } else if count == 1 {
            crate::i18n::format!("要归档 {count} 个优先对话串吗？" => "Archive {count} priority thread?")
        } else {
            crate::i18n::format!("要归档 {count} 个优先对话串吗？" => "Archive {count} priority threads?")
        }
    }

    pub fn description(&self) -> String {
        if self.running {
            if self.thread_ids.len() == 1 {
                crate::i18n::text("归档会停止所有正在进行的工作。你可以稍后在设置中恢复该聊天。")
                    .to_owned()
            } else {
                crate::i18n::text("归档会停止所有正在进行的工作。你可以稍后在设置中恢复这些聊天。")
                    .to_owned()
            }
        } else {
            crate::i18n::text("最近的对话串不会被归档").to_owned()
        }
    }

    pub fn confirm_label(&self) -> &'static str {
        if self.archiving {
            crate::i18n::text("正在归档…")
        } else if self.running {
            crate::i18n::text("停止并归档")
        } else {
            crate::i18n::text("归档")
        }
    }
}

/// Weekday headings print the day's long name in the UI language.
fn weekday_label(start_ms: i64) -> &'static str {
    use chrono::{Datelike, Local, TimeZone, Weekday};
    let weekday = Local
        .timestamp_millis_opt(start_ms)
        .single()
        .map(|day| day.weekday())
        .unwrap_or(Weekday::Mon);
    crate::i18n::text(match weekday {
        Weekday::Mon => "星期一",
        Weekday::Tue => "星期二",
        Weekday::Wed => "星期三",
        Weekday::Thu => "星期四",
        Weekday::Fri => "星期五",
        Weekday::Sat => "星期六",
        Weekday::Sun => "星期日",
    })
}

/// The reference prints the worktree glyph after a chat that runs in a
/// Codex-managed worktree (`$CODEX_HOME/worktrees/...`).
fn runs_in_codex_worktree(thread: &ThreadSummary) -> bool {
    let home = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".codex"))
        });
    home.is_some_and(|home| thread.cwd.starts_with(home.join("worktrees")))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl SidebarView {
    pub(super) fn activity_inputs(&self) -> ActivityInputs {
        ActivityInputs::from_snapshot(&self.snapshot, self.viewed_thread.clone(), now_ms())
    }

    #[cfg(test)]
    pub fn activity_is_open(&self) -> bool {
        self.activity.is_some()
    }

    /// Opens the view for a capture and applies `request` as soon as the
    /// workspace has loaded, so the frame shows real chats, never a pending
    /// list.
    pub fn capture_activity(&mut self, request: ActivityCaptureRequest, cx: &mut Context<Self>) {
        self.pending_activity_capture = Some(request);
        self.activity_capture_applied = false;
        self.apply_pending_activity_capture(cx);
    }

    pub(super) fn apply_pending_activity_capture(&mut self, cx: &mut Context<Self>) {
        let loading = self.snapshot.loading;
        if loading.recent {
            self.activity_capture_saw_load = true;
        }
        if loading.projects || loading.recent || loading.pinned {
            return;
        }
        // The first snapshot can predate the first `thread/list`; only a
        // finished load (or listed chats) counts.
        if !self.activity_capture_saw_load && self.snapshot.recent_threads.is_empty() {
            return;
        }
        let Some(request) = self.pending_activity_capture.take() else {
            return;
        };
        if self.activity.is_none() {
            self.set_activity_open(true, cx);
        }
        // The list may first have to reveal more rows before it is that tall;
        // `activity_list` keeps asking for the offset until it holds.
        self.activity_capture_scroll.set(request.scroll);
        if let Some(hover) = &request.hover {
            self.hovered_thread_id = self
                .snapshot
                .recent_threads
                .iter()
                .chain(&self.snapshot.pinned_threads)
                .find(|thread| thread.title == *hover || thread.thread_id == *hover)
                .map(|thread| thread.thread_id.clone());
        }
        self.activity_menu_open = request.options_open;
        self.activity_tooltip = match request.tooltip.as_deref() {
            Some("bell") => Some(ActivityTooltipTarget::Bell),
            Some("clear-read") => Some(ActivityTooltipTarget::ClearRead),
            Some("restore-defaults") => Some(ActivityTooltipTarget::RestoreDefaults),
            _ => None,
        };
        self.activity_capture_applied = true;
        cx.notify();
    }

    /// True once a capture request has been applied to loaded data.
    #[cfg(feature = "screenshot")]
    pub fn activity_capture_ready(&self) -> bool {
        self.activity_capture_applied
            && self.activity.is_some()
            && self.activity_capture_scroll.get().is_none()
    }

    pub fn viewed_thread(&self) -> Option<&ThreadId> {
        self.viewed_thread.as_ref()
    }

    /// The bell and ⌥⌘U: open the view (taking its Priority snapshot) or close
    /// it again.
    pub fn toggle_activity(&mut self, cx: &mut Context<Self>) {
        let open = self.activity.is_none();
        self.set_activity_open(open, cx);
    }

    pub fn set_activity_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.close_transient_menus(cx);
        self.archived_open = false;
        self.activity_menu_open = false;
        self.activity_tooltip = None;
        if open == self.activity.is_some() {
            cx.notify();
            return;
        }
        self.activity = open.then(|| ActivitySession::activate(&self.activity_inputs(), now_ms()));
        self.activity_loader_requested.set(None);
        if !open {
            self.activity_archive = None;
        }
        cx.notify();
    }

    /// Folds a workspace update into the open view.
    pub(super) fn refresh_activity(&mut self) {
        let inputs = self.activity_inputs();
        if let Some(session) = self.activity.as_mut() {
            session.refresh(&inputs);
        }
    }

    /// The application reports which chat the main area shows; the view keeps
    /// a chat started from it in Priority, and the store reads it.
    pub fn set_viewed_thread(&mut self, thread_id: Option<ThreadId>, cx: &mut Context<Self>) {
        if self.viewed_thread == thread_id {
            return;
        }
        self.viewed_thread.clone_from(&thread_id);
        self.store.set_viewed_thread(thread_id);
        self.refresh_activity();
        cx.notify();
    }

    fn set_activity_preferences(
        &mut self,
        preferences: ActivityPreferences,
        cx: &mut Context<Self>,
    ) {
        self.snapshot.preferences.activity = preferences.clone();
        self.store.set_activity_preferences(preferences);
        self.refresh_activity();
        cx.notify();
    }

    fn clear_read_activity(&mut self, cx: &mut Context<Self>) {
        let inputs = self.activity_inputs();
        if let Some(session) = self.activity.as_mut() {
            session.clear_read(&inputs);
        }
        self.activity_tooltip = None;
        cx.notify();
    }

    fn mark_priority_read(&mut self, cx: &mut Context<Self>) {
        let inputs = self.activity_inputs();
        if let Some(session) = &self.activity {
            let thread_ids: Vec<ThreadId> = session
                .priority_threads(&inputs)
                .into_iter()
                .map(|(thread_id, _)| thread_id)
                .collect();
            self.store.mark_threads_read(&thread_ids);
        }
        self.activity_menu_open = false;
        cx.notify();
    }

    fn request_priority_archive(&mut self, cx: &mut Context<Self>) {
        let inputs = self.activity_inputs();
        let Some(session) = &self.activity else {
            return;
        };
        let priority = session.priority_threads(&inputs);
        if priority.is_empty() {
            return;
        }
        self.activity_archive = Some(ActivityArchiveConfirmation {
            running: priority
                .iter()
                .any(|(_, attention)| matches!(attention, Attention::Active | Attention::Waiting)),
            thread_ids: priority
                .into_iter()
                .map(|(thread_id, _)| thread_id)
                .collect(),
            archiving: false,
        });
        self.activity_menu_open = false;
        cx.notify();
    }

    pub fn activity_archive_confirmation(&self) -> Option<ActivityArchiveConfirmation> {
        self.activity_archive.clone()
    }

    /// Cancel, Escape and the scrim close the dialog unless it is archiving.
    pub fn dismiss_activity_archive(&mut self, cx: &mut Context<Self>) {
        if self
            .activity_archive
            .as_ref()
            .is_some_and(|confirmation| !confirmation.archiving)
        {
            self.activity_archive = None;
            cx.notify();
        }
    }

    pub fn confirm_activity_archive(&mut self, cx: &mut Context<Self>) {
        let Some(confirmation) = self.activity_archive.as_mut() else {
            return;
        };
        if confirmation.archiving {
            return;
        }
        confirmation.archiving = true;
        let receiver = self.store.archive_threads(confirmation.thread_ids.clone());
        cx.spawn(async move |this, cx| {
            let _ = receiver.recv().await;
            let _ = this.update(cx, |this, cx| {
                this.activity_archive = None;
                this.refresh_activity();
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn set_activity_tooltip_hovered(
        &mut self,
        target: ActivityTooltipTarget,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        if !hovered {
            if self.activity_tooltip_hovered == Some(target) {
                self.activity_tooltip_hovered = None;
            }
            if self.activity_tooltip == Some(target) {
                self.activity_tooltip = None;
                cx.notify();
            }
            return;
        }
        self.activity_tooltip_hovered = Some(target);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TOOLTIP_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                if this.activity_tooltip_hovered == Some(target) {
                    this.activity_tooltip = Some(target);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Records a tooltip trigger's window bounds so the tooltip can anchor to
    /// it rather than to the pointer.
    fn tooltip_anchor(&self, target: ActivityTooltipTarget) -> impl IntoElement {
        let anchors = self.activity_tooltip_anchors.clone();
        canvas(
            move |bounds, _, _| {
                anchors.borrow_mut().insert(target, bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    /// The header's bell. Off it is the secondary ghost button every sidebar
    /// icon button shares; on it is the `info` `soft` button, and while a chat
    /// needs attention its glyph carries the blue badge.
    pub(super) fn activity_bell(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let can_list_threads = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadList);
        let active = self.activity.is_some();
        let needs_attention = {
            let inputs = self.activity_inputs();
            match &self.activity {
                Some(session) => session.needs_attention(&inputs),
                None => inputs
                    .candidates
                    .iter()
                    .any(|candidate| candidate.attention.needs_attention()),
            }
        };
        let rest: Hsla = if active {
            theme.activity_info.into()
        } else {
            theme.sidebar_icon_muted.into()
        };
        let hover: Hsla = if active {
            theme.activity_info.into()
        } else {
            theme.sidebar_text.into()
        };
        let label = if active {
            crate::i18n::text("关闭活动视图")
        } else if needs_attention {
            crate::i18n::text("查看活动，需要关注")
        } else {
            crate::i18n::text("查看活动")
        };
        let glyph = if needs_attention && !active {
            div()
                .relative()
                .size(px(16.0))
                .child(
                    icon("activity-attention", rest)
                        .size(px(16.0))
                        .group_hover(super::ICON_BUTTON_GROUP, move |style| {
                            style.text_color(hover)
                        }),
                )
                .child(
                    icon("activity-attention-badge", theme.activity_badge.into())
                        .absolute()
                        .top_0()
                        .left_0()
                        .size(px(16.0)),
                )
                .into_any_element()
        } else {
            icon("activity", rest)
                .size(px(16.0))
                .group_hover(super::ICON_BUTTON_GROUP, move |style| {
                    style.text_color(hover)
                })
                .into_any_element()
        };
        div()
            .id("sidebar-activity")
            .relative()
            .size(px(24.0))
            .rounded(px(ICON_BUTTON_RADIUS))
            .flex()
            .items_center()
            .justify_center()
            .group(super::ICON_BUTTON_GROUP)
            .role(gpui::Role::Button)
            .aria_label(label)
            // The tooltip only shows while the pointer rests on the bell, so it
            // implies the hover plate too.
            .when(active, |button| {
                button.bg(
                    if self.activity_tooltip == Some(ActivityTooltipTarget::Bell) {
                        theme.activity_info_soft_hover
                    } else {
                        theme.activity_info_soft
                    },
                )
            })
            .when(
                !active && self.activity_tooltip == Some(ActivityTooltipTarget::Bell),
                |button| button.bg(theme.sidebar_hover),
            )
            .when(!can_list_threads, |button| {
                button.opacity(0.4).cursor_default()
            })
            .when(can_list_threads, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| {
                        style.bg(if active {
                            theme.activity_info_soft_hover
                        } else {
                            theme.sidebar_hover
                        })
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_activity(cx)))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.set_activity_tooltip_hovered(ActivityTooltipTarget::Bell, *hovered, cx)
                    }))
            })
            .child(glyph)
            .child(self.tooltip_anchor(ActivityTooltipTarget::Bell))
    }

    /// The scroll area's content while the view is open: the same navigation
    /// rows, then the activity list in place of pinned, projects and recents.
    pub(super) fn activity_list(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let inputs = self.activity_inputs();
        let Some(session) = &self.activity else {
            return div();
        };
        let layout = session.layout(&inputs);
        let priority = session.priority_threads(&inputs);
        if let Some(offset) = self.activity_capture_scroll.get() {
            // The handle stores whatever it is given; only a layout tall
            // enough for the offset actually scrolls that far. An offset the
            // list never reaches leaves the capture to time out.
            let reached = (f32::from(self.scroll.offset().y) + offset).abs() < 0.5
                && f32::from(self.scroll.max_offset().y) + 0.5 >= offset;
            if reached {
                self.activity_capture_scroll.set(None);
            } else {
                self.scroll.set_offset(gpui::point(px(0.0), px(-offset)));
            }
        }
        let scrolled = f32::from(self.scroll.offset().y) < -0.5;
        let mut list = div().flex().flex_col();
        let ActivityLayout { sections, has_more } = layout;
        let section_count = sections.len();
        if sections.is_empty() {
            // Nothing to show with Priority hidden: a lone `Today` heading
            // keeps the options menu reachable.
            list = list.child(self.activity_heading(
                "activity-heading-today".into(),
                crate::i18n::text("今天").into(),
                false,
                true,
                &priority,
                theme,
                cx,
            ));
        }
        for (index, section) in sections.into_iter().enumerate() {
            let ActivitySection { kind, threads } = section;
            let (id, title, wrapped): (SharedString, SharedString, bool) = match &kind {
                ActivitySectionKind::Priority => (
                    "activity-priority".into(),
                    crate::i18n::text("优先级").into(),
                    true,
                ),
                ActivitySectionKind::Pinned => (
                    "activity-pinned".into(),
                    crate::i18n::text("置顶").into(),
                    true,
                ),
                ActivitySectionKind::Day { start_ms, relative } => (
                    format!("activity-day-{start_ms}").into(),
                    match relative {
                        RelativeDay::Today => crate::i18n::text("今天"),
                        RelativeDay::Yesterday => crate::i18n::text("昨天"),
                        RelativeDay::Weekday => weekday_label(*start_ms),
                    }
                    .into(),
                    false,
                ),
            };
            let show_options = kind == ActivitySectionKind::Priority
                || (!inputs.preferences.show_priority && index == 0);
            let heading = self.activity_heading(
                format!("{id}-heading").into(),
                title,
                wrapped,
                show_options,
                if kind == ActivitySectionKind::Priority {
                    &priority
                } else {
                    &[]
                },
                theme,
                cx,
            );
            let mut rows = div().flex().flex_col().gap(px(ROW_GAP));
            if kind == ActivitySectionKind::Priority && threads.is_empty() {
                rows = rows.child(
                    div()
                        .id("activity-priority-empty")
                        .p(px(EMPTY_STATE_PADDING))
                        .text_size(px(14.0))
                        .line_height(px(HEADING_TITLE_LINE))
                        .text_color(theme.sidebar_text_muted)
                        .opacity(EMPTY_STATE_OPACITY)
                        .child(crate::i18n::text("暂无需要关注的任务")),
                );
            }
            for thread_id in threads {
                let attention = inputs
                    .candidates
                    .iter()
                    .find(|candidate| candidate.thread_id == thread_id)
                    .map_or(Attention::Idle, |candidate| candidate.attention);
                if let Some(thread) = self.snapshot.thread(&thread_id) {
                    rows = rows.child(self.activity_row(thread, attention, theme, window, cx));
                }
            }
            list = list.child(sticky_section(id, heading, rows, scrolled));
            if index + 1 < section_count {
                list = list.child(div().flex_none().h(px(SECTION_GAP)));
            }
        }
        if has_more {
            list = list.child(self.activity_loader(theme, cx));
        }
        div().px(px(ROW_HORIZONTAL_PADDING)).child(list)
    }

    #[allow(clippy::too_many_arguments)]
    fn activity_heading(
        &self,
        id: SharedString,
        title: SharedString,
        wrapped: bool,
        show_options: bool,
        priority: &[(ThreadId, Attention)],
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let show_clear_read = show_options
            && wrapped
            && self.snapshot.preferences.activity.show_priority
            && priority
                .iter()
                .any(|(_, attention)| *attention == Attention::Idle);
        let title_row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .pl(px(HEADING_PADDING_LEFT))
            .pr(px(HEADING_PADDING_RIGHT))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .when(wrapped, |title| title.py(px(WRAPPED_TITLE_PADDING_Y)))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(14.0))
                    .line_height(px(HEADING_TITLE_LINE))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.sidebar_text_muted)
                    .opacity(SECTION_TITLE_OPACITY)
                    .child(title),
            )
            .when(show_options, |row| {
                row.child(
                    div()
                        .flex_none()
                        // The 24 px buttons centre on the 25 px title row, so
                        // they start on a half pixel that Chromium rounds down
                        // the page: one pixel below GPUI's rounding.
                        .relative()
                        .top(px(super::HALF_PIXEL_LINE_BASELINE_SHIFT))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(self.activity_options_button(theme, cx))
                        .when(show_clear_read, |actions| {
                            actions.child(self.activity_clear_read_button(theme, cx))
                        }),
                )
            });
        div()
            .id(id)
            .flex_none()
            .h(px(STICKY_HEADING_HEIGHT))
            .child(title_row)
    }

    /// A `transparent` secondary icon button: no plate, the glyph brightens
    /// on hover and while its menu is open.
    fn activity_transparent_button(
        id: &'static str,
        glyph: &'static str,
        open: bool,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        let rest: Hsla = if open {
            theme.sidebar_text.into()
        } else {
            theme.sidebar_icon_muted.into()
        };
        let hover: Hsla = theme.sidebar_text.into();
        div()
            .id(id)
            .relative()
            .flex_none()
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .group(id)
            .child(
                icon(glyph, rest)
                    .size(px(16.0))
                    .group_hover(id, move |style| style.text_color(hover)),
            )
    }

    fn activity_options_button(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let open = self.activity_menu_open;
        let anchors = self.activity_options_anchor.clone();
        Self::activity_transparent_button("activity-options", "more-horizontal", open, theme)
            .role(gpui::Role::Button)
            .aria_label(crate::i18n::text("活动视图选项"))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                let open = !this.activity_menu_open;
                this.close_transient_menus(cx);
                this.activity_menu_open = open;
                cx.notify();
            }))
            .child(
                canvas(
                    move |bounds, _, _| anchors.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }

    fn activity_clear_read_button(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        Self::activity_transparent_button(
            "activity-clear-read",
            "activity-clear-read",
            false,
            theme,
        )
        .role(gpui::Role::Button)
        .aria_label(crate::i18n::text("清除已读聊天"))
        .on_click(cx.listener(|this, _, _, cx| this.clear_read_activity(cx)))
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            this.set_activity_tooltip_hovered(ActivityTooltipTarget::ClearRead, *hovered, cx)
        }))
        .child(self.tooltip_anchor(ActivityTooltipTarget::ClearRead))
    }

    /// One chat: its title (marquee on hover), the trailing status and
    /// worktree icons, and the source line under it; Pin and Archive replace
    /// the trailing icons while the row is hovered.
    fn activity_row(
        &self,
        thread: &ThreadSummary,
        attention: Attention,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let thread_id = thread.thread_id.clone();
        let hovered = self.hovered_thread_id.as_deref() == Some(thread_id.as_str());
        let selected = self.selected_thread_id.as_deref() == Some(thread_id.as_str());
        let pending = self.snapshot.is_pending_thread(&thread_id);
        let pinned = self
            .snapshot
            .pinned_threads
            .iter()
            .any(|candidate| candidate.thread_id == thread_id);
        let worktree = runs_in_codex_worktree(thread);
        let status = match attention {
            Attention::Unread => Some(Attention::Unread),
            Attention::Active => Some(Attention::Active),
            _ => None,
        };
        let trailing_icons = usize::from(worktree) + usize::from(status.is_some());
        let trailing_width = if trailing_icons == 0 {
            0.0
        } else {
            ROW_TITLE_TRAILING_GAP
                + trailing_icons as f32 * ROW_TRAILING_ICON_BOX
                + (trailing_icons - 1) as f32 * ROW_TRAILING_ICON_GAP
        };
        let content_width = self.activity_row_content_width();
        let viewport_width = if hovered {
            content_width - ROW_HOVER_RAIL_RESERVE
        } else {
            content_width - trailing_width
        }
        .max(0.0);
        let title_width = Self::thread_title_width(&thread.title, window);
        let scroll_distance = if hovered {
            (title_width - viewport_width).max(0.0)
        } else {
            0.0
        };
        let scroll_offset = self.marquee_started_at.map_or(0.0, |started_at| {
            super::marquee_offset(
                scroll_distance,
                cx.background_executor()
                    .now()
                    .saturating_duration_since(started_at),
                cx.reduce_motion(),
            )
        });

        let mut trailing = div()
            .flex_none()
            .h(px(ROW_TITLE_LINE))
            .flex()
            .items_center()
            .gap(px(ROW_TRAILING_ICON_GAP));
        if worktree {
            trailing = trailing.child(
                div()
                    .size(px(ROW_TRAILING_ICON_BOX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        icon("project-worktree", theme.sidebar_text_muted.into())
                            .size(px(ROW_ENV_ICON)),
                    ),
            );
        }
        match status {
            Some(Attention::Unread) => {
                trailing = trailing.child(
                    div()
                        .size(px(ROW_TRAILING_ICON_BOX))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .size(px(UNREAD_DOT))
                                .rounded_full()
                                .bg(theme.activity_badge),
                        ),
                );
            }
            Some(_) => {
                let spinner = icon("search-spinner", theme.activity_spinner.into())
                    .size(px(ROW_SPINNER))
                    .with_animation(
                        format!("activity-running-{thread_id}"),
                        Animation::new(ROW_SPINNER_PERIOD).repeat(),
                        |spinner, progress| {
                            spinner.with_transformation(Transformation::rotate(radians(
                                progress * std::f32::consts::TAU,
                            )))
                        },
                    );
                trailing = trailing.child(
                    div()
                        .size(px(ROW_TRAILING_ICON_BOX))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(spinner),
                );
            }
            None => {}
        }

        let title_line =
            div()
                .w_full()
                .h(px(ROW_TITLE_LINE))
                .flex()
                .items_center()
                .gap(px(ROW_TITLE_TRAILING_GAP))
                .when(hovered, |line| line.pr(px(ROW_HOVER_RAIL_RESERVE)))
                .child(div().min_w(px(0.0)).flex_1().h(px(ROW_TITLE_LINE)).child(
                    thread_title_canvas(
                        thread.title.clone().into(),
                        theme.sidebar_text.into(),
                        scroll_offset,
                        title_width > viewport_width,
                    ),
                ))
                .when(trailing_icons > 0 && !hovered, |line| line.child(trailing));

        let project =
            project_id_for_thread(thread, &self.snapshot.projects).and_then(|project_id| {
                self.snapshot
                    .projects
                    .iter()
                    .find(|project| project.project_id == project_id)
            });
        let (detail_glyph, detail_label): (&'static str, SharedString) = match project {
            Some(project) => ("utility-folder", project.name.clone().into()),
            None => ("project-local", "Codex".into()),
        };
        let detail = div()
            .w_full()
            .h(px(ROW_DETAIL_HEIGHT))
            .pr(px(ROW_PADDING_RIGHT))
            .flex()
            .items_start()
            .child(
                div()
                    .min_w(px(0.0))
                    .h(px(ROW_DETAIL_LINE))
                    .flex()
                    .items_center()
                    .gap(px(ROW_DETAIL_GAP))
                    .text_size(px(12.0))
                    .line_height(px(ROW_DETAIL_LINE))
                    .text_color(theme.sidebar_text_muted)
                    .child(
                        icon(detail_glyph, theme.sidebar_text_muted.into())
                            .flex_none()
                            .size(px(ROW_DETAIL_ICON)),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_overflow(gpui::TextOverflow::Truncate("…".into()))
                            .child(detail_label),
                    ),
            );

        let can_pin = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadSectionMove)
            && self
                .snapshot
                .capabilities
                .supports(AgentCapability::ThreadSectionList)
            && self
                .snapshot
                .capabilities
                .supports(AgentCapability::ThreadSectionCreate);
        let can_archive = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadArchive);
        let pin_id = thread_id.clone();
        let archive_id = thread_id.clone();
        let actions = div()
            .absolute()
            .top(px(ROW_PADDING_TOP))
            .right(px(ROW_PADDING_RIGHT))
            .flex()
            .gap(px(ROW_ACTION_GAP))
            .when(!hovered, |actions| actions.invisible())
            .child(
                Self::activity_row_action(
                    format!("activity-pin-{thread_id}"),
                    "activity-pin",
                    if pinned {
                        crate::i18n::text("取消置顶聊天")
                    } else {
                        crate::i18n::text("置顶聊天")
                    },
                    pending || !can_pin,
                    theme,
                )
                .when(!pending && can_pin, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.store.set_thread_pinned(pin_id.clone(), !pinned);
                    }))
                }),
            )
            .child(
                Self::activity_row_action(
                    format!("activity-archive-{thread_id}"),
                    "activity-archive",
                    crate::i18n::text("归档聊天"),
                    pending || !can_archive,
                    theme,
                )
                .when(!pending && can_archive, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.store.archive_thread(archive_id.clone());
                    }))
                }),
            );

        let hover_id = thread_id.clone();
        let select_id = thread_id.clone();
        let rename_id = thread_id.clone();
        let rename_title = thread.title.clone();
        let context_id = thread_id.clone();
        let bounds_id = thread_id.clone();
        let card_id = thread_id.clone();
        let marquee_title = thread.title.clone();
        let row_bounds = self.thread_row_bounds.clone();
        div()
            .id(format!("activity-row-{thread_id}"))
            .relative()
            .w_full()
            .pt(px(ROW_PADDING_TOP))
            .pb(px(ROW_PADDING_BOTTOM))
            .pl(px(ROW_HORIZONTAL_PADDING))
            .pr(px(ROW_PADDING_RIGHT))
            .rounded(px(ROW_RADIUS))
            .text_color(theme.sidebar_text)
            .overflow_hidden()
            .role(gpui::Role::Button)
            .aria_label(thread.title.clone())
            .when(pending, |row| row.opacity(0.4).cursor_default())
            .when(!pending, |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
            })
            .when(selected || hovered, |row| row.bg(theme.sidebar_hover))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(ROW_LINE_GAP))
                    .child(title_line)
                    .child(detail),
            )
            .child(actions)
            .child(
                canvas(
                    move |bounds, _window, _cx| {
                        row_bounds.borrow_mut().insert(bounds_id.clone(), bounds);
                    },
                    |_, _, _window, _cx| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered {
                    this.hovered_thread_id = Some(hover_id.clone());
                    let viewport = this.activity_row_content_width() - ROW_HOVER_RAIL_RESERVE;
                    this.start_marquee(&marquee_title, viewport.max(0.0), window, cx);
                } else if this.hovered_thread_id.as_deref() == Some(hover_id.as_str()) {
                    this.hovered_thread_id = None;
                    this.stop_marquee();
                }
                this.set_thread_row_hovered(card_id.clone(), *hovered, cx);
                cx.notify();
            }))
            .when(!pending, |row| {
                row.on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    this.select_activity_thread(select_id.clone(), cx);
                    if event.click_count() > 1 {
                        this.open_thread_rename(rename_id.clone(), rename_title.clone(), cx);
                    }
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.thread_menu_id = Some(context_id.clone());
                        this.project_menu_id = None;
                        this.delete_confirmation = None;
                        this.menu_origin =
                            (f32::from(event.position.x), f32::from(event.position.y));
                        cx.notify();
                    }),
                )
            })
    }

    /// Opening a chat from the view keeps the view open, as in the reference.
    fn select_activity_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.selected_thread_id = Some(thread_id.clone());
        self.project_menu_id = None;
        self.thread_menu_id = None;
        self.activity_menu_open = false;
        cx.emit(SelectThread { thread_id });
        cx.notify();
    }

    /// Width of a row's content box: the list is inset by `px-row-x` on each
    /// side (and the scrollbar gutter once it overflows), the row by its own
    /// start and end padding.
    fn activity_row_content_width(&self) -> f32 {
        (self.width
            - self.activity_gutter()
            - 2.0 * ROW_HORIZONTAL_PADDING
            - ROW_HORIZONTAL_PADDING
            - ROW_PADDING_RIGHT)
            .max(0.0)
    }

    fn activity_gutter(&self) -> f32 {
        if self.scrollbar_gutter.get() {
            super::SCROLLBAR_GUTTER
        } else {
            0.0
        }
    }

    fn activity_row_action(
        id: String,
        glyph: &'static str,
        label: &'static str,
        disabled: bool,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        let group = SharedString::from(format!("{id}-group"));
        let hover: Hsla = theme.sidebar_text.into();
        div()
            .id(SharedString::from(id))
            .flex_none()
            .size(px(ROW_ACTION_BUTTON))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .group(group.clone())
            .role(gpui::Role::Button)
            .aria_label(label)
            .when(disabled, |button| button.opacity(0.4).cursor_default())
            .when(!disabled, |button| button.cursor_pointer())
            .child(
                icon(glyph, theme.sidebar_icon_muted.into())
                    .size(px(ROW_ACTION_ICON))
                    .when(!disabled, |glyph| {
                        glyph.group_hover(group, move |style| style.text_color(hover))
                    }),
            )
    }

    /// Reveals the next page when the loader scrolls into view, like the
    /// reference's intersection observer: the observer disconnects once it
    /// fires and a new one watches the loader after the page grows, so each
    /// page is requested once.
    fn activity_loader(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let view = cx.entity().downgrade();
        let page = self
            .activity
            .as_ref()
            .map_or(0, ActivitySession::visible_count);
        let requested = self.activity_loader_requested.clone();
        let spinner = icon("search-spinner", theme.activity_loader.into())
            .size(px(LOADER_SPINNER))
            .with_animation(
                "activity-loader-spinner",
                Animation::new(LOADER_PERIOD).repeat(),
                |spinner, progress| {
                    spinner.with_transformation(Transformation::rotate(radians(
                        progress * std::f32::consts::TAU,
                    )))
                },
            );
        div()
            .id("activity-loader")
            .relative()
            .h(px(LOADER_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .child(spinner)
            .child(
                canvas(
                    move |bounds: Bounds<Pixels>, window, _| {
                        // The reference's observer has no root: it watches the
                        // window viewport, so the loader fires while it is
                        // still hidden behind the footer.
                        let viewport = Bounds::new(gpui::Point::default(), window.viewport_size());
                        if bounds.intersects(&viewport) && requested.get() != Some(page) {
                            requested.set(Some(page));
                            let view = view.clone();
                            window.on_next_frame(move |_, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    if let Some(session) = this.activity.as_mut()
                                        && session.visible_count() == page
                                    {
                                        session.load_more();
                                        cx.notify();
                                    }
                                });
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }

    /// The options menu under the Priority heading's `…` button.
    pub(super) fn activity_options_menu(
        &self,
        theme: Theme,
        scale_factor: f32,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !self.activity_menu_open {
            return None;
        }
        let anchor = self.activity_options_anchor.get()?;
        let inputs = self.activity_inputs();
        let session = self.activity.as_ref()?;
        let priority = session.priority_threads(&inputs);
        let preferences = self.snapshot.preferences.activity.clone();
        let can_mark_read = priority
            .iter()
            .any(|(_, attention)| *attention == Attention::Unread);
        let can_archive = !priority.is_empty()
            && self
                .snapshot
                .capabilities
                .supports(AgentCapability::ThreadArchive);
        // Rows keep the reference's fractional heights snapped to the device
        // grid, as in the account menu.
        let mut rhythm = AccountMenuRhythm::new(scale_factor);
        let label_height = rhythm.span(OPTIONS_MENU_LABEL_HEIGHT);
        let toggle_heights = [
            rhythm.span(OPTIONS_MENU_ITEM_HEIGHT),
            rhythm.span(OPTIONS_MENU_ITEM_HEIGHT),
            rhythm.span(OPTIONS_MENU_ITEM_HEIGHT),
        ];
        let separator_height = rhythm.span(9.0);
        let action_heights = [
            rhythm.span(OPTIONS_MENU_ITEM_HEIGHT),
            rhythm.span(OPTIONS_MENU_ITEM_HEIGHT),
        ];
        let toggle = |id: &'static str,
                      label: &'static str,
                      checked: bool,
                      height: f32,
                      apply: fn(&mut ActivityPreferences, bool)| {
            let preferences = preferences.clone();
            account_menu_row(id, label.into(), None, None, height, theme, true)
                .relative()
                .role(gpui::Role::MenuItemCheckBox)
                .aria_label(label)
                .when(checked, |row| {
                    row.child(
                        icon("check", theme.text.into())
                            .absolute()
                            .top(px((height - OPTIONS_MENU_CHECK) / 2.0))
                            .right(px(8.0))
                            .size(px(OPTIONS_MENU_CHECK)),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    let mut next = preferences.clone();
                    apply(&mut next, !checked);
                    this.set_activity_preferences(next, cx);
                }))
        };
        let restore = (!preferences.is_default()).then(|| {
            let hover: Hsla = theme.text.into();
            div()
                .id("activity-restore-defaults")
                .relative()
                .size(px(16.0))
                .p(px(2.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .group("activity-restore-defaults")
                .role(gpui::Role::Button)
                .aria_label(crate::i18n::text("恢复默认设置"))
                .child(
                    icon("activity-restore", theme.sidebar_text_muted.into())
                        .size(px(14.0))
                        .group_hover("activity-restore-defaults", move |style| {
                            style.text_color(hover)
                        }),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_activity_preferences(ActivityPreferences::default(), cx);
                }))
                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    this.set_activity_tooltip_hovered(
                        ActivityTooltipTarget::RestoreDefaults,
                        *hovered,
                        cx,
                    )
                }))
                .child(self.tooltip_anchor(ActivityTooltipTarget::RestoreDefaults))
        });
        let menu = div()
            .id("activity-options-menu")
            .role(gpui::Role::Menu)
            .aria_label(crate::i18n::text("活动视图选项"))
            .w(px(OPTIONS_MENU_WIDTH))
            .p(px(4.0))
            .rounded(px(20.0))
            .bg(account_menu_surface(theme))
            .shadow({
                let mut shadows = account_menu_ring(theme, scale_factor).to_vec();
                shadows.push(
                    gpui::BoxShadow::new(px(0.0), px(8.0), gpui::rgba(0x0000001f).into())
                        .blur_radius(px(16.0))
                        .spread_radius(px(-4.0)),
                );
                shadows
            })
            .font_family(".SystemUIFont")
            .text_size(px(13.0))
            .line_height(px(18.5714))
            .text_color(theme.text)
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h(px(label_height))
                    .px(px(8.0))
                    .pt(px(4.0))
                    .flex()
                    .items_start()
                    .justify_between()
                    .text_color(theme.sidebar_text_muted)
                    .child(crate::i18n::text("显示"))
                    .children(restore),
            )
            .child(toggle(
                "activity-show-priority",
                crate::i18n::text("优先事项部分"),
                preferences.show_priority,
                toggle_heights[0],
                |preferences, value| preferences.show_priority = value,
            ))
            .child(toggle(
                "activity-show-pinned",
                crate::i18n::text("置顶"),
                preferences.show_pinned,
                toggle_heights[1],
                |preferences, value| preferences.show_pinned = value,
            ))
            .child(toggle(
                "activity-show-scheduled",
                crate::i18n::text("定时任务"),
                preferences.show_scheduled,
                toggle_heights[2],
                |preferences, value| preferences.show_scheduled = value,
            ))
            .child(account_menu_separator(separator_height, theme))
            .child(
                account_menu_row(
                    "activity-mark-all-read",
                    crate::i18n::text("全部标为已读").into(),
                    None,
                    None,
                    action_heights[0],
                    theme,
                    can_mark_read,
                )
                .role(gpui::Role::MenuItem)
                .when(!can_mark_read, |row| row.opacity(0.5))
                .when(can_mark_read, |row| {
                    row.on_click(cx.listener(|this, _, _, cx| this.mark_priority_read(cx)))
                }),
            )
            .child(
                account_menu_row(
                    "activity-archive-priority",
                    crate::i18n::format!("归档聊天" => "Archive chats").into(),
                    None,
                    None,
                    action_heights[1],
                    theme,
                    can_archive,
                )
                .role(gpui::Role::MenuItem)
                .when(!can_archive, |row| row.opacity(0.5))
                .when(can_archive, |row| {
                    row.on_click(cx.listener(|this, _, _, cx| this.request_priority_archive(cx)))
                }),
            );
        let left = f32::from(anchor.origin.x) + OPTIONS_MENU_OFFSET_X;
        let top = f32::from(anchor.bottom()) + OPTIONS_MENU_OFFSET_Y;
        Some(div().absolute().left(px(left)).top(px(top)).child(menu))
    }

    /// The tooltip of whichever control has been hovered for 200 ms, centred
    /// above it.
    pub(super) fn activity_tooltip_overlay(
        &self,
        theme: Theme,
        window: &mut Window,
    ) -> Option<impl IntoElement> {
        let target = self.activity_tooltip?;
        let anchor = *self.activity_tooltip_anchors.borrow().get(&target)?;
        let (label, shortcut) = match target {
            ActivityTooltipTarget::Bell => (
                if self.activity.is_some() {
                    crate::i18n::text("关闭活动视图")
                } else {
                    crate::i18n::text("查看活动")
                },
                Some(ACTIVITY_SHORTCUT_LABEL),
            ),
            ActivityTooltipTarget::ClearRead => (crate::i18n::text("清除已读聊天"), None),
            ActivityTooltipTarget::RestoreDefaults => (crate::i18n::text("恢复默认设置"), None),
        };
        const SLOT: f32 = 480.0;
        let center = f32::from(anchor.origin.x) + f32::from(anchor.size.width) / 2.0;
        let bottom = f32::from(anchor.origin.y) - TOOLTIP_OFFSET;
        Some(
            div()
                .absolute()
                .top_0()
                .left(px(center - SLOT / 2.0))
                .w(px(SLOT))
                .h(px(bottom.max(0.0)))
                .flex()
                .flex_col()
                .justify_end()
                .items_center()
                .child(activity_tooltip(label, shortcut, theme, window)),
        )
    }
}

/// `--tracking-tooltip`: Chromium adds the spacing after every glyph.
const TOOLTIP_TRACKING: f32 = -0.15;

/// A single line printed glyph by glyph with letter spacing, which a shaped
/// run cannot express; the element takes the tracked width.
fn tracked_label(text: &'static str, color: Hsla, window: &mut Window) -> impl IntoElement {
    use unicode_segmentation::UnicodeSegmentation;
    let mut font = window.text_style().font();
    font.family = crate::theme::UI_FONT_FAMILY.into();
    font.weight = crate::theme::UI_BODY_FONT_WEIGHT;
    let shape = move |window: &mut Window, grapheme: &str| {
        let grapheme = SharedString::from(grapheme.to_owned());
        window.text_system().shape_line(
            grapheme.clone(),
            px(13.0),
            &[gpui::TextRun {
                len: grapheme.len(),
                font: font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
    };
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let width: f32 = graphemes
        .iter()
        .map(|grapheme| f32::from(shape(window, grapheme).width()) + TOOLTIP_TRACKING)
        .sum();
    canvas(
        move |_, window, _| {
            graphemes
                .iter()
                .map(|grapheme| shape(window, grapheme))
                .collect::<Vec<_>>()
        },
        move |bounds, lines, window, cx| {
            let mut x = bounds.origin.x;
            for line in lines {
                let _ = line.paint(
                    gpui::point(x, bounds.origin.y),
                    px(TOOLTIP_LINE),
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
                x += line.width() + px(TOOLTIP_TRACKING);
            }
        },
    )
    .flex_none()
    .w(px(width))
    .h(px(TOOLTIP_LINE))
}

/// The reference's compact tooltip pill: always the dark tooltip surface, in
/// both themes.
pub(super) fn activity_tooltip(
    label: &'static str,
    shortcut: Option<&'static str>,
    theme: Theme,
    window: &mut Window,
) -> Div {
    div()
        .flex_none()
        .px(px(TOOLTIP_PADDING_X))
        .py(px(TOOLTIP_PADDING_Y))
        .rounded(px(TOOLTIP_RADIUS))
        .border(px(1.0))
        .border_color(theme.tooltip_border)
        .bg(theme.tooltip_surface)
        .shadow(vec![
            gpui::BoxShadow::new(px(0.0), px(8.0), theme.tooltip_shadow.into())
                .blur_radius(px(18.0)),
        ])
        .font_family(".SystemUIFont")
        .text_size(px(13.0))
        .line_height(px(TOOLTIP_LINE))
        .text_color(theme.tooltip_text)
        .whitespace_nowrap()
        .flex()
        .items_center()
        .gap(px(TOOLTIP_SHORTCUT_GAP))
        .child(tracked_label(label, theme.tooltip_text.into(), window))
        .when_some(shortcut, |tooltip, shortcut| {
            tooltip.child(
                div()
                    .h(px(TOOLTIP_LINE))
                    .min_w(px(16.0))
                    .px(px(6.0))
                    .mr(px(-TOOLTIP_SHORTCUT_OVERHANG))
                    .rounded_full()
                    .bg(theme.tooltip_shortcut_surface)
                    .text_color(theme.tooltip_shortcut_text)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(shortcut),
            )
        })
}

#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;
