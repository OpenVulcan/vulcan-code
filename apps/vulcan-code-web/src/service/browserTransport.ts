import type {
  ConversationRunSettledEvent,
  CommandSessionDisplayChangedEvent,
  PluginUiInvalidatedEvent,
  ProjectTerminalDisplayChangedEvent,
  ProjectTerminalRunningCountChangedEvent,
  SessionAddonChangedEvent,
  StartupSnapshot,
  WorkspaceGitChangedEvent,
  VceProjectChangedEvent,
  VceJobSummary,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";
import enUSTranslations from "@catalogs/webui/en-US.json";
import { formatMessage, translateActive, type TranslationKey } from "@/i18n";
import type { ConversationEventLiveDeltaV1 } from "@/service/conversation-event-window-client";
import {
  DEDICATED_PROMPT_DRAFT_PROGRESS_TOPIC,
  type DedicatedPromptDraftProgressEvent,
} from "@/service/dedicated-prompt-draft-events";
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
    // message is the human-readable error text in the service's default language.
    // message 是服务默认语言的人类可读错误文本。
    message: string;
    // localized is the catalog-backed form of message, rendered in the active language when present.
    // localized 是 message 的目录形式，存在时按当前语言渲染。
    localized?: BrowserServiceLocalizedMessage;
  };
}

// BrowserServiceLocalizedMessage names one webui catalog key and its placeholder values.
// BrowserServiceLocalizedMessage 指定一个 webui 目录键及其占位符取值。
export interface BrowserServiceLocalizedMessage {
  // key is the stable webui catalog key.
  // key 是稳定的 webui 目录键。
  key: string;
  // params maps placeholder names to their values; omitted when the text has no placeholders.
  // params 将占位符名称映射到取值；文案没有占位符时省略。
  params?: Record<string, string>;
}

// renderServiceErrorMessage chooses the displayed text of one failed RPC envelope.
// renderServiceErrorMessage 选择单个失败 RPC 信封的展示文本。
//
// # Parameters
// 参数
// `message` is the service's default-language text and `localized` its optional catalog form.
// `message` 是服务默认语言文本，`localized` 是其可选目录形式。
//
// # Returns
// 返回值
// The catalog text in the active language, or `message` when this build does not know the key.
// 返回当前语言的目录文案；本构建不认识该键时返回 `message`。
export function renderServiceErrorMessage(message: string, localized?: BrowserServiceLocalizedMessage): string {
  if (!localized || !Object.hasOwn(enUSTranslations, localized.key)) {
    return message;
  }
  return formatMessage(translateActive(localized.key as TranslationKey), localized.params ?? {});
}

// BrowserServiceRpcError preserves the server code for control flow and the message for display.
// BrowserServiceRpcError 保留服务端错误码用于控制流程，保留文案用于展示。
export class BrowserServiceRpcError extends Error {
  // code comes directly from the RPC envelope, independently of its message language.
  // code 直接来自 RPC 信封，不依赖消息语言。
  readonly code: string;

  // Constructs an error from the declared wire code and display message; returns the error instance.
  // 根据声明的线协议错误码和展示消息构造错误；返回错误实例。
  constructor(code: string, message: string) {
    super(message);
    this.name = "BrowserServiceRpcError";
    this.code = code;
  }
}

// BrowserServiceEvent describes service events received through WebSocket.
// BrowserServiceEvent 描述通过 WebSocket 收到的服务事件。
type BrowserServiceEvent =
  | { kind: "startup"; payload: StartupSnapshot }
  | { kind: "workbench"; payload: WorkbenchSnapshot }
  | { kind: "workbenchDelta"; payload: WorkbenchDeltaEnvelope }
  | { kind: "conversationRunSettled"; payload: ConversationRunSettledEvent }
  | { kind: "conversationEventDeltaV1"; payload: ConversationEventLiveDeltaV1 }
  | { kind: "sessionAddonChanged"; payload: SessionAddonChangedEvent }
  | { kind: "pluginUiInvalidated"; payload: PluginUiInvalidatedEvent }
  | { kind: "workspaceGitChanged"; payload: WorkspaceGitChangedEvent }
  | { kind: "vceProjectChanged"; payload: VceProjectChangedEvent }
  | { kind: "vceJobChanged"; payload: VceJobSummary }
  | { kind: "commandSessionDisplayChanged"; payload: CommandSessionDisplayChangedEvent }
  | { kind: "projectTerminalDisplayChanged"; payload: ProjectTerminalDisplayChangedEvent }
  | { kind: "projectTerminalRunningCountChanged"; payload: ProjectTerminalRunningCountChangedEvent }
  | { kind: "dedicatedPromptDraftProgress"; payload: DedicatedPromptDraftProgressEvent };

// EVENT_KIND_TO_TOPIC maps service event kinds to protocol topics.
// EVENT_KIND_TO_TOPIC 将服务事件类型映射为协议主题。
const EVENT_KIND_TO_TOPIC: Record<BrowserServiceEvent["kind"], string> = {
  startup: "startup",
  workbench: "workbench",
  workbenchDelta: "workbench.delta",
  conversationRunSettled: "conversation.run.settled",
  conversationEventDeltaV1: "conversation.event.delta.v1",
  sessionAddonChanged: "session.addon.changed",
  pluginUiInvalidated: "plugin.ui.invalidated",
  workspaceGitChanged: "workspace.git.changed",
  vceProjectChanged: "vce.project.changed",
  vceJobChanged: "vce.job.changed",
  commandSessionDisplayChanged: "command.session.display.changed",
  projectTerminalDisplayChanged: "project.terminal.display.changed",
  projectTerminalRunningCountChanged: "project.terminal.running-count.changed",
  dedicatedPromptDraftProgress: DEDICATED_PROMPT_DRAFT_PROGRESS_TOPIC,
};

// closeWebSocketWhenReady closes one socket without aborting its opening handshake.
// closeWebSocketWhenReady 在不中断建连握手的前提下关闭一个套接字。
//
// Parameters
// 参数
// `socket` is the exact browser WebSocket and `shouldClose` revalidates ownership when it opens.
// `socket` 是精确的浏览器 WebSocket，`shouldClose` 在连接建立时重新校验所有权。
//
// Returns
// 返回值
// This function returns after closing an open socket or registering a one-shot deferred close.
// 函数在关闭已连接套接字或登记一次性延迟关闭后返回。
export function closeWebSocketWhenReady(
  socket: WebSocket,
  shouldClose: () => boolean,
): void {
  if (socket.readyState === socket.CONNECTING) {
    socket.addEventListener("open", () => {
      if (shouldClose()) {
        socket.close();
      }
    }, { once: true });
    return;
  }
  if (socket.readyState === socket.OPEN && shouldClose()) {
    socket.close();
  }
}

// waitForWebSocketOpen resolves only after one exact socket can deliver service events.
// waitForWebSocketOpen 仅在精确套接字已经能够传递服务事件后解析。
//
// Parameters
// 参数
// `socket` is the connection whose opening handshake must complete before callers proceed.
// `socket` 是调用方继续执行前必须完成建连握手的连接。
//
// Returns
// 返回值
// A promise that resolves on `open` and rejects when the connection fails or closes first.
// 返回在 `open` 时解析、在连接先失败或关闭时拒绝的 Promise。
export function waitForWebSocketOpen(socket: WebSocket): Promise<void> {
  if (socket.readyState === socket.OPEN) {
    return Promise.resolve();
  }
  if (socket.readyState !== socket.CONNECTING) {
    return Promise.reject(new Error("Browser service event WebSocket is not connectable."));
  }
  return new Promise<void>((resolve, reject) => {
    // cleanup removes every temporary handshake observer after one terminal outcome.
    // cleanup 在任一终态结果后移除全部临时握手观察器。
    const cleanup = () => {
      socket.removeEventListener("open", handleOpen);
      socket.removeEventListener("error", handleError);
      socket.removeEventListener("close", handleClose);
    };
    // handleOpen completes the event-channel readiness barrier.
    // handleOpen 完成事件通道就绪屏障。
    const handleOpen = () => {
      cleanup();
      resolve();
    };
    // handleError rejects a handshake that cannot carry initial workbench deltas.
    // handleError 拒绝无法承载初始工作台增量的握手。
    const handleError = () => {
      cleanup();
      reject(new Error("Browser service event WebSocket connection failed."));
    };
    // handleClose rejects a connection that closed before becoming usable.
    // handleClose 拒绝在可用前已经关闭的连接。
    const handleClose = () => {
      cleanup();
      reject(new Error("Browser service event WebSocket closed before opening."));
    };
    socket.addEventListener("open", handleOpen, { once: true });
    socket.addEventListener("error", handleError, { once: true });
    socket.addEventListener("close", handleClose, { once: true });
  });
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
      if (!payload.error) {
        throw new BrowserServiceRpcError("invalid_rpc_response", "Browser service RPC failure is missing its error envelope.");
      }
      throw new BrowserServiceRpcError(
        payload.error.code,
        renderServiceErrorMessage(payload.error.message, payload.error.localized),
      );
    }
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

  // subscribe registers a topic handler and resolves only after the event WebSocket is open.
  // subscribe 注册主题处理函数，并且仅在事件 WebSocket 已连接后解析。
  async subscribe<T>(topic: string, handler: ServiceEventHandler<T>): Promise<ServiceUnsubscribe> {
    const typedHandler = handler as ServiceEventHandler<unknown>;
    const topicHandlers = this.handlers.get(topic) ?? new Set<ServiceEventHandler<unknown>>();
    topicHandlers.add(typedHandler);
    this.handlers.set(topic, topicHandlers);
    this.disposed = false;
    // unsubscribe owns this exact handler and closes an otherwise unused event channel.
    // unsubscribe 持有当前精确处理器，并关闭不再被使用的事件通道。
    const unsubscribe = () => {
      const currentHandlers = this.handlers.get(topic);
      currentHandlers?.delete(typedHandler);
      if (currentHandlers && currentHandlers.size === 0) {
        this.handlers.delete(topic);
      }
      if (this.handlerCount() === 0) {
        this.clearReconnectTimer();
        this.closeSocketIfUnused();
      }
    };
    try {
      const socket = await this.ensureSocket();
      if (!socket) {
        throw new Error("Browser service event WebSocket was not created.");
      }
      await waitForWebSocketOpen(socket);
      return unsubscribe;
    } catch (error) {
      unsubscribe();
      throw error;
    }
  }

  // dispose closes the WebSocket and clears all handlers.
  // dispose 关闭 WebSocket 并清理所有处理函数。
  dispose(): void {
    this.disposed = true;
    this.handlers.clear();
    this.clearReconnectTimer();
    this.closeSocketIfUnused();
  }

  // ensureSocket creates or returns the single event WebSocket without claiming it is already open.
  // ensureSocket 创建或返回唯一事件 WebSocket，但不会声称连接已经打开。
  private async ensureSocket(): Promise<WebSocket | null> {
    if (this.disposed || this.handlerCount() === 0) {
      return null;
    }
    if (this.socket) {
      return this.socket;
    }
    this.clearReconnectTimer();
    let baseUrl: string;
    try {
      baseUrl = await this.resolveBaseUrl();
    } catch (error) {
      // Only the event-channel owner reports a failed connection attempt, including discovery failures.
      // 只有事件通道归属方报告连接尝试失败，包括发现失败。
      emitBrowserServiceConnectionStatus("disconnected", error instanceof Error ? error.message : String(error));
      this.scheduleReconnect();
      throw error;
    }
    if (this.disposed || this.handlerCount() === 0) {
      return null;
    }
    if (this.socket) {
      return this.socket;
    }
    const socket = new WebSocket(webSocketUrlFromBaseUrl(baseUrl));
    socket.addEventListener("message", (event) => {
      if (this.socket !== socket) return;
      this.dispatchSocketMessage(event.data);
    });
    socket.addEventListener("open", () => {
      if (this.socket !== socket) return;
      this.reconnectDelayMs = 500;
      // Only an open event socket closes the delivery gap; HTTP success cannot prove this.
      // 只有事件套接字打开才能闭合推送缺口；HTTP 成功无法证明这一点。
      emitBrowserServiceConnectionStatus("connected");
    });
    socket.addEventListener("close", () => {
      // A superseded socket cannot clear the newer socket's endpoint or reconnect state.
      // 已被替换的旧套接字不能清除新套接字的端点或重连状态。
      if (this.socket !== socket) return;
      this.socket = null;
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
      // A failed handshake cannot remain the reusable socket for the next bootstrap attempt.
      // 失败的握手不能继续充当下一次启动尝试复用的套接字。
      if (this.socket !== socket) return;
      this.socket = null;
      emitBrowserServiceConnectionStatus("disconnected", "Browser service connection failed.");
      this.scheduleReconnect();
    });
    this.socket = socket;
    return socket;
  }

  // closeSocketIfUnused closes the current socket only after its ownership state is revalidated.
  // closeSocketIfUnused 仅在重新校验当前套接字所有权状态后关闭连接。
  //
  // Returns
  // 返回值
  // This method returns after requesting an immediate or deferred close when no subscriber remains.
  // 当没有订阅者时，该方法在请求立即关闭或延迟关闭后返回。
  private closeSocketIfUnused(): void {
    const socket = this.socket;
    if (!socket) {
      return;
    }
    closeWebSocketWhenReady(
      socket,
      () => this.socket === socket && (this.disposed || this.handlerCount() === 0),
    );
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
      // Background reconnect failures are already projected by discovery and the next timer.
      // 后台重连失败已由发现流程与下一次定时器投影。
      void this.ensureSocket().catch(() => undefined);
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
      return this.baseUrl;
    } catch (error) {
      this.baseUrl = null;
      this.baseUrlPromise = null;
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
