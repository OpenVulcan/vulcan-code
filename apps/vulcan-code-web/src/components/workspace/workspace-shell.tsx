import { WorkspaceDirectoryLayout } from "@/components/workspace/workspace-directory-layout";
import { FeatureLoadBoundary } from "@/components/features/feature-shell-content";
import React from "react";
import { toast } from "sonner";
import { createWorkspaceGitRefreshOwnership } from "@/components/workspace/workspace-shell/git-refresh-ownership";
import { useVceProjectVisibility } from "@/components/features/code-context/vce-project-surface";
import { ChatArea } from "@/components/conversation/chat-area";
import {
  ConversationSessionCommitDialog,
  ConversationSessionTodolistPanel,
  type ConversationSessionCommitBusyState,
} from "@/components/conversation/session-todolist-panel";
import type { ConversationSessionGitBranchActions } from "@/components/conversation/session-todolist-panel/git-branch-controls";
import { dispatchConversationLayoutWillChange } from "@/components/conversation/conversation-layout-events";
import { terminalSessionViewModel } from "@/components/conversation/conversation-terminal-model";
import { activeSkillHeaderSummary } from "@/components/features/skills/skill-area-header";
import type { SkillHeaderBusyActionId } from "@/components/features/skills/skill-area-types";
import { activeAgentsHeaderSummary } from "@/components/features/agents/agent-area-header";
import { agentSkillsCatalogSummary } from "@/components/features/agent-skills/agent-skills-types";
import {
  ProjectSidebar,
} from "@/components/project/project-sidebar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  PluginWorkspaceMenu,
  skillSubAreaLabel,
  WorkspaceSkillsSettings,
} from "@/components/workspace/workspace-skills-settings";
import {
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
  selectConversationRightPluginMounts,
  WORKSPACE_CONVERSATION_RESERVED_WIDTH,
  WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH,
  WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH,
} from "@/components/workspace/workspace-resizable-body";
import { observeCoreBrowserSessions } from "@/service/core-browser-metadata";
import { readElectronLocalBrowserBridge } from "@/host-shell/electron-bridge";
import { readConversationLinkPreference } from "@/components/conversation/conversation-link-preference";
import {
  type WorkspaceContextTabKind,
} from "@/components/workspace/workspace-context-panel";
import type {
  DiffReviewSelection,
  WorkspaceDiffSnapshotResponse,
  WorkspaceReviewFocusRequest,
} from "@/diff/types";
import {
  loadWorkspaceGitBranches,
  loadWorkspaceGitCatalog,
  loadWorkspaceGitHistory,
  setWorkspaceGitMonitoring,
  switchWorkspaceGitBranch,
} from "@/service/workbench-snapshot-service";
import { refreshCommandSessions } from "@/service/conversation-actions-service";
import {
  selectConversationSessionTerminalMutationTokenV1,
  selectLiveConversationSessionTerminalsV1,
  selectConversationSessionTodolistV1,
} from "@/service/conversation-session-tools-v1";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import { conversationSelectionEqual, useSelectedConversationEventWindowSelector } from "@/state/conversation-event-window-react";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { WorkspaceGitChangedEvent } from "@/types";
import type { WorkspacePageDescriptor } from "@/components/workspace/workspace-page-descriptor";
import { useI18n, formatMessage } from "@/i18n";
import {
  PanelRightClose,
  PanelRightOpen,
  PackagePlus,
  RefreshCw,
  TerminalSquare,
} from "lucide-react";

import {
  activateWorkspaceContextTab,
  closeWorkspaceContextTab,
  emptyWorkspaceContextTabState,
  normalizeWorkspaceContextTabState,
  openWorkspaceContextTab,
  openWorkspaceSubagentContextTab,
  renameWorkspaceFileContextTab,
  trimWorkspaceContextTabStateCache,
  workspaceSubagentTabId,
  workspaceContextTabStateCache,
  workspaceContextTabCacheKey,
  type WorkspaceContextTabStateEntry,
} from "@/components/workspace/workspace-shell/context-tabs";
import {
  buildWorkspaceProjectGitSummary,
  type WorkspaceProjectGitRepositoryState,
  type WorkspaceProjectGitStateMap,
} from "@/components/workspace/workspace-shell/git-summary";
import type { WorkspaceShellProps } from "@/components/workspace/workspace-shell/types";
export type { WorkspaceMainView } from "@/components/workspace/workspace-shell/types";

// LazyAgentsArea keeps the independent agent editor outside the console-first chunk.
// LazyAgentsArea 将独立智能体编辑器保留在控制台优先分块之外。
const LazyAgentsArea = React.lazy(() => import("@/components/features/agents/agents-area").then((module) => ({ default: module.AgentsArea })));
// LazyAgentCategoryMenu defers agent directory navigation with its owning feature.
// LazyAgentCategoryMenu 将智能体目录导航与其所属功能一同延迟。
const LazyAgentCategoryMenu = React.lazy(() => import("@/components/features/agents/agents-area").then((module) => ({ default: module.AgentCategoryMenu })));
// LazyAgentSkillsArea keeps the prompt-skill editor outside the console-first chunk.
// LazyAgentSkillsArea 将提示词技能编辑器保留在控制台优先分块之外。
const LazyAgentSkillsArea = React.lazy(() => import("@/components/features/agent-skills/agent-skills-area").then((module) => ({ default: module.AgentSkillsArea })));
// LazyAgentSkillCategoryMenu defers prompt-skill directory navigation.
// LazyAgentSkillCategoryMenu 延迟提示词技能目录导航。
const LazyAgentSkillCategoryMenu = React.lazy(() => import("@/components/features/agent-skills/agent-skills-area").then((module) => ({ default: module.AgentSkillCategoryMenu })));
// LazyKnowledgeBaseArea keeps knowledge management outside the console-first chunk.
// LazyKnowledgeBaseArea 将知识库管理保留在控制台优先分块之外。
const LazyKnowledgeBaseArea = React.lazy(() => import("@/components/features/knowledge/knowledge-base-area").then((module) => ({ default: module.KnowledgeBaseArea })));
// WorkspaceShell renders the shadcn dashboard-like fused sidebar and inset console area.
// WorkspaceShell 渲染类似 shadcn dashboard 的融合侧栏与 inset 控制台区域。
// selectEventWindowOwnerSessionId reads the selected V1 owner id with a stable selector identity.
// selectEventWindowOwnerSessionId 以稳定的选择器身份读取选中的 V1 归属 id。
function selectEventWindowOwnerSessionId(snapshot: ConversationEventWindowControllerSnapshot): string | null {
  return snapshot.owner?.session_id ?? null;
}

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
type WorkspaceShellSubagentNavigation = NonNullable<WorkspaceShellProps["workbench"]["subagentNavigation"]>;
type WorkspaceShellSubagentTask = NonNullable<WorkspaceShellSubagentNavigation["tasks"]>[number];

// resolveWorkspaceSubagentTask maps a tool-row selector to the backend UUID task used by the side panel.
// resolveWorkspaceSubagentTask 将工具行选择器映射为右侧面板使用的后端 UUID 任务。
function resolveWorkspaceSubagentTask(
  // Current subagent navigation snapshot for the selected parent session.
  // 当前选中父会话的子代理导航快照。
  navigation: WorkspaceShellSubagentNavigation | null,
  // Selector supplied by a floating panel row or transcript tool row.
  // 浮动面板行或转录工具行提供的选择器。
  selector: string,
): WorkspaceShellSubagentTask | null {
  const normalizedSelector = selector.trim();
  if (!navigation || !normalizedSelector) {
    return null;
  }
  return navigation.tasks?.find((task) => (
    task.taskId === normalizedSelector
    || task.publicTaskId === normalizedSelector
    || task.label === normalizedSelector
  )) ?? null;
}

// WorkspaceShellContent renders the workspace shell after sidebar context is available.
// WorkspaceShellContent 在侧栏上下文可用后渲染工作台外壳。
// Parameters: props supplies workspace authorities and page actions. Returns the sidebar, selected page and owned floating panels.
// 参数：props 提供工作区权威数据和页面动作。返回侧栏、选中页面及其持有的浮动面板。
function WorkspaceShellContent(props: WorkspaceShellProps) {
  // Shared semantic state can make the floating panel visible without Git, terminals, or agents.
  // 共享语义状态可独立于 Git、终端或智能体使浮动面板可见。
  const semanticNodeVisible = useVceProjectVisibility();
  // sidebar controls the left project tree that can collapse automatically.
  // sidebar 控制可自动折叠的左侧项目树。
  const sidebar = useSidebar();
  // t translates shared workspace chrome labels.
  // t 翻译共享工作区外壳标签。
  const { locale, t } = useI18n();
  // eventWindowOwnerSessionId is the selected V1 owner; the shell reads only derived facts from the event window,
  // so streaming deltas do not re-render the sidebar and the rest of the shell.
  // eventWindowOwnerSessionId 是选中的 V1 归属；外壳只从事件窗口读取派生事实，使流式增量不会重新渲染侧栏和
  // 外壳其余部分。
  const eventWindowOwnerSessionId = useSelectedConversationEventWindowSelector(selectEventWindowOwnerSessionId);
  // routeDraftConversation ends draft-only chrome once the exact durable summary is present.
  // routeDraftConversation 在精确持久摘要出现后结束仅草稿界面状态。
  const routeDraftConversation = props.selectedConversation ? null : props.selectedDraftConversation;
  // sidebarNavigationTargetId keeps both sidebar highlights on the clicked target until a failed selection reveals the original route.
  // sidebarNavigationTargetId 使侧栏两处高亮跟随点击目标，选择失败并恢复原路由时一同回退。
  const sidebarNavigationTargetId = props.conversationNavigation
    && (props.conversationNavigation.phase !== "entering" || props.selectedConversationId === props.conversationNavigation.targetId)
    ? props.conversationNavigation.targetId
    : null;
  // sidebarActiveProjectId follows the clicked conversation while project tool ownership stays authoritative.
  // sidebarActiveProjectId 在点击时跟随目标会话，同时项目工具归属仍以权威状态为准。
  const sidebarActiveProjectId = sidebarNavigationTargetId
    ? props.workbench.conversations.find((conversation) => conversation.id === sidebarNavigationTargetId)?.projectId ?? props.activeProjectId
    : props.activeProjectId;
  // The selected V1 owner is the reveal gate for a navigated durable session.
  // 选中 V1 归属是持久会话导航入场的显示门槛。
  React.useLayoutEffect(() => {
    const navigation = props.conversationNavigation;
    if (navigation?.phase !== "waiting-owner" || props.selectedConversationId !== navigation.targetId) return;
    if (!hasServiceRuntime() || eventWindowOwnerSessionId === navigation.targetId) {
      props.onConversationNavigationReady?.(navigation.targetId);
    }
  }, [eventWindowOwnerSessionId, props.conversationNavigation, props.onConversationNavigationReady, props.selectedConversationId]);
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
    () => workspaceContextTabCacheKey(props.workbench, props.selectedConversationId, routeDraftConversation),
    [props.selectedConversationId, routeDraftConversation, props.workbench],
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
  // aiBrowserState is the last owner-scoped live Core browser listing observed by this client.
  // aiBrowserState 是本客户端最后一次观察到的、按归属限定的 Core 浏览器实时列表。
  const [aiBrowserState, setAiBrowserState] = React.useState<{ owner: string | null; ids: string[] }>({ owner: null, ids: [] });
  // observedBrowserIds remembers each conversation's live browsers without reopening a manually closed panel on return.
  // observedBrowserIds 记住各会话的活跃浏览器，避免返回会话时重新打开用户手动关闭的面板。
  const observedBrowserIds = React.useRef(new Map<string, Set<string>>());
  // aiBrowserPluginEnabled gates discovery and UI on the actual enabled plugin mount.
  // aiBrowserPluginEnabled 以真实启用的插件挂载点限定发现与界面。
  const aiBrowserPluginEnabled = React.useMemo(() => selectConversationRightPluginMounts(props.workbench.plugins)
    .some(({ plugin, mount }) => plugin.id === "vulcan-browser" && mount.id === "core-browser"), [props.workbench.plugins]);
  // reviewFocusRequest increments whenever chat tools ask the review panel to show uncommitted changes.
  // reviewFocusRequest 在会话工具要求审核面板显示未提交变更时递增。
  const [reviewFocusRequest, setReviewFocusRequest] = React.useState<WorkspaceReviewFocusRequest>({
    sequence: 0,
    repositoryKey: null,
  });
  // leftAutoCollapsed records automatic left-sidebar collapse caused by width pressure.
  // leftAutoCollapsed 记录宽度压力导致的左侧栏自动折叠。
  const [leftAutoCollapsed, setLeftAutoCollapsed] = React.useState(false);
  // selectedAgentCategoryId stores the active agents page category filter.
  // selectedAgentCategoryId 保存智能体页当前分类筛选。
  const [selectedAgentCategoryId, setSelectedAgentCategoryId] = React.useState("");
  // selectedAgentSkillCategoryId stores the active Agent Skills category filter.
  // selectedAgentSkillCategoryId 保存提示词技能页当前分类筛选。
  const [selectedAgentSkillCategoryId, setSelectedAgentSkillCategoryId] = React.useState("");
  // sessionToolExpanded stores the shared adaptive menu state for the chat tool panel.
  // sessionToolExpanded 保存会话工具共享自适应面板展开状态。
  const [sessionToolExpanded, setSessionToolExpanded] = React.useState(false);
  // workspaceGitStateByProject stores project-level Git state shared by all conversations.
  // workspaceGitStateByProject 保存所有会话共享的项目级 Git 状态。
  const [workspaceGitStateByProject, setWorkspaceGitStateByProject] = React.useState<WorkspaceProjectGitStateMap>({});
  // workspaceGitStateByProjectRef exposes the latest committed project map to long-running refresh callbacks.
  // workspaceGitStateByProjectRef 向长生命周期刷新回调暴露最近提交的项目状态映射。
  const workspaceGitStateByProjectRef = React.useRef<WorkspaceProjectGitStateMap>({});
  workspaceGitStateByProjectRef.current = workspaceGitStateByProject;
  // selectedGitRepositoryKey stores the repository node used by commit and push actions.
  // selectedGitRepositoryKey 保存提交与推送动作当前使用的仓库节点。
  const [selectedGitRepositoryKey, setSelectedGitRepositoryKey] = React.useState<string | null>(null);
  // activeGitCommitRepositoryKey freezes the repository bound to an open or running commit dialog.
  // activeGitCommitRepositoryKey 固定已打开或正在运行的提交弹窗所绑定的仓库。
  const [activeGitCommitRepositoryKey, setActiveGitCommitRepositoryKey] = React.useState<string | null>(null);
  // gitCommitDialogOpen stores the shared commit dialog state across chat and review surfaces.
  // gitCommitDialogOpen 保存聊天区与审核区共享的提交弹窗状态。
  const [gitCommitDialogOpen, setGitCommitDialogOpen] = React.useState(false);
  // gitCommitBusyStateByWorkspace stores running Git actions by workspace path.
  // gitCommitBusyStateByWorkspace 按工作区路径保存正在执行的 Git 动作状态。
  const [gitCommitBusyStateByWorkspace, setGitCommitBusyStateByWorkspace] = React.useState<Record<string, NonNullable<ConversationSessionCommitBusyState>>>({});
  // workspaceGitRefreshOwner restricts success, failure and selection writes to their current project request.
  // workspaceGitRefreshOwner 将成功、失败与选择写入限制到所属项目的当前请求。
  const workspaceGitRefreshOwner = React.useRef(createWorkspaceGitRefreshOwnership());
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
  // skillHeaderActionRunningRef closes the synchronous repeat-click window before React commits busy state.
  // skillHeaderActionRunningRef 在 React 提交忙碌状态前关闭同步重复点击窗口。
  const skillHeaderActionRunningRef = React.useRef(false);
  // hasSelectedChatConversation is true only for an existing persisted conversation surface.
  // hasSelectedChatConversation 仅在当前界面是已存在的持久化会话时为真。
  const hasSelectedChatConversation = props.mainView === "chat" && !routeDraftConversation && Boolean(props.selectedConversationId);
  // aiBrowserAvailable never carries a previous conversation's browser into the selected route.
  // aiBrowserAvailable 绝不会把上一会话的浏览器带入当前路由。
  const aiBrowserAvailable = aiBrowserPluginEnabled && aiBrowserState.owner === props.selectedConversationId
    && aiBrowserState.ids.length > 0;
  // localBrowserAvailable follows this GUI window's native capability, independent of plugins and Core sessions.
  // localBrowserAvailable 取决于此 GUI 窗口的原生能力，与插件和 Core 会话无关。
  const localBrowserAvailable = readElectronLocalBrowserBridge() !== null;
  // selectedChatProject resolves the selected persisted conversation project for Git review summaries.
  // selectedChatProject 解析当前持久化会话所属项目以加载 Git 审查摘要。
  const selectedChatProject = hasSelectedChatConversation && props.selectedConversation?.projectId
    ? props.workbench.projects.find((project) => project.id === props.selectedConversation?.projectId) ?? null
    : null;
  // selectedGitCommitWorkspaceKey identifies the Git workspace whose commit state is currently shown.
  // selectedGitCommitWorkspaceKey 标识当前展示提交状态的 Git 工作区。
  const selectedGitCommitWorkspaceKey = selectedChatProject?.path ?? null;
  // selectedGitWorkspaceRef prevents delayed refreshes from changing another project's visible selection.
  // selectedGitWorkspaceRef 防止迟到刷新改变另一项目的可见仓库选择。
  const selectedGitWorkspaceRef = React.useRef(selectedGitCommitWorkspaceKey);
  selectedGitWorkspaceRef.current = selectedGitCommitWorkspaceKey;
  // selectedWorkspaceGitState stores the selected project's shared Git state entry.
  // selectedWorkspaceGitState 保存当前选中项目的共享 Git 状态条目。
  const selectedWorkspaceGitState = selectedGitCommitWorkspaceKey
    ? workspaceGitStateByProject[selectedGitCommitWorkspaceKey] ?? null
    : null;
  // sessionReviewRepositories exposes every independent repository node to the compact panel.
  // sessionReviewRepositories 向紧凑面板暴露全部独立仓库节点。
  const sessionReviewRepositories = selectedWorkspaceGitState?.repositories ?? [];
  // selectedGitRepositoryState stores the repository currently targeted by commit and push.
  // selectedGitRepositoryState 保存提交与推送当前指向的仓库。
  const enabledReviewRepositories = sessionReviewRepositories.filter((repository) => repository.monitoringEnabled);
  // sessionReviewSummary exposes only an actively monitored primary summary to action surfaces.
  // sessionReviewSummary 只向操作界面暴露正在监听的主要仓库摘要。
  const sessionReviewSummary = enabledReviewRepositories.length > 0
    ? enabledReviewRepositories.find((repository) => repository.repositoryKey === selectedGitRepositoryKey)?.summary
      ?? enabledReviewRepositories.find((repository) => repository.repositoryKey === selectedWorkspaceGitState?.primaryRepositoryKey)?.summary
      ?? enabledReviewRepositories.find((repository) => repository.summary)?.summary
      ?? null
    : null;
  const selectedGitRepositoryState = selectedGitRepositoryKey
    ? enabledReviewRepositories.find((repository) => repository.repositoryKey === selectedGitRepositoryKey)
      ?? enabledReviewRepositories.find((repository) => repository.repositoryKey === selectedWorkspaceGitState?.primaryRepositoryKey)
      ?? enabledReviewRepositories[0]
      ?? null
    : enabledReviewRepositories.find((repository) => repository.repositoryKey === selectedWorkspaceGitState?.primaryRepositoryKey)
      ?? enabledReviewRepositories[0]
      ?? null;
  // selectedGitRepositoryForActions always has a deterministic repository key when a catalog exists.
  // selectedGitRepositoryForActions 在存在仓库目录时始终具有确定性仓库键。
  const selectedGitRepositoryForActions = selectedGitRepositoryState?.repositoryKey ?? null;
  // selectedGitRepositoryForActionsRef lets a selection made in the same click reach the dialog opener before React re-renders.
  // selectedGitRepositoryForActionsRef 让同一次点击中的仓库选择在 React 重渲染前即可被提交弹窗读取。
  const selectedGitRepositoryForActionsRef = React.useRef(selectedGitRepositoryForActions);
  selectedGitRepositoryForActionsRef.current = selectedGitRepositoryForActions;
  // activeGitCommitRepositoryState keeps the dialog target independent from right-panel selection changes.
  // activeGitCommitRepositoryState 让提交弹窗目标独立于右侧面板选择变化。
  const activeGitCommitRepositoryState = activeGitCommitRepositoryKey
    ? enabledReviewRepositories.find((repository) => repository.repositoryKey === activeGitCommitRepositoryKey) ?? null
    : selectedGitRepositoryState;
  // activeGitCommitRepositoryForActions never falls back after a target has been captured.
  // activeGitCommitRepositoryForActions 在捕获目标后绝不回退到其他仓库，避免弹窗或异步动作漂移。
  const activeGitCommitRepositoryForActions = activeGitCommitRepositoryKey !== null
    ? activeGitCommitRepositoryState?.repositoryKey ?? null
    : selectedGitRepositoryForActions;
  // activeSessionReviewSummary is the independent repository summary used by commit controls.
  // activeSessionReviewSummary 是提交控件使用的独立仓库摘要。
  const activeSessionReviewSummary = activeGitCommitRepositoryState
    ? activeGitCommitRepositoryState.summary
    : sessionReviewSummary;
  // gitCommitBusyState stores the selected workspace's shared Git action phase.
  // gitCommitBusyState 保存当前选中工作区的共享 Git 操作阶段。
  const gitCommitBusyState = selectedGitCommitWorkspaceKey && selectedGitRepositoryForActions
    ? gitCommitBusyStateByWorkspace[`${selectedGitCommitWorkspaceKey}\u0000${selectedGitRepositoryForActions}`] ?? null
    : null;
  // updateGitCommitBusyState updates the running Git action state for the captured workspace.
  // updateGitCommitBusyState 更新当前闭包捕获工作区的 Git 动作状态。
  const updateGitCommitBusyState = React.useCallback((repositoryKey: string | null, state: ConversationSessionCommitBusyState) => {
    const workspaceKey = selectedGitCommitWorkspaceKey && repositoryKey
      ? `${selectedGitCommitWorkspaceKey}\u0000${repositoryKey}`
      : null;
    if (!workspaceKey) {
      return;
    }
    setGitCommitBusyStateByWorkspace((current) => {
      if (state) {
        return { ...current, [workspaceKey]: state };
      }
      if (!Object.prototype.hasOwnProperty.call(current, workspaceKey)) {
        return current;
      }
      const next = { ...current };
      delete next[workspaceKey];
      return next;
    });
  }, [selectedGitCommitWorkspaceKey]);
  // gitCommitBusyStateForRepository exposes independent action phases without cross-repository bleed.
  // gitCommitBusyStateForRepository 暴露独立操作阶段，避免仓库之间串状态。
  const gitCommitBusyStateForRepository = React.useCallback((repositoryKey: string) => {
    if (!selectedGitCommitWorkspaceKey) {
      return null;
    }
    return gitCommitBusyStateByWorkspace[
      `${selectedGitCommitWorkspaceKey}\u0000${repositoryKey}`
    ] ?? null;
  }, [gitCommitBusyStateByWorkspace, selectedGitCommitWorkspaceKey]);
  // activeGitCommitBusyState shows the action phase for the repository frozen by the dialog.
  // activeGitCommitBusyState 显示提交弹窗固定仓库对应的操作阶段。
  const activeGitCommitBusyState = activeGitCommitRepositoryForActions
    ? gitCommitBusyStateForRepository(activeGitCommitRepositoryForActions)
    : gitCommitBusyState;
  // sessionTerminalSessions uses the live runtime as the sole authority for currently visible terminals.
  // sessionTerminalSessions 使用实时运行时作为当前可见终端的唯一权威。
  const sessionTerminalSessions = React.useMemo(
    () => hasSelectedChatConversation && props.selectedConversationId
      ? selectLiveConversationSessionTerminalsV1(
        props.selectedConversationId,
        props.workbench.commandSessions ?? [],
      )
      : [],
    [hasSelectedChatConversation, props.selectedConversationId, props.workbench.commandSessions],
  );
  // terminalMutationToken identifies every completed terminal operation in the selected durable V1 window.
  // terminalMutationToken 标识选中可靠 V1 窗口内每次完成的终端操作。
  const terminalMutationToken = useSelectedConversationEventWindowSelector(React.useCallback(
    (snapshot: ConversationEventWindowControllerSnapshot) => snapshot.owner?.session_id === props.selectedConversationId
      ? selectConversationSessionTerminalMutationTokenV1(snapshot)
      : null,
    [props.selectedConversationId],
  ));
  React.useEffect(() => {
    if (!hasSelectedChatConversation || !terminalMutationToken) {
      return;
    }
    // cancelled prevents a late recovery failure from outliving the selected conversation surface.
    // cancelled 防止迟到的恢复失败越过当前选中会话界面。
    let cancelled = false;
    void refreshCommandSessions().catch((error) => {
      if (!cancelled) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
    });
    return () => {
      cancelled = true;
    };
  }, [hasSelectedChatConversation, terminalMutationToken]);
  // visibleSessionTerminalSessions mirrors the terminal panel's own render filter.
  // visibleSessionTerminalSessions 对齐终端面板自身的可渲染过滤条件。
  const visibleSessionTerminalSessions = React.useMemo(
    () => sessionTerminalSessions
      .map((summary) => terminalSessionViewModel(summary, t))
      .filter((session) => session.canStop),
    [sessionTerminalSessions, t],
  );
  // effectiveRightPanelOpen suppresses stale right-panel state while a draft conversation is selected.
  // effectiveRightPanelOpen 在选中新建草稿会话时屏蔽残留的右侧面板状态。
  const effectiveRightPanelOpen = hasSelectedChatConversation && rightPanelOpen;
  // activeContextTab is the exact right-panel tab currently rendered when the panel is visible.
  // activeContextTab 是右侧面板可见时当前真实渲染的页签。
  const activeContextTab = effectiveRightPanelOpen
    ? contextTabState.tabs.find((tab) => tab.id === contextTabState.activeTabId) ?? null
    : null;

  React.useEffect(() => {
    if (props.mainView !== "chat" || routeDraftConversation !== null) {
      props.onNotificationPresentationContextChange({
        conversationViewActive: false,
        visibleConversationId: null,
      });
      return;
    }
    if (activeContextTab?.kind === "subagent") {
      // subagentTaskId is required by the subagent-tab construction contract; an invalid tab disables suppression.
      // subagentTaskId 由子代理页签构造契约强制要求；无效页签会禁用抑制。
      const subagentTaskId = activeContextTab.subagentTaskId;
      props.onNotificationPresentationContextChange({
        conversationViewActive: typeof subagentTaskId === "string" && subagentTaskId.length > 0,
        visibleConversationId: typeof subagentTaskId === "string" && subagentTaskId.length > 0
          ? subagentTaskId
          : null,
      });
      return;
    }
    props.onNotificationPresentationContextChange({
      conversationViewActive: props.selectedConversationId !== null,
      visibleConversationId: props.selectedConversationId,
    });
  }, [
    activeContextTab,
    props.mainView,
    props.onNotificationPresentationContextChange,
    props.selectedConversationId,
    routeDraftConversation,
  ]);
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
      state: openWorkspaceContextTab(current.state, kind, t),
    }));
  }, [t]);
  // browserClosePending prevents duplicate native confirmation dialogs before React updates.
  // browserClosePending 在 React 更新前阻止重复原生确认弹窗。
  const browserClosePending = React.useRef(false);
  // closeContextTab removes tabId after browser confirmation and resource release; returns immediately.
  // closeContextTab 在浏览器确认及资源释放后移除 tabId；调用立即返回。
  const closeContextTab = React.useCallback((tabId: string) => {
    // tab resolves the exact singleton being closed rather than the currently selected tab.
    // tab 定位正在关闭的精确单实例，而不是当前选中的页签。
    const tab = contextTabState.tabs.find((candidate) => candidate.id === tabId);
    if (tab?.kind === "local-browser") {
      if (browserClosePending.current || !props.selectedConversationId) return;
      // bridge closes only this conversation's GUI-owned pages, never Core AI pages.
      // bridge 仅关闭此会话由 GUI 持有的页面，绝不关闭 Core AI 页面。
      const bridge = readElectronLocalBrowserBridge();
      if (!bridge) return;
      browserClosePending.current = true;
      void bridge.closeOwner(props.selectedConversationId, locale).then((closed) => {
        if (!closed) return;
        setContextTabStateEntry((current) => current.cacheKey === contextTabCacheKey
          ? { ...current, state: closeWorkspaceContextTab(current.state, tabId) } : current);
      }).catch((error: unknown) => {
        toast.error(error instanceof Error ? error.message : String(error));
      }).finally(() => { browserClosePending.current = false; });
      return;
    }
    setContextTabStateEntry((current) => ({
      ...current,
      state: closeWorkspaceContextTab(current.state, tabId),
    }));
  }, [contextTabCacheKey, contextTabState.tabs, locale, props.selectedConversationId]);
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
      state: renameWorkspaceFileContextTab(current.state, tabId, title, t),
    }));
  }, [t]);
  // openRightContextPanel opens the context rail and chooses floating mode from the current shell width.
  // openRightContextPanel 打开上下文栏，并根据当前外壳宽度选择浮动模式。
  const openRightContextPanel = React.useCallback(() => {
    notifyConversationLayoutWillChange();
    const width = shellRef.current?.getBoundingClientRect().width ?? 0;
    const expandedLeftWidth = 288;
    const collapsedLeftWidth = 46;
    const leftWidth = sidebar.isMobile ? 0 : sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
    setRightPanelFloating(width > 0 && width - leftWidth - WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH
      - WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH < WORKSPACE_CONVERSATION_RESERVED_WIDTH);
    setRightPanelOpen(true);
  }, [notifyConversationLayoutWillChange, sidebar.isMobile, sidebar.open]);

  // selectedConversationForLocalLinks rejects a link request after navigation changes its owner.
  // selectedConversationForLocalLinks 在会话切换后拒绝旧会话的链接请求。
  const selectedConversationForLocalLinks = React.useRef(props.selectedConversationId);
  selectedConversationForLocalLinks.current = props.selectedConversationId;
  // openConversationLinkInLocalBrowser keeps normal webpage clicks in this GUI and this conversation.
  // openConversationLinkInLocalBrowser 将普通网页点击限定在当前 GUI 与当前会话的浏览器中。
  const openConversationLinkInLocalBrowser = React.useCallback((event: React.MouseEvent<HTMLElement>): void => {
    if (!hasSelectedChatConversation || !props.selectedConversationId || !localBrowserAvailable
      || !readConversationLinkPreference()
      || event.defaultPrevented || event.button !== 0 || event.ctrlKey || event.metaKey
      || event.shiftKey || event.altKey || !(event.target instanceof Element)) return;
    const anchor = event.target.closest<HTMLAnchorElement>("a[href]");
    // Only transcript content participates; adjacent plugin mounts and other workspace controls do not.
    // 仅会话转录内容参与此行为；相邻插件挂载区与其他工作台控件不参与。
    if (!anchor?.closest(".conversation-scroll-content")) return;
    // Resource links are Core-owned downloads and previews, not navigable conversation websites.
    // 资源链接是 Core 管理的下载与预览，不属于会话中的网页导航。
    if (!anchor || !event.currentTarget.contains(anchor) || anchor.hasAttribute("download")
      || anchor.closest(".conversation-v1-resource")) return;
    let destination: URL;
    try {
      destination = new URL(anchor.href);
    } catch {
      return;
    }
    if ((destination.protocol !== "http:" && destination.protocol !== "https:")
      || destination.origin === window.location.origin) return;
    const owner = props.selectedConversationId;
    const bridge = readElectronLocalBrowserBridge();
    if (!bridge) return;
    event.preventDefault();
    event.stopPropagation();
    void (async () => {
      await bridge.setOwner(owner);
      if (selectedConversationForLocalLinks.current !== owner) return;
      await bridge.open(destination.href);
      if (selectedConversationForLocalLinks.current !== owner) return;
      setContextTabStateEntry((current) => current.cacheKey === contextTabCacheKey
        ? { ...current, state: openWorkspaceContextTab(current.state, "local-browser", t) } : current);
      openRightContextPanel();
    })().catch((error: unknown) => {
      toast.error(`${t("workspace.couldNotOpenLink")}: ${error instanceof Error ? error.message : String(error)}`);
    });
  }, [contextTabCacheKey, hasSelectedChatConversation, localBrowserAvailable, locale,
    openRightContextPanel, props.selectedConversationId, t]);
  React.useEffect(() => {
    if (!hasSelectedChatConversation || !props.selectedConversationId || !aiBrowserPluginEnabled) {
      setAiBrowserState({ owner: null, ids: [] });
      return;
    }
    // owner and cancelled bind every asynchronous result to the exact visible conversation.
    // owner 与 cancelled 将每次异步结果绑定到精确的当前可见会话。
    const owner = props.selectedConversationId;
    let cancelled = false;
    // failureLogged keeps normal Core restarts from flooding the GUI console every poll.
    // failureLogged 防止正常重启 Core 时每轮轮询都刷满 GUI 控制台。
    let failureLogged = false;
    const stop = observeCoreBrowserSessions(owner, {
      next(sessions) {
        if (cancelled) return;
        failureLogged = false;
        // Keep crashed browsers discoverable; only an explicit close removes the diagnostic panel.
        // 保留崩溃浏览器的可见入口；只有显式关闭才移除诊断面板。
        const ids = sessions.filter((session) => session.status === "running" || session.status === "crashed").map((session) => session.sessionId);
        setAiBrowserState((current) => current.owner === owner && current.ids.length === ids.length
          && current.ids.every((id, index) => id === ids[index]) ? current : { owner, ids });
        const previous = observedBrowserIds.current.get(owner) ?? new Set<string>();
        const newlyCreated = ids.some((id) => !previous.has(id));
        observedBrowserIds.current.set(owner, new Set(ids));
        if (ids.length === 0) {
          setContextTabStateEntry((current) => current.cacheKey === contextTabCacheKey
            && current.state.tabs.some((tab) => tab.id === "browser")
            ? { ...current, state: closeWorkspaceContextTab(current.state, "browser") } : current);
        } else if (newlyCreated) {
          setContextTabStateEntry((current) => current.cacheKey === contextTabCacheKey
            ? { ...current, state: openWorkspaceContextTab(current.state, "browser", t) } : current);
          openRightContextPanel();
        }
      },
      error(error) {
        if (!cancelled && !failureLogged) {
          failureLogged = true;
          console.warn("Failed to refresh conversation browser list", error);
        }
      },
    });
    return () => { cancelled = true; stop(); };
  }, [aiBrowserPluginEnabled, contextTabCacheKey, hasSelectedChatConversation, openRightContextPanel, props.selectedConversationId, t]);
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
    openRightContextPanel();
  }, [notifyConversationLayoutWillChange, openRightContextPanel, rightPanelOpen]);

  // handleOpenReviewChanges opens the right panel and focuses the live uncommitted review source.
  // handleOpenReviewChanges 打开右侧面板并聚焦实时未提交审核来源。
  const handleOpenReviewChanges = React.useCallback((repositoryKey?: string) => {
    if (!hasSelectedChatConversation) {
      return;
    }
    const nextRepositoryKey = repositoryKey ?? selectedGitRepositoryForActions;
    if (nextRepositoryKey) {
      setSelectedGitRepositoryKey(nextRepositoryKey);
    }
    openContextTab("review");
    setReviewFocusRequest((current) => ({
      sequence: current.sequence + 1,
      repositoryKey: nextRepositoryKey,
    }));
    openRightContextPanel();
  }, [hasSelectedChatConversation, openContextTab, openRightContextPanel, selectedGitRepositoryForActions]);

  // handleSelectReviewRepository updates the visible selection and freezes the next commit dialog target.
  // handleSelectReviewRepository 更新可见选择，并固定下一次提交弹窗目标。
  const handleSelectReviewRepository = React.useCallback((repositoryKey: string) => {
    selectedGitRepositoryForActionsRef.current = repositoryKey;
    setSelectedGitRepositoryKey(repositoryKey);
    // Keep the open or running shared action bound to its original repository.
    // 保持已打开或正在运行的共享动作绑定原始仓库。
    if (!gitCommitDialogOpen && !activeGitCommitBusyState) {
      setActiveGitCommitRepositoryKey(repositoryKey);
    }
  }, [activeGitCommitBusyState, gitCommitDialogOpen]);

  // handleGitCommitDialogOpenChange preserves the selected repository identity across dialog close and reopen.
  // handleGitCommitDialogOpenChange 在弹窗关闭与重开之间保留所选仓库身份。
  const handleGitCommitDialogOpenChange = React.useCallback((open: boolean) => {
    const repositoryKey = selectedGitRepositoryForActionsRef.current;
    if (open && !gitCommitDialogOpen && repositoryKey) {
      setActiveGitCommitRepositoryKey(repositoryKey);
    }
    setGitCommitDialogOpen(open);
  }, [gitCommitDialogOpen]);

  // handleOpenGitCommitDialog opens the shared dialog against the currently selected repository.
  // handleOpenGitCommitDialog 针对当前选中仓库打开共享提交弹窗。
  const handleOpenGitCommitDialog = React.useCallback(() => {
    if (!selectedGitRepositoryForActions) {
      return;
    }
    setActiveGitCommitRepositoryKey(selectedGitRepositoryForActions);
    setGitCommitDialogOpen(true);
  }, [selectedGitRepositoryForActions]);

  // handleOpenSubagentPanel opens a read-only subagent transcript tab in the right context panel.
  // handleOpenSubagentPanel 在右侧上下文面板中打开只读子代理转录页签。
  const handleOpenSubagentPanel = React.useCallback(async (parentConversationId: string, taskId: string): Promise<boolean> => {
    // navigation stores the current backend task projection when it belongs to the requested parent.
    // navigation 保存请求父会话对应的当前后端任务投影。
    const navigation = props.workbench.subagentNavigation?.parentConversationId === parentConversationId
      ? props.workbench.subagentNavigation
      : null;
    // task stores the visible task row used for the tab title when available.
    // task 保存可用时用于页签标题的可见任务行。
    const task = resolveWorkspaceSubagentTask(navigation, taskId);
    const resolvedTaskId = task?.taskId ?? taskId;
    const subagentTabId = workspaceSubagentTabId(parentConversationId, resolvedTaskId);
    // Open the side tab optimistically so panel routing feedback is immediate.
    // 乐观打开侧栏页签，让面板路由反馈立即可见。
    setContextTabStateEntry((current) => ({
      ...current,
      state: openWorkspaceSubagentContextTab(
        current.state,
        parentConversationId,
        resolvedTaskId,
        task?.label ?? resolvedTaskId,
        t,
      ),
    }));
    openRightContextPanel();
    const opened = await props.onOpenSubagentPanel(parentConversationId, resolvedTaskId);
    if (!opened) {
      // Roll back only the optimistic subagent tab when the backend cannot recover that child transcript.
      // 后端无法恢复该子会话转录时，只回滚本次乐观打开的子智能体页签。
      setContextTabStateEntry((current) => ({
        ...current,
        state: closeWorkspaceContextTab(current.state, subagentTabId),
      }));
      return false;
    }
    return true;
  }, [openRightContextPanel, props.onOpenSubagentPanel, props.workbench.subagentNavigation, t]);

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
  //
  // Parameters: actionId selects the busy indicator and action performs the registered service request.
  // 参数：actionId 选择忙碌提示，action 执行已注册的服务请求。
  // Returns: a promise that settles after feedback timing and the synchronous guard are released.
  // 返回值：在反馈时长结束且同步守卫释放后结束的 Promise。
  const runSkillHeaderAction = React.useCallback(async (actionId: SkillHeaderBusyActionId, action: () => Promise<void>) => {
    if (skillHeaderActionRunningRef.current) {
      return;
    }
    skillHeaderActionRunningRef.current = true;
    setSkillHeaderBusyAction(actionId);
    const startedAt = window.performance.now();
    try {
      await action();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      const remainingMs = 450 - (window.performance.now() - startedAt);
      if (remainingMs > 0) {
        await new Promise((resolve) => {
          window.setTimeout(resolve, remainingMs);
        });
      }
      skillHeaderActionRunningRef.current = false;
      setSkillHeaderBusyAction(null);
    }
  }, []);

  // sessionTodolist restores the latest exact todolist from the selected V1 event generation.
  // sessionTodolist 从选中的 V1 事件代次恢复最近一份精确待办清单。
  const sessionTodolist = useSelectedConversationEventWindowSelector(React.useCallback(
    (snapshot: ConversationEventWindowControllerSnapshot) => hasSelectedChatConversation
      && snapshot.owner?.session_id === props.selectedConversationId
      ? selectConversationSessionTodolistV1(snapshot)
      : null,
    [hasSelectedChatConversation, props.selectedConversationId],
  ), conversationSelectionEqual);
  // sessionGitRepositoryAvailable tracks whether the current chat project resolved as a Git repository.
  // sessionGitRepositoryAvailable 标记当前聊天项目是否已解析为 Git 仓库。
  const sessionGitRepositoryAvailable = Boolean(sessionReviewRepositories.length || sessionReviewSummary || selectedWorkspaceGitState?.loading);
  // sessionGitCommitActionAvailable tracks whether commit or push has a runnable target.
  // sessionGitCommitActionAvailable 标记提交或推送是否存在可执行目标。
  const sessionGitCommitActionAvailable = Boolean(activeSessionReviewSummary && (
    activeSessionReviewSummary.changedFiles > 0
    || activeSessionReviewSummary.unpushedCommits > 0
  ));
  // sessionGitCommitActionVisible keeps shared commit triggers visible for Git repositories or running Git work.
  // sessionGitCommitActionVisible 在 Git 仓库或正在执行 Git 工作时保持共享提交入口可见。
  const sessionGitCommitActionVisible = sessionGitRepositoryAvailable || Boolean(gitCommitBusyState);
  // sessionSubagentNavigation keeps subagent tool rows scoped to the selected parent conversation.
  // sessionSubagentNavigation 将子代理工具行限定在当前选中的父会话范围内。
  const sessionSubagentNavigation = hasSelectedChatConversation
    && props.workbench.subagentNavigation?.parentConversationId === props.selectedConversationId
    ? props.workbench.subagentNavigation
    : null;
  // visibleSessionSubagentTaskCount mirrors the backend unclosed-agent navigation count.
  // visibleSessionSubagentTaskCount 镜像后端未关闭代理导航数量。
  const visibleSessionSubagentTaskCount = React.useMemo(
    () => (sessionSubagentNavigation?.tasks ?? []).length,
    [sessionSubagentNavigation?.tasks],
  );
  // sessionToolPanelVisible opens the floating chat tool panel only when it has renderable content.
  // sessionToolPanelVisible 仅在浮动工具面板存在可渲染内容时打开。
  const sessionToolPanelVisible = (props.mainView === "chat" && semanticNodeVisible) || (hasSelectedChatConversation && (
    sessionGitCommitActionVisible
    || visibleSessionTerminalSessions.length > 0
    || visibleSessionSubagentTaskCount > 0
  ));

  // refreshProjectGitSummary reloads the shared Git summary for one project path.
  // refreshProjectGitSummary 重新加载单个项目路径的共享 Git 摘要。
  // Parameters: workspacePath is the cache owner; projectId/conversationId scope IO; optional repositoryKey limits diff refreshes.
  // 参数：workspacePath 是缓存归属；projectId/conversationId 限定输入输出；可选 repositoryKey 限制差异刷新范围。
  // Returns after the current request publishes a summary/error or loses ownership; it never writes another project's selection.
  // 当前请求发布摘要/错误或失去归属后返回；绝不写入另一项目的选择。
  const refreshProjectGitSummary = React.useCallback(async (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    repositoryKey?: string | null,
  ) => {
    // ticket gates every terminal result, including failed catalog reads and visible selection updates.
    // ticket 对所有终态结果设门，包括目录读取失败及可见选择更新。
    const ticket = workspaceGitRefreshOwner.current.begin(workspacePath);
    const requestId = ticket.requestId;
    setWorkspaceGitStateByProject((current) => {
      const previous = current[workspacePath];
      return {
        ...current,
        [workspacePath]: {
          error: null,
          loading: true,
          projectId,
          refreshRequestId: requestId,
          revision: previous?.revision ?? 0,
          stagedSnapshot: previous?.stagedSnapshot ?? null,
          summary: previous?.summary ?? null,
          unstagedSnapshot: previous?.unstagedSnapshot ?? null,
          workspacePath,
          repositories: previous?.repositories.map((repository) => ({
            ...repository,
            error: repositoryKey && repository.repositoryKey !== repositoryKey ? repository.error : null,
            loading: repository.monitoringEnabled
              && (!repositoryKey || repository.repositoryKey === repositoryKey),
          })) ?? [],
          primaryRepositoryKey: previous?.primaryRepositoryKey ?? null,
        },
      };
    });
    let catalog;
    try {
      catalog = await loadWorkspaceGitCatalog(workspacePath, projectId, conversationId);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setWorkspaceGitStateByProject((current) => workspaceGitRefreshOwner.current.owns(ticket) ? ({
        ...current,
        [workspacePath]: {
          ...(current[workspacePath] ?? {
            projectId,
            revision: 0,
            stagedSnapshot: null,
            summary: null,
            unstagedSnapshot: null,
            workspacePath,
            repositories: [],
            primaryRepositoryKey: null,
          }),
          error: message,
          loading: false,
          projectId,
          refreshRequestId: requestId,
        },
      }) : current);
      return;
    }
    if (!workspaceGitRefreshOwner.current.owns(ticket)) return;
    const previousEntry = workspaceGitStateByProjectRef.current[workspacePath];
    const repositoryStates = await Promise.all(catalog.repositories.map(async (repository) => {
      const previousRepository = previousEntry?.repositories.find((entry) => entry.repositoryKey === repository.repositoryKey);
      if (repositoryKey && repository.repositoryKey !== repositoryKey) {
        return previousRepository ?? {
          error: null,
          isPrimary: repository.isPrimary,
          loading: false,
          monitoringEnabled: repository.monitoringEnabled,
          projectId,
          relativePath: repository.relativePath,
          repositoryKey: repository.repositoryKey,
          repositoryName: repository.name,
          revision: 0,
          stagedSnapshot: null,
          summary: null,
          unstagedSnapshot: null,
          workspacePath,
        } satisfies WorkspaceProjectGitRepositoryState;
      }
      if (!repository.monitoringEnabled) {
        return {
          error: null,
          isPrimary: repository.isPrimary,
          loading: false,
          monitoringEnabled: false,
          projectId,
          relativePath: repository.relativePath,
          repositoryKey: repository.repositoryKey,
          repositoryName: repository.name,
          revision: (previousRepository?.revision ?? 0) + 1,
          // Disabled repositories remain visible for settings but must not expose stale review counts.
          // 已关闭监听的仓库仍保留设置入口，但不能继续展示过期审查统计。
          stagedSnapshot: null,
          summary: null,
          unstagedSnapshot: null,
          workspacePath,
        } satisfies WorkspaceProjectGitRepositoryState;
      }
      const [unstagedResult, stagedResult] = await Promise.allSettled([
        props.onLoadWorkspaceDiffSnapshot(workspacePath, projectId, conversationId, {
          mode: "unstaged",
          repositoryKey: repository.repositoryKey,
        }),
        props.onLoadWorkspaceDiffSnapshot(workspacePath, projectId, conversationId, {
          mode: "staged",
          repositoryKey: repository.repositoryKey,
        }),
      ]);
      const rejectedError = [unstagedResult, stagedResult]
        .find((result): result is PromiseRejectedResult => result.status === "rejected")
        ?.reason;
      const unstagedSnapshot = unstagedResult.status === "fulfilled"
        ? unstagedResult.value
        : previousRepository?.unstagedSnapshot ?? null;
      const stagedSnapshot = stagedResult.status === "fulfilled"
        ? stagedResult.value
        : previousRepository?.stagedSnapshot ?? null;
      return {
        error: rejectedError instanceof Error ? rejectedError.message : rejectedError ? String(rejectedError) : null,
        isPrimary: repository.isPrimary,
        loading: false,
        monitoringEnabled: true,
        projectId,
        relativePath: repository.relativePath,
        repositoryKey: repository.repositoryKey,
        repositoryName: repository.name,
        revision: (previousRepository?.revision ?? 0) + 1,
        stagedSnapshot,
        summary: buildWorkspaceProjectGitSummary(unstagedSnapshot, stagedSnapshot),
        unstagedSnapshot,
        workspacePath,
      } satisfies WorkspaceProjectGitRepositoryState;
    }));
    const primaryRepository = repositoryStates.find((repository) => (
      catalog.repositories.find((candidate) => candidate.repositoryKey === repository.repositoryKey)?.isPrimary
    )) ?? repositoryStates[0] ?? null;
    setWorkspaceGitStateByProject((current) => {
      const previous = current[workspacePath];
      if (!workspaceGitRefreshOwner.current.owns(ticket)) {
        return current;
      }
      return {
        ...current,
        [workspacePath]: {
          error: repositoryStates.find((repository) => repository.error)?.error ?? null,
          loading: repositoryStates.some((repository) => repository.loading),
          projectId,
          refreshRequestId: requestId,
          revision: (previous?.revision ?? 0) + 1,
          stagedSnapshot: primaryRepository?.stagedSnapshot ?? null,
          summary: primaryRepository?.summary ?? null,
          unstagedSnapshot: primaryRepository?.unstagedSnapshot ?? null,
          workspacePath,
          repositories: repositoryStates,
          primaryRepositoryKey: primaryRepository?.repositoryKey ?? null,
        },
      };
    });
    setSelectedGitRepositoryKey((current) => (
      !workspaceGitRefreshOwner.current.ownsSelection(ticket, selectedGitWorkspaceRef.current) ? current
        : current && repositoryStates.some((repository) => repository.repositoryKey === current && repository.monitoringEnabled)
        ? current
        : repositoryStates.find((repository) => repository.monitoringEnabled && repository.repositoryKey === primaryRepository?.repositoryKey)?.repositoryKey
          ?? repositoryStates.find((repository) => repository.monitoringEnabled)?.repositoryKey
          ?? null
    ));
  }, [props.onLoadWorkspaceDiffSnapshot]);

  // refreshSessionReviewSummary reloads Git state for the currently selected project.
  // refreshSessionReviewSummary 重新加载当前选中项目的 Git 状态。
  const refreshSessionReviewSummary = React.useCallback(async () => {
    if (!hasSelectedChatConversation || !selectedChatProject?.path) {
      return;
    }
    await refreshProjectGitSummary(
      selectedChatProject.path,
      selectedChatProject.id ?? null,
      props.selectedConversationId,
    );
  }, [
    hasSelectedChatConversation,
    props.selectedConversationId,
    refreshProjectGitSummary,
    selectedChatProject?.id,
    selectedChatProject?.path,
  ]);

  // refreshSelectedGitRepositorySummary refreshes only the repository captured by the commit dialog.
  // refreshSelectedGitRepositorySummary 只刷新提交弹窗捕获的目标仓库。
  const refreshSelectedGitRepositorySummary = React.useCallback(async () => {
    if (!hasSelectedChatConversation || !selectedChatProject?.path || !activeGitCommitRepositoryForActions) {
      return;
    }
    await refreshProjectGitSummary(
      selectedChatProject.path,
      selectedChatProject.id ?? null,
      props.selectedConversationId,
      activeGitCommitRepositoryForActions,
    );
  }, [
    hasSelectedChatConversation,
    props.selectedConversationId,
    refreshProjectGitSummary,
    activeGitCommitRepositoryForActions,
    selectedChatProject?.id,
    selectedChatProject?.path,
  ]);

  // commitBranchPrefix is the configured prefix shared by commit and branch-creation recommendations.
  // commitBranchPrefix 是提交与新建分支推荐共用的已配置前缀。
  const commitBranchPrefix = props.workbench.gitCommit?.newBranchPrefix ?? "vulcan/";

  // sessionGitBranchActions binds branch listing, switching, and history paging to the selected project.
  // sessionGitBranchActions 将分支列表、切换与历史分页绑定到当前选中项目。
  const sessionGitBranchActions = React.useMemo<ConversationSessionGitBranchActions | null>(() => {
    if (!hasSelectedChatConversation || !selectedChatProject?.path) {
      return null;
    }
    const workspacePath = selectedChatProject.path;
    const projectId = selectedChatProject.id ?? null;
    const conversationId = props.selectedConversationId;
    return {
      commitBranchPrefix,
      loadBranches: (repositoryKey) => loadWorkspaceGitBranches(workspacePath, projectId, conversationId, repositoryKey),
      loadHistory: (repositoryKey, skip) => loadWorkspaceGitHistory(workspacePath, projectId, conversationId, repositoryKey, skip),
      switchBranch: async (repositoryKey, branchName, create) => {
        await switchWorkspaceGitBranch(workspacePath, projectId, conversationId, repositoryKey, branchName, create);
        await refreshProjectGitSummary(workspacePath, projectId, conversationId, repositoryKey);
      },
    };
  }, [
    commitBranchPrefix,
    hasSelectedChatConversation,
    props.selectedConversationId,
    refreshProjectGitSummary,
    selectedChatProject?.id,
    selectedChatProject?.path,
  ]);

  React.useEffect(() => {
    if (!hasSelectedChatConversation || !selectedChatProject?.path || !hasServiceRuntime()) {
      return undefined;
    }
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    const workspacePath = selectedChatProject.path;
    const projectId = selectedChatProject.id ?? null;
    const conversationId = props.selectedConversationId;
    const normalizeWorkspacePath = (value: string): string => value.replaceAll("\\", "/").replace(/\/+$/, "").toLocaleLowerCase();
    // pendingRepositoryKeys coalesces bursts of file events so each repository refreshes at most once per drain round.
    // pendingRepositoryKeys 合并突发文件事件，使每个仓库在一轮排空中最多刷新一次。
    const pendingRepositoryKeys = new Set<string>();
    let draining = false;
    // drainPendingRefreshes serializes refreshes and reports one failure per drain instead of one per event.
    // drainPendingRefreshes 串行执行刷新，每轮排空只报告一次失败而不是每个事件都报告。
    const drainPendingRefreshes = async (): Promise<void> => {
      if (draining) {
        return;
      }
      draining = true;
      let firstError: unknown = null;
      try {
        while (!cancelled && pendingRepositoryKeys.size > 0) {
          const repositoryKey = pendingRepositoryKeys.values().next().value as string;
          pendingRepositoryKeys.delete(repositoryKey);
          try {
            await refreshProjectGitSummary(workspacePath, projectId, conversationId, repositoryKey);
          } catch (error: unknown) {
            firstError ??= error;
          }
        }
      } finally {
        draining = false;
      }
      if (firstError !== null && !cancelled) {
        toast.error(firstError instanceof Error ? firstError.message : String(firstError));
      }
    };
    const subscription = serviceClient.subscribe<WorkspaceGitChangedEvent>("workspace.git.changed", (event) => {
      if (cancelled || normalizeWorkspacePath(event.workspacePath) !== normalizeWorkspacePath(workspacePath)) {
        return;
      }
      if (event.projectId && projectId && event.projectId !== projectId) {
        return;
      }
      const repositoryKeys = [...new Set(event.repositoryKeys)].filter((key) => key.trim().length > 0);
      if (repositoryKeys.length === 0) {
        return;
      }
      for (const repositoryKey of repositoryKeys) {
        pendingRepositoryKeys.add(repositoryKey);
      }
      void drainPendingRefreshes();
    });
    void subscription.then((nextUnsubscribe) => {
      if (cancelled) {
        nextUnsubscribe();
      } else {
        unsubscribe = nextUnsubscribe;
      }
    }).catch((error: unknown) => {
      // A failed subscription must remain visible; stale surfaces never report a new connection's failure.
      // 订阅失败必须可见；失效表面不会报告新连接的错误。
      if (!cancelled) toast.error(error instanceof Error ? error.message : String(error));
    });
    return () => {
      cancelled = true;
      unsubscribe?.();
      unsubscribe = null;
      pendingRepositoryKeys.clear();
    };
  }, [
    hasSelectedChatConversation,
    props.selectedConversationId,
    refreshProjectGitSummary,
    selectedChatProject?.id,
    selectedChatProject?.path,
  ]);

  // handleToggleReviewMonitoring persists one repository listener switch and refreshes its own node.
  // handleToggleReviewMonitoring 持久化单个仓库监听开关并刷新该仓库节点。
  // Parameters: repositoryKey identifies the exact node and enabled is the requested persisted state. Returns a rejecting promise on failure.
  // 参数：repositoryKey 标识精确节点，enabled 是请求持久化的状态。失败时返回拒绝的 Promise。
  const handleToggleReviewMonitoring = React.useCallback(async (repositoryKey: string, enabled: boolean) => {
    if (!selectedChatProject?.path || !selectedChatProject.id || !props.selectedConversationId) {
      return;
    }
    await setWorkspaceGitMonitoring(
      selectedChatProject.path,
      selectedChatProject.id,
      props.selectedConversationId,
      repositoryKey,
      enabled,
    );
    await refreshProjectGitSummary(
      selectedChatProject.path,
      selectedChatProject.id,
      props.selectedConversationId,
    );
    // Only the project still displayed may request a panel catalog refresh after persistence.
    // 持久化结束后，仅仍在显示的项目可以请求面板目录刷新。
    if (selectedGitWorkspaceRef.current === selectedChatProject.path) {
      setReviewFocusRequest((current) => ({
        repositoryKey: current.repositoryKey,
        sequence: current.sequence + 1,
      }));
    }
  }, [props.selectedConversationId, refreshProjectGitSummary, selectedChatProject?.id, selectedChatProject?.path]);

  // recordWorkspaceDiffSnapshotForProject merges a panel-loaded snapshot into the shared project Git state.
  // recordWorkspaceDiffSnapshotForProject 将面板加载的快照合并进共享项目 Git 状态。
  const recordWorkspaceDiffSnapshotForProject = React.useCallback((
    workspacePath: string,
    projectId: string | null,
    selection: DiffReviewSelection,
    response: WorkspaceDiffSnapshotResponse,
  ) => {
    if (selection.mode !== "unstaged" && selection.mode !== "staged") {
      return;
    }
    setWorkspaceGitStateByProject((current) => {
      const previous = current[workspacePath];
      if (previous?.loading) {
        return current;
      }
      const repositoryKey = selection.repositoryKey ?? previous?.primaryRepositoryKey ?? ".";
      const previousRepository = previous?.repositories.find((repository) => repository.repositoryKey === repositoryKey);
      const nextUnstagedSnapshot = selection.mode === "unstaged"
        ? response
        : previousRepository?.unstagedSnapshot ?? null;
      const nextStagedSnapshot = selection.mode === "staged"
        ? response
        : previousRepository?.stagedSnapshot ?? null;
      const nextRepository: WorkspaceProjectGitRepositoryState = {
        error: null,
        isPrimary: previousRepository?.isPrimary ?? repositoryKey === (previous?.primaryRepositoryKey ?? "."),
        loading: false,
        monitoringEnabled: previousRepository?.monitoringEnabled ?? true,
        projectId,
        relativePath: previousRepository?.relativePath ?? repositoryKey,
        repositoryKey,
        repositoryName: previousRepository?.repositoryName ?? repositoryKey,
        revision: (previousRepository?.revision ?? 0) + 1,
        stagedSnapshot: nextStagedSnapshot,
        summary: buildWorkspaceProjectGitSummary(nextUnstagedSnapshot, nextStagedSnapshot),
        unstagedSnapshot: nextUnstagedSnapshot,
        workspacePath,
      };
      const repositories = previous?.repositories.filter((repository) => repository.repositoryKey !== repositoryKey) ?? [];
      repositories.push(nextRepository);
      repositories.sort((left, right) => left.repositoryKey.localeCompare(right.repositoryKey));
      const primaryRepository = repositories.find((repository) => repository.repositoryKey === (previous?.primaryRepositoryKey ?? "."))
        ?? repositories[0]
        ?? null;
      return {
        ...current,
        [workspacePath]: {
          error: null,
          loading: false,
          projectId,
          revision: (previous?.revision ?? 0) + 1,
          refreshRequestId: previous?.refreshRequestId ?? 0,
          stagedSnapshot: primaryRepository?.stagedSnapshot ?? null,
          summary: primaryRepository?.summary ?? null,
          unstagedSnapshot: primaryRepository?.unstagedSnapshot ?? null,
          workspacePath,
          repositories,
          primaryRepositoryKey: primaryRepository?.repositoryKey ?? null,
        },
      };
    });
  }, []);

  React.useEffect(() => {
    if (!hasSelectedChatConversation || (gitCommitDialogOpen && !activeSessionReviewSummary)) {
      setGitCommitDialogOpen(false);
    }
  }, [activeSessionReviewSummary, gitCommitDialogOpen, hasSelectedChatConversation]);

  React.useEffect(() => {
    let cancelled = false;
    if (
      hasSelectedChatConversation
      && selectedChatProject?.path
      && !selectedWorkspaceGitState
    ) {
      void refreshSessionReviewSummary().catch((error) => {
        if (!cancelled) {
          const message = error instanceof Error ? error.message : String(error);
          setWorkspaceGitStateByProject((current) => ({
            ...current,
            [selectedChatProject.path]: {
              error: message,
              loading: false,
              projectId: selectedChatProject.id ?? null,
              refreshRequestId: current[selectedChatProject.path]?.refreshRequestId ?? 0,
              revision: current[selectedChatProject.path]?.revision ?? 0,
              stagedSnapshot: current[selectedChatProject.path]?.stagedSnapshot ?? null,
              summary: current[selectedChatProject.path]?.summary ?? null,
              unstagedSnapshot: current[selectedChatProject.path]?.unstagedSnapshot ?? null,
              workspacePath: selectedChatProject.path,
              repositories: current[selectedChatProject.path]?.repositories ?? [],
              primaryRepositoryKey: current[selectedChatProject.path]?.primaryRepositoryKey ?? null,
            },
          }));
        }
      });
    }
    return () => {
      cancelled = true;
    };
  }, [
    hasSelectedChatConversation,
    refreshSessionReviewSummary,
    selectedChatProject?.id,
    selectedChatProject?.path,
    selectedWorkspaceGitState,
  ]);

  // activeTerminalProject is the project-level terminal owner selected by the workbench.
  // activeTerminalProject 是工作台选中的项目级终端归属项目。
  const activeTerminalProject = props.workbench.projects.find(
    (project) => project.id === props.activeProjectId,
  ) ?? null;
  // projectTerminalLabel describes the active project's top-bar terminal toggle action.
  // projectTerminalLabel 描述当前活动项目的顶部栏终端切换操作。
  const projectTerminalLabel = activeTerminalProject
    ? props.projectTerminalRunningCount > 0
      ? formatMessage(t("workspace.terminalToggle.withRunning"), { name: activeTerminalProject.name, count: props.projectTerminalRunningCount })
      : formatMessage(t("workspace.terminalToggle.named"), { name: activeTerminalProject.name })
    : t("workspace.terminalToggle.generic");
  // projectTerminalAction anchors the floating terminal immediately left of the context-panel toggle.
  // projectTerminalAction 将浮动终端入口锚定在上下文面板切换按钮左侧。
  const projectTerminalAction = activeTerminalProject ? (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          ref={props.projectTerminalButtonRef}
          aria-haspopup="dialog"
          aria-label={projectTerminalLabel}
          className="icon-button project-terminal-launch-button"
          onClick={props.onToggleProjectTerminal}
          size={props.projectTerminalRunningCount > 0 ? "sm" : "icon-sm"}
          type="button"
          variant="ghost"
        >
          <TerminalSquare size={16} />
          {props.projectTerminalRunningCount > 0 ? (
            <Badge aria-hidden="true" variant="secondary">
              {props.projectTerminalRunningCount}
            </Badge>
          ) : null}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{projectTerminalLabel}</TooltipContent>
    </Tooltip>
  ) : null;
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
  const skillHeaderSummary = activeSkillHeaderSummary(props.activeSkillSubArea, props.workbench, locale, t);
  // agentsHeaderSummary stores the independent Agents page title detail for descriptor generation.
  // agentsHeaderSummary 保存独立智能体页 descriptor 使用的标题详情。
  const agentsHeaderSummary = activeAgentsHeaderSummary(props.workbench, t);
  // skillHeaderActions stores the registered actions for the active skills sub page.
  // skillHeaderActions 保存当前技能子页面注册的标题栏动作。
  const skillHeaderActions = React.useMemo(() => {
    if (props.activeSkillSubArea === "catalog") {
      return (
        <div className="feature-header-actions workspace-skills-header-actions">
          <Button
            aria-label={skillHeaderBusyAction === "lua-refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}
            title={t("skills.action.refresh")}
            className="feature-header-refresh"
            disabled={skillHeaderBusyAction !== null}
            onClick={() => void runSkillHeaderAction("lua-refresh", props.luaSkillActions.onRefreshLuaSkills)}
            size="sm"
            type="button"
            variant="secondary"
          >
            <RefreshCw className={skillHeaderBusyAction === "lua-refresh" ? "spin" : undefined} size={15} />
            <span>{skillHeaderBusyAction === "lua-refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}</span>
          </Button>
          <Button onClick={() => setLuaInstallDialogOpen(true)} size="sm" type="button">
            <PackagePlus size={15} />
            <span>{t("skills.action.installLua")}</span>
          </Button>
        </div>
      );
    }
    return null;
  }, [props.activeSkillSubArea, props.luaSkillActions, runSkillHeaderAction, skillHeaderBusyAction, t]);
  // agentsHeaderActions stays empty because the agents page owns a sticky action bar.
  // agentsHeaderActions 保持为空，因为智能体页面自行持有锁顶操作栏。
  const agentsHeaderActions = null;
  // memoryHeaderMeta stores the active memory page title detail for descriptor generation.
  // memoryHeaderMeta 保存当前记忆页 descriptor 使用的标题详情。
  const memoryHeaderMeta = memorySubAreaMeta(props.activeMemorySubArea, props.workbench, t);

  React.useEffect(() => {
    const element = shellRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      const width = entry.contentRect.width;
      const previousWidth = previousShellWidthRef.current;
      const isGrowing = previousWidth !== null && width > previousWidth + 1;
      // Only initial layout or a real shrink may override desktop visibility; rerenders must respect a manual reopen.
      // 只有初次布局或实际缩窄可以自动折叠；普通重绘不能覆盖用户手动重新展开的选择。
      const canAutoCollapse = previousWidth === null || width < previousWidth - 1;
      previousShellWidthRef.current = width;
      const expandedLeftWidth = 288;
      const collapsedLeftWidth = 46;
      const leftWidth = sidebar.isMobile ? 0 : sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
      const dockedPanelFits = width - leftWidth - WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH
        - WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH >= WORKSPACE_CONVERSATION_RESERVED_WIDTH;
      const shouldFloatRightPanel = effectiveRightPanelOpen && !dockedPanelFits;
      const dockedRightWidth = effectiveRightPanelOpen && !shouldFloatRightPanel ? WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH : 0;
      const shellGaps = effectiveRightPanelOpen && !shouldFloatRightPanel ? WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH : 0;
      const mainWidth = width - leftWidth - dockedRightWidth - shellGaps;
      if (effectiveRightPanelOpen) {
        if (rightPanelFloating !== shouldFloatRightPanel) {
          notifyConversationLayoutWillChange();
        }
        setRightPanelFloating(shouldFloatRightPanel);
      }
      // Phone navigation overlays the workspace; desktop auto-collapse must not dismiss it.
      // 手机导航覆盖于工作区上方，桌面自动折叠不能将其关闭。
      if (sidebar.isMobile) {
        return;
      }
      if (canAutoCollapse && mainWidth < 800 && sidebar.open) {
        notifyConversationLayoutWillChange();
        sidebar.setOpen(false);
        setLeftAutoCollapsed(true);
        return;
      }
      if (!isGrowing) {
        return;
      }
      if (leftAutoCollapsed && !sidebar.open) {
        const restoredRightWidth = effectiveRightPanelOpen && !shouldFloatRightPanel ? WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH : 0;
        const restoredShellGaps = effectiveRightPanelOpen && !shouldFloatRightPanel ? WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH : 0;
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
      <WorkspaceDirectoryLayout contentClassName="workspace-directory-catalog" navigation={
        <PluginWorkspaceMenu
          activeSubArea={props.activeSkillSubArea}
          onSelectSubArea={props.onSelectSkillSubArea}
          workbench={props.workbench}
        />
      }>
        <WorkspaceSkillsSettings
          activeSubArea={props.activeSkillSubArea}
          luaInstallDialogOpen={luaInstallDialogOpen}
          onLuaInstallDialogOpenChange={setLuaInstallDialogOpen}
          skillActions={props.luaSkillActions}
          workbench={props.workbench}
        />
      </WorkspaceDirectoryLayout>
    ),
    detail: `${skillSubAreaLabel(props.activeSkillSubArea, props.workbench, locale, t)} · ${skillHeaderSummary.detail}`,
    headerActions: skillHeaderActions,
    title: t("workspaceShell.skillsTitle"),
  } : props.mainView === "agent-skills" ? {
    content: (
      <WorkspaceDirectoryLayout contentClassName="workspace-directory-catalog" navigation={
        <React.Suspense fallback={null}>
          <LazyAgentSkillCategoryMenu
            selectedCategoryId={selectedAgentSkillCategoryId}
            onSelectCategory={setSelectedAgentSkillCategoryId}
            workbench={props.workbench}
          />
        </React.Suspense>
      }>
        <React.Suspense fallback={null}>
          <LazyAgentSkillsArea
            actions={props.agentSkillActions}
            selectedCategoryId={selectedAgentSkillCategoryId}
            onSelectCategory={setSelectedAgentSkillCategoryId}
            workbench={props.workbench}
          />
        </React.Suspense>
      </WorkspaceDirectoryLayout>
    ),
    detail: agentSkillsCatalogSummary(props.workbench.agentSkills, t),
    headerActions: null,
    title: t("area.agentSkills"),
  } : props.mainView === "agents" ? {
    content: (
      <WorkspaceDirectoryLayout contentClassName="workspace-directory-catalog" navigation={
        <React.Suspense fallback={null}>
          <LazyAgentCategoryMenu
            selectedCategoryId={selectedAgentCategoryId}
            onSelectCategory={setSelectedAgentCategoryId}
            workbench={props.workbench}
          />
        </React.Suspense>
      }>
        <React.Suspense fallback={null}>
          <LazyAgentsArea
            actions={props.agentActions}
            selectedCategoryId={selectedAgentCategoryId}
            onSelectCategory={setSelectedAgentCategoryId}
            workbench={props.workbench}
          />
        </React.Suspense>
      </WorkspaceDirectoryLayout>
    ),
    detail: agentsHeaderSummary.detail,
    headerActions: agentsHeaderActions,
    title: t("workspaceShell.agentsTitle"),
  } : props.mainView === "memory" ? {
    content: (
      <WorkspaceMemorySettings
        activeSubArea={props.activeMemorySubArea}
        memoryActions={props.memoryActions}
        onOpenCredentialManagement={props.onOpenCredentialManagement}
        onSelectSubArea={props.onSelectMemorySubArea}
        workbench={props.workbench}
      />
    ),
    detail: `${memoryHeaderMeta.label} · ${memoryHeaderMeta.detail}`,
    headerActions: null,
    title: t("workspaceShell.memoryTitle"),

  } : props.mainView === "knowledge" ? {
    content: <React.Suspense fallback={null}><LazyKnowledgeBaseArea /></React.Suspense>,
    detail: t("area.knowledge.description"),
    headerActions: null,
    title: t("area.knowledge"),
  } : {
    content: (
      <ChatArea
        startup={props.startup}
        workbench={props.workbench}
        selectedDraftConversation={props.selectedDraftConversation}
        selectedConversationId={props.selectedConversationId}
        sessionReviewSummary={sessionReviewSummary}
        runtimeStatusHost={props.runtimeStatusHost}
        todolistStatusHost={props.todolistStatusHost}
        onSelectWorkbenchModel={props.onSelectWorkbenchModel}
        onListWorkbenchModelSecondaryRoutes={props.onListWorkbenchModelSecondaryRoutes}
        onSelectWorkbenchModelSecondaryRoute={props.onSelectWorkbenchModelSecondaryRoute}
        onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
        onSlashCommand={props.onSlashCommand}
        onOpenKnowledge={props.onOpenKnowledge}
        onOpenMemorySettings={props.onOpenMemory}
        onSubmitPrompt={props.onSubmitPrompt}
        onSaveSessionGoal={props.onSaveSessionGoal}
        onResumeSessionGoal={props.onResumeSessionGoal}
        onClearSessionGoal={props.onClearSessionGoal}
        onUpdateSessionGoalStatus={props.onUpdateSessionGoalStatus}
        onSetSessionPlanMode={props.onSetSessionPlanMode}
        onSetSessionVmmPolicy={props.onSetSessionVmmPolicy}
        onSetSessionOutputStyle={props.onSetSessionOutputStyle}
        onSetSessionContextCompactionWindow={props.onSetSessionContextCompactionWindow}
        onStopConversationRun={props.onStopConversationRun}
        onContinueConversation={props.onContinueConversation}
        onEditConversationGeneration={props.onEditConversationGeneration}
        onForkConversationGeneration={props.onForkConversationGeneration}
        onOpenSubagentPanel={handleOpenSubagentPanel}
        onStopSubagentTask={props.onStopSubagentTask}
        onEditPendingPrompt={props.onEditPendingPrompt}
        onInsertPendingPrompt={props.onInsertPendingPrompt}
        onRemovePendingPrompt={props.onRemovePendingPrompt}
        onReorderPendingPrompts={props.onReorderPendingPrompts}
        onStageAttachmentBytes={props.onStageAttachmentBytes}
        onCopyText={props.onCopyText}
        resolveMediaSourceUrl={props.resolveMediaSourceUrl}
        onSelectConversation={props.onSelectConversation}
        onFirstSubmissionReconciled={props.onFirstSubmissionReconciled}
        layoutStabilityKey={[
          props.selectedConversationId ?? "none",
          routeDraftConversation?.id ?? "persisted",
          sessionTodolist ? "todolist-progress" : "no-todolist-progress",
          props.workbench.sessionGoal ? `goal-${props.workbench.sessionGoal.status}` : "no-goal",
          sessionReviewSummary && sessionReviewSummary.changedFiles > 0 ? "review-summary" : "no-review-summary",
          visibleSessionTerminalSessions.length > 0 ? `terminal-${visibleSessionTerminalSessions.length}` : "no-terminal",
          visibleSessionSubagentTaskCount > 0 ? `subagent-${visibleSessionSubagentTaskCount}` : "no-subagent",
          sessionToolPanelVisible && sessionToolExpanded ? "review-expanded" : "review-collapsed",
          effectiveRightPanelOpen ? "context-open" : "context-closed",
          rightPanelFloating ? "context-floating" : "context-docked",
          sidebar.open ? "sidebar-open" : "sidebar-collapsed",
        ].join("|")}
        onTrace={props.onTrace}
      />
    ),
    headerRightActions: projectTerminalAction || hasSelectedChatConversation ? (
      <>
        {projectTerminalAction}
        {hasSelectedChatConversation ? contextPanelToggleAction : null}
      </>
    ) : undefined,
    toolPanels: [{
      className: "conversation-session-tool-adaptive-panel",
      contentClassName: "conversation-session-tool-content",
      contentShellClassName: "conversation-session-tool-content-shell",
      expanded: sessionToolPanelVisible && sessionToolExpanded,
      forceOverlay: effectiveRightPanelOpen && rightPanelFloating,
      inlineGap: 52,
      minContentWidth: 800,
      onLayoutModeWillChange: notifyConversationLayoutWillChange,
      panel: sessionToolPanelVisible ? (
        <ConversationSessionTodolistPanel
          activeReviewRepositoryKey={gitCommitDialogOpen ? activeGitCommitRepositoryKey : selectedGitRepositoryForActions}
          commitBusyState={gitCommitBusyState}
          commitBusyStateForRepository={gitCommitBusyStateForRepository}
          commitDialogOpen={gitCommitDialogOpen}
          expanded={sessionToolExpanded}
          gitBranchActions={sessionGitBranchActions}
          reviewSummary={sessionReviewSummary}
          reviewRepositoryOrderStorageKey={selectedGitCommitWorkspaceKey}
          reviewRepositories={sessionReviewRepositories.map((repository) => ({
            error: repository.error,
            isPrimary: repository.isPrimary,
            loading: repository.loading,
            monitoringEnabled: repository.monitoringEnabled,
            relativePath: repository.relativePath,
            repositoryKey: repository.repositoryKey,
            repositoryName: repository.repositoryName,
            summary: repository.monitoringEnabled ? repository.summary : null,
          }))}
          subagentNavigation={sessionSubagentNavigation}
          terminalSessions={sessionTerminalSessions}
          todolist={null}
          onCommitDialogOpenChange={handleGitCommitDialogOpenChange}
          onExpandedChange={handleSessionToolExpandedChange}
          onOpenReviewChanges={handleOpenReviewChanges}
          onSelectReviewRepository={handleSelectReviewRepository}
          onToggleReviewMonitoring={(repositoryKey, enabled) => {
            return handleToggleReviewMonitoring(repositoryKey, enabled).catch((error) => {
              const message = error instanceof Error ? error.message : String(error);
              // Persistence failure must be visible without overwriting a newer catalog snapshot.
              // 持久化失败必须可见，同时不能覆盖较新的目录快照。
              toast.error(message);
            });
          }}
          onOpenSubagentPanel={handleOpenSubagentPanel}
          onStopCommandSession={props.onStopCommandSession}
        />
      ) : null,
      panelWidth: 320,
      reserveGap: 72,
      side: "right",
    }],
  };

  return (
    <div className={`workspace-panel-shell${props.mainView === "memory" ? " vmm-workspace-host" : ""}`} ref={shellRef}>
      <ProjectSidebar
        activeProjectId={sidebarActiveProjectId}
        projects={props.workbench.projects}
        conversations={props.workbench.conversations}
        expandedProjectIds={props.workbench.expandedProjectIds}
        pinnedConversationIds={props.pinnedConversationIds}
        selectedConversationId={sidebarNavigationTargetId ?? props.selectedConversationId}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={props.onCreateConversation}
        onCreateProject={props.onCreateProject}
        onOpenSkills={props.onOpenSkills}
        onOpenAgentSkills={props.onOpenAgentSkills}
        onOpenAgents={props.onOpenAgents}
        onOpenMemory={props.onOpenMemory}
        onOpenKnowledge={props.onOpenKnowledge}
        onSelectArea={props.onSelectArea}
        titlebarDragRegion={props.titlebarDragRegion}
        onTitlebarDoubleClick={props.onTitlebarDoubleClick}
        memoryActive={props.mainView === "memory"}
        knowledgeActive={props.mainView === "knowledge"}
        agentsActive={props.mainView === "agents"}
        skillsActive={props.mainView === "skills"}
        agentSkillsActive={props.mainView === "agent-skills"}
        onExpandedProjectIdsChange={props.onExpandedProjectIdsChange}
        onProjectMenuAction={props.onProjectMenuAction}
        onReorderProjects={props.onReorderProjects}
        onSelectProject={props.onSelectProject}
        onSelectConversation={props.onSelectConversation}
      />
      <SidebarInset className="workspace-inset">
        <WorkspaceResizableBody
          aiBrowserAvailable={aiBrowserAvailable}
          localBrowserAvailable={localBrowserAvailable}
          conversationNavigationPhase={props.mainView === "chat" ? props.conversationNavigation?.phase ?? null : null}
          conversationNavigationShowLoading={props.mainView === "chat" && props.conversationNavigation?.showLoading === true}
          rightPanelFloating={hasSelectedChatConversation && rightPanelFloating}
          rightPanelOpen={effectiveRightPanelOpen}
          contextPanelActiveTabId={contextTabState.activeTabId}
          contextPanelTabs={contextTabState.tabs}
          reviewFocusRequest={reviewFocusRequest}
          startup={props.startup}
          workbench={props.workbench}
          selectedDraftConversation={routeDraftConversation}
          selectedConversation={props.selectedConversation}
          isSelectedConversationPinned={props.selectedConversation ? props.pinnedConversationIds.has(props.selectedConversation.id) : false}
          selectedConversationId={props.selectedConversationId}
          headerActions={pageDescriptor.headerActions}
          chromeActions={props.chromeActions}
          titlebarDragRegion={props.titlebarDragRegion}
          onTitlebarDoubleClick={props.onTitlebarDoubleClick}
          onConversationMenuAction={props.onConversationMenuAction}
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
          onConversationLinkClickCapture={props.mainView === "chat" ? openConversationLinkInLocalBrowser : undefined}
          onOpenSubagentPanel={handleOpenSubagentPanel}
          onStopSubagentTask={props.onStopSubagentTask}
          onCloseSubagentTask={props.onCloseSubagentTask}
          onStopCommandSession={props.onStopCommandSession}
          onCopyRawDiff={props.onCopyText}
          onCopyText={props.onCopyText}
          resolveMediaSourceUrl={props.resolveMediaSourceUrl}
          loadWorkspaceDiffFileDetail={props.onLoadWorkspaceDiffFileDetail}
          loadWorkspaceDiffSearch={props.onLoadWorkspaceDiffSearch}
          loadWorkspaceDiffSnapshot={props.onLoadWorkspaceDiffSnapshot}
          loadWorkspaceFileDirectory={props.onLoadWorkspaceFileDirectory}
          loadWorkspaceFilePreview={props.onLoadWorkspaceFilePreview}
          runWorkspaceGitFileAction={props.onRunWorkspaceGitFileAction}
          onWorkspaceDiffSnapshotLoaded={recordWorkspaceDiffSnapshotForProject}
          onWorkspaceGitStateChanged={refreshProjectGitSummary}
          onSelectGitRepository={setSelectedGitRepositoryKey}
          onRunVmmAction={props.memoryActions.onRunVmmAction}
          onTrace={props.onTrace}
          gitCommitActionBusy={Boolean(gitCommitBusyState)}
          gitCommitActionDisabled={!sessionGitCommitActionAvailable}
          gitCommitActionLabel={gitCommitBusyState?.label ?? t("gitCommitDialog.open")}
          onOpenGitCommitAction={sessionGitCommitActionVisible ? handleOpenGitCommitDialog : undefined}
        >
          {/* Embedded feature imports share recovery with settings, leaving the project navigation mounted. */}
          {/* 内嵌功能导入与设置共用恢复界面，保持项目导航挂载。 */}
          {props.mainView === "chat" ? pageDescriptor.content : (
            <FeatureLoadBoundary regionKey={`${props.mainView}:${props.mainView === "memory" ? props.activeMemorySubArea : props.mainView === "skills" ? props.activeSkillSubArea : ""}`}>
              {pageDescriptor.content}
            </FeatureLoadBoundary>
          )}
        </WorkspaceResizableBody>
        {props.footer ? (
          <div className="workspace-statusbar-host">
            {props.footer}
          </div>
        ) : null}
      </SidebarInset>
      {sessionGitCommitActionVisible && activeSessionReviewSummary ? (
        <ConversationSessionCommitDialog
          busyState={activeGitCommitBusyState}
          commitBranchPrefix={commitBranchPrefix}
          conversationId={props.selectedConversationId}
          open={gitCommitDialogOpen}
          repositoryKey={activeGitCommitRepositoryForActions}
          reviewSummary={activeSessionReviewSummary}
          workspacePath={selectedChatProject?.path ?? null}
          onBusyStateChange={updateGitCommitBusyState}
          onOpenChange={handleGitCommitDialogOpenChange}
          onRefreshReviewSummary={refreshSelectedGitRepositorySummary}
        />
      ) : null}
    </div>
  );
}
