//! `/review`: the reference's Code review slash command.
//!
//! The command opens a submenu, as the reference's `review-mode` command does:
//! "Review uncommitted changes", then a "Review against a base branch"
//! section that lists the repository's default target and its recent
//! branches (`git_review::review_branches`). Typing after the remaining `/`
//! filters the rows. Choosing one starts `review/start` in this chat, or, when
//! Git → Review delivery is Detached and this chat already exists, hands the
//! target to the app, which starts it in a new chat of the same project. The
//! review pane then opens on the reviewed diff.

use gpui::{Context, EventEmitter};

use super::{ComposerView, ConversationChanged, toast::ToastKind};
use crate::agent::{AgentReviewRequest, AgentReviewTarget, AgentThreadTarget};

/// Asks the host to start this review in a new chat of the same project.
pub struct StartDetachedReview(pub AgentReviewTarget);
/// A review started; the host shows the diff it looks at.
pub struct CodeReviewStarted(pub crate::git_review::Scope);

impl EventEmitter<StartDetachedReview> for ComposerView {}
impl EventEmitter<CodeReviewStarted> for ComposerView {}

/// The base branches of the submenu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum ReviewBranches {
    #[default]
    Loading,
    Loaded(Vec<String>),
    Failed,
}

/// One selectable submenu row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReviewRow {
    Uncommitted,
    Branch(String),
}

impl ReviewRow {
    pub(crate) fn title(&self) -> String {
        match self {
            Self::Uncommitted => {
                crate::i18n::format!("审查未提交的更改" => "Review uncommitted changes")
            }
            Self::Branch(branch) => branch.clone(),
        }
    }

    fn target(&self) -> AgentReviewTarget {
        match self {
            Self::Uncommitted => AgentReviewTarget::UncommittedChanges,
            Self::Branch(branch) => AgentReviewTarget::BaseBranch {
                branch: branch.clone(),
            },
        }
    }
}

impl ComposerView {
    pub fn set_review_delivery(
        &mut self,
        delivery: crate::workspace::ReviewDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.review_delivery != delivery {
            self.review_delivery = delivery;
            cx.notify();
        }
    }

    pub fn conversation_cwd(&self) -> std::path::PathBuf {
        self.conversation.cwd.clone()
    }

    pub fn conversation_project_id(&self) -> Option<crate::agent::ProjectId> {
        self.conversation.project_id.clone()
    }

    /// `/review` needs a repository to compare, and a side chat has no
    /// workspace of its own.
    pub(super) fn review_available(&self) -> bool {
        !self.side_chat
            && self
                .workspace
                .checkout
                .as_ref()
                .is_some_and(crate::git_review::Checkout::is_repository)
    }

    /// Opens the submenu and (re)reads its branches in the background.
    pub(super) fn open_review_menu(&mut self, cx: &mut Context<Self>) {
        self.load_review_branches(cx);
        cx.notify();
    }

    pub(crate) fn load_review_branches(&mut self, cx: &mut Context<Self>) {
        self.review_branches = ReviewBranches::Loading;
        self.review_branches_cycle = self.review_branches_cycle.wrapping_add(1);
        #[cfg(not(test))]
        {
            let cycle = self.review_branches_cycle;
            let cwd = self.conversation.cwd.clone();
            cx.spawn(async move |this, cx| {
                let branches = cx
                    .background_executor()
                    .spawn(async move { crate::git_review::review_branches(&cwd) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.review_branches_cycle == cycle {
                        this.set_review_branches(
                            branches.map_or(ReviewBranches::Failed, ReviewBranches::Loaded),
                            cx,
                        );
                    }
                });
            })
            .detach();
        }
        #[cfg(test)]
        let _ = cx;
    }

    pub(crate) fn set_review_branches(&mut self, branches: ReviewBranches, cx: &mut Context<Self>) {
        self.review_branches = branches;
        if let Some(menu) = self.slash_menu.as_mut() {
            menu.highlighted = 0;
        }
        cx.notify();
    }

    /// The submenu's rows for the text typed after `/`: case-insensitive
    /// substring matches, the uncommitted row first.
    pub(crate) fn review_rows(&self, cx: &gpui::App) -> Vec<ReviewRow> {
        let query = self.slash_query(cx).unwrap_or_default();
        let matches = |title: &str| query.is_empty() || title.to_lowercase().contains(&query);
        let mut rows = Vec::new();
        if matches(&ReviewRow::Uncommitted.title()) {
            rows.push(ReviewRow::Uncommitted);
        }
        if let ReviewBranches::Loaded(branches) = &self.review_branches {
            rows.extend(
                branches
                    .iter()
                    .filter(|branch| matches(branch))
                    .map(|branch| ReviewRow::Branch(branch.clone())),
            );
        }
        rows
    }

    /// Runs the highlighted or clicked submenu row.
    pub(crate) fn select_review_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.review_rows(cx).into_iter().nth(index) else {
            return;
        };
        self.slash_menu = None;
        self.remove_slash_token(cx);
        self.start_code_review(row.target(), cx);
    }

    /// Starts a review of `target` where Review delivery says.
    pub(crate) fn start_code_review(&mut self, target: AgentReviewTarget, cx: &mut Context<Self>) {
        if self.is_running() {
            self.show_toast(
                ToastKind::Danger,
                crate::i18n::format!("聊天期间无法开始代码审查" => "Code review is unavailable while a chat is in progress"),
                cx,
            );
            return;
        }
        if self.review_delivery == crate::workspace::ReviewDelivery::Detached
            && self.conversation.thread_id.is_some()
        {
            cx.emit(StartDetachedReview(target));
            return;
        }
        if self.conversation.thread_id.is_none()
            && (self.permission_catalog_loading || self.permission_catalog_error.is_some())
        {
            self.show_toast(ToastKind::Danger, failed_to_start(), cx);
            return;
        }
        if self.conversation.selected_model.is_empty() {
            self.show_toast(ToastKind::Danger, failed_to_start(), cx);
            return;
        }
        let branch = self
            .workspace
            .checkout
            .as_ref()
            .and_then(|checkout| checkout.branch().map(str::to_owned));
        let request_text = crate::conversation::review_request_text(&target, branch.as_deref());
        let scope = match &target {
            AgentReviewTarget::BaseBranch { branch } => {
                crate::git_review::Scope::Branch(branch.clone())
            }
            _ => crate::git_review::Scope::Uncommitted,
        };
        let cycle = self.conversation.begin_review(&request_text);
        self.submission_error = None;
        self.focus_prompt_pending = true;
        let model = self.conversation.selected_model.clone();
        self.conversation.actual_model = Some(model.clone());
        self.conversation.model_status = None;
        self.conversation.safety_buffering = false;
        let run = self.backend.run_review(AgentReviewRequest {
            thread: AgentThreadTarget {
                thread_id: self.conversation.thread_id.clone(),
                cwd: self.conversation.cwd.clone(),
                project_id: self.conversation.project_id.clone(),
                model,
                service_tier: self.conversation.selected_service_tier.clone(),
                permission_mode: self.selected_agent_permission_mode(),
            },
            target,
        });
        let (receiver, interrupt) = run.into_parts();
        self.conversation.active_turn = interrupt;
        self.review_start_cycle = Some(cycle);
        self.consume_agent_events(receiver, cycle, cx);
        cx.emit(CodeReviewStarted(scope));
        cx.emit(ConversationChanged);
        cx.notify();
    }

    /// Starts `target` as soon as this conversation can: a chat opened for a
    /// detached review still loads its model and permission settings.
    pub fn queue_code_review(&mut self, target: AgentReviewTarget, cx: &mut Context<Self>) {
        self.pending_review = Some(target);
        self.start_pending_review(cx);
    }

    /// Called whenever the model or permission catalog finishes loading.
    pub(super) fn start_pending_review(&mut self, cx: &mut Context<Self>) {
        if self.pending_review.is_none()
            || self.permission_catalog_loading
            || (self.conversation.selected_model.is_empty()
                && self.conversation.model_catalog_error.is_none())
        {
            return;
        }
        if let Some(target) = self.pending_review.take() {
            self.start_code_review(target, cx);
        }
    }

    /// The reference's toast when a review never started. Called when the
    /// review's run ends before its turn was accepted.
    pub(super) fn review_start_settled(&mut self, accepted: bool, cx: &mut Context<Self>) {
        if self.review_start_cycle.take().is_some() && !accepted {
            self.show_toast(ToastKind::Danger, failed_to_start(), cx);
        }
    }
}

fn failed_to_start() -> String {
    crate::i18n::format!("无法开始代码审查" => "Failed to start code review")
}
