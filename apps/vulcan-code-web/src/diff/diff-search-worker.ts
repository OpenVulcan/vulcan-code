import { buildDiffSearchIndexSliced, type DiffSearchIndex } from "./diff-search";
import { DIFF_SEARCH_WORKER_INDEX_CACHE_LIMIT } from "./limits";
import type { DiffPanelSnapshot } from "./types";

// DiffSearchWorkerRequest asks the worker to build a loaded-hunk search index.
// DiffSearchWorkerRequest 请求 Worker 构建已加载 hunk 的搜索索引。
interface DiffSearchWorkerRequest {
  // Request id echoed back to the main thread.
  // 回传给主线程的请求标识。
  id: number;
  // Stable key for the parsed snapshot cached inside this worker.
  // 此 Worker 内缓存的已解析 snapshot 稳定 key。
  snapshotKey: string;
  // Parsed diff snapshot to search.
  // 需要搜索的已解析 diff 快照。
  snapshot?: DiffPanelSnapshot;
  // Raw query text from the toolbar.
  // 工具栏输入的原始查询文本。
  query: string;
  // Whether this message only supersedes the current scan without starting a new search.
  // 当前消息是否只取代正在执行的扫描而不启动新搜索。
  cancelOnly?: boolean;
}

// DiffSearchWorkerResponse returns the computed search index or an error.
// DiffSearchWorkerResponse 返回计算后的搜索索引或错误。
interface DiffSearchWorkerResponse {
  // Request id echoed from the request.
  // 从请求中回传的请求标识。
  id: number;
  // Computed search index when successful.
  // 成功时计算出的搜索索引。
  index?: DiffSearchIndex;
  // Optional error message returned to the worker-only caller.
  // 返回给 Worker 单轨调用方的可选错误信息。
  error?: string;
  // Whether this request was superseded by a newer query before it finished.
  // 当前请求是否在完成前被更新查询取代。
  cancelled?: boolean;
}

// cachedSnapshotKey identifies the parsed diff snapshot currently retained by this worker.
// cachedSnapshotKey 标识当前由此 Worker 保留的已解析 diff snapshot。
let cachedSnapshotKey: string | null = null;

// cachedSnapshot stores the parsed diff snapshot so repeated query changes do not clone it again.
// cachedSnapshot 保存已解析 diff snapshot，避免连续查询变更时重复克隆。
let cachedSnapshot: DiffPanelSnapshot | null = null;

// cachedSearchIndex stores the latest search result for safe incremental query narrowing.
// cachedSearchIndex 保存最近一次搜索结果，用于安全的增量查询收窄。
let cachedSearchIndex: DiffSearchIndex | null = null;

// cachedSearchIndexesByQuery stores exact-query search indexes for the current snapshot.
// cachedSearchIndexesByQuery 保存当前 snapshot 内精确查询对应的搜索索引。
const cachedSearchIndexesByQuery = new Map<string, DiffSearchIndex>();

// latestSearchRequestSerial identifies the newest request allowed to publish results.
// latestSearchRequestSerial 标识允许发布结果的最新请求。
let latestSearchRequestSerial = 0;

self.onmessage = (event: MessageEvent<DiffSearchWorkerRequest>) => {
  const request = event.data;
  const requestSerial = ++latestSearchRequestSerial;
  if (request.cancelOnly) {
    return;
  }
  void handleDiffSearchWorkerRequest(request, requestSerial);
};

// handleDiffSearchWorkerRequest scans a query in cancellable slices.
// handleDiffSearchWorkerRequest 以可取消片段扫描单个查询。
async function handleDiffSearchWorkerRequest(request: DiffSearchWorkerRequest, requestSerial: number): Promise<void> {
  try {
    const snapshot = diffSearchWorkerSnapshotForRequest(request);
    const normalizedQuery = normalizedSearchWorkerQuery(request.query);
    const cachedIndex = normalizedQuery ? cachedSearchIndexesByQuery.get(normalizedQuery) : null;
    if (cachedIndex && requestSerial === latestSearchRequestSerial) {
      cachedSearchIndex = cachedIndex;
      self.postMessage({
        id: request.id,
        index: cachedIndex,
      } satisfies DiffSearchWorkerResponse);
      return;
    }
    const reusableIndex = diffSearchWorkerReusableIndex(normalizedQuery);
    const index = await buildDiffSearchIndexSliced(snapshot, normalizedQuery, reusableIndex, {
      shouldContinue: () => requestSerial === latestSearchRequestSerial,
      yieldToEventLoop: yieldToDiffSearchWorkerEventLoop,
    });
    if (!index || requestSerial !== latestSearchRequestSerial) {
      self.postMessage({
        id: request.id,
        cancelled: true,
      } satisfies DiffSearchWorkerResponse);
      return;
    }
    cachedSearchIndex = index;
    rememberDiffSearchWorkerIndex(index);
    const response: DiffSearchWorkerResponse = {
      id: request.id,
      index,
    };
    self.postMessage(response);
  } catch (error) {
    const response: DiffSearchWorkerResponse = {
      id: request.id,
      error: error instanceof Error ? error.message : String(error),
    };
    self.postMessage(response);
  }
}

// diffSearchWorkerSnapshotForRequest returns the cached snapshot or refreshes it from the request.
// diffSearchWorkerSnapshotForRequest 返回缓存的 snapshot，或从请求中刷新缓存。
function diffSearchWorkerSnapshotForRequest(request: DiffSearchWorkerRequest): DiffPanelSnapshot {
  if (request.snapshot) {
    cachedSnapshotKey = request.snapshotKey;
    cachedSnapshot = request.snapshot;
    cachedSearchIndex = null;
    cachedSearchIndexesByQuery.clear();
    return request.snapshot;
  }
  if (cachedSnapshot && cachedSnapshotKey === request.snapshotKey) {
    return cachedSnapshot;
  }
  throw new Error("Diff search worker snapshot cache is unavailable.");
}

// normalizedSearchWorkerQuery returns the exact query cache key used inside the worker.
// normalizedSearchWorkerQuery 返回 Worker 内部精确查询缓存使用的键。
function normalizedSearchWorkerQuery(query: string): string {
  return query.trim().toLowerCase();
}

// diffSearchWorkerReusableIndex selects the best cached prefix index for incremental narrowing.
// diffSearchWorkerReusableIndex 为增量收窄选择最合适的缓存前缀索引。
function diffSearchWorkerReusableIndex(normalizedQuery: string): DiffSearchIndex | null {
  if (!normalizedQuery) {
    return null;
  }
  if (
    cachedSearchIndex
    && !cachedSearchIndex.truncated
    && cachedSearchIndex.query
    && normalizedQuery.startsWith(cachedSearchIndex.query)
  ) {
    return cachedSearchIndex;
  }
  let bestIndex: DiffSearchIndex | null = null;
  for (const index of cachedSearchIndexesByQuery.values()) {
    if (!index.query || index.truncated || !normalizedQuery.startsWith(index.query)) {
      continue;
    }
    if (!bestIndex || index.query.length > bestIndex.query.length) {
      bestIndex = index;
    }
  }
  return bestIndex;
}

// rememberDiffSearchWorkerIndex stores one exact-query result with bounded FIFO eviction.
// rememberDiffSearchWorkerIndex 以有界 FIFO 淘汰策略保存一个精确查询结果。
function rememberDiffSearchWorkerIndex(index: DiffSearchIndex): void {
  if (!index.query) {
    return;
  }
  if (cachedSearchIndexesByQuery.has(index.query)) {
    cachedSearchIndexesByQuery.delete(index.query);
  } else if (cachedSearchIndexesByQuery.size >= DIFF_SEARCH_WORKER_INDEX_CACHE_LIMIT) {
    const oldestQuery = cachedSearchIndexesByQuery.keys().next().value;
    if (oldestQuery) {
      cachedSearchIndexesByQuery.delete(oldestQuery);
    }
  }
  cachedSearchIndexesByQuery.set(index.query, index);
}

// yieldToDiffSearchWorkerEventLoop lets queued search messages preempt stale scans.
// yieldToDiffSearchWorkerEventLoop 让排队搜索消息可以抢占过期扫描。
function yieldToDiffSearchWorkerEventLoop(): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
}
