//! Searchable, single-line base-branch picker for the Review panel.
//! Keep branch identities intact: ellipsis belongs to layout, never Git data.

use super::controls::{ReviewTooltip, menu_surface};
use crate::{
    components::{
        icons::icon,
        prompt_input::{PromptChanged, PromptInput},
    },
    theme::{Theme, ThemeMode},
};
use gpui::{
    AppContext, Context, Entity, EventEmitter, Focusable, KeyDownEvent, MouseButton, Render, Role,
    ScrollHandle, Window, div, point, prelude::*, px,
};

#[cfg(test)]
mod tests;

pub(super) const WIDTH: f32 = 296.;
/// Live ChatGPT review rows are 28.5625px: 13px text on an 18.5625px line box
/// with 5px of vertical padding. GPUI lays that line box out at 18.75px, so the
/// row that can be pinned exactly is 28.5px; these constants describe what GPUI
/// draws.
const ROW_HEIGHT: f32 = 28.5;
/// The reference search row is one row tall; the section label is shorter
/// because it only carries 4px of vertical padding (26.5625px in the browser).
const SEARCH_HEIGHT: f32 = 28.5;
const LABEL_HEIGHT: f32 = 26.5;
const SECTION_GAP: f32 = 6.;
/// ChatGPT pins the whole branch section to 200px and lets it scroll, so the
/// popup keeps the same height whatever the repository holds. GPUI keeps the
/// section label fixed instead of scrolling it away, so the list itself takes
/// the remainder: 200px minus the label and its gap.
const SECTION_HEIGHT: f32 = 200.;
const LIST_HEIGHT: f32 = SECTION_HEIGHT - LABEL_HEIGHT - SECTION_GAP;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BranchPickerEvent {
    Selected(String),
    Dismissed,
}

pub(super) struct BranchPicker {
    mode: ThemeMode,
    input: Entity<PromptInput>,
    branches: Vec<String>,
    current: String,
    matches: Vec<usize>,
    selected: usize,
    keyboard_selection: bool,
    scroll: ScrollHandle,
    focus_pending: bool,
}

impl EventEmitter<BranchPickerEvent> for BranchPicker {}

impl BranchPicker {
    pub(super) fn new(mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let label = crate::i18n::format!("搜索分支" => "Search branches");
            let mut input = PromptInput::chat_search(mode, label.clone(), cx);
            input.set_accessible_name(label);
            input
        });
        cx.subscribe(&input, |picker, input, _: &PromptChanged, cx| {
            let query = input.read(cx).text().to_owned();
            picker.filter(&query);
            cx.notify();
        })
        .detach();
        Self {
            mode,
            input,
            branches: Vec::new(),
            current: String::new(),
            matches: Vec::new(),
            selected: 0,
            keyboard_selection: false,
            scroll: ScrollHandle::new(),
            focus_pending: false,
        }
    }

    pub(super) fn prepare(&mut self, branches: &[String], current: &str, cx: &mut Context<Self>) {
        self.branches = branches.to_vec();
        self.current = current.to_owned();
        // The reference puts the checked comparison base first, not HEAD.
        // Do not strip origin/ or otherwise conflate local and remote refs.
        if let Some(index) = self.branches.iter().position(|branch| branch == current) {
            let branch = self.branches.remove(index);
            self.branches.insert(0, branch);
        } else if !current.is_empty() {
            // A base such as `origin/main` is not a local branch, and the app
            // still lists it as the first selectable entry.
            self.branches.insert(0, current.to_owned());
        }
        self.input
            .update(cx, |input, cx| input.set_text_silently("", cx));
        self.filter("");
        self.focus_pending = true;
        cx.notify();
    }

    pub(super) fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.input.update(cx, |input, cx| input.set_mode(mode, cx));
        cx.notify();
    }

    fn filter(&mut self, query: &str) {
        let query = query.trim().to_lowercase();
        self.matches = self
            .branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| branch.to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
        self.keyboard_selection = false;
        self.scroll.set_offset(point(px(0.), px(0.)));
    }

    fn activate(&self, index: usize, cx: &mut Context<Self>) {
        if let Some(&index) = self.matches.get(index) {
            cx.emit(BranchPickerEvent::Selected(self.branches[index].clone()));
        }
    }

    fn key_down(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        if key == "escape" {
            cx.emit(BranchPickerEvent::Dismissed);
            cx.stop_propagation();
            return;
        }
        let modifiers = &e.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }
        let input_focused = self.input.read(cx).focus_handle(cx).is_focused(w);
        let count = self.matches.len();
        match key {
            "enter" if input_focused => self.activate(self.selected, cx),
            "down" => self.selected = (self.selected + 1) % count.max(1),
            "up" => self.selected = (self.selected + count.max(1) - 1) % count.max(1),
            // Leave Home/End to the text editor while searching.
            "home" if !input_focused => self.selected = 0,
            "end" if !input_focused => self.selected = count.saturating_sub(1),
            _ => return,
        }
        if key != "enter" {
            self.keyboard_selection = true;
            self.input.read(cx).focus_handle(cx).focus(w, cx);
            if count > 0 {
                self.scroll.scroll_to_item(self.selected);
            }
        }
        cx.notify();
        cx.stop_propagation();
    }
}

impl Render for BranchPicker {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_pending {
            self.focus_pending = false;
            self.input.read(cx).focus_handle(cx).focus(w, cx);
        }
        let t = Theme::for_mode(self.mode);
        let mut items = div()
            .id("review-branch-scroll")
            .w_full()
            .min_w(px(0.))
            .h(px(LIST_HEIGHT))
            .max_h(px(LIST_HEIGHT))
            .flex_none()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .flex()
            .flex_col();
        if self.matches.is_empty() {
            items = items.child(
                div()
                    .h(px(ROW_HEIGHT))
                    .flex_none()
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .text_size(px(13.))
                    .text_color(t.text_tertiary)
                    .child(if self.branches.is_empty() {
                        crate::i18n::format!("暂无分支" => "No branches")
                    } else {
                        crate::i18n::format!("没有匹配的分支" => "No matching branches")
                    }),
            );
        }
        for (index, &branch_index) in self.matches.iter().enumerate() {
            let branch = &self.branches[branch_index];
            let current = branch == &self.current;
            let hint = branch.clone();
            items = items.child(
                div()
                    .id(("review-branch-item", index))
                    .role(Role::MenuItem)
                    .aria_label(branch.clone())
                    .aria_selected(current)
                    .focusable()
                    .tab_stop(true)
                    .w_full()
                    .min_w(px(0.))
                    // A minimum height alone still permits wrapping and flex
                    // shrinkage. The hit target and its highlight must agree.
                    .h(px(ROW_HEIGHT))
                    .min_h(px(ROW_HEIGHT))
                    .max_h(px(ROW_HEIGHT))
                    .flex_none()
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(15.))
                    .overflow_hidden()
                    .text_size(px(13.))
                    .line_height(px(18.5714))
                    .text_color(t.text)
                    .cursor_pointer()
                    .when(self.keyboard_selection && self.selected == index, |row| {
                        row.bg(t.sidebar_hover)
                    })
                    .hover(move |row| row.bg(t.sidebar_hover))
                    .focus_visible(move |row| row.bg(t.sidebar_hover))
                    .tooltip(move |_, cx| cx.new(|_| ReviewTooltip(hint.clone().into())).into())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_move(cx.listener(move |picker, _, _, cx| {
                        if picker.keyboard_selection || picker.selected != index {
                            picker.selected = index;
                            picker.keyboard_selection = false;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |picker, _, _, cx| {
                        picker.activate(index, cx);
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(move |picker, e: &KeyDownEvent, _, cx| {
                        if e.keystroke.key == "enter" || e.keystroke.key == "space" {
                            picker.activate(index, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        icon("branch", t.text.into())
                            .size(px(16.))
                            .opacity(0.75)
                            .flex_none(),
                    )
                    .child(
                        // Only the label shrinks. Keep both icons outside its
                        // ellipsis box and retain the complete ref for actions.
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .child(branch.clone()),
                    )
                    .when(current, |row| {
                        row.child(
                            icon("check", t.text.into())
                                .size(px(16.))
                                .opacity(0.75)
                                .flex_none(),
                        )
                    }),
            );
        }
        div()
            .id("review-branch-picker")
            .debug_selector(|| "review-branch-picker".into())
            .role(Role::Menu)
            .aria_label(crate::i18n::format!("分支" => "Branches"))
            .w_full()
            .min_w(px(0.))
            // Same surface recipe as the review menus: 4px inset, 20px corner,
            // half-pixel ring, and the shared 8px/16px menu shadow.
            .p(px(4.))
            .gap(px(SECTION_GAP))
            .rounded(px(20.))
            .bg(menu_surface(t))
            .border(px(0.5))
            .border_color(t.border)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(8.0), t.profile_menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .flex()
            .flex_col()
            .occlude()
            .capture_key_down(cx.listener(Self::key_down))
            .child(
                div()
                    .h(px(SEARCH_HEIGHT))
                    .flex_none()
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        // The reference keeps a 16px leading slot but paints the
                        // 14px tertiary search glyph inside it.
                        div()
                            .w(px(16.))
                            .h(px(16.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                icon("search", t.text_tertiary.into())
                                    .size(px(14.))
                                    .flex_none(),
                            ),
                    )
                    .child(div().flex_1().min_w(px(0.)).child(self.input.clone())),
            )
            .child(
                div()
                    .h(px(LABEL_HEIGHT))
                    .flex_none()
                    .px(px(8.))
                    .py(px(4.))
                    .flex()
                    .items_center()
                    .text_size(px(13.))
                    .line_height(px(18.5714))
                    .text_color(t.text_tertiary)
                    .child(crate::i18n::format!("分支" => "Branches")),
            )
            .child(items)
    }
}
