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
  // Returns the current native temporary-tab projection.
  // 返回当前原生临时标签投影。
  snapshot: 'vulcan:local-browser:snapshot',
  // Creates a sandboxed temporary tab.
  // 创建一个沙箱临时标签。
  open: 'vulcan:local-browser:open',
  // Navigates one existing temporary tab.
  // 导航一个已有临时标签。
  navigate: 'vulcan:local-browser:navigate',
  // Selects one existing temporary tab.
  // 选中一个已有临时标签。
  select: 'vulcan:local-browser:select',
  // Closes one existing temporary tab.
  // 关闭一个已有临时标签。
  close: 'vulcan:local-browser:close',
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
