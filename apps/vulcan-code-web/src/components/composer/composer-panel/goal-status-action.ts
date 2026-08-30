import type { SessionGoalStatus, SessionGoalUserStatusUpdate } from "@/types";

// composerGoalToggleTarget is shared by the button and its command so lifecycle state cannot drift between them.
// composerGoalToggleTarget 由按钮与命令共享，防止两者对生命周期状态的判断发生偏离。
// Parameters: status is the persisted goal state. Returns: the permitted direct transition, or null for a separate/no action.
// 参数：status 是持久化目标状态。返回值：允许的直接转换；需要独立动作或无动作时返回 null。
export function composerGoalToggleTarget(status: SessionGoalStatus): SessionGoalUserStatusUpdate | null {
  // Only active execution can pause; every resumable stopped UI state must issue the resume command.
  // 只有活动执行可以暂停；所有可恢复的停止界面状态都必须发送恢复命令。
  switch (status) {
    case "active":
      return "paused";
    case "paused":
    case "blocked":
    case "usage_limited":
      return "active";
    // Budget-limited goals require the existing budget editor; completed goals cannot be resumed.
    // 预算受限目标需要既有预算编辑器；已完成目标不能恢复。
    case "budget_limited":
    case "complete":
      return null;
  }
}
