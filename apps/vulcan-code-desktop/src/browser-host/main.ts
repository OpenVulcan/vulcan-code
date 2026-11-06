// Independent Core browser host: one session, one process, one context.
// 独立 Core 浏览器宿主：一个会话、一个进程、一个上下文。
//
// The host is a bounded subprocess owned by the Core browser manager. It writes exactly one
// endpoint frame to standard output, writes diagnostics to standard error, and carries the
// duplex control protocol on one authenticated loopback connection. It reports its local CDP
// endpoint through its own user-data directory so no code scans unrelated machine ports.
// 宿主是由 Core 浏览器管理器拥有的受限子进程。它向标准输出只写一条端点帧，向标准错误写诊断，
// 并在一条经过鉴权的回环连接上承载双工控制协议。它通过自有用户数据目录报告本地 CDP 端点，
// 使任何代码都不扫描机器上的无关端口。

import { app, BrowserWindow, session } from 'electron';
import process from 'node:process';
import type net from 'node:net';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { mkdir } from 'node:fs/promises';

import {
  BROWSER_HOST_MAX_DOWNLOAD_BYTES,
  BROWSER_HOST_MAX_IMAGE_BYTES,
  BROWSER_HOST_MAX_PAGES,
  BROWSER_HOST_MAX_VIEWPORT_DIMENSION,
  BROWSER_HOST_PROTOCOL_VERSION,
  ControlFrameDecoder,
  encodeControlFrame,
  type BrowserHostCoreHello,
  type BrowserHostEvent,
  type BrowserHostHandshake,
  type BrowserHostRequest,
  type BrowserHostResponse,
} from './host-protocol.js';
import { waitForCdpEndpoint } from './cdp-discovery.js';
import {
  BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION,
  writeControlDiscovery,
} from './control-discovery.js';
import {
  BROWSER_HOST_CONTROL_ACCEPT_TIMEOUT_MS,
  listenForCoreControl,
} from './control-server.js';
import { BROWSER_HOST_RUNTIME_VERSION } from './runtime-version.js';

/** Capabilities this host build actually implements and therefore reports. */
/** 本宿主构建实际实现并因此如实报告的能力。 */
const BROWSER_HOST_CAPABILITIES: string[] = [
  'cdp-endpoint',
  'page-window',
  'loopback-control',
  'page.create',
  'page.navigate',
  'page.list',
  'page.events',
  'viewport.set',
  'window.hide',
  'download.capture',
  'dialog.control',
  'file-chooser.intercept',
];

/** Bounded wait for the runtime debugging endpoint to appear. */
/** 等待运行时调试端点出现的有限时长。 */
const BROWSER_HOST_CDP_DISCOVERY_TIMEOUT_MS = 15_000;

/** Bound on the graceful quit that lets Chromium finish writing profile state. */
/** 允许 Chromium 完成画像状态写入的优雅退出时限。 */
const BROWSER_HOST_GRACEFUL_QUIT_TIMEOUT_MS = 4_000;

/** Parsed command-line inputs of one browser-host process. */
/** 单个浏览器宿主进程解析出的命令行输入。 */
interface BrowserHostOptions {
  /** Session identity assigned by Core. */
  /** 由 Core 分配的会话标识。 */
  sessionId: string;
  /** Host generation assigned by Core. */
  /** 由 Core 分配的宿主代际。 */
  hostGeneration: string;
  /** Explicit user-data directory for this host only. */
  /** 仅属于本宿主的明确用户数据目录。 */
  userDataDirectory: string;
  /**
   * Directory where this attempt publishes its control endpoint.
   * 本次尝试发布其控制端点的目录。
   */
  controlDirectory: string;
  /** Requested debugging port; zero selects an ephemeral port. */
  /** 请求的调试端口；零表示选择临时端口。 */
  debuggingPort: number;
  /** Initial page URL, or about:blank when absent. */
  /** 初始页面地址，缺省时为 about:blank。 */
  initialUrl: string | null;
  /** One-time secret Core must present on the control connection. */
  /** Core 必须在控制连接上出示的一次性密钥。 */
  connectionSecret: string;
}

/** Reads one required value from explicit `--name=value` arguments. */
/** 从明确的 `--name=value` 参数读取一个必需值。 */
function readRequiredArgument(argv: string[], name: string): string {
  const prefix = `--${name}=`;
  const match = argv.find((argument) => argument.startsWith(prefix));
  if (!match) {
    throw new Error(`Browser host requires the explicit ${prefix}<value> argument.`);
  }
  const value = match.slice(prefix.length);
  if (value.trim().length === 0) {
    throw new Error(`Browser host argument ${prefix} must not be empty.`);
  }
  return value;
}

/** Reads one optional value from explicit `--name=value` arguments. */
/** 从明确的 `--name=value` 参数读取一个可选值。 */
function readOptionalArgument(argv: string[], name: string): string | null {
  const prefix = `--${name}=`;
  const match = argv.find((argument) => argument.startsWith(prefix));
  if (!match) {
    return null;
  }
  const value = match.slice(prefix.length);
  return value.trim().length === 0 ? null : value;
}

/** Parses browser-host arguments before any window is created. */
/** 在创建任何窗口之前解析浏览器宿主参数。 */
function parseBrowserHostOptions(argv: string[]): BrowserHostOptions {
  const portText = readOptionalArgument(argv, 'cdp-port') ?? '0';
  if (!/^[0-9]+$/.test(portText)) {
    throw new Error('Browser host --cdp-port must be a decimal port number.');
  }
  const debuggingPort = Number.parseInt(portText, 10);
  if (debuggingPort < 0 || debuggingPort > 65535) {
    throw new Error('Browser host --cdp-port must be within 0..65535.');
  }
  return {
    sessionId: readRequiredArgument(argv, 'session-id'),
    hostGeneration: readRequiredArgument(argv, 'host-generation'),
    userDataDirectory: readRequiredArgument(argv, 'user-data-dir'),
    // Core passes the endpoint directory explicitly so this attempt's publication is per session
    // rather than inherited from a profile directory shared with earlier sessions.
    // Core 显式传入端点目录，使本次尝试的发布按会话隔离，而不是继承与早前会话共享的画像目录。
    controlDirectory: readOptionalArgument(argv, 'control-dir') ?? readRequiredArgument(argv, 'user-data-dir'),
    debuggingPort,
    initialUrl: readOptionalArgument(argv, 'initial-url'),
    connectionSecret: readRequiredArgument(argv, 'control-secret'),
  };
}

/** Writes one diagnostic line to standard error without any credential content. */
/** 向标准错误写入一行不含任何凭据内容的诊断。 */
function logDiagnostic(message: string): void {
  process.stderr.write(`[vulcan-browser-host] ${message}\n`);
}

/** Options resolved before Electron created any window. */
/** 在 Electron 创建任何窗口之前解析完成的选项。 */
let options: BrowserHostOptions;

/** Accepted control socket, or null before Core connects. */
/** 已接受的控制套接字；Core 连接前为空。 */
let controlSocket: net.Socket | null = null;

/** Monotonic event sequence owned by this host lifetime. */
/** 本宿主生命周期内单调的事件序号。 */
let eventSequence = 0;

/** True once a bounded control-channel shutdown has started. */
/** 受限控制通道关闭开始后为真。 */
let shuttingDown = false;

/** Exact remote-debugging port published by this host's own Chromium profile. */
/** 本宿主自身 Chromium 画像发布的精确远程调试端口。 */
let pageCdpPort = 0;

/** One page-level CDP transport used for dialog and file chooser events. */
/** 一个用于对话框与文件选择器事件的页面级 CDP 传输。 */
interface PageCdpLink {
  socket: WebSocket;
  nextId: number;
  pending: Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>;
}

/** Page transports keyed by their Electron webContents identity. */
/** 按 Electron webContents 身份索引的页面传输。 */
const pageCdpLinks = new Map<string, PageCdpLink>();

/** Sends one correlated command to the exact page CDP socket. */
/** 向精确页面 CDP 套接字发送一条可关联的命令。 */
function sendPageCdpCommand(targetId: string, method: string, params: Record<string, unknown>): Promise<unknown> {
  const link = pageCdpLinks.get(targetId);
  if (!link || link.socket.readyState !== WebSocket.OPEN) {
    return Promise.reject(new Error(`Page CDP transport is unavailable for target ${targetId}.`));
  }
  link.nextId += 1;
  const id = link.nextId;
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      link.pending.delete(id);
      reject(new Error(`Page CDP command ${method} timed out.`));
    }, 5000);
    link.pending.set(id, {
      resolve: (value) => { clearTimeout(timeout); resolve(value); },
      reject: (error) => { clearTimeout(timeout); reject(error); },
    });
    link.socket.send(JSON.stringify({ id, method, params }));
  });
}

/** Connects to this host's own page target and enables its event stream. */
/** 连接到本宿主自身的页面目标，并启用其事件流。 */
async function connectPageCdp(
  contents: Electron.WebContents,
  targetId: string,
  onEvent: (method: string, params: Record<string, unknown>) => void,
): Promise<void> {
  if (pageCdpPort === 0 || contents.isDestroyed() || pageCdpLinks.has(targetId)) return;
  const info = await contents.debugger.sendCommand('Target.getTargetInfo') as {
    targetInfo?: { targetId?: string };
  };
  const cdpTargetId = info.targetInfo?.targetId;
  if (!cdpTargetId) throw new Error(`Page ${targetId} has no CDP target identity.`);
  const socket = new WebSocket(`ws://127.0.0.1:${pageCdpPort}/devtools/page/${encodeURIComponent(cdpTargetId)}`);
  const link: PageCdpLink = { socket, nextId: 0, pending: new Map() };
  pageCdpLinks.set(targetId, link);
  socket.addEventListener('message', (event) => {
    let message: { id?: number; method?: string; params?: Record<string, unknown>; result?: unknown; error?: { message?: string } };
    try { message = JSON.parse(String(event.data)); } catch { return; }
    if (typeof message.id === 'number') {
      const pending = link.pending.get(message.id);
      if (!pending) return;
      link.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message ?? 'Page CDP command failed.'));
      else pending.resolve(message.result);
    } else if (typeof message.method === 'string') {
      onEvent(message.method, message.params ?? {});
    }
  });
  socket.addEventListener('close', () => {
    if (pageCdpLinks.get(targetId) !== link) return;
    pageCdpLinks.delete(targetId);
    for (const pending of link.pending.values()) pending.reject(new Error('Page CDP transport closed.'));
    link.pending.clear();
    if (!shuttingDown && !contents.isDestroyed()) {
      setTimeout(() => void connectPageCdp(contents, targetId, onEvent)
        .catch((error: unknown) => logDiagnostic(`page CDP reconnect failed: ${String(error)}`)), 500);
    }
  });
  socket.addEventListener('open', () => {
    void sendPageCdpCommand(targetId, 'Page.enable', {})
      .then(() => sendPageCdpCommand(targetId, 'Page.setInterceptFileChooserDialog', { enabled: true }))
      .then(() => logDiagnostic(`page CDP events enabled for target ${targetId}`))
      .catch((error: unknown) => logDiagnostic(`page CDP setup failed: ${String(error)}`));
  });
}

/** Writes one protocol frame to the accepted control connection. */
/** 向已接受的控制连接写入一个协议帧。 */
function writeControlFrame(value: unknown): void {
  if (!controlSocket || controlSocket.destroyed) {
    return;
  }
  controlSocket.write(encodeControlFrame(value));
}

/** Resolves the host target id of the page that opened one page, when it can be identified. */
/** 在可识别时解析打开某个页面的页面宿主目标标识。 */
function resolveOpenerTargetId(contents: Electron.WebContents): string | null {
  const opener = contents.opener;
  if (!opener) {
    return null;
  }
  // The opener is a frame, not a web contents, so the owning window is matched by frame identity
  // rather than by URL, which is never a page identity.
  // 打开者是一个框架而不是 web 内容，因此按框架身份匹配所属窗口，而绝不按地址匹配——地址从不是页面标识。
  const openerTop = opener.top ?? opener;
  for (const window of BrowserWindow.getAllWindows()) {
    if (window.webContents.mainFrame === openerTop) {
      return String(window.webContents.id);
    }
  }
  return null;
}

/** Registers page lifecycle reporting for every window-level web contents this host creates. */
/** 为本宿主创建的每个窗口级 web 内容注册页面生命周期上报。 */
function watchBrowserTargets(): void {
  app.on('web-contents-created', (_event, contents) => {
    // Only real page surfaces are Core pages; helper and devtools contents are not.
    // 只有真实页面表面才是 Core 页面；辅助内容与开发者工具内容不是。
    const kind = contents.getType();
    if (kind !== 'window' && kind !== 'browserView') {
      return;
    }
    // Popups belong to the background session and must not unexpectedly create visible windows.
    // 弹出页归后台会话所有，不能意外创建可见窗口。
    contents.setWindowOpenHandler(() => ({
      action: 'allow',
      overrideBrowserWindowOptions: {
        show: false,
        webPreferences: {
          nodeIntegration: false, contextIsolation: true, sandbox: true, backgroundThrottling: false,
        },
      },
    }));
    const targetId = String(contents.id);
    watchPageDialogs(contents, targetId);
    // A page opened by another page keeps its opener so Core can relate the two identities.
    // 由其他页面打开的页面保留其打开者，使 Core 能关联两个标识。
    const openerTargetId = resolveOpenerTargetId(contents);
    emitEvent('pageCreated', {
      targetId,
      url: contents.getURL(),
      openerTargetId,
    });
    contents.on('did-navigate', (_navigationEvent, url) => {
      emitEvent('pageNavigated', { targetId, url });
    });
    contents.on('did-navigate-in-page', (_navigationEvent, url) => {
      emitEvent('pageNavigated', { targetId, url });
    });
    contents.once('destroyed', () => {
      emitEvent('pageDestroyed', { targetId });
    });
  });
}

/** One unresolved page dialog, with a bounded default-dismiss timer. */
/** 一个尚未解决的页面对话框，带有限时默认拒绝计时器。 */
interface PendingPageDialog {
  dialogId: string;
  targetId: string;
  timeout: ReturnType<typeof setTimeout>;
}

/** One intercepted HTML file input awaiting a controller-supplied Core upload. */
/** 一个等待控制者提供 Core 上传文件的已拦截 HTML 文件输入。 */
interface PendingFileChooser {
  chooserId: string;
  backendNodeId: number;
  timeout: ReturnType<typeof setTimeout>;
}

/** Dialogs currently waiting for the session's exclusive controller. */
/** 当前等待会话独占控制者处理的对话框。 */
const pendingPageDialogs = new Map<string, PendingPageDialog>();

/** File chooser identities keyed by the exact host page that opened them. */
/** 按打开它们的精确宿主页面索引的文件选择器身份。 */
const pendingFileChoosers = new Map<string, PendingFileChooser>();

/** Captures JavaScript dialogs through the page's CDP debugger and defaults unanswered dialogs closed. */
/** 通过页面 CDP 调试器捕获 JavaScript 对话框，并对未答复的对话框执行默认关闭。 */
function watchPageDialogs(contents: Electron.WebContents, targetId: string): void {
  try {
    contents.debugger.attach('1.3');
  } catch (error) {
    logDiagnostic(`page dialog debugger attach failed: ${String(error)}`);
    return;
  }
  const onPageEvent = (method: string, params: Record<string, unknown>) => {
    if (method === 'Page.javascriptDialogOpening') {
      logDiagnostic(`page dialog opened on target ${targetId}`);
      const dialogId = randomUUID();
      const old = pendingPageDialogs.get(targetId);
      if (old) clearTimeout(old.timeout);
      const timeout = setTimeout(() => {
        if (pendingPageDialogs.get(targetId)?.dialogId !== dialogId) return;
        pendingPageDialogs.delete(targetId);
        void sendPageCdpCommand(targetId, 'Page.handleJavaScriptDialog', { accept: false })
          .catch((error: unknown) => logDiagnostic(`dialog default dismissal failed: ${String(error)}`));
        emitEvent('dialogClosed', { targetId, dialogId, reason: 'timeout' });
      }, 30_000);
      pendingPageDialogs.set(targetId, { dialogId, targetId, timeout });
      emitEvent('dialogOpened', {
        targetId, dialogId,
        dialogType: String(params.type ?? ''),
        message: String(params.message ?? '').slice(0, 4096),
        defaultPrompt: String(params.defaultPrompt ?? '').slice(0, 4096),
      });
    } else if (method === 'Page.javascriptDialogClosed') {
      const pending = pendingPageDialogs.get(targetId);
      if (!pending) return;
      clearTimeout(pending.timeout);
      pendingPageDialogs.delete(targetId);
      emitEvent('dialogClosed', { targetId, dialogId: pending.dialogId, reason: 'closed' });
    } else if (method === 'Page.fileChooserOpened') {
      if (!Number.isInteger(params.backendNodeId) || Number(params.backendNodeId) <= 0) return;
      const chooserId = randomUUID();
      const old = pendingFileChoosers.get(targetId);
      if (old) clearTimeout(old.timeout);
      const timeout = setTimeout(() => {
        if (pendingFileChoosers.get(targetId)?.chooserId !== chooserId) return;
        pendingFileChoosers.delete(targetId);
        emitEvent('fileChooserClosed', { targetId, chooserId, reason: 'timeout' });
      }, 30_000);
      pendingFileChoosers.set(targetId, { chooserId, backendNodeId: Number(params.backendNodeId), timeout });
      emitEvent('fileChooserOpened', { targetId, chooserId, mode: String(params.mode ?? '') });
    }
  };
  contents.once('destroyed', () => {
    const pending = pendingPageDialogs.get(targetId);
    if (pending) clearTimeout(pending.timeout);
    pendingPageDialogs.delete(targetId);
    const chooser = pendingFileChoosers.get(targetId);
    if (chooser) clearTimeout(chooser.timeout);
    pendingFileChoosers.delete(targetId);
    pageCdpLinks.get(targetId)?.socket.close();
    pageCdpLinks.delete(targetId);
  });
  // The page's remote CDP transport is known to emit dialogs in the measured Playwright probe;
  // Electron's local debugger did not, despite Page.enable succeeding on the same target.
  // 已实测页面远程 CDP 传输能够发出对话框事件；Electron 本地调试器虽成功执行 Page.enable，
  // 却没有收到同一目标的事件。
  const enablePageEvents = () => {
    if (!contents.debugger.isAttached()) return;
    void connectPageCdp(contents, targetId, onPageEvent)
      .catch((error: unknown) => logDiagnostic(`page CDP connection failed: ${String(error)}`));
  };
  enablePageEvents();
  contents.on('did-finish-load', enablePageEvents);
}

/** Applies one exact pending dialog answer from the Core-authorized controller. */
/** 应用由 Core 授权控制者提交的一个精确待处理对话框答复。 */
async function respondToPageDialog(request: BrowserHostRequest): Promise<void> {
  const window = resolveRequiredTarget(request.params?.targetId);
  const dialogId = typeof request.params?.dialogId === 'string' ? request.params.dialogId : '';
  const pending = window ? pendingPageDialogs.get(String(window.webContents.id)) : null;
  if (!window || !pending || pending.dialogId !== dialogId || typeof request.params?.accept !== 'boolean') {
    respond({ requestId: request.requestId, ok: false,
      error: { code: 'stale_dialog', message: 'The requested page dialog is no longer pending.' } });
    return;
  }
  const promptText = request.params?.promptText;
  if (promptText !== undefined && promptText !== null && (typeof promptText !== 'string' || promptText.length > 4096)) {
    respond({ requestId: request.requestId, ok: false,
      error: { code: 'invalid_request', message: 'Dialog prompt text must be at most 4096 characters.' } });
    return;
  }
  try {
    await sendPageCdpCommand(pending.targetId, 'Page.handleJavaScriptDialog', {
      accept: request.params.accept,
      ...(typeof promptText === 'string' ? { promptText } : {}),
    });
    clearTimeout(pending.timeout);
    pendingPageDialogs.delete(pending.targetId);
    emitEvent('dialogClosed', { targetId: pending.targetId, dialogId, reason: 'answered' });
    respond({ requestId: request.requestId, ok: true,
      result: { targetId: pending.targetId, dialogId, answered: true } });
  } catch (error) {
    respond({ requestId: request.requestId, ok: false,
      error: { code: 'dialog_failed', message: String(error) } });
  }
}

/** Keeps page downloads inside the per-session Core control directory and reports their identity. */
/** 将页面下载保存在每会话的 Core 控制目录中，并报告其身份。 */
function watchBrowserDownloads(downloadDirectory: string): void {
  session.defaultSession.on('will-download', (_event, item, contents) => {
    const window = BrowserWindow.fromWebContents(contents);
    if (!window || window.isDestroyed()) {
      item.cancel();
      return;
    }
    // The file path is generated from a UUID, not from the origin-controlled suggested name.
    // 文件路径由 UUID 生成，而非由源站可控制的建议文件名生成。
    const downloadId = randomUUID();
    const targetId = String(contents.id);
    const fileName = path.basename(item.getFilename()).replace(/[<>:"/\\|?*\x00-\x1f]/g, '_').slice(0, 180) || 'download';
    item.setSavePath(path.join(downloadDirectory, downloadId));
    emitEvent('downloadStarted', { targetId, downloadId, fileName, url: item.getURL() });
    item.on('updated', () => {
      if (item.getReceivedBytes() > BROWSER_HOST_MAX_DOWNLOAD_BYTES) {
        item.cancel();
      }
    });
    item.once('done', (_doneEvent, state) => {
      emitEvent('downloadFinished', {
        targetId, downloadId, fileName,
        status: state === 'completed' && item.getReceivedBytes() <= BROWSER_HOST_MAX_DOWNLOAD_BYTES
          ? 'completed' : 'failed',
        byteLength: item.getReceivedBytes(),
      });
    });
  });
}

/** Hides every local host window while keeping the session and its pages alive. */
/** 隐藏所有宿主本地窗口，同时保持会话及其页面存活。 */
function hideOwnedWindows(): number {
  const windows = BrowserWindow.getAllWindows();
  for (const window of windows) {
    // Hiding never closes a page, so the browser context and every target survive.
    // 隐藏绝不关闭页面，因此浏览器上下文与每个目标都存活。
    window.hide();
  }
  return windows.length;
}

/** True only while the host is genuinely ending, so shutdown may really close its windows. */
/** 仅在宿主真正结束时为真，使关闭过程可以真正关闭其窗口。 */
let hostIsEnding = false;

/** Installs the window-close policy: a user close hides the surface instead of ending the session. */
/** 安装窗口关闭策略：用户关闭只隐藏表面，而不结束会话。 */
function guardWindowClosure(window: BrowserWindow): void {
  window.on('close', (event) => {
    if (hostIsEnding) {
      return;
    }
    // Closing the local operation window must not end the browser context; only Core ending the
    // session does. The window is hidden so the user can reveal it again without losing state.
    // 关闭本地操作窗口绝不结束浏览器上下文；只有 Core 结束会话才会。窗口被隐藏，使用户可以在不丢失
    // 状态的情况下再次显示它。
    event.preventDefault();
    window.hide();
    emitEvent('windowHidden', {
      targetId: String(window.webContents.id),
      reason: 'localClose',
    });
  });
}

/** Events emitted before the control channel existed, flushed once Core connects. */
/** 控制通道建立之前发出的事件，在 Core 连接后一次性送出。 */
const bufferedEvents: BrowserHostEvent[] = [];

/** Emits one unsolicited event to Core, buffering it until the control channel exists. */
/** 向 Core 发送一条非请求事件；在控制通道建立前先缓冲。 */
function emitEvent(kind: string, payload: unknown): void {
  eventSequence += 1;
  const event: BrowserHostEvent = {
    eventSequence: String(eventSequence),
    kind,
    payload,
  };
  // The initial page exists before Core connects, so an event emitted then must be retained
  // rather than dropped; otherwise Core would never learn about the session's first page.
  // 初始页面在 Core 连接之前就已存在，因此那时发出的事件必须被保留而不是丢弃，
  // 否则 Core 永远不会得知该会话的第一个页面。
  if (!controlSocket || controlSocket.destroyed) {
    bufferedEvents.push(event);
    return;
  }
  writeControlFrame(event);
}

/** Sends every buffered event now that Core owns the channel. */
/** 在 Core 拥有通道后送出所有已缓冲事件。 */
function flushBufferedEvents(): void {
  while (bufferedEvents.length > 0) {
    const event = bufferedEvents.shift();
    if (event) {
      writeControlFrame(event);
    }
  }
}

/** Reports one operation result while echoing the exact request identity. */
/** 回传精确请求标识的同时报告一条操作结果。 */
function respond(response: BrowserHostResponse): void {
  writeControlFrame(response);
}

/** Closes the host page within a bounded wait and exits the process. */
/** 在有限等待内关闭宿主页面并退出进程。 */
async function shutdownAndExit(): Promise<void> {
  if (shuttingDown) {
    return;
  }
  shuttingDown = true;
  // From here on the host is genuinely ending, so its windows may really close.
  // 自此宿主真正结束，其窗口可以真正关闭。
  hostIsEnding = true;
  // Held keys are released before the context goes away, so a session that ends mid-shortcut does
  // not leave a modifier pressed in whatever page was focused.
  // 在上下文消失之前释放已按下的按键，因此在一个快捷键中途结束的会话不会让任何页面残留按下的修饰键。
  releaseAllPressedKeys();
  logDiagnostic('control channel ended; browser host is flushing profile state');
  // A persistent profile only retains a login if its stores reach disk, and Chromium keeps both
  // cookies and DOM storage in memory until it is told to commit. Measured: without these two
  // flushes neither a cookie nor a localStorage entry survives into the next session, because
  // destroying windows first and exiting abruptly discards the buffered writes.
  // 持久画像只有在存储落盘后才会保留登录状态，而 Chromium 在被要求提交之前会把 Cookie 与 DOM 存储
  // 都留在内存中。实测：缺少这两次刷新时，Cookie 与 localStorage 都无法延续到下一个会话，因为先销毁
  // 窗口再突然退出会丢弃缓冲中的写入。
  try {
    await session.defaultSession.cookies.flushStore();
  } catch (error) {
    logDiagnostic(`cookie store flush failed: ${String(error)}`);
  }
  try {
    await session.defaultSession.flushStorageData();
  } catch (error) {
    logDiagnostic(`DOM storage flush failed: ${String(error)}`);
  }
  // A bounded fallback guarantees the process still terminates promptly if the graceful path is
  // blocked. It is armed after the flushes, so it never truncates them.
  // 有限兜底保证即使优雅路径被阻塞进程仍会及时终止。它在刷新之后才被设置，因此绝不会截断刷新。
  const forcedExit = setTimeout(() => {
    logDiagnostic('graceful quit exceeded its bound; exiting immediately');
    app.exit(0);
  }, BROWSER_HOST_GRACEFUL_QUIT_TIMEOUT_MS);
  forcedExit.unref?.();
  // Windows are closed by the quit itself so renderers get a chance to commit their own state;
  // destroying them up front is what loses DOM storage.
  // 由退出流程自身关闭窗口，使渲染进程有机会提交自身状态；提前销毁窗口正是丢失 DOM 存储的原因。
  app.quit();
  return;
}

/** Content size applied to every owned page, including pages created later. */
/** 应用于所有自有页面（包括后来创建的页面）的内容尺寸。 */
let activeViewport = { width: 1280, height: 800 };

/** Accepts only the page schemes admitted by the Core browser contract. */
/** 仅接受 Core 浏览器契约允许的页面协议。 */
function isAllowedPageUrl(value: string): boolean {
  if (value === 'about:blank') {
    return true;
  }
  try {
    const parsed = new URL(value);
    return (parsed.protocol === 'http:' || parsed.protocol === 'https:') && Boolean(parsed.hostname);
  } catch {
    return false;
  }
}

/** Handles one Core request inside the single-session host. */
/** 在单会话宿主内处理一条 Core 请求。 */
function handleRequest(request: BrowserHostRequest): void {
  if (request.sessionId !== options.sessionId || request.hostGeneration !== options.hostGeneration) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: {
        code: 'host_generation_mismatch',
        message: 'Request identity does not match this browser host.',
      },
    });
    return;
  }
  switch (request.operation) {
    case 'host.ping':
      respond({ requestId: request.requestId, ok: true, result: { pid: process.pid } });
      return;
    case 'host.shutdown':
      respond({ requestId: request.requestId, ok: true, result: { acknowledged: true } });
      void shutdownAndExit();
      return;
    case 'page.create':
      void createOwnedPage(request);
      return;
    case 'page.navigate':
      void navigateOwnedPage(request);
      return;
    case 'viewport.set':
      setOwnedViewport(request);
      return;
    case 'window.hide':
      respond({
        requestId: request.requestId,
        ok: true,
        result: { hidden: hideOwnedWindows() },
      });
      return;
    case 'page.list':
      respond({
        requestId: request.requestId,
        ok: true,
        result: {
          pages: BrowserWindow.getAllWindows().map((window) => ({
            targetId: String(window.webContents.id),
            url: window.webContents.getURL(),
            // Report native visibility so background-window behavior is independently verifiable.
            // 报告原生可见性，使后台窗口行为可以独立验证。
            visible: window.isVisible(),
          })),
        },
      });
      return;
    case 'page.capture':
      void captureOwnedPage(request);
      return;
    case 'input.dispatch':
      dispatchInputEvent(request);
      return;
    case 'input.releaseAll':
      respond({
        requestId: request.requestId,
        ok: true,
        result: { released: releaseAllPressedKeys() },
      });
      return;
    case 'page.upload':
      void uploadIntoOwnedPage(request);
      return;
    case 'dialog.respond':
      void respondToPageDialog(request);
      return;
    default:
      respond({
        requestId: request.requestId,
        ok: false,
        error: {
          code: 'capability_unsupported',
          message: `Browser host does not implement operation \`${request.operation}\`.`,
        },
      });
  }
}

/** Keys this host has pressed and not yet released, keyed by the page that received them. */
/** 本宿主已按下但尚未释放的按键，按接收它们的页面索引。 */
const pressedKeys = new Map<number, Set<string>>();

/** Resolves the host page one input event is addressed to. */
/** 解析一条输入事件所指向的宿主页面。 */
function resolveInputTarget(targetId: unknown): Electron.BrowserWindow | null {
  const requested = typeof targetId === 'string' ? targetId : null;
  const windows = BrowserWindow.getAllWindows();
  if (!requested) {
    return windows[0] ?? null;
  }
  return windows.find((candidate) => String(candidate.webContents.id) === requested) ?? null;
}

/** Resolves an explicitly named host target without selecting a fallback page. */
/** 解析明确指定的宿主目标，不回退到任意页面。 */
function resolveRequiredTarget(targetId: unknown): Electron.BrowserWindow | null {
  if (typeof targetId !== 'string' || targetId.length === 0) {
    return null;
  }
  return BrowserWindow.getAllWindows().find(
    (candidate) => String(candidate.webContents.id) === targetId,
  ) ?? null;
}

/** Navigates only the Core-selected host target and reports its actual URL. */
/** 仅导航 Core 选定的宿主目标，并报告其实际地址。 */
async function navigateOwnedPage(request: BrowserHostRequest): Promise<void> {
  const page = resolveRequiredTarget(request.params?.targetId);
  const url = request.params?.url;
  if (!page || typeof url !== 'string' || !isAllowedPageUrl(url)) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: { code: 'invalid_request', message: 'Navigation requires a live target and an allowed URL.' },
    });
    return;
  }
  try {
    await page.loadURL(url);
    respond({
      requestId: request.requestId,
      ok: true,
      result: { targetId: String(page.webContents.id), url: page.webContents.getURL() },
    });
  } catch (error) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: { code: 'navigation_failed', message: String(error) },
    });
  }
}

/** Resizes every owned page's content area before confirming the new session viewport. */
/** 调整每个自有页面的内容区域后才确认新的会话视口。 */
function setOwnedViewport(request: BrowserHostRequest): void {
  const width = request.params?.width;
  const height = request.params?.height;
  if (!Number.isInteger(width) || !Number.isInteger(height)
      || typeof width !== 'number' || typeof height !== 'number'
      || width < 1 || height < 1 || width > BROWSER_HOST_MAX_VIEWPORT_DIMENSION || height > BROWSER_HOST_MAX_VIEWPORT_DIMENSION) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: { code: 'invalid_request', message: `Viewport dimensions must be integers from 1 to ${BROWSER_HOST_MAX_VIEWPORT_DIMENSION}.` },
    });
    return;
  }
  try {
    const windows = BrowserWindow.getAllWindows();
    for (const window of windows) {
      window.setContentSize(width, height);
      const [actualWidth, actualHeight] = window.getContentSize();
      if (actualWidth !== width || actualHeight !== height) {
        throw new Error(`Host content size is ${actualWidth}x${actualHeight}, expected ${width}x${height}.`);
      }
    }
    activeViewport = { width, height };
    respond({
      requestId: request.requestId,
      ok: true,
      result: { width, height, pageCount: windows.length },
    });
  } catch (error) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: { code: 'viewport_failed', message: String(error) },
    });
  }
}

/** Releases every key this host still holds down, so no key stays stuck in a page. */
/** 释放本宿主仍按下的所有按键，使页面中不会有按键被粘住。 */
function releaseAllPressedKeys(): number {
  let released = 0;
  for (const [webContentsId, keys] of pressedKeys) {
    const window = BrowserWindow.getAllWindows().find(
      (candidate) => candidate.webContents.id === webContentsId,
    );
    if (!window) {
      continue;
    }
    for (const keyCode of keys) {
      // A keyUp for a key the page saw pressed is what actually clears the stuck state.
      // 为页面已看到按下的按键发送 keyUp 才是真正清除粘键状态的操作。
      window.webContents.sendInputEvent({ type: 'keyUp', keyCode });
      released += 1;
    }
  }
  pressedKeys.clear();
  return released;
}

/** Dispatches one input event into a host page. */
/** 将一条输入事件派发到宿主页面。 */
function dispatchInputEvent(request: BrowserHostRequest): void {
  const window = resolveInputTarget(request.params?.targetId);
  if (!window) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: {
        code: 'unknown_target',
        message: 'The browser host owns no page with the requested target identity.',
      },
    });
    return;
  }
  const params = request.params ?? {};
  const kind = typeof params.kind === 'string' ? params.kind : '';
  const respondUnsupported = (): void =>
    respond({
      requestId: request.requestId,
      ok: false,
      // An unsupported input capability is named explicitly rather than silently ignored, because
      // a dropped input looks to the user like an unresponsive page.
      // 不支持的输入能力会被明确指名，而不是被静默忽略，因为被丢弃的输入在用户看来就是页面无响应。
      error: {
        code: 'capability_unsupported',
        message: `Browser host does not implement input kind \`${kind}\`.`,
      },
    });

  if (kind === 'key') {
    const type = params.type === 'up' ? 'keyUp' : params.type === 'down' ? 'keyDown' : null;
    const keyCode = typeof params.key === 'string' ? params.key : '';
    if (!type || !keyCode) {
      respondUnsupported();
      return;
    }
    const held = pressedKeys.get(window.webContents.id) ?? new Set<string>();
    if (type === 'keyDown') {
      held.add(keyCode);
      pressedKeys.set(window.webContents.id, held);
    } else {
      held.delete(keyCode);
      if (held.size === 0) {
        pressedKeys.delete(window.webContents.id);
      }
    }
    window.webContents.sendInputEvent({ type, keyCode });
    respond({ requestId: request.requestId, ok: true, result: { applied: true, kind, type } });
    return;
  }
  if (kind === 'text') {
    // Committed input-method text is delivered as character events, which is how a real IME
    // commits Chinese text into a focused field instead of relying on key codes.
    // 已提交的输入法文本以字符事件交付，这是真实输入法把中文提交到聚焦字段的方式，而不依赖键码。
    const text = typeof params.text === 'string' ? params.text : '';
    if (!text) {
      respondUnsupported();
      return;
    }
    for (const character of text) {
      window.webContents.sendInputEvent({ type: 'char', keyCode: character });
    }
    respond({
      requestId: request.requestId,
      ok: true,
      result: { applied: true, kind, characters: [...text].length },
    });
    return;
  }
  if (kind === 'pointer') {
    const x = Number(params.x);
    const y = Number(params.y);
    if (!Number.isFinite(x) || !Number.isFinite(y)) {
      respondUnsupported();
      return;
    }
    const type =
      params.type === 'up' ? 'mouseUp' : params.type === 'move' ? 'mouseMove' : 'mouseDown';
    const button = typeof params.button === 'string' ? params.button : 'left';
    // Coordinates outside the visible page are clamped instead of rejected: the empty area around
    // the rendered content is still part of the page, and a click there must land somewhere real.
    // 超出可见页面的坐标会被收敛而不是被拒绝：渲染内容周围的空白区域仍属于页面，那里的点击必须落在
    // 真实位置。
    const bounds = window.getContentBounds();
    const clampedX = Math.min(Math.max(Math.round(x), 0), Math.max(bounds.width - 1, 0));
    const clampedY = Math.min(Math.max(Math.round(y), 0), Math.max(bounds.height - 1, 0));
    window.webContents.sendInputEvent({
      type,
      x: clampedX,
      y: clampedY,
      button,
      clickCount: 1,
    } as Electron.MouseInputEvent);
    respond({
      requestId: request.requestId,
      ok: true,
      result: { applied: true, kind, type, x: clampedX, y: clampedY },
    });
    return;
  }
  if (kind === 'zoom') {
    const delta = Number(params.delta);
    if (!Number.isFinite(delta)) {
      respondUnsupported();
      return;
    }
    const level = window.webContents.getZoomLevel() + delta;
    window.webContents.setZoomLevel(level);
    respond({
      requestId: request.requestId,
      ok: true,
      result: { applied: true, kind, zoomLevel: window.webContents.getZoomLevel() },
    });
    return;
  }
  respondUnsupported();
}

/** Places one Core-staged file into a page's file input. */
/** 将一个 Core 暂存的文件放入页面的文件输入中。 */
async function uploadIntoOwnedPage(request: BrowserHostRequest): Promise<void> {
  const window = resolveInputTarget(request.params?.targetId);
  if (!window) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: {
        code: 'unknown_target',
        message: 'The browser host owns no page with the requested target identity.',
      },
    });
    return;
  }
  const stagedPath =
    typeof request.params?.stagedPath === 'string' ? request.params.stagedPath : '';
  if (!stagedPath) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: {
        code: 'invalid_request',
        message: 'Uploading requires a Core-staged file path.',
      },
    });
    return;
  }
  const contents = window.webContents;
  try {
    // `DOM.setFileInputFiles` is the only path that puts a real file into a sandboxed page's file
    // input, because the page itself cannot be handed a path from the renderer.
    // `DOM.setFileInputFiles` 是唯一能把真实文件放入沙箱页面文件输入的途径，因为渲染进程本身无法
    // 被交付路径。
    if (!contents.debugger.isAttached()) {
      contents.debugger.attach('1.3');
    }
    await contents.debugger.sendCommand('DOM.enable');
    const chooserId = typeof request.params?.chooserId === 'string' ? request.params.chooserId : null;
    const chooser = pendingFileChoosers.get(String(contents.id));
    if (chooserId && (!chooser || chooser.chooserId !== chooserId)) {
      respond({ requestId: request.requestId, ok: false,
        error: { code: 'stale_file_chooser', message: 'The selected file chooser is no longer pending.' } });
      return;
    }
    // A controller-selected chooser uses the clicked input; programmatic attachment retains its
    // explicit first-input fallback for automation that never opened the native chooser.
    // 控制者选定的选择器使用被点击的输入；未打开本机选择器的自动化仍沿用明确的首个输入路径。
    let node: { nodeId: number } | { backendNodeId: number };
    if (chooserId && chooser) {
      node = { backendNodeId: chooser.backendNodeId };
    } else {
      const document = (await contents.debugger.sendCommand('DOM.getDocument')) as {
        root: { nodeId: number };
      };
      node = (await contents.debugger.sendCommand('DOM.querySelector', {
        nodeId: document.root.nodeId,
        selector: 'input[type=file]',
      })) as { nodeId: number };
    }
    if ('nodeId' in node && !node.nodeId) {
      respond({
        requestId: request.requestId,
        ok: false,
        error: {
          code: 'no_file_input',
          message: 'The page exposes no file input to receive the upload.',
        },
      });
      return;
    }
    await contents.debugger.sendCommand('DOM.setFileInputFiles', {
      files: [stagedPath],
      ...node,
    });
    if (chooserId && chooser) {
      clearTimeout(chooser.timeout);
      pendingFileChoosers.delete(String(contents.id));
      emitEvent('fileChooserClosed', { targetId: String(contents.id), chooserId, reason: 'selected' });
    }
    respond({
      requestId: request.requestId,
      ok: true,
      result: { attached: true, targetId: String(contents.id) },
    });
  } catch (error) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: { code: 'upload_failed', message: String(error) },
    });
  }
}

/** Creates one host-owned page, because Electron does not accept CDP Target.createTarget. */
/** 创建一个宿主自有页面，因为 Electron 不接受 CDP Target.createTarget。 */
async function createOwnedPage(request: BrowserHostRequest): Promise<void> {
  // Measured boundary (W02): Playwright's own newPage() fails on Electron with
  // "Protocol error (Target.createTarget): Not supported". Page creation is therefore owned by
  // the host, and automation attaches to the page the host created.
  // 已实测边界（W02）：Playwright 自身的 newPage() 在 Electron 上以
  // “Protocol error (Target.createTarget): Not supported” 失败。页面创建因此由宿主拥有，
  // 自动化连接到宿主已创建的页面。
  if (BrowserWindow.getAllWindows().length >= BROWSER_HOST_MAX_PAGES) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: {
        code: 'payload_too_large',
        message: `Browser host already owns ${BROWSER_HOST_MAX_PAGES} pages.`,
      },
    });
    return;
  }
  const url = typeof request.params?.url === 'string' ? request.params.url : 'about:blank';
  if (!isAllowedPageUrl(url)) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: { code: 'invalid_request', message: 'Page creation requires an allowed URL.' },
    });
    return;
  }
  try {
    const page = new BrowserWindow({
      show: false,
      width: activeViewport.width,
      height: activeViewport.height,
      webPreferences: {
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
        // Observation remains active while the Core-owned local window is hidden.
        // Core 拥有的本地窗口隐藏时，观察仍保持活动。
        backgroundThrottling: false,
      },
    });
    guardWindowClosure(page);
    page.setContentSize(activeViewport.width, activeViewport.height);
    await page.loadURL(url);
    respond({
      requestId: request.requestId,
      ok: true,
      result: { created: true, url: page.webContents.getURL(), webContentsId: page.webContents.id },
    });
  } catch (error) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: { code: 'internal_error', message: `Browser host could not create a page: ${String(error)}` },
    });
  }
}

/** Builds the handshake reported once the host owns a live context. */
/** 构建宿主拥有活动上下文后报告一次的握手消息。 */
function buildHandshake(): BrowserHostHandshake {
  return {
    protocolVersion: BROWSER_HOST_PROTOCOL_VERSION,
    sessionId: options.sessionId,
    hostGeneration: options.hostGeneration,
    pid: process.pid,
    runtimeVersion: BROWSER_HOST_RUNTIME_VERSION,
    capabilities: [...BROWSER_HOST_CAPABILITIES],
  };
}

/** Captures one page and reports the image so Core can publish it to observers. */
/** 捕获一个页面并上报图像，使 Core 能将其发布给观察者。 */
async function captureOwnedPage(request: BrowserHostRequest): Promise<void> {
  const window = resolveRequiredTarget(request.params?.targetId);
  if (!window) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: {
        code: 'unknown_target',
        message: 'The browser host owns no page with the requested target identity.',
      },
    });
    return;
  }
  try {
    const image = await window.webContents.capturePage();
    const compressed = image.toJPEG(70);
    // A frame larger than the protocol bound cannot be delivered, and truncating it would present a
    // partial image as if it were complete.
    // 超过协议上限的帧无法被交付，而截断它会把残缺图像当作完整图像呈现。
    if (compressed.length > BROWSER_HOST_MAX_IMAGE_BYTES) {
      respond({
        requestId: request.requestId,
        ok: false,
        error: {
          code: 'frame_too_large',
          message: `Captured frame of ${compressed.length} bytes exceeds the ${BROWSER_HOST_MAX_IMAGE_BYTES} byte limit.`,
        },
      });
      return;
    }
    respond({
      requestId: request.requestId,
      ok: true,
      result: {
        targetId: String(window.webContents.id),
        url: window.webContents.getURL(),
        width: window.getContentSize()[0],
        height: window.getContentSize()[1],
        mimeType: 'image/jpeg',
        byteLength: compressed.length,
        dataBase64: compressed.toString('base64'),
        capturedAt: String(Date.now()),
      },
    });
  } catch (error) {
    respond({
      requestId: request.requestId,
      ok: false,
      error: { code: 'capture_failed', message: String(error) },
    });
  }
}

/** Validates one inbound coreHello frame against this host's spawn-time identity. */
/** 使用本宿主启动时的身份校验一条入站 coreHello 帧。 */
function coreHelloMatches(value: unknown): value is BrowserHostCoreHello {
  if (!value || typeof value !== 'object') {
    return false;
  }
  const hello = value as Partial<BrowserHostCoreHello>;
  return (
    hello.kind === 'coreHello' &&
    hello.protocolVersion === BROWSER_HOST_PROTOCOL_VERSION &&
    hello.connectionSecret === options.connectionSecret &&
    hello.sessionId === options.sessionId &&
    hello.hostGeneration === options.hostGeneration
  );
}

/** Serves the accepted control connection until Core disconnects. */
/** 服务已接受的控制连接，直至 Core 断开。 */
function serveControlConnection(socket: net.Socket): void {
  controlSocket = socket;
  const decoder = new ControlFrameDecoder();
  let handshakeSent = false;
  socket.on('data', (chunk: Buffer) => {
    for (const value of decoder.push(chunk)) {
      if (!handshakeSent) {
        if (!coreHelloMatches(value)) {
          logDiagnostic('rejected a control connection that did not present the spawn-time identity');
          socket.destroy();
          return;
        }
        handshakeSent = true;
        writeControlFrame(buildHandshake());
        // Pages that already existed before Core connected are reported now, in order.
        // Core 连接之前就已存在的页面在此刻按顺序上报。
        flushBufferedEvents();
        // hostReady receives the next sequence only after earlier page events are delivered.
        // 早先的页面事件送达后，hostReady 才获取下一序号。
        emitEvent('hostReady', { url: BrowserWindow.getAllWindows()[0]?.webContents.getURL() ?? null });
        continue;
      }
      handleRequest(value as BrowserHostRequest);
    }
  });
  socket.on('end', () => {
    void shutdownAndExit();
  });
  socket.on('close', () => {
    void shutdownAndExit();
  });
  socket.on('error', (error) => {
    logDiagnostic(`control channel error: ${error.message}`);
    void shutdownAndExit();
  });
}

/** Runs the browser-host process after Electron reported readiness. */
/** Electron 报告就绪后运行浏览器宿主进程。 */
async function runBrowserHost(): Promise<void> {
  // The host never requests the application-level single-instance lock: several Core browser
  // sessions must be able to run this entry point at the same time.
  // 宿主绝不申请应用级单实例锁：多个 Core 浏览器会话必须能同时运行本入口。
  const control = await listenForCoreControl();
  // Closing every local window only hides the surface: the session, its pages, and the browser
  // context stay alive until Core explicitly closes the session.
  // 关闭所有本地窗口只隐藏表面：会话、其页面与浏览器上下文保持存活，直到 Core 显式关闭会话。
  app.on('window-all-closed', () => {
    // Subscribing at all suppresses Electron's default quit-on-last-window-closed behaviour.
    // 只要订阅该事件即可抑制 Electron 默认的“最后一个窗口关闭即退出”行为。
    logDiagnostic('all local windows closed; the browser session stays alive');
  });
  watchBrowserTargets();
  // Download files are saved under the exact control root Core assigned to this session.
  // 下载文件保存在 Core 为此会话分配的精确控制根之下。
  const downloadDirectory = path.join(options.controlDirectory, 'downloads');
  await mkdir(downloadDirectory, { recursive: true });
  watchBrowserDownloads(downloadDirectory);
  // The resolved profile paths are reported because a mismatch between the requested user-data
  // directory and the profile the default session actually uses silently defeats every
  // persistence guarantee.
  // 报告解析后的画像路径，因为“请求的用户数据目录”与“默认会话实际使用的画像”不一致会静默地
  // 使所有持久化保证失效。
  logDiagnostic(
    `profile paths: requested=${options.userDataDirectory} userData=${app.getPath('userData')} session=${session.defaultSession.getStoragePath() ?? 'unknown'}`,
  );
  const endpoint = await waitForCdpEndpoint(
    options.userDataDirectory,
    BROWSER_HOST_CDP_DISCOVERY_TIMEOUT_MS,
  );
  pageCdpPort = endpoint.port;
  const window = new BrowserWindow({
    show: false,
    width: activeViewport.width,
    height: activeViewport.height,
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      // A Core session starts in the background; showing it must be an explicit user action.
      // Core 会话从后台启动；显示窗口必须由用户明确操作。
      backgroundThrottling: false,
    },
  });
  guardWindowClosure(window);
  window.setContentSize(activeViewport.width, activeViewport.height);
  await window.loadURL(options.initialUrl ?? 'about:blank');

  // The discovery document carries no credential; Core already owns the spawn-time secret.
  // 发现文档不携带任何凭据；Core 已持有启动时确定的一次性密钥。
  await writeControlDiscovery(options.controlDirectory, {
    schemaVersion: BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION,
    protocolVersion: BROWSER_HOST_PROTOCOL_VERSION,
    controlHost: control.host,
    controlPort: control.port,
    pid: process.pid,
    sessionId: options.sessionId,
    hostGeneration: options.hostGeneration,
    runtimeVersion: BROWSER_HOST_RUNTIME_VERSION,
    startedAt: String(Date.now()),
  });
  logDiagnostic(`control endpoint published on ${control.host}:${control.port}`);

  const socket = await control.waitForClient(BROWSER_HOST_CONTROL_ACCEPT_TIMEOUT_MS);
  control.close();
  serveControlConnection(socket);
}

/** Reports one fatal host startup failure and exits with a non-zero code. */
/** 报告一次致命宿主启动失败并以非零码退出。 */
function failHostStartup(error: unknown): void {
  logDiagnostic(`startup failed: ${String(error)}`);
  process.exitCode = 1;
  app.exit(1);
}

// Mode selection and debugging switches are installed before any BrowserWindow exists.
// 模式选择与调试开关在任何 BrowserWindow 存在之前完成安装。
try {
  options = parseBrowserHostOptions(process.argv);
} catch (error) {
  logDiagnostic(`argument error: ${String(error)}`);
  process.exit(2);
}

app.commandLine.appendSwitch('remote-debugging-port', String(options.debuggingPort));
app.commandLine.appendSwitch('user-data-dir', options.userDataDirectory);
// Loopback debugging accepts local same-user tooling only; no remote interface is exposed.
// 回环调试仅接受本机同用户工具；不暴露任何远程接口。
app.commandLine.appendSwitch('remote-allow-origins', 'http://127.0.0.1');

app.whenReady().then(runBrowserHost).catch(failHostStartup);
