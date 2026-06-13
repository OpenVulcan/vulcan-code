import type {
  ConversationMessage,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";
import type {
  DiffReviewSelection,
  WorkspaceDiffSnapshotResponse,
} from "@/diff/types";
import {
  browserFallbackSnapshot,
  browserFallbackStartup,
  loadBrowserFallbackConversationMessages,
} from "@/service/browser-fallback-workbench";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

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
    return {
      repositoryRoot: workspacePath,
      unifiedDiff: "",
      changedFiles: 0,
      additions: 0,
      deletions: 0,
      notice: "浏览器预览模式没有连接本地 Git diff 服务。",
    };
  }
  return serviceClient.request<WorkspaceDiffSnapshotResponse>("workspace.diff.snapshot", {
    workspacePath,
    projectId,
    conversationId,
    request,
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
