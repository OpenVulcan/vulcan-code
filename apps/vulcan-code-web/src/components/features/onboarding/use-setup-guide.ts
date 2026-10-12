import * as React from "react";
import type { JudgeBackend } from "@/types";

// SEEN_KEY remembers, per browser, that the guide already opened once on its own.
// SEEN_KEY 按浏览器记住引导已自动打开过一次。
const SEEN_KEY = "vulcan.setupGuide.seen";

// readSeen reports whether the guide already opened on its own; unreadable storage counts as seen
// so a blocked store never reopens it on every start.
// readSeen 报告引导是否已自动打开过；存储不可读时视为已打开，避免每次启动都重新弹出。
function readSeen(): boolean {
  try {
    return window.localStorage.getItem(SEEN_KEY) === "true";
  } catch {
    return true;
  }
}

// writeSeen records the first automatic opening; failures only mean it may open again.
// writeSeen 记录首次自动打开；失败只意味着之后可能再次打开。
function writeSeen() {
  try {
    window.localStorage.setItem(SEEN_KEY, "true");
  } catch {
    // Storage is a per-viewer convenience; the guide stays reachable from settings.
    // 存储只是按浏览者的便利；引导仍可从设置进入。
  }
}

// useSetupGuide opens the guide once while no judge backend is configured and keeps it
// re-enterable. `judgeBackend` is undefined until the service reports it, and `ready` holds the
// automatic opening back while startup is still showing.
// useSetupGuide 在尚未配置判定后端时自动打开一次引导，并保持可重新进入。`judgeBackend` 在服务报告前为
// undefined，`ready` 在启动画面仍显示时阻止自动打开。
// Returns the open state, its setter and an opener for entry points.
// 返回打开状态、其设置函数及供入口使用的打开函数。
export function useSetupGuide(judgeBackend: JudgeBackend | null | undefined, ready: boolean) {
  const [open, setOpen] = React.useState(false);
  React.useEffect(() => {
    if (!ready || judgeBackend !== null || readSeen()) return;
    writeSeen();
    setOpen(true);
  }, [judgeBackend, ready]);
  const show = React.useCallback(() => setOpen(true), []);
  return { open, setOpen, show };
}
