//! The Files tab's viewer header (ChatGPT 26.930): the open file's path as a
//! breadcrumb capsule, the file tree toggle, and the `Open` split button with
//! its targets.

use gpui::{
    AnyElement, BoxShadow, Context, Div, FontWeight, MouseButton, Role, SharedString, Stateful,
    Window, div, prelude::*, px,
};

use super::FilePanel;
use crate::{
    components::{icons::icon, viewer_header},
    theme::Theme,
};

/// Where the `Open` split button opens the current file.
#[derive(Clone, Copy)]
enum OpenTarget {
    DefaultApp,
    Terminal,
    Folder,
    Reload,
}

impl FilePanel {
    /// The open document's path for the breadcrumb: the workspace folder
    /// first, then each component, or nothing for the empty `/` state.
    fn breadcrumb(&self) -> Vec<String> {
        let Some(d) = self
            .current()
            .filter(|d| d.plan.is_none() && d.terminal.is_none())
        else {
            return Vec::new();
        };
        let mut crumbs = vec![
            self.cwd
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ];
        let relative = d.path.strip_prefix(&self.cwd).unwrap_or(&d.path);
        crumbs.extend(
            relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned()),
        );
        crumbs
    }

    pub(super) fn viewer_header(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let mode = self.mode;
        let crumbs = self.breadcrumb();
        let last = crumbs.len().saturating_sub(1);
        let path = div()
            .id("file-path")
            .role(Role::Navigation)
            .aria_label(crate::i18n::text("文件路径"))
            .h(px(28.))
            .min_w(px(0.))
            .px(px(8.))
            .flex()
            .items_center()
            .justify_end()
            .overflow_hidden()
            .text_size(px(13.))
            .line_height(px(18.5714))
            .text_color(theme.text_secondary)
            .when(crumbs.is_empty(), |nav| nav.child("/"))
            .children(crumbs.into_iter().enumerate().map(|(index, crumb)| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .when(index > 0, |item| {
                        item.child(
                            icon("review-chevron-right", theme.text_tertiary.into())
                                .size(px(14.))
                                .flex_none(),
                        )
                    })
                    .child(
                        div()
                            .whitespace_nowrap()
                            .when(index == last, |label| {
                                label.font_weight(FontWeight::MEDIUM).text_color(theme.text)
                            })
                            .child(crumb),
                    )
            }));
        let status = self.current().filter(|d| d.editor.is_some()).map(|d| {
            div()
                .flex_none()
                .text_size(px(11.))
                .text_color(if d.error.is_some() {
                    theme.warning
                } else {
                    theme.text_tertiary
                })
                .child(if d.saving {
                    crate::i18n::text("保存中…")
                } else if d.dirty(cx) {
                    crate::i18n::text("未保存")
                } else {
                    ""
                })
        });
        let preview = self
            .current()
            .filter(|d| {
                d.editor.is_some()
                    && matches!(
                        d.path.extension().and_then(|s| s.to_str()),
                        Some("md" | "markdown")
                    )
            })
            .map(|d| {
                let id = d.id;
                let label = if d.preview {
                    crate::i18n::text("查看源代码")
                } else {
                    crate::i18n::text("预览")
                };
                viewer_header::capsule(mode).child(
                    div()
                        .id("file-preview")
                        .role(Role::Button)
                        .aria_label(label)
                        .tab_stop(true)
                        .h(px(28.))
                        .px(px(10.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .text_color(theme.text)
                        .cursor_pointer()
                        .hover(move |s| s.bg(theme.sidebar_hover))
                        .child(label)
                        .on_click(cx.listener(move |s, _, window, cx| {
                            if let Some(d) = s.documents.iter_mut().find(|d| d.id == id) {
                                d.preview = !d.preview;
                                s.focus_editor = true;
                            }
                            window.refresh();
                            cx.notify();
                        })),
                )
            });
        let (pressed_fg, pressed_bg) = viewer_header::pressed(mode);
        let tree_open = self.tree_open;
        let toggle = div()
            .id("toggle-file-tree")
            .role(Role::Button)
            .aria_label(crate::i18n::text("切换文件树"))
            .aria_selected(tree_open)
            .tab_stop(true)
            .size(px(32.))
            .flex_none()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .shadow(viewer_header::shadow(mode))
            .bg(if tree_open {
                pressed_bg
            } else {
                viewer_header::surface(mode)
            })
            .when(!tree_open, |b| b.hover(move |s| s.bg(theme.sidebar_hover)))
            .focus_visible(move |s| s.border_1().border_color(theme.accent))
            .child(
                icon(
                    "pr-file-tree",
                    if tree_open {
                        pressed_fg
                    } else {
                        theme.text_tertiary
                    }
                    .into(),
                )
                .size(px(18.)),
            )
            .on_click(cx.listener(|s, _, _, cx| {
                s.tree_open = !s.tree_open;
                cx.notify();
            }));
        div()
            .h(px(viewer_header::HEIGHT))
            .w_full()
            .flex_none()
            .p(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                viewer_header::capsule(mode)
                    .min_w(px(0.))
                    .flex_shrink(1.)
                    .child(path),
            )
            .children(status)
            .child(div().flex_1().min_w(px(0.)))
            .children(preview)
            .child(toggle)
            .when(self.open_target_available(), |row| {
                row.child(self.open_button(theme, cx))
            })
    }

    fn open_target_available(&self) -> bool {
        self.current().is_some_and(|d| {
            d.plan.is_none() && d.terminal.is_none() && d.goal.is_none() && !d.loading
        })
    }

    /// `Open` and its `Open options` chevron, joined in one 32px pill.
    fn open_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let anchor = self.open_anchor.clone();
        let menu_open = self.open_menu;
        div()
            .id("file-open-split")
            .h(px(32.))
            .flex_none()
            .flex()
            .items_stretch()
            .rounded_full()
            .overflow_hidden()
            .bg(viewer_header::surface(self.mode))
            .shadow(viewer_header::shadow(self.mode))
            .child(
                gpui::canvas(
                    move |bounds, _, _| anchor.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .id("file-open")
                    .role(Role::Button)
                    .aria_label(crate::i18n::text("在默认应用中打开"))
                    .tab_stop(true)
                    .pl(px(8.))
                    .pr(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(theme.text)
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .child(icon("review-open", theme.text_secondary.into()).size(px(16.)))
                    .child(div().whitespace_nowrap().child(crate::i18n::text("打开")))
                    .on_click(cx.listener(|s, _, window, cx| {
                        s.open_in(OpenTarget::DefaultApp, window, cx)
                    })),
            )
            .child(
                div()
                    .id("file-open-options")
                    .role(Role::Button)
                    .aria_label(crate::i18n::text("打开选项"))
                    .aria_expanded(menu_open)
                    .tab_stop(true)
                    .pl(px(2.))
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .when(menu_open, |b| b.bg(theme.sidebar_hover))
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .child(
                        icon("chevron-down", theme.text.into())
                            .size(px(14.))
                            .opacity(0.5),
                    )
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.open_menu = !s.open_menu;
                        cx.notify();
                    })),
            )
    }

    /// The `Open options` menu under the split button: the default app and
    /// Terminal, then the enclosing folder, plus Echora's reload from disk.
    pub(super) fn open_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.open_menu || !self.open_target_available() {
            return None;
        }
        let bounds = self.open_anchor.get()?;
        let item = |id: &'static str, label: &'static str, glyph: &'static str, target| {
            div()
                .id(id)
                .role(Role::MenuItem)
                .aria_label(SharedString::from(crate::i18n::text(label)))
                .h(px(28.5625))
                .px(px(8.))
                .rounded(px(15.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(13.))
                .text_color(theme.text)
                .cursor_pointer()
                .hover(move |s| s.bg(theme.sidebar_hover))
                .child(
                    icon(glyph, theme.text.into())
                        .size(px(16.))
                        .opacity(0.75)
                        .flex_none(),
                )
                .child(crate::i18n::text(label))
                .on_click(cx.listener(move |s, _, window, cx| s.open_in(target, window, cx)))
        };
        let rule = || {
            div()
                .px(px(8.))
                .py(px(4.))
                .child(div().h(px(1.)).bg(theme.border))
        };
        let menu = div()
            .id("file-open-menu")
            .role(Role::Menu)
            .w(px(200.))
            .p(px(4.))
            .rounded(px(20.))
            .bg(theme.control)
            .border(px(0.5))
            .border_color(theme.border)
            .shadow(vec![
                BoxShadow::new(px(0.), px(8.), theme.profile_menu_shadow.into())
                    .blur_radius(px(16.))
                    .spread_radius(px(-4.)),
            ])
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                s.open_menu = false;
                cx.notify();
            }))
            .child(item(
                "file-open-default",
                "默认应用",
                "review-open",
                OpenTarget::DefaultApp,
            ))
            .child(item(
                "file-open-terminal",
                "终端",
                "panel-terminal",
                OpenTarget::Terminal,
            ))
            .child(rule())
            .child(item(
                "file-open-folder",
                "在文件夹中打开",
                "folder",
                OpenTarget::Folder,
            ))
            .child(item(
                "file-reload",
                "重新加载",
                "settings-refresh",
                OpenTarget::Reload,
            ));
        Some(
            gpui::deferred(
                gpui::anchored()
                    .anchor(gpui::Anchor::TopRight)
                    .position(bounds.bottom_right() + gpui::point(px(0.), px(4.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    fn open_in(&mut self, target: OpenTarget, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = false;
        let Some(d) = self.current() else {
            cx.notify();
            return;
        };
        let (id, path) = (d.id, d.path.clone());
        match target {
            OpenTarget::DefaultApp => cx.open_with_system(&path),
            OpenTarget::Terminal => {
                let folder = path.parent().unwrap_or(&self.cwd).to_owned();
                let _ = std::process::Command::new("open")
                    .args(["-a", "Terminal"])
                    .arg(folder)
                    .spawn();
            }
            OpenTarget::Folder => {
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(&path)
                    .spawn();
            }
            OpenTarget::Reload => self.reload(id, window, cx),
        }
        cx.notify();
    }
}
