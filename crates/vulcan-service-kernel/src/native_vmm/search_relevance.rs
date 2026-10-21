//! Applies the relevance floor to native memory search hits before they reach callers.
//! 在原生记忆搜索命中交给调用方之前应用相关性下限。

use vulcan_memorymesh_core::SearchHit;

/// Minimum rerank score a memory search hit needs before it is returned.
/// 记忆搜索命中被返回前所需的最低重排评分。
/// Rerank scores are calibrated relevance in `[0, 1]`; lower hits are noise for the model.
/// 重排评分是 `[0, 1]` 内的校准相关度；低于该值的命中对模型只是噪声。
pub(super) const MEMORY_SEARCH_MIN_RERANK_SCORE: f64 = 0.8;

/// Keeps the hits of one query group that clear the relevance floor, preserving their order.
/// 保留单个查询组中达到相关性下限的命中，并保持原有顺序。
pub(super) fn relevant_search_hits(hits: &[SearchHit]) -> Vec<&SearchHit> {
    retain_relevant(hits, |hit| (hit.rerank_score, hit.final_score))
}

/// Filters only when a reranker scored the group: rank-normalized scores carry no
/// calibrated relevance, so a floor on them would drop hits arbitrarily.
/// 仅当重排器为该组评分时才过滤：按名次归一化的评分不含校准相关度，
/// 对其设置下限会任意丢弃命中。
/// Inside a reranked group, a hit without its own rerank score was never judged and is dropped.
/// 在已重排的组中，没有自身重排评分的命中未经评判，予以丢弃。
fn retain_relevant<T>(items: &[T], scores: impl Fn(&T) -> (Option<f64>, f64)) -> Vec<&T> {
    let reranked = items.iter().any(|item| scores(item).0.is_some());
    items
        .iter()
        .filter(|item| {
            let (rerank_score, final_score) = scores(item);
            !reranked || (rerank_score.is_some() && final_score >= MEMORY_SEARCH_MIN_RERANK_SCORE)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{MEMORY_SEARCH_MIN_RERANK_SCORE, retain_relevant};

    fn kept(items: &[(Option<f64>, f64)]) -> Vec<(Option<f64>, f64)> {
        retain_relevant(items, |item| *item)
            .into_iter()
            .copied()
            .collect()
    }

    // A reranked group keeps only judged hits at or above the floor, in order.
    // 已重排的组只保留已评判且不低于下限的命中，并保持顺序。
    #[test]
    fn reranked_group_drops_hits_below_floor() {
        let floor = MEMORY_SEARCH_MIN_RERANK_SCORE;
        let items = [
            (Some(0.95), 0.95),
            (Some(0.18), 0.18),
            (Some(floor), floor),
            (None, 0.9),
        ];
        assert_eq!(kept(&items), vec![(Some(0.95), 0.95), (Some(floor), floor)]);
    }

    // Without a reranker the scores are rank-derived and every hit is kept.
    // 没有重排器时评分来自名次，所有命中都保留。
    #[test]
    fn rank_only_group_keeps_every_hit() {
        let items = [(None, 1.0), (None, 0.5)];
        assert_eq!(kept(&items), items.to_vec());
    }
}
