import * as React from "react";

import {
  APP_UPDATE_CHANGED_TOPIC,
  type AppUpdateSnapshot,
} from "@/service/app-update-events";
import { loadAppUpdateStatus } from "@/service/app-update-service";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import { errorMessage } from "@/lib/display-format";

// useAppUpdate follows the updater state: one status read, then every change event.
// useAppUpdate 跟随更新器状态：读取一次状态，之后接收每个变化事件。
// Parameters: serviceInstanceEpoch restarts the read and subscription after a reconnect.
// 参数：serviceInstanceEpoch 在重连后重新读取并订阅。
// Returns the latest snapshot (null until known), the read failure, and a setter for call results.
// 返回最新快照（未知时为 null）、读取失败原因，以及用于写入调用结果的设置函数。
export function useAppUpdate(serviceInstanceEpoch = 0): {
  snapshot: AppUpdateSnapshot | null;
  loadError: string;
  setSnapshot: (snapshot: AppUpdateSnapshot) => void;
} {
  const [snapshot, setSnapshot] = React.useState<AppUpdateSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState("");

  React.useEffect(() => {
    if (!hasServiceRuntime()) return undefined;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    void serviceClient.subscribe<AppUpdateSnapshot>(APP_UPDATE_CHANGED_TOPIC, (next) => {
      if (!cancelled) setSnapshot(next);
    }).then((nextUnsubscribe) => {
      if (cancelled) nextUnsubscribe();
      else unsubscribe = nextUnsubscribe;
    }).catch(() => undefined);
    void loadAppUpdateStatus().then(
      (next) => { if (!cancelled) { setSnapshot(next); setLoadError(""); } },
      (cause: unknown) => { if (!cancelled) setLoadError(errorMessage(cause)); },
    );
    return () => {
      cancelled = true;
      unsubscribe?.();
      unsubscribe = null;
    };
  }, [serviceInstanceEpoch]);

  return { snapshot, loadError, setSnapshot };
}

// releaseNotesFor picks the notes of the interface language, then English, then any language.
// releaseNotesFor 依次选择界面语言、英语、任一语言的发布说明。
export function releaseNotesFor(notes: Record<string, string>, locale: string): string {
  return notes[locale] ?? notes["en-US"] ?? Object.values(notes)[0] ?? "";
}
