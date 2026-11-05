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

// pendingUpdateVersion names the version the user can move to: the offered one first (the Core only
// offers a version newer than a ready one), otherwise the version ready to start. Returns null when
// there is nothing to update or this copy cannot update.
// pendingUpdateVersion 给出用户可以升级到的版本：优先提议的版本（Core 只提议高于已就绪版本的版本），
// 否则为已就绪待启动的版本。无可更新内容或本副本无法更新时返回 null。
export function pendingUpdateVersion(snapshot: AppUpdateSnapshot | null): string | null {
  if (!snapshot || snapshot.unavailableReason) return null;
  return snapshot.offer?.version ?? snapshot.readyVersion;
}
