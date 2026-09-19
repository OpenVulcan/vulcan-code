import { readElectronDesktopBridge, type ElectronDesktopBridge } from "@/shell/electronBridge";
import type {
  ShellClient,
  ShellNotificationActivation,
  ShellNotificationCapability,
  ShellNotificationPermission,
  ShellNotificationPresentationState,
  ShellNotificationRequest,
} from "@/shell/protocol";

// ElectronShellClient implements client-host actions through the trusted Electron preload bridge.
// ElectronShellClient 通过受信 Electron 预加载桥实现客户端宿主动作。
export class ElectronShellClient implements ShellClient {
  // bridge caches the trusted preload bridge resolved at construction time.
  // bridge 缓存构造时解析的受信预加载桥。
  private readonly bridge: ElectronDesktopBridge | null = readElectronDesktopBridge();

  // openDeveloperTools exists only when this exact preload supports the new native action.
  // openDeveloperTools 仅在当前预加载桥支持新增原生操作时存在。
  readonly openDeveloperTools?: () => Promise<void>;

  // constructor binds the optional developer-tools action to the trusted preload bridge.
  // constructor 将可选开发者工具操作绑定到可信预加载桥。
  constructor() {
    // open is captured from the exact bridge version installed before this renderer starts.
    // open 从此渲染器启动前安装的精确预加载桥版本中取得。
    const open = this.bridge?.openDeveloperTools;
    if (open) {
      this.openDeveloperTools = () => open();
    }
  }

  // requireBridge returns the trusted bridge or reports the missing host.
  // requireBridge 返回受信桥，缺失时报告宿主缺失。
  private requireBridge(): ElectronDesktopBridge {
    if (!this.bridge) {
      throw new Error("Electron desktop bridge is unavailable.");
    }
    return this.bridge;
  }

  // getNotificationCapability reports native notification support in the desktop host.
  // getNotificationCapability 报告桌面宿主中的原生通知支持。
  getNotificationCapability(): ShellNotificationCapability {
    return {
      shellKind: "electron",
      supported: this.bridge?.nativeNotificationsAvailable === true,
      activationSupported: this.bridge?.nativeNotificationsAvailable === true,
    };
  }

  // getNotificationPermission reads the native authorization without prompting.
  // getNotificationPermission 读取原生授权状态，且不提示用户。
  async getNotificationPermission(): Promise<ShellNotificationPermission> {
    if (!this.bridge) {
      return "unsupported";
    }
    return this.bridge.getNotificationPermission();
  }

  // requestNotificationPermission delegates to the native host after an explicit user action.
  // requestNotificationPermission 仅在明确用户操作后委托原生宿主请求权限。
  async requestNotificationPermission(): Promise<ShellNotificationPermission> {
    if (!this.bridge) {
      return "unsupported";
    }
    return this.bridge.requestNotificationPermission();
  }

  // getNotificationPresentationState reads authoritative native window facts.
  // getNotificationPresentationState 读取权威的原生窗口事实。
  async getNotificationPresentationState(): Promise<ShellNotificationPresentationState> {
    const bridge = this.requireBridge();
    const [minimized, focused, visible] = await Promise.all([
      bridge.isWindowMinimized(),
      bridge.isWindowFocused(),
      bridge.isWindowVisible(),
    ]);
    return {
      foreground: visible && !minimized,
      focused,
      visible,
    };
  }

  // subscribeNotificationPresentationChange listens to native focus and resize transitions.
  // subscribeNotificationPresentationChange 监听原生焦点与尺寸变化。
  async subscribeNotificationPresentationChange(handler: () => void): Promise<() => void> {
    const bridge = this.requireBridge();
    const [releaseFocus, releaseResize] = await Promise.all([
      bridge.subscribeWindowFocusChange(handler),
      bridge.subscribeWindowResize(handler),
    ]);
    return () => {
      releaseFocus();
      releaseResize();
    };
  }

  // showLocalNotification presents one notification through the native host.
  // showLocalNotification 通过原生宿主展示一条通知。
  async showLocalNotification(request: ShellNotificationRequest): Promise<void> {
    await this.requireBridge().showNotification(request);
  }

  // subscribeNotificationActivation observes native notification clicks.
  // subscribeNotificationActivation 观察原生通知点击。
  async subscribeNotificationActivation(
    handler: (activation: ShellNotificationActivation) => void,
  ): Promise<() => void> {
    return this.requireBridge().subscribeNotificationActivation(handler);
  }

  // supportsWindowControls reports that Electron exposes native window controls.
  // supportsWindowControls 表示 Electron 可提供原生窗口控制。
  supportsWindowControls(): boolean {
    return this.bridge !== null;
  }

  // isWindowMaximized returns the current native maximize state.
  // isWindowMaximized 返回当前原生最大化状态。
  async isWindowMaximized(): Promise<boolean> {
    return this.requireBridge().isWindowMaximized();
  }

  // minimizeWindow minimizes the current native window.
  // minimizeWindow 最小化当前原生窗口。
  async minimizeWindow(): Promise<void> {
    await this.requireBridge().minimizeWindow();
  }

  // toggleWindowMaximize toggles maximize or restore on the current native window.
  // toggleWindowMaximize 在当前原生窗口上切换最大化或还原。
  async toggleWindowMaximize(): Promise<void> {
    await this.requireBridge().toggleWindowMaximize();
  }

  // closeWindow closes the current GUI window without stopping Core.
  // closeWindow 关闭当前 GUI 窗口，不停止 Core。
  async closeWindow(): Promise<void> {
    await this.requireBridge().closeWindow();
  }

  // subscribeWindowResize listens for native resize events.
  // subscribeWindowResize 监听原生窗口尺寸变化事件。
  async subscribeWindowResize(handler: () => void): Promise<() => void> {
    return this.requireBridge().subscribeWindowResize(handler);
  }
}

// createElectronShellClient creates one Electron-backed shell facade.
// createElectronShellClient 创建 Electron 支持的壳门面。
export function createElectronShellClient(): ElectronShellClient {
  return new ElectronShellClient();
}
