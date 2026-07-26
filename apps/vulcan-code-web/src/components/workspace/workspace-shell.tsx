import React from "react";
import { ChatArea } from "@/components/conversation/chat-area";
import {
  ConversationSessionCommitDialog,
  ConversationSessionTodolistPanel,
  type ConversationSessionCommitBusyState,
} from "@/components/conversation/session-todolist-panel";
import {
  parseConversationBlocks,
} from "@/components/conversation/conversation-transcript";
import { dispatchConversationLayoutWillChange } from "@/components/conversation/conversation-layout-events";
import { terminalSessionViewModel } from "@/components/conversation/conversation-terminal-model";
import {
  activeSkillHeaderSummary,
  type SkillHeaderBusyActionId,
} from "@/components/features/skills/SkillsArea";
import {
  AgentsArea,
  activeAgentsHeaderSummary,
  FloatingAgentCategoryMenu,
} from "@/components/features/agents/AgentsArea";
import { AgentSkillsArea, FloatingAgentSkillCategoryMenu } from "@/components/features/agent-skills/AgentSkillsArea";
import { KnowledgeBaseArea } from "@/components/features/knowledge/KnowledgeBaseArea";
import {
  ProjectSidebar,
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
} from "@/components/workspace/workspace-resizable-body";
import {
  type WorkspaceContextTabKind,
} from "@/components/workspace/WorkspaceContextPanel";
import type {
  DiffReviewSelection,
  WorkspaceDiffSnapshotResponse,
} from "@/diff/types";
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
import {
  buildWorkspaceProjectGitSummary,
  type WorkspaceProjectGitStateMap,
} from "@/components/workspace/workspace-shell/git-summary";
import type { WorkspaceShellProps } from "@/components/workspace/workspace-shell/types";
export type { WorkspaceMainView } from "@/components/workspace/workspace-shell/types";
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
function WorkspaceShellContent(props: WorkspaceShellProps) {
  // sidebar controls the left project tree that can collapse automatically.
  // sidebar 控制可自动折叠的左侧项目树。
  const sidebar = useSidebar();
  // t translates shared workspace chrome labels.
  // t 翻译共享工作区外壳标签。
  const { locale, t } = useI18n();
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
  // agentsMenuExpanded stores the shared adaptive menu state for the agents page.
  // agentsMenuExpanded 保存智能体页共享自适应菜单展开状态。
  const [agentsMenuExpanded, setAgentsMenuExpanded] = React.useState(true);
  // agentSkillsMenuExpanded stores the shared adaptive directory state for the Agent Skills page.
  // agentSkillsMenuExpanded 保存提示词技能页共享自适应目录展开状态。
  const [agentSkillsMenuExpanded, setAgentSkillsMenuExpanded] = React.useState(true);
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
  // gitCommitDialogOpen stores the shared commit dialog state across chat and review surfaces.
  // gitCommitDialogOpen 保存聊天区与审核区共享的提交弹窗状态。
  const [gitCommitDialogOpen, setGitCommitDialogOpen] = React.useState(false);
  // gitCommitBusyStateByWorkspace stores running Git actions by workspace path.
  // gitCommitBusyStateByWorkspace 按工作区路径保存正在执行的 Git 动作状态。
  const [gitCommitBusyStateByWorkspace, setGitCommitBusyStateByWorkspace] = React.useState<Record<string, NonNullable<ConversationSessionCommitBusyState>>>({});
  // workspaceGitRefreshRequestSeqRef provides monotonic ids for project-level Git refreshes.
  // workspaceGitRefreshRequestSeqRef 为项目级 Git 刷新提供单调请求标识。
  const workspaceGitRefreshRequestSeqRef = React.useRef(0);
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
  const hasSelectedChatConversation = props.mainView === "chat" && !props.selectedDraftConversation && Boolean(props.selectedConversationId);
  // selectedChatProject resolves the selected persisted conversation project for Git review summaries.
  // selectedChatProject 解析当前持久化会话所属项目以加载 Git 审查摘要。
  const selectedChatProject = hasSelectedChatConversation && props.selectedConversation?.projectId
    ? props.workbench.projects.find((project) => project.id === props.selectedConversation?.projectId) ?? null
    : null;
  // selectedGitCommitWorkspaceKey identifies the Git workspace whose commit state is currently shown.
  // selectedGitCommitWorkspaceKey 标识当前展示提交状态的 Git 工作区。
  const selectedGitCommitWorkspaceKey = selectedChatProject?.path ?? null;
  // selectedWorkspaceGitState stores the selected project's shared Git state entry.
  // selectedWorkspaceGitState 保存当前选中项目的共享 Git 状态条目。
  const selectedWorkspaceGitState = selectedGitCommitWorkspaceKey
    ? workspaceGitStateByProject[selectedGitCommitWorkspaceKey] ?? null
    : null;
  // sessionReviewSummary exposes the selected project's Git summary to chat and review triggers.
  // sessionReviewSummary 向会话和审核入口暴露当前选中项目的 Git 摘要。
  const sessionReviewSummary = selectedWorkspaceGitState?.summary ?? null;
  // gitCommitBusyState stores the selected workspace's shared Git action phase.
  // gitCommitBusyState 保存当前选中工作区的共享 Git 操作阶段。
  const gitCommitBusyState = selectedGitCommitWorkspaceKey
    ? gitCommitBusyStateByWorkspace[selectedGitCommitWorkspaceKey] ?? null
    : null;
  // updateGitCommitBusyState updates the running Git action state for the captured workspace.
  // updateGitCommitBusyState 更新当前闭包捕获工作区的 Git 动作状态。
  const updateGitCommitBusyState = React.useCallback((state: ConversationSessionCommitBusyState) => {
    const workspaceKey = selectedGitCommitWorkspaceKey;
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
  // visibleSessionTerminalSessions mirrors the terminal panel's own render filter.
  // visibleSessionTerminalSessions 对齐终端面板自身的可渲染过滤条件。
  const visibleSessionTerminalSessions = React.useMemo(
    () => sessionTerminalSessions
      .map((summary) => terminalSessionViewModel(summary, t))
      .filter((session) => session.canStop || session.state === "lost" || session.state === "waiting"),
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
    if (props.mainView !== "chat" || props.selectedDraftConversation !== null) {
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
    props.selectedDraftConversation,
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
    const preferredRightWidth = 480;
    const handleWidth = 9;
    const leftWidth = sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
    setRightPanelFloating(width > 0 && width - leftWidth - preferredRightWidth - handleWidth < 800);
    setRightPanelOpen(true);
  }, [notifyConversationLayoutWillChange, sidebar.open]);
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
  const handleOpenReviewChanges = React.useCallback(() => {
    if (!hasSelectedChatConversation) {
      return;
    }
    openContextTab("review");
    setReviewFocusRequest((current) => current + 1);
    openRightContextPanel();
  }, [hasSelectedChatConversation, openContextTab, openRightContextPanel]);

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

  // sessionTodolist stores the selected conversation todolist payload when it should be visible.
  // sessionTodolist 保存需要展示时当前会话的 todolist 载荷。
  const sessionTodolist = hasSelectedChatConversation && props.workbench.sessionTodolist?.items.length
    ? props.workbench.sessionTodolist
    : null;
  // sessionGitRepositoryAvailable tracks whether the current chat project resolved as a Git repository.
  // sessionGitRepositoryAvailable 标记当前聊天项目是否已解析为 Git 仓库。
  const sessionGitRepositoryAvailable = Boolean(sessionReviewSummary || selectedWorkspaceGitState?.loading);
  // sessionGitCommitActionAvailable tracks whether commit or push has a runnable target.
  // sessionGitCommitActionAvailable 标记提交或推送是否存在可执行目标。
  const sessionGitCommitActionAvailable = Boolean(sessionReviewSummary && (
    sessionReviewSummary.changedFiles > 0
    || sessionReviewSummary.unpushedCommits > 0
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
  const sessionToolPanelVisible = hasSelectedChatConversation && (
    sessionGitCommitActionVisible || visibleSessionTerminalSessions.length > 0 || visibleSessionSubagentTaskCount > 0
  );

  // refreshProjectGitSummary reloads the shared Git summary for one project path.
  // refreshProjectGitSummary 重新加载单个项目路径的共享 Git 摘要。
  const refreshProjectGitSummary = React.useCallback(async (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
  ) => {
    const requestId = workspaceGitRefreshRequestSeqRef.current + 1;
    workspaceGitRefreshRequestSeqRef.current = requestId;
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
        },
      };
    });
    const [unstagedResult, stagedResult] = await Promise.allSettled([
      props.onLoadWorkspaceDiffSnapshot(
        workspacePath,
        projectId,
        conversationId,
        { mode: "unstaged" },
      ),
      props.onLoadWorkspaceDiffSnapshot(
        workspacePath,
        projectId,
        conversationId,
        { mode: "staged" },
      ),
    ]);
    const rejectedError = [unstagedResult, stagedResult]
      .find((result): result is PromiseRejectedResult => result.status === "rejected")
      ?.reason;
    setWorkspaceGitStateByProject((current) => {
      const previous = current[workspacePath];
      if (previous && previous.refreshRequestId !== requestId) {
        return current;
      }
      const unstagedSnapshot = unstagedResult.status === "fulfilled"
        ? unstagedResult.value
        : previous?.unstagedSnapshot ?? null;
      const stagedSnapshot = stagedResult.status === "fulfilled"
        ? stagedResult.value
        : previous?.stagedSnapshot ?? null;
      return {
        ...current,
        [workspacePath]: {
          error: rejectedError instanceof Error ? rejectedError.message : rejectedError ? String(rejectedError) : null,
          loading: false,
          projectId,
          refreshRequestId: requestId,
          revision: (previous?.revision ?? 0) + 1,
          stagedSnapshot,
          summary: buildWorkspaceProjectGitSummary(unstagedSnapshot, stagedSnapshot),
          unstagedSnapshot,
          workspacePath,
        },
      };
    });
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
      const nextUnstagedSnapshot = selection.mode === "unstaged"
        ? response
        : previous?.unstagedSnapshot ?? null;
      const nextStagedSnapshot = selection.mode === "staged"
        ? response
        : previous?.stagedSnapshot ?? null;
      return {
        ...current,
        [workspacePath]: {
          error: null,
          loading: false,
          projectId,
          revision: (previous?.revision ?? 0) + 1,
          refreshRequestId: previous?.refreshRequestId ?? 0,
          stagedSnapshot: nextStagedSnapshot,
          summary: buildWorkspaceProjectGitSummary(nextUnstagedSnapshot, nextStagedSnapshot),
          unstagedSnapshot: nextUnstagedSnapshot,
          workspacePath,
        },
      };
    });
  }, []);

  React.useEffect(() => {
    if (!hasSelectedChatConversation || !sessionReviewSummary) {
      setGitCommitDialogOpen(false);
    }
  }, [hasSelectedChatConversation, sessionReviewSummary]);

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
          <button
            className="skill-action-button secondary"
            disabled={skillHeaderBusyAction !== null}
    onClick={() => void runSkillHeaderAction("lua-refresh", props.luaSkillActions.onRefreshLuaSkills)}
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
          skillActions={props.luaSkillActions}
        workbench={props.workbench}
      />
    ),
    detail: `${skillSubAreaLabel(props.activeSkillSubArea, props.workbench, locale, t)} · ${skillHeaderSummary.detail}`,
    headerActions: skillHeaderActions,
    title: t("workspaceShell.skillsTitle"),
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
          workbench={props.workbench}
        />
      ),
      side: "left",
    }],
  } : props.mainView === "agent-skills" ? {
    content: (
      <AgentSkillsArea
        actions={props.agentSkillActions}
        selectedCategoryId={selectedAgentSkillCategoryId}
        onSelectCategory={setSelectedAgentSkillCategoryId}
        workbench={props.workbench}
      />
    ),
    detail: props.workbench.agentSkills.notice,
    headerActions: null,
    title: t("area.agentSkills"),
    toolPanels: [{
      className: "workspace-tool-panel-page workspace-skills-settings workspace-agent-settings",
      contentClassName: "workspace-skills-content workspace-agent-content",
      contentShellClassName: "workspace-skills-content-shell",
      expanded: agentSkillsMenuExpanded,
      onLayoutModeWillChange: notifyConversationLayoutWillChange,
      panel: (
        <FloatingAgentSkillCategoryMenu
          expanded={agentSkillsMenuExpanded}
          selectedCategoryId={selectedAgentSkillCategoryId}
          onExpandedChange={setAgentSkillsMenuExpanded}
          onSelectCategory={setSelectedAgentSkillCategoryId}
          workbench={props.workbench}
        />
      ),
      side: "left",
    }],
  } : props.mainView === "agents" ? {
    content: (
      <AgentsArea
        actions={props.agentActions}
        selectedCategoryId={selectedAgentCategoryId}
        onSelectCategory={setSelectedAgentCategoryId}
        workbench={props.workbench}
      />
    ),
    detail: agentsHeaderSummary.detail,
    headerActions: agentsHeaderActions,
    title: t("workspaceShell.agentsTitle"),
    toolPanels: [{
      className: "workspace-tool-panel-page workspace-skills-settings workspace-agent-settings",
      contentClassName: "workspace-skills-content workspace-agent-content",
      contentShellClassName: "workspace-skills-content-shell",
      expanded: agentsMenuExpanded,
      onLayoutModeWillChange: notifyConversationLayoutWillChange,
      panel: (
        <FloatingAgentCategoryMenu
          expanded={agentsMenuExpanded}
          selectedCategoryId={selectedAgentCategoryId}
          onExpandedChange={setAgentsMenuExpanded}
          onSelectCategory={setSelectedAgentCategoryId}
          workbench={props.workbench}
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
    title: t("workspaceShell.memoryTitle"),
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
  } : props.mainView === "knowledge" ? {
    content: <KnowledgeBaseArea />,
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
        loadingConversationId={props.loadingConversationId}
        conversationLoadError={props.conversationLoadError}
        conversationHydration={props.conversationHydration}
        conversationHasMoreHistory={props.conversationHasMoreHistory}
        conversationHistoryLoading={props.conversationHistoryLoading}
        sessionReviewSummary={sessionReviewSummary}
        runtimeStatusHost={props.runtimeStatusHost}
        todolistStatusHost={props.todolistStatusHost}
        onSelectWorkbenchModel={props.onSelectWorkbenchModel}
        onListWorkbenchModelSecondaryRoutes={props.onListWorkbenchModelSecondaryRoutes}
        onSelectWorkbenchModelSecondaryRoute={props.onSelectWorkbenchModelSecondaryRoute}
        onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
        onSelectWorkbenchThinkingDisplayMode={props.onSelectWorkbenchThinkingDisplayMode}
        onSlashCommand={props.onSlashCommand}
        onOpenKnowledge={props.onOpenKnowledge}
        onSubmitPrompt={props.onSubmitPrompt}
        onSaveAiCapabilityConfiguration={props.onSaveAiCapabilityConfiguration}
        onSaveSessionGoal={props.onSaveSessionGoal}
        onResumeSessionGoal={props.onResumeSessionGoal}
        onClearSessionGoal={props.onClearSessionGoal}
        onUpdateSessionGoalStatus={props.onUpdateSessionGoalStatus}
        onSetSessionPlanMode={props.onSetSessionPlanMode}
        onSetSessionVmmPolicy={props.onSetSessionVmmPolicy}
        onStopConversationRun={props.onStopConversationRun}
        onOpenSubagentPanel={handleOpenSubagentPanel}
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
    headerRightActions: hasSelectedChatConversation ? contextPanelToggleAction : undefined,
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
          commitBusyState={gitCommitBusyState}
          commitDialogOpen={gitCommitDialogOpen}
          expanded={sessionToolExpanded}
          reviewSummary={sessionReviewSummary}
          subagentNavigation={sessionSubagentNavigation}
          terminalSessions={sessionTerminalSessions}
          todolist={null}
          onCommitDialogOpenChange={setGitCommitDialogOpen}
          onExpandedChange={handleSessionToolExpandedChange}
          onOpenReviewChanges={handleOpenReviewChanges}
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
          onOpenSubagentPanel={handleOpenSubagentPanel}
          onStopSubagentTask={props.onStopSubagentTask}
          onCloseSubagentTask={props.onCloseSubagentTask}
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
          onWorkspaceDiffSnapshotLoaded={recordWorkspaceDiffSnapshotForProject}
          onWorkspaceGitStateChanged={refreshProjectGitSummary}
          onRunVmmAction={props.memoryActions.onRunVmmAction}
          onLoadToolDetail={props.onLoadToolDetail}
          onTrace={props.onTrace}
          gitCommitActionBusy={Boolean(gitCommitBusyState)}
          gitCommitActionDisabled={!sessionGitCommitActionAvailable}
          gitCommitActionLabel={gitCommitBusyState?.label ?? t("gitCommitDialog.open")}
          onOpenGitCommitAction={sessionGitCommitActionVisible ? () => setGitCommitDialogOpen(true) : undefined}
        >
          {pageDescriptor.content}
        </WorkspaceResizableBody>
        {props.footer ? (
          <div className="workspace-statusbar-host">
            {props.footer}
          </div>
        ) : null}
      </SidebarInset>
      {sessionGitCommitActionVisible && sessionReviewSummary ? (
        <ConversationSessionCommitDialog
          busyState={gitCommitBusyState}
          commitBranchPrefix={props.workbench.gitCommit?.newBranchPrefix ?? "vulcan/"}
          conversationId={props.selectedConversationId}
          open={gitCommitDialogOpen}
          reviewSummary={sessionReviewSummary}
          workspacePath={selectedChatProject?.path ?? null}
          onBusyStateChange={updateGitCommitBusyState}
          onOpenChange={setGitCommitDialogOpen}
          onRefreshReviewSummary={refreshSessionReviewSummary}
        />
      ) : null}
      {props.conversationForkInProgress ? (
        <WorkspaceBlockingProgressOverlay
          message={t("conversation.action.forkInProgressMessage")}
          title={t("conversation.action.forkInProgressTitle")}
        />
      ) : null}
    </div>
  );
}

// WorkspaceBlockingProgressOverlay blocks all workspace interaction while one modal backend action is pending.
// WorkspaceBlockingProgressOverlay 在单个模态后端动作等待期间阻塞所有工作台交互。
function WorkspaceBlockingProgressOverlay(props: {
  // Primary progress title shown in the shared loading card.
  // 共享加载卡片中展示的主标题。
  title: string;
  // Secondary progress message shown below the indeterminate bar.
  // 不确定进度条下方展示的辅助说明。
  message: string;
}) {
  return (
    <div className="workspace-blocking-progress-overlay" role="status" aria-live="polite" aria-busy="true">
      <section className="conversation-loading-card workspace-blocking-progress-card">
        <div className="conversation-loading-heading">
          <strong>{props.title}</strong>
        </div>
        <div
          aria-label={props.title}
          aria-valuetext={props.message}
          className="conversation-loading-bar workspace-blocking-progress-bar"
          role="progressbar"
        >
          <div className="conversation-loading-bar-value workspace-blocking-progress-bar-value" />
        </div>
        <p className="workspace-blocking-progress-message">{props.message}</p>
      </section>
    </div>
  );
}
