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
} from "@/components/project/project-settings-dialog";
import {
  browserFallbackSnapshot,
  updateBrowserFallbackWorkbench,
} from "@/service/browser-fallback-workbench";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

// runVmmMemoryAction executes one VMM memory workbench action.
// runVmmMemoryAction 执行单个 VMM 记忆工作台动作。
export async function runVmmMemoryAction(request: VmmMemoryActionRequest): Promise<VmmMemoryActionResult> {
  if (!hasServiceRuntime()) {
    return {
      action: request.action,
      summary: `预览模式已执行 ${request.action}`,
      kind: request.action,
      request,
      response: previewVmmActionResponse(request.action),
    };
  }
  return serviceClient.request<VmmMemoryActionResult>("service_run_vmm_memory_action", { request });
}

// loadProjectVmmConfig loads the editable project-scoped VMM draft.
// loadProjectVmmConfig 加载可编辑的项目级 VMM 草稿。
export async function loadProjectVmmConfig(projectId: string): Promise<VmmConfigDraftSummary> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot().vmmMemory.projectDraft;
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
    return {
      cutoffDate: request.cutoffDate,
      purgedAssetCount: 0,
      deletedFileCount: 0,
      alreadyMissingFileCount: 0,
      failedDeleteCount: 0,
      purgedByteLenLabel: "0 B",
      impactedSessionCount: 0,
      summary: "预览模式不会删除托管附件。",
    };
  }
  return serviceClient.request<ProjectAttachmentCleanupReportSummary>("service_execute_project_attachment_cleanup", { request });
}

// saveVmmMemoryConfig persists one VMM memory configuration draft.
// saveVmmMemoryConfig 持久化单个 VMM 记忆配置草稿。
export async function saveVmmMemoryConfig(request: VmmConfigSaveRequest): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      if (request.scope === "project") {
        snapshot.vmmMemory.projectDraft = { ...request.draft, hasOverride: request.draft.hasOverride };
      } else {
        snapshot.vmmMemory.globalDraft = { ...request.draft, hasOverride: true };
      }
      snapshot.vmmMemory.effective = {
        ...snapshot.vmmMemory.effective,
        enabled: request.draft.enabled,
        grpcTarget: request.draft.grpcTarget,
        grpcConfigured: request.draft.grpcTarget.trim().length > 0,
        userId: Number(request.draft.userId) || null,
        projectId: Number(request.draft.projectId) || null,
        aiToolsEnabled: request.draft.aiToolsEnabled,
        precheckEnabled: request.draft.precheckEnabled,
        implicitMemoryTurns: Number(request.draft.implicitMemoryTurns) || snapshot.vmmMemory.effective.implicitMemoryTurns,
        profileRefreshTurns: Number(request.draft.profileRefreshTurns) || snapshot.vmmMemory.effective.profileRefreshTurns,
        profileNodeLimit: Number(request.draft.profileNodeLimit) || snapshot.vmmMemory.effective.profileNodeLimit,
        injectionTag: request.draft.injectionTag,
        injectionDescription: request.draft.injectionDescription,
      };
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_vmm_memory_config", { request });
}

// previewVmmActionResponse returns a compact browser-preview result for one VMM action.
// previewVmmActionResponse 返回单个 VMM 动作的紧凑浏览器预览结果。
function previewVmmActionResponse(action: string): unknown {
  if (action === "load_users_projects") {
    return {
      users: [{ userId: 20000, userName: "preview-user" }],
      projects: [{ projectId: 1001, displayPath: "OpenVulcan/Vulcan/Code" }],
    };
  }
  if (action === "load_profile_nodes") {
    return {
      nodes: [
        { profileNodeId: 1, content: "用户偏好工程化闭环。", priority: "high", level: "user", bindId: 20000 },
        { profileNodeId: 2, content: "当前项目正在迁移 WebUI。", priority: "normal", level: "project", bindId: 1001 },
      ],
    };
  }
  if (action === "search_memory_events") {
    return {
      results: [{ query: "webui", hits: [{ memoryId: 1, sourceTurnId: 42, abstractText: "技能页迁移到 WebUI。" }] }],
      traceId: "preview-trace",
    };
  }
  return { ok: true, preview: true };
}

// projectAttachmentPreviewFallback builds a no-op cleanup preview for browser-only mode.
// projectAttachmentPreviewFallback 为纯浏览器模式构建无副作用清理预览。
function projectAttachmentPreviewFallback(request: ProjectAttachmentCleanupRequest): ProjectAttachmentCleanupPreviewSummary {
  const selectedKinds = projectAttachmentSelectedKindLabels(request.selection);
  let summary = `预览模式将检查 ${request.cutoffDate} 之前的 ${selectedKinds.join("、")} 托管附件。`;
  if (!request.cutoffDate) {
    summary = "请选择有效的截止日期。";
  } else if (selectedKinds.length === 0) {
    summary = "至少选择一种附件类型后才能预览。";
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
      return "图片";
    case "audio":
      return "音频";
    case "video":
      return "视频";
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
