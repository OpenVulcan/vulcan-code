import type { ShellClient } from "@/shell/protocol";

// BrowserShellClient implements host actions for ordinary browser clients.
// BrowserShellClient 为普通浏览器客户端实现宿主动作。
export class BrowserShellClient implements ShellClient {
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
