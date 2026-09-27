//! Pull request domain data shared by the list, detail, and diff views.
//!
//! Nothing here depends on GPUI: the values come from the local `gh` CLI and
//! are reduced into the shapes the Pull Requests page renders.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListTab {
    All,
    Reviewing,
    Authored,
}

impl ListTab {
    pub const ALL: [Self; 3] = [Self::All, Self::Reviewing, Self::Authored];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Reviewing => "Reviewing",
            Self::Authored => "Authored",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusFilter {
    All,
    Open,
    Merged,
    Closed,
}

impl StatusFilter {
    pub const ALL: [Self; 4] = [Self::All, Self::Open, Self::Merged, Self::Closed];

    /// The page opens on open pull requests, matching the reference app.
    pub fn initial() -> Self {
        Self::Open
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All states",
            Self::Open => "Open",
            Self::Merged => "Merged",
            Self::Closed => "Closed",
        }
    }

    /// GitHub search qualifier. The GraphQL search API accepts `state:open`
    /// but uses `is:merged` for merged pull requests, so `state:merged` (an
    /// empty result) marks merge state instead. Closed means closed without
    /// being merged, which needs `is:closed is:unmerged`.
    pub fn query(self) -> &'static str {
        match self {
            Self::All => "",
            Self::Open => "state:open",
            Self::Merged => "is:merged",
            Self::Closed => "is:closed is:unmerged",
        }
    }

    pub fn matches(self, status: PullRequestStatus) -> bool {
        match self {
            Self::All => true,
            Self::Open => matches!(status, PullRequestStatus::Open | PullRequestStatus::Draft),
            Self::Merged => status == PullRequestStatus::Merged,
            Self::Closed => status == PullRequestStatus::Closed,
        }
    }
}

impl fmt::Display for StatusFilter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestFilter {
    pub status: StatusFilter,
    pub repository: Option<String>,
}

impl Default for PullRequestFilter {
    fn default() -> Self {
        Self {
            status: StatusFilter::initial(),
            repository: None,
        }
    }
}

impl PullRequestFilter {
    pub fn is_active(&self) -> bool {
        self.status != StatusFilter::initial() || self.repository.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullRequestStatus {
    Draft,
    Open,
    Merged,
    Closed,
}

impl PullRequestStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Open => "Open",
            Self::Merged => "Merged",
            Self::Closed => "Closed",
        }
    }

    /// Status rows offer draft, ready, and closed transitions.
    pub fn selectable() -> [Self; 3] {
        [Self::Draft, Self::Open, Self::Closed]
    }

    pub fn is_merged(self) -> bool {
        self == Self::Merged
    }
}

/// Inbox sections, in the order the reference stacks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupKind {
    UserReviewRequested,
    TeamReviewRequested,
    PreviouslyReviewed,
    Authored,
    /// Text with an explicit relationship qualifier.
    Results,
}

impl GroupKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::UserReviewRequested => "Needs my review",
            Self::TeamReviewRequested => "Needs my team’s review",
            Self::PreviouslyReviewed => "Previously reviewed",
            Self::Authored => "Authored",
            Self::Results => "Results",
        }
    }

    /// The sections a tab stacks while it loads, in order.
    pub fn loading_sections(tab: ListTab, text: &str) -> &'static [GroupKind] {
        if super::gh::has_relationship_qualifier(text) {
            return &[Self::Results];
        }
        match tab {
            ListTab::All => &[
                Self::UserReviewRequested,
                Self::TeamReviewRequested,
                Self::PreviouslyReviewed,
                Self::Authored,
            ],
            ListTab::Reviewing => &[
                Self::UserReviewRequested,
                Self::TeamReviewRequested,
                Self::PreviouslyReviewed,
            ],
            ListTab::Authored => &[Self::Authored],
        }
    }

    /// Rows in every section but `Authored` show the author's avatar.
    pub fn shows_author_avatar(self) -> bool {
        self != Self::Authored
    }
}

/// The reference's inbox reduction (`sAn`): a pull request appears once, in
/// the first section that claims it. Team requests are the review requests not
/// addressed to the user directly, previously reviewed drops anything still
/// requested, and authored drops everything above it.
pub fn dedupe_sections(
    user_requested: Vec<PullRequestSummary>,
    requested: Vec<PullRequestSummary>,
    reviewed: Vec<PullRequestSummary>,
    authored: Option<Vec<PullRequestSummary>>,
) -> Vec<PullRequestGroup> {
    fn key(item: &PullRequestSummary) -> (String, u64) {
        (item.repository.to_lowercase(), item.number)
    }
    fn unique(items: Vec<PullRequestSummary>) -> Vec<PullRequestSummary> {
        let mut seen = std::collections::HashSet::new();
        items
            .into_iter()
            .filter(|item| seen.insert(key(item)))
            .collect()
    }
    let user_requested = unique(user_requested);
    let requested = unique(requested);
    let direct: std::collections::HashSet<_> = user_requested.iter().map(key).collect();
    let mut claimed = direct.clone();
    claimed.extend(requested.iter().map(key));
    let team: Vec<_> = requested
        .into_iter()
        .filter(|item| !direct.contains(&key(item)))
        .collect();
    let reviewed: Vec<_> = unique(reviewed)
        .into_iter()
        .filter(|item| !claimed.contains(&key(item)))
        .collect();
    claimed.extend(reviewed.iter().map(key));
    let mut groups = Vec::new();
    for (kind, items) in [
        (GroupKind::UserReviewRequested, user_requested),
        (GroupKind::TeamReviewRequested, team),
        (GroupKind::PreviouslyReviewed, reviewed),
    ] {
        if !items.is_empty() {
            groups.push(PullRequestGroup { kind, items });
        }
    }
    if let Some(authored) = authored {
        let authored: Vec<_> = unique(authored)
            .into_iter()
            .filter(|item| !claimed.contains(&key(item)))
            .collect();
        if !authored.is_empty() {
            groups.push(PullRequestGroup {
                kind: GroupKind::Authored,
                items: authored,
            });
        }
    }
    groups
}

/// Combined status of the head commit's checks, as the reference reduces a
/// pull request's check rollup.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CiStatus {
    #[default]
    None,
    Pending,
    Passing,
    Failing,
}

/// The glyph a row or header shows for a pull request. Closed and merged pull
/// requests show their state; open ones show their merge readiness, with the
/// dot colour of `ready`/`successful` (green), `in_progress` (yellow), and
/// `failing` (red).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusIcon {
    Draft,
    Merged,
    Closed,
    Failing,
    InProgress,
    Ready,
    Successful,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestSummary {
    pub number: u64,
    pub title: String,
    pub repository: String,
    pub head_branch: String,
    pub base_branch: String,
    pub additions: u64,
    pub deletions: u64,
    pub status: PullRequestStatus,
    pub age: String,
    pub author: String,
    /// `avatarUrl(size: 48)`, the image the reference rows draw at 16px.
    pub author_avatar_url: Option<String>,
    pub url: String,
    pub can_merge: bool,
    pub has_conflicts: bool,
    pub ci_status: CiStatus,
}

impl PullRequestSummary {
    /// The reference's `pullRequestStatusIconState`: closed and merged keep
    /// their state, drafts stay drafts, conflicts or failing checks win over
    /// everything else, and passing checks without a merge path read as
    /// `successful` rather than `ready`.
    pub fn status_icon(&self) -> StatusIcon {
        match self.status {
            PullRequestStatus::Merged => StatusIcon::Merged,
            PullRequestStatus::Closed => StatusIcon::Closed,
            PullRequestStatus::Draft => StatusIcon::Draft,
            PullRequestStatus::Open
                if self.has_conflicts || self.ci_status == CiStatus::Failing =>
            {
                StatusIcon::Failing
            }
            PullRequestStatus::Open if self.ci_status == CiStatus::Passing && !self.can_merge => {
                StatusIcon::Successful
            }
            PullRequestStatus::Open if self.can_merge => StatusIcon::Ready,
            PullRequestStatus::Open => StatusIcon::InProgress,
        }
    }

    /// Matches the search box: title, repository, and branch names.
    pub fn matches_query(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        self.title.to_lowercase().contains(&query)
            || self.repository.to_lowercase().contains(&query)
            || self.head_branch.to_lowercase().contains(&query)
            || self.base_branch.to_lowercase().contains(&query)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestGroup {
    pub kind: GroupKind,
    pub items: Vec<PullRequestSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_self: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Pending,
    Passed,
    Failed,
    Skipped,
}

impl CheckState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Passed => "Passed",
            Self::Failed => "Failed",
            Self::Skipped => "Skipped",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    pub details_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub age: String,
}

impl Commit {
    pub fn short_sha(&self) -> &str {
        self.sha.get(..7).unwrap_or(&self.sha)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub database_id: Option<u64>,
    pub url: String,
    pub author: String,
    pub avatar_url: Option<String>,
    pub body: String,
    pub age: String,
    /// ISO-8601 creation timestamp, used to order the activity feed.
    pub at: String,
    pub is_review: bool,
    /// Review comments keep the file and line they were written on.
    pub path: Option<String>,
    pub line: Option<u32>,
    pub thread_id: Option<String>,
    pub diff_hunk: String,
    pub resolved: bool,
    pub can_edit: bool,
    pub can_delete: bool,
    pub can_quote: bool,
}

#[derive(Clone, Debug)]
pub struct NewReviewComment {
    pub path: String,
    pub line: u32,
    pub old: bool,
    pub commit: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub line: Option<u32>,
    pub resolved: bool,
    pub comments: Vec<Comment>,
}

/// What one activity card shows. The reference interleaves the pull request's
/// commits and state changes with its comments, in timestamp order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimelineKind {
    Comment,
    Commit,
    Opened,
    Merged,
    Closed,
    Reopened,
}

/// One card of the activity feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineEntry {
    pub kind: TimelineKind,
    /// Timestamp the reference sorts by.
    pub at: String,
    pub actor: String,
    pub age: String,
    /// Comment cards render the `Comment` with this id.
    pub comment_id: Option<String>,
    pub commit_sha: Option<String>,
    pub commit_subject: Option<String>,
}

impl TimelineEntry {
    /// Sort key: ISO-8601 timestamps compare lexicographically.
    pub fn sort_key(&self) -> &str {
        &self.at
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestDetail {
    pub summary: PullRequestSummary,
    pub body: String,
    pub requested_reviewers: Vec<User>,
    pub reviewers: Vec<User>,
    pub comments: Vec<Comment>,
    pub review_threads: Vec<ReviewThread>,
    /// Activity cards in render order (events, commits, and comments).
    pub timeline: Vec<TimelineEntry>,
    pub checks: Vec<Check>,
    pub commits: Vec<Commit>,
    pub author: User,
    pub mergeable: bool,
    pub age: String,
    pub created_age: String,
    pub additions: u64,
    pub deletions: u64,
    /// Head commit, used to fetch file contents for expanded diff context.
    pub head_sha: String,
}

impl PullRequestDetail {
    pub fn comment(&self, id: &str) -> Option<&Comment> {
        self.comments
            .iter()
            .chain(
                self.review_threads
                    .iter()
                    .flat_map(|thread| &thread.comments),
            )
            .find(|comment| comment.id == id)
    }
}

/// Formats a GitHub timestamp the way the reference's compact relative time
/// does: at least one minute, whole hours below a day, then calendar days in
/// the local time zone bucketed into days, weeks (`/7`), months (`/30`), and
/// years (`/365`).
pub fn relative_age(updated: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    relative_age_in(updated, now, &chrono::Local)
}

pub fn relative_age_in<Tz: chrono::TimeZone>(
    updated: &str,
    now: chrono::DateTime<chrono::Utc>,
    zone: &Tz,
) -> String {
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(updated) else {
        return String::new();
    };
    let parsed = parsed.with_timezone(&chrono::Utc);
    let minutes = ((now - parsed).num_seconds().div_euclid(60)).max(1);
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let today = now.with_timezone(zone).date_naive();
    let then = parsed.with_timezone(zone).date_naive();
    let days = (today - then).num_days().max(1);
    if days < 7 {
        format!("{days}d")
    } else if days < 30 {
        format!("{}w", days / 7)
    } else if days < 365 {
        format!("{}mo", days / 30)
    } else {
        format!("{}y", days / 365)
    }
}

/// Groups thousands the way the reference's `Intl.NumberFormat` renders diff
/// stats in English (`1,120`).
pub fn format_count(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// Applies the status filter, repository filter, and search query to a loaded
/// list. The reference app reloads from the server on filter changes; the
/// local list is applied on top of whatever the server returned so search stays
/// instant.
pub fn filter_groups(
    groups: &[PullRequestGroup],
    filter: &PullRequestFilter,
    query: &str,
) -> Vec<PullRequestGroup> {
    let mut result = Vec::new();
    for group in groups {
        let items: Vec<PullRequestSummary> = group
            .items
            .iter()
            .filter(|item| filter.status.matches(item.status))
            .filter(|item| match filter.repository.as_deref() {
                Some(repository) => item.repository == repository,
                None => true,
            })
            .filter(|item| item.matches_query(query))
            .cloned()
            .collect();
        if items.is_empty() {
            continue;
        }
        result.push(PullRequestGroup {
            kind: group.kind,
            items,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_queries_match_the_search_api_vocabulary() {
        // `state:merged` silently returns nothing on the GraphQL search API.
        assert_eq!(StatusFilter::Merged.query(), "is:merged");
        assert_eq!(StatusFilter::Closed.query(), "is:closed is:unmerged");
        assert_eq!(StatusFilter::Open.query(), "state:open");
        assert_eq!(StatusFilter::All.query(), "");
    }

    fn summary(
        title: &str,
        repo: &str,
        branch: &str,
        status: PullRequestStatus,
    ) -> PullRequestSummary {
        PullRequestSummary {
            number: 1,
            title: title.to_string(),
            repository: repo.to_string(),
            head_branch: branch.to_string(),
            base_branch: "main".to_string(),
            additions: 1,
            deletions: 1,
            status,
            age: "1w".to_string(),
            author: "rita152".to_string(),
            author_avatar_url: None,
            url: "https://github.com/example/example/pull/1".to_string(),
            can_merge: false,
            has_conflicts: false,
            ci_status: CiStatus::None,
        }
    }

    #[test]
    fn search_matches_title_repository_and_branch() {
        let item = summary(
            "refactor(codex): deduplicate workspace page response decoding",
            "rita152/Echora",
            "refactor/workspace-page-decoding-20260911",
            PullRequestStatus::Draft,
        );
        assert!(item.matches_query(""));
        assert!(item.matches_query("DEDUPLICATE"));
        assert!(item.matches_query("echora"));
        assert!(item.matches_query("page-decoding"));
        assert!(!item.matches_query("nothing-here"));
    }

    #[test]
    fn status_filter_treats_drafts_as_open() {
        assert!(StatusFilter::Open.matches(PullRequestStatus::Draft));
        assert!(StatusFilter::Open.matches(PullRequestStatus::Open));
        assert!(!StatusFilter::Open.matches(PullRequestStatus::Merged));
        assert!(StatusFilter::Merged.matches(PullRequestStatus::Merged));
        assert!(StatusFilter::All.matches(PullRequestStatus::Closed));
    }

    #[test]
    fn filtering_drops_empty_groups_and_keeps_order() {
        let groups = vec![
            PullRequestGroup {
                kind: GroupKind::PreviouslyReviewed,
                items: vec![summary(
                    "reviewed",
                    "rita152/Echora",
                    "topic",
                    PullRequestStatus::Open,
                )],
            },
            PullRequestGroup {
                kind: GroupKind::Authored,
                items: vec![summary(
                    "authored",
                    "rita152/Echora",
                    "topic",
                    PullRequestStatus::Draft,
                )],
            },
        ];
        let filter = PullRequestFilter {
            status: StatusFilter::Open,
            repository: Some("rita152/Echora".to_string()),
        };
        let filtered = filter_groups(&groups, &filter, "auth");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].kind, GroupKind::Authored);
        assert_eq!(filtered[0].items.len(), 1);
        let filtered = filter_groups(&groups, &filter, "nomatch");
        assert!(filtered.is_empty());
    }

    #[test]
    fn relative_age_matches_the_reference_buckets() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-18T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let zone = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
        let age = |at: &str| relative_age_in(at, now, &zone);
        // Under a minute still reads as one minute.
        assert_eq!(age("2026-09-18T11:59:30Z"), "1m");
        assert_eq!(age("2026-09-18T11:42:00Z"), "18m");
        assert_eq!(age("2026-09-18T06:00:00Z"), "6h");
        // Days count calendar dates in the local zone (UTC+8 here), so 25
        // hours across one local midnight is one day.
        assert_eq!(age("2026-09-17T11:00:00Z"), "1d");
        assert_eq!(age("2026-09-15T12:00:00Z"), "3d");
        assert_eq!(age("2026-09-11T09:59:07Z"), "1w");
        // 36 days is five weeks in GitHub's words but `1mo` in the reference.
        assert_eq!(age("2026-08-13T12:00:00Z"), "1mo");
        assert_eq!(age("2025-01-01T00:00:00Z"), "1y");
    }

    #[test]
    fn sections_claim_each_pull_request_once() {
        let item = |number: u64| {
            let mut item = summary("t", "o/r", "b", PullRequestStatus::Open);
            item.number = number;
            item
        };
        let groups = dedupe_sections(
            vec![item(1)],
            vec![item(1), item(2)],
            vec![item(2), item(3)],
            Some(vec![item(3), item(4)]),
        );
        let numbers: Vec<(GroupKind, Vec<u64>)> = groups
            .iter()
            .map(|group| {
                (
                    group.kind,
                    group.items.iter().map(|item| item.number).collect(),
                )
            })
            .collect();
        assert_eq!(
            numbers,
            vec![
                (GroupKind::UserReviewRequested, vec![1]),
                (GroupKind::TeamReviewRequested, vec![2]),
                (GroupKind::PreviouslyReviewed, vec![3]),
                (GroupKind::Authored, vec![4]),
            ]
        );
    }

    #[test]
    fn counts_group_thousands() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(373), "373");
        assert_eq!(format_count(1120), "1,120");
        assert_eq!(format_count(1234567), "1,234,567");
    }

    #[test]
    fn status_icons_follow_merge_readiness() {
        let mut item = summary("t", "o/r", "b", PullRequestStatus::Open);
        assert_eq!(item.status_icon(), StatusIcon::InProgress);
        item.can_merge = true;
        assert_eq!(item.status_icon(), StatusIcon::Ready);
        item.ci_status = CiStatus::Failing;
        assert_eq!(item.status_icon(), StatusIcon::Failing);
        item.ci_status = CiStatus::Passing;
        item.can_merge = false;
        assert_eq!(item.status_icon(), StatusIcon::Successful);
        item.status = PullRequestStatus::Merged;
        assert_eq!(item.status_icon(), StatusIcon::Merged);
    }
}
