// ShellKind identifies the client host without coupling business events to an operating system.
// ShellKind 标识客户端宿主，且不把业务事件耦合到操作系统。
export type ShellKind = "browser" | "tauri" | "electron" | "mobile";

// ShellNotificationPermission normalizes notification authorization across client hosts.
// ShellNotificationPermission 统一不同客户端宿主的通知授权状态。
export type ShellNotificationPermission = "unsupported" | "default" | "granted" | "denied";

// ShellNotificationCapability describes local notification support in one host.
// ShellNotificationCapability 描述单个宿主中的本地通知支持能力。
export interface ShellNotificationCapability {
  // shellKind identifies the current client host.
  // shellKind 标识当前客户端宿主。
  shellKind: ShellKind;
  // supported indicates whether local system notifications are available.
  // supported 表示本地系统通知是否可用。
  supported: boolean;
  // activationSupported indicates whether notification clicks can be observed.
  // activationSupported 表示是否可以观察通知点击。
  activationSupported: boolean;
}

// ShellNotificationPresentationState contains current host focus and visibility facts.
// ShellNotificationPresentationState 包含当前宿主焦点与可见性事实。
export interface ShellNotificationPresentationState {
  // foreground indicates that the host surface is visible and not minimized.
  // foreground 表示宿主界面可见且未最小化。
  foreground: boolean;
  // focused indicates that the host window receives keyboard interaction.
  // focused 表示宿主窗口正在接收键盘交互。
  focused: boolean;
  // visible indicates that the host surface is currently visible.
  // visible 表示宿主界面当前可见。
  visible: boolean;
}

// ShellNotificationRequest contains presentation-only data accepted by local shells.
// ShellNotificationRequest 包含本地壳接收的纯展示数据。
export interface ShellNotificationRequest {
  // id correlates optional activation events with the originating intent.
  // id 将可选激活事件与来源意图关联。
  id: string;
  // title is the localized notification title.
  // title 是本地化通知标题。
  title: string;
  // body is the localized notification body.
  // body 是本地化通知正文。
  body: string;
}

// ShellNotificationActivation describes a local notification click when supported.
// ShellNotificationActivation 描述支持时的本地通知点击。
export interface ShellNotificationActivation {
  // notificationId identifies the displayed notification request.
  // notificationId 标识已展示的通知请求。
  notificationId: string;
}

// ShellClient describes client-host capabilities that are not Core business RPC.
// ShellClient 描述不属于 Core 业务 RPC 的客户端宿主能力。
export interface ShellClient {
  // Opens developer tools only when the current host explicitly exposes that action.
  // 仅在当前宿主明确提供此操作时打开开发者工具。
  openDeveloperTools?: () => Promise<void>;
  // Returns local notification capability metadata for this host.
  // 返回当前宿主的本地通知能力元数据。
  getNotificationCapability(): ShellNotificationCapability;

  // Returns current local notification permission without prompting the user.
  // 返回当前本地通知权限，且不提示用户。
  getNotificationPermission(): Promise<ShellNotificationPermission>;

  // Requests notification permission for enabled startup or an explicit user action.
  // 在已启用的启动流程或明确用户操作时请求通知权限。
  requestNotificationPermission(): Promise<ShellNotificationPermission>;

  // Returns current host foreground, focus, and visibility state.
  // 返回当前宿主的前台、焦点与可见性状态。
  getNotificationPresentationState(): Promise<ShellNotificationPresentationState>;

  // Subscribes to host focus or visibility changes.
  // 订阅宿主焦点或可见性变化。
  subscribeNotificationPresentationChange(handler: () => void): Promise<() => void>;

  // Shows one localized local notification.
  // 展示一条已本地化的本地通知。
  showLocalNotification(request: ShellNotificationRequest): Promise<void>;

  // Optionally subscribes to local notification activation events.
  // 可选订阅本地通知激活事件。
  subscribeNotificationActivation?: (
    handler: (activation: ShellNotificationActivation) => void,
  ) => Promise<() => void>;

  // Returns whether native window controls are available in this host.
  // 返回当前宿主是否支持原生窗口控制。
  supportsWindowControls(): boolean;

  // Returns whether the current native window is maximized.
  // 返回当前原生窗口是否已最大化。
  isWindowMaximized(): Promise<boolean>;

  // Minimizes the current native window when supported.
  // 在支持时最小化当前原生窗口。
  minimizeWindow(): Promise<void>;

  // Toggles the current native window maximize state when supported.
  // 在支持时切换当前原生窗口最大化状态。
  toggleWindowMaximize(): Promise<void>;

  // Returns whether the current native window is in full screen.
  // 返回当前原生窗口是否处于全屏。
  isWindowFullScreen(): Promise<boolean>;

  // Toggles native full screen when supported; the macOS title strip double click uses it like the green button.
  // 在支持时切换原生全屏；macOS 标题条双击以此达到与绿色按钮相同的效果。
  toggleWindowFullScreen(): Promise<void>;

  // Places the operating system's own window buttons (macOS) in the title strip right of `left`, centred in
  // `height` CSS pixels; resolves with their right edge, or null where the host cannot place them. Callers use it
  // only where the operating system draws those buttons; a desktop host without them rejects.
  // 将操作系统自绘的窗口按钮（macOS）放入标题条中 `left` 右侧、在 `height` CSS 像素内垂直居中；返回其右边缘，
  // 宿主无法放置时返回 null。调用方只在操作系统自绘这些按钮时使用；没有这些按钮的桌面宿主会拒绝。
  placeNativeWindowButtons(placement: { left: number; height: number }): Promise<number | null>;

  // Closes the current native window when supported.
  // 在支持时关闭当前原生窗口。
  closeWindow(): Promise<void>;

  // Subscribes to native window resize events.
  // 订阅原生窗口尺寸变化事件。
  subscribeWindowResize(handler: () => void): Promise<() => void>;
}
