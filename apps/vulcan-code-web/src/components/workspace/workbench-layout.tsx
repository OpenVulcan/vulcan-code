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
import type {
  AgentSkillsCatalogUpdate,
  ComposerSlashCommandSummary,
  ConversationArchivedDelta,
  ConversationMessage,
  ConversationSummary,
  FeatureAreaSummary,
  LuaSkillsCatalogUpdate,
  MainAreaId,
  PendingQuestionSnapshot,
  PendingPromptSummary,
  ProjectSummary,
  SettingsSubAreaId,
  StartupSnapshot,
  SubagentCatalogUpdate,
  VmmConfigDraftSummary,
  WorkbenchDeltaEnvelope,
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
  saveAiCapabilityTargets,
  saveAiCapabilityVendor,
  saveGitCommitSettings,
  saveMcpServerConfiguration,
  saveProviderConfiguration,
  saveProviderEnabledModels,
  saveProviderModelOverride,
  selectWebuiLocale,
  setAiCapabilityTargetEnabled,
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
} from "@/service/workbench-snapshot-service";
import { localMediaSourceUrl } from "@/resources/localMedia";
import {
  applyWorkbenchDeltaWithSubagentSurfaceMessageCache,
  coalesceWorkbenchDeltaEnvelopes,
  markConversationLocallyRead,
  mergeConversationHydrationSnapshotWithSubagentSurfaceMessageCache,
  mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache,
  shouldAcceptWorkbenchRevision,
  workbenchWithSelectedConversationProjection,
  workbenchWithSubagentSurfaceMessageCache,
  type SubagentSurfaceMessageCache,
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
  const [activeWorkbenchSkillSubArea, setActiveWorkbenchSkillSubArea] = React.useState<SkillSubAreaId>("catalog");
  // activeWorkbenchMemorySubArea stores the embedded memory settings sub page.
  // activeWorkbenchMemorySubArea 保存内嵌记忆设置的子页面。
  const [activeWorkbenchMemorySubArea, setActiveWorkbenchMemorySubArea] = React.useState<MemorySubAreaId>("status");
  // initialDraftConversationRef computes the first-run draft once for local selection state.
  // initialDraftConversationRef 为本地选择状态只计算一次首次启动草稿。
  const initialDraftConversationRef = React.useRef<DraftConversationSelection | null | undefined>(undefined);
  if (initialDraftConversationRef.current === undefined) {
    initialDraftConversationRef.current = defaultDraftConversationForEmptyWorkbench(props.workbench);
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
  // locallyCreatedConversationIdsRef distinguishes live-created transcripts from persisted conversations that require history hydration.
  // locallyCreatedConversationIdsRef 区分实时创建的转录与需要历史水合的持久化会话。
  const locallyCreatedConversationIdsRef = React.useRef<Set<string>>(new Set());
  // subagentSurfaceMessageCacheRef retains every loaded child transcript for instant side-panel reuse.
  // subagentSurfaceMessageCacheRef 保留每个已载入的子代理转录，用于侧栏即时复用。
  const subagentSurfaceMessageCacheRef = React.useRef<SubagentSurfaceMessageCache>(new Map());
  // workbenchState stores local message hydration updates.
  // workbenchState 保存本地会话正文水合更新。
  const [workbenchState, setWorkbenchState] = React.useState<WorkbenchSnapshot>(
    () => workbenchWithSubagentSurfaceMessageCache(props.workbench, subagentSurfaceMessageCacheRef.current),
  );
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
  // workbenchStateRef exposes the latest workbench state to route-click callbacks without stale closures.
  // workbenchStateRef 向路由点击回调暴露最新工作台状态，避免闭包过期。
  const workbenchStateRef = React.useRef<WorkbenchSnapshot>(workbenchState);
  workbenchStateRef.current = workbenchState;
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
  // mergeWorkbenchWithLocalCaches centralizes service snapshot merges that must preserve loaded child surfaces.
  // mergeWorkbenchWithLocalCaches 集中处理必须保留已载入子代理界面的服务快照合并。
  const mergeWorkbenchWithLocalCaches = React.useCallback((
    current: WorkbenchSnapshot,
    incoming: WorkbenchSnapshot,
    options?: WorkbenchMergeOptions,
  ) => (
    (() => {
      const merged = mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache(
        current,
        incoming,
        subagentSurfaceMessageCacheRef.current,
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
  // mergeConversationHydrationWithLocalCaches merges focused conversation body responses without losing child panels.
  // mergeConversationHydrationWithLocalCaches 合并聚焦会话正文响应且不丢失子代理面板。
  const mergeConversationHydrationWithLocalCaches = React.useCallback((
    current: WorkbenchSnapshot,
    incoming: WorkbenchSnapshot,
    options?: WorkbenchMergeOptions,
  ) => (
    (() => {
      const merged = mergeConversationHydrationSnapshotWithSubagentSurfaceMessageCache(
        current,
        incoming,
        subagentSurfaceMessageCacheRef.current,
        options,
      );
      if (merged !== current) {
        acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, merged.revision ?? 0);
      }
      return merged;
    })()
  ), []);
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
      (nextWorkbench, envelope) => applyWorkbenchDeltaWithSubagentSurfaceMessageCache(
        nextWorkbench,
        envelope,
        subagentSurfaceMessageCacheRef.current,
      ),
      current,
    ));
    return true;
  }, [acceptConversationArchiveTransitions, acceptProjectRemovalTransition]);
  // applyLocalPreviewWithLocalCaches applies optimistic snapshots without service revision gating.
  // applyLocalPreviewWithLocalCaches 应用乐观快照，不经过服务版本门禁。
  const applyLocalPreviewWithLocalCaches = React.useCallback((current: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => {
    return workbenchWithSubagentSurfaceMessageCache(incoming, subagentSurfaceMessageCacheRef.current);
  }, []);
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
  // selectedConversationCreatedLocally identifies a new live transcript that has no persisted history to hydrate.
  // selectedConversationCreatedLocally 标识没有持久化历史需要水合的新建实时转录。
  const selectedConversationCreatedLocally = selectedConversationId
    ? locallyCreatedConversationIdsRef.current.has(selectedConversationId)
    : false;
  // selectedConversationHydrated records whether the selected body is already available locally or retained by Rust.
  // selectedConversationHydrated 记录当前正文是否已由本地创建流程提供或由 Rust 保留。
  const selectedConversationHydrated = Boolean(
    selectedConversationCreatedLocally || selectedConversationHistoryCursor?.subagentMetadataLoaded,
  );
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
    mergeConversationHydrationWithLocalCaches,
    setWorkbenchState,
  });
  // selectedDraftConversation stores the draft used by the console header and empty state.
  // selectedDraftConversation 保存控制台标题栏与空状态使用的草稿会话。
  const selectedDraftConversation = selectedConversationId === draftConversation?.id ? draftConversation : null;
  // consoleWorkbenchState reprojects selected-session fields from the UI-local selection.
  // consoleWorkbenchState 根据 UI 本地选择重新投影当前会话字段。
  const consoleWorkbenchState = React.useMemo<WorkbenchSnapshot>(() => {
    const projectedConversationId = selectedDraftConversation ? null : selectedConversationId;
    return workbenchWithSelectedConversationProjection(workbenchState, projectedConversationId);
  }, [selectedConversationId, selectedDraftConversation, workbenchState]);
  // showStartupOverlay blocks workspace interaction until the real service bootstrap completes.
  // showStartupOverlay 在真实服务启动完成前阻塞工作区交互。
  const showStartupOverlay = !props.startup.ready && props.startup.phase !== "browser-preview";

  // Keeps the async selection guard aligned with the latest sidebar choice.
  // 保持异步选中保护与最新侧栏选择一致。
  React.useEffect(() => {
    selectedConversationIdRef.current = selectedConversationId;
  }, [selectedConversationId]);
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

  const {
    handleSaveAiCapabilityConfiguration,
    handleSaveSessionGoal,
    handleResumeSessionGoal,
    handleClearSessionGoal,
    handleUpdateSessionGoalStatus,
    handleSetSessionPlanMode,
    handleSetSessionVmmPolicy,
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
    handleStopSubagentTask,
    handleCloseSubagentTask,
    handleRemovePendingPrompt,
    handleReorderPendingPrompts,
    handleSelectWorkbenchModel,
    handleStopCommandSession,
    handleListWorkbenchModelSecondaryRoutes,
    handleSelectWorkbenchModelSecondaryRoute,
    handleGetUserPromptEditDraft,
    handleSubmitUserPromptEdit,
    handleForkConversation,
    conversationForkInProgress,
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
    locallyCreatedConversationIdsRef,
    workbenchStateRef,
    requestStopConversationRunChoice,
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
    setSelectedConversationId(conversationId);
    setWorkbenchState((current) => markConversationLocallyRead(current, conversationId));
    void selectConversation(conversationId)
      .then((snapshot) => {
        if (snapshot) {
          setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(
            current,
            snapshot,
            { selectedConversationSource: "incoming" },
          ));
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

  // Ensure service snapshots with projects but no sessions still have a draft send target.
  // 确保服务快照存在项目但没有会话时仍有草稿发送目标。
  React.useEffect(() => {
    if (draftConversation || selectedConversationId || workbenchState.conversations.length > 0) {
      return;
    }
    const nextDraftConversation = defaultDraftConversationForEmptyWorkbench(workbenchState);
    if (!nextDraftConversation) {
      return;
    }
    setDraftConversation(nextDraftConversation);
    setSelectedConversationId(nextDraftConversation.id);
  }, [draftConversation, selectedConversationId, workbenchState.conversations.length, workbenchState.projects]);

  React.useEffect(() => {
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
      };
      acceptedProjectSettingsRevisionByKeyRef.current.set(`${projectId}:vmm`, acceptedRevisions.vmm);
      acceptedProjectSettingsRevisionByKeyRef.current.set(
        `${projectId}:agentSkills`,
        acceptedRevisions.agentSkills,
      );
      acceptedProjectSettings[projectId] = acceptedRevisions;
    }
    setProjectSettingsRevisionById((current) => ({ ...current, ...acceptedProjectSettings }));
    setWorkbenchState((current) => (
      mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache(
        current,
        props.workbench,
        subagentSurfaceMessageCacheRef.current,
        { preserveExpandedProjectIds: expandedProjectIdsLocallyEditedRef.current },
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
  }, [acceptServiceWorkbenchSnapshot, props.workbench]);

  React.useEffect(() => {
    if (!props.workbenchDelta) {
      return;
    }
    const acceptedEnvelopes: WorkbenchDeltaEnvelope[] = [];
    // acceptedProjectSettingsRevisions collects target-and-domain invalidations committed by this event batch.
    // acceptedProjectSettingsRevisions 收集当前事件批次提交的目标与领域失效版本。
    const acceptedProjectSettingsRevisions = new Map<string, ProjectSettingsRevision>();
    let nextAcceptedRevision = acceptedWorkbenchRevisionRef.current;
    for (const envelope of props.workbenchDelta.envelopes) {
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
      return;
    }
    acceptedWorkbenchRevisionRef.current = nextAcceptedRevision;
    setWorkbenchState((current) => (
      acceptedEnvelopes.reduce(
        (nextWorkbench, envelope) => applyWorkbenchDeltaWithSubagentSurfaceMessageCache(
          nextWorkbench,
          envelope,
          subagentSurfaceMessageCacheRef.current,
        ),
        current,
      )
    ));
  }, [acceptConversationArchiveTransitions, acceptProjectRemovalTransition, props.workbenchDelta]);

  // Complete draft materialization only after the authoritative sidebar row becomes visible.
  // 仅在权威侧栏行可见后完成草稿转正状态。
  React.useEffect(() => {
    const pendingConversationId = pendingMaterializedConversationIdRef.current;
    if (!pendingConversationId) {
      return;
    }
    if (workbenchState.conversations.some((conversation) => conversation.id === pendingConversationId)) {
      pendingMaterializedConversationIdRef.current = null;
    }
  }, [workbenchState.conversations]);

  // Retire local-creation markers after their active conversation lifecycle ends.
  // 在对应活动会话生命周期结束后清理本地创建标记。
  React.useEffect(() => {
    // activeConversationIds is the authoritative active sidebar identity set for marker retention.
    // activeConversationIds 是保留创建标记所依据的权威活动侧栏身份集合。
    const activeConversationIds = new Set(workbenchState.conversations.map((conversation) => conversation.id));
    for (const conversationId of locallyCreatedConversationIdsRef.current) {
      if (conversationId === pendingMaterializedConversationIdRef.current) {
        continue;
      }
      if (!activeConversationIds.has(conversationId)) {
        locallyCreatedConversationIdsRef.current.delete(conversationId);
      }
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
            conversationForkInProgress={conversationForkInProgress}
            pinnedConversationIds={pinnedConversationIds}
            onConversationMenuAction={handleConversationMenuAction}
            onCreateConversation={handleCreateConversation}
            onCreateProject={handleCreateProject}
            onOpenSkills={handleOpenWorkspaceSkills}
            onOpenAgentSkills={handleOpenWorkspaceAgentSkills}
            onOpenAgents={handleOpenWorkspaceAgents}
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
            onSaveAiCapabilityConfiguration={handleSaveAiCapabilityConfiguration}
            onSaveSessionGoal={handleSaveSessionGoal}
            onResumeSessionGoal={handleResumeSessionGoal}
            onClearSessionGoal={handleClearSessionGoal}
            onUpdateSessionGoalStatus={handleUpdateSessionGoalStatus}
            onSetSessionPlanMode={handleSetSessionPlanMode}
            onSetSessionVmmPolicy={handleSetSessionVmmPolicy}
            onStopConversationRun={handleStopConversationRun}
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
            onGetUserPromptEditDraft={handleGetUserPromptEditDraft}
            onSubmitUserPromptEdit={handleSubmitUserPromptEdit}
            onForkConversation={handleForkConversation}
            onLoadToolDetail={loadCachedConversationToolDetail}
            resolveMediaSourceUrl={localMediaSourceUrl}
            footer={(
              <footer className="statusbar workspace-statusbar">
                <span>{formatWorkbenchTranslation(i18n.t, "workspaceShell.version", { version: props.workbench.appVersion })}</span>
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
            luaSkillActions={luaSkillActions}
            agentSkillActions={agentSkillActions}
            agentActions={agentActions}
            memoryActions={{
              onLoadProjectVmmConfig: loadProjectVmmConfig,
              onRunVmmAction: runVmmMemoryAction,
              onSaveVmmConfig: (request) => applySkillWorkbenchAction(() => saveVmmMemoryConfig(request)),
            }}
            settingsActions={{
              onSaveProvider: (request) => applySkillWorkbenchAction(async () => (await saveProviderConfiguration(request)) ?? workbenchStateRef.current),
              onRemoveProvider: (providerId) => applySkillWorkbenchAction(async () => (await removeProviderConfiguration(providerId)) ?? workbenchStateRef.current),
              onSaveProviderEnabledModels: (request) => applySkillWorkbenchAction(async () => (await saveProviderEnabledModels(request)) ?? workbenchStateRef.current),
              onSaveProviderModelOverride: (request) => applySkillWorkbenchAction(async () => (await saveProviderModelOverride(request)) ?? workbenchStateRef.current),
              onSaveGitCommitSettings: (request) => applySkillWorkbenchAction(async () => (await saveGitCommitSettings(request)) ?? workbenchStateRef.current),
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
          }
          : { vmm: 0, agentSkills: 0 }}
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
  );
}

// defaultDraftConversationForEmptyWorkbench creates a local draft when projects exist but sessions do not.
// defaultDraftConversationForEmptyWorkbench 在存在项目但没有会话时创建本地草稿。
function defaultDraftConversationForEmptyWorkbench(
  // Workbench snapshot used to choose the draft owner project.
  // 用于选择草稿归属项目的工作台快照。
  workbench: WorkbenchSnapshot,
): DraftConversationSelection | null {
  if (workbench.conversations.length > 0) {
    return null;
  }
  const project = workbench.projects[0] ?? null;
  if (!project) {
    return null;
  }
  return draftConversationForProject(project);
}

// draftConversationForProject creates the local route used before the first persisted send.
// draftConversationForProject 创建首次持久化发送前使用的本地路由。
function draftConversationForProject(
  // Project that owns the future persisted conversation.
  // 拥有未来持久化会话的项目。
  project: ProjectSummary,
): DraftConversationSelection {
  return {
    id: `draft-${project.id}-${Date.now()}`,
    projectId: project.id,
    projectName: project.name,
  };
}

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

