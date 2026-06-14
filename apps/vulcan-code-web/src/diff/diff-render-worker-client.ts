import * as React from "react";

import {
  renderPlainDiffFileHtmlCore,
  renderPlainDiffVirtualRowsHtmlCore,
  type DiffHighlightedFileRenderOptions,
  type DiffHighlightedLineSpans,
  type DiffHighlightedRowsRenderOptions,
  type DiffPlainFileRenderOptions,
  type DiffPlainRowsRenderOptions,
  type DiffRenderableVirtualRow,
} from "./diff-render-html-core";
import { highlightedDiffLineSpansForDiffLine } from "./diff-highlight";
import { diffSearchLineKey } from "./diff-search";
import type { DiffFile, DiffHunkSeparatorStyle, DiffIndicatorStyle, DiffLine, DiffOverflowMode, DiffViewMode } from "./types";
import { escapeHtml, normalizeShikiLanguage, type DiffShikiTokenSpan } from "./diff-shiki-shared";
import type { DiffMountedVirtualRow } from "./diff-virtual-layout";

// DIFF_RENDER_WORKER_CACHE_LIMIT caps retained worker-rendered plain HTML blocks.
// DIFF_RENDER_WORKER_CACHE_LIMIT 限制保留的 Worker 纯 HTML 渲染块数量。
const DIFF_RENDER_WORKER_CACHE_LIMIT = 512;

// DIFF_RENDER_WORKER_HIGHLIGHTED_SPAN_CACHE_LIMIT caps mirrored spans shared with the render worker.
// DIFF_RENDER_WORKER_HIGHLIGHTED_SPAN_CACHE_LIMIT 限制与渲染 Worker 同步的高亮 span 数量。
const DIFF_RENDER_WORKER_HIGHLIGHTED_SPAN_CACHE_LIMIT = 12000;

// DISABLED_DIFF_RENDER_CACHE_KEY keeps inactive renderer hooks stable without computing real cache keys.
// DISABLED_DIFF_RENDER_CACHE_KEY 让未启用的渲染 hook 保持稳定，避免计算真实缓存键。
const DISABLED_DIFF_RENDER_CACHE_KEY = "diff-render-disabled";

// EMPTY_DIFF_RENDERABLE_ROWS reuses one immutable empty row payload for inactive row renderers.
// EMPTY_DIFF_RENDERABLE_ROWS 为未启用的行渲染器复用一个不可变空行载荷。
const EMPTY_DIFF_RENDERABLE_ROWS: readonly DiffRenderableVirtualRow[] = Object.freeze([]);

// DiffRenderWorkerRequest asks the render worker to build one plain HTML block.
// DiffRenderWorkerRequest 请求渲染 Worker 构建一个纯 HTML 块。
type DiffRenderWorkerRequest =
  | {
      // Request id used to match worker responses.
      // 用于匹配 Worker 响应的请求标识。
      id: number;
      // Render kind for one complete file.
      // 单个完整文件渲染类型。
      kind: "file";
      // Parsed file payload to render.
      // 需要渲染的已解析文件载荷。
      file: DiffFile;
      // Search line keys serialized for structured clone.
      // 为结构化克隆序列化的搜索行 key。
      searchLineKeys?: string[];
      // Stable search-line key signature for worker-side lookup caching.
      // 用于 Worker 侧查询缓存的稳定搜索行 key 指纹。
      searchLineKeySignature?: string;
      // Active search line key.
      // 当前活跃搜索行 key。
      activeSearchLineKey?: string | null;
      // Highlighted token spans serialized for structured clone.
      // 为结构化克隆序列化的增量高亮 token span。
      highlightedLineSpans?: DiffHighlightedLineSpans[];
      // Whether the render worker should read highlighted spans from its cache.
      // render worker 是否应从自身缓存读取高亮 span。
      highlighted?: boolean;
      // Whether the render worker should clear highlighted span cache before applying updates.
      // render worker 是否应在应用增量前清空高亮 span 缓存。
      resetHighlightedLineSpans?: boolean;
      // Whether hunk headers should include delegated context controls.
      // hunk 头部是否应包含委托上下文控件。
      contextControls?: boolean;
      // Whether paired changed lines should render inline change markers.
      // 配对变更行是否应渲染行内变化标记。
      inlineDiff?: boolean;
      // Visual layout mode used for rendered diff rows.
      // 渲染 diff 行使用的视觉布局模式。
      viewMode?: DiffViewMode;
      // Horizontal overflow behavior for rendered code columns.
      // 渲染代码列的横向溢出行为。
      overflowMode?: DiffOverflowMode;
      // Hunk separator style used by the target diff renderer.
      // 目标 diff 渲染器使用的 hunk 分隔样式。
      hunkSeparatorStyle?: DiffHunkSeparatorStyle;
      // Change indicator style used by the target diff renderer.
      // 目标 diff 渲染器使用的变更航标样式。
      indicatorStyle?: DiffIndicatorStyle;
    }
  | {
      // Request id used to match worker responses.
      // 用于匹配 Worker 响应的请求标识。
      id: number;
      // Render kind for one virtual row window.
      // 单个虚拟行窗口渲染类型。
      kind: "rows";
      // Stable parsed-file key that owns the rows.
      // 拥有这些行的已解析文件稳定键。
      fileKey: string;
      // Mounted virtual rows to render.
      // 需要渲染的已挂载虚拟行。
      rows: readonly DiffRenderableVirtualRow[];
      // Search line keys serialized for structured clone.
      // 为结构化克隆序列化的搜索行 key。
      searchLineKeys?: string[];
      // Stable search-line key signature for worker-side lookup caching.
      // 用于 Worker 侧查询缓存的稳定搜索行 key 指纹。
      searchLineKeySignature?: string;
      // Active search line key.
      // 当前活跃搜索行 key。
      activeSearchLineKey?: string | null;
      // Highlighted token spans serialized for structured clone.
      // 为结构化克隆序列化的增量高亮 token span。
      highlightedLineSpans?: DiffHighlightedLineSpans[];
      // Whether the render worker should read highlighted spans from its cache.
      // render worker 是否应从自身缓存读取高亮 span。
      highlighted?: boolean;
      // Whether the render worker should clear highlighted span cache before applying updates.
      // render worker 是否应在应用增量前清空高亮 span 缓存。
      resetHighlightedLineSpans?: boolean;
      // Whether hunk headers should include delegated context controls.
      // hunk 头部是否应包含委托上下文控件。
      contextControls?: boolean;
      // Whether paired changed lines should render inline change markers.
      // 配对变更行是否应渲染行内变化标记。
      inlineDiff?: boolean;
      // Visual layout mode used for rendered diff rows.
      // 渲染 diff 行使用的视觉布局模式。
      viewMode?: DiffViewMode;
      // Horizontal overflow behavior for rendered code columns.
      // 渲染代码列的横向溢出行为。
      overflowMode?: DiffOverflowMode;
      // Hunk separator style used by the target diff renderer.
      // 目标 diff 渲染器使用的 hunk 分隔样式。
      hunkSeparatorStyle?: DiffHunkSeparatorStyle;
      // Change indicator style used by the target diff renderer.
      // 目标 diff 渲染器使用的变更航标样式。
      indicatorStyle?: DiffIndicatorStyle;
    };

// DiffRenderWorkerCancelRequest asks the render worker to drop one obsolete request.
// DiffRenderWorkerCancelRequest 请求渲染 Worker 丢弃一个已过期请求。
interface DiffRenderWorkerCancelRequest {
  // Request id that should be cancelled inside the render worker queue.
  // 应在渲染 Worker 队列内取消的请求标识。
  id: number;
  // Message kind used to distinguish cancellation from render work.
  // 用于区分取消消息与渲染任务的消息类型。
  kind: "cancel";
}

// DiffHighlightedFileWorkerOptions stores inputs for highlighted complete-file worker rendering.
// DiffHighlightedFileWorkerOptions 保存完整文件高亮 Worker 渲染输入。
export interface DiffHighlightedFileWorkerOptions extends Omit<DiffHighlightedFileRenderOptions, "highlightedLineSpans"> {
  // Canonical syntax language used by the highlight cache.
  // 高亮缓存使用的规范语法语言。
  language: string;
  // Monotonic highlight revision used to rescan cached token spans before cache-key comparison.
  // 用于在 cache key 比较前重新收集缓存 token span 的高亮修订号。
  highlightRevision: number;
}

// DiffPlainRowsWorkerOptions stores mounted rows before they are compacted for the render worker.
// DiffPlainRowsWorkerOptions 保存发送给 render worker 前的完整已挂载行。
export interface DiffPlainRowsWorkerOptions extends Omit<DiffPlainRowsRenderOptions, "rows"> {
  // Mounted virtual rows with layout-only fields still available on the main thread.
  // 主线程仍可使用布局专用字段的已挂载虚拟行。
  rows: readonly DiffMountedVirtualRow[];
}

// DiffHighlightedRowsWorkerOptions stores inputs for highlighted virtual-window worker rendering.
// DiffHighlightedRowsWorkerOptions 保存虚拟窗口高亮 Worker 渲染输入。
export interface DiffHighlightedRowsWorkerOptions extends Omit<DiffHighlightedRowsRenderOptions, "highlightedLineSpans" | "rows"> {
  // Mounted virtual rows with layout-only fields still available on the main thread.
  // 主线程仍可使用布局专用字段的已挂载虚拟行。
  rows: readonly DiffMountedVirtualRow[];
  // Canonical syntax language used by the highlight cache.
  // 高亮缓存使用的规范语法语言。
  language: string;
  // Monotonic highlight revision used to rescan cached token spans before cache-key comparison.
  // 用于在 cache key 比较前重新收集缓存 token span 的高亮修订号。
  highlightRevision: number;
}

// DiffRenderWorkerResponse carries rendered HTML or an error.
// DiffRenderWorkerResponse 承载渲染后的 HTML 或错误。
interface DiffRenderWorkerResponse {
  // Request id echoed from the request.
  // 从请求中回传的请求标识。
  id: number;
  // Rendered HTML when available.
  // 可用时的渲染 HTML。
  html?: string;
  // Optional error text.
  // 可选错误文本。
  error?: string;
}

// DiffSerializedSearchPayload stores worker-ready search keys and their stable signature.
// DiffSerializedSearchPayload 保存可直接发送给 Worker 的搜索 key 与稳定签名。
interface DiffSerializedSearchPayload {
  // Ordered search line keys for structured clone.
  // 用于结构化克隆的有序搜索行 key。
  keys?: string[];
  // Stable signature used by the worker-side set cache.
  // Worker 侧集合缓存使用的稳定签名。
  signature?: string;
}

// DiffWorkerHtmlState stores the last HTML snapshot visible to one renderer hook.
// DiffWorkerHtmlState 保存单个渲染 hook 当前可见的最后一份 HTML 快照。
interface DiffWorkerHtmlState {
  // Cache key that owns this HTML snapshot.
  // 拥有这份 HTML 快照的缓存 key。
  cacheKey: string;
  // Rendered HTML for the cache key.
  // 对应缓存 key 的已渲染 HTML。
  html: string;
}

// DiffRenderFallbackHtml builds immediate HTML while the render worker catches up.
// DiffRenderFallbackHtml 在渲染 Worker 追赶期间构建即时 HTML。
type DiffRenderFallbackHtml = () => string;

// DiffRenderWorkerCallback resolves one in-flight render request.
// DiffRenderWorkerCallback 解析一个进行中的渲染请求。
type DiffRenderWorkerCallback = (response: DiffRenderWorkerResponse) => void;

// DiffRenderQueuedRequest stores one scoped worker request waiting to be posted.
// DiffRenderQueuedRequest 保存一个等待发送的分 scope Worker 请求。
interface DiffRenderQueuedRequest {
  // Request id used to match worker responses.
  // 用于匹配 Worker 响应的请求标识。
  id: number;
  // Worker instance that should receive this request.
  // 应接收该请求的 Worker 实例。
  worker: Worker;
  // Stable scope key used to collapse same-window render work.
  // 用于合并同窗口渲染任务的稳定 scope key。
  scopeKey: string;
  // Complete worker request with the assigned id.
  // 已分配请求标识的完整 Worker 请求。
  request: DiffRenderWorkerRequest;
  // Called after postMessage succeeds so client-side worker caches stay in sync.
  // 在 postMessage 成功后调用，使客户端侧 worker 缓存记录保持同步。
  onPostSuccess?: (request: DiffRenderWorkerRequest) => void;
  // Lightweight failure handler invoked when posting fails.
  // 发送失败时调用的轻量失败处理器。
  onPostFailure: () => void;
}

// diffRenderWorker stores the lazily created render worker.
// diffRenderWorker 保存懒创建的渲染 Worker。
let diffRenderWorker: Worker | null = null;

// diffRenderWorkerFailed disables worker retries after construction or runtime failure.
// diffRenderWorkerFailed 在 Worker 构建或运行失败后禁用重试。
let diffRenderWorkerFailed = false;

// nextDiffRenderWorkerRequestId increments for every render request.
// nextDiffRenderWorkerRequestId 为每个渲染请求递增。
let nextDiffRenderWorkerRequestId = 0;

// diffRenderWorkerCallbacks maps request ids to live callbacks.
// diffRenderWorkerCallbacks 将请求标识映射到存活回调。
const diffRenderWorkerCallbacks = new Map<number, DiffRenderWorkerCallback>();

// diffRenderRequestScopes maps request ids back to their coalescing scope.
// diffRenderRequestScopes 将请求标识映射回其合并 scope。
const diffRenderRequestScopes = new Map<number, string>();

// diffRenderInFlightScopes tracks the active request for each render scope.
// diffRenderInFlightScopes 跟踪每个渲染 scope 当前正在执行的请求。
const diffRenderInFlightScopes = new Map<string, number>();

// diffRenderPendingScopes keeps only the latest queued request per render scope.
// diffRenderPendingScopes 对每个渲染 scope 只保留最新排队请求。
const diffRenderPendingScopes = new Map<string, DiffRenderQueuedRequest>();

// diffRenderHtmlCache stores rendered HTML blocks shared across remounts and scroll windows.
// diffRenderHtmlCache 缓存跨重挂载和滚动窗口复用的渲染 HTML 块。
const diffRenderHtmlCache = new Map<string, string>();

// diffRenderWorkerHighlightedSpanCache mirrors spans known to be posted to the current render worker.
// diffRenderWorkerHighlightedSpanCache 记录已成功发送到当前 render worker 的高亮 span。
const diffRenderWorkerHighlightedSpanCache = new Map<string, DiffShikiTokenSpan[]>();

// diffRenderWorkerHighlightedSpanCacheRevision invalidates derived payloads when the worker span mirror changes.
// diffRenderWorkerHighlightedSpanCacheRevision 在 Worker span 镜像变化时失效派生载荷。
let diffRenderWorkerHighlightedSpanCacheRevision = 0;

// highlightedLineSpansWorkerPayloadCache stores incremental span payloads for stable collected span maps.
// highlightedLineSpansWorkerPayloadCache 为稳定收集到的 span map 缓存增量载荷。
const highlightedLineSpansWorkerPayloadCache = new WeakMap<ReadonlyMap<string, DiffShikiTokenSpan[]>, DiffHighlightedLineSpansWorkerPayloadCacheEntry>();

// DIFF_RENDER_FINGERPRINT_SEED is the FNV-1a seed used by render cache fingerprints.
// DIFF_RENDER_FINGERPRINT_SEED 是渲染缓存 fingerprint 使用的 FNV-1a 种子。
const DIFF_RENDER_FINGERPRINT_SEED = 0x811c9dc5;

// DIFF_RENDER_CACHE_KEY_SEPARATOR separates stable render cache-key fields.
// DIFF_RENDER_CACHE_KEY_SEPARATOR 分隔稳定渲染缓存键字段。
const DIFF_RENDER_CACHE_KEY_SEPARATOR = "\u0000";

// diffFileContentFingerprintCache stores content fingerprints per parsed file object.
// diffFileContentFingerprintCache 按已解析文件对象缓存内容 fingerprint。
const diffFileContentFingerprintCache = new WeakMap<DiffFile, string>();

// diffRowsContentFingerprintCache stores content fingerprints per stable virtual row array.
// diffRowsContentFingerprintCache 按稳定虚拟行数组缓存内容 fingerprint。
const diffRowsContentFingerprintCache = new WeakMap<ReadonlyArray<DiffMountedVirtualRow>, string>();

// diffRenderableRowsCache stores compact render-worker rows for stable virtual row arrays.
// diffRenderableRowsCache 按稳定虚拟行数组缓存精简后的 render worker 行。
const diffRenderableRowsCache = new WeakMap<ReadonlyArray<DiffMountedVirtualRow>, DiffRenderableVirtualRow[]>();

// serializedSearchLineKeysCache stores ordered search keys for stable search result sets.
// serializedSearchLineKeysCache 按稳定搜索结果集合缓存有序搜索键。
const serializedSearchLineKeysCache = new WeakMap<ReadonlySet<string>, string[]>();

// serializedSearchLineKeyStringCache stores joined search keys for repeated render cache keys.
// serializedSearchLineKeyStringCache 为重复渲染缓存键保存已拼接的搜索键。
const serializedSearchLineKeyStringCache = new WeakMap<ReadonlySet<string>, string>();

// serializedSearchPayloadCache stores worker payloads for stable search result sets.
// serializedSearchPayloadCache 按稳定搜索结果集合缓存 Worker 载荷。
const serializedSearchPayloadCache = new WeakMap<ReadonlySet<string>, DiffSerializedSearchPayload>();

// EMPTY_SERIALIZED_SEARCH_LINE_KEYS keeps empty search payloads allocation-free.
// EMPTY_SERIALIZED_SEARCH_LINE_KEYS 让空搜索载荷保持零分配。
const EMPTY_SERIALIZED_SEARCH_LINE_KEYS: string[] = [];

// EMPTY_SERIALIZED_SEARCH_PAYLOAD keeps disabled or empty search requests allocation-free.
// EMPTY_SERIALIZED_SEARCH_PAYLOAD 让禁用或空搜索请求保持零分配。
const EMPTY_SERIALIZED_SEARCH_PAYLOAD: DiffSerializedSearchPayload = {};

// highlightedSpansFingerprintCache stores compact signatures for stable highlighted span maps.
// highlightedSpansFingerprintCache 按稳定高亮 span map 缓存紧凑签名。
const highlightedSpansFingerprintCache = new WeakMap<ReadonlyMap<string, unknown>, string>();

// DIFF_HIGHLIGHTED_SPAN_COLLECTION_CACHE_LIMIT caps retained virtual-window span collections.
// DIFF_HIGHLIGHTED_SPAN_COLLECTION_CACHE_LIMIT 限制保留的虚拟窗口 span 收集结果数量。
const DIFF_HIGHLIGHTED_SPAN_COLLECTION_CACHE_LIMIT = 512;

// DiffHighlightedSpanCollectionCacheEntry stores the latest file-level highlighted span collection.
// DiffHighlightedSpanCollectionCacheEntry 保存最新的文件级高亮 span 收集结果。
interface DiffHighlightedSpanCollectionCacheEntry {
  // Stable cache key for the collection input and highlight revision.
  // 收集输入与高亮修订对应的稳定缓存键。
  cacheKey: string;
  // Cached line span map; undefined means this input currently has no highlighted spans.
  // 缓存的行 span map；undefined 表示该输入当前没有高亮 span。
  spans: Map<string, DiffShikiTokenSpan[]> | undefined;
}

// DiffHighlightedRowsSpanStableCacheEntry stores the latest semantically equal row-window span map.
// DiffHighlightedRowsSpanStableCacheEntry 保存最新的语义等价虚拟窗口 span map。
interface DiffHighlightedRowsSpanStableCacheEntry {
  // Canonical language used to collect token spans.
  // 收集 token span 时使用的规范语言。
  language: string;
  // Stable content fingerprint for the mounted row window.
  // 已挂载行窗口的稳定内容指纹。
  rowsFingerprint: string;
  // Stable fingerprint of currently available highlighted spans.
  // 当前可用高亮 span 的稳定指纹。
  spansFingerprint: string;
  // Reusable span map; undefined means no highlighted spans are ready.
  // 可复用的 span map；undefined 表示当前没有已就绪高亮 span。
  spans: Map<string, DiffShikiTokenSpan[]> | undefined;
}

// DiffHighlightedFileSpanStableCacheEntry stores the latest semantically equal file-level span map.
// DiffHighlightedFileSpanStableCacheEntry 保存最新的语义等价文件级 span map。
interface DiffHighlightedFileSpanStableCacheEntry {
  // Canonical language used to collect token spans.
  // 收集 token span 时使用的规范语言。
  language: string;
  // Stable content fingerprint for the parsed diff file.
  // 已解析 diff 文件的稳定内容指纹。
  fileFingerprint: string;
  // Stable fingerprint of currently available highlighted spans.
  // 当前可用高亮 span 的稳定指纹。
  spansFingerprint: string;
  // Reusable span map; undefined means no highlighted spans are ready.
  // 可复用的 span map；undefined 表示当前没有已就绪高亮 span。
  spans: Map<string, DiffShikiTokenSpan[]> | undefined;
}

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
    () => shouldCollectHighlightedSpans ? collectHighlightedFileLineSpans(options.file, options.language, options.highlightRevision) : undefined,
    [options.file, options.highlightRevision, options.language, shouldCollectHighlightedSpans],
  );
  const cacheKey = React.useMemo(() => shouldRender ? highlightedFileRenderCacheKey(options, highlightedLineSpans, searchPayload.signature) : DISABLED_DIFF_RENDER_CACHE_KEY, [
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
    () => shouldCollectHighlightedSpans ? collectHighlightedRowsLineSpans(options.fileKey, options.rows, options.language, options.highlightRevision) : undefined,
    [options.fileKey, options.highlightRevision, options.language, options.rows, shouldCollectHighlightedSpans],
  );
  // workerRows contains only the fields needed by the render worker.
  // workerRows 仅包含 render worker 所需的字段。
  const workerRows = React.useMemo(() => shouldRender ? diffRenderableRows(options.rows) : EMPTY_DIFF_RENDERABLE_ROWS, [options.rows, shouldRender]);
  const cacheKey = React.useMemo(() => shouldRender ? highlightedRowsRenderCacheKey(options, highlightedLineSpans, searchPayload.signature) : DISABLED_DIFF_RENDER_CACHE_KEY, [
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
    const requestId = ++nextDiffRenderWorkerRequestId;
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

// diffRenderWorkerFailureHtml builds a tiny new-pipeline failure block instead of rendering diff on the main thread.
// diffRenderWorkerFailureHtml 构建轻量的新链路失败块，避免在主线程渲染 diff。
function diffRenderWorkerFailureHtml(message: string): string {
  return `<div class="diff-render-worker-note"><span>Diff render worker unavailable</span><small>${escapeHtml(message)}</small></div>`;
}

// currentDiffRenderWorker returns the singleton render worker when available.
// currentDiffRenderWorker 在可用时返回单例渲染 Worker。
function currentDiffRenderWorker(): Worker | null {
  if (diffRenderWorkerFailed || typeof Worker === "undefined") {
    return null;
  }
  if (!diffRenderWorker) {
    try {
      resetDiffRenderWorkerHighlightedSpanCacheMirror();
      diffRenderWorker = new Worker(new URL("./diff-render-worker.ts", import.meta.url), {
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
function enqueueDiffRenderWorkerRequest(
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
function cancelQueuedDiffRenderWorkerRequest(requestId: number): void {
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
function rememberDiffRenderHtml(cacheKey: string, html: string): void {
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

// plainFileRenderCacheKey builds one stable cache key for complete file plain HTML.
// plainFileRenderCacheKey 为完整文件纯 HTML 构建稳定缓存 key。
function plainFileRenderCacheKey(options: DiffPlainFileRenderOptions, searchSignature?: string): string {
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
function plainRowsRenderCacheKey(options: DiffPlainRowsWorkerOptions, searchSignature?: string): string {
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
function highlightedFileRenderCacheKey(
  options: DiffHighlightedFileWorkerOptions,
  highlightedLineSpans?: ReadonlyMap<string, unknown>,
  searchSignature?: string,
): string {
  return "highlight-file"
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + options.file.key
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
function highlightedRowsRenderCacheKey(
  options: DiffHighlightedRowsWorkerOptions,
  highlightedLineSpans?: ReadonlyMap<string, unknown>,
  searchSignature?: string,
): string {
  return "highlight-rows"
    + DIFF_RENDER_CACHE_KEY_SEPARATOR + options.fileKey
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

// diffFileContentFingerprint returns a stable content signature for one parsed file object.
// diffFileContentFingerprint 返回单个已解析文件对象的稳定内容签名。
function diffFileContentFingerprint(file: DiffFile): string {
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
function diffRowsContentFingerprint(rows: ReadonlyArray<DiffMountedVirtualRow>): string {
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
function diffRenderableRows(rows: ReadonlyArray<DiffMountedVirtualRow>): DiffRenderableVirtualRow[] {
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
function highlightedSpansFingerprint(highlightedLineSpans?: ReadonlyMap<string, unknown>): string {
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

// DiffHighlightedLineSpansWorkerPayload stores line keys plus only spans missing from the worker cache.
// DiffHighlightedLineSpansWorkerPayload 保存行 key 以及 worker 缓存缺失的 span 增量。
interface DiffHighlightedLineSpansWorkerPayload {
  // Whether highlighted rendering is active for this request.
  // 当前请求是否启用高亮渲染。
  highlighted?: boolean;
  // Whether the render worker span cache should be cleared before applying updates.
  // render worker span 缓存是否应在应用增量前清空。
  reset?: boolean;
  // Incremental span updates that must be posted to the render worker.
  // 必须发送给 render worker 的 span 增量。
  updates?: DiffHighlightedLineSpans[];
}

// DiffHighlightedLineSpansWorkerPayloadCacheEntry stores one payload for a worker span-cache revision.
// DiffHighlightedLineSpansWorkerPayloadCacheEntry 保存某个 Worker span 缓存版本对应的载荷。
interface DiffHighlightedLineSpansWorkerPayloadCacheEntry {
  // Worker span-cache revision used to derive this payload.
  // 派生该载荷时使用的 Worker span 缓存版本。
  revision: number;
  // Cached incremental payload for the collected span map.
  // 当前收集 span map 对应的缓存增量载荷。
  payload: DiffHighlightedLineSpansWorkerPayload;
}

// highlightedLineSpansWorkerPayload builds an incremental payload for the render worker span cache.
// highlightedLineSpansWorkerPayload 为 render worker 的 span 缓存构建增量载荷。
function highlightedLineSpansWorkerPayload(
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
function resetDiffRenderWorkerHighlightedSpanCacheMirror(): void {
  diffRenderWorkerHighlightedSpanCache.clear();
  diffRenderWorkerHighlightedSpanCacheRevision += 1;
}

// rememberPostedHighlightedLineSpans records span updates only after postMessage succeeds.
// rememberPostedHighlightedLineSpans 仅在 postMessage 成功后记录已发送的 span 更新。
function rememberPostedHighlightedLineSpans(request: DiffRenderWorkerRequest): void {
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

// appendFingerprintText folds text into an unsigned FNV-1a hash.
// appendFingerprintText 将文本折叠进无符号 FNV-1a hash。
function appendFingerprintText(hash: number, value: string): number {
  let nextHash = hash;
  for (let index = 0; index < value.length; index += 1) {
    nextHash = Math.imul(nextHash ^ value.charCodeAt(index), 0x01000193) >>> 0;
  }
  return nextHash;
}

// appendFingerprintNumber folds a number into an unsigned FNV-1a hash.
// appendFingerprintNumber 将数字折叠进无符号 FNV-1a hash。
function appendFingerprintNumber(hash: number, value: number): number {
  return appendFingerprintText(hash, String(value));
}

// collectHighlightedFileLineSpans gathers cached token spans for one full file.
// collectHighlightedFileLineSpans 收集单个完整文件中已经缓存的 token span。
function collectHighlightedFileLineSpans(file: DiffFile, language: string, highlightRevision: number): Map<string, DiffShikiTokenSpan[]> | undefined {
  // normalizedLanguage keeps the file-level highlight lookup on the canonical Shiki id.
  // normalizedLanguage 让文件级高亮查询复用规范化后的 Shiki 标识。
  const normalizedLanguage = normalizeShikiLanguage(language);
  if (!normalizedLanguage) {
    return undefined;
  }
  const fileFingerprint = diffFileContentFingerprint(file);
  // cacheKey identifies this file span collection for one language and highlight revision.
  // cacheKey 标识当前语言与高亮修订下的文件 span 收集结果。
  const cacheKey = highlightedFileLineSpansCacheKey(file, fileFingerprint, normalizedLanguage, highlightRevision);
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
      const spans = highlightedDiffLineSpansForDiffLine(line, normalizedLanguage);
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
    && stableEntry.language === normalizedLanguage
    && stableEntry.fileFingerprint === fileFingerprint
    && stableEntry.spansFingerprint === spansFingerprint
  ) {
    highlightedFileLineSpansCache.set(file, { cacheKey, spans: stableEntry.spans });
    return stableEntry.spans;
  }
  highlightedFileLineSpansStableCache.set(file, {
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
function collectHighlightedRowsLineSpans(
  fileKey: string,
  rows: readonly DiffMountedVirtualRow[],
  language: string,
  highlightRevision: number,
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
  const cacheKey = highlightedRowsLineSpansCacheKey(fileKey, rowsFingerprint, normalizedLanguage, highlightRevision);
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
    const spans = highlightedDiffLineSpansForDiffLine(row.line, normalizedLanguage);
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
    && stableEntry.language === normalizedLanguage
    && stableEntry.rowsFingerprint === rowsFingerprint
    && stableEntry.spansFingerprint === spansFingerprint
  ) {
    rememberHighlightedRowsLineSpans(cacheKey, stableEntry.spans);
    return stableEntry.spans;
  }
  highlightedRowsLineSpansStableCache.set(rows, {
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

// rememberHighlightedSpansFingerprint reuses the collection-time signature for render cache keys.
// rememberHighlightedSpansFingerprint 复用收集阶段签名，供渲染缓存键直接读取。
function rememberHighlightedSpansFingerprint(
  spansByLineKey: Map<string, DiffShikiTokenSpan[]> | undefined,
  spansFingerprint: string,
): void {
  if (spansByLineKey) {
    highlightedSpansFingerprintCache.set(spansByLineKey, spansFingerprint);
  }
}

// highlightedFileLineSpansCacheKey builds a stable key for one file span collection.
// highlightedFileLineSpansCacheKey 为单个文件 span 收集结果构建稳定键。
function highlightedFileLineSpansCacheKey(file: DiffFile, fileFingerprint: string, language: string, highlightRevision: number): string {
  return language
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
): string {
  return fileKey
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

// searchStateCacheKey builds a stable cache key fragment from the serialized search signature.
// searchStateCacheKey 基于已序列化搜索签名构建稳定缓存 key 片段。
function searchStateCacheKey(searchSignature?: string, activeSearchLineKey?: string | null): string {
  return `${activeSearchLineKey ?? ""}\u0001${searchSignature ?? ""}`;
}

// serializedSearchPayloadForRequest returns worker-ready search keys and signature together.
// serializedSearchPayloadForRequest 同时返回可发送给 Worker 的搜索 key 与签名。
function serializedSearchPayloadForRequest(searchLineKeys?: ReadonlySet<string>): DiffSerializedSearchPayload {
  if (!searchLineKeys) {
    return EMPTY_SERIALIZED_SEARCH_PAYLOAD;
  }
  const cached = serializedSearchPayloadCache.get(searchLineKeys);
  if (cached) {
    return cached;
  }
  const { keys, signature } = serializedSearchPayloadParts(searchLineKeys);
  if (!keys.length) {
    serializedSearchPayloadCache.set(searchLineKeys, EMPTY_SERIALIZED_SEARCH_PAYLOAD);
    return EMPTY_SERIALIZED_SEARCH_PAYLOAD;
  }
  const payload = {
    keys,
    signature: signature || undefined,
  };
  serializedSearchPayloadCache.set(searchLineKeys, payload);
  return payload;
}

// serializedSearchPayloadParts builds ordered keys and a cache signature in one pass.
// serializedSearchPayloadParts 单次遍历生成有序 key 与缓存签名。
function serializedSearchPayloadParts(searchLineKeys: ReadonlySet<string>): Required<DiffSerializedSearchPayload> {
  if (!searchLineKeys.size) {
    serializedSearchLineKeysCache.set(searchLineKeys, EMPTY_SERIALIZED_SEARCH_LINE_KEYS);
    serializedSearchLineKeyStringCache.set(searchLineKeys, "");
    return { keys: EMPTY_SERIALIZED_SEARCH_LINE_KEYS, signature: "" };
  }
  const cachedKeys = serializedSearchLineKeysCache.get(searchLineKeys);
  const cachedSignature = serializedSearchLineKeyStringCache.get(searchLineKeys);
  if (cachedKeys && cachedSignature != null) {
    return { keys: cachedKeys, signature: cachedSignature };
  }
  if (cachedKeys) {
    const signature = serializedSearchLineKeyStringFromKeys(cachedKeys);
    serializedSearchLineKeyStringCache.set(searchLineKeys, signature);
    return { keys: cachedKeys, signature };
  }
  const keys: string[] = new Array(searchLineKeys.size);
  let index = 0;
  for (const key of searchLineKeys) {
    keys[index] = key;
    index += 1;
  }
  // signature joins once after the key array is complete to avoid per-key intermediate strings.
  // signature 在 key 数组完成后一次性拼接，避免每个 key 都产生中间字符串。
  const signature = keys.join("|");
  serializedSearchLineKeysCache.set(searchLineKeys, keys);
  serializedSearchLineKeyStringCache.set(searchLineKeys, signature);
  return { keys, signature };
}

// serializedSearchLineKeyString returns a cached joined search-key string.
// serializedSearchLineKeyString 返回缓存后的搜索键拼接字符串。
function serializedSearchLineKeyString(searchLineKeys?: ReadonlySet<string>): string {
  if (!searchLineKeys) {
    return "";
  }
  const cached = serializedSearchLineKeyStringCache.get(searchLineKeys);
  if (cached != null) {
    return cached;
  }
  const serializedKeyString = serializedSearchPayloadParts(searchLineKeys).signature;
  serializedSearchLineKeyStringCache.set(searchLineKeys, serializedKeyString);
  return serializedKeyString;
}

// serializedSearchLineKeys converts a readonly set into a stable string array.
// serializedSearchLineKeys 将只读集合转换为稳定字符串数组。
function serializedSearchLineKeys(searchLineKeys?: ReadonlySet<string>): string[] {
  if (!searchLineKeys) {
    return EMPTY_SERIALIZED_SEARCH_LINE_KEYS;
  }
  const cached = serializedSearchLineKeysCache.get(searchLineKeys);
  if (cached) {
    return cached;
  }
  if (!searchLineKeys.size) {
    serializedSearchLineKeysCache.set(searchLineKeys, EMPTY_SERIALIZED_SEARCH_LINE_KEYS);
    return EMPTY_SERIALIZED_SEARCH_LINE_KEYS;
  }
  const serializedKeys: string[] = new Array(searchLineKeys.size);
  let index = 0;
  for (const key of searchLineKeys) {
    serializedKeys[index] = key;
    index += 1;
  }
  serializedSearchLineKeysCache.set(searchLineKeys, serializedKeys);
  return serializedKeys;
}

// serializedSearchLineKeyStringFromKeys joins cached keys without rebuilding the key array.
// serializedSearchLineKeyStringFromKeys 复用已缓存 key 数组生成签名。
function serializedSearchLineKeyStringFromKeys(keys: readonly string[]): string {
  return keys.join("|");
}
