//! Detail-pane overlays and the comment controls around the activity feed:
//! the comment actions menu, the edit buttons, and a review thread's footer
//! with its `Reply`/`Resolve` buttons and reply composer.

use gpui::{Div, SharedString, div, prelude::*, px};

use super::{PullRequestsView, theme::*};
use crate::components::icons::icon;

impl PullRequestsView {
    pub(super) fn comment_edit_actions(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let cancel = view.clone();
        div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(
                Self::toolbar_button("pr-comment-edit-cancel", theme, false)
                    .text_color(theme.text)
                    .aria_label("Cancel")
                    .on_click(move |_, _, cx| {
                        cancel.update(cx, |view, cx| view.cancel_comment_edit(cx));
                    })
                    .child("Cancel"),
            )
            .child(
                Self::toolbar_button("pr-comment-edit-save", theme, true)
                    .bg(theme.inverted_surface)
                    .text_color(theme.inverted_text)
                    .aria_label("Save changes")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.save_comment_edit(cx));
                    })
                    .child("Save changes"),
            )
    }

    /// A review thread's footer (`px-3 py-2`, 13/20 tertiary): `Reply` and
    /// `Resolve`, or, once `Reply` is pressed, the reply composer above a rule.
    pub(super) fn comment_reply_actions(
        &self,
        comment_id: &str,
        thread: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme();
        let view = cx.entity();
        let reply_open = self
            .reply_target
            .as_deref()
            .is_some_and(|target| Some(target) == thread.as_deref() || target == comment_id);
        let footer = div()
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(13.0))
            .line_height(px(20.0))
            .text_color(theme.text_muted);
        if reply_open {
            let cancel_view = view.clone();
            let post_view = view.clone();
            let has_text = !self.mutation_pending
                && self
                    .reply
                    .as_ref()
                    .is_some_and(|(_, editor)| !editor.read(cx).text().trim().is_empty());
            let Some((_, editor)) = self.reply.clone() else {
                return footer;
            };
            return footer
                .border_t(px(1.0))
                .border_color(theme.border)
                .child(self.composer(
                    editor,
                    "pr-reply-editor-frame",
                    false,
                    super::render::ComposerActions {
                        cancel: Some(Box::new(move |_, _, cx: &mut gpui::App| {
                            cancel_view.update(cx, |view, cx| view.cancel_reply(cx));
                        })),
                        post_label: "Post reply",
                        post_enabled: has_text,
                        post: Box::new(move |_, _, cx: &mut gpui::App| {
                            post_view.update(cx, |view, cx| view.post_reply(cx));
                        }),
                    },
                    cx,
                ));
        }
        let reply_view = view.clone();
        let reply_thread = thread.clone();
        let mut actions = div().flex().items_start().gap(px(8.0)).child(
            Self::footer_button(SharedString::from(format!("pr-reply-{comment_id}")), theme)
                .aria_label("Reply")
                .on_click(move |_, _, cx| {
                    let thread = reply_thread.clone();
                    reply_view.update(cx, |view, cx| view.begin_reply(thread.clone(), None, cx));
                })
                .child(icon("pr-reply", theme.text_muted.into()).size(px(18.0)))
                .child(div().pl(px(4.0)).child("Reply")),
        );
        let resolvable = thread.as_ref().and_then(|id| {
            self.detail
                .as_ref()
                .and_then(|detail| detail.thread(id))
                .filter(|thread| thread.can_resolve)
        });
        if let Some(resolvable) = resolvable {
            let resolve_view = view.clone();
            let thread_id = resolvable.id.clone();
            let label = if resolvable.resolved {
                "Unresolve"
            } else {
                "Resolve"
            };
            actions = actions.child(
                Self::footer_button(
                    SharedString::from(format!("pr-resolve-{comment_id}")),
                    theme,
                )
                .aria_label(label)
                .on_click(move |_, _, cx| {
                    resolve_view.update(cx, |view, cx| view.resolve_thread(thread_id.clone(), cx));
                })
                .child(label),
            );
        }
        footer.child(actions)
    }

    /// `Reply`/`Resolve`: a 24px ghost button, `px-2 py-0.5` inside a
    /// transparent border.
    fn footer_button(id: SharedString, theme: PrTheme) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .role(gpui::Role::Button)
            .flex_none()
            .h(px(24.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(9.375))
            .line_height(px(18.0))
            .whitespace_nowrap()
            .hover(move |style| style.bg(theme.row_hover))
    }

    /// Overlays anchored inside the detail pane.
    pub(super) fn detail_overlays(&self, cx: &mut gpui::Context<Self>) -> Vec<gpui::AnyElement> {
        let mut overlays = Vec::new();
        if self.status_menu {
            overlays.push(self.popup("pr-status", self.status_menu(cx)));
        }
        if self.description_menu {
            overlays.push(self.popup("pr-description-actions", self.description_menu(cx)));
        }
        if self.review_options_open {
            overlays.push(self.popup("pr-review-options", self.review_options_menu(cx)));
        }
        if self.scope_menu_open {
            overlays.push(self.popup("pr-review-tab-scope", self.scope_menu(cx)));
        }
        if let Some(id) = &self.comment_menu
            && self
                .detail
                .as_ref()
                .and_then(|detail| detail.comment(id))
                .is_some()
        {
            overlays.push(self.popup_at(
                &format!("pr-comment-actions-{id}"),
                gpui::point(px(-1.0), px(2.0)),
                self.comment_menu_overlay(id.clone(), cx),
            ));
        }
        overlays
    }

    /// `Comment actions`: `Edit`, `Quote reply`, and `Delete` (not for review
    /// summaries), each with its leading glyph.
    fn comment_menu_overlay(&self, id: String, cx: &mut gpui::Context<Self>) -> Div {
        let theme = self.theme();
        let view = cx.entity();
        let mut menu = Self::menu_surface("pr-comment-menu", theme)
            .w(px(160.0))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let comment = self.detail.as_ref().and_then(|detail| detail.comment(&id));
        let entries = [
            (
                "Edit",
                "pr-menu-edit",
                comment.is_some_and(|comment| comment.can_edit),
            ),
            (
                "Quote reply",
                "pr-menu-quote",
                comment.is_some_and(|comment| comment.can_quote),
            ),
            (
                "Delete",
                "pr-menu-delete",
                comment.is_some_and(|comment| {
                    comment.can_delete && !(comment.is_review && comment.thread_id.is_none())
                }),
            ),
        ];
        for (entry, glyph, enabled) in entries {
            if !enabled {
                continue;
            }
            let view = view.clone();
            let id = id.clone();
            menu = menu.child(
                Self::menu_row(
                    SharedString::from(format!("pr-comment-action-{entry}")),
                    theme,
                    false,
                    false,
                )
                .aria_label(entry)
                .on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| match entry {
                        "Edit" => view.begin_comment_edit(id.clone(), cx),
                        "Quote reply" => {
                            let comment = view
                                .detail
                                .as_ref()
                                .and_then(|detail| detail.comment(&id).cloned());
                            let quote = comment
                                .as_ref()
                                .map(|comment| {
                                    comment
                                        .body
                                        .lines()
                                        .map(|line| format!("> {line}"))
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                })
                                .unwrap_or_default();
                            let thread = comment
                                .as_ref()
                                .and_then(|comment| comment.thread_id.clone());
                            view.begin_reply(thread, Some(format!("{quote}\n\n")), cx);
                        }
                        "Delete" => view.delete_comment(id.clone(), cx),
                        _ => {}
                    });
                })
                // `tone: "danger"` draws `Delete` in `text-danger`.
                .when(entry == "Delete", |row| row.text_color(theme.chart_red))
                .child(Self::tinted_menu_icon(
                    glyph,
                    if entry == "Delete" {
                        theme.chart_red
                    } else {
                        theme.text
                    },
                ))
                .child(div().flex_1().min_w(px(0.0)).child(entry)),
            );
        }
        div().child(menu)
    }
}
