//! Shared Projects/Recents layout and chat sorting menus.
use super::{
    AccountMenuRhythm, SidebarView, Theme, account_menu_ring, account_menu_row,
    account_menu_separator, account_menu_surface, icon,
};
use gpui::{
    Bounds, Context, Div, FocusHandle, IntoElement, MouseButton, Pixels, Window, canvas, deferred,
    div, prelude::*, px,
};

const ITEM: f32 = 28.5625;
#[derive(Default)]
pub(super) struct RecentsMenu {
    pub open: bool,
    pub from_projects: bool,
    pub project_anchor: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    pub project_focus: Option<FocusHandle>,
    pub submenu: Option<usize>,
    pub selected: Option<usize>,
    pub sub_selected: Option<usize>,
    pub anchor: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    pub focus: Option<FocusHandle>,
}

impl SidebarView {
    pub(super) fn recents_loader(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let view = cx.entity().downgrade();
        let count = self.recent_visible_count;
        let requested = self.recent_loader_requested.clone();
        div()
            .id("recents-loader")
            .relative()
            .h(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .child(icon("search-spinner", theme.sidebar_icon_muted.into()).size(px(16.0)))
            .child(
                canvas(
                    move |bounds: Bounds<Pixels>, window, _| {
                        let viewport = Bounds::new(gpui::Point::default(), window.viewport_size());
                        if bounds.intersects(&viewport) && requested.get() != count {
                            requested.set(count);
                            let view = view.clone();
                            window.on_next_frame(move |_, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    this.recent_visible_count = count + 20;
                                    cx.notify();
                                });
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
    }

    fn recents_open(
        &mut self,
        keyboard: bool,
        from_projects: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = !self.recents_menu.open || self.recents_menu.from_projects != from_projects;
        self.close_transient_menus(cx);
        self.recents_menu.open = open;
        self.recents_menu.from_projects = from_projects;
        self.recents_menu.selected = keyboard.then_some(0);
        if open {
            self.hovered_thread_id = None;
            self.hovered_project_id = None;
            self.stop_marquee();
            self.project_hover_card = None;
            self.thread_hover_card = None;
            self.options_focus().focus(window, cx);
        }
        cx.notify();
    }

    fn options_focus(&self) -> &FocusHandle {
        if self.recents_menu.from_projects {
            self.recents_menu.project_focus.as_ref().unwrap()
        } else {
            self.recents_menu.focus.as_ref().unwrap()
        }
    }

    pub(super) fn projects_options_button(
        &self,
        visible: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let open = self.recents_menu.open && self.recents_menu.from_projects;
        let anchor = self.recents_menu.project_anchor.clone();
        Self::activity_transparent_button("projects-options", "more-horizontal", open, theme)
            .role(gpui::Role::Button)
            .aria_label(crate::i18n::text("项目侧边栏选项"))
            .aria_expanded(open)
            .track_focus(self.recents_menu.project_focus.as_ref().unwrap())
            .tab_stop(true)
            .when(!visible, |e| e.opacity(0.0))
            .focus_visible(|s| s.opacity(1.0))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.recents_open(false, true, window, cx);
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if !this.recents_menu.open
                    && matches!(event.keystroke.key.as_str(), "enter" | "space" | "down")
                {
                    this.recents_open(true, true, window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                canvas(
                    move |bounds, _, _| anchor.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
    }

    pub(super) fn recent_heading(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let visible = self.hovered_section_id == Some("recent-heading")
            || (self.recents_menu.open && !self.recents_menu.from_projects);
        let anchor = self.recents_menu.anchor.clone();
        let options = Self::activity_transparent_button(
            "recents-options",
            "more-horizontal",
            self.recents_menu.open && !self.recents_menu.from_projects,
            theme,
        )
        .role(gpui::Role::Button)
        .aria_label(crate::i18n::text("聊天侧边栏选项"))
        .aria_expanded(self.recents_menu.open && !self.recents_menu.from_projects)
        .track_focus(self.recents_menu.focus.as_ref().unwrap())
        .tab_stop(true)
        .focus_visible(|s| s.opacity(1.0))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|this, _, window, cx| {
            cx.stop_propagation();
            this.recents_open(false, false, window, cx)
        }))
        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
            if this.recents_menu.open {
                return;
            }
            if matches!(event.keystroke.key.as_str(), "enter" | "space" | "down") {
                this.recents_open(true, false, window, cx);
                cx.stop_propagation();
            }
        }))
        .child(
            canvas(
                move |bounds, _, _| anchor.set(Some(bounds)),
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
        let new_chat =
            Self::activity_transparent_button("recents-new-chat", "recents-new-chat", false, theme)
                .role(gpui::Role::Button)
                .aria_label(crate::i18n::text("新聊天"))
                .aria_keyshortcuts("Meta+N")
                .focusable()
                .tab_stop(true)
                .focus_visible(|s| s.opacity(1.0))
                .tooltip(move |_, cx| cx.new(|_| RecentsTooltip(theme)).into())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.new_conversation(None, cx)
                }))
                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        this.new_conversation(None, cx);
                        cx.stop_propagation();
                    }
                }));
        self.section_header(
            "recent-heading",
            crate::i18n::text("最近"),
            self.snapshot.preferences.recent_collapsed,
            super::RECENTS_HEADER_HEIGHT,
            theme,
            cx,
        )
        .pr(px(6.0))
        .child(
            div()
                .flex_none()
                .flex()
                .gap(px(4.0))
                .child(options.when(!visible, |e| e.opacity(0.0)))
                .child(new_chat.when(!visible, |e| e.opacity(0.0))),
        )
    }

    fn recents_apply(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let mut next = self.snapshot.preferences.sidebar.clone();
        match self.recents_menu.submenu {
            Some(0) => next.combined = index == 1,
            Some(1) => {
                next.manual_sort = index == 1;
                if next.manual_sort && next.thread_order.is_empty() {
                    next.thread_order = self
                        .snapshot
                        .recent_threads
                        .iter()
                        .map(|t| t.thread_id.clone())
                        .collect();
                }
            }
            None if index == 2 && !next.combined && !self.recents_menu.from_projects => {
                next.projects_hidden = !next.projects_hidden
            }
            _ => return,
        }
        self.store.set_sidebar_preferences(next);
        self.close_recents(window, cx);
    }

    #[cfg(test)]
    pub fn recents_menu_state(&self) -> (bool, Option<Bounds<Pixels>>) {
        (self.recents_menu.open, self.recents_menu.anchor.get())
    }

    pub fn dismiss_recents_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.recents_menu.open {
            return false;
        }
        self.close_recents(window, cx);
        true
    }

    fn close_recents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.recents_menu.open = false;
        self.recents_menu.submenu = None;
        if self.recents_menu.from_projects
            && (self.snapshot.preferences.sidebar.combined
                || self.store.snapshot().preferences.sidebar.combined)
        {
            self.recents_menu.focus.as_ref().unwrap().focus(window, cx);
        } else {
            self.options_focus().focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn recents_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.recents_menu.open {
            return;
        }
        let count = if self.recents_menu.submenu.is_some()
            || self.snapshot.preferences.sidebar.combined
            || self.recents_menu.from_projects
        {
            2
        } else {
            3
        };
        let top_selected = self.recents_menu.selected.unwrap_or(0);
        let selected = if self.recents_menu.submenu.is_some() {
            &mut self.recents_menu.sub_selected
        } else {
            &mut self.recents_menu.selected
        };
        match event.keystroke.key.as_str() {
            "down" => *selected = Some(selected.map_or(0, |n| (n + 1) % count)),
            "up" => *selected = Some(selected.map_or(count - 1, |n| (n + count - 1) % count)),
            "home" => *selected = Some(0),
            "end" => *selected = Some(count - 1),
            "right" | "enter" | "space"
                if self.recents_menu.submenu.is_none() && top_selected < 2 =>
            {
                self.recents_menu.submenu = Some(top_selected);
                self.recents_menu.sub_selected = Some(0);
            }
            "left" if self.recents_menu.submenu.is_some() => {
                self.recents_menu.submenu = None;
                self.recents_menu.sub_selected = None;
            }
            "enter" | "space" => {
                let index = selected.unwrap_or(0);
                self.recents_apply(index, window, cx);
            }
            "escape" | "tab" => self.close_recents(window, cx),
            key => {
                let labels = match self.recents_menu.submenu {
                    Some(0) => [
                        crate::i18n::text("分项目显示"),
                        crate::i18n::text("合并显示"),
                        "",
                    ],
                    Some(1) => [
                        crate::i18n::text("最近更新"),
                        crate::i18n::text("手动排序"),
                        "",
                    ],
                    _ => [
                        crate::i18n::text("整理侧边栏"),
                        crate::i18n::text("聊天排序方式"),
                        crate::i18n::text("项目"),
                    ],
                };
                if key.chars().count() == 1 {
                    if let Some(index) = labels
                        .iter()
                        .position(|s| s.to_lowercase().starts_with(&key.to_lowercase()))
                    {
                        *selected = Some(index);
                    }
                } else {
                    return;
                }
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn recents_surface(
        &self,
        id: &'static str,
        width: f32,
        theme: Theme,
        scale: f32,
    ) -> gpui::Stateful<Div> {
        let mut shadows = account_menu_ring(theme, scale).to_vec();
        shadows.push(
            gpui::BoxShadow::new(px(0.0), px(8.0), gpui::rgba(0x0000001f).into())
                .blur_radius(px(16.0))
                .spread_radius(px(-4.0)),
        );
        div()
            .id(id)
            .role(gpui::Role::Menu)
            .w(px(width))
            .p(px(4.0))
            .rounded(px(20.0))
            .bg(account_menu_surface(theme))
            .shadow(shadows)
            .font(super::hover_card_font(crate::theme::UI_BODY_FONT_WEIGHT))
            .flex()
            .flex_col()
            .text_size(px(crate::typography::ui_size(13.0)))
            .text_color(theme.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
    }

    pub(super) fn recents_overlay(
        &self,
        theme: Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.recents_menu.open {
            return None;
        }
        let anchor = if self.recents_menu.from_projects {
            self.recents_menu.project_anchor.get()?
        } else {
            self.recents_menu.anchor.get()?
        };
        let prefs = &self.snapshot.preferences.sidebar;
        let show_projects = !prefs.combined && !self.recents_menu.from_projects;
        let height = if !show_projects {
            8.0 + 2.0 * ITEM
        } else {
            8.0 + 3.0 * ITEM + 9.0 + 26.0
        };
        let viewport = window.viewport_size();
        let x = (f32::from(anchor.left()) + 1.0)
            .clamp(8.0, (f32::from(viewport.width) - 228.0).max(8.0));
        let y = (f32::from(anchor.bottom()) + 2.0)
            .min(f32::from(viewport.height) - height - 8.0)
            .max(8.0);
        let mut menu = self
            .recents_surface("recents-menu", 220.0, theme, window.scale_factor())
            .aria_label(crate::i18n::text(if self.recents_menu.from_projects {
                "项目侧边栏选项"
            } else {
                "聊天侧边栏选项"
            }));
        let mut rhythm = AccountMenuRhythm::new(window.scale_factor());
        for (index, (id, label, glyph)) in [
            (
                "recents-organize",
                crate::i18n::text("整理侧边栏"),
                "recents-organize",
            ),
            (
                "recents-sort",
                crate::i18n::text("聊天排序方式"),
                "recents-sort",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            menu = menu.child(
                account_menu_row(
                    id,
                    label.into(),
                    Some(glyph),
                    None,
                    rhythm.span(ITEM),
                    theme,
                    true,
                )
                .relative()
                .role(gpui::Role::MenuItem)
                .aria_label(label)
                .aria_expanded(self.recents_menu.submenu == Some(index))
                .when(self.recents_menu.selected == Some(index), |e| {
                    e.bg(theme.sidebar_hover)
                })
                .child(
                    icon("settings-chevron-right", theme.text_tertiary.into())
                        .absolute()
                        .right(px(8.0))
                        .top(px(6.0))
                        .size(px(16.0)),
                )
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered {
                        this.recents_menu.selected = Some(index);
                        this.recents_menu.submenu = Some(index);
                        this.recents_menu.sub_selected = None;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.recents_menu.submenu = Some(index);
                    this.recents_menu.selected = Some(index);
                    cx.notify();
                })),
            );
        }
        if show_projects {
            menu = menu
                .child(account_menu_separator(rhythm.span(9.0), theme))
                .child(
                    div()
                        .h(px(rhythm.span(26.0)))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .text_size(px(crate::typography::ui_size(12.0)))
                        .text_color(theme.text_tertiary)
                        .child(crate::i18n::text("显示")),
                )
                .child(
                    account_menu_row(
                        "recents-show-projects",
                        crate::i18n::text("项目").into(),
                        None,
                        None,
                        rhythm.span(ITEM),
                        theme,
                        true,
                    )
                    .relative()
                    .role(gpui::Role::MenuItemCheckBox)
                    .aria_toggled(if prefs.projects_hidden {
                        gpui::Toggled::False
                    } else {
                        gpui::Toggled::True
                    })
                    .aria_label(crate::i18n::text("项目"))
                    .when(self.recents_menu.selected == Some(2), |e| {
                        e.bg(theme.sidebar_hover)
                    })
                    .when(!prefs.projects_hidden, |e| {
                        e.child(
                            icon("check", theme.text.into())
                                .size(px(16.0))
                                .absolute()
                                .right(px(8.0))
                                .top(px(6.0)),
                        )
                    })
                    .on_hover(cx.listener(|this, hover: &bool, _, cx| {
                        if *hover {
                            this.recents_menu.selected = Some(2);
                            this.recents_menu.submenu = None;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.recents_menu.submenu = None;
                        this.recents_apply(2, window, cx)
                    })),
                );
        }
        let mut overlay = div()
            .absolute()
            .top_0()
            .left_0()
            .w(viewport.width)
            .h(viewport.height)
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close_recents(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(div().absolute().left(px(x)).top(px(y)).child(menu));
        if let Some(kind) = self.recents_menu.submenu {
            let mut submenu =
                self.recents_surface("recents-submenu", 180.0, theme, window.scale_factor());
            let labels = if kind == 0 {
                [
                    crate::i18n::text("分项目显示"),
                    crate::i18n::text("合并显示"),
                ]
            } else {
                [crate::i18n::text("最近更新"), crate::i18n::text("手动排序")]
            };
            let mut rhythm = AccountMenuRhythm::new(window.scale_factor());
            for (index, label) in labels.into_iter().enumerate() {
                let checked = if kind == 0 {
                    prefs.combined == (index == 1)
                } else {
                    prefs.manual_sort == (index == 1)
                };
                submenu =
                    submenu.child(
                        account_menu_row(
                            if index == 0 {
                                "recents-choice-first"
                            } else {
                                "recents-choice-second"
                            },
                            label.into(),
                            (kind == 0).then_some(if index == 0 {
                                "thread-hover-project"
                            } else {
                                "recents-combined"
                            }),
                            None,
                            rhythm.span(ITEM),
                            theme,
                            true,
                        )
                        .relative()
                        .role(gpui::Role::MenuItemRadio)
                        .aria_label(label)
                        .aria_toggled(if checked {
                            gpui::Toggled::True
                        } else {
                            gpui::Toggled::False
                        })
                        .when(checked, |e| {
                            e.child(
                                icon("check", theme.text.into())
                                    .absolute()
                                    .right(px(8.0))
                                    .top(px(6.0))
                                    .size(px(16.0)),
                            )
                        })
                        .when(self.recents_menu.sub_selected == Some(index), |e| {
                            e.bg(theme.sidebar_hover)
                        })
                        .on_hover(cx.listener(move |this, hover: &bool, _, cx| {
                            if *hover {
                                this.recents_menu.sub_selected = Some(index);
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.recents_apply(index, window, cx),
                        )),
                    );
            }
            let sx = if x + 225.0 + 180.0 > f32::from(viewport.width) - 8.0 {
                x - 185.0
            } else {
                x + 225.0
            };
            let sy = (y + kind as f32 * ITEM).min(f32::from(viewport.height) - 2.0 * ITEM - 16.0);
            overlay = overlay.child(div().absolute().left(px(sx)).top(px(sy)).child(submenu));
        }
        Some(deferred(overlay).into_any_element())
    }
}

struct RecentsTooltip(Theme);
impl gpui::Render for RecentsTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = self.0;
        div()
            .px(px(12.0))
            .py(px(5.0))
            .rounded(px(20.0))
            .bg(theme.model_picker_surface)
            .text_color(theme.text)
            .text_size(px(crate::typography::ui_size(13.0)))
            .line_height(px(18.0))
            .flex()
            .gap(px(8.0))
            .child(crate::i18n::text("新聊天"))
            .child("⌘N")
    }
}
