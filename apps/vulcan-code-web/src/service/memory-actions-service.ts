import type {
  VmmConfigDraftSummary,
  VmmConfigSaveRequest,
  VmmMemoryActionRequest,
  VmmMemoryActionResult,
  WorkbenchSnapshot,
} from "@/types";
import type {
  ProjectAttachmentCleanupPreviewSummary,
  ProjectAttachmentCleanupReportSummary,
  ProjectAttachmentCleanupRequest,
} from "@/components/project/project-settings-types";
import {
  browserFallbackSnapshot,
} from "@/service/browser-fallback-workbench";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

// runVmmMemoryAction executes one VMM memory workbench action.
// runVmmMemoryAction 执行单个 VMM 记忆工作台动作。
export async function runVmmMemoryAction(request: VmmMemoryActionRequest): Promise<VmmMemoryActionResult> {
  if (!hasServiceRuntime()) {
    throw new Error("The service runtime is required to execute memory actions.");
  }
  return serviceClient.request<VmmMemoryActionResult>("service_run_vmm_memory_action", { request });
}

// loadProjectVmmConfig loads the editable project-scoped VMM draft.
// loadProjectVmmConfig 加载可编辑的项目级 VMM 草稿。
export async function loadProjectVmmConfig(projectId: string): Promise<VmmConfigDraftSummary> {
  if (!hasServiceRuntime()) {
    return browserPreviewProjectVmmDraft(browserFallbackSnapshot(), projectId);
  }
  return serviceClient.request<VmmConfigDraftSummary>("service_load_project_vmm_config", { projectId });
}

// previewProjectAttachmentCleanup builds a project-scoped managed attachment cleanup preview.
// previewProjectAttachmentCleanup 构建项目级托管附件清理预览。
export async function previewProjectAttachmentCleanup(request: ProjectAttachmentCleanupRequest): Promise<ProjectAttachmentCleanupPreviewSummary> {
  if (!hasServiceRuntime()) {
    return projectAttachmentPreviewFallback(request);
  }
  return serviceClient.request<ProjectAttachmentCleanupPreviewSummary>("service_preview_project_attachment_cleanup", { request });
}

// executeProjectAttachmentCleanup runs one project-scoped managed attachment cleanup.
// executeProjectAttachmentCleanup 执行单次项目级托管附件清理。
export async function executeProjectAttachmentCleanup(request: ProjectAttachmentCleanupRequest): Promise<ProjectAttachmentCleanupReportSummary> {
  if (!hasServiceRuntime()) {
    throw new Error("The service runtime is required to execute attachment cleanup.");
  }
  return serviceClient.request<ProjectAttachmentCleanupReportSummary>("service_execute_project_attachment_cleanup", { request });
}

// saveVmmMemoryConfig persists one VMM memory configuration draft.
// saveVmmMemoryConfig 持久化单个 VMM 记忆配置草稿。
export async function saveVmmMemoryConfig(request: VmmConfigSaveRequest): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("The service runtime is required to save memory configuration.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_vmm_memory_config", { request });
}

// browserPreviewProjectVmmDraft resolves one exact project draft without borrowing another project's override.
// browserPreviewProjectVmmDraft 解析一个准确项目草稿，不借用其他项目的覆盖配置。
// Parameters: snapshot owns the preview project catalog and projectId selects the requested draft.
// 参数：snapshot 持有预览项目目录，projectId 选择请求草稿。
// Returns: the selected fixture override or a project inheritance draft derived from global configuration.
// 返回值：选中夹具覆盖配置，或从全局配置派生的项目继承草稿。
function browserPreviewProjectVmmDraft(
  snapshot: WorkbenchSnapshot,
  projectId: string,
): VmmConfigDraftSummary {
  // project is the authoritative preview catalog entry for this request.
  // project 是当前请求对应的权威预览目录条目。
  const project = snapshot.vmmMemory.projectScopes.find((candidate) => candidate.projectId === projectId);
  if (!project) {
    throw new Error("Preview project VMM scope does not exist.");
  }
  if (!project.hasOverride) {
    return {
      ...snapshot.vmmMemory.globalDraft,
      hasOverride: false,
      precheckMode: "inherit",
      memoryWritebackMode: "inherit",
    };
  }
  if (snapshot.vmmMemory.selectedProjectId === projectId && snapshot.vmmMemory.projectDraft.hasOverride) {
    return { ...snapshot.vmmMemory.projectDraft };
  }
  throw new Error("Preview mode does not have the requested project VMM override payload.");
}

// projectAttachmentPreviewFallback builds a no-op cleanup preview for browser-only mode.
// projectAttachmentPreviewFallback 为纯浏览器模式构建无副作用清理预览。
function projectAttachmentPreviewFallback(request: ProjectAttachmentCleanupRequest): ProjectAttachmentCleanupPreviewSummary {
  const selectedKinds = projectAttachmentSelectedKindLabels(request.selection);
  let summary = `Preview mode will inspect managed ${selectedKinds.join(", ")} attachments before ${request.cutoffDate}.`;
  if (!request.cutoffDate) {
    summary = "Select a valid cutoff date.";
  } else if (selectedKinds.length === 0) {
    summary = "Select at least one attachment kind before previewing.";
  }
  return {
    cutoffDate: request.cutoffDate,
    matchedAssetCount: 0,
    matchedByteLenLabel: "0 B",
    impactedSessionCount: 0,
    perKindCounts: {},
    summary,
  };
}

// projectAttachmentKindLabel returns the localized label for an attachment kind.
// projectAttachmentKindLabel 返回附件类型的本地化标签。
function projectAttachmentKindLabel(kind: keyof ProjectAttachmentCleanupRequest["selection"]): string {
  switch (kind) {
    case "image":
      return "image";
    case "audio":
      return "audio";
    case "video":
      return "video";
    case "pdf":
      return "PDF";
  }
}

// projectAttachmentSelectedKindLabels returns selected cleanup-kind labels.
// projectAttachmentSelectedKindLabels 返回选中的清理类型标签。
function projectAttachmentSelectedKindLabels(kinds: ProjectAttachmentCleanupRequest["selection"]): string[] {
  return (Object.keys(kinds) as Array<keyof ProjectAttachmentCleanupRequest["selection"]>)
    .filter((kind) => kinds[kind])
    .map(projectAttachmentKindLabel);
}
