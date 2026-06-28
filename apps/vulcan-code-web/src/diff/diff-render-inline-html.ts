import {
  escapeHtml,
  tokenSpansToHtml,
  type DiffShikiTokenSpan,
} from "./diff-shiki-shared";
import type { DiffInlineChangeRange, DiffLine } from "./types";

// EMPTY_INLINE_CHANGE_RANGES reuses one immutable range list for lines without inline changes.
// EMPTY_INLINE_CHANGE_RANGES 为没有行内变化的行复用不可变空范围列表。
const EMPTY_INLINE_CHANGE_RANGES: readonly DiffInlineChangeRange[] = Object.freeze([]);

// normalizedInlineChangeRangesCache stores clamped inline ranges by stable diff line object.
// normalizedInlineChangeRangesCache 按稳定 diff 行对象缓存已约束的行内变化范围。
const normalizedInlineChangeRangesCache = new WeakMap<DiffLine, readonly DiffInlineChangeRange[]>();

// escapedDiffLineContentCache stores escaped plain content by stable diff line object.
// escapedDiffLineContentCache 按稳定 diff 行对象缓存已转义纯文本内容。
const escapedDiffLineContentCache = new WeakMap<DiffLine, string>();

// plainInlineDiffHtmlCache stores inline-marked plain HTML by stable diff line object.
// plainInlineDiffHtmlCache 按稳定 diff 行对象缓存带行内标记的纯文本 HTML。
const plainInlineDiffHtmlCache = new WeakMap<DiffLine, string>();

// tokenInlineDiffHtmlCache stores inline-marked token HTML by stable token and range arrays.
// tokenInlineDiffHtmlCache 按稳定 token 与 range 数组缓存行内标记后的高亮 HTML。
const tokenInlineDiffHtmlCache = new WeakMap<readonly DiffShikiTokenSpan[], WeakMap<readonly DiffInlineChangeRange[], string>>();

// lineContentHtml returns highlighted token HTML when available, otherwise escaped text.
// lineContentHtml 在可用时返回高亮 token HTML，否则返回转义文本。
export function lineContentHtml(
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

// textToInlineDiffHtml wraps raw text ranges with inline change markers.
// textToInlineDiffHtml 使用行内变化标记包裹多段原始文本范围。
function textToInlineDiffHtml(content: string, ranges: readonly DiffInlineChangeRange[]): string {
  const text = content || " ";
  let cursor = 0;
  let html = "";
  for (const range of ranges) {
    html += escapeHtml(text.slice(cursor, range.start));
    html += `<span data-diff-span="">${escapeHtml(text.slice(range.start, range.end))}</span>`;
    cursor = range.end;
  }
  html += escapeHtml(text.slice(cursor));
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
      html += `<span data-diff-span="">${tokenSpanSegmentHtml(content.slice(rangeStart, rangeEnd), style)}</span>`;
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
