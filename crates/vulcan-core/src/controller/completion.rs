use super::*;

impl VulcanCoreController {
    /// Appends one final completion event that summarizes the host-owned loop outcome.
    /// 追加一条概括宿主自有循环结果的最终完成事件。
    pub(super) fn push_final_event(
        &mut self,
        provider_turns_executed: usize,
        tool_calls_processed: usize,
        final_assistant_message_id: Option<&str>,
    ) {
        self.push_event(CoreEvent {
            kind: CoreEventKind::Final,
            title: String::from("控制器完成"),
            body: format!(
                "宿主控制器已完成一次完整回合，Provider 回合数为 {}，工具调用数为 {}。",
                provider_turns_executed, tool_calls_processed
            ),
            metadata: Some(json!({
                "provider_turns_executed": provider_turns_executed,
                "tool_calls_processed": tool_calls_processed,
                "final_assistant_message_id": final_assistant_message_id,
            })),
        });
    }

    /// Generates one new local identifier for synthesized protocol entities.
    /// 为合成协议实体生成一个新的本地标识。
    pub(super) fn next_local_id(&mut self, prefix: &str) -> String {
        self.local_id_counter += 1;
        format!(
            "{prefix}-{}-{}",
            current_unix_millis_text(),
            self.local_id_counter
        )
    }
}
