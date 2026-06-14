import type { DiffFile, DiffLine, DiffViewMode } from "./types";

// TargetDiffContent stores one target-style hunk content block.
// TargetDiffContent 保存一个目标风格 hunk 内容块。
export type TargetDiffContent =
  | {
      // Content block kind for paired old/new changes.
      // 用于旧/新配对变更的内容块类型。
      type: "change";
      // Number of addition rows in this block.
      // 当前块中的新增行数量。
      additions: number;
      // Number of deletion rows in this block.
      // 当前块中的删除行数量。
      deletions: number;
      // Start index into the file-level addition line array.
      // 指向文件级新增行数组的起始索引。
      additionLineIndex: number;
      // Start index into the file-level deletion line array.
      // 指向文件级删除行数组的起始索引。
      deletionLineIndex: number;
      // Source hunk line index for each addition row.
      // 每条新增行对应的源 hunk 行索引。
      additionSourceLineIndexes: number[];
      // Source hunk line index for each deletion row.
      // 每条删除行对应的源 hunk 行索引。
      deletionSourceLineIndexes: number[];
    }
  | {
      // Content block kind for unchanged context rows.
      // 用于未变上下文行的内容块类型。
      type: "context";
      // Number of context rows in this block.
      // 当前块中的上下文行数量。
      lines: number;
      // Start index into the file-level addition line array.
      // 指向文件级新增行数组的起始索引。
      additionLineIndex: number;
      // Start index into the file-level deletion line array.
      // 指向文件级删除行数组的起始索引。
      deletionLineIndex: number;
      // Source hunk line index for each context row.
      // 每条上下文行对应的源 hunk 行索引。
      sourceLineIndexes: number[];
    };

// TargetDiffHunk stores one target-compatible hunk model.
// TargetDiffHunk 保存一个目标兼容的 hunk 模型。
export interface TargetDiffHunk {
  // Original hunk header text.
  // 原始 hunk 头部文本。
  header: string;
  // Number of collapsed unchanged rows before this hunk.
  // 当前 hunk 前被折叠的未变行数量。
  collapsedBefore: number;
  // First split row index occupied by this hunk body.
  // 当前 hunk 正文占用的首个 split 行索引。
  splitLineStart: number;
  // Number of split rows occupied by this hunk body.
  // 当前 hunk 正文占用的 split 行数量。
  splitLineCount: number;
  // First unified row index occupied by this hunk body.
  // 当前 hunk 正文占用的首个 unified 行索引。
  unifiedLineStart: number;
  // Number of unified rows occupied by this hunk body.
  // 当前 hunk 正文占用的 unified 行数量。
  unifiedLineCount: number;
  // Hunk old-file start line.
  // hunk 旧文件起始行。
  deletionStart: number;
  // Hunk new-file start line.
  // hunk 新文件起始行。
  additionStart: number;
  // Hunk old-file line count from the hunk header.
  // hunk 头中声明的旧文件行数。
  deletionCount: number;
  // Hunk new-file line count from the hunk header.
  // hunk 头中声明的新文件行数。
  additionCount: number;
  // Number of deletion lines materialized from the body.
  // 从正文中物化出的删除行数量。
  deletionLines: number;
  // Number of addition lines materialized from the body.
  // 从正文中物化出的新增行数量。
  additionLines: number;
  // Start index into the file-level deletion line array.
  // 指向文件级删除行数组的起始索引。
  deletionLineIndex: number;
  // Start index into the file-level addition line array.
  // 指向文件级新增行数组的起始索引。
  additionLineIndex: number;
  // Grouped hunk content blocks used by target iteration.
  // 目标迭代使用的分组 hunk 内容块。
  hunkContent: TargetDiffContent[];
  // Whether deletion side ends with no newline.
  // 删除侧是否以无换行结束。
  noEOFCRDeletions?: boolean;
  // Whether addition side ends with no newline.
  // 新增侧是否以无换行结束。
  noEOFCRAdditions?: boolean;
}

// TargetDiffFile stores target-compatible counters and line arrays for one file.
// TargetDiffFile 保存单个文件的目标兼容计数与行数组。
export interface TargetDiffFile {
  // Original parsed project diff file.
  // 原始解析后的项目 diff 文件。
  source: DiffFile;
  // File display name.
  // 文件展示名称。
  name: string;
  // Previous file display name.
  // 旧文件展示名称。
  prevName?: string;
  // Target-compatible hunk models.
  // 目标兼容 hunk 模型。
  hunks: TargetDiffHunk[];
  // File-level addition line array.
  // 文件级新增行数组。
  additionLines: DiffLine[];
  // File-level deletion line array.
  // 文件级删除行数组。
  deletionLines: DiffLine[];
  // Total split visual line count.
  // split 总可视行数。
  splitLineCount: number;
  // Total unified visual line count.
  // unified 总可视行数。
  unifiedLineCount: number;
  // Whether this file payload is partial.
  // 当前文件载荷是否为局部内容。
  isPartial: boolean;
}

// TargetDiffExpandedRegion stores how many collapsed context lines are rendered.
// TargetDiffExpandedRegion 保存折叠上下文中实际渲染的行数量。
interface TargetDiffExpandedRegion {
  // Number of lines rendered from the collapsed region start.
  // 从折叠区开头渲染的行数。
  fromStart: number;
  // Number of lines rendered from the collapsed region end.
  // 从折叠区末尾渲染的行数。
  fromEnd: number;
  // Original collapsed region size.
  // 原始折叠区大小。
  rangeSize: number;
  // Lines that remain collapsed after expansion.
  // 展开后仍保持折叠的行数。
  collapsedLines: number;
}

// TargetDiffIterationLine stores one target iterator side line reference.
// TargetDiffIterationLine 保存目标迭代器的单侧行引用。
export interface TargetDiffIterationLine {
  // Source file line number.
  // 源文件行号。
  lineNumber: number;
  // Index inside the target file-level side line array.
  // 在目标文件级单侧行数组中的索引。
  lineIndex: number;
  // Whether this line has a no-newline marker.
  // 当前行是否带无换行标记。
  noEOFCR: boolean;
  // Unified visual line index.
  // 合并视图可视行索引。
  unifiedLineIndex: number;
  // Split visual line index.
  // 分栏视图可视行索引。
  splitLineIndex: number;
  // Source line index inside the original hunk when available.
  // 可用时对应原始 hunk 内的源行索引。
  sourceLineIndex?: number;
}

// TargetDiffIterationEvent stores one callback payload from the target iterator.
// TargetDiffIterationEvent 保存目标迭代器的一次回调载荷。
export interface TargetDiffIterationEvent {
  // Event row type.
  // 事件行类型。
  type: "context" | "context-expanded" | "change";
  // Hunk index owning this row.
  // 当前行所属 hunk 索引。
  hunkIndex: number;
  // Hunk model owning this row when present.
  // 当前行所属 hunk 模型。
  hunk?: TargetDiffHunk;
  // Collapsed rows before this emitted row.
  // 当前发射行之前的折叠行数。
  collapsedBefore: number;
  // Collapsed rows after this emitted row.
  // 当前发射行之后的折叠行数。
  collapsedAfter: number;
  // Old-side line reference.
  // 旧侧行引用。
  deletionLine?: TargetDiffIterationLine;
  // New-side line reference.
  // 新侧行引用。
  additionLine?: TargetDiffIterationLine;
}

// TargetDiffRenderRange mirrors the target virtual render range object.
// TargetDiffRenderRange 对齐目标虚拟渲染范围对象。
export interface TargetDiffRenderRange {
  // First target visual line to render.
  // 需要渲染的首条目标可视行。
  startingLine: number;
  // Number of target visual lines to render.
  // 需要渲染的目标可视行数量。
  totalLines: number;
  // Pixel buffer before rendered rows.
  // 已渲染行之前的像素占位。
  bufferBefore: number;
  // Pixel buffer after rendered rows.
  // 已渲染行之后的像素占位。
  bufferAfter: number;
}

// TargetDiffVirtualMetrics mirrors the target VirtualizedFileDiff metric object.
// TargetDiffVirtualMetrics 对齐目标 VirtualizedFileDiff 的度量对象。
export interface TargetDiffVirtualMetrics {
  // Render chunk size in target visual lines.
  // 目标可视行渲染块大小。
  hunkLineCount: number;
  // Fixed line height used by the target virtualizer.
  // 目标虚拟器使用的固定行高。
  lineHeight: number;
  // File header height used by the original target component.
  // 目标原组件使用的文件头高度。
  diffHeaderHeight: number;
  // Hunk separator height.
  // hunk 分隔行高度。
  hunkSeparatorHeight: number;
  // Vertical gap between target file sections.
  // 目标文件段之间的垂直间距。
  fileGap: number;
}

// TARGET_DIFF_VIRTUAL_METRICS keeps the default values from the target bundle.
// TARGET_DIFF_VIRTUAL_METRICS 保留目标包中的默认虚拟化数值。
export const TARGET_DIFF_VIRTUAL_METRICS: TargetDiffVirtualMetrics = Object.freeze({
  hunkLineCount: 50,
  lineHeight: 20,
  diffHeaderHeight: 44,
  hunkSeparatorHeight: 32,
  fileGap: 8,
});

// TARGET_DIFF_FULL_RANGE renders the complete file through the target iterator.
// TARGET_DIFF_FULL_RANGE 通过目标迭代器渲染完整文件。
export const TARGET_DIFF_FULL_RANGE: TargetDiffRenderRange = Object.freeze({
  startingLine: 0,
  totalLines: Number.POSITIVE_INFINITY,
  bufferBefore: 0,
  bufferAfter: 0,
});

// targetDiffFileCache stores converted target models per stable parsed file object.
// targetDiffFileCache 按稳定解析文件对象缓存目标模型。
const targetDiffFileCache = new WeakMap<DiffFile, TargetDiffFile>();

// targetDiffFile returns a target-compatible model for one parsed diff file.
// targetDiffFile 返回单个已解析 diff 文件的目标兼容模型。
export function targetDiffFile(file: DiffFile): TargetDiffFile {
  const cached = targetDiffFileCache.get(file);
  if (cached) {
    return cached;
  }
  const model = buildTargetDiffFile(file);
  targetDiffFileCache.set(file, model);
  return model;
}

// targetDiffFileVisualRowCount returns the target visual row count for one view mode.
// targetDiffFileVisualRowCount 返回指定视图模式下的目标可视行数。
export function targetDiffFileVisualRowCount(file: DiffFile, viewMode: DiffViewMode): number {
  if (file.estimatedRowCount != null && file.hunks.length === 0) {
    return file.estimatedRowCount;
  }
  const model = targetDiffFile(file);
  const contentRows = viewMode === "split" ? model.splitLineCount : model.unifiedLineCount;
  return file.hunks.length + contentRows;
}

// targetIterateOverDiff walks target hunkContent with the same window counters as the target bundle.
// targetIterateOverDiff 使用与目标包一致的窗口计数遍历目标 hunkContent。
export function targetIterateOverDiff(options: {
  // Target-compatible diff file.
  // 目标兼容 diff 文件。
  diff: TargetDiffFile;
  // Active diff visual style.
  // 当前 diff 可视模式。
  diffStyle: DiffViewMode | "both";
  // First target visual line to visit.
  // 需要访问的首条目标可视行。
  startingLine?: number;
  // Maximum target visual lines to visit.
  // 需要访问的最大目标可视行数。
  totalLines?: number;
  // Expanded hunk map or true for fully expanded context.
  // 展开 hunk 映射；true 表示完全展开上下文。
  expandedHunks?: boolean | ReadonlyMap<number, { fromStart: number; fromEnd: number }>;
  // Collapsed context threshold.
  // 折叠上下文阈值。
  collapsedContextThreshold?: number;
  // Callback invoked for every emitted target row.
  // 每个发射目标行调用的回调。
  callback: (event: TargetDiffIterationEvent) => boolean | void;
}): void {
  const startingLine = options.startingLine ?? 0;
  const totalLines = options.totalLines ?? Number.POSITIVE_INFINITY;
  const diffStyle = options.diffStyle;
  const state = {
    finalHunk: options.diff.hunks.at(-1),
    viewportStart: startingLine,
    viewportEnd: startingLine + totalLines,
    isWindowedHighlight: startingLine > 0 || totalLines < Number.POSITIVE_INFINITY,
    splitCount: 0,
    unifiedCount: 0,
    shouldBreak() {
      if (!state.isWindowedHighlight) {
        return false;
      }
      const unifiedDone = state.unifiedCount >= startingLine + totalLines;
      const splitDone = state.splitCount >= startingLine + totalLines;
      return diffStyle === "unified" ? unifiedDone : (diffStyle === "split" || unifiedDone) && splitDone;
    },
    shouldSkip(unifiedLines: number, splitLines: number) {
      if (!state.isWindowedHighlight) {
        return false;
      }
      const unifiedBefore = state.unifiedCount + unifiedLines < startingLine;
      const splitBefore = state.splitCount + splitLines < startingLine;
      return diffStyle === "unified" ? unifiedBefore : (diffStyle === "split" || unifiedBefore) && splitBefore;
    },
    incrementCounts(unifiedLines: number, splitLines: number) {
      if (diffStyle === "unified" || diffStyle === "both") {
        state.unifiedCount += unifiedLines;
      }
      if (diffStyle === "split" || diffStyle === "both") {
        state.splitCount += splitLines;
      }
    },
    isInWindow(unifiedLinesBefore: number, splitLinesBefore: number) {
      if (!state.isWindowedHighlight) {
        return true;
      }
      const inUnified = state.isInUnifiedWindow(unifiedLinesBefore);
      const inSplit = state.isInSplitWindow(splitLinesBefore);
      return diffStyle === "unified" ? inUnified : diffStyle === "split" ? inSplit : inUnified || inSplit;
    },
    isInUnifiedWindow(unifiedLinesBefore: number) {
      return !state.isWindowedHighlight || (state.unifiedCount >= startingLine - unifiedLinesBefore && state.unifiedCount < startingLine + totalLines);
    },
    isInSplitWindow(splitLinesBefore: number) {
      return !state.isWindowedHighlight || (state.splitCount >= startingLine - splitLinesBefore && state.splitCount < startingLine + totalLines);
    },
    emit(event: TargetDiffIterationEvent, skipIncrement = false) {
      if (!skipIncrement) {
        if (diffStyle === "unified") {
          state.incrementCounts(1, 0);
        } else if (diffStyle === "split") {
          state.incrementCounts(0, 1);
        } else {
          state.incrementCounts(1, 1);
        }
      }
      return options.callback(event) ?? false;
    },
  };

  hunkIterator: for (const [hunkIndex, hunk] of options.diff.hunks.entries()) {
    if (state.shouldBreak()) {
      break;
    }
    const expandedRegion = targetExpandedRegion(
      options.diff.isPartial,
      hunk.collapsedBefore,
      options.expandedHunks,
      hunkIndex,
      options.collapsedContextThreshold ?? 1,
    );
    const trailingRegion = (() => {
      if (hunk !== state.finalHunk || !targetHasTrailingContext(options.diff)) {
        return undefined;
      }
      const additions = options.diff.additionLines.length - (hunk.additionLineIndex + hunk.additionCount);
      const deletions = options.diff.deletionLines.length - (hunk.deletionLineIndex + hunk.deletionCount);
      if (additions !== deletions) {
        throw Error(`iterateOverDiff: trailing context mismatch (additions=${additions}, deletions=${deletions}) for ${options.diff.name}`);
      }
      return targetExpandedRegion(
        options.diff.isPartial,
        Math.min(additions, deletions),
        options.expandedHunks,
        options.diff.hunks.length,
        options.collapsedContextThreshold ?? 1,
      );
    })();
    const expandedRegionVisibleLines = expandedRegion.fromStart + expandedRegion.fromEnd;
    function trailingCollapsedAfter(unifiedLineIndex: number, splitLineIndex: number): number {
      if (trailingRegion == null || trailingRegion.collapsedLines <= 0 || trailingRegion.fromStart + trailingRegion.fromEnd > 0) {
        return 0;
      }
      return diffStyle === "unified"
        ? unifiedLineIndex === hunk.unifiedLineStart + hunk.unifiedLineCount - 1
          ? trailingRegion.collapsedLines
          : 0
        : splitLineIndex === hunk.splitLineStart + hunk.splitLineCount - 1
          ? trailingRegion.collapsedLines
          : 0;
    }
    function consumeCollapsedBefore(): number {
      if (expandedRegion.collapsedLines === 0) {
        return 0;
      }
      const collapsedLines = expandedRegion.collapsedLines;
      expandedRegion.collapsedLines = 0;
      return collapsedLines;
    }
    if (state.shouldSkip(expandedRegionVisibleLines, expandedRegionVisibleLines)) {
      state.incrementCounts(expandedRegionVisibleLines, expandedRegionVisibleLines);
      consumeCollapsedBefore();
    } else {
      let unifiedLineIndex = hunk.unifiedLineStart - expandedRegion.rangeSize;
      let splitLineIndex = hunk.splitLineStart - expandedRegion.rangeSize;
      let deletionLineIndex = hunk.deletionLineIndex - expandedRegion.rangeSize;
      let additionLineIndex = hunk.additionLineIndex - expandedRegion.rangeSize;
      let deletionLineNumber = hunk.deletionStart - expandedRegion.rangeSize;
      let additionLineNumber = hunk.additionStart - expandedRegion.rangeSize;
      let offset = 0;
      for (; offset < expandedRegion.fromStart;) {
        if (state.isInWindow(0, 0)) {
          if (state.emit({
            hunkIndex,
            hunk,
            collapsedBefore: 0,
            collapsedAfter: 0,
            type: "context-expanded",
            deletionLine: {
              lineNumber: deletionLineNumber + offset,
              lineIndex: deletionLineIndex + offset,
              noEOFCR: false,
              unifiedLineIndex: unifiedLineIndex + offset,
              splitLineIndex: splitLineIndex + offset,
            },
            additionLine: {
              unifiedLineIndex: unifiedLineIndex + offset,
              splitLineIndex: splitLineIndex + offset,
              lineIndex: additionLineIndex + offset,
              lineNumber: additionLineNumber + offset,
              noEOFCR: false,
            },
          })) {
            break hunkIterator;
          }
        } else {
          state.incrementCounts(1, 1);
        }
        offset += 1;
      }
      unifiedLineIndex = hunk.unifiedLineStart - expandedRegion.fromEnd;
      splitLineIndex = hunk.splitLineStart - expandedRegion.fromEnd;
      deletionLineIndex = hunk.deletionLineIndex - expandedRegion.fromEnd;
      additionLineIndex = hunk.additionLineIndex - expandedRegion.fromEnd;
      deletionLineNumber = hunk.deletionStart - expandedRegion.fromEnd;
      additionLineNumber = hunk.additionStart - expandedRegion.fromEnd;
      offset = 0;
      for (; offset < expandedRegion.fromEnd;) {
        if (state.isInWindow(0, 0)) {
          if (state.emit({
            hunkIndex,
            hunk,
            collapsedBefore: consumeCollapsedBefore(),
            collapsedAfter: 0,
            type: "context-expanded",
            deletionLine: {
              lineNumber: deletionLineNumber + offset,
              lineIndex: deletionLineIndex + offset,
              noEOFCR: false,
              unifiedLineIndex: unifiedLineIndex + offset,
              splitLineIndex: splitLineIndex + offset,
            },
            additionLine: {
              unifiedLineIndex: unifiedLineIndex + offset,
              splitLineIndex: splitLineIndex + offset,
              lineIndex: additionLineIndex + offset,
              lineNumber: additionLineNumber + offset,
              noEOFCR: false,
            },
          })) {
            break hunkIterator;
          }
        } else {
          state.incrementCounts(1, 1);
        }
        offset += 1;
      }
    }

    let unifiedLineIndex = hunk.unifiedLineStart;
    let splitLineIndex = hunk.splitLineStart;
    let deletionLineIndex = hunk.deletionLineIndex;
    let additionLineIndex = hunk.additionLineIndex;
    let deletionLineNumber = hunk.deletionStart;
    let additionLineNumber = hunk.additionStart;
    const finalContent = hunk.hunkContent.at(-1);
    for (const content of hunk.hunkContent) {
      if (state.shouldBreak()) {
        break hunkIterator;
      }
      const isLastContent = content === finalContent;
      if (content.type === "context") {
        if (state.shouldSkip(content.lines, content.lines)) {
          state.incrementCounts(content.lines, content.lines);
          consumeCollapsedBefore();
        } else {
          let contentOffset = 0;
          for (; contentOffset < content.lines;) {
            if (state.isInWindow(0, 0)) {
              const isLastLine = isLastContent && contentOffset === content.lines - 1;
              const currentUnifiedLineIndex = unifiedLineIndex + contentOffset;
              const currentSplitLineIndex = splitLineIndex + contentOffset;
              if (state.emit({
                hunkIndex,
                hunk,
                collapsedBefore: consumeCollapsedBefore(),
                collapsedAfter: trailingCollapsedAfter(currentUnifiedLineIndex, currentSplitLineIndex),
                type: "context",
                deletionLine: {
                  lineNumber: deletionLineNumber + contentOffset,
                  lineIndex: deletionLineIndex + contentOffset,
                noEOFCR: isLastLine && Boolean(hunk.noEOFCRDeletions),
                unifiedLineIndex: currentUnifiedLineIndex,
                splitLineIndex: currentSplitLineIndex,
                sourceLineIndex: content.sourceLineIndexes[contentOffset],
              },
              additionLine: {
                unifiedLineIndex: currentUnifiedLineIndex,
                splitLineIndex: currentSplitLineIndex,
                lineIndex: additionLineIndex + contentOffset,
                lineNumber: additionLineNumber + contentOffset,
                noEOFCR: isLastLine && Boolean(hunk.noEOFCRAdditions),
                sourceLineIndex: content.sourceLineIndexes[contentOffset],
              },
              })) {
                break hunkIterator;
              }
            } else {
              state.incrementCounts(1, 1);
            }
            contentOffset += 1;
          }
        }
        unifiedLineIndex += content.lines;
        splitLineIndex += content.lines;
        deletionLineIndex += content.lines;
        additionLineIndex += content.lines;
        deletionLineNumber += content.lines;
        additionLineNumber += content.lines;
      } else {
        const splitCount = Math.max(content.deletions, content.additions);
        const unifiedCount = content.deletions + content.additions;
        if (!state.shouldSkip(unifiedCount, splitCount)) {
          const ranges = targetChangedContentRanges(state, content, diffStyle);
          for (const [from, to] of ranges) {
            for (let index = from; index < to; index += 1) {
              const collapsedAfter = trailingCollapsedAfter(
                unifiedLineIndex + index,
                diffStyle === "unified" ? splitLineIndex + (index < content.deletions ? index : index - content.deletions) : splitLineIndex + index,
              );
              if (state.emit(targetChangedLineEvent({
                hunkIndex,
                hunk,
                collapsedBefore: consumeCollapsedBefore(),
                collapsedAfter,
                diffStyle,
                index,
                unifiedLineIndex,
                splitLineIndex,
                additionLineIndex,
                deletionLineIndex,
                additionLineNumber,
                deletionLineNumber,
                content,
                isLastContent,
                unifiedCount,
                splitCount,
              }), true)) {
                break hunkIterator;
              }
            }
          }
        }
        consumeCollapsedBefore();
        state.incrementCounts(unifiedCount, splitCount);
        unifiedLineIndex += unifiedCount;
        splitLineIndex += splitCount;
        deletionLineIndex += content.deletions;
        additionLineIndex += content.additions;
        deletionLineNumber += content.deletions;
        additionLineNumber += content.additions;
      }
    }

    if (trailingRegion != null) {
      const visibleTrailingLines = trailingRegion.fromStart + trailingRegion.fromEnd;
      let trailingOffset = 0;
      for (; trailingOffset < visibleTrailingLines;) {
        if (state.shouldBreak()) {
          break hunkIterator;
        }
        if (state.isInWindow(0, 0)) {
          const isLastTrailingLine = trailingOffset === visibleTrailingLines - 1;
          if (state.emit({
            hunkIndex: options.diff.hunks.length,
            hunk: undefined,
            collapsedBefore: 0,
            collapsedAfter: isLastTrailingLine ? trailingRegion.collapsedLines : 0,
            type: "context-expanded",
            deletionLine: {
              lineNumber: deletionLineNumber + trailingOffset,
              lineIndex: deletionLineIndex + trailingOffset,
              noEOFCR: false,
              unifiedLineIndex: unifiedLineIndex + trailingOffset,
              splitLineIndex: splitLineIndex + trailingOffset,
            },
            additionLine: {
              unifiedLineIndex: unifiedLineIndex + trailingOffset,
              splitLineIndex: splitLineIndex + trailingOffset,
              lineIndex: additionLineIndex + trailingOffset,
              lineNumber: additionLineNumber + trailingOffset,
              noEOFCR: false,
            },
          })) {
            break hunkIterator;
          }
        } else {
          state.incrementCounts(1, 1);
        }
        trailingOffset += 1;
      }
    }
  }
}

// targetExpandedLineCount mirrors VirtualizedFileDiff.getExpandedLineCount.
// targetExpandedLineCount 对齐 VirtualizedFileDiff.getExpandedLineCount。
export function targetExpandedLineCount(
  diff: TargetDiffFile,
  diffStyle: DiffViewMode,
  expandedHunks?: boolean | ReadonlyMap<number, { fromStart: number; fromEnd: number }>,
  collapsedContextThreshold = 1,
): number {
  let lineCount = 0;
  if (diff.isPartial) {
    for (const hunk of diff.hunks) {
      lineCount += diffStyle === "split" ? hunk.splitLineCount : hunk.unifiedLineCount;
    }
    return lineCount;
  }
  for (const [hunkIndex, hunk] of diff.hunks.entries()) {
    lineCount += diffStyle === "split" ? hunk.splitLineCount : hunk.unifiedLineCount;
    const collapsedBefore = Math.max(hunk.collapsedBefore, 0);
    const region = targetExpandedRegion(diff.isPartial, collapsedBefore, expandedHunks, hunkIndex, collapsedContextThreshold);
    if (collapsedBefore > 0) {
      lineCount += region.collapsedLines === 0 ? collapsedBefore : region.fromStart + region.fromEnd;
    }
  }
  const finalHunk = diff.hunks.at(-1);
  if (finalHunk != null && targetHasTrailingContext(diff)) {
    const additions = diff.additionLines.length - (finalHunk.additionLineIndex + finalHunk.additionCount);
    const deletions = diff.deletionLines.length - (finalHunk.deletionLineIndex + finalHunk.deletionCount);
    if (additions !== deletions) {
      throw Error(`VirtualizedFileDiff: trailing context mismatch (additions=${additions}, deletions=${deletions}) for ${diff.name}`);
    }
    const trailingLines = Math.min(additions, deletions);
    if (trailingLines > 0) {
      const region = targetExpandedRegion(diff.isPartial, trailingLines, expandedHunks, diff.hunks.length, collapsedContextThreshold);
      lineCount += region.collapsedLines === 0 ? trailingLines : region.fromStart;
    }
  }
  return lineCount;
}

// targetComputeApproximateBodyHeight mirrors the target approximate size loop for file bodies.
// targetComputeApproximateBodyHeight 对齐目标文件正文近似高度计算循环。
export function targetComputeApproximateBodyHeight(
  diff: TargetDiffFile,
  diffStyle: DiffViewMode,
  metrics: TargetDiffVirtualMetrics = TARGET_DIFF_VIRTUAL_METRICS,
): number {
  let height = 0;
  targetIterateOverDiff({
    diff,
    diffStyle,
    callback: ({ hunkIndex, collapsedBefore, collapsedAfter, deletionLine, additionLine }) => {
      if (collapsedBefore > 0) {
        if (hunkIndex > 0) {
          height += metrics.fileGap;
        }
        height += metrics.hunkSeparatorHeight + metrics.fileGap;
      }
      const noEOFCR = Boolean(additionLine?.noEOFCR) || Boolean(deletionLine?.noEOFCR);
      height += metrics.lineHeight * (noEOFCR ? 2 : 1);
      if (collapsedAfter > 0) {
        height += metrics.fileGap + metrics.hunkSeparatorHeight;
      }
    },
  });
  if (diff.hunks.length > 0) {
    height += metrics.fileGap;
  }
  return height;
}

// targetComputeRenderRangeFromWindow mirrors VirtualizedFileDiff.computeRenderRangeFromWindow.
// targetComputeRenderRangeFromWindow 对齐 VirtualizedFileDiff.computeRenderRangeFromWindow。
export function targetComputeRenderRangeFromWindow(options: {
  // Target-compatible diff file.
  // 目标兼容 diff 文件。
  diff: TargetDiffFile;
  // Active visual mode.
  // 当前可视模式。
  diffStyle: DiffViewMode;
  // File top inside the scroll container.
  // 文件在滚动容器中的顶部。
  fileTop: number;
  // Scroll window top.
  // 滚动窗口顶部。
  windowTop: number;
  // Scroll window bottom.
  // 滚动窗口底部。
  windowBottom: number;
  // Precomputed total file body height.
  // 预计算的文件正文总高度。
  totalHeight: number;
  // Target metrics.
  // 目标度量值。
  metrics?: TargetDiffVirtualMetrics;
}): TargetDiffRenderRange {
  const metrics = options.metrics ?? TARGET_DIFF_VIRTUAL_METRICS;
  const totalHeight = options.totalHeight;
  const expandedLineCount = targetExpandedLineCount(options.diff, options.diffStyle);
  if (options.fileTop < options.windowTop - totalHeight || options.fileTop > options.windowBottom) {
    return { startingLine: 0, totalLines: 0, bufferBefore: 0, bufferAfter: totalHeight };
  }
  if (expandedLineCount <= metrics.hunkLineCount || options.diff.hunks.length === 0) {
    return { startingLine: 0, totalLines: metrics.hunkLineCount, bufferBefore: 0, bufferAfter: 0 };
  }

  const viewportLineCount = Math.ceil(Math.max(options.windowBottom - options.windowTop, 0) / metrics.lineHeight);
  const totalLines = Math.ceil(viewportLineCount / metrics.hunkLineCount) * metrics.hunkLineCount + metrics.hunkLineCount;
  const chunkCount = totalLines / metrics.hunkLineCount;
  const chunksBeforeAfter = chunkCount;
  const chunkOffsets: number[] = [];
  const viewportCenter = (options.windowTop + options.windowBottom) / 2;
  let cursor = options.fileTop;
  let lineCount = 0;
  let firstVisibleChunk: number | undefined;
  let centerChunk: number | undefined;
  let chunksAfter: number | undefined;

  targetIterateOverDiff({
    diff: options.diff,
    diffStyle: options.diffStyle,
    callback: ({ hunkIndex, collapsedBefore, collapsedAfter, deletionLine, additionLine }) => {
      let beforeHeight = collapsedBefore > 0 ? metrics.hunkSeparatorHeight + metrics.fileGap + (hunkIndex > 0 ? metrics.fileGap : 0) : 0;
      cursor += beforeHeight;
      const isChunkBoundary = lineCount % metrics.hunkLineCount === 0;
      if (isChunkBoundary) {
        chunkOffsets.push(cursor - (options.fileTop + beforeHeight));
        if (chunksAfter != null) {
          if (chunksAfter <= 0) {
            return true;
          }
          chunksAfter -= 1;
        }
      }
      const noEOFCR = Boolean(additionLine?.noEOFCR) || Boolean(deletionLine?.noEOFCR);
      const lineHeight = metrics.lineHeight * (noEOFCR ? 2 : 1);
      const chunkIndex = Math.floor(lineCount / metrics.hunkLineCount);
      if (cursor > options.windowTop - lineHeight && cursor < options.windowBottom) {
        firstVisibleChunk ??= chunkIndex;
      }
      if (centerChunk == null && cursor + lineHeight > viewportCenter) {
        centerChunk = chunkIndex;
      }
      if (chunksAfter == null && cursor >= options.windowBottom && isChunkBoundary) {
        chunksAfter = chunksBeforeAfter;
      }
      lineCount += 1;
      cursor += lineHeight;
      if (collapsedAfter > 0) {
        cursor += metrics.hunkSeparatorHeight + metrics.fileGap;
      }
      return false;
    },
  });

  if (firstVisibleChunk == null) {
    return { startingLine: 0, totalLines: 0, bufferBefore: 0, bufferAfter: totalHeight };
  }
  const chunkOffsetCount = chunkOffsets.length;
  centerChunk ??= firstVisibleChunk;
  const rawStartingChunk = Math.round(centerChunk - chunkCount / 2);
  const maxStartingChunk = Math.max(0, chunkOffsetCount - chunkCount);
  const startingChunk = Math.max(0, Math.min(rawStartingChunk, maxStartingChunk));
  const startingLine = startingChunk * metrics.hunkLineCount;
  const linesToRender = rawStartingChunk < 0 ? totalLines + rawStartingChunk * metrics.hunkLineCount : totalLines;
  const bufferBefore = chunkOffsets[startingChunk] ?? 0;
  const endingChunk = startingChunk + linesToRender / metrics.hunkLineCount;
  return {
    startingLine,
    totalLines: linesToRender,
    bufferBefore,
    bufferAfter: endingChunk < chunkOffsets.length ? totalHeight - chunkOffsets[endingChunk] : totalHeight - (cursor - options.fileTop),
  };
}

// buildTargetDiffFile converts the project parser output into target hunkContent form.
// buildTargetDiffFile 将项目解析器输出转换为目标 hunkContent 形态。
function buildTargetDiffFile(file: DiffFile): TargetDiffFile {
  const additionLines: DiffLine[] = [];
  const deletionLines: DiffLine[] = [];
  const hunks: TargetDiffHunk[] = [];
  let nextSplitLine = 0;
  let nextUnifiedLine = 0;
  let previousAdditionEnd = 0;

  for (const hunk of file.hunks) {
    const hunkHeader = parseTargetHunkHeader(hunk.header, hunk.oldStart, hunk.newStart);
    const targetHunk: TargetDiffHunk = {
      header: hunk.header,
      collapsedBefore: Math.max(hunkHeader.additionStart - 1 - previousAdditionEnd, 0),
      splitLineStart: 0,
      splitLineCount: 0,
      unifiedLineStart: 0,
      unifiedLineCount: 0,
      deletionStart: hunkHeader.deletionStart,
      additionStart: hunkHeader.additionStart,
      deletionCount: hunkHeader.deletionCount,
      additionCount: hunkHeader.additionCount,
      deletionLines: 0,
      additionLines: 0,
      deletionLineIndex: deletionLines.length,
      additionLineIndex: additionLines.length,
      hunkContent: [],
      noEOFCRAdditions: false,
      noEOFCRDeletions: false,
    };

    appendTargetHunkContent(targetHunk, hunk.lines, additionLines, deletionLines);
    for (const content of targetHunk.hunkContent) {
      if (content.type === "context") {
        targetHunk.splitLineCount += content.lines;
        targetHunk.unifiedLineCount += content.lines;
      } else {
        targetHunk.splitLineCount += Math.max(content.additions, content.deletions);
        targetHunk.unifiedLineCount += content.additions + content.deletions;
      }
    }
    targetHunk.splitLineStart = nextSplitLine + targetHunk.collapsedBefore;
    targetHunk.unifiedLineStart = nextUnifiedLine + targetHunk.collapsedBefore;
    nextSplitLine += targetHunk.collapsedBefore + targetHunk.splitLineCount;
    nextUnifiedLine += targetHunk.collapsedBefore + targetHunk.unifiedLineCount;
    previousAdditionEnd = targetHunk.additionStart + targetHunk.additionCount - 1;
    hunks.push(targetHunk);
  }

  return {
    source: file,
    name: file.newPath,
    prevName: file.status === "renamed" ? file.oldPath : undefined,
    hunks,
    additionLines,
    deletionLines,
    splitLineCount: nextSplitLine,
    unifiedLineCount: nextUnifiedLine,
    isPartial: Boolean(file.isPartial) || file.fileDiffLoadState?.status === "loading",
  };
}

// appendTargetHunkContent groups raw hunk lines into target context/change blocks.
// appendTargetHunkContent 将原始 hunk 行分组为目标 context/change 块。
function appendTargetHunkContent(
  targetHunk: TargetDiffHunk,
  lines: readonly DiffLine[],
  additionLines: DiffLine[],
  deletionLines: DiffLine[],
): void {
  let activeContent: TargetDiffContent | null = null;
  for (let sourceLineIndex = 0; sourceLineIndex < lines.length; sourceLineIndex += 1) {
    const line = lines[sourceLineIndex];
    if (line.kind === "added") {
      targetHunk.noEOFCRAdditions = targetHunk.noEOFCRAdditions || targetNoEofLine(line);
      if (activeContent?.type !== "change") {
        activeContent = {
          type: "change",
          additions: 0,
          deletions: 0,
          additionLineIndex: additionLines.length,
          deletionLineIndex: deletionLines.length,
          additionSourceLineIndexes: [],
          deletionSourceLineIndexes: [],
        };
        targetHunk.hunkContent.push(activeContent);
      }
      additionLines.push(line);
      activeContent.additions += 1;
      activeContent.additionSourceLineIndexes.push(sourceLineIndex);
      targetHunk.additionLines += 1;
      continue;
    }
    if (line.kind === "removed") {
      targetHunk.noEOFCRDeletions = targetHunk.noEOFCRDeletions || targetNoEofLine(line);
      if (activeContent?.type !== "change") {
        activeContent = {
          type: "change",
          additions: 0,
          deletions: 0,
          additionLineIndex: additionLines.length,
          deletionLineIndex: deletionLines.length,
          additionSourceLineIndexes: [],
          deletionSourceLineIndexes: [],
        };
        targetHunk.hunkContent.push(activeContent);
      }
      deletionLines.push(line);
      activeContent.deletions += 1;
      activeContent.deletionSourceLineIndexes.push(sourceLineIndex);
      targetHunk.deletionLines += 1;
      continue;
    }
    if (line.kind === "metadata") {
      targetHunk.noEOFCRAdditions = targetHunk.noEOFCRAdditions || line.content.includes("No newline at end of file");
      targetHunk.noEOFCRDeletions = targetHunk.noEOFCRDeletions || line.content.includes("No newline at end of file");
      continue;
    }
    if (activeContent?.type !== "context") {
      activeContent = {
        type: "context",
        lines: 0,
        additionLineIndex: additionLines.length,
        deletionLineIndex: deletionLines.length,
        sourceLineIndexes: [],
      };
      targetHunk.hunkContent.push(activeContent);
    }
    additionLines.push(line);
    deletionLines.push(line);
    activeContent.lines += 1;
    activeContent.sourceLineIndexes.push(sourceLineIndex);
    targetHunk.additionLines += 1;
    targetHunk.deletionLines += 1;
  }
}

// parseTargetHunkHeader extracts target-compatible line counts from a hunk header.
// parseTargetHunkHeader 从 hunk 头部提取目标兼容行计数。
function parseTargetHunkHeader(
  header: string,
  fallbackDeletionStart: number,
  fallbackAdditionStart: number,
): {
  // Hunk old-file start line.
  // hunk 旧文件起始行。
  deletionStart: number;
  // Hunk old-file line count.
  // hunk 旧文件行数。
  deletionCount: number;
  // Hunk new-file start line.
  // hunk 新文件起始行。
  additionStart: number;
  // Hunk new-file line count.
  // hunk 新文件行数。
  additionCount: number;
} {
  const match = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(header);
  return {
    deletionStart: match ? Number.parseInt(match[1], 10) : fallbackDeletionStart,
    deletionCount: match?.[2] ? Number.parseInt(match[2], 10) : 1,
    additionStart: match ? Number.parseInt(match[3], 10) : fallbackAdditionStart,
    additionCount: match?.[4] ? Number.parseInt(match[4], 10) : 1,
  };
}

// targetExpandedRegion mirrors the target Id helper for collapsed context regions.
// targetExpandedRegion 对齐目标 Id helper，用于折叠上下文区域。
function targetExpandedRegion(
  isPartial: boolean,
  collapsedLineCount: number,
  expandedHunks: boolean | ReadonlyMap<number, { fromStart: number; fromEnd: number }> | undefined,
  hunkIndex: number,
  collapsedContextThreshold: number,
): TargetDiffExpandedRegion {
  const rangeSize = Math.max(collapsedLineCount, 0);
  if (rangeSize === 0 || isPartial) {
    return { fromStart: 0, fromEnd: 0, rangeSize, collapsedLines: Math.max(rangeSize, 0) };
  }
  if (expandedHunks === true || rangeSize <= collapsedContextThreshold) {
    return { fromStart: rangeSize, fromEnd: 0, rangeSize, collapsedLines: 0 };
  }
  const expandedHunk = expandedHunks ? expandedHunks.get(hunkIndex) : undefined;
  const fromStart = Math.min(Math.max(expandedHunk?.fromStart ?? 0, 0), rangeSize);
  const fromEnd = Math.min(Math.max(expandedHunk?.fromEnd ?? 0, 0), rangeSize);
  const expandedLineCount = fromStart + fromEnd;
  const renderAll = expandedLineCount >= rangeSize;
  return {
    fromStart: renderAll ? rangeSize : fromStart,
    fromEnd: renderAll ? 0 : fromEnd,
    rangeSize,
    collapsedLines: Math.max(rangeSize - expandedLineCount, 0),
  };
}

// targetHasTrailingContext mirrors the target Ld helper.
// targetHasTrailingContext 对齐目标 Ld helper。
function targetHasTrailingContext(diff: TargetDiffFile): boolean {
  const finalHunk = diff.hunks.at(-1);
  return finalHunk == null || diff.isPartial || diff.additionLines.length === 0 || diff.deletionLines.length === 0
    ? false
    : finalHunk.additionLineIndex + finalHunk.additionCount < diff.additionLines.length
      || finalHunk.deletionLineIndex + finalHunk.deletionCount < diff.deletionLines.length;
}

// targetChangedContentRanges mirrors the target Rd helper for windowed change blocks.
// targetChangedContentRanges 对齐目标 Rd helper，用于窗口化 change 块。
function targetChangedContentRanges(
  state: {
    // Whether the iteration is windowed.
    // 当前迭代是否为窗口化迭代。
    isWindowedHighlight: boolean;
    // Window start in target line coordinates.
    // 目标行坐标中的窗口起点。
    viewportStart: number;
    // Window end in target line coordinates.
    // 目标行坐标中的窗口终点。
    viewportEnd: number;
    // Current unified counter.
    // 当前 unified 计数。
    unifiedCount: number;
    // Current split counter.
    // 当前 split 计数。
    splitCount: number;
  },
  content: Extract<TargetDiffContent, { type: "change" }>,
  diffStyle: DiffViewMode | "both",
): Array<[number, number]> {
  if (!state.isWindowedHighlight) {
    return [[0, diffStyle === "unified" ? content.deletions + content.additions : Math.max(content.deletions, content.additions)]];
  }
  const includeUnified = diffStyle !== "split";
  const includeSplit = diffStyle !== "unified";
  const baseMode = diffStyle === "unified" ? "unified" : "split";
  const ranges: Array<[number, number]> = [];
  function windowRange(baseIndex: number, count: number): [number, number] | undefined {
    if (baseIndex + count <= state.viewportStart || baseIndex >= state.viewportEnd) {
      return undefined;
    }
    const start = Math.max(0, state.viewportStart - baseIndex);
    const end = Math.min(count, state.viewportEnd - baseIndex);
    return end > start ? [start, end] : undefined;
  }
  function contentRange(range: [number, number], side: "additions" | "deletions"): [number, number] {
    return baseMode === "split" ? range : side === "additions" ? [range[0] + content.deletions, range[1] + content.deletions] : range;
  }
  function pushRange(range: [number, number] | undefined, side: "additions" | "deletions"): void {
    if (range == null) {
      return;
    }
    const [start, end] = contentRange(range, side);
    if (end > start) {
      ranges.push([start, end]);
    }
  }
  if (includeUnified) {
    pushRange(windowRange(state.unifiedCount, content.deletions), "deletions");
    pushRange(windowRange(state.unifiedCount + content.deletions, content.additions), "additions");
  }
  if (includeSplit) {
    pushRange(windowRange(state.splitCount, content.deletions), "deletions");
    pushRange(windowRange(state.splitCount, content.additions), "additions");
  }
  if (ranges.length === 0) {
    return ranges;
  }
  ranges.sort((left, right) => left[0] - right[0]);
  const merged: Array<[number, number]> = [ranges[0]];
  for (const [start, end] of ranges.slice(1)) {
    const previous = merged[merged.length - 1];
    if (start <= previous[1]) {
      previous[1] = Math.max(previous[1], end);
    } else {
      merged.push([start, end]);
    }
  }
  return merged;
}

// targetChangedLineEvent mirrors the target zd helper for one change row.
// targetChangedLineEvent 对齐目标 zd helper，用于生成单条 change 行事件。
function targetChangedLineEvent(input: {
  // Owning hunk index.
  // 所属 hunk 索引。
  hunkIndex: number;
  // Owning hunk.
  // 所属 hunk。
  hunk: TargetDiffHunk;
  // Collapsed rows after this row.
  // 当前行之后的折叠行数。
  collapsedAfter: number;
  // Collapsed rows before this row.
  // 当前行之前的折叠行数。
  collapsedBefore: number;
  // Active diff visual style.
  // 当前 diff 可视模式。
  diffStyle: DiffViewMode | "both";
  // Index inside the current change content block.
  // 当前 change 内容块内部索引。
  index: number;
  // Unified line index at content block start.
  // 内容块起始 unified 行索引。
  unifiedLineIndex: number;
  // Split line index at content block start.
  // 内容块起始 split 行索引。
  splitLineIndex: number;
  // Addition line index at content block start.
  // 内容块起始新增行索引。
  additionLineIndex: number;
  // Deletion line index at content block start.
  // 内容块起始删除行索引。
  deletionLineIndex: number;
  // Addition line number at content block start.
  // 内容块起始新增行号。
  additionLineNumber: number;
  // Deletion line number at content block start.
  // 内容块起始删除行号。
  deletionLineNumber: number;
  // Current change content block.
  // 当前 change 内容块。
  content: Extract<TargetDiffContent, { type: "change" }>;
  // Whether this block is the final content block in its hunk.
  // 当前块是否为 hunk 中最后一个内容块。
  isLastContent: boolean;
  // Unified row count for this change block.
  // 当前 change 块的 unified 行数。
  unifiedCount: number;
  // Split row count for this change block.
  // 当前 change 块的 split 行数。
  splitCount: number;
}): TargetDiffIterationEvent {
  const deletionUnifiedLineIndex = input.index < input.content.deletions ? input.unifiedLineIndex + input.index : undefined;
  const additionUnifiedLineIndex = input.diffStyle === "unified"
    ? input.index >= input.content.deletions
      ? input.unifiedLineIndex + input.index
      : undefined
    : input.index < input.content.additions
      ? input.unifiedLineIndex + input.content.deletions + input.index
      : undefined;
  const splitLineIndex = input.diffStyle === "unified"
    ? input.splitLineIndex + (input.index < input.content.deletions ? input.index : input.index - input.content.deletions)
    : input.splitLineIndex + input.index;
  const deletionLineIndex = input.index < input.content.deletions ? input.deletionLineIndex + input.index : undefined;
  const deletionLineNumber = input.index < input.content.deletions ? input.deletionLineNumber + input.index : undefined;
  const additionLineIndex = input.diffStyle === "unified"
    ? input.index >= input.content.deletions
      ? input.additionLineIndex + (input.index - input.content.deletions)
      : undefined
    : input.index < input.content.additions
      ? input.additionLineIndex + input.index
      : undefined;
  const additionLineNumber = input.diffStyle === "unified"
    ? input.index >= input.content.deletions
      ? input.additionLineNumber + (input.index - input.content.deletions)
      : undefined
    : input.index < input.content.additions
      ? input.additionLineNumber + input.index
      : undefined;
  const deletionNoEOFCR = input.diffStyle === "unified"
    ? input.isLastContent && input.index === input.content.deletions - 1 && input.hunk.noEOFCRDeletions
    : input.isLastContent && input.index === input.splitCount - 1 && input.hunk.noEOFCRDeletions;
  const additionNoEOFCR = input.diffStyle === "unified"
    ? input.isLastContent && input.index === input.unifiedCount - 1 && input.hunk.noEOFCRAdditions
    : input.isLastContent && input.index === input.splitCount - 1 && input.hunk.noEOFCRAdditions;
  const deletionLine = deletionLineIndex != null && deletionLineNumber != null && deletionUnifiedLineIndex != null
    ? {
        lineNumber: deletionLineNumber,
        lineIndex: deletionLineIndex,
        noEOFCR: Boolean(deletionNoEOFCR),
        unifiedLineIndex: deletionUnifiedLineIndex,
        splitLineIndex,
        sourceLineIndex: input.content.deletionSourceLineIndexes[input.index],
      }
    : undefined;
  const additionSourceIndex = input.diffStyle === "unified"
    ? input.index >= input.content.deletions
      ? input.content.additionSourceLineIndexes[input.index - input.content.deletions]
      : undefined
    : input.content.additionSourceLineIndexes[input.index];
  const additionLine = additionLineIndex != null && additionLineNumber != null && additionUnifiedLineIndex != null
    ? {
        unifiedLineIndex: additionUnifiedLineIndex,
        splitLineIndex,
        lineIndex: additionLineIndex,
        lineNumber: additionLineNumber,
        noEOFCR: Boolean(additionNoEOFCR),
        sourceLineIndex: additionSourceIndex,
      }
    : undefined;
  if (deletionLine == null && additionLine != null) {
    return {
      type: "change",
      hunkIndex: input.hunkIndex,
      hunk: input.hunk,
      collapsedAfter: input.collapsedAfter,
      collapsedBefore: input.collapsedBefore,
      deletionLine: undefined,
      additionLine,
    };
  }
  if (deletionLine != null && additionLine == null) {
    return {
      type: "change",
      hunkIndex: input.hunkIndex,
      hunk: input.hunk,
      collapsedAfter: input.collapsedAfter,
      collapsedBefore: input.collapsedBefore,
      deletionLine,
      additionLine: undefined,
    };
  }
  if (deletionLine == null || additionLine == null) {
    throw Error("iterateOverDiff: missing change line data");
  }
  return {
    type: "change",
    hunkIndex: input.hunkIndex,
    hunk: input.hunk,
    collapsedAfter: input.collapsedAfter,
    collapsedBefore: input.collapsedBefore,
    deletionLine,
    additionLine,
  };
}

// targetNoEofLine detects parser metadata that marks a no-newline row.
// targetNoEofLine 检测解析器中标记无换行的行。
function targetNoEofLine(line: DiffLine): boolean {
  return line.content === "\\ No newline at end of file" || line.content.includes("No newline at end of file");
}
