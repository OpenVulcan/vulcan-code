//! The single local judge worker: start, warm-up, atomic model switch and status.
//! 唯一的本机判定 worker：启动、预热、原子模型切换与状态。
//!
//! One worker loads one model on one device. A worker that no longer matches the settings is
//! replaced; a load that failed is not retried by evaluations, only by an explicit user action, so a
//! broken setup costs one failure instead of one per tool call.
//! 一个 worker 在一个设备上加载一个模型。不再与设置匹配的 worker 会被替换；加载失败后评估不会重试，
//! 只有用户的明确操作才会重试，因此损坏的配置只失败一次，而不是每次工具调用都失败。

use std::sync::Mutex;

use tokio::sync::{Mutex as AsyncMutex, MutexGuard, oneshot};
use tokio::time::{Duration, timeout};
use vulcan_host_tools::judge_host_client::JudgeHostClient;
use vulcan_permission_judge::protocol::{
    BenchmarkReport, HostStatus, JudgeBatchLimits, JudgeInput, JudgeOutcome, LoadReport,
};
use vulcan_service_api::{PermissionJudgeWorkerPhase, PermissionJudgeWorkerStatus};

use super::environment::LoadSpec;

/// Longest a status poll waits for the worker, which may be busy judging.
/// 状态轮询等待 worker 的最长时间；worker 可能正在判定。
const STATUS_POLL_WAIT: Duration = Duration::from_millis(200);

/// Longest a gated call waits for a warm-up in progress before going to the user.
/// 受管控调用在交给用户前等待进行中预热的最长时间。
const JUDGE_WARMUP_WAIT: Duration = Duration::from_secs(30);

/// A started worker and the model it holds.
/// 已启动的 worker 及其持有的模型。
struct LoadedWorker {
    spec: LoadSpec,
    client: JudgeHostClient,
    limits: JudgeBatchLimits,
}

/// One caller waiting for its calls to be judged.
/// 一个等待其调用被判定的调用方。
struct Waiting {
    model_id: String,
    items: Vec<JudgeInput>,
    reply: oneshot::Sender<Result<Vec<JudgeOutcome>, String>>,
}

/// Outcome of one caller's calls.
/// 一个调用方的调用结果。
type CallerResult = Result<Vec<JudgeOutcome>, String>;

/// Flattens the callers' calls in order into requests of at most `limit` calls; each call keeps
/// the index of the caller that owns it.
/// 按顺序将各调用方的调用展平为每个最多 `limit` 条的请求；每条调用保留其所属调用方的索引。
fn merge_requests(groups: Vec<Vec<JudgeInput>>, limit: usize) -> Vec<Vec<(usize, JudgeInput)>> {
    let mut requests: Vec<Vec<(usize, JudgeInput)>> = Vec::new();
    for (owner, items) in groups.into_iter().enumerate() {
        for item in items {
            match requests.last_mut() {
                Some(request) if request.len() < limit.max(1) => request.push((owner, item)),
                _ => requests.push(vec![(owner, item)]),
            }
        }
    }
    requests
}

/// Gathers each caller's outcomes from the request results, in request order; a caller with any
/// call in a failed request receives that failure.
/// 按请求顺序从请求结果中收集每个调用方的结果；任一调用位于失败请求中的调用方收到该失败。
fn split_results(
    callers: usize,
    answered: Vec<(Vec<usize>, CallerResult)>,
) -> Vec<CallerResult> {
    let mut results: Vec<CallerResult> = (0..callers).map(|_| Ok(Vec::new())).collect();
    for (owners, result) in answered {
        match result {
            Ok(outcomes) => {
                for (owner, outcome) in owners.into_iter().zip(outcomes) {
                    if let Some(Ok(collected)) = results.get_mut(owner) {
                        collected.push(outcome);
                    }
                }
            }
            Err(error) => {
                for owner in owners {
                    if let Some(slot) = results.get_mut(owner) {
                        *slot = Err(error.clone());
                    }
                }
            }
        }
    }
    results
}

/// Result a caller receives; a dropped reply means the serving caller was cancelled mid-request.
/// 调用方收到的结果；应答被丢弃表示代为服务的调用方在请求中途被取消。
fn answered_or_dropped(answered: Result<CallerResult, oneshot::error::RecvError>) -> CallerResult {
    answered.unwrap_or_else(|_| {
        Err(String::from(
            "The local judge request was cancelled before it finished.",
        ))
    })
}

/// Last known lifecycle state, readable without waiting for the worker.
/// 最近已知的生命周期状态，无需等待 worker 即可读取。
#[derive(Clone)]
struct PhaseState {
    phase: PermissionJudgeWorkerPhase,
    model_id: Option<String>,
    spec: Option<LoadSpec>,
    error: Option<String>,
    process_id: Option<u32>,
    report: Option<LoadReport>,
    status: Option<HostStatus>,
}

impl Default for PhaseState {
    fn default() -> Self {
        Self {
            phase: PermissionJudgeWorkerPhase::NotLoaded,
            model_id: None,
            spec: None,
            error: None,
            process_id: None,
            report: None,
            status: None,
        }
    }
}

/// Owner of the local worker.
/// 本机 worker 的所有者。
#[derive(Default)]
pub(crate) struct JudgeWorker {
    slot: AsyncMutex<Option<LoadedWorker>>,
    phase: Mutex<PhaseState>,
    queue: Mutex<Vec<Waiting>>,
}

/// Starts a worker and loads `spec`.
/// 启动 worker 并加载 `spec`。
async fn start(spec: &LoadSpec) -> Result<(JudgeHostClient, LoadReport), String> {
    let mut client = JudgeHostClient::spawn(&spec.library_dirs).await?;
    match client
        .load(
            spec.package_dir.clone(),
            spec.runtime_library.clone(),
            spec.execution_provider_library.clone(),
            spec.device,
        )
        .await
    {
        Ok(report) => Ok((client, report)),
        Err(error) => {
            client.shutdown().await;
            Err(error)
        }
    }
}

impl JudgeWorker {
    /// Replaces the phase snapshot.
    /// 替换阶段快照。
    fn set_phase(&self, update: impl FnOnce(&mut PhaseState)) {
        if let Ok(mut phase) = self.phase.lock() {
            update(&mut phase);
        }
    }

    /// Marks `spec` as warming.
    /// 将 `spec` 标记为预热中。
    fn mark_warming(&self, spec: &LoadSpec) {
        self.set_phase(|phase| {
            *phase = PhaseState {
                phase: PermissionJudgeWorkerPhase::Warming,
                model_id: Some(spec.model_id.clone()),
                spec: Some(spec.clone()),
                ..PhaseState::default()
            }
        });
    }

    /// Records the outcome of a load of `spec`.
    /// 记录加载 `spec` 的结果。
    fn mark_loaded(&self, spec: &LoadSpec, outcome: Result<(Option<u32>, LoadReport), &str>) {
        self.set_phase(|phase| {
            *phase = match outcome {
                Ok((process_id, report)) => PhaseState {
                    phase: PermissionJudgeWorkerPhase::Ready,
                    model_id: Some(spec.model_id.clone()),
                    spec: Some(spec.clone()),
                    process_id,
                    report: Some(report),
                    ..PhaseState::default()
                },
                Err(error) => PhaseState {
                    phase: PermissionJudgeWorkerPhase::Failed,
                    model_id: Some(spec.model_id.clone()),
                    spec: Some(spec.clone()),
                    error: Some(error.to_string()),
                    ..PhaseState::default()
                },
            }
        });
    }

    /// Returns the worker for `spec`, starting it when needed. A failed load of the same spec is
    /// retried only when `retry_failed` is set.
    /// 返回 `spec` 的 worker，必要时启动。同一 spec 的失败加载只有在设置 `retry_failed` 时才重试。
    async fn ensure(
        &self,
        spec: &LoadSpec,
        retry_failed: bool,
    ) -> Result<MutexGuard<'_, Option<LoadedWorker>>, String> {
        let mut slot = self.slot.lock().await;
        if slot
            .as_ref()
            .is_some_and(|loaded| &loaded.spec == spec && loaded.client.is_usable())
        {
            return Ok(slot);
        }
        if !retry_failed
            && let Ok(phase) = self.phase.lock()
            && phase.phase == PermissionJudgeWorkerPhase::Failed
            && phase.spec.as_ref() == Some(spec)
        {
            return Err(phase
                .error
                .clone()
                .unwrap_or_else(|| String::from("The local judge failed to load.")));
        }
        if let Some(previous) = slot.take() {
            previous.client.shutdown().await;
        }
        self.mark_warming(spec);
        match start(spec).await {
            Ok((client, report)) => {
                let limits = report.limits;
                self.mark_loaded(spec, Ok((client.process_id(), report)));
                *slot = Some(LoadedWorker {
                    spec: spec.clone(),
                    client,
                    limits,
                });
                Ok(slot)
            }
            Err(error) => {
                self.mark_loaded(spec, Err(&error));
                Err(error)
            }
        }
    }

    /// Starts or restarts the worker for `spec` as an explicit user action.
    /// 作为用户明确操作启动或重启 `spec` 的 worker。
    pub(crate) async fn warm(&self, spec: &LoadSpec) -> Result<(), String> {
        self.ensure(spec, true).await.map(|_| ())
    }

    /// Verifies and warms `spec` in a new worker while the current one keeps serving, then
    /// swaps them; on failure the current worker stays in place.
    /// 在新 worker 中验证并预热 `spec`，当前 worker 继续服务，然后交换；失败时保留当前 worker。
    pub(crate) async fn switch_to(&self, spec: &LoadSpec) -> Result<(), String> {
        let (client, report) = start(spec).await?;
        let mut slot = self.slot.lock().await;
        if let Some(previous) = slot.take() {
            previous.client.shutdown().await;
        }
        let limits = report.limits;
        self.mark_loaded(spec, Ok((client.process_id(), report)));
        *slot = Some(LoadedWorker {
            spec: spec.clone(),
            client,
            limits,
        });
        Ok(())
    }

    /// Verifies and warms `spec` in a throwaway worker, leaving the current one untouched.
    /// 在一次性 worker 中验证并预热 `spec`，不影响当前 worker。
    pub(crate) async fn verify(&self, spec: &LoadSpec) -> Result<LoadReport, String> {
        let (client, report) = start(spec).await?;
        client.shutdown().await;
        Ok(report)
    }

    /// Stops the worker.
    /// 停止 worker。
    pub(crate) async fn unload(&self) {
        let mut slot = self.slot.lock().await;
        if let Some(previous) = slot.take() {
            previous.client.shutdown().await;
        }
        self.set_phase(|phase| *phase = PhaseState::default());
    }

    /// Stops the worker and records why the model in use cannot be loaded at all, such as a device
    /// that is no longer available.
    /// 停止 worker 并记录使用中模型完全无法加载的原因，例如设备已不可用。
    pub(crate) async fn record_unavailable(&self, model_id: &str, error: String) {
        let mut slot = self.slot.lock().await;
        if let Some(previous) = slot.take() {
            previous.client.shutdown().await;
        }
        self.set_phase(|phase| {
            *phase = PhaseState {
                phase: PermissionJudgeWorkerPhase::Failed,
                model_id: Some(model_id.to_string()),
                error: Some(error),
                ..PhaseState::default()
            }
        });
    }

    /// Judges `items` with the loaded worker when it holds `model_id`, waiting at most
    /// [`JUDGE_WARMUP_WAIT`] for a warm-up in progress. It never starts or retries a load, so a
    /// missing or failed worker sends the call to the user instead.
    /// 当已加载 worker 持有 `model_id` 时用它判定 `items`，对进行中的预热最多等待
    /// [`JUDGE_WARMUP_WAIT`]。它从不启动或重试加载，因此缺失或失败的 worker 会让调用交给用户。
    ///
    /// Concurrent callers, such as the gates of one model reply's tool calls, are merged: each
    /// queues its calls, and whoever holds the worker next sends every queued call in requests of
    /// at most the package's `max_request_items`, so a caller may be answered by another one.
    /// 并发调用方（例如同一模型回复中各工具调用的闸门）会被合并：每个调用方把调用加入队列，下一个持有
    /// worker 的调用方以每个请求最多 `max_request_items` 条的方式发送全部排队调用，因此调用方可能由
    /// 其他调用方代为应答。
    pub(crate) async fn evaluate(
        &self,
        model_id: &str,
        items: Vec<JudgeInput>,
    ) -> Result<Vec<JudgeOutcome>, String> {
        let (reply, mut answer) = oneshot::channel();
        if let Ok(mut queue) = self.queue.lock() {
            queue.push(Waiting {
                model_id: model_id.to_string(),
                items,
                reply,
            });
        }
        let slot = tokio::select! {
            answered = &mut answer => return answered_or_dropped(answered),
            slot = timeout(JUDGE_WARMUP_WAIT, self.slot.lock()) => slot
                .map_err(|_| String::from("The local judge is still warming up."))?,
        };
        self.serve_queue(slot).await;
        answered_or_dropped(answer.await)
    }

    /// Sends every queued call to the loaded worker in merged requests and answers each caller;
    /// callers whose model is not the loaded one are refused.
    /// 以合并请求将全部排队调用发送给已加载 worker 并应答每个调用方；模型不是已加载模型的调用方被拒绝。
    async fn serve_queue(&self, mut slot: MutexGuard<'_, Option<LoadedWorker>>) {
        let waiting: Vec<Waiting> = match self.queue.lock() {
            Ok(mut queue) => queue.drain(..).collect(),
            Err(_) => return,
        };
        // A caller that stopped waiting needs no answer.
        // 已停止等待的调用方无需应答。
        let waiting = waiting.into_iter().filter(|entry| !entry.reply.is_closed());
        let loaded = slot.as_mut().filter(|loaded| loaded.client.is_usable());
        let Some(loaded) = loaded else {
            let reason = self.not_loaded_reason();
            for entry in waiting {
                let _ = entry.reply.send(Err(reason.clone()));
            }
            return;
        };
        let (served, refused): (Vec<Waiting>, Vec<Waiting>) =
            waiting.partition(|entry| entry.model_id == loaded.spec.model_id);
        let reason = self.not_loaded_reason();
        for entry in refused {
            let _ = entry.reply.send(Err(reason.clone()));
        }
        let (groups, replies): (Vec<Vec<JudgeInput>>, Vec<_>) = served
            .into_iter()
            .map(|entry| (entry.items, entry.reply))
            .unzip();
        let limit = loaded.limits.max_request_items as usize;
        let mut answered = Vec::new();
        for request in merge_requests(groups, limit) {
            let (owners, items): (Vec<usize>, Vec<JudgeInput>) = request.into_iter().unzip();
            answered.push((owners, loaded.client.evaluate(items).await));
        }
        let results = split_results(replies.len(), answered);
        for (reply, result) in replies.into_iter().zip(results) {
            let _ = reply.send(result);
        }
    }

    /// Why calls cannot be judged now: the last load failure, or that nothing is loaded.
    /// 当前无法判定调用的原因：上次加载失败，或尚未加载。
    fn not_loaded_reason(&self) -> String {
        self.phase
            .lock()
            .ok()
            .and_then(|phase| phase.error.clone())
            .unwrap_or_else(|| String::from("The local judge is not loaded."))
    }

    /// Times `batch_size` self-check inputs on the model of `spec`.
    /// 在 `spec` 的模型上为 `batch_size` 条自检输入计时。
    pub(crate) async fn benchmark(
        &self,
        spec: &LoadSpec,
        batch_size: u32,
    ) -> Result<BenchmarkReport, String> {
        let mut slot = self.ensure(spec, true).await?;
        let loaded = slot
            .as_mut()
            .ok_or_else(|| String::from("The local judge is not loaded."))?;
        loaded.client.benchmark(batch_size).await
    }

    /// Current status; asks a ready worker for fresh resource use when it is not busy.
    /// 当前状态；就绪的 worker 不忙时向其获取最新资源占用。
    pub(crate) async fn status(&self) -> PermissionJudgeWorkerStatus {
        if let Ok(mut slot) = timeout(STATUS_POLL_WAIT, self.slot.lock()).await
            && let Some(loaded) = slot.as_mut()
        {
            match loaded.client.status().await {
                Ok(status) => self.set_phase(|phase| phase.status = Some(status)),
                // A worker that can no longer answer is dropped, which kills its process.
                // 无法再应答的 worker 会被丢弃，从而结束其进程。
                Err(error) if !loaded.client.is_usable() => {
                    let spec = loaded.spec.clone();
                    *slot = None;
                    self.mark_loaded(&spec, Err(&error));
                }
                Err(_) => {}
            }
        }
        let phase = self
            .phase
            .lock()
            .map(|phase| phase.clone())
            .unwrap_or_default();
        let status = phase.status.as_ref();
        let limits = phase.report.as_ref().map(|report| report.limits);
        PermissionJudgeWorkerStatus {
            phase: phase.phase,
            model_id: phase.model_id.clone(),
            device: phase.spec.as_ref().map(|spec| spec.device),
            error: phase.error.clone(),
            process_id: phase.process_id,
            verify_ms: phase.report.as_ref().map(|report| report.verify_ms),
            load_ms: phase.report.as_ref().map(|report| report.load_ms),
            warmup_ms: phase.report.as_ref().map(|report| report.warmup_ms),
            working_set_bytes: status.and_then(|status| status.working_set_bytes),
            private_bytes: status.and_then(|status| status.private_bytes),
            gpu_dedicated_bytes: status.and_then(|status| status.gpu_dedicated_bytes),
            gpu_shared_bytes: status.and_then(|status| status.gpu_shared_bytes),
            gpu_utilization_percent: status.and_then(|status| status.gpu_utilization_percent),
            evaluations: status.map_or(0, |status| status.evaluations),
            recent_p50_ms: status.and_then(|status| status.recent_p50_ms),
            recent_p95_ms: status.and_then(|status| status.recent_p95_ms),
            max_tokens: limits.map(|limits| limits.max_tokens),
            max_request_items: limits.map(|limits| limits.max_request_items),
            micro_batch: limits.map(|limits| limits.micro_batch),
            padded_batch_tokens: limits.map(|limits| limits.padded_batch_tokens),
        }
    }
}

#[cfg(test)]
mod tests {

    use vulcan_permission_judge::protocol::JudgeDevice;
    use vulcan_provider_config::provider_config::{
        PersistedPermissionJudgeConfiguration, RegisteredJudgeModel,
    };

    use super::*;
    use crate::permission_judge::environment::JudgeEnvironment;

    fn input(rendered: &str) -> JudgeInput {
        JudgeInput {
            settings: Default::default(),
            rendered: rendered.to_string(),
        }
    }

    fn outcome(input_tokens: u32) -> JudgeOutcome {
        JudgeOutcome::InputTokenLimit { input_tokens }
    }

    // Concurrent callers share requests; a caller larger than the limit is split across requests.
    // 并发调用方共享请求；超过上限的调用方被拆分到多个请求。
    #[test]
    fn merged_requests_respect_the_limit_and_keep_owners() {
        let groups = vec![
            vec![input("a0")],
            vec![input("b0"), input("b1"), input("b2")],
            vec![input("c0")],
        ];
        let requests = merge_requests(groups, 2);
        let shape: Vec<Vec<(usize, &str)>> = requests
            .iter()
            .map(|request| {
                request
                    .iter()
                    .map(|(owner, item)| (*owner, item.rendered.as_str()))
                    .collect()
            })
            .collect();
        assert_eq!(
            shape,
            [
                vec![(0, "a0"), (1, "b0")],
                vec![(1, "b1"), (1, "b2")],
                vec![(2, "c0")]
            ]
        );
    }

    // Each caller gets its own outcomes in order; a failed request fails every caller in it.
    // 每个调用方按顺序得到自己的结果；失败的请求使其中每个调用方失败。
    #[test]
    fn split_results_return_each_callers_outcomes_or_its_failure() {
        let answered = vec![
            (vec![0, 1], Ok(vec![outcome(1), outcome(2)])),
            (vec![1, 1], Ok(vec![outcome(3), outcome(4)])),
            (vec![2], Err(String::from("worker failed"))),
        ];
        let results = split_results(4, answered);
        assert_eq!(results[0], Ok(vec![outcome(1)]));
        assert_eq!(results[1], Ok(vec![outcome(2), outcome(3), outcome(4)]));
        assert_eq!(results[2], Err(String::from("worker failed")));
        assert_eq!(
            results[3],
            Ok(Vec::new()),
            "a caller without calls gets none"
        );
    }

    /// Loads a real package on the bundled CPU and WebGPU runtimes, switches between them, times
    /// both speed-test sizes and checks that a broken package fails without replacing the device.
    /// 在随包 CPU 与 WebGPU 执行库上加载真实模型包并相互切换，为两种测速规模计时，并检查损坏的模型包
    /// 会失败且不替换设备。
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs a model package in VULCAN_JUDGE_TEST_PACKAGE, the bundled runtime and the judge host"]
    async fn real_package_switches_devices_and_benchmarks() {
        let package = std::env::var("VULCAN_JUDGE_TEST_PACKAGE").expect("package directory");
        let environment = JudgeEnvironment::probe();
        let model = RegisteredJudgeModel {
            id: String::from("test"),
            name: String::from("test"),
            path: package,
            known_package_id: None,
            parent_sha256: String::new(),
            policy_version: String::new(),
            precision: String::new(),
            total_bytes: 0,
            registered_at_ms: 0,
        };
        let worker = JudgeWorker::default();
        let mut last = None;
        for device in [JudgeDevice::Cpu, JudgeDevice::WebGpu] {
            let settings = PersistedPermissionJudgeConfiguration {
                device: Some(device),
                ..Default::default()
            };
            let spec = environment.load_spec(&settings, &model).unwrap();
            worker.switch_to(&spec).await.unwrap();
            let status = worker.status().await;
            assert_eq!(status.phase, PermissionJudgeWorkerPhase::Ready);
            assert_eq!(status.device, Some(device));
            assert!(status.working_set_bytes.is_some());
            for batch_size in [1, 10] {
                let report = worker.benchmark(&spec, batch_size).await.unwrap();
                assert_eq!((report.device, report.batch_size), (device, batch_size));
                eprintln!("{device:?} x{batch_size}: p50 {:.1} ms", report.p50_ms);
            }
            let status = worker.status().await;
            eprintln!(
                "{device:?}: dedicated {:?} shared {:?} utilization {:?}",
                status.gpu_dedicated_bytes, status.gpu_shared_bytes, status.gpu_utilization_percent
            );
            if cfg!(windows) && device == JudgeDevice::WebGpu {
                assert!(status.gpu_dedicated_bytes.is_some_and(|bytes| bytes > 0));
            }
            last = Some(spec);
        }
        let broken = LoadSpec {
            package_dir: std::env::temp_dir().join("vulcan-missing-judge-package"),
            ..last.unwrap()
        };
        assert!(worker.warm(&broken).await.is_err());
        let status = worker.status().await;
        assert_eq!(status.phase, PermissionJudgeWorkerPhase::Failed);
        assert_eq!(status.device, Some(JudgeDevice::WebGpu));
        worker.unload().await;
        assert_eq!(
            worker.status().await.phase,
            PermissionJudgeWorkerPhase::NotLoaded
        );
    }
}
