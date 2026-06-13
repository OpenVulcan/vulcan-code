import * as React from "react";

import {
  EMPTY_DIFF_SEARCH_INDEX,
  type DiffSearchIndex,
} from "./diff-search";
import { DIFF_SEARCH_WORKER_REQUEST_DEBOUNCE_MS } from "./limits";
import type { DiffPanelSnapshot } from "./types";

// DiffSearchWorkerRequest asks the search worker to index one snapshot/query pair.
// DiffSearchWorkerRequest 请求搜索 Worker 索引一组 snapshot/query。
interface DiffSearchWorkerRequest {
  // Request id used to match worker responses.
  // 用于匹配 Worker 响应的请求标识。
  id: number;
  // Stable key for the parsed snapshot cached inside the worker.
  // Worker 内缓存的已解析 snapshot 稳定 key。
  snapshotKey: string;
  // Parsed diff snapshot to search.
  // 需要搜索的已解析 diff 快照。
  snapshot?: DiffPanelSnapshot;
  // Raw query text.
  // 原始查询文本。
  query: string;
  // Whether this message only asks the worker to preempt the current scan.
  // 当前消息是否仅要求 Worker 抢占正在执行的扫描。
  cancelOnly?: boolean;
}

// DiffSearchWorkerResponse carries a computed index or an error.
// DiffSearchWorkerResponse 承载计算后的索引或错误。
interface DiffSearchWorkerResponse {
  // Request id echoed from the request.
  // 从请求中回传的请求标识。
  id: number;
  // Computed index when available.
  // 可用时的计算结果索引。
  index?: DiffSearchIndex;
  // Optional worker error text.
  // 可选 Worker 错误文本。
  error?: string;
  // Whether the worker dropped this request because a newer query superseded it.
  // Worker 是否因为更新查询取代当前请求而丢弃了它。
  cancelled?: boolean;
}

// DiffSearchWorkerState is the React-facing async search result.
// DiffSearchWorkerState 是暴露给 React 的异步搜索结果。
export interface DiffSearchWorkerState {
  // Current search index.
  // 当前搜索索引。
  index: DiffSearchIndex;
  // Whether the latest non-empty query is still being indexed.
  // 最新非空查询是否仍在索引中。
  pending: boolean;
  // Optional worker error when the index could not be built.
  // 索引无法构建时的可选 Worker 错误。
  error: string | null;
}

// EMPTY_DIFF_SEARCH_WORKER_STATE is the stable idle state for empty or unavailable queries.
// EMPTY_DIFF_SEARCH_WORKER_STATE 是空查询或无可用查询时复用的稳定空闲状态。
const EMPTY_DIFF_SEARCH_WORKER_STATE: DiffSearchWorkerState = {
  index: EMPTY_DIFF_SEARCH_INDEX,
  pending: false,
  error: null,
};

// PENDING_DIFF_SEARCH_WORKER_STATE is reused while the worker indexes the latest query.
// PENDING_DIFF_SEARCH_WORKER_STATE 在 Worker 索引最新查询时复用。
const PENDING_DIFF_SEARCH_WORKER_STATE: DiffSearchWorkerState = {
  index: EMPTY_DIFF_SEARCH_INDEX,
  pending: true,
  error: null,
};

// DIFF_SEARCH_WORKER_UNAVAILABLE_STATE is reused after worker construction is unavailable.
// DIFF_SEARCH_WORKER_UNAVAILABLE_STATE 在搜索 Worker 不可构造时复用。
const DIFF_SEARCH_WORKER_UNAVAILABLE_STATE: DiffSearchWorkerState = {
  index: EMPTY_DIFF_SEARCH_INDEX,
  pending: false,
  error: "Diff search worker is unavailable.",
};

// DIFF_SEARCH_WORKER_POST_FAILURE_STATE is reused when a worker request cannot be posted.
// DIFF_SEARCH_WORKER_POST_FAILURE_STATE 在搜索 Worker 请求发送失败时复用。
const DIFF_SEARCH_WORKER_POST_FAILURE_STATE: DiffSearchWorkerState = {
  index: EMPTY_DIFF_SEARCH_INDEX,
  pending: false,
  error: "Diff search worker request could not be posted.",
};

// DIFF_SEARCH_WORKER_FAILURE_STATE is reused for the common worker failure response.
// DIFF_SEARCH_WORKER_FAILURE_STATE 复用常见的搜索 Worker 失败状态。
const DIFF_SEARCH_WORKER_FAILURE_STATE: DiffSearchWorkerState = {
  index: EMPTY_DIFF_SEARCH_INDEX,
  pending: false,
  error: "Diff search worker failed.",
};

// SearchWorkerCallback resolves one in-flight search request.
// SearchWorkerCallback 解析一个进行中的搜索请求。
type SearchWorkerCallback = (response: DiffSearchWorkerResponse) => void;

// DiffSearchQueuedRequest stores one worker request that may be coalesced before posting.
// DiffSearchQueuedRequest 保存一个可在发送前合并的 Worker 请求。
interface DiffSearchQueuedRequest {
  // Request id used to match worker responses.
  // 用于匹配 Worker 响应的请求标识。
  id: number;
  // Worker instance that should receive this request.
  // 应接收该请求的 Worker 实例。
  worker: Worker;
  // Complete worker request with the assigned id.
  // 已分配请求标识的完整 Worker 请求。
  request: DiffSearchWorkerRequest;
  // Called after postMessage succeeds so snapshot clone state stays accurate.
  // postMessage 成功后调用，保持 snapshot 克隆状态准确。
  onPostSuccess: () => void;
  // Called when postMessage fails for this request.
  // 当前请求发送失败时调用。
  onPostFailure: () => void;
}

// diffSearchWorker stores the lazily created search worker.
// diffSearchWorker 保存懒创建的搜索 Worker。
let diffSearchWorker: Worker | null = null;

// diffSearchWorkerFailed disables worker retries after construction or runtime failure.
// diffSearchWorkerFailed 在 Worker 构造或运行失败后禁用重试。
let diffSearchWorkerFailed = false;

// nextSearchWorkerRequestId increments for every search request.
// nextSearchWorkerRequestId 为每个搜索请求递增。
let nextSearchWorkerRequestId = 0;

// nextSearchWorkerSnapshotId assigns stable keys to parsed snapshot objects.
// nextSearchWorkerSnapshotId 为已解析 snapshot 对象分配稳定 key。
let nextSearchWorkerSnapshotId = 0;

// postedDiffSearchWorkerSnapshotKey tracks the latest snapshot cloned into the worker.
// postedDiffSearchWorkerSnapshotKey 记录最近一次已克隆进 Worker 的 snapshot key。
let postedDiffSearchWorkerSnapshotKey: string | null = null;

// diffSearchInFlightRequestId stores the request currently running inside the worker.
// diffSearchInFlightRequestId 保存当前正在 Worker 内执行的请求标识。
let diffSearchInFlightRequestId: number | null = null;

// diffSearchPendingRequest keeps only the newest queued search request while one is running.
// diffSearchPendingRequest 在已有请求执行时仅保留最新的等待搜索请求。
let diffSearchPendingRequest: DiffSearchQueuedRequest | null = null;

// searchWorkerCallbacks maps request ids to live callbacks.
// searchWorkerCallbacks 将请求标识映射到存活回调。
const searchWorkerCallbacks = new Map<number, SearchWorkerCallback>();

// diffSearchSnapshotKeys stores main-thread snapshot identities without retaining old snapshots.
// diffSearchSnapshotKeys 按主线程 snapshot 身份存储 key，且不阻止旧 snapshot 回收。
const diffSearchSnapshotKeys = new WeakMap<DiffPanelSnapshot, string>();

// useDiffSearchIndex builds the loaded-line search index through the worker-only path.
// useDiffSearchIndex 通过仅 Worker 路径构建已加载行搜索索引。
export function useDiffSearchIndex(snapshot: DiffPanelSnapshot | null, query: string): DiffSearchWorkerState {
  const normalizedQuery = React.useMemo(() => query.trim(), [query]);
  const [state, setState] = React.useState<DiffSearchWorkerState>(EMPTY_DIFF_SEARCH_WORKER_STATE);
  React.useEffect(() => {
    if (!snapshot || !normalizedQuery) {
      setState(EMPTY_DIFF_SEARCH_WORKER_STATE);
      return undefined;
    }
    const worker = currentDiffSearchWorker();
    if (!worker) {
      setState(DIFF_SEARCH_WORKER_UNAVAILABLE_STATE);
      return undefined;
    }
    let cancelled = false;
    let postTimeout: number | null = null;
    const requestId = ++nextSearchWorkerRequestId;
    const snapshotKey = diffSearchSnapshotKey(snapshot);
    setState(PENDING_DIFF_SEARCH_WORKER_STATE);
    searchWorkerCallbacks.set(requestId, (response) => {
      if (cancelled) {
        return;
      }
      if (response.index && !response.error) {
        setState({ index: response.index, pending: false, error: null });
      } else {
        setState(diffSearchWorkerErrorState(response.error));
      }
    });
    const postRequest = () => {
      postTimeout = null;
      if (cancelled) {
        return;
      }
      const shouldPostSnapshot = postedDiffSearchWorkerSnapshotKey !== snapshotKey;
      const request: DiffSearchWorkerRequest = {
        id: requestId,
        snapshotKey,
        snapshot: shouldPostSnapshot ? snapshot : undefined,
        query: normalizedQuery,
      };
      try {
        enqueueDiffSearchWorkerRequest(worker, request, () => {
          if (shouldPostSnapshot) {
            postedDiffSearchWorkerSnapshotKey = snapshotKey;
          }
        }, () => {
          searchWorkerCallbacks.delete(requestId);
          if (!cancelled) {
            setState(DIFF_SEARCH_WORKER_POST_FAILURE_STATE);
          }
        });
      } catch {
        searchWorkerCallbacks.delete(requestId);
        if (!cancelled) {
          setState(DIFF_SEARCH_WORKER_POST_FAILURE_STATE);
        }
      }
    };
    if (typeof window === "undefined") {
      postRequest();
    } else {
      postTimeout = window.setTimeout(postRequest, DIFF_SEARCH_WORKER_REQUEST_DEBOUNCE_MS);
    }
    return () => {
      cancelled = true;
      if (postTimeout != null) {
        window.clearTimeout(postTimeout);
      }
      cancelQueuedDiffSearchWorkerRequest(requestId);
    };
  }, [normalizedQuery, snapshot]);
  return state;
}

// diffSearchWorkerErrorState returns a stable state for common worker errors.
// diffSearchWorkerErrorState 为常见搜索 Worker 错误返回稳定状态。
function diffSearchWorkerErrorState(error?: string): DiffSearchWorkerState {
  return error === DIFF_SEARCH_WORKER_FAILURE_STATE.error || error === "Diff search worker failed" || !error
    ? DIFF_SEARCH_WORKER_FAILURE_STATE
    : { index: EMPTY_DIFF_SEARCH_INDEX, pending: false, error };
}

// diffSearchSnapshotKey returns a stable per-object key for worker-side snapshot reuse.
// diffSearchSnapshotKey 返回用于 Worker 侧复用 snapshot 的对象级稳定 key。
function diffSearchSnapshotKey(snapshot: DiffPanelSnapshot): string {
  const existing = diffSearchSnapshotKeys.get(snapshot);
  if (existing) {
    return existing;
  }
  const nextKey = `diff-search-snapshot-${++nextSearchWorkerSnapshotId}`;
  diffSearchSnapshotKeys.set(snapshot, nextKey);
  return nextKey;
}

// currentDiffSearchWorker returns the singleton search worker when available.
// currentDiffSearchWorker 在可用时返回单例搜索 Worker。
function currentDiffSearchWorker(): Worker | null {
  if (diffSearchWorkerFailed || typeof Worker === "undefined") {
    return null;
  }
  if (!diffSearchWorker) {
    try {
      diffSearchWorker = new Worker(new URL("./diff-search-worker.ts", import.meta.url), {
        type: "module",
      });
      diffSearchWorker.onmessage = handleDiffSearchWorkerMessage;
      diffSearchWorker.onerror = handleDiffSearchWorkerFailure;
      diffSearchWorker.onmessageerror = handleDiffSearchWorkerFailure;
    } catch {
      diffSearchWorkerFailed = true;
      return null;
    }
  }
  return diffSearchWorker;
}

// handleDiffSearchWorkerMessage dispatches one worker response to its callback.
// handleDiffSearchWorkerMessage 将单个 Worker 响应分发给对应回调。
function handleDiffSearchWorkerMessage(event: MessageEvent<DiffSearchWorkerResponse>): void {
  const response = event.data;
  const callback = searchWorkerCallbacks.get(response.id);
  searchWorkerCallbacks.delete(response.id);
  finishDiffSearchWorkerRequest(response.id);
  if (response.cancelled) {
    return;
  }
  callback?.(response);
}

// handleDiffSearchWorkerFailure disables the worker and releases pending callbacks.
// handleDiffSearchWorkerFailure 禁用 Worker 并释放等待中的回调。
function handleDiffSearchWorkerFailure(): void {
  diffSearchWorkerFailed = true;
  diffSearchWorker?.terminate();
  diffSearchWorker = null;
  postedDiffSearchWorkerSnapshotKey = null;
  diffSearchInFlightRequestId = null;
  diffSearchPendingRequest = null;
  const callbacks = [...searchWorkerCallbacks.values()];
  searchWorkerCallbacks.clear();
  for (const callback of callbacks) {
    callback({ id: 0, error: "Diff search worker failed" });
  }
}

// enqueueDiffSearchWorkerRequest posts immediately or replaces the latest queued search.
// enqueueDiffSearchWorkerRequest 立即发送请求，或替换最新等待搜索请求。
function enqueueDiffSearchWorkerRequest(
  worker: Worker,
  request: DiffSearchWorkerRequest,
  onPostSuccess: () => void,
  onPostFailure: () => void,
): void {
  const queuedRequest: DiffSearchQueuedRequest = {
    id: request.id,
    worker,
    request,
    onPostSuccess,
    onPostFailure,
  };
  if (diffSearchInFlightRequestId != null) {
    const inFlightRequestId = diffSearchInFlightRequestId;
    if (diffSearchPendingRequest) {
      searchWorkerCallbacks.delete(diffSearchPendingRequest.id);
    }
    diffSearchPendingRequest = queuedRequest;
    if (cancelInFlightDiffSearchWorkerRequest(worker, inFlightRequestId)) {
      finishDiffSearchWorkerRequest(inFlightRequestId);
    }
    return;
  }
  postQueuedDiffSearchWorkerRequest(queuedRequest);
}

// postQueuedDiffSearchWorkerRequest sends one queued search when its callback is still live.
// postQueuedDiffSearchWorkerRequest 在回调仍存活时发送单个等待搜索。
function postQueuedDiffSearchWorkerRequest(queuedRequest: DiffSearchQueuedRequest): void {
  if (!searchWorkerCallbacks.has(queuedRequest.id)) {
    return;
  }
  diffSearchInFlightRequestId = queuedRequest.id;
  try {
    queuedRequest.worker.postMessage(queuedRequest.request);
    queuedRequest.onPostSuccess();
  } catch {
    finishDiffSearchWorkerRequest(queuedRequest.id);
    queuedRequest.onPostFailure();
  }
}

// cancelInFlightDiffSearchWorkerRequest sends preemption and reports whether local release is safe.
// cancelInFlightDiffSearchWorkerRequest 发送抢占消息，并报告本地释放执行槽是否安全。
function cancelInFlightDiffSearchWorkerRequest(worker: Worker, requestId: number): boolean {
  try {
    worker.postMessage({
      id: requestId,
      snapshotKey: "",
      query: "",
      cancelOnly: true,
    } satisfies DiffSearchWorkerRequest);
    return true;
  } catch {
    // A failed cancel message is non-fatal; the original in-flight request will still finish.
    // cancel 消息失败不是致命问题；原执行中请求仍会自然完成。
    return false;
  }
}

// finishDiffSearchWorkerRequest releases the running slot and posts the newest pending search.
// finishDiffSearchWorkerRequest 释放执行槽位，并发送最新等待搜索。
function finishDiffSearchWorkerRequest(requestId: number): void {
  if (diffSearchInFlightRequestId === requestId) {
    diffSearchInFlightRequestId = null;
  }
  const pendingRequest = diffSearchPendingRequest;
  if (!pendingRequest || diffSearchInFlightRequestId != null) {
    return;
  }
  diffSearchPendingRequest = null;
  postQueuedDiffSearchWorkerRequest(pendingRequest);
}

// cancelQueuedDiffSearchWorkerRequest drops callbacks and unposted pending work for one request id.
// cancelQueuedDiffSearchWorkerRequest 按请求标识丢弃回调与尚未发送的等待任务。
function cancelQueuedDiffSearchWorkerRequest(requestId: number): void {
  searchWorkerCallbacks.delete(requestId);
  if (diffSearchPendingRequest?.id === requestId) {
    diffSearchPendingRequest = null;
  }
  if (
    diffSearchInFlightRequestId === requestId &&
    diffSearchWorker &&
    cancelInFlightDiffSearchWorkerRequest(diffSearchWorker, requestId)
  ) {
    finishDiffSearchWorkerRequest(requestId);
  }
}
