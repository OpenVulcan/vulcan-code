import {
  DIFF_HUNK_HEADER_HEIGHT,
  DIFF_LINE_HEIGHT,
  DIFF_LIST_CARD_HEADER_HEIGHT,
  DIFF_LIST_CARD_VERTICAL_GAP,
  DIFF_LIST_VIRTUALIZATION_FILE_LIMIT,
  DIFF_LIST_VIRTUALIZATION_ROW_LIMIT,
  DIFF_RENDER_POLICY_NOTE_HEIGHT,
  DIFF_SUMMARY_PLACEHOLDER_MIN_HEIGHT,
  DIFF_VIRTUAL_OVERSCAN_PX,
  shouldUseLightweightDiffRendering,
} from "./limits";
import { buildDiffVirtualLayout, currentDiffRenderedBodyHeight, fileTotalRowCount } from "./diff-virtual-layout";
import { targetDiffFile } from "./target-diff-model";
import type { DiffFile, DiffLine, DiffPanelSnapshot, DiffViewMode, WorkspaceDiffSearchFileMatch } from "./types";
import type { CSSProperties } from "react";

// DiffFileVirtualItem stores one file card's estimated scroll-space range.
// DiffFileVirtualItem 保存单个文件卡片的估算滚动空间范围。
export interface DiffFileVirtualItem {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Pixel top inside the virtual file list canvas.
  // 虚拟文件列表画布中的像素顶部。
  top: number;
  // Estimated pixel height for this file card.
  // 该文件卡片的估算像素高度。
  height: number;
  // Stable absolute-position style reused while this virtual item remains mounted.
  // 当前虚拟项保持挂载期间复用的稳定绝对定位样式。
  style: CSSProperties;
  // Estimated top of the diff line host inside the virtual canvas.
  // diff 行宿主在虚拟画布内的估算顶部。
  linesTop: number;
}

// DiffFileVirtualLayout stores the file-list virtualization and lookup index.
// DiffFileVirtualLayout 保存文件列表虚拟化与查找索引。
export interface DiffFileVirtualLayout {
  // Whether file-level virtualization should be used.
  // 是否应该启用文件级虚拟化。
  enabled: boolean;
  // Estimated total pixel height for all file cards.
  // 全部文件卡片的估算像素总高度。
  totalHeight: number;
  // Stable canvas style for the virtual file list.
  // 虚拟文件列表使用的稳定画布样式。
  canvasStyle: CSSProperties;
  // Total visual rows across all file hunks.
  // 全部文件 hunk 的可视行总数。
  totalRows: number;
  // Sticky policy note height before the virtual canvas.
  // 虚拟画布前的粘性策略提示高度。
  policyNoticeHeight: number;
  // Per-file virtual ranges.
  // 每个文件的虚拟范围。
  items: DiffFileVirtualItem[];
  // View mode used to compute file and line heights.
  // 计算文件与行高度时使用的视图模式。
  viewMode: DiffViewMode;
  // Per-file virtual range lookup by stable file key.
  // 按稳定文件 key 查找文件虚拟范围。
  itemByKey: Map<string, DiffFileVirtualItem>;
}

// EMPTY_DIFF_FILE_VIRTUAL_CANVAS_STYLE keeps the empty virtual layout allocation-free.
// EMPTY_DIFF_FILE_VIRTUAL_CANVAS_STYLE 让空虚拟布局保持零分配。
const EMPTY_DIFF_FILE_VIRTUAL_CANVAS_STYLE: CSSProperties = { height: 0 };

// EMPTY_DIFF_FILE_VIRTUAL_LAYOUT keeps hook order stable before a snapshot is available.
// EMPTY_DIFF_FILE_VIRTUAL_LAYOUT 在快照可用前保持 Hook 顺序稳定。
export const EMPTY_DIFF_FILE_VIRTUAL_LAYOUT: DiffFileVirtualLayout = {
  enabled: false,
  totalHeight: 0,
  canvasStyle: EMPTY_DIFF_FILE_VIRTUAL_CANVAS_STYLE,
  totalRows: 0,
  policyNoticeHeight: 0,
  items: [],
  viewMode: "unified",
  itemByKey: new Map(),
};

// DiffMountedFileWindowCacheEntry stores the last mounted file window for one layout.
// DiffMountedFileWindowCacheEntry 保存单个布局最近一次挂载的文件窗口。
interface DiffMountedFileWindowCacheEntry {
  // Inclusive first mounted file index.
  // 挂载窗口的包含式起始文件索引。
  firstIndex: number;
  // Exclusive end mounted file index.
  // 挂载窗口的排他式结束文件索引。
  endIndex: number;
  // Stable mounted file item array for this index range.
  // 该索引范围对应的稳定挂载文件条目数组。
  mounted: DiffFileVirtualItem[];
}

// diffFileVirtualLayoutCache stores file-list layout indexes per stable snapshot object.
// diffFileVirtualLayoutCache 按稳定 snapshot 对象缓存文件列表布局索引。
const diffFileVirtualLayoutCache = new WeakMap<DiffPanelSnapshot, DiffFileVirtualLayout>();

// EMPTY_COLLAPSED_DIFF_FILE_KEYS avoids allocating a fallback collapse set.
// EMPTY_COLLAPSED_DIFF_FILE_KEYS 避免为默认折叠集合额外分配对象。
const EMPTY_COLLAPSED_DIFF_FILE_KEYS: ReadonlySet<string> = new Set();

// mountedDiffFileWindowCache stores the last mounted window per virtual layout.
// mountedDiffFileWindowCache 按虚拟布局缓存最近一次挂载窗口。
const mountedDiffFileWindowCache = new WeakMap<DiffFileVirtualLayout, DiffMountedFileWindowCacheEntry>();

// EMPTY_MOUNTED_DIFF_FILES keeps empty virtual windows reference-stable.
// EMPTY_MOUNTED_DIFF_FILES 保持空虚拟窗口引用稳定。
const EMPTY_MOUNTED_DIFF_FILES: DiffFileVirtualItem[] = [];

// buildDiffFileVirtualLayout builds the file-list virtual layout and lookup table.
// buildDiffFileVirtualLayout 构建文件列表虚拟布局与查找表。
export function buildDiffFileVirtualLayout(
  // Parsed diff snapshot whose files should be indexed.
  // 需要建立索引的已解析 diff 快照。
  snapshot: DiffPanelSnapshot,
  // User-collapsed file keys that should only reserve header height.
  // 用户折叠且只应保留文件头高度的文件 key。
  collapsedFileKeys: ReadonlySet<string> = EMPTY_COLLAPSED_DIFF_FILE_KEYS,
  // Active diff view mode.
  // 当前 diff 视图模式。
  viewMode: DiffViewMode = "unified",
): DiffFileVirtualLayout {
  const collapseAware = collapsedFileKeys.size > 0;
  const cached = collapseAware || viewMode !== "unified" ? undefined : diffFileVirtualLayoutCache.get(snapshot);
  if (!collapseAware && cached) {
    return cached;
  }
  let nextTop = 0;
  let totalRows = 0;
  const itemByKey = new Map<string, DiffFileVirtualItem>();
  const items: DiffFileVirtualItem[] = new Array(snapshot.files.length);
  for (let fileIndex = 0; fileIndex < snapshot.files.length; fileIndex += 1) {
    const file = snapshot.files[fileIndex];
    const collapsed = collapsedFileKeys.has(file.key);
    const height = estimateDiffFileCardHeight(file, collapsed, viewMode);
    const item = {
      file,
      top: nextTop,
      height,
      style: { transform: `translateY(${nextTop}px)` },
      linesTop: nextTop + diffFileLinesHostOffset(file),
    };
    nextTop += height + DIFF_LIST_CARD_VERTICAL_GAP;
    totalRows += fileTotalRowCount(file, viewMode);
    itemByKey.set(file.key, item);
    items[fileIndex] = item;
  }
  const enabled = snapshot.stats.isLarge
    || snapshot.files.length > DIFF_LIST_VIRTUALIZATION_FILE_LIMIT
    || totalRows > DIFF_LIST_VIRTUALIZATION_ROW_LIMIT;
  const layout = {
    enabled,
    totalHeight: Math.max(0, nextTop),
    canvasStyle: { height: Math.max(0, nextTop) },
    totalRows,
    policyNoticeHeight: 0,
    items,
    viewMode,
    itemByKey,
  };
  if (!collapseAware && viewMode === "unified") {
    diffFileVirtualLayoutCache.set(snapshot, layout);
  }
  return layout;
}

// mountedDiffFiles returns only file cards near the current viewport.
// mountedDiffFiles 仅返回当前视口附近的文件卡片。
export function mountedDiffFiles(layout: DiffFileVirtualLayout, scrollTop: number, clientHeight: number): DiffFileVirtualItem[] {
  if (!layout.enabled) {
    return layout.items;
  }
  const viewportHeight = clientHeight > 0 ? clientHeight : DIFF_VIRTUAL_OVERSCAN_PX;
  const virtualScrollTop = Math.max(0, scrollTop - layout.policyNoticeHeight);
  const startTop = Math.max(0, virtualScrollTop - DIFF_VIRTUAL_OVERSCAN_PX);
  const endBottom = virtualScrollTop + viewportHeight + DIFF_VIRTUAL_OVERSCAN_PX;
  const firstIndex = firstVisibleFileIndex(layout.items, startTop);
  const endIndex = firstFileStartAfterTop(layout.items, endBottom);
  const mountedCount = Math.max(0, endIndex - firstIndex);
  if (mountedCount <= 0) {
    // An empty file window should stay empty instead of pre-mounting the first card off-screen.
    // 空文件窗口应保持为空，而不是提前挂载屏幕外的首个文件卡片。
    return mountedDiffFileWindow(layout, firstIndex, endIndex);
  }
  return mountedDiffFileWindow(layout, firstIndex, endIndex);
}

// stickyDiffFileItem returns the file whose header should be pinned above the virtual canvas.
// stickyDiffFileItem 返回应在虚拟画布上方吸顶展示文件头的文件。
export function stickyDiffFileItem(
  // Current file-list virtual layout.
  // 当前文件列表虚拟布局。
  layout: DiffFileVirtualLayout,
  // Scroll top from the shared file-list container.
  // 共享文件列表容器的 scrollTop。
  scrollTop: number,
): DiffFileVirtualItem | null {
  if (!layout.enabled || !layout.items.length) {
    return null;
  }
  const virtualScrollTop = Math.max(0, scrollTop - layout.policyNoticeHeight);
  const item = layout.items[firstVisibleFileIndex(layout.items, virtualScrollTop)];
  if (!item) {
    return null;
  }
  const stickyStart = item.top + DIFF_LIST_CARD_HEADER_HEIGHT;
  const stickyEnd = item.top + item.height - DIFF_LIST_CARD_HEADER_HEIGHT;
  if (virtualScrollTop < stickyStart || virtualScrollTop >= stickyEnd) {
    return null;
  }
  return item;
}

// mountedDiffFileWindow returns a stable mounted item array while the index window is unchanged.
// mountedDiffFileWindow 在索引窗口不变时返回稳定的挂载条目数组。
function mountedDiffFileWindow(layout: DiffFileVirtualLayout, firstIndex: number, endIndex: number): DiffFileVirtualItem[] {
  const cached = mountedDiffFileWindowCache.get(layout);
  if (cached && cached.firstIndex === firstIndex && cached.endIndex === endIndex) {
    return cached.mounted;
  }
  const mountedCount = Math.max(0, endIndex - firstIndex);
  if (mountedCount <= 0) {
    mountedDiffFileWindowCache.set(layout, {
      firstIndex,
      endIndex,
      mounted: EMPTY_MOUNTED_DIFF_FILES,
    });
    return EMPTY_MOUNTED_DIFF_FILES;
  }
  const mounted: DiffFileVirtualItem[] = new Array(mountedCount);
  for (let index = 0; index < mountedCount; index += 1) {
    mounted[index] = layout.items[firstIndex + index];
  }
  mountedDiffFileWindowCache.set(layout, {
    firstIndex,
    endIndex,
    mounted,
  });
  return mounted;
}

// diffFileLinesHostOffset returns the fixed offset from file-card top to the virtual line host.
// diffFileLinesHostOffset 返回从文件卡片顶部到虚拟行宿主的固定偏移。
function diffFileLinesHostOffset(file: DiffFile): number {
  return DIFF_LIST_CARD_HEADER_HEIGHT + (shouldUseLightweightDiffRendering(file) ? DIFF_RENDER_POLICY_NOTE_HEIGHT : 0);
}

// diffScrollTopForLine estimates the scrollTop needed to reveal one diff line.
// diffScrollTopForLine 估算展示单条 diff 行所需的 scrollTop。
export function diffScrollTopForLine(
  layout: DiffFileVirtualLayout,
  fileKey: string,
  hunkIndex: number,
  lineIndex: number,
  offset = 96,
): number | null {
  const item = layout.itemByKey.get(fileKey);
  if (!item) {
    return null;
  }
  const rowTop = layout.policyNoticeHeight
    + item.top
    + diffFileLinesHostOffset(item.file)
    + diffFileLineOffset(item.file, hunkIndex, lineIndex, layout.viewMode);
  return Math.max(0, rowTop - offset);
}

// diffScrollTopForSummarySearchMatch estimates the scrollTop for a backend file-level search hit.
// diffScrollTopForSummarySearchMatch 估算后端文件级搜索命中的滚动位置。
export function diffScrollTopForSummarySearchMatch(
  layout: DiffFileVirtualLayout,
  match: WorkspaceDiffSearchFileMatch,
  offset = 96,
): number | null {
  const item = layout.itemByKey.get(match.key);
  if (!item) {
    return null;
  }
  const rowTop = layout.policyNoticeHeight
    + item.top
    + diffFileLinesHostOffset(item.file)
    + diffSummarySearchMatchOffset(item.file, match, layout.viewMode);
  return Math.max(0, rowTop - offset);
}

// estimateDiffFileCardHeight estimates the card height from fixed diff row metrics.
// estimateDiffFileCardHeight 根据固定 diff 行指标估算文件卡片高度。
function estimateDiffFileCardHeight(
  // File whose visual card height should be estimated.
  // 需要估算视觉卡片高度的文件。
  file: DiffFile,
  // Whether only the sticky file header should occupy scroll space.
  // 是否只让吸顶文件头占用滚动空间。
  collapsed: boolean,
  // Active diff view mode used by the body renderer.
  // 正文渲染器使用的当前 diff 视图模式。
  viewMode: DiffViewMode,
): number {
  if (collapsed) {
    return DIFF_LIST_CARD_HEADER_HEIGHT;
  }
  let bodyHeight = 0;
  if (file.isBinary || file.isGitlink || !file.hunks.length) {
    bodyHeight = file.isPartial && file.estimatedRowCount
      ? Math.max(DIFF_SUMMARY_PLACEHOLDER_MIN_HEIGHT, estimateDiffRowsHeight(file, file.estimatedRowCount))
      : 58;
  } else {
    bodyHeight = Math.max(
      1,
      currentDiffRenderedBodyHeight(targetDiffFile(file), viewMode),
    );
    if (shouldUseLightweightDiffRendering(file)) {
      bodyHeight += DIFF_RENDER_POLICY_NOTE_HEIGHT;
    }
  }
  return DIFF_LIST_CARD_HEADER_HEIGHT + bodyHeight;
}

// estimateDiffRowsHeight converts a visual row count into fixed-pixel diff body height.
// estimateDiffRowsHeight 将可视行数转换为固定像素 diff 正文高度。
function estimateDiffRowsHeight(
  // File metadata used for fallback hunk estimates.
  // 用于回退 hunk 估算的文件元数据。
  file: DiffFile,
  // Visual row count used for scroll-space height.
  // 用于滚动空间高度的可视行数。
  rowCount: number,
  // Optional monotonic hunk count override.
  // 可选的单调 hunk 数覆盖值。
  hunkCount?: number,
  // Active diff view mode.
  // 当前 diff 视图模式。
  viewMode: DiffViewMode = "unified",
): number {
  const hunkRows = hunkCount
    ?? (file.hunks.length
      || Math.min(rowCount, Math.max(1, file.estimatedHunkCount ?? (file.additions + file.deletions > 0 ? 1 : 0))));
  const lineRows = Math.max(0, rowCount - hunkRows);
  return hunkRows * DIFF_HUNK_HEADER_HEIGHT + lineRows * DIFF_LINE_HEIGHT;
}

// monotonicDiffFileVirtualRowCount keeps lazy detail hydration from shrinking file scroll space.
// monotonicDiffFileVirtualRowCount 防止懒加载详情水合缩小文件滚动空间。
function monotonicDiffFileVirtualRowCount(
  // File whose scroll-space row count is being estimated.
  // 正在估算滚动空间行数的文件。
  file: DiffFile,
  // Active diff view mode.
  // 当前 diff 视图模式。
  viewMode: DiffViewMode = "unified",
): number {
  return Math.max(1, fileTotalRowCount(file, viewMode), file.estimatedRowCount ?? 0);
}

// monotonicDiffFileVirtualHunkCount keeps estimated hidden hunk space when details load.
// monotonicDiffFileVirtualHunkCount 在详情加载后保留估算的隐藏 hunk 空间。
function monotonicDiffFileVirtualHunkCount(
  // File whose scroll-space hunk count is being estimated.
  // 正在估算滚动空间 hunk 数的文件。
  file: DiffFile,
  // Monotonic row count that bounds the hunk count.
  // 用于约束 hunk 数的单调行数。
  rowCount: number,
): number {
  const loadedHunkCount = file.hunks.length;
  const estimatedHunkCount = file.estimatedHunkCount ?? 0;
  return Math.max(1, Math.min(rowCount, Math.max(loadedHunkCount, estimatedHunkCount)));
}

// diffFileLineOffset estimates the pixel top of one line inside a file body.
// diffFileLineOffset 估算单条行在文件正文中的像素顶部。
function diffFileLineOffset(file: DiffFile, hunkIndex: number, lineIndex: number, viewMode: DiffViewMode = "unified"): number {
  const hunkRange = buildDiffVirtualLayout(file, viewMode).hunkRanges[hunkIndex];
  if (hunkRange) {
    return hunkRange.top + DIFF_HUNK_HEADER_HEIGHT + Math.max(0, lineIndex) * DIFF_LINE_HEIGHT;
  }
  return 0;
}

// diffSummarySearchMatchOffset estimates a backend file-level hit inside one file body.
// diffSummarySearchMatchOffset 估算后端文件级命中在单个文件正文内的位置。
function diffSummarySearchMatchOffset(file: DiffFile, match: WorkspaceDiffSearchFileMatch, viewMode: DiffViewMode): number {
  const loadedOffset = diffLoadedSummarySearchMatchOffset(file, match, viewMode);
  if (loadedOffset != null) {
    return loadedOffset;
  }
  const hunkCount = Math.max(1, file.estimatedHunkCount ?? file.hunks.length);
  const targetHunkIndex = Math.max(0, Math.min(match.firstHunkIndex ?? 0, hunkCount - 1));
  const estimatedRows = Math.max(hunkCount, file.estimatedRowCount ?? file.changedLines + hunkCount);
  const estimatedLineRows = Math.max(0, estimatedRows - hunkCount);
  const fullLineRowsBefore = Math.floor((estimatedLineRows * targetHunkIndex) / hunkCount);
  return targetHunkIndex * DIFF_HUNK_HEADER_HEIGHT + fullLineRowsBefore * DIFF_LINE_HEIGHT;
}

// diffLoadedSummarySearchMatchOffset maps a backend search hit when loaded hunk rows exist.
// diffLoadedSummarySearchMatchOffset 在已加载 hunk 行存在时映射后端搜索命中。
function diffLoadedSummarySearchMatchOffset(file: DiffFile, match: WorkspaceDiffSearchFileMatch, viewMode: DiffViewMode): number | null {
  const hunkIndex = match.firstHunkIndex ?? 0;
  const hunk = file.hunks[hunkIndex];
  if (!hunk) {
    return null;
  }
  const lineIndex = hunk.lines.findIndex((line) => diffLineMatchesSummarySearchPosition(line, match));
  return diffFileLineOffset(file, hunkIndex, lineIndex >= 0 ? lineIndex : 0, viewMode);
}

// diffLineMatchesSummarySearchPosition checks whether a loaded line matches backend metadata.
// diffLineMatchesSummarySearchPosition 检查已加载行是否匹配后端元数据。
function diffLineMatchesSummarySearchPosition(line: DiffLine, match: WorkspaceDiffSearchFileMatch): boolean {
  if (match.firstLineKind && match.firstLineKind !== "hunk" && line.kind !== match.firstLineKind) {
    return false;
  }
  if (match.firstOldLine != null && line.oldLine !== match.firstOldLine) {
    return false;
  }
  if (match.firstNewLine != null && line.newLine !== match.firstNewLine) {
    return false;
  }
  return match.firstOldLine != null || match.firstNewLine != null || Boolean(match.firstLineKind);
}

// firstVisibleFileIndex finds the first file range that can intersect the viewport.
// firstVisibleFileIndex 查找首个可能与视口相交的文件范围。
function firstVisibleFileIndex(items: DiffFileVirtualItem[], top: number): number {
  let low = 0;
  let high = items.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    const item = items[middle];
    if (item.top + item.height < top) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}

// firstFileStartAfterTop finds the first file whose top starts after the viewport bottom.
// firstFileStartAfterTop 查找首个顶部位于视口底部之后的文件。
function firstFileStartAfterTop(items: DiffFileVirtualItem[], top: number): number {
  let low = 0;
  let high = items.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    const item = items[middle];
    if (item.top <= top) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}
