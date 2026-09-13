//! Native mcpServer/elicitation/request form and url cards.
//!
//! This module is deliberately presentation-only: it does not depend on the
//! app-server protocol and never writes a response itself. Geometry, copy,
//! colors, and interaction states follow the ChatGPT desktop CDP captures in
//! artifacts/mcp-elicitation-cdp-20260913/. Every field keeps an explicit
//! domain value (text, boolean, single choice, multi choice) plus its own
//! validation error so a rejected submit is recoverable without inventing a
//! second response for the same request.

use std::fmt;

use gpui::{
    BoxShadow, Div, Entity, FontWeight, Role, SharedString, Stateful, div, prelude::*, px, rgba,
};

use crate::{
    agent::{
        AgentMcpElicitationAction, AgentMcpElicitationContent, AgentMcpElicitationField,
        AgentMcpElicitationFieldKind, AgentMcpElicitationFieldValue, AgentMcpElicitationMode,
        AgentMcpElicitationRequest, AgentMcpElicitationStringFormat, AgentMcpElicitationValue,
    },
    components::{callback::UiCallback, icons::icon, prompt_input::PromptInput},
    theme::Theme,
};

pub const MCP_ELICITATION_CARD_RADIUS: f32 = 25.0;
/// url cards use the reference rounded-2xl card; form cards use rounded-3xl.
pub const MCP_ELICITATION_URL_CARD_RADIUS: f32 = 15.0;
pub const MCP_ELICITATION_HEADER_HEIGHT: f32 = 48.0;
pub const MCP_ELICITATION_CONTENT_PADDING: f32 = 12.0;
pub const MCP_ELICITATION_FIELD_GAP: f32 = 12.0;
pub const MCP_ELICITATION_CONTROL_HEIGHT: f32 = 32.0;
pub const MCP_ELICITATION_OPTION_HEIGHT: f32 = 32.0;
pub const MCP_ELICITATION_OPTION_GAP: f32 = 4.0;
pub const MCP_ELICITATION_FOOTER_HEIGHT: f32 = 45.0;
pub const MCP_ELICITATION_BUTTON_HEIGHT: f32 = 28.0;
/// CDP: field labels and option rows resolve to 13px / 18.5714px.
pub const MCP_ELICITATION_LABEL_SIZE: f32 = 13.0;
pub const MCP_ELICITATION_LABEL_LINE_HEIGHT: f32 = 18.5714;
pub const MCP_ELICITATION_TITLE_SIZE: f32 = 14.0;
pub const MCP_ELICITATION_TITLE_LINE_HEIGHT: f32 = 20.0;

fn element_id(prefix: &str, request_id: &str, suffix: impl std::fmt::Display) -> SharedString {
    format!("{prefix}-{request_id}-{suffix}").into()
}

/// Lifecycle of one elicitation card. A submitted response stays mounted and
/// disabled until the matching serverRequest/resolved arrives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpElicitationStatus {
    #[default]
    Pending,
    Submitting,
    Accepted,
    Declined,
    Cancelled,
    /// The request can no longer be answered: connection loss, closed thread,
    /// a failed write, or a late event from a retired generation.
    Invalid,
}

impl McpElicitationStatus {
    pub fn is_interactive(self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Pending and in-flight requests own the space above the Composer; the
    /// terminal states stay in the conversation stream only.
    pub fn is_overlay_visible(self) -> bool {
        matches!(self, Self::Pending | Self::Submitting)
    }

    pub fn should_render(self) -> bool {
        true
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pending => "等待输入",
            Self::Submitting => "正在提交…",
            Self::Accepted => "已完成",
            Self::Declined => "已拒绝",
            Self::Cancelled => "已取消",
            Self::Invalid => "请求已失效",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpElicitationOptionPresentation {
    pub value: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpElicitationFieldControl {
    Text {
        placeholder: String,
        secret: bool,
    },
    Number {
        integer: bool,
        minimum: Option<String>,
        maximum: Option<String>,
    },
    Boolean,
    SingleSelect {
        options: Vec<McpElicitationOptionPresentation>,
    },
    MultiSelect {
        options: Vec<McpElicitationOptionPresentation>,
        min_items: Option<u64>,
        max_items: Option<u64>,
    },
}

/// The concrete value the user is editing for one field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpElicitationFieldValueState {
    Text(String),
    Boolean(bool),
    Selection(Option<usize>),
    MultiSelection(Vec<usize>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpElicitationFieldPresentation {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub required: bool,
    pub control: McpElicitationFieldControl,
    pub value: McpElicitationFieldValueState,
    /// Validation error shown under the control. Cleared on the next edit.
    pub error: Option<String>,
}

impl McpElicitationFieldPresentation {
    pub fn display_title(&self) -> &str {
        if self.title.trim().is_empty() {
            &self.name
        } else {
            &self.title
        }
    }

    pub fn is_text_like(&self) -> bool {
        matches!(
            self.control,
            McpElicitationFieldControl::Text { .. } | McpElicitationFieldControl::Number { .. }
        )
    }

    pub fn text(&self) -> &str {
        match &self.value {
            McpElicitationFieldValueState::Text(text) => text,
            _ => "",
        }
    }

    pub fn set_text(&mut self, text: String) {
        self.value = McpElicitationFieldValueState::Text(text);
        self.error = None;
    }

    pub fn placeholder(&self) -> String {
        match &self.control {
            McpElicitationFieldControl::Text { placeholder, .. } => placeholder.clone(),
            McpElicitationFieldControl::Number {
                minimum, maximum, ..
            } => match (minimum, maximum) {
                (Some(minimum), Some(maximum)) => format!("{minimum} – {maximum}"),
                (Some(minimum), None) => format!("≥ {minimum}"),
                (None, Some(maximum)) => format!("≤ {maximum}"),
                (None, None) => "输入数字".to_owned(),
            },
            _ => String::new(),
        }
    }

    pub fn accessible_value(&self) -> String {
        match &self.value {
            McpElicitationFieldValueState::Text(text) => text.clone(),
            McpElicitationFieldValueState::Boolean(value) => {
                if *value {
                    "已开启".to_owned()
                } else {
                    "已关闭".to_owned()
                }
            }
            McpElicitationFieldValueState::Selection(index) => index
                .and_then(|index| self.options().get(index))
                .map(|option| option.title.clone())
                .unwrap_or_else(|| "未选择".to_owned()),
            McpElicitationFieldValueState::MultiSelection(indices) => {
                let titles = indices
                    .iter()
                    .filter_map(|index| self.options().get(*index))
                    .map(|option| option.title.clone())
                    .collect::<Vec<_>>();
                if titles.is_empty() {
                    "未选择".to_owned()
                } else {
                    titles.join("、")
                }
            }
        }
    }

    fn options(&self) -> &[McpElicitationOptionPresentation] {
        match &self.control {
            McpElicitationFieldControl::SingleSelect { options }
            | McpElicitationFieldControl::MultiSelect { options, .. } => options,
            _ => &[],
        }
    }

    fn is_selected(&self, index: usize) -> bool {
        match &self.value {
            McpElicitationFieldValueState::Selection(selected) => *selected == Some(index),
            McpElicitationFieldValueState::MultiSelection(selected) => selected.contains(&index),
            _ => false,
        }
    }

    /// One submitted value, or a user-facing reason it cannot be submitted.
    pub fn domain_value(&self) -> Result<AgentMcpElicitationValue, String> {
        let title = self.display_title().to_owned();
        match (&self.control, &self.value) {
            (
                McpElicitationFieldControl::Text { .. },
                McpElicitationFieldValueState::Text(text),
            ) => Ok(AgentMcpElicitationValue::String(text.clone())),
            (
                McpElicitationFieldControl::Number { integer, .. },
                McpElicitationFieldValueState::Text(text),
            ) => {
                let text = text.trim();
                if text.is_empty() {
                    return Err(format!("{title} 不能为空"));
                }
                let number = text
                    .parse::<serde_json::Number>()
                    .or_else(|_| {
                        text.parse::<f64>()
                            .ok()
                            .and_then(serde_json::Number::from_f64)
                            .ok_or(())
                    })
                    .map_err(|_| format!("{title} 必须是数字"))?;
                if *integer && number.as_i64().is_none() && number.as_u64().is_none() {
                    return Err(format!("{title} 必须是整数"));
                }
                Ok(AgentMcpElicitationValue::Number(number))
            }
            (
                McpElicitationFieldControl::Boolean,
                McpElicitationFieldValueState::Boolean(value),
            ) => Ok(AgentMcpElicitationValue::Boolean(*value)),
            (
                McpElicitationFieldControl::SingleSelect { options },
                McpElicitationFieldValueState::Selection(index),
            ) => {
                let option = index
                    .and_then(|index| options.get(index))
                    .ok_or_else(|| format!("{title} 需要选择一个选项"))?;
                Ok(AgentMcpElicitationValue::String(option.value.clone()))
            }
            (
                McpElicitationFieldControl::MultiSelect { options, .. },
                McpElicitationFieldValueState::MultiSelection(indices),
            ) => {
                let mut values = Vec::with_capacity(indices.len());
                for index in indices {
                    let option = options
                        .get(*index)
                        .ok_or_else(|| format!("{title} 包含无效选项"))?;
                    values.push(option.value.clone());
                }
                Ok(AgentMcpElicitationValue::StringArray(values))
            }
            _ => Err(format!("{title} 的输入类型与请求不一致")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpElicitationModePresentation {
    Form {
        message: String,
        fields: Vec<McpElicitationFieldPresentation>,
    },
    Url {
        message: String,
        elicitation_id: String,
        url: String,
        /// Opening the link is a local action; it never answers the request.
        opened: bool,
    },
}

/// Logical focus inside the card. Native window focus stays on the enclosing
/// surface; this preserves the CDP-observed Tab order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpElicitationFocus {
    Field(usize),
    OpenUrl,
    Decline,
    Cancel,
    Accept,
}

#[derive(Clone, PartialEq, Eq)]
pub struct McpElicitationPresentation {
    /// Stable key: connection generation plus the original request id.
    pub request_id: String,
    pub server_name: String,
    pub mode: McpElicitationModePresentation,
    pub status: McpElicitationStatus,
    pub last_action: Option<AgentMcpElicitationAction>,
    pub failure_message: Option<String>,
    pub keyboard_focus: Option<McpElicitationFocus>,
}

impl fmt::Debug for McpElicitationPresentation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, field_count) = match &self.mode {
            McpElicitationModePresentation::Form { fields, .. } => ("form", fields.len()),
            McpElicitationModePresentation::Url { .. } => ("url", 0),
        };
        formatter
            .debug_struct("McpElicitationPresentation")
            .field("request_id", &self.request_id)
            .field("server_name", &self.server_name)
            .field("mode", &kind)
            .field("field_count", &field_count)
            .field("status", &self.status)
            .field("last_action", &self.last_action)
            .field("keyboard_focus", &self.keyboard_focus)
            .field("failure_message", &self.failure_message)
            .finish()
    }
}

impl McpElicitationPresentation {
    pub fn pending(request_id: impl Into<String>, request: &AgentMcpElicitationRequest) -> Self {
        let mode = match &request.mode {
            AgentMcpElicitationMode::Form(form) => McpElicitationModePresentation::Form {
                message: form.message.clone(),
                fields: form.fields.iter().map(elicited_field).collect(),
            },
            AgentMcpElicitationMode::Url(url) => McpElicitationModePresentation::Url {
                message: url.message.clone(),
                elicitation_id: url.elicitation_id.clone(),
                url: url.url.clone(),
                opened: false,
            },
        };
        let keyboard_focus = match &mode {
            McpElicitationModePresentation::Form { fields, .. } => {
                fields.first().map(|_| McpElicitationFocus::Field(0))
            }
            McpElicitationModePresentation::Url { .. } => Some(McpElicitationFocus::OpenUrl),
        };
        Self {
            request_id: request_id.into(),
            server_name: request.server_name.clone(),
            mode,
            status: McpElicitationStatus::Pending,
            last_action: None,
            failure_message: None,
            keyboard_focus,
        }
    }

    pub fn is_interactive(&self) -> bool {
        self.status.is_interactive()
    }

    /// Form elicitations are modal to the composer while they are pending or
    /// being written. URL elicitations intentionally remain in the activity
    /// stream, matching ChatGPT's inline action card and leaving the composer
    /// available for ordinary navigation.
    pub fn is_overlay_visible(&self) -> bool {
        matches!(self.mode, McpElicitationModePresentation::Form { .. })
            && self.status.is_overlay_visible()
    }

    /// A URL request is rendered inline until its response is acknowledged.
    /// Keeping the submitting state mounted prevents a one-frame disappearance
    /// while the app-server sends the matching resolved event.
    pub fn is_inline_url_visible(&self) -> bool {
        matches!(self.mode, McpElicitationModePresentation::Url { .. })
            && self.status.is_overlay_visible()
    }

    /// Only form requests take keyboard ownership from the composer.
    pub fn blocks_keyboard(&self) -> bool {
        matches!(self.mode, McpElicitationModePresentation::Form { .. })
            && self.status.is_interactive()
    }

    pub fn fields(&self) -> &[McpElicitationFieldPresentation] {
        match &self.mode {
            McpElicitationModePresentation::Form { fields, .. } => fields,
            McpElicitationModePresentation::Url { .. } => &[],
        }
    }

    pub fn message(&self) -> &str {
        match &self.mode {
            McpElicitationModePresentation::Form { message, .. }
            | McpElicitationModePresentation::Url { message, .. } => message,
        }
    }

    pub fn url(&self) -> Option<(&str, &str, bool)> {
        match &self.mode {
            McpElicitationModePresentation::Url {
                elicitation_id,
                url,
                opened,
                ..
            } => Some((elicitation_id, url, *opened)),
            McpElicitationModePresentation::Form { .. } => None,
        }
    }

    /// Ordered focus targets for Tab traversal.
    pub fn focus_targets(&self) -> Vec<McpElicitationFocus> {
        let mut targets = (0..self.fields().len())
            .map(McpElicitationFocus::Field)
            .collect::<Vec<_>>();
        if self.mode_has_url() {
            targets.push(McpElicitationFocus::OpenUrl);
        }
        targets.push(McpElicitationFocus::Cancel);
        targets.push(McpElicitationFocus::Decline);
        targets.push(McpElicitationFocus::Accept);
        targets
    }

    fn mode_has_url(&self) -> bool {
        matches!(self.mode, McpElicitationModePresentation::Url { .. })
    }

    /// Next focus target in the CDP-observed Tab order.
    pub fn next_focus(
        &self,
        current: Option<McpElicitationFocus>,
        backwards: bool,
    ) -> McpElicitationFocus {
        let targets = self.focus_targets();
        let Some(current) = current else {
            return if backwards {
                targets
                    .last()
                    .copied()
                    .unwrap_or(McpElicitationFocus::Accept)
            } else {
                targets
                    .first()
                    .copied()
                    .unwrap_or(McpElicitationFocus::Accept)
            };
        };
        let index = targets
            .iter()
            .position(|target| *target == current)
            .unwrap_or(0);
        let count = targets.len().max(1);
        let next = if backwards {
            (index + count - 1) % count
        } else {
            (index + 1) % count
        };
        targets[next]
    }

    pub fn focus_field(&mut self, index: usize) -> bool {
        if index < self.fields().len() {
            self.keyboard_focus = Some(McpElicitationFocus::Field(index));
            true
        } else {
            false
        }
    }

    pub fn set_focus(&mut self, focus: McpElicitationFocus) -> bool {
        match focus {
            McpElicitationFocus::Field(index) => self.focus_field(index),
            McpElicitationFocus::OpenUrl => {
                if self.mode_has_url() {
                    self.keyboard_focus = Some(focus);
                    true
                } else {
                    false
                }
            }
            McpElicitationFocus::Decline
            | McpElicitationFocus::Cancel
            | McpElicitationFocus::Accept => {
                self.keyboard_focus = Some(focus);
                true
            }
        }
    }

    pub fn focused_field(&self) -> Option<usize> {
        match self.keyboard_focus {
            Some(McpElicitationFocus::Field(index)) => Some(index),
            _ => None,
        }
    }

    pub fn field_mut(&mut self, index: usize) -> Option<&mut McpElicitationFieldPresentation> {
        match &mut self.mode {
            McpElicitationModePresentation::Form { fields, .. } => fields.get_mut(index),
            McpElicitationModePresentation::Url { .. } => None,
        }
    }

    pub fn field(&self, index: usize) -> Option<&McpElicitationFieldPresentation> {
        self.fields().get(index)
    }

    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields().iter().position(|field| field.name == name)
    }

    pub fn set_field_text(&mut self, name: &str, text: String) -> bool {
        let Some(index) = self.field_index(name) else {
            return false;
        };
        let Some(field) = self.field_mut(index) else {
            return false;
        };
        field.set_text(text);
        true
    }

    pub fn toggle_boolean(&mut self, index: usize) -> bool {
        let Some(field) = self.field_mut(index) else {
            return false;
        };
        let McpElicitationFieldValueState::Boolean(value) = field.value else {
            return false;
        };
        field.value = McpElicitationFieldValueState::Boolean(!value);
        field.error = None;
        true
    }

    pub fn select_option(&mut self, index: usize, option: usize) -> bool {
        let Some(field) = self.field_mut(index) else {
            return false;
        };
        if !matches!(
            field.control,
            McpElicitationFieldControl::SingleSelect { .. }
        ) || option >= field.options().len()
        {
            return false;
        }
        field.value = McpElicitationFieldValueState::Selection(Some(option));
        field.error = None;
        true
    }

    pub fn toggle_multi_option(&mut self, index: usize, option: usize) -> bool {
        let Some(field) = self.field_mut(index) else {
            return false;
        };
        if !matches!(
            field.control,
            McpElicitationFieldControl::MultiSelect { .. }
        ) || option >= field.options().len()
        {
            return false;
        }
        let mut selected = field
            .options()
            .iter()
            .enumerate()
            .filter_map(|(index, _)| field.is_selected(index).then_some(index))
            .collect::<Vec<_>>();
        if let Some(position) = selected.iter().position(|value| *value == option) {
            selected.remove(position);
        } else {
            selected.push(option);
            selected.sort_unstable();
        }
        field.value = McpElicitationFieldValueState::MultiSelection(selected);
        field.error = None;
        true
    }

    pub fn mark_url_opened(&mut self) -> bool {
        let McpElicitationModePresentation::Url { opened, .. } = &mut self.mode else {
            return false;
        };
        *opened = true;
        true
    }

    /// Validate every field locally before a response is written. Local
    /// validation is the recoverable path: the request stays pending and the
    /// offending fields keep their own error.
    pub fn validate(&mut self) -> Result<AgentMcpElicitationContent, Vec<String>> {
        let McpElicitationModePresentation::Form { fields, .. } = &mut self.mode else {
            return Ok(AgentMcpElicitationContent::default());
        };
        let mut errors = Vec::new();
        let mut content = Vec::new();
        for field in fields.iter_mut() {
            match validate_field(field) {
                Ok(Some(value)) => content.push(AgentMcpElicitationFieldValue {
                    name: field.name.clone(),
                    value,
                }),
                Ok(None) => {}
                Err(error) => {
                    field.error = Some(error.clone());
                    errors.push(error);
                }
            }
        }
        if errors.is_empty() {
            Ok(AgentMcpElicitationContent { fields: content })
        } else {
            Err(errors)
        }
    }

    pub fn first_invalid_field(&self) -> Option<usize> {
        self.fields().iter().position(|field| field.error.is_some())
    }

    pub fn focus_first_invalid_field(&mut self) -> bool {
        match self.first_invalid_field() {
            Some(index) => self.focus_field(index),
            None => false,
        }
    }

    pub fn mark_submitted(&mut self, action: AgentMcpElicitationAction) {
        self.status = McpElicitationStatus::Submitting;
        self.last_action = Some(action);
        self.failure_message = None;
    }

    pub fn mark_resolved(&mut self) {
        self.status = match self.last_action {
            Some(AgentMcpElicitationAction::Accept) => McpElicitationStatus::Accepted,
            Some(AgentMcpElicitationAction::Decline) => McpElicitationStatus::Declined,
            Some(AgentMcpElicitationAction::Cancel) => McpElicitationStatus::Cancelled,
            None => McpElicitationStatus::Accepted,
        };
    }

    pub fn mark_invalid(&mut self, message: impl Into<String>) {
        self.status = McpElicitationStatus::Invalid;
        self.failure_message = Some(message.into());
    }

    /// The server or its transport ended the request without a user action.
    pub fn mark_cancelled(&mut self, message: impl Into<String>) {
        self.status = McpElicitationStatus::Cancelled;
        self.failure_message = Some(message.into());
    }

    pub fn mark_write_failed(&mut self, message: impl Into<String>) {
        self.status = McpElicitationStatus::Invalid;
        self.failure_message = Some(message.into());
    }

    /// Keyboard act on the focused target. Enter submits the form when the
    /// focused target is a text field and every field is valid.
    pub fn activate_focus(&self) -> Option<McpElicitationEvent> {
        match self.keyboard_focus? {
            McpElicitationFocus::Field(index) => {
                let field = self.field(index)?;
                match field.control {
                    McpElicitationFieldControl::Boolean => {
                        Some(McpElicitationEvent::ToggleBoolean { field: index })
                    }
                    McpElicitationFieldControl::SingleSelect { .. }
                    | McpElicitationFieldControl::MultiSelect { .. } => {
                        Some(McpElicitationEvent::Accept)
                    }
                    McpElicitationFieldControl::Text { .. }
                    | McpElicitationFieldControl::Number { .. } => {
                        Some(McpElicitationEvent::Accept)
                    }
                }
            }
            McpElicitationFocus::OpenUrl => Some(McpElicitationEvent::OpenUrl),
            McpElicitationFocus::Decline => Some(McpElicitationEvent::Decline),
            McpElicitationFocus::Cancel => Some(McpElicitationEvent::Cancel),
            McpElicitationFocus::Accept => Some(McpElicitationEvent::Accept),
        }
    }
}

fn validate_field(
    field: &McpElicitationFieldPresentation,
) -> Result<Option<AgentMcpElicitationValue>, String> {
    let title = field.display_title().to_owned();
    if field.is_text_like() {
        let text = field.text();
        if text.trim().is_empty() {
            return if field.required {
                Err(format!("{title} 为必填项"))
            } else {
                Ok(None)
            };
        }
    }
    match (&field.control, &field.value) {
        (McpElicitationFieldControl::Text { .. }, McpElicitationFieldValueState::Text(text)) => {
            if text.trim().is_empty() && !field.required {
                return Ok(None);
            }
            Ok(Some(AgentMcpElicitationValue::String(text.clone())))
        }
        (
            McpElicitationFieldControl::Number {
                integer,
                minimum,
                maximum,
            },
            McpElicitationFieldValueState::Text(text),
        ) => {
            if text.trim().is_empty() && !field.required {
                return Ok(None);
            }
            let parsed = field.domain_value()?;
            let AgentMcpElicitationValue::Number(number) = &parsed else {
                return Err(format!("{title} 必须是数字"));
            };
            if *integer && number.as_i64().is_none() && number.as_u64().is_none() {
                return Err(format!("{title} 必须是整数"));
            }
            let numeric = number
                .as_f64()
                .ok_or_else(|| format!("{title} 必须是数字"))?;
            if let Some(minimum) = minimum
                && let Ok(minimum) = minimum.parse::<f64>()
                && numeric < minimum
            {
                return Err(format!("{title} 不能小于 {minimum}"));
            }
            if let Some(maximum) = maximum
                && let Ok(maximum) = maximum.parse::<f64>()
                && numeric > maximum
            {
                return Err(format!("{title} 不能大于 {maximum}"));
            }
            Ok(Some(parsed))
        }
        (McpElicitationFieldControl::Boolean, McpElicitationFieldValueState::Boolean(value)) => {
            Ok(Some(AgentMcpElicitationValue::Boolean(*value)))
        }
        (
            McpElicitationFieldControl::SingleSelect { .. },
            McpElicitationFieldValueState::Selection(index),
        ) => {
            if index.is_none() {
                return if field.required {
                    Err(format!("{title} 需要选择一个选项"))
                } else {
                    Ok(None)
                };
            }
            Ok(Some(field.domain_value()?))
        }
        (
            McpElicitationFieldControl::MultiSelect {
                min_items,
                max_items,
                ..
            },
            McpElicitationFieldValueState::MultiSelection(indices),
        ) => {
            let count = indices.len() as u64;
            if let Some(minimum) = min_items
                && count < *minimum
            {
                return Err(format!("{title} 至少需要选择 {minimum} 项"));
            }
            if let Some(maximum) = max_items
                && count > *maximum
            {
                return Err(format!("{title} 最多允许选择 {maximum} 项"));
            }
            if count == 0 {
                return if field.required {
                    Err(format!("{title} 至少需要选择一项"))
                } else {
                    Ok(None)
                };
            }
            Ok(Some(field.domain_value()?))
        }
        _ => Err(format!("{title} 的输入类型与请求不一致")),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpElicitationEvent {
    Focus(McpElicitationFocus),
    ToggleBoolean { field: usize },
    SelectOption { field: usize, option: usize },
    ToggleMultiOption { field: usize, option: usize },
    OpenUrl,
    Accept,
    Decline,
    Cancel,
}

pub type McpElicitationCallback = UiCallback<McpElicitationEvent>;

#[derive(Clone, Copy)]
struct McpElicitationPalette {
    card: gpui::Rgba,
    outline: gpui::Rgba,
    text: gpui::Rgba,
    secondary: gpui::Rgba,
    soft: gpui::Rgba,
    secondary_button: gpui::Rgba,
    footer_border: gpui::Rgba,
    border: gpui::Rgba,
    number_border: gpui::Rgba,
    focus: gpui::Rgba,
    primary: gpui::Rgba,
    primary_text: gpui::Rgba,
    error: gpui::Rgba,
    error_border: gpui::Rgba,
}

impl McpElicitationPalette {
    fn for_theme(theme: Theme) -> Self {
        if theme.surface == rgba(0x181818ff) {
            Self {
                // CDP dark card: rgb(24, 24, 24) with a 1px white/8.2% outline.
                card: rgba(0x181818ff),
                outline: rgba(0xffffff15),
                text: rgba(0xdfdfdfff),
                // CDP: rgba(255, 255, 255, 0.494) for descriptions and
                // secondary buttons; selections use the 5.5% white wash.
                secondary: rgba(0xffffff7e),
                soft: rgba(0xffffff0e),
                secondary_button: rgba(0xffffff0e),
                footer_border: rgba(0xffffff0a),
                border: rgba(0xffffff15),
                number_border: rgba(0xffffff1e),
                focus: rgba(0xffffff4d),
                primary: rgba(0xdfdfdfff),
                primary_text: rgba(0x2d2d2dff),
                error: rgba(0xe02e2aff),
                error_border: rgba(0xff8583ff),
            }
        } else {
            Self {
                card: rgba(0xffffffff),
                outline: rgba(0x1a1c1f14),
                text: rgba(0x1a1c1fff),
                secondary: rgba(0x1a1c1f7e),
                // Chromium's `color-mix` selection wash resolves to an
                // opaque #f4f4f4 on the light card. Keeping this as a solid
                // token avoids a blue-channel rounding difference in GPUI's
                // Retina downsample.
                soft: rgba(0xf4f4f4ff),
                secondary_button: rgba(0x1a1c1f0e),
                footer_border: rgba(0x1a1c1f0a),
                border: rgba(0x1a1c1f14),
                number_border: rgba(0x1a1c1f1e),
                focus: rgba(0x1a1c1f4d),
                primary: rgba(0x1a1c1fff),
                primary_text: rgba(0xffffffff),
                // The light ChatGPT validation token composites to #ce4035
                // in the captured surface (the DOM reports the pre-mix
                // #e02e2a token). Use the resolved raster color here.
                error: rgba(0xce4035ff),
                // Input outlines retain the pre-mix danger token; only the
                // validation copy is color-managed by the browser surface.
                error_border: rgba(0xe02e2aff),
            }
        }
    }

    /// CDP: the form card paints only a 1px outline plus a very light shadow;
    /// the measurement showed no visible drop shadow beyond that ring.
    fn shadows(self) -> Vec<BoxShadow> {
        vec![BoxShadow::new(px(0.0), px(0.0), self.outline.into()).spread_radius(px(0.5))]
    }
}

fn elicited_field(field: &AgentMcpElicitationField) -> McpElicitationFieldPresentation {
    McpElicitationFieldPresentation {
        name: field.name.clone(),
        title: field.title.clone().unwrap_or_else(|| field.name.clone()),
        description: field.description.clone(),
        required: field.required,
        control: control_for(field),
        value: value_for(field),
        error: None,
    }
}

fn control_for(field: &AgentMcpElicitationField) -> McpElicitationFieldControl {
    match &field.kind {
        AgentMcpElicitationFieldKind::String { format, .. } => McpElicitationFieldControl::Text {
            placeholder: match format {
                Some(AgentMcpElicitationStringFormat::Email) => "name@example.com",
                Some(AgentMcpElicitationStringFormat::Uri) => "https://",
                Some(AgentMcpElicitationStringFormat::Date) => "2026-09-13",
                Some(AgentMcpElicitationStringFormat::DateTime) => "2026-09-13T08:00:00Z",
                None => "",
            }
            .to_owned(),
            secret: false,
        },
        AgentMcpElicitationFieldKind::Number {
            integer,
            minimum,
            maximum,
        } => McpElicitationFieldControl::Number {
            integer: *integer,
            minimum: minimum.as_ref().map(serde_json::Number::to_string),
            maximum: maximum.as_ref().map(serde_json::Number::to_string),
        },
        AgentMcpElicitationFieldKind::Boolean => McpElicitationFieldControl::Boolean,
        AgentMcpElicitationFieldKind::SingleSelect { options } => {
            McpElicitationFieldControl::SingleSelect {
                options: options
                    .iter()
                    .map(|option| McpElicitationOptionPresentation {
                        value: option.value.clone(),
                        title: option.title.clone(),
                    })
                    .collect(),
            }
        }
        AgentMcpElicitationFieldKind::MultiSelect {
            options,
            min_items,
            max_items,
        } => McpElicitationFieldControl::MultiSelect {
            options: options
                .iter()
                .map(|option| McpElicitationOptionPresentation {
                    value: option.value.clone(),
                    title: option.title.clone(),
                })
                .collect(),
            min_items: *min_items,
            max_items: *max_items,
        },
    }
}

fn value_for(field: &AgentMcpElicitationField) -> McpElicitationFieldValueState {
    match (&field.kind, &field.default) {
        (_, Some(AgentMcpElicitationValue::String(default))) => match &field.kind {
            AgentMcpElicitationFieldKind::SingleSelect { options } => {
                let index = options.iter().position(|option| &option.value == default);
                McpElicitationFieldValueState::Selection(index)
            }
            _ => McpElicitationFieldValueState::Text(default.clone()),
        },
        (_, Some(AgentMcpElicitationValue::Number(default))) => {
            McpElicitationFieldValueState::Text(default.to_string())
        }
        (_, Some(AgentMcpElicitationValue::Boolean(default))) => {
            McpElicitationFieldValueState::Boolean(*default)
        }
        (
            AgentMcpElicitationFieldKind::MultiSelect { options, .. },
            Some(AgentMcpElicitationValue::StringArray(default)),
        ) => {
            let selected = default
                .iter()
                .filter_map(|value| options.iter().position(|option| &option.value == value))
                .collect();
            McpElicitationFieldValueState::MultiSelection(selected)
        }
        (AgentMcpElicitationFieldKind::Boolean, None) => {
            McpElicitationFieldValueState::Boolean(false)
        }
        (AgentMcpElicitationFieldKind::SingleSelect { .. }, None) => {
            McpElicitationFieldValueState::Selection(None)
        }
        (AgentMcpElicitationFieldKind::MultiSelect { .. }, None) => {
            McpElicitationFieldValueState::MultiSelection(Vec::new())
        }
        _ => McpElicitationFieldValueState::Text(String::new()),
    }
}

/// Render one elicitation card. Submitted and invalid requests stay mounted in
/// a disabled status state until the protocol reports a resolution.
pub fn render_mcp_elicitation(
    model: &McpElicitationPresentation,
    theme: Theme,
    text_input: Entity<PromptInput>,
    callback: McpElicitationCallback,
) -> Option<Stateful<Div>> {
    if !model.status.should_render() {
        return None;
    }
    let palette = McpElicitationPalette::for_theme(theme);
    if !model.status.is_interactive() {
        return Some(render_status_card(model, palette));
    }
    let header = render_header(model, palette, callback.clone());
    let body = match &model.mode {
        McpElicitationModePresentation::Form { fields, .. } => {
            let mut column = div().flex().flex_col().gap(px(MCP_ELICITATION_FIELD_GAP));
            for (index, field) in fields.iter().enumerate() {
                column = column.child(render_field(
                    model,
                    index,
                    field,
                    palette,
                    text_input.clone(),
                    callback.clone(),
                ));
            }
            column
        }
        McpElicitationModePresentation::Url { url, opened, .. } => {
            render_url_body(model, url, *opened, palette, callback.clone())
        }
    };
    let footer = render_footer(model, palette, callback.clone());
    Some(
        div()
            .id(element_id(
                "mcp-elicitation-card",
                &model.request_id,
                "card",
            ))
            .role(Role::Form)
            .aria_label(model.message().to_owned())
            .w_full()
            .overflow_hidden()
            .rounded(px(card_radius(&model.mode)))
            .bg(palette.card)
            .shadow(palette.shadows())
            .border_1()
            .border_color(palette.outline)
            // ChatGPT uses the platform UI stack for elicitation forms. This
            // keeps CJK fallback and Latin punctuation on the same CoreText
            // metrics as the reference client.
            .font_family(".SystemUIFont")
            .text_color(palette.text)
            .child(header)
            .child(div().px(px(8.0)).pt(px(0.0)).pb(px(8.0)).child(body))
            .child(footer),
    )
}

/// Render a URL elicitation in the conversation stream.
///
/// ChatGPT uses the same compact action surface as tool suggestions: a
/// 40&nbsp;px connector icon, an action-required title, a two-line description,
/// and right-aligned “暂不”/“打开链接” (or “继续”) buttons. It is deliberately
/// separate from the form renderer because URL requests do not block the
/// composer or create a bottom overlay.
pub fn render_mcp_elicitation_url_activity(
    model: &McpElicitationPresentation,
    theme: Theme,
    callback: McpElicitationCallback,
) -> Option<Stateful<Div>> {
    let McpElicitationModePresentation::Url {
        message,
        url,
        opened,
        ..
    } = &model.mode
    else {
        return None;
    };
    if !model.status.should_render() {
        return None;
    }
    if !model.status.is_interactive() {
        return Some(render_status_card(
            model,
            McpElicitationPalette::for_theme(theme),
        ));
    }

    let palette = McpElicitationPalette::for_theme(theme);
    // The compact action surface uses a white primary pill in dark mode,
    // whereas the larger elicitation form intentionally uses the softer
    // `#dfdfdf` primary token. Keep the two surfaces independent.
    let mut url_palette = palette;
    if theme.surface == rgba(0x181818ff) {
        url_palette.primary = rgba(0xffffffff);
        url_palette.primary_text = rgba(0x1a1c1fff);
    }
    let title_text = if theme.surface == rgba(0x181818ff) {
        rgba(0xffffffff)
    } else {
        rgba(0x1a1c1fff)
    };
    let description_text = if theme.surface == rgba(0x181818ff) {
        // The Electron token is `color-mix(... var(--color-text) 70%,
        // transparent)`, which composites to approximately rgb(186,186,186)
        // on the #181818 conversation surface.
        rgba(0xffffffb3)
    } else {
        rgba(0x1a1c1fb3)
    };
    let tile_surface = if theme.surface == rgba(0x181818ff) {
        rgba(0x141414ff)
    } else {
        rgba(0xf4f4f4ff)
    };
    let secondary_surface = if theme.surface == rgba(0x181818ff) {
        // `color=outline` buttons use a subtle white wash over the card.
        rgba(0xffffff08)
    } else {
        rgba(0x1a1c1f08)
    };
    let secondary_border = if theme.surface == rgba(0x181818ff) {
        rgba(0xffffff14)
    } else {
        rgba(0x1a1c1f14)
    };

    let opened_for_primary = *opened;
    let primary_callback = callback.clone();
    let button_style = UrlActionButtonStyle {
        palette: url_palette,
        title_text,
        secondary_surface,
        secondary_border,
    };
    let primary = url_action_button(
        &model.request_id,
        if opened_for_primary {
            "继续"
        } else {
            "打开链接"
        },
        true,
        false,
        button_style,
        move |window, cx| {
            primary_callback.emit(
                if opened_for_primary {
                    McpElicitationEvent::Accept
                } else {
                    McpElicitationEvent::OpenUrl
                },
                window,
                cx,
            )
        },
    );
    let decline_callback = callback.clone();
    let decline = url_action_button(
        &model.request_id,
        "暂不",
        false,
        false,
        button_style,
        move |window, cx| decline_callback.emit(McpElicitationEvent::Decline, window, cx),
    );

    let message_line = div()
        .min_w(px(0.0))
        .max_w_full()
        .text_size(px(13.0))
        .line_height(px(20.0))
        .text_color(description_text)
        .child(message.clone());
    let url_line = div()
        .min_w(px(0.0))
        .max_w_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .text_size(px(13.0))
        .line_height(px(20.0))
        .child(div().flex_none().text_color(description_text).child("URL"))
        .child(
            div()
                .min_w(px(0.0))
                .overflow_hidden()
                .text_color(title_text)
                .child(url.clone()),
        );
    let description = div()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(message_line)
        .child(url_line);
    let content = div()
        .min_w(px(0.0))
        .flex_1()
        .flex_basis(px(256.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .size(px(40.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.0))
                .bg(tile_surface)
                // The ChatGPT plugin-light-24 asset keeps its native 24px
                // box inside the 40px tile (the path itself resolves to a
                // 20px visible mark). Scaling the SVG to 20px would make the
                // visible ring four pixels too small.
                .child(icon("mcp-action-required", description_text.into()).size(px(24.0))),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(0.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(MCP_ELICITATION_TITLE_SIZE))
                        .line_height(px(MCP_ELICITATION_TITLE_LINE_HEIGHT))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(title_text)
                        .child("需要采取行动"),
                )
                .child(description),
        );
    let actions = div()
        .flex_none()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_end()
        .gap(px(8.0))
        .child(decline)
        .child(primary);

    Some(
        div()
            .id(element_id(
                "mcp-elicitation-url-card",
                &model.request_id,
                "activity",
            ))
            .role(Role::Form)
            .aria_label(format!("需要采取行动，{}", message))
            .w_full()
            .overflow_hidden()
            // Uir.Root in ChatGPT uses `rounded-xl` (12px), unlike the
            // larger form card's rounded-3xl treatment.
            .rounded(px(12.0))
            .border_1()
            .border_color(if theme.surface == rgba(0x181818ff) {
                // CDP's dark compact card resolves to an opaque #2b ring;
                // using the resolved value avoids a second alpha composite in
                // GPUI's Retina downsample.
                rgba(0x2b2b2bff)
            } else {
                palette.outline
            })
            .bg(palette.card)
            // ChatGPT's compact action card inherits the platform UI font;
            // keeping Latin URL glyphs on the same fallback stack matters for
            // the one-pixel text metrics in the CDP reference.
            .font_family(".SystemUIFont")
            .text_color(title_text)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .p(px(12.0))
                    .child(content)
                    .child(actions),
            ),
    )
}

#[derive(Clone, Copy)]
struct UrlActionButtonStyle {
    palette: McpElicitationPalette,
    title_text: gpui::Rgba,
    secondary_surface: gpui::Rgba,
    secondary_border: gpui::Rgba,
}

fn url_action_button(
    request_id: &str,
    label: &str,
    primary: bool,
    disabled: bool,
    style: UrlActionButtonStyle,
    handler: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    let UrlActionButtonStyle {
        palette,
        title_text,
        secondary_surface,
        secondary_border,
    } = style;
    let (background, foreground, border) = if primary {
        (palette.primary, palette.primary_text, rgba(0x00000000))
    } else {
        (secondary_surface, title_text, secondary_border)
    };
    // The reference's four-glyph “打开链接” pill is 70px wide at the
    // captured 13px UI font; the form's 72px submit pill is a separate token.
    let label_width = if primary { 70.0 } else { 44.0 };
    div()
        .id(element_id("mcp-elicitation-url-action", request_id, label))
        .role(Role::Button)
        .aria_label(label.to_owned())
        .focusable()
        .tab_stop(true)
        .h(px(28.0))
        .w(px(label_width))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(14.0))
        .bg(background)
        .text_color(foreground)
        .border_1()
        .border_color(border)
        .when(!disabled, |button| button.cursor_pointer())
        .when(disabled, |button| button.opacity(0.55))
        .when(!primary && !disabled, |button| {
            button.hover(move |button| button.bg(palette.border))
        })
        .when(primary && !disabled, |button| {
            button
                .hover(|button| button.opacity(0.9))
                .active(|button| button.opacity(0.8))
        })
        .on_click(move |_, window, cx| {
            if !disabled {
                handler(window, cx)
            }
        })
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::MEDIUM)
                .child(label.to_owned()),
        )
}

fn card_radius(mode: &McpElicitationModePresentation) -> f32 {
    match mode {
        McpElicitationModePresentation::Form { .. } => MCP_ELICITATION_CARD_RADIUS,
        McpElicitationModePresentation::Url { .. } => MCP_ELICITATION_URL_CARD_RADIUS,
    }
}

fn render_header(
    model: &McpElicitationPresentation,
    palette: McpElicitationPalette,
    callback: McpElicitationCallback,
) -> Div {
    let dismiss_callback = callback.clone();
    div()
        .h(px(MCP_ELICITATION_HEADER_HEIGHT))
        .pl(px(MCP_ELICITATION_CONTENT_PADDING))
        .pr(px(MCP_ELICITATION_CONTENT_PADDING))
        .pt(px(MCP_ELICITATION_CONTENT_PADDING))
        .pb(px(8.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(MCP_ELICITATION_TITLE_SIZE))
                .line_height(px(MCP_ELICITATION_TITLE_LINE_HEIGHT))
                .font_weight(FontWeight::MEDIUM)
                .child(icon("mcp-form-server", palette.text.into()).size(px(18.0)))
                .child(model.message().to_owned()),
        )
        .child(
            div()
                .id(element_id(
                    "mcp-elicitation-dismiss",
                    &model.request_id,
                    "close",
                ))
                .role(Role::Button)
                .aria_label("取消")
                .size(px(28.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(14.0))
                .text_color(palette.secondary)
                .cursor_pointer()
                .when(
                    model.keyboard_focus == Some(McpElicitationFocus::Cancel),
                    |button| button.border_1().border_color(palette.focus),
                )
                .hover(move |button| button.bg(palette.soft))
                .on_click(move |_, window, cx| {
                    dismiss_callback.emit(McpElicitationEvent::Cancel, window, cx);
                })
                .child(icon("close-dialog", palette.text.into()).size(px(16.0))),
        )
}

fn render_field(
    model: &McpElicitationPresentation,
    index: usize,
    field: &McpElicitationFieldPresentation,
    palette: McpElicitationPalette,
    text_input: Entity<PromptInput>,
    callback: McpElicitationCallback,
) -> gpui::AnyElement {
    let focused = model.keyboard_focus == Some(McpElicitationFocus::Field(index));
    // ChatGPT presents a boolean request as one option row.  Its title is
    // the row label, so the generic label/control wrapper would duplicate the
    // title and add a full line of unnecessary vertical rhythm.
    if matches!(&field.control, McpElicitationFieldControl::Boolean) {
        return render_boolean_control(index, field, focused, palette, callback).into_any_element();
    }
    let select_control = matches!(
        &field.control,
        McpElicitationFieldControl::SingleSelect { .. }
            | McpElicitationFieldControl::MultiSelect { .. }
    );
    let mut column = div()
        .flex()
        .flex_col()
        .gap(px(if select_control { 0.0 } else { 4.0 }))
        .px(px(if select_control { 0.0 } else { 8.0 }))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(MCP_ELICITATION_LABEL_SIZE))
                        .line_height(px(if select_control {
                            19.5
                        } else {
                            MCP_ELICITATION_LABEL_LINE_HEIGHT
                        }))
                        .font_weight(FontWeight::MEDIUM)
                        .child(field.display_title().to_owned()),
                )
                .when(field.required, |row| {
                    // CDP: the reference card marks required fields only through
                    // validation ("填写此字段以继续"), never with a visible chip.
                    row
                }),
        );
    if let Some(description) = &field.description {
        column = column.child(
            div()
                .text_size(px(MCP_ELICITATION_LABEL_SIZE))
                .line_height(px(if select_control {
                    19.5
                } else {
                    MCP_ELICITATION_LABEL_LINE_HEIGHT
                }))
                .text_color(palette.secondary)
                .child(description.clone()),
        );
    }
    let control = render_control(model, index, field, focused, palette, text_input, callback);
    column = if select_control && field.description.is_some() {
        column.child(div().mt(px(4.0)).child(control))
    } else {
        column.child(control)
    };
    if let Some(error) = &field.error {
        column = column.child(
            div()
                .text_size(px(MCP_ELICITATION_LABEL_SIZE))
                // The DOM uses `pt-1` plus the normal 18.5714px text line;
                // keeping those separate preserves both the baseline and the
                // 22.5625px validation block height.
                .pt(px(4.0))
                .line_height(px(MCP_ELICITATION_LABEL_LINE_HEIGHT))
                .text_color(palette.error)
                .child(error.clone()),
        );
    }
    column.into_any_element()
}

fn render_control(
    model: &McpElicitationPresentation,
    index: usize,
    field: &McpElicitationFieldPresentation,
    focused: bool,
    palette: McpElicitationPalette,
    text_input: Entity<PromptInput>,
    callback: McpElicitationCallback,
) -> impl IntoElement {
    match &field.control {
        McpElicitationFieldControl::Text { .. } | McpElicitationFieldControl::Number { .. } => {
            render_text_control(model, index, field, focused, palette, text_input, callback)
                .into_any_element()
        }
        McpElicitationFieldControl::Boolean => {
            render_boolean_control(index, field, focused, palette, callback).into_any_element()
        }
        McpElicitationFieldControl::SingleSelect { options } => {
            render_select_control(index, field, options, false, focused, palette, callback)
                .into_any_element()
        }
        McpElicitationFieldControl::MultiSelect { options, .. } => {
            render_select_control(index, field, options, true, focused, palette, callback)
                .into_any_element()
        }
    }
}

fn render_text_control(
    model: &McpElicitationPresentation,
    index: usize,
    field: &McpElicitationFieldPresentation,
    focused: bool,
    palette: McpElicitationPalette,
    text_input: Entity<PromptInput>,
    callback: McpElicitationCallback,
) -> Stateful<Div> {
    let focus_callback = callback.clone();
    let text = field.text().to_owned();
    let empty = text.trim().is_empty();
    let invalid = field.error.is_some();
    div()
        .id(element_id(
            "mcp-elicitation-field",
            &model.request_id,
            field.name.as_str(),
        ))
        .role(Role::TextInput)
        .aria_label(field.display_title().to_owned())
        .h(px(
            if matches!(&field.control, McpElicitationFieldControl::Number { .. }) {
                36.0
            } else {
                MCP_ELICITATION_CONTROL_HEIGHT
            },
        ))
        .w_full()
        .px(px(11.0))
        .flex()
        .items_center()
        .rounded(px(
            if matches!(&field.control, McpElicitationFieldControl::Number { .. }) {
                10.0
            } else {
                15.0
            },
        ))
        .bg(rgba(0x00000000))
        .border_1()
        .border_color(if invalid {
            palette.error_border
        } else if focused || (index == 0 && empty) {
            palette.focus
        } else if matches!(&field.control, McpElicitationFieldControl::Number { .. }) {
            palette.number_border
        } else {
            palette.outline
        })
        .cursor_text()
        .on_click(move |_, window, cx| {
            focus_callback.emit(
                McpElicitationEvent::Focus(McpElicitationFocus::Field(index)),
                window,
                cx,
            );
        })
        .child(if focused {
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(text_input)
                .into_any_element()
        } else if empty {
            div()
                .text_size(px(MCP_ELICITATION_LABEL_SIZE))
                .line_height(px(MCP_ELICITATION_LABEL_LINE_HEIGHT))
                .text_color(palette.secondary)
                .child(field.placeholder())
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .text_size(px(MCP_ELICITATION_LABEL_SIZE))
                .line_height(px(MCP_ELICITATION_LABEL_LINE_HEIGHT))
                .child(text)
                .into_any_element()
        })
}

fn render_boolean_control(
    index: usize,
    field: &McpElicitationFieldPresentation,
    focused: bool,
    palette: McpElicitationPalette,
    callback: McpElicitationCallback,
) -> Stateful<Div> {
    let on = matches!(field.value, McpElicitationFieldValueState::Boolean(true));
    let toggle_callback = callback.clone();
    div()
        .id(element_id("mcp-elicitation-boolean", &field.name, index))
        .role(Role::CheckBox)
        .aria_label(format!(
            "{} {}",
            field.display_title(),
            field.accessible_value()
        ))
        .h(px(MCP_ELICITATION_OPTION_HEIGHT))
        .w_full()
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(15.0))
        .when(on, |row| row.bg(palette.soft))
        .cursor_pointer()
        .border_1()
        .border_color(if focused {
            palette.focus
        } else {
            rgba(0x00000000)
        })
        .when(cfg!(not(feature = "screenshot")), |row| {
            row.hover(move |row| row.bg(palette.border))
        })
        .on_click(move |_, window, cx| {
            toggle_callback.emit(
                McpElicitationEvent::ToggleBoolean { field: index },
                window,
                cx,
            );
        })
        .child(indicator(on, false, palette))
        .child(
            div()
                .flex()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(MCP_ELICITATION_LABEL_SIZE))
                .line_height(px(MCP_ELICITATION_LABEL_LINE_HEIGHT))
                .child(field.display_title().to_owned()),
        )
}

fn render_select_control(
    field_index: usize,
    field: &McpElicitationFieldPresentation,
    options: &[McpElicitationOptionPresentation],
    multiple: bool,
    focused: bool,
    palette: McpElicitationPalette,
    callback: McpElicitationCallback,
) -> Stateful<Div> {
    let mut column = div()
        .id(element_id(
            "mcp-elicitation-select",
            &field.name,
            field_index,
        ))
        .flex()
        .flex_col()
        .gap(px(MCP_ELICITATION_OPTION_GAP));
    for (option_index, option) in options.iter().enumerate() {
        let selected = field.is_selected(option_index);
        let option_callback = callback.clone();
        column = column.child(
            div()
                .id(element_id(
                    "mcp-elicitation-option",
                    &format!("{}-{option_index}", field.name),
                    if multiple { "multi" } else { "single" },
                ))
                .role(if multiple {
                    Role::CheckBox
                } else {
                    Role::RadioButton
                })
                .aria_label(option.title.clone())
                .h(px(MCP_ELICITATION_OPTION_HEIGHT))
                .w_full()
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(15.0))
                .when(selected, |row| row.bg(palette.soft))
                .cursor_pointer()
                .when(cfg!(not(feature = "screenshot")), |row| {
                    row.hover(move |row| row.bg(palette.soft))
                })
                .on_click(move |_, window, cx| {
                    option_callback.emit(
                        if multiple {
                            McpElicitationEvent::ToggleMultiOption {
                                field: field_index,
                                option: option_index,
                            }
                        } else {
                            McpElicitationEvent::SelectOption {
                                field: field_index,
                                option: option_index,
                            }
                        },
                        window,
                        cx,
                    );
                })
                .when(selected && focused && multiple, |row| {
                    row.border_1().border_color(palette.focus)
                })
                .child(indicator(selected, multiple, palette))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(MCP_ELICITATION_LABEL_SIZE))
                        .line_height(px(MCP_ELICITATION_LABEL_LINE_HEIGHT))
                        .child(option.title.clone()),
                ),
        );
    }
    column
}

fn indicator(selected: bool, _multiple: bool, palette: McpElicitationPalette) -> Div {
    div()
        .size(px(16.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        // ChatGPT intentionally uses the same circular marker for booleans,
        // radio choices, and multi-select choices. The ARIA role carries the
        // semantic distinction; the raster affordance does not.
        .rounded(px(8.0))
        .border_1()
        .border_color(if selected {
            palette.primary
        } else {
            palette.border
        })
        .bg(if selected {
            palette.primary
        } else {
            rgba(0x00000000)
        })
        .when(selected, |indicator| {
            indicator.child(div().size(px(6.0)).rounded(px(3.0)).bg(palette.card))
        })
}

fn render_url_body(
    model: &McpElicitationPresentation,
    url: &str,
    opened: bool,
    palette: McpElicitationPalette,
    callback: McpElicitationCallback,
) -> Div {
    let open_callback = callback.clone();
    let focused = model.keyboard_focus == Some(McpElicitationFocus::OpenUrl);
    div()
        .flex()
        .flex_col()
        .gap(px(MCP_ELICITATION_FIELD_GAP))
        .child(
            div()
                .px(px(12.0))
                .py(px(10.0))
                .rounded(px(14.0))
                .bg(palette.soft)
                .text_size(px(MCP_ELICITATION_LABEL_SIZE))
                .line_height(px(MCP_ELICITATION_LABEL_LINE_HEIGHT))
                .text_color(palette.secondary)
                .overflow_hidden()
                .child(url.to_owned()),
        )
        .child(
            div()
                .id(element_id(
                    "mcp-elicitation-open-url",
                    &model.request_id,
                    "open",
                ))
                .role(Role::Button)
                .aria_label("打开链接")
                .h(px(MCP_ELICITATION_BUTTON_HEIGHT))
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(16.0))
                .bg(palette.soft)
                .cursor_pointer()
                .hover(move |button| button.bg(palette.border))
                .border_1()
                .border_color(if focused {
                    palette.focus
                } else {
                    rgba(0x00000000)
                })
                .on_click(move |_, window, cx| {
                    open_callback.emit(McpElicitationEvent::OpenUrl, window, cx);
                })
                .child(icon("approval-link-globe", palette.text.into()).size(px(14.0)))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .child(if opened {
                            "重新打开链接"
                        } else {
                            "打开链接"
                        }),
                ),
        )
        .when(opened, |body| {
            body.child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(palette.secondary)
                    .child("已打开链接。请在浏览器中完成操作后再选择“继续”。"),
            )
        })
}

fn render_footer(
    model: &McpElicitationPresentation,
    palette: McpElicitationPalette,
    callback: McpElicitationCallback,
) -> Div {
    // Reference labels: 跳过 is the protocol decline, 继续 is accept. Cancel
    // lives in the header so all three actions keep their own affordance.
    let accept_label = "继续";
    let has_errors = model.fields().iter().any(|field| field.error.is_some());
    let mut footer_palette = palette;
    if has_errors {
        // ChatGPT keeps the submit action mounted but applies its disabled
        // 80% surface while validation is visible.
        footer_palette.primary = palette.primary.alpha(0.8);
        footer_palette.secondary_button = rgba(0x00000000);
    }
    div()
        .h(px(MCP_ELICITATION_FOOTER_HEIGHT))
        .mt(px(4.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_end()
        .border_t_1()
        .border_color(palette.footer_border)
        .gap(px(8.0))
        .child(button(
            &model.request_id,
            "decline",
            "跳过",
            ButtonKind::Secondary,
            model.keyboard_focus == Some(McpElicitationFocus::Decline),
            footer_palette,
            {
                let callback = callback.clone();
                move |window, cx| callback.emit(McpElicitationEvent::Decline, window, cx)
            },
        ))
        .child(button(
            &model.request_id,
            "accept",
            accept_label,
            ButtonKind::Primary,
            model.keyboard_focus == Some(McpElicitationFocus::Accept),
            footer_palette,
            move |window, cx| callback.emit(McpElicitationEvent::Accept, window, cx),
        ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonKind {
    Secondary,
    Primary,
}

fn button(
    request_id: &str,
    suffix: &str,
    label: &str,
    kind: ButtonKind,
    focused: bool,
    palette: McpElicitationPalette,
    handler: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    let (background, foreground, width) = match kind {
        // CDP leaves the secondary action transparent until hover. Its
        // intrinsic width is 44px (26px label plus 9px insets).
        ButtonKind::Secondary => (palette.secondary_button, palette.secondary, 44.0),
        ButtonKind::Primary => (palette.primary, palette.primary_text, 72.0),
    };
    div()
        .id(element_id("mcp-elicitation-button", request_id, suffix))
        .role(Role::Button)
        .aria_label(label.to_owned())
        .h(px(MCP_ELICITATION_BUTTON_HEIGHT))
        .w(px(width))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(14.0))
        .bg(background)
        .text_color(foreground)
        .cursor_pointer()
        .when(kind == ButtonKind::Secondary, |button| {
            button.hover(move |button| button.bg(palette.border))
        })
        .when(kind == ButtonKind::Primary, |button| {
            button
                .hover(|button| button.opacity(0.9))
                .active(|button| button.opacity(0.8))
        })
        .border_1()
        .border_color(if focused {
            palette.focus
        } else {
            rgba(0x00000000)
        })
        .on_click(move |_, window, cx| handler(window, cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_owned()),
                )
                .when(kind == ButtonKind::Primary, |row| {
                    row.child(
                        div()
                            .h(px(16.0))
                            .min_w(px(16.0))
                            .px(px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .bg(palette.primary_text.alpha(0.10))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(palette.primary_text)
                            .child("⏎"),
                    )
                }),
        )
}

/// Terminal elicitation state. It stays in the conversation stream so a
/// finished, declined, cancelled, or invalidated request is never hidden
/// without an observable outcome.
pub fn render_mcp_elicitation_status(
    model: &McpElicitationPresentation,
    theme: Theme,
) -> Stateful<Div> {
    render_status_card(model, McpElicitationPalette::for_theme(theme))
}

fn render_status_card(
    model: &McpElicitationPresentation,
    palette: McpElicitationPalette,
) -> Stateful<Div> {
    let invalid = model.status == McpElicitationStatus::Invalid;
    div()
        .id(element_id(
            "mcp-elicitation-card",
            &model.request_id,
            "status",
        ))
        .role(Role::Alert)
        .aria_label(format!("{}，{}", model.message(), model.status.label()))
        .min_h(px(96.0))
        .w_full()
        .px(px(MCP_ELICITATION_CONTENT_PADDING))
        .py(px(16.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .rounded(px(MCP_ELICITATION_CARD_RADIUS))
        .bg(palette.card)
        .shadow(palette.shadows())
        .font_family("PingFang SC")
        .text_color(palette.text)
        .when(!invalid, |card| card.opacity(0.98))
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(palette.secondary)
                .child(format!("{} 请求输入", model.server_name)),
        )
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::MEDIUM)
                .child(model.message().to_owned()),
        )
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(19.0))
                .text_color(if invalid {
                    palette.error
                } else {
                    palette.secondary
                })
                .child(model.status.label()),
        )
        .when_some(model.failure_message.clone(), |card, message| {
            card.child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(palette.secondary)
                    .child(message),
            )
        })
}
