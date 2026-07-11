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
    pub(super) fn next_local_id(&self, prefix: &str) -> String {
        loop {
            // candidate uses an ownership-independent UUID so snapshot replacement and clock rollback cannot repeat a prior sequence.
            // candidate 使用与所有权无关的 UUID，确保快照替换与时钟回拨不会重复既有序列。
            let candidate = format!("{prefix}-{}", uuid::Uuid::new_v4());
            // identifier_is_occupied checks both Core namespaces because this helper serves messages and records.
            // identifier_is_occupied 同时检查两个 Core 命名空间，因为该辅助函数同时服务消息与记录。
            let identifier_is_occupied = self
                .snapshot
                .messages
                .iter()
                .any(|message| message.id == candidate)
                || self
                    .snapshot
                    .records
                    .iter()
                    .any(|record| record.id == candidate);
            if !identifier_is_occupied {
                return candidate;
            }
        }
    }

    /// Validates one caller-supplied durable record identifier before append.
    /// 在追加前校验一个调用方提供的持久化记录标识。
    ///
    /// # Parameters
    /// 参数
    /// `record_id` is the exact externally owned identifier to register.
    /// `record_id` 是需要注册的外部所有精确标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns success when the identifier is canonical and unused, otherwise an invalid-state error.
    /// 标识规范且未占用时返回成功，否则返回无效状态错误。
    pub(super) fn validate_new_record_id(&self, record_id: &str) -> Result<(), CoreError> {
        if record_id.trim().is_empty() {
            return Err(CoreError::InvalidState {
                message: String::from("new conversation record has an empty id"),
            });
        }
        if record_id.trim() != record_id {
            return Err(CoreError::InvalidState {
                message: format!(
                    "new conversation record has surrounding whitespace in id {record_id:?}"
                ),
            });
        }
        if self
            .snapshot
            .records
            .iter()
            .any(|record| record.id == record_id)
        {
            return Err(CoreError::InvalidState {
                message: format!("new conversation record duplicates id {record_id:?}"),
            });
        }
        Ok(())
    }
}
