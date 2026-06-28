import type { DiffFile, DiffViewMode } from "./types";
import { buildTargetDiffFile } from "./target-diff-model/builder";
import type {
  TargetDiffContent,
  TargetDiffExpandedRegion,
  TargetDiffFile,
  TargetDiffHunk,
  TargetDiffIterationEvent,
  TargetDiffRenderRange,
  TargetDiffVirtualMetrics,
  TargetTrailingContextExpansion,
} from "./target-diff-model/types";
import { TARGET_DIFF_VIRTUAL_METRICS } from "./target-diff-model/types";
export {
  TARGET_DIFF_FULL_RANGE,
  TARGET_DIFF_VIRTUAL_METRICS,
} from "./target-diff-model/types";
export type {
  TargetDiffContent,
  TargetDiffFile,
  TargetDiffHunk,
  TargetDiffIterationEvent,
  TargetDiffIterationLine,
  TargetDiffRenderRange,
  TargetDiffVirtualMetrics,
  TargetTrailingContextExpansion,
} from "./target-diff-model/types";

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
  return file.hunks.length + contentRows + (targetTrailingContextExpansion(model) ? 1 : 0);
}

// targetTrailingContextExpansion returns the final hidden EOF region from backend line counts.
// targetTrailingContextExpansion 基于后端完整行数返回最终隐藏 EOF 区域。
export function targetTrailingContextExpansion(diff: TargetDiffFile): TargetTrailingContextExpansion | null {
  if (diff.isPartial || diff.hunks.length === 0) {
    return null;
  }
  const finalHunk = diff.hunks[diff.hunks.length - 1];
  const oldRemaining = diff.source.oldFileLineCount == null || diff.source.status === "added"
    ? null
    : Math.max(diff.source.oldFileLineCount - (finalHunk.deletionStart + finalHunk.deletionCount - 1), 0);
  const newRemaining = diff.source.newFileLineCount == null || diff.source.status === "deleted"
    ? null
    : Math.max(diff.source.newFileLineCount - (finalHunk.additionStart + finalHunk.additionCount - 1), 0);
  const collapsedLines = targetComparableTrailingLineCount(oldRemaining, newRemaining);
  if (collapsedLines <= 0) {
    return null;
  }
  return {
    hunkIndex: diff.hunks.length - 1,
    collapsedLines,
  };
}
// targetComparableTrailingLineCount chooses the shared hidden tail length for the visible side set.
// targetComparableTrailingLineCount 为可见侧集合选择共享隐藏尾部长度。
function targetComparableTrailingLineCount(oldRemaining: number | null, newRemaining: number | null): number {
  if (oldRemaining == null) {
    return newRemaining ?? 0;
  }
  if (newRemaining == null) {
    return oldRemaining;
  }
  return Math.min(oldRemaining, newRemaining);
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
      const trailingExpansion = targetTrailingContextExpansion(options.diff);
      if (hunk !== state.finalHunk || trailingExpansion == null) {
        return undefined;
      }
      return targetExpandedRegion(
        options.diff.isPartial,
        trailingExpansion.collapsedLines,
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
  const trailingExpansion = targetTrailingContextExpansion(diff);
  if (finalHunk != null && trailingExpansion != null) {
    if (trailingExpansion.collapsedLines > 0) {
      const region = targetExpandedRegion(diff.isPartial, trailingExpansion.collapsedLines, expandedHunks, diff.hunks.length, collapsedContextThreshold);
      lineCount += region.collapsedLines === 0 ? trailingExpansion.collapsedLines : region.fromStart;
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
