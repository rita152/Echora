//! Environment Git actions reuse the review's snapshots and guarded mutations.
use super::{
    controls::Action,
    render::{addition_color, deletion_color},
    *,
};
use crate::theme::Theme;
use gpui::{AnyElement, Div, MouseButton, Role, Stateful, div, prelude::*, rgba};

impl ReviewPanel {
    pub fn has_environment_dialog(&self) -> bool {
        self.environment_dialog && (self.commit_open || self.environment_git_pending.is_some())
    }

    pub(super) fn return_to_environment(&mut self, cx: &mut Context<Self>) {
        if self.environment_dialog {
            self.environment_dialog = false;
            cx.emit(ReviewEvent::ReturnToEnvironment);
        }
    }

    pub(super) fn environment_commit_dialog(&self, cx: &Context<Self>) -> Stateful<Div> {
        let theme = Theme::for_mode(self.mode);
        let disabled =
            self.busy || (self.new_branch && self.branch_input.read(cx).text().trim() == "codex/");
        let mut files = div()
            .id("environment-commit-files")
            .max_h(px(128.))
            .overflow_y_scroll()
            .border_1()
            .border_color(theme.border)
            .rounded(px(8.))
            .py(px(2.));
        for (index, file) in self.snapshot.files.iter().enumerate() {
            let path = file.path.clone();
            files = files.child(
                div()
                    .id(("environment-commit-file", index))
                    .role(Role::Button)
                    .aria_label(path.clone())
                    .h(px(30.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.text.alpha(0.04)))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(ReviewEvent::OpenFile {
                            path: path.clone(),
                            line: None,
                        })
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_ellipsis()
                            .font_family(".SystemMonospaced")
                            .child(file.path.clone()),
                    )
                    .child(
                        div()
                            .text_color(addition_color(self.mode))
                            .child(format!("+{}", file.additions)),
                    )
                    .child("/")
                    .child(
                        div()
                            .text_color(deletion_color(self.mode))
                            .child(format!("−{}", file.deletions)),
                    ),
            );
        }
        let button =
            |id: &'static str, label: &'static str, glyph: &'static str, action: Action| {
                self.button(id, label, Some(glyph), action, cx)
                    .h(px(32.))
                    .w_full()
                    .justify_start()
                    .px(px(8.))
                    .gap(px(8.))
                    .child(label)
                    .rounded(px(10.))
                    .when(disabled, |s| s.opacity(0.4))
            };
        let mut card = div()
            .id("environment-commit-dialog")
            .role(Role::Dialog)
            .aria_label("Commit")
            .w(px(448.))
            .max_w_full()
            .m(px(16.))
            .rounded(px(22.))
            .bg(theme.model_picker_surface)
            .shadow_lg()
            .text_size(px(crate::typography::ui_size(13.)))
            .text_color(theme.text)
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .p(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().text_color(theme.text_tertiary).child("Commit"))
                    .child(
                        div()
                            .text_size(px(crate::typography::ui_size(14.)))
                            .child(self.snapshot.branch.clone()),
                    )
                    .when(self.new_branch, |s| {
                        s.child(div().h(px(32.)).child(self.branch_input.clone()))
                    })
                    .child(div().h(px(56.)).pt(px(4.)).child(self.commit_input.clone()))
                    .child(
                        div()
                            .h(px(30.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .text_color(theme.text_tertiary)
                            .child(
                                div()
                                    .flex_1()
                                    .child(format!("{} files", self.snapshot.files.len())),
                            )
                            .child(
                                div()
                                    .text_color(addition_color(self.mode))
                                    .child(format!("+{}", self.snapshot.working_additions)),
                            )
                            .child(
                                div()
                                    .text_color(deletion_color(self.mode))
                                    .child(format!("−{}", self.snapshot.working_deletions)),
                            )
                            .child(self.button(
                                "environment-commit-edit",
                                "Edit",
                                None,
                                Action::EnvironmentEdit,
                                cx,
                            )),
                    )
                    .child(files),
            )
            .child(div().h(px(1.)).bg(theme.border))
            .child(
                div()
                    .p(px(8.))
                    .flex()
                    .flex_col()
                    .child(button(
                        "environment-commit-new-branch",
                        if self.new_branch {
                            "Commit on current branch"
                        } else {
                            "Commit on new branch"
                        },
                        "branch",
                        Action::NewBranch(!self.new_branch),
                    ))
                    .child(button(
                        "environment-commit-confirm",
                        if self.busy { "Working…" } else { "Commit" },
                        "review-commit",
                        Action::SaveCommit,
                    ))
                    .child(button(
                        "environment-commit-push",
                        "Commit & push",
                        "environment-push",
                        Action::CommitAndPush,
                    ))
                    .child(button(
                        "environment-commit-create-pr",
                        "Create PR",
                        "pr-status-open",
                        Action::PullRequest,
                    )),
            );
        if let Some(error) = self.display_error() {
            card = card.child(
                div()
                    .px(px(16.))
                    .pb(px(12.))
                    .text_color(deletion_color(self.mode))
                    .child(error),
            );
        }
        div()
            .id("environment-commit-overlay")
            .absolute()
            .inset_0()
            .bg(rgba(0x00000099))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if !this.busy {
                        this.commit_open = false;
                        this.pr_open = false;
                        this.environment_git_pending = None;
                        this.return_to_environment(cx);
                        cx.notify();
                    }
                }),
            )
            .child(card)
    }

    pub fn prepare_environment_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.has_environment_dialog() {
            return None;
        }
        self.focus_pending = false;
        if self.focus_input && self.commit_open {
            self.focus_input = false;
            self.commit_input
                .read(cx)
                .focus_handle(cx)
                .focus(window, cx);
        }
        (self.commit_open).then(|| {
            if self.pr_open {
                self.dialog(cx).into_any_element()
            } else {
                self.environment_commit_dialog(cx).into_any_element()
            }
        })
    }
}
