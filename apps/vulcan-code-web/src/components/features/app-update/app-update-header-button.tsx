import * as React from "react";

import { AppUpdateContext, pendingUpdateVersion } from "@/components/features/app-update/app-update-context";
import { formatMessage, useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip";

// AppUpdateDialog loads with its notes renderer only when the user opens it.
// AppUpdateDialog 仅在用户打开时才连同其说明渲染器一起加载。
const AppUpdateDialog = React.lazy(() => import("@/components/features/app-update/app-update-dialog"));

// AppUpdateHeaderButton shows a compact "Update" entry while the Core reports a newer version or a
// version ready to start, and opens the update dialog. It renders nothing otherwise.
// AppUpdateHeaderButton 在 Core 报告更新版本或已就绪待启动版本时显示紧凑的“更新”入口并打开更新对话框，
// 否则不渲染任何内容。
export function AppUpdateHeaderButton() {
  const { t } = useI18n();
  const host = React.useContext(AppUpdateContext);
  const [open, setOpen] = React.useState(false);
  const version = pendingUpdateVersion(host?.snapshot ?? null);
  if (!host || !version) return null;
  const hint = host.snapshot?.offer
    ? formatMessage(t("settings.appUpdate.available"), { version })
    : formatMessage(t("settings.appUpdate.ready"), { version });
  // The entry sits in the title strip, whose double click maximizes the window; double clicks on the
  // entry or inside its dialog (a portal that still bubbles through React) stay here.
  // 入口位于标题条中，标题条双击会最大化窗口；入口及其对话框（门户仍按 React 树冒泡）内的双击在此截止。
  return <span className="vk-brand-update" onDoubleClick={(event) => event.stopPropagation()}>
    <Tooltip>
      <TooltipTrigger asChild>
        <Button aria-haspopup="dialog" data-app-update-entry="" onClick={() => setOpen(true)} size="xs" variant="ember">
          <span>{t("appUpdate.header.button")}</span>
        </Button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{hint}</TooltipContent>
    </Tooltip>
    {open ? (
      <React.Suspense fallback={null}>
        <AppUpdateDialog host={host} onClose={() => setOpen(false)} />
      </React.Suspense>
    ) : null}
  </span>;
}
