import type {
  ConversationMessage,
  ConversationHistoryPageResponse,
  ConversationMessagesLoadResponse,
  StartupSnapshot,
  WorkspaceFileDirectoryListing,
  WorkspaceFileFilterOptions,
  WorkspaceFileReadResponse,
  WorkbenchSnapshot,
} from "@/types";
import type {
  DiffReviewSelection,
  WorkspaceGitFileActionRequest,
  WorkspaceGitFileActionResult,
  WorkspaceDiffFileDetailRequest,
  WorkspaceDiffFileDetailResponse,
  WorkspaceDiffSearchRequest,
  WorkspaceDiffSearchResponse,
  WorkspaceDiffSnapshotResponse,
  WorkspaceGitRepositoryCatalog,
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

// conversationMessagesRequestBySession shares only active exact-session body loads across component lifetimes.
// conversationMessagesRequestBySession 仅在组件生命周期之间共享仍在途的精确会话正文加载。
const conversationMessagesRequestBySession = new Map<string, Promise<ConversationMessagesLoadResponse>>();

// WorkspaceDiffFileDetailLoadOptions controls one lazy file detail request.
// WorkspaceDiffFileDetailLoadOptions 控制单次懒加载文件详情请求。
export interface WorkspaceDiffFileDetailLoadOptions {
  // signal aborts the backend detail request when the caller times out or unmounts.
  // signal 在调用方超时或卸载时中止后端详情请求。
  signal?: AbortSignal;
}

// DEFAULT_WORKSPACE_FILE_FILTERS mirrors the backend's safe file tree defaults.
// DEFAULT_WORKSPACE_FILE_FILTERS 对齐后端的安全文件树默认值。
export const DEFAULT_WORKSPACE_FILE_FILTERS: WorkspaceFileFilterOptions = {
  ignoreHidden: true,
  ignoreDotNames: true,
  useIgnoreFiles: true,
  useGitignore: true,
  filterNonPreviewFiles: false,
  allowImages: false,
  allowVideos: false,
};

// loadStartupSnapshot loads startup state from the local service or fallback data.
// loadStartupSnapshot 从本地服务或回退数据加载启动状态。
export async function loadStartupSnapshot(): Promise<StartupSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackStartup;
  }
  return serviceClient.request<StartupSnapshot>("startup.snapshot");
}

// rebuildStartupSessionVmm discards only the service-classified session VMM artifacts and retries startup.
// rebuildStartupSessionVmm 仅丢弃服务端已分类的会话 VMM 工件并重试启动。
export async function rebuildStartupSessionVmm(): Promise<StartupSnapshot> {
  return serviceClient.request<StartupSnapshot>("startup.rebuildSessionVmm");
}

// loadWorkbenchSnapshot loads workbench state from the local service or fallback data.
// loadWorkbenchSnapshot 从本地服务或回退数据加载工作台状态。
export async function loadWorkbenchSnapshot(signal?: AbortSignal): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot();
  }
  return serviceClient.request<WorkbenchSnapshot>("workbench.snapshot", undefined, { signal });
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

// loadWorkspaceGitCatalog discovers the project repositories used by review nodes.
// loadWorkspaceGitCatalog 发现审核节点使用的项目仓库。
export async function loadWorkspaceGitCatalog(
  workspacePath: string,
  projectId: string | null,
  conversationId: string | null,
): Promise<WorkspaceGitRepositoryCatalog> {
  if (!hasServiceRuntime()) {
    return { projectRoot: workspacePath, repositories: [] };
  }
  return serviceClient.request<WorkspaceGitRepositoryCatalog>("workspace.git.catalog", {
    workspacePath,
    projectId,
    conversationId,
  });
}

// setWorkspaceGitMonitoring persists one repository's live-monitoring switch.
// setWorkspaceGitMonitoring 持久化单个仓库的实时监听开关。
export async function setWorkspaceGitMonitoring(
  workspacePath: string,
  projectId: string | null,
  conversationId: string | null,
  repositoryKey: string,
  enabled: boolean,
): Promise<WorkspaceGitRepositoryCatalog> {
  if (!hasServiceRuntime()) {
    return { projectRoot: workspacePath, repositories: [] };
  }
  return serviceClient.request<WorkspaceGitRepositoryCatalog>("workspace.git.setMonitoring", {
    workspacePath,
    projectId,
    conversationId,
    request: { repositoryKey, enabled },
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

// runWorkspaceGitFileAction executes a file-level Git mutation for one workspace.
// runWorkspaceGitFileAction 为单个工作区执行一次文件级 Git 变更操作。
export async function runWorkspaceGitFileAction(
  workspacePath: string,
  projectId: string | null,
  conversationId: string | null,
  request: WorkspaceGitFileActionRequest,
): Promise<WorkspaceGitFileActionResult> {
  if (!hasServiceRuntime()) {
    return {
      repositoryRoot: workspacePath,
      action: request.action,
      affectedPathCount: request.files?.length ?? 0,
      summary: "Browser preview mode does not modify the Git index.",
    };
  }
  return serviceClient.request<WorkspaceGitFileActionResult>("workspace.git.fileAction", {
    workspacePath,
    projectId,
    conversationId,
    request,
  });
}

// loadWorkspaceFileDirectory loads one lazy workspace file tree directory.
// loadWorkspaceFileDirectory 加载单个懒加载工作区文件树目录。
export async function loadWorkspaceFileDirectory(
  workspacePath: string,
  projectId: string | null,
  conversationId: string | null,
  directoryPath: string | null,
  filters: WorkspaceFileFilterOptions,
): Promise<WorkspaceFileDirectoryListing> {
  if (!hasServiceRuntime()) {
    return loadBrowserFallbackWorkspaceFileDirectory(workspacePath, directoryPath, filters);
  }
  return serviceClient.request<WorkspaceFileDirectoryListing>("workspace.files.list", {
    workspacePath,
    projectId,
    conversationId,
    directoryPath,
    filters,
  });
}

// loadWorkspaceFilePreview loads one workspace file preview payload.
// loadWorkspaceFilePreview 加载单个工作区文件预览载荷。
export async function loadWorkspaceFilePreview(
  workspacePath: string,
  projectId: string | null,
  conversationId: string | null,
  filePath: string,
  filters: WorkspaceFileFilterOptions,
): Promise<WorkspaceFileReadResponse> {
  if (!hasServiceRuntime()) {
    return loadBrowserFallbackWorkspaceFilePreview(workspacePath, filePath, filters);
  }
  return serviceClient.request<WorkspaceFileReadResponse>("workspace.files.read", {
    workspacePath,
    projectId,
    conversationId,
    filePath,
    filters,
  });
}

// loadConversationMessages loads one conversation body from the local service.
// loadConversationMessages 从本地服务加载单个会话正文。
export async function loadConversationMessages(sessionId: string): Promise<ConversationMessagesLoadResponse> {
  if (!hasServiceRuntime()) {
    const progress = {
      sessionId,
      status: "ready" as const,
      phase: "done",
      message: "Conversation loaded.",
      percent: 100,
    };
    const messages = loadBrowserFallbackConversationMessages(sessionId);
    const workbench = {
      ...browserFallbackSnapshot(),
      messages,
      conversationHydration: progress,
    };
    return {
      workbench,
      messages,
      earliestOrdinal: null,
      hasMoreHistory: false,
      progress,
    };
  }
  // retainedRequest prevents a real component remount from issuing a second physical access while the first is unresolved.
  // retainedRequest 防止组件真实重挂载时在首个请求未完成前再次发出物理访问。
  const retainedRequest = conversationMessagesRequestBySession.get(sessionId);
  if (retainedRequest) {
    return retainedRequest;
  }
  // request is retained only until settlement, so later explicit access still reaches the service normally.
  // request 仅保留到完成或失败，之后的显式访问仍会正常到达服务端。
  const request = serviceClient
    .request<ConversationMessagesLoadResponse>("service_session_messages", { sessionId })
    .finally(() => {
      if (conversationMessagesRequestBySession.get(sessionId) === request) {
        conversationMessagesRequestBySession.delete(sessionId);
      }
    });
  conversationMessagesRequestBySession.set(sessionId, request);
  return request;
}

// loadConversationHistoryPage loads older turns before the current conversation window.
// loadConversationHistoryPage 加载当前会话窗口之前的更早轮次。
export async function loadConversationHistoryPage(
  sessionId: string,
  beforeOrdinal: number,
): Promise<ConversationHistoryPageResponse> {
  if (!hasServiceRuntime()) {
    return {
      sessionId,
      workbench: browserFallbackSnapshot(),
      messages: [],
      earliestOrdinal: null,
      hasMoreHistory: false,
    };
  }
  return serviceClient.request<ConversationHistoryPageResponse>("service_session_history_page", {
    sessionId,
    beforeOrdinal,
  });
}

// loadBrowserFallbackWorkspaceFileDirectory returns a compact demo tree for browser-only previews.
// loadBrowserFallbackWorkspaceFileDirectory 为仅浏览器预览返回紧凑演示树。
function loadBrowserFallbackWorkspaceFileDirectory(
  workspacePath: string,
  directoryPath: string | null,
  filters: WorkspaceFileFilterOptions,
): WorkspaceFileDirectoryListing {
  const rootPath = workspacePath || "/preview";
  const path = directoryPath || rootPath;
  const relativePath = path === rootPath ? "" : path.replace(`${rootPath}/`, "");
  const entries = relativePath
    ? [
        {
          name: "example.ts",
          path: `${path}/example.ts`,
          relativePath: `${relativePath}/example.ts`,
          kind: "code" as const,
          isDirectory: false,
          extension: "ts",
          mimeType: "text/plain",
          sizeBytes: 128,
          modifiedAt: null,
          hidden: false,
          hasChildren: false,
        },
      ]
    : [
        {
          name: "src",
          path: `${rootPath}/src`,
          relativePath: "src",
          kind: "directory" as const,
          isDirectory: true,
          extension: null,
          mimeType: null,
          sizeBytes: null,
          modifiedAt: null,
          hidden: false,
          hasChildren: true,
        },
        {
          name: "README.md",
          path: `${rootPath}/README.md`,
          relativePath: "README.md",
          kind: "code" as const,
          isDirectory: false,
          extension: "md",
          mimeType: "text/markdown",
          sizeBytes: 256,
          modifiedAt: null,
          hidden: false,
          hasChildren: false,
        },
      ];
  return {
    workspacePath: rootPath,
    path,
    relativePath,
    parentPath: relativePath ? rootPath : null,
    entries,
    filters,
    truncated: false,
    entryCount: entries.length,
    notice: "Browser preview mode is using a static file-tree sample.",
  };
}

// loadBrowserFallbackWorkspaceFilePreview returns a static code preview for browser-only previews.
// loadBrowserFallbackWorkspaceFilePreview 为仅浏览器预览返回静态代码预览。
function loadBrowserFallbackWorkspaceFilePreview(
  workspacePath: string,
  filePath: string,
  _filters: WorkspaceFileFilterOptions,
): WorkspaceFileReadResponse {
  const rootPath = workspacePath || "/preview";
  const relativePath = filePath.replace(`${rootPath}/`, "");
  return {
    workspacePath: rootPath,
    path: filePath,
    relativePath,
    kind: "code",
    mimeType: "text/plain",
    sizeBytes: 64,
    content: `// Browser preview file\n${relativePath || "README.md"}\n`,
    encoding: "utf-8",
    lineCount: 2,
    truncated: false,
  };
}
