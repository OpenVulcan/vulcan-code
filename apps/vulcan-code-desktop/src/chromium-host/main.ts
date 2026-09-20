// Core-owned standalone Chromium host using the existing authenticated control protocol.
// 使用现有鉴权控制协议的 Core 自有独立 Chromium 宿主。
import { randomUUID } from 'node:crypto';
import { mkdir, stat, unlink } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { createRequire } from 'node:module';
import type net from 'node:net';
import { chromium, type BrowserContext, type CDPSession, type Dialog, type FileChooser, type Page } from 'playwright-core';
import { PlaywrightProxy } from './playwright-proxy.js';
import { monitorHostCrash } from './crash-report.js';
import { observePageTitle } from './page-title.js';
// Report the actual pinned driver, including when running an isolated packaged host.
// 报告实际锁定驱动版本，包括运行隔离打包宿主时。
const runtimeVersion = `chromium-playwright-${createRequire(import.meta.url)('playwright-core/package.json').version}`;
import { writeControlDiscovery, BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION } from '../browser-host/control-discovery.js';
import { listenForCoreControl, BROWSER_HOST_CONTROL_ACCEPT_TIMEOUT_MS } from '../browser-host/control-server.js';
import {
  BROWSER_HOST_PROTOCOL_VERSION, ControlFrameDecoder, encodeControlFrame,
  type BrowserHostCoreHello, type BrowserHostEvent,
  type BrowserHostRequest, type BrowserHostResponse,
} from '../browser-host/host-protocol.js';

/** Maximum simultaneous pages owned by one Core browser session. */
/** 一个 Core 浏览器会话拥有的最大并存页面数。 */
const MAX_PAGES = 8;
/** Maximum retained bytes for one browser download. */
/** 一份浏览器下载可保留的最大字节数。 */
const MAX_DOWNLOAD_BYTES = 64 * 1024 * 1024;
/** Maximum saved and in-flight downloads owned by one Core browser session. */
/** 单个 Core 浏览器会话最多拥有的已保存与进行中下载数。 */
const MAX_RETAINED_DOWNLOADS = 16;
/** Maximum bytes saved beneath one Core browser session's control directory. */
/** 单个 Core 浏览器会话控制目录内最多保存的下载字节数。 */
const MAX_RETAINED_DOWNLOAD_BYTES = 128 * 1024 * 1024;
/** Maximum JPEG bytes before the bounded control envelope is constructed. */
/** 构造有界控制信封之前允许的最大 JPEG 字节数。 */
const MAX_IMAGE_BYTES = 2 * 1024 * 1024;
/** Maximum page events retained before the authenticated Core handshake. */
/** Core 鉴权握手前最多保留的页面事件数。 */
const MAX_STARTUP_EVENTS = 256;
/** Chromium host capabilities supported by this exact implementation. */
/** 此实现确实支持的 Chromium 宿主能力。 */
const CAPABILITIES = [
  'playwright-endpoint-on-demand', 'automation.proxy', 'loopback-control', 'page.create', 'page.navigate',
  'page.list', 'page.close', 'page.events', 'viewport.set', 'window.hide', 'download.capture',
  'dialog.control', 'file-chooser.intercept',
];

/** Spawn-time identity and explicit filesystem inputs for one host. */
/** 单个宿主的启动身份和明确的文件系统输入。 */
interface Options {
  sessionId: string;
  hostGeneration: string;
  userDataDirectory: string;
  controlDirectory: string;
  connectionSecret: string;
  chromiumExecutable: string;
  initialUrl: string;
}

/** Pending browser dialog held for the Core controller. */
/** 为 Core 控制者保留的待处理浏览器对话框。 */
interface PendingDialog {
  dialogId: string;
  dialog: Dialog;
  timeout: ReturnType<typeof setTimeout>;
}

/** Pending HTML file chooser held for the Core controller. */
/** 为 Core 控制者保留的待处理 HTML 文件选择器。 */
interface PendingChooser {
  chooserId: string;
  chooser: FileChooser;
  timeout: ReturnType<typeof setTimeout>;
}

/** One bounded JPEG frame captured from an exact Chromium page target. */
/** 从精确 Chromium 页面目标捕获的一帧有界 JPEG 画面。 */
interface CapturedPageFrame {
  /** Registered Chromium target identity. */
  /** 已登记的 Chromium 目标身份。 */
  targetId: string;
  /** Actual main-frame navigation count at screenshot time. */
  /** 截图时实际主框架导航次数。 */
  navigationEpoch: number;
  /** Page URL at screenshot completion. */
  /** 截图完成时的页面地址。 */
  url: string;
  /** Captured CSS viewport width. */
  /** 捕获时的 CSS 视口宽度。 */
  width: number;
  /** Captured CSS viewport height. */
  /** 捕获时的 CSS 视口高度。 */
  height: number;
  /** Encoded frame media type. */
  /** 编码后画面的媒体类型。 */
  mimeType: string;
  /** Decoded JPEG byte length. */
  /** 解码后的 JPEG 字节长度。 */
  byteLength: number;
  /** Base64 JPEG payload carried by the bounded control frame. */
  /** 由有界控制帧承载的 Base64 JPEG 内容。 */
  dataBase64: string;
  /** Host wall-clock capture timestamp in Unix milliseconds. */
  /** 宿主记录的 Unix 毫秒截图时间。 */
  capturedAt: string;
}

/** A typed reason why the host could not produce a current screenshot. */
/** 宿主无法生成当前截图时的类型化原因。 */
interface PageCaptureRefusal {
  /** Stable host error classification. */
  /** 稳定的宿主错误类别。 */
  code: string;
  /** Human-readable host refusal reason. */
  /** 供人阅读的宿主拒绝原因。 */
  message: string;
}

/** Returns one nonempty named command-line argument. */
/** 返回一项非空的具名命令行参数。 */
function requiredArgument(name: string): string {
  const value = process.argv.find(argument => argument.startsWith(`--${name}=`))?.slice(name.length + 3);
  if (!value) throw new Error(`Chromium host requires --${name}=<value>.`);
  return value;
}

/** Returns one optional named command-line argument. */
/** 返回一项可选的具名命令行参数。 */
function optionalArgument(name: string): string | null {
  return process.argv.find(argument => argument.startsWith(`--${name}=`))?.slice(name.length + 3) ?? null;
}

/** Parses and validates the precise launch contract before starting Chromium. */
/** 在启动 Chromium 前解析并校验精确的启动契约。 */
function parseOptions(): Options {
  const userDataDirectory = requiredArgument('user-data-dir');
  const initialUrl = optionalArgument('initial-url') ?? 'about:blank';
  if (!allowedPageUrl(initialUrl)) throw new Error('Initial page URL is not allowed.');
  return {
    sessionId: requiredArgument('session-id'),
    hostGeneration: requiredArgument('host-generation'),
    userDataDirectory,
    controlDirectory: requiredArgument('control-dir'),
    connectionSecret: requiredArgument('control-secret'),
    chromiumExecutable: requiredArgument('chromium-executable'),
    initialUrl,
  };
}

/** Admits only ordinary web pages and an explicit blank page. */
/** 仅允许普通网页和明确的空白页。 */
function allowedPageUrl(value: string): boolean {
  if (value === 'about:blank') return true;
  try {
    const url = new URL(value);
    return (url.protocol === 'http:' || url.protocol === 'https:') && Boolean(url.hostname);
  } catch { return false; }
}

/** Writes a diagnostic that does not contain the spawn-time control secret. */
/** 写入一条不含启动控制密钥的诊断。 */
function diagnostic(message: string): void {
  process.stderr.write(`[vulcan-chromium-host] ${message}\n`);
}

/** One live browser session and its Core protocol state. */
/** 一份活动浏览器会话及其 Core 协议状态。 */
class ChromiumHost {
  private context: BrowserContext | null = null;
  /** One Core-granted CDP gateway, revoked with the host or automation owner. */
  /** 随宿主或自动化所有者撤销的单个 Core 授权 CDP 网关。 */
  private playwrightProxy: PlaywrightProxy | null = null;
  private socket: net.Socket | null = null;
  private sequence = 0;
  private ending = false;
  private authenticated = false;
  private viewport = { width: 1280, height: 800 };
  private readonly pages = new Map<string, Page>();
  /** Actual main-frame navigation count for each registered Chromium target. */
  /** 每个已登记 Chromium 目标实际发生的主框架导航次数。 */
  private readonly navigationEpochs = new Map<string, number>();
  private readonly ids = new Map<Page, string>();
  /** Preserve a new document's title when its binding runs before target registration completes. */
  /** 新文档绑定早于目标登记完成时，保留其标题。 */
  private readonly pageTitles = new WeakMap<Page, { title: string; url: string }>();
  private readonly registrations = new Map<Page, Promise<string>>();
  private readonly cdpSessions = new Map<string, CDPSession>();
  private readonly buffered: BrowserHostEvent[] = [];
  private readonly dialogs = new Map<string, PendingDialog>();
  private readonly pointerDialogSignals = new Map<string, () => void>();
  private readonly captureDialogSignals = new Map<string, () => void>();
  /** At most one screenshot per page, shared by deferred projection and first-frame requests. */
  /** 每页最多一项截图任务，供异步投影与首帧请求共同等待。 */
  private readonly pendingCaptures = new Map<string, Promise<CapturedPageFrame | PageCaptureRefusal>>();
  private readonly choosers = new Map<string, PendingChooser>();
  private readonly pressedKeys = new Map<string, Set<string>>();
  /** One pressed UI pointer per page, retained until up or Core control release. */
  /** 每个页面保留一个已按下的界面指针，直到抬起或 Core 释放控制权。 */
  private readonly pressedPointers = new Map<string, { button: 'left' | 'middle' | 'right'; x: number; y: number }>();
  private readonly zoomLevels = new Map<string, number>();
  /** Download identities awaiting a terminal Playwright result. */
  /** 等待 Playwright 终态结果的下载身份。 */
  private readonly activeDownloads = new Set<string>();
  /** Number of successful downloads retained for this browser session. */
  /** 本浏览器会话保留的成功下载数量。 */
  private retainedDownloadCount = 0;
  /** Bytes of successful downloads retained for this browser session. */
  /** 本浏览器会话保留的成功下载字节数。 */
  private retainedDownloadBytes = 0;

  /** Constructs a host from exact Core-selected runtime inputs. */
  /** 使用 Core 精确选定的运行时输入构造宿主。 */
  constructor(private readonly options: Options) {}

  /** Writes one framed control value to the authenticated Core socket. */
  /** 向经过鉴权的 Core 套接字写入一份控制帧。 */
  private write(value: unknown): void {
    // Page close events can arrive after Core has intentionally closed the socket. They are
    // session-local telemetry, so dropping them is safer than an uncaught shutdown exception.
    // Core 主动关闭套接字后页面关闭事件仍可能到达；这些事件只用于本会话遥测，丢弃它们比未捕获异常更安全。
    if (!this.socket || this.socket.destroyed || !this.socket.writable) return;
    this.socket.write(encodeControlFrame(value));
  }

  /** Emits a sequential event, retaining startup events until Core connects. */
  /** 发出顺序事件，并在 Core 连接前保留启动事件。 */
  private emit(kind: string, payload: unknown): void {
    const event: BrowserHostEvent = { eventSequence: String(++this.sequence), kind, payload };
    if (!this.authenticated) {
      if (this.buffered.length >= MAX_STARTUP_EVENTS) {
        diagnostic('startup page event limit exceeded before Core authentication');
        void this.shutdown();
        return;
      }
      this.buffered.push(event);
      return;
    }
    this.write(event);
  }

  /** Sends one correlated result or typed error to Core. */
  /** 向 Core 发送一份关联结果或类型化错误。 */
  private answer(request: BrowserHostRequest, result: unknown, code?: string): void {
    const response: BrowserHostResponse = code
      ? { requestId: request.requestId, ok: false, error: { code, message: String(result) } }
      : { requestId: request.requestId, ok: true, result };
    this.write(response);
  }

  /** Finds only the exact registered page named by Core. */
  /** 仅查找 Core 指名的精确已登记页面。 */
  private page(targetId: unknown): Page | null {
    return typeof targetId === 'string' ? this.pages.get(targetId) ?? null : null;
  }

  /** Registers a page and returns its actual Chromium CDP target identity. */
  /** 登记页面并返回其真实 Chromium CDP 目标身份。 */
  private register(page: Page): Promise<string> {
    const existing = this.registrations.get(page);
    if (existing) return existing;
    const registration = this.registerNewPage(page);
    this.registrations.set(page, registration);
    return registration;
  }

  /** Completes page registration before exposing it to Core. */
  /** 在向 Core 暴露页面之前完成页面登记。 */
  private async registerNewPage(page: Page): Promise<string> {
    if (!this.context) throw new Error('Browser context is not ready.');
    if (this.pages.size >= MAX_PAGES) {
      await page.close();
      throw new Error(`Browser host already owns ${MAX_PAGES} pages.`);
    }
    const cdp = await this.context.newCDPSession(page);
    const target = await cdp.send('Target.getTargetInfo') as { targetInfo: { targetId: string } };
    const targetId = target.targetInfo.targetId;
    const opener = await page.opener();
    const openerTargetId = opener ? this.ids.get(opener) ?? null : null;
    // A popup may arrive while Core resizes existing pages. Apply the latest session viewport
    // before publishing its target; repeat if the requested size changed during the async resize.
    // 弹窗可能在 Core 调整现有页面时出现。发布目标前先应用最新会话视口；异步调整期间若目标尺寸
    // 变化，则再次应用，避免新页面以旧尺寸进入投影。
    let appliedViewport = this.viewport;
    for (;;) {
      await page.setViewportSize(appliedViewport);
      const currentViewport = this.viewport;
      if (appliedViewport.width === currentViewport.width && appliedViewport.height === currentViewport.height) break;
      appliedViewport = currentViewport;
    }
    this.ids.set(page, targetId);
    this.pages.set(targetId, page);
    this.navigationEpochs.set(targetId, 0);
    this.cdpSessions.set(targetId, cdp);
    this.emit('pageCreated', { targetId, url: page.url(), openerTargetId });
    page.on('framenavigated', frame => {
      if (frame === page.mainFrame()) {
        const previousEpoch = this.navigationEpochs.get(targetId);
        if (previousEpoch === undefined) return;
        this.navigationEpochs.set(targetId, previousEpoch + 1);
        this.emit('pageNavigated', { targetId, url: frame.url() });
      }
    });
    page.on('dialog', dialog => this.openDialog(targetId, dialog));
    page.on('filechooser', chooser => this.openChooser(targetId, chooser));
    page.on('download', download => void this.saveDownload(targetId, download));
    page.on('close', () => {
      this.clearDialog(targetId, 'pageClosed');
      this.clearChooser(targetId, 'pageClosed');
      this.pages.delete(targetId);
      this.navigationEpochs.delete(targetId);
      this.ids.delete(page);
      this.cdpSessions.delete(targetId);
      this.pressedKeys.delete(targetId);
      this.pressedPointers.delete(targetId);
      this.zoomLevels.delete(targetId);
      this.emit('pageDestroyed', { targetId });
    });
    await cdp.send('Page.enable');
    cdp.on('Page.javascriptDialogClosed', () => this.clearDialog(targetId, 'closed'));
    if (page.url() !== 'about:blank') this.emit('pageNavigated', { targetId, url: page.url() });
    // A popup may report its initial title before registration; publish only its current document.
    // 弹窗可能在登记前报告初始标题；仅发布它当前文档的数据。
    const metadata = this.pageTitles.get(page);
    if (metadata && metadata.url === page.url()) this.emit('pageMetadataChanged', { targetId, ...metadata });
    return targetId;
  }

  /** Holds a browser dialog until the lease owner responds or its timeout expires. */
  /** 保留浏览器对话框，直到租约所有者答复或超时。 */
  private openDialog(targetId: string, dialog: Dialog): void {
    this.pointerDialogSignals.get(targetId)?.();
    this.captureDialogSignals.get(targetId)?.();
    this.clearDialog(targetId, 'replaced');
    const dialogId = randomUUID();
    const timeout = setTimeout(() => {
      if (this.dialogs.get(targetId)?.dialogId !== dialogId) return;
      void dialog.dismiss().catch(error => diagnostic(`dialog timeout dismissal failed: ${String(error)}`));
      this.clearDialog(targetId, 'timeout');
    }, 30_000);
    this.dialogs.set(targetId, { dialogId, dialog, timeout });
    this.emit('dialogOpened', {
      targetId, dialogId, dialogType: dialog.type(), message: dialog.message().slice(0, 4096),
      defaultPrompt: dialog.defaultValue().slice(0, 4096),
    });
  }

  /** Clears one exact outstanding dialog and informs observers. */
  /** 清除一份精确的待处理对话框并通知观察者。 */
  private clearDialog(targetId: string, reason: string): void {
    const pending = this.dialogs.get(targetId);
    if (!pending) return;
    clearTimeout(pending.timeout);
    this.dialogs.delete(targetId);
    this.emit('dialogClosed', { targetId, dialogId: pending.dialogId, reason });
  }

  /** Holds one clicked file chooser for the authorized Core upload. */
  /** 为经过授权的 Core 上传保留一份点击产生的文件选择器。 */
  private openChooser(targetId: string, chooser: FileChooser): void {
    this.clearChooser(targetId, 'replaced');
    const chooserId = randomUUID();
    const timeout = setTimeout(() => {
      if (this.choosers.get(targetId)?.chooserId !== chooserId) return;
      void chooser.setFiles([]).catch(error => diagnostic(`file chooser timeout failed: ${String(error)}`));
      this.clearChooser(targetId, 'timeout');
    }, 30_000);
    this.choosers.set(targetId, { chooserId, chooser, timeout });
    this.emit('fileChooserOpened', { targetId, chooserId, mode: chooser.isMultiple() ? 'selectMultiple' : 'selectSingle' });
  }

  /** Clears one outstanding chooser and informs Core. */
  /** 清除一份待处理文件选择器并通知 Core。 */
  private clearChooser(targetId: string, reason: string): void {
    const pending = this.choosers.get(targetId);
    if (!pending) return;
    clearTimeout(pending.timeout);
    this.choosers.delete(targetId);
    this.emit('fileChooserClosed', { targetId, chooserId: pending.chooserId, reason });
  }

  /** Saves a browser download only beneath the Core-assigned control directory. */
  /** 仅在 Core 指定的控制目录内保存浏览器下载。 */
  private async saveDownload(targetId: string, download: import('playwright-core').Download): Promise<void> {
    // Each page in this host shares one session budget and one Core-assigned control directory.
    // 此宿主内所有页面共用一份会话额度与一个 Core 指定的控制目录。
    const downloadId = randomUUID();
    const fileName = path.basename(download.suggestedFilename()).replace(/[<>:"/\\|?*\x00-\x1f]/g, '_').slice(0, 180) || 'download';
    const destination = path.join(this.options.controlDirectory, 'downloads', downloadId);
    this.emit('downloadStarted', { targetId, downloadId, fileName, url: download.url() });
    if (this.retainedDownloadCount + this.activeDownloads.size >= MAX_RETAINED_DOWNLOADS
        || this.retainedDownloadBytes >= MAX_RETAINED_DOWNLOAD_BYTES) {
      try { await download.cancel(); }
      catch (error) { diagnostic(`download ${downloadId} cancellation failed: ${String(error)}`); }
      this.emit('downloadFinished', { targetId, downloadId, fileName, status: 'failed', byteLength: 0 });
      return;
    }
    this.activeDownloads.add(downloadId);
    try {
      await download.saveAs(destination);
      const size = (await stat(destination)).size;
      if (size > MAX_DOWNLOAD_BYTES || this.retainedDownloadBytes + size > MAX_RETAINED_DOWNLOAD_BYTES) {
        throw new Error('Browser download exceeds the per-file or per-session retention limit.');
      }
      this.retainedDownloadBytes += size;
      this.retainedDownloadCount += 1;
      this.emit('downloadFinished', { targetId, downloadId, fileName, status: 'completed', byteLength: size });
    } catch (error) {
      try { await unlink(destination); }
      catch (cleanupError) {
        if ((cleanupError as NodeJS.ErrnoException).code !== 'ENOENT') {
          diagnostic(`download ${downloadId} cleanup failed: ${String(cleanupError)}`);
        }
      }
      this.emit('downloadFinished', { targetId, downloadId, fileName, status: 'failed', byteLength: 0 });
      diagnostic(`download ${downloadId} failed: ${String(error)}`);
    } finally {
      this.activeDownloads.delete(downloadId);
    }
  }

  /** Releases held keys and pointer buttons before browser shutdown or control handoff. */
  /** 在浏览器关闭或控制交接前释放已按住的按键与指针按钮。 */
  private async releaseInputs(): Promise<number> {
    let released = 0;
    const failures: string[] = [];
    for (const [targetId, keys] of this.pressedKeys) {
      const page = this.pages.get(targetId);
      if (!page) {
        this.pressedKeys.delete(targetId);
        continue;
      }
      for (const key of keys) {
        try {
          await page.keyboard.up(key);
          keys.delete(key);
          released += 1;
        } catch (error) {
          diagnostic(`key release failed: ${String(error)}`);
          failures.push(`key ${key} on ${targetId}`);
        }
      }
      if (keys.size === 0) this.pressedKeys.delete(targetId);
    }
    // A control handoff must release a drag even if its GUI never delivered pointerup.
    // 即使 GUI 未送达 pointerup，控制权交接也必须释放页面拖拽。
    for (const [targetId, pointer] of this.pressedPointers) {
      // cdp belongs to the exact page where this pointer was pressed.
      // cdp 归属于此指针最初按下的精确页面。
      const cdp = this.cdpSessions.get(targetId);
      if (!cdp) {
        if (this.pages.has(targetId)) failures.push(`pointer on ${targetId}: missing CDP session`);
        else this.pressedPointers.delete(targetId);
        continue;
      }
      try {
        await cdp.send('Input.dispatchMouseEvent', {
          type: 'mouseReleased', x: pointer.x, y: pointer.y,
          button: pointer.button, buttons: 0, clickCount: 1,
        });
        this.pressedPointers.delete(targetId);
        released += 1;
      } catch (error) {
        diagnostic(`pointer release failed: ${String(error)}`);
        failures.push(`pointer on ${targetId}`);
      }
    }
    // Core must block a handoff until every still-live page confirms input release.
    // 每个仍存活的页面确认输入已释放前，Core 必须阻止控制权交接。
    if (failures.length > 0) throw new Error(`Input release unconfirmed: ${failures.join(', ')}`);
    return released;
  }

  /** Delivers one validated Core input event to its exact page. */
  /** 将一条经过 Core 校验的输入事件交付给精确页面。 */
  private async input(request: BrowserHostRequest): Promise<void> {
    const targetId = request.params.targetId;
    const page = this.page(targetId);
    if (!page || typeof targetId !== 'string') return this.answer(request, 'Unknown page target.', 'unknown_target');
    const kind = request.params.kind;
    if (kind === 'key' && typeof request.params.key === 'string'
        && (request.params.type === 'down' || request.params.type === 'up')) {
      const key = request.params.key;
      const held = this.pressedKeys.get(targetId) ?? new Set<string>();
      if (request.params.type === 'down') {
        await page.keyboard.down(key);
        held.add(key);
        this.pressedKeys.set(targetId, held);
      } else {
        await page.keyboard.up(key);
        held.delete(key);
        if (held.size === 0) this.pressedKeys.delete(targetId);
      }
      return this.answer(request, { applied: true, kind, type: request.params.type === 'down' ? 'keyDown' : 'keyUp' });
    }
    if (kind === 'text' && typeof request.params.text === 'string' && request.params.text.length > 0) {
      await page.keyboard.insertText(request.params.text);
      return this.answer(request, { applied: true, kind, characters: [...request.params.text].length });
    }
    if (kind === 'pointer' && Number.isFinite(request.params.x) && Number.isFinite(request.params.y)) {
      const x = Math.min(Math.max(Math.round(Number(request.params.x)), 0), this.viewport.width - 1);
      const y = Math.min(Math.max(Math.round(Number(request.params.y)), 0), this.viewport.height - 1);
      if (request.params.type !== 'down' && request.params.type !== 'move' && request.params.type !== 'up') {
        return this.answer(request, 'Unsupported pointer event type.', 'capability_unsupported');
      }
      const type = request.params.type === 'up' ? 'mouseUp' : request.params.type === 'move' ? 'mouseMove' : 'mouseDown';
      const button = request.params.button === 'right' ? 'right' : request.params.button === 'middle' ? 'middle' : 'left';
      // One page retains one pressed pointer; reject unsupported button chords explicitly.
      // 一个页面只保留一个已按下指针；明确拒绝未支持的多按钮组合。
      const pressed = this.pressedPointers.get(targetId);
      if (type === 'mouseDown' && pressed) {
        return this.answer(request, 'A pointer button is already pressed on this page.', 'capability_unsupported');
      }
      if (type === 'mouseUp' && pressed && pressed.button !== button) {
        return this.answer(request, 'Pointer release button does not match the pressed button.', 'pointer_button_mismatch');
      }
      // CDP receives the button state confirmed by this host, not an untrusted client bit field.
      // CDP 使用本宿主确认的按钮状态，而非客户端传来的未经校验位字段。
      const buttons = type === 'mouseMove'
        ? pressed ? pressed.button === 'right' ? 2 : pressed.button === 'middle' ? 4 : 1 : 0
        : type === 'mouseDown' ? button === 'right' ? 2 : button === 'middle' ? 4 : 1 : 0;
      const cdp = this.cdpSessions.get(targetId);
      if (!cdp) return this.answer(request, 'Page CDP session is not available.', 'unknown_target');
      // Playwright's mouse.up waits for a synchronous alert opened by the click and would deadlock
      // the Core RPC that must return before the controller can answer that dialog.
      // Playwright 的 mouse.up 会等待点击触发的同步 alert，从而阻塞必须先返回、控制者才能答复
      // 对话框的 Core RPC；直接派发 CDP 输入以保持二者解耦。
      const dialogOpened = new Promise<void>(resolve => this.pointerDialogSignals.set(targetId, resolve));
      const dispatch = cdp.send('Input.dispatchMouseEvent', {
        type: type === 'mouseMove' ? 'mouseMoved' : type === 'mouseDown' ? 'mousePressed' : 'mouseReleased',
        x, y, button: type === 'mouseMove' ? 'none' : button, buttons,
        clickCount: type === 'mouseMove' ? 0 : 1,
      });
      try {
        // CDP itself may defer the input command response while the page is blocked in a native
        // dialog. The dialog-opening event is independent proof that the click reached this page;
        // answering the input then allows Core to expose the dialog to its lease owner.
        // 页面阻塞于原生对话框时，CDP 本身可能延迟输入命令应答。对话框打开事件独立证明点击已到达
        // 当前页面；此时答复输入，Core 才能向租约所有者展示对话框。
        await Promise.race([dispatch, dialogOpened]);
      } finally {
        this.pointerDialogSignals.delete(targetId);
      }
      if (type === 'mouseDown') this.pressedPointers.set(targetId, { button, x, y });
      else if (type === 'mouseMove') {
        // pressed preserves the initiating button while the drag position changes.
        // pressed 在拖拽位置变化时保留最初按下的按钮。
        const pressed = this.pressedPointers.get(targetId);
        if (pressed) this.pressedPointers.set(targetId, { ...pressed, x, y });
      } else this.pressedPointers.delete(targetId);
      return this.answer(request, { applied: true, kind, type, x, y });
    }
    if (kind === 'wheel' && Number.isFinite(request.params.x) && Number.isFinite(request.params.y)
        && Number.isFinite(request.params.deltaX) && Number.isFinite(request.params.deltaY)) {
      // x and y identify the host viewport position where the wheel event applies.
      // x 和 y 标识滚轮事件在宿主视口中的作用位置。
      const x = Math.min(Math.max(Math.round(Number(request.params.x)), 0), this.viewport.width - 1);
      const y = Math.min(Math.max(Math.round(Number(request.params.y)), 0), this.viewport.height - 1);
      // deltaX and deltaY retain the client's CSS-pixel scroll distance.
      // deltaX 和 deltaY 保留客户端的 CSS 像素滚动距离。
      const deltaX = Number(request.params.deltaX);
      const deltaY = Number(request.params.deltaY);
      // cdp belongs to the exact page target, never to the workbench Electron view.
      // cdp 归属于精确页面目标，绝不归属于工作台 Electron 视图。
      const cdp = this.cdpSessions.get(targetId);
      if (!cdp) return this.answer(request, 'Page CDP session is not available.', 'unknown_target');
      await cdp.send('Input.dispatchMouseEvent', {
        type: 'mouseWheel', x, y, deltaX, deltaY, button: 'none', buttons: 0,
      });
      return this.answer(request, { applied: true, kind, x, y, deltaX, deltaY });
    }
    if (kind === 'zoom' && Number.isFinite(request.params.delta)) {
      const zoomLevel = (this.zoomLevels.get(targetId) ?? 0) + Number(request.params.delta);
      const cdp = this.cdpSessions.get(targetId);
      if (!cdp) return this.answer(request, 'Page CDP session is not available.', 'unknown_target');
      await cdp.send('Emulation.setPageScaleFactor', { pageScaleFactor: Math.pow(1.2, zoomLevel) });
      this.zoomLevels.set(targetId, zoomLevel);
      return this.answer(request, { applied: true, kind, zoomLevel });
    }
    return this.answer(request, `Chromium host does not implement input kind \`${String(kind)}\`.`, 'capability_unsupported');
  }

  /** Captures one page once while allowing Core to await or defer the same bounded operation. */
  /** 对一个页面执行一次有界截图，允许 Core 同步等待或异步接收同一操作。 */
  /** targetId identifies the registered target; page is its live Playwright page. */
  /** targetId 标识已登记目标；page 是其活动 Playwright 页面。 */
  /** Returns a JPEG frame or a typed refusal; unexpected screenshot failures reject. */
  /** 返回 JPEG 画面或类型化拒绝；非预期截图故障以异常返回。 */
  private async capturePageFrame(targetId: string, page: Page): Promise<CapturedPageFrame | PageCaptureRefusal> {
    if (this.dialogs.has(targetId))
      return { code: 'capture_blocked_by_dialog', message: 'Page is waiting for a dialog answer.' };
    // navigationEpoch is the host's real page lifecycle fact, including Playwright navigation
    // that never passed through Core's own browser.navigate RPC.
    // navigationEpoch 是宿主观察到的真实页面生命周期事实，包含未经过 Core
    // browser.navigate RPC 的 Playwright 导航。
    const navigationEpoch = this.navigationEpochs.get(targetId);
    if (navigationEpoch === undefined)
      return { code: 'unknown_target', message: 'Page target is no longer registered.' };
    const dialogOpened = new Promise<null>(resolve => this.captureDialogSignals.set(targetId, () => resolve(null)));
    let image: Buffer | null;
    // captureStartedAt identifies slow screenshot work that can hold Core's shared control gate.
    // captureStartedAt 标识可能占用 Core 共享控制锁的缓慢截图工作。
    const captureStartedAt = Date.now();
    try {
      // An in-flight screenshot can wait for the page's synchronous JavaScript dialog. Core
      // must release its host connection promptly so the lease owner can answer that dialog.
      // 截图可能等待页面中的同步 JavaScript 对话框；Core 必须迅速释放宿主连接，租约所有者
      // 才能答复对话框。
      image = await Promise.race([page.screenshot({ type: 'jpeg', quality: 70, timeout: 1_500 }), dialogOpened]);
    } finally {
      this.captureDialogSignals.delete(targetId);
      const captureDurationMs = Date.now() - captureStartedAt;
      if (captureDurationMs > 200) diagnostic(`slow page.capture target=${targetId} durationMs=${captureDurationMs}`);
    }
    if (this.navigationEpochs.get(targetId) !== navigationEpoch)
      return { code: 'stale_capture', message: 'Page navigated while the frame was captured.' };
    if (!image) return { code: 'capture_blocked_by_dialog', message: 'Page opened a dialog while the frame was captured.' };
    if (image.length > MAX_IMAGE_BYTES)
      return { code: 'frame_too_large', message: `Captured frame exceeds ${MAX_IMAGE_BYTES} bytes.` };
    const viewport = page.viewportSize();
    if (!viewport) return { code: 'capture_failed', message: 'Page viewport is unavailable.' };
    return {
      targetId, navigationEpoch, url: page.url(), width: viewport.width, height: viewport.height,
      mimeType: 'image/jpeg', byteLength: image.length, dataBase64: image.toString('base64'),
      capturedAt: String(Date.now()),
    };
  }

  /** Applies one host operation without accepting model-selected task identity. */
  /** 执行一项宿主操作，不接受模型自选任务身份。 */
  private async operation(request: BrowserHostRequest): Promise<void> {
    if (request.sessionId !== this.options.sessionId || request.hostGeneration !== this.options.hostGeneration)
      return this.answer(request, 'Request identity does not match this browser host.', 'host_generation_mismatch');
    if (request.operation === 'host.shutdown') {
      this.answer(request, { acknowledged: true });
      void this.shutdown();
      return;
    }
    const context = this.context;
    if (!context) return this.answer(request, 'Browser context is not ready.', 'host_unavailable');
    try {
      switch (request.operation) {
        case 'host.ping': return this.answer(request, { pid: process.pid });
        case 'host.closeContext': {
          // A revoked proxy socket cannot prove a forwarded command stopped.
          // 已撤销的代理套接字无法证明已转发命令停止。
          // Closing Chromium is the host proof required before Core discards uncertain control.
          // 关闭 Chromium 是 Core 丢弃未知控制权之前必需的宿主证明。
          try { await this.playwrightProxy?.shutdown(); }
          catch (error) { diagnostic(`proxy shutdown before context close failed: ${String(error)}`); }
          try { await this.releaseInputs(); }
          catch (error) { diagnostic(`input release before context close failed: ${String(error)}`); }
          await context.close();
          this.context = null;
          this.playwrightProxy = null;
          return this.answer(request, { closed: true });
        }
        case 'automation.open': {
          const connectionId = request.params.connectionId;
          if (typeof connectionId !== 'string' || !connectionId.trim() || !this.playwrightProxy)
            return this.answer(request, 'A valid Core automation connection is required.', 'invalid_request');
          return this.answer(request, await this.playwrightProxy.open(connectionId));
        }
        case 'automation.close': {
          const connectionId = request.params.connectionId;
          if (typeof connectionId !== 'string' || !this.playwrightProxy)
            return this.answer(request, 'A valid Core automation connection is required.', 'invalid_request');
          return this.answer(request, await this.playwrightProxy.close(connectionId));
        }
        case 'page.create': {
          if (this.pages.size >= MAX_PAGES) return this.answer(request, `Browser host already owns ${MAX_PAGES} pages.`, 'payload_too_large');
          const url = typeof request.params.url === 'string' ? request.params.url : 'about:blank';
          if (!allowedPageUrl(url)) return this.answer(request, 'Page creation requires an allowed URL.', 'invalid_request');
          const page = await context.newPage();
          const targetId = await this.register(page);
          await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 15_000 });
          return this.answer(request, { created: true, targetId, url: page.url() });
        }
        case 'page.navigate': {
          const page = this.page(request.params.targetId);
          const url = request.params.url;
          if (!page || typeof url !== 'string' || !allowedPageUrl(url))
            return this.answer(request, 'Navigation requires a live target and an allowed URL.', 'invalid_request');
          await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 15_000 });
          return this.answer(request, { targetId: request.params.targetId, url: page.url() });
        }
        case 'page.close': {
          // Only Core's resolved target is closed; no active-page or URL fallback is permitted.
          // 仅关闭 Core 解析的目标，不使用活动页或地址兜底。
          const page = this.page(request.params.targetId);
          if (!page) return this.answer(request, 'Page target is unavailable.', 'invalid_request');
          await page.close();
          return this.answer(request, { targetId: request.params.targetId, closed: true });
        }
        case 'page.list': return this.answer(request, { pages: [...this.pages].map(([targetId, page]) => ({ targetId, url: page.url(), visible: false })) });
        case 'window.hide': return this.answer(request, { hidden: 0, alreadyHeadless: true });
        case 'viewport.set': {
          const width = request.params.width;
          const height = request.params.height;
          if (!Number.isInteger(width) || !Number.isInteger(height)
              || Number(width) < 1 || Number(height) < 1 || Number(width) > 4096 || Number(height) > 4096)
            return this.answer(request, 'Viewport dimensions must be integers from 1 to 4096.', 'invalid_request');
          // Publish the requested size before awaiting page resizes so concurrently registered
          // popups receive the same viewport rather than the context's launch-time default.
          // 等待页面调整前先发布目标尺寸，使并发登记的弹窗采用相同视口，而非上下文启动时的默认值。
          const previousViewport = this.viewport;
          const requestedViewport = { width: Number(width), height: Number(height) };
          this.viewport = requestedViewport;
          try {
            for (const page of this.pages.values()) {
              if (!page.isClosed()) await page.setViewportSize(requestedViewport);
            }
          } catch (error) {
            // Core retains its old generation after a refusal. Restore every surviving page before
            // returning so the host's actual viewport does not silently diverge from Core.
            // Core 拒绝后保留旧代次。答复前恢复所有存活页面，避免宿主实际视口与 Core 悄然分离。
            this.viewport = previousViewport;
            const restorations = await Promise.allSettled([...this.pages.values()].filter(page => !page.isClosed())
              .map(page => page.setViewportSize(previousViewport)));
            const restorationFailed = restorations.some(result => result.status === 'rejected');
            return this.answer(request,
              `Viewport resize failed: ${String(error)}${restorationFailed ? '; rollback was incomplete' : ''}.`,
              'viewport_failed');
          }
          return this.answer(request, { width, height, pageCount: this.pages.size });
        }
        case 'page.capture': {
          const targetId = request.params.targetId;
          const page = this.page(targetId);
          if (!page || typeof targetId !== 'string') return this.answer(request, 'Unknown page target.', 'unknown_target');
          if (request.params.deferred === true) {
            const pageGeneration = request.params.pageGeneration;
            const viewportGeneration = request.params.viewportGeneration;
            if (typeof pageGeneration !== 'string' || !/^\d+$/.test(pageGeneration)
                || typeof viewportGeneration !== 'string' || !/^\d+$/.test(viewportGeneration))
              return this.answer(request, 'Deferred capture requires exact page and viewport generations.', 'invalid_request');
            if (this.pendingCaptures.has(targetId)) return this.answer(request, { scheduled: false });
            // The host acknowledges scheduling before a slow screenshot finishes. The resulting
            // event carries the Core generation captured with this request for stale-frame rejection.
            // 宿主先确认调度，再等待慢截图完成；结果事件带回本次 Core 代次，供旧画面拒绝。
            const capture = this.capturePageFrame(targetId, page);
            this.pendingCaptures.set(targetId, capture);
            this.answer(request, { scheduled: true });
            void capture.then(result => {
              if ('dataBase64' in result && this.navigationEpochs.get(targetId) === result.navigationEpoch)
                this.emit('pageCaptured', { ...result, pageGeneration, viewportGeneration });
            }).catch(error => diagnostic(`deferred page.capture failed: ${String(error)}`))
              .finally(() => {
                if (this.pendingCaptures.get(targetId) === capture) this.pendingCaptures.delete(targetId);
              });
            return;
          }
          const captured = await (this.pendingCaptures.get(targetId) ?? this.capturePageFrame(targetId, page));
          return 'dataBase64' in captured
            ? this.answer(request, captured)
            : this.answer(request, captured.message, captured.code);
        }
        case 'input.dispatch': {
          // inputStartedAt separates host-side CDP delay from time spent waiting for Core's gate.
          // inputStartedAt 将宿主侧 CDP 延迟与等待 Core 控制锁的时间区分开。
          const inputStartedAt = Date.now();
          try { await this.input(request); }
          finally {
            const inputDurationMs = Date.now() - inputStartedAt;
            if (inputDurationMs > 200) diagnostic(`slow input.dispatch durationMs=${inputDurationMs}`);
          }
          return;
        }
        case 'input.releaseAll': return this.answer(request, { released: await this.releaseInputs() });
        case 'page.upload': {
          const targetId = request.params.targetId;
          const page = this.page(targetId);
          const stagedPath = request.params.stagedPath;
          if (!page || typeof targetId !== 'string') return this.answer(request, 'Unknown page target.', 'unknown_target');
          if (typeof stagedPath !== 'string' || !stagedPath) return this.answer(request, 'Uploading requires a Core-staged file path.', 'invalid_request');
          const chooserId = request.params.chooserId;
          if (typeof chooserId === 'string') {
            const pending = this.choosers.get(targetId);
            if (!pending || pending.chooserId !== chooserId) return this.answer(request, 'The selected file chooser is no longer pending.', 'stale_file_chooser');
            await pending.chooser.setFiles([stagedPath]);
            this.clearChooser(targetId, 'selected');
          } else {
            const input = page.locator('input[type=file]').first();
            if (await input.count() === 0) return this.answer(request, 'The page exposes no file input to receive the upload.', 'no_file_input');
            await input.setInputFiles(stagedPath);
          }
          return this.answer(request, { attached: true, targetId });
        }
        case 'dialog.respond': {
          const targetId = request.params.targetId;
          const pending = typeof targetId === 'string' ? this.dialogs.get(targetId) : null;
          if (!pending || pending.dialogId !== request.params.dialogId || typeof request.params.accept !== 'boolean')
            return this.answer(request, 'The requested page dialog is no longer pending.', 'stale_dialog');
          const promptText = request.params.promptText;
          if (promptText != null && (typeof promptText !== 'string' || promptText.length > 4096))
            return this.answer(request, 'Dialog prompt text must be at most 4096 characters.', 'invalid_request');
          if (request.params.accept) await pending.dialog.accept(typeof promptText === 'string' ? promptText : undefined);
          else await pending.dialog.dismiss();
          this.clearDialog(targetId as string, 'answered');
          return this.answer(request, { targetId, dialogId: pending.dialogId, answered: true });
        }
        default: return this.answer(request, `Chromium host does not implement operation \`${request.operation}\`.`, 'capability_unsupported');
      }
    } catch (error) {
      this.answer(request, String(error), `${request.operation.replaceAll('.', '_')}_failed`);
    }
  }

  /** Closes Chromium gracefully after releasing input and flushing its profile. */
  /** 释放输入并写入资料目录后优雅关闭 Chromium。 */
  private async shutdown(): Promise<void> {
    if (this.ending) return;
    this.ending = true;
    // A stalled relay unbind must not leave Chromium alive after Core disconnects.
    // 中继解绑卡住时不能让 Chromium 在 Core 断开后一直驻留。
    // Start the deadline before asynchronous cleanup while allowing graceful profile flushes.
    // 在异步清理前启动期限，同时允许正常完成时优雅写盘。
    const forced = setTimeout(() => process.exit(0), 6_000);
    // Every cleanup stage runs even when an earlier stage rejects; closing the context is the
    // final browser-process boundary rather than a side effect of successful input release.
    // 即使前一项清理失败也继续执行后续清理；关闭上下文是浏览器进程的最终边界，
    // 不依赖输入释放成功这一前提。
    try { await this.playwrightProxy?.shutdown(); }
    catch (error) { diagnostic(`browser relay shutdown failed: ${String(error)}`); }
    try { await this.releaseInputs(); }
    catch (error) { diagnostic(`browser input shutdown failed: ${String(error)}`); }
    try { await this.context?.close(); }
    catch (error) { diagnostic(`browser context shutdown failed: ${String(error)}`); }
    clearTimeout(forced);
    process.exit(0);
  }

  /** Closes a partially launched browser before reporting a failed startup. */
  /** 在报告启动失败前关闭已部分启动的浏览器；无论中继清理结果如何都尝试关闭上下文。 */
  async abortStartup(): Promise<void> {
    // A failed Core handshake must not strand a Chromium child while cleanup stalls.
    // Core 握手失败后，即使清理卡住也不能留下 Chromium 子进程。
    const forced = setTimeout(() => process.exit(1), 6_000);
    try { await this.playwrightProxy?.shutdown(); }
    catch (error) { diagnostic(`startup relay cleanup failed: ${String(error)}`); }
    try { await this.context?.close(); }
    catch (error) { diagnostic(`startup context cleanup failed: ${String(error)}`); }
    clearTimeout(forced);
  }

  /** Starts Chromium, publishes discovery, then accepts one authenticated Core connection. */
  /** 启动 Chromium、发布发现信息，然后接受一条经过鉴权的 Core 连接。 */
  async run(): Promise<void> {
    await mkdir(path.join(this.options.controlDirectory, 'downloads'), { recursive: true });
    const control = await listenForCoreControl();
    try {
      // Persistent contexts use Playwright's private pipe transport; no TCP debugging listener is requested.
      // 持久化上下文使用 Playwright 私有管道传输，宿主不请求 TCP 调试监听器。
      this.context = await chromium.launchPersistentContext(this.options.userDataDirectory, {
        executablePath: this.options.chromiumExecutable,
        channel: 'chromium', headless: true, acceptDownloads: true,
        viewport: this.viewport,
      });
      // Chromium permits a trusted CDP client to navigate directly to file: URLs. Intercept every
      // routable local-file request before a page can read Core-machine files.
      // Chromium 允许受信 CDP 客户端直接导航 file:；在页面读取 Core 本机文件前拦截全部可路由文件请求。
      await this.context.route('**/*', async route => {
        const request = route.request();
        const scheme = new URL(request.url()).protocol;
        if (scheme === 'file:' || (request.isNavigationRequest() && scheme !== 'http:' && scheme !== 'https:')) {
          await route.abort('blockedbyclient');
        } else {
          await route.continue();
        }
      });
      // Headless Target metadata does not reliably track document.title. Observe the document itself.
      // 无头 Target 元数据不能可靠跟踪 document.title，因此直接观察文档。
      const browser = this.context.browser();
      if (!browser) throw new Error('Persistent Chromium context did not expose its browser owner.');
      await this.context.exposeBinding('__vulcanReportPageTitle', ({ page, frame }, title: unknown, url: unknown) => {
        if (frame !== page.mainFrame() || typeof title !== 'string' || typeof url !== 'string') return;
        // Reject old-document callbacks and never adopt a target solely from page-provided metadata.
        // 拒绝旧文档回调，绝不仅凭页面提供的元数据纳入新目标。
        const targetId = this.ids.get(page);
        if (page.isClosed() || url !== page.url()) return;
        this.pageTitles.set(page, { title, url });
        if (!targetId) return;
        this.emit('pageMetadataChanged', { targetId, url, title });
      });
      await this.context.addInitScript(observePageTitle);
      this.context.on('page', page => void this.register(page).catch(error => diagnostic(`page registration failed: ${String(error)}`)));
      const initial = this.context.pages()[0] ?? await this.context.newPage();
      await this.register(initial);
      // The initial about:blank document predates addInitScript; later pages initialize automatically.
      // 初始 about:blank 文档早于 addInitScript；后续页面会自动初始化。
      await initial.evaluate(observePageTitle);
      if (this.options.initialUrl !== 'about:blank') await initial.goto(this.options.initialUrl, { waitUntil: 'domcontentloaded', timeout: 15_000 });
      this.playwrightProxy = new PlaywrightProxy(browser);
      await writeControlDiscovery(this.options.controlDirectory, {
        schemaVersion: BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION,
        protocolVersion: BROWSER_HOST_PROTOCOL_VERSION,
        controlHost: control.host, controlPort: control.port, pid: process.pid,
        sessionId: this.options.sessionId, hostGeneration: this.options.hostGeneration,
        runtimeVersion, startedAt: String(Date.now()),
      });
      const socket = await control.waitForClient(BROWSER_HOST_CONTROL_ACCEPT_TIMEOUT_MS);
      this.serve(socket);
    } finally {
      // The one-use listener is no longer needed after success or any startup failure.
      // 一次性监听器在成功接入或任意启动失败后都必须关闭。
      control.close();
    }
  }

  /** Serves one authenticated length-framed Core connection. */
  /** 服务一条经过鉴权、使用长度帧的 Core 连接。 */
  private serve(socket: net.Socket): void {
    this.socket = socket;
    const decoder = new ControlFrameDecoder();
    socket.on('data', chunk => {
      try {
        for (const value of decoder.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk))) {
          if (!this.authenticated) {
            const hello = value as Partial<BrowserHostCoreHello>;
            if (hello.kind !== 'coreHello' || hello.protocolVersion !== BROWSER_HOST_PROTOCOL_VERSION
                || hello.sessionId !== this.options.sessionId || hello.hostGeneration !== this.options.hostGeneration
                || hello.connectionSecret !== this.options.connectionSecret) {
              socket.destroy();
              return;
            }
            this.authenticated = true;
            this.write({
              protocolVersion: BROWSER_HOST_PROTOCOL_VERSION, sessionId: this.options.sessionId,
              hostGeneration: this.options.hostGeneration, pid: process.pid,
              runtimeVersion, capabilities: CAPABILITIES,
            });
            for (const event of this.buffered.splice(0)) this.write(event);
            this.emit('hostReady', { url: this.context?.pages()[0]?.url() ?? null });
            continue;
          }
          void this.operation(value as BrowserHostRequest);
        }
      } catch (error) {
        diagnostic(`control frame error: ${String(error)}`);
        socket.destroy();
      }
    });
    socket.on('end', () => void this.shutdown());
    socket.on('close', () => void this.shutdown());
    socket.on('error', error => diagnostic(`control socket error: ${error.message}`));
  }
}

// The host never starts the desktop GUI and never reads a real user's browser profile.
// 宿主绝不启动桌面 GUI，也绝不读取真实用户的浏览器资料目录。
// host remains available for partial-startup cleanup if Chromium launched before an error.
// host 在 Chromium 已启动后发生错误时仍可执行部分启动清理。
let host: ChromiumHost | null = null;
try {
  // Bind fatal diagnostics before asynchronous host work can fail outside startup's catch.
  // 在异步宿主工作可能于启动捕获范围外失败之前绑定致命错误诊断。
  const options = parseOptions();
  monitorHostCrash(options.controlDirectory, options.sessionId, options.hostGeneration);
  host = new ChromiumHost(options);
  await host.run();
} catch (error) {
  diagnostic(`startup failed: ${String(error)}`);
  await host?.abortStartup();
  process.exit(1);
}
