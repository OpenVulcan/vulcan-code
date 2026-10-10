// Permission judge settings, model registry, worker status and CUDA runtime, mirroring
// `vulcan_service_api::permission_judge`.
// 权限判定设置、模型登记表、worker 状态与 CUDA 执行库，镜像 `vulcan_service_api::permission_judge`。

// JudgeState is off or enabled; null means not configured yet.
// JudgeState 为关闭或启用；null 表示尚未设置。
export type JudgeState = "off" | "enabled";

// JudgeBackend names where calls are judged: the local model, or the saved cloud decision route.
// JudgeBackend 指定判定调用的位置：本机模型，或已保存的云端判定路由。
export type JudgeBackend = "local" | "cloud";

// JudgeDevice is the execution device of the local model.
// JudgeDevice 是本机模型的执行设备。
export type JudgeDevice = "cuda" | "webgpu" | "cpu";

// PermissionJudgeIssue is a failure with a stable code and an English detail.
// PermissionJudgeIssue 是带稳定代码与英文细节的失败。
export interface PermissionJudgeIssue {
  code: string;
  detail: string;
}

// PermissionJudgeDeviceSummary tells whether one device can run the local model.
// PermissionJudgeDeviceSummary 说明某个设备能否运行本机模型。
export interface PermissionJudgeDeviceSummary {
  device: JudgeDevice;
  // Stable reason code when unusable, such as `driver_too_old`.
  // 不可用时的稳定原因代码，例如 `driver_too_old`。
  unavailableReason: string | null;
}

// PermissionJudgePackageFacts describes a fully verified package directory that is not yet registered.
// PermissionJudgePackageFacts 描述一个已完整验证、尚未登记的模型包目录。
export interface PermissionJudgePackageFacts {
  modelId: string;
  knownPackageId: string | null;
  defaultName: string;
  policyVersion: string;
  precision: string;
  totalBytes: number;
  registered: boolean;
}

// PermissionJudgePackageInspection is the outcome of checking a folder; exactly one field is set.
// PermissionJudgePackageInspection 是检查文件夹的结果；恰好一个字段有值。
export interface PermissionJudgePackageInspection {
  issue: PermissionJudgeIssue | null;
  package: PermissionJudgePackageFacts | null;
}

// PermissionJudgeModelSummary is one registered local model package.
// PermissionJudgeModelSummary 是一条已登记的本机模型包。
export interface PermissionJudgeModelSummary {
  id: string;
  name: string;
  path: string;
  // Official package id; null for a custom model.
  // 官方模型包标识；自定义模型为 null。
  knownPackageId: string | null;
  policyVersion: string;
  precision: string;
  totalBytes: number;
  registeredAtMs: number;
}

// PermissionJudgeKnownPackage is one official package family.
// PermissionJudgeKnownPackage 是一个官方模型包家族。
export interface PermissionJudgeKnownPackage {
  id: string;
  displayName: string;
}

// PermissionJudgePolicyOption is one authorization option of the policy in use.
// PermissionJudgePolicyOption 是使用中策略的一个授权选项。
export interface PermissionJudgePolicyOption {
  key: string;
  defaultMode: string;
  modes: string[];
}

// PermissionJudgeCudaComponent is an installable CUDA runtime part.
// PermissionJudgeCudaComponent 是 CUDA 执行库的可安装部分。
export type PermissionJudgeCudaComponent = "onnxruntime" | "cuda_libraries";

// PermissionJudgeCudaSnapshot is the CUDA runtime state of this machine.
// PermissionJudgeCudaSnapshot 是本机的 CUDA 执行库状态。
export interface PermissionJudgeCudaSnapshot {
  platform: string;
  supported: boolean;
  cudaMajor: number | null;
  minimumDriverMajor: number | null;
  driver: { version: string; major: number; gpus: string[] } | null;
  driverError: string | null;
  requiredLibraries: string[];
  onnxruntimeDirectory: string | null;
  detectedDirectories: { source: "managed" | "toolkit" | "search_path"; directory: string }[];
  lockedDirectory: string | null;
  lockedDirectoryMissing: string[];
  archives: { component: PermissionJudgeCudaComponent; name: string; url: string; size: number }[];
  links: { id: string; url: string }[];
}

// PermissionJudgeWorkerPhase is the local worker lifecycle phase.
// PermissionJudgeWorkerPhase 是本机 worker 的生命周期阶段。
export type PermissionJudgeWorkerPhase = "not_loaded" | "warming" | "ready" | "failed";

// PermissionJudgeWorkerStatus is the local worker status; unreadable metrics are null, never zero.
// PermissionJudgeWorkerStatus 是本机 worker 状态；无法读取的指标为 null，而不是 0。
export interface PermissionJudgeWorkerStatus {
  phase: PermissionJudgeWorkerPhase;
  modelId: string | null;
  device: JudgeDevice | null;
  error: string | null;
  processId: number | null;
  verifyMs: number | null;
  loadMs: number | null;
  warmupMs: number | null;
  workingSetBytes: number | null;
  privateBytes: number | null;
  gpuDedicatedBytes: number | null;
  gpuSharedBytes: number | null;
  gpuUtilizationPercent: number | null;
  evaluations: number;
  recentP50Ms: number | null;
  recentP95Ms: number | null;
}

// PermissionJudgeCudaInstall is the progress of one CUDA installation.
// PermissionJudgeCudaInstall 是一次 CUDA 安装的进度。
export interface PermissionJudgeCudaInstall {
  component: PermissionJudgeCudaComponent;
  running: boolean;
  phase: string | null;
  completedBytes: number | null;
  totalBytes: number | null;
  directory: string | null;
  error: string | null;
}

// PermissionJudgeStatus is the live status polled while the settings page is open.
// PermissionJudgeStatus 是设置页打开期间轮询的实时状态。
export interface PermissionJudgeStatus {
  worker: PermissionJudgeWorkerStatus;
  cudaInstall: PermissionJudgeCudaInstall | null;
  // How often this status is re-read while the settings page is open, set by the service.
  // 设置页打开期间重新读取本状态的间隔，由服务设定。
  pollIntervalMs: number;
}

// PermissionJudgeSettingsSnapshot is everything the "Permission judge" settings page shows.
// PermissionJudgeSettingsSnapshot 是“权限判定”设置页展示的全部内容。
export interface PermissionJudgeSettingsSnapshot {
  state: JudgeState | null;
  backend: JudgeBackend | null;
  device: JudgeDevice | null;
  recommendedDevice: JudgeDevice | null;
  devices: PermissionJudgeDeviceSummary[];
  llmMayAllowBlocked: boolean;
  recordDecisions: boolean;
  models: PermissionJudgeModelSummary[];
  activeModelId: string | null;
  previousModelId: string | null;
  knownPackages: PermissionJudgeKnownPackage[];
  policyOptions: PermissionJudgePolicyOption[];
  activeModelIssue: PermissionJudgeIssue | null;
  cuda: PermissionJudgeCudaSnapshot;
  status: PermissionJudgeStatus;
}

// PermissionJudgeSettingsSaveRequest saves the system-level choices.
// PermissionJudgeSettingsSaveRequest 保存系统级选择。
export interface PermissionJudgeSettingsSaveRequest {
  state: JudgeState;
  backend: JudgeBackend | null;
  device: JudgeDevice | null;
  cudaLibraryDirectory: string | null;
  llmMayAllowBlocked: boolean;
  recordDecisions: boolean;
}

// PermissionJudgeBenchmarkResult is one speed test, kept only on the page that ran it.
// PermissionJudgeBenchmarkResult 是一次速度测试，只保留在执行它的页面上。
export interface PermissionJudgeBenchmarkResult {
  // Device of the local model; null for the cloud judge.
  // 本机模型的设备；云端判定时为 null。
  device: JudgeDevice | null;
  modelId: string;
  batchSize: number;
  rounds: number;
  p50Ms: number;
  p95Ms: number;
  perItemMs: number;
  itemsPerSecond: number;
}

// ProjectPermissionJudgePolicySnapshot holds a project's choices against the model in use.
// ProjectPermissionJudgePolicySnapshot 保存项目针对使用中模型的选择。
export interface ProjectPermissionJudgePolicySnapshot {
  projectId: string;
  options: (PermissionJudgePolicyOption & { chosenMode: string | null })[];
  unsupportedOptions: Record<string, string>;
  issue: PermissionJudgeIssue | null;
}

// JudgeLabel is one judge decision.
// JudgeLabel 是一个判定结果。
export type JudgeLabel = "allow" | "review" | "block";

// PermissionJudgeRecordTrace is what one judgement read and produced.
// PermissionJudgeRecordTrace 是一次判定读取与产生的内容。
export interface PermissionJudgeRecordTrace {
  backend: string;
  model: string;
  policyVersion: string;
  settings: Record<string, string>;
  rendered: string;
  // Raw local scores in label order; null for the cloud judge.
  // 按标签顺序排列的本机原始分数；云端判定时为 null。
  logits: [number, number, number] | null;
  allowProbability: number | null;
  inputTokens: number | null;
}

// PermissionJudgeRecord is one decision the judge or the reviewer took part in.
// PermissionJudgeRecord 是一次判定器或复核器参与的决定。
export interface PermissionJudgeRecord {
  id: string;
  judgedAtMs: number;
  sessionId: string;
  workspaceRoot: string | null;
  toolName: string;
  // Argument text exactly as the agent model sent it.
  // 代理模型发送的原样参数文本。
  arguments: string;
  highRisk: string | null;
  judge: {
    // Absent when the judge could not score the call.
    // 判定器无法评分时缺省。
    label: JudgeLabel | null;
    error: string | null;
    elapsedMs: number;
    trace: PermissionJudgeRecordTrace | null;
  } | null;
  review: {
    verdict: string | null;
    reason: string | null;
    error: string | null;
    elapsedMs: number;
  } | null;
  // Which layer decided and how, such as "user_denied".
  // 由哪一层决定以及如何决定，例如 "user_denied"。
  outcome: string;
  // Label the user says the judge should have given.
  // 用户认为判定器应给出的标签。
  correction: JudgeLabel | null;
}

// PermissionJudgeRecordsRequest reads one page of the decision log, newest first.
// PermissionJudgeRecordsRequest 读取判定记录的一页，最新在前。
export interface PermissionJudgeRecordsRequest {
  // "allow", "review", "block" or "failed".
  // "allow"、"review"、"block" 或 "failed"。
  label: string | null;
  workspace: string | null;
  markedOnly: boolean;
  before: string | null;
  limit: number;
}

// PermissionJudgeRecordPage is one page of the decision log with the facts the log page shows.
// PermissionJudgeRecordPage 是判定记录的一页，以及记录页显示的信息。
export interface PermissionJudgeRecordPage {
  records: PermissionJudgeRecord[];
  hasMore: boolean;
  total: number;
  marked: number;
  workspaces: string[];
  retentionRecords: number;
  retentionDays: number;
}

// PermissionJudgeRecordExport is the marked records as training samples in JSON Lines.
// PermissionJudgeRecordExport 是以 JSON Lines 表示、作为训练样本的已标记记录。
export interface PermissionJudgeRecordExport {
  fileName: string;
  content: string;
  count: number;
}

// UnattendedSystemDefaultSnapshot holds the system default deny policy for unattended mode.
// UnattendedSystemDefaultSnapshot 保存无人值守模式的系统默认拒绝策略。
export interface UnattendedSystemDefaultSnapshot {
  denyPolicy: string;
  maxChars: number;
}

// ProjectUnattendedDefaultSnapshot holds one project's default next to the system default it inherits.
// ProjectUnattendedDefaultSnapshot 保存一个项目的默认值及其继承的系统默认值。
export interface ProjectUnattendedDefaultSnapshot {
  projectId: string;
  // null follows the system default.
  // null 表示采用系统默认值。
  denyPolicy: string | null;
  systemDenyPolicy: string;
  maxChars: number;
}
