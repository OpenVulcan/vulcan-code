import React from "react";
import { CircleAlert, Play } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

// ConversationContinueControlProps defines the complete presentation contract for one problem-turn recovery entry.
// ConversationContinueControlProps 定义单个问题回合恢复入口的完整展示契约。
export interface ConversationContinueControlProps {
  // Whether the exact retained recovery boundary is currently eligible for claiming.
  // 当前精确保留恢复边界是否具备领取资格。
  eligible: boolean;
  // Whether an eligible recovery claim is already being submitted.
  // 是否正在提交一份具备资格的恢复领取。
  claiming: boolean;
  // Localized idle action label.
  // 本地化空闲操作标签。
  continueLabel: string;
  // Localized in-progress action label.
  // 本地化进行中操作标签。
  continuingLabel: string;
  // Localized prefix explaining that recovery is unavailable.
  // 说明恢复不可用的本地化前缀。
  blockedLabel: string;
  // Exact localized blocked reason shown only through the warning tooltip.
  // 仅通过警告 Tooltip 显示的精确本地化阻断原因。
  blockedReason: string;
  // Optional localized claim feedback shown only after an actual eligible click.
  // 仅在真实可点击领取后显示的可选本地化领取反馈。
  feedback?: { kind: "failed" | "accepted"; label: string } | null;
  // Callback that claims the exact visible recovery boundary.
  // 领取精确可见恢复边界的回调。
  onContinue: () => void;
}

// ConversationContinueControl renders an action for problem turns and a disabled reason tooltip for unsafe boundaries.
// ConversationContinueControl 为问题回合渲染操作，并为不安全边界渲染带原因 Tooltip 的禁用状态。
export function ConversationContinueControl(props: ConversationContinueControlProps) {
  // blockedDescription is shared by the focusable warning trigger and tooltip content.
  // blockedDescription 由可聚焦警告触发器与 Tooltip 内容共享。
  const blockedDescription = `${props.blockedLabel}: ${props.blockedReason}`;
  return (
    <div className="conversation-continue-footer" role="status" aria-live="polite">
      <button
        className={cn(
          "conversation-continue-button",
          !props.eligible && "conversation-continue-button-blocked",
        )}
        disabled={!props.eligible || props.claiming}
        onClick={props.eligible ? props.onContinue : undefined}
        type="button"
      >
        <Play aria-hidden="true" size={15} />
        {props.claiming ? props.continuingLabel : props.continueLabel}
      </button>
      {!props.eligible ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <span
              aria-label={blockedDescription}
              className="conversation-continue-blocked-reason-trigger"
              role="img"
              tabIndex={0}
            >
              <CircleAlert aria-hidden="true" size={17} />
            </span>
          </TooltipTrigger>
          <TooltipContent className="conversation-continue-blocked-tooltip" side="top">
            {blockedDescription}
          </TooltipContent>
        </Tooltip>
      ) : null}
      {props.feedback ? (
        <span
          className={cn(
            "conversation-continue-status",
            props.feedback.kind === "failed" && "conversation-continue-status-failed",
          )}
        >
          {props.feedback.label}
        </span>
      ) : null}
    </div>
  );
}
