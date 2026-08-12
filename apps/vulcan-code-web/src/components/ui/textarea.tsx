import * as React from "react"

import { cn } from "@/lib/utils"

// TextareaDensity identifies the approved minimum-height and padding recipes.
// TextareaDensity 标识允许使用的最小高度与内边距配方。
type TextareaDensity = "compact" | "default" | "large"

// TextareaProps adds shared visual density to native textarea properties.
// TextareaProps 为原生 textarea 属性增加统一视觉密度。
type TextareaProps = React.ComponentProps<"textarea"> & {
  // Visual density selected from the shared multiline scale.
  // 从统一多行输入体系中选择的视觉密度。
  density?: TextareaDensity
}

// textareaDensityClasses maps semantic density to one shared multiline recipe.
// textareaDensityClasses 将语义密度映射到统一多行输入配方。
const textareaDensityClasses: Record<TextareaDensity, string> = {
  compact: "min-h-16 px-2 py-1.5 text-xs",
  default: "min-h-20 px-2.5 py-2 text-sm",
  large: "min-h-36 px-3 py-2.5 text-sm",
}

// Textarea renders an ordinary multiline field through the unified visual contract.
// Textarea 通过统一视觉契约渲染普通多行字段。
function Textarea({ className, density = "default", ...props }: TextareaProps) {
  return (
    <textarea
      data-slot="textarea"
      data-density={density}
      className={cn(
        "flex field-sizing-content w-full resize-y rounded-lg border border-input bg-transparent transition-colors outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring/35 disabled:cursor-not-allowed disabled:bg-input/50 disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-1 aria-invalid:ring-destructive/30 dark:bg-input/30 dark:disabled:bg-input/80 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40",
        textareaDensityClasses[density],
        className
      )}
      {...props}
    />
  )
}

export { Textarea }
export type { TextareaDensity, TextareaProps }
