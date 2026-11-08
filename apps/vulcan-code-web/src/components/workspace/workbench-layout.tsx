import React from "react";
import { ComposerAccountHostContext, ComposerSettingsHostContext } from "@/components/composer/composer-settings-host";
import { DraftComposerContext, type DraftComposerContextValue } from "@/components/composer/dedicated/draft-composer-context";
import { ToolCollapsePreferencesContext } from "@/components/conversation/tool-collapse-preferences";
import { ThinkingDisplaySystemContext } from "@/components/conversation/thinking-display-preferences";
import type { SkillActionProps } from "@/components/features/skills/skill-area-types";
import {
  FeatureShell as ExtractedFeatureShell,
  areaIconMap,
  pluginDetailMountIcon,
  primaryFeatureAreas as buildPrimaryFeatureAreas,
} from "@/components/features/feature-shell";
import { Variable } from "lucide-react";
import { ActivityRail, type ActivityRailEntry } from "@/components/shell/activity-rail";
import { CommandPalette } from "@/components/shell/command-palette";
import { useNewConversationRequests } from "@/components/composer/composer-external-commands";
import { WorkbenchStatusBar } from "@/components/shell/workbench-status-bar";
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
import { NavigationPanelBody, SidebarFlowSwitch } from "@/components/project/project-sidebar";
import type { ProjectTerminalVisibilityRequest } from "@/components/project/terminal/project-floating-terminal-layer";
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
import { useSessionTitleToasts } from "@/components/workspace/workbench-layout/use-session-title-toasts";
import { useAppUpdatePrompts } from "@/components/workspace/workbench-layout/use-app-update-prompts";
import { useAppUpdate } from "@/components/features/app-update/use-app-update";
import { AppUpdateContext, type AppUpdateHost } from "@/components/features/app-update/app-update-context";
import { useProjectLifecycleConfirmation } from "@/components/workspace/workbench-layout/use-project-lifecycle-confirmation";
import { useConversationMenuActions } from "@/components/workspace/workbench-layout/use-conversation-menu-actions";
import { UnsupportedSessionDeleteDialog } from "@/components/project/unsupported-session-delete-dialog";
import { useConversationSelectionNavigation } from "@/components/workspace/workbench-layout/use-conversation-selection-navigation";
import { PermissionJudgeEntryContext, type PermissionJudgeEntry } from "@/components/composer/permission-judge-entry-context";
import { useSetupGuide } from "@/components/features/onboarding/use-setup-guide";
import { useOpenVulcanAccount } from "@/components/features/account/use-openvulcan-account";
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context";
import { useFeedbackCenter } from "@/components/features/feedback/use-feedback-center";
import { FeedbackCenterContext } from "@/components/features/feedback/feedback-center-context";
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
  ConversationSummary,
  MainAreaId,
  ProjectSummary,
  SettingsSubAreaId,
  StartupSnapshot,
  WorkbenchSnapshot,
  MediaUploadProgressChangedDelta,
  EnvironmentValuesSubmitRequest,
} from "@/types";
import {
  rejectPendingQuestion,
  replyPendingQuestion,
  submitEnvironmentValues,
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
  saveDefaultCommandShell,
  saveProviderRequestTraceSettings,
  saveProviderRetrySettings,
  saveMcpServerConfiguration,
  savePromptAnswerMode,
  saveSessionTitleGeneration,
  saveToolChainSettings,
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
  fallbackMainViewConversationId,
  markConversationLocallyRead,
  visibleConversationId,
  workbenchWithSelectedConversationProjection,
} from "@/state/workbench-state";
import { toast } from "@/ui-kit/primitives/toast";
import { AssistantDockContext } from "@/components/assistant/assistant-dock-context";
import { useAssistantDockController } from "@/components/assistant/use-assistant-dock-controller";

// AssistantFloatingLayer loads only once at least one session floats as an assistant.
// AssistantFloatingLayer 仅在至少一个会话作为助手浮动后加载。
const AssistantFloatingLayer = React.lazy(() => import("@/components/assistant/assistant-floating-layer"));
// ProjectFloatingTerminalLayer stays mounted but loads after the first screen; an open request made before it loads
// is applied when it mounts.
// ProjectFloatingTerminalLayer 始终挂载，但在首屏之后加载；加载前发出的打开请求在其挂载时生效。
const ProjectFloatingTerminalLayer = React.lazy(() => import("@/components/project/terminal/project-floating-terminal-layer")
  .then((module) => ({ default: module.ProjectFloatingTerminalLayer })));
// FileWindowLayer shows the file windows of the selected session.
// FileWindowLayer 显示所选会话的文件窗口。
const FileWindowLayer = React.lazy(() => import("@/components/file-windows/file-window-layer"));

// WorkbenchDialogs loads project and question dialogs only after one actually opens.
// WorkbenchDialogs 仅在项目或提问对话框真正打开后加载其界面。
const WorkbenchDialogs = React.lazy(() => import("@/components/workspace/workbench-dialogs")
  .then((module) => ({ default: module.WorkbenchDialogs })));
// LazyProjectHome loads the project home page and its scoped styles only when a project home opens.
// LazyProjectHome 仅在打开项目首页时加载该页面及其作用域样式。
const LazyProjectHome = React.lazy(() => import("@/components/project/project-home")
  .then((module) => ({ default: module.ProjectHome })));
const SetupGuideDialog = React.lazy(() => import("@/components/features/onboarding/setup-guide-dialog"));
// OpenVulcanAccountDialog loads only when onboarding or account management is opened.
// OpenVulcanAccountDialog 仅在打开引导或账户管理时加载。
const OpenVulcanAccountDialog = React.lazy(() => import("@/components/features/account/openvulcan-account-dialog"));
// The feedback window, message center and announcement banner load only when shown.
// 反馈窗口、消息中心与公告横幅仅在显示时加载。
const FeedbackSurfaces = React.lazy(() => import("@/components/features/feedback/feedback-surfaces"));

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
  // accountHost receives the active composer's model account item in the footer.
  // accountHost 在底栏中接收当前输入区的模型账号项。
  const [accountHost, setAccountHost] = React.useState<HTMLDivElement | null>(null);
  // todolistStatusHost stores the footer node immediately following the settings entry.
  // todolistStatusHost 保存紧跟设置入口的底栏 Todo 节点。
  const [todolistStatusHost, setTodolistStatusHost] = React.useState<HTMLDivElement | null>(null);
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
  // contextPanelOpen keeps the conversation's right panel across visits to settings, which unmount the workspace shell.
  // contextPanelOpen 在访问会卸载工作区外壳的设置页期间保留会话右侧面板状态。
  const [contextPanelOpen, setContextPanelOpen] = React.useState(false);
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
    fallbackMainViewConversationId(props.workbench.conversations, props.workbench.selectedConversationId)
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
  // projectTerminalExpanded mirrors whether the active project's floating terminal is expanded.
  // projectTerminalExpanded 镜像当前项目的浮动终端是否展开。
  const [projectTerminalExpanded, setProjectTerminalExpanded] = React.useState(false);
  // projectTerminalButtonRef exposes the status-bar terminal item the window minimizes into.
  // projectTerminalButtonRef 暴露终端窗口最小化时收入的状态栏终端项。
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
  useSessionTitleToasts(props.serviceInstanceEpoch);
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
    nativeWindowButtons,
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
  // projectHome names the project whose home page covers the chat stage and the conversation selection it opened over;
  // any later selection change closes it.
  // projectHome 记录其首页覆盖聊天舞台的项目及打开时的会话选择；之后任何选择变化都会关闭首页。
  const [projectHome, setProjectHome] = React.useState<{ projectId: string; selectionKey: string } | null>(null);
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
  // accountHostValue lets the composer's account item open the model services page.
  // accountHostValue 让输入区的账号项打开模型服务页面。
  const accountHostValue = React.useMemo(() => ({
    host: accountHost,
    onOpenSettings: () => {
      setActiveSettingsSubArea("model-providers");
      handleSelectTopFeatureArea("settings");
    },
  }), [accountHost, handleSelectTopFeatureArea]);
  // handleOpenAppUpdateSettings opens the update settings page from an update prompt.
  // handleOpenAppUpdateSettings 从更新提示打开更新设置页。
  const handleOpenAppUpdateSettings = React.useCallback(() => {
    setActiveSettingsSubArea("app-update");
    handleSelectTopFeatureArea("settings");
  }, [handleSelectTopFeatureArea]);
  // appUpdate is the one updater state shared by the update prompts and the sidebar update entry.
  // appUpdate 是更新提示与侧栏更新入口共用的唯一更新器状态。
  const appUpdate = useAppUpdate(props.serviceInstanceEpoch);
  useAppUpdatePrompts(appUpdate.snapshot, props.serviceInstanceEpoch, handleOpenAppUpdateSettings);
  const appUpdateHost = React.useMemo<AppUpdateHost>(() => ({
    snapshot: appUpdate.snapshot,
    setSnapshot: appUpdate.setSnapshot,
    openSettings: handleOpenAppUpdateSettings,
  }), [appUpdate.snapshot, appUpdate.setSnapshot, handleOpenAppUpdateSettings]);
  // handleOpenPermissionJudgeSettings opens the application-wide permission judge settings page.
  // handleOpenPermissionJudgeSettings 打开应用全局的权限判定设置页。
  const handleOpenPermissionJudgeSettings = React.useCallback(() => {
    setActiveSettingsSubArea("permission-judge");
    handleSelectTopFeatureArea("settings");
  }, [handleSelectTopFeatureArea]);
  // account owns installation-wide onboarding before the existing optional setup guide.
  // account 在既有可选初始化引导之前持有整个安装的账户引导。
  const accountController = useOpenVulcanAccount(props.startup.ready, props.serviceInstanceEpoch);
  // account adds the shell's way to the account settings page, closing the quick dialog first.
  // account 加入外壳打开账户设置页的方式，并先关闭快捷弹窗。
  const account = React.useMemo(() => ({
    ...accountController,
    openDetails: () => {
      if (accountController.open) accountController.setOpen(false);
      setActiveSettingsSubArea("openvulcan-account");
      handleSelectTopFeatureArea("settings");
    },
  }), [accountController, handleSelectTopFeatureArea]);
  // feedbackCenter opens the feedback window and the message center and reads their badge state.
  // feedbackCenter 打开反馈窗口与消息中心，并读取其徽标状态。
  const feedbackCenter = useFeedbackCenter(props.startup.ready, props.serviceInstanceEpoch, accountController.snapshot?.connectionId ?? null);
  const setupGuide = useSetupGuide(workbenchState.permissionJudgeBackend, props.startup.ready && account.initialized && !account.open);
  const permissionJudgeEntry = React.useMemo<PermissionJudgeEntry>(() => ({
    backend: workbenchState.permissionJudgeBackend ?? null,
    openSettings: handleOpenPermissionJudgeSettings,
    openGuide: setupGuide.show,
  }), [handleOpenPermissionJudgeSettings, setupGuide.show, workbenchState.permissionJudgeBackend]);
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
  // projectHomeSelectionKeyRef reads the current selection when a home opens, without re-creating the opener.
  // projectHomeSelectionKeyRef 在打开首页时读取当前选择，而无需重建打开函数。
  const projectHomeSelectionKeyRef = React.useRef(selectedConversationId ?? "");
  projectHomeSelectionKeyRef.current = selectedConversationId ?? "";
  // openProjectHome shows one project's home page in the chat stage, leaving any embedded page first.
  // openProjectHome 在聊天舞台显示单个项目的首页，必要时先离开内嵌页面。
  const openProjectHome = React.useCallback((project: ProjectSummary) => {
    handleOpenWorkspaceChat();
    setActiveProjectId(project.id);
    setProjectHome({ projectId: project.id, selectionKey: projectHomeSelectionKeyRef.current });
  }, [handleOpenWorkspaceChat]);
  // closeProjectHome returns the stage to the current conversation.
  // closeProjectHome 让舞台回到当前会话。
  const closeProjectHome = React.useCallback(() => setProjectHome(null), []);
  // projectHomeProject is the visible home's project: it exists, the chat stage is showing, and the selection is unchanged.
  // projectHomeProject 是可见首页的项目：项目存在、聊天舞台可见且选择未变化。
  const projectHomeProject = projectHome && activeArea === "chat" && workspaceMainView === "chat"
    && projectHome.selectionKey === (selectedConversationId ?? "")
    ? workbenchState.projects.find((project) => project.id === projectHome.projectId) ?? null
    : null;
  React.useEffect(() => {
    // A home hidden by navigation or project removal is closed, so returning later shows the conversation.
    // 因导航或项目移除而隐藏的首页会被关闭，之后返回时显示会话。
    if (projectHome && !projectHomeProject) setProjectHome(null);
  }, [projectHome, projectHomeProject]);
  // handleSelectConversationLeavingHome opens a conversation, closing the home even when it was already selected.
  // handleSelectConversationLeavingHome 打开会话；即使该会话已选中也会关闭首页。
  const handleSelectConversationLeavingHome = React.useCallback((conversationId: string) => {
    setProjectHome(null);
    handleSelectConversation(conversationId);
  }, [handleSelectConversation]);
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
  // statusBar is the one bottom strip both the workspace and the feature pages show.
  // statusBar 是工作台与功能页共用的底部状态条。
  const statusBar = (
    <WorkbenchStatusBar
      appVersion={props.workbench.appVersion}
      hosts={{ settings: setSettingsHost, todolist: setTodolistStatusHost, account: setAccountHost }}
      plugins={workbenchState.plugins}
      startup={props.startup}
      terminal={activeProject ? {
        expanded: projectTerminalExpanded,
        projectName: activeProject.name,
        runningCount: projectTerminalRunningCounts[activeProject.id]?.runningCount ?? 0,
      } : null}
      terminalAnchorRef={projectTerminalButtonRef}
      vmmMemory={workbenchState.vmmMemory}
      onOpenDiagnostics={handleOpenRuntimeDiagnostics}
      onOpenExternalUrl={openExternalUrl}
      onOpenMemory={handleOpenWorkspaceMemory}
      onOpenPlugins={handleOpenWorkspaceSkills}
      onToggleTerminal={handleToggleActiveProjectTerminal}
    />
  );
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
    handleProjectGroupAction,
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
    openProjectHome,
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
    setProjectHome(null);
    setActiveProjectId(project.id);
    handleCreateConversation(project);
  }, [handleCreateConversation]);
  // handleCreateDedicatedConversationForProject opens a draft preset to a dedicated assistant in one project.
  // handleCreateDedicatedConversationForProject 在单个项目中打开预设为专职助手的草稿。
  const handleCreateDedicatedConversationForProject = React.useCallback((project: ProjectSummary) => {
    setProjectHome(null);
    setActiveProjectId(project.id);
    handleCreateConversation(project, "dedicated");
  }, [handleCreateConversation]);
  // handleLeaveConvertedConversation moves the main view off a session that just became an assistant,
  // onto a new draft in its project rather than an arbitrary sibling the user did not pick.
  // handleLeaveConvertedConversation 将主视图从刚转为助手的会话移到其项目的新草稿，而不是用户未选择的任意同级会话。
  const handleLeaveConvertedConversation = React.useCallback((conversation: ConversationSummary) => {
    if (selectedConversationIdRef.current !== conversation.id) return;
    const project = workbenchStateRef.current.projects.find((candidate) => candidate.id === conversation.projectId);
    if (project) handleCreateConversationForProject(project);
  }, [handleCreateConversationForProject]);
  const assistantDock = useAssistantDockController({
    workbench: workbenchState,
    applySnapshot: applyProjectActionSnapshot,
    onLeaveConverted: handleLeaveConvertedConversation,
    onRestored: handleSelectConversation,
  });

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
  const { pinnedConversationIds, handleConversationMenuAction, unsupportedDelete } = useConversationMenuActions({
    translate: i18n.t,
    workbenchState,
    setRenameTarget,
    applyProjectActionSnapshot,
    applyLifecycleResponseSnapshot,
  });

  // commandPaletteOpen says whether the command palette (Ctrl+K) is shown.
  // commandPaletteOpen 表示命令面板（Ctrl+K）是否显示。
  const [commandPaletteOpen, setCommandPaletteOpen] = React.useState(false);
  // railEntries lists built-in areas first, then the environment variable page, then every enabled plugin
  // detail page from the live catalog.
  // railEntries 先列出内置区域，再列出环境变量页，然后是实时目录中每个已启用的插件详情页。
  const railEntries = React.useMemo<ActivityRailEntry[]>(() => {
    const builtIns = WORKSPACE_FEATURE_AREAS.map((area): ActivityRailEntry => ({
      id: area.id,
      label: i18n.t(area.label),
      icon: areaIconMap[area.id],
      active: activeArea === "chat" && workspaceMainView === area.id,
      onSelect: () => {
        // Plugin detail pages and the environment variable page own their rail entries, so the plugin entry
        // itself opens plugin management.
        // 插件详情页与环境变量页拥有各自的轨道入口，因此插件入口本身打开插件管理。
        if (area.id === "skills" && (isPluginDetailSubArea(activeWorkbenchSkillSubArea)
          || activeWorkbenchSkillSubArea === "environment-variables")) {
          setActiveWorkbenchSkillSubArea("catalog");
        }
        handleSelectTopFeatureArea(area.id);
      },
    }));
    // environmentVariables is a plugin management page that can be pinned like a plugin detail page.
    // environmentVariables 是插件管理下的页面，可以像插件详情页一样固定到轨道。
    const environmentVariables: ActivityRailEntry = {
      id: "environment-variables",
      label: i18n.t("environment.area"),
      icon: <Variable size={17} />,
      active: activeArea === "chat" && workspaceMainView === "skills" && activeWorkbenchSkillSubArea === "environment-variables",
      parentId: "skills",
      onSelect: () => {
        setActiveWorkbenchSkillSubArea("environment-variables");
        handleSelectTopFeatureArea("skills");
      },
    };
    const pluginPages =enabledPluginDetailMounts(workbenchState.plugins).map((selection): ActivityRailEntry => ({
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
    return [...builtIns, environmentVariables, ...pluginPages];
  }, [activeArea, activeWorkbenchSkillSubArea, handleSelectTopFeatureArea, i18n, workbenchState.plugins, workspaceMainView]);
  // Other surfaces, such as the memory page, open a prefilled conversation in the active project, or the
  // first project when none is active. A named project, such as one just added for on-device fixing, may
  // reach the workbench after the request, so the request waits for it.
  // 其他界面（如记忆页）在当前项目（没有当前项目时为第一个项目）中打开预填的会话。指定的项目（如刚为本机修复
  // 添加的项目）可能在请求之后才到达工作台，因此请求会等待它。
  const [requestedProjectId, setRequestedProjectId] = React.useState<string | null>(null);
  useNewConversationRequests((projectId) => {
    if (projectId) return setRequestedProjectId(projectId);
    const project = activeProject ?? workbenchState.projects[0];
    if (project) handleCreateConversationForProject(project);
  });
  // The project usually arrives in a service snapshot, whose later selection effects in the same commit would
  // still see the previous draft and reselect the backend choice; the draft therefore opens after that commit,
  // exactly as a click would.
  // 项目通常随服务快照到达，同一提交中靠后的选择 effect 仍会看到旧草稿并重新选中后端选择；因此草稿在该提交之后
  // 打开，与点击完全一致。
  React.useEffect(() => {
    const project = workbenchState.projects.find((entry) => entry.id === requestedProjectId);
    if (!project) return;
    const timer = window.setTimeout(() => {
      setRequestedProjectId(null);
      handleCreateConversationForProject(project);
    }, 0);
    return () => window.clearTimeout(timer);
  }, [handleCreateConversationForProject, requestedProjectId, workbenchState.projects]);
  // paletteConversations names every session with its project so the command palette can open it by either.
  // paletteConversations 为每个会话附上项目名称，使命令面板可按两者之一打开会话。
  const paletteConversations = React.useMemo(() => {
    const projectNames = new Map(workbenchState.projects.map((project) => [project.id, project.name]));
    return workbenchState.conversations.map((conversation) => ({
      id: conversation.id,
      title: conversation.title,
      projectName: conversation.projectId ? projectNames.get(conversation.projectId) ?? null : null,
      updatedAtMs: Number.parseInt(conversation.lastUpdatedAt ?? "", 10) || 0,
    }));
  }, [workbenchState.conversations, workbenchState.projects]);
  // commandPalette is mounted with the workbench itself, not with the rail: narrow windows show the rail only in
  // a drawer, and Ctrl+K must work there too.
  // commandPalette 随工作台本身挂载，而不是随轨道挂载：窄窗口只在抽屉中显示轨道，Ctrl+K 在那里也必须可用。
  const commandPalette = (
    <CommandPalette
      areas={railEntries}
      conversations={paletteConversations}
      onOpenConversation={handleSelectConversationLeavingHome}
      openConversationId={selectedConversationId}
      onNewConversation={activeProject ? () => handleCreateConversationForProject(activeProject) : undefined}
      onOpenChange={setCommandPaletteOpen}
      onOpenSessions={handleOpenWorkspaceChat}
      onOpenSettingsPage={(page) => {
        setActiveSettingsSubArea(page);
        handleSelectTopFeatureArea("settings");
      }}
      open={commandPaletteOpen}
      slashCommands={workbenchState.composerSlashCommands ?? []}
    />
  );
  // activityRail is shared by the workspace and settings shells so navigation stays put across areas.
  // activityRail 由工作台与设置外壳共用，使导航在区域切换时保持不动。
  const activityRail = (
    <ActivityRail
      entries={railEntries}
      onOpenCommandPalette={() => setCommandPaletteOpen(true)}
      sessionsActive={activeArea === "chat" && workspaceMainView === "chat"}
      settingsActive={activeArea === "settings"}
      onSelectSessions={handleOpenWorkspaceChat}
      onSelectSettings={() => handleSelectTopFeatureArea("settings")}
      // The flyout reuses the session panel's tree and persisted expansion so both surfaces stay identical.
      // 浮层复用会话面板的会话树与持久化展开状态，使两处界面保持一致。
      sessionTree={(
        <NavigationPanelBody
          activeProjectId={activeProjectId}
          conversations={consoleWorkbenchState.conversations}
          expandedProjectIds={consoleWorkbenchState.expandedProjectIds}
          onConversationMenuAction={handleConversationMenuAction}
          onCreateConversation={handleCreateConversationForProject}
          onCreateDedicatedConversation={handleCreateDedicatedConversationForProject}
          onCreateProject={handleCreateProject}
          onExpandedProjectIdsChange={handleExpandedProjectIdsChange}
          onProjectMenuAction={handleWorkbenchProjectMenuAction}
          onProjectGroupAction={handleProjectGroupAction}
          onSelectConversation={handleSelectConversationLeavingHome}
          onOpenProjectHome={openProjectHome}
          onSelectProject={setActiveProjectId}
          pinnedConversationIds={pinnedConversationIds}
          projectTree={consoleWorkbenchState.projectTree}
          projects={consoleWorkbenchState.projects}
          selectedConversationId={selectedConversationId}
        />
      )}
      sessionTreeActions={<SidebarFlowSwitch />}
      selectedConversationId={selectedConversationId}
      titlebarDragRegion={windowDragRegionEnabled}
      onTitlebarDoubleClick={handleTitlebarDoubleClick}
    />
  );

  React.useEffect(() => {
    notificationCoordinator.setRouteHandler((route) => {
      handleSelectConversationLeavingHome(route.conversationId);
    });
    return () => {
      notificationCoordinator.setRouteHandler(null);
    };
  }, [handleSelectConversationLeavingHome]);

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

  // approvalRequestId names the pending tool approval or value request; its outcome shows on the resumed tool card
  // and the session state, so only ask answers are acknowledged with a toast.
  // approvalRequestId 标识待处理的工具审批或取值请求；其结果体现在恢复的工具卡与会话状态上，因此只有 ask 回答才以提示确认。
  const approvalRequestId = workbenchState.pendingQuestion?.approval || workbenchState.pendingQuestion?.environment
    ? workbenchState.pendingQuestion.id
    : null;

  // handlePendingQuestionSubmit resumes a blocked ask tool call with validated modal answers.
  // handlePendingQuestionSubmit 使用弹窗校验后的答案恢复被阻塞的 ask 工具调用。
  const handlePendingQuestionSubmit = React.useCallback(async (requestId: string, answers: string[][]) => {
    try {
      await replyPendingQuestion(requestId, answers);
      if (requestId !== approvalRequestId) toast.success(i18n.t("workbenchAsk.submitted"));
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      throw nextError;
    }
  }, [approvalRequestId, i18n.t]);

  // handlePendingQuestionReject cancels a blocked ask tool call after a second confirmation.
  // handlePendingQuestionReject 在二次确认后取消被阻塞的 ask 工具调用。
  const handlePendingQuestionReject = React.useCallback(async (requestId: string) => {
    try {
      await rejectPendingQuestion(requestId);
      if (requestId !== approvalRequestId) toast.info(i18n.t("workbenchAsk.cancelled"));
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      throw nextError;
    }
  }, [approvalRequestId, i18n.t]);

  // handlePendingEnvironmentSubmit stores or declines the values of a pending environment value request.
  // handlePendingEnvironmentSubmit 存储或拒绝待处理环境变量取值请求的值。
  const handlePendingEnvironmentSubmit = React.useCallback(async (request: EnvironmentValuesSubmitRequest) => {
    try {
      await submitEnvironmentValues(request);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      throw nextError;
    }
  }, []);

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
      if (visibleConversationId(workbenchState.conversations, current)) {
        return current;
      }
      // The backend lifecycle choice is used only while active; assistant sessions are never auto-selected,
      // and null hands an assistant-only list to the empty-workbench draft route.
      // 后端生命周期选择仅在仍活动时使用；助手会话永不自动选中，null 会把仅剩助手的列表交给空工作台草稿路由。
      return fallbackMainViewConversationId(workbenchState.conversations, workbenchState.selectedConversationId);
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
      case "goal":
        return { handled: true, openTarget: "goal" };
      case "settings":
        return { handled: true, openTarget: "settings", settingsSection: null };
      case "style":
        return { handled: true, openTarget: "settings", settingsSection: "style" };
      case "answer":
        return { handled: true, openTarget: "settings", settingsSection: "answer" };
      case "knowledge":
        return { handled: true, openTarget: "settings", settingsSection: "knowledge" };
      case "collapse":
        return { handled: true, openTarget: "settings", settingsSection: "tools" };
      case "thinking":
        return { handled: true, openTarget: "settings", settingsSection: "thinking" };
      case "memory":
        return { handled: true, openTarget: "settings", settingsSection: "memory" };
      default:
        return { handled: false };
    }
  }, [handleCompactSelectedConversationContext, handleOpenWorkspaceSkills]);

  // projectHomeView carries the visible project home into the workspace shell, or null for the conversation stage.
  // projectHomeView 将可见的项目首页交给工作区外壳；为 null 时显示会话舞台。
  const projectHomeView = projectHomeProject ? {
    project: projectHomeProject,
    onClose: closeProjectHome,
    content: (
      <React.Suspense fallback={null}>
        <LazyProjectHome
          project={projectHomeProject}
          workbench={workbenchState}
          settingsRevision={{
            vmm: Math.max(
              globalProjectSettingsRevision.vmm,
              projectSettingsRevisionById[projectHomeProject.id]?.vmm ?? 0,
            ),
            agentSkills: Math.max(
              globalProjectSettingsRevision.agentSkills,
              projectSettingsRevisionById[projectHomeProject.id]?.agentSkills ?? 0,
            ),
          }}
          onRename={(name) => handleRenameConfirm({ kind: "project", id: projectHomeProject.id, name: projectHomeProject.name }, name)}
          onProjectMenuAction={handleWorkbenchProjectMenuAction}
          onCreateConversation={handleCreateConversationForProject}
          onCreateDedicatedConversation={handleCreateDedicatedConversationForProject}
          onSelectConversation={handleSelectConversationLeavingHome}
          onExecuteAttachmentCleanup={executeProjectAttachmentCleanup}
          onOpenKnowledge={handleOpenWorkspaceKnowledge}
          onLoadAgentSkillOverrides={loadProjectAgentSkillOverrides}
          onLoadVmmDraft={loadProjectVmmConfig}
          onPreviewAttachmentCleanup={previewProjectAttachmentCleanup}
          onSaveVmmDraft={handleProjectSettingsVmmSave}
          onSetAgentSkillState={handleProjectSettingsAgentSkillSave}
        />
      </React.Suspense>
    ),
  } : null;

  // dialogsRequested identifies a visible dialog without loading dialog code for the common workbench path.
  // dialogsRequested 识别可见对话框，避免常规工作台路径加载对话框代码。
  const dialogsRequested = projectFolderPickerOpen || renameTarget !== null || projectIconTarget !== null
    || archiveProjectTarget !== null || removeProjectTarget !== null
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
    <OpenVulcanAccountContext.Provider value={account}>
    <AppUpdateContext.Provider value={appUpdateHost}>
    <FeedbackCenterContext.Provider value={feedbackCenter}>
    {account.open ? <React.Suspense fallback={null}><OpenVulcanAccountDialog /></React.Suspense> : null}
    {feedbackCenter.feedbackSeed || feedbackCenter.messageCenterTab || feedbackCenter.announcements
      ? <React.Suspense fallback={null}><FeedbackSurfaces /></React.Suspense> : null}
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
    <ComposerAccountHostContext.Provider value={accountHostValue}>
    <DraftComposerContext.Provider value={draftComposerContext}>
    <ComposerMediaUploadProgressContext.Provider value={mediaUploadProgresses}>
    <div
      className="vk-workbench"
      data-native-buttons={nativeWindowButtons ? "" : undefined}
      data-window={showWindowControls ? "native" : "browser"}
    >
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
      {commandPalette}
      <PermissionJudgeEntryContext.Provider value={permissionJudgeEntry}>
      <AssistantDockContext.Provider value={assistantDock}>
      <main className="vk-workbench-main">
        {showConsoleSidebar ? (
          <WorkspaceShell
            activeProjectId={activeProjectId}
            startup={props.startup}
            workbench={consoleWorkbenchState}
            activeSkillSubArea={activeWorkbenchSkillSubArea}
            activeMemorySubArea={activeWorkbenchMemorySubArea}
            mainView={workspaceMainView}
            sidebarOpen={sessionPanelOpen}
            onSidebarOpenChange={setSessionPanelOpen}
            contextPanelOpen={contextPanelOpen}
            onContextPanelOpenChange={setContextPanelOpen}
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
            onCreateDedicatedConversation={handleCreateDedicatedConversationForProject}
            onCreateProject={handleCreateProject}
            projectHome={projectHomeView}
            onOpenProjectHome={openProjectHome}
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
              onProjectGroupAction={handleProjectGroupAction}
            onSelectProject={setActiveProjectId}
            onSelectConversation={handleSelectConversationLeavingHome}
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
            footer={statusBar}
            todolistStatusHost={todolistStatusHost}
            onTrace={webuiClickTrace}
          />
        ) : (
          <ExtractedFeatureShell
            serviceInstanceEpoch={props.serviceInstanceEpoch}
            onOpenUsageStatistics={usageStatisticsMount ? () => {
              setActiveWorkbenchSkillSubArea(usageStatisticsMount.subAreaId);
              handleSelectTopFeatureArea("skills");
            } : undefined}
            activeArea={activeArea}
            activeFeature={activeFeature}
            rail={activityRail}
            activeSettingsSubArea={activeSettingsSubArea}
            onSelectSettingsSubArea={setActiveSettingsSubArea}
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
              onSaveSessionTitleGeneration: (enabled) => applySkillWorkbenchAction(async () => await saveSessionTitleGeneration(enabled)),
              onSaveToolChainSettings: (enabled, maxSteps) => applySkillWorkbenchAction(async () => await saveToolChainSettings(enabled, maxSteps)),
              onSaveGitCommitSettings: (request) => applySkillWorkbenchAction(async () => (await saveGitCommitSettings(request)) ?? workbenchStateRef.current),
              onSaveDefaultCommandShell: (shellId) => applySkillWorkbenchAction(async () => (await saveDefaultCommandShell(shellId)) ?? workbenchStateRef.current),
              onSaveProviderRequestTraceSettings: (enabled, errorsEnabled) => applySkillWorkbenchAction(async () => (await saveProviderRequestTraceSettings(enabled, errorsEnabled)) ?? workbenchStateRef.current),
              onOpenSelectedProviderApiTraceDirectory: async () => {
                await openSelectedProviderApiTraceDirectory();
              },
              onClearSelectedProviderApiTraceDirectory: clearSelectedProviderApiTraceDirectory,
              onSaveProviderRetrySettings: (request) => applySkillWorkbenchAction(async () => (await saveProviderRetrySettings(request)) ?? workbenchStateRef.current),
              onRestoreArchivedConversations: (conversationIds) => applySkillWorkbenchAction(async () => (await restoreArchivedConversations(conversationIds)) ?? workbenchStateRef.current),
              onDeleteArchivedConversations: (conversationIds) => applySkillWorkbenchAction(async () => (await deleteArchivedConversations(conversationIds)) ?? workbenchStateRef.current),
            }}
            workbench={workbenchState}
            chromeActions={shellChromeActions}
            footer={statusBar}
            titlebarDragRegion={windowDragRegionEnabled}
            onTitlebarDoubleClick={handleTitlebarDoubleClick}
          />
        )}
        <React.Suspense fallback={null}>
          <ProjectFloatingTerminalLayer
            minimizeAnchorRef={projectTerminalButtonRef}
            onExpandedChange={setProjectTerminalExpanded}
            openRequest={projectTerminalOpenRequest}
            project={activeProject}
            projects={workbenchState.projects}
          />
        </React.Suspense>
        {assistantDock.assistants.length > 0 ? (
          <React.Suspense fallback={null}><AssistantFloatingLayer /></React.Suspense>
        ) : null}
        {selectedConversationId ? <React.Suspense fallback={null}><FileWindowLayer sessionId={selectedConversationId} /></React.Suspense> : null}
        <UnsupportedSessionDeleteDialog {...unsupportedDelete} />
        {showStartupOverlay ? <StartupOverlay startup={props.startup} /> : null}
      </main>
      </AssistantDockContext.Provider>
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
        pendingQuestionRequester={workbenchState.pendingQuestionRequester ?? null}
        projectFolderPickerOpen={projectFolderPickerOpen}
        projectIconTarget={projectIconTarget}
        removeProjectSubmitting={removeProjectSubmitting}
        removeProjectTarget={removeProjectTarget}
        renameTarget={renameTarget}
        stopConversationAgents={stopConversationAgents}
        workbench={workbenchState}
        onBrowseProjectDirectories={browseProjectDirectories}
        onCancelArchiveProject={handleCancelArchiveProject}
        onCancelProjectIcon={() => setProjectIconTarget(null)}
        onCancelRemoveProject={handleCancelRemoveProject}
        onCancelRename={() => setRenameTarget(null)}
        onCancelStopConversationRun={() => resolveStopConversationRunChoice(null)}
        onConfirmArchiveProject={handleArchiveProjectConfirm}
        onConfirmProjectFolder={handleConfirmProjectFolder}
        onConfirmProjectIcon={handleProjectIconConfirm}
        onConfirmRemoveProject={handleRemoveProjectConfirm}
        onConfirmRename={handleRenameConfirm}
        onConfirmStopConversationRun={resolveStopConversationRunChoice}
        onProjectFolderPickerOpenChange={setProjectFolderPickerOpen}
        onRejectPendingQuestion={handlePendingQuestionReject}
        onResetProjectIcon={handleProjectIconReset}
        onSubmitPendingQuestion={handlePendingQuestionSubmit}
        onSubmitPendingEnvironment={handlePendingEnvironmentSubmit}
      /></React.Suspense> : null}
    </div>
    </ComposerMediaUploadProgressContext.Provider>
    </DraftComposerContext.Provider>
    </ComposerAccountHostContext.Provider>
    </ComposerSettingsHostContext.Provider>
    </ToolCollapsePreferencesContext.Provider>
    </ThinkingDisplaySystemContext.Provider>
    </FeedbackCenterContext.Provider>
    </AppUpdateContext.Provider>
    </OpenVulcanAccountContext.Provider>
  );
}
