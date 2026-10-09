import { serviceClient } from "@/service/webui-service-runtime";
import type {
  PermissionJudgeBenchmarkResult,
  PermissionJudgeCudaComponent,
  PermissionJudgeSettingsSaveRequest,
  PermissionJudgeSettingsSnapshot,
  PermissionJudgeStatus,
  ProjectPermissionJudgePolicySnapshot,
} from "@/types";

// loadPermissionJudgeSettings reads the settings page snapshot, including device and CUDA probing.
// loadPermissionJudgeSettings 读取设置页快照，包括设备与 CUDA 探测。
export async function loadPermissionJudgeSettings(): Promise<PermissionJudgeSettingsSnapshot> {
  return serviceClient.request<PermissionJudgeSettingsSnapshot>("service_permission_judge_settings", {});
}

// loadPermissionJudgeStatus reads the live worker and installation status for polling.
// loadPermissionJudgeStatus 读取用于轮询的实时 worker 与安装状态。
export async function loadPermissionJudgeStatus(): Promise<PermissionJudgeStatus> {
  return serviceClient.request<PermissionJudgeStatus>("service_permission_judge_status", {});
}

// savePermissionJudgeSettings saves the system-level choices.
// savePermissionJudgeSettings 保存系统级选择。
export async function savePermissionJudgeSettings(
  request: PermissionJudgeSettingsSaveRequest,
): Promise<PermissionJudgeSettingsSnapshot> {
  return serviceClient.request<PermissionJudgeSettingsSnapshot>("service_save_permission_judge_settings", { request });
}

// registerPermissionJudgeModel verifies a package directory and registers it.
// registerPermissionJudgeModel 验证模型包目录并登记。
export async function registerPermissionJudgeModel(
  path: string,
  name: string,
  custom: boolean,
): Promise<PermissionJudgeSettingsSnapshot> {
  return serviceClient.request<PermissionJudgeSettingsSnapshot>("service_register_permission_judge_model", {
    request: { path, name, custom },
  });
}

// activatePermissionJudgeModel verifies, warms and switches to a registered model.
// activatePermissionJudgeModel 验证、预热并切换到已登记模型。
export async function activatePermissionJudgeModel(modelId: string): Promise<PermissionJudgeSettingsSnapshot> {
  return serviceClient.request<PermissionJudgeSettingsSnapshot>("service_activate_permission_judge_model", {
    request: { modelId },
  });
}

// rollbackPermissionJudgeModel returns to the model used before the last switch.
// rollbackPermissionJudgeModel 返回上次切换前使用的模型。
export async function rollbackPermissionJudgeModel(): Promise<PermissionJudgeSettingsSnapshot> {
  return serviceClient.request<PermissionJudgeSettingsSnapshot>("service_rollback_permission_judge_model", {});
}

// removePermissionJudgeModel removes a registration that is not in use; files stay on disk.
// removePermissionJudgeModel 移除未在使用的登记；文件保留在磁盘上。
export async function removePermissionJudgeModel(modelId: string): Promise<PermissionJudgeSettingsSnapshot> {
  return serviceClient.request<PermissionJudgeSettingsSnapshot>("service_remove_permission_judge_model", {
    request: { modelId },
  });
}

// reloadPermissionJudge retries loading the model in use.
// reloadPermissionJudge 重新加载使用中模型。
export async function reloadPermissionJudge(): Promise<PermissionJudgeStatus> {
  return serviceClient.request<PermissionJudgeStatus>("service_reload_permission_judge", {});
}

// benchmarkPermissionJudge times one call or a batch of ten on the model in use.
// benchmarkPermissionJudge 在使用中模型上为一条调用或十条批次计时。
export async function benchmarkPermissionJudge(batchSize: 1 | 10): Promise<PermissionJudgeBenchmarkResult> {
  return serviceClient.request<PermissionJudgeBenchmarkResult>("service_benchmark_permission_judge", {
    request: { batchSize },
  });
}

// installPermissionJudgeCuda starts installing one CUDA runtime part from the official archives.
// installPermissionJudgeCuda 开始从官方归档安装一个 CUDA 执行库部分。
export async function installPermissionJudgeCuda(
  component: PermissionJudgeCudaComponent,
): Promise<PermissionJudgeStatus> {
  return serviceClient.request<PermissionJudgeStatus>("service_install_permission_judge_cuda", {
    request: { component },
  });
}

// loadProjectPermissionJudgePolicy reads a project's choices for the model in use.
// loadProjectPermissionJudgePolicy 读取项目针对使用中模型的选择。
export async function loadProjectPermissionJudgePolicy(projectId: string): Promise<ProjectPermissionJudgePolicySnapshot> {
  return serviceClient.request<ProjectPermissionJudgePolicySnapshot>("service_project_permission_judge_policy", {
    projectId,
  });
}

// saveProjectPermissionJudgePolicy saves a project's choices; omitted options follow the default.
// saveProjectPermissionJudgePolicy 保存项目的选择；未列出的选项跟随默认值。
export async function saveProjectPermissionJudgePolicy(
  projectId: string,
  options: Record<string, string>,
): Promise<ProjectPermissionJudgePolicySnapshot> {
  return serviceClient.request<ProjectPermissionJudgePolicySnapshot>("service_save_project_permission_judge_policy", {
    request: { projectId, options },
  });
}
