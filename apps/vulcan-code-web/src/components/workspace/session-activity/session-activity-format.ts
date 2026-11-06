// Compact display helpers of the session activity panel.
// 会话活动面板的紧凑显示辅助函数。

// formatActivityDuration renders a duration as "12s", "3m 05s" or "1h 02m".
// formatActivityDuration 将时长渲染为 "12s"、"3m 05s" 或 "1h 02m"。
export function formatActivityDuration(milliseconds: number): string {
  const seconds = Math.max(0, Math.floor((Number.isFinite(milliseconds) ? milliseconds : 0) / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
  return `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m`;
}
