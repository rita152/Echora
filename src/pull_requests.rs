//! Pull request data for the Pull Requests page.
//!
//! The Codex app-server protocol has no pull-request method in either the
//! default or experimental schema (`artifacts/pull-requests-reference/protocol-scan.json`),
//! so this module reads GitHub through the authenticated local `gh` CLI that
//! the repository already requires for review and pull-request creation. It is
//! free of GPUI and of the UI adapters, matching the boundary `src/git_review/`
//! keeps for Git operations.

pub mod associations;
pub mod avatars;
pub mod detection;
mod gh;
mod model;

pub use gh::{
    BranchPullRequest, GhClient, PullRequestLiveState, pull_request_for_branch, pull_request_state,
    search_users,
};
pub use model::CiStatus;
pub use model::{
    ActivityEventKind, ActivityItem, CheckState, Comment, Commit, GroupKind, ListTab,
    NewReviewComment, PullRequestDetail, PullRequestFilter, PullRequestGroup, PullRequestStatus,
    PullRequestSummary, ReviewThread, StatusFilter, StatusIcon, User, filter_groups, format_count,
};

/// A GitHub timestamp as the reference's compact age ("2d", "5h").
pub fn age_since(timestamp: &str) -> String {
    model::relative_age(timestamp, chrono::Utc::now())
}
