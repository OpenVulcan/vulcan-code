import type { ShellClient } from "../../src/shell/protocol";

// rejected forbids native effects while keeping every real shell control reachable for failure inspection.
// rejected 禁止原生副作用，同时保持每个真实壳控件可进入失败检查。
async function rejected(): Promise<never> { throw new Error("UI_AUDIT_NATIVE_ACTION_DISABLED"); }
// subscribe owns no OS resource and supplies the same unsubscribe lifecycle as the production shell.
// subscribe 不持有操作系统资源，并提供与生产壳一致的退订生命周期。
async function subscribe(): Promise<() => void> { return () => undefined; }
// shellClient is an explicit virtual browser host: no permission prompts, real notifications or window actions.
// shellClient 是明确的虚拟浏览器宿主：不申请权限、不发真实通知、不执行窗口动作。
export const shellClient: ShellClient = {
  getNotificationCapability: () => ({ shellKind: "browser", supported: true, activationSupported: false }),
  getNotificationPermission: async () => "denied", requestNotificationPermission: rejected,
  getNotificationPresentationState: async () => ({ foreground: true, focused: true, visible: true }),
  subscribeNotificationPresentationChange: subscribe, subscribeNotificationActivation: subscribe,
  showLocalNotification: rejected, supportsWindowControls: () => false, isWindowMaximized: async () => false,
  minimizeWindow: rejected, toggleWindowMaximize: rejected, closeWindow: rejected, subscribeWindowResize: subscribe,
};
// createShellClient returns the single virtual host for both direct chrome imports and shared service imports.
// createShellClient 为直接外壳导入与共享服务导入返回同一个虚拟宿主。
export function createShellClient(): ShellClient { return shellClient; }
