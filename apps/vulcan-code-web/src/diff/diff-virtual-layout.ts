import type { DiffFile, DiffLine } from "./types";
import type { CSSProperties } from "react";
import {
  DIFF_HUNK_HEADER_HEIGHT,
  DIFF_LINE_HEIGHT,
} from "./limits";
import { diffSearchLineKey } from "./diff-search";
import {
  TARGET_DIFF_VIRTUAL_METRICS,
  targetComputeApproximateBodyHeight,
  targetComputeRenderRangeFromWindow,
  targetDiffFile,
  targetDiffFileVisualRowCount,
  targetIterateOverDiff,
  type TargetDiffFile,
  type TargetDiffRenderRange,
} from "./target-diff-model";
import type { DiffViewMode } from "./types";

// DiffScrollState stores the scroll viewport used by file-level virtualizers.
// DiffScrollState 保存文件级虚拟列表使用的滚动视口。
export interface DiffScrollState {
  // Current scroll offset of the diff file list.
  // diff 文件列表当前滚动偏移。
  scrollTop: number;
  // Visible height of the diff file list.
  // diff 文件列表当前可见高度。
  clientHeight: number;
  // Monotonic revision that updates when the viewport changes.
  // 视口变化时递增的修订号。
  revision: number;
}

// DiffVirtualHunkRange stores row and pixel offsets for one hunk.
// DiffVirtualHunkRange 保存单个 hunk 的行偏移与像素偏移。
export interface DiffVirtualHunkRange {
  // Index of the source hunk.
  // 源 hunk 索引。
  hunkIndex: number;
  // First virtual row occupied by this hunk header.
  // 该 hunk 头部占用的首个虚拟行。
  startRow: number;
  // Exclusive virtual row after this hunk.
  // 该 hunk 之后的排他虚拟行。
  endRow: number;
  // Pixel top of this hunk in the file body.
  // 该 hunk 在文件正文中的像素顶部。
  top: number;
  // Pixel height of this hunk.
  // 该 hunk 的像素高度。
  height: number;
}

// DiffVirtualLayout stores compact per-hunk indexes without duplicating every line.
// DiffVirtualLayout 保存紧凑的 hunk 索引，避免复制每一行。
export interface DiffVirtualLayout {
  // Compact hunk offset ranges.
  // 紧凑的 hunk 偏移范围。
  hunkRanges: DiffVirtualHunkRange[];
  // Total virtual row count.
  // 虚拟行总数。
  totalRows: number;
  // Total pixel height.
  // 总像素高度。
  totalHeight: number;
  // Target-compatible diff file model.
  // 目标兼容 diff 文件模型。
  targetFile: TargetDiffFile;
  // View mode used to build this layout.
  // 构建该布局时使用的视图模式。
  viewMode: DiffViewMode;
}

// DiffVirtualRange stores the mounted row window and spacer heights.
// DiffVirtualRange 保存已挂载行窗口与占位高度。
interface DiffVirtualRange {
  // Top spacer height.
  // 顶部占位高度。
  paddingTop: number;
  // Bottom spacer height.
  // 底部占位高度。
  paddingBottom: number;
}

// DiffMountedVirtualRow stores a visible-only row created for the current viewport.
// DiffMountedVirtualRow 保存仅为当前视口创建的可见行。
export type DiffMountedVirtualRow =
  | {
      // Row kind for a hunk header.
      // hunk 头部行类型。
      kind: "hunk";
      // Stable row key.
      // 稳定行键。
      key: string;
      // Header text.
      // 头部文本。
      header: string;
      // Source hunk index.
      // 源 hunk 索引。
      hunkIndex: number;
      // Pixel top inside the file body.
      // 文件正文中的像素顶部。
      top: number;
      // Fixed row height.
      // 固定行高。
      height: number;
    }
  | {
      // Row kind for a normal diff line.
      // 普通 diff 行类型。
      kind: "line";
      // Stable row key.
      // 稳定行键。
      key: string;
      // Parsed diff line.
      // 已解析 diff 行。
      line: DiffLine;
      // Source hunk index.
      // 源 hunk 索引。
      hunkIndex: number;
      // Source line index inside the hunk.
      // hunk 内的源行索引。
      lineIndex: number;
      // Stable search/navigation key shared with rendered row data attributes.
      // 与已渲染行 data 属性共享的稳定搜索与导航键。
      lineKey: string;
      // Pixel top inside the file body.
      // 文件正文中的像素顶部。
      top: number;
      // Fixed row height.
      // 固定行高。
      height: number;
    }
  | {
      // Row kind for one target paired diff row.
      // 目标配对 diff 行类型。
      kind: "pair";
      // Stable row key.
      // 稳定行键。
      key: string;
      // Target event row type.
      // 目标事件行类型。
      type: "context" | "context-expanded" | "change";
      // Source hunk index.
      // 源 hunk 索引。
      hunkIndex: number;
      // Old-side parsed line when present.
      // 存在时的旧侧解析行。
      deletionLine?: DiffLine;
      // New-side parsed line when present.
      // 存在时的新侧解析行。
      additionLine?: DiffLine;
      // Old-side source hunk line index.
      // 旧侧源 hunk 行索引。
      deletionLineIndex?: number;
      // New-side source hunk line index.
      // 新侧源 hunk 行索引。
      additionLineIndex?: number;
      // Stable old-side search/navigation key.
      // 稳定旧侧搜索与导航键。
      deletionLineKey?: string;
      // Stable new-side search/navigation key.
      // 稳定新侧搜索与导航键。
      additionLineKey?: string;
      // Pixel top inside the file body.
      // 文件正文中的像素顶部。
      top: number;
      // Fixed row height.
      // 固定行高。
      height: number;
    };

// DiffMountedVirtualWindow stores the rows and spacers for the current viewport.
// DiffMountedVirtualWindow 保存当前视口的行与占位高度。
export interface DiffMountedVirtualWindow extends DiffVirtualRange {
  // Visible-only mounted rows.
  // 仅当前可见范围内挂载的行。
  rows: DiffMountedVirtualRow[];
  // Stable top spacer style for the mounted window.
  // 当前挂载窗口使用的稳定顶部占位样式。
  topSpacerStyle: CSSProperties;
  // Stable bottom spacer style for the mounted window.
  // 当前挂载窗口使用的稳定底部占位样式。
  bottomSpacerStyle: CSSProperties;
}

// DiffMountedVirtualWindowCacheEntry stores the latest mounted row window for one file layout.
// DiffMountedVirtualWindowCacheEntry 保存单个文件布局最近一次挂载的行窗口。
interface DiffMountedVirtualWindowCacheEntry {
  // First row kind in the mounted window.
  // 挂载窗口中的首个行类型。
  firstRowKind: DiffMountedVirtualBoundaryKind;
  // Source hunk index for the first row.
  // 首行对应的源 hunk 索引。
  firstRowHunkIndex: number;
  // Source line index for the first row, or -1 for hunk rows.
  // 首行对应的源行索引；hunk 行使用 -1。
  firstRowLineIndex: number;
  // Last row kind in the mounted window.
  // 挂载窗口中的最后一个行类型。
  lastRowKind: DiffMountedVirtualBoundaryKind;
  // Source hunk index for the last row.
  // 末行对应的源 hunk 索引。
  lastRowHunkIndex: number;
  // Source line index for the last row, or -1 for hunk rows.
  // 末行对应的源行索引；hunk 行使用 -1。
  lastRowLineIndex: number;
  // Mounted row count.
  // 挂载行数量。
  rowCount: number;
  // Top spacer height.
  // 顶部占位高度。
  paddingTop: number;
  // Bottom spacer height.
  // 底部占位高度。
  paddingBottom: number;
  // Stable mounted row window.
  // 稳定的挂载行窗口。
  window: DiffMountedVirtualWindow;
}

// DiffMountedVirtualBoundaryKind describes one numeric mounted-window boundary row.
// DiffMountedVirtualBoundaryKind 描述一个数值化挂载窗口边界行。
type DiffMountedVirtualBoundaryKind = "none" | "hunk" | "line";

// DiffMountedVirtualWindowDescriptor stores a cheap signature for one visible row window.
// DiffMountedVirtualWindowDescriptor 保存单个可见行窗口的轻量签名。
interface DiffMountedVirtualWindowDescriptor extends DiffVirtualRange {
  // Source hunk index where row creation should begin.
  // 行创建应开始的源 hunk 索引。
  firstHunkIndex: number;
  // Pixel top used to clip visible rows.
  // 用于裁剪可见行的像素顶部。
  startTop: number;
  // Pixel bottom used to clip visible rows.
  // 用于裁剪可见行的像素底部。
  endBottom: number;
  // First row kind in the mounted window.
  // 挂载窗口中的首个行类型。
  firstRowKind: DiffMountedVirtualBoundaryKind;
  // Source hunk index for the first row.
  // 首行对应的源 hunk 索引。
  firstRowHunkIndex: number;
  // Source line index for the first row, or -1 for hunk rows.
  // 首行对应的源行索引；hunk 行使用 -1。
  firstRowLineIndex: number;
  // Last row kind in the mounted window.
  // 挂载窗口中的最后一个行类型。
  lastRowKind: DiffMountedVirtualBoundaryKind;
  // Source hunk index for the last row.
  // 末行对应的源 hunk 索引。
  lastRowHunkIndex: number;
  // Source line index for the last row, or -1 for hunk rows.
  // 末行对应的源行索引；hunk 行使用 -1。
  lastRowLineIndex: number;
  // Mounted row count.
  // 挂载行数量。
  rowCount: number;
}

// diffFileRowCountCache stores view-specific file row counts for parsed diff objects.
// diffFileRowCountCache 为已解析 diff 对象缓存按视图区分的文件行数。
const diffFileRowCountCache = new WeakMap<DiffFile, Partial<Record<DiffViewMode, number>>>();

// diffVirtualLayoutCache stores view-specific virtual layouts for stable parsed files.
// diffVirtualLayoutCache 为稳定解析文件缓存按视图区分的虚拟布局。
const diffVirtualLayoutCache = new WeakMap<DiffFile, Partial<Record<DiffViewMode, DiffVirtualLayout>>>();

// diffMountedVirtualWindowCache stores the latest row window for each stable virtual layout.
// diffMountedVirtualWindowCache 为每个稳定虚拟布局缓存最近一次行窗口。
const diffMountedVirtualWindowCache = new WeakMap<DiffVirtualLayout, DiffMountedVirtualWindowCacheEntry>();

// EMPTY_MOUNTED_DIFF_ROWS keeps empty virtual windows allocation-free.
// EMPTY_MOUNTED_DIFF_ROWS 让空虚拟窗口保持零分配。
const EMPTY_MOUNTED_DIFF_ROWS: DiffMountedVirtualRow[] = [];

// TARGET_DIFF_OVERSCROLL_SIZE mirrors the target virtualizer default overscroll.
// TARGET_DIFF_OVERSCROLL_SIZE 对齐目标 virtualizer 的默认过扫描距离。
const TARGET_DIFF_OVERSCROLL_SIZE = 1000;

// fileTotalRowCount returns visual row count including hunk headers.
// fileTotalRowCount 返回包含 hunk 头部的可视行数量。
export function fileTotalRowCount(file: DiffFile, viewMode: DiffViewMode = "unified"): number {
  if (file.rowCount != null) {
    return viewMode === "split" ? targetDiffFileVisualRowCount(file, viewMode) : file.rowCount;
  }
  if (file.estimatedRowCount != null) {
    return file.estimatedRowCount;
  }
  const cached = diffFileRowCountCache.get(file)?.[viewMode];
  if (cached != null) {
    return cached;
  }
  const rowCount = targetDiffFileVisualRowCount(file, viewMode);
  const cachedByMode = diffFileRowCountCache.get(file) ?? {};
  cachedByMode[viewMode] = rowCount;
  diffFileRowCountCache.set(file, cachedByMode);
  return rowCount;
}

// buildDiffVirtualLayout builds compact per-hunk offsets without duplicating diff lines.
// buildDiffVirtualLayout 构建紧凑的 hunk 偏移，不复制 diff 行。
export function buildDiffVirtualLayout(file: DiffFile, viewMode: DiffViewMode = "unified"): DiffVirtualLayout {
  const cached = diffVirtualLayoutCache.get(file)?.[viewMode];
  if (cached) {
    return cached;
  }
  let nextRow = 0;
  let nextTop = 0;
  const targetFile = targetDiffFile(file);
  const hunkRanges: DiffVirtualHunkRange[] = new Array(targetFile.hunks.length);
  for (let hunkIndex = 0; hunkIndex < targetFile.hunks.length; hunkIndex += 1) {
    const targetHunk = targetFile.hunks[hunkIndex];
    const bodyRows = viewMode === "split" ? targetHunk.splitLineCount : targetHunk.unifiedLineCount;
    const rowCount = 1 + bodyRows;
    const height = DIFF_HUNK_HEADER_HEIGHT + bodyRows * DIFF_LINE_HEIGHT;
    hunkRanges[hunkIndex] = {
      hunkIndex,
      startRow: nextRow,
      endRow: nextRow + rowCount,
      top: nextTop,
      height,
    };
    nextRow += rowCount;
    nextTop += height;
  }
  const layout = {
    hunkRanges,
    totalRows: nextRow,
    totalHeight: targetComputeApproximateBodyHeight(targetFile, viewMode, TARGET_DIFF_VIRTUAL_METRICS) || nextTop,
    targetFile,
    viewMode,
  };
  const cachedByMode = diffVirtualLayoutCache.get(file) ?? {};
  cachedByMode[viewMode] = layout;
  diffVirtualLayoutCache.set(file, cachedByMode);
  return layout;
}

// mountedDiffRows creates only visible rows for the current viewport.
// mountedDiffRows 仅为当前视口创建可见行。
export function mountedDiffRows(file: DiffFile, layout: DiffVirtualLayout, viewportTop: number, viewportHeight: number): DiffMountedVirtualWindow {
  if (layout.totalHeight <= 0) {
    return emptyMountedDiffWindow(layout, 0, 0);
  }
  const effectiveViewportHeight = viewportHeight > 0 ? viewportHeight : TARGET_DIFF_OVERSCROLL_SIZE;
  const effectiveViewportTop = Number.isFinite(viewportTop) ? viewportTop : 0;
  const windowSpecs = targetVirtualWindowSpecs({
    scrollTop: effectiveViewportTop,
    scrollHeight: layout.totalHeight,
    height: effectiveViewportHeight,
    overscrollSize: TARGET_DIFF_OVERSCROLL_SIZE,
  });
  if (windowSpecs.bottom <= 0) {
    // The file is below the viewport; keep its height with a bottom spacer without mounting rows.
    // 当前文件位于视口下方；通过底部占位保留高度，但不挂载行。
    return emptyMountedDiffWindow(layout, 0, layout.totalHeight);
  }
  if (windowSpecs.top >= layout.totalHeight) {
    return emptyMountedDiffWindow(layout, layout.totalHeight, 0);
  }

  const renderRange = targetComputeRenderRangeFromWindow({
    diff: layout.targetFile,
    diffStyle: layout.viewMode,
    fileTop: 0,
    windowTop: windowSpecs.top,
    windowBottom: windowSpecs.bottom,
    totalHeight: layout.totalHeight,
    metrics: TARGET_DIFF_VIRTUAL_METRICS,
  });
  const rows = buildTargetMountedDiffRows(file, layout, renderRange);
  const descriptor = mountedDiffWindowDescriptorFromRows(layout, renderRange, rows);
  const cached = cachedMountedDiffWindow(layout, descriptor);
  if (cached) {
    return cached;
  }
  return rememberMountedDiffWindow(layout, descriptor, rows);
}

// buildTargetMountedDiffRows creates mounted rows from the target render range.
// buildTargetMountedDiffRows 根据目标渲染范围创建挂载行。
function buildTargetMountedDiffRows(file: DiffFile, layout: DiffVirtualLayout, renderRange: TargetDiffRenderRange): DiffMountedVirtualRow[] {
  if (renderRange.totalLines <= 0) {
    return EMPTY_MOUNTED_DIFF_ROWS;
  }
  const rows: DiffMountedVirtualRow[] = [];
  let cursor = renderRange.bufferBefore;
  targetIterateOverDiff({
    diff: layout.targetFile,
    diffStyle: layout.viewMode,
    startingLine: renderRange.startingLine,
    totalLines: renderRange.totalLines,
    callback: (event) => {
      if (event.collapsedBefore > 0) {
        rows.push({
          kind: "hunk",
          key: `${file.key}-target-separator-before-${event.hunkIndex}-${event.collapsedBefore}`,
          header: event.hunk?.header ?? `${event.collapsedBefore} unmodified lines`,
          hunkIndex: event.hunkIndex,
          top: cursor,
          height: TARGET_DIFF_VIRTUAL_METRICS.hunkSeparatorHeight,
        });
        cursor += TARGET_DIFF_VIRTUAL_METRICS.hunkSeparatorHeight;
      }
      const deletionLine = event.deletionLine ? layout.targetFile.deletionLines[event.deletionLine.lineIndex] : undefined;
      const additionLine = event.additionLine ? layout.targetFile.additionLines[event.additionLine.lineIndex] : undefined;
      const deletionLineIndex = event.deletionLine?.sourceLineIndex;
      const additionLineIndex = event.additionLine?.sourceLineIndex;
      const deletionLineKey = deletionLineIndex == null ? undefined : diffSearchLineKey(file.key, event.hunkIndex, deletionLineIndex);
      const additionLineKey = additionLineIndex == null ? undefined : diffSearchLineKey(file.key, event.hunkIndex, additionLineIndex);
      const noEOFCR = Boolean(event.deletionLine?.noEOFCR) || Boolean(event.additionLine?.noEOFCR);
      rows.push({
        kind: "pair",
        key: `${file.key}-target-pair-${event.hunkIndex}-${event.deletionLine?.lineIndex ?? "x"}-${event.additionLine?.lineIndex ?? "x"}-${event.type}`,
        type: event.type,
        hunkIndex: event.hunkIndex,
        deletionLine,
        additionLine,
        deletionLineIndex,
        additionLineIndex,
        deletionLineKey,
        additionLineKey,
        top: cursor,
        height: TARGET_DIFF_VIRTUAL_METRICS.lineHeight * (noEOFCR ? 2 : 1),
      });
      cursor += TARGET_DIFF_VIRTUAL_METRICS.lineHeight * (noEOFCR ? 2 : 1);
      if (event.collapsedAfter > 0) {
        const separatorHunkIndex = event.type === "context-expanded" ? event.hunkIndex : event.hunkIndex + 1;
        rows.push({
          kind: "hunk",
          key: `${file.key}-target-separator-after-${separatorHunkIndex}-${event.collapsedAfter}`,
          header: layout.targetFile.hunks[separatorHunkIndex]?.header ?? `${event.collapsedAfter} unmodified lines`,
          hunkIndex: separatorHunkIndex,
          top: cursor,
          height: TARGET_DIFF_VIRTUAL_METRICS.hunkSeparatorHeight,
        });
        cursor += TARGET_DIFF_VIRTUAL_METRICS.hunkSeparatorHeight;
      }
    },
  });
  return rows;
}

// elementTopWithinScrollContainer measures one child top in scroll-content coordinates.
// elementTopWithinScrollContainer 按滚动内容坐标测量子元素顶部。
export function elementTopWithinScrollContainer(element: HTMLElement, container: HTMLElement): number {
  const elementRect = element.getBoundingClientRect();
  const containerRect = container.getBoundingClientRect();
  return elementRect.top - containerRect.top + container.scrollTop;
}

// emptyMountedDiffWindow returns a stable empty mounted window for clipped or empty layouts.
// emptyMountedDiffWindow 为裁剪外或空布局返回稳定的空挂载窗口。
function emptyMountedDiffWindow(layout: DiffVirtualLayout, paddingTop: number, paddingBottom: number): DiffMountedVirtualWindow {
  const descriptor: DiffMountedVirtualWindowDescriptor = {
    firstHunkIndex: 0,
    startTop: 0,
    endBottom: 0,
    firstRowKind: "none",
    firstRowHunkIndex: -1,
    firstRowLineIndex: -1,
    lastRowKind: "none",
    lastRowHunkIndex: -1,
    lastRowLineIndex: -1,
    rowCount: 0,
    paddingTop,
    paddingBottom,
  };
  const cached = cachedMountedDiffWindow(layout, descriptor);
  if (cached) {
    return cached;
  }
  return rememberMountedDiffWindow(layout, descriptor, EMPTY_MOUNTED_DIFF_ROWS);
}

// mountedDiffWindowDescriptorFromRows computes the cache signature for a target render range.
// mountedDiffWindowDescriptorFromRows 计算目标渲染范围对应的缓存签名。
function mountedDiffWindowDescriptorFromRows(
  layout: DiffVirtualLayout,
  renderRange: TargetDiffRenderRange,
  rows: readonly DiffMountedVirtualRow[],
): DiffMountedVirtualWindowDescriptor {
  const firstRow = rows[0];
  const lastRow = rows.at(-1);
  return {
    firstHunkIndex: renderRange.startingLine,
    startTop: renderRange.bufferBefore,
    endBottom: Math.max(0, layout.totalHeight - renderRange.bufferAfter),
    firstRowKind: firstRow?.kind === "hunk" ? "hunk" : firstRow?.kind === "pair" || firstRow?.kind === "line" ? "line" : "none",
    firstRowHunkIndex: firstRow?.hunkIndex ?? -1,
    firstRowLineIndex: firstRow?.kind === "pair"
      ? firstRow.additionLineIndex ?? firstRow.deletionLineIndex ?? -1
      : firstRow?.kind === "line"
        ? firstRow.lineIndex
        : -1,
    lastRowKind: lastRow?.kind === "hunk" ? "hunk" : lastRow?.kind === "pair" || lastRow?.kind === "line" ? "line" : "none",
    lastRowHunkIndex: lastRow?.hunkIndex ?? -1,
    lastRowLineIndex: lastRow?.kind === "pair"
      ? lastRow.additionLineIndex ?? lastRow.deletionLineIndex ?? -1
      : lastRow?.kind === "line"
        ? lastRow.lineIndex
        : -1,
    rowCount: rows.length,
    paddingTop: renderRange.bufferBefore,
    paddingBottom: Math.max(0, renderRange.bufferAfter),
  };
}

// cachedMountedDiffWindow reuses the previous row window when the descriptor is unchanged.
// cachedMountedDiffWindow 在 descriptor 未变化时复用上一份行窗口。
function cachedMountedDiffWindow(layout: DiffVirtualLayout, descriptor: DiffMountedVirtualWindowDescriptor): DiffMountedVirtualWindow | null {
  const cached = diffMountedVirtualWindowCache.get(layout);
  if (
    cached
    && cached.rowCount === descriptor.rowCount
    && cached.firstRowKind === descriptor.firstRowKind
    && cached.firstRowHunkIndex === descriptor.firstRowHunkIndex
    && cached.firstRowLineIndex === descriptor.firstRowLineIndex
    && cached.lastRowKind === descriptor.lastRowKind
    && cached.lastRowHunkIndex === descriptor.lastRowHunkIndex
    && cached.lastRowLineIndex === descriptor.lastRowLineIndex
    && cached.paddingTop === descriptor.paddingTop
    && cached.paddingBottom === descriptor.paddingBottom
  ) {
    return cached.window;
  }
  return null;
}

// rememberMountedDiffWindow stores one newly created mounted row window.
// rememberMountedDiffWindow 保存一个新创建的挂载行窗口。
function rememberMountedDiffWindow(
  layout: DiffVirtualLayout,
  descriptor: DiffMountedVirtualWindowDescriptor,
  rows: DiffMountedVirtualRow[],
): DiffMountedVirtualWindow {
  const window = {
    rows,
    paddingTop: descriptor.paddingTop,
    paddingBottom: descriptor.paddingBottom,
    topSpacerStyle: { height: descriptor.paddingTop },
    bottomSpacerStyle: { height: descriptor.paddingBottom },
  };
  diffMountedVirtualWindowCache.set(layout, {
    firstRowKind: descriptor.firstRowKind,
    firstRowHunkIndex: descriptor.firstRowHunkIndex,
    firstRowLineIndex: descriptor.firstRowLineIndex,
    lastRowKind: descriptor.lastRowKind,
    lastRowHunkIndex: descriptor.lastRowHunkIndex,
    lastRowLineIndex: descriptor.lastRowLineIndex,
    rowCount: descriptor.rowCount,
    paddingTop: descriptor.paddingTop,
    paddingBottom: descriptor.paddingBottom,
    window,
  });
  return window;
}

// targetVirtualWindowSpecs mirrors the target Qd helper.
// targetVirtualWindowSpecs 对齐目标 Qd helper。
function targetVirtualWindowSpecs(options: {
  // Scroll top inside the container.
  // 容器内滚动顶部。
  scrollTop: number;
  // Full scrollable height.
  // 完整可滚动高度。
  scrollHeight: number;
  // Visible container height.
  // 容器可见高度。
  height: number;
  // Optional container offset.
  // 可选容器偏移。
  containerOffset?: number;
  // Whether the window should fit the viewport exactly.
  // 窗口是否应精确贴合视口。
  fitPerfectly?: boolean;
  // Overscroll size.
  // 过扫描距离。
  overscrollSize: number;
}): { top: number; bottom: number } {
  const containerOffset = options.containerOffset ?? 0;
  const overscrolledHeight = options.height + options.overscrollSize * 2;
  const visibleHeight = options.fitPerfectly ? options.height : overscrolledHeight;
  let scrollHeight = Math.max(options.scrollHeight, visibleHeight);
  if (overscrolledHeight >= scrollHeight || options.fitPerfectly) {
    const top = Math.max(options.scrollTop - containerOffset, 0);
    const bottom = Math.min(options.scrollTop + visibleHeight, scrollHeight) - containerOffset;
    return { top, bottom: Math.max(bottom, top) };
  }
  let top = options.scrollTop + options.height / 2 - overscrolledHeight / 2;
  let bottom = top + overscrolledHeight;
  if (top < 0) {
    top = 0;
  }
  if (bottom > scrollHeight) {
    bottom = scrollHeight;
  }
  top = Math.floor(Math.max(top - containerOffset, 0));
  return { top, bottom: Math.ceil(Math.max(Math.min(bottom, scrollHeight) - containerOffset, top)) };
}
