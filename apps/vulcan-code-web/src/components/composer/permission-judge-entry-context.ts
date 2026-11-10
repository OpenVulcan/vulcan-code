import { createContext } from "react";
import type { JudgeBackend } from "@/types/permission-judge";
import type { PermissionMode } from "@/service/permission-mode";

/** Permission judge the composer's auto-approve mode depends on.
 * 输入区自动审批模式所依赖的权限判定。 */
export interface PermissionJudgeEntry {
  /** Configured backend; null keeps auto-approve unavailable until the user sets one up.
   * 已配置的后端；为 null 时自动审批不可用，直到用户完成设置。 */
  backend: JudgeBackend | null;
  // System permission default before checking whether automatic approval is available.
  // 检查自动审批可用性前的系统权限默认值。
  defaultPermissionMode?: PermissionMode;
  /** Opens the permission judge settings page without changing saved settings.
   * 打开权限判定设置页，不改变已保存的设置。 */
  openSettings: () => void;
  /** Re-enters the setup guide whose first step is the permission judge.
   * 重新进入以权限判定为第一步的初始化引导。 */
  openGuide: () => void;
}

// The workbench layout owns both the configured backend and settings navigation; absent outside it.
// 工作台布局同时持有已配置后端与设置导航；在其外部时为空。
export const PermissionJudgeEntryContext = createContext<PermissionJudgeEntry | null>(null);
