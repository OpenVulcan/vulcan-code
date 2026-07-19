import type {
  ShellClient,
  ShellNotificationActivation,
  ShellNotificationCapability,
  ShellNotificationPermission,
  ShellNotificationPresentationState,
  ShellNotificationRequest,
} from "@/shell/protocol";

// BrowserShellClient implements host actions for ordinary browser clients.
// BrowserShellClient 为普通浏览器客户端实现宿主动作。
export class BrowserShellClient implements ShellClient {
  // activationHandlers stores browser notification click subscribers for this shell instance.
  // activationHandlers 保存当前壳实例的浏览器通知点击订阅者。
  private readonly activationHandlers = new Set<(activation: ShellNotificationActivation) => void>();

  // getNotificationCapability reports standard Notification API availability.
  // getNotificationCapability 报告标准 Notification API 的可用性。
  getNotificationCapability(): ShellNotificationCapability {
    return {
      shellKind: "browser",
      supported: typeof Notification !== "undefined",
      activationSupported: typeof Notification !== "undefined",
    };
  }

  // getNotificationPermission reads browser authorization without opening a prompt.
  // getNotificationPermission 读取浏览器授权状态，且不打开提示。
  async getNotificationPermission(): Promise<ShellNotificationPermission> {
    if (typeof Notification === "undefined") {
      return "unsupported";
    }
    return Notification.permission;
  }

  // requestNotificationPermission delegates to the browser only after a settings-page user action.
  // requestNotificationPermission 仅在设置页用户操作后委托浏览器请求权限。
  async requestNotificationPermission(): Promise<ShellNotificationPermission> {
    if (typeof Notification === "undefined") {
      return "unsupported";
    }
    return Notification.requestPermission();
  }

  // getNotificationPresentationState reads the current page visibility and window focus.
  // getNotificationPresentationState 读取当前页面可见性与窗口焦点。
  async getNotificationPresentationState(): Promise<ShellNotificationPresentationState> {
    // visible is the standard tab visibility fact used by browser notification policy.
    // visible 是浏览器通知策略使用的标准标签页可见性事实。
    const visible = document.visibilityState === "visible";
    return {
      foreground: visible,
      focused: document.hasFocus(),
      visible,
    };
  }

  // subscribeNotificationPresentationChange listens to browser focus and tab visibility transitions.
  // subscribeNotificationPresentationChange 监听浏览器焦点与标签页可见性变化。
  async subscribeNotificationPresentationChange(handler: () => void): Promise<() => void> {
    window.addEventListener("focus", handler);
    window.addEventListener("blur", handler);
    document.addEventListener("visibilitychange", handler);
    return () => {
      window.removeEventListener("focus", handler);
      window.removeEventListener("blur", handler);
      document.removeEventListener("visibilitychange", handler);
    };
  }

  // showLocalNotification presents one standard browser notification and forwards optional activation.
  // showLocalNotification 展示一条标准浏览器通知并转发可选激活事件。
  async showLocalNotification(request: ShellNotificationRequest): Promise<void> {
    if (typeof Notification === "undefined") {
      throw new Error("Browser notifications are not supported.");
    }
    // notification is the concrete browser-owned notification object.
    // notification 是浏览器拥有的具体通知对象。
    const notification = new Notification(request.title, {
      body: request.body,
      tag: request.id,
    });
    notification.onclick = () => {
      // activation carries only shell correlation data; business routing remains outside the shell.
      // activation 仅携带壳关联数据；业务路由仍位于壳之外。
      const activation: ShellNotificationActivation = { notificationId: request.id };
      for (const activationHandler of this.activationHandlers) {
        activationHandler(activation);
      }
      window.focus();
      notification.close();
    };
  }

  // subscribeNotificationActivation registers one browser notification click observer.
  // subscribeNotificationActivation 注册一个浏览器通知点击观察者。
  async subscribeNotificationActivation(
    handler: (activation: ShellNotificationActivation) => void,
  ): Promise<() => void> {
    this.activationHandlers.add(handler);
    return () => {
      this.activationHandlers.delete(handler);
    };
  }

  // supportsWindowControls reports that native desktop buttons are unavailable in browsers.
  // supportsWindowControls 表示浏览器中不可用原生桌面窗口按钮。
  supportsWindowControls(): boolean {
    return false;
  }

  // isWindowMaximized always returns false for browser hosts.
  // isWindowMaximized 在浏览器宿主中始终返回 false。
  async isWindowMaximized(): Promise<boolean> {
    return false;
  }

  // minimizeWindow is a no-op because browsers cannot minimize their host window.
  // minimizeWindow 是空操作，因为浏览器无法最小化宿主窗口。
  async minimizeWindow(): Promise<void> {}

  // toggleWindowMaximize is a no-op because browsers cannot toggle their host window.
  // toggleWindowMaximize 是空操作，因为浏览器无法切换宿主窗口状态。
  async toggleWindowMaximize(): Promise<void> {}

  // closeWindow is a no-op because browser pages should not close their host window implicitly.
  // closeWindow 是空操作，因为浏览器页面不应隐式关闭宿主窗口。
  async closeWindow(): Promise<void> {}

  // subscribeWindowResize returns an empty unsubscribe function for browser hosts.
  // subscribeWindowResize 在浏览器宿主中返回空取消订阅函数。
  async subscribeWindowResize(_handler: () => void): Promise<() => void> {
    return () => {};
  }
}

// createBrowserShellClient creates one ordinary browser shell facade.
// createBrowserShellClient 创建普通浏览器壳门面。
export function createBrowserShellClient(): BrowserShellClient {
  return new BrowserShellClient();
}
