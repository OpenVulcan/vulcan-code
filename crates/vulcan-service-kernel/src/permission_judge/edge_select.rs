//! Endpoint selection of the cloud judge: a background task probes every endpoint of
//! the chosen model through its public status route and picks the one judge requests use, unless
//! the user locked one.
//! 云端判定的端点选择：后台任务通过公开状态路由探测所选模型的每个端点，并选出判定请求使用的
//! 端点，除非用户已锁定端点。
//!
//! Probing never happens per request: it runs at start, on a fixed interval, after a settings
//! change and, at most once per [`FAILURE_REPROBE_GAP_MS`], after a request found its endpoint
//! unusable. Results stay in memory.
//! 探测从不随请求进行：只在启动时、按固定间隔、设置变化后，以及请求发现端点不可用后（每
//! [`FAILURE_REPROBE_GAP_MS`] 至多一次）运行。结果只保存在内存中。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use tokio::sync::{Mutex, Notify};
use vulcan_permission_judge::{
    EDGE_STATUS_PATH, EdgeCandidate, EdgeExclusion, EdgeStatus, choose_edge_endpoint, edge_score,
    median_ms, scores_add_compute,
};
use vulcan_provider_adapters::OpenVulcanEdgeEndpoint;
use vulcan_provider_config::provider_config::PersistedPermissionJudgeConfiguration;
use vulcan_service_api::{
    PermissionJudgeEdgeEndpoint, PermissionJudgeEdgeEndpointMode, PermissionJudgeEdgeEndpoints,
};

use super::PermissionJudgeRuntime;
use super::edge::{Grant, Target, http};
use super::services::{edge_model, read_state};
use crate::public_ai_settings::openvulcan_account::signed_in_credential;
use crate::{SharedServiceBridgeState, current_v1_unix_millis};

/// Time between scheduled probe rounds.
/// 两轮计划探测之间的间隔。
const PROBE_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// Status requests per endpoint in one round; the round trip is their median.
/// 每轮对每个端点发出的状态请求数；往返取其中位数。
const PROBE_ATTEMPTS: usize = 3;

/// Longest wait for one status request.
/// 一次状态请求的最长等待时间。
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest status answer read.
/// 读取的最大状态回答。
const STATUS_MAX_BYTES: usize = 16 * 1024;

/// Wake-ups within this time after the first one join a single round.
/// 首次唤醒后该时间内的唤醒合并为一轮。
const WAKE_DEBOUNCE: Duration = Duration::from_secs(2);

/// Shortest time between rounds that failed requests trigger.
/// 失败请求触发的两轮探测之间的最短间隔。
const FAILURE_REPROBE_GAP_MS: u64 = 30 * 1_000;

/// Successful probe of one endpoint.
/// 对一个端点的成功探测。
#[derive(Debug, Clone)]
struct Measured {
    /// Status the endpoint reported last.
    /// 端点最后报告的状态。
    status: EdgeStatus,
    /// Median round trip in milliseconds.
    /// 往返中位数（毫秒）。
    rtt_ms: u64,
}

/// One endpoint with its latest probe.
/// 一个端点及其最近一次探测。
#[derive(Debug, Clone)]
struct Probed {
    /// Endpoint as the deployment offered it.
    /// 部署提供的端点。
    endpoint: OpenVulcanEdgeEndpoint,
    /// Probe result, or why every attempt failed.
    /// 探测结果，或全部尝试失败的原因。
    outcome: Result<Measured, String>,
}

/// What the last round found.
/// 上一轮探测的发现。
#[derive(Debug, Default)]
struct View {
    /// Edge model the round probed.
    /// 本轮探测的边缘模型。
    model_id: Option<String>,
    /// Site marker of the account.
    /// 账户的来源标记。
    site: Option<String>,
    /// Endpoints in the deployment's order.
    /// 按部署顺序排列的端点。
    endpoints: Vec<Probed>,
    /// Endpoint automatic selection uses.
    /// 自动选择使用的端点。
    selected: Option<String>,
    /// When the round finished, Unix milliseconds.
    /// 本轮完成时间（Unix 毫秒）。
    probed_at_ms: Option<u64>,
    /// When the next scheduled round starts, Unix milliseconds.
    /// 下一轮计划探测的开始时间（Unix 毫秒）。
    next_probe_at_ms: Option<u64>,
    /// Why the endpoint list could not be obtained.
    /// 无法取得端点列表的原因。
    error: Option<String>,
}

/// Endpoint selection state shared by the background task, judge requests and settings calls.
/// 后台任务、判定请求与设置调用共享的端点选择状态。
#[derive(Default)]
pub(super) struct EdgeSelector {
    /// Findings of the last round.
    /// 上一轮的发现。
    view: StdMutex<View>,
    /// Wakes the background task for an early round.
    /// 唤醒后台任务提前进行一轮。
    wake: Notify,
    /// Keeps rounds from overlapping.
    /// 防止多轮探测重叠。
    probing: Mutex<()>,
    /// Unix milliseconds of the last wake-up a failed request caused.
    /// 上一次失败请求引起唤醒的时间（Unix 毫秒）。
    failure_wake_ms: AtomicU64,
    /// Whether the background task runs.
    /// 后台任务是否已运行。
    started: AtomicBool,
}

/// Current Unix milliseconds.
/// 当前 Unix 毫秒。
fn now_ms() -> u64 {
    current_v1_unix_millis()
        .ok()
        .and_then(|now| u64::try_from(now).ok())
        .unwrap_or_default()
}

/// Requests `GET /v1/status` from `endpoint` once, without credentials.
/// 不带凭证地向 `endpoint` 请求一次 `GET /v1/status`。
async fn fetch_status(endpoint: &str) -> Result<EdgeStatus, String> {
    let response = http()?
        .get(format!("{endpoint}{EDGE_STATUS_PATH}"))
        .timeout(PROBE_TIMEOUT)
        .header(reqwest::header::CACHE_CONTROL, "no-store")
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                String::from("The endpoint status did not answer in time.")
            } else {
                String::from("The endpoint status could not be reached.")
            }
        })?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|_| String::from("The endpoint status answer could not be read."))?;
    if !status.is_success() {
        return Err(format!(
            "The endpoint status answered HTTP {}.",
            status.as_u16()
        ));
    }
    if body.len() > STATUS_MAX_BYTES {
        return Err(String::from("The endpoint status answer is too large."));
    }
    serde_json::from_slice(&body)
        .map_err(|_| String::from("The endpoint status answer violates its contract."))
}

/// Probes `endpoint` [`PROBE_ATTEMPTS`] times in a row; the round trip is the median of the
/// successful attempts, and the probe fails only when every attempt failed.
/// 连续探测 `endpoint` [`PROBE_ATTEMPTS`] 次；往返取成功尝试的中位数，只有全部尝试失败时探测才失败。
async fn measure(endpoint: &str) -> Result<Measured, String> {
    let mut samples = Vec::with_capacity(PROBE_ATTEMPTS);
    let mut last = Err(String::new());
    for _ in 0..PROBE_ATTEMPTS {
        let started = Instant::now();
        match fetch_status(endpoint).await {
            Ok(status) => {
                samples.push(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
                last = Ok(status);
            }
            Err(error) if last.is_err() => last = Err(error),
            Err(_) => {}
        }
    }
    let status = last?;
    Ok(Measured {
        status,
        rtt_ms: median_ms(samples).unwrap_or_default(),
    })
}

impl Probed {
    /// Score for automatic selection, or why the endpoint is excluded; `None` when the probe
    /// failed. `with_compute` comes from [`add_compute`] over the same endpoints.
    /// 自动选择使用的评分或端点被排除的原因；探测失败时为 `None`。`with_compute` 来自对同一组端点的
    /// [`add_compute`]。
    fn score(
        &self,
        model_id: &str,
        site: &str,
        with_compute: bool,
    ) -> Option<Result<u64, EdgeExclusion>> {
        let measured = self.outcome.as_ref().ok()?;
        Some(edge_score(
            &measured.status,
            measured.rtt_ms,
            &self.endpoint.endpoint_id,
            model_id,
            site,
            with_compute,
        ))
    }
}

/// Whether the scores of `endpoints` add compute time: only when every endpoint that can be chosen
/// reports one, otherwise all compare by round trip.
/// `endpoints` 的评分是否加上计算耗时：仅当所有可被选中的端点都报告了计算耗时，否则全部按往返比较。
fn add_compute(model_id: &str, site: &str, endpoints: &[Probed]) -> bool {
    scores_add_compute(endpoints.iter().filter_map(|probed| {
        probed
            .score(model_id, site, false)?
            .ok()
            .and(probed.outcome.as_ref().ok())
            .map(|measured| &measured.status)
    }))
}

/// Endpoint automatic selection picks among `endpoints`, keeping `current` unless another is
/// clearly faster.
/// 自动选择在 `endpoints` 中选出的端点；除非另一端点明显更快，否则保持 `current`。
fn choose(
    model_id: &str,
    site: &str,
    endpoints: &[Probed],
    current: Option<&str>,
) -> Option<String> {
    let with_compute = add_compute(model_id, site, endpoints);
    let candidates = endpoints
        .iter()
        .map(|probed| EdgeCandidate {
            endpoint_id: &probed.endpoint.endpoint_id,
            priority: probed.endpoint.priority,
            score_ms: probed
                .score(model_id, site, with_compute)
                .and_then(Result::ok),
        })
        .collect::<Vec<_>>();
    choose_edge_endpoint(&candidates, current).map(str::to_string)
}

impl EdgeSelector {
    /// Endpoint a judge request for `model_id` goes to: the locked one, which must be offered,
    /// or the automatically selected one; before any round, the first by priority, and a round is
    /// asked for.
    /// `model_id` 的判定请求发往的端点：锁定的端点（必须在提供之列），或自动选择的端点；尚未探测时
    /// 使用优先级第一的端点，并请求一轮探测。
    pub(super) fn target(
        &self,
        model_id: &str,
        grant: &Grant,
        locked: Option<&str>,
    ) -> Result<Target, String> {
        let chosen = match locked {
            Some(id) => id.to_string(),
            None => {
                let selected = {
                    let view = self.view.lock().map_err(|_| poisoned())?;
                    (view.model_id.as_deref() == Some(model_id))
                        .then(|| view.selected.clone())
                        .flatten()
                };
                let offered = selected.filter(|id| {
                    grant
                        .endpoints
                        .iter()
                        .any(|endpoint| &endpoint.endpoint_id == id)
                });
                match offered {
                    Some(id) => id,
                    None => {
                        self.request_probe();
                        let unprobed = grant
                            .endpoints
                            .iter()
                            .map(|endpoint| Probed {
                                endpoint: endpoint.clone(),
                                outcome: Err(String::new()),
                            })
                            .collect::<Vec<_>>();
                        choose(model_id, &grant.site, &unprobed, None)
                            .ok_or_else(|| no_endpoint(model_id))?
                    }
                }
            }
        };
        let endpoint = grant
            .endpoints
            .iter()
            .find(|endpoint| endpoint.endpoint_id == chosen)
            .ok_or_else(|| {
                format!(
                    "The locked cloud judge endpoint `{chosen}` is not offered for `{model_id}`; lock another endpoint or return to automatic selection."
                )
            })?;
        Ok(Target {
            endpoint_id: endpoint.endpoint_id.clone(),
            endpoint: endpoint.endpoint.clone(),
            site: grant.site.clone(),
        })
    }

    /// Asks for an early round after a failed request, at most once per
    /// [`FAILURE_REPROBE_GAP_MS`].
    /// 请求失败后要求提前一轮，每 [`FAILURE_REPROBE_GAP_MS`] 至多一次。
    pub(super) fn request_probe(&self) {
        let now = now_ms();
        let last = self.failure_wake_ms.load(Ordering::Relaxed);
        if now.saturating_sub(last) >= FAILURE_REPROBE_GAP_MS
            && self
                .failure_wake_ms
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            self.wake.notify_one();
        }
    }

    /// Asks for an early round after the user changed the backend, model or lock.
    /// 用户修改后端、模型或锁定后要求提前一轮。
    pub(super) fn wake(&self) {
        self.wake.notify_one();
    }

    /// Runs one round for the saved settings: obtains the grant, probes every endpoint in turn and
    /// updates the selection. Clears the findings when the cloud backend is not in use.
    /// 按已保存设置运行一轮：取得授权，依次探测每个端点并更新选择。未使用云端后端时清空发现。
    pub(super) async fn probe(
        &self,
        state: &SharedServiceBridgeState,
        runtime: &PermissionJudgeRuntime,
    ) -> Result<(), String> {
        let _round = self.probing.lock().await;
        let (_, settings) = read_state(state)?;
        let Some(model_id) = edge_model(&settings).map(str::to_string) else {
            *self.view.lock().map_err(|_| poisoned())? = View::default();
            return Ok(());
        };
        let grant = match signed_in_credential(state).await {
            Ok(Some((connection_id, secret))) => {
                runtime
                    .edge_access
                    .access(&connection_id, &secret, &model_id, false)
                    .await
            }
            Ok(None) => Err(String::from(
                "Sign in to OpenVulcan before using the cloud judge.",
            )),
            Err(error) => Err(error),
        };
        let grant = match grant {
            Ok(grant) => grant,
            Err(error) => {
                let mut view = self.view.lock().map_err(|_| poisoned())?;
                let next_probe_at_ms = view.next_probe_at_ms;
                *view = View {
                    model_id: Some(model_id),
                    next_probe_at_ms,
                    error: Some(error),
                    ..View::default()
                };
                return Ok(());
            }
        };
        let mut endpoints = Vec::with_capacity(grant.endpoints.len());
        for endpoint in &grant.endpoints {
            endpoints.push(Probed {
                endpoint: endpoint.clone(),
                outcome: measure(&endpoint.endpoint).await,
            });
        }
        let mut view = self.view.lock().map_err(|_| poisoned())?;
        let current = (view.model_id.as_deref() == Some(model_id.as_str()))
            .then(|| view.selected.clone())
            .flatten();
        view.selected = choose(&model_id, &grant.site, &endpoints, current.as_deref());
        view.model_id = Some(model_id);
        view.site = Some(grant.site);
        view.endpoints = endpoints;
        view.probed_at_ms = Some(now_ms());
        view.error = None;
        Ok(())
    }

    /// Records when the next scheduled round starts.
    /// 记录下一轮计划探测的开始时间。
    fn schedule(&self, next_probe_at_ms: u64) {
        if let Ok(mut view) = self.view.lock() {
            view.next_probe_at_ms = Some(next_probe_at_ms);
        }
    }

    /// Endpoint table for the saved settings.
    /// 已保存设置的端点表。
    pub(super) fn snapshot(
        &self,
        settings: &PersistedPermissionJudgeConfiguration,
    ) -> Result<PermissionJudgeEdgeEndpoints, String> {
        let model_id = edge_model(settings).map(str::to_string);
        let locked = settings.edge_endpoint_id.clone();
        let view = self.view.lock().map_err(|_| poisoned())?;
        let current = model_id.is_some() && view.model_id == model_id;
        let site = view.site.clone().filter(|_| current);
        let endpoints: Vec<PermissionJudgeEdgeEndpoint> = if current {
            let model = model_id.as_deref().unwrap_or_default();
            let with_compute = site
                .as_deref()
                .is_some_and(|site| add_compute(model, site, &view.endpoints));
            view.endpoints
                .iter()
                .map(|probed| {
                    let score = site
                        .as_deref()
                        .and_then(|site| probed.score(model, site, with_compute));
                    let measured = probed.outcome.as_ref().ok();
                    PermissionJudgeEdgeEndpoint {
                        endpoint_id: probed.endpoint.endpoint_id.clone(),
                        endpoint: probed.endpoint.endpoint.clone(),
                        priority: probed.endpoint.priority,
                        online: probed.endpoint.online,
                        region: measured.map(|measured| measured.status.region.clone()),
                        ready: measured.map(|measured| measured.status.ready),
                        accepts_site: measured.and_then(|measured| {
                            Some(measured.status.sites.contains(site.as_ref()?))
                        }),
                        rtt_ms: measured.map(|measured| measured.rtt_ms),
                        compute_p50_ms: measured
                            .and_then(|measured| measured.status.compute_ms)
                            .map(|compute| compute.p50),
                        score_ms: score.and_then(Result::ok),
                        error: match (&probed.outcome, score) {
                            (Err(error), _) => Some(error.clone()),
                            (_, Some(Err(EdgeExclusion::Mismatched))) => Some(String::from(
                                "The endpoint reported another endpoint id or model.",
                            )),
                            _ => None,
                        },
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        let selected_endpoint_id = match &locked {
            Some(id) => endpoints
                .iter()
                .any(|endpoint| &endpoint.endpoint_id == id)
                .then(|| id.clone()),
            None => view.selected.clone().filter(|_| current),
        };
        Ok(PermissionJudgeEdgeEndpoints {
            model_id,
            site,
            mode: if locked.is_some() {
                PermissionJudgeEdgeEndpointMode::Locked
            } else {
                PermissionJudgeEdgeEndpointMode::Auto
            },
            locked_endpoint_id: locked,
            selected_endpoint_id,
            probed_at_ms: view.probed_at_ms.filter(|_| current),
            next_probe_at_ms: view.next_probe_at_ms,
            error: view.error.clone().filter(|_| current),
            endpoints,
        })
    }
}

/// Error for a poisoned selection lock.
/// 选择锁中毒时的错误。
fn poisoned() -> String {
    String::from("The cloud judge endpoint selection state is unavailable.")
}

/// Error for a grant without endpoints, which the access parser already refuses.
/// 授权没有端点时的错误；访问解析已拒绝这种情况。
fn no_endpoint(model_id: &str) -> String {
    format!("OpenVulcan offered no endpoint for the cloud model `{model_id}`.")
}

/// Starts the background selection task once per process. It probes at start, then every
/// [`PROBE_INTERVAL`] or when woken, and ends when the service state is dropped.
/// 每个进程只启动一次后台选择任务。它在启动时探测，之后每 [`PROBE_INTERVAL`] 或被唤醒时探测，在服务
/// 状态释放后结束。
pub(super) fn start(state: &SharedServiceBridgeState, runtime: &Arc<PermissionJudgeRuntime>) {
    if runtime.edge_select.started.swap(true, Ordering::AcqRel) {
        return;
    }
    let weak_state = Arc::downgrade(state);
    let runtime = Arc::clone(runtime);
    tokio::spawn(async move {
        loop {
            let Some(state) = weak_state.upgrade() else {
                break;
            };
            if let Err(error) = runtime.edge_select.probe(&state, &runtime).await {
                tracing::warn!("[permission-judge] edge endpoint probe failed: {error}");
            }
            drop(state);
            let interval_ms = u64::try_from(PROBE_INTERVAL.as_millis()).unwrap_or(u64::MAX);
            runtime
                .edge_select
                .schedule(now_ms().saturating_add(interval_ms));
            tokio::select! {
                () = tokio::time::sleep(PROBE_INTERVAL) => {}
                () = runtime.edge_select.wake.notified() => {
                    tokio::time::sleep(WAKE_DEBOUNCE).await;
                    // Wake-ups during the debounce join this round.
                    // 去抖期间的唤醒并入本轮。
                    let _ = runtime.edge_select.wake.notified().now_or_never();
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// Starts a loopback status route that answers `body` with `status` to every request.
    /// 启动回环状态路由，对每个请求以 `status` 回答 `body`。
    async fn status_route(status: &'static str, body: serde_json::Value) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer).await;
                let body = body.to_string();
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
            }
        });
        format!("http://{address}")
    }

    fn status(endpoint_id: &str, ready: bool) -> serde_json::Value {
        serde_json::json!({
            "endpoint_id": endpoint_id, "model_id": "m", "region": "us-west", "instance": "sv-1",
            "sites": ["global"], "ready": ready, "nodes_online": 1,
            "compute_ms": null, "inflight": 0, "generated_at": "2026-10-26T00:00:00Z"
        })
    }

    fn endpoint(id: &str, url: &str, priority: u32) -> OpenVulcanEdgeEndpoint {
        OpenVulcanEdgeEndpoint {
            endpoint_id: id.to_string(),
            endpoint: url.to_string(),
            priority,
            online: true,
        }
    }

    fn grant(endpoints: Vec<OpenVulcanEdgeEndpoint>) -> Grant {
        Grant {
            site: String::from("global"),
            endpoints,
            token: String::new(),
        }
    }

    // A probe measures a round trip from a status answer, and a refused or malformed status
    // fails the probe.
    // 探测从状态回答中测得往返；被拒绝或格式不符的状态使探测失败。
    #[tokio::test]
    async fn probes_measure_status_answers_and_fail_on_bad_ones() {
        let good = status_route("200 OK", status("a", true)).await;
        let measured = measure(&good).await.unwrap();
        assert_eq!(measured.status.endpoint_id, "a");
        assert!(measured.status.compute_ms.is_none());
        let refused = status_route("503 Service Unavailable", status("a", true)).await;
        assert!(measure(&refused).await.is_err());
        let malformed = status_route("200 OK", serde_json::json!({"ready": true})).await;
        assert!(measure(&malformed).await.is_err());
    }

    // Without a round the first endpoint by priority is used; a locked endpoint is used as is,
    // and a locked endpoint that is not offered fails instead of falling back.
    // 尚未探测时使用优先级第一的端点；锁定的端点原样使用，未提供的锁定端点直接失败而不回退。
    #[test]
    fn targets_follow_priority_until_probed_and_honour_the_lock() {
        let selector = EdgeSelector::default();
        let offered = grant(vec![
            endpoint("b", "https://b.example.com", 20),
            endpoint("a", "https://a.example.com", 10),
        ]);
        let target = selector.target("m", &offered, None).unwrap();
        assert_eq!(
            (target.endpoint_id.as_str(), target.site.as_str()),
            ("a", "global")
        );
        let locked = selector.target("m", &offered, Some("b")).unwrap();
        assert_eq!(locked.endpoint, "https://b.example.com");
        assert!(selector.target("m", &offered, Some("gone")).is_err());
    }

    // An endpoint without a compute figure makes every endpoint compare by round trip.
    // 有端点没有计算耗时时，所有端点都按往返比较。
    #[test]
    fn an_endpoint_without_compute_makes_selection_compare_round_trips() {
        let probed = |id: &str, rtt_ms: u64, compute: Option<u64>| {
            let mut answer = status(id, true);
            answer["compute_ms"] = compute.map_or(
                serde_json::Value::Null,
                |p50| serde_json::json!({"p50": p50, "p95": p50}),
            );
            Probed {
                endpoint: endpoint(id, "https://x.example.com", 10),
                outcome: Ok(Measured {
                    status: serde_json::from_value(answer).unwrap(),
                    rtt_ms,
                }),
            }
        };
        // Near with 200 ms compute against far without any: compared by round trip, near wins.
        let endpoints = [probed("near", 58, Some(200)), probed("far", 182, None)];
        assert_eq!(
            choose("m", "global", &endpoints, None).as_deref(),
            Some("near")
        );
        // Once both report compute it counts: far's faster compute outweighs its round trip.
        let endpoints = [probed("near", 58, Some(200)), probed("far", 182, Some(20))];
        assert_eq!(
            choose("m", "global", &endpoints, None).as_deref(),
            Some("far")
        );
    }

    // Selection skips endpoints that are not ready and keeps a selection made for the same model.
    // 选择跳过未就绪的端点，并保持为同一模型做出的选择。
    #[test]
    fn selection_skips_unready_endpoints_and_is_kept_per_model() {
        let probed = |id: &str, priority: u32, ready: bool, rtt_ms: u64| Probed {
            endpoint: endpoint(id, "https://x.example.com", priority),
            outcome: Ok(Measured {
                status: serde_json::from_value(status(id, ready)).unwrap(),
                rtt_ms,
            }),
        };
        let endpoints = [probed("a", 10, false, 5), probed("b", 20, true, 90)];
        assert_eq!(
            choose("m", "global", &endpoints, None).as_deref(),
            Some("b")
        );
        assert_eq!(choose("m", "cn", &endpoints, None).as_deref(), Some("a"));

        let selector = EdgeSelector::default();
        *selector.view.lock().unwrap() = View {
            model_id: Some(String::from("m")),
            selected: Some(String::from("b")),
            ..View::default()
        };
        let offered = grant(vec![
            endpoint("a", "https://a.example.com", 10),
            endpoint("b", "https://b.example.com", 20),
        ]);
        assert_eq!(
            selector.target("m", &offered, None).unwrap().endpoint_id,
            "b"
        );
        assert_eq!(
            selector
                .target("other", &offered, None)
                .unwrap()
                .endpoint_id,
            "a"
        );
    }

    // Failed requests wake the prober at most once per gap.
    // 失败请求每个间隔内至多唤醒一次探测。
    #[tokio::test]
    async fn failure_wake_ups_are_rate_limited() {
        let selector = EdgeSelector::default();
        selector.request_probe();
        assert!(selector.wake.notified().now_or_never().is_some());
        selector.request_probe();
        assert!(selector.wake.notified().now_or_never().is_none());
    }
}
