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
    pub fn repository_label(&self) -> &str {
        self.repository.as_deref().unwrap_or("All repositories")
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupKind {
    ReviewRequested,
    PreviouslyReviewed,
    Authored,
}

impl GroupKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReviewRequested => "Review requested",
            Self::PreviouslyReviewed => "Previously reviewed",
            Self::Authored => "Authored",
        }
    }
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
    pub url: String,
}

impl PullRequestSummary {
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

/// Formats a GitHub timestamp the way the reference app does: the largest
/// single unit, with `w`/`d`/`h`/`m` suffixes and `now` for anything under a
/// minute.
pub fn relative_age(updated: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(updated)
        .ok()
        .map(|value| value.with_timezone(&chrono::Utc));
    let Some(parsed) = parsed else {
        return String::new();
    };
    let seconds = (now - parsed).num_seconds().max(0);
    let minutes = seconds / 60;
    if minutes < 1 {
        return "now".to_string();
    }
    let hours = minutes / 60;
    if hours < 1 {
        return format!("{minutes}m");
    }
    let days = hours / 24;
    if days < 1 {
        return format!("{hours}h");
    }
    let weeks = days / 7;
    if weeks < 1 {
        return format!("{days}d");
    }
    if weeks < 52 {
        return format!("{weeks}w");
    }
    format!("{}y", weeks / 52)
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
            url: "https://github.com/example/example/pull/1".to_string(),
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
    fn relative_age_uses_the_largest_unit() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-18T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(relative_age("2026-09-18T11:59:30Z", now), "now");
        assert_eq!(relative_age("2026-09-18T11:42:00Z", now), "18m");
        assert_eq!(relative_age("2026-09-18T06:00:00Z", now), "6h");
        assert_eq!(relative_age("2026-09-15T12:00:00Z", now), "3d");
        assert_eq!(relative_age("2026-09-11T09:59:07Z", now), "1w");
        assert_eq!(relative_age("2025-01-01T00:00:00Z", now), "1y");
    }
}
