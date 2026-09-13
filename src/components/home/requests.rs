//! Requests presentation and interaction for the conversation view.

use gpui::{Div, Entity, div, prelude::*};

use super::{HomeView, context::RequestOwner};
use crate::{
    components::{
        approval::{ApprovalCardCallback, render_approval_card},
        file_change::{FileApprovalCallback, FileApprovalPresentation, render_file_approval_card},
        mcp_elicitation::{
            McpElicitationCallback, McpElicitationEvent, McpElicitationFocus,
            McpElicitationPresentation, render_mcp_elicitation,
        },
        permissions_approval::{
            PermissionApprovalCallback, PermissionApprovalPresentation, render_permissions_approval,
        },
        prompt_input::PromptInput,
        user_input_request::{
            UserInputRequestCallback, UserInputRequestEvent, render_user_input_request,
        },
    },
    theme::Theme,
};

pub(super) fn command_approval_card(
    home_entity: Entity<HomeView>,
    owner: RequestOwner,
    model: crate::components::approval::ApprovalCardViewModel,
    theme: Theme,
    preview: Option<Entity<crate::components::file_editor::FileEditor>>,
) -> Option<gpui::Stateful<Div>> {
    let scope = owner.scope();
    let request_id = model.request_id.clone();
    let target = home_entity;
    let callback = ApprovalCardCallback::new(move |event, window, cx| {
        let request_id = request_id.clone();
        target.update(cx, |home, cx| {
            if !owner.matches(home, cx) {
                return;
            }
            window.focus(&home.approval_focus, cx);
            home.handle_approval_card_event(&request_id, event, cx)
        });
    });
    render_approval_card(&model, theme, preview, callback)
        .map(|card| div().id(scope).w_full().child(card))
}

pub(super) fn file_approval_card(
    home_entity: Entity<HomeView>,
    owner: RequestOwner,
    model: FileApprovalPresentation,
    theme: Theme,
) -> Option<gpui::Stateful<Div>> {
    let scope = owner.scope();
    let request_id = model.request_id.clone();
    let target = home_entity;
    let callback = FileApprovalCallback::new(move |event, window, cx| {
        let request_id = request_id.clone();
        target.update(cx, |home, cx| {
            if !owner.matches(home, cx) {
                return;
            }
            window.focus(&home.approval_focus, cx);
            home.handle_file_approval_event(&request_id, event, cx)
        });
    });
    render_file_approval_card(&model, theme, callback)
        .map(|card| div().id(scope).w_full().child(card))
}

pub(super) fn permissions_approval_card(
    home_entity: Entity<HomeView>,
    owner: RequestOwner,
    model: PermissionApprovalPresentation,
    theme: Theme,
) -> Option<gpui::Stateful<Div>> {
    let scope = owner.scope();
    let request_id = model.request_id.clone();
    let target = home_entity;
    let callback = PermissionApprovalCallback::new(move |event, _, cx| {
        let request_id = request_id.clone();
        target.update(cx, |home, cx| {
            if !owner.matches(home, cx) {
                return;
            }
            home.handle_permissions_approval_event(&request_id, event, cx)
        });
    });
    render_permissions_approval(&model, theme, callback)
        .map(|card| div().id(scope).w_full().child(card))
}

pub(super) fn user_input_request_card(
    home_entity: Entity<HomeView>,
    owner: RequestOwner,
    model: crate::components::user_input_request::UserInputRequestPresentation,
    theme: Theme,
    other_input: Entity<PromptInput>,
) -> Option<gpui::Stateful<Div>> {
    let scope = owner.scope();
    let request_id = model.request_id.clone();
    let target = home_entity;
    let callback = UserInputRequestCallback::new(move |event, window, cx| {
        if !owner.matches(target.read(cx), cx) {
            return;
        }
        let request_id = request_id.clone();
        if matches!(event, UserInputRequestEvent::BeginOtherAnswer { .. }) {
            let focus = target
                .read(cx)
                .composer
                .read(cx)
                .user_input_other_focus_handle(cx);
            window.focus(&focus, cx);
        }
        target.update(cx, |home, cx| {
            if !owner.matches(home, cx) {
                return;
            }
            home.handle_user_input_request_event(&request_id, event, cx)
        });
    });
    render_user_input_request(&model, theme, other_input, callback)
        .map(|card| div().id(scope).w_full().child(card))
}

/// Standalone MCP elicitation card. Its responder is connection-scoped, so the
/// card stays interactive independently of any active turn.
pub(super) fn mcp_elicitation_card(
    home_entity: Entity<HomeView>,
    owner: RequestOwner,
    model: McpElicitationPresentation,
    theme: Theme,
    text_input: Entity<PromptInput>,
) -> Option<gpui::Stateful<Div>> {
    let scope = owner.scope();
    let request_id = model.request_id.clone();
    let fields = model.fields().to_vec();
    let target = home_entity;
    let callback = McpElicitationCallback::new(move |event, window, cx| {
        if !owner.matches(target.read(cx), cx) {
            return;
        }
        let request_id = request_id.clone();
        let focus_editor = match &event {
            McpElicitationEvent::Focus(McpElicitationFocus::Field(index)) => fields
                .get(*index)
                .is_some_and(
                crate::components::mcp_elicitation::McpElicitationFieldPresentation::is_text_like,
            ),
            McpElicitationEvent::Focus(_) => false,
            _ => false,
        };
        target.update(cx, |home, cx| {
            if !owner.matches(home, cx) {
                return;
            }
            home.handle_mcp_elicitation_event(&request_id, event, cx);
        });
        if focus_editor {
            let composer = target.read(cx).composer.clone();
            let focus = composer.read(cx).mcp_elicitation_focus_handle(cx);
            window.focus(&focus, cx);
        }
    });
    render_mcp_elicitation(&model, theme, text_input, callback)
        .map(|card| div().id(scope).w_full().child(card))
}
