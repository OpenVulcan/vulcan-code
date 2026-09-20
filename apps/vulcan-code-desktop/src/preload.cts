// Isolated preload that exposes exactly the documented Electron desktop bridge contract.
// 隔离预加载：只暴露文档规定的 Electron 桌面桥契约。
//
// The renderer never receives the raw ipcRenderer object. Every method returns a promise and every
// subscription returns its own unsubscribe function, so the WebUI cannot leak listeners across
// component lifetimes.
// 渲染进程绝不接收原始 ipcRenderer 对象。每个方法返回 Promise，每个订阅返回自身的取消订阅函数，
// 因此 WebUI 不会跨组件生命周期泄漏监听器。

import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron';

// Channel names are inlined because a sandboxed preload cannot require a relative local module:
// only the `electron` module and a small set of Node built-ins are available there. The shared
// definition stays in ./gui/ipc-channels.cts and ipc-channels.test.ts fails if the two ever drift.
// 通道名在此内联，因为沙箱预加载无法 require 相对本地模块：其中只有 `electron` 模块与少量 Node 内置
// 模块可用。共享定义仍位于 ./gui/ipc-channels.cts，若两者发生漂移则 ipc-channels.test.ts 会失败。
const WINDOW_COMMAND_CHANNELS = {
  openDeveloperTools: 'vulcan:window:open-developer-tools',
  minimize: 'vulcan:window:minimize',
  toggleMaximize: 'vulcan:window:toggle-maximize',
  isMaximized: 'vulcan:window:is-maximized',
  isMinimized: 'vulcan:window:is-minimized',
  isFocused: 'vulcan:window:is-focused',
  isVisible: 'vulcan:window:is-visible',
  close: 'vulcan:window:close',
} as const;

// Clipboard channels mirror the main-process allowlist without exposing raw Electron IPC.
// 剪贴板通道与主进程允许列表一致，不暴露原始 Electron IPC。
const CLIPBOARD_CHANNELS = {
  readText: 'vulcan:clipboard:read-text',
  writeText: 'vulcan:clipboard:write-text',
  paste: 'vulcan:clipboard:paste',
} as const;

const LOCAL_BROWSER_CHANNELS = {
  // Matches the main-process initialization channel.
  // 与主进程初始化通道一致。
  initialize: 'vulcan:local-browser:initialize',
  setOwner: 'vulcan:local-browser:set-owner',
  snapshot: 'vulcan:local-browser:snapshot',
  open: 'vulcan:local-browser:open',
  navigate: 'vulcan:local-browser:navigate',
  // Native page actions mirror the main-process allowlist.
  // 原生页面操作与主进程允许列表一致。
  history: 'vulcan:local-browser:history',
  move: 'vulcan:local-browser:move',
  openDeveloperTools: 'vulcan:local-browser:devtools',
  openExternal: 'vulcan:local-browser:external',
  select: 'vulcan:local-browser:select',
  close: 'vulcan:local-browser:close',
  // Matches the confirmed conversation-wide close operation.
  // 与经确认的会话级全部关闭操作一致。
  closeOwner: 'vulcan:local-browser:close-owner',
  viewport: 'vulcan:local-browser:viewport',
  changed: 'vulcan:local-browser:changed',
} as const;

const WINDOW_EVENT_CHANNELS = {
  resized: 'vulcan:window:resized',
  focusChanged: 'vulcan:window:focus-changed',
} as const;

const NOTIFICATION_CHANNELS = {
  permission: 'vulcan:notification:permission',
  requestPermission: 'vulcan:notification:request-permission',
  show: 'vulcan:notification:show',
} as const;

const NOTIFICATION_EVENT_CHANNELS = {
  activated: 'vulcan:notification:activated',
} as const;

/** Subscribes to one main-process event channel and returns the unsubscribe function. */
function subscribe(channel: string, handler: () => void): () => void {
  // The wrapper drops the Electron event object so no IPC internals reach the renderer.
  // 包装函数丢弃 Electron 事件对象，使任何 IPC 内部结构都不会到达渲染进程。
  const listener = (_event: IpcRendererEvent): void => handler();
  ipcRenderer.on(channel, listener);
  return () => {
    ipcRenderer.removeListener(channel, listener);
  };
}

/** The one bridge object mounted on `window.vulcanDesktop.desktop`. */
const desktop = {
  // Source electron.exe has a shared Start Menu shortcut; only branded binaries own native toasts.
  // 源码 electron.exe 使用共享开始菜单快捷方式；只有品牌化二进制文件拥有原生通知。
  nativeNotificationsAvailable: process.platform !== 'win32' || !/[\\/]electron\.exe$/i.test(process.execPath),
  // openDeveloperTools requests DevTools for the owning workbench without exposing raw IPC.
  // openDeveloperTools 请求打开所属工作台的开发者工具，且不暴露原始 IPC。
  openDeveloperTools: (): Promise<void> => ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.openDeveloperTools),
  // readClipboardText returns only the current system clipboard's plain text.
  // readClipboardText 仅返回当前系统剪贴板中的纯文本。
  readClipboardText: (): Promise<string> => ipcRenderer.invoke(CLIPBOARD_CHANNELS.readText),
  // writeClipboardText writes only plain text and never exposes the Electron clipboard object.
  // writeClipboardText 仅写入纯文本，绝不暴露 Electron 剪贴板对象。
  writeClipboardText: (value: string): Promise<void> => ipcRenderer.invoke(CLIPBOARD_CHANNELS.writeText, value),
  // pasteClipboard reuses the page's normal paste event, including attachment handling.
  // pasteClipboard 复用页面的正常粘贴事件，包括附件处理。
  pasteClipboard: (): Promise<void> => ipcRenderer.invoke(CLIPBOARD_CHANNELS.paste),
  minimizeWindow: (): Promise<void> => ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.minimize),
  toggleWindowMaximize: (): Promise<void> =>
    ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.toggleMaximize),
  isWindowMaximized: (): Promise<boolean> => ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.isMaximized),
  isWindowMinimized: (): Promise<boolean> => ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.isMinimized),
  isWindowFocused: (): Promise<boolean> => ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.isFocused),
  isWindowVisible: (): Promise<boolean> => ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.isVisible),
  closeWindow: (): Promise<void> => ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.close),
  subscribeWindowResize: async (handler: () => void): Promise<() => void> =>
    subscribe(WINDOW_EVENT_CHANNELS.resized, handler),
  subscribeWindowFocusChange: async (handler: () => void): Promise<() => void> =>
    subscribe(WINDOW_EVENT_CHANNELS.focusChanged, handler),
  getNotificationPermission: (): Promise<string> =>
    ipcRenderer.invoke(NOTIFICATION_CHANNELS.permission),
  requestNotificationPermission: (): Promise<string> =>
    ipcRenderer.invoke(NOTIFICATION_CHANNELS.requestPermission),
  showNotification: (request: {
    id: string;
    title: string;
    body: string;
  }): Promise<void> => ipcRenderer.invoke(NOTIFICATION_CHANNELS.show, request),
  subscribeNotificationActivation: async (
    handler: (activation: { notificationId: string }) => void,
  ): Promise<() => void> => {
    const listener = (_event: IpcRendererEvent, activation: { notificationId: string }): void =>
      handler(activation);
    ipcRenderer.on(NOTIFICATION_EVENT_CHANNELS.activated, listener);
    return () => {
      ipcRenderer.removeListener(NOTIFICATION_EVENT_CHANNELS.activated, listener);
    };
  },
};

// Service discovery is owned by the GUI and never placed in a URL or guessed by the renderer.
// 服务发现由 GUI 拥有，绝不写入 URL 或由渲染端猜测。
const service = {
  // Returns the current ready instance selected by the owning GUI.
  // 返回所属 GUI 选中的当前就绪实例。
  getConnection: (): Promise<unknown> => ipcRenderer.invoke('vulcan:service:connection'),
};

// localBrowser exposes only tab operations and sidebar geometry, never native WebContents objects.
// localBrowser 只暴露标签操作与侧栏几何，不暴露原生 WebContents 对象。
const localBrowser = {
  // closeOwner resolves true after confirmation and page destruction; false means cancellation.
  // closeOwner 在确认并销毁页面后返回 true；false 表示取消。
  closeOwner: (ownerSessionId: string, locale: string): Promise<boolean> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.closeOwner, ownerSessionId, locale),
  // initialize releases pages left by the previous renderer; resolves after reset.
  // initialize 释放上一个渲染界面遗留的页面；重置后完成。
  initialize: (): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.initialize),
  // setOwner selects one conversation's native tabs within this GUI window.
  // setOwner 在此 GUI 窗口内选择一个会话的原生标签。
  setOwner: (ownerSessionId: string | null): Promise<unknown> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.setOwner, ownerSessionId),
  snapshot: (): Promise<unknown> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.snapshot),
  open: (url: string): Promise<string> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.open, url),
  navigate: (id: string, url: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.navigate, id, url),
  // history forwards an exact page id and navigation action; resolves after dispatch.
  // history 转发精确页面标识与导航操作；派发后完成。
  history: (id: string, action: 'back' | 'forward' | 'reload' | 'stop'): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.history, id, action),
  // move forwards source and destination tab ids; resolves after reordering.
  // move 转发源标签与目标标签标识；排序后完成。
  move: (id: string, targetId: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.move, id, targetId),
  // openDeveloperTools opens the given page's detached inspector; resolves after dispatch.
  // openDeveloperTools 打开指定页面的独立检查器；派发后完成。
  openDeveloperTools: (id: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.openDeveloperTools, id),
  // openExternal uses the given page's native URL; resolves after the OS request.
  // openExternal 使用指定页面的原生地址；操作系统请求后完成。
  openExternal: (id: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.openExternal, id),
  select: (id: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.select, id),
  close: (id: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.close, id),
  viewport: (rect: unknown): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.viewport, rect),
  subscribeChanged: (handler: (snapshot: unknown) => void): (() => void) => {
    const listener = (_event: IpcRendererEvent, snapshot: unknown): void => handler(snapshot);
    ipcRenderer.on(LOCAL_BROWSER_CHANNELS.changed, listener);
    return () => ipcRenderer.removeListener(LOCAL_BROWSER_CHANNELS.changed, listener);
  },
};
contextBridge.exposeInMainWorld('vulcanDesktop', { desktop, service, localBrowser });
