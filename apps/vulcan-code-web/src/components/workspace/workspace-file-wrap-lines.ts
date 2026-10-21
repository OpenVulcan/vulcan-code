// Wrap lines: whether file previews fold long code lines into the panel width instead of scrolling sideways. One
// choice for every preview, toggled from the file bar and remembered in this browser only.
// 折行：文件预览是否将过长代码行折入面板宽度，而非横向滚动。所有预览共用一个选择，从文件栏切换，仅在当前浏览器中记住。
import React from "react";

const WRAP_LINES_STORAGE_KEY = "vulcan.workspace.files.wrap-lines.v1";

let wrapLines = readStoredWrapLines();
const listeners = new Set<() => void>();

// useWorkspaceFileWrapLines returns whether code previews wrap and a toggle that changes it for every preview.
// useWorkspaceFileWrapLines 返回代码预览是否折行，以及对所有预览生效的切换函数。
export function useWorkspaceFileWrapLines(): [boolean, () => void] {
  const current = React.useSyncExternalStore(subscribe, () => wrapLines, () => false);
  return [current, toggleWrapLines];
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function toggleWrapLines(): void {
  wrapLines = !wrapLines;
  try {
    window.localStorage.setItem(WRAP_LINES_STORAGE_KEY, String(wrapLines));
  } catch {
    // The choice still holds for this page.
    // 该选择在当前页面仍然有效。
  }
  for (const listener of listeners) listener();
}

// readStoredWrapLines is off unless the viewer turned wrapping on.
// readStoredWrapLines 在查看者未开启折行时为关闭。
function readStoredWrapLines(): boolean {
  try {
    return window.localStorage.getItem(WRAP_LINES_STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}
