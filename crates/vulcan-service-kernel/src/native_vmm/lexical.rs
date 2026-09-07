//! Declares native lexical scoring independently of the reference token-overlap producer.
//! 独立于参考词元重合度生产者声明原生词法评分。

use vulcan_memorymesh_core::{
    DecisionScoreMode, RawScoreKind, ScoreContractDescriptor, ScoreContractRef, ScoreDirection,
};

/// Returns the versioned Jieba/SQLite BM25 contract with external-VMM rank projection.
/// 返回采用外置 VMM 排名投影的版本化 Jieba/SQLite BM25 契约。
/// Raw values remain lower-is-better; decision values use the admitted channel's final rank and size.
/// 原始值保持越小越优；决策值使用合法候选通道的最终排名及数量。
pub(super) fn score_contract() -> ScoreContractDescriptor {
    ScoreContractDescriptor {
        contract: ScoreContractRef {
            id: "native-vmm-jieba-bm25-rank".to_owned(),
            version: 1,
        },
        raw_score_kind: RawScoreKind::Bm25,
        direction: ScoreDirection::LowerIsBetter,
        decision_score_mode: DecisionScoreMode::HostProjected,
    }
}

/// Projects a valid zero-based `rank` within `total` admitted hits to the external VMM score.
/// 将 `total` 条合法命中中有效的从零开始 `rank` 投影为外置 VMM 分数。
/// Mirrors memory_query_search.go::normalizedRankScore; no raw BM25 magnitude enters this value.
/// 对齐 memory_query_search.go::normalizedRankScore；该值不使用 BM25 原值大小。
pub(super) fn rank_score(rank: usize, total: usize) -> f64 {
    if total == 0 || rank >= total {
        return 0.0;
    }
    1.0 - rank as f64 / (total as f64 * 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Freezes singleton, tied-order and empty-channel rank semantics without token-overlap reuse.
    /// 冻结单条、同分顺序及空通道的排名语义，不复用词元重合度。
    #[test]
    fn bm25_contract_and_projection_match_external_rank_scoring() {
        assert_eq!(score_contract().raw_score_kind, RawScoreKind::Bm25);
        assert_eq!(score_contract().direction, ScoreDirection::LowerIsBetter);
        assert_eq!(
            score_contract().decision_score_mode,
            DecisionScoreMode::HostProjected
        );
        assert_eq!(rank_score(0, 1), 1.0);
        assert_eq!(
            (0..4).map(|rank| rank_score(rank, 4)).collect::<Vec<_>>(),
            vec![1.0, 0.875, 0.75, 0.625]
        );
        assert_eq!(rank_score(0, 0), 0.0);
        assert_eq!(rank_score(4, 4), 0.0);
    }
}
