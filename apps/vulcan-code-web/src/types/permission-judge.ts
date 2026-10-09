// Permission judge settings, model registry, worker status and CUDA runtime, mirroring
// `vulcan_service_api::permission_judge`.
// 权限判定设置、模型登记表、worker 状态与 CUDA 执行库，镜像 `vulcan_service_api::permission_judge`。

// JudgeState is off or enabled; null means not configured yet.
// JudgeState 为关闭或启用；null 表示尚未设置。
export type JudgeState = "off" | "enabled";

// JudgeBackend names where calls are judged.
// JudgeBackend 指定判定调用的位置。
export type JudgeBackend = "local";

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
}

// PermissionJudgeBenchmarkResult is one speed test, kept only on the page that ran it.
// PermissionJudgeBenchmarkResult 是一次速度测试，只保留在执行它的页面上。
export interface PermissionJudgeBenchmarkResult {
  device: JudgeDevice;
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
