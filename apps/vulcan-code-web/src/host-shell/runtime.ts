import { isElectronRuntime } from "@/host-shell/electron-runtime";

// isTauriRuntime returns whether the WebUI is hosted by the desktop Tauri shell.
// isTauriRuntime 返回 WebUI 是否运行在 Tauri 桌面壳中。
export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

// ShellHostMarker identifies which client host renders the current document.
// ShellHostMarker 标识当前文档由哪个客户端宿主渲染。
export type ShellHostMarker = "browser" | "electron" | "tauri";

// detectShellHost resolves the current client host without side effects.
// detectShellHost 在无副作用的前提下解析当前客户端宿主。
export function detectShellHost(): ShellHostMarker {
  if (isElectronRuntime()) {
    return "electron";
  }
  return isTauriRuntime() ? "tauri" : "browser";
}

// applyShellHostMarker publishes the detected host on the document element.
// applyShellHostMarker 在文档根元素上发布已检测到的宿主。
//
// Chromium-based hosts need the CSS `-webkit-app-region` property to move a frameless window,
// while Tauri consumes its own `data-tauri-drag-region` attribute and would double-handle the same
// element. Scoping the CSS to the Electron marker keeps exactly one host handling native dragging.
// 基于 Chromium 的宿主需要 CSS `-webkit-app-region` 属性才能移动无边框窗口，而 Tauri 消费自身的
// `data-tauri-drag-region` 属性，会对同一元素重复处理。将 CSS 限定到 Electron 标记可保证恰好只有
// 一个宿主处理原生拖动。
export function applyShellHostMarker(): ShellHostMarker {
  const host = detectShellHost();
  if (typeof document !== "undefined") {
    document.documentElement.dataset.vulcanHost = host;
  }
  return host;
}

// isDesktopShutdownDialogRuntime returns whether the current host owns the durable-close dialog.
// isDesktopShutdownDialogRuntime 返回当前宿主是否拥有耐久关闭弹窗。
//
// The durable close drain belongs to the Tauri service shell only: its tray quit and window close
// both drive the Core shutdown coordinator. The Electron client never requests service shutdown,
// so exposing the dialog there would subscribe to Tauri events that do not exist and would imply
// that closing the GUI stops Core.
// 耐久关闭排空只属于 Tauri 服务壳：其托盘退出与窗口关闭都会驱动 Core 关闭协调器。Electron 客户端
// 绝不请求服务停止，因此在那里暴露该弹窗会订阅并不存在的 Tauri 事件，并暗示关闭 GUI 会停止 Core。
export function isDesktopShutdownDialogRuntime(): boolean {
  return isTauriRuntime();
}
