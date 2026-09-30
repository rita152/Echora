//! Thread attachments: independently persisted records a server keeps next to
//! a thread, such as the pull request a chat produced or the worktree it runs
//! in.
//!
//! Each attachment is identified on its thread by `(attachment_type,
//! identity_key)`; adding the same pair again returns the existing record, so
//! two clients that attach the same pull request converge on one attachment.
//! Known types are decoded into typed payloads; any other type, or a known
//! type whose payload does not have the expected shape, keeps its raw JSON.

use serde_json::Value;

pub const PULL_REQUEST_ATTACHMENT_TYPE: &str = "pull_request";
pub const WORKTREE_ATTACHMENT_TYPE: &str = "worktree";

/// A pull request a thread is associated with. `url` is kept as the server
/// returned it; `root` and `head_branch` are the repository root and branch
/// the association was made from, when known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPullRequestAttachment {
    pub url: String,
    pub root: Option<String>,
    pub head_branch: Option<String>,
}

/// A worktree a thread runs in, and the workspace root it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentWorktreeAttachment {
    pub root: String,
    pub workspace_root: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AgentAttachmentContent {
    PullRequest(AgentPullRequestAttachment),
    Worktree(AgentWorktreeAttachment),
    /// An unknown type, or a known type whose payload did not decode. The
    /// type string and the payload are kept exactly as the server sent them.
    Other {
        attachment_type: String,
        payload: Value,
    },
}

/// One attachment exactly as the server persisted it. `created_at` is in Unix
/// seconds and never changes for the life of the record.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentThreadAttachment {
    pub id: String,
    pub identity_key: String,
    pub created_at: i64,
    pub content: AgentAttachmentContent,
}

impl AgentThreadAttachment {
    pub fn pull_request(&self) -> Option<&AgentPullRequestAttachment> {
        match &self.content {
            AgentAttachmentContent::PullRequest(pull_request) => Some(pull_request),
            _ => None,
        }
    }

    pub fn worktree(&self) -> Option<&AgentWorktreeAttachment> {
        match &self.content {
            AgentAttachmentContent::Worktree(worktree) => Some(worktree),
            _ => None,
        }
    }
}

/// Every attachment of one thread, read in one connection generation.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentThreadAttachments {
    pub generation: u64,
    pub thread_id: String,
    pub attachments: Vec<AgentThreadAttachment>,
}

/// Why an attachment operation produced no result. `Unsupported` means the
/// server has no attachment methods at all (an older CLI answers
/// method-not-found); callers fall back instead of reporting an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentAttachmentError {
    Unsupported,
    Failed(String),
}

impl std::fmt::Display for AgentAttachmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => {
                formatter.write_str(crate::i18n::text("当前 app-server 不支持线程附件"))
            }
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAttachmentAddOutcome {
    Created,
    /// The pair was already attached; the server kept the original record
    /// and its payload.
    Existing,
}

/// Adds one attachment on the generation the caller read from.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentAttachmentAddRequest {
    pub generation: u64,
    pub thread_id: String,
    pub attachment_type: String,
    pub identity_key: String,
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentAttachmentAdded {
    pub outcome: AgentAttachmentAddOutcome,
    pub attachment: AgentThreadAttachment,
}

/// Removes one attachment by its thread-local identity. Removing a pair that
/// is not attached succeeds without a change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAttachmentRemoveRequest {
    pub generation: u64,
    pub thread_id: String,
    pub attachment_type: String,
    pub identity_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAttachmentOperation {
    Created,
    Deleted,
}

/// A persisted attachment change on any thread. The server broadcasts it to
/// every connection, whatever threads they are subscribed to; it names the
/// record but carries no payload, so it is an invalidation signal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAttachmentUpdate {
    pub generation: u64,
    pub thread_id: String,
    pub attachment_id: String,
    pub attachment_type: String,
    pub identity_key: String,
    pub operation: AgentAttachmentOperation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPullRequestProvider {
    GitHub,
    GitLab,
}

/// A pull (or merge) request named by its web URL. `owner` is the GitLab
/// group path (it may contain `/`) for GitLab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPullRequestRef {
    pub provider: AgentPullRequestProvider,
    pub hostname: String,
    pub owner: String,
    pub repository: String,
    pub number: u64,
}

impl AgentPullRequestRef {
    /// Parses the URL of a GitHub pull request (`/owner/repo/pull/N` on any
    /// host) or of a GitLab merge request (gitlab.com, or `/-/merge_requests/N`
    /// on any host), the way the ChatGPT desktop app recognises them.
    pub fn parse(url: &str) -> Option<Self> {
        let parsed = url::Url::parse(url).ok()?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return None;
        }
        let host = parsed.host_str()?.to_owned();
        if host == "gitlab.com" {
            return gitlab_merge_request(url, &parsed, &host);
        }
        if let Some(github) = github_pull_request(&parsed, &host) {
            return Some(github);
        }
        gitlab_merge_request(url, &parsed, &host)
    }

    /// The canonical web URL: lowercase host, the owner and repository as
    /// written, and nothing after the number.
    pub fn canonical_url(&self) -> String {
        let host = normalize_host(&self.hostname);
        match self.provider {
            AgentPullRequestProvider::GitHub if host != "gitlab.com" => format!(
                "https://{host}/{}/{}/pull/{}",
                encode_uri_component(&self.owner),
                encode_uri_component(&self.repository),
                self.number
            ),
            _ => {
                let group = self
                    .owner
                    .split('/')
                    .map(encode_uri_component)
                    .collect::<Vec<_>>()
                    .join("/");
                format!(
                    "https://{host}/{group}/{}/-/merge_requests/{}",
                    encode_uri_component(&self.repository),
                    self.number
                )
            }
        }
    }

    /// The attachment identity key, byte for byte the reference's
    /// `JSON.stringify([host, owner, repository, number])` with host, owner and
    /// repository lowercased. Both applications therefore attach the same pull
    /// request under one key.
    pub fn identity_key(&self) -> String {
        serde_json::to_string(&serde_json::json!([
            normalize_host(&self.hostname),
            self.owner.to_lowercase(),
            self.repository.to_lowercase(),
            self.number
        ]))
        .expect("strings and a number always serialize")
    }

    /// Same host, owner, repository and number, ignoring case.
    pub fn same_as(&self, other: &Self) -> bool {
        self.identity_key() == other.identity_key()
    }
}

fn normalize_host(host: &str) -> String {
    host.trim().to_lowercase().trim_end_matches('.').to_owned()
}

fn decode_segment(segment: &str) -> Option<String> {
    percent_decode(segment)
}

fn github_pull_request(parsed: &url::Url, host: &str) -> Option<AgentPullRequestRef> {
    let segments: Vec<&str> = parsed.path().split('/').filter(|s| !s.is_empty()).collect();
    let [owner, repository, kind, number, ..] = segments.as_slice() else {
        return None;
    };
    if *kind != "pull" {
        return None;
    }
    let owner = decode_segment(owner)?;
    let repository = decode_segment(repository)?;
    let has_separator = |text: &str| text.chars().any(|c| c.is_whitespace() || c == '/');
    if has_separator(&owner) || has_separator(&repository) {
        return None;
    }
    let owner = owner.trim().to_owned();
    let repository = repository.trim().to_owned();
    let hostname = normalize_host(host);
    let number = parse_number(number.trim())?;
    if owner.is_empty() || repository.is_empty() || hostname.is_empty() {
        return None;
    }
    Some(AgentPullRequestRef {
        provider: if hostname == "gitlab.com" {
            AgentPullRequestProvider::GitLab
        } else {
            AgentPullRequestProvider::GitHub
        },
        hostname,
        owner,
        repository,
        number,
    })
}

fn gitlab_merge_request(raw: &str, parsed: &url::Url, host: &str) -> Option<AgentPullRequestRef> {
    // Only `https://<host>` origins, without credentials, port or backslashes.
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || raw.contains('\\')
    {
        return None;
    }
    let path = parsed.path();
    let marker = path.find("/-/merge_requests/")?;
    let (project, rest) = path.split_at(marker);
    let digits: String = rest["/-/merge_requests/".len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let after = &rest["/-/merge_requests/".len() + digits.len()..];
    if digits.is_empty() || !(after.is_empty() || after.starts_with('/')) {
        return None;
    }
    let mut segments = Vec::new();
    for segment in project.trim_start_matches('/').split('/') {
        let decoded = decode_segment(segment)?;
        if decoded.trim().is_empty() || decoded == "." || decoded == ".." {
            return None;
        }
        segments.push(decoded);
    }
    if segments.len() < 2 {
        return None;
    }
    let repository = segments.pop()?;
    Some(AgentPullRequestRef {
        provider: AgentPullRequestProvider::GitLab,
        hostname: normalize_host(host),
        owner: segments.join("/"),
        repository,
        number: parse_number(&digits)?,
    })
}

fn parse_number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let number: u64 = text.parse().ok()?;
    (1..=(1u64 << 53) - 1).contains(&number).then_some(number)
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// JavaScript's `encodeURIComponent`.
fn encode_uri_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests;
