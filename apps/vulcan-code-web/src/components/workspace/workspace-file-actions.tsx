import React from "react";
import { readElectronLocalFileBridge } from "@/host-shell/electron-bridge";
import type { WorkspaceFileLines, WorkspaceFileOpener } from "@/components/workspace/workspace-file-open-request";

// WorkspaceFileAction is what to do with a file the side panel does not show: "prompt" asks whether to open it with
// the default program, "open" opens it at once and "reveal" shows it in its folder. Runnable files are never opened;
// the dialog then offers only their folder.
// WorkspaceFileAction 是对侧边栏不显示的文件执行的操作："prompt" 询问是否用默认程序打开，"open" 直接打开，"reveal"
// 在所在文件夹中显示。可运行文件永不打开，此时对话框只提供其所在文件夹。
export type WorkspaceFileAction = "prompt" | "open" | "reveal";

// WorkspaceFileActionRequest is one pending action on an absolute file path.
// WorkspaceFileActionRequest 是针对单个绝对文件路径的待处理操作。
export interface WorkspaceFileActionRequest {
  // Distinguishes repeated requests for the same file.
  // 区分针对同一文件的重复请求。
  id: number;
  // Absolute path of the file.
  // 文件绝对路径。
  path: string;
  // Requested action.
  // 请求的操作。
  action: WorkspaceFileAction;
}

// WORKSPACE_FILE_ACTION_EVENT tells the mounted host that an action is waiting.
// WORKSPACE_FILE_ACTION_EVENT 通知已挂载的宿主有待处理的操作。
const WORKSPACE_FILE_ACTION_EVENT = "vulcan:workspace-file-action";
let pendingAction: WorkspaceFileActionRequest | null = null;
let lastActionId = 0;

// requestWorkspaceFileAction asks the workbench to run `action` on the file at absolute `path`.
// requestWorkspaceFileAction 请求工作台对绝对路径 `path` 处的文件执行 `action`。
export function requestWorkspaceFileAction(path: string, action: WorkspaceFileAction): void {
  pendingAction = { id: ++lastActionId, path, action };
  window.dispatchEvent(new CustomEvent(WORKSPACE_FILE_ACTION_EVENT));
}

// localFileActionsAvailable tells whether this client can hand files to the OS, which only the desktop can.
// localFileActionsAvailable 判断当前客户端能否将文件交给操作系统；只有桌面端可以。
export function localFileActionsAvailable(): boolean {
  return readElectronLocalFileBridge() !== null;
}

// openFileReference activates a file reference with `open`, by click `event` or, without one, by keyboard. On the
// desktop a Ctrl-click (Cmd-click on macOS) opens the file with its default program; other activations, and every
// one on the web client, show the file in the side panel.
// openFileReference 通过 `open` 激活文件引用：带点击 `event` 时按点击处理，没有时按键盘激活处理。桌面端的 Ctrl 点击
// （macOS 为 Cmd 点击）用默认程序打开文件；其他激活方式以及网页客户端的所有激活都在侧边栏显示文件。
export function openFileReference(
  open: WorkspaceFileOpener,
  path: string,
  lines: WorkspaceFileLines | null,
  event?: { ctrlKey: boolean; metaKey: boolean },
): void {
  if ((event?.ctrlKey || event?.metaKey) && localFileActionsAvailable()) open(path, lines, "open");
  else open(path, lines);
}

// fileReferenceData marks an element as a reference to `path` at `lines`, so the conversation menu can offer file
// actions for it.
// fileReferenceData 将元素标记为指向 `path`（及 `lines`）的文件引用，使会话菜单可为其提供文件操作。
export function fileReferenceData(path: string, lines: WorkspaceFileLines | null): Record<string, string | undefined> {
  return { "data-file-path": path, "data-file-lines": lines ? `${lines.start}-${lines.end}` : undefined };
}

// readFileReference returns the file reference that `target` belongs to, or null.
// readFileReference 返回 `target` 所属的文件引用；不属于任何文件引用时返回 null。
export function readFileReference(target: Element): { path: string; lines: WorkspaceFileLines | null } | null {
  const element = target.closest<HTMLElement>("[data-file-path]");
  const path = element?.dataset.filePath;
  if (!element || !path) return null;
  const range = /^(\d+)-(\d+)$/.exec(element.dataset.fileLines ?? "");
  return { path, lines: range ? { start: Number(range[1]), end: Number(range[2]) } : null };
}

// The dialog and its OS calls load with the first request.
// 对话框及其操作系统调用随首个请求加载。
const LazyWorkspaceFileActionDialog = React.lazy(() => import("@/components/workspace/workspace-file-action-dialog"));

// WorkspaceFileActionHost runs file action requests for the whole workbench, one at a time.
// WorkspaceFileActionHost 为整个工作台逐个执行文件操作请求。
export function WorkspaceFileActionHost() {
  const [request, setRequest] = React.useState<WorkspaceFileActionRequest | null>(null);
  React.useEffect(() => {
    const take = () => {
      if (pendingAction) setRequest(pendingAction);
      pendingAction = null;
    };
    take();
    window.addEventListener(WORKSPACE_FILE_ACTION_EVENT, take);
    return () => window.removeEventListener(WORKSPACE_FILE_ACTION_EVENT, take);
  }, []);
  const done = React.useCallback(() => setRequest(null), []);
  return request ? (
    <React.Suspense fallback={null}>
      <LazyWorkspaceFileActionDialog key={request.id} onDone={done} request={request} />
    </React.Suspense>
  ) : null;
}
