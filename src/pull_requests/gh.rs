//! GitHub access for the Pull Requests page through the local `gh` CLI.
//!
//! The reference application reads pull requests through its own GitHub
//! connection; the native application uses the authenticated `gh` installation
//! the repository already requires for review and pull-request creation, so
//! both sides show the same data for the same account.

use std::{
    path::Path,
    process::Command,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::model::{
    Check, CheckState, Comment, Commit, GroupKind, ListTab, PullRequestDetail, PullRequestFilter,
    PullRequestGroup, PullRequestStatus, PullRequestSummary, ReviewThread, TimelineEntry,
    TimelineKind, User, relative_age,
};
use crate::git_review::{FileDiff, process};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(45);

const SEARCH_FIELDS: &str = r#"
  number
  title
  url
  isDraft
  state
  additions
  deletions
  headRefName
  baseRefName
  updatedAt
  author { login }
  repository { nameWithOwner }
"#;

fn gh(args: &[&str], input: Option<&[u8]>, cwd: Option<&Path>) -> Result<String> {
    let mut command = Command::new("gh");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = process::run(&mut command, input, COMMAND_TIMEOUT)
        .with_context(|| format!("无法运行 gh {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{}", stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn graphql(query: &str, variables: &[(&str, Value)]) -> Result<Value> {
    let mut args: Vec<String> = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
    ];
    for (name, value) in variables {
        let rendered = match value {
            Value::Number(number) => number.to_string(),
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        args.push("-F".to_string());
        args.push(format!("{name}={rendered}"));
    }
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    let raw = gh(&borrowed, None, None)?;
    let value: Value = serde_json::from_str(&raw).context("无法解析 gh 输出")?;
    if let Some(errors) = value.get("errors").and_then(Value::as_array)
        && let Some(first) = errors.first()
    {
        bail!(
            "{}",
            first
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("GitHub 查询失败")
        );
    }
    Ok(value)
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn u64_value(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn nodes(value: &Value, pointer: &str) -> Vec<Value> {
    value
        .pointer(pointer)
        .and_then(|value| value.get("nodes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn now() -> chrono::DateTime<chrono::Utc> {
    SystemTime::now().into()
}

fn user_from(value: &Value) -> Option<User> {
    let login = text(value, "login")?;
    Some(User {
        login,
        name: text(value, "name").filter(|name| !name.is_empty()),
        avatar_url: text(value, "avatarUrl"),
        is_self: value
            .get("viewerIsSelf")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn summary_from(node: &Value) -> PullRequestSummary {
    let state = text(node, "state").unwrap_or_default();
    let draft = node
        .get("isDraft")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let status = match (state.as_str(), draft) {
        ("MERGED", _) => PullRequestStatus::Merged,
        ("CLOSED", _) => PullRequestStatus::Closed,
        _ if draft => PullRequestStatus::Draft,
        _ => PullRequestStatus::Open,
    };
    PullRequestSummary {
        number: u64_value(node, "number"),
        title: text(node, "title").unwrap_or_default(),
        repository: text(
            node.get("repository").unwrap_or(&Value::Null),
            "nameWithOwner",
        )
        .unwrap_or_default(),
        head_branch: text(node, "headRefName").unwrap_or_default(),
        base_branch: text(node, "baseRefName").unwrap_or_default(),
        additions: u64_value(node, "additions"),
        deletions: u64_value(node, "deletions"),
        status,
        age: relative_age(&text(node, "updatedAt").unwrap_or_default(), now()),
        author: text(node.get("author").unwrap_or(&Value::Null), "login").unwrap_or_default(),
        url: text(node, "url").unwrap_or_default(),
    }
}

fn search(query: &str) -> Result<Vec<PullRequestSummary>> {
    let value = graphql(
        &format!(
            "query($searchQuery: String!) {{ search(query: $searchQuery, type: ISSUE, first: 50) {{ nodes {{ ... on PullRequest {{ {SEARCH_FIELDS} }} }} }} }}"
        ),
        &[("searchQuery", Value::String(query.to_string()))],
    )?;
    // Search can answer with nodes that are not readable pull requests (for
    // example a repository the token cannot see); they carry no repository and
    // must not reach the list.
    Ok(nodes(&value, "/data/search")
        .iter()
        .map(summary_from)
        .filter(|summary| !summary.repository.is_empty() && !summary.title.is_empty())
        .collect())
}

fn query_for(tab: ListTab, filter: &PullRequestFilter, relation: &str) -> String {
    let mut query = format!("is:pr {relation} archived:false");
    let status = filter.status.query();
    if !status.is_empty() {
        query.push(' ');
        query.push_str(status);
    }
    if let Some(repository) = filter.repository.as_deref() {
        query.push_str(" repo:");
        query.push_str(repository);
    }
    if tab == ListTab::Reviewing {
        query.push_str(" review-requested:@me");
    }
    query.push_str(" sort:updated-desc");
    query
}

/// Users available in the reviewer picker: assignable users of the repository,
/// filtered locally the way the reference dialog filters as you type.
pub fn search_users(repository: &str, query: &str) -> Result<Vec<User>> {
    let raw = gh(
        &[
            "api",
            &format!("search/users?q={query}+type:user"),
            "--jq",
            ".items[] | {login: .login, avatarUrl: .avatar_url}",
        ],
        None,
        None,
    )?;
    let mut users = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line)?;
        if let Some(user) = user_from(&value) {
            users.push(user);
        }
    }
    let _ = repository;
    Ok(users)
}

pub struct GhClient {
    cwd: Option<std::path::PathBuf>,
}

impl GhClient {
    pub fn new(cwd: Option<std::path::PathBuf>) -> Self {
        Self { cwd }
    }

    /// Lists the groups the reference app renders for a tab.
    pub fn list(&self, tab: ListTab, filter: &PullRequestFilter) -> Result<Vec<PullRequestGroup>> {
        let _ = &self.cwd;
        match tab {
            ListTab::All => {
                let mut groups = Vec::new();
                let reviewed = search(&query_for(tab, filter, "reviewed-by:@me"))?;
                if !reviewed.is_empty() {
                    groups.push(PullRequestGroup {
                        kind: GroupKind::PreviouslyReviewed,
                        items: reviewed,
                    });
                }
                let authored = search(&query_for(tab, filter, "author:@me"))?;
                if !authored.is_empty() {
                    groups.push(PullRequestGroup {
                        kind: GroupKind::Authored,
                        items: authored,
                    });
                }
                Ok(groups)
            }
            ListTab::Reviewing => {
                let items = search(&query_for(tab, filter, "review-requested:@me"))?;
                Ok(if items.is_empty() {
                    Vec::new()
                } else {
                    vec![PullRequestGroup {
                        kind: GroupKind::PreviouslyReviewed,
                        items,
                    }]
                })
            }
            ListTab::Authored => {
                let items = search(&query_for(tab, filter, "author:@me"))?;
                Ok(if items.is_empty() {
                    Vec::new()
                } else {
                    vec![PullRequestGroup {
                        kind: GroupKind::Authored,
                        items,
                    }]
                })
            }
        }
    }

    pub fn detail(&self, repository: &str, number: u64) -> Result<PullRequestDetail> {
        let (owner, name) = repository
            .split_once('/')
            .with_context(|| format!("仓库名称必须是 owner/name（收到 {repository:?}）"))?;
        let query = r#"
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      number title url isDraft state additions deletions headRefOid
      headRefName baseRefName createdAt updatedAt merged mergeable
      body
      author { login avatarUrl }
      reviewRequests(first: 20) { nodes { requestedReviewer { ... on User { login name avatarUrl } } } }
      reviews(first: 50) { nodes { id author { login avatarUrl } body submittedAt state } }
      comments(first: 50) { nodes { id databaseId author { login avatarUrl } body createdAt } }
      reviewThreads(first: 50) {
        nodes {
          id isResolved path line
          comments(first: 30) { nodes { id databaseId author { login avatarUrl } body createdAt } }
        }
      }
      commits(first: 100) {
        nodes { commit { oid messageHeadline committedDate author { name user { login } } } }
      }
      timelineItems(first: 100) {
        nodes {
          __typename
          ... on PullRequestCommit { commit { oid messageHeadline committedDate } }
          ... on IssueComment { id createdAt }
          ... on PullRequestReview { id submittedAt }
          ... on MergedEvent { actor { login } createdAt }
          ... on ClosedEvent { actor { login } createdAt }
          ... on ReopenedEvent { actor { login } createdAt }
        }
      }
      statusCheckRollup {
        contexts(first: 50) {
          nodes {
            __typename
            ... on CheckRun { name conclusion status detailsUrl }
            ... on StatusContext { context state targetUrl }
          }
        }
      }
    }
  }
}
"#;
        let value = graphql(
            query,
            &[
                ("owner", Value::String(owner.to_string())),
                ("name", Value::String(name.to_string())),
                ("number", Value::from(number)),
            ],
        )?;
        let pull = value
            .pointer("/data/repository/pullRequest")
            .cloned()
            .unwrap_or(Value::Null);
        if pull.is_null() {
            bail!("找不到该 Pull Request");
        }
        let mut summary = summary_from(&pull);
        summary.repository = repository.to_string();
        let author = user_from(pull.get("author").unwrap_or(&Value::Null)).unwrap_or(User {
            login: summary.author.clone(),
            name: None,
            avatar_url: None,
            is_self: false,
        });

        let mut requested_reviewers = Vec::new();
        for node in nodes(&pull, "/reviewRequests") {
            if let Some(user) = user_from(node.get("requestedReviewer").unwrap_or(&Value::Null)) {
                requested_reviewers.push(user);
            }
        }

        let mut comments = Vec::new();
        for node in nodes(&pull, "/comments") {
            comments.push(comment_from(&node, None));
        }
        for node in nodes(&pull, "/reviews") {
            let body = text(&node, "body").unwrap_or_default();
            if body.trim().is_empty() {
                continue;
            }
            let mut comment = comment_from(&node, None);
            comment.is_review = true;
            comment.can_quote = true;
            comments.push(comment);
        }
        comments.sort_by_key(|comment| comment.id.clone());

        let mut review_threads = Vec::new();
        for node in nodes(&pull, "/reviewThreads") {
            let thread_id = text(&node, "id").unwrap_or_default();
            let path = text(&node, "path").unwrap_or_default();
            let line = node
                .get("line")
                .and_then(Value::as_u64)
                .map(|line| line as u32);
            let resolved = node
                .get("isResolved")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let thread_comments: Vec<Comment> = nodes(&node, "/comments")
                .iter()
                .map(|comment| comment_from(comment, Some((&thread_id, &path, line))))
                .collect();
            review_threads.push(ReviewThread {
                id: thread_id,
                path,
                line,
                resolved,
                comments: thread_comments,
            });
        }

        let mut commits = Vec::new();
        for node in nodes(&pull, "/commits") {
            let commit = node.get("commit").cloned().unwrap_or(Value::Null);
            let author = commit
                .pointer("/author/user/login")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| text(commit.get("author").unwrap_or(&Value::Null), "name"))
                .unwrap_or_default();
            commits.push(Commit {
                sha: text(&commit, "oid").unwrap_or_default(),
                subject: text(&commit, "messageHeadline").unwrap_or_default(),
                author,
                age: relative_age(&text(&commit, "committedDate").unwrap_or_default(), now()),
            });
        }

        // The activity feed interleaves the commits, the state changes the feed
        // draws itself (`opened`), and the comments, ordered by timestamp.
        let mut timeline = Vec::new();
        timeline.push(TimelineEntry {
            kind: TimelineKind::Opened,
            at: text(&pull, "createdAt").unwrap_or_default(),
            actor: author.login.clone(),
            age: relative_age(&text(&pull, "createdAt").unwrap_or_default(), now()),
            comment_id: None,
            commit_sha: None,
            commit_subject: None,
        });
        for node in nodes(&pull, "/timelineItems") {
            let kind = text(&node, "__typename").unwrap_or_default();
            match kind.as_str() {
                "PullRequestCommit" => {
                    let commit = node.get("commit").cloned().unwrap_or(Value::Null);
                    timeline.push(TimelineEntry {
                        kind: TimelineKind::Commit,
                        at: text(&commit, "committedDate").unwrap_or_default(),
                        actor: String::new(),
                        age: relative_age(
                            &text(&commit, "committedDate").unwrap_or_default(),
                            now(),
                        ),
                        comment_id: None,
                        commit_sha: text(&commit, "oid"),
                        commit_subject: text(&commit, "messageHeadline"),
                    });
                }
                "IssueComment" | "PullRequestReview" => {
                    let id = text(&node, "id").unwrap_or_default();
                    if let Some(comment) = comments.iter().find(|comment| comment.id == id) {
                        let at = text(&node, "createdAt")
                            .or_else(|| text(&node, "submittedAt"))
                            .unwrap_or_default();
                        timeline.push(TimelineEntry {
                            kind: TimelineKind::Comment,
                            at,
                            actor: comment.author.clone(),
                            age: comment.age.clone(),
                            comment_id: Some(id),
                            commit_sha: None,
                            commit_subject: None,
                        });
                    }
                }
                "MergedEvent" | "ClosedEvent" | "ReopenedEvent" => {
                    // A merge closes the pull request too; the reference only
                    // draws the `merged` card for that pair.
                    if kind == "ClosedEvent"
                        && timeline
                            .iter()
                            .any(|entry| entry.kind == TimelineKind::Merged)
                    {
                        continue;
                    }
                    let event_kind = match kind.as_str() {
                        "MergedEvent" => TimelineKind::Merged,
                        "ReopenedEvent" => TimelineKind::Reopened,
                        _ => TimelineKind::Closed,
                    };
                    timeline.push(TimelineEntry {
                        kind: event_kind,
                        at: text(&node, "createdAt").unwrap_or_default(),
                        actor: node
                            .pointer("/actor/login")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        age: relative_age(&text(&node, "createdAt").unwrap_or_default(), now()),
                        comment_id: None,
                        commit_sha: None,
                        commit_subject: None,
                    });
                }
                _ => {}
            }
        }
        timeline.sort_by(|left, right| left.sort_key().cmp(right.sort_key()));
        // A feed that only has the synthetic `opened` entry means the timeline
        // query returned nothing useful; fall back to listing the comments.
        let timeline: Vec<TimelineEntry> = if timeline
            .iter()
            .any(|entry| entry.kind != TimelineKind::Opened)
        {
            timeline
        } else {
            Vec::new()
        };

        let mut checks = Vec::new();
        for node in nodes(&pull, "/statusCheckRollup/contexts") {
            let kind = text(&node, "__typename").unwrap_or_default();
            if kind == "CheckRun" {
                let conclusion = text(&node, "conclusion").unwrap_or_default();
                let status = text(&node, "status").unwrap_or_default();
                let state = match (status.as_str(), conclusion.as_str()) {
                    (_, "SUCCESS") => CheckState::Passed,
                    (_, "NEUTRAL" | "SKIPPED") => CheckState::Skipped,
                    (_, "FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED") => {
                        CheckState::Failed
                    }
                    _ => CheckState::Pending,
                };
                checks.push(Check {
                    name: text(&node, "name").unwrap_or_default(),
                    state,
                    details_url: text(&node, "detailsUrl"),
                });
            } else {
                let state = match text(&node, "state").unwrap_or_default().as_str() {
                    "SUCCESS" => CheckState::Passed,
                    "PENDING" | "EXPECTED" => CheckState::Pending,
                    "FAILURE" | "ERROR" => CheckState::Failed,
                    _ => CheckState::Skipped,
                };
                checks.push(Check {
                    name: text(&node, "context").unwrap_or_default(),
                    state,
                    details_url: text(&node, "targetUrl"),
                });
            }
        }

        Ok(PullRequestDetail {
            summary: summary.clone(),
            body: text(&pull, "body").unwrap_or_default(),
            requested_reviewers,
            reviewers: Vec::new(),
            comments,
            review_threads,
            timeline,
            checks,
            commits,
            author,
            mergeable: pull.get("mergeable").and_then(Value::as_str) == Some("MERGEABLE"),
            age: summary.age.clone(),
            created_age: relative_age(&text(&pull, "createdAt").unwrap_or_default(), now()),
            additions: summary.additions,
            deletions: summary.deletions,
            head_sha: text(&pull, "headRefOid").unwrap_or_default(),
        })
    }

    pub fn diff(&self, repository: &str, number: u64) -> Result<Vec<FileDiff>> {
        // Plain unified diff: `--patch` would emit a git format-patch mail
        // envelope (From/Date/Subject) that is not a diff at all.
        let raw = gh(
            &["pr", "diff", &number.to_string(), "--repo", repository],
            None,
            self.cwd.as_deref(),
        )?;
        let mut files = crate::git_review::parse_unified(&raw);
        files.retain(|file| !file.path.is_empty());
        Ok(files)
    }

    /// Whole file contents at the pull request head commit, used to fill in the
    /// context lines an `N unmodified lines` expander reveals.
    pub fn file_lines(&self, repository: &str, path: &str, sha: &str) -> Result<Vec<String>> {
        let raw = gh(
            &[
                "api",
                &format!("repos/{repository}/contents/{path}?ref={sha}"),
                "--jq",
                ".content",
            ],
            None,
            self.cwd.as_deref(),
        )?;
        let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
        if cleaned.is_empty() {
            bail!("文件内容不可用");
        }
        let bytes = decode_base64(&cleaned)?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(text.lines().map(str::to_string).collect())
    }

    pub fn set_status(
        &self,
        repository: &str,
        number: u64,
        status: PullRequestStatus,
    ) -> Result<()> {
        let number = number.to_string();
        let args: Vec<&str> = match status {
            PullRequestStatus::Draft => {
                vec!["pr", "ready", &number, "--repo", repository, "--undo"]
            }
            PullRequestStatus::Open => vec!["pr", "ready", &number, "--repo", repository],
            PullRequestStatus::Closed => vec!["pr", "close", &number, "--repo", repository],
            PullRequestStatus::Merged => vec!["pr", "reopen", &number, "--repo", repository],
        };
        gh(&args, None, self.cwd.as_deref())?;
        Ok(())
    }

    pub fn merge(&self, repository: &str, number: u64) -> Result<()> {
        let number = number.to_string();
        gh(
            &["pr", "merge", &number, "--repo", repository],
            None,
            self.cwd.as_deref(),
        )?;
        Ok(())
    }

    pub fn edit_title(&self, repository: &str, number: u64, title: &str) -> Result<()> {
        let number = number.to_string();
        gh(
            &[
                "pr", "edit", &number, "--repo", repository, "--title", title,
            ],
            None,
            self.cwd.as_deref(),
        )?;
        Ok(())
    }

    pub fn edit_body(&self, repository: &str, number: u64, body: &str) -> Result<()> {
        let number = number.to_string();
        gh(
            &["pr", "edit", &number, "--repo", repository, "--body", body],
            None,
            self.cwd.as_deref(),
        )?;
        Ok(())
    }

    pub fn comment(&self, repository: &str, number: u64, body: &str) -> Result<()> {
        let number = number.to_string();
        gh(
            &[
                "pr", "comment", &number, "--repo", repository, "--body", body,
            ],
            None,
            self.cwd.as_deref(),
        )?;
        Ok(())
    }

    pub fn request_reviewers(
        &self,
        repository: &str,
        number: u64,
        logins: &[String],
    ) -> Result<()> {
        let number = number.to_string();
        let mut args: Vec<&str> = vec!["pr", "edit", &number, "--repo", repository];
        for login in logins {
            args.push("--add-reviewer");
            args.push(login);
        }
        gh(&args, None, self.cwd.as_deref())?;
        Ok(())
    }

    pub fn edit_comment(&self, comment_id: &str, body: &str) -> Result<()> {
        let mutation = r#"
mutation($id: ID!, $body: String!) {
  updatePullRequestReviewComment(input: { pullRequestReviewCommentId: $id, body: $body }) {
    clientMutationId
  }
}"#;
        graphql(
            mutation,
            &[
                ("id", Value::String(comment_id.to_string())),
                ("body", Value::String(body.to_string())),
            ],
        )?;
        Ok(())
    }

    pub fn delete_comment(&self, comment_id: &str) -> Result<()> {
        let mutation = r#"
mutation($id: ID!) {
  deletePullRequestReviewComment(input: { id: $id }) { clientMutationId }
}"#;
        graphql(mutation, &[("id", Value::String(comment_id.to_string()))])?;
        Ok(())
    }

    pub fn reply_to_thread(&self, thread_id: &str, body: &str) -> Result<()> {
        let mutation = r#"
mutation($threadId: ID!, $body: String!) {
  addPullRequestReviewThreadReply(input: { pullRequestReviewThreadId: $threadId, body: $body }) {
    comment { id }
  }
}"#;
        graphql(
            mutation,
            &[
                ("threadId", Value::String(thread_id.to_string())),
                ("body", Value::String(body.to_string())),
            ],
        )?;
        Ok(())
    }

    pub fn resolve_thread(&self, thread_id: &str) -> Result<()> {
        let mutation = r#"
mutation($id: ID!) {
  resolveReviewThread(input: { threadId: $id }) { thread { id isResolved } }
}"#;
        graphql(mutation, &[("id", Value::String(thread_id.to_string()))])?;
        Ok(())
    }

    pub fn add_review_comment(
        &self,
        repository: &str,
        number: u64,
        path: &str,
        line: u32,
        body: &str,
    ) -> Result<()> {
        let (owner, name) = repository
            .split_once('/')
            .with_context(|| format!("仓库名称必须是 owner/name（收到 {repository:?}）"))?;
        let head = gh(
            &[
                "api",
                &format!("repos/{repository}/pulls/{number}"),
                "--jq",
                ".head.sha",
            ],
            None,
            self.cwd.as_deref(),
        )?;
        let mutation = r#"
mutation($pullRequestId: ID!, $body: String!, $path: String!, $line: Int!, $commitId: String!) {
  addPullRequestReviewThread(input: { pullRequestId: $pullRequestId, body: $body, path: $path, line: $line, commitOID: $commitId }) {
    thread { id }
  }
}"#;
        let pull_id = graphql(
            "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { id } } }",
            &[
                ("owner", Value::String(owner.to_string())),
                ("name", Value::String(name.to_string())),
                ("number", Value::from(number)),
            ],
        )?;
        let pull_id = pull_id
            .pointer("/data/repository/pullRequest/id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        graphql(
            mutation,
            &[
                ("pullRequestId", Value::String(pull_id)),
                ("body", Value::String(body.to_string())),
                ("path", Value::String(path.to_string())),
                ("line", Value::from(line)),
                ("commitId", Value::String(head.trim().to_string())),
            ],
        )?;
        Ok(())
    }
}

fn decode_base64(input: &str) -> Result<Vec<u8>> {
    // The GitHub contents API returns standard base64; `gh api --jq .content`
    // hands it back with newlines stripped by the caller.
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (index, byte) in TABLE.iter().enumerate() {
        lookup[*byte as usize] = index as u8;
    }
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        let value = lookup[byte as usize];
        if value == 255 {
            bail!("文件内容不是有效的 base64");
        }
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
        }
    }
    Ok(output)
}

fn comment_from(node: &Value, thread: Option<(&str, &str, Option<u32>)>) -> Comment {
    let author = node.get("author").unwrap_or(&Value::Null);
    let login = text(author, "login").unwrap_or_default();
    let body = text(node, "body").unwrap_or_default();
    let at = text(node, "createdAt")
        .or_else(|| text(node, "submittedAt"))
        .unwrap_or_default();
    Comment {
        id: text(node, "id").unwrap_or_default(),
        database_id: node.get("databaseId").and_then(Value::as_u64),
        author: login.clone(),
        avatar_url: text(author, "avatarUrl"),
        body: body.clone(),
        age: relative_age(&at, now()),
        at,
        is_review: thread.is_some(),
        path: thread.map(|(_, path, _)| path.to_string()),
        line: thread.and_then(|(_, _, line)| line),
        thread_id: thread.map(|(id, _, _)| id.to_string()),
        resolved: false,
        can_edit: !login.is_empty(),
        can_delete: !login.is_empty(),
        can_quote: !body.trim().is_empty(),
    }
}
