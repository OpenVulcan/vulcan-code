import { serviceClient } from "@/service/webui-service-runtime";
import type {
  JudgeLabel,
  PermissionJudgeBenchmarkResult,
  PermissionJudgeRecord,
  PermissionJudgeRecordExport,
  PermissionJudgeRecordPage,
  PermissionJudgeRecordsRequest,
  PermissionJudgeCudaComponent,
  PermissionJudgeEdgeModels,
  PermissionJudgeModelFormat,
  PermissionJudgePackageInspection,
  PermissionJudgeSettingsSaveRequest,
  PermissionJudgeSettingsSnapshot,
  PermissionJudgeStatus,
  ProjectPermissionJudgePolicySnapshot,
  ProjectReviewRulesSnapshot,
  ProjectUnattendedDefaultSnapshot,
  ReviewRules,
  ReviewRulesSystemDefaultSnapshot,
  UnattendedSystemDefaultSnapshot,
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

// loadPermissionJudgeEdgeModels lists the official direct models the signed-in OpenVulcan account's
// deployment currently serves; it fails when nobody is signed in.
// loadPermissionJudgeEdgeModels 列出已登录 OpenVulcan 账户所属部署当前提供的官方直连模型；无人登录时失败。
export async function loadPermissionJudgeEdgeModels(): Promise<PermissionJudgeEdgeModels> {
  return serviceClient.request<PermissionJudgeEdgeModels>("service_permission_judge_edge_models", {});
}

// savePermissionJudgeSettings saves the system-level choices.
// savePermissionJudgeSettings 保存系统级选择。
export async function savePermissionJudgeSettings(
  request: PermissionJudgeSettingsSaveRequest,
): Promise<PermissionJudgeSettingsSnapshot> {
  return serviceClient.request<PermissionJudgeSettingsSnapshot>("service_save_permission_judge_settings", { request });
}

// inspectPermissionJudgePackage verifies a package directory without registering it.
// inspectPermissionJudgePackage 验证模型包目录但不登记。
export async function inspectPermissionJudgePackage(path: string): Promise<PermissionJudgePackageInspection> {
  return serviceClient.request<PermissionJudgePackageInspection>("service_inspect_permission_judge_package", {
    request: { path },
  });
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

// downloadPermissionJudgeModel starts downloading one published format of an official package;
// it is verified and registered without being put in use.
// downloadPermissionJudgeModel 开始下载官方模型包的一个已发布格式；下载后验证并登记，但不投入使用。
export async function downloadPermissionJudgeModel(
  packageId: string,
  format: PermissionJudgeModelFormat,
): Promise<PermissionJudgeStatus> {
  return serviceClient.request<PermissionJudgeStatus>("service_download_permission_judge_model", {
    request: { packageId, format },
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

// loadPermissionJudgeRecords reads one page of the decision log, newest first.
// loadPermissionJudgeRecords 读取判定记录的一页，最新在前。
export async function loadPermissionJudgeRecords(request: PermissionJudgeRecordsRequest): Promise<PermissionJudgeRecordPage> {
  return serviceClient.request<PermissionJudgeRecordPage>("service_permission_judge_records", { request });
}

// markPermissionJudgeRecord marks the label the judge should have given, or clears it with null.
// markPermissionJudgeRecord 标记判定器本应给出的标签，传 null 时清除标记。
export async function markPermissionJudgeRecord(id: string, correction: JudgeLabel | null): Promise<PermissionJudgeRecord> {
  return serviceClient.request<PermissionJudgeRecord>("service_mark_permission_judge_record", {
    request: { id, correction },
  });
}

// exportPermissionJudgeRecords returns the marked records as training samples.
// exportPermissionJudgeRecords 返回作为训练样本的已标记记录。
export async function exportPermissionJudgeRecords(): Promise<PermissionJudgeRecordExport> {
  return serviceClient.request<PermissionJudgeRecordExport>("service_export_permission_judge_records", {});
}

// clearPermissionJudgeRecords removes every record of the decision log.
// clearPermissionJudgeRecords 删除判定记录中的全部记录。
export async function clearPermissionJudgeRecords(): Promise<void> {
  await serviceClient.request<null>("service_clear_permission_judge_records", {});
}

// loadUnattendedSystemDefault reads the system default deny policy for unattended mode.
// loadUnattendedSystemDefault 读取无人值守模式的系统默认拒绝策略。
export async function loadUnattendedSystemDefault(): Promise<UnattendedSystemDefaultSnapshot> {
  return serviceClient.request<UnattendedSystemDefaultSnapshot>("service_unattended_system_default", {});
}

// saveUnattendedSystemDefault saves the system default; running sessions keep what they confirmed.
// saveUnattendedSystemDefault 保存系统默认值；运行中的会话保留其已确认的内容。
export async function saveUnattendedSystemDefault(denyPolicy: string): Promise<UnattendedSystemDefaultSnapshot> {
  return serviceClient.request<UnattendedSystemDefaultSnapshot>("service_save_unattended_system_default", {
    request: { denyPolicy },
  });
}

// loadProjectUnattendedDefault reads one project's default deny policy.
// loadProjectUnattendedDefault 读取一个项目的默认拒绝策略。
export async function loadProjectUnattendedDefault(projectId: string): Promise<ProjectUnattendedDefaultSnapshot> {
  return serviceClient.request<ProjectUnattendedDefaultSnapshot>("service_project_unattended_default", { projectId });
}

// saveProjectUnattendedDefault saves one project's default; null returns it to the system default.
// saveProjectUnattendedDefault 保存一个项目的默认值；null 使其恢复采用系统默认值。
export async function saveProjectUnattendedDefault(
  projectId: string,
  denyPolicy: string | null,
): Promise<ProjectUnattendedDefaultSnapshot> {
  return serviceClient.request<ProjectUnattendedDefaultSnapshot>("service_save_project_unattended_default", {
    request: { projectId, denyPolicy },
  });
}

// loadReviewRulesSystemDefault reads the review rules every project uses unless it keeps its own.
// loadReviewRulesSystemDefault 读取所有项目在未保留自身规则时使用的复核规则。
export async function loadReviewRulesSystemDefault(): Promise<ReviewRulesSystemDefaultSnapshot> {
  return serviceClient.request<ReviewRulesSystemDefaultSnapshot>("service_review_rules_system_default", {});
}

// saveReviewRulesSystemDefault saves the system review rules; the next review reads them.
// saveReviewRulesSystemDefault 保存系统复核规则；下一次复核即读取它们。
export async function saveReviewRulesSystemDefault(rules: ReviewRules): Promise<ReviewRulesSystemDefaultSnapshot> {
  return serviceClient.request<ReviewRulesSystemDefaultSnapshot>("service_save_review_rules_system_default", {
    request: { rules },
  });
}

// loadProjectReviewRules reads one project's review rules.
// loadProjectReviewRules 读取一个项目的复核规则。
export async function loadProjectReviewRules(projectId: string): Promise<ProjectReviewRulesSnapshot> {
  return serviceClient.request<ProjectReviewRulesSnapshot>("service_project_review_rules", { projectId });
}

// saveProjectReviewRules saves one project's rules; null returns it to the system rules.
// saveProjectReviewRules 保存一个项目的规则；null 使其恢复采用系统规则。
export async function saveProjectReviewRules(
  projectId: string,
  rules: ReviewRules | null,
): Promise<ProjectReviewRulesSnapshot> {
  return serviceClient.request<ProjectReviewRulesSnapshot>("service_save_project_review_rules", {
    request: { projectId, rules },
  });
}
