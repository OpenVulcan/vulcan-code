//! Deterministic memory algorithms and the shared Command/Effect/Resume engine.
//! 确定性记忆算法与共享 Command/Effect/Resume 引擎。

mod algorithms;
mod checkpoint_codec;
mod engine;
mod precheck;
#[cfg(test)]
mod precheck_tests;
mod query_batch;
mod query_case;
mod retrieval_kernel;
#[cfg(test)]
mod rrf_width_tests;
mod weibull_math;

pub use algorithms::{decayed_score, deduplicate_by_content, mmr_select, normalize_text};
pub use engine::{CommandEngine, estimate_text_tokens};
pub use retrieval_kernel::{
    ContextScore, context_score, hard_duplicate_similarity, normalized_rank_score,
    passes_threshold, review_score, rrf_fusion, weibull_multiplier,
};
