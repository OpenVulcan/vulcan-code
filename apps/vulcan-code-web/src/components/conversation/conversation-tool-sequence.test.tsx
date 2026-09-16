// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ConversationRenderItem } from "./conversation-model";
import { ConversationToolSequenceGroup, groupVisibleProcessBodyEntries } from "./conversation-render-items";
import { ConversationMeasuredCollapseProvider } from "./conversation-measured-collapse";

// All rows and timers are synthetic; production collapse and expansion-memory components remain mounted.
// 所有记录与计时器均为合成数据；生产折叠组件和展开记忆组件保持真实挂载。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("matchMedia", () => ({ matches: true, addEventListener() {}, removeEventListener() {} }));
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  vi.useFakeTimers();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

/** Creates one exact tool row; id is stable and status controls its execution summary.
 * 创建一条精确工具记录；id 保持稳定，status 控制执行摘要。
 */
function row(id: string, status: "running" | "completed" = "completed"): ConversationRenderItem {
  return { id, role: "assistant", sourceMessages: [], blocks: [{
    id: id + "-block", kind: "tool-call", title: id, body: "", summary: "", status,
    toolCallId: id, toolEvidence: true, toolName: "shell_command",
  }] };
}

/** Renders a grouped sequence with real memory; active and enabled control lifecycle and user preference.
 * 使用真实记忆渲染分组；active 和 enabled 控制生命周期与用户偏好。
 */
async function render(items: ConversationRenderItem[], active = true, enabled = true) {
  // entry uses production grouping, including the single-tool case.
  // entry 使用生产分组逻辑，包含单工具情况。
  const entry = groupVisibleProcessBodyEntries("process", items, true)[0];
  if (entry.kind !== "tool-sequence") throw new Error("Expected tool group");
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN">
    <ConversationMeasuredCollapseProvider initialMemory={{}} onMemoryChange={() => {}} scopeId="session">
      <ConversationToolSequenceGroup {...entry} active={active} collapseEnabled={enabled}
        renderItem={(item) => <div key={item.id} data-tool={item.id}>{item.id}</div>} />
    </ConversationMeasuredCollapseProvider>
  </I18nProvider>));
}

it("keeps the first tool in the same body through append, completion, and a long idle gap", async () => {
  await render([row("one", "running")]);
  // first preserves the DOM identity rather than remounting into a retirement layer.
  // first 保留 DOM 标识，而不是重新挂载到退场层。
  const first = host.querySelector('[data-tool="one"]');
  await render([row("one"), row("two", "running")]);
  expect(host.querySelector('[data-tool="one"]')).toBe(first);
  await render([row("one"), row("two")]);
  await act(async () => vi.advanceTimersByTime(60_000));
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(2);
  expect(host.querySelector(".conversation-tool-sequence")?.getAttribute("data-expanded")).toBe("true");
  expect(host.querySelector(".conversation-tool-sequence-running, [data-retirement-phase]")).toBeNull();
});

it("folds the entire stable group and preserves explicit manual expansion across updates", async () => {
  await render([row("one"), row("two")]);
  await render([row("one"), row("two")], false);
  expect(host.querySelector(".conversation-tool-sequence")?.getAttribute("data-expanded")).toBe("false");
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(0);
  await act(async () => host.querySelector<HTMLButtonElement>(".conversation-tool-sequence-toggle")!.click());
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(2);
  await render([row("one"), row("two"), row("three")], false);
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(3);
});

it("keeps every row open with folding disabled, including a previously manually closed group", async () => {
  await render([row("one"), row("two")]);
  await act(async () => host.querySelector<HTMLButtonElement>(".conversation-tool-sequence-toggle")!.click());
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(0);
  await render([row("one"), row("two")], false, false);
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(2);
  expect(host.querySelector<HTMLButtonElement>(".conversation-tool-sequence-toggle")!.disabled).toBe(true);
});
