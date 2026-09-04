import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type {
  VceBlobDetailResponse,
  VceFileSummary,
  VceGlobalConfigurationStatus,
  VceIntegrityIssueSummary,
  VceMaintenancePlanResponse,
  VceOperationSummary,
  VceProjectChangedEvent,
  VcePageResponse,
  VceProjectOverviewResponse,
  VceProjectSettingsInput,
  VceProjectSettingsSummary,
} from "@/types";

// browserVceOverview returns an explicit unavailable preview instead of inventing a ready index.
// browserVceOverview 返回明确不可用的预览，而不是虚构就绪索引。
function browserVceOverview(projectId: string): VceProjectOverviewResponse {
  const project = browserFallbackSnapshot().projects.find((candidate) => candidate.id === projectId);
  if (!project) {
    throw new Error("Preview project does not exist.");
  }
  const enabled = Boolean(project.vceEnabled);
  return {
    configuration: browserVceConfigurationStatus(),
    settings: {
      enabled,
      revision: project.vceSettingsRevision ?? 1,
      migratedFromCodebaseMemory: false,
      maxFileBytes: 1_048_576,
      extraIgnorePatterns: [],
      allowQueryRewrite: false,
      allowRerank: true,
      embeddingMaxConcurrency: 4,
      queryMaxConcurrency: 3,
      maxResults: 10,
      maxContextChars: 32_000,
    },
    status: {
      projectId,
      enabled: false,
      settingsRevision: project.vceSettingsRevision ?? 1,
      lifecycle: enabled ? "blocked_configuration" : "disabled",
      presentFiles: 0,
      checkpointBlobs: 0,
      lastError: enabled ? "代码语义服务仅在桌面服务连接后可用。" : null,
    },
    ai: null,
    counts: null,
    integrity: null,
  };
}

// browserVceConfigurationStatus returns an explicit closed state outside the desktop service.
// browserVceConfigurationStatus 在桌面服务之外返回明确的关闭状态。
function browserVceConfigurationStatus(): VceGlobalConfigurationStatus {
  const issue = "代码语义模型配置只能由桌面服务验证。";
  return {
    ready: false,
    consumerProfileId: "vce",
    queryRewritePurposeId: "code_retrieval.query_rewrite",
    reasoningPolicy: "fixed_off",
    embedding: { configured: false, available: false, issue },
    queryRewrite: { configured: false, available: false, issue },
    rerank: { configured: false, available: false, issue },
    issues: [issue],
  };
}

// loadVceConfigurationStatus loads the only global prerequisite gate used by VCE.
// loadVceConfigurationStatus 加载 VCE 使用的唯一全局前置闸门。
export async function loadVceConfigurationStatus(): Promise<VceGlobalConfigurationStatus> {
  if (!hasServiceRuntime()) {
    return browserVceConfigurationStatus();
  }
  return serviceClient.request<VceGlobalConfigurationStatus>(
    "service_get_vce_configuration_status",
  );
}

// loadVceProjectOverview loads current settings and provider-independent runtime diagnostics.
// loadVceProjectOverview 加载当前设置与不依赖 Provider 的运行诊断。
export async function loadVceProjectOverview(projectId: string): Promise<VceProjectOverviewResponse> {
  if (!hasServiceRuntime()) {
    return browserVceOverview(projectId);
  }
  return serviceClient.request<VceProjectOverviewResponse>("service_get_vce_project_overview", {
    request: { projectId },
  });
}

// updateVceProjectSettings saves one complete settings document under optimistic revision control.
// updateVceProjectSettings 在乐观修订控制下保存一份完整设置文档。
export async function updateVceProjectSettings(
  projectId: string,
  expectedRevision: number,
  settings: VceProjectSettingsInput,
): Promise<VceProjectSettingsSummary> {
  if (!hasServiceRuntime()) {
    throw new Error("代码语义设置只能在桌面服务连接后保存。");
  }
  return serviceClient.request<VceProjectSettingsSummary>("service_update_vce_project_settings", {
    request: { projectId, expectedRevision, settings },
  });
}

// refreshVceProject requests a full refresh guarded by the visible settings revision.
// refreshVceProject 请求一次由可见设置修订号保护的全量刷新。
export async function refreshVceProject(
  projectId: string,
  expectedSettingsRevision: number,
): Promise<VceProjectOverviewResponse> {
  if (!hasServiceRuntime()) {
    throw new Error("代码语义索引只能在桌面服务连接后刷新。");
  }
  return serviceClient.request<VceProjectOverviewResponse>("service_refresh_vce_project", {
    request: {
      projectId,
      expectedSettingsRevision,
      expectedCheckpointVersion: null,
      actionIds: [],
    },
  });
}

// listVceFiles loads one bounded project file page.
// listVceFiles 加载一页有界项目文件记录。
export async function listVceFiles(
  projectId: string,
  cursor: string | null = null,
  limit = 100,
): Promise<VcePageResponse<VceFileSummary>> {
  if (!hasServiceRuntime()) {
    return { items: [], nextCursor: null, total: 0 };
  }
  return serviceClient.request<VcePageResponse<VceFileSummary>>("service_list_vce_files", {
    request: {
      projectId,
      pathPrefix: null,
      status: null,
      auxiliaryFailuresOnly: false,
      page: { cursor, limit },
    },
  });
}

// loadVceBlobDetail loads one exact Blob and its bounded chunks.
// loadVceBlobDetail 加载一个精确 Blob 及其有界代码块。
export async function loadVceBlobDetail(
  projectId: string,
  blobName: string,
): Promise<VceBlobDetailResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<VceBlobDetailResponse | null>("service_get_vce_blob_detail", {
    request: { projectId, blobName },
  });
}

// listVceOperations loads one bounded operation-history page.
// listVceOperations 加载一页有界操作历史。
export async function listVceOperations(
  projectId: string,
  cursor: string | null = null,
  limit = 100,
): Promise<VcePageResponse<VceOperationSummary>> {
  if (!hasServiceRuntime()) {
    return { items: [], nextCursor: null, total: 0 };
  }
  return serviceClient.request<VcePageResponse<VceOperationSummary>>("service_list_vce_operations", {
    request: { projectId, page: { cursor, limit } },
  });
}

// listVceIntegrityIssues loads one bounded integrity-diagnostic page.
// listVceIntegrityIssues 加载一页有界完整性诊断。
export async function listVceIntegrityIssues(
  projectId: string,
  cursor: string | null = null,
  limit = 100,
): Promise<VcePageResponse<VceIntegrityIssueSummary>> {
  if (!hasServiceRuntime()) {
    return { items: [], nextCursor: null, total: 0 };
  }
  return serviceClient.request<VcePageResponse<VceIntegrityIssueSummary>>(
    "service_list_vce_integrity_issues",
    { request: { projectId, page: { cursor, limit } } },
  );
}

// planVceMaintenance creates a read-only plan before any maintenance mutation.
// planVceMaintenance 在任何维护修改前创建只读计划。
export async function planVceMaintenance(
  projectId: string,
  mode: "repair" | "rebuild" | "cleanup_staging",
): Promise<VceMaintenancePlanResponse> {
  if (!hasServiceRuntime()) {
    return { mode, actions: [], affectedBlobCount: 0 };
  }
  return serviceClient.request<VceMaintenancePlanResponse>("service_plan_vce_maintenance", {
    request: { projectId, mode },
  });
}

// executeVceMaintenance executes only the exact action identities from a current guarded plan.
// executeVceMaintenance 仅执行当前受保护计划中的精确操作身份。
export async function executeVceMaintenance(
  projectId: string,
  mode: string,
  expectedSettingsRevision: number,
  expectedCheckpointVersion: number,
  actionIds: string[],
): Promise<VceProjectOverviewResponse> {
  if (!hasServiceRuntime()) {
    throw new Error("代码语义维护只能在桌面服务连接后执行。");
  }
  return serviceClient.request<VceProjectOverviewResponse>("service_execute_vce_maintenance", {
    request: {
      projectId,
      mode,
      expectedSettingsRevision,
      expectedCheckpointVersion,
      actionIds,
    },
  });
}

// retryVceOperation retries one visible failed operation as a complete recovery refresh.
// retryVceOperation 将一项可见失败操作重试为完整恢复刷新。
export async function retryVceOperation(
  projectId: string,
  operationId: string,
  expectedSettingsRevision: number,
  expectedCheckpointVersion: number | null,
): Promise<VceProjectOverviewResponse> {
  if (!hasServiceRuntime()) {
    throw new Error("代码语义作业只能在桌面服务连接后重试。");
  }
  return serviceClient.request<VceProjectOverviewResponse>("service_retry_vce_operation", {
    request: { projectId, operationId, expectedSettingsRevision, expectedCheckpointVersion },
  });
}

// cancelVceOperation requests cancellation of the matching currently active refresh.
// cancelVceOperation 请求取消匹配的当前活动刷新。
export async function cancelVceOperation(
  projectId: string,
  operationId: string,
  expectedSettingsRevision: number,
  expectedCheckpointVersion: number | null,
): Promise<VceProjectOverviewResponse> {
  if (!hasServiceRuntime()) {
    throw new Error("代码语义作业只能在桌面服务连接后取消。");
  }
  return serviceClient.request<VceProjectOverviewResponse>("service_cancel_vce_operation", {
    request: { projectId, operationId, expectedSettingsRevision, expectedCheckpointVersion },
  });
}

// subscribeVceProjectChanged observes project lifecycle and checkpoint invalidations.
// subscribeVceProjectChanged 观察项目生命周期与检查点失效通知。
export async function subscribeVceProjectChanged(
  listener: (event: VceProjectChangedEvent) => void,
): Promise<() => void> {
  if (!hasServiceRuntime()) {
    return () => undefined;
  }
  return serviceClient.subscribe<VceProjectChangedEvent>("vce.project.changed", listener);
}
