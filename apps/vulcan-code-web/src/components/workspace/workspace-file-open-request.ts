import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { useI18n } from "@/i18n";
import { relativeToWorkspace } from "@/components/workspace/workspace-path-actions";
import type { WorkspaceFileDirectoryListing, WorkspaceFileEntrySummary } from "@/types";

// WORKSPACE_FILE_OPEN_EVENT tells the mounted files panel that a file open request is waiting for it.
// WORKSPACE_FILE_OPEN_EVENT 通知已挂载的文件面板有一个开文件请求在等待它。
const WORKSPACE_FILE_OPEN_EVENT = "vulcan:workspace-file-open";

// WorkspaceFileLines is an inclusive, one-based line range of a file.
// WorkspaceFileLines 是文件中包含两端、从 1 开始的行范围。
export interface WorkspaceFileLines {
  // First line.
  // 首行。
  start: number;
  // Last line.
  // 末行。
  end: number;
}

// WorkspaceFileOpenRequest names the files tab that should show `path`, and optionally the lines to bring into view.
// WorkspaceFileOpenRequest 指明应显示 `path` 的文件页签，以及可选的需要滚动到视野内的行。
type WorkspaceFileOpenRequest = { tabId: string; path: string; lines: WorkspaceFileLines | null };

// WorkspaceFileOpener shows a project file, optionally at some lines, in the files panel.
// WorkspaceFileOpener 在文件面板中显示项目文件，可选定位到指定行。
export type WorkspaceFileOpener = (path: string, lines?: WorkspaceFileLines | null) => void;

// pendingFileOpen holds the latest request until the files panel of that tab takes it; the tab may still be mounting.
// pendingFileOpen 保存最近一次请求，直到对应页签的文件面板取走；该页签可能仍在挂载。
let pendingFileOpen: WorkspaceFileOpenRequest | null = null;

// WorkspaceFileLineReveal is one request to bring lines of an absolute file path into view; id tells requests apart.
// WorkspaceFileLineReveal 是一次将某绝对路径文件的行滚动到视野内的请求；id 用于区分请求。
export interface WorkspaceFileLineReveal {
  // Request identity.
  // 请求标识。
  id: number;
  // Absolute file path.
  // 文件绝对路径。
  path: string;
  // Lines to show.
  // 需要显示的行。
  lines: WorkspaceFileLines;
}

// pendingLineReveal holds the reveal the next preview of its file should apply; lastLineRevealId numbers them.
// pendingLineReveal 保存其文件下一次预览应应用的定位请求；lastLineRevealId 为其编号。
let pendingLineReveal: WorkspaceFileLineReveal | null = null;
let lastLineRevealId = 0;

// peekWorkspaceFileLineReveal returns the waiting reveal for `path` without taking it, so rendering stays pure.
// peekWorkspaceFileLineReveal 返回 `path` 等待中的定位请求但不取走，使渲染保持纯粹。
export function peekWorkspaceFileLineReveal(path: string): WorkspaceFileLineReveal | null {
  return pendingLineReveal?.path === path ? pendingLineReveal : null;
}

// consumeWorkspaceFileLineReveal drops reveal `id` once a preview has applied it; a newer reveal stays.
// consumeWorkspaceFileLineReveal 在预览应用后丢弃定位请求 `id`；更新的请求保持不变。
export function consumeWorkspaceFileLineReveal(id: number): void {
  if (pendingLineReveal?.id === id) pendingLineReveal = null;
}

// WorkspaceFileOpenContext carries the workbench's "show this file in the files panel" action to transcript
// cards; null where no files panel exists, so cards offer no such action there.
// WorkspaceFileOpenContext 将工作台的「在文件面板中显示该文件」动作传给转录卡片；没有文件面板的地方为 null，
// 卡片因此不提供该动作。
export const WorkspaceFileOpenContext = React.createContext<WorkspaceFileOpener | null>(null);

// useWorkspaceFileOpener returns the action that shows a project file in the files panel, or null.
// useWorkspaceFileOpener 返回在文件面板中显示项目文件的动作，没有时返回 null。
export function useWorkspaceFileOpener(): WorkspaceFileOpener | null {
  return React.useContext(WorkspaceFileOpenContext);
}

// publishedOpener is the opener the workbench offers to surfaces rendered outside its provider, such as the
// floating project terminals; publishedOpenerListeners re-render them when it changes.
// publishedOpener 是工作台提供给其 Provider 之外界面（如浮动项目终端）的打开动作；
// publishedOpenerListeners 在其变化时使这些界面重新渲染。
let publishedOpener: WorkspaceFileOpener | null = null;
const publishedOpenerListeners = new Set<() => void>();

// setPublishedOpener replaces the published opener and notifies its readers.
// setPublishedOpener 替换已发布的打开动作并通知读取方。
function setPublishedOpener(opener: WorkspaceFileOpener | null): void {
  publishedOpener = opener;
  for (const listener of publishedOpenerListeners) listener();
}

// usePublishWorkspaceFileOpener offers `opener` to surfaces outside the provider while the caller is mounted.
// usePublishWorkspaceFileOpener 在调用方挂载期间将 `opener` 提供给 Provider 之外的界面。
export function usePublishWorkspaceFileOpener(opener: WorkspaceFileOpener | null): void {
  React.useEffect(() => {
    setPublishedOpener(opener);
    return () => {
      if (publishedOpener === opener) setPublishedOpener(null);
    };
  }, [opener]);
}

// usePublishedWorkspaceFileOpener returns the opener the workbench published, or null when none is offered.
// usePublishedWorkspaceFileOpener 返回工作台发布的打开动作；未提供时返回 null。
export function usePublishedWorkspaceFileOpener(): WorkspaceFileOpener | null {
  return React.useSyncExternalStore((listener) => {
    publishedOpenerListeners.add(listener);
    return () => { publishedOpenerListeners.delete(listener); };
  }, () => publishedOpener);
}

// requestWorkspaceFileOpen asks the files panel of tab `tabId` to open `path` at `lines`, now or once it mounts.
// requestWorkspaceFileOpen 请求 `tabId` 页签的文件面板立即或在挂载后打开 `path` 并定位到 `lines`。
export function requestWorkspaceFileOpen(tabId: string, path: string, lines: WorkspaceFileLines | null = null): void {
  pendingFileOpen = { tabId, path, lines };
  window.dispatchEvent(new CustomEvent(WORKSPACE_FILE_OPEN_EVENT));
}

// WorkspaceFileTarget is a requested file split into its listing directory (null for the project root) and name.
// WorkspaceFileTarget 是拆分为所在列表目录（项目根为 null）与文件名的请求文件。
export interface WorkspaceFileTarget {
  // Absolute directory path in the workspace's own separator style, or null for the root.
  // 使用工作区自身分隔符风格的绝对目录路径；根目录为 null。
  directory: string | null;
  // File name inside that directory.
  // 该目录中的文件名。
  name: string;
}

// resolveWorkspaceFileTarget places an absolute or project-relative path inside workspacePath. Returns null for a
// path outside the project, one that climbs with "..", or one that names the root itself. Drive-letter roots
// compare without case, as Windows paths do.
// resolveWorkspaceFileTarget 将绝对路径或项目相对路径定位到 workspacePath 中。项目之外、含 ".." 或指向根目录本身的路径
// 返回 null。带盘符的根目录按 Windows 路径的习惯不区分大小写比较。
export function resolveWorkspaceFileTarget(workspacePath: string, path: string): WorkspaceFileTarget | null {
  const separator = workspacePath.includes("\\") ? "\\" : "/";
  const root = workspacePath.replace(/[\\/]+$/, "");
  const requested = path.trim();
  const relative = /^(?:[A-Za-z]:)?[\\/]/.test(requested) ? relativeToWorkspace(workspacePath, requested) : requested.replace(/\\/g, "/");
  if (relative === null) return null;
  const segments = relative.split("/").filter((segment) => segment !== "" && segment !== ".");
  const name = segments.pop();
  if (!name || name === ".." || segments.includes("..")) return null;
  return { directory: segments.length > 0 ? [root, ...segments].join(separator) : null, name };
}

// FILE_PATH_MENTION matches inline text naming a file by path: at least one folder separator, a file name with a
// letter-led extension, and an optional ":line" or ":start-end" suffix. URLs, spaces and wildcards do not match.
// FILE_PATH_MENTION 匹配以路径指名文件的行内文本：至少一个目录分隔符、扩展名以字母开头的文件名，以及可选的
// ":行" 或 ":起-止" 后缀。URL、空格与通配符不匹配。
const FILE_PATH_MENTION = /^((?:[A-Za-z]:)?[^\s:*?"<>|]*[\\/][^\s:*?"<>|\\/]+\.[A-Za-z][A-Za-z0-9]{0,9})(?::(\d+)(?:[-–](\d+))?)?$/;

// parseFilePathMention reads a path, and any line range, from inline text such as `src/app.ts:12-20`.
// Returns null for text that does not look like a file path, including text with surrounding whitespace or a line
// break, as fenced code always ends with one.
// parseFilePathMention 从行内文本（例如 `src/app.ts:12-20`）读取路径及可能的行范围；不像文件路径时返回 null，
// 包括带首尾空白或换行的文本（围栏代码总以换行结尾）。
export function parseFilePathMention(text: string): { path: string; lines: WorkspaceFileLines | null } | null {
  const match = FILE_PATH_MENTION.exec(text);
  if (!match) return null;
  const start = match[2] ? Number(match[2]) : null;
  const end = match[3] ? Number(match[3]) : start;
  return { path: match[1], lines: start !== null && end !== null && start > 0 && end >= start ? { start, end } : null };
}

// findRequestedFileEntry picks the file named `name` from a listing, preferring an exact match over one that
// differs only in case.
// findRequestedFileEntry 从目录列表中选出名为 `name` 的文件，精确匹配优先于仅大小写不同的匹配。
export function findRequestedFileEntry(listing: WorkspaceFileDirectoryListing, name: string): WorkspaceFileEntrySummary | null {
  const files = listing.entries.filter((entry) => !entry.isDirectory);
  return files.find((entry) => entry.name === name)
    ?? files.find((entry) => entry.name.toLowerCase() === name.toLowerCase()) ?? null;
}

// useWorkspaceFileOpenRequests lets one files panel take requests addressed to its tab while `ready`: it lists the
// file's directory with `load` and hands the entry to `select`; a path outside the project, or one the listing
// does not show (deleted, or hidden by the panel's filters), is reported instead.
// useWorkspaceFileOpenRequests 让单个文件面板在 `ready` 时接收发给其页签的请求：用 `load` 列出文件所在目录并将条目交给
// `select`；项目之外或目录列表中不存在（已删除或被面板过滤规则隐藏）的路径改为提示。
export function useWorkspaceFileOpenRequests(args: {
  // Tab id this panel serves.
  // 当前面板服务的页签标识。
  tabId: string;
  // Project root, or null when the conversation has none.
  // 项目根目录；会话没有项目时为 null。
  workspacePath: string | null;
  // Whether the panel's state belongs to its current route and filters.
  // 面板状态是否属于当前路由与过滤器。
  ready: boolean;
  // Lists one directory (null for the root) with the panel's filters.
  // 按面板过滤器列出单个目录（根目录为 null）。
  load: (directory: string | null) => Promise<WorkspaceFileDirectoryListing>;
  // Opens one file entry in the panel.
  // 在面板中打开单个文件条目。
  select: (entry: WorkspaceFileEntrySummary) => unknown;
}): void {
  const { t } = useI18n();
  const latest = React.useRef(args);
  latest.current = args;
  const take = React.useCallback(() => {
    const request = pendingFileOpen;
    const { load, select, tabId, workspacePath } = latest.current;
    if (!request || request.tabId !== tabId || !workspacePath) return;
    pendingFileOpen = null;
    const target = resolveWorkspaceFileTarget(workspacePath, request.path);
    if (!target) {
      toast.info(t("workspaceFiles.openPathOutside"));
      return;
    }
    void load(target.directory).then((listing) => {
      const entry = findRequestedFileEntry(listing, target.name);
      if (entry) {
        pendingLineReveal = request.lines ? { id: ++lastLineRevealId, path: entry.path, lines: request.lines } : null;
        void select(entry);
      } else toast.info(t("workspaceFiles.openPathMissing"));
    }, (error: unknown) => toast.error(error instanceof Error ? error.message : String(error)));
  }, [t]);
  React.useEffect(() => {
    if (!args.ready) return;
    take();
    window.addEventListener(WORKSPACE_FILE_OPEN_EVENT, take);
    return () => window.removeEventListener(WORKSPACE_FILE_OPEN_EVENT, take);
  }, [args.ready, take]);
}
