// Restarting to update stops every running background command. Before the restart starts, the user is told how many
// would stop and confirms through the warning's action; with none running the restart starts at once.
// 重启更新会停止所有运行中的后台命令。重启开始之前告知用户将停止的数量，由用户通过警告中的操作确认；没有运行中的
// 后台命令时立即重启。
import { formatMessage, translateActive } from "@/i18n";
import { countRunningShellJobs } from "@/service/session-activity-service";
import { toast } from "@/ui-kit/primitives/toast";

// RESTART_GUARD_TOAST_ID keeps a single warning however often the restart is clicked.
// RESTART_GUARD_TOAST_ID 使无论点击多少次重启都只保留一条警告。
const RESTART_GUARD_TOAST_ID = "app-update-background-jobs";

/** Runs `restart` now, or after the user confirms that running background commands will stop.
 * 立即执行 `restart`，或在用户确认运行中的后台命令将被停止之后执行。 */
export async function restartAfterBackgroundJobCheck(restart: () => void): Promise<void> {
  let running = 0;
  try {
    running = await countRunningShellJobs();
  } catch (cause) {
    // The count is a courtesy; a failed count must not block an update.
    // 该计数只是提醒；计数失败不得阻止更新。
    console.warn("Failed to count the running background commands.", cause);
  }
  if (running === 0) {
    restart();
    return;
  }
  toast.warning(formatMessage(translateActive("appUpdate.toast.backgroundJobs"), { count: running }), {
    id: RESTART_GUARD_TOAST_ID,
    duration: 15_000,
    action: {
      label: translateActive("appUpdate.toast.restartAnyway"),
      onClick: restart,
    },
  });
}
