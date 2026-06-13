import { diffSearchLineKey } from "./diff-search";
import {
  escapeHtml,
  tokenSpansToHtml,
  type DiffShikiTokenSpan,
} from "./diff-shiki-shared";
import type { DiffFile, DiffInlineChangeRange, DiffLine, DiffLineKind, DiffViewMode } from "./types";

// EMPTY_INLINE_CHANGE_RANGES reuses one immutable empty inline-range list for rows without inline changes.
// EMPTY_INLINE_CHANGE_RANGES 为没有行内变化的行复用一个不可变空范围列表。
const EMPTY_INLINE_CHANGE_RANGES: readonly DiffInlineChangeRange[] = Object.freeze([]);

// normalizedInlineChangeRangesCache stores clamped inline ranges per stable diff line object.
// normalizedInlineChangeRangesCache 按稳定 diff 行对象缓存已约束的行内变化范围。
const normalizedInlineChangeRangesCache = new WeakMap<DiffLine, readonly DiffInlineChangeRange[]>();

// tokenInlineDiffHtmlCache stores inline-marked token HTML for stable spans and range arrays.
// tokenInlineDiffHtmlCache 按稳定 token span 与范围数组缓存已标记 inline diff 的 HTML。
const tokenInlineDiffHtmlCache = new WeakMap<readonly DiffShikiTokenSpan[], WeakMap<readonly DiffInlineChangeRange[], string>>();

// plainInlineDiffHtmlCache stores inline-marked plain HTML for stable parsed diff lines.
// plainInlineDiffHtmlCache 按稳定解析 diff 行缓存已标记 inline diff 的纯文本 HTML。
const plainInlineDiffHtmlCache = new WeakMap<DiffLine, string>();

// escapedDiffLineContentCache stores escaped plain text for stable parsed diff lines.
// escapedDiffLineContentCache 按稳定解析 diff 行缓存已转义的纯文本内容。
const escapedDiffLineContentCache = new WeakMap<DiffLine, string>();

// diffUnifiedLineNumberCache stores gutter line numbers for stable parsed diff lines.
// diffUnifiedLineNumberCache 按稳定解析 diff 行缓存统一视图 gutter 行号。
const diffUnifiedLineNumberCache = new WeakMap<DiffLine, string>();

// diffOldLineNumberCache stores old-side split line numbers for stable parsed diff lines.
// diffOldLineNumberCache 按稳定解析 diff 行缓存 split 旧侧行号。
const diffOldLineNumberCache = new WeakMap<DiffLine, string>();

// diffNewLineNumberCache stores new-side split line numbers for stable parsed diff lines.
// diffNewLineNumberCache 按稳定解析 diff 行缓存 split 新侧行号。
const diffNewLineNumberCache = new WeakMap<DiffLine, string>();

// DIFF_LINE_BASE_CLASS_NAMES stores search-free row class names by diff line kind.
// DIFF_LINE_BASE_CLASS_NAMES 按 diff 行类型保存无搜索状态的基础行 class。
const DIFF_LINE_BASE_CLASS_NAMES: Readonly<Record<DiffLineKind, string>> = Object.freeze({
  added: "diff-line-row added",
  removed: "diff-line-row removed",
  context: "diff-line-row context",
  metadata: "diff-line-row metadata",
});

// DiffLineSearchClassSet stores row classes for none, match, active, and match-active states.
// DiffLineSearchClassSet 保存普通、命中、活跃、命中且活跃四种行 class。
type DiffLineSearchClassSet = readonly [string, string, string, string];

// DIFF_LINE_SEARCH_CLASS_NAMES stores prebuilt row classes for search and active-search states.
// DIFF_LINE_SEARCH_CLASS_NAMES 保存搜索命中与活跃搜索状态的预构建行 class。
const DIFF_LINE_SEARCH_CLASS_NAMES = Object.freeze({
  added: Object.freeze([
    "diff-line-row added",
    "diff-line-row added search-match",
    "diff-line-row added search-active",
    "diff-line-row added search-match search-active",
  ] as const),
  removed: Object.freeze([
    "diff-line-row removed",
    "diff-line-row removed search-match",
    "diff-line-row removed search-active",
    "diff-line-row removed search-match search-active",
  ] as const),
  context: Object.freeze([
    "diff-line-row context",
    "diff-line-row context search-match",
    "diff-line-row context search-active",
    "diff-line-row context search-match search-active",
  ] as const),
  metadata: Object.freeze([
    "diff-line-row metadata",
    "diff-line-row metadata search-match",
    "diff-line-row metadata search-active",
    "diff-line-row metadata search-match search-active",
  ] as const),
} satisfies Record<DiffLineKind, DiffLineSearchClassSet>);

// DiffLineRowAttributesCacheEntry stores immutable data attributes for one rendered line key.
// DiffLineRowAttributesCacheEntry 保存单个渲染行键对应的不可变 data 属性。
interface DiffLineRowAttributesCacheEntry {
  // Stable line key used to validate this cached attribute string.
  // 用于校验当前缓存属性字符串的稳定行键。
  lineKey: string;
  // Complete data-* attribute string for the rendered row.
  // 渲染行对应的完整 data-* 属性字符串。
  attributes: string;
}

// diffLineRowAttributesCache stores repeated row data attributes for stable parsed diff lines.
// diffLineRowAttributesCache 按稳定解析 diff 行缓存重复使用的行 data 属性。
const diffLineRowAttributesCache = new WeakMap<DiffLine, DiffLineRowAttributesCacheEntry>();

// diffStaticLineHtmlCache stores search-free plain line HTML for stable parsed diff lines.
// diffStaticLineHtmlCache 按稳定解析 diff 行缓存无搜索态的纯文本行 HTML。
const diffStaticLineHtmlCache = new WeakMap<DiffLine, Map<string, string>>();

// DIFF_HUNK_HEADER_HTML_CACHE_LIMIT caps repeated hunk header HTML fragments.
// DIFF_HUNK_HEADER_HTML_CACHE_LIMIT 限制重复 hunk header HTML 片段缓存数量。
const DIFF_HUNK_HEADER_HTML_CACHE_LIMIT = 4096;

// diffHunkHeaderHtmlCache stores stable hunk header HTML across virtual remounts.
// diffHunkHeaderHtmlCache 缓存虚拟重挂载期间稳定的 hunk header HTML。
const diffHunkHeaderHtmlCache = new Map<string, string>();

// DiffPlainRenderSearchState carries dynamic search classes for plain HTML rendering.
// DiffPlainRenderSearchState 承载纯 HTML 渲染时使用的动态搜索 class。
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

// renderDiffFileHtmlCore renders one complete file as HTML.
// renderDiffFileHtmlCore 将单个完整文件渲染为 HTML。
function renderDiffFileHtmlCore(options: DiffPlainFileRenderOptions & DiffHighlightedRenderState): string {
  let html = "";
  for (let hunkIndex = 0; hunkIndex < options.file.hunks.length; hunkIndex += 1) {
    const hunk = options.file.hunks[hunkIndex];
    html += `<section class="diff-hunk">${renderHunkHeaderHtml(hunk.header, hunkIndex, options.contextControls)}<div class="diff-lines">`;
    for (let lineIndex = 0; lineIndex < hunk.lines.length; lineIndex += 1) {
      html += renderLineHtml({
        fileKey: options.file.key,
        hunkIndex,
        lineIndex,
        line: hunk.lines[lineIndex],
        searchLineKeys: options.searchLineKeys,
        activeSearchLineKey: options.activeSearchLineKey,
        highlightedLineSpans: options.highlightedLineSpans,
        inlineDiff: options.inlineDiff,
        viewMode: options.viewMode,
      });
    }
    html += "</div></section>";
  }
  return html;
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

// renderDiffVirtualRowsHtmlCore renders a mounted virtual row range as HTML.
// renderDiffVirtualRowsHtmlCore 将已挂载虚拟行范围渲染为 HTML。
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
  let html = "";
  const safeStartIndex = Math.max(0, Math.min(startIndex, options.rows.length));
  const safeEndIndex = Math.max(safeStartIndex, Math.min(endIndex, options.rows.length));
  for (let rowIndex = safeStartIndex; rowIndex < safeEndIndex; rowIndex += 1) {
    const row = options.rows[rowIndex];
    if (row.kind === "hunk") {
      html += renderHunkHeaderHtml(row.header, row.hunkIndex, options.contextControls, true);
      continue;
    }
    html += renderLineHtml({
      fileKey: options.fileKey,
      hunkIndex: row.hunkIndex,
      lineIndex: row.lineIndex,
      lineKey: row.lineKey,
      line: row.line,
      searchLineKeys: options.searchLineKeys,
      activeSearchLineKey: options.activeSearchLineKey,
      highlightedLineSpans: options.highlightedLineSpans,
      inlineDiff: options.inlineDiff,
      viewMode: options.viewMode,
    });
  }
  return html;
}

// renderHunkHeaderHtml renders one hunk header with optional delegated controls.
// renderHunkHeaderHtml 渲染单个 hunk 头部，并按需包含委托事件控件。
function renderHunkHeaderHtml(header: string, hunkIndex: number, contextControls = false, virtual = false): string {
  const cacheKey = hunkHeaderHtmlCacheKey(header, hunkIndex, contextControls, virtual);
  const cached = diffHunkHeaderHtmlCache.get(cacheKey);
  if (cached != null) {
    return cached;
  }
  const headerText = `<span class="diff-hunk-header-text">${escapeHtml(header)}</span>`;
  // hunkAttributes exposes stable row identity for delegated actions and future split mapping.
  // hunkAttributes 为委托事件和后续 split 映射暴露稳定行身份。
  const hunkAttributes = `data-diff-row-kind="hunk" data-diff-hunk-index="${hunkIndex}"`;
  let html: string;
  if (!contextControls) {
    html = `<div class="diff-hunk-header${virtual ? " virtual" : ""}" ${hunkAttributes}>${headerText}</div>`;
    rememberDiffHunkHeaderHtml(cacheKey, html);
    return html;
  }
  const controlsHtml = hunkExpansionButtonHtml(hunkIndex, "up", "向上展开上下文", "上")
    + hunkExpansionButtonHtml(hunkIndex, "down", "向下展开上下文", "下")
    + hunkExpansionButtonHtml(hunkIndex, "both", "双向展开上下文", "双向");
  html = `<div class="diff-hunk-header${virtual ? " virtual" : ""}" ${hunkAttributes}>${headerText}<span class="diff-hunk-expand-controls">${controlsHtml}</span></div>`;
  rememberDiffHunkHeaderHtml(cacheKey, html);
  return html;
}

// hunkHeaderHtmlCacheKey builds a stable key for one hunk header render mode.
// hunkHeaderHtmlCacheKey 为单个 hunk header 渲染模式构建稳定缓存键。
function hunkHeaderHtmlCacheKey(header: string, hunkIndex: number, contextControls: boolean, virtual: boolean): string {
  return `${virtual ? "v" : "f"}\u0000${contextControls ? "c" : "p"}\u0000${hunkIndex}\u0000${header}`;
}

// rememberDiffHunkHeaderHtml stores one hunk header fragment with FIFO eviction.
// rememberDiffHunkHeaderHtml 使用 FIFO 淘汰策略保存一个 hunk header 片段。
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

// hunkExpansionButtonHtml renders one delegated hunk expansion command.
// hunkExpansionButtonHtml 渲染单个委托式 hunk 展开命令。
function hunkExpansionButtonHtml(hunkIndex: number, direction: "up" | "down" | "both", title: string, label: string): string {
  return `<button class="diff-hunk-expand-button ${direction}" data-diff-hunk-action="expand-context" data-diff-hunk-direction="${direction}" data-diff-hunk-index="${hunkIndex}" title="${escapeHtml(title)}" type="button">${escapeHtml(label)}</button>`;
}

// renderLineHtml renders one escaped or highlighted diff line with search classes.
// renderLineHtml 渲染一条带搜索 class 的转义或高亮 diff 行。
function renderLineHtml(options: {
  // Stable owning file key.
  // 稳定的所属文件 key。
  fileKey: string;
  // Source hunk index.
  // 源 hunk 索引。
  hunkIndex: number;
  // Source line index inside the hunk.
  // hunk 内源行索引。
  lineIndex: number;
  // Stable search/navigation line key when precomputed by the caller.
  // 调用方预计算出的稳定搜索与导航行键。
  lineKey?: string;
  // Parsed diff line.
  // 已解析 diff 行。
  line: DiffLine;
  // Search result line keys visible on the current result page.
  // 当前结果页可见的搜索命中行 key。
  searchLineKeys?: ReadonlySet<string>;
  // Active search result line key.
  // 当前活跃搜索命中行 key。
  activeSearchLineKey?: string | null;
  // Highlighted token spans keyed by stable line key.
  // 按稳定行 key 索引的高亮 token span。
  highlightedLineSpans?: ReadonlyMap<string, DiffShikiTokenSpan[]>;
  // Whether inline diff ranges should be rendered.
  // 是否渲染行内 diff 范围。
  inlineDiff?: boolean;
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode?: DiffViewMode;
}): string {
  const lineKey = options.lineKey ?? diffSearchLineKey(options.fileKey, options.hunkIndex, options.lineIndex);
  const useStaticHtml = shouldUseStaticDiffLineHtml(options);
  if (useStaticHtml) {
    const cachedStaticHtml = staticDiffLineHtml(options.line, lineKey, options.viewMode);
    if (cachedStaticHtml != null) {
      return cachedStaticHtml;
    }
  }
  const classes = plainLineClassName(options.line.kind, lineKey, options);
  const contentHtml = lineContentHtml(options.line, lineKey, options.highlightedLineSpans, options.inlineDiff);
  // rowAttributes keeps DOM-mounted rows queryable without reconstructing parser indexes.
  // rowAttributes 让已挂载 DOM 行可被查询定位，无需重新构造解析索引。
  const rowAttributes = lineRowAttributes(lineKey, options);
  if (options.viewMode === "split") {
    const html = renderSplitLineHtml(options.line, classes, rowAttributes, contentHtml);
    rememberStaticDiffLineHtml(options.line, lineKey, options.viewMode, html, useStaticHtml);
    return html;
  }
  const lineNumberHtml = formatLineNumber(options.line);
  const html = `<div class="${classes}" ${rowAttributes}><span class="diff-line-number">${lineNumberHtml}</span><code>${contentHtml}</code></div>`;
  rememberStaticDiffLineHtml(options.line, lineKey, options.viewMode, html, useStaticHtml);
  return html;
}

// staticDiffLineHtml reads cached plain line HTML when no dynamic render state is active.
// staticDiffLineHtml 在没有动态渲染状态时读取缓存的纯文本行 HTML。
function staticDiffLineHtml(
  // Parsed diff line used as the weak cache owner.
  // 作为弱缓存宿主的已解析 diff 行。
  line: DiffLine,
  // Stable DOM/search key for this rendered line.
  // 当前渲染行的稳定 DOM/搜索 key。
  lineKey: string,
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode: DiffViewMode | undefined,
): string | null {
  return diffStaticLineHtmlCache.get(line)?.get(staticDiffLineHtmlCacheKey(lineKey, viewMode)) ?? null;
}

// rememberStaticDiffLineHtml stores plain line HTML only when dynamic state is absent.
// rememberStaticDiffLineHtml 仅在动态状态不存在时保存纯文本行 HTML。
function rememberStaticDiffLineHtml(
  // Parsed diff line used as the weak cache owner.
  // 作为弱缓存宿主的已解析 diff 行。
  line: DiffLine,
  // Stable DOM/search key for this rendered line.
  // 当前渲染行的稳定 DOM/搜索 key。
  lineKey: string,
  // Visual layout mode used for rendered diff rows.
  // 渲染 diff 行使用的视觉布局模式。
  viewMode: DiffViewMode | undefined,
  // Rendered HTML fragment to reuse.
  // 要复用的已渲染 HTML 片段。
  html: string,
  // Whether this row has no dynamic state and may be cached.
  // 当前行是否没有动态状态并允许缓存。
  useStaticHtml: boolean,
): void {
  if (!useStaticHtml) {
    return;
  }
  let htmlByMode = diffStaticLineHtmlCache.get(line);
  if (!htmlByMode) {
    htmlByMode = new Map();
    diffStaticLineHtmlCache.set(line, htmlByMode);
  }
  htmlByMode.set(staticDiffLineHtmlCacheKey(lineKey, viewMode), html);
}

// shouldUseStaticDiffLineHtml returns whether a row has no dynamic search, highlight, or inline state.
// shouldUseStaticDiffLineHtml 返回一行是否没有动态搜索、高亮或行内状态。
function shouldUseStaticDiffLineHtml(options: DiffPlainRenderSearchState & DiffHighlightedRenderState): boolean {
  return !options.activeSearchLineKey
    && !options.searchLineKeys?.size
    && !options.highlightedLineSpans
    && !options.inlineDiff;
}

// staticDiffLineHtmlCacheKey separates unified and split static row HTML for one stable line key.
// staticDiffLineHtmlCacheKey 为同一稳定行 key 区分统一视图与分栏视图静态行 HTML。
function staticDiffLineHtmlCacheKey(lineKey: string, viewMode: DiffViewMode | undefined): string {
  return `${viewMode ?? "unified"}\u0000${lineKey}`;
}

// lineRowAttributes builds stable row data attributes without per-line array allocation.
// lineRowAttributes 构建稳定行数据属性，避免每行创建临时数组。
function lineRowAttributes(
  // Stable line key shared with search/navigation.
  // 与搜索和导航共享的稳定行 key。
  lineKey: string,
  // Render options carrying hunk and line coordinates.
  // 携带 hunk 与行坐标的渲染选项。
  options: {
    // Source hunk index.
    // 源 hunk 索引。
    hunkIndex: number;
    // Source line index inside the hunk.
    // hunk 内源行索引。
    lineIndex: number;
    // Parsed diff line.
    // 已解析 diff 行。
    line: DiffLine;
  },
): string {
  const cached = diffLineRowAttributesCache.get(options.line);
  if (cached?.lineKey === lineKey) {
    return cached.attributes;
  }
  // lineKey is produced by diffSearchLineKey and is already URI-encoded for safe DOM attributes.
  // lineKey 由 diffSearchLineKey 生成，已经 URI 编码，可安全用于 DOM 属性。
  let attributes = `data-diff-line-key="${lineKey}" data-diff-row-kind="line" data-diff-hunk-index="${options.hunkIndex}" data-diff-line-index="${options.lineIndex}"`;
  if (options.line.oldLine != null) {
    attributes += ` data-diff-old-line="${options.line.oldLine}"`;
  }
  if (options.line.newLine != null) {
    attributes += ` data-diff-new-line="${options.line.newLine}"`;
  }
  diffLineRowAttributesCache.set(options.line, { lineKey, attributes });
  return attributes;
}

// renderSplitLineHtml renders one row into old/new side-by-side cells.
// renderSplitLineHtml 将单行渲染为新旧并排单元格。
function renderSplitLineHtml(line: DiffLine, classes: string, rowAttributes: string, contentHtml: string): string {
  if (line.kind === "metadata") {
    return `<div class="${classes} split metadata" ${rowAttributes}><span class="diff-line-number"></span><code class="diff-split-code metadata">${contentHtml}</code></div>`;
  }
  const oldLineNumber = formatOldLineNumber(line);
  const newLineNumber = formatNewLineNumber(line);
  const oldContentHtml = line.kind === "added" ? "" : contentHtml;
  const newContentHtml = line.kind === "removed" ? "" : contentHtml;
  return `<div class="${classes} split" ${rowAttributes}><span class="diff-line-number old">${oldLineNumber}</span><code class="diff-split-code old">${oldContentHtml}</code><span class="diff-line-number new">${newLineNumber}</span><code class="diff-split-code new">${newContentHtml}</code></div>`;
}

// lineContentHtml returns highlighted token HTML when available, otherwise escaped text.
// lineContentHtml 在可用时返回高亮 token HTML，否则返回转义文本。
function lineContentHtml(
  line: DiffLine,
  lineKey: string,
  highlightedLineSpans?: ReadonlyMap<string, DiffShikiTokenSpan[]>,
  inlineDiff = false,
): string {
  const inlineChangeRanges = inlineDiff ? normalizedInlineChangeRanges(line) : EMPTY_INLINE_CHANGE_RANGES;
  if (line.kind !== "metadata") {
    const spans = highlightedLineSpans?.get(lineKey);
    if (spans) {
      return inlineChangeRanges.length ? tokenSpansToInlineDiffHtml(spans, inlineChangeRanges) : tokenSpansToHtml(spans);
    }
  }
  return inlineChangeRanges.length ? plainInlineDiffHtml(line, inlineChangeRanges) : escapedDiffLineContent(line);
}

// plainLineClassName returns one row class string with dynamic search state.
// plainLineClassName 返回包含动态搜索状态的单行 class 字符串。
function plainLineClassName(
  rowKind: DiffLine["kind"],
  lineKey: string,
  searchState: DiffPlainRenderSearchState,
): string {
  if (!searchState.activeSearchLineKey && !searchState.searchLineKeys?.size) {
    return DIFF_LINE_BASE_CLASS_NAMES[rowKind];
  }
  const isSearchMatch = Boolean(searchState.searchLineKeys?.has(lineKey));
  const isActiveSearchMatch = searchState.activeSearchLineKey === lineKey;
  return DIFF_LINE_SEARCH_CLASS_NAMES[rowKind][(isSearchMatch ? 1 : 0) + (isActiveSearchMatch ? 2 : 0)];
}

// formatLineNumber formats optional old and new line numbers for the gutter.
// formatLineNumber 格式化 gutter 中可选的新旧行号。
function formatLineNumber(line: DiffLine): string {
  const cached = diffUnifiedLineNumberCache.get(line);
  if (cached != null) {
    return cached;
  }
  const visualLineNumber = line.kind === "removed" ? line.oldLine : line.newLine;
  const lineNumber = visualLineNumber == null ? "" : String(visualLineNumber);
  diffUnifiedLineNumberCache.set(line, lineNumber);
  return lineNumber;
}

// formatOldLineNumber formats and caches the old-side split gutter line number.
// formatOldLineNumber 格式化并缓存 split 旧侧 gutter 行号。
function formatOldLineNumber(line: DiffLine): string {
  const cached = diffOldLineNumberCache.get(line);
  if (cached != null) {
    return cached;
  }
  const lineNumber = line.oldLine == null ? "" : String(line.oldLine);
  diffOldLineNumberCache.set(line, lineNumber);
  return lineNumber;
}

// formatNewLineNumber formats and caches the new-side split gutter line number.
// formatNewLineNumber 格式化并缓存 split 新侧 gutter 行号。
function formatNewLineNumber(line: DiffLine): string {
  const cached = diffNewLineNumberCache.get(line);
  if (cached != null) {
    return cached;
  }
  const lineNumber = line.newLine == null ? "" : String(line.newLine);
  diffNewLineNumberCache.set(line, lineNumber);
  return lineNumber;
}

// escapedDiffLineContent returns cached escaped plain text for one diff line.
// escapedDiffLineContent 返回单条 diff 行的缓存转义纯文本。
function escapedDiffLineContent(line: DiffLine): string {
  const cached = escapedDiffLineContentCache.get(line);
  if (cached != null) {
    return cached;
  }
  const escapedContent = escapeHtml(diffLineContentText(line));
  escapedDiffLineContentCache.set(line, escapedContent);
  return escapedContent;
}

// diffLineContentText returns the exact visible text used for empty and non-empty diff lines.
// diffLineContentText 返回空行与非空 diff 行使用的精确可见文本。
function diffLineContentText(line: DiffLine): string {
  return line.content || " ";
}

// normalizedInlineChangeRanges clamps parsed inline ranges to the current line content.
// normalizedInlineChangeRanges 将解析得到的行内范围约束到当前行内容内。
function normalizedInlineChangeRanges(line: DiffLine): readonly DiffInlineChangeRange[] {
  const cached = normalizedInlineChangeRangesCache.get(line);
  if (cached) {
    return cached;
  }
  let normalizedRanges: readonly DiffInlineChangeRange[];
  if (line.kind !== "added" && line.kind !== "removed") {
    normalizedRanges = EMPTY_INLINE_CHANGE_RANGES;
  } else if (line.inlineChangeRanges?.length) {
    const ranges: DiffInlineChangeRange[] = [];
    for (const range of line.inlineChangeRanges) {
      const start = Math.max(0, Math.min(range.start, line.content.length));
      const end = Math.max(start, Math.min(range.end, line.content.length));
      if (end > start) {
        pushNormalizedInlineRange(ranges, start, end);
      }
    }
    normalizedRanges = ranges.length ? ranges : EMPTY_INLINE_CHANGE_RANGES;
  } else if (line.inlineChangeRange) {
    const start = Math.max(0, Math.min(line.inlineChangeRange.start, line.content.length));
    const end = Math.max(start, Math.min(line.inlineChangeRange.end, line.content.length));
    normalizedRanges = end > start ? [{ start, end }] : EMPTY_INLINE_CHANGE_RANGES;
  } else {
    normalizedRanges = EMPTY_INLINE_CHANGE_RANGES;
  }
  normalizedInlineChangeRangesCache.set(line, normalizedRanges);
  return normalizedRanges;
}

// pushNormalizedInlineRange appends one sorted range and merges overlaps.
// pushNormalizedInlineRange 追加一个已排序范围并合并重叠部分。
function pushNormalizedInlineRange(ranges: DiffInlineChangeRange[], start: number, end: number): void {
  const previous = ranges[ranges.length - 1];
  if (previous && previous.end >= start) {
    previous.end = Math.max(previous.end, end);
    return;
  }
  ranges.push({ start, end });
}

// textToInlineDiffHtml wraps raw text ranges with inline change markers.
// textToInlineDiffHtml 使用行内变化标记包裹多段原始文本范围。
function textToInlineDiffHtml(content: string, ranges: readonly DiffInlineChangeRange[]): string {
  const text = content || " ";
  let cursor = 0;
  let html = "";
  for (const range of ranges) {
    html += escapeHtml(text.slice(cursor, range.start));
    html += `<span class="diff-inline-change">${escapeHtml(text.slice(range.start, range.end))}</span>`;
    cursor = range.end;
  }
  html += escapeHtml(text.slice(cursor));
  return html;
}

// plainInlineDiffHtml returns cached inline-marked plain HTML for one parsed diff line.
// plainInlineDiffHtml 返回单条解析 diff 行缓存后的 inline 纯文本 HTML。
function plainInlineDiffHtml(line: DiffLine, ranges: readonly DiffInlineChangeRange[]): string {
  const cached = plainInlineDiffHtmlCache.get(line);
  if (cached != null) {
    return cached;
  }
  const html = textToInlineDiffHtml(diffLineContentText(line), ranges);
  plainInlineDiffHtmlCache.set(line, html);
  return html;
}

// tokenSpansToInlineDiffHtml wraps highlighted token ranges without discarding token colors.
// tokenSpansToInlineDiffHtml 在不丢弃 token 颜色的前提下包裹多段高亮 token 范围。
function tokenSpansToInlineDiffHtml(spans: readonly DiffShikiTokenSpan[], ranges: readonly DiffInlineChangeRange[]): string {
  const cached = tokenInlineDiffHtmlCache.get(spans)?.get(ranges);
  if (cached != null) {
    return cached;
  }
  let offset = 0;
  let html = "";
  let rangeIndex = 0;
  for (const span of spans) {
    const content = span[0];
    const style = span[1];
    const nextOffset = offset + content.length;
    let localOffset = 0;
    while (rangeIndex < ranges.length && ranges[rangeIndex].end <= offset) {
      rangeIndex += 1;
    }
    let activeRangeIndex = rangeIndex;
    while (activeRangeIndex < ranges.length && ranges[activeRangeIndex].start < nextOffset) {
      const range = ranges[activeRangeIndex];
      const rangeStart = Math.max(0, range.start - offset);
      const rangeEnd = Math.min(content.length, range.end - offset);
      html += tokenSpanSegmentHtml(content.slice(localOffset, rangeStart), style);
      html += `<span class="diff-inline-change">${tokenSpanSegmentHtml(content.slice(rangeStart, rangeEnd), style)}</span>`;
      localOffset = rangeEnd;
      if (range.end <= nextOffset) {
        activeRangeIndex += 1;
      } else {
        break;
      }
    }
    html += tokenSpanSegmentHtml(content.slice(localOffset), style);
    offset = nextOffset;
  }
  const result = html || escapeHtml(" ");
  rememberTokenInlineDiffHtml(spans, ranges, result);
  return result;
}

// rememberTokenInlineDiffHtml stores one token/range inline HTML result by stable object identity.
// rememberTokenInlineDiffHtml 按稳定对象身份保存一次 token 与 range 组合的 inline HTML。
function rememberTokenInlineDiffHtml(
  // Stable Shiki token span array from the highlight cache.
  // 来自高亮缓存的稳定 Shiki token span 数组。
  spans: readonly DiffShikiTokenSpan[],
  // Stable normalized inline range array from the parsed diff line.
  // 来自解析 diff 行的稳定归一化 inline range 数组。
  ranges: readonly DiffInlineChangeRange[],
  // Rendered inline HTML fragment.
  // 已渲染的 inline HTML 片段。
  html: string,
): void {
  let htmlByRanges = tokenInlineDiffHtmlCache.get(spans);
  if (!htmlByRanges) {
    htmlByRanges = new WeakMap<readonly DiffInlineChangeRange[], string>();
    tokenInlineDiffHtmlCache.set(spans, htmlByRanges);
  }
  htmlByRanges.set(ranges, html);
}

// tokenSpanSegmentHtml renders one token segment with its original inline style.
// tokenSpanSegmentHtml 使用原始行内样式渲染一个 token 片段。
function tokenSpanSegmentHtml(content: string, style: string): string {
  if (!content) {
    return "";
  }
  const escapedContent = escapeHtml(content);
  return style ? `<span style="${style}">${escapedContent}</span>` : escapedContent;
}
