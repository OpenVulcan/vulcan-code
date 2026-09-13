import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

// Accessible icon action with a tooltip that remains available while disabled.
// 无障碍图标操作，禁用时仍可查看提示。
interface VceActionButtonProps {
  // Accessible action name and optional detailed tooltip.
  // 无障碍操作名称及可选详细提示。
  label: string;
  tooltip?: string;
  // Visual icon and current interaction availability.
  // 展示图标及当前交互可用性。
  icon: ReactNode;
  disabled?: boolean;
  // Execute the explicit user action; returns no value.
  // 执行明确用户操作；无返回值。
  onClick(): void;
}

// Render the supplied icon, label, availability and callback as a shared action control.
// 将传入的图标、标签、可用性及回调渲染为共用操作控件。
export function VceActionButton(props: VceActionButtonProps) {
  return <Tooltip delayDuration={500}>
    <TooltipTrigger asChild><span className="inline-flex" tabIndex={props.disabled ? 0 : undefined}>
      <Button size="icon" variant="outline" aria-label={props.label} disabled={props.disabled} onClick={props.onClick}>{props.icon}</Button>
    </span></TooltipTrigger>
    <TooltipContent>{props.tooltip ?? props.label}</TooltipContent>
  </Tooltip>;
}
