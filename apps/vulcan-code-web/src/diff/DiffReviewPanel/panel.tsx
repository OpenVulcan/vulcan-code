import React from "react";
import { GitCompare, RefreshCw } from "lucide-react";
import { PageEmptyState } from "@/components/page/page-feedback";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { toast } from "@/ui-kit/primitives/toast";

import { useI18n, type TranslationKey } from "@/i18n";
import type { WorkspacePathAction } from "@/components/workspace/workspace-path-actions";
import { openProjectDirectoryPath } from "@/service/project-actions-service";
import { DiffFileCard, useDiffScrollState } from "../diff-file-card";
import { DiffPanelToolbar } from "../diff-review-toolbar";
import { DIFF_SEARCH_DETAIL_PREFETCH_TARGET_MATCHES, DIFF_SEARCH_SUMMARY_FILE_MATCH_LIMIT, DIFF_SEARCH_SUMMARY_REQUEST_DEBOUNCE_MS } from "../limits";
import type { DiffFile, DiffPanelSnapshot, DiffRenderPreferences, DiffReviewSelection, DiffViewMode, WorkspaceDiffSearchResponse, WorkspaceGitFileActionKind, WorkspaceGitRepositorySummary } from "../types";
import { clampDiffSearchIndex, diffSearchVisiblePage } from "../diff-search";
import { useDiffSearchIndex } from "../diff-search-worker-client";
import {
  EMPTY_DIFF_FILE_VIRTUAL_LAYOUT,
  buildDiffFileVirtualLayout,
  mountedDiffFiles,
} from "../diff-render-index";
import "../diffReview.css";
import { DiffFileBar, diffFilesInFolderOrder } from "./file-bar";
import { DiffReviewFloatingGitActions } from "./floating-git-actions";
import { initialDiffRenderPreferences, initialDiffViewMode, persistDiffDisplayPreferences } from "./preferences";
import {
  EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS,
  EMPTY_DIFF_SUMMARY_SEARCH_STATE,
  EMPTY_SUMMARY_SEARCH_MATCHES,
  activeDiffSearchLineKeyForFile,
  diffAbsoluteFilePath,
  diffCollapsedFileKeys,
  diffDetailHydrationRequestsWithoutDuplicates,
  diffReviewScrollResetKey,
  diffSearchDetailPrefetchCandidateForFile,
  diffSearchDetailPrefetchCandidates,
  diffSearchIndexForSummaryTarget,
  diffSummarySearchDerived,
  diffSummarySearchRequestKey,
  diffSummarySearchTargetKey,
  nextSearchIndex,
  previousSearchIndex,
  scheduleDiffDetailHydrationRequests,
  scrollToDiffSearchMatch,
  scrollToDiffSummarySearchMatch,
  searchResultLabel,
  shouldLoadDiffFileDetail,
  type DiffSearchDetailPrefetchCandidate,
  type DiffSummarySearchState,
} from "./search";
import type { DiffFileDetailLoader } from "./types";
// DiffReviewPanel renders one structured Git diff snapshot.
// DiffReviewPanel 渲染单个结构化 Git diff 快照。
export function DiffReviewPanel(props: {
  // Parsed diff snapshot to render.
  // 要渲染的已解析 diff 快照。
  snapshot: DiffPanelSnapshot | null;
  // Whether the panel is currently loading.
  // 面板当前是否正在加载。
  loading?: boolean;
  // Current Git review source selection.
  // 当前 Git 审核来源选择。
  selection?: DiffReviewSelection;
  // Discovered repositories available for right-panel selection.
  // 右侧面板可选择的已发现仓库。
  repositories?: WorkspaceGitRepositorySummary[];
  // Optional empty-state title.
  // 可选空状态标题。
  emptyTitle?: string;
  // Optional empty-state body.
  // 可选空状态正文。
  emptyBody?: string;
  // Optional refresh action.
  // 可选刷新动作。
  onRefresh?: () => void | Promise<void>;
  // Optional selection change handler.
  // 可选选择变更处理器。
  onSelectionChange?: (selection: DiffReviewSelection) => void;
  // Optional copy action receiving raw diff text.
  // 接收原始 diff 文本的可选复制动作。
  onCopyRaw?: (rawDiff: string) => void | Promise<void>;
  // Optional actions for the shown file, given its local path when the repository root is known.
  // 当前文件的可选操作；已知仓库根目录时传入其本地路径。
  fileActions?: (file: DiffFile, absolutePath: string | null) => WorkspacePathAction[];
  // Optional lazy detail loader for partial summaries and hunk context expansion.
  // 用于局部摘要与 hunk 上下文展开的可选懒加载详情函数。
  onLoadFileDetail?: DiffFileDetailLoader;
  // Optional backend compact diff search loader for summary-only hydration.
  // 用于摘要文件水合的可选后端紧凑 diff 搜索加载器。
  onSearchWorkspaceDiff?: (query: string, limit?: number) => Promise<WorkspaceDiffSearchResponse | null>;
  // Whether a Git file action is currently running.
  // 是否有 Git 文件操作正在运行。
  gitFileActionPending?: boolean;
  // Optional shared Git commit action label shown in the toolbar.
  // 工具栏中展示的可选共享 Git 提交动作标签。
  gitCommitActionLabel?: string;
  // Whether the shared Git commit action is currently running.
  // 共享 Git 提交动作当前是否正在运行。
  gitCommitActionBusy?: boolean;
  // Whether the shared Git commit action is currently unavailable.
  // 共享 Git 提交动作当前是否不可用。
  gitCommitActionDisabled?: boolean;
  // Optional file-level Git action dispatcher for live workspace diffs.
  // 用于实时工作区 diff 的可选文件级 Git 操作派发器。
  onGitFileAction?: (action: WorkspaceGitFileActionKind, file?: DiffFile | null) => void | Promise<void>;
  // Opens the shared Git commit or push dialog.
  // 打开共享 Git 提交或推送弹窗。
  onOpenGitCommitAction?: () => void;
}) {
  const { t } = useI18n();
  const fileListRef = React.useRef<HTMLDivElement | null>(null);
  const summarySearchRequestSeqRef = React.useRef(0);
  const summarySearchResolvedKeyRef = React.useRef<string | null>(null);
  const summarySearchTargetAppliedKeyRef = React.useRef<string | null>(null);
  // searchScrollAppliedRequestRef prevents one explicit navigation request from scrolling repeatedly after layout updates.
  // searchScrollAppliedRequestRef 防止一次显式导航请求在布局更新后重复滚动。
  const searchScrollAppliedRequestRef = React.useRef(0);
  const [scrollState, handleFileListScroll, syncFileListScrollState] = useDiffScrollState(fileListRef);
  const snapshot = props.snapshot;
  const selection = props.selection ?? { mode: "unstaged" };
  const onRefresh = props.onRefresh;
  const onLoadFileDetail = props.onLoadFileDetail;
  const onSearchWorkspaceDiff = props.onSearchWorkspaceDiff;
  const [searchQuery, setSearchQuery] = React.useState("");
  const [activeSearchIndex, setActiveSearchIndex] = React.useState(0);
  // searchScrollRequest increments only for Enter or step-button navigation.
  // searchScrollRequest 仅在 Enter 或上下条按钮导航时递增。
  const [searchScrollRequest, setSearchScrollRequest] = React.useState(0);
  const [viewMode, setViewMode] = React.useState<DiffViewMode>(initialDiffViewMode);
  const [renderPreferences, setRenderPreferences] = React.useState<DiffRenderPreferences>(initialDiffRenderPreferences);
  const [expandedFileKey, setExpandedFileKey] = React.useState<string | null>(null);
  // refreshPending stores the immediate local refresh lock before parent loading propagates.
  // refreshPending 保存父级 loading 传入前的本地即时刷新锁。
  const [refreshPending, setRefreshPending] = React.useState(false);
  // refreshPendingRef closes the repeat-click gap before React renders the pending state.
  // refreshPendingRef 关闭 React 渲染等待状态前的重复点击窗口。
  const refreshPendingRef = React.useRef(false);
  const [summarySearchState, setSummarySearchState] = React.useState<DiffSummarySearchState>(EMPTY_DIFF_SUMMARY_SEARCH_STATE);
  const diffSourceScrollResetKey = React.useMemo(
    () => diffReviewScrollResetKey(snapshot, selection),
    [selection, snapshot],
  );
  // renderedSnapshot must not be narrowed by search because the diff body height must stay stable while typing.
  // renderedSnapshot 不能被搜索缩窄，因为输入搜索内容时 diff 主体高度必须保持稳定。
  const renderedSnapshot = snapshot;
  // orderedFiles is the order the file bar, its menu and Alt+Up/Down all walk.
  // orderedFiles 是文件栏、其菜单与 Alt+上/下 共同遵循的顺序。
  const orderedFiles = React.useMemo(() => diffFilesInFolderOrder(renderedSnapshot?.files ?? []), [renderedSnapshot]);
  // The review always shows one file: the chosen one while it is still in the diff, otherwise the first file.
  // 审核始终展示一个文件：所选文件仍在 diff 中时展示它，否则展示第一个文件。
  const selectedDiffFile = React.useMemo(
    () => orderedFiles.find((file) => file.key === expandedFileKey) ?? orderedFiles[0] ?? null,
    [expandedFileKey, orderedFiles],
  );
  const effectiveExpandedFileKey = selectedDiffFile?.key ?? null;
  const selectedDiffSnapshot = React.useMemo(
    () => renderedSnapshot && selectedDiffFile
      ? { ...renderedSnapshot, files: [selectedDiffFile] }
      : null,
    [renderedSnapshot, selectedDiffFile],
  );
  // searchScopeSnapshot limits search to the file body currently shown in the review pane.
  // searchScopeSnapshot 将搜索限制在审核面板当前实际展示的文件正文内。
  const searchScopeSnapshot = selectedDiffSnapshot;
  const { index: searchIndex, pending: searchPending, error: searchWorkerError } = useDiffSearchIndex(searchScopeSnapshot, searchQuery);
  const safeActiveSearchIndex = clampDiffSearchIndex(activeSearchIndex, searchIndex.matches.length);
  const activeSearchMatch = searchIndex.matches[safeActiveSearchIndex] ?? null;
  const summarySearchRequestKey = React.useMemo(
    () => diffSummarySearchRequestKey(searchScopeSnapshot, selection, searchQuery),
    [searchQuery, searchScopeSnapshot, selection],
  );
  const activeSummarySearchMatches = summarySearchState.requestKey === summarySearchRequestKey
    ? summarySearchState.matches
    : EMPTY_SUMMARY_SEARCH_MATCHES;
  const summarySearchDerived = React.useMemo(
    () => diffSummarySearchDerived(activeSummarySearchMatches),
    [activeSummarySearchMatches],
  );
  const summarySearchMatchByFileKey = summarySearchDerived.matchByFileKey;
  const searchVisiblePage = React.useMemo(
    () => diffSearchVisiblePage(searchIndex.matches, safeActiveSearchIndex),
    [safeActiveSearchIndex, searchIndex.matches],
  );
  const searchVisibleLineKeys = searchVisiblePage.lineKeys;
  const selectedCollapsedFileKeys = React.useMemo(
    () => diffCollapsedFileKeys(selectedDiffSnapshot, effectiveExpandedFileKey),
    [effectiveExpandedFileKey, selectedDiffSnapshot],
  );
  const virtualLayout = React.useMemo(
    () => (selectedDiffSnapshot ? buildDiffFileVirtualLayout(selectedDiffSnapshot, selectedCollapsedFileKeys, viewMode) : EMPTY_DIFF_FILE_VIRTUAL_LAYOUT),
    [selectedCollapsedFileKeys, selectedDiffSnapshot, viewMode],
  );
  const renderedFileCount = renderedSnapshot?.files.length ?? 0;
  // refreshBusy merges parent loading and the local refresh lock for button state.
  // refreshBusy 合并父级加载态与本地刷新锁，用于按钮状态。
  const refreshBusy = Boolean(props.loading || refreshPending);
  // shouldShowGitFileActions limits mutating buttons to live worktree and index views.
  // shouldShowGitFileActions 将变更按钮限制在实时工作区和索引视图中。
  const shouldShowGitFileActions = Boolean(props.onGitFileAction && (selection.mode === "unstaged" || selection.mode === "staged"));
  const searchActive = searchQuery.trim().length > 0;
  const backendSearchPrefetchCandidates = React.useMemo<readonly DiffSearchDetailPrefetchCandidate[]>(
    () => searchScopeSnapshot && searchActive && !searchPending
      ? diffSearchDetailPrefetchCandidates(
        searchScopeSnapshot,
        searchQuery,
        searchIndex.matches,
        safeActiveSearchIndex,
        activeSummarySearchMatches,
      )
      : EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS,
    [
      activeSummarySearchMatches,
      safeActiveSearchIndex,
      searchActive,
      searchIndex.matches,
      searchPending,
      searchQuery,
      searchScopeSnapshot,
    ],
  );
  const mountedFiles = React.useMemo(
    () => mountedDiffFiles(virtualLayout, scrollState.scrollTop, scrollState.clientHeight),
    [scrollState.clientHeight, scrollState.scrollTop, virtualLayout],
  );
  const handleFileCollapsedChange = React.useCallback((fileKey: string, collapsed: boolean) => {
    setExpandedFileKey((current) => (collapsed ? (current === fileKey ? null : current) : fileKey));
    if (typeof window !== "undefined") {
      window.requestAnimationFrame(syncFileListScrollState);
    }
  }, [syncFileListScrollState]);
  const handleSelectFile = React.useCallback((file: DiffFile) => {
    setExpandedFileKey(file.key);
    if (shouldLoadDiffFileDetail(file)) {
      onLoadFileDetail?.(file);
    }
    if (typeof window !== "undefined") {
      window.requestAnimationFrame(() => {
        const element = fileListRef.current;
        if (element) {
          element.scrollTop = 0;
        }
        syncFileListScrollState();
      });
    }
  }, [onLoadFileDetail, syncFileListScrollState]);
  const handleRevealFile = React.useCallback((file: DiffFile) => {
    const absolutePath = diffAbsoluteFilePath(renderedSnapshot?.repositoryRoot, file);
    if (!absolutePath) {
      return;
    }
    void openProjectDirectoryPath(absolutePath).catch((error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    });
  }, [renderedSnapshot?.repositoryRoot]);
  // handleRefreshClick starts one refresh request and blocks duplicate clicks until it settles.
  // handleRefreshClick 启动一次刷新请求，并在完成前阻止重复点击。
  const handleRefreshClick = React.useCallback(async () => {
    if (!onRefresh || props.loading || refreshPendingRef.current) {
      return;
    }
    refreshPendingRef.current = true;
    setRefreshPending(true);
    try {
      await onRefresh();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      refreshPendingRef.current = false;
      setRefreshPending(false);
    }
  }, [onRefresh, props.loading]);
  // requestSearchScroll records one user-requested search navigation scroll.
  // requestSearchScroll 记录一次用户请求的搜索导航滚动。
  const requestSearchScroll = React.useCallback(() => {
    setSearchScrollRequest((request) => request + 1);
  }, []);
  // handleSearchNext moves to the next loaded search match and requests one scroll.
  // handleSearchNext 移动到下一条已加载搜索命中并请求一次滚动。
  const handleSearchNext = React.useCallback(() => {
    setActiveSearchIndex((index) => nextSearchIndex(index, searchIndex.matches.length));
    requestSearchScroll();
  }, [requestSearchScroll, searchIndex.matches.length]);
  // handleSearchPrevious moves to the previous loaded search match and requests one scroll.
  // handleSearchPrevious 移动到上一条已加载搜索命中并请求一次滚动。
  const handleSearchPrevious = React.useCallback(() => {
    setActiveSearchIndex((index) => previousSearchIndex(index, searchIndex.matches.length));
    requestSearchScroll();
  }, [requestSearchScroll, searchIndex.matches.length]);
  React.useEffect(() => {
    persistDiffDisplayPreferences(viewMode, renderPreferences);
  }, [renderPreferences, viewMode]);
  React.useLayoutEffect(() => {
    const element = fileListRef.current;
    if (!element) {
      return undefined;
    }
    element.scrollTop = 0;
    syncFileListScrollState();
    return undefined;
  }, [diffSourceScrollResetKey, syncFileListScrollState]);
  React.useEffect(() => {
    setExpandedFileKey(null);
  }, [diffSourceScrollResetKey]);
  // Resyncs viewport state after virtual layout height changes.
  // 虚拟布局高度变化后重新同步视口状态。
  React.useLayoutEffect(() => {
    if (typeof window === "undefined") {
      syncFileListScrollState();
      return undefined;
    }
    const frameId = window.requestAnimationFrame(syncFileListScrollState);
    return () => window.cancelAnimationFrame(frameId);
  }, [syncFileListScrollState, virtualLayout.totalHeight]);
  // mountedDetailHydrationRequests contains visible summary files that should be hydrated lazily.
  // mountedDetailHydrationRequests 保存当前可见且需要懒水合的摘要文件。
  const mountedDetailHydrationRequests = React.useMemo<readonly DiffSearchDetailPrefetchCandidate[]>(() => {
    let requests: DiffSearchDetailPrefetchCandidate[] | null = null;
    for (const item of mountedFiles) {
      if (selectedCollapsedFileKeys.has(item.file.key)) {
        continue;
      }
      if (!shouldLoadDiffFileDetail(item.file)) {
        continue;
      }
      requests ??= [];
      requests.push(diffSearchDetailPrefetchCandidateForFile(item.file, summarySearchMatchByFileKey.get(item.file.key)));
    }
    return requests ?? EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS;
  }, [mountedFiles, selectedCollapsedFileKeys, summarySearchMatchByFileKey]);
  // backendSearchHydrationRequests excludes requests already covered by visible summary hydration.
  // backendSearchHydrationRequests 排除已由可见摘要水合覆盖的请求。
  const backendSearchHydrationRequests = React.useMemo<readonly DiffSearchDetailPrefetchCandidate[]>(
    () => diffDetailHydrationRequestsWithoutDuplicates(backendSearchPrefetchCandidates, mountedDetailHydrationRequests),
    [backendSearchPrefetchCandidates, mountedDetailHydrationRequests],
  );
  React.useEffect(() => {
    const nextIndex = clampDiffSearchIndex(activeSearchIndex, searchIndex.matches.length);
    if (nextIndex !== activeSearchIndex) {
      setActiveSearchIndex(nextIndex);
    }
  }, [activeSearchIndex, searchIndex.matches.length]);
  React.useEffect(() => {
    if (searchScrollRequest === 0 || searchScrollAppliedRequestRef.current === searchScrollRequest) {
      return;
    }
    if (activeSearchMatch && scrollToDiffSearchMatch(fileListRef.current, virtualLayout, activeSearchMatch)) {
      searchScrollAppliedRequestRef.current = searchScrollRequest;
      return;
    }
    if (!activeSearchMatch && activeSummarySearchMatches.length && searchIndex.matches.length === 0 && !summarySearchState.pending) {
      const target = activeSummarySearchMatches[0];
      if (scrollToDiffSummarySearchMatch(fileListRef.current, virtualLayout, target)) {
        searchScrollAppliedRequestRef.current = searchScrollRequest;
      }
    }
  }, [
    activeSearchMatch,
    activeSummarySearchMatches,
    searchIndex.matches.length,
    searchScrollRequest,
    summarySearchState.pending,
    virtualLayout,
  ]);
  React.useEffect(() => {
    if (!summarySearchRequestKey || !onSearchWorkspaceDiff || !searchActive || searchPending) {
      if (!summarySearchRequestKey) {
        summarySearchResolvedKeyRef.current = null;
        setSummarySearchState(EMPTY_DIFF_SUMMARY_SEARCH_STATE);
      }
      return;
    }
    if (searchIndex.matches.length >= DIFF_SEARCH_DETAIL_PREFETCH_TARGET_MATCHES) {
      return;
    }
    if (summarySearchResolvedKeyRef.current === summarySearchRequestKey) {
      return;
    }
    const requestSeq = summarySearchRequestSeqRef.current + 1;
    // requestFileKey freezes the visible file scope for this asynchronous backend search.
    // requestFileKey 固定本次异步后端搜索对应的可见文件作用域。
    const requestFileKey = searchScopeSnapshot?.files[0]?.key ?? null;
    summarySearchRequestSeqRef.current = requestSeq;
    setSummarySearchState({
      requestKey: summarySearchRequestKey,
      query: searchQuery.trim(),
      matches: [],
      pending: true,
      truncated: false,
      scannedLines: 0,
      error: null,
    });
    const requestQuery = searchQuery.trim();
    const runSearch = () => {
      void onSearchWorkspaceDiff(requestQuery, DIFF_SEARCH_SUMMARY_FILE_MATCH_LIMIT)
        .then((response) => {
          if (summarySearchRequestSeqRef.current !== requestSeq) {
            return;
          }
          // scopedMatches removes backend matches from files that are not currently displayed.
          // scopedMatches 移除当前未显示文件中的后端命中。
          const scopedMatches = requestFileKey
            ? (response?.matches ?? []).filter((match) => match.key === requestFileKey)
            : EMPTY_SUMMARY_SEARCH_MATCHES;
          summarySearchResolvedKeyRef.current = summarySearchRequestKey;
          summarySearchTargetAppliedKeyRef.current = null;
          setSummarySearchState({
            requestKey: summarySearchRequestKey,
            query: response?.query ?? requestQuery,
            matches: scopedMatches,
            pending: false,
            truncated: false,
            scannedLines: response?.scannedLines ?? 0,
            error: null,
          });
        })
        .catch((error) => {
          if (summarySearchRequestSeqRef.current !== requestSeq) {
            return;
          }
          summarySearchResolvedKeyRef.current = summarySearchRequestKey;
          summarySearchTargetAppliedKeyRef.current = null;
          setSummarySearchState({
            requestKey: summarySearchRequestKey,
            query: requestQuery,
            matches: [],
            pending: false,
            truncated: false,
            scannedLines: 0,
            error: error instanceof Error ? error.message : String(error),
          });
        });
    };
    if (typeof window === "undefined") {
      runSearch();
      return undefined;
    }
    const timeoutId = window.setTimeout(runSearch, DIFF_SEARCH_SUMMARY_REQUEST_DEBOUNCE_MS);
    return () => window.clearTimeout(timeoutId);
  }, [
    onSearchWorkspaceDiff,
    searchActive,
    searchIndex.matches.length,
    searchPending,
    searchQuery,
    searchScopeSnapshot,
    summarySearchRequestKey,
  ]);
  React.useEffect(() => {
    if (!activeSummarySearchMatches.length || !searchIndex.matches.length || summarySearchState.pending) {
      return;
    }
    const target = activeSummarySearchMatches[0];
    const targetKey = diffSummarySearchTargetKey(summarySearchRequestKey, target);
    if (!targetKey || summarySearchTargetAppliedKeyRef.current === targetKey || safeActiveSearchIndex !== 0) {
      return;
    }
    const targetIndex = diffSearchIndexForSummaryTarget(searchIndex.matches, target);
    if (targetIndex >= 0) {
      summarySearchTargetAppliedKeyRef.current = targetKey;
      setActiveSearchIndex(targetIndex);
    }
  }, [
    activeSummarySearchMatches,
    safeActiveSearchIndex,
    searchIndex.matches,
    summarySearchRequestKey,
    summarySearchState.pending,
  ]);
  React.useEffect(() => {
    if (!onLoadFileDetail || !mountedDetailHydrationRequests.length) {
      return;
    }
    return scheduleDiffDetailHydrationRequests(mountedDetailHydrationRequests, onLoadFileDetail);
  }, [mountedDetailHydrationRequests, onLoadFileDetail]);
  React.useEffect(() => {
    if (!onLoadFileDetail || !backendSearchHydrationRequests.length) {
      return;
    }
    return scheduleDiffDetailHydrationRequests(backendSearchHydrationRequests, onLoadFileDetail);
  }, [backendSearchHydrationRequests, onLoadFileDetail]);

  if (!snapshot) {
    // The host is still reading the diff: show where it will appear rather than an empty verdict.
    // 宿主仍在读取 diff：展示其将出现的位置，而不是空结论。
    if (props.loading) {
      return <section className="diff-review-panel empty"><StreamPlaceholder className="vk-stream-inset" count={6} label={t("diff.loading")} /></section>;
    }
    return <DiffReviewEmptyPanel body={props.emptyBody ?? t("diff.emptyBody")} refreshBusy={refreshBusy} title={props.emptyTitle ?? t("diff.cleanTitle")} onRefresh={props.onRefresh ? handleRefreshClick : undefined} />;
  }
  if (snapshot.gitRepository === false) {
    // A directory outside Git has no review modes or Git actions; offer only a refresh for after `git init`.
    // 不在 Git 中的目录没有审核模式与 Git 操作；只提供刷新，便于 `git init` 之后重新加载。
    return <DiffReviewEmptyPanel body={t("diff.notRepositoryBody")} refreshBusy={refreshBusy} title={t("diff.notRepositoryTitle")} onRefresh={props.onRefresh ? handleRefreshClick : undefined} />;
  }

  return (
    <section
      className={`diff-review-panel${shouldShowGitFileActions ? " has-floating-actions" : ""}`}
      // Alt+Up and Alt+Down walk the changed files without leaving the keyboard.
      // Alt+上与 Alt+下 无需离开键盘即可逐个切换变更文件。
      onKeyDown={(event) => {
        if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown") || !selectedDiffFile) return;
        const at = orderedFiles.indexOf(selectedDiffFile);
        const next = orderedFiles[at + (event.key === "ArrowDown" ? 1 : -1)];
        if (next) {
          event.preventDefault();
          handleSelectFile(next);
        }
      }}
    >
      <DiffPanelToolbar
        loading={refreshBusy}
        selection={selection}
        repositories={props.repositories}
        snapshot={snapshot}
        canSearchNext={searchIndex.matches.length > 1}
        canSearchPrevious={searchIndex.matches.length > 1}
        onCopyRaw={props.onCopyRaw}
        onRefresh={handleRefreshClick}
        onSearchNext={handleSearchNext}
        onSearchPrevious={handleSearchPrevious}
        onSearchQueryChange={(query) => {
          setSearchQuery(query);
          setActiveSearchIndex(0);
        }}
        onSearchSubmit={handleSearchNext}
        gitCommitActionBusy={props.gitCommitActionBusy}
        gitCommitActionDisabled={props.gitCommitActionDisabled}
        gitCommitActionLabel={props.gitCommitActionLabel}
        onOpenGitCommitAction={props.onOpenGitCommitAction}
        onSelectionChange={props.onSelectionChange}
        onRenderPreferencesChange={setRenderPreferences}
        onViewModeChange={setViewMode}
        renderPreferences={renderPreferences}
        searchQuery={searchQuery}
        searchResultLabel={searchWorkerError ? t("diff.search.indexUnavailable") : searchResultLabel(searchIndex.matches.length, safeActiveSearchIndex, searchIndex.truncated)}
        searchResultTruncated={searchIndex.truncated}
        viewMode={viewMode}
      />
      {renderedSnapshot && selectedDiffFile ? (
        <DiffFileBar activeKey={selectedDiffFile.key} files={orderedFiles} onSelect={handleSelectFile} actions={props.fileActions?.(selectedDiffFile, diffAbsoluteFilePath(renderedSnapshot?.repositoryRoot, selectedDiffFile))} />
      ) : null}
      <div className={`diff-review-body${selectedDiffFile ? " has-selection" : " directory-only"}`}>
        <div
          className={`diff-file-list ${viewMode === "split" ? "split-mode" : "unified-mode"}${virtualLayout.enabled ? " virtualized" : ""}`}
          onScroll={handleFileListScroll}
          ref={fileListRef}
        >
          {selectedDiffFile ? (
            <>
              {virtualLayout.enabled ? (
                <div className="diff-file-list-virtual-canvas" style={virtualLayout.canvasStyle}>
                  {mountedFiles.map((item) => (
                    <div className="diff-file-list-virtual-item" key={item.file.key} style={item.style}>
                      <DiffFileCard
                        file={item.file}
                        headerless
                        collapsed={selectedCollapsedFileKeys.has(item.file.key)}
                        scrollContainerRef={fileListRef}
                        scrollState={scrollState}
                        virtualHostTop={virtualLayout.policyNoticeHeight + item.linesTop}
                        viewMode={viewMode}
                        renderPreferences={renderPreferences}
                        activeSearchLineKey={activeDiffSearchLineKeyForFile(item.file.key, activeSearchMatch)}
                        onLoadFileDetail={onLoadFileDetail}
                        onRevealFile={handleRevealFile}
                        onCollapsedChange={handleFileCollapsedChange}
                        summarySearchMatch={summarySearchMatchByFileKey.get(item.file.key)}
                        searchLineKeys={searchVisibleLineKeys}
                      />
                    </div>
                  ))}
                </div>
              ) : (
                <DiffFileCard
                  file={selectedDiffFile}
                  headerless
                  collapsed={selectedCollapsedFileKeys.has(selectedDiffFile.key)}
                  key={selectedDiffFile.key}
                  scrollContainerRef={fileListRef}
                  scrollState={scrollState}
                  viewMode={viewMode}
                  renderPreferences={renderPreferences}
                  activeSearchLineKey={activeDiffSearchLineKeyForFile(selectedDiffFile.key, activeSearchMatch)}
                  onLoadFileDetail={onLoadFileDetail}
                  onRevealFile={handleRevealFile}
                  onCollapsedChange={handleFileCollapsedChange}
                  summarySearchMatch={summarySearchMatchByFileKey.get(selectedDiffFile.key)}
                  searchLineKeys={searchVisibleLineKeys}
                />
              )}
            </>
          ) : renderedFileCount ? (
            null
          ) : (
            <PageEmptyState
              description={props.emptyBody ?? t(DIFF_CLEAN_BODY[selection.mode] ?? "diff.emptyBody")}
              icon={<GitCompare />}
              size="panel"
              title={t("diff.cleanTitle")}
            />
          )}
        </div>
      </div>
      {shouldShowGitFileActions && props.onGitFileAction ? (
        <DiffReviewFloatingGitActions
          busy={Boolean(props.gitFileActionPending || props.loading)}
          fileCount={renderedFileCount}
          mode={selection.mode}
          selectedFile={selectedDiffFile}
          onAction={props.onGitFileAction}
          onRevealFile={handleRevealFile}
        />
      ) : null}
    </section>
  );
}

// DIFF_CLEAN_BODY says what a review mode found nothing in when it has no changed files.
// DIFF_CLEAN_BODY 说明某审核模式没有变更文件时，是在哪里没有找到修改。
const DIFF_CLEAN_BODY: Partial<Record<DiffReviewSelection["mode"], TranslationKey>> = {
  unstaged: "diff.cleanUnstaged",
  staged: "diff.cleanStaged",
};

// DiffReviewEmptyPanel renders the whole-panel empty state with an optional refresh.
// DiffReviewEmptyPanel 渲染整面板空状态，并可选提供刷新。
function DiffReviewEmptyPanel(props: { title: string; body: string; refreshBusy: boolean; onRefresh?: () => void }) {
  const { t } = useI18n();
  return (
    <section className="diff-review-panel empty">
      <PageEmptyState
        action={props.onRefresh ? (
          <Button disabled={props.refreshBusy} onClick={props.onRefresh} size="sm" variant="outline">
            <RefreshCw />
            {props.refreshBusy ? t("diff.loading") : t("diff.refresh")}
          </Button>
        ) : null}
        description={props.body}
        icon={<GitCompare />}
        size="panel"
        title={props.title}
      />
    </section>
  );
}
