// Isolated preload that exposes exactly the documented Electron desktop bridge contract.
// 隔离预加载：只暴露文档规定的 Electron 桌面桥契约。
//
// The renderer never receives the raw ipcRenderer object. Every method returns a promise and every
// subscription returns its own unsubscribe function, so the WebUI cannot leak listeners across
// component lifetimes.
// 渲染进程绝不接收原始 ipcRenderer 对象。每个方法返回 Promise，每个订阅返回自身的取消订阅函数，
// 因此 WebUI 不会跨组件生命周期泄漏监听器。

import { contextBridge, ipcRenderer, webUtils, type IpcRendererEvent } from 'electron';

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
  toggleFullScreen: 'vulcan:window:toggle-full-screen',
  isFullScreen: 'vulcan:window:is-full-screen',
  placeNativeButtons: 'vulcan:window:place-native-buttons',
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
  pickElement: 'vulcan:local-browser:pick-element',
  cancelElementPick: 'vulcan:local-browser:cancel-element-pick',
  openExternal: 'vulcan:local-browser:external',
  // Matches the validated conversation URL opener in the main process.
  // 与主进程中经过校验的会话 URL 打开器一致。
  openExternalUrl: 'vulcan:local-browser:external-url',
  select: 'vulcan:local-browser:select',
  close: 'vulcan:local-browser:close',
  // Matches the confirmed conversation-wide close operation.
  // 与经确认的会话级全部关闭操作一致。
  closeOwner: 'vulcan:local-browser:close-owner',
  viewport: 'vulcan:local-browser:viewport',
  changed: 'vulcan:local-browser:changed',
} as const;

// Local file channels mirror the main-process handlers that recheck every path.
// 本地文件通道与逐一复核路径的主进程处理器一致。
const LOCAL_FILE_CHANNELS = {
  inspect: 'vulcan:local-file:inspect',
  openWithDefaultApp: 'vulcan:local-file:open-default',
  showInFolder: 'vulcan:local-file:show-in-folder',
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
  // toggleWindowFullScreen enters or leaves native full screen, the macOS green-button action.
  // toggleWindowFullScreen 进入或离开原生全屏，即 macOS 绿色按钮的操作。
  toggleWindowFullScreen: (): Promise<void> => ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.toggleFullScreen),
  isWindowFullScreen: (): Promise<boolean> => ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.isFullScreen),
  // placeNativeButtons forwards the title strip geometry; resolves with the right edge of the macOS buttons.
  // placeNativeButtons 转发标题条几何；返回 macOS 按钮的右边缘。
  placeNativeButtons: (placement: { left: number; height: number }): Promise<number> =>
    ipcRenderer.invoke(WINDOW_COMMAND_CHANNELS.placeNativeButtons, placement),
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
  // Ends this client after an update restart of the Core (see the main-process handler).
  // 在 Core 更新重启后结束本客户端（见主进程处理器）。
  restartForUpdate: (reason: 'update' | 'installer' | 'setup'): Promise<void> =>
    ipcRenderer.invoke('vulcan:service:restart-for-update', reason),
};

// startup connects the WebUI to the startup page this window showed before it.
// startup 连接 WebUI 与本窗口在其之前显示的启动页。
const startup = {
  // Every workbench window shows the desktop startup page first, so the WebUI continues its intro.
  // 每个工作台窗口都会先显示桌面启动页，因此 WebUI 直接接续其入场动效。
  surfaceShown: true,
  // Remembers the WebUI language and appearance for the next startup page.
  // 为下一次启动页记住 WebUI 语言与外观。
  reportAppearance: (appearance: unknown): Promise<void> =>
    ipcRenderer.invoke('vulcan:startup:report-appearance', appearance),
};

// localBrowser exposes only tab operations and sidebar geometry, never native WebContents objects.
// localBrowser 只暴露标签操作与侧栏几何，不暴露原生 WebContents 对象。
const localBrowser = {
  // closeOwner forwards the in-app confirmation for one owner; resolves true only after page destruction.
  // closeOwner 转发一个会话的应用内确认；仅在页面销毁后返回 true。
  closeOwner: (ownerSessionId: string, confirmed: boolean): Promise<boolean> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.closeOwner, ownerSessionId, confirmed),
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
  // pickElement lets the user point at one element of the given page; resolves with it or null when cancelled.
  // pickElement 让用户指向指定页面的一个元素；返回该元素，取消时返回 null。
  pickElement: (id: string): Promise<unknown> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.pickElement, id),
  // cancelElementPick ends a running pick on the given page; resolves after the page is told.
  // cancelElementPick 结束指定页面上进行中的选择；通知页面后完成。
  cancelElementPick: (id: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.cancelElementPick, id),
  // openExternal uses the given page's native URL; resolves after the OS request.
  // openExternal 使用指定页面的原生地址；操作系统请求后完成。
  openExternal: (id: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.openExternal, id),
  // openExternalUrl forwards an explicit link; resolves after the main process validates and opens it.
  // openExternalUrl 转发明确链接；主进程校验并打开后完成。
  openExternalUrl: (url: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.openExternalUrl, url),
  select: (id: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.select, id),
  close: (id: string): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.close, id),
  viewport: (rect: unknown): Promise<void> => ipcRenderer.invoke(LOCAL_BROWSER_CHANNELS.viewport, rect),
  subscribeChanged: (handler: (snapshot: unknown) => void): (() => void) => {
    const listener = (_event: IpcRendererEvent, snapshot: unknown): void => handler(snapshot);
    ipcRenderer.on(LOCAL_BROWSER_CHANNELS.changed, listener);
    return () => ipcRenderer.removeListener(LOCAL_BROWSER_CHANNELS.changed, listener);
  },
};
// localFile passes one absolute path per call; the main process decides what the OS may do with it.
// localFile 每次调用传递一个绝对路径；由主进程决定操作系统可对其执行的操作。
const localFile = {
  // Resolve the OS path of a user-provided native File without reading its contents.
  // 解析用户提供的原生 File 对象的操作系统路径，无需读取文件内容。
  pathForFile: (file: File): string => webUtils.getPathForFile(file),
  // inspect resolves with whether the path exists and whether its default program may open it.
  // inspect 返回路径是否存在，以及能否交给默认程序打开。
  inspect: (filePath: string): Promise<unknown> => ipcRenderer.invoke(LOCAL_FILE_CHANNELS.inspect, filePath),
  // openWithDefaultApp resolves after the OS opened a non-runnable file, and rejects otherwise.
  // openWithDefaultApp 在操作系统打开非可运行文件后完成，否则拒绝。
  openWithDefaultApp: (filePath: string): Promise<void> => ipcRenderer.invoke(LOCAL_FILE_CHANNELS.openWithDefaultApp, filePath),
  // showInFolder resolves after the file manager was asked to select the path.
  // showInFolder 在请求文件管理器选中该路径后完成。
  showInFolder: (filePath: string): Promise<void> => ipcRenderer.invoke(LOCAL_FILE_CHANNELS.showInFolder, filePath),
};
contextBridge.exposeInMainWorld('vulcanDesktop', { desktop, service, startup, localBrowser, localFile });
