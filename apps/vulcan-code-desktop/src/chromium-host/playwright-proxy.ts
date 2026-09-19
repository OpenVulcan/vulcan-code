// One-use Core-granted Playwright endpoint backed by a private named pipe.
// 由 Core 一次性授权、并由私有命名管道承载的 Playwright 端点。
import { randomBytes, timingSafeEqual } from 'node:crypto';
import { createServer, type IncomingMessage, type Server } from 'node:http';
import net, { type Socket } from 'node:net';
import type { Duplex } from 'node:stream';
import type { Browser } from 'playwright-core';
import WebSocket, { WebSocketServer, type RawData } from 'ws';

/** Maximum Playwright protocol message accepted by the authenticated relay. */
/** 经过鉴权中继允许的 Playwright 协议消息最大字节数。 */
const MAX_PLAYWRIGHT_MESSAGE_BYTES = 16 * 1024 * 1024;

/** Lifetime of an unused Core grant before it is revoked. */
/** 未使用的 Core 授权在自动撤销前的有效时长。 */
const PLAYWRIGHT_GRANT_TTL_MS = 30_000;

/** Maximum time spent connecting the relay to the private named pipe. */
/** 中继连接私有命名管道允许消耗的最长时间。 */
const PIPE_CONNECT_TIMEOUT_MS = 5_000;

/** Exact grant returned to Core for one independent MCP child process. */
/** 返回给 Core、供一个独立 MCP 子进程使用的精确授权。 */
export interface PlaywrightProxyGrant {
  /** Loopback WebSocket URL containing the one-use bearer capability. */
  /** 包含一次性承载能力的回环 WebSocket 地址。 */
  endpoint: string;
  /** Expiration time expressed as Unix milliseconds. */
  /** 使用 Unix 毫秒表示的过期时间。 */
  expiresAt: string;
}

/** Playwright protocol request identifier tracked during revocation. */
/** 撤销期间跟踪的 Playwright 协议请求标识。 */
export type PlaywrightRequestId = number | string;

/** Explicit result of revoking a Core-owned Playwright relay. */
/** 撤销 Core 所有 Playwright 中继后的明确结果。 */
export interface PlaywrightProxyCloseResult {
  /** Whether the exact Core connection was revoked. */
  /** 指定的 Core 连接是否已撤销。 */
  revoked: boolean;
  /** Whether forwarded Playwright requests were still awaiting responses. */
  /** 已转发的 Playwright 请求是否仍在等待响应。 */
  status: 'revoked' | 'inFlight' | 'unknown';
  /** Request identifiers forwarded without an observed response. */
  /** 已转发但撤销前没有观察到响应的请求标识。 */
  inFlightRequestIds: PlaywrightRequestId[];
}

/** One WebSocket-to-named-pipe relay and its pending protocol requests. */
/** 一个 WebSocket 到命名管道的中继及其待处理协议请求。 */
interface Relay {
  /** MCP client's WebSocket. */
  /** MCP 客户端 WebSocket。 */
  client: WebSocket;
  /** Playwright BrowserServer named-pipe socket. */
  /** Playwright BrowserServer 命名管道套接字。 */
  upstream: Socket;
  /** Bytes received from the pipe that do not yet end at a NUL delimiter. */
  /** 从管道收到但尚未遇到 NUL 分隔符的字节。 */
  pending: Buffer;
  /** Requests sent upstream and awaiting a matching response. */
  /** 已发送到上游并等待匹配响应的请求。 */
  inFlight: Map<string, PlaywrightRequestId>;
  /** Protocol method names retained only for bounded uncertainty diagnostics. */
  /** 仅为有界不确定性诊断保留的协议方法名。 */
  inFlightMethods: Map<string, string>;
  /** Prevents duplicate relay teardown from racing. */
  /** 防止中继重复关闭产生竞态。 */
  stopped: boolean;
}

/** Secure loopback relay from one Core grant to a private Playwright pipe. */
/** 从单个 Core 授权到私有 Playwright 管道的安全回环中继。 */
export class PlaywrightProxy {
  /** HTTP listener used only for the short-lived authenticated WebSocket upgrade. */
  /** 仅用于短时鉴权 WebSocket 升级的 HTTP 监听器。 */
  private server: Server | null = null;
  /** WebSocket upgrade helper with no public HTTP routing of its own. */
  /** 不自行公开 HTTP 路由的 WebSocket 升级辅助器。 */
  private webSocketServer: WebSocketServer | null = null;
  /** BrowserServer named-pipe endpoint kept inside this host process. */
  /** 仅保存在宿主进程内的 BrowserServer 命名管道端点。 */
  private browserPipe: string | null = null;
  /** Exact Core connection owning the current grant. */
  /** 当前授权所属的精确 Core 连接。 */
  private connectionId: string | null = null;
  /** Reserves the gateway while asynchronous pipe binding is still in progress. */
  /** 异步绑定管道期间预留网关，阻止并发授予第二个连接。 */
  private opening = false;
  /** Resolves only after an in-progress grant has published or cleaned up its binding. */
  /** 仅在进行中的授权发布或清理绑定后完成。 */
  private openingFinished: Promise<void> = Promise.resolve();
  /** Prevents a late grant from being published after host shutdown begins. */
  /** 阻止宿主开始关闭后发布迟到的授权。 */
  private shuttingDown = false;
  /** One-use path capability encoded as bytes for constant-time comparison. */
  /** 用于恒定时间比较的一次性路径能力字节。 */
  private capability: Buffer | null = null;
  /** True after the one-use WebSocket upgrade has been consumed. */
  /** 一次性 WebSocket 升级被消费后为真。 */
  private consumed = false;
  /** Expiration deadline for the current unused capability. */
  /** 当前未使用能力的过期截止时间。 */
  private expiresAt = 0;
  /** Timer that revokes an unused grant. */
  /** 自动撤销未使用授权的计时器。 */
  private expiryTimer: ReturnType<typeof setTimeout> | null = null;
  /** Active relays whose sockets must be terminated on revocation. */
  /** 撤销时必须终止套接字的活动中继。 */
  private readonly relays = new Set<Relay>();
  /** Forwarded requests whose outcome became unknowable when a relay disconnected. */
  /** 中继断线后结果无法确认的已转发请求。 */
  private readonly unconfirmedRequests = new Map<string, PlaywrightRequestId>();
  /** Serializes unbind with the next bind when Core reopens automation. */
  /** Core 重新打开自动化时串行化解绑与下一次绑定。 */
  private unbindPromise: Promise<void> = Promise.resolve();
  /** Keeps an unsuccessful BrowserServer unbind from becoming a confirmed revocation. */
  /** 防止 BrowserServer 解绑失败被误报为已确认撤销。 */
  private unbindFailed = false;

  /** Creates a relay around the exact browser owned by this host. */
  /** 为本宿主精确拥有的浏览器创建中继。 */
  constructor(private readonly browser: Browser) {}

  /** Opens one expiring endpoint and starts a private BrowserServer pipe. */
  /** 打开一个会过期的端点，并启动私有 BrowserServer 管道。 */
  async open(connectionId: string): Promise<PlaywrightProxyGrant> {
    if (this.shuttingDown) throw new Error('Playwright relay is shutting down.');
    if (!connectionId.trim()) throw new Error('Automation connection ID is required.');
    if (this.opening || this.server || this.connectionId)
      throw new Error('A Playwright relay is already active for this session.');
    if (this.unbindFailed)
      throw new Error('A previous Playwright BrowserServer unbind failed; close the browser context before reopening automation.');
    if (this.unconfirmedRequests.size > 0)
      throw new Error('A previous Playwright request remains unconfirmed; close the browser context before reopening automation.');
    this.opening = true;
    let finishOpening!: () => void;
    this.openingFinished = new Promise<void>(resolve => { finishOpening = resolve; });
    try {
      await this.unbindPromise;
      if (this.shuttingDown) throw new Error('Playwright relay is shutting down.');
      if (this.unbindFailed)
        throw new Error('A previous Playwright BrowserServer unbind failed; close the browser context before reopening automation.');
      if (this.unconfirmedRequests.size > 0)
        throw new Error('A previous Playwright request remains unconfirmed; close the browser context before reopening automation.');
      const binding = await this.browser.bind('vulcan-core-browser');
      this.browserPipe = binding.endpoint;
      if (this.shuttingDown) throw new Error('Playwright relay is shutting down.');
      const capability = randomBytes(32).toString('base64url');
      const capabilityPath = `/devtools/browser/${capability}`;
      this.capability = Buffer.from(capabilityPath, 'utf8');
      this.connectionId = connectionId;
      this.consumed = false;
      this.expiresAt = Date.now() + PLAYWRIGHT_GRANT_TTL_MS;
      const server = createServer((_request, response) => {
        // BrowserServer's own `/json` route is never exposed by this relay.
        // 此中继绝不暴露 BrowserServer 自带的 `/json` 路由。
        response.writeHead(404, { 'cache-control': 'no-store' }).end();
      });
      this.server = server;
      const webSocketServer = new WebSocketServer({
        noServer: true,
        maxPayload: MAX_PLAYWRIGHT_MESSAGE_BYTES,
        perMessageDeflate: false,
      });
      this.webSocketServer = webSocketServer;
      server.on('upgrade', (request, socket, head) => {
        void this.upgrade(webSocketServer, server, request, socket, head).catch(() => {
          this.diagnostic('Playwright relay upgrade failed; socket closed.');
          socket.destroy();
        });
      });
      await new Promise<void>((resolve, reject) => {
        server.once('error', reject);
        server.listen(0, '127.0.0.1', () => {
          server.off('error', reject);
          resolve();
        });
      });
      const address = server.address();
      if (!address || typeof address === 'string' || address.port <= 0)
        throw new Error('Playwright relay did not report a loopback port.');
      if (this.shuttingDown) throw new Error('Playwright relay is shutting down.');
      this.expiryTimer = setTimeout(() => {
        if (this.connectionId === connectionId && !this.consumed) this.revokeCurrent();
      }, PLAYWRIGHT_GRANT_TTL_MS);
      return {
        endpoint: `ws://127.0.0.1:${address.port}${capabilityPath}`,
        expiresAt: String(this.expiresAt),
      };
    } catch (error) {
      // A failed grant after bind must not leave a private pipe or listener reusable.
      // 绑定后的授权失败不得留下可复用的私有管道或监听器。
      if (this.browserPipe || this.server || this.connectionId) {
        this.revokeCurrent();
        await this.unbindPromise;
      }
      throw error;
    } finally {
      this.opening = false;
      finishOpening();
    }
  }

  /** Revokes one exact connection and closes its gateway and pipe. */
  /** 撤销一个精确连接，并关闭其网关与管道。 */
  async close(connectionId: string): Promise<PlaywrightProxyCloseResult> {
    if (this.connectionId !== connectionId)
      return { revoked: false, status: 'unknown', inFlightRequestIds: [] };
    const inFlightRequestIds = [
      ...this.unconfirmedRequests.values(),
      ...[...this.relays].flatMap(relay => [...relay.inFlight.values()]),
    ];
    const result: PlaywrightProxyCloseResult = {
      revoked: true,
      status: inFlightRequestIds.length > 0 ? 'inFlight' : 'revoked',
      inFlightRequestIds,
    };
    this.revokeCurrent();
    await this.unbindPromise;
    if (this.unbindFailed) result.status = 'unknown';
    return result;
  }

  /** Closes every relay and waits for BrowserServer named-pipe shutdown. */
  /** 关闭所有中继，并等待 BrowserServer 命名管道停止。 */
  async shutdown(): Promise<void> {
    this.shuttingDown = true;
    await this.openingFinished;
    if (this.connectionId || this.server || this.browserPipe) this.revokeCurrent();
    await this.unbindPromise;
  }

  /** Rejects an unauthenticated upgrade without revealing the valid path. */
  /** 拒绝未鉴权升级，且不泄露有效路径。 */
  private rejectUpgrade(socket: Duplex, status: string): void {
    if (!socket.destroyed) socket.write(`${status}\r\nContent-Length: 0\r\n\r\n`);
    socket.destroy();
  }

  /** Authenticates one upgrade and connects it to the private named pipe. */
  /** 鉴权一次升级，并将其连接到私有命名管道。 */
  private async upgrade(
    webSocketServer: WebSocketServer,
    server: Server,
    request: IncomingMessage,
    socket: Duplex,
    head: Buffer,
  ): Promise<void> {
    let pathname: string;
    try {
      pathname = new URL(request.url ?? '/', 'http://127.0.0.1').pathname;
    } catch {
      this.rejectUpgrade(socket, 'HTTP/1.1 400 Bad Request');
      return;
    }
    const offered = Buffer.from(pathname, 'utf8');
    const expected = this.capability;
    if (!expected || this.server !== server || this.consumed || Date.now() >= this.expiresAt
        || request.headers.origin || offered.length !== expected.length
        || !timingSafeEqual(offered, expected)) {
      this.rejectUpgrade(socket, 'HTTP/1.1 403 Forbidden');
      return;
    }
    const pipeName = this.browserPipe;
    if (!pipeName) {
      this.rejectUpgrade(socket, 'HTTP/1.1 503 Service Unavailable');
      return;
    }
    // Consume before opening the pipe so two concurrent upgrades cannot share a grant.
    // 在打开管道前消费能力，防止两个并发升级共享同一个授权。
    this.consumed = true;
    // Expiry limits only an unused capability; an attached MCP socket lives until Core revokes it.
    // 到期限制只适用于未使用能力；已挂接的 MCP 套接字保持到 Core 主动撤销。
    if (this.expiryTimer) clearTimeout(this.expiryTimer);
    this.expiryTimer = null;
    const upstream = await this.connectPipe(pipeName).catch(error => {
      this.rejectUpgrade(socket, 'HTTP/1.1 503 Service Unavailable');
      throw error;
    });
    if (this.server !== server || socket.destroyed || !this.connectionId) {
      upstream.destroy();
      socket.destroy();
      return;
    }
    webSocketServer.handleUpgrade(request, socket, head, client => this.bridge(client, upstream));
  }

  /** Connects to the private pipe with a bounded timeout and no endpoint disclosure. */
  /** 在有界超时内连接私有管道，且不披露管道端点。 */
  private async connectPipe(pipeName: string): Promise<Socket> {
    const upstream = net.createConnection(pipeName);
    await new Promise<void>((resolve, reject) => {
      let settled = false;
      const finish = (error?: Error): void => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        upstream.off('connect', onConnect);
        upstream.off('error', onError);
        upstream.off('close', onClose);
        if (error) reject(error);
        else resolve();
      };
      const onConnect = (): void => finish();
      const onError = (error: Error): void => finish(error);
      const onClose = (): void => finish(new Error('Playwright BrowserServer pipe closed before connection.'));
      const timer = setTimeout(() => finish(new Error('Playwright BrowserServer pipe connection timed out.')), PIPE_CONNECT_TIMEOUT_MS);
      upstream.once('connect', onConnect);
      upstream.once('error', onError);
      upstream.once('close', onClose);
    }).catch(error => {
      upstream.destroy();
      throw error;
    });
    return upstream;
  }

  /** Bridges WebSocket text messages to NUL-delimited Playwright pipe messages. */
  /** 将 WebSocket 文本消息中继为 NUL 分隔的 Playwright 管道消息。 */
  private bridge(client: WebSocket, upstream: Socket): void {
    const relay: Relay = {
      client,
      upstream,
      pending: Buffer.alloc(0),
      inFlight: new Map(),
      inFlightMethods: new Map(),
      stopped: false,
    };
    this.relays.add(relay);
    const stop = (): void => {
      if (relay.stopped) return;
      relay.stopped = true;
      // Closing a socket is not evidence that Chrome stopped a forwarded request. Preserve the
      // uncertainty after this relay leaves the active set, including client-initiated closes.
      // 关闭套接字不能证明 Chrome 停止了已转发请求；即使客户端主动断线也要保留该不确定性。
      if (relay.inFlight.size > 0) {
        const methods = [...relay.inFlightMethods.values()].slice(0, 8).join(',');
        this.diagnostic(`Playwright relay disconnected with ${relay.inFlight.size} unanswered requests; methods=${methods}`);
      }
      for (const [key, id] of relay.inFlight) this.unconfirmedRequests.set(key, id);
      this.relays.delete(relay);
      if (client.readyState === WebSocket.OPEN || client.readyState === WebSocket.CONNECTING) client.terminate();
      upstream.destroy();
    };
    client.on('message', (data: RawData, isBinary: boolean) => {
      if (relay.stopped || isBinary || !upstream.writable || upstream.writableLength > MAX_PLAYWRIGHT_MESSAGE_BYTES)
        return stop();
      const body = rawDataBuffer(data);
      if (body.length === 0 || body.length > MAX_PLAYWRIGHT_MESSAGE_BYTES || body.includes(0)) return stop();
      let message: unknown;
      try { message = JSON.parse(body.toString('utf8')); } catch { return stop(); }
      if (!isRecord(message)) return stop();
      if (Object.prototype.hasOwnProperty.call(message, 'id')) {
        const id = protocolRequestId(message.id);
        if (id === null) return stop();
        // Playwright's dispatcher intentionally sends no response to these protocol messages.
        // Playwright 分发器刻意不回复这两类协议消息，不能把它们算作未完成页面操作。
        const oneWay = message.method === '__waitInfo__' || message.method === '__abort__';
        if (!oneWay) {
          if (relay.inFlight.has(protocolRequestKey(id))) return stop();
          relay.inFlight.set(protocolRequestKey(id), id);
          // A protocol method is untrusted text; keep diagnostics short and single-line.
          // 协议方法名属于不可信文本；诊断仅保留短且单行的安全名称。
          const diagnosticMethod = typeof message.method === 'string'
            && /^[A-Za-z_$][A-Za-z0-9_.$]{0,95}$/.test(message.method)
            ? message.method
            : `<non-request:${['guid', 'params', 'result', 'error']
              .filter(key => Object.prototype.hasOwnProperty.call(message, key)).join('|')}>`;
          relay.inFlightMethods.set(protocolRequestKey(id), diagnosticMethod);
        }
      }
      upstream.write(body);
      upstream.write('\0');
    });
    upstream.on('data', (chunk: Buffer) => {
      if (relay.stopped) return;
      relay.pending = relay.pending.length === 0 ? chunk : Buffer.concat([relay.pending, chunk]);
      if (relay.pending.length > MAX_PLAYWRIGHT_MESSAGE_BYTES * 2) return stop();
      let end = relay.pending.indexOf(0);
      while (end !== -1) {
        const body = relay.pending.subarray(0, end);
        relay.pending = relay.pending.subarray(end + 1);
        if (body.length === 0 || body.length > MAX_PLAYWRIGHT_MESSAGE_BYTES
            || client.readyState !== WebSocket.OPEN || client.bufferedAmount > MAX_PLAYWRIGHT_MESSAGE_BYTES) return stop();
        let message: unknown;
        try { message = JSON.parse(body.toString('utf8')); } catch { return stop(); }
        if (isRecord(message) && Object.prototype.hasOwnProperty.call(message, 'id')) {
          const id = protocolRequestId(message.id);
          if (id !== null) {
            relay.inFlight.delete(protocolRequestKey(id));
            relay.inFlightMethods.delete(protocolRequestKey(id));
          }
        }
        client.send(body.toString('utf8'));
        end = relay.pending.indexOf(0);
      }
    });
    client.on('close', stop);
    client.on('error', stop);
    upstream.on('close', stop);
    upstream.on('error', stop);
  }

  /** Revokes the current grant and schedules named-pipe shutdown exactly once. */
  /** 精确撤销当前授权，并只安排一次命名管道关闭。 */
  private revokeCurrent(): void {
    if (!this.connectionId && !this.server && !this.browserPipe) return;
    if (this.expiryTimer) clearTimeout(this.expiryTimer);
    this.expiryTimer = null;
    for (const relay of [...this.relays]) {
      for (const [key, id] of relay.inFlight) this.unconfirmedRequests.set(key, id);
      relay.client.terminate();
      relay.upstream.destroy();
    }
    this.relays.clear();
    const server = this.server;
    const webSocketServer = this.webSocketServer;
    this.server = null;
    this.webSocketServer = null;
    this.connectionId = null;
    this.capability = null;
    this.consumed = false;
    this.expiresAt = 0;
    this.browserPipe = null;
    try { server?.close(); } catch { /* The listener may already have closed with its pipe. */ }
    try { webSocketServer?.close(); } catch { /* NoServer WebSocket server may have no clients. */ }
    this.unbindPromise = this.unbindPromise.then(() => this.browser.unbind()).catch(() => {
      this.unbindFailed = true;
      this.diagnostic('Playwright BrowserServer unbind failed; browser context closure is required.');
    });
  }

  /** Writes a bounded diagnostic without any endpoint, path, or pipe secret. */
  /** 写入不包含端点、路径或管道秘密的有界诊断。 */
  private diagnostic(message: string): void {
    process.stderr.write(`[vulcan-playwright-relay] ${message}\n`);
  }
}

/** Converts every ws raw message representation into one bounded byte buffer. */
/** 将 ws 的各种原始消息表示转换为一个有界字节缓冲区。 */
function rawDataBuffer(data: RawData): Buffer {
  if (Buffer.isBuffer(data)) return data;
  if (Array.isArray(data)) return Buffer.concat(data);
  return Buffer.from(data);
}

/** Checks that a decoded protocol value is a JSON object. */
/** 检查解码后的协议值是否为 JSON 对象。 */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Returns a finite protocol request identifier, excluding null and booleans. */
/** 返回有限的协议请求标识，并排除 null 与布尔值。 */
function protocolRequestId(value: unknown): PlaywrightRequestId | null {
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  return typeof value === 'string' && value.length > 0 ? value : null;
}

/** Preserves numeric and string request identifiers in the pending map. */
/** 在待处理映射中保留数字与字符串请求标识的类型。 */
function protocolRequestKey(value: PlaywrightRequestId): string {
  return `${typeof value}:${String(value)}`;
}
