import React from "react";
import { ChatArea } from "@/components/conversation/chat-area";
import {
  ConversationSessionTodolistPanel,
  type ConversationSessionReviewSummary,
} from "@/components/conversation/session-todolist-panel";
import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type {
  ComposerAiCapabilityConfigurationSave,
  ComposerSlashCommandResult,
  ComposerSubmission,
} from "@/components/composer/composer-model";
import {
  parseConversationBlocks,
} from "@/components/conversation/conversation-transcript";
import { dispatchConversationLayoutWillChange } from "@/components/conversation/conversation-layout-events";
import {
  activeSkillHeaderSummary,
  type SkillActionProps,
  type SkillHeaderBusyActionId,
  type SkillSubAreaId,
} from "@/components/features/skills/SkillsArea";
import type {
  MemoryActionProps,
  MemorySubAreaId,
} from "@/components/features/memory/MemoryArea";
import {
  ProjectSidebar,
  type ConversationMenuAction,
  type ProjectMenuAction,
} from "@/components/project/ProjectSidebar";
import { Button } from "@/components/ui/button";
import {
  FloatingSkillMenu,
  skillSubAreaLabel,
  WorkspaceSkillsSettings,
} from "@/components/workspace/workspace-skills-settings";
import {
  FloatingMemoryMenu,
  memorySubAreaMeta,
  WorkspaceMemorySettings,
} from "@/components/workspace/workspace-memory-settings";
import { SidebarInset, SidebarProvider, useSidebar } from "@/components/ui/sidebar";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  WorkspaceResizableBody,
  type DraftConversationSelection,
} from "@/components/workspace/workspace-resizable-body";
import {
  workspaceContextTabDefaultTitle,
  type WorkspaceContextTabItem,
  type WorkspaceContextTabKind,
  type WorkspaceContextTabState,
} from "@/components/workspace/WorkspaceContextPanel";
import {
  activeWorkbenchSurfaceId,
  selectedConversationWorkspacePath,
} from "@/diff/workspace-review-model";
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
import type {
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ConversationHydrationProgress,
  ConversationSummary,
  MainAreaId,
  PendingPromptSummary,
  ProjectSummary,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  StartupSnapshot,
  UserPromptEditDraftSummary,
  WorkspaceFileDirectoryListing,
  WorkspaceFileFilterOptions,
  WorkspaceFileReadResponse,
  WorkbenchSnapshot,
} from "@/types";
import type { WorkspaceDiffFileDetailLoadOptions } from "@/service/workbench-snapshot-service";
import type { WorkspacePageDescriptor } from "@/components/workspace/workspace-page-descriptor";
import { useI18n } from "@/i18n";
import {
  PanelRightClose,
  PanelRightOpen,
  PackagePlus,
  RefreshCw,
} from "lucide-react";

// workspaceContextTabStateCache preserves right context tabs during the current app runtime.
// workspaceContextTabStateCache 在当前程序运行期内保留右侧上下文页签。
const workspaceContextTabStateCache = new Map<string, WorkspaceContextTabState>();

// workspaceContextTabStateCacheLimit bounds retained conversation tab workspaces.
// workspaceContextTabStateCacheLimit 限制保留的会话页签工作区数量。
const workspaceContextTabStateCacheLimit = 32;

// workspaceContextFileTabCounter keeps file tab ids unique in the current runtime.
// workspaceContextFileTabCounter 保持当前运行期内文件页签标识唯一。
let workspaceContextFileTabCounter = 1;

// WorkspaceContextTabStateEntry couples a tab state with the cache key it belongs to.
// WorkspaceContextTabStateEntry 将页签状态与其所属缓存键绑定。
interface WorkspaceContextTabStateEntry {
  // Cache key that owns this state snapshot.
  // 拥有该状态快照的缓存键。
  cacheKey: string;
  // Opened right context tab state.
  // 已打开的右侧上下文页签状态。
  state: WorkspaceContextTabState;
}

// WorkspaceMainView identifies the active main surface inside the workbench.
// WorkspaceMainView 标识工作台内部当前激活的主内容界面。
export type WorkspaceMainView = "chat" | "skills" | "memory";

// emptyWorkspaceContextTabState creates a blank right-context tab workspace.
// emptyWorkspaceContextTabState 创建空白右侧上下文页签工作区。
function emptyWorkspaceContextTabState(): WorkspaceContextTabState {
  return { activeTabId: null, tabs: [] };
}

// workspaceContextTabCacheKey builds the runtime-only cache key for one conversation surface.
// workspaceContextTabCacheKey 为单个会话界面构建仅运行期使用的缓存键。
function workspaceContextTabCacheKey(
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
  // surfaceId mirrors the context-panel cache scope used by review and file hosts.
  // surfaceId 对齐审查与文件宿主使用的上下文面板缓存范围。
  const surfaceId = activeWorkbenchSurfaceId(workbench, selectedConversationId);
  // workspacePath adds project-root separation for conversations that share ids across sources.
  // workspacePath 为跨来源共享标识的会话补充项目根目录隔离。
  const workspacePath = selectedConversationWorkspacePath(workbench, selectedConversationId, selectedDraftConversation);
  return `${surfaceId ?? selectedDraftConversation?.id ?? "none"}:${workspacePath ?? selectedDraftConversation?.projectId ?? "none"}`;
}

// normalizeWorkspaceContextTabState removes stale active ids from a tab state.
// normalizeWorkspaceContextTabState 从页签状态中移除失效的活动标识。
function normalizeWorkspaceContextTabState(
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
function openWorkspaceContextTab(
  // Current tab state.
  // 当前页签状态。
  state: WorkspaceContextTabState,
  // Context tab kind to open.
  // 需要打开的上下文页签类型。
  kind: WorkspaceContextTabKind,
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
  const nextTab = createWorkspaceContextTab(kind);
  return {
    activeTabId: nextTab.id,
    tabs: [...current.tabs, nextTab],
  };
}

// closeWorkspaceContextTab closes one tab and activates a nearby remaining tab.
// closeWorkspaceContextTab 关闭一个页签并激活附近剩余页签。
function closeWorkspaceContextTab(
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

// activateWorkspaceContextTab activates an already opened tab.
// activateWorkspaceContextTab 激活一个已经打开的页签。
function activateWorkspaceContextTab(
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

// renameWorkspaceFileContextTab updates a file tab title from its selected file.
// renameWorkspaceFileContextTab 根据选中文件更新文件页签标题。
function renameWorkspaceFileContextTab(
  // Current tab state.
  // 当前页签状态。
  state: WorkspaceContextTabState,
  // File tab id to rename.
  // 需要重命名的文件页签标识。
  tabId: string,
  // New visible title.
  // 新的可见标题。
  title: string,
): WorkspaceContextTabState {
  // normalizedTitle stores the non-empty title fallback.
  // normalizedTitle 保存非空标题回退值。
  const normalizedTitle = title.trim() || workspaceContextTabDefaultTitle("files");
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
    title: workspaceContextTabDefaultTitle(kind),
  };
}

// trimWorkspaceContextTabStateCache removes old conversation tab workspaces.
// trimWorkspaceContextTabStateCache 移除旧的会话页签工作区。
function trimWorkspaceContextTabStateCache(): void {
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

// WorkspaceShellProps contains the visible workspace layout state and injected service edges.
// WorkspaceShellProps 包含可见工作区布局状态和注入的服务边界。
interface WorkspaceShellProps {
  // Startup snapshot shown by nested workspace panels.
  // 嵌套工作区面板展示的启动快照。
  startup: StartupSnapshot;
  // Workbench snapshot rendered by the project tree and conversation area.
  // 项目树与会话区渲染使用的工作台快照。
  workbench: WorkbenchSnapshot;
  // Active main surface inside the workbench.
  // 工作台内部当前激活的主内容界面。
  mainView: WorkspaceMainView;
  // Shared navigation sidebar open state.
  // 共享导航侧栏展开状态。
  sidebarOpen: boolean;
  // Updates the shared navigation sidebar open state.
  // 更新共享导航侧栏展开状态。
  onSidebarOpenChange: (open: boolean) => void;
  // Selected embedded skills sub page.
  // 当前选中的内嵌技能子页面。
  activeSkillSubArea: SkillSubAreaId;
  // Selected embedded memory sub page.
  // 当前选中的内嵌记忆子页面。
  activeMemorySubArea: MemorySubAreaId;
  // Draft conversation selected before a persisted session exists.
  // 持久会话创建前选中的草稿会话。
  selectedDraftConversation: DraftConversationSelection | null;
  // Selected conversation summary, when one exists.
  // 当前选中的会话摘要，如果存在。
  selectedConversation?: ConversationSummary;
  // Selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Conversation id currently being hydrated.
  // 当前正在水合的会话标识。
  loadingConversationId: string | null;
  // Last conversation loading error.
  // 最近一次会话加载错误。
  conversationLoadError: string | null;
  // Current lazy conversation body loading progress.
  // 当前懒加载会话正文进度。
  conversationHydration: ConversationHydrationProgress | null;
  // Whether older history exists before the loaded selected conversation window.
  // 当前已加载选中会话窗口之前是否存在更早历史。
  conversationHasMoreHistory: boolean;
  // Whether an older history page is currently being loaded.
  // 当前是否正在加载更早历史页。
  conversationHistoryLoading: boolean;
  // Pinned conversation ids used by the project tree.
  // 项目树使用的置顶会话标识集合。
  pinnedConversationIds: Set<string>;
  // Handles one conversation context-menu action.
  // 处理单个会话右键菜单动作。
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Creates a new conversation in one project.
  // 在单个项目中创建新会话。
  onCreateConversation: (project: ProjectSummary) => void;
  // Opens the create-project flow.
  // 打开创建项目流程。
  onCreateProject: () => void;
  // Opens the skills feature area from the workbench sidebar.
  // 从工作台侧栏打开技能功能区。
  onOpenSkills: () => void;
  // Opens the memory feature area from the workbench sidebar.
  // 从工作台侧栏打开记忆功能区。
  onOpenMemory: () => void;
  // Selects one top-level application area from the sidebar brand menu.
  // 从侧栏品牌菜单选择一个顶层应用区域。
  onSelectArea: (area: MainAreaId) => void;
  // Updates the selected embedded skills sub page.
  // 更新当前选中的内嵌技能子页面。
  onSelectSkillSubArea: (subArea: SkillSubAreaId) => void;
  // Updates the selected embedded memory sub page.
  // 更新当前选中的内嵌记忆子页面。
  onSelectMemorySubArea: (subArea: MemorySubAreaId) => void;
  // Persists the expanded project id list.
  // 持久化展开的项目标识列表。
  onExpandedProjectIdsChange: (projectIds: string[]) => void;
  // Handles one project context-menu action.
  // 处理单个项目右键菜单动作。
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  // Persists project order changes.
  // 持久化项目排序变化。
  onReorderProjects: (projectIds: string[]) => void;
  // Selects one persisted conversation.
  // 选择一个持久化会话。
  onSelectConversation: (conversationId: string) => void;
  // Loads one older history page for a conversation.
  // 为单个会话加载一页更早历史。
  onLoadConversationHistory: (conversationId: string) => Promise<boolean>;
  // Selects the active model for the workbench.
  // 选择工作台当前模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Loads backend-managed secondary routes for one model.
  // 加载单个模型的后端二级路由。
  onListWorkbenchModelSecondaryRoutes: (
    providerId: string,
    modelId: string,
  ) => Promise<ProviderModelSecondaryRouteSummary[]>;
  // Persists a backend-managed secondary route for one model.
  // 持久化单个模型的后端二级路由。
  onSelectWorkbenchModelSecondaryRoute: (
    providerId: string,
    modelId: string,
    route: ProviderModelSecondaryRouteSelection | null,
  ) => Promise<boolean>;
  // Selects the active reasoning effort for the workbench.
  // 选择工作台当前推理强度。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps;
  // Service-backed memory page actions.
  // 服务端支持的记忆页面动作。
  memoryActions: MemoryActionProps;
  // Fixed footer content pinned to the workspace bottom edge.
  // 固定在工作区底边的信息栏内容。
  footer?: React.ReactNode;
  // App-wide chrome actions rendered in the shared content header.
  // 渲染在共享内容标题栏中的应用级窗口控制区。
  chromeActions?: React.ReactNode;
  // Whether shared title bar empty space should be a native drag region.
  // 共享标题栏空白区域是否作为原生拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
  // Routes one composer slash command.
  // 路由一个输入区斜杠命令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Submits one composer prompt.
  // 提交一个输入区提示词。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // Persists one session-scoped AI capability draft.
  // 持久化单个会话级 AI 能力草稿。
  onSaveAiCapabilityConfiguration: (request: ComposerAiCapabilityConfigurationSave) => Promise<boolean>;
  // Persists one user-authored session goal.
  // 持久化单个用户编写的会话目标。
  onSaveSessionGoal: (objective: string, tokenBudget?: number | null) => Promise<boolean>;
  // Resumes one stopped session goal with a fresh budget stage.
  // 使用新的预算阶段继续单个已停止会话目标。
  onResumeSessionGoal: (objective: string, tokenBudget?: number | null) => Promise<boolean>;
  // Clears the current user-authored session goal.
  // 清理当前用户编写的会话目标。
  onClearSessionGoal: () => Promise<boolean>;
  // Updates the current session goal lifecycle status.
  // 更新当前会话目标生命周期状态。
  onUpdateSessionGoalStatus: (status: string) => Promise<boolean>;
  // Persists the current session Plan-mode toggle.
  // 持久化当前会话计划模式开关。
  onSetSessionPlanMode: (enabled: boolean) => Promise<boolean>;
  // Stops the selected conversation run.
  // 停止当前选中会话运行。
  onStopConversationRun: () => Promise<boolean>;
  // Opens a routed subagent transcript panel.
  // 打开已路由的子代理转录面板。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Closes the routed subagent transcript panel.
  // 关闭已路由的子代理转录面板。
  onCloseSubagentPanel: () => Promise<void>;
  // Stops one subagent task.
  // 停止一个子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Force-stops one live command session.
  // 强制停止单条实时命令会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
  // Edits one queued prompt.
  // 编辑一个队列提示词。
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  // Inserts one queued prompt into the composer.
  // 将一个队列提示词插入输入区。
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  // Removes one queued prompt.
  // 移除一个队列提示词。
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  // Reorders queued prompts.
  // 重排队列提示词。
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
  // Loads a fresh workspace diff snapshot.
  // 加载最新工作区 diff 快照。
  onLoadWorkspaceDiffSnapshot: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    selection: DiffReviewSelection,
  ) => Promise<WorkspaceDiffSnapshotResponse>;
  // Loads a fresh workspace diff detail for one file.
  // 加载单个文件的最新工作区 diff 详情。
  onLoadWorkspaceDiffFileDetail: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    selection: DiffReviewSelection,
    file: WorkspaceDiffFileDetailRequest,
    options?: WorkspaceDiffFileDetailLoadOptions,
  ) => Promise<WorkspaceDiffFileDetailResponse>;
  // Loads compact file-level search hits for the current workspace diff.
  // 加载当前工作区 diff 的紧凑文件级搜索命中。
  onLoadWorkspaceDiffSearch: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    selection: DiffReviewSelection,
    search: WorkspaceDiffSearchRequest,
  ) => Promise<WorkspaceDiffSearchResponse>;
  // Runs one file-level Git action for the workspace review panel.
  // 为工作区审核面板执行一次文件级 Git 操作。
  onRunWorkspaceGitFileAction: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: WorkspaceGitFileActionRequest,
  ) => Promise<WorkspaceGitFileActionResult>;
  // Loads one lazy workspace file directory.
  // 加载单个懒加载工作区文件目录。
  onLoadWorkspaceFileDirectory: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    directoryPath: string | null,
    filters: WorkspaceFileFilterOptions,
  ) => Promise<WorkspaceFileDirectoryListing>;
  // Loads one workspace file preview response.
  // 加载单个工作区文件预览响应。
  onLoadWorkspaceFilePreview: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    filePath: string,
    filters: WorkspaceFileFilterOptions,
  ) => Promise<WorkspaceFileReadResponse>;
  // Stages one browser-provided attachment through the service.
  // 通过服务暂存一个浏览器提供的附件。
  onStageAttachmentBytes: (
    projectId: string | null,
    conversationId: string | null,
    file: File,
  ) => Promise<ComposerAttachmentSummary | null>;
  // Copies text through the shell-provided clipboard path.
  // 通过外壳提供的剪贴板路径复制文本。
  onCopyText: (text: string) => Promise<void>;
  // Loads one backend-validated user prompt edit draft.
  // 加载单条后端校验过的用户提示词编辑草稿。
  onGetUserPromptEditDraft: (
    conversationId: string,
    messageId: string,
  ) => Promise<UserPromptEditDraftSummary | null>;
  // Submits one user prompt edit and replays it.
  // 提交单条用户提示词编辑并重新执行。
  onSubmitUserPromptEdit: (request: {
    conversationId: string;
    messageId: string;
    prompt: string;
    attachments: ComposerAttachmentSummary[];
  }) => Promise<boolean>;
  // Creates a forked copy of one conversation.
  // 创建单个会话的分叉副本。
  onForkConversation: (conversationId: string, beforeMessageId?: string | null) => Promise<boolean>;
  // Loads hidden tool details for transcript rows.
  // 为转录行加载隐藏工具详情。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Resolves local media paths into renderable URLs.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Emits lightweight click diagnostics.
  // 输出轻量点击诊断信息。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
}

// WorkspaceShell renders the shadcn dashboard-like fused sidebar and inset console area.
// WorkspaceShell 渲染类似 shadcn dashboard 的融合侧栏与 inset 控制台区域。
export function WorkspaceShell(props: WorkspaceShellProps) {
  return (
    <SidebarProvider
      className="console-shell"
      open={props.sidebarOpen}
      onOpenChange={props.onSidebarOpenChange}
    >
      <WorkspaceShellContent {...props} />
    </SidebarProvider>
  );
}

// WorkspaceShellContent renders the workspace shell after sidebar context is available.
// WorkspaceShellContent 在侧栏上下文可用后渲染工作台外壳。
function WorkspaceShellContent(props: WorkspaceShellProps) {
  // sidebar controls the left project tree that can collapse automatically.
  // sidebar 控制可自动折叠的左侧项目树。
  const sidebar = useSidebar();
  // t translates shared workspace chrome labels.
  // t 翻译共享工作区外壳标签。
  const { t } = useI18n();
  // shellRef measures the available workspace width for automatic panel collapse.
  // shellRef 测量工作台可用宽度，用于自动折叠面板。
  const shellRef = React.useRef<HTMLDivElement | null>(null);
  // previousShellWidthRef tracks resize direction so auto-restore only runs while width grows.
  // previousShellWidthRef 跟踪尺寸变化方向，确保自动恢复只在宽度增长时执行。
  const previousShellWidthRef = React.useRef<number | null>(null);
  // rightPanelOpen stores whether the review and memory panel is visible.
  // rightPanelOpen 保存审核与记忆面板是否可见。
  const [rightPanelOpen, setRightPanelOpen] = React.useState(false);
  // rightPanelFloating stores whether the right panel overlays instead of resizing the chat area.
  // rightPanelFloating 保存右侧面板是否以浮动方式覆盖而不是压缩会话区。
  const [rightPanelFloating, setRightPanelFloating] = React.useState(false);
  // contextTabCacheKey stores the current conversation-scoped right tab memory key.
  // contextTabCacheKey 保存当前会话范围的右侧页签记忆键。
  const contextTabCacheKey = React.useMemo(
    () => workspaceContextTabCacheKey(props.workbench, props.selectedConversationId, props.selectedDraftConversation),
    [props.selectedConversationId, props.selectedDraftConversation, props.workbench],
  );
  // contextTabStateEntry stores tab state together with the cache key it belongs to.
  // contextTabStateEntry 将页签状态与所属缓存键一起保存。
  const [contextTabStateEntry, setContextTabStateEntry] = React.useState<WorkspaceContextTabStateEntry>(
    () => ({
      cacheKey: contextTabCacheKey,
      state: workspaceContextTabStateCache.get(contextTabCacheKey) ?? emptyWorkspaceContextTabState(),
    }),
  );
  // contextTabState exposes the state snapshot for the current render.
  // contextTabState 暴露当前渲染使用的状态快照。
  const contextTabState = contextTabStateEntry.state;
  // reviewFocusRequest increments whenever chat tools ask the review panel to show uncommitted changes.
  // reviewFocusRequest 在会话工具要求审核面板显示未提交变更时递增。
  const [reviewFocusRequest, setReviewFocusRequest] = React.useState(0);
  // leftAutoCollapsed records automatic left-sidebar collapse caused by width pressure.
  // leftAutoCollapsed 记录宽度压力导致的左侧栏自动折叠。
  const [leftAutoCollapsed, setLeftAutoCollapsed] = React.useState(false);
  // skillsMenuExpanded stores the shared adaptive menu state for the skills page.
  // skillsMenuExpanded 保存技能页共享自适应菜单展开状态。
  const [skillsMenuExpanded, setSkillsMenuExpanded] = React.useState(true);
  // memoryMenuExpanded stores the shared adaptive menu state for the memory page.
  // memoryMenuExpanded 保存记忆页共享自适应菜单展开状态。
  const [memoryMenuExpanded, setMemoryMenuExpanded] = React.useState(true);
  // sessionToolExpanded stores the shared adaptive menu state for the chat tool panel.
  // sessionToolExpanded 保存会话工具共享自适应面板展开状态。
  const [sessionToolExpanded, setSessionToolExpanded] = React.useState(true);
  // sessionReviewSummary stores compact Git review state for the selected chat tool panel.
  // sessionReviewSummary 保存当前会话工具面板使用的紧凑 Git 审查状态。
  const [sessionReviewSummary, setSessionReviewSummary] = React.useState<ConversationSessionReviewSummary | null>(null);
  // notifyConversationLayoutWillChange lets the active chat preserve scroll before shell geometry changes.
  // notifyConversationLayoutWillChange 让当前会话在外壳几何变化前保留滚动位置。
  const notifyConversationLayoutWillChange = React.useCallback(() => {
    dispatchConversationLayoutWillChange();
  }, []);
  // luaInstallDialogOpen controls the workspace LuaSkills install dialog.
  // luaInstallDialogOpen 控制工作区 LuaSkills 安装弹窗。
  const [luaInstallDialogOpen, setLuaInstallDialogOpen] = React.useState(false);
  // skillHeaderBusyAction stores the registered skill header action currently running.
  // skillHeaderBusyAction 保存当前正在运行的已注册技能标题栏动作。
  const [skillHeaderBusyAction, setSkillHeaderBusyAction] = React.useState<SkillHeaderBusyActionId | null>(null);
  // hasSelectedChatConversation is true only for an existing persisted conversation surface.
  // hasSelectedChatConversation 仅在当前界面是已存在的持久化会话时为真。
  const hasSelectedChatConversation = props.mainView === "chat" && !props.selectedDraftConversation && Boolean(props.selectedConversationId);
  // selectedChatProject resolves the selected persisted conversation project for Git review summaries.
  // selectedChatProject 解析当前持久化会话所属项目以加载 Git 审查摘要。
  const selectedChatProject = hasSelectedChatConversation && props.selectedConversation?.projectId
    ? props.workbench.projects.find((project) => project.id === props.selectedConversation?.projectId) ?? null
    : null;
  // sessionTerminalSessions stores terminal sessions relevant to the selected conversation.
  // sessionTerminalSessions 保存与当前选中会话相关的终端会话。
  const sessionTerminalSessions = React.useMemo(
    () => {
      if (!hasSelectedChatConversation || !props.selectedConversationId) {
        return [];
      }
      return (props.workbench.commandSessions ?? []).filter((session) => {
        if (session.id === "vldb-controller") {
          return false;
        }
        return session.ownerSessionId === props.selectedConversationId;
      });
    },
    [hasSelectedChatConversation, props.selectedConversationId, props.workbench.commandSessions],
  );
  // effectiveRightPanelOpen suppresses stale right-panel state while a draft conversation is selected.
  // effectiveRightPanelOpen 在选中新建草稿会话时屏蔽残留的右侧面板状态。
  const effectiveRightPanelOpen = hasSelectedChatConversation && rightPanelOpen;
  React.useEffect(() => {
    setContextTabStateEntry({
      cacheKey: contextTabCacheKey,
      state: workspaceContextTabStateCache.get(contextTabCacheKey) ?? emptyWorkspaceContextTabState(),
    });
  }, [contextTabCacheKey]);
  React.useEffect(() => {
    if (!hasSelectedChatConversation) {
      return;
    }
    workspaceContextTabStateCache.set(
      contextTabStateEntry.cacheKey,
      normalizeWorkspaceContextTabState(contextTabStateEntry.state),
    );
    trimWorkspaceContextTabStateCache();
  }, [contextTabStateEntry, hasSelectedChatConversation]);
  // openContextTab opens a new file tab or focuses the singleton review/memory tab.
  // openContextTab 打开新的文件页签或聚焦单例审查/记忆页签。
  const openContextTab = React.useCallback((kind: WorkspaceContextTabKind) => {
    setContextTabStateEntry((current) => ({
      ...current,
      state: openWorkspaceContextTab(current.state, kind),
    }));
  }, []);
  // closeContextTab removes one opened right context tab and selects a nearby tab.
  // closeContextTab 移除一个已打开的右侧上下文页签并选择相邻页签。
  const closeContextTab = React.useCallback((tabId: string) => {
    setContextTabStateEntry((current) => ({
      ...current,
      state: closeWorkspaceContextTab(current.state, tabId),
    }));
  }, []);
  // activateContextTab switches the right context content to an opened tab.
  // activateContextTab 将右侧上下文内容切换到已打开页签。
  const activateContextTab = React.useCallback((tabId: string) => {
    setContextTabStateEntry((current) => ({
      ...current,
      state: activateWorkspaceContextTab(current.state, tabId),
    }));
  }, []);
  // updateFileContextTabTitle updates one file tab title after selection changes.
  // updateFileContextTabTitle 在文件选择变化后更新单个文件页签标题。
  const updateFileContextTabTitle = React.useCallback((tabId: string, title: string) => {
    setContextTabStateEntry((current) => ({
      ...current,
      state: renameWorkspaceFileContextTab(current.state, tabId, title),
    }));
  }, []);
  // handleLeftPanelToggle clears automatic restore intent when the user toggles the left sidebar manually.
  // handleLeftPanelToggle 在用户手动切换左侧栏时清除自动恢复意图。
  const handleLeftPanelToggle = React.useCallback(() => {
    notifyConversationLayoutWillChange();
    setLeftAutoCollapsed(false);
  }, [notifyConversationLayoutWillChange]);
  // handleRightPanelToggle clears automatic restore intent when the user toggles the right panel manually.
  // handleRightPanelToggle 在用户手动切换右侧面板时清除自动恢复意图。
  const handleRightPanelToggle = React.useCallback(() => {
    notifyConversationLayoutWillChange();
    const nextOpen = !rightPanelOpen;
    if (!nextOpen) {
      setRightPanelFloating(false);
      setRightPanelOpen(false);
      return;
    }
    const width = shellRef.current?.getBoundingClientRect().width ?? 0;
    const expandedLeftWidth = 288;
    const collapsedLeftWidth = 46;
    const preferredRightWidth = 480;
    const handleWidth = 9;
    const leftWidth = sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
    setRightPanelFloating(width > 0 && width - leftWidth - preferredRightWidth - handleWidth < 800);
    setRightPanelOpen(true);
  }, [notifyConversationLayoutWillChange, rightPanelOpen, sidebar.open]);

  // handleOpenReviewChanges opens the right panel and focuses the live uncommitted review source.
  // handleOpenReviewChanges 打开右侧面板并聚焦实时未提交审核来源。
  const handleOpenReviewChanges = React.useCallback(() => {
    if (!hasSelectedChatConversation) {
      return;
    }
    notifyConversationLayoutWillChange();
    openContextTab("review");
    setReviewFocusRequest((current) => current + 1);
    const width = shellRef.current?.getBoundingClientRect().width ?? 0;
    const expandedLeftWidth = 288;
    const collapsedLeftWidth = 46;
    const preferredRightWidth = 480;
    const handleWidth = 9;
    const leftWidth = sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
    setRightPanelFloating(width > 0 && width - leftWidth - preferredRightWidth - handleWidth < 800);
    setRightPanelOpen(true);
  }, [hasSelectedChatConversation, notifyConversationLayoutWillChange, openContextTab, sidebar.open]);

  // handleSessionToolExpandedChange preserves the transcript viewport before Git panel shape changes.
  // handleSessionToolExpandedChange 在 Git 面板形态变化前保留转录视口。
  const handleSessionToolExpandedChange = React.useCallback((expanded: boolean) => {
    notifyConversationLayoutWillChange();
    setSessionToolExpanded(expanded);
  }, [notifyConversationLayoutWillChange]);

  React.useEffect(() => {
    if (!hasSelectedChatConversation && rightPanelOpen) {
      notifyConversationLayoutWillChange();
      setRightPanelOpen(false);
      setRightPanelFloating(false);
    }
  }, [hasSelectedChatConversation, notifyConversationLayoutWillChange, rightPanelOpen]);

  // runSkillHeaderAction wraps registered skill header actions with busy feedback.
  // runSkillHeaderAction 为注册的技能标题栏动作包裹忙碌反馈。
  const runSkillHeaderAction = React.useCallback(async (actionId: SkillHeaderBusyActionId, action: () => Promise<void>) => {
    setSkillHeaderBusyAction(actionId);
    const startedAt = window.performance.now();
    try {
      await action();
    } finally {
      const remainingMs = 450 - (window.performance.now() - startedAt);
      if (remainingMs > 0) {
        await new Promise((resolve) => {
          window.setTimeout(resolve, remainingMs);
        });
      }
      setSkillHeaderBusyAction(null);
    }
  }, []);

  // sessionTodolist stores the selected conversation todolist payload when it should be visible.
  // sessionTodolist 保存需要展示时当前会话的 todolist 载荷。
  const sessionTodolist = hasSelectedChatConversation && props.workbench.sessionTodolist?.items.length
    ? props.workbench.sessionTodolist
    : null;
  // sessionToolPanelVisible opens the floating chat tool panel for Git review or terminal sessions only.
  // sessionToolPanelVisible 仅在存在 Git 审查或终端会话时打开会话浮动工具面板。
  const sessionToolPanelVisible = hasSelectedChatConversation && (
    Boolean(sessionReviewSummary?.branchName) || sessionTerminalSessions.length > 0
  );

  React.useEffect(() => {
    if (!hasSelectedChatConversation || !selectedChatProject?.path) {
      setSessionReviewSummary(null);
      return undefined;
    }
    let cancelled = false;
    const loadSessionReviewSummary = async () => {
      const [unstagedResult, stagedResult] = await Promise.allSettled([
        props.onLoadWorkspaceDiffSnapshot(
          selectedChatProject.path,
          selectedChatProject.id,
          props.selectedConversationId,
          { mode: "unstaged" },
        ),
        props.onLoadWorkspaceDiffSnapshot(
          selectedChatProject.path,
          selectedChatProject.id,
          props.selectedConversationId,
          { mode: "staged" },
        ),
      ]);
      if (cancelled) {
        return;
      }
      const snapshots = [unstagedResult, stagedResult]
        .filter((result): result is PromiseFulfilledResult<WorkspaceDiffSnapshotResponse> => result.status === "fulfilled")
        .map((result) => result.value);
      const primarySnapshot = snapshots[0] ?? null;
      const branchName = primarySnapshot?.currentBranch ?? null;
      if (!branchName) {
        setSessionReviewSummary(null);
        return;
      }
      setSessionReviewSummary({
        additions: snapshots.reduce((total, snapshot) => total + snapshot.additions, 0),
        branchName,
        changedFiles: snapshots.reduce((total, snapshot) => total + snapshot.changedFiles, 0),
        deletions: snapshots.reduce((total, snapshot) => total + snapshot.deletions, 0),
      });
    };
    void loadSessionReviewSummary().catch(() => {
      if (!cancelled) {
        setSessionReviewSummary(null);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [
    hasSelectedChatConversation,
    props.onLoadWorkspaceDiffSnapshot,
    props.selectedConversationId,
    props.workbench.messages.length,
    selectedChatProject?.id,
    selectedChatProject?.path,
  ]);

  // rightPanelLabel describes the page-registered context panel toggle action.
  // rightPanelLabel 描述页面注册的上下文面板切换动作。
  const rightPanelLabel = effectiveRightPanelOpen ? t("contextPanel.close") : t("contextPanel.open");
  // contextPanelToggleAction is registered by chat pages instead of being hardcoded in the header.
  // contextPanelToggleAction 由会话页注册，而不是硬编码在标题栏组件中。
  const contextPanelToggleAction = (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          aria-label={rightPanelLabel}
          aria-pressed={effectiveRightPanelOpen}
          className="icon-button"
          onClick={handleRightPanelToggle}
          size="icon-sm"
          type="button"
          variant={effectiveRightPanelOpen ? "secondary" : "ghost"}
        >
          {effectiveRightPanelOpen ? <PanelRightClose size={16} /> : <PanelRightOpen size={16} />}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{rightPanelLabel}</TooltipContent>
    </Tooltip>
  );
  // skillHeaderSummary stores the active skills page title detail for descriptor generation.
  // skillHeaderSummary 保存当前技能页 descriptor 使用的标题详情。
  const skillHeaderSummary = activeSkillHeaderSummary(props.activeSkillSubArea, props.workbench, t);
  // skillHeaderActions stores the registered actions for the active skills sub page.
  // skillHeaderActions 保存当前技能子页面注册的标题栏动作。
  const skillHeaderActions = React.useMemo(() => {
    if (props.activeSkillSubArea === "lua") {
      return (
        <div className="feature-header-actions workspace-skills-header-actions">
          <button
            className="skill-action-button secondary"
            disabled={skillHeaderBusyAction !== null}
            onClick={() => void runSkillHeaderAction("lua-refresh", props.skillActions.onRefreshLuaSkills)}
            type="button"
          >
            <RefreshCw className={skillHeaderBusyAction === "lua-refresh" ? "spin" : undefined} size={15} />
            <span>{skillHeaderBusyAction === "lua-refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}</span>
          </button>
          <button className="skill-action-button" onClick={() => setLuaInstallDialogOpen(true)} type="button">
            <PackagePlus size={15} />
            <span>{t("skills.action.installLua")}</span>
          </button>
        </div>
      );
    }
    return (
      <div className="feature-header-actions workspace-skills-header-actions">
        <button
          className="skill-action-button secondary"
          disabled={skillHeaderBusyAction !== null}
          onClick={() => void runSkillHeaderAction("agent-refresh", props.skillActions.onRefreshAgentSkills)}
          type="button"
        >
          <RefreshCw className={skillHeaderBusyAction === "agent-refresh" ? "spin" : undefined} size={15} />
          <span>{skillHeaderBusyAction === "agent-refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}</span>
        </button>
        <button
          className="skill-action-button secondary"
          disabled={skillHeaderBusyAction !== null}
          onClick={() => void runSkillHeaderAction("agent-sync", props.skillActions.onInstallSystemAgentSkills)}
          type="button"
        >
          <PackagePlus className={skillHeaderBusyAction === "agent-sync" ? "spin" : undefined} size={15} />
          <span>{skillHeaderBusyAction === "agent-sync" ? t("skills.action.syncing") : t("skills.action.syncSystem")}</span>
        </button>
      </div>
    );
  }, [props.activeSkillSubArea, props.skillActions, runSkillHeaderAction, skillHeaderBusyAction, t]);
  // memoryHeaderMeta stores the active memory page title detail for descriptor generation.
  // memoryHeaderMeta 保存当前记忆页 descriptor 使用的标题详情。
  const memoryHeaderMeta = memorySubAreaMeta(props.activeMemorySubArea, props.workbench);

  React.useEffect(() => {
    const element = shellRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      const width = entry.contentRect.width;
      const previousWidth = previousShellWidthRef.current;
      const isGrowing = previousWidth !== null && width > previousWidth + 1;
      previousShellWidthRef.current = width;
      const expandedLeftWidth = 288;
      const collapsedLeftWidth = 46;
      const preferredRightWidth = 480;
      const handleWidth = 9;
      const leftWidth = sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
      const dockedPanelFits = width - leftWidth - preferredRightWidth - handleWidth >= 800;
      const shouldFloatRightPanel = effectiveRightPanelOpen && !dockedPanelFits;
      const dockedRightWidth = effectiveRightPanelOpen && !shouldFloatRightPanel ? preferredRightWidth : 0;
      const shellGaps = effectiveRightPanelOpen && !shouldFloatRightPanel ? handleWidth : 0;
      const mainWidth = width - leftWidth - dockedRightWidth - shellGaps;
      if (effectiveRightPanelOpen) {
        if (rightPanelFloating !== shouldFloatRightPanel) {
          notifyConversationLayoutWillChange();
        }
        setRightPanelFloating(shouldFloatRightPanel);
      }
      if (mainWidth < 800 && sidebar.open) {
        notifyConversationLayoutWillChange();
        sidebar.setOpen(false);
        setLeftAutoCollapsed(true);
        return;
      }
      if (!isGrowing) {
        return;
      }
      if (leftAutoCollapsed && !sidebar.open) {
        const restoredRightWidth = effectiveRightPanelOpen && !shouldFloatRightPanel ? preferredRightWidth : 0;
        const restoredShellGaps = effectiveRightPanelOpen && !shouldFloatRightPanel ? handleWidth : 0;
        const restoredMainWidth = width - expandedLeftWidth - restoredRightWidth - restoredShellGaps;
        if (restoredMainWidth >= 800) {
          notifyConversationLayoutWillChange();
          sidebar.setOpen(true);
          setLeftAutoCollapsed(false);
          return;
        }
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [effectiveRightPanelOpen, leftAutoCollapsed, notifyConversationLayoutWillChange, rightPanelFloating, sidebar]);

  // pageDescriptor registers page-specific title, actions, content, and adaptive side panels.
  // pageDescriptor 注册页面专用标题、按钮、内容与自适应侧边面板。
  const pageDescriptor: WorkspacePageDescriptor = props.mainView === "skills" ? {
    content: (
      <WorkspaceSkillsSettings
        activeSubArea={props.activeSkillSubArea}
        luaInstallDialogOpen={luaInstallDialogOpen}
        onLuaInstallDialogOpenChange={setLuaInstallDialogOpen}
        skillActions={props.skillActions}
        workbench={props.workbench}
      />
    ),
    detail: `${skillSubAreaLabel(props.activeSkillSubArea)} · ${skillHeaderSummary.detail}`,
    headerActions: skillHeaderActions,
    title: "技能设置",
    toolPanels: [{
      className: "workspace-tool-panel-page workspace-skills-settings",
      contentClassName: "workspace-skills-content",
      contentShellClassName: "workspace-skills-content-shell",
      expanded: skillsMenuExpanded,
      onLayoutModeWillChange: notifyConversationLayoutWillChange,
      panel: (
        <FloatingSkillMenu
          activeSubArea={props.activeSkillSubArea}
          expanded={skillsMenuExpanded}
          onExpandedChange={setSkillsMenuExpanded}
          onSelectSubArea={props.onSelectSkillSubArea}
        />
      ),
      side: "left",
    }],
  } : props.mainView === "memory" ? {
    content: (
      <WorkspaceMemorySettings
        activeSubArea={props.activeMemorySubArea}
        memoryActions={props.memoryActions}
        workbench={props.workbench}
      />
    ),
    detail: `${memoryHeaderMeta.label} · ${memoryHeaderMeta.detail}`,
    headerActions: null,
    title: "记忆设置",
    toolPanels: [{
      className: "workspace-tool-panel-page workspace-skills-settings workspace-memory-settings",
      contentClassName: "workspace-skills-content workspace-memory-content",
      contentShellClassName: "workspace-skills-content-shell",
      expanded: memoryMenuExpanded,
      onLayoutModeWillChange: notifyConversationLayoutWillChange,
      panel: (
        <FloatingMemoryMenu
          activeSubArea={props.activeMemorySubArea}
          expanded={memoryMenuExpanded}
          onExpandedChange={setMemoryMenuExpanded}
          onSelectSubArea={props.onSelectMemorySubArea}
          workbench={props.workbench}
        />
      ),
      side: "left",
    }],
  } : {
    content: (
      <ChatArea
        startup={props.startup}
        workbench={props.workbench}
        selectedDraftConversation={props.selectedDraftConversation}
        selectedConversationId={props.selectedConversationId}
        loadingConversationId={props.loadingConversationId}
        conversationLoadError={props.conversationLoadError}
        conversationHydration={props.conversationHydration}
        conversationHasMoreHistory={props.conversationHasMoreHistory}
        conversationHistoryLoading={props.conversationHistoryLoading}
        sessionReviewSummary={sessionReviewSummary}
        onSelectWorkbenchModel={props.onSelectWorkbenchModel}
        onListWorkbenchModelSecondaryRoutes={props.onListWorkbenchModelSecondaryRoutes}
        onSelectWorkbenchModelSecondaryRoute={props.onSelectWorkbenchModelSecondaryRoute}
        onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
        onSlashCommand={props.onSlashCommand}
        onSubmitPrompt={props.onSubmitPrompt}
        onSaveAiCapabilityConfiguration={props.onSaveAiCapabilityConfiguration}
        onSaveSessionGoal={props.onSaveSessionGoal}
        onResumeSessionGoal={props.onResumeSessionGoal}
        onClearSessionGoal={props.onClearSessionGoal}
        onUpdateSessionGoalStatus={props.onUpdateSessionGoalStatus}
        onSetSessionPlanMode={props.onSetSessionPlanMode}
        onStopConversationRun={props.onStopConversationRun}
        onOpenSubagentPanel={props.onOpenSubagentPanel}
        onCloseSubagentPanel={props.onCloseSubagentPanel}
        onStopSubagentTask={props.onStopSubagentTask}
        onEditPendingPrompt={props.onEditPendingPrompt}
        onInsertPendingPrompt={props.onInsertPendingPrompt}
        onRemovePendingPrompt={props.onRemovePendingPrompt}
        onReorderPendingPrompts={props.onReorderPendingPrompts}
        onStageAttachmentBytes={props.onStageAttachmentBytes}
        onCopyText={props.onCopyText}
        onGetUserPromptEditDraft={props.onGetUserPromptEditDraft}
        onSubmitUserPromptEdit={props.onSubmitUserPromptEdit}
        onForkConversation={props.onForkConversation}
        onSelectConversation={props.onSelectConversation}
        onLoadConversationHistory={props.onLoadConversationHistory}
        onLoadToolDetail={props.onLoadToolDetail}
        resolveMediaSourceUrl={props.resolveMediaSourceUrl}
        layoutStabilityKey={[
          props.selectedConversationId ?? "none",
          props.selectedDraftConversation?.id ?? "persisted",
          sessionTodolist ? "todolist-progress" : "no-todolist-progress",
          props.workbench.sessionGoal ? `goal-${props.workbench.sessionGoal.status}` : "no-goal",
          sessionReviewSummary?.branchName ? "review-summary" : "no-review-summary",
          sessionTerminalSessions.length > 0 ? `terminal-${sessionTerminalSessions.length}` : "no-terminal",
          sessionToolPanelVisible && sessionToolExpanded ? "review-expanded" : "review-collapsed",
          effectiveRightPanelOpen ? "context-open" : "context-closed",
          rightPanelFloating ? "context-floating" : "context-docked",
          sidebar.open ? "sidebar-open" : "sidebar-collapsed",
        ].join("|")}
        onTrace={props.onTrace}
      />
    ),
    headerRightActions: hasSelectedChatConversation ? contextPanelToggleAction : undefined,
    toolPanels: [{
      className: "conversation-session-tool-adaptive-panel",
      contentClassName: "conversation-session-tool-content",
      contentShellClassName: "conversation-session-tool-content-shell",
      expanded: sessionToolPanelVisible && sessionToolExpanded,
      inlineGap: 52,
      minContentWidth: 800,
      onLayoutModeWillChange: notifyConversationLayoutWillChange,
      panel: sessionToolPanelVisible ? (
        <ConversationSessionTodolistPanel
          expanded={sessionToolExpanded}
          reviewSummary={sessionReviewSummary}
          terminalSessions={sessionTerminalSessions}
          todolist={null}
          onExpandedChange={handleSessionToolExpandedChange}
          onOpenReviewChanges={handleOpenReviewChanges}
          onStopCommandSession={props.onStopCommandSession}
        />
      ) : null,
      panelWidth: 320,
      reserveGap: 72,
      side: "right",
    }],
  };

  return (
    <div className="workspace-panel-shell" ref={shellRef}>
      <ProjectSidebar
        projects={props.workbench.projects}
        conversations={props.workbench.conversations}
        expandedProjectIds={props.workbench.expandedProjectIds}
        pinnedConversationIds={props.pinnedConversationIds}
        selectedConversationId={props.selectedConversationId}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={props.onCreateConversation}
        onCreateProject={props.onCreateProject}
        onOpenSkills={props.onOpenSkills}
        onOpenMemory={props.onOpenMemory}
        onSelectArea={props.onSelectArea}
        memoryActive={props.mainView === "memory"}
        skillsActive={props.mainView === "skills"}
        onExpandedProjectIdsChange={props.onExpandedProjectIdsChange}
        onProjectMenuAction={props.onProjectMenuAction}
        onReorderProjects={props.onReorderProjects}
        onSelectConversation={props.onSelectConversation}
      />
      <SidebarInset className="workspace-inset">
        <WorkspaceResizableBody
          rightPanelFloating={hasSelectedChatConversation && rightPanelFloating}
          rightPanelOpen={effectiveRightPanelOpen}
          contextPanelActiveTabId={contextTabState.activeTabId}
          contextPanelTabs={contextTabState.tabs}
          reviewFocusRequest={reviewFocusRequest}
          startup={props.startup}
          workbench={props.workbench}
          selectedDraftConversation={props.selectedDraftConversation}
          selectedConversation={props.selectedConversation}
          selectedConversationId={props.selectedConversationId}
          headerActions={pageDescriptor.headerActions}
          chromeActions={props.chromeActions}
          titlebarDragRegion={props.titlebarDragRegion}
          onTitlebarDoubleClick={props.onTitlebarDoubleClick}
          headerDetail={pageDescriptor.detail}
          headerTitle={pageDescriptor.title}
          headerRightActions={pageDescriptor.headerRightActions}
          toolPanels={pageDescriptor.toolPanels}
          onToggleLeftPanel={handleLeftPanelToggle}
          onToggleRightPanel={handleRightPanelToggle}
          onActivateContextTab={activateContextTab}
          onCloseContextTab={closeContextTab}
          onFileContextTabTitleChange={updateFileContextTabTitle}
          onOpenContextTab={openContextTab}
          onOpenSubagentPanel={props.onOpenSubagentPanel}
          onCloseSubagentPanel={props.onCloseSubagentPanel}
          onStopSubagentTask={props.onStopSubagentTask}
          onStopCommandSession={props.onStopCommandSession}
          onCopyRawDiff={(rawDiff) => void props.onCopyText(rawDiff)}
          onCopyText={(text) => void props.onCopyText(text)}
          parseConversationBlocks={parseConversationBlocks}
          resolveMediaSourceUrl={props.resolveMediaSourceUrl}
          loadWorkspaceDiffFileDetail={props.onLoadWorkspaceDiffFileDetail}
          loadWorkspaceDiffSearch={props.onLoadWorkspaceDiffSearch}
          loadWorkspaceDiffSnapshot={props.onLoadWorkspaceDiffSnapshot}
          loadWorkspaceFileDirectory={props.onLoadWorkspaceFileDirectory}
          loadWorkspaceFilePreview={props.onLoadWorkspaceFilePreview}
          runWorkspaceGitFileAction={props.onRunWorkspaceGitFileAction}
          onRunVmmAction={props.memoryActions.onRunVmmAction}
          onTrace={props.onTrace}
        >
          {pageDescriptor.content}
        </WorkspaceResizableBody>
        {props.footer ? (
          <div className="workspace-statusbar-host">
            {props.footer}
          </div>
        ) : null}
      </SidebarInset>
    </div>
  );
}
