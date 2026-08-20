import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import { ComposerPanelToolbarRight } from "@/components/composer/composer-panel/toolbar-right";
import { I18nProvider } from "@/i18n";

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
}): string {
  return renderToStaticMarkup(
    <I18nProvider initialLocale="en-US">
      <ComposerPanelToolbarRight
        attachmentCount={options.attachmentCount}
        contextCompactionWindowNodes={[]}
        contextUsage={null}
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
        sessionContextCompactionWindowNodeId={null}
        stopAriaLabel="Stop"
      />
    </I18nProvider>,
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
});
