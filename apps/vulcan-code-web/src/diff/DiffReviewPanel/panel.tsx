import React from "react";
import { FileText, RefreshCw } from "lucide-react";

import { WorkspaceDefaultFileIcon, WorkspaceFileTreeDrawer, WorkspaceFileTreeStats } from "@/components/workspace/WorkspaceFileTreeDrawer";
import { openProjectDirectoryPath } from "@/service/project-actions-service";
import { DiffFileCard, DiffFileHeader, useDiffScrollState } from "../diff-file-card";
import { DiffPanelToolbar } from "../diff-review-toolbar";
import { DIFF_SEARCH_DETAIL_PREFETCH_TARGET_MATCHES, DIFF_SEARCH_SUMMARY_FILE_MATCH_LIMIT, DIFF_SEARCH_SUMMARY_REQUEST_DEBOUNCE_MS } from "../limits";
import type { DiffFile, DiffPanelSnapshot, DiffRenderPreferences, DiffReviewSelection, DiffViewMode, WorkspaceDiffSearchResponse, WorkspaceGitFileActionKind } from "../types";
import { clampDiffSearchIndex, diffSearchVisiblePage } from "../diff-search";
import { useDiffSearchIndex } from "../diff-search-worker-client";
import {
  EMPTY_DIFF_FILE_VIRTUAL_LAYOUT,
  buildDiffFileVirtualLayout,
  mountedDiffFiles,
  stickyDiffFileItem,
} from "../diff-render-index";
import "../diffReview.css";
import { buildDiffFileTree } from "./file-tree";
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
  diffRevealRelativePath,
  diffSearchDetailPrefetchCandidateForFile,
  diffSearchDetailPrefetchCandidates,
  diffSearchIndexForSummaryTarget,
  diffSummarySearchDerived,
  diffSummarySearchRequestKey,
  diffSummarySearchTargetKey,
  isPromiseLike,
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
  // renderedSnapshot must not be narrowed by search because the diff body height must stay stable while typing.
  // renderedSnapshot 不能被搜索缩窄，因为输入搜索内容时 diff 主体高度必须保持稳定。
  const renderedSnapshot = snapshot;
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
  // isSingleFileReview marks any mode that is currently displaying one selected file body.
  // isSingleFileReview 标记任何当前正在展示单个已选文件正文的模式。
  const isSingleFileReview = Boolean(selectedDiffFile);
  // shouldShowFileTree limits the navigation drawer to multi-file diffs.
  // shouldShowFileTree 将导航目录限制在多文件 diff 场景。
  const shouldShowFileTree = renderedFileCount > 1;
  // renderedFileTree stores the review directory tree passed to the shared drawer.
  // renderedFileTree 保存传递给共享抽屉的审核目录树。
  const renderedFileTree = React.useMemo(
    () => renderedSnapshot && shouldShowFileTree ? buildDiffFileTree(renderedSnapshot.files, fileTreeFilter) : null,
    [fileTreeFilter, renderedSnapshot, shouldShowFileTree],
  );
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
    if (!props.loading) {
      setRefreshPending(false);
    }
  }, [props.loading]);

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
    <section className={`diff-review-panel${shouldShowGitFileActions ? " has-floating-actions" : ""}`}>
      <DiffPanelToolbar
        loading={refreshBusy}
        selection={selection}
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
        searchResultLabel={searchWorkerError ? "索引不可用" : searchResultLabel(searchIndex.matches.length, safeActiveSearchIndex, searchIndex.truncated)}
        searchResultTruncated={searchIndex.truncated}
        viewMode={viewMode}
      />
      <div className={`diff-review-body${selectedDiffFile ? " has-selection" : " directory-only"}`}>
        {renderedFileTree ? (
          <WorkspaceFileTreeDrawer
            activeFileKey={effectiveExpandedFileKey}
            collapseOnSelect
            emptyLabel="没有匹配的文件"
            fileIconClassName={(node) => node.file?.status ?? "modified"}
            fileKey={(file) => file.key}
            fileTitle={(file, node) => diffRevealRelativePath(file) ?? node.name}
            filter={fileTreeFilter}
            lockedOpen={!selectedDiffFile}
            pinnedOpen={fileTreeDrawerPinnedOpen}
            renderFileIcon={(node) => (
              <WorkspaceDefaultFileIcon name={node.name} warning={Boolean(node.file?.tooLargeToDisplay)} />
            )}
            renderFileTrailing={(node) => (
              <WorkspaceFileTreeStats additions={node.additions ?? 0} deletions={node.deletions ?? 0} />
            )}
            tree={renderedFileTree}
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
          {selectedDiffFile ? (
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
      {shouldShowGitFileActions && props.onGitFileAction ? (
        <DiffReviewFloatingGitActions
          busy={Boolean(props.gitFileActionPending || props.loading)}
          mode={selection.mode}
          selectedFile={selectedDiffFile}
          onAction={props.onGitFileAction}
        />
      ) : null}
    </section>
  );
}

