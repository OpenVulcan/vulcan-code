// Electron desktop GUI main process.
// Electron 桌面 GUI 主进程。
//
// The GUI owns a window and native integrations only. It starts or reuses the independent Core
// service, waits for real business readiness, and never stops that service when its window closes.
// GUI 只拥有窗口与原生集成。它启动或复用独立 Core 服务、等待真实业务就绪，并且在关闭窗口时
// 绝不停止该服务。

import { BrowserWindow, Notification, app, clipboard, ipcMain, shell } from 'electron';
import path from 'node:path';

import {
  SERVICE_CHANNELS,
  CLIPBOARD_CHANNELS,
  LOCAL_BROWSER_CHANNELS,
  NOTIFICATION_CHANNELS,
  NOTIFICATION_EVENT_CHANNELS,
  WINDOW_COMMAND_CHANNELS,
  WINDOW_EVENT_CHANNELS,
} from './ipc-channels.cjs';
import { LocalBrowserManager, type LocalBrowserViewport } from './local-browser.js';
import {
  ensureServiceInstance,
  readServiceLaunchDescription,
  readLiveServiceInstance,
  resolveServiceExecutable,
  resolveWebUiUrl,
  type LiveServiceInstance,
  type ServiceStartupProgress,
} from './service-launcher.js';
import { renderStartupDocument } from './startup-page.js';
import { canUseNativeNotifications, configureGuiNotificationIdentity } from './notification-identity.js';
import { createWindowsToastXml } from './notification-toast.js';

// Direct development launches bypass the packaged entry but keep the same identity contract.
// 直接开发启动会绕开发行入口，但仍遵循相同的身份契约。
configureGuiNotificationIdentity();

/** Initial window size used before any user resize. */
const INITIAL_WINDOW_WIDTH = 1440;
const INITIAL_WINDOW_HEIGHT = 900;

/** Minimum window size that keeps the workbench usable. */
const MINIMUM_WINDOW_WIDTH = 960;
const MINIMUM_WINDOW_HEIGHT = 600;

/** Service switches this GUI forwards to the service process on request. */
const FORWARDED_SERVICE_ARGUMENTS = new Set(['--test-root']);

/** Command-line options parsed by this GUI. */
interface DesktopGuiOptions {
  /** Explicit service executable path, or null to resolve the debug default. */
  serviceExecutable: string | null;
  /** Explicit WebUI URL, or null to use the described mode. */
  webUiUrl: string | null;
  /** Arguments forwarded to the service process and its description command. */
  forwardedArguments: string[];
}

/** Splits one `--name=value` or bare `--name` argument. */
function splitArgument(argument: string): { name: string; value: string | null } {
  const separator = argument.indexOf('=');
  if (separator < 0) {
    return { name: argument, value: null };
  }
  return { name: argument.slice(0, separator), value: argument.slice(separator + 1) };
}

/** Parses the GUI command line while preserving only service-owned pass-through arguments. */
// parseDesktopGuiOptions separates GUI-owned switches from the service allowlist. Electron and
// Chromium switches are neither consumed nor forwarded: the service rejects unknown arguments, so
// forwarding an Electron switch such as --remote-debugging-port would break its description command.
// parseDesktopGuiOptions 分离 GUI 自有开关与服务允许列表。Electron 与 Chromium 开关既不消费也不
// 转发：服务会拒绝未知参数，因此转发诸如 --remote-debugging-port 的开关会破坏其描述命令。
function parseDesktopGuiOptions(argv: string[]): DesktopGuiOptions {
  let serviceExecutable: string | null = null;
  let webUiUrl: string | null = null;
  const forwardedArguments: string[] = [];
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index] ?? '';
    const { name, value } = splitArgument(argument);
    if (name === '--service-executable') {
      serviceExecutable = value ?? argv[index + 1] ?? null;
      if (value === null) {
        index += 1;
      }
      continue;
    }
    if (name === '--webui-url') {
      webUiUrl = value ?? argv[index + 1] ?? null;
      if (value === null) {
        index += 1;
      }
      continue;
    }
    // The console switch has no value and must reach both Core description and launch.
    // 控制台开关不带参数，且必须同时到达 Core 描述与启动流程。
    if (argument === '--no-debug-console') {
      forwardedArguments.push(argument);
      continue;
    }
    if (FORWARDED_SERVICE_ARGUMENTS.has(name)) {
      if (value === null) {
        const next = argv[index + 1];
        forwardedArguments.push(next === undefined ? argument : `${name}=${next}`);
        if (next !== undefined) {
          index += 1;
        }
      } else {
        forwardedArguments.push(argument);
      }
    }
  }
  return { serviceExecutable, webUiUrl, forwardedArguments };
}

/** State shared by the IPC handlers of the current window. */
interface GuiWindowState {
  /** The single workbench window. */
  window: BrowserWindow | null;
  /** The confirmed live service instance. */
  instance: LiveServiceInstance | null;
}

const guiState: GuiWindowState = { window: null, instance: null };

// Retain pending Action Center notifications after banner timeout so click handlers stay live.
// 横幅超时后仍保留通知中心的待点击对象，使点击处理器保持有效。
const nativeNotifications = new Map<string, Notification>();
// The renderer keeps at most this many click routes; mirror its bound in the native host.
// 渲染端最多保留相同数量的点击路由；原生宿主采用一致的上限。
const MAX_RETAINED_NATIVE_NOTIFICATIONS = 512;

// retainNativeNotification holds one native click handler and evicts the oldest beyond the route bound.
// retainNativeNotification 保留一个原生点击处理器，并在超出路由上限时逐出最旧对象。
// id identifies the native toast; notification is its live Electron object; no value is returned.
// id 标识原生通知；notification 为有效的 Electron 对象；此函数不返回值。
function retainNativeNotification(id: string, notification: Notification): void {
  nativeNotifications.set(id, notification);
  while (nativeNotifications.size > MAX_RETAINED_NATIVE_NOTIFICATIONS) {
    const oldestId = nativeNotifications.keys().next().value;
    if (typeof oldestId !== 'string') break;
    nativeNotifications.delete(oldestId);
  }
}

// activateNativeNotification routes a click only for a toast delivered by this live GUI.
// activateNativeNotification 只路由由当前有效 GUI 投递的通知点击。
// id is the validated native tag; no value is returned.
// id 为已验证的原生标签；此函数不返回值。
function activateNativeNotification(id: string): void {
  if (!nativeNotifications.has(id)) return;
  const window = guiState.window;
  if (!window || window.isDestroyed()) return;
  nativeNotifications.delete(id);
  if (window.isMinimized()) window.restore();
  window.show();
  // A user click in Windows Action Center can leave the notification pane above a restored GUI.
  // 用户从 Windows 通知中心点击后，通知面板可能仍压在已还原的 GUI 上方。
  window.moveTop();
  window.focus();
  window.webContents.send(NOTIFICATION_EVENT_CHANNELS.activated, { notificationId: id });
}

// parseWindowsNotificationActivation accepts only one exact body-click tag from our toast XML.
// parseWindowsNotificationActivation 只接受本应用通知 XML 中唯一且精确的正文点击标签。
// details are Electron's Windows activation arguments; returns the valid tag or null.
// details 为 Electron 的 Windows 激活参数；返回有效标签或空值。
function parseWindowsNotificationActivation(details: { type: string; arguments: string }): string | null {
  if (details.type !== 'click') return null;
  const parameters = new URLSearchParams(details.arguments);
  const types = parameters.getAll('type');
  const tags = parameters.getAll('tag');
  if (types.length !== 1 || types[0] !== 'click' || tags.length !== 1) return null;
  const tag = tags[0];
  return tag && /^[A-Za-z0-9:._-]{1,64}$/.test(tag) ? tag : null;
}

/** Publishes one startup progress stage to the startup window. */
function reportStartupProgress(window: BrowserWindow, progress: ServiceStartupProgress): void {
  if (window.isDestroyed()) {
    return;
  }
  void window.webContents.executeJavaScript(
    `window.vulcanStartup && window.vulcanStartup(${JSON.stringify(progress.stage)}, ${JSON.stringify(progress.message)});`,
    true,
  );
}

/** Creates the startup window that shows real stages and real errors. */
async function createStartupWindow(): Promise<BrowserWindow> {
  const window = new BrowserWindow({
    width: 520,
    height: 320,
    resizable: false,
    frame: false,
    show: true,
    backgroundColor: '#101215',
    webPreferences: { contextIsolation: true, nodeIntegration: false, sandbox: true },
  });
  await window.loadURL(
    `data:text/html;charset=utf-8,${encodeURIComponent(renderStartupDocument())}`,
  );
  return window;
}

/** Creates the workbench window hosting the WebUI. */
function createWorkbenchWindow(webUiUrl: string): BrowserWindow {
  const window = new BrowserWindow({
    width: INITIAL_WINDOW_WIDTH,
    height: INITIAL_WINDOW_HEIGHT,
    minWidth: MINIMUM_WINDOW_WIDTH,
    minHeight: MINIMUM_WINDOW_HEIGHT,
    resizable: true,
    // The WebUI renders its own title bar and window controls, so the native frame is removed.
    // WebUI 自行渲染标题栏与窗口控制，因此移除原生边框。
    frame: false,
    titleBarStyle: process.platform === 'darwin' ? 'hidden' : 'default',
    // Only WebUI provides visible application chrome and controls.
    // 可见应用边框与控件全部由 WebUI 提供。
    autoHideMenuBar: true,
    show: false,
    backgroundColor: '#101215',
    webPreferences: {
      // The preload is CommonJS because a sandboxed renderer cannot load an ESM preload.
      // 预加载为 CommonJS，因为沙箱渲染进程无法加载 ESM 预加载。
      preload: path.join(import.meta.dirname, '..', 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  window.setMenu(null);
  // Workbench visibility follows successful navigation, not the first empty renderer paint.
  // 工作台只在导航成功后显示，不以空白渲染进程的首次绘制作为就绪。
  window.webContents.on('will-navigate', (event, url) => {
    if (new URL(url).origin !== new URL(webUiUrl).origin) {
      event.preventDefault();
    }
  });
  window.webContents.on('will-redirect', (event, url) => {
    if (new URL(url).origin !== new URL(webUiUrl).origin) {
      event.preventDefault();
    }
  });
  window.on('resize', () => {
    if (!window.isDestroyed()) {
      window.webContents.send(WINDOW_EVENT_CHANNELS.resized);
    }
  });
  window.on('focus', () => {
    if (!window.isDestroyed()) {
      window.webContents.send(WINDOW_EVENT_CHANNELS.focusChanged);
    }
  });
  window.on('blur', () => {
    if (!window.isDestroyed()) {
      window.webContents.send(WINDOW_EVENT_CHANNELS.focusChanged);
    }
  });
  // External links open in the system browser instead of inside the application frame.
  // 外部链接在系统浏览器中打开，而不是在应用框架内打开。
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (['http:', 'https:'].includes(new URL(url).protocol)) {
      void shell.openExternal(url);
    }
    return { action: 'deny' };
  });
  return window;
}

/** Serializes maximize toggles so rapid mixed clicks cannot interleave. */
// A rapid sequence of toggle commands must converge to a single well-defined state instead of
// racing two native maximize transitions.
// 快速连续的切换命令必须收敛到单一明确状态，而不是让两次原生最大化转换互相竞争。
let maximizeQueue: Promise<void> = Promise.resolve();

/** Registers every main-process handler for one window. */
function registerWindowIpcHandlers(window: BrowserWindow, webUiUrl: string, discoveryPath: string, serviceBaseUrl: string): void {
  // blockedBrowserOrigins grows with confirmed Core generations so a restarted Core cannot be
  // opened as an untrusted temporary page when it binds a different port.
  // blockedBrowserOrigins 随已确认的 Core 代次增长，避免重启后换端口的 Core 被临时页面打开。
  const blockedBrowserOrigins = new Set([new URL(webUiUrl).origin, new URL(serviceBaseUrl).origin]);
  // Every bridge handler accepts only the selected workbench main frame, including after navigation.
  // 每个桥处理器仅接受选定工作台主框架，导航后同样核对。
  const handle = <Args extends unknown[]>(channel: string, listener: (event: Electron.IpcMainInvokeEvent, ...args: Args) => unknown): void => {
    ipcMain.handle(channel, (event, ...args) => {
      if (event.sender !== window.webContents || event.senderFrame !== window.webContents.mainFrame
        || new URL(event.senderFrame.url).origin !== new URL(webUiUrl).origin) {
        throw new Error('Desktop capability is restricted to the workbench main frame.');
      }
      return listener(event, ...args as Args);
    });
  };
  // The owner-checked IPC boundary lets the renderer edit text without unrestricted native access.
  // 已核对所有者的 IPC 边界让渲染端编辑文本，同时不开放无限制的原生访问。
  handle(CLIPBOARD_CHANNELS.readText, () => clipboard.readText());
  handle(CLIPBOARD_CHANNELS.writeText, (_event, value: string) => {
    if (typeof value !== 'string') throw new TypeError('Clipboard text must be a string.');
    clipboard.writeText(value);
  });
  handle(CLIPBOARD_CHANNELS.paste, () => window.webContents.paste());
  handle(SERVICE_CHANNELS.connection, async () => {
    // Re-read the authoritative discovery path on reconnect; never search another user namespace.
    // 重连时重读权威发现路径，绝不搜索另一个用户命名空间。
    const instance = await readLiveServiceInstance(discoveryPath);
    if (!instance || instance.status !== 'ready') {
      throw new Error('The selected Core service is not ready.');
    }
    blockedBrowserOrigins.add(new URL(instance.baseUrl).origin);
    guiState.instance = instance;
    return instance;
  });
  // Temporary browser pages run in a separate in-memory partition with no workbench preload.
  // 临时浏览页面运行在独立的内存分区中，不带工作台预加载桥。
  const localBrowser = new LocalBrowserManager(
    window,
    blockedBrowserOrigins,
    (snapshot) => window.webContents.send(LOCAL_BROWSER_CHANNELS.changed, snapshot),
  );
  window.on('close', () => localBrowser.dispose());
  handle(LOCAL_BROWSER_CHANNELS.setOwner, (_event, ownerSessionId: string | null) => localBrowser.setOwner(ownerSessionId));
  handle(LOCAL_BROWSER_CHANNELS.snapshot, () => localBrowser.snapshot());
  handle(LOCAL_BROWSER_CHANNELS.open, (_event, url: string) => localBrowser.open(url));
  handle(LOCAL_BROWSER_CHANNELS.navigate, (_event, id: string, url: string) => localBrowser.navigate(id, url));
  handle(LOCAL_BROWSER_CHANNELS.select, (_event, id: string) => localBrowser.select(id));
  handle(LOCAL_BROWSER_CHANNELS.close, (_event, id: string) => localBrowser.close(id));
  handle(LOCAL_BROWSER_CHANNELS.viewport, (_event, viewport: LocalBrowserViewport | null) =>
    localBrowser.setViewport(viewport));
  // The shared handler guard limits DevTools activation to this workbench's trusted main frame.
  // 共用处理器校验将开发者工具的打开权限限定于此工作台的可信主框架。
  handle(WINDOW_COMMAND_CHANNELS.openDeveloperTools, () => {
    if (window.isDestroyed()) {
      throw new Error('The workbench window is no longer available.');
    }
    window.webContents.openDevTools({ mode: 'right', activate: true });
  });
  handle(WINDOW_COMMAND_CHANNELS.minimize, () => {
    window.minimize();
  });
  handle(WINDOW_COMMAND_CHANNELS.toggleMaximize, () => {
    maximizeQueue = maximizeQueue.then(() => {
      if (window.isDestroyed()) {
        return;
      }
      if (window.isMaximized()) {
        window.unmaximize();
      } else {
        window.maximize();
      }
    });
    return maximizeQueue;
  });
  handle(WINDOW_COMMAND_CHANNELS.isMaximized, () => !window.isDestroyed() && window.isMaximized());
  handle(WINDOW_COMMAND_CHANNELS.isMinimized, () => !window.isDestroyed() && window.isMinimized());
  handle(WINDOW_COMMAND_CHANNELS.isFocused, () => !window.isDestroyed() && window.isFocused());
  handle(WINDOW_COMMAND_CHANNELS.isVisible, () => !window.isDestroyed() && window.isVisible());
  handle(WINDOW_COMMAND_CHANNELS.close, () => {
    if (!window.isDestroyed()) {
      // Closing the GUI window is local; the independent Core service keeps running.
      // 关闭 GUI 窗口是本地的；独立 Core 服务继续运行。
      window.close();
    }
  });
  handle(NOTIFICATION_CHANNELS.permission, () =>
    canUseNativeNotifications() && Notification.isSupported() ? 'granted' : 'unsupported',
  );
  handle(NOTIFICATION_CHANNELS.requestPermission, () =>
    canUseNativeNotifications() && Notification.isSupported() ? 'granted' : 'unsupported',
  );
  handle(
    NOTIFICATION_CHANNELS.show,
    async (_event, request: { id: string; title: string; body: string }) => {
      if (!canUseNativeNotifications() || !Notification.isSupported()) {
        throw new Error('Native notifications are not supported on this host.');
      }
      if (typeof request?.id !== 'string' || !/^[A-Za-z0-9:._-]{1,64}$/.test(request.id)
        || typeof request.title !== 'string' || request.title.length > 256
        || typeof request.body !== 'string' || request.body.length > 2048) {
        throw new Error('Invalid native notification request.');
      }
      // The stable native id preserves the WebUI's exact activation correlation key.
      // 稳定的原生 ID 保留 WebUI 用于激活关联的精确键。
      const notification = new Notification({
        id: request.id, title: request.title, body: request.body,
        ...(process.platform === 'win32' ? {
          toastXml: createWindowsToastXml(request.id, request.title, request.body),
        } : {}),
      });
      retainNativeNotification(request.id, notification);
      // Electron can deliver an in-process Windows body click only through the instance event;
      // COM activation calls the process-wide handler as well. The route removes its id on first use.
      // Electron 可能仅通过对象事件投递进程内的 Windows 正文点击；COM 激活也会调用进程级处理器。
      // 路由首次使用时移除 ID，因此两条路径不会重复打开业务内容。
      notification.on('click', () => activateNativeNotification(request.id));
      // Windows also emits close when only the banner times out while the same toast remains in
      // Action Center. Releasing the object here would discard its later click callback.
      // Windows 仅横幅超时、通知仍在通知中心时也会发出 close；此处释放对象会丢失后续点击回调。
      // Windows builds the toast asynchronously; the IPC call succeeds only after native delivery.
      // Windows 异步创建通知；IPC 仅在原生投递得到确认后返回成功。
      await new Promise<void>((resolve, reject) => {
        // timeout prevents a native backend that emits neither result from hanging the settings UI.
        // timeout 防止原生后端不发出任何结果时设置界面永久等待。
        const timeout = setTimeout(() => {
          nativeNotifications.delete(request.id);
          reject(new Error('Native notification delivery was not confirmed.'));
        }, 10_000);
        notification.once('show', () => {
          clearTimeout(timeout);
          resolve();
        });
        notification.once('failed', (_failedEvent, error) => {
          clearTimeout(timeout);
          nativeNotifications.delete(request.id);
          reject(new Error(`Native notification delivery failed: ${error}`));
        });
        try {
          notification.show();
        } catch (error) {
          clearTimeout(timeout);
          nativeNotifications.delete(request.id);
          reject(error);
        }
      });
    },
  );
}

/** Runs the GUI startup sequence and reports real failures to the user. */
async function runDesktopGui(): Promise<void> {
  const options = parseDesktopGuiOptions(process.argv.slice(1));
  const startupWindow = await createStartupWindow();
  try {
    const executable = await resolveServiceExecutable(options.serviceExecutable);
    const description = await readServiceLaunchDescription(executable, options.forwardedArguments);
    const instance = await ensureServiceInstance(description, options.forwardedArguments, (progress) =>
      reportStartupProgress(startupWindow, progress),
    );
    guiState.instance = instance;
    const webUiUrl = resolveWebUiUrl(description, instance, options.webUiUrl);
    const window = createWorkbenchWindow(webUiUrl);
    guiState.window = window;
    registerWindowIpcHandlers(window, webUiUrl, description.discoveryPath, instance.baseUrl);
    window.once('closed', () => {
      guiState.window = null;
    });
    try {
      await window.loadURL(webUiUrl);
    } catch (error) {
      window.destroy();
      throw new Error(`无法加载工作台 ${webUiUrl}：${String(error)}`);
    }
    window.show();
    startupWindow.destroy();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (!startupWindow.isDestroyed()) {
      void startupWindow.webContents.executeJavaScript(
        `window.vulcanStartupFailed && window.vulcanStartupFailed(${JSON.stringify(message)});`,
        true,
      );
    }
    // The process stays alive so the user can read the real failure instead of a silent exit.
    // 进程保持存活，使用户能读到真实失败信息，而不是静默退出。
    throw error;
  }
}

// The GUI itself is single-instance: a second launch focuses the existing window.
// GUI 自身为单实例：第二次启动会聚焦既有窗口。
if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on('second-instance', () => {
    const window = guiState.window;
    if (window && !window.isDestroyed()) {
      if (window.isMinimized()) {
        window.restore();
      }
      window.focus();
    }
  });
  app.whenReady().then(() => {
    if (process.platform === 'win32') {
      // Register before the workbench starts so Electron can replay a pending native activation.
      // 在工作台启动前注册，使 Electron 能重放待处理的原生激活事件。
      Notification.handleActivation((details) => {
        const id = parseWindowsNotificationActivation(details);
        if (id) activateNativeNotification(id);
      });
    }
    void runDesktopGui().catch((error: unknown) => {
      process.stderr.write(`[vulcan-code-desktop] startup failed: ${String(error)}\n`);
    });
  });
  app.on('window-all-closed', () => {
    // Closing the last GUI window ends the client process only, never the Core service.
    // 关闭最后一个 GUI 窗口只结束客户端进程，绝不结束 Core 服务。
    app.quit();
  });
}
