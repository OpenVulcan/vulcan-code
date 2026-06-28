import { escapeHtml } from "../diff-shiki-shared";
import { DIFF_RENDER_WORKER_CACHE_LIMIT } from "./constants";
import { resetDiffRenderWorkerHighlightedSpanCacheMirror } from "./highlighted-spans";
import type { DiffRenderQueuedRequest, DiffRenderWorkerCallback, DiffRenderWorkerCancelRequest, DiffRenderWorkerRequest, DiffRenderWorkerResponse } from "./types";
// diffRenderWorker stores the lazily created render worker.
// diffRenderWorker 保存懒创建的渲染 Worker。
let diffRenderWorker: Worker | null = null;

// diffRenderWorkerFailed disables worker retries after construction or runtime failure.
// diffRenderWorkerFailed 在 Worker 构建或运行失败后禁用重试。
let diffRenderWorkerFailed = false;

// nextDiffRenderWorkerRequestId increments for every render request.
// nextDiffRenderWorkerRequestId 为每个渲染请求递增。
let nextDiffRenderWorkerRequestId = 0;

// allocateDiffRenderWorkerRequestId returns the next unique render worker request id.
// allocateDiffRenderWorkerRequestId 返回下一个唯一渲染 Worker 请求标识。
export function allocateDiffRenderWorkerRequestId(): number {
  nextDiffRenderWorkerRequestId += 1;
  return nextDiffRenderWorkerRequestId;
}

// diffRenderWorkerCallbacks maps request ids to live callbacks.
// diffRenderWorkerCallbacks 将请求标识映射到存活回调。
export const diffRenderWorkerCallbacks = new Map<number, DiffRenderWorkerCallback>();

// diffRenderRequestScopes maps request ids back to their coalescing scope.
// diffRenderRequestScopes 将请求标识映射回其合并 scope。
export const diffRenderRequestScopes = new Map<number, string>();

// diffRenderInFlightScopes tracks the active request for each render scope.
// diffRenderInFlightScopes 跟踪每个渲染 scope 当前正在执行的请求。
const diffRenderInFlightScopes = new Map<string, number>();

// diffRenderPendingScopes keeps only the latest queued request per render scope.
// diffRenderPendingScopes 对每个渲染 scope 只保留最新排队请求。
const diffRenderPendingScopes = new Map<string, DiffRenderQueuedRequest>();

// diffRenderHtmlCache stores rendered HTML blocks shared across remounts and scroll windows.
// diffRenderHtmlCache 缓存跨重挂载和滚动窗口复用的渲染 HTML 块。
export const diffRenderHtmlCache = new Map<string, string>();
// diffRenderWorkerFailureHtml builds a tiny new-pipeline failure block instead of rendering diff on the main thread.
// diffRenderWorkerFailureHtml 构建轻量的新链路失败块，避免在主线程渲染 diff。
export function diffRenderWorkerFailureHtml(message: string): string {
  return `<div class="diff-render-worker-note"><span>Diff render worker unavailable</span><small>${escapeHtml(message)}</small></div>`;
}

// currentDiffRenderWorker returns the singleton render worker when available.
// currentDiffRenderWorker 在可用时返回单例渲染 Worker。
export function currentDiffRenderWorker(): Worker | null {
  if (diffRenderWorkerFailed || typeof Worker === "undefined") {
    return null;
  }
  if (!diffRenderWorker) {
    try {
      resetDiffRenderWorkerHighlightedSpanCacheMirror();
      diffRenderWorker = new Worker(new URL("../diff-render-worker.ts", import.meta.url), {
        type: "module",
      });
      diffRenderWorker.onmessage = handleDiffRenderWorkerMessage;
      diffRenderWorker.onerror = handleDiffRenderWorkerFailure;
      diffRenderWorker.onmessageerror = handleDiffRenderWorkerFailure;
    } catch {
      diffRenderWorkerFailed = true;
      return null;
    }
  }
  return diffRenderWorker;
}

// handleDiffRenderWorkerMessage dispatches one worker response to its callback.
// handleDiffRenderWorkerMessage 将单个 Worker 响应分发给对应回调。
function handleDiffRenderWorkerMessage(event: MessageEvent<DiffRenderWorkerResponse>): void {
  const response = event.data;
  const callback = diffRenderWorkerCallbacks.get(response.id);
  diffRenderWorkerCallbacks.delete(response.id);
  finishDiffRenderWorkerRequest(response.id);
  callback?.(response);
}

// handleDiffRenderWorkerFailure disables the worker and releases pending callbacks.
// handleDiffRenderWorkerFailure 禁用 Worker 并释放等待中的回调。
function handleDiffRenderWorkerFailure(): void {
  diffRenderWorkerFailed = true;
  diffRenderWorker?.terminate();
  diffRenderWorker = null;
  resetDiffRenderWorkerHighlightedSpanCacheMirror();
  const callbacks = [...diffRenderWorkerCallbacks.values()];
  diffRenderWorkerCallbacks.clear();
  diffRenderRequestScopes.clear();
  diffRenderInFlightScopes.clear();
  diffRenderPendingScopes.clear();
  for (const callback of callbacks) {
    callback({ id: 0, error: "Diff render worker failed" });
  }
}

// enqueueDiffRenderWorkerRequest posts one request or replaces the latest pending request for the same scope.
// enqueueDiffRenderWorkerRequest 发送一个请求，或替换同 scope 的最新等待请求。
export function enqueueDiffRenderWorkerRequest(
  worker: Worker,
  request: DiffRenderWorkerRequest,
  onPostFailure: () => void,
  onPostSuccess?: (request: DiffRenderWorkerRequest) => void,
): void {
  const scopeKey = diffRenderRequestScopeKey(request);
  const queuedRequest: DiffRenderQueuedRequest = {
    id: request.id,
    worker,
    scopeKey,
    request,
    onPostFailure,
    onPostSuccess,
  };
  diffRenderRequestScopes.set(request.id, scopeKey);
  if (diffRenderInFlightScopes.has(scopeKey)) {
    const previousPending = diffRenderPendingScopes.get(scopeKey);
    if (previousPending) {
      diffRenderWorkerCallbacks.delete(previousPending.id);
      diffRenderRequestScopes.delete(previousPending.id);
    }
    diffRenderPendingScopes.set(scopeKey, queuedRequest);
    return;
  }
  postQueuedDiffRenderWorkerRequest(queuedRequest);
}

// postQueuedDiffRenderWorkerRequest sends a scoped request to the worker when its callback is still live.
// postQueuedDiffRenderWorkerRequest 在回调仍存活时向 Worker 发送分 scope 请求。
function postQueuedDiffRenderWorkerRequest(queuedRequest: DiffRenderQueuedRequest): void {
  if (!diffRenderWorkerCallbacks.has(queuedRequest.id)) {
    diffRenderRequestScopes.delete(queuedRequest.id);
    return;
  }
  diffRenderInFlightScopes.set(queuedRequest.scopeKey, queuedRequest.id);
  try {
    queuedRequest.worker.postMessage(queuedRequest.request);
    queuedRequest.onPostSuccess?.(queuedRequest.request);
  } catch {
    finishDiffRenderWorkerRequest(queuedRequest.id);
    queuedRequest.onPostFailure();
  }
}

// finishDiffRenderWorkerRequest releases a completed scope and posts its newest pending request.
// finishDiffRenderWorkerRequest 释放已完成 scope，并发送该 scope 最新等待请求。
function finishDiffRenderWorkerRequest(requestId: number): void {
  const scopeKey = diffRenderRequestScopes.get(requestId);
  if (!scopeKey) {
    return;
  }
  diffRenderRequestScopes.delete(requestId);
  if (diffRenderInFlightScopes.get(scopeKey) === requestId) {
    diffRenderInFlightScopes.delete(scopeKey);
  }
  const pendingRequest = diffRenderPendingScopes.get(scopeKey);
  if (!pendingRequest) {
    return;
  }
  diffRenderPendingScopes.delete(scopeKey);
  postQueuedDiffRenderWorkerRequest(pendingRequest);
}

// cancelQueuedDiffRenderWorkerRequest drops callbacks and pending work for one request id.
// cancelQueuedDiffRenderWorkerRequest 按请求标识丢弃回调和等待任务。
export function cancelQueuedDiffRenderWorkerRequest(requestId: number): void {
  diffRenderWorkerCallbacks.delete(requestId);
  const scopeKey = diffRenderRequestScopes.get(requestId);
  if (!scopeKey) {
    return;
  }
  const pendingRequest = diffRenderPendingScopes.get(scopeKey);
  if (pendingRequest?.id === requestId) {
    diffRenderPendingScopes.delete(scopeKey);
    diffRenderRequestScopes.delete(requestId);
    return;
  }
  if (diffRenderInFlightScopes.get(scopeKey) === requestId) {
    cancelInFlightDiffRenderWorkerRequest(requestId);
    finishDiffRenderWorkerRequest(requestId);
  }
}

// cancelInFlightDiffRenderWorkerRequest notifies the worker that an obsolete request can stop early.
// cancelInFlightDiffRenderWorkerRequest 通知 Worker 一个过期请求可以提前停止。
function cancelInFlightDiffRenderWorkerRequest(requestId: number): void {
  try {
    diffRenderWorker?.postMessage({ id: requestId, kind: "cancel" } satisfies DiffRenderWorkerCancelRequest);
  } catch {
    // Cancellation is best-effort; local scope bookkeeping is released separately.
    // 取消是尽力而为；本地 scope 记录会单独释放。
  }
}

// diffRenderRequestScopeKey returns the coalescing scope for one render request.
// diffRenderRequestScopeKey 返回单个渲染请求的合并 scope。
function diffRenderRequestScopeKey(request: DiffRenderWorkerRequest): string {
  const modeKey = diffRenderRequestModeKey(request);
  return request.kind === "rows"
    ? `rows\u0000${modeKey}\u0000${request.fileKey}`
    : `file\u0000${modeKey}\u0000${request.file.key}`;
}

// diffRenderRequestModeKey returns the render mode portion of a coalescing scope.
// diffRenderRequestModeKey 返回合并 scope 中的渲染模式片段。
function diffRenderRequestModeKey(request: DiffRenderWorkerRequest): string {
  return request.highlighted || request.highlightedLineSpans?.length ? "highlight" : "plain";
}

// rememberDiffRenderHtml stores one rendered HTML block with FIFO eviction.
// rememberDiffRenderHtml 使用 FIFO 淘汰策略保存一个渲染 HTML 块。
export function rememberDiffRenderHtml(cacheKey: string, html: string): void {
  if (diffRenderHtmlCache.has(cacheKey)) {
    diffRenderHtmlCache.delete(cacheKey);
  } else if (diffRenderHtmlCache.size >= DIFF_RENDER_WORKER_CACHE_LIMIT) {
    const oldestKey = diffRenderHtmlCache.keys().next().value;
    if (oldestKey) {
      diffRenderHtmlCache.delete(oldestKey);
    }
  }
  diffRenderHtmlCache.set(cacheKey, html);
}

