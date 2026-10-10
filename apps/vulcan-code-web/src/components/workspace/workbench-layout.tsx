import React from "react";
import { ComposerSettingsHostContext } from "@/components/composer/composer-settings-host";
import { DraftComposerContext, type DraftComposerContextValue } from "@/components/composer/dedicated/draft-composer-context";
import { ToolCollapsePreferencesContext } from "@/components/conversation/tool-collapse-preferences";
import { ThinkingDisplaySystemContext } from "@/components/conversation/thinking-display-preferences";
import { VceProjectSurfaceProvider } from "@/components/features/code-context/vce-project-surface";
import { VceJobObserverProvider } from "@/components/features/code-context/vce-job-observer-context";
import type { SkillActionProps } from "@/components/features/skills/skill-area-types";
import {
  FeatureShell as ExtractedFeatureShell,
  areaIconMap,
  pluginDetailMountIcon,
  primaryFeatureAreas as buildPrimaryFeatureAreas,
} from "@/components/features/feature-shell";
import { ActivityRail, type ActivityRailEntry } from "@/components/shell/activity-rail";
import { RepositoryLink } from "@/components/shell/app-chrome";
import { StartupOverlay } from "@/components/shell/startup-screen";
import {
  normalizedComposerSlashCommandId,
  SLASH_SKILL_PROMPT,
  type ComposerSlashCommandResult,
} from "@/components/composer/composer-model";
import {
  restoreProjectDisplayName,
  type RenameTarget,
} from "@/components/project/project-action-dialogs";
import {
  type ProjectIconDialogTarget,
} from "@/components/project/project-icons";
import {
  type ProjectSettingsDialogTarget,
} from "@/components/project/project-settings-types";
import { ProjectSessionTree } from "@/components/project/project-sidebar";
import {
  ProjectFloatingTerminalLayer,
  type ProjectTerminalVisibilityRequest,
} from "@/components/project/terminal/project-floating-terminal-layer";
import {
  type DraftConversationSelection,
} from "@/components/workspace/workspace-resizable-body";
import { retireMaterializedDraftSubmission } from "@/components/workspace/draft-submission-state";
import { resolveEmptyWorkbenchDraftRoute } from "@/components/workspace/workbench-layout/empty-workbench-route";
import { ComposerMediaUploadProgressContext } from "@/components/composer/composer-media-upload-context";
import {
  mediaUploadProgressKey,
} from "@/components/workspace/media-upload-progress-state";
import { routeWorkspaceInputOwnerFromPointer } from "@/components/workspace/workspace-input-owner";

import { useWorkbenchShellChrome } from "@/components/workspace/workbench-layout/shell-chrome";
import {
  copyText,
  formatWorkbenchTranslation,
  webuiClickTrace,
  type WorkbenchDeltaSignal,
} from "@/components/workspace/workbench-layout/types";
import { useWorkbenchConversationActions } from "@/components/workspace/workbench-layout/conversation-actions";
import { useWorkbenchProjectActions } from "@/components/workspace/workbench-layout/project-actions";
import { useWorkbenchSessionActions } from "@/components/workspace/workbench-layout/session-actions";
import { useStopConversationRunDecision } from "@/components/workspace/workbench-layout/use-stop-conversation-run-decision";
import { useWorkbenchRevisionGates, useWorkbenchServiceSync } from "@/components/workspace/workbench-layout/use-workbench-revision-sync";
import { useProjectTerminalRunningCounts } from "@/components/workspace/workbench-layout/use-project-terminal-running-counts";
import { useProjectLifecycleConfirmation } from "@/components/workspace/workbench-layout/use-project-lifecycle-confirmation";
import { useConversationMenuActions } from "@/components/workspace/workbench-layout/use-conversation-menu-actions";
import { useConversationSelectionNavigation } from "@/components/workspace/workbench-layout/use-conversation-selection-navigation";
import { PermissionJudgeEntryContext, type PermissionJudgeEntry } from "@/components/composer/permission-judge-entry-context";
import { useSetupGuide } from "@/components/features/onboarding/use-setup-guide";
export type { WorkbenchDeltaSignal } from "@/components/workspace/workbench-layout/types";
import {
  WorkspaceShell,
  type WorkspaceMainView,
} from "@/components/workspace/workspace-shell";
import { WORKSPACE_FEATURE_AREAS } from "@/components/workspace/workspace-shell/types";
import type {
  SkillSubAreaId,
} from "@/components/features/skills/skills-area";
import {
  enabledPluginDetailMounts,
  findEnabledPluginDetailMount,
  isPluginDetailSubArea,
  localizedPluginMountLabel,
} from "@/components/features/skills/skill-area-types";
import type {
  MemorySubAreaId,
} from "@/components/features/memory/memory-area";
import { useI18n, type LocaleCode } from "@/i18n";
import { notificationCoordinator } from "@/notifications/coordinator";
import {
  resetNotificationPresentationContext,
  setNotificationPresentationContext,
} from "@/notifications/presentation-context";
import type { NotificationPresentationContext } from "@/notifications/types";
import { useConversationLiveStreamStore } from "@/state/conversation-live-stream-context";
import type {
  ComposerSlashCommandSummary,
  FeatureAreaSummary,
  MainAreaId,
  ProjectSummary,
  SettingsSubAreaId,
  StartupSnapshot,
  WorkbenchSnapshot,
  MediaUploadProgressChangedDelta,
} from "@/types";
import {
  rejectPendingQuestion,
  replyPendingQuestion,
  stageComposerAttachmentBytes,
} from "@/service/conversation-actions-service";
import {
  browseProjectDirectories,
  deleteArchivedConversations,
  openExternalUrl,
  renameConversation,
  renameProject,
  restoreArchivedConversations,
} from "@/service/project-actions-service";
import {
  clearSelectedProviderApiTraceDirectory,
  openSelectedProviderApiTraceDirectory,
  removeMcpServerConfiguration,
  saveGitCommitSettings,
  saveProviderRequestTraceSettings,
  saveProviderRetrySettings,
  saveMcpServerConfiguration,
  savePromptAnswerMode,
  savePromptOutputStyle,
  selectWebuiLocale,
} from "@/service/settings-actions-service";
import {
  loadProjectAgentSkillOverrides,
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
  loadWorkbenchSnapshot,
} from "@/service/workbench-snapshot-service";
import { localMediaSourceUrl } from "@/resources/localMedia";
import {
  markConversationLocallyRead,
  workbenchWithSelectedConversationProjection,
} from "@/state/workbench-state";
import { toast } from "sonner";

// WorkbenchDialogs loads project and question dialogs only after one actually opens.
// WorkbenchDialogs 仅在项目或提问对话框真正打开后加载其界面。
const WorkbenchDialogs = React.lazy(() => import("@/components/workspace/workbench-dialogs")
  .then((module) => ({ default: module.WorkbenchDialogs })));
const SetupGuideDialog = React.lazy(() => import("@/components/features/onboarding/setup-guide-dialog"));

// MediaUploadProgressPanel loads only for an active transfer while preserving the stream-owned progress row.
// MediaUploadProgressPanel 仅在传输进行时加载，同时保留由事件流管理的进度行。
const MediaUploadProgressPanel = React.lazy(() => import("@/components/workspace/media-upload-progress-panel")
  .then((module) => ({ default: module.MediaUploadProgressPanel })));

// WorkbenchLayout renders the WebUI workbench shell.
// WorkbenchLayout 渲染 WebUI 工作台外壳。
export function WorkbenchLayout(props: {
  // Browser-visible local service generation used to reset revision ordering after a reconnect.
  // 浏览器可见的本地服务代次，用于在重连后重置版本排序。
  serviceInstanceEpoch: number;
  // Latest startup snapshot displayed by the shell blocker.
  // 壳层阻塞面板展示的最近启动快照。
  startup: StartupSnapshot;
  // Latest full workbench snapshot from the backend.
  // 后端返回的最近完整工作台快照。
  workbench: WorkbenchSnapshot;
  // Latest partial workbench update signal from the backend.
  // 后端返回的最近局部工作台更新信号。
  workbenchDelta: WorkbenchDeltaSignal | null;
  // Acknowledges one exact partial signal after every envelope has been considered.
  // 在每条增量都完成处理后确认一个精确的局部信号。
  onWorkbenchDeltaConsumed: (sequence: number) => void;
  // Publishes the exact durable conversation selected by the local workbench route.
  // 发布本地工作台路由选中的精确持久化会话。
  onSelectedConversationIdChanged: (conversationId: string | null) => void;
  // Reports whether the exact conversation already owns a hydrated event window.
  // 报告精确会话是否已有完成初始化的事件窗口。
  isConversationInitialized: (conversationId: string) => boolean;
}) {
  React.useEffect(() => {
    // handleWorkspacePointerOwner routes input ownership from explicit pointer targets before bubble handlers run.
    // handleWorkspacePointerOwner 在冒泡处理器运行前，根据明确指针目标路由输入所有权。
    const handleWorkspacePointerOwner = (event: PointerEvent) => {
      routeWorkspaceInputOwnerFromPointer(event.target);
    };
    document.addEventListener("pointerdown", handleWorkspacePointerOwner, true);
    return () => document.removeEventListener("pointerdown", handleWorkspacePointerOwner, true);
  }, []);
  // settingsHost moves the active composer's menu into the footer without moving state ownership.
  // settingsHost 将当前输入区菜单显示到底栏，不迁移状态归属。
  const [settingsHost, setSettingsHost] = React.useState<HTMLDivElement | null>(null);
  // todolistStatusHost stores the footer node immediately following the settings entry.
  // todolistStatusHost 保存紧跟设置入口的底栏 Todo 节点。
  const [todolistStatusHost, setTodolistStatusHost] = React.useState<HTMLDivElement | null>(null);
  // runtimeStatusHost stores the center footer node for the current API request state.
  // runtimeStatusHost 保存当前 API 请求状态使用的底栏中央节点。
  const [runtimeStatusHost, setRuntimeStatusHost] = React.useState<HTMLDivElement | null>(null);
  // liveStreamStore reconciles selection snapshots before the new conversation subtree mounts.
  // liveStreamStore 在新会话子树挂载前对账选择快照。
  const liveStreamStore = useConversationLiveStreamStore();
  // i18n provides localized shell labels and the active language.
  // i18n 提供本地化外壳标签与当前语言。
  const i18n = useI18n();
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
  // handleNotificationPresentationContextChange publishes the exact child workspace surface.
  // handleNotificationPresentationContextChange 发布精确的子工作区界面。
  const handleNotificationPresentationContextChange = React.useCallback(
    (context: NotificationPresentationContext) => {
      setNotificationPresentationContext(context);
    },
    [],
  );
  React.useEffect(() => {
    if (activeArea !== "chat") {
      resetNotificationPresentationContext();
    }
    return () => {
      resetNotificationPresentationContext();
    };
  }, [activeArea]);
  // sessionPanelOpen is the only collapsible panel state; feature menus and the settings tree always stay beside the rail.
  // sessionPanelOpen 是唯一可折叠的面板状态；功能菜单与设置树始终位于轨道旁。
  const [sessionPanelOpen, setSessionPanelOpen] = React.useState(true);
  // workspaceMainView stores the selected main surface inside the workbench.
  // workspaceMainView 保存工作台内部当前选中的主内容界面。
  const [workspaceMainView, setWorkspaceMainView] = React.useState<WorkspaceMainView>("chat");
  // activeWorkbenchSkillSubArea stores the embedded skills settings sub page.
  // activeWorkbenchSkillSubArea 保存内嵌技能设置的子页面。
  const [activeWorkbenchSkillSubArea, setActiveWorkbenchSkillSubArea] = React.useState<SkillSubAreaId>("catalog");
  // activeWorkbenchMemorySubArea stores the embedded memory settings sub page.
  // activeWorkbenchMemorySubArea 保存内嵌记忆设置的子页面。
  // activeWorkbenchMemorySubArea starts with the memory reading workspace.
  // activeWorkbenchMemorySubArea 从记忆阅读工作区开始。
  const [activeWorkbenchMemorySubArea, setActiveWorkbenchMemorySubArea] = React.useState<MemorySubAreaId>("content");
  // initialDraftConversationRef computes the first-run draft once for local selection state.
  // initialDraftConversationRef 为本地选择状态只计算一次首次启动草稿。
  const initialDraftConversationRef = React.useRef<DraftConversationSelection | null | undefined>(undefined);
  if (initialDraftConversationRef.current === undefined) {
    initialDraftConversationRef.current = resolveEmptyWorkbenchDraftRoute(
      props.workbench,
      null,
      null,
    )?.draftConversation ?? null;
  }
  const initialDraftConversation = initialDraftConversationRef.current;
  // selectedConversationId stores the conversation selected in the sidebar.
  // selectedConversationId 保存侧栏当前选中的会话。
  const [selectedConversationId, setSelectedConversationId] = React.useState<string | null>(
    props.workbench.selectedConversationId
      ?? initialDraftConversation?.id
    ?? null,
  );
  // Stop decision controller binds confirmation to the conversation selected when preflight started.
  // 停止决策控制器将确认绑定到预检开始时选中的会话。
  const {
    agents: stopConversationAgents,
    request: requestStopConversationRunChoice,
    resolve: resolveStopConversationRunChoice,
  } = useStopConversationRunDecision({ activeConversationId: selectedConversationId });
  // selectedConversationIdRef lets async submit completions avoid stealing focus after the user switches sessions.
  // selectedConversationIdRef 让异步提交完成时避免在用户已切换会话后抢回焦点。
  const selectedConversationIdRef = React.useRef<string | null>(selectedConversationId);
  // draftConversation stores the local-only conversation opened before the first user message is sent.
  // draftConversation 保存发送第一条用户消息前打开的本地草稿会话。
  const [draftConversation, setDraftConversation] = React.useState<DraftConversationSelection | null>(
    initialDraftConversation,
  );
  // draftConversationIdRef keeps the latest draft id without retriggering workbench snapshot hydration.
  // draftConversationIdRef 保存最新草稿标识，避免重新触发工作台快照水合。
  const draftConversationIdRef = React.useRef<string | null>(null);
  // pendingMaterializedConversationIdRef protects a committed draft selection until its sidebar summary arrives.
  // pendingMaterializedConversationIdRef 在侧栏摘要到达前保护已提交草稿对应的会话选择。
  const pendingMaterializedConversationIdRef = React.useRef<string | null>(null);
  // workbenchState stores local message hydration updates.
  // workbenchState 保存本地会话正文水合更新。
  const [workbenchState, setWorkbenchState] = React.useState<WorkbenchSnapshot>(() => props.workbench);
  // activeProjectId owns project-level tools independently from conversation lifetime.
  // activeProjectId 独立于会话生命周期持有项目级工具。
  const [activeProjectId, setActiveProjectId] = React.useState<string | null>(() => {
    const selectedProjectId = props.workbench.conversations.find(
      (conversation) => conversation.id === selectedConversationId,
    )?.projectId ?? initialDraftConversation?.projectId ?? null;
    return props.workbench.projects.some((project) => project.id === selectedProjectId)
      ? selectedProjectId
      : props.workbench.projects[0]?.id ?? null;
  });
  // projectTerminalOpenRequest carries explicit open or toggle commands to the project floating terminal.
  // projectTerminalOpenRequest 向项目浮动终端传递明确的展开或切换命令。
  const [projectTerminalOpenRequest, setProjectTerminalOpenRequest] = React.useState<ProjectTerminalVisibilityRequest | null>(null);
  // projectTerminalButtonRef exposes the exact top-bar target for the macOS-style Genie animation.
  // projectTerminalButtonRef 暴露 macOS 风格精灵动画使用的精确顶部栏目标。
  const projectTerminalButtonRef = React.useRef<HTMLButtonElement | null>(null);
  // mediaUploadProgresses keeps every active pre-dispatch or tool-triggered transfer keyed by its exact identity.
  // mediaUploadProgresses 按精确身份保存所有活跃的分派前或工具触发传输，避免并发上传互相覆盖。
  const [mediaUploadProgresses, setMediaUploadProgresses] = React.useState<Record<string, MediaUploadProgressChangedDelta>>({});
  // floatingMediaUploadProgresses excludes draft transfers because their progress is rendered on the attachment tile.
  // floatingMediaUploadProgresses 排除草稿传输，因为其进度直接渲染在附件块上。
  const floatingMediaUploadProgresses = React.useMemo(
    () => Object.values(mediaUploadProgresses).filter(
      (progress) => !progress.clientSubmissionId.startsWith("draft-media:"),
    ),
    [mediaUploadProgresses],
  );
  // projectTerminalRunningCounts stores count-only lifecycle state independently from terminal content.
  // projectTerminalRunningCounts 独立于终端内容保存仅包含数量的生命周期状态。
  const projectTerminalRunningCounts = useProjectTerminalRunningCounts(props.serviceInstanceEpoch, activeProjectId);
  React.useEffect(() => {
    if (
      isPluginDetailSubArea(activeWorkbenchSkillSubArea)
      && !findEnabledPluginDetailMount(workbenchState.plugins, activeWorkbenchSkillSubArea)
    ) {
      setActiveWorkbenchSkillSubArea("plugins");
    }
  }, [activeWorkbenchSkillSubArea, workbenchState.plugins]);
  // workbenchStateRef exposes the latest workbench state to route-click callbacks without stale closures.
  // workbenchStateRef 向路由点击回调暴露最新工作台状态，避免闭包过期。
  const workbenchStateRef = React.useRef<WorkbenchSnapshot>(workbenchState);
  workbenchStateRef.current = workbenchState;
  React.useEffect(() => {
    // Ephemeral subscribers observe the exact batch after React commits the corresponding structural state.
    // 瞬态订阅者在 React 提交对应结构状态后观察精确批次。
    liveStreamStore.flushNotifications();
  }, [liveStreamStore, workbenchState]);
  // revisionGates owns every service revision gate and the snapshot merges that advance them.
  // revisionGates 持有全部服务版本闸门，以及推进这些闸门的快照合并。
  const revisionGates = useWorkbenchRevisionGates({
    serviceInstanceEpoch: props.serviceInstanceEpoch,
    workbenchState,
    setWorkbenchState,
    workbenchStateRef,
  });
  const {
    mergeWorkbenchWithLocalCaches,
    acceptServiceWorkbenchSnapshot,
    applyLifecycleResponseSnapshot,
    applyLocalPreviewWithLocalCaches,
    applyLuaSkillsCatalogUpdate,
    applyAgentSkillsCatalogUpdate,
    applySubagentCatalogUpdate,
    projectSettingsRevisionById,
    globalProjectSettingsRevision,
  } = revisionGates;
  // expandedProjectIdsLocallyEditedRef records whether the user has touched project expansion in this session.
  // expandedProjectIdsLocallyEditedRef 记录本轮会话中用户是否已经手动修改过项目展开状态。
  const expandedProjectIdsLocallyEditedRef = React.useRef(false);
  // localeSaveSeqRef prevents older locale-save responses from overriding newer choices.
  // localeSaveSeqRef 防止较早的语言保存响应覆盖较新的选择。
  const localeSaveSeqRef = React.useRef(0);
  // handleWebuiLocaleChange loads the requested language before applying and persisting it.
  // handleWebuiLocaleChange 在应用及持久化所选语言之前先加载其目录。
  const handleWebuiLocaleChange = React.useCallback((locale: LocaleCode) => {
    const previousLocale = i18n.locale;
    const requestSeq = localeSaveSeqRef.current + 1;
    localeSaveSeqRef.current = requestSeq;
    void i18n.setLocale(locale).then(() => {
      // A superseded download must not persist a language the user has already moved away from.
      // 已被替代的下载不得持久化用户已经放弃的语言。
      if (localeSaveSeqRef.current !== requestSeq) return null;
      return selectWebuiLocale(locale);
    }).then((snapshot) => {
      if (snapshot && localeSaveSeqRef.current === requestSeq) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    }).catch((error) => {
      if (localeSaveSeqRef.current === requestSeq) {
        void i18n.setLocale(previousLocale).catch((rollbackError: unknown) => {
          console.error("Could not restore the previous locale after a failed save.", rollbackError);
        });
        toast.error(formatWorkbenchTranslation(i18n.t, "global.languageSaveFailed", {
          error: error instanceof Error ? error.message : String(error),
        }));
      }
    });
  }, [i18n, mergeWorkbenchWithLocalCaches]);
  // shellChrome owns app-wide theme, window controls, and titlebar gesture handlers.
  // shellChrome 持有应用级主题、窗口控制与标题栏手势处理器。
  const {
    shellChromeActions,
    showWindowControls,
    windowDragRegionEnabled,
    handleTitlebarDoubleClick,
  } = useWorkbenchShellChrome({
    startup: props.startup,
    locale: i18n.locale,
    onChangeLocale: handleWebuiLocaleChange,
  });
  // applySkillWorkbenchAction runs one skill-page service action and merges the returned snapshot.
  // applySkillWorkbenchAction 执行单个技能页面服务动作并合并返回快照。
  const applySkillWorkbenchAction = React.useCallback(async (action: () => Promise<WorkbenchSnapshot>) => {
    const snapshot = await action();
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
  }, [mergeWorkbenchWithLocalCaches]);
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
  // removeProjectTarget stores the project waiting for removal confirmation.
  // removeProjectTarget 保存正在等待移除确认的项目。
  const [removeProjectTarget, setRemoveProjectTarget] = React.useState<ProjectSummary | null>(null);
  // activeSettingsSubArea stores the selected settings sidebar page.
  // activeSettingsSubArea 保存设置侧栏当前选择的页面。
  const [activeSettingsSubArea, setActiveSettingsSubArea] = React.useState<SettingsSubAreaId>("model-providers");
  // activeFeature stores the selected feature summary from the complete top navigation list.
  // activeFeature 从完整顶部导航列表中保存当前选中的功能区域摘要。
  const activeFeature = localizedTopFeatureAreas.find((area) => area.id === activeArea) ?? localizedTopFeatureAreas[0];
  // Resolve the bundled statistics mount against the live catalog before offering navigation.
  // 提供导航前，根据实时目录解析内置统计插件的精确挂载点。
  const usageStatisticsMount = findEnabledPluginDetailMount(workbenchState.plugins, "plugin:vulcan-usage-statistics:usage-statistics");
  // handleSelectTopFeatureArea routes top navigation away from the embedded skills view.
  // handleSelectTopFeatureArea 将顶部导航路由出内嵌技能视图。
  const handleSelectTopFeatureArea = React.useCallback((area: MainAreaId) => {
    if (area === "agent-skills") {
      setActiveArea("chat");
      setWorkspaceMainView("agent-skills");
      return;
    }
    if (area === "skills") {
      setActiveArea("chat");
      setWorkspaceMainView("skills");
      return;
    }
    if (area === "agents") {
      setActiveArea("chat");
      setWorkspaceMainView("agents");
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
  // handleOpenWorkspaceKnowledge opens knowledge library management under plugin management in the workbench main panel.
  // handleOpenWorkspaceKnowledge 在工作台主面板的插件管理下打开知识库管理。
  const handleOpenWorkspaceKnowledge = React.useCallback(() => {
    setActiveArea("chat");
    setWorkspaceMainView("skills");
    setActiveWorkbenchSkillSubArea("knowledge");
  }, []);
  // handleOpenRuntimeDiagnostics leaves plugin management for the application-wide runtime diagnostics settings page.
  // handleOpenRuntimeDiagnostics 从插件管理跳转到应用全局运行诊断设置页。
  const handleOpenRuntimeDiagnostics = React.useCallback(() => {
    setActiveSettingsSubArea("runtime-diagnostics");
    handleSelectTopFeatureArea("settings");
  }, [handleSelectTopFeatureArea]);
  // handleOpenPermissionJudgeSettings opens the application-wide permission judge settings page.
  // handleOpenPermissionJudgeSettings 打开应用全局的权限判定设置页。
  const handleOpenPermissionJudgeSettings = React.useCallback(() => {
    setActiveSettingsSubArea("permission-judge");
    handleSelectTopFeatureArea("settings");
  }, [handleSelectTopFeatureArea]);
  const setupGuide = useSetupGuide(workbenchState.permissionJudgeState, props.startup.ready);
  const permissionJudgeEntry = React.useMemo<PermissionJudgeEntry>(() => ({
    state: workbenchState.permissionJudgeState ?? null,
    openSettings: handleOpenPermissionJudgeSettings,
    openGuide: setupGuide.show,
  }), [handleOpenPermissionJudgeSettings, setupGuide.show, workbenchState.permissionJudgeState]);
  // handleOpenWorkspaceChat returns the workbench main panel to the conversation surface.
  // handleOpenWorkspaceChat 将工作台主面板切回会话界面。
  const handleOpenWorkspaceChat = React.useCallback(() => {
    setActiveArea("chat");
    setWorkspaceMainView("chat");
  }, []);
  // conversationSelection orders sidebar, notification and diagnostic-owner selections and their visual transition.
  // conversationSelection 管理侧栏、通知与诊断所有者选择的先后顺序及其视觉过渡。
  const {
    conversationNavigation,
    handleConversationNavigationReady,
    handleSelectConversation,
    handleSelectPluginResourceOwner,
  } = useConversationSelectionNavigation({
    selectedConversationId,
    selectedConversationIdRef,
    setSelectedConversationId,
    setWorkbenchState,
    workbenchStateRef,
    mergeWorkbenchWithLocalCaches,
    acceptServiceWorkbenchSnapshot,
    liveStreamStore,
    handleOpenWorkspaceChat,
    isConversationInitialized: props.isConversationInitialized,
    activeArea,
    workspaceMainView,
    activeSkillSubArea: activeWorkbenchSkillSubArea,
  });
  // selectedConversation stores the conversation used by the console header.
  // selectedConversation 保存控制台标题栏使用的当前会话。
  const selectedConversation = workbenchState.conversations.find((conversation) => conversation.id === selectedConversationId);
  // selectedDraftConversation stores the draft used by the console header and empty state.
  // selectedDraftConversation 保存控制台标题栏与空状态使用的草稿会话。
  const selectedDraftConversation = selectedConversationId === draftConversation?.id ? draftConversation : null;
  // actionDraftConversation ends draft-only commands as soon as the durable conversation summary exists.
  // actionDraftConversation 在持久会话摘要出现后立即结束仅草稿命令。
  const actionDraftConversation = selectedConversation ? null : selectedDraftConversation;
  // handleChangeDraftProject moves an unsent draft to another project chosen from the composer chip.
  // handleChangeDraftProject 将未发送草稿移动到输入区标签选择的另一个项目。
  const handleChangeDraftProject = React.useCallback((projectId: string) => {
    const project = workbenchStateRef.current.projects.find((candidate) => candidate.id === projectId);
    if (!project) return;
    setDraftConversation((current) => current && !current.pendingSubmission
      ? { ...current, projectId: project.id, projectName: project.name }
      : current);
    setActiveProjectId(project.id);
  }, []);
  // draftComposerContext lets the composer pick the draft's project and session kind without prop threading.
  // draftComposerContext 让输入区无需逐层传参即可选择草稿的项目与会话种类。
  const draftComposerContext = React.useMemo<DraftComposerContextValue | null>(() => actionDraftConversation ? {
    draftId: actionDraftConversation.id,
    projectId: actionDraftConversation.projectId,
    projects: workbenchState.projects.map((project) => ({ id: project.id, name: project.name })),
    submissionPending: Boolean(actionDraftConversation.pendingSubmission),
    onChangeProject: handleChangeDraftProject,
  } : null, [actionDraftConversation, handleChangeDraftProject, workbenchState.projects]);
  // selectedRouteProjectId changes only when the user selects a conversation or draft in another project.
  // selectedRouteProjectId 仅在用户选择另一个项目的会话或草稿时变化。
  const selectedRouteProjectId = selectedDraftConversation?.projectId ?? selectedConversation?.projectId ?? null;
  // Semantic controls follow the conversation owner, not an independently selected terminal project.
  // 语义控件跟随会话归属，而非独立选择的终端项目。
  const selectedVceProject = workbenchState.projects.find((project) => project.id === selectedRouteProjectId) ?? null;
  // activeProject resolves the explicit owner used by project-level floating tools.
  // activeProject 解析项目级浮动工具使用的明确归属项目。
  const activeProject = workbenchState.projects.find((project) => project.id === activeProjectId) ?? null;
  // handleToggleActiveProjectTerminal switches the current project's terminal between visible and minimized states.
  // handleToggleActiveProjectTerminal 在当前项目终端的显示与最小化状态之间切换。
  const handleToggleActiveProjectTerminal = React.useCallback(() => {
    if (!activeProject) return;
    setProjectTerminalOpenRequest((request) => ({
      projectId: activeProject.id,
      action: "toggle",
      nonce: (request?.nonce ?? 0) + 1,
    }));
  }, [activeProject]);
  // consoleWorkbenchState reprojects selected-session fields from the UI-local selection.
  // consoleWorkbenchState 根据 UI 本地选择重新投影当前会话字段。
  const consoleWorkbenchState = React.useMemo<WorkbenchSnapshot>(() => {
    const projectedConversationId = selectedDraftConversation && !selectedConversation ? null : selectedConversationId;
    return workbenchWithSelectedConversationProjection(workbenchState, projectedConversationId);
  }, [selectedConversation, selectedConversationId, selectedDraftConversation, workbenchState]);
  // Align ownership before paint so a cached route switch cannot flash the previous owner's loading placeholder.
  // 在绘制前对齐归属，使缓存路由切换不会闪现旧归属对应的加载占位。
  React.useLayoutEffect(() => {
    props.onSelectedConversationIdChanged(
      selectedDraftConversation && !selectedConversation ? null : selectedConversationId,
    );
  }, [props.onSelectedConversationIdChanged, selectedConversation, selectedConversationId, selectedDraftConversation]);
  // showStartupOverlay blocks workspace interaction until the real service bootstrap completes.
  // showStartupOverlay 在真实服务启动完成前阻塞工作区交互。
  const showStartupOverlay = !props.startup.ready && props.startup.phase !== "browser-preview";

  // Keeps the async selection guard aligned with the latest sidebar choice.
  // 保持异步选中保护与最新侧栏选择一致。
  React.useEffect(() => {
    selectedConversationIdRef.current = selectedConversationId;
  }, [selectedConversationId]);
  // Follow explicit conversation navigation across projects while preserving project selection across same-project sessions.
  // 跟随跨项目的明确会话导航，同时在同项目会话切换时保留项目选择。
  React.useEffect(() => {
    if (selectedRouteProjectId) setActiveProjectId(selectedRouteProjectId);
  }, [selectedConversationId, selectedRouteProjectId]);
  // Reconcile a removed active project against the current route or the first remaining project.
  // 在活动项目被移除后，根据当前路由或首个剩余项目进行协调。
  React.useEffect(() => {
    if (activeProjectId && workbenchState.projects.some((project) => project.id === activeProjectId)) return;
    const routeProjectId = selectedRouteProjectId
      && workbenchState.projects.some((project) => project.id === selectedRouteProjectId)
      ? selectedRouteProjectId
      : null;
    setActiveProjectId(routeProjectId ?? workbenchState.projects[0]?.id ?? null);
  }, [activeProjectId, selectedRouteProjectId, workbenchState.projects]);
  // Keep the normalized workbench projection aligned with the locally selected chat.
  // 保持归一化工作台投影与本地选中的聊天会话一致。
  React.useEffect(() => {
    if (!selectedConversationId || draftConversation?.id === selectedConversationId) {
      return;
    }
    setWorkbenchState((current) => markConversationLocallyRead(current, selectedConversationId));
  }, [draftConversation?.id, selectedConversationId]);
  // showConsoleSidebar limits project and conversation context to the console view.
  // showConsoleSidebar 将项目与会话上下文限定在控制台页面内。
  const showConsoleSidebar = activeArea === "chat";

  const {
    applyProjectActionSnapshot,
    handleProjectMenuAction,
    handleProjectSettingsVmmSave,
    handleProjectSettingsAgentSkillSave,
    luaSkillActions: luaSkillCatalogActions,
    agentSkillActions,
    agentActions,
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
    acceptServiceWorkbenchSnapshot,
    applyLocalPreviewWithLocalCaches,
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
    applyLuaSkillsCatalogUpdate,
    applyAgentSkillsCatalogUpdate,
    applySubagentCatalogUpdate,
    handleOpenWorkspaceChat,
  });
  // luaSkillActions adds the MCP server page hosted by plugin management to the catalog lifecycle actions.
  // luaSkillActions 在目录生命周期动作之外加入插件管理承载的 MCP 服务器页面动作。
  const luaSkillActions = React.useMemo<SkillActionProps>(() => ({
    ...luaSkillCatalogActions,
    onSaveMcpServer: (request) => applySkillWorkbenchAction(async () => (await saveMcpServerConfiguration(request as unknown as Record<string, unknown>)) ?? workbenchStateRef.current),
    onRemoveMcpServer: (serverId) => applySkillWorkbenchAction(async () => (await removeMcpServerConfiguration(serverId)) ?? workbenchStateRef.current),
  }), [applySkillWorkbenchAction, luaSkillCatalogActions]);
  // handleWorkbenchProjectMenuAction owns the UI-only terminal entry before delegating persisted actions.
  // handleWorkbenchProjectMenuAction 在委托持久化动作前处理仅属于界面的终端入口。
  const handleWorkbenchProjectMenuAction = React.useCallback((
    action: Parameters<typeof handleProjectMenuAction>[0],
    project: Parameters<typeof handleProjectMenuAction>[1],
  ) => {
    if (action === "open-terminal") {
      setActiveProjectId(project.id);
      setProjectTerminalOpenRequest((request) => ({
        projectId: project.id,
        action: "open",
        nonce: (request?.nonce ?? 0) + 1,
      }));
      return;
    }
    handleProjectMenuAction(action, project);
  }, [handleProjectMenuAction]);
  // handleCreateConversationForProject keeps project-level tools bound to the newly selected draft owner.
  // handleCreateConversationForProject 使项目级工具绑定到新选择草稿的归属项目。
  const handleCreateConversationForProject = React.useCallback((project: ProjectSummary) => {
    setActiveProjectId(project.id);
    handleCreateConversation(project);
  }, [handleCreateConversation]);

  const {
    handleSaveSessionGoal,
    handleResumeSessionGoal,
    handleClearSessionGoal,
    handleUpdateSessionGoalStatus,
    handleSetSessionPlanMode,
    handleSetSessionVmmPolicy,
    handleSetSessionOutputStyle,
    handleSetSessionContextCompactionWindow,
    handleEditPendingPrompt,
  } = useWorkbenchSessionActions({
    selectedConversation,
    selectedDraftConversation: actionDraftConversation,
    setWorkbenchState,
    mergeWorkbenchWithLocalCaches,
  });

  const {
    handleSubmitConversationPrompt,
    handleInsertPendingPrompt,
    handleStopConversationRun,
    handleContinueConversation,
    handleEditConversationGeneration,
    handleForkConversationGeneration,
    handleCancelConversationSubmission,
    handleCompactSelectedConversationContext,
    handleOpenSubagentPanel,
    handleStopSubagentTask,
    handleCloseSubagentTask,
    handleRemovePendingPrompt,
    handleReorderPendingPrompts,
    handleSelectWorkbenchModel,
    handleStopCommandSession,
    handleListWorkbenchModelSecondaryRoutes,
    handleSelectWorkbenchModelSecondaryRoute,
    handleSelectWorkbenchReasoningEffort,
    handleSelectWorkbenchThinkingDisplayMode,
    handleSaveToolCollapseDefault,
  } = useWorkbenchConversationActions({
    translate: i18n.t,
    selectedConversation,
    selectedConversationId,
    selectedDraftConversation: actionDraftConversation,
    setWorkbenchState,
    mergeWorkbenchWithLocalCaches,
    setDraftConversation,
    setSelectedConversationId,
    selectedConversationIdRef,
    pendingMaterializedConversationIdRef,
    workbenchStateRef,
    requestStopConversationRunChoice,
  });

  // conversationMenu executes conversation-tree context menu commands and keeps the local pin set.
  // conversationMenu 执行会话树右键菜单命令并保存本地置顶集合。
  const { pinnedConversationIds, handleConversationMenuAction } = useConversationMenuActions({
    translate: i18n.t,
    workbenchState,
    setRenameTarget,
    applyProjectActionSnapshot,
    applyLifecycleResponseSnapshot,
  });

  // railEntries lists built-in areas first, then every enabled plugin detail page from the live catalog.
  // railEntries 先列出内置区域，再列出实时目录中每个已启用的插件详情页。
  const railEntries = React.useMemo<ActivityRailEntry[]>(() => {
    const builtIns = WORKSPACE_FEATURE_AREAS.map((area): ActivityRailEntry => ({
      id: area.id,
      label: i18n.t(area.label),
      icon: areaIconMap[area.id],
      active: activeArea === "chat" && workspaceMainView === area.id,
      onSelect: () => {
        // Plugin detail pages own their rail entries, so the plugin entry itself opens plugin management.
        // 插件详情页拥有各自的轨道入口，因此插件入口本身打开插件管理。
        if (area.id === "skills" && isPluginDetailSubArea(activeWorkbenchSkillSubArea)) {
          setActiveWorkbenchSkillSubArea("catalog");
        }
        handleSelectTopFeatureArea(area.id);
      },
    }));
    const pluginPages = enabledPluginDetailMounts(workbenchState.plugins).map((selection): ActivityRailEntry => ({
      id: selection.subAreaId,
      label: localizedPluginMountLabel(selection, i18n.locale),
      icon: pluginDetailMountIcon(selection.mount.icon),
      active: activeArea === "chat" && workspaceMainView === "skills" && activeWorkbenchSkillSubArea === selection.subAreaId,
      parentId: "skills",
      onSelect: () => {
        setActiveWorkbenchSkillSubArea(selection.subAreaId);
        handleSelectTopFeatureArea("skills");
      },
    }));
    return [...builtIns, ...pluginPages];
  }, [activeArea, activeWorkbenchSkillSubArea, handleSelectTopFeatureArea, i18n, workbenchState.plugins, workspaceMainView]);
  // activityRail is shared by the workspace and settings shells so navigation stays put across areas.
  // activityRail 由工作台与设置外壳共用，使导航在区域切换时保持不动。
  const activityRail = (
    <ActivityRail
      entries={railEntries}
      sessionsActive={activeArea === "chat" && workspaceMainView === "chat"}
      settingsActive={activeArea === "settings"}
      onSelectSessions={handleOpenWorkspaceChat}
      onSelectSettings={() => handleSelectTopFeatureArea("settings")}
      // The flyout reuses the session panel's tree and persisted expansion so both surfaces stay identical.
      // 浮层复用会话面板的会话树与持久化展开状态，使两处界面保持一致。
      sessionTree={(
        <ProjectSessionTree
          activeProjectId={activeProjectId}
          conversations={consoleWorkbenchState.conversations}
          expandedProjectIds={consoleWorkbenchState.expandedProjectIds}
          onConversationMenuAction={handleConversationMenuAction}
          onCreateConversation={handleCreateConversationForProject}
          onCreateProject={handleCreateProject}
          onExpandedProjectIdsChange={handleExpandedProjectIdsChange}
          onProjectMenuAction={handleWorkbenchProjectMenuAction}
          onReorderProjects={handleReorderProjects}
          onSelectConversation={handleSelectConversation}
          onSelectProject={setActiveProjectId}
          pinnedConversationIds={pinnedConversationIds}
          projects={consoleWorkbenchState.projects}
          selectedConversationId={selectedConversationId}
        />
      )}
      selectedConversationId={selectedConversationId}
      titlebarDragRegion={windowDragRegionEnabled}
      onTitlebarDoubleClick={handleTitlebarDoubleClick}
    />
  );

  React.useEffect(() => {
    notificationCoordinator.setRouteHandler((route) => {
      handleSelectConversation(route.conversationId);
    });
    return () => {
      notificationCoordinator.setRouteHandler(null);
    };
  }, [handleSelectConversation]);

  // handleRenameConfirm commits the current rename dialog value.
  // handleRenameConfirm 提交当前重命名对话框值。
  const handleRenameConfirm = React.useCallback(async (target: RenameTarget, name: string) => {
    const normalizedName = name.trim();
    if (!normalizedName && target.kind !== "project") {
      throw new Error(i18n.t("workbenchConversation.nameEmpty"));
    }
    const project = target.kind === "project"
      ? workbenchState.projects.find((candidate) => candidate.id === target.id) ?? null
      : null;
    const effectiveProjectName = normalizedName || restoreProjectDisplayName(project, target);
    try {
      const snapshot = target.kind === "project"
        ? await renameProject(target.id, effectiveProjectName)
        : await renameConversation(target.id, normalizedName);
      // A rename must return its committed snapshot; missing service data is never a successful preview write.
      // 重命名必须返回已提交快照；缺失服务数据绝不视为成功的预览写入。
      if (!snapshot) throw new Error("Rename did not return a committed service snapshot.");
      applyProjectActionSnapshot(snapshot);
      setRenameTarget(null);
      toast.success(target.kind === "project"
        ? normalizedName
          ? formatWorkbenchTranslation(i18n.t, "workbenchConversation.projectRenamed", { project: effectiveProjectName })
          : formatWorkbenchTranslation(i18n.t, "workbenchConversation.projectNameRestored", { project: effectiveProjectName })
        : formatWorkbenchTranslation(i18n.t, "workbenchConversation.conversationRenamed", { conversation: normalizedName }));
    } catch (nextError) {
      // Let the dialog keep the edited name and display the failure beside its action.
      // 由弹窗保留编辑名称，并在操作旁展示失败。
      throw nextError;
    }
  }, [applyProjectActionSnapshot, i18n.t, workbenchState]);

  // projectLifecycle confirms project archive and removal dialogs exactly once per request.
  // projectLifecycle 对项目归档与移除弹窗的每次请求只确认一次。
  const {
    archiveProjectSubmitting,
    removeProjectSubmitting,
    handleArchiveProjectConfirm,
    handleRemoveProjectConfirm,
    handleCancelArchiveProject,
    handleCancelRemoveProject,
  } = useProjectLifecycleConfirmation({
    translate: i18n.t,
    workbenchState,
    archiveProjectTarget,
    setArchiveProjectTarget,
    removeProjectTarget,
    setRemoveProjectTarget,
    applyProjectActionSnapshot,
    applyLifecycleResponseSnapshot,
  });

  // handlePendingQuestionSubmit resumes a blocked ask tool call with validated modal answers.
  // handlePendingQuestionSubmit 使用弹窗校验后的答案恢复被阻塞的 ask 工具调用。
  const handlePendingQuestionSubmit = React.useCallback(async (requestId: string, answers: string[][]) => {
    try {
      await replyPendingQuestion(requestId, answers);
      toast.success(i18n.t("workbenchAsk.submitted"));
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      throw nextError;
    }
  }, [i18n.t]);

  // handlePendingQuestionReject cancels a blocked ask tool call after a second confirmation.
  // handlePendingQuestionReject 在二次确认后取消被阻塞的 ask 工具调用。
  const handlePendingQuestionReject = React.useCallback(async (requestId: string) => {
    try {
      await rejectPendingQuestion(requestId);
      toast.info(i18n.t("workbenchAsk.cancelled"));
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      throw nextError;
    }
  }, [i18n.t]);

  React.useEffect(() => {
    draftConversationIdRef.current = draftConversation?.id ?? null;
  }, [draftConversation?.id]);

  // Route an empty active session collection to a draft owned by the last active project.
  // 将空活动会话集合路由到最后活动项目拥有的草稿。
  React.useEffect(() => {
    // emptyWorkbenchRoute keeps the local draft and selected surface on one exact project-owned identity.
    // emptyWorkbenchRoute 使本地草稿与选中界面保持为同一个项目所属身份。
    const emptyWorkbenchRoute = resolveEmptyWorkbenchDraftRoute(
      workbenchState,
      activeProjectId,
      draftConversation,
    );
    if (!emptyWorkbenchRoute) {
      return;
    }
    if (draftConversation !== emptyWorkbenchRoute.draftConversation) {
      setDraftConversation(emptyWorkbenchRoute.draftConversation);
    }
    if (selectedConversationId !== emptyWorkbenchRoute.selectedConversationId) {
      setSelectedConversationId(emptyWorkbenchRoute.selectedConversationId);
    }
  }, [activeProjectId, draftConversation, selectedConversationId, workbenchState.conversations, workbenchState.projects]);

  // Service snapshots and delta batches pass through the revision gates in this exact effect order.
  // 服务快照与增量批次按此精确 effect 顺序经过版本闸门。
  useWorkbenchServiceSync(revisionGates, {
    serviceInstanceEpoch: props.serviceInstanceEpoch,
    workbench: props.workbench,
    workbenchDelta: props.workbenchDelta,
    onWorkbenchDeltaConsumed: props.onWorkbenchDeltaConsumed,
    setWorkbenchState,
    setSelectedConversationId,
    setMediaUploadProgresses,
    draftConversationIdRef,
    pendingMaterializedConversationIdRef,
    expandedProjectIdsLocallyEditedRef,
    t: i18n.t,
  });

  // Release the selection guard when the row arrives; keep the optimistic prompt until canonical correlation.
  // 侧栏行到达时释放选择保护；乐观提示词保留到规范关联完成。
  React.useEffect(() => {
    const pendingConversationId = pendingMaterializedConversationIdRef.current;
    if (!pendingConversationId) {
      return;
    }
    if (workbenchState.conversations.some((conversation) => conversation.id === pendingConversationId)) {
      pendingMaterializedConversationIdRef.current = null;
    }
  }, [workbenchState.conversations]);

  // handleFirstSubmissionReconciled removes only the optimistic prompt replaced by the exact V1 input.
  // handleFirstSubmissionReconciled 仅移除被精确 V1 输入替换的乐观提示词。
  const handleFirstSubmissionReconciled = React.useCallback((conversationId: string, clientSubmissionId: string) => {
    setDraftConversation((current) => retireMaterializedDraftSubmission(current, conversationId, clientSubmissionId));
  }, []);


  // Keep local sidebar selection inside the active conversation lifecycle domain after deltas apply.
  // 在增量应用后，保持本地侧栏选择位于活动会话生命周期领域内。
  React.useEffect(() => {
    setSelectedConversationId((current) => {
      if (current && current === draftConversationIdRef.current) {
        return current;
      }
      if (current && current === pendingMaterializedConversationIdRef.current) {
        return current;
      }
      if (current && workbenchState.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      // nextSelectedConversationId uses the backend lifecycle choice only while it remains active.
      // nextSelectedConversationId 仅在后端生命周期选择仍处于活动状态时使用它。
      const nextSelectedConversationId = workbenchState.selectedConversationId;
      if (nextSelectedConversationId
        && workbenchState.conversations.some((conversation) => conversation.id === nextSelectedConversationId)) {
        return nextSelectedConversationId;
      }
      return null;
    });
  }, [workbenchState.conversations, workbenchState.selectedConversationId]);

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

  // dialogsRequested identifies a visible dialog without loading dialog code for the common workbench path.
  // dialogsRequested 识别可见对话框，避免常规工作台路径加载对话框代码。
  const dialogsRequested = projectFolderPickerOpen || renameTarget !== null || projectIconTarget !== null
    || projectSettingsTarget !== null || archiveProjectTarget !== null || removeProjectTarget !== null
    || Boolean(workbenchState.pendingQuestion) || stopConversationAgents !== null;
  // dialogsActivated keeps the dialog host mounted after its first use so close animations can finish.
  // dialogsActivated 在首次使用后保持对话框宿主挂载，以便关闭动画完成。
  const [dialogsActivated, setDialogsActivated] = React.useState(dialogsRequested);
  React.useEffect(() => {
    if (dialogsRequested) {
      setDialogsActivated(true);
    }
  }, [dialogsRequested]);

  return (
    <ThinkingDisplaySystemContext.Provider value={{
      mode: workbenchState.thinkingDisplayMode,
      save: handleSelectWorkbenchThinkingDisplayMode,
    }}>
    <ToolCollapsePreferencesContext.Provider value={{
      defaultEnabled: workbenchState.toolCollapseDefaultEnabled ?? true,
      activeConversationId: selectedConversationId,
      saveDefault: handleSaveToolCollapseDefault,
    }}>
    <ComposerSettingsHostContext.Provider value={settingsHost}>
    <DraftComposerContext.Provider value={draftComposerContext}>
    <ComposerMediaUploadProgressContext.Provider value={mediaUploadProgresses}>
      <VceJobObserverProvider serviceEpoch={props.serviceInstanceEpoch}>
      <VceProjectSurfaceProvider
      // Only the visible chat owns these observations; project settings subscribe independently.
      // 仅可见会话持有这些观察；项目设置自行独立订阅。
      projectId={showConsoleSidebar && workspaceMainView === "chat" ? selectedVceProject?.id ?? null : null}
      projectEnabled={selectedVceProject?.vceEnabled === true}
      revisionKey={`${props.serviceInstanceEpoch}:${globalProjectSettingsRevision.vce}:${selectedVceProject ? projectSettingsRevisionById[selectedVceProject.id]?.vce ?? 0 : 0}`}
      onOpenEngineSettings={() => { handleOpenWorkspaceSkills(); setActiveWorkbenchSkillSubArea("context-engine"); }}
      onOpenProjectSettings={() => { if (selectedVceProject) setProjectSettingsTarget({ project: selectedVceProject, initialSection: "code-context" }); }}
    >
    <div className={`app-shell ${showConsoleSidebar ? "with-console-sidebar" : "without-console-sidebar"} ${showWindowControls ? "tauri-shell" : "browser-shell"}`}>
      {floatingMediaUploadProgresses.length > 0 ? (
        <div className="media-upload-progress-stack">
          <React.Suspense fallback={<div role="status">{i18n.t("composer.attachment.processing")}</div>}>
          {floatingMediaUploadProgresses.map((progress) => (
            <MediaUploadProgressPanel
              key={mediaUploadProgressKey(progress)}
              progress={progress}
              onCancel={handleCancelConversationSubmission}
            />
          ))}
          </React.Suspense>
        </div>
      ) : null}
      <PermissionJudgeEntryContext.Provider value={permissionJudgeEntry}>
      <main className="workspace">
        {showConsoleSidebar ? (
          <WorkspaceShell
            activeProjectId={activeProjectId}
            projectTerminalButtonRef={projectTerminalButtonRef}
            projectTerminalRunningCount={activeProjectId
              ? projectTerminalRunningCounts[activeProjectId]?.runningCount ?? 0
              : 0}
            onToggleProjectTerminal={handleToggleActiveProjectTerminal}
            startup={props.startup}
            workbench={consoleWorkbenchState}
            activeSkillSubArea={activeWorkbenchSkillSubArea}
            activeMemorySubArea={activeWorkbenchMemorySubArea}
            mainView={workspaceMainView}
            sidebarOpen={sessionPanelOpen}
            onSidebarOpenChange={setSessionPanelOpen}
            rail={activityRail}
            selectedDraftConversation={selectedDraftConversation}
            selectedConversation={selectedConversation}
            selectedConversationId={selectedConversationId}
            conversationNavigation={conversationNavigation}
            onConversationNavigationReady={handleConversationNavigationReady}
            onFirstSubmissionReconciled={handleFirstSubmissionReconciled}
            onNotificationPresentationContextChange={handleNotificationPresentationContextChange}
            chromeActions={shellChromeActions}
            titlebarDragRegion={windowDragRegionEnabled}
            onTitlebarDoubleClick={handleTitlebarDoubleClick}
            pinnedConversationIds={pinnedConversationIds}
            onConversationMenuAction={handleConversationMenuAction}
            onCreateConversation={handleCreateConversationForProject}
            onCreateProject={handleCreateProject}
            onOpenCredentialManagement={() => {
              setActiveSettingsSubArea("credential-management")
              handleSelectTopFeatureArea("settings")
            }}
            onOpenMemory={handleOpenWorkspaceMemory}
            onOpenKnowledge={handleOpenWorkspaceKnowledge}
            onOpenRuntimeDiagnostics={handleOpenRuntimeDiagnostics}
            onSelectSkillSubArea={setActiveWorkbenchSkillSubArea}
            onSelectMemorySubArea={setActiveWorkbenchMemorySubArea}
            onExpandedProjectIdsChange={handleExpandedProjectIdsChange}
            onProjectMenuAction={handleWorkbenchProjectMenuAction}
            onReorderProjects={handleReorderProjects}
            onSelectProject={setActiveProjectId}
            onSelectConversation={handleSelectConversation}
            onSelectPluginResourceOwner={handleSelectPluginResourceOwner}
            onSelectWorkbenchModel={handleSelectWorkbenchModel}
            onListWorkbenchModelSecondaryRoutes={handleListWorkbenchModelSecondaryRoutes}
            onSelectWorkbenchModelSecondaryRoute={handleSelectWorkbenchModelSecondaryRoute}
            onSelectWorkbenchReasoningEffort={handleSelectWorkbenchReasoningEffort}
            luaSkillActions={luaSkillActions}
            agentSkillActions={agentSkillActions}
            agentActions={agentActions}
            memoryActions={{
              onLoadProjectVmmConfig: loadProjectVmmConfig,
              onRunVmmAction: runVmmMemoryAction,
              onSaveVmmConfig: (request) => applySkillWorkbenchAction(() => saveVmmMemoryConfig(request)),
            }}
            onSlashCommand={handleComposerSlashCommand}
            onSubmitPrompt={handleSubmitConversationPrompt}
            onSaveSessionGoal={handleSaveSessionGoal}
            onResumeSessionGoal={handleResumeSessionGoal}
            onClearSessionGoal={handleClearSessionGoal}
            onUpdateSessionGoalStatus={handleUpdateSessionGoalStatus}
            onSetSessionPlanMode={handleSetSessionPlanMode}
            onSetSessionVmmPolicy={handleSetSessionVmmPolicy}
            onSetSessionOutputStyle={handleSetSessionOutputStyle}
            onSetSessionContextCompactionWindow={handleSetSessionContextCompactionWindow}
            onStopConversationRun={handleStopConversationRun}
            onContinueConversation={handleContinueConversation}
            onEditConversationGeneration={handleEditConversationGeneration}
            onForkConversationGeneration={handleForkConversationGeneration}
            onOpenSubagentPanel={handleOpenSubagentPanel}
            onStopSubagentTask={handleStopSubagentTask}
            onCloseSubagentTask={handleCloseSubagentTask}
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
            resolveMediaSourceUrl={localMediaSourceUrl}
            footer={(
              <footer className="statusbar workspace-statusbar">
                <div className="workspace-statusbar-leading">
                  <div className="workspace-statusbar-settings" ref={setSettingsHost} />
                  <div className="workspace-statusbar-todolist-status" ref={setTodolistStatusHost} />
                </div>
                <div className="workspace-statusbar-runtime-status" ref={setRuntimeStatusHost} />
                <div className="workspace-statusbar-trailing">
                  <RepositoryLink onOpenExternalUrl={openExternalUrl} />
                  <span className="workspace-statusbar-version">v{props.workbench.appVersion}</span>
                </div>
              </footer>
            )}
            runtimeStatusHost={runtimeStatusHost}
            todolistStatusHost={todolistStatusHost}
            onTrace={webuiClickTrace}
          />
        ) : (
          <ExtractedFeatureShell
            onOpenUsageStatistics={usageStatisticsMount ? () => {
              setActiveWorkbenchSkillSubArea(usageStatisticsMount.subAreaId);
              handleSelectTopFeatureArea("skills");
            } : undefined}
            activeArea={activeArea}
            activeFeature={activeFeature}
            rail={activityRail}
            activeSettingsSubArea={activeSettingsSubArea}
            onSelectSettingsSubArea={setActiveSettingsSubArea}
            onSelectArea={handleSelectTopFeatureArea}
            luaSkillActions={luaSkillActions}
            agentSkillActions={agentSkillActions}
            agentActions={agentActions}
            memoryActions={{
              onLoadProjectVmmConfig: loadProjectVmmConfig,
              onRunVmmAction: runVmmMemoryAction,
              onSaveVmmConfig: (request) => applySkillWorkbenchAction(() => saveVmmMemoryConfig(request)),
            }}
            settingsActions={{
              onSavePromptOutputStyle: (styleId) => applySkillWorkbenchAction(async () => {
                const saved = await savePromptOutputStyle(styleId);
                if (saved) {
                  toast.info(i18n.t("workspace.lifecycle.outputStyleSaved"), {
                    id: "output-style-safe-boundary",
                  });
                }
                return (await loadWorkbenchSnapshot()) ?? workbenchStateRef.current;
              }),
              onSavePromptAnswerMode: (mode) => applySkillWorkbenchAction(async () => {
                await savePromptAnswerMode(mode);
                return (await loadWorkbenchSnapshot()) ?? workbenchStateRef.current;
              }),
              onSaveGitCommitSettings: (request) => applySkillWorkbenchAction(async () => (await saveGitCommitSettings(request)) ?? workbenchStateRef.current),
              onSaveProviderRequestTraceSettings: (enabled, errorsEnabled) => applySkillWorkbenchAction(async () => (await saveProviderRequestTraceSettings(enabled, errorsEnabled)) ?? workbenchStateRef.current),
              onOpenSelectedProviderApiTraceDirectory: async () => {
                await openSelectedProviderApiTraceDirectory();
              },
              onClearSelectedProviderApiTraceDirectory: clearSelectedProviderApiTraceDirectory,
              onSaveProviderRetrySettings: (request) => applySkillWorkbenchAction(async () => (await saveProviderRetrySettings(request)) ?? workbenchStateRef.current),
              onRestoreArchivedConversations: (conversationIds) => applySkillWorkbenchAction(async () => (await restoreArchivedConversations(conversationIds)) ?? workbenchStateRef.current),
              onDeleteArchivedConversations: (conversationIds) => applySkillWorkbenchAction(async () => (await deleteArchivedConversations(conversationIds)) ?? workbenchStateRef.current),
            }}
            startup={props.startup}
            workbench={workbenchState}
            selectedConversationId={selectedConversationId}
            chromeActions={shellChromeActions}
            titlebarDragRegion={windowDragRegionEnabled}
            onTitlebarDoubleClick={handleTitlebarDoubleClick}
          />
        )}
        <ProjectFloatingTerminalLayer
          minimizeAnchorRef={projectTerminalButtonRef}
          openRequest={projectTerminalOpenRequest}
          project={activeProject}
          projects={workbenchState.projects}
        />
        {showStartupOverlay ? <StartupOverlay startup={props.startup} /> : null}
      </main>
      </PermissionJudgeEntryContext.Provider>
      {setupGuide.open ? <React.Suspense fallback={null}><SetupGuideDialog open onOpenChange={setupGuide.setOpen}
        onOpenProviderSettings={() => {
          setupGuide.setOpen(false);
          setActiveSettingsSubArea("model-providers");
          handleSelectTopFeatureArea("settings");
        }} /></React.Suspense> : null}
      {dialogsActivated ? <React.Suspense fallback={null}><WorkbenchDialogs
        archiveProjectSubmitting={archiveProjectSubmitting}
        archiveProjectTarget={archiveProjectTarget}
        pendingQuestion={workbenchState.pendingQuestion ?? null}
        projectFolderPickerOpen={projectFolderPickerOpen}
        projectIconTarget={projectIconTarget}
        projectSettingsRevision={projectSettingsTarget
          ? {
            vmm: Math.max(
              globalProjectSettingsRevision.vmm,
              projectSettingsRevisionById[projectSettingsTarget.project.id]?.vmm ?? 0,
            ),
            vce: Math.max(
              globalProjectSettingsRevision.vce,
              projectSettingsRevisionById[projectSettingsTarget.project.id]?.vce ?? 0,
            ),
            agentSkills: Math.max(
              globalProjectSettingsRevision.agentSkills,
              projectSettingsRevisionById[projectSettingsTarget.project.id]?.agentSkills ?? 0,
            ),
          }
          : { vmm: 0, vce: 0, agentSkills: 0 }}
        projectSettingsTarget={projectSettingsTarget}
        removeProjectSubmitting={removeProjectSubmitting}
        removeProjectTarget={removeProjectTarget}
        renameTarget={renameTarget}
        stopConversationAgents={stopConversationAgents}
        workbench={workbenchState}
        onBrowseProjectDirectories={browseProjectDirectories}
        onCancelArchiveProject={handleCancelArchiveProject}
        onCancelProjectIcon={() => setProjectIconTarget(null)}
        onCancelProjectSettings={() => setProjectSettingsTarget(null)}
        onOpenEngineSettings={() => {
          setProjectSettingsTarget(null);
          handleOpenWorkspaceSkills();
          setActiveWorkbenchSkillSubArea("context-engine");
        }}
        onCancelRemoveProject={handleCancelRemoveProject}
        onCancelRename={() => setRenameTarget(null)}
        onCancelStopConversationRun={() => resolveStopConversationRunChoice(null)}
        onConfirmArchiveProject={handleArchiveProjectConfirm}
        onConfirmProjectFolder={handleConfirmProjectFolder}
        onConfirmProjectIcon={handleProjectIconConfirm}
        onConfirmRemoveProject={handleRemoveProjectConfirm}
        onConfirmRename={handleRenameConfirm}
        onConfirmStopConversationRun={resolveStopConversationRunChoice}
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
      /></React.Suspense> : null}
    </div>
      </VceProjectSurfaceProvider>
      </VceJobObserverProvider>
    </ComposerMediaUploadProgressContext.Provider>
    </DraftComposerContext.Provider>
    </ComposerSettingsHostContext.Provider>
    </ToolCollapsePreferencesContext.Provider>
    </ThinkingDisplaySystemContext.Provider>
  );
}
