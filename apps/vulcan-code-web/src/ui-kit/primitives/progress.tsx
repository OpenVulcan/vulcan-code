import * as React from "react";
import { cx } from "../behavior/core";

// ProgressProps take a value on a 0..max scale; a missing value shows indeterminate progress.
// ProgressProps 接收 0..max 区间的值；缺少值时显示不确定进度。
export interface ProgressProps extends Omit<React.ComponentProps<"div">, "children"> {
  value?: number | null;
  max?: number;
}

// Progress renders a thin progress bar with synchronized ARIA values.
// Progress 渲染细进度条，并同步 ARIA 数值。
export function Progress({ className, value, max = 100, ...props }: ProgressProps) {
  const validMax = Number.isFinite(max) && max > 0 ? max : 100;
  const validValue = typeof value === "number" && Number.isFinite(value) ? Math.min(validMax, Math.max(0, value)) : null;
  const percentage = validValue === null ? 0 : (validValue / validMax) * 100;
  const state = validValue === null ? "indeterminate" : validValue === validMax ? "complete" : "loading";
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={validMax}
      aria-valuenow={validValue ?? undefined}
      data-state={state}
      data-slot="progress"
      data-indeterminate={validValue === null ? "" : undefined}
      className={cx("vk-progress", className)}
      {...props}
    >
      <i data-slot="progress-indicator" style={{ transform: `translateX(-${100 - percentage}%)` }} />
    </div>
  );
}
