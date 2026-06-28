import React from "react";
import {
  FeatureShell as ExtractedFeatureShell,
  primaryFeatureAreas as buildPrimaryFeatureAreas,
} from "@/components/features/feature-shell";
import {
  RepositoryLink,
  StatusbarRuntimeActivities,
} from "@/components/shell/app-chrome";
import { StartupOverlay } from "@/components/shell/startup-screen";
import {
  normalizedComposerSlashCommandId,
  SLASH_SKILL_PROMPT,
  type ComposerSlashCommandResult,
} from "@/components/composer/composer-model";
import {
  buildStatusbarRuntimeActivities,
} from "@/components/conversation/conversation-runtime";
import { projectConversationRenderItems } from "@/components/conversation/conversation-transcript";
import {
  restoreProjectDisplayName,
  type RenameTarget,
} from "@/components/project/project-action-dialogs";
import {
  normalizeProjectIconSelection,
  type ProjectIconDialogTarget,
  type ProjectIconSelection,
} from "@/components/project/project-icons";
import {
  projectAgentSkillSaveMessage,
  projectConversationCount,
  type ProjectAgentSkillState,
  type ProjectSettingsDialogTarget,
} from "@/components/project/project-settings-dialog";
import {
  type ConversationMenuAction,
  type ProjectMenuAction,
} from "@/components/project/ProjectSidebar";
import {
  type DraftConversationSelection,
} from "@/components/workspace/workspace-resizable-body";
import { WorkbenchDialogs } from "@/components/workspace/workbench-dialogs";
import { useWorkbenchShellChrome } from "@/components/workspace/workbench-layout/shell-chrome";
import {
  copyText,
  webuiClickTrace,
  type WorkbenchDeltaSignal,
} from "@/components/workspace/workbench-layout/types";
import { useConversationHydration } from "@/components/workspace/workbench-layout/conversation-hydration";
import { useWorkbenchConversationActions } from "@/components/workspace/workbench-layout/conversation-actions";
import { useWorkbenchProjectActions } from "@/components/workspace/workbench-layout/project-actions";
import { useWorkbenchSessionActions } from "@/components/workspace/workbench-layout/session-actions";
export type { WorkbenchDeltaSignal } from "@/components/workspace/workbench-layout/types";
import {
  WorkspaceShell,
  type WorkspaceMainView,
} from "@/components/workspace/workspace-shell";
import type {
  SkillActionProps,
  SkillSubAreaId,
} from "@/components/features/skills/SkillsArea";
import type {
  MemorySubAreaId,
} from "@/components/features/memory/MemoryArea";
import { cn } from "@/lib/utils";
import { useI18n, type LocaleCode } from "@/i18n";
import type {
  AgentSkillsCatalogSummary,
  ComposerSlashCommandSummary,
  ConversationMessage,
  ConversationSummary,
  FeatureAreaSummary,
  MainAreaId,
  PendingQuestionSnapshot,
  PendingPromptSummary,
  ProjectSummary,
  SettingsSubAreaId,
  StartupSnapshot,
  VmmConfigDraftSummary,
  WorkbenchSnapshot,
} from "@/types";
import {
  rejectPendingQuestion,
  replyPendingQuestion,
  stageComposerAttachmentBytes,
} from "@/service/conversation-actions-service";
import { loadCachedConversationToolDetail } from "@/service/conversation-tool-detail-service";
import {
  addProjectFromPath,
  archiveConversation,
  archiveProjectConversations,
  browseProjectDirectories,
  markConversationRead,
  openExternalUrl,
  openProjectDirectory,
  openProjectDirectoryPath,
  removeProject,
  renameConversation,
  renameProject,
  reorderProjects,
  resetProjectIcon,
  selectConversation,
  setExpandedTaskProjects,
  setProjectIcon,
  toggleProjectPinned,
} from "@/service/project-actions-service";
import {
  removeAiCapabilityVendor,
  removeMcpServerConfiguration,
  removeProviderConfiguration,
  getRtkGainStats,
  saveAiCapabilityTargets,
  saveAiCapabilityVendor,
  saveGitCommitSettings,
  saveMcpServerConfiguration,
  saveProviderConfiguration,
  saveProviderEnabledModels,
  saveProviderModelOverride,
  saveRtkRoutingSettings,
  setAiCapabilityTargetEnabled,
} from "@/service/settings-actions-service";
import {
  installLuaSkill,
  loadProjectAgentSkillOverrides,
  refreshAgentSkillsCatalog,
  refreshLuaSkillsCatalog,
  setAgentSkillEnabled,
  setAgentSkillProjectState,
  setAgentSkillsEnabled,
  setLuaSkillEnabled,
  setLuaSkillToolRegistrationState,
  uninstallLuaSkill,
  updateLuaSkill,
} from "@/service/skills-actions-service";
import {
  executeProjectAttachmentCleanup,
  loadProjectVmmConfig,
  previewProjectAttachmentCleanup,
  runVmmMemoryAction,
  saveVmmMemoryConfig,
} from "@/service/memory-actions-service";
import {
  loadWorkspaceDiffFileDetail,
  loadWorkspaceDiffSearch,
  loadWorkspaceDiffSnapshot,
  loadWorkspaceFileDirectory,
  loadWorkspaceFilePreview,
  runWorkspaceGitFileAction,
} from "@/service/workbench-snapshot-service";
import { localMediaSourceUrl } from "@/resources/localMedia";
import {
  applySubagentRoutePreference,
  applyWorkbenchDeltaWithSubagentSurfaceMessageCache,
  closeSubagentSurfaceForConversation,
  coalesceWorkbenchDeltaEnvelopes,
  markConversationLocallyRead,
  mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache,
  workbenchWithSubagentSurfaceMessageCache,
  type InflightSubagentOpenRequest,
  type SubagentRoutePreference,
  type SubagentSurfaceMessageCache,
} from "@/state/workbench-state";
import {
  applyPreviewConversationAction,
  applyPreviewConversationRename,
  applyPreviewProjectAction,
  applyPreviewProjectIcon,
  applyPreviewProjectOrder,
  applyPreviewProjectRename,
} from "@/state/workbench-preview-actions";
import { projectForConversation } from "@/state/workbench-selectors";
import { toast } from "sonner";
import * as LucideIcons from "lucide-react";
import {
  Boxes,
  BrainCircuit,
  ArrowUp,
  ChevronLeft,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Check,
  Calendar,
  Clock3,
  Code2,
  Copy,
  Database,
  FileCode2,
  FileJson2,
  FileText,
  Folder,
  FolderCog,
  Hammer,
  History,
  Image as ImageIcon,
  Minus,
  Plus,
  Maximize2,
  Scan,
  Save,
  Pencil,
  Play,
  PlugZap,
  ScrollText,
  Settings,
  Shield,
  Moon,
  Sun,
  TerminalSquare,
  Trash2,
} from "lucide-react";

// WorkbenchLayout renders the WebUI workbench shell.
// WorkbenchLayout 渲染 WebUI 工作台外壳。
export function WorkbenchLayout(props: {
  // Latest startup snapshot displayed by the shell blocker.
  // 壳层阻塞面板展示的最近启动快照。
  startup: StartupSnapshot;
  // Latest full workbench snapshot from the backend.
  // 后端返回的最近完整工作台快照。
  workbench: WorkbenchSnapshot;
  // Latest partial workbench update signal from the backend.
  // 后端返回的最近局部工作台更新信号。
  workbenchDelta: WorkbenchDeltaSignal | null;
}) {
  // i18n provides localized shell labels and the active language.
  // i18n 提供本地化外壳标签与当前语言。
  const i18n = useI18n();
  // shellChrome owns app-wide theme, window controls, and titlebar gesture handlers.
  // shellChrome 持有应用级主题、窗口控制与标题栏手势处理器。
  const {
    shellChromeActions,
    showWindowControls,
    handleTitlebarDoubleClick,
  } = useWorkbenchShellChrome({
    startup: props.startup,
    locale: i18n.locale,
    onChangeLocale: i18n.setLocale,
  });
  // localizedPrimaryFeatureAreas stores the translated top navigation metadata.
  // localizedPrimaryFeatureAreas 保存已翻译的顶部导航元数据。
  const localizedPrimaryFeatureAreas = React.useMemo(
    () => buildPrimaryFeatureAreas(i18n.t).filter((area) => area.id !== "skills"),
    [i18n.t],
  );
  // localizedSettingsFeatureArea stores the translated settings metadata.
  // localizedSettingsFeatureArea 保存已翻译的设置元数据。
  const localizedSettingsFeatureArea = React.useMemo<FeatureAreaSummary>(() => ({
    id: "settings",
    label: i18n.t("area.settings"),
    description: i18n.t("area.settings.description"),
    status: i18n.t("feature.status.preview"),
  }), [i18n]);
  // localizedTopFeatureAreas keeps settings as the final primary navigation item.
  // localizedTopFeatureAreas 将设置保留为主导航的最后一项。
  const localizedTopFeatureAreas = React.useMemo(
    () => [...localizedPrimaryFeatureAreas, localizedSettingsFeatureArea],
    [localizedPrimaryFeatureAreas, localizedSettingsFeatureArea],
  );
  // activeArea stores the currently selected feature area.
  // activeArea 保存当前选中的功能区域。
  const [activeArea, setActiveArea] = React.useState<MainAreaId>("chat");
  // navigationSidebarOpen preserves sidebar collapse state across workspace and settings shells.
  // navigationSidebarOpen 在工作台与设置外壳之间保留侧栏折叠状态。
  const [navigationSidebarOpen, setNavigationSidebarOpen] = React.useState(true);
  // workspaceMainView stores the selected main surface inside the workbench.
  // workspaceMainView 保存工作台内部当前选中的主内容界面。
  const [workspaceMainView, setWorkspaceMainView] = React.useState<WorkspaceMainView>("chat");
  // activeWorkbenchSkillSubArea stores the embedded skills settings sub page.
  // activeWorkbenchSkillSubArea 保存内嵌技能设置的子页面。
  const [activeWorkbenchSkillSubArea, setActiveWorkbenchSkillSubArea] = React.useState<SkillSubAreaId>("lua");
  // activeWorkbenchMemorySubArea stores the embedded memory settings sub page.
  // activeWorkbenchMemorySubArea 保存内嵌记忆设置的子页面。
  const [activeWorkbenchMemorySubArea, setActiveWorkbenchMemorySubArea] = React.useState<MemorySubAreaId>("status");
  // selectedConversationId stores the conversation selected in the sidebar.
  // selectedConversationId 保存侧栏当前选中的会话。
  const [selectedConversationId, setSelectedConversationId] = React.useState<string | null>(
    props.workbench.selectedConversationId ?? props.workbench.conversations[0]?.id ?? null,
  );
  // selectedConversationIdRef lets async submit completions avoid stealing focus after the user switches sessions.
  // selectedConversationIdRef 让异步提交完成时避免在用户已切换会话后抢回焦点。
  const selectedConversationIdRef = React.useRef<string | null>(selectedConversationId);
  // subagentNavigationRequestSeqRef rejects stale async subagent route responses after a newer click.
  // subagentNavigationRequestSeqRef 在发生更新点击后丢弃过期的异步子代理路由响应。
  const subagentNavigationRequestSeqRef = React.useRef(0);
  // subagentRoutePreferenceRef stores the last user-selected subagent display route for WebUI-only routing.
  // subagentRoutePreferenceRef 保存用户最后选择的子代理显示路由，仅用于 WebUI 显示路由。
  const subagentRoutePreferenceRef = React.useRef<SubagentRoutePreference | null>(null);
  // inflightSubagentOpenRequestRef coalesces duplicate backend open commands for the same route.
  // inflightSubagentOpenRequestRef 合并同一路由的重复后端打开命令。
  const inflightSubagentOpenRequestRef = React.useRef<InflightSubagentOpenRequest | null>(null);
  // draftConversation stores the local-only conversation opened before the first user message is sent.
  // draftConversation 保存发送第一条用户消息前打开的本地草稿会话。
  const [draftConversation, setDraftConversation] = React.useState<DraftConversationSelection | null>(null);
  // draftConversationIdRef keeps the latest draft id without retriggering workbench snapshot hydration.
  // draftConversationIdRef 保存最新草稿标识，避免重新触发工作台快照水合。
  const draftConversationIdRef = React.useRef<string | null>(null);
  // subagentSurfaceMessageCacheRef retains every loaded child transcript surface for instant route switching.
  // subagentSurfaceMessageCacheRef 保留每个已载入的子代理转录界面，用于即时路由切换。
  const subagentSurfaceMessageCacheRef = React.useRef<SubagentSurfaceMessageCache>(new Map());
  // workbenchState stores local message hydration updates.
  // workbenchState 保存本地会话正文水合更新。
  const [workbenchState, setWorkbenchState] = React.useState<WorkbenchSnapshot>(
    () => workbenchWithSubagentSurfaceMessageCache(props.workbench, subagentSurfaceMessageCacheRef.current),
  );
  // workbenchStateRef exposes the latest workbench state to route-click callbacks without stale closures.
  // workbenchStateRef 向路由点击回调暴露最新工作台状态，避免闭包过期。
  const workbenchStateRef = React.useRef<WorkbenchSnapshot>(workbenchState);
  workbenchStateRef.current = workbenchState;
  // expandedProjectIdsLocallyEditedRef records whether the user has touched project expansion in this session.
  // expandedProjectIdsLocallyEditedRef 记录本轮会话中用户是否已经手动修改过项目展开状态。
  const expandedProjectIdsLocallyEditedRef = React.useRef(false);
  // mergeWorkbenchWithLocalCaches centralizes service snapshot merges that must preserve loaded child surfaces.
  // mergeWorkbenchWithLocalCaches 集中处理必须保留已载入子代理界面的服务快照合并。
  const mergeWorkbenchWithLocalCaches = React.useCallback((current: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => (
    mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache(current, incoming, subagentSurfaceMessageCacheRef.current)
  ), []);
  // applySkillWorkbenchAction runs one skill-page service action and merges the returned snapshot.
  // applySkillWorkbenchAction 执行单个技能页面服务动作并合并返回快照。
  const applySkillWorkbenchAction = React.useCallback(async (action: () => Promise<WorkbenchSnapshot>) => {
    const snapshot = await action();
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
  }, [mergeWorkbenchWithLocalCaches]);
  // applyAgentSkillsCatalog merges one lightweight Agent Skills catalog update.
  // applyAgentSkillsCatalog 合并单个轻量 Agent Skills 目录更新。
  const applyAgentSkillsCatalog = React.useCallback((agentSkills: AgentSkillsCatalogSummary) => {
    setWorkbenchState((current) => ({
      ...current,
      agentSkills,
    }));
  }, []);
  // renameTarget stores the project or conversation currently being renamed.
  // renameTarget 保存当前正在重命名的项目或会话目标。
  const [renameTarget, setRenameTarget] = React.useState<RenameTarget | null>(null);
  // projectIconTarget stores the project currently editing its sidebar icon.
  // projectIconTarget 保存当前正在编辑侧栏图标的项目。
  const [projectIconTarget, setProjectIconTarget] = React.useState<ProjectIconDialogTarget | null>(null);
  // projectSettingsTarget stores the project currently shown in the settings dialog.
  // projectSettingsTarget 保存当前显示设置弹窗的项目。
  const [projectSettingsTarget, setProjectSettingsTarget] = React.useState<ProjectSettingsDialogTarget | null>(null);
  // projectFolderPickerOpen stores whether the service-backed folder picker is open.
  // projectFolderPickerOpen 保存服务驱动的文件夹选择器是否打开。
  const [projectFolderPickerOpen, setProjectFolderPickerOpen] = React.useState(false);
  // archiveProjectTarget stores the project waiting for archive confirmation.
  // archiveProjectTarget 保存正在等待归档确认的项目。
  const [archiveProjectTarget, setArchiveProjectTarget] = React.useState<ProjectSummary | null>(null);
  // archiveProjectSubmitting tracks a bulk archive request in the confirmation dialog.
  // archiveProjectSubmitting 跟踪批量归档确认弹窗中的请求状态。
  const [archiveProjectSubmitting, setArchiveProjectSubmitting] = React.useState(false);
  // removeProjectTarget stores the project waiting for removal confirmation.
  // removeProjectTarget 保存正在等待移除确认的项目。
  const [removeProjectTarget, setRemoveProjectTarget] = React.useState<ProjectSummary | null>(null);
  // removeProjectSubmitting tracks an archive-then-remove project request.
  // removeProjectSubmitting 跟踪先归档再移除项目的请求状态。
  const [removeProjectSubmitting, setRemoveProjectSubmitting] = React.useState(false);
  // pinnedConversationIds stores the locally pinned conversations until persistence is added.
  // pinnedConversationIds 保存本地置顶的会话，直到后续接入持久化。
  const [pinnedConversationIds, setPinnedConversationIds] = React.useState<Set<string>>(() => new Set());
  // activeSettingsSubArea stores the selected settings sidebar page.
  // activeSettingsSubArea 保存设置侧栏当前选择的页面。
  const [activeSettingsSubArea, setActiveSettingsSubArea] = React.useState<SettingsSubAreaId>("model-providers");
  // activeFeature stores the selected feature summary from the complete top navigation list.
  // activeFeature 从完整顶部导航列表中保存当前选中的功能区域摘要。
  const activeFeature = localizedTopFeatureAreas.find((area) => area.id === activeArea) ?? localizedTopFeatureAreas[0];
  // handleSelectTopFeatureArea routes top navigation away from the embedded skills view.
  // handleSelectTopFeatureArea 将顶部导航路由出内嵌技能视图。
  const handleSelectTopFeatureArea = React.useCallback((area: MainAreaId) => {
    if (area === "skills") {
      setActiveArea("chat");
      setWorkspaceMainView("skills");
      return;
    }
    if (area === "memory") {
      setActiveArea("chat");
      setWorkspaceMainView("memory");
      return;
    }
    setActiveArea(area);
    setWorkspaceMainView("chat");
  }, []);
  // handleOpenWorkspaceSkills opens skills settings inside the workbench main panel.
  // handleOpenWorkspaceSkills 在工作台主面板内部打开技能设置。
  const handleOpenWorkspaceSkills = React.useCallback(() => {
    setActiveArea("chat");
    setWorkspaceMainView("skills");
  }, []);
  // handleOpenWorkspaceMemory opens memory settings inside the workbench main panel.
  // handleOpenWorkspaceMemory 在工作台主面板内部打开记忆设置。
  const handleOpenWorkspaceMemory = React.useCallback(() => {
    setActiveArea("chat");
    setWorkspaceMainView("memory");
  }, []);
  // handleOpenWorkspaceChat returns the workbench main panel to the conversation surface.
  // handleOpenWorkspaceChat 将工作台主面板切回会话界面。
  const handleOpenWorkspaceChat = React.useCallback(() => {
    setActiveArea("chat");
    setWorkspaceMainView("chat");
  }, []);
  // selectedConversation stores the conversation used by the console header.
  // selectedConversation 保存控制台标题栏使用的当前会话。
  const selectedConversation = workbenchState.conversations.find((conversation) => conversation.id === selectedConversationId);
  // selectedConversationMessages stores the current parent conversation message window.
  // selectedConversationMessages 保存当前父会话消息窗口。
  const selectedConversationMessages = React.useMemo(
    () => workbenchState.messages.filter((message) => message.conversationId === selectedConversationId),
    [selectedConversationId, workbenchState.messages],
  );
  // hydratedConversationWindowById indexes Rust-retained hydrated windows for render and request decisions.
  // hydratedConversationWindowById 为渲染与请求判断索引 Rust 保留的已水合窗口。
  const hydratedConversationWindowById = React.useMemo(
    () => new Map((workbenchState.hydratedConversationWindows ?? []).map((window) => [window.sessionId, window])),
    [workbenchState.hydratedConversationWindows],
  );
  // selectedConversationHistoryCursor reflects Rust-retained pagination plus local request progress.
  // selectedConversationHistoryCursor 反映 Rust 保留分页与本地请求进度。
  const selectedConversationHistoryCursor = selectedConversationId
    ? hydratedConversationWindowById.get(selectedConversationId)
    : null;
  // selectedConversationHydrated records whether the selected conversation body is already retained by Rust.
  // selectedConversationHydrated 记录当前选中会话正文是否已经由 Rust 保留。
  const selectedConversationHydrated = Boolean(selectedConversationHistoryCursor);
  // conversationHydration owns backend hydration and older-history paging for the selected conversation.
  // conversationHydration 持有当前选中会话的后端水合与更早历史分页状态。
  const {
    loadingConversationId,
    conversationLoadError,
    conversationHistoryLoadingIds,
    handleLoadConversationHistory,
  } = useConversationHydration({
    selectedConversationId,
    draftConversationId: draftConversation?.id ?? null,
    selectedConversationHydrated,
    hydratedConversationWindowById,
    mergeWorkbenchWithLocalCaches,
    setWorkbenchState,
  });
  // selectedConversationRenderItems stores parsed rows for sidebar/runtime summaries.
  // selectedConversationRenderItems 保存侧栏与运行状态摘要复用的解析行。
  const selectedConversationRenderItems = React.useMemo(
    () => projectConversationRenderItems(selectedConversationMessages),
    [selectedConversationMessages],
  );
  // statusbarRuntimeActivities stores bottom-bar tool and subagent runtime badges for the selected parent conversation.
  // statusbarRuntimeActivities 保存当前选中父会话的底部工具与子代理运行徽标。
  const statusbarRuntimeActivities = React.useMemo(
    () => buildStatusbarRuntimeActivities(
      selectedConversation,
      selectedConversationRenderItems,
      workbenchState.subagentNavigation?.parentConversationId === selectedConversationId
        ? workbenchState.subagentNavigation
        : null,
      i18n.t,
    ),
    [i18n.t, selectedConversation, selectedConversationId, selectedConversationRenderItems, workbenchState.subagentNavigation],
  );
  // selectedDraftConversation stores the draft used by the console header and empty state.
  // selectedDraftConversation 保存控制台标题栏与空状态使用的草稿会话。
  const selectedDraftConversation = selectedConversationId === draftConversation?.id ? draftConversation : null;
  // consoleWorkbenchState clears selected-session projections while a local draft conversation is active.
  // consoleWorkbenchState 在本地草稿会话激活时清空当前会话投影，避免沿用上一会话的侧栏状态。
  const consoleWorkbenchState = React.useMemo<WorkbenchSnapshot>(() => {
    if (!selectedDraftConversation) {
      return workbenchState;
    }
    return {
      ...workbenchState,
      contextInjection: null,
    };
  }, [selectedDraftConversation, workbenchState]);
  // showStartupOverlay blocks workspace interaction until the real service bootstrap completes.
  // showStartupOverlay 在真实服务启动完成前阻塞工作区交互。
  const showStartupOverlay = !props.startup.ready && props.startup.phase !== "browser-preview";

  // Keeps the async selection guard aligned with the latest sidebar choice.
  // 保持异步选中保护与最新侧栏选择一致。
  React.useEffect(() => {
    selectedConversationIdRef.current = selectedConversationId;
  }, [selectedConversationId]);
  // showConsoleSidebar limits project and conversation context to the console view.
  // showConsoleSidebar 将项目与会话上下文限定在控制台页面内。
  const showConsoleSidebar = activeArea === "chat";

  const {
    applyProjectActionSnapshot,
    handleProjectMenuAction,
    handleProjectSettingsVmmSave,
    handleProjectSettingsAgentSkillSave,
    skillActions,
    handleProjectIconConfirm,
    handleProjectIconReset,
    handleCreateConversation,
    handleCreateProject,
    handleConfirmProjectFolder,
    handleReorderProjects,
    handleExpandedProjectIdsChange,
  } = useWorkbenchProjectActions({
    workbenchState,
    setWorkbenchState,
    mergeWorkbenchWithLocalCaches,
    setDraftConversation,
    setSelectedConversationId,
    setRenameTarget,
    setProjectIconTarget,
    setProjectSettingsTarget,
    setProjectFolderPickerOpen,
    setArchiveProjectTarget,
    setRemoveProjectTarget,
    expandedProjectIdsLocallyEditedRef,
    applySkillWorkbenchAction,
    applyAgentSkillsCatalog,
    handleOpenWorkspaceChat,
  });

  const {
    handleSaveAiCapabilityConfiguration,
    handleSaveSessionGoal,
    handleResumeSessionGoal,
    handleClearSessionGoal,
    handleUpdateSessionGoalStatus,
    handleSetSessionPlanMode,
    handleEditPendingPrompt,
  } = useWorkbenchSessionActions({
    selectedConversation,
    selectedDraftConversation,
    setWorkbenchState,
    mergeWorkbenchWithLocalCaches,
  });

  const {
    handleSubmitConversationPrompt,
    handleInsertPendingPrompt,
    handleStopConversationRun,
    handleCompactSelectedConversationContext,
    handleOpenSubagentPanel,
    handleCloseSubagentPanel,
    handleStopSubagentTask,
    handleRemovePendingPrompt,
    handleReorderPendingPrompts,
    handleSelectWorkbenchModel,
    handleStopCommandSession,
    handleListWorkbenchModelSecondaryRoutes,
    handleSelectWorkbenchModelSecondaryRoute,
    handleGetUserPromptEditDraft,
    handleSubmitUserPromptEdit,
    handleForkConversation,
    handleSelectWorkbenchReasoningEffort,
    handleSelectWorkbenchThinkingDisplayMode,
  } = useWorkbenchConversationActions({
    translate: i18n.t,
    selectedConversation,
    selectedConversationId,
    selectedDraftConversation,
    setWorkbenchState,
    mergeWorkbenchWithLocalCaches,
    setDraftConversation,
    setSelectedConversationId,
    selectedConversationIdRef,
    workbenchStateRef,
    subagentNavigationRequestSeqRef,
    subagentRoutePreferenceRef,
    inflightSubagentOpenRequestRef,
    subagentSurfaceMessageCacheRef,
  });

  // handleConversationMenuAction executes conversation-tree context menu commands.
  // handleConversationMenuAction 执行会话树右键菜单命令。
  const handleConversationMenuAction = React.useCallback(async (action: ConversationMenuAction, conversation: ConversationSummary) => {
    try {
      if (action === "rename") {
        setRenameTarget({ kind: "conversation", id: conversation.id, name: conversation.title });
        return;
      }
      if (action === "pin") {
        setPinnedConversationIds((current) => {
          const next = new Set(current);
          if (next.has(conversation.id)) {
            next.delete(conversation.id);
            toast.success(`已取消置顶对话：${conversation.title}`);
          } else {
            next.add(conversation.id);
            toast.success(`已置顶对话：${conversation.title}`);
          }
          return next;
        });
        return;
      }
      if (action === "archive") {
        const snapshot = await archiveConversation(conversation.id);
        if (snapshot) {
          applyProjectActionSnapshot(snapshot);
        } else {
          applyProjectActionSnapshot(applyPreviewConversationAction(workbenchState, conversation.id, action).snapshot);
        }
        toast.success(`已归档对话：${conversation.title}`);
        return;
      }
      if (action === "open-directory") {
        const project = projectForConversation(workbenchState, conversation);
        if (!project) {
          toast.error("没有找到这个对话所属的项目。");
          return;
        }
        const message = await openProjectDirectoryPath(project.path);
        toast.success(message ?? `已请求打开项目目录：${project.name}`);
        return;
      }
      if (action === "copy-workdir") {
        const project = projectForConversation(workbenchState, conversation);
        if (!project) {
          toast.error("没有找到这个对话所属的项目。");
          return;
        }
        await navigator.clipboard.writeText(project.path);
        toast.success("已复制工作目录。");
        return;
      }
      if (action === "copy-session-id") {
        await navigator.clipboard.writeText(conversation.id);
        toast.success("已复制会话 ID。");
        return;
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleSelectConversation selects a conversation and persists its read marker.
  // handleSelectConversation 选择会话并持久化它的已读标记。
  const handleSelectConversation = React.useCallback((conversationId: string) => {
    handleOpenWorkspaceChat();
    subagentNavigationRequestSeqRef.current += 1;
    subagentRoutePreferenceRef.current = { parentConversationId: conversationId, taskId: null };
    setSelectedConversationId(conversationId);
    setWorkbenchState((current) => closeSubagentSurfaceForConversation(
      markConversationLocallyRead(current, conversationId),
      conversationId,
    ));
    void selectConversation(conversationId)
      .then((snapshot) => {
        if (snapshot) {
          setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
        }
        return markConversationRead(conversationId);
      })
      .then((snapshot) => {
        if (snapshot) {
          setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
        }
      })
      .catch((error) => {
        toast.error(error instanceof Error ? error.message : String(error));
      });
  }, [handleOpenWorkspaceChat]);

  // handleRenameConfirm commits the current rename dialog value.
  // handleRenameConfirm 提交当前重命名对话框值。
  const handleRenameConfirm = React.useCallback(async (target: RenameTarget, name: string) => {
    const normalizedName = name.trim();
    if (!normalizedName && target.kind !== "project") {
      toast.error("名称不能为空。");
      return;
    }
    const project = target.kind === "project"
      ? workbenchState.projects.find((candidate) => candidate.id === target.id) ?? null
      : null;
    const effectiveProjectName = normalizedName || restoreProjectDisplayName(project, target);
    try {
      const snapshot = target.kind === "project"
        ? await renameProject(target.id, effectiveProjectName)
        : await renameConversation(target.id, normalizedName);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      } else if (target.kind === "project") {
        applyProjectActionSnapshot(applyPreviewProjectRename(workbenchState, target.id, effectiveProjectName).snapshot);
      } else {
        applyProjectActionSnapshot(applyPreviewConversationRename(workbenchState, target.id, normalizedName).snapshot);
      }
      setRenameTarget(null);
      toast.success(target.kind === "project"
        ? normalizedName
          ? `已重命名项目：${effectiveProjectName}`
          : `已还原项目名称：${effectiveProjectName}`
        : `已重命名对话：${normalizedName}`);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleArchiveProjectConfirm archives all conversations under the selected project.
  // handleArchiveProjectConfirm 归档选定项目下的全部对话。
  const handleArchiveProjectConfirm = React.useCallback(async () => {
    if (!archiveProjectTarget || archiveProjectSubmitting) {
      return;
    }
    setArchiveProjectSubmitting(true);
    try {
      const snapshot = await archiveProjectConversations(archiveProjectTarget.id);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      } else {
        applyProjectActionSnapshot(applyPreviewProjectAction(workbenchState, archiveProjectTarget.id, "archive-conversations").snapshot);
      }
      toast.success(`已归档 ${projectConversationCount(workbenchState, archiveProjectTarget.id)} 个对话。`);
      setArchiveProjectTarget(null);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      setArchiveProjectSubmitting(false);
    }
  }, [applyProjectActionSnapshot, archiveProjectSubmitting, archiveProjectTarget, workbenchState]);

  // handleRemoveProjectConfirm archives conversations and removes the selected project.
  // handleRemoveProjectConfirm 归档对话并移除选中的项目。
  const handleRemoveProjectConfirm = React.useCallback(async () => {
    if (!removeProjectTarget || removeProjectSubmitting) {
      return;
    }
    setRemoveProjectSubmitting(true);
    try {
      const snapshot = await removeProject(removeProjectTarget.id);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      } else {
        applyProjectActionSnapshot(applyPreviewProjectAction(workbenchState, removeProjectTarget.id, "remove-project").snapshot);
      }
      toast.success(`已归档并移除项目：${removeProjectTarget.name}`);
      setRemoveProjectTarget(null);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      setRemoveProjectSubmitting(false);
    }
  }, [applyProjectActionSnapshot, removeProjectSubmitting, removeProjectTarget, workbenchState]);

  // handlePendingQuestionSubmit resumes a blocked ask tool call with validated modal answers.
  // handlePendingQuestionSubmit 使用弹窗校验后的答案恢复被阻塞的 ask 工具调用。
  const handlePendingQuestionSubmit = React.useCallback(async (requestId: string, answers: string[][]) => {
    try {
      const snapshot = await replyPendingQuestion(requestId, answers);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
      toast.success("已提交 ask 回答。");
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [mergeWorkbenchWithLocalCaches]);

  // handlePendingQuestionReject cancels a blocked ask tool call after a second confirmation.
  // handlePendingQuestionReject 在二次确认后取消被阻塞的 ask 工具调用。
  const handlePendingQuestionReject = React.useCallback(async (requestId: string) => {
    try {
      const snapshot = await rejectPendingQuestion(requestId);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
      toast.info("已取消 ask 请求。");
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [mergeWorkbenchWithLocalCaches]);

  React.useEffect(() => {
    draftConversationIdRef.current = draftConversation?.id ?? null;
  }, [draftConversation?.id]);

  React.useEffect(() => {
    setWorkbenchState((current) => applySubagentRoutePreference(
      mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache(
        current,
        props.workbench,
        subagentSurfaceMessageCacheRef.current,
        { preserveExpandedProjectIds: expandedProjectIdsLocallyEditedRef.current },
      ),
      subagentRoutePreferenceRef.current,
    ));
    const nextSelected = props.workbench.selectedConversationId ?? props.workbench.conversations[0]?.id ?? null;
    setSelectedConversationId((current) => {
      if (current && current === draftConversationIdRef.current) {
        return current;
      }
      if (current && props.workbench.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return nextSelected;
    });
  }, [props.workbench]);

  React.useEffect(() => {
    if (!props.workbenchDelta) {
      return;
    }
    setWorkbenchState((current) => applySubagentRoutePreference(
      props.workbenchDelta!.envelopes.reduce(
        (nextWorkbench, envelope) => applyWorkbenchDeltaWithSubagentSurfaceMessageCache(
          nextWorkbench,
          envelope,
          subagentSurfaceMessageCacheRef.current,
        ),
        current,
      ),
      subagentRoutePreferenceRef.current,
    ));
  }, [props.workbenchDelta]);

  // handleComposerSlashCommand routes slash commands into existing workbench actions.
  // handleComposerSlashCommand 将 slash 指令路由到现有工作台动作。
  const handleComposerSlashCommand = React.useCallback((command: ComposerSlashCommandSummary): ComposerSlashCommandResult => {
    switch (normalizedComposerSlashCommandId(command)) {
      case "model":
        return { handled: true, openTarget: "model" };
      case "reasoning":
        return { handled: true, openTarget: "reasoning" };
      case "skill":
        return { handled: true, replacementText: SLASH_SKILL_PROMPT };
      case "skills":
        handleOpenWorkspaceSkills();
        return { handled: true };
      case "compact":
        void handleCompactSelectedConversationContext();
        return { handled: true };
      default:
        return { handled: false };
    }
  }, [handleCompactSelectedConversationContext, handleOpenWorkspaceSkills]);

  return (
    <div className={`app-shell ${showConsoleSidebar ? "with-console-sidebar" : "without-console-sidebar"} ${showWindowControls ? "tauri-shell" : "browser-shell"}`}>
      <main className="workspace">
        {showConsoleSidebar ? (
          <WorkspaceShell
            startup={props.startup}
            workbench={consoleWorkbenchState}
            activeSkillSubArea={activeWorkbenchSkillSubArea}
            activeMemorySubArea={activeWorkbenchMemorySubArea}
            mainView={workspaceMainView}
            sidebarOpen={navigationSidebarOpen}
            onSidebarOpenChange={setNavigationSidebarOpen}
            selectedDraftConversation={selectedDraftConversation}
            selectedConversation={selectedConversation}
            selectedConversationId={selectedConversationId}
            chromeActions={shellChromeActions}
            titlebarDragRegion={showWindowControls}
            onTitlebarDoubleClick={handleTitlebarDoubleClick}
            loadingConversationId={loadingConversationId}
            conversationLoadError={conversationLoadError}
            conversationHydration={workbenchState.conversationHydration ?? null}
            conversationHasMoreHistory={Boolean(selectedConversationHistoryCursor?.hasMoreHistory)}
            conversationHistoryLoading={selectedConversationId ? conversationHistoryLoadingIds.has(selectedConversationId) : false}
            pinnedConversationIds={pinnedConversationIds}
            onConversationMenuAction={handleConversationMenuAction}
            onCreateConversation={handleCreateConversation}
            onCreateProject={handleCreateProject}
            onOpenSkills={handleOpenWorkspaceSkills}
            onOpenMemory={handleOpenWorkspaceMemory}
            onSelectArea={handleSelectTopFeatureArea}
            onSelectSkillSubArea={setActiveWorkbenchSkillSubArea}
            onSelectMemorySubArea={setActiveWorkbenchMemorySubArea}
            onExpandedProjectIdsChange={handleExpandedProjectIdsChange}
            onProjectMenuAction={handleProjectMenuAction}
            onReorderProjects={handleReorderProjects}
            onSelectConversation={handleSelectConversation}
            onLoadConversationHistory={handleLoadConversationHistory}
            onSelectWorkbenchModel={handleSelectWorkbenchModel}
            onListWorkbenchModelSecondaryRoutes={handleListWorkbenchModelSecondaryRoutes}
            onSelectWorkbenchModelSecondaryRoute={handleSelectWorkbenchModelSecondaryRoute}
            onSelectWorkbenchReasoningEffort={handleSelectWorkbenchReasoningEffort}
            onSelectWorkbenchThinkingDisplayMode={handleSelectWorkbenchThinkingDisplayMode}
            skillActions={skillActions}
            memoryActions={{
              onRunVmmAction: runVmmMemoryAction,
              onSaveVmmConfig: (request) => applySkillWorkbenchAction(() => saveVmmMemoryConfig(request)),
            }}
            onSlashCommand={handleComposerSlashCommand}
            onSubmitPrompt={handleSubmitConversationPrompt}
            onSaveAiCapabilityConfiguration={handleSaveAiCapabilityConfiguration}
            onSaveSessionGoal={handleSaveSessionGoal}
            onResumeSessionGoal={handleResumeSessionGoal}
            onClearSessionGoal={handleClearSessionGoal}
            onUpdateSessionGoalStatus={handleUpdateSessionGoalStatus}
            onSetSessionPlanMode={handleSetSessionPlanMode}
            onStopConversationRun={handleStopConversationRun}
            onOpenSubagentPanel={handleOpenSubagentPanel}
            onCloseSubagentPanel={handleCloseSubagentPanel}
            onStopSubagentTask={handleStopSubagentTask}
            onStopCommandSession={handleStopCommandSession}
            onEditPendingPrompt={handleEditPendingPrompt}
            onInsertPendingPrompt={handleInsertPendingPrompt}
            onRemovePendingPrompt={handleRemovePendingPrompt}
            onReorderPendingPrompts={handleReorderPendingPrompts}
            onLoadWorkspaceDiffFileDetail={loadWorkspaceDiffFileDetail}
            onLoadWorkspaceDiffSearch={loadWorkspaceDiffSearch}
            onLoadWorkspaceDiffSnapshot={loadWorkspaceDiffSnapshot}
            onLoadWorkspaceFileDirectory={loadWorkspaceFileDirectory}
            onLoadWorkspaceFilePreview={loadWorkspaceFilePreview}
            onRunWorkspaceGitFileAction={runWorkspaceGitFileAction}
            onStageAttachmentBytes={stageComposerAttachmentBytes}
            onCopyText={copyText}
            onGetUserPromptEditDraft={handleGetUserPromptEditDraft}
            onSubmitUserPromptEdit={handleSubmitUserPromptEdit}
            onForkConversation={handleForkConversation}
            onLoadToolDetail={loadCachedConversationToolDetail}
            resolveMediaSourceUrl={localMediaSourceUrl}
            footer={(
              <footer className="statusbar workspace-statusbar">
                <span>版本 {props.workbench.appVersion}</span>
                <StatusbarRuntimeActivities activities={statusbarRuntimeActivities} />
                <RepositoryLink onOpenExternalUrl={openExternalUrl} />
              </footer>
            )}
            onTrace={webuiClickTrace}
          />
        ) : (
          <ExtractedFeatureShell
            activeArea={activeArea}
            activeFeature={activeFeature}
            sidebarOpen={navigationSidebarOpen}
            onSidebarOpenChange={setNavigationSidebarOpen}
            activeSettingsSubArea={activeSettingsSubArea}
            onSelectSettingsSubArea={setActiveSettingsSubArea}
            onSelectArea={handleSelectTopFeatureArea}
            skillActions={skillActions}
            memoryActions={{
              onRunVmmAction: runVmmMemoryAction,
              onSaveVmmConfig: (request) => applySkillWorkbenchAction(() => saveVmmMemoryConfig(request)),
            }}
            settingsActions={{
              onSaveProvider: (request) => applySkillWorkbenchAction(async () => (await saveProviderConfiguration(request)) ?? workbenchStateRef.current),
              onRemoveProvider: (providerId) => applySkillWorkbenchAction(async () => (await removeProviderConfiguration(providerId)) ?? workbenchStateRef.current),
              onSaveProviderEnabledModels: (request) => applySkillWorkbenchAction(async () => (await saveProviderEnabledModels(request)) ?? workbenchStateRef.current),
              onSaveProviderModelOverride: (request) => applySkillWorkbenchAction(async () => (await saveProviderModelOverride(request)) ?? workbenchStateRef.current),
              onSaveGitCommitSettings: (request) => applySkillWorkbenchAction(async () => (await saveGitCommitSettings(request)) ?? workbenchStateRef.current),
              onSaveRtkRoutingSettings: (request) => applySkillWorkbenchAction(async () => (await saveRtkRoutingSettings(request)) ?? workbenchStateRef.current),
              onGetRtkGainStats: () => getRtkGainStats(),
              onSaveAiCapabilityVendor: (request) => applySkillWorkbenchAction(async () => (await saveAiCapabilityVendor(request)) ?? workbenchStateRef.current),
              onRemoveAiCapabilityVendor: (vendorId) => applySkillWorkbenchAction(async () => (await removeAiCapabilityVendor(vendorId)) ?? workbenchStateRef.current),
              onSetAiCapabilityTargetEnabled: (request) => applySkillWorkbenchAction(async () => (await setAiCapabilityTargetEnabled(request)) ?? workbenchStateRef.current),
              onSaveAiCapabilityTargets: (request) => applySkillWorkbenchAction(async () => (await saveAiCapabilityTargets(request)) ?? workbenchStateRef.current),
              onSaveMcpServer: (request) => applySkillWorkbenchAction(async () => (await saveMcpServerConfiguration(request as unknown as Record<string, unknown>)) ?? workbenchStateRef.current),
              onRemoveMcpServer: (serverId) => applySkillWorkbenchAction(async () => (await removeMcpServerConfiguration(serverId)) ?? workbenchStateRef.current),
            }}
            startup={props.startup}
            workbench={workbenchState}
            chromeActions={shellChromeActions}
            titlebarDragRegion={showWindowControls}
            onTitlebarDoubleClick={handleTitlebarDoubleClick}
          />
        )}
        {showStartupOverlay ? <StartupOverlay startup={props.startup} /> : null}
      </main>
      <WorkbenchDialogs
        archiveProjectSubmitting={archiveProjectSubmitting}
        archiveProjectTarget={archiveProjectTarget}
        pendingQuestion={workbenchState.pendingQuestion ?? null}
        projectFolderPickerOpen={projectFolderPickerOpen}
        projectIconTarget={projectIconTarget}
        projectSettingsTarget={projectSettingsTarget}
        removeProjectSubmitting={removeProjectSubmitting}
        removeProjectTarget={removeProjectTarget}
        renameTarget={renameTarget}
        workbench={workbenchState}
        onBrowseProjectDirectories={browseProjectDirectories}
        onCancelArchiveProject={() => {
          if (!archiveProjectSubmitting) {
            setArchiveProjectTarget(null);
          }
        }}
        onCancelProjectIcon={() => setProjectIconTarget(null)}
        onCancelProjectSettings={() => setProjectSettingsTarget(null)}
        onCancelRemoveProject={() => {
          if (!removeProjectSubmitting) {
            setRemoveProjectTarget(null);
          }
        }}
        onCancelRename={() => setRenameTarget(null)}
        onConfirmArchiveProject={handleArchiveProjectConfirm}
        onConfirmProjectFolder={handleConfirmProjectFolder}
        onConfirmProjectIcon={handleProjectIconConfirm}
        onConfirmRemoveProject={handleRemoveProjectConfirm}
        onConfirmRename={handleRenameConfirm}
        onExecuteAttachmentCleanup={executeProjectAttachmentCleanup}
        onLoadAgentSkillOverrides={loadProjectAgentSkillOverrides}
        onLoadVmmDraft={loadProjectVmmConfig}
        onPreviewAttachmentCleanup={previewProjectAttachmentCleanup}
        onProjectFolderPickerOpenChange={setProjectFolderPickerOpen}
        onRejectPendingQuestion={handlePendingQuestionReject}
        onResetProjectIcon={handleProjectIconReset}
        onSaveVmmDraft={handleProjectSettingsVmmSave}
        onSetAgentSkillState={handleProjectSettingsAgentSkillSave}
        onSubmitPendingQuestion={handlePendingQuestionSubmit}
      />
    </div>
  );
}

