import type { NotificationRouteV1 } from "@/notifications/types";

// ROUTE_STORAGE_KEY keeps click targets across a reload of the same client surface.
// ROUTE_STORAGE_KEY 在同一客户端页面重载后保留通知点击目标。
const ROUTE_STORAGE_KEY = "vulcan.notifications.routes.v1";

// MAX_STORED_ROUTES matches the coordinator's bounded in-memory activation map.
// MAX_STORED_ROUTES 与协调器有界的内存激活映射保持一致。
const MAX_STORED_ROUTES = 512;

// MAX_ROUTE_DOCUMENT_LENGTH accommodates every bounded route while rejecting oversized data.
// MAX_ROUTE_DOCUMENT_LENGTH 容纳所有有界路由，同时拒绝过大的存储数据。
const MAX_ROUTE_DOCUMENT_LENGTH = 1_048_576;

// NOTIFICATION_ID_PATTERN matches the native host's accepted toast tag format.
// NOTIFICATION_ID_PATTERN 与原生宿主接受的通知标签格式一致。
const NOTIFICATION_ID_PATTERN = /^[A-Za-z0-9:._-]{1,64}$/;

// NotificationRouteStorage is the session-scoped storage needed for reload recovery.
// NotificationRouteStorage 是重载恢复所需的会话级存储接口。
export interface NotificationRouteStorage {
  // getItem reads one stored route document or null when it is absent.
  // getItem 读取一份已存储的路由文档，不存在时返回 null。
  getItem(key: string): string | null;
  // setItem writes one complete route document for the current client.
  // setItem 为当前客户端写入一份完整的路由文档。
  setItem(key: string, value: string): void;
}

// StoredNotificationRoute pairs one delivered notification ID with its exact route.
// StoredNotificationRoute 将一条已投递通知的 ID 与其精确路由配对。
export interface StoredNotificationRoute {
  // notificationId is the native or browser notification correlation ID.
  // notificationId 是原生或网页通知的关联 ID。
  notificationId: string;
  // route is the conversation opened when that notification is activated.
  // route 是激活该通知时打开的会话。
  route: NotificationRouteV1;
}

// currentNotificationRouteStorage obtains this page's sessionStorage when available.
// currentNotificationRouteStorage 在可用时取得当前页面的 sessionStorage。
// It returns null in non-browser or restricted contexts.
// 它在非浏览器或受限环境中返回 null。
export function currentNotificationRouteStorage(): NotificationRouteStorage | null {
  try {
    return typeof globalThis.sessionStorage === "undefined" ? null : globalThis.sessionStorage;
  } catch {
    return null;
  }
}

// isStoredNotificationRoute checks the exact persisted shape before using a click target.
// isStoredNotificationRoute 在使用点击目标前检查其持久化结构。
// candidate is untrusted storage data; the return value narrows it to a route record.
// candidate 是不可信存储数据；返回值将其收窄为路由记录。
function isStoredNotificationRoute(candidate: unknown): candidate is StoredNotificationRoute {
  if (candidate === null || typeof candidate !== "object" || Array.isArray(candidate)) return false;
  // record is the untrusted top-level route entry being checked.
  // record 是正在检查的不可信顶层路由条目。
  const record = candidate as Record<string, unknown>;
  if (typeof record.notificationId !== "string" || !NOTIFICATION_ID_PATTERN.test(record.notificationId)) return false;
  if (record.route === null || typeof record.route !== "object" || Array.isArray(record.route)) return false;
  // route is the untrusted navigation target inside that entry.
  // route 是该条目内不可信的导航目标。
  const route = record.route as Record<string, unknown>;
  return route.kind === "conversation"
    && typeof route.conversationId === "string" && route.conversationId.length > 0 && route.conversationId.length <= 256
    && (route.projectId === null || (typeof route.projectId === "string" && route.projectId.length > 0 && route.projectId.length <= 256));
}

// loadNotificationRoutes restores only routes written by this exact client instance.
// loadNotificationRoutes 仅恢复由此精确客户端实例写入的路由。
// storage is the page-local store and clientInstanceId is its owner; invalid data yields no routes.
// storage 是页面本地存储，clientInstanceId 是其所有者；无效数据返回空路由。
export function loadNotificationRoutes(
  storage: NotificationRouteStorage | null,
  clientInstanceId: string,
): StoredNotificationRoute[] {
  if (storage === null) return [];
  try {
    // raw is capped before parsing to avoid unbounded work from edited page storage.
    // raw 在解析前限制长度，避免被修改的页面存储造成无界处理。
    const raw = storage.getItem(ROUTE_STORAGE_KEY);
    if (raw === null || raw.length > MAX_ROUTE_DOCUMENT_LENGTH) return [];
    // parsed is not trusted until the entire document and every route are checked.
    // parsed 在整份文档与每条路由通过检查前都不可信。
    const parsed: unknown = JSON.parse(raw);
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) return [];
    // document exposes the version, owner and route list for exact validation.
    // document 提供版本、所有者及路由列表供精确校验。
    const document = parsed as Record<string, unknown>;
    if (document.version !== 1 || document.clientInstanceId !== clientInstanceId
      || !Array.isArray(document.routes) || document.routes.length > MAX_STORED_ROUTES
      || !document.routes.every(isStoredNotificationRoute)) return [];
    return document.routes;
  } catch {
    return [];
  }
}

// saveNotificationRoutes replaces the current client's bounded activation document.
// saveNotificationRoutes 替换当前客户端有界的激活文档。
// storage may be unavailable; clientInstanceId owns every route; failures preserve live routing.
// storage 可能不可用；clientInstanceId 拥有全部路由；写入失败不影响内存路由。
export function saveNotificationRoutes(
  storage: NotificationRouteStorage | null,
  clientInstanceId: string,
  routes: readonly StoredNotificationRoute[],
): void {
  if (storage === null) return;
  try {
    storage.setItem(ROUTE_STORAGE_KEY, JSON.stringify({
      version: 1,
      clientInstanceId,
      routes: routes.slice(-MAX_STORED_ROUTES),
    }));
  } catch {
    // A disabled or full sessionStorage must not prevent notification delivery.
    // 被禁用或写满的 sessionStorage 不能阻止通知投递。
  }
}
