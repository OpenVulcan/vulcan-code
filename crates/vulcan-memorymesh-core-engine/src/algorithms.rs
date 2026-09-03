//! Pure deterministic memory processing algorithms.
//! 纯确定性记忆处理算法。

use std::collections::BTreeSet;
use vulcan_memorymesh_core_contracts::SearchHit;

/// Normalizes query whitespace exactly like VMM `textutil.NormalizeWhitespace`.
/// 按 VMM `textutil.NormalizeWhitespace` 的实际实现归一化查询空白。
///
/// This is a query helper, not a body sanitizer: compatibility characters and controls remain intact.
/// 此函数只处理查询，不是正文清洗器：兼容字符与非空白控制字符保持原样。
#[must_use]
pub fn normalize_text(value: &str) -> String {
    // Go's regexp \\s is ASCII-only and excludes vertical tab; TrimSpace is Unicode-aware.
    // Go 正则的 \\s 仅包含 ASCII 空白且不含垂直制表符；TrimSpace 则处理 Unicode 首尾空白。
    value
        .trim()
        .split([' ', '\t', '\n', '\r', '\u{000c}'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Builds comparison/hash text using VMM's simple lowercase mapping and Unicode Fields.
/// 使用 VMM 的逐字符小写映射与 Unicode Fields 规则构建比较及哈希文本。
///
/// Never use this representation for persisted prose or embedding summaries.
/// 不得用该表示替换持久正文或嵌入摘要。
#[must_use]
pub fn normalize_hash_text(value: &str) -> String {
    simple_lowercase(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Applies Go-compatible simple lowercase without contextual or multi-character expansion.
/// 应用与 Go 一致的逐字符小写映射，不引入上下文变化或多字符扩展。
pub fn simple_lowercase(value: &str) -> String {
    // Per-character lowercase avoids Rust's contextual final-sigma conversion, absent in Go.
    // 逐字符小写避免 Rust 的上下文词尾 Sigma 转换，因为 Go 不进行该转换。
    value
        .chars()
        .map(|character| character.to_lowercase().next().unwrap_or(character))
        .collect::<String>()
}

/// Applies exponential half-life decay to a normalized base score.
/// 对规范化基础评分应用指数半衰期衰减。
///
/// This standalone mathematical utility is not the VMM retrieval decay policy.
/// 此独立数学辅助函数不是 VMM 检索衰减策略。
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub fn decayed_score(base_score: f64, age_ms: u64, half_life_ms: u64) -> f64 {
    if !base_score.is_finite() || !(0.0..=1.0).contains(&base_score) || half_life_ms == 0 {
        return 0.0;
    }
    // Convert millisecond counters to floating-point half-life units; sub-millisecond precision at extreme u64 ages cannot affect the public f64 score.
    // 将毫秒计数器转换为浮点半衰期单位；u64 极端年龄下的亚毫秒精度不会影响公共 f64 评分。
    let periods = age_ms as f64 / half_life_ms as f64;
    base_score * (-periods).exp2()
}

/// Removes normalized duplicate memory contents while preserving first occurrence order.
/// 删除规范化重复记忆内容，同时保持首次出现顺序。
///
/// Callers must preselect one ownership boundary; the production engine does not use this helper.
/// 调用方须先限定同一归属边界；生产引擎不使用此辅助函数。
#[must_use]
pub fn deduplicate_by_content(hits: Vec<SearchHit>) -> Vec<SearchHit> {
    // Track normalized content keys without altering the returned original text.
    // 跟踪规范化内容键而不改变返回的原始文本。
    let mut seen = BTreeSet::new();
    hits.into_iter()
        .filter(|hit| seen.insert(normalize_text(&hit.memory.content).to_lowercase()))
        .collect()
}

/// Selects diverse hits with Maximal Marginal Relevance using supplied pairwise similarities.
/// 使用调用方提供的两两相似度，通过最大边际相关性选择多样化命中。
///
/// `similarities[row][column]` must correspond to the same hit order and contain finite values.
/// `similarities[row][column]` 必须与命中顺序一致并包含有限值。
/// This matrix-based utility is separate from the production vector-aware retrieval kernel.
/// 此基于矩阵的辅助函数与生产环境使用向量的检索内核相互独立。
#[must_use]
pub fn mmr_select(
    hits: &[SearchHit],
    similarities: &[Vec<f64>],
    limit: usize,
    relevance_weight: f64,
) -> Vec<SearchHit> {
    if hits.is_empty()
        || limit == 0
        || similarities.len() != hits.len()
        || similarities.iter().any(|row| row.len() != hits.len())
        || !(0.0..=1.0).contains(&relevance_weight)
        || similarities
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
    {
        return Vec::new();
    }
    // Track selected indexes so similarity penalties reference stable source positions.
    // 跟踪已选择索引，使相似度惩罚引用稳定源位置。
    let mut selected = Vec::new();
    // Track remaining indexes in deterministic ascending order.
    // 以确定性升序跟踪剩余索引。
    let mut remaining = (0..hits.len()).collect::<BTreeSet<_>>();
    while selected.len() < limit.min(hits.len()) {
        // Evaluate every remaining candidate and retain the highest MMR score with stable tie-breaking.
        // 评估每个剩余候选，并以稳定平局规则保留最高 MMR 评分。
        let best = remaining.iter().copied().max_by(|left, right| {
            // Calculate the left candidate diversity penalty.
            // 计算左侧候选的多样性惩罚。
            let left_penalty = selected
                .iter()
                .map(|selected_index| similarities[*left][*selected_index])
                .fold(0.0_f64, f64::max);
            // Calculate the right candidate diversity penalty.
            // 计算右侧候选的多样性惩罚。
            let right_penalty = selected
                .iter()
                .map(|selected_index| similarities[*right][*selected_index])
                .fold(0.0_f64, f64::max);
            // Calculate the left MMR objective.
            // 计算左侧 MMR 目标值。
            let left_score = (1.0 - relevance_weight)
                .mul_add(-left_penalty, relevance_weight * hits[*left].final_score);
            // Calculate the right MMR objective.
            // 计算右侧 MMR 目标值。
            let right_score = (1.0 - relevance_weight)
                .mul_add(-right_penalty, relevance_weight * hits[*right].final_score);
            left_score
                .total_cmp(&right_score)
                .then_with(|| right.cmp(left))
        });
        // The input validation guarantees at least one remaining item until the limit is reached.
        // 输入校验保证在达到限制前至少存在一个剩余项目。
        let Some(best) = best else {
            break;
        };
        remaining.remove(&best);
        selected.push(best);
    }
    selected
        .into_iter()
        .map(|index| hits[index].clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{decayed_score, normalize_hash_text, normalize_text};

    /// Verifies query normalization preserves compatibility forms and non-ASCII separators.
    /// 验证查询归一化保留兼容形式与非 ASCII 分隔符。
    #[test]
    fn normalization_is_deterministic() {
        assert_eq!(normalize_text("  Ａ\tB\n C\u{0000}"), "Ａ B C\u{0000}");
        assert_eq!(
            normalize_text("\u{2003}a\u{00a0}b\u{000b}c\u{2003}"),
            "a\u{00a0}b\u{000b}c"
        );
        assert_eq!(normalize_hash_text(" Ａ\u{00a0}ΟΣ\nİ "), "ａ οσ i");
        assert_ne!(normalize_hash_text("Ａ"), normalize_hash_text("A"));
    }

    /// Verifies exact half-life decay behavior.
    /// 验证精确半衰期衰减行为。
    #[test]
    fn decay_halves_at_one_period() {
        assert!((decayed_score(0.8, 1000, 1000) - 0.4).abs() < f64::EPSILON);
        assert!(decayed_score(0.8, 1000, 0).abs() <= f64::EPSILON);
    }

    /// Verifies normalization idempotence and decay monotonicity over a deterministic corpus.
    /// 在确定性语料上验证规范化幂等性与衰减单调性。
    #[test]
    fn deterministic_properties_hold_across_boundary_inputs() {
        let corpus = [
            "",
            "  a  b  ",
            "ＡＢＣ",
            "中\u{0000}文\n测试",
            "é\u{0301}\tvalue",
            "\u{2003}\u{2002}spaced\u{00a0}",
        ];
        for value in corpus {
            let normalized = normalize_text(value);
            assert_eq!(normalize_text(&normalized), normalized);
        }
        let mut previous = decayed_score(1.0, 0, 1_000);
        for age_ms in (1..=20_000).step_by(137) {
            let current = decayed_score(1.0, age_ms, 1_000);
            assert!(current.is_finite());
            assert!((0.0..=previous).contains(&current));
            previous = current;
        }
    }
}
