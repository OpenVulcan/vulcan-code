import { readElectronDesktopBridge } from "@/host-shell/electron-bridge";

// isElectronRuntime returns whether the WebUI is hosted by the Electron desktop shell.
// isElectronRuntime 返回 WebUI 是否运行在 Electron 桌面壳中。
export function isElectronRuntime(): boolean {
  return typeof window !== "undefined" && readElectronDesktopBridge() !== null;
}
