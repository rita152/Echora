//! The thread summary panel: the reference's pinned island to the right of a
//! chat (`thread-summary-panel`), 300 px wide, with the sections Echora has data
//! for: the checkout's environment (its branch and the attached pull requests
//! on it), the other attached pull requests, and background processes.
//!
//! Layout and tokens come from the reference's DOM (`artifacts/batch4-panel-*`,
//! `artifacts/batch4-background-*`): island `rounded-2xl` on
//! `bg-surface-elevated-secondary`, `py-2.5 gap-2`; section headers `h-7 ps-5
//! pe-3.5` in 13 px secondary text with a chevron on hover; rows `py-1` with an
//! 18 px leading icon, 13 px label, and a 24 px action button that appears on
//! hover or focus.

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gpui::{
    Animation, AnimationExt, AnyElement, Bounds, ClickEvent, Context, Div, Entity, EventEmitter,
    FocusHandle, KeyDownEvent, MouseButton, Pixels, Point, Role, SharedString, Stateful,
    Subscription, Transformation, Window, anchored, canvas, deferred, div, linear_color_stop,
    linear_gradient, point, prelude::*, px, radians,
};

use super::{
    composer::{ComposerView, ConversationChanged, ToastKind},
    icons::icon,
    pull_requests::{PrTheme, PullRequestsView},
};
use crate::{
    agent::{AgentPullRequestRef, ThreadId},
    conversation::{BackgroundCleanState, BackgroundTerminal},
    pull_requests::{PullRequestLiveState, PullRequestStatus, PullRequestSummary},
    theme::ThemeMode,
    workspace::{
        BranchLookup, GitCheckout, ThreadPullRequest, WorkspaceSnapshot, WorkspaceStore, chip_icon,
    },
};

pub const SUMMARY_PANEL_WIDTH: f32 = 300.0;
/// The layout gutter the pinned panel reserves beside the chat.
pub const SUMMARY_PANEL_GUTTER: f32 = 306.0;
const ISLAND_RADIUS: f32 = 16.0;
const ISLAND_PADDING_Y: f32 = 10.0;
const SECTION_GAP: f32 = 8.0;
const SECTION_BOTTOM: f32 = 8.0;
const HEADER_HEIGHT: f32 = 28.0;
const HEADER_PADDING_START: f32 = 20.0;
const HEADER_PADDING_END: f32 = 14.0;
const BODY_PADDING_X: f32 = 20.0;
const ROW_HEIGHT: f32 = 28.0;
const ROW_GAP: f32 = 2.0;
const ROW_ICON: f32 = 18.0;
const ROW_LABEL_START: f32 = 30.0;
const ROW_HOVER_OUTSET: f32 = 8.0;
const LABEL_FADE: f32 = 16.0;
const FOCUS_RING_INSET: f32 = 4.0;
const MENU_ITEM_HEIGHT: f32 = 28.57;
const ACTION_SIZE: f32 = 24.0;
const MENU_WIDTH: f32 = 220.0;
/// The "PR actions" menu opens this far left of the island, 3 px below the
/// top of its row (`artifacts/batch4-panel-*/pr-actions-menu`).
const MENU_GAP: f32 = 9.0;
const MENU_ROW_OFFSET: f32 = 3.0;
/// Rows a list shows before "Show N more".
const VISIBLE_ITEMS: usize = 6;
/// The reference's spring (0.3 s, bounce 0.01), as an ease-out.
pub const SUMMARY_PANEL_ANIMATION: Duration = Duration::from_millis(300);

/// Where the panel sits for the chat column's width: the reference's `Ee`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryDisplayMode {
    /// Too narrow to pin: the header button opens it as a popover.
    Overlay,
    /// Pinned, with the chat shifted left by 153 px.
    Shift,
    /// Pinned in the gutter beside the centred chat.
    Gutter,
}

impl SummaryDisplayMode {
    pub fn for_main_width(width: f32) -> Self {
        let room = (width - 736.0) / 2.0;
        if room < 180.0 {
            Self::Overlay
        } else if room < 400.0 {
            Self::Shift
        } else {
            Self::Gutter
        }
    }
}

/// Asks the app to show something the panel links to.
pub enum SummaryPanelEvent {
    /// A background terminal row: open its output tab in the right panel.
    OpenBackgroundTerminal {
        thread_id: ThreadId,
        item_id: String,
        title: String,
        output: String,
    },
    /// "View PR": the pull request on the Pull Requests page.
    ViewPullRequest(PullRequestSummary),
    /// The branch row: the review panel on this checkout.
    OpenReview,
}

impl EventEmitter<SummaryPanelEvent> for SummaryPanel {}

/// The open "PR actions" menu of one pull request row.
#[derive(Clone)]
struct PullRequestMenu {
    identity_key: String,
    url: String,
    /// The menu's top left in window coordinates, beside the island.
    anchor: Point<Pixels>,
}

pub struct SummaryPanel {
    mode: ThemeMode,
    store: Arc<WorkspaceStore>,
    snapshot: WorkspaceSnapshot,
    composer: Option<Entity<ComposerView>>,
    thread_id: Option<ThreadId>,
    cwd: Option<PathBuf>,
    hovered_row: Option<SharedString>,
    /// Each row's trigger, and the one with keyboard focus at the last render:
    /// focus reveals the row's action like hover does.
    row_focus: RefCell<HashMap<SharedString, FocusHandle>>,
    focused_row: Option<SharedString>,
    /// Whether the focused row shows its ring: keyboard focus only, like the
    /// browser's `:focus-visible` in the reference.
    focus_ring: bool,
    /// Each row's bounds at the last paint, where its menu opens.
    row_bounds: Rc<RefCell<HashMap<SharedString, Bounds<Pixels>>>>,
    /// `--batch4-state`: the hover, focus or menu to show once its row exists.
    #[cfg(feature = "screenshot")]
    capture_state: Option<String>,
    /// The menu's row at the previous frame: the island slides in, so the
    /// menu opens once its row stops moving.
    #[cfg(feature = "screenshot")]
    capture_row_seen: Option<Bounds<Pixels>>,
    #[cfg(feature = "screenshot")]
    capture_focus_ring: bool,
    hovered_header: Option<&'static str>,
    menu: Option<PullRequestMenu>,
    show_all: BTreeMap<&'static str, bool>,
    attaching: bool,
    focus: FocusHandle,
    _composer_changes: Option<Subscription>,
}

impl SummaryPanel {
    pub fn new(mode: ThemeMode, store: Arc<WorkspaceStore>, cx: &mut Context<Self>) -> Self {
        let snapshot = store.snapshot();
        let receiver = store.subscribe();
        cx.spawn(async move |this, cx| {
            while let Ok(snapshot) = receiver.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.snapshot = snapshot;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            mode,
            store,
            snapshot,
            composer: None,
            thread_id: None,
            cwd: None,
            hovered_row: None,
            row_focus: RefCell::default(),
            focused_row: None,
            focus_ring: false,
            row_bounds: Rc::default(),
            #[cfg(feature = "screenshot")]
            capture_state: None,
            #[cfg(feature = "screenshot")]
            capture_row_seen: None,
            #[cfg(feature = "screenshot")]
            capture_focus_ring: false,
            hovered_header: None,
            menu: None,
            show_all: BTreeMap::new(),
            attaching: false,
            focus: cx.focus_handle(),
            _composer_changes: None,
        }
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        cx.notify();
    }

    /// The chat the panel summarises. Its attachments are read (again) when
    /// it becomes the panel's chat, like the reference's refetch on mount.
    pub fn set_conversation(
        &mut self,
        composer: Option<Entity<ComposerView>>,
        thread_id: Option<ThreadId>,
        cwd: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let same = self.thread_id == thread_id
            && self.composer.as_ref().map(Entity::entity_id)
                == composer.as_ref().map(Entity::entity_id);
        if let Some(cwd) = &cwd {
            self.store.ensure_checkout(cwd);
        }
        if same {
            // The chat's directory is known once its history is read.
            if self.cwd != cwd {
                self.cwd = cwd;
                cx.notify();
            }
            return;
        }
        self._composer_changes = composer.as_ref().map(|composer| {
            cx.subscribe(composer, |_, _, _: &ConversationChanged, cx| cx.notify())
        });
        self.composer = composer;
        self.thread_id = thread_id.clone();
        self.cwd = cwd;
        self.menu = None;
        if let Some(thread_id) = thread_id {
            let store = self.store.clone();
            let thread = self.snapshot.thread(&thread_id).cloned();
            std::thread::spawn(move || {
                store.refresh_thread_attachments(&thread_id);
                if let Some(thread) = thread {
                    store.request_thread_pull_requests(std::slice::from_ref(&thread));
                }
            });
        }
        cx.notify();
    }

    /// The window regained focus: the current chat's attachments are re-read.
    pub fn refresh_on_focus(&self) {
        if let Some(thread_id) = self.thread_id.clone() {
            let store = self.store.clone();
            std::thread::spawn(move || store.refresh_thread_attachments(&thread_id));
        }
    }

    pub fn dismiss_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if self.menu.take().is_some() {
            cx.notify();
            return true;
        }
        false
    }

    fn checkout(&self) -> Option<GitCheckout> {
        WorkspaceStore::checkout_for(&self.snapshot, self.cwd.as_ref()?)
    }

    fn pull_requests(&self) -> Vec<ThreadPullRequest> {
        self.thread_id
            .as_deref()
            .and_then(|thread_id| self.snapshot.thread_attachments(thread_id))
            .map(|entry| entry.pull_requests_newest_first())
            .unwrap_or_default()
    }

    /// The reference's `G$i` + branch match: attached pull requests shown in
    /// the checkout's environment section (same root, on the checked-out
    /// branch, or url-only ones of the origin repository on that branch).
    fn environment_pull_requests(
        &self,
        checkout: &GitCheckout,
    ) -> (Vec<ThreadPullRequest>, Vec<ThreadPullRequest>) {
        let origin = checkout
            .origin_url
            .as_deref()
            .and_then(crate::workspace::remote_repository);
        self.pull_requests().into_iter().partition(|pull_request| {
            let same_root = match &pull_request.root {
                Some(root) => std::path::Path::new(root.trim_end_matches('/')) == checkout.root,
                None => AgentPullRequestRef::parse(&pull_request.url).is_some_and(|parsed| {
                    origin.as_deref()
                        == Some(format!(
                            "{}/{}",
                            parsed.owner.to_lowercase(),
                            parsed.repository.to_lowercase()
                        ))
                        .as_deref()
                }),
            };
            let branch = pull_request.head_branch.clone().or_else(|| {
                self.live(pull_request)
                    .map(|state| state.summary.head_branch.clone())
            });
            same_root && branch.is_some() && branch == checkout.branch
        })
    }

    fn live(&self, pull_request: &ThreadPullRequest) -> Option<&PullRequestLiveState> {
        self.snapshot
            .pull_request_live_state(&pull_request.identity_key)
    }

    fn background_terminals(&self, cx: &gpui::App) -> Vec<BackgroundTerminal> {
        self.composer
            .as_ref()
            .map(|composer| composer.read(cx).background_terminals())
            .unwrap_or_default()
    }

    fn clean_state(&self, cx: &gpui::App) -> BackgroundCleanState {
        self.composer
            .as_ref()
            .map(|composer| composer.read(cx).background_clean_state().clone())
            .unwrap_or_default()
    }

    /// "Existing pull request · Attach": the branch has this account's open or
    /// merged pull request that the chat has not attached yet.
    fn existing_branch_pull_request(&self, checkout: &GitCheckout) -> Option<String> {
        let branch = checkout.branch.clone()?;
        let entry = self
            .snapshot
            .pull_requests
            .branch_lookups
            .get(&(checkout.root.clone(), branch))?;
        let BranchLookup::Ready(Some(found)) = &entry.lookup else {
            return None;
        };
        let found_key = AgentPullRequestRef::parse(&found.url)?.identity_key();
        let attached = self
            .pull_requests()
            .iter()
            .any(|pull_request| pull_request.identity_key == found_key);
        (!attached).then(|| found.url.clone())
    }

    /// Whether the panel has anything to show for this chat.
    pub fn has_content(&self, cx: &gpui::App) -> bool {
        self.thread_id.is_some()
            && (self.checkout().is_some()
                || !self.pull_requests().is_empty()
                || !self.background_terminals(cx).is_empty())
    }

    fn expanded(&self, key: &str) -> bool {
        self.snapshot
            .preferences
            .summary_section_expanded
            .get(key)
            .copied()
            .unwrap_or(true)
    }

    fn toggle_section(&mut self, key: &'static str, cx: &mut Context<Self>) {
        let expanded = !self.expanded(key);
        let store = self.store.clone();
        std::thread::spawn(move || store.set_summary_section_expanded(key, expanded));
        // Show the change right away; the store confirms it.
        self.snapshot
            .preferences
            .summary_section_expanded
            .insert(key.to_owned(), expanded);
        cx.notify();
    }

    fn clean(&mut self, item_id: String, cx: &mut Context<Self>) {
        if let Some(composer) = &self.composer {
            composer.update(cx, |composer, cx| {
                composer.clean_background_terminals(Some(item_id), true, cx)
            });
        }
    }

    fn open_terminal(&mut self, terminal: &BackgroundTerminal, cx: &mut Context<Self>) {
        let (Some(thread_id), Some(composer)) = (self.thread_id.clone(), self.composer.as_ref())
        else {
            return;
        };
        let output = composer
            .read(cx)
            .background_command(&terminal.item_id)
            .map(|command| command.output)
            .unwrap_or_default();
        cx.emit(SummaryPanelEvent::OpenBackgroundTerminal {
            thread_id,
            item_id: terminal.item_id.clone(),
            title: terminal_title(terminal),
            output,
        });
    }

    fn toast(
        &self,
        kind: ToastKind,
        text: String,
        action: Option<(String, super::composer::ToastAction)>,
        cx: &mut Context<Self>,
    ) {
        if let Some(composer) = &self.composer {
            composer.update(cx, |composer, cx| {
                composer.show_panel_toast(kind, text, action, cx)
            });
        }
    }

    fn remove_pull_request(&mut self, url: String, cx: &mut Context<Self>) {
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        self.menu = None;
        let receiver = self.store.detach_pull_request(thread_id, Some(url));
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| Err(String::new()));
            let _ = this.update(cx, |this, cx| {
                if result.is_err() {
                    this.toast(
                        ToastKind::Danger,
                        crate::i18n::format!("无法移除任务附件" => "Could not remove task attachment"),
                        None,
                        cx,
                    );
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn attach_existing(&mut self, url: String, checkout: GitCheckout, cx: &mut Context<Self>) {
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        if self.attaching {
            return;
        }
        self.attaching = true;
        let receiver = self.store.attach_pull_request(
            thread_id.clone(),
            url.clone(),
            Some(checkout.root.to_string_lossy().into_owned()),
            checkout.branch.clone(),
        );
        let store = self.store.clone();
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| Err(String::new()));
            let _ = this.update(cx, |this, cx| {
                this.attaching = false;
                match result {
                    Ok(()) => {
                        let undo_store = store.clone();
                        let undo: super::composer::ToastAction = std::rc::Rc::new(move |_, _| {
                            let _ = undo_store.detach_pull_request(thread_id.clone(), Some(url.clone()));
                        });
                        this.toast(
                            ToastKind::Success,
                            crate::i18n::format!("已将拉取请求关联到此任务" => "Pull request associated with this task"),
                            Some((crate::i18n::format!("撤销" => "Undo"), undo)),
                            cx,
                        );
                    }
                    Err(_) => this.toast(
                        ToastKind::Danger,
                        crate::i18n::format!("无法更新任务附件" => "Could not update task attachment"),
                        None,
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

/// Where the "PR actions" menu of a row opens: left of the island, level
/// with the row; without the row's bounds, left of the pointer.
fn menu_anchor(row: Option<Bounds<Pixels>>, pointer: Point<Pixels>) -> Point<Pixels> {
    match row {
        Some(row) => point(
            row.left() - px(BODY_PADDING_X + MENU_GAP + MENU_WIDTH),
            row.top() + px(MENU_ROW_OFFSET),
        ),
        None => point(pointer.x - px(MENU_WIDTH + 16.0), pointer.y - px(16.0)),
    }
}

/// The reference's focus ring colour (the browser default outline).
fn focus_ring_color(mode: ThemeMode) -> gpui::Rgba {
    match mode {
        ThemeMode::Light => gpui::rgb(0x539af8),
        _ => gpui::rgb(0x799ec9),
    }
}

fn pull_request_row_id(pull_request: &ThreadPullRequest) -> SharedString {
    format!("summary-pr-{}", pull_request.identity_key).into()
}

fn terminal_title(terminal: &BackgroundTerminal) -> String {
    if terminal.command.is_empty() {
        crate::i18n::format!("后台终端" => "Background terminal")
    } else {
        terminal.command.clone()
    }
}

fn pull_request_label(
    pull_request: &ThreadPullRequest,
    live: Option<&PullRequestLiveState>,
) -> String {
    live.map(|state| state.summary.title.trim().to_owned())
        .filter(|title| !title.is_empty())
        .or_else(|| {
            AgentPullRequestRef::parse(&pull_request.url)
                .map(|parsed| format!("PR #{}", parsed.number))
        })
        .unwrap_or_else(|| pull_request.url.clone())
}

fn status_text(state: &PullRequestLiveState) -> String {
    match state.summary.status {
        PullRequestStatus::Draft => crate::i18n::format!("草稿" => "Draft"),
        PullRequestStatus::Open => crate::i18n::format!("待审查" => "Ready for review"),
        PullRequestStatus::Merged => {
            let age = state
                .merged_at
                .as_deref()
                .map(crate::pull_requests::age_since)
                .unwrap_or_default();
            crate::i18n::format!("{age}前已合并" => "Merged {age} ago")
        }
        PullRequestStatus::Closed => {
            let age = state
                .closed_at
                .as_deref()
                .map(crate::pull_requests::age_since)
                .unwrap_or_default();
            crate::i18n::format!("{age}前已关闭" => "Closed {age} ago")
        }
    }
}

impl SummaryPanel {
    fn header(
        &self,
        key: &'static str,
        title: String,
        count: Option<usize>,
        theme: PrTheme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let expanded = self.expanded(key);
        let hovered = self.hovered_header == Some(key);
        div()
            .id(SharedString::from(format!("summary-section-{key}")))
            .h(px(HEADER_HEIGHT))
            .w_full()
            .pl(px(HEADER_PADDING_START))
            .pr(px(HEADER_PADDING_END))
            .pb(px(2.0))
            .flex()
            .items_center()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.hovered_header = hovered.then_some(key);
                cx.notify();
            }))
            .child(
                div()
                    .id(SharedString::from(format!("summary-section-toggle-{key}")))
                    .debug_selector(move || format!("summary-section-toggle-{key}"))
                    .role(Role::Button)
                    .aria_expanded(expanded)
                    .aria_label(title.clone())
                    .ml(px(-3.5))
                    .pl(px(3.5))
                    .pr(px(4.0))
                    .py(px(2.0))
                    .rounded(px(15.0))
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .text_color(theme.icon_muted)
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_section(key, cx)))
                    .child(div().min_w(px(0.0)).truncate().child(title))
                    .child(
                        icon("chevron-down", theme.icon_muted.into())
                            .size(px(14.0))
                            .flex_none()
                            .when(!expanded, |chevron| {
                                chevron.with_transformation(Transformation::rotate(radians(
                                    -std::f32::consts::FRAC_PI_2,
                                )))
                            })
                            // Hidden while expanded until the header is hovered.
                            .when(expanded && !hovered, |chevron| chevron.opacity(0.0)),
                    )
                    .when_some(count.filter(|_| !expanded), |toggle, count| {
                        toggle.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .text_color(theme.text_muted)
                                .child("·")
                                .child(count.to_string()),
                        )
                    }),
            )
    }

    /// One row: the whole row is the trigger; an action button appears on
    /// hover or keyboard focus.
    #[allow(clippy::too_many_arguments)]
    fn row(
        &self,
        id: SharedString,
        glyph: AnyElement,
        label: String,
        action: Option<AnyElement>,
        theme: PrTheme,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let hovered = self.hovered_row.as_ref() == Some(&id);
        let focused = self.focused_row.as_ref() == Some(&id);
        let revealed = hovered || focused;
        let ring = focused && self.focus_ring;
        let surface = gpui::Hsla::from(theme.popover_surface);
        let fade = if hovered {
            surface.blend(theme.row_hover.into())
        } else {
            surface
        };
        let focus = self
            .row_focus
            .borrow_mut()
            .entry(id.clone())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        let hover_id = id.clone();
        div()
            .id(id.clone())
            .relative()
            .w_full()
            .h(px(ROW_HEIGHT))
            .flex()
            .items_center()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.hovered_row = Some(hover_id.clone());
                } else if this.hovered_row.as_ref() == Some(&hover_id) {
                    this.hovered_row = None;
                }
                cx.notify();
            }))
            .child({
                let bounds = self.row_bounds.clone();
                let id = id.clone();
                canvas(
                    move |row, _, _| {
                        bounds.borrow_mut().insert(id, row);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
            })
            .child(
                // The hover fill reaches 8 px past the body on both sides.
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(-ROW_HOVER_OUTSET))
                    .right(px(-ROW_HOVER_OUTSET))
                    .rounded(px(4.0))
                    .when(hovered, |fill| fill.bg(theme.row_hover)),
            )
            .child(
                div()
                    .id(SharedString::from(format!("{id}-trigger")))
                    .debug_selector({
                        let id = id.clone();
                        move || format!("{id}-trigger")
                    })
                    .role(Role::Button)
                    .aria_label(label.clone())
                    .track_focus(&focus)
                    .relative()
                    .min_w(px(0.0))
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .on_click(cx.listener(on_click))
                    // The reference's focus ring: 2 px inside the 20 px tall
                    // button, square.
                    .when(ring, |trigger| {
                        trigger.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .top(px(FOCUS_RING_INSET))
                                .bottom(px(FOCUS_RING_INSET))
                                .border_2()
                                .border_color(focus_ring_color(self.mode)),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .w(px(ROW_ICON))
                            .h(px(ROW_ICON))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(glyph),
                    )
                    .child(
                        div()
                            .relative()
                            .ml(px(ROW_LABEL_START - ROW_ICON))
                            .min_w(px(0.0))
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .text_color(theme.text)
                            .child(label)
                            // The reference fades a long label out over its
                            // last 16 px instead of an ellipsis.
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .right_0()
                                    .w(px(LABEL_FADE))
                                    .bg(linear_gradient(
                                        90.0,
                                        linear_color_stop(fade.opacity(0.0), 0.0),
                                        linear_color_stop(fade, 1.0),
                                    )),
                            ),
                    ),
            )
            .when_some(action, |row, action| {
                row.child(
                    div()
                        .flex_none()
                        .mr(px(-6.0))
                        .ml(px(4.0))
                        .when(!revealed, |slot| slot.invisible())
                        .child(action),
                )
            })
    }

    fn action_button(
        id: SharedString,
        glyph: &'static str,
        label: String,
        disabled: bool,
        theme: PrTheme,
    ) -> Stateful<Div> {
        let selector = id.to_string();
        div()
            .id(id)
            .debug_selector(move || selector)
            .role(Role::Button)
            .aria_label(label)
            .size(px(ACTION_SIZE))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .when(!disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.row_hover))
            })
            .when(disabled, |button| button.opacity(0.5))
            .child(icon(glyph, theme.text_muted.into()).size(px(18.0)))
    }

    fn pull_request_row(
        &self,
        pull_request: &ThreadPullRequest,
        theme: PrTheme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let live = self.live(pull_request).cloned();
        let label = pull_request_label(pull_request, live.as_ref());
        let glyph = match &live {
            Some(state) => {
                PullRequestsView::status_glyph(chip_icon(state), theme, 16.0).into_any_element()
            }
            None => icon("pr-status-open", theme.text_muted.into())
                .size(px(16.0))
                .into_any_element(),
        };
        let id = pull_request_row_id(pull_request);
        let identity_key = pull_request.identity_key.clone();
        let url = pull_request.url.clone();
        let menu_key = identity_key.clone();
        let menu_url = url.clone();
        let menu_row = id.clone();
        let action = Self::action_button(
            format!("{id}-actions").into(),
            "more-horizontal",
            crate::i18n::format!("{label} 的操作" => "Actions for {label}", label = label.clone()),
            false,
            theme,
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
            cx.stop_propagation();
            let row = this.row_bounds.borrow().get(&menu_row).copied();
            this.menu = Some(PullRequestMenu {
                identity_key: menu_key.clone(),
                url: menu_url.clone(),
                anchor: menu_anchor(row, event.position()),
            });
            cx.notify();
        }))
        .into_any_element();
        let open_live = live.clone();
        self.row(
            id,
            glyph,
            label,
            Some(action),
            theme,
            move |_, _, _, cx| {
                // The reference opens its pull request side panel; Echora's Pull
                // Requests page shows the same detail, or GitHub when unknown.
                match &open_live {
                    Some(state) => {
                        cx.emit(SummaryPanelEvent::ViewPullRequest(state.summary.clone()))
                    }
                    None => cx.open_url(&url),
                }
            },
            cx,
        )
    }

    fn menu_item(
        id: &'static str,
        glyph: &'static str,
        label: String,
        trailing: Option<AnyElement>,
        danger: bool,
        theme: PrTheme,
    ) -> Stateful<Div> {
        let color = if danger { theme.chart_red } else { theme.text };
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .role(Role::MenuItem)
            .aria_label(label.clone())
            .h(px(MENU_ITEM_HEIGHT))
            .px(px(8.0))
            .mx(px(4.0))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .hover(move |style| style.bg(theme.menu_hover))
            .text_size(px(13.0))
            .line_height(px(18.5714))
            .text_color(color)
            .when(!danger, |item| {
                item.child(icon(glyph, theme.text.into()).size(px(16.0)).flex_none())
            })
            .child(div().flex_1().min_w(px(0.0)).truncate().child(label))
            .when_some(trailing, |item, trailing| item.child(trailing))
    }

    fn pull_request_menu(
        &self,
        menu: &PullRequestMenu,
        theme: PrTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let live = self
            .snapshot
            .pull_request_live_state(&menu.identity_key)
            .cloned();
        let url = menu.url.clone();
        let copy_url = url.clone();
        let github_url = url.clone();
        let remove_url = url.clone();
        let view_live = live.clone();
        let view_url = url.clone();
        let mut items: Vec<AnyElement> = vec![
            Self::menu_item(
                "summary-pr-view",
                "pr-open-file",
                crate::i18n::format!("查看 PR" => "View PR"),
                Some(
                    div()
                        .flex()
                        .gap(px(2.0))
                        .child(
                            Self::action_button(
                                "summary-pr-copy-link".into(),
                                "pr-permalink",
                                crate::i18n::format!("复制链接" => "Copy link"),
                                false,
                                theme,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        copy_url.clone(),
                                    ));
                                    this.menu = None;
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            Self::action_button(
                                "summary-pr-open-github".into(),
                                "markdown-github",
                                crate::i18n::format!("在 GitHub 中打开" => "Open in GitHub"),
                                false,
                                theme,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    cx.open_url(&github_url);
                                    this.menu = None;
                                    cx.notify();
                                },
                            )),
                        )
                        .into_any_element(),
                ),
                false,
                theme,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.menu = None;
                match &view_live {
                    Some(state) => {
                        cx.emit(SummaryPanelEvent::ViewPullRequest(state.summary.clone()))
                    }
                    None => cx.open_url(&view_url),
                }
                cx.notify();
            }))
            .into_any_element(),
        ];
        if let Some(state) = &live {
            items.push(
                Self::menu_item(
                    "summary-pr-code-changes",
                    "panel-review",
                    crate::i18n::format!("代码更改" => "Code changes"),
                    Some(
                        PullRequestsView::diff_stats(
                            state.summary.additions,
                            state.summary.deletions,
                            theme.chart_green,
                            theme.chart_red,
                            14.0,
                        )
                        .into_any_element(),
                    ),
                    false,
                    theme,
                )
                .into_any_element(),
            );
            items.push(
                Self::menu_item(
                    "summary-pr-status",
                    "pr-menu-status",
                    crate::i18n::format!("状态" => "Status"),
                    Some(
                        div()
                            .text_color(theme.text_muted)
                            .child(status_text(state))
                            .into_any_element(),
                    ),
                    false,
                    theme,
                )
                .into_any_element(),
            );
        }
        items.push(
            div()
                .mx(px(12.0))
                .my(px(4.0))
                .h(px(0.5))
                .bg(theme.border)
                .into_any_element(),
        );
        items.push(
            Self::menu_item(
                "summary-pr-remove",
                "",
                crate::i18n::format!("从任务中移除 PR" => "Remove PR from task"),
                None,
                true,
                theme,
            )
            .on_click(
                cx.listener(move |this, _, _, cx| this.remove_pull_request(remove_url.clone(), cx)),
            )
            .into_any_element(),
        );
        deferred(
            anchored().position(menu.anchor).snap_to_window().child(
                div()
                    .id("summary-pr-menu")
                    .role(Role::Menu)
                    .occlude()
                    .w(px(MENU_WIDTH))
                    .py(px(4.0))
                    .rounded(px(16.0))
                    .bg(theme.popover_surface)
                    .shadow(vec![
                        gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                            .spread_radius(px(0.5)),
                        gpui::BoxShadow::new(px(0.0), px(8.0), theme.menu_shadow.into())
                            .blur_radius(px(24.0))
                            .spread_radius(px(-4.0)),
                    ])
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.menu = None;
                        cx.notify();
                    }))
                    .children(items),
            ),
        )
        .with_priority(2)
        .into_any_element()
    }

    fn environment_section(
        &self,
        checkout: &GitCheckout,
        pull_requests: &[ThreadPullRequest],
        theme: PrTheme,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows: Vec<AnyElement> = Vec::new();
        let branch = checkout
            .branch
            .clone()
            .unwrap_or_else(|| crate::i18n::format!("分离的 HEAD" => "Detached HEAD"));
        rows.push(
            self.row(
                "summary-environment-branch".into(),
                icon("panel-review", theme.text.into())
                    .size(px(ROW_ICON))
                    .into_any_element(),
                branch,
                None,
                theme,
                |_, _, _, cx| cx.emit(SummaryPanelEvent::OpenReview),
                cx,
            )
            .into_any_element(),
        );
        for pull_request in pull_requests {
            rows.push(
                self.pull_request_row(pull_request, theme, cx)
                    .into_any_element(),
            );
        }
        if let Some(url) = self.existing_branch_pull_request(checkout) {
            let checkout = checkout.clone();
            let attaching = self.attaching;
            let row = self
                .row(
                    "summary-environment-existing-pr".into(),
                    icon("pr-status-open", theme.text_muted.into())
                        .size(px(16.0))
                        .into_any_element(),
                    crate::i18n::format!("已有拉取请求" => "Existing pull request"),
                    None,
                    theme,
                    move |this, _, _, cx| {
                        if !attaching {
                            this.attach_existing(url.clone(), checkout.clone(), cx);
                        }
                    },
                    cx,
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(13.0))
                        .text_color(theme.text_muted)
                        .child(crate::i18n::format!("附加" => "Attach")),
                );
            rows.push(row.into_any_element());
        }
        rows
    }

    fn background_section(
        &self,
        terminals: &[BackgroundTerminal],
        theme: PrTheme,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let clean = self.clean_state(cx);
        let pending = match &clean {
            BackgroundCleanState::InFlight { clicked_item_id } => Some(clicked_item_id.clone()),
            _ => None,
        };
        let show_all = self
            .show_all
            .get("background-tasks")
            .copied()
            .unwrap_or(false);
        let visible = if show_all {
            terminals.len()
        } else {
            terminals.len().min(VISIBLE_ITEMS)
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        for terminal in &terminals[..visible] {
            let spinning = pending
                .as_ref()
                .is_some_and(|clicked| clicked.as_deref() == Some(terminal.item_id.as_str()));
            let stop_item = terminal.item_id.clone();
            let stop: AnyElement = if spinning {
                div()
                    .size(px(ACTION_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        icon("pr-spinner", theme.text_muted.into())
                            .size(px(16.0))
                            .with_animation(
                                SharedString::from(format!(
                                    "summary-stop-spin-{}",
                                    terminal.item_id
                                )),
                                Animation::new(Duration::from_millis(800)).repeat(),
                                |spinner, progress| {
                                    spinner.with_transformation(Transformation::rotate(radians(
                                        progress * std::f32::consts::TAU,
                                    )))
                                },
                            ),
                    )
                    .into_any_element()
            } else {
                Self::action_button(
                    format!("summary-background-stop-{}", terminal.item_id).into(),
                    "composer-stop",
                    crate::i18n::format!("停止所有后台终端" => "Stop all background terminals"),
                    pending.is_some(),
                    theme,
                )
                .when(pending.is_none(), |button| {
                    button
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.clean(stop_item.clone(), cx);
                        }))
                })
                .into_any_element()
            };
            let open = terminal.clone();
            let row_id: SharedString = format!("summary-background-{}", terminal.item_id).into();
            let focused_stop = pending.is_none().then(|| terminal.item_id.clone());
            rows.push(
                self.row(
                    row_id.clone(),
                    icon("panel-terminal", theme.text.into())
                        .size(px(ROW_ICON))
                        .into_any_element(),
                    terminal_title(terminal),
                    Some(stop),
                    theme,
                    move |this, _, _, cx| this.open_terminal(&open, cx),
                    cx,
                )
                // Keyboard: the focused row reveals its stop button; Delete
                // or Backspace stops, like pressing it.
                .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                    if let Some(item) = &focused_stop
                        && matches!(event.keystroke.key.as_str(), "delete" | "backspace")
                    {
                        this.clean(item.clone(), cx);
                        cx.stop_propagation();
                    }
                }))
                .into_any_element(),
            );
        }
        if terminals.len() > VISIBLE_ITEMS {
            let hidden = terminals.len() - VISIBLE_ITEMS;
            let label = if show_all {
                crate::i18n::format!("收起" => "Show less")
            } else {
                crate::i18n::format!("再显示 {hidden} 个" => "Show {hidden} more")
            };
            rows.push(
                div()
                    .id("summary-background-show-more")
                    .role(Role::Button)
                    .aria_label(label.clone())
                    .h(px(ROW_HEIGHT))
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .text_color(theme.text_muted)
                    .cursor_pointer()
                    .hover(move |style| style.text_color(theme.icon_muted))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.show_all.insert("background-tasks", !show_all);
                        cx.notify();
                    }))
                    .child(label)
                    .into_any_element(),
            );
        }
        rows
    }
}

#[cfg(test)]
mod tests;

impl gpui::Focusable for SummaryPanel {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SummaryPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = PrTheme::for_mode(self.mode);
        let previous_focus = self.focused_row.take();
        self.focused_row = self
            .row_focus
            .borrow()
            .iter()
            .find(|(_, focus)| focus.is_focused(window))
            .map(|(id, _)| id.clone());
        // A row focused from the keyboard shows the ring until focus moves
        // on; one focused by a click does not.
        let keyboard = window.last_input_was_keyboard();
        if self.focused_row != previous_focus {
            self.focus_ring = keyboard;
        } else if keyboard {
            self.focus_ring = true;
        }
        #[cfg(feature = "screenshot")]
        {
            self.focus_ring |= self.capture_focus_ring && self.focused_row.is_some();
        }
        #[cfg(feature = "screenshot")]
        self.apply_capture_state(window, cx);
        let checkout = self.checkout();
        let terminals = self.background_terminals(cx);
        let mut sections: Vec<(&'static str, String, Option<usize>, Vec<AnyElement>)> = Vec::new();
        let unmatched = match &checkout {
            Some(checkout) => {
                let (environment, unmatched) = self.environment_pull_requests(checkout);
                let title = checkout
                    .root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let rows = self.environment_section(checkout, &environment, theme, cx);
                sections.push(("environment", title, None, rows));
                unmatched
            }
            None => self.pull_requests(),
        };
        if !unmatched.is_empty() {
            let rows = unmatched
                .iter()
                .map(|pull_request| {
                    self.pull_request_row(pull_request, theme, cx)
                        .into_any_element()
                })
                .collect();
            sections.push((
                "pull-requests",
                crate::i18n::format!("拉取请求" => "Pull requests"),
                Some(unmatched.len()),
                rows,
            ));
        }
        if !terminals.is_empty() {
            let rows = self.background_section(&terminals, theme, cx);
            sections.push((
                "background-tasks",
                crate::i18n::format!("后台进程" => "Background processes"),
                Some(terminals.len()),
                rows,
            ));
        }
        let last = sections.len().saturating_sub(1);
        let menu = self
            .menu
            .clone()
            .map(|menu| self.pull_request_menu(&menu, theme, cx));
        let mut island = div()
            .id("thread-summary-panel")
            .debug_selector(|| "THREAD_SUMMARY_PANEL".to_owned())
            .track_focus(&self.focus)
            .w(px(SUMMARY_PANEL_WIDTH))
            .max_w_full()
            .py(px(ISLAND_PADDING_Y))
            .flex()
            .flex_col()
            .gap(px(SECTION_GAP))
            .rounded(px(ISLAND_RADIUS))
            .bg(theme.popover_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into()).spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(4.0), theme.menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font_family(".SystemUIFont")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && this.dismiss_menu(cx) {
                    cx.stop_propagation();
                }
            }));
        for (index, (key, title, count, rows)) in sections.into_iter().enumerate() {
            let expanded = self.expanded(key);
            let header = self.header(key, title, count, theme, cx);
            island = island.child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .when(index != last, |section| section.pb(px(SECTION_BOTTOM)))
                    .child(header)
                    .when(expanded, |section| {
                        section.child(
                            div()
                                .mt(px(2.0))
                                .px(px(BODY_PADDING_X))
                                .flex()
                                .flex_col()
                                .gap(px(ROW_GAP))
                                .children(rows),
                        )
                    })
                    .when(index != last, |section| {
                        section.child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left(px(BODY_PADDING_X))
                                .right(px(BODY_PADDING_X))
                                .h(px(0.5))
                                .bg(theme.border),
                        )
                    }),
            );
        }
        island.when_some(menu, |island, menu| island.child(menu))
    }
}

#[cfg(feature = "screenshot")]
impl SummaryPanel {
    /// `--batch4-state=panel:<state>` / `background:<state>`: a hovered
    /// header or row, a focused row, or the open "PR actions" menu. It is
    /// applied once the row it needs is drawn.
    pub fn set_capture_state(&mut self, state: &str, cx: &mut Context<Self>) {
        self.capture_state = Some(state.to_owned());
        cx.notify();
    }

    pub fn capture_ready(&self) -> bool {
        self.capture_state.is_none()
    }

    fn apply_capture_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.capture_state.clone() else {
            return;
        };
        let checkout = self.checkout();
        let (environment, unmatched) = match &checkout {
            Some(checkout) => self.environment_pull_requests(checkout),
            None => (Vec::new(), self.pull_requests()),
        };
        let terminal = self
            .background_terminals(cx)
            .first()
            .map(|terminal| SharedString::from(format!("summary-background-{}", terminal.item_id)));
        let applied = match state.as_str() {
            "section-hover" => {
                self.hovered_header = checkout.is_some().then_some("environment");
                checkout.is_some()
            }
            "pr-row-hover" => environment
                .first()
                .map(|pull_request| self.hovered_row = Some(pull_request_row_id(pull_request)))
                .is_some(),
            "unmatched-pr-hover" => unmatched
                .first()
                .map(|pull_request| self.hovered_row = Some(pull_request_row_id(pull_request)))
                .is_some(),
            "pr-actions-menu" => {
                let row = environment.first().and_then(|pull_request| {
                    let id = pull_request_row_id(pull_request);
                    let bounds = self.row_bounds.borrow().get(&id).copied()?;
                    Some((id, bounds, pull_request.clone()))
                });
                let settled = row.as_ref().map(|(_, bounds, _)| *bounds);
                let still = settled.is_some() && settled == self.capture_row_seen;
                self.capture_row_seen = settled;
                match row.filter(|_| still) {
                    Some((id, bounds, pull_request)) => {
                        self.hovered_row = Some(id);
                        self.menu = Some(PullRequestMenu {
                            identity_key: pull_request.identity_key,
                            url: pull_request.url,
                            anchor: menu_anchor(Some(bounds), bounds.origin),
                        });
                        true
                    }
                    None => false,
                }
            }
            "row-hover" => terminal.map(|id| self.hovered_row = Some(id)).is_some(),
            // A row click: the terminal's output tab in the right panel.
            "terminal-tab" => match self.background_terminals(cx).first().cloned() {
                Some(first) => {
                    self.open_terminal(&first, cx);
                    true
                }
                None => false,
            },
            "row-focus" => {
                let focus = terminal.and_then(|id| self.row_focus.borrow().get(&id).cloned());
                match focus {
                    Some(focus) => {
                        // `element.focus()` in the reference shows the ring.
                        self.capture_focus_ring = true;
                        cx.defer_in(window, move |_, window, cx| focus.focus(window, cx));
                        true
                    }
                    None => false,
                }
            }
            _ => true,
        };
        if applied {
            self.capture_state = None;
        }
        // Once more for the frame that shows it (or for the row to appear).
        cx.notify();
    }
}
