use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    process::Command,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    Animation, AnimationExt, App, Bounds, ContentMask, Context, Div, Entity, Focusable, Hsla,
    IntoElement, MouseButton, MouseDownEvent, Pixels, Render, ScrollHandle, ShapedLine,
    SharedString, TextAlign, TextRun, Transformation, Window, canvas, deferred, div, point,
    prelude::*, px, radians, size,
};

use crate::{
    agent::{
        AgentCapability, AgentThreadActiveFlag, Project, ProjectId, ThreadActivity, ThreadId,
        ThreadSummary, UpdateProject,
    },
    components::{
        account::AccountView,
        icons::{chevron, icon},
        prompt_input::{PromptInput, PromptSubmitted},
    },
    git_review::ProjectRepo,
    theme::{Theme, ThemeMode},
    workspace::{WorkspaceSnapshot, WorkspaceStore, project_id_for_thread},
};

pub struct OpenSettings;
pub struct OpenProjectCreation;
/// The sidebar `Pull requests` entry asks the host to open the Pull Requests
/// page in the main content area.
pub struct OpenPullRequests;
/// The sidebar search button asks the host to open the chat search dialog.
pub struct OpenChatSearch;

/// Actions the account surfaces ask the application to perform. Requests that
/// need a manager RPC are never issued from a view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountIntent {
    /// account/read followed by account/rateLimits/read.
    Refresh,
    StartLogin,
    CancelLogin(String),
    /// Opens the logout confirmation.
    RequestLogout,
    /// Opens a backend-provided URL in the system browser.
    OpenExternalUrl(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountAction(pub AccountIntent);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectThread {
    pub thread_id: ThreadId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewConversation {
    pub project_id: Option<ProjectId>,
    pub cwd: PathBuf,
}

impl gpui::EventEmitter<OpenSettings> for SidebarView {}
impl gpui::EventEmitter<OpenPullRequests> for SidebarView {}
impl gpui::EventEmitter<AccountAction> for SidebarView {}
impl gpui::EventEmitter<OpenProjectCreation> for SidebarView {}
impl gpui::EventEmitter<OpenChatSearch> for SidebarView {}
impl gpui::EventEmitter<SelectThread> for SidebarView {}
impl gpui::EventEmitter<NewConversation> for SidebarView {}

/// Project and working directory for a new conversation. Selecting a project in
/// the sidebar drives both; without one the process working directory is used.
fn new_conversation_target(project: Option<&Project>) -> (Option<ProjectId>, PathBuf) {
    let project_id = project.map(|project| project.project_id.clone());
    let cwd = project
        .and_then(|project| project.roots.first().cloned())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    (project_id, cwd)
}

// These values come from the live ChatGPT desktop app at 127.0.0.1:9222.
/// Matches the reference shell's `--codex-sidebar-preferred-width` (275px).
pub(crate) const SIDEBAR_WIDTH: f32 = 275.0;
const SIDEBAR_TITLEBAR_SAFE_TOP: f32 = 46.0;
const ROW_HEIGHT: f32 = 30.0;
const ROW_RADIUS: f32 = 12.5;
const ROW_HORIZONTAL_PADDING: f32 = 8.0;
const SECTION_HEADER_HEIGHT: f32 = 25.0;
const MAX_VISIBLE_PROJECT_THREADS: usize = 5;
const MAX_VISIBLE_RECENTS: usize = 10;
const SIDEBAR_BODY_FONT_WEIGHT: gpui::FontWeight = crate::theme::UI_BODY_FONT_WEIGHT;
const MARQUEE_HOVER_DELAY: Duration = Duration::from_millis(350);
const MARQUEE_SPEED: f32 = 28.0;
/// Sidebar project hover card measured from the CDP reference capture
/// (`artifacts/project-hover-20260921/`). The card anchors to the row it
/// describes, not to the pointer: 320 px wide, 15 px radius, 8 px padding,
/// 6 px between sections, 4 px between rows, 24 px header, 20 px rows.
const PROJECT_HOVER_CARD_WIDTH: f32 = 320.0;
const PROJECT_HOVER_CARD_RADIUS: f32 = 15.0;
const PROJECT_HOVER_CARD_PADDING: f32 = 8.0;
const PROJECT_HOVER_CARD_GAP: f32 = 6.0;
const PROJECT_HOVER_ROW_GAP: f32 = 4.0;
const PROJECT_HOVER_ROW_HEIGHT: f32 = 20.0;
const PROJECT_HOVER_HEADER_HEIGHT: f32 = 24.0;
const PROJECT_HOVER_ICON_SLOT: f32 = 16.0;
const PROJECT_HOVER_ICON_GAP: f32 = 6.0;
const PROJECT_HOVER_INLINE_GAP: f32 = 8.0;
const PROJECT_HOVER_SECTION_PADDING: f32 = 6.0;
const PROJECT_HOVER_ROW_RADIUS: f32 = 10.0;
const PROJECT_HOVER_TRAILING_SLOT: f32 = 20.0;
const PROJECT_HOVER_CARD_OFFSET_X: f32 = 3.0;
const PROJECT_HOVER_CARD_OFFSET_Y: f32 = 1.0;
/// The reference opens the card 239 ms after the pointer enters the row.
const PROJECT_HOVER_CARD_DELAY: Duration = Duration::from_millis(240);
/// Grace period so a pointer crossing the 3 px gap into the card keeps it.
const PROJECT_HOVER_CARD_CLOSE_DELAY: Duration = Duration::from_millis(60);
/// Sidebar task hover card, measured from the live ChatGPT desktop app over CDP
/// (`artifacts/thread-hover-20260922/reference/`). It reuses the project card's
/// 320 px shell, 15 px radius, ring, and shadow, and prints two rows: the task
/// title with its environment icon and compact timestamp, then the project the
/// task is filed under.
const THREAD_HOVER_CARD_WIDTH: f32 = 320.0;
const THREAD_HOVER_CARD_RADIUS: f32 = 15.0;
const THREAD_HOVER_CARD_PADDING_X: f32 = 8.0;
const THREAD_HOVER_CARD_PADDING_Y: f32 = 6.0;
const THREAD_HOVER_CARD_GAP: f32 = 4.0;
const THREAD_HOVER_TITLE_ROW_HEIGHT: f32 = 21.0;
const THREAD_HOVER_ROW_HEIGHT: f32 = 20.0;
const THREAD_HOVER_TITLE_LINE_HEIGHT: f32 = 20.0;
const THREAD_HOVER_TITLE_INSET: f32 = 2.0;
const THREAD_HOVER_TITLE_PADDING_X: f32 = 6.0;
const THREAD_HOVER_TITLE_RADIUS: f32 = 10.0;
const THREAD_HOVER_TITLE_ICON_GAP: f32 = 4.0;
const THREAD_HOVER_TITLE_TRAILING_GAP: f32 = 12.0;
/// `items-baseline` puts the 12 px timestamp one pixel below the 14 px title.
const THREAD_HOVER_DURATION_OFFSET_Y: f32 = 1.0;
const THREAD_HOVER_ENV_ICON: f32 = 14.0;
const THREAD_HOVER_PROJECT_ICON: f32 = 16.0;
const THREAD_HOVER_PROJECT_GAP: f32 = 6.0;
/// The reference opens the task card 227-229 ms after the pointer enters.
const THREAD_HOVER_CARD_DELAY: Duration = Duration::from_millis(240);
const THREAD_HOVER_CARD_CLOSE_DELAY: Duration = Duration::from_millis(60);
const PROJECT_THREAD_TITLE_INSETS: f32 = 120.0;
const RECENT_THREAD_TITLE_INSETS: f32 = 96.0;
const THREAD_ACTION_RAIL_INSETS: f32 = 51.0;
/// Account surfaces measured from the live ChatGPT desktop app at 1440x900:
/// the sidebar row is 184x30 with an 18px avatar, the menu is 224 wide with 4px
/// padding, a 42.5625px account header, and 28.5625px rows.
const ACCOUNT_ROW_HEIGHT: f32 = 30.0;
const ACCOUNT_MENU_WIDTH: f32 = 224.0;
const ACCOUNT_MENU_ITEM_HEIGHT: f32 = 28.5625;
const ACCOUNT_HEADER_HEIGHT: f32 = 42.5625;
const ACCOUNT_HEADER_LINE: f32 = 18.5714;
const ACCOUNT_AVATAR_SIZE: f32 = 18.0;
const ACCOUNT_MENU_BOTTOM: f32 = 44.625;
const TITLE_FADE_IN: f32 = 8.0;
const TITLE_FADE_OUT: f32 = 16.0;

fn account_avatar(initials: Option<&str>, theme: Theme) -> Div {
    div()
        .size(px(ACCOUNT_AVATAR_SIZE))
        .flex_none()
        .rounded_full()
        .bg(theme.control)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(8.0))
        .text_color(theme.sidebar_text)
        .child(initials.unwrap_or("").to_owned())
}

/// One account menu row. Actions live on the caller so a row without a
/// supported backend action stays inert instead of faking success.
fn account_menu_row(
    id: &'static str,
    label: &'static str,
    glyph: &'static str,
    trailing: Option<String>,
    theme: Theme,
    enabled: bool,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .h(px(ACCOUNT_MENU_ITEM_HEIGHT))
        .px(px(8.0))
        .rounded(px(12.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(13.0))
        .line_height(px(18.5714))
        .text_color(theme.sidebar_text)
        .when(enabled, |row| {
            row.cursor_pointer()
                .hover(move |style| style.bg(theme.sidebar_hover))
        })
        .child(icon(glyph, theme.sidebar_text.into()).size(px(16.0)))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(label),
        )
        .when_some(trailing, |row, trailing| {
            row.child(
                div()
                    .flex_none()
                    .text_size(px(13.0))
                    .text_color(theme.sidebar_text_muted)
                    .child(trailing),
            )
        })
}

fn account_menu_separator(theme: Theme) -> Div {
    div()
        .h(px(9.0))
        .px(px(4.0))
        .flex()
        .items_center()
        .child(div().h(px(1.0)).w_full().bg(theme.border))
}

/// The reference shortens a project path exactly once: a leading
/// `/Users/<name>` becomes `~`, everything else stays absolute.
fn home_shortened_path(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    let mut parts = text.splitn(4, '/');
    let (empty, users, account) = (parts.next(), parts.next(), parts.next());
    match (empty, users, account) {
        (Some(""), Some("Users"), Some(account)) if !account.is_empty() => match parts.next() {
            Some(rest) if !rest.is_empty() => format!("~/{rest}"),
            _ => "~".to_owned(),
        },
        _ => text.into_owned(),
    }
}

/// Path rows the card shows for a project: each project root with the home
/// prefix shortened, de-duplicated, and never repeating the project label or
/// the repository row the reference already printed.
fn project_hover_paths(project: &Project, repo: Option<&ProjectRepo>) -> Vec<(String, PathBuf)> {
    let label = project.name.trim().to_lowercase();
    let mut seen = HashSet::new();
    if let Some(repo) = repo {
        seen.insert(repo.label.trim().to_lowercase());
    }
    let mut paths = Vec::new();
    for root in &project.roots {
        let display = home_shortened_path(root);
        let key = display.trim().to_lowercase();
        if key.is_empty() || key == label || !seen.insert(key) {
            continue;
        }
        paths.push((display, root.clone()));
    }
    paths
}

/// Compact "time ago" label the reference prints beside a task title, matching
/// ChatGPT's `compactMinutesAgo`/`compactHoursAgo`/... formatter: whole minutes
/// under an hour, whole hours under a day, then calendar days, weeks, months,
/// and years.
fn compact_relative_time(now_ms: i64, then_ms: i64) -> String {
    const MINUTE_MS: i64 = 60_000;
    let minutes = ((now_ms - then_ms) / MINUTE_MS).max(1);
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = calendar_days_between(now_ms, then_ms).max(1);
    if days < 7 {
        return format!("{days}d");
    }
    if days < 30 {
        return format!("{}w", days / 7);
    }
    if days < 365 {
        return format!("{}mo", days / 30);
    }
    format!("{}y", days / 365)
}

/// Whole calendar days between two instants in local time, which is the day
/// bucket the reference compares once a task is more than a day old.
fn calendar_days_between(now_ms: i64, then_ms: i64) -> i64 {
    use chrono::{Local, TimeZone};
    let day = |ms: i64| {
        Local
            .timestamp_millis_opt(ms)
            .single()
            .map(|moment| moment.date_naive())
    };
    match (day(now_ms), day(then_ms)) {
        (Some(now), Some(then)) => (now - then).num_days(),
        _ => (now_ms - then_ms) / 86_400_000,
    }
}

/// The timestamp the card prints: the reference prefers the task's recency, then
/// its last update, then its creation. The app-server reports those in epoch
/// seconds; the formatter works in milliseconds.
fn thread_hover_timestamp_ms(thread: &ThreadSummary) -> i64 {
    thread.recency_at.unwrap_or(thread.updated_at) * 1_000
}

/// The reference paints both sidebar hover cards with
/// `bg-surface-elevated-secondary/90`, so what the user sees is that color
/// resolved against the pane behind the card. GPUI composites a deferred
/// overlay against the window backdrop rather than against the pane the card
/// overhangs, so the card carries the resolved color instead of the alpha.
fn hover_card_surface(theme: Theme) -> gpui::Rgba {
    let over = theme.project_hover_surface;
    let under = theme.surface;
    let (a, rest) = (over.a, 1.0 - over.a);
    gpui::Rgba {
        r: over.r * a + under.r * rest,
        g: over.g * a + under.g * rest,
        b: over.b * a + under.b * rest,
        a: 1.0,
    }
}

/// The reference resolves CJK runs through CoreText's system cascade, which
/// lands on the hidden `.PingFang UI` face and its 0.9587em ideographs; the
/// shared theme font pins the public `PingFang SC` instead, whose full 1em
/// advance makes every mixed Chinese/English line about 2% too wide. The card
/// therefore names the hidden family: CoreText hands CJK on to its own cascade
/// (the CDP font probe reports `.PingFangUIDisplaySC-Default` for the reference
/// title), and the measured title advance drops from 245 px to the reference's
/// 241 px.
const HOVER_CARD_CJK_FALLBACK: &str = ".PingFang UI SC";

/// Font for every run the hover cards print, so a Chinese project or task title
/// keeps the reference's advances instead of the theme's wider public fallback.
fn hover_card_font(weight: gpui::FontWeight) -> gpui::Font {
    gpui::Font {
        family: crate::theme::UI_FONT_FAMILY.into(),
        features: Default::default(),
        fallbacks: Some(gpui::FontFallbacks::from_fonts(vec![
            HOVER_CARD_CJK_FALLBACK.to_owned(),
        ])),
        weight,
        style: gpui::FontStyle::Normal,
    }
}

/// The reference's inline summary row: the task count, then one entry per
/// attention bucket that has tasks, separated by middle dots.
fn project_hover_summary(threads: &[&ThreadSummary]) -> String {
    let mut parts = vec![if threads.len() == 1 {
        crate::i18n::format!("{count} 个任务" => "{count}\u{a0}task", count = threads.len())
    } else {
        crate::i18n::format!("{count} 个任务" => "{count}\u{a0}tasks", count = threads.len())
    }];
    let waiting = threads
        .iter()
        .filter(|thread| thread_is_waiting(thread))
        .count();
    let running = threads
        .iter()
        .filter(|thread| thread_is_running(thread))
        .count();
    if waiting > 0 {
        parts.push(crate::i18n::format!(
            "{count} 个等待输入" => "{count}\u{a0}waiting",
            count = waiting
        ));
    }
    if running > 0 {
        parts.push(crate::i18n::format!(
            "{count} 个进行中" => "{count}\u{a0}active",
            count = running
        ));
    }
    parts.join(" · ")
}

fn thread_is_waiting(thread: &ThreadSummary) -> bool {
    matches!(
        &thread.activity,
        ThreadActivity::Active { flags } if flags
            .iter()
            .any(|flag| matches!(flag, AgentThreadActiveFlag::WaitingOnApproval | AgentThreadActiveFlag::WaitingOnUserInput))
    )
}

fn thread_is_running(thread: &ThreadSummary) -> bool {
    matches!(&thread.activity, ThreadActivity::Active { flags } if flags.is_empty())
}

/// Account action failures stay visible on the surfaces that can retry them.
fn account_menu_notice(message: String, theme: Theme) -> gpui::Stateful<Div> {
    div()
        .id("account-menu-notice")
        .w_full()
        .px(px(8.0))
        .py(px(4.0))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .text_color(theme.sidebar_text_muted)
        .child(message)
}

fn sidebar_thread_title_viewport_width(width: f32, flat: bool, show_actions: bool) -> f32 {
    let action_insets = if flat {
        RECENT_THREAD_TITLE_INSETS
    } else {
        PROJECT_THREAD_TITLE_INSETS
    };
    let insets = if show_actions {
        action_insets
    } else {
        action_insets - THREAD_ACTION_RAIL_INSETS
    };
    (width - insets).max(0.0)
}

fn faded_sidebar_text_color(color: Hsla, fade: f32) -> Hsla {
    color.alpha(color.a * fade)
}

fn thread_title_canvas(
    title: SharedString,
    color: Hsla,
    scroll_offset: f32,
    overflows: bool,
) -> impl IntoElement {
    canvas(
        move |_, window, _| {
            let mut font = window.text_style().font();
            font.family = ".SystemUIFont".into();
            font.weight = SIDEBAR_BODY_FONT_WEIGHT;
            let shape = |alpha: f32| {
                window.text_system().shape_line(
                    title.clone(),
                    px(14.0),
                    &[TextRun {
                        len: title.len(),
                        font: font.clone(),
                        color: faded_sidebar_text_color(color, alpha),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
            };
            let opaque = shape(1.0);
            let left = overflows.then(|| {
                (0..TITLE_FADE_IN as usize)
                    .map(|index| shape((index as f32 + 0.5) / TITLE_FADE_IN))
                    .collect::<Vec<_>>()
            });
            let right = overflows.then(|| {
                (0..TITLE_FADE_OUT as usize)
                    .map(|index| shape(1.0 - (index as f32 + 0.5) / TITLE_FADE_OUT))
                    .collect::<Vec<_>>()
            });
            (opaque, left, right)
        },
        move |bounds, (opaque, left, right): (ShapedLine, _, _), window, cx| {
            let origin = point(
                bounds.origin.x + px(TITLE_FADE_IN - scroll_offset),
                bounds.origin.y,
            );
            let paint =
                |line: &ShapedLine, mask: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
                    window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                        line.paint(origin, px(20.0), TextAlign::Left, None, window, cx)
                            .expect("sidebar title glyphs should paint")
                    });
                };
            if let (Some(left), Some(right)) = (left.as_ref(), right.as_ref()) {
                let center_left = bounds.origin.x + px(TITLE_FADE_IN);
                let center_right = bounds.right() - px(TITLE_FADE_OUT);
                if center_right > center_left {
                    paint(
                        &opaque,
                        Bounds::from_corners(
                            point(center_left, bounds.origin.y),
                            point(center_right, bounds.bottom()),
                        ),
                        window,
                        cx,
                    );
                }
                for (index, line) in left.iter().enumerate() {
                    let x = bounds.origin.x + px(index as f32);
                    paint(
                        line,
                        Bounds::new(point(x, bounds.origin.y), size(px(1.0), bounds.size.height)),
                        window,
                        cx,
                    );
                }
                for (index, line) in right.iter().enumerate() {
                    let x = bounds.right() - px(TITLE_FADE_OUT) + px(index as f32);
                    paint(
                        line,
                        Bounds::new(point(x, bounds.origin.y), size(px(1.0), bounds.size.height)),
                        window,
                        cx,
                    );
                }
            } else {
                paint(&opaque, bounds, window, cx);
            }
        },
    )
    .h_full()
    .min_w(px(0.0))
    .ml(px(-TITLE_FADE_IN))
    .flex_1()
}

fn marquee_offset(scroll_distance: f32, elapsed: Duration, reduce_motion: bool) -> f32 {
    if reduce_motion || scroll_distance <= 0.0 || elapsed <= MARQUEE_HOVER_DELAY {
        return 0.0;
    }
    let travel_time = scroll_distance / MARQUEE_SPEED;
    let progress = ((elapsed - MARQUEE_HOVER_DELAY).as_secs_f32() / travel_time).clamp(0.0, 1.0);
    scroll_distance * marquee_ease(progress)
}

fn marquee_duration(scroll_distance: f32) -> Duration {
    MARQUEE_HOVER_DELAY + Duration::from_secs_f32(scroll_distance.max(0.0) / MARQUEE_SPEED)
}

fn marquee_ease(progress: f32) -> f32 {
    fn bezier(t: f32, first: f32, second: f32) -> f32 {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
    }
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..12 {
        let parameter = (lower + upper) * 0.5;
        if bezier(parameter, 0.5, 0.7) < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    bezier((lower + upper) * 0.5, 0.6, 1.0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RenameTarget {
    Project(ProjectId),
    Thread(ThreadId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DeleteTarget {
    Project(ProjectId),
    Thread(ThreadId),
}

pub struct SidebarView {
    width: f32,
    mode: ThemeMode,
    store: Arc<WorkspaceStore>,
    snapshot: WorkspaceSnapshot,
    scroll: ScrollHandle,
    activity_scroll: ScrollHandle,
    scroll_to_bottom: bool,
    selected_project_id: Option<ProjectId>,
    selected_thread_id: Option<ThreadId>,
    hovered_thread_id: Option<ThreadId>,
    hovered_project_id: Option<ProjectId>,
    /// Bounds of every rendered project row, recorded while prepainting so the
    /// hover card can anchor to the row instead of the pointer.
    project_row_bounds: Rc<RefCell<HashMap<ProjectId, Bounds<Pixels>>>>,
    /// Bounds of every rendered task row, recorded the same way so the task
    /// hover card anchors to its row rather than to the pointer.
    thread_row_bounds: Rc<RefCell<HashMap<ThreadId, Bounds<Pixels>>>>,
    /// Project whose hover card is on screen, plus whether the pointer sits
    /// inside the card (the row and the card are one hover region).
    project_hover_card: Option<ProjectId>,
    project_hover_card_hovered: bool,
    /// Task whose hover card is on screen, plus whether the pointer sits inside
    /// the card. Only tasks that belong to a project carry one: the reference
    /// suppresses the card for projectless tasks rather than printing a card
    /// without its project row.
    thread_hover_card: Option<ThreadId>,
    thread_hover_card_hovered: bool,
    /// Repository row content, resolved once per project from local Git.
    project_repos: HashMap<ProjectId, Option<ProjectRepo>>,
    project_repos_pending: HashSet<ProjectId>,
    /// Capture request that arrived before the workspace listed its projects.
    pending_project_hover_card: Option<String>,
    /// Capture request that arrived before the workspace listed the task.
    pending_thread_hover_card: Option<String>,
    hovered_section_id: Option<&'static str>,
    marquee_started_at: Option<Instant>,
    marquee_animation_ends_at: Option<Instant>,
    marquee_animation_running: bool,
    show_all_projects: BTreeSet<ProjectId>,
    rename_input: Entity<PromptInput>,
    rename_target: Option<RenameTarget>,
    rename_focus_pending: bool,
    project_menu_id: Option<ProjectId>,
    thread_menu_id: Option<ThreadId>,
    menu_origin: (f32, f32),
    delete_confirmation: Option<DeleteTarget>,
    projects_section_menu_open: bool,
    pinned_menu_open: bool,
    profile_menu_open: bool,
    account: AccountView,
    activity_open: bool,
    archived_open: bool,
    /// True while the main content area shows the Pull Requests page.
    pull_requests_open: bool,
    local_error: Option<String>,
}

impl SidebarView {
    pub fn new(
        mode: ThemeMode,
        scroll_to_bottom: bool,
        store: Arc<WorkspaceStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = store.snapshot();
        let rename_input = cx.new(|cx| PromptInput::inline_other(mode, "名称", false, cx));
        cx.subscribe(&rename_input, |this, _, event: &PromptSubmitted, cx| {
            this.commit_rename(event.0.clone(), cx);
        })
        .detach();
        let receiver = store.subscribe();
        cx.spawn(async move |this, cx| {
            while let Ok(snapshot) = receiver.recv().await {
                let _ = this.update(cx, |this, cx| {
                    this.snapshot = snapshot;
                    if let Some(project) = this.pending_project_hover_card.clone() {
                        this.pending_project_hover_card = None;
                        this.open_project_hover_card_for_capture(&project, cx);
                    }
                    if let Some(thread) = this.pending_thread_hover_card.clone() {
                        this.pending_thread_hover_card = None;
                        this.open_thread_hover_card_for_capture(&thread, cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        Self {
            width: SIDEBAR_WIDTH,
            mode,
            store,
            snapshot,
            scroll: ScrollHandle::new(),
            activity_scroll: ScrollHandle::new(),
            scroll_to_bottom,
            selected_project_id: None,
            selected_thread_id: None,
            hovered_thread_id: None,
            hovered_project_id: None,
            project_row_bounds: Rc::new(RefCell::new(HashMap::new())),
            thread_row_bounds: Rc::new(RefCell::new(HashMap::new())),
            project_hover_card: None,
            project_hover_card_hovered: false,
            thread_hover_card: None,
            thread_hover_card_hovered: false,
            project_repos: HashMap::new(),
            project_repos_pending: HashSet::new(),
            pending_project_hover_card: None,
            pending_thread_hover_card: None,
            hovered_section_id: None,
            marquee_started_at: None,
            marquee_animation_ends_at: None,
            marquee_animation_running: false,
            show_all_projects: BTreeSet::new(),
            rename_input,
            rename_target: None,
            rename_focus_pending: false,
            project_menu_id: None,
            thread_menu_id: None,
            menu_origin: (0.0, 0.0),
            delete_confirmation: None,
            projects_section_menu_open: false,
            pinned_menu_open: false,
            profile_menu_open: false,
            account: AccountView::default(),
            activity_open: false,
            archived_open: false,
            pull_requests_open: false,
            local_error: None,
        }
    }

    /// Highlights the `Pull requests` row while that page owns the main area.
    pub fn set_pull_requests_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.pull_requests_open == open {
            return;
        }
        self.pull_requests_open = open;
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.rename_input
            .update(cx, |input, cx| input.set_mode(mode, cx));
        cx.notify();
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if (self.width - width).abs() > f32::EPSILON {
            self.width = width;
            cx.notify();
        }
    }

    /// Account surfaces render exactly what the connection snapshot reports.
    pub fn set_account_view(&mut self, account: AccountView, cx: &mut Context<Self>) {
        if self.account != account {
            self.account = account;
            cx.notify();
        }
    }

    pub fn set_profile_menu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.close_transient_menus(cx);
        self.profile_menu_open = open;
        cx.notify();
    }

    pub fn set_activity_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.close_transient_menus(cx);
        self.activity_open = open;
        self.archived_open = false;
        cx.notify();
    }

    pub fn set_activity_scroll_for_capture(&mut self, offset: f32, cx: &mut Context<Self>) {
        self.activity_scroll.set_offset(point(px(0.0), px(-offset)));
        cx.notify();
    }

    pub fn set_activity_hovered_thread_for_capture(
        &mut self,
        thread_id: ThreadId,
        cx: &mut Context<Self>,
    ) {
        self.hovered_thread_id = self
            .snapshot
            .recent_threads
            .iter()
            .any(|thread| thread.thread_id == thread_id)
            .then_some(thread_id);
        cx.notify();
    }

    pub fn open_projects_section_menu_for_capture(&mut self, cx: &mut Context<Self>) {
        self.projects_section_menu_open = true;
        cx.notify();
    }

    pub fn close_transient_menus(&mut self, cx: &mut Context<Self>) {
        self.project_menu_id = None;
        self.thread_menu_id = None;
        self.projects_section_menu_open = false;
        self.pinned_menu_open = false;
        self.profile_menu_open = false;
        self.delete_confirmation = None;
        cx.notify();
    }

    #[cfg(test)]
    pub fn projects_section_menu_is_open(&self) -> bool {
        self.projects_section_menu_open
    }

    #[cfg(test)]
    pub fn project_menu_is_open(&self) -> bool {
        self.project_menu_id.is_some()
    }

    #[cfg(test)]
    pub fn pinned_menu_is_open(&self) -> bool {
        self.pinned_menu_open
    }

    pub fn open_project_menu_for_capture(&mut self, project_id: ProjectId, cx: &mut Context<Self>) {
        if self
            .snapshot
            .projects
            .iter()
            .any(|project| project.project_id == project_id)
        {
            self.project_menu_id = Some(project_id);
            self.menu_origin = (172.0, 160.0);
            cx.notify();
        }
    }

    /// Opens the project hover card without a pointer so the screenshot path
    /// can capture the same surface a real hover produces. `project` matches
    /// the project name first and its stable id second.
    pub fn open_project_hover_card_for_capture(&mut self, project: &str, cx: &mut Context<Self>) {
        let project_id = self
            .snapshot
            .projects
            .iter()
            .find(|candidate| candidate.name == project || candidate.project_id == project)
            .map(|project| project.project_id.clone());
        let Some(project_id) = project_id else {
            // The workspace may not have listed its projects yet.
            self.pending_project_hover_card = Some(project.to_owned());
            return;
        };
        self.project_hover_card = Some(project_id.clone());
        self.ensure_project_repo(project_id, cx);
        cx.notify();
    }

    #[cfg(test)]
    pub fn project_hover_card_is_open(&self) -> bool {
        self.project_hover_card.is_some()
    }

    /// Opens the task hover card without a pointer so the screenshot path can
    /// capture the same surface a real hover produces. `thread` matches the
    /// task title first and its stable id second.
    pub fn open_thread_hover_card_for_capture(&mut self, thread: &str, cx: &mut Context<Self>) {
        let thread_id = self
            .snapshot
            .recent_threads
            .iter()
            .find(|candidate| candidate.title == thread || candidate.thread_id == thread)
            .map(|thread| thread.thread_id.clone());
        let Some(thread_id) = thread_id else {
            // The workspace may not have listed its tasks yet.
            self.pending_thread_hover_card = Some(thread.to_owned());
            return;
        };
        if self.project_for_thread(&thread_id).is_none() {
            return;
        }
        self.thread_hover_card = Some(thread_id);
        cx.notify();
    }

    #[cfg(test)]
    pub fn thread_hover_card_is_open(&self) -> bool {
        self.thread_hover_card.is_some()
    }

    /// The project a task row's card belongs to. The reference only prints a
    /// task card when it can name the project, which is why projectless tasks
    /// in the Recents section show no card at all.
    fn project_for_thread(&self, thread_id: &str) -> Option<Project> {
        let thread = self.snapshot.thread(thread_id)?;
        let project_id = project_id_for_thread(thread, &self.snapshot.projects)?;
        self.snapshot
            .projects
            .iter()
            .find(|project| project.project_id == project_id)
            .cloned()
    }

    /// The pointer entered or left a project row. The card opens after the
    /// reference's 239 ms delay and stays open while either the row or the card
    /// itself is hovered.
    fn set_project_row_hovered(
        &mut self,
        project_id: ProjectId,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        self.hovered_project_id = hovered.then(|| project_id.clone());
        if hovered {
            self.schedule_project_hover_card(project_id, cx);
        } else {
            self.schedule_project_hover_card_close(cx);
        }
        cx.notify();
    }

    /// The pointer entered or left a task row. The card follows the same open
    /// and close schedule as the project card.
    fn set_thread_row_hovered(
        &mut self,
        thread_id: ThreadId,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        if hovered {
            self.schedule_thread_hover_card(thread_id, cx);
        } else {
            self.schedule_thread_hover_card_close(cx);
        }
    }

    fn schedule_thread_hover_card(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        if self.thread_hover_card.as_deref() == Some(thread_id.as_str())
            || self.project_for_thread(thread_id.as_str()).is_none()
        {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(THREAD_HOVER_CARD_DELAY)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.hovered_thread_id.as_deref() != Some(thread_id.as_str()) {
                    return;
                }
                this.thread_hover_card = Some(thread_id.clone());
                cx.notify();
            });
        })
        .detach();
    }

    /// Delayed so a pointer crossing from the row into the card never closes it.
    fn schedule_thread_hover_card_close(&mut self, cx: &mut Context<Self>) {
        if self.thread_hover_card.is_none() {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(THREAD_HOVER_CARD_CLOSE_DELAY)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.thread_hover_card_hovered
                    || this.hovered_thread_id.as_deref() == this.thread_hover_card.as_deref()
                {
                    return;
                }
                if this.thread_hover_card.take().is_some() {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn schedule_project_hover_card(&mut self, project_id: ProjectId, cx: &mut Context<Self>) {
        if self.project_hover_card.as_deref() == Some(project_id.as_str()) {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(PROJECT_HOVER_CARD_DELAY)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.hovered_project_id.as_deref() != Some(project_id.as_str()) {
                    return;
                }
                this.project_hover_card = Some(project_id.clone());
                this.ensure_project_repo(project_id, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Delayed so a pointer crossing from the row into the card never closes it.
    fn schedule_project_hover_card_close(&mut self, cx: &mut Context<Self>) {
        if self.project_hover_card.is_none() {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(PROJECT_HOVER_CARD_CLOSE_DELAY)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.project_hover_card_hovered || this.hovered_project_id.is_some() {
                    return;
                }
                if this.project_hover_card.take().is_some() {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The card's repository row comes from the local repository that owns the
    /// project root, so it is resolved once per project on first hover.
    fn ensure_project_repo(&mut self, project_id: ProjectId, cx: &mut Context<Self>) {
        if self.project_repos.contains_key(&project_id)
            || self.project_repos_pending.contains(&project_id)
        {
            return;
        }
        let Some(root) = self
            .snapshot
            .projects
            .iter()
            .find(|project| project.project_id == project_id)
            .and_then(|project| project.roots.first().cloned())
        else {
            return;
        };
        self.project_repos_pending.insert(project_id.clone());
        cx.spawn(async move |this, cx| {
            let repo = cx
                .background_executor()
                .spawn(async move { crate::git_review::project_repo(&root) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.project_repos_pending.remove(&project_id);
                this.project_repos.insert(project_id, repo);
                cx.notify();
            });
        })
        .detach();
    }

    /// Mirrors the visual state produced by a real sidebar thread selection
    /// without emitting a second navigation event. The capture resume path
    /// drives the matching conversation load directly from `ChatApp`.
    pub fn select_thread_for_capture(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.selected_thread_id = Some(thread_id);
        self.activity_open = false;
        self.archived_open = false;
        self.project_menu_id = None;
        self.thread_menu_id = None;
        cx.notify();
    }

    #[cfg(feature = "screenshot")]
    pub fn resumed_thread_ready_for_capture(&self, thread_id: &str) -> Result<bool, String> {
        if let Some(error) = &self.snapshot.error {
            return Err(
                crate::i18n::format!("侧栏数据加载失败：{error}" => "Could not load sidebar: {error}"),
            );
        }
        Ok(self.selected_thread_id.as_deref() == Some(thread_id)
            && (cfg!(test) || self.snapshot.thread(thread_id).is_some())
            && !self.snapshot.loading.projects
            && !self.snapshot.loading.recent
            && !self.snapshot.loading.pinned)
    }

    fn select_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.selected_thread_id = Some(thread_id.clone());
        self.activity_open = false;
        self.archived_open = false;
        self.project_menu_id = None;
        self.thread_menu_id = None;
        cx.emit(SelectThread { thread_id });
        cx.notify();
    }

    fn new_conversation(&mut self, project: Option<&Project>, cx: &mut Context<Self>) {
        let (project_id, cwd) = new_conversation_target(project);
        self.selected_project_id = project_id.clone();
        self.selected_thread_id = None;
        cx.emit(NewConversation { project_id, cwd });
        cx.notify();
    }

    /// Project and working directory the next conversation should use, derived
    /// from the sidebar's current selection. The chat search dialog asks for the
    /// same target instead of guessing from its own list.
    pub fn new_conversation_target(&self) -> (Option<ProjectId>, PathBuf) {
        let project = self.selected_project_id.as_deref().and_then(|project_id| {
            self.snapshot
                .projects
                .iter()
                .find(|project| project.project_id == project_id)
        });
        new_conversation_target(project)
    }

    fn start_rename(&mut self, target: RenameTarget, current: String, cx: &mut Context<Self>) {
        self.rename_target = Some(target);
        self.rename_input
            .update(cx, |input, cx| input.set_text_silently(current, cx));
        self.rename_focus_pending = true;
        self.project_menu_id = None;
        self.thread_menu_id = None;
        self.delete_confirmation = None;
        cx.notify();
    }

    fn commit_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(target) = self.rename_target.take() else {
            return;
        };
        match target {
            RenameTarget::Project(project_id) => self.store.update_project(
                project_id,
                UpdateProject {
                    name: Some(name),
                    roots: None,
                },
            ),
            RenameTarget::Thread(thread_id) => self.store.rename_thread(thread_id, name),
        }
        self.rename_input.update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    fn reveal_in_finder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        let result = Command::new("open").arg("-R").arg(path).spawn();
        #[cfg(not(target_os = "macos"))]
        let result = Command::new("xdg-open").arg(path).spawn();
        self.local_error = result
            .err()
            .map(|error| crate::i18n::format!("无法在 Finder 中显示：{error}" => "Could not reveal in Finder: {error}"));
        self.project_menu_id = None;
        cx.notify();
    }

    fn confirm_or_delete(&mut self, target: DeleteTarget, cx: &mut Context<Self>) {
        if self.delete_confirmation.as_ref() == Some(&target) {
            match target {
                DeleteTarget::Project(project_id) => self.store.delete_project(project_id),
                DeleteTarget::Thread(thread_id) => self.store.delete_thread(thread_id),
            }
            self.delete_confirmation = None;
            self.project_menu_id = None;
            self.thread_menu_id = None;
        } else {
            self.delete_confirmation = Some(target);
        }
        cx.notify();
    }

    fn nav_icon_button(
        id: impl Into<gpui::ElementId>,
        glyph: &'static str,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .size(px(24.0))
            .rounded(px(ROW_RADIUS))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(theme.sidebar_icon_muted)
            .hover(move |style| style.bg(theme.sidebar_hover).text_color(theme.sidebar_text))
            .child(icon(glyph, theme.sidebar_icon_muted.into()).size(px(16.0)))
    }

    fn action_icon_button(
        id: impl Into<gpui::ElementId>,
        glyph: &'static str,
        theme: Theme,
        pending: bool,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .w(px(19.0))
            .h(px(20.0))
            .rounded(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .when(pending, |button| button.opacity(0.4).cursor_default())
            .when(!pending, |button| {
                button.cursor_pointer().hover(move |style| {
                    style.bg(theme.sidebar_hover).text_color(theme.sidebar_text)
                })
            })
            .child(icon(glyph, theme.sidebar_icon_muted.into()).size(px(16.0)))
    }

    fn static_nav_row(
        id: &'static str,
        label: &'static str,
        glyph: &'static str,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .flex_none()
            .relative()
            .top(px(1.0))
            .h(px(30.0))
            .w_full()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(10.0))
            .text_size(px(14.0))
            .text_color(theme.sidebar_text)
            .hover(move |style| style.bg(theme.sidebar_hover))
            .child(icon(glyph, theme.sidebar_text.into()))
            .child(div().relative().left(px(0.25)).child(label))
            .child(div().flex_1())
    }

    /// Sidebar entry that switches the main content area to the Pull Requests
    /// page; the reference highlights the row while that page is open.
    pub(super) fn pull_requests_nav_row(
        &self,
        theme: Theme,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let view = cx.entity();
        div()
            .id("sidebar-pull-requests")
            .flex_none()
            .relative()
            .top(px(1.0))
            .h(px(30.0))
            .w_full()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(10.0))
            .text_size(px(14.0))
            .text_color(theme.sidebar_text)
            .cursor_pointer()
            .role(gpui::Role::Button)
            .aria_label("Pull requests")
            .when(selected, |row| row.bg(theme.sidebar_hover))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_click(move |_, _, cx| {
                view.update(cx, |_, cx| cx.emit(OpenPullRequests));
            })
            .child(icon("pull-request", theme.sidebar_text.into()))
            .child(
                div()
                    .relative()
                    .left(px(0.25))
                    .child(crate::i18n::text("拉取请求")),
            )
            .child(div().flex_1())
    }

    fn section_header(
        &self,
        id: &'static str,
        label: &'static str,
        collapsed: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let hovered = self.hovered_section_id == Some(id);
        div()
            .id(id)
            .h(px(SECTION_HEADER_HEIGHT))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(10.0))
            .cursor_pointer()
            .text_size(px(14.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(theme.sidebar_text_muted)
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(label)
                    .child(
                        icon("section-chevron", theme.sidebar_icon_muted.into())
                            .size(px(14.0))
                            .when(!hovered, |chevron| chevron.invisible())
                            .with_transformation(Transformation::rotate(radians(if collapsed {
                                -std::f32::consts::FRAC_PI_2
                            } else {
                                0.0
                            }))),
                    ),
            )
            .on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                if *is_hovered {
                    this.hovered_section_id = Some(id);
                } else if this.hovered_section_id == Some(id) {
                    this.hovered_section_id = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, _, _| match id {
                "pinned-heading" => this.store.set_section_collapsed("pinned", !collapsed),
                "projects-heading" => this.store.set_section_collapsed("projects", !collapsed),
                "recent-heading" => this.store.set_section_collapsed("recent", !collapsed),
                _ => {}
            }))
    }

    fn status_row(
        &self,
        id: impl Into<gpui::ElementId>,
        text: impl Into<SharedString>,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .h(px(ROW_HEIGHT))
            .px(px(ROW_HORIZONTAL_PADDING))
            .flex()
            .items_center()
            .text_size(px(14.0))
            .text_color(theme.sidebar_text_muted)
            .overflow_hidden()
            .whitespace_nowrap()
            .child(text.into())
    }

    fn retryable_error_row(
        &self,
        id: &'static str,
        error: String,
        search_query: Option<String>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .min_h(px(ROW_HEIGHT))
            .px(px(ROW_HORIZONTAL_PADDING))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(13.0))
            .text_color(theme.warning)
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(error),
            )
            .child(
                div()
                    .id(format!("{id}-retry"))
                    .rounded(px(7.0))
                    .px(px(6.0))
                    .py(px(3.0))
                    .cursor_pointer()
                    .text_color(theme.sidebar_text)
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .child(crate::i18n::text("重试"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(query) = &search_query {
                            this.store.search(query.clone());
                        } else {
                            this.store.retry();
                        }
                        cx.notify();
                    })),
            )
    }

    fn thread_title_width(text: &str, window: &mut Window) -> f32 {
        let mut font = window.text_style().font();
        font.family = ".SystemUIFont".into();
        font.weight = SIDEBAR_BODY_FONT_WEIGHT;
        let run = TextRun {
            len: text.len(),
            font,
            color: window.text_style().color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        f32::from(
            window
                .text_system()
                .shape_line(text.to_owned().into(), px(14.0), &[run], None)
                .width(),
        )
    }

    fn start_marquee(
        &mut self,
        text: &str,
        viewport_width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        self.marquee_started_at = Some(now);
        let distance = (Self::thread_title_width(text, window) - viewport_width).max(0.0);
        self.marquee_animation_ends_at = (distance > 0.0).then(|| now + marquee_duration(distance));
        if distance > 0.0 && !cx.reduce_motion() && !self.marquee_animation_running {
            self.marquee_animation_running = true;
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_marquee_animation(window, cx)
            });
        }
    }

    fn stop_marquee(&mut self) {
        self.marquee_started_at = None;
        self.marquee_animation_ends_at = None;
        self.marquee_animation_running = false;
    }

    fn advance_marquee_animation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.marquee_animation_running {
            return;
        }
        let now = cx.background_executor().now();
        if cx.reduce_motion()
            || self
                .marquee_animation_ends_at
                .is_none_or(|animation_end| now >= animation_end)
            || self.hovered_thread_id.is_none()
        {
            self.marquee_animation_running = false;
            cx.notify();
            return;
        }
        cx.notify();
        cx.on_next_frame(window, |this, window, cx| {
            this.advance_marquee_animation(window, cx)
        });
    }

    fn thread_row(
        &self,
        thread: &ThreadSummary,
        placement: ThreadRowPlacement,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let ThreadRowPlacement {
            indented,
            pinned,
            archived,
        } = placement;
        let thread_id = thread.thread_id.clone();
        let selected = self.selected_thread_id.as_deref() == Some(thread_id.as_str());
        let hovered = self.hovered_thread_id.as_deref() == Some(thread_id.as_str());
        let pending = self.snapshot.is_pending_thread(&thread_id);
        let rename_active =
            self.rename_target.as_ref() == Some(&RenameTarget::Thread(thread_id.clone()));
        let active = matches!(thread.activity, ThreadActivity::Active { .. });
        let status_error = matches!(thread.activity, ThreadActivity::SystemError);
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
        let can_archive = self.snapshot.capabilities.supports(if archived {
            AgentCapability::ThreadUnarchive
        } else {
            AgentCapability::ThreadArchive
        });
        let pin_id = thread_id.clone();
        let pin = Self::action_icon_button(
            format!("thread-pin-{thread_id}"),
            "pin",
            theme,
            pending || !can_pin,
        )
        .when(!pending && can_pin, |button| {
            button.on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.store.set_thread_pinned(pin_id.clone(), !pinned);
            }))
        });
        let archive_id = thread_id.clone();
        let archive_button = Self::action_icon_button(
            format!("thread-archive-{thread_id}"),
            "archive",
            theme,
            pending || !can_archive,
        )
        .when(!pending && can_archive, |button| {
            button.on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if archived {
                    this.store.unarchive_thread(archive_id.clone());
                } else {
                    this.store.archive_thread(archive_id.clone());
                }
            }))
        });
        let actions = div()
            .absolute()
            .right(px(8.0))
            .top(px(5.0))
            .flex()
            .gap(px(8.0))
            .when(!hovered, |actions| actions.invisible())
            .child(pin)
            .child(archive_button);
        let title_viewport_width =
            sidebar_thread_title_viewport_width(self.width, !indented, hovered);
        let title_width = Self::thread_title_width(&thread.title, window);
        let title_scroll_distance = if hovered {
            (title_width - title_viewport_width).max(0.0)
        } else {
            0.0
        };
        let title_scroll_offset = self.marquee_started_at.map_or(0.0, |started_at| {
            marquee_offset(
                title_scroll_distance,
                cx.background_executor()
                    .now()
                    .saturating_duration_since(started_at),
                cx.reduce_motion(),
            )
        });
        let trailing_rail = div()
            .ml(px(3.0))
            .flex_none()
            .when(hovered, |rail| rail.w(px(48.0)).min_w(px(48.0)))
            .when(active && !hovered, |rail| rail.w(px(25.0)).min_w(px(25.0)))
            .when(status_error && !hovered && !active, |rail| {
                rail.w(px(16.0)).min_w(px(16.0))
            })
            .when(!hovered && !active && !status_error, |rail| {
                rail.w(px(0.0)).min_w(px(0.0))
            });
        let title = if rename_active {
            self.rename_input.clone().into_any_element()
        } else {
            div()
                .min_w(px(0.0))
                .flex_1()
                .h(px(20.0))
                .child(thread_title_canvas(
                    thread.title.clone().into(),
                    theme.sidebar_text.into(),
                    title_scroll_offset,
                    title_width > title_viewport_width,
                ))
                .into_any_element()
        };
        let hover_id = thread_id.clone();
        let select_id = thread_id.clone();
        let context_id = thread_id.clone();
        let bounds_id = thread_id.clone();
        let card_id = thread_id.clone();
        let row_bounds = self.thread_row_bounds.clone();
        let marquee_title = thread.title.clone();
        let spinner = icon("dictation-spinner", theme.sidebar_icon_muted.into())
            .size(px(20.0))
            .with_animation(
                format!("thread-running-{thread_id}"),
                Animation::new(Duration::from_millis(800)).repeat(),
                |spinner, progress| {
                    spinner.with_transformation(Transformation::rotate(radians(
                        progress * std::f32::consts::TAU,
                    )))
                },
            );
        div()
            .id(format!("thread-row-{thread_id}"))
            .h(px(ROW_HEIGHT))
            .relative()
            .top(px(1.0))
            .pl(px(8.0))
            .pr(px(5.0))
            .flex()
            .items_center()
            .rounded(px(8.0))
            .text_size(px(14.0))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.sidebar_text)
            .overflow_hidden()
            .whitespace_nowrap()
            .when(pending, |row| row.opacity(0.4).cursor_default())
            .when(!pending, |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
            })
            .when(selected, |row| row.bg(theme.sidebar_hover))
            .child(
                div()
                    .h_full()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .items_center()
                            .when(indented, |title_row| {
                                title_row.gap(px(8.0)).child(div().w(px(16.0)).flex_none())
                            })
                            .child(title),
                    )
                    .child(trailing_rail),
            )
            .when(active && !hovered, |row| {
                row.child(
                    div()
                        .absolute()
                        .right(px(5.0))
                        .top(px(5.0))
                        .size(px(22.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(spinner),
                )
            })
            .when(status_error && !hovered, |row| {
                row.child(
                    div()
                        .absolute()
                        .right(px(12.0))
                        .size(px(6.0))
                        .rounded_full()
                        .bg(theme.warning),
                )
            })
            .child(actions)
            .child(
                // Records the row's window bounds for the hover card, which is
                // deferred so the sidebar's own clip never cuts it off.
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
                    this.start_marquee(
                        &marquee_title,
                        sidebar_thread_title_viewport_width(this.width, !indented, true),
                        window,
                        cx,
                    );
                } else if this.hovered_thread_id.as_deref() == Some(hover_id.as_str()) {
                    this.hovered_thread_id = None;
                    this.stop_marquee();
                }
                this.set_thread_row_hovered(card_id.clone(), *hovered, cx);
                cx.notify();
            }))
            .when(!pending && !rename_active, |row| {
                row.on_click(cx.listener(move |this, _, _, cx| {
                    this.select_thread(select_id.clone(), cx);
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
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

    fn project_group(
        &self,
        project: &Project,
        pinned_ids: &HashSet<&str>,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let project_id = project.project_id.clone();
        let collapsed = self
            .snapshot
            .preferences
            .collapsed_project_ids
            .contains(&project_id);
        let pending = self.snapshot.is_pending_project(&project_id);
        let hovered = self.hovered_project_id.as_deref() == Some(project_id.as_str());
        let rename_active =
            self.rename_target.as_ref() == Some(&RenameTarget::Project(project_id.clone()));
        let menu_open = self.project_menu_id.as_deref() == Some(project_id.as_str());
        let threads = self
            .snapshot
            .recent_threads
            .iter()
            .filter(|thread| {
                project_id_for_thread(thread, &self.snapshot.projects).as_deref()
                    == Some(project_id.as_str())
                    && !pinned_ids.contains(thread.thread_id.as_str())
            })
            .collect::<Vec<_>>();
        let show_all = self.show_all_projects.contains(&project_id);
        let toggle_id = project_id.clone();
        let hover_id = project_id.clone();
        let bounds_id = project_id.clone();
        let menu_id = project_id.clone();
        let new_chat_project = project.clone();
        let row_bounds = self.project_row_bounds.clone();
        let actions = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(!hovered && !menu_open, |actions| actions.invisible())
            .child(
                Self::nav_icon_button(
                    format!("project-menu-{project_id}"),
                    "more-horizontal",
                    theme,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.project_menu_id = Some(menu_id.clone());
                        this.thread_menu_id = None;
                        this.delete_confirmation = None;
                        this.menu_origin =
                            (f32::from(event.position.x), f32::from(event.position.y));
                        cx.notify();
                    }),
                ),
            )
            .child(
                Self::nav_icon_button(format!("project-new-chat-{project_id}"), "new-chat", theme)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.new_conversation(Some(&new_chat_project), cx);
                    })),
            );
        let title = if rename_active {
            self.rename_input.clone().into_any_element()
        } else {
            div()
                .min_w(px(0.0))
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(project.name.clone())
                .into_any_element()
        };
        let mut group = div()
            .id(format!("project-group-{project_id}"))
            .flex()
            .flex_col()
            .gap(px(1.0));
        group = group.child(
            div()
                .id(format!("project-row-{project_id}"))
                .h(px(ROW_HEIGHT))
                .w_full()
                .relative()
                .pl(px(1.0))
                .pr(px(6.0))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .text_size(px(14.0))
                .text_color(theme.sidebar_text)
                .when(pending, |row| row.opacity(0.4).cursor_default())
                .when(!pending, |row| {
                    row.cursor_pointer()
                        .hover(move |style| style.bg(theme.sidebar_hover))
                })
                .child(
                    div()
                        .size(px(30.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon("folder", theme.sidebar_text.into())),
                )
                .child(title)
                .child(actions)
                .child(
                    // Records the row's window bounds for the hover card, which
                    // is deferred so the sidebar's own clip never cuts it off.
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
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.set_project_row_hovered(hover_id.clone(), *hovered, cx);
                }))
                .when(!pending && !rename_active, |row| {
                    row.on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_project_id = Some(toggle_id.clone());
                        this.store
                            .set_project_collapsed(toggle_id.clone(), !collapsed);
                        cx.notify();
                    }))
                }),
        );
        if !collapsed {
            let visible = if show_all {
                threads.len()
            } else {
                threads.len().min(MAX_VISIBLE_PROJECT_THREADS)
            };
            for thread in threads.iter().take(visible) {
                group = group.child(self.thread_row(
                    thread,
                    ThreadRowPlacement {
                        indented: true,
                        pinned: false,
                        archived: false,
                    },
                    theme,
                    window,
                    cx,
                ));
            }
            if threads.len() > MAX_VISIBLE_PROJECT_THREADS {
                let show_id = project_id.clone();
                group = group.child(
                    div()
                        .id(format!("project-show-all-{project_id}"))
                        .h(px(ROW_HEIGHT))
                        .pl(px(32.0))
                        .flex()
                        .items_center()
                        .text_size(px(14.0))
                        .text_color(theme.sidebar_text_muted)
                        .cursor_pointer()
                        .hover(move |style| style.text_color(theme.sidebar_text))
                        .child(if show_all {
                            crate::i18n::text("收起")
                        } else {
                            crate::i18n::text("展开显示")
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.show_all_projects.insert(show_id.clone()) {
                                this.show_all_projects.remove(&show_id);
                            }
                            cx.notify();
                        })),
                );
            }
        }
        group
    }

    /// The sidebar project hover card. Row order, geometry, and colors come
    /// from the ChatGPT reference capture in `artifacts/project-hover-20260921/`:
    /// the card anchors three pixels right of the row it describes and
    /// top-aligns with it, mirroring the reference's `data-side="right"` tooltip.
    fn project_hover_card(
        &self,
        project: &Project,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let project_id = project.project_id.clone();
        let threads = self
            .snapshot
            .recent_threads
            .iter()
            .filter(|thread| {
                project_id_for_thread(thread, &self.snapshot.projects).as_deref()
                    == Some(project_id.as_str())
            })
            .collect::<Vec<_>>();
        let summary = project_hover_summary(&threads);
        let repo = self
            .project_repos
            .get(&project_id)
            .and_then(|repo| repo.clone())
            .filter(|repo| repo.label.trim().to_lowercase() != project.name.trim().to_lowercase());
        let paths = project_hover_paths(project, repo.as_ref());

        let card_hover_id = project_id.clone();
        let rename_id = project_id.clone();
        let rename_name = project.name.clone();
        let edit_options_id = project_id.clone();
        let path_group = format!("project-hover-path-{project_id}");

        let header = div()
            .h(px(PROJECT_HOVER_HEADER_HEIGHT))
            .w_full()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(PROJECT_HOVER_ICON_GAP))
            .child(Self::project_hover_icon("project-hover-marker", theme))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(PROJECT_HOVER_INLINE_GAP))
                    .child(
                        div()
                            .id(format!("project-hover-name-{project_id}"))
                            .min_w(px(0.0))
                            .flex_shrink_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .rounded(px(PROJECT_HOVER_ROW_RADIUS))
                            .text_size(px(14.0))
                            .line_height(px(24.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .child(project.name.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.start_rename(
                                    RenameTarget::Project(rename_id.clone()),
                                    rename_name.clone(),
                                    cx,
                                );
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        // The reference always shows the pin action here; this
                        // build has no project pinning, so the icon stays inert.
                        div()
                            .flex_none()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("project-pin", theme.text_tertiary.into()).size(px(16.0))),
                    ),
            );

        let mut card = div()
            .id(format!("project-hover-card-{project_id}"))
            .w(px(PROJECT_HOVER_CARD_WIDTH))
            .p(px(PROJECT_HOVER_CARD_PADDING))
            .flex()
            .flex_col()
            .gap(px(PROJECT_HOVER_CARD_GAP))
            .rounded(px(PROJECT_HOVER_CARD_RADIUS))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.project_hover_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.profile_menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font_family(".SystemUIFont")
            .text_size(px(13.0))
            .line_height(px(PROJECT_HOVER_ROW_HEIGHT))
            .text_color(theme.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.project_hover_card_hovered = *hovered;
                if !*hovered && this.hovered_project_id.as_deref() != Some(card_hover_id.as_str()) {
                    this.project_hover_card = None;
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(PROJECT_HOVER_ROW_GAP))
                    .child(header)
                    .child(Self::project_hover_row(
                        format!("project-hover-tasks-{project_id}"),
                        "project-hover-tasks",
                        summary,
                        theme,
                    )),
            );

        if repo.is_some() || !paths.is_empty() {
            let mut sources = div()
                .flex()
                .flex_col()
                .gap(px(PROJECT_HOVER_ROW_GAP))
                .border_t(px(1.0))
                .border_color(theme.border)
                .pt(px(PROJECT_HOVER_SECTION_PADDING));
            if let Some(repo) = &repo {
                sources = sources.child(Self::project_hover_row(
                    format!("project-hover-repo-{project_id}"),
                    "project-hover-repo",
                    repo.label.clone(),
                    theme,
                ));
            }
            for (index, (display, path)) in paths.into_iter().enumerate() {
                let reveal = path.clone();
                sources = sources.child(
                    div()
                        .id(format!("project-hover-path-{project_id}-{index}"))
                        .group(path_group.clone())
                        .h(px(PROJECT_HOVER_ROW_HEIGHT))
                        .w_full()
                        .min_w(px(0.0))
                        .rounded(px(PROJECT_HOVER_ROW_RADIUS))
                        .flex()
                        .items_center()
                        .gap(px(PROJECT_HOVER_ICON_GAP))
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.sidebar_hover))
                        .child(Self::project_hover_icon("project-hover-path", theme))
                        .child(Self::project_hover_text(display, theme))
                        .child(
                            div()
                                .flex_none()
                                .size(px(PROJECT_HOVER_TRAILING_SLOT))
                                .flex()
                                .items_center()
                                .justify_center()
                                .opacity(0.0)
                                .group_hover(path_group.clone(), |slot| slot.opacity(1.0))
                                .child(
                                    icon("project-hover-open", theme.text_tertiary.into())
                                        .size(px(14.0)),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.reveal_in_finder(reveal.clone(), cx);
                        })),
                );
            }
            card = card.child(sources);
        }

        card.child(
            div()
                .flex()
                .flex_col()
                .gap(px(PROJECT_HOVER_ROW_GAP))
                .border_t(px(1.0))
                .border_color(theme.border)
                .pt(px(PROJECT_HOVER_SECTION_PADDING))
                .child(
                    div()
                        .id(format!("project-hover-edit-{project_id}"))
                        .h(px(PROJECT_HOVER_ROW_HEIGHT))
                        .w_full()
                        .min_w(px(0.0))
                        .rounded(px(PROJECT_HOVER_ROW_RADIUS))
                        .flex()
                        .items_center()
                        .gap(px(PROJECT_HOVER_ICON_GAP))
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.sidebar_hover))
                        .child(Self::project_hover_icon("project-hover-edit", theme))
                        .child(Self::project_hover_text(
                            crate::i18n::text("编辑项目").to_owned(),
                            theme,
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.project_menu_id = Some(edit_options_id.clone());
                            this.thread_menu_id = None;
                            this.delete_confirmation = None;
                            this.menu_origin = (160.0, 160.0);
                            cx.notify();
                        })),
                ),
        )
    }

    fn project_hover_icon(glyph: &'static str, theme: Theme) -> Div {
        div()
            .w(px(PROJECT_HOVER_ICON_SLOT))
            .h(px(PROJECT_HOVER_ROW_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(icon(glyph, theme.text_tertiary.into()).size(px(PROJECT_HOVER_ICON_SLOT)))
    }

    fn project_hover_text(text: String, theme: Theme) -> Div {
        div()
            .min_w(px(0.0))
            .flex_1()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(13.0))
            .line_height(px(PROJECT_HOVER_ROW_HEIGHT))
            .text_color(theme.text)
            .child(text)
    }

    /// The sidebar task hover card. Geometry, colors, and row order come from
    /// the live reference capture in `artifacts/thread-hover-20260922/`: the
    /// card anchors three pixels right of the row it describes and top-aligns
    /// one pixel below it, prints the task title with its environment icon and
    /// compact timestamp, then the project the task is filed under.
    fn thread_hover_card(
        &self,
        thread: &ThreadSummary,
        project: &Project,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let thread_id = thread.thread_id.clone();
        let card_hover_id = thread_id.clone();
        let duration = compact_relative_time(
            chrono::Utc::now().timestamp_millis(),
            thread_hover_timestamp_ms(thread),
        );

        let title_row = div()
            .w_full()
            .min_w(px(0.0))
            .h(px(THREAD_HOVER_TITLE_ROW_HEIGHT))
            .flex()
            .items_start()
            .gap(px(THREAD_HOVER_TITLE_TRAILING_GAP))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_start()
                    .gap(px(THREAD_HOVER_TITLE_ICON_GAP))
                    .child(
                        div()
                            .id(format!("thread-hover-title-{thread_id}"))
                            .ml(px(-THREAD_HOVER_TITLE_INSET))
                            .min_w(px(0.0))
                            .px(px(THREAD_HOVER_TITLE_PADDING_X))
                            .rounded(px(THREAD_HOVER_TITLE_RADIUS))
                            .text_size(px(14.0))
                            .line_height(px(THREAD_HOVER_TITLE_LINE_HEIGHT))
                            .font(hover_card_font(gpui::FontWeight::MEDIUM))
                            .text_color(theme.text)
                            .line_clamp(3)
                            .whitespace_normal()
                            .child(thread.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(THREAD_HOVER_ENV_ICON))
                            .h(px(THREAD_HOVER_ROW_HEIGHT))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                icon("local", theme.text_tertiary.into())
                                    .size(px(THREAD_HOVER_ENV_ICON)),
                            ),
                    ),
            )
            .child(
                // `items-baseline` in the reference drops the 12 px timestamp
                // one pixel below the 14 px title.
                div()
                    .flex_none()
                    .mt(px(THREAD_HOVER_DURATION_OFFSET_Y))
                    .h(px(THREAD_HOVER_ROW_HEIGHT))
                    .flex()
                    .items_center()
                    .font(hover_card_font(crate::theme::UI_BODY_FONT_WEIGHT))
                    .text_size(px(12.0))
                    .line_height(px(THREAD_HOVER_ROW_HEIGHT))
                    .text_color(theme.text_tertiary)
                    .child(duration),
            );

        div()
            .id(format!("thread-hover-card-{thread_id}"))
            .w(px(THREAD_HOVER_CARD_WIDTH))
            .px(px(THREAD_HOVER_CARD_PADDING_X))
            .py(px(THREAD_HOVER_CARD_PADDING_Y))
            .flex()
            .flex_col()
            .gap(px(THREAD_HOVER_CARD_GAP))
            .rounded(px(THREAD_HOVER_CARD_RADIUS))
            .bg(hover_card_surface(theme))
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.profile_menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font_family(".SystemUIFont")
            .text_size(px(13.0))
            .line_height(px(18.5714))
            .text_color(theme.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.thread_hover_card_hovered = *hovered;
                if !*hovered && this.hovered_thread_id.as_deref() != Some(card_hover_id.as_str()) {
                    this.thread_hover_card = None;
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(THREAD_HOVER_CARD_GAP))
                    .pb(px(2.0))
                    .child(title_row),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .h(px(THREAD_HOVER_ROW_HEIGHT))
                    .flex()
                    .items_center()
                    .gap(px(THREAD_HOVER_PROJECT_GAP))
                    .child(
                        div()
                            .flex_none()
                            .w(px(THREAD_HOVER_PROJECT_ICON))
                            .h(px(THREAD_HOVER_ROW_HEIGHT))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                icon("thread-hover-project", theme.text_tertiary.into())
                                    .size(px(THREAD_HOVER_PROJECT_ICON)),
                            ),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_overflow(gpui::TextOverflow::Truncate("…".into()))
                            .font(hover_card_font(crate::theme::UI_BODY_FONT_WEIGHT))
                            .text_size(px(13.0))
                            .line_height(px(THREAD_HOVER_ROW_HEIGHT))
                            .text_color(theme.text)
                            .child(project.name.clone()),
                    ),
            )
    }

    /// A read-only hover card row: icon column plus a single text line.
    fn project_hover_row(
        id: String,
        glyph: &'static str,
        text: String,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .h(px(PROJECT_HOVER_ROW_HEIGHT))
            .w_full()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(PROJECT_HOVER_ICON_GAP))
            .child(Self::project_hover_icon(glyph, theme))
            .child(Self::project_hover_text(text, theme))
    }

    fn pinned_section(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let collapsed = self.snapshot.preferences.pinned_collapsed;
        let mut section = div()
            .id("pinned-section")
            .pt(px(10.0))
            .flex()
            .flex_col()
            .child(self.section_header(
                "pinned-heading",
                crate::i18n::text("置顶"),
                collapsed,
                theme,
                cx,
            ));
        if collapsed {
            return section;
        }
        if self.snapshot.loading.pinned && self.snapshot.pinned_threads.is_empty() {
            return section.child(self.status_row(
                "pinned-loading",
                crate::i18n::text("正在加载…"),
                theme,
            ));
        }
        for thread in &self.snapshot.pinned_threads {
            section = section.child(self.thread_row(
                thread,
                ThreadRowPlacement {
                    indented: false,
                    pinned: true,
                    archived: false,
                },
                theme,
                window,
                cx,
            ));
        }
        section
    }

    fn projects_section(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let collapsed = self.snapshot.preferences.projects_collapsed;
        let heading_hovered = self.hovered_section_id == Some("projects-heading");
        let show_heading_actions = heading_hovered || self.projects_section_menu_open;
        let can_create = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ProjectCreate);
        let pinned_ids = self
            .snapshot
            .pinned_threads
            .iter()
            .map(|thread| thread.thread_id.as_str())
            .collect::<HashSet<_>>();
        let menu_button = Self::nav_icon_button("projects-options", "more-horizontal", theme)
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.projects_section_menu_open = !this.projects_section_menu_open;
                this.project_menu_id = None;
                cx.notify();
            }));
        let add_button = Self::nav_icon_button("project-add", "add", theme)
            .when(!can_create, |button| button.opacity(0.4).cursor_default())
            .when(can_create, |button| {
                button.on_click(cx.listener(|_, _, _, cx| {
                    cx.stop_propagation();
                    cx.emit(OpenProjectCreation);
                }))
            });
        let mut section = div().id("projects-section").pt(px(10.0)).flex().flex_col();
        section = section.child(
            div()
                .id("projects-section-heading-row")
                .h(px(SECTION_HEADER_HEIGHT))
                .px(px(8.0))
                .flex()
                .items_center()
                .text_size(px(14.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.sidebar_text_muted)
                .child(
                    div()
                        .id("projects-heading")
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .cursor_pointer()
                        .child(crate::i18n::text("项目"))
                        .child(
                            icon("section-chevron", theme.sidebar_icon_muted.into())
                                .size(px(14.0))
                                .when(!show_heading_actions, |chevron| chevron.invisible())
                                .with_transformation(Transformation::rotate(radians(
                                    if collapsed {
                                        -std::f32::consts::FRAC_PI_2
                                    } else {
                                        0.0
                                    },
                                ))),
                        )
                        .on_click(cx.listener(move |this, _, _, _| {
                            this.store.set_section_collapsed("projects", !collapsed);
                        })),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .when(!show_heading_actions, |actions| actions.invisible())
                        .child(menu_button)
                        .child(add_button),
                )
                .on_hover(cx.listener(|this, is_hovered: &bool, _, cx| {
                    if *is_hovered {
                        this.hovered_section_id = Some("projects-heading");
                    } else if this.hovered_section_id == Some("projects-heading") {
                        this.hovered_section_id = None;
                    }
                    cx.notify();
                })),
        );
        if collapsed {
            return section;
        }
        if self.snapshot.loading.projects && self.snapshot.projects.is_empty() {
            return section.child(self.status_row(
                "projects-loading",
                crate::i18n::text("正在加载…"),
                theme,
            ));
        }
        let creating = self.snapshot.pending.iter().any(|operation| {
            matches!(
                operation,
                crate::workspace::WorkspaceOperation::CreateProject(_)
            )
        });
        if self.snapshot.projects.is_empty() {
            return section.child(self.status_row(
                "projects-empty",
                if creating {
                    crate::i18n::text("正在创建项目…")
                } else {
                    crate::i18n::text("暂无项目")
                },
                theme,
            ));
        }
        let mut projects = div().flex().flex_col().gap(px(10.0));
        for project in &self.snapshot.projects {
            projects = projects.child(self.project_group(project, &pinned_ids, theme, window, cx));
        }
        section = section.child(projects);
        if creating {
            section = section.child(self.status_row(
                "project-creating",
                crate::i18n::text("正在创建项目…"),
                theme,
            ));
        }
        section
    }

    fn recent_section(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let collapsed = self.snapshot.preferences.recent_collapsed;
        let pinned_ids = self
            .snapshot
            .pinned_threads
            .iter()
            .map(|thread| thread.thread_id.as_str())
            .collect::<HashSet<_>>();
        let threads = self
            .snapshot
            .recent_threads
            .iter()
            .filter(|thread| {
                project_id_for_thread(thread, &self.snapshot.projects).is_none()
                    && !pinned_ids.contains(thread.thread_id.as_str())
            })
            .take(MAX_VISIBLE_RECENTS)
            .collect::<Vec<_>>();
        let mut section = div()
            .id("recent-section")
            .pt(px(10.0))
            .pb(px(12.0))
            .flex()
            .flex_col()
            .child(self.section_header(
                "recent-heading",
                crate::i18n::text("最近"),
                collapsed,
                theme,
                cx,
            ));
        if collapsed {
            return section;
        }
        if self.snapshot.loading.recent && self.snapshot.recent_threads.is_empty() {
            return section.child(self.status_row(
                "recent-loading",
                crate::i18n::text("正在加载…"),
                theme,
            ));
        }
        if threads.is_empty() {
            return section.child(self.status_row(
                "recent-empty",
                crate::i18n::text("暂无最近聊天"),
                theme,
            ));
        }
        for thread in threads {
            section = section.child(self.thread_row(
                thread,
                ThreadRowPlacement {
                    indented: false,
                    pinned: false,
                    archived: false,
                },
                theme,
                window,
                cx,
            ));
        }
        section
    }

    fn activity_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut seen = HashSet::new();
        let active = self
            .snapshot
            .pinned_threads
            .iter()
            .chain(&self.snapshot.recent_threads)
            .filter(|thread| {
                seen.insert(thread.thread_id.as_str())
                    && matches!(
                        thread.activity,
                        ThreadActivity::Active { .. } | ThreadActivity::SystemError
                    )
            })
            .collect::<Vec<_>>();
        let mut content = div()
            .id("activity-content")
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.activity_scroll)
            .px(px(8.0))
            .pt(px(8.0))
            .child(
                div()
                    .h(px(SECTION_HEADER_HEIGHT))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.sidebar_text_muted)
                    .child(crate::i18n::text("活动")),
            );
        if let Some(error) = self.snapshot.error.clone() {
            return content.child(self.retryable_error_row(
                "activity-error",
                error,
                None,
                theme,
                cx,
            ));
        }
        if (self.snapshot.loading.recent || self.snapshot.loading.pinned) && active.is_empty() {
            return content.child(self.status_row(
                "activity-loading",
                crate::i18n::text("正在加载…"),
                theme,
            ));
        }
        if active.is_empty() {
            return content.child(self.status_row(
                "activity-empty",
                crate::i18n::text("暂无进行中的聊天"),
                theme,
            ));
        }
        for thread in active {
            content = content.child(self.thread_row(
                thread,
                ThreadRowPlacement {
                    indented: false,
                    pinned: false,
                    archived: false,
                },
                theme,
                window,
                cx,
            ));
        }
        content
    }

    fn archived_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut content = div()
            .id("archived-content")
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(8.0))
            .pt(px(8.0))
            .child(
                div()
                    .h(px(SECTION_HEADER_HEIGHT))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.sidebar_text_muted)
                    .child(crate::i18n::text("已归档")),
            );
        if let Some(error) = self.snapshot.error.clone() {
            return content.child(self.retryable_error_row(
                "archived-error",
                error,
                None,
                theme,
                cx,
            ));
        }
        if self.snapshot.loading.archived && self.snapshot.archived_threads.is_empty() {
            return content.child(self.status_row(
                "archived-loading",
                crate::i18n::text("正在加载…"),
                theme,
            ));
        }
        if self.snapshot.archived_threads.is_empty() {
            return content.child(self.status_row(
                "archived-empty",
                crate::i18n::text("暂无已归档聊天"),
                theme,
            ));
        }
        for thread in &self.snapshot.archived_threads {
            content = content.child(self.thread_row(
                thread,
                ThreadRowPlacement {
                    indented: false,
                    pinned: false,
                    archived: true,
                },
                theme,
                window,
                cx,
            ));
        }
        content
    }

    fn workspace_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut content = div()
            .id("workspace-content")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(8.0));
        if let Some(error) = self
            .local_error
            .as_ref()
            .or(self.snapshot.preference_error.as_ref())
            .or(self.snapshot.error.as_ref())
        {
            content = content.child(
                div()
                    .id("workspace-error")
                    .mx(px(8.0))
                    .my(px(6.0))
                    .p(px(8.0))
                    .rounded(px(ROW_RADIUS))
                    .bg(theme.sidebar_hover)
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .text_size(px(13.0))
                    .text_color(theme.warning)
                    .child(error.clone())
                    .child(
                        div()
                            .id("workspace-retry")
                            .text_color(theme.sidebar_text)
                            .cursor_pointer()
                            .child(crate::i18n::text("重试"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.local_error = None;
                                this.store.retry();
                                cx.notify();
                            })),
                    ),
            );
        }
        content
            .child(
                div()
                    .pb(px(21.0))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(self.pull_requests_nav_row(theme, self.pull_requests_open, cx))
                    .child(Self::static_nav_row(
                        "sidebar-scheduled",
                        crate::i18n::text("已安排"),
                        "scheduled",
                        theme,
                    ))
                    .child(Self::static_nav_row(
                        "sidebar-plugins",
                        crate::i18n::text("插件"),
                        "plugins",
                        theme,
                    )),
            )
            .when(!self.snapshot.pinned_threads.is_empty(), |content| {
                content.child(self.pinned_section(theme, window, cx))
            })
            .child(self.projects_section(theme, window, cx))
            .child(self.recent_section(theme, window, cx))
    }

    fn menu_shell(
        &self,
        id: impl Into<gpui::ElementId>,
        width: f32,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .w(px(width))
            .p(px(4.0))
            .rounded(px(10.0))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(10.0), theme.profile_menu_shadow.into())
                    .blur_radius(px(24.0))
                    .spread_radius(px(-3.0)),
            ])
            .font_family(".SystemUIFont")
            .text_size(px(13.0))
            .text_color(theme.sidebar_text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }

    fn menu_item(
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        glyph: &'static str,
        theme: Theme,
        enabled: bool,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .h(px(27.0))
            .px(px(6.0))
            .rounded(px(7.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .text_color(theme.sidebar_text)
            .when(enabled, |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
            })
            .when(!enabled, |row| row.opacity(0.4).cursor_default())
            .child(icon(glyph, theme.sidebar_text.into()).size(px(16.0)))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(label.into()),
            )
    }

    fn menu_separator(theme: Theme) -> Div {
        div()
            .h(px(9.0))
            .px(px(5.0))
            .flex()
            .items_center()
            .child(div().h(px(0.5)).w_full().bg(theme.border))
    }

    fn project_context_menu(
        &self,
        project: &Project,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let project_id = project.project_id.clone();
        let project_name = project.name.clone();
        let project_root = project.roots.first().cloned();
        let index = self
            .snapshot
            .projects
            .iter()
            .position(|candidate| candidate.project_id == project_id)
            .unwrap_or(0);
        let move_up_before = index
            .checked_sub(1)
            .and_then(|before| self.snapshot.projects.get(before))
            .map(|project| project.project_id.clone());
        let move_down_before = self
            .snapshot
            .projects
            .get(index + 2)
            .map(|project| project.project_id.clone());
        let can_move_up = index > 0;
        let can_move_down = index + 1 < self.snapshot.projects.len();
        let can_update = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ProjectUpdate);
        let can_move = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ProjectMove);
        let can_delete = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ProjectDelete);
        let rename_id = project_id.clone();
        let up_id = project_id.clone();
        let down_id = project_id.clone();
        let delete_id = project_id.clone();
        let deleting =
            self.delete_confirmation.as_ref() == Some(&DeleteTarget::Project(project_id.clone()));
        let mut menu = self
            .menu_shell(format!("project-context-{project_id}"), 190.0, theme)
            .child(
                Self::menu_item(
                    format!("project-rename-{project_id}"),
                    crate::i18n::text("重命名"),
                    "settings-edit",
                    theme,
                    can_update,
                )
                .when(can_update, |row| {
                    row.on_click(cx.listener(move |this, _, _, cx| {
                        this.start_rename(
                            RenameTarget::Project(rename_id.clone()),
                            project_name.clone(),
                            cx,
                        );
                    }))
                }),
            );
        if let Some(root) = project_root {
            menu = menu.child(
                Self::menu_item(
                    format!("project-finder-{project_id}"),
                    crate::i18n::text("在 Finder 中显示"),
                    "project-reveal",
                    theme,
                    true,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.reveal_in_finder(root.clone(), cx);
                })),
            );
        }
        menu = menu
            .child(Self::menu_separator(theme))
            .child(
                Self::menu_item(
                    format!("project-up-{project_id}"),
                    crate::i18n::text("上移"),
                    "settings-chevron-up",
                    theme,
                    can_move && can_move_up,
                )
                .when(can_move && can_move_up, |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        this.store
                            .move_project(up_id.clone(), move_up_before.clone());
                        this.project_menu_id = None;
                    }))
                }),
            )
            .child(
                Self::menu_item(
                    format!("project-down-{project_id}"),
                    crate::i18n::text("下移"),
                    "chevron-down",
                    theme,
                    can_move && can_move_down,
                )
                .when(can_move && can_move_down, |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        this.store
                            .move_project(down_id.clone(), move_down_before.clone());
                        this.project_menu_id = None;
                    }))
                }),
            )
            .child(Self::menu_separator(theme))
            .child(
                Self::menu_item(
                    format!("project-delete-{project_id}"),
                    if deleting {
                        crate::i18n::text("再次点击以移除")
                    } else {
                        crate::i18n::text("移除项目")
                    },
                    "close-dialog",
                    theme,
                    can_delete,
                )
                .when(can_delete, |row| {
                    row.on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm_or_delete(DeleteTarget::Project(delete_id.clone()), cx);
                    }))
                }),
            );
        menu
    }

    fn thread_context_menu(
        &self,
        thread: &ThreadSummary,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let thread_id = thread.thread_id.clone();
        let pinned = self
            .snapshot
            .pinned_threads
            .iter()
            .any(|candidate| candidate.thread_id == thread_id);
        let archived = self
            .snapshot
            .archived_threads
            .iter()
            .any(|candidate| candidate.thread_id == thread_id);
        let rename_id = thread_id.clone();
        let rename_title = thread.title.clone();
        let pin_id = thread_id.clone();
        let archive_id = thread_id.clone();
        let unassigned_id = thread_id.clone();
        let delete_id = thread_id.clone();
        let can_rename = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadRename);
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
        let can_archive = self.snapshot.capabilities.supports(if archived {
            AgentCapability::ThreadUnarchive
        } else {
            AgentCapability::ThreadArchive
        });
        let can_move = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadMetadataUpdate);
        let can_delete = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadDelete);
        let deleting =
            self.delete_confirmation.as_ref() == Some(&DeleteTarget::Thread(thread_id.clone()));
        let mut menu = self
            .menu_shell(format!("thread-context-{thread_id}"), 214.0, theme)
            .child(
                Self::menu_item(
                    format!("thread-rename-{thread_id}"),
                    crate::i18n::text("重命名"),
                    "settings-edit",
                    theme,
                    can_rename,
                )
                .when(can_rename, |row| {
                    row.on_click(cx.listener(move |this, _, _, cx| {
                        this.start_rename(
                            RenameTarget::Thread(rename_id.clone()),
                            rename_title.clone(),
                            cx,
                        );
                    }))
                }),
            )
            .child(
                Self::menu_item(
                    format!("thread-context-pin-{thread_id}"),
                    if pinned {
                        crate::i18n::text("取消置顶")
                    } else {
                        crate::i18n::text("置顶")
                    },
                    "pin",
                    theme,
                    can_pin,
                )
                .when(can_pin, |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        this.store.set_thread_pinned(pin_id.clone(), !pinned);
                        this.thread_menu_id = None;
                    }))
                }),
            )
            .child(
                Self::menu_item(
                    format!("thread-context-archive-{thread_id}"),
                    if archived {
                        crate::i18n::text("取消归档")
                    } else {
                        crate::i18n::text("归档")
                    },
                    "archive",
                    theme,
                    can_archive,
                )
                .when(can_archive, |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        if archived {
                            this.store.unarchive_thread(archive_id.clone());
                        } else {
                            this.store.archive_thread(archive_id.clone());
                        }
                        this.thread_menu_id = None;
                    }))
                }),
            )
            .child(Self::menu_separator(theme))
            .child(
                Self::menu_item(
                    format!("thread-unassign-{thread_id}"),
                    crate::i18n::text("移至“无项目”"),
                    "folder",
                    theme,
                    can_move && thread.project_id.is_some(),
                )
                .when(can_move && thread.project_id.is_some(), |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        this.store
                            .move_thread_to_project(unassigned_id.clone(), None);
                        this.thread_menu_id = None;
                    }))
                }),
            );
        for project in &self.snapshot.projects {
            let move_id = thread_id.clone();
            let project_id = project.project_id.clone();
            let enabled = can_move && thread.project_id.as_deref() != Some(project_id.as_str());
            menu = menu.child(
                Self::menu_item(
                    format!("thread-move-{thread_id}-{project_id}"),
                    crate::i18n::format!("移至“{}”" => "Move to “{}”", project.name),
                    "folder",
                    theme,
                    enabled,
                )
                .when(enabled, |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        this.store
                            .move_thread_to_project(move_id.clone(), Some(project_id.clone()));
                        this.thread_menu_id = None;
                    }))
                }),
            );
        }
        menu.child(Self::menu_separator(theme)).child(
            Self::menu_item(
                format!("thread-delete-{thread_id}"),
                if deleting {
                    crate::i18n::text("再次点击以删除")
                } else {
                    crate::i18n::text("删除聊天")
                },
                "close-dialog",
                theme,
                can_delete,
            )
            .when(can_delete, |row| {
                row.on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm_or_delete(DeleteTarget::Thread(delete_id.clone()), cx);
                }))
            }),
        )
    }

    fn projects_options_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let can_list_threads = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadList);
        self.menu_shell("projects-options-menu", 176.0, theme)
            .child(
                Self::menu_item(
                    "projects-refresh",
                    crate::i18n::text("刷新"),
                    "settings-refresh",
                    theme,
                    true,
                )
                .on_click(cx.listener(|this, _, _, _| {
                    this.store.refresh_all();
                    this.projects_section_menu_open = false;
                })),
            )
            .child(
                Self::menu_item(
                    "projects-archived",
                    crate::i18n::text("已归档聊天"),
                    "archive",
                    theme,
                    can_list_threads,
                )
                .when(can_list_threads, |row| {
                    row.on_click(cx.listener(|this, _, _, cx| {
                        this.archived_open = true;
                        this.activity_open = false;
                        this.projects_section_menu_open = false;
                        cx.notify();
                    }))
                }),
            )
    }

    fn header(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let can_search = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadSearch);
        let can_list_threads = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadList);
        let new_project = self
            .selected_project_id
            .as_deref()
            .and_then(|project_id| {
                self.snapshot
                    .projects
                    .iter()
                    .find(|project| project.project_id == project_id)
            })
            .cloned();
        let new_conversation = div()
            .id("sidebar-new-conversation")
            .h(px(ROW_HEIGHT))
            .w_full()
            .px(px(8.0))
            .rounded(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .text_size(px(14.0))
            .text_color(theme.sidebar_text)
            .hover(move |style| style.bg(theme.sidebar_hover))
            .child(icon("new-chat", theme.sidebar_text.into()))
            .child(div().flex_1().child(crate::i18n::text("新对话")))
            .child(icon("quick-chat", theme.sidebar_text_muted.into()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.new_conversation(new_project.as_ref(), cx);
            }));
        let search = Self::nav_icon_button("sidebar-search", "search", theme)
            .when(!can_search, |button| button.opacity(0.4).cursor_default())
            .when(can_search, |button| {
                button.on_click(cx.listener(|this, _, _, cx| {
                    this.close_transient_menus(cx);
                    cx.emit(OpenChatSearch);
                    cx.notify();
                }))
            });
        let activity = Self::nav_icon_button("sidebar-activity", "activity", theme)
            .when(!can_list_threads, |button| {
                button.opacity(0.4).cursor_default()
            })
            .when(can_list_threads, |button| {
                button.on_click(cx.listener(|this, _, _, cx| {
                    this.activity_open = true;
                    this.archived_open = false;
                    cx.notify();
                }))
            });
        div()
            .flex_none()
            .child(
                div()
                    .h(px(38.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .relative()
                            .top(px(-2.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(17.0))
                            .font(crate::typography::brand_font(cx))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.sidebar_title_text)
                            .child("Codex")
                            .child(chevron(theme.sidebar_icon_muted.into()).size(px(14.0))),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(search)
                            .child(activity),
                    ),
            )
            // 保留修改前已由 CDP 校准的标题栏与新对话入口；未接入的入口
            // 只保留既有视觉，不制造对应业务数据或协议行为。
            .child(
                div()
                    .relative()
                    .top(px(1.0))
                    .px(px(8.0))
                    .h(px(31.0))
                    .child(new_conversation),
            )
    }

    fn subpage_header(&self, title: &'static str, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(46.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                Self::nav_icon_button("sidebar-back", "back", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.activity_open = false;
                        this.archived_open = false;
                        cx.notify();
                    },
                )),
            )
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.sidebar_title_text)
                    .child(title),
            )
    }

    /// Account row at the bottom of the sidebar. The label, avatar initials and
    /// sign-in state all come from the connection snapshot.
    fn profile_button(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let title = self
            .account
            .account_label()
            .or_else(|| {
                if self.account.is_signed_in() {
                    Some(crate::i18n::text("已登录").to_owned())
                } else if self.account.needs_login() {
                    Some(crate::i18n::text("登录").to_owned())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| crate::i18n::text("账户").to_owned());
        let initials = self.account.account_initials();
        div()
            .id("sidebar-profile")
            .w(px((self.width - 56.0).max(96.0)))
            .h(px(ACCOUNT_ROW_HEIGHT))
            .mx(px(8.0))
            .mb(px(8.0))
            .px(px(8.0))
            .rounded(px(ROW_RADIUS))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .text_size(px(14.0))
            .line_height(px(21.0))
            .text_color(theme.sidebar_text)
            .hover(move |style| style.bg(theme.sidebar_hover))
            .child(account_avatar(initials.as_deref(), theme))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(title),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.profile_menu_open = !this.profile_menu_open;
                if this.profile_menu_open {
                    // Opening the account page reads the account and the quota.
                    cx.emit(AccountAction(AccountIntent::Refresh));
                }
                cx.notify();
            }))
    }

    fn account_menu_header(&self, theme: Theme) -> Div {
        let title = self
            .account
            .account_label()
            .unwrap_or_else(|| crate::i18n::text("已登录").to_owned());
        let plan = self.account.plan_label().unwrap_or("").to_owned();
        div()
            .h(px(ACCOUNT_HEADER_HEIGHT))
            .px(px(8.0))
            .rounded(px(ROW_RADIUS))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(account_avatar(
                self.account.account_initials().as_deref(),
                theme,
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(ACCOUNT_HEADER_LINE))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(theme.sidebar_text_muted)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(plan),
                    ),
            )
    }

    fn account_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let mut menu = div()
            .id("account-menu")
            .w(px(ACCOUNT_MENU_WIDTH))
            .p(px(4.0))
            .rounded(px(20.0))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.profile_menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font_family(".SystemUIFont")
            .text_size(px(13.0))
            .text_color(theme.sidebar_text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        if self.account.is_signed_in() {
            menu = menu
                .child(self.account_menu_header(theme))
                .child(account_menu_separator(theme));
        }
        if let Some(error) = self.account.action_error.clone() {
            menu = menu.child(account_menu_notice(error, theme));
        }
        if self.account.login_pending() {
            menu = menu.child(account_menu_row(
                "account-login-pending",
                crate::i18n::text("登录中…"),
                "profile-lock",
                None,
                theme,
                false,
            ));
            if let Some(login_id) = self.account.login_id().map(str::to_owned) {
                menu = menu.child(
                    account_menu_row(
                        "account-login-cancel",
                        crate::i18n::text("取消登录"),
                        "close-dialog",
                        None,
                        theme,
                        true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.profile_menu_open = false;
                        cx.emit(AccountAction(AccountIntent::CancelLogin(login_id.clone())));
                        cx.notify();
                    })),
                );
            }
        } else if self.account.needs_login() {
            menu = menu.child(
                account_menu_row(
                    "account-sign-in",
                    crate::i18n::text("使用 ChatGPT 登录"),
                    "profile-lock",
                    None,
                    theme,
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.profile_menu_open = false;
                    cx.emit(AccountAction(AccountIntent::StartLogin));
                    cx.notify();
                })),
            );
        } else if self.account.account_unknown() {
            // No answer yet: say so and offer a real read instead of showing a
            // guessed account or a fabricated quota.
            menu = menu.child(account_menu_row(
                "account-unknown",
                crate::i18n::text("账户状态未知"),
                "profile-lock",
                None,
                theme,
                false,
            ));
            menu = menu.child(
                account_menu_row(
                    "account-retry",
                    crate::i18n::text("读取账户状态"),
                    "settings-refresh",
                    None,
                    theme,
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.profile_menu_open = false;
                    cx.emit(AccountAction(AccountIntent::Refresh));
                    cx.notify();
                })),
            );
        } else {
            menu = menu.child(account_menu_row(
                "profile-invite",
                crate::i18n::text("邀请好友"),
                "profile-invite",
                None,
                theme,
                false,
            ));
        }
        menu = menu.child(
            account_menu_row(
                "profile-settings",
                crate::i18n::text("设置"),
                "profile-settings",
                Some("⌘,".to_owned()),
                theme,
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.profile_menu_open = false;
                cx.emit(OpenSettings);
                cx.notify();
            })),
        );
        if self.account.is_signed_in() {
            menu = menu.child(
                account_menu_row(
                    "profile-logout",
                    crate::i18n::text("退出登录"),
                    "profile-logout",
                    None,
                    theme,
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.profile_menu_open = false;
                    cx.emit(AccountAction(AccountIntent::RequestLogout));
                    cx.notify();
                })),
            );
        }
        menu
    }
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.scroll_to_bottom) {
            self.scroll.scroll_to_bottom();
        }
        if std::mem::take(&mut self.rename_focus_pending) {
            self.rename_input.focus_handle(cx).focus(window, cx);
        }
        let theme = Theme::for_mode(self.mode);
        let mut sidebar = div()
            .id("sidebar")
            .relative()
            .w(px(self.width))
            .h_full()
            .flex_none()
            .pt(px(SIDEBAR_TITLEBAR_SAFE_TOP))
            .flex()
            .flex_col()
            .font_family(".SystemUIFont")
            // The app shell owns the sidebar material so it is composited only
            // once over the native blur. Painting the same translucent tint in
            // this content view would compound its alpha and make the settled
            // sidebar appear opaque in both light and dark themes.
            .text_color(theme.sidebar_text);
        if self.activity_open {
            sidebar = sidebar
                .child(self.subpage_header(crate::i18n::text("活动"), theme, cx))
                .child(self.activity_content(theme, window, cx));
        } else if self.archived_open {
            sidebar = sidebar
                .child(self.subpage_header(crate::i18n::text("已归档"), theme, cx))
                .child(self.archived_content(theme, window, cx));
        } else {
            sidebar = sidebar
                .child(self.header(theme, cx))
                .child(self.workspace_content(theme, window, cx))
                .child(self.profile_button(theme, cx));
        }

        if let Some(project_id) = self.project_menu_id.as_deref()
            && let Some(project) = self
                .snapshot
                .projects
                .iter()
                .find(|project| project.project_id == project_id)
        {
            let left = self.menu_origin.0.clamp(8.0, self.width - 198.0);
            let top = self.menu_origin.1.max(42.0);
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .child(self.project_context_menu(project, theme, cx)),
            ));
        }
        if let Some(thread_id) = self.thread_menu_id.as_deref()
            && let Some(thread) = self.snapshot.thread(thread_id)
        {
            let left = self.menu_origin.0.clamp(8.0, self.width - 222.0);
            let top = self.menu_origin.1.max(42.0);
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .child(self.thread_context_menu(thread, theme, cx)),
            ));
        }
        if self.projects_section_menu_open {
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(56.0))
                    .top(px(220.0))
                    .child(self.projects_options_menu(theme, cx)),
            ));
        }
        if self.profile_menu_open {
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(9.0))
                    .bottom(px(ACCOUNT_MENU_BOTTOM))
                    .child(self.account_menu(theme, cx)),
            ));
        }
        let hover_card_anchor = self.project_hover_card.as_deref().and_then(|project_id| {
            self.project_row_bounds
                .borrow()
                .get(project_id)
                .copied()
                .map(|bounds| (project_id.to_owned(), bounds))
        });
        if let Some((project_id, bounds)) = hover_card_anchor
            && let Some(project) = self
                .snapshot
                .projects
                .iter()
                .find(|project| project.project_id == project_id)
        {
            // Anchored to the row, not the pointer: the deferred draw keeps the
            // card outside the sidebar's own clip, exactly like the reference
            // tooltip that overhangs the sidebar into the main pane.
            let left = f32::from(bounds.origin.x + bounds.size.width) + PROJECT_HOVER_CARD_OFFSET_X;
            let top = f32::from(bounds.origin.y) + PROJECT_HOVER_CARD_OFFSET_Y;
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .child(self.project_hover_card(project, theme, cx)),
            ));
        }
        let thread_card_anchor = self.thread_hover_card.as_deref().and_then(|thread_id| {
            self.thread_row_bounds
                .borrow()
                .get(thread_id)
                .copied()
                .map(|bounds| (thread_id.to_owned(), bounds))
        });
        if let Some((thread_id, bounds)) = thread_card_anchor
            && let Some(thread) = self.snapshot.thread(&thread_id).cloned()
            && let Some(project) = self.project_for_thread(&thread_id)
        {
            // Anchored to the row, not the pointer, and deferred for the same
            // reason as the project card: the reference tooltip overhangs the
            // sidebar into the main pane.
            let left = f32::from(bounds.origin.x + bounds.size.width) + PROJECT_HOVER_CARD_OFFSET_X;
            let top = f32::from(bounds.origin.y) + PROJECT_HOVER_CARD_OFFSET_Y;
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .child(self.thread_hover_card(&thread, &project, theme, cx)),
            ));
        }
        sidebar
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };

    use async_channel::{Receiver, Sender};
    use gpui::{Bounds, MouseButton, TestApp, WindowBounds, WindowOptions, point, px, size};

    use super::{
        PROJECT_HOVER_CARD_CLOSE_DELAY, PROJECT_HOVER_CARD_DELAY, PROJECT_HOVER_HEADER_HEIGHT,
        ROW_HEIGHT, ROW_RADIUS, SIDEBAR_TITLEBAR_SAFE_TOP, SIDEBAR_WIDTH, SidebarView,
        THREAD_HOVER_CARD_CLOSE_DELAY, THREAD_HOVER_CARD_DELAY, compact_relative_time,
        home_shortened_path, hover_card_surface, project_hover_paths, project_hover_summary,
    };
    use crate::{
        agent::{
            AgentBackend, AgentCapabilities, AgentCapability, AgentConnectionEvent, AgentEvent,
            AgentModelCatalog, AgentPermissionProfile, AgentRequest, AgentRun, Page, PageRequest,
            Project, ThreadActivity, ThreadListRequest, ThreadSummary, WorkspaceResult,
        },
        git_review::ProjectRepo,
        theme::ThemeMode,
        workspace::WorkspaceStore,
    };

    fn response<T: Send + 'static>(value: T) -> Receiver<T> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.send_blocking(value);
        receiver
    }

    struct SidebarBackend {
        _events: Sender<AgentConnectionEvent>,
        event_receiver: Receiver<AgentConnectionEvent>,
        /// When set, the fixture lists one task that belongs to no project, the
        /// state the reference renders in the Recents section.
        projectless: bool,
    }

    impl SidebarBackend {
        fn new() -> Arc<Self> {
            Self::with_projectless(false)
        }

        fn projectless() -> Arc<Self> {
            Self::with_projectless(true)
        }

        fn with_projectless(projectless: bool) -> Arc<Self> {
            let (events, event_receiver) = async_channel::unbounded();
            Arc::new(Self {
                _events: events,
                event_receiver,
                projectless,
            })
        }
    }

    impl AgentBackend for SidebarBackend {
        fn capabilities(&self) -> AgentCapabilities {
            AgentCapabilities::new([
                AgentCapability::ProjectList,
                AgentCapability::ThreadList,
                AgentCapability::ThreadSectionList,
            ])
        }

        fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
            self.event_receiver.clone()
        }

        fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
            response(Err("not used".to_owned()))
        }

        fn load_permission_profiles(
            &self,
            _cwd: PathBuf,
        ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
            response(Err("not used".to_owned()))
        }

        fn update_thread_permissions(
            &self,
            _request: crate::agent::AgentThreadPermissionUpdate,
        ) -> Receiver<Result<crate::agent::AgentThreadPermissionResult, String>> {
            response(Err("not used".to_owned()))
        }

        fn list_projects(&self, _page: PageRequest) -> Receiver<WorkspaceResult<Page<Project>>> {
            response(Ok(Page::single(vec![Project {
                project_id: "project-stable-id".to_owned(),
                name: "Stable project".to_owned(),
                roots: vec![PathBuf::from("/tmp/stable-project")],
                created_at: 1,
                updated_at: 2,
                recency_at: Some(3),
                position: 0,
            }])))
        }

        fn list_threads(
            &self,
            request: ThreadListRequest,
        ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
            let threads = (!request.archived).then(|| ThreadSummary {
                thread_id: "thread-stable-id".to_owned(),
                title: "Stable thread".to_owned(),
                preview: "Stable thread".to_owned(),
                cwd: if self.projectless {
                    PathBuf::from("/tmp/loose-thread")
                } else {
                    PathBuf::from("/tmp/stable-project")
                },
                project_id: (!self.projectless).then(|| "project-stable-id".to_owned()),
                section: None,
                created_at: 1,
                updated_at: 2,
                recency_at: Some(3),
                activity: ThreadActivity::Idle,
            });
            response(Ok(Page::single(threads.into_iter().collect())))
        }

        fn list_thread_sections(
            &self,
            _page: PageRequest,
        ) -> Receiver<WorkspaceResult<Page<crate::agent::ThreadSection>>> {
            response(Ok(Page::single(Vec::new())))
        }

        fn run_prompt(&self, _request: AgentRequest) -> AgentRun {
            let (sender, receiver) = async_channel::unbounded::<AgentEvent>();
            drop(sender);
            AgentRun::new(receiver, None)
        }
    }

    #[test]
    fn store_backed_sidebar_selects_the_rendered_thread_by_stable_id() {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let preferences = std::env::temp_dir()
            .join(format!(
                "gpui-sidebar-ui-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ))
            .join("preferences.json");
        let store =
            WorkspaceStore::with_preferences_path(SidebarBackend::new(), preferences.clone());
        store.refresh_all();
        let deadline = Instant::now() + Duration::from_secs(3);
        while store.snapshot().loading.recent {
            assert!(Instant::now() < deadline, "sidebar fixture did not load");
            std::thread::sleep(Duration::from_millis(5));
        }

        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| SidebarView::new(ThemeMode::Dark, false, store, cx),
        );
        window.draw();
        // 46 px titlebar safe area + 38 px brand + 31 px new-chat row,
        // followed by the 3-row navigation block, project heading,
        // project row, and the first server-backed thread row.
        window.simulate_click(point(px(80.0), px(309.0)), MouseButton::Left);
        assert_eq!(
            window.read(|sidebar, _| sidebar.selected_thread_id.clone()),
            Some("thread-stable-id".to_owned())
        );
        assert_eq!(SIDEBAR_WIDTH, 275.0);
        assert_eq!(SIDEBAR_TITLEBAR_SAFE_TOP, 46.0);
        assert_eq!(ROW_HEIGHT, 30.0);
        assert_eq!(ROW_RADIUS, 12.5);
        if let Some(parent) = preferences.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
        if let Some(parent) = preferences.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
    #[test]
    fn account_row_click_opens_and_closes_the_menu_with_the_real_snapshot() {
        use crate::agent::{
            AgentAccount, AgentAccountPlanType, AgentAccountPresence, AgentAccountSnapshot,
        };
        use crate::components::account::{AccountLoadStatus, AccountView};

        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let preferences = std::env::temp_dir()
            .join(format!(
                "gpui-sidebar-account-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ))
            .join("preferences.json");
        let store =
            WorkspaceStore::with_preferences_path(SidebarBackend::new(), preferences.clone());
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| SidebarView::new(ThemeMode::Dark, false, store, cx),
        );
        window.update(|sidebar, _, cx| {
            sidebar.set_account_view(
                AccountView {
                    state: crate::agent::AgentAccountState {
                        generation: 1,
                        account: AgentAccountSnapshot {
                            requires_openai_auth: true,
                            account: AgentAccountPresence::Account(AgentAccount::Chatgpt {
                                email: Some("rita@example.com".into()),
                                plan_type: AgentAccountPlanType::Pro,
                            }),
                            auth_mode: None,
                            plan_type: Some(AgentAccountPlanType::Pro),
                        },
                        login: Default::default(),
                        rate_limits: Default::default(),
                    },
                    status: AccountLoadStatus::Loaded,
                    dialog: None,
                    action_error: None,
                },
                cx,
            );
        });
        window.draw();
        assert_eq!(
            window.read(|sidebar, _| sidebar.account.account_label()),
            Some("rita".to_owned())
        );

        // The account row sits at the bottom of the sidebar: 8px side inset,
        // 30px tall, 8px above the window edge.
        let row_y = 700.0 - 8.0 - 15.0;
        window.simulate_click(point(px(80.0), px(row_y)), MouseButton::Left);
        assert!(window.read(|sidebar, _| sidebar.profile_menu_open));
        // Clicking the account row again closes the menu it opened.
        window.simulate_click(point(px(80.0), px(row_y)), MouseButton::Left);
        assert!(!window.read(|sidebar, _| sidebar.profile_menu_open));
        if let Some(parent) = preferences.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn home_shortened_paths_match_the_reference_substitution() {
        for (path, expected) in [
            ("/Users/zp/Desktop/GPUI", "~/Desktop/GPUI"),
            ("/Users/zp", "~"),
            ("/Users/zp/", "~"),
            ("/Volumes/ExternalSSD/GPUI", "/Volumes/ExternalSSD/GPUI"),
            ("/home/zp/project", "/home/zp/project"),
            ("/Users", "/Users"),
        ] {
            assert_eq!(
                home_shortened_path(&PathBuf::from(path)),
                expected,
                "{path}"
            );
        }
    }

    #[test]
    fn project_hover_paths_skip_the_label_and_repository_row() {
        let project = Project {
            project_id: "project-1".to_owned(),
            name: "GPUI".to_owned(),
            roots: vec![
                PathBuf::from("/Users/zp/GPUI"),
                PathBuf::from("/Users/zp/GPUI"),
                PathBuf::from("GPUI"),
                PathBuf::from("/Volumes/ExternalSSD/GPUI"),
            ],
            created_at: 0,
            updated_at: 0,
            recency_at: None,
            position: 0,
        };
        let paths = project_hover_paths(&project, None);
        assert_eq!(
            paths
                .iter()
                .map(|(display, _)| display.as_str())
                .collect::<Vec<_>>(),
            vec!["~/GPUI", "/Volumes/ExternalSSD/GPUI"]
        );

        let repo = ProjectRepo {
            root: PathBuf::from("/Users/zp/GPUI"),
            label: "~/GPUI".to_owned(),
        };
        assert_eq!(project_hover_paths(&project, Some(&repo)).len(), 1);
    }

    #[test]
    fn project_hover_summary_counts_tasks_waiting_and_running() {
        fn thread(activity: ThreadActivity) -> ThreadSummary {
            ThreadSummary {
                thread_id: "thread".to_owned(),
                title: "Thread".to_owned(),
                preview: String::new(),
                cwd: PathBuf::from("/tmp/stable-project"),
                project_id: Some("project-stable-id".to_owned()),
                section: None,
                created_at: 0,
                updated_at: 0,
                recency_at: None,
                activity,
            }
        }
        let threads = [
            thread(ThreadActivity::Idle),
            thread(ThreadActivity::Active { flags: Vec::new() }),
            thread(ThreadActivity::Active {
                flags: vec![crate::agent::AgentThreadActiveFlag::WaitingOnUserInput],
            }),
        ];
        let refs = threads.iter().collect::<Vec<_>>();
        let summary = project_hover_summary(&refs);
        assert!(summary.starts_with("3"), "{summary}");
        assert_eq!(summary.matches(" · ").count(), 2, "{summary}");
        assert_eq!(summary.matches("1").count(), 2, "{summary}");
    }

    #[test]
    fn project_hover_card_follows_the_row_and_only_while_hovered() {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let preferences = std::env::temp_dir()
            .join(format!(
                "gpui-sidebar-hover-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ))
            .join("preferences.json");
        let store =
            WorkspaceStore::with_preferences_path(SidebarBackend::new(), preferences.clone());
        store.refresh_all();
        let deadline = Instant::now() + Duration::from_secs(3);
        while store.snapshot().loading.recent {
            assert!(Instant::now() < deadline, "sidebar fixture did not load");
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| SidebarView::new(ThemeMode::Dark, false, store, cx),
        );
        window.draw();

        // The rendered project row reports where it was painted, which is also
        // the anchor the card uses.
        let row = window.read(|sidebar, _| {
            *sidebar
                .project_row_bounds
                .borrow()
                .get("project-stable-id")
                .expect("the rendered row records its bounds")
        });
        let row_center = point(
            row.origin.x + row.size.width / 2.0,
            row.origin.y + row.size.height / 2.0,
        );

        window.simulate_mouse_move(point(px(700.0), px(400.0)));
        window.simulate_mouse_move(row_center);
        assert!(window.read(|sidebar, _| sidebar.hovered_project_id.is_some()));
        assert!(
            !window.read(|sidebar, _| sidebar.project_hover_card_is_open()),
            "the card waits for the reference's hover delay"
        );

        app.advance_clock(PROJECT_HOVER_CARD_DELAY);
        window.draw();
        assert!(window.read(|sidebar, _| sidebar.project_hover_card_is_open()));

        // The card is painted to the right of the row and stays open while the
        // pointer is inside it.
        window.simulate_mouse_move(point(
            row.origin.x + row.size.width + px(40.0),
            row.origin.y + px(PROJECT_HOVER_HEADER_HEIGHT + 12.0),
        ));
        assert!(!window.read(|sidebar, _| sidebar.hovered_project_id.is_some()));
        app.advance_clock(PROJECT_HOVER_CARD_CLOSE_DELAY * 2);
        window.draw();
        assert!(window.read(|sidebar, _| sidebar.project_hover_card_is_open()));

        // Leaving both the row and the card closes it.
        window.simulate_mouse_move(point(px(700.0), px(500.0)));
        app.advance_clock(PROJECT_HOVER_CARD_CLOSE_DELAY * 2);
        window.draw();
        assert!(!window.read(|sidebar, _| sidebar.project_hover_card_is_open()));

        if let Some(parent) = preferences.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn thread_hover_card_follows_the_row_and_only_for_project_tasks() {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let preferences = std::env::temp_dir()
            .join(format!(
                "gpui-sidebar-thread-hover-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ))
            .join("preferences.json");
        let store =
            WorkspaceStore::with_preferences_path(SidebarBackend::new(), preferences.clone());
        store.refresh_all();
        let deadline = Instant::now() + Duration::from_secs(3);
        while store.snapshot().loading.recent {
            assert!(Instant::now() < deadline, "sidebar fixture did not load");
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| SidebarView::new(ThemeMode::Dark, false, store, cx),
        );
        window.draw();

        let row = window.read(|sidebar, _| {
            *sidebar
                .thread_row_bounds
                .borrow()
                .get("thread-stable-id")
                .expect("the rendered task row records its bounds")
        });
        let row_center = point(
            row.origin.x + row.size.width / 2.0,
            row.origin.y + row.size.height / 2.0,
        );

        window.simulate_mouse_move(point(px(700.0), px(400.0)));
        window.simulate_mouse_move(row_center);
        assert!(
            !window.read(|sidebar, _| sidebar.thread_hover_card_is_open()),
            "the card waits for the reference's hover delay"
        );

        app.advance_clock(THREAD_HOVER_CARD_DELAY);
        window.draw();
        assert!(window.read(|sidebar, _| sidebar.thread_hover_card_is_open()));

        // The card hangs off the row's right edge and stays open while the
        // pointer is inside it.
        window.simulate_mouse_move(point(
            row.origin.x + row.size.width + px(40.0),
            row.origin.y + px(10.0),
        ));
        app.advance_clock(THREAD_HOVER_CARD_CLOSE_DELAY * 2);
        window.draw();
        assert!(window.read(|sidebar, _| sidebar.thread_hover_card_is_open()));

        window.simulate_mouse_move(point(px(700.0), px(500.0)));
        app.advance_clock(THREAD_HOVER_CARD_CLOSE_DELAY * 2);
        window.draw();
        assert!(!window.read(|sidebar, _| sidebar.thread_hover_card_is_open()));

        if let Some(parent) = preferences.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn projectless_tasks_never_open_a_hover_card() {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let preferences = std::env::temp_dir()
            .join(format!(
                "gpui-sidebar-projectless-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ))
            .join("preferences.json");
        let store = WorkspaceStore::with_preferences_path(
            SidebarBackend::projectless(),
            preferences.clone(),
        );
        store.refresh_all();
        let deadline = Instant::now() + Duration::from_secs(3);
        while store.snapshot().loading.recent {
            assert!(Instant::now() < deadline, "sidebar fixture did not load");
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| SidebarView::new(ThemeMode::Dark, false, store, cx),
        );
        window.draw();
        let row = window.read(|sidebar, _| {
            *sidebar
                .thread_row_bounds
                .borrow()
                .get("thread-stable-id")
                .expect("the rendered task row records its bounds")
        });
        window.simulate_mouse_move(point(
            row.origin.x + row.size.width / 2.0,
            row.origin.y + row.size.height / 2.0,
        ));
        app.advance_clock(THREAD_HOVER_CARD_DELAY * 2);
        window.draw();
        assert!(
            !window.read(|sidebar, _| sidebar.thread_hover_card_is_open()),
            "the reference prints no card for a task it cannot file under a project"
        );
        if let Some(parent) = preferences.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn compact_relative_time_matches_the_reference_buckets() {
        use chrono::{Local, TimeZone};
        let at = |day: u32, hour: u32, minute: u32| {
            Local
                .with_ymd_and_hms(2026, 9, day, hour, minute, 0)
                .single()
                .expect("local time")
                .timestamp_millis()
        };
        let on = |year: i32, month: u32, day: u32| {
            Local
                .with_ymd_and_hms(year, month, day, 12, 0, 0)
                .single()
                .expect("local time")
                .timestamp_millis()
        };
        let now = at(22, 12, 0);
        assert_eq!(compact_relative_time(now, now), "1m");
        assert_eq!(compact_relative_time(now, at(22, 11, 50)), "10m");
        assert_eq!(compact_relative_time(now, at(22, 9, 30)), "2h");
        // Whole hours win until a full day has passed, even across midnight.
        assert_eq!(compact_relative_time(now, at(21, 23, 0)), "13h");
        assert_eq!(compact_relative_time(now, at(21, 11, 0)), "1d");
        assert_eq!(compact_relative_time(now, at(18, 12, 0)), "4d");
        assert_eq!(compact_relative_time(now, at(12, 12, 0)), "1w");
        assert_eq!(compact_relative_time(now, on(2026, 8, 13)), "1mo");
        assert_eq!(compact_relative_time(now, on(2025, 7, 1)), "1y");
    }

    #[test]
    fn hover_card_surface_resolves_the_reference_tint_over_the_pane() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let theme = crate::theme::Theme::for_mode(mode);
            let surface = hover_card_surface(theme);
            assert_eq!(surface.a, 1.0, "the card cannot depend on its backdrop");
            let (over, under) = (theme.project_hover_surface, theme.surface);
            let expected = (over.r * over.a + under.r * (1.0 - over.a)) * 255.0;
            assert!(
                (surface.r * 255.0 - expected).abs() <= 0.5,
                "{mode:?} card surface {:?} should composite to {expected}",
                surface.r * 255.0
            );
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ThreadRowPlacement {
    pub(super) indented: bool,
    pub(super) pinned: bool,
    pub(super) archived: bool,
}
