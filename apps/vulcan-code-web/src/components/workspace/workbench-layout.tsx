import React from "react";
import {
  FeatureShell as ExtractedFeatureShell,
  primaryFeatureAreas as buildPrimaryFeatureAreas,
} from "@/components/features/feature-shell";
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
  normalizeProjectIconSelection,
  type ProjectIconDialogTarget,
  type ProjectIconSelection,
} from "@/components/project/project-icons";
import {
  projectAgentSkillSaveMessage,
  projectConversationCount,
  type ProjectAgentSkillState,
  type ProjectSettingsDialogTarget,
  type ProjectSettingsRevision,
} from "@/components/project/project-settings-dialog";
import {
  type ConversationMenuAction,
  type ProjectMenuAction,
} from "@/components/project/ProjectSidebar";
import {
  ProjectFloatingTerminalLayer,
  type ProjectTerminalVisibilityRequest,
} from "@/components/project/terminal/project-floating-terminal-layer";
import { clearProjectTerminalInputBuffers } from "@/components/project/terminal/project-terminal-input-buffer";
import {
  type DraftConversationSelection,
} from "@/components/workspace/workspace-resizable-body";
import { retireMaterializedDraftSubmission } from "@/components/workspace/draft-submission-state";
import { resolveEmptyWorkbenchDraftRoute } from "@/components/workspace/workbench-layout/empty-workbench-route";
import { MediaUploadProgressPanel } from "@/components/workspace/media-upload-progress-panel";
import { ComposerMediaUploadProgressContext } from "@/components/composer/composer-media-upload-context";
import {
  mediaUploadProgressKey,
  reduceMediaUploadProgress,
} from "@/components/workspace/media-upload-progress-state";
import { WorkbenchDialogs } from "@/components/workspace/workbench-dialogs";
import { routeWorkspaceInputOwnerFromPointer } from "@/components/workspace/workspace-input-owner";
import { useWorkbenchShellChrome } from "@/components/workspace/workbench-layout/shell-chrome";
import {
  copyText,
  webuiClickTrace,
  type WorkbenchDeltaSignal,
} from "@/components/workspace/workbench-layout/types";
import { useWorkbenchConversationActions } from "@/components/workspace/workbench-layout/conversation-actions";
import { useWorkbenchProjectActions } from "@/components/workspace/workbench-layout/project-actions";
import { useWorkbenchSessionActions } from "@/components/workspace/workbench-layout/session-actions";
import { useStopConversationRunDecision } from "@/components/workspace/workbench-layout/use-stop-conversation-run-decision";
export type { WorkbenchDeltaSignal } from "@/components/workspace/workbench-layout/types";
import {
  WorkspaceShell,
  type WorkspaceMainView,
} from "@/components/workspace/workspace-shell";
import type {
  SkillActionProps,
  SkillSubAreaId,
} from "@/components/features/skills/SkillsArea";
import {
  findEnabledPluginDetailMount,
  isPluginDetailSubArea,
} from "@/components/features/skills/skill-area-types";
import type {
  MemorySubAreaId,
} from "@/components/features/memory/MemoryArea";
import { cn } from "@/lib/utils";
import { useI18n, type LocaleCode, type TranslationKey } from "@/i18n";
import { notificationCoordinator } from "@/notifications/coordinator";
import {
  resetNotificationPresentationContext,
  setNotificationPresentationContext,
} from "@/notifications/presentation-context";
import type { NotificationPresentationContext } from "@/notifications/types";
import { useConversationLiveStreamStore } from "@/state/conversation-live-stream-context";
import type {
  AgentSkillsCatalogUpdate,
  ComposerSlashCommandSummary,
  ConversationArchivedDelta,
  ConversationSummary,
  FeatureAreaSummary,
  LuaSkillsCatalogUpdate,
  MainAreaId,
  PendingQuestionSnapshot,
  PendingPromptSummary,
  ProjectSummary,
  ProjectTerminalRunningCountChangedEvent,
  SettingsSubAreaId,
  StartupSnapshot,
  SubagentCatalogUpdate,
  VmmConfigDraftSummary,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
  MediaUploadProgressChangedDelta,
} from "@/types";
import { listProjectTerminals } from "@/service/terminal-service";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import {
  rejectPendingQuestion,
  replyPendingQuestion,
  stageComposerAttachmentBytes,
} from "@/service/conversation-actions-service";
import {
  addProjectFromPath,
  archiveConversation,
  archiveProjectConversations,
  browseProjectDirectories,
  deleteArchivedConversations,
  markConversationRead,
  openExternalUrl,
  openProjectDirectory,
  openProjectDirectoryPath,
  removeProject,
  renameConversation,
  renameProject,
  reorderProjects,
  restoreArchivedConversations,
  resetProjectIcon,
  selectConversation,
  setExpandedTaskProjects,
  setProjectIcon,
  toggleProjectPinned,
} from "@/service/project-actions-service";
import {
  clearSelectedProviderApiTraceDirectory,
  openSelectedProviderApiTraceDirectory,
  removeMcpServerConfiguration,
  saveGitCommitSettings,
  saveProviderRequestTraceSettings,
  saveProviderRetrySettings,
  saveMcpServerConfiguration,
  savePromptOutputStyle,
  selectWebuiLocale,
} from "@/service/settings-actions-service";
import {
  installLuaSkill,
  loadProjectAgentSkillOverrides,
  refreshAgentSkillsCatalog,
  refreshLuaSkillsCatalog,
  setAgentSkillEnabled,
  setAgentSkillProjectState,
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
  loadWorkbenchSnapshot,
} from "@/service/workbench-snapshot-service";
import { localMediaSourceUrl } from "@/resources/localMedia";
import {
  applyWorkbenchDelta,
  markConversationLocallyRead,
  mergeWorkbenchSnapshot,
  shouldAcceptWorkbenchRevision,
  workbenchWithSelectedConversationProjection,
  type WorkbenchMergeOptions,
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
  // todolistStatusHost stores the footer node immediately following the version label.
  // todolistStatusHost 保存紧跟版本号的底栏 Todo 节点。
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
  // navigationSidebarOpen preserves sidebar collapse state across workspace and settings shells.
  // navigationSidebarOpen 在工作台与设置外壳之间保留侧栏折叠状态。
  const [navigationSidebarOpen, setNavigationSidebarOpen] = React.useState(true);
  // workspaceMainView stores the selected main surface inside the workbench.
  // workspaceMainView 保存工作台内部当前选中的主内容界面。
  const [workspaceMainView, setWorkspaceMainView] = React.useState<WorkspaceMainView>("chat");
  // activeWorkbenchSkillSubArea stores the embedded skills settings sub page.
  // activeWorkbenchSkillSubArea 保存内嵌技能设置的子页面。
  const [activeWorkbenchSkillSubArea, setActiveWorkbenchSkillSubArea] = React.useState<SkillSubAreaId>("catalog");
  // activeWorkbenchMemorySubArea stores the embedded memory settings sub page.
  // activeWorkbenchMemorySubArea 保存内嵌记忆设置的子页面。
  // activeWorkbenchMemorySubArea starts on the current product overview instead of a removed legacy id.
  // activeWorkbenchMemorySubArea 从当前产品总览开始，而不是已移除的旧分区标识。
  const [activeWorkbenchMemorySubArea, setActiveWorkbenchMemorySubArea] = React.useState<MemorySubAreaId>("overview");
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
      ?? props.workbench.conversations[0]?.id
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
  // conversationSelectionRequestVersionRef prevents an older selection response from committing after a newer click.
  // conversationSelectionRequestVersionRef 防止较早的选择响应在较新的点击之后提交。
  const conversationSelectionRequestVersionRef = React.useRef(0);
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
  // projectTerminalRunningCounts stores count-only lifecycle state independently from terminal content.
  // projectTerminalRunningCounts 独立于终端内容保存仅包含数量的生命周期状态。
  const [projectTerminalRunningCounts, setProjectTerminalRunningCounts] = React.useState<Record<
    string,
    { runningCount: number; revision: number }
  >>({});
  // projectTerminalCountListenerReady closes the bootstrap race between event registration and the first query.
  // projectTerminalCountListenerReady 消除事件注册与首次查询之间的初始竞态。
  const [projectTerminalCountListenerReady, setProjectTerminalCountListenerReady] = React.useState(false);
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
  React.useEffect(() => {
    setProjectTerminalCountListenerReady(false);
    setProjectTerminalRunningCounts({});
    if (!hasServiceRuntime()) return undefined;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    // subscription receives project metadata only; terminal output remains bound to visible display attachments.
    // subscription 仅接收项目元数据；终端输出仍严格绑定可见显示挂接。
    const subscription = serviceClient.subscribe<ProjectTerminalRunningCountChangedEvent>(
      "project.terminal.running-count.changed",
      (event) => {
        if (cancelled) return;
        setProjectTerminalRunningCounts((current) => {
          const previous = current[event.projectId];
          if (previous && previous.revision > event.revision) return current;
          return {
            ...current,
            [event.projectId]: {
              runningCount: event.runningCount,
              revision: event.revision,
            },
          };
        });
      },
    );
    void subscription.then((nextUnsubscribe) => {
      if (cancelled) nextUnsubscribe();
      else {
        unsubscribe = nextUnsubscribe;
        setProjectTerminalCountListenerReady(true);
      }
    }).catch(() => undefined);
    return () => {
      cancelled = true;
      unsubscribe?.();
      unsubscribe = null;
    };
  }, [props.serviceInstanceEpoch]);
  React.useEffect(() => {
    if (!activeProjectId || !projectTerminalCountListenerReady || !hasServiceRuntime()) return undefined;
    let cancelled = false;
    // loadRunningCount performs one metadata bootstrap for the active project without attaching screen content.
    // loadRunningCount 为当前活动项目执行一次元数据初始加载，不挂接屏幕内容。
    const loadRunningCount = async (): Promise<void> => {
      const catalog = await listProjectTerminals(activeProjectId);
      if (cancelled || !catalog || catalog.projectId !== activeProjectId) return;
      const runningCount = catalog.terminals.filter((terminal) => terminal.status === "running").length;
      setProjectTerminalRunningCounts((current) => {
        const previous = current[activeProjectId];
        if (previous && previous.revision > catalog.runningCountRevision) return current;
        return {
          ...current,
          [activeProjectId]: {
            runningCount,
            revision: catalog.runningCountRevision,
          },
        };
      });
    };
    void loadRunningCount().catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [activeProjectId, projectTerminalCountListenerReady, props.serviceInstanceEpoch]);
  React.useEffect(() => {
    if (
      isPluginDetailSubArea(activeWorkbenchSkillSubArea)
      && !findEnabledPluginDetailMount(workbenchState.plugins, activeWorkbenchSkillSubArea)
    ) {
      setActiveWorkbenchSkillSubArea("plugins");
    }
  }, [activeWorkbenchSkillSubArea, workbenchState.plugins]);
  // acceptedWorkbenchRevisionRef tracks the highest service revision accepted by any state entrypoint.
  // acceptedWorkbenchRevisionRef 跟踪任意状态入口已接受的最高服务版本。
  const acceptedWorkbenchRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedLuaSkillsRevisionRef tracks the newest full LuaSkills domain projection applied locally.
  // acceptedLuaSkillsRevisionRef 跟踪本地已应用的最新完整 LuaSkills 领域投影版本。
  const acceptedLuaSkillsRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedAgentSkillsRevisionRef tracks the newest full Agent Skills domain projection applied locally.
  // acceptedAgentSkillsRevisionRef 跟踪本地已应用的最新完整 Agent Skills 领域投影版本。
  const acceptedAgentSkillsRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedSubagentCatalogRevisionRef tracks the newest full agent-catalog domain projection applied locally.
  // acceptedSubagentCatalogRevisionRef 跟踪本地已应用的最新完整智能体目录领域投影版本。
  const acceptedSubagentCatalogRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedConversationLifecycleRevisionRef tracks the newest complete conversation lifecycle transition independently.
  // acceptedConversationLifecycleRevisionRef 独立跟踪最新的完整会话生命周期转换。
  const acceptedConversationLifecycleRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedConversationLifecycleRevisionByIdRef orders target-owned archive responses independently.
  // acceptedConversationLifecycleRevisionByIdRef 按目标独立排序归档响应。
  const acceptedConversationLifecycleRevisionByIdRef = React.useRef<Map<string, number>>(new Map());
  // acceptedProjectLifecycleRevisionRef tracks the newest authoritative full-project lifecycle snapshot.
  // acceptedProjectLifecycleRevisionRef 跟踪最新权威完整项目生命周期快照。
  const acceptedProjectLifecycleRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedProjectLifecycleRevisionByIdRef orders target-owned project removal responses independently.
  // acceptedProjectLifecycleRevisionByIdRef 按目标独立排序项目移除响应。
  const acceptedProjectLifecycleRevisionByIdRef = React.useRef<Map<string, number>>(new Map());
  // acceptedProjectSettingsRevisionByKeyRef orders settings invalidations independently by project and domain.
  // acceptedProjectSettingsRevisionByKeyRef 按项目与领域独立排序设置失效通知。
  const acceptedProjectSettingsRevisionByKeyRef = React.useRef<Map<string, number>>(new Map());
  // projectSettingsRevisionInitializedRef limits initial snapshot seeding to the first render.
  // projectSettingsRevisionInitializedRef 将初始快照播种限制在首次渲染。
  const projectSettingsRevisionInitializedRef = React.useRef(false);
  if (!projectSettingsRevisionInitializedRef.current) {
    for (const [projectId, revisions] of Object.entries(workbenchState.projectSettingsRevisions.byProject)) {
      acceptedProjectSettingsRevisionByKeyRef.current.set(`${projectId}:vmm`, revisions.vmm);
      acceptedProjectSettingsRevisionByKeyRef.current.set(`${projectId}:agentSkills`, revisions.agentSkills);
    }
    projectSettingsRevisionInitializedRef.current = true;
  }
  // acceptedGlobalProjectSettingsRevisionByDomainRef orders global invalidations independently by domain.
  // acceptedGlobalProjectSettingsRevisionByDomainRef 按领域独立排序全局设置失效通知。
  const acceptedGlobalProjectSettingsRevisionByDomainRef = React.useRef<ProjectSettingsRevision>(
    workbenchState.projectSettingsRevisions.global,
  );
  // acceptedServiceInstanceEpochRef identifies which local service owns all accepted revisions.
  // acceptedServiceInstanceEpochRef 标识所有已接受版本归属的本地服务实例。
  const acceptedServiceInstanceEpochRef = React.useRef(props.serviceInstanceEpoch);
  // workbenchStateRef exposes the latest workbench state to route-click callbacks without stale closures.
  // workbenchStateRef 向路由点击回调暴露最新工作台状态，避免闭包过期。
  const workbenchStateRef = React.useRef<WorkbenchSnapshot>(workbenchState);
  workbenchStateRef.current = workbenchState;
  React.useEffect(() => {
    // Ephemeral subscribers observe the exact batch after React commits the corresponding structural state.
    // 瞬态订阅者在 React 提交对应结构状态后观察精确批次。
    liveStreamStore.flushNotifications();
  }, [liveStreamStore, workbenchState]);
  // conversationArchivePendingIdsRef blocks duplicate archive RPCs until failure or visible lifecycle convergence.
  // conversationArchivePendingIdsRef 在失败或可见生命周期收敛前阻止重复归档 RPC。
  const conversationArchivePendingIdsRef = React.useRef<Set<string>>(new Set());
  acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, workbenchState.revision ?? 0);
  // expandedProjectIdsLocallyEditedRef records whether the user has touched project expansion in this session.
  // expandedProjectIdsLocallyEditedRef 记录本轮会话中用户是否已经手动修改过项目展开状态。
  const expandedProjectIdsLocallyEditedRef = React.useRef(false);
  // localeSaveSeqRef prevents older locale-save responses from overriding newer choices.
  // localeSaveSeqRef 防止较早的语言保存响应覆盖较新的选择。
  const localeSaveSeqRef = React.useRef(0);
  // mergeWorkbenchWithLocalCaches centralizes service snapshot merges while preserving UI-local state.
  // mergeWorkbenchWithLocalCaches 集中处理服务快照合并，同时保留界面本地状态。
  const mergeWorkbenchWithLocalCaches = React.useCallback((
    current: WorkbenchSnapshot,
    incoming: WorkbenchSnapshot,
    options?: WorkbenchMergeOptions,
  ) => (
    (() => {
      const merged = mergeWorkbenchSnapshot(
        current,
        incoming,
        options,
      );
      if (merged !== current) {
        // mergedRevision is the authoritative full-snapshot version accepted by this merge.
        // mergedRevision 是本次合并接受的权威全量快照版本。
        const mergedRevision = merged.revision ?? 0;
        acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, mergedRevision);
        acceptedLuaSkillsRevisionRef.current = Math.max(acceptedLuaSkillsRevisionRef.current, mergedRevision);
        acceptedAgentSkillsRevisionRef.current = Math.max(acceptedAgentSkillsRevisionRef.current, mergedRevision);
        acceptedSubagentCatalogRevisionRef.current = Math.max(acceptedSubagentCatalogRevisionRef.current, mergedRevision);
        acceptedConversationLifecycleRevisionRef.current = Math.max(
          acceptedConversationLifecycleRevisionRef.current,
          mergedRevision,
        );
        acceptedProjectLifecycleRevisionRef.current = Math.max(
          acceptedProjectLifecycleRevisionRef.current,
          mergedRevision,
        );
      }
      return merged;
    })()
  ), []);
  // resetAcceptedWorkbenchOrderingForServiceInstance clears ordering owned by a replaced service.
  // resetAcceptedWorkbenchOrderingForServiceInstance 清理由已替换服务持有的版本排序。
  const resetAcceptedWorkbenchOrderingForServiceInstance = React.useCallback((
    serviceInstanceEpoch: number,
  ): boolean => {
    // serviceInstanceChanged is the only condition that permits a lower revision to become authoritative.
    // serviceInstanceChanged 是唯一允许较低版本重新成为权威状态的条件。
    const serviceInstanceChanged =
      acceptedServiceInstanceEpochRef.current !== serviceInstanceEpoch;
    if (!serviceInstanceChanged) {
      return false;
    }
    acceptedServiceInstanceEpochRef.current = serviceInstanceEpoch;
    acceptedWorkbenchRevisionRef.current = 0;
    acceptedLuaSkillsRevisionRef.current = 0;
    acceptedAgentSkillsRevisionRef.current = 0;
    acceptedSubagentCatalogRevisionRef.current = 0;
    acceptedConversationLifecycleRevisionRef.current = 0;
    acceptedConversationLifecycleRevisionByIdRef.current.clear();
    acceptedProjectLifecycleRevisionRef.current = 0;
    acceptedProjectLifecycleRevisionByIdRef.current.clear();
    acceptedProjectSettingsRevisionByKeyRef.current.clear();
    acceptedGlobalProjectSettingsRevisionByDomainRef.current = {
      vmm: 0,
      agentSkills: 0,
      codebaseMemory: 0,
    };
    return true;
  }, []);
  // acceptServiceWorkbenchSnapshot reserves one service snapshot revision before applying related local state.
  // acceptServiceWorkbenchSnapshot 在应用关联本地状态前预占一个服务快照版本。
  const acceptServiceWorkbenchSnapshot = React.useCallback((snapshot: WorkbenchSnapshot): boolean => {
    const incomingRevision = snapshot.revision ?? 0;
    if (!shouldAcceptWorkbenchRevision(acceptedWorkbenchRevisionRef.current, incomingRevision)) {
      return false;
    }
    acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, incomingRevision);
    acceptedLuaSkillsRevisionRef.current = Math.max(acceptedLuaSkillsRevisionRef.current, incomingRevision);
    acceptedAgentSkillsRevisionRef.current = Math.max(acceptedAgentSkillsRevisionRef.current, incomingRevision);
    acceptedSubagentCatalogRevisionRef.current = Math.max(acceptedSubagentCatalogRevisionRef.current, incomingRevision);
    acceptedConversationLifecycleRevisionRef.current = Math.max(
      acceptedConversationLifecycleRevisionRef.current,
      incomingRevision,
    );
    acceptedProjectLifecycleRevisionRef.current = Math.max(
      acceptedProjectLifecycleRevisionRef.current,
      incomingRevision,
    );
    return true;
  }, []);
  // acceptConversationArchiveTransitions filters archive payloads through full-snapshot and per-target revisions.
  // acceptConversationArchiveTransitions 通过完整快照与按目标版本过滤归档载荷。
  //
  // Parameters: transitions are exact archive payloads and generation is their committed backend revision.
  // 参数：transitions 是精确归档载荷，generation 是其已提交后端版本。
  // Returns: only transitions that are newer for their own conversation lifecycle target.
  // 返回值：仅返回对各自会话生命周期目标而言较新的转换。
  const acceptConversationArchiveTransitions = React.useCallback((
    transitions: ConversationArchivedDelta[],
    generation: number,
  ): ConversationArchivedDelta[] => transitions.filter((transition) => {
    // targetRevision combines the latest full snapshot and target-owned archive response.
    // targetRevision 合并最新完整快照与目标自有归档响应。
    const targetRevision = Math.max(
      acceptedConversationLifecycleRevisionRef.current,
      acceptedConversationLifecycleRevisionByIdRef.current.get(transition.conversationId) ?? 0,
    );
    if (!shouldAcceptWorkbenchRevision(targetRevision, generation)) {
      return false;
    }
    acceptedConversationLifecycleRevisionByIdRef.current.set(transition.conversationId, generation);
    return true;
  }), []);
  // acceptProjectRemovalTransition reserves one target-owned project removal revision.
  // acceptProjectRemovalTransition 预占一条目标自有项目移除版本。
  //
  // Parameters: projectId is the removed project target and generation is the committed backend revision.
  // 参数：projectId 是移除项目目标，generation 是已提交后端版本。
  // Returns: true only when this removal is newer for the exact project target.
  // 返回值：仅当此移除对精确项目目标而言较新时返回 true。
  const acceptProjectRemovalTransition = React.useCallback((projectId: string, generation: number): boolean => {
    // targetRevision combines the latest full snapshot and target-owned project removal response.
    // targetRevision 合并最新完整快照与目标自有项目移除响应。
    const targetRevision = Math.max(
      acceptedProjectLifecycleRevisionRef.current,
      acceptedProjectLifecycleRevisionByIdRef.current.get(projectId) ?? 0,
    );
    if (!shouldAcceptWorkbenchRevision(targetRevision, generation)) {
      return false;
    }
    acceptedProjectLifecycleRevisionByIdRef.current.set(projectId, generation);
    return true;
  }, []);
  // applyLifecycleResponseSnapshot applies only archive and project-removal fields from a rejected full RPC snapshot.
  // applyLifecycleResponseSnapshot 仅从被拒绝的完整 RPC 快照应用归档与项目移除字段。
  //
  // Parameters: snapshot is the committed RPC result, conversationIds are archived targets, and removedProjectId is the optional removed project.
  // 参数：snapshot 是已提交 RPC 结果，conversationIds 是归档目标，removedProjectId 是可选的已移除项目。
  // Returns: true when at least one target transition was applied or the targets already converged.
  // 返回值：至少应用一条目标转换或目标已经收敛时返回 true。
  const applyLifecycleResponseSnapshot = React.useCallback((
    snapshot: WorkbenchSnapshot,
    conversationIds: string[],
    removedProjectId?: string,
  ): boolean => {
    // generation is the exact backend revision committed by the lifecycle RPC.
    // generation 是生命周期 RPC 提交的精确后端版本。
    const generation = snapshot.revision ?? 0;
    // archiveTransitions project only target-owned fields from the full RPC snapshot.
    // archiveTransitions 仅从完整 RPC 快照投影目标自有字段。
    const archiveTransitions = conversationIds.map((conversationId): ConversationArchivedDelta => ({
      conversationId,
      archivedConversation: (snapshot.archivedConversations ?? [])
        .find((conversation) => conversation.id === conversationId) ?? null,
      selectedConversationId: snapshot.selectedConversationId ?? null,
    }));
    // acceptedArchives contains only target transitions newer than their own lifecycle state.
    // acceptedArchives 仅包含比各自生命周期状态更新的目标转换。
    const acceptedArchives = acceptConversationArchiveTransitions(archiveTransitions, generation);
    // projectAccepted records whether the optional project removal owns a newer target revision.
    // projectAccepted 记录可选项目移除是否拥有较新的目标版本。
    const projectAccepted = removedProjectId
      ? acceptProjectRemovalTransition(removedProjectId, generation)
      : false;
    if (acceptedArchives.length === 0 && !projectAccepted) {
      // currentWorkbench is used only to verify that a previously accepted transition already converged.
      // currentWorkbench 仅用于验证此前接受的转换已经收敛。
      const currentWorkbench = workbenchStateRef.current;
      // conversationsScheduled recognizes target transitions already accepted but not yet committed by React rendering.
      // conversationsScheduled 识别已经接受但尚未由 React 渲染提交的目标转换。
      const conversationsScheduled = conversationIds.every((conversationId) => Math.max(
        acceptedConversationLifecycleRevisionRef.current,
        acceptedConversationLifecycleRevisionByIdRef.current.get(conversationId) ?? 0,
      ) >= generation);
      const conversationsConverged = conversationIds.every((conversationId) => (
        !currentWorkbench.conversations.some((conversation) => conversation.id === conversationId)
      ));
      // projectScheduled recognizes a project removal already accepted but not yet rendered.
      // projectScheduled 识别已经接受但尚未渲染的项目移除。
      const projectScheduled = !removedProjectId || Math.max(
        acceptedProjectLifecycleRevisionRef.current,
        acceptedProjectLifecycleRevisionByIdRef.current.get(removedProjectId) ?? 0,
      ) >= generation;
      const projectConverged = !removedProjectId
        || !currentWorkbench.projects.some((project) => project.id === removedProjectId);
      return (conversationsScheduled || conversationsConverged)
        && (projectScheduled || projectConverged);
    }
    // acceptedEnvelopes separates conversation and project domains when only one target class is newer.
    // acceptedEnvelopes 在仅一类目标较新时拆分会话与项目领域。
    const acceptedEnvelopes: WorkbenchDeltaEnvelope[] = [];
    if (projectAccepted && removedProjectId) {
      acceptedEnvelopes.push({
        generation,
        delta: {
          kind: "projectRemoved",
          payload: {
            projectId: removedProjectId,
            projects: snapshot.projects,
            expandedProjectIds: snapshot.expandedProjectIds ?? [],
            archivedConversations: acceptedArchives,
          },
        },
      });
    } else if (acceptedArchives.length > 0) {
      acceptedEnvelopes.push({
        generation,
        delta: {
          kind: "conversationsArchived",
          payload: { archivedConversations: acceptedArchives },
        },
      });
    }
    acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, generation);
    setWorkbenchState((current) => acceptedEnvelopes.reduce(
      (nextWorkbench, envelope) => applyWorkbenchDelta(nextWorkbench, envelope),
      current,
    ));
    return true;
  }, [acceptConversationArchiveTransitions, acceptProjectRemovalTransition]);
  // applyLocalPreviewWithLocalCaches applies optimistic snapshots without service revision gating.
  // applyLocalPreviewWithLocalCaches 应用乐观快照，不经过服务版本门禁。
  const applyLocalPreviewWithLocalCaches = React.useCallback((current: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => (
    incoming
  ), []);
  // handleWebuiLocaleChange applies the locale immediately and persists it through the service.
  // handleWebuiLocaleChange 立即应用语言，并通过服务持久化。
  const handleWebuiLocaleChange = React.useCallback((locale: LocaleCode) => {
    const previousLocale = i18n.locale;
    const requestSeq = localeSaveSeqRef.current + 1;
    localeSaveSeqRef.current = requestSeq;
    i18n.setLocale(locale);
    void selectWebuiLocale(locale).then((snapshot) => {
      if (snapshot && localeSaveSeqRef.current === requestSeq) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    }).catch((error) => {
      if (localeSaveSeqRef.current === requestSeq) {
        i18n.setLocale(previousLocale);
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
  // applyLuaSkillsCatalogUpdate applies the newest complete LuaSkills domain response independently of unrelated deltas.
  // applyLuaSkillsCatalogUpdate 独立于无关增量应用最新的完整 LuaSkills 领域响应。
  //
  // Parameters: update contains the committed revision, LuaSkills catalog, and derived plugin rows.
  // 参数：update 包含已提交版本、LuaSkills 目录与派生插件行。
  // Returns: nothing; accepted data is scheduled into workbench state without lowering its global revision.
  // 返回值：无；已接受数据会安排到工作台状态且不会降低全局版本。
  const applyLuaSkillsCatalogUpdate = React.useCallback((update: LuaSkillsCatalogUpdate) => {
    if (!shouldAcceptWorkbenchRevision(acceptedLuaSkillsRevisionRef.current, update.revision)) {
      return;
    }
    acceptedLuaSkillsRevisionRef.current = update.revision;
    acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, update.revision);
    setWorkbenchState((current) => {
      if (update.revision < acceptedLuaSkillsRevisionRef.current) {
        return current;
      }
      return {
        ...current,
        revision: Math.max(current.revision ?? 0, update.revision),
        luaSkills: update.luaSkills,
        plugins: update.plugins,
      };
    });
  }, []);
  // applyAgentSkillsCatalogUpdate applies the newest complete Agent Skills domain response independently of unrelated deltas.
  // applyAgentSkillsCatalogUpdate 独立于无关增量应用最新的完整 Agent Skills 领域响应。
  //
  // Parameters: update contains the committed revision and complete Agent Skills catalog.
  // 参数：update 包含已提交版本与完整 Agent Skills 目录。
  // Returns: nothing; accepted data is scheduled into workbench state without lowering its global revision.
  // 返回值：无；已接受数据会安排到工作台状态且不会降低全局版本。
  const applyAgentSkillsCatalogUpdate = React.useCallback((update: AgentSkillsCatalogUpdate) => {
    if (!shouldAcceptWorkbenchRevision(acceptedAgentSkillsRevisionRef.current, update.revision)) {
      return;
    }
    acceptedAgentSkillsRevisionRef.current = update.revision;
    acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, update.revision);
    setWorkbenchState((current) => {
      if (update.revision < acceptedAgentSkillsRevisionRef.current) {
        return current;
      }
      return {
        ...current,
        revision: Math.max(current.revision ?? 0, update.revision),
        agentSkills: update.agentSkills,
      };
    });
  }, []);
  // applySubagentCatalogUpdate applies the newest complete agent-catalog domain response independently of unrelated deltas.
  // applySubagentCatalogUpdate 独立于无关增量应用最新的完整智能体目录领域响应。
  //
  // Parameters: update contains the committed revision and complete agent catalog.
  // 参数：update 包含已提交版本与完整智能体目录。
  // Returns: nothing; accepted data is scheduled into workbench state without lowering its global revision.
  // 返回值：无；已接受数据会安排到工作台状态且不会降低全局版本。
  const applySubagentCatalogUpdate = React.useCallback((update: SubagentCatalogUpdate) => {
    if (!shouldAcceptWorkbenchRevision(acceptedSubagentCatalogRevisionRef.current, update.revision)) {
      return;
    }
    acceptedSubagentCatalogRevisionRef.current = update.revision;
    acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, update.revision);
    setWorkbenchState((current) => {
      if (update.revision < acceptedSubagentCatalogRevisionRef.current) {
        return current;
      }
      return {
        ...current,
        revision: Math.max(current.revision ?? 0, update.revision),
        subagentCatalog: update.subagentCatalog,
      };
    });
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
  // projectSettingsRevisionById exposes accepted domain revisions without coupling projects or drafts.
  // projectSettingsRevisionById 暴露已接收的领域版本，同时避免项目或草稿相互耦合。
  const [projectSettingsRevisionById, setProjectSettingsRevisionById] = React.useState<Record<string, ProjectSettingsRevision>>(
    workbenchState.projectSettingsRevisions.byProject,
  );
  // globalProjectSettingsRevision exposes independently ordered global invalidations for every project dialog.
  // globalProjectSettingsRevision 向所有项目弹窗暴露独立排序的全局失效版本。
  const [globalProjectSettingsRevision, setGlobalProjectSettingsRevision] = React.useState<ProjectSettingsRevision>(
    workbenchState.projectSettingsRevisions.global,
  );
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
    if (area === "knowledge") {
      setActiveArea("chat");
      setWorkspaceMainView("knowledge");
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
  // handleOpenWorkspaceAgentSkills opens prompt skill management inside the workbench main panel.
  // handleOpenWorkspaceAgentSkills 在工作台主面板内部打开提示词技能管理。
  const handleOpenWorkspaceAgentSkills = React.useCallback(() => {
    setActiveArea("chat");
    setWorkspaceMainView("agent-skills");
  }, []);
  // handleOpenWorkspaceAgents opens agent management inside the workbench main panel.
  // handleOpenWorkspaceAgents 在工作台主面板内部打开智能体管理。
  const handleOpenWorkspaceAgents = React.useCallback(() => {
    setActiveArea("chat");
    setWorkspaceMainView("agents");
  }, []);
  // handleOpenWorkspaceMemory opens memory settings inside the workbench main panel.
  // handleOpenWorkspaceMemory 在工作台主面板内部打开记忆设置。
  const handleOpenWorkspaceMemory = React.useCallback(() => {
    setActiveArea("chat");
    setWorkspaceMainView("memory");
  }, []);
  // handleOpenWorkspaceKnowledge opens knowledge library management inside the workbench main panel.
  // handleOpenWorkspaceKnowledge 在工作台主面板内打开知识库管理。
  const handleOpenWorkspaceKnowledge = React.useCallback(() => {
    setActiveArea("chat");
    setWorkspaceMainView("knowledge");
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
  // selectedDraftConversation stores the draft used by the console header and empty state.
  // selectedDraftConversation 保存控制台标题栏与空状态使用的草稿会话。
  const selectedDraftConversation = selectedConversationId === draftConversation?.id ? draftConversation : null;
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
  // consoleWorkbenchState reprojects selected-session fields from the UI-local selection.
  // consoleWorkbenchState 根据 UI 本地选择重新投影当前会话字段。
  const consoleWorkbenchState = React.useMemo<WorkbenchSnapshot>(() => {
    const projectedConversationId = selectedDraftConversation ? null : selectedConversationId;
    return workbenchWithSelectedConversationProjection(workbenchState, projectedConversationId);
  }, [selectedConversationId, selectedDraftConversation, workbenchState]);
  // Keep the WorkbenchApp V1 owner aligned with the real local route; drafts own no durable session.
  // 让 WorkbenchApp 的 V1 归属对齐真实本地路由；草稿不持有持久化会话。
  React.useEffect(() => {
    props.onSelectedConversationIdChanged(
      selectedDraftConversation ? null : selectedConversationId,
    );
  }, [props.onSelectedConversationIdChanged, selectedConversationId, selectedDraftConversation]);
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
    luaSkillActions,
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
    selectedDraftConversation,
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
    pendingMaterializedConversationIdRef,
    workbenchStateRef,
    requestStopConversationRunChoice,
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
            toast.success(formatWorkbenchTranslation(i18n.t, "workbenchConversation.unpinned", { conversation: conversation.title }));
          } else {
            next.add(conversation.id);
            toast.success(formatWorkbenchTranslation(i18n.t, "workbenchConversation.pinned", { conversation: conversation.title }));
          }
          return next;
        });
        return;
      }
      if (action === "archive") {
        if (conversationArchivePendingIdsRef.current.has(conversation.id)) {
          return;
        }
        conversationArchivePendingIdsRef.current.add(conversation.id);
        // archiveCommitted keeps the request locked after success until the active row actually disappears.
        // archiveCommitted 在成功后继续保持请求锁，直到活动行真正消失。
        let archiveCommitted = false;
        try {
          const snapshot = await archiveConversation(conversation.id);
          if (snapshot) {
            const snapshotApplied = applyProjectActionSnapshot(snapshot);
            if (!snapshotApplied && !applyLifecycleResponseSnapshot(snapshot, [conversation.id])) {
              throw new Error("归档已提交，但本地生命周期状态未能收敛，请重试刷新。");
            }
          } else {
            applyProjectActionSnapshot(
              applyPreviewConversationAction(workbenchState, conversation.id, action).snapshot,
              "local",
            );
          }
          archiveCommitted = true;
          toast.success(formatWorkbenchTranslation(i18n.t, "workbenchConversation.archived", { conversation: conversation.title }));
        } finally {
          if (!archiveCommitted) {
            conversationArchivePendingIdsRef.current.delete(conversation.id);
          }
        }
        return;
      }
      if (action === "open-directory") {
        const project = projectForConversation(workbenchState, conversation);
        if (!project) {
          toast.error(i18n.t("workbenchConversation.noProject"));
          return;
        }
        const message = await openProjectDirectoryPath(project.path);
        toast.success(message ?? formatWorkbenchTranslation(i18n.t, "projectActions.openRequested", { project: project.name }));
        return;
      }
      if (action === "copy-workdir") {
        const project = projectForConversation(workbenchState, conversation);
        if (!project) {
          toast.error(i18n.t("workbenchConversation.noProject"));
          return;
        }
        await navigator.clipboard.writeText(project.path);
        toast.success(i18n.t("workbenchConversation.copiedWorkspace"));
        return;
      }
      if (action === "copy-title") {
        await navigator.clipboard.writeText(conversation.title);
        toast.success(i18n.t("workbenchConversation.copiedTitle"));
        return;
      }
      if (action === "copy-session-id") {
        await navigator.clipboard.writeText(conversation.id);
        toast.success(i18n.t("workbenchConversation.copiedSessionId"));
        return;
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyLifecycleResponseSnapshot, applyProjectActionSnapshot, i18n.t, workbenchState]);

  // handleSelectConversation selects a conversation and persists its read marker.
  // handleSelectConversation 选择会话并持久化它的已读标记。
  const handleSelectConversation = React.useCallback((conversationId: string) => {
    handleOpenWorkspaceChat();
    // requestVersion keeps the visible route on its current fully owned snapshot until the target snapshot is ready.
    // requestVersion 在目标快照就绪前让可见路由继续使用当前完整归属快照。
    const requestVersion = conversationSelectionRequestVersionRef.current + 1;
    conversationSelectionRequestVersionRef.current = requestVersion;
    void selectConversation(conversationId)
      .then((snapshot) => {
        if (conversationSelectionRequestVersionRef.current !== requestVersion) {
          return null;
        }
        // Commit the route and its session-sensitive controls atomically after the exact selection response arrives.
        // 精确选择响应到达后，原子提交路由及其会话敏感控制字段。
        selectedConversationIdRef.current = conversationId;
        setSelectedConversationId(conversationId);
        if (snapshot && snapshot.selectedConversationId === conversationId) {
          liveStreamStore.resetForWorkbenchSnapshot();
          liveStreamStore.flushNotifications();
          setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(
            current,
            snapshot,
            { selectedConversationSource: "incoming" },
          ));
        } else {
          // Browser preview has no service snapshot but still owns a local-only route.
          // 浏览器预览没有服务快照，但仍持有本地路由。
          setWorkbenchState((current) => markConversationLocallyRead(current, conversationId));
        }
        // Read-marker persistence is ancillary and must never turn a successful navigation into an error toast.
        // 已读标记持久化属于附属操作，绝不能把成功导航变成错误提示。
        return markConversationRead(conversationId).catch((error) => {
          console.warn("[workbench] conversation read marker update failed", error);
          return null;
        });
      })
      .then((snapshot) => {
        // Read-marker responses can arrive after another conversation has already been selected.
        // 已读标记响应可能在另一个会话已经被选中后才到达。
        if (
          snapshot
          && conversationSelectionRequestVersionRef.current === requestVersion
          && selectedConversationIdRef.current === conversationId
          && snapshot.selectedConversationId === conversationId
        ) {
          liveStreamStore.resetForWorkbenchSnapshot();
          liveStreamStore.flushNotifications();
          setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
        }
      })
      .catch(async (error) => {
        if (conversationSelectionRequestVersionRef.current !== requestVersion) {
          return;
        }
        const message = error instanceof Error ? error.message : String(error);
        if (message === "会话已被删除，已经清理。") {
          // Clear the deleted route before applying the refreshed authoritative snapshot.
          // 应用刷新后的权威快照前先清理已删除会话的路由选择。
          selectedConversationIdRef.current = null;
          setSelectedConversationId(null);
          try {
            const snapshot = await loadWorkbenchSnapshot();
            applyProjectActionSnapshot(snapshot);
          } catch (refreshError) {
            toast.error(`${message} 列表刷新失败：${refreshError instanceof Error ? refreshError.message : String(refreshError)}`);
            return;
          }
        }
        toast.error(message);
      });
  }, [
    applyProjectActionSnapshot,
    handleOpenWorkspaceChat,
    liveStreamStore,
    loadWorkbenchSnapshot,
    selectedConversationIdRef,
    setSelectedConversationId,
  ]);

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
      toast.error(i18n.t("workbenchConversation.nameEmpty"));
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
        applyProjectActionSnapshot(applyPreviewProjectRename(workbenchState, target.id, effectiveProjectName).snapshot, "local");
      } else {
        applyProjectActionSnapshot(applyPreviewConversationRename(workbenchState, target.id, normalizedName).snapshot, "local");
      }
      setRenameTarget(null);
      toast.success(target.kind === "project"
        ? normalizedName
          ? formatWorkbenchTranslation(i18n.t, "workbenchConversation.projectRenamed", { project: effectiveProjectName })
          : formatWorkbenchTranslation(i18n.t, "workbenchConversation.projectNameRestored", { project: effectiveProjectName })
        : formatWorkbenchTranslation(i18n.t, "workbenchConversation.conversationRenamed", { conversation: normalizedName }));
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, i18n.t, workbenchState]);

  // handleArchiveProjectConfirm archives all conversations under the selected project.
  // handleArchiveProjectConfirm 归档选定项目下的全部对话。
  const handleArchiveProjectConfirm = React.useCallback(async () => {
    if (!archiveProjectTarget || archiveProjectSubmitting) {
      return;
    }
    setArchiveProjectSubmitting(true);
    try {
      // archivedConversationIds captures the exact visible child lifecycle set before the RPC commits.
      // archivedConversationIds 捕获 RPC 提交前的精确可见子生命周期集合。
      const archivedConversationIds = workbenchState.conversations
        .filter((conversation) => conversation.projectId === archiveProjectTarget.id)
        .map((conversation) => conversation.id);
      const snapshot = await archiveProjectConversations(archiveProjectTarget.id);
      if (snapshot) {
        const snapshotApplied = applyProjectActionSnapshot(snapshot);
        if (!snapshotApplied && !applyLifecycleResponseSnapshot(snapshot, archivedConversationIds)) {
          throw new Error("批量归档已提交，但本地生命周期状态未能收敛，请重试刷新。");
        }
      } else {
        applyProjectActionSnapshot(applyPreviewProjectAction(workbenchState, archiveProjectTarget.id, "archive-conversations").snapshot, "local");
      }
      toast.success(formatWorkbenchTranslation(i18n.t, "workbenchConversation.archivedCount", {
        count: String(projectConversationCount(workbenchState, archiveProjectTarget.id)),
      }));
      setArchiveProjectTarget(null);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      setArchiveProjectSubmitting(false);
    }
  }, [applyLifecycleResponseSnapshot, applyProjectActionSnapshot, archiveProjectSubmitting, archiveProjectTarget, i18n.t, workbenchState]);

  // handleRemoveProjectConfirm archives conversations and removes the selected project.
  // handleRemoveProjectConfirm 归档对话并移除选中的项目。
  const handleRemoveProjectConfirm = React.useCallback(async () => {
    if (!removeProjectTarget || removeProjectSubmitting) {
      return;
    }
    setRemoveProjectSubmitting(true);
    try {
      // archivedConversationIds captures the exact visible child lifecycle set before project removal commits.
      // archivedConversationIds 捕获项目移除提交前的精确可见子生命周期集合。
      const archivedConversationIds = workbenchState.conversations
        .filter((conversation) => conversation.projectId === removeProjectTarget.id)
        .map((conversation) => conversation.id);
      const snapshot = await removeProject(removeProjectTarget.id);
      if (snapshot) {
        const snapshotApplied = applyProjectActionSnapshot(snapshot);
        if (!snapshotApplied && !applyLifecycleResponseSnapshot(
          snapshot,
          archivedConversationIds,
          removeProjectTarget.id,
        )) {
          throw new Error("项目移除已提交，但本地生命周期状态未能收敛，请重试刷新。");
        }
      } else {
        applyProjectActionSnapshot(applyPreviewProjectAction(workbenchState, removeProjectTarget.id, "remove-project").snapshot, "local");
      }
      clearProjectTerminalInputBuffers(removeProjectTarget.id);
      toast.success(formatWorkbenchTranslation(i18n.t, "workbenchConversation.archivedAndRemovedProject", { project: removeProjectTarget.name }));
      setRemoveProjectTarget(null);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      setRemoveProjectSubmitting(false);
    }
  }, [applyLifecycleResponseSnapshot, applyProjectActionSnapshot, i18n.t, removeProjectSubmitting, removeProjectTarget, workbenchState]);

  // handlePendingQuestionSubmit resumes a blocked ask tool call with validated modal answers.
  // handlePendingQuestionSubmit 使用弹窗校验后的答案恢复被阻塞的 ask 工具调用。
  const handlePendingQuestionSubmit = React.useCallback(async (requestId: string, answers: string[][]) => {
    try {
      await replyPendingQuestion(requestId, answers);
      toast.success(i18n.t("workbenchAsk.submitted"));
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
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
    }
  }, [i18n.t]);

  React.useEffect(() => {
    draftConversationIdRef.current = draftConversation?.id ?? null;
  }, [draftConversation?.id]);

  // Release committed archive request locks only after the authoritative active row is no longer visible.
  // 仅在权威活动行不再可见后释放已提交归档请求锁。
  React.useEffect(() => {
    // activeConversationIds is the current lifecycle boundary rendered by the sidebar.
    // activeConversationIds 是侧栏当前渲染的生命周期边界。
    const activeConversationIds = new Set(workbenchState.conversations.map((conversation) => conversation.id));
    for (const conversationId of conversationArchivePendingIdsRef.current) {
      if (!activeConversationIds.has(conversationId)) {
        conversationArchivePendingIdsRef.current.delete(conversationId);
      }
    }
  }, [workbenchState.conversations]);

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

  React.useEffect(() => {
    const serviceInstanceChanged = resetAcceptedWorkbenchOrderingForServiceInstance(
      props.serviceInstanceEpoch,
    );
    if (!acceptServiceWorkbenchSnapshot(props.workbench)) {
      return;
    }
    // acceptedGlobalSettings keeps a later delta authoritative if it arrived before this snapshot effect ran.
    // acceptedGlobalSettings 在较新增量先于当前快照副作用到达时继续保持其权威性。
    const acceptedGlobalSettings = {
      vmm: Math.max(
        acceptedGlobalProjectSettingsRevisionByDomainRef.current.vmm,
        props.workbench.projectSettingsRevisions.global.vmm,
      ),
      agentSkills: Math.max(
        acceptedGlobalProjectSettingsRevisionByDomainRef.current.agentSkills,
        props.workbench.projectSettingsRevisions.global.agentSkills,
      ),
      codebaseMemory: Math.max(
        acceptedGlobalProjectSettingsRevisionByDomainRef.current.codebaseMemory,
        props.workbench.projectSettingsRevisions.global.codebaseMemory,
      ),
    };
    acceptedGlobalProjectSettingsRevisionByDomainRef.current = acceptedGlobalSettings;
    setGlobalProjectSettingsRevision(acceptedGlobalSettings);
    // acceptedProjectSettings applies every full-snapshot domain revision without lowering a newer focused delta.
    // acceptedProjectSettings 应用全量快照中的每个领域版本，同时不降低较新的聚焦增量。
    const acceptedProjectSettings: Record<string, ProjectSettingsRevision> = {};
    for (const [projectId, revisions] of Object.entries(props.workbench.projectSettingsRevisions.byProject)) {
      // acceptedRevisions merges two independently ordered domains for this exact project.
      // acceptedRevisions 合并当前准确项目的两个独立排序领域。
      const acceptedRevisions = {
        vmm: Math.max(
          acceptedProjectSettingsRevisionByKeyRef.current.get(`${projectId}:vmm`) ?? 0,
          revisions.vmm,
        ),
        agentSkills: Math.max(
          acceptedProjectSettingsRevisionByKeyRef.current.get(`${projectId}:agentSkills`) ?? 0,
          revisions.agentSkills,
        ),
        codebaseMemory: Math.max(
          acceptedProjectSettingsRevisionByKeyRef.current.get(`${projectId}:codebaseMemory`) ?? 0,
          revisions.codebaseMemory,
        ),
      };
      acceptedProjectSettingsRevisionByKeyRef.current.set(`${projectId}:vmm`, acceptedRevisions.vmm);
      acceptedProjectSettingsRevisionByKeyRef.current.set(
        `${projectId}:agentSkills`,
        acceptedRevisions.agentSkills,
      );
      acceptedProjectSettingsRevisionByKeyRef.current.set(
        `${projectId}:codebaseMemory`,
        acceptedRevisions.codebaseMemory,
      );
      acceptedProjectSettings[projectId] = acceptedRevisions;
    }
    setProjectSettingsRevisionById((current) => (
      serviceInstanceChanged
        ? acceptedProjectSettings
        : { ...current, ...acceptedProjectSettings }
    ));
    setWorkbenchState((current) => (
      mergeWorkbenchSnapshot(
        current,
        props.workbench,
        {
          allowServiceRevisionReset: serviceInstanceChanged,
          preserveExpandedProjectIds: expandedProjectIdsLocallyEditedRef.current,
        },
      )
    ));
    const nextSelected = props.workbench.selectedConversationId ?? props.workbench.conversations[0]?.id ?? null;
    setSelectedConversationId((current) => {
      if (current && current === draftConversationIdRef.current) {
        return current;
      }
      if (current && current === pendingMaterializedConversationIdRef.current) {
        return current;
      }
      if (current && props.workbench.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return nextSelected;
    });
  }, [
    acceptServiceWorkbenchSnapshot,
    props.serviceInstanceEpoch,
    props.workbench,
    resetAcceptedWorkbenchOrderingForServiceInstance,
  ]);

  React.useEffect(() => {
    if (!props.workbenchDelta) {
      return;
    }
    // signalSequence binds acknowledgement to the exact batch observed by this effect.
    // signalSequence 将确认操作绑定到当前 effect 观察到的精确批次。
    const signalSequence = props.workbenchDelta.seq;
    const acceptedEnvelopes: WorkbenchDeltaEnvelope[] = [];
    // acceptedProjectSettingsRevisions collects target-and-domain invalidations committed by this event batch.
    // acceptedProjectSettingsRevisions 收集当前事件批次提交的目标与领域失效版本。
    const acceptedProjectSettingsRevisions = new Map<string, ProjectSettingsRevision>();
    let nextAcceptedRevision = acceptedWorkbenchRevisionRef.current;
    for (const envelope of props.workbenchDelta.envelopes) {
      if (envelope.delta.kind === "mediaUploadProgressChanged") {
        // Upload progress still participates in global ordering so a terminal event cannot be overtaken by an older snapshot.
        // 上传进度仍参与全局排序，避免旧快照覆盖上传终态。
        if (!shouldAcceptWorkbenchRevision(nextAcceptedRevision, envelope.generation)) {
          continue;
        }
        const progress = envelope.delta.payload;
        setMediaUploadProgresses((current) => reduceMediaUploadProgress(current, progress));
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = envelope.generation;
        continue;
      }
      if (envelope.delta.kind === "conversationArchived") {
        // acceptedArchives filters this single target through its own lifecycle revision.
        // acceptedArchives 通过目标自身生命周期版本过滤此单个目标。
        const acceptedArchives = acceptConversationArchiveTransitions(
          [envelope.delta.payload],
          envelope.generation,
        );
        if (acceptedArchives.length > 0) {
          acceptedEnvelopes.push(envelope);
          nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        }
        continue;
      }
      if (envelope.delta.kind === "conversationsArchived") {
        // acceptedArchives independently filters every child target in the shared batch generation.
        // acceptedArchives 在共享批次 generation 中独立过滤每个子目标。
        const acceptedArchives = acceptConversationArchiveTransitions(
          envelope.delta.payload.archivedConversations,
          envelope.generation,
        );
        if (acceptedArchives.length > 0) {
          acceptedEnvelopes.push({
            ...envelope,
            delta: {
              kind: "conversationsArchived",
              payload: { archivedConversations: acceptedArchives },
            },
          });
          nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        }
        continue;
      }
      if (envelope.delta.kind === "projectRemoved") {
        // acceptedArchives filters child lifecycle targets independently from the project target.
        // acceptedArchives 独立于项目目标过滤子生命周期目标。
        const acceptedArchives = acceptConversationArchiveTransitions(
          envelope.delta.payload.archivedConversations,
          envelope.generation,
        );
        // projectAccepted orders removal only against the exact project lifecycle target.
        // projectAccepted 仅针对精确项目生命周期目标排序移除操作。
        const projectAccepted = acceptProjectRemovalTransition(
          envelope.delta.payload.projectId,
          envelope.generation,
        );
        if (projectAccepted) {
          acceptedEnvelopes.push({
            ...envelope,
            delta: {
              kind: "projectRemoved",
              payload: {
                ...envelope.delta.payload,
                archivedConversations: acceptedArchives,
              },
            },
          });
        } else if (acceptedArchives.length > 0) {
          acceptedEnvelopes.push({
            generation: envelope.generation,
            delta: {
              kind: "conversationsArchived",
              payload: { archivedConversations: acceptedArchives },
            },
          });
        }
        if (projectAccepted || acceptedArchives.length > 0) {
          nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        }
        continue;
      }
      if (envelope.delta.kind === "pluginCatalogChanged") {
        // One plugin catalog event owns all three revision gates so its domains are never applied partially.
        // 单个插件目录事件同时拥有三个版本闸门，确保其领域绝不被部分应用。
        const acceptedPluginCatalogRevision = Math.max(
          acceptedLuaSkillsRevisionRef.current,
          acceptedAgentSkillsRevisionRef.current,
          acceptedSubagentCatalogRevisionRef.current,
        );
        if (!shouldAcceptWorkbenchRevision(acceptedPluginCatalogRevision, envelope.generation)) {
          continue;
        }
        acceptedLuaSkillsRevisionRef.current = envelope.generation;
        acceptedAgentSkillsRevisionRef.current = envelope.generation;
        acceptedSubagentCatalogRevisionRef.current = envelope.generation;
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        continue;
      }
      if (envelope.delta.kind === "agentSkillsCatalogChanged") {
        // Agent Skills uses its own revision so an unrelated newer domain cannot suppress this catalog.
        // Agent Skills 使用独立版本，避免无关领域的较新版本压掉当前目录。
        if (!shouldAcceptWorkbenchRevision(acceptedAgentSkillsRevisionRef.current, envelope.generation)) {
          continue;
        }
        acceptedAgentSkillsRevisionRef.current = envelope.generation;
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        continue;
      }
      if (
        envelope.delta.kind === "luaSkillsCatalogChanged"
        || envelope.delta.kind === "luaSkillsManagedRuntimeProgressChanged"
      ) {
        // Every LuaSkills projection shares one domain gate so unrelated newer deltas cannot hide progress.
        // 所有 LuaSkills 投影共享同一领域闸门，避免无关的较新增量隐藏进度。
        if (!shouldAcceptWorkbenchRevision(acceptedLuaSkillsRevisionRef.current, envelope.generation)) {
          continue;
        }
        acceptedLuaSkillsRevisionRef.current = envelope.generation;
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        continue;
      }
      if (envelope.delta.kind === "subagentCatalogChanged") {
        // Subagent catalog uses one revision gate for broadcast events and initiating RPC responses.
        // 智能体目录让广播事件与发起方 RPC 响应共享同一版本闸门。
        if (!shouldAcceptWorkbenchRevision(acceptedSubagentCatalogRevisionRef.current, envelope.generation)) {
          continue;
        }
        acceptedSubagentCatalogRevisionRef.current = envelope.generation;
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        continue;
      }
      if (envelope.delta.kind === "projectSettingsChanged") {
        // projectId and domain own independent gates so unrelated settings never suppress each other.
        // projectId 与 domain 持有独立闸门，避免无关设置相互压制。
        const projectId = envelope.delta.payload.projectId;
        const domain = envelope.delta.payload.domain;
        if (projectId === null) {
          const acceptedRevision = acceptedGlobalProjectSettingsRevisionByDomainRef.current[domain];
          if (!shouldAcceptWorkbenchRevision(acceptedRevision, envelope.generation)) {
            continue;
          }
          acceptedGlobalProjectSettingsRevisionByDomainRef.current = {
            ...acceptedGlobalProjectSettingsRevisionByDomainRef.current,
            [domain]: envelope.generation,
          };
          setGlobalProjectSettingsRevision((current) => ({
            ...current,
            [domain]: envelope.generation,
          }));
          acceptedEnvelopes.push(envelope);
          nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
          continue;
        }
        const settingsKey = `${projectId}:${domain}`;
        const acceptedRevision = acceptedProjectSettingsRevisionByKeyRef.current.get(settingsKey) ?? 0;
        if (!shouldAcceptWorkbenchRevision(acceptedRevision, envelope.generation)) {
          continue;
        }
        acceptedProjectSettingsRevisionByKeyRef.current.set(settingsKey, envelope.generation);
        const acceptedDomains = acceptedProjectSettingsRevisions.get(projectId)
          ?? {
            vmm: acceptedProjectSettingsRevisionByKeyRef.current.get(`${projectId}:vmm`) ?? 0,
            agentSkills: acceptedProjectSettingsRevisionByKeyRef.current.get(`${projectId}:agentSkills`) ?? 0,
            codebaseMemory: acceptedProjectSettingsRevisionByKeyRef.current.get(`${projectId}:codebaseMemory`) ?? 0,
          };
        acceptedProjectSettingsRevisions.set(projectId, {
          ...acceptedDomains,
          [domain]: envelope.generation,
        });
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        continue;
      }
      if (!shouldAcceptWorkbenchRevision(nextAcceptedRevision, envelope.generation)) {
        continue;
      }
      acceptedEnvelopes.push(envelope);
      nextAcceptedRevision = envelope.generation;
    }
    if (acceptedProjectSettingsRevisions.size > 0) {
      setProjectSettingsRevisionById((current) => {
        // next exposes every independently accepted project revision in one React commit.
        // next 在一次 React 提交中暴露所有独立接收的项目版本。
        const next = { ...current };
        for (const [projectId, revisions] of acceptedProjectSettingsRevisions) {
          next[projectId] = revisions;
        }
        return next;
      });
    }
    if (acceptedEnvelopes.length === 0) {
      props.onWorkbenchDeltaConsumed(signalSequence);
      return;
    }
    acceptedWorkbenchRevisionRef.current = nextAcceptedRevision;
    setWorkbenchState((current) => (
      acceptedEnvelopes.reduce(
        (nextWorkbench, envelope) => applyWorkbenchDelta(nextWorkbench, envelope),
        current,
      )
    ));
    props.onWorkbenchDeltaConsumed(signalSequence);
  }, [
    acceptConversationArchiveTransitions,
    acceptProjectRemovalTransition,
    props.onWorkbenchDeltaConsumed,
    props.workbenchDelta,
  ]);

  // Complete draft materialization only after the authoritative sidebar row becomes visible.
  // 仅在权威侧栏行可见后完成草稿转正状态。
  React.useEffect(() => {
    const pendingConversationId = pendingMaterializedConversationIdRef.current;
    if (!pendingConversationId) {
      return;
    }
    if (workbenchState.conversations.some((conversation) => conversation.id === pendingConversationId)) {
      pendingMaterializedConversationIdRef.current = null;
      setDraftConversation((current) => retireMaterializedDraftSubmission(current, pendingConversationId));
    }
  }, [workbenchState.conversations]);


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
      return workbenchState.conversations[0]?.id ?? null;
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

  return (
    <ComposerMediaUploadProgressContext.Provider value={mediaUploadProgresses}>
    <div className={`app-shell ${showConsoleSidebar ? "with-console-sidebar" : "without-console-sidebar"} ${showWindowControls ? "tauri-shell" : "browser-shell"}`}>
      {floatingMediaUploadProgresses.length > 0 ? (
        <div className="media-upload-progress-stack">
          {floatingMediaUploadProgresses.map((progress) => (
            <MediaUploadProgressPanel
              key={mediaUploadProgressKey(progress)}
              progress={progress}
              onCancel={handleCancelConversationSubmission}
            />
          ))}
        </div>
      ) : null}
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
            sidebarOpen={navigationSidebarOpen}
            onSidebarOpenChange={setNavigationSidebarOpen}
            selectedDraftConversation={selectedDraftConversation}
            selectedConversation={selectedConversation}
            selectedConversationId={selectedConversationId}
            onNotificationPresentationContextChange={handleNotificationPresentationContextChange}
            chromeActions={shellChromeActions}
            titlebarDragRegion={windowDragRegionEnabled}
            onTitlebarDoubleClick={handleTitlebarDoubleClick}
            pinnedConversationIds={pinnedConversationIds}
            onConversationMenuAction={handleConversationMenuAction}
            onCreateConversation={handleCreateConversationForProject}
            onCreateProject={handleCreateProject}
            onOpenSkills={handleOpenWorkspaceSkills}
            onOpenAgentSkills={handleOpenWorkspaceAgentSkills}
            onOpenAgents={handleOpenWorkspaceAgents}
            onOpenCredentialManagement={() => {
              setActiveSettingsSubArea("credential-management")
              handleSelectTopFeatureArea("settings")
            }}
            onOpenMemory={handleOpenWorkspaceMemory}
            onOpenKnowledge={handleOpenWorkspaceKnowledge}
            onSelectArea={handleSelectTopFeatureArea}
            onSelectSkillSubArea={setActiveWorkbenchSkillSubArea}
            onSelectMemorySubArea={setActiveWorkbenchMemorySubArea}
            onExpandedProjectIdsChange={handleExpandedProjectIdsChange}
            onProjectMenuAction={handleWorkbenchProjectMenuAction}
            onReorderProjects={handleReorderProjects}
            onSelectProject={setActiveProjectId}
            onSelectConversation={handleSelectConversation}
            onSelectWorkbenchModel={handleSelectWorkbenchModel}
            onListWorkbenchModelSecondaryRoutes={handleListWorkbenchModelSecondaryRoutes}
            onSelectWorkbenchModelSecondaryRoute={handleSelectWorkbenchModelSecondaryRoute}
            onSelectWorkbenchReasoningEffort={handleSelectWorkbenchReasoningEffort}
            onSelectWorkbenchThinkingDisplayMode={handleSelectWorkbenchThinkingDisplayMode}
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
                  <span>{formatWorkbenchTranslation(i18n.t, "workspaceShell.version", { version: props.workbench.appVersion })}</span>
                  <div className="workspace-statusbar-todolist-status" ref={setTodolistStatusHost} />
                </div>
                <div className="workspace-statusbar-runtime-status" ref={setRuntimeStatusHost} />
                <RepositoryLink onOpenExternalUrl={openExternalUrl} />
              </footer>
            )}
            runtimeStatusHost={runtimeStatusHost}
            todolistStatusHost={todolistStatusHost}
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
                  toast.info("已保存输出风格。为避免打破上下文，风格切换只会在新会话或上下文压缩后的会话更新。", {
                    id: "output-style-safe-boundary",
                  });
                }
                return (await loadWorkbenchSnapshot()) ?? workbenchStateRef.current;
              }),
              onSaveGitCommitSettings: (request) => applySkillWorkbenchAction(async () => (await saveGitCommitSettings(request)) ?? workbenchStateRef.current),
              onSaveProviderRequestTraceSettings: (enabled) => applySkillWorkbenchAction(async () => (await saveProviderRequestTraceSettings(enabled)) ?? workbenchStateRef.current),
              onOpenSelectedProviderApiTraceDirectory: async () => {
                await openSelectedProviderApiTraceDirectory();
              },
              onClearSelectedProviderApiTraceDirectory: clearSelectedProviderApiTraceDirectory,
              onSaveProviderRetrySettings: (request) => applySkillWorkbenchAction(async () => (await saveProviderRetrySettings(request)) ?? workbenchStateRef.current),
              onSaveMcpServer: (request) => applySkillWorkbenchAction(async () => (await saveMcpServerConfiguration(request as unknown as Record<string, unknown>)) ?? workbenchStateRef.current),
              onRemoveMcpServer: (serverId) => applySkillWorkbenchAction(async () => (await removeMcpServerConfiguration(serverId)) ?? workbenchStateRef.current),
              onRestoreArchivedConversations: (conversationIds) => applySkillWorkbenchAction(async () => (await restoreArchivedConversations(conversationIds)) ?? workbenchStateRef.current),
              onDeleteArchivedConversations: (conversationIds) => applySkillWorkbenchAction(async () => (await deleteArchivedConversations(conversationIds)) ?? workbenchStateRef.current),
            }}
            startup={props.startup}
            workbench={workbenchState}
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
      <WorkbenchDialogs
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
            agentSkills: Math.max(
              globalProjectSettingsRevision.agentSkills,
              projectSettingsRevisionById[projectSettingsTarget.project.id]?.agentSkills ?? 0,
            ),
            codebaseMemory: Math.max(
              globalProjectSettingsRevision.codebaseMemory,
              projectSettingsRevisionById[projectSettingsTarget.project.id]?.codebaseMemory ?? 0,
            ),
          }
          : { vmm: 0, agentSkills: 0, codebaseMemory: 0 }}
        projectSettingsTarget={projectSettingsTarget}
        removeProjectSubmitting={removeProjectSubmitting}
        removeProjectTarget={removeProjectTarget}
        renameTarget={renameTarget}
        stopConversationAgents={stopConversationAgents}
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
      />
    </div>
    </ComposerMediaUploadProgressContext.Provider>
  );
}

// defaultDraftConversationForEmptyWorkbench creates a local draft when projects exist but sessions do not.
// defaultDraftConversationForEmptyWorkbench 在存在项目但没有会话时创建本地草稿。
// formatWorkbenchTranslation replaces named placeholders in localized workbench messages.
// formatWorkbenchTranslation 替换工作台本地化消息中的具名占位符。
function formatWorkbenchTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}

