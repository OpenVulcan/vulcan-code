import * as React from "react";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import { getCoreBrowserRuntimeStatus, installCoreBrowserRuntime, type CoreBrowserRuntimeStatus } from "@/service/core-browser-service";

// CoreBrowserRuntimeControls exposes installation, repair and actionable failure details.
// CoreBrowserRuntimeControls 提供安装、修复及可操作的失败详情。
// Takes no props; returns controls for the Core-owned runtime, independent of GUI browser tabs.
// 无参数；返回 Core 自有运行时控件，独立于 GUI 浏览器标签。
export function CoreBrowserRuntimeControls() {
  // locale controls visible copy, while status always comes from the owning Core.
  // locale 控制显示文案，status 始终来自所属 Core。
  const { locale } = useI18n();
  const chinese = locale.startsWith("zh");
  const [status, setStatus] = React.useState<CoreBrowserRuntimeStatus | null>(null);
  const [error, setError] = React.useState("");
  const [starting, setStarting] = React.useState(false);
  // revision invalidates stale status reads around a new explicit installation request.
  // revision 在新的显式安装请求前后使过期状态读取失效。
  const revision = React.useRef(0);
  const mounted = React.useRef(false);
  const startPending = React.useRef(false);
  // refresh reads current task state and never initiates a download.
  // refresh 读取当前任务状态，绝不发起下载。
  const refresh = React.useCallback(async () => {
    const requested = ++revision.current;
    try {
      const next = await getCoreBrowserRuntimeStatus();
      if (mounted.current && requested === revision.current) { setStatus(next); setError(""); }
    } catch (reason) {
      if (mounted.current && requested === revision.current) setError(String(reason));
    }
  }, []);
  React.useEffect(() => {
    mounted.current = true;
    void refresh();
    // Polling discovers tasks started by another GUI/WebUI client and survives view remounts.
    // 轮询发现其他 GUI/WebUI 客户端发起的任务，并支持界面重新挂载。
    const timer = window.setInterval(() => { if (!startPending.current) void refresh(); }, 2000);
    return () => { mounted.current = false; revision.current += 1; window.clearInterval(timer); };
  }, [refresh]);
  // start asks Core to install or repair; errors stay visible and allow an explicit retry.
  // start 请求 Core 安装或修复；错误保持可见并允许显式重试。
  const start = async (): Promise<void> => {
    if (startPending.current) return;
    startPending.current = true;
    revision.current += 1;
    setStarting(true);
    setError("");
    try {
      const task = await installCoreBrowserRuntime();
      if (mounted.current) setStatus((current) => current ? { ...current, installation: task } : current);
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    } finally {
      startPending.current = false;
      if (mounted.current) setStarting(false);
    }
  };
  // busy covers the request handshake and the independent Core background worker.
  // busy 覆盖请求握手及独立 Core 后台工作。
  const busy = starting || status?.installation?.state === "running";
  return <div className="space-y-2 text-xs">
    <div className="flex flex-wrap items-center gap-2">
      <span role="status">{busy
        ? (chinese ? "正在下载并验证 Core 浏览器组件，可离开此页面…" : "Downloading and verifying the Core browser. You may leave this page…")
        : status?.ready ? (chinese ? "Core 浏览器组件：已就绪" : "Core browser: ready")
        : status ? (chinese ? "Core 浏览器组件：需要安装或修复" : "Core browser: installation or repair required")
        : (chinese ? "正在检查 Core 浏览器组件…" : "Checking Core browser…")}</span>
      {status?.canInstall && <Button type="button" size="sm" variant="outline" disabled={busy}
        onClick={() => void start()}>{chinese ? (status.ready ? "修复浏览器组件" : "安装／修复浏览器组件") : "Install / repair browser"}</Button>}
      <Button type="button" size="sm" variant="ghost" disabled={starting} onClick={() => void refresh()}>
        {chinese ? "刷新状态" : "Refresh status"}
      </Button>
    </div>
    {!busy && (error || status?.installation?.error || status?.diagnostic) && <p role="alert" className="whitespace-pre-wrap break-all text-destructive">
      {error || status?.installation?.error || status?.diagnostic}
    </p>}
  </div>;
}
