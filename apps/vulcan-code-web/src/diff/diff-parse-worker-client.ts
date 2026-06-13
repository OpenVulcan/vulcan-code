import type { DiffPanelSnapshot, DiffPanelSource } from "./types";
import { DIFF_PARSE_WORKER_PROMISE_CACHE_LIMIT } from "./limits";

// DiffParseWorkerRequest asks the parse worker to process one raw diff payload.
// DiffParseWorkerRequest 请求解析 Worker 处理一个原始 diff 载荷。
interface DiffParseWorkerRequest {
  // Request id used to match worker responses.
  // 用于匹配 Worker 响应的请求标识。
  id: number;
  // Raw unified diff text.
  // 原始 unified diff 文本。
  rawDiff: string;
  // Diff source label used by the renderer.
  // 渲染器使用的 diff 来源标签。
  source: DiffPanelSource;
  // Repository root associated with the diff.
  // diff 关联的仓库根路径。
  repositoryRoot: string;
}

// DiffParseWorkerResponse carries a parsed snapshot or an error.
// DiffParseWorkerResponse 承载解析后的快照或错误。
interface DiffParseWorkerResponse {
  // Request id echoed from the worker request.
  // 从 Worker 请求中回传的请求标识。
  id: number;
  // Parsed snapshot when the worker succeeds.
  // Worker 成功时解析出的快照。
  snapshot?: DiffPanelSnapshot;
  // Optional error text surfaced to the async caller.
  // 暴露给异步调用方的可选错误文本。
  error?: string;
}

// DiffParseWorkerCallback resolves one in-flight parse request.
// DiffParseWorkerCallback 解析一个进行中的解析请求。
type DiffParseWorkerCallback = (response: DiffParseWorkerResponse) => void;

// DiffParsePromiseCacheEntry stores one reusable parse promise for an exact raw diff payload.
// DiffParsePromiseCacheEntry 保存一个精确原始 diff 载荷对应的可复用解析 Promise。
interface DiffParsePromiseCacheEntry {
  // Raw unified diff text used to validate the compact cache key.
  // 用于校验紧凑缓存键的原始 unified diff 文本。
  rawDiff: string;
  // Diff source label used by the renderer.
  // 渲染器使用的 diff 来源标签。
  source: DiffPanelSource;
  // Repository root associated with the diff.
  // diff 关联的仓库根路径。
  repositoryRoot: string;
  // In-flight or fulfilled worker parse promise.
  // 正在执行或已完成的 Worker 解析 Promise。
  promise: Promise<DiffPanelSnapshot>;
}

// diffParseWorker stores the lazily created parse worker.
// diffParseWorker 保存懒创建的解析 Worker。
let diffParseWorker: Worker | null = null;

// diffParseWorkerFailed disables worker retries after construction or runtime failure.
// diffParseWorkerFailed 在 Worker 构建或运行失败后禁用重试。
let diffParseWorkerFailed = false;

// nextDiffParseWorkerRequestId increments for every parse request.
// nextDiffParseWorkerRequestId 为每个解析请求递增。
let nextDiffParseWorkerRequestId = 0;

// diffParseWorkerCallbacks maps request ids to live callbacks.
// diffParseWorkerCallbacks 将请求标识映射到存活回调。
const diffParseWorkerCallbacks = new Map<number, DiffParseWorkerCallback>();

// diffParsePromiseCache stores bounded exact raw-diff parse promises for repeated detail hydration.
// diffParsePromiseCache 为重复详情水合保存有界的精确原始 diff 解析 Promise。
const diffParsePromiseCache = new Map<string, DiffParsePromiseCacheEntry>();

// DIFF_PARSE_CACHE_HASH_SAMPLE_CHARS limits main-thread work spent deriving compact raw-diff keys.
// DIFF_PARSE_CACHE_HASH_SAMPLE_CHARS 限制主线程为原始 diff 紧凑键采样的字符数量。
const DIFF_PARSE_CACHE_HASH_SAMPLE_CHARS = 4096;

// parseUnifiedDiffAsync parses workspace-sized unified diff payloads only through the worker pipeline.
// parseUnifiedDiffAsync 仅通过 Worker 流水线解析工作区规模的 unified diff 载荷。
export function parseUnifiedDiffAsync(
  rawDiff: string,
  source: DiffPanelSource = "message",
  repositoryRoot = "",
): Promise<DiffPanelSnapshot> {
  const worker = currentDiffParseWorker();
  if (!worker) {
    return Promise.reject(new Error("Diff parse worker is unavailable."));
  }
  const cacheKey = diffParsePromiseCacheKey(rawDiff, source, repositoryRoot);
  const cached = diffParsePromiseCache.get(cacheKey);
  if (cached && cached.rawDiff === rawDiff && cached.source === source && cached.repositoryRoot === repositoryRoot) {
    return cached.promise;
  }
  const requestId = ++nextDiffParseWorkerRequestId;
  const promise = new Promise<DiffPanelSnapshot>((resolve, reject) => {
    diffParseWorkerCallbacks.set(requestId, (response) => {
      if (response.snapshot && !response.error) {
        resolve(response.snapshot);
      } else {
        reject(new Error(response.error || "Diff parse worker failed."));
      }
    });
    try {
      const request: DiffParseWorkerRequest = {
        id: requestId,
        rawDiff,
        source,
        repositoryRoot,
      };
      worker.postMessage(request);
    } catch {
      diffParseWorkerCallbacks.delete(requestId);
      reject(new Error("Diff parse worker request could not be posted."));
    }
  });
  rememberDiffParsePromise(cacheKey, {
    rawDiff,
    source,
    repositoryRoot,
    promise,
  });
  promise.catch(() => {
    const cachedEntry = diffParsePromiseCache.get(cacheKey);
    if (cachedEntry?.promise === promise) {
      diffParsePromiseCache.delete(cacheKey);
    }
  });
  return promise;
}

// currentDiffParseWorker returns the singleton parse worker when available.
// currentDiffParseWorker 在可用时返回单例解析 Worker。
function currentDiffParseWorker(): Worker | null {
  if (diffParseWorkerFailed || typeof Worker === "undefined") {
    return null;
  }
  if (!diffParseWorker) {
    try {
      diffParseWorker = new Worker(new URL("./diff-parse-worker.ts", import.meta.url), {
        type: "module",
      });
      diffParseWorker.onmessage = handleDiffParseWorkerMessage;
      diffParseWorker.onerror = handleDiffParseWorkerFailure;
      diffParseWorker.onmessageerror = handleDiffParseWorkerFailure;
    } catch {
      diffParseWorkerFailed = true;
      return null;
    }
  }
  return diffParseWorker;
}

// handleDiffParseWorkerMessage dispatches one worker response to its callback.
// handleDiffParseWorkerMessage 将单个 Worker 响应分发给对应回调。
function handleDiffParseWorkerMessage(event: MessageEvent<DiffParseWorkerResponse>): void {
  const response = event.data;
  const callback = diffParseWorkerCallbacks.get(response.id);
  if (!callback) {
    return;
  }
  diffParseWorkerCallbacks.delete(response.id);
  callback(response);
}

// handleDiffParseWorkerFailure disables the worker and releases pending callbacks.
// handleDiffParseWorkerFailure 禁用 Worker 并释放等待中的回调。
function handleDiffParseWorkerFailure(): void {
  diffParseWorkerFailed = true;
  diffParseWorker?.terminate();
  diffParseWorker = null;
  const callbacks = [...diffParseWorkerCallbacks.values()];
  diffParseWorkerCallbacks.clear();
  diffParsePromiseCache.clear();
  for (const callback of callbacks) {
    callback({ id: 0, error: "Diff parse worker failed" });
  }
}

// diffParsePromiseCacheKey builds a compact key without concatenating full raw diff text.
// diffParsePromiseCacheKey 构建紧凑键，避免拼接完整原始 diff 文本。
function diffParsePromiseCacheKey(rawDiff: string, source: DiffPanelSource, repositoryRoot: string): string {
  let hash = 0x811c9dc5;
  hash = appendDiffParseSampleHash(hash, rawDiff);
  hash = appendDiffParseHashText(hash, source);
  hash = appendDiffParseHashText(hash, repositoryRoot);
  return `${source}\u0000${repositoryRoot.length}\u0000${rawDiff.length}\u0000${hash.toString(36)}`;
}

// appendDiffParseSampleHash folds bounded raw-diff samples into the compact cache key.
// appendDiffParseSampleHash 将有界原始 diff 采样折叠进紧凑缓存键。
function appendDiffParseSampleHash(hash: number, value: string): number {
  if (value.length <= DIFF_PARSE_CACHE_HASH_SAMPLE_CHARS * 3) {
    return appendDiffParseHashText(hash, value);
  }
  let nextHash = appendDiffParseHashRange(hash, value, 0, DIFF_PARSE_CACHE_HASH_SAMPLE_CHARS);
  const middleStart = Math.max(0, Math.floor((value.length - DIFF_PARSE_CACHE_HASH_SAMPLE_CHARS) / 2));
  nextHash = appendDiffParseHashRange(nextHash, value, middleStart, middleStart + DIFF_PARSE_CACHE_HASH_SAMPLE_CHARS);
  return appendDiffParseHashRange(nextHash, value, value.length - DIFF_PARSE_CACHE_HASH_SAMPLE_CHARS, value.length);
}

// appendDiffParseHashText folds text into an unsigned FNV-1a hash for compact cache keys.
// appendDiffParseHashText 将文本折叠进无符号 FNV-1a hash，用于紧凑缓存键。
function appendDiffParseHashText(hash: number, value: string): number {
  return appendDiffParseHashRange(hash, value, 0, value.length);
}

// appendDiffParseHashRange folds a bounded text range into an unsigned FNV-1a hash.
// appendDiffParseHashRange 将有界文本范围折叠进无符号 FNV-1a hash。
function appendDiffParseHashRange(hash: number, value: string, start: number, end: number): number {
  let nextHash = hash;
  for (let index = start; index < end; index += 1) {
    nextHash = Math.imul(nextHash ^ value.charCodeAt(index), 0x01000193) >>> 0;
  }
  return nextHash;
}

// rememberDiffParsePromise stores one parse promise with bounded FIFO eviction.
// rememberDiffParsePromise 以有界 FIFO 淘汰策略保存一个解析 Promise。
function rememberDiffParsePromise(cacheKey: string, entry: DiffParsePromiseCacheEntry): void {
  if (diffParsePromiseCache.has(cacheKey)) {
    diffParsePromiseCache.delete(cacheKey);
  } else if (diffParsePromiseCache.size >= DIFF_PARSE_WORKER_PROMISE_CACHE_LIMIT) {
    const oldestKey = diffParsePromiseCache.keys().next().value;
    if (oldestKey) {
      diffParsePromiseCache.delete(oldestKey);
    }
  }
  diffParsePromiseCache.set(cacheKey, entry);
}
