import type {
  ConversationMessage,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";
import type {
  DiffReviewSelection,
  WorkspaceDiffFileDetailRequest,
  WorkspaceDiffFileDetailResponse,
  WorkspaceDiffSearchRequest,
  WorkspaceDiffSearchResponse,
  WorkspaceDiffSnapshotResponse,
} from "@/diff/types";
import {
  browserFallbackSnapshot,
  browserFallbackStartup,
  loadBrowserFallbackConversationMessages,
} from "@/service/browser-fallback-workbench";
import {
  loadBrowserFallbackWorkspaceDiffFileDetail,
  loadBrowserFallbackWorkspaceDiffSearch,
  loadBrowserFallbackWorkspaceDiffSnapshot,
} from "@/service/browser-fallback-diff";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

// WorkspaceDiffFileDetailLoadOptions controls one lazy file detail request.
// WorkspaceDiffFileDetailLoadOptions 控制单次懒加载文件详情请求。
export interface WorkspaceDiffFileDetailLoadOptions {
  // signal aborts the backend detail request when the caller times out or unmounts.
  // signal 在调用方超时或卸载时中止后端详情请求。
  signal?: AbortSignal;
}

// loadStartupSnapshot loads startup state from the local service or fallback data.
// loadStartupSnapshot 从本地服务或回退数据加载启动状态。
export async function loadStartupSnapshot(): Promise<StartupSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackStartup;
  }
  return serviceClient.request<StartupSnapshot>("startup.snapshot");
}

// loadWorkbenchSnapshot loads workbench state from the local service or fallback data.
// loadWorkbenchSnapshot 从本地服务或回退数据加载工作台状态。
export async function loadWorkbenchSnapshot(): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot();
  }
  return serviceClient.request<WorkbenchSnapshot>("workbench.snapshot");
}

// loadWorkspaceDiffSnapshot loads a fresh Git diff snapshot for one project path.
// loadWorkspaceDiffSnapshot 为单个项目路径加载最新 Git diff 快照。
export async function loadWorkspaceDiffSnapshot(
  workspacePath: string,
  projectId: string | null,
  conversationId: string | null,
  request: DiffReviewSelection,
): Promise<WorkspaceDiffSnapshotResponse> {
  if (!hasServiceRuntime()) {
    return loadBrowserFallbackWorkspaceDiffSnapshot(workspacePath, request);
  }
  return serviceClient.request<WorkspaceDiffSnapshotResponse>("workspace.diff.snapshot", {
    workspacePath,
    projectId,
    conversationId,
    request,
  });
}

// loadWorkspaceDiffFileDetail loads a full Git diff for one summarized file.
// loadWorkspaceDiffFileDetail 为单个摘要文件加载完整 Git diff。
export async function loadWorkspaceDiffFileDetail(
  workspacePath: string,
  projectId: string | null,
  conversationId: string | null,
  request: DiffReviewSelection,
  file: WorkspaceDiffFileDetailRequest,
  options?: WorkspaceDiffFileDetailLoadOptions,
): Promise<WorkspaceDiffFileDetailResponse> {
  if (!hasServiceRuntime()) {
    return loadBrowserFallbackWorkspaceDiffFileDetail(workspacePath, request, file, options);
  }
  return serviceClient.request<WorkspaceDiffFileDetailResponse>("workspace.diff.fileDetail", {
    workspacePath,
    projectId,
    conversationId,
    request,
    file,
  }, {
    signal: options?.signal,
  });
}

// loadWorkspaceDiffSearch loads compact backend file-level search hits for one diff.
// loadWorkspaceDiffSearch 为单个 diff 加载紧凑的后端文件级搜索命中。
export async function loadWorkspaceDiffSearch(
  workspacePath: string,
  projectId: string | null,
  conversationId: string | null,
  request: DiffReviewSelection,
  search: WorkspaceDiffSearchRequest,
): Promise<WorkspaceDiffSearchResponse> {
  if (!hasServiceRuntime()) {
    return loadBrowserFallbackWorkspaceDiffSearch(workspacePath, request, search);
  }
  return serviceClient.request<WorkspaceDiffSearchResponse>("workspace.diff.search", {
    workspacePath,
    projectId,
    conversationId,
    request,
    search,
  });
}

// loadConversationMessages loads one conversation body from the local service.
// loadConversationMessages 从本地服务加载单个会话正文。
export async function loadConversationMessages(sessionId: string): Promise<ConversationMessage[]> {
  if (!hasServiceRuntime()) {
    return loadBrowserFallbackConversationMessages(sessionId);
  }
  return serviceClient.request<ConversationMessage[]>("service_session_messages", { sessionId });
}
