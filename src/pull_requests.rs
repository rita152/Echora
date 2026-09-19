//! Pull request data for the Pull Requests page.
//!
//! The Codex app-server protocol has no pull-request method in either the
//! default or experimental schema (`artifacts/pull-requests-reference/protocol-scan.json`),
//! so this module reads GitHub through the authenticated local `gh` CLI that
//! the repository already requires for review and pull-request creation. It is
//! free of GPUI and of the UI adapters, matching the boundary `src/git_review/`
//! keeps for Git operations.

mod gh;
mod model;

pub use gh::{GhClient, search_users};
pub use model::{
    CheckState, Comment, GroupKind, ListTab, PullRequestDetail, PullRequestFilter,
    PullRequestGroup, PullRequestStatus, PullRequestSummary, StatusFilter, TimelineEntry,
    TimelineKind, User, filter_groups,
};
