import { diffSearchLineKey } from "./diff-search";
import {
  hunkSeparatorControlsHtml,
  hunkSeparatorLabelHtml,
  parsedUnmodifiedLineCount,
  type TargetHunkSeparatorState,
} from "./diff-render-hunk-controls";
import { lineContentHtml } from "./diff-render-inline-html";
import { escapeHtml, type DiffShikiTokenSpan } from "./diff-shiki-shared";
import {
  targetDiffFile,
  targetIterateOverDiff,
  type TargetDiffFile,
  type TargetDiffIterationEvent,
} from "./target-diff-model";
import type {
  DiffFile,
  DiffHunkSeparatorStyle,
  DiffIndicatorStyle,
  DiffLine,
  DiffOverflowMode,
  DiffViewMode,
} from "./types";

// DiffPlainRenderSearchState carries dynamic search and render controls.
// DiffPlainRenderSearchState 承载动态搜索状态与渲染控制参数。
export interface DiffPlainRenderSearchState {
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行 key。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行 key。
  activeSearchLineKey?: string | null;
  // Whether hunk headers should render context-expansion controls.
  // hunk 头部是否渲染上下文展开控件。
  contextControls?: boolean;
  // Whether paired changed lines should render inline change markers.
  // 配对变更行是否渲染行内变化标记。
  inlineDiff?: boolean;
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode?: DiffViewMode;
  // Horizontal overflow behavior for rendered code columns.
  // 渲染代码列的横向溢出行为。
  overflowMode?: DiffOverflowMode;
  // Hunk separator style used by the target diff renderer.
  // 目标 diff 渲染器使用的 hunk 分隔样式。
  hunkSeparatorStyle?: DiffHunkSeparatorStyle;
  // Change indicator style used by the target diff renderer.
  // 目标 diff 渲染器使用的变更航标样式。
  indicatorStyle?: DiffIndicatorStyle;
}

// DiffHighlightedLineSpans stores cached token spans for one rendered diff line.
// DiffHighlightedLineSpans 保存单条已渲染 diff 行对应的缓存 token span。
export interface DiffHighlightedLineSpans {
  // Stable line key shared with search/navigation.
  // 与搜索和导航共享的稳定行 key。
  lineKey: string;
  // Highlight token spans for this line.
  // 该行对应的高亮 token span。
  spans: DiffShikiTokenSpan[];
}

// DiffHighlightedRenderState carries token spans for highlighted HTML rendering.
// DiffHighlightedRenderState 承载高亮 HTML 渲染使用的 token span。
export interface DiffHighlightedRenderState {
  // Highlighted token spans keyed by stable line key.
  // 按稳定行 key 索引的高亮 token span。
  highlightedLineSpans?: ReadonlyMap<string, DiffShikiTokenSpan[]>;
}

// DiffRenderableVirtualRow stores only the fields needed to render one virtual row as HTML.
// DiffRenderableVirtualRow 仅保存将单个虚拟行渲染为 HTML 所需的字段。
export type DiffRenderableVirtualRow =
  | {
      // Row kind for a hunk header.
      // hunk 头部行类型。
      kind: "hunk";
      // Header text rendered for the hunk.
      // 渲染到 hunk 头部的文本。
      header: string;
      // Source hunk index used by delegated hunk actions.
      // 委托 hunk 操作使用的源 hunk 索引。
      hunkIndex: number;
      // Number of collapsed unchanged lines represented by this separator.
      // 当前分隔条代表的折叠未修改行数。
      collapsedLines?: number;
      // Whether this separator is before the first hunk.
      // 当前分隔条是否位于首个 hunk 之前。
      isFirstHunk?: boolean;
      // Whether this separator is after the last hunk.
      // 当前分隔条是否位于最后 hunk 之后。
      isLastHunk?: boolean;
      // Hunk index expanded by the up control.
      // 上箭头控件要展开的 hunk 索引。
      expandUpHunkIndex?: number;
      // Hunk index expanded by the down control.
      // 下箭头控件要展开的 hunk 索引。
      expandDownHunkIndex?: number;
    }
  | {
      // Row kind for a normal diff line.
      // 普通 diff 行类型。
      kind: "line";
      // Parsed diff line to render.
      // 需要渲染的已解析 diff 行。
      line: DiffLine;
      // Source hunk index used for stable line keys.
      // 用于稳定行 key 的源 hunk 索引。
      hunkIndex: number;
      // Source line index inside the hunk.
      // hunk 内的源行索引。
      lineIndex: number;
      // Stable search/navigation line key when the caller already knows it.
      // 调用方已知时使用的稳定搜索与导航行键。
      lineKey?: string;
    }
  | {
      // Row kind for one target paired diff row.
      // 目标配对 diff 行类型。
      kind: "pair";
      // Target event row type.
      // 目标事件行类型。
      type: "context" | "context-expanded" | "change";
      // Source hunk index used for stable line keys.
      // 用于稳定行 key 的源 hunk 索引。
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
    };

// DiffPlainFileRenderOptions stores inputs for one complete plain file render.
// DiffPlainFileRenderOptions 保存一次完整纯文本文件渲染输入。
export interface DiffPlainFileRenderOptions extends DiffPlainRenderSearchState {
  // Parsed diff file to render.
  // 需要渲染的已解析 diff 文件。
  file: DiffFile;
}

// DiffPlainRowsRenderOptions stores inputs for one virtual row-window plain render.
// DiffPlainRowsRenderOptions 保存一次虚拟行窗口纯文本渲染输入。
export interface DiffPlainRowsRenderOptions extends DiffPlainRenderSearchState {
  // Stable key of the parsed diff file that owns the rows.
  // 拥有这些行的已解析 diff 文件稳定键。
  fileKey: string;
  // Mounted virtual rows to render.
  // 需要渲染的已挂载虚拟行。
  rows: readonly DiffRenderableVirtualRow[];
}

// DiffHighlightedFileRenderOptions stores inputs for one complete highlighted file render.
// DiffHighlightedFileRenderOptions 保存一次完整高亮文件渲染输入。
export interface DiffHighlightedFileRenderOptions extends DiffPlainFileRenderOptions, DiffHighlightedRenderState {}

// DiffHighlightedRowsRenderOptions stores inputs for one highlighted virtual row-window render.
// DiffHighlightedRowsRenderOptions 保存一次高亮虚拟行窗口渲染输入。
export interface DiffHighlightedRowsRenderOptions extends DiffPlainRowsRenderOptions, DiffHighlightedRenderState {}

// TargetCodeColumnHtml stores one target-style code column's gutter and content fragments.
// TargetCodeColumnHtml 保存单个目标风格代码列的 gutter 与 content 片段。
interface TargetCodeColumnHtml {
  // Gutter HTML fragment for the rendered column.
  // 当前列的行号栏 HTML 片段。
  gutter: string;
  // Content HTML fragment for the rendered column.
  // 当前列的内容 HTML 片段。
  content: string;
}

// TargetSplitColumnsHtml stores old and new target-style split columns.
// TargetSplitColumnsHtml 保存目标风格分栏中的旧列与新列。
interface TargetSplitColumnsHtml {
  // Old/deletions column fragments.
  // 旧侧/删除侧列片段。
  deletions: TargetCodeColumnHtml;
  // New/additions column fragments.
  // 新侧/新增侧列片段。
  additions: TargetCodeColumnHtml;
}

// TargetPendingSplitBuffer stores consecutive missing-side rows before target flushes them as one buffer.
// TargetPendingSplitBuffer 保存连续缺失侧行，直到目标将其一次性刷新为一个 buffer。
interface TargetPendingSplitBuffer {
  // Missing split side that should receive the buffer.
  // 应接收 buffer 的缺失分栏侧。
  side: "deletions" | "additions" | undefined;
  // Number of consecutive rows in the pending buffer.
  // 当前待刷新 buffer 覆盖的连续行数。
  size: number;
}

// TargetHunkSeparatorState stores the same separator inputs target J receives.
// TargetHunkSeparatorState 保存目标 J 接收的同类分隔行输入。
// TargetColumnsHtml stores rendered code columns for one render pass.
// TargetColumnsHtml 保存一次渲染过程中的代码列片段。
type TargetColumnsHtml =
  | { mode: "unified"; rowCount: number; unified: TargetCodeColumnHtml }
  | {
      mode: "split";
      rowCount: number;
      visibleSide: "both" | "deletions" | "additions";
      split: TargetSplitColumnsHtml;
      pendingBuffer: TargetPendingSplitBuffer;
    };

// renderPlainDiffFileHtmlCore renders one complete file as escaped plain HTML.
// renderPlainDiffFileHtmlCore 将单个完整文件渲染为已转义纯 HTML。
export function renderPlainDiffFileHtmlCore(options: DiffPlainFileRenderOptions): string {
  return renderDiffFileHtmlCore(options);
}

// renderHighlightedDiffFileHtmlCore renders one complete file with cached token spans when available.
// renderHighlightedDiffFileHtmlCore 使用可用的缓存 token span 渲染一个完整文件。
export function renderHighlightedDiffFileHtmlCore(options: DiffHighlightedFileRenderOptions): string {
  return renderDiffFileHtmlCore(options);
}

// renderPlainDiffVirtualRowsHtmlCore renders one mounted virtual row window as escaped plain HTML.
// renderPlainDiffVirtualRowsHtmlCore 将一个已挂载虚拟行窗口渲染为已转义纯 HTML。
export function renderPlainDiffVirtualRowsHtmlCore(options: DiffPlainRowsRenderOptions): string {
  return renderDiffVirtualRowsHtmlCore(options, 0, options.rows.length);
}

// renderHighlightedDiffVirtualRowsHtmlCore renders one virtual row window with cached token spans.
// renderHighlightedDiffVirtualRowsHtmlCore 使用缓存 token span 渲染一个虚拟行窗口。
export function renderHighlightedDiffVirtualRowsHtmlCore(options: DiffHighlightedRowsRenderOptions): string {
  return renderDiffVirtualRowsHtmlCore(options, 0, options.rows.length);
}

// renderPlainDiffVirtualRowsHtmlRangeCore renders a bounded plain row range without slicing.
// renderPlainDiffVirtualRowsHtmlRangeCore 在不切片数组的情况下渲染受限纯文本行范围。
export function renderPlainDiffVirtualRowsHtmlRangeCore(
  // Render options that own the full mounted virtual row array.
  // 持有完整已挂载虚拟行数组的渲染选项。
  options: DiffPlainRowsRenderOptions,
  // Inclusive row index where the rendered range starts.
  // 渲染范围的起始行索引（包含）。
  startIndex: number,
  // Exclusive row index where the rendered range ends.
  // 渲染范围的结束行索引（不包含）。
  endIndex: number,
): string {
  return renderDiffVirtualRowsHtmlCore(options, startIndex, endIndex);
}

// renderHighlightedDiffVirtualRowsHtmlRangeCore renders a bounded highlighted row range without slicing.
// renderHighlightedDiffVirtualRowsHtmlRangeCore 在不切片数组的情况下渲染受限高亮行范围。
export function renderHighlightedDiffVirtualRowsHtmlRangeCore(
  // Render options that own the full mounted virtual row array and span cache.
  // 持有完整已挂载虚拟行数组与 span 缓存的渲染选项。
  options: DiffHighlightedRowsRenderOptions,
  // Inclusive row index where the rendered range starts.
  // 渲染范围的起始行索引（包含）。
  startIndex: number,
  // Exclusive row index where the rendered range ends.
  // 渲染范围的结束行索引（不包含）。
  endIndex: number,
): string {
  return renderDiffVirtualRowsHtmlCore(options, startIndex, endIndex);
}

// renderDiffFileHtmlCore renders one complete file through the target column model.
// renderDiffFileHtmlCore 通过目标列模型渲染一个完整文件。
function renderDiffFileHtmlCore(options: DiffPlainFileRenderOptions & DiffHighlightedRenderState): string {
  const columns = emptyTargetColumns(options.viewMode, options.file);
  const targetFile = targetDiffFile(options.file);
  appendTargetIteratedRows(columns, options.file.key, targetFile, options);
  return renderTargetColumnsHtml(columns, options);
}

// renderDiffVirtualRowsHtmlCore renders a virtual row range through the same target code shell.
// renderDiffVirtualRowsHtmlCore 通过同一目标代码外壳渲染虚拟行范围。
function renderDiffVirtualRowsHtmlCore(
  // Render options that own the full mounted virtual row array.
  // 持有完整已挂载虚拟行数组的渲染选项。
  options: DiffPlainRowsRenderOptions & DiffHighlightedRenderState,
  // Inclusive row index where rendering starts.
  // 渲染开始的行索引（包含）。
  startIndex: number,
  // Exclusive row index where rendering ends.
  // 渲染结束的行索引（不包含）。
  endIndex: number,
): string {
  const columns = emptyTargetColumns(options.viewMode);
  const safeStartIndex = Math.max(0, Math.min(startIndex, options.rows.length));
  const safeEndIndex = Math.max(safeStartIndex, Math.min(endIndex, options.rows.length));
  for (let rowIndex = safeStartIndex; rowIndex < safeEndIndex; rowIndex += 1) {
    const row = options.rows[rowIndex];
    if (row.kind === "hunk") {
      flushTargetSplitBuffer(columns);
      appendHunkSeparator(columns, row.header, row.hunkIndex, options.contextControls, true, options.hunkSeparatorStyle, {
        collapsedLines: row.collapsedLines ?? parsedUnmodifiedLineCount(row.header),
        isFirstHunk: row.isFirstHunk,
        isLastHunk: row.isLastHunk,
        isExpandable: true,
        expandUpHunkIndex: row.expandUpHunkIndex,
        expandDownHunkIndex: row.expandDownHunkIndex,
      });
      continue;
    }
    if (row.kind === "pair") {
      appendRenderablePairRow(columns, options.fileKey, row, options);
      continue;
    }
    if (columns.mode === "split") {
      appendSingleVirtualSplitLine(columns, options.fileKey, row, options);
    } else {
      appendUnifiedLine(columns.unified, row.line, {
        ...options,
        fileKey: options.fileKey,
        hunkIndex: row.hunkIndex,
        lineIndex: row.lineIndex,
        lineKey: row.lineKey ?? diffSearchLineKey(options.fileKey, row.hunkIndex, row.lineIndex),
      });
      columns.rowCount += 1;
    }
  }
  flushTargetSplitBuffer(columns);
  return renderTargetColumnsHtml(columns, options);
}

// emptyTargetColumns creates one mutable target-column accumulator for the active mode.
// emptyTargetColumns 为当前模式创建一个可变目标列累加器。
function emptyTargetColumns(viewMode: DiffViewMode | undefined, file?: DiffFile): TargetColumnsHtml {
  if (viewMode === "split") {
    return {
      mode: "split",
      rowCount: 0,
      visibleSide: file?.status === "added" ? "additions" : file?.status === "deleted" ? "deletions" : "both",
      split: {
        deletions: emptyTargetCodeColumn(),
        additions: emptyTargetCodeColumn(),
      },
      pendingBuffer: { side: undefined, size: 0 },
    };
  }
  return { mode: "unified", rowCount: 0, unified: emptyTargetCodeColumn() };
}

// emptyTargetCodeColumn creates one mutable target code-column accumulator.
// emptyTargetCodeColumn 创建一个可变目标代码列累加器。
function emptyTargetCodeColumn(): TargetCodeColumnHtml {
  return { gutter: "", content: "" };
}

// appendTargetIteratedRows appends target iterator rows in the active view mode.
// appendTargetIteratedRows 按当前视图模式追加目标迭代器行。
function appendTargetIteratedRows(
  columns: TargetColumnsHtml,
  fileKey: string,
  targetFile: TargetDiffFile,
  options: DiffPlainRenderSearchState & DiffHighlightedRenderState,
): void {
  targetIterateOverDiff({
    diff: targetFile,
    diffStyle: options.viewMode === "split" ? "split" : "unified",
    callback: (event) => {
      appendTargetIterationEvent(columns, fileKey, targetFile, event, options);
    },
  });
}

// appendTargetIterationEvent appends one target iterator event into target columns.
// appendTargetIterationEvent 将一个目标迭代事件追加到目标列。
function appendTargetIterationEvent(
  columns: TargetColumnsHtml,
  fileKey: string,
  targetFile: TargetDiffFile,
  event: TargetDiffIterationEvent,
  options: DiffPlainRenderSearchState & DiffHighlightedRenderState,
): void {
  if (event.collapsedBefore > 0) {
    flushTargetSplitBuffer(columns);
    appendHunkSeparator(columns, `${event.collapsedBefore} unmodified lines`, event.hunkIndex, options.contextControls, false, options.hunkSeparatorStyle, {
      collapsedLines: event.collapsedBefore,
      isFirstHunk: event.hunkIndex === 0,
      isLastHunk: false,
      isExpandable: true,
      expandUpHunkIndex: event.hunkIndex,
      expandDownHunkIndex: event.hunkIndex > 0 ? event.hunkIndex - 1 : undefined,
    });
  }
  if (columns.mode === "split") {
    if (event.type !== "change") {
      flushTargetSplitBuffer(columns);
    }
    appendTargetSplitIterationEvent(columns, fileKey, targetFile, event, options);
  } else {
    appendTargetUnifiedIterationEvent(columns, fileKey, targetFile, event, options);
  }
  if (event.collapsedAfter > 0) {
    const separatorHunkIndex = event.type === "context-expanded" ? event.hunkIndex : event.hunkIndex + 1;
    const previousHunkIndex = targetFile.hunks.length > 0
      ? Math.min(Math.max(event.hunkIndex, 0), targetFile.hunks.length - 1)
      : undefined;
    const nextHunkIndex = separatorHunkIndex < targetFile.hunks.length ? separatorHunkIndex : undefined;
    flushTargetSplitBuffer(columns);
    appendHunkSeparator(columns, `${event.collapsedAfter} unmodified lines`, separatorHunkIndex, options.contextControls, false, options.hunkSeparatorStyle, {
      collapsedLines: event.collapsedAfter,
      isFirstHunk: false,
      isLastHunk: nextHunkIndex == null,
      isExpandable: true,
      expandUpHunkIndex: nextHunkIndex,
      expandDownHunkIndex: previousHunkIndex,
    });
  }
}

// appendTargetUnifiedIterationEvent appends one target iterator event in unified mode.
// appendTargetUnifiedIterationEvent 以合并模式追加一个目标迭代事件。
function appendTargetUnifiedIterationEvent(
  columns: Extract<TargetColumnsHtml, { mode: "unified" }>,
  fileKey: string,
  targetFile: TargetDiffFile,
  event: TargetDiffIterationEvent,
  options: DiffPlainRenderSearchState & DiffHighlightedRenderState,
): void {
  const sideLine = event.additionLine ?? event.deletionLine;
  const line = event.additionLine
    ? targetFile.additionLines[event.additionLine.lineIndex]
    : event.deletionLine
      ? targetFile.deletionLines[event.deletionLine.lineIndex]
      : undefined;
  if (!line || !sideLine) {
    return;
  }
  const sourceLineIndex = sideLine.sourceLineIndex ?? 0;
  appendUnifiedLine(columns.unified, line, {
    ...options,
    fileKey,
    hunkIndex: event.hunkIndex,
    lineIndex: sourceLineIndex,
    lineKey: diffSearchLineKey(fileKey, event.hunkIndex, sourceLineIndex),
    lineType: event.type === "change" ? (event.additionLine ? "change-addition" : "change-deletion") : event.type,
  });
  columns.rowCount += 1;
}

// appendTargetSplitIterationEvent appends one target iterator event in split mode.
// appendTargetSplitIterationEvent 以分栏模式追加一个目标迭代事件。
function appendTargetSplitIterationEvent(
  columns: Extract<TargetColumnsHtml, { mode: "split" }>,
  fileKey: string,
  targetFile: TargetDiffFile,
  event: TargetDiffIterationEvent,
  options: DiffPlainRenderSearchState & DiffHighlightedRenderState,
): void {
  appendPairedSplitLine(columns, {
    additionLine: event.additionLine ? targetFile.additionLines[event.additionLine.lineIndex] : undefined,
    additionLineIndex: event.additionLine?.sourceLineIndex,
    deletionLine: event.deletionLine ? targetFile.deletionLines[event.deletionLine.lineIndex] : undefined,
    deletionLineIndex: event.deletionLine?.sourceLineIndex,
    fileKey,
    hunkIndex: event.hunkIndex,
    options,
    type: event.type,
  });
}

// appendRenderablePairRow appends one compact target pair row from the worker payload.
// appendRenderablePairRow 追加一条来自 worker 载荷的紧凑目标配对行。
function appendRenderablePairRow(
  columns: TargetColumnsHtml,
  fileKey: string,
  row: Extract<DiffRenderableVirtualRow, { kind: "pair" }>,
  options: DiffPlainRenderSearchState & DiffHighlightedRenderState,
): void {
  if (columns.mode === "split") {
    appendPairedSplitLine(columns, {
      additionLine: row.additionLine,
      additionLineIndex: row.additionLineIndex,
      additionLineKey: row.additionLineKey,
      deletionLine: row.deletionLine,
      deletionLineIndex: row.deletionLineIndex,
      deletionLineKey: row.deletionLineKey,
      fileKey,
      hunkIndex: row.hunkIndex,
      options,
      type: row.type,
    });
    return;
  }
  const line = row.additionLine ?? row.deletionLine;
  const lineIndex = row.additionLineIndex ?? row.deletionLineIndex ?? 0;
  const lineKey = row.additionLineKey ?? row.deletionLineKey ?? diffSearchLineKey(fileKey, row.hunkIndex, lineIndex);
  if (line) {
    appendUnifiedLine(columns.unified, line, {
      ...options,
      fileKey,
      hunkIndex: row.hunkIndex,
      lineIndex,
      lineKey,
      lineType: row.type === "change" ? (row.additionLine ? "change-addition" : "change-deletion") : row.type,
    });
    columns.rowCount += 1;
  }
}

// appendSingleVirtualSplitLine keeps legacy virtual windows renderable through target columns.
// appendSingleVirtualSplitLine 让旧虚拟窗口仍可通过目标列结构渲染。
function appendSingleVirtualSplitLine(
  columns: Extract<TargetColumnsHtml, { mode: "split" }>,
  fileKey: string,
  row: Extract<DiffRenderableVirtualRow, { kind: "line" }>,
  options: DiffPlainRenderSearchState & DiffHighlightedRenderState,
): void {
  if (row.line.kind === "removed") {
    appendPairedSplitLine(columns, {
      deletionLine: row.line,
      deletionLineIndex: row.lineIndex,
      fileKey,
      hunkIndex: row.hunkIndex,
      options,
      type: "change",
    });
    return;
  }
  if (row.line.kind === "added") {
    appendPairedSplitLine(columns, {
      additionLine: row.line,
      additionLineIndex: row.lineIndex,
      fileKey,
      hunkIndex: row.hunkIndex,
      options,
      type: "change",
    });
    return;
  }
  appendPairedSplitLine(columns, {
    additionLine: row.line,
    additionLineIndex: row.lineIndex,
    deletionLine: row.line,
    deletionLineIndex: row.lineIndex,
    fileKey,
    hunkIndex: row.hunkIndex,
    options,
    type: row.line.kind === "metadata" ? "metadata" : "context",
  });
}

// appendPairedSplitLine appends one target split row, including missing-side buffers.
// appendPairedSplitLine 追加一条目标 split 行，并包含缺失侧 buffer。
function appendPairedSplitLine(
  columns: Extract<TargetColumnsHtml, { mode: "split" }>,
  input: {
    // Optional old-side line.
    // 可选旧侧行。
    deletionLine?: DiffLine;
    // Optional new-side line.
    // 可选新侧行。
    additionLine?: DiffLine;
    // Old-side source line index.
    // 旧侧源行索引。
    deletionLineIndex?: number;
    // New-side source line index.
    // 新侧源行索引。
    additionLineIndex?: number;
    // Old-side stable line key.
    // 旧侧稳定行键。
    deletionLineKey?: string;
    // New-side stable line key.
    // 新侧稳定行键。
    additionLineKey?: string;
    // Stable file key.
    // 稳定文件 key。
    fileKey: string;
    // Source hunk index.
    // 源 hunk 索引。
    hunkIndex: number;
    // Shared render options.
    // 共享渲染选项。
    options: DiffPlainRenderSearchState & DiffHighlightedRenderState;
    // Target row type.
    // 目标行类型。
    type: "change" | "context" | "context-expanded" | "metadata";
  },
): void {
  if (input.deletionLine) {
    appendSplitSideLine(columns.split.deletions, input.deletionLine, "deletions", {
      ...input.options,
      fileKey: input.fileKey,
      hunkIndex: input.hunkIndex,
      lineIndex: input.deletionLineIndex ?? 0,
      lineKey: input.deletionLineKey ?? diffSearchLineKey(input.fileKey, input.hunkIndex, input.deletionLineIndex ?? 0),
      lineType: input.type === "change" ? "change-deletion" : input.type,
    });
  } else if (columns.visibleSide !== "additions") {
    appendTargetPendingSplitBuffer(columns, "deletions");
  }
  if (input.additionLine) {
    appendSplitSideLine(columns.split.additions, input.additionLine, "additions", {
      ...input.options,
      fileKey: input.fileKey,
      hunkIndex: input.hunkIndex,
      lineIndex: input.additionLineIndex ?? 0,
      lineKey: input.additionLineKey ?? diffSearchLineKey(input.fileKey, input.hunkIndex, input.additionLineIndex ?? 0),
      lineType: input.type === "change" ? "change-addition" : input.type,
    });
  } else if (columns.visibleSide !== "deletions") {
    appendTargetPendingSplitBuffer(columns, "additions");
  }
  columns.rowCount += 1;
}

// appendTargetPendingSplitBuffer mirrors the target pending split-buffer accumulator.
// appendTargetPendingSplitBuffer 复刻目标 pending split-buffer 累加器。
function appendTargetPendingSplitBuffer(
  columns: Extract<TargetColumnsHtml, { mode: "split" }>,
  side: "deletions" | "additions",
): void {
  if (columns.pendingBuffer.side != null && columns.pendingBuffer.side !== side) {
    flushTargetSplitBuffer(columns);
  }
  columns.pendingBuffer.side = side;
  columns.pendingBuffer.size += 1;
}

// flushTargetSplitBuffer emits one target-sized buffer for consecutive missing-side rows.
// flushTargetSplitBuffer 为连续缺失侧行输出一个目标尺寸的 buffer。
function flushTargetSplitBuffer(columns: TargetColumnsHtml): void {
  if (columns.mode !== "split") {
    return;
  }
  const { side, size } = columns.pendingBuffer;
  if (!side || size <= 0) {
    columns.pendingBuffer.side = undefined;
    columns.pendingBuffer.size = 0;
    return;
  }
  if (side === "additions") {
    appendTargetBuffer(columns.split.additions, "context", size);
  } else {
    appendTargetBuffer(columns.split.deletions, "context", size);
  }
  columns.pendingBuffer.side = undefined;
  columns.pendingBuffer.size = 0;
}

// appendHunkSeparator appends one target-style separator row to active columns.
// appendHunkSeparator 向当前列追加一条目标风格分隔行。
function appendHunkSeparator(
  columns: TargetColumnsHtml,
  header: string,
  hunkIndex: number,
  contextControls = false,
  virtual = false,
  separatorStyle: DiffHunkSeparatorStyle = "line-info",
  separatorState: TargetHunkSeparatorState = {},
): void {
  flushTargetSplitBuffer(columns);
  const expandIndex = contextControls && separatorState.isExpandable !== false ? hunkIndex : undefined;
  const separatorControlsHtml = hunkSeparatorControlsHtml(header, hunkIndex, expandIndex, separatorStyle, separatorState);
  const separatorLabelHtml = hunkSeparatorLabelHtml(header, hunkIndex, expandIndex, separatorStyle, separatorState);
  const firstLastAttributes = `${separatorState.isFirstHunk ? ` data-separator-first=""` : ""}${separatorState.isLastHunk ? ` data-separator-last=""` : ""}`;
  const expandUpAttribute = expandIndex != null && separatorState.expandUpHunkIndex != null ? ` data-expand-up-index="${separatorState.expandUpHunkIndex}"` : "";
  const expandDownAttribute = expandIndex != null && separatorState.expandDownHunkIndex != null ? ` data-expand-down-index="${separatorState.expandDownHunkIndex}"` : "";
  const rowAttributes = `data-separator="${separatorStyle}" data-diff-row-kind="hunk" data-diff-hunk-index="${hunkIndex}"${expandIndex != null ? ` data-expand-index="${hunkIndex}"` : ""}${expandUpAttribute}${expandDownAttribute}${firstLastAttributes}${virtual ? ` data-virtual-row=""` : ""}`;
  if (columns.mode === "split") {
    const splitContentSide = columns.visibleSide === "additions" ? "additions" : "deletions";
    appendTargetSeparatorCells(
      columns.split.deletions,
      hunkIndex,
      splitContentSide === "deletions" ? separatorControlsHtml : "",
      splitContentSide === "deletions" ? separatorLabelHtml : "",
      rowAttributes,
      separatorStyle,
    );
    appendTargetSeparatorCells(
      columns.split.additions,
      hunkIndex,
      splitContentSide === "additions" ? separatorControlsHtml : "",
      splitContentSide === "additions" ? separatorLabelHtml : "",
      rowAttributes,
      separatorStyle,
    );
  } else {
    appendTargetSeparatorCells(columns.unified, hunkIndex, separatorControlsHtml, separatorLabelHtml, rowAttributes, separatorStyle);
  }
  columns.rowCount += 1;
}

// appendTargetSeparator appends one separator cell pair to a target code column.
// appendTargetSeparator 向目标代码列追加一组分隔 gutter/content 单元。
function appendTargetSeparatorCells(
  column: TargetCodeColumnHtml,
  hunkIndex: number,
  gutterHtml: string,
  contentHtml: string,
  rowAttributes: string,
  separatorStyle: DiffHunkSeparatorStyle,
): void {
  appendTargetColumnCells(
    column,
    `<div data-separator="${separatorStyle}" data-line-index="hunk-${hunkIndex}" ${rowAttributes}>${gutterHtml}</div>`,
    `<div data-separator="${separatorStyle}" data-column-content="" ${rowAttributes}>${contentHtml}</div>`,
  );
}

// appendUnifiedLine appends one line to the unified target column.
// appendUnifiedLine 向目标合并列追加一行。
function appendUnifiedLine(
  column: TargetCodeColumnHtml,
  line: DiffLine,
  options: DiffPlainRenderSearchState & DiffHighlightedRenderState & {
    // Stable file key.
    // 稳定文件 key。
    fileKey: string;
    // Source hunk index.
    // 源 hunk 索引。
    hunkIndex: number;
    // Source line index inside the hunk.
    // hunk 内源行索引。
    lineIndex: number;
    // Stable line key.
    // 稳定行 key。
    lineKey: string;
    // Target line type override.
    // 目标行类型覆盖值。
    lineType?: string;
  },
): void {
  const lineType = options.lineType ?? unifiedLineType(line);
  const lineNumber = line.kind === "removed" ? line.oldLine : line.newLine;
  appendTargetColumnCells(
    column,
    targetGutterLineHtml(lineType, lineNumber, `${line.oldLine ?? ""},${line.newLine ?? ""}`, options),
    targetContentLineHtml(line, lineType, "unified", options),
  );
}

// appendSplitSideLine appends one real line to a target split side.
// appendSplitSideLine 向目标 split 单侧追加一条真实行。
function appendSplitSideLine(
  column: TargetCodeColumnHtml,
  line: DiffLine,
  side: "deletions" | "additions",
  options: DiffPlainRenderSearchState & DiffHighlightedRenderState & {
    // Stable file key.
    // 稳定文件 key。
    fileKey: string;
    // Source hunk index.
    // 源 hunk 索引。
    hunkIndex: number;
    // Source line index inside the hunk.
    // hunk 内源行索引。
    lineIndex: number;
    // Stable line key.
    // 稳定行 key。
    lineKey: string;
    // Target line type.
    // 目标行类型。
    lineType: string;
  },
): void {
  const lineNumber = side === "deletions" ? line.oldLine : line.newLine;
  appendTargetColumnCells(
    column,
    targetGutterLineHtml(options.lineType, lineNumber, `${line.oldLine ?? ""},${line.newLine ?? ""}`, options),
    targetContentLineHtml(line, options.lineType, side, options),
  );
}

// appendTargetBuffer appends one target-style missing-side buffer row.
// appendTargetBuffer 追加一条目标风格缺失侧 buffer 行。
function appendTargetBuffer(column: TargetCodeColumnHtml, lineType: string, size = 1): void {
  appendTargetColumnCells(
    column,
    `<div data-gutter-buffer="buffer" data-buffer-size="${size}" data-line-type="${lineType}" style="min-height:calc(${size} * 1lh);"></div>`,
    `<div data-content-buffer="" data-buffer-size="${size}" style="min-height:calc(${size} * 1lh)"></div>`,
    size,
  );
}

// appendTargetColumnCells appends one gutter/content pair for both scroll and wrap layouts.
// appendTargetColumnCells 同时为滚动布局和换行布局追加一组行号/内容单元。
function appendTargetColumnCells(column: TargetCodeColumnHtml, gutterHtml: string, contentHtml: string, rowSpan = 1): void {
  if (rowSpan > 1) {
    column.gutter += withTargetGridSpan(gutterHtml, rowSpan);
    column.content += withTargetGridSpan(contentHtml, rowSpan);
    return;
  }
  column.gutter += gutterHtml;
  column.content += contentHtml;
}

// withTargetGridSpan applies the logical row span used by buffer rows.
// withTargetGridSpan 为 buffer 行应用逻辑行跨度。
function withTargetGridSpan(html: string, rowSpan: number): string {
  const gridRow = `grid-row: span ${rowSpan};`;
  if (html.includes(` style="`)) {
    return html.replace(` style="`, ` style="${gridRow}`);
  }
  return html.replace("<div ", `<div style="${gridRow}" `);
}

// targetGutterLineHtml renders one target-style gutter line.
// targetGutterLineHtml 渲染一条目标风格 gutter 行。
function targetGutterLineHtml(
  lineType: string,
  lineNumber: number | null | undefined,
  lineIndex: string,
  options: { lineKey?: string; searchLineKeys?: ReadonlySet<string>; activeSearchLineKey?: string | null },
): string {
  const selected = targetSearchAttributes(options.lineKey, options.searchLineKeys, options.activeSearchLineKey);
  const lineKeyAttribute = options.lineKey ? ` data-diff-line-key="${escapeHtml(options.lineKey)}"` : "";
  const numberHtml = lineNumber == null ? "" : `<span data-line-number-content="">${lineNumber}</span>`;
  return `<div data-line-type="${lineType}" data-column-number="${lineNumber ?? ""}" data-line-index="${escapeHtml(lineIndex)}"${lineKeyAttribute}${selected}>${numberHtml}</div>`;
}

// targetContentLineHtml renders one target-style content line.
// targetContentLineHtml 渲染一条目标风格内容行。
function targetContentLineHtml(
  line: DiffLine,
  lineType: string,
  column: "unified" | "deletions" | "additions",
  options: DiffPlainRenderSearchState & DiffHighlightedRenderState & {
    // Stable line key.
    // 稳定行 key。
    lineKey: string;
  },
): string {
  const selected = targetSearchAttributes(options.lineKey, options.searchLineKeys, options.activeSearchLineKey);
  const contentHtml = lineContentHtml(line, options.lineKey, options.highlightedLineSpans, options.inlineDiff);
  return `<div data-line="" data-line-type="${lineType}" data-column-content="" data-${column}="" data-diff-line-key="${options.lineKey}"${selected}>${contentHtml}</div>`;
}

// targetSearchAttributes returns target-compatible search marker attributes.
// targetSearchAttributes 返回目标兼容的搜索标记属性。
function targetSearchAttributes(
  lineKey: string | undefined,
  searchLineKeys: ReadonlySet<string> | undefined,
  activeSearchLineKey: string | null | undefined,
): string {
  if (!lineKey) {
    return "";
  }
  const isMatch = Boolean(searchLineKeys?.has(lineKey));
  const isActive = activeSearchLineKey === lineKey;
  return `${isMatch ? ` data-search-match=""` : ""}${isActive ? ` data-search-active=""` : ""}`;
}

// renderTargetColumnsHtml renders the final target-style pre/code shell.
// renderTargetColumnsHtml 渲染最终目标风格 pre/code 外壳。
function renderTargetColumnsHtml(columns: TargetColumnsHtml, options: DiffPlainRenderSearchState): string {
  flushTargetSplitBuffer(columns);
  const rowCount = Math.max(columns.rowCount, 1);
  const overflowMode = options.overflowMode ?? "scroll";
  const indicatorStyle = options.indicatorStyle ?? "bars";
  if (columns.mode === "split") {
    if (columns.visibleSide === "deletions") {
      return `<pre data-diff="" data-diff-type="single" data-overflow="${overflowMode}" data-background="" data-indicators="${indicatorStyle}" style="--diffs-min-number-column-width-default:4ch"><code data-code="" data-container-size="" data-deletions="" style="grid-row: span ${rowCount}">${renderTargetCodeColumnInnerHtml(columns.split.deletions, rowCount, overflowMode)}</code></pre>`;
    }
    if (columns.visibleSide === "additions") {
      return `<pre data-diff="" data-diff-type="single" data-overflow="${overflowMode}" data-background="" data-indicators="${indicatorStyle}" style="--diffs-min-number-column-width-default:4ch"><code data-code="" data-container-size="" data-additions="" style="grid-row: span ${rowCount}">${renderTargetCodeColumnInnerHtml(columns.split.additions, rowCount, overflowMode)}</code></pre>`;
    }
    return `<pre data-diff="" data-diff-type="split" data-overflow="${overflowMode}" data-background="" data-indicators="${indicatorStyle}" style="--diffs-min-number-column-width-default:4ch"><code data-code="" data-container-size="" data-deletions="" style="grid-row: span ${rowCount}">${renderTargetCodeColumnInnerHtml(columns.split.deletions, rowCount, overflowMode)}</code><code data-code="" data-container-size="" data-additions="" style="grid-row: span ${rowCount}">${renderTargetCodeColumnInnerHtml(columns.split.additions, rowCount, overflowMode)}</code></pre>`;
  }
  return `<pre data-diff="" data-diff-type="single" data-overflow="${overflowMode}" data-background="" data-indicators="${indicatorStyle}" style="--diffs-min-number-column-width-default:4ch"><code data-code="" data-container-size="" data-unified="" style="grid-row: span ${rowCount}">${renderTargetCodeColumnInnerHtml(columns.unified, rowCount, overflowMode)}</code></pre>`;
}

// renderTargetCodeColumnInnerHtml renders one target code node's gutter/content grid.
// renderTargetCodeColumnInnerHtml 渲染单个目标 code 节点的 gutter/content 网格。
function renderTargetCodeColumnInnerHtml(column: TargetCodeColumnHtml, rowCount: number, _overflowMode: DiffOverflowMode): string {
  return `<div data-gutter="" style="grid-row: span ${rowCount}">${column.gutter}</div><div data-content="" style="grid-row: span ${rowCount}">${column.content}</div>`;
}

// unifiedLineType returns the target line type for unified rendering.
// unifiedLineType 返回合并渲染使用的目标行类型。
function unifiedLineType(line: DiffLine): string {
  if (line.kind === "added") {
    return "change-addition";
  }
  if (line.kind === "removed") {
    return "change-deletion";
  }
  if (line.kind === "metadata") {
    return "metadata";
  }
  return "context";
}
