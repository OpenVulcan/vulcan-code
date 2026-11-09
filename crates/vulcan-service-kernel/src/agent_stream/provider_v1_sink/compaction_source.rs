//! Frozen V1 summary sources and exact per-attempt transport selection.
//! 冻结的 V1 摘要来源与每次尝试的精确传输选择。

use super::*;
use vulcan_context::SessionMessageEntry;

/// Planning authority retained across compaction attempts without incorporating retry output into history.
/// 跨压缩尝试保留的规划权威，不将重试输出混入历史。
#[derive(Clone)]
pub(super) struct CompactionSourceWindow {
    /// Original durable semantic boundary, including exact source ranges and generation.
    /// 原可靠语义边界，包括精确来源范围与代次。
    snapshot: SessionProviderProjectionSnapshot,
    /// Transcript records produced by the same V1 planning conversion.
    /// 同一 V1 规划转换产生的转录记录。
    messages: Vec<SessionMessageEntry>,
    /// Whether the runtime has confirmed the complete frozen source.
    /// 运行时是否已经确认完整冻结来源。
    prepared: bool,
}

impl CompactionSourceWindow {
    /// Binds `state` to a real planning `snapshot`; rejects any semantic drift before execution.
    /// 将`state`绑定到真实规划`snapshot`；执行前拒绝任何语义漂移。
    /// Returns the immutable source owner, without changing session state.
    /// 返回不可变来源所有者，不改变会话状态。
    pub(super) fn capture(
        snapshot: SessionProviderProjectionSnapshot,
        state: &SessionConversationState,
    ) -> Result<Self, SessionRuntimeError> {
        // Read planner-owned records from exact durable source ranges.
        // 从精确可靠来源范围读取规划器记录。
        let messages = snapshot
            .sourced_items
            .iter()
            .filter_map(crate::context_compaction::compaction_message_from_v1_projection)
            .map(|entry| entry.map(|(message, _)| message))
            .collect::<Result<Vec<_>, _>>()
            .map_err(provider_projection_runtime_error)?;
        // Rebuild the planning representation before accepting its authority.
        // 接纳权威之前重建规划表示。
        let expected = vulcan_context::conversation_state_from_session_messages(&messages)
            .map_err(provider_projection_runtime_error)?;
        if serde_json::to_value(&expected).map_err(provider_projection_runtime_error)?
            != serde_json::to_value(state).map_err(provider_projection_runtime_error)?
        {
            return Err(provider_projection_runtime_error(
                "Compaction planning source changed before execution; a new plan is required.",
            ));
        }
        Ok(Self {
            snapshot,
            messages,
            prepared: false,
        })
    }

    /// Confirms the full runtime `source` in chronological order, returning an error on any drift.
    /// 按时间顺序确认完整运行时`source`，任何漂移都会返回错误。
    pub(super) fn select(
        &mut self,
        source: &[SessionMessageEntry],
    ) -> Result<(), SessionRuntimeError> {
        // state is reconstructed with the same normalization used when planning the summary.
        // state 通过规划摘要时使用的同一规范化逻辑重建。
        let state = vulcan_context::conversation_state_from_session_messages(&self.messages)
            .map_err(provider_projection_runtime_error)?;
        // expected is the full proof, including every message identity and its content.
        // expected 是完整证明，包含每条消息的身份及内容。
        let expected = vulcan_context::conversation_state_to_compaction_source_messages(&state)
            .map_err(provider_projection_runtime_error)?;
        if serde_json::to_value(&expected).map_err(provider_projection_runtime_error)?
            != serde_json::to_value(source).map_err(provider_projection_runtime_error)?
        {
            return Err(provider_projection_runtime_error(
                "Full compaction source changed.",
            ));
        }
        self.prepared = true;
        Ok(())
    }

    /// Validates current `snapshot` against the frozen frontier for `facts`, returning only this attempt's durable view.
    /// 根据`facts`以冻结前沿校验当前`snapshot`，返回仅属于本次尝试的可靠视图。
    pub(super) fn bound_snapshot(
        &self,
        mut snapshot: SessionProviderProjectionSnapshot,
        facts: &ProviderV1RequestFacts,
    ) -> Result<SessionProviderProjectionSnapshot, SessionRuntimeError> {
        // Report the violated ownership field without exposing source text; every original exclusion remains mandatory.
        // 报告违反的归属字段而不暴露来源正文；全部原有排除条件继续强制执行。
        for (field, changed) in [
            (
                "request purpose",
                facts.purpose != vulcan_session_core::v1::event::ApiPurposeKind::Compaction,
            ),
            ("prepared attempt", !self.prepared),
            (
                "open turn",
                snapshot.open_turn_id != self.snapshot.open_turn_id,
            ),
            (
                "child execution",
                snapshot.child_business_execution_id != self.snapshot.child_business_execution_id,
            ),
            (
                "history generation",
                snapshot.segment_generation != self.snapshot.segment_generation,
            ),
            (
                "compaction node",
                snapshot.current_compaction_id != self.snapshot.current_compaction_id,
            ),
            (
                "retained boundary",
                snapshot.retained_from_seq != self.snapshot.retained_from_seq,
            ),
            (
                "source count",
                snapshot.sourced_items.len() < self.snapshot.sourced_items.len(),
            ),
            (
                "source content",
                snapshot
                    .sourced_items
                    .iter()
                    .zip(&self.snapshot.sourced_items)
                    .any(|(current, frozen)| {
                        current.source_from_seq != frozen.source_from_seq
                            || current.source_to_seq != frozen.source_to_seq
                            || current.item != frozen.item
                    }),
            ),
        ] {
            if changed {
                return Err(provider_projection_runtime_error(format!(
                    "Compaction source frontier rejected: {field}."
                )));
            }
        }
        // retained_count is the full frozen history; later retry output stays outside the summary.
        // retained_count 是完整冻结历史的长度，后续重试输出不会进入摘要。
        let retained_count = self.snapshot.items.len();
        snapshot.items.truncate(retained_count);
        snapshot.sourced_items.truncate(retained_count);
        snapshot.preparing_user_prompt_media_seqs.retain(|seq| {
            snapshot
                .sourced_items
                .iter()
                .any(|item| item.source_from_seq == *seq && item.source_to_seq == *seq)
        });
        Ok(snapshot)
    }
}

impl ProviderV1StreamSink {
    /// Verifies the bound source belongs to the original child invocation before starting its model stream.
    /// 启动模型流之前，校验已绑定来源属于原子调用。
    /// `turn_id` and `execution_id` come from admitted durable ownership; returns an explicit mismatch without mutation.
    /// `turn_id` 与 `execution_id` 来自已准入可靠归属；失配时明确返回且不修改状态。
    pub(crate) fn require_child_compaction_source_owner(
        &self,
        turn_id: vulcan_session_core::v1::ids::TurnId,
        execution_id: &str,
    ) -> Result<(), String> {
        let source = self
            .compaction_source
            .as_ref()
            .ok_or_else(|| String::from("Child compaction source is not bound."))?;
        if source.snapshot.open_turn_id != Some(turn_id)
            || source.snapshot.child_business_execution_id.as_deref() != Some(execution_id)
        {
            return Err(String::from(
                "Child compaction source execution owner changed.",
            ));
        }
        Ok(())
    }

    /// Freezes the real source for `state` before compaction attempts; returns any planning drift.
    /// 在压缩尝试前冻结`state`的真实来源；返回任何规划漂移。
    pub(crate) fn bind_compaction_source(
        &mut self,
        state: &SessionConversationState,
    ) -> Result<(), String> {
        if self.active.is_some() || self.compaction_source.is_some() {
            return Err(String::from(
                "Compaction source is already bound or a request is active.",
            ));
        }
        // Capture through the unique registered owner before any attempt starts.
        // 任何尝试开始前，通过唯一注册所有者捕获。
        let snapshot = self
            .handle
            .capture_context_compaction_planning_snapshot()
            .map_err(|error| error.to_string())?;
        self.compaction_source = Some(
            CompactionSourceWindow::capture(snapshot, state).map_err(|error| error.to_string())?,
        );
        Ok(())
    }

    /// Confirms the complete runtime `source` for the next request; an active request cannot be replaced.
    /// 为下一请求确认完整运行时`source`，不能替换活动请求。
    pub(crate) fn select_compaction_source(
        &mut self,
        source: &[SessionMessageEntry],
    ) -> Result<(), String> {
        if self.active.is_some() {
            return Err(String::from("Cannot replace an active compaction attempt."));
        }
        self.compaction_source
            .as_mut()
            .ok_or_else(|| String::from("Compaction source was not bound."))?
            .select(source)
            .map_err(|error| error.to_string())
    }
}

/// Exercises full physical replay using the caller's route and budget fixture, returning any production error.
/// 使用调用方的路线与预算夹具验证完整物理重放，返回任何生产错误。
#[cfg(test)]
pub(super) fn verify_compaction_transport_selection(
    mut snapshot: SessionProviderProjectionSnapshot,
    facts: &ProviderV1RequestFacts,
    mut candidate: ProviderRequestPreparationCandidate,
) -> Result<(), SessionRuntimeError> {
    use vulcan_session_core::v1::projection::SourcedProjectedItem;

    // The long early message must survive byte for byte alongside the recent message.
    // 较早的长消息必须与最近消息一同逐字节保留。
    snapshot.items = vec![
        ProjectedItem::UserInput {
            text: "early-business-body".repeat(2000),
            resource_ids: Vec::new(),
        },
        ProjectedItem::AssistantText {
            text: "recent-summary-source".repeat(300),
            reasoning: None,
            tool_calls: Vec::new(),
            provider_source: None,
        },
    ];
    snapshot.sourced_items = snapshot
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| SourcedProjectedItem {
            source_from_seq: Seq::try_new(index as u64 + 1).unwrap(),
            source_to_seq: Seq::try_new(index as u64 + 1).unwrap(),
            user_prompt_parameters: None,
            item: item.clone(),
        })
        .collect();
    snapshot.through_seq = Seq::try_new(2).unwrap();
    let messages = snapshot
        .sourced_items
        .iter()
        .filter_map(crate::context_compaction::compaction_message_from_v1_projection)
        .map(|entry| entry.unwrap().0)
        .collect::<Vec<_>>();
    let state = vulcan_context::conversation_state_from_session_messages(&messages).unwrap();
    let mut source = CompactionSourceWindow::capture(snapshot.clone(), &state)?;
    assert!(source.bound_snapshot(snapshot.clone(), facts).is_err());
    // full contains every frozen source row. An incomplete or duplicated proof must fail.
    // full 包含所有冻结源记录。缺失记录或重复记录的证明必须被拒绝。
    let full = vulcan_context::conversation_state_to_compaction_source_messages(&state).unwrap();
    assert!(source.select(&full[1..]).is_err());
    // duplicated repeats an existing identity within the otherwise complete source.
    // duplicated 在完整来源中重复一个已有身份。
    let mut duplicated = full.clone();
    duplicated.push(full[0].clone());
    assert!(source.select(&duplicated).is_err());
    source.select(&full)?;
    candidate.exact_request.messages = vec![
        text_ai_message(MessageRole::System, "pre-gate-top-must-not-leak".into()),
        text_ai_message(MessageRole::User, "pre-gate-control-must-not-leak".into()),
    ];
    let result = rebuild_provider_transport_candidate_with_compaction(
        snapshot.clone(),
        facts,
        candidate.clone(),
        media::PreparedV1UserMedia::default(),
        Some(&source),
    )?;
    let serialized = serde_json::to_string(&result.transport.exact_request.messages).unwrap();
    for item in &snapshot.items {
        if let ProjectedItem::UserInput { text, .. } | ProjectedItem::AssistantText { text, .. } =
            item
        {
            assert!(serialized.contains(text));
        }
    }
    assert!(!serialized.contains("pre-gate-top-must-not-leak"));
    assert!(!serialized.contains("pre-gate-control-must-not-leak"));
    assert_eq!(
        result.transport.request_snapshot.history,
        result.transport.controller_snapshot.messages
    );
    assert_eq!(
        result
            .request_budget_evidence
            .request_text_measurement
            .history_message_count,
        (snapshot.items.len() + 1) as u64
    );
    assert_eq!(result.projection.through_seq, snapshot.through_seq);
    assert!(result.user_prompt_media.is_empty());

    let mut stale = snapshot.clone();
    stale.segment_generation += 1;
    assert!(source.bound_snapshot(stale, facts).is_err());
    // Identical source text cannot authorize a different turn or a replacement child execution.
    // 相同来源正文不能授权不同回合或替换后的子执行。
    let mut replaced_turn = snapshot.clone();
    replaced_turn.open_turn_id = Some(vulcan_session_core::v1::ids::TurnId::try_new(9001).unwrap());
    assert!(source.bound_snapshot(replaced_turn, facts).is_err());
    // The same open turn may be restored under a new exact child execution identity.
    // 相同开放回合可能以新的精确子执行身份恢复。
    let mut replaced_execution = snapshot.clone();
    replaced_execution.child_business_execution_id = Some(String::from("replacement-child-run"));
    assert!(source.bound_snapshot(replaced_execution, facts).is_err());

    // Full retries retain the frozen native prefix and exclude output from a failed summary attempt.
    // 完整重试保留冻结的原生前缀，排除失败的摘要尝试所产生的输出。
    source.select(&full)?;
    let mut retry_snapshot = snapshot.clone();
    let retry_item = ProjectedItem::AssistantText {
        text: "failed-summary-output".into(),
        reasoning: None,
        tool_calls: Vec::new(),
        provider_source: None,
    };
    retry_snapshot.items.push(retry_item.clone());
    retry_snapshot.sourced_items.push(SourcedProjectedItem {
        source_from_seq: Seq::try_new(3).unwrap(),
        source_to_seq: Seq::try_new(3).unwrap(),
        user_prompt_parameters: None,
        item: retry_item,
    });
    retry_snapshot.through_seq = Seq::try_new(3).unwrap();
    let full_result = rebuild_provider_transport_candidate_with_compaction(
        retry_snapshot,
        facts,
        candidate.clone(),
        media::PreparedV1UserMedia::default(),
        Some(&source),
    )?;
    let full_text = serde_json::to_string(&full_result.transport.exact_request.messages).unwrap();
    assert_eq!(full_text, serialized);
    assert!(!full_text.contains("failed-summary-output"));
    assert_eq!(full_result.projection.through_seq, Seq::try_new(3).unwrap());

    Ok(())
}
