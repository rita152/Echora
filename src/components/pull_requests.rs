//! Pull Requests page: list, detail, diff, and review surfaces.
//!
//! Layout, colors, and copy are taken from the ChatGPT/Codex desktop app over
//! CDP; see `scripts/cdp_capture_pull_requests.mjs` and
//! `artifacts/pull-requests-reference/`. The page mirrors the reference
//! interaction tree item for item: list tabs, search, filter submenus, grouped
//! rows, the detail header with its nested buttons, the activity timeline, the
//! diff toolbar and file headers, the right-hand file tree, and the review tab.

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
    control_bounds: std::rc::Rc<
        std::cell::RefCell<std::collections::HashMap<String, gpui::Bounds<gpui::Pixels>>>,
    >,

    // List pane.
    tab: ListTab,
    filter: PullRequestFilter,
    repositories: Vec<String>,
    groups: Vec<PullRequestGroup>,
    list_loading: bool,
    list_error: Option<String>,
    query: String,
    search: Entity<PromptInput>,
    collapsed_groups: HashSet<GroupKind>,
    selected: Option<PullRequestSummary>,
    list_menu: Option<ListMenu>,
    filter_submenu: Option<FilterSubmenu>,
    filter_loading: bool,

    // Detail pane.
    detail: Option<PullRequestDetail>,
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
    collapsed_comments: HashSet<String>,
    checks_expanded: bool,
    activity_expanded: bool,
    commits_expanded: bool,

    // Diff surfaces.
    diff: Vec<FileDiff>,
    diff_loading: bool,
    diff_error: Option<String>,
    collapsed_files: HashSet<String>,
    split: bool,
    wrap: bool,
    rich: bool,
    words: bool,
    review_options_open: bool,
    scope_menu_open: bool,
    file_tree_open: bool,
    tree_filter: Entity<PromptInput>,
    collapsed_folders: HashSet<String>,
    selected_file: Option<String>,
    expanded_context: HashSet<String>,
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
    code_width: f32,
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
            let mut input = PromptInput::inline_other(mode, "Search pull requests", false, cx);
            input.set_accessible_name("Search pull requests");
            input
        });
        let tree_filter = cx.new(|cx| {
            let mut input = PromptInput::inline_other(mode, "Filter files…", false, cx);
            input.set_accessible_name("Filter files");
            input
        });
        let comment_box = cx.new(|cx| {
            let mut editor = FileEditor::prose(mode, "Pull request comment", cx);
            // The reference's composer shows `Leave a comment` while keeping the
            // `Pull request comment` accessible name.
            editor.set_placeholder("Leave a comment", cx);
            editor.set_accessible_name("Pull request comment");
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
                this.query = input.read(cx).text().to_string();
                cx.notify();
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
            pane_width: 646.0,
            list_width: theme::LIST_PANE_WIDTH,
            control_bounds: Default::default(),
            tab: ListTab::All,
            filter: PullRequestFilter::default(),
            repositories: Vec::new(),
            groups: Vec::new(),
            list_loading: true,
            list_error: None,
            query: String::new(),
            search,
            collapsed_groups: HashSet::new(),
            selected: None,
            list_menu: None,
            filter_submenu: None,
            filter_loading: false,
            detail: None,
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
            checks_expanded: true,
            activity_expanded: true,
            commits_expanded: true,
            diff: Vec::new(),
            diff_loading: false,
            diff_error: None,
            collapsed_files: HashSet::new(),
            split: false,
            // The reference diff view opens with word wrap on, which is why its
            // toolbar offers `Disable word wrap`.
            wrap: true,
            rich: false,
            words: false,
            review_options_open: false,
            scope_menu_open: false,
            file_tree_open: false,
            tree_filter,
            collapsed_folders: HashSet::new(),
            selected_file: None,
            expanded_context: HashSet::new(),
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
    pub(super) fn set_code_width(&mut self, width: f32) {
        self.code_width = width;
    }

    pub(super) fn code_width(&self) -> f32 {
        self.code_width
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
        if self.capture_intent.is_some() || self.pending_detail_scroll.is_some() {
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
            return !self.diff.is_empty();
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
        cx.notify();
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
                "split" => self.split = true,
                "collapse-all" => {
                    let all: Vec<String> = self.diff.iter().map(|file| file.path.clone()).collect();
                    self.collapsed_files.extend(all);
                }
                "review-options" => self.review_options_open = true,
                "filter-menu" => self.list_menu = Some(ListMenu::Filter),
                "filter-status" => {
                    self.list_menu = Some(ListMenu::Filter);
                    self.filter_submenu = Some(FilterSubmenu::Status);
                }
                "filter-repository" => {
                    self.list_menu = Some(ListMenu::Filter);
                    self.filter_submenu = Some(FilterSubmenu::Repository);
                }
                "inline-comment" => {
                    if let Some(file) = self.diff.first() {
                        let path = file.path.clone();
                        let line = file
                            .hunks
                            .first()
                            .and_then(|hunk| hunk.lines.iter().find_map(|line| line.new))
                            .unwrap_or(1);
                        self.begin_inline_comment(0, path, line, false, cx);
                    }
                }
                _ => {}
            }
            cx.notify();
        }
        let Some((tab, file_tree)) = self.capture_intent.take() else {
            return;
        };
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

    // ------------------------------------------------------------------
    // Data loading
    // ------------------------------------------------------------------

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let client = self.client.clone();
        let tab = self.tab;
        let filter = self.filter.clone();
        self.list_loading = true;
        self.list_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    client
                        .list(tab, &filter)
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
                        view.groups = groups;
                        view.list_error = None;
                        view.reload_repositories();
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
            let result = cx
                .background_executor()
                .spawn(async move {
                    client
                        .detail(&repository, number)
                        .map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.detail_generation != generation {
                    return;
                }
                view.detail_loading = false;
                match result {
                    Ok(detail) => {
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
        self.query.clear();
        cx.notify();
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
            crate::pull_requests::filter_groups(&self.groups, &self.filter, &self.query)
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
                                crate::pull_requests::filter_groups(
                                    &view.groups,
                                    &view.filter,
                                    &view.query,
                                )
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
        self.expanded_context.clear();
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
        cx.notify();
    }

    pub fn toggle_scope_menu(&mut self, cx: &mut Context<Self>) {
        self.scope_menu_open = !self.scope_menu_open;
        self.review_options_open = false;
        cx.notify();
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

    pub fn toggle_commits(&mut self, cx: &mut Context<Self>) {
        self.commits_expanded = !self.commits_expanded;
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
        let editor = cx.new(|cx| {
            let mut editor = FileEditor::prose(mode, "Pull request reply", cx);
            editor.set_accessible_name("Pull request reply");
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

    pub fn toggle_split(&mut self, cx: &mut Context<Self>) {
        self.split = !self.split;
        cx.notify();
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
        cx.notify();
    }

    pub fn toggle_folder(&mut self, path: String, cx: &mut Context<Self>) {
        if !self.collapsed_folders.remove(&path) {
            self.collapsed_folders.insert(path);
        }
        cx.notify();
    }

    /// Expands an `N unmodified lines` bar: fetch the file at the head commit
    /// (once per path) and then reveal the lines it hides.
    pub fn expand_context(&mut self, file: usize, key: String, cx: &mut Context<Self>) {
        let Some(path) = self.diff.get(file).map(|file| file.path.clone()) else {
            return;
        };
        self.expanded_context.insert(key.clone());
        if self.file_lines.contains_key(&path) || self.file_lines_loading.contains(&path) {
            cx.notify();
            return;
        }
        self.load_file_lines(path, cx);
        cx.notify();
    }

    /// Lines fetched for a path, when an expander needs them.
    pub(super) fn context_lines(&self, path: &str) -> Option<&Vec<String>> {
        self.file_lines.get(path)
    }

    /// Loads one file's lines at the head commit (shared by the context
    /// expanders and the activity code previews).
    fn load_file_lines(&mut self, path: String, cx: &mut Context<Self>) {
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
                        view.expanded_context
                            .retain(|key| !key.starts_with(&format!("{path}:")));
                        view.show_notice(error, cx);
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
            let mut editor = FileEditor::prose(mode, "Request change", cx);
            editor.set_accessible_name("Request change");
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
