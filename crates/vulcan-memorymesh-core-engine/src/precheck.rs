//! Pure `PreCheck` intent and presentation rules extracted from the frozen VMM use case.
//! 从冻结 VMM 用例提取的纯 `PreCheck` 意图及展示规则，供命令引擎使用。

use crate::algorithms::{normalize_text, simple_lowercase};
use crate::retrieval_kernel::{
    normalize_context_key, normalize_context_value, review_score, tokenize_context_query,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_memorymesh_core_config::PiiScrubber;
use vulcan_memorymesh_core_contracts::{
    CoreError, ErrorCode, ErrorStage, MemoryCategory, MemoryId, PreCheckPolicy,
    PreCheckReviewCandidate, SearchHit, TurnId,
};

/// Keeps a presentation view separate from the exact observed record used for optimistic lifecycle writes.
/// 将展示视图与乐观生命周期写入使用的精确观测记录分开保存。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// Keep the explicit crate boundary even if the containing module is reorganized later.
// 即使未来调整所属模块，也保留明确的 crate 边界，避免私有原记录被重新导出。
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct ReviewCandidateState {
    /// Retains the original representative hit without merging or redacting its durable record.
    /// 保留原始代表命中，不合并或脱敏其持久记录字段。
    pub source: SearchHit,
    /// Carries the independent redacted view exposed to L2 and final context consumers.
    /// 携带向 L2 和最终上下文消费者公开的独立脱敏视图。
    pub view: PreCheckReviewCandidate,
}

/// Accumulates only admitted unique memories while preserving first-seen order and post-merge redaction semantics.
/// 仅累计已准入唯一记忆，保留首见顺序及合并后脱敏语义，避免检查点重复保存全部查询结果。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
// This continuation is not part of the public reviewer contract.
// 此续体不属于公共审核契约，保持明确的内部可见性。
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct ReviewAccumulator {
    /// Retains the exact number of fully processed query groups.
    /// 保留已经完整处理的查询组数量。
    pub processed_groups: usize,
    /// Retains first-seen candidates with their private source records.
    /// 按首见顺序保留候选及其私有来源记录。
    candidates: Vec<ReviewCandidateState>,
    /// Marks only repeated IDs whose labels require post-redaction normalization.
    /// 仅标记需要脱敏后再规范化标签的重复身份。
    merged_ids: BTreeSet<MemoryId>,
}

/// Validates retained candidate identities and finite reviewer scores without re-running mutable external effects.
/// 校验保留候选身份及有限审核分，不重跑可能变化的外部效果；用于检查点恢复的一致性门禁。
pub fn validate_review_states(
    candidates: &[ReviewCandidateState],
    policy: &PreCheckPolicy,
) -> bool {
    let mut seen = BTreeSet::new();
    candidates.iter().all(|candidate| {
        let view = &candidate.view;
        candidate.source.memory.id == view.memory_id
            && seen.insert(&view.memory_id)
            && view.score.is_finite()
            && (policy.min_review_score..=1.0).contains(&view.score)
            && view.score.to_bits() == view.explanation.final_score.to_bits()
            && (!view.summary.is_empty() || !view.details.is_empty())
    })
}

/// Validates the streaming merge cursor and marker set against its admitted source records.
/// 根据准入来源记录校验流式合并游标及重复标记集合，拒绝恢复后跳组或伪造重复标记。
pub fn validate_review_accumulator(
    accumulator: &ReviewAccumulator,
    group: usize,
    policy: &PreCheckPolicy,
) -> bool {
    accumulator.processed_groups == group
        && (group != 0 || accumulator.candidates.is_empty())
        && validate_review_states(&accumulator.candidates, policy)
        && accumulator.merged_ids.iter().all(|id| {
            accumulator
                .candidates
                .iter()
                .any(|candidate| candidate.view.memory_id == *id)
        })
}

/// Names the exact deictic fragments in VMM's stage-one stabilization policy.
/// 列出 VMM 第一阶段稳定化策略的精确指代片段。
const GENERIC_FRAGMENTS: &[&str] = &[
    "这个",
    "这里",
    "这样",
    "这种",
    "这块",
    "这一块",
    "这段",
    "这一段",
    "这个改动",
    "这个地方",
    "这个问题",
    "这个方案",
    "这个实现",
    "这个逻辑",
    "这个流程",
    "上面",
    "前面",
    "后面",
    "刚才",
    "刚刚",
];

/// Names explicit historical or technical anchors that keep retrieval enabled.
/// 列出必须保留检索的显式历史或技术锚点。
const LONG_TERM_ANCHORS: &[&str] = &[
    "之前", "以前", "历史", "长期", "记忆", "偏好", "规则", "约定", "规范", "约束", "架构", "决策",
    "兼容", "版本", "配置", "迁移", "schema", "grpc", "http", "tls", "sqlite", "duckdb", "lancedb",
    "phase",
];

/// Stabilizes already validated L1 queries; an empty result explicitly skips retrieval.
/// 稳定化已通过输出契约验证的 L1 查询；空结果明确表示跳过检索。
pub fn normalize_intent_queries(
    queries: Vec<String>,
    current: &str,
    has_recent_turns: bool,
    limit: usize,
) -> Vec<String> {
    let current = normalize_text(current);
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for query in queries {
        let mut query = normalize_text(&query);
        if query.is_empty() {
            continue;
        }
        let lower = simple_lowercase(&query);
        if contains_marker(&lower, GENERIC_FRAGMENTS)
            && !has_explicit_anchor(&lower)
            && !current.is_empty()
        {
            query.clone_from(&current);
        }
        if seen.insert(simple_lowercase(&query)) {
            normalized.push(query);
        }
    }
    // The current request is an explicit source-defined replacement, not a guessed query.
    // 当前请求是来源规则明确允许的替代文本，不是凭猜测生成的新查询。
    if normalized.is_empty() && !current.is_empty() {
        normalized.push(current.clone());
    }
    if has_recent_turns && normalized.len() == 1 && normalized[0] == current {
        let lower = simple_lowercase(&current);
        if contains_marker(&lower, GENERIC_FRAGMENTS)
            && !has_explicit_anchor(&lower)
            && current.chars().count() <= 18
        {
            return Vec::new();
        }
    }
    normalized.truncate(limit);
    normalized
}

/// Reports exact substring markers on the source-normalized lowercase surface.
/// 在来源规则归一化的小写表面检查精确子串标记。
fn contains_marker(text: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| text.contains(marker))
}

/// Recognizes source-compatible Han or ASCII anchors without guessing from punctuation.
/// 按来源规则识别汉字或 ASCII 锚点，不从标点猜测含义。
fn has_explicit_anchor(query: &str) -> bool {
    if contains_marker(query, LONG_TERM_ANCHORS) {
        return true;
    }
    let mut strong_tokens = 0;
    for token in tokenize_context_query(query) {
        if contains_marker(&token, GENERIC_FRAGMENTS) {
            continue;
        }
        if token.len() >= 2
            && token.chars().any(|value| {
                value.is_ascii_lowercase() || value.is_ascii_digit() || value == '_' || value == '-'
            })
        {
            return true;
        }
        if token.chars().count() >= 2 {
            strong_tokens += 1;
            if strong_tokens >= 2 {
                return true;
            }
        }
    }
    false
}

/// Projects and gates each query independently, merges views, then redacts before stable reviewer truncation.
/// 独立投影及筛选各查询，合并展示后脱敏，最后稳定排序及送审截断；原记录始终不变。
#[allow(clippy::float_cmp)]
pub fn accumulate_review_group(
    accumulator: &mut ReviewAccumulator,
    hits: Vec<SearchHit>,
    policy: &PreCheckPolicy,
) -> Result<(), CoreError> {
    let mut positions = accumulator
        .candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.view.memory_id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let candidates = &mut accumulator.candidates;
    let merged_ids = &mut accumulator.merged_ids;
    let total = hits.len();
    for (index, hit) in hits.into_iter().enumerate() {
        if !hit.final_score.is_finite() {
            return Err(CoreError::new(
                ErrorCode::Internal,
                ErrorStage::Command,
                "PreCheck pipeline score is nonfinite",
            ));
        }
        let original = hit.final_score.clamp(0.0, 1.0);
        let score = review_score(
            hit.final_score,
            hit.rerank_score.is_some(),
            index + 1,
            total,
        );
        if score < policy.min_review_score {
            continue;
        }
        let summary = hit.memory.summary.trim().to_owned();
        let mut details = hit.memory.content.trim().to_owned();
        if summary.is_empty() && details.is_empty() {
            continue;
        }
        if details.is_empty() {
            details.clone_from(&summary);
        }
        let mut explanation = hit.explanation.clone().ok_or_else(|| {
            CoreError::new(
                ErrorCode::Internal,
                ErrorStage::Command,
                "PreCheck pipeline hit has no score explanation",
            )
        })?;
        explanation.pre_review_score = Some(original);
        explanation.final_score = score;
        let view = PreCheckReviewCandidate {
            memory_id: hit.memory.id.clone(),
            source_turn_id: hit.memory.source_turn_id.clone(),
            created_at_ns: hit.memory.created_at_ns,
            source_kind: hit.memory.source_kind,
            scope_class: hit.memory.scope_class,
            category: hit.memory.category,
            summary,
            details,
            score,
            explanation,
        };
        let candidate = ReviewCandidateState { source: hit, view };
        if let Some(position) = positions.get(&candidate.view.memory_id).copied() {
            merged_ids.insert(candidate.view.memory_id.clone());
            merge_review_candidate(&mut candidates[position], candidate);
        } else {
            positions.insert(candidate.view.memory_id.clone(), candidates.len());
            candidates.push(candidate);
        }
    }
    accumulator.processed_groups += 1;
    Ok(())
}

/// Redacts the fully merged candidate set once, then applies the reviewer-only stable limit.
/// 仅对完整合并后的候选集合脱敏一次，再应用审核专用稳定数量上限。
#[allow(clippy::float_cmp)]
pub fn finish_review_candidates(
    accumulator: ReviewAccumulator,
    policy: &PreCheckPolicy,
    scrubber: &PiiScrubber,
) -> Result<Vec<ReviewCandidateState>, CoreError> {
    let ReviewAccumulator {
        mut candidates,
        merged_ids,
        ..
    } = accumulator;
    // Mask only the merged view; the raw source remains private for versioned writes and checkpoint recovery.
    // 只对合并视图脱敏；原始来源仍为私有数据，用于版本写入和检查点恢复。
    for candidate in &mut candidates {
        let view = &mut candidate.view;
        view.summary = scrubber.scrub(&view.summary)?.text;
        view.details = scrubber.scrub(&view.details)?.text;
        for label in &mut view.explanation.context_matched_values {
            *label = scrubber.scrub(label)?.text;
        }
        if merged_ids.contains(&view.memory_id) && view.explanation.context_matched_values.len() > 1
        {
            normalize_review_labels(&mut view.explanation.context_matched_values);
        }
        if view.details.is_empty() {
            view.details.clone_from(&view.summary);
        }
    }
    candidates.retain(|candidate| {
        !candidate.view.summary.is_empty() || !candidate.view.details.is_empty()
    });
    candidates.sort_by(|left, right| {
        if left.view.score == right.view.score {
            std::cmp::Ordering::Equal
        } else {
            right.view.score.total_cmp(&left.view.score)
        }
    });
    candidates.truncate(policy.review_candidate_limit as usize);
    Ok(candidates)
}

/// Merges repeated presentation facts while retaining the chosen representative's unmodified source record.
/// 合并重复展示事实，同时保留所选代表的未改动原记录；计数取最大值而不是累加。
fn merge_review_candidate(current: &mut ReviewCandidateState, mut incoming: ReviewCandidateState) {
    let left_delta = current.view.explanation.context_delta;
    let right_delta = incoming.view.explanation.context_delta;
    let delta = if right_delta.unwrap_or(0.0).abs() > left_delta.unwrap_or(0.0).abs() {
        right_delta
    } else {
        left_delta
    };
    if prefer_incoming(&current.view, &incoming.view) {
        std::mem::swap(current, &mut incoming);
    }
    let primary = &mut current.view;
    let secondary = &incoming.view;
    if secondary.summary.len() > primary.summary.len() {
        primary.summary.clone_from(&secondary.summary);
    }
    if secondary.details.len() > primary.details.len() {
        primary.details.clone_from(&secondary.details);
    }
    if primary.source_turn_id.is_none() {
        primary.source_turn_id.clone_from(&secondary.source_turn_id);
    }
    if primary.created_at_ns == 0 && secondary.created_at_ns > 0 {
        primary.created_at_ns = secondary.created_at_ns;
    }
    if primary.category == MemoryCategory::General {
        primary.category = secondary.category;
    }
    let primary = &mut primary.explanation;
    let secondary = &secondary.explanation;
    primary.context_matched_support_count = primary
        .context_matched_support_count
        .max(secondary.context_matched_support_count);
    primary.context_matched_rebuttal_count = primary
        .context_matched_rebuttal_count
        .max(secondary.context_matched_rebuttal_count);
    primary.context_total_support_count = primary
        .context_total_support_count
        .max(secondary.context_total_support_count);
    primary.context_total_rebuttal_count = primary
        .context_total_rebuttal_count
        .max(secondary.context_total_rebuttal_count);
    primary.context_delta = delta;
    primary
        .context_matched_values
        .extend(secondary.context_matched_values.iter().cloned());
    normalize_review_labels(&mut primary.context_matched_values);
}

/// Canonicalizes merged key/value labels using the existing VMM context surface, retaining untouched single-hit labels.
/// 按既有 VMM 情境表面规范化合并后的键值标签，不改动未经合并的单次命中标签。
fn normalize_review_labels(values: &mut Vec<String>) {
    *values = values
        .iter()
        .filter_map(|raw| {
            let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
            let label = if let Some((key, value)) = normalized.split_once('=') {
                let key = normalize_context_key(key);
                let value = normalize_context_value(value);
                if value.is_empty() {
                    return None;
                }
                if key.is_empty() {
                    value
                } else {
                    format!("{key}={value}")
                }
            } else {
                normalize_context_value(&normalized)
            };
            (!label.is_empty()).then_some(label)
        })
        .collect();
    values.sort();
    values.dedup();
}

/// Compacts only final presentation, preserving all accepted records for lifecycle write-back.
/// 仅压缩最终展示；全部已采纳记录仍保留给生命周期写回，不能按展示条数减少强化。
pub fn assemble_context(
    accepted: &[ReviewCandidateState],
) -> (Vec<PreCheckReviewCandidate>, String) {
    let mut representatives: Vec<&PreCheckReviewCandidate> = Vec::new();
    let mut turn_positions: BTreeMap<&TurnId, usize> = BTreeMap::new();
    for candidate in accepted {
        let hit = &candidate.view;
        if let Some(turn_id) = &hit.source_turn_id {
            if let Some(position) = turn_positions.get(turn_id) {
                if prefer_incoming(representatives[*position], hit) {
                    representatives[*position] = hit;
                }
                continue;
            }
            turn_positions.insert(turn_id, representatives.len());
        }
        representatives.push(hit);
    }
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    let mut lines = Vec::new();
    for hit in representatives {
        let text = memory_text(&hit.summary, &hit.details);
        if !text.is_empty() && seen.insert(simple_lowercase(&normalize_text(&text))) {
            items.push(hit.clone());
            lines.push(format!("- {text}"));
        }
    }
    (items, lines.join("\n"))
}

/// Joins distinct abstract and detail text using the exact VMM final-injection comparison surface.
/// 使用 VMM 最终注入的精确比较规则连接不同的摘要与详情，保留原正文排版。
fn memory_text(summary: &str, details: &str) -> String {
    let summary = summary.trim();
    let details = details.trim();
    if summary.is_empty() {
        return details.to_owned();
    }
    if details.is_empty()
        || simple_lowercase(&normalize_text(summary)) == simple_lowercase(&normalize_text(details))
    {
        return summary.to_owned();
    }
    format!("{summary}\n{details}")
}

/// Chooses the richer same-turn representative without moving its first-selected position.
/// 选择信息更充分的同轮次代表项，同时保留该轮次首次被选中的位置。
#[allow(clippy::float_cmp)]
fn prefer_incoming(existing: &PreCheckReviewCandidate, incoming: &PreCheckReviewCandidate) -> bool {
    let score_order = if incoming.score == existing.score {
        std::cmp::Ordering::Equal
    } else {
        incoming.score.total_cmp(&existing.score)
    };
    score_order
        .then_with(|| {
            context_richness(incoming)
                .0
                .total_cmp(&context_richness(existing).0)
        })
        .then_with(|| {
            context_richness(incoming)
                .1
                .cmp(&context_richness(existing).1)
        })
        .then_with(|| origin_richness(incoming).cmp(&origin_richness(existing)))
        .then_with(|| {
            incoming
                .details
                .trim()
                .len()
                .cmp(&existing.details.trim().len())
        })
        .then_with(|| {
            incoming
                .summary
                .trim()
                .len()
                .cmp(&existing.summary.trim().len())
        })
        .is_gt()
}

/// Reads optional evidence facts as absent, rather than inventing signed contextual support.
/// 将可选证据事实缺省视为缺失，不编造带符号情境支持。
fn context_richness(hit: &PreCheckReviewCandidate) -> (f64, usize) {
    (
        hit.explanation.context_delta.unwrap_or(0.0).abs(),
        hit.explanation.context_matched_values.len(),
    )
}

/// Derives VMM origin richness from typed channel/rerank/MMR facts, not string-name guesses.
/// 从强类型通道、重排及 MMR 事实推导 VMM 来源丰富度，不猜测字符串名称。
fn origin_richness(hit: &PreCheckReviewCandidate) -> u8 {
    let explanation = &hit.explanation;
    let vector = explanation.raw_vector_score.is_some();
    let lexical = explanation.raw_lexical_score.is_some();
    let rerank = explanation.rerank_score.is_some();
    let mmr = explanation.mmr_selected == Some(true);
    if !vector && !lexical && !rerank && !mmr {
        return 0;
    }
    1 + if vector && lexical {
        4
    } else if vector || lexical {
        2
    } else {
        0
    } + u8::from(rerank) * 2
        + u8::from(mmr)
}

#[cfg(test)]
mod tests {
    use super::{memory_text, normalize_intent_queries};

    /// Preserves distinct details while collapsing formatting-equivalent summaries exactly once.
    /// 保留不同详情，同时仅一次呈现格式等价的摘要。
    #[test]
    fn final_memory_text_preserves_source_rules() {
        assert_eq!(
            memory_text(
                "SQLite schema 13 compatibility",
                " sqlite   schema 13\ncompatibility "
            ),
            "SQLite schema 13 compatibility"
        );
        assert_eq!(
            memory_text(" summary ", " detail\n  code "),
            "summary\ndetail\n  code"
        );
        assert_eq!(memory_text("", "detail"), "detail");
        assert_eq!(memory_text("summary", ""), "summary");
        assert_eq!(memory_text("", ""), "");
        assert_eq!(memory_text("ΟΣ", "οσ"), "ΟΣ");
    }

    /// Mirrors frozen VMM generic-query cases and protects explicit anchors and recent-window rules.
    /// 对照冻结 VMM 指代查询案例，并保护显式锚点与最近窗口规则。
    #[test]
    fn intent_queries_preserve_source_stabilization() {
        let current = "这个改动会不会影响 SQLite schema 13 的兼容性？";
        assert_eq!(
            normalize_intent_queries(vec!["这个改动为什么这样做".into()], current, true, 3),
            vec![current]
        );
        assert!(
            normalize_intent_queries(
                vec!["这里为什么这么改？".into()],
                "这里为什么这么改？",
                true,
                3
            )
            .is_empty()
        );
        assert_eq!(
            normalize_intent_queries(
                vec!["这里为什么这么改？".into()],
                "这里为什么这么改？",
                false,
                3
            ),
            vec!["这里为什么这么改？"]
        );
        assert_eq!(
            normalize_intent_queries(
                vec![
                    "SQLite schema 13 compatibility".into(),
                    " sqlite   schema 13 compatibility ".into(),
                    "SQLITE\nschema 13 compatibility".into()
                ],
                "在 phase4 下应该继续用哪个方案？",
                true,
                3
            ),
            vec!["SQLite schema 13 compatibility"]
        );
        for anchor in ["这里的历史规则？", "这里 foo42？", "这里 北京 上海？"] {
            assert_eq!(
                normalize_intent_queries(vec![anchor.into()], anchor, true, 3),
                vec![anchor]
            );
        }
        assert_eq!(
            normalize_intent_queries(
                vec!["这里为什么这么改？".into(), "长期约定".into()],
                "这里为什么这么改？",
                true,
                1
            ),
            vec!["这里为什么这么改？"]
        );
    }
}
