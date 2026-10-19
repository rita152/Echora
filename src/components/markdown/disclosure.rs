//! Disclosure state survives virtual rows leaving the Markdown file viewport.
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{App, FocusHandle, ListState, MouseButton, RenderOnce, Window};

use super::*;

#[derive(Clone)]
pub(super) struct State(Rc<Inner>);
struct Inner {
    open: RefCell<HashMap<u64, bool>>,
    focus: RefCell<HashMap<u64, FocusHandle>>,
    scroll: Option<ListState>,
}
impl State {
    pub(super) fn is_open(&self, identity: u64, default: bool) -> bool {
        self.0
            .open
            .borrow()
            .get(&identity)
            .copied()
            .unwrap_or(default)
    }
    pub(super) fn new(scroll: Option<ListState>) -> Self {
        Self(Rc::new(Inner {
            open: RefCell::new(HashMap::new()),
            focus: RefCell::new(HashMap::new()),
            scroll,
        }))
    }
    fn toggle(&self, identity: u64, default: bool, window: &mut Window) {
        let mut values = self.0.open.borrow_mut();
        let open = values.entry(identity).or_insert(default);
        *open = !*open;
        if let Some(scroll) = &self.0.scroll {
            scroll.remeasure();
        }
        window.refresh();
    }
}

#[derive(IntoElement)]
pub(super) struct Disclosure {
    pub summary: Vec<MarkdownInline>,
    pub blocks: Vec<MarkdownBlock>,
    pub default_open: bool,
    pub identity: u64,
    pub style: MarkdownRenderStyle,
    pub state: Option<State>,
}

impl RenderOnce for Disclosure {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let identity = self.identity;
        let default_open = self.default_open;
        let state = self.state.unwrap_or_else(|| {
            window
                .use_keyed_state(
                    markdown_element_id("markdown-disclosure-state", &identity),
                    cx,
                    |_, _| State::new(None),
                )
                .read(cx)
                .clone()
        });
        let open = state.is_open(identity, default_open);
        let focus = state
            .0
            .focus
            .borrow_mut()
            .entry(identity)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let pointer = state.clone();
        let click_focus = focus.clone();
        let style = self.style;
        div()
            .w_full()
            .min_w(px(0.))
            .child(
                div()
                    .id(markdown_element_id("markdown-disclosure", &identity))
                    .debug_selector(move || format!("markdown-disclosure-{identity}"))
                    .role(gpui::Role::Button)
                    .aria_label(plain_summary(&self.summary))
                    .aria_expanded(open)
                    .track_focus(&focus)
                    .tab_stop(true)
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .py(px(4.))
                    .cursor_pointer()
                    .rounded(px(4.))
                    .focus_visible(|s| s.border_1().border_color(style.palette.link))
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        focus.focus(window, cx);
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(move |_, window, cx| {
                        click_focus.focus(window, cx);
                        pointer.toggle(identity, default_open, window);
                        cx.stop_propagation();
                    })
                    .child(
                        icon(
                            if open {
                                "chevron-down"
                            } else {
                                "review-chevron-right"
                            },
                            style.palette.text.into(),
                        )
                        .size(px(14.))
                        .flex_none(),
                    )
                    .child(render_inline_block(
                        &self.summary,
                        style,
                        style.layout.base_size,
                        style.layout.base_line_height,
                        FontWeight::MEDIUM,
                        markdown_hash(&(identity, "summary")),
                        &MarkdownFadeHandle::none(),
                    )),
            )
            .when(open, |outer| {
                outer.child(div().w_full().min_w(px(0.)).pl(px(22.)).pt(px(8.)).child(
                    render_block_sequence(
                        &self.blocks,
                        style,
                        0,
                        SequenceContext::Root,
                        markdown_hash(&(identity, "body")),
                        &MarkdownFadeHandle::none(),
                        Some(&state),
                    ),
                ))
            })
    }
}

fn plain_summary(inlines: &[MarkdownInline]) -> String {
    inlines
        .iter()
        .map(|inline| match inline {
            MarkdownInline::Text(text) | MarkdownInline::Code(text) => text.clone(),
            MarkdownInline::Strong(children)
            | MarkdownInline::Emphasis(children)
            | MarkdownInline::Strikethrough(children)
            | MarkdownInline::Link {
                content: children, ..
            } => plain_summary(children),
            MarkdownInline::SoftBreak | MarkdownInline::HardBreak => " ".into(),
        })
        .collect()
}
