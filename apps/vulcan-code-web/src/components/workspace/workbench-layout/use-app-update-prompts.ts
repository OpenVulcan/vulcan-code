import React from "react";

import { restartElectronClientForUpdate } from "@/host-shell/electron-bridge";
import { formatMessage, translateActive } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { SERVICE_RESTARTING_TOPIC, type AppUpdateSnapshot, type ServiceRestartingEvent } from "@/service/app-update-events";
import { restartToUpdate } from "@/service/app-update-service";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import { toast } from "@/ui-kit/primitives/toast";

// Stable toast identities, so a repeated snapshot replaces its prompt instead of stacking it.
// 稳定的提示标识，使重复的快照替换其提示而不是叠加。
const OFFER_TOAST_ID = "app-update-offer";
const READY_TOAST_ID = "app-update-ready";
const RESTARTING_TOAST_ID = "app-update-restarting";
const DESKTOP_PROFILE_TOAST_ID = "app-update-desktop-profile";

// useAppUpdatePrompts announces a newer version and a version ready to start, once per version
// and page, and the AppArmor profile the Linux desktop app still needs, once per page; it hands an
// announced Core restart to the desktop client.
// useAppUpdatePrompts 按版本与页面各提示一次更新版本与已就绪版本，按页面提示一次 Linux 桌面应用仍需要的
// AppArmor 配置；并把已宣布的 Core 重启交给桌面客户端。
// Parameters: snapshot is the workbench's updater state (from useAppUpdate); serviceInstanceEpoch
// resets the restart subscription after a reconnect; openUpdateSettings shows the update settings page.
// 参数：snapshot 为工作台的更新器状态（来自 useAppUpdate）；serviceInstanceEpoch 在重连后重置重启订阅；
// openUpdateSettings 打开更新设置页。
export function useAppUpdatePrompts(
  snapshot: AppUpdateSnapshot | null,
  serviceInstanceEpoch: number,
  openUpdateSettings: () => void,
): void {
  const prompted = React.useRef(new Set<string>());

  React.useEffect(() => {
    const offer = snapshot?.offer;
    if (offer && !prompted.current.has(`offer:${offer.version}`)) {
      prompted.current.add(`offer:${offer.version}`);
      toast.info(formatMessage(translateActive("settings.appUpdate.available"), { version: offer.version }), {
        id: OFFER_TOAST_ID,
        duration: Infinity,
        action: { label: translateActive("appUpdate.toast.view"), onClick: openUpdateSettings },
      });
    }
    const ready = snapshot?.readyVersion;
    if (ready && !prompted.current.has(`ready:${ready}`)) {
      prompted.current.add(`ready:${ready}`);
      toast.dismiss(OFFER_TOAST_ID);
      toast.success(formatMessage(translateActive("settings.appUpdate.ready"), { version: ready }), {
        id: READY_TOAST_ID,
        duration: Infinity,
        action: {
          label: translateActive("settings.appUpdate.restart"),
          onClick: () => {
            void restartToUpdate().catch((cause: unknown) => {
              toast.error(translateActive("settings.appUpdate.failed.restart"), { description: errorMessage(cause) });
            });
          },
        },
      });
    }
    // The settings page explains the profile and offers the terminal command as well.
    // 设置页说明该配置，并同时提供终端命令。
    if (snapshot?.desktopProfile && !prompted.current.has("desktop-profile")) {
      prompted.current.add("desktop-profile");
      toast.warning(translateActive("appUpdate.toast.desktopProfile"), {
        id: DESKTOP_PROFILE_TOAST_ID,
        duration: Infinity,
        action: { label: translateActive("appUpdate.toast.view"), onClick: openUpdateSettings },
      });
    }
  }, [snapshot, openUpdateSettings]);

  React.useEffect(() => {
    // A new service generation means the announced restart has finished.
    // 新的服务代次表示已宣布的重启已经完成。
    toast.dismiss(RESTARTING_TOAST_ID);
    if (!hasServiceRuntime()) return undefined;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    void serviceClient.subscribe<ServiceRestartingEvent>(SERVICE_RESTARTING_TOPIC, (event) => {
      if (cancelled) return;
      toast.dismiss(READY_TOAST_ID);
      toast.loading(translateActive(event.reason === "installer" ? "appUpdate.toast.installing" : "appUpdate.toast.restarting"), {
        id: RESTARTING_TOAST_ID,
        duration: Infinity,
      });
      // In a browser the page reconnects to the new Core by itself.
      // 在浏览器中页面会自行重连到新 Core。
      void restartElectronClientForUpdate(event.reason).catch(() => undefined);
    }).then((nextUnsubscribe) => {
      if (cancelled) nextUnsubscribe();
      else unsubscribe = nextUnsubscribe;
    }).catch(() => undefined);
    return () => {
      cancelled = true;
      unsubscribe?.();
      unsubscribe = null;
    };
  }, [serviceInstanceEpoch]);
}
