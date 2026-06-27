import React from "react";
import { toast } from "sonner";
import { DiffReviewPanel } from "@/diff/DiffReviewPanel";
import { parseUnifiedDiffAsync } from "@/diff/diff-parse-worker-client";
import {
  activeWorkbenchSurfaceId,
  diffFileFromWorkspaceDetailAsync,
  diffReviewSelectionStableKey,
  diffSnapshotFromWorkspaceResponseAsync,
  diffSnapshotWithWorkspaceDetailFile,
  latestConversationDiffSource,
  selectedConversationProjectId,
  selectedConversationWorkspacePath,
  type WorkspaceReviewConversationBlock,
  type WorkspaceReviewConversationDiffSource,
  type WorkspaceReviewDraftConversation,
} from "@/diff/workspace-review-model";
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
} from "@/diff/types";
import { useI18n } from "@/i18n";
import type { WorkspaceDiffFileDetailLoadOptions } from "@/service/workbench-snapshot-service";
import type {
  ConversationMessage,
  WorkbenchSnapshot,
} from "@/types";

// WorkspaceReviewPanelCacheEntry stores one mounted review panel state across close and reopen cycles.
// WorkspaceReviewPanelCacheEntry 保存一次右侧审核面板在关闭与重开之间需要复用的状态。
interface WorkspaceReviewPanelCacheEntry {
  // Latest workspace diff snapshot rendered by the review panel.
  // 审核面板最近渲染的工作区 diff 快照。
  snapshot: DiffPanelSnapshot | null;
  // Git review source selection used by the cached snapshot.
  // 缓存快照对应的 Git 审核来源选择。
  selection: DiffReviewSelection;
  // Conversation or subagent surface that produced the cached snapshot.
  // 生成缓存快照的会话或子代理界面标识。
  surfaceId: string | null;
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

// lastWorkspaceReviewVisibleSurfaceId tracks the latest conversation surface that the review panel represented.
// lastWorkspaceReviewVisibleSurfaceId 跟踪审核面板最近代表的会话界面。
let lastWorkspaceReviewVisibleSurfaceId: string | null = null;

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
  reviewFocusRequest: number;
  // Copies raw diff text using the host-aware clipboard implementation.
  // 使用宿主感知的剪贴板实现复制原始 diff 文本。
  onCopyRawDiff: (rawDiff: string) => void;
  // Parses one conversation message into render blocks.
  // 将单条会话消息解析为渲染块。
  parseConversationBlocks: (message: ConversationMessage) => WorkspaceReviewConversationBlock[];
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
}) {
  // t translates the review panel shell labels.
  // t 翻译审核面板外壳标签。
  const { t } = useI18n();
  // conversationDiffSource stores the newest raw diff block for the selected conversation.
  // conversationDiffSource 保存当前选中会话中最新的原始 diff 块。
  const conversationDiffSource = React.useMemo(
    () => latestConversationDiffSource(props.workbench, props.selectedConversationId, props.parseConversationBlocks),
    [props.selectedConversationId, props.workbench, props.parseConversationBlocks],
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
  // reviewPanelCacheKey stores the cache bucket for this visible conversation surface.
  // reviewPanelCacheKey 保存当前可见会话界面的审核缓存桶。
  const reviewPanelCacheKey = React.useMemo(
    () => workspaceReviewPanelCacheKey(activeSurfaceId, workspacePath),
    [activeSurfaceId, workspacePath],
  );
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
  // workspaceReviewSurfaceId stores the surface that produced the current workspace diff snapshot.
  // workspaceReviewSurfaceId 保存生成当前工作区 diff 快照的界面标识。
  const [workspaceReviewSurfaceId, setWorkspaceReviewSurfaceId] = React.useState<string | null>(
    () => initialReviewPanelCache?.surfaceId ?? null,
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
  // workspaceGitActionPending tracks one in-flight Git mutation from the review panel.
  // workspaceGitActionPending 跟踪审核面板发起的一次进行中的 Git 变更操作。
  const [workspaceGitActionPending, setWorkspaceGitActionPending] = React.useState(false);
  // conversationReviewSnapshot stores the asynchronously parsed conversation diff snapshot.
  // conversationReviewSnapshot 保存异步解析后的会话 diff 快照。
  const [conversationReviewSnapshot, setConversationReviewSnapshot] = React.useState<DiffPanelSnapshot | null>(null);
  // conversationReviewLoading tracks async parsing of conversation diff blocks.
  // conversationReviewLoading 跟踪会话 diff 块的异步解析状态。
  const [conversationReviewLoading, setConversationReviewLoading] = React.useState(false);
  // conversationReviewError stores the latest conversation diff parse error.
  // conversationReviewError 保存最近一次会话 diff 解析错误。
  const [conversationReviewError, setConversationReviewError] = React.useState<string | null>(null);
  // workspaceReviewRequestSeqRef prevents stale async refresh results from replacing the current project diff.
  // workspaceReviewRequestSeqRef 防止过期异步刷新结果覆盖当前项目 diff。
  const workspaceReviewRequestSeqRef = React.useRef(0);
  // conversationReviewParseSeqRef prevents stale async message diff parses from replacing current state.
  // conversationReviewParseSeqRef 防止过期会话 diff 异步解析结果覆盖当前状态。
  const conversationReviewParseSeqRef = React.useRef(0);
  // hydratedReviewPanelCacheKeyRef prevents stale state from being saved under a newly selected surface.
  // hydratedReviewPanelCacheKeyRef 防止旧状态被写入新选中界面的缓存桶。
  const hydratedReviewPanelCacheKeyRef = React.useRef(reviewPanelCacheKey);
  // forceWorkspaceReviewRefreshRef marks real conversation switches while ignoring simple collapse/reopen cycles.
  // forceWorkspaceReviewRefreshRef 标记真实会话切换，同时忽略单纯折叠再展开。
  const forceWorkspaceReviewRefreshRef = React.useRef(false);
  // workspaceDetailPendingRef prevents duplicate lazy file-detail requests.
  // workspaceDetailPendingRef 防止重复发起文件详情懒加载请求。
  const workspaceDetailPendingRef = React.useRef(new Set<string>());
  // workspaceDetailAttemptedRef records file details already attempted for this snapshot.
  // workspaceDetailAttemptedRef 记录当前快照中已经尝试加载过详情的文件。
  const workspaceDetailAttemptedRef = React.useRef(new Set<string>());
  // handledReviewFocusRequestRef stores the latest external focus request already applied.
  // handledReviewFocusRequestRef 保存已经应用的最近一次外部聚焦请求。
  const handledReviewFocusRequestRef = React.useRef(0);
  const activeWorkspaceReviewSnapshot = workspaceReviewSurfaceId === activeSurfaceId
    && workspaceReviewPath === workspacePath
    && workspaceReviewKey === diffReviewSelectionKey
      ? workspaceReviewSnapshot
      : null;
  const activeReviewSnapshot = activeWorkspaceReviewSnapshot ?? conversationReviewSnapshot;

  React.useEffect(() => {
    const requestSeq = conversationReviewParseSeqRef.current + 1;
    conversationReviewParseSeqRef.current = requestSeq;
    if (!conversationDiffSource) {
      setConversationReviewSnapshot(null);
      setConversationReviewLoading(false);
      setConversationReviewError(null);
      return undefined;
    }
    let cancelled = false;
    setConversationReviewSnapshot(null);
    setConversationReviewLoading(true);
    setConversationReviewError(null);
    void parseConversationDiffSourceAsync(conversationDiffSource)
      .then((snapshot) => {
        if (!cancelled && conversationReviewParseSeqRef.current === requestSeq) {
          setConversationReviewSnapshot(snapshot);
          setConversationReviewError(null);
        }
      })
      .catch((error) => {
        if (!cancelled && conversationReviewParseSeqRef.current === requestSeq) {
          setConversationReviewSnapshot(null);
          setConversationReviewError(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (!cancelled && conversationReviewParseSeqRef.current === requestSeq) {
          setConversationReviewLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    conversationDiffSource?.key,
    conversationDiffSource?.rawDiff,
    conversationDiffSource?.source,
  ]);

  // handleRefreshWorkspaceDiff refreshes the current project Git diff through the local service.
  // handleRefreshWorkspaceDiff 通过本地服务刷新当前项目 Git diff。
  const handleRefreshWorkspaceDiff = React.useCallback(async (selectionOverride?: DiffReviewSelection) => {
    if (!workspacePath) {
      setWorkspaceReviewError("当前会话没有关联项目路径，无法刷新 Git diff。");
      return;
    }
    const requestSelection = selectionOverride ?? diffReviewSelection;
    const requestKey = diffReviewSelectionStableKey(requestSelection);
    const requestSeq = workspaceReviewRequestSeqRef.current + 1;
    workspaceReviewRequestSeqRef.current = requestSeq;
    setWorkspaceReviewLoading(true);
    setWorkspaceReviewError(null);
    try {
      const response = await props.loadWorkspaceDiffSnapshot(workspacePath, workspaceProjectId, activeSurfaceId, requestSelection);
      if (workspaceReviewRequestSeqRef.current !== requestSeq) {
        return;
      }
      const snapshot = await diffSnapshotFromWorkspaceResponseAsync(response);
      if (workspaceReviewRequestSeqRef.current !== requestSeq) {
        return;
      }
      setWorkspaceReviewSnapshot(snapshot);
      setWorkspaceReviewSurfaceId(activeSurfaceId);
      setWorkspaceReviewPath(workspacePath);
      setWorkspaceReviewKey(requestKey);
      workspaceDetailPendingRef.current.clear();
      workspaceDetailAttemptedRef.current.clear();
    } catch (error) {
      if (workspaceReviewRequestSeqRef.current !== requestSeq) {
        return;
      }
      const message = error instanceof Error ? error.message : String(error);
      setWorkspaceReviewError(message);
      toast.error(`刷新 Git diff 失败：${message}`);
    } finally {
      if (workspaceReviewRequestSeqRef.current === requestSeq) {
        setWorkspaceReviewLoading(false);
      }
    }
  }, [activeSurfaceId, diffReviewSelection, props, workspacePath, workspaceProjectId]);

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
    if (workspaceDetailPendingRef.current.has(detailKey) || (workspaceDetailAttemptedRef.current.has(detailKey) && !retryable)) {
      return;
    }
    workspaceDetailPendingRef.current.add(detailKey);
    workspaceDetailAttemptedRef.current.add(detailKey);
    setWorkspaceReviewSnapshot((previous) => previous
      ? workspaceDiffSnapshotWithFileLoadState(previous, file.key, {
        status: "loading",
        message: contextLines
          ? diffHunkExpansionLoadingMessage(expansion)
          : "正在加载文件 diff 详情。",
      })
      : previous);
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
      const detailFile = await diffFileFromWorkspaceDetailAsync(response, file, expansion);
      setWorkspaceReviewSnapshot((previous) => {
        if (!previous) {
          return previous;
        }
        return diffSnapshotWithWorkspaceDetailFile(previous, detailFile, file) ?? previous;
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      const timedOut = isWorkspaceDiffFileDetailTimeout(error);
      setWorkspaceReviewSnapshot((previous) => previous
        ? workspaceDiffSnapshotWithFileLoadState(previous, file.key, {
          status: timedOut ? "timed-out" : "failed",
          message,
        })
        : previous);
      toast.error(timedOut ? `加载文件 diff 详情超时：${message}` : `加载文件 diff 详情失败：${message}`);
    } finally {
      workspaceDetailPendingRef.current.delete(detailKey);
    }
  }, [
    activeSurfaceId,
    activeWorkspaceReviewSnapshot,
    diffReviewSelection,
    diffReviewSelectionKey,
    props,
    workspacePath,
    workspaceProjectId,
  ]);

  // handleSearchWorkspaceDiff loads backend file-level search hits for the active live diff.
  // handleSearchWorkspaceDiff 为当前实时 diff 加载后端文件级搜索命中。
  const handleSearchWorkspaceDiff = React.useCallback(async (query: string, limit?: number) => {
    if (!workspacePath || !activeWorkspaceReviewSnapshot) {
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
    props,
    workspacePath,
    workspaceProjectId,
  ]);

  // handleWorkspaceGitFileAction executes a Git mutation and refreshes the active review snapshot.
  // handleWorkspaceGitFileAction 执行一次 Git 变更操作并刷新当前审核快照。
  const handleWorkspaceGitFileAction = React.useCallback(async (
    action: WorkspaceGitFileActionKind,
    file?: DiffFile | null,
  ) => {
    if (workspaceGitActionPending) {
      return;
    }
    if (!workspacePath) {
      toast.error("当前会话没有关联项目路径，无法执行 Git 操作。");
      return;
    }
    const files = file ? [{
      oldPath: file.oldPath,
      newPath: file.newPath,
      status: file.status,
    }] : [];
    setWorkspaceGitActionPending(true);
    try {
      const result = await props.runWorkspaceGitFileAction(
        workspacePath,
        workspaceProjectId,
        activeSurfaceId,
        { action, files },
      );
      toast.success(result.summary);
      setWorkspaceReviewSnapshot(null);
      setWorkspaceReviewKey(null);
      workspaceDetailPendingRef.current.clear();
      workspaceDetailAttemptedRef.current.clear();
      await handleRefreshWorkspaceDiff(diffReviewSelection);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error(`Git 操作失败：${message}`);
    } finally {
      setWorkspaceGitActionPending(false);
    }
  }, [
    activeSurfaceId,
    diffReviewSelection,
    handleRefreshWorkspaceDiff,
    props,
    workspaceGitActionPending,
    workspacePath,
    workspaceProjectId,
  ]);

  // handleDiffReviewSelectionChange applies a new Git review source and refreshes it.
  // handleDiffReviewSelectionChange 应用新的 Git 审核来源并刷新。
  const handleDiffReviewSelectionChange = React.useCallback((selection: DiffReviewSelection) => {
    setDiffReviewSelection(selection);
    setWorkspaceReviewSnapshot(null);
    setWorkspaceReviewSurfaceId(activeSurfaceId);
    setWorkspaceReviewPath(workspacePath);
    setWorkspaceReviewKey(null);
    workspaceDetailPendingRef.current.clear();
    workspaceDetailAttemptedRef.current.clear();
    void handleRefreshWorkspaceDiff(selection);
  }, [activeSurfaceId, handleRefreshWorkspaceDiff, workspacePath]);

  React.useEffect(() => {
    if (props.reviewFocusRequest <= 0 || handledReviewFocusRequestRef.current === props.reviewFocusRequest) {
      return;
    }
    handledReviewFocusRequestRef.current = props.reviewFocusRequest;
    handleDiffReviewSelectionChange(DEFAULT_WORKSPACE_REVIEW_SELECTION);
  }, [handleDiffReviewSelectionChange, props.reviewFocusRequest]);

  React.useEffect(() => {
    if (lastWorkspaceReviewVisibleSurfaceId !== activeSurfaceId) {
      forceWorkspaceReviewRefreshRef.current = true;
      lastWorkspaceReviewVisibleSurfaceId = activeSurfaceId;
    }
  }, [activeSurfaceId]);

  React.useEffect(() => {
    if (hydratedReviewPanelCacheKeyRef.current === reviewPanelCacheKey) {
      return;
    }
    hydratedReviewPanelCacheKeyRef.current = reviewPanelCacheKey;
    workspaceReviewRequestSeqRef.current += 1;
    const cachedEntry = workspaceReviewPanelCache.get(reviewPanelCacheKey) ?? null;
    setDiffReviewSelection(cachedEntry?.selection ?? DEFAULT_WORKSPACE_REVIEW_SELECTION);
    setWorkspaceReviewSnapshot(cachedEntry?.snapshot ?? null);
    setWorkspaceReviewSurfaceId(cachedEntry?.surfaceId ?? activeSurfaceId);
    setWorkspaceReviewPath(cachedEntry?.workspacePath ?? workspacePath);
    setWorkspaceReviewKey(cachedEntry?.reviewKey ?? null);
    setWorkspaceReviewError(cachedEntry?.error ?? null);
    setWorkspaceReviewLoading(false);
    workspaceDetailPendingRef.current.clear();
    workspaceDetailAttemptedRef.current.clear();
  }, [activeSurfaceId, reviewPanelCacheKey, workspacePath]);

  React.useEffect(() => {
    if (hydratedReviewPanelCacheKeyRef.current !== reviewPanelCacheKey) {
      return;
    }
    rememberWorkspaceReviewPanelCache(reviewPanelCacheKey, {
      snapshot: workspaceReviewSnapshot,
      selection: diffReviewSelection,
      surfaceId: workspaceReviewSurfaceId,
      workspacePath: workspaceReviewPath,
      reviewKey: workspaceReviewKey,
      error: workspaceReviewError,
    });
  }, [
    diffReviewSelection,
    reviewPanelCacheKey,
    workspaceReviewError,
    workspaceReviewKey,
    workspaceReviewPath,
    workspaceReviewSnapshot,
    workspaceReviewSurfaceId,
  ]);

  React.useEffect(() => {
    if (hydratedReviewPanelCacheKeyRef.current !== reviewPanelCacheKey) {
      return;
    }
    const forceRefreshForSurface = forceWorkspaceReviewRefreshRef.current;
    if (!workspacePath) {
      forceWorkspaceReviewRefreshRef.current = false;
      setWorkspaceReviewSnapshot(null);
      setWorkspaceReviewSurfaceId(activeSurfaceId);
      setWorkspaceReviewPath(null);
      setWorkspaceReviewKey(null);
      setWorkspaceReviewError(null);
      setWorkspaceReviewLoading(false);
      workspaceDetailPendingRef.current.clear();
      workspaceDetailAttemptedRef.current.clear();
      return;
    }
    if (
      workspaceReviewSurfaceId === activeSurfaceId
      && workspaceReviewPath === workspacePath
      && workspaceReviewKey === diffReviewSelectionKey
      && workspaceReviewSnapshot
      && !forceRefreshForSurface
    ) {
      setWorkspaceReviewError(null);
      return;
    }
    forceWorkspaceReviewRefreshRef.current = false;
    setWorkspaceReviewSnapshot(null);
    setWorkspaceReviewSurfaceId(activeSurfaceId);
    setWorkspaceReviewPath(workspacePath);
    setWorkspaceReviewKey(diffReviewSelectionKey);
    workspaceDetailPendingRef.current.clear();
    workspaceDetailAttemptedRef.current.clear();
    void handleRefreshWorkspaceDiff();
  }, [
    activeSurfaceId,
    diffReviewSelectionKey,
    handleRefreshWorkspaceDiff,
    reviewPanelCacheKey,
    workspacePath,
    workspaceReviewKey,
    workspaceReviewPath,
    workspaceReviewSnapshot,
    workspaceReviewSurfaceId,
  ]);

  return (
    <DiffReviewPanel
      emptyBody={workspaceReviewError ?? conversationReviewError ?? t("contextPanel.reviewEmpty")}
      emptyTitle={t("contextPanel.review")}
      loading={workspaceReviewLoading || (!activeWorkspaceReviewSnapshot && conversationReviewLoading)}
      selection={diffReviewSelection}
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

// workspaceReviewPanelCacheKey builds a per-surface cache key for right-panel review state.
// workspaceReviewPanelCacheKey 为右侧审核状态构建按界面隔离的缓存键。
function workspaceReviewPanelCacheKey(surfaceId: string | null, workspacePath: string | null): string {
  return `${surfaceId ?? "none"}::${workspacePath ?? "no-workspace"}`;
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

// parseConversationDiffSourceAsync parses one conversation diff through the worker-only pipeline.
// parseConversationDiffSourceAsync 通过 Worker 单轨流水线解析单个会话 diff。
function parseConversationDiffSourceAsync(source: WorkspaceReviewConversationDiffSource): Promise<DiffPanelSnapshot> {
  return parseUnifiedDiffAsync(source.rawDiff, source.source, "");
}

// diffHunkExpansionLoadingMessage formats the pending text for targeted hunk expansion.
// diffHunkExpansionLoadingMessage 格式化定向 hunk 展开的等待文案。
function diffHunkExpansionLoadingMessage(expansion: DiffHunkExpansionRequest | undefined): string {
  if (!expansion) {
    return "正在展开 hunk 上下文。";
  }
  const directionLabel = expansion.direction === "up"
    ? "向上"
    : expansion.direction === "down"
      ? "向下"
      : "双向";
  return `正在${directionLabel}展开 hunk ${expansion.hunkIndex + 1} 上下文。`;
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
      const error = new Error(`单文件 diff 详情加载超过 ${Math.round(timeoutMs / 1000)} 秒。`);
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
