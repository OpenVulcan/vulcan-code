use super::*;

/// Completion state for one logical conversation turn.
/// 单个逻辑会话回合的完成状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationTurnCompletionStatus {
    /// The model response finished, but child subagents still belong to the same user turn.
    /// 模型响应已经结束，但子代理仍属于同一个用户回合。
    AwaitingSubagents,
    /// The full logical user turn is complete and can be submitted to completed-turn consumers.
    /// 完整逻辑用户回合已经结束，可以提交给完成回合消费方。
    Completed,
}

impl ConversationTurnCompletionStatus {
    /// Returns the stable payload value used by hook consumers.
    /// 返回 hook 消费方使用的稳定载荷值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingSubagents => "awaiting_subagents",
            Self::Completed => "completed",
        }
    }

    /// Returns whether this state closes the active logical turn.
    /// 返回该状态是否会闭合活动逻辑回合。
    pub const fn is_final(self) -> bool {
        matches!(self, Self::Completed)
    }
}

/// One compact timeline item emitted to conversation hook consumers.
/// 发送给会话 hook 消费方的一条精简 timeline 项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationTimelineItem {
    /// Stable role used by memory and audit consumers.
    /// 供记忆与审计消费方使用的稳定角色。
    pub role: String,
    /// Stable event kind inside the logical turn.
    /// 逻辑回合内部的稳定事件类型。
    pub kind: String,
    /// Filtered text content suitable for extension consumption.
    /// 适合扩展消费的过滤后文本内容。
    pub content: String,
    /// Optional source id such as request id or tool call id.
    /// 可选来源标识，例如请求标识或工具调用标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

/// Host-visible VMM writeback effect returned by hook handlers.
/// hook 处理器返回给宿主可见的 VMM 写回效果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmmWritebackHookEffect {
    /// Previously queued outbox entries were flushed successfully.
    /// 先前排队的 outbox 条目已成功冲刷。
    Flushed {
        /// Local outbox entry ids that should be removed.
        /// 应被移除的本地 outbox 条目标识。
        entry_ids: Vec<String>,
    },
    /// One PostAction request should be retained for later replay.
    /// 一条 PostAction 请求应保留用于后续重放。
    Enqueue {
        /// Original VMM PostAction request.
        /// 原始 VMM PostAction 请求。
        request: PostActionRequest,
        /// Failure message that caused the enqueue.
        /// 导致入队的失败消息。
        error: String,
    },
    /// One queued PostAction request failed during replay.
    /// 一条排队的 PostAction 请求在重放时失败。
    Failed {
        /// Local outbox entry id that should be updated.
        /// 应被更新的本地 outbox 条目标识。
        entry_id: String,
        /// Failure message observed during replay.
        /// 重放期间观察到的失败消息。
        error: String,
    },
}

/// Host-visible VMM profile sync effect returned by hook handlers.
/// hook 处理器返回给宿主可见的 VMM 画像同步效果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmProfileSyncHookEffect {
    /// Updated profile sync state for the owning session.
    /// 拥有会话的已更新画像同步状态。
    pub state: VmmProfileSyncState,
}

/// Typed VMM recall batch effect transported through hook metadata.
/// 通过 hook metadata 传输的类型化 VMM 召回批次效果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmmRecallBatchHookEffect {
    /// Authoritative structured recall batch returned by the built-in VMM hook.
    /// 内置 VMM hook 返回的权威结构化召回批次。
    pub batch: vulcan_vmm::VmmRecallBatch,
}

impl ConversationTimelineItem {
    /// Builds one timeline item.
    /// 构建一条 timeline 项。
    pub(super) fn new(
        role: impl Into<String>,
        kind: impl Into<String>,
        content: impl Into<String>,
        source_id: Option<String>,
    ) -> Self {
        Self {
            role: role.into(),
            kind: kind.into(),
            content: content.into(),
            source_id,
        }
    }
}

/// Extracts VMM profile sync effects from one hook dispatch report.
/// 从一次 hook 调度报告中提取 VMM 画像同步效果。
pub fn vmm_profile_sync_effect_from_report(
    report: &HookDispatchReport,
) -> Option<VmmProfileSyncHookEffect> {
    report
        .metadata
        .get(HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT)
        .into_iter()
        .flat_map(|values| values.iter())
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .next_back()
}

/// Extracts the latest typed VMM recall batch from one hook dispatch report.
/// 从一次 hook 调度报告中提取最近一批类型化 VMM 召回结果。
///
/// # Parameters
/// 参数
/// `report` is the aggregated hook report whose metadata preserves hook ordering.
/// `report` 是 metadata 保留 hook 顺序的聚合 hook 报告。
///
/// # Returns
/// 返回值
/// The unique typed VMM recall effect, absence, or an explicit malformed/ambiguous metadata error.
/// 返回唯一类型化 VMM 召回效果、缺失结果，或明确的畸形/歧义 metadata 错误。
pub fn vmm_recall_batch_effect_from_report(
    report: &HookDispatchReport,
) -> Result<Option<VmmRecallBatchHookEffect>, String> {
    let Some(values) = report
        .metadata
        .get(super::HOOK_METADATA_VMM_RECALL_BATCH_EFFECT)
    else {
        return Ok(None);
    };
    let [value] = values.as_slice() else {
        return Err(format!(
            "VMM recall hook metadata requires exactly one typed effect, found {}.",
            values.len()
        ));
    };
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|error| format!("Invalid typed VMM recall hook metadata: {error}"))
}

/// In-memory state for one logical user-to-assistant turn.
/// 单个用户到助手逻辑回合的内存状态。
#[derive(Debug, Clone)]
pub(super) struct ActiveConversationTurn {
    /// Stable turn id generated by the workbench runtime.
    /// 工作台运行时生成的稳定回合标识。
    pub(super) turn_id: String,
    /// First visible user content that opened the turn.
    /// 打开该回合的首条可见用户内容。
    pub(super) user_content: String,
    /// Timeline items collected before the final assistant answer.
    /// 最终助手回答前收集到的 timeline 项。
    pub(super) timeline: Vec<ConversationTimelineItem>,
    /// Prompt entry ids that were already recorded when queued during this turn.
    /// 在该回合排队时已经记录过的提示词条目标识集合。
    pub(super) recorded_prompt_entry_ids: std::collections::BTreeSet<String>,
}

impl ActiveConversationTurn {
    /// Builds one active turn from the first user prompt.
    /// 根据首条用户提示构建一个活动回合。
    pub(super) fn new(turn_id: impl Into<String>, user_content: impl Into<String>) -> Self {
        Self {
            turn_id: turn_id.into(),
            user_content: user_content.into(),
            timeline: Vec::new(),
            recorded_prompt_entry_ids: std::collections::BTreeSet::new(),
        }
    }
}

#[cfg(test)]
mod recall_effect_tests {
    use super::*;

    /// Builds one empty Hook report whose metadata can be populated by strict extraction tests.
    /// 构建一份空 Hook 报告，供严格提取测试填充 metadata。
    fn empty_report() -> HookDispatchReport {
        HookDispatchReport::empty(HookPayload { value: json!({}) })
    }

    /// Verifies malformed reserved metadata is rejected instead of being mistaken for no recall.
    /// 验证畸形保留 metadata 会被拒绝，而不会误判为无召回。
    #[test]
    fn malformed_recall_metadata_is_rejected() {
        let mut report = empty_report();
        report.metadata.insert(
            HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_string(),
            vec![json!({"unexpected": true})],
        );

        let error = vmm_recall_batch_effect_from_report(&report)
            .expect_err("malformed reserved metadata must fail closed");

        assert!(error.contains("Invalid typed VMM recall hook metadata"));
    }

    /// Verifies duplicate reserved metadata is rejected as ambiguous ownership.
    /// 验证重复保留 metadata 会因归属歧义而被拒绝。
    #[test]
    fn duplicate_recall_metadata_is_rejected() {
        let mut report = empty_report();
        report.metadata.insert(
            HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_string(),
            vec![json!({"batch": {}}), json!({"batch": {}})],
        );

        let error = vmm_recall_batch_effect_from_report(&report)
            .expect_err("duplicate reserved metadata must fail closed");

        assert!(error.contains("exactly one typed effect"));
    }
}
