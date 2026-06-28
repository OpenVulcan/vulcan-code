import type { DiffHighlightedLineSpans } from "../diff-render-html-core";
import { highlightedDiffLineSpansForDiffLine } from "../diff-highlight";
import { diffSearchLineKey } from "../diff-search";
import { normalizeShikiLanguage, type DiffShikiTheme, type DiffShikiTokenSpan } from "../diff-shiki-shared";
import type { DiffMountedVirtualRow } from "../diff-virtual-layout";
import type { DiffFile } from "../types";
import { DIFF_HIGHLIGHTED_SPAN_COLLECTION_CACHE_LIMIT, DIFF_RENDER_CACHE_KEY_SEPARATOR, DIFF_RENDER_FINGERPRINT_SEED, DIFF_RENDER_WORKER_HIGHLIGHTED_SPAN_CACHE_LIMIT } from "./constants";
import { appendFingerprintNumber, appendFingerprintText, diffFileContentFingerprint, diffRowsContentFingerprint, rememberHighlightedSpansFingerprint } from "./fingerprints";
import type {
  DiffHighlightedFileSpanStableCacheEntry,
  DiffHighlightedLineSpansWorkerPayload,
  DiffHighlightedLineSpansWorkerPayloadCacheEntry,
  DiffHighlightedRowsSpanStableCacheEntry,
  DiffHighlightedSpanCollectionCacheEntry,
  DiffRenderWorkerRequest,
} from "./types";
// diffRenderWorkerHighlightedSpanCache mirrors spans known to be posted to the current render worker.
// diffRenderWorkerHighlightedSpanCache 记录已成功发送到当前 render worker 的高亮 span。
const diffRenderWorkerHighlightedSpanCache = new Map<string, DiffShikiTokenSpan[]>();

// diffRenderWorkerHighlightedSpanCacheRevision invalidates derived payloads when the worker span mirror changes.
// diffRenderWorkerHighlightedSpanCacheRevision 在 Worker span 镜像变化时失效派生载荷。
let diffRenderWorkerHighlightedSpanCacheRevision = 0;

// highlightedLineSpansWorkerPayloadCache stores incremental span payloads for stable collected span maps.
// highlightedLineSpansWorkerPayloadCache 为稳定收集到的 span map 缓存增量载荷。
const highlightedLineSpansWorkerPayloadCache = new WeakMap<ReadonlyMap<string, DiffShikiTokenSpan[]>, DiffHighlightedLineSpansWorkerPayloadCacheEntry>();
// highlightedFileLineSpansCache stores the latest span collection per parsed file object.
// highlightedFileLineSpansCache 按已解析文件对象保存最新 span 收集结果。
const highlightedFileLineSpansCache = new WeakMap<DiffFile, DiffHighlightedSpanCollectionCacheEntry>();

// highlightedFileLineSpansStableCache reuses equal file-level span maps across highlight revisions.
// highlightedFileLineSpansStableCache 为完整文件跨高亮修订复用等价 span map。
const highlightedFileLineSpansStableCache = new WeakMap<DiffFile, DiffHighlightedFileSpanStableCacheEntry>();

// highlightedRowsLineSpansCache stores span collections for recently mounted virtual windows.
// highlightedRowsLineSpansCache 缓存最近挂载过的虚拟窗口 span 收集结果。
const highlightedRowsLineSpansCache = new Map<string, Map<string, DiffShikiTokenSpan[]> | undefined>();

// highlightedRowsLineSpansStableCache reuses equal span maps across highlight revisions for stable row arrays.
// highlightedRowsLineSpansStableCache 为稳定行数组跨高亮修订复用等价 span map。
const highlightedRowsLineSpansStableCache = new WeakMap<ReadonlyArray<DiffMountedVirtualRow>, DiffHighlightedRowsSpanStableCacheEntry>();
// highlightedLineSpansWorkerPayload builds an incremental payload for the render worker span cache.
// highlightedLineSpansWorkerPayload 为 render worker 的 span 缓存构建增量载荷。
export function highlightedLineSpansWorkerPayload(
  spansByLineKey?: ReadonlyMap<string, DiffShikiTokenSpan[]>,
): DiffHighlightedLineSpansWorkerPayload {
  if (!spansByLineKey?.size) {
    return {};
  }
  const cached = highlightedLineSpansWorkerPayloadCache.get(spansByLineKey);
  if (cached?.revision === diffRenderWorkerHighlightedSpanCacheRevision) {
    return cached.payload;
  }
  const reset = shouldResetRenderWorkerHighlightedSpanCache(spansByLineKey.size);
  let updates: DiffHighlightedLineSpans[] | undefined;
  for (const [lineKey, spans] of spansByLineKey.entries()) {
    if (reset || diffRenderWorkerHighlightedSpanCache.get(lineKey) !== spans) {
      updates ??= [];
      updates.push({ lineKey, spans });
    }
  }
  const payload = {
    highlighted: true,
    reset,
    updates,
  };
  highlightedLineSpansWorkerPayloadCache.set(spansByLineKey, {
    revision: diffRenderWorkerHighlightedSpanCacheRevision,
    payload,
  });
  return payload;
}

// shouldResetRenderWorkerHighlightedSpanCache decides when to resync the worker span cache explicitly.
// shouldResetRenderWorkerHighlightedSpanCache 判断何时显式重新同步 Worker span 缓存。
function shouldResetRenderWorkerHighlightedSpanCache(nextSpanCount: number): boolean {
  return diffRenderWorkerHighlightedSpanCache.size + nextSpanCount > DIFF_RENDER_WORKER_HIGHLIGHTED_SPAN_CACHE_LIMIT;
}

// resetDiffRenderWorkerHighlightedSpanCacheMirror clears the local worker span mirror and invalidates derived payloads.
// resetDiffRenderWorkerHighlightedSpanCacheMirror 清空本地 Worker span 镜像并失效派生载荷。
export function resetDiffRenderWorkerHighlightedSpanCacheMirror(): void {
  diffRenderWorkerHighlightedSpanCache.clear();
  diffRenderWorkerHighlightedSpanCacheRevision += 1;
}

// rememberPostedHighlightedLineSpans records span updates only after postMessage succeeds.
// rememberPostedHighlightedLineSpans 仅在 postMessage 成功后记录已发送的 span 更新。
export function rememberPostedHighlightedLineSpans(request: DiffRenderWorkerRequest): void {
  let changed = false;
  if (request.resetHighlightedLineSpans) {
    diffRenderWorkerHighlightedSpanCache.clear();
    changed = true;
  }
  if (!request.highlightedLineSpans?.length) {
    if (changed) {
      diffRenderWorkerHighlightedSpanCacheRevision += 1;
    }
    return;
  }
  for (const entry of request.highlightedLineSpans) {
    if (diffRenderWorkerHighlightedSpanCache.get(entry.lineKey) !== entry.spans) {
      diffRenderWorkerHighlightedSpanCache.set(entry.lineKey, entry.spans);
      changed = true;
    }
  }
  if (changed) {
    diffRenderWorkerHighlightedSpanCacheRevision += 1;
  }
}
// collectHighlightedFileLineSpans gathers cached token spans for one full file.
// collectHighlightedFileLineSpans 收集单个完整文件中已经缓存的 token span。
export function collectHighlightedFileLineSpans(file: DiffFile, language: string, highlightRevision: number, theme: DiffShikiTheme): Map<string, DiffShikiTokenSpan[]> | undefined {
  // normalizedLanguage keeps the file-level highlight lookup on the canonical Shiki id.
  // normalizedLanguage 让文件级高亮查询复用规范化后的 Shiki 标识。
  const normalizedLanguage = normalizeShikiLanguage(language);
  if (!normalizedLanguage) {
    return undefined;
  }
  const fileFingerprint = diffFileContentFingerprint(file);
  // cacheKey identifies this file span collection for one language and highlight revision.
  // cacheKey 标识当前语言与高亮修订下的文件 span 收集结果。
  const cacheKey = highlightedFileLineSpansCacheKey(file, fileFingerprint, normalizedLanguage, highlightRevision, theme);
  // cachedEntry reuses the latest collection for stable parsed file objects.
  // cachedEntry 复用稳定已解析文件对象上的最新收集结果。
  const cachedEntry = highlightedFileLineSpansCache.get(file);
  if (cachedEntry?.cacheKey === cacheKey) {
    return cachedEntry.spans;
  }
  let spansByLineKey: Map<string, DiffShikiTokenSpan[]> | undefined;
  let spansHash = DIFF_RENDER_FINGERPRINT_SEED;
  let spansCount = 0;
  for (let hunkIndex = 0; hunkIndex < file.hunks.length; hunkIndex += 1) {
    const hunk = file.hunks[hunkIndex];
    for (let lineIndex = 0; lineIndex < hunk.lines.length; lineIndex += 1) {
      const line = hunk.lines[lineIndex];
      if (line.kind === "metadata") {
        continue;
      }
      const spans = highlightedDiffLineSpansForDiffLine(line, normalizedLanguage, theme);
      if (spans) {
        const lineKey = diffSearchLineKey(file.key, hunkIndex, lineIndex);
        spansCount += 1;
        spansHash = appendFingerprintText(spansHash, lineKey);
        spansHash = appendFingerprintNumber(spansHash, spans.length);
        spansByLineKey ??= new Map();
        spansByLineKey.set(lineKey, spans);
      }
    }
  }
  const spansFingerprint = spansCount > 0 ? `${spansCount}:${spansHash.toString(36)}` : "0";
  rememberHighlightedSpansFingerprint(spansByLineKey, spansFingerprint);
  const stableEntry = highlightedFileLineSpansStableCache.get(file);
  if (
    stableEntry
    && stableEntry.theme === theme
    && stableEntry.language === normalizedLanguage
    && stableEntry.fileFingerprint === fileFingerprint
    && stableEntry.spansFingerprint === spansFingerprint
  ) {
    highlightedFileLineSpansCache.set(file, { cacheKey, spans: stableEntry.spans });
    return stableEntry.spans;
  }
  highlightedFileLineSpansStableCache.set(file, {
    theme,
    language: normalizedLanguage,
    fileFingerprint,
    spansFingerprint,
    spans: spansByLineKey,
  });
  // spans stores undefined for empty collections so repeated renders avoid rescanning.
  // spans 对空收集结果保存 undefined，避免重复渲染时再次扫描。
  highlightedFileLineSpansCache.set(file, { cacheKey, spans: spansByLineKey });
  return spansByLineKey;
}

// collectHighlightedRowsLineSpans gathers cached token spans for one mounted virtual row window.
// collectHighlightedRowsLineSpans 收集一个已挂载虚拟行窗口中已经缓存的 token span。
export function collectHighlightedRowsLineSpans(
  fileKey: string,
  rows: readonly DiffMountedVirtualRow[],
  language: string,
  highlightRevision: number,
  theme: DiffShikiTheme,
): Map<string, DiffShikiTokenSpan[]> | undefined {
  // normalizedLanguage keeps the virtual-window highlight lookup on the canonical Shiki id.
  // normalizedLanguage 让虚拟窗口高亮查询复用规范化后的 Shiki 标识。
  const normalizedLanguage = normalizeShikiLanguage(language);
  if (!normalizedLanguage) {
    return undefined;
  }
  const rowsFingerprint = diffRowsContentFingerprint(rows);
  // cacheKey identifies one virtual window span collection for a language and revision.
  // cacheKey 标识当前语言与修订下的虚拟窗口 span 收集结果。
  const cacheKey = highlightedRowsLineSpansCacheKey(fileKey, rowsFingerprint, normalizedLanguage, highlightRevision, theme);
  if (highlightedRowsLineSpansCache.has(cacheKey)) {
    return highlightedRowsLineSpansCache.get(cacheKey);
  }
  let spansByLineKey: Map<string, DiffShikiTokenSpan[]> | undefined;
  let spansHash = DIFF_RENDER_FINGERPRINT_SEED;
  let spansCount = 0;
  for (const row of rows) {
    if (row.kind !== "line" || row.line.kind === "metadata") {
      continue;
    }
    const spans = highlightedDiffLineSpansForDiffLine(row.line, normalizedLanguage, theme);
    if (spans) {
      spansCount += 1;
      spansHash = appendFingerprintText(spansHash, row.lineKey);
      spansHash = appendFingerprintNumber(spansHash, spans.length);
      spansByLineKey ??= new Map();
      spansByLineKey.set(row.lineKey, spans);
    }
  }
  const spansFingerprint = spansCount > 0 ? `${spansCount}:${spansHash.toString(36)}` : "0";
  rememberHighlightedSpansFingerprint(spansByLineKey, spansFingerprint);
  const stableEntry = highlightedRowsLineSpansStableCache.get(rows);
  if (
    stableEntry
    && stableEntry.theme === theme
    && stableEntry.language === normalizedLanguage
    && stableEntry.rowsFingerprint === rowsFingerprint
    && stableEntry.spansFingerprint === spansFingerprint
  ) {
    rememberHighlightedRowsLineSpans(cacheKey, stableEntry.spans);
    return stableEntry.spans;
  }
  highlightedRowsLineSpansStableCache.set(rows, {
    theme,
    language: normalizedLanguage,
    rowsFingerprint,
    spansFingerprint,
    spans: spansByLineKey,
  });
  // spans stores undefined for empty collections so repeated windows avoid rescanning.
  // spans 对空收集结果保存 undefined，避免重复窗口再次扫描。
  rememberHighlightedRowsLineSpans(cacheKey, spansByLineKey);
  return spansByLineKey;
}

// highlightedFileLineSpansCacheKey builds a stable key for one file span collection.
// highlightedFileLineSpansCacheKey 为单个文件 span 收集结果构建稳定键。
function highlightedFileLineSpansCacheKey(file: DiffFile, fileFingerprint: string, language: string, highlightRevision: number, theme: DiffShikiTheme): string {
  return theme
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + language
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + String(highlightRevision)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + file.key
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + fileFingerprint;
}

// highlightedRowsLineSpansCacheKey builds a stable key for one virtual-window span collection.
// highlightedRowsLineSpansCacheKey 为单个虚拟窗口 span 收集结果构建稳定键。
function highlightedRowsLineSpansCacheKey(
  fileKey: string,
  rowsFingerprint: string,
  language: string,
  highlightRevision: number,
  theme: DiffShikiTheme,
): string {
  return fileKey
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + theme
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + language
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + String(highlightRevision)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + rowsFingerprint;
}

// rememberHighlightedRowsLineSpans stores one virtual-window span collection with FIFO eviction.
// rememberHighlightedRowsLineSpans 以 FIFO 淘汰策略保存一个虚拟窗口 span 收集结果。
function rememberHighlightedRowsLineSpans(cacheKey: string, spans: Map<string, DiffShikiTokenSpan[]> | undefined): void {
  if (highlightedRowsLineSpansCache.has(cacheKey)) {
    highlightedRowsLineSpansCache.delete(cacheKey);
  } else if (highlightedRowsLineSpansCache.size >= DIFF_HIGHLIGHTED_SPAN_COLLECTION_CACHE_LIMIT) {
    // oldestKey points at the earliest retained virtual-window collection.
    // oldestKey 指向最早保留的虚拟窗口收集结果。
    const oldestKey = highlightedRowsLineSpansCache.keys().next().value;
    if (oldestKey) {
      highlightedRowsLineSpansCache.delete(oldestKey);
    }
  }
  highlightedRowsLineSpansCache.set(cacheKey, spans);
}

