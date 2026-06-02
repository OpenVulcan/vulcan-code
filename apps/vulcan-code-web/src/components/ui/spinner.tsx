import { LoaderIcon } from "lucide-react";

import { cn } from "@/lib/utils";

// Spinner renders the shared lightweight loading indicator.
// Spinner 渲染共享的轻量加载指示器。
export function Spinner({ className, ...props }: React.ComponentProps<"svg">) {
  return (
    <LoaderIcon
      role="status"
      aria-label="Loading"
      className={cn("size-4 animate-spin", className)}
      {...props}
    />
  );
}
