import type { ReactNode } from "react";
import { CircleHelp } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

// Render contextual notes beside a field using the shared focusable tooltip contract.
// 在字段旁展示上下文备注，使用共享的可聚焦提示层契约。
// label names the help button; children contains its explanation; returns a question-mark trigger.
// label 命名帮助按钮；children 包含说明；返回问号触发器。
export function VceHelpTooltip({ label, children }: { label: string; children: ReactNode }) {
  return <Tooltip delayDuration={500}>
    <TooltipTrigger asChild><button type="button" aria-label={label} className="inline-flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring">
      <CircleHelp className="size-3.5" />
    </button></TooltipTrigger>
    <TooltipContent className="max-w-80 text-xs leading-relaxed">{children}</TooltipContent>
  </Tooltip>;
}
