import * as React from "react"
import { Progress as ProgressPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

// Progress keeps its accessible value and visual percentage on the same validated scale.
// Progress 使无障碍值和可视百分比使用相同的有效刻度。
function Progress({
  className,
  value,
  max = 100,
  ...props
}: React.ComponentProps<typeof ProgressPrimitive.Root>) {
  // validMax follows the primitive contract: the maximum must be finite and positive.
  // validMax 遵循底层契约：最大值必须为有限正数。
  const validMax = Number.isFinite(max) && max > 0 ? max : 100
  // validValue preserves unknown progress and clamps only actual numeric measurements.
  // validValue 保留未知进度，仅约束实际数值测量。
  const validValue = typeof value === "number" && Number.isFinite(value) ? Math.min(validMax, Math.max(0, value)) : null
  // percentage converts the exact measurement into the CSS indicator's 100-point scale.
  // percentage 将准确测量值转换为 CSS 指示器的百分刻度。
  const percentage = validValue === null ? 0 : (validValue / validMax) * 100
  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      className={cn(
        "relative flex h-1 w-full items-center overflow-x-hidden rounded-full bg-muted",
        className
      )}
      {...props}
      max={validMax}
      value={validValue}
    >
      <ProgressPrimitive.Indicator
        data-slot="progress-indicator"
        className="size-full flex-1 bg-primary transition-all"
        style={{ transform: `translateX(-${100 - percentage}%)` }}
      />
    </ProgressPrimitive.Root>
  )
}

export { Progress }
