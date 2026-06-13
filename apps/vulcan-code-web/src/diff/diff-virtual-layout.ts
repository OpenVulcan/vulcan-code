import type { DiffFile, DiffLine } from "./types";

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
interface DiffVirtualHunkRange {
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
}

// DIFF_LINE_HEIGHT is the fixed visual height for one diff code row.
// DIFF_LINE_HEIGHT 是单条 diff 代码行的固定视觉高度。
const DIFF_LINE_HEIGHT = 22;

// DIFF_HUNK_HEADER_HEIGHT is the fixed visual height for one hunk header row.
// DIFF_HUNK_HEADER_HEIGHT 是单条 hunk 头部行的固定视觉高度。
const DIFF_HUNK_HEADER_HEIGHT = 29;

// DIFF_VIRTUALIZATION_THRESHOLD enables row virtualization only for large files.
// DIFF_VIRTUALIZATION_THRESHOLD 仅对大文件启用行虚拟化。
export const DIFF_VIRTUALIZATION_THRESHOLD = 400;

// DIFF_VIRTUAL_OVERSCAN_PX keeps nearby rows mounted to avoid blank edges during fast scroll.
// DIFF_VIRTUAL_OVERSCAN_PX 保留视口附近行，避免快速滚动时出现空白边缘。
const DIFF_VIRTUAL_OVERSCAN_PX = 900;

// diffFileRowCountCache stores file row counts for the lifetime of parsed diff objects.
// diffFileRowCountCache 在已解析 diff 对象生命周期内缓存文件行数。
const diffFileRowCountCache = new WeakMap<DiffFile, number>();

// diffVirtualLayoutCache stores virtual layouts for stable parsed diff file objects.
// diffVirtualLayoutCache 为稳定的已解析 diff 文件对象缓存虚拟布局。
const diffVirtualLayoutCache = new WeakMap<DiffFile, DiffVirtualLayout>();

// fileTotalRowCount returns visual row count including hunk headers.
// fileTotalRowCount 返回包含 hunk 头部的可视行数量。
export function fileTotalRowCount(file: DiffFile): number {
  const cached = diffFileRowCountCache.get(file);
  if (cached != null) {
    return cached;
  }
  const rowCount = file.hunks.reduce((total, hunk) => total + 1 + hunk.lines.length, 0);
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
  const hunkRanges = file.hunks.map<DiffVirtualHunkRange>((hunk, hunkIndex) => {
    const rowCount = 1 + hunk.lines.length;
    const height = DIFF_HUNK_HEADER_HEIGHT + hunk.lines.length * DIFF_LINE_HEIGHT;
    const range = {
      hunkIndex,
      startRow: nextRow,
      endRow: nextRow + rowCount,
      top: nextTop,
      height,
    };
    nextRow += rowCount;
    nextTop += height;
    return range;
  });
  const layout = { hunkRanges, totalRows: nextRow, totalHeight: nextTop };
  diffVirtualLayoutCache.set(file, layout);
  return layout;
}

// mountedDiffRows creates only visible rows for the current viewport.
// mountedDiffRows 仅为当前视口创建可见行。
export function mountedDiffRows(file: DiffFile, layout: DiffVirtualLayout, viewportTop: number, viewportHeight: number): DiffMountedVirtualWindow {
  if (layout.totalHeight <= 0) {
    return { rows: [], paddingTop: 0, paddingBottom: 0 };
  }
  const effectiveViewportHeight = viewportHeight > 0 ? viewportHeight : DIFF_VIRTUAL_OVERSCAN_PX;
  const effectiveViewportTop = Number.isFinite(viewportTop) ? viewportTop : 0;
  const startTop = Math.max(0, effectiveViewportTop - DIFF_VIRTUAL_OVERSCAN_PX);
  const endBottom = Math.min(layout.totalHeight, effectiveViewportTop + effectiveViewportHeight + DIFF_VIRTUAL_OVERSCAN_PX);
  if (endBottom <= 0) {
    return mountedDiffRows(file, layout, 0, effectiveViewportHeight);
  }
  if (startTop >= layout.totalHeight) {
    return { rows: [], paddingTop: layout.totalHeight, paddingBottom: 0 };
  }

  const rows: DiffMountedVirtualRow[] = [];
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
    appendMountedHunkRows(rows, file, hunk, range, startTop, endBottom);
  }

  if (!rows.length) {
    return mountedDiffRows(file, layout, 0, effectiveViewportHeight);
  }
  const firstTop = rows[0].top;
  const lastRow = rows[rows.length - 1];
  const lastBottom = lastRow.top + lastRow.height;
  return {
    rows,
    paddingTop: firstTop,
    paddingBottom: Math.max(0, layout.totalHeight - lastBottom),
  };
}

// elementTopWithinScrollContainer measures one child top in scroll-content coordinates.
// elementTopWithinScrollContainer 按滚动内容坐标测量子元素顶部。
export function elementTopWithinScrollContainer(element: HTMLElement, container: HTMLElement): number {
  const elementRect = element.getBoundingClientRect();
  const containerRect = container.getBoundingClientRect();
  return elementRect.top - containerRect.top + container.scrollTop;
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
