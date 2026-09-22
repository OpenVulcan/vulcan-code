//! Lossless native checkpoints inside clone-pruned compaction sources.
//! 仅克隆剪枝压缩来源中的无损原生检查点。

use std::collections::BTreeMap;
use vulcan_context::{
    ContextCompactionPolicy, SessionConversationMessage, SessionConversationPayload,
    SessionConversationState, SessionMessageEntry, conversation_state_from_session_messages,
    conversation_state_to_compaction_source_messages, estimate_mixed_language_text_tokens,
};

use crate::{
    CompactionInputBudget, CompactionInputPruneMode, PreparedCompactionSource,
    estimate_compaction_source_tokens, prepare_compaction_source_for_target_budget,
};

/// Borrows canonical checkpoint ownership for pruning and restoration, without trusting transcript metadata.
/// 借用规范检查点归属进行剪枝和恢复，不信任转录元数据。
pub struct NativeCompactionSources<'a> {
    /// Exact chronological positions in the authoritative conversation.
    /// 权威会话中的精确时序位置。
    positions: BTreeMap<&'a str, usize>,
    /// Native carriers keyed by their canonical message identity.
    /// 按规范消息身份索引的原生载体。
    checkpoints: BTreeMap<&'a str, &'a SessionConversationMessage>,
    /// Additional heuristic cost of ordered native items, excluding carrier text already counted by rows.
    /// 有序原生项的额外启发式开销，不含转录行已计入的载体正文。
    native_token_estimate: usize,
}

impl<'a> NativeCompactionSources<'a> {
    /// Captures `state` identities and explicit Responses checkpoint carriers; returns duplicate or malformed-state errors.
    /// 捕获`state`身份和明确的 Responses 检查点载体；重复或无效状态返回错误。
    /// This preserves data only; the actual frozen route must still authorize replay before dispatch.
    /// 此处仅保留数据；发送前仍必须由实际冻结路由授权重放。
    pub fn capture(state: &'a SessionConversationState) -> Result<Self, String> {
        // These maps borrow immutable authority rather than reinterpreting rendered labels.
        // 这些映射借用不可变权威，不重新解释渲染标签。
        let mut captured = Self {
            positions: BTreeMap::new(),
            checkpoints: BTreeMap::new(),
            native_token_estimate: 0,
        };
        for (index, message) in state.messages.iter().enumerate() {
            if captured.positions.insert(&message.id, index).is_some() {
                return Err(String::from(
                    "Compaction source contains duplicate canonical identities.",
                ));
            }
            // Only this producer-owned extension represents an opaque checkpoint.
            // 只有此生产者持有的扩展表示不透明检查点。
            let SessionConversationPayload::Assistant {
                reasoning,
                provider_message_id,
                provider_message_state: Some(provider_state),
                usage,
                tool_calls,
                ..
            } = &message.payload
            else {
                continue;
            };
            // Ordinary reasoning replay envelopes remain subject to the existing transcript policy.
            // 普通推理重放封套继续遵守既有转录策略。
            let Some(envelope) = provider_state.pointer("/vulcan_ai/responses_compaction") else {
                continue;
            };
            if !reasoning.is_empty()
                || provider_message_id.is_some()
                || usage.is_some()
                || !tool_calls.is_empty()
            {
                return Err(String::from(
                    "Native compaction carrier contains unrelated assistant state.",
                ));
            }
            // Native items cannot be shortened or converted into summary prose.
            // 原生项不能缩短，也不能转换成摘要文字。
            let items = envelope
                .get("items")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    String::from("Native compaction carrier is missing its ordered item list.")
                })?;
            if !items.iter().any(|item| {
                item.get("type").and_then(serde_json::Value::as_str) == Some("compaction")
            }) || !items
                .iter()
                .all(vulcan_ai_contract::is_replayable_codex_responses_item)
            {
                return Err(String::from(
                    "Native compaction carrier has invalid replay items.",
                ));
            }
            // This is a planning heuristic, never a provider-reported token measurement.
            // 这是规划启发式估算，绝非供应商报告的 Token 实测值。
            let encoded = serde_json::to_string(items).map_err(|error| error.to_string())?;
            captured.native_token_estimate = captured
                .native_token_estimate
                .saturating_add(estimate_mixed_language_text_tokens(&encoded));
            captured.checkpoints.insert(&message.id, message);
        }
        Ok(captured)
    }

    /// Estimates `rows` including immutable native item cost; does not replace the final wire budget check.
    /// 估算包含不可变原生项开销的`rows`；不替代最终出站预算检查。
    pub fn estimate_tokens(&self, rows: &[SessionMessageEntry]) -> usize {
        estimate_compaction_source_tokens(rows).saturating_add(self.native_token_estimate)
    }

    /// Restores selected `rows` from their immutable native owners; rejects omitted, altered, or reordered carriers.
    /// 根据不可变原生所有者恢复选定`rows`；拒绝遗漏、改写或重排的载体。
    /// Other rows retain ordinary transcript pruning semantics and no original text is backfilled.
    /// 其他行保留普通转录剪枝语义，不回填原始正文。
    pub fn restore(
        &self,
        rows: &[SessionMessageEntry],
    ) -> Result<SessionConversationState, String> {
        // Preserve existing behavior for sources without any native checkpoint.
        // 没有原生检查点的来源保留既有行为。
        if self.checkpoints.is_empty() {
            return conversation_state_from_session_messages(rows);
        }
        self.validate_rows(rows)?;
        // Reconstruct only selected text, then replace exact checkpoint payloads from canonical authority.
        // 只重建选定文字，再从规范权威替换精确检查点载荷。
        let mut state = conversation_state_from_session_messages(rows)?;
        for (id, original) in &self.checkpoints {
            // Missing conversion is an error even if a display-only row still has the requested ID.
            // 即使仅展示行仍具有所需 ID，转换缺失也属于错误。
            let carrier = state
                .messages
                .iter_mut()
                .find(|message| message.id == *id)
                .ok_or_else(|| {
                    String::from(
                        "Native compaction carrier is absent from selected protocol history.",
                    )
                })?;
            // Compare the expected transcript payload, allowing both existing canonical and V1 planning record kinds.
            // 比较预期转录载荷，同时允许既有规范记录类型与 V1 规划记录类型。
            let original_state = SessionConversationState {
                messages: vec![(*original).clone()],
                ..Default::default()
            };
            // Use the same actual transcript conversion as the planner, including diagnostic text normalization.
            // 使用与规划器相同的真实转录转换，包括诊断正文规范化。
            let original_rows = conversation_state_to_compaction_source_messages(&original_state)?;
            // The single native carrier always projects to one assistant transcript payload.
            // 单一原生载体始终投影为一条助手转录载荷。
            let expected = conversation_state_from_session_messages(&original_rows)?;
            if serde_json::to_value(&carrier.payload).map_err(|error| error.to_string())?
                != serde_json::to_value(&expected.messages[0].payload)
                    .map_err(|error| error.to_string())?
            {
                return Err(String::from(
                    "Native compaction carrier was altered by transcript pruning.",
                ));
            }
            carrier.payload = original.payload.clone();
        }
        Ok(state)
    }

    /// Prunes `rows` under `budget`, `policy`, and `mode`, retaining each native carrier unchanged and in order.
    /// 按`budget`、`policy`与`mode`剪枝`rows`，保持每个原生载体完整且顺序不变。
    /// Returns the actual merged source and accounting report, or a canonical ownership error.
    /// 返回实际合并来源与统计报告，或规范归属错误。
    pub fn prepare(
        &self,
        rows: &[SessionMessageEntry],
        budget: CompactionInputBudget,
        policy: ContextCompactionPolicy,
        mode: CompactionInputPruneMode,
    ) -> Result<PreparedCompactionSource, String> {
        if self.checkpoints.is_empty() {
            return Ok(prepare_compaction_source_for_target_budget(
                rows, budget, policy, mode,
            ));
        }
        self.restore(rows)?;
        // Exclude protected carriers before the generic oldest-first removal and body truncation steps.
        // 在通用最旧优先删除和正文截断步骤前排除受保护载体。
        let dialogue = rows
            .iter()
            .filter(|row| !self.checkpoints.contains_key(row.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        // Reserve carrier framing plus native bytes; saturation never permits dropping native data.
        // 预留载体封装与原生字节开销；饱和相减绝不允许丢弃原生数据。
        let protected = rows
            .iter()
            .filter(|row| self.checkpoints.contains_key(row.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        // The reduced budget applies only to ordinary dialogue rows.
        // 缩减预算只作用于普通对话行。
        let dialogue_budget = CompactionInputBudget {
            target_input_budget_tokens: budget
                .target_input_budget_tokens
                .saturating_sub(self.estimate_tokens(&protected)),
            safety_reserve_tokens: budget.safety_reserve_tokens,
        };
        // Preserve the existing dialogue pruning algorithm and its detailed counters.
        // 保留既有对话剪枝算法及详细计数。
        let mut prepared =
            prepare_compaction_source_for_target_budget(&dialogue, dialogue_budget, policy, mode);
        // Merge at canonical positions, never append an old checkpoint after its retained tail.
        // 按规范位置合并，绝不把旧检查点追加到其保留尾部之后。
        prepared.source_messages.extend(protected);
        prepared
            .source_messages
            .sort_by_key(|row| self.positions[row.id.as_str()]);
        prepared.prune_report.target_input_budget_tokens = budget.target_input_budget_tokens;
        prepared.prune_report.original_token_estimate = self.estimate_tokens(rows);
        prepared.prune_report.pruned_token_estimate =
            self.estimate_tokens(&prepared.source_messages);
        prepared.prune_report.still_exceeds_budget =
            prepared.prune_report.pruned_token_estimate > budget.target_input_budget_tokens;
        Ok(prepared)
    }

    /// Checks that selected `rows` form a unique ordered subset containing every immutable checkpoint.
    /// 校验选定`rows`构成唯一有序子集，并包含每个不可变检查点。
    fn validate_rows(&self, rows: &[SessionMessageEntry]) -> Result<(), String> {
        // Strictly increasing positions reject duplicate identities without choosing a first match.
        // 严格递增位置拒绝重复身份，不选择第一个匹配项。
        let mut previous = None;
        // Count exact native identities only after their order has been proven.
        // 只在顺序得到证明后统计精确原生身份。
        let mut checkpoint_count = 0;
        for row in rows {
            // Unknown rows have no authority to stand in for original checkpoint sources.
            // 未知行无权替代原检查点来源。
            let position = *self.positions.get(row.id.as_str()).ok_or_else(|| {
                String::from("Native compaction source contains an unknown identity.")
            })?;
            if previous.is_some_and(|previous| position <= previous) {
                return Err(String::from(
                    "Native compaction source identities are repeated or reordered.",
                ));
            }
            previous = Some(position);
            checkpoint_count += usize::from(self.checkpoints.contains_key(row.id.as_str()));
        }
        if checkpoint_count != self.checkpoints.len() {
            return Err(String::from(
                "Native compaction source omitted an immutable checkpoint.",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
