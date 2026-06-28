import { escapeHtml } from "./diff-shiki-shared";
import type { DiffHunkExpansionDirection, DiffHunkSeparatorStyle } from "./types";

// DIFF_HUNK_HEADER_HTML_CACHE_LIMIT caps repeated hunk separator fragments.
// DIFF_HUNK_HEADER_HTML_CACHE_LIMIT 限制重复 hunk 分隔片段缓存数量。
const DIFF_HUNK_HEADER_HTML_CACHE_LIMIT = 4096;

// diffHunkHeaderHtmlCache stores stable hunk separator HTML across virtual remounts.
// diffHunkHeaderHtmlCache 缓存虚拟重挂载期间稳定的 hunk 分隔 HTML。
const diffHunkHeaderHtmlCache = new Map<string, string>();

// TargetHunkSeparatorState carries edge-aware expansion controls for one target separator.
// TargetHunkSeparatorState 为单个目标分隔条承载感知边界的展开控件状态。
export interface TargetHunkSeparatorState {
  // Number of collapsed unchanged lines represented by the separator.
  // 当前分隔条代表的折叠未修改行数。
  collapsedLines?: number;
  // Whether this separator is before the first hunk.
  // 当前分隔条是否位于首个 hunk 之前。
  isFirstHunk?: boolean;
  // Whether this separator is after the last hunk.
  // 当前分隔条是否位于最后一个 hunk 之后。
  isLastHunk?: boolean;
  // Whether this separator can request more context from the backend.
  // 当前分隔条是否可以请求更多上下文。
  isExpandable?: boolean;
  // Hunk index used for upward expansion.
  // 用于向上展开的 hunk 索引。
  expandUpHunkIndex?: number;
  // Hunk index used for downward expansion.
  // 用于向下展开的 hunk 索引。
  expandDownHunkIndex?: number;
}

// hunkSeparatorControlsHtml renders separator controls for the gutter cell.
// hunkSeparatorControlsHtml 为行号 gutter 单元格渲染分隔条控件。
export function hunkSeparatorControlsHtml(
  header: string,
  hunkIndex: number,
  expandIndex: number | undefined,
  separatorStyle: DiffHunkSeparatorStyle = "line-info",
  separatorState: TargetHunkSeparatorState = {},
): string {
  const chunked = (separatorState.collapsedLines ?? parsedUnmodifiedLineCount(header) ?? 0) > 100;
  const cacheKey = `controls\u0000${separatorStyle}\u0000${expandIndex ?? "p"}\u0000${chunked ? "chunked" : "single"}\u0000${separatorState.isFirstHunk ? "first" : ""}\u0000${separatorState.isLastHunk ? "last" : ""}\u0000${separatorState.expandUpHunkIndex ?? "u"}\u0000${separatorState.expandDownHunkIndex ?? "d"}\u0000${hunkIndex}\u0000${header}`;
  const cached = diffHunkHeaderHtmlCache.get(cacheKey);
  if (cached != null) {
    return cached;
  }
  if (separatorStyle === "simple") {
    rememberDiffHunkHeaderHtml(cacheKey, "");
    return "";
  }
  const controls = targetSeparatorControlsHtml(expandIndex, chunked, separatorState);
  rememberDiffHunkHeaderHtml(cacheKey, controls);
  return controls;
}

// hunkSeparatorLabelHtml renders separator text for the content cell.
// hunkSeparatorLabelHtml 为内容单元格渲染分隔条文本。
export function hunkSeparatorLabelHtml(
  header: string,
  hunkIndex: number,
  expandIndex: number | undefined,
  separatorStyle: DiffHunkSeparatorStyle = "line-info",
  separatorState: TargetHunkSeparatorState = {},
): string {
  const cacheKey = `label\u0000${separatorStyle}\u0000${expandIndex ?? "p"}\u0000${separatorState.expandUpHunkIndex ?? "u"}\u0000${separatorState.expandDownHunkIndex ?? "d"}\u0000${hunkIndex}\u0000${header}`;
  const cached = diffHunkHeaderHtmlCache.get(cacheKey);
  if (cached != null) {
    return cached;
  }
  if (separatorStyle === "simple") {
    rememberDiffHunkHeaderHtml(cacheKey, "");
    return "";
  }
  const expandUpAttribute = expandIndex != null && separatorState.expandUpHunkIndex != null ? ` data-expand-up-index="${separatorState.expandUpHunkIndex}"` : "";
  const expandDownAttribute = expandIndex != null && separatorState.expandDownHunkIndex != null ? ` data-expand-down-index="${separatorState.expandDownHunkIndex}"` : "";
  const clickableAttribute = expandIndex != null ? ` data-unmodified-lines="" role="button"` : "";
  const label = `<div data-separator-content=""${clickableAttribute}${expandUpAttribute}${expandDownAttribute}>${escapeHtml(header)}</div>`;
  rememberDiffHunkHeaderHtml(cacheKey, label);
  return label;
}

// targetSeparatorControlsHtml mirrors target gutter-side separator controls.
// targetSeparatorControlsHtml 复刻目标行号侧分隔条控件。
function targetSeparatorControlsHtml(
  expandIndex: number | undefined,
  chunked: boolean,
  separatorState: TargetHunkSeparatorState,
): string {
  let buttonCount = 0;
  let buttonsHtml = "";
  if (expandIndex != null) {
    if (separatorState.expandUpHunkIndex != null) {
      buttonsHtml += targetExpandButtonHtml("up", separatorState.expandUpHunkIndex);
      buttonCount += 1;
    }
    if (separatorState.expandDownHunkIndex != null) {
      buttonsHtml += targetExpandButtonHtml("down", separatorState.expandDownHunkIndex);
      buttonCount += 1;
    }
  }
  const chunkedAttribute = chunked ? ` data-separator-chunked=""` : "";
  const expandUpAttribute = expandIndex != null && separatorState.expandUpHunkIndex != null ? ` data-expand-up-index="${separatorState.expandUpHunkIndex}"` : "";
  const expandDownAttribute = expandIndex != null && separatorState.expandDownHunkIndex != null ? ` data-expand-down-index="${separatorState.expandDownHunkIndex}"` : "";
  if (!buttonsHtml) {
    return "";
  }
  return `<div data-separator-wrapper="" data-separator-controls=""${buttonCount > 1 ? ` data-separator-multi-button=""` : ""}${chunkedAttribute}${expandUpAttribute}${expandDownAttribute}>${buttonsHtml}</div>`;
}

// targetExpandButtonHtml mirrors target q for line-info expansion buttons.
// targetExpandButtonHtml 复刻目标 q 的 line-info 展开按钮。
function targetExpandButtonHtml(direction: DiffHunkExpansionDirection, hunkIndex?: number): string {
  const dataAttribute = direction === "both" ? "data-expand-both" : direction === "up" ? "data-expand-up" : "data-expand-down";
  const indexAttribute = hunkIndex != null ? ` data-expand-hunk-index="${hunkIndex}"` : "";
  return `<div role="button" data-expand-button="" ${dataAttribute}=""${indexAttribute}>${targetExpandIconHtml(direction === "both" ? "all" : "single")}</div>`;
}

// targetExpandIconHtml emits the target expand SVG paths without relying on a global sprite.
// targetExpandIconHtml 输出目标展开 SVG 路径，避免依赖全局 sprite。
function targetExpandIconHtml(kind: "single" | "all"): string {
  if (kind === "all") {
    return `<svg data-icon="" aria-hidden="true" width="16" height="16" viewBox="0 0 16 16"><path d="M11.47 9.47a.75.75 0 1 1 1.06 1.06l-4 4a.75.75 0 0 1-1.06 0l-4-4a.75.75 0 1 1 1.06-1.06L8 12.94zM7.526 1.418a.75.75 0 0 1 1.004.052l4 4a.75.75 0 1 1-1.06 1.06L8 3.06 4.53 6.53a.75.75 0 1 1-1.06-1.06l4-4z"></path></svg>`;
  }
  return `<svg data-icon="" aria-hidden="true" width="16" height="16" viewBox="0 0 16 16"><path d="M3.47 5.47a.75.75 0 0 1 1.06 0L8 8.94l3.47-3.47a.75.75 0 1 1 1.06 1.06l-4 4a.75.75 0 0 1-1.06 0l-4-4a.75.75 0 0 1 0-1.06"></path></svg>`;
}

// parsedUnmodifiedLineCount extracts the unchanged-line count from target separator text.
// parsedUnmodifiedLineCount 从目标分隔行文本中提取未修改行数。
export function parsedUnmodifiedLineCount(header: string): number | undefined {
  const match = /^(\d+)\s+unmodified\s+lines?$/i.exec(header.trim());
  return match ? Number.parseInt(match[1], 10) : undefined;
}

// rememberDiffHunkHeaderHtml stores one hunk separator fragment with FIFO eviction.
// rememberDiffHunkHeaderHtml 使用 FIFO 淘汰策略保存一个 hunk 分隔片段。
function rememberDiffHunkHeaderHtml(cacheKey: string, html: string): void {
  if (diffHunkHeaderHtmlCache.has(cacheKey)) {
    diffHunkHeaderHtmlCache.delete(cacheKey);
  } else if (diffHunkHeaderHtmlCache.size >= DIFF_HUNK_HEADER_HTML_CACHE_LIMIT) {
    const oldestKey = diffHunkHeaderHtmlCache.keys().next().value;
    if (oldestKey) {
      diffHunkHeaderHtmlCache.delete(oldestKey);
    }
  }
  diffHunkHeaderHtmlCache.set(cacheKey, html);
}
