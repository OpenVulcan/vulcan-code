import { WorkspaceDirectoryLayout } from "@/components/workspace/workspace-directory-layout";
import { FeatureLoadBoundary } from "@/components/features/feature-shell-content";
import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { ChatArea } from "@/components/conversation/chat-area";
import {
  ConversationSessionCommitDialog,
  ConversationSessionTodolistPanel,
} from "@/components/conversation/session-todolist-panel";
import { dispatchConversationLayoutWillChange } from "@/components/conversation/conversation-layout-events";
import { terminalSessionViewModel } from "@/components/conversation/conversation-terminal-model";
import { activeSkillHeaderSummary } from "@/components/features/skills/skill-area-header";
import type { SkillHeaderBusyActionId } from "@/components/features/skills/skill-area-types";
import { activeAgentsHeaderSummary } from "@/components/features/agents/agent-area-header";
import { agentSkillsCatalogSummary } from "@/components/features/agent-skills/agent-skills-types";
import {
  ProjectSidebar,
} from "@/components/project/project-sidebar";
import { Button } from "@/ui-kit/primitives/button";
import {
  PluginWorkspaceMenu,
  skillSubAreaLabel,
  WorkspaceSkillsSettings,
} from "@/components/workspace/workspace-skills-settings";
import {
  memorySubAreaMeta,
  MemoryWorkspaceNavigation,
  WorkspaceMemorySettings,
} from "@/components/workspace/workspace-memory-settings";
import { WorkspaceFeaturePanel } from "@/components/workspace/workspace-feature-panel";
import { SidebarInset, SidebarProvider, useSidebar, useSidebarNavigation } from "@/ui-kit/shell/sidebar";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";
import {
  WorkspaceResizableBody,
  selectConversationRightPluginMounts,
} from "@/components/workspace/workspace-resizable-body";
import { observeCoreBrowserSessions } from "@/service/core-browser-metadata";
import { readElectronLocalBrowserBridge } from "@/host-shell/electron-bridge";
import { LocalBrowserCloseDialog } from "@/components/features/browser/local-browser-close-dialog";
import { readConversationLinkPreference } from "@/components/conversation/conversation-link-preference";
import { resolveConversationLinkTarget } from "@/components/conversation/conversation-link-navigation";
import {
  type WorkspaceContextTabKind,
} from "@/components/workspace/workspace-context-panel";
import type { WorkspaceReviewFocusRequest } from "@/diff/types";
import { refreshCommandSessions } from "@/service/conversation-actions-service";
import {
  selectConversationSessionTerminalMutationTokenV1,
  selectLiveConversationSessionTerminalsV1,
  selectConversationSessionTodolistV1,
} from "@/service/conversation-session-tools-v1";
import { hasServiceRuntime } from "@/service/webui-service-runtime";
import { conversationSelectionEqual, useSelectedConversationEventWindowSelector } from "@/state/conversation-event-window-react";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { WorkspacePageDescriptor } from "@/components/workspace/workspace-page-descriptor";
import { useI18n } from "@/i18n";
import {
  PanelRightClose,
  PanelRightOpen,
  PackagePlus,
  RefreshCw,
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
import { useWorkspaceGitState } from "@/components/workspace/workspace-shell/use-workspace-git-state";
import { AssistantDockButton } from "@/components/assistant/assistant-dock-button";
import { TurnChangeReviewActionsContext } from "@/components/conversation/turn-change-review-context";
import { useReviewFocusActions } from "@/components/workspace/workspace-shell/use-review-focus-actions";
import { useWorkspacePanelLayout } from "@/components/workspace/workspace-shell/use-workspace-panel-layout";
import {
  WORKSPACE_FEATURE_AREAS,
  type WorkspaceShellProps,
} from "@/components/workspace/workspace-shell/types";
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
// WorkspaceShell renders the fused sidebar and the inset console area.
// WorkspaceShell 渲染融合侧栏与内嵌控制台区域。
// selectEventWindowOwnerSessionId reads the selected V1 owner id with a stable selector identity.
// selectEventWindowOwnerSessionId 以稳定的选择器身份读取选中的 V1 归属 id。
function selectEventWindowOwnerSessionId(snapshot: ConversationEventWindowControllerSnapshot): string | null {
  return snapshot.owner?.session_id ?? null;
}

export function WorkspaceShell(props: WorkspaceShellProps) {
  // Only the session panel collapses; feature menus stay beside the rail like the settings tree.
  // 只有会话面板可折叠；功能菜单与设置树一样始终位于轨道旁。
  const collapsible = props.mainView === "chat";
  return (
    <SidebarProvider
      collapsible={collapsible}
      open={!collapsible || props.sidebarOpen}
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
  // sidebar controls the left navigation panel (project tree or feature menu) that can collapse automatically.
  // sidebar 控制可自动折叠的左侧导航面板（项目树或功能菜单）。
  const sidebar = useSidebar();
  // navigate closes the phone drawer after a feature menu selection.
  // navigate 在选择功能菜单项后关闭手机抽屉。
  const navigate = useSidebarNavigation();
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
  // selectedAgentCategoryId stores the active agents page category filter.
  // selectedAgentCategoryId 保存智能体页当前分类筛选。
  const [selectedAgentCategoryId, setSelectedAgentCategoryId] = React.useState("");
  // selectedAgentSkillCategoryId stores the active Agent Skills category filter.
  // selectedAgentSkillCategoryId 保存提示词技能页当前分类筛选。
  const [selectedAgentSkillCategoryId, setSelectedAgentSkillCategoryId] = React.useState("");
  // sessionToolExpanded stores the shared adaptive menu state for the chat tool panel.
  // sessionToolExpanded 保存会话工具共享自适应面板展开状态。
  const [sessionToolExpanded, setSessionToolExpanded] = React.useState(false);
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
  // panelLayout opens, floats and auto-collapses the side panels around the measured shell width.
  // panelLayout 根据测量的外壳宽度打开、浮动并自动折叠两侧面板。
  const {
    shellRef,
    rightPanelFloating,
    effectiveRightPanelOpen,
    openRightContextPanel,
    handleLeftPanelToggle,
    handleRightPanelToggle,
  } = useWorkspacePanelLayout({ sidebar, hasSelectedChatConversation, notifyConversationLayoutWillChange });
  // aiBrowserAvailable never carries a previous conversation's browser into the selected route.
  // aiBrowserAvailable 绝不会把上一会话的浏览器带入当前路由。
  const aiBrowserAvailable = aiBrowserPluginEnabled && aiBrowserState.owner === props.selectedConversationId
    && aiBrowserState.ids.length > 0;
  // localBrowserAvailable follows this GUI window's native capability, independent of plugins and Core sessions.
  // localBrowserAvailable 取决于此 GUI 窗口的原生能力，与插件和 Core 会话无关。
  const localBrowserAvailable = readElectronLocalBrowserBridge() !== null;
  // gitState holds the selected project's repositories, commit dialog target and Git actions.
  // gitState 保存当前项目的仓库、提交弹窗目标与 Git 动作。
  const {
    selectedChatProject,
    selectedGitCommitWorkspaceKey,
    sessionReviewRepositories,
    sessionReviewSummary,
    selectedGitRepositoryForActions,
    setSelectedGitRepositoryKey,
    activeGitCommitRepositoryKey,
    activeGitCommitRepositoryForActions,
    activeSessionReviewSummary,
    gitCommitDialogOpen,
    gitCommitBusyState,
    gitCommitBusyStateForRepository,
    activeGitCommitBusyState,
    updateGitCommitBusyState,
    handleSelectReviewRepository,
    handleGitCommitDialogOpenChange,
    handleOpenGitCommitDialog,
    sessionGitCommitActionAvailable,
    sessionGitCommitActionVisible,
    refreshProjectGitSummary,
    refreshSelectedGitRepositorySummary,
    commitBranchPrefix,
    sessionGitBranchActions,
    handleToggleReviewMonitoring,
    recordWorkspaceDiffSnapshotForProject,
  } = useWorkspaceGitState({
    hasSelectedChatConversation,
    selectedConversationId: props.selectedConversationId,
    selectedConversation: props.selectedConversation,
    projects: props.workbench.projects,
    configuredBranchPrefix: props.workbench.gitCommit?.newBranchPrefix,
    onLoadWorkspaceDiffSnapshot: props.onLoadWorkspaceDiffSnapshot,
    setReviewFocusRequest,
  });
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
  // browserCloseRequest binds the shared confirmation dialog to the exact panel and conversation.
  // browserCloseRequest 将共享确认弹窗绑定到精确的面板与会话。
  const [browserCloseRequest, setBrowserCloseRequest] = React.useState<{
    // Conversation whose native pages may be destroyed.
    // 允许销毁原生页面的会话。
    owner: string;
    // Context state identity protected against late completion after navigation.
    // 防止导航后迟到完成写入的上下文状态身份。
    cacheKey: string;
    // Exact browser panel removed only after native destruction succeeds.
    // 仅在原生销毁成功后移除的精确浏览器面板。
    tabId: string;
  } | null>(null);
  React.useEffect(() => { setBrowserCloseRequest(null); }, [contextTabCacheKey]);
  // closeContextTab removes tabId after browser confirmation and resource release; returns immediately.
  // closeContextTab 在浏览器确认及资源释放后移除 tabId；调用立即返回。
  const closeContextTab = React.useCallback((tabId: string) => {
    // tab resolves the exact singleton being closed rather than the currently selected tab.
    // tab 定位正在关闭的精确单实例，而不是当前选中的页签。
    const tab = contextTabState.tabs.find((candidate) => candidate.id === tabId);
    if (tab?.kind === "local-browser") {
      if (!props.selectedConversationId || !readElectronLocalBrowserBridge()) return;
      setBrowserCloseRequest({ owner: props.selectedConversationId, cacheKey: contextTabCacheKey, tabId });
      return;
    }
    setContextTabStateEntry((current) => ({
      ...current,
      state: closeWorkspaceContextTab(current.state, tabId),
    }));
  }, [contextTabCacheKey, contextTabState.tabs, props.selectedConversationId]);
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

  // selectedConversationForLocalLinks rejects a link request after navigation changes its owner.
  // selectedConversationForLocalLinks 在会话切换后拒绝旧会话的链接请求。
  const selectedConversationForLocalLinks = React.useRef(props.selectedConversationId);
  selectedConversationForLocalLinks.current = props.selectedConversationId;
  // openConversationLinkInLocalBrowser routes webpage and file clicks using this GUI's explicit preference.
  // openConversationLinkInLocalBrowser 按当前 GUI 的明确设置路由网页与文件点击。
  const openConversationLinkInLocalBrowser = React.useCallback((event: React.MouseEvent<HTMLElement>): void => {
    if (!hasSelectedChatConversation || !props.selectedConversationId || !localBrowserAvailable) return;
    // The web client has no native bridge and keeps normal browser link navigation.
    // 网页客户端没有原生桥，保留浏览器正常链接导航。
    const destination = resolveConversationLinkTarget(event, window.location.origin);
    if (!destination) return;
    const owner = props.selectedConversationId;
    const bridge = readElectronLocalBrowserBridge();
    if (!bridge) return;
    event.preventDefault();
    event.stopPropagation();
    void (async () => {
      if (!readConversationLinkPreference()) {
        await bridge.openExternalUrl(destination.href);
        return;
      }
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

  // reviewFocus opens the right review panel on live changes or on the last-turn source.
  // reviewFocus 打开右侧审核面板并切到实时变更或上一轮来源。
  const reviewFocus = useReviewFocusActions({
    selectedConversationId: props.selectedConversationId,
    hasSelectedChatConversation,
    selectedGitRepositoryForActions,
    setSelectedGitRepositoryKey,
    setReviewFocusRequest,
    openReviewTab: React.useCallback(() => openContextTab("review"), [openContextTab]),
    openRightContextPanel,
  });

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
  const sessionToolPanelVisible = (hasSelectedChatConversation && (
    sessionGitCommitActionVisible
    || visibleSessionTerminalSessions.length > 0
    || visibleSessionSubagentTaskCount > 0
  ));

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
            <RefreshCw className={skillHeaderBusyAction === "lua-refresh" ? "vk-spin" : undefined} size={15} />
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

  // featureNavigation is the menu a feature page shows in the panel beside the rail, replacing the project tree.
  // featureNavigation 是功能页显示在轨道旁面板中的菜单，用于替换项目树。
  const featureNavigation: React.ReactNode | null = props.mainView === "skills" ? (
    <PluginWorkspaceMenu
      activeSubArea={props.activeSkillSubArea}
      onSelectSubArea={navigate(props.onSelectSkillSubArea)}
      workbench={props.workbench}
    />
  ) : props.mainView === "agent-skills" ? (
    <React.Suspense fallback={null}>
      <LazyAgentSkillCategoryMenu
        selectedCategoryId={selectedAgentSkillCategoryId}
        onSelectCategory={navigate(setSelectedAgentSkillCategoryId)}
        workbench={props.workbench}
      />
    </React.Suspense>
  ) : props.mainView === "agents" ? (
    <React.Suspense fallback={null}>
      <LazyAgentCategoryMenu
        selectedCategoryId={selectedAgentCategoryId}
        onSelectCategory={navigate(setSelectedAgentCategoryId)}
        workbench={props.workbench}
      />
    </React.Suspense>
  ) : props.mainView === "memory" ? (
    <MemoryWorkspaceNavigation
      activeSubArea={props.activeMemorySubArea}
      onSelectSubArea={navigate(props.onSelectMemorySubArea)}
      workbench={props.workbench}
    />
  ) : null;
  // featureArea names the feature page that owns the panel, when one does.
  // featureArea 在功能页持有面板时给出其名称。
  const featureArea = WORKSPACE_FEATURE_AREAS.find((area) => area.id === props.mainView);

  // pageDescriptor registers page-specific title, actions, content, and adaptive side panels.
  // pageDescriptor 注册页面专用标题、按钮、内容与自适应侧边面板。
  const pageDescriptor: WorkspacePageDescriptor = props.mainView === "skills" ? {
    content: (
      <WorkspaceDirectoryLayout contentClassName="workspace-directory-catalog">
        <WorkspaceSkillsSettings
          activeSubArea={props.activeSkillSubArea}
          luaInstallDialogOpen={luaInstallDialogOpen}
          onLuaInstallDialogOpenChange={setLuaInstallDialogOpen}
          onOpenDiagnostics={props.onOpenRuntimeDiagnostics}
          onSelectPluginResourceOwner={props.onSelectPluginResourceOwner}
          skillActions={props.luaSkillActions}
          workbench={props.workbench}
        />
      </WorkspaceDirectoryLayout>
    ),
    // Like settings and memory, the header names the active sub page; the rail already names the area.
    // 与设置和记忆一致，标题栏显示当前子页面名称；侧栏图标已标明所在区域。
    detail: skillHeaderSummary.detail,
    headerActions: skillHeaderActions,
    title: skillSubAreaLabel(props.activeSkillSubArea, props.workbench, locale, t),
  } : props.mainView === "agent-skills" ? {
    content: (
      <WorkspaceDirectoryLayout contentClassName="workspace-directory-catalog">
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
      <WorkspaceDirectoryLayout contentClassName="workspace-directory-catalog">
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
    // The header carries the page title and description; memory pages have no in-page heading.
    // 标题栏承载页面标题与说明；记忆页面内不再有独立标题。
    detail: memoryHeaderMeta.detail,
    headerActions: null,
    title: memoryHeaderMeta.label,

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
    headerRightActions: <>
      <AssistantDockButton />
      {hasSelectedChatConversation ? contextPanelToggleAction : null}
    </>,
    toolPanels: [{
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
          onOpenReviewChanges={reviewFocus.openReviewChanges}
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
    <div className="vk-shell-frame" ref={shellRef}>
      {featureArea ? (
        <WorkspaceFeaturePanel
          navigation={featureNavigation}
          rail={props.rail}
          title={t(featureArea.label)}
          titlebarDragRegion={props.titlebarDragRegion}
          onTitlebarDoubleClick={props.onTitlebarDoubleClick}
        />
      ) : <ProjectSidebar
        activeProjectId={sidebarActiveProjectId}
        projects={props.workbench.projects}
        conversations={props.workbench.conversations}
        expandedProjectIds={props.workbench.expandedProjectIds}
        pinnedConversationIds={props.pinnedConversationIds}
        selectedConversationId={sidebarNavigationTargetId ?? props.selectedConversationId}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={props.onCreateConversation}
        onCreateDedicatedConversation={props.onCreateDedicatedConversation}
        onCreateProject={props.onCreateProject}
        rail={props.rail}
        titlebarDragRegion={props.titlebarDragRegion}
        onTitlebarDoubleClick={props.onTitlebarDoubleClick}
        onExpandedProjectIdsChange={props.onExpandedProjectIdsChange}
        onProjectMenuAction={props.onProjectMenuAction}
        onReorderProjects={props.onReorderProjects}
        onSelectProject={props.onSelectProject}
        onSelectConversation={props.onSelectConversation}
      />}
      <SidebarInset>
        <TurnChangeReviewActionsContext.Provider value={reviewFocus.turnChangeReviewActions}>
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
          <div className="vk-status-frame">
            {props.footer}
          </div>
        ) : null}
        </TurnChangeReviewActionsContext.Provider>
      </SidebarInset>
      {browserCloseRequest && browserCloseRequest.cacheKey === contextTabCacheKey
        && browserCloseRequest.owner === props.selectedConversationId ? <LocalBrowserCloseDialog
          key={`${browserCloseRequest.cacheKey}:${browserCloseRequest.tabId}`}
          ownerSessionId={browserCloseRequest.owner}
          onCancel={() => setBrowserCloseRequest(null)}
          onClosed={() => {
            setContextTabStateEntry((current) => current.cacheKey === browserCloseRequest.cacheKey
              ? { ...current, state: closeWorkspaceContextTab(current.state, browserCloseRequest.tabId) } : current);
            setBrowserCloseRequest(null);
          }}
        /> : null}
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
