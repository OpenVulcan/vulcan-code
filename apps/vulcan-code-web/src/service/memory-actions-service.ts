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

// browserPreviewProjectVmmOverrides stores exact project drafts created during the browser-only session.
// browserPreviewProjectVmmOverrides 保存纯浏览器会话期间创建的准确项目草稿。
const browserPreviewProjectVmmOverrides = new Map<string, VmmConfigDraftSummary>();

// runVmmMemoryAction executes one VMM memory workbench action.
// runVmmMemoryAction 执行单个 VMM 记忆工作台动作。
export async function runVmmMemoryAction(request: VmmMemoryActionRequest): Promise<VmmMemoryActionResult> {
  if (!hasServiceRuntime()) {
    return {
      action: request.action,
      summary: `Preview mode executed ${request.action}`,
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
    return {
      cutoffDate: request.cutoffDate,
      purgedAssetCount: 0,
      deletedFileCount: 0,
      alreadyMissingFileCount: 0,
      failedDeleteCount: 0,
      purgedByteLenLabel: "0 B",
      impactedSessionCount: 0,
      summary: "Preview mode does not delete managed attachments.",
    };
  }
  return serviceClient.request<ProjectAttachmentCleanupReportSummary>("service_execute_project_attachment_cleanup", { request });
}

// saveVmmMemoryConfig persists one VMM memory configuration draft.
// saveVmmMemoryConfig 持久化单个 VMM 记忆配置草稿。
export async function saveVmmMemoryConfig(request: VmmConfigSaveRequest): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    // targetProjectId is mandatory for a project save because preview mode no longer guesses ownership.
    // targetProjectId 是项目保存的必填项，因为预览模式不再猜测归属。
    const targetProjectId = request.scope === "project" ? request.projectId?.trim() ?? "" : "";
    if (request.scope === "project") {
      // currentSnapshot validates that the requested preview project still exists.
      // currentSnapshot 校验请求的预览项目仍然存在。
      const currentSnapshot = browserFallbackSnapshot();
      if (!targetProjectId
        || !currentSnapshot.vmmMemory.projectScopes.some((project) => project.projectId === targetProjectId)) {
        throw new Error("Preview project VMM scope does not exist.");
      }
      if (request.draft.hasOverride) {
        browserPreviewProjectVmmOverrides.set(targetProjectId, { ...request.draft });
      } else {
        browserPreviewProjectVmmOverrides.delete(targetProjectId);
      }
    }
    return updateBrowserFallbackWorkbench((snapshot) => {
      if (request.scope === "project") {
        snapshot.vmmMemory.projectScopes = snapshot.vmmMemory.projectScopes.map((project) => (
          project.projectId === targetProjectId
            ? { ...project, hasOverride: request.draft.hasOverride }
            : project
        ));
        if (snapshot.vmmMemory.selectedProjectId === targetProjectId) {
          snapshot.vmmMemory.projectDraft = browserPreviewProjectVmmDraft(snapshot, targetProjectId);
        }
      } else {
        snapshot.vmmMemory.globalDraft = { ...request.draft, hasOverride: true };
        if (snapshot.vmmMemory.selectedProjectId && !snapshot.vmmMemory.projectDraft.hasOverride) {
          snapshot.vmmMemory.projectDraft = browserPreviewProjectVmmDraft(
            snapshot,
            snapshot.vmmMemory.selectedProjectId,
          );
        }
      }
      if (request.scope !== "project" || snapshot.vmmMemory.selectedProjectId === targetProjectId) {
        snapshot.vmmMemory.effective = {
          ...snapshot.vmmMemory.effective,
          enabled: request.draft.enabled,
          grpcTarget: request.draft.grpcTarget,
          grpcConfigured: request.draft.grpcTarget.trim().length > 0,
          userId: Number(request.draft.userId) || null,
          projectId: Number(request.draft.projectId) || null,
          diagnostic: request.draft.enabled ? "ready" : "disabled_by_config",
          precheckDiagnostic: request.draft.enabled && request.draft.precheckEnabled ? "ready" : "disabled_by_config",
          writebackDiagnostic: request.draft.enabled && request.draft.memoryWritebackEnabled ? "ready" : "disabled_by_config",
          aiToolsEnabled: request.draft.aiToolsEnabled,
          precheckEnabled: request.draft.precheckEnabled,
          memoryWritebackEnabled: request.draft.memoryWritebackEnabled,
          implicitMemoryTurns: Number(request.draft.implicitMemoryTurns) || snapshot.vmmMemory.effective.implicitMemoryTurns,
          profileRefreshTurns: Number(request.draft.profileRefreshTurns) || snapshot.vmmMemory.effective.profileRefreshTurns,
          profileNodeLimit: Number(request.draft.profileNodeLimit) || snapshot.vmmMemory.effective.profileNodeLimit,
          injectionTag: request.draft.injectionTag,
          injectionDescription: request.draft.injectionDescription,
        };
      }
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_vmm_memory_config", { request });
}

// browserPreviewProjectVmmDraft resolves one exact project draft without borrowing another project's override.
// browserPreviewProjectVmmDraft 解析一个准确项目草稿，不借用其他项目的覆盖配置。
// Parameters: snapshot owns the preview project catalog and projectId selects the requested draft.
// 参数：snapshot 持有预览项目目录，projectId 选择请求草稿。
// Returns: an exact saved override or a project inheritance draft derived from global configuration.
// 返回值：准确已保存覆盖配置，或从全局配置派生的项目继承草稿。
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
  // savedOverride is an exact project draft created during this browser-only session.
  // savedOverride 是当前纯浏览器会话期间创建的准确项目草稿。
  const savedOverride = browserPreviewProjectVmmOverrides.get(projectId);
  if (savedOverride?.hasOverride) {
    return { ...savedOverride };
  }
  if (snapshot.vmmMemory.selectedProjectId === projectId && snapshot.vmmMemory.projectDraft.hasOverride) {
    return { ...snapshot.vmmMemory.projectDraft };
  }
  throw new Error("Preview mode does not have the requested project VMM override payload.");
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
        { profileNodeId: 1, content: "User prefers engineering closure.", priority: "high", level: "user", bindId: 20000 },
        { profileNodeId: 2, content: "The current project is migrating the WebUI.", priority: "normal", level: "project", bindId: 1001 },
      ],
    };
  }
  if (action === "search_memory_events") {
    return {
      results: [{ query: "webui", hits: [{ memoryId: 1, sourceTurnId: 42, abstractText: "The skills page migrated to WebUI." }] }],
      traceId: "preview-trace",
    };
  }
  if (action === "delete_memories") {
    return {
      deletedMemoryIds: [1],
      notFoundMemoryIds: [],
      deletedVectorRows: 1,
      traceId: "preview-delete-trace",
    };
  }
  return { ok: true, preview: true };
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
