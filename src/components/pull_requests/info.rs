//! Additional page actions retain the existing review and full-screen entry points.
use super::PullRequestsView;
use crate::{components::icons::icon, pull_requests::PullRequestStatus};
use gpui::{Div, prelude::*, px};
impl PullRequestsView {
    pub(super) fn detail_actions_menu(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.theme();
        let fix = cx.entity();
        let fullscreen = cx.entity();
        let refresh = cx.entity();
        let mut menu = Self::solid_menu_surface("pr-detail-actions-menu", theme)
            .w(px(224.0))
            .rounded(px(12.0));
        if self.selected.as_ref().is_some_and(|r| !r.is_issue()) {
            menu = menu.child(
                Self::menu_row("pr-fix-findings", theme, false, self.detail.is_none())
                    .aria_label("Fix findings")
                    .child(icon("review-send", theme.text.into()).size(px(16.0)))
                    .child("Fix findings")
                    .on_click(move |_, _, cx| {
                        fix.update(cx, |v, cx| {
                            v.dismiss_menus(cx);
                            if let Some(detail) = &v.detail {
                                let prompt = findings_prompt(detail);
                                cx.emit(super::OpenChatForPullRequest { prompt });
                            }
                        })
                    }),
            )
        }
        if let Some(item) = &self.selected
            && item.status != PullRequestStatus::Merged
            && self.detail.as_ref().is_some_and(|d| d.author.is_self)
        {
            let close = cx.entity();
            let status = if item.status == PullRequestStatus::Closed {
                PullRequestStatus::Open
            } else {
                PullRequestStatus::Closed
            };
            let label = if status == PullRequestStatus::Closed {
                "Close"
            } else {
                "Reopen"
            };
            menu = menu.child(
                Self::menu_row("pr-close-item", theme, false, self.mutation_pending)
                    .aria_label(label)
                    .child(label)
                    .on_click(move |_, _, cx| {
                        close.update(cx, |v, cx| {
                            v.dismiss_menus(cx);
                            if !v.mutation_pending {
                                v.set_status(status, cx);
                            }
                        })
                    }),
            );
        }
        menu.child(Self::menu_rule(theme))
            .child(
                Self::menu_row("pr-fullscreen", theme, false, false)
                    .aria_label(if self.fullscreen {
                        "Exit full screen"
                    } else {
                        "Enter full screen"
                    })
                    .child(if self.fullscreen {
                        "Exit full screen"
                    } else {
                        "Enter full screen"
                    })
                    .on_click(move |_, _, cx| {
                        fullscreen.update(cx, |v, cx| {
                            v.dismiss_menus(cx);
                            v.toggle_fullscreen(cx);
                        })
                    }),
            )
            .child(
                Self::menu_row("pr-refresh-detail", theme, false, self.detail_loading)
                    .aria_label("Refresh details")
                    .child("Refresh details")
                    .on_click(move |_, _, cx| {
                        refresh.update(cx, |v, cx| {
                            v.dismiss_menus(cx);
                            v.refresh(cx);
                        })
                    }),
            )
    }
}

pub(super) fn findings_prompt(detail: &crate::pull_requests::PullRequestDetail) -> String {
    let mut prompt = format!(
        "Fix the review findings for {}. Read the review comments, address each actionable finding, and verify the changes. Resolved threads are included for context; check whether they still need action.",
        detail.summary.url
    );
    let mut seen = std::collections::HashSet::new();
    for (comment, thread) in detail.comments.iter().map(|c| (c, None)).chain(
        detail
            .review_threads
            .iter()
            .flat_map(|t| t.comments.iter().map(move |c| (c, Some(t)))),
    ) {
        if comment.body.trim().is_empty() || !seen.insert(&comment.id) {
            continue;
        }
        let path = comment
            .path
            .as_deref()
            .or_else(|| thread.map(|t| t.path.as_str()));
        let line = comment.line.or_else(|| thread.and_then(|t| t.line));
        let location = path
            .map(|path| match line {
                Some(line) => format!(" ({path}:{line})"),
                None => format!(" ({path})"),
            })
            .unwrap_or_default();
        let resolved = if comment.resolved || thread.is_some_and(|t| t.resolved) {
            " [resolved]"
        } else {
            ""
        };
        prompt.push_str(&format!(
            "\n\n{}{location}{resolved}: {}",
            comment.author, comment.body
        ));
    }
    prompt
}
