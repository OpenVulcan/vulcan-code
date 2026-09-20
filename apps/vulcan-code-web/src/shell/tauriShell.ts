import type {
  ShellClient,
  ShellNotificationCapability,
  ShellNotificationPermission,
  ShellNotificationPresentationState,
  ShellNotificationRequest,
} from "@/shell/protocol";

// TauriWindowHandle exposes only the window APIs used by the custom title bar.
// TauriWindowHandle 仅暴露自定义标题栏使用的窗口 API。
interface TauriWindowHandle {
  // Minimizes the current desktop window.
  // 最小化当前桌面窗口。
  minimize: () => Promise<void>;
  // Toggles the current desktop window maximized state.
  // 切换当前桌面窗口最大化状态。
  toggleMaximize: () => Promise<void>;
  // Returns whether the current desktop window is maximized.
  // 返回当前桌面窗口是否处于最大化状态。
  isMaximized: () => Promise<boolean>;
  // Closes the current desktop window.
  // 关闭当前桌面窗口。
  close: () => Promise<void>;
  // Subscribes to native window resize events.
  // 订阅原生窗口尺寸变化事件。
  onResized: (handler: () => void) => Promise<() => void>;
  // Returns whether the current desktop window is minimized.
  // 返回当前桌面窗口是否已最小化。
  isMinimized: () => Promise<boolean>;
  // Returns whether the current desktop window has input focus.
  // 返回当前桌面窗口是否拥有输入焦点。
  isFocused: () => Promise<boolean>;
  // Returns whether the current desktop window is visible.
  // 返回当前桌面窗口是否可见。
  isVisible: () => Promise<boolean>;
  // Subscribes to native window focus changes.
  // 订阅原生窗口焦点变化。
  onFocusChanged: (handler: () => void) => Promise<() => void>;
}

// TauriNotificationApi exposes only native command calls used by the desktop shell facade.
// TauriNotificationApi 仅暴露桌面壳门面使用的原生命令调用。
interface TauriNotificationApi {
  // isPermissionGranted checks current desktop notification authorization.
  // isPermissionGranted 检查当前桌面通知授权。
  isPermissionGranted: () => Promise<boolean | null>;
  // requestPermission requests desktop notification authorization from a user action.
  // requestPermission 通过用户操作请求桌面通知授权。
  requestPermission: () => Promise<NotificationPermission>;
  // sendNotification presents one native desktop notification.
  // sendNotification 展示一条原生桌面通知。
  sendNotification: (options: { title: string; body: string }) => Promise<void>;
}

// TauriShellClient implements client-host actions through Tauri APIs.
// TauriShellClient 通过 Tauri API 实现客户端宿主动作。
export class TauriShellClient implements ShellClient {
  // windowPromise caches the dynamic current-window handle import.
  // windowPromise 缓存动态当前窗口句柄导入。
  private windowPromise: Promise<TauriWindowHandle> | null = null;
  // notificationApiPromise caches the native notification command import.
  // notificationApiPromise 缓存原生通知命令导入。
  private notificationApiPromise: Promise<TauriNotificationApi> | null = null;

  // getNotificationCapability reports native notification support provided by the desktop commands.
  // getNotificationCapability 报告桌面命令提供的原生通知支持。
  getNotificationCapability(): ShellNotificationCapability {
    return { shellKind: "tauri", supported: true, activationSupported: false };
  }

  // getNotificationPermission checks native authorization without prompting the user.
  // getNotificationPermission 检查原生授权状态，且不提示用户。
  async getNotificationPermission(): Promise<ShellNotificationPermission> {
    // granted is the authoritative positive permission fact exposed by the native shell.
    // granted 是原生壳暴露的权威肯定权限事实。
    const granted = await (await this.notificationApi()).isPermissionGranted();
    return granted === null ? "default" : granted ? "granted" : "denied";
  }

  // requestNotificationPermission requests native authorization at enabled startup or from settings.
  // requestNotificationPermission 在已启用的启动流程或设置操作时请求原生授权。
  async requestNotificationPermission(): Promise<ShellNotificationPermission> {
    // permission returns the current native registration and OS authorization result.
    // permission 返回当前原生注册和操作系统授权结果。
    const permission = await (await this.notificationApi()).requestPermission();
    return permission;
  }

  // getNotificationPresentationState reads native minimized, visibility, and focus facts.
  // getNotificationPresentationState 读取原生最小化、可见性与焦点事实。
  async getNotificationPresentationState(): Promise<ShellNotificationPresentationState> {
    // windowHandle is the single native window queried for one coherent presentation snapshot.
    // windowHandle 是用于一次一致界面快照查询的唯一原生窗口。
    const windowHandle = await this.currentWindow();
    // stateFacts resolves independent native facts concurrently without changing their ownership.
    // stateFacts 并发解析独立原生事实，且不改变其归属。
    const [minimized, focused, visible] = await Promise.all([
      windowHandle.isMinimized(),
      windowHandle.isFocused(),
      windowHandle.isVisible(),
    ]);
    return { foreground: visible && !minimized, focused, visible };
  }

  // subscribeNotificationPresentationChange listens to native focus and size transitions.
  // subscribeNotificationPresentationChange 监听原生焦点与尺寸变化。
  async subscribeNotificationPresentationChange(handler: () => void): Promise<() => void> {
    // windowHandle owns both native subscriptions and their cleanup functions.
    // windowHandle 持有两个原生订阅及其清理函数。
    const windowHandle = await this.currentWindow();
    // unlisteners are exact cleanup callbacks returned by Tauri.
    // unlisteners 是 Tauri 返回的精确清理回调。
    const unlisteners = await Promise.all([
      windowHandle.onFocusChanged(handler),
      windowHandle.onResized(handler),
    ]);
    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }

  // showLocalNotification submits localized text and propagates native submission failures.
  // showLocalNotification 提交已本地化文案，并传播原生提交失败。
  async showLocalNotification(request: ShellNotificationRequest): Promise<void> {
    await (await this.notificationApi()).sendNotification({ title: request.title, body: request.body });
  }

  // supportsWindowControls reports that Tauri exposes native window controls.
  // supportsWindowControls 表示 Tauri 可提供原生窗口控制。
  supportsWindowControls(): boolean {
    return true;
  }

  // isWindowMaximized returns the current native maximize state.
  // isWindowMaximized 返回当前原生最大化状态。
  async isWindowMaximized(): Promise<boolean> {
    return this.currentWindow().then((windowHandle) => windowHandle.isMaximized());
  }

  // minimizeWindow minimizes the current native window.
  // minimizeWindow 最小化当前原生窗口。
  async minimizeWindow(): Promise<void> {
    await (await this.currentWindow()).minimize();
  }

  // toggleWindowMaximize toggles maximize or restore on the current native window.
  // toggleWindowMaximize 在当前原生窗口上切换最大化或还原。
  async toggleWindowMaximize(): Promise<void> {
    await (await this.currentWindow()).toggleMaximize();
  }

  // closeWindow closes the current native window.
  // closeWindow 关闭当前原生窗口。
  async closeWindow(): Promise<void> {
    await (await this.currentWindow()).close();
  }

  // subscribeWindowResize listens for native resize events.
  // subscribeWindowResize 监听原生窗口尺寸变化事件。
  async subscribeWindowResize(handler: () => void): Promise<() => void> {
    return (await this.currentWindow()).onResized(handler);
  }

  // currentWindow loads and returns the current Tauri window handle.
  // currentWindow 加载并返回当前 Tauri 窗口句柄。
  private async currentWindow(): Promise<TauriWindowHandle> {
    if (!this.windowPromise) {
      this.windowPromise = import("@tauri-apps/api/window").then((api) => api.getCurrentWindow() as TauriWindowHandle);
    }
    return this.windowPromise;
  }

  // notificationApi loads the native notification command facade and returns its asynchronous methods.
  // notificationApi 加载原生通知命令门面，并返回其异步方法。
  private async notificationApi(): Promise<TauriNotificationApi> {
    if (!this.notificationApiPromise) {
      // The 2.3.3 JS wrapper reads a Windows WebView permission cache and discards notify promises.
      // 2.3.3 的 JS 包装读取 Windows WebView 权限缓存，并丢弃通知调用的 Promise。
      // Dedicated desktop commands preserve application identity, native policy, and submission failures.
      // 专用桌面命令保留应用身份、原生策略和通知提交失败。
      this.notificationApiPromise = import("@tauri-apps/api/core").then(({ invoke }) => ({
        isPermissionGranted: () => invoke<boolean | null>("notification_permission"),
        requestPermission: async () => {
          // permission is the exact PermissionState returned by the registered Tauri command.
          // permission 是已注册 Tauri 命令返回的精确 PermissionState。
          const permission = await invoke<"granted" | "denied" | "prompt" | "prompt-with-rationale">("request_notification_permission");
          return permission === "prompt" || permission === "prompt-with-rationale" ? "default" : permission;
        },
        sendNotification: (options) => invoke<void>("show_notification", { options }),
      }));
    }
    return this.notificationApiPromise;
  }
}

// createTauriShellClient creates one Tauri-backed shell facade.
// createTauriShellClient 创建 Tauri 支持的壳门面。
export function createTauriShellClient(): TauriShellClient {
  return new TauriShellClient();
}
