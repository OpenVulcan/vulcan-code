import React from "react";
import { RefreshCw } from "lucide-react";
import { IconButton } from "@/ui-kit/primitives/button";
import { useI18n } from "@/i18n";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type { WorkspaceFilesRequestOwnership } from "@/components/workspace/workspace-files-panel-request-ownership";
import type {
  ConversationRunSettledEvent,
  WorkspaceFileDirectoryListing,
  WorkspaceFileEntrySummary,
  WorkspaceFileReadResponse,
  WorkspaceGitChangedEvent,
} from "@/types";

// WORKSPACE_FILES_AUTO_REFRESH_DELAY_MS coalesces a burst of change events into one tree refresh.
// WORKSPACE_FILES_AUTO_REFRESH_DELAY_MS 将一串变更事件合并为一次目录树刷新。
const WORKSPACE_FILES_AUTO_REFRESH_DELAY_MS = 400;

// WorkspaceFilesRefreshOptions supplies the panel state a refresh reloads and prunes.
// WorkspaceFilesRefreshOptions 提供刷新所重新加载与裁剪的面板状态。
export interface WorkspaceFilesRefreshOptions {
  // Project root the panel lists; null disables refreshing.
  // 面板列出的项目根目录；为 null 时不刷新。
  workspacePath: string | null;
  // Owning project, which scopes run-settled events.
  // 所属项目，用于限定运行结束事件的范围。
  projectId: string | null;
  // Whether the panel state belongs to the current route and filters, so a refresh may write it.
  // 面板状态是否属于当前路由与过滤器，即刷新是否可以写入。
  enabled: boolean;
  // Loaded listings keyed by absolute directory path.
  // 按绝对目录路径保存的已加载目录列表。
  directories: ReadonlyMap<string, WorkspaceFileDirectoryListing>;
  // Loads one directory (null is the root) and resolves with its listing, or undefined when it did not apply.
  // 加载单个目录（null 表示根目录），解析为其列表；未生效时解析为 undefined。
  loadDirectory: (directoryPath: string | null) => Promise<WorkspaceFileDirectoryListing | undefined>;
  // Replaces the loaded listings.
  // 替换已加载的目录列表。
  setDirectories: React.Dispatch<React.SetStateAction<Map<string, WorkspaceFileDirectoryListing>>>;
  // The open file whose shown text a refresh rereads.
  // 刷新时重新读取其所示文本的已打开文件。
  openFile: WorkspaceFilesOpenFile;
}

// WorkspaceFilesOpenFile names the previewed file and how to reread it in place.
// WorkspaceFilesOpenFile 指明正在预览的文件以及原位重新读取它的方式。
export interface WorkspaceFilesOpenFile {
  // The open file, or null when no text preview is shown.
  // 已打开的文件；未显示文本预览时为 null。
  entry: WorkspaceFileEntrySummary | null;
  // The preview on screen.
  // 当前显示的预览。
  preview: WorkspaceFileReadResponse | null;
  // Preview ownership, so choosing or closing a file cancels a reread.
  // 预览所有权，使选择或关闭文件时取消重新读取。
  ownership: WorkspaceFilesRequestOwnership;
  // Reads the file's preview.
  // 读取文件预览。
  load: (entry: WorkspaceFileEntrySummary) => Promise<WorkspaceFileReadResponse>;
  // Shows a new preview.
  // 显示新的预览。
  setPreview: (preview: WorkspaceFileReadResponse) => void;
}

// useWorkspaceFilesRefresh reloads every loaded directory still reachable from the root, keeping expansion,
// drops listings of directories that disappeared, and rereads the open file's text in place. It runs on demand and after a run in the project settles or Git
// reports a change in the workspace, so files created outside the panel show up while it stays open.
// useWorkspaceFilesRefresh 重新加载从根目录仍可到达的每个已加载目录并保留展开状态，丢弃已消失目录的列表，
// 并原位重新读取已打开文件的文本。
// 它按需运行，也会在项目中的运行结束或 Git 报告工作区变化后运行，使面板保持打开时也能显示在面板外新建的文件。
// Returns whether a refresh is running and the function that starts one.
// 返回是否正在刷新以及启动刷新的函数。
export function useWorkspaceFilesRefresh(options: WorkspaceFilesRefreshOptions): {
  refreshing: boolean;
  refresh: () => void;
} {
  const latest = React.useRef(options);
  latest.current = options;
  const [refreshing, setRefreshing] = React.useState(false);
  // runState serializes refreshes: a request during a run queues exactly one more.
  // runState 串行执行刷新：运行期间的请求只排队一次后续刷新。
  const runState = React.useRef({ running: false, queued: false });

  const refresh = React.useCallback(() => {
    const state = runState.current;
    if (state.running) {
      state.queued = true;
      return;
    }
    state.running = true;
    setRefreshing(true);
    void (async () => {
      try {
        do {
          state.queued = false;
          await refreshLoadedDirectories(latest.current);
          await rereadOpenFile(latest.current.openFile);
        } while (state.queued && latest.current.enabled);
      } finally {
        state.running = false;
        setRefreshing(false);
      }
    })().catch(() => undefined);
  }, []);

  const { projectId, workspacePath } = options;
  React.useEffect(() => {
    if (!workspacePath || !hasServiceRuntime()) return undefined;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const schedule = () => {
      if (cancelled || timer !== null) return;
      timer = setTimeout(() => {
        timer = null;
        if (!cancelled && latest.current.enabled) refresh();
      }, WORKSPACE_FILES_AUTO_REFRESH_DELAY_MS);
    };
    const workspaceKey = comparableWorkspacePath(workspacePath);
    const subscriptions = [
      serviceClient.subscribe<ConversationRunSettledEvent>("conversation.run.settled", (event) => {
        if (projectId && event.projectId === projectId) schedule();
      }),
      serviceClient.subscribe<WorkspaceGitChangedEvent>("workspace.git.changed", (event) => {
        if (comparableWorkspacePath(event.workspacePath) === workspaceKey) schedule();
      }),
    ];
    const unsubscribes: Array<() => void> = [];
    for (const subscription of subscriptions) {
      // A failed subscription only loses automatic refreshes; the button still works.
      // 订阅失败只会失去自动刷新，按钮仍然可用。
      void subscription.then((unsubscribe) => {
        if (cancelled) unsubscribe();
        else unsubscribes.push(unsubscribe);
      }).catch(() => undefined);
    }
    return () => {
      cancelled = true;
      if (timer !== null) clearTimeout(timer);
      for (const unsubscribe of unsubscribes) unsubscribe();
    };
  }, [projectId, refresh, workspacePath]);

  return { refreshing, refresh };
}

// refreshLoadedDirectories walks from the root through directories that were loaded, reloading each, then drops
// listings the walk no longer reaches. A directory that did not reload leaves every listing in place.
// refreshLoadedDirectories 从根目录出发遍历已加载的目录并逐一重新加载，然后丢弃遍历不再到达的列表。
// 任一目录未能重新加载时保留全部列表。
async function refreshLoadedDirectories(options: WorkspaceFilesRefreshOptions): Promise<void> {
  if (!options.workspacePath || !options.enabled) return;
  const loaded = new Set(options.directories.keys());
  const reached = new Set<string>();
  let complete = true;
  const visit = async (directoryPath: string | null): Promise<void> => {
    const listing = await options.loadDirectory(directoryPath);
    if (!listing) {
      complete = false;
      return;
    }
    reached.add(listing.path);
    await Promise.all(listing.entries
      .filter((entry) => entry.isDirectory && loaded.has(entry.path))
      .map((entry) => visit(entry.path)));
  };
  await visit(null);
  if (!complete) return;
  options.setDirectories((current) => {
    const stale = [...current.keys()].filter((path) => loaded.has(path) && !reached.has(path));
    if (stale.length === 0) return current;
    const next = new Map(current);
    for (const path of stale) next.delete(path);
    return next;
  });
}

// rereadOpenFile replaces the open file's text preview only when the file changed, so an unchanged file keeps
// its scroll position and nothing flashes. A failed reread keeps what is shown; the preview's retry stays available.
// rereadOpenFile 仅在文件变化时替换已打开文件的文本预览，使未变化的文件保持滚动位置且不闪烁。重新读取失败时保留当前
// 显示内容，预览的重试入口仍然可用。
async function rereadOpenFile(openFile: WorkspaceFilesOpenFile): Promise<void> {
  const { entry, preview } = openFile;
  if (!entry || !preview || typeof preview.content !== "string") return;
  const request = openFile.ownership.beginPreviewRequest();
  if (!request) return;
  try {
    const next = await openFile.load(entry);
    if (!openFile.ownership.ownsPreviewRequest(request)) return;
    if (next.content !== preview.content || next.truncated !== preview.truncated || next.sizeBytes !== preview.sizeBytes) {
      openFile.setPreview(next);
    }
  } catch {
    // Keep the shown preview; a deleted or unreadable file surfaces when the user reopens it.
    // 保留当前预览；文件被删除或不可读时会在用户重新打开时显示。
  } finally {
    openFile.ownership.finishPreviewRequest(request);
  }
}

// comparableWorkspacePath folds separator style, trailing separators and case so host path spellings compare equal.
// comparableWorkspacePath 统一分隔符、末尾分隔符与大小写，使宿主路径的不同写法可以相等比较。
function comparableWorkspacePath(path: string): string {
  return path.replaceAll("\\", "/").replace(/\/+$/, "").toLocaleLowerCase();
}

// WorkspaceFilesRefreshButton reloads the file tree; it spins while a refresh runs.
// WorkspaceFilesRefreshButton 重新加载文件目录树；刷新期间图标旋转。
export function WorkspaceFilesRefreshButton(props: { refreshing: boolean; onRefresh: () => void }) {
  const { t } = useI18n();
  return (
    <IconButton aria-label={t("workspaceFiles.refresh")} disabled={props.refreshing} onClick={props.onRefresh} size="sm" title={t("workspaceFiles.refresh")}>
      <RefreshCw className={props.refreshing ? "vk-spin" : undefined} size={14} />
    </IconButton>
  );
}
