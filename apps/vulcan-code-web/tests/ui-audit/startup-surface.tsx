import React from "react";
import { BootErrorScreen, BootLoadingScreen } from "../../src/app/boot-startup-screen";
import { ErrorScreen, LoadingScreen, StartupOverlay } from "../../src/components/shell/startup-screen";
import { I18nProvider } from "../../src/i18n";
import type { StartupSnapshot } from "../../src/types";

/** Displays declared startup states without starting or modifying any service.
 * 展示已声明启动状态，不启动或修改任何服务。
 * No parameters; returns one exact boot or shell surface selected by the audit query.
 * 无参数；返回由验收查询选择的精确启动或壳层界面。
 */
export function AuditStartupSurface() {
  // query explicitly selects the entry and the synthetic measurement boundary.
  // query 明确选择入口与合成测量边界。
  const query = new URLSearchParams(window.location.search);
  // entry selects only a declared production component.
  // entry 仅选择已声明的生产组件。
  const entry = query.get("entry");
  // long supplies an intentionally unbroken diagnostic for containment verification.
  // long 提供刻意连续的诊断文本，用于边界验证。
  const long = query.get("content") === "long";
  // error remains a synthetic display string, never a real credential or filesystem path.
  // error 始终是合成展示字符串，绝非真实凭据或文件路径。
  const error = long ? "SyntheticStartupFailure".repeat(100) : "合成启动错误";
  // reloaded records the explicit recovery callback without reloading or contacting the host.
  // reloaded 记录显式恢复回调，不重载页面或联系宿主。
  const [reloaded, setReloaded] = React.useState(false);
  // startup follows the exact service contract; no elapsed-time estimate becomes progress.
  // startup 遵循精确服务契约；耗时估计不会成为进度。
  const startup: StartupSnapshot = { ready: false, phase: "audit", uiLocale: "zh-CN", recentLogs: [], steps: [
    { id: "done", label: "已完成", status: "ready" },
    { id: "download", label: "运行时下载", status: "running", progressPhase: "downloading", message: long ? error : "正在准备运行时", completedBytes: 1048576, totalBytes: query.get("progress") === "unknown" ? null : 4194304, progressPercent: query.get("progress") === "unknown" ? null : 25 },
  ] };
  return <I18nProvider><output className="fixed left-2 top-2 z-50">{reloaded ? "恢复回调已执行" : "启动展示验收"}</output>{entry === "boot-error" ? <BootErrorScreen error={error} uiLocale="zh-CN" onReload={() => setReloaded(true)} /> : entry === "shell-error" ? <ErrorScreen error={error} /> : entry === "shell" ? <LoadingScreen startup={startup} /> : entry === "overlay" ? <div className="relative h-dvh"><StartupOverlay startup={startup} /></div> : <BootLoadingScreen startup={startup} />}</I18nProvider>;
}
