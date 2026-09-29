//! Custom sidebar sections, after the reference's custom sections: each one a
//! heading like Pinned's with a collapse chevron and an "Options for {name}"
//! menu (New chat in {name}, Edit, Archive chats, Mark all as read, Remove
//! section), then its chats and projects, or "Drop chats or projects here".
//! Chats and projects move into a section by drag and drop, from their menus'
//! section rows, or from "New section…"; sections reorder by dragging their
//! headings. The New section / Edit section dialog is drawn by the app shell
//! like the rename dialog.
//!
//! The reference opens these menus natively, so their look follows this
//! sidebar's own menus; the section headings reuse Pinned's.

use std::collections::HashSet;

use gpui::{
    AnyElement, AppContext, Context, Div, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Transformation, Window, div, prelude::FluentBuilder, px, radians,
};

use super::{
    ROW_GAP, ROW_HORIZONTAL_PADDING, ROW_LABEL_FONT_SIZE, SECTION_HEADER_HEIGHT,
    SECTION_HEADER_LINE_HEIGHT, SECTION_LIST_PADDING_TOP, SECTION_TITLE_OPACITY, SidebarView,
    ThreadRowPlacement,
};
use crate::{
    agent::{ProjectId, ThreadId, ThreadSectionId},
    components::icons::icon,
    theme::{Theme, ThemeMode},
    workspace::{CustomSection, SectionItem},
};

/// "New chat in {section}": the host starts a draft whose thread joins the
/// section once it exists.
pub struct NewChatInSection(pub ThreadSectionId);

/// The New section / Edit section dialog the shell draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionDialog {
    /// The section being edited; `None` creates one.
    pub editing: Option<ThreadSectionId>,
    /// What a new section starts with (from a row's "New section…").
    pub item: Option<SectionItem>,
    /// The name the field started from.
    pub name: String,
}

/// A chat or project row being dragged.
#[derive(Clone, Debug)]
pub struct SidebarItemDrag {
    pub(super) item: SectionItem,
    pub(super) label: SharedString,
    pub(super) mode: ThemeMode,
}

/// A section heading being dragged to reorder the sections.
#[derive(Clone, Debug)]
pub struct SectionDrag {
    pub(super) section_id: ThreadSectionId,
    pub(super) label: SharedString,
    pub(super) mode: ThemeMode,
}

fn drag_preview(label: SharedString, mode: ThemeMode) -> Div {
    let theme = Theme::for_mode(mode);
    div()
        .h(px(28.0))
        .max_w(px(220.0))
        .px(px(10.0))
        .rounded(px(10.0))
        .bg(theme.model_picker_surface)
        .border(px(0.5))
        .border_color(theme.border)
        .flex()
        .items_center()
        .text_size(px(13.0))
        .text_color(theme.sidebar_text)
        .overflow_hidden()
        .whitespace_nowrap()
        .child(label)
}

impl Render for SidebarItemDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        drag_preview(self.label.clone(), self.mode)
    }
}

impl Render for SectionDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        drag_preview(self.label.clone(), self.mode)
    }
}

impl SidebarView {
    /// Chats that a section (Pinned or custom) holds, which the Projects and
    /// Recents lists leave out.
    pub(super) fn sectioned_thread_ids(&self) -> HashSet<&str> {
        self.snapshot
            .pinned_threads
            .iter()
            .chain(
                self.snapshot
                    .custom_sections
                    .iter()
                    .flat_map(|section| section.threads.iter()),
            )
            .map(|thread| thread.thread_id.as_str())
            .collect()
    }

    /// Projects placed in a custom section, which Projects leaves out.
    pub(super) fn sectioned_project_ids(&self) -> HashSet<&str> {
        self.snapshot
            .custom_sections
            .iter()
            .flat_map(|section| section.projects.iter())
            .map(String::as_str)
            .collect()
    }

    pub(super) fn custom_sections_enabled(&self) -> bool {
        self.snapshot.supports_custom_sections()
    }

    pub(super) fn custom_section_elements(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !self.custom_sections_enabled() {
            return Vec::new();
        }
        self.snapshot
            .custom_sections
            .iter()
            .map(|section| {
                self.custom_section(section, theme, window, cx)
                    .into_any_element()
            })
            .collect()
    }

    fn custom_section(
        &self,
        section: &CustomSection,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let section_id = section.section.section_id.clone();
        let collapsed = self
            .snapshot
            .preferences
            .collapsed_section_ids
            .contains(&section_id);
        let drop_id = section_id.clone();
        let hover_bg = theme.sidebar_hover;
        let mut container = div()
            .id(SharedString::from(format!("custom-section-{section_id}")))
            .debug_selector({
                let id = section_id.clone();
                move || format!("CUSTOM_SECTION_{id}")
            })
            .px(px(ROW_HORIZONTAL_PADDING))
            .flex()
            .flex_col()
            .rounded(px(12.5))
            .drag_over::<SidebarItemDrag>(move |style, _, _, _| style.bg(hover_bg))
            .on_drop(cx.listener(move |this, drag: &SidebarItemDrag, _, cx| {
                this.store
                    .move_to_custom_section(drag.item.clone(), Some(drop_id.clone()));
                cx.notify();
            }))
            .child(self.custom_section_header(section, collapsed, theme, cx));
        if collapsed {
            return container;
        }
        let projects = section
            .projects
            .iter()
            .filter_map(|id| {
                self.snapshot
                    .projects
                    .iter()
                    .find(|project| &project.project_id == id)
            })
            .collect::<Vec<_>>();
        if section.threads.is_empty() && projects.is_empty() {
            return container.child(
                self.status_row(
                    SharedString::from(format!("custom-section-empty-{section_id}")),
                    crate::i18n::format!("将聊天或项目拖到这里" => "Drop chats or projects here"),
                    theme,
                )
                .debug_selector({
                    let id = section_id.clone();
                    move || format!("CUSTOM_SECTION_EMPTY_{id}")
                }),
            );
        }
        let sectioned = self.sectioned_thread_ids();
        let mut list = div()
            .pt(px(SECTION_LIST_PADDING_TOP))
            .flex()
            .flex_col()
            .gap(px(ROW_GAP));
        for project in projects {
            list = list.child(self.project_group(project, &sectioned, theme, window, cx));
        }
        for thread in &section.threads {
            list = list.child(self.thread_row(
                thread,
                ThreadRowPlacement {
                    indented: false,
                    pinned: false,
                    archived: false,
                },
                theme,
                window,
                cx,
            ));
        }
        container = container.child(list);
        container
    }

    fn custom_section_header(
        &self,
        section: &CustomSection,
        collapsed: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let section_id = section.section.section_id.clone();
        let name = section.section.name.clone();
        let hovered = self.hovered_custom_section.as_deref() == Some(section_id.as_str());
        let menu_open = self.section_menu_id.as_deref() == Some(section_id.as_str());
        let show_actions = hovered || menu_open;
        let toggle_id = section_id.clone();
        let hover_id = section_id.clone();
        let menu_id = section_id.clone();
        let reorder_id = section_id.clone();
        let options = Self::nav_icon_button(
            SharedString::from(format!("custom-section-options-{section_id}")),
            "more-horizontal",
            theme,
        )
        .aria_label(crate::i18n::format!("{name}的选项" => "Options for {name}"))
        .debug_selector({
            let id = section_id.clone();
            move || format!("CUSTOM_SECTION_OPTIONS_{id}")
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.section_menu_id = if this.section_menu_id.as_deref() == Some(menu_id.as_str())
                {
                    None
                } else {
                    Some(menu_id.clone())
                };
                this.project_menu_id = None;
                this.thread_menu_id = None;
                this.projects_section_menu_open = false;
                this.menu_origin = (f32::from(event.position.x), f32::from(event.position.y));
                cx.notify();
            }),
        );
        let hover_bg = theme.sidebar_hover;
        let bounds_state = self.section_header_bounds.clone();
        let bounds_id = section_id.clone();
        div()
            .id(SharedString::from(format!(
                "custom-section-heading-{section_id}"
            )))
            .relative()
            .child(
                gpui::canvas(
                    move |bounds, _, _| {
                        bounds_state.borrow_mut().insert(bounds_id.clone(), bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .h(px(SECTION_HEADER_HEIGHT))
            .pl(px(8.0))
            .pr(px(2.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(10.0))
            .text_size(px(ROW_LABEL_FONT_SIZE))
            .line_height(px(SECTION_HEADER_LINE_HEIGHT))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(theme.sidebar_text_muted)
            .on_drag(
                SectionDrag {
                    section_id: section_id.clone(),
                    label: name.clone().into(),
                    mode: self.mode,
                },
                |drag, _, _, cx| cx.new(|_| drag.clone()),
            )
            .drag_over::<SectionDrag>(move |style, _, _, _| style.bg(hover_bg))
            .on_drop(cx.listener(move |this, drag: &SectionDrag, _, cx| {
                this.store
                    .move_custom_section(&drag.section_id, Some(reorder_id.as_str()));
                cx.notify();
            }))
            .child(
                div()
                    .id(SharedString::from(format!(
                        "custom-section-toggle-{section_id}"
                    )))
                    .debug_selector({
                        let id = section_id.clone();
                        move || format!("CUSTOM_SECTION_TOGGLE_{id}")
                    })
                    .min_w(px(0.0))
                    .flex_1()
                    .h(px(SECTION_HEADER_HEIGHT))
                    .ml(px(-4.0))
                    .py(px(2.0))
                    .px(px(4.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(10.0))
                    .opacity(SECTION_TITLE_OPACITY)
                    .cursor_pointer()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(name.clone()),
                    )
                    .child(
                        icon("section-chevron", theme.sidebar_icon_muted.into())
                            .size(px(14.0))
                            .flex_none()
                            .when(!show_actions, |chevron| chevron.invisible())
                            .with_transformation(Transformation::rotate(radians(if collapsed {
                                -std::f32::consts::FRAC_PI_2
                            } else {
                                0.0
                            }))),
                    )
                    .on_click(cx.listener(move |this, _, _, _| {
                        this.store
                            .set_custom_section_collapsed(&toggle_id, !collapsed);
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .when(!show_actions, |actions| actions.invisible())
                    .child(options),
            )
            .on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                if *is_hovered {
                    this.hovered_custom_section = Some(hover_id.clone());
                } else if this.hovered_custom_section.as_deref() == Some(hover_id.as_str()) {
                    this.hovered_custom_section = None;
                }
                cx.notify();
            }))
    }

    /// The heading's "Options for {name}" menu.
    pub(super) fn section_options_menu(
        &self,
        section: &CustomSection,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let section_id = section.section.section_id.clone();
        let name = section.section.name.clone();
        let chat_ids = self.snapshot.custom_section_thread_ids(&section_id);
        let unread = chat_ids
            .iter()
            .any(|id| self.snapshot.preferences.unread_thread_ids.contains(id));
        let can_archive = !chat_ids.is_empty()
            && self
                .snapshot
                .capabilities
                .supports(crate::agent::AgentCapability::ThreadArchive);
        let new_chat_id = section_id.clone();
        let edit_id = section_id.clone();
        let edit_name = name.clone();
        let archive_ids = chat_ids.clone();
        let read_ids = chat_ids;
        let remove_id = section_id.clone();
        self.menu_shell(
            SharedString::from(format!("custom-section-menu-{section_id}")),
            214.0,
            theme,
        )
        .child(
            Self::menu_item(
                "custom-section-new-chat",
                crate::i18n::format!("在{name}中新建聊天" => "New chat in {name}"),
                "new-chat",
                theme,
                true,
            )
            .debug_selector(|| "CUSTOM_SECTION_MENU_NEW_CHAT".to_owned())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.section_menu_id = None;
                cx.emit(NewChatInSection(new_chat_id.clone()));
                cx.notify();
            })),
        )
        .child(Self::menu_separator(theme))
        .child(
            Self::menu_item(
                "custom-section-edit",
                crate::i18n::format!("编辑" => "Edit"),
                "settings-edit",
                theme,
                true,
            )
            .debug_selector(|| "CUSTOM_SECTION_MENU_EDIT".to_owned())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_section_dialog(Some(edit_id.clone()), None, edit_name.clone(), cx);
            })),
        )
        .child(
            Self::menu_item(
                "custom-section-archive",
                crate::i18n::format!("归档聊天" => "Archive chats"),
                "archive",
                theme,
                can_archive,
            )
            .debug_selector(|| "CUSTOM_SECTION_MENU_ARCHIVE".to_owned())
            .when(can_archive, |row| {
                row.on_click(cx.listener(move |this, _, _, cx| {
                    this.section_menu_id = None;
                    let _ = this.store.archive_threads(archive_ids.clone());
                    cx.notify();
                }))
            }),
        )
        .child(
            Self::menu_item(
                "custom-section-mark-read",
                crate::i18n::format!("全部标为已读" => "Mark all as read"),
                "check",
                theme,
                unread,
            )
            .debug_selector(|| "CUSTOM_SECTION_MENU_MARK_READ".to_owned())
            .when(unread, |row| {
                row.on_click(cx.listener(move |this, _, _, cx| {
                    this.section_menu_id = None;
                    this.store.mark_threads_read(&read_ids);
                    cx.notify();
                }))
            }),
        )
        .child(Self::menu_separator(theme))
        .child(
            Self::menu_item(
                "custom-section-remove",
                crate::i18n::format!("移除分区" => "Remove section"),
                "close-dialog",
                theme,
                true,
            )
            .debug_selector(|| "CUSTOM_SECTION_MENU_REMOVE".to_owned())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.section_menu_id = None;
                this.store.delete_custom_section(remove_id.clone());
                cx.notify();
            })),
        )
    }

    /// A row menu's section rows: one per section (checked for the item's
    /// own, which choosing again takes it out) and "New section…".
    pub(super) fn section_menu_rows(
        &self,
        item: SectionItem,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !self.custom_sections_enabled() {
            return Vec::new();
        }
        let current = match &item {
            SectionItem::Thread(id) => self.snapshot.custom_section_of_thread(id).cloned(),
            SectionItem::Project(id) => self.snapshot.custom_section_of_project(id).cloned(),
        };
        let key = match &item {
            SectionItem::Thread(id) => format!("thread-{id}"),
            SectionItem::Project(id) => format!("project-{id}"),
        };
        let mut rows = vec![Self::menu_separator(theme).into_any_element()];
        for section in &self.snapshot.custom_sections {
            let section_id = section.section.section_id.clone();
            let checked = current.as_deref() == Some(section_id.as_str());
            let move_item = item.clone();
            rows.push(
                Self::menu_item(
                    SharedString::from(format!("section-move-{key}-{section_id}")),
                    crate::i18n::format!("移至分区“{}”" => "Move to section “{}”", section.section.name),
                    if checked { "check" } else { "folder" },
                    theme,
                    true,
                )
                .debug_selector({
                    let selector = format!("SECTION_MOVE_{section_id}");
                    move || selector.clone()
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.close_row_menus();
                    this.store.move_to_custom_section(
                        move_item.clone(),
                        (!checked).then(|| section_id.clone()),
                    );
                    cx.notify();
                }))
                .into_any_element(),
            );
        }
        let new_item = item;
        rows.push(
            Self::menu_item(
                SharedString::from(format!("section-new-{key}")),
                crate::i18n::format!("新建分区…" => "New section…"),
                "add",
                theme,
                true,
            )
            .debug_selector(|| "SECTION_MOVE_NEW".to_owned())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_section_dialog(None, Some(new_item.clone()), String::new(), cx);
            }))
            .into_any_element(),
        );
        rows
    }

    /// The Projects heading menu's last row, after a separator.
    pub(super) fn new_section_menu_rows(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !self.custom_sections_enabled() {
            return Vec::new();
        }
        vec![
            Self::menu_separator(theme).into_any_element(),
            Self::menu_item(
                "projects-new-section",
                crate::i18n::format!("新建分区" => "New section"),
                "add",
                theme,
                true,
            )
            .debug_selector(|| "PROJECTS_MENU_NEW_SECTION".to_owned())
            .on_click(cx.listener(|this, _, _, cx| {
                this.open_section_dialog(None, None, String::new(), cx);
            }))
            .into_any_element(),
        ]
    }

    /// A drop on Projects or Recents takes a chat or project out of its
    /// custom section.
    pub(super) fn section_exit_drop<E: InteractiveElement + FluentBuilder>(
        &self,
        target: E,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> E {
        if !self.custom_sections_enabled() {
            return target;
        }
        let hover_bg = theme.sidebar_hover;
        target
            .drag_over::<SidebarItemDrag>(move |style, _, _, _| style.bg(hover_bg))
            .on_drop(cx.listener(|this, drag: &SidebarItemDrag, _, cx| {
                let in_section = match &drag.item {
                    SectionItem::Thread(id) => this.snapshot.custom_section_of_thread(id).is_some(),
                    SectionItem::Project(id) => {
                        this.snapshot.custom_section_of_project(id).is_some()
                    }
                };
                if in_section {
                    this.store.move_to_custom_section(drag.item.clone(), None);
                    cx.notify();
                }
            }))
    }

    pub(super) fn thread_drag(&self, thread_id: &ThreadId, title: &str) -> Option<SidebarItemDrag> {
        self.custom_sections_enabled().then(|| SidebarItemDrag {
            item: SectionItem::Thread(thread_id.clone()),
            label: title.to_owned().into(),
            mode: self.mode,
        })
    }

    pub(super) fn project_drag(
        &self,
        project_id: &ProjectId,
        name: &str,
    ) -> Option<SidebarItemDrag> {
        self.custom_sections_enabled().then(|| SidebarItemDrag {
            item: SectionItem::Project(project_id.clone()),
            label: name.to_owned().into(),
            mode: self.mode,
        })
    }

    fn close_row_menus(&mut self) {
        self.thread_menu_id = None;
        self.project_menu_id = None;
        self.section_menu_id = None;
        self.projects_section_menu_open = false;
    }

    /// Opens the New section / Edit section dialog with its field focused.
    pub fn open_section_dialog(
        &mut self,
        editing: Option<ThreadSectionId>,
        item: Option<SectionItem>,
        name: String,
        cx: &mut Context<Self>,
    ) {
        self.close_row_menus();
        self.section_name_input
            .update(cx, |input, cx| input.set_rename_text(&name, cx));
        self.section_dialog = Some(SectionDialog {
            editing,
            item,
            name,
        });
        self.section_dialog_focus_pending = true;
        cx.notify();
    }

    pub fn section_dialog(&self) -> Option<SectionDialog> {
        self.section_dialog.clone()
    }

    pub fn section_name_input(&self) -> gpui::Entity<crate::components::prompt_input::PromptInput> {
        self.section_name_input.clone()
    }

    /// Whether the dialog's primary button may submit: editing needs a name,
    /// creating does not (an empty name saves as "New section").
    pub fn section_dialog_submittable(&self, cx: &gpui::App) -> bool {
        self.section_dialog.as_ref().is_some_and(|dialog| {
            dialog.editing.is_none() || !self.section_name_input.read(cx).text().trim().is_empty()
        })
    }

    pub fn dismiss_section_dialog(&mut self, cx: &mut Context<Self>) {
        if self.section_dialog.take().is_some() {
            self.section_name_input
                .update(cx, |input, cx| input.clear(cx));
            cx.notify();
        }
    }

    pub fn submit_section_dialog(&mut self, cx: &mut Context<Self>) {
        self.section_name_input
            .update(cx, |input, cx| input.submit(cx));
    }

    /// Enter or the primary button: creates the section (with its first
    /// item) or renames it. A rename to an empty or unchanged name is a no-op.
    pub(super) fn commit_section_dialog(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(dialog) = self.section_dialog.clone() else {
            return;
        };
        let name = name.trim().to_owned();
        match dialog.editing {
            Some(section_id) => {
                if name.is_empty() {
                    return;
                }
                if name != dialog.name {
                    self.store.rename_custom_section(section_id, name);
                }
            }
            None => self.store.create_custom_section(name, dialog.item),
        }
        self.section_dialog = None;
        self.section_name_input
            .update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    /// Screenshot states once the seeded sections are listed: `sidebar`,
    /// `hover` (the first heading's actions), `menu` (its options menu),
    /// `thread-menu` (a chat's menu with the section rows), `dialog-new`,
    /// `dialog-edit`.
    #[cfg(feature = "screenshot")]
    pub fn set_sections_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        let Some(section) = self.snapshot.custom_sections.first().cloned() else {
            self.pending_sections_capture = Some(state.to_owned());
            return;
        };
        let id = section.section.section_id.clone();
        match state {
            "hover" => self.hovered_custom_section = Some(id),
            "menu" => {
                self.hovered_custom_section = Some(id.clone());
                self.section_menu_id = Some(id);
                self.menu_origin = (0.0, 0.0);
            }
            "thread-menu" => {
                if let Some(thread) = section.threads.first() {
                    self.thread_menu_id = Some(thread.thread_id.clone());
                    self.menu_origin = (0.0, 0.0);
                }
            }
            "dialog-new" => self.open_section_dialog(None, None, String::new(), cx),
            "dialog-edit" => {
                self.open_section_dialog(Some(id), None, section.section.name.clone(), cx)
            }
            _ => {}
        }
        cx.notify();
    }
}
