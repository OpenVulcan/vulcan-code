//! Permission judge settings, model registry, local worker status and CUDA runtime commands.
//! 权限判定设置、模型登记表、本机 worker 状态与 CUDA 执行库命令。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vulcan_permission_judge::JudgeLabel;
pub use vulcan_permission_judge::ModelFormat;
pub use vulcan_permission_judge::protocol::JudgeDevice;
pub use vulcan_permission_judge::system::{JudgeBackend, ReviewRules};

/// Everything the "Permission judge" settings page shows.
/// “权限判定”设置页展示的全部内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeSettingsSnapshot {
    /// Configured backend; absent until one is set up, which keeps auto-approve unavailable.
    /// 已配置的后端；设置完成前缺省，此时自动审批不可用。
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
    /// Whether decisions are kept in the local decision log.
    /// 是否在本机判定记录中保存决定。
    pub record_decisions: bool,
    /// Seconds a permission approval waits before the call is denied; zero waits for the user.
    /// 权限审批在调用被拒绝前等待的秒数；为零时一直等待用户。
    pub approval_timeout_seconds: u32,
    /// Longest accepted approval wait, in seconds.
    /// 允许的最长审批等待时间（秒）。
    pub max_approval_timeout_seconds: u32,
    /// Registered local model packages.
    /// 已登记的本机模型包。
    pub models: Vec<PermissionJudgeModelSummary>,
    /// Registration in use.
    /// 使用中的登记。
    pub active_model_id: Option<String>,
    /// Registration a rollback returns to.
    /// 回退将返回的登记。
    pub previous_model_id: Option<String>,
    /// Official model chosen for the direct backend, by its edge model id.
    /// 为官方直连后端选择的官方模型，以其边缘模型 id 表示。
    pub edge_model_id: Option<String>,
    /// Endpoint the user locked the direct backend to; `None` selects automatically.
    /// 用户为官方直连后端锁定的端点；`None` 表示自动选择。
    pub edge_endpoint_id: Option<String>,
    /// Official packages this build recognizes, with their published formats.
    /// 本构建识别的官方模型包及其已发布格式。
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
    /// Published formats of this family.
    /// 该家族已发布的格式。
    pub variants: Vec<PermissionJudgeKnownVariant>,
}

/// Official direct models the signed-in account's deployment currently serves.
/// 已登录账户所属部署当前提供的官方直连模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeEdgeModels {
    /// Enabled models sorted by id; empty when none is enabled.
    /// 按 id 排序的已启用模型；没有启用任何模型时为空。
    pub models: Vec<PermissionJudgeEdgeModel>,
}

/// One official direct model.
/// 一个官方直连模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeEdgeModel {
    /// Full edge model id, shown as is.
    /// 完整的边缘模型 id，原样显示。
    pub model_id: String,
}

/// How the direct backend picks its endpoint.
/// 官方直连后端选择端点的方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionJudgeEdgeEndpointMode {
    /// The background prober picks the fastest eligible endpoint.
    /// 后台探测选择最快的可用端点。
    Auto,
    /// Every request goes to the endpoint the user locked.
    /// 每个请求都发往用户锁定的端点。
    Locked,
}

/// Endpoints of the chosen official direct model and how requests are routed among them. Probe
/// results live only in memory and start empty after a restart.
/// 所选官方直连模型的端点，以及请求在其中的走向。探测结果只保存在内存中，重启后为空。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeEdgeEndpoints {
    /// Edge model the endpoints serve; `None` when none is chosen.
    /// 端点服务的边缘模型；未选择时为 `None`。
    pub model_id: Option<String>,
    /// Deployment id sent as the site marker; `None` until access was obtained.
    /// 作为来源标记发送的部署 id；取得访问之前为 `None`。
    pub site: Option<String>,
    /// Routing mode.
    /// 走向方式。
    pub mode: PermissionJudgeEdgeEndpointMode,
    /// Endpoint the user locked, when locked.
    /// 锁定时用户锁定的端点。
    pub locked_endpoint_id: Option<String>,
    /// Endpoint requests go to now; `None` when no endpoint is known or the locked one is not
    /// offered.
    /// 请求当前发往的端点；尚无已知端点或锁定的端点未提供时为 `None`。
    pub selected_endpoint_id: Option<String>,
    /// When the last probe round finished, Unix milliseconds.
    /// 上一轮探测完成的时间（Unix 毫秒）。
    pub probed_at_ms: Option<u64>,
    /// When the next scheduled probe round starts, Unix milliseconds.
    /// 下一轮计划探测开始的时间（Unix 毫秒）。
    pub next_probe_at_ms: Option<u64>,
    /// Why the endpoint list could not be obtained, when it could not.
    /// 无法取得端点列表时的原因。
    pub error: Option<String>,
    /// Endpoints in the deployment's priority order.
    /// 按部署优先顺序排列的端点。
    pub endpoints: Vec<PermissionJudgeEdgeEndpoint>,
}

/// One endpoint with its latest probe.
/// 一个端点及其最近一次探测。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeEdgeEndpoint {
    /// Endpoint id.
    /// 端点 id。
    pub endpoint_id: String,
    /// Endpoint base URL.
    /// 端点基础地址。
    pub endpoint: String,
    /// Operator's default order; lower comes first.
    /// 运营方的默认顺序；数值小者在前。
    pub priority: u32,
    /// Whether the deployment heard from the endpoint recently; advisory only.
    /// 部署最近是否收到该端点的上报；仅供参考。
    pub online: bool,
    /// Region the endpoint reported; `None` until probed successfully.
    /// 端点报告的地区；成功探测之前为 `None`。
    pub region: Option<String>,
    /// Whether the endpoint reported it can judge.
    /// 端点是否报告其可以判定。
    pub ready: Option<bool>,
    /// Whether the endpoint accepts this account's site.
    /// 端点是否接受该账户的来源标记。
    pub accepts_site: Option<bool>,
    /// Median network round trip of the probe.
    /// 探测的网络往返中位数。
    pub rtt_ms: Option<u64>,
    /// Median compute time the endpoint reported; `None` when it reported none.
    /// 端点报告的计算耗时中位数；未报告时为 `None`。
    pub compute_p50_ms: Option<u64>,
    /// Estimated latency used for automatic selection; `None` when the endpoint is not eligible.
    /// 自动选择使用的预计延迟；端点不可用时为 `None`。
    pub score_ms: Option<u64>,
    /// Why the probe failed, when it failed.
    /// 探测失败时的原因。
    pub error: Option<String>,
}

/// Locks the direct backend to one endpoint, or returns it to automatic selection.
/// 将官方直连后端锁定到一个端点，或恢复自动选择。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeEdgeEndpointLockRequest {
    /// Endpoint to lock to; `None` selects automatically.
    /// 要锁定的端点；`None` 表示自动选择。
    pub endpoint_id: Option<String>,
}

/// Sends a few real judge requests to one endpoint of the chosen official direct model and reports
/// each one; endpoint selection and the lock are not changed.
/// 向所选官方直连模型的一个端点发送若干真实判定请求并逐个报告；不改变端点选择与锁定。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeEdgeRequestTestRequest {
    /// Endpoint to test; it must be offered for the chosen model.
    /// 要测试的端点；必须在所选模型的提供之列。
    pub endpoint_id: String,
    /// Call the requests carry.
    /// 请求携带的调用。
    pub input: PermissionJudgeEdgeTestInput,
}

/// Call a request test sends, rendered as a `shell_command` call.
/// 请求测试发送的调用，渲染为 `shell_command` 调用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PermissionJudgeEdgeTestInput {
    /// A sample command the service defines.
    /// 服务定义的示例命令。
    Preset {
        /// Which sample.
        /// 哪个示例。
        preset: PermissionJudgeEdgeTestPreset,
    },
    /// A command the user typed.
    /// 用户输入的命令。
    Custom {
        /// Command text; it must not be blank.
        /// 命令文本；不得为空白。
        command: String,
    },
}

/// Sample commands of the request test; the interface labels them by value.
/// 请求测试的示例命令；界面按取值显示标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionJudgeEdgeTestPreset {
    /// Ordinary work: `git status`.
    /// 普通工作：`git status`。
    Ordinary,
    /// Deleting the home directory.
    /// 删除主目录。
    Destructive,
    /// Dropping a database schema.
    /// 删除数据库模式。
    DatabaseReset,
    /// An input longer than any model's token limit, answered without running the model.
    /// 超过任何模型 token 上限的输入，不运行模型即作答。
    Oversized,
}

/// Result of a request test; kept only in the page that asked for it.
/// 请求测试结果；只保留在发起请求的页面中。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeEdgeRequestTest {
    /// Edge model the requests named.
    /// 请求所用的边缘模型。
    pub model_id: String,
    /// Tested endpoint id.
    /// 被测试的端点 id。
    pub endpoint_id: String,
    /// Tested endpoint base URL.
    /// 被测试端点的基础地址。
    pub endpoint: String,
    /// Site marker the requests carried.
    /// 请求携带的来源标记。
    pub site: String,
    /// Characters of the rendered call each request carried.
    /// 每个请求携带的渲染调用字符数。
    pub rendered_chars: u64,
    /// Requests in the order sent; the run stops after one that could not reach the endpoint.
    /// 按发送顺序排列的请求；某个请求无法到达端点后停止。
    pub attempts: Vec<PermissionJudgeEdgeRequestAttempt>,
    /// Requests answered with judgements that passed the client's checks.
    /// 得到并通过客户端检查的判定回答的请求数。
    pub succeeded: u32,
    /// Fastest total time of the successful requests.
    /// 成功请求的最短总耗时。
    pub min_ms: Option<f64>,
    /// Median total time of the successful requests.
    /// 成功请求的总耗时中位数。
    pub median_ms: Option<f64>,
    /// Slowest total time of the successful requests.
    /// 成功请求的最长总耗时。
    pub max_ms: Option<f64>,
}

/// One request of a request test. Stage times come from the endpoint's `Server-Timing` header and
/// are `None` when it did not report them.
/// 请求测试中的一个请求。各阶段耗时来自端点的 `Server-Timing` 头，未报告时为 `None`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeEdgeRequestAttempt {
    /// Request identifier sent and expected back.
    /// 发送并期望回显的请求标识。
    pub request_id: String,
    /// Client time from sending to the whole answer being read.
    /// 客户端从发送到读完整个回答的耗时。
    pub total_ms: f64,
    /// Total time minus the time inside the endpoint.
    /// 总耗时减去端点内部耗时。
    pub network_ms: Option<f64>,
    /// Time inside the endpoint.
    /// 端点内部耗时。
    pub endpoint_ms: Option<f64>,
    /// Round trip between the endpoint and its GPU node.
    /// 端点与其 GPU 节点之间的往返耗时。
    pub node_ms: Option<f64>,
    /// Time the inputs waited in the node's queue.
    /// 输入在节点队列中的等待耗时。
    pub queue_ms: Option<f64>,
    /// Model compute time on the node.
    /// 节点上的模型计算耗时。
    pub compute_ms: Option<f64>,
    /// HTTP status of the answer; `None` when the endpoint could not be reached.
    /// 回答的 HTTP 状态；无法到达端点时为 `None`。
    pub http_status: Option<u16>,
    /// The endpoint rejected the cached token, so this request was sent again with a renewed one.
    /// 端点拒绝了缓存的令牌，因此该请求以续期后的令牌重新发送。
    pub token_renewed: bool,
    /// The answer's content, when the endpoint judged.
    /// 端点作出判定时的回答内容。
    pub answer: Option<PermissionJudgeEdgeTestAnswer>,
    /// Error code from the endpoint's error body.
    /// 端点错误正文中的错误代码。
    pub error_code: Option<String>,
    /// Why the request failed, as a real judgement would record it; English.
    /// 请求失败的原因，与真实判定记录的相同；英文。
    pub error: Option<String>,
}

/// What an endpoint answered.
/// 端点的回答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeEdgeTestAnswer {
    /// Region of the endpoint that answered.
    /// 作答端点所在的区域。
    pub region: String,
    /// Model id the endpoint serves.
    /// 端点提供的模型 id。
    pub model: String,
    /// Digest of the checkpoint the node ran.
    /// 节点运行的检查点摘要。
    pub checkpoint_sha256: String,
    /// Policy revision of the deployed package.
    /// 已部署模型包的策略修订。
    pub policy_version: String,
    /// One result per input.
    /// 每条输入一条结果。
    pub results: Vec<PermissionJudgeEdgeTestResult>,
}

/// One result of an answer.
/// 回答中的一条结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeEdgeTestResult {
    /// Decision after the package's margin rule.
    /// 应用模型包余量规则后的判定。
    pub label: JudgeLabel,
    /// Raw scores in label order; present when the model ran.
    /// 按标签顺序排列的原始分数；模型运行时存在。
    pub logits: Option<[f32; 3]>,
    /// Sequence length the model read; present when the model ran.
    /// 模型读取的序列长度；模型运行时存在。
    pub input_tokens: Option<u32>,
    /// Whether the model scored the input.
    /// 模型是否为该输入评分。
    pub model_evaluated: bool,
    /// Why the model did not run, when it did not.
    /// 模型未运行时的原因。
    pub reason_code: Option<String>,
}

/// One published format of an official package family.
/// 官方模型包家族的一个已发布格式。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeKnownVariant {
    /// Weight format.
    /// 权重格式。
    pub format: ModelFormat,
    /// Devices this format runs on.
    /// 该格式可运行的设备。
    pub devices: Vec<JudgeDevice>,
    /// Download size; absent while no download address is published for this format.
    /// 下载大小；该格式尚未发布下载地址时缺省。
    pub download_bytes: Option<u64>,
    /// Whether this is the format recommended for the effective device and GPU.
    /// 是否为当前生效设备与显卡推荐的格式。
    pub recommended: bool,
    /// Registration of this format already on this machine, when present.
    /// 本机已有的该格式登记（如有）。
    pub installed_model_id: Option<String>,
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
    /// Lowest CUDA compute capability of those GPUs, such as `8.9`; absent when not reported.
    /// 这些显卡中最低的 CUDA 计算能力，例如 `8.9`；未报告时缺省。
    pub compute_capability: Option<String>,
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
    /// Latest official model download, while running or until the next one starts.
    /// 最近一次官方模型下载，运行中或直到下次开始前保留。
    pub model_download: Option<PermissionJudgeModelDownload>,
    /// How often clients re-read this status while the settings page is open, in milliseconds.
    /// 设置页打开期间客户端重新读取本状态的间隔（毫秒）。
    pub poll_interval_ms: u64,
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
    /// Longest call in tokens the loaded package evaluates.
    /// 已加载模型包评估的最长调用词元数。
    pub max_tokens: Option<u32>,
    /// Most calls the service merges into one evaluation request; more are split.
    /// 服务合并进一个评估请求的最多调用数；更多的会拆分。
    pub max_request_items: Option<u32>,
    /// Most rows per graph run.
    /// 每次计算图运行的最多行数。
    pub micro_batch: Option<u32>,
    /// Most padded tokens (rows times the longest row) per graph run.
    /// 每次计算图运行的最多填充词元数（行数乘以最长行）。
    pub padded_batch_tokens: Option<u32>,
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

/// Progress of one official model download.
/// 一次官方模型下载的进度。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeModelDownload {
    /// Official package family.
    /// 官方模型包家族。
    pub package_id: String,
    /// Weight format being downloaded.
    /// 正在下载的权重格式。
    pub format: ModelFormat,
    /// Whether the download is still running.
    /// 下载是否仍在运行。
    pub running: bool,
    /// Stable phase key of the latest progress event.
    /// 最近进度事件的稳定阶段键。
    pub phase: Option<String>,
    /// Bytes downloaded and verified so far.
    /// 目前已下载并验证的字节数。
    pub completed_bytes: u64,
    /// Total size of the package.
    /// 模型包总大小。
    pub total_bytes: u64,
    /// Registration created after success; it is not put in use automatically.
    /// 成功后创建的登记；不会自动投入使用。
    pub model_id: Option<String>,
    /// Failure, when the download failed.
    /// 下载失败时的原因。
    pub error: Option<String>,
}

/// Saves the system-level choices; the registry has its own commands.
/// 保存系统级选择；登记表有独立命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeSettingsSaveRequest {
    /// Backend to configure; absent leaves the judge unconfigured. A backend is saved only once it
    /// is ready: a local model in use, a chosen cloud decision model, or a signed-in OpenVulcan
    /// account with an official direct model.
    /// 要配置的后端；缺省表示不配置判定器。后端只有在就绪后才能保存：本机有使用中的模型、云端已
    /// 选择判定模型，或已登录 OpenVulcan 账户并选择了官方直连模型。
    #[serde(default)]
    pub backend: Option<JudgeBackend>,
    /// Official model for the direct backend, by its edge model id; required with that backend.
    /// 官方直连后端使用的官方模型，以其边缘模型 id 表示；选择该后端时必填。
    #[serde(default)]
    pub edge_model_id: Option<String>,
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
    /// Whether decisions are kept in the local decision log; required so that no save turns it
    /// off by omission.
    /// 是否在本机判定记录中保存决定；必填，避免任何保存因遗漏而将其关闭。
    pub record_decisions: bool,
    /// Seconds a permission approval waits before the call is denied; zero waits for the user.
    /// Required so that no save clears it by omission.
    /// 权限审批在调用被拒绝前等待的秒数；为零时一直等待用户。必填，避免任何保存因遗漏而将其清除。
    pub approval_timeout_seconds: u32,
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

/// Checks a package directory without registering it.
/// 检查模型包目录但不登记。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgePackageInspectRequest {
    /// Absolute package directory.
    /// 绝对模型包目录。
    pub path: String,
}

/// Outcome of checking a package directory; exactly one of `issue` and `package` is present.
/// 检查模型包目录的结果；`issue` 与 `package` 恰好存在一个。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgePackageInspection {
    /// Why the directory is not a usable package.
    /// 目录不是可用模型包的原因。
    pub issue: Option<PermissionJudgeIssue>,
    /// Facts of the fully verified package.
    /// 完整验证后的模型包信息。
    pub package: Option<PermissionJudgePackageFacts>,
}

/// Facts of a verified package directory.
/// 已验证模型包目录的信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgePackageFacts {
    /// Registration identity the package would get, the manifest digest.
    /// 模型包将获得的登记身份，即清单摘要。
    pub model_id: String,
    /// Official package identifier; absent for a custom package, which needs explicit trust.
    /// 官方模型包标识；自定义模型包缺省，需要显式信任。
    pub known_package_id: Option<String>,
    /// Name used when the user leaves the name blank.
    /// 用户未填名称时使用的名称。
    pub default_name: String,
    /// Policy revision identifier.
    /// 策略修订标识。
    pub policy_version: String,
    /// Weight precision description.
    /// 权重精度说明。
    pub precision: String,
    /// Total size of all listed files.
    /// 所有列出文件的总大小。
    pub total_bytes: u64,
    /// Whether this exact package is already registered.
    /// 这个准确模型包是否已经登记。
    pub registered: bool,
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
    /// Device the session runs on; absent for the cloud and official direct judges.
    /// 会话运行的设备；云端与官方直连判定时缺省。
    pub device: Option<JudgeDevice>,
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

/// Starts downloading one published format of an official package and registering it.
/// 开始下载官方模型包的一个已发布格式并登记。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeModelDownloadRequest {
    /// Official package family.
    /// 官方模型包家族。
    pub package_id: String,
    /// Weight format to download.
    /// 要下载的权重格式。
    pub format: ModelFormat,
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

/// One decision the judge or the reviewer took part in, as the decision log keeps it.
/// 判定记录所保存的一次判定器或复核器参与的决定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeRecord {
    /// Record identity.
    /// 记录标识。
    pub id: String,
    /// When the call reached the judge layer, in Unix milliseconds.
    /// 调用到达判定层的时间（Unix 毫秒）。
    pub judged_at_ms: u64,
    /// Session that made the call.
    /// 发起调用的会话。
    pub session_id: String,
    /// Workspace the call runs in.
    /// 调用运行的工作区。
    pub workspace_root: Option<String>,
    /// Model-visible tool name.
    /// 模型可见的工具名称。
    pub tool_name: String,
    /// Argument text exactly as the agent model sent it.
    /// 代理模型发送的原样参数文本。
    pub arguments: String,
    /// Matched high-risk category key, when any.
    /// 命中的高风险类别键（如有）。
    pub high_risk: Option<String>,
    /// Judge layer result, when a judge is configured.
    /// 判定层结果（配置判定器时）。
    pub judge: Option<PermissionJudgeRecordJudge>,
    /// Review layer result, when a reviewer is configured.
    /// 复核层结果（配置复核器时）。
    pub review: Option<PermissionJudgeRecordReview>,
    /// Which layer decided and how, such as `user_denied`.
    /// 由哪一层决定以及如何决定，例如 `user_denied`。
    pub outcome: String,
    /// Label the user says the judge should have given.
    /// 用户认为判定器应给出的标签。
    pub correction: Option<JudgeLabel>,
}

/// Judge layer result of one recorded call.
/// 一次已记录调用的判定层结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeRecordJudge {
    /// Decision; absent when the judge could not score the call.
    /// 判定结果；判定器无法评分时缺省。
    pub label: Option<JudgeLabel>,
    /// Why the judge could not score the call.
    /// 判定器无法评分的原因。
    pub error: Option<String>,
    /// Time the judge layer took.
    /// 判定层耗时。
    pub elapsed_ms: f64,
    /// What the judge read and produced, when it got that far.
    /// 判定器读取与产生的内容（若已进行到该步）。
    pub trace: Option<PermissionJudgeRecordTrace>,
}

/// What one judgement read and produced.
/// 一次判定读取与产生的内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeRecordTrace {
    /// Judge backend key, `local`, `cloud` or `edge`.
    /// 判定后端键，`local`、`cloud` 或 `edge`。
    pub backend: String,
    /// Local model name, cloud model id or official direct model id.
    /// 本机模型名称、云端模型标识或官方直连模型标识。
    pub model: String,
    /// Policy version of the model.
    /// 模型的策略版本。
    pub policy_version: String,
    /// Category choices the judge read.
    /// 判定器读取的类别档位。
    pub settings: BTreeMap<String, String>,
    /// Call text the judge read.
    /// 判定器读取的调用文本。
    pub rendered: String,
    /// Raw local scores in label order.
    /// 按标签顺序排列的本机原始分数。
    pub logits: Option<[f32; 3]>,
    /// Cloud probability of the allow label.
    /// 云端 allow 标签的概率。
    pub allow_probability: Option<f32>,
    /// Sequence length the local model read or refused.
    /// 本机模型读取或拒绝的序列长度。
    pub input_tokens: Option<u32>,
    /// Checkpoint digest the official direct endpoint reported; absent for other backends.
    /// 官方直连端点报告的检查点摘要；其他后端缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_sha256: Option<String>,
}

/// Review layer result of one recorded call.
/// 一次已记录调用的复核层结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeRecordReview {
    /// Reviewer verdict key; absent when the reviewer could not review the call.
    /// 复核决定键；复核器无法复核时缺省。
    pub verdict: Option<String>,
    /// Reviewer reason.
    /// 复核理由。
    pub reason: Option<String>,
    /// Why the reviewer could not review the call.
    /// 复核器无法复核的原因。
    pub error: Option<String>,
    /// Time the review layer took.
    /// 复核层耗时。
    pub elapsed_ms: f64,
}

/// Reads one page of the decision log, newest first.
/// 读取判定记录的一页，最新在前。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeRecordsRequest {
    /// Judge result to keep: `allow`, `review`, `block` or `failed`.
    /// 保留的判定结果：`allow`、`review`、`block` 或 `failed`。
    pub label: Option<String>,
    /// Workspace to keep.
    /// 保留的工作区。
    pub workspace: Option<String>,
    /// Keep only records the user marked.
    /// 只保留用户已标记的记录。
    pub marked_only: bool,
    /// Return records older than this record.
    /// 返回早于该记录的记录。
    pub before: Option<String>,
    /// Records per page.
    /// 每页记录数。
    pub limit: u32,
}

/// One page of the decision log with the facts the log page shows.
/// 判定记录的一页，以及记录页显示的信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeRecordPage {
    /// Matching records, newest first.
    /// 匹配的记录，最新在前。
    pub records: Vec<PermissionJudgeRecord>,
    /// Whether older matching records remain.
    /// 是否还有更早的匹配记录。
    pub has_more: bool,
    /// Records kept in the log.
    /// 记录中保存的总数。
    pub total: u32,
    /// Records the user marked.
    /// 用户已标记的记录数。
    pub marked: u32,
    /// Workspaces of every kept record, for the filter.
    /// 所有已保存记录的工作区，供筛选使用。
    pub workspaces: Vec<String>,
    /// Most records the log keeps.
    /// 记录保存的最大条数。
    pub retention_records: u32,
    /// Most days the log keeps a record.
    /// 记录保存的最长天数。
    pub retention_days: u32,
}

/// Marks the label the judge should have given a record, or clears the mark.
/// 标记判定器本应给出的标签，或清除标记。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionJudgeRecordMarkRequest {
    /// Record identity.
    /// 记录标识。
    pub id: String,
    /// Expected label; absent clears the mark.
    /// 期望标签；缺省时清除标记。
    pub correction: Option<JudgeLabel>,
}

/// Marked records as training samples in JSON Lines.
/// 以 JSON Lines 表示、作为训练样本的已标记记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionJudgeRecordExport {
    /// Suggested file name.
    /// 建议的文件名。
    pub file_name: String,
    /// One sample per line.
    /// 每行一个样本。
    pub content: String,
    /// Samples exported.
    /// 导出的样本数。
    pub count: u32,
}
