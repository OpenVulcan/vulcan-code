import React from "react";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type { WorkspaceGitRepositoryCatalog } from "@/diff/types";

// Announces which project needs to reload its folder associations.
// 通知哪个项目需要重新加载目录关联。
const FOLDERS_CHANGED = "vulcan:project-attached-folders-changed";

// Owns one loaded project's folder list or its service error.
// 保存单个已加载项目的目录列表或服务错误。
interface AttachedFolderSnapshot {
  // Project whose associations were read.
  // 已读取关联的项目。
  projectId: string;
  // Current associations, or null when the read failed.
  // 当前关联，读取失败时为 null。
  paths: readonly string[] | null;
  // Service diagnostic from a failed read.
  // 读取失败时的服务诊断。
  error: string | null;
}

// Loads the saved folders for projectId from the current service.
// 从当前服务加载 projectId 已保存的目录。
export function loadProjectAttachedFolders(projectId: string): Promise<string[]> {
  // The browser-only preview has no persisted host directory associations.
  // 仅浏览器预览模式没有已持久化的宿主目录关联。
  if (!hasServiceRuntime()) return Promise.resolve([]);
  return serviceClient.request<string[]>("project.attachedFolders.list", { projectId });
}

// Updates the association between projectId and path, then invalidates its consumers.
// 更新 projectId 与 path 的关联，再通知使用方刷新。
export async function setProjectAttachedFolder(projectId: string, path: string, attached: boolean): Promise<void> {
  await serviceClient.request<void>("project.attachedFolders.set", { projectId, path, attached });
  window.dispatchEvent(new CustomEvent(FOLDERS_CHANGED, { detail: projectId }));
}

// Calls change with the exact project ID of each acknowledged mutation and returns an unsubscribe function.
// 每次变更被确认后用确切的项目标识调用 change，并返回取消订阅函数。
export function subscribeProjectAttachedFolderChanges(change: (projectId: string) => void): () => void {
  // Reads the mutation's project identity from its typed event.
  // 从类型化事件读取变更所属的项目。
  const receive = (event: Event) => change((event as CustomEvent<string>).detail);
  window.addEventListener(FOLDERS_CHANGED, receive);
  return () => window.removeEventListener(FOLDERS_CHANGED, receive);
}

// Returns the existing Git discovery catalog for path registered to projectId.
// 返回 projectId 已登记路径 path 的现有 Git 发现目录。
export function detectProjectAttachedFolderGit(projectId: string, path: string): Promise<WorkspaceGitRepositoryCatalog> {
  return serviceClient.request<WorkspaceGitRepositoryCatalog>("project.attachedFolders.git", { projectId, path });
}

// Loads the visible project's folders and accepts updates only for that same project.
// 加载可见项目的目录，并只接收该项目的更新。
// Returns null until projectId has a completed read or a saved update.
// 在 projectId 的读取完成或收到持久化更新前返回 null。
export function useProjectAttachedFolders(projectId: string | null): AttachedFolderSnapshot | null {
  // Tag retained results so a project switch cannot display the previous project's folders.
  // 标记保留结果的归属，避免切换项目时展示前一个项目的目录。
  const [snapshot, setSnapshot] = React.useState<AttachedFolderSnapshot | null>(null);
  React.useEffect(() => {
    if (!projectId) return;
    // The revision rejects a slow initial read if a mutation has since published newer data.
    // 如果变更已经发布更新的数据，版本号会拒绝迟到的初次读取。
    let revision = 0;
    // Read the active project's latest list and discard responses from a superseded read.
    // 读取活动项目的最新列表，并丢弃被替代的读取结果。
    const load = () => {
      // Each request captures the revision it can update.
      // 每次请求记录自己可以更新的版本。
      const request = ++revision;
      void loadProjectAttachedFolders(projectId).then(
        (paths) => { if (request === revision) setSnapshot({ projectId, paths, error: null }); },
        (error: unknown) => { if (request === revision) setSnapshot({ projectId, paths: null, error: error instanceof Error ? error.message : String(error) }); },
      );
    };
    // Reload after an acknowledged mutation so concurrent responses cannot replace a newer list.
    // 变更确认后重新加载，避免并发返回的响应覆盖较新的列表。
    const unsubscribe = subscribeProjectAttachedFolderChanges((changedProjectId) => {
      if (changedProjectId === projectId) load();
    });
    load();
    window.addEventListener("focus", load);
    return () => {
      revision += 1;
      unsubscribe();
      window.removeEventListener("focus", load);
    };
  }, [projectId]);
  return snapshot?.projectId === projectId ? snapshot : null;
}
