import { createContext } from "react";

import type { AppUpdateSnapshot } from "@/service/app-update-events";

// AppUpdateHost shares the workbench's one updater state with surfaces outside the settings page.
// AppUpdateHost 将工作台唯一的更新器状态共享给设置页之外的界面。
export interface AppUpdateHost {
  // Latest snapshot reported by the Core; null until the first read settles.
  // Core 报告的最新快照；首次读取完成前为 null。
  snapshot: AppUpdateSnapshot | null;
  // Stores the snapshot a call returned, so every surface shows the accepted state at once.
  // 保存调用返回的快照，使所有界面立即显示已接受的状态。
  setSnapshot: (snapshot: AppUpdateSnapshot) => void;
  // Opens Settings → Software update.
  // 打开“设置 → 软件更新”。
  openSettings: () => void;
}

// AppUpdateContext carries the workbench updater state; absent outside the workbench.
// AppUpdateContext 承载工作台的更新器状态；工作台之外为空。
export const AppUpdateContext = createContext<AppUpdateHost | null>(null);

// pendingUpdateVersion returns the version worth notifying the user about.
// pendingUpdateVersion 返回用于提示用户的版本。
export function pendingUpdateVersion(snapshot: AppUpdateSnapshot | null): string | null {
  if (!snapshot || snapshot.unavailableReason) return null;
  if (snapshot.settings.autoDownload && snapshot.offer?.installable) {
    return snapshot.activity === "downloading" ? null : snapshot.readyVersion;
  }
  return snapshot.offer?.version ?? snapshot.readyVersion;
}
