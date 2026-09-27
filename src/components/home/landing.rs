//! Landing presentation and interaction for the conversation view.

use gpui::{Bounds, Div, Entity, canvas, div, point, prelude::*, px, size};

use super::{
    COMPOSER_BOTTOM_INSET, CONVERSATION_BOTTOM_INSET,
    context::{ConversationRenderContext, MainConversationSnapshot},
    conversation::conversation,
    navigation::user_message_navigation_overlay,
    requests::{
        command_approval_card, file_approval_card, mcp_elicitation_card, permissions_approval_card,
        user_input_request_card,
    },
};
use crate::{
    components::{
        composer::{COMPOSER_CORNER_RADIUS, ComposerView, WorkspacePresentation},
        icons::icon,
        prompt_input::PromptInput,
    },
    conversation::{ConversationActivity, ConversationPhase},
    theme::Theme,
};

/// Chromium draws the 28px heading glyphs 1.5px lower in the same 33.6px line
/// box than GPUI does (DPR 2 captures of both), so the text moves down by that
/// much while the underline keeps its own position.
const HERO_GLYPH_OFFSET: f32 = 1.5;
/// Offset of the dotted underline's 1px row from the line box's top edge: the
/// reference's `underline-offset-4` on SF Pro at 28px lands it 30.6px down
/// (CDP, DPR 2). It is measured inside the shifted text.
const HERO_UNDERLINE_TOP: f32 = 30.6 - HERO_GLYPH_OFFSET;
/// Top of the reference's home column, below the titlebar strip.
const HOME_COLUMN_TOP: f32 = 46.0;
/// Content height of the reference's composer row at the default composer
/// height: its rail starts 160px above the window's bottom edge.
const HOME_COMPOSER_BLOCK_HEIGHT: f32 = 160.0;

/// The reference's home heading, split around the project name it lets the
/// user change. `None` stands for the reference's pending state, where the
/// heading stays empty until the variant is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HeroHeading {
    pub(super) lead: String,
    /// The underlined project trigger; English keeps the question mark inside
    /// it, Chinese only the name.
    pub(super) project: Option<String>,
    pub(super) tail: String,
}

/// Picks the reference's hero copy: `What should we build?` without a project,
/// `…build in {project}?` inside a Git repository and `…work on in {project}?`
/// elsewhere. A project waits for its checkout, since that decides the copy.
pub(super) fn hero_heading(workspace: &WorkspacePresentation) -> Option<HeroHeading> {
    let Some(label) = workspace.project_label.as_ref() else {
        return Some(HeroHeading {
            lead: crate::i18n::text("我们要构建什么？").to_owned(),
            project: None,
            tail: String::new(),
        });
    };
    let repository = workspace.checkout.as_ref()?.is_repository();
    let (lead, project, tail) = match (crate::i18n::is_english(), repository) {
        (true, true) => ("What should we build in ", format!("{label}?"), ""),
        (true, false) => ("What should we work on in ", format!("{label}?"), ""),
        (false, true) => ("你想让我们在 ", label.to_string(), " 中构建什么？"),
        (false, false) => ("我们应该在", label.to_string(), "中做些什么？"),
    };
    Some(HeroHeading {
        lead: lead.to_owned(),
        project: Some(project),
        tail: tail.to_owned(),
    })
}

/// The reference home column: below the 46px titlebar and its `pt-6`, two
/// `grow basis-0` rows share the height. The upper row's `pb-24` counts in its
/// flex base size, so it ends 96px taller than the composer row, and the hero
/// sits on its bottom padding. A composer taller than its half pushes the hero
/// up through the lower row's content height.
fn home_hero(heading: Option<HeroHeading>, composer_height: f32, theme: Theme) -> Div {
    div()
        .absolute()
        .top(px(HOME_COLUMN_TOP))
        .bottom_0()
        .w_full()
        .pt(px(24.0))
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .flex_grow(1.0)
                .flex_basis(px(0.0))
                .pb(px(96.0))
                .items_end()
                .justify_center()
                .child(
                    div()
                        .w_full()
                        .max_w(px(768.0))
                        .px(px(20.0))
                        .min_h(px(112.0))
                        .flex()
                        .flex_col()
                        .justify_end()
                        .items_center()
                        .gap(px(24.0))
                        .child(icon("home-mark", theme.home_mark.into()).size(px(56.0)))
                        .child(hero_title(heading, theme)),
                ),
        )
        .child(
            div()
                .flex_grow(1.0)
                .flex_shrink_0()
                .flex_basis(px(0.0))
                .min_h(px(
                    HOME_COMPOSER_BLOCK_HEIGHT + (composer_height - 98.0).max(0.0)
                )),
        )
}

fn hero_title(heading: Option<HeroHeading>, theme: Theme) -> Div {
    let title = div()
        .max_w_full()
        .min_h(px(33.6))
        .flex()
        .flex_wrap()
        .items_end()
        .justify_center()
        .text_center()
        .text_size(px(28.0))
        .line_height(px(33.6))
        .font_weight(gpui::FontWeight::NORMAL)
        .text_color(theme.text);
    let Some(heading) = heading else {
        return title;
    };
    title
        .relative()
        .top(px(HERO_GLYPH_OFFSET))
        .child(heading.lead)
        .when_some(heading.project, |title, project| {
            title.child(hero_project_trigger(project, theme))
        })
        .when(!heading.tail.is_empty(), |title| title.child(heading.tail))
}

/// The project name inside the heading: `underline decoration-dotted
/// decoration-[1px] decoration-text-tertiary underline-offset-4` and
/// `hover:text-secondary`. GPUI underlines are solid or wavy only, so the 1px
/// dots and 1px gaps Chromium draws are painted directly, snapped to device
/// pixels from the trigger's leading edge.
fn hero_project_trigger(project: String, theme: Theme) -> impl IntoElement {
    let dot = theme.text_tertiary;
    div()
        .id("home-hero-project")
        .debug_selector(|| "home-hero-project".to_owned())
        .relative()
        .max_w_full()
        .hover(move |style| style.text_color(theme.text.alpha(0.65)))
        .child(project)
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let scale = window.scale_factor();
                    let top = (f32::from(bounds.top()) * scale).round() / scale;
                    let right = f32::from(bounds.right());
                    let mut x = (f32::from(bounds.left()) * scale).floor() / scale;
                    while x < right {
                        window.paint_quad(gpui::fill(
                            Bounds::new(point(px(x), px(top)), size(px(1.0), px(1.0))),
                            dot,
                        ));
                        x += 2.0;
                    }
                },
            )
            .absolute()
            .left_0()
            .top(px(HERO_UNDERLINE_TOP))
            .w_full()
            .h(px(1.0)),
        )
}

pub(super) fn home(
    render: ConversationRenderContext,
    composer: Entity<ComposerView>,
    user_input_other: Entity<PromptInput>,
    snapshot: MainConversationSnapshot,
) -> Div {
    let home_entity = render.home_entity.clone();
    let theme = render.theme;
    let MainConversationSnapshot {
        side_chat,
        composer_height,
        rows: conversation_rows,
        phase,
        activities: conversation_activity,
        list: conversation_list,
        navigation,
        hero,
    } = snapshot;
    let visible_request = conversation_activity
        .iter()
        .find(|activity| activity.shows_request());
    let pending_command_approval =
        if let Some(ConversationActivity::Approval(model)) = visible_request {
            Some(model.clone())
        } else {
            None
        };
    let pending_user_input = if let Some(ConversationActivity::UserInput(model)) = visible_request {
        Some(model.clone())
    } else {
        None
    };
    let pending_file_approval =
        if let Some(ConversationActivity::FileApproval(model)) = visible_request {
            Some(model.clone())
        } else {
            None
        };
    let pending_permissions_approval =
        if let Some(ConversationActivity::PermissionsApproval(model)) = visible_request {
            Some(model.clone())
        } else {
            None
        };
    let pending_mcp_elicitation =
        if let Some(ConversationActivity::McpElicitation(model)) = visible_request {
            Some(model.as_ref().clone())
        } else {
            None
        };
    let turn_plan = conversation_activity
        .iter()
        .rev()
        .find_map(|activity| match activity {
            ConversationActivity::TurnPlan(plan) if !plan.steps.is_empty() => Some(plan.clone()),
            _ => None,
        });
    let blocking_request_pending = pending_command_approval.is_some()
        || pending_user_input.is_some()
        || pending_file_approval.is_some()
        || pending_permissions_approval.is_some()
        || pending_mcp_elicitation.is_some();

    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .relative()
        .when(phase == ConversationPhase::Empty && side_chat, |root| {
            root.child(
                div()
                    .absolute()
                    // These are component boundaries, not a viewport-specific
                    // heading coordinate. GPUI centers the group in between them.
                    .top(px(78.0))
                    .bottom(px(composer_height + 60.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(768.0))
                            .px(px(24.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                icon("side-chat", theme.text_secondary.into())
                                    .size(px(32.0))
                                    .relative()
                                    .top(px(-2.0)),
                            )
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .line_height(px(24.0))
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(theme.text)
                                    .child(crate::i18n::text("侧边聊天")),
                            )
                            .child(
                                div()
                                    .mt(px(-4.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.5714))
                                    .text_color(theme.text_secondary)
                                    .text_center()
                                    .child(crate::i18n::text(
                                        "侧边聊天是临时聊天，关闭应用后会消失。",
                                    )),
                            ),
                    ),
            )
        })
        .when(phase == ConversationPhase::Empty && !side_chat, |root| {
            root.child(home_hero(hero, composer_height, theme))
        })
        .when(phase != ConversationPhase::Empty, |root| {
            root.child(conversation(
                render.clone(),
                conversation_rows,
                conversation_list.clone(),
                if side_chat {
                    composer_height + 55.0
                } else {
                    CONVERSATION_BOTTOM_INSET + (composer_height - 98.0).max(0.0)
                },
            ))
            // The rail floats over the transcript, in the pane's left gutter.
            .when_some(navigation.filter(|_| !side_chat), |root, rail| {
                root.child(user_message_navigation_overlay(
                    rail,
                    theme,
                    home_entity.clone(),
                ))
            })
        })
        .when(phase != ConversationPhase::Empty, |root| {
            root.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .w_full()
                    // The upper corner cutouts remain open so rows pass behind
                    // the floating Composer. Its lower corners and the window
                    // inset are outside the conversation's visible region.
                    .h(px(COMPOSER_BOTTOM_INSET + COMPOSER_CORNER_RADIUS))
                    .bg(theme.surface),
            )
        })
        .when_some(pending_command_approval, |root, model| {
            let preview = render.approval_previews.get(&model.request_id).cloned();
            let card = command_approval_card(
                home_entity.clone(),
                render.request_owner.clone(),
                model,
                theme,
                preview,
            );
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .id("command-approval-overlay")
                        .debug_selector(|| "command-approval-overlay".to_owned())
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(
                            div()
                                .relative()
                                .left(px(render.approval_border_offset))
                                .child(card),
                        ),
                )
            })
        })
        .when_some(pending_user_input, |root, model| {
            let card = user_input_request_card(
                home_entity.clone(),
                render.request_owner.clone(),
                model,
                theme,
                user_input_other.clone(),
            );
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(div().relative().left(px(2.671_875)).w_full().child(card)),
                )
            })
        })
        .when_some(pending_file_approval, |root, model| {
            let card = file_approval_card(
                home_entity.clone(),
                render.request_owner.clone(),
                model,
                theme,
            );
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(
                            div()
                                .relative()
                                .left(px(render.approval_border_offset))
                                .w_full()
                                .child(card),
                        ),
                )
            })
        })
        .when_some(pending_permissions_approval, |root, model| {
            let card = permissions_approval_card(
                home_entity.clone(),
                render.request_owner.clone(),
                model,
                theme,
            );
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(div().relative().left(px(0.671_875)).w_full().child(card)),
                )
            })
        })
        .when_some(pending_mcp_elicitation, |root, model| {
            let card = mcp_elicitation_card(
                home_entity.clone(),
                render.request_owner.clone(),
                model,
                theme,
                render.mcp_elicitation_input.clone(),
            );
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .id("mcp-elicitation-overlay")
                        .debug_selector(|| "mcp-elicitation-overlay".to_owned())
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(div().relative().left(px(0.671_875)).w_full().child(card)),
                )
            })
        })
        .child(
            div()
                .id("composer-overlay")
                .debug_selector(|| "composer-overlay".to_owned())
                .absolute()
                .bottom(px(COMPOSER_BOTTOM_INSET))
                .w_full()
                .max_w(px(748.0))
                // Match the reference composition at every window size: the
                // composer sits 6px inside its responsive container, while
                // its utility strip adds its own 14px inset.
                .px(px(6.0))
                .flex()
                .flex_col()
                .justify_end()
                .gap(px(8.0))
                .when_some(
                    turn_plan.filter(|_| {
                        !blocking_request_pending
                            && matches!(
                                phase,
                                ConversationPhase::Starting
                                    | ConversationPhase::Thinking
                                    | ConversationPhase::Streaming
                                    | ConversationPhase::Stopping
                            )
                    }),
                    |container, plan| {
                        let expanded = render
                            .disclosures
                            .expanded_commands
                            .contains(&format!("turn-plan-{}", plan.turn_id));
                        container.child(div().w_full().flex().justify_center().child(
                            super::progress::turn_plan_control(
                                home_entity.clone(),
                                plan,
                                expanded,
                                theme,
                            ),
                        ))
                    },
                )
                .when(!blocking_request_pending, |container| {
                    container.child(composer)
                }),
        )
}
