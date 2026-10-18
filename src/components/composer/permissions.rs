//! Permissions behavior and presentation for the prompt composer.

use gpui::{
    BoxShadow, Context, Div, KeyDownEvent, SharedString, Window, div, hsla, prelude::*, px, rgba,
};

use super::{
    ComposerView, ConversationChanged, PermissionMode, RequestFullAccessConfirmation, ToastKind,
};
use crate::{
    agent::AgentEvent,
    components::icons::icon,
    conversation::ConversationActivity,
    theme::{Theme, ThemeMode, ui_font},
};

/// Title line height of a permission row (13px `text-sm` in ChatGPT 26.1007).
const PERMISSION_TITLE_LINE_HEIGHT: f32 = 18.5714;
/// 5px padding, the title line, and the 12/14 description line.
const PERMISSION_ROW_HEIGHT: f32 = 5.0 + PERMISSION_TITLE_LINE_HEIGHT + 14.0 + 5.0;

/// The reference's message when a thread cannot be resumed because the
/// configuration does not load: what happened, what to fix, and what to do next.
fn config_load_toast_text(failure: &crate::agent::ConfigLoadFailure) -> String {
    let location = failure.location();
    let detail = &failure.detail;
    crate::i18n::format!(
        "Echora 无法加载 config.toml，因此无法恢复此对话。\n请修复 {location}：{detail}。保存文件后，请重新打开此对话。" =>
        "Echora can't load config.toml, so this thread can't resume.\nFix {location}: {detail}. After saving the file, reopen the thread."
    )
}

impl ComposerView {
    pub(crate) fn refresh_draft_defaults(&mut self, cx: &mut Context<Self>) {
        if self.conversation.thread_id.is_none() {
            self.load_permission_catalog(cx);
            cx.notify();
        }
    }
    pub(crate) fn selected_agent_permission_mode(&self) -> crate::agent::AgentPermissionMode {
        self.permission_selection.clone()
    }

    pub(super) fn load_permission_catalog(&mut self, cx: &mut Context<Self>) {
        self.load_provider_capabilities(cx);
        self.permission_catalog_cycle = self.permission_catalog_cycle.wrapping_add(1);
        let cycle = self.permission_catalog_cycle;
        let cwd = self.conversation.cwd.clone();
        self.permission_catalog_loading = true;
        // Retire a settings read started with the previous catalog/generation.
        self.permission_read_cycle = self.permission_read_cycle.wrapping_add(1);
        self.permission_effective_loading = false;
        let config = self.backend.read_config(cwd.clone());
        let profiles = self.backend.load_permission_profiles(cwd.clone());
        cx.spawn(async move |this, cx| {
            let config = config.recv().await;
            let profiles = profiles.recv().await;
            let _ = this.update(cx, |this, cx| {
                if this.permission_catalog_cycle != cycle || this.conversation.cwd != cwd {
                    return;
                }
                this.permission_catalog_loading = false;
                match (config, profiles) {
                    (Ok(Ok(config)), Ok(Ok(profiles))) => {
                        if config.generation < this.conversation.runtime.generation {
                            // A newer shared generation may start while these reads
                            // are in flight. Read again on that generation.
                            this.load_permission_catalog(cx);
                            return;
                        } else {
                            this.conversation.apply_config_defaults(&config);
                            this.permission_config = Some(config);
                            this.permission_profiles = profiles;
                            this.permission_catalog_error = None;
                            if this.conversation.permission_error.as_deref()
                                == Some(crate::i18n::text("连接已变化，请重新读取权限配置"))
                            {
                                this.conversation.permission_error = None;
                            }
                            if this.conversation.thread_id.is_none() {
                                this.set_permission_selection(this.permission_selection.clone());
                            }
                            this.load_effective_permissions(cx);
                        }
                    }
                    (Ok(Err(error)), _) => this.permission_catalog_error = Some(error.message),
                    (_, Ok(Err(error))) => this.permission_catalog_error = Some(error),
                    _ => {
                        this.permission_catalog_error =
                            Some(crate::i18n::text("权限配置连接已关闭").into())
                    }
                }
                this.start_pending_review(cx);
                this.resume_worktree_send(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-read with the new-chat settings, so a provider change saved in
    /// the configuration reaches the gated entry points.
    fn load_provider_capabilities(&mut self, cx: &mut Context<Self>) {
        let receiver = self.backend.read_provider_capabilities();
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await;
            let _ = this.update(cx, |this, cx| {
                let capabilities = match result {
                    Ok(Ok(capabilities))
                        if capabilities.generation >= this.conversation.runtime.generation =>
                    {
                        Some(capabilities)
                    }
                    Ok(Err(error)) => {
                        eprintln!("modelProvider/capabilities/read 读取失败：{error}");
                        None
                    }
                    _ => None,
                };
                if this.provider_capabilities != capabilities {
                    this.provider_capabilities = capabilities;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Image generation is gated only when the provider says it lacks it.
    pub fn image_generation_available(&self) -> bool {
        self.provider_capabilities
            .is_none_or(|capabilities| capabilities.image_generation)
    }

    pub(super) fn load_effective_permissions(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.conversation.thread_id.clone() else {
            return;
        };
        let Some(generation) = self
            .permission_config
            .as_ref()
            .map(|config| config.generation)
        else {
            return;
        };
        let cycle = self.permission_update_cycle;
        self.permission_read_cycle = self.permission_read_cycle.wrapping_add(1);
        let read_cycle = self.permission_read_cycle;
        self.permission_effective_loading = true;
        let result = self
            .backend
            .load_thread_settings(thread_id.clone(), generation);
        cx.spawn(async move |this, cx| {
            let result = result.recv().await;
            let _ = this.update(cx, |this, cx| {
                if this.permission_read_cycle != read_cycle
                    || this.conversation.thread_id.as_ref() != Some(&thread_id)
                    || this.permission_update_cycle != cycle
                    || (this.side_chat && !this.side_ready)
                {
                    return;
                }
                this.permission_effective_loading = false;
                match result {
                    Ok(Ok(snapshot)) => {
                        if snapshot.generation != generation
                            || snapshot.generation < this.conversation.runtime.generation
                            || this
                                .permission_config
                                .as_ref()
                                .is_none_or(|config| config.generation != generation)
                        {
                            return;
                        }
                        if let Some(permissions) = &snapshot.settings.permissions {
                            this.sync_permission_selection(permissions);
                        }
                        this.apply_agent_event_batch(vec![AgentEvent::ThreadSettingsUpdated(
                            snapshot.settings,
                        )]);
                        cx.emit(ConversationChanged);
                        cx.notify();
                    }
                    failure => {
                        let message = match failure {
                            Ok(Err(error)) => error,
                            _ => crate::i18n::text("线程设置连接在返回前关闭").into(),
                        };
                        if let Some(failure) = crate::agent::config_load_failure(&message) {
                            // The thread cannot be resumed until the user fixes
                            // the configuration; say so once, as a toast, not as a
                            // protocol dump above the input.
                            this.conversation.permission_error = None;
                            this.show_sticky_toast(
                                format!("resume-config-error-{thread_id}"),
                                ToastKind::Danger,
                                config_load_toast_text(&failure),
                                cx,
                            );
                        } else {
                            this.conversation.permission_error =
                                Some(crate::i18n::format!("无法读取当前有效权限：{message}" => "Could not read effective permissions: {message}"));
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn profile_parent(&self, id: &str) -> Option<&str> {
        self.permission_profiles
            .iter()
            .find(|profile| profile.id == id)
            .and_then(|profile| profile.extends.as_deref())
            .or_else(|| {
                self.permission_config
                    .as_ref()?
                    .profile_parents
                    .get(id)
                    .map(String::as_str)
            })
    }
    fn profile_inherits_full(&self, id: &str) -> bool {
        let mut current = id;
        let mut seen = std::collections::HashSet::new();
        while seen.insert(current) {
            if current == ":danger-full-access" {
                return true;
            }
            let Some(parent) = self.profile_parent(current) else {
                return false;
            };
            current = parent;
        }
        false
    }

    fn permission_unavailable_reason(
        &self,
        selection: &crate::agent::AgentPermissionMode,
    ) -> Option<String> {
        use crate::agent::AgentPermissionMode as Mode;
        if self.conversation.thread_id.is_none() && self.is_running() {
            return Some(
                crate::i18n::text("聊天正在启动，请在就绪后更改权限；更改不会影响当前轮次").into(),
            );
        }
        if self.side_chat && !self.side_ready {
            return Some(crate::i18n::text("临时聊天连接已结束").into());
        }
        if self.permission_catalog_loading || self.permission_effective_loading {
            return Some(crate::i18n::text("正在读取权限配置").into());
        }
        if let Some(error) = &self.permission_catalog_error {
            return Some(error.clone());
        }
        let Some(config) = &self.permission_config else {
            return Some(crate::i18n::text("权限配置尚未读取").into());
        };
        if config.generation < self.conversation.runtime.generation {
            return Some(crate::i18n::text("连接已变化，请重新读取权限配置").into());
        }
        let profile = match selection {
            Mode::Request | Mode::Assist => Some(":workspace"),
            Mode::Full => Some(":danger-full-access"),
            Mode::Custom => config
                .effective
                .get("default_permissions")
                .and_then(serde_json::Value::as_str),
            Mode::Profile(id) => Some(id.as_str()),
        };
        if let Some(profile) = profile
            && self
                .permission_profiles
                .iter()
                .find(|entry| entry.id == profile)
                .is_none_or(|entry| !entry.allowed)
        {
            return Some(crate::i18n::text("管理员或服务端不允许使用此权限配置").into());
        }
        let policy = match selection {
            Mode::Request | Mode::Assist => Some("on-request"),
            Mode::Full => Some("never"),
            _ => None,
        };
        let reviewer = match selection {
            Mode::Assist => Some("auto_review"),
            Mode::Request | Mode::Full => Some("user"),
            _ => None,
        };
        policy
            .and_then(|policy| config.restriction("approval_policy", &serde_json::json!(policy)))
            .or_else(|| {
                reviewer.and_then(|reviewer| {
                    config.restriction("approvals_reviewer", &serde_json::json!(reviewer))
                })
            })
    }

    pub(super) fn activate_permission_mode(
        &mut self,
        mode: PermissionMode,
        cx: &mut Context<Self>,
    ) {
        self.activate_permission_selection(mode.agent_mode(), cx);
    }

    pub(super) fn activate_permission_selection(
        &mut self,
        selection: crate::agent::AgentPermissionMode,
        cx: &mut Context<Self>,
    ) {
        if !self.permission_ui_enabled {
            return;
        }
        if let Some(reason) = self.permission_unavailable_reason(&selection) {
            self.conversation.permission_error = Some(reason);
            cx.notify();
            return;
        }
        self.permission_menu_open = false;
        self.permission_menu_keyboard_focus = false;
        let full = matches!(&selection, crate::agent::AgentPermissionMode::Full)
            || matches!(&selection,crate::agent::AgentPermissionMode::Profile(id) if self.profile_inherits_full(id));
        if full && selection != self.selected_agent_permission_mode() {
            self.permission_confirmation_selection = Some(selection);
            cx.emit(RequestFullAccessConfirmation);
        } else {
            self.request_permission_selection(selection, cx);
        }
        cx.notify();
    }

    fn request_permission_selection(
        &mut self,
        selection: crate::agent::AgentPermissionMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(reason) = self.permission_unavailable_reason(&selection) {
            self.conversation.permission_error = Some(reason);
            cx.notify();
            return;
        }
        let Some(thread_id) = self.conversation.thread_id.clone() else {
            self.set_permission_selection(selection.clone());
            cx.emit(super::PermissionSelectionChanged(selection));
            self.conversation.permission_error = None;
            cx.notify();
            return;
        };
        self.permission_update_cycle = self.permission_update_cycle.wrapping_add(1);
        let cycle = self.permission_update_cycle;
        let generation = self
            .permission_config
            .as_ref()
            .map(|config| config.generation);
        let pending = crate::conversation::PermissionChange {
            operation_id: cycle,
            thread_id: thread_id.clone(),
            generation,
            selection: selection.clone(),
        };
        self.conversation.permission_change = Some(pending.clone());
        self.conversation.permission_error = None;
        let receiver =
            self.backend
                .update_thread_permissions(crate::agent::AgentThreadPermissionUpdate {
                    thread_id: thread_id.clone(),
                    cwd: self.conversation.cwd.clone(),
                    mode: selection.clone(),
                    expected_generation: generation,
                    operation_id: cycle,
                });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .recv()
                .await
                .unwrap_or_else(|_| Err(crate::i18n::text("权限设置连接在返回结果前关闭").into()));
            let _ = this.update(cx, |this, cx| {
                if this.permission_update_cycle != cycle
                    || this.conversation.thread_id.as_ref() != Some(&thread_id)
                    || (this.side_chat && !this.side_ready)
                {
                    return;
                }
                if let Ok(result) = &result
                    && (!pending.confirms(result)
                        || result.generation < this.conversation.runtime.generation
                        || this
                            .permission_config
                            .as_ref()
                            .is_some_and(|config| config.generation != result.generation))
                {
                    return;
                }
                this.conversation.permission_change = None;
                let turn_reviewer = result
                    .as_ref()
                    .ok()
                    .and_then(|result| result.active_turn_reviewer.clone());
                let confirmed = result.is_ok();
                this.apply_permission_update_result(
                    selection,
                    result.map(|result| result.settings),
                );
                if confirmed {
                    cx.emit(super::PermissionSelectionChanged(this.permission_selection.clone()));
                }
                // The thread settings did change; only the running turn kept
                // its reviewer, which is worth saying rather than hiding.
                if let Some(crate::agent::AgentActiveTurnReviewerUpdate::Failed {
                    message, ..
                }) = turn_reviewer
                {
                    this.conversation.permission_error = Some(crate::i18n::format!(
                        "权限已更新，但当前轮次的复核者未能更新：{message}；将从下一轮起生效" =>
                        "Permissions updated, but the running turn kept its reviewer: {message}. The change applies from the next turn"
                    ));
                }
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn sync_permission_selection(
        &mut self,
        permissions: &crate::agent::AgentEffectivePermissions,
    ) {
        use crate::agent::AgentPermissionMode as Mode;
        let selection = match permissions
            .active_permission_profile
            .as_ref()
            .map(|profile| profile.id.as_str())
        {
            Some(":workspace")
                if permissions.approval_policy == serde_json::json!("on-request") =>
            {
                if matches!(
                    permissions.approvals_reviewer.as_str(),
                    "auto_review" | "guardian_subagent"
                ) {
                    Mode::Assist
                } else {
                    Mode::Request
                }
            }
            Some(":danger-full-access")
                if permissions.approval_policy == serde_json::json!("never") =>
            {
                Mode::Full
            }
            Some(id) => Mode::Profile(id.into()),
            None => Mode::Custom,
        };
        self.permission_selection = selection;
        let full = permissions
            .active_permission_profile
            .as_ref()
            .is_some_and(|profile| {
                self.profile_inherits_full(&profile.id)
                    || profile
                        .extends
                        .as_deref()
                        .is_some_and(|parent| self.profile_inherits_full(parent))
            })
            || permissions
                .sandbox_policy
                .as_ref()
                .and_then(|sandbox| sandbox.get("type"))
                .and_then(serde_json::Value::as_str)
                == Some("dangerFullAccess");
        self.permission_mode = Self::permission_presentation(
            Some(&permissions.approval_policy),
            &permissions.approvals_reviewer,
            full,
        );
    }

    fn permission_presentation(
        approval_policy: Option<&serde_json::Value>,
        reviewer: &str,
        full: bool,
    ) -> PermissionMode {
        if full && approval_policy.and_then(serde_json::Value::as_str) == Some("never") {
            PermissionMode::Full
        } else if matches!(reviewer, "auto_review" | "guardian_subagent") {
            PermissionMode::Assist
        } else {
            PermissionMode::Request
        }
    }

    fn configured_permission_mode(&self, profile: Option<&str>) -> PermissionMode {
        let Some(config) = &self.permission_config else {
            return PermissionMode::Request;
        };
        let profile = profile.or_else(|| {
            config
                .effective
                .get("default_permissions")
                .and_then(serde_json::Value::as_str)
        });
        let full = profile.map_or_else(
            || {
                config
                    .effective
                    .get("sandbox_mode")
                    .and_then(serde_json::Value::as_str)
                    == Some("danger-full-access")
            },
            |id| self.profile_inherits_full(id),
        );
        Self::permission_presentation(
            config.effective.get("approval_policy"),
            config
                .effective
                .get("approvals_reviewer")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("user"),
            full,
        )
    }

    pub(crate) fn set_permission_selection(
        &mut self,
        selection: crate::agent::AgentPermissionMode,
    ) {
        use crate::agent::AgentPermissionMode as Mode;
        self.permission_mode = match &selection {
            Mode::Request => PermissionMode::Request,
            Mode::Assist => PermissionMode::Assist,
            Mode::Full => PermissionMode::Full,
            Mode::Custom => self.configured_permission_mode(None),
            Mode::Profile(id) => self.configured_permission_mode(Some(id)),
        };
        self.permission_selection = selection;
    }

    pub(super) fn apply_permission_update_result(
        &mut self,
        selection: crate::agent::AgentPermissionMode,
        result: Result<crate::agent::AgentThreadSettings, String>,
    ) {
        match result {
            Ok(settings) => {
                self.set_permission_selection(selection);
                if let Some(permissions) = &settings.permissions {
                    self.sync_permission_selection(permissions);
                }
                self.apply_agent_event_batch(vec![AgentEvent::ThreadSettingsUpdated(settings)]);
            }
            Err(error) => {
                let message = crate::i18n::format!("无法更新权限模式：{error}" => "Could not update permission mode: {error}");
                self.conversation.permission_error = Some(message.clone());
                self.conversation
                    .activities
                    .push(ConversationActivity::Error { message });
            }
        }
    }

    pub(super) fn handle_permission_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.permission_ui_enabled {
            return;
        }
        let key = event.keystroke.key.as_str();
        if !self.permission_menu_open {
            match key {
                "enter" | "space" | "down" | "up" => {
                    self.menu_open = false;
                    self.model_list_open = false;
                    self.permission_menu_open = true;
                    self.permission_menu_keyboard_focus = matches!(key, "down" | "up");
                    self.permission_menu_focused_item = if key == "up" {
                        self.permission_menu_count() - 1
                    } else {
                        0
                    };
                    window.focus(&self.permission_menu_focus, cx);
                }
                _ => {
                    cx.propagate();
                    return;
                }
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }

        match key {
            "down" => {
                self.permission_menu_focused_item = if self.permission_menu_keyboard_focus {
                    (self.permission_menu_focused_item + 1) % self.permission_menu_count()
                } else {
                    0
                };
                self.permission_menu_keyboard_focus = true;
            }
            "up" => {
                self.permission_menu_focused_item = if self.permission_menu_keyboard_focus {
                    (self.permission_menu_focused_item + self.permission_menu_count() - 1)
                        % self.permission_menu_count()
                } else {
                    self.permission_menu_count() - 1
                };
                self.permission_menu_keyboard_focus = true;
            }
            "home" => {
                self.permission_menu_focused_item = 0;
                self.permission_menu_keyboard_focus = true;
            }
            "end" => {
                self.permission_menu_focused_item = self.permission_menu_count() - 1;
                self.permission_menu_keyboard_focus = true;
            }
            "enter" | "space" if self.permission_menu_keyboard_focus => {
                if let Some(mode) = PermissionMode::MENU
                    .get(self.permission_menu_focused_item)
                    .copied()
                {
                    self.activate_permission_mode(mode, cx);
                }
                cx.stop_propagation();
                return;
            }
            "escape" => {
                self.permission_menu_open = false;
                self.permission_menu_keyboard_focus = false;
            }
            "tab" => {
                self.permission_menu_open = false;
                self.permission_menu_keyboard_focus = false;
                if event.keystroke.modifiers.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                cx.stop_propagation();
                cx.notify();
                return;
            }
            _ => {
                cx.propagate();
                return;
            }
        }
        self.permission_menu_scroll
            .scroll_to_item(self.permission_menu_focused_item);
        cx.stop_propagation();
        cx.notify();
    }
    pub(crate) fn cancel_full_access_confirmation(&mut self) {
        self.permission_confirmation_selection = None;
    }
    pub(crate) fn focus_permission_control(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.permission_menu_open = false;
        self.permission_menu_focus.focus(window, cx);
        cx.notify();
    }
    pub fn confirm_full_access(&mut self, cx: &mut Context<Self>) {
        let selection = self
            .permission_confirmation_selection
            .take()
            .unwrap_or(crate::agent::AgentPermissionMode::Full);
        self.request_permission_selection(selection, cx);
        self.permission_menu_open = false;
        self.permission_menu_keyboard_focus = false;
        cx.notify();
    }
    #[cfg(test)]
    pub(super) fn permission_mode_name(&self) -> &'static str {
        match self.permission_mode {
            PermissionMode::Request => "request",
            PermissionMode::Assist => "assist",
            PermissionMode::Full => "full",
        }
    }
    pub(super) fn permission_label(&self) -> (SharedString, &'static str, gpui::Rgba) {
        let theme = Theme::for_mode(self.mode);
        if self.permission_catalog_loading || self.permission_effective_loading {
            return (
                crate::i18n::text("读取权限中…").into(),
                "permission-custom",
                theme.text_tertiary,
            );
        }
        if self.conversation.permission_change.is_some() {
            return (
                crate::i18n::text("设置权限中…").into(),
                "permission-custom",
                theme.text_tertiary,
            );
        }
        let (label, icon, color) = match self.permission_mode {
            PermissionMode::Request => (
                crate::i18n::text("请求批准"),
                "permission-request",
                theme.text_tertiary,
            ),
            PermissionMode::Assist => (
                crate::i18n::text("帮我批准"),
                "permission-assist",
                theme.text_tertiary,
            ),
            PermissionMode::Full => (crate::i18n::text("完全访问"), "permission", theme.warning),
        };
        (label.into(), icon, color)
    }
    /// The menu lists exactly the three permission modes.
    fn permission_menu_count(&self) -> usize {
        PermissionMode::MENU.len()
    }
    pub(super) fn permission_row(
        &self,
        index: usize,
        option: PermissionOption,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let PermissionOption {
            mode,
            title,
            detail,
            glyph,
        } = option;
        let selected = self.permission_mode == mode;
        let disabled_reason = self.permission_unavailable_reason(&mode.agent_mode());
        let enabled = disabled_reason.is_none();
        let warning = mode == PermissionMode::Full;
        let color = if warning {
            theme.warning
        } else {
            theme.markdown_text
        };
        let keyboard_focused =
            self.permission_menu_keyboard_focus && self.permission_menu_focused_item == index;
        let hover_group: SharedString = format!("permission-menu-row-{index}").into();
        div()
            .id(("permission-menu-item", index))
            .role(gpui::Role::MenuItem)
            .aria_label(title)
            .aria_selected(selected)
            .when_some(disabled_reason.clone(), |row, reason| {
                row.aria_description(reason)
            })
            .when(keyboard_focused, |row| row.aria_active_descendant())
            .group(hover_group.clone())
            // CDP (ChatGPT 26.1007): 5px block padding around a 13/18.57
            // title and a 12/14 description, so each row is 42.57px tall.
            .min_h(px(PERMISSION_ROW_HEIGHT))
            .when(!enabled, |row| row.opacity(0.45))
            .px(px(8.0))
            .py(px(5.0))
            // `rounded-xl` resolves to 15px in this build.
            .rounded(px(15.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .when(keyboard_focused, |row| row.bg(theme.sidebar_hover))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.activate_permission_mode(mode, cx);
            }))
            // The `icon-sm` glyph is an 18×18 box that starts 7px into the
            // row; the text column follows 5px later, 30px from the row edge.
            .child(
                icon(glyph, color.into())
                    .flex_none()
                    .size(px(18.0))
                    .ml(px(-1.0))
                    .opacity(if keyboard_focused { 1.0 } else { 0.75 })
                    .group_hover(hover_group.clone(), |glyph| glyph.opacity(1.0)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    // Grow from the text's own width: a zero flex basis would
                    // give the shrink-to-fit menu no width to size from.
                    .flex_grow(1.0)
                    .flex()
                    .flex_col()
                    .ml(px(5.0))
                    .whitespace_nowrap()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(PERMISSION_TITLE_LINE_HEIGHT))
                            .text_color(color)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(14.0))
                            .text_color(if warning {
                                theme.warning
                            } else {
                                theme.text_tertiary
                            })
                            .child(disabled_reason.clone().unwrap_or_else(|| detail.into())),
                    ),
            )
            .when(selected, |row| {
                row.child(
                    icon("permission-check", color.into())
                        // `icon-xs` is a 16×16 layout box, 6px after the text.
                        .flex_none()
                        .size(px(16.0))
                        .ml(px(6.0))
                        .opacity(if keyboard_focused { 1.0 } else { 0.75 })
                        .group_hover(hover_group, |glyph| glyph.opacity(1.0)),
                )
            })
    }
    pub(super) fn permission_menu(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let surface = if self.mode == ThemeMode::Light {
            // The real light popup is a 90%-opaque white surface over the
            // white application background, so its captured pixel is white.
            rgba(0xffffffff)
        } else {
            theme.model_picker_surface
        };
        div()
            .id("permission-menu")
            .role(gpui::Role::Menu)
            .aria_label(crate::i18n::text("权限配置"))
            .absolute()
            .left(px(42.0))
            // CDP: the menu's bottom edge is 1.5px above the 28px trigger.
            .bottom(px(36.5))
            // The reference sizes the menu to its widest row (`min-width:
            // max-content`), so no description wraps.
            .max_h(px(400.0))
            .overflow_y_scroll()
            .track_scroll(&self.permission_menu_scroll)
            .p(px(4.0))
            // `rounded-2xl` resolves to 20px in this build.
            .rounded(px(20.0))
            .bg(surface)
            .shadow(vec![
                // ChatGPT uses two 0.5px rings. Keeping them as shadows is
                // important: CSS rings do not consume the row's one-pixel
                // layout budget the way a native border would.
                BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.12))
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font(ui_font())
            .text_size(px(13.0))
            .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
            .text_color(theme.text)
            .track_focus(&self.permission_menu_focus)
            .on_key_down(cx.listener(Self::handle_permission_menu_key))
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .child(self.permission_row(
                0,
                PermissionOption {
                    mode: PermissionMode::Request,
                    title: crate::i18n::text("请求批准"),
                    detail: crate::i18n::text("编辑外部文件和使用互联网时始终询问"),
                    glyph: "permission-request",
                },
                theme,
                cx,
            ))
            .child(self.permission_row(
                1,
                PermissionOption {
                    mode: PermissionMode::Assist,
                    title: crate::i18n::text("帮我批准"),
                    detail: crate::i18n::text("仅对检测到的风险操作请求批准"),
                    glyph: "permission-assist",
                },
                theme,
                cx,
            ))
            .child(self.permission_row(
                2,
                PermissionOption {
                    mode: PermissionMode::Full,
                    title: crate::i18n::text("完全访问权限"),
                    detail: crate::i18n::text("可不受限制地访问互联网和你电脑上的任何文件"),
                    glyph: "permission",
                },
                theme,
                cx,
            ))
    }
}

#[derive(Clone, Copy)]
pub(super) struct PermissionOption {
    pub(super) mode: PermissionMode,
    pub(super) title: &'static str,
    pub(super) detail: &'static str,
    pub(super) glyph: &'static str,
}
