export { createShellClient } from "@/shell/client";
export { BrowserShellClient, createBrowserShellClient } from "@/shell/browserShell";
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
