import type {
  ConversationMessage,
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

// DEFAULT_WORKSPACE_FILE_FILTERS mirrors the backend's safe file tree defaults.
// DEFAULT_WORKSPACE_FILE_FILTERS 对齐后端的安全文件树默认值。
export const DEFAULT_WORKSPACE_FILE_FILTERS: WorkspaceFileFilterOptions = {
  ignoreHidden: true,
  ignoreDotNames: true,
  useIgnoreFiles: true,
  useGitignore: true,
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
      summary: "浏览器预览模式不会修改 Git 暂存区。",
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
export async function loadConversationMessages(sessionId: string): Promise<ConversationMessage[]> {
  if (!hasServiceRuntime()) {
    return loadBrowserFallbackConversationMessages(sessionId);
  }
  return serviceClient.request<ConversationMessage[]>("service_session_messages", { sessionId });
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
    notice: "浏览器预览模式使用静态文件树示例。",
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
    content: `// Browser preview file\n// 浏览器预览文件\n${relativePath || "README.md"}\n`,
    encoding: "utf-8",
    lineCount: 3,
    truncated: false,
  };
}
