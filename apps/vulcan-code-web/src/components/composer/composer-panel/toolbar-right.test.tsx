import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import { ComposerPanelToolbarRight } from "@/components/composer/composer-panel/toolbar-right";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { ContextUsageSnapshot, SessionProviderUsageSummary } from "@/types";

// renderToolbar renders the exact composer action surface for one draft and run state.
// renderToolbar 为一个草稿与运行状态渲染精确的输入区操作面。
function renderToolbar(options: {
  // Current attachment count.
  // 当前附件数量。
  attachmentCount: number;
  // Current draft text.
  // 当前草稿文本。
  draftInput: string;
  // Whether the conversation is running.
  // 会话是否正在运行。
  isConversationRunning: boolean;
  // Context usage snapshot shown by the context-capacity card.
  // 上下文容量卡片展示的上下文占用快照。
  contextUsage?: ContextUsageSnapshot | null;
  // Session-level Provider accounting shown by the context-capacity card.
  // 上下文容量卡片展示的会话级 Provider 统计。
  sessionProviderUsage?: SessionProviderUsageSummary | null;
}): string {
  return renderToStaticMarkup(
    <TooltipProvider>
      <I18nProvider initialLocale="en-US">
        <ComposerPanelToolbarRight
          draftSlotKey="conversation:toolbar-test"
          attachmentCount={options.attachmentCount}
          contextCompactionWindowNodes={[]}
          contextUsage={options.contextUsage ?? null}
          draftInput={options.draftInput}
          isConversationRunning={options.isConversationRunning}
          modelContextLimit={null}
          onReasoningEffortChange={vi.fn()}
          onSetSessionContextCompactionWindow={vi.fn()}
          onSend={vi.fn()}
          reasoningAriaLabel="Reasoning"
          reasoningEffort=""
          reasoningEffortOptions={[]}
          reasoningPickerOpenSignal={0}
          selectFallbackLabel="Select"
          sendAriaLabel="Send"
          sendBlocked={false}
          stopPending={false}
          sessionProviderUsage={options.sessionProviderUsage ?? null}
          sessionContextCompactionWindowNodeId={null}
          stopAriaLabel="Stop"
        />
      </I18nProvider>
    </TooltipProvider>,
  );
}

// Composer action selection keeps stop and send mutually exclusive during an active run.
// 输入区操作选择在活跃运行期间保持停止与发送互斥。
describe("composer toolbar primary action", () => {
  it("shows only stop while running without new input", () => {
    const markup = renderToolbar({ attachmentCount: 0, draftInput: "", isConversationRunning: true });

    expect(markup).toContain('aria-label="Stop"');
    expect(markup).not.toContain('aria-label="Send"');
  });

  it("shows only send while running with new text", () => {
    const markup = renderToolbar({ attachmentCount: 0, draftInput: "next request", isConversationRunning: true });

    expect(markup).toContain('aria-label="Send"');
    expect(markup).not.toContain('aria-label="Stop"');
  });

  it("shows only send while running with a new attachment", () => {
    const markup = renderToolbar({ attachmentCount: 1, draftInput: "", isConversationRunning: true });

    expect(markup).toContain('aria-label="Send"');
    expect(markup).not.toContain('aria-label="Stop"');
  });

  it("renders session Provider usage inside the context-capacity tooltip", () => {
    // contextUsage is the minimal active snapshot required to mount the capacity tooltip.
    // contextUsage 是挂载上下文容量浮层所需的最小活动快照。
    const contextUsage: ContextUsageSnapshot = {
      baselineMatchesActiveRuntime: true,
      breakdown: [],
      cacheHitBasisPoints: 9_820,
      maxTokens: 950_000,
      percent: 8,
      severity: "normal",
      tooltipRows: [],
      tooltipTitle: "Context capacity",
      usageBasisPoints: 810,
      usedTokens: 77_000,
    };
    // sessionProviderUsage mirrors one selected-session accounting projection.
    // sessionProviderUsage 模拟当前选中会话的一份统计投影。
    const sessionProviderUsage: SessionProviderUsageSummary = {
      cumulativeUsage: { totalTokens: 35_037_962 },
      inFlightCallCount: 0,
      lastUsage: { totalTokens: 35_037_962 },
      latestCall: null,
      providerCallCount: 2,
      unknownUsageCallCount: 0,
      usageRevision: 1,
    };
    const markup = renderToolbar({
      attachmentCount: 0,
      contextUsage,
      draftInput: "",
      isConversationRunning: false,
      sessionProviderUsage,
    });

    expect(markup).toContain('aria-label="Context capacity"');
    expect(markup).not.toContain("Total tokens: 35.04M");
  });
});
