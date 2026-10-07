//! Canonical conversation compaction rewrite helpers shared by WebUI state and persistence flows.
//! 供WebUI状态与持久化流程共享的规范会话压缩改写辅助模块。

use serde_json::{Value, json};
use vulcan_ai_contract::{
    canonicalize_codex_responses_replay_items, is_replayable_codex_responses_item,
};
use vulcan_context::protocol::validate_session_conversation_message_identities;
use vulcan_context::{
    ContextCompactionPolicy, SessionConversationMessage, SessionConversationPayload,
    SessionConversationRecord, SessionConversationState, SessionMessageKind,
    SessionMessageRecordKind, conversation_state_to_compaction_source_messages,
};

use crate::planning::{
    CompactionPlanInput, CompactionResult, CompactionSummaryRecord, plan_context_compaction,
};
use crate::preview::preview_tool_output;

/// Maximum body length retained for old historical records after record-level compaction.
/// 记录级压缩后为旧历史记录保留的最大正文长度。
const PROTOCOL_RECORD_COMPACTED_BODY_CHARS: usize = 12 * 1024;

/// Function pointer used to compact one `change_set` host result into its minimal transcript-safe form.
/// 用于将一份 `change_set` 宿主结果压缩为最小可转录安全形态的函数指针。
pub type ChangeSetCompactor = fn(&Value) -> Option<Value>;

/// Audit report returned by exact record compaction after a canonical rewrite.
/// 规范改写后精确记录压缩返回的审计报告。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConversationRecordCompactionReport {
    /// Record count before compaction.
    /// 压缩前记录数量。
    pub original_record_count: usize,
    /// Records retained verbatim by exact identity.
    /// 按精确身份原样保留的记录数量。
    pub full_record_count: usize,
    /// Historical dialogue or structured records compacted by existing rules.
    /// 按现有规则紧凑化的历史对话或结构化记录数量。
    pub compacted_historical_record_count: usize,
    /// Records removed because no retained identity or historical rule required them.
    /// 因没有保留身份或历史规则而删除的记录数量。
    pub deleted_record_count: usize,
    /// Record count after compaction.
    /// 压缩后记录数量。
    pub final_record_count: usize,
    /// Retained tool identities that did not match any record identity.
    /// 未匹配任何记录身份的保留工具身份数量。
    pub unmatched_retained_tool_identity_count: usize,
}

/// Message-index segment representing one API round plus its following tool responses.
/// 表示一个 API 执行轮次及其后续工具响应的消息索引段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanonicalTailSegment {
    /// Inclusive start index of the retained segment.
    /// 保留段的起始索引，包含该位置。
    start: usize,
    /// Exclusive end index of the retained segment.
    /// 保留段的结束索引，不包含该位置。
    end: usize,
}

/// Rewrites the active canonical window using one compaction summary plus the retained recent tail.
/// 使用一条压缩摘要与保留的最近尾部消息改写当前活动规范窗口。
///
/// # Returns
/// 返回值
/// Returns success after one validated atomic rewrite, or an identity error with messages and continuation unchanged.
/// 完成一次经过校验的原子改写后返回成功；身份错误时消息与 continuation 保持不变。
pub fn compact_conversation_with_summary(
    state: &mut SessionConversationState,
    summary_message_id: impl Into<String>,
    summary_kind: SessionMessageKind,
    summary_body: impl Into<String>,
    updated_at: impl Into<String>,
    policy: ContextCompactionPolicy,
) -> Result<(), String> {
    compact_conversation_with_legacy_inputs(
        state,
        summary_message_id.into(),
        summary_kind,
        summary_body.into(),
        updated_at.into(),
        None,
        policy,
    )
}

/// Rewrites the active canonical window using the planner's exact covered-source boundary.
/// 使用规划器给出的精确源覆盖边界改写当前活动规范窗口。
///
/// # Parameters
/// # 参数
/// `state` is the canonical conversation window to rewrite.
/// `state` 是需要改写的规范会话窗口。
/// `summary_message_id` is the hidden summary message identifier inserted as the new context anchor.
/// `summary_message_id` 是作为新上下文锚点插入的隐藏摘要消息标识。
/// `summary_kind` controls whether the synthetic summary is projected as user or assistant text.
/// `summary_kind` 控制合成摘要按用户文本还是助手文本投影。
/// `summary_body` is the durable continuation summary.
/// `summary_body` 是持久续写摘要。
/// `updated_at` is the continuation timestamp written after the rewrite.
/// `updated_at` 是改写后写入 continuation 的时间戳。
/// `source_until_message_id` is the inclusive source boundary covered by the summary.
/// `source_until_message_id` 是摘要覆盖的闭区间源边界。
/// `policy` is the fallback policy used when the boundary is absent or stale.
/// `policy` 是边界缺失或过期时使用的回退策略。
///
/// # Returns
/// 返回值
/// Returns success after one validated atomic rewrite, or an identity error with messages and continuation unchanged.
/// 完成一次经过校验的原子改写后返回成功；身份错误时消息与 continuation 保持不变。
pub fn compact_conversation_with_summary_until(
    state: &mut SessionConversationState,
    summary_message_id: impl Into<String>,
    summary_kind: SessionMessageKind,
    summary_body: impl Into<String>,
    updated_at: impl Into<String>,
    source_until_message_id: Option<&str>,
    policy: ContextCompactionPolicy,
) -> Result<(), String> {
    compact_conversation_with_legacy_inputs(
        state,
        summary_message_id.into(),
        summary_kind,
        summary_body.into(),
        updated_at.into(),
        source_until_message_id,
        policy,
    )
}

/// Routes compatibility rewrite inputs through the shared planner and frozen-result apply path.
/// 将兼容改写输入路由到共享规划器与冻结结果应用路径。
///
/// # Parameters
/// # 参数
/// `state` is the canonical mutable conversation, the summary fields describe the replacement anchor,
/// `source_until_message_id` optionally freezes an explicit source boundary, and `policy` is the exact policy seed.
/// `state` 是规范可变会话，摘要字段描述替换锚点，`source_until_message_id` 可选冻结显式来源边界，
/// `policy` 是精确策略种子。
///
/// # Returns
/// # 返回值
/// Success after the compatibility inputs are planned and applied through the frozen-result path,
/// or an explicit planning/rewrite error with no partial rewrite.
/// 兼容输入经冻结结果路径完成规划与应用后返回成功；否则返回显式规划或 rewrite 错误且不产生部分改写。
fn compact_conversation_with_legacy_inputs(
    state: &mut SessionConversationState,
    summary_message_id: String,
    summary_kind: SessionMessageKind,
    summary_body: String,
    updated_at: String,
    source_until_message_id: Option<&str>,
    policy: ContextCompactionPolicy,
) -> Result<(), String> {
    // Plan from the same isolated host selection that the summary and rewrite will consume.
    // 从摘要与改写共用的同一隔离宿主选择规划。
    let mut candidate = vulcan_context::runtime_host::prepare_runtime_host_compaction(state)?;
    let source_messages = conversation_state_to_compaction_source_messages(&candidate)?;
    let plan = plan_context_compaction(CompactionPlanInput {
        session_label: "legacy-rewrite",
        messages: &source_messages,
        all_messages: &source_messages,
        context_start_message_id: None,
        source_until_message_id,
        policy,
    })
    .ok_or_else(|| String::from("Compatibility compaction inputs did not produce a safe plan."))?;
    let result = CompactionResult {
        summary_record: CompactionSummaryRecord {
            kind: summary_kind,
            record_kind: SessionMessageRecordKind::CompactionSummary,
            title: String::from("Context Continuation Prompt"),
            body: summary_body.clone(),
            preview: summary_body,
        },
        source_until_message_id: plan.source_until_message_id.clone(),
        source_until_turn_index: plan.source_until_turn_index,
        retained_tail_start_turn_index: plan.retained_tail_start_turn_index,
        compaction_id: plan.compaction_id.clone(),
        created_at: updated_at,
        policy: plan.policy,
        tail_selection: plan.tail_selection,
        provider_context_items: Vec::new(),
        provider_context_route: None,
        provider_context_usage: None,
        provider_context_response_id: None,
    };
    compact_conversation_with_compaction_result(&mut candidate, summary_message_id, &result)?;
    *state = candidate;
    Ok(())
}

/// Atomically rewrites one canonical conversation from a frozen compaction result.
/// 根据冻结压缩结果原子改写一份规范会话。
///
/// # Parameters
/// # 参数
/// `state` is the current canonical conversation candidate, `summary_message_id` is the new anchor,
/// and `result` owns the exact source boundary, retained message IDs, policy, and summary body.
/// `state` 是当前规范会话候选，`summary_message_id` 是新锚点，`result` 持有精确来源边界、
/// 保留消息 ID、策略与摘要正文。
///
/// # Returns
/// # 返回值
/// Success after one validated replacement, or an explicit stale-plan error with state unchanged.
/// 完成一次校验后替换时返回成功；计划失效时返回显式错误且状态保持不变。
pub fn compact_conversation_with_compaction_result(
    state: &mut SessionConversationState,
    summary_message_id: impl Into<String>,
    result: &CompactionResult,
) -> Result<(), String> {
    validate_session_conversation_message_identities(&state.messages)
        .map_err(|error| format!("Invalid source conversation message identity: {error}"))?;
    validate_tail_selection_policy(result)?;

    // retainedIds are compared against the current suffix before any candidate mutation occurs.
    // retainedIds 在任何候选变更发生前与当前后缀比较。
    let retained_ids = &result.tail_selection.retained_message_ids;
    if retained_ids.len() > state.messages.len() {
        return Err(String::from(
            "Frozen retained message IDs are longer than the current canonical conversation.",
        ));
    }
    let retained_start = state.messages.len().saturating_sub(retained_ids.len());
    let current_suffix_ids = state.messages[retained_start..]
        .iter()
        .map(|message| message.id.as_str())
        .collect::<Vec<_>>();
    let frozen_suffix_ids = retained_ids.iter().map(String::as_str).collect::<Vec<_>>();
    if current_suffix_ids != frozen_suffix_ids {
        return Err(String::from(
            "Frozen retained message IDs are no longer the current canonical suffix.",
        ));
    }
    let expected_source_boundary = retained_start
        .checked_sub(1)
        .and_then(|index| state.messages.get(index))
        .map(|message| message.id.as_str());
    if expected_source_boundary != result.source_until_message_id.as_deref() {
        return Err(String::from(
            "Frozen compaction source boundary is no longer adjacent to the retained suffix.",
        ));
    }

    // retainedTail is a clone so every validation failure leaves the active state unchanged.
    // retainedTail 使用克隆，确保任一校验失败都不会改变活动状态。
    let mut retained_tail = state.messages[retained_start..].to_vec();
    let segments = canonical_api_round_segments(&retained_tail);
    let newest_segment_start = segments.last().map(|segment| segment.start).unwrap_or(0);
    let mut previewed_tool_output_count = 0usize;
    let mut omitted_tool_output_token_estimate = 0usize;
    for (index, message) in retained_tail.iter_mut().enumerate() {
        if index >= newest_segment_start {
            continue;
        }
        if let SessionConversationPayload::ToolResult { text, .. } = &mut message.payload {
            let preview = preview_tool_output(text);
            if preview.previewed {
                previewed_tool_output_count = previewed_tool_output_count.saturating_add(1);
                omitted_tool_output_token_estimate = omitted_tool_output_token_estimate
                    .saturating_add(preview.omitted_token_estimate);
                *text = preview.text;
            }
        }
    }
    if previewed_tool_output_count != result.tail_selection.previewed_tool_output_count
        || omitted_tool_output_token_estimate
            != result.tail_selection.omitted_tool_output_token_estimate
    {
        return Err(format!(
            "Frozen historical tool-output preview statistics no longer match canonical messages (expected_count={}, actual_count={}, expected_omitted_tokens={}, actual_omitted_tokens={}).",
            result.tail_selection.previewed_tool_output_count,
            previewed_tool_output_count,
            result.tail_selection.omitted_tool_output_token_estimate,
            omitted_tool_output_token_estimate,
        ));
    }

    let summary_message = summary_conversation_message(
        summary_message_id.into(),
        result.summary_record.kind,
        result.summary_record.body.clone(),
    );
    let mut compacted_messages = Vec::with_capacity(retained_tail.len() + 1);
    compacted_messages.push(summary_message);
    compacted_messages.extend(retained_tail);
    validate_session_conversation_message_identities(&compacted_messages)
        .map_err(|error| format!("Invalid compacted conversation message identity: {error}"))?;

    // Preserve still-active exact host instructions and mark removed events only in an atomic candidate.
    // 在原子候选中保留仍有效的精确宿主指令，并标记已移出的事件。
    let mut candidate =
        vulcan_context::runtime_host::rebuild_runtime_host_compaction(state, compacted_messages)?;
    // continuation is reset only after every frozen-boundary and identity validation succeeds.
    // continuation 仅在全部冻结边界与身份校验成功后重置。
    candidate.continuation = Default::default();
    candidate.continuation.updated_at = Some(result.created_at.clone());
    *state = candidate;
    Ok(())
}

/// Validates that one frozen tail report belongs to the policy carried by the same result.
/// 校验一份冻结尾部报告属于同一结果携带的策略。
///
/// # Parameters
/// # 参数
/// `result` carries both the frozen adaptive policy and the immutable tail selection report.
/// `result` 同时携带冻结自适应策略与不可变尾部选择报告。
///
/// # Returns
/// # 返回值
/// Success when all policy fields and retained identities are self-consistent; otherwise an explicit error.
/// 全部策略字段与保留身份自洽时返回成功；否则返回显式错误。
fn validate_tail_selection_policy(result: &CompactionResult) -> Result<(), String> {
    let selection = &result.tail_selection;
    if selection.context_limit != result.policy.tail_context_limit
        || selection.policy_source != result.policy.tail_policy_source
        || selection.nominal_token_budget != result.policy.preserve_recent_tokens
        || selection.segment_limit != result.policy.tail_turns
    {
        return Err(String::from(
            "Frozen tail selection does not match the compaction policy.",
        ));
    }
    let unique_ids = selection
        .retained_message_ids
        .iter()
        .collect::<std::collections::BTreeSet<_>>();
    if unique_ids.len() != selection.retained_message_ids.len() {
        return Err(String::from(
            "Frozen retained message IDs contain duplicates.",
        ));
    }
    Ok(())
}

/// Inserts route-bound opaque Responses compaction context immediately after its summary anchor.
/// 将绑定路由的不透明 Responses 压缩上下文紧接其摘要锚点插入。
///
/// # Parameters
/// 参数
/// `state` is the rewritten canonical conversation, `summary_message_id` identifies its new anchor,
/// and `result` carries the validated provider items plus their exact route identity.
/// `state` 是已改写的规范会话，`summary_message_id` 标识新锚点，`result` 携带已校验的
/// 供应商项及其精确路由身份。
///
/// # Returns
/// 返回值
/// Success when local compaction needs no provider envelope or after one atomic validated insertion;
/// otherwise an explicit invariant error with the conversation unchanged.
/// 本地压缩无需供应商封装时，或完成一次原子校验插入后返回成功；否则返回显式约束错误且会话保持不变。
pub fn append_remote_responses_compaction_context(
    state: &mut SessionConversationState,
    summary_message_id: &str,
    result: &CompactionResult,
) -> Result<(), String> {
    if result.provider_context_items.is_empty() {
        if result.provider_context_route.is_none() {
            return Ok(());
        }
        return Err(String::from(
            "Remote Responses compaction route exists without provider context items.",
        ));
    }
    // route is mandatory because opaque provider items cannot be replayed across accounts or endpoints.
    // route 是必需项，因为不透明供应商项不得跨账号或端点重放。
    let route = result.provider_context_route.as_ref().ok_or_else(|| {
        String::from("Remote Responses compaction items are missing their route identity.")
    })?;
    if [
        route.provider_id.as_str(),
        route.connection_id.as_str(),
        route.route_fingerprint.as_str(),
        route.protocol_profile_id.as_str(),
        route.model_id.as_str(),
    ]
    .iter()
    .any(|value| value.trim().is_empty())
    {
        return Err(String::from(
            "Remote Responses compaction route identity contains an empty field.",
        ));
    }
    // canonicalItems mirrors the official Codex legacy alias before persistence validation.
    // canonicalItems 在持久化校验前镜像官方 Codex 旧别名。
    let mut canonical_items = result.provider_context_items.clone();
    canonicalize_codex_responses_replay_items(&mut canonical_items);
    let contains_compaction = canonical_items
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("compaction"));
    if !contains_compaction
        || !canonical_items
            .iter()
            .all(is_replayable_codex_responses_item)
    {
        return Err(String::from(
            "Remote Responses compaction context contains an invalid ordered replayable item.",
        ));
    }
    // summaryIndex proves the opaque checkpoint is attached to the exact new anchor.
    // summaryIndex 证明不透明检查点附着于精确的新锚点。
    let summary_index = state
        .messages
        .iter()
        .position(|message| message.id == summary_message_id)
        .ok_or_else(|| {
            String::from(
                "Remote Responses compaction summary anchor is missing from the conversation.",
            )
        })?;
    // candidateMessages preserves atomicity when anchor normalization or identity validation fails.
    // candidateMessages 在锚点规范化或身份校验失败时保持原子性。
    let mut candidate_messages = state.messages.clone();
    // anchorPayload must be a synthetic assistant message with no pre-existing provider-visible state.
    // anchorPayload 必须是尚未携带供应商可见状态的合成助手消息。
    let anchor_payload = &mut candidate_messages[summary_index].payload;
    let SessionConversationPayload::Assistant {
        reasoning: anchor_reasoning,
        provider_message_id: anchor_provider_message_id,
        provider_message_state: anchor_provider_state,
        usage: anchor_usage,
        tool_calls: anchor_tool_calls,
        ..
    } = anchor_payload
    else {
        return Err(String::from(
            "Remote Responses compaction summary anchor must be an assistant message.",
        ));
    };
    if !anchor_reasoning.is_empty()
        || anchor_provider_message_id.is_some()
        || anchor_provider_state.is_some()
        || anchor_usage.is_some()
        || !anchor_tool_calls.is_empty()
    {
        return Err(String::from(
            "Remote Responses compaction summary anchor contains unexpected provider-visible state.",
        ));
    }
    // anchorProviderState co-locates the checkpoint with its anchor so route-aware projection can suppress diagnostics.
    // anchorProviderState 将检查点与锚点共置，使路由感知投影能够抑制诊断文本。
    *anchor_provider_state = Some(json!({
        "vulcan_ai": {
            "responses_compaction": {
                "provider_id": route.provider_id,
                "connection_id": route.connection_id,
                "route_fingerprint": route.route_fingerprint,
                "protocol_profile_id": route.protocol_profile_id,
                "model_id": route.model_id,
                "items": canonical_items,
                "usage": result.provider_context_usage,
                "response_id": result.provider_context_response_id,
            }
        }
    }));
    validate_session_conversation_message_identities(&candidate_messages).map_err(|error| {
        format!("Invalid remote Responses compaction message identity: {error}")
    })?;
    state.messages = candidate_messages;
    Ok(())
}

/// Builds the synthetic canonical summary message inserted by one compaction rewrite.
/// 构建一次压缩改写插入的合成规范摘要消息。
fn summary_conversation_message(
    id: String,
    summary_kind: SessionMessageKind,
    summary_text: String,
) -> SessionConversationMessage {
    SessionConversationMessage {
        id,
        payload: match summary_kind {
            SessionMessageKind::User => SessionConversationPayload::UserText {
                text: summary_text,
                attachments: Vec::new(),
            },
            SessionMessageKind::Assistant | SessionMessageKind::Error => {
                SessionConversationPayload::Assistant {
                    text: summary_text,
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                }
            }
        },
    }
}

/// Compacts canonical records after summary creation so old tool details stop inflating persistence.
/// 在摘要创建后压缩规范记录，避免旧工具细节继续放大持久化体积。
///
/// # Parameters
/// # 参数
/// `state` is the rewritten canonical conversation, `summary_record_id` identifies the new anchor,
/// `policy` controls historical record pruning, and `compact_change_set_host_result` preserves change-set metadata.
/// `state` 是已改写规范会话，`summary_record_id` 标识新锚点，`policy` 控制历史记录剪枝，
/// `compact_change_set_host_result` 保护 change-set 元数据。
///
/// # Returns
/// # 返回值
/// Exact keep/compact/delete statistics from the shared identity-based record compactor.
/// 返回共享身份式记录压缩器产出的精确保留、紧凑化与删除统计。
pub fn compact_conversation_records_after_summary(
    state: &mut SessionConversationState,
    summary_record_id: &str,
    policy: ContextCompactionPolicy,
    compact_change_set_host_result: ChangeSetCompactor,
) -> ConversationRecordCompactionReport {
    compact_conversation_records_after_summary_with_retained_ids(
        state,
        summary_record_id,
        &[],
        policy,
        compact_change_set_host_result,
    )
}

/// Compacts records using retained canonical/tool identities and explicit transaction record IDs.
/// 使用保留的规范/工具身份与显式事务记录 ID 压缩记录。
///
/// # Parameters
/// # 参数
/// `state` owns canonical messages and records after rewrite, `summary_record_id` identifies the anchor,
/// `additional_retained_record_ids` names status or continuation records from the same transaction,
/// `policy` controls historical body pruning, and `compact_change_set_host_result` preserves change-set metadata.
/// `state` 持有改写后的规范消息与记录，`summary_record_id` 标识锚点，
/// `additional_retained_record_ids` 指定同一事务的状态或 continuation 记录，`policy` 控制历史正文剪枝，
/// `compact_change_set_host_result` 保护 change-set 元数据。
///
/// # Returns
/// # 返回值
/// Exact keep/compact/delete counts and unmatched retained tool identities.
/// 返回精确的保留、紧凑化、删除数量与未匹配工具身份数量。
pub fn compact_conversation_records_after_summary_with_retained_ids(
    state: &mut SessionConversationState,
    summary_record_id: &str,
    additional_retained_record_ids: &[String],
    policy: ContextCompactionPolicy,
    compact_change_set_host_result: ChangeSetCompactor,
) -> ConversationRecordCompactionReport {
    let original_record_count = state.records.len();
    // retainedCanonicalIds preserves every canonical message in the frozen rewritten suffix.
    // retainedCanonicalIds 保留冻结改写后缀中的每条规范消息。
    let retained_canonical_ids = state
        .messages
        .iter()
        .map(|message| message.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut retained_local_tool_ids = std::collections::BTreeSet::new();
    let mut retained_tool_call_ids = std::collections::BTreeSet::new();
    for message in &state.messages {
        match &message.payload {
            SessionConversationPayload::Assistant { tool_calls, .. } => {
                for tool_call in tool_calls {
                    if !tool_call.id.trim().is_empty() {
                        retained_local_tool_ids.insert(tool_call.id.clone());
                    }
                    if let Some(tool_call_id) = tool_call
                        .tool_call_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        retained_tool_call_ids.insert(tool_call_id.to_string());
                    }
                }
            }
            SessionConversationPayload::ToolResult { tool_call_id, .. } => {
                if let Some(tool_call_id) = tool_call_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    retained_tool_call_ids.insert(tool_call_id.to_string());
                }
            }
            SessionConversationPayload::UserText { .. }
            | SessionConversationPayload::AgentMessage { .. } => {}
        }
    }

    let mut full_record_ids = retained_canonical_ids;
    full_record_ids.insert(summary_record_id.to_string());
    full_record_ids.extend(additional_retained_record_ids.iter().cloned());
    let mut matched_local_tool_ids = std::collections::BTreeSet::new();
    let mut matched_tool_call_ids = std::collections::BTreeSet::new();
    for record in &state.records {
        // Runtime host records own replay identities even when their messages leave the active window.
        // 即使消息离开活动窗口，运行时宿主记录仍持有重放身份。
        // Their producer reserves this ID namespace and rejects missing metadata; display pruning must not erase it.
        // 生产者独占此 ID 命名空间并拒绝缺失元数据；展示记录剪枝不得清除该账本。
        if record
            .id
            .starts_with(vulcan_context::runtime_host::RUNTIME_HOST_RECORD_PREFIX)
        {
            full_record_ids.insert(record.id.clone());
        }
        if model_switch_display_record_should_survive_compaction(record) {
            // Model-switch audit records remain complete so the renderer can localize their frozen labels after compaction.
            // 模型切换审计记录保持完整，使渲染器在压缩后仍能本地化其冻结标签。
            full_record_ids.insert(record.id.clone());
        }
        if retained_local_tool_ids.contains(&record.id) {
            full_record_ids.insert(record.id.clone());
            matched_local_tool_ids.insert(record.id.clone());
        }
        let metadata_tool_call_id = record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_call_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(tool_call_id) = metadata_tool_call_id
            && retained_tool_call_ids.contains(tool_call_id)
        {
            full_record_ids.insert(record.id.clone());
            matched_tool_call_ids.insert(tool_call_id.to_string());
            if let Some(detail_record_id) = record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("result"))
                .and_then(|result| result.get("detail_ref"))
                .and_then(|detail_ref| detail_ref.get("record_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                full_record_ids.insert(detail_record_id.to_string());
            }
        }
    }

    let mut retained_records = Vec::new();
    let mut retained_ids = std::collections::BTreeSet::new();
    let mut full_record_count = 0usize;
    let mut compacted_historical_record_count = 0usize;
    for record in state.records.iter().cloned() {
        let keep_full = full_record_ids.contains(&record.id);
        let keep_historical_dialogue = historical_record_survives_compaction(&record);
        if !(keep_full || keep_historical_dialogue) {
            continue;
        }
        let retained_record = if keep_full {
            full_record_count = full_record_count.saturating_add(1);
            record
        } else {
            compacted_historical_record_count = compacted_historical_record_count.saturating_add(1);
            compact_historical_record(record, policy.prune, compact_change_set_host_result)
        };
        if retained_ids.insert(retained_record.id.clone()) {
            retained_records.push(retained_record);
        }
    }
    let final_record_count = retained_records.len();
    state.records = retained_records;
    ConversationRecordCompactionReport {
        original_record_count,
        full_record_count,
        compacted_historical_record_count,
        deleted_record_count: original_record_count.saturating_sub(final_record_count),
        final_record_count,
        unmatched_retained_tool_identity_count: retained_local_tool_ids
            .len()
            .saturating_sub(matched_local_tool_ids.len())
            .saturating_add(
                retained_tool_call_ids
                    .len()
                    .saturating_sub(matched_tool_call_ids.len()),
            ),
    }
}

/// Builds safe tail segments at assistant API-round boundaries so tool calls and tool results stay together.
/// 按 Assistant API 执行轮次边界构建安全尾部段，确保工具调用与工具结果保持在同一组内。
///
/// # Parameters
/// # 参数
/// `messages` is the ordered canonical conversation after rewrite candidate construction.
/// `messages` 是 rewrite 候选构建后的有序规范会话。
///
/// # Returns
/// # 返回值
/// Contiguous canonical segments covering every message exactly once.
/// 返回连续规范段，确保每条消息恰好被覆盖一次。
fn canonical_api_round_segments(
    messages: &[SessionConversationMessage],
) -> Vec<CanonicalTailSegment> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut round_starts = Vec::new();
    let mut last_assistant_round_key: Option<String> = None;
    for (index, message) in messages.iter().enumerate() {
        let Some(round_key) = canonical_assistant_round_key(message) else {
            continue;
        };
        if last_assistant_round_key.as_deref() != Some(round_key.as_str()) {
            round_starts.push(index);
            last_assistant_round_key = Some(round_key);
        }
    }

    if round_starts.is_empty() {
        return vec![CanonicalTailSegment {
            start: 0,
            end: messages.len(),
        }];
    }
    if round_starts[0] != 0 {
        // Leading history belongs to the first Assistant/provider segment so rewrite mirrors planning exactly.
        // 首条 Assistant 之前的历史属于第一个 Assistant/Provider 执行段，使 rewrite 与 planning 完全一致。
        round_starts[0] = 0;
    }

    round_starts
        .iter()
        .enumerate()
        .map(|(position, start)| CanonicalTailSegment {
            start: *start,
            end: round_starts
                .get(position + 1)
                .copied()
                .unwrap_or(messages.len()),
        })
        .collect()
}

/// Returns the stable assistant API-round key for one canonical message.
/// 返回一条规范消息的稳定 Assistant API 执行轮次键。
///
/// # Parameters
/// # 参数
/// `message` is one canonical conversation message whose Assistant identity is inspected.
/// `message` 是待检查 Assistant 身份的一条规范会话消息。
///
/// # Returns
/// # 返回值
/// A trimmed provider message identity for Assistant entries, or `None` for non-Assistant/unidentified entries.
/// Assistant 条目返回去除空白的 Provider 消息身份；非 Assistant 或无身份条目返回 `None`。
fn canonical_assistant_round_key(message: &SessionConversationMessage) -> Option<String> {
    let SessionConversationPayload::Assistant {
        provider_message_id,
        ..
    } = &message.payload
    else {
        return None;
    };
    provider_message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Some(message.id.clone()))
}

/// Returns whether one older record should survive record-level compaction.
/// 返回一条旧记录是否应在记录级压缩后保留。
fn historical_record_survives_compaction(record: &SessionConversationRecord) -> bool {
    matches!(record.kind, SessionMessageKind::User)
        || matches!(
            record.record_kind,
            SessionMessageRecordKind::AssistantOutput | SessionMessageRecordKind::CompactionSummary
        )
        || generated_attachment_tool_result_should_survive_compaction(record)
        || change_set_tool_result_should_survive_compaction(record)
}

/// Returns whether one display-only model-switch audit record must remain complete after compaction.
/// 返回一条仅展示的模型切换审计记录是否必须在压缩后保持完整。
///
/// # Parameters
/// 参数
/// `record` is the canonical transcript record being evaluated for record-level compaction.
/// `record` 是正在接受记录级压缩判定的规范转录记录。
///
/// # Returns
/// 返回值
/// Returns true only for the explicit `model_switch` display-note contract.
/// 仅对显式的 `model_switch` 展示说明契约返回 true。
fn model_switch_display_record_should_survive_compaction(
    record: &SessionConversationRecord,
) -> bool {
    matches!(record.record_kind, SessionMessageRecordKind::DisplayNote)
        && record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("notice_kind"))
            .and_then(Value::as_str)
            == Some("model_switch")
}

/// Reduces an old surviving dialogue record to compact text and minimal metadata.
/// 将一条保留下来的旧对话记录降级为紧凑正文与最小元数据。
fn compact_historical_record(
    mut record: SessionConversationRecord,
    prune_tools: bool,
    compact_change_set_host_result: ChangeSetCompactor,
) -> SessionConversationRecord {
    if generated_attachment_tool_result_should_survive_compaction(&record) {
        record.body.clear();
        record.metadata = compact_generated_attachment_tool_result_metadata(record.metadata.take());
        return record;
    }
    if change_set_tool_result_should_survive_compaction(&record) {
        record.body.clear();
        record.metadata = compact_change_set_tool_result_metadata(
            record.metadata.take(),
            compact_change_set_host_result,
        );
        return record;
    }
    record.body = if prune_tools
        && matches!(
            record.record_kind,
            SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
        ) {
        String::from(
            "Tool output was compacted. Re-run or open the original session detail if needed.",
        )
    } else {
        truncate_compacted_record_body(&record.body)
    };
    record.metadata = None;
    record
}

/// Returns whether one canonical tool-result record carries generated-media metadata that must remain transcript-visible after compaction.
/// 返回一条规范工具结果记录是否携带必须在压缩后继续保留为转录可见的生成媒体元数据。
fn generated_attachment_tool_result_should_survive_compaction(
    record: &SessionConversationRecord,
) -> bool {
    matches!(
        record.record_kind,
        SessionMessageRecordKind::ToolResultOutput
    ) && record
        .metadata
        .as_ref()
        .and_then(resolve_generated_attachment_host_result_from_metadata)
        .is_some()
}

/// Returns whether one canonical tool-result record carries `change_set` metadata that must remain transcript-visible after compaction.
/// 返回一条规范工具结果记录是否携带必须在压缩后继续保留为转录可见的 `change_set` 元数据。
fn change_set_tool_result_should_survive_compaction(record: &SessionConversationRecord) -> bool {
    matches!(
        record.record_kind,
        SessionMessageRecordKind::ToolResultOutput
    ) && record
        .metadata
        .as_ref()
        .and_then(resolve_change_set_host_result_from_metadata)
        .is_some()
}

/// Resolves the generated-attachments host-result payload from transcript metadata, accepting nested and flattened shapes.
/// 从转录元数据中解析 generated-attachments 宿主结果载荷，同时兼容嵌套与扁平两种结构。
fn resolve_generated_attachment_host_result_from_metadata(metadata: &Value) -> Option<&Value> {
    let nested_host_result = metadata
        .get("host_result")
        .filter(|value| value.is_object())
        .filter(|value| value.get("kind").and_then(Value::as_str) == Some("generated_attachments"));
    nested_host_result.or_else(|| {
        (metadata.get("kind").and_then(Value::as_str) == Some("generated_attachments")
            && metadata.get("payload").is_some())
        .then_some(metadata)
    })
}

/// Resolves the `change_set` host-result payload from transcript metadata, accepting nested and flattened shapes.
/// 从转录元数据中解析 `change_set` 宿主结果载荷，同时兼容嵌套与扁平两种结构。
fn resolve_change_set_host_result_from_metadata(metadata: &Value) -> Option<&Value> {
    let nested_host_result = metadata
        .get("host_result")
        .filter(|value| value.is_object())
        .filter(|value| value.get("kind").and_then(Value::as_str) == Some("change_set"));
    nested_host_result.or_else(|| {
        (metadata.get("kind").and_then(Value::as_str) == Some("change_set")
            && metadata.get("payload").is_some())
        .then_some(metadata)
    })
}

/// Compacts one generated-media tool-result metadata object down to the minimum host payload required to rebuild inline transcript cards later.
/// 将一份生成媒体工具结果元数据压缩到后续重建内联转录卡片所需的最小宿主载荷。
fn compact_generated_attachment_tool_result_metadata(metadata: Option<Value>) -> Option<Value> {
    let metadata = metadata?;
    let host_result = resolve_generated_attachment_host_result_from_metadata(&metadata)?;
    let compact_host_result = compact_generated_attachment_host_result(host_result)?;
    let mut metadata_object = serde_json::Map::new();
    metadata_object.insert(String::from("host_result"), compact_host_result);
    Some(Value::Object(metadata_object))
}

/// Compacts one `change_set` tool-result metadata object down to the minimum host payload required to rebuild inline change summaries later.
/// 将一份 `change_set` 工具结果元数据压缩到后续重建内联变更摘要所需的最小宿主载荷。
fn compact_change_set_tool_result_metadata(
    metadata: Option<Value>,
    compact_change_set_host_result: ChangeSetCompactor,
) -> Option<Value> {
    let metadata = metadata?;
    let host_result = resolve_change_set_host_result_from_metadata(&metadata)?;
    let compact_host_result = compact_change_set_host_result(host_result)?;
    let mut metadata_object = serde_json::Map::new();
    metadata_object.insert(String::from("host_result"), compact_host_result);
    Some(Value::Object(metadata_object))
}

/// Compacts one generated-attachments host-result payload to the minimal structure required by transcript media rendering and host actions.
/// 将一份 generated-attachments 宿主结果载荷压缩到转录媒体渲染与宿主动作所需的最小结构。
fn compact_generated_attachment_host_result(host_result: &Value) -> Option<Value> {
    let outputs = host_result
        .get("payload")
        .and_then(|payload| payload.get("outputs"))
        .and_then(Value::as_array)?;
    let compact_outputs = outputs
        .iter()
        .filter_map(compact_generated_attachment_output)
        .collect::<Vec<_>>();
    if compact_outputs.is_empty() {
        return None;
    }

    let mut payload_object = serde_json::Map::new();
    payload_object.insert(String::from("outputs"), Value::Array(compact_outputs));

    let mut host_result_object = serde_json::Map::new();
    host_result_object.insert(
        String::from("kind"),
        Value::String(String::from("generated_attachments")),
    );
    host_result_object.insert(String::from("payload"), Value::Object(payload_object));
    Some(Value::Object(host_result_object))
}

/// Compacts one generated attachment output to the fields required by transcript restoration, previews, and file actions.
/// 将单条生成附件输出压缩到转录恢复、预览显示与文件动作所需的字段集合。
fn compact_generated_attachment_output(output: &Value) -> Option<Value> {
    let output_object = output.as_object()?;
    let kind = output_object
        .get("kind")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let file_name = output_object
        .get("file_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let local_path = output_object
        .get("local_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;

    let mut compact_output = serde_json::Map::new();
    compact_output.insert(String::from("kind"), Value::String(kind.to_string()));
    compact_output.insert(
        String::from("file_name"),
        Value::String(file_name.to_string()),
    );
    compact_output.insert(
        String::from("local_path"),
        Value::String(local_path.to_string()),
    );

    if let Some(mime_type) = output_object
        .get("mime_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact_output.insert(
            String::from("mime_type"),
            Value::String(mime_type.to_string()),
        );
    }
    if let Some(byte_len) = output_object.get("byte_len").cloned() {
        compact_output.insert(String::from("byte_len"), byte_len);
    }
    if let Some(source_path) = output_object
        .get("source_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact_output.insert(
            String::from("source_path"),
            Value::String(source_path.to_string()),
        );
    }
    if let Some(preview_local_path) = output_object
        .get("preview_local_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact_output.insert(
            String::from("preview_local_path"),
            Value::String(preview_local_path.to_string()),
        );
    }
    if let Some(asset_id) = output_object
        .get("asset_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact_output.insert(
            String::from("asset_id"),
            Value::String(asset_id.to_string()),
        );
    }

    Some(Value::Object(compact_output))
}

/// Truncates one old record body to keep reconstructed history useful but bounded.
/// 截断旧记录正文，使重建历史保持有用但大小有界。
fn truncate_compacted_record_body(body: &str) -> String {
    if body.chars().count() <= PROTOCOL_RECORD_COMPACTED_BODY_CHARS {
        return body.to_string();
    }

    let truncated = body
        .chars()
        .take(PROTOCOL_RECORD_COMPACTED_BODY_CHARS.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    //! Rewrite regression tests covering retained tool turns and transcript-safe metadata preservation.
    //! 覆盖保留工具轮次与转录安全元数据保留的改写回归测试。

    use super::*;
    use crate::preview::TOOL_OUTPUT_PREVIEW_MARKER_PREFIX;
    use serde_json::json;
    use vulcan_context::SessionConversationToolCall;

    /// Builds one compact `change_set` host-result payload used by rewrite tests.
    /// 构建供改写测试使用的紧凑 `change_set` 宿主结果载荷。
    fn test_compact_change_set_host_result(host_result: &Value) -> Option<Value> {
        let files = host_result
            .get("payload")
            .and_then(|payload| payload.get("files"))
            .and_then(Value::as_array)?;
        let file_count = files.len() as u64;

        Some(json!({
            "kind": "change_set",
            "payload": {
                "stats": {
                    "file_count": file_count
                },
                "verification": {
                    "status": "skipped"
                }
            }
        }))
    }

    /// Builds canonical history plus the exact frozen result used by rewrite consistency tests.
    /// 构建规范历史以及 rewrite 一致性测试使用的精确冻结结果。
    ///
    /// # Parameters
    /// # 参数
    /// `historical_is_error` and `newest_is_error` control the structured states of both tool results.
    /// `historical_is_error` 与 `newest_is_error` 控制两条工具结果的结构化状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns the untouched canonical state, its planner-produced result, and the historical/newest tool bodies.
    /// 返回未改写的规范状态、规划器产出的结果，以及历史/最新工具正文。
    fn frozen_rewrite_fixture_with_tool_errors(
        historical_is_error: bool,
        newest_is_error: bool,
    ) -> (SessionConversationState, CompactionResult, String, String) {
        // historicalToolText is closed by a later Assistant message and therefore eligible for preview.
        // historicalToolText 已被后续 Assistant 消息闭合，因此允许生成预览。
        let historical_tool_text = "历".repeat(3_500);
        // newestToolText belongs to the unconsumed newest segment and must remain complete.
        // newestToolText 属于尚未被消费的最新执行段，必须保持完整。
        let newest_tool_text = "新".repeat(3_500);
        // state contains three provider segments so the oldest can become the summarized source.
        // state 包含三个 Provider 执行段，使最旧一段能够成为摘要来源。
        let state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("assistant-old"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("old completed segment"),
                        reasoning: String::new(),
                        provider_message_id: Some(String::from("provider-old")),
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("assistant-historical"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::new(),
                        reasoning: String::new(),
                        provider_message_id: Some(String::from("provider-historical")),
                        provider_message_state: None,
                        usage: None,
                        tool_calls: vec![SessionConversationToolCall {
                            id: String::from("local-historical"),
                            tool_call_id: Some(String::from("call-historical")),
                            tool_name: String::from("shell_command"),
                            arguments: json!({"command": "historical"}),
                            argument_parse_error: None,
                        }],
                    },
                },
                SessionConversationMessage {
                    id: String::from("tool-historical"),
                    payload: SessionConversationPayload::ToolResult {
                        tool_call_id: Some(String::from("call-historical")),
                        is_error: historical_is_error,
                        text: historical_tool_text.clone(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("assistant-newest"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::new(),
                        reasoning: String::new(),
                        provider_message_id: Some(String::from("provider-newest")),
                        provider_message_state: None,
                        usage: None,
                        tool_calls: vec![SessionConversationToolCall {
                            id: String::from("local-newest"),
                            tool_call_id: Some(String::from("call-newest")),
                            tool_name: String::from("shell_command"),
                            arguments: json!({"command": "newest"}),
                            argument_parse_error: None,
                        }],
                    },
                },
                SessionConversationMessage {
                    id: String::from("tool-newest"),
                    payload: SessionConversationPayload::ToolResult {
                        tool_call_id: Some(String::from("call-newest")),
                        is_error: newest_is_error,
                        text: newest_tool_text.clone(),
                    },
                },
            ],
            records: Vec::new(),
            continuation: vulcan_context::SessionProviderContinuationState {
                last_provider_message_id: Some(String::from("provider-newest")),
                updated_at: Some(String::from("before")),
                ..vulcan_context::SessionProviderContinuationState::default()
            },
        };
        // sourceMessages is the canonical model projection consumed by the only tail selector.
        // sourceMessages 是唯一尾部选择器消费的规范模型投影。
        let source_messages = conversation_state_to_compaction_source_messages(&state)
            .expect("valid compaction source messages");
        // policy retains two segments under a budget that fits one historical preview and the full newest segment.
        // policy 在可容纳一份历史预览和完整最新段的预算下保留两个执行段。
        let policy = ContextCompactionPolicy {
            tail_turns: 2,
            preserve_recent_tokens: 5_000,
            ..ContextCompactionPolicy::default()
        };
        // plan freezes exact retained IDs and preview statistics before rewrite.
        // plan 在 rewrite 前冻结精确保留 ID 与预览统计。
        let plan = plan_context_compaction(CompactionPlanInput {
            session_label: "rewrite-fixture",
            messages: &source_messages,
            all_messages: &source_messages,
            context_start_message_id: None,
            source_until_message_id: None,
            policy,
        })
        .expect("three execution segments should produce one compaction plan");
        // result carries the frozen plan through the same production constructor used after model summarization.
        // result 通过模型摘要后的同一生产构造器携带冻结计划。
        let result = CompactionResult::from_model_summary(&plan, "frozen rewrite summary");
        (state, result, historical_tool_text, newest_tool_text)
    }

    /// Builds the default successful-newest variant of the frozen rewrite fixture.
    /// 构建最新工具结果成功的默认冻结 rewrite 夹具变体。
    ///
    /// # Returns
    /// 返回值
    /// Returns canonical history, its frozen result, and the historical/newest tool bodies.
    /// 返回规范历史、对应冻结结果以及历史/最新工具正文。
    fn frozen_rewrite_fixture() -> (SessionConversationState, CompactionResult, String, String) {
        frozen_rewrite_fixture_with_tool_errors(true, false)
    }

    /// Verifies planner source boundaries control the retained canonical tail exactly.
    /// 验证规划器源边界会精确控制保留的规范尾部。
    #[test]
    fn compact_conversation_with_summary_until_keeps_messages_after_source_boundary() {
        let mut state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("old-user"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("旧问题"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("old-assistant"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("旧回答"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("tail-user"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("继续任务"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("tail-assistant"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("继续执行"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
            ],
            ..SessionConversationState::default()
        };

        compact_conversation_with_summary_until(
            &mut state,
            "summary-1",
            SessionMessageKind::Assistant,
            "旧上下文摘要",
            "1780000000000",
            Some("tail-user"),
            ContextCompactionPolicy::default(),
        )
        .expect("valid compacted message identities");

        let ids = state
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["summary-1", "tail-assistant"]);
    }

    /// Verifies remote Responses context is canonicalized and inserted directly after its anchor.
    /// 验证远端 Responses 上下文会被规范化并紧接其锚点插入。
    #[test]
    fn remote_responses_context_is_inserted_atomically_after_summary() {
        // state contains the exact summary anchor followed by one retained tail message.
        // state 包含精确摘要锚点及其后一条保留尾部消息。
        let mut state = SessionConversationState {
            messages: vec![
                summary_conversation_message(
                    String::from("summary-1"),
                    SessionMessageKind::Assistant,
                    String::from("checkpoint installed"),
                ),
                SessionConversationMessage {
                    id: String::from("retained-user"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("continue"),
                        attachments: Vec::new(),
                    },
                },
            ],
            records: Vec::new(),
            continuation: Default::default(),
        };
        // result intentionally uses the official legacy alias to exercise canonical persistence.
        // result 刻意使用官方旧别名，以覆盖规范持久化路径。
        let result = CompactionResult {
            summary_record: crate::planning::CompactionSummaryRecord {
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::CompactionSummary,
                title: String::from("Context compaction"),
                body: String::from("checkpoint installed"),
                preview: String::from("checkpoint installed"),
            },
            source_until_message_id: None,
            source_until_turn_index: None,
            retained_tail_start_turn_index: None,
            compaction_id: String::from("compaction-1"),
            created_at: String::from("1"),
            policy: ContextCompactionPolicy::default(),
            tail_selection: crate::planning::CompactionTailSelectionReport {
                context_limit: None,
                policy_source:
                    vulcan_context::ContextCompactionTailPolicySource::MissingContextLimitMinimum,
                nominal_token_budget: 5_000,
                segment_limit: 5,
                retained_segment_count: 1,
                retained_message_ids: vec![String::from("retained-user")],
                retained_token_estimate: 16,
                previewed_tool_output_count: 0,
                omitted_tool_output_token_estimate: 0,
                newest_segment_exceeds_nominal_budget: false,
            },
            provider_context_items: vec![json!({
                "type": "compaction_summary",
                "encrypted_content": "opaque-checkpoint"
            })],
            provider_context_route: Some(crate::planning::ProviderContextRoute {
                provider_id: String::from("custom:test"),
                connection_id: String::from("connection-test"),
                route_fingerprint: String::from("https://proxy.example/v1/responses"),
                protocol_profile_id: String::from("openai-codex-responses"),
                model_id: String::from("gpt-test"),
            }),
            provider_context_usage: Some(json!({ "input_tokens": 10 })),
            provider_context_response_id: Some(String::from("response-test")),
        };

        append_remote_responses_compaction_context(&mut state, "summary-1", &result)
            .expect("remote context should be inserted");

        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[0].id, "summary-1");
        let SessionConversationPayload::Assistant {
            text,
            provider_message_state: Some(provider_state),
            ..
        } = &state.messages[0].payload
        else {
            panic!("remote summary anchor must remain an assistant message");
        };
        assert_eq!(text, "checkpoint installed");
        assert_eq!(state.messages[1].id, "retained-user");
        assert_eq!(
            provider_state
                .pointer("/vulcan_ai/responses_compaction/items/0/type")
                .and_then(Value::as_str),
            Some("compaction")
        );
        assert_eq!(
            provider_state
                .pointer("/vulcan_ai/responses_compaction/connection_id")
                .and_then(Value::as_str),
            Some("connection-test")
        );
    }

    /// Verifies compaction keeps the newest tool-call turn structurally replayable even when the result is large.
    /// 验证压缩在工具结果很大时仍会保持最新工具调用轮次的可回放结构。
    #[test]
    fn compact_conversation_keeps_tool_turn_intact_when_tail_is_large() {
        let mut state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("old-user"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("旧问题"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("old-assistant"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("旧回答"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("recent-user"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("读取文件"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("recent-assistant"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::new(),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: vec![SessionConversationToolCall {
                            id: String::from("tool-call-local"),
                            tool_call_id: Some(String::from("tool-call-provider")),
                            tool_name: String::from("read_file"),
                            arguments: json!({"path": "Cargo.toml"}),
                            argument_parse_error: None,
                        }],
                    },
                },
                SessionConversationMessage {
                    id: String::from("recent-tool-result"),
                    payload: SessionConversationPayload::ToolResult {
                        tool_call_id: Some(String::from("tool-call-provider")),
                        is_error: false,
                        text: "很大的工具输出".repeat(200),
                    },
                },
            ],
            records: Vec::new(),
            continuation: Default::default(),
        };
        let policy = ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 16,
            ..ContextCompactionPolicy::default()
        };

        compact_conversation_with_summary(
            &mut state,
            "summary",
            SessionMessageKind::Assistant,
            "摘要",
            "2",
            policy,
        )
        .expect("valid compacted message identities");

        assert_eq!(state.messages.len(), 3);
        assert_eq!(state.messages[0].id, "summary");
        assert!(matches!(
            &state.messages[1].payload,
            SessionConversationPayload::Assistant { tool_calls, .. } if tool_calls.len() == 1
        ));
        assert!(matches!(
            &state.messages[2].payload,
            SessionConversationPayload::ToolResult { text, .. }
                if text == &"很大的工具输出".repeat(200)
        ));
    }

    /// Verifies rewrite applies the planner's exact suffix, previews only historical output, and preserves newest output.
    /// 验证 rewrite 精确应用规划后缀、仅预览历史输出并完整保留最新输出。
    #[test]
    fn frozen_rewrite_applies_exact_tail_and_preview_boundary() {
        // fixtureResult owns the exact retained IDs and statistics produced from the untouched state.
        // fixtureResult 持有从未改写状态产出的精确保留 ID 与统计。
        let (mut state, result, historical_tool_text, newest_tool_text) = frozen_rewrite_fixture();

        compact_conversation_with_compaction_result(&mut state, "summary-frozen", &result)
            .expect("unchanged canonical suffix should accept the frozen result");

        // ids prove rewrite did not independently select a different suffix.
        // ids 证明 rewrite 没有独立选择其他后缀。
        let ids = state
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "summary-frozen",
                "assistant-historical",
                "tool-historical",
                "assistant-newest",
                "tool-newest"
            ]
        );
        let SessionConversationPayload::Assistant {
            tool_calls: historical_tool_calls,
            ..
        } = &state.messages[1].payload
        else {
            panic!("historical retained assistant row should remain an assistant result");
        };
        assert_eq!(historical_tool_calls.len(), 1);
        assert_eq!(historical_tool_calls[0].id, "local-historical");
        assert_eq!(
            historical_tool_calls[0].tool_call_id.as_deref(),
            Some("call-historical")
        );
        assert_eq!(historical_tool_calls[0].tool_name, "shell_command");
        assert_eq!(
            historical_tool_calls[0].arguments,
            json!({"command": "historical"})
        );
        let SessionConversationPayload::ToolResult {
            tool_call_id: historical_tool_call_id,
            is_error: historical_is_error,
            text: historical_text,
        } = &state.messages[2].payload
        else {
            panic!("historical retained row should remain a tool result");
        };
        assert_eq!(historical_tool_call_id.as_deref(), Some("call-historical"));
        assert!(*historical_is_error);
        assert_ne!(historical_text, &historical_tool_text);
        assert!(historical_text.contains("[上下文压缩省略："));
        let SessionConversationPayload::ToolResult {
            tool_call_id: newest_tool_call_id,
            is_error: newest_is_error,
            text: newest_text,
        } = &state.messages[4].payload
        else {
            panic!("newest retained row should remain a tool result");
        };
        assert_eq!(newest_tool_call_id.as_deref(), Some("call-newest"));
        assert!(!newest_is_error);
        assert_eq!(newest_text, &newest_tool_text);
        assert_eq!(
            state.continuation.updated_at.as_deref(),
            Some(result.created_at.as_str())
        );
        assert!(state.continuation.last_provider_message_id.is_none());
    }

    /// Verifies a failed newest tool result keeps its full text and structured error identity.
    /// 验证失败的最新工具结果保持完整正文与结构化错误身份。
    #[test]
    fn frozen_rewrite_preserves_failed_newest_tool_result_verbatim() {
        // fixture covers a successful historical result and a failed protected newest result.
        // fixture 覆盖成功历史结果与失败且受保护的最新结果。
        let (mut state, result, historical_tool_text, newest_tool_text) =
            frozen_rewrite_fixture_with_tool_errors(false, true);

        compact_conversation_with_compaction_result(&mut state, "summary-failed-newest", &result)
            .expect("an unchanged failed newest result should accept the frozen plan");

        let SessionConversationPayload::ToolResult {
            tool_call_id: historical_tool_call_id,
            is_error: historical_is_error,
            text: historical_text,
        } = &state.messages[2].payload
        else {
            panic!("historical successful row should remain a tool result");
        };
        assert_eq!(historical_tool_call_id.as_deref(), Some("call-historical"));
        assert!(!historical_is_error);
        assert_ne!(historical_text, &historical_tool_text);
        assert!(historical_text.contains(TOOL_OUTPUT_PREVIEW_MARKER_PREFIX));

        let SessionConversationPayload::ToolResult {
            tool_call_id: newest_tool_call_id,
            is_error: newest_is_error,
            text: newest_text,
        } = &state.messages[4].payload
        else {
            panic!("newest failed row should remain a tool result");
        };
        assert_eq!(newest_tool_call_id.as_deref(), Some("call-newest"));
        assert!(*newest_is_error);
        assert_eq!(newest_text, &newest_tool_text);
    }

    /// Verifies a message appended after planning rejects the stale frozen suffix without partial mutation.
    /// 验证规划后追加消息会拒绝失效冻结后缀，且不会产生部分修改。
    #[test]
    fn frozen_rewrite_rejects_concurrent_suffix_change_atomically() {
        // state and result begin from the same planner snapshot.
        // state 与 result 起初来自同一规划快照。
        let (mut state, result, _, _) = frozen_rewrite_fixture();
        state.messages.push(SessionConversationMessage {
            id: String::from("concurrent-user"),
            payload: SessionConversationPayload::UserText {
                text: String::from("new message after planning"),
                attachments: Vec::new(),
            },
        });
        // originalState captures messages, records, and continuation before the stale apply attempt.
        // originalState 捕获失效应用前的消息、记录与 continuation。
        let original_state = serde_json::to_value(&state).expect("state should serialize");

        let error =
            compact_conversation_with_compaction_result(&mut state, "summary-concurrent", &result)
                .expect_err("a concurrent suffix change must reject the frozen result");

        assert!(error.contains("no longer the current canonical suffix"));
        assert_eq!(
            serde_json::to_value(&state).expect("state should serialize after rejection"),
            original_state
        );
    }

    /// Verifies deleting or changing the frozen source boundary rejects the candidate without mutation.
    /// 验证删除或改变冻结来源边界都会拒绝候选且不修改状态。
    #[test]
    fn frozen_rewrite_rejects_deleted_or_changed_source_boundary_atomically() {
        // removedState lacks the exact source boundary covered by the frozen summary.
        // removedState 缺少冻结摘要覆盖的精确来源边界。
        let (mut removed_state, result, _, _) = frozen_rewrite_fixture();
        removed_state.messages.remove(0);
        let removed_snapshot = serde_json::to_value(&removed_state)
            .expect("removed-boundary state should serialize before rejection");
        let removed_error = compact_conversation_with_compaction_result(
            &mut removed_state,
            "summary-removed-boundary",
            &result,
        )
        .expect_err("a deleted source boundary must reject the frozen result");
        assert!(removed_error.contains("source boundary"));
        assert_eq!(
            serde_json::to_value(&removed_state)
                .expect("removed-boundary state should serialize after rejection"),
            removed_snapshot
        );

        // changedState keeps the same position but changes the immutable source identity.
        // changedState 保持相同位置，但改变不可变来源身份。
        let (mut changed_state, result, _, _) = frozen_rewrite_fixture();
        changed_state.messages[0].id = String::from("assistant-old-changed");
        let changed_snapshot = serde_json::to_value(&changed_state)
            .expect("changed-boundary state should serialize before rejection");
        let changed_error = compact_conversation_with_compaction_result(
            &mut changed_state,
            "summary-changed-boundary",
            &result,
        )
        .expect_err("a changed source boundary must reject the frozen result");
        assert!(changed_error.contains("source boundary"));
        assert_eq!(
            serde_json::to_value(&changed_state)
                .expect("changed-boundary state should serialize after rejection"),
            changed_snapshot
        );
    }

    /// Verifies exact record identities retain visible tool output and its detail while unrelated details are reclaimed.
    /// 验证精确记录身份会保留可见工具输出及其详情，并回收无关详情。
    #[test]
    fn record_compaction_uses_exact_tool_and_detail_identities() {
        // originalToolBody and originalDetailBody must survive verbatim even when they exceed historical body limits.
        // originalToolBody 与 originalDetailBody 即使超过历史正文上限也必须原样保留。
        let original_tool_body = "可见工具原文".repeat(3_000);
        let original_detail_body = "工具详情原文".repeat(3_000);
        // records contain seven exact transaction records plus 240 unrelated tool details.
        // records 包含七条精确事务记录以及 240 条无关工具详情。
        let mut records = vec![
            SessionConversationRecord {
                id: String::from("summary-exact"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::CompactionSummary,
                visible: false,
                title: String::from("summary"),
                body: String::from("summary"),
                metadata: None,
                created_at: String::from("1"),
            },
            SessionConversationRecord {
                id: String::from("assistant-retained"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::new(),
                body: String::from("retained assistant"),
                metadata: None,
                created_at: String::from("2"),
            },
            SessionConversationRecord {
                id: String::from("local-retained"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::from("tool call"),
                body: String::from("arguments"),
                metadata: None,
                created_at: String::from("3"),
            },
            SessionConversationRecord {
                id: String::from("visible-retained"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("tool result"),
                body: original_tool_body.clone(),
                metadata: Some(json!({
                    "tool_call_id": "provider-retained",
                    "result": {
                        "detail_ref": {
                            "record_id": "detail-retained"
                        }
                    }
                })),
                created_at: String::from("4"),
            },
            SessionConversationRecord {
                id: String::from("detail-retained"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolDetailOutput,
                visible: false,
                title: String::from("tool detail"),
                body: original_detail_body.clone(),
                metadata: None,
                created_at: String::from("5"),
            },
            SessionConversationRecord {
                id: String::from("section-retained"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::CompactionStatus,
                visible: true,
                title: String::from("status"),
                body: String::new(),
                metadata: None,
                created_at: String::from("6"),
            },
            SessionConversationRecord {
                id: String::from("continuation-retained"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: false,
                title: String::from("continuation"),
                body: String::from("continue"),
                metadata: None,
                created_at: String::from("7"),
            },
        ];
        for index in 0..240 {
            records.push(SessionConversationRecord {
                id: format!("unrelated-detail-{index}"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolDetailOutput,
                visible: false,
                title: String::from("unrelated detail"),
                body: String::from("must be reclaimed"),
                metadata: None,
                created_at: (index + 8).to_string(),
            });
        }
        // state carries the exact canonical tool identities selected by the frozen tail.
        // state 携带冻结尾部选中的精确规范工具身份。
        let mut state = SessionConversationState {
            messages: vec![
                summary_conversation_message(
                    String::from("summary-exact"),
                    SessionMessageKind::Assistant,
                    String::from("summary"),
                ),
                SessionConversationMessage {
                    id: String::from("assistant-retained"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::new(),
                        reasoning: String::new(),
                        provider_message_id: Some(String::from("provider-message-retained")),
                        provider_message_state: None,
                        usage: None,
                        tool_calls: vec![SessionConversationToolCall {
                            id: String::from("local-retained"),
                            tool_call_id: Some(String::from("provider-retained")),
                            tool_name: String::from("shell_command"),
                            arguments: json!({"command": "exact"}),
                            argument_parse_error: None,
                        }],
                    },
                },
                SessionConversationMessage {
                    id: String::from("canonical-tool-result"),
                    payload: SessionConversationPayload::ToolResult {
                        tool_call_id: Some(String::from("provider-retained")),
                        is_error: false,
                        text: String::from("model projection may be previewed"),
                    },
                },
            ],
            records,
            continuation: Default::default(),
        };
        // additionalRetainedIds names non-canonical status rows owned by the same transaction.
        // additionalRetainedIds 指定同一事务持有的非规范状态记录。
        let additional_retained_ids = vec![
            String::from("section-retained"),
            String::from("continuation-retained"),
        ];

        let report = compact_conversation_records_after_summary_with_retained_ids(
            &mut state,
            "summary-exact",
            &additional_retained_ids,
            ContextCompactionPolicy {
                tail_turns: 20,
                ..ContextCompactionPolicy::default()
            },
            test_compact_change_set_host_result,
        );

        let retained_ids = state
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(state.records.len(), 7);
        assert_eq!(report.original_record_count, 247);
        assert_eq!(report.full_record_count, 7);
        assert_eq!(report.deleted_record_count, 240);
        assert_eq!(report.unmatched_retained_tool_identity_count, 0);
        assert!(retained_ids.contains("visible-retained"));
        assert!(retained_ids.contains("detail-retained"));
        assert!(retained_ids.contains("section-retained"));
        assert!(retained_ids.contains("continuation-retained"));
        assert!(!retained_ids.contains("unrelated-detail-239"));
        assert_eq!(
            state
                .records
                .iter()
                .find(|record| record.id == "visible-retained")
                .map(|record| record.body.as_str()),
            Some(original_tool_body.as_str())
        );
        assert_eq!(
            state
                .records
                .iter()
                .find(|record| record.id == "detail-retained")
                .map(|record| record.body.as_str()),
            Some(original_detail_body.as_str())
        );
    }

    /// Verifies host replay ledgers survive even after their messages leave the active context.
    /// 验证宿主重放账本在对应消息离开活动上下文后仍完整保留。
    #[test]
    fn record_compaction_preserves_host_ledger_without_retained_message() {
        // The reserved identity, not a user-authored body or metadata lookalike, owns this ledger.
        // 账本归属由保留身份确定，而不是用户正文或相似元数据。
        let ledger = SessionConversationRecord {
            id: String::from("runtime-host-record:runtime-host:occurrence-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::ExploreOutput,
            visible: false,
            title: String::from("Runtime host context"),
            body: "x".repeat(PROTOCOL_RECORD_COMPACTED_BODY_CHARS.saturating_add(1)),
            metadata: Some(json!({"event_id": "occurrence-1", "version": 1})),
            created_at: String::from("0"),
        };
        // Ordinary user records deliberately have identical content but remain subject to display pruning.
        // 普通用户记录故意具有相同内容，仍按展示记录规则裁剪。
        let mut user_record = ledger.clone();
        user_record.id = String::from("human-record");
        let mut state = SessionConversationState {
            records: vec![ledger.clone(), user_record],
            ..Default::default()
        };
        let report = compact_conversation_records_after_summary(
            &mut state,
            "summary",
            ContextCompactionPolicy::default(),
            test_compact_change_set_host_result,
        );
        assert_eq!(report.full_record_count, 1);
        assert_eq!(report.compacted_historical_record_count, 1);
        assert_eq!(
            serde_json::to_value(&state.records[0]).unwrap(),
            serde_json::to_value(ledger).unwrap()
        );
        assert!(state.records[1].metadata.is_none());
        assert_eq!(
            state.records[1].body.chars().count(),
            PROTOCOL_RECORD_COMPACTED_BODY_CHARS
        );
        assert!(
            state.messages.is_empty(),
            "ledger retention must never reinsert a provider message"
        );
    }

    /// Verifies record compaction preserves the complete i18n metadata of model-switch audit notes.
    /// 验证记录压缩会完整保留模型切换审计说明的 i18n 元数据。
    #[test]
    fn record_compaction_preserves_model_switch_display_audit_metadata() {
        // modelSwitchMetadata freezes exact route labels for locale-specific rendering after reload or compaction.
        // modelSwitchMetadata 冻结精确路由标签，供重载或压缩后的本地化渲染使用。
        let model_switch_metadata = json!({
            "notice_kind": "model_switch",
            "activation": "next_submission",
            "provider_id": "command-code",
            "provider_name": "Command Code",
            "connection_id": "command-code-account",
            "connection_name": "Command Code Account",
            "model_id": "deepseek",
            "model_name": "DeepSeek",
        });
        // state includes one durable model-switch note and one unrelated display note eligible for deletion.
        // state 包含一条可靠模型切换说明，以及一条允许删除的无关展示说明。
        let mut state = SessionConversationState {
            messages: vec![summary_conversation_message(
                String::from("summary-exact"),
                SessionMessageKind::Assistant,
                String::from("summary"),
            )],
            records: vec![
                SessionConversationRecord {
                    id: String::from("summary-exact"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::CompactionSummary,
                    visible: false,
                    title: String::from("summary"),
                    body: String::from("summary"),
                    metadata: None,
                    created_at: String::from("1"),
                },
                SessionConversationRecord {
                    id: String::from("model-switch-audit"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::DisplayNote,
                    visible: true,
                    title: String::new(),
                    body: String::from(
                        "provider_id=command-code\nconnection_id=command-code-account\nmodel_id=deepseek",
                    ),
                    metadata: Some(model_switch_metadata.clone()),
                    created_at: String::from("2"),
                },
                SessionConversationRecord {
                    id: String::from("discarded-display-note"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::DisplayNote,
                    visible: true,
                    title: String::from("temporary"),
                    body: String::from("temporary"),
                    metadata: Some(json!({"notice_kind": "temporary"})),
                    created_at: String::from("3"),
                },
            ],
            continuation: Default::default(),
        };

        // report captures the exact identity-based retention outcome.
        // report 捕获基于身份的精确保留结果。
        let report = compact_conversation_records_after_summary_with_retained_ids(
            &mut state,
            "summary-exact",
            &[],
            ContextCompactionPolicy::default(),
            test_compact_change_set_host_result,
        );
        // retainedModelSwitch is the complete durable audit row after compaction.
        // retainedModelSwitch 是压缩后的完整可靠审计行。
        let retained_model_switch = state
            .records
            .iter()
            .find(|record| record.id == "model-switch-audit")
            .expect("model-switch audit record must survive compaction");

        assert_eq!(retained_model_switch.metadata, Some(model_switch_metadata));
        assert_eq!(report.original_record_count, 3);
        assert_eq!(report.full_record_count, 2);
        assert_eq!(report.deleted_record_count, 1);
        assert!(
            state
                .records
                .iter()
                .all(|record| record.id != "discarded-display-note")
        );
    }

    /// Verifies record-level compaction keeps generated-media tool results visible with the minimal host metadata needed to rebuild inline cards later.
    /// 验证记录级压缩会保留生成媒体工具结果的可见性，并保留后续重建内联卡片所需的最小宿主元数据。
    #[test]
    fn record_compaction_preserves_generated_media_tool_result_metadata() {
        let mut records = vec![SessionConversationRecord {
            id: String::from("tool-result-media-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("调用 image_generation 工具"),
            body: String::from("## 工具返回\n\n媒体已生成。"),
            metadata: Some(json!({
                "tool_call_id": "call-media-1",
                "host_result": {
                    "kind": "generated_attachments",
                    "payload": {
                        "outputs": [{
                            "kind": "image",
                            "file_name": "demo.jpg",
                            "local_path": r"D:\demo.jpg",
                            "mime_type": "image/jpeg",
                            "byte_len": 1024,
                            "asset_id": "asset-1",
                            "ignored": "drop-me"
                        }]
                    }
                },
                "noise": {
                    "large": "drop-me-too"
                }
            })),
            created_at: String::from("1"),
        }];
        for index in 0..50 {
            records.push(SessionConversationRecord {
                id: format!("recent-{index}"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::new(),
                body: format!("最近消息 {index}"),
                metadata: None,
                created_at: (index + 2).to_string(),
            });
        }

        let mut state = SessionConversationState {
            messages: Vec::new(),
            records,
            continuation: Default::default(),
        };
        let report = compact_conversation_records_after_summary(
            &mut state,
            "missing-summary",
            ContextCompactionPolicy {
                tail_turns: 1,
                ..ContextCompactionPolicy::default()
            },
            test_compact_change_set_host_result,
        );

        assert_eq!(state.records.len(), 1);
        assert_eq!(report.original_record_count, 51);
        assert_eq!(report.deleted_record_count, 50);
        let preserved_record = state
            .records
            .iter()
            .find(|record| record.id == "tool-result-media-1")
            .expect("generated-media tool result should survive compaction");
        assert!(preserved_record.body.is_empty());
        let metadata = preserved_record
            .metadata
            .as_ref()
            .expect("generated-media tool result should preserve metadata");
        let outputs = metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("outputs"))
            .and_then(Value::as_array)
            .expect("generated-media outputs should remain available");
        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs[0].get("kind").and_then(Value::as_str),
            Some("image")
        );
        assert_eq!(
            outputs[0].get("file_name").and_then(Value::as_str),
            Some("demo.jpg")
        );
        assert_eq!(
            outputs[0].get("local_path").and_then(Value::as_str),
            Some(r"D:\demo.jpg")
        );
        assert!(outputs[0].get("ignored").is_none());
        assert!(metadata.get("noise").is_none());
    }

    /// Verifies record-level compaction keeps `change_set` tool results visible with the compact host payload needed to rebuild inline file-change summaries later.
    /// 验证记录级压缩会保留 `change_set` 工具结果的可见性，并保留后续重建内联文件变更摘要所需的紧凑宿主载荷。
    #[test]
    fn record_compaction_preserves_change_set_tool_result_metadata() {
        let mut records = vec![SessionConversationRecord {
            id: String::from("tool-result-change-set-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("调用 write_file 工具"),
            body: String::from("## 工具返回\n\n文件变更已应用。"),
            metadata: Some(json!({
                "tool_call_id": "call-change-set-1",
                "host_result": {
                    "kind": "change_set",
                    "payload": {
                        "mode": "preview",
                        "summary": "Previewed file edit.",
                        "files": [{
                            "change": "create",
                            "path": r"D:\openvulcan\vulcan-code\src\demo.lua",
                            "content": "return {}\n"
                        }]
                    }
                },
                "noise": {
                    "large": "drop-me-too"
                }
            })),
            created_at: String::from("1"),
        }];
        for index in 0..50 {
            records.push(SessionConversationRecord {
                id: format!("recent-{index}"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::new(),
                body: format!("最近消息 {index}"),
                metadata: None,
                created_at: (index + 2).to_string(),
            });
        }

        let mut state = SessionConversationState {
            messages: Vec::new(),
            records,
            continuation: Default::default(),
        };
        let report = compact_conversation_records_after_summary(
            &mut state,
            "missing-summary",
            ContextCompactionPolicy {
                tail_turns: 1,
                ..ContextCompactionPolicy::default()
            },
            test_compact_change_set_host_result,
        );

        assert_eq!(state.records.len(), 1);
        assert_eq!(report.original_record_count, 51);
        assert_eq!(report.deleted_record_count, 50);
        let metadata = state
            .records
            .iter()
            .find(|record| record.id == "tool-result-change-set-1")
            .and_then(|record| record.metadata.as_ref())
            .expect("change-set tool result should preserve metadata");
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|host_result| host_result.get("kind"))
                .and_then(Value::as_str),
            Some("change_set")
        );
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|host_result| host_result.get("payload"))
                .and_then(|payload| payload.get("stats"))
                .and_then(|stats| stats.get("file_count"))
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|host_result| host_result.get("payload"))
                .and_then(|payload| payload.get("verification"))
                .and_then(|verification| verification.get("status"))
                .and_then(Value::as_str),
            Some("skipped")
        );
        assert!(metadata.get("noise").is_none());
    }

    /// Verifies a summary identity collision leaves messages and continuation completely unchanged.
    /// 验证摘要身份冲突会使消息与 continuation 完全保持不变。
    #[test]
    fn compaction_rewrite_is_atomic_on_summary_identity_collision() {
        // state retains `summary-1` after the selected source boundary, creating an exact collision.
        // state 会在所选源边界后保留 `summary-1`，从而形成准确冲突。
        let mut state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("source-1"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("source"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("source-assistant"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("source output"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("summary-1"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("retained"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
            ],
            records: Vec::new(),
            continuation: vulcan_context::SessionProviderContinuationState {
                provider_id: Some(String::from("provider")),
                model_id: Some(String::from("model")),
                last_provider_message_id: Some(String::from("response-1")),
                updated_at: Some(String::from("before")),
                ..vulcan_context::SessionProviderContinuationState::default()
            },
        };
        // original_state captures both the message window and continuation before validation.
        // original_state 捕获校验前的消息窗口与 continuation。
        let original_state = serde_json::to_value(&state).expect("state should serialize");

        // error proves the candidate window is rejected before either canonical field is assigned.
        // error 证明候选窗口会在任何规范字段赋值前被拒绝。
        let error = compact_conversation_with_summary_until(
            &mut state,
            "summary-1",
            SessionMessageKind::Assistant,
            "compacted",
            "after",
            Some("source-assistant"),
            ContextCompactionPolicy::default(),
        )
        .expect_err("summary identity collision must fail");

        assert!(error.contains("duplicates id \"summary-1\""));
        assert_eq!(
            serde_json::to_value(&state).expect("state should serialize after failure"),
            original_state
        );
    }
}
