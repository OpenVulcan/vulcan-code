import { createBrowserShellClient } from "@/shell/browserShell";
import { createElectronShellClient } from "@/shell/electronShell";
import { isElectronRuntime } from "@/shell/electronRuntime";
import type { ShellClient } from "@/shell/protocol";
import { isTauriRuntime } from "@/shell/runtime";
import { createTauriShellClient } from "@/shell/tauriShell";

// createShellClient chooses the host-specific shell facade for the current runtime.
// createShellClient 为当前运行时选择宿主专用壳门面。
export function createShellClient(): ShellClient {
  if (isElectronRuntime()) {
    return createElectronShellClient();
  }
  return isTauriRuntime() ? createTauriShellClient() : createBrowserShellClient();
}
