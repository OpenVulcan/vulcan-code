import React from "react";
import { ChevronDown, ChevronRight } from "lucide-react";

import type { DiffFile, DiffHunkExpansionDirection, DiffHunkExpansionRequest, DiffViewMode, WorkspaceDiffSearchFileMatch } from "./types";
import {
  diffLanguageForFile,
  useHighlightedDiffFileRevision,
  useHighlightedDiffWindowRevision,
  useHighlightLanguage,
} from "./diff-highlight";
import {
  DIFF_LIGHTWEIGHT_DETAIL_CHANGED_LINES_LIMIT,
  DIFF_FILE_VIRTUALIZATION_THRESHOLD,
  DIFF_HUNK_HEADER_HEIGHT,
  DIFF_LINE_HEIGHT,
  DIFF_SCROLL_STATE_GRANULARITY_PX,
  DIFF_SHIKI_TOKEN_ROW_LIMIT,
  formatLargeReasons,
  shouldUseLightweightDiffRendering,
  DIFF_SUMMARY_PLACEHOLDER_MIN_HEIGHT,
} from "./limits";
import {
  type DiffMountedVirtualRow,
  type DiffScrollState,
  buildDiffVirtualLayout,
  elementTopWithinScrollContainer,
  fileTotalRowCount,
  mountedDiffRows,
} from "./diff-virtual-layout";
import {
  useHighlightedDiffFileHtml,
  useHighlightedDiffRowsHtml,
  usePlainDiffFileHtml,
  usePlainDiffRowsHtml,
} from "./diff-render-worker-client";

// DIFF_DEFAULT_CONTEXT_LINES matches Git's default unified-diff context.
// DIFF_DEFAULT_CONTEXT_LINES 对齐 Git 默认 unified-diff 上下文行数。
const DIFF_DEFAULT_CONTEXT_LINES = 3;

// DIFF_CONTEXT_EXPANSION_STEPS defines bounded hunk context expansion requests.
// DIFF_CONTEXT_EXPANSION_STEPS 定义受限的 hunk 上下文展开请求档位。
const DIFF_CONTEXT_EXPANSION_STEPS = [20, 50, 100] as const;

// DIFF_SUMMARY_PLACEHOLDER_MAX_HUNKS caps rendered summary-only hunk skeleton nodes.
// DIFF_SUMMARY_PLACEHOLDER_MAX_HUNKS 限制 summary-only hunk 骨架节点数量。
const DIFF_SUMMARY_PLACEHOLDER_MAX_HUNKS = 24;

// DIFF_SUMMARY_PLACEHOLDER_MAX_LINES_PER_HUNK caps rendered skeleton lines per hunk.
// DIFF_SUMMARY_PLACEHOLDER_MAX_LINES_PER_HUNK 限制每个 hunk 的骨架行数量。
const DIFF_SUMMARY_PLACEHOLDER_MAX_LINES_PER_HUNK = 6;

// diffFileLoadedRowsHeightCache stores full loaded-file body heights per stable diff file.
// diffFileLoadedRowsHeightCache 按稳定 diff 文件缓存完整已加载文件体高度。
const diffFileLoadedRowsHeightCache = new WeakMap<DiffFile, number>();

// diffMountedRowsHeightCache stores mounted-window heights per stable row array.
// diffMountedRowsHeightCache 按稳定行数组缓存挂载窗口高度。
const diffMountedRowsHeightCache = new WeakMap<readonly DiffMountedVirtualRow[], number>();

// diffFileLoadedRowsPlaceholderStyleCache stores stable pending placeholder styles per loaded file.
// diffFileLoadedRowsPlaceholderStyleCache 按已加载文件缓存稳定等待占位样式。
const diffFileLoadedRowsPlaceholderStyleCache = new WeakMap<DiffFile, React.CSSProperties>();

// diffMountedRowsPlaceholderStyleCache stores stable pending placeholder styles per mounted row window.
// diffMountedRowsPlaceholderStyleCache 按挂载行窗口缓存稳定等待占位样式。
const diffMountedRowsPlaceholderStyleCache = new WeakMap<readonly DiffMountedVirtualRow[], React.CSSProperties>();

// DiffFileCardProps stores render inputs for one memoized file card.
// DiffFileCardProps 保存单个记忆化文件卡片的渲染输入。
interface DiffFileCardProps {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
  // Estimated line host top supplied by file-list virtualization.
  // 文件列表虚拟化提供的行宿主估算顶部。
  virtualHostTop?: number;
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode: DiffViewMode;
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行键。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行键。
  activeSearchLineKey?: string | null;
  // Optional lazy detail loader used by retryable summary-only files.
  // 可选的懒加载详情函数，用于可重试的摘要文件。
  onLoadFileDetail?: (file: DiffFile, contextLines?: number, expansion?: DiffHunkExpansionRequest) => void;
  // Optional backend search hit shown before full detail hydration finishes.
  // 完整详情水合完成前展示的可选后端搜索命中。
  summarySearchMatch?: WorkspaceDiffSearchFileMatch;
}

// DiffFileCard renders one file-level diff card.
// DiffFileCard 渲染单个文件级 diff 卡片。
export const DiffFileCard = React.memo(function DiffFileCard(props: DiffFileCardProps) {
  const [collapsed, setCollapsed] = React.useState(false);
  const ToggleIcon = collapsed ? ChevronRight : ChevronDown;
  const rowCount = React.useMemo(() => fileTotalRowCount(props.file), [props.file]);
  const lightweightRendering = shouldUseLightweightDiffRendering(props.file);
  const shouldUseShikiTokens = !lightweightRendering && rowCount <= DIFF_SHIKI_TOKEN_ROW_LIMIT;
  const inlineDiff = !lightweightRendering;
  const language = !collapsed && shouldUseShikiTokens ? diffLanguageForFile(props.file) : null;
  const highlightSupported = useHighlightLanguage(language);
  const contextExpansionAvailable = props.onLoadFileDetail ? canExpandDiffFileContext(props.file) : false;
  const handleToggle = React.useCallback(() => {
    setCollapsed((previous) => !previous);
  }, []);
  const handleExpandFileContext = React.useCallback((hunkIndex: number, direction: DiffHunkExpansionDirection) => {
    if (canExpandDiffFileContext(props.file)) {
      props.onLoadFileDetail?.(props.file, nextExpandedContextLineCount(props.file), { hunkIndex, direction });
    }
  }, [props.file, props.onLoadFileDetail]);
  return (
    <article className={`diff-file-card ${props.file.status}`}>
      <button className="diff-file-header" onClick={handleToggle} type="button">
        <ToggleIcon size={15} />
        <div className="diff-file-paths">
          <strong>
            <span>{displayPath(props.file)}</span>
            {props.file.status === "added" ? <span aria-label="新增文件" className="diff-file-status-dot" /> : null}
          </strong>
          {props.file.status === "renamed" ? <span>{`${props.file.oldPath} -> ${props.file.newPath}`}</span> : null}
        </div>
        <div className="diff-file-meta">
          <span className="diff-stat added">+{props.file.additions}</span>
          <span className="diff-stat removed">-{props.file.deletions}</span>
        </div>
      </button>
      {!collapsed ? (
        <div className="diff-file-body">
          {props.file.isBinary ? (
            <div className="diff-binary-note">Binary file not shown</div>
          ) : props.file.isGitlink ? (
            <div className="diff-binary-note">子模块指针变更，未渲染文本内容。</div>
          ) : props.file.hunks.length ? (
            <>
              <DiffRenderPolicyNote file={props.file} lightweightRendering={lightweightRendering} rowCount={rowCount} />
              <DiffFileLines
                file={props.file}
                highlightSupported={highlightSupported}
                language={language}
                rowCount={rowCount}
                inlineDiff={inlineDiff}
                viewMode={props.viewMode}
                scrollContainerRef={props.scrollContainerRef}
                scrollState={props.scrollState}
                virtualHostTop={props.virtualHostTop}
                activeSearchLineKey={props.activeSearchLineKey}
                searchLineKeys={props.searchLineKeys}
                onExpandFileContext={contextExpansionAvailable ? handleExpandFileContext : undefined}
              />
            </>
          ) : props.file.isPartial ? (
            <DiffFileLoadStateNote file={props.file} summarySearchMatch={props.summarySearchMatch} onRetry={props.onLoadFileDetail} />
          ) : props.file.status === "renamed" ? (
            <div className="diff-binary-note">File renamed without changes</div>
          ) : (
            <div className="diff-binary-note">No content</div>
          )}
        </div>
      ) : null}
    </article>
  );
}, areDiffFileCardPropsEqual);

// DiffFileLoadStateNote displays lazy detail hydration state for summary-only files.
// DiffFileLoadStateNote 展示摘要文件的懒加载详情水合状态。
function DiffFileLoadStateNote(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Optional backend search hit shown before full detail hydration finishes.
  // 完整详情水合完成前展示的可选后端搜索命中。
  summarySearchMatch?: WorkspaceDiffSearchFileMatch;
  // Optional retry action for failed or timed-out loads.
  // 失败或超时加载对应的可选重试动作。
  onRetry?: (file: DiffFile) => void;
}) {
  const loadState = props.file.fileDiffLoadState;
  const status = loadState?.status ?? "idle";
  const message = loadState?.message ?? diffFileLoadStateMessage(status);
  const metrics = diffSummaryPlaceholderMetrics(props.file, props.summarySearchMatch);
  // canRetry exposes manual recovery only for terminal lazy-load states.
  // canRetry 仅为终止态懒加载状态提供手动恢复入口。
  const canRetry = Boolean(props.onRetry) && (status === "failed" || status === "timed-out");
  return (
    <div className={`diff-summary-placeholder diff-file-load-state ${status}`} style={metrics.style}>
      <div className="diff-summary-placeholder-canvas" aria-hidden="true">
        {renderSummaryPlaceholderHunks(metrics.hunks)}
      </div>
      <div className="diff-summary-placeholder-note">
        <span>{message}</span>
        {status === "idle" || status === "loading" ? <small>{`${metrics.rowCount} 行待加载，约 ${metrics.hunkCount} 个 hunk`}</small> : null}
        {canRetry ? (
          <button className="diff-file-load-retry" onClick={() => props.onRetry?.(props.file)} type="button">
            重试
          </button>
        ) : null}
      </div>
      {props.summarySearchMatch ? (
        <div
          className="diff-file-search-placeholder diff-summary-search-marker"
          data-diff-summary-search-file-key={props.summarySearchMatch.key}
          style={metrics.searchStyle}
        >
          <strong>{diffSummarySearchMatchLocation(props.summarySearchMatch)}</strong>
          <div className="diff-summary-search-previews">
            {renderSummarySearchPreviews(props.summarySearchMatch)}
          </div>
        </div>
      ) : null}
    </div>
  );
}

// renderSummaryPlaceholderHunks renders bounded summary skeleton hunks without callback allocation.
// renderSummaryPlaceholderHunks 渲染受限 summary 骨架 hunk，避免回调式数组分配。
function renderSummaryPlaceholderHunks(hunks: readonly DiffSummaryPlaceholderHunk[]): React.ReactNode[] {
  const nodes: React.ReactNode[] = new Array(hunks.length);
  for (let index = 0; index < hunks.length; index += 1) {
    const hunk = hunks[index];
    nodes[index] = (
      <div className={`diff-summary-hunk-placeholder${hunk.active ? " active" : ""}`} key={hunk.index} style={hunk.style}>
        <div className="diff-summary-hunk-header-placeholder">{`@@ hunk ${hunk.index + 1}`}</div>
        <div className="diff-summary-hunk-lines-placeholder">
          {renderSummaryPlaceholderLines(hunk)}
        </div>
      </div>
    );
  }
  return nodes;
}

// renderSummaryPlaceholderLines renders one skeleton hunk's bounded line placeholders.
// renderSummaryPlaceholderLines 渲染单个骨架 hunk 的受限行占位。
function renderSummaryPlaceholderLines(hunk: DiffSummaryPlaceholderHunk): React.ReactNode[] {
  const nodes: React.ReactNode[] = new Array(hunk.lineCount);
  for (let index = 0; index < hunk.lineCount; index += 1) {
    nodes[index] = (
      <span key={`${hunk.index}-${index}`} style={hunk.lineWidthStyles[index]} />
    );
  }
  return nodes;
}

// renderSummarySearchPreviews renders bounded backend snippets without JSX map callbacks.
// renderSummarySearchPreviews 渲染受限后端片段，避免 JSX map 回调。
function renderSummarySearchPreviews(match: WorkspaceDiffSearchFileMatch): React.ReactNode[] {
  const previews = diffSummarySearchMatchPreviews(match);
  const nodes: React.ReactNode[] = new Array(previews.length);
  for (let index = 0; index < previews.length; index += 1) {
    nodes[index] = <code key={`${match.key}-${index}`}>{previews[index]}</code>;
  }
  return nodes;
}

// DiffSummaryPlaceholderHunk stores one rendered hunk skeleton position.
// DiffSummaryPlaceholderHunk 保存单个已渲染 hunk 骨架的位置。
interface DiffSummaryPlaceholderHunk {
  // Source hunk index represented by this placeholder.
  // 当前占位表示的源 hunk 索引。
  index: number;
  // Estimated pixel top inside the summary-only canvas.
  // summary-only 画布内的估算像素顶部。
  top: number;
  // Stable top style for the summary hunk placeholder.
  // summary hunk 占位使用的稳定顶部样式。
  style: React.CSSProperties;
  // Number of lightweight skeleton lines rendered under the hunk header.
  // hunk 头部下方渲染的轻量骨架行数量。
  lineCount: number;
  // Deterministic line widths used to avoid visually flat placeholders.
  // 用于避免占位过于单调的确定性行宽。
  lineWidthPercentages: number[];
  // Stable width styles for deterministic skeleton lines.
  // 确定性骨架行使用的稳定宽度样式。
  lineWidthStyles: React.CSSProperties[];
  // Whether this hunk is the backend summary-search target.
  // 当前 hunk 是否为后端 summary search 目标。
  active: boolean;
}

// DiffSummaryPlaceholderMetrics stores bounded summary-only placeholder geometry.
// DiffSummaryPlaceholderMetrics 保存受限的 summary-only 占位几何信息。
interface DiffSummaryPlaceholderMetrics {
  // Fixed canvas height matching the file-level virtualization estimate.
  // 与文件级虚拟化估算一致的固定画布高度。
  height: number;
  // Stable root style for the summary placeholder.
  // summary 占位根节点使用的稳定样式。
  style: React.CSSProperties;
  // Estimated visual row count represented by this placeholder.
  // 当前占位表示的估算可视行数。
  rowCount: number;
  // Estimated hunk count represented by this placeholder.
  // 当前占位表示的估算 hunk 数量。
  hunkCount: number;
  // Bounded hunk skeletons rendered into the canvas.
  // 渲染进画布的受限 hunk 骨架。
  hunks: DiffSummaryPlaceholderHunk[];
  // Estimated pixel top for the backend search marker.
  // 后端搜索标记的估算像素顶部。
  searchTop: number;
  // Stable top style for the backend search marker.
  // 后端搜索标记使用的稳定顶部样式。
  searchStyle: React.CSSProperties;
}

// diffSummaryPlaceholderMetricsCache stores summary-only geometry per stable diff file object.
// diffSummaryPlaceholderMetricsCache 按稳定 diff 文件对象缓存 summary-only 几何信息。
const diffSummaryPlaceholderMetricsCache = new WeakMap<DiffFile, DiffSummaryPlaceholderMetrics>();

// diffSummarySearchPlaceholderMetricsCache stores search-focused summary geometry per backend match.
// diffSummarySearchPlaceholderMetricsCache 按后端命中缓存带搜索焦点的 summary 几何信息。
const diffSummarySearchPlaceholderMetricsCache = new WeakMap<DiffFile, WeakMap<WorkspaceDiffSearchFileMatch, DiffSummaryPlaceholderMetrics>>();

// diffSummaryPlaceholderMetrics builds a bounded visual skeleton for a summary-only file.
// diffSummaryPlaceholderMetrics 为 summary-only 文件构建受限的可视骨架。
function diffSummaryPlaceholderMetrics(
  file: DiffFile,
  summarySearchMatch: WorkspaceDiffSearchFileMatch | undefined,
): DiffSummaryPlaceholderMetrics {
  if (summarySearchMatch) {
    let metricsByMatch = diffSummarySearchPlaceholderMetricsCache.get(file);
    const cached = metricsByMatch?.get(summarySearchMatch);
    if (cached) {
      return cached;
    }
    const metrics = buildDiffSummaryPlaceholderMetrics(file, summarySearchMatch);
    if (!metricsByMatch) {
      metricsByMatch = new WeakMap();
      diffSummarySearchPlaceholderMetricsCache.set(file, metricsByMatch);
    }
    metricsByMatch.set(summarySearchMatch, metrics);
    return metrics;
  }
  const cached = diffSummaryPlaceholderMetricsCache.get(file);
  if (cached) {
    return cached;
  }
  const metrics = buildDiffSummaryPlaceholderMetrics(file, undefined);
  diffSummaryPlaceholderMetricsCache.set(file, metrics);
  return metrics;
}

// buildDiffSummaryPlaceholderMetrics computes bounded summary-only skeleton geometry.
// buildDiffSummaryPlaceholderMetrics 计算受限的 summary-only 骨架几何信息。
function buildDiffSummaryPlaceholderMetrics(
  file: DiffFile,
  summarySearchMatch: WorkspaceDiffSearchFileMatch | undefined,
): DiffSummaryPlaceholderMetrics {
  const hunkCount = Math.max(1, Math.min(diffSummaryEstimatedRowCount(file), file.estimatedHunkCount ?? conservativeSummaryHunkCount(file)));
  const rowCount = Math.max(hunkCount, diffSummaryEstimatedRowCount(file));
  const lineRows = Math.max(0, rowCount - hunkCount);
  const height = Math.max(DIFF_SUMMARY_PLACEHOLDER_MIN_HEIGHT, diffSummaryEstimatedRowsHeight(hunkCount, rowCount));
  const targetHunkIndex = Math.max(0, Math.min(summarySearchMatch?.firstHunkIndex ?? 0, hunkCount - 1));
  const hunkIndexes = boundedSummaryPlaceholderHunkIndexes(hunkCount, targetHunkIndex);
  const hunks: DiffSummaryPlaceholderHunk[] = new Array(hunkIndexes.length);
  for (let index = 0; index < hunkIndexes.length; index += 1) {
    const hunkIndex = hunkIndexes[index];
    const top = diffSummaryEstimatedHunkTop(hunkIndex, hunkCount, lineRows);
    const nextTop = hunkIndex + 1 < hunkCount
      ? diffSummaryEstimatedHunkTop(hunkIndex + 1, hunkCount, lineRows)
      : height;
    const lineCount = Math.max(1, Math.min(DIFF_SUMMARY_PLACEHOLDER_MAX_LINES_PER_HUNK, Math.floor((nextTop - top - DIFF_HUNK_HEADER_HEIGHT) / DIFF_LINE_HEIGHT)));
    hunks[index] = {
      index: hunkIndex,
      top,
      style: { top },
      lineCount,
      lineWidthPercentages: diffSummaryPlaceholderLineWidths(hunkIndex, lineCount),
      lineWidthStyles: diffSummaryPlaceholderLineWidthStyles(hunkIndex, lineCount),
      active: hunkIndex === targetHunkIndex && Boolean(summarySearchMatch),
    };
  }
  const searchTop = Math.max(0, Math.min(height - 58, diffSummaryEstimatedHunkTop(targetHunkIndex, hunkCount, lineRows) + DIFF_HUNK_HEADER_HEIGHT));
  return {
    height,
    style: { height },
    rowCount,
    hunkCount,
    hunks,
    searchTop,
    searchStyle: { top: searchTop },
  };
}

// diffSummaryEstimatedRowCount returns the best known visual row estimate for a summary-only file.
// diffSummaryEstimatedRowCount 返回 summary-only 文件当前最佳可视行数估算。
function diffSummaryEstimatedRowCount(file: DiffFile): number {
  return Math.max(1, file.estimatedRowCount ?? file.changedLines + conservativeSummaryHunkCount(file));
}

// conservativeSummaryHunkCount returns a bounded hunk count estimate for summary-only files.
// conservativeSummaryHunkCount 返回 summary-only 文件的受限 hunk 数量估算。
function conservativeSummaryHunkCount(file: DiffFile): number {
  return Math.max(1, file.estimatedHunkCount ?? (file.additions + file.deletions > 0 ? 1 : 0));
}

// diffSummaryEstimatedRowsHeight mirrors file-list virtualization row-height math.
// diffSummaryEstimatedRowsHeight 对齐文件列表虚拟化的行高估算。
function diffSummaryEstimatedRowsHeight(hunkCount: number, rowCount: number): number {
  const lineRows = Math.max(0, rowCount - hunkCount);
  return hunkCount * DIFF_HUNK_HEADER_HEIGHT + lineRows * DIFF_LINE_HEIGHT;
}

// diffSummaryEstimatedHunkTop estimates one hunk's top inside a summary-only placeholder.
// diffSummaryEstimatedHunkTop 估算单个 hunk 在 summary-only 占位中的顶部。
function diffSummaryEstimatedHunkTop(hunkIndex: number, hunkCount: number, lineRows: number): number {
  const boundedIndex = Math.max(0, Math.min(hunkIndex, hunkCount - 1));
  const fullLineRowsBefore = Math.floor((lineRows * boundedIndex) / hunkCount);
  return boundedIndex * DIFF_HUNK_HEADER_HEIGHT + fullLineRowsBefore * DIFF_LINE_HEIGHT;
}

// boundedSummaryPlaceholderHunkIndexes selects representative hunk skeletons with the target included.
// boundedSummaryPlaceholderHunkIndexes 选择代表性 hunk 骨架并确保包含目标 hunk。
function boundedSummaryPlaceholderHunkIndexes(hunkCount: number, targetHunkIndex: number): number[] {
  if (hunkCount <= DIFF_SUMMARY_PLACEHOLDER_MAX_HUNKS) {
    const indexes: number[] = new Array(hunkCount);
    for (let index = 0; index < hunkCount; index += 1) {
      indexes[index] = index;
    }
    return indexes;
  }
  const indexes: number[] = [];
  insertSortedSummaryPlaceholderHunkIndex(indexes, 0);
  insertSortedSummaryPlaceholderHunkIndex(indexes, targetHunkIndex);
  insertSortedSummaryPlaceholderHunkIndex(indexes, hunkCount - 1);
  const slots = Math.max(1, DIFF_SUMMARY_PLACEHOLDER_MAX_HUNKS - indexes.length);
  for (let slot = 1; slot <= slots; slot += 1) {
    insertSortedSummaryPlaceholderHunkIndex(indexes, Math.floor((slot * (hunkCount - 1)) / (slots + 1)));
  }
  return indexes;
}

// insertSortedSummaryPlaceholderHunkIndex inserts one unique hunk index into a tiny sorted list.
// insertSortedSummaryPlaceholderHunkIndex 将单个唯一 hunk 索引插入小型有序列表。
function insertSortedSummaryPlaceholderHunkIndex(indexes: number[], hunkIndex: number): void {
  for (let index = 0; index < indexes.length; index += 1) {
    if (indexes[index] === hunkIndex) {
      return;
    }
    if (indexes[index] > hunkIndex) {
      indexes.splice(index, 0, hunkIndex);
      return;
    }
  }
  indexes.push(hunkIndex);
}

// diffSummaryPlaceholderLineWidths returns deterministic skeleton line widths.
// diffSummaryPlaceholderLineWidths 返回确定性的骨架行宽。
function diffSummaryPlaceholderLineWidths(hunkIndex: number, lineCount: number): number[] {
  const widths: number[] = new Array(lineCount);
  for (let lineIndex = 0; lineIndex < lineCount; lineIndex += 1) {
    widths[lineIndex] = 42 + ((hunkIndex * 17 + lineIndex * 13) % 48);
  }
  return widths;
}

// diffSummaryPlaceholderLineWidthStyles returns stable skeleton line width styles.
// diffSummaryPlaceholderLineWidthStyles 返回稳定的骨架行宽样式。
function diffSummaryPlaceholderLineWidthStyles(hunkIndex: number, lineCount: number): React.CSSProperties[] {
  const styles: React.CSSProperties[] = new Array(lineCount);
  for (let lineIndex = 0; lineIndex < lineCount; lineIndex += 1) {
    styles[lineIndex] = { width: `${42 + ((hunkIndex * 17 + lineIndex * 13) % 48)}%` };
  }
  return styles;
}

// diffSummarySearchMatchLocation formats one backend search hit location for summary placeholders.
// diffSummarySearchMatchLocation 为摘要占位提示格式化单个后端搜索命中位置。
function diffSummarySearchMatchLocation(match: WorkspaceDiffSearchFileMatch): string {
  const parts = [`${match.matchCount} 条命中`];
  if (match.firstHunkIndex != null) {
    parts.push(`hunk ${match.firstHunkIndex + 1}`);
  }
  if (match.firstOldLine != null || match.firstNewLine != null) {
    const oldLabel = match.firstOldLine != null ? `-${match.firstOldLine}` : "";
    const newLabel = match.firstNewLine != null ? `+${match.firstNewLine}` : "";
    parts.push(oldLabel && newLabel ? `${oldLabel} / ${newLabel}` : oldLabel || newLabel);
  }
  return parts.join(" · ");
}

// diffSummarySearchMatchPreviews returns bounded backend snippets for summary-only search display.
// diffSummarySearchMatchPreviews 返回摘要态搜索展示使用的受限后端片段。
function diffSummarySearchMatchPreviews(match: WorkspaceDiffSearchFileMatch): string[] {
  const sourcePreviews = match.previews?.length ? match.previews : [match.firstPreview];
  const previews: string[] = [];
  for (const preview of sourcePreviews) {
    const normalizedPreview = preview.trim();
    if (normalizedPreview) {
      previews.push(normalizedPreview);
      if (previews.length >= 4) {
        break;
      }
    }
  }
  return previews.length ? previews : ["后端完整 diff 命中该文件。"];
}

// diffFileLoadStateMessage returns a user-facing lazy detail status message.
// diffFileLoadStateMessage 返回面向用户的懒加载详情状态文案。
function diffFileLoadStateMessage(status: NonNullable<DiffFile["fileDiffLoadState"]>["status"]): string {
  if (status === "loading") {
    return "正在加载文件 diff 详情。";
  }
  if (status === "timed-out") {
    return "文件 diff 详情加载超时。";
  }
  if (status === "failed") {
    return "文件 diff 详情加载失败。";
  }
  if (status === "loaded") {
    return "文件 diff 详情已加载，但没有可展示的文本内容。";
  }
  return "该文件为摘要状态，进入视口后将按需加载详情。";
}

// DiffRenderPolicyNote explains the lightweight rendering policy inside the new renderer.
// DiffRenderPolicyNote 在新渲染器内部解释轻量渲染策略。
function DiffRenderPolicyNote(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Whether expensive detail rendering is skipped by the lightweight policy.
  // 昂贵细节渲染是否被轻量策略省略。
  lightweightRendering: boolean;
  // Precomputed visual row count for this file.
  // 该文件预先计算的可视行数。
  rowCount: number;
}) {
  if (!props.lightweightRendering) {
    return null;
  }
  const reasonText = props.file.largeReasons.length
    ? formatLargeReasons(props.file.largeReasons)
    : props.file.changedLines > DIFF_LIGHTWEIGHT_DETAIL_CHANGED_LINES_LIMIT
      ? `变更行超过 ${DIFF_LIGHTWEIGHT_DETAIL_CHANGED_LINES_LIMIT}，高亮细节已按轻量策略省略`
      : "高亮细节已按轻量策略省略";
  return (
    <div className="diff-render-policy-note">
      <span>{reasonText}</span>
      <span>{`${props.rowCount} 行按轻量模式渲染`}</span>
    </div>
  );
}

// DiffFileLines switches between direct and virtualized rendering for one file.
// DiffFileLines 为单个文件在直接渲染和虚拟渲染之间切换。
function DiffFileLines(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the Shiki worker supports the inferred language.
  // Shiki Worker 是否支持推断出的语言。
  highlightSupported: boolean;
  // Precomputed visual row count for this file.
  // 该文件预先计算的可视行数。
  rowCount: number;
  // Whether paired changed lines should render inline change markers.
  // 配对变更行是否应渲染行内变化标记。
  inlineDiff: boolean;
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode: DiffViewMode;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
  // Estimated line host top supplied by file-list virtualization.
  // 文件列表虚拟化提供的行宿主估算顶部。
  virtualHostTop?: number;
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行键。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行键。
  activeSearchLineKey?: string | null;
  // Triggered when a hunk header asks for a larger Git context window.
  // hunk 头部请求更大的 Git 上下文窗口时触发。
  onExpandFileContext?: (hunkIndex: number, direction: DiffHunkExpansionDirection) => void;
}) {
  if (props.rowCount < DIFF_FILE_VIRTUALIZATION_THRESHOLD) {
    return (
      <DirectDiffFileLines
        file={props.file}
        highlightSupported={props.highlightSupported}
        language={props.language}
        inlineDiff={props.inlineDiff}
        viewMode={props.viewMode}
        activeSearchLineKey={props.activeSearchLineKey}
        searchLineKeys={props.searchLineKeys}
        onExpandFileContext={props.onExpandFileContext}
      />
    );
  }
  return (
    <VirtualDiffFileLines
      file={props.file}
      highlightSupported={props.highlightSupported}
      language={props.language}
      inlineDiff={props.inlineDiff}
      viewMode={props.viewMode}
      scrollContainerRef={props.scrollContainerRef}
      scrollState={props.scrollState}
      virtualHostTop={props.virtualHostTop}
      activeSearchLineKey={props.activeSearchLineKey}
      searchLineKeys={props.searchLineKeys}
      onExpandFileContext={props.onExpandFileContext}
    />
  );
}

// DirectDiffFileLines renders one small file as a cached HTML block.
// DirectDiffFileLines 将一个小文件渲染为缓存后的 HTML 块。
function DirectDiffFileLines(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the Shiki worker supports the inferred language.
  // Shiki Worker 是否支持推断出的语言。
  highlightSupported: boolean;
  // Whether paired changed lines should render inline change markers.
  // 配对变更行是否应渲染行内变化标记。
  inlineDiff: boolean;
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode: DiffViewMode;
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行键。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行键。
  activeSearchLineKey?: string | null;
  // Triggered when a hunk header asks for a larger Git context window.
  // hunk 头部请求更大的 Git 上下文窗口时触发。
  onExpandFileContext?: (hunkIndex: number, direction: DiffHunkExpansionDirection) => void;
}) {
  const useHighlightedRenderer = Boolean(props.language && props.highlightSupported);
  const highlightRevision = useHighlightedDiffFileRevision(props.file, props.language, useHighlightedRenderer);
  const plainHtml = usePlainDiffFileHtml({
    file: props.file,
    searchLineKeys: props.searchLineKeys,
    activeSearchLineKey: props.activeSearchLineKey,
    contextControls: Boolean(props.onExpandFileContext),
    inlineDiff: props.inlineDiff,
    viewMode: props.viewMode,
  }, !useHighlightedRenderer);
  const highlightedHtml = useHighlightedDiffFileHtml({
    file: props.file,
    language: props.language ?? "text",
    highlightRevision,
    searchLineKeys: props.searchLineKeys,
    activeSearchLineKey: props.activeSearchLineKey,
    contextControls: Boolean(props.onExpandFileContext),
    inlineDiff: props.inlineDiff,
    viewMode: props.viewMode,
  }, useHighlightedRenderer);
  const html = useHighlightedRenderer ? highlightedHtml : plainHtml;
  return <DiffRowsHtml html={html} onExpandFileContext={props.onExpandFileContext} pendingStyle={diffFileLoadedRowsPlaceholderStyle(props.file)} />;
}

// areDiffFileCardPropsEqual avoids re-rendering unaffected file cards during scroll or collapse.
// areDiffFileCardPropsEqual 避免滚动或折叠时重渲染未受影响的文件卡片。
function areDiffFileCardPropsEqual(previous: DiffFileCardProps, next: DiffFileCardProps): boolean {
  if (
    previous.file !== next.file
    || previous.scrollContainerRef !== next.scrollContainerRef
    || previous.viewMode !== next.viewMode
    || previous.virtualHostTop !== next.virtualHostTop
    || previous.searchLineKeys !== next.searchLineKeys
    || previous.activeSearchLineKey !== next.activeSearchLineKey
    || previous.onLoadFileDetail !== next.onLoadFileDetail
    || previous.summarySearchMatch !== next.summarySearchMatch
  ) {
    return false;
  }
  if (fileTotalRowCount(next.file) < DIFF_FILE_VIRTUALIZATION_THRESHOLD) {
    return true;
  }
  return previous.scrollState.scrollTop === next.scrollState.scrollTop
    && previous.scrollState.clientHeight === next.scrollState.clientHeight;
}

// VirtualDiffFileLines renders only the visible rows for one large file diff.
// VirtualDiffFileLines 仅渲染大文件 diff 当前可见的行。
function VirtualDiffFileLines(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the Shiki worker supports the inferred language.
  // Shiki Worker 是否支持推断出的语言。
  highlightSupported: boolean;
  // Whether paired changed lines should render inline change markers.
  // 配对变更行是否应渲染行内变化标记。
  inlineDiff: boolean;
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode: DiffViewMode;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
  // Estimated line host top supplied by file-list virtualization.
  // 文件列表虚拟化提供的行宿主估算顶部。
  virtualHostTop?: number;
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行键。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行键。
  activeSearchLineKey?: string | null;
  // Triggered when a hunk header asks for a larger Git context window.
  // hunk 头部请求更大的 Git 上下文窗口时触发。
  onExpandFileContext?: (hunkIndex: number, direction: DiffHunkExpansionDirection) => void;
}) {
  const hostRef = React.useRef<HTMLDivElement | null>(null);
  const [hostTop, setHostTop] = React.useState(0);
  const layout = React.useMemo(() => buildDiffVirtualLayout(props.file), [props.file]);
  const effectiveHostTop = props.virtualHostTop ?? hostTop;
  // measureRevision only follows scroll changes when DOM measurement is still required.
  // measureRevision 仅在仍需 DOM 测量时跟随滚动变化。
  const measureRevision = props.virtualHostTop == null ? props.scrollState.revision : 0;
  // window contains the stable visible row window from the virtual layout cache.
  // window 保存来自虚拟布局缓存的稳定可见行窗口。
  const window = React.useMemo(
    () => mountedDiffRows(props.file, layout, props.scrollState.scrollTop - effectiveHostTop, props.scrollState.clientHeight),
    [effectiveHostTop, layout, props.file, props.scrollState.clientHeight, props.scrollState.scrollTop],
  );
  const useHighlightedRenderer = Boolean(props.language && props.highlightSupported);
  const highlightRevision = useHighlightedDiffWindowRevision(window.rows, props.language, useHighlightedRenderer);
  const plainRowsHtml = usePlainDiffRowsHtml({
    fileKey: props.file.key,
    rows: window.rows,
    searchLineKeys: props.searchLineKeys,
    activeSearchLineKey: props.activeSearchLineKey,
    contextControls: Boolean(props.onExpandFileContext),
    inlineDiff: props.inlineDiff,
    viewMode: props.viewMode,
  }, !useHighlightedRenderer);
  const highlightedRowsHtml = useHighlightedDiffRowsHtml({
    fileKey: props.file.key,
    rows: window.rows,
    language: props.language ?? "text",
    highlightRevision,
    searchLineKeys: props.searchLineKeys,
    activeSearchLineKey: props.activeSearchLineKey,
    contextControls: Boolean(props.onExpandFileContext),
    inlineDiff: props.inlineDiff,
    viewMode: props.viewMode,
  }, useHighlightedRenderer);
  const rowsHtml = useHighlightedRenderer ? highlightedRowsHtml : plainRowsHtml;

  React.useLayoutEffect(() => {
    if (props.virtualHostTop != null) {
      return undefined;
    }
    const host = hostRef.current;
    const container = props.scrollContainerRef.current;
    if (!host || !container) {
      return undefined;
    }
    // A single layout-effect measurement keeps row virtualization aligned without adding a second scroll-time layout read.
    // 单次 layout-effect 测量即可保持行虚拟化对齐，避免滚动时额外增加第二次布局读取。
    const nextTop = elementTopWithinScrollContainer(host, container);
    setHostTop((previousTop) => (Math.abs(previousTop - nextTop) > 0.5 ? nextTop : previousTop));
    return undefined;
  }, [layout.totalHeight, measureRevision, props.scrollContainerRef, props.virtualHostTop]);

  return (
    <div className="diff-virtual-lines" ref={hostRef}>
      {window.paddingTop > 0 ? <div aria-hidden="true" className="diff-virtual-spacer" style={window.topSpacerStyle} /> : null}
      <DiffRowsHtml html={rowsHtml} onExpandFileContext={props.onExpandFileContext} pendingStyle={diffMountedRowsPlaceholderStyle(window.rows)} />
      {window.paddingBottom > 0 ? <div aria-hidden="true" className="diff-virtual-spacer" style={window.bottomSpacerStyle} /> : null}
    </div>
  );
}

// DiffRowsHtml mounts cached diff rows as one HTML block.
// DiffRowsHtml 将缓存后的 diff 行作为单个 HTML 块挂载。
function DiffRowsHtml(props: {
  // Rendered HTML for the mounted row window.
  // 已挂载行窗口或整文件对应的渲染 HTML。
  html: string;
  // Stable placeholder style used while worker HTML is pending.
  // Worker HTML 等待期间使用的稳定占位样式。
  pendingStyle: React.CSSProperties;
  // Optional handler for delegated hunk context expansion controls.
  // hunk 上下文展开控件使用的可选委托处理器。
  onExpandFileContext?: (hunkIndex: number, direction: DiffHunkExpansionDirection) => void;
}) {
  const handleClick = React.useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    const target = event.target instanceof Element
      ? event.target.closest<HTMLElement>("[data-diff-hunk-action='expand-context']")
      : null;
    if (!target) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const hunkIndex = Number.parseInt(target.dataset.diffHunkIndex ?? "", 10);
    const direction = normalizedHunkExpansionDirection(target.dataset.diffHunkDirection);
    if (Number.isFinite(hunkIndex) && direction) {
      props.onExpandFileContext?.(hunkIndex, direction);
    }
  }, [props.onExpandFileContext]);
  if (!props.html && diffPlaceholderStyleHeight(props.pendingStyle) > 0) {
    return (
      <div className="diff-plain-rendered-window" onClick={handleClick}>
        <div aria-hidden="true" className="diff-render-placeholder" style={props.pendingStyle} />
      </div>
    );
  }
  return <div className="diff-plain-rendered-window" dangerouslySetInnerHTML={{ __html: props.html }} onClick={handleClick} />;
}

// normalizedHunkExpansionDirection validates delegated hunk direction data.
// normalizedHunkExpansionDirection 校验委托事件中的 hunk 方向数据。
function normalizedHunkExpansionDirection(value: string | undefined): DiffHunkExpansionDirection | null {
  if (value === "up" || value === "down" || value === "both") {
    return value;
  }
  return null;
}

// diffFileLoadedRowsHeight returns the fixed-pixel height for loaded hunks in one file.
// diffFileLoadedRowsHeight 返回单个文件已加载 hunk 的固定像素高度。
function diffFileLoadedRowsHeight(file: DiffFile): number {
  const cached = diffFileLoadedRowsHeightCache.get(file);
  if (cached != null) {
    return cached;
  }
  let height = 0;
  for (const hunk of file.hunks) {
    height += DIFF_HUNK_HEADER_HEIGHT + hunk.lines.length * DIFF_LINE_HEIGHT;
  }
  diffFileLoadedRowsHeightCache.set(file, height);
  return height;
}

// diffFileLoadedRowsPlaceholderStyle returns a stable pending placeholder style for a full file render.
// diffFileLoadedRowsPlaceholderStyle 返回整文件渲染使用的稳定等待占位样式。
function diffFileLoadedRowsPlaceholderStyle(file: DiffFile): React.CSSProperties {
  const cached = diffFileLoadedRowsPlaceholderStyleCache.get(file);
  if (cached) {
    return cached;
  }
  const style = { height: diffFileLoadedRowsHeight(file) };
  diffFileLoadedRowsPlaceholderStyleCache.set(file, style);
  return style;
}

// diffMountedRowsHeight returns the fixed-pixel height for one mounted virtual window.
// diffMountedRowsHeight 返回一个已挂载虚拟窗口的固定像素高度。
function diffMountedRowsHeight(rows: readonly DiffMountedVirtualRow[]): number {
  const cached = diffMountedRowsHeightCache.get(rows);
  if (cached != null) {
    return cached;
  }
  let height = 0;
  for (const row of rows) {
    height += row.kind === "hunk" ? DIFF_HUNK_HEADER_HEIGHT : DIFF_LINE_HEIGHT;
  }
  diffMountedRowsHeightCache.set(rows, height);
  return height;
}

// diffMountedRowsPlaceholderStyle returns a stable pending placeholder style for one mounted row window.
// diffMountedRowsPlaceholderStyle 返回单个挂载行窗口使用的稳定等待占位样式。
function diffMountedRowsPlaceholderStyle(rows: readonly DiffMountedVirtualRow[]): React.CSSProperties {
  const cached = diffMountedRowsPlaceholderStyleCache.get(rows);
  if (cached) {
    return cached;
  }
  const style = { height: diffMountedRowsHeight(rows) };
  diffMountedRowsPlaceholderStyleCache.set(rows, style);
  return style;
}

// diffPlaceholderStyleHeight reads the numeric height from a cached placeholder style.
// diffPlaceholderStyleHeight 从缓存占位样式中读取数字高度。
function diffPlaceholderStyleHeight(style: React.CSSProperties): number {
  return typeof style.height === "number" ? style.height : 0;
}

// useDiffScrollState tracks the scroll viewport for the diff file list.
// useDiffScrollState 跟踪 diff 文件列表的滚动视口。
export function useDiffScrollState(
  // Scrollable diff file-list element reference.
  // 可滚动 diff 文件列表元素引用。
  containerRef: React.RefObject<HTMLDivElement | null>,
): [DiffScrollState, React.UIEventHandler<HTMLDivElement>, () => void] {
  const frameRef = React.useRef<number | null>(null);
  const [state, setState] = React.useState<DiffScrollState>({ scrollTop: 0, clientHeight: 0, revision: 0 });

  const updateState = React.useCallback(() => {
    const element = containerRef.current;
    if (!element) {
      return;
    }
    setState((previous) => {
      const nextScrollTop = quantizedDiffScrollTop(element.scrollTop);
      const nextClientHeight = quantizedDiffClientHeight(element.clientHeight);
      if (previous.scrollTop === nextScrollTop && previous.clientHeight === nextClientHeight) {
        return previous;
      }
      return {
        scrollTop: nextScrollTop,
        clientHeight: nextClientHeight,
        revision: previous.revision + 1,
      };
    });
  }, [containerRef]);

  const scheduleUpdate = React.useCallback(() => {
    if (frameRef.current != null) {
      return;
    }
    frameRef.current = window.requestAnimationFrame(() => {
      frameRef.current = null;
      updateState();
    });
  }, [updateState]);

  React.useLayoutEffect(() => {
    updateState();
    const element = containerRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return undefined;
    }
    const observer = new ResizeObserver(scheduleUpdate);
    observer.observe(element);
    scheduleUpdate();
    return () => {
      observer.disconnect();
      if (frameRef.current != null) {
        window.cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
    };
  }, [containerRef, scheduleUpdate, updateState]);

  return [state, scheduleUpdate, updateState];
}

// quantizedDiffScrollTop coalesces pixel-level scroll changes while keeping mounted windows conservative.
// quantizedDiffScrollTop 合并像素级滚动变化，同时保持挂载窗口保守覆盖。
function quantizedDiffScrollTop(scrollTop: number): number {
  return Math.floor(Math.max(0, scrollTop) / DIFF_SCROLL_STATE_GRANULARITY_PX) * DIFF_SCROLL_STATE_GRANULARITY_PX;
}

// quantizedDiffClientHeight expands viewport height to avoid under-mounting after resize rounding.
// quantizedDiffClientHeight 放大视口高度，避免 resize 取整后挂载不足。
function quantizedDiffClientHeight(clientHeight: number): number {
  if (clientHeight <= 0) {
    return 0;
  }
  return Math.ceil(clientHeight / DIFF_SCROLL_STATE_GRANULARITY_PX) * DIFF_SCROLL_STATE_GRANULARITY_PX;
}

// displayPath returns the primary file path shown in the file header.
// displayPath 返回文件头中展示的主路径。
function displayPath(file: DiffFile): string {
  return file.newPath && file.newPath !== "/dev/null" ? file.newPath : file.oldPath;
}

// nextExpandedContextLineCount returns the next bounded Git context size for one file.
// nextExpandedContextLineCount 返回单个文件下一档受限 Git 上下文大小。
function nextExpandedContextLineCount(file: DiffFile): number {
  const currentContextLineCount = file.contextLineCount ?? DIFF_DEFAULT_CONTEXT_LINES;
  return DIFF_CONTEXT_EXPANSION_STEPS.find((step) => step > currentContextLineCount)
    ?? DIFF_CONTEXT_EXPANSION_STEPS[DIFF_CONTEXT_EXPANSION_STEPS.length - 1];
}

// canExpandDiffFileContext returns whether another larger Git context step exists.
// canExpandDiffFileContext 返回是否还存在更大的 Git 上下文档位。
function canExpandDiffFileContext(file: DiffFile): boolean {
  const currentContextLineCount = file.contextLineCount ?? DIFF_DEFAULT_CONTEXT_LINES;
  return DIFF_CONTEXT_EXPANSION_STEPS.some((step) => step > currentContextLineCount);
}
