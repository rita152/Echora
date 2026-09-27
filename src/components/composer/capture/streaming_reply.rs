//! A timed assistant reply for capturing the paced reveal and word fade.
use super::{ComposerView, ConversationChanged, progress::CaptureInterrupt};
use gpui::Context;

const CAPTURE_REPLY_ITEM: &str = "capture-streaming-reply";

const CAPTURE_REPLY: &str = "## 流式输出验收\n\n这段回复通过归约器按定时突发喂入，用来核对逐词淡入与节奏揭示是否与 ChatGPT 应用一致。Streaming text should appear at a steady pace, and every newly revealed word fades in over 0.7 s instead of landing at once.\n\n- 列表项作为整体先淡入，再由其中的词逐个淡入\n- 行内代码 `render_streaming_assistant_markdown` 与链接 [GPUI](https://github.com/zed-industries/zed) 各自作为一个整体淡入\n- 完成快照到达后，剩余缓冲文本立即显示\n\n| 阶段 | 时长 |\n|---|---|\n| 词淡入 | 0.7 s |\n| 块淡入 | 0.15 s |\n\n最后一段用于观察表格之后的段落是否继续按节奏显示。";

/// Token burst sizes and gaps, cycled over the reply to mimic a live model.
const BURST_CHARS: [usize; 6] = [4, 9, 3, 12, 6, 8];
const BURST_GAP_MS: [u64; 4] = [80, 120, 60, 150];

impl ComposerView {
    pub fn set_streaming_reply_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        use crate::agent::AgentEvent as E;
        self.conversation.begin_prompt("展示流式输出效果。");
        self.conversation.user_message_time = None;
        self.conversation.apply_agent_event_batch(vec![
            E::Started,
            E::AssistantMessageStarted {
                item_id: CAPTURE_REPLY_ITEM.into(),
                phase: Some("final_answer".into()),
            },
        ]);
        let complete = state == "completed";
        let (sender, receiver) = async_channel::unbounded();
        self.conversation.active_turn = Some(crate::agent::AgentInterruptHandle::new(
            std::sync::Arc::new(CaptureInterrupt(sender.clone())),
        ));
        let cycle = self.conversation.cycle;
        cx.spawn(async move |this, cx| {
            while let Ok(event) = receiver.recv().await {
                let terminal = matches!(&event, E::Completed | E::Interrupted | E::Failed(_));
                let _ = this.update(cx, |this, cx| {
                    if this.conversation.cycle == cycle {
                        this.conversation.apply_agent_event_batch(vec![event]);
                        this.conversation.assistant_message_time = None;
                        cx.emit(ConversationChanged);
                        cx.notify();
                    }
                });
                if terminal {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |_, cx| {
            let characters = CAPTURE_REPLY.chars().collect::<Vec<_>>();
            let mut offset = 0;
            let mut burst = 0;
            while offset < characters.len() {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(
                        BURST_GAP_MS[burst % BURST_GAP_MS.len()],
                    ))
                    .await;
                let end = (offset + BURST_CHARS[burst % BURST_CHARS.len()]).min(characters.len());
                let delta = characters[offset..end].iter().collect::<String>();
                offset = end;
                burst += 1;
                if sender
                    .send(E::TextDelta {
                        item_id: CAPTURE_REPLY_ITEM.into(),
                        delta,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            if complete {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
                eprintln!("streaming reply capture: completion snapshot");
                let _ = sender
                    .send(E::AssistantMessageCompleted {
                        item_id: CAPTURE_REPLY_ITEM.into(),
                        text: CAPTURE_REPLY.into(),
                        phase: Some("final_answer".into()),
                    })
                    .await;
                let _ = sender.send(E::Completed).await;
            }
        })
        .detach();
        self.conversation.assistant_message_time = None;
        cx.emit(ConversationChanged);
        cx.notify();
    }
}
