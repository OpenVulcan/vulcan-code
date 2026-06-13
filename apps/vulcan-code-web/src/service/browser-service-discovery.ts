// BrowserServiceDiscoverySnapshot describes the service endpoint advertised by /health or dev discovery.
// BrowserServiceDiscoverySnapshot 描述 /health 或开发发现端点公布的服务端点。
interface BrowserServiceDiscoverySnapshot {
  // status is the service readiness marker.
  // status 是服务就绪标记。
  status?: string;
  // baseUrl is the browser-facing HTTP endpoint.
  // baseUrl 是面向浏览器的 HTTP 端点。
  baseUrl?: string;
  // port is the loopback TCP port used by the service.
  // port 是服务使用的回环 TCP 端口。
  port?: number;
  // serviceToken is the instance token required by protected service routes.
  // serviceToken 是受保护服务路由要求携带的实例令牌。
  serviceToken?: string;
}

// BrowserServiceEndpoint describes one reachable service endpoint and its access token.
// BrowserServiceEndpoint 描述一个可访问服务端点及其访问令牌。
interface BrowserServiceEndpoint {
  // baseUrl is the normalized service HTTP endpoint.
  // baseUrl 是标准化后的服务 HTTP 端点。
  baseUrl: string;
  // serviceToken is the optional instance token advertised by the service.
  // serviceToken 是服务公布的可选实例令牌。
  serviceToken: string | null;
}

// BROWSER_SERVICE_URL_STORAGE_KEY stores the manually overridden local service URL.
// BROWSER_SERVICE_URL_STORAGE_KEY 保存用户手动覆盖的本地服务地址。
const BROWSER_SERVICE_URL_STORAGE_KEY = "vulcan.service.url";

// BROWSER_SERVICE_TOKEN_STORAGE_KEY stores the current browser service instance token for media URLs.
// BROWSER_SERVICE_TOKEN_STORAGE_KEY 保存当前浏览器服务实例令牌，供媒体 URL 使用。
const BROWSER_SERVICE_TOKEN_STORAGE_KEY = "vulcan.service.token";

// DEFAULT_BROWSER_SERVICE_URL is the loopback endpoint started by the desktop service.
// DEFAULT_BROWSER_SERVICE_URL 是桌面服务启动的回环端点。
const DEFAULT_BROWSER_SERVICE_URL = "http://127.0.0.1:17891";

// BROWSER_SERVICE_PORT_END is the last loopback port probed for automatic discovery.
// BROWSER_SERVICE_PORT_END 是自动发现探测的最后一个回环端口。
const BROWSER_SERVICE_PORT_END = 17920;

// BROWSER_SERVICE_HEALTH_TIMEOUT_MS caps each discovery probe without treating brief startup stalls as offline.
// BROWSER_SERVICE_HEALTH_TIMEOUT_MS 限制单次发现探测时长，同时避免把短暂启动卡顿误判为离线。
const BROWSER_SERVICE_HEALTH_TIMEOUT_MS = 750;

// BROWSER_SERVICE_DISCOVERY_PATH is served by the Vite dev server from the latest service file.
// BROWSER_SERVICE_DISCOVERY_PATH 由 Vite 开发服务器从最新服务文件中提供。
const BROWSER_SERVICE_DISCOVERY_PATH = "/vulcan-service-discovery.json";

// BROWSER_SERVICE_CONNECTION_EVENT is emitted whenever the local service connection changes.
// BROWSER_SERVICE_CONNECTION_EVENT 会在本地服务连接状态变化时发出。
export const BROWSER_SERVICE_CONNECTION_EVENT = "vulcan:browser-service-connection";

// BrowserServiceConnectionStatus is the frontend-visible local service connection state.
// BrowserServiceConnectionStatus 是前端可见的本地服务连接状态。
export type BrowserServiceConnectionStatus = "connected" | "disconnected" | "reconnecting";

// BrowserServiceConnectionDetail describes one browser service connection state event.
// BrowserServiceConnectionDetail 描述单个浏览器服务连接状态事件。
export interface BrowserServiceConnectionDetail {
  // status is the current connection state.
  // status 是当前连接状态。
  status: BrowserServiceConnectionStatus;
  // message is the optional diagnostic message.
  // message 是可选诊断消息。
  message?: string;
}

// resolvedBrowserServiceBaseUrl stores the last positively probed service endpoint for sync helpers.
// resolvedBrowserServiceBaseUrl 保存最近一次明确探测成功的服务端点，供同步辅助函数复用。
let resolvedBrowserServiceBaseUrl: string | null = null;

// resolvedBrowserServiceAccessToken stores the last discovered protected-route token.
// resolvedBrowserServiceAccessToken 保存最近发现的受保护路由令牌。
let resolvedBrowserServiceAccessToken: string | null = null;

// lastBrowserServiceConnectionStatus stores the most recently emitted connection state.
// lastBrowserServiceConnectionStatus 保存最近一次发出的连接状态。
let lastBrowserServiceConnectionStatus: BrowserServiceConnectionStatus | null = null;

// emitBrowserServiceConnectionStatus notifies the UI about meaningful connection changes.
// emitBrowserServiceConnectionStatus 将有意义的连接变化通知 UI。
export function emitBrowserServiceConnectionStatus(
  status: BrowserServiceConnectionStatus,
  message?: string,
): void {
  if (lastBrowserServiceConnectionStatus === status && !message) {
    return;
  }
  lastBrowserServiceConnectionStatus = status;
  window.dispatchEvent(new CustomEvent<BrowserServiceConnectionDetail>(
    BROWSER_SERVICE_CONNECTION_EVENT,
    { detail: { status, message } },
  ));
}

// resolveBrowserServiceBaseUrl resolves the local service endpoint for browser mode.
// resolveBrowserServiceBaseUrl 解析浏览器模式使用的本地服务端点。
export function resolveBrowserServiceBaseUrl(): string {
  if (resolvedBrowserServiceBaseUrl) {
    return resolvedBrowserServiceBaseUrl;
  }
  const fromQuery = new URLSearchParams(window.location.search).get("service");
  const fromStorage = window.localStorage.getItem(BROWSER_SERVICE_URL_STORAGE_KEY);
  const importMeta = import.meta as ImportMeta & { env?: Record<string, string | undefined> };
  const fromEnv = importMeta.env?.VITE_VULCAN_SERVICE_URL;
  return (fromQuery || fromStorage || fromEnv || DEFAULT_BROWSER_SERVICE_URL).replace(/\/+$/, "");
}

// resolveBrowserServiceAccessToken returns the current browser service access token when known.
// resolveBrowserServiceAccessToken 返回已知的当前浏览器服务访问令牌。
export function resolveBrowserServiceAccessToken(): string | null {
  if (resolvedBrowserServiceAccessToken) {
    return resolvedBrowserServiceAccessToken;
  }
  const fromQuery = new URLSearchParams(window.location.search).get("serviceToken");
  const fromStorage = window.sessionStorage.getItem(BROWSER_SERVICE_TOKEN_STORAGE_KEY);
  const token = normalizeBrowserServiceAccessToken(fromQuery || fromStorage);
  resolvedBrowserServiceAccessToken = token;
  return token;
}

// resolveReachableBrowserServiceBaseUrl finds the active local service endpoint.
// resolveReachableBrowserServiceBaseUrl 查找当前可用的本地服务端点。
export async function resolveReachableBrowserServiceBaseUrl(): Promise<string> {
  const candidates = await browserServiceDiscoveryCandidates();
  for (const candidate of candidates) {
    const reachable = await probeBrowserServiceBaseUrl(candidate);
    if (reachable) {
      rememberBrowserServiceEndpoint(reachable);
      emitBrowserServiceConnectionStatus("connected");
      return reachable.baseUrl;
    }
  }
  emitBrowserServiceConnectionStatus("disconnected", "Browser service is unavailable.");
  throw new Error("Browser service is unavailable.");
}

// clearBrowserServiceEndpointCache clears stale endpoint and token state after authorization failures.
// clearBrowserServiceEndpointCache 在鉴权失败后清理陈旧的端点和令牌状态。
export function clearBrowserServiceEndpointCache(): void {
  resolvedBrowserServiceBaseUrl = null;
  resolvedBrowserServiceAccessToken = null;
  window.sessionStorage.removeItem(BROWSER_SERVICE_TOKEN_STORAGE_KEY);
}

// isBrowserServiceRuntime returns whether the current JavaScript host can use the HTTP/WebSocket service transport.
// isBrowserServiceRuntime 返回当前 JavaScript 宿主是否可使用 HTTP/WebSocket 服务传输。
export function isBrowserServiceRuntime(): boolean {
  return typeof window.fetch === "function" && typeof window.WebSocket === "function";
}

// webSocketUrlFromBaseUrl converts an HTTP base URL into the WebSocket events URL.
// webSocketUrlFromBaseUrl 将 HTTP 基础地址转换为 WebSocket 事件地址。
export function webSocketUrlFromBaseUrl(baseUrl: string): string {
  const url = new URL(`${baseUrl}/events`);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  const token = resolveBrowserServiceAccessToken();
  if (token) {
    url.searchParams.set("token", token);
  }
  return url.toString();
}

// browserServiceDiscoveryCandidates returns prioritized service endpoint candidates.
// browserServiceDiscoveryCandidates 返回按优先级排序的服务端点候选项。
async function browserServiceDiscoveryCandidates(): Promise<string[]> {
  const fromQuery = new URLSearchParams(window.location.search).get("service");
  const fromLocation = browserServiceBaseUrlFromLocation();
  const fromDiscovery = await readBrowserServiceDiscoveryBaseUrl();
  const fromStorage = window.localStorage.getItem(BROWSER_SERVICE_URL_STORAGE_KEY);
  const importMeta = import.meta as ImportMeta & { env?: Record<string, string | undefined> };
  const fromEnv = importMeta.env?.VITE_VULCAN_SERVICE_URL;
  const rangeCandidates = browserServicePortRangeCandidates();
  return uniqueBrowserServiceBaseUrls([
    fromQuery,
    fromLocation,
    fromDiscovery,
    fromStorage,
    fromEnv,
    DEFAULT_BROWSER_SERVICE_URL,
    ...rangeCandidates,
  ]);
}

// browserServiceBaseUrlFromLocation returns the current same-origin service URL when the page is served by the local service.
// browserServiceBaseUrlFromLocation 在页面由本地服务提供时返回当前同源服务地址。
function browserServiceBaseUrlFromLocation(): string | null {
  if (window.location.protocol !== "http:" && window.location.protocol !== "https:") {
    return null;
  }
  const hostname = window.location.hostname.toLowerCase();
  const isLoopback = hostname === "localhost" || hostname === "127.0.0.1" || hostname === "::1" || hostname === "[::1]";
  if (!isLoopback) {
    return null;
  }
  return window.location.origin;
}

// readBrowserServiceDiscoveryBaseUrl reads the same-origin service discovery document when available.
// readBrowserServiceDiscoveryBaseUrl 在可用时读取同源服务发现文档。
async function readBrowserServiceDiscoveryBaseUrl(): Promise<string | null> {
  try {
    const response = await fetch(BROWSER_SERVICE_DISCOVERY_PATH, {
      cache: "no-store",
    });
    if (!response.ok) {
      return null;
    }
    const payload = await response.json() as BrowserServiceDiscoverySnapshot;
    const endpoint = browserServiceEndpointFromDiscovery(payload, payload.baseUrl ?? null);
    if (endpoint) {
      rememberBrowserServiceEndpoint(endpoint);
      return endpoint.baseUrl;
    }
    return normalizeBrowserServiceBaseUrl(payload.baseUrl ?? null);
  } catch {
    return null;
  }
}

// browserServicePortRangeCandidates returns loopback URLs matching the backend bind range.
// browserServicePortRangeCandidates 返回与后端绑定范围一致的回环 URL。
function browserServicePortRangeCandidates(): string[] {
  const urls: string[] = [];
  const startPort = Number(new URL(DEFAULT_BROWSER_SERVICE_URL).port);
  for (let port = startPort; port <= BROWSER_SERVICE_PORT_END; port += 1) {
    urls.push(`http://127.0.0.1:${port}`);
  }
  return urls;
}

// uniqueBrowserServiceBaseUrls normalizes, filters, and deduplicates candidate URLs.
// uniqueBrowserServiceBaseUrls 标准化、过滤并去重候选 URL。
function uniqueBrowserServiceBaseUrls(values: Array<string | null | undefined>): string[] {
  const seen = new Set<string>();
  const urls: string[] = [];
  for (const value of values) {
    const normalized = normalizeBrowserServiceBaseUrl(value ?? null);
    if (!normalized || seen.has(normalized)) {
      continue;
    }
    seen.add(normalized);
    urls.push(normalized);
  }
  return urls;
}

// normalizeBrowserServiceBaseUrl trims and validates one candidate service URL.
// normalizeBrowserServiceBaseUrl 裁剪并校验单个候选服务 URL。
function normalizeBrowserServiceBaseUrl(value: string | null): string | null {
  const trimmed = value?.trim().replace(/\/+$/, "");
  if (!trimmed) {
    return null;
  }
  try {
    const url = new URL(trimmed);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      return null;
    }
    return url.toString().replace(/\/+$/, "");
  } catch {
    return null;
  }
}

// probeBrowserServiceBaseUrl checks whether one service endpoint responds to /health.
// probeBrowserServiceBaseUrl 检查单个服务端点是否响应 /health。
async function probeBrowserServiceBaseUrl(baseUrl: string): Promise<BrowserServiceEndpoint | null> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), BROWSER_SERVICE_HEALTH_TIMEOUT_MS);
  try {
    const response = await fetch(`${baseUrl}/health`, {
      cache: "no-store",
      signal: controller.signal,
    });
    if (!response.ok) {
      return null;
    }
    const contentType = response.headers.get("content-type") ?? "";
    if (!contentType.includes("application/json")) {
      return null;
    }
    const payload = await response.json() as BrowserServiceDiscoverySnapshot;
    const endpoint = browserServiceEndpointFromDiscovery(payload, payload.baseUrl ?? baseUrl);
    if (!endpoint) {
      return null;
    }
    rememberBrowserServiceEndpoint(endpoint);
    return endpoint;
  } catch {
    return null;
  } finally {
    window.clearTimeout(timeout);
  }
}

// browserServiceEndpointFromDiscovery normalizes one discovery payload into a usable endpoint.
// browserServiceEndpointFromDiscovery 将单个发现载荷标准化为可用端点。
function browserServiceEndpointFromDiscovery(
  payload: BrowserServiceDiscoverySnapshot,
  fallbackBaseUrl: string | null,
): BrowserServiceEndpoint | null {
  const baseUrl = normalizeBrowserServiceBaseUrl(payload.baseUrl ?? fallbackBaseUrl);
  if (!baseUrl) {
    return null;
  }
  return {
    baseUrl,
    serviceToken: normalizeBrowserServiceAccessToken(payload.serviceToken ?? null),
  };
}

// rememberBrowserServiceEndpoint stores one discovered endpoint for future sync helpers.
// rememberBrowserServiceEndpoint 保存一个已发现端点，供后续同步辅助逻辑使用。
function rememberBrowserServiceEndpoint(endpoint: BrowserServiceEndpoint): void {
  resolvedBrowserServiceBaseUrl = endpoint.baseUrl;
  resolvedBrowserServiceAccessToken = endpoint.serviceToken;
  window.localStorage.setItem(BROWSER_SERVICE_URL_STORAGE_KEY, endpoint.baseUrl);
  if (endpoint.serviceToken) {
    window.sessionStorage.setItem(BROWSER_SERVICE_TOKEN_STORAGE_KEY, endpoint.serviceToken);
  } else {
    window.sessionStorage.removeItem(BROWSER_SERVICE_TOKEN_STORAGE_KEY);
  }
}

// normalizeBrowserServiceAccessToken trims one candidate token.
// normalizeBrowserServiceAccessToken 裁剪单个候选访问令牌。
function normalizeBrowserServiceAccessToken(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}
