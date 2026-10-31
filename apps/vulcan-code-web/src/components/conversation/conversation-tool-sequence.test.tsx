// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ConversationRenderItem } from "./conversation-model";
import { ConversationToolSequenceGroup, groupVisibleProcessBodyEntries } from "./conversation-render-items";
import { ConversationMeasuredCollapseProvider } from "./conversation-measured-collapse";
import { CONVERSATION_AUTOMATIC_FOLD_EVENT } from "./conversation-layout-events";

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

/** Renders the first production sequence; enabled controls folding; returns completion.
 * 渲染首个生产序列；enabled 控制折叠；返回渲染完成。
 */
async function render(items: ConversationRenderItem[], enabled = true, running = true) {
  // entry uses production grouping, including the single-tool case.
  // entry 使用生产分组逻辑，包含单工具情况。
  const entry = groupVisibleProcessBodyEntries("process", items, running)[0];
  if (entry.kind !== "tool-sequence") throw new Error("Expected tool group");
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN">
    <ConversationMeasuredCollapseProvider initialMemory={{}} onMemoryChange={() => {}} scopeId="session">
      <ConversationToolSequenceGroup {...entry} collapseEnabled={enabled}
        renderItem={(item) => <div key={item.id} data-tool={item.id}>{item.id}</div>} />
    </ConversationMeasuredCollapseProvider>
  </I18nProvider>));
}

it.each(["running", "completed", "failed", "stopped"] as const)("shows a single %s tool without a group toggle", async (status) => {
  await render([row("one", status)]);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  expect(host.querySelector('[data-tool="one"]')).not.toBeNull();
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-expanded")).toBe("true");
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-grouped")).toBe("false");
});

it("keeps a single tool visible even when its group previously remembered a manual collapse", async () => {
  await render([row("one"), row("two")]);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  await render([row("one")]);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  expect(host.querySelector('[data-tool="one"]')).not.toBeNull();
});

it("keeps a lone running tool as a plain card and keeps its DOM identity while it completes", async () => {
  await render([row("one", "running")]);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  // first preserves the DOM identity rather than remounting into a retirement layer.
  // first 保留 DOM 标识，而不是重新挂载到退场层。
  const first = host.querySelector('[data-tool="one"]');
  await render([row("one")]);
  await act(async () => vi.advanceTimersByTime(60_000));
  expect(host.querySelector('[data-tool="one"]')).toBe(first);
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-expanded")).toBe("true");
  expect(host.querySelector(".vk-tool-seq-running, [data-retirement-phase]")).toBeNull();
});

it("folds a run from its second tool on, showing only the running cards under the summary", async () => {
  await render([row("one", "running"), row("two", "running")]);
  expect(host.querySelector(".vk-tools-head")).not.toBeNull();
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-running")).toBe("true");
  // two is still the same card when its neighbour finishes, so a live terminal keeps its state.
  // one 结束时 two 仍是同一张卡片，使实时终端保持其状态。
  const two = host.querySelector('[data-tool="two"]');
  await render([row("one"), row("two", "running")]);
  expect([...host.querySelectorAll("[data-tool]")].map((element) => element.getAttribute("data-tool"))).toEqual(["two"]);
  expect(host.querySelector('[data-tool="two"]')).toBe(two);
  // The last running card stays mounted while the body closes, then the group is a summary only.
  // 正文收起期间最后一张运行卡片保持挂载，之后分组只剩摘要。
  await render([row("one"), row("two"), row("three", "running")]);
  expect([...host.querySelectorAll("[data-tool]")].map((element) => element.getAttribute("data-tool"))).toEqual(["three"]);
  // The run is over only once its process stops; until then the last card waits for the next tool.
  // 只有过程停止后序列才结束；在此之前最后一张卡片等待下一个工具。
  await render([row("one"), row("two"), row("three")], true, false);
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-running")).toBe("false");
  expect(host.querySelector(".vk-tools-head")?.getAttribute("aria-expanded")).toBe("false");
});

/** Reads the cards currently laid out in the body, excluding a card that is sliding out.
 * 读取正文中当前布局的卡片，不含正在滑出的卡片。
 */
function shownTools(): (string | null)[] {
  return [...host.querySelectorAll(".vk-tool-seq-slot:not([data-leaving]) [data-tool]")]
    .map((element) => element.getAttribute("data-tool"));
}

it("keeps the last card between serial calls instead of closing and reopening the body", async () => {
  const folds = vi.fn();
  window.addEventListener(CONVERSATION_AUTOMATIC_FOLD_EVENT, folds);
  await render([row("one"), row("two", "running")]);
  await render([row("one"), row("two")]);
  expect(shownTools()).toEqual(["two"]);
  expect(host.querySelector(".vk-collapse")?.getAttribute("aria-hidden")).toBe("false");
  await render([row("one"), row("two"), row("three", "running")]);
  expect(shownTools()).toEqual(["three"]);
  expect(folds).not.toHaveBeenCalled();
  window.removeEventListener(CONVERSATION_AUTOMATIC_FOLD_EVENT, folds);
});

it("folds once when the run closes", async () => {
  const folds = vi.fn();
  window.addEventListener(CONVERSATION_AUTOMATIC_FOLD_EVENT, folds);
  await render([row("one"), row("two", "running"), row("three", "running")]);
  await render([row("one"), row("two"), row("three", "running")]);
  expect(shownTools()).toEqual(["three"]);
  await render([row("one"), row("two"), row("three")]);
  expect(folds).not.toHaveBeenCalled();
  await render([row("one"), row("two"), row("three")], true, false);
  expect(folds).toHaveBeenCalledTimes(1);
  window.removeEventListener(CONVERSATION_AUTOMATIC_FOLD_EVENT, folds);
});

it("closes a run as soon as another row follows it", () => {
  const answer: ConversationRenderItem = { id: "answer", role: "assistant", sourceMessages: [], blocks: [
    { id: "answer-block", kind: "markdown", title: "", body: "Done", summary: "" },
  ] };
  expect(groupVisibleProcessBodyEntries("process", [row("one"), row("two")], true)[0]).toMatchObject({ closed: false });
  expect(groupVisibleProcessBodyEntries("process", [row("one"), row("two"), answer], true)[0]).toMatchObject({ closed: true });
  expect(groupVisibleProcessBodyEntries("process", [row("one"), row("two")], false)[0]).toMatchObject({ closed: true });
});

it("slides the replaced card out above its successor during a 1-for-1 swap", async () => {
  vi.stubGlobal("matchMedia", () => ({ matches: false, addEventListener() {}, removeEventListener() {} }));
  await render([row("one"), row("two", "running")]);
  await render([row("one"), row("two"), row("three", "running")]);
  expect(host.querySelector('.vk-tool-seq-slot[data-leaving] [data-tool="two"]')).not.toBeNull();
  expect(host.querySelector('.vk-tool-seq-slot[data-entering] [data-tool="three"]')).not.toBeNull();
  await act(async () => vi.advanceTimersByTime(1_000));
  expect(host.querySelector(".vk-tool-seq-slot[data-leaving]")).toBeNull();
  expect(shownTools()).toEqual(["three"]);
});

it("closes at once when the user folds a running run they had expanded", async () => {
  await render([row("one"), row("two", "running")]);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  await render([row("one"), row("two")]);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-expanded")).toBe("false");
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(0);
});

it("keeps every card of a running run visible once the user expands it", async () => {
  await render([row("one"), row("two", "running")]);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(2);
  await render([row("one"), row("two"), row("three", "running")]);
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(3);
  await render([row("one"), row("two"), row("three")]);
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(3);
});

it("folds the entire stable group and preserves explicit manual expansion across updates", async () => {
  await render([row("one"), row("two")]);
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-expanded")).toBe("false");
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(0);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(2);
  await render([row("one"), row("two"), row("three")]);
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(3);
});

it("keeps every row open with folding disabled, including a previously manually closed group", async () => {
  await render([row("one"), row("two")]);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(0);
  await render([row("one"), row("two")], false);
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(2);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-grouped")).toBe("false");
});

it("shows every row without a summary while folding is disabled, without remounting tool rows", async () => {
  await render([row("one"), row("two", "running")], false);
  // first verifies changing the setting does not reset the tool card's own state.
  // first 验证切换设置不会重置工具卡片自身的状态。
  const first = host.querySelector('[data-tool="one"]');
  expect(first).not.toBeNull();
  expect(host.querySelector(".vk-tools-head")).toBeNull();
  await render([row("one"), row("two")], false);
  expect(host.querySelector('[data-tool="one"]')).toBe(first);
  expect(host.querySelectorAll("[data-tool]")).toHaveLength(2);
  expect(host.querySelector(".vk-tools-head")).toBeNull();
});

it("keeps reasoning inside the run and keeps one group from the second tool through the answer", async () => {
  // thinking and answer model actual stream rows, not forced component lifecycle overrides.
  // thinking 与 answer 模拟真实流记录，不直接覆盖组件生命周期。
  const thinking: ConversationRenderItem = { id: "thinking", role: "assistant", sourceMessages: [], blocks: [
    { id: "thinking-block", kind: "thinking", title: "", body: "Inspecting", summary: "" },
  ] };
  const answer: ConversationRenderItem = { id: "answer", role: "assistant", sourceMessages: [], blocks: [
    { id: "answer-block", kind: "markdown", title: "", body: "Done", summary: "" },
  ] };
  await render([row("one")]);
  await render([row("one"), thinking]);
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-grouped")).toBe("false");
  for (const items of [
    [row("one"), thinking, row("two")],
    [row("one"), thinking, row("two"), { ...thinking, id: "thinking-next" }],
  ]) {
    await render(items);
    expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-grouped")).toBe("true");
    expect(host.querySelectorAll(".vk-tool-seq")).toHaveLength(1);
  }
  await render([row("one"), thinking, row("two"), answer]);
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-expanded")).toBe("false");
  expect(host.querySelectorAll(".vk-tools-strip > i")).toHaveLength(2);
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-tools-head")!.click());
  expect([...host.querySelectorAll("[data-tool]")].map((element) => element.getAttribute("data-tool")))
    .toEqual(["one", "thinking", "two"]);
  await render([row("one"), thinking, row("two"), answer, row("three")]);
  expect(host.querySelector(".vk-tool-seq")?.getAttribute("data-expanded")).toBe("true");
});

it("closes and folds the tools before a context-compaction card", () => {
  // compaction is the notice row the transcript classifier marks as a compaction section.
  // compaction 是转录分类器标记为压缩分段的通知行。
  const compaction: ConversationRenderItem = { id: "compaction", role: "system", sourceMessages: [], blocks: [
    { id: "compaction-block", kind: "notice", title: "", body: "", summary: "", compactionSectionId: "s", compactionRole: "end", compactionState: "completed" },
  ] };
  const [tools, card, after] = groupVisibleProcessBodyEntries("process", [row("one"), row("two"), compaction, row("three")], true);
  expect(tools).toMatchObject({ kind: "tool-sequence", runningItems: [] });
  expect(card).toMatchObject({ kind: "item", item: { id: "compaction" } });
  expect(after).toMatchObject({ kind: "tool-sequence", items: [{ id: "three" }] });
});

it.each(["context_injection", "reasoning_effort_switch", "model_switch"])(
  "closes the tools before a %s notice and opens a new run after it",
  (noticeKind) => {
    // notice is the display row inserted between tool calls by a prompt injection or a settings switch.
    // notice 是提示词注入或设置切换插入到工具调用之间的显示行。
    const notice: ConversationRenderItem = { id: "notice", role: "assistant", sourceMessages: [], blocks: [
      { id: "notice-block", kind: "notice", title: "", body: "", summary: "", rawMetadata: { notice_kind: noticeKind } },
    ] };
    const [tools, card, after] = groupVisibleProcessBodyEntries("process", [row("one"), row("two"), notice, row("three"), row("four", "running")], true);
    expect(tools).toMatchObject({ kind: "tool-sequence", runningItems: [] });
    expect(card).toMatchObject({ kind: "item", item: { id: "notice" } });
    expect(after).toMatchObject({ kind: "tool-sequence", runningItems: [{ id: "four" }] });
    if (after?.kind !== "tool-sequence") throw new Error("Expected the run after the notice to be a tool sequence");
    expect(after.collapseId).not.toBe((tools as { collapseId: string }).collapseId);
  },
);

it("keeps a still-running tool visible in a run closed by a notice", () => {
  const notice: ConversationRenderItem = { id: "notice", role: "assistant", sourceMessages: [], blocks: [
    { id: "notice-block", kind: "notice", title: "", body: "", summary: "", rawMetadata: { notice_kind: "context_injection" } },
  ] };
  const [tools] = groupVisibleProcessBodyEntries("process", [row("one"), row("two", "running"), notice], true);
  expect(tools).toMatchObject({ kind: "tool-sequence", runningItems: [{ id: "two" }] });
});
