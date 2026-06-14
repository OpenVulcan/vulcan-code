import React from "react";
import { ChevronDown, ChevronRight, FileText, FileWarning, Folder, FolderClosed, FolderOpen, Search, RefreshCw } from "lucide-react";

import { DiffFileCard, DiffFileHeader, useDiffScrollState } from "./diff-file-card";
import { DiffPanelToolbar } from "./diff-review-toolbar";
import { openProjectDirectoryPath } from "@/service/project-actions-service";
import {
  DIFF_SEARCH_DETAIL_PREFETCH_CONTEXT_LINES,
  DIFF_SEARCH_DETAIL_PREFETCH_DISPATCH_DELAY_MS,
  DIFF_SEARCH_DETAIL_PREFETCH_LIMIT,
  DIFF_SEARCH_DETAIL_PREFETCH_TARGET_MATCHES,
  DIFF_SEARCH_PAGE_SIZE,
  DIFF_SEARCH_SUMMARY_FILE_MATCH_LIMIT,
  DIFF_SEARCH_SUMMARY_REQUEST_DEBOUNCE_MS,
} from "./limits";
import type {
  DiffFile,
  DiffHunkExpansionRequest,
  DiffPanelSnapshot,
  DiffRenderPreferences,
  DiffReviewSelection,
  DiffViewMode,
  WorkspaceDiffSearchFileMatch,
  WorkspaceDiffSearchResponse,
} from "./types";
import {
  clampDiffSearchIndex,
  diffSearchVisiblePage,
  type DiffSearchMatch,
} from "./diff-search";
import { useDiffSearchIndex } from "./diff-search-worker-client";
import {
  EMPTY_DIFF_FILE_VIRTUAL_LAYOUT,
  buildDiffFileVirtualLayout,
  diffScrollTopForLine,
  diffScrollTopForSummarySearchMatch,
  mountedDiffFiles,
  stickyDiffFileItem,
  type DiffFileVirtualLayout,
} from "./diff-render-index";
import "./diffReview.css";

// DiffFileDetailLoader lazily requests one file-level diff detail from the host.
// DiffFileDetailLoader 从宿主懒请求单个文件级 diff 详情。
type DiffFileDetailLoader = (file: DiffFile, contextLines?: number, expansion?: DiffHunkExpansionRequest) => void;

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
  onCopyRaw?: (rawDiff: string) => void;
  // Optional lazy detail loader for partial summaries and hunk context expansion.
  // 用于局部摘要与 hunk 上下文展开的可选懒加载详情函数。
  onLoadFileDetail?: DiffFileDetailLoader;
  // Optional backend compact diff search loader for summary-only hydration.
  // 用于摘要文件水合的可选后端紧凑 diff 搜索加载器。
  onSearchWorkspaceDiff?: (query: string, limit?: number) => Promise<WorkspaceDiffSearchResponse | null>;
}) {
  const fileListRef = React.useRef<HTMLDivElement | null>(null);
  const summarySearchRequestSeqRef = React.useRef(0);
  const summarySearchResolvedKeyRef = React.useRef<string | null>(null);
  const summarySearchTargetAppliedKeyRef = React.useRef<string | null>(null);
  const summarySearchScrollAppliedKeyRef = React.useRef<string | null>(null);
  const [scrollState, handleFileListScroll, syncFileListScrollState] = useDiffScrollState(fileListRef);
  const snapshot = props.snapshot;
  const selection = props.selection ?? { mode: "unstaged" };
  const onRefresh = props.onRefresh;
  const onLoadFileDetail = props.onLoadFileDetail;
  const onSearchWorkspaceDiff = props.onSearchWorkspaceDiff;
  const [searchQuery, setSearchQuery] = React.useState("");
  const [activeSearchIndex, setActiveSearchIndex] = React.useState(0);
  const [viewMode, setViewMode] = React.useState<DiffViewMode>("unified");
  const [renderPreferences, setRenderPreferences] = React.useState<DiffRenderPreferences>({
    overflowMode: "scroll",
    hunkSeparatorStyle: "line-info",
    indicatorStyle: "bars",
  });
  const [expandedFileKey, setExpandedFileKey] = React.useState<string | null>(null);
  const [fileTreeFilter, setFileTreeFilter] = React.useState("");
  const [fileTreeDrawerPinnedOpen, setFileTreeDrawerPinnedOpen] = React.useState(true);
  // refreshPending stores the immediate local refresh lock before parent loading propagates.
  // refreshPending 保存父级 loading 传入前的本地即时刷新锁。
  const [refreshPending, setRefreshPending] = React.useState(false);
  const [summarySearchState, setSummarySearchState] = React.useState<DiffSummarySearchState>(EMPTY_DIFF_SUMMARY_SEARCH_STATE);
  const diffSourceScrollResetKey = React.useMemo(
    () => diffReviewScrollResetKey(snapshot, selection),
    [selection, snapshot],
  );
  const { index: searchIndex, pending: searchPending, error: searchWorkerError } = useDiffSearchIndex(snapshot, searchQuery);
  const safeActiveSearchIndex = clampDiffSearchIndex(activeSearchIndex, searchIndex.matches.length);
  const activeSearchMatch = searchIndex.matches[safeActiveSearchIndex] ?? null;
  const summarySearchRequestKey = React.useMemo(
    () => diffSummarySearchRequestKey(snapshot, selection, searchQuery),
    [searchQuery, selection, snapshot],
  );
  const activeSummarySearchMatches = summarySearchState.requestKey === summarySearchRequestKey
    ? summarySearchState.matches
    : EMPTY_SUMMARY_SEARCH_MATCHES;
  const activeSummarySearchPending = summarySearchState.requestKey === summarySearchRequestKey && summarySearchState.pending;
  const summarySearchDerived = React.useMemo(
    () => diffSummarySearchDerived(activeSummarySearchMatches),
    [activeSummarySearchMatches],
  );
  const summarySearchMatchByFileKey = summarySearchDerived.matchByFileKey;
  const summarySearchPageFileKeys = summarySearchDerived.pageFileKeys;
  const searchVisiblePage = React.useMemo(
    () => diffSearchVisiblePage(searchIndex.matches, safeActiveSearchIndex),
    [safeActiveSearchIndex, searchIndex.matches],
  );
  const searchPageMatches = searchVisiblePage.matches;
  const searchPageFileKeys = searchVisiblePage.fileKeys;
  const searchVisibleLineKeys = searchVisiblePage.lineKeys;
  const renderedSnapshot = React.useMemo(
    () => (snapshot
      ? projectedSearchSnapshot(
        snapshot,
        searchQuery,
        searchIndex.matches.length,
        searchPageFileKeys,
        summarySearchPageFileKeys,
      )
      : null),
    [searchIndex.matches.length, searchPageFileKeys, searchQuery, snapshot, summarySearchPageFileKeys],
  );
  // autoSelectedSingleFile uses the only rendered file directly instead of showing a directory landing page.
  // autoSelectedSingleFile 在仅渲染一个文件时直接使用该文件，避免展示目录入口页。
  const autoSelectedSingleFile = renderedSnapshot?.files.length === 1 ? renderedSnapshot.files[0] : null;
  // effectiveExpandedFileKey keeps the single-file direct view expanded without mutating manual multi-file selection state.
  // effectiveExpandedFileKey 让单文件直显保持展开，同时不污染多文件手动选择状态。
  const effectiveExpandedFileKey = expandedFileKey ?? autoSelectedSingleFile?.key ?? null;
  const collapsedFileKeys = React.useMemo(
    () => diffCollapsedFileKeys(renderedSnapshot, effectiveExpandedFileKey),
    [effectiveExpandedFileKey, renderedSnapshot],
  );
  const selectedDiffFile = React.useMemo(
    () => renderedSnapshot?.files.find((file) => file.key === effectiveExpandedFileKey) ?? null,
    [effectiveExpandedFileKey, renderedSnapshot],
  );
  const selectedDiffSnapshot = React.useMemo(
    () => renderedSnapshot && selectedDiffFile
      ? { ...renderedSnapshot, files: [selectedDiffFile] }
      : null,
    [renderedSnapshot, selectedDiffFile],
  );
  const selectedCollapsedFileKeys = React.useMemo(
    () => diffCollapsedFileKeys(selectedDiffSnapshot, effectiveExpandedFileKey),
    [effectiveExpandedFileKey, selectedDiffSnapshot],
  );
  const virtualLayout = React.useMemo(
    () => (selectedDiffSnapshot ? buildDiffFileVirtualLayout(selectedDiffSnapshot, selectedCollapsedFileKeys, viewMode) : EMPTY_DIFF_FILE_VIRTUAL_LAYOUT),
    [selectedCollapsedFileKeys, selectedDiffSnapshot, viewMode],
  );
  const renderedFileCount = renderedSnapshot?.files.length ?? 0;
  // isSingleFileReview marks any mode that is currently displaying one selected file body.
  // isSingleFileReview 标记任何当前正在展示单个已选文件正文的模式。
  const isSingleFileReview = Boolean(selectedDiffFile);
  // shouldShowFileTree limits the navigation drawer to multi-file diffs.
  // shouldShowFileTree 将导航目录限制在多文件 diff 场景。
  const shouldShowFileTree = renderedFileCount > 1;
  // refreshBusy merges parent loading and the local refresh lock for button state.
  // refreshBusy 合并父级加载态与本地刷新锁，用于按钮状态。
  const refreshBusy = Boolean(props.loading || refreshPending);
  const searchActive = searchQuery.trim().length > 0;
  const searchWaitingForBackendMatches = searchActive
    && !searchPending
    && searchIndex.matches.length === 0
    && activeSummarySearchPending;
  const searchHasNoMatches = searchActive
    && !searchPending
    && !activeSummarySearchPending
    && searchIndex.matches.length === 0
    && activeSummarySearchMatches.length === 0;
  const searchPageDescription = React.useMemo(
    () => searchActive && searchIndex.matches.length
      ? searchFilePageDescription(searchPageMatches, searchPageFileKeys, searchIndex.matches.length)
      : searchActive && activeSummarySearchMatches.length
        ? summarySearchFilePageDescription(activeSummarySearchMatches, summarySearchState.truncated)
      : null,
    [
      activeSummarySearchMatches,
      searchActive,
      searchIndex.matches.length,
      searchPageFileKeys,
      searchPageMatches,
      summarySearchState.truncated,
    ],
  );
  const backendSearchPrefetchCandidates = React.useMemo<readonly DiffSearchDetailPrefetchCandidate[]>(
    () => snapshot && searchActive && !searchPending
      ? diffSearchDetailPrefetchCandidates(
        snapshot,
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
      snapshot,
    ],
  );
  const mountedFiles = React.useMemo(
    () => mountedDiffFiles(virtualLayout, scrollState.scrollTop, scrollState.clientHeight),
    [scrollState.clientHeight, scrollState.scrollTop, virtualLayout],
  );
  const stickyFileItem = React.useMemo(
    () => stickyDiffFileItem(virtualLayout, scrollState.scrollTop),
    [scrollState.scrollTop, virtualLayout],
  );
  // floatingHeaderFile keeps the single-file title pinned even before regular sticky logic activates.
  // floatingHeaderFile 让单文件标题在常规吸顶逻辑触发前也保持固定。
  const floatingHeaderFile = isSingleFileReview ? selectedDiffFile : stickyFileItem?.file ?? null;
  const handleFileCollapsedChange = React.useCallback((fileKey: string, collapsed: boolean) => {
    setExpandedFileKey((current) => (collapsed ? (current === fileKey ? null : current) : fileKey));
    if (typeof window !== "undefined") {
      window.requestAnimationFrame(syncFileListScrollState);
    }
  }, [syncFileListScrollState]);
  const handleFileTreeSelect = React.useCallback((file: DiffFile) => {
    setExpandedFileKey(file.key);
    setFileTreeDrawerPinnedOpen(false);
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
    void openProjectDirectoryPath(absolutePath);
  }, [renderedSnapshot?.repositoryRoot]);
  // handleRefreshClick starts one refresh request and blocks duplicate clicks until it settles.
  // handleRefreshClick 启动一次刷新请求，并在完成前阻止重复点击。
  const handleRefreshClick = React.useCallback(() => {
    if (!onRefresh || refreshBusy) {
      return;
    }
    setRefreshPending(true);
    try {
      const refreshResult = onRefresh();
      if (isPromiseLike(refreshResult)) {
        void refreshResult.finally(() => setRefreshPending(false));
      } else if (typeof window !== "undefined") {
        window.setTimeout(() => setRefreshPending(false), 0);
      } else {
        setRefreshPending(false);
      }
    } catch (error) {
      setRefreshPending(false);
      throw error;
    }
  }, [onRefresh, refreshBusy]);
  React.useEffect(() => {
    if (!props.loading) {
      setRefreshPending(false);
    }
  }, [props.loading]);
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
    setFileTreeDrawerPinnedOpen(true);
    setFileTreeFilter("");
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
    if (!activeSearchMatch) {
      return;
    }
    scrollToDiffSearchMatch(fileListRef.current, virtualLayout, activeSearchMatch);
  }, [activeSearchMatch, virtualLayout]);
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
          summarySearchResolvedKeyRef.current = summarySearchRequestKey;
          summarySearchTargetAppliedKeyRef.current = null;
          setSummarySearchState({
            requestKey: summarySearchRequestKey,
            query: response?.query ?? requestQuery,
            matches: response?.matches ?? [],
            pending: false,
            truncated: Boolean(response?.truncated),
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
    if (!activeSummarySearchMatches.length || searchIndex.matches.length > 0 || summarySearchState.pending) {
      return;
    }
    const target = activeSummarySearchMatches[0];
    const targetKey = diffSummarySearchTargetKey(summarySearchRequestKey, target);
    if (!targetKey || summarySearchScrollAppliedKeyRef.current === targetKey) {
      return;
    }
    if (scrollToDiffSummarySearchMatch(fileListRef.current, virtualLayout, target)) {
      summarySearchScrollAppliedKeyRef.current = targetKey;
    }
  }, [
    activeSummarySearchMatches,
    searchIndex.matches.length,
    summarySearchRequestKey,
    summarySearchState.pending,
    virtualLayout,
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
    return (
      <section className="diff-review-panel empty">
        <div className="diff-empty-state">
          <FileText size={22} />
          <strong>{props.emptyTitle ?? "Git Diff"}</strong>
          <span>{props.emptyBody ?? "当前没有可展示的 Git Diff。"}</span>
          {props.onRefresh ? (
            <button className="diff-action-button" disabled={refreshBusy} onClick={handleRefreshClick} type="button">
              <RefreshCw className={refreshBusy ? "diff-refresh-icon loading" : "diff-refresh-icon"} size={14} />
              <span>{refreshBusy ? "加载中" : "刷新"}</span>
            </button>
          ) : null}
        </div>
      </section>
    );
  }

  return (
    <section className="diff-review-panel">
      <DiffPanelToolbar
        loading={refreshBusy}
        selection={selection}
        snapshot={snapshot}
        canSearchNext={searchIndex.matches.length > 1}
        canSearchPrevious={searchIndex.matches.length > 1}
        onCopyRaw={props.onCopyRaw}
        onRefresh={handleRefreshClick}
        onSearchNext={() => setActiveSearchIndex((index) => nextSearchIndex(index, searchIndex.matches.length))}
        onSearchPrevious={() => setActiveSearchIndex((index) => previousSearchIndex(index, searchIndex.matches.length))}
        onSearchQueryChange={(query) => {
          setSearchQuery(query);
          setActiveSearchIndex(0);
        }}
        onSelectionChange={props.onSelectionChange}
        onRenderPreferencesChange={setRenderPreferences}
        onViewModeChange={setViewMode}
        renderPreferences={renderPreferences}
        searchQuery={searchQuery}
        searchResultLabel={searchPending ? "..." : searchWorkerError ? "索引不可用" : searchResultLabel(searchIndex.matches.length, safeActiveSearchIndex, searchIndex.truncated)}
        searchResultTruncated={searchIndex.truncated}
        viewMode={viewMode}
      />
      <div className={`diff-review-body${selectedDiffFile ? " has-selection" : " directory-only"}`}>
        {renderedSnapshot && shouldShowFileTree ? (
          <DiffFileTreeDrawer
            activeFileKey={effectiveExpandedFileKey}
            files={renderedSnapshot.files}
            filter={fileTreeFilter}
            lockedOpen={!selectedDiffFile}
            pinnedOpen={fileTreeDrawerPinnedOpen}
            onFilterChange={setFileTreeFilter}
            onPinnedOpenChange={setFileTreeDrawerPinnedOpen}
            onSelectFile={handleFileTreeSelect}
          />
        ) : null}
        <div
          className={`diff-file-list ${viewMode === "split" ? "split-mode" : "unified-mode"}${virtualLayout.enabled ? " virtualized" : ""}${isSingleFileReview ? " single-file-floating-header" : ""}`}
          onScroll={handleFileListScroll}
          ref={fileListRef}
        >
          {searchActive && (searchPending || searchWaitingForBackendMatches) ? (
            <DiffSearchPendingState query={searchQuery} />
          ) : searchHasNoMatches ? (
            <DiffSearchEmptyState
              error={searchWorkerError}
              query={searchQuery}
              scannedLines={searchIndex.scannedLines}
              prefetchingMore={activeSummarySearchPending || backendSearchHydrationRequests.length > 0}
            />
          ) : selectedDiffFile ? (
            <>
              {floatingHeaderFile ? (
                <div className="diff-file-sticky-header-overlay">
                  <DiffFileHeader
                    className={isSingleFileReview ? "overlay glass" : "overlay"}
                    collapsed={selectedCollapsedFileKeys.has(floatingHeaderFile.key)}
                    file={floatingHeaderFile}
                    onToggle={() => handleFileCollapsedChange(floatingHeaderFile.key, !selectedCollapsedFileKeys.has(floatingHeaderFile.key))}
                  />
                </div>
              ) : null}
              {searchPageDescription ? <DiffSearchPageNotice description={searchPageDescription} /> : null}
              {virtualLayout.enabled ? (
                <div className="diff-file-list-virtual-canvas" style={virtualLayout.canvasStyle}>
                  {mountedFiles.map((item) => (
                    <div className="diff-file-list-virtual-item" key={item.file.key} style={item.style}>
                      <DiffFileCard
                        file={item.file}
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
            <div className="diff-empty-state in-list">
              <FileText size={20} />
              <strong>{props.emptyTitle ?? "Git Diff"}</strong>
              <span>{snapshot.notice ?? props.emptyBody ?? "当前没有可展示的 Git Diff。"}</span>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

// DiffSearchPendingState displays while the search worker builds the loaded-line index.
// DiffSearchPendingState 在搜索 Worker 构建已加载行索引时显示。
function DiffSearchPendingState(props: {
  // Current search query.
  // 当前搜索查询。
  query: string;
}) {
  return (
    <div className="diff-empty-state in-list">
      <FileText size={20} />
      <strong>正在搜索</strong>
      <span>{`正在索引已加载 diff 中的 “${props.query.trim()}”。`}</span>
    </div>
  );
}

// DiffFileTreeNode stores one directory or file entry in the review tree.
// DiffFileTreeNode 保存审核目录树中的单个目录或文件节点。
interface DiffFileTreeNode {
  // Stable tree path used for React keys and directory toggles.
  // 用于 React key 与目录展开状态的稳定树路径。
  path: string;
  // Display name for the current path segment.
  // 当前路径片段的展示名称。
  name: string;
  // Child directories and files sorted for display.
  // 排序后用于展示的子目录与文件。
  children: DiffFileTreeNode[];
  // File payload when this node is a leaf.
  // 当前节点为叶子文件时对应的文件数据。
  file?: DiffFile;
  // Aggregated added-line count for this node.
  // 当前节点聚合后的新增行数。
  additions: number;
  // Aggregated deleted-line count for this node.
  // 当前节点聚合后的删除行数。
  deletions: number;
}

// DiffFileTreeDrawer renders the collapsible file-directory navigation drawer.
// DiffFileTreeDrawer 渲染可收起的文件目录导航抽屉。
function DiffFileTreeDrawer(props: {
  // Files available in the current rendered snapshot.
  // 当前渲染快照中可用的文件列表。
  files: readonly DiffFile[];
  // Current selected file key.
  // 当前选中的文件 key。
  activeFileKey: string | null;
  // File tree filter text.
  // 文件目录树筛选文本。
  filter: string;
  // Whether the drawer must stay open because no file is selected.
  // 是否因未选择文件而必须保持目录抽屉展开。
  lockedOpen: boolean;
  // Whether the drawer is explicitly pinned open by the user.
  // 目录抽屉是否被用户显式固定展开。
  pinnedOpen: boolean;
  // Called when the filter text changes.
  // 筛选文本变化时调用。
  onFilterChange: (filter: string) => void;
  // Called when the pinned drawer open state changes.
  // 目录抽屉固定展开状态变化时调用。
  onPinnedOpenChange: (open: boolean) => void;
  // Called when a file node is selected.
  // 选择文件节点时调用。
  onSelectFile: (file: DiffFile) => void;
}) {
  const tree = React.useMemo(() => buildDiffFileTree(props.files, props.filter), [props.files, props.filter]);
  const defaultOpenDirectoryPaths = React.useMemo(() => defaultDiffTreeOpenDirectoryPaths(tree), [tree]);
  const [openDirectoryPaths, setOpenDirectoryPaths] = React.useState<ReadonlySet<string>>(defaultOpenDirectoryPaths);
  const [hoverOpen, setHoverOpen] = React.useState(false);
  const [suppressHoverOpen, setSuppressHoverOpen] = React.useState(false);
  const hoverOpenTimeoutRef = React.useRef<number | null>(null);
  const hoverCloseTimeoutRef = React.useRef<number | null>(null);
  const drawerOpen = props.lockedOpen || props.pinnedOpen || hoverOpen;
  const clearHoverOpenTimeout = React.useCallback(() => {
    if (hoverOpenTimeoutRef.current != null) {
      window.clearTimeout(hoverOpenTimeoutRef.current);
      hoverOpenTimeoutRef.current = null;
    }
  }, []);
  const clearHoverCloseTimeout = React.useCallback(() => {
    if (hoverCloseTimeoutRef.current != null) {
      window.clearTimeout(hoverCloseTimeoutRef.current);
      hoverCloseTimeoutRef.current = null;
    }
  }, []);
  React.useEffect(() => {
    setOpenDirectoryPaths(defaultOpenDirectoryPaths);
  }, [defaultOpenDirectoryPaths]);
  React.useEffect(() => {
    if (props.lockedOpen) {
      clearHoverOpenTimeout();
      clearHoverCloseTimeout();
      setHoverOpen(false);
      setSuppressHoverOpen(false);
    }
  }, [clearHoverCloseTimeout, clearHoverOpenTimeout, props.lockedOpen]);
  React.useEffect(() => {
    return () => {
      clearHoverOpenTimeout();
      clearHoverCloseTimeout();
    };
  }, [clearHoverCloseTimeout, clearHoverOpenTimeout]);
  const toggleDirectory = React.useCallback((path: string) => {
    setOpenDirectoryPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);
  // expandAllDirectories opens every visible directory in the filtered tree.
  // expandAllDirectories 展开当前筛选目录树里的全部可见目录。
  const expandAllDirectories = React.useCallback(() => {
    setOpenDirectoryPaths(defaultOpenDirectoryPaths);
  }, [defaultOpenDirectoryPaths]);
  // collapseAllDirectories closes every directory in the filtered tree.
  // collapseAllDirectories 折叠当前筛选目录树里的全部目录。
  const collapseAllDirectories = React.useCallback(() => {
    setOpenDirectoryPaths(new Set<string>());
  }, []);
  const handleDrawerMouseEnter = React.useCallback((event: React.MouseEvent<HTMLElement>) => {
    clearHoverCloseTimeout();
    if (props.lockedOpen || suppressHoverOpen) {
      return;
    }
    if ((event.buttons & 1) === 1) {
      clearHoverOpenTimeout();
      setSuppressHoverOpen(true);
      return;
    }
    clearHoverOpenTimeout();
    hoverOpenTimeoutRef.current = window.setTimeout(() => {
      hoverOpenTimeoutRef.current = null;
      setHoverOpen(true);
    }, DIFF_FILE_TREE_HOVER_DELAY_MS);
  }, [clearHoverCloseTimeout, clearHoverOpenTimeout, props.lockedOpen, suppressHoverOpen]);
  const handleDrawerMouseLeave = React.useCallback(() => {
    clearHoverOpenTimeout();
    setSuppressHoverOpen(false);
    hoverCloseTimeoutRef.current = window.setTimeout(() => {
      hoverCloseTimeoutRef.current = null;
      setHoverOpen(false);
    }, DIFF_FILE_TREE_HOVER_DELAY_MS);
  }, [clearHoverOpenTimeout]);
  const handleDrawerToggle = React.useCallback(() => {
    if (props.lockedOpen) {
      return;
    }
    clearHoverOpenTimeout();
    clearHoverCloseTimeout();
    if (drawerOpen) {
      props.onPinnedOpenChange(false);
      setHoverOpen(false);
      setSuppressHoverOpen(true);
      return;
    }
    props.onPinnedOpenChange(true);
    setSuppressHoverOpen(false);
  }, [clearHoverCloseTimeout, clearHoverOpenTimeout, drawerOpen, props]);
  return (
    <aside
      className={`diff-file-tree-drawer${drawerOpen ? " open" : " collapsed"}${props.lockedOpen ? " locked-open" : ""}`}
      onMouseEnter={handleDrawerMouseEnter}
      onMouseLeave={handleDrawerMouseLeave}
    >
      <button
        aria-label={drawerOpen ? "收起文件目录" : "展开文件目录"}
        className="diff-file-tree-handle"
        onClick={handleDrawerToggle}
        type="button"
      >
        {drawerOpen ? <ChevronRight size={14} /> : <ChevronLeftIcon />}
      </button>
      <div className="diff-file-tree-panel">
        <label className="diff-file-tree-filter">
          <Search size={14} />
          <input
            aria-label="筛选文件"
            onChange={(event) => props.onFilterChange(event.currentTarget.value)}
            placeholder="筛选文件..."
            value={props.filter}
          />
          <span className="diff-file-tree-filter-actions">
            <button
              aria-label="全部展开"
              disabled={defaultOpenDirectoryPaths.size === 0}
              onClick={expandAllDirectories}
              title="全部展开"
              type="button"
            >
              <FolderOpen size={13} />
            </button>
            <button
              aria-label="全部折叠"
              disabled={defaultOpenDirectoryPaths.size === 0}
              onClick={collapseAllDirectories}
              title="全部折叠"
              type="button"
            >
              <FolderClosed size={13} />
            </button>
          </span>
        </label>
        <div className="diff-file-tree-list">
          {tree.children.length ? tree.children.map((node) => (
            <DiffFileTreeNodeView
              activeFileKey={props.activeFileKey}
              depth={0}
              key={node.path}
              node={node}
              onSelectFile={props.onSelectFile}
              onToggleDirectory={toggleDirectory}
              openDirectoryPaths={openDirectoryPaths}
            />
          )) : (
            <div className="diff-file-tree-empty">没有匹配的文件</div>
          )}
        </div>
      </div>
    </aside>
  );
}

// DiffFileTreeNodeView renders one recursive directory-tree row.
// DiffFileTreeNodeView 渲染目录树中的一个递归节点行。
function DiffFileTreeNodeView(props: {
  // Tree node to render.
  // 需要渲染的树节点。
  node: DiffFileTreeNode;
  // Current tree nesting depth.
  // 当前目录树嵌套深度。
  depth: number;
  // Current selected file key.
  // 当前选中的文件 key。
  activeFileKey: string | null;
  // Open directory path set.
  // 已展开目录路径集合。
  openDirectoryPaths: ReadonlySet<string>;
  // Directory toggle handler.
  // 目录展开切换处理器。
  onToggleDirectory: (path: string) => void;
  // File selection handler.
  // 文件选择处理器。
  onSelectFile: (file: DiffFile) => void;
}) {
  const isDirectory = !props.node.file;
  const isOpen = isDirectory && props.openDirectoryPaths.has(props.node.path);
  const isActive = props.node.file?.key === props.activeFileKey;
  const rowStyle = { "--diff-tree-depth": props.depth } as React.CSSProperties;
  if (isDirectory) {
    return (
      <div className="diff-file-tree-node directory">
        <button
          className="diff-file-tree-row directory"
          onClick={() => props.onToggleDirectory(props.node.path)}
          style={rowStyle}
          type="button"
        >
          {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          <Folder size={14} />
          <span className="diff-file-tree-name">{props.node.name}</span>
        </button>
        {isOpen ? props.node.children.map((child) => (
          <DiffFileTreeNodeView
            activeFileKey={props.activeFileKey}
            depth={props.depth + 1}
            key={child.path}
            node={child}
            onSelectFile={props.onSelectFile}
            onToggleDirectory={props.onToggleDirectory}
            openDirectoryPaths={props.openDirectoryPaths}
          />
        )) : null}
      </div>
    );
  }
  return (
    <button
      className={`diff-file-tree-row file${isActive ? " active" : ""}`}
      onClick={() => props.node.file ? props.onSelectFile(props.node.file) : undefined}
      style={rowStyle}
      title={props.node.file ? diffRevealRelativePath(props.node.file) ?? props.node.name : props.node.name}
      type="button"
    >
      <span className={`diff-file-tree-file-icon ${props.node.file?.status ?? "modified"}`}>
        {props.node.file?.tooLargeToDisplay ? <FileWarning size={14} /> : diffFileTreeExtensionLabel(props.node.name)}
      </span>
      <span className="diff-file-tree-name">{props.node.name}</span>
      <DiffTreeStats additions={props.node.additions} deletions={props.node.deletions} />
    </button>
  );
}

// DiffTreeStats renders compact added/deleted counters.
// DiffTreeStats 渲染紧凑的新增/删除计数。
function DiffTreeStats(props: {
  // Aggregated added-line count.
  // 聚合新增行数。
  additions: number;
  // Aggregated deleted-line count.
  // 聚合删除行数。
  deletions: number;
}) {
  return (
    <span className="diff-file-tree-stats">
      {props.additions ? <span className="added">{`+${props.additions}`}</span> : null}
      {props.deletions ? <span className="removed">{`-${props.deletions}`}</span> : null}
    </span>
  );
}

// ChevronLeftIcon provides a small local icon for the collapsed drawer handle.
// ChevronLeftIcon 为收起目录把手提供一个小型本地图标。
function ChevronLeftIcon() {
  return (
    <svg aria-hidden="true" fill="none" height="14" viewBox="0 0 14 14" width="14">
      <path d="M8.75 3.5 5.25 7l3.5 3.5" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.5" />
    </svg>
  );
}

// buildDiffFileTree builds a filtered directory tree and aggregates line-change counters.
// buildDiffFileTree 构建过滤后的目录树，并聚合行变更计数。
function buildDiffFileTree(
  // Files available in the current rendered diff snapshot.
  // 当前渲染 diff 快照中的文件列表。
  files: readonly DiffFile[],
  // User-entered path filter text.
  // 用户输入的路径筛选文本。
  filter: string,
): DiffFileTreeNode {
  const root: DiffFileTreeNode = {
    path: "",
    name: "",
    children: [],
    additions: 0,
    deletions: 0,
  };
  for (const file of files) {
    if (!diffFileTreeMatchesFilter(file, filter)) {
      continue;
    }
    insertDiffFileTreeNode(root, file);
  }
  sortDiffFileTreeNode(root);
  compressDiffFileTreeDirectoryChains(root);
  return root;
}

// insertDiffFileTreeNode inserts one file and updates parent directory counters in place.
// insertDiffFileTreeNode 插入单个文件，并原地更新父目录计数。
function insertDiffFileTreeNode(
  // Mutable tree root or directory node.
  // 可变树根节点或目录节点。
  root: DiffFileTreeNode,
  // File to insert into the tree.
  // 需要插入目录树的文件。
  file: DiffFile,
): void {
  const parts = diffFileTreePathParts(file);
  let current = root;
  current.additions += file.additions;
  current.deletions += file.deletions;
  for (let index = 0; index < parts.length; index += 1) {
    const part = parts[index];
    const isFile = index === parts.length - 1;
    const path = parts.slice(0, index + 1).join("/");
    let child = current.children.find((node) => node.path === path && Boolean(node.file) === isFile);
    if (!child) {
      child = {
        path,
        name: part,
        children: [],
        file: isFile ? file : undefined,
        additions: 0,
        deletions: 0,
      };
      current.children.push(child);
    }
    child.additions += file.additions;
    child.deletions += file.deletions;
    current = child;
  }
}

// diffFileTreePathParts returns stable visible path segments for one file node.
// diffFileTreePathParts 返回单个文件节点稳定可见的路径片段。
function diffFileTreePathParts(
  // File whose best visible path should be split.
  // 需要拆分最佳可见路径的文件。
  file: DiffFile,
): string[] {
  const relativePath = diffRevealRelativePath(file) ?? file.key;
  const parts = relativePath.split(/[\\/]+/).map((part) => part.trim()).filter(Boolean);
  return parts.length ? parts : [file.key || "unknown"];
}

// diffFileTreeMatchesFilter checks whether one file should remain visible under the filter.
// diffFileTreeMatchesFilter 检查单个文件在筛选条件下是否应保持可见。
function diffFileTreeMatchesFilter(
  // File to test against the filter.
  // 需要与筛选文本匹配的文件。
  file: DiffFile,
  // User-entered path filter text.
  // 用户输入的路径筛选文本。
  filter: string,
): boolean {
  const normalizedFilter = filter.trim().toLowerCase();
  return !normalizedFilter || diffFilePathText(file).includes(normalizedFilter);
}

// sortDiffFileTreeNode sorts directories before files and names alphabetically.
// sortDiffFileTreeNode 将目录排在文件前，并按名称排序。
function sortDiffFileTreeNode(
  // Directory node whose descendants should be sorted.
  // 需要排序其后代的目录节点。
  node: DiffFileTreeNode,
): void {
  node.children.sort(compareDiffFileTreeNodes);
  for (const child of node.children) {
    if (!child.file) {
      sortDiffFileTreeNode(child);
    }
  }
}

// compressDiffFileTreeDirectoryChains merges single-child directory chains into compact path rows.
// compressDiffFileTreeDirectoryChains 将单子目录链合并为紧凑路径行。
function compressDiffFileTreeDirectoryChains(
  // Mutable root node whose directory descendants should be compacted.
  // 需要压缩其目录后代的可变根节点。
  root: DiffFileTreeNode,
): void {
  root.children = root.children.map(compressDiffFileTreeDirectoryNode);
}

// compressDiffFileTreeDirectoryNode returns one node with folder-only chains merged.
// compressDiffFileTreeDirectoryNode 返回已合并纯目录链的单个节点。
function compressDiffFileTreeDirectoryNode(
  // Tree node to compact.
  // 需要压缩的目录树节点。
  node: DiffFileTreeNode,
): DiffFileTreeNode {
  if (node.file) {
    return node;
  }
  let compacted: DiffFileTreeNode = {
    ...node,
    children: node.children.map(compressDiffFileTreeDirectoryNode),
  };
  while (compacted.children.length === 1 && !compacted.children[0].file) {
    const child = compacted.children[0];
    compacted = {
      ...child,
      name: `${compacted.name}/${child.name}`,
    };
  }
  return compacted;
}

// compareDiffFileTreeNodes compares tree nodes for directory-first display.
// compareDiffFileTreeNodes 按目录优先的展示方式比较树节点。
function compareDiffFileTreeNodes(
  // Left tree node.
  // 左侧树节点。
  left: DiffFileTreeNode,
  // Right tree node.
  // 右侧树节点。
  right: DiffFileTreeNode,
): number {
  if (Boolean(left.file) !== Boolean(right.file)) {
    return left.file ? 1 : -1;
  }
  return left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: "base" });
}

// defaultDiffTreeOpenDirectoryPaths returns the initial expanded directory set.
// defaultDiffTreeOpenDirectoryPaths 返回目录树初始展开目录集合。
function defaultDiffTreeOpenDirectoryPaths(
  // Tree root to inspect.
  // 需要检查的目录树根节点。
  root: DiffFileTreeNode,
): ReadonlySet<string> {
  const openPaths = new Set<string>();
  collectDefaultDiffTreeOpenDirectoryPaths(root, openPaths);
  return openPaths;
}

// collectDefaultDiffTreeOpenDirectoryPaths expands all visible directories for fast scanning.
// collectDefaultDiffTreeOpenDirectoryPaths 展开所有可见目录，方便快速浏览。
function collectDefaultDiffTreeOpenDirectoryPaths(
  // Current tree node.
  // 当前目录树节点。
  node: DiffFileTreeNode,
  // Mutable set receiving directory paths.
  // 接收目录路径的可变集合。
  openPaths: Set<string>,
): void {
  for (const child of node.children) {
    if (child.file) {
      continue;
    }
    openPaths.add(child.path);
    collectDefaultDiffTreeOpenDirectoryPaths(child, openPaths);
  }
}

// diffFileTreeExtensionLabel returns a compact file-type label for the tree icon slot.
// diffFileTreeExtensionLabel 返回目录树图标位使用的紧凑文件类型标签。
function diffFileTreeExtensionLabel(
  // File name displayed by the tree row.
  // 目录树行展示的文件名。
  name: string,
): string {
  const extension = name.includes(".") ? name.split(".").pop()?.trim() : "";
  return extension ? extension.slice(0, 3).toUpperCase() : "•";
}

// DiffSearchPageNotice displays only active search narrowing state.
// DiffSearchPageNotice 仅展示搜索缩小结果集时的状态。
function DiffSearchPageNotice(props: {
  // Search page status shown when search narrows rendered files.
  // 搜索缩小渲染文件时显示的分页状态。
  description: string;
}) {
  return (
    <div className="diff-snapshot-policy-note">
      <span>{props.description}</span>
    </div>
  );
}

// DiffSearchEmptyState displays an empty state when a query has no loaded-line matches.
// DiffSearchEmptyState 在查询没有已加载行命中时显示空状态。
function DiffSearchEmptyState(props: {
  // Current search query.
  // 当前搜索查询。
  query: string;
  // Number of loaded diff lines scanned.
  // 已扫描的已加载 diff 行数量。
  scannedLines: number;
  // Optional local worker search error.
  // 可选的本地 Worker 搜索错误。
  error?: string | null;
  // Whether summary-only files are being requested for this search.
  // 当前搜索是否正在请求摘要文件详情。
  prefetchingMore?: boolean;
}) {
  return (
    <div className="diff-empty-state in-list">
      <FileText size={20} />
      <strong>{props.error ? "搜索索引暂不可用" : props.prefetchingMore ? "正在加载更多文件" : "没有匹配结果"}</strong>
      <span>
        {props.error
          ? `已停止在主线程重建搜索索引：${props.error}`
          : props.prefetchingMore
            ? `已在 ${props.scannedLines} 行已加载 diff 中搜索 “${props.query.trim()}”，正在按需加载更多文件详情。`
            : `已在 ${props.scannedLines} 行已加载 diff 中搜索 “${props.query.trim()}”。`}
      </span>
    </div>
  );
}

// DiffSummarySearchState stores the compact backend file-hit search state.
// DiffSummarySearchState 保存紧凑后端文件命中搜索状态。
interface DiffSummarySearchState {
  // Stable request key for the active diff source and query.
  // 当前 diff 来源与搜索词对应的稳定请求键。
  requestKey: string;
  // Backend-trimmed query text.
  // 后端裁剪空白后的搜索词。
  query: string;
  // File-level matches returned by the backend.
  // 后端返回的文件级命中。
  matches: WorkspaceDiffSearchFileMatch[];
  // Whether the backend search request is still pending.
  // 后端搜索请求是否仍在进行。
  pending: boolean;
  // Whether the backend omitted additional matching files.
  // 后端是否省略了更多命中文件。
  truncated: boolean;
  // Number of backend diff lines scanned.
  // 后端已扫描的 diff 行数量。
  scannedLines: number;
  // Optional backend search error.
  // 可选的后端搜索错误。
  error: string | null;
}

// DiffSummarySearchRank stores a backend file-hit rank for hydration scoring.
// DiffSummarySearchRank 保存用于水合评分的后端文件命中排名。
interface DiffSummarySearchRank {
  // Zero-based match order returned by the backend.
  // 后端返回的从零开始的命中顺序。
  index: number;
  // Number of matched diff lines in the file.
  // 文件中的命中 diff 行数量。
  matchCount: number;
  // First matching hunk index returned by the backend when available.
  // 后端返回的首个命中 hunk 索引。
  firstHunkIndex?: number | null;
  // Original backend match used to build hunk-scoped requests.
  // 用于构建 hunk 级请求的原始后端命中。
  match: WorkspaceDiffSearchFileMatch;
}

// DiffSearchDetailPrefetchCandidate stores one search-driven detail hydration request.
// DiffSearchDetailPrefetchCandidate 保存一个搜索驱动的详情水合请求。
interface DiffSearchDetailPrefetchCandidate {
  // File summary selected for detail hydration.
  // 被选中进行详情水合的文件摘要。
  file: DiffFile;
  // Optional context line count for hunk-scoped search hydration.
  // hunk 级搜索水合使用的可选上下文行数。
  contextLines?: number;
  // Optional hunk expansion targeting the backend search hit.
  // 指向后端搜索命中的可选 hunk 展开请求。
  expansion?: DiffHunkExpansionRequest;
}

// DiffScoredSearchDetailPrefetchCandidate stores one ranked hydration candidate.
// DiffScoredSearchDetailPrefetchCandidate 保存一个已评分的详情水合候选。
interface DiffScoredSearchDetailPrefetchCandidate {
  // File summary selected for detail hydration.
  // 被选中进行详情水合的文件摘要。
  file: DiffFile;
  // File index in the current snapshot.
  // 当前快照中的文件索引。
  fileIndex: number;
  // Lower score means the file should hydrate earlier.
  // 分数越低表示文件越应该优先水合。
  score: number;
  // Optional backend search hit used to focus hunk hydration.
  // 用于聚焦 hunk 水合的可选后端搜索命中。
  summaryMatch?: WorkspaceDiffSearchFileMatch;
}

// DiffSnapshotFileIndexEntry stores one file and its original snapshot position.
// DiffSnapshotFileIndexEntry 保存单个文件及其在原始快照中的位置。
interface DiffSnapshotFileIndexEntry {
  // File object from the source snapshot.
  // 源快照中的文件对象。
  file: DiffFile;
  // Zero-based file position in the source snapshot.
  // 文件在源快照中的从零开始的位置。
  index: number;
}

// DiffSummarySearchDerived stores all first-order indexes for one backend search result array.
// DiffSummarySearchDerived 保存一组后端搜索结果的一阶派生索引。
interface DiffSummarySearchDerived {
  // First backend match by file key.
  // 每个文件 key 对应的首个后端命中。
  matchByFileKey: Map<string, WorkspaceDiffSearchFileMatch>;
  // First page of backend-hit file keys shown before detail hydration.
  // 详情水合前展示的首屏后端命中文件 key。
  pageFileKeys: Set<string>;
  // Ranked backend hits by file key for detail prefetch scoring.
  // 用于详情预取评分的按文件 key 排名信息。
  rankByKey: Map<string, DiffSummarySearchRank>;
}

// diffFilePathTextCache stores normalized path search text per stable diff file object.
// diffFilePathTextCache 按稳定 diff 文件对象缓存归一化路径搜索文本。
const diffFilePathTextCache = new WeakMap<DiffFile, string>();

// projectedSearchSnapshotCache stores search-projected snapshots by source snapshot and file-key set identity.
// projectedSearchSnapshotCache 按源 snapshot 与文件 key 集合身份缓存搜索投影后的 snapshot。
const projectedSearchSnapshotCache = new WeakMap<DiffPanelSnapshot, WeakMap<ReadonlySet<string>, DiffPanelSnapshot>>();

// projectedSearchSnapshotFileIndexCache stores file lookup indexes per source snapshot.
// projectedSearchSnapshotFileIndexCache 按源 snapshot 缓存文件查找索引。
const projectedSearchSnapshotFileIndexCache = new WeakMap<DiffPanelSnapshot, Map<string, DiffSnapshotFileIndexEntry>>();

// EMPTY_SUMMARY_SEARCH_MATCHES keeps inactive backend search matches reference-stable.
// EMPTY_SUMMARY_SEARCH_MATCHES 保持未激活后端搜索命中列表引用稳定。
const EMPTY_SUMMARY_SEARCH_MATCHES: readonly WorkspaceDiffSearchFileMatch[] = Object.freeze([]);

// EMPTY_SUMMARY_SEARCH_DERIVED keeps inactive backend search indexes reference-stable.
// EMPTY_SUMMARY_SEARCH_DERIVED 保持未激活后端搜索派生索引引用稳定。
const EMPTY_SUMMARY_SEARCH_DERIVED: DiffSummarySearchDerived = Object.freeze({
  matchByFileKey: new Map<string, WorkspaceDiffSearchFileMatch>(),
  pageFileKeys: new Set<string>(),
  rankByKey: new Map<string, DiffSummarySearchRank>(),
});

// summarySearchDerivedCache stores all derived indexes per backend match array.
// summarySearchDerivedCache 按后端命中数组缓存全部派生索引。
const summarySearchDerivedCache = new WeakMap<readonly WorkspaceDiffSearchFileMatch[], DiffSummarySearchDerived>();

// EMPTY_DIFF_SUMMARY_SEARCH_STATE keeps inactive backend search state reference-stable.
// EMPTY_DIFF_SUMMARY_SEARCH_STATE 保持未激活后端搜索状态引用稳定。
const EMPTY_DIFF_SUMMARY_SEARCH_STATE: DiffSummarySearchState = Object.freeze({
  requestKey: "",
  query: "",
  matches: [],
  pending: false,
  truncated: false,
  scannedLines: 0,
  error: null,
});

// DIFF_FILE_TREE_HOVER_DELAY_MS delays drawer hover changes to avoid accidental drag-triggered opens.
// DIFF_FILE_TREE_HOVER_DELAY_MS 延迟目录抽屉 hover 状态变化，避免拖拽时误触展开。
const DIFF_FILE_TREE_HOVER_DELAY_MS = 500;

// EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS keeps inactive hydration effects reference-stable.
// EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS 保持未激活水合 effect 的请求列表引用稳定。
const EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS: readonly DiffSearchDetailPrefetchCandidate[] = Object.freeze([]);

// diffDetailHydrationRequestsWithoutDuplicates removes requests covered by an earlier queue.
// diffDetailHydrationRequestsWithoutDuplicates 移除已被较早队列覆盖的请求。
function diffDetailHydrationRequestsWithoutDuplicates(
  // Candidate requests to filter.
  // 待过滤的候选请求。
  candidates: readonly DiffSearchDetailPrefetchCandidate[],
  // Requests that will already be dispatched first.
  // 已经会优先派发的请求。
  existing: readonly DiffSearchDetailPrefetchCandidate[],
): readonly DiffSearchDetailPrefetchCandidate[] {
  if (!candidates.length || !existing.length) {
    return candidates.length ? candidates : EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS;
  }
  const existingKeys = new Set<string>();
  for (const request of existing) {
    existingKeys.add(diffDetailHydrationRequestKey(request));
  }
  let filtered: DiffSearchDetailPrefetchCandidate[] | null = null;
  for (let index = 0; index < candidates.length; index += 1) {
    const request = candidates[index];
    if (existingKeys.has(diffDetailHydrationRequestKey(request))) {
      if (!filtered) {
        // filtered starts with the already accepted prefix only after the first duplicate appears.
        // filtered 只在首次发现重复后，才复制此前已接受的前缀。
        filtered = candidates.slice(0, index);
      }
      continue;
    }
    filtered?.push(request);
  }
  return filtered ? (filtered.length ? filtered : EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS) : candidates;
}

// diffDetailHydrationRequestKey identifies equivalent detail hydration requests.
// diffDetailHydrationRequestKey 标识等价的详情水合请求。
function diffDetailHydrationRequestKey(request: DiffSearchDetailPrefetchCandidate): string {
  return [
    request.file.key,
    request.contextLines ?? 0,
    request.expansion?.hunkIndex ?? "",
    request.expansion?.direction ?? "",
  ].join("\u0000");
}

// scheduleDiffDetailHydrationRequests dispatches detail loads over multiple frames.
// scheduleDiffDetailHydrationRequests 将详情加载分散到多个帧中派发。
function scheduleDiffDetailHydrationRequests(
  // Requests to dispatch in order.
  // 需要按顺序派发的请求。
  requests: readonly DiffSearchDetailPrefetchCandidate[],
  // Host detail loader.
  // 宿主详情加载函数。
  onLoadFileDetail: DiffFileDetailLoader,
): () => void {
  // cancelled stops already scheduled callbacks after the owning effect is cleaned up.
  // cancelled 在所属 effect 清理后阻止已调度回调继续执行。
  let cancelled = false;
  // nextIndex points at the next hydration request to dispatch.
  // nextIndex 指向下一条需要派发的水合请求。
  let nextIndex = 0;
  // frameId stores the pending animation frame callback id.
  // frameId 保存等待中的动画帧回调编号。
  let frameId: number | null = null;
  // timeoutId stores the pending delayed dispatch callback id.
  // timeoutId 保存等待中的延迟派发回调编号。
  let timeoutId: number | null = null;

  // clearScheduledDispatch cancels pending browser callbacks.
  // clearScheduledDispatch 取消等待中的浏览器回调。
  const clearScheduledDispatch = () => {
    if (typeof window === "undefined") {
      return;
    }
    if (frameId != null) {
      window.cancelAnimationFrame(frameId);
      frameId = null;
    }
    if (timeoutId != null) {
      window.clearTimeout(timeoutId);
      timeoutId = null;
    }
  };

  // dispatchNext sends one detail request and schedules the following request if needed.
  // dispatchNext 派发一个详情请求，并在需要时继续调度下一条请求。
  const dispatchNext = () => {
    frameId = null;
    timeoutId = null;
    if (cancelled) {
      return;
    }
    const request = requests[nextIndex];
    if (!request) {
      return;
    }
    onLoadFileDetail(request.file, request.contextLines, request.expansion);
    nextIndex += 1;
    if (nextIndex < requests.length) {
      scheduleNext();
    }
  };

  // scheduleNext spaces detail loads so scrolling and rendering can paint between requests.
  // scheduleNext 拉开详情加载间隔，让滚动与渲染能在请求之间完成绘制。
  const scheduleNext = () => {
    if (typeof window === "undefined") {
      dispatchNext();
      return;
    }
    frameId = window.requestAnimationFrame(() => {
      frameId = null;
      timeoutId = window.setTimeout(dispatchNext, DIFF_SEARCH_DETAIL_PREFETCH_DISPATCH_DELAY_MS);
    });
  };

  scheduleNext();
  return () => {
    cancelled = true;
    clearScheduledDispatch();
  };
}

// diffReviewScrollResetKey identifies diff source changes that should reset the virtual scroll window.
// diffReviewScrollResetKey 标识需要重置虚拟滚动窗口的 diff 来源变化。
function diffReviewScrollResetKey(
  // Current parsed diff snapshot.
  // 当前已解析的 diff 快照。
  snapshot: DiffPanelSnapshot | null,
  // Current user-selected diff source.
  // 当前用户选择的 diff 来源。
  selection: DiffReviewSelection,
): string {
  if (!snapshot) {
    return ["empty", selection.mode].join("\u0000");
  }
  return [
    snapshot.repositoryRoot ?? "",
    snapshot.source,
    selection.mode,
    selection.baseBranch ?? snapshot.selectedBaseBranch ?? "",
    selection.compareBranch ?? snapshot.selectedCompareBranch ?? "",
    selection.commitHash ?? snapshot.selectedCommitHash ?? "",
  ].join("\u0000");
}

// diffSummarySearchRequestKey builds a stable backend search scope key.
// diffSummarySearchRequestKey 构建稳定的后端搜索作用域键。
function diffSummarySearchRequestKey(
  snapshot: DiffPanelSnapshot | null,
  selection: DiffReviewSelection,
  query: string,
): string {
  const normalizedQuery = query.trim();
  if (!snapshot || !normalizedQuery) {
    return "";
  }
  return [
    snapshot.repositoryRoot ?? "",
    snapshot.source,
    selection.mode,
    selection.baseBranch ?? "",
    selection.compareBranch ?? "",
    selection.commitHash ?? "",
    snapshot.selectedBaseBranch ?? "",
    snapshot.selectedCompareBranch ?? "",
    snapshot.selectedCommitHash ?? "",
    normalizedQuery,
  ].join("\u0000");
}

// diffSummarySearchTargetKey builds a stable key for applying one backend target once.
// diffSummarySearchTargetKey 构建用于一次性应用后端目标的稳定键。
function diffSummarySearchTargetKey(scopeKey: string, target: WorkspaceDiffSearchFileMatch): string {
  if (!scopeKey || !target.key) {
    return "";
  }
  return [
    scopeKey,
    target.key,
    target.firstHunkIndex ?? "",
    target.firstOldLine ?? "",
    target.firstNewLine ?? "",
    target.firstLineKind ?? "",
  ].join("\u0000");
}

// diffSummarySearchDerived returns all cached indexes for backend search hits in one pass.
// diffSummarySearchDerived 单次遍历返回后端搜索命中的全部缓存索引。
function diffSummarySearchDerived(matches: readonly WorkspaceDiffSearchFileMatch[]): DiffSummarySearchDerived {
  if (!matches.length) {
    return EMPTY_SUMMARY_SEARCH_DERIVED;
  }
  const cached = summarySearchDerivedCache.get(matches);
  if (cached) {
    return cached;
  }
  const matchByFileKey = new Map<string, WorkspaceDiffSearchFileMatch>();
  const pageFileKeys = new Set<string>();
  const rankByKey = new Map<string, DiffSummarySearchRank>();
  for (let index = 0; index < matches.length; index += 1) {
    const match = matches[index];
    if (!matchByFileKey.has(match.key)) {
      matchByFileKey.set(match.key, match);
    }
    if (pageFileKeys.size < DIFF_SEARCH_PAGE_SIZE) {
      pageFileKeys.add(match.key);
    }
    rankByKey.set(match.key, {
      index,
      matchCount: match.matchCount,
      firstHunkIndex: match.firstHunkIndex,
      match,
    });
  }
  const derived = { matchByFileKey, pageFileKeys, rankByKey };
  summarySearchDerivedCache.set(matches, derived);
  return derived;
}

// diffSearchIndexForSummaryTarget finds the loaded-line match closest to one backend file hit.
// diffSearchIndexForSummaryTarget 查找最接近单个后端文件命中的已加载行命中。
function diffSearchIndexForSummaryTarget(
  matches: readonly DiffSearchMatch[],
  target: WorkspaceDiffSearchFileMatch,
): number {
  let firstFileMatchIndex = -1;
  let exactMatchIndex = -1;
  let hunkMatchIndex = -1;
  for (let index = 0; index < matches.length; index += 1) {
    const match = matches[index];
    if (match.fileKey !== target.key) {
      continue;
    }
    if (firstFileMatchIndex < 0) {
      firstFileMatchIndex = index;
    }
    if (hunkMatchIndex < 0 && target.firstHunkIndex != null && match.hunkIndex === target.firstHunkIndex) {
      hunkMatchIndex = index;
    }
    if (exactMatchIndex < 0 && diffSearchMatchMatchesSummaryTarget(match, target)) {
      exactMatchIndex = index;
    }
  }
  if (exactMatchIndex >= 0) {
    return exactMatchIndex;
  }
  if (hunkMatchIndex >= 0) {
    return hunkMatchIndex;
  }
  return firstFileMatchIndex;
}

// diffSearchMatchMatchesSummaryTarget checks whether one loaded match exactly matches backend hit metadata.
// diffSearchMatchMatchesSummaryTarget 检查单个已加载命中是否精确匹配后端命中元数据。
function diffSearchMatchMatchesSummaryTarget(match: DiffSearchMatch, target: WorkspaceDiffSearchFileMatch): boolean {
  if (match.fileKey !== target.key) {
    return false;
  }
  if (target.firstHunkIndex != null && match.hunkIndex !== target.firstHunkIndex) {
    return false;
  }
  if (target.firstLineKind && target.firstLineKind !== "hunk" && match.kind !== target.firstLineKind) {
    return false;
  }
  if (target.firstOldLine != null && match.oldLine !== target.firstOldLine) {
    return false;
  }
  if (target.firstNewLine != null && match.newLine !== target.firstNewLine) {
    return false;
  }
  return true;
}

// projectedSearchSnapshot narrows rendered files to the active search file page.
// projectedSearchSnapshot 将渲染文件缩小到当前搜索文件分页。
function projectedSearchSnapshot(
  snapshot: DiffPanelSnapshot,
  query: string,
  matchCount: number,
  fileKeys: ReadonlySet<string>,
  summaryFileKeys: ReadonlySet<string>,
): DiffPanelSnapshot {
  if (!query.trim()) {
    return snapshot;
  }
  if (matchCount <= 0 && summaryFileKeys.size <= 0) {
    return snapshot;
  }
  const projectedFileKeys = matchCount > 0 ? fileKeys : summaryFileKeys;
  if (projectedFileKeys.size <= 0) {
    return snapshot;
  }
  const cachedProjection = cachedProjectedSearchSnapshot(snapshot, projectedFileKeys);
  if (cachedProjection) {
    return cachedProjection;
  }
  const projectedFiles = projectedSnapshotFiles(snapshot, projectedFileKeys);
  if (projectedFiles.length === snapshot.files.length) {
    rememberProjectedSearchSnapshot(snapshot, projectedFileKeys, snapshot);
    return snapshot;
  }
  const projectedSnapshot = {
    ...snapshot,
    files: projectedFiles,
  };
  rememberProjectedSearchSnapshot(snapshot, projectedFileKeys, projectedSnapshot);
  return projectedSnapshot;
}

// projectedSnapshotFiles maps a small search file page back to source files in original order.
// projectedSnapshotFiles 将小规模搜索文件分页映射回原始顺序的源文件列表。
function projectedSnapshotFiles(snapshot: DiffPanelSnapshot, fileKeys: ReadonlySet<string>): DiffFile[] {
  const fileIndexByKey = diffSnapshotFileIndexByKey(snapshot);
  const projectedEntries: DiffSnapshotFileIndexEntry[] = [];
  for (const fileKey of fileKeys) {
    const entry = fileIndexByKey.get(fileKey);
    if (entry) {
      projectedEntries.push(entry);
    }
  }
  if (projectedEntries.length > 1) {
    projectedEntries.sort(compareDiffSnapshotFileIndexEntries);
  }
  const projectedFiles: DiffFile[] = new Array(projectedEntries.length);
  for (let index = 0; index < projectedEntries.length; index += 1) {
    projectedFiles[index] = projectedEntries[index].file;
  }
  return projectedFiles;
}

// diffSnapshotFileIndexByKey returns an O(1) file lookup index for one stable snapshot.
// diffSnapshotFileIndexByKey 返回单个稳定 snapshot 的 O(1) 文件查找索引。
function diffSnapshotFileIndexByKey(snapshot: DiffPanelSnapshot): Map<string, DiffSnapshotFileIndexEntry> {
  const cached = projectedSearchSnapshotFileIndexCache.get(snapshot);
  if (cached) {
    return cached;
  }
  const fileIndexByKey = new Map<string, DiffSnapshotFileIndexEntry>();
  for (let index = 0; index < snapshot.files.length; index += 1) {
    const file = snapshot.files[index];
    fileIndexByKey.set(file.key, { file, index });
  }
  projectedSearchSnapshotFileIndexCache.set(snapshot, fileIndexByKey);
  return fileIndexByKey;
}

// compareDiffSnapshotFileIndexEntries keeps projected search files in original snapshot order.
// compareDiffSnapshotFileIndexEntries 保持搜索投影文件与原始快照顺序一致。
function compareDiffSnapshotFileIndexEntries(
  // Left indexed file entry.
  // 左侧带索引的文件条目。
  left: DiffSnapshotFileIndexEntry,
  // Right indexed file entry.
  // 右侧带索引的文件条目。
  right: DiffSnapshotFileIndexEntry,
): number {
  return left.index - right.index;
}

// cachedProjectedSearchSnapshot reads a cached search projection for one stable file-key set.
// cachedProjectedSearchSnapshot 读取指定稳定文件 key 集合对应的搜索投影缓存。
function cachedProjectedSearchSnapshot(snapshot: DiffPanelSnapshot, fileKeys: ReadonlySet<string>): DiffPanelSnapshot | null {
  return projectedSearchSnapshotCache.get(snapshot)?.get(fileKeys) ?? null;
}

// rememberProjectedSearchSnapshot stores one projected snapshot without retaining old source snapshots forever.
// rememberProjectedSearchSnapshot 缓存单个投影 snapshot，且不会永久保留旧源 snapshot。
function rememberProjectedSearchSnapshot(
  snapshot: DiffPanelSnapshot,
  fileKeys: ReadonlySet<string>,
  projectedSnapshot: DiffPanelSnapshot,
): void {
  let projectionByFileKeys = projectedSearchSnapshotCache.get(snapshot);
  if (!projectionByFileKeys) {
    projectionByFileKeys = new WeakMap();
    projectedSearchSnapshotCache.set(snapshot, projectionByFileKeys);
  }
  projectionByFileKeys.set(fileKeys, projectedSnapshot);
}

// searchFilePageDescription formats the search-mode file paging notice.
// searchFilePageDescription 格式化搜索态文件分页提示。
function searchFilePageDescription(
  pageMatches: DiffSearchMatch[],
  pageFileKeys: ReadonlySet<string>,
  totalMatches: number,
): string {
  const firstMatch = pageMatches[0];
  const lastMatch = pageMatches[pageMatches.length - 1];
  if (!firstMatch || !lastMatch) {
    return "搜索结果按匹配文件分页展示";
  }
  return `搜索结果按匹配文件分页展示：当前 ${pageFileKeys.size} 个文件，显示 ${pageMatches.length} / ${totalMatches} 条命中`;
}

// summarySearchFilePageDescription formats backend-hit file paging before detail hydration finishes.
// summarySearchFilePageDescription 格式化详情水合完成前的后端命中文件分页提示。
function summarySearchFilePageDescription(
  matches: readonly WorkspaceDiffSearchFileMatch[],
  truncated: boolean,
): string {
  const visibleFileCount = Math.min(DIFF_SEARCH_PAGE_SIZE, matches.length);
  return `后端已在完整 diff 中找到 ${matches.length}${truncated ? "+" : ""} 个命中文件，当前展示前 ${visibleFileCount} 个并按需水合详情`;
}

// isPromiseLike checks whether a refresh result exposes a finalizer.
// isPromiseLike 检查刷新结果是否提供完成回调。
function isPromiseLike(
  // Unknown refresh return value from the optional host callback.
  // 可选宿主回调返回的未知刷新结果。
  value: unknown,
): value is Promise<void> {
  return Boolean(value)
    && (typeof value === "object" || typeof value === "function")
    && typeof (value as Promise<void>).finally === "function";
}

// diffCollapsedFileKeys builds the default-collapsed, single-expanded file key set.
// diffCollapsedFileKeys 构建默认全折叠且仅允许单个文件展开的 key 集合。
function diffCollapsedFileKeys(
  // Current rendered snapshot after search projection.
  // 搜索投影后的当前渲染快照。
  snapshot: DiffPanelSnapshot | null,
  // The only file key allowed to stay expanded.
  // 唯一允许保持展开的文件 key。
  expandedFileKey: string | null,
): ReadonlySet<string> {
  const collapsed = new Set<string>();
  if (!snapshot) {
    return collapsed;
  }
  for (const file of snapshot.files) {
    if (file.key !== expandedFileKey) {
      collapsed.add(file.key);
    }
  }
  return collapsed;
}

// diffAbsoluteFilePath returns the best local path for revealing one diff file.
// diffAbsoluteFilePath 返回用于在文件管理器中定位单个 diff 文件的最佳本地路径。
function diffAbsoluteFilePath(
  // Repository root returned by the backend.
  // 后端返回的仓库根目录。
  repositoryRoot: string | null | undefined,
  // File whose current or previous path should be revealed.
  // 需要定位当前路径或旧路径的文件。
  file: DiffFile,
): string | null {
  const root = repositoryRoot?.trim();
  if (!root) {
    return null;
  }
  const relativePath = diffRevealRelativePath(file);
  if (!relativePath) {
    return root;
  }
  const separator = root.includes("\\") ? "\\" : "/";
  return `${root.replace(/[\\/]+$/, "")}${separator}${relativePath.replace(/[\\/]+/g, separator)}`;
}

// diffRevealRelativePath returns the non-null side path for one diff file.
// diffRevealRelativePath 返回单个 diff 文件可定位的非空侧路径。
function diffRevealRelativePath(
  // File whose visible path should be normalized for the host file manager.
  // 需要为宿主文件管理器归一化可见路径的文件。
  file: DiffFile,
): string | null {
  const path = file.newPath && file.newPath !== "/dev/null" ? file.newPath : file.oldPath;
  const normalized = path.trim();
  return normalized && normalized !== "/dev/null" ? normalized : null;
}

// shouldLoadDiffFileDetail returns whether one mounted summary needs backend detail hydration.
// shouldLoadDiffFileDetail 返回单个已挂载摘要是否需要后端详情水合。
function shouldLoadDiffFileDetail(file: DiffFile): boolean {
  return Boolean(file.isPartial && !file.tooLargeToDisplay && !file.isBinary && !file.isGitlink && file.changedLines > 0);
}

// diffSearchDetailPrefetchCandidates selects a small batch of summary-only files to hydrate for search.
// diffSearchDetailPrefetchCandidates 为搜索选择一小批需要水合的摘要文件。
function diffSearchDetailPrefetchCandidates(
  snapshot: DiffPanelSnapshot,
  query: string,
  loadedMatches: readonly DiffSearchMatch[],
  activeMatchIndex: number,
  summaryMatches: readonly WorkspaceDiffSearchFileMatch[] = [],
): readonly DiffSearchDetailPrefetchCandidate[] {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) {
    return EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS;
  }
  const activeMatch = loadedMatches[clampDiffSearchIndex(activeMatchIndex, loadedMatches.length)] ?? null;
  const activeFileIndex = activeMatch?.fileIndex ?? 0;
  const shouldContinueContentPrefetch = loadedMatches.length < DIFF_SEARCH_DETAIL_PREFETCH_TARGET_MATCHES;
  const summaryMatchRankByKey = diffSummarySearchDerived(summaryMatches).rankByKey;
  const hasSummaryMatches = summaryMatchRankByKey.size > 0;
  const scoredCandidates: DiffScoredSearchDetailPrefetchCandidate[] = [];
  for (let fileIndex = 0; fileIndex < snapshot.files.length; fileIndex += 1) {
    const file = snapshot.files[fileIndex];
    if (!shouldPrefetchDiffFileDetailForSearch(file)) {
      continue;
    }
    const summaryMatch = summaryMatchRankByKey.get(file.key);
    const score = diffSearchDetailPrefetchScore(
      file,
      fileIndex,
      normalizedQuery,
      activeFileIndex,
      shouldContinueContentPrefetch,
      summaryMatch,
      hasSummaryMatches,
    );
    if (score < Number.POSITIVE_INFINITY) {
      insertBoundedSearchDetailPrefetchCandidate(scoredCandidates, {
        file,
        fileIndex,
        score,
        summaryMatch: summaryMatch?.match,
      });
    }
  }
  const candidates: DiffSearchDetailPrefetchCandidate[] = new Array(scoredCandidates.length);
  for (let index = 0; index < scoredCandidates.length; index += 1) {
    const candidate = scoredCandidates[index];
    candidates[index] = diffSearchDetailPrefetchCandidateForFile(candidate.file, candidate.summaryMatch);
  }
  return candidates.length ? candidates : EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS;
}

// insertBoundedSearchDetailPrefetchCandidate keeps only the best prefetch candidates in score order.
// insertBoundedSearchDetailPrefetchCandidate 只按分数顺序保留最优预取候选。
function insertBoundedSearchDetailPrefetchCandidate(
  // Existing bounded candidates sorted from best to worst.
  // 已有的有界候选，按优先级从高到低排序。
  candidates: DiffScoredSearchDetailPrefetchCandidate[],
  // Candidate to insert if it ranks high enough.
  // 可能需要插入的候选。
  candidate: DiffScoredSearchDetailPrefetchCandidate,
): void {
  let insertIndex = candidates.length;
  for (let index = 0; index < candidates.length; index += 1) {
    if (compareSearchDetailPrefetchCandidates(candidate, candidates[index]) < 0) {
      insertIndex = index;
      break;
    }
  }
  if (insertIndex >= DIFF_SEARCH_DETAIL_PREFETCH_LIMIT) {
    return;
  }
  candidates.splice(insertIndex, 0, candidate);
  if (candidates.length > DIFF_SEARCH_DETAIL_PREFETCH_LIMIT) {
    candidates.pop();
  }
}

// compareSearchDetailPrefetchCandidates preserves the previous score and file-order ranking.
// compareSearchDetailPrefetchCandidates 保持原有分数与文件顺序排序语义。
function compareSearchDetailPrefetchCandidates(
  // Left candidate.
  // 左侧候选。
  left: DiffScoredSearchDetailPrefetchCandidate,
  // Right candidate.
  // 右侧候选。
  right: DiffScoredSearchDetailPrefetchCandidate,
): number {
  return left.score - right.score || left.fileIndex - right.fileIndex;
}

// diffSearchDetailPrefetchCandidateForFile builds a focused hunk request when backend search found a hunk.
// diffSearchDetailPrefetchCandidateForFile 在后端搜索命中 hunk 时构建聚焦 hunk 请求。
function diffSearchDetailPrefetchCandidateForFile(
  file: DiffFile,
  summaryMatch?: WorkspaceDiffSearchFileMatch,
): DiffSearchDetailPrefetchCandidate {
  if (summaryMatch?.firstHunkIndex == null) {
    return { file };
  }
  return {
    file,
    contextLines: DIFF_SEARCH_DETAIL_PREFETCH_CONTEXT_LINES,
    expansion: {
      hunkIndex: summaryMatch.firstHunkIndex,
      direction: "both",
    },
  };
}

// diffFilePathText returns normalized searchable path text for one file.
// diffFilePathText 返回单个文件归一化后的可搜索路径文本。
function diffFilePathText(file: DiffFile): string {
  const cached = diffFilePathTextCache.get(file);
  if (cached != null) {
    return cached;
  }
  const normalized = `${file.oldPath}\n${file.newPath}`.toLowerCase();
  diffFilePathTextCache.set(file, normalized);
  return normalized;
}

// shouldPrefetchDiffFileDetailForSearch returns whether search may hydrate one summary file automatically.
// shouldPrefetchDiffFileDetailForSearch 返回搜索是否可以自动水合一个摘要文件。
function shouldPrefetchDiffFileDetailForSearch(file: DiffFile): boolean {
  const status = file.fileDiffLoadState?.status;
  return shouldLoadDiffFileDetail(file) && status !== "loading" && status !== "failed" && status !== "timed-out";
}

// diffSearchDetailPrefetchScore ranks summary files by direct path hits, locality, and hydration cost.
// diffSearchDetailPrefetchScore 按路径命中、当前位置接近度和水合成本为摘要文件排序。
function diffSearchDetailPrefetchScore(
  file: DiffFile,
  fileIndex: number,
  normalizedQuery: string,
  activeFileIndex: number,
  shouldContinueContentPrefetch: boolean,
  summaryMatchRank?: DiffSummarySearchRank,
  hasSummaryMatches = false,
): number {
  const estimatedCost = Math.min(file.estimatedRowCount ?? file.changedLines, 10_000);
  if (summaryMatchRank) {
    const firstHunkBias = Math.min(summaryMatchRank.firstHunkIndex ?? 0, 99);
    return summaryMatchRank.index * 1_000 + firstHunkBias + Math.min(estimatedCost, 999) - Math.min(summaryMatchRank.matchCount, 50);
  }
  const pathMatches = diffFilePathText(file).includes(normalizedQuery);
  if (!pathMatches && (hasSummaryMatches || !shouldContinueContentPrefetch)) {
    return Number.POSITIVE_INFINITY;
  }
  const pathBias = pathMatches ? 0 : 100_000;
  const locality = Math.abs(fileIndex - activeFileIndex);
  return pathBias + locality * 100 + estimatedCost;
}

// nextSearchIndex moves to the next search result with wraparound.
// nextSearchIndex 以循环方式移动到下一条搜索结果。
function nextSearchIndex(index: number, matchCount: number): number {
  if (matchCount <= 0) {
    return 0;
  }
  return (clampDiffSearchIndex(index, matchCount) + 1) % matchCount;
}

// previousSearchIndex moves to the previous search result with wraparound.
// previousSearchIndex 以循环方式移动到上一条搜索结果。
function previousSearchIndex(index: number, matchCount: number): number {
  if (matchCount <= 0) {
    return 0;
  }
  return (clampDiffSearchIndex(index, matchCount) - 1 + matchCount) % matchCount;
}

// searchResultLabel formats active result position and capped totals.
// searchResultLabel 格式化活跃结果位置和被截断的总数。
function searchResultLabel(matchCount: number, activeIndex: number, truncated: boolean): string {
  if (matchCount <= 0) {
    return "0/0";
  }
  return `${activeIndex + 1}/${matchCount}${truncated ? "+" : ""}`;
}

// activeDiffSearchLineKeyForFile scopes the active line key to the file that owns it.
// activeDiffSearchLineKeyForFile 将当前活跃行键限制到其所属文件。
function activeDiffSearchLineKeyForFile(
  // File key for the card being rendered.
  // 当前正在渲染的文件卡片 key。
  fileKey: string,
  // Active loaded-line search match.
  // 当前活跃的已加载行搜索命中。
  activeMatch: DiffSearchMatch | null,
): string | null {
  return activeMatch?.fileKey === fileKey ? activeMatch.lineKey : null;
}

// scrollToDiffSearchMatch scrolls to a matched line using estimated virtual offsets, then refines by DOM.
// scrollToDiffSearchMatch 使用估算虚拟偏移滚动到命中行，然后通过 DOM 精调。
function scrollToDiffSearchMatch(
  container: HTMLDivElement | null,
  layout: DiffFileVirtualLayout,
  match: DiffSearchMatch,
): void {
  if (!container) {
    return;
  }
  const scrollTop = diffScrollTopForLine(layout, match.fileKey, match.hunkIndex, match.lineIndex);
  if (scrollTop == null) {
    return;
  }
  container.scrollTop = scrollTop;
  window.requestAnimationFrame(() => {
    const lineElement = container.querySelector<HTMLElement>(`[data-diff-line-key="${match.lineKey}"]`);
    lineElement?.scrollIntoView({ block: "center" });
  });
}

// scrollToDiffSummarySearchMatch scrolls to a backend file-level hit before detail hydration finishes.
// scrollToDiffSummarySearchMatch 在详情水合完成前滚动到后端文件级命中。
function scrollToDiffSummarySearchMatch(
  container: HTMLDivElement | null,
  layout: DiffFileVirtualLayout,
  match: WorkspaceDiffSearchFileMatch,
): boolean {
  if (!container) {
    return false;
  }
  const scrollTop = diffScrollTopForSummarySearchMatch(layout, match);
  if (scrollTop == null) {
    return false;
  }
  container.scrollTop = scrollTop;
  window.requestAnimationFrame(() => {
    const placeholder = container.querySelector<HTMLElement>(
      `[data-diff-summary-search-file-key="${cssSelectorString(match.key)}"]`,
    );
    placeholder?.scrollIntoView({ block: "center" });
  });
  return true;
}

// cssSelectorString escapes a value for a quoted CSS attribute selector.
// cssSelectorString 将字符串转义为可放入 CSS 属性选择器引号内的值。
function cssSelectorString(value: string): string {
  let escaped = "";
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    const character = value[index];
    if (character === "\\" || character === "\"") {
      escaped += `\\${character}`;
    } else if (code === 0) {
      escaped += "\uFFFD";
    } else if (code >= 1 && code <= 31 || code === 127) {
      escaped += `\\${code.toString(16)} `;
    } else {
      escaped += character;
    }
  }
  return escaped;
}
