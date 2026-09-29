//! How a code review request reads in the conversation.
//!
//! The reference shows a review as the user's request ("Please review my
//! uncommitted changes") with a "Review mode" mark. `review/start` persists no
//! user message, only `enteredReviewMode` with the server's own label
//! ("current changes", "changes against 'main'"), so a restored review turn
//! derives the request from that label.

use crate::agent::AgentReviewTarget;

/// The request a review this client starts is shown as. `current_branch`
/// names the reviewed side of a base-branch comparison, as the reference's
/// "Please review changes on {from} against {to}".
pub(crate) fn review_request_text(
    target: &AgentReviewTarget,
    current_branch: Option<&str>,
) -> String {
    match target {
        AgentReviewTarget::UncommittedChanges => uncommitted_request(),
        AgentReviewTarget::BaseBranch { branch } => {
            let from = current_branch.unwrap_or("HEAD");
            crate::i18n::format!(
                "请审查 {from} 相对 {branch} 的更改" => "Please review changes on {from} against {branch}"
            )
        }
    }
}

fn uncommitted_request() -> String {
    crate::i18n::format!("请审查我未提交的更改" => "Please review my uncommitted changes")
}

/// The request of a restored or observed review turn, from the label of its
/// `enteredReviewMode` item. Labels this client does not know (a commit, free
/// instructions) are shown as the server wrote them.
pub(crate) fn review_request_from_label(label: &str) -> String {
    let label = label.trim();
    if label == "current changes" {
        return uncommitted_request();
    }
    if let Some(branch) = label
        .strip_prefix("changes against '")
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return crate::i18n::format!("请审查相对 {branch} 的更改" => "Please review changes against {branch}");
    }
    label.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_read_like_the_reference() {
        crate::i18n::set_language(crate::i18n::Language::English);
        assert_eq!(
            review_request_text(&AgentReviewTarget::UncommittedChanges, Some("main")),
            "Please review my uncommitted changes"
        );
        assert_eq!(
            review_request_text(
                &AgentReviewTarget::BaseBranch {
                    branch: "origin/main".into()
                },
                Some("feature")
            ),
            "Please review changes on feature against origin/main"
        );
        assert_eq!(
            review_request_text(
                &AgentReviewTarget::BaseBranch {
                    branch: "main".into()
                },
                None
            ),
            "Please review changes on HEAD against main"
        );
        crate::i18n::set_language(crate::i18n::Language::SimplifiedChinese);
    }

    #[test]
    fn labels_recorded_by_the_probe_map_back_to_requests() {
        crate::i18n::set_language(crate::i18n::Language::English);
        assert_eq!(
            review_request_from_label("current changes"),
            "Please review my uncommitted changes"
        );
        assert_eq!(
            review_request_from_label("changes against 'main'"),
            "Please review changes against main"
        );
        assert_eq!(
            review_request_from_label("commit 451c3d4: Greet the world"),
            "commit 451c3d4: Greet the world"
        );
        crate::i18n::set_language(crate::i18n::Language::SimplifiedChinese);
    }
}
