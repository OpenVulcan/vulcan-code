//! Endpoint choice of the official direct judge: the wire shape of an edge endpoint's public
//! `GET /v1/status` and the rule that turns probe results into the endpoint judge requests go to.
//! 官方直连判定的端点选择：边缘端点公开 `GET /v1/status` 的线协议形状，以及把探测结果转换为判定
//! 请求所用端点的规则。
//!
//! A probe's estimated latency is its network round trip plus the compute time the endpoint
//! reports. The client switches only when another endpoint is clearly faster, so two endpoints of
//! similar latency do not alternate.
//! 探测的预计延迟是网络往返加上端点报告的计算耗时。只有另一端点明显更快时客户端才切换，使延迟相近
//! 的两个端点不会来回切换。

use serde::Deserialize;

/// Path of the public status route below an edge endpoint's base URL.
/// 公开状态路由在边缘端点基础地址下的路径。
pub const EDGE_STATUS_PATH: &str = "/v1/status";

/// Header carrying the id of the deployment that issued the access token.
/// 携带签发访问令牌的部署 id 的请求头。
pub const EDGE_SITE_HEADER: &str = "X-OpenVulcan-Site";

/// A better endpoint replaces the current one only when its score is below this percentage of the
/// current one's.
/// 只有更优端点的评分低于当前端点评分的该百分比时才替换当前端点。
pub const EDGE_SWITCH_PERCENT: u64 = 80;

/// Answer of `GET /v1/status`; fields the client does not read are ignored.
/// `GET /v1/status` 的回答；客户端不读取的字段一律忽略。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EdgeStatus {
    /// Endpoint id the endpoint serves under.
    /// 端点服务所用的端点 id。
    pub endpoint_id: String,
    /// Model the endpoint serves.
    /// 端点服务的模型。
    pub model_id: String,
    /// Region label of the endpoint.
    /// 端点的地区标签。
    pub region: String,
    /// Instance that answered.
    /// 作答的实例。
    pub instance: String,
    /// Deployment ids whose tokens the endpoint accepts.
    /// 端点接受其令牌的部署 id。
    pub sites: Vec<String>,
    /// Whether the endpoint can judge now.
    /// 端点当前能否判定。
    pub ready: bool,
    /// GPU nodes linked to the answering instance.
    /// 与作答实例相连的 GPU 节点数。
    pub nodes_online: u32,
    /// In-endpoint time of the latest successful judgements; `None` only before the instance's
    /// first one.
    /// 最近若干次成功判定的端点内耗时；只在实例首次成功判定之前为 `None`。
    pub compute_ms: Option<EdgeComputeMs>,
    /// Requests the instance is serving.
    /// 实例正在处理的请求数。
    pub inflight: u32,
    /// When the answer was produced, RFC 3339.
    /// 回答生成时间，RFC 3339。
    pub generated_at: String,
}

/// Percentiles of recent in-endpoint judgement time, in milliseconds.
/// 最近端点内判定耗时的分位，单位毫秒。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct EdgeComputeMs {
    /// Median.
    /// 中位数。
    pub p50: u64,
    /// 95th percentile.
    /// 第 95 百分位。
    pub p95: u64,
}

/// Why a probed endpoint cannot be chosen automatically.
/// 已探测端点不能被自动选中的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeExclusion {
    /// The status names another endpoint or model.
    /// 状态指向其他端点或模型。
    Mismatched,
    /// The endpoint does not accept the account's site.
    /// 端点不接受该账户的来源标记。
    SiteNotAccepted,
    /// The endpoint reports it cannot judge now.
    /// 端点报告当前无法判定。
    NotReady,
}

/// Whether scores add the reported compute time: only when every endpoint that can be chosen
/// reports one. Otherwise all of them compare by round trip alone, so an endpoint without a figure
/// is neither favoured nor penalised.
/// 评分是否加上报告的计算耗时：仅当所有可被选中的端点都报告了计算耗时。否则全部只按往返比较，
/// 使没有数值的端点既不占便宜也不吃亏。
pub fn scores_add_compute<'a>(eligible: impl IntoIterator<Item = &'a EdgeStatus>) -> bool {
    eligible
        .into_iter()
        .all(|status| status.compute_ms.is_some())
}

/// Scores a successful probe of `endpoint_id` for `model_id` and `site`: the round trip, plus the
/// reported median compute time when `with_compute` (see [`scores_add_compute`]).
/// 为 `endpoint_id` 针对 `model_id` 与 `site` 的一次成功探测评分：往返；`with_compute` 时再加上报告的
/// 计算耗时中位数（见 [`scores_add_compute`]）。
pub fn edge_score(
    status: &EdgeStatus,
    rtt_ms: u64,
    endpoint_id: &str,
    model_id: &str,
    site: &str,
    with_compute: bool,
) -> Result<u64, EdgeExclusion> {
    if status.endpoint_id != endpoint_id || status.model_id != model_id {
        return Err(EdgeExclusion::Mismatched);
    }
    if !status.sites.iter().any(|accepted| accepted == site) {
        return Err(EdgeExclusion::SiteNotAccepted);
    }
    if !status.ready {
        return Err(EdgeExclusion::NotReady);
    }
    let compute = status.compute_ms.filter(|_| with_compute);
    Ok(rtt_ms.saturating_add(compute.map_or(0, |compute| compute.p50)))
}

/// Median of round-trip samples; the lower middle one for an even count.
/// 往返样本的中位数；样本数为偶数时取较小的中间值。
pub fn median_ms(mut samples: Vec<u64>) -> Option<u64> {
    samples.sort_unstable();
    samples.get(samples.len().checked_sub(1)? / 2).copied()
}

/// One endpoint as the chooser sees it.
/// 选择器看到的一个端点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeCandidate<'a> {
    /// Endpoint id.
    /// 端点 id。
    pub endpoint_id: &'a str,
    /// Operator's default order; lower comes first.
    /// 运营方的默认顺序；数值小者在前。
    pub priority: u32,
    /// Score of the latest probe; `None` when unprobed, failed or excluded.
    /// 最近一次探测的评分；未探测、失败或被排除时为 `None`。
    pub score_ms: Option<u64>,
}

/// Chooses the endpoint automatic selection uses. The lowest score wins, ties broken by priority
/// and id; `current` is kept while it is eligible and the best score is not below
/// [`EDGE_SWITCH_PERCENT`] of its own. With no eligible endpoint the first by priority is used.
/// 选择自动选点使用的端点。评分最低者胜出，平局按优先级与 id 决定；`current` 仍可用且最佳评分不低于
/// 其评分的 [`EDGE_SWITCH_PERCENT`] 时保持不变。没有可用端点时使用优先级第一的端点。
pub fn choose_edge_endpoint<'a>(
    candidates: &[EdgeCandidate<'a>],
    current: Option<&str>,
) -> Option<&'a str> {
    let best = candidates
        .iter()
        .filter_map(|candidate| Some((candidate.score_ms?, candidate)))
        .min_by_key(|(score, candidate)| (*score, candidate.priority, candidate.endpoint_id));
    let Some((best_score, best)) = best else {
        return candidates
            .iter()
            .min_by_key(|candidate| (candidate.priority, candidate.endpoint_id))
            .map(|candidate| candidate.endpoint_id);
    };
    let kept = candidates.iter().find_map(|candidate| {
        (Some(candidate.endpoint_id) == current)
            .then_some(candidate)
            .and_then(|candidate| Some((candidate.score_ms?, candidate.endpoint_id)))
    });
    match kept {
        Some((current_score, id))
            if best_score.saturating_mul(100)
                >= current_score.saturating_mul(EDGE_SWITCH_PERCENT) =>
        {
            Some(id)
        }
        _ => Some(best.endpoint_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(ready: bool, sites: &[&str], compute: Option<u64>) -> EdgeStatus {
        EdgeStatus {
            endpoint_id: String::from("us-mini"),
            model_id: String::from("m"),
            region: String::from("us-west"),
            instance: String::from("sv-1"),
            sites: sites.iter().map(|site| site.to_string()).collect(),
            ready,
            nodes_online: 1,
            compute_ms: compute.map(|p50| EdgeComputeMs { p50, p95: p50 * 2 }),
            inflight: 0,
            generated_at: String::from("2026-10-26T00:00:00Z"),
        }
    }

    fn candidate(id: &str, priority: u32, score_ms: Option<u64>) -> EdgeCandidate<'_> {
        EdgeCandidate {
            endpoint_id: id,
            priority,
            score_ms,
        }
    }

    // The status answer parses with extra fields and a null compute time.
    // 状态回答在带额外字段与计算耗时为 null 时都能解析。
    #[test]
    fn status_answers_tolerate_extra_fields_and_null_compute() {
        let parsed: EdgeStatus = serde_json::from_value(serde_json::json!({
            "endpoint_id": "us-mini", "model_id": "m", "region": "us-west", "instance": "sv-1",
            "sites": ["global", "cn"], "ready": true, "nodes_online": 1,
            "compute_ms": {"p50": 64, "p95": 128}, "inflight": 0,
            "generated_at": "2026-10-26T00:00:00Z", "future_field": {"x": 1}
        }))
        .unwrap();
        assert_eq!(parsed.compute_ms, Some(EdgeComputeMs { p50: 64, p95: 128 }));
        let parsed: EdgeStatus = serde_json::from_value(serde_json::json!({
            "endpoint_id": "us-mini", "model_id": "m", "region": "us-west", "instance": "sv-1",
            "sites": [], "ready": false, "nodes_online": 0, "compute_ms": null, "inflight": 0,
            "generated_at": "2026-10-26T00:00:00Z"
        }))
        .unwrap();
        assert_eq!(parsed.compute_ms, None);
        assert!(
            serde_json::from_value::<EdgeStatus>(serde_json::json!({"endpoint_id": "x"})).is_err()
        );
    }

    // Scores add the compute median only when asked; exclusions name their cause.
    // 只在要求时评分加上计算中位数；被排除时给出原因。
    #[test]
    fn scores_add_reported_compute_and_exclude_unusable_endpoints() {
        let score = |status: &EdgeStatus| edge_score(status, 40, "us-mini", "m", "cn", true);
        assert_eq!(score(&status(true, &["global", "cn"], Some(64))), Ok(104));
        assert_eq!(
            edge_score(
                &status(true, &["cn"], Some(64)),
                40,
                "us-mini",
                "m",
                "cn",
                false
            ),
            Ok(40)
        );
        assert_eq!(
            score(&status(true, &["global"], Some(64))),
            Err(EdgeExclusion::SiteNotAccepted)
        );
        assert_eq!(
            score(&status(false, &["cn"], Some(64))),
            Err(EdgeExclusion::NotReady)
        );
        assert_eq!(
            edge_score(&status(true, &["cn"], None), 40, "other", "m", "cn", true),
            Err(EdgeExclusion::Mismatched)
        );
        assert_eq!(
            edge_score(
                &status(true, &["cn"], None),
                40,
                "us-mini",
                "other",
                "cn",
                true
            ),
            Err(EdgeExclusion::Mismatched)
        );
    }

    // Compute counts only when every eligible endpoint reports it; one missing figure makes all
    // compare by round trip, so a slow-network endpoint without a figure cannot win on it.
    // 只有所有可选端点都报告计算耗时时才计入；缺一个就全部按往返比较，网络慢但没有数值的端点
    // 不能因此胜出。
    #[test]
    fn one_missing_compute_makes_all_compare_by_round_trip() {
        let near = status(true, &["cn"], Some(200));
        let far_unmeasured = status(true, &["cn"], None);
        assert!(!scores_add_compute([&near, &far_unmeasured]));
        assert!(scores_add_compute([
            &near,
            &status(true, &["cn"], Some(90))
        ]));
        assert!(scores_add_compute([]));
        let with_compute = scores_add_compute([&near, &far_unmeasured]);
        let near_score = edge_score(&near, 58, "us-mini", "m", "cn", with_compute).unwrap();
        let far_score = edge_score(&far_unmeasured, 182, "us-mini", "m", "cn", with_compute);
        assert!(near_score < far_score.unwrap());
    }

    // The median ignores outliers and needs at least one sample.
    // 中位数忽略离群值，且至少需要一个样本。
    #[test]
    fn round_trips_use_the_median_sample() {
        assert_eq!(median_ms(vec![30, 900, 31]), Some(31));
        assert_eq!(median_ms(vec![50, 20]), Some(20));
        assert_eq!(median_ms(vec![7]), Some(7));
        assert_eq!(median_ms(Vec::new()), None);
    }

    // The lowest score wins; without any score the first by priority is used.
    // 评分最低者胜出；没有任何评分时使用优先级第一的端点。
    #[test]
    fn the_fastest_eligible_endpoint_wins() {
        let candidates = [
            candidate("a", 10, Some(120)),
            candidate("b", 20, Some(60)),
            candidate("c", 0, None),
        ];
        assert_eq!(choose_edge_endpoint(&candidates, None), Some("b"));
        let unprobed = [candidate("b", 20, None), candidate("a", 10, None)];
        assert_eq!(choose_edge_endpoint(&unprobed, Some("b")), Some("a"));
        let tied = [candidate("b", 10, Some(60)), candidate("a", 10, Some(60))];
        assert_eq!(choose_edge_endpoint(&tied, None), Some("a"));
        assert_eq!(choose_edge_endpoint(&[], None), None);
    }

    // The current endpoint stays until another is clearly faster, and is left when it fails.
    // 当前端点保持到另一端点明显更快为止，失败时离开。
    #[test]
    fn the_current_endpoint_is_kept_until_another_is_clearly_faster() {
        let close = [candidate("a", 10, Some(100)), candidate("b", 20, Some(81))];
        assert_eq!(choose_edge_endpoint(&close, Some("a")), Some("a"));
        let edge = [candidate("a", 10, Some(100)), candidate("b", 20, Some(80))];
        assert_eq!(choose_edge_endpoint(&edge, Some("a")), Some("a"));
        let clear = [candidate("a", 10, Some(100)), candidate("b", 20, Some(79))];
        assert_eq!(choose_edge_endpoint(&clear, Some("a")), Some("b"));
        let failed = [candidate("a", 10, None), candidate("b", 20, Some(500))];
        assert_eq!(choose_edge_endpoint(&failed, Some("a")), Some("b"));
        let gone = [candidate("b", 20, Some(500))];
        assert_eq!(choose_edge_endpoint(&gone, Some("a")), Some("b"));
    }
}
