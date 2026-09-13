import React from "react";
import { I18nProvider } from "../../src/i18n";
import { Button } from "../../src/components/ui/button";
import { VmmNativeMaintenancePage } from "../../src/components/features/memory/vmm-native-maintenance-page";
import { maintenanceReply } from "../../src/components/features/memory/vmm-native-maintenance.test-fixture";

/** Mounts the real maintenance page using observable in-memory replies only.
 * 仅使用可观察内存响应挂载真实维护页。
 * No parameters; returns the isolated page and explicit failure controls.
 * 无参数；返回隔离页面及明确失败控制。
 */
export function AuditMemoryMaintenanceSurface() {
  // query explicitly chooses long display names without changing request identities.
  // query 明确选择长显示名称，不更改请求身份。
  const query = new URLSearchParams(window.location.search);
  const [failed, setFailed] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [log, setLog] = React.useState("无操作");
  // scenario remains in a ref so toggling a failure does not replace the action channel.
  // scenario 保存在引用中，使切换失败不替换动作通道。
  const scenario = React.useRef({ failed, longName: query.get("content") === "long" });
  scenario.current = { failed, longName: query.get("content") === "long" };
  const run = React.useCallback(async (action: string, payload?: Record<string, unknown>) => {
    setLog((previous) => `${previous}\n${action} ${JSON.stringify(payload)}`);
    return scenario.current.failed ? null : maintenanceReply(action, payload, scenario.current.longName);
  }, []);
  return <I18nProvider initialLocale="zh-CN"><main className="h-dvh overflow-auto bg-background p-4 text-foreground"><div className="mx-auto max-w-5xl space-y-4">
    <h1>清洗与回收隔离验收</h1><div className="flex gap-2"><Button onClick={() => setFailed(!failed)}>切换失败</Button><Button onClick={() => setBusy(!busy)}>切换忙碌</Button></div>
    <VmmNativeMaintenancePage busy={busy} onRunAction={run} />
    <output className="block max-h-36 overflow-auto break-all text-xs" aria-label="维护动作">{log}</output>
  </div></main></I18nProvider>;
}
