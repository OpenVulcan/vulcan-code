//! Maps editable ranking settings onto the exact host-owned retrieval contracts.
//! 将可编辑排序设置映射到准确的宿主检索契约。

use vulcan_memorymesh_core::{RetrievalPolicy, ThresholdBinding};
use vulcan_service_api::VmmNativeRetrievalPolicySummary;

/// Projects Core defaults without exposing score-contract IDs or algorithm-version selectors.
/// 投影 Core 默认值，不暴露评分契约 ID 或算法版本选择器。
pub(super) fn defaults() -> VmmNativeRetrievalPolicySummary {
    // policy is the single authoritative default producer.
    // policy 是唯一的权威默认值生产者。
    let policy = RetrievalPolicy::v1();
    VmmNativeRetrievalPolicySummary {
        hybrid_enabled: policy.hybrid_enabled,
        lexical_candidate_floor: policy.lexical_candidate_floor,
        rrf_k: policy.rrf_k.to_string(),
        replace_min_similarity: policy.replace_min_similarity[0].threshold,
        hard_dedupe_cosine_threshold: policy.hard_dedupe[0].threshold,
        hard_dedupe_pool_top_k: policy.hard_dedupe_pool_top_k,
        replacement_top_k: policy.replacement_top_k,
        rerank_top_n: policy.rerank_top_n,
        mmr_enabled: policy.mmr.enabled,
        mmr_lambda: policy.mmr.lambda,
        weibull_enabled: policy.decay.enabled,
        weibull_shape: policy.decay.shape,
        weibull_scale_hours: policy.decay.scale_hours,
        weibull_min_multiplier: policy.decay.min_multiplier,
        weibull_reinforce_weight: policy.decay.reinforce_weight,
        weibull_cross_session_boost: policy.decay.cross_session_boost,
    }
}

/// Applies one complete editable ranking group; Core subsequently validates numeric and cross-field constraints.
/// 应用完整的可编辑排序组；数值与跨字段约束随后由 Core 校验。
/// Returns an error for a noncanonical or out-of-range decimal RRF constant; never clamps or rounds it.
/// RRF 常数不是规范十进制或越界时返回错误，绝不截断或舍入。
pub(super) fn apply(
    policy: &mut RetrievalPolicy,
    settings: &VmmNativeRetrievalPolicySummary,
) -> Result<(), String> {
    // rrf_k preserves all signed-64 source values without crossing a JavaScript floating-point number.
    // rrf_k 保留来源有符号64位值域，不经过 JavaScript 浮点数转换。
    let rrf_k = settings.rrf_k.parse::<u64>().map_err(|_| {
        String::from("The RRF constant must be a canonical positive decimal integer.")
    })?;
    if rrf_k == 0 || rrf_k > i64::MAX as u64 || rrf_k.to_string() != settings.rrf_k {
        return Err(String::from(
            "The RRF constant must be between 1 and 9223372036854775807 and must not contain spaces, signs or leading zeros.",
        ));
    }
    policy.hybrid_enabled = settings.hybrid_enabled;
    policy.lexical_candidate_floor = settings.lexical_candidate_floor;
    policy.rrf_k = rrf_k;
    // Replacement consumes projected reviewer scores; hard dedupe separately consumes exact cosine.
    // 替代使用投影后的审核评分；硬去重独立使用准确余弦分值。
    // Both sets bind to the actual vector producer; user text never chooses a producer identity.
    // 两组均绑定实际向量生产者；用户文本不能选择生产者身份。
    policy.replace_min_similarity = vec![ThresholdBinding {
        contract: policy.vector_contract.contract.clone(),
        threshold: settings.replace_min_similarity,
    }];
    policy.hard_dedupe = vec![ThresholdBinding {
        contract: policy.vector_contract.contract.clone(),
        threshold: settings.hard_dedupe_cosine_threshold,
    }];
    policy.hard_dedupe_pool_top_k = settings.hard_dedupe_pool_top_k;
    policy.replacement_top_k = settings.replacement_top_k;
    policy.rerank_top_n = settings.rerank_top_n;
    policy.mmr.enabled = settings.mmr_enabled;
    policy.mmr.lambda = settings.mmr_lambda;
    policy.decay.enabled = settings.weibull_enabled;
    policy.decay.shape = settings.weibull_shape;
    policy.decay.scale_hours = settings.weibull_scale_hours;
    policy.decay.min_multiplier = settings.weibull_min_multiplier;
    policy.decay.reinforce_weight = settings.weibull_reinforce_weight;
    policy.decay.cross_session_boost = settings.weibull_cross_session_boost;
    Ok(())
}
