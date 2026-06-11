import type { WorkbenchSnapshot } from "@/types";

// ShellClient describes client-host capabilities that are not Core business RPC.
// ShellClient 描述不属于 Core 业务 RPC 的客户端宿主能力。
export interface ShellClient {
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

  // Closes the current native window when supported.
  // 在支持时关闭当前原生窗口。
  closeWindow(): Promise<void>;

  // Subscribes to native window resize events.
  // 订阅原生窗口尺寸变化事件。
  subscribeWindowResize(handler: () => void): Promise<() => void>;

  // Opens a client-local folder picker and registers the selected folder as a project.
  // 打开客户端本地文件夹选择器，并将选中的文件夹注册为项目。
  addProjectFromFolder(): Promise<WorkbenchSnapshot | null>;
}
