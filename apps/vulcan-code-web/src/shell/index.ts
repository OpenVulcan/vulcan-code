export { createShellClient } from "@/shell/client";
export { BrowserShellClient, createBrowserShellClient } from "@/shell/browserShell";
export { ElectronShellClient, createElectronShellClient } from "@/shell/electronShell";
export { isElectronRuntime } from "@/shell/electronRuntime";
export { TauriShellClient, createTauriShellClient } from "@/shell/tauriShell";
export type {
  ShellClient,
  ShellKind,
  ShellNotificationActivation,
  ShellNotificationCapability,
  ShellNotificationPermission,
  ShellNotificationPresentationState,
  ShellNotificationRequest,
} from "@/shell/protocol";
