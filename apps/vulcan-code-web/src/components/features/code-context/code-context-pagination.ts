import type { VcePageResponse } from "@/types";

// VCE_MANAGEMENT_PAGE_SIZE bounds every management-page request from the WebUI.
// VCE_MANAGEMENT_PAGE_SIZE 限制 WebUI 发出的每项管理分页请求大小。
export const VCE_MANAGEMENT_PAGE_SIZE = 25;

// VceManagementPageState retains one opaque current cursor and its exact backward history.
// VceManagementPageState 保存一个不透明当前游标及其精确后退历史。
export interface VceManagementPageState<T> {
  // Current stable records returned by the service.
  // 服务返回的当前稳定记录。
  items: T[];
  // Opaque cursor used to load the current page.
  // 加载当前页所使用的不透明游标。
  cursor: string | null;
  // Opaque cursor returned for the next page.
  // 为下一页返回的不透明游标。
  nextCursor: string | null;
  // Exact total when supplied by the service.
  // 服务提供时的精确总数。
  total: number | null;
  // Exact earlier cursors ordered from the first page to the immediate previous page.
  // 从第一页到紧邻上一页依次排列的精确历史游标。
  previousCursors: Array<string | null>;
}

// VceManagementPageRequest describes one safe forward or backward page transition.
// VceManagementPageRequest 描述一次安全的向前或向后分页转换。
export interface VceManagementPageRequest {
  // Opaque cursor to send to the service.
  // 发送给服务的不透明游标。
  cursor: string | null;
  // Backward history to retain after the requested page is loaded.
  // 请求页加载后需要保留的后退历史。
  previousCursors: Array<string | null>;
}

// emptyVceManagementPage creates the explicit unloaded first-page state.
// emptyVceManagementPage 创建明确的未加载第一页状态。
export function emptyVceManagementPage<T>(): VceManagementPageState<T> {
  return {
    items: [],
    cursor: null,
    nextCursor: null,
    total: null,
    previousCursors: [],
  };
}

// materializeVceManagementPage binds one service response to the cursor that produced it.
// materializeVceManagementPage 将一项服务响应绑定到产生它的游标。
export function materializeVceManagementPage<T>(
  response: VcePageResponse<T>,
  request: VceManagementPageRequest,
): VceManagementPageState<T> {
  return {
    items: response.items,
    cursor: request.cursor,
    nextCursor: response.nextCursor ?? null,
    total: response.total ?? null,
    previousCursors: request.previousCursors,
  };
}

// nextVceManagementPageRequest advances only when the service returned an opaque continuation.
// nextVceManagementPageRequest 仅在服务返回不透明继续游标时前进。
export function nextVceManagementPageRequest<T>(
  page: VceManagementPageState<T>,
): VceManagementPageRequest | null {
  if (!page.nextCursor) {
    return null;
  }
  return {
    cursor: page.nextCursor,
    previousCursors: [...page.previousCursors, page.cursor],
  };
}

// previousVceManagementPageRequest returns to the exact prior opaque cursor without decoding it.
// previousVceManagementPageRequest 在不解析游标的前提下返回精确的上一游标。
export function previousVceManagementPageRequest<T>(
  page: VceManagementPageState<T>,
): VceManagementPageRequest | null {
  if (page.previousCursors.length === 0) {
    return null;
  }
  return {
    cursor: page.previousCursors[page.previousCursors.length - 1] ?? null,
    previousCursors: page.previousCursors.slice(0, -1),
  };
}
