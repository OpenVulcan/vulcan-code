import * as React from "react"

import { cn } from "@/lib/utils"

// InputDensity identifies the approved height and horizontal-padding recipes.
// InputDensity 标识允许使用的高度与水平内边距配方。
type InputDensity = "compact" | "default" | "large"

// InputProps adds visual density without conflicting with the native numeric size attribute.
// InputProps 增加视觉密度，同时避免与原生数字 size 属性冲突。
type InputProps = React.ComponentProps<"input"> & {
  // Visual density selected from the shared input scale.
  // 从统一输入控件体系中选择的视觉密度。
  density?: InputDensity
}

// inputDensityClasses maps semantic density to one shared control recipe.
// inputDensityClasses 将语义密度映射到统一控件配方。
const inputDensityClasses: Record<InputDensity, string> = {
  compact: "h-7 px-2 text-xs",
  default: "h-8 px-2.5 text-sm",
  large: "h-9 px-3 text-sm",
}

// Input renders an ordinary single-line field through the unified visual contract.
// Input 通过统一视觉契约渲染普通单行字段。
function Input({ className, density = "default", type, ...props }: InputProps) {
  return (
    <input
      type={type}
      data-slot="input"
      data-density={density}
      className={cn(
        "w-full min-w-0 rounded-lg border border-input bg-card py-1 transition-[color,background-color,border-color,box-shadow] duration-(--vcn-duration-fast) ease-(--vcn-ease-out) outline-none hover:border-[color-mix(in_oklch,var(--input),var(--foreground)_14%)] focus-visible:shadow-(--vcn-focus-halo) file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring/35 disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-input/50 disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-1 aria-invalid:ring-destructive/30 dark:disabled:bg-input/80 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40",
        inputDensityClasses[density],
        className
      )}
      {...props}
    />
  )
}

export { Input }
export type { InputDensity, InputProps }
