import React from "react";
import ReactDOM from "react-dom/client";
import { WebuiApp } from "@/app/webui-app";
import { applyShellHostMarker } from "@/host-shell/runtime";
import { readElectronLocalBrowserBridge, readElectronStartupBridge } from "@/host-shell/electron-bridge";
import { initializeAppearance } from "@/ui-kit/appearance";
import "./boot-styles.css";

// The host marker must be published before the first paint so native drag regions are correct.
// 宿主标记必须在首帧之前发布，使原生拖动区域从一开始就正确。
applyShellHostMarker();
// The persisted theme is applied before the first paint so the startup screen never flashes the light palette.
// 持久化主题在首帧前套用，使启动界面不会先闪现亮色色板。
initializeAppearance();
// The desktop window already played the startup intro on its local startup page, so this startup surface continues it.
// 桌面窗口已在本地启动页播放过启动入场动效，因此本启动层直接接续。
if (readElectronStartupBridge()?.surfaceShown) document.documentElement.dataset.vkStartupContinued = "";

// mountWorkbench resets native view ownership before React can open panels in the new document.
// mountWorkbench 在 React 能打开新文档面板前重置原生视图归属。
// Takes no arguments; resolves after the GUI acknowledges initialization and React is mounted.
// 无参数；GUI 确认初始化且 React 挂载后完成。
async function mountWorkbench(): Promise<void> {
  // localBrowser exists only in the desktop shell and is initialized once per document.
  // localBrowser 仅在桌面壳存在，每个文档只初始化一次。
  const localBrowser = readElectronLocalBrowserBridge();
  if (localBrowser) await localBrowser.initialize();
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <WebuiApp />
    </React.StrictMode>,
  );
}

void mountWorkbench().catch((error: unknown) => {
  // Startup failures remain visible instead of mounting over an uninitialized native surface.
  // 启动失败保持可见，避免在未初始化的原生视图上继续挂载。
  document.getElementById("root")!.textContent = `Vulcan Code failed to start: ${String(error)}`;
});
