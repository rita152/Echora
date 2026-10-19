//! Status and feedback raised by slash commands.
use super::ComposerView;
use crate::{
    components::file_editor::{EditorEvent, FileEditor},
    theme::{Theme, ThemeMode},
};
use gpui::{
    AnyElement, Context, Entity, FocusHandle, Focusable, MouseButton, Role, Window, div,
    prelude::*, px,
};

pub(super) struct CommandPanels {
    status: bool,
    status_cycle: u64,
    quota: Vec<String>,
    quota_loading: bool,
    quota_error: Option<String>,
    pub(super) feedback: bool,
    classification: Option<usize>,
    reason: Entity<FileEditor>,
    include_documents: bool,
    result: Option<(bool, String)>,
    include_logs: bool,
    include_browser_logs: bool,
    browser_diagnostics: Vec<serde_json::Value>,
    sending: bool,
    error: Option<String>,
    focus: FocusHandle,
    focus_pending: bool,
    controls: Vec<FocusHandle>,
}
const CLASSIFICATIONS: [(&str, &str, &str); 7] = [
    ("bug", "错误", "Bug"),
    ("bad_result", "结果异常", "Bad result"),
    ("good_result", "结果正常", "Good result"),
    ("safety_check", "安全检查", "Safety check"),
    ("research_failure", "研究失败", "Research failure"),
    ("high_memory_usage", "RAM 用量过高", "High RAM usage"),
    ("other", "其他", "Other"),
];
impl CommandPanels {
    pub fn new(cx: &mut Context<ComposerView>) -> Self {
        let reason = cx.new(|cx| {
            let mut input =
                FileEditor::prose(ThemeMode::Dark, crate::i18n::text("填写详情（必填）"), cx);
            input.set_text_metrics(13., 18.571_4, cx);
            input
        });
        cx.subscribe(&reason, |_, _, _: &EditorEvent, cx| cx.notify())
            .detach();
        Self {
            status: false,
            status_cycle: 0,
            quota: Vec::new(),
            quota_loading: false,
            quota_error: None,
            feedback: false,
            classification: None,
            reason,
            include_documents: true,
            result: None,
            include_logs: true,
            include_browser_logs: true,
            browser_diagnostics: Vec::new(),
            sending: false,
            error: None,
            focus: cx.focus_handle(),
            focus_pending: false,
            controls: (0..15).map(|_| cx.focus_handle().tab_stop(true)).collect(),
        }
    }
}
impl ComposerView {
    pub(super) fn open_command_status(&mut self, cx: &mut Context<Self>) {
        self.command_panels.status = true;
        self.command_panels.status_cycle = self.command_panels.status_cycle.wrapping_add(1);
        let cycle = self.command_panels.status_cycle;
        self.command_panels.quota_loading = true;
        self.command_panels.quota.clear();
        self.command_panels.quota_error = None;
        let response = self.backend.read_rate_limits();
        cx.spawn(async move |this, cx| {
            let result = response.recv().await;
            let _ = this.update(cx, |this, cx| {
                if this.command_panels.status_cycle != cycle {
                    return;
                }
                this.command_panels.quota_loading = false;
                match result {
                    Ok(Ok(read)) => {
                        for patch in read.patches {
                            let name = patch
                                .limit_name
                                .as_ref()
                                .and_then(|s| s.as_deref())
                                .unwrap_or(patch.key());
                            for window in [patch.primary.as_ref(), patch.secondary.as_ref()]
                                .into_iter()
                                .flatten()
                                .flatten()
                            {
                                let remaining = 100 - window.used_percent.clamp(0, 100);
                                let duration = window
                                    .window_duration_mins
                                    .map(|m| {
                                        if m % 1440 == 0 {
                                            format!("{}d", m / 1440)
                                        } else {
                                            format!("{}h", m / 60)
                                        }
                                    })
                                    .unwrap_or_default();
                                let reset = window
                                    .resets_at
                                    .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                                    .map(|t| {
                                        t.with_timezone(&chrono::Local)
                                            .format("%m/%d %H:%M")
                                            .to_string()
                                    })
                                    .unwrap_or_default();
                                this.command_panels
                                    .quota
                                    .push(format!("{name} {duration}: {remaining}% · {reset}"));
                            }
                        }
                    }
                    Ok(Err(error)) => this.command_panels.quota_error = Some(error),
                    Err(error) => this.command_panels.quota_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_command_status(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.command_panels.status {
            return None;
        }
        let mut rows = Vec::new();
        if let Some(id) = &self.conversation.thread_id {
            rows.push(format!("{}: {id}", crate::i18n::text("聊天")));
        }
        if let Some(usage) = self.context_usage_percent() {
            rows.push(crate::i18n::format!("上下文：已使用 {usage}%" => "Context: {usage}% used"));
        }
        if self.command_panels.quota_loading {
            rows.push(crate::i18n::format!("额度：正在加载…" => "Limits: loading…"));
        } else if self.command_panels.quota.is_empty() {
            rows.push(crate::i18n::format!("额度：不可用" => "Limits: unavailable"));
        } else {
            rows.extend(self.command_panels.quota.clone());
        }
        Some(
            div()
                .id("slash-status")
                .w_full()
                .mb(px(12.))
                .p(px(12.))
                .rounded(px(12.))
                .bg(theme.control_soft)
                .text_color(theme.text)
                .text_size(px(crate::typography::ui_size(13.)))
                .line_height(px(20.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .child(crate::i18n::text("状态"))
                        .child(
                            div()
                                .id("slash-status-close")
                                .role(Role::Button)
                                .cursor_pointer()
                                .text_color(theme.text_tertiary)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.command_panels.status = false;
                                    cx.notify();
                                }))
                                .child(crate::i18n::text("关闭")),
                        ),
                )
                .children(rows.into_iter().map(|row| div().child(row)))
                .when_some(self.command_panels.quota_error.clone(), |d, error| {
                    d.child(div().text_color(theme.text_tertiary).child(error))
                })
                .into_any_element(),
        )
    }

    pub(crate) fn set_feedback_browser_diagnostics(
        &mut self,
        diagnostics: Vec<serde_json::Value>,
        cx: &mut Context<Self>,
    ) {
        self.command_panels.browser_diagnostics = diagnostics;
        cx.notify();
    }
    pub(super) fn open_command_feedback(&mut self, cx: &mut Context<Self>) {
        cx.emit(super::FeedbackContextRequested);
        self.command_panels.feedback = true;
        self.command_panels.result = None;
        self.command_panels.focus_pending = true;
        self.command_panels.reason.update(cx, |input, cx| {
            input.set_mode(self.mode, cx);
            input.set_accessible_name(crate::i18n::text("填写详情（必填）"));
            input.set_placeholder(crate::i18n::text("填写详情（必填）"), cx);
        });
        cx.notify();
    }
    fn send_command_feedback(&mut self, cx: &mut Context<Self>) {
        let Some(category) = self.command_panels.classification else {
            return;
        };
        let reason = self.command_panels.reason.read(cx).text().trim().to_owned();
        if reason.is_empty() || self.command_panels.sending {
            return;
        }
        self.command_panels.sending = true;
        self.command_panels.error = None;
        let mut extra_log_files = Vec::new();
        if self.command_panels.include_browser_logs {
            let path = std::env::temp_dir().join(format!(
                "echora-browser-feedback-{}-{}.json",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let result = serde_json::to_vec_pretty(&self.command_panels.browser_diagnostics)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| {
                    std::fs::write(&path, bytes)?;
                    Ok(())
                });
            if let Err(error) = result {
                self.command_panels.sending = false;
                self.command_panels.error = Some(error.to_string());
                cx.notify();
                return;
            }
            extra_log_files.push(path);
        }
        let cleanup = extra_log_files.clone();
        if self.command_panels.include_documents {
            extra_log_files.extend(self.feedback_document_paths());
        }
        let response = self
            .backend
            .upload_feedback(crate::agent::AgentFeedbackRequest {
                classification: CLASSIFICATIONS[category].0.into(),
                reason,
                include_logs: self.command_panels.include_logs,
                thread_id: self.conversation.thread_id.clone(),
                extra_log_files,
            });
        cx.spawn(async move |this, cx| {
            let result = response.recv().await;
            for path in cleanup {
                let _ = std::fs::remove_file(path);
            }
            let _ = this.update(cx, |this, cx| {
                this.command_panels.sending = false;
                match result {
                    Ok(Ok(receipt)) => {
                        this.command_panels.result = Some((true, receipt));
                        this.command_panels.classification = None;
                        this.command_panels
                            .reason
                            .update(cx, |input, cx| input.set_text_silently("", cx));
                    }
                    Ok(Err(error)) => {
                        this.command_panels.error =
                            Some(error.user_message(crate::i18n::text("提交反馈")));
                        this.command_panels.result = Some((
                            false,
                            this.conversation.thread_id.clone().unwrap_or_default(),
                        ));
                    }
                    Err(error) => {
                        this.command_panels.error = Some(error.to_string());
                        this.command_panels.result = Some((
                            false,
                            this.conversation.thread_id.clone().unwrap_or_default(),
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn feedback_document_paths(&self) -> Vec<std::path::PathBuf> {
        use crate::agent::UserMessageAttachment;
        let mut paths = self
            .conversation
            .transcript
            .iter()
            .flat_map(|t| &t.user_images)
            .chain(&self.conversation.user_images)
            .filter_map(|a| match a {
                UserMessageAttachment::File(p) => Some(p.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for text in self
            .conversation
            .transcript
            .iter()
            .map(|t| t.assistant_message.as_str())
            .chain(std::iter::once(
                self.conversation.assistant_message.as_str(),
            ))
        {
            for event in pulldown_cmark::Parser::new(text) {
                if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
                    dest_url, ..
                }) = event
                {
                    let path = std::path::PathBuf::from(dest_url.as_ref());
                    if path.is_absolute() && path.is_file() {
                        paths.push(path);
                    }
                }
            }
        }
        paths.sort();
        paths.dedup();
        paths
    }

    fn feedback_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if self.command_panels.sending {
            cx.stop_propagation();
            return;
        }
        if key == "escape" {
            self.command_panels.feedback = false;
            self.focus_prompt_pending = true;
        } else if key == "tab" {
            let mut order = if self.command_panels.result.is_some() {
                vec![
                    self.command_panels.controls[14].clone(),
                    self.command_panels.controls[10].clone(),
                ]
            } else {
                let mut order = self.command_panels.controls[..7].to_vec();
                order.push(self.command_panels.reason.read(cx).focus_handle(cx));
                if self.thread_id().is_some() {
                    order.push(self.command_panels.controls[7].clone());
                }
                order.extend(self.command_panels.controls[8..10].iter().cloned());
                order.push(self.command_panels.controls[12].clone());
                order.push(self.command_panels.controls[10].clone());
                order
            };
            order.push(self.command_panels.controls[11].clone());
            let at = order.iter().position(|f| f.is_focused(window));
            let next = match (at, event.keystroke.modifiers.shift) {
                (Some(0), true) | (None, true) => order.len() - 1,
                (Some(i), true) => i - 1,
                (Some(i), false) => (i + 1) % order.len(),
                (None, false) => 0,
            };
            order[next].focus(window, cx);
        } else {
            let Some(at) = self
                .command_panels
                .controls
                .iter()
                .position(|f| f.is_focused(window))
            else {
                return;
            };
            if at < 7 && matches!(key, "left" | "right" | "up" | "down") {
                let next = (at + if matches!(key, "left" | "up") { 6 } else { 1 }) % 7;
                self.command_panels.classification = Some(next);
                self.command_panels.controls[next].focus(window, cx);
            } else if matches!(key, "space" | "enter") {
                match at {
                    0..=6=>self.command_panels.classification=Some(at),
                    7=>self.command_panels.include_documents = !self.command_panels.include_documents,
                    8=>self.command_panels.include_logs = !self.command_panels.include_logs,
                    9=>self.command_panels.include_browser_logs = !self.command_panels.include_browser_logs,
                    10 if self.command_panels.result.is_none()=>self.send_command_feedback(cx),
                    10|11=>{self.command_panels.feedback=false;self.focus_prompt_pending=true;},
                    12=>cx.open_url("https://help.openai.com/en/articles/5722486-how-your-data-is-used-to-improve-model-performance"),
                    14=>if let Some((_,id))=&self.command_panels.result {cx.write_to_clipboard(gpui::ClipboardItem::new_string(id.clone()));},
                    _=>return,
                }
            } else {
                return;
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn feedback_checkbox(
        &self,
        index: usize,
        selected: bool,
        label: String,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(("feedback-option", index))
            .track_focus(&self.command_panels.controls[7 + index])
            .tab_stop(true)
            .focus_visible(|s| s.bg(theme.sidebar_hover))
            .role(Role::CheckBox)
            .aria_label(label.clone())
            .h(px(21.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(crate::typography::ui_size(14.)))
            .line_height(px(crate::typography::ui_leading(14., 21.)))
            .text_color(theme.text_tertiary)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.command_panels.sending {
                    return;
                }
                match index {
                    0 => {
                        this.command_panels.include_documents =
                            !this.command_panels.include_documents
                    }
                    1 => this.command_panels.include_logs = !this.command_panels.include_logs,
                    _ => {
                        this.command_panels.include_browser_logs =
                            !this.command_panels.include_browser_logs
                    }
                }
                cx.notify();
            }))
            .child(
                div()
                    .size(px(14.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(theme.border)
                    .bg(if selected {
                        theme.control_soft
                    } else {
                        theme.control
                    })
                    .text_color(theme.text)
                    .text_size(px(crate::typography::ui_size(11.)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(if selected { "✓" } else { "" }),
            )
            .child(label)
            .into_any_element()
    }

    pub(super) fn render_command_feedback(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.command_panels.feedback {
            return None;
        }
        if self.command_panels.focus_pending {
            self.command_panels.focus_pending = false;
            self.command_panels.focus.focus(window, cx);
        }
        let theme = Theme::for_mode(self.mode);
        let busy = self.command_panels.sending;
        let valid = !busy
            && self.command_panels.classification.is_some()
            && !self.command_panels.reason.read(cx).text().trim().is_empty();
        let result = self.command_panels.result.clone();
        let close = cx.listener(|this: &mut Self, _, _, cx| {
            if !this.command_panels.sending {
                this.command_panels.feedback = false;
                this.focus_prompt_pending = true;
                cx.notify();
            }
        });
        let mut body = div()
            .id("feedback-dialog")
            .role(Role::Dialog)
            .aria_label(crate::i18n::text("提交反馈"))
            .track_focus(&self.command_panels.focus)
            .on_action(
                cx.listener(|this, _: &crate::app::DismissPermissionUi, window, cx| {
                    this.feedback_key(
                        &gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("escape").unwrap(),
                            is_held: false,
                            prefer_character_input: false,
                        },
                        window,
                        cx,
                    );
                }),
            )
            .capture_action(cx.listener(
                |this, _: &crate::components::file_editor::EditorTab, window, cx| {
                    this.feedback_key(
                        &gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("tab").unwrap(),
                            is_held: false,
                            prefer_character_input: false,
                        },
                        window,
                        cx,
                    );
                },
            ))
            .w(px(if result.is_some() { 520. } else { 800. }))
            .max_w_full()
            .p(px(20.))
            .rounded(px(25.))
            .bg(theme.elevated)
            .text_color(theme.text)
            .relative()
            .flex()
            .flex_col()
            .on_click(|_, _, cx| cx.stop_propagation())
            .capture_key_down(cx.listener(Self::feedback_key));
        if let Some((success, id)) = result {
            body = body.gap(px(12.))
                .child(div().text_size(px(crate::typography::ui_size(20.))).line_height(px(28.)).font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(if success {crate::i18n::format!("感谢你的反馈"=>"Thanks for your feedback")} else {crate::i18n::format!("反馈上传未确认"=>"Feedback upload unconfirmed")}))
                .when(!success, |d| d.child(div().text_size(px(crate::typography::ui_size(14.))).line_height(px(crate::typography::ui_leading(14., 21.))).child(crate::i18n::format!("我们无法确认是否已收到你的反馈。反馈可能已送达，重新发送可能会导致重复提交"=>"We couldn't confirm whether your feedback was received. It may have arrived; sending it again may create a duplicate."))))
                .when(!id.is_empty(), |d| d.child(div().p(px(10.)).rounded(px(8.)).border_1().border_color(theme.border).flex().items_center().justify_between()
                    .child(div().flex().flex_col().child(div().text_size(px(crate::typography::ui_size(12.))).text_color(theme.text_tertiary).child(crate::i18n::format!("反馈 ID"=>"Feedback ID"))).child(id.clone()))
                    .child(div().id("feedback-copy-id").track_focus(&self.command_panels.controls[14]).tab_stop(true).focus_visible(|s| s.bg(theme.sidebar_hover)).role(Role::Button).aria_label(crate::i18n::text("复制")).cursor_pointer().on_click(move |_,_,cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(id.clone()))).child(super::super::icons::icon("message-copy",theme.text_tertiary.into()).size(px(16.))))))
                .child(div().id("feedback-result-close").track_focus(&self.command_panels.controls[10]).tab_stop(true).focus_visible(|s| s.bg(theme.sidebar_hover)).role(Role::Button).aria_label(crate::i18n::text("关闭")).w_full().h(px(32.)).rounded(px(12.5)).bg(theme.button).text_color(theme.button_text).cursor_pointer().flex().items_center().justify_center().on_click(close).child(crate::i18n::text("关闭")));
        } else {
            body = body.child(div().text_size(px(crate::typography::ui_size(20.))).line_height(px(28.)).font_weight(gpui::FontWeight::SEMIBOLD).mb(px(20.)).child(crate::i18n::text("提交反馈")))
                .child(div().flex().flex_wrap().gap(px(8.)).mb(px(12.)).children(CLASSIFICATIONS.iter().enumerate().map(|(i,(_,zh,en))| {
                    let selected=self.command_panels.classification==Some(i);
                    div().id(("feedback-category",i)).track_focus(&self.command_panels.controls[i]).tab_stop(true).focus_visible(|s| s.bg(theme.sidebar_hover)).role(Role::RadioButton).aria_label(if crate::i18n::is_english() {*en} else {*zh})
                        .px(px(12.)).py(px(4.)).text_size(px(crate::typography::ui_size(14.))).line_height(px(crate::typography::ui_leading(14., 21.))).border_1().border_color(theme.border).rounded_full()
                        .bg(if selected {theme.sidebar_hover} else {theme.elevated}).cursor_pointer().flex().items_center().gap(px(4.))
                        .on_click(cx.listener(move |this,_,_,cx| {if !this.command_panels.sending {this.command_panels.classification=Some(i);cx.notify();}}))
                        .child(div().size(px(14.)).rounded_full().border_1().border_color(theme.text_tertiary).flex().items_center().justify_center().when(selected, |d| d.child(div().size(px(8.)).rounded_full().bg(theme.text))))
                        .child(if crate::i18n::is_english() {*en} else {*zh})
                })))
                .child(div().h(px(110.8125)).px(px(10.)).py(px(8.)).rounded(px(10.)).border_1().border_color(theme.border).bg(theme.control).mb(px(12.)).child(self.command_panels.reason.clone()))
                .child(div().flex().flex_col().gap(px(12.))
                    .when(self.thread_id().is_some(), |d| d.child(self.feedback_checkbox(0,self.command_panels.include_documents,crate::i18n::format!("附上此聊天中的所有文档（如有）"=>"Attach all documents in this chat (if any)"),theme,cx)))
                    .child(self.feedback_checkbox(1,self.command_panels.include_logs,crate::i18n::format!("包含 ChatGPT 诊断日志"=>"Include ChatGPT diagnostic logs"),theme,cx))
                    .child(self.feedback_checkbox(2,self.command_panels.include_browser_logs,crate::i18n::format!("包括所有浏览器标签页和浏览器日志"=>"Include all browser tabs and browser logs"),theme,cx)))
                .child(div().mt(px(12.)).text_size(px(crate::typography::ui_size(13.))).line_height(px(crate::typography::ui_leading(13., 18.5714))).text_color(theme.text_tertiary).flex()
                    .child(crate::i18n::format!("你的反馈可用于改进 ChatGPT。"=>"Your feedback may be used to improve ChatGPT. "))
                    .child(div().id("feedback-privacy").track_focus(&self.command_panels.controls[12]).tab_stop(true).focus_visible(|s| s.bg(theme.sidebar_hover)).role(Role::Link).cursor_pointer().underline().on_click(|_,_,cx| cx.open_url("https://help.openai.com/en/articles/5722486-how-your-data-is-used-to-improve-model-performance")).child(crate::i18n::format!("了解更多。"=>"Learn more."))))
                .when_some(self.command_panels.error.clone(), |d,error| d.child(div().text_color(gpui::rgb(0xe55b58)).child(error)))
                .child(div().id("feedback-submit").track_focus(&self.command_panels.controls[10]).tab_stop(true).focus_visible(|s| s.bg(theme.sidebar_hover)).role(Role::Button).aria_label(crate::i18n::text("提交")).mt(px(20.)).w_full().h(px(32.)).rounded(px(12.5)).bg(theme.button).text_color(theme.button_text).flex().items_center().justify_center().opacity(if valid {1.} else {0.4})
                    .when(valid, |d| d.cursor_pointer().on_click(cx.listener(|this,_,_,cx| this.send_command_feedback(cx))))
                    .child(if busy {crate::i18n::format!("正在提交…"=>"Submitting…")} else {crate::i18n::text("提交").into()}));
        }
        body = body.child(
            div()
                .id("feedback-close")
                .track_focus(&self.command_panels.controls[11])
                .tab_stop(true)
                .focus_visible(|s| s.bg(theme.sidebar_hover))
                .role(Role::Button)
                .aria_label(crate::i18n::text("关闭对话框"))
                .absolute()
                .top(px(16.))
                .right(px(16.))
                .size(px(24.))
                .rounded(px(4.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.command_panels.sending {
                        this.command_panels.feedback = false;
                        this.focus_prompt_pending = true;
                        cx.notify();
                    }
                }))
                .child(super::super::icons::icon("close-dialog", theme.text.into()).size(px(16.))),
        );
        Some(
            div()
                .id("feedback-overlay")
                .absolute()
                .inset_0()
                .bg(gpui::rgba(0x00000055))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.command_panels.sending {
                        this.command_panels.feedback = false;
                        this.focus_prompt_pending = true;
                        cx.notify();
                    }
                }))
                .child(body)
                .into_any_element(),
        )
    }
}
