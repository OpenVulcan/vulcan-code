//! Formats retrieval provenance from recorded stage facts, never memory generation source.
//! 从已记录阶段事实格式化检索来源，绝不使用记忆生成来源代替。

use vulcan_memorymesh_core::ScoreExplanation;

/// Carries the stable origin token and its reviewer-facing explanation.
/// 携带稳定来源标记及面向评审模型的解释。
pub(super) struct RetrievalOrigin {
    /// Names the observed retrieval path using external VMM tokens where applicable.
    /// 在适用时使用外置 VMM 标记命名已观察检索路径。
    pub origin: String,
    /// Labels the actual retrieval stages for the reviewer.
    /// 向评审模型标注实际检索阶段。
    pub label: String,
    /// Explains only stages whose execution is recorded in the score contract.
    /// 仅解释评分契约记录已执行的阶段。
    pub explanation: String,
}

/// Describes channel membership, RRF, rerank and MMR from the supplied score facts.
/// 根据传入评分事实描述通道命中、RRF、重排和 MMR。
/// Missing score facts remain explicitly unknown; zero scores still represent executed stages.
/// 缺失评分事实明确保持未知；零分仍代表阶段已执行。
pub(super) fn describe_retrieval_origin(score: Option<&ScoreExplanation>) -> RetrievalOrigin {
    let Some(score) = score else {
        return RetrievalOrigin {
            origin: String::new(),
            label: String::from("Unknown Retrieval Path"),
            explanation: String::from(
                "Retrieval provenance was not recorded; do not infer executed stages from memory generation source.",
            ),
        };
    };
    // channel membership is explicit in the raw-score field contract, including zero-valued scores.
    // 原始分数字段契约显式记录通道命中，包括分值为零的情况。
    let (origin, label, explanation) = match (
        score.raw_vector_score.is_some(),
        score.raw_lexical_score.is_some(),
    ) {
        (true, true) if score.rrf_score.is_some() => (
            "hybrid_rrf",
            "Hybrid RRF",
            "The candidate was returned by both vector and lexical channels and participated in RRF fusion.",
        ),
        (true, true) => (
            "vector_lexical",
            "Vector + Lexical Recall",
            "Both channels returned the candidate; RRF execution was not recorded.",
        ),
        (true, false) => (
            "vector_search",
            "Vector Recall",
            "The candidate was returned by the vector channel, not the lexical channel.",
        ),
        (false, true) => (
            "lexical_search",
            "Lexical Recall",
            "The candidate was returned by the lexical channel, not the vector channel.",
        ),
        (false, false) => (
            "",
            "Unknown Retrieval Path",
            "The original retrieval channel was not recorded.",
        ),
    };
    // result accumulates only positively recorded stages and keeps generation provenance separate.
    // result 仅累计明确记录的阶段，并与生成来源保持分离。
    let mut result = RetrievalOrigin {
        origin: origin.to_owned(),
        label: label.to_owned(),
        explanation: explanation.to_owned(),
    };
    if score.rrf_score.is_some() && origin != "hybrid_rrf" {
        result
            .explanation
            .push_str(" RRF participation is recorded for this candidate.");
    }
    if score.rerank_score.is_some() {
        result.origin = match result.origin.as_str() {
            "vector_search" => String::from("vector_rerank"),
            "lexical_search" => String::from("lexical_rerank"),
            "" => String::from("rerank"),
            _ => format!("{}_rerank", result.origin),
        };
        result.label.push_str(" + Rerank");
        result
            .explanation
            .push_str(" A rerank model evaluated the candidate.");
    }
    if score.mmr_selected == Some(true) {
        result.origin = if result.origin.is_empty() {
            String::from("mmr")
        } else {
            format!("{}_mmr", result.origin)
        };
        result.label.push_str(" + MMR");
        result
            .explanation
            .push_str(" MMR selected the candidate for diversity.");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies observed channels and optional stage execution produce source-compatible tokens.
    /// 验证已观察通道及可选阶段执行生成与来源兼容的标记。
    #[test]
    fn origin_tracks_recorded_stages_including_zero_scores() {
        for (vector, lexical, base) in [
            (true, false, "vector"),
            (false, true, "lexical"),
            (true, true, "hybrid_rrf"),
        ] {
            for rerank in [false, true] {
                for mmr in [None, Some(false), Some(true)] {
                    // score uses zero-valued observations to distinguish presence from score magnitude.
                    // score 使用零值观测区分事实存在与分数大小。
                    let score = ScoreExplanation {
                        raw_vector_score: vector.then_some(0.0),
                        raw_lexical_score: lexical.then_some(0.0),
                        rrf_score: (vector && lexical).then_some(0.0),
                        rerank_score: rerank.then_some(0.0),
                        mmr_selected: mmr,
                        ..ScoreExplanation::default()
                    };
                    // expected preserves external token spellings for each normal pipeline path.
                    // expected 保留每种正常流水线路径的外置标记拼写。
                    let mut expected = if rerank {
                        format!("{base}_rerank")
                    } else if vector && lexical {
                        base.to_owned()
                    } else {
                        format!("{base}_search")
                    };
                    if mmr == Some(true) {
                        expected.push_str("_mmr");
                    }
                    assert_eq!(describe_retrieval_origin(Some(&score)).origin, expected);
                }
            }
        }
    }

    /// Verifies absent facts do not fabricate fusion, rerank or diversity processing.
    /// 验证缺失事实不会虚构融合、重排或多样性处理。
    #[test]
    fn unknown_origin_remains_unknown_without_stage_evidence() {
        assert!(describe_retrieval_origin(None).origin.is_empty());
        assert!(
            describe_retrieval_origin(Some(&ScoreExplanation::default()))
                .origin
                .is_empty()
        );
        // unfused explicitly records two channels without claiming the RRF stage executed.
        // unfused 显式记录两个通道，但不宣称执行过 RRF 阶段。
        let unfused = ScoreExplanation {
            raw_vector_score: Some(0.3),
            raw_lexical_score: Some(0.5),
            ..ScoreExplanation::default()
        };
        assert_eq!(
            describe_retrieval_origin(Some(&unfused)).origin,
            "vector_lexical"
        );
    }
}
