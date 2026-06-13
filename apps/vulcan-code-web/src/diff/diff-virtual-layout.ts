import type { DiffFile, DiffLine } from "./types";
import type { CSSProperties } from "react";
import {
  DIFF_HUNK_HEADER_HEIGHT,
  DIFF_LINE_HEIGHT,
  DIFF_VIRTUAL_OVERSCAN_PX,
} from "./limits";
import { diffSearchLineKey } from "./diff-search";

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

// diffFileRowCountCache stores file row counts for the lifetime of parsed diff objects.
// diffFileRowCountCache 在已解析 diff 对象生命周期内缓存文件行数。
const diffFileRowCountCache = new WeakMap<DiffFile, number>();

// diffVirtualLayoutCache stores virtual layouts for stable parsed diff file objects.
// diffVirtualLayoutCache 为稳定的已解析 diff 文件对象缓存虚拟布局。
const diffVirtualLayoutCache = new WeakMap<DiffFile, DiffVirtualLayout>();

// diffMountedVirtualWindowCache stores the latest row window for each stable virtual layout.
// diffMountedVirtualWindowCache 为每个稳定虚拟布局缓存最近一次行窗口。
const diffMountedVirtualWindowCache = new WeakMap<DiffVirtualLayout, DiffMountedVirtualWindowCacheEntry>();

// EMPTY_MOUNTED_DIFF_ROWS keeps empty virtual windows allocation-free.
// EMPTY_MOUNTED_DIFF_ROWS 让空虚拟窗口保持零分配。
const EMPTY_MOUNTED_DIFF_ROWS: DiffMountedVirtualRow[] = [];

// fileTotalRowCount returns visual row count including hunk headers.
// fileTotalRowCount 返回包含 hunk 头部的可视行数量。
export function fileTotalRowCount(file: DiffFile): number {
  if (file.rowCount != null) {
    return file.rowCount;
  }
  if (file.estimatedRowCount != null) {
    return file.estimatedRowCount;
  }
  const cached = diffFileRowCountCache.get(file);
  if (cached != null) {
    return cached;
  }
  let rowCount = 0;
  for (const hunk of file.hunks) {
    rowCount += 1 + hunk.lines.length;
  }
  diffFileRowCountCache.set(file, rowCount);
  return rowCount;
}

// buildDiffVirtualLayout builds compact per-hunk offsets without duplicating diff lines.
// buildDiffVirtualLayout 构建紧凑的 hunk 偏移，不复制 diff 行。
export function buildDiffVirtualLayout(file: DiffFile): DiffVirtualLayout {
  const cached = diffVirtualLayoutCache.get(file);
  if (cached) {
    return cached;
  }
  let nextRow = 0;
  let nextTop = 0;
  const hunkRanges: DiffVirtualHunkRange[] = new Array(file.hunks.length);
  for (let hunkIndex = 0; hunkIndex < file.hunks.length; hunkIndex += 1) {
    const hunk = file.hunks[hunkIndex];
    const rowCount = 1 + hunk.lines.length;
    const height = DIFF_HUNK_HEADER_HEIGHT + hunk.lines.length * DIFF_LINE_HEIGHT;
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
  const layout = { hunkRanges, totalRows: nextRow, totalHeight: nextTop };
  diffVirtualLayoutCache.set(file, layout);
  return layout;
}

// mountedDiffRows creates only visible rows for the current viewport.
// mountedDiffRows 仅为当前视口创建可见行。
export function mountedDiffRows(file: DiffFile, layout: DiffVirtualLayout, viewportTop: number, viewportHeight: number): DiffMountedVirtualWindow {
  if (layout.totalHeight <= 0) {
    return emptyMountedDiffWindow(layout, 0, 0);
  }
  const effectiveViewportHeight = viewportHeight > 0 ? viewportHeight : DIFF_VIRTUAL_OVERSCAN_PX;
  const effectiveViewportTop = Number.isFinite(viewportTop) ? viewportTop : 0;
  const startTop = Math.max(0, effectiveViewportTop - DIFF_VIRTUAL_OVERSCAN_PX);
  const endBottom = Math.min(layout.totalHeight, effectiveViewportTop + effectiveViewportHeight + DIFF_VIRTUAL_OVERSCAN_PX);
  if (endBottom <= 0) {
    // The file is below the viewport; keep its height with a bottom spacer without mounting rows.
    // 当前文件位于视口下方；通过底部占位保留高度，但不挂载行。
    return emptyMountedDiffWindow(layout, 0, layout.totalHeight);
  }
  if (startTop >= layout.totalHeight) {
    return emptyMountedDiffWindow(layout, layout.totalHeight, 0);
  }

  const descriptor = mountedDiffWindowDescriptor(file, layout, startTop, endBottom);
  if (!descriptor) {
    return mountedDiffRows(file, layout, 0, effectiveViewportHeight);
  }
  const cached = cachedMountedDiffWindow(layout, descriptor);
  if (cached) {
    return cached;
  }
  const rows = buildMountedDiffRows(file, layout, descriptor);
  return rememberMountedDiffWindow(layout, descriptor, rows);
}

// buildMountedDiffRows creates row objects only after the descriptor proves the window changed.
// buildMountedDiffRows 仅在 descriptor 确认窗口变化后创建行对象。
function buildMountedDiffRows(file: DiffFile, layout: DiffVirtualLayout, descriptor: DiffMountedVirtualWindowDescriptor): DiffMountedVirtualRow[] {
  const rows: DiffMountedVirtualRow[] = [];
  for (let rangeIndex = descriptor.firstHunkIndex; rangeIndex < layout.hunkRanges.length; rangeIndex += 1) {
    const range = layout.hunkRanges[rangeIndex];
    if (range.top >= descriptor.endBottom) {
      break;
    }
    const hunk = file.hunks[range.hunkIndex];
    if (!hunk) {
      continue;
    }
    appendMountedHunkRows(rows, file, hunk, range, descriptor.startTop, descriptor.endBottom);
  }
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

// mountedDiffWindowDescriptor computes the visible-window signature before row allocation.
// mountedDiffWindowDescriptor 在分配行对象前计算可见窗口签名。
function mountedDiffWindowDescriptor(
  file: DiffFile,
  layout: DiffVirtualLayout,
  startTop: number,
  endBottom: number,
): DiffMountedVirtualWindowDescriptor | null {
  let firstRowKind: DiffMountedVirtualBoundaryKind = "none";
  let firstRowHunkIndex = -1;
  let firstRowLineIndex = -1;
  let lastRowKind: DiffMountedVirtualBoundaryKind = "none";
  let lastRowHunkIndex = -1;
  let lastRowLineIndex = -1;
  let rowCount = 0;
  let firstTop = 0;
  let lastBottom = 0;
  const firstHunkIndex = firstIntersectingHunkIndex(layout.hunkRanges, startTop);
  for (let rangeIndex = firstHunkIndex; rangeIndex < layout.hunkRanges.length; rangeIndex += 1) {
    const range = layout.hunkRanges[rangeIndex];
    if (range.top >= endBottom) {
      break;
    }
    const hunk = file.hunks[range.hunkIndex];
    if (!hunk) {
      continue;
    }
    if (range.top + DIFF_HUNK_HEADER_HEIGHT >= startTop && range.top <= endBottom) {
      if (rowCount === 0) {
        firstRowKind = "hunk";
        firstRowHunkIndex = range.hunkIndex;
        firstRowLineIndex = -1;
        firstTop = range.top;
      }
      lastRowKind = "hunk";
      lastRowHunkIndex = range.hunkIndex;
      lastRowLineIndex = -1;
      lastBottom = range.top + DIFF_HUNK_HEADER_HEIGHT;
      rowCount += 1;
    }
    const linesTop = range.top + DIFF_HUNK_HEADER_HEIGHT;
    const firstLine = Math.max(0, Math.floor((startTop - linesTop) / DIFF_LINE_HEIGHT));
    const lastLine = Math.min(hunk.lines.length, Math.ceil((endBottom - linesTop) / DIFF_LINE_HEIGHT));
    if (lastLine <= firstLine) {
      continue;
    }
    if (rowCount === 0) {
      firstRowKind = "line";
      firstRowHunkIndex = range.hunkIndex;
      firstRowLineIndex = firstLine;
      firstTop = linesTop + firstLine * DIFF_LINE_HEIGHT;
    }
    lastRowKind = "line";
    lastRowHunkIndex = range.hunkIndex;
    lastRowLineIndex = lastLine - 1;
    lastBottom = linesTop + lastLine * DIFF_LINE_HEIGHT;
    rowCount += lastLine - firstLine;
  }
  if (rowCount <= 0) {
    return null;
  }
  return {
    firstHunkIndex,
    startTop,
    endBottom,
    firstRowKind,
    firstRowHunkIndex,
    firstRowLineIndex,
    lastRowKind,
    lastRowHunkIndex,
    lastRowLineIndex,
    rowCount,
    paddingTop: firstTop,
    paddingBottom: Math.max(0, layout.totalHeight - lastBottom),
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

// appendMountedHunkRows appends visible rows from one hunk range.
// appendMountedHunkRows 从单个 hunk 范围追加可见行。
function appendMountedHunkRows(
  rows: DiffMountedVirtualRow[],
  file: DiffFile,
  hunk: DiffFile["hunks"][number],
  range: DiffVirtualHunkRange,
  startTop: number,
  endBottom: number,
): void {
  if (range.top + DIFF_HUNK_HEADER_HEIGHT >= startTop && range.top <= endBottom) {
    rows.push({
      kind: "hunk",
      key: `${file.key}-virtual-hunk-${range.hunkIndex}`,
      header: hunk.header,
      hunkIndex: range.hunkIndex,
      top: range.top,
      height: DIFF_HUNK_HEADER_HEIGHT,
    });
  }
  const linesTop = range.top + DIFF_HUNK_HEADER_HEIGHT;
  const firstLine = Math.max(0, Math.floor((startTop - linesTop) / DIFF_LINE_HEIGHT));
  const lastLine = Math.min(hunk.lines.length, Math.ceil((endBottom - linesTop) / DIFF_LINE_HEIGHT));
  for (let lineIndex = firstLine; lineIndex < lastLine; lineIndex += 1) {
    const line = hunk.lines[lineIndex];
    if (!line) {
      continue;
    }
    rows.push({
      kind: "line",
      key: `${file.key}-virtual-line-${range.hunkIndex}-${lineIndex}`,
      line,
      hunkIndex: range.hunkIndex,
      lineIndex,
      lineKey: diffSearchLineKey(file.key, range.hunkIndex, lineIndex),
      top: linesTop + lineIndex * DIFF_LINE_HEIGHT,
      height: DIFF_LINE_HEIGHT,
    });
  }
}

// firstIntersectingHunkIndex finds the first hunk whose pixel range intersects the viewport.
// firstIntersectingHunkIndex 查找首个像素范围与视口相交的 hunk。
function firstIntersectingHunkIndex(hunkRanges: DiffVirtualHunkRange[], top: number): number {
  let low = 0;
  let high = hunkRanges.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    const range = hunkRanges[middle];
    if (range.top + range.height < top) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}
