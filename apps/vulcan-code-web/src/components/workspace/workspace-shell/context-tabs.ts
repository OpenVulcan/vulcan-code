import {
  type WorkspaceContextTabItem,
  type WorkspaceContextTabKind,
  type WorkspaceContextTabState,
} from "@/components/workspace/workspace-context-panel";
import { workspaceContextTabDefaultTitle } from "@/components/workspace/workspace-context-tab-title";
import type { TranslationKey } from "@/i18n";
import {
  selectedConversationWorkspacePath,
} from "@/diff/workspace-review-model";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import type { WorkbenchSnapshot } from "@/types";

// workspaceContextTabStateCache preserves right context tabs during the current app runtime.
// workspaceContextTabStateCache 在当前程序运行期内保留右侧上下文页签。
export const workspaceContextTabStateCache = new Map<string, WorkspaceContextTabState>();

// workspaceContextTabStateCacheLimit bounds retained conversation tab workspaces.
// workspaceContextTabStateCacheLimit 限制保留的会话页签工作区数量。
const workspaceContextTabStateCacheLimit = 32;

// workspaceContextFileTabCounter keeps generated file-tab ids unique within this runtime.
// workspaceContextFileTabCounter 保持当前运行期生成的文件页签标识唯一。
let workspaceContextFileTabCounter = 0;

// WorkspaceContextTabStateEntry couples a tab state with the cache key it belongs to.
// WorkspaceContextTabStateEntry 将页签状态与其所属缓存键绑定。
export interface WorkspaceContextTabStateEntry {
  // Cache key that owns this state snapshot.
  // 拥有该状态快照的缓存键。
  cacheKey: string;
  // Opened right context tab state.
  // 已打开的右侧上下文页签状态。
  state: WorkspaceContextTabState;
}

// emptyWorkspaceContextTabState creates a blank right-context tab workspace.
// emptyWorkspaceContextTabState 创建空白右侧上下文页签工作区。
export function emptyWorkspaceContextTabState(): WorkspaceContextTabState {
  return { activeTabId: null, tabs: [] };
}

// workspaceContextTabCacheKey builds the runtime-only cache key for one conversation surface.
// workspaceContextTabCacheKey 为单个会话界面构建仅运行期使用的缓存键。
export function workspaceContextTabCacheKey(
  // Workbench snapshot containing conversation routing state.
  // 包含会话路由状态的工作台快照。
  workbench: WorkbenchSnapshot,
  // Current persisted conversation id.
  // 当前持久化会话标识。
  selectedConversationId: string | null,
  // Current draft conversation selection.
  // 当前草稿会话选择。
  selectedDraftConversation: DraftConversationSelection | null,
): string {
  // surfaceId keeps right-panel memory scoped to the selected parent conversation.
  // surfaceId 将右侧面板记忆范围限定在当前选中的父会话。
  const surfaceId = selectedConversationId ?? null;
  // workspacePath adds project-root separation for conversations that share ids across sources.
  // workspacePath 为跨来源共享标识的会话补充项目根目录隔离。
  const workspacePath = selectedConversationWorkspacePath(workbench, selectedConversationId, selectedDraftConversation);
  return `${surfaceId ?? selectedDraftConversation?.id ?? "none"}:${workspacePath ?? selectedDraftConversation?.projectId ?? "none"}`;
}

// openWorkspaceSubagentContextTab opens or focuses one subagent transcript tab.
// openWorkspaceSubagentContextTab 打开或聚焦单个子代理转录页签。
export function openWorkspaceSubagentContextTab(
  // Current tab state.
  // 当前页签状态。
  state: WorkspaceContextTabState,
  // Parent conversation that owns the subagent task.
  // 拥有子代理任务的父会话。
  parentConversationId: string,
  // Child task conversation id represented by the tab.
  // 页签代表的子任务会话标识。
  taskId: string,
  // Visible tab title resolved from the backend task row.
  // 从后端任务行解析出的可见页签标题。
  title: string,
  // Translation helper for default tab titles.
  // 默认页签标题的翻译辅助函数。
  t: (key: TranslationKey) => string,
): WorkspaceContextTabState {
  // current stores a normalized base state before modification.
  // current 保存修改前规整后的基础状态。
  const current = normalizeWorkspaceContextTabState(state);
  // subagentTabId stores a stable singleton id per parent-task pair.
  // subagentTabId 为每个父会话与任务组合保存稳定单例页签标识。
  const subagentTabId = workspaceSubagentTabId(parentConversationId, taskId);
  // existing stores a previously opened tab for the same subagent task.
  // existing 保存同一子代理任务此前打开过的页签。
  const existing = current.tabs.find((tab) => tab.id === subagentTabId);
  if (existing) {
    return {
      activeTabId: existing.id,
      tabs: current.tabs.map((tab) => (
        tab.id === existing.id
          ? { ...tab, title: title.trim() || tab.title || workspaceContextTabDefaultTitle("subagent", t) }
          : tab
      )),
    };
  }
  // nextTab stores the newly created subagent context tab.
  // nextTab 保存新创建的子代理上下文页签。
  const nextTab: WorkspaceContextTabItem = {
    createdAt: Date.now(),
    id: subagentTabId,
    kind: "subagent",
    selectedFileName: null,
    subagentParentConversationId: parentConversationId,
    subagentTaskId: taskId,
    title: title.trim() || workspaceContextTabDefaultTitle("subagent", t),
  };
  return {
    activeTabId: nextTab.id,
    tabs: [...current.tabs, nextTab],
  };
}

// openWorkspaceViewerContextTab shows a file in the single viewer tab, opening it when needed; the tab is named
// after the file.
// openWorkspaceViewerContextTab 在唯一的查看页签中显示文件，需要时打开该页签；页签以文件名命名。
export function openWorkspaceViewerContextTab(
  // Current tab state.
  // 当前页签状态。
  state: WorkspaceContextTabState,
  // Absolute path of the file to show.
  // 需要显示的文件绝对路径。
  path: string,
  // Translation helper for default tab titles.
  // 默认页签标题的翻译辅助函数。
  t: (key: TranslationKey) => string,
): WorkspaceContextTabState {
  const opened = openWorkspaceContextTab(state, "viewer", t);
  const title = path.split(/[\\/]/).pop() || workspaceContextTabDefaultTitle("viewer", t);
  return {
    ...opened,
    tabs: opened.tabs.map((tab) => (tab.id === opened.activeTabId ? { ...tab, title, viewerPath: path } : tab)),
  };
}

// normalizeWorkspaceContextTabState removes stale active ids from a tab state.
// normalizeWorkspaceContextTabState 从页签状态中移除失效的活动标识。
export function normalizeWorkspaceContextTabState(
  // Candidate tab state to normalize.
  // 需要规整的候选页签状态。
  state: WorkspaceContextTabState,
): WorkspaceContextTabState {
  // activeExists records whether the current active id still points to an opened tab.
  // activeExists 记录当前活动标识是否仍指向已打开页签。
  const activeExists = state.tabs.some((tab) => tab.id === state.activeTabId);
  return {
    activeTabId: activeExists ? state.activeTabId : state.tabs[0]?.id ?? null,
    tabs: state.tabs,
  };
}

// openWorkspaceContextTab opens a tab kind or focuses an existing singleton tab.
// openWorkspaceContextTab 打开页签类型或聚焦已有单例页签。
export function openWorkspaceContextTab(
  // Current tab state.
  // 当前页签状态。
  state: WorkspaceContextTabState,
  // Context tab kind to open.
  // 需要打开的上下文页签类型。
  kind: WorkspaceContextTabKind,
  // Translation helper for default tab titles.
  // 默认页签标题的翻译辅助函数。
  t: (key: TranslationKey) => string,
): WorkspaceContextTabState {
  // current stores a normalized base state before modification.
  // current 保存修改前规整后的基础状态。
  const current = normalizeWorkspaceContextTabState(state);
  if (kind !== "files") {
    // existing stores the already opened singleton review or memory tab.
    // existing 保存已经打开的单例审查或记忆页签。
    const existing = current.tabs.find((tab) => tab.kind === kind);
    if (existing) {
      return { ...current, activeTabId: existing.id };
    }
  }
  // nextTab stores the newly created context tab.
  // nextTab 保存新创建的上下文页签。
  const nextTab = createWorkspaceContextTab(kind, t);
  return {
    activeTabId: nextTab.id,
    tabs: [...current.tabs, nextTab],
  };
}

// closeWorkspaceContextTab closes one tab and activates a nearby remaining tab.
// closeWorkspaceContextTab 关闭一个页签并激活附近剩余页签。
export function closeWorkspaceContextTab(
  // Current tab state.
  // 当前页签状态。
  state: WorkspaceContextTabState,
  // Tab id to close.
  // 需要关闭的页签标识。
  tabId: string,
): WorkspaceContextTabState {
  // current stores a normalized base state before modification.
  // current 保存修改前规整后的基础状态。
  const current = normalizeWorkspaceContextTabState(state);
  // closedIndex stores the visual index of the closed tab.
  // closedIndex 保存被关闭页签的视觉位置。
  const closedIndex = current.tabs.findIndex((tab) => tab.id === tabId);
  if (closedIndex < 0) {
    return current;
  }
  // nextTabs stores the remaining visual tab order.
  // nextTabs 保存剩余的视觉页签顺序。
  const nextTabs = current.tabs.filter((tab) => tab.id !== tabId);
  if (current.activeTabId !== tabId) {
    return { activeTabId: current.activeTabId, tabs: nextTabs };
  }
  // nextActiveTab stores the right-neighbor preference, falling back to the left neighbor.
  // nextActiveTab 保存优先右邻、其次左邻的下一个活动页签。
  const nextActiveTab = nextTabs[closedIndex] ?? nextTabs[closedIndex - 1] ?? null;
  return {
    activeTabId: nextActiveTab?.id ?? null,
    tabs: nextTabs,
  };
}

// WorkspaceContextTabCount records how many tabs one conversation's right panel held at one render.
// WorkspaceContextTabCount 记录某个会话右侧面板在一次渲染时持有的页签数量。
export interface WorkspaceContextTabCount {
  // Cache key of the conversation surface that owns the tabs.
  // 拥有这些页签的会话界面缓存键。
  cacheKey: string;
  // Number of opened tabs.
  // 已打开页签数量。
  count: number;
}

// workspaceContextLastTabClosed reports whether the panel's last page was just closed in the same conversation.
// Switching to a conversation that has no pages is not a close, so the panel stays as the user left it.
// workspaceContextLastTabClosed 判断是否刚在同一会话内关闭了面板的最后一个页面。
// 切换到没有页面的会话不算关闭，面板保持用户离开时的状态。
export function workspaceContextLastTabClosed(
  // Tab count observed at the previous render.
  // 上一次渲染观察到的页签数量。
  previous: WorkspaceContextTabCount,
  // Tab count observed at the current render.
  // 当前渲染观察到的页签数量。
  next: WorkspaceContextTabCount,
): boolean {
  return previous.cacheKey === next.cacheKey && previous.count > 0 && next.count === 0;
}

// activateWorkspaceContextTab activates an already opened tab.
// activateWorkspaceContextTab 激活一个已经打开的页签。
export function activateWorkspaceContextTab(
  // Current tab state.
  // 当前页签状态。
  state: WorkspaceContextTabState,
  // Tab id to activate.
  // 需要激活的页签标识。
  tabId: string,
): WorkspaceContextTabState {
  // current stores a normalized base state before modification.
  // current 保存修改前规整后的基础状态。
  const current = normalizeWorkspaceContextTabState(state);
  return current.tabs.some((tab) => tab.id === tabId) ? { ...current, activeTabId: tabId } : current;
}

// focusWorkspaceFilesTab makes a files tab active for showing a requested file: the active one when it is a files
// tab, else the first opened files tab, else a new one.
// focusWorkspaceFilesTab 激活一个文件页签以显示请求的文件：当前活动页签是文件页签时用它，否则用第一个已打开的文件页签，
// 都没有时新建一个。
export function focusWorkspaceFilesTab(
  // Current tab state.
  // 当前页签状态。
  state: WorkspaceContextTabState,
  // Translation helper for a new tab's default title.
  // 新页签默认标题的翻译辅助函数。
  t: (key: TranslationKey) => string,
): WorkspaceContextTabState {
  const current = normalizeWorkspaceContextTabState(state);
  if (current.tabs.some((tab) => tab.id === current.activeTabId && tab.kind === "files")) return current;
  const existing = current.tabs.find((tab) => tab.kind === "files");
  return existing ? { ...current, activeTabId: existing.id } : openWorkspaceContextTab(current, "files", t);
}

// renameWorkspaceFileContextTab updates a file tab title from its selected file.
// renameWorkspaceFileContextTab 根据选中文件更新文件页签标题。
export function renameWorkspaceFileContextTab(
  // Current tab state.
  // 当前页签状态。
  state: WorkspaceContextTabState,
  // File tab id to rename.
  // 需要重命名的文件页签标识。
  tabId: string,
  // New visible title.
  // 新的可见标题。
  title: string,
  // Translation helper for empty file-tab title fallback.
  // 空文件页签标题回退的翻译辅助函数。
  t: (key: TranslationKey) => string,
): WorkspaceContextTabState {
  // normalizedTitle stores the non-empty title fallback.
  // normalizedTitle 保存非空标题回退值。
  const normalizedTitle = title.trim() || workspaceContextTabDefaultTitle("files", t);
  return {
    activeTabId: state.activeTabId,
    tabs: state.tabs.map((tab) => (
      tab.id === tabId && tab.kind === "files"
        ? { ...tab, selectedFileName: normalizedTitle, title: normalizedTitle }
        : tab
    )),
  };
}

// createWorkspaceContextTab creates one opened context tab descriptor.
// createWorkspaceContextTab 创建一个已打开上下文页签描述。
function createWorkspaceContextTab(
  // Context tab kind to create.
  // 需要创建的上下文页签类型。
  kind: WorkspaceContextTabKind,
  // Translation helper for default tab titles.
  // 默认页签标题的翻译辅助函数。
  t: (key: TranslationKey) => string,
): WorkspaceContextTabItem {
  // createdAt stores the runtime creation time for this tab.
  // createdAt 保存该页签的运行期创建时间。
  const createdAt = Date.now();
  // id stores singleton ids for review/memory and unique ids for file tabs.
  // id 为审查/记忆保存单例标识，为文件页签保存唯一标识。
  const id = kind === "files" ? `files-${createdAt}-${workspaceContextFileTabCounter++}` : kind;
  return {
    createdAt,
    id,
    kind,
    selectedFileName: null,
    title: workspaceContextTabDefaultTitle(kind, t),
  };
}

// workspaceSubagentTabId builds the stable right-panel tab id for one subagent task.
// workspaceSubagentTabId 为单个子代理任务构建稳定右侧页签标识。
export function workspaceSubagentTabId(parentConversationId: string, taskId: string): string {
  return `subagent-${encodeURIComponent(parentConversationId)}-${encodeURIComponent(taskId)}`;
}

// trimWorkspaceContextTabStateCache removes old conversation tab workspaces.
// trimWorkspaceContextTabStateCache 移除旧的会话页签工作区。
export function trimWorkspaceContextTabStateCache(): void {
  while (workspaceContextTabStateCache.size > workspaceContextTabStateCacheLimit) {
    // firstKey stores the oldest inserted cache key.
    // firstKey 保存最早写入的缓存键。
    const firstKey = workspaceContextTabStateCache.keys().next().value;
    if (!firstKey) {
      return;
    }
    workspaceContextTabStateCache.delete(firstKey);
  }
}

