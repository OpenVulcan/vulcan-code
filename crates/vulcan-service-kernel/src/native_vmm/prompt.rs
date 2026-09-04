//! Stable prompt-input projection for native VMM Core LLM effects.
//! 原生 VMM Core LLM Effect 的稳定提示词输入投影。

use serde_json::{Map, Value, json};
use time::{OffsetDateTime, UtcOffset, macros::format_description};
use vulcan_memorymesh_core::{
    CoreError, ErrorCode, ErrorStage, LlmInput, MemoryCategory, MemoryScopeClass, MemorySourceKind,
    PostActionReviewContext, PreCheckTurnContext, ProfileLevel, ProfileNode, ProfileSourceKind,
    Role, SectionRole, TurnInputKind,
};

/// Renders one typed Core LLM input into the stable JSON body described by the bundled prompt.
/// 将一项强类型 Core LLM 输入渲染为内置提示词约定的稳定 JSON 请求体。
///
/// # Errors
/// Returns a typed Core effect error when a timestamp or JSON value cannot be projected.
/// 当时间戳或 JSON 值无法投影时返回强类型 Core Effect 错误。
pub(super) fn render_llm_input(input: &LlmInput) -> Result<String, CoreError> {
    let value = match input {
        LlmInput::PreCheckIntent {
            recent_turns,
            user_content,
            max_search_queries,
        } => {
            let rendered_turns = recent_turns
                .iter()
                .map(|turn| {
                    Ok(json!({
                        "turn_id": native_turn_prompt_id(turn.turn_id.as_str())?,
                        "content_type": turn.content_type.as_str(),
                        "content": turn.content.trim(),
                    }))
                })
                .collect::<Result<Vec<_>, CoreError>>()?;
            json!({
                "recent_turns": rendered_turns,
                "current_user_input": user_content.trim(),
                "current_context_hints": extract_precheck_context_hints(user_content, 4),
                "recent_context_hints": extract_recent_turn_context_hints(recent_turns, 4),
                "max_search_queries": max_search_queries,
            })
        }
        LlmInput::PreCheckReview {
            current_timestamp_ns,
            search_queries,
            intent_reason,
            user_content,
            candidates,
        } => {
            let candidates = candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| {
                    let explanation = &candidate.explanation;
                    json!({
                        "candidate_number": index.saturating_add(1),
                        "memory_id": candidate.memory_id.as_str(),
                        "source_turn_id": candidate.source_turn_id.as_ref().map(|id| id.as_str()),
                        "created_datetime": format_timestamp(candidate.created_at_ns),
                        "source_kind": memory_source_label(candidate.source_kind),
                        "scope_level": memory_scope_label(candidate.scope_class),
                        "category": memory_category_number(candidate.category),
                        "abstract": candidate.summary.trim(),
                        "details": candidate.details.trim(),
                        "score": candidate.score,
                        "score_label": "final_score",
                        "score_explanation": score_explanation(explanation),
                        "origin": memory_source_label(candidate.source_kind),
                        "origin_label": memory_source_label(candidate.source_kind),
                        "origin_explanation": "Core-authoritative memory provenance",
                        "support_count": explanation.context_total_support_count,
                        "rebuttal_count": explanation.context_total_rebuttal_count,
                        "matched_context_values": explanation.context_matched_values,
                        "matched_context_support_count": explanation.context_matched_support_count,
                        "matched_context_rebuttal_count": explanation.context_matched_rebuttal_count,
                        "matched_context_score_delta": explanation.context_delta.unwrap_or(0.0),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "current_datetime": format_timestamp(*current_timestamp_ns),
                "user_content": user_content.trim(),
                "search_queries": normalized_strings(search_queries),
                "intent_reason": intent_reason.as_deref().unwrap_or_default().trim(),
                "candidates": candidates,
            })
        }
        LlmInput::PostActionAnalysis {
            current_timestamp_ns,
            reference_turns,
            target_turn,
            recent_direct_writes,
        } => {
            let references = reference_turns
                .iter()
                .map(|turn| {
                    json!({
                        "turn_id": turn.turn_id.as_str(),
                        "details": turn.details.trim(),
                    })
                })
                .collect::<Vec<_>>();
            let direct_writes = recent_direct_writes
                .iter()
                .map(|memory| {
                    json!({
                        "memory_id": memory.memory_id.as_str(),
                        "scope_level": memory_scope_label(memory.scope_class),
                        "abstract": memory.summary.trim(),
                        "details": memory.content.trim(),
                        "created_datetime": format_timestamp(memory.created_at_ns),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "current_time": {
                    "datetime": format_timestamp(*current_timestamp_ns),
                },
                "reference_turns": references,
                "target_turn": {
                    "turn_id": target_turn.id.as_str(),
                    "created_datetime": format_timestamp(target_turn.created_at_ns),
                    "raw_turn": target_turn,
                },
                "recent_direct_memory_writes": direct_writes,
            })
        }
        LlmInput::PostActionReview {
            context,
            memory_candidates,
            profile_candidates,
            current_profile_nodes,
        } => render_postaction_review(
            context,
            memory_candidates,
            profile_candidates,
            current_profile_nodes,
        ),
        LlmInput::ProfileInstruction {
            target_section,
            instruction,
            current_nodes,
        } => {
            let nodes = current_nodes
                .iter()
                .map(render_profile_node_for_instruction)
                .collect::<Vec<_>>();
            json!({
                "target": section_target_label(target_section.role),
                "bind_id": target_section.scope_id.as_str(),
                "instruction": instruction.trim(),
                "authority_floor": {
                    "priority": "P1",
                    "level": "L2",
                    "reason": "Manual user and project profile instructions are explicit assertions and must remain stable.",
                },
                "active_nodes": nodes,
            })
        }
    };
    serde_json::to_string(&value).map_err(|error| {
        CoreError::new(
            ErrorCode::InvalidArgument,
            ErrorStage::Effect,
            format!("failed to encode native VMM prompt input: {error}"),
        )
    })
}

/// Resolves a native durable turn identity to the numeric prompt contract used by VMM.
/// 将原生耐久轮次身份解析为 VMM 使用的数字提示词契约。
fn native_turn_prompt_id(turn_id: &str) -> Result<u64, CoreError> {
    turn_id
        .strip_prefix("vmm-turn-")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::InvalidArgument,
                ErrorStage::Effect,
                "native VMM PreCheck turn identity is not canonical",
            )
        })
}

/// Extracts compact deterministic prompt anchors without invoking another model.
/// 在不调用另一个模型的情况下提取紧凑且确定性的提示词锚点。
fn extract_precheck_context_hints(text: &str, limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let mut normalized = normalize_hint_text(text);
    if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
        let mut parts = Vec::new();
        for key in ["user", "assistant", "details", "content"] {
            if let Some(value) = value.get(key) {
                collect_json_hint_text(value, &mut parts);
            }
        }
        if let Some(items) = value.get("timeline").and_then(Value::as_array) {
            for item in items {
                if let Some(content) = item.get("content") {
                    collect_json_hint_text(content, &mut parts);
                }
            }
        }
        if !parts.is_empty() {
            normalized = normalize_hint_text(&parts.join(" "));
        }
    }
    if normalized.is_empty() {
        return Vec::new();
    }
    let mut hints = Vec::new();
    append_precheck_hint(&mut hints, &normalized, limit);
    for segment in normalized.split([
        '，', ',', '。', '；', ';', '：', ':', '、', '（', '）', '(', ')', '？', '?', '！', '!',
    ]) {
        append_precheck_hint(&mut hints, segment, limit);
        if hints.len() >= limit {
            break;
        }
    }
    hints
}

/// Extracts newest-first deduplicated anchors from recent turn contexts.
/// 从最近轮次上下文中按新到旧提取去重锚点。
fn extract_recent_turn_context_hints(turns: &[PreCheckTurnContext], limit: usize) -> Vec<String> {
    let mut hints = Vec::new();
    for turn in turns.iter().rev() {
        for hint in extract_precheck_context_hints(&turn.content, 2) {
            append_precheck_hint(&mut hints, &hint, limit);
            if hints.len() >= limit {
                return hints;
            }
        }
    }
    hints
}

/// Appends one normalized, bounded, non-deictic hint exactly once.
/// 仅追加一次经过规范化、有界且非纯指代的锚点。
fn append_precheck_hint(hints: &mut Vec<String>, value: &str, limit: usize) {
    if hints.len() >= limit {
        return;
    }
    let value = normalize_hint_text(value)
        .chars()
        .take(48)
        .collect::<String>();
    if is_weak_precheck_hint(&value)
        || hints
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        return;
    }
    hints.push(value);
}

/// Reports whether a candidate is only conversational glue and not a useful anchor.
/// 判断候选是否仅为对话连接词而不是有效锚点。
fn is_weak_precheck_hint(value: &str) -> bool {
    const WEAK_HINTS: &[&str] = &[
        "这个",
        "这个改动",
        "这个地方",
        "这个问题",
        "这个方案",
        "这个实现",
        "这个逻辑",
        "这个流程",
        "这里",
        "这样",
        "这种",
        "这一块",
        "这块",
        "这段",
        "这一段",
        "上面",
        "前面",
        "后面",
        "刚才",
        "刚刚",
        "当前",
        "为什么",
        "怎么",
        "如何",
        "是否",
        "会不会",
    ];
    let normalized = normalize_hint_text(value).to_lowercase();
    normalized.chars().count() <= 1 || WEAK_HINTS.contains(&normalized.as_str())
}

/// Collapses Unicode whitespace into one stable single-line hint.
/// 将 Unicode 空白折叠为稳定的单行锚点。
fn normalize_hint_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Collects textual leaves from one known prompt JSON field.
/// 从一个已知提示词 JSON 字段中收集文本叶节点。
fn collect_json_hint_text(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            let text = normalize_hint_text(text);
            if !text.is_empty() {
                output.push(text);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_json_hint_text(item, output);
            }
        }
        Value::Object(fields) => {
            for value in fields.values() {
                collect_json_hint_text(value, output);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Renders the joint post-action memory and profile review payload.
/// 渲染联合 PostAction 记忆与画像评审载荷。
fn render_postaction_review(
    context: &PostActionReviewContext,
    memory_candidates: &[vulcan_memorymesh_core::MemoryReviewCandidate],
    profile_candidates: &[vulcan_memorymesh_core::ProfileCandidate],
    current_profile_nodes: &[ProfileNode],
) -> Value {
    let (current_ns, turn_id, user_input_kind, user_content, assistant_content, sections) =
        match context {
            PostActionReviewContext::Turn {
                target_turn,
                user_input_kind,
                profile_sections,
            } => (
                target_turn.created_at_ns,
                Some(target_turn.id.as_str()),
                Some(turn_input_kind_label(*user_input_kind)),
                timeline_content(target_turn.timeline.as_slice(), Role::User),
                timeline_content(target_turn.timeline.as_slice(), Role::Assistant),
                profile_sections.as_slice(),
            ),
            PostActionReviewContext::DirectWrite { occurred_at_ns } => (
                *occurred_at_ns,
                None,
                None,
                String::new(),
                String::new(),
                &[] as &[vulcan_memorymesh_core::ProfileViewSection],
            ),
        };

    let memories = memory_candidates
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let similar_memories = item
                .similar_memories
                .iter()
                .map(|hit| {
                    json!({
                        "memory_id": hit.memory.id.as_str(),
                        "source_turn_id": hit.memory.source_turn_id.as_ref().map(|id| id.as_str()),
                        "created_datetime": format_timestamp(hit.memory.created_at_ns),
                        "scope_level": memory_scope_label(hit.memory.scope_class),
                        "category": memory_category_number(hit.memory.category),
                        "score": hit.final_score,
                        "origin": memory_source_label(hit.memory.source_kind),
                        "abstract": hit.memory.summary.trim(),
                        "details": hit.memory.content.trim(),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "candidate_index": index,
                "candidate_datetime": format_timestamp(current_ns),
                "category": memory_category_number(item.candidate.category),
                "abstract": item.candidate.summary.trim(),
                "details": item.candidate.content.trim(),
                "evidence_source": item.candidate.evidence_source.trim(),
                "admission_reason": item.candidate.admission_reason.as_deref().unwrap_or_default().trim(),
                "similar_memories": similar_memories,
            })
        })
        .collect::<Vec<_>>();

    let mut root = Map::new();
    root.insert(
        "current_datetime".to_owned(),
        json!(format_timestamp(current_ns)),
    );
    root.insert(
        "current_turn_datetime".to_owned(),
        json!(format_timestamp(current_ns)),
    );
    if let Some(label) = user_input_kind {
        root.insert("user_input_kind".to_owned(), json!(label));
    }
    if !user_content.is_empty() {
        root.insert("user_content".to_owned(), json!(user_content));
    }
    if !assistant_content.is_empty() {
        root.insert("assistant_content".to_owned(), json!(assistant_content));
    }
    if !memories.is_empty() {
        root.insert("memory".to_owned(), Value::Array(memories));
    }

    for role in [SectionRole::Subject, SectionRole::Environment] {
        let new_candidates = profile_candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.section_role == role)
            .map(|(index, candidate)| {
                json!({
                    "candidate_index": index,
                    "turn_id": turn_id,
                    "datetime": format_timestamp(current_ns),
                    "content": candidate.content.trim(),
                    "evidence_source": candidate.evidence_source.trim(),
                    "admission_reason": candidate.admission_reason.as_deref().unwrap_or_default().trim(),
                })
            })
            .collect::<Vec<_>>();
        if new_candidates.is_empty() {
            continue;
        }
        let section_scope = sections
            .iter()
            .find(|section| section.role == role)
            .map(|section| &section.scope_id);
        let active_nodes = current_profile_nodes
            .iter()
            .filter(|node| section_scope.is_some_and(|scope_id| node.scope_id == *scope_id))
            .map(render_profile_node_for_review)
            .collect::<Vec<_>>();
        let latest_active_datetime = current_profile_nodes
            .iter()
            .filter(|node| section_scope.is_some_and(|scope_id| node.scope_id == *scope_id))
            .map(profile_date_anchor)
            .max()
            .map(format_timestamp)
            .unwrap_or_default();
        root.insert(
            section_json_key(role).to_owned(),
            json!({
                "latest_active_datetime": latest_active_datetime,
                "active_nodes": active_nodes,
                "new_candidates": new_candidates,
            }),
        );
    }
    Value::Object(root)
}

/// Renders one active profile node for automatic post-action review.
/// 为自动 PostAction 评审渲染一项活跃画像节点。
fn render_profile_node_for_review(node: &ProfileNode) -> Value {
    json!({
        "id": node.id.as_str(),
        "datetime": format_timestamp(profile_date_anchor(node)),
        "priority": priority_label(node.priority),
        "level": profile_level_label(node.profile_level),
        "refresh_weight": node.refresh_weight,
        "content": node.content.trim(),
    })
}

/// Renders one active profile node for an explicit natural-language instruction.
/// 为显式自然语言画像指令渲染一项活跃画像节点。
fn render_profile_node_for_instruction(node: &ProfileNode) -> Value {
    json!({
        "id": node.id.as_str(),
        "datetime": format_timestamp(profile_date_anchor(node)),
        "priority": priority_label(node.priority),
        "level": profile_level_label(node.profile_level),
        "refresh_weight": node.refresh_weight,
        "source_kind": profile_source_label(node.source_kind),
        "source_id": node.source_id.as_ref().map(|id| id.as_str()),
        "content": node.content.trim(),
    })
}

/// Selects the durable date anchor shared by profile freshness and prompt display.
/// 选择画像新鲜度判断与提示词展示共享的持久日期锚点。
fn profile_date_anchor(node: &ProfileNode) -> i64 {
    node.effective_date_ns.unwrap_or(node.created_at_ns)
}

/// Joins timeline text for one exact author role without changing item order.
/// 在不改变项目顺序的前提下拼接一个精确作者角色的时间线文本。
fn timeline_content(items: &[vulcan_memorymesh_core::TimelineItem], role: Role) -> String {
    items
        .iter()
        .filter(|item| item.role == role)
        .map(|item| item.content.trim())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Formats a Unix-nanosecond timestamp into the local prompt datetime form.
/// 将 Unix 纳秒时间戳格式化为提示词使用的本地日期时间格式。
fn format_timestamp(timestamp_ns: i64) -> String {
    if timestamp_ns <= 0 {
        return String::new();
    }
    let Ok(value) = OffsetDateTime::from_unix_timestamp_nanos(i128::from(timestamp_ns)) else {
        return String::new();
    };
    let local = UtcOffset::current_local_offset()
        .map(|offset| value.to_offset(offset))
        .unwrap_or(value);
    local
        .format(format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .unwrap_or_default()
}

/// Produces a compact stable explanation for the pre-check reviewer.
/// 为 PreCheck 评审器生成紧凑且稳定的评分说明。
fn score_explanation(explanation: &vulcan_memorymesh_core::ScoreExplanation) -> String {
    format!(
        "final={:.6}; base={}; decay={}; context_delta={}; rerank={}",
        explanation.final_score,
        optional_score(explanation.base_decision_score),
        optional_score(explanation.decay_multiplier),
        optional_score(explanation.context_delta),
        optional_score(explanation.rerank_score),
    )
}

/// Formats one optional finite score without inventing a zero value.
/// 格式化一个可选有限评分且不编造零值。
fn optional_score(score: Option<f64>) -> String {
    score.map_or_else(|| "none".to_owned(), |value| format!("{value:.6}"))
}

/// Trims, removes empty strings, and preserves first-occurrence order.
/// 裁剪并移除空字符串，同时保留首次出现顺序。
fn normalized_strings(values: &[String]) -> Vec<&str> {
    let mut seen = std::collections::BTreeSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty() && seen.insert((*value).to_owned()))
        .collect()
}

/// Returns the stable prompt label for one turn classification.
/// 返回一个轮次分类的稳定提示词标签。
const fn turn_input_kind_label(kind: TurnInputKind) -> &'static str {
    match kind {
        TurnInputKind::Question => "question",
        TurnInputKind::Statement => "statement",
        TurnInputKind::Mixed => "mixed",
    }
}

/// Returns the numeric prompt value for one memory category.
/// 返回一个记忆分类的数字提示词值。
const fn memory_category_number(category: MemoryCategory) -> u32 {
    category as u32
}

/// Returns the stable prompt label for one memory ownership horizon.
/// 返回一个记忆归属跨度的稳定提示词标签。
const fn memory_scope_label(scope: MemoryScopeClass) -> &'static str {
    match scope {
        MemoryScopeClass::Session => "session",
        MemoryScopeClass::Environment => "project",
        MemoryScopeClass::Subject => "subject",
    }
}

/// Returns the stable prompt label for one memory source.
/// 返回一个记忆来源的稳定提示词标签。
const fn memory_source_label(source: MemorySourceKind) -> &'static str {
    match source {
        MemorySourceKind::Turn => "turn",
        MemorySourceKind::Direct => "direct",
        MemorySourceKind::Import => "import",
        MemorySourceKind::SystemSeed => "system_seed",
        MemorySourceKind::Legacy => "legacy",
    }
}

/// Returns the stable prompt label for one profile source.
/// 返回一个画像来源的稳定提示词标签。
const fn profile_source_label(source: ProfileSourceKind) -> &'static str {
    match source {
        ProfileSourceKind::TurnExtract => "turn_extract",
        ProfileSourceKind::ManualInstruction => "manual_instruction",
        ProfileSourceKind::SystemSeed => "system_seed",
        ProfileSourceKind::RetainedAfterSubjectDelete => "retained_after_subject_delete",
    }
}

/// Converts a normalized priority into the frozen P0/P1/P2 prompt label.
/// 将规范化优先级转换为冻结的 P0/P1/P2 提示词标签。
const fn priority_label(priority: u32) -> &'static str {
    if priority >= 100 {
        "P0"
    } else if priority >= 60 {
        "P1"
    } else {
        "P2"
    }
}

/// Converts a profile lifecycle into the frozen L0/L1/L2/L3 prompt label.
/// 将画像生命周期转换为冻结的 L0/L1/L2/L3 提示词标签。
const fn profile_level_label(level: ProfileLevel) -> &'static str {
    match level {
        ProfileLevel::Transient => "L0",
        ProfileLevel::Situational => "L1",
        ProfileLevel::Stable => "L2",
        ProfileLevel::Persistent => "L3",
    }
}

/// Maps the generic profile role onto the two native VulcanCode profile blocks.
/// 将通用画像角色映射到 VulcanCode 原生的两个画像区块。
const fn section_json_key(role: SectionRole) -> &'static str {
    match role {
        SectionRole::Subject => "user",
        SectionRole::Environment => "project",
    }
}

/// Maps the generic profile role onto the manual-instruction target label.
/// 将通用画像角色映射到手工指令目标标签。
const fn section_target_label(role: SectionRole) -> &'static str {
    match role {
        SectionRole::Subject => "USER",
        SectionRole::Environment => "PROJECT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_memorymesh_core::{LlmInput, PreCheckTurnContentType, PreCheckTurnContext, TurnId};

    /// Verifies that pre-check intent projection keeps the prompt-owned field names.
    /// 验证 PreCheck 意图投影保留提示词拥有的字段名。
    #[test]
    fn precheck_intent_uses_stable_prompt_shape() -> Result<(), Box<dyn std::error::Error>> {
        let rendered = render_llm_input(&LlmInput::PreCheckIntent {
            recent_turns: vec![PreCheckTurnContext {
                turn_id: TurnId::new("vmm-turn-7")?,
                content_type: PreCheckTurnContentType::Details,
                content: " Earlier answer. ".to_owned(),
            }],
            user_content: " Current question. ".to_owned(),
            max_search_queries: 3,
        })?;
        let value: Value = serde_json::from_str(&rendered)?;
        assert_eq!(value["current_user_input"], "Current question.");
        assert_eq!(value["max_search_queries"], 3);
        assert_eq!(value["recent_turns"][0]["turn_id"], 7);
        assert_eq!(value["recent_turns"][0]["content_type"], "DETAILS");
        assert_eq!(value["current_context_hints"][0], "Current question.");
        assert_eq!(value["recent_context_hints"][0], "Earlier answer.");
        Ok(())
    }
}
