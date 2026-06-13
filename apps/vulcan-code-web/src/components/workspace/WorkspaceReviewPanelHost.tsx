import React from "react";
import { toast } from "sonner";
import { DiffReviewPanel } from "@/diff/DiffReviewPanel";
import {
  activeWorkbenchSurfaceId,
  diffReviewSelectionStableKey,
  diffSnapshotFromWorkspaceResponse,
  latestConversationDiffSnapshot,
  selectedConversationProjectId,
  selectedConversationWorkspacePath,
  type WorkspaceReviewConversationBlock,
  type WorkspaceReviewDraftConversation,
} from "@/diff/workspace-review-model";
import type {
  DiffPanelSnapshot,
  DiffReviewSelection,
  WorkspaceDiffSnapshotResponse,
} from "@/diff/types";
import { useI18n } from "@/i18n";
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
}) {
  // t translates the review panel shell labels.
  // t 翻译审核面板外壳标签。
  const { t } = useI18n();
  // reviewSnapshot stores the newest renderable diff for the selected conversation.
  // reviewSnapshot 保存当前选中会话中最新的可渲染 diff。
  const reviewSnapshot = React.useMemo(
    () => latestConversationDiffSnapshot(props.workbench, props.selectedConversationId, props.parseConversationBlocks),
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
    () => initialReviewPanelCache?.selection ?? { mode: "branch" },
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
  // workspaceReviewRequestSeqRef prevents stale async refresh results from replacing the current project diff.
  // workspaceReviewRequestSeqRef 防止过期异步刷新结果覆盖当前项目 diff。
  const workspaceReviewRequestSeqRef = React.useRef(0);
  // hydratedReviewPanelCacheKeyRef prevents stale state from being saved under a newly selected surface.
  // hydratedReviewPanelCacheKeyRef 防止旧状态被写入新选中界面的缓存桶。
  const hydratedReviewPanelCacheKeyRef = React.useRef(reviewPanelCacheKey);
  // forceWorkspaceReviewRefreshRef marks real conversation switches while ignoring simple collapse/reopen cycles.
  // forceWorkspaceReviewRefreshRef 标记真实会话切换，同时忽略单纯折叠再展开。
  const forceWorkspaceReviewRefreshRef = React.useRef(false);
  const activeWorkspaceReviewSnapshot = workspaceReviewSurfaceId === activeSurfaceId
    && workspaceReviewPath === workspacePath
    && workspaceReviewKey === diffReviewSelectionKey
      ? workspaceReviewSnapshot
      : null;
  const activeReviewSnapshot = activeWorkspaceReviewSnapshot ?? reviewSnapshot;

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
      setWorkspaceReviewSnapshot(diffSnapshotFromWorkspaceResponse(response));
      setWorkspaceReviewSurfaceId(activeSurfaceId);
      setWorkspaceReviewPath(workspacePath);
      setWorkspaceReviewKey(requestKey);
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

  // handleDiffReviewSelectionChange applies a new Git review source and refreshes it.
  // handleDiffReviewSelectionChange 应用新的 Git 审核来源并刷新。
  const handleDiffReviewSelectionChange = React.useCallback((selection: DiffReviewSelection) => {
    setDiffReviewSelection(selection);
    setWorkspaceReviewSnapshot(null);
    setWorkspaceReviewSurfaceId(activeSurfaceId);
    setWorkspaceReviewPath(workspacePath);
    setWorkspaceReviewKey(null);
    void handleRefreshWorkspaceDiff(selection);
  }, [activeSurfaceId, handleRefreshWorkspaceDiff, workspacePath]);

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
    setDiffReviewSelection(cachedEntry?.selection ?? { mode: "branch" });
    setWorkspaceReviewSnapshot(cachedEntry?.snapshot ?? null);
    setWorkspaceReviewSurfaceId(cachedEntry?.surfaceId ?? activeSurfaceId);
    setWorkspaceReviewPath(cachedEntry?.workspacePath ?? workspacePath);
    setWorkspaceReviewKey(cachedEntry?.reviewKey ?? null);
    setWorkspaceReviewError(cachedEntry?.error ?? null);
    setWorkspaceReviewLoading(false);
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
      emptyBody={workspaceReviewError ?? t("contextPanel.reviewEmpty")}
      emptyTitle={t("contextPanel.review")}
      loading={workspaceReviewLoading}
      selection={diffReviewSelection}
      snapshot={activeReviewSnapshot}
      onCopyRaw={props.onCopyRawDiff}
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
