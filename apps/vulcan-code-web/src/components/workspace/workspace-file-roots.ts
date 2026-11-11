import React from "react";
import { useProjectAttachedFolders } from "@/components/project/project-attached-folders-state";

// Holds the available attached paths and root selection owned by a conversation's project.
// 保存会话所属项目可用的附属路径与根目录选择。
export interface WorkspaceFileRoots {
  // The project that owns the supplied paths.
  // 所提供路径的所属项目。
  projectId: string;
  // Registered attached roots for this project.
  // 此项目已登记的附属根目录。
  attachedPaths: readonly string[];
  // Attached root selected by each open files tab; an absent entry uses the primary root.
  // 每个已打开文件页签选择的附属根目录；没有条目时使用主目录。
  selectedPaths: ReadonlyMap<string, string>;
  // Sets the selected root for tabId; null selects the primary root.
  // 设置 tabId 的根目录；null 表示选择主目录。
  select: (tabId: string, path: string | null) => void;
}

// Shares a single active project's file-root selection with right-panel tabs.
// 与右侧文件页签共享单个活动项目的文件根目录选择。
export const WorkspaceFileRootsContext = React.createContext<WorkspaceFileRoots | null>(null);

// Loads projectId's registered folders and tracks each tab's selection within the conversation owner.
// 加载 projectId 已登记的目录，并在会话 owner 范围内跟踪各页签的选择。
// Returns the loaded list, the scoped panel context and a tab selection callback.
// 返回已加载的列表、限定作用域的面板上下文以及页签选择回调。
export function useWorkspaceFileRoots(projectId: string | null, owner: string) {
  // The snapshot is unavailable until the current project's association read completes.
  // 当前项目的关联读取完成前，快照不可用。
  const attachedFolders = useProjectAttachedFolders(projectId);
  // Retain choices per tab while this conversation owns the context rail.
  // 在此会话持有右侧栏时按页签保留选择。
  const [fileRoots, setFileRoots] = React.useState<{ owner: string; paths: ReadonlyMap<string, string> } | null>(null);
  // Sets path for tabId; null restores the primary root.
  // 为 tabId 设置 path；null 恢复主目录。
  const selectFileRoot = React.useCallback((tabId: string, path: string | null) => {
    setFileRoots((current) => {
      // A new conversation starts with its own empty tab selection.
      // 新会话从自身的空页签选择开始。
      const paths = new Map(current?.owner === owner ? current.paths : []);
      if (path === null) paths.delete(tabId);
      else paths.set(tabId, path);
      return { owner, paths };
    });
  }, [owner]);
  // Offer registered roots only to panels that belong to this project.
  // 仅向此项目所属的面板提供已登记根目录。
  const fileRootContext = React.useMemo<WorkspaceFileRoots | null>(() => projectId && attachedFolders?.paths?.length ? {
    projectId,
    attachedPaths: attachedFolders.paths,
    selectedPaths: fileRoots?.owner === owner ? fileRoots.paths : new Map(),
    select: selectFileRoot,
  } : null, [attachedFolders, fileRoots, owner, projectId, selectFileRoot]);
  return { attachedFolders, fileRootContext, selectFileRoot };
}
