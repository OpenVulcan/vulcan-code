import { createBrowserShellClient } from "@/host-shell/browser-shell";
import { createElectronShellClient } from "@/host-shell/electron-shell";
import { isElectronRuntime } from "@/host-shell/electron-runtime";
import type { ShellClient } from "@/host-shell/protocol";
import { isTauriRuntime } from "@/host-shell/runtime";
import { createTauriShellClient } from "@/host-shell/tauri-shell";

// createShellClient chooses the host-specific shell facade for the current runtime.
// createShellClient 为当前运行时选择宿主专用壳门面。
export function createShellClient(): ShellClient {
  if (isElectronRuntime()) {
    return createElectronShellClient();
  }
  return isTauriRuntime() ? createTauriShellClient() : createBrowserShellClient();
}
