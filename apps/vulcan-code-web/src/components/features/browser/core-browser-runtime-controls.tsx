import * as React from "react";
import { Button } from "@/ui-kit/primitives/button";
import { useI18n } from "@/i18n";
import { getCoreBrowserRuntimeStatus, installCoreBrowserRuntime, type CoreBrowserRuntimeStatus } from "@/service/core-browser-service";
import "@/ui-kit/styles/features/browser.css"

// CoreBrowserRuntimeControls exposes installation, repair and actionable failure details.
// CoreBrowserRuntimeControls 提供安装、修复及可操作的失败详情。
// compact hides a healthy status while keeping installation failures actionable; returns runtime controls.
// compact 隐藏健康状态并保留可操作的安装故障；返回运行时控件。
export function CoreBrowserRuntimeControls({ compact = false }: { compact?: boolean }) {
  // t controls visible copy, while status always comes from the owning Core.
  // t 控制显示文案，status 始终来自所属 Core。
  const { t } = useI18n();
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
  // A background readiness check must not add and remove a temporary toolbar row.
  // 后台就绪检查不得临时插入再移除工具栏行。
  if (compact && !busy && !error && (!status || (status.ready && !status.diagnostic && !status.installation?.error))) return null;
  return <div className="vk-browser-runtime">
    <div>
      <span role="status">{busy
        ? t("browser.downloadingAndVerifyingThe")
        : status?.ready ? t("browser.coreBrowserReady")
        : status ? t("browser.builtInCoreBrowser")
        : t("browser.checkingCoreBrowser")}</span>
      {status?.canInstall && <Button type="button" size="sm" variant="outline" disabled={busy}
        onClick={() => void start()}>{t(status.ready ? "browser.repairComponent" : "browser.installRepairComponent")}</Button>}
      <Button type="button" size="sm" variant="ghost" disabled={starting} onClick={() => void refresh()}>
        {t("browser.refreshStatus")}
      </Button>
    </div>
    {!busy && (error || status?.installation?.error || status?.diagnostic) && <pre role="alert">
      {error || status?.installation?.error || status?.diagnostic}
    </pre>}
  </div>;
}
