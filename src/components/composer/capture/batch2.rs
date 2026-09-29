//! Deterministic `/memories` and find-in-chat conversation states. The
//! fixtures write the conversation state directly and never call the backend.
//! Texts match the reference captures in artifacts/batch2-*.

use super::super::{dialogs::ComposerDialog, toast::ToastKind};
use super::{ComposerView, ConversationChanged};
use crate::{agent::AgentTurnIdentity, conversation::ConversationPhase};
use gpui::Context;

const THREAD: &str = "capture-batch2";
const TURN: &str = "capture-batch2-turn";
const GENERATION: u64 = 1_000_000;

/// The reference thread the find captures search ("Say hello to user").
pub(crate) const FIND_PROMPT: &str = "hello";
pub(crate) const FIND_REPLY: &str = "Hello! I'm here and ready to help with the Echora project (or anything else you have in mind).\n\nWhat would you like to work on?";

impl ComposerView {
    fn begin_batch2_capture(&mut self, started: bool) {
        self.permission_catalog_cycle = self.permission_catalog_cycle.wrapping_add(1);
        self.permission_read_cycle = self.permission_read_cycle.wrapping_add(1);
        self.permission_catalog_loading = false;
        self.permission_effective_loading = false;
        self.memories_feature = Some((self.conversation.runtime.generation, true));
        if !started {
            return;
        }
        self.conversation.thread_id = Some(THREAD.into());
        self.conversation.queue.reset(Some(THREAD.into()));
        self.conversation.goal.reset(Some(THREAD.into()));
        self.conversation.memory.reset(Some(THREAD.into()));
        self.conversation.begin_prompt(FIND_PROMPT);
        self.conversation.turn_id = Some(TURN.into());
        self.conversation.user_message_time = None;
        self.conversation.turn_identity = Some(AgentTurnIdentity {
            generation: GENERATION,
            thread_id: THREAD.into(),
            turn_id: TURN.into(),
        });
        self.conversation.assistant_message = FIND_REPLY.into();
        self.conversation.phase = ConversationPhase::Complete;
    }

    /// `slash` (`/mem`), `dialog-new` (before the chat starts), `dialog-started`
    /// (Use memories fixed), `dialog-generate-off` (generation turned off),
    /// `rollback` (a failed change: the danger toast).
    pub fn set_memories_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.begin_batch2_capture(state != "slash" && state != "dialog-new");
        match state {
            "slash" => {
                self.prompt_editor.update(cx, |editor, cx| {
                    let len = editor.text().len();
                    editor.replace_range(0..len, "/mem", cx);
                });
                self.update_slash_menu(cx);
            }
            "rollback" => self.show_toast(
                ToastKind::Danger,
                crate::i18n::format!("无法更新聊天记忆设置" => "Unable to update chat memory settings"),
                cx,
            ),
            _ => {
                if state == "dialog-generate-off" {
                    self.conversation.memory.generate_memories = Some(false);
                }
                self.dialog = Some(ComposerDialog::Memories);
            }
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    /// The completed "hello" turn the find captures search.
    pub fn set_find_conversation_for_capture(&mut self, cx: &mut Context<Self>) {
        self.begin_batch2_capture(true);
        cx.emit(ConversationChanged);
        cx.notify();
    }
}
