import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import type { ServiceRequestOptions } from "@/service/protocol";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type {
  VceBlobDetailResponse,
  VceFileSummary,
  VceGlobalConfigurationStatus,
  VceIntegrityIssueSummary,
  VceMaintenancePlanResponse,
  VceMaintenanceExecutionResponse,
  VceOperationSummary,
  VceProjectChangedEvent,
  VceJobSummary,
  VceJobSubmitRequest,
  VcePageResponse,
  VceProjectOverviewResponse,
  VceProjectActivityResponse,
  VceProjectSettingsInput,
  VceProjectSettingsSummary,
  VceEmbeddingPreflight,
  PublicAiInferenceRoute,
} from "@/types";

// Inspect a draft without model calls, route activation, or index creation.
// 检查草稿，不调用模型、不激活路由、不创建索引。
// route and dimension are explicit UI intent; returns host-validated candidate and project impacts.
// route 与 dimension 为明确界面意图；返回宿主验证的候选及项目影响。
export async function preflightVceEmbeddingRoute(route: PublicAiInferenceRoute | null,
  dimension?: number): Promise<VceEmbeddingPreflight> {
  if (!hasServiceRuntime()) throw new Error("语义引擎模型预检需要连接宿主服务。");
  return serviceClient.request<VceEmbeddingPreflight>("service_preflight_vce_embedding_route", {
    request: { route, dimension },
  });
}

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
      updateMode: "manual",
      autoUpdateMinIntervalSeconds: 300,
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
// options optionally carries the caller-owned transport abort signal.
// options 可选携带调用方持有的传输中止信号。
export async function loadVceProjectOverview(
  projectId: string,
  options?: ServiceRequestOptions,
): Promise<VceProjectOverviewResponse> {
  if (!hasServiceRuntime()) {
    return browserVceOverview(projectId);
  }
  return serviceClient.request<VceProjectOverviewResponse>("service_get_vce_project_overview", {
    request: { projectId },
  }, options);
}

// Read one project's lightweight activity for shared composer and status surfaces.
// 读取一个项目的轻量活动，供共享输入区与状态界面使用。
// projectId selects authoritative ownership. Returns no synthetic initialization or progress.
// projectId 选择权威归属。不返回虚构初始化或进度。
// options optionally carries the caller-owned transport abort signal.
// options 可选携带调用方持有的传输中止信号。
export async function loadVceProjectActivity(
  projectId: string,
  options?: ServiceRequestOptions,
): Promise<VceProjectActivityResponse> {
  if (!hasServiceRuntime()) {
    const overview = browserVceOverview(projectId);
    return { configuration: overview.configuration, settings: overview.settings, status: overview.status,
      queryAvailable: false, lastSuccessfulUpdateUnixMs: null,
      sourceFreshness: { watching: false, pendingChanges: false, reconciliationRequired: true } };
  }
  return serviceClient.request<VceProjectActivityResponse>("service_get_vce_project_activity", {
    request: { projectId },
  }, options);
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

// Submit the exact human command and return after durable admission, before indexing finishes.
// 提交精确人为命令，在持久准入后、索引完成前返回。
// request contains project/checkpoint guards and a stable replay key; returns a durable receipt.
// request 包含项目、检查点保护与稳定重放键；返回持久回执。
export async function submitVceJob(request: VceJobSubmitRequest): Promise<VceJobSummary> {
  if (!hasServiceRuntime()) {
    throw new Error("请先连接服务，再创建或同步语义索引。");
  }
  return serviceClient.request<VceJobSummary>("service_submit_vce_job", { request });
}

// Read the latest job or an exact stable ID without starting indexing or invoking a model.
// 读取最近作业或精确稳定编号，不启动索引或调用模型。
// projectId owns the receipt; jobId selects exact history when provided. Returns absence for no jobs.
// projectId 持有回执；提供 jobId 时选择精确历史。没有作业时返回空值。
// options optionally carries the caller-owned transport abort signal.
// options 可选携带调用方持有的传输中止信号。
export async function loadVceJob(
  projectId: string,
  jobId: string | null = null,
  options?: ServiceRequestOptions,
): Promise<VceJobSummary | null> {
  if (!hasServiceRuntime()) return null;
  return serviceClient.request<VceJobSummary | null>("service_get_vce_job", { request: { projectId, jobId } }, options);
}

// Cancel the exact displayed host job; this never shares conversation cancellation.
// 取消精确展示的宿主作业；绝不共享会话取消。
// projectId and jobId identify the receipt. Returns durable cancellation acknowledgement.
// projectId 与 jobId 标识回执。返回持久取消确认。
export async function cancelVceJob(projectId: string, jobId: string): Promise<VceJobSummary> {
  if (!hasServiceRuntime()) throw new Error("请先连接服务，再取消语义索引作业。");
  return serviceClient.request<VceJobSummary>("service_cancel_vce_job", { request: { projectId, jobId } });
}

// Observe durable project-wide progress, with stable IDs supporting read-after-reconnect.
// 观察持久项目级进度，通过稳定编号支持重连后读取。
// listener receives acknowledged receipts. Returns an unsubscribe function.
// listener 接收已确认回执。返回取消订阅函数。
export async function subscribeVceJobChanged(listener: (job: VceJobSummary) => void): Promise<() => void> {
  if (!hasServiceRuntime()) return () => undefined;
  return serviceClient.subscribe<VceJobSummary>("vce.job.changed", listener);
}

// listVceFiles loads one bounded project file page.
// listVceFiles 加载一页有界项目文件记录。
// projectId and expectedReadIdentity freeze the displayed publication; cursor/limit select its page.
// projectId 与 expectedReadIdentity 固定所显示发布；cursor/limit 选择其分页。
// Returns that page or propagates a stale-publication rejection without changing identity.
// 返回该分页，或直接传递过期发布拒绝，不修改身份。
export async function listVceFiles(
  projectId: string,
  expectedReadIdentity: string,
  cursor: string | null = null,
  limit = 100,
): Promise<VcePageResponse<VceFileSummary>> {
  if (!hasServiceRuntime()) {
    return { items: [], nextCursor: null, total: 0 };
  }
  return serviceClient.request<VcePageResponse<VceFileSummary>>("service_list_vce_files", {
    request: {
      projectId,
      expectedReadIdentity,
      pathPrefix: null,
      status: null,
      auxiliaryFailuresOnly: false,
      page: { cursor, limit },
    },
  });
}

// loadVceBlobDetail loads one exact Blob and its bounded chunks.
// loadVceBlobDetail 加载一个精确 Blob 及其有界代码块。
// projectId/blobName select the record within expectedReadIdentity; returns detail or explicit absence.
// projectId/blobName 在 expectedReadIdentity 内选择记录；返回详情或明确空值。
export async function loadVceBlobDetail(
  projectId: string,
  blobName: string,
  expectedReadIdentity: string,
): Promise<VceBlobDetailResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<VceBlobDetailResponse | null>("service_get_vce_blob_detail", {
    request: { projectId, blobName, expectedReadIdentity },
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
// The project and read identity fix checkpoint content; live cleanup diagnostics remain current.
// 项目及读取身份固定检查点内容；实时清理诊断保持当前状态。
// cursor/limit select a bounded page; a changed publication is rejected rather than retried.
// cursor/limit 选择有界分页；发布变化时拒绝，不重试其他发布。
export async function listVceIntegrityIssues(
  projectId: string,
  expectedReadIdentity: string,
  cursor: string | null = null,
  limit = 100,
): Promise<VcePageResponse<VceIntegrityIssueSummary>> {
  if (!hasServiceRuntime()) {
    return { items: [], nextCursor: null, total: 0 };
  }
  return serviceClient.request<VcePageResponse<VceIntegrityIssueSummary>>(
    "service_list_vce_integrity_issues",
    { request: { projectId, expectedReadIdentity, page: { cursor, limit } } },
  );
}

// planVceMaintenance creates a read-only plan before any maintenance mutation.
// planVceMaintenance 在任何维护修改前创建只读计划。
export async function planVceMaintenance(
  projectId: string,
  mode: "repair" | "rebuild" | "cleanup_staging" | "compact",
): Promise<VceMaintenancePlanResponse> {
  if (!hasServiceRuntime()) {
    throw new Error("代码语义维护计划需要连接桌面服务，不能生成未绑定模型的预览。");
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
  expectedCheckpointVersion: number | null,
  actionIds: string[],
  idempotencyKey: string,
  // The confirmed preview owns this value, not the current model picker.
  // 此值由已确认预览持有，而非当前模型选择器。
  expectedEmbeddingFingerprint: string,
): Promise<VceMaintenanceExecutionResponse> {
  if (!hasServiceRuntime()) {
    throw new Error("代码语义维护只能在桌面服务连接后执行。");
  }
  // An absent checkpoint is a recovery contract reserved for rebuild admission.
  // 缺失检查点仅属于重建准入恢复契约。
  if (expectedCheckpointVersion === null && mode !== "rebuild") {
    throw new Error("修复、物理整理或暂存清理要求一个已发布检查点。");
  }
  return serviceClient.request<VceMaintenanceExecutionResponse>("service_execute_vce_maintenance", {
    request: {
      idempotencyKey,
      expectedEmbeddingFingerprint,
      projectId,
      mode,
      expectedSettingsRevision,
      expectedCheckpointVersion,
      actionIds,
    },
  });
}

// Retry one exact durable job through the common asynchronous admission path.
// 通过公共异步准入路径重试一个精确持久作业。
// Inputs freeze visible project/checkpoint authority and origin; returns the new durable receipt.
// 输入冻结可见项目、检查点权威及来源；返回新持久回执。
export async function retryVceOperation(
  projectId: string,
  operationId: string,
  expectedSettingsRevision: number,
  expectedCheckpointVersion: number | null,
): Promise<VceJobSummary> {
  return submitVceJob({ projectId, retryOfJobId: operationId, expectedSettingsRevision,
    expectedCheckpointVersion, action: expectedCheckpointVersion === null ? "initialize" : "synchronize",
    idempotencyKey: crypto.randomUUID() });
}

// cancelVceOperation requests cancellation of the matching currently active refresh.
// cancelVceOperation 请求取消匹配的当前活动刷新。
export async function cancelVceOperation(
  projectId: string,
  operationId: string,
): Promise<VceJobSummary> {
  return cancelVceJob(projectId, operationId);
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
