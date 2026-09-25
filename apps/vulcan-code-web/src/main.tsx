import React from "react";
import ReactDOM from "react-dom/client";
import { WebuiApp } from "@/app/webui-app";
import { applyShellHostMarker } from "@/host-shell/runtime";
import { readElectronLocalBrowserBridge } from "@/host-shell/electron-bridge";
import "./boot-styles.css";

// The host marker must be published before the first paint so native drag regions are correct.
// 宿主标记必须在首帧之前发布，使原生拖动区域从一开始就正确。
applyShellHostMarker();

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
  document.getElementById("root")!.textContent = `GUI 初始化失败：${String(error)}`;
});
