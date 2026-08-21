import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ConversationContinueControl } from "./conversation-continue-control";

// renderContinueControl renders one problem-turn recovery control with the shared Tooltip context.
// renderContinueControl 使用共享 Tooltip 上下文渲染一个问题回合恢复控件。
function renderContinueControl(eligible: boolean): string {
  return renderToStaticMarkup(
    <TooltipProvider>
      <ConversationContinueControl
        blockedLabel="无法继续"
        blockedReason="恢复快照校验失败"
        claiming={false}
        continueLabel="继续"
        continuingLabel="正在继续"
        eligible={eligible}
        onContinue={vi.fn()}
      />
    </TooltipProvider>,
  );
}

describe("ConversationContinueControl", () => {
  it("renders an enabled action without an unavailable reason when recovery is eligible", () => {
    const markup = renderContinueControl(true);

    expect(markup).toContain("conversation-continue-button");
    expect(markup).not.toContain("conversation-continue-button-blocked");
    expect(markup).not.toContain("恢复快照校验失败");
  });

  it("renders a disabled action and focusable localized warning for an ineligible problem turn", () => {
    const markup = renderContinueControl(false);

    expect(markup).toContain("conversation-continue-button-blocked");
    expect(markup).toContain("disabled=\"\"");
    expect(markup).toContain("tabindex=\"0\"");
    expect(markup).toContain("无法继续: 恢复快照校验失败");
    expect(markup).not.toContain("conversation-continue-status-blocked");
  });
});
