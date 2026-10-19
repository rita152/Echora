//! Picker behavior and presentation for the prompt composer.
//!
//! The picker follows ChatGPT 26.1007's model picker: a trigger that reads
//! "<model> <effort>", and a 254px popup centred above it with two views. The
//! slider view shows the selection above a power slider; the model view lists
//! "Default" and the catalog's models.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, BoxShadow, Context, Div, KeyDownEvent, MouseButton, SharedString,
    Window, div, hsla, linear_color_stop, linear_gradient, prelude::*, px, rgba,
};

use super::{
    ComposerView, MODEL_PICKER_MENU_GAP, MODEL_PICKER_WIDTH, PARTICLE_TIMELINE_MS, TrayTooltip,
    layout::{max_particle_drift, particle_layers},
};
use crate::{
    agent::{AgentModel, AgentModelCatalog},
    components::icons::icon,
    theme::{Theme, ThemeMode, ui_font},
};

/// `--color-chart-blue`: the slider range, accented effort and Fast toggle.
/// ChatGPT uses the same value in both themes.
const CHART_BLUE: u32 = 0x339cffff;
/// Menu rows are `text-sm` (13/18.57) with 5px block padding.
const ROW_LINE_HEIGHT: f32 = 18.5714;
const ROW_HEIGHT: f32 = 5.0 + ROW_LINE_HEIGHT + 5.0;
/// The "Default" row adds a 16px description line.
const DEFAULT_ROW_HEIGHT: f32 = ROW_HEIGHT + 16.0;
/// The model list keeps the popup inside the window like Radix's collision
/// handling does for long catalogs.
const MODEL_LIST_MAX_HEIGHT: f32 = 420.0;

/// One focusable control of the slider view, in keyboard order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PickerControl {
    ViewToggle,
    Reset,
    Fast,
    Slider,
}

/// ChatGPT's display-name formatting for the picker (`stripGptPrefix`):
/// "GPT-6.1-Sol" reads "6.1 Sol" and "GPT-5.5" reads "5.5"; names that do not
/// start with "gpt" are shown unchanged.
pub(super) fn short_model_label(name: &str) -> String {
    let trimmed = name.trim_start();
    if !trimmed.to_lowercase().starts_with("gpt") {
        return name.to_owned();
    }
    let lower = trimmed.to_lowercase();
    let versioned = lower.starts_with("gpt-")
        && lower[4..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit());
    let separator = if versioned { " " } else { "-" };
    let mut formatted = String::new();
    let mut rest = name;
    while !rest.is_empty() {
        let split = rest
            .find(|character: char| {
                character.is_whitespace() != rest.starts_with(char::is_whitespace)
            })
            .unwrap_or(rest.len());
        let (piece, tail) = rest.split_at(split);
        rest = tail;
        if piece.trim().is_empty() {
            formatted.push_str(piece);
            continue;
        }
        let joined = piece
            .split('-')
            .enumerate()
            .map(|(index, word)| {
                if word.eq_ignore_ascii_case("gpt") {
                    "GPT".to_owned()
                } else if word.eq_ignore_ascii_case("oai") {
                    "OAI".to_owned()
                } else if index > 0 && !word.is_empty() {
                    let mut characters = word.chars();
                    characters
                        .next()
                        .map(|first| first.to_uppercase().chain(characters).collect())
                        .unwrap_or_default()
                } else {
                    word.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(separator);
        match joined.strip_prefix("GPT ") {
            Some(tail) if tail.starts_with(|character: char| character.is_ascii_digit()) => {
                formatted.push_str("GPT-");
                formatted.push_str(tail);
            }
            _ => formatted.push_str(&joined),
        }
    }
    match formatted.get(..4) {
        Some(prefix) if prefix.eq_ignore_ascii_case("gpt-") => formatted[4..].to_owned(),
        _ => formatted,
    }
}

impl ComposerView {
    pub(super) fn apply_model_catalog(&mut self, catalog: AgentModelCatalog) {
        self.conversation.apply_model_catalog(catalog);
        if let Some(config) = &self.permission_config {
            self.conversation.apply_config_defaults(config);
        }
    }
    pub(super) fn apply_model_selection(
        &mut self,
        index: usize,
        preferred_effort: Option<String>,
        preferred_service_tier: Option<String>,
        preserve_standard_tier: bool,
    ) {
        self.conversation.model_user_selected = true;
        self.conversation.apply_model_selection(
            index,
            preferred_effort,
            preferred_service_tier,
            preserve_standard_tier,
        )
    }
    /// Choosing a model in the list makes it the explicit selection and, as
    /// in ChatGPT, keeps the current effort and speed when the model offers
    /// them.
    pub(super) fn select_model_at(&mut self, index: usize) {
        self.model_ultra_warning_until = None;
        let effort = self.request_effort();
        let service_tier = self.conversation.selected_service_tier.clone();
        self.apply_model_selection(index, Some(effort), service_tier, true);
        self.conversation.model_explicit = true;
    }
    /// "Default" and "Reset to default" return to the catalog default model
    /// with its advertised effort and speed.
    pub(super) fn select_default_model(&mut self) {
        self.model_ultra_warning_until = None;
        if let Some(index) = self.default_model_index() {
            self.apply_model_selection(index, None, None, false);
        }
        self.conversation.model_explicit = false;
    }
    pub(super) fn select_effort_at(&mut self, index: usize) {
        let Some(effort) = self.conversation.effort_ids().into_iter().nth(index) else {
            return;
        };
        self.conversation.model_user_selected = true;
        self.conversation.selected_effort = effort;
        self.conversation.slider_index = index;
        self.conversation.actual_model = None;
        self.conversation.model_status = None;
        self.conversation.safety_buffering = false;
    }
    pub(super) fn selected_model_entry(&self) -> Option<&AgentModel> {
        self.conversation.selected_model_entry()
    }
    pub(super) fn model_display_name<'a>(&'a self, model_name: &'a str) -> &'a str {
        self.conversation.model_display_name(model_name)
    }
    /// The trigger's model label: the formatted catalog name, ChatGPT's
    /// "Custom" for a configured model the catalog does not list, or the
    /// unavailable state when there is no model at all.
    pub(super) fn selected_model_label(&self) -> String {
        if self.conversation.selected_model.is_empty() {
            crate::i18n::text("模型不可用").to_owned()
        } else if let Some(model) = self.selected_model_entry() {
            short_model_label(&model.display_name)
        } else {
            crate::i18n::text("自定义").to_owned()
        }
    }
    pub(super) fn effective_model_label(&self) -> String {
        match self.conversation.actual_model.as_deref() {
            Some(model) if model != self.conversation.selected_model => {
                short_model_label(self.model_display_name(model))
            }
            _ => self.selected_model_label(),
        }
    }
    pub(super) fn effort_label(effort: &str) -> &str {
        match effort {
            "none" => crate::i18n::text("无"),
            "minimal" => crate::i18n::text("极低"),
            "low" => crate::i18n::text("轻度"),
            "medium" => crate::i18n::text("中"),
            "high" => crate::i18n::text("高"),
            "xhigh" => crate::i18n::text("极高"),
            "max" => "Max",
            "ultra" => "Ultra",
            other => other,
        }
    }
    pub(super) fn selected_effort_label(&self) -> String {
        let effort = self.request_effort();
        if effort.is_empty() {
            "—".to_owned()
        } else {
            Self::effort_label(&effort).to_owned()
        }
    }
    pub(super) fn default_model_index(&self) -> Option<usize> {
        self.conversation
            .models
            .iter()
            .position(|model| model.is_default)
            .or((!self.conversation.models.is_empty()).then_some(0))
    }
    pub(super) fn reset_model_selection(&mut self) {
        self.select_default_model();
        self.model_list_open = false;
        self.slider_dragging = false;
    }
    pub(super) fn toggle_accelerated_service_tier(&mut self) {
        let effort = self.request_effort();
        if self.conversation.selected_service_tier.is_some() {
            self.conversation.selected_service_tier = None;
        } else {
            self.conversation.selected_service_tier = self
                .selected_model_entry()
                .and_then(|model| model.service_tiers.first())
                .map(|tier| tier.id.clone());
        }
        self.conversation.selected_effort = effort;
        self.conversation.sync_slider_index();
        self.conversation.model_user_selected = true;
        self.conversation.actual_model = None;
        self.conversation.model_status = None;
        self.conversation.safety_buffering = false;
    }
    /// ChatGPT offers Fast only for a model with a faster service tier served
    /// by the OpenAI provider; a custom provider never shows the toggle.
    pub(super) fn fast_mode_available(&self) -> bool {
        let openai_provider = self
            .permission_config
            .as_ref()
            .and_then(|config| config.effective.get("model_provider"))
            .and_then(serde_json::Value::as_str)
            .is_none_or(|provider| provider == "openai");
        openai_provider
            && self
                .selected_model_entry()
                .is_some_and(|model| !model.service_tiers.is_empty())
    }
    /// With fewer than two power steps there is nothing to slide, so the
    /// popup opens on the model list, as ChatGPT does.
    pub(super) fn picker_shows_model_list(&self) -> bool {
        self.model_list_open || self.conversation.effort_ids().len() < 2
    }
    pub(super) fn picker_controls(&self) -> Vec<PickerControl> {
        let mut controls = vec![PickerControl::ViewToggle];
        if self.conversation.model_explicit {
            controls.push(PickerControl::Reset);
        }
        if self.fast_mode_available() {
            controls.push(PickerControl::Fast);
        }
        controls.push(PickerControl::Slider);
        controls
    }
    fn focused_picker_control(&self) -> Option<PickerControl> {
        if !self.model_menu_keyboard_focus || self.picker_shows_model_list() {
            return None;
        }
        self.picker_controls()
            .get(self.model_menu_focused_item)
            .copied()
    }
    /// Rows of the model view: "Default" followed by the catalog.
    fn model_list_len(&self) -> usize {
        1 + self.conversation.models.len()
    }
    fn picker_item_count(&self) -> usize {
        if self.picker_shows_model_list() {
            self.model_list_len()
        } else {
            self.picker_controls().len()
        }
    }
    fn show_model_list(&mut self, open: bool) {
        self.model_ultra_warning_until = None;
        self.model_list_open = open;
        self.slider_dragging = false;
        self.model_menu_focused_item = 0;
    }
    fn finish_model_choice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_model_list(false);
        if self.conversation.effort_ids().len() < 2 {
            self.close_picker(cx);
            self.model_trigger_focus.focus(window, cx);
        } else {
            self.model_menu_focus.focus(window, cx);
        }
    }
    pub(super) fn step_slider(&mut self, delta: isize) {
        let count = self.conversation.effort_ids().len();
        if count == 0 {
            return;
        }
        let index = self.effective_slider_index() as isize + delta;
        self.set_slider_index(index.clamp(0, count as isize - 1) as usize);
    }
    pub(super) fn effective_slider_index(&self) -> usize {
        let effort = self.request_effort();
        self.conversation
            .effort_ids()
            .iter()
            .position(|option| *option == effort)
            .unwrap_or(0)
    }
    pub(super) fn handle_model_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.menu_open {
            return;
        }
        let key = event.keystroke.key.as_str();
        let count = self.picker_item_count().max(1);
        match key {
            "down" => {
                self.model_menu_focused_item = if self.model_menu_keyboard_focus {
                    (self.model_menu_focused_item + 1) % count
                } else {
                    0
                };
                self.model_menu_keyboard_focus = true;
            }
            "up" => {
                self.model_menu_focused_item = if self.model_menu_keyboard_focus {
                    (self.model_menu_focused_item + count - 1) % count
                } else {
                    count - 1
                };
                self.model_menu_keyboard_focus = true;
            }
            "home" => {
                self.model_menu_focused_item = 0;
                self.model_menu_keyboard_focus = true;
            }
            "end" => {
                self.model_menu_focused_item = count - 1;
                self.model_menu_keyboard_focus = true;
            }
            "left" | "right" if !self.picker_shows_model_list() => {
                self.step_slider(if key == "left" { -1 } else { 1 });
            }
            "left" if self.model_list_open && self.conversation.effort_ids().len() >= 2 => {
                self.show_model_list(false);
            }
            "enter" | "space" if self.model_menu_keyboard_focus => {
                if self.picker_shows_model_list() {
                    match self.model_menu_focused_item {
                        0 => self.select_default_model(),
                        row => self.select_model_at(row - 1),
                    }
                    self.finish_model_choice(window, cx);
                    self.model_menu_keyboard_focus = true;
                } else {
                    match self.focused_picker_control() {
                        Some(PickerControl::ViewToggle) => {
                            self.show_model_list(true);
                            self.model_menu_keyboard_focus = true;
                        }
                        Some(PickerControl::Reset) => {
                            self.reset_model_selection();
                            self.model_menu_focused_item = 0;
                        }
                        Some(PickerControl::Fast) => self.toggle_accelerated_service_tier(),
                        Some(PickerControl::Slider) | None => {}
                    }
                }
            }
            "escape" | "tab" => {
                self.close_picker(cx);
                self.model_trigger_focus.focus(window, cx);
            }
            _ => return,
        }
        if self.picker_shows_model_list() {
            self.model_list_scroll
                .scroll_to_item(self.model_menu_focused_item + 1);
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub fn close_picker(&mut self, cx: &mut Context<Self>) {
        if self.context_menu_open {
            self.context_menu_open = false;
            self.context_catalog.cancel();
            self.focus_prompt_pending = true;
            cx.notify();
        }
        if self.menu_open {
            self.model_ultra_warning_until = None;
            self.menu_open = false;
            self.model_list_open = false;
            self.slider_dragging = false;
            cx.notify();
        }
        if self.permission_menu_open {
            self.permission_menu_open = false;
            self.permission_menu_keyboard_focus = false;
            cx.notify();
        }
    }
    pub fn open_picker(&mut self, cx: &mut Context<Self>) {
        self.menu_open = true;
        self.model_list_open = false;
        self.model_menu_keyboard_focus = false;
        cx.notify();
    }
    /// Capture entry point: `simple` opens the slider view and `model` (or
    /// `list`) the model list.
    pub fn open_picker_submenu(&mut self, name: &str, cx: &mut Context<Self>) {
        self.menu_open = true;
        self.model_list_open = matches!(name, "model" | "list" | "models");
        cx.notify();
    }
    pub fn open_picker_slider_at(&mut self, index: usize, fast: bool, cx: &mut Context<Self>) {
        self.menu_open = true;
        self.model_list_open = false;
        self.conversation.selected_service_tier = if fast {
            self.selected_model_entry()
                .and_then(|model| model.service_tiers.first())
                .map(|tier| tier.id.clone())
        } else {
            None
        };
        self.set_slider_index(index);
        cx.notify();
    }
    pub(super) fn set_slider_index(&mut self, index: usize) {
        let effort_count = self.conversation.effort_ids().len();
        if effort_count == 0 {
            self.conversation.slider_index = 0;
            return;
        }
        let previous_effort = self.request_effort();
        self.select_effort_at(index.min(effort_count - 1));
        if self.conversation.selected_effort != "ultra" {
            self.model_ultra_warning_until = None;
        } else if previous_effort != "ultra" && self.conversation.model_explicit {
            // The reference briefly replaces the model subtitle on reaching
            // Ultra, then restores it after two seconds.
            self.model_ultra_warning_until =
                Some(std::time::Instant::now() + Duration::from_secs(2));
        }
    }

    pub(super) fn model_ultra_warning_visible(&self) -> bool {
        self.conversation.model_explicit
            && self.conversation.selected_effort == "ultra"
            && self
                .model_ultra_warning_until
                .is_some_and(|until| until > std::time::Instant::now())
    }

    pub(super) fn power_slider(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        // CDP (26.1007): a 230px track whose first and last ticks sit 13px
        // inside its ends.
        const TRACK_WIDTH: f32 = 230.0;
        const TRACK_INSET: f32 = 13.0;
        let effort_count = self.conversation.effort_ids().len().max(1);
        let step = if effort_count > 1 {
            (TRACK_WIDTH - TRACK_INSET * 2.0) / (effort_count - 1) as f32
        } else {
            0.0
        };
        let step_center = |index: usize| {
            if effort_count > 1 {
                TRACK_INSET + step * index as f32
            } else {
                TRACK_WIDTH * 0.5
            }
        };
        let slider_index = self.effective_slider_index().min(effort_count - 1);
        let thumb_center = step_center(slider_index);
        let ultra_mode = self.request_effort() == "ultra";
        let thumb_size = if self.slider_dragging { 32.0 } else { 28.0 };
        let mut range = div()
            .absolute()
            .left_0()
            .top_0()
            .h_full()
            .w(px(thumb_center))
            .rounded(px(12.0))
            .overflow_hidden()
            .bg(rgba(CHART_BLUE));

        if ultra_mode {
            // Keep the first radius of the rounded range as solid blue. Starting the
            // rectangular gradient at the circle tangent prevents its square corners
            // from leaking through the rounded left cap.
            const RANGE_RADIUS: f32 = 12.0;
            range = range.child(
                div()
                    .absolute()
                    .left(px(RANGE_RADIUS))
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .child(div().h_full().w(px(TRACK_WIDTH * 0.55 - RANGE_RADIUS)).bg(
                        linear_gradient(
                            90.0,
                            linear_color_stop(rgba(CHART_BLUE), 0.0),
                            linear_color_stop(rgba(0xad7bf9ff), 1.0),
                        ),
                    ))
                    .child(div().h_full().flex_1().bg(linear_gradient(
                        90.0,
                        linear_color_stop(rgba(0xad7bf9ff), 0.0),
                        linear_color_stop(rgba(0x8b84fbff), 1.0),
                    ))),
            );
        }

        let (show_max_particles, show_fast_particles) = particle_layers(
            ultra_mode,
            self.conversation.selected_service_tier.is_some(),
        );
        if show_max_particles || show_fast_particles {
            const MAX_PARTICLES: [(f32, f32, f32, f32, f32, u64); 14] = [
                (0.50, 3.0, 17.0, 0.616, 0.405, 3102),
                (0.87, 3.0, 17.0, 0.740, 0.528, 2843),
                (0.72, 0.0, 17.0, 0.946, 0.837, 1352),
                (0.60, -3.0, 8.0, 0.595, 0.898, 1607),
                (0.48, -4.0, 15.0, 0.627, 0.993, 1738),
                (0.56, -1.0, 11.0, 0.617, 0.718, 1967),
                (0.18, -3.0, 12.0, 0.522, 0.788, 2066),
                (0.42, -2.0, 11.0, 0.682, 0.992, 1660),
                (0.29, -1.0, 12.0, 0.580, 0.755, 2481),
                (0.49, 0.0, 13.0, 0.564, 0.701, 1577),
                (0.90, -3.0, 12.0, 0.852, 0.614, 2675),
                (0.08, -3.0, 5.0, 0.523, 0.892, 2156),
                (0.13, -3.0, 14.0, 0.759, 0.773, 1553),
                (0.08, 2.0, 14.0, 0.563, 0.880, 1863),
            ];
            const FAST_PARTICLES: [(f32, f32, f32, u64, f32); 14] = [
                (8.94, 0.616, 0.405, 1701, 0.20),
                (16.65, 0.740, 0.528, 1708, 0.12),
                (20.50, 0.946, 0.837, 2259, 0.08),
                (7.68, 0.595, 0.898, 1928, 0.99),
                (10.22, 0.627, 0.993, 1843, 0.91),
                (19.24, 0.617, 0.718, 1629, 0.78),
                (6.07, 0.522, 0.788, 1963, 0.68),
                (15.18, 0.682, 0.992, 1851, 0.57),
                (10.92, 0.580, 0.755, 1745, 0.46),
                (3.44, 0.564, 0.701, 2014, 0.39),
                (18.27, 0.852, 0.614, 1843, 0.31),
                (13.25, 0.523, 0.892, 2292, 0.24),
                (20.65, 0.759, 0.773, 1982, 0.16),
                (14.98, 0.563, 0.880, 1809, 0.08),
            ];

            // One phase-locked clock drives the complete particle layer. Each particle
            // moves at canvas paint time, so animation frames neither fan out into 14/28
            // independent timers nor invalidate their layout positions.
            range = range.child(div().absolute().inset_0().with_animation(
                "model-slider-particles",
                Animation::new(Duration::from_secs(120)).repeat_synced(),
                move |layer, progress| {
                    layer.child(
                        gpui::canvas(
                            |bounds, _, _| bounds,
                            move |bounds, _, window, _| {
                                let mut paint_particle =
                                    |x: f32, y: f32, diameter: f32, opacity: f32| {
                                        let particle_bounds = gpui::Bounds {
                                            origin: gpui::point(
                                                bounds.origin.x + px(x),
                                                bounds.origin.y + px(y),
                                            ),
                                            size: gpui::size(px(diameter), px(diameter)),
                                        };
                                        let radius = px(diameter / 2.0);
                                        window.paint_drop_shadows(
                                            particle_bounds,
                                            radius.into(),
                                            &[BoxShadow::new(
                                                px(0.0),
                                                px(0.0),
                                                hsla(0.0, 0.0, 1.0, 0.34 * opacity),
                                            )
                                            .blur_radius(px(5.0))],
                                        );
                                        window.paint_quad(gpui::quad(
                                            particle_bounds,
                                            radius,
                                            hsla(0.0, 0.0, 1.0, 0.72 * opacity),
                                            px(0.0),
                                            hsla(0.0, 0.0, 1.0, 0.0),
                                            Default::default(),
                                        ));
                                    };

                                if show_max_particles {
                                    for (index, (position, offset, y, scale, opacity, duration)) in
                                        MAX_PARTICLES.into_iter().enumerate()
                                    {
                                        let (drift_x, drift_y) =
                                            max_particle_drift(progress, index, duration);
                                        paint_particle(
                                            position * TRACK_WIDTH + offset + drift_x,
                                            (y + drift_y).clamp(4.0, 20.0),
                                            3.0 * scale,
                                            opacity,
                                        );
                                    }
                                }

                                if show_fast_particles {
                                    for (y, scale, base_opacity, duration, phase) in FAST_PARTICLES
                                    {
                                        let loops =
                                            (PARTICLE_TIMELINE_MS / duration as f32).round();
                                        let travel = (progress * loops + phase).fract();
                                        let opacity = if travel < 0.08 {
                                            travel / 0.08
                                        } else if travel > 0.92 {
                                            (1.0 - travel) / 0.08
                                        } else {
                                            1.0
                                        };
                                        paint_particle(
                                            (1.0 - travel) * thumb_center,
                                            y,
                                            3.0 * scale,
                                            opacity * base_opacity,
                                        );
                                    }
                                }
                            },
                        )
                        .absolute()
                        .inset_0()
                        .size_full(),
                    )
                },
            ));
        }

        let mut track = div()
            .absolute()
            .left_0()
            .top(px(2.0))
            .w(px(TRACK_WIDTH))
            .h(px(24.0))
            .rounded(px(12.0))
            .overflow_hidden()
            .bg(theme.text.alpha(0.10))
            .border(px(0.5))
            .border_color(theme.border)
            .child(range);

        // Ticks inside the range are white at 30%; the rest use half of the
        // description color, which differs between the themes.
        let mut unselected_tick: gpui::Hsla = theme.text_tertiary.into();
        unselected_tick.a *= 0.5;
        for index in 0..effort_count {
            let center = step_center(index);
            let selected = index <= slider_index;
            let hidden =
                ultra_mode || (self.conversation.selected_service_tier.is_some() && selected);
            track = track.child(
                div()
                    .absolute()
                    .left(px(center - if hidden { 1.5 } else { 2.0 }))
                    .top(px(if hidden { 10.5 } else { 10.0 }))
                    .size(px(if hidden { 3.0 } else { 4.0 }))
                    .rounded_full()
                    .bg(if selected {
                        rgba(0xffffff4d).into()
                    } else {
                        unselected_tick
                    })
                    .opacity(if hidden { 0.0 } else { 1.0 }),
            );
        }

        let mut hit_areas = div().absolute().inset_0();
        for index in 0..effort_count {
            let center = step_center(index);
            let left = if index == 0 { 0.0 } else { center - step * 0.5 };
            let right = if index + 1 == effort_count {
                TRACK_WIDTH
            } else {
                center + step * 0.5
            };
            hit_areas = hit_areas.child(
                div()
                    .id(("model-power-slider-step", index))
                    .absolute()
                    .left(px(left))
                    .top_0()
                    .w(px(right - left))
                    .h_full()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.slider_dragging = true;
                            this.set_slider_index(index);
                            cx.notify();
                        }),
                    )
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered
                            && this.slider_dragging
                            && this.conversation.slider_index != index
                        {
                            this.set_slider_index(index);
                            cx.notify();
                        }
                    })),
            );
        }

        div()
            .id("model-power-slider")
            .role(gpui::Role::Slider)
            .aria_label(crate::i18n::format!("推理强度" => "Power"))
            .aria_value(self.selected_effort_label())
            .aria_numeric_value(slider_index as f64)
            .aria_min_numeric_value(0.)
            .aria_max_numeric_value(effort_count.saturating_sub(1) as f64)
            .aria_numeric_value_step(1.)
            .on_a11y_action(gpui::AccessibleAction::Increment, {
                let view = cx.entity();
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.step_slider(1);
                        cx.notify();
                    })
                }
            })
            .on_a11y_action(gpui::AccessibleAction::Decrement, {
                let view = cx.entity();
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.step_slider(-1);
                        cx.notify();
                    })
                }
            })
            .relative()
            .h(px(32.0))
            .mx(px(2.0))
            .px(px(6.0))
            .py(px(2.0))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.slider_dragging = false;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.slider_dragging = false;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .relative()
                    .w(px(TRACK_WIDTH))
                    .h(px(28.0))
                    .child(track)
                    .child(
                        div()
                            .absolute()
                            .left(px(thumb_center - thumb_size * 0.5))
                            .top(px((28.0 - thumb_size) * 0.5))
                            .size(px(thumb_size))
                            .rounded_full()
                            .bg(rgba(0xffffffff))
                            .border(px(0.5))
                            // `--color-border-strong`.
                            .border_color(if self.mode == ThemeMode::Light {
                                rgba(0x1a1c1f1e)
                            } else {
                                rgba(0xffffff28)
                            })
                            .shadow(vec![
                                BoxShadow::new(px(0.0), px(0.0), hsla(0.0, 0.0, 0.0, 0.10))
                                    .blur_radius(px(2.0)),
                            ]),
                    )
                    .child(hit_areas),
            )
    }

    fn picker_view_toggle(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let explicit = self.conversation.model_explicit;
        let ultra_warning = self.model_ultra_warning_visible();
        let model = if ultra_warning {
            crate::i18n::text("更快消耗使用额度").to_owned()
        } else {
            self.selected_model_label()
        };
        let effort = self.selected_effort_label();
        let focused = self.focused_picker_control() == Some(PickerControl::ViewToggle);
        let subtitle = div()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(crate::typography::ui_size(if explicit {
                12.0
            } else {
                14.0
            })))
            .line_height(px(if explicit { 16.0 } else { 20.0 }))
            .text_color(if explicit {
                theme.text_secondary
            } else {
                theme.text
            })
            // Equal side columns center the model below the effort; the
            // transient Ultra warning spans the entire subtitle instead.
            .when(explicit && !ultra_warning, |d| {
                d.child(div().w(px(16.0)).flex_none())
            })
            .child(div().min_w(px(0.0)).truncate().child(model))
            .when(!explicit, |d| {
                d.child(div().text_color(theme.text_tertiary).child(effort.clone()))
            })
            .when(!ultra_warning, |d| {
                d.child(div().w(px(16.0)).flex_none().flex().items_center().child(
                    icon("slash-submenu-chevron", theme.text_tertiary.into()).size(px(12.0)),
                ))
            });
        let content = div()
            .min_w(px(0.0))
            .px(px(4.0))
            .py(px(2.0))
            .flex()
            .when(explicit, |d| d.flex_col())
            .items_center()
            .justify_center()
            .when(explicit, |d| {
                d.child(
                    div()
                        .text_size(px(crate::typography::ui_size(14.0)))
                        .line_height(px(crate::typography::ui_leading(14.0, 20.0)))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(if self.request_effort() == "ultra" {
                            rgba(0xad7bf9ff)
                        } else {
                            rgba(CHART_BLUE)
                        })
                        .child(effort),
                )
            })
            .child(subtitle);
        div()
            .id("model-picker-view-toggle")
            .debug_selector(|| "MODEL_PICKER_VIEW_TOGGLE".to_owned())
            .role(gpui::Role::MenuItem)
            .aria_label(crate::i18n::text("选择模型"))
            .min_w(px(0.0))
            .max_w_full()
            .rounded(px(8.0))
            .p(px(4.0))
            .cursor_pointer()
            .when(focused, |d| d.bg(theme.sidebar_hover))
            .hover(move |d| d.bg(theme.sidebar_hover))
            .on_click(cx.listener(|this, _, window, cx| {
                this.show_model_list(true);
                this.model_menu_keyboard_focus = false;
                this.model_menu_focus.focus(window, cx);
                cx.stop_propagation();
                cx.notify();
            }))
            .child(content)
    }

    fn slider_view(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let explicit = self.conversation.model_explicit;
        let ultra_warning = self.model_ultra_warning_visible();
        let fast = self.fast_mode_available();
        let mut controls = div()
            .relative()
            .h(px(if explicit { 48.0 } else { 36.0 }))
            .px(px(if explicit && !ultra_warning {
                32.0
            } else if fast || ultra_warning {
                16.0
            } else {
                8.0
            }))
            .flex()
            .items_center()
            .justify_center()
            .when(!explicit, |d| d.child(div().w(px(16.0)).flex_none()))
            .child(self.picker_view_toggle(theme, cx));
        if explicit {
            let focused = self.focused_picker_control() == Some(PickerControl::Reset);
            controls = controls.child(
                div()
                    .id("model-picker-reset")
                    .role(gpui::Role::MenuItem)
                    .aria_label(crate::i18n::text("重置为默认模型"))
                    .absolute()
                    .right_0()
                    .top_0()
                    .size(px(32.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(focused, |d| d.bg(theme.sidebar_hover))
                    .hover(move |d| d.bg(theme.sidebar_hover))
                    .tooltip(|_, cx| {
                        cx.new(|_| TrayTooltip(crate::i18n::text("重置为默认模型").into()))
                            .into()
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.reset_model_selection();
                        this.model_menu_focus.focus(window, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(icon("model-reset", theme.text_tertiary.into()).size(px(16.0))),
            );
        }
        if fast {
            let focused = self.focused_picker_control() == Some(PickerControl::Fast);
            controls = controls.child(
                div()
                    .id("model-picker-fast-toggle")
                    .role(gpui::Role::MenuItem)
                    .aria_label(crate::i18n::text("快速模式"))
                    .aria_selected(self.conversation.selected_service_tier.is_some())
                    .absolute()
                    .left_0()
                    .top_0()
                    .size(px(32.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(focused, |d| d.bg(theme.sidebar_hover))
                    .hover(move |d| d.bg(theme.sidebar_hover))
                    .tooltip(|_, cx| {
                        cx.new(|_| TrayTooltip(crate::i18n::text("快速模式").into()))
                            .into()
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_accelerated_service_tier();
                        this.model_menu_focus.focus(window, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(
                        icon(
                            "model-fast",
                            if self.conversation.selected_service_tier.is_some() {
                                if self.request_effort() == "ultra" {
                                    rgba(0xad7bf9ff).into()
                                } else {
                                    rgba(CHART_BLUE).into()
                                }
                            } else {
                                theme.text_tertiary.into()
                            },
                        )
                        .size(px(16.0)),
                    ),
            );
        }
        let focused = self.focused_picker_control() == Some(PickerControl::Slider);
        div().p(px(4.0)).flex().flex_col().child(controls)
            .child(div().id("model-picker-power-control").py(px(4.0)).role(gpui::Role::MenuItem)
                .aria_label(crate::i18n::format!("推理强度：{effort}" => "Power: {effort}", effort = self.selected_effort_label()))
                .when(focused, |d| d.bg(theme.sidebar_hover).rounded(px(8.0)))
                .child(self.power_slider(theme, cx)))
    }

    fn model_list(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let row = |index: usize, title: String, detail: Option<SharedString>, selected: bool| {
            div()
                .id(("model-picker-model", index))
                .debug_selector(move || format!("MODEL_PICKER_MODEL_{index}"))
                .role(gpui::Role::MenuItem)
                .aria_label(title.clone())
                .aria_selected(selected)
                .min_h(px(if detail.is_some() {
                    DEFAULT_ROW_HEIGHT
                } else {
                    ROW_HEIGHT
                }))
                .px(px(8.0))
                .py(px(5.0))
                .rounded(px(15.0))
                .flex()
                .items_center()
                .cursor_pointer()
                .when(
                    self.model_menu_keyboard_focus && self.model_menu_focused_item == index,
                    |d| d.bg(theme.sidebar_hover).aria_active_descendant(),
                )
                .hover(move |d| d.bg(theme.sidebar_hover))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if index == 0 {
                        this.select_default_model();
                    } else {
                        this.select_model_at(index - 1);
                    }
                    this.finish_model_choice(window, cx);
                    this.model_menu_keyboard_focus = false;
                    cx.stop_propagation();
                    cx.notify();
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(crate::typography::ui_size(13.0)))
                                .line_height(px(ROW_LINE_HEIGHT))
                                .child(title),
                        )
                        .children(detail.map(|detail| {
                            div()
                                .text_size(px(crate::typography::ui_size(12.0)))
                                .line_height(px(16.0))
                                .text_color(theme.text_tertiary)
                                .child(detail)
                        })),
                )
                .when(selected, |d| {
                    d.child(
                        icon("permission-check", theme.text_secondary.into())
                            .size(px(16.0))
                            .ml(px(6.0)),
                    )
                })
        };
        div()
            .id("model-picker-model-list")
            .p(px(4.0))
            .max_h(px(MODEL_LIST_MAX_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&self.model_list_scroll)
            .child(
                div()
                    .px(px(8.0))
                    .py(px(5.0))
                    .text_size(px(crate::typography::ui_size(13.0)))
                    .line_height(px(ROW_LINE_HEIGHT))
                    .text_color(theme.text_tertiary)
                    .child(crate::i18n::text("选择模型")),
            )
            .when(!self.conversation.models.is_empty(), |menu| {
                menu.child(row(
                    0,
                    crate::i18n::text("默认").to_owned(),
                    Some(crate::i18n::text("推荐模型组合").into()),
                    !self.conversation.model_explicit,
                ))
            })
            .when(self.conversation.models.is_empty(), |menu| {
                menu.child(
                    div()
                        .px(px(8.0))
                        .py(px(5.0))
                        .text_size(px(crate::typography::ui_size(13.0)))
                        .line_height(px(ROW_LINE_HEIGHT))
                        .text_color(theme.text_tertiary)
                        .child(
                            self.conversation
                                .model_catalog_error
                                .clone()
                                .unwrap_or_else(|| crate::i18n::text("没有可用模型").to_owned()),
                        ),
                )
            })
            .children(
                self.conversation
                    .models
                    .iter()
                    .enumerate()
                    .map(|(index, model)| {
                        row(
                            index + 1,
                            short_model_label(&model.display_name),
                            None,
                            self.conversation.model_explicit
                                && (self.conversation.selected_model == model.model
                                    || self.conversation.selected_model == model.id),
                        )
                        .tooltip({
                            let description = model.description.clone();
                            move |_, cx| cx.new(|_| TrayTooltip(description.clone().into())).into()
                        })
                    }),
            )
    }

    pub(super) fn model_menu(
        &self,
        _viewport_width: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let menu = div()
            .id("model-picker-menu")
            .debug_selector(|| "MODEL_PICKER_MENU".to_owned())
            .role(gpui::Role::Menu)
            .key_context("ComposerModelPicker")
            .track_focus(&self.model_menu_focus)
            .on_action(
                cx.listener(|this, _: &super::DismissModelPicker, window, cx| {
                    this.close_picker(cx);
                    this.model_trigger_focus.focus(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_key_down(cx.listener(Self::handle_model_menu_key))
            .w(px(MODEL_PICKER_WIDTH))
            .rounded(px(20.0))
            .bg(theme.model_picker_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(0.0), theme.border.into()).spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.12))
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font(ui_font())
            .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
            .text_color(theme.text)
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                if event.button == MouseButton::Left
                    && !this
                        .model_trigger_bounds
                        .get()
                        .is_some_and(|bounds| bounds.contains(&event.position))
                {
                    this.close_picker(cx);
                }
            }))
            .child(if self.picker_shows_model_list() {
                self.model_list(theme, cx).into_any_element()
            } else {
                self.slider_view(theme, cx).into_any_element()
            });
        if let Some(trigger) = self.model_trigger_bounds.get() {
            gpui::anchored()
                .anchor(gpui::Anchor::BottomLeft)
                .position(gpui::point(
                    trigger.center().x - px(MODEL_PICKER_WIDTH / 2.),
                    trigger.top() - px(MODEL_PICKER_MENU_GAP),
                ))
                .snap_to_window_with_margin(px(8.))
                .child(menu)
                .into_any_element()
        } else {
            menu.absolute()
                .right(px(super::MODEL_PICKER_RIGHT_INSET))
                .bottom(px(8.0 + 28.0 + MODEL_PICKER_MENU_GAP))
                .into_any_element()
        }
    }
}
