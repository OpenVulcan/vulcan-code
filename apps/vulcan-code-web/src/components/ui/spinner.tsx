import { LoaderIcon } from "lucide-react";

import { cn } from "@/lib/utils";
import { useUiText } from "@/components/ui/ui-text";

// Spinner renders the shared lightweight loading indicator.
// Spinner 渲染共享的轻量加载指示器。
export function Spinner({ className, ...props }: React.ComponentProps<"svg">) {
  // text follows the application locale while preserving the standalone primitive contract.
  // text 跟随应用语言，同时保留基础组件独立挂载契约。
  const text = useUiText();
  return (
    <LoaderIcon
      role="status"
      aria-label={text.loading}
      className={cn("size-4 animate-spin", className)}
      {...props}
    />
  );
}
