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
function row(id: string, status: "running" | "completed" | "failed" | "stopped" = "completed"): ConversationRenderItem {
  return { id, role: "assistant", sourceMessages: [], blocks: [{
    id: id + "-block", kind: "tool-call", title: id, body: "", summary: "", status,
    toolCallId: id, toolEvidence: true, toolName: "shell_command",
  }] };
}

/** Renders the first production sequence; active omits the following answer, enabled controls folding; returns completion.
 * 渲染首个生产序列；active 表示后续回复尚未到达，enabled 控制折叠；返回渲染完成。
 */
async function render(items: ConversationRenderItem[], active = true, enabled = true) {
  // entry uses production grouping, including the single-tool case.
  // entry 使用生产分组逻辑，包含单工具情况。
  const entry = groupVisibleProcessBodyEntries("process", items, true, !active)[0];
  if (entry.kind !== "tool-sequence") throw new Error("Expected tool group");
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN">
    <ConversationMeasuredCollapseProvider initialMemory={{}} onMemoryChange={() => {}} scopeId="session">
      <ConversationToolSequenceGroup {...entry} collapseEnabled={enabled}
        renderItem={(item) => <div key={item.id} data-tool={item.id}>{item.id}</div>} />
    </ConversationMeasuredCollapseProvider>
  </I18nProvider>));
}

it.each([
  ["running", true],
  ["completed", true],
  ["completed", false],
  ["failed", false],
  ["stopped", false],
] as const)("shows a single %s tool without a group toggle (active=%s)", async (status, active) => {
  await render([row("one", status)], active);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  expect(host.querySelector('[data-tool="one"]')).not.toBeNull();
  expect(host.querySelector(".conversation-tool-sequence")?.getAttribute("data-expanded")).toBe("true");
  expect(host.querySelector(".conversation-tool-sequence")?.getAttribute("data-grouped")).toBe("false");
});

it("keeps a single tool visible even when its group previously remembered a manual collapse", async () => {
  await render([row("one"), row("two")], false);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  await render([row("one")], false);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  expect(host.querySelector('[data-tool="one"]')).not.toBeNull();
});

it("keeps the first tool in the same body through append, completion, and a long idle gap", async () => {
  await render([row("one", "running")]);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  // first preserves the DOM identity rather than remounting into a retirement layer.
  // first 保留 DOM 标识，而不是重新挂载到退场层。
  const first = host.querySelector('[data-tool="one"]');
  await render([row("one"), row("two", "running")]);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
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
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(2);
  await render([row("one"), row("two"), row("three")], false);
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(3);
});

it("keeps every row open with folding disabled, including a previously manually closed group", async () => {
  await render([row("one"), row("two")], false);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(0);
  await render([row("one"), row("two")], false, false);
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(2);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  expect(host.querySelector(".conversation-tool-sequence")?.getAttribute("data-grouped")).toBe("false");
});

it("hides running and completed summaries while folding is disabled without remounting tool rows", async () => {
  await render([row("one"), row("two", "running")], true, false);
  // first verifies changing the setting does not reset the tool card's own state.
  // first 验证切换设置不会重置工具卡片自身的状态。
  const first = host.querySelector('[data-tool="one"]');
  expect(first).not.toBeNull();
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  await render([row("one"), row("two")], true, true);
  expect(host.querySelector('[data-tool="one"]')).toBe(first);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  await render([row("one"), row("two")], false, false);
  expect(host.querySelector('[data-tool="one"]')).toBe(first);
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(2);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
});

it("keeps tools unbounded through repeated reasoning and folds once assistant text arrives", async () => {
  // thinking and answer model actual stream rows, not forced component lifecycle overrides.
  // thinking 与 answer 模拟真实流记录，不直接覆盖组件生命周期。
  const thinking: ConversationRenderItem = { id: "thinking", role: "assistant", sourceMessages: [], blocks: [
    { id: "thinking-block", kind: "thinking", title: "", body: "Inspecting", summary: "" },
  ] };
  const answer: ConversationRenderItem = { id: "answer", role: "assistant", sourceMessages: [], blocks: [
    { id: "answer-block", kind: "markdown", title: "", body: "Done", summary: "" },
  ] };
  await render([row("one")]);
  const first = host.querySelector('[data-tool="one"]');
  for (const items of [
    [row("one"), thinking],
    [row("one"), thinking, row("two")],
    [row("one"), thinking, row("two"), { ...thinking, id: "thinking-next" }],
  ]) {
    await render(items);
    expect(host.querySelector('[data-tool="one"]')).toBe(first);
    expect(host.querySelector(".vk-tools-head")).toBeNull();
    expect(host.querySelector(".conversation-tool-sequence")?.getAttribute("data-grouped")).toBe("false");
  }
  await render([row("one"), thinking, row("two"), answer]);
  expect(host.querySelector(".conversation-tool-sequence")?.getAttribute("data-expanded")).toBe("false");
  expect(host.querySelectorAll(".vk-tools-strip > i")).toHaveLength(2);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  expect([...host.querySelectorAll("[data-tool]")].map((element) => element.getAttribute("data-tool")))
    .toEqual(["one", "thinking", "two"]);
  await render([row("one"), thinking, row("two"), answer, row("three")]);
  expect(host.querySelector(".conversation-tool-sequence")?.getAttribute("data-expanded")).toBe("true");
});
