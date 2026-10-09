//! The single local judge worker: start, warm-up, atomic model switch and status.
//! 唯一的本机判定 worker：启动、预热、原子模型切换与状态。
//!
//! One worker loads one model on one device. A worker that no longer matches the settings is
//! replaced; a load that failed is not retried by evaluations, only by an explicit user action, so a
//! broken setup costs one failure instead of one per tool call.
//! 一个 worker 在一个设备上加载一个模型。不再与设置匹配的 worker 会被替换；加载失败后评估不会重试，
//! 只有用户的明确操作才会重试，因此损坏的配置只失败一次，而不是每次工具调用都失败。

use std::sync::Mutex;

use tokio::sync::{Mutex as AsyncMutex, MutexGuard};
use tokio::time::{Duration, timeout};
use vulcan_host_tools::judge_host_client::JudgeHostClient;
use vulcan_permission_judge::protocol::{BenchmarkReport, HostStatus, LoadReport};
use vulcan_service_api::{PermissionJudgeWorkerPhase, PermissionJudgeWorkerStatus};

use super::environment::LoadSpec;

/// Longest a status poll waits for the worker, which may be busy judging.
/// 状态轮询等待 worker 的最长时间；worker 可能正在判定。
const STATUS_POLL_WAIT: Duration = Duration::from_millis(200);

/// A started worker and the model it holds.
/// 已启动的 worker 及其持有的模型。
struct LoadedWorker {
    spec: LoadSpec,
    client: JudgeHostClient,
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
                self.mark_loaded(spec, Ok((client.process_id(), report)));
                *slot = Some(LoadedWorker {
                    spec: spec.clone(),
                    client,
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
        self.mark_loaded(spec, Ok((client.process_id(), report)));
        *slot = Some(LoadedWorker {
            spec: spec.clone(),
            client,
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
