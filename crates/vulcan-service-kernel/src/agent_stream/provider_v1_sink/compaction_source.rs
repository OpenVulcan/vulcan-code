//! Frozen V1 summary sources and exact per-attempt transport selection.
//! 冻结的 V1 摘要来源与每次尝试的精确传输选择。

use super::*;
use vulcan_context::SessionMessageEntry;
use vulcan_context_compaction::{CompactionInputPruneMode, PreparedCompactionSource};

/// Mutually exclusive source authority for the next physical compaction attempt.
/// 下一次物理压缩尝试互斥的来源权威。
#[derive(Clone)]
enum CompactionSourceSelection {
    /// No runtime source has been admitted yet.
    /// 尚未准入运行时来源。
    Unprepared,
    /// Local summarization replays the full frozen history.
    /// 本地摘要重放完整冻结历史。
    LocalFull,
    /// Local summarization uses exact selected text.
    /// 本地摘要使用精确选定文字。
    LocalPruned(Box<SessionConversationState>),
}

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
    /// Exact mutually exclusive representation admitted for this attempt.
    /// 本次尝试准入的精确互斥表示。
    selection: CompactionSourceSelection,
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
            selection: CompactionSourceSelection::Unprepared,
        })
    }

    /// Accepts trusted runtime `source` in chronological order under `mode`; returns an identity error on drift.
    /// 按`mode`接纳可信运行时`source`的时序选择；身份漂移时返回错误。
    pub(super) fn select(
        &mut self,
        source: &PreparedCompactionSource,
        mode: CompactionInputPruneMode,
    ) -> Result<(), SessionRuntimeError> {
        if mode == CompactionInputPruneMode::None {
            // Full mode uses canonical runtime rows, including normalized historical tools.
            // 完整模式使用规范运行时行，包括规范化后的历史工具。
            let state = vulcan_context::conversation_state_from_session_messages(&self.messages)
                .map_err(provider_projection_runtime_error)?;
            // Derive the same conversion used by the full-fidelity runtime.
            // 派生完整保真运行时使用的同一转换。
            let expected = vulcan_context::conversation_state_to_compaction_source_messages(&state)
                .map_err(provider_projection_runtime_error)?;
            if serde_json::to_value(&expected).map_err(provider_projection_runtime_error)?
                != serde_json::to_value(&source.source_messages)
                    .map_err(provider_projection_runtime_error)?
            {
                return Err(provider_projection_runtime_error(
                    "Full compaction source changed.",
                ));
            }
            self.selection = CompactionSourceSelection::LocalFull;
            return Ok(());
        }
        // Monotonic identity consumption rejects duplicates and reordered source rows.
        // 单调消费身份，拒绝重复与乱序来源行。
        let mut next_index = 0;
        for selected in &source.source_messages {
            // Resolve only inside the remaining chronological suffix.
            // 仅在剩余时序后缀内解析。
            let offset = self.messages[next_index..]
                .iter()
                .position(|original| original.id == selected.id)
                .ok_or_else(|| {
                    provider_projection_runtime_error(
                        "Compaction source identities are unknown, repeated, or out of order.",
                    )
                })?;
            // The original typed row owns the admitted role.
            // 原类型化行持有准入角色。
            let original = &self.messages[next_index + offset];
            if original.kind != selected.kind || original.record_kind != selected.record_kind {
                return Err(provider_projection_runtime_error(
                    "Compaction source role changed.",
                ));
            }
            next_index += offset + 1;
        }
        // The runtime's mode chooses the representation even when its pruning report records no reduction.
        // 即使剪枝报告记录未发生删减，仍由运行时模式决定所用表示。
        let pruned_state =
            vulcan_context::conversation_state_from_session_messages(&source.source_messages)
                .map_err(provider_projection_runtime_error)?;
        self.selection = CompactionSourceSelection::LocalPruned(Box::new(pruned_state));
        Ok(())
    }

    /// Borrows the selected local minimal state, returning None for other explicitly selected modes.
    /// 借用选定本地最小状态；其他明确选择的模式返回 None。
    fn pruned_state(&self) -> Option<&SessionConversationState> {
        match &self.selection {
            CompactionSourceSelection::LocalPruned(state) => Some(state.as_ref()),
            _ => None,
        }
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
            (
                "prepared attempt",
                matches!(self.selection, CompactionSourceSelection::Unprepared),
            ),
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
        // A minimal transcript contains textual resource references only; it must not prepare omitted native media.
        // 最小转录仅包含文本资源引用；不能准备已经省略的原生媒体。
        let retained_count = match self.selection {
            CompactionSourceSelection::Unprepared => {
                unreachable!("unprepared source rejected above")
            }
            CompactionSourceSelection::LocalPruned(_) => 0,
            CompactionSourceSelection::LocalFull => self.snapshot.items.len(),
        };
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

    /// Extracts the exact normalized source from `candidate`; returns None for native full-fidelity replay.
    /// 从`candidate`提取精确规范化来源；原生完整保真重放返回 None。
    /// Every selected controller payload must match and have a distinct physical source position.
    /// 每条选定控制器载荷必须匹配，并拥有不同的物理来源位置。
    pub(super) fn pruned_transport(
        &self,
        candidate: &ProviderRequestPreparationCandidate,
    ) -> Result<
        Option<(vulcan_core::types::ConversationSnapshot, Vec<AiMessage>)>,
        SessionRuntimeError,
    > {
        // Absence explicitly selects native full-fidelity replay.
        // 不存在时明确选择原生完整保真重放。
        let Some(state) = self.pruned_state() else {
            return Ok(None);
        };
        // The same canonical payloads serve validation and final request budgeting.
        // 相同规范载荷同时用于校验与最终请求预算。
        let history =
            build_core_conversation_snapshot(state).map_err(provider_projection_runtime_error)?;
        // Collect only physical messages owned by selected sources.
        // 只收集选定来源持有的物理消息。
        let mut messages = Vec::with_capacity(history.messages.len());
        // Require strict order without assuming a synthetic prefix length.
        // 要求严格顺序，不假设合成前缀长度。
        let mut last_position = None;
        for expected in &history.messages {
            // Validate the unique controller identity before resolving its physical position.
            // 解析物理位置之前校验唯一控制器身份。
            let matches = candidate
                .controller_snapshot
                .messages
                .iter()
                .filter(|message| message.id == expected.id)
                .collect::<Vec<_>>();
            if matches.len() != 1
                || serde_json::to_value(matches[0]).map_err(provider_projection_runtime_error)?
                    != serde_json::to_value(expected).map_err(provider_projection_runtime_error)?
            {
                return Err(provider_projection_runtime_error(
                    "Compaction candidate source payload changed.",
                ));
            }
            // This map was built alongside the actual transport conversion.
            // 此映射与实际传输转换同时建立。
            let position = *candidate
                .source_message_positions
                .get(&expected.id)
                .ok_or_else(|| {
                    provider_projection_runtime_error(
                        "Compaction candidate source position is missing.",
                    )
                })?;
            if last_position.is_some_and(|previous| position <= previous) {
                return Err(provider_projection_runtime_error(
                    "Compaction candidate source order changed.",
                ));
            }
            // A missing position is an error, never a neighboring-message fallback.
            // 位置缺失是错误，绝不退回相邻消息。
            let message = candidate
                .exact_request
                .messages
                .get(position)
                .ok_or_else(|| {
                    provider_projection_runtime_error(
                        "Compaction candidate source position is invalid.",
                    )
                })?;
            messages.push(message.clone());
            last_position = Some(position);
        }
        Ok(Some((history, messages)))
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

    /// Installs the runtime-selected `source` and `mode` for the next request; active requests cannot be replaced.
    /// 为下一请求安装运行时选定的`source`及`mode`；不能替换活动请求。
    pub(crate) fn select_compaction_source(
        &mut self,
        source: &PreparedCompactionSource,
        mode: CompactionInputPruneMode,
    ) -> Result<(), String> {
        if self.active.is_some() {
            return Err(String::from("Cannot replace an active compaction attempt."));
        }
        self.compaction_source
            .as_mut()
            .ok_or_else(|| String::from("Compaction source was not bound."))?
            .select(source, mode)
            .map_err(|error| error.to_string())
    }
}

/// Exercises real pruning and physical rebuild using the caller's complete route/budget fixture; returns any production error.
/// 使用调用方完整路线与预算夹具验证真实剪枝及物理重建；返回任何生产错误。
#[cfg(test)]
pub(super) fn verify_compaction_transport_selection(
    mut snapshot: SessionProviderProjectionSnapshot,
    facts: &ProviderV1RequestFacts,
    mut candidate: ProviderRequestPreparationCandidate,
) -> Result<(), SessionRuntimeError> {
    use vulcan_context_compaction::{
        CompactionInputBudget, prepare_compaction_source_for_target_budget,
    };
    use vulcan_session_core::v1::projection::SourcedProjectedItem;

    // Original content is intentionally larger than the summary target and must not reappear after pruning.
    // 原内容有意大于摘要目标，剪枝后不能重新出现。
    snapshot.items = vec![
        ProjectedItem::UserInput {
            text: "omitted-business-body".repeat(2000),
            resource_ids: Vec::new(),
        },
        ProjectedItem::AssistantText {
            text: "selected-summary-source".repeat(300),
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
    let mode = CompactionInputPruneMode::RequiredForWindowDowngrade;
    let prepared = prepare_compaction_source_for_target_budget(
        &messages,
        CompactionInputBudget {
            target_input_budget_tokens: 64,
            safety_reserve_tokens: 0,
        },
        vulcan_context::ContextCompactionPolicy::default(),
        mode,
    );
    assert_eq!(prepared.source_messages.len(), 1);
    assert_eq!(prepared.prune_report.truncated_source_message_count, 1);
    source.select(&prepared, mode)?;
    candidate.controller_snapshot =
        build_core_conversation_snapshot(source.pruned_state().unwrap()).unwrap();
    let expected = candidate.controller_snapshot.messages[0].clone();
    let vulcan_core::types::ConversationPayload::Assistant { text, .. } = &expected.payload else {
        panic!("assistant source");
    };
    candidate.exact_request.messages = vec![
        text_ai_message(MessageRole::System, "pre-gate-top-must-not-leak".into()),
        text_ai_message(MessageRole::Assistant, text.clone()),
        text_ai_message(MessageRole::User, "pre-gate-control-must-not-leak".into()),
    ];
    candidate.source_message_positions = BTreeMap::from([(expected.id.clone(), 1)]);
    let result = rebuild_provider_transport_candidate_with_compaction(
        snapshot.clone(),
        facts,
        candidate.clone(),
        media::PreparedV1UserMedia::default(),
        Some(&source),
    )?;
    let serialized = serde_json::to_string(&result.transport.exact_request.messages).unwrap();
    assert!(serialized.contains("selected-summary-source"));
    assert!(!serialized.contains("omitted-business-body"));
    assert!(!serialized.contains("pre-gate-top-must-not-leak"));
    assert!(!serialized.contains("pre-gate-control-must-not-leak"));
    assert!(serialized.contains("Message truncated for context-compaction input only"));
    assert_eq!(result.transport.controller_snapshot.messages[0], expected);
    assert_eq!(
        result.transport.request_snapshot.history,
        result.transport.controller_snapshot.messages
    );
    assert_eq!(
        result
            .request_budget_evidence
            .request_text_measurement
            .history_message_count,
        6
    );
    assert_eq!(result.projection.through_seq, snapshot.through_seq);
    assert!(result.user_prompt_media.is_empty());

    // Missing correspondence and stale generations are rejected rather than using the complete current history.
    // 缺少对应关系或代次陈旧时拒绝，而不是使用当前完整历史兜底。
    let mut wrong_candidate = candidate.clone();
    wrong_candidate.source_message_positions.clear();
    assert!(source.pruned_transport(&wrong_candidate).is_err());
    wrong_candidate = candidate.clone();
    wrong_candidate.controller_snapshot.messages[0].id = "forged-source".into();
    assert!(source.pruned_transport(&wrong_candidate).is_err());
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
    let mut wrong_source = prepared.clone();
    wrong_source
        .source_messages
        .push(wrong_source.source_messages[0].clone());
    assert!(source.select(&wrong_source, mode).is_err());

    // Full retries retain the frozen native prefix, excluding appended output from an earlier failed summary attempt.
    // 完整重试保留冻结原生前缀，排除更早失败摘要尝试后来追加的输出。
    let mut full = prepared;
    full.source_messages =
        vulcan_context::conversation_state_to_compaction_source_messages(&state).unwrap();
    source.select(&full, CompactionInputPruneMode::None)?;
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
    assert!(full_text.contains("omitted-business-body"));
    assert!(!full_text.contains("failed-summary-output"));
    assert_eq!(full_result.projection.through_seq, Seq::try_new(3).unwrap());

    Ok(())
}
