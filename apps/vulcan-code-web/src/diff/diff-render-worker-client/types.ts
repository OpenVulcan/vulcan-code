import type {
  DiffHighlightedFileRenderOptions,
  DiffHighlightedLineSpans,
  DiffHighlightedRowsRenderOptions,
  DiffPlainRowsRenderOptions,
  DiffRenderableVirtualRow,
} from "../diff-render-html-core";
import type { DiffFile, DiffHunkSeparatorStyle, DiffIndicatorStyle, DiffOverflowMode, DiffViewMode } from "../types";
import type { DiffShikiTheme, DiffShikiTokenSpan } from "../diff-shiki-shared";
import type { DiffMountedVirtualRow } from "../diff-virtual-layout";
// DiffRenderWorkerRequest asks the render worker to build one plain HTML block.
// DiffRenderWorkerRequest 请求渲染 Worker 构建一个纯 HTML 块。
export type DiffRenderWorkerRequest =
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
export interface DiffRenderWorkerCancelRequest {
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
export interface DiffRenderWorkerResponse {
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
export interface DiffSerializedSearchPayload {
  // Ordered search line keys for structured clone.
  // 用于结构化克隆的有序搜索行 key。
  keys?: string[];
  // Stable signature used by the worker-side set cache.
  // Worker 侧集合缓存使用的稳定签名。
  signature?: string;
}

// DiffWorkerHtmlState stores the last HTML snapshot visible to one renderer hook.
// DiffWorkerHtmlState 保存单个渲染 hook 当前可见的最后一份 HTML 快照。
export interface DiffWorkerHtmlState {
  // Cache key that owns this HTML snapshot.
  // 拥有这份 HTML 快照的缓存 key。
  cacheKey: string;
  // Rendered HTML for the cache key.
  // 对应缓存 key 的已渲染 HTML。
  html: string;
}

// DiffRenderFallbackHtml builds immediate HTML while the render worker catches up.
// DiffRenderFallbackHtml 在渲染 Worker 追赶期间构建即时 HTML。
export type DiffRenderFallbackHtml = () => string;

// DiffRenderWorkerCallback resolves one in-flight render request.
// DiffRenderWorkerCallback 解析一个进行中的渲染请求。
export type DiffRenderWorkerCallback = (response: DiffRenderWorkerResponse) => void;

// DiffRenderQueuedRequest stores one scoped worker request waiting to be posted.
// DiffRenderQueuedRequest 保存一个等待发送的分 scope Worker 请求。
export interface DiffRenderQueuedRequest {
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
// DiffHighlightedSpanCollectionCacheEntry stores the latest file-level highlighted span collection.
// DiffHighlightedSpanCollectionCacheEntry 保存最新的文件级高亮 span 收集结果。
export interface DiffHighlightedSpanCollectionCacheEntry {
  // Stable cache key for the collection input and highlight revision.
  // 收集输入与高亮修订对应的稳定缓存键。
  cacheKey: string;
  // Cached line span map; undefined means this input currently has no highlighted spans.
  // 缓存的行 span map；undefined 表示该输入当前没有高亮 span。
  spans: Map<string, DiffShikiTokenSpan[]> | undefined;
}

// DiffHighlightedRowsSpanStableCacheEntry stores the latest semantically equal row-window span map.
// DiffHighlightedRowsSpanStableCacheEntry 保存最新的语义等价虚拟窗口 span map。
export interface DiffHighlightedRowsSpanStableCacheEntry {
  // Shiki theme used to collect token spans.
  // 收集 token span 时使用的 Shiki 主题。
  theme: DiffShikiTheme;
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
export interface DiffHighlightedFileSpanStableCacheEntry {
  // Shiki theme used to collect token spans.
  // 收集 token span 时使用的 Shiki 主题。
  theme: DiffShikiTheme;
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
// DiffHighlightedLineSpansWorkerPayload stores line keys plus only spans missing from the worker cache.
// DiffHighlightedLineSpansWorkerPayload 保存行 key 以及 worker 缓存缺失的 span 增量。
export interface DiffHighlightedLineSpansWorkerPayload {
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
export interface DiffHighlightedLineSpansWorkerPayloadCacheEntry {
  // Worker span-cache revision used to derive this payload.
  // 派生该载荷时使用的 Worker span 缓存版本。
  revision: number;
  // Cached incremental payload for the collected span map.
  // 当前收集 span map 对应的缓存增量载荷。
  payload: DiffHighlightedLineSpansWorkerPayload;
}

