import type {
  ShellClient,
  ShellNotificationActivation,
  ShellNotificationPermission,
  ShellNotificationPresentationState,
  ShellNotificationRequest,
} from "@/host-shell";

// NotificationTestShell provides one configurable ShellClient test boundary without browser or Tauri globals.
// NotificationTestShell 提供一个不依赖浏览器或 Tauri 全局对象的可配置 ShellClient 测试边界。
export class NotificationTestShell implements ShellClient {
  // permission is the exact authorization result returned to notification consumers.
  // permission 是返回给通知消费者的精确授权结果。
  permission: ShellNotificationPermission = "granted";
  // presentationState is the exact foreground, focus, and visibility snapshot returned to policy.
  // presentationState 是返回给策略的精确前台、焦点与可见性快照。
  presentationState: ShellNotificationPresentationState = {
    foreground: false,
    focused: false,
    visible: false,
  };
  // requests stores every displayed local notification.
  // requests 保存每条已展示的本地通知。
  readonly requests: ShellNotificationRequest[] = [];
  // activationSubscriptionAttempts counts exact activation subscription attempts.
  // activationSubscriptionAttempts 统计精确激活订阅尝试次数。
  activationSubscriptionAttempts = 0;
  // failNextActivationSubscription requests one deterministic transient subscription failure.
  // failNextActivationSubscription 请求一次确定的瞬时订阅失败。
  failNextActivationSubscription = false;
  // activationHandler stores the current coordinator activation subscriber.
  // activationHandler 保存当前协调器激活订阅者。
  private activationHandler: ((activation: ShellNotificationActivation) => void) | null = null;

  // getNotificationCapability reports deterministic supported browser-style capability.
  // getNotificationCapability 报告确定的浏览器式支持能力。
  getNotificationCapability() {
    return { shellKind: "browser" as const, supported: true, activationSupported: true };
  }

  // getNotificationPermission returns the configured authorization state without prompting.
  // getNotificationPermission 无提示返回已配置授权状态。
  async getNotificationPermission() {
    return this.permission;
  }

  // requestNotificationPermission returns the configured user-request result.
  // requestNotificationPermission 返回已配置的用户请求结果。
  async requestNotificationPermission() {
    return this.permission;
  }

  // getNotificationPresentationState returns the configured client presentation facts.
  // getNotificationPresentationState 返回已配置客户端界面事实。
  async getNotificationPresentationState() {
    return this.presentationState;
  }

  // subscribeNotificationPresentationChange returns an exact no-op cleanup callback.
  // subscribeNotificationPresentationChange 返回精确的空操作清理回调。
  async subscribeNotificationPresentationChange(_handler: () => void) {
    return () => {};
  }

  // showLocalNotification records one local display request.
  // showLocalNotification 记录一个本地展示请求。
  async showLocalNotification(request: ShellNotificationRequest) {
    this.requests.push(request);
  }

  // subscribeNotificationActivation records the coordinator activation handler or raises the requested transient failure.
  // subscribeNotificationActivation 记录协调器激活处理器，或抛出已请求的瞬时失败。
  async subscribeNotificationActivation(handler: (activation: ShellNotificationActivation) => void) {
    this.activationSubscriptionAttempts += 1;
    if (this.failNextActivationSubscription) {
      this.failNextActivationSubscription = false;
      throw new Error("transient activation failure");
    }
    this.activationHandler = handler;
    return () => {
      this.activationHandler = null;
    };
  }

  // activate dispatches one correlated notification click to the current subscriber.
  // activate 向当前订阅者分发一次有关联的通知点击。
  activate(notificationId: string): void {
    this.activationHandler?.({ notificationId });
  }

  // supportsWindowControls reports no native controls for notification tests.
  // supportsWindowControls 报告通知测试不提供原生窗口控制。
  supportsWindowControls(): boolean { return false; }
  // isWindowMaximized returns the deterministic test window state.
  // isWindowMaximized 返回确定的测试窗口状态。
  async isWindowMaximized(): Promise<boolean> { return false; }
  // minimizeWindow performs no action in notification tests.
  // minimizeWindow 在通知测试中不执行操作。
  async minimizeWindow(): Promise<void> {}
  // toggleWindowMaximize performs no action in notification tests.
  // toggleWindowMaximize 在通知测试中不执行操作。
  async toggleWindowMaximize(): Promise<void> {}
  // isWindowFullScreen returns the deterministic test window state.
  // isWindowFullScreen 返回确定的测试窗口状态。
  async isWindowFullScreen(): Promise<boolean> { return false; }
  // toggleWindowFullScreen performs no action in notification tests.
  // toggleWindowFullScreen 在通知测试中不执行操作。
  async toggleWindowFullScreen(): Promise<void> {}
  // placeNativeWindowButtons reports no native buttons in notification tests.
  // placeNativeWindowButtons 在通知测试中报告没有原生按钮。
  async placeNativeWindowButtons(_placement: { left: number; height: number }): Promise<number | null> { return null; }
  // closeWindow performs no action in notification tests.
  // closeWindow 在通知测试中不执行操作。
  async closeWindow(): Promise<void> {}
  // subscribeWindowResize returns an exact no-op cleanup callback.
  // subscribeWindowResize 返回精确的空操作清理回调。
  async subscribeWindowResize(_handler: () => void): Promise<() => void> { return () => {}; }
}
