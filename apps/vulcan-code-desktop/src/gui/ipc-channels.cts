// IPC channel names shared by the GUI main process and its isolated preload.
// GUI 主进程与其隔离预加载共享的 IPC 通道名。

// Service channels expose only the instance selected by the owning GUI.
// 服务通道仅暴露所属 GUI 选中的实例。
export const SERVICE_CHANNELS = {
  // Returns the ready Core connection after validating its discovery identity.
  // 校验发现身份后返回就绪 Core 的连接。
  connection: 'vulcan:service:connection',
} as const;

// Local browser channels are scoped to the current GUI and never address a Core browser session.
// 本地浏览器通道限定于当前 GUI，绝不寻址 Core 浏览器会话。
export const LOCAL_BROWSER_CHANNELS = {
  // Resets native pages before a new workbench renderer mounts.
  // 在新的工作台渲染界面挂载前重置原生页面。
  initialize: 'vulcan:local-browser:initialize',
  // Selects the conversation whose native tabs this GUI window may display.
  // 选择此 GUI 窗口可显示其原生标签的会话。
  setOwner: 'vulcan:local-browser:set-owner',
  // Returns the current native temporary-tab projection.
  // 返回当前原生临时标签投影。
  snapshot: 'vulcan:local-browser:snapshot',
  // Creates a sandboxed temporary tab.
  // 创建一个沙箱临时标签。
  open: 'vulcan:local-browser:open',
  // Navigates one existing temporary tab.
  // 导航一个已有临时标签。
  navigate: 'vulcan:local-browser:navigate',
  // Executes a native history or loading action for one page.
  // 为单个页面执行原生历史或加载操作。
  history: 'vulcan:local-browser:history',
  // Moves a tab to a sibling's position.
  // 将标签移动到同级标签的位置。
  move: 'vulcan:local-browser:move',
  // Opens page DevTools in a detached window.
  // 在独立窗口打开页面开发者工具。
  openDeveloperTools: 'vulcan:local-browser:devtools',
  // Lets the user point at one page element and returns it with an annotated screenshot.
  // 让用户指向一个页面元素，并返回该元素及带标注的截图。
  pickElement: 'vulcan:local-browser:pick-element',
  // Ends a running element pick.
  // 结束进行中的元素选择。
  cancelElementPick: 'vulcan:local-browser:cancel-element-pick',
  // Opens the actual page URL in the default browser.
  // 在默认浏览器打开实际页面地址。
  openExternal: 'vulcan:local-browser:external',
  // Opens an explicit conversation URL without creating an embedded tab.
  // 打开明确的会话 URL，不创建内置标签。
  openExternalUrl: 'vulcan:local-browser:external-url',
  // Selects one existing temporary tab.
  // 选中一个已有临时标签。
  select: 'vulcan:local-browser:select',
  // Closes one existing temporary tab.
  // 关闭一个已有临时标签。
  close: 'vulcan:local-browser:close',
  // Confirms and releases every native page owned by one conversation.
  // 确认并释放一个会话持有的全部原生页面。
  closeOwner: 'vulcan:local-browser:close-owner',
  // Updates the visible sidebar rectangle or hides the native view.
  // 更新可见侧栏矩形，或隐藏原生视图。
  viewport: 'vulcan:local-browser:viewport',
  // Publishes a changed temporary-tab projection.
  // 发布已变化的临时标签投影。
  changed: 'vulcan:local-browser:changed',
} as const;

/** Renderer-to-main window command channels. */
/** 渲染进程到主进程的窗口命令通道。 */
export const WINDOW_COMMAND_CHANNELS = {
  /** Opens the workbench window's developer tools. */
  /** 打开工作台窗口的开发者工具。 */
  openDeveloperTools: 'vulcan:window:open-developer-tools',
  /** Minimizes the current window. */
  /** 最小化当前窗口。 */
  minimize: 'vulcan:window:minimize',
  /** Toggles the current window maximize state. */
  /** 切换当前窗口最大化状态。 */
  toggleMaximize: 'vulcan:window:toggle-maximize',
  /** Reports the current window maximize state. */
  /** 报告当前窗口最大化状态。 */
  isMaximized: 'vulcan:window:is-maximized',
  /** Reports the current window minimized state. */
  /** 报告当前窗口最小化状态。 */
  isMinimized: 'vulcan:window:is-minimized',
  /** Reports whether the current window has input focus. */
  /** 报告当前窗口是否拥有输入焦点。 */
  isFocused: 'vulcan:window:is-focused',
  /** Reports whether the current window is visible. */
  /** 报告当前窗口是否可见。 */
  isVisible: 'vulcan:window:is-visible',
  /** Closes the current window without stopping Core. */
  /** 关闭当前窗口，不停止 Core。 */
  close: 'vulcan:window:close',
} as const;

// Clipboard channels expose only plain text to the owning workbench frame.
// 剪贴板通道仅向所属工作台框架开放纯文本。
export const CLIPBOARD_CHANNELS = {
  // Reads the current plain-text clipboard contents.
  // 读取当前剪贴板纯文本内容。
  readText: 'vulcan:clipboard:read-text',
  // Writes plain text provided by the current workbench.
  // 写入当前工作台提供的纯文本。
  writeText: 'vulcan:clipboard:write-text',
  // Dispatches the native Paste command into the focused workbench input.
  // 向当前聚焦的工作台输入框派发原生粘贴命令。
  paste: 'vulcan:clipboard:paste',
} as const;

/** Main-to-renderer window event channels. */
/** 主进程到渲染进程的窗口事件通道。 */
export const WINDOW_EVENT_CHANNELS = {
  /** Emitted after the window bounds changed. */
  /** 窗口边界变化后发出。 */
  resized: 'vulcan:window:resized',
  /** Emitted after the window focus state changed. */
  /** 窗口焦点状态变化后发出。 */
  focusChanged: 'vulcan:window:focus-changed',
} as const;

/** Renderer-to-main notification channels. */
/** 渲染进程到主进程的通知通道。 */
export const NOTIFICATION_CHANNELS = {
  /** Reports native notification permission. */
  /** 报告原生通知权限。 */
  permission: 'vulcan:notification:permission',
  /** Requests native notification permission. */
  /** 请求原生通知权限。 */
  requestPermission: 'vulcan:notification:request-permission',
  /** Presents one notification. */
  /** 展示一条通知。 */
  show: 'vulcan:notification:show',
} as const;

/** Main-to-renderer notification event channels. */
/** 主进程到渲染进程的通知事件通道。 */
export const NOTIFICATION_EVENT_CHANNELS = {
  /** Emitted when the user activates a presented notification. */
  /** 用户激活已展示通知时发出。 */
  activated: 'vulcan:notification:activated',
} as const;
