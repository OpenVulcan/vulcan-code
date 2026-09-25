import React from "react";
import { toast } from "sonner";
import { DiffReviewPanel } from "@/diff/diff-review-panel";
import {
  activeWorkbenchSurfaceId,
  diffFileFromWorkspaceDetailAsync,
  diffReviewSelectionStableKey,
  diffSnapshotFromWorkspaceResponseAsync,
  diffSnapshotWithWorkspaceDetailFile,
  selectedConversationProjectId,
  selectedConversationWorkspacePath,
  type WorkspaceReviewDraftConversation,
} from "@/diff/workspace-review-model";
import {
  selectLatestWorkspaceReviewChangeSetV1,
  workspaceReviewSnapshotFromChangeSetV1,
} from "@/diff/workspace-review-v1-selector";
import type {
  DiffFile,
  DiffHunkExpansionRequest,
  DiffPanelSnapshot,
  DiffReviewSelection,
  WorkspaceGitFileActionKind,
  WorkspaceGitFileActionRequest,
  WorkspaceGitFileActionResult,
  WorkspaceDiffFileDetailRequest,
  WorkspaceDiffFileDetailResponse,
  WorkspaceDiffSearchRequest,
  WorkspaceDiffSearchResponse,
  WorkspaceDiffSnapshotResponse,
  WorkspaceGitRepositorySummary,
  WorkspaceReviewFocusRequest,
} from "@/diff/types";
import { useI18n, type TranslationKey } from "@/i18n";
import type { WorkspaceDiffFileDetailLoadOptions } from "@/service/workbench-snapshot-service";
import { loadWorkspaceGitCatalog } from "@/service/workbench-snapshot-service";
import { useSelectedConversationEventWindowSnapshot } from "@/state/conversation-event-window-react";
import { useWorkspaceReviewRequestOwnership } from "@/components/workspace/workspace-review-panel-request-ownership";
import type { WorkbenchSnapshot } from "@/types";

// WorkspaceReviewPanelCacheEntry stores one mounted review panel state across close and reopen cycles.
// WorkspaceReviewPanelCacheEntry 保存一次右侧审核面板在关闭与重开之间需要复用的状态。
interface WorkspaceReviewPanelCacheEntry {
  // Latest workspace diff snapshot rendered by the review panel.
  // 审核面板最近渲染的工作区 diff 快照。
  snapshot: DiffPanelSnapshot | null;
  // Git review source selection used by the cached snapshot.
  // 缓存快照对应的 Git 审核来源选择。
  selection: DiffReviewSelection;
  // Workspace path that produced the cached snapshot.
  // 生成缓存快照的工作区路径。
  workspacePath: string | null;
  // Stable Git review source key that produced the cached snapshot.
  // 生成缓存快照的稳定 Git 审核来源键。
  reviewKey: string | null;
  // Last refresh error shown by the panel.
  // 面板最近展示的刷新错误。
  error: string | null;
}

// workspaceReviewPanelCache preserves review content while the right rail is temporarily collapsed.
// workspaceReviewPanelCache 在右侧栏临时折叠时保留审核内容。
const workspaceReviewPanelCache = new Map<string, WorkspaceReviewPanelCacheEntry>();

// lastWorkspaceReviewVisibleWorkspacePath tracks the latest project path represented by the review panel.
// lastWorkspaceReviewVisibleWorkspacePath 跟踪审核面板最近代表的项目路径。
let lastWorkspaceReviewVisibleWorkspacePath: string | null = null;

// workspaceReviewPanelCacheLimit caps cached review states to avoid retaining old diff trees forever.
// workspaceReviewPanelCacheLimit 限制缓存的审核状态数量，避免长期保留旧 diff 树。
const workspaceReviewPanelCacheLimit = 12;

// DEFAULT_WORKSPACE_REVIEW_SELECTION is the live Git diff source shown when no cached choice exists.
// DEFAULT_WORKSPACE_REVIEW_SELECTION 是没有缓存选择时展示的实时 Git diff 默认来源。
const DEFAULT_WORKSPACE_REVIEW_SELECTION: DiffReviewSelection = { mode: "unstaged" };

// WORKSPACE_DIFF_FILE_DETAIL_TIMEOUT_MS mirrors the target review-diff request timeout.
// WORKSPACE_DIFF_FILE_DETAIL_TIMEOUT_MS 对齐目标项目的单文件 review-diff 请求超时。
const WORKSPACE_DIFF_FILE_DETAIL_TIMEOUT_MS = 15_000;

// WorkspaceReviewPanelHost owns Git diff review state for the right context panel.
// WorkspaceReviewPanelHost 持有右侧上下文面板的 Git diff 审核状态。
// Parameters: props supplies selected route identity and typed diff/Git IO. Returns the current-source review surface.
// 参数：props 提供选中路由身份与强类型 diff/Git 输入输出。返回当前来源的评审界面。
export function WorkspaceReviewPanelHost(props: {
  // Current draft conversation selection when the selected route is not persisted yet.
  // 当前路由尚未持久化时对应的草稿会话选择。
  selectedDraftConversation: WorkspaceReviewDraftConversation | null;
  // Current selected conversation id from the local workbench route.
  // 来自本地工作台路由的当前选中会话标识。
  selectedConversationId: string | null;
  // Workbench snapshot carrying selected-session side-panel projections.
  // 携带当前选中会话侧面板投影的工作台快照。
  workbench: WorkbenchSnapshot;
  // Monotonic request id used to focus the live uncommitted review source.
  // 用于聚焦实时未提交审核来源的单调请求标识。
  reviewFocusRequest: WorkspaceReviewFocusRequest;
  // Copies raw diff text using the host-aware clipboard implementation.
  // 使用宿主感知的剪贴板实现复制原始 diff 文本。
  onCopyRawDiff: (rawDiff: string) => void | Promise<void>;
  // Loads one live workspace Git diff snapshot.
  // 加载一次实时工作区 Git diff 快照。
  loadWorkspaceDiffSnapshot: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: DiffReviewSelection,
  ) => Promise<WorkspaceDiffSnapshotResponse>;
  // Loads one live workspace Git diff file detail.
  // 加载一次实时工作区 Git diff 文件详情。
  loadWorkspaceDiffFileDetail: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: DiffReviewSelection,
    file: WorkspaceDiffFileDetailRequest,
    options?: WorkspaceDiffFileDetailLoadOptions,
  ) => Promise<WorkspaceDiffFileDetailResponse>;
  // Loads compact backend file-level search hits for one workspace diff.
  // 为单个工作区 diff 加载紧凑的后端文件级搜索命中。
  loadWorkspaceDiffSearch: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: DiffReviewSelection,
    search: WorkspaceDiffSearchRequest,
  ) => Promise<WorkspaceDiffSearchResponse>;
  // Runs one file-level Git action and returns the backend operation summary.
  // 执行一次文件级 Git 操作并返回后端操作摘要。
  runWorkspaceGitFileAction: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: WorkspaceGitFileActionRequest,
  ) => Promise<WorkspaceGitFileActionResult>;
  // Records one loaded workspace diff snapshot in the project-level Git store.
  // 将一次已加载的工作区 diff 快照记录到项目级 Git 状态。
  onWorkspaceDiffSnapshotLoaded?: (
    workspacePath: string,
    projectId: string | null,
    selection: DiffReviewSelection,
    response: WorkspaceDiffSnapshotResponse,
  ) => void;
  // Refreshes the project-level Git summary after a Git mutation.
  // Git 变更后刷新项目级 Git 摘要。
  onWorkspaceGitStateChanged?: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    repositoryKey?: string | null,
  ) => Promise<void>;
  // Optional shared Git commit action label shown in the review toolbar.
  // 审核工具栏中展示的可选共享 Git 提交动作标签。
  gitCommitActionLabel?: string;
  // Whether the shared Git commit action is currently running.
  // 共享 Git 提交动作当前是否正在运行。
  gitCommitActionBusy?: boolean;
  // Whether the shared Git commit action is currently unavailable.
  // 共享 Git 提交动作当前是否不可用。
  gitCommitActionDisabled?: boolean;
  // Opens the shared Git commit or push dialog.
  // 打开共享 Git 提交或推送弹窗。
  onOpenGitCommitAction?: () => void;
  // Synchronizes the active right-panel repository with shared commit controls.
  // 将右侧面板当前仓库同步到共享提交控件。
  onSelectGitRepository?: (repositoryKey: string | null) => void;
}) {
  // t translates the review panel shell labels.
  // t 翻译审核面板外壳标签。
  const { t } = useI18n();
  // conversationEventWindowSnapshot is the atomically published selected-session V1 authority.
  // conversationEventWindowSnapshot 是原子发布的选中会话 V1 权威快照。
  const conversationEventWindowSnapshot = useSelectedConversationEventWindowSnapshot();
  // conversationChangeSetSelection is empty while the external-store owner belongs to another route.
  // conversationChangeSetSelection 在 external-store owner 属于其他路由时为空。
  const conversationChangeSetSelection = React.useMemo(() => {
    if (conversationEventWindowSnapshot.owner?.session_id !== props.selectedConversationId) {
      return null;
    }
    return selectLatestWorkspaceReviewChangeSetV1(conversationEventWindowSnapshot);
  }, [conversationEventWindowSnapshot, props.selectedConversationId]);
  // conversationReviewSnapshot projects the typed persistent ChangeSet without raw-diff parsing.
  // conversationReviewSnapshot 无需解析 raw diff，直接投影强类型持久化 ChangeSet。
  const conversationReviewSnapshot = React.useMemo(
    () => conversationChangeSetSelection === null
      ? null
      : workspaceReviewSnapshotFromChangeSetV1(conversationChangeSetSelection),
    [conversationChangeSetSelection],
  );
  // activeSurfaceId stores the conversation or subagent surface currently represented by the panel.
  // activeSurfaceId 保存当前面板代表的会话或子代理界面标识。
  const activeSurfaceId = activeWorkbenchSurfaceId(props.workbench, props.selectedConversationId);
  // workspaceProjectId stores the backend project id used to resolve the Git diff path.
  // workspaceProjectId 保存用于解析 Git diff 路径的后端项目标识。
  const workspaceProjectId = React.useMemo(
    () => selectedConversationProjectId(props.workbench, props.selectedConversationId, props.selectedDraftConversation),
    [props.selectedConversationId, props.selectedDraftConversation, props.workbench],
  );
  // workspacePath stores the selected conversation owner project path for live diff refresh.
  // workspacePath 保存当前选中会话所属项目路径，用于刷新实时 diff。
  const workspacePath = React.useMemo(
    () => selectedConversationWorkspacePath(props.workbench, props.selectedConversationId, props.selectedDraftConversation),
    [props.selectedConversationId, props.selectedDraftConversation, props.workbench],
  );
  // reviewPanelCacheKey stores the project-level cache bucket for Git review state.
  // reviewPanelCacheKey 保存 Git 审核状态的项目级缓存桶。
  const reviewPanelCacheKey = React.useMemo(
    () => workspaceReviewPanelCacheKey(workspacePath),
    [workspacePath],
  );
  // gitRepositories stores repositories available to the right-panel selector.
  // gitRepositories 保存右侧面板选择器可用的仓库。
  const [gitRepositories, setGitRepositories] = React.useState<WorkspaceGitRepositorySummary[]>([]);
  // gitRepositoryCatalogError stores discovery or monitoring-setting errors.
  // gitRepositoryCatalogError 保存发现或监听设置错误。
  const [gitRepositoryCatalogError, setGitRepositoryCatalogError] = React.useState<string | null>(null);
  // requestOwnership binds all review async writers to the committed project-level cache route.
  // requestOwnership 将全部审核异步写入者绑定到已提交的项目级缓存路由。
  const requestOwnership = useWorkspaceReviewRequestOwnership(reviewPanelCacheKey);
  // initialReviewPanelCache restores a prior collapsed-panel state for this surface.
  // initialReviewPanelCache 恢复该界面先前折叠面板时保留的状态。
  const initialReviewPanelCache = React.useMemo(
    () => workspaceReviewPanelCache.get(reviewPanelCacheKey) ?? null,
    [reviewPanelCacheKey],
  );
  // workspaceReviewSnapshot stores the latest user-refreshed workspace Git diff.
  // workspaceReviewSnapshot 保存最近一次用户刷新得到的工作区 Git diff。
  const [workspaceReviewSnapshot, setWorkspaceReviewSnapshot] = React.useState<DiffPanelSnapshot | null>(
    () => initialReviewPanelCache?.snapshot ?? null,
  );
  // diffReviewSelection stores the current Git review source selection.
  // diffReviewSelection 保存当前 Git 审核来源选择。
  const [diffReviewSelection, setDiffReviewSelection] = React.useState<DiffReviewSelection>(
    () => initialReviewPanelCache?.selection ?? DEFAULT_WORKSPACE_REVIEW_SELECTION,
  );
  // diffReviewSelectionKey stores a stable key for the current Git review source.
  // diffReviewSelectionKey 保存当前 Git 审核来源的稳定键。
  const diffReviewSelectionKey = React.useMemo(
    () => diffReviewSelectionStableKey(diffReviewSelection),
    [diffReviewSelection],
  );
  // reviewSourceCacheKey isolates right-panel snapshots and source choices by project and repository.
  // reviewSourceCacheKey 按项目与仓库隔离右侧面板快照及来源选择。
  const reviewSourceCacheKey = React.useMemo(
    () => workspaceReviewSourceCacheKey(workspacePath, diffReviewSelection.repositoryKey),
    [diffReviewSelection.repositoryKey, workspacePath],
  );
  // workspaceReviewPath stores the project path that produced the current workspace diff snapshot.
  // workspaceReviewPath 保存生成当前工作区 diff 快照的项目路径。
  const [workspaceReviewPath, setWorkspaceReviewPath] = React.useState<string | null>(
    () => initialReviewPanelCache?.workspacePath ?? null,
  );
  // workspaceReviewKey stores the Git review source that produced the current snapshot.
  // workspaceReviewKey 保存生成当前快照的 Git 审核来源。
  const [workspaceReviewKey, setWorkspaceReviewKey] = React.useState<string | null>(
    () => initialReviewPanelCache?.reviewKey ?? null,
  );
  // workspaceReviewLoading tracks the live diff refresh request.
  // workspaceReviewLoading 跟踪实时 diff 刷新请求状态。
  const [workspaceReviewLoading, setWorkspaceReviewLoading] = React.useState(false);
  // workspaceReviewError stores the latest live diff refresh error.
  // workspaceReviewError 保存最近一次实时 diff 刷新错误。
  const [workspaceReviewError, setWorkspaceReviewError] = React.useState<string | null>(
    () => initialReviewPanelCache?.error ?? null,
  );
  // hydratedReviewPanelCacheKey records which project cache route owns the current React review state.
  // hydratedReviewPanelCacheKey 记录当前 React 审核状态所属的项目缓存路由。
  const [hydratedReviewPanelCacheKey, setHydratedReviewPanelCacheKey] = React.useState(reviewPanelCacheKey);
  // workspaceGitActionPending tracks one in-flight Git mutation from the review panel.
  // workspaceGitActionPending 跟踪审核面板发起的一次进行中的 Git 变更操作。
  const [workspaceGitActionPending, setWorkspaceGitActionPending] = React.useState(false);
  // lastHydratedReviewPanelCacheKeyRef limits cache hydration to actual project or repository changes.
  // lastHydratedReviewPanelCacheKeyRef 将缓存水合限制为真实项目或仓库变化。
  const lastHydratedReviewPanelCacheKeyRef = React.useRef<string | null>(null);
  // Distinguish project restoration from a deliberate repository choice in the current project.
  // 区分项目恢复与当前项目内用户主动选择的仓库。
  const lastHydratedWorkspaceCacheKeyRef = React.useRef<string | null>(null);
  // pendingReviewSourceHydrationRef prevents the previous repository state from overwriting a cached target.
  // pendingReviewSourceHydrationRef 防止旧仓库状态覆盖正在水合的目标缓存。
  const pendingReviewSourceHydrationRef = React.useRef<string | null>(null);
  // forceWorkspaceReviewRefreshRef marks real project switches while ignoring simple collapse/reopen cycles.
  // forceWorkspaceReviewRefreshRef 标记真实项目切换，同时忽略单纯折叠再展开。
  const forceWorkspaceReviewRefreshRef = React.useRef(false);
  // workspaceDetailAttemptedRef records file details already attempted for this snapshot.
  // workspaceDetailAttemptedRef 记录当前快照中已经尝试加载过详情的文件。
  const workspaceDetailAttemptedRef = React.useRef(new Set<string>());
  // latestDiffReviewSelectionRef exposes the latest committed source to long-running Git mutation continuations.
  // latestDiffReviewSelectionRef 向长生命周期 Git 变更延续逻辑暴露最近提交的审核来源。
  const latestDiffReviewSelectionRef = React.useRef(diffReviewSelection);
  // handledReviewFocusRequestRef stores the latest external focus request already applied.
  // handledReviewFocusRequestRef 保存已经应用的最近一次外部聚焦请求。
  const handledReviewFocusRequestRef = React.useRef(0);
  const activeWorkspaceReviewSnapshot = workspaceReviewPath === workspacePath
    && workspaceReviewKey === diffReviewSelectionKey
      ? workspaceReviewSnapshot
      : null;
  const activeReviewSnapshot = activeWorkspaceReviewSnapshot ?? conversationReviewSnapshot;

  React.useLayoutEffect(() => {
    // Project route changes synchronously release loading flags owned by invalidated requests.
    // 项目路由变化会同步释放由失效请求持有的加载标记。
    setWorkspaceReviewLoading(false);
    setWorkspaceGitActionPending(false);
  }, [reviewPanelCacheKey]);

  React.useEffect(() => {
    let cancelled = false;
    if (!workspacePath) {
      setGitRepositories([]);
      setGitRepositoryCatalogError(null);
      return undefined;
    }
    void loadWorkspaceGitCatalog(workspacePath, workspaceProjectId, props.selectedConversationId)
      .then((catalog) => {
        if (!cancelled) {
          setGitRepositories(catalog.repositories);
          setDiffReviewSelection((current) => {
            const enabledRepositories = catalog.repositories.filter((repository) => repository.monitoringEnabled);
            const currentRepository = current.repositoryKey
              ? enabledRepositories.find((repository) => repository.repositoryKey === current.repositoryKey)
              : null;
            if (currentRepository) {
              return current;
            }
            if (enabledRepositories.length === 0) {
              return { ...current, repositoryKey: null };
            }
            const primaryRepository = enabledRepositories.find((repository) => repository.isPrimary)
              ?? enabledRepositories[0];
            return { ...current, repositoryKey: primaryRepository.repositoryKey };
          });
          setGitRepositoryCatalogError(null);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setGitRepositoryCatalogError(error instanceof Error ? error.message : String(error));
          setGitRepositories([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [props.reviewFocusRequest.sequence, props.selectedConversationId, workspacePath, workspaceProjectId]);

  const enabledGitRepositories = React.useMemo(
    () => gitRepositories.filter((repository) => repository.monitoringEnabled),
    [gitRepositories],
  );

  React.useLayoutEffect(() => {
    latestDiffReviewSelectionRef.current = diffReviewSelection;
  }, [diffReviewSelection]);

  React.useEffect(() => {
    props.onSelectGitRepository?.(diffReviewSelection.repositoryKey ?? null);
  }, [diffReviewSelection.repositoryKey, props.onSelectGitRepository]);

  // handleRefreshWorkspaceDiff refreshes the current project Git diff through the local service.
  // handleRefreshWorkspaceDiff 通过本地服务刷新当前项目 Git diff。
  const handleRefreshWorkspaceDiff = React.useCallback(async (selectionOverride?: DiffReviewSelection) => {
    if (hydratedReviewPanelCacheKey !== reviewPanelCacheKey) {
      return;
    }
    if (!workspacePath) {
      setWorkspaceReviewError(t("workspaceReview.noProjectPathForRefresh"));
      return;
    }
    // ownedRequest makes only the latest refresh for the committed project route eligible to update state.
    // ownedRequest 仅允许已提交项目路由的最新刷新更新状态。
    const ownedRequest = requestOwnership.beginRefreshRequest();
    if (!ownedRequest) {
      return;
    }
    // requestSelection freezes the requested Git source across service load and asynchronous parsing.
    // requestSelection 在服务加载与异步解析期间固定所请求的 Git 来源。
    const requestSelection = selectionOverride ?? diffReviewSelection;
    // requestKey is the stable source identity attached to the resulting snapshot.
    // requestKey 是附加到结果快照的稳定来源身份。
    const requestKey = diffReviewSelectionStableKey(requestSelection);
    setWorkspaceReviewLoading(true);
    setWorkspaceReviewError(null);
    try {
      // response is the backend diff payload for the captured project route and source.
      // response 是捕获项目路由与来源对应的后端 diff 载荷。
      const response = await props.loadWorkspaceDiffSnapshot(workspacePath, workspaceProjectId, activeSurfaceId, requestSelection);
      if (!requestOwnership.ownsRefreshRequest(ownedRequest)) {
        return;
      }
      props.onWorkspaceDiffSnapshotLoaded?.(workspacePath, workspaceProjectId, requestSelection, response);
      // snapshot is the parsed render model produced by the worker pipeline.
      // snapshot 是工作线程流水线产出的已解析渲染模型。
      const snapshot = await diffSnapshotFromWorkspaceResponseAsync(response);
      if (!requestOwnership.ownsRefreshRequest(ownedRequest)) {
        return;
      }
      setWorkspaceReviewSnapshot(snapshot);
      setWorkspaceReviewPath(workspacePath);
      setWorkspaceReviewKey(requestKey);
      requestOwnership.invalidateDetailRequests();
      workspaceDetailAttemptedRef.current.clear();
    } catch (error) {
      if (!requestOwnership.ownsRefreshRequest(ownedRequest)) {
        return;
      }
      // message normalizes service and parser failures for panel state and user feedback.
      // message 归一化服务与解析失败，供面板状态和用户反馈使用。
      const message = error instanceof Error ? error.message : String(error);
      setWorkspaceReviewError(message);
      toast.error(formatWorkspaceReviewTranslation(t, "workspaceReview.refreshGitDiffFailed", { error: message }));
    } finally {
      if (requestOwnership.finishRefreshRequest(ownedRequest)) {
        setWorkspaceReviewLoading(false);
      }
    }
  }, [
    activeSurfaceId,
    diffReviewSelection,
    hydratedReviewPanelCacheKey,
    props.loadWorkspaceDiffSnapshot,
    props.onWorkspaceDiffSnapshotLoaded,
    requestOwnership,
    reviewPanelCacheKey,
    t,
    workspacePath,
    workspaceProjectId,
  ]);

  // handleLoadWorkspaceDiffFileDetail lazily hydrates one partial file from the backend.
  // handleLoadWorkspaceDiffFileDetail 从后端懒加载并水合单个局部文件。
  const handleLoadWorkspaceDiffFileDetail = React.useCallback(async (
    file: DiffFile,
    contextLines?: number,
    expansion?: DiffHunkExpansionRequest,
  ) => {
    if (!workspacePath || !activeWorkspaceReviewSnapshot) {
      return;
    }
    const detailKey = [
      diffReviewSelectionKey,
      file.key,
      contextLines ?? 0,
      expansion?.hunkIndex ?? "",
      expansion?.direction ?? "",
    ].join("\u0000");
    // retryable allows explicit user retries after failed or timed-out detail loads.
    // retryable 允许用户在失败或超时后显式重试详情加载。
    const retryable = Boolean(contextLines)
      || file.fileDiffLoadState?.status === "failed"
      || file.fileDiffLoadState?.status === "timed-out";
    if (workspaceDetailAttemptedRef.current.has(detailKey) && !retryable) {
      return;
    }
    // ownedRequest coalesces this detail key and binds every continuation to the committed project route.
    // ownedRequest 合并该详情键，并将全部延续逻辑绑定到已提交项目路由。
    const ownedRequest = requestOwnership.beginDetailRequest(detailKey);
    if (!ownedRequest) {
      return;
    }
    workspaceDetailAttemptedRef.current.add(detailKey);
    setWorkspaceReviewSnapshot((previous) => {
      if (!requestOwnership.ownsDetailRequest(detailKey, ownedRequest) || !previous) {
        return previous;
      }
      return workspaceDiffSnapshotWithFileLoadState(previous, file.key, {
        status: "loading",
        message: contextLines
          ? diffHunkExpansionLoadingMessage(expansion, t)
          : t("workspaceReview.fileDetailLoading"),
      });
    });
    try {
      const response = await workspaceDiffFileDetailWithTimeout(
        (signal) => props.loadWorkspaceDiffFileDetail(
          workspacePath,
          workspaceProjectId,
          activeSurfaceId,
          diffReviewSelection,
          {
            key: file.key,
            oldPath: file.oldPath,
            newPath: file.newPath,
            contextLines,
            hunkIndex: expansion?.hunkIndex,
            hunkExpansionDirection: expansion?.direction,
          },
          { signal },
        ),
        WORKSPACE_DIFF_FILE_DETAIL_TIMEOUT_MS,
      );
      if (!requestOwnership.ownsDetailRequest(detailKey, ownedRequest)) {
        return;
      }
      // detailFile is the parsed file projection for this exact base snapshot and expansion request.
      // detailFile 是该准确基础快照与展开请求对应的已解析文件投影。
      const detailFile = await diffFileFromWorkspaceDetailAsync(response, file, expansion);
      if (!requestOwnership.ownsDetailRequest(detailKey, ownedRequest)) {
        return;
      }
      setWorkspaceReviewSnapshot((previous) => {
        if (!requestOwnership.ownsDetailRequest(detailKey, ownedRequest) || !previous) {
          return previous;
        }
        return diffSnapshotWithWorkspaceDetailFile(previous, detailFile, file) ?? previous;
      });
    } catch (error) {
      if (!requestOwnership.ownsDetailRequest(detailKey, ownedRequest)) {
        return;
      }
      // timedOut distinguishes the bounded lazy-load timeout from backend and parsing failures.
      // timedOut 区分受限懒加载超时与后端、解析失败。
      const timedOut = isWorkspaceDiffFileDetailTimeout(error);
      // message is the stable status text written into the matching file card.
      // message 是写入匹配文件卡片的稳定状态文本。
      const message = timedOut
        ? formatWorkspaceReviewTranslation(t, "workspaceReview.fileDetailTimeoutError", {
          seconds: String(Math.round(WORKSPACE_DIFF_FILE_DETAIL_TIMEOUT_MS / 1000)),
        })
        : error instanceof Error ? error.message : String(error);
      setWorkspaceReviewSnapshot((previous) => {
        if (!requestOwnership.ownsDetailRequest(detailKey, ownedRequest) || !previous) {
          return previous;
        }
        return workspaceDiffSnapshotWithFileLoadState(previous, file.key, {
          status: timedOut ? "timed-out" : "failed",
          message,
        });
      });
      toast.error(formatWorkspaceReviewTranslation(
        t,
        timedOut ? "workspaceReview.fileDetailTimeout" : "workspaceReview.fileDetailFailed",
        { error: message },
      ));
    } finally {
      requestOwnership.finishDetailRequest(detailKey, ownedRequest);
    }
  }, [
    activeSurfaceId,
    activeWorkspaceReviewSnapshot,
    diffReviewSelection,
    diffReviewSelectionKey,
    props.loadWorkspaceDiffFileDetail,
    requestOwnership,
    t,
    workspacePath,
    workspaceProjectId,
  ]);

  // handleSearchWorkspaceDiff loads backend file-level search hits for the active live diff.
  // handleSearchWorkspaceDiff 为当前实时 diff 加载后端文件级搜索命中。
  const handleSearchWorkspaceDiff = React.useCallback(async (query: string, limit?: number) => {
    if (hydratedReviewPanelCacheKey !== reviewPanelCacheKey || !workspacePath || !activeWorkspaceReviewSnapshot) {
      return null;
    }
    return props.loadWorkspaceDiffSearch(
      workspacePath,
      workspaceProjectId,
      activeSurfaceId,
      diffReviewSelection,
      { query, limit },
    );
  }, [
    activeSurfaceId,
    activeWorkspaceReviewSnapshot,
    diffReviewSelection,
    hydratedReviewPanelCacheKey,
    props.loadWorkspaceDiffSearch,
    reviewPanelCacheKey,
    workspacePath,
    workspaceProjectId,
  ]);

  // handleWorkspaceGitFileAction executes a Git mutation and refreshes the active review snapshot.
  // handleWorkspaceGitFileAction 执行一次 Git 变更操作并刷新当前审核快照。
  // Parameters: action selects the exact Git operation; optional file restricts its pathspecs. Returns a rejecting promise on failure.
  // 参数：action 指定精确 Git 操作；可选 file 限制路径规格。失败时返回拒绝的 Promise。
  // Successful completion includes shared-state refresh; only the still-owned source may replace the visible review snapshot.
  // 成功完成包括共享状态刷新；仅仍被持有的来源可替换可见评审快照。
  const handleWorkspaceGitFileAction = React.useCallback(async (
    action: WorkspaceGitFileActionKind,
    file?: DiffFile | null,
  ) => {
    if (workspaceGitActionPending) {
      return;
    }
    if (hydratedReviewPanelCacheKey !== reviewPanelCacheKey) {
      return;
    }
    if (!workspacePath) {
      toast.error(t("workspaceReview.noProjectPathForGit"));
      return;
    }
    // ownedRequest synchronously gates duplicate mutations and binds UI effects to this project route.
    // ownedRequest 同步拦截重复变更，并将界面副作用绑定到该项目路由。
    const ownedRequest = requestOwnership.beginMutationRequest();
    if (!ownedRequest) {
      return;
    }
    // files freezes the optional file target before the asynchronous Git mutation starts.
    // files 在异步 Git 变更开始前固定可选文件目标。
    const files = file ? [{
      oldPath: file.oldPath,
      newPath: file.newPath,
      status: file.status,
    }] : [];
    // requestSelection freezes the repository and diff mode owned by this mutation.
    // requestSelection 固定本次变更所属的仓库与 diff 模式。
    const requestSelection = diffReviewSelection;
    const requestSelectionKey = diffReviewSelectionStableKey(requestSelection);
    setWorkspaceGitActionPending(true);
    try {
      // result is the authoritative Git mutation outcome for the captured project route.
      // result 是捕获项目路由对应的权威 Git 变更结果。
      const result = await props.runWorkspaceGitFileAction(
        workspacePath,
        workspaceProjectId,
        activeSurfaceId,
        {
          action,
          files,
          repositoryKey: requestSelection.repositoryKey,
        },
      );
      const selectionStillActive = diffReviewSelectionStableKey(latestDiffReviewSelectionRef.current) === requestSelectionKey;
      if (requestOwnership.ownsMutationRequest(ownedRequest) && selectionStillActive) {
        toast.success(result.summary);
        requestOwnership.invalidateRefreshRequest();
        setWorkspaceReviewLoading(false);
        setWorkspaceReviewSnapshot(null);
        setWorkspaceReviewKey(null);
        requestOwnership.invalidateDetailRequests();
        workspaceDetailAttemptedRef.current.clear();
      }
      // Shared project Git state must refresh even when the user has already navigated away.
      // 即使用户已经离开，也必须刷新共享项目 Git 状态。
      await props.onWorkspaceGitStateChanged?.(
        workspacePath,
        workspaceProjectId,
        activeSurfaceId,
        requestSelection.repositoryKey,
      );
      if (!requestOwnership.ownsMutationRequest(ownedRequest)
        || diffReviewSelectionStableKey(latestDiffReviewSelectionRef.current) !== requestSelectionKey) {
        return;
      }
      await handleRefreshWorkspaceDiff(requestSelection);
    } catch (error) {
      // Preserve the rejected action for its awaiting control; only the active source owns inline feedback.
      // 为等待动作的控件保留拒绝结果；仅当前来源拥有内联反馈。
      if (requestOwnership.ownsMutationRequest(ownedRequest)
        && diffReviewSelectionStableKey(latestDiffReviewSelectionRef.current) === requestSelectionKey) {
        setWorkspaceReviewError(error instanceof Error ? error.message : String(error));
      }
      throw error;
    } finally {
      if (requestOwnership.finishMutationRequest(ownedRequest)) {
        setWorkspaceGitActionPending(false);
      }
    }
  }, [
    activeSurfaceId,
    handleRefreshWorkspaceDiff,
    hydratedReviewPanelCacheKey,
    props.onWorkspaceGitStateChanged,
    props.runWorkspaceGitFileAction,
    requestOwnership,
    reviewPanelCacheKey,
    workspaceGitActionPending,
    workspacePath,
    workspaceProjectId,
  ]);

  // handleDiffReviewSelectionChange applies a new Git review source and refreshes it.
  // handleDiffReviewSelectionChange 应用新的 Git 审核来源并刷新。
  const handleDiffReviewSelectionChange = React.useCallback((selection: DiffReviewSelection) => {
    if (hydratedReviewPanelCacheKey !== reviewPanelCacheKey) {
      return;
    }
    // nextSelectionKey determines whether this action changes source or explicitly refreshes the current source.
    // nextSelectionKey 判断该动作是切换来源，还是显式刷新当前来源。
    const nextSelectionKey = diffReviewSelectionStableKey(selection);
    props.onSelectGitRepository?.(selection.repositoryKey ?? null);
    requestOwnership.invalidateRefreshRequest();
    setWorkspaceReviewLoading(false);
    requestOwnership.invalidateDetailRequests();
    workspaceDetailAttemptedRef.current.clear();
    if (nextSelectionKey === diffReviewSelectionKey) {
      void handleRefreshWorkspaceDiff(selection);
      return;
    }
    setDiffReviewSelection(selection);
    setWorkspaceReviewSnapshot(null);
    setWorkspaceReviewPath(workspacePath);
    setWorkspaceReviewKey(null);
  }, [diffReviewSelectionKey, handleRefreshWorkspaceDiff, hydratedReviewPanelCacheKey, props.onSelectGitRepository, requestOwnership, reviewPanelCacheKey, workspacePath]);

  React.useEffect(() => {
    if (props.reviewFocusRequest.sequence <= 0 || handledReviewFocusRequestRef.current === props.reviewFocusRequest.sequence) {
      return;
    }
    handledReviewFocusRequestRef.current = props.reviewFocusRequest.sequence;
    handleDiffReviewSelectionChange({
      ...DEFAULT_WORKSPACE_REVIEW_SELECTION,
      repositoryKey: props.reviewFocusRequest.repositoryKey,
    });
  }, [handleDiffReviewSelectionChange, props.reviewFocusRequest]);

  React.useEffect(() => {
    if (lastWorkspaceReviewVisibleWorkspacePath !== workspacePath) {
      forceWorkspaceReviewRefreshRef.current = true;
      lastWorkspaceReviewVisibleWorkspacePath = workspacePath;
    }
  }, [workspacePath]);

  React.useEffect(() => {
    if (lastHydratedReviewPanelCacheKeyRef.current === reviewSourceCacheKey) {
      return;
    }
    lastHydratedReviewPanelCacheKeyRef.current = reviewSourceCacheKey;
    pendingReviewSourceHydrationRef.current = reviewSourceCacheKey;
    // A project restores its last source; an uncached repository must retain the user's new selection.
    // 项目恢复上次来源；未缓存的仓库必须保留用户的新选择。
    const projectChanged = lastHydratedWorkspaceCacheKeyRef.current !== reviewPanelCacheKey;
    lastHydratedWorkspaceCacheKeyRef.current = reviewPanelCacheKey;
    const cachedEntry = workspaceReviewPanelCache.get(projectChanged ? reviewPanelCacheKey : reviewSourceCacheKey) ?? null;
    if (cachedEntry) {
      setDiffReviewSelection(cachedEntry.selection);
    } else if (projectChanged) {
      setDiffReviewSelection(DEFAULT_WORKSPACE_REVIEW_SELECTION);
    }
    setWorkspaceReviewSnapshot(cachedEntry?.snapshot ?? null);
    setWorkspaceReviewPath(cachedEntry?.workspacePath ?? workspacePath);
    setWorkspaceReviewKey(cachedEntry?.reviewKey ?? null);
    setWorkspaceReviewError(cachedEntry?.error ?? null);
    setWorkspaceReviewLoading(false);
    requestOwnership.invalidateDetailRequests();
    workspaceDetailAttemptedRef.current.clear();
    setHydratedReviewPanelCacheKey(reviewPanelCacheKey);
  }, [requestOwnership, reviewPanelCacheKey, reviewSourceCacheKey, workspacePath]);

  React.useEffect(() => {
    if (hydratedReviewPanelCacheKey !== reviewPanelCacheKey) {
      return;
    }
    if (pendingReviewSourceHydrationRef.current === reviewSourceCacheKey) {
      pendingReviewSourceHydrationRef.current = null;
      return;
    }
    // Retain both the selected source and its repository snapshot for reopening and project navigation.
    // 同时保留选中来源与仓库快照，支持重开面板和项目导航。
    const cacheEntry: WorkspaceReviewPanelCacheEntry = {
      snapshot: workspaceReviewSnapshot,
      selection: diffReviewSelection,
      workspacePath: workspaceReviewPath,
      reviewKey: workspaceReviewKey,
      error: workspaceReviewError,
    };
    rememberWorkspaceReviewPanelCache(reviewSourceCacheKey, cacheEntry);
    rememberWorkspaceReviewPanelCache(reviewPanelCacheKey, cacheEntry);
  }, [
    diffReviewSelection,
    hydratedReviewPanelCacheKey,
    reviewPanelCacheKey,
    reviewSourceCacheKey,
    workspaceReviewError,
    workspaceReviewKey,
    workspaceReviewPath,
    workspaceReviewSnapshot,
  ]);

  React.useEffect(() => {
    if (hydratedReviewPanelCacheKey !== reviewPanelCacheKey) {
      return;
    }
    const forceRefreshForProject = forceWorkspaceReviewRefreshRef.current;
    if (!workspacePath) {
      forceWorkspaceReviewRefreshRef.current = false;
      setWorkspaceReviewSnapshot(null);
      setWorkspaceReviewPath(null);
      setWorkspaceReviewKey(null);
      setWorkspaceReviewError(null);
      setWorkspaceReviewLoading(false);
      requestOwnership.invalidateDetailRequests();
      workspaceDetailAttemptedRef.current.clear();
      return;
    }
    if (
      workspaceReviewPath === workspacePath
      && workspaceReviewKey === diffReviewSelectionKey
      && workspaceReviewSnapshot
      && !forceRefreshForProject
    ) {
      setWorkspaceReviewError(null);
      return;
    }
    forceWorkspaceReviewRefreshRef.current = false;
    setWorkspaceReviewSnapshot(null);
    setWorkspaceReviewPath(workspacePath);
    setWorkspaceReviewKey(diffReviewSelectionKey);
    requestOwnership.invalidateDetailRequests();
    workspaceDetailAttemptedRef.current.clear();
    void handleRefreshWorkspaceDiff();
  }, [
    diffReviewSelectionKey,
    handleRefreshWorkspaceDiff,
    hydratedReviewPanelCacheKey,
    requestOwnership,
    reviewPanelCacheKey,
    workspacePath,
    workspaceReviewKey,
    workspaceReviewPath,
    workspaceReviewSnapshot,
  ]);

  return (
    <DiffReviewPanel
      emptyBody={workspaceReviewError ?? gitRepositoryCatalogError ?? t("contextPanel.reviewEmpty")}
      emptyTitle={t("contextPanel.review")}
      loading={workspaceReviewLoading}
      selection={diffReviewSelection}
      repositories={enabledGitRepositories}
      snapshot={activeReviewSnapshot}
      onLoadFileDetail={activeWorkspaceReviewSnapshot ? handleLoadWorkspaceDiffFileDetail : undefined}
      onSearchWorkspaceDiff={activeWorkspaceReviewSnapshot ? handleSearchWorkspaceDiff : undefined}
      onCopyRaw={props.onCopyRawDiff}
      gitCommitActionBusy={props.gitCommitActionBusy}
      gitCommitActionDisabled={props.gitCommitActionDisabled}
      gitCommitActionLabel={props.gitCommitActionLabel}
      gitFileActionPending={workspaceGitActionPending}
      onGitFileAction={activeWorkspaceReviewSnapshot ? handleWorkspaceGitFileAction : undefined}
      onOpenGitCommitAction={props.onOpenGitCommitAction}
      onRefresh={handleRefreshWorkspaceDiff}
      onSelectionChange={handleDiffReviewSelectionChange}
    />
  );
}

// workspaceReviewPanelCacheKey builds a project-level cache key for right-panel Git review state.
// workspaceReviewPanelCacheKey 为右侧 Git 审核状态构建项目级缓存键。
function workspaceReviewPanelCacheKey(workspacePath: string | null): string {
  return workspacePath ?? "no-workspace";
}

// workspaceReviewSourceCacheKey builds a bounded cache identity for one project repository source.
// workspaceReviewSourceCacheKey 为单个项目仓库来源构建有界缓存身份。
function workspaceReviewSourceCacheKey(workspacePath: string | null, repositoryKey: string | null | undefined): string {
  return `${workspaceReviewPanelCacheKey(workspacePath)}\u0000${repositoryKey?.trim() || "."}`;
}

// rememberWorkspaceReviewPanelCache records one bounded review panel cache entry.
// rememberWorkspaceReviewPanelCache 记录一条有上限的审核面板缓存。
function rememberWorkspaceReviewPanelCache(key: string, entry: WorkspaceReviewPanelCacheEntry): void {
  workspaceReviewPanelCache.set(key, entry);
  while (workspaceReviewPanelCache.size > workspaceReviewPanelCacheLimit) {
    const oldestKey = workspaceReviewPanelCache.keys().next().value;
    if (typeof oldestKey !== "string") {
      break;
    }
    workspaceReviewPanelCache.delete(oldestKey);
  }
}

// diffHunkExpansionLoadingMessage formats the pending text for targeted hunk expansion.
// diffHunkExpansionLoadingMessage 格式化定向 hunk 展开的等待文案。
function diffHunkExpansionLoadingMessage(expansion: DiffHunkExpansionRequest | undefined, t: (key: TranslationKey) => string): string {
  if (!expansion) {
    return t("workspaceReview.hunkExpanding");
  }
  const directionLabel = expansion.direction === "up"
    ? t("workspaceReview.hunkDirectionUp")
    : expansion.direction === "down"
      ? t("workspaceReview.hunkDirectionDown")
      : t("workspaceReview.hunkDirectionBoth");
  return formatWorkspaceReviewTranslation(t, "workspaceReview.hunkExpandingSpecific", {
    direction: directionLabel,
    index: String(expansion.hunkIndex + 1),
  });
}

// workspaceDiffFileDetailWithTimeout applies the target-compatible lazy detail timeout.
// workspaceDiffFileDetailWithTimeout 应用目标兼容的懒加载详情超时。
function workspaceDiffFileDetailWithTimeout(
  request: (signal: AbortSignal) => Promise<WorkspaceDiffFileDetailResponse>,
  timeoutMs: number,
): Promise<WorkspaceDiffFileDetailResponse> {
  const controller = new AbortController();
  let timeoutId: ReturnType<typeof globalThis.setTimeout> | null = null;
  const timeoutPromise = new Promise<WorkspaceDiffFileDetailResponse>((_, reject) => {
    timeoutId = globalThis.setTimeout(() => {
      controller.abort();
      const error = new Error("WorkspaceDiffFileDetailTimeout");
      error.name = "WorkspaceDiffFileDetailTimeout";
      reject(error);
    }, timeoutMs);
  });
  return Promise.race([request(controller.signal), timeoutPromise]).finally(() => {
    if (timeoutId != null) {
      globalThis.clearTimeout(timeoutId);
    }
    controller.abort();
  });
}

// isWorkspaceDiffFileDetailTimeout returns whether an error came from the detail timeout guard.
// isWorkspaceDiffFileDetailTimeout 返回错误是否来自详情超时保护。
function isWorkspaceDiffFileDetailTimeout(error: unknown): boolean {
  return error instanceof Error && error.name === "WorkspaceDiffFileDetailTimeout";
}

// workspaceDiffSnapshotWithFileLoadState updates one file's lazy detail state immutably.
// workspaceDiffSnapshotWithFileLoadState 不可变地更新单个文件的懒加载详情状态。
function workspaceDiffSnapshotWithFileLoadState(
  snapshot: DiffPanelSnapshot,
  fileKey: string,
  fileDiffLoadState: NonNullable<DiffFile["fileDiffLoadState"]>,
): DiffPanelSnapshot {
  const fileIndex = snapshot.files.findIndex((file) => file.key === fileKey);
  if (fileIndex < 0) {
    return snapshot;
  }
  const files = [...snapshot.files];
  files[fileIndex] = {
    ...files[fileIndex],
    fileDiffLoadState,
  };
  return {
    ...snapshot,
    files,
  };
}

// formatWorkspaceReviewTranslation replaces named placeholders in localized review messages.
// formatWorkspaceReviewTranslation 替换审核面板本地化消息中的具名占位符。
function formatWorkspaceReviewTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}
