import {
  DIFF_SEARCH_DETAIL_PREFETCH_CONTEXT_LINES,
  DIFF_SEARCH_DETAIL_PREFETCH_DISPATCH_DELAY_MS,
  DIFF_SEARCH_DETAIL_PREFETCH_LIMIT,
  DIFF_SEARCH_DETAIL_PREFETCH_TARGET_MATCHES,
} from "../limits";
import type { DiffFile, DiffHunkExpansionRequest, DiffPanelSnapshot, DiffReviewSelection, WorkspaceDiffSearchFileMatch } from "../types";
import { clampDiffSearchIndex, type DiffSearchMatch } from "../diff-search";
import { diffScrollTopForLine, diffScrollTopForSummarySearchMatch, type DiffFileVirtualLayout } from "../diff-render-index";
import type { DiffFileDetailLoader } from "./types";
// DiffSummarySearchState stores the compact backend file-hit search state.
// DiffSummarySearchState 保存紧凑后端文件命中搜索状态。
export interface DiffSummarySearchState {
  // Stable request key for the active diff source and query.
  // 当前 diff 来源与搜索词对应的稳定请求键。
  requestKey: string;
  // Backend-trimmed query text.
  // 后端裁剪空白后的搜索词。
  query: string;
  // File-level matches returned by the backend.
  // 后端返回的文件级命中。
  matches: readonly WorkspaceDiffSearchFileMatch[];
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
export interface DiffSummarySearchRank {
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
export interface DiffSearchDetailPrefetchCandidate {
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

// DiffSummarySearchDerived stores all first-order indexes for one backend search result array.
// DiffSummarySearchDerived 保存一组后端搜索结果的一阶派生索引。
export interface DiffSummarySearchDerived {
  // First backend match by file key.
  // 每个文件 key 对应的首个后端命中。
  matchByFileKey: Map<string, WorkspaceDiffSearchFileMatch>;
  // Ranked backend hits by file key for detail prefetch scoring.
  // 用于详情预取评分的按文件 key 排名信息。
  rankByKey: Map<string, DiffSummarySearchRank>;
}

// diffFilePathTextCache stores normalized path search text per stable diff file object.
// diffFilePathTextCache 按稳定 diff 文件对象缓存归一化路径搜索文本。
const diffFilePathTextCache = new WeakMap<DiffFile, string>();

// EMPTY_SUMMARY_SEARCH_MATCHES keeps inactive backend search matches reference-stable.
// EMPTY_SUMMARY_SEARCH_MATCHES 保持未激活后端搜索命中列表引用稳定。
export const EMPTY_SUMMARY_SEARCH_MATCHES: readonly WorkspaceDiffSearchFileMatch[] = Object.freeze([]);

// EMPTY_SUMMARY_SEARCH_DERIVED keeps inactive backend search indexes reference-stable.
// EMPTY_SUMMARY_SEARCH_DERIVED 保持未激活后端搜索派生索引引用稳定。
const EMPTY_SUMMARY_SEARCH_DERIVED: DiffSummarySearchDerived = Object.freeze({
  matchByFileKey: new Map<string, WorkspaceDiffSearchFileMatch>(),
  rankByKey: new Map<string, DiffSummarySearchRank>(),
});

// summarySearchDerivedCache stores all derived indexes per backend match array.
// summarySearchDerivedCache 按后端命中数组缓存全部派生索引。
const summarySearchDerivedCache = new WeakMap<readonly WorkspaceDiffSearchFileMatch[], DiffSummarySearchDerived>();

// EMPTY_DIFF_SUMMARY_SEARCH_STATE keeps inactive backend search state reference-stable.
// EMPTY_DIFF_SUMMARY_SEARCH_STATE 保持未激活后端搜索状态引用稳定。
export const EMPTY_DIFF_SUMMARY_SEARCH_STATE: DiffSummarySearchState = Object.freeze({
  requestKey: "",
  query: "",
  matches: [],
  pending: false,
  truncated: false,
  scannedLines: 0,
  error: null,
});

// EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS keeps inactive hydration effects reference-stable.
// EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS 保持未激活水合 effect 的请求列表引用稳定。
export const EMPTY_DIFF_DETAIL_HYDRATION_REQUESTS: readonly DiffSearchDetailPrefetchCandidate[] = Object.freeze([]);

// diffDetailHydrationRequestsWithoutDuplicates removes requests covered by an earlier queue.
// diffDetailHydrationRequestsWithoutDuplicates 移除已被较早队列覆盖的请求。
export function diffDetailHydrationRequestsWithoutDuplicates(
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
export function scheduleDiffDetailHydrationRequests(
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
export function diffReviewScrollResetKey(
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
export function diffSummarySearchRequestKey(
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
    snapshot.files.map((file) => file.key).join("\u0001"),
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
export function diffSummarySearchTargetKey(scopeKey: string, target: WorkspaceDiffSearchFileMatch): string {
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
export function diffSummarySearchDerived(matches: readonly WorkspaceDiffSearchFileMatch[]): DiffSummarySearchDerived {
  if (!matches.length) {
    return EMPTY_SUMMARY_SEARCH_DERIVED;
  }
  const cached = summarySearchDerivedCache.get(matches);
  if (cached) {
    return cached;
  }
  const matchByFileKey = new Map<string, WorkspaceDiffSearchFileMatch>();
  const rankByKey = new Map<string, DiffSummarySearchRank>();
  for (let index = 0; index < matches.length; index += 1) {
    const match = matches[index];
    if (!matchByFileKey.has(match.key)) {
      matchByFileKey.set(match.key, match);
    }
    rankByKey.set(match.key, {
      index,
      matchCount: match.matchCount,
      firstHunkIndex: match.firstHunkIndex,
      match,
    });
  }
  const derived = { matchByFileKey, rankByKey };
  summarySearchDerivedCache.set(matches, derived);
  return derived;
}

// diffSearchIndexForSummaryTarget finds the loaded-line match closest to one backend file hit.
// diffSearchIndexForSummaryTarget 查找最接近单个后端文件命中的已加载行命中。
export function diffSearchIndexForSummaryTarget(
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

// diffCollapsedFileKeys builds the default-collapsed, single-expanded file key set.
// diffCollapsedFileKeys 构建默认全折叠且仅允许单个文件展开的 key 集合。
export function diffCollapsedFileKeys(
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
export function diffAbsoluteFilePath(
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
export function diffRevealRelativePath(
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
export function shouldLoadDiffFileDetail(file: DiffFile): boolean {
  return Boolean(file.isPartial && !file.tooLargeToDisplay && !file.isBinary && !file.isGitlink && file.changedLines > 0);
}

// diffSearchDetailPrefetchCandidates selects a small batch of summary-only files to hydrate for search.
// diffSearchDetailPrefetchCandidates 为搜索选择一小批需要水合的摘要文件。
export function diffSearchDetailPrefetchCandidates(
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
export function diffSearchDetailPrefetchCandidateForFile(
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
export function diffFilePathText(file: DiffFile): string {
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
export function nextSearchIndex(index: number, matchCount: number): number {
  if (matchCount <= 0) {
    return 0;
  }
  return (clampDiffSearchIndex(index, matchCount) + 1) % matchCount;
}

// previousSearchIndex moves to the previous search result with wraparound.
// previousSearchIndex 以循环方式移动到上一条搜索结果。
export function previousSearchIndex(index: number, matchCount: number): number {
  if (matchCount <= 0) {
    return 0;
  }
  return (clampDiffSearchIndex(index, matchCount) - 1 + matchCount) % matchCount;
}

// searchResultLabel formats active result position and capped totals.
// searchResultLabel 格式化活跃结果位置和被截断的总数。
export function searchResultLabel(matchCount: number, activeIndex: number, truncated: boolean): string {
  if (matchCount <= 0) {
    return "0/0";
  }
  return `${activeIndex + 1}/${matchCount}${truncated ? "+" : ""}`;
}

// activeDiffSearchLineKeyForFile scopes the active line key to the file that owns it.
// activeDiffSearchLineKeyForFile 将当前活跃行键限制到其所属文件。
export function activeDiffSearchLineKeyForFile(
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
export function scrollToDiffSearchMatch(
  container: HTMLDivElement | null,
  layout: DiffFileVirtualLayout,
  match: DiffSearchMatch,
): boolean {
  if (!container) {
    return false;
  }
  const scrollTop = diffScrollTopForLine(layout, match.fileKey, match.hunkIndex, match.lineIndex);
  if (scrollTop == null) {
    return false;
  }
  container.scrollTop = scrollTop;
  window.requestAnimationFrame(() => {
    const lineElement = container.querySelector<HTMLElement>(`[data-diff-line-key="${match.lineKey}"]`);
    lineElement?.scrollIntoView({ block: "center" });
  });
  return true;
}

// scrollToDiffSummarySearchMatch scrolls to a backend file-level hit before detail hydration finishes.
// scrollToDiffSummarySearchMatch 在详情水合完成前滚动到后端文件级命中。
export function scrollToDiffSummarySearchMatch(
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

