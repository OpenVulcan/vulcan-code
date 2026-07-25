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
      indexStatus: null,
      lastSuccessfulIndexAt: null,
      indexRevision: null,
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
      indexStatus: null,
      lastSuccessfulIndexAt: null,
      indexRevision: null,
      lastError: null,
    };
  }
  return serviceClient.request<CodebaseMemoryProjectSettingsSummary>(
    "service_set_project_codebase_memory_enabled",
    { projectId, enabled },
  );
}

// reindexProjectCodebaseMemory requests a non-destructive rebuild for one enabled project.
// reindexProjectCodebaseMemory 为一个已启用项目请求非破坏性重建。
export async function reindexProjectCodebaseMemory(
  projectId: string,
): Promise<CodebaseMemoryProjectSettingsSummary> {
  if (!hasServiceRuntime()) {
    return loadProjectCodebaseMemorySettings(projectId);
  }
  return serviceClient.request<CodebaseMemoryProjectSettingsSummary>(
    "service_reindex_project_codebase_memory",
    { projectId },
  );
}
