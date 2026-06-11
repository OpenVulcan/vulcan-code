import type { ShellClient } from "@/shell/protocol";
import type { WorkbenchSnapshot } from "@/types";

// TauriInvoke invokes a backend command when the app runs inside Tauri.
// TauriInvoke 在应用运行于 Tauri 内部时调用后端命令。
type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

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
}

// TauriShellClient implements client-host actions through Tauri APIs.
// TauriShellClient 通过 Tauri API 实现客户端宿主动作。
export class TauriShellClient implements ShellClient {
  // invokePromise caches the dynamic Tauri invoke import.
  // invokePromise 缓存动态 Tauri invoke 导入。
  private invokePromise: Promise<TauriInvoke> | null = null;

  // windowPromise caches the dynamic current-window handle import.
  // windowPromise 缓存动态当前窗口句柄导入。
  private windowPromise: Promise<TauriWindowHandle> | null = null;

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

  // addProjectFromFolder opens the native folder picker and registers the selected project.
  // addProjectFromFolder 打开原生文件夹选择器并注册选中的项目。
  async addProjectFromFolder(): Promise<WorkbenchSnapshot | null> {
    return (await this.invoke())<WorkbenchSnapshot | null>("service_add_project_from_folder");
  }

  // invoke loads and returns the Tauri invoke function.
  // invoke 加载并返回 Tauri invoke 函数。
  private async invoke(): Promise<TauriInvoke> {
    if (!this.invokePromise) {
      this.invokePromise = import("@tauri-apps/api/core").then((api) => api.invoke as TauriInvoke);
    }
    return this.invokePromise;
  }

  // currentWindow loads and returns the current Tauri window handle.
  // currentWindow 加载并返回当前 Tauri 窗口句柄。
  private async currentWindow(): Promise<TauriWindowHandle> {
    if (!this.windowPromise) {
      this.windowPromise = import("@tauri-apps/api/window").then((api) => api.getCurrentWindow() as TauriWindowHandle);
    }
    return this.windowPromise;
  }
}

// createTauriShellClient creates one Tauri-backed shell facade.
// createTauriShellClient 创建 Tauri 支持的壳门面。
export function createTauriShellClient(): TauriShellClient {
  return new TauriShellClient();
}
