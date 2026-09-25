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

/// One host-validated human input in an exact V1 in-turn continuation batch.
/// 精确 V1 回合内续跑批次中一条经宿主验证的人类输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalInTurnHookInput {
    /// Canonical browser action used for identity across queue admission and later promotion.
    /// 用于队列准入和后续提升之间身份关联的规范浏览器动作。
    pub client_action_id: vulcan_session_core::v1::ClientActionId,
    /// Original session-local input allocation; retries cannot substitute another input.
    /// 原会话本地输入分配；重试不能替换为另一输入。
    pub input_id: vulcan_session_core::v1::PendingInputId,
    /// Provider-visible event position validated against the exact terminal receipt.
    /// 按精确终态回执验证的 Provider 可见事件位置。
    pub input_seq: vulcan_session_core::v1::Seq,
    /// Canonical raw user text; manual-injection syntax is sanitized only for a new Hook item.
    /// 规范原始用户正文；仅为新 Hook 条目清理手动注入语法。
    pub text: String,
}

/// Host-visible VMM writeback effect returned by hook handlers.
/// hook 处理器返回给宿主可见的 VMM 写回效果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmmWritebackHookEffect {
    /// Old unaccepted entries were cancelled after their source changed, not delivered successfully.
    /// 来源变化后取消旧的未接受条目，并非投递成功。
    Cancelled {
        /// Exact local queue IDs to remove without automatic replay, even when the source returns later.
        /// 准确本地队列 ID，移除后不自动重放，即使稍后切回原来源。
        entry_ids: Vec<String>,
        /// Explicit diagnostic, including cancellation before the current request ever entered the queue.
        /// 明确诊断，也包含当前请求尚未入队即被取消的情况。
        reason: String,
    },
    /// Authoritative lifecycle state returned by one successful PostAction request.
    /// 单次成功 PostAction 请求返回的权威生命周期状态。
    SessionStatus {
        /// Exact admission generation that observed this lifecycle result.
        /// 观察该生命周期结果的准确准入代次。
        permit_id: u64,
        /// Authoritative lifecycle state for the owning session.
        /// 所属会话的权威生命周期状态。
        status: vulcan_vmm::SessionMemoryStatus,
    },
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
        /// Exact generation authorized when this delivery began.
        /// 本次投递开始时授权的准确代次。
        permit_id: u64,
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
    /// Source frozen by the executing client, never reconstructed from current settings.
    /// 执行客户端冻结的来源，绝不从当前设置重建。
    pub read_source: vulcan_vmm::VmmMemoryReadSource,
    /// Authoritative structured recall batch returned by the built-in VMM hook.
    /// 内置 VMM hook 返回的权威结构化召回批次。
    pub batch: vulcan_vmm::VmmRecallBatch,
}

/// Typed authoritative VMM session status transported through hook metadata.
/// 通过 hook metadata 传输的类型化 VMM 权威会话状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmSessionStatusHookEffect {
    /// Source whose lifecycle was actually observed, including empty recall responses.
    /// 实际观察生命周期的来源，包括空召回响应。
    pub read_source: vulcan_vmm::VmmMemoryReadSource,
    /// Authoritative lifecycle state returned by VMM.
    /// VMM 返回的权威生命周期状态。
    pub status: vulcan_vmm::SessionMemoryStatus,
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

/// Extracts the sole typed profile effect from a report, returning absence or explicit malformed/duplicate errors.
/// 从报告提取唯一类型化画像效果，返回缺失或明确的畸形/重复错误。
pub fn vmm_profile_sync_effect_from_report(
    report: &HookDispatchReport,
) -> Result<Option<VmmProfileSyncHookEffect>, String> {
    // One profile line owns exactly one typed effect; malformed or duplicate data cannot silently disappear.
    // 一条画像线路准确拥有一个类型化效果；畸形或重复数据不能静默消失。
    let Some(values) = report.metadata.get(HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT) else {
        return Ok(None);
    };
    let [value] = values.as_slice() else {
        return Err(format!(
            "VMM profile hook metadata requires exactly one typed effect, found {}.",
            values.len()
        ));
    };
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|error| format!("Invalid typed VMM profile hook metadata: {error}"))
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
    // Validate decoded source semantics before any native publisher can replace or filter this effect.
    // 在原生发布器可以替换或过滤此效果前校验解码来源语义。
    let effect: VmmRecallBatchHookEffect = serde_json::from_value(value.clone())
        .map_err(|error| format!("Invalid typed VMM recall hook metadata: {error}"))?;
    effect
        .read_source
        .validate()
        .map_err(|error| error.to_string())?;
    Ok(Some(effect))
}

/// Extracts the unique authoritative VMM session status from one hook dispatch report.
/// 从单次 hook 调度报告中提取唯一的 VMM 权威会话状态。
///
/// # Parameters
/// 参数
/// `report` is the aggregated hook report whose metadata preserves hook ordering.
/// `report` 是 metadata 保留 hook 顺序的聚合 hook 报告。
///
/// # Returns
/// 返回值
/// The unique typed status effect, absence, or an explicit malformed/ambiguous metadata error.
/// 返回唯一类型化状态效果、缺失结果，或明确的畸形/歧义 metadata 错误。
pub fn vmm_session_status_effect_from_report(
    report: &HookDispatchReport,
) -> Result<Option<VmmSessionStatusHookEffect>, String> {
    let Some(values) = report
        .metadata
        .get(super::HOOK_METADATA_VMM_SESSION_STATUS_EFFECT)
    else {
        return Ok(None);
    };
    let [value] = values.as_slice() else {
        return Err(format!(
            "VMM session status hook metadata requires exactly one typed effect, found {}.",
            values.len()
        ));
    };
    // An impossible visibility set is a contract error, even if a later publisher can read current lifecycle state.
    // 即使后续发布器可以读取当前生命周期，不可能的可见范围仍属于契约错误。
    let effect: VmmSessionStatusHookEffect = serde_json::from_value(value.clone())
        .map_err(|error| format!("Invalid typed VMM session status hook metadata: {error}"))?;
    effect
        .read_source
        .validate()
        .map_err(|error| error.to_string())?;
    Ok(Some(effect))
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
    /// Whether the VMM service was fully configured and live when this turn was admitted.
    /// 当前回合准入时 VMM 服务是否已完成配置并处于运行就绪状态。
    pub(super) vmm_runtime_ready: bool,
    /// Durable lifecycle policy; agent-originated preparation and completion skip user observers.
    /// 可靠生命周期策略；代理发起的准备和完成跳过用户观察器。
    pub(super) observes_user: bool,
    /// Timeline items collected before the final assistant answer.
    /// 最终助手回答前收集到的 timeline 项。
    pub(super) timeline: Vec<ConversationTimelineItem>,
    /// Prompt entry ids that were already recorded when queued during this turn.
    /// 在该回合排队时已经记录过的提示词条目标识集合。
    pub(super) recorded_prompt_entry_ids: std::collections::BTreeSet<String>,
    /// Durable admission identities projected into this Hook, independent of preparation notifications.
    /// 投影到本 Hook 的可靠准入身份，独立于准备通知。
    pub(super) observed_input_identities: BTreeMap<
        String,
        (
            vulcan_session_core::v1::PendingInputId,
            vulcan_session_core::v1::business_hook::BusinessHookInputKind,
            u64,
        ),
    >,
    /// Exact promoted-input coordinates already projected during this business Hook lifetime.
    /// 本业务 Hook 生命周期中已投影的精确提升输入坐标。
    pub(super) canonical_input_receipts: BTreeMap<
        String,
        (
            vulcan_session_core::v1::PendingInputId,
            vulcan_session_core::v1::Seq,
        ),
    >,
}

impl ActiveConversationTurn {
    /// Builds one active turn from the first user prompt.
    /// 根据首条用户提示构建一个活动回合。
    pub(super) fn new(
        turn_id: impl Into<String>,
        user_content: impl Into<String>,
        vmm_runtime_ready: bool,
    ) -> Self {
        Self {
            turn_id: turn_id.into(),
            user_content: user_content.into(),
            vmm_runtime_ready,
            observes_user: true,
            timeline: Vec::new(),
            recorded_prompt_entry_ids: std::collections::BTreeSet::new(),
            observed_input_identities: BTreeMap::new(),
            canonical_input_receipts: BTreeMap::new(),
        }
    }

    /// Installs the original observation policy while retaining internal execution correlation.
    /// 安装原始观察策略，同时保留内部执行关联。
    pub(super) fn with_user_observation(mut self, observes_user: bool) -> Self {
        self.observes_user = observes_user;
        self
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

    /// Distinguishes a skipped profile line from malformed or ambiguous reserved effects.
    /// 区分跳过画像线路与畸形或有歧义的保留效果。
    #[test]
    fn profile_metadata_requires_one_well_formed_effect() {
        let mut report = empty_report();
        assert!(
            vmm_profile_sync_effect_from_report(&report)
                .expect("absent")
                .is_none()
        );
        report.metadata.insert(
            HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT.to_owned(),
            vec![json!({"bad": true})],
        );
        assert!(
            vmm_profile_sync_effect_from_report(&report)
                .expect_err("malformed")
                .contains("Invalid typed VMM profile")
        );
        report
            .metadata
            .insert(HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT.to_owned(), vec![]);
        assert!(
            vmm_profile_sync_effect_from_report(&report)
                .expect_err("empty reserved field")
                .contains("exactly one")
        );
        let effect = json!(VmmProfileSyncHookEffect {
            state: VmmProfileSyncState::default()
        });
        report.metadata.insert(
            HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT.to_owned(),
            vec![effect.clone(), effect.clone()],
        );
        assert!(
            vmm_profile_sync_effect_from_report(&report)
                .expect_err("duplicate")
                .contains("exactly one")
        );
        report.metadata.insert(
            HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT.to_owned(),
            vec![effect],
        );
        assert!(
            vmm_profile_sync_effect_from_report(&report)
                .expect("unique")
                .is_some()
        );
    }

    /// Rejects structurally decodable but semantically invalid read scopes for recall and lifecycle effects.
    /// 对召回及生命周期效果拒绝结构可解码但语义无效的读取范围。
    /// Returns after both decoders reject an owner missing from its declared visibility.
    /// 两个解码器均拒绝可见范围缺少归属项目后返回。
    #[test]
    fn recall_and_status_metadata_reject_invalid_source_scopes() {
        // Construct the invalid value directly to test the untrusted decoding boundary.
        // 直接构造无效值以测试不可信解码边界。
        let source = vulcan_vmm::VmmMemoryReadSource {
            owner: vulcan_vmm::VmmMemorySource::native(7).expect("owner"),
            visible_project_ids: vec![8],
        };
        // The report deliberately contains two otherwise valid typed effects.
        // 报告刻意包含两个其他部分有效的类型化效果。
        let mut report = empty_report();
        report.metadata.insert(
            HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_owned(),
            vec![json!(VmmRecallBatchHookEffect {
                read_source: source.clone(),
                batch: vulcan_vmm::VmmRecallBatch::default(),
            })],
        );
        report.metadata.insert(
            crate::agent_runtime::conversation_hooks::HOOK_METADATA_VMM_SESSION_STATUS_EFFECT
                .to_owned(),
            vec![json!(VmmSessionStatusHookEffect {
                read_source: source,
                status: vulcan_vmm::SessionMemoryStatus::Active,
            })],
        );
        assert!(
            vmm_recall_batch_effect_from_report(&report)
                .expect_err("recall scope")
                .contains("invalid backend-specific visibility scope")
        );
        assert!(
            vmm_session_status_effect_from_report(&report)
                .expect_err("status scope")
                .contains("invalid backend-specific visibility scope")
        );
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
