import { cn } from "@/lib/utils"

// Skeleton renders a non-interactive loading placeholder with inherited dimensions.
// Skeleton 渲染继承尺寸的非交互式加载占位符。
function Skeleton({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="skeleton"
      className={cn("animate-pulse rounded-md bg-muted", className)}
      {...props}
    />
  )
}

export { Skeleton }
