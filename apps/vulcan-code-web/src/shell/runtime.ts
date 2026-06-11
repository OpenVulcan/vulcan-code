// isTauriRuntime returns whether the WebUI is hosted by the desktop Tauri shell.
// isTauriRuntime 返回 WebUI 是否运行在 Tauri 桌面壳中。
export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
