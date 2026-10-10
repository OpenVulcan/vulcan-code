import { createContext } from "react";
import type { JudgeState } from "@/types/permission-judge";

/** Permission judge entry shown beside the composer permission modes.
 * 显示在输入区权限模式旁的权限判定入口。 */
export interface PermissionJudgeEntry {
  /** Saved switch: null needs setup, off hides the entry, enabled shows its state.
   * 已保存的开关：null 需要设定，off 隐藏入口，enabled 显示其状态。 */
  state: JudgeState | null;
  /** Opens the permission judge settings page without changing saved settings.
   * 打开权限判定设置页，不改变已保存的设置。 */
  openSettings: () => void;
}

// The workbench layout owns both the saved switch and settings navigation; absent outside it.
// 工作台布局同时持有已保存开关与设置导航；在其外部时为空。
export const PermissionJudgeEntryContext = createContext<PermissionJudgeEntry | null>(null);
