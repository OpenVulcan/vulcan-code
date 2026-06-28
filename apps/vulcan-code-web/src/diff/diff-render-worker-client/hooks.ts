import * as React from "react";

import { renderPlainDiffFileHtmlCore, renderPlainDiffVirtualRowsHtmlCore, type DiffPlainFileRenderOptions } from "../diff-render-html-core";
import { useDiffShikiTheme } from "../diff-highlight";
import { DISABLED_DIFF_RENDER_CACHE_KEY, EMPTY_DIFF_RENDERABLE_ROWS } from "./constants";
import { highlightedFileRenderCacheKey, highlightedRowsRenderCacheKey, plainFileRenderCacheKey, plainRowsRenderCacheKey } from "./cache-keys";
import { diffRenderableRows } from "./fingerprints";
import { collectHighlightedFileLineSpans, collectHighlightedRowsLineSpans, highlightedLineSpansWorkerPayload, rememberPostedHighlightedLineSpans } from "./highlighted-spans";
import { EMPTY_SERIALIZED_SEARCH_PAYLOAD, serializedSearchPayloadForRequest } from "./search-payload";
import type { DiffHighlightedFileWorkerOptions, DiffHighlightedRowsWorkerOptions, DiffPlainRowsWorkerOptions, DiffRenderFallbackHtml, DiffRenderWorkerRequest, DiffWorkerHtmlState } from "./types";
import { allocateDiffRenderWorkerRequestId, cancelQueuedDiffRenderWorkerRequest, currentDiffRenderWorker, diffRenderHtmlCache, diffRenderRequestScopes, diffRenderWorkerCallbacks, diffRenderWorkerFailureHtml, enqueueDiffRenderWorkerRequest, rememberDiffRenderHtml } from "./worker-runtime";
// disabledDiffRenderWorkerRequest is never posted; it keeps disabled hooks on a stable callback.
// disabledDiffRenderWorkerRequest 不会被发送，只用于让禁用 hook 保持稳定回调。
function disabledDiffRenderWorkerRequest(): DiffRenderWorkerRequest {
  return {
    id: 0,
    kind: "rows",
    fileKey: "",
    rows: EMPTY_DIFF_RENDERABLE_ROWS,
  };
}

// usePlainDiffFileHtml renders one complete unhighlighted file with an immediate visible fallback.
// usePlainDiffFileHtml 使用即时可见兜底渲染一个完整未高亮文件。
export function usePlainDiffFileHtml(options: DiffPlainFileRenderOptions, enabled: boolean): string {
  // shouldRender keeps the disabled sibling hook out of expensive fingerprint work.
  // shouldRender 让未启用的兄弟 hook 跳过昂贵的指纹计算。
  const shouldRender = enabled;
  // searchPayload is reused by the request builder to avoid duplicate serialization.
  // searchPayload 由请求构建器复用，避免重复序列化搜索 key。
  const searchPayload = React.useMemo(
    () => shouldRender ? serializedSearchPayloadForRequest(options.searchLineKeys) : EMPTY_SERIALIZED_SEARCH_PAYLOAD,
    [options.searchLineKeys, shouldRender],
  );
  const cacheKey = React.useMemo(() => shouldRender ? plainFileRenderCacheKey(options, searchPayload.signature) : DISABLED_DIFF_RENDER_CACHE_KEY, [
    options.activeSearchLineKey,
    options.contextControls,
    options.file,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.overflowMode,
    searchPayload,
    options.viewMode,
    shouldRender,
  ]);
  const request = React.useCallback((): DiffRenderWorkerRequest => ({
    id: 0,
    kind: "file",
    file: options.file,
    searchLineKeys: searchPayload.keys,
    searchLineKeySignature: searchPayload.signature,
    activeSearchLineKey: options.activeSearchLineKey,
    contextControls: options.contextControls,
    hunkSeparatorStyle: options.hunkSeparatorStyle,
    indicatorStyle: options.indicatorStyle,
    inlineDiff: options.inlineDiff,
    overflowMode: options.overflowMode,
    viewMode: options.viewMode,
  }), [
    options.activeSearchLineKey,
    options.contextControls,
    options.file,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.overflowMode,
    searchPayload,
    options.viewMode,
  ]);
  const fallbackHtml = React.useCallback(() => renderPlainDiffFileHtmlCore({
    file: options.file,
    searchLineKeys: options.searchLineKeys,
    activeSearchLineKey: options.activeSearchLineKey,
    contextControls: options.contextControls,
    hunkSeparatorStyle: options.hunkSeparatorStyle,
    indicatorStyle: options.indicatorStyle,
    inlineDiff: options.inlineDiff,
    overflowMode: options.overflowMode,
    viewMode: options.viewMode,
  }), [
    options.activeSearchLineKey,
    options.contextControls,
    options.file,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.overflowMode,
    options.searchLineKeys,
    options.viewMode,
  ]);
  return usePlainDiffWorkerHtml({
    cacheKey,
    enabled: shouldRender,
    fallbackHtml: shouldRender ? fallbackHtml : undefined,
    request: shouldRender ? request : disabledDiffRenderWorkerRequest,
  });
}

// usePlainDiffRowsHtml renders one unhighlighted virtual row window with an immediate visible fallback.
// usePlainDiffRowsHtml 使用即时可见兜底渲染一个未高亮虚拟行窗口。
export function usePlainDiffRowsHtml(options: DiffPlainRowsWorkerOptions, enabled: boolean): string {
  // hasRows keeps empty virtual windows out of the render worker queue.
  // hasRows 避免空虚拟窗口进入渲染 Worker 队列。
  const hasRows = options.rows.length > 0;
  // shouldRender keeps the disabled sibling hook out of row compaction and fingerprint work.
  // shouldRender 让未启用的兄弟 hook 跳过行压缩与指纹计算。
  const shouldRender = enabled && hasRows;
  // searchPayload is reused by the request builder to avoid duplicate serialization.
  // searchPayload 由请求构建器复用，避免重复序列化搜索 key。
  const searchPayload = React.useMemo(
    () => shouldRender ? serializedSearchPayloadForRequest(options.searchLineKeys) : EMPTY_SERIALIZED_SEARCH_PAYLOAD,
    [options.searchLineKeys, shouldRender],
  );
  // workerRows contains only the fields needed by the render worker.
  // workerRows 仅包含 render worker 所需的字段。
  const workerRows = React.useMemo(() => shouldRender ? diffRenderableRows(options.rows) : EMPTY_DIFF_RENDERABLE_ROWS, [options.rows, shouldRender]);
  const cacheKey = React.useMemo(() => shouldRender ? plainRowsRenderCacheKey(options, searchPayload.signature) : DISABLED_DIFF_RENDER_CACHE_KEY, [
    options.activeSearchLineKey,
    options.contextControls,
    options.fileKey,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.overflowMode,
    options.rows,
    searchPayload,
    options.viewMode,
    shouldRender,
  ]);
  const request = React.useCallback((): DiffRenderWorkerRequest => ({
    id: 0,
    kind: "rows",
    fileKey: options.fileKey,
    rows: workerRows,
    searchLineKeys: searchPayload.keys,
    searchLineKeySignature: searchPayload.signature,
    activeSearchLineKey: options.activeSearchLineKey,
    contextControls: options.contextControls,
    hunkSeparatorStyle: options.hunkSeparatorStyle,
    indicatorStyle: options.indicatorStyle,
    inlineDiff: options.inlineDiff,
    overflowMode: options.overflowMode,
    viewMode: options.viewMode,
  }), [
    options.activeSearchLineKey,
    options.contextControls,
    options.fileKey,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.overflowMode,
    searchPayload,
    options.viewMode,
    workerRows,
  ]);
  const fallbackHtml = React.useCallback(() => renderPlainDiffVirtualRowsHtmlCore({
    fileKey: options.fileKey,
    rows: workerRows,
    searchLineKeys: options.searchLineKeys,
    activeSearchLineKey: options.activeSearchLineKey,
    contextControls: options.contextControls,
    hunkSeparatorStyle: options.hunkSeparatorStyle,
    indicatorStyle: options.indicatorStyle,
    inlineDiff: options.inlineDiff,
    overflowMode: options.overflowMode,
    viewMode: options.viewMode,
  }), [
    options.activeSearchLineKey,
    options.contextControls,
    options.fileKey,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.overflowMode,
    options.searchLineKeys,
    options.viewMode,
    workerRows,
  ]);
  return usePlainDiffWorkerHtml({
    cacheKey,
    enabled: shouldRender,
    fallbackHtml: shouldRender ? fallbackHtml : undefined,
    request: shouldRender ? request : disabledDiffRenderWorkerRequest,
  });
}

// useHighlightedDiffFileHtml renders one highlighted complete file through the render worker.
// useHighlightedDiffFileHtml 通过渲染 Worker 渲染一个完整高亮文件。
export function useHighlightedDiffFileHtml(options: DiffHighlightedFileWorkerOptions, enabled: boolean): string {
  const theme = useDiffShikiTheme();
  // shouldRender keeps the disabled sibling hook out of highlighted fingerprint work.
  // shouldRender 让未启用的兄弟 hook 跳过高亮指纹计算。
  const shouldRender = enabled;
  // shouldCollectHighlightedSpans prevents disabled highlighted hooks from scanning cached token spans.
  // shouldCollectHighlightedSpans 避免未启用的高亮 hook 扫描缓存 token span。
  const shouldCollectHighlightedSpans = shouldRender;
  // searchPayload is reused by the request builder to avoid duplicate serialization.
  // searchPayload 由请求构建器复用，避免重复序列化搜索 key。
  const searchPayload = React.useMemo(
    () => shouldRender ? serializedSearchPayloadForRequest(options.searchLineKeys) : EMPTY_SERIALIZED_SEARCH_PAYLOAD,
    [options.searchLineKeys, shouldRender],
  );
  const highlightedLineSpans = React.useMemo(
    () => shouldCollectHighlightedSpans ? collectHighlightedFileLineSpans(options.file, options.language, options.highlightRevision, theme) : undefined,
    [options.file, options.highlightRevision, options.language, shouldCollectHighlightedSpans, theme],
  );
  const cacheKey = React.useMemo(() => shouldRender ? highlightedFileRenderCacheKey(options, theme, highlightedLineSpans, searchPayload.signature) : DISABLED_DIFF_RENDER_CACHE_KEY, [
    highlightedLineSpans,
    options.activeSearchLineKey,
    options.contextControls,
    options.file,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.language,
    options.overflowMode,
    searchPayload,
    theme,
    options.viewMode,
    shouldRender,
  ]);
  const request = React.useCallback((): DiffRenderWorkerRequest => {
    const highlightedPayload = highlightedLineSpansWorkerPayload(highlightedLineSpans);
    return {
      id: 0,
      kind: "file",
      file: options.file,
      searchLineKeys: searchPayload.keys,
      searchLineKeySignature: searchPayload.signature,
      activeSearchLineKey: options.activeSearchLineKey,
      contextControls: options.contextControls,
      hunkSeparatorStyle: options.hunkSeparatorStyle,
      indicatorStyle: options.indicatorStyle,
      inlineDiff: options.inlineDiff,
      overflowMode: options.overflowMode,
      viewMode: options.viewMode,
      highlighted: highlightedPayload.highlighted,
      resetHighlightedLineSpans: highlightedPayload.reset,
      highlightedLineSpans: highlightedPayload.updates,
    };
  }, [
    highlightedLineSpans,
    options.activeSearchLineKey,
    options.contextControls,
    options.file,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.overflowMode,
    searchPayload,
    options.viewMode,
  ]);
  const fallbackHtml = React.useCallback(() => renderPlainDiffFileHtmlCore({
    file: options.file,
    searchLineKeys: options.searchLineKeys,
    activeSearchLineKey: options.activeSearchLineKey,
    contextControls: options.contextControls,
    hunkSeparatorStyle: options.hunkSeparatorStyle,
    indicatorStyle: options.indicatorStyle,
    inlineDiff: options.inlineDiff,
    overflowMode: options.overflowMode,
    viewMode: options.viewMode,
  }), [
    options.activeSearchLineKey,
    options.contextControls,
    options.file,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.overflowMode,
    options.searchLineKeys,
    options.viewMode,
  ]);
  return useDiffWorkerHtml({
    cacheKey,
    enabled: shouldRender,
    fallbackHtml: shouldRender ? fallbackHtml : undefined,
    onPostSuccess: rememberPostedHighlightedLineSpans,
    request: shouldRender ? request : disabledDiffRenderWorkerRequest,
  });
}

// useHighlightedDiffRowsHtml renders one highlighted virtual row window through the render worker.
// useHighlightedDiffRowsHtml 通过渲染 Worker 渲染一个高亮虚拟行窗口。
export function useHighlightedDiffRowsHtml(options: DiffHighlightedRowsWorkerOptions, enabled: boolean): string {
  const theme = useDiffShikiTheme();
  // hasRows keeps empty virtual windows out of highlight span collection and render dispatch.
  // hasRows 避免空虚拟窗口进入高亮 span 收集与渲染调度。
  const hasRows = options.rows.length > 0;
  // shouldRender keeps the disabled sibling hook out of highlighted row compaction and fingerprint work.
  // shouldRender 让未启用的兄弟 hook 跳过高亮行压缩与指纹计算。
  const shouldRender = enabled && hasRows;
  // shouldCollectHighlightedSpans prevents disabled highlighted hooks from scanning cached token spans.
  // shouldCollectHighlightedSpans 避免未启用的高亮 hook 扫描缓存 token span。
  const shouldCollectHighlightedSpans = shouldRender;
  // searchPayload is reused by the request builder to avoid duplicate serialization.
  // searchPayload 由请求构建器复用，避免重复序列化搜索 key。
  const searchPayload = React.useMemo(
    () => shouldRender ? serializedSearchPayloadForRequest(options.searchLineKeys) : EMPTY_SERIALIZED_SEARCH_PAYLOAD,
    [options.searchLineKeys, shouldRender],
  );
  const highlightedLineSpans = React.useMemo(
    () => shouldCollectHighlightedSpans ? collectHighlightedRowsLineSpans(options.fileKey, options.rows, options.language, options.highlightRevision, theme) : undefined,
    [options.fileKey, options.highlightRevision, options.language, options.rows, shouldCollectHighlightedSpans, theme],
  );
  // workerRows contains only the fields needed by the render worker.
  // workerRows 仅包含 render worker 所需的字段。
  const workerRows = React.useMemo(() => shouldRender ? diffRenderableRows(options.rows) : EMPTY_DIFF_RENDERABLE_ROWS, [options.rows, shouldRender]);
  const cacheKey = React.useMemo(() => shouldRender ? highlightedRowsRenderCacheKey(options, theme, highlightedLineSpans, searchPayload.signature) : DISABLED_DIFF_RENDER_CACHE_KEY, [
    highlightedLineSpans,
    options.activeSearchLineKey,
    options.contextControls,
    options.fileKey,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.language,
    options.overflowMode,
    options.rows,
    searchPayload,
    theme,
    options.viewMode,
    shouldRender,
  ]);
  const request = React.useCallback((): DiffRenderWorkerRequest => {
    const highlightedPayload = highlightedLineSpansWorkerPayload(highlightedLineSpans);
    return {
      id: 0,
      kind: "rows",
      fileKey: options.fileKey,
      rows: workerRows,
      searchLineKeys: searchPayload.keys,
      searchLineKeySignature: searchPayload.signature,
      activeSearchLineKey: options.activeSearchLineKey,
      contextControls: options.contextControls,
      hunkSeparatorStyle: options.hunkSeparatorStyle,
      indicatorStyle: options.indicatorStyle,
      inlineDiff: options.inlineDiff,
      overflowMode: options.overflowMode,
      viewMode: options.viewMode,
      highlighted: highlightedPayload.highlighted,
      resetHighlightedLineSpans: highlightedPayload.reset,
      highlightedLineSpans: highlightedPayload.updates,
    };
  }, [
    highlightedLineSpans,
    options.activeSearchLineKey,
    options.contextControls,
    options.fileKey,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.overflowMode,
    searchPayload,
    options.viewMode,
    workerRows,
  ]);
  const fallbackHtml = React.useCallback(() => renderPlainDiffVirtualRowsHtmlCore({
    fileKey: options.fileKey,
    rows: workerRows,
    searchLineKeys: options.searchLineKeys,
    activeSearchLineKey: options.activeSearchLineKey,
    contextControls: options.contextControls,
    hunkSeparatorStyle: options.hunkSeparatorStyle,
    indicatorStyle: options.indicatorStyle,
    inlineDiff: options.inlineDiff,
    overflowMode: options.overflowMode,
    viewMode: options.viewMode,
  }), [
    options.activeSearchLineKey,
    options.contextControls,
    options.fileKey,
    options.hunkSeparatorStyle,
    options.indicatorStyle,
    options.inlineDiff,
    options.overflowMode,
    options.searchLineKeys,
    options.viewMode,
    workerRows,
  ]);
  return useDiffWorkerHtml({
    cacheKey,
    enabled: shouldRender,
    fallbackHtml: shouldRender ? fallbackHtml : undefined,
    onPostSuccess: rememberPostedHighlightedLineSpans,
    request: shouldRender ? request : disabledDiffRenderWorkerRequest,
  });
}

// nextDiffWorkerHtmlState reuses the previous state when cache key and HTML are unchanged.
// nextDiffWorkerHtmlState 在缓存 key 与 HTML 均未变化时复用上一份状态。
function nextDiffWorkerHtmlState(
  // Previous hook state.
  // 上一次 hook 状态。
  previous: DiffWorkerHtmlState,
  // Next cache key.
  // 下一次缓存 key。
  cacheKey: string,
  // Next rendered HTML.
  // 下一次已渲染 HTML。
  html: string,
): DiffWorkerHtmlState {
  if (previous.cacheKey === cacheKey && previous.html === html) {
    return previous;
  }
  return { cacheKey, html };
}

// diffWorkerFallbackHtml safely computes a synchronous visible fallback for cache misses.
// diffWorkerFallbackHtml 为缓存未命中安全计算同步可见兜底 HTML。
function diffWorkerFallbackHtml(fallbackHtml: DiffRenderFallbackHtml | undefined): string {
  if (!fallbackHtml) {
    return "";
  }
  try {
    return fallbackHtml();
  } catch (error) {
    return diffRenderWorkerFailureHtml(error instanceof Error ? error.message : String(error));
  }
}

// useDiffWorkerHtml owns one async worker render lifecycle with an optional same-window fallback.
// useDiffWorkerHtml 管理一次异步 Worker 渲染生命周期，并支持同窗口可选兜底。
function useDiffWorkerHtml(options: {
  // Stable cache key for this render request.
  // 当前渲染请求的稳定缓存 key。
  cacheKey: string;
  // Whether the worker render path is active.
  // Worker 渲染路径是否启用。
  enabled: boolean;
  // Called after a worker request is successfully posted.
  // Worker 请求成功发送后调用。
  onPostSuccess?: (request: DiffRenderWorkerRequest) => void;
  // Request factory without id.
  // 不含 id 的请求构造器。
  request: () => DiffRenderWorkerRequest;
  // Optional synchronous HTML used only until the worker result is ready.
  // 仅在 Worker 结果就绪前使用的可选同步 HTML。
  fallbackHtml?: DiffRenderFallbackHtml;
}): string {
  // requestRef keeps the latest request factory without making the render effect restart on equivalent cache keys.
  // requestRef 保存最新请求构造器，避免等价 cache key 下因函数引用变化重启渲染 effect。
  const requestRef = React.useRef(options.request);
  requestRef.current = options.request;
  const [state, setState] = React.useState<DiffWorkerHtmlState>(() => ({
    cacheKey: options.cacheKey,
    html: diffRenderHtmlCache.get(options.cacheKey) ?? diffWorkerFallbackHtml(options.fallbackHtml),
  }));
  React.useEffect(() => {
    if (!options.enabled) {
      setState((previous) => nextDiffWorkerHtmlState(previous, options.cacheKey, ""));
      return undefined;
    }
    const cached = diffRenderHtmlCache.get(options.cacheKey);
    if (cached != null) {
      setState((previous) => nextDiffWorkerHtmlState(previous, options.cacheKey, cached));
      return undefined;
    }
    const fallbackHtml = diffWorkerFallbackHtml(options.fallbackHtml);
    const worker = currentDiffRenderWorker();
    if (!worker) {
      const failureHtml = diffRenderWorkerFailureHtml("Diff render worker is unavailable.");
      rememberDiffRenderHtml(options.cacheKey, failureHtml);
      setState((previous) => nextDiffWorkerHtmlState(previous, options.cacheKey, failureHtml));
      return undefined;
    }
    let cancelled = false;
    let postFrame: number | null = null;
    const requestId = allocateDiffRenderWorkerRequestId();
    setState((previous) => nextDiffWorkerHtmlState(previous, options.cacheKey, fallbackHtml));
    diffRenderWorkerCallbacks.set(requestId, (response) => {
      if (cancelled) {
        return;
      }
      const nextHtml = response.html && !response.error
        ? response.html
        : diffRenderWorkerFailureHtml(response.error ?? "Diff render worker failed.");
      rememberDiffRenderHtml(options.cacheKey, nextHtml);
      setState((previous) => nextDiffWorkerHtmlState(previous, options.cacheKey, nextHtml));
    });
    // Deferring the worker post by one frame lets fast virtual-scroll updates cancel stale windows before they enter the worker queue.
    // 延迟一帧发送 Worker 请求，让快速虚拟滚动更新能在进入 Worker 队列前取消过期窗口。
    const postRequest = () => {
      postFrame = null;
      if (cancelled) {
        return;
      }
      const request = { ...requestRef.current(), id: requestId };
      try {
        enqueueDiffRenderWorkerRequest(worker, request, () => {
          diffRenderWorkerCallbacks.delete(requestId);
          if (cancelled) {
            return;
          }
          const failureHtml = diffRenderWorkerFailureHtml("Diff render worker postMessage failed.");
          rememberDiffRenderHtml(options.cacheKey, failureHtml);
          setState((previous) => nextDiffWorkerHtmlState(previous, options.cacheKey, failureHtml));
        }, options.onPostSuccess);
      } catch {
        diffRenderWorkerCallbacks.delete(requestId);
        diffRenderRequestScopes.delete(requestId);
      }
    };
    if (typeof window === "undefined") {
      postRequest();
    } else {
      postFrame = window.requestAnimationFrame(postRequest);
    }
    return () => {
      cancelled = true;
      if (postFrame != null) {
        window.cancelAnimationFrame(postFrame);
      }
      cancelQueuedDiffRenderWorkerRequest(requestId);
    };
  }, [options.cacheKey, options.enabled, options.fallbackHtml, options.onPostSuccess]);
  return state.cacheKey === options.cacheKey
    ? state.html
    : diffRenderHtmlCache.get(options.cacheKey) ?? diffWorkerFallbackHtml(options.fallbackHtml);
}

// usePlainDiffWorkerHtml owns one plain render lifecycle through the shared worker hook.
// usePlainDiffWorkerHtml 通过共享 Worker hook 管理一次纯文本渲染生命周期。
function usePlainDiffWorkerHtml(options: {
  // Stable cache key for this render request.
  // 当前渲染请求的稳定缓存 key。
  cacheKey: string;
  // Whether the worker render path is active.
  // Worker 渲染路径是否启用。
  enabled: boolean;
  // Called after a worker request is successfully posted.
  // Worker 请求成功发送后调用。
  onPostSuccess?: (request: DiffRenderWorkerRequest) => void;
  // Request factory without id.
  // 不含 id 的请求构造器。
  request: () => DiffRenderWorkerRequest;
  // Optional synchronous HTML used only until the worker result is ready.
  // 仅在 Worker 结果就绪前使用的可选同步 HTML。
  fallbackHtml?: DiffRenderFallbackHtml;
}): string {
  return useDiffWorkerHtml(options);
}

