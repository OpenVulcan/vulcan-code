// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { ToolCallCard } from "../tool-call-card";

/** Verifies a hidden worksheet result and an office failure through the conversation's card entry point.
 * 通过会话的工具卡入口验证隐藏工作表结果和办公失败。 */
it("renders worksheet results and keeps the failure trace in its own disclosure", () => {
  // The result lists two sheets from an implicit LuaSkills invocation.
  // 该结果来自隐式 LuaSkills 调用，列出两个工作表。
  const event = {
    api_call_id: 1,
    input: {
      name: "vulcan-work-xlsx-list-sheets",
      args: [{ name: "path", value: "D:\\work\\ledger.xlsx" }],
    },
    kind: "tool_call",
    output: {
      host_result: null,
      is_error: false,
      model_output: JSON.stringify({
        sheet_count: 2,
        sheets: [{ name: "Budget", visibility: "visible" }, { name: "Archive", visibility: "hidden" }],
      }),
      tool_call_id: "call-1",
      tool_name: "call_luaskill",
    },
    request_stream_id: "stream",
    status: "completed",
    tool_call_id: 1,
    tool_name: "call_luaskill",
  } satisfies ToolCallEvent;

  // Mount the real card router so the same component is exercised for a saved event.
  // 挂载实际工具卡路由，用同一个入口覆盖已保存的调用。
  const container = document.createElement("div");
  const root = createRoot(container);
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

  /** Renders `call` as an expanded conversation card in the active locale.
   * 在当前语言下将 `call` 渲染为展开的会话卡片。 */
  function render(call: ToolCallEvent) {
    act(() => root.render(
      <I18nProvider initialLocale="en-US">
        <ToolCallCard event={call} open onOpenChange={() => undefined} />
      </I18nProvider>,
    ));
  }

  try {
    render(event);
    expect(container.querySelector(".vk-tool-title code")?.textContent).toBe("ledger.xlsx");
    expect(Array.from(container.querySelectorAll(".vk-facts dt"), (item) => item.textContent)).toEqual(["Budget", "Archive"]);

    // An implicit writing failure must retain its concise message and a separate diagnostic.
    // 隐式写入失败需要保留简洁的错误原因及独立的诊断。
    const message = "Leave at least one slide.";
    const traceback = "stack traceback:\n\t[C]: in function 'error'";
    render({
      ...event,
      input: { name: "vulcan-work-pptx-remove-slides", args: [] },
      status: "failed",
      output: {
        ...event.output,
        host_result: { kind: "tool_error", payload: { error: message, diagnostic: traceback } },
        is_error: true,
        model_output: message,
      },
    });
    expect(container.querySelector(".vk-tool-result")?.textContent).toContain(message);
    expect(container.querySelector(".vk-tool-result")?.textContent).not.toContain(traceback);
    const disclosure = container.querySelector("details");
    expect(disclosure?.open).toBe(false);
    expect(disclosure?.querySelector("pre")?.textContent).toBe(traceback);
  } finally {
    act(() => root.unmount());
  }
});
