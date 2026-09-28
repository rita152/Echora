//! Pull Requests page: list, detail, diff, and review surfaces.
//!
//! Layout, colors, and copy are taken from the ChatGPT/Codex desktop app over
//! CDP; see `scripts/cdp_capture_pull_requests.mjs` and
//! `artifacts/pull-requests-reference/`. The page mirrors the reference
//! interaction tree item for item: list tabs, search, filter submenus, grouped
//! rows, the detail header with its nested buttons, the activity timeline, the
//! diff toolbar and file headers, the right-hand file tree, and the review tab.

mod activity;
mod detail;
mod diff;
mod list;
mod menus;
mod mutations;
mod render;
#[cfg(test)]
mod tests;
mod theme;

use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

use gpui::{Context, Entity, EventEmitter, FocusHandle, Focusable, Window, prelude::*};

use self::theme::PrTheme;
use super::file_editor::FileEditor;
use super::prompt_input::PromptInput;
use crate::{
    git_review::FileDiff,
    pull_requests::{
        GhClient, GroupKind, ListTab, PullRequestDetail, PullRequestFilter, PullRequestGroup,
        PullRequestSummary, User,
    },
    theme::ThemeMode,
};

/// Pause after the last keystroke before the search refetches.
const SEARCH_DEBOUNCE_MS: u64 = 250;

/// `Open chat`: the header opens the chat the reference already started for
/// the pull request.
#[derive(Clone, Debug)]
pub struct OpenPullRequestChat {
    pub thread_id: String,
}

/// Frames an expansion re-anchors its hunk while revealed rows are measured.
const EXPAND_ANCHOR_FRAMES: u8 = 4;
/// Lines one expand button reveals (the viewer's `expansionLineCount`).
const REVIEW_EXPANSION_LINES: u32 = 100;

/// How long the pointer rests on a file-tree row before its name tooltip.
/// The file tree's own name tooltip waits about 600ms; the app's Radix
/// tooltips on buttons wait 250ms (both measured in the reference).
const TREE_TOOLTIP_DELAY: Duration = Duration::from_millis(600);
const TOOLTIP_DELAY: Duration = Duration::from_millis(250);

/// The Code tab's diff layout, cycled by its view toggle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(super) enum DiffLayout {
    #[default]
    Unified,
    Split,
    /// Split for files with both additions and deletions, unified otherwise.
    Auto,
}

impl DiffLayout {
    fn next(self) -> Self {
        match self {
            Self::Unified => Self::Split,
            Self::Split => Self::Auto,
            Self::Auto => Self::Unified,
        }
    }

    pub(super) fn splits(self, file: &FileDiff) -> bool {
        match self {
            Self::Unified => false,
            Self::Split => true,
            Self::Auto => file.additions > 0 && file.deletions > 0,
        }
    }
}

impl EventEmitter<OpenPullRequestChat> for PullRequestsView {}

/// The reference opens a new conversation from the detail header with the pull
/// request prefilled but not sent.
#[derive(Clone, Debug)]
pub struct OpenChatForPullRequest {
    pub prompt: String,
}

impl EventEmitter<OpenChatForPullRequest> for PullRequestsView {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailTab {
    Summary,
    Code,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewScope {
    AllChanges,
    Commit(String),
}

/// A review tab opened from the change-stats button; it keeps its own scope and
/// sits next to `Summary`/`Code` in the header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewTab {
    pub scope: ReviewScope,
    pub commits: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListMenu {
    Filter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterSubmenu {
    Status,
    Repository,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineComment {
    pub path: String,
    pub line: u32,
    pub file: usize,
    pub old: bool,
}

pub struct PullRequestsView {
    mode: ThemeMode,
    client: Arc<GhClient>,
    focus: FocusHandle,
    generation: u64,
    detail_generation: u64,
    diff_generation: u64,
    file_generation: u64,
    reviewers_generation: u64,
    mutation_pending: bool,
    reviewers_loading: bool,
    reviewers_error: Option<String>,
    pane_width: f32,
    list_width: f32,
    compact_layout: bool,
    /// The system shows classic scroll bars, so the diff reserves their
    /// 11px gutter (`overflow-y-auto` without `scrollbar-gutter`).
    classic_scrollbars: bool,
    /// The tooltip trigger under the pointer, and the one whose tooltip
    /// shows (after its delay, or at once while another is open).
    tooltip_hover: Option<String>,
    tooltip: Option<String>,
    /// Lines a review tab revealed per gap (`path:hunk` → from its start,
    /// from its end).
    expanded_gaps: std::collections::HashMap<String, (u32, u32)>,
    /// Capture helper: expand the first gap once the review diff arrives.
    capture_expand_first_gap: bool,
    /// Capture: collapse every file once the diff has loaded.
    capture_collapse_all: bool,
    /// Capture: open an inline comment draft (on this new-file line of the
    /// first file, or its first line) once the diff has loaded.
    capture_inline_comment: Option<Option<u32>>,
    /// After revealing lines above a hunk, keep that hunk where it was:
    /// `(file, hunk, its first row's offset from the diff's top, frames)`.
    /// The anchor is re-applied while the revealed rows get measured.
    pending_expand_anchor: Option<(usize, usize, f32, u8)>,
    /// The Pull Requests page's own size, reported by the host every frame
    /// (the window minus the revealed sidebar and its hairline).
    page_width: f32,
    page_height: f32,
    /// How far the titlebar's traffic lights and sidebar trigger reach past
    /// the page's left edge: 128px with the sidebar closed, negative while
    /// the open sidebar keeps them clear. Reported by the host every frame.
    titlebar_inset: f32,
    /// Stored `app-shell:right-panel-width` ratio; `None` uses the reference
    /// default width.
    detail_ratio: Option<f32>,
    /// Width at the start of a separator drag, and the pointer x it began at.
    detail_resize: Option<(f32, f32)>,
    list_scroll: gpui::ScrollHandle,
    /// Avatar downloads by URL.
    avatars: std::collections::HashMap<String, AvatarState>,
    control_bounds: std::rc::Rc<
        std::cell::RefCell<std::collections::HashMap<String, gpui::Bounds<gpui::Pixels>>>,
    >,

    // List pane.
    /// Bumped per keystroke; the search reloads once typing pauses.
    search_serial: u64,
    tab: ListTab,
    filter: PullRequestFilter,
    repositories: Vec<String>,
    groups: Vec<PullRequestGroup>,
    list_loading: bool,
    list_error: Option<String>,
    query: String,
    search: Entity<PromptInput>,
    /// Tab, filter, and search text of the rows on screen.
    loaded_key: Option<(ListTab, PullRequestFilter, String)>,
    collapsed_groups: HashSet<GroupKind>,
    selected: Option<PullRequestSummary>,
    list_menu: Option<ListMenu>,
    filter_submenu: Option<FilterSubmenu>,
    filter_loading: bool,

    // Detail pane.
    detail: Option<PullRequestDetail>,
    /// The chat the reference associated with the selected pull request.
    chat_thread: Option<String>,
    detail_loading: bool,
    detail_error: Option<String>,
    detail_tab: DetailTab,
    review_tab: Option<ReviewTab>,
    fullscreen: bool,
    title_edit: Option<Entity<PromptInput>>,
    description_edit: Option<Entity<FileEditor>>,
    description_menu: bool,
    description_collapsed: bool,
    status_menu: bool,
    comment_menu: Option<String>,
    comment_edit: Option<(String, Entity<FileEditor>)>,
    reply: Option<(Option<String>, Entity<FileEditor>)>,
    reply_target: Option<String>,
    comment_box: Entity<FileEditor>,
    reviewers_open: bool,
    reviewers_query: Option<Entity<PromptInput>>,
    reviewers_results: Vec<User>,
    reviewers_selected: HashSet<String>,
    /// Comments whose collapse state the user flipped from its default.
    collapsed_comments: HashSet<String>,
    /// Activity commit groups the user opened, by feed index.
    expanded_commit_groups: HashSet<usize>,
    /// Clamped comment bodies the user expanded with `Show more`.
    expanded_bodies: HashSet<String>,
    body_measurements: activity::BodyMeasurements,
    checks_expanded: bool,
    activity_expanded: bool,

    // Diff surfaces.
    diff: Vec<FileDiff>,
    diff_loading: bool,
    diff_error: Option<String>,
    collapsed_files: HashSet<String>,
    diff_layout: DiffLayout,
    wrap: bool,
    rich: bool,
    words: bool,
    review_options_open: bool,
    scope_menu_open: bool,
    /// The scope menu's `Commits` flyout, opened by hovering its row.
    scope_commits_open: bool,
    file_tree_open: bool,
    tree_filter: Entity<PromptInput>,
    collapsed_folders: HashSet<String>,
    selected_file: Option<String>,
    inline_comment: Option<InlineComment>,
    inline_editor: Option<Entity<FileEditor>>,
    scrolled_to_file: Option<String>,

    notice: Option<String>,
    notice_serial: u64,
    /// Capture-only request applied once the selected pull request loads.
    capture_intent: Option<(Option<String>, bool)>,
    /// Scroll positions of the detail surfaces (the reference scrolls both the
    /// summary column and the diff).
    detail_scroll: gpui::ScrollHandle,
    /// Deferred capture scroll offset and the frames left to apply it.
    pending_detail_scroll: Option<(f32, u8)>,
    /// Frame-local capture offset read by the detail surfaces.
    pub(super) capture_offset: f32,
    diff_scroll: gpui::ScrollHandle,
    diff_viewport: diff::DiffViewport,
    /// Width available to diff code text, recomputed every frame so wrapped
    /// rows break exactly where the reference viewer breaks them.
    /// Wrap widths of a full-width and of a half-width (split) code cell.
    code_width: f32,
    split_code_width: f32,
    /// File contents fetched from the head commit so an `N unmodified lines`
    /// expander can render the lines it reveals.
    file_lines: std::collections::HashMap<String, Vec<String>>,
    file_lines_loading: HashSet<String>,
    file_errors: std::collections::HashMap<String, String>,
    /// Capture-only: the launch asked for a specific row, so "ready" must wait
    /// for that selection to land.
    #[cfg(feature = "screenshot")]
    capture_expect_selection: bool,
    /// Capture-only: interaction to open once the detail loads.
    capture_action: Option<String>,
}

/// The app-shell detail panel width for a main area of `page` × `height`:
/// `320 + ratio * (max - 320)` with `max = page - 352`, and without a stored
/// ratio the reference default `max(320, min(1.6 * height, page - 500),
/// min(640, page - 352))` clamped into the same range.
pub(super) fn detail_panel_width(page: f32, height: f32, ratio: Option<f32>) -> f32 {
    let maximum = (page - theme::DETAIL_MAX_INSET).max(theme::DETAIL_MIN_WIDTH);
    let minimum = theme::DETAIL_MIN_WIDTH.min(maximum);
    let width = match ratio {
        Some(ratio) if ratio.is_finite() => minimum + ratio.clamp(0.0, 1.0) * (maximum - minimum),
        _ => theme::DETAIL_MIN_WIDTH
            .max((height * 1.6).min(page - 500.0))
            .max(640.0_f32.min(page - theme::DETAIL_MAX_INSET)),
    };
    width.clamp(minimum, maximum)
}

/// The ratio a detail width stores (`L5e`).
pub(super) fn detail_panel_ratio(page: f32, width: f32) -> f32 {
    let maximum = (page - theme::DETAIL_MAX_INSET).max(theme::DETAIL_MIN_WIDTH);
    let minimum = theme::DETAIL_MIN_WIDTH.min(maximum);
    if maximum <= minimum {
        return 0.0;
    }
    ((width.clamp(minimum, maximum) - minimum) / (maximum - minimum)).clamp(0.0, 1.0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AvatarState {
    Loading,
    Ready(PathBuf),
    Failed,
}

/// Emitted when the separator drag ends, so the host can persist the ratio.
#[derive(Clone, Copy, Debug)]
pub struct DetailPanelResized {
    pub ratio: f32,
}

impl EventEmitter<DetailPanelResized> for PullRequestsView {}

impl Focusable for PullRequestsView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl PullRequestsView {
    pub fn new(mode: ThemeMode, cwd: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        Self::build(mode, cwd, true, cx)
    }

    fn build(mode: ThemeMode, cwd: Option<PathBuf>, load: bool, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            let mut input = PromptInput::pull_request_search(mode, "Search pull requests", cx);
            input.set_accessible_name("Search pull requests");
            input
        });
        let tree_filter = cx.new(|cx| {
            // The tree's filter uses the inbox search field's 14/18 type and
            // tertiary placeholder.
            let mut input = PromptInput::pull_request_search(mode, "Filter files…", cx);
            input.set_accessible_name("Filter files");
            input
        });
        let comment_box = cx.new(|cx| {
            let mut editor = FileEditor::prose(mode, "Pull request comment", cx);
            // The reference's composer shows `Leave a comment` while keeping the
            // `Pull request comment` accessible name.
            editor.set_placeholder("Leave a comment", cx);
            editor.set_accessible_name("Pull request comment");
            editor.set_text_metrics(14.0, 28.0, cx);
            editor
        });
        cx.subscribe(
            &comment_box,
            |_, _, _: &super::file_editor::EditorEvent, cx| cx.notify(),
        )
        .detach();
        cx.subscribe(
            &search,
            |this, input, _: &super::prompt_input::PromptChanged, cx| {
                let query = input.read(cx).text().to_string();
                this.set_query(query, cx);
            },
        )
        .detach();
        cx.subscribe(
            &tree_filter,
            |_, _, _: &super::prompt_input::PromptChanged, cx| {
                cx.notify();
            },
        )
        .detach();
        let mut view = Self {
            mode,
            client: Arc::new(GhClient::new(cwd)),
            focus: cx.focus_handle(),
            generation: 0,
            detail_generation: 0,
            diff_generation: 0,
            file_generation: 0,
            reviewers_generation: 0,
            mutation_pending: false,
            reviewers_loading: false,
            reviewers_error: None,
            pane_width: 606.0,
            list_width: 593.0,
            compact_layout: false,
            classic_scrollbars: false,
            tooltip_hover: None,
            tooltip: None,
            expanded_gaps: Default::default(),
            capture_expand_first_gap: false,
            capture_collapse_all: false,
            capture_inline_comment: None,
            pending_expand_anchor: None,
            page_width: 0.0,
            page_height: 0.0,
            titlebar_inset: 0.0,
            detail_ratio: None,
            detail_resize: None,
            list_scroll: gpui::ScrollHandle::new(),
            avatars: Default::default(),
            control_bounds: Default::default(),
            search_serial: 0,
            tab: ListTab::All,
            filter: PullRequestFilter::default(),
            repositories: Vec::new(),
            groups: Vec::new(),
            list_loading: true,
            list_error: None,
            query: String::new(),
            search,
            loaded_key: None,
            collapsed_groups: HashSet::new(),
            selected: None,
            list_menu: None,
            filter_submenu: None,
            filter_loading: false,
            detail: None,
            chat_thread: None,
            detail_loading: false,
            detail_error: None,
            detail_tab: DetailTab::Summary,
            review_tab: None,
            fullscreen: false,
            title_edit: None,
            description_edit: None,
            description_menu: false,
            description_collapsed: false,
            status_menu: false,
            comment_menu: None,
            comment_edit: None,
            reply: None,
            reply_target: None,
            comment_box,
            reviewers_open: false,
            reviewers_query: None,
            reviewers_results: Vec::new(),
            reviewers_selected: HashSet::new(),
            collapsed_comments: HashSet::new(),
            expanded_commit_groups: HashSet::new(),
            expanded_bodies: HashSet::new(),
            body_measurements: Default::default(),
            checks_expanded: true,
            activity_expanded: true,
            diff: Vec::new(),
            diff_loading: false,
            diff_error: None,
            collapsed_files: HashSet::new(),
            diff_layout: DiffLayout::Unified,
            // The reference diff view opens with word wrap on, which is why its
            // toolbar offers `Disable word wrap`.
            wrap: true,
            rich: false,
            // Word highlights are on too (`Disable word diffs`).
            words: true,
            review_options_open: false,
            scope_menu_open: false,
            scope_commits_open: false,
            file_tree_open: false,
            tree_filter,
            collapsed_folders: HashSet::new(),
            selected_file: None,
            inline_comment: None,
            inline_editor: None,
            scrolled_to_file: None,
            notice: None,
            notice_serial: 0,
            capture_intent: None,
            detail_scroll: gpui::ScrollHandle::new(),
            pending_detail_scroll: None,
            capture_offset: 0.0,
            diff_scroll: gpui::ScrollHandle::new(),
            diff_viewport: Default::default(),
            code_width: 500.0,
            split_code_width: 250.0,
            file_lines: std::collections::HashMap::new(),
            file_lines_loading: HashSet::new(),
            file_errors: Default::default(),
            #[cfg(feature = "screenshot")]
            capture_expect_selection: false,
            capture_action: None,
        };
        if load {
            view.reload(cx);
        }
        view
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.search.update(cx, |input, cx| input.set_mode(mode, cx));
        self.tree_filter
            .update(cx, |input, cx| input.set_mode(mode, cx));
        self.comment_box
            .update(cx, |editor, cx| editor.set_mode(mode, cx));
        if let Some(input) = &self.title_edit {
            input.update(cx, |input, cx| input.set_mode(mode, cx));
        }
        if let Some(editor) = self.description_edit.clone() {
            editor.update(cx, |editor, cx| editor.set_mode(mode, cx));
        }
        if let Some((_, editor)) = self.comment_edit.clone() {
            editor.update(cx, |editor, cx| editor.set_mode(mode, cx));
        }
        if let Some((_, editor)) = self.reply.clone() {
            editor.update(cx, |editor, cx| editor.set_mode(mode, cx));
        }
        if let Some(editor) = self.inline_editor.clone() {
            editor.update(cx, |editor, cx| editor.set_mode(mode, cx));
        }
        if let Some(query) = self.reviewers_query.clone() {
            query.update(cx, |input, cx| input.set_mode(mode, cx));
        }
        cx.notify();
    }

    pub fn theme(&self) -> PrTheme {
        PrTheme::for_mode(self.mode)
    }

    /// The host reports the page's size each frame; it is not a render input
    /// of its own, so no notify.
    pub fn set_page_size(&mut self, width: f32, height: f32) {
        self.page_width = width;
        self.page_height = height;
    }

    /// Reported with the page size, and for the same reason not a render
    /// input of its own.
    pub fn set_titlebar_inset(&mut self, inset: f32) {
        self.titlebar_inset = inset;
    }

    pub fn set_detail_ratio(&mut self, ratio: Option<f32>, cx: &mut Context<Self>) {
        if self.detail_ratio != ratio {
            self.detail_ratio = ratio;
            cx.notify();
        }
    }

    pub(super) fn begin_detail_resize(&mut self, x: f32) {
        self.detail_resize = Some((self.pane_width, x));
    }

    pub(super) fn drag_detail_resize(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some((start, origin)) = self.detail_resize else {
            return;
        };
        let page = self.pane_width + self.list_width;
        let width = start + (origin - x);
        self.detail_ratio = Some(detail_panel_ratio(page, width));
        cx.notify();
    }

    pub(super) fn end_detail_resize(&mut self, cx: &mut Context<Self>) {
        if self.detail_resize.take().is_some()
            && let Some(ratio) = self.detail_ratio
        {
            cx.emit(DetailPanelResized { ratio });
        }
    }

    /// Starts downloads for avatars not yet cached; rows draw the reference's
    /// white placeholder circle meanwhile.
    fn request_avatars(&mut self, urls: Vec<String>, cx: &mut Context<Self>) {
        for url in urls {
            if self.avatars.contains_key(&url) {
                continue;
            }
            if let Some(path) = crate::pull_requests::avatars::cached(&url) {
                self.avatars.insert(url, AvatarState::Ready(path));
                continue;
            }
            self.avatars.insert(url.clone(), AvatarState::Loading);
            cx.spawn(async move |this, cx| {
                let fetch = url.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { crate::pull_requests::avatars::fetch(&fetch) })
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.avatars.insert(
                        url,
                        match result {
                            Ok(path) => AvatarState::Ready(path),
                            Err(_) => AvatarState::Failed,
                        },
                    );
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub(super) fn avatar_path(&self, url: &str) -> Option<&PathBuf> {
        match self.avatars.get(url) {
            Some(AvatarState::Ready(path)) => Some(path),
            _ => None,
        }
    }

    /// Re-enters the page: refresh the list and the selected detail so the data
    /// matches the reference, which reloads whenever the page is shown.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.reset_diff(cx);
        self.reload(cx);
        if let Some(summary) = self.selected.clone() {
            self.load_detail(summary.repository, summary.number, cx);
        }
    }

    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// Code column width used by wrapped diff rows.
    pub(super) fn set_code_width(&mut self, unified: f32, split: f32) {
        self.code_width = unified;
        self.split_code_width = split;
    }

    pub(super) fn code_width(&self, split: bool) -> f32 {
        if split {
            self.split_code_width
        } else {
            self.code_width
        }
    }

    /// Capture diagnostics: the readiness inputs, printed by the screenshot
    /// scheduler when `GPUI_PR_DEBUG` is set.
    #[cfg(feature = "screenshot")]
    pub fn capture_diagnostics(&self) -> String {
        format!(
            "list_loading={} groups={} selected={} detail_loading={} detail={} detail_error={:?} diff_loading={} diff={} intent={} expect_selection={} pending_scroll={:?}",
            self.list_loading,
            self.groups.len(),
            self.selected.is_some(),
            self.detail_loading,
            self.detail.is_some(),
            self.detail_error,
            self.diff_loading,
            self.diff.len(),
            self.capture_intent.is_some(),
            self.capture_expect_selection,
            self.pending_detail_scroll,
        )
    }

    /// True when the page has finished every load the current tab needs, so a
    /// screenshot capture is stable.
    #[cfg(feature = "screenshot")]
    pub fn capture_ready(&self) -> bool {
        if self.list_loading || self.detail_loading || self.diff_loading {
            return false;
        }
        // Avatars are part of the frame the reference shows.
        if self
            .avatars
            .values()
            .any(|state| *state == AvatarState::Loading)
        {
            return false;
        }
        if self.capture_intent.is_some() || self.pending_detail_scroll.is_some() {
            return false;
        }
        // Revealed review context waits for its file.
        if !self.file_lines_loading.is_empty() {
            return false;
        }
        if self.capture_expand_first_gap
            || self.capture_collapse_all
            || self.capture_inline_comment.is_some()
            || self.pending_expand_anchor.is_some()
        {
            return false;
        }
        // A hovered tree row is still waiting for its tooltip.
        if self.tooltip_hover.is_some() && self.tooltip.is_none() {
            return false;
        }
        // The `Commits` flyout places itself once its row has laid out.
        if self.scope_commits_open
            && !self
                .control_bounds
                .borrow()
                .contains_key("pr-scope-commits")
        {
            return false;
        }
        if self.capture_expect_selection && self.selected.is_none() {
            return false;
        }
        if self.detail_error.is_some() || self.list_error.is_some() || self.diff_error.is_some() {
            return false;
        }
        if self.selected.is_some() && self.detail.is_none() {
            return false;
        }
        if self.detail_tab == DetailTab::Code || self.review_tab.is_some() {
            return !self.diff.is_empty() && self.diff_capture_offset_settled();
        }
        true
    }

    /// Selects a tab and (for the review tab) opens it, for deterministic
    /// captures. `tab` accepts `summary`, `code`, or `review`.
    #[cfg(feature = "screenshot")]
    pub fn open_tab_for_capture(&mut self, tab: &str, cx: &mut Context<Self>) {
        if self.detail_loading || self.detail.is_none() {
            self.capture_intent = Some((Some(tab.to_string()), false));
            cx.notify();
            return;
        }
        match tab {
            "code" => self.set_detail_tab(DetailTab::Code, cx),
            "review" => self.open_review_tab(cx),
            _ => self.set_detail_tab(DetailTab::Summary, cx),
        }
    }

    /// Capture helper: opens one interaction state after the detail loads.
    #[cfg(feature = "screenshot")]
    pub fn open_action_for_capture(&mut self, action: &str, cx: &mut Context<Self>) {
        self.capture_action = Some(action.to_string());
        // The inbox's menus need no selection; the rest wait for the detail.
        if action.starts_with("filter-") {
            self.apply_capture_intent(cx);
        }
        cx.notify();
    }

    /// Capture helper: applies a status filter before the list loads.
    #[cfg(feature = "screenshot")]
    pub fn set_status_filter_for_capture(
        &mut self,
        status: crate::pull_requests::StatusFilter,
        cx: &mut Context<Self>,
    ) {
        self.filter.status = status;
        self.reload(cx);
    }

    /// Capture helper: switches the list between the `All`, `Reviewing`, and
    /// `Authored` tabs before the list loads.
    #[cfg(feature = "screenshot")]
    pub fn set_list_tab_for_capture(&mut self, tab: ListTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.reload(cx);
    }

    /// Capture helper: types a query into the search field so the filtered
    /// list and the empty state can be captured.
    #[cfg(feature = "screenshot")]
    pub fn set_search_for_capture(&mut self, query: &str, cx: &mut Context<Self>) {
        self.search.update(cx, |input, cx| {
            input.set_text_silently(query.to_string(), cx)
        });
        self.query = query.to_string();
        self.reload(cx);
    }

    /// Capture helper: collapses one grouping header (`Previously reviewed` or
    /// `Authored`) without a pointer.
    #[cfg(feature = "screenshot")]
    pub fn collapse_group_for_capture(&mut self, kind: GroupKind, cx: &mut Context<Self>) {
        self.collapsed_groups.insert(kind);
        cx.notify();
    }

    /// Capture helper: scrolls the detail column once it has content.
    #[cfg(feature = "screenshot")]
    pub fn scroll_detail_for_capture(&mut self, offset: f32, cx: &mut Context<Self>) {
        self.pending_detail_scroll = Some((offset, 8));
        cx.notify();
    }

    /// Moves a deferred capture offset into the frame-local field the summary
    /// surface reads while rendering. The scroll handle clamps offsets that
    /// exceed its measured content, so captures shift the column at render time
    /// instead of re-applying the scroll offset frame by frame.
    pub(super) fn take_capture_offset(&mut self) {
        if let Some((offset, _)) = self.pending_detail_scroll.take() {
            self.capture_offset = offset;
        }
    }

    /// Capture helper: shows or hides the diff file tree.
    #[cfg(feature = "screenshot")]
    pub fn set_file_tree_for_capture(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.detail_loading || self.detail.is_none() {
            let tab = self.capture_intent.take().and_then(|(tab, _)| tab);
            self.capture_intent = Some((tab, open));
            cx.notify();
            return;
        }
        if self.file_tree_open != open {
            self.toggle_file_tree(cx);
        }
    }

    fn apply_capture_intent(&mut self, cx: &mut Context<Self>) {
        // The tab first: switching tabs dismisses open menus.
        if let Some((tab, file_tree)) = self.capture_intent.take() {
            if let Some(tab) = tab.as_deref() {
                match tab {
                    "code" => self.set_detail_tab(DetailTab::Code, cx),
                    "review" => self.open_review_tab(cx),
                    _ => self.set_detail_tab(DetailTab::Summary, cx),
                }
            }
            if file_tree && !self.file_tree_open {
                self.toggle_file_tree(cx);
            }
        }
        if let Some(action) = self.capture_action.take() {
            match action.as_str() {
                "title-edit" => self.begin_title_edit(cx),
                "reviewers" => self.open_reviewers(cx),
                "status-menu" => self.status_menu = true,
                "description-menu" => self.description_menu = true,
                "comment-menu" => {
                    if let Some(id) = self.detail.as_ref().and_then(|detail| {
                        detail.comments.first().map(|comment| comment.id.clone())
                    }) {
                        self.comment_menu = Some(id);
                    }
                }
                "split" => self.diff_layout = DiffLayout::Split,
                "auto-layout" => self.diff_layout = DiffLayout::Auto,
                // The diff loads after the detail, so collapse once it has.
                "collapse-all" => self.capture_collapse_all = true,
                "review-options" => self.review_options_open = true,
                "scope-menu" => self.scope_menu_open = true,
                "scope-commits" => {
                    self.scope_menu_open = true;
                    self.scope_commits_open = true;
                }
                "scope-commit" | "scope-commit-menu" => {
                    let first = self
                        .review_tab
                        .as_ref()
                        .and_then(|tab| tab.commits.first())
                        .map(|(sha, _)| sha.clone());
                    if let Some(sha) = first {
                        self.set_review_scope(ReviewScope::Commit(sha), cx);
                    }
                    if action == "scope-commit-menu" {
                        self.scope_menu_open = true;
                        self.scope_commits_open = true;
                    }
                }
                "fullscreen" => self.fullscreen = true,
                "expand-first-gap" => self.capture_expand_first_gap = true,
                "expand-commits" => {
                    let groups: Vec<usize> = self
                        .detail
                        .as_ref()
                        .map(|detail| {
                            detail
                                .activity
                                .iter()
                                .enumerate()
                                .filter(|(_, item)| {
                                    matches!(item, crate::pull_requests::ActivityItem::Commits { commits } if commits.len() > 1)
                                })
                                .map(|(index, _)| index)
                                .collect()
                        })
                        .unwrap_or_default();
                    self.expanded_commit_groups.extend(groups);
                }
                "filter-menu" => self.list_menu = Some(ListMenu::Filter),
                "filter-status" => {
                    self.list_menu = Some(ListMenu::Filter);
                    self.filter_submenu = Some(FilterSubmenu::Status);
                }
                "filter-repository" => {
                    self.list_menu = Some(ListMenu::Filter);
                    self.filter_submenu = Some(FilterSubmenu::Repository);
                }
                // `inline-comment[=N]`: a draft on new-file line N of the
                // first file (its first line by default) once the diff loads.
                action if action == "inline-comment" || action.starts_with("inline-comment=") => {
                    self.capture_inline_comment = Some(
                        action
                            .strip_prefix("inline-comment=")
                            .and_then(|line| line.parse().ok()),
                    );
                }
                _ => {}
            }
            cx.notify();
        }
    }

    // ------------------------------------------------------------------
    // Data loading
    // ------------------------------------------------------------------

    /// The search text is part of the GitHub query, as in the reference: each
    /// pause in typing refetches every section.
    fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.query == query {
            return;
        }
        self.query = query;
        self.search_serial += 1;
        let serial = self.search_serial;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(SEARCH_DEBOUNCE_MS))
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.search_serial == serial {
                    view.reload(cx);
                }
            });
        })
        .detach();
    }

    /// Fetches the list. A new tab, filter, or search text replaces the rows
    /// with per-section skeletons (the reference's query key changed); the
    /// same query refetches behind the loaded rows with the `Refreshing`
    /// spinner.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let client = self.client.clone();
        let tab = self.tab;
        let filter = self.filter.clone();
        let query = self.query.trim().to_string();
        let key = (tab, filter.clone(), query.clone());
        if self.loaded_key.as_ref() != Some(&key) {
            self.groups.clear();
            self.loaded_key = Some(key);
        }
        self.list_loading = true;
        self.list_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    client
                        .list(tab, &filter, &query)
                        .map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.list_loading = false;
                view.filter_loading = false;
                match result {
                    Ok(groups) => {
                        let urls = groups
                            .iter()
                            .filter(|group| group.kind.shows_author_avatar())
                            .flat_map(|group| &group.items)
                            .filter_map(|item| item.author_avatar_url.clone())
                            .collect();
                        view.groups = groups;
                        view.list_error = None;
                        view.reload_repositories();
                        view.request_avatars(urls, cx);
                    }
                    Err(error) => {
                        view.groups = Vec::new();
                        view.list_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Repository filter entries come from the loaded list, so opening the page
    /// costs the same two searches the list already performs.
    fn reload_repositories(&mut self) {
        let mut repositories: Vec<String> = self
            .groups
            .iter()
            .flat_map(|group| group.items.iter().map(|item| item.repository.clone()))
            .collect();
        repositories.sort();
        repositories.dedup();
        self.repositories.extend(repositories);
        self.repositories.sort();
        self.repositories.dedup();
    }

    fn load_detail(&mut self, repository: String, number: u64, cx: &mut Context<Self>) {
        self.detail_generation += 1;
        let generation = self.detail_generation;
        let client = self.client.clone();
        self.detail_loading = true;
        self.detail_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let (result, chat_thread) = cx
                .background_executor()
                .spawn(async move {
                    let chat_thread =
                        crate::pull_requests::associations::chat_thread(&repository, number);
                    (
                        client
                            .detail(&repository, number)
                            .map_err(|error| format!("{error:#}")),
                        chat_thread,
                    )
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.detail_generation != generation {
                    return;
                }
                view.detail_loading = false;
                view.chat_thread = chat_thread;
                match result {
                    Ok(detail) => {
                        let urls: Vec<String> = std::iter::once(detail.author.avatar_url.clone())
                            .chain(
                                detail
                                    .comments
                                    .iter()
                                    .map(|comment| comment.avatar_url.clone()),
                            )
                            .chain(detail.review_threads.iter().flat_map(|thread| {
                                thread
                                    .comments
                                    .iter()
                                    .map(|comment| comment.avatar_url.clone())
                            }))
                            .chain(detail.commits.iter().map(|commit| commit.avatar_url()))
                            .flatten()
                            .collect();
                        view.request_avatars(urls, cx);
                        view.selected = Some(detail.summary.clone());
                        view.detail = Some(detail);
                        view.detail_error = None;
                        if view.detail_tab == DetailTab::Code || view.review_tab.is_some() {
                            view.load_diff(cx);
                        }
                        view.apply_capture_intent(cx);
                    }
                    Err(error) => {
                        view.detail = None;
                        view.detail_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_diff(&mut self, cx: &mut Context<Self>) {
        let Some(summary) = self.selected.clone() else {
            return;
        };
        if !self.diff.is_empty() || self.diff_loading {
            return;
        }
        self.diff_generation += 1;
        let scope = self
            .review_tab
            .as_ref()
            .map(|tab| tab.scope.clone())
            .unwrap_or(ReviewScope::AllChanges);
        self.diff_loading = true;
        self.diff_error = None;
        let client = self.client.clone();
        let generation = self.diff_generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match scope {
                        ReviewScope::AllChanges => client.diff(&summary.repository, summary.number),
                        ReviewScope::Commit(sha) => client.commit_diff(&summary.repository, &sha),
                    }
                    .map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.diff_generation != generation {
                    return;
                }
                view.diff_loading = false;
                match result {
                    Ok(files) => {
                        view.diff = files;
                        view.diff_error = None;
                        if view.rich {
                            let paths: Vec<_> = view
                                .diff
                                .iter()
                                .filter(|file| file.path.ends_with(".md") && file.status != 'D')
                                .map(|file| file.path.clone())
                                .collect();
                            for path in paths {
                                view.load_file_lines(path, cx);
                            }
                        }
                    }
                    Err(error) => {
                        view.diff = Vec::new();
                        view.diff_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn show_notice(&mut self, text: String, cx: &mut Context<Self>) {
        self.notice = Some(text);
        cx.notify();
        self.notice_serial += 1;
        let serial = self.notice_serial;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(8)).await;
            let _ = this.update(cx, |view, cx| {
                if view.notice_serial == serial {
                    view.notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // ------------------------------------------------------------------
    // List pane interactions
    // ------------------------------------------------------------------

    pub fn select_tab(&mut self, tab: ListTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.reload(cx);
    }

    pub fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.search
            .update(cx, |input, cx| input.set_text_silently("", cx));
        self.set_query(String::new(), cx);
    }

    pub fn toggle_filter_menu(&mut self, cx: &mut Context<Self>) {
        self.list_menu = match self.list_menu {
            Some(ListMenu::Filter) => None,
            None => Some(ListMenu::Filter),
        };
        self.filter_submenu = None;
        cx.notify();
    }

    pub fn hover_filter_submenu(&mut self, submenu: Option<FilterSubmenu>, cx: &mut Context<Self>) {
        self.filter_submenu = submenu;
        cx.notify();
    }

    pub fn apply_status_filter(
        &mut self,
        status: crate::pull_requests::StatusFilter,
        cx: &mut Context<Self>,
    ) {
        self.filter.status = status;
        self.list_menu = None;
        self.filter_submenu = None;
        self.filter_loading = true;
        cx.notify();
        self.reload(cx);
    }

    pub fn apply_repository_filter(&mut self, repository: Option<String>, cx: &mut Context<Self>) {
        self.filter.repository = repository;
        self.list_menu = None;
        self.filter_submenu = None;
        self.filter_loading = true;
        self.reload(cx);
    }

    pub fn toggle_group(&mut self, kind: GroupKind, cx: &mut Context<Self>) {
        if !self.collapsed_groups.remove(&kind) {
            self.collapsed_groups.insert(kind);
        }
        cx.notify();
    }

    /// Capture helper: selects a row by position in the filtered list, waiting
    /// for the list to load first.
    #[cfg(feature = "screenshot")]
    pub fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
        self.select_matching(Some(index), None, cx);
    }

    /// Capture helper: selects the row whose title contains `needle`, which
    /// keeps the reference and the native capture on the same pull request even
    /// when their list orders differ.
    #[cfg(feature = "screenshot")]
    pub fn select_title(&mut self, needle: &str, cx: &mut Context<Self>) {
        self.select_matching(None, Some(needle.to_string()), cx);
    }

    #[cfg(feature = "screenshot")]
    fn select_matching(
        &mut self,
        index: Option<usize>,
        needle: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.capture_expect_selection = true;
        let pick = |items: &[PullRequestSummary]| -> Option<PullRequestSummary> {
            if let Some(needle) = needle.as_deref() {
                items
                    .iter()
                    .find(|item| item.title.contains(needle))
                    .cloned()
            } else {
                index.and_then(|index| items.get(index).cloned())
            }
        };
        let items: Vec<PullRequestSummary> =
            crate::pull_requests::filter_groups(&self.groups, &self.filter, "")
                .into_iter()
                .flat_map(|group| group.items)
                .filter(|item| !item.repository.is_empty())
                .collect();
        if let Some(summary) = pick(&items) {
            self.select(summary, cx);
        } else if self.list_loading {
            let needle = needle.clone();
            cx.spawn(async move |this, cx| {
                for _ in 0..80 {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                    let done = this
                        .update(cx, |view, cx| {
                            if view.list_loading {
                                return false;
                            }
                            let items: Vec<PullRequestSummary> =
                                crate::pull_requests::filter_groups(&view.groups, &view.filter, "")
                                    .into_iter()
                                    .flat_map(|group| group.items)
                                    .filter(|item| !item.repository.is_empty())
                                    .collect();
                            let summary = if let Some(needle) = needle.as_deref() {
                                items
                                    .iter()
                                    .find(|item| item.title.contains(needle))
                                    .cloned()
                            } else {
                                index.and_then(|index| items.get(index).cloned())
                            };
                            match summary {
                                Some(summary) => {
                                    view.select(summary, cx);
                                    true
                                }
                                None => false,
                            }
                        })
                        .unwrap_or(true);
                    if done {
                        return;
                    }
                }
            })
            .detach();
        }
    }

    pub fn select(&mut self, summary: PullRequestSummary, cx: &mut Context<Self>) {
        let deselect = self.selected.as_ref().is_some_and(|current| {
            current.number == summary.number && current.repository == summary.repository
        });
        self.detail_generation += 1;
        self.reset_diff(cx);
        self.detail = None;
        self.chat_thread = None;
        self.expanded_commit_groups.clear();
        self.expanded_bodies.clear();
        self.collapsed_comments.clear();
        self.detail_loading = false;
        self.detail_error = None;
        self.review_tab = None;
        self.detail_tab = DetailTab::Summary;
        self.title_edit = None;
        self.description_edit = None;
        self.comment_edit = None;
        self.reply = None;
        self.reply_target = None;
        self.comment_box
            .update(cx, |editor, cx| editor.set_text_silently("", cx));
        self.dismiss_menus(cx);
        self.detail_scroll
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        self.selected = if deselect {
            None
        } else {
            Some(summary.clone())
        };
        if !deselect {
            self.load_detail(summary.repository, summary.number, cx);
        }
        cx.notify();
    }

    fn reset_diff(&mut self, cx: &mut Context<Self>) {
        self.diff_generation += 1;
        self.file_generation += 1;
        self.diff.clear();
        self.diff_viewport = Default::default();
        self.diff_loading = false;
        self.diff_error = None;
        self.file_lines.clear();
        self.file_lines_loading.clear();
        self.file_errors.clear();
        self.collapsed_files.clear();
        self.expanded_gaps.clear();
        self.selected_file = None;
        self.inline_comment = None;
        self.inline_editor = None;
        self.scrolled_to_file = None;
        self.tree_filter
            .update(cx, |input, cx| input.set_text_silently("", cx));
        self.diff_scroll
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
    }

    // ------------------------------------------------------------------
    // Detail pane interactions
    // ------------------------------------------------------------------

    pub fn set_detail_tab(&mut self, tab: DetailTab, cx: &mut Context<Self>) {
        if self
            .review_tab
            .as_ref()
            .is_some_and(|tab| matches!(tab.scope, ReviewScope::Commit(_)))
        {
            self.reset_diff(cx);
        }
        self.dismiss_menus(cx);
        self.detail_tab = tab;
        self.review_tab = None;
        if tab == DetailTab::Code {
            self.load_diff(cx);
        }
        cx.notify();
    }

    pub fn close_review_tab(&mut self, cx: &mut Context<Self>) {
        self.set_detail_tab(self.detail_tab, cx);
    }

    pub fn open_review_tab(&mut self, cx: &mut Context<Self>) {
        let commits = self
            .detail
            .as_ref()
            .map(|detail| {
                detail
                    .commits
                    .iter()
                    .map(|commit| (commit.sha.clone(), commit.subject.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.review_tab = Some(ReviewTab {
            scope: ReviewScope::AllChanges,
            commits,
        });
        self.scope_menu_open = false;
        self.scope_commits_open = false;
        self.load_diff(cx);
        cx.notify();
    }

    pub fn set_review_scope(&mut self, scope: ReviewScope, cx: &mut Context<Self>) {
        if let Some(tab) = self.review_tab.as_mut()
            && tab.scope != scope
        {
            tab.scope = scope;
            self.reset_diff(cx);
            self.load_diff(cx);
        }
        self.scope_menu_open = false;
        self.scope_commits_open = false;
        cx.notify();
    }

    pub fn toggle_fullscreen(&mut self, cx: &mut Context<Self>) {
        self.fullscreen = !self.fullscreen;
        cx.notify();
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool, cx: &mut Context<Self>) {
        self.fullscreen = fullscreen;
        cx.notify();
    }

    pub fn toggle_status_menu(&mut self, cx: &mut Context<Self>) {
        self.status_menu = !self.status_menu;
        self.description_menu = false;
        self.comment_menu = None;
        self.review_options_open = false;
        cx.notify();
    }

    pub fn toggle_description_menu(&mut self, cx: &mut Context<Self>) {
        self.description_menu = !self.description_menu;
        self.status_menu = false;
        self.comment_menu = None;
        cx.notify();
    }

    pub fn toggle_review_options(&mut self, cx: &mut Context<Self>) {
        self.review_options_open = !self.review_options_open;
        self.status_menu = false;
        self.description_menu = false;
        self.scope_menu_open = false;
        self.scope_commits_open = false;
        cx.notify();
    }

    pub fn toggle_scope_menu(&mut self, cx: &mut Context<Self>) {
        self.scope_menu_open = !self.scope_menu_open;
        self.scope_commits_open = false;
        self.review_options_open = false;
        cx.notify();
    }

    pub fn set_scope_commits_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.scope_commits_open != open {
            self.scope_commits_open = open;
            cx.notify();
        }
    }

    pub fn toggle_comment_menu(&mut self, id: String, cx: &mut Context<Self>) {
        if self.comment_menu.as_deref() == Some(id.as_str()) {
            self.comment_menu = None;
        } else {
            self.comment_menu = Some(id);
            self.status_menu = false;
            self.description_menu = false;
        }
        cx.notify();
    }

    pub fn dismiss_menus(&mut self, cx: &mut Context<Self>) -> bool {
        let mut dismissed = false;
        if self.list_menu.is_some() {
            self.list_menu = None;
            self.filter_submenu = None;
            dismissed = true;
        }
        if self.status_menu {
            self.status_menu = false;
            dismissed = true;
        }
        if self.description_menu {
            self.description_menu = false;
            dismissed = true;
        }
        if self.comment_menu.is_some() {
            self.comment_menu = None;
            dismissed = true;
        }
        if self.review_options_open {
            self.review_options_open = false;
            dismissed = true;
        }
        if self.scope_menu_open {
            self.scope_menu_open = false;
            self.scope_commits_open = false;
            dismissed = true;
        }
        if self.reviewers_open {
            // `Esc` closes the reviewer popover, matching the reference.
            self.close_reviewers(cx);
            dismissed = true;
        }
        if dismissed {
            cx.notify();
        }
        dismissed
    }

    pub fn toggle_description(&mut self, cx: &mut Context<Self>) {
        self.description_collapsed = !self.description_collapsed;
        cx.notify();
    }

    pub fn toggle_checks(&mut self, cx: &mut Context<Self>) {
        self.checks_expanded = !self.checks_expanded;
        cx.notify();
    }

    pub fn toggle_activity(&mut self, cx: &mut Context<Self>) {
        self.activity_expanded = !self.activity_expanded;
        cx.notify();
    }

    pub fn toggle_commit_group(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.expanded_commit_groups.remove(&index) {
            self.expanded_commit_groups.insert(index);
        }
        cx.notify();
    }

    pub fn toggle_body_expanded(&mut self, id: String, cx: &mut Context<Self>) {
        if !self.expanded_bodies.remove(&id) {
            self.expanded_bodies.insert(id);
        }
        cx.notify();
    }

    pub fn toggle_comment_collapsed(&mut self, id: String, cx: &mut Context<Self>) {
        if !self.collapsed_comments.remove(&id) {
            self.collapsed_comments.insert(id);
        }
        cx.notify();
    }

    pub fn begin_title_edit(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let title = detail.summary.title.clone();
        let mode = self.mode;
        let input = cx.new(|cx| {
            let mut input = PromptInput::inline_other(mode, "Pull request title", false, cx);
            input.set_accessible_name("Pull request title");
            input.set_text_silently(&title, cx);
            input
        });
        cx.subscribe(
            &input,
            |view, _, _: &super::prompt_input::PromptSubmitted, cx| view.save_title(cx),
        )
        .detach();
        self.title_edit = Some(input.clone());
        cx.notify();
        let focus = input;
        cx.defer(move |cx| {
            focus.update(cx, |input, cx| input.focus_handle(cx));
        });
    }

    pub fn cancel_title_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.title_edit = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn begin_description_edit(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let body = detail.body.clone();
        let mode = self.mode;
        let editor = cx.new(|cx| {
            let mut editor = FileEditor::prose(mode, "Edit description", cx);
            editor.set_accessible_name("Edit description");
            editor.set_text_silently(&body, cx);
            editor
        });
        cx.subscribe(&editor, |_, _, _: &super::file_editor::EditorEvent, cx| {
            cx.notify()
        })
        .detach();
        self.description_edit = Some(editor);
        self.description_menu = false;
        cx.notify();
    }

    pub fn cancel_description_edit(&mut self, cx: &mut Context<Self>) {
        self.description_edit = None;
        cx.notify();
    }

    /// `Chat` creates a conversation for the pull request, prefilled but not
    /// sent; `Open chat` reuses the existing one when the server reports it.
    pub fn open_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(thread_id) = self.chat_thread.clone() {
            cx.emit(OpenPullRequestChat { thread_id });
            return;
        }
        let Some(summary) = self.selected.clone() else {
            return;
        };
        let prompt = format!(
            "Help me understand pull request #{}: {} {}",
            summary.number, summary.title, summary.url
        );
        cx.emit(OpenChatForPullRequest { prompt });
    }

    pub fn open_in_browser(&mut self, cx: &mut Context<Self>) {
        if let Some(summary) = self.selected.clone() {
            cx.open_url(&summary.url);
        }
    }

    pub fn open_reviewers(&mut self, cx: &mut Context<Self>) {
        self.reviewers_generation += 1;
        self.reviewers_loading = false;
        self.reviewers_error = None;
        self.reviewers_open = true;
        self.reviewers_results.clear();
        self.reviewers_selected.clear();
        let mode = self.mode;
        let input = cx.new(|cx| {
            // The field's own placeholder is the reference's `Request review
            // from…`; the results area shows the longer hint.
            let mut input = PromptInput::inline_other(mode, "Request review from…", false, cx);
            input.set_accessible_name("Request review from");
            input
        });
        cx.subscribe(
            &input,
            |this, input, _: &super::prompt_input::PromptChanged, cx| {
                let query = input.read(cx).text().to_string();
                this.search_reviewers(query, cx);
            },
        )
        .detach();
        self.reviewers_query = Some(input.clone());
        cx.notify();
        cx.defer(move |cx| {
            input.update(cx, |input, cx| input.focus_handle(cx));
        });
    }

    pub fn close_reviewers(&mut self, cx: &mut Context<Self>) {
        self.reviewers_generation += 1;
        self.reviewers_loading = false;
        self.reviewers_open = false;
        self.reviewers_results.clear();
        self.reviewers_selected.clear();
        self.reviewers_query = None;
        cx.notify();
    }

    fn search_reviewers(&mut self, query: String, cx: &mut Context<Self>) {
        let query = query.trim().to_string();
        self.reviewers_generation += 1;
        let serial = self.reviewers_generation;
        self.reviewers_results.clear();
        self.reviewers_error = None;
        self.reviewers_loading = !query.is_empty();
        cx.notify();
        if query.is_empty() {
            return;
        }
        let repository = self
            .selected
            .as_ref()
            .map(|s| s.repository.clone())
            .unwrap_or_default();
        cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move {
                    crate::pull_requests::search_users(&repository, &query)
                        .map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.reviewers_generation != serial || !view.reviewers_open {
                    return;
                }
                view.reviewers_loading = false;
                match results {
                    Ok(users) => {
                        view.reviewers_results = users
                            .into_iter()
                            .filter(|user| {
                                view.selected
                                    .as_ref()
                                    .is_none_or(|pr| pr.author != user.login)
                            })
                            .collect()
                    }
                    Err(error) => view.reviewers_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn toggle_reviewer(&mut self, login: String, cx: &mut Context<Self>) {
        // The reference sends the review request as soon as a user is chosen:
        // the popover has no confirm button.
        self.reviewers_selected.clear();
        self.reviewers_selected.insert(login);
        self.submit_reviewers(cx);
    }

    pub fn begin_comment_edit(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let Some(comment) = detail.comment(&id).filter(|comment| comment.can_edit) else {
            return;
        };
        let body = comment.body.clone();
        let mode = self.mode;
        let editor = cx.new(|cx| {
            let mut editor = FileEditor::prose(mode, "Edit pull request comment", cx);
            editor.set_accessible_name("Edit pull request comment");
            editor.set_text_silently(&body, cx);
            editor
        });
        cx.subscribe(&editor, |_, _, _: &super::file_editor::EditorEvent, cx| {
            cx.notify()
        })
        .detach();
        self.comment_edit = Some((id, editor));
        self.comment_menu = None;
        cx.notify();
    }

    pub fn cancel_comment_edit(&mut self, cx: &mut Context<Self>) {
        self.comment_edit = None;
        cx.notify();
    }

    pub fn begin_reply(
        &mut self,
        thread: Option<String>,
        quote: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if thread.is_none() {
            self.comment_box.update(cx, |editor, cx| {
                editor.set_text_silently(quote.as_deref().unwrap_or(""), cx)
            });
            self.comment_menu = None;
            self.detail_scroll
                .set_offset(gpui::point(gpui::px(0.0), gpui::px(-1_000_000.0)));
            cx.notify();
            return;
        }
        let mode = self.mode;
        let author = self
            .detail
            .as_ref()
            .and_then(|detail| thread.as_deref().and_then(|id| detail.thread(id)))
            .and_then(|thread| thread.comments.first())
            .map(|comment| comment.author.clone())
            .unwrap_or_default();
        let editor = cx.new(|cx| {
            let mut editor = FileEditor::prose(mode, "Pull request reply", cx);
            editor.set_accessible_name("Pull request reply");
            editor.set_placeholder(format!("Reply to {author}"), cx);
            editor.set_text_metrics(14.0, 28.0, cx);
            if let Some(quote) = quote {
                editor.set_text_silently(&quote, cx);
            }
            editor
        });
        cx.subscribe(&editor, |_, _, _: &super::file_editor::EditorEvent, cx| {
            cx.notify()
        })
        .detach();
        self.reply = Some((thread.clone(), editor));
        self.reply_target = thread;
        self.comment_menu = None;
        cx.notify();
    }

    pub fn cancel_reply(&mut self, cx: &mut Context<Self>) {
        self.reply = None;
        self.reply_target = None;
        cx.notify();
    }

    pub fn open_file(&mut self, path: String, line: Option<u32>, cx: &mut Context<Self>) {
        let Some(summary) = &self.selected else {
            return;
        };
        let sha = self.content_sha();
        if let Ok(mut url) = url::Url::parse(&format!(
            "https://github.com/{}/blob/{sha}/",
            summary.repository
        )) {
            if let Ok(mut segments) = url.path_segments_mut() {
                segments.pop_if_empty();
                segments.extend(path.split('/'));
            }
            if let Some(line) = line {
                url.set_fragment(Some(&format!("L{line}")));
            }
            cx.open_url(url.as_str());
        }
    }

    pub fn copy_path(&mut self, path: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.clone()));
        self.show_notice(format!("Copied {path}"), cx);
    }

    pub fn scroll_to_file(&mut self, path: String, cx: &mut Context<Self>) {
        self.selected_file = Some(path.clone());
        self.scrolled_to_file = Some(path);
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Diff interactions
    // ------------------------------------------------------------------

    pub fn toggle_file(&mut self, path: String, cx: &mut Context<Self>) {
        if !self.collapsed_files.remove(&path) {
            self.collapsed_files.insert(path);
        }
        cx.notify();
    }

    pub fn collapse_all(&mut self, cx: &mut Context<Self>) {
        let all: Vec<String> = self.diff.iter().map(|file| file.path.clone()).collect();
        if all.iter().all(|path| self.collapsed_files.contains(path)) {
            self.collapsed_files.clear();
        } else {
            self.collapsed_files.extend(all);
        }
        cx.notify();
    }

    fn content_sha(&self) -> String {
        match self.review_tab.as_ref().map(|tab| &tab.scope) {
            Some(ReviewScope::Commit(sha)) => sha.clone(),
            _ => self
                .detail
                .as_ref()
                .map(|detail| detail.head_sha.clone())
                .unwrap_or_default(),
        }
    }

    /// The view toggle cycles unified → split → auto → unified.
    pub fn toggle_split(&mut self, cx: &mut Context<Self>) {
        self.diff_layout = self.diff_layout.next();
        cx.notify();
    }

    /// Whether `file` renders side by side under the current layout.
    pub(super) fn file_splits(&self, file: &FileDiff) -> bool {
        self.diff_layout.splits(file)
    }

    pub fn toggle_wrap(&mut self, cx: &mut Context<Self>) {
        self.wrap = !self.wrap;
        cx.notify();
    }

    pub fn toggle_rich(&mut self, cx: &mut Context<Self>) {
        self.rich = !self.rich;
        if self.rich {
            let paths: Vec<_> = self
                .diff
                .iter()
                .filter(|file| file.path.ends_with(".md") && file.status != 'D')
                .map(|file| file.path.clone())
                .collect();
            for path in paths {
                self.load_file_lines(path, cx);
            }
        }
        cx.notify();
    }

    pub fn toggle_words(&mut self, cx: &mut Context<Self>) {
        self.words = !self.words;
        cx.notify();
    }

    pub fn toggle_file_tree(&mut self, cx: &mut Context<Self>) {
        self.file_tree_open = !self.file_tree_open;
        self.review_options_open = false;
        // The tree opens with the file at the top of the diff selected.
        if self.file_tree_open && self.selected_file.is_none() {
            self.selected_file = self.top_diff_file();
        }
        cx.notify();
    }

    /// A review tab separator's expand button: reveal `REVIEW_EXPANSION_LINES`
    /// more lines of the gap above hunk `hunk`, from its start (just below the
    /// previous hunk) or from its end (just above this one), loading the file
    /// first when needed.
    pub fn expand_gap(
        &mut self,
        file: usize,
        hunk: usize,
        from_start: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.diff.get(file).map(|file| file.path.clone()) else {
            return;
        };
        if !from_start {
            self.pending_expand_anchor = self
                .hunk_offset(file, hunk)
                .map(|offset| (file, hunk, offset, EXPAND_ANCHOR_FRAMES));
        }
        let entry = self
            .expanded_gaps
            .entry(format!("{path}:{hunk}"))
            .or_default();
        if from_start {
            entry.0 += REVIEW_EXPANSION_LINES;
        } else {
            entry.1 += REVIEW_EXPANSION_LINES;
        }
        if !self.file_lines.contains_key(&path) && !self.file_lines_loading.contains(&path) {
            self.load_file_lines(path, cx);
        }
        cx.notify();
    }

    /// Tracks the pointer over a tooltip trigger: its tooltip shows after
    /// `delay`, or at once while another one is open, as Radix skips the
    /// delay between neighbouring triggers.
    pub(super) fn set_tooltip_hover(
        &mut self,
        key: String,
        hovered: bool,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        if !hovered {
            if self.tooltip_hover.as_deref() == Some(key.as_str()) {
                self.tooltip_hover = None;
                self.tooltip = None;
                cx.notify();
            }
            return;
        }
        self.tooltip_hover = Some(key.clone());
        if self.tooltip.is_some() {
            self.tooltip = Some(key);
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |view, cx| {
                if view.tooltip_hover.as_deref() == Some(key.as_str()) {
                    view.tooltip = Some(key);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Pressing a trigger closes its tooltip until the pointer enters again.
    pub(super) fn dismiss_tooltip(&mut self, cx: &mut Context<Self>) {
        if self.tooltip_hover.take().is_some() | self.tooltip.take().is_some() {
            cx.notify();
        }
    }

    pub fn toggle_folder(&mut self, path: String, cx: &mut Context<Self>) {
        if !self.collapsed_folders.remove(&path) {
            self.collapsed_folders.insert(path);
        }
        cx.notify();
    }

    /// Lines fetched for a path, for the rich preview and review expansion.
    pub(super) fn context_lines(&self, path: &str) -> Option<&Vec<String>> {
        self.file_lines.get(path)
    }

    /// Loads one file's lines at the head commit for the rich preview and a
    /// review tab's expanded context.
    fn load_file_lines(&mut self, path: String, cx: &mut Context<Self>) {
        self.fetch_file_lines(path, true, cx);
    }

    /// A review tab reads each file it draws in full, as the reference does,
    /// so its separators can reveal lines and it can count those after the
    /// last hunk. A failure only leaves the separators as they are.
    pub(super) fn prefetch_review_file(&mut self, file: usize, cx: &mut Context<Self>) {
        let Some(file) = self.diff.get(file) else {
            return;
        };
        if file.binary || matches!(file.status, 'A' | 'D') || file.hunks.is_empty() {
            return;
        }
        let path = &file.path;
        if self.file_lines.contains_key(path)
            || self.file_lines_loading.contains(path)
            || self.file_errors.contains_key(path)
        {
            return;
        }
        self.fetch_file_lines(path.clone(), false, cx);
    }

    fn fetch_file_lines(&mut self, path: String, report: bool, cx: &mut Context<Self>) {
        let Some(summary) = self.selected.clone() else {
            return;
        };
        let sha = self.content_sha();
        let generation = self.file_generation;
        if sha.is_empty() {
            return;
        }
        self.file_errors.remove(&path);
        self.file_lines_loading.insert(path.clone());
        let client = self.client.clone();
        let path_call = path.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    client
                        .file_lines(&summary.repository, &path_call, &sha)
                        .map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.file_generation != generation {
                    return;
                }
                view.file_lines_loading.remove(&path);
                match result {
                    Ok(lines) => {
                        view.file_lines.insert(path.clone(), lines);
                    }
                    Err(error) => {
                        view.file_errors.insert(path.clone(), error.clone());
                        if report {
                            view.show_notice(error, cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn begin_inline_comment(
        &mut self,
        file: usize,
        path: String,
        line: u32,
        old: bool,
        cx: &mut Context<Self>,
    ) {
        let mode = self.mode;
        let editor = cx.new(|cx| {
            let mut editor = FileEditor::prose(mode, "Add a comment…", cx);
            editor.set_placeholder("Add a comment…", cx);
            editor.set_placeholder_opacity(0.5, cx);
            editor.set_accessible_name("Add a comment…");
            editor.set_text_metrics(14.0, 22.75, cx);
            editor
        });
        cx.subscribe(&editor, |_, _, _: &super::file_editor::EditorEvent, cx| {
            cx.notify()
        })
        .detach();
        self.inline_editor = Some(editor);
        self.inline_comment = Some(InlineComment {
            path,
            line,
            file,
            old,
        });
        cx.notify();
    }

    pub fn cancel_inline_comment(&mut self, cx: &mut Context<Self>) {
        self.inline_comment = None;
        self.inline_editor = None;
        cx.notify();
    }
}
