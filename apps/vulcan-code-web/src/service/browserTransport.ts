import type {
  StartupSnapshot,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";
import type {
  ServiceEventHandler,
  ServiceParams,
  ServiceTransport,
  ServiceUnsubscribe,
} from "@/service/protocol";

// BrowserServiceRpcResponse describes the local service HTTP RPC response envelope.
// BrowserServiceRpcResponse 描述本地服务 HTTP RPC 响应信封。
interface BrowserServiceRpcResponse<T = unknown> {
  // ok indicates whether the service method succeeded.
  // ok 表示服务方法是否执行成功。
  ok: boolean;
  // result stores the successful response payload.
  // result 保存成功响应载荷。
  result?: T;
  // error stores a normalized service error payload.
  // error 保存标准化服务错误载荷。
  error?: {
    // code is a stable machine-readable error code.
    // code 是稳定的机器可读错误码。
    code: string;
    // message is the human-readable error text.
    // message 是人类可读错误文本。
    message: string;
  };
}

// BrowserServiceEvent describes service events received through WebSocket.
// BrowserServiceEvent 描述通过 WebSocket 收到的服务事件。
type BrowserServiceEvent =
  | { kind: "startup"; payload: StartupSnapshot }
  | { kind: "workbench"; payload: WorkbenchSnapshot }
  | { kind: "workbenchDelta"; payload: WorkbenchDeltaEnvelope };

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

// EVENT_KIND_TO_TOPIC maps service event kinds to protocol topics.
// EVENT_KIND_TO_TOPIC 将服务事件类型映射为协议主题。
const EVENT_KIND_TO_TOPIC: Record<BrowserServiceEvent["kind"], string> = {
  startup: "startup",
  workbench: "workbench",
  workbenchDelta: "workbench.delta",
};

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
function emitBrowserServiceConnectionStatus(
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
async function resolveReachableBrowserServiceBaseUrl(): Promise<string> {
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

// clearBrowserServiceEndpointCache clears stale endpoint and token state after authorization failures.
// clearBrowserServiceEndpointCache 在鉴权失败后清理陈旧的端点和令牌状态。
function clearBrowserServiceEndpointCache(): void {
  resolvedBrowserServiceBaseUrl = null;
  resolvedBrowserServiceAccessToken = null;
  window.sessionStorage.removeItem(BROWSER_SERVICE_TOKEN_STORAGE_KEY);
}

// normalizeBrowserServiceAccessToken trims one candidate token.
// normalizeBrowserServiceAccessToken 裁剪单个候选访问令牌。
function normalizeBrowserServiceAccessToken(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

// isBrowserServiceRuntime returns whether the current JavaScript host can use the HTTP/WebSocket service transport.
// isBrowserServiceRuntime 返回当前 JavaScript 宿主是否可使用 HTTP/WebSocket 服务传输。
export function isBrowserServiceRuntime(): boolean {
  return typeof window.fetch === "function" && typeof window.WebSocket === "function";
}

// webSocketUrlFromBaseUrl converts an HTTP base URL into the WebSocket events URL.
// webSocketUrlFromBaseUrl 将 HTTP 基础地址转换为 WebSocket 事件地址。
function webSocketUrlFromBaseUrl(baseUrl: string): string {
  const url = new URL(`${baseUrl}/events`);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  const token = resolveBrowserServiceAccessToken();
  if (token) {
    url.searchParams.set("token", token);
  }
  return url.toString();
}

// BrowserServiceTransport talks to the local service through HTTP RPC and WebSocket events.
// BrowserServiceTransport 通过 HTTP RPC 与 WebSocket 事件连接本地服务。
export class BrowserServiceTransport implements ServiceTransport {
  // baseUrl stores the resolved local service HTTP endpoint.
  // baseUrl 保存已解析的本地服务 HTTP 端点。
  private baseUrl: string | null;

  // baseUrlPromise stores an in-flight endpoint discovery task.
  // baseUrlPromise 保存正在进行的端点发现任务。
  private baseUrlPromise: Promise<string> | null = null;

  // socket stores the active event WebSocket.
  // socket 保存当前活动事件 WebSocket。
  private socket: WebSocket | null = null;

  // reconnectTimer stores the pending reconnect timer id.
  // reconnectTimer 保存待执行的重连定时器标识。
  private reconnectTimer: number | null = null;

  // reconnectDelayMs stores the current reconnect backoff delay.
  // reconnectDelayMs 保存当前重连退避延迟。
  private reconnectDelayMs = 500;

  // disposed tracks whether the transport was intentionally closed.
  // disposed 标记传输是否已被主动关闭。
  private disposed = false;

  // handlers stores event handlers grouped by protocol topic.
  // handlers 按协议主题保存事件处理函数。
  private readonly handlers = new Map<string, Set<ServiceEventHandler<unknown>>>();

  // constructor creates one browser service transport.
  // constructor 创建单个浏览器服务传输。
  constructor(baseUrl: string | null = null) {
    this.baseUrl = baseUrl;
  }

  // request sends one RPC call through the local HTTP bridge.
  // request 通过本地 HTTP 桥发送单个 RPC 调用。
  async request<T>(method: string, params?: ServiceParams): Promise<T> {
    const baseUrl = await this.resolveBaseUrl();
    let response = await this.fetchRpc(baseUrl, method, params);
    if (response.status === 401) {
      this.baseUrl = null;
      this.baseUrlPromise = null;
      clearBrowserServiceEndpointCache();
      const refreshedBaseUrl = await this.resolveBaseUrl();
      response = await this.fetchRpc(refreshedBaseUrl, method, params);
    }
    if (!response.ok) {
      throw new Error(`Browser service RPC failed with HTTP ${response.status}.`);
    }
    const payload = await response.json() as BrowserServiceRpcResponse<T>;
    if (!payload.ok) {
      throw new Error(payload.error?.message || "Browser service RPC failed.");
    }
    return payload.result as T;
  }

  // fetchRpc sends the raw HTTP request for one RPC call.
  // fetchRpc 发送单次 RPC 调用的原始 HTTP 请求。
  private fetchRpc(baseUrl: string, method: string, params?: ServiceParams): Promise<Response> {
    const headers: Record<string, string> = {
      "content-type": "application/json",
    };
    const token = resolveBrowserServiceAccessToken();
    if (token) {
      headers["x-vulcan-service-token"] = token;
    }
    return fetch(`${baseUrl}/rpc`, {
      method: "POST",
      headers,
      body: JSON.stringify({ method, params: params ?? null }),
    });
  }

  // subscribe registers a topic handler and lazily opens the event WebSocket.
  // subscribe 注册主题处理函数，并按需打开事件 WebSocket。
  async subscribe<T>(topic: string, handler: ServiceEventHandler<T>): Promise<ServiceUnsubscribe> {
    const typedHandler = handler as ServiceEventHandler<unknown>;
    const topicHandlers = this.handlers.get(topic) ?? new Set<ServiceEventHandler<unknown>>();
    topicHandlers.add(typedHandler);
    this.handlers.set(topic, topicHandlers);
    this.disposed = false;
    void this.ensureSocket();
    return () => {
      const currentHandlers = this.handlers.get(topic);
      currentHandlers?.delete(typedHandler);
      if (currentHandlers && currentHandlers.size === 0) {
        this.handlers.delete(topic);
      }
      if (this.handlerCount() === 0) {
        this.clearReconnectTimer();
        this.socket?.close();
        this.socket = null;
      }
    };
  }

  // dispose closes the WebSocket and clears all handlers.
  // dispose 关闭 WebSocket 并清理所有处理函数。
  dispose(): void {
    this.disposed = true;
    this.handlers.clear();
    this.clearReconnectTimer();
    this.socket?.close();
    this.socket = null;
  }

  // ensureSocket opens the event WebSocket once.
  // ensureSocket 只打开一次事件 WebSocket。
  private async ensureSocket(): Promise<void> {
    if (this.disposed || this.socket || this.handlerCount() === 0) {
      return;
    }
    this.clearReconnectTimer();
    let baseUrl: string;
    try {
      baseUrl = await this.resolveBaseUrl();
    } catch {
      this.scheduleReconnect();
      return;
    }
    if (this.disposed || this.socket || this.handlerCount() === 0) {
      return;
    }
    const socket = new WebSocket(webSocketUrlFromBaseUrl(baseUrl));
    socket.addEventListener("message", (event) => {
      this.dispatchSocketMessage(event.data);
    });
    socket.addEventListener("open", () => {
      this.reconnectDelayMs = 500;
      emitBrowserServiceConnectionStatus("connected");
    });
    socket.addEventListener("close", () => {
      if (this.socket === socket) {
        this.socket = null;
      }
      // Active reconnects must rediscover the endpoint because service restarts rotate tokens.
      // 活跃重连必须重新发现端点，因为服务重启会轮换令牌。
      if (!this.disposed && this.handlerCount() > 0) {
        this.baseUrl = null;
        this.baseUrlPromise = null;
        clearBrowserServiceEndpointCache();
        emitBrowserServiceConnectionStatus("disconnected", "Browser service connection was closed.");
      }
      this.scheduleReconnect();
    });
    socket.addEventListener("error", () => {
      emitBrowserServiceConnectionStatus("disconnected", "Browser service connection failed.");
      socket.close();
    });
    this.socket = socket;
  }

  // scheduleReconnect queues a reconnect while active handlers still exist.
  // scheduleReconnect 在仍存在活动订阅时排队重连。
  private scheduleReconnect(): void {
    if (this.disposed || this.reconnectTimer !== null || this.handlerCount() === 0) {
      return;
    }
    const delay = this.reconnectDelayMs;
    this.reconnectDelayMs = Math.min(this.reconnectDelayMs * 2, 5000);
    emitBrowserServiceConnectionStatus("reconnecting");
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      void this.ensureSocket();
    }, delay);
  }

  // resolveBaseUrl returns the cached endpoint or discovers a reachable service.
  // resolveBaseUrl 返回已缓存端点，或发现一个可连接服务。
  private async resolveBaseUrl(): Promise<string> {
    if (this.baseUrl) {
      return this.baseUrl;
    }
    if (!this.baseUrlPromise) {
      this.baseUrlPromise = resolveReachableBrowserServiceBaseUrl();
    }
    try {
      this.baseUrl = await this.baseUrlPromise;
      emitBrowserServiceConnectionStatus("connected");
      return this.baseUrl;
    } catch (error) {
      this.baseUrl = null;
      this.baseUrlPromise = null;
      emitBrowserServiceConnectionStatus(
        "disconnected",
        error instanceof Error ? error.message : String(error),
      );
      throw error;
    } finally {
      if (this.baseUrl) {
        this.baseUrlPromise = null;
      }
    }
  }

  // clearReconnectTimer cancels any pending reconnect.
  // clearReconnectTimer 取消任何待执行的重连。
  private clearReconnectTimer(): void {
    if (this.reconnectTimer !== null) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  // handlerCount returns the total number of active event handlers.
  // handlerCount 返回当前活动事件处理函数总数。
  private handlerCount(): number {
    let count = 0;
    for (const handlers of this.handlers.values()) {
      count += handlers.size;
    }
    return count;
  }

  // dispatchSocketMessage parses and dispatches one WebSocket message.
  // dispatchSocketMessage 解析并分发单条 WebSocket 消息。
  private dispatchSocketMessage(data: unknown): void {
    if (typeof data !== "string") {
      return;
    }
    let event: BrowserServiceEvent;
    try {
      event = JSON.parse(data) as BrowserServiceEvent;
    } catch {
      return;
    }
    const topic = EVENT_KIND_TO_TOPIC[event.kind];
    if (!topic) {
      return;
    }
    const topicHandlers = this.handlers.get(topic);
    if (!topicHandlers) {
      return;
    }
    for (const handler of topicHandlers) {
      handler(event.payload);
    }
  }
}

// createBrowserServiceTransport creates the browser-mode local service transport.
// createBrowserServiceTransport 创建浏览器模式本地服务传输。
export function createBrowserServiceTransport(): BrowserServiceTransport {
  return new BrowserServiceTransport();
}
