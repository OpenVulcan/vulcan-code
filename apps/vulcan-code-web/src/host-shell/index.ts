export { createShellClient } from "@/host-shell/client";
export { BrowserShellClient, createBrowserShellClient } from "@/host-shell/browser-shell";
export { ElectronShellClient, createElectronShellClient } from "@/host-shell/electron-shell";
export { isElectronRuntime } from "@/host-shell/electron-runtime";
export { TauriShellClient, createTauriShellClient } from "@/host-shell/tauri-shell";
export type {
  ShellClient,
  ShellKind,
  ShellNotificationActivation,
  ShellNotificationCapability,
  ShellNotificationPermission,
  ShellNotificationPresentationState,
  ShellNotificationRequest,
} from "@/host-shell/protocol";
