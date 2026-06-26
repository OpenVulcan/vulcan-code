import type {
  SessionAddonChangedEvent,
  StartupSnapshot,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";
import type {
  ServiceEventHandler,
  ServiceParams,
  ServiceRequestOptions,
  ServiceTransport,
  ServiceUnsubscribe,
} from "@/service/protocol";
import {
  clearBrowserServiceEndpointCache,
  emitBrowserServiceConnectionStatus,
  resolveBrowserServiceAccessToken,
  resolveReachableBrowserServiceBaseUrl,
  webSocketUrlFromBaseUrl,
} from "@/service/browser-service-discovery";

export {
  BROWSER_SERVICE_CONNECTION_EVENT,
  isBrowserServiceRuntime,
  resolveBrowserServiceAccessToken,
  resolveBrowserServiceBaseUrl,
  type BrowserServiceConnectionDetail,
  type BrowserServiceConnectionStatus,
} from "@/service/browser-service-discovery";

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
  | { kind: "workbenchDelta"; payload: WorkbenchDeltaEnvelope }
  | { kind: "sessionAddonChanged"; payload: SessionAddonChangedEvent };

// EVENT_KIND_TO_TOPIC maps service event kinds to protocol topics.
// EVENT_KIND_TO_TOPIC 将服务事件类型映射为协议主题。
const EVENT_KIND_TO_TOPIC: Record<BrowserServiceEvent["kind"], string> = {
  startup: "startup",
  workbench: "workbench",
  workbenchDelta: "workbench.delta",
  sessionAddonChanged: "session.addon.changed",
};

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
  async request<T>(method: string, params?: ServiceParams, options?: ServiceRequestOptions): Promise<T> {
    const baseUrl = await this.resolveBaseUrl();
    let response = await this.fetchRpc(baseUrl, method, params, options);
    if (response.status === 401) {
      this.baseUrl = null;
      this.baseUrlPromise = null;
      clearBrowserServiceEndpointCache();
      const refreshedBaseUrl = await this.resolveBaseUrl();
      response = await this.fetchRpc(refreshedBaseUrl, method, params, options);
    }
    if (!response.ok) {
      throw new Error(`Browser service RPC failed with HTTP ${response.status}.`);
    }
    const payload = await response.json() as BrowserServiceRpcResponse<T>;
    if (!payload.ok) {
      throw new Error(payload.error?.message || "Browser service RPC failed.");
    }
    emitBrowserServiceConnectionStatus("connected");
    return payload.result as T;
  }

  // fetchRpc sends the raw HTTP request for one RPC call.
  // fetchRpc 发送单次 RPC 调用的原始 HTTP 请求。
  private fetchRpc(baseUrl: string, method: string, params?: ServiceParams, options?: ServiceRequestOptions): Promise<Response> {
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
      signal: options?.signal,
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
