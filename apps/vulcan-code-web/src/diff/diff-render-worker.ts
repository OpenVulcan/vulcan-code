import {
  renderHighlightedDiffFileHtmlCore,
  renderHighlightedDiffVirtualRowsHtmlRangeCore,
  renderPlainDiffFileHtmlCore,
  renderPlainDiffVirtualRowsHtmlRangeCore,
  type DiffHighlightedLineSpans,
  type DiffRenderableVirtualRow,
} from "./diff-render-html-core";
import type { DiffFile, DiffViewMode } from "./types";
import type { DiffShikiTokenSpan } from "./diff-shiki-shared";

// DiffRenderWorkerRequest asks the worker to render one plain diff HTML block.
// DiffRenderWorkerRequest 请求 Worker 渲染一个纯文本 diff HTML 块。
type DiffRenderWorkerRequest =
  | {
      // Request id echoed back to the main thread.
      // 回传给主线程的请求标识。
      id: number;
      // Render kind for a complete file.
      // 完整文件渲染类型。
      kind: "file";
      // Parsed file payload to render.
      // 需要渲染的已解析文件载荷。
      file: DiffFile;
      // Search line keys serialized from the main thread.
      // 从主线程序列化得到的搜索行 key。
      searchLineKeys?: string[];
      // Stable search-line key signature used for worker-side lookup caching.
      // 用于 Worker 侧查询缓存的稳定搜索行 key 指纹。
      searchLineKeySignature?: string;
      // Active search line key.
      // 当前活跃搜索行 key。
      activeSearchLineKey?: string | null;
      // Highlighted token spans serialized from the main thread.
      // 从主线程序列化得到的高亮 token span 增量。
      highlightedLineSpans?: DiffHighlightedLineSpans[];
      // Whether highlighted rendering should read spans from the worker cache.
      // 高亮渲染是否应从 worker 缓存读取 span。
      highlighted?: boolean;
      // Whether highlighted span cache should be cleared before applying updates.
      // 是否应在应用增量前清空高亮 span 缓存。
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
    }
  | {
      // Request id echoed back to the main thread.
      // 回传给主线程的请求标识。
      id: number;
      // Render kind for a mounted virtual window.
      // 已挂载虚拟窗口渲染类型。
      kind: "rows";
      // Stable parsed-file key that owns the rows.
      // 拥有这些行的已解析文件稳定键。
      fileKey: string;
      // Mounted virtual rows to render.
      // 需要渲染的已挂载虚拟行。
      rows: readonly DiffRenderableVirtualRow[];
      // Search line keys serialized from the main thread.
      // 从主线程序列化得到的搜索行 key。
      searchLineKeys?: string[];
      // Stable search-line key signature used for worker-side lookup caching.
      // 用于 Worker 侧查询缓存的稳定搜索行 key 指纹。
      searchLineKeySignature?: string;
      // Active search line key.
      // 当前活跃搜索行 key。
      activeSearchLineKey?: string | null;
      // Highlighted token spans serialized from the main thread.
      // 从主线程序列化得到的高亮 token span 增量。
      highlightedLineSpans?: DiffHighlightedLineSpans[];
      // Whether highlighted rendering should read spans from the worker cache.
      // 高亮渲染是否应从 worker 缓存读取 span。
      highlighted?: boolean;
      // Whether highlighted span cache should be cleared before applying updates.
      // 是否应在应用增量前清空高亮 span 缓存。
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
    };

// DiffRenderWorkerCancelRequest asks the worker to cancel one queued or running request.
// DiffRenderWorkerCancelRequest 请求 Worker 取消一个排队中或执行中的请求。
interface DiffRenderWorkerCancelRequest {
  // Request id that should be cancelled.
  // 应被取消的请求标识。
  id: number;
  // Message kind used to distinguish cancellation from render work.
  // 用于区分取消消息与渲染任务的消息类型。
  kind: "cancel";
}

// DiffRenderWorkerMessage stores every message accepted by the render worker.
// DiffRenderWorkerMessage 保存渲染 Worker 接收的所有消息。
type DiffRenderWorkerMessage = DiffRenderWorkerRequest | DiffRenderWorkerCancelRequest;

// DiffRenderWorkerResponse returns rendered HTML or an error.
// DiffRenderWorkerResponse 返回渲染后的 HTML 或错误。
interface DiffRenderWorkerResponse {
  // Request id echoed from the request.
  // 从请求中回传的请求标识。
  id: number;
  // Rendered HTML when successful.
  // 成功时渲染出的 HTML。
  html?: string;
  // Optional error message returned to the worker client.
  // 返回给 Worker 客户端的可选错误信息。
  error?: string;
  // Whether the worker stopped because the main thread cancelled the request.
  // Worker 是否因主线程取消而停止。
  cancelled?: boolean;
}

// DIFF_RENDER_WORKER_ROW_SLICE_LIMIT yields after rendering this many virtual rows.
// DIFF_RENDER_WORKER_ROW_SLICE_LIMIT 每渲染这么多虚拟行后让出一次事件循环。
const DIFF_RENDER_WORKER_ROW_SLICE_LIMIT = 96;

// renderWorkerHighlightedLineSpansCache stores highlighted spans already received by this worker.
// renderWorkerHighlightedLineSpansCache 保存当前 worker 已收到的高亮 span。
const renderWorkerHighlightedLineSpansCache = new Map<string, DiffShikiTokenSpan[]>();

// DIFF_RENDER_WORKER_SEARCH_SET_CACHE_LIMIT caps retained search-key lookup sets.
// DIFF_RENDER_WORKER_SEARCH_SET_CACHE_LIMIT 限制保留的搜索 key 查询集合数量。
const DIFF_RENDER_WORKER_SEARCH_SET_CACHE_LIMIT = 64;

// renderWorkerSearchLineKeysCache stores lookup sets for repeated search pages.
// renderWorkerSearchLineKeysCache 缓存重复搜索分页对应的查询集合。
const renderWorkerSearchLineKeysCache = new Map<string, ReadonlySet<string>>();

// cancelledRenderRequestIds stores request ids that should be skipped or stopped at the next slice boundary.
// cancelledRenderRequestIds 保存应跳过或在下个分片边界停止的请求标识。
const cancelledRenderRequestIds = new Set<number>();

// queuedRenderWorkerRequests stores render requests accepted by this worker before processing.
// queuedRenderWorkerRequests 保存 Worker 接收后等待处理的渲染请求。
const queuedRenderWorkerRequests: DiffRenderWorkerRequest[] = [];

// queuedRenderWorkerRequestIds indexes requests that are still waiting in the worker queue.
// queuedRenderWorkerRequestIds 索引仍在 Worker 队列中等待的请求。
const queuedRenderWorkerRequestIds = new Set<number>();

// queuedRenderWorkerRequestCursor points at the next queued render request without Array.shift churn.
// queuedRenderWorkerRequestCursor 指向下一个排队渲染请求，避免 Array.shift 搬移成本。
let queuedRenderWorkerRequestCursor = 0;

// currentRenderWorkerRequestId tracks the request that can still observe cancellation between slices.
// currentRenderWorkerRequestId 跟踪仍可在分片边界响应取消的当前请求。
let currentRenderWorkerRequestId: number | null = null;

// renderWorkerProcessing tracks the internal queue consumer.
// renderWorkerProcessing 标记内部队列消费者是否正在运行。
let renderWorkerProcessing = false;

self.onmessage = (event: MessageEvent<DiffRenderWorkerMessage>) => {
  if (event.data.kind === "cancel") {
    rememberCancelledRenderRequest(event.data.id);
    return;
  }
  queuedRenderWorkerRequests.push(event.data);
  queuedRenderWorkerRequestIds.add(event.data.id);
  void processQueuedRenderWorkerRequests();
};

// processQueuedRenderWorkerRequests consumes accepted render work in order.
// processQueuedRenderWorkerRequests 按顺序消费已接收的渲染任务。
async function processQueuedRenderWorkerRequests(): Promise<void> {
  if (renderWorkerProcessing) {
    return;
  }
  renderWorkerProcessing = true;
  try {
    let request = dequeueRenderWorkerRequest();
    while (request) {
      if (isRenderRequestCancelled(request.id)) {
        syncHighlightedLineSpansForCancelledRequest(request);
        cancelledRenderRequestIds.delete(request.id);
        request = dequeueRenderWorkerRequest();
        continue;
      }
      currentRenderWorkerRequestId = request.id;
      let response: DiffRenderWorkerResponse;
      try {
        response = await renderWorkerResponseForRequest(request);
      } finally {
        currentRenderWorkerRequestId = null;
      }
      cancelledRenderRequestIds.delete(request.id);
      if (!response.cancelled) {
        self.postMessage(response);
      }
      request = dequeueRenderWorkerRequest();
    }
  } finally {
    renderWorkerProcessing = false;
  }
}

// dequeueRenderWorkerRequest returns the next queued request without shifting the array.
// dequeueRenderWorkerRequest 返回下一个排队请求，避免移动数组。
function dequeueRenderWorkerRequest(): DiffRenderWorkerRequest | null {
  if (queuedRenderWorkerRequestCursor >= queuedRenderWorkerRequests.length) {
    queuedRenderWorkerRequests.length = 0;
    queuedRenderWorkerRequestIds.clear();
    queuedRenderWorkerRequestCursor = 0;
    return null;
  }
  const request = queuedRenderWorkerRequests[queuedRenderWorkerRequestCursor];
  queuedRenderWorkerRequestIds.delete(request.id);
  queuedRenderWorkerRequestCursor += 1;
  if (queuedRenderWorkerRequestCursor > 64 && queuedRenderWorkerRequestCursor * 2 >= queuedRenderWorkerRequests.length) {
    queuedRenderWorkerRequests.splice(0, queuedRenderWorkerRequestCursor);
    queuedRenderWorkerRequestCursor = 0;
  }
  return request;
}

// renderWorkerResponseForRequest renders one request and converts failures into worker responses.
// renderWorkerResponseForRequest 渲染单个请求，并将失败转换为 Worker 响应。
async function renderWorkerResponseForRequest(request: DiffRenderWorkerRequest): Promise<DiffRenderWorkerResponse> {
  try {
    const searchLineKeys = searchLineKeysForRequest(request.searchLineKeys, request.searchLineKeySignature);
    const highlightedLineSpans = highlightedLineSpansForRequest(request);
    let html: string;
    if (request.kind === "file") {
      html = highlightedLineSpans
        ? renderHighlightedDiffFileHtmlCore({
          file: request.file,
          searchLineKeys,
          activeSearchLineKey: request.activeSearchLineKey,
          contextControls: request.contextControls,
          inlineDiff: request.inlineDiff,
          viewMode: request.viewMode,
          highlightedLineSpans,
        })
        : renderPlainDiffFileHtmlCore({
          file: request.file,
          searchLineKeys,
          activeSearchLineKey: request.activeSearchLineKey,
          contextControls: request.contextControls,
          inlineDiff: request.inlineDiff,
          viewMode: request.viewMode,
        });
    } else if (highlightedLineSpans) {
      const renderedHtml = await renderHighlightedDiffVirtualRowsHtmlInSlices(request, searchLineKeys, highlightedLineSpans);
      if (renderedHtml == null) {
        return cancelledRenderResponse(request.id);
      }
      html = renderedHtml;
    } else {
      const renderedHtml = await renderPlainDiffVirtualRowsHtmlInSlices(request, searchLineKeys);
      if (renderedHtml == null) {
        return cancelledRenderResponse(request.id);
      }
      html = renderedHtml;
    }
    return { id: request.id, html };
  } catch (error) {
    return {
      id: request.id,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

// renderPlainDiffVirtualRowsHtmlInSlices renders one virtual window cooperatively.
// renderPlainDiffVirtualRowsHtmlInSlices 以协作式分片渲染一个虚拟窗口。
async function renderPlainDiffVirtualRowsHtmlInSlices(
  request: Extract<DiffRenderWorkerRequest, { kind: "rows" }>,
  searchLineKeys: ReadonlySet<string> | undefined,
): Promise<string | null> {
  let html = "";
  for (let startIndex = 0; startIndex < request.rows.length; startIndex += DIFF_RENDER_WORKER_ROW_SLICE_LIMIT) {
    if (isRenderRequestCancelled(request.id)) {
      return null;
    }
    html += renderPlainDiffVirtualRowsHtmlRangeCore({
      fileKey: request.fileKey,
      rows: request.rows,
      searchLineKeys,
      activeSearchLineKey: request.activeSearchLineKey,
      contextControls: request.contextControls,
      inlineDiff: request.inlineDiff,
      viewMode: request.viewMode,
    }, startIndex, startIndex + DIFF_RENDER_WORKER_ROW_SLICE_LIMIT);
    if (startIndex + DIFF_RENDER_WORKER_ROW_SLICE_LIMIT < request.rows.length) {
      await yieldToRenderWorkerEventLoop();
    }
  }
  return html;
}

// renderHighlightedDiffVirtualRowsHtmlInSlices renders highlighted virtual rows cooperatively.
// renderHighlightedDiffVirtualRowsHtmlInSlices 以协作式分片渲染高亮虚拟行。
async function renderHighlightedDiffVirtualRowsHtmlInSlices(
  request: Extract<DiffRenderWorkerRequest, { kind: "rows" }>,
  searchLineKeys: ReadonlySet<string> | undefined,
  highlightedLineSpans: ReadonlyMap<string, DiffShikiTokenSpan[]>,
): Promise<string | null> {
  let html = "";
  for (let startIndex = 0; startIndex < request.rows.length; startIndex += DIFF_RENDER_WORKER_ROW_SLICE_LIMIT) {
    if (isRenderRequestCancelled(request.id)) {
      return null;
    }
    html += renderHighlightedDiffVirtualRowsHtmlRangeCore({
      fileKey: request.fileKey,
      rows: request.rows,
      searchLineKeys,
      activeSearchLineKey: request.activeSearchLineKey,
      contextControls: request.contextControls,
      inlineDiff: request.inlineDiff,
      viewMode: request.viewMode,
      highlightedLineSpans,
    }, startIndex, startIndex + DIFF_RENDER_WORKER_ROW_SLICE_LIMIT);
    if (startIndex + DIFF_RENDER_WORKER_ROW_SLICE_LIMIT < request.rows.length) {
      await yieldToRenderWorkerEventLoop();
    }
  }
  return html;
}

// yieldToRenderWorkerEventLoop lets cancellation messages run between render slices.
// yieldToRenderWorkerEventLoop 让取消消息能在渲染分片之间执行。
function yieldToRenderWorkerEventLoop(): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
}

// cancelledRenderResponse returns a lightweight marker for internal queue handling.
// cancelledRenderResponse 返回内部队列处理使用的轻量取消标记。
function cancelledRenderResponse(id: number): DiffRenderWorkerResponse {
  return {
    id,
    cancelled: true,
  };
}

// rememberCancelledRenderRequest records one cooperative render cancellation.
// rememberCancelledRenderRequest 记录一次协作式渲染取消。
function rememberCancelledRenderRequest(requestId: number): void {
  if (currentRenderWorkerRequestId !== requestId && !queuedRenderWorkerRequestIdExists(requestId)) {
    return;
  }
  cancelledRenderRequestIds.add(requestId);
}

// queuedRenderWorkerRequestIdExists returns whether a request is still waiting in the worker queue.
// queuedRenderWorkerRequestIdExists 返回某个请求是否仍在 Worker 队列中等待。
function queuedRenderWorkerRequestIdExists(requestId: number): boolean {
  return queuedRenderWorkerRequestIds.has(requestId);
}

// isRenderRequestCancelled returns whether one request has been superseded by the main thread.
// isRenderRequestCancelled 返回某个请求是否已经被主线程替换。
function isRenderRequestCancelled(requestId: number): boolean {
  return cancelledRenderRequestIds.has(requestId);
}

// syncHighlightedLineSpansForCancelledRequest keeps the worker span cache aligned with the main-thread mirror.
// syncHighlightedLineSpansForCancelledRequest 让取消请求中的高亮 span 缓存仍与主线程镜像保持一致。
function syncHighlightedLineSpansForCancelledRequest(request: DiffRenderWorkerRequest): void {
  highlightedLineSpansForRequest(request);
}

// searchLineKeysForRequest returns a cached lookup set for repeated search-page renders.
// searchLineKeysForRequest 为重复搜索分页渲染返回缓存的查询集合。
function searchLineKeysForRequest(
  searchLineKeys: readonly string[] | undefined,
  signature: string | undefined,
): ReadonlySet<string> | undefined {
  if (!searchLineKeys?.length) {
    return undefined;
  }
  if (!signature) {
    return new Set(searchLineKeys);
  }
  const cached = renderWorkerSearchLineKeysCache.get(signature);
  if (cached) {
    return cached;
  }
  const searchLineKeySet = new Set(searchLineKeys);
  rememberWorkerSearchLineKeys(signature, searchLineKeySet);
  return searchLineKeySet;
}

// rememberWorkerSearchLineKeys stores one worker-side search lookup with FIFO eviction.
// rememberWorkerSearchLineKeys 以 FIFO 淘汰策略保存一个 Worker 侧搜索查询集合。
function rememberWorkerSearchLineKeys(signature: string, searchLineKeys: ReadonlySet<string>): void {
  if (renderWorkerSearchLineKeysCache.has(signature)) {
    renderWorkerSearchLineKeysCache.delete(signature);
  } else if (renderWorkerSearchLineKeysCache.size >= DIFF_RENDER_WORKER_SEARCH_SET_CACHE_LIMIT) {
    const oldestKey = renderWorkerSearchLineKeysCache.keys().next().value;
    if (oldestKey) {
      renderWorkerSearchLineKeysCache.delete(oldestKey);
    }
  }
  renderWorkerSearchLineKeysCache.set(signature, searchLineKeys);
}

// highlightedLineSpansForRequest applies span updates and returns the shared worker lookup map.
// highlightedLineSpansForRequest 应用 span 增量并返回共享的 Worker 查询映射。
function highlightedLineSpansForRequest(request: DiffRenderWorkerRequest): Map<string, DiffShikiTokenSpan[]> | undefined {
  if (request.resetHighlightedLineSpans) {
    renderWorkerHighlightedLineSpansCache.clear();
  }
  if (request.highlightedLineSpans?.length) {
    for (const update of request.highlightedLineSpans) {
      renderWorkerHighlightedLineSpansCache.set(update.lineKey, update.spans);
    }
  }
  if (!request.highlighted || !renderWorkerHighlightedLineSpansCache.size) {
    return undefined;
  }
  return renderWorkerHighlightedLineSpansCache;
}
