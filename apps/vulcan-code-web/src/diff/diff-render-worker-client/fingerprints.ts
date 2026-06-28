import type { DiffRenderableVirtualRow } from "../diff-render-html-core";
import type { DiffMountedVirtualRow } from "../diff-virtual-layout";
import type { DiffShikiTokenSpan } from "../diff-shiki-shared";
import type { DiffFile, DiffLine } from "../types";
import { DIFF_RENDER_FINGERPRINT_SEED } from "./constants";
// diffFileContentFingerprintCache stores content fingerprints per parsed file object.
// diffFileContentFingerprintCache 按已解析文件对象缓存内容 fingerprint。
const diffFileContentFingerprintCache = new WeakMap<DiffFile, string>();

// diffRowsContentFingerprintCache stores content fingerprints per stable virtual row array.
// diffRowsContentFingerprintCache 按稳定虚拟行数组缓存内容 fingerprint。
const diffRowsContentFingerprintCache = new WeakMap<ReadonlyArray<DiffMountedVirtualRow>, string>();

// diffRenderableRowsCache stores compact render-worker rows for stable virtual row arrays.
// diffRenderableRowsCache 按稳定虚拟行数组缓存精简后的 render worker 行。
const diffRenderableRowsCache = new WeakMap<ReadonlyArray<DiffMountedVirtualRow>, DiffRenderableVirtualRow[]>();
// highlightedSpansFingerprintCache stores compact signatures for stable highlighted span maps.
// highlightedSpansFingerprintCache 按稳定高亮 span map 缓存紧凑签名。
const highlightedSpansFingerprintCache = new WeakMap<ReadonlyMap<string, unknown>, string>();
// diffFileContentFingerprint returns a stable content signature for one parsed file object.
// diffFileContentFingerprint 返回单个已解析文件对象的稳定内容签名。
export function diffFileContentFingerprint(file: DiffFile): string {
  const cached = diffFileContentFingerprintCache.get(file);
  if (cached) {
    return cached;
  }
  let hash = DIFF_RENDER_FINGERPRINT_SEED;
  hash = appendFingerprintText(hash, file.status);
  hash = appendFingerprintText(hash, file.oldPath);
  hash = appendFingerprintText(hash, file.newPath);
  hash = appendFingerprintText(hash, file.fileMode ?? "");
  hash = appendFingerprintNumber(hash, file.hunks.length);
  for (const hunk of file.hunks) {
    hash = appendFingerprintText(hash, hunk.header);
    hash = appendFingerprintNumber(hash, hunk.oldStart);
    hash = appendFingerprintNumber(hash, hunk.newStart);
    hash = appendFingerprintNumber(hash, hunk.lines.length);
    for (const line of hunk.lines) {
      hash = appendFingerprintText(hash, line.kind);
      hash = appendFingerprintText(hash, line.content);
      hash = appendFingerprintNumber(hash, line.oldLine ?? -1);
      hash = appendFingerprintNumber(hash, line.newLine ?? -1);
      hash = appendInlineChangeRangesFingerprint(hash, line);
    }
  }
  const fingerprint = `${file.hunks.length}:${hash.toString(36)}`;
  diffFileContentFingerprintCache.set(file, fingerprint);
  return fingerprint;
}

// diffRowsContentFingerprint returns a stable content signature for one mounted virtual row array.
// diffRowsContentFingerprint 返回单个已挂载虚拟行数组的稳定内容签名。
export function diffRowsContentFingerprint(rows: ReadonlyArray<DiffMountedVirtualRow>): string {
  const cached = diffRowsContentFingerprintCache.get(rows);
  if (cached) {
    return cached;
  }
  let hash = DIFF_RENDER_FINGERPRINT_SEED;
  hash = appendFingerprintNumber(hash, rows.length);
  for (const row of rows) {
    hash = appendFingerprintText(hash, row.kind);
    hash = appendFingerprintText(hash, row.key);
    hash = appendFingerprintNumber(hash, row.top);
    hash = appendFingerprintNumber(hash, row.height);
    if (row.kind === "hunk") {
      hash = appendFingerprintText(hash, row.header);
      hash = appendFingerprintNumber(hash, row.hunkIndex);
      hash = appendFingerprintNumber(hash, row.collapsedLines ?? -1);
      hash = appendFingerprintNumber(hash, row.isFirstHunk ? 1 : 0);
      hash = appendFingerprintNumber(hash, row.isLastHunk ? 1 : 0);
      hash = appendFingerprintNumber(hash, row.expandUpHunkIndex ?? -1);
      hash = appendFingerprintNumber(hash, row.expandDownHunkIndex ?? -1);
    } else if (row.kind === "line") {
      hash = appendFingerprintNumber(hash, row.hunkIndex);
      hash = appendFingerprintNumber(hash, row.lineIndex);
      hash = appendFingerprintText(hash, row.line.kind);
      hash = appendFingerprintText(hash, row.line.content);
      hash = appendFingerprintNumber(hash, row.line.oldLine ?? -1);
      hash = appendFingerprintNumber(hash, row.line.newLine ?? -1);
      hash = appendInlineChangeRangesFingerprint(hash, row.line);
    } else {
      hash = appendFingerprintText(hash, row.type);
      hash = appendFingerprintNumber(hash, row.hunkIndex);
      hash = appendFingerprintNumber(hash, row.deletionLineIndex ?? -1);
      hash = appendFingerprintNumber(hash, row.additionLineIndex ?? -1);
      hash = appendFingerprintText(hash, row.deletionLineKey ?? "");
      hash = appendFingerprintText(hash, row.additionLineKey ?? "");
      if (row.deletionLine) {
        hash = appendFingerprintText(hash, row.deletionLine.kind);
        hash = appendFingerprintText(hash, row.deletionLine.content);
        hash = appendFingerprintNumber(hash, row.deletionLine.oldLine ?? -1);
        hash = appendFingerprintNumber(hash, row.deletionLine.newLine ?? -1);
        hash = appendInlineChangeRangesFingerprint(hash, row.deletionLine);
      }
      if (row.additionLine) {
        hash = appendFingerprintText(hash, row.additionLine.kind);
        hash = appendFingerprintText(hash, row.additionLine.content);
        hash = appendFingerprintNumber(hash, row.additionLine.oldLine ?? -1);
        hash = appendFingerprintNumber(hash, row.additionLine.newLine ?? -1);
        hash = appendInlineChangeRangesFingerprint(hash, row.additionLine);
      }
    }
  }
  const fingerprint = `${rows.length}:${hash.toString(36)}`;
  diffRowsContentFingerprintCache.set(rows, fingerprint);
  return fingerprint;
}

// appendInlineChangeRangesFingerprint folds single-range and multi-range inline diff metadata into a hash.
// appendInlineChangeRangesFingerprint 将单段与多段行内 diff 元数据写入哈希。
function appendInlineChangeRangesFingerprint(hash: number, line: DiffLine): number {
  if (line.inlineChangeRanges?.length) {
    hash = appendFingerprintNumber(hash, line.inlineChangeRanges.length);
    for (const range of line.inlineChangeRanges) {
      hash = appendFingerprintNumber(hash, range.start);
      hash = appendFingerprintNumber(hash, range.end);
    }
    return hash;
  }
  if (line.inlineChangeRange) {
    hash = appendFingerprintNumber(hash, 1);
    hash = appendFingerprintNumber(hash, line.inlineChangeRange.start);
    hash = appendFingerprintNumber(hash, line.inlineChangeRange.end);
    return hash;
  }
  return appendFingerprintNumber(hash, 0);
}

// diffRenderableRows returns compact rows with only fields required by HTML rendering.
// diffRenderableRows 返回仅包含 HTML 渲染所需字段的精简行。
export function diffRenderableRows(rows: ReadonlyArray<DiffMountedVirtualRow>): DiffRenderableVirtualRow[] {
  const cached = diffRenderableRowsCache.get(rows);
  if (cached) {
    return cached;
  }
  const renderableRows: DiffRenderableVirtualRow[] = new Array(rows.length);
  for (let index = 0; index < rows.length; index += 1) {
    const row = rows[index];
    if (row.kind === "hunk") {
      renderableRows[index] = {
        kind: "hunk",
        header: row.header,
        hunkIndex: row.hunkIndex,
        collapsedLines: row.collapsedLines,
        isFirstHunk: row.isFirstHunk,
        isLastHunk: row.isLastHunk,
        expandUpHunkIndex: row.expandUpHunkIndex,
        expandDownHunkIndex: row.expandDownHunkIndex,
      };
      continue;
    }
    if (row.kind === "line") {
      renderableRows[index] = {
        kind: "line",
        line: row.line,
        hunkIndex: row.hunkIndex,
        lineIndex: row.lineIndex,
        lineKey: row.lineKey,
      };
      continue;
    }
    renderableRows[index] = {
      kind: "pair",
      type: row.type,
      hunkIndex: row.hunkIndex,
      deletionLine: row.deletionLine,
      additionLine: row.additionLine,
      deletionLineIndex: row.deletionLineIndex,
      additionLineIndex: row.additionLineIndex,
      deletionLineKey: row.deletionLineKey,
      additionLineKey: row.additionLineKey,
    };
  }
  diffRenderableRowsCache.set(rows, renderableRows);
  return renderableRows;
}

// highlightedSpansFingerprint returns a compact signature for the currently available highlighted rows.
// highlightedSpansFingerprint 返回当前可用高亮行集合的紧凑签名。
export function highlightedSpansFingerprint(highlightedLineSpans?: ReadonlyMap<string, unknown>): string {
  if (!highlightedLineSpans?.size) {
    return "0";
  }
  const cached = highlightedSpansFingerprintCache.get(highlightedLineSpans);
  if (cached) {
    return cached;
  }
  let hash = DIFF_RENDER_FINGERPRINT_SEED;
  hash = appendFingerprintNumber(hash, highlightedLineSpans.size);
  for (const [lineKey, spans] of highlightedLineSpans.entries()) {
    hash = appendFingerprintText(hash, lineKey);
    hash = appendFingerprintNumber(hash, Array.isArray(spans) ? spans.length : 0);
  }
  const fingerprint = `${highlightedLineSpans.size}:${hash.toString(36)}`;
  highlightedSpansFingerprintCache.set(highlightedLineSpans, fingerprint);
  return fingerprint;
}
// appendFingerprintText folds text into an unsigned FNV-1a hash.
// appendFingerprintText 将文本折叠进无符号 FNV-1a hash。
export function appendFingerprintText(hash: number, value: string): number {
  let nextHash = hash;
  for (let index = 0; index < value.length; index += 1) {
    nextHash = Math.imul(nextHash ^ value.charCodeAt(index), 0x01000193) >>> 0;
  }
  return nextHash;
}

// appendFingerprintNumber folds a number into an unsigned FNV-1a hash.
// appendFingerprintNumber 将数字折叠进无符号 FNV-1a hash。
export function appendFingerprintNumber(hash: number, value: number): number {
  return appendFingerprintText(hash, String(value));
}
// rememberHighlightedSpansFingerprint reuses the collection-time signature for render cache keys.
// rememberHighlightedSpansFingerprint 复用收集阶段签名，供渲染缓存键直接读取。
export function rememberHighlightedSpansFingerprint(
  spansByLineKey: Map<string, DiffShikiTokenSpan[]> | undefined,
  spansFingerprint: string,
): void {
  if (spansByLineKey) {
    highlightedSpansFingerprintCache.set(spansByLineKey, spansFingerprint);
  }
}

