import React from "react";

import { useAppUpdate } from "@/components/features/app-update/use-app-update";
import { restartElectronClientForUpdate } from "@/host-shell/electron-bridge";
import { formatMessage, translateActive } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { SERVICE_RESTARTING_TOPIC, type ServiceRestartingEvent } from "@/service/app-update-events";
import { restartToUpdate } from "@/service/app-update-service";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import { toast } from "@/ui-kit/primitives/toast";

// Stable toast identities, so a repeated snapshot replaces its prompt instead of stacking it.
// 稳定的提示标识，使重复的快照替换其提示而不是叠加。
const OFFER_TOAST_ID = "app-update-offer";
const READY_TOAST_ID = "app-update-ready";
const RESTARTING_TOAST_ID = "app-update-restarting";

// useAppUpdatePrompts announces a newer version and a version ready to start, once per version
// and page, and hands an announced Core restart to the desktop client.
// useAppUpdatePrompts 按版本与页面各提示一次更新版本与已就绪版本，并把已宣布的 Core 重启交给桌面客户端。
// Parameters: serviceInstanceEpoch resets the subscriptions after a reconnect; openUpdateSettings
// shows the update settings page.
// 参数：serviceInstanceEpoch 在重连后重置订阅；openUpdateSettings 打开更新设置页。
export function useAppUpdatePrompts(serviceInstanceEpoch: number, openUpdateSettings: () => void): void {
  const { snapshot } = useAppUpdate(serviceInstanceEpoch);
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
  }, [snapshot, openUpdateSettings]);

  React.useEffect(() => {
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
