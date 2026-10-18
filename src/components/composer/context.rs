//! Native file context and planning mode for side conversations.

use super::ComposerView;
use crate::{agent::AgentInputFile, components::icons::icon, theme::Theme};
use gpui::{Context, KeyDownEvent, PathPromptOptions, Role, Window, div, img, prelude::*, px};

pub struct OpenAttachmentPreview(pub std::path::PathBuf);
impl gpui::EventEmitter<OpenAttachmentPreview> for ComposerView {}

impl ComposerView {
    pub(super) fn toggle_context(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu_open {
            self.dismiss_context(cx);
        } else {
            self.context_menu_open = true;
            self.menu_open = false;
            self.permission_menu_open = false;
            self.slash_menu = None;
            self.load_context_catalog(cx);
        }
        self.prompt_focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(super) fn attach_files(&mut self, cx: &mut Context<Self>) {
        self.context_menu_open = false;
        self.context_catalog.cancel();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some(crate::i18n::text("添加文件或文件夹").into()),
        });
        let revision = self.draft_revision;
        let cwd = self.conversation.cwd.clone();
        let thread = self.thread_id().map(str::to_owned);
        let generation = self.conversation.runtime.generation;
        cx.spawn(async move |this, cx| {
            let result = selection.await;
            let _ = this.update(cx, |this, cx| {
                if this.draft_revision != revision
                    || this.conversation.cwd != cwd
                    || this.thread_id() != thread.as_deref()
                    || this.conversation.runtime.generation != generation
                {
                    return;
                }
                match result {
                    Ok(Ok(Some(paths))) => this.attach_paths(paths, cx),
                    Ok(Ok(None)) => {
                        this.focus_prompt_pending = true;
                        cx.notify();
                    }
                    error => {
                        let message = match error {
                            Ok(Err(e)) => e.to_string(),
                            Err(e) => e.to_string(),
                            _ => unreachable!(),
                        };
                        this.show_toast(super::ToastKind::Danger, message, cx);
                        this.focus_prompt_pending = true;
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }
    pub(crate) fn attach_paths(&mut self, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        if !paths.is_empty() {
            self.draft_revision = self.draft_revision.wrapping_add(1);
        }
        for path in paths.into_iter().take(16) {
            if self.prompt_context.files.len() >= 16 {
                break;
            }
            if !self
                .prompt_context
                .files
                .iter()
                .any(|file| file.path == path)
            {
                let image = crate::media::read_image_dimensions(&path)
                    .ok()
                    .flatten()
                    .is_some();
                self.prompt_context
                    .files
                    .push(AgentInputFile { path, image });
            }
        }
        self.focus_prompt_pending = true;
        cx.notify();
    }

    pub(super) fn render_attachments(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("side-chat-attachments")
            .mb(px(6.0))
            .py(px(4.0))
            .px(px(4.0))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(12.0))
            .overflow_x_scroll()
            .children(
                self.prompt_context
                    .files
                    .iter()
                    .enumerate()
                    .map(|(index, file)| {
                        let label = file
                            .path
                            .file_name()
                            .unwrap_or(file.path.as_os_str())
                            .to_string_lossy()
                            .into_owned();
                        let path = file.path.clone();
                        if file.image {
                            let key_path = path.clone();
                            let remove_path = path.clone();
                            let keyboard_remove_path = path.clone();
                            let group = gpui::SharedString::from(format!("composer-image-{index}"));
                            return div()
                                .id(("composer-image", index))
                                .debug_selector(move || format!("COMPOSER_IMAGE_{index}"))
                                .role(Role::Button)
                                .aria_label(crate::i18n::format!("预览图片 {label}" => "Preview image {label}"))
                                .focusable().tab_stop(true)
                                .group(group.clone())
                                .relative().flex_none().size(px(122.0))
                                .rounded(px(10.0)).cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.reopen_sketch(&path,cx) { cx.emit(OpenAttachmentPreview(path.clone())); }
                                    cx.stop_propagation();
                                }))
                                .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                        if !this.reopen_sketch(&key_path,cx) { cx.emit(OpenAttachmentPreview(key_path.clone())); }
                                        cx.stop_propagation();
                                    }
                                }))
                                .child(div().size_full().rounded(px(10.0)).overflow_hidden()
                                    .child(img(file.path.clone()).flex_none().size(px(122.0)).aspect_ratio(1.0).rounded(px(10.0)).object_fit(gpui::ObjectFit::Cover)))
                                .child(div().absolute().inset_0().rounded(px(10.0))
                                    .border_1().border_color(theme.text.alpha(0.1))
                                    .group_hover(group.clone(), move |s| s.bg(theme.text.alpha(0.05)).border_color(theme.text.alpha(0.2))))
                                .child(div().id(("side-chat-remove-attachment", index))
                                    .role(Role::Button)
                                    .aria_label(crate::i18n::format!("移除附件 {label}" => "Remove attachment {label}"))
                                    .focusable().tab_stop(true)
                                    .absolute().top(px(-2.0)).right(px(-2.0)).size(px(16.0))
                                    .rounded_full().bg(theme.text).text_color(theme.control_soft)
                                    .shadow(vec![gpui::BoxShadow::new(px(0.0), px(0.0), theme.surface.into()).spread_radius(px(2.0))])
                                    .flex().items_center().justify_center().cursor_pointer()
                                    .opacity(0.0)
                                    .group_hover(group, |s| s.opacity(1.0))
                                    .focus_visible(|s| s.opacity(1.0))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_attachment(&remove_path, cx);
                                        cx.stop_propagation();
                                    }))
                                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                            this.remove_attachment(&keyboard_remove_path, cx);
                                            cx.stop_propagation();
                                        }
                                    }))
                                    .child(icon("close-dialog", theme.surface.into()).size(px(12.0))))
                                .into_any_element();
                        }
                        div()
                            .id(("side-chat-attachment", index))
                            .flex_none()
                            .max_w(px(200.0))
                            .h(px(26.0))
                            .px(px(8.0))
                            .rounded(px(8.0))
                            .bg(theme.text.alpha(0.06))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                icon(
                                    "utility-folder",
                                    theme.text_secondary.into(),
                                )
                                .size(px(14.0)),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(12.0))
                                    .text_color(theme.text)
                                    .child(label.clone()),
                            )
                            .child(
                                div()
                                    .id(("side-chat-remove-attachment", index))
                                    .role(Role::Button)
                                    .aria_label(crate::i18n::format!("移除附件 {label}" => "Remove attachment {label}"))
                                    .focusable()
                                    .tab_stop(true)
                                    .size(px(20.0))
                                    .rounded(px(4.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(theme.sidebar_hover))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.draft_revision = this.draft_revision.wrapping_add(1);
                                        this.prompt_context.files.retain(|file| file.path != path);
                                        cx.notify();
                                        cx.stop_propagation();
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, e: &KeyDownEvent, _, cx| {
                                            if matches!(e.keystroke.key.as_str(), "enter" | "space")
                                            {
                                                if index < this.prompt_context.files.len() {
                                                    this.draft_revision =
                                                        this.draft_revision.wrapping_add(1);
                                                    this.prompt_context.files.remove(index);
                                                }
                                                cx.notify();
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .child(
                                        icon("close-dialog", theme.text_secondary.into())
                                            .size(px(12.0)),
                                    ),
                            )
                            .into_any_element()
                    }),
            )
    }

    fn remove_attachment(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        self.draft_revision = self.draft_revision.wrapping_add(1);
        self.prompt_context.files.retain(|file| file.path != path);
        self.focus_prompt_pending = true;
        cx.notify();
    }
}
