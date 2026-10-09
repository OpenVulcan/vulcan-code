//! Permission judge settings, model registry, local worker status and CUDA runtime commands.
//! 权限判定设置、模型登记表、本机 worker 状态与 CUDA 执行库命令。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
pub use vulcan_permission_judge::protocol::JudgeDevice;
pub use vulcan_permission_judge::system::{JudgeBackend, JudgeState};

/// Everything the "Permission judge" settings page shows.
/// “权限判定”设置页展示的全部内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeSettingsSnapshot {
    /// Off or enabled; absent until the user decides.
    /// 关闭或启用；用户决定前缺省。
    pub state: Option<JudgeState>,
    /// Backend used while enabled.
    /// 启用时使用的后端。
    pub backend: Option<JudgeBackend>,
    /// Device chosen by the user; absent means the recommended device.
    /// 用户选择的设备；缺省表示推荐设备。
    pub device: Option<JudgeDevice>,
    /// Device used when the user has not chosen one; absent when no device is usable.
    /// 用户未选择时使用的设备；没有可用设备时缺省。
    pub recommended_device: Option<JudgeDevice>,
    /// Availability of every device on this machine, in preference order.
    /// 本机每个设备的可用性，按优先顺序排列。
    pub devices: Vec<PermissionJudgeDeviceSummary>,
    /// Whether the review LLM may allow calls the judge blocked.
    /// 复核 LLM 是否可以放行判定器拦截的调用。
    pub llm_may_allow_blocked: bool,
    /// Registered local model packages.
    /// 已登记的本机模型包。
    pub models: Vec<PermissionJudgeModelSummary>,
    /// Registration in use.
    /// 使用中的登记。
    pub active_model_id: Option<String>,
    /// Registration a rollback returns to.
    /// 回退将返回的登记。
    pub previous_model_id: Option<String>,
    /// Official packages this build recognizes, without download locations.
    /// 本构建识别的官方模型包，不含下载位置。
    pub known_packages: Vec<PermissionJudgeKnownPackage>,
    /// Policy options of the model in use; empty when no verified model is in use.
    /// 使用中模型的策略选项；没有已验证的使用中模型时为空。
    pub policy_options: Vec<PermissionJudgePolicyOption>,
    /// Why the model in use cannot be opened, when it cannot.
    /// 使用中模型无法打开时的原因。
    pub active_model_issue: Option<PermissionJudgeIssue>,
    /// CUDA runtime state of this machine.
    /// 本机的 CUDA 执行库状态。
    pub cuda: PermissionJudgeCudaSnapshot,
    /// Live status of the local worker and any CUDA installation.
    /// 本机 worker 与任何 CUDA 安装的实时状态。
    pub status: PermissionJudgeStatus,
}

/// Whether one device can run the local model.
/// 某个设备能否运行本机模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeDeviceSummary {
    /// Device.
    /// 设备。
    pub device: JudgeDevice,
    /// Stable reason code when the device cannot be used, such as `driver_too_old`.
    /// 设备不可用时的稳定原因代码，例如 `driver_too_old`。
    pub unavailable_reason: Option<String>,
}

/// A failure with a stable code for localization and an English detail.
/// 带可本地化稳定代码与英文细节的失败。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeIssue {
    /// Stable code, such as `digest_mismatch`.
    /// 稳定代码，例如 `digest_mismatch`。
    pub code: String,
    /// English detail naming the failing step or file.
    /// 指明失败步骤或文件的英文细节。
    pub detail: String,
}

/// One registered local model package.
/// 一条已登记的本机模型包。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeModelSummary {
    /// Registration identity, the package manifest digest.
    /// 登记身份，即模型包清单摘要。
    pub id: String,
    /// Name given by the user.
    /// 用户给出的名称。
    pub name: String,
    /// Absolute package directory.
    /// 绝对模型包目录。
    pub path: String,
    /// Official package identifier; absent for a custom model.
    /// 官方模型包标识；自定义模型缺省。
    pub known_package_id: Option<String>,
    /// Policy revision the weights were trained with.
    /// 权重训练所用的策略修订。
    pub policy_version: String,
    /// Weight precision.
    /// 权重精度。
    pub precision: String,
    /// Total size of the package files.
    /// 模型包文件总大小。
    pub total_bytes: u64,
    /// Registration time in Unix milliseconds.
    /// 登记时间（Unix 毫秒）。
    pub registered_at_ms: u64,
}

/// One official package family.
/// 一个官方模型包家族。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeKnownPackage {
    /// Stable identifier, also the key of its localized description.
    /// 稳定标识，同时是其本地化说明的键。
    pub id: String,
    /// Product name shown as is.
    /// 原样显示的产品名称。
    pub display_name: String,
}

/// One authorization option of the policy in use.
/// 使用中策略的一个授权选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgePolicyOption {
    /// Option key, also the key of its localized label.
    /// 选项键，同时是其本地化标签的键。
    pub key: String,
    /// Mode used when a project has not chosen one.
    /// 项目未选择时使用的档位。
    pub default_mode: String,
    /// Selectable modes in policy order.
    /// 按策略顺序排列的可选档位。
    pub modes: Vec<String>,
}

/// CUDA runtime state: driver, the two installable parts, library directories and official pages.
/// CUDA 执行库状态：驱动、两个可安装部分、库目录与官方页面。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeCudaSnapshot {
    /// Platform key, such as `windows-x86_64`.
    /// 平台键，例如 `windows-x86_64`。
    pub platform: String,
    /// Whether a CUDA build exists for this platform and the bundled runtime release.
    /// 本平台与随包执行库版本是否存在 CUDA 构建。
    pub supported: bool,
    /// CUDA major version the GPU build links.
    /// GPU 构建链接的 CUDA 主版本。
    pub cuda_major: Option<u32>,
    /// Lowest NVIDIA driver branch that runs it.
    /// 可运行它的最低 NVIDIA 驱动分支。
    pub minimum_driver_major: Option<u32>,
    /// Installed NVIDIA driver, when detected.
    /// 检测到的已安装 NVIDIA 驱动。
    pub driver: Option<PermissionJudgeNvidiaDriver>,
    /// Why the driver could not be read, when it could not.
    /// 无法读取驱动时的原因。
    pub driver_error: Option<String>,
    /// Libraries the CUDA library directory must contain.
    /// CUDA 库目录必须包含的库。
    pub required_libraries: Vec<String>,
    /// Installed GPU build of ONNX Runtime, when present.
    /// 已安装的 ONNX Runtime GPU 构建（如有）。
    pub onnxruntime_directory: Option<String>,
    /// Complete CUDA library directories found on this machine, in preference order.
    /// 本机发现的完整 CUDA 库目录，按优先顺序排列。
    pub detected_directories: Vec<PermissionJudgeCudaDirectory>,
    /// Directory locked by the user's choice of CUDA.
    /// 因用户选择 CUDA 而锁定的目录。
    pub locked_directory: Option<String>,
    /// Required libraries missing from the locked directory.
    /// 锁定目录中缺失的必需库。
    pub locked_directory_missing: Vec<String>,
    /// Official archives the one-click install downloads, also offered for manual download.
    /// 一键安装下载的官方归档，也提供手动下载。
    pub archives: Vec<PermissionJudgeCudaArchive>,
    /// Official pages for manual installation; the interface labels them by `id`.
    /// 手动安装使用的官方页面；界面按 `id` 显示标签。
    pub links: Vec<PermissionJudgeLink>,
}

/// Installed NVIDIA display driver.
/// 已安装的 NVIDIA 显示驱动。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeNvidiaDriver {
    /// Driver version, such as `591.86`.
    /// 驱动版本，例如 `591.86`。
    pub version: String,
    /// Driver branch.
    /// 驱动分支。
    pub major: u32,
    /// GPU names.
    /// 显卡名称。
    pub gpus: Vec<String>,
}

/// One complete CUDA library directory.
/// 一个完整的 CUDA 库目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeCudaDirectory {
    /// `managed`, `toolkit` or `search_path`.
    /// `managed`、`toolkit` 或 `search_path`。
    pub source: String,
    /// Absolute directory.
    /// 绝对目录。
    pub directory: String,
}

/// Installable CUDA runtime part.
/// CUDA 执行库的可安装部分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionJudgeCudaComponent {
    /// GPU build of ONNX Runtime.
    /// ONNX Runtime 的 GPU 构建。
    Onnxruntime,
    /// CUDA libraries.
    /// CUDA 库。
    CudaLibraries,
}

/// One official archive.
/// 一个官方归档。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeCudaArchive {
    /// Part it installs.
    /// 它安装的部分。
    pub component: PermissionJudgeCudaComponent,
    /// Upstream file name.
    /// 上游文件名。
    pub name: String,
    /// Official HTTPS address.
    /// 官方 HTTPS 地址。
    pub url: String,
    /// Size in bytes.
    /// 字节数。
    pub size: u64,
}

/// One official page.
/// 一个官方页面。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeLink {
    /// Stable identifier the interface maps to a label.
    /// 界面映射为标签的稳定标识。
    pub id: String,
    /// Official HTTPS address.
    /// 官方 HTTPS 地址。
    pub url: String,
}

/// Live status polled while the settings page is open.
/// 设置页打开期间轮询的实时状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeStatus {
    /// Local worker.
    /// 本机 worker。
    pub worker: PermissionJudgeWorkerStatus,
    /// Latest CUDA installation, while running or until the next one starts.
    /// 最近一次 CUDA 安装，运行中或直到下次开始前保留。
    pub cuda_install: Option<PermissionJudgeCudaInstall>,
}

/// Local worker lifecycle phase.
/// 本机 worker 生命周期阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionJudgeWorkerPhase {
    /// No model is loaded.
    /// 没有加载模型。
    NotLoaded,
    /// The worker is verifying, loading and self-checking a model.
    /// worker 正在验证、加载并自检模型。
    Warming,
    /// The model passed its self-check and judges calls.
    /// 模型已通过自检并判定调用。
    Ready,
    /// The last load failed; calls go to the next layer.
    /// 上次加载失败；调用交给下一层。
    Failed,
}

/// Local worker status; metrics the worker cannot read stay absent rather than zero.
/// 本机 worker 状态；worker 无法读取的指标保持缺省而非填 0。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeWorkerStatus {
    /// Lifecycle phase.
    /// 生命周期阶段。
    pub phase: PermissionJudgeWorkerPhase,
    /// Registration being loaded or loaded.
    /// 正在加载或已加载的登记。
    pub model_id: Option<String>,
    /// Device being loaded or in use.
    /// 正在加载或使用中的设备。
    pub device: Option<JudgeDevice>,
    /// Failure of the last load.
    /// 上次加载的失败原因。
    pub error: Option<String>,
    /// Worker process id.
    /// worker 进程标识。
    pub process_id: Option<u32>,
    /// Package verification time.
    /// 模型包验证耗时。
    pub verify_ms: Option<f64>,
    /// Runtime and session creation time.
    /// 执行库与会话创建耗时。
    pub load_ms: Option<f64>,
    /// Self-check time, which also warms the session.
    /// 自检耗时，同时预热会话。
    pub warmup_ms: Option<f64>,
    /// Physical memory of the worker process.
    /// worker 进程的物理内存。
    pub working_set_bytes: Option<u64>,
    /// Private committed memory of the worker process.
    /// worker 进程的私有提交内存。
    pub private_bytes: Option<u64>,
    /// Dedicated video memory committed by the worker process.
    /// worker 进程提交的专用显存。
    pub gpu_dedicated_bytes: Option<u64>,
    /// Shared system memory the worker process uses for the GPU.
    /// worker 进程为 GPU 使用的共享系统内存。
    pub gpu_shared_bytes: Option<u64>,
    /// Busiest GPU engine type of the worker process since the previous poll, in percent.
    /// 自上次轮询以来 worker 进程最繁忙 GPU 引擎类型的百分比。
    pub gpu_utilization_percent: Option<f64>,
    /// Evaluations since load.
    /// 加载以来的评估次数。
    pub evaluations: u64,
    /// Median of recent evaluation latencies.
    /// 最近评估延迟的中位数。
    pub recent_p50_ms: Option<f64>,
    /// 95th percentile of recent evaluation latencies.
    /// 最近评估延迟的第 95 百分位。
    pub recent_p95_ms: Option<f64>,
}

/// Progress of one CUDA installation.
/// 一次 CUDA 安装的进度。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeCudaInstall {
    /// Part being installed.
    /// 正在安装的部分。
    pub component: PermissionJudgeCudaComponent,
    /// Whether the installation is still running.
    /// 安装是否仍在运行。
    pub running: bool,
    /// Stable phase key of the latest progress event.
    /// 最近进度事件的稳定阶段键。
    pub phase: Option<String>,
    /// Bytes downloaded of the current archive.
    /// 当前归档已下载字节数。
    pub completed_bytes: Option<u64>,
    /// Size of the current archive.
    /// 当前归档大小。
    pub total_bytes: Option<u64>,
    /// Installed directory after success.
    /// 成功后的安装目录。
    pub directory: Option<String>,
    /// Failure, when the installation failed.
    /// 安装失败时的原因。
    pub error: Option<String>,
}

/// Saves the system-level choices; the registry has its own commands.
/// 保存系统级选择；登记表有独立命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeSettingsSaveRequest {
    /// Off or enabled.
    /// 关闭或启用。
    pub state: JudgeState,
    /// Backend; required when enabled.
    /// 后端；启用时必填。
    #[serde(default)]
    pub backend: Option<JudgeBackend>,
    /// Chosen device; absent follows the recommendation.
    /// 所选设备；缺省时跟随推荐。
    #[serde(default)]
    pub device: Option<JudgeDevice>,
    /// CUDA library directory to lock; with CUDA chosen and none given, the first detected
    /// complete directory is locked.
    /// 要锁定的 CUDA 库目录；选择 CUDA 且未给出时锁定第一个检测到的完整目录。
    #[serde(default)]
    pub cuda_library_directory: Option<String>,
    /// Whether the review LLM may allow calls the judge blocked.
    /// 复核 LLM 是否可以放行判定器拦截的调用。
    #[serde(default)]
    pub llm_may_allow_blocked: bool,
}

/// Registers a local model package after full verification.
/// 完整验证后登记一个本机模型包。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeModelRegisterRequest {
    /// Absolute package directory.
    /// 绝对模型包目录。
    pub path: String,
    /// Name to show; blank uses the package's official name or directory name.
    /// 显示名称；为空时使用官方名称或目录名。
    #[serde(default)]
    pub name: String,
    /// Accepts a package that is not an official one.
    /// 接受非官方模型包。
    #[serde(default)]
    pub custom: bool,
}

/// Names one registration.
/// 指定一条登记。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeModelRequest {
    /// Registration identity.
    /// 登记身份。
    pub model_id: String,
}

/// Times the local model on self-check inputs.
/// 用自检输入为本机模型计时。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeBenchmarkRequest {
    /// Rows per timed run: 1 or 10.
    /// 每次计时运行的行数：1 或 10。
    pub batch_size: u32,
}

/// Benchmark result; kept only in the page that asked for it.
/// 基准结果；只保留在发起请求的页面中。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeBenchmarkResult {
    /// Device the session runs on.
    /// 会话运行的设备。
    pub device: JudgeDevice,
    /// Registration that was timed.
    /// 被计时的登记。
    pub model_id: String,
    /// Rows per run.
    /// 每次运行的行数。
    pub batch_size: u32,
    /// Timed runs after warm-up.
    /// 预热后的计时运行次数。
    pub rounds: u32,
    /// Median run time.
    /// 运行时间中位数。
    pub p50_ms: f64,
    /// 95th-percentile run time.
    /// 运行时间第 95 百分位。
    pub p95_ms: f64,
    /// Median time per row.
    /// 每行时间中位数。
    pub per_item_ms: f64,
    /// Rows per second at the median.
    /// 按中位数计的每秒行数。
    pub items_per_second: f64,
}

/// Starts installing one CUDA runtime part from the official archives.
/// 开始从官方归档安装一个 CUDA 执行库部分。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeCudaInstallRequest {
    /// Part to install.
    /// 要安装的部分。
    pub component: PermissionJudgeCudaComponent,
}

/// A project's policy choices against the model in use.
/// 项目针对使用中模型的策略选择。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPermissionJudgePolicySnapshot {
    /// Project identity.
    /// 项目标识。
    pub project_id: String,
    /// Options of the model in use with the project's choice.
    /// 使用中模型的选项及项目的选择。
    pub options: Vec<ProjectPermissionJudgePolicyOption>,
    /// Saved choices for options the model in use no longer has; kept but not sent to the judge.
    /// 使用中模型已不存在之选项的已保存选择；保留但不发送给判定器。
    pub unsupported_options: BTreeMap<String, String>,
    /// Why the choices cannot be applied, such as a mode the model does not offer.
    /// 选择无法应用的原因，例如模型未提供的档位。
    pub issue: Option<PermissionJudgeIssue>,
}

/// One option with the project's choice.
/// 一个选项及项目的选择。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPermissionJudgePolicyOption {
    /// Option definition.
    /// 选项定义。
    #[serde(flatten)]
    pub option: PermissionJudgePolicyOption,
    /// Mode the project saved; absent follows the default.
    /// 项目保存的档位；缺省时跟随默认值。
    pub chosen_mode: Option<String>,
}

/// Saves a project's choices for the options of the model in use.
/// 保存项目针对使用中模型选项的选择。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectPermissionJudgePolicySaveRequest {
    /// Project identity.
    /// 项目标识。
    pub project_id: String,
    /// Chosen mode per option; options left out follow the default.
    /// 每个选项的已选档位；未列出的选项跟随默认值。
    pub options: BTreeMap<String, String>,
}
