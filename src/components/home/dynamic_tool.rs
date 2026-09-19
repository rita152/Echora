//! Dynamic tool call presentation for the conversation view.

use gpui::{
    Div, Entity, FontWeight, Role, ScrollHandle, SharedString, Transformation, div, prelude::*, px,
    radians, rgba,
};

use super::{
    HomeView, MCP_TOOL_CALL_ICON_SIZE, MCP_TOOL_CALL_ICON_TEXT_GAP, MCP_TOOL_CALL_ROW_HEIGHT,
    MCP_TOOL_CALL_TEXT_SIZE, tools::toggle_command_activity,
};
use crate::{
    agent::{AgentDynamicToolCall, AgentDynamicToolCallContentItem, AgentDynamicToolCallStatus},
    components::icons::icon,
    theme::Theme,
};

const DYNAMIC_TOOL_CHEVRON_SIZE: f32 = 12.0;
const DYNAMIC_TOOL_BODY_GAP: f32 = 2.0;
const DYNAMIC_TOOL_STATUS_GAP: f32 = 4.0;

/// The reference client suppresses dynamic tool calls for the tools it renders
/// through a dedicated surface, and only when the protocol left the namespace
/// empty. This mirrors that predicate exactly.
const SUPPRESSED_UNNAMESPACED_TOOLS: [&str; 2] =
    ["automation_update", "load_workspace_dependencies"];

pub(super) fn is_dynamic_tool_call_visible(call: &AgentDynamicToolCall) -> bool {
    match call.namespace.as_deref() {
        Some(_) => true,
        None => !SUPPRESSED_UNNAMESPACED_TOOLS.contains(&call.tool.as_str()),
    }
}

fn humanize_tool_name(tool: &str) -> String {
    let mut label = tool.replace('_', " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}

pub(super) fn dynamic_tool_call_label(call: &AgentDynamicToolCall) -> String {
    humanize_tool_name(&call.tool)
}

pub(super) fn dynamic_tool_call_activity(
    home_entity: Entity<HomeView>,
    call: &AgentDynamicToolCall,
    expanded: bool,
    scroll_handle: ScrollHandle,
    theme: Theme,
) -> Div {
    let label = dynamic_tool_call_label(call);
    let failed = call.status == AgentDynamicToolCallStatus::Failed;
    let display_label = if failed {
        crate::i18n::format!("{label} 失败" => "{label} failed")
    } else {
        label
    };
    let status = match call.status {
        AgentDynamicToolCallStatus::InProgress => crate::i18n::text("进行中"),
        AgentDynamicToolCallStatus::Completed => crate::i18n::text("已完成"),
        AgentDynamicToolCallStatus::Failed => crate::i18n::text("失败"),
    };
    let accessible_label =
        crate::i18n::format!("动态工具 {}，{status}" => "Dynamic tool {}, {status}", call.tool);
    let foreground = if failed {
        theme.warning
    } else {
        theme.text.alpha(0.60)
    };
    let label_color = if failed {
        theme.warning
    } else {
        theme.text.alpha(0.60)
    };

    let id = call.id.clone();
    let hover: SharedString = format!("dynamic-tool-call-{id}").into();
    let click_home = home_entity.clone();
    let click_id = id.clone();
    let click_scroll = scroll_handle.clone();
    let key_home = home_entity.clone();
    let key_id = id.clone();
    let key_scroll = scroll_handle;

    let row = div()
        .id(SharedString::from(format!("dynamic-tool-call-{}", call.id)))
        .group(hover.clone())
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(DYNAMIC_TOOL_STATUS_GAP))
        .rounded(px(6.0))
        .focusable()
        .tab_stop(true)
        .role(Role::Button)
        .aria_expanded(expanded)
        .aria_label(accessible_label)
        .focus_visible(|style| style.border_1().border_color(rgba(0x3a83f7ff)))
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            toggle_command_activity(&click_home, &click_id, &click_scroll, cx)
        })
        .on_key_down(move |event, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                toggle_command_activity(&key_home, &key_id, &key_scroll, cx);
                cx.stop_propagation();
            }
        })
        .child(
            div()
                .h(px(MCP_TOOL_CALL_ROW_HEIGHT))
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(MCP_TOOL_CALL_ICON_TEXT_GAP))
                .child(icon("mcp-tool-call", foreground.into()).size(px(MCP_TOOL_CALL_ICON_SIZE)))
                .child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(MCP_TOOL_CALL_TEXT_SIZE))
                        .text_color(label_color)
                        .font_family(".SystemUIFont")
                        .font_weight(FontWeight::NORMAL)
                        .child(display_label),
                ),
        )
        .child(
            icon("settings-chevron-right", foreground.into())
                .size(px(DYNAMIC_TOOL_CHEVRON_SIZE))
                .flex_none()
                .opacity(if expanded { 1.0 } else { 0.0 })
                .group_hover(hover, |s| s.opacity(1.0))
                .with_transformation(Transformation::rotate(radians(if expanded {
                    std::f32::consts::FRAC_PI_2
                } else {
                    0.0
                }))),
        );

    let mut body = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(DYNAMIC_TOOL_BODY_GAP));
    if expanded {
        if let Some(duration_ms) = call.duration_ms
            && duration_ms > 0
        {
            body = body.child(
                div()
                    .text_size(px(MCP_TOOL_CALL_TEXT_SIZE))
                    .text_color(theme.text.alpha(0.60))
                    .child(
                        crate::i18n::format!("耗时 {duration_ms} 毫秒" => "Took {duration_ms} ms"),
                    ),
            );
        }
        if !matches!(call.arguments, serde_json::Value::Null) {
            let arguments = match &call.arguments {
                serde_json::Value::String(text) => text.clone(),
                other => serde_json::to_string_pretty(other).unwrap_or_default(),
            };
            if !arguments.trim().is_empty() {
                body = body.child(crate::components::markdown::render_tool_text(
                    &arguments,
                    theme,
                    &format!("{id}-arguments"),
                ));
            }
        }
        for (index, item) in call.content_items.iter().flatten().enumerate() {
            match item {
                AgentDynamicToolCallContentItem::Text { text } => {
                    body = body.child(crate::components::markdown::render_tool_text(
                        text,
                        theme,
                        &format!("{id}-content-{index}"),
                    ));
                }
                AgentDynamicToolCallContentItem::Image { image_url } => {
                    use base64::Engine as _;
                    let encoded = image_url
                        .split_once(",")
                        .filter(|(prefix, _)| prefix.starts_with("data:"))
                        .map(|(_, bytes)| bytes)
                        .unwrap_or(image_url.as_str());
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                        let format = super::media::tool_image_format(&bytes, None);
                        let (width, height) =
                            crate::media::encoded_image_dimensions(&bytes).unwrap_or((160, 160));
                        let scale = (160.0 / height.max(1) as f32).min(1.0);
                        body = body.child(
                            gpui::img(std::sync::Arc::new(gpui::Image::from_bytes(format, bytes)))
                                .w(px(width as f32 * scale))
                                .h(px(height as f32 * scale))
                                .max_w_full()
                                .object_fit(gpui::ObjectFit::Contain)
                                .rounded(px(10.0)),
                        );
                    }
                }
                // Audio content has no player in the reference activity row.
                AgentDynamicToolCallContentItem::Audio { .. } => {}
            }
        }
    }

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .child(row)
        .when(expanded, |column| column.child(body))
}
