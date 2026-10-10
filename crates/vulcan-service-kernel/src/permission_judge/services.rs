//! Permission judge RPC handlers.
//! 权限判定 RPC 处理函数。
//!
//! Settings are written only by these user commands. Verification and warm-up run before a model
//! becomes the one in use, and nothing falls back to another device or model on failure.
//! 设置只由这些用户命令写入。模型成为使用中模型前先完成验证与预热，失败时不会退回其他设备或模型。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vulcan_agent_runtime::agent_runtime::tool_permission::{
    JudgeCall, JudgeFailure, JudgeTrace, JudgeVerdict, PermissionRecord,
};
use vulcan_permission_judge::protocol::{JudgeDevice, JudgeInput, JudgeOutcome};
use vulcan_permission_judge::{
    JudgePackage, JudgePolicy, cloud_policy, find_known_package, known_packages, render_call,
};
use vulcan_provider_config::provider_config::{
    JudgeBackend, JudgeState, PersistedPermissionJudgeConfiguration, RegisteredJudgeModel,
    load_project_configuration_file, save_project_configuration_file,
};
use vulcan_service_api::{
    PermissionJudgeBenchmarkRequest, PermissionJudgeBenchmarkResult, PermissionJudgeCudaArchive,
    PermissionJudgeCudaComponent, PermissionJudgeCudaDirectory, PermissionJudgeCudaInstall,
    PermissionJudgeCudaInstallRequest, PermissionJudgeCudaSnapshot, PermissionJudgeDeviceSummary,
    PermissionJudgeIssue, PermissionJudgeKnownPackage, PermissionJudgeLink,
    PermissionJudgeModelRegisterRequest, PermissionJudgeModelRequest, PermissionJudgeModelSummary,
    PermissionJudgeNvidiaDriver, PermissionJudgePackageFacts, PermissionJudgePackageInspectRequest,
    PermissionJudgePackageInspection, PermissionJudgePolicyOption, PermissionJudgeRecord,
    PermissionJudgeRecordExport, PermissionJudgeRecordMarkRequest, PermissionJudgeRecordPage,
    PermissionJudgeRecordsRequest, PermissionJudgeSettingsSaveRequest,
    PermissionJudgeSettingsSnapshot, PermissionJudgeStatus, ProjectPermissionJudgePolicyOption,
    ProjectPermissionJudgePolicySaveRequest, ProjectPermissionJudgePolicySnapshot,
    ServiceEventEmitter,
};
use vulcan_service_runtime::judge_cuda_runtime::{
    CudaComponent, CudaLibrarySource, host_platform, install_component, missing_cuda_libraries,
    official_links,
};

use super::PermissionJudgeRuntime;
use super::cloud::{self, CloudCall};
use super::environment::{DEVICE_PREFERENCE, JudgeEnvironment, LoadSpec, device_name};
use crate::runtime_workspace_helpers::workspace_task_project_for_project_id;
use crate::{
    SharedServiceBridgeState, current_v1_unix_millis, emit_workbench_event,
    mutate_provider_configuration,
};

/// Rows a speed test may time: one call, or a batch of ten.
/// 速度测试可计时的行数：一条调用或十条批次。
const BENCHMARK_BATCH_SIZES: [u32; 2] = [1, 10];

/// How often clients re-read the live status while the settings page is open; the only definition,
/// published in every status read.
/// 设置页打开期间客户端重新读取实时状态的间隔；唯一定义，随每次状态读取发布。
const STATUS_POLL_INTERVAL_MS: u64 = 2_000;

/// Reads the runtime and the saved judge settings.
/// 读取运行时与已保存的判定设置。
pub(super) fn read_state(
    state: &SharedServiceBridgeState,
) -> Result<
    (
        Arc<PermissionJudgeRuntime>,
        PersistedPermissionJudgeConfiguration,
    ),
    String,
> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let settings = guard
        .agent_runtime
        .as_ref()
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?
        .provider_configuration
        .permission_judge
        .clone();
    Ok((Arc::clone(&guard.permission_judge), settings))
}

/// Saves a judge settings mutation and returns the saved settings.
/// 保存一次判定设置变更并返回已保存的设置。
pub(super) async fn save_settings(
    state: &SharedServiceBridgeState,
    mutate: impl FnOnce(&mut PersistedPermissionJudgeConfiguration) -> Result<(), String>,
) -> Result<PersistedPermissionJudgeConfiguration, String> {
    mutate_provider_configuration(
        state,
        "保存权限判定设置失败",
        |configuration| mutate(&mut configuration.permission_judge),
        |_, configuration| configuration.permission_judge.clone(),
    )
    .await
}

/// Runs blocking probing or hashing off the async executor.
/// 在异步执行器之外运行阻塞的探测或哈希。
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| format!("permission judge task failed: {error}"))
}

/// Whether the saved settings run the local judge.
/// 已保存设置是否运行本机判定器。
pub(super) fn local_enabled(settings: &PersistedPermissionJudgeConfiguration) -> bool {
    settings.state == Some(JudgeState::Enabled) && settings.backend == Some(JudgeBackend::Local)
}

/// Whether the saved settings run the cloud judge.
/// 已保存设置是否运行云端判定。
fn cloud_enabled(settings: &PersistedPermissionJudgeConfiguration) -> bool {
    settings.state == Some(JudgeState::Enabled) && settings.backend == Some(JudgeBackend::Cloud)
}

/// Policy of the chosen backend: the cloud policy, or the local model in use; `None` when neither
/// applies or the local package cannot be opened.
/// 所选后端的策略：云端策略或使用中本机模型的策略；两者都不适用或本机模型包无法打开时为 `None`。
pub(super) async fn backend_policy(
    runtime: &Arc<PermissionJudgeRuntime>,
    settings: &PersistedPermissionJudgeConfiguration,
) -> Result<Option<JudgePolicy>, PermissionJudgeIssue> {
    if settings.backend == Some(JudgeBackend::Cloud) {
        return Ok(Some(cloud_policy().clone()));
    }
    match settings.active_model() {
        Some(model) => open_package(runtime, PathBuf::from(&model.path))
            .await
            .map(|package| Some(package.policy)),
        None => Ok(None),
    }
}

/// Opens and verifies a package directory with the shared digest cache.
/// 使用共享摘要缓存打开并验证模型包目录。
pub(super) async fn open_package(
    runtime: &Arc<PermissionJudgeRuntime>,
    path: PathBuf,
) -> Result<JudgePackage, PermissionJudgeIssue> {
    let cache = Arc::clone(&runtime.digest_cache);
    blocking(move || JudgePackage::open(&path, cache.as_ref()))
        .await
        .map_err(|detail| PermissionJudgeIssue {
            code: String::from("internal"),
            detail,
        })?
        .map_err(|error| PermissionJudgeIssue {
            code: error.code().to_string(),
            detail: error.to_string(),
        })
}

/// Load spec for `model_id` under `settings`.
/// 在 `settings` 下 `model_id` 的加载规格。
async fn load_spec(
    settings: &PersistedPermissionJudgeConfiguration,
    model_id: &str,
) -> Result<LoadSpec, String> {
    let model = settings
        .model(model_id)
        .cloned()
        .ok_or_else(|| format!("No judge model is registered as `{model_id}`."))?;
    let settings = settings.clone();
    blocking(move || JudgeEnvironment::probe().load_spec(&settings, &model)).await?
}

/// Policy options in the order the model was trained with.
/// 按模型训练顺序排列的策略选项。
fn policy_options(policy: &JudgePolicy) -> Vec<PermissionJudgePolicyOption> {
    policy
        .options
        .iter()
        .map(|(key, option)| PermissionJudgePolicyOption {
            key: key.clone(),
            default_mode: option.default.clone(),
            modes: option.modes.keys().cloned().collect(),
        })
        .collect()
}

/// Projects the CUDA part of the environment.
/// 投影环境中的 CUDA 部分。
fn cuda_snapshot(
    environment: &JudgeEnvironment,
    settings: &PersistedPermissionJudgeConfiguration,
) -> PermissionJudgeCudaSnapshot {
    let target = environment.cuda_target.as_ref();
    let locked = settings.cuda_library_directory.clone();
    let archives = target
        .map(|target| {
            [
                (
                    PermissionJudgeCudaComponent::Onnxruntime,
                    CudaComponent::Onnxruntime,
                ),
                (
                    PermissionJudgeCudaComponent::CudaLibraries,
                    CudaComponent::CudaLibraries,
                ),
            ]
            .into_iter()
            .flat_map(|(component, source)| {
                target
                    .archives(source)
                    .iter()
                    .map(move |archive| PermissionJudgeCudaArchive {
                        component,
                        name: archive.name.clone(),
                        url: archive.url.clone(),
                        size: archive.size,
                    })
            })
            .collect()
        })
        .unwrap_or_default();
    let (driver, driver_error) = match &environment.driver {
        Ok(driver) => (
            driver.as_ref().map(|driver| PermissionJudgeNvidiaDriver {
                version: driver.version.clone(),
                major: driver.major,
                gpus: driver.gpus.clone(),
            }),
            None,
        ),
        Err(error) => (None, Some(error.clone())),
    };
    PermissionJudgeCudaSnapshot {
        platform: host_platform(),
        supported: target.is_some(),
        cuda_major: target.map(|target| target.cuda_major),
        minimum_driver_major: target.map(|target| target.minimum_driver_major),
        driver,
        driver_error,
        required_libraries: target
            .map(|target| target.required_libraries().to_vec())
            .unwrap_or_default(),
        onnxruntime_directory: environment
            .onnxruntime_gpu
            .as_ref()
            .map(|directory| directory.display().to_string()),
        detected_directories: environment
            .detected
            .iter()
            .map(|found| PermissionJudgeCudaDirectory {
                source: String::from(match found.source {
                    CudaLibrarySource::Managed => "managed",
                    CudaLibrarySource::Toolkit => "toolkit",
                    CudaLibrarySource::SearchPath => "search_path",
                }),
                directory: found.directory.display().to_string(),
            })
            .collect(),
        locked_directory_missing: match (target, &locked) {
            (Some(target), Some(directory)) => missing_cuda_libraries(target, Path::new(directory)),
            _ => Vec::new(),
        },
        locked_directory: locked,
        archives,
        links: official_links()
            .map(|links| {
                links
                    .iter()
                    .map(|link| PermissionJudgeLink {
                        id: link.id.clone(),
                        url: link.url.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Live worker and installation status.
/// 实时 worker 与安装状态。
async fn status_of(runtime: &PermissionJudgeRuntime) -> PermissionJudgeStatus {
    PermissionJudgeStatus {
        worker: runtime.worker.status().await,
        cuda_install: runtime
            .cuda_install
            .lock()
            .ok()
            .and_then(|install| install.clone()),
        poll_interval_ms: STATUS_POLL_INTERVAL_MS,
    }
}

/// Builds the settings page snapshot.
/// 构建设置页快照。
async fn snapshot(
    runtime: &Arc<PermissionJudgeRuntime>,
    settings: PersistedPermissionJudgeConfiguration,
) -> Result<PermissionJudgeSettingsSnapshot, String> {
    let environment = blocking(JudgeEnvironment::probe).await?;
    let (policy_options, active_model_issue) = match backend_policy(runtime, &settings).await {
        Ok(policy) => (
            policy.as_ref().map(policy_options).unwrap_or_default(),
            None,
        ),
        Err(issue) => (Vec::new(), Some(issue)),
    };
    let locked = settings.cuda_library_directory.as_deref().map(Path::new);
    Ok(PermissionJudgeSettingsSnapshot {
        state: settings.state,
        backend: settings.backend,
        device: settings.device,
        recommended_device: environment.recommended_device(locked),
        devices: DEVICE_PREFERENCE
            .into_iter()
            .map(|device| PermissionJudgeDeviceSummary {
                device,
                unavailable_reason: environment
                    .unavailable_reason(device, locked)
                    .map(str::to_string),
            })
            .collect(),
        llm_may_allow_blocked: settings.llm_may_allow_blocked,
        record_decisions: settings.record_decisions,
        models: settings
            .models
            .iter()
            .map(|model| PermissionJudgeModelSummary {
                id: model.id.clone(),
                name: model.name.clone(),
                path: model.path.clone(),
                known_package_id: model.known_package_id.clone(),
                policy_version: model.policy_version.clone(),
                precision: model.precision.clone(),
                total_bytes: model.total_bytes,
                registered_at_ms: model.registered_at_ms,
            })
            .collect(),
        active_model_id: settings.active_model_id.clone(),
        previous_model_id: settings.previous_model_id.clone(),
        known_packages: known_packages()
            .iter()
            .map(|package| PermissionJudgeKnownPackage {
                id: package.id.clone(),
                display_name: package.display_name.clone(),
            })
            .collect(),
        policy_options,
        active_model_issue,
        cuda: cuda_snapshot(&environment, &settings),
        status: status_of(runtime).await,
    })
}

/// Returns the settings page snapshot.
/// 返回设置页快照。
pub(crate) async fn service_permission_judge_settings(
    state: SharedServiceBridgeState,
) -> Result<PermissionJudgeSettingsSnapshot, String> {
    let (runtime, settings) = read_state(&state)?;
    snapshot(&runtime, settings).await
}

/// Returns the live status polled while the settings page is open.
/// 返回设置页打开期间轮询的实时状态。
pub(crate) async fn service_permission_judge_status(
    state: SharedServiceBridgeState,
) -> Result<PermissionJudgeStatus, String> {
    let (runtime, _) = read_state(&state)?;
    Ok(status_of(&runtime).await)
}

/// Brings the worker in line with the saved settings: warms the active model when the local judge
/// is enabled, and stops the worker otherwise. Warm-up runs in the background.
/// 使 worker 与已保存设置一致：本机判定器启用时预热使用中模型，否则停止 worker。预热在后台运行。
fn reconcile_worker(
    runtime: &Arc<PermissionJudgeRuntime>,
    settings: &PersistedPermissionJudgeConfiguration,
) {
    let runtime = Arc::clone(runtime);
    let settings = settings.clone();
    tokio::spawn(async move {
        let Some(model_id) = settings
            .active_model_id
            .clone()
            .filter(|_| local_enabled(&settings))
        else {
            runtime.worker.unload().await;
            return;
        };
        match load_spec(&settings, &model_id).await {
            Ok(spec) => {
                // The outcome is recorded in the worker phase shown by the settings page.
                // 结果记录在设置页展示的 worker 阶段中。
                let _ = runtime.worker.warm(&spec).await;
            }
            Err(error) => runtime.worker.record_unavailable(&model_id, error).await,
        }
    });
}

/// Warms the saved configuration at service start when the local judge is enabled.
/// 本机判定器启用时，在服务启动时预热已保存的配置。
pub(crate) fn start_permission_judge(state: &SharedServiceBridgeState) {
    if let Ok((runtime, settings)) = read_state(state)
        && local_enabled(&settings)
    {
        reconcile_worker(&runtime, &settings);
    }
}

/// Saves the system-level choices. Choosing CUDA locks a complete library directory: the given
/// one, or the first detected one.
/// 保存系统级选择。选择 CUDA 会锁定一个完整的库目录：给定的目录，或第一个检测到的目录。
pub(crate) async fn service_save_permission_judge_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PermissionJudgeSettingsSaveRequest,
) -> Result<PermissionJudgeSettingsSnapshot, String> {
    let (runtime, current) = read_state(&state)?;
    if request.state == JudgeState::Enabled {
        if request.backend.is_none() {
            return Err(String::from(
                "Choose a backend to enable the permission judge.",
            ));
        }
        if request.backend == Some(JudgeBackend::Local) && current.active_model().is_none() {
            return Err(String::from(
                "Register and activate a local model before enabling the local judge.",
            ));
        }
        if request.backend == Some(JudgeBackend::Cloud)
            && cloud::configured_model(&state)?.is_none()
        {
            return Err(String::from(
                "Choose a decision model before enabling the cloud judge.",
            ));
        }
    }
    let environment = blocking(JudgeEnvironment::probe).await?;
    let cuda_library_directory = match &request.cuda_library_directory {
        Some(directory) => {
            let path = Path::new(directory);
            if !path.is_absolute() {
                return Err(format!("CUDA library directory `{directory}` is not absolute."));
            }
            let target = environment
                .cuda_target
                .as_ref()
                .ok_or_else(|| String::from("CUDA is not supported on this platform."))?;
            let missing = missing_cuda_libraries(target, path);
            if !missing.is_empty() {
                return Err(format!(
                    "CUDA library directory `{directory}` lacks {}.",
                    missing.join(", ")
                ));
            }
            Some(directory.clone())
        }
        None if request.device == Some(JudgeDevice::Cuda) => Some(
            environment
                .detected
                .first()
                .map(|found| found.directory.display().to_string())
                .ok_or_else(|| {
                    String::from(
                        "No complete CUDA library directory was found; install the CUDA libraries or choose a directory.",
                    )
                })?,
        ),
        None => None,
    };
    if let Some(device) = request.device
        && let Some(reason) =
            environment.unavailable_reason(device, cuda_library_directory.as_deref().map(Path::new))
    {
        return Err(format!(
            "The {} device cannot run the local judge: {reason}.",
            device_name(device)
        ));
    }
    let settings = save_settings(&state, |settings| {
        settings.state = Some(request.state);
        settings.backend = request.backend;
        settings.device = request.device;
        settings.cuda_library_directory = cuda_library_directory;
        settings.llm_may_allow_blocked = request.llm_may_allow_blocked;
        settings.record_decisions = request.record_decisions;
        Ok(())
    })
    .await?;
    // The composer's permission mode selector reads the switch from the workbench.
    // 输入区的权限模式选择器从工作台读取该开关。
    if let Ok(mut guard) = state.lock() {
        guard.workbench.permission_judge_state = settings.state;
        emit_workbench_event(&app, &guard.workbench);
    }
    reconcile_worker(&runtime, &settings);
    snapshot(&runtime, settings).await
}

/// Name a registration gets when the user leaves it blank: the official name, else the directory
/// name, else the manifest digest.
/// 用户未填名称时登记使用的名称：官方名称，其次目录名，最后清单摘要。
fn default_model_name(path: &Path, package: &JudgePackage) -> String {
    find_known_package(&package.summary.parent_sha256)
        .map(|known| known.display_name.clone())
        .or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| package.summary.manifest_sha256.clone())
}

/// Verifies a package directory completely without registering it, so a form can show whether the
/// folder is usable before submitting; the digest cache makes the following registration cheap.
/// 完整验证模型包目录但不登记，使表单提交前即可显示文件夹是否可用；摘要缓存让随后的登记很快完成。
pub(crate) async fn service_inspect_permission_judge_package(
    state: SharedServiceBridgeState,
    request: PermissionJudgePackageInspectRequest,
) -> Result<PermissionJudgePackageInspection, String> {
    let (runtime, settings) = read_state(&state)?;
    let path = PathBuf::from(request.path.trim());
    let package = match open_package(&runtime, path.clone()).await {
        Ok(package) => package,
        Err(issue) => {
            return Ok(PermissionJudgePackageInspection {
                issue: Some(issue),
                package: None,
            });
        }
    };
    let model_id = package.summary.manifest_sha256.clone();
    Ok(PermissionJudgePackageInspection {
        issue: None,
        package: Some(PermissionJudgePackageFacts {
            registered: settings.model(&model_id).is_some(),
            known_package_id: find_known_package(&package.summary.parent_sha256)
                .map(|known| known.id.clone()),
            default_name: default_model_name(&path, &package),
            policy_version: package.summary.policy_version.clone(),
            precision: package.summary.precision.clone(),
            total_bytes: package.summary.total_bytes,
            model_id,
        }),
    })
}

/// Verifies a package directory completely and registers it.
/// 完整验证模型包目录并登记。
pub(crate) async fn service_register_permission_judge_model(
    state: SharedServiceBridgeState,
    request: PermissionJudgeModelRegisterRequest,
) -> Result<PermissionJudgeSettingsSnapshot, String> {
    let (runtime, _) = read_state(&state)?;
    let path = PathBuf::from(request.path.trim());
    let package = open_package(&runtime, path.clone())
        .await
        .map_err(|issue| format!("[{}] {}", issue.code, issue.detail))?;
    let known = find_known_package(&package.summary.parent_sha256);
    if known.is_none() && !request.custom {
        return Err(String::from(
            "This package is not an official judge model; add it as a custom model instead.",
        ));
    }
    let name = match request.name.trim() {
        "" => default_model_name(&path, &package),
        name => name.to_string(),
    };
    let model = RegisteredJudgeModel {
        id: package.summary.manifest_sha256.clone(),
        name,
        path: path.display().to_string(),
        known_package_id: known.map(|package| package.id.clone()),
        parent_sha256: package.summary.parent_sha256.clone(),
        policy_version: package.summary.policy_version.clone(),
        precision: package.summary.precision.clone(),
        total_bytes: package.summary.total_bytes,
        registered_at_ms: u64::try_from(current_v1_unix_millis()?)
            .map_err(|_| String::from("The system clock is before 1970."))?,
    };
    let settings = save_settings(&state, |settings| {
        settings.register(model);
        Ok(())
    })
    .await?;
    snapshot(&runtime, settings).await
}

/// Makes a registration the model in use after it verified and warmed up on the configured
/// device; the worker switches atomically and the previous model stays available for rollback.
/// 登记在已设置设备上通过验证与预热后成为使用中模型；worker 原子切换，先前模型保留以便回退。
async fn activate(
    state: &SharedServiceBridgeState,
    model_id: String,
) -> Result<PermissionJudgeSettingsSnapshot, String> {
    let (runtime, current) = read_state(state)?;
    let spec = load_spec(&current, &model_id).await?;
    if local_enabled(&current) {
        runtime.worker.switch_to(&spec).await?;
    } else {
        runtime.worker.verify(&spec).await?;
    }
    let settings = save_settings(state, |settings| {
        settings
            .activate(&model_id)
            .map_err(|error| error.to_string())
    })
    .await?;
    snapshot(&runtime, settings).await
}

/// Switches to a registered model.
/// 切换到一个已登记模型。
pub(crate) async fn service_activate_permission_judge_model(
    state: SharedServiceBridgeState,
    request: PermissionJudgeModelRequest,
) -> Result<PermissionJudgeSettingsSnapshot, String> {
    activate(&state, request.model_id).await
}

/// Returns to the model used before the last switch.
/// 返回上次切换前使用的模型。
pub(crate) async fn service_rollback_permission_judge_model(
    state: SharedServiceBridgeState,
) -> Result<PermissionJudgeSettingsSnapshot, String> {
    let (_, current) = read_state(&state)?;
    let target = current
        .rollback_target()
        .map_err(|error| error.to_string())?
        .id
        .clone();
    activate(&state, target).await
}

/// Removes a registration that is not in use; the files stay on disk.
/// 移除未在使用的登记；文件保留在磁盘上。
pub(crate) async fn service_remove_permission_judge_model(
    state: SharedServiceBridgeState,
    request: PermissionJudgeModelRequest,
) -> Result<PermissionJudgeSettingsSnapshot, String> {
    let (runtime, _) = read_state(&state)?;
    let settings = save_settings(&state, |settings| {
        settings
            .remove(&request.model_id)
            .map_err(|error| error.to_string())
    })
    .await?;
    snapshot(&runtime, settings).await
}

/// Retries loading the model in use, for example after fixing a driver or moving files back.
/// 重新加载使用中模型，例如在修复驱动或移回文件之后。
pub(crate) async fn service_reload_permission_judge(
    state: SharedServiceBridgeState,
) -> Result<PermissionJudgeStatus, String> {
    let (runtime, settings) = read_state(&state)?;
    let model_id = settings
        .active_model_id
        .clone()
        .filter(|_| local_enabled(&settings))
        .ok_or_else(|| String::from("The local judge is not enabled."))?;
    let spec = load_spec(&settings, &model_id).await?;
    // A failed load is reported through the returned worker phase.
    // 加载失败通过返回的 worker 阶段报告。
    let _ = runtime.worker.warm(&spec).await;
    Ok(status_of(&runtime).await)
}

/// Times one call or a batch of ten self-check inputs on the model in use.
/// 在使用中模型上为一条调用或十条自检输入的批次计时。
pub(crate) async fn service_benchmark_permission_judge(
    state: SharedServiceBridgeState,
    request: PermissionJudgeBenchmarkRequest,
) -> Result<PermissionJudgeBenchmarkResult, String> {
    if !BENCHMARK_BATCH_SIZES.contains(&request.batch_size) {
        return Err(format!(
            "Benchmark batch size must be one of {BENCHMARK_BATCH_SIZES:?}."
        ));
    }
    let (runtime, settings) = read_state(&state)?;
    if cloud_enabled(&settings) {
        let model_id = cloud::configured_model(&state)?
            .ok_or_else(|| String::from("Choose a decision model for the cloud judge."))?;
        // One timed round keeps a paid speed test to the calls the user asked for.
        // 仅计时一轮，使付费速度测试只发出用户要求的调用。
        let elapsed_ms = cloud::time_batch(&state, request.batch_size)
            .await?
            .as_secs_f64()
            * 1_000.0;
        return Ok(PermissionJudgeBenchmarkResult {
            device: None,
            model_id,
            batch_size: request.batch_size,
            rounds: 1,
            p50_ms: elapsed_ms,
            p95_ms: elapsed_ms,
            per_item_ms: elapsed_ms / f64::from(request.batch_size),
            items_per_second: f64::from(request.batch_size) * 1_000.0 / elapsed_ms,
        });
    }
    let model_id = settings
        .active_model_id
        .clone()
        .filter(|_| local_enabled(&settings))
        .ok_or_else(|| String::from("Enable the permission judge before running a speed test."))?;
    let spec = load_spec(&settings, &model_id).await?;
    let report = runtime.worker.benchmark(&spec, request.batch_size).await?;
    Ok(PermissionJudgeBenchmarkResult {
        device: Some(report.device),
        model_id,
        batch_size: report.batch_size,
        rounds: report.rounds,
        p50_ms: report.p50_ms,
        p95_ms: report.p95_ms,
        per_item_ms: report.per_item_ms,
        items_per_second: report.items_per_second,
    })
}

/// Starts installing one CUDA runtime part in the background; progress is read through status.
/// 在后台开始安装一个 CUDA 执行库部分；进度通过状态读取。
pub(crate) async fn service_install_permission_judge_cuda(
    state: SharedServiceBridgeState,
    request: PermissionJudgeCudaInstallRequest,
) -> Result<PermissionJudgeStatus, String> {
    let (runtime, _) = read_state(&state)?;
    let environment = blocking(JudgeEnvironment::probe).await?;
    let target = environment
        .cuda_target
        .clone()
        .ok_or_else(|| String::from("CUDA is not supported on this platform."))?;
    let root = environment
        .cuda_root
        .clone()
        .ok_or_else(|| String::from("The user directory is unavailable."))?;
    let component = match request.component {
        PermissionJudgeCudaComponent::Onnxruntime => CudaComponent::Onnxruntime,
        PermissionJudgeCudaComponent::CudaLibraries => CudaComponent::CudaLibraries,
    };
    {
        let mut install = runtime
            .cuda_install
            .lock()
            .map_err(|error| format!("failed to lock the CUDA installation: {error}"))?;
        if install.as_ref().is_some_and(|install| install.running) {
            return Err(String::from("A CUDA installation is already running."));
        }
        *install = Some(PermissionJudgeCudaInstall {
            component: request.component,
            running: true,
            phase: None,
            completed_bytes: None,
            total_bytes: None,
            directory: None,
            error: None,
        });
    }
    let worker_runtime = Arc::clone(&runtime);
    tokio::task::spawn_blocking(move || {
        let update = |change: &dyn Fn(&mut PermissionJudgeCudaInstall)| {
            if let Ok(mut install) = worker_runtime.cuda_install.lock()
                && let Some(install) = install.as_mut()
            {
                change(install);
            }
        };
        let result = install_component(&target, component, &root, &mut |progress| {
            update(&|install| {
                install.phase = Some(progress.phase.clone());
                install.completed_bytes = progress.completed_bytes;
                install.total_bytes = progress.total_bytes;
            })
        });
        update(&|install| {
            install.running = false;
            match &result {
                Ok(directory) => install.directory = Some(directory.display().to_string()),
                Err(error) => install.error = Some(error.clone()),
            }
        });
    });
    Ok(status_of(&runtime).await)
}

/// Resolves a project identity to its directory.
/// 将项目标识解析为其目录。
pub(super) fn project_directory(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<PathBuf, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("workspace is not loaded"))?;
    workspace_task_project_for_project_id(workspace, project_id)
        .map(|project| PathBuf::from(&project.path))
        .ok_or_else(|| format!("unknown project: {project_id}"))
}

/// Policy of the chosen backend.
/// 所选后端的策略。
async fn active_policy(state: &SharedServiceBridgeState) -> Result<JudgePolicy, String> {
    let (runtime, settings) = read_state(state)?;
    backend_policy(&runtime, &settings)
        .await
        .map_err(|issue| format!("[{}] {}", issue.code, issue.detail))?
        .ok_or_else(|| String::from("No judge model is in use."))
}

/// Projects a project's saved choices against `policy`.
/// 将项目已保存的选择按 `policy` 投影。
fn project_policy_snapshot(
    project_id: String,
    policy: &JudgePolicy,
    chosen: &BTreeMap<String, String>,
) -> ProjectPermissionJudgePolicySnapshot {
    ProjectPermissionJudgePolicySnapshot {
        options: policy_options(policy)
            .into_iter()
            .map(|option| ProjectPermissionJudgePolicyOption {
                chosen_mode: chosen.get(&option.key).cloned(),
                option,
            })
            .collect(),
        unsupported_options: chosen
            .iter()
            .filter(|(key, _)| !policy.options.contains_key(*key))
            .map(|(key, mode)| (key.clone(), mode.clone()))
            .collect(),
        issue: policy
            .resolve_settings(chosen)
            .err()
            .map(|error| PermissionJudgeIssue {
                code: String::from("unknown_mode"),
                detail: error.to_string(),
            }),
        project_id,
    }
}

/// Returns a project's choices for the options of the model in use.
/// 返回项目针对使用中模型选项的选择。
pub(crate) async fn service_project_permission_judge_policy(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<ProjectPermissionJudgePolicySnapshot, String> {
    let directory = project_directory(&state, &project_id)?;
    let policy = active_policy(&state).await?;
    let configuration = load_project_configuration_file(&directory)?;
    Ok(project_policy_snapshot(
        project_id,
        &policy,
        &configuration.permission_judge.options,
    ))
}

/// Combines a project's new choices with saved choices for options the model in use no longer
/// has; a choice for an unknown option or an unknown mode is refused.
/// 将项目的新选择与使用中模型已不存在之选项的已保存选择合并；未知选项或未知档位的选择会被拒绝。
fn merge_project_options(
    policy: &JudgePolicy,
    saved: BTreeMap<String, String>,
    chosen: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    if let Some(unknown) = chosen.keys().find(|key| !policy.options.contains_key(*key)) {
        return Err(format!(
            "Option `{unknown}` is not part of the model in use."
        ));
    }
    policy
        .resolve_settings(&chosen)
        .map_err(|error| error.to_string())?;
    let mut options: BTreeMap<String, String> = saved
        .into_iter()
        .filter(|(key, _)| !policy.options.contains_key(key))
        .collect();
    options.extend(chosen);
    Ok(options)
}

/// Saves a project's choices; choices for options the model in use no longer has are kept.
/// 保存项目的选择；使用中模型已不存在之选项的选择会被保留。
pub(crate) async fn service_save_project_permission_judge_policy(
    state: SharedServiceBridgeState,
    request: ProjectPermissionJudgePolicySaveRequest,
) -> Result<ProjectPermissionJudgePolicySnapshot, String> {
    let directory = project_directory(&state, &request.project_id)?;
    let policy = active_policy(&state).await?;
    let mut configuration = load_project_configuration_file(&directory)?;
    configuration.permission_judge.options = merge_project_options(
        &policy,
        std::mem::take(&mut configuration.permission_judge.options),
        request.options,
    )?;
    save_project_configuration_file(&directory, &configuration)?;
    Ok(project_policy_snapshot(
        request.project_id,
        &policy,
        &configuration.permission_judge.options,
    ))
}

/// Reads the policy choices a workspace's project configuration makes.
/// 读取工作区项目配置所作的策略选择。
pub(super) fn load_project_options(root: &Path) -> Result<BTreeMap<String, String>, String> {
    Ok(load_project_configuration_file(root)?
        .permission_judge
        .options)
}

/// Judges one escalated call with the saved backend; `Ok(None)` when the judge is not enabled.
/// 用已保存的后端判定一次升级调用；判定未启用时返回 `Ok(None)`。
pub(crate) async fn judge_call(
    state: &SharedServiceBridgeState,
    call: JudgeCall,
) -> Result<Option<JudgeVerdict>, JudgeFailure> {
    let fail = |reason: String| JudgeFailure {
        reason,
        trace: None,
    };
    let (runtime, settings) = read_state(state).map_err(fail)?;
    if !local_enabled(&settings) && !cloud_enabled(&settings) {
        return Ok(None);
    }
    let raw_arguments = call.raw_arguments.ok_or_else(|| {
        fail(String::from(
            "The original argument text of this call is no longer available.",
        ))
    })?;
    let policy = backend_policy(&runtime, &settings)
        .await
        .map_err(|issue| fail(format!("[{}] {}", issue.code, issue.detail)))?
        .ok_or_else(|| fail(String::from("No judge model is in use.")))?;
    let chosen = match &call.workspace_root {
        Some(root) => load_project_options(root).map_err(fail)?,
        None => BTreeMap::new(),
    };
    let modes = policy
        .resolve_settings(&chosen)
        .map_err(|error| fail(error.to_string()))?;
    let root = call
        .workspace_root
        .as_ref()
        .map(|root| root.display().to_string());
    let trace = |backend: &str, model: String| JudgeTrace {
        backend: backend.to_string(),
        model,
        policy_version: policy.version.clone(),
        settings: modes.to_map(),
        rendered: render_call(root.as_deref(), &call.tool_name, &raw_arguments),
        logits: None,
        allow_probability: None,
        input_tokens: None,
    };
    if cloud_enabled(&settings) {
        let model = cloud::configured_model(state)
            .ok()
            .flatten()
            .unwrap_or_default();
        let trace = trace("cloud", model);
        let judged = cloud::judge(
            state,
            &modes,
            CloudCall {
                project_root: root.as_deref(),
                tool: &call.tool_name,
                raw_arguments: &raw_arguments,
            },
        )
        .await;
        return match judged {
            Ok((label, allow)) => Ok(Some(JudgeVerdict {
                label,
                trace: JudgeTrace {
                    allow_probability: Some(allow),
                    ..trace
                },
            })),
            Err(reason) => Err(JudgeFailure {
                reason,
                trace: Some(trace),
            }),
        };
    }
    let model = settings
        .active_model()
        .ok_or_else(|| fail(String::from("No judge model is in use.")))?;
    let trace = trace("local", model.name.clone());
    let input = JudgeInput {
        settings: trace.settings.clone(),
        rendered: trace.rendered.clone(),
    };
    let outcome = match runtime.worker.evaluate(&model.id, vec![input]).await {
        Ok(mut outcomes) => outcomes.pop(),
        Err(reason) => {
            return Err(JudgeFailure {
                reason,
                trace: Some(trace),
            });
        }
    };
    match outcome {
        Some(JudgeOutcome::Evaluated {
            label,
            logits,
            input_tokens,
        }) => Ok(Some(JudgeVerdict {
            label,
            trace: JudgeTrace {
                logits: Some(logits),
                input_tokens: Some(input_tokens),
                ..trace
            },
        })),
        Some(JudgeOutcome::InputTokenLimit { input_tokens }) => Err(JudgeFailure {
            reason: format!("The call is too long for the judge model ({input_tokens} tokens)."),
            trace: Some(JudgeTrace {
                input_tokens: Some(input_tokens),
                ..trace
            }),
        }),
        None => Err(JudgeFailure {
            reason: String::from("The local judge returned no result."),
            trace: Some(trace),
        }),
    }
}

/// Keeps one finished gate decision in the decision log without blocking the gate; a failure to
/// keep it is only logged.
/// 在不阻塞闸门的情况下把一次已完成的闸门决定保存到判定记录；保存失败只写日志。
pub(crate) fn record_decision(state: &SharedServiceBridgeState, decision: PermissionRecord) {
    let store = match state.lock() {
        // Only the saved switch lets a decision reach the disk.
        // 只有已保存的开关允许决定写入磁盘。
        Ok(guard)
            if guard.agent_runtime.as_ref().is_some_and(|runtime| {
                runtime
                    .provider_configuration
                    .permission_judge
                    .record_decisions
            }) =>
        {
            Arc::clone(&guard.permission_judge.records)
        }
        Ok(_) => return,
        Err(error) => {
            tracing::warn!("failed to lock service state for the decision log: {error}");
            return;
        }
    };
    let record = super::records::record_of(decision);
    tokio::task::spawn_blocking(move || {
        if let Err(error) = store.append(record) {
            tracing::warn!("failed to keep a permission decision: {error}");
        }
    });
}

/// Returns the decision log store.
/// 返回判定记录存储。
fn record_store(
    state: &SharedServiceBridgeState,
) -> Result<Arc<super::records::RecordStore>, String> {
    let (runtime, _) = read_state(state)?;
    Ok(Arc::clone(&runtime.records))
}

/// Reads one page of the decision log, newest first.
/// 读取判定记录的一页，最新在前。
pub(crate) async fn service_permission_judge_records(
    state: SharedServiceBridgeState,
    request: PermissionJudgeRecordsRequest,
) -> Result<PermissionJudgeRecordPage, String> {
    let store = record_store(&state)?;
    blocking(move || store.page(&request)).await?
}

/// Marks the label the judge should have given a record, or clears the mark.
/// 标记判定器本应给出的记录标签，或清除标记。
pub(crate) async fn service_mark_permission_judge_record(
    state: SharedServiceBridgeState,
    request: PermissionJudgeRecordMarkRequest,
) -> Result<PermissionJudgeRecord, String> {
    let store = record_store(&state)?;
    blocking(move || store.mark(&request.id, request.correction)).await?
}

/// Exports marked records as training samples.
/// 将已标记的记录导出为训练样本。
pub(crate) async fn service_export_permission_judge_records(
    state: SharedServiceBridgeState,
) -> Result<PermissionJudgeRecordExport, String> {
    let store = record_store(&state)?;
    blocking(move || store.export()).await?
}

/// Removes every record of the decision log.
/// 删除判定记录中的全部记录。
pub(crate) async fn service_clear_permission_judge_records(
    state: SharedServiceBridgeState,
) -> Result<(), String> {
    let store = record_store(&state)?;
    blocking(move || store.clear()).await?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Policy with one writes option and one reset option.
    /// 含一个写入选项与一个重置选项的策略。
    fn policy() -> JudgePolicy {
        serde_json::from_str(
            r#"{
                "version": "test-v1",
                "core_instructions": "Core.",
                "criteria": {"allow": "A.", "review": "R.", "block": "B."},
                "options": {
                    "writes": {"default": "allow", "modes": {"allow": "W.", "review": "W."}},
                    "reset": {"default": "confirm", "modes": {"confirm": "R.", "allow": "R."}}
                }
            }"#,
        )
        .unwrap()
    }

    fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(key, mode)| (key.to_string(), mode.to_string()))
            .collect()
    }

    #[test]
    fn saving_keeps_retired_choices_and_drops_omitted_ones() {
        let saved = map(&[("writes", "review"), ("retired", "on")]);
        let merged = merge_project_options(&policy(), saved, map(&[("reset", "allow")])).unwrap();
        assert_eq!(merged, map(&[("reset", "allow"), ("retired", "on")]));
    }

    #[test]
    fn saving_refuses_unknown_options_and_modes() {
        assert!(merge_project_options(&policy(), BTreeMap::new(), map(&[("other", "x")])).is_err());
        assert!(
            merge_project_options(&policy(), BTreeMap::new(), map(&[("writes", "x")])).is_err()
        );
    }

    #[test]
    fn snapshot_marks_retired_choices_and_unknown_saved_modes() {
        let snapshot = project_policy_snapshot(
            String::from("p"),
            &policy(),
            &map(&[("writes", "later"), ("retired", "on")]),
        );
        let keys: Vec<&str> = snapshot
            .options
            .iter()
            .map(|option| option.option.key.as_str())
            .collect();
        assert_eq!(keys, ["writes", "reset"]);
        assert_eq!(snapshot.options[0].chosen_mode.as_deref(), Some("later"));
        assert_eq!(snapshot.unsupported_options, map(&[("retired", "on")]));
        assert_eq!(
            snapshot.issue.map(|issue| issue.code).as_deref(),
            Some("unknown_mode")
        );
    }
}
