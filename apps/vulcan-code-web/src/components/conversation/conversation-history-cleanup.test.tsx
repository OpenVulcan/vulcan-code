// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { serviceClient } from "@/service/webui-service-runtime";
import { ConversationHistoryDialogContext } from "@/state/conversation-history-dialog";
import { ConversationHistoryCleanup } from "./conversation-history-cleanup";

vi.mock("@/service/webui-service-runtime", () => ({
  hasServiceRuntime: () => true,
  serviceClient: { request: vi.fn() },
}));

it("previews the whole eligible prefix and submits only the chosen cutoff after confirmation", async () => {
  // The mock uses separated source positions to catch accidental index-based RPC payloads.
  // 使用分隔的来源位置，以发现误用数组下标的 RPC 请求。
  const eligible = [
    { through: 11, prompt_preview: "Oldest prompt", started_at_ms: 1_750_000_000_000 },
    { through: 35, prompt_preview: "Middle prompt", started_at_ms: 1_750_000_010_000 },
    { through: 64, prompt_preview: "Newest eligible prompt", started_at_ms: 1_750_000_020_000 },
  ];
  // Return one stable preview to all calls in this interaction.
  // 为这次交互中的所有请求返回同一份预览。
  const request = vi.mocked(serviceClient.request);
  request.mockResolvedValue({
    enabled: true, eligible, cleaned_ranges: [], history: { turns: [], resources: [] }, generation: 3, revision: 18,
  });
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // Mount to the document so the real dialog and select portals can receive keyboard events.
  // 挂载到文档，使真实弹窗及选择器门户可以接收键盘事件。
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  try {
    await React.act(async () => root.render(<I18nProvider initialLocale="en-US">
      <ConversationHistoryDialogContext.Provider value={{ sessionId: "sample", open: true, setOpen: vi.fn() }}>
        <ConversationHistoryCleanup sessionId="sample" running={false} compactionRevision={null} onCoverage={vi.fn()} />
      </ConversationHistoryDialogContext.Provider>
    </I18nProvider>));
    // The modal exposes only one selectable boundary and starts at the full range.
    // 弹窗只提供一个可选边界，默认使用完整范围。
    const dialog = document.querySelector<HTMLElement>('[role="dialog"]')!;
    const picker = dialog.querySelector<HTMLButtonElement>('[role="combobox"]')!;
    expect(dialog.querySelectorAll('[role="combobox"]')).toHaveLength(1);
    expect(dialog.textContent).toContain(eligible[0].prompt_preview);
    expect(picker.textContent).toContain(eligible[2].prompt_preview);
    expect(picker.querySelector("time")?.textContent).toBeTruthy();
    await React.act(async () => picker.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
    // Select the middle prompt by its visible text.
    // 按可见提示词选择中间回合。
    const middle = [...document.querySelectorAll<HTMLElement>('[role="option"]')]
      .find((option) => option.textContent?.includes(eligible[1].prompt_preview))!;
    await React.act(async () => middle.click());
    expect(picker.textContent).toContain(eligible[1].prompt_preview);
    // A first press only reviews the destructive action.
    // 第一次点击只进入变更确认。
    const clean = dialog.querySelector<HTMLButtonElement>('[data-slot="dialog-footer"] button[data-variant="primary"]')!;
    await React.act(async () => clean.click());
    expect(request.mock.calls.some(([, input]) => input?.operation === "clean")).toBe(false);
    // The confirmed request contains only the chosen endpoint and snapshot identity.
    // 确认后的请求只包含所选截止位置与快照标识。
    const confirm = dialog.querySelector<HTMLButtonElement>('button[data-variant="danger"]')!;
    await React.act(async () => confirm.click());
    expect(request).toHaveBeenLastCalledWith("service_conversation_history_v1", {
      operation: "clean", session_id: "sample", through: eligible[1].through, generation: 3, revision: 18,
    });
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
    request.mockReset();
  }
});
