import { readElectronServiceConnection } from "@/host-shell/electron-bridge";
import { notificationClientInstanceId } from "@/notifications/client-identity";

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
  // pid is the owning service process identifier used for instance matching.
  // pid 是用于实例匹配的服务进程标识。
  pid?: number;
  // startedAt is the UTC startup timestamp used for instance matching.
  // startedAt 是用于实例匹配的 UTC 启动时间戳。
  startedAt?: string;
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
  // pid is the owning service process identifier used for instance matching.
  // pid 是用于实例匹配的服务进程标识。
  pid: number | null;
  // startedAt is the UTC startup timestamp used for instance matching.
  // startedAt 是用于实例匹配的 UTC 启动时间戳。
  startedAt: string | null;
  // serviceToken is the optional instance token advertised by the service.
  // serviceToken 是服务公布的可选实例令牌。
  serviceToken: string | null;
}

// BrowserServiceProbeResult separates a missing service from one that answered but refused this page.
// BrowserServiceProbeResult 区分服务不存在与服务已响应但拒绝当前页面两种情况。
type BrowserServiceProbeResult =
  | { kind: "reachable"; endpoint: BrowserServiceEndpoint }
  | { kind: "rejected"; baseUrl: string; status: number }
  | { kind: "absent" };

// browserServiceRejectedMessage describes a service that is running but refuses this page, which retrying cannot fix.
// browserServiceRejectedMessage 描述服务正在运行但拒绝当前页面的情况，该情况无法通过重试恢复。
function browserServiceRejectedMessage(baseUrl: string, status: number): string {
  return `The browser service at ${baseUrl} rejected this page with HTTP ${status}.`;
}

// BROWSER_SERVICE_URL_STORAGE_KEY stores the manually overridden local service URL.
// BROWSER_SERVICE_URL_STORAGE_KEY 保存用户手动覆盖的本地服务地址。
const BROWSER_SERVICE_URL_STORAGE_KEY = "vulcan.service.url";

// BROWSER_SERVICE_TOKEN_STORAGE_KEY stores the current browser service instance token for media URLs.
// BROWSER_SERVICE_TOKEN_STORAGE_KEY 保存当前浏览器服务实例令牌，供媒体 URL 使用。
const BROWSER_SERVICE_TOKEN_STORAGE_KEY = "vulcan.service.token";

// DEFAULT_BROWSER_SERVICE_URL is the profile-specific loopback endpoint started by the desktop service.
// DEFAULT_BROWSER_SERVICE_URL 是桌面服务按构建配置启动的回环端点。
const DEFAULT_BROWSER_SERVICE_URL = import.meta.env.DEV
  ? "http://127.0.0.1:17921"
  : "http://127.0.0.1:17891";

// BROWSER_SERVICE_PORT_END is the profile-specific last loopback port probed for automatic discovery.
// BROWSER_SERVICE_PORT_END 是自动发现按构建配置探测的最后一个回环端口。
const BROWSER_SERVICE_PORT_END = import.meta.env.DEV ? 17950 : 17920;

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

// currentBrowserServiceConnectionStatus returns the latest emitted local-service connection state.
// currentBrowserServiceConnectionStatus 返回最近一次已发出的本地服务连接状态。
export function currentBrowserServiceConnectionStatus(): BrowserServiceConnectionStatus | null {
  return lastBrowserServiceConnectionStatus;
}

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

// resolveBrowserServiceClientId returns the identity shared by notifications, terminal attaches, and the event socket.
// resolveBrowserServiceClientId 返回由通知、终端挂接与事件套接字共享的标识。
export function resolveBrowserServiceClientId(): string {
  return notificationClientInstanceId;
}

// BrowserServiceCandidatePlan separates an explicit selection from fallback discovery candidates.
// BrowserServiceCandidatePlan 将明确选择与回退发现候选项区分开。
export interface BrowserServiceCandidatePlan {
  // pinnedBaseUrl is the exclusively selected endpoint, or null when nothing was explicitly selected.
  // pinnedBaseUrl 是被排他选中的端点；未明确选择时为 null。
  pinnedBaseUrl: string | null;
  // candidates are the fallback endpoints, probed only when nothing was pinned.
  // candidates 是回退端点，仅在未固定任何端点时探测。
  candidates: string[];
}

// BrowserServiceCandidateSources lists every raw endpoint source in documented priority order.
// BrowserServiceCandidateSources 按文档规定的优先级列出全部原始端点来源。
export interface BrowserServiceCandidateSources {
  // fromQuery is the explicit `?service=` selection from the page URL.
  // fromQuery 是页面 URL 中显式的 `?service=` 选择。
  fromQuery: string | null;
  // fromEnv is the explicit build-time service URL.
  // fromEnv 是构建期显式的服务 URL。
  fromEnv: string | null;
  // fromLocation is the same-origin URL when the page is served by the local service.
  // fromLocation 是页面由本地服务提供时的同源 URL。
  fromLocation: string | null;
  // fromDiscovery is the endpoint published by the authoritative same-origin discovery document.
  // fromDiscovery 是同源权威发现文档发布的端点。
  fromDiscovery: string | null;
  // fromStorage is the cached endpoint from an earlier successful connection.
  // fromStorage 是此前成功连接的缓存端点。
  fromStorage: string | null;
  // fromDefault is the documented default loopback endpoint.
  // fromDefault 是文档规定的默认回环端点。
  fromDefault: string | null;
  // rangeCandidates are the last-resort loopback endpoints inside the backend bind range.
  // rangeCandidates 是后端绑定范围内最后的回环端点兜底。
  rangeCandidates: string[];
}

// planBrowserServiceCandidates pins an explicit selection and never allows silent switching.
// planBrowserServiceCandidates 固定明确选择，且绝不允许静默切换。
export function planBrowserServiceCandidates(
  sources: BrowserServiceCandidateSources,
): BrowserServiceCandidatePlan {
  // An explicit selection is authoritative: if it is down the client must report an error
  // instead of connecting to another running instance.
  // 明确选择是权威的：若其不可用，客户端必须报告错误，而不是连接到另一个运行中的实例。
  if (sources.fromQuery !== null || sources.fromEnv !== null) {
    const selected = sources.fromQuery !== null ? sources.fromQuery : sources.fromEnv;
    const pinnedBaseUrl = normalizeBrowserServiceBaseUrl(selected);
    if (!pinnedBaseUrl) {
      throw new Error("The explicitly selected browser service URL is invalid.");
    }
    return { pinnedBaseUrl, candidates: [] };
  }
  return {
    pinnedBaseUrl: null,
    candidates: uniqueBrowserServiceBaseUrls([
      sources.fromLocation,
      sources.fromDiscovery,
      sources.fromStorage,
      sources.fromDefault,
      ...sources.rangeCandidates,
    ]),
  };
}

// resolveReachableBrowserServiceBaseUrl finds the active local service endpoint.
// resolveReachableBrowserServiceBaseUrl 查找当前可用的本地服务端点。
export async function resolveReachableBrowserServiceBaseUrl(): Promise<string> {
  // The GUI has already selected and authenticated Core; renderer discovery cannot override it.
  // GUI 已选定并验证 Core，渲染端发现流程不能覆盖此选择。
  const desktopConnection = await readElectronServiceConnection();
  if (desktopConnection) {
    if (desktopConnection.status !== "ready") {
      throw new Error("The selected desktop service is not ready.");
    }
    rememberBrowserServiceEndpoint(desktopConnection);
    emitBrowserServiceConnectionStatus("connected");
    return desktopConnection.baseUrl;
  }
  let plan: BrowserServiceCandidatePlan;
  try {
    plan = await browserServiceDiscoveryPlan();
  } catch (error) {
    // Invalid explicit selections must become visible connection failures as well as rejected calls.
    // 非法显式选择既要拒绝调用，也要进入可见的连接失败状态。
    emitBrowserServiceConnectionStatus("disconnected", String(error));
    throw error;
  }
  if (plan.pinnedBaseUrl) {
    const probe = await probeBrowserServiceBaseUrl(plan.pinnedBaseUrl);
    if (probe.kind === "reachable" && probe.endpoint.baseUrl === plan.pinnedBaseUrl) {
      rememberBrowserServiceEndpoint(probe.endpoint);
      emitBrowserServiceConnectionStatus("connected");
      return probe.endpoint.baseUrl;
    }
    // Falling through to another instance here would silently connect the user to the wrong Core.
    // 此处若继续回退到另一个实例，会把用户静默连接到错误的 Core。
    const message = probe.kind === "rejected"
      ? browserServiceRejectedMessage(probe.baseUrl, probe.status)
      : `The selected browser service ${plan.pinnedBaseUrl} is unavailable.`;
    emitBrowserServiceConnectionStatus("disconnected", message);
    throw new Error(message);
  }
  // firstRejection keeps scanning for an accessible instance, but a refusal must not look like an absent service.
  // firstRejection 仍继续查找可访问实例，但拒绝访问不得被当作服务不存在。
  let firstRejection: Extract<BrowserServiceProbeResult, { kind: "rejected" }> | null = null;
  for (const candidate of plan.candidates) {
    const probe = await probeBrowserServiceBaseUrl(candidate);
    if (probe.kind === "reachable") {
      rememberBrowserServiceEndpoint(probe.endpoint);
      emitBrowserServiceConnectionStatus("connected");
      return probe.endpoint.baseUrl;
    }
    if (probe.kind === "rejected" && !firstRejection) {
      firstRejection = probe;
    }
  }
  if (firstRejection) {
    // The rejection message is deliberately not retryable, so startup shows an error instead of rescanning forever.
    // 拒绝消息刻意不可重试，使启动页显示错误而不是无休止地重新扫描。
    const message = browserServiceRejectedMessage(firstRejection.baseUrl, firstRejection.status);
    emitBrowserServiceConnectionStatus("disconnected", message);
    throw new Error(message);
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
  url.searchParams.set("clientId", resolveBrowserServiceClientId());
  return url.toString();
}

// browserServiceDiscoveryPlan collects every raw source and returns the candidate plan.
// browserServiceDiscoveryPlan 收集全部原始来源并返回候选计划。
async function browserServiceDiscoveryPlan(): Promise<BrowserServiceCandidatePlan> {
  const importMeta = import.meta as ImportMeta & { env?: Record<string, string | undefined> };
  // An explicit target must be validated before even reading another instance's discovery.
  // 显式目标须先完成校验，不能预先读取其他实例的发现文档。
  const fromQuery = new URLSearchParams(window.location.search).get("service");
  const fromEnv = importMeta.env?.VITE_VULCAN_SERVICE_URL ?? null;
  return planBrowserServiceCandidates({
    fromQuery,
    fromEnv,
    fromLocation: browserServiceBaseUrlFromLocation(),
    fromDiscovery: fromQuery !== null || fromEnv !== null
      ? null : await readBrowserServiceDiscoveryBaseUrl(),
    fromStorage: window.localStorage.getItem(BROWSER_SERVICE_URL_STORAGE_KEY),
    fromDefault: DEFAULT_BROWSER_SERVICE_URL,
    rangeCandidates: browserServicePortRangeCandidates(),
  });
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
    // A service selection names an origin, never a path or embedded credential.
    // 服务选择只能指定来源，不能携带路径或内嵌凭据。
    if (url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
      return null;
    }
    return url.toString().replace(/\/+$/, "");
  } catch {
    return null;
  }
}

// probeBrowserServiceBaseUrl checks whether one service endpoint responds to /health.
// probeBrowserServiceBaseUrl 检查单个服务端点是否响应 /health。
// Returns: reachable with the endpoint, rejected for an HTTP 401/403 answer, or absent otherwise.
// 返回：可访问时附带端点；HTTP 401/403 时为 rejected；其余情况为 absent。
async function probeBrowserServiceBaseUrl(baseUrl: string): Promise<BrowserServiceProbeResult> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), BROWSER_SERVICE_HEALTH_TIMEOUT_MS);
  try {
    const response = await fetch(`${baseUrl}/health`, {
      cache: "no-store",
      signal: controller.signal,
    });
    if (response.status === 401 || response.status === 403) {
      return { kind: "rejected", baseUrl, status: response.status };
    }
    if (!response.ok) {
      return { kind: "absent" };
    }
    const contentType = response.headers.get("content-type") ?? "";
    if (!contentType.includes("application/json")) {
      return { kind: "absent" };
    }
    const payload = await response.json() as BrowserServiceDiscoverySnapshot;
    const endpoint = browserServiceEndpointFromDiscovery(payload, payload.baseUrl ?? baseUrl);
    return endpoint ? { kind: "reachable", endpoint } : { kind: "absent" };
  } catch {
    return { kind: "absent" };
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
    pid: typeof payload.pid === "number" ? payload.pid : null,
    startedAt: typeof payload.startedAt === "string" && payload.startedAt ? payload.startedAt : null,
    serviceToken: normalizeBrowserServiceAccessToken(payload.serviceToken ?? null),
  };
}

// browserServiceIdentityMatches reports whether two endpoints describe the same service generation.
// browserServiceIdentityMatches 报告两个端点是否描述同一服务代际。
export function browserServiceIdentityMatches(
  first: Pick<BrowserServiceEndpoint, "pid" | "startedAt" | "serviceToken"> | null,
  second: Pick<BrowserServiceEndpoint, "pid" | "startedAt" | "serviceToken"> | null,
): boolean {
  if (!first || !second) {
    return false;
  }
  if (first.pid === null || second.pid === null || first.pid !== second.pid) {
    return false;
  }
  if (!first.startedAt || !second.startedAt || first.startedAt !== second.startedAt) {
    return false;
  }
  if (!first.serviceToken || !second.serviceToken || first.serviceToken !== second.serviceToken) {
    return false;
  }
  return true;
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
