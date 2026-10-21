import React from "react";

import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n-messages";
import type { DiffFile, WorkspaceDiffSearchFileMatch } from "../types";
import {
  DIFF_HUNK_HEADER_HEIGHT,
  DIFF_LINE_HEIGHT,
  DIFF_SUMMARY_PLACEHOLDER_MIN_HEIGHT,
} from "../limits";

// DIFF_SUMMARY_PLACEHOLDER_MAX_HUNKS caps rendered summary-only hunk skeleton nodes.
// DIFF_SUMMARY_PLACEHOLDER_MAX_HUNKS 限制 summary-only hunk 骨架节点数量。
const DIFF_SUMMARY_PLACEHOLDER_MAX_HUNKS = 24;

// DIFF_SUMMARY_PLACEHOLDER_MAX_LINES_PER_HUNK caps rendered skeleton lines per hunk.
// DIFF_SUMMARY_PLACEHOLDER_MAX_LINES_PER_HUNK 限制每个 hunk 的骨架行数量。
const DIFF_SUMMARY_PLACEHOLDER_MAX_LINES_PER_HUNK = 6;

// DiffFileLoadStateNote displays lazy detail hydration state for summary-only files.
// DiffFileLoadStateNote 展示摘要文件的懒加载详情水合状态。
export function DiffFileLoadStateNote(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Optional backend search hit shown before full detail hydration finishes.
  // 完整详情水合完成前展示的可选后端搜索命中。
  summarySearchMatch?: WorkspaceDiffSearchFileMatch;
  // Optional retry action for failed or timed-out loads.
  // 失败或超时加载对应的可选重试动作。
  onRetry?: (file: DiffFile) => void;
  // Optional action used to reveal backend-skipped files on disk.
  // 用于在磁盘中定位后端跳过文件的可选动作。
  onRevealFile?: (file: DiffFile) => void;
}) {
  const { t } = useI18n();
  const loadState = props.file.fileDiffLoadState;
  const status = loadState?.status ?? "idle";
  const message = loadState?.message ?? diffFileLoadStateMessage(status, t);
  const metrics = props.file.tooLargeToDisplay ? null : diffSummaryPlaceholderMetrics(props.file, props.summarySearchMatch);
  // canRetry exposes manual recovery only for terminal lazy-load states.
  // canRetry 仅为终止态懒加载状态提供手动恢复入口。
  const canRetry = Boolean(props.onRetry) && (status === "failed" || status === "timed-out");
  // canReveal exposes the host file manager when the backend intentionally skipped the file body.
  // canReveal 在后端主动跳过文件正文时展示宿主文件管理器入口。
  const canReveal = Boolean(props.onRevealFile && props.file.isPartial && props.file.hunks.length === 0);
  return (
    <div className={`diff-summary-placeholder diff-file-load-state ${status}${props.file.tooLargeToDisplay ? " too-large" : ""}`} style={metrics?.style}>
      {metrics ? (
        <div className="diff-summary-placeholder-canvas" aria-hidden="true">
          {renderSummaryPlaceholderHunks(metrics.hunks)}
        </div>
      ) : null}
      <div className="diff-summary-placeholder-note">
        <span>{message}</span>
        {metrics && (status === "idle" || status === "loading") ? (
          <small>{formatDiffLoadTranslation(t, "diff.load.rowsPending", { rows: String(metrics.rowCount), hunks: String(metrics.hunkCount) })}</small>
        ) : null}
        {canRetry ? (
          <button className="diff-file-load-retry" onClick={() => props.onRetry?.(props.file)} type="button">
            {t("diff.load.retry")}
          </button>
        ) : null}
        {canReveal ? (
          <button className="diff-file-load-retry" onClick={() => props.onRevealFile?.(props.file)} type="button">
            {t("workspaceFiles.revealInFolder")}
          </button>
        ) : null}
      </div>
      {props.summarySearchMatch ? (
        <div
          className="diff-file-search-placeholder diff-summary-search-marker"
          data-diff-summary-search-file-key={props.summarySearchMatch.key}
          style={metrics?.searchStyle}
        >
          <strong>{diffSummarySearchMatchLocation(props.summarySearchMatch, t)}</strong>
          <div className="diff-summary-search-previews">
            {renderSummarySearchPreviews(props.summarySearchMatch, t)}
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
function renderSummarySearchPreviews(match: WorkspaceDiffSearchFileMatch, t: (key: TranslationKey) => string): React.ReactNode[] {
  const previews = diffSummarySearchMatchPreviews(match, t);
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
function diffSummarySearchMatchLocation(match: WorkspaceDiffSearchFileMatch, t: (key: TranslationKey) => string): string {
  const parts = [formatDiffLoadTranslation(t, "diff.load.matchCount", { count: String(match.matchCount) })];
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
function diffSummarySearchMatchPreviews(match: WorkspaceDiffSearchFileMatch, t: (key: TranslationKey) => string): string[] {
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
  return previews.length ? previews : [t("diff.load.backendMatchFallback")];
}

// diffFileLoadStateMessage returns a user-facing lazy detail status message.
// diffFileLoadStateMessage 返回面向用户的懒加载详情状态文案。
function diffFileLoadStateMessage(status: NonNullable<DiffFile["fileDiffLoadState"]>["status"], t: (key: TranslationKey) => string): string {
  if (status === "loading") {
    return t("diff.load.loadingDetail");
  }
  if (status === "timed-out") {
    return t("diff.load.timeout");
  }
  if (status === "failed") {
    return t("diff.load.failed");
  }
  if (status === "loaded") {
    return t("diff.load.loadedEmpty");
  }
  return t("diff.load.summaryIdle");
}

// formatDiffLoadTranslation applies small named placeholders for load-state labels.
// formatDiffLoadTranslation 为加载状态标签替换少量具名占位符。
function formatDiffLoadTranslation(t: (key: TranslationKey) => string, key: TranslationKey, values: Record<string, string>): string {
  let text = t(key);
  for (const [name, value] of Object.entries(values)) {
    text = text.replace(`{${name}}`, value);
  }
  return text;
}
