// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ScrollText } from "lucide-react";
import { I18nProvider } from "@/i18n";
import type { ConversationBlock } from "./conversation-model";
import { ConversationNoticeCard } from "./conversation-notice-card";

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

// injectionBlock carries two categories, the second with two entries, as a merged card does.
// injectionBlock 携带两个类别（第二个含两条），与合并后的卡片一致。
const injectionBlock: ConversationBlock = {
  id: "injection-1",
  kind: "notice",
  title: "Context injection",
  body: "",
  summary: "sent",
  status: "completed",
  rawMetadata: {
    notice_kind: "context_injection",
    context_entries: [
      { context_key: "plan_mode", category: "plan", operation: "upsert", content: "PLAN-TEXT" },
      { context_key: "dynamic.provider_tools", category: "tools", operation: "upsert", content: "TOOLS-A" },
      { context_key: "dynamic.tool_guidance", category: "tools", operation: "upsert", content: "TOOLS-B" },
    ],
  },
};

// Verifies the card opens in place and pages one category at a time, never through a dialog.
// 验证卡片就地展开并逐类分页，绝不经由弹窗。
it("opens a merged injection card inline with one page per category", () => {
  const onExpandedChange = vi.fn();
  act(() => root.render(
    <I18nProvider initialLocale="en-US">
      <ConversationNoticeCard block={injectionBlock} expandedIds={new Set()} icon={ScrollText}
        onExpandedChange={onExpandedChange} title="Context injection" />
    </I18nProvider>,
  ));
  expect(host.textContent).not.toContain("PLAN-TEXT");
  act(() => host.querySelector<HTMLButtonElement>(".vk-tool-head")!.click());
  expect(onExpandedChange).toHaveBeenCalledWith("note:injection-1", true);
  expect(document.querySelector("[role=dialog]")).toBeNull();
  expect(host.textContent).toContain("PLAN-TEXT");
  expect(host.textContent).toContain("1 / 2");
  expect(host.textContent).not.toContain("TOOLS-A");

  const [, next] = host.querySelectorAll<HTMLButtonElement>(".vk-memory-pager button");
  act(() => next!.click());
  expect(host.textContent).toContain("2 / 2");
  expect(host.textContent).toContain("TOOLS-A");
  expect(host.textContent).toContain("TOOLS-B");
  expect(host.textContent).not.toContain("PLAN-TEXT");
});

// Verifies a single-category card shows its text without a pager.
// 验证单类别卡片直接显示原文且没有翻页。
it("shows a single-category card without a pager", () => {
  const single = { ...injectionBlock, rawMetadata: {
    notice_kind: "context_injection",
    context_entries: [{ context_key: "plan_mode", category: "plan", operation: "remove", content: "PLAN-TEXT" }],
  } };
  act(() => root.render(
    <I18nProvider initialLocale="en-US">
      <ConversationNoticeCard block={single} expandedIds={new Set(["note:injection-1"])} icon={ScrollText}
        onExpandedChange={() => undefined} title="Context injection" />
    </I18nProvider>,
  ));
  expect(host.textContent).toContain("PLAN-TEXT");
  expect(host.querySelector(".vk-memory-pager")).toBeNull();
});
