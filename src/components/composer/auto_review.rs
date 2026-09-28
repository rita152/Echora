//! Approving an auto-review denial from its review block.

use gpui::Context;

use super::{ComposerView, ConversationChanged, toast::ToastKind};
use crate::agent::AgentAutoApprovalReviewKey;

impl ComposerView {
    /// Records the user's approval of one denied review. The conversation
    /// refuses a second click while one is in flight and never resends an
    /// approved review; the answer only lands in the thread that asked.
    pub(crate) fn approve_review(
        &mut self,
        key: AgentAutoApprovalReviewKey,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.conversation.begin_review_approval(&key) else {
            return;
        };
        let receiver = self.backend.approve_auto_review_denial(request);
        cx.emit(ConversationChanged);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::i18n::format!("批准请求在返回前中断" => "The approval ended before it returned"))
            });
            let _ = this.update(cx, |this, cx| {
                let toast = match &result {
                    Ok(()) => (
                        ToastKind::Success,
                        crate::i18n::format!("已记录批准" => "Approval recorded"),
                    ),
                    Err(_) => (
                        ToastKind::Danger,
                        crate::i18n::format!("无法记录自动审核批准" => "Could not record auto-review approval"),
                    ),
                };
                let recorded = result.is_ok();
                if this.conversation.finish_review_approval(&key, result) {
                    if recorded {
                        this.close_denial_menu(cx);
                    }
                    this.show_toast(toast.0, toast.1, cx);
                    cx.emit(ConversationChanged);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
