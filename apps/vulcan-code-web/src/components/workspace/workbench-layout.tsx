import React from "react";
import {
  FeatureShell as ExtractedFeatureShell,
  primaryFeatureAreas as buildPrimaryFeatureAreas,
} from "@/components/features/feature-shell";
import {
  BrandHeader,
  FeatureNav,
  GlobalActions,
  RepositoryLink,
  StatusbarRuntimeActivities,
  WindowControls,
} from "@/components/shell/app-chrome";
import { StartupOverlay } from "@/components/shell/startup-screen";
import {
  type ModelPickerChoice,
} from "@/components/composer/model-picker-control";
import {
  normalizedComposerSlashCommandId,
  SLASH_SKILL_PROMPT,
  type ComposerSlashCommandResult,
  type ComposerSubmission,
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
import { WorkspaceShell } from "@/components/workspace/workspace-shell";
import { cn } from "@/lib/utils";
import { useI18n, type LocaleCode } from "@/i18n";
import type {
  AiCapabilityTargetSummary,
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
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";
import {
  closeSubagentPanel,
  compactSelectedConversationContext,
  editPendingPrompt,
  insertPendingPrompt,
  openSubagentPanel,
  rejectPendingQuestion,
  removePendingPrompt,
  reorderPendingPrompts,
  replyPendingQuestion,
  stageComposerAttachmentBytes,
  stopConversationRun,
  stopSubagentTask,
  submitConversationPrompt,
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
  saveAiCapabilityTargets,
  saveAiCapabilityVendor,
  saveMcpServerConfiguration,
  saveProviderConfiguration,
  saveProviderEnabledModels,
  saveProviderModelOverride,
  selectWorkbenchModel,
  selectWorkbenchReasoningEffort,
  setAiCapabilityTargetEnabled,
} from "@/service/settings-actions-service";
import {
  installLuaSkill,
  installSystemAgentSkills,
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
  loadConversationMessages,
  loadWorkbenchSnapshot,
  loadWorkspaceDiffFileDetail,
  loadWorkspaceDiffSearch,
  loadWorkspaceDiffSnapshot,
} from "@/service/workbench-snapshot-service";
import { shellClient } from "@/service/webui-service-runtime";
import { localMediaSourceUrl } from "@/resources/localMedia";
import {
  applySubagentRoutePreference,
  applyWorkbenchDeltaWithSubagentSurfaceMessageCache,
  canOpenCachedSubagentSurface,
  closeSubagentSurfaceForConversation,
  coalesceWorkbenchDeltaEnvelopes,
  injectCachedSubagentSurfaceMessages,
  markConversationLocallyRead,
  mergeConversationMessagesWithSubagentSurfaceMessageCache,
  mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache,
  openSubagentSurfaceForConversation,
  reorderPendingPromptSummaries,
  workbenchDeltaConversationId,
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

// ThemeMode describes the two shadcn default theme modes.
// ThemeMode 描述 shadcn 默认的两种主题模式。
type ThemeMode = "light" | "dark";

// WorkbenchDeltaSignal wraps the newest partial update with a React-visible sequence.
// WorkbenchDeltaSignal 使用 React 可见序号包装最新局部更新。
export interface WorkbenchDeltaSignal {
  // Monotonic frontend sequence that makes repeated delta objects observable.
  // 前端单调序号，用于让重复增量对象也能被观察到。
  seq: number;
  // Backend partial workbench update envelopes batched for one render frame.
  // 单个渲染帧内批处理的后端局部工作台更新信封。
  envelopes: WorkbenchDeltaEnvelope[];
}
// WorkbenchLayout renders the GUI-like workbench shell.
// WorkbenchLayout 渲染接近 GUI 的工作台外壳。
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
  // localizedPrimaryFeatureAreas stores the translated top navigation metadata.
  // localizedPrimaryFeatureAreas 保存已翻译的顶部导航元数据。
  const localizedPrimaryFeatureAreas = React.useMemo(() => buildPrimaryFeatureAreas(i18n.t), [i18n.t]);
  // localizedSettingsFeatureArea stores the translated settings metadata.
  // localizedSettingsFeatureArea 保存已翻译的设置元数据。
  const localizedSettingsFeatureArea = React.useMemo<FeatureAreaSummary>(() => ({
    id: "settings",
    label: i18n.t("area.settings"),
    description: i18n.t("area.settings.description"),
    status: i18n.t("feature.status.preview"),
  }), [i18n]);
  // activeArea stores the currently selected feature area.
  // activeArea 保存当前选中的功能区域。
  const [activeArea, setActiveArea] = React.useState<MainAreaId>("chat");
  // theme stores the current shadcn visual theme.
  // theme 保存当前 shadcn 视觉主题。
  const [theme, setTheme] = React.useState<ThemeMode>(() => initialThemeMode());
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
  // mergeConversationMessagesWithLocalCaches centralizes lazy message hydration cache updates.
  // mergeConversationMessagesWithLocalCaches 集中处理懒加载正文时的缓存更新。
  const mergeConversationMessagesWithLocalCaches = React.useCallback((
    current: WorkbenchSnapshot,
    conversationId: string,
    messages: ConversationMessage[],
  ) => mergeConversationMessagesWithSubagentSurfaceMessageCache(
    current,
    conversationId,
    messages,
    subagentSurfaceMessageCacheRef.current,
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
  // loadingConversationId stores the conversation currently loading from the service.
  // loadingConversationId 保存当前正在从服务加载正文的会话标识。
  const [loadingConversationId, setLoadingConversationId] = React.useState<string | null>(null);
  // conversationLoadError stores the latest conversation hydration error.
  // conversationLoadError 保存最近一次会话正文水合错误。
  const [conversationLoadError, setConversationLoadError] = React.useState<string | null>(null);
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
  // hydratedConversationIds stores conversations already requested from the service.
  // hydratedConversationIds 保存已经向服务请求过正文的会话标识。
  const [hydratedConversationIds, setHydratedConversationIds] = React.useState<Set<string>>(
    () => new Set(),
  );
  // activeFeature stores the selected feature summary, including settings opened from the global button.
  // activeFeature 保存当前选中的功能区域摘要，并包含从全局按钮打开的设置页。
  const activeFeature = activeArea === "settings"
    ? localizedSettingsFeatureArea
    : localizedPrimaryFeatureAreas.find((area) => area.id === activeArea) ?? localizedPrimaryFeatureAreas[0];
  // selectedConversation stores the conversation used by the console header.
  // selectedConversation 保存控制台标题栏使用的当前会话。
  const selectedConversation = workbenchState.conversations.find((conversation) => conversation.id === selectedConversationId);
  // selectedConversationMessages stores the current parent conversation message window.
  // selectedConversationMessages 保存当前父会话消息窗口。
  const selectedConversationMessages = React.useMemo(
    () => workbenchState.messages.filter((message) => message.conversationId === selectedConversationId),
    [selectedConversationId, workbenchState.messages],
  );
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
  // showWindowControls enables the custom titlebar only inside the Tauri desktop shell.
  // showWindowControls 仅在 Tauri 桌面壳内启用自定义标题栏。
  const showWindowControls = shellClient.supportsWindowControls();
  // isWindowMaximized tracks the current Tauri window maximize state for the titlebar button.
  // isWindowMaximized 跟踪当前 Tauri 窗口最大化状态，用于标题栏按钮。
  const [isWindowMaximized, setIsWindowMaximized] = React.useState(false);
  // refreshWindowMaximizedState synchronizes the local titlebar state with Tauri.
  // refreshWindowMaximizedState 将本地标题栏状态与 Tauri 同步。
  const refreshWindowMaximizedState = React.useCallback(async () => {
    setIsWindowMaximized(await shellClient.isWindowMaximized());
  }, []);
  // toggleWindowMaximize toggles native maximize state and refreshes the icon.
  // toggleWindowMaximize 切换原生最大化状态并刷新图标。
  const toggleWindowMaximize = React.useCallback(async () => {
    await shellClient.toggleWindowMaximize();
    window.setTimeout(() => void refreshWindowMaximizedState(), 40);
  }, [refreshWindowMaximizedState]);
  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }
    void refreshWindowMaximizedState();
  }, [refreshWindowMaximizedState, showWindowControls]);

  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }

    let cancelled = false;
    let unlisten: null | (() => void) = null;

    // subscribeWindowResize keeps the restore icon aligned when native drag-region double click changes state.
    // subscribeWindowResize 在原生拖动区域双击改变窗口状态后同步还原图标。
    async function subscribeWindowResize() {
      if (cancelled) {
        return;
      }
      unlisten = await shellClient.subscribeWindowResize(() => {
        void refreshWindowMaximizedState();
      });
    }

    void subscribeWindowResize();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refreshWindowMaximizedState, showWindowControls]);

  // applyProjectActionSnapshot merges a project-menu operation result into the local workbench.
  // applyProjectActionSnapshot 将项目菜单操作结果合并进本地工作台。
  const applyProjectActionSnapshot = React.useCallback((snapshot: WorkbenchSnapshot | null) => {
    if (!snapshot) {
      return;
    }
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    setDraftConversation(null);
    setSelectedConversationId((current) => {
      if (current && snapshot.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return snapshot.selectedConversationId ?? snapshot.conversations[0]?.id ?? null;
    });
  }, []);

  // handleProjectMenuAction executes immediate project-tree context menu commands.
  // handleProjectMenuAction 执行可立即处理的项目树右键菜单命令。
  const handleProjectMenuAction = React.useCallback(async (action: ProjectMenuAction, project: ProjectSummary) => {
    try {
      if (action === "open-directory") {
        const message = await openProjectDirectory(project.id);
        toast.success(message ?? `已请求打开项目目录：${project.name}`);
        return;
      }

      if (action === "rename") {
        setRenameTarget({ kind: "project", id: project.id, name: project.name });
        return;
      }

      if (action === "set-icon") {
        setProjectIconTarget({ project });
        return;
      }

      if (action === "archive-conversations") {
        setArchiveProjectTarget(project);
        return;
      }

      if (action === "remove-project") {
        setRemoveProjectTarget(project);
        return;
      }

      if (action === "settings") {
        setProjectSettingsTarget({ project });
        return;
      }

      const preview = applyPreviewProjectAction(workbenchState, project.id, action);
      applyProjectActionSnapshot(preview.snapshot);
      await toggleProjectPinned(project.id);
      toast.success(project.pinned ? `已取消置顶项目：${project.name}` : `已置顶项目：${project.name}`);
    } catch (nextError) {
      applyProjectActionSnapshot(workbenchState);
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleProjectSettingsVmmSave persists the project-scoped VMM settings draft.
  // handleProjectSettingsVmmSave 持久化项目级 VMM 设置草稿。
  const handleProjectSettingsVmmSave = React.useCallback(async (projectId: string, draft: VmmConfigDraftSummary) => {
    await applySkillWorkbenchAction(() => saveVmmMemoryConfig({ scope: "project", projectId, draft }));
    toast.success("已保存项目 VMM 设置。");
  }, [applySkillWorkbenchAction]);

  // handleProjectSettingsAgentSkillSave persists one project Agent Skill override.
  // handleProjectSettingsAgentSkillSave 持久化单个项目 Agent Skill 覆盖。
  const handleProjectSettingsAgentSkillSave = React.useCallback(async (projectId: string, name: string, stateKey: ProjectAgentSkillState) => {
    await setAgentSkillProjectState(projectId, name, stateKey);
    toast.success(projectAgentSkillSaveMessage(name, stateKey));
  }, []);

  // handleSetAgentSkillsEnabled persists the global Agent Skills switch with a lightweight catalog refresh.
  // handleSetAgentSkillsEnabled 使用轻量目录刷新持久化 Agent Skills 全局开关。
  const handleSetAgentSkillsEnabled = React.useCallback(async (enabled: boolean) => {
    const previousAgentSkills = workbenchState.agentSkills;
    applyAgentSkillsCatalog({
      ...previousAgentSkills,
      globalEnabled: enabled,
      skills: previousAgentSkills.skills.map((skill) => ({ ...skill, enabled: enabled && skill.enabled })),
    });
    try {
      applyAgentSkillsCatalog(await setAgentSkillsEnabled(enabled));
    } catch (error) {
      applyAgentSkillsCatalog(previousAgentSkills);
      throw error;
    }
  }, [applyAgentSkillsCatalog, workbenchState.agentSkills]);

  // handleSetAgentSkillEnabled persists one Agent Skill state with a lightweight catalog refresh.
  // handleSetAgentSkillEnabled 使用轻量目录刷新持久化单个 Agent Skill 状态。
  const handleSetAgentSkillEnabled = React.useCallback(async (name: string, enabled: boolean) => {
    const previousAgentSkills = workbenchState.agentSkills;
    applyAgentSkillsCatalog({
      ...previousAgentSkills,
      skills: previousAgentSkills.skills.map((skill) => (
        skill.name === name ? { ...skill, enabled: enabled && previousAgentSkills.globalEnabled } : skill
      )),
    });
    try {
      applyAgentSkillsCatalog(await setAgentSkillEnabled(name, enabled));
    } catch (error) {
      applyAgentSkillsCatalog(previousAgentSkills);
      throw error;
    }
  }, [applyAgentSkillsCatalog, workbenchState.agentSkills]);

  // handleProjectIconConfirm persists the selected project icon override.
  // handleProjectIconConfirm 持久化选择的项目图标覆盖。
  const handleProjectIconConfirm = React.useCallback(async (project: ProjectSummary, icon: ProjectIconSelection) => {
    const normalizedIcon = normalizeProjectIconSelection(icon);
    try {
      applyProjectActionSnapshot(applyPreviewProjectIcon(workbenchState, project.id, normalizedIcon).snapshot);
      await setProjectIcon(project.id, normalizedIcon);
      setProjectIconTarget(null);
      toast.success(`已更新项目图标：${project.name}`);
    } catch (error) {
      applyProjectActionSnapshot(workbenchState);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleProjectIconReset clears the selected project icon override.
  // handleProjectIconReset 清除选择的项目图标覆盖。
  const handleProjectIconReset = React.useCallback(async (project: ProjectSummary) => {
    try {
      applyProjectActionSnapshot(applyPreviewProjectIcon(workbenchState, project.id, null).snapshot);
      await resetProjectIcon(project.id);
      setProjectIconTarget(null);
      toast.success(`已还原项目图标：${project.name}`);
    } catch (error) {
      applyProjectActionSnapshot(workbenchState);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, [applyProjectActionSnapshot, workbenchState]);

  // handleCreateConversation creates and selects a new empty conversation for a project.
  // handleCreateConversation 为项目创建并选中一个新的空白会话。
  const handleCreateConversation = React.useCallback(async (project: ProjectSummary) => {
    const draftId = `draft-${project.id}-${Date.now()}`;
    setDraftConversation({
      id: draftId,
      projectId: project.id,
      projectName: project.name,
    });
    setSelectedConversationId(draftId);
    setConversationLoadError(null);
    setLoadingConversationId(null);
  }, []);

  // handleCreateProject opens the service-backed project folder picker.
  // handleCreateProject 打开服务驱动的项目文件夹选择器。
  const handleCreateProject = React.useCallback(() => {
    setProjectFolderPickerOpen(true);
  }, []);

  // handleConfirmProjectFolder adds the selected service-host directory as a project.
  // handleConfirmProjectFolder 将选中的服务宿主目录添加为项目。
  const handleConfirmProjectFolder = React.useCallback(async (path: string) => {
    const snapshot = await addProjectFromPath(path) ?? await loadWorkbenchSnapshot();
    applyProjectActionSnapshot(snapshot);
    toast.success("项目列表已更新。");
  }, [applyProjectActionSnapshot]);

  // handleReorderProjects persists a same-section project order using the existing order model.
  // handleReorderProjects 使用现有顺序模型持久化同一分区内的项目排序。
  const handleReorderProjects = React.useCallback(async (projectIds: string[]) => {
    setWorkbenchState((current) => applyPreviewProjectOrder(current, projectIds).snapshot);
    try {
      const snapshot = await reorderProjects(projectIds);
      if (snapshot) {
        applyProjectActionSnapshot(snapshot);
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      try {
        const snapshot = await loadWorkbenchSnapshot();
        applyProjectActionSnapshot(snapshot);
      } catch {
        // Keep the optimistic order when the refresh fallback is unavailable.
        // 刷新回退不可用时保留乐观排序结果。
      }
    }
  }, [applyProjectActionSnapshot]);

  // handleExpandedProjectIdsChange persists project folder expansion without reapplying stale snapshots.
  // handleExpandedProjectIdsChange 持久化项目文件夹展开状态，但不重新套用可能过期的快照。
  const handleExpandedProjectIdsChange = React.useCallback((projectIds: string[]) => {
    expandedProjectIdsLocallyEditedRef.current = true;
    setWorkbenchState((current) => ({ ...current, expandedProjectIds: projectIds }));
    void setExpandedTaskProjects(projectIds).catch((nextError) => {
      console.warn("persist expanded projects failed", nextError);
    });
  }, []);

  // handleSubmitConversationPrompt sends the composer text through the shared backend runtime.
  // handleSubmitConversationPrompt 通过共享后端运行时发送输入区文本。
  const handleSubmitConversationPrompt = React.useCallback(async (submission: ComposerSubmission): Promise<boolean> => {
    // submittedSelectionId captures the sidebar choice that owned this submission.
    // submittedSelectionId 捕获发起本次提交时所属的侧栏选择。
    const submittedSelectionId = selectedConversationId;
    const activeProjectId =
      selectedDraftConversation?.projectId
      ?? selectedConversation?.projectId
      ?? workbenchState.projects[0]?.id
      ?? null;
    const activeConversationId = selectedDraftConversation
      ? null
      : selectedConversation?.id ?? null;
    try {
      const snapshot = await submitConversationPrompt(
        activeProjectId,
        activeConversationId,
        submission.prompt,
        submission.attachments,
        submission.executionMode,
        submission.reasoningEffort,
        submission.disabledAiCapabilityKinds,
        submission.disabledAiCapabilityTargetSelectors,
        submission.aiCapabilityTargetOrderOverrides,
      );
      if (!snapshot) {
        toast.info("浏览器预览模式下无法发送真实会话。");
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      setDraftConversation(null);
      const nextConversationId = snapshot.selectedConversationId ?? activeConversationId;
      if (nextConversationId) {
        setHydratedConversationIds((current) => new Set(current).add(nextConversationId));
        if (selectedConversationIdRef.current === submittedSelectionId) {
          setSelectedConversationId(nextConversationId);
        }
      }
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, [selectedConversation, selectedConversationId, selectedDraftConversation, workbenchState.projects]);

  // handleEditPendingPrompt restores one queued prompt back into the active composer.
  // handleEditPendingPrompt 将单条排队提示词恢复到当前输入区。
  const handleEditPendingPrompt = React.useCallback(async (entryId: string): Promise<PendingPromptSummary | null> => {
    try {
      const pendingPrompt = await editPendingPrompt(entryId);
      if (!pendingPrompt) {
        return null;
      }
      const snapshot = await loadWorkbenchSnapshot();
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      return pendingPrompt;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return null;
    }
  }, []);

  // handleInsertPendingPrompt routes one queued prompt through the shared backend queue router.
  // handleInsertPendingPrompt 通过共享后端队列路由引导单条排队提示词。
  const handleInsertPendingPrompt = React.useCallback(async (entryId: string): Promise<void> => {
    webuiClickTrace("pending_prompt.insert.invoke", { entryId });
    try {
      const snapshot = await insertPendingPrompt(entryId);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, []);

  // handleStopConversationRun stops the active backend conversation stream.
  // handleStopConversationRun 停止当前活跃的后端会话流。
  const handleStopConversationRun = React.useCallback(async (): Promise<boolean> => {
    webuiClickTrace("conversation.stop.invoke");
    try {
      const snapshot = await stopConversationRun();
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
      return Boolean(snapshot);
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, []);

  // handleCompactSelectedConversationContext starts manual compaction from slash commands.
  // handleCompactSelectedConversationContext 从斜杠指令启动手动上下文压缩。
  const handleCompactSelectedConversationContext = React.useCallback(async (): Promise<boolean> => {
    webuiClickTrace("conversation.compact.invoke");
    try {
      const snapshot = await compactSelectedConversationContext();
      if (!snapshot) {
        toast.info("浏览器预览模式下无法执行真实上下文压缩。");
        return false;
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, []);

  // handleOpenSubagentPanel asks the backend to display one child task surface and reports whether it succeeded.
  // handleOpenSubagentPanel 请求后端显示单个子任务界面，并返回是否打开成功。
  const handleOpenSubagentPanel = React.useCallback(async (parentConversationId: string, taskId: string): Promise<boolean> => {
    const routeKey = `${parentConversationId}:${taskId}`;
    const existingRequest = inflightSubagentOpenRequestRef.current;
    if (canOpenCachedSubagentSurface(
      workbenchStateRef.current,
      subagentSurfaceMessageCacheRef.current,
      parentConversationId,
      taskId,
    )) {
      subagentRoutePreferenceRef.current = { parentConversationId, taskId };
      webuiClickTrace("subagent.open.local_cache", { parentConversationId, taskId, routeKey });
      setWorkbenchState((current) => openSubagentSurfaceForConversation(
        injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current),
        parentConversationId,
        taskId,
      ));
      return true;
    }
    if (existingRequest?.key === routeKey) {
      subagentRoutePreferenceRef.current = { parentConversationId, taskId };
      webuiClickTrace("subagent.open.coalesced", { parentConversationId, taskId, routeKey });
      setWorkbenchState((current) => openSubagentSurfaceForConversation(
        injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current),
        parentConversationId,
        taskId,
      ));
      return true;
    }
    const requestSeq = subagentNavigationRequestSeqRef.current + 1;
    subagentNavigationRequestSeqRef.current = requestSeq;
    subagentRoutePreferenceRef.current = { parentConversationId, taskId };
    webuiClickTrace("subagent.open.invoke", { parentConversationId, taskId, requestSeq, routeKey });
    setWorkbenchState((current) => openSubagentSurfaceForConversation(
      injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current),
      parentConversationId,
      taskId,
    ));
    const requestPromise = openSubagentPanel(parentConversationId, taskId);
    inflightSubagentOpenRequestRef.current = { key: routeKey, promise: requestPromise };
    try {
      const snapshot = await requestPromise;
      if (inflightSubagentOpenRequestRef.current?.promise === requestPromise) {
        inflightSubagentOpenRequestRef.current = null;
      }
      if (snapshot) {
        if (subagentNavigationRequestSeqRef.current !== requestSeq) {
          webuiClickTrace("subagent.open.stale_result", {
            parentConversationId,
            taskId,
            requestSeq,
            latestRequestSeq: subagentNavigationRequestSeqRef.current,
          });
          return true;
        }
        setWorkbenchState((current) => applySubagentRoutePreference(
          mergeWorkbenchWithLocalCaches(current, snapshot),
          subagentRoutePreferenceRef.current,
        ));
        setHydratedConversationIds((current) => new Set(current).add(snapshot.subagentNavigation?.activeConversationId ?? taskId));
      }
      webuiClickTrace("subagent.open.result", {
        parentConversationId,
        taskId,
        requestSeq,
        opened: Boolean(snapshot),
        activeConversationId: snapshot?.subagentNavigation?.activeConversationId ?? null,
        activeTaskId: snapshot?.subagentNavigation?.activeTaskId ?? null,
      });
      return Boolean(snapshot);
    } catch (nextError) {
      if (inflightSubagentOpenRequestRef.current?.promise === requestPromise) {
        inflightSubagentOpenRequestRef.current = null;
      }
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      webuiClickTrace("subagent.open.error", { parentConversationId, taskId, message });
      if (message.includes("不可恢复") || message.includes("没有找到可打开的子代理任务")) {
        return false;
      }
      toast.error(message);
      return false;
    }
  }, []);

  // handleCloseSubagentPanel asks the backend to return to the parent conversation surface.
  // handleCloseSubagentPanel 请求后端返回父会话界面。
  const handleCloseSubagentPanel = React.useCallback(async (): Promise<void> => {
    const parentConversationId = selectedConversationIdRef.current;
    inflightSubagentOpenRequestRef.current = null;
    const requestSeq = subagentNavigationRequestSeqRef.current + 1;
    subagentNavigationRequestSeqRef.current = requestSeq;
    if (parentConversationId) {
      subagentRoutePreferenceRef.current = { parentConversationId, taskId: null };
      setWorkbenchState((current) => closeSubagentSurfaceForConversation(current, parentConversationId));
    }
    webuiClickTrace("subagent.close.invoke", { parentConversationId, requestSeq });
    try {
      const snapshot = await closeSubagentPanel();
      if (snapshot) {
        if (subagentNavigationRequestSeqRef.current !== requestSeq) {
          webuiClickTrace("subagent.close.stale_result", {
            parentConversationId,
            requestSeq,
            latestRequestSeq: subagentNavigationRequestSeqRef.current,
          });
          return;
        }
        setWorkbenchState((current) => applySubagentRoutePreference(
          mergeWorkbenchWithLocalCaches(current, snapshot),
          subagentRoutePreferenceRef.current,
        ));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, []);

  // handleStopSubagentTask asks the backend to stop one running child task.
  // handleStopSubagentTask 请求后端停止单个运行中的子任务。
  const handleStopSubagentTask = React.useCallback(async (parentConversationId: string, taskId: string): Promise<void> => {
    webuiClickTrace("subagent.stop.invoke", { parentConversationId, taskId });
    try {
      const snapshot = await stopSubagentTask(parentConversationId, taskId);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, []);

  // handleRemovePendingPrompt removes one queued prompt through the shared service queue.
  // handleRemovePendingPrompt 通过共享服务队列移除单条排队提示词。
  const handleRemovePendingPrompt = React.useCallback(async (entryId: string): Promise<void> => {
    try {
      const snapshot = await removePendingPrompt(entryId);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, []);

  // handleReorderPendingPrompts persists one visible pending prompt order update.
  // handleReorderPendingPrompts 持久化一次可见待发送提示词排序更新。
  const handleReorderPendingPrompts = React.useCallback(async (
    entryIds: string[],
    draggedEntryId: string,
    targetEntryId: string | null,
  ): Promise<void> => {
    setWorkbenchState((current) => ({ ...current, pendingPrompts: reorderPendingPromptSummaries(current.pendingPrompts ?? [], entryIds) }));
    try {
      const snapshot = await reorderPendingPrompts(selectedConversationIdRef.current, draggedEntryId, targetEntryId);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      const snapshot = await loadWorkbenchSnapshot();
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
  }, []);

  // handleSelectWorkbenchModel persists one model picker choice through the shared service.
  // handleSelectWorkbenchModel 通过共享服务持久化单个模型筛选器选择。
  const handleSelectWorkbenchModel = React.useCallback(async (choice: ModelPickerChoice): Promise<boolean> => {
    try {
      const snapshot = await selectWorkbenchModel(choice);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
        toast.success(`当前模型已切换为 ${choice.modelName}（${choice.providerName}）。`);
      }
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, []);

  // handleSelectWorkbenchReasoningEffort persists one reasoning-effort choice through the shared service router.
  // handleSelectWorkbenchReasoningEffort 通过共享服务路由持久化单个思考档位选择。
  const handleSelectWorkbenchReasoningEffort = React.useCallback(async (effort: string): Promise<boolean> => {
    try {
      const snapshot = await selectWorkbenchReasoningEffort(effort);
      if (snapshot) {
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      }
      return true;
    } catch (nextError) {
      toast.error(nextError instanceof Error ? nextError.message : String(nextError));
      return false;
    }
  }, []);

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
  }, []);

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
    document.documentElement.classList.toggle("dark", theme === "dark");
    document.documentElement.style.colorScheme = theme;
    window.localStorage.setItem("vulcan-webui-theme", theme);
  }, [theme]);

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
    const conversationIds = props.workbenchDelta.envelopes
      .map(workbenchDeltaConversationId)
      .filter((conversationId): conversationId is string => Boolean(conversationId));
    if (conversationIds.length > 0) {
      setHydratedConversationIds((current) => {
        const next = new Set(current);
        for (const conversationId of conversationIds) {
          next.add(conversationId);
        }
        return next;
      });
    }
  }, [props.workbenchDelta]);

  React.useEffect(() => {
    if (!selectedConversationId) {
      return;
    }
    if (selectedConversationId === draftConversation?.id) {
      return;
    }
    const conversationId = selectedConversationId;
    if (hydratedConversationIds.has(conversationId)) {
      return;
    }

    let cancelled = false;
    setLoadingConversationId(conversationId);
    setConversationLoadError(null);

    // hydrateConversation loads the selected conversation without reloading the whole workbench.
    // hydrateConversation 在不重载整个工作台的情况下加载当前选中会话正文。
    async function hydrateConversation() {
      try {
        const messages = await loadConversationMessages(conversationId);
        if (cancelled) {
          return;
        }
        setWorkbenchState((current) => mergeConversationMessagesWithLocalCaches(current, conversationId, messages));
        setHydratedConversationIds((current) => new Set(current).add(conversationId));
      } catch (nextError) {
        if (!cancelled) {
          setConversationLoadError(nextError instanceof Error ? nextError.message : String(nextError));
        }
      } finally {
        if (!cancelled) {
          setLoadingConversationId(null);
        }
      }
    }

    void hydrateConversation();
    return () => {
      cancelled = true;
    };
  }, [draftConversation?.id, hydratedConversationIds, selectedConversationId]);

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
        setActiveArea("skills");
        return { handled: true };
      case "compact":
        void handleCompactSelectedConversationContext();
        return { handled: true };
      default:
        return { handled: false };
    }
  }, [handleCompactSelectedConversationContext]);

  return (
    <div className={`app-shell ${showConsoleSidebar ? "with-console-sidebar" : "without-console-sidebar"} ${showWindowControls ? "tauri-shell" : "browser-shell"}`}>
      <header
        className="main-menu"
        aria-label="主菜单"
      >
        <BrandHeader dragRegion={showWindowControls} />
        <FeatureNav
          areas={localizedPrimaryFeatureAreas}
          activeArea={activeArea}
          onSelect={setActiveArea}
        />
        {showWindowControls ? <div className="titlebar-drag-region" data-tauri-drag-region /> : null}
        <GlobalActions
          activeArea={activeArea}
          startup={props.startup}
          theme={theme}
          locale={i18n.locale}
          onChangeLocale={i18n.setLocale}
          onToggleTheme={() => setTheme((current) => current === "dark" ? "light" : "dark")}
          onOpenSettings={() => setActiveArea("settings")}
        />
        {showWindowControls ? (
          <WindowControls
            isMaximized={isWindowMaximized}
            onMaximizeToggle={toggleWindowMaximize}
            onMaximizeStateRefresh={refreshWindowMaximizedState}
          />
        ) : null}
      </header>
      <main className="workspace">
        {showConsoleSidebar ? (
          <WorkspaceShell
            startup={props.startup}
            workbench={consoleWorkbenchState}
            selectedDraftConversation={selectedDraftConversation}
            selectedConversation={selectedConversation}
            selectedConversationId={selectedConversationId}
            loadingConversationId={loadingConversationId}
            conversationLoadError={conversationLoadError}
            pinnedConversationIds={pinnedConversationIds}
            onConversationMenuAction={handleConversationMenuAction}
            onCreateConversation={handleCreateConversation}
            onCreateProject={handleCreateProject}
            onExpandedProjectIdsChange={handleExpandedProjectIdsChange}
            onProjectMenuAction={handleProjectMenuAction}
            onReorderProjects={handleReorderProjects}
            onSelectConversation={handleSelectConversation}
            onSelectWorkbenchModel={handleSelectWorkbenchModel}
            onSelectWorkbenchReasoningEffort={handleSelectWorkbenchReasoningEffort}
            onSlashCommand={handleComposerSlashCommand}
            onSubmitPrompt={handleSubmitConversationPrompt}
            onStopConversationRun={handleStopConversationRun}
            onOpenSubagentPanel={handleOpenSubagentPanel}
            onCloseSubagentPanel={handleCloseSubagentPanel}
            onStopSubagentTask={handleStopSubagentTask}
            onEditPendingPrompt={handleEditPendingPrompt}
            onInsertPendingPrompt={handleInsertPendingPrompt}
            onRemovePendingPrompt={handleRemovePendingPrompt}
            onReorderPendingPrompts={handleReorderPendingPrompts}
            onLoadWorkspaceDiffFileDetail={loadWorkspaceDiffFileDetail}
            onLoadWorkspaceDiffSearch={loadWorkspaceDiffSearch}
            onLoadWorkspaceDiffSnapshot={loadWorkspaceDiffSnapshot}
            onStageAttachmentBytes={stageComposerAttachmentBytes}
            onCopyText={copyText}
            onLoadToolDetail={loadCachedConversationToolDetail}
            resolveMediaSourceUrl={localMediaSourceUrl}
            onTrace={webuiClickTrace}
          />
        ) : (
          <ExtractedFeatureShell
            activeArea={activeArea}
            activeFeature={activeFeature}
            activeSettingsSubArea={activeSettingsSubArea}
            onSelectSettingsSubArea={setActiveSettingsSubArea}
            skillActions={{
              onRefreshLuaSkills: () => applySkillWorkbenchAction(refreshLuaSkillsCatalog),
              onInstallLuaSkill: (skillId, source, sourceType) => applySkillWorkbenchAction(() => installLuaSkill(skillId, source, sourceType)),
              onUpdateLuaSkill: (skillId) => applySkillWorkbenchAction(() => updateLuaSkill(skillId)),
              onSetLuaSkillEnabled: (skillId, enabled) => applySkillWorkbenchAction(() => setLuaSkillEnabled(skillId, enabled)),
              onUninstallLuaSkill: (skillId) => applySkillWorkbenchAction(() => uninstallLuaSkill(skillId)),
              onSetLuaSkillToolRegistrationState: (skillId, localName, registrationState) => applySkillWorkbenchAction(() => setLuaSkillToolRegistrationState(skillId, localName, registrationState)),
              onRefreshAgentSkills: () => applySkillWorkbenchAction(refreshAgentSkillsCatalog),
              onInstallSystemAgentSkills: () => applySkillWorkbenchAction(installSystemAgentSkills),
              onSetAgentSkillsEnabled: handleSetAgentSkillsEnabled,
              onSetAgentSkillEnabled: handleSetAgentSkillEnabled,
            }}
            memoryActions={{
              onRunVmmAction: runVmmMemoryAction,
              onSaveVmmConfig: (request) => applySkillWorkbenchAction(() => saveVmmMemoryConfig(request)),
            }}
            settingsActions={{
              onSaveProvider: (request) => applySkillWorkbenchAction(async () => (await saveProviderConfiguration(request)) ?? workbenchStateRef.current),
              onRemoveProvider: (providerId) => applySkillWorkbenchAction(async () => (await removeProviderConfiguration(providerId)) ?? workbenchStateRef.current),
              onSaveProviderEnabledModels: (request) => applySkillWorkbenchAction(async () => (await saveProviderEnabledModels(request)) ?? workbenchStateRef.current),
              onSaveProviderModelOverride: (request) => applySkillWorkbenchAction(async () => (await saveProviderModelOverride(request)) ?? workbenchStateRef.current),
              onSaveAiCapabilityVendor: (request) => applySkillWorkbenchAction(async () => (await saveAiCapabilityVendor(request)) ?? workbenchStateRef.current),
              onRemoveAiCapabilityVendor: (vendorId) => applySkillWorkbenchAction(async () => (await removeAiCapabilityVendor(vendorId)) ?? workbenchStateRef.current),
              onSetAiCapabilityTargetEnabled: (request) => applySkillWorkbenchAction(async () => (await setAiCapabilityTargetEnabled(request)) ?? workbenchStateRef.current),
              onSaveAiCapabilityTargets: (request) => applySkillWorkbenchAction(async () => (await saveAiCapabilityTargets(request)) ?? workbenchStateRef.current),
              onSaveMcpServer: (request) => applySkillWorkbenchAction(async () => (await saveMcpServerConfiguration(request as unknown as Record<string, unknown>)) ?? workbenchStateRef.current),
              onRemoveMcpServer: (serverId) => applySkillWorkbenchAction(async () => (await removeMcpServerConfiguration(serverId)) ?? workbenchStateRef.current),
            }}
            startup={props.startup}
            workbench={workbenchState}
          />
        )}
        {showStartupOverlay ? <StartupOverlay startup={props.startup} /> : null}
      </main>
      <footer className="statusbar">
        <span>版本 {props.workbench.appVersion}</span>
        <StatusbarRuntimeActivities activities={statusbarRuntimeActivities} />
        <RepositoryLink onOpenExternalUrl={openExternalUrl} />
      </footer>
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

// initialThemeMode returns the persisted shadcn theme, defaulting to light.
// initialThemeMode 返回持久化的 shadcn 主题，默认使用亮色。
function initialThemeMode(): ThemeMode {
  const stored = window.localStorage.getItem("vulcan-webui-theme");
  return stored === "light" || stored === "dark" ? stored : "light";
}

// webuiClickTrace emits lightweight browser-console diagnostics for user initiated WebUI actions.
// webuiClickTrace 为用户触发的 WebUI 操作输出轻量浏览器控制台诊断日志。
function webuiClickTrace(_action: string, _detail: Record<string, unknown> = {}): void {
  // Kept as a no-op compatibility hook so existing click paths stay unchanged without emitting dev logs.
  // 保留为空实现兼容钩子，避免改变现有点击路径，同时不再输出开发日志。
}

// copyText writes transcript detail text to the system clipboard when available.
// copyText 在可用时将转录详情文本写入系统剪贴板。
async function copyText(text: string): Promise<void> {
  await navigator.clipboard?.writeText(text);
}

