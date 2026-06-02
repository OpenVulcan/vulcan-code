import * as React from "react"

import { cn } from "@/lib/utils"

// ShinyText renders reusable animated text for active in-progress states.
// ShinyText 渲染可复用的进行中状态扫光文字。
function ShinyText({
  className,
  speed = 2,
  text,
  ...props
}: React.ComponentProps<"span"> & {
  // Text displayed inside the animated shine effect.
  // 显示在扫光动画中的文字。
  text: string
  // Duration of one shine cycle in seconds.
  // 单次扫光循环的持续秒数。
  speed?: number
}) {
  return (
    <span
      className={cn("shiny-text", className)}
      style={{ "--shiny-text-speed": `${speed}s`, ...props.style } as React.CSSProperties}
      {...props}
    >
      {text}
    </span>
  )
}

export { ShinyText }
