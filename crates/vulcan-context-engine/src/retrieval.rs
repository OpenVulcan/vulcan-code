//! Provider-neutral retrieval fusion, ranking, and context selection.
//! 不绑定供应商的检索融合、排序和上下文选择。

use crate::model::SearchHit;
use crate::query::QueryIntent;
use std::collections::{BTreeMap, BTreeSet};

/// One ranked result list with an explicit RRF weight.
/// 一组带有明确 RRF 权重的有序结果列表。
#[derive(Debug, Clone, PartialEq)]
pub struct RankedHits {
    /// Weight applied to this ranked list during fusion.
    /// 融合时应用于该结果列表的权重。
    pub weight: f32,
    /// Hits in descending source rank order.
    /// 按来源排序从高到低排列的命中结果。
    pub hits: Vec<SearchHit>,
}

/// Reciprocal Rank Fusion parameters.
/// Reciprocal Rank Fusion 参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RrfConfig {
    /// Rank constant that dampens lower-ranked results.
    /// 抑制低排名结果的 RRF 排名常数。
    pub rrf_k: f32,
    /// Weight assigned to the original query list.
    /// 分配给原始查询列表的权重。
    pub original_query_weight: f32,
    /// Weight assigned to facet query lists.
    /// 分配给分面查询列表的权重。
    pub facet_query_weight: f32,
}

impl Default for RrfConfig {
    /// Return the documented OCE-inspired fusion defaults.
    /// 返回 OCE 思路中记录的默认融合参数。
    fn default() -> Self {
        Self {
            rrf_k: 60.0,
            original_query_weight: 1.0,
            facet_query_weight: 0.75,
        }
    }
}

/// Coverage-aware final context selection limits.
/// 具备覆盖度意识的最终上下文选择限制。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverageConfig {
    /// Maximum number of selected chunks from one path.
    /// 单一路径最多选择的代码块数量。
    pub max_per_path: usize,
    /// Hard total character budget for selected context.
    /// 最终上下文的硬性总字符预算。
    pub max_chars: usize,
    /// Same-path line overlap ratio at which a hit is suppressed.
    /// 同一路径行区间重叠达到该比例时抑制后续命中。
    pub overlap_threshold: f32,
    /// Soft result count limit enforced as a hard selection cap.
    /// 作为硬选择上限执行的结果数量限制。
    pub max_results: usize,
}

impl Default for CoverageConfig {
    /// Return the documented OCE-inspired coverage defaults.
    /// 返回 OCE 思路中记录的默认覆盖度参数。
    fn default() -> Self {
        Self {
            max_per_path: 2,
            max_chars: 32_000,
            overlap_threshold: 0.6,
            max_results: 10,
        }
    }
}

/// A complete provider-independent ranking and selection pipeline.
/// 完整的不依赖供应商的排序和选择管线。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetrievalPipeline {
    /// RRF fusion configuration.
    /// RRF 融合配置。
    pub rrf: RrfConfig,
    /// Source priority and coverage configuration.
    /// 源码优先级和覆盖度配置。
    pub coverage: CoverageConfig,
    /// Effective score floor for weak matches.
    /// 弱匹配的有效分数下限。
    pub confidence_floor: Option<f32>,
    /// Score boost applied when an auxiliary path hit covers the same Blob.
    /// 辅助路径命中覆盖相同 Blob 时应用的分数增益。
    pub path_boost_weight: f32,
}

/// Callback that returns candidate indexes in reranker order.
/// 返回重排器候选顺序下标的回调。
pub type RerankCallback = dyn Fn(&[SearchHit]) -> Vec<usize>;

impl Default for RetrievalPipeline {
    /// Return a pipeline using the extracted OCE-inspired defaults.
    /// 返回使用提取 OCE 默认值的检索管线。
    fn default() -> Self {
        Self {
            rrf: RrfConfig::default(),
            coverage: CoverageConfig::default(),
            confidence_floor: None,
            path_boost_weight: 0.1,
        }
    }
}

impl RetrievalPipeline {
    /// Fuse, optionally rerank, prioritize, filter, and coverage-select candidates.
    /// 融合候选结果，可选重排，再执行优先级、过滤和覆盖度选择。
    pub fn finalize(
        &self,
        ranked_lists: &[RankedHits],
        path_hits: &[SearchHit],
        intent: QueryIntent,
        identifiers: &[String],
        reranker: Option<&RerankCallback>,
    ) -> Vec<SearchHit> {
        self.finalize_with_path_fallbacks(
            ranked_lists,
            path_hits,
            &[],
            intent,
            identifiers,
            reranker,
        )
    }

    /// Finalize retrieval while allowing the host to provide first-Chunk path fallbacks.
    /// 在允许宿主提供文件首个代码块路径回填的情况下完成检索。
    pub fn finalize_with_path_fallbacks(
        &self,
        ranked_lists: &[RankedHits],
        path_hits: &[SearchHit],
        path_fallback_hits: &[SearchHit],
        intent: QueryIntent,
        identifiers: &[String],
        reranker: Option<&RerankCallback>,
    ) -> Vec<SearchHit> {
        let fused = reciprocal_rank_fusion(ranked_lists, self.rrf.rrf_k);
        self.finalize_candidates_with_path_fallbacks(
            fused,
            path_hits,
            path_fallback_hits,
            intent,
            identifiers,
            reranker,
        )
    }

    /// Finalize already fused candidates without changing their score scale.
    /// 在不改变候选分数尺度的前提下完成已融合候选的后处理。
    pub fn finalize_candidates_with_path_fallbacks(
        &self,
        candidates: Vec<SearchHit>,
        path_hits: &[SearchHit],
        path_fallback_hits: &[SearchHit],
        intent: QueryIntent,
        identifiers: &[String],
        reranker: Option<&RerankCallback>,
    ) -> Vec<SearchHit> {
        let candidates = self.prepare_candidates(candidates, path_hits, path_fallback_hits, intent);
        self.finalize_prepared_candidates(
            candidates,
            intent,
            identifiers,
            intent == QueryIntent::Symbol,
            reranker,
        )
    }

    /// Apply every deterministic transform that must precede host Rerank indexing.
    /// 应用宿主重排下标解释前必须完成的全部确定性变换。
    pub(crate) fn prepare_candidates(
        &self,
        mut candidates: Vec<SearchHit>,
        path_hits: &[SearchHit],
        path_fallback_hits: &[SearchHit],
        intent: QueryIntent,
    ) -> Vec<SearchHit> {
        apply_path_boost(&mut candidates, path_hits, self.path_boost_weight);
        append_path_fallbacks(&mut candidates, path_hits, path_fallback_hits);
        apply_source_priority(&mut candidates, intent);
        candidates
    }

    /// Apply host Rerank order and post-Rerank filters without rebuilding candidate order.
    /// 应用宿主重排顺序及其后的过滤步骤，且不重新构建候选顺序。
    pub(crate) fn finalize_prepared_candidates(
        &self,
        mut candidates: Vec<SearchHit>,
        intent: QueryIntent,
        identifiers: &[String],
        promote_endpoint_definitions: bool,
        reranker: Option<&RerankCallback>,
    ) -> Vec<SearchHit> {
        if let Some(reranker) = reranker {
            candidates = reorder_by_rerank(&candidates, reranker);
        }
        if promote_endpoint_definitions {
            promote_endpoints(&mut candidates, intent, identifiers);
        }
        if let Some(floor) = self.confidence_floor {
            candidates.retain(|hit| hit.score >= floor);
        }
        coverage_select_in_order(candidates, self.coverage)
    }
}

/// Fuse ranked lists by stable source-occurrence identity.
/// 根据稳定源码出现位置身份融合多个有序结果列表。
pub fn reciprocal_rank_fusion(lists: &[RankedHits], rrf_k: f32) -> Vec<SearchHit> {
    let rank_constant = if rrf_k.is_finite() && rrf_k >= 0.0 {
        rrf_k
    } else {
        60.0
    };
    let active_lists = lists
        .iter()
        .filter(|list| list.weight.is_finite() && list.weight > 0.0 && !list.hits.is_empty())
        .collect::<Vec<_>>();
    if active_lists.is_empty() {
        return Vec::new();
    }
    if active_lists.len() == 1 {
        return active_lists[0].hits.clone();
    }

    let maximum_score = active_lists
        .iter()
        .map(|list| f64::from(list.weight) / (f64::from(rank_constant) + 1.0))
        .sum::<f64>();
    let mut fused = BTreeMap::<String, SearchHit>::new();
    let mut scores = BTreeMap::<String, f64>::new();
    let mut first_seen = BTreeMap::<String, usize>::new();
    let mut ordinal = 0usize;
    for list in active_lists {
        for (rank, hit) in list.hits.iter().enumerate() {
            let key = hit.occurrence_key();
            let contribution =
                f64::from(list.weight) / (f64::from(rank_constant) + rank as f64 + 1.0);
            *scores.entry(key.clone()).or_insert(0.0) += contribution;
            first_seen.entry(key.clone()).or_insert_with(|| {
                let current = ordinal;
                ordinal = ordinal.saturating_add(1);
                current
            });
            fused
                .entry(key)
                .and_modify(|existing| {
                    if hit_kind_priority(hit.kind) > hit_kind_priority(existing.kind) {
                        existing.kind = hit.kind;
                    }
                    if hit.score > existing.score {
                        existing.score = hit.score;
                    }
                })
                .or_insert_with(|| {
                    let mut copy = hit.clone();
                    copy.source = crate::model::SearchSource::Fused;
                    copy
                });
        }
    }

    let mut output = fused
        .into_iter()
        .map(|(key, mut hit)| {
            hit.score = (scores.get(&key).copied().unwrap_or(0.0) / maximum_score) as f32;
            let ordinal = first_seen.get(&key).copied().unwrap_or(usize::MAX);
            (ordinal, hit)
        })
        .collect::<Vec<_>>();
    output.sort_by(|(left_ordinal, left), (right_ordinal, right)| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left_ordinal.cmp(right_ordinal))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.range.start_line.cmp(&right.range.start_line))
            .then_with(|| left.start_char_offset.cmp(&right.start_char_offset))
            .then_with(|| left.id.cmp(&right.id))
    });
    output.into_iter().map(|(_, hit)| hit).collect()
}

/// Merge exact-symbol recall with semantic recall without treating both channels as query facets.
/// 合并精确符号召回与语义召回，但不把两个通道错误地视为查询分面。
pub fn merge_semantic_and_exact(
    semantic_hits: Vec<SearchHit>,
    exact_hits: Vec<SearchHit>,
    intent: QueryIntent,
    candidate_limit: usize,
) -> Vec<SearchHit> {
    if candidate_limit == 0 {
        return Vec::new();
    }
    if semantic_hits.is_empty() {
        let mut exact_hits = exact_hits;
        sort_hits(&mut exact_hits);
        exact_hits.truncate(candidate_limit);
        return exact_hits;
    }
    if exact_hits.is_empty() {
        let mut semantic_hits = semantic_hits;
        semantic_hits.truncate(candidate_limit);
        return semantic_hits;
    }

    if intent == QueryIntent::CallChain {
        return merge_call_chain_hits(semantic_hits, exact_hits, candidate_limit);
    }

    let mut merged = Vec::<SearchHit>::new();
    let mut positions = BTreeMap::<String, usize>::new();
    for hit in exact_hits.into_iter().chain(semantic_hits) {
        let key = hit.occurrence_key();
        match positions.get(&key).copied() {
            Some(position) => {
                let existing = &mut merged[position];
                if hit.score > existing.score {
                    existing.score = hit.score;
                }
                if hit_kind_priority(hit.kind) > hit_kind_priority(existing.kind) {
                    existing.kind = hit.kind;
                }
                existing.source = crate::model::SearchSource::Fused;
            }
            None => {
                positions.insert(key, merged.len());
                merged.push(hit);
            }
        }
    }
    sort_hits(&mut merged);
    merged.truncate(candidate_limit);
    merged
}

/// Preserve semantic call-chain context while admitting a bounded exact-only reserve.
/// 在保留语义调用链上下文的同时接纳受限的纯精确命中席位。
fn merge_call_chain_hits(
    semantic_hits: Vec<SearchHit>,
    exact_hits: Vec<SearchHit>,
    candidate_limit: usize,
) -> Vec<SearchHit> {
    let semantic_keys = semantic_hits
        .iter()
        .map(SearchHit::occurrence_key)
        .collect::<BTreeSet<_>>();
    let mut exact_only = exact_hits
        .into_iter()
        .filter(|hit| !semantic_keys.contains(&hit.occurrence_key()))
        .collect::<Vec<_>>();
    sort_hits(&mut exact_only);

    let reserve = if candidate_limit > 1 {
        exact_only
            .len()
            .min((candidate_limit / 3).max(1))
            .min(candidate_limit - 1)
    } else {
        0
    };
    if reserve == 0 {
        return semantic_hits.into_iter().take(candidate_limit).collect();
    }

    let semantic_slots = candidate_limit - reserve;
    let anchor_index = semantic_slots.min(semantic_hits.len()).saturating_sub(1);
    let anchor_score = semantic_hits.get(anchor_index).map_or(0.0, |hit| hit.score);
    for hit in exact_only.iter_mut().take(reserve) {
        hit.score = hit.score.min(anchor_score);
    }
    exact_only.truncate(reserve);

    let mut merged = semantic_hits;
    merged.extend(exact_only);
    sort_hits(&mut merged);
    merged.truncate(candidate_limit);
    merged
}

/// Add a bounded path-index boost to content hits from the same Blob.
/// 为来自同一 Blob 的内容命中增加有界路径索引提升分数。
pub fn apply_path_boost(content_hits: &mut [SearchHit], path_hits: &[SearchHit], weight: f32) {
    if !weight.is_finite() || weight <= 0.0 {
        return;
    }
    let mut path_scores = BTreeMap::<String, f32>::new();
    for hit in path_hits {
        path_scores
            .entry(hit.blob_name.clone())
            .and_modify(|score| *score = score.max(hit.score))
            .or_insert(hit.score);
    }
    for hit in content_hits {
        if let Some(path_score) = path_scores.get(&hit.blob_name) {
            let boosted = f64::from(hit.score) + f64::from(*path_score) * f64::from(weight);
            hit.score = boosted.clamp(f64::from(f32::MIN), f64::from(f32::MAX)) as f32;
        }
    }
}

/// Append one host-supplied first-Chunk fallback for each uncovered path Blob.
/// 为每个没有内容命中的路径 Blob 添加一个由宿主提供的首个代码块回填。
pub fn append_path_fallbacks(
    content_hits: &mut Vec<SearchHit>,
    path_hits: &[SearchHit],
    path_fallback_hits: &[SearchHit],
) {
    let covered_blobs = content_hits
        .iter()
        .map(|hit| hit.blob_name.clone())
        .collect::<BTreeSet<_>>();
    let mut fallback_by_blob = BTreeMap::<String, SearchHit>::new();
    for hit in path_fallback_hits {
        if hit.source == crate::model::SearchSource::Path {
            continue;
        }
        fallback_by_blob
            .entry(hit.blob_name.clone())
            .and_modify(|existing| {
                let current_key = (
                    existing.range.start_line,
                    existing.range.end_line,
                    existing.start_char_offset,
                    &existing.id,
                );
                let candidate_key = (
                    hit.range.start_line,
                    hit.range.end_line,
                    hit.start_char_offset,
                    &hit.id,
                );
                if candidate_key < current_key {
                    *existing = hit.clone();
                }
            })
            .or_insert_with(|| hit.clone());
    }

    let mut selected_blobs = covered_blobs;
    let mut ordered_paths = path_hits.iter().collect::<Vec<_>>();
    ordered_paths.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.id.cmp(&right.id))
    });
    for path_hit in ordered_paths {
        if !selected_blobs.insert(path_hit.blob_name.clone()) {
            continue;
        }
        let Some(mut fallback) = fallback_by_blob.get(&path_hit.blob_name).cloned() else {
            continue;
        };
        fallback.score = path_hit.score;
        fallback.source = crate::model::SearchSource::Fused;
        content_hits.push(fallback);
    }
    sort_hits(content_hits);
}

/// Apply source-aware multiplicative priority factors to a candidate list.
/// 为候选结果应用源码感知的乘性优先级因子。
pub fn apply_source_priority(hits: &mut [SearchHit], intent: QueryIntent) {
    for hit in &mut *hits {
        hit.score *= source_priority_factor(&hit.path, intent);
    }
    sort_hits(hits);
}

/// Return the documented source priority factor for one path and intent.
/// 返回指定路径和查询意图对应的源码优先级因子。
pub fn source_priority_factor(path: &str, intent: QueryIntent) -> f32 {
    if intent == QueryIntent::Path {
        return 1.0;
    }
    let normalized = path.replace('\\', "/").to_lowercase();
    let name = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    if matches!(stem, "license" | "notice" | "copying") {
        return 0.1;
    }
    if stem == "readme" {
        return 1.0;
    }
    if stem.starts_with("readme") {
        return 0.2;
    }
    if format!("/{normalized}").contains("/docs/")
        || normalized.ends_with(".md")
        || normalized.ends_with(".rst")
        || normalized.ends_with(".txt")
    {
        return 0.5;
    }
    if format!("/{normalized}").contains("/tests/")
        || name.starts_with("test_")
        || name == "conftest.py"
        || name.contains(".test.")
        || name.contains(".spec.")
    {
        return 0.6;
    }
    if matches!(
        name,
        "index.ts" | "index.tsx" | "index.js" | "index.jsx" | "types.ts"
    ) {
        return 0.85;
    }
    1.0
}

/// Promote endpoint definitions for explicit symbol-definition queries.
/// 对明确的符号定义查询提升入口定义命中。
pub fn promote_endpoints(hits: &mut [SearchHit], intent: QueryIntent, identifiers: &[String]) {
    if intent != QueryIntent::Symbol || identifiers.is_empty() {
        return;
    }
    hits.sort_by(|left, right| {
        let left_endpoint = is_matching_endpoint(left, identifiers);
        let right_endpoint = is_matching_endpoint(right, identifiers);
        right_endpoint
            .cmp(&left_endpoint)
            .then_with(|| right.score.total_cmp(&left.score))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.range.cmp(&right.range))
            .then_with(|| left.start_char_offset.cmp(&right.start_char_offset))
            .then_with(|| left.id.cmp(&right.id))
    });
}

/// Reorder candidates using a provider-independent list of source indexes.
/// 使用不依赖供应商的源码下标列表重新排列候选结果。
pub fn reorder_by_rerank(hits: &[SearchHit], orderer: &RerankCallback) -> Vec<SearchHit> {
    let order = orderer(hits);
    let mut seen = BTreeSet::new();
    let mut reordered = Vec::with_capacity(hits.len());
    for index in order {
        if let Some(hit) = hits.get(index)
            && seen.insert(index)
        {
            reordered.push(hit.clone());
        }
    }
    for (index, hit) in hits.iter().enumerate() {
        if seen.insert(index) {
            reordered.push(hit.clone());
        }
    }
    reordered
}

/// Remove weak candidates below an explicit confidence floor.
/// 移除低于明确置信度下限的候选结果。
pub fn filter_confidence(hits: &mut Vec<SearchHit>, floor: f32) {
    hits.retain(|hit| hit.score >= floor);
}

/// Select high-quality context while maximizing distinct paths and respecting overlap.
/// 在遵守重叠限制的同时优先选择不同路径的高质量上下文。
pub fn coverage_select(mut hits: Vec<SearchHit>, config: CoverageConfig) -> Vec<SearchHit> {
    sort_hits(&mut hits);
    coverage_select_in_order(hits, config)
}

/// Select coverage while preserving an authoritative upstream ranking.
/// 在保留上游权威排序的前提下执行覆盖选择。
fn coverage_select_in_order(hits: Vec<SearchHit>, config: CoverageConfig) -> Vec<SearchHit> {
    if config.max_per_path == 0 || config.max_chars == 0 || config.max_results == 0 {
        return Vec::new();
    }
    let mut selected = Vec::new();
    let mut selected_paths = BTreeSet::new();
    let mut selected_ids = BTreeSet::new();
    let mut selected_chars = 0usize;

    for hit in &hits {
        if selected.len() >= config.max_results || selected_paths.contains(&hit.path) {
            continue;
        }
        if can_select(hit, &selected, &selected_ids, selected_chars, config) {
            selected_chars = selected_chars.saturating_add(hit.char_len());
            selected_paths.insert(hit.path.clone());
            selected_ids.insert(hit.id.clone());
            selected.push(hit.clone());
        }
    }

    for hit in &hits {
        if selected.len() >= config.max_results {
            break;
        }
        if can_select(hit, &selected, &selected_ids, selected_chars, config) {
            selected_chars = selected_chars.saturating_add(hit.char_len());
            selected_ids.insert(hit.id.clone());
            selected.push(hit.clone());
        }
    }
    selected
}

/// Render structured hits as line-numbered context only after selection is complete.
/// 只有在选择完成后将结构化命中渲染为带行号的上下文。
pub fn render_context(hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return "No code context found.\n".to_owned();
    }
    let mut output = String::new();
    for hit in hits {
        output.push_str(&format!(
            "Path: {}\nLines: {}-{}\n",
            hit.path, hit.range.start_line, hit.range.end_line
        ));
        if hit.start_char_offset > 0 {
            output.push_str(&format!(
                "Start character offset: {}\n",
                hit.start_char_offset
            ));
        }
        let lines = hit.content.split('\n').collect::<Vec<_>>();
        for (offset, line) in lines.iter().enumerate() {
            if offset + 1 == lines.len() && line.is_empty() && hit.content.ends_with('\n') {
                continue;
            }
            output.push_str(&format!(
                "    {}\t{}\n",
                hit.range.start_line.saturating_add(offset),
                line
            ));
        }
    }
    output
}

/// Sort hits by score and stable source identity.
/// 按分数和稳定源码身份对命中结果排序。
fn sort_hits(hits: &mut [SearchHit]) {
    hits.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.range.cmp(&right.range))
            .then_with(|| left.start_char_offset.cmp(&right.start_char_offset))
            .then_with(|| left.id.cmp(&right.id))
    });
}

/// Check whether a candidate can enter the coverage-selected context.
/// 判断候选结果是否可以进入覆盖度选择后的上下文。
fn can_select(
    hit: &SearchHit,
    selected: &[SearchHit],
    selected_ids: &BTreeSet<String>,
    selected_chars: usize,
    config: CoverageConfig,
) -> bool {
    if selected_ids.contains(&hit.id)
        || selected_chars.saturating_add(hit.char_len()) > config.max_chars
        || selected
            .iter()
            .filter(|selected_hit| selected_hit.path == hit.path)
            .count()
            >= config.max_per_path
    {
        return false;
    }
    selected
        .iter()
        .filter(|selected_hit| selected_hit.path == hit.path)
        .all(|selected_hit| hit_overlap_ratio(selected_hit, hit) < config.overlap_threshold)
}

/// Return overlap using character intervals when both hits belong to one physical source line.
/// 当两个命中都属于同一物理源码行时，使用字符区间返回重叠比例。
fn hit_overlap_ratio(first: &SearchHit, second: &SearchHit) -> f32 {
    if first.range.start_line == first.range.end_line
        && second.range.start_line == second.range.end_line
        && first.range == second.range
    {
        let first_end = first
            .start_char_offset
            .saturating_add(first.content.chars().count());
        let second_end = second
            .start_char_offset
            .saturating_add(second.content.chars().count());
        let overlap_start = first.start_char_offset.max(second.start_char_offset);
        let overlap_end = first_end.min(second_end);
        if overlap_end <= overlap_start {
            return 0.0;
        }
        let overlap = overlap_end.saturating_sub(overlap_start) as f32;
        let shorter = first
            .content
            .chars()
            .count()
            .min(second.content.chars().count()) as f32;
        return if shorter == 0.0 {
            0.0
        } else {
            overlap / shorter
        };
    }
    first.range.overlap_ratio(second.range)
}

/// Check whether a hit is an endpoint containing one of the requested identifiers.
/// 判断命中是否为包含请求标识符的入口定义。
fn is_matching_endpoint(hit: &SearchHit, identifiers: &[String]) -> bool {
    hit.kind == crate::model::HitKind::Endpoint
        && identifiers
            .iter()
            .any(|identifier| crate::symbol::content_contains_identifier(&hit.content, identifier))
}

/// Return the semantic priority used when dense and exact hits share one occurrence.
/// 返回稠密命中与精确命中共享同一出现位置时使用的语义优先级。
fn hit_kind_priority(kind: crate::model::HitKind) -> u8 {
    match kind {
        crate::model::HitKind::Content => 0,
        crate::model::HitKind::Reference => 1,
        crate::model::HitKind::Definition => 2,
        crate::model::HitKind::Endpoint => 3,
    }
}

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::{Chunk, ChunkKind, HitKind, SearchSource};

    /// Build a small deterministic hit fixture for ranking tests.
    /// 构建用于排序测试的小型确定性命中数据。
    fn hit(path: &str, start_line: usize, score: f32, kind: HitKind) -> SearchHit {
        let chunk = Chunk::new(
            format!("blob:{path}"),
            path,
            format!("fn value_{start_line}() {{}}"),
            start_line,
            start_line,
            ChunkKind::Recursive,
        )
        .unwrap();
        SearchHit::from_chunk(&chunk, score, SearchSource::Dense, kind)
    }

    /// Verify RRF merges the same occurrence and keeps deterministic ordering.
    /// 验证 RRF 会合并同一源码出现位置并保持确定性排序。
    #[test]
    fn fuses_duplicate_occurrences() {
        let first = hit("src/a.rs", 1, 0.9, HitKind::Content);
        let second = first.clone();
        let result = reciprocal_rank_fusion(
            &[
                RankedHits {
                    weight: 1.0,
                    hits: vec![first],
                },
                RankedHits {
                    weight: 0.75,
                    hits: vec![second],
                },
            ],
            60.0,
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].score, 1.0);
    }

    /// Verify a single recall list keeps the provider score used by confidence filtering.
    /// 验证单路召回保留置信度过滤所使用的供应商分数。
    #[test]
    fn single_list_preserves_provider_scores() {
        let result = reciprocal_rank_fusion(
            &[RankedHits {
                weight: 1.0,
                hits: vec![
                    hit("src/a.rs", 1, 0.82, HitKind::Content),
                    hit("src/b.rs", 1, 0.61, HitKind::Content),
                ],
            }],
            60.0,
        );
        assert_eq!(result[0].score, 0.82);
        assert_eq!(result[1].score, 0.61);
    }

    /// Verify multi-query RRF scores remain normalized to a stable zero-to-one scale.
    /// 验证多查询 RRF 分数保持在稳定的零到一归一化尺度内。
    #[test]
    fn multi_list_scores_are_normalized() {
        let result = reciprocal_rank_fusion(
            &[
                RankedHits {
                    weight: 1.0,
                    hits: vec![hit("src/a.rs", 1, 0.9, HitKind::Content)],
                },
                RankedHits {
                    weight: 0.75,
                    hits: vec![hit("src/b.rs", 1, 0.8, HitKind::Content)],
                },
            ],
            60.0,
        );
        assert!(
            result
                .iter()
                .all(|candidate| (0.0..=1.0).contains(&candidate.score))
        );
    }

    /// Verify extreme finite fusion and boost inputs cannot create non-finite ranking scores.
    /// 验证极端有限融合与提升输入不会产生非有限排序分数。
    #[test]
    fn extreme_finite_ranking_inputs_remain_finite() {
        let mut fused = reciprocal_rank_fusion(
            &[
                RankedHits {
                    weight: f32::MIN_POSITIVE,
                    hits: vec![hit("src/a.rs", 1, 0.9, HitKind::Content)],
                },
                RankedHits {
                    weight: f32::MIN_POSITIVE,
                    hits: vec![hit("src/b.rs", 1, 0.8, HitKind::Content)],
                },
            ],
            f32::MAX,
        );
        assert!(fused.iter().all(|candidate| candidate.score.is_finite()));

        let path_hit = {
            let mut candidate = fused[0].clone();
            candidate.score = f32::MAX;
            candidate
        };
        fused[0].score = f32::MAX;
        apply_path_boost(&mut fused, &[path_hit], f32::MAX);
        assert!(fused.iter().all(|candidate| candidate.score.is_finite()));
    }

    /// Verify exact-only hits cannot displace the semantic body of a call-chain result.
    /// 验证纯精确命中不会挤掉调用链结果中的语义主体。
    #[test]
    fn call_chain_merge_reserves_semantic_context() {
        let semantic = (1..=6)
            .map(|line| {
                hit(
                    "src/semantic.rs",
                    line,
                    1.0 - line as f32 * 0.05,
                    HitKind::Reference,
                )
            })
            .collect::<Vec<_>>();
        let exact = (1..=6)
            .map(|line| {
                hit(
                    "src/exact.rs",
                    line,
                    0.99 - line as f32 * 0.01,
                    HitKind::Definition,
                )
            })
            .collect::<Vec<_>>();
        let merged = merge_semantic_and_exact(semantic, exact, QueryIntent::CallChain, 6);
        let semantic_count = merged
            .iter()
            .filter(|candidate| candidate.path == "src/semantic.rs")
            .count();
        let exact_count = merged
            .iter()
            .filter(|candidate| candidate.path == "src/exact.rs")
            .count();
        assert_eq!(semantic_count, 4);
        assert_eq!(exact_count, 2);
    }

    /// Verify fusion retains stronger exact symbol metadata for endpoint promotion.
    /// 验证融合会保留更强的精确符号元数据，以便后续入口提升生效。
    #[test]
    fn fusion_preserves_stronger_symbol_kind() {
        let chunk = Chunk::new(
            "blob",
            "src/a.rs",
            "fn run() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let dense = SearchHit::from_chunk(&chunk, 0.9, SearchSource::Dense, HitKind::Content);
        let exact = SearchHit::from_chunk(&chunk, 0.8, SearchSource::Exact, HitKind::Endpoint);
        let result = reciprocal_rank_fusion(
            &[
                RankedHits {
                    weight: 1.0,
                    hits: vec![dense],
                },
                RankedHits {
                    weight: 0.75,
                    hits: vec![exact],
                },
            ],
            60.0,
        );
        assert_eq!(result[0].kind, HitKind::Endpoint);
    }

    /// Verify path hits boost same-Blob content without replacing content hits.
    /// 验证路径命中会提升同 Blob 内容而不会替换内容命中。
    #[test]
    fn boosts_same_blob_content() {
        let mut content = vec![hit("src/a.rs", 1, 0.5, HitKind::Content)];
        let path = hit("src/a.rs", 1, 1.0, HitKind::Content);
        apply_path_boost(&mut content, &[path], 0.1);
        assert!(content[0].score > 0.5);
    }

    /// Verify uncovered path hits can promote a host-provided first code Chunk.
    /// 验证没有内容命中时，路径命中可以回填宿主提供的首个代码块。
    #[test]
    fn appends_first_chunk_for_uncovered_path() {
        let path_hit = crate::path_index::PathDocument::new("blob", "src/a.rs")
            .unwrap()
            .to_hit(0.8)
            .unwrap();
        let chunk = Chunk::new(
            "blob",
            "src/a.rs",
            "fn first() {}",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let fallback = SearchHit::from_chunk(&chunk, 0.0, SearchSource::Dense, HitKind::Content);
        let mut content = Vec::new();
        append_path_fallbacks(&mut content, &[path_hit], &[fallback]);
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].content, "fn first() {}");
        assert_eq!(content[0].source, SearchSource::Fused);
    }

    /// Verify the main README stays authoritative while localized README files are demoted.
    /// 验证主 README 保持权威，而本地化 README 文件会被降权。
    #[test]
    fn applies_documented_readme_source_priorities() {
        assert_eq!(
            source_priority_factor("README.md", QueryIntent::Feature),
            1.0
        );
        assert_eq!(
            source_priority_factor("README.zh-CN.md", QueryIntent::Feature),
            0.2
        );
        assert_eq!(
            source_priority_factor("docs/guide.md", QueryIntent::Feature),
            0.5
        );
    }

    /// Verify host Rerank order remains authoritative through final coverage selection.
    /// 验证宿主重排顺序经过最终覆盖选择后仍保持权威。
    #[test]
    fn rerank_order_survives_final_selection() {
        let pipeline = RetrievalPipeline::default();
        let reranker = |_: &[SearchHit]| vec![1, 0];
        let selected = pipeline.finalize_candidates_with_path_fallbacks(
            vec![
                hit("src/high.rs", 1, 0.9, HitKind::Content),
                hit("src/low.rs", 1, 0.4, HitKind::Content),
            ],
            &[],
            &[],
            QueryIntent::Feature,
            &[],
            Some(&reranker),
        );
        assert_eq!(selected[0].path, "src/low.rs");
        assert_eq!(selected[1].path, "src/high.rs");
    }

    /// Verify endpoint promotion remains authoritative through final coverage selection.
    /// 验证入口定义提升经过最终覆盖选择后仍保持权威。
    #[test]
    fn endpoint_promotion_survives_final_selection() {
        let pipeline = RetrievalPipeline::default();
        let selected = pipeline.finalize_candidates_with_path_fallbacks(
            vec![
                hit("src/content.rs", 1, 0.9, HitKind::Content),
                hit("src/endpoint.rs", 2, 0.4, HitKind::Endpoint),
            ],
            &[],
            &[],
            QueryIntent::Symbol,
            &["value_2".to_owned()],
            None,
        );
        assert_eq!(selected[0].path, "src/endpoint.rs");
        assert_eq!(selected[1].path, "src/content.rs");
    }

    /// Verify endpoint promotion requires a complete identifier token rather than a substring.
    /// 验证入口提升要求完整标识符词元，而不是子字符串。
    #[test]
    fn endpoint_promotion_rejects_identifier_substrings() {
        let pipeline = RetrievalPipeline::default();
        let selected = pipeline.finalize_candidates_with_path_fallbacks(
            vec![
                hit("src/content.rs", 1, 0.9, HitKind::Content),
                hit("src/endpoint.rs", 20, 0.4, HitKind::Endpoint),
            ],
            &[],
            &[],
            QueryIntent::Symbol,
            &["value_2".to_owned(), String::new()],
            None,
        );
        assert_eq!(selected[0].path, "src/content.rs");
        assert_eq!(selected[1].path, "src/endpoint.rs");
    }

    /// Verify coverage selects distinct paths before adding a second chunk.
    /// 验证覆盖度选择会先选择不同路径，再选择同一路径第二个代码块。
    #[test]
    fn selects_distinct_paths_first() {
        let hits = vec![
            hit("src/a.rs", 1, 1.0, HitKind::Content),
            hit("src/a.rs", 4, 0.9, HitKind::Content),
            hit("src/b.rs", 1, 0.8, HitKind::Content),
        ];
        let selected = coverage_select(
            hits,
            CoverageConfig {
                max_per_path: 2,
                max_chars: 1_000,
                overlap_threshold: 0.6,
                max_results: 2,
            },
        );
        assert_eq!(selected.len(), 2);
        assert_ne!(selected[0].path, selected[1].path);
    }

    /// Verify disjoint fragments on one physical line are not treated as overlapping lines.
    /// 验证同一物理行上互不相交的片段不会被误判为行区间重叠。
    #[test]
    fn selects_disjoint_fragments_from_one_long_line() {
        let first = Chunk::new_with_start_char_offset(
            "blob",
            0,
            "src/a.rs",
            "abcd",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let second = Chunk::new_with_start_char_offset(
            "blob",
            4,
            "src/a.rs",
            "efgh",
            1,
            1,
            ChunkKind::Recursive,
        )
        .unwrap();
        let selected = coverage_select(
            vec![
                SearchHit::from_chunk(&first, 1.0, SearchSource::Dense, HitKind::Content),
                SearchHit::from_chunk(&second, 0.9, SearchSource::Dense, HitKind::Content),
            ],
            CoverageConfig {
                max_per_path: 2,
                max_chars: 8,
                overlap_threshold: 0.6,
                max_results: 2,
            },
        );

        assert_eq!(selected.len(), 2);
    }

    /// Verify line-numbered rendering remains a final formatting operation.
    /// 验证带行号渲染只作为最后的格式化操作。
    #[test]
    fn renders_line_numbered_context() {
        let chunk = Chunk::new(
            "blob",
            "src/lib.rs",
            "let value = 1;\nlet result = value;",
            10,
            11,
            ChunkKind::Recursive,
        )
        .unwrap();
        let rendered = render_context(&[SearchHit::from_chunk(
            &chunk,
            1.0,
            SearchSource::Dense,
            HitKind::Content,
        )]);
        assert!(rendered.contains("Lines: 10-11"));
        assert!(rendered.contains("    11\tlet result = value;"));
    }

    /// Verify rendered context preserves a nonzero character location for long-line fragments.
    /// 验证渲染上下文会保留超长行片段的非零字符位置。
    #[test]
    fn renders_long_line_character_offset() {
        let chunk = Chunk::new_with_start_char_offset(
            "blob",
            12,
            "src/lib.rs",
            "fn value() {}",
            7,
            7,
            ChunkKind::Recursive,
        )
        .unwrap();
        let rendered = render_context(&[SearchHit::from_chunk(
            &chunk,
            1.0,
            SearchSource::Dense,
            HitKind::Content,
        )]);

        assert!(rendered.contains("Start character offset: 12"));
    }
}
