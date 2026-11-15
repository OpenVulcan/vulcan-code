// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { readToolCall } from "../tool-call";
import { GenericToolCard } from "./generic-card";

/** Verifies the cause is visible and the host diagnostic stays collapsed in a failed tool card.
 * 验证失败工具卡直接显示具体原因，并默认折叠宿主诊断。 */
it("shows the failure cause and collapses its diagnostic", () => {
  // The result has separate model and host content, as produced by the runtime bridge.
  // 调用结果分别包含模型正文和宿主诊断，与运行时桥接的输出一致。
  const cause = "Leave at least one slide.";
  const stack = "stack traceback:\n  [C]: in function 'error'";
  const event = {
    api_call_id: 1,
    input: { path: "slides.pptx" },
    kind: "tool_call",
    output: {
      host_result: { kind: "tool_error", payload: { error: cause, diagnostic: stack } },
      is_error: true,
      model_output: cause,
      tool_call_id: "call-1",
      tool_name: "call_luaskill",
    },
    request_stream_id: "stream-1",
    started_at_ms: 0,
    status: "failed",
    tool_call_id: 1,
    tool_name: "call_luaskill",
  } satisfies ToolCallEvent;

  // Mount the outer card open to verify that its diagnostic has an independent disclosure.
  // 展开外层工具卡，验证诊断仍具有独立的折叠状态。
  const target = document.createElement("div");
  const root = createRoot(target);
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  try {
    act(() => root.render(
      <I18nProvider initialLocale="en-US">
        <GenericToolCard call={readToolCall(event)} family="other" open onOpenChange={() => undefined} />
      </I18nProvider>,
    ));
    const details = target.querySelector<HTMLDetailsElement>("details");
    expect(details?.open).toBe(false);
    expect(details?.querySelector("pre")?.textContent).toBe(stack);
    details?.remove();
    expect(target.textContent).toContain(cause);
    expect(target.textContent).not.toContain(stack);
  } finally {
    act(() => root.unmount());
  }
});
