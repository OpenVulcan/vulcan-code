import { serviceClient } from "@/service/webui-service-runtime";
import {
  resolveBrowserServiceAccessToken,
  resolveBrowserServiceBaseUrl,
} from "@/service/browser-service-discovery";

// CoreBrowserSession is the browser state published by the single owning Core.
// CoreBrowserSession 是唯一归属 Core 发布的浏览器状态。
export interface CoreBrowserSession {
  sessionId: string;
  // Immutable VulcanCode conversation that owns this Core browser.
  // 拥有此 Core 浏览器的不可变 VulcanCode 会话。
  ownerSessionId: string;
  status: "running" | "crashed" | "closed";
  profileMode: "ephemeral" | "persistent";
  currentUrl: string | null;
  leaseState: "free" | "held" | "revoking" | "draining" | "blocked";
  blockedReason: string | null;
  automationState: "detached" | "attached";
  pageGeneration: string;
  viewportGeneration: string;
  viewportSize: { width: number; height: number } | null;
  controlEpoch: string;
  hostGeneration: string;
  pageCount: number;
  requiresExplicitReopen: boolean;
}

// CoreBrowserRuntimeStatus distinguishes the optional Core browser package from plugin dependencies.
// CoreBrowserRuntimeStatus 区分可选 Core 浏览器组件与插件运行依赖。
export interface CoreBrowserRuntimeStatus {
  ready: boolean;
  // Recovery capability and diagnostic are reported by the owning Core host.
  // 恢复能力及诊断由所属 Core 宿主报告。
  canInstall: boolean;
  diagnostic: string | null;
  // Installation remains queryable after the initiating view closes.
  // 发起界面关闭后，安装仍可查询。
  installation: CoreBrowserInstallTask | null;
}

// CoreBrowserInstallTask identifies one deduplicated background installation attempt.
// CoreBrowserInstallTask 标识去重后的后台安装尝试。
export interface CoreBrowserInstallTask {
  taskId: string;
  state: "running" | "succeeded" | "failed";
  error: string | null;
}

// installCoreBrowserRuntime starts explicit recovery; returns the Core-owned task immediately.
// installCoreBrowserRuntime 启动显式恢复；立即返回 Core 持有的任务。
export function installCoreBrowserRuntime(): Promise<CoreBrowserInstallTask> {
  return serviceClient.request<CoreBrowserInstallTask>("browser.runtime.install", {});
}

// getCoreBrowserRuntimeStatus reads Core-wide browser component readiness without a conversation.
// getCoreBrowserRuntimeStatus 在没有选中会话时读取 Core 全局浏览器组件就绪状态。
export function getCoreBrowserRuntimeStatus(): Promise<CoreBrowserRuntimeStatus> {
  return serviceClient.request<CoreBrowserRuntimeStatus>("browser.runtime.status", {});
}

// CoreBrowserPage is the stable Core identity of one real host target.
// CoreBrowserPage 是真实宿主目标的稳定 Core 身份。
export interface CoreBrowserPage {
  pageId: string;
  targetId: string;
  url: string;
  // A title is absent until Chromium reports metadata (including an older running Core during update).
  // Chromium 报告元数据前标题为空；更新期间仍在运行的旧 Core 也可能尚未提供标题。
  title?: string | null;
  openerPageId: string | null;
  hostGeneration: number;
}

// CoreBrowserLease keeps its secret in memory only while this UI owns control.
// CoreBrowserLease 仅在本界面持有控制权时于内存中保存凭据。
export interface CoreBrowserLease {
  session: CoreBrowserSession;
  leaseSecret: string;
  heartbeatIntervalMs: number;
}

// CoreBrowserDownload is path-free metadata for a file completed by the owning browser host.
// CoreBrowserDownload 是归属浏览器宿主完成的文件的不含路径元数据。
export interface CoreBrowserDownload {
  downloadId: string;
  sessionId: string;
  pageId: string;
  fileName: string;
  status: "pending" | "completed" | "failed";
  byteLength: number | null;
  sha256: string | null;
}

// CoreBrowserDialog is a pending JavaScript dialog on one Core-owned page.
// CoreBrowserDialog 是一个 Core 自有页面上待处理的 JavaScript 对话框。
export interface CoreBrowserDialog {
  dialogId: string;
  sessionId: string;
  pageId: string;
  dialogType: "alert" | "confirm" | "prompt" | "beforeunload";
  message: string;
  defaultPrompt: string;
}

// CoreBrowserFileChooser names one intercepted page input without exposing its host DOM identity.
// CoreBrowserFileChooser 标识一个已拦截的页面输入，而不暴露其宿主 DOM 身份。
export interface CoreBrowserFileChooser {
  chooserId: string;
  sessionId: string;
  pageId: string;
  mode: "selectSingle" | "selectMultiple";
}

// listCoreBrowserFileChoosers reads the selected session's pending file input requests.
// listCoreBrowserFileChoosers 读取选定会话待处理的文件输入请求。
export function listCoreBrowserFileChoosers(ownerSessionId: string, sessionId: string): Promise<CoreBrowserFileChooser[]> {
  return requestCoreBrowser(ownerSessionId, "browser.fileChooser.list", { sessionId });
}

// listCoreBrowserDialogs lets observers see pending dialogs without answering them.
// listCoreBrowserDialogs 使观察者看到待处理对话框，但不授予其答复权。
export function listCoreBrowserDialogs(ownerSessionId: string, sessionId: string): Promise<CoreBrowserDialog[]> {
  return requestCoreBrowser(ownerSessionId, "browser.dialog.list", { sessionId });
}

// listCoreBrowserDownloads reads the selected session's Core-owned download records.
// listCoreBrowserDownloads 读取选定会话中由 Core 持有的下载记录。
export function listCoreBrowserDownloads(ownerSessionId: string, sessionId: string): Promise<CoreBrowserDownload[]> {
  return requestCoreBrowser(ownerSessionId, "browser.download.list", { sessionId });
}

// fetchCoreBrowserDownload reads a completed file through the authenticated Core route.
// fetchCoreBrowserDownload 通过已鉴权的 Core 路由读取完成的文件。
export async function fetchCoreBrowserDownload(ownerSessionId: string, downloadId: string): Promise<Blob> {
  const token = resolveBrowserServiceAccessToken();
  if (!token) {
    throw new Error("Browser service authentication is unavailable.");
  }
  const url = new URL(`/v1/browser/downloads/${encodeURIComponent(downloadId)}/content`, resolveBrowserServiceBaseUrl());
  const response = await fetch(url, {
    headers: { "x-vulcan-service-token": token, "x-vulcan-browser-owner-session-id": ownerSessionId },
    cache: "no-store",
  });
  if (!response.ok) {
    throw new Error(`Browser download failed: HTTP ${response.status}`);
  }
  return response.blob();
}

// CoreBrowserFrame combines a verified binary frame header with its image bytes.
// CoreBrowserFrame 将已验证的二进制帧头与图像字节组合。
export interface CoreBrowserFrame {
  sequence: string;
  pageId: string;
  pageGeneration: string;
  viewportGeneration: string;
  width: number;
  height: number;
  capturedAt: string;
  mimeType: string;
  image: Uint8Array<ArrayBuffer>;
}

// CoreBrowserStreamCallbacks reports stream state and frames to one observer component.
// CoreBrowserStreamCallbacks 向一个观察组件报告流状态与画面。
export interface CoreBrowserStreamCallbacks {
  onFrame: (frame: CoreBrowserFrame) => void;
  onState: (status: string) => void;
  onError: (message: string) => void;
}

// listCoreBrowserSessions reads browser sessions owned by the selected conversation.
// listCoreBrowserSessions 读取所选 VulcanCode 会话拥有的浏览器会话。
export function listCoreBrowserSessions(ownerSessionId: string): Promise<CoreBrowserSession[]> {
  return requestCoreBrowser(ownerSessionId, "browser.session.list");
}

// listCoreBrowserPages reads the stable pages of one session.
// listCoreBrowserPages 读取一个会话的稳定页面。
export function listCoreBrowserPages(ownerSessionId: string, sessionId: string): Promise<CoreBrowserPage[]> {
  return requestCoreBrowser(ownerSessionId, "browser.pages.list", { sessionId });
}

// requestCoreBrowser attaches the selected conversation to every browser RPC at one boundary.
// requestCoreBrowser 在单一边界为每个浏览器 RPC 附加选中的会话身份。
// Parameters: ownerSessionId selects the VulcanCode conversation, method the RPC, and params its typed fields.
// 参数：ownerSessionId 选择 VulcanCode 会话，method 是 RPC，params 是类型化字段。
// Returns: the Core-validated result for this conversation's browser resources only.
// 返回值：Core 校验后仅针对该会话浏览器资源的结果。
export function requestCoreBrowser<T>(ownerSessionId: string, method: string, params: Record<string, unknown> = {}): Promise<T> {
  return serviceClient.request<T>(method, { ...params, ownerSessionId });
}

// decodeCoreBrowserFrame separates one binary WebSocket message into metadata and image.
// decodeCoreBrowserFrame 将一条二进制 WebSocket 消息拆分为元数据与图像。
// Parameters: packet is the complete [u32 header length][JSON header][image] message.
// 参数：packet 是完整的 [u32 头长度][JSON 头][图像] 消息。
// Returns: the validated frame or throws for a malformed message.
// 返回值：已验证的帧；消息格式错误时抛出异常。
export function decodeCoreBrowserFrame(packet: ArrayBuffer): CoreBrowserFrame {
  // bytes is the immutable packet view used for header and image bounds.
  // bytes 是用于头部与图像边界检查的不可变消息视图。
  const bytes = new Uint8Array(packet);
  if (bytes.byteLength < 5) {
    throw new Error("Browser frame is shorter than its header prefix.");
  }
  // headerLength is the network-order JSON header size.
  // headerLength 是网络字节序的 JSON 头尺寸。
  const headerLength = new DataView(packet).getUint32(0, false);
  if (headerLength === 0 || headerLength > bytes.byteLength - 4) {
    throw new Error("Browser frame header length is invalid.");
  }
  // metadata is the host-published frame identity and viewport.
  // metadata 是宿主发布的帧身份与视口信息。
  const metadata = JSON.parse(new TextDecoder().decode(bytes.subarray(4, 4 + headerLength))) as Record<string, unknown>;
  // image is copied out so the rendered Blob owns stable bytes.
  // image 被复制出来，使渲染的 Blob 拥有稳定字节。
  const image = Uint8Array.from(bytes.subarray(4 + headerLength));
  if (metadata.kind !== "frame"
    || typeof metadata.sequence !== "string"
    || typeof metadata.pageId !== "string"
    || typeof metadata.pageGeneration !== "string"
    || typeof metadata.viewportGeneration !== "string"
    || typeof metadata.width !== "number"
    || typeof metadata.height !== "number"
    || typeof metadata.capturedAt !== "string"
    || (metadata.mimeType !== "image/jpeg" && metadata.mimeType !== "image/png")
    || image.byteLength === 0) {
    throw new Error("Browser frame metadata is invalid.");
  }
  return {
    sequence: metadata.sequence,
    pageId: metadata.pageId,
    pageGeneration: metadata.pageGeneration,
    viewportGeneration: metadata.viewportGeneration,
    width: metadata.width,
    height: metadata.height,
    capturedAt: metadata.capturedAt,
    mimeType: metadata.mimeType,
    image,
  };
}

// openCoreBrowserStream obtains a one-use ticket and observes one chosen page.
// openCoreBrowserStream 获取一次性凭据并观察一个选定页面。
// Parameters: sessionId and pageId are stable Core identities; callbacks receive stream updates.
// 参数：sessionId 与 pageId 是稳定 Core 身份；callbacks 接收流更新。
// Returns: a close function that releases the exact WebSocket subscription.
// 返回值：关闭并释放此 WebSocket 订阅的函数。
export async function openCoreBrowserStream(
  ownerSessionId: string,
  sessionId: string,
  pageId: string,
  callbacks: CoreBrowserStreamCallbacks,
): Promise<() => void> {
  // ticket authorizes one connection to the named Core session.
  // ticket 授权一条连接访问指定 Core 会话。
  const ticket = await requestCoreBrowser<{ ticket: string }>(ownerSessionId, "browser.stream.ticket", { sessionId });
  // serviceUrl is the same selected Core endpoint used by normal WebUI RPC.
  // serviceUrl 是普通 WebUI RPC 使用的同一个已选 Core 端点。
  const serviceUrl = resolveBrowserServiceBaseUrl();
  // url carries only the one-use ticket and the service instance token.
  // url 只携带一次性流凭据与服务实例令牌。
  const url = new URL(`${serviceUrl}/v1/browser/stream`);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.searchParams.set("ticket", ticket.ticket);
  url.searchParams.set("pageId", pageId);
  url.searchParams.set("subscriptionId", crypto.randomUUID());
  // token is resolved from the already-selected service instance, never from an arbitrary port.
  // token 从已选择的服务实例解析，绝不从任意端口取得。
  const token = resolveBrowserServiceAccessToken();
  if (token) {
    url.searchParams.set("token", token);
  }
  // socket owns the lifetime of this one page observation.
  // socket 拥有这一页面观察的生命周期。
  const socket = new WebSocket(url);
  socket.binaryType = "arraybuffer";
  socket.addEventListener("open", () => callbacks.onState("connected"));
  socket.addEventListener("message", (event) => {
    try {
      if (typeof event.data === "string") {
        // update contains snapshot/resume state and any stream refusal.
        // update 包含快照或恢复状态以及任何流拒绝。
        const update = JSON.parse(event.data) as { kind?: string; mode?: string; message?: string };
        if (update.kind === "error") {
          callbacks.onError(update.message ?? "Browser stream failed.");
        } else {
          // Snapshot/resume are delivery modes on an already connected socket, not connection states.
          // snapshot/resume 是已连接套接字上的传送模式，不是连接状态。
          callbacks.onState("connected");
        }
      } else if (event.data instanceof ArrayBuffer) {
        callbacks.onFrame(decodeCoreBrowserFrame(event.data));
      }
    } catch (error) {
      callbacks.onError(error instanceof Error ? error.message : String(error));
    }
  });
  socket.addEventListener("error", () => callbacks.onError("Browser stream connection failed."));
  socket.addEventListener("close", () => callbacks.onState("disconnected"));
  return () => {
    if (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING) {
      socket.close();
    }
  };
}
