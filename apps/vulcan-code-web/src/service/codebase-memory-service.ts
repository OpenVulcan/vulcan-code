import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type { CodebaseMemoryProjectSettingsSummary } from "@/types";

// loadProjectCodebaseMemorySettings loads project-owned enablement and live diagnostics.
// loadProjectCodebaseMemorySettings 加载项目自有启用状态与实时诊断。
export async function loadProjectCodebaseMemorySettings(
  projectId: string,
): Promise<CodebaseMemoryProjectSettingsSummary> {
  if (!hasServiceRuntime()) {
    const project = browserFallbackSnapshot().projects.find((candidate) => candidate.id === projectId);
    if (!project) {
      throw new Error("Preview project does not exist.");
    }
    return {
      projectId,
      enabled: Boolean(project.codebaseMemoryEnabled),
      serviceStatus: "preview",
      availability: null,
      watcherRegistered: null,
      lastSuccessfulIndexAt: null,
      indexRevision: null,
      nodeCount: null,
      edgeCount: null,
      activeJob: null,
      lastJob: null,
      lastError: null,
    };
  }
  return serviceClient.request<CodebaseMemoryProjectSettingsSummary>(
    "service_get_project_codebase_memory_settings",
    { projectId },
  );
}

// setProjectCodebaseMemoryEnabled persists one exact project-owned enablement state.
// setProjectCodebaseMemoryEnabled 持久化一个精确项目自有启用状态。
export async function setProjectCodebaseMemoryEnabled(
  projectId: string,
  enabled: boolean,
): Promise<CodebaseMemoryProjectSettingsSummary> {
  if (!hasServiceRuntime()) {
    return {
      projectId,
      enabled,
      serviceStatus: "preview",
      availability: null,
      watcherRegistered: null,
      lastSuccessfulIndexAt: null,
      indexRevision: null,
      nodeCount: null,
      edgeCount: null,
      activeJob: null,
      lastJob: null,
      lastError: null,
    };
  }
  return serviceClient.request<CodebaseMemoryProjectSettingsSummary>(
    "service_set_project_codebase_memory_enabled",
    { projectId, enabled },
  );
}

// refreshProjectCodebaseMemorySettings queries the live Sidecar status for one enabled project.
// refreshProjectCodebaseMemorySettings 查询一个已启用项目的实时 Sidecar 状态。
export async function refreshProjectCodebaseMemorySettings(
  projectId: string,
): Promise<CodebaseMemoryProjectSettingsSummary> {
  if (!hasServiceRuntime()) {
    return loadProjectCodebaseMemorySettings(projectId);
  }
  return serviceClient.request<CodebaseMemoryProjectSettingsSummary>(
    "service_refresh_project_codebase_memory_settings",
    { projectId },
  );
}

// updateProjectCodebaseMemoryIndex schedules one exact incremental update or isolated rebuild.
// updateProjectCodebaseMemoryIndex 调度一次精确的增量更新或隔离重建。
export async function updateProjectCodebaseMemoryIndex(
  projectId: string,
  mode: "update" | "rebuild",
): Promise<CodebaseMemoryProjectSettingsSummary> {
  if (!hasServiceRuntime()) {
    return loadProjectCodebaseMemorySettings(projectId);
  }
  return serviceClient.request<CodebaseMemoryProjectSettingsSummary>(
    "service_update_project_codebase_memory_index",
    { projectId, mode },
  );
}
