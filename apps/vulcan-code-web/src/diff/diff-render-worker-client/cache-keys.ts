import type { DiffPlainFileRenderOptions } from "../diff-render-html-core";
import type { DiffShikiTheme } from "../diff-shiki-shared";
import { DIFF_RENDER_CACHE_KEY_SEPARATOR } from "./constants";
import { diffFileContentFingerprint, diffRowsContentFingerprint, highlightedSpansFingerprint } from "./fingerprints";
import { searchStateCacheKey } from "./search-payload";
import type { DiffHighlightedFileWorkerOptions, DiffHighlightedRowsWorkerOptions, DiffPlainRowsWorkerOptions } from "./types";
// plainFileRenderCacheKey builds one stable cache key for complete file plain HTML.
// plainFileRenderCacheKey 为完整文件纯 HTML 构建稳定缓存 key。
export function plainFileRenderCacheKey(options: DiffPlainFileRenderOptions, searchSignature?: string): string {
  return "file"
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + options.file.key
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + String(options.file.rowCount ?? options.file.estimatedRowCount ?? 0)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + diffFileContentFingerprint(options.file)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.contextControls ? "controls" : "plain")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.inlineDiff ? "inline" : "line")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.viewMode ?? "unified")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.overflowMode ?? "scroll")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.hunkSeparatorStyle ?? "line-info")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.indicatorStyle ?? "bars")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + searchStateCacheKey(searchSignature, options.activeSearchLineKey);
}

// plainRowsRenderCacheKey builds one stable cache key for a virtual row-window plain HTML block.
// plainRowsRenderCacheKey 为虚拟行窗口纯 HTML 块构建稳定缓存 key。
export function plainRowsRenderCacheKey(options: DiffPlainRowsWorkerOptions, searchSignature?: string): string {
  return "rows"
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + options.fileKey
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + String(options.rows.length)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + diffRowsContentFingerprint(options.rows)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.contextControls ? "controls" : "plain")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.inlineDiff ? "inline" : "line")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.viewMode ?? "unified")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.overflowMode ?? "scroll")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.hunkSeparatorStyle ?? "line-info")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.indicatorStyle ?? "bars")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + searchStateCacheKey(searchSignature, options.activeSearchLineKey);
}

// highlightedFileRenderCacheKey builds one stable cache key for highlighted complete file HTML.
// highlightedFileRenderCacheKey 为完整文件高亮 HTML 构建稳定缓存 key。
export function highlightedFileRenderCacheKey(
  options: DiffHighlightedFileWorkerOptions,
  theme: DiffShikiTheme,
  highlightedLineSpans?: ReadonlyMap<string, unknown>,
  searchSignature?: string,
): string {
  return "highlight-file"
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + options.file.key
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + theme
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + options.language
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + highlightedSpansFingerprint(highlightedLineSpans)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + String(options.file.rowCount ?? options.file.estimatedRowCount ?? 0)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + diffFileContentFingerprint(options.file)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.contextControls ? "controls" : "plain")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.inlineDiff ? "inline" : "line")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.viewMode ?? "unified")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.overflowMode ?? "scroll")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.hunkSeparatorStyle ?? "line-info")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.indicatorStyle ?? "bars")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + searchStateCacheKey(searchSignature, options.activeSearchLineKey);
}

// highlightedRowsRenderCacheKey builds one stable cache key for highlighted virtual row-window HTML.
// highlightedRowsRenderCacheKey 为虚拟行窗口高亮 HTML 构建稳定缓存 key。
export function highlightedRowsRenderCacheKey(
  options: DiffHighlightedRowsWorkerOptions,
  theme: DiffShikiTheme,
  highlightedLineSpans?: ReadonlyMap<string, unknown>,
  searchSignature?: string,
): string {
  return "highlight-rows"
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + options.fileKey
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + theme
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + options.language
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + highlightedSpansFingerprint(highlightedLineSpans)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + String(options.rows.length)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + diffRowsContentFingerprint(options.rows)
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.contextControls ? "controls" : "plain")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.inlineDiff ? "inline" : "line")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.viewMode ?? "unified")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.overflowMode ?? "scroll")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.hunkSeparatorStyle ?? "line-info")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + (options.indicatorStyle ?? "bars")
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + searchStateCacheKey(searchSignature, options.activeSearchLineKey);
}

