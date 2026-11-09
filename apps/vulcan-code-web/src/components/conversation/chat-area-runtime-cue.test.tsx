// @vitest-environment jsdom
import * as React from "react";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { ConversationEventBody, ConversationEventSummary } from "@/service/conversation-event-window-store";
import { THINKING_OVERLAY_EXIT_MS } from "@/components/conversation/chat-area/visual-state";
import type { ChatAreaProps } from "@/components/conversation/chat-area/types";
import type { TransientReasoningStatus } from "@/types";

// liveStatus lets each scenario choose which request-local status competes with the runtime cue.
// liveStatus 让每个场景选择与运行提示竞争的请求局部状态。
const liveStatus: { reasoning: TransientReasoningStatus | null } = { reasoning: null };

// The event window owns one visible user prompt for the selected session; no service is started.
// 事件窗口为选中会话持有一条可见用户提示；不启动任何服务。
const eventWindow: ConversationEventWindowControllerSnapshot = {
  owner: { session_id: "test-playwright", window_generation: 1 } as ConversationEventWindowControllerSnapshot["owner"],
  service_epoch: null,
  memory_revision: 1,
  durable_revision: 1,
  events: [{
    seq: 1,
    event_revision: 1,
    visibility: "model_visible",
    event: { kind: "user_input", client_action_id: "client-1", text: "run the tests" },
  } as unknown as ConversationEventSummary],
  older_before_seq: null,
  has_more: false,
  open_turn_id: null,
  open_turn_start_seq: null,
  session_config: null,
};

// initialEvents restores the default transcript after each synthetic lifecycle test.
// initialEvents 在每个合成生命周期测试后恢复默认转录。
const initialEvents = eventWindow.events;

vi.mock("@/state/conversation-event-window-react", () => ({
  useSelectedConversationEventWindowSnapshot: () => eventWindow,
  useSelectedConversationEventWindowCommands: () => ({ rehydrate: async () => true }),
}));
vi.mock("@/state/conversation-live-stream-context", () => ({
  useConversationProviderRetryStatus: () => null,
  useConversationTransientReasoningStatus: () => liveStatus.reasoning,
}));
// The transcript and composer stop at their slots: the transcript shows its tail status, the dock its floating status.
// 转录与输入区止于各自插槽：转录显示尾部状态，停靠层显示悬浮状态。
vi.mock("@/components/conversation/conversation-branch-workspace-v1", () => ({
  ConversationBranchWorkspaceV1: (props: { transientReasoningStatus?: React.ReactNode }) => (
    <section data-testid="transcript">{props.transientReasoningStatus}</section>
  ),
}));
vi.mock("@/components/conversation/chat-area/composer-dock-layer", () => ({
  ChatAreaComposerDockLayer: (props: { runtimeStatus?: React.ReactNode }) => (
    <div data-testid="composer-dock">{props.runtimeStatus}</div>
  ),
}));
vi.mock("@/components/conversation/conversation-find-bar", () => ({ ConversationFindBar: () => null }));
vi.mock("@/components/conversation/conversation-generation-dialog-v1", () => ({ ConversationGenerationDialogV1: () => null }));

const { ChatArea } = await import("@/components/conversation/chat-area");

/** Builds ChatArea props for a running session in the given backend run phase.
 * 为处于指定后端运行阶段的运行中会话构建 ChatArea 属性。
 * runPhase is the backend phase text; returns detached synthetic props whose callbacks are never invoked.
 * runPhase 为后端阶段文本；返回独立的合成属性，其回调从不被调用。
 */
function chatAreaProps(runPhase: string): ChatAreaProps {
  const workbench = browserFallbackSnapshot();
  workbench.conversations = workbench.conversations.map((conversation) => (
    conversation.id === "test-playwright" ? { ...conversation, status: "Running", runPhase } : conversation
  ));
  workbench.selectedConversationId = "test-playwright";
  return { workbench, selectedConversationId: "test-playwright" } as unknown as ChatAreaProps;
}

/** Stubs popup viewport observation in jsdom. Takes no inputs and returns nothing.
 * 在 jsdom 中替换浮层视区观察。无参数和返回值。 */
function stubPopupResizeObserver() {
  vi.stubGlobal("ResizeObserver", class {
    // Accepts an observed element without delivering geometry notifications.
    // 接收被观察的元素，不发送几何变化通知。
    observe() {}
    // Releases the synthetic observer without retaining any elements.
    // 释放模拟观察器，不保留任何元素。
    disconnect() {}
  });
}

describe("session runtime cue placement", () => {
  let host: HTMLDivElement;
  let root: ReturnType<typeof createRoot>;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    liveStatus.reasoning = null;
  });

  afterEach(async () => {
    await React.act(async () => root.unmount());
    host.remove();
    eventWindow.events = initialEvents;
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  /** Renders ChatArea with props, optionally observing commits with onRender; returns after the DOM settles.
   * 用 props 渲染 ChatArea，可选用 onRender 观察提交；DOM 稳定后返回。 */
  async function render(props: ChatAreaProps, onRender?: React.ProfilerOnRenderCallback) {
    // area is the real conversation surface under test.
    // area 是受测的真实会话界面。
    const area = <ChatArea {...props} />;
    await React.act(async () => root.render(
      <I18nProvider initialLocale="en-US">
        {onRender ? <React.Profiler id="chat" onRender={onRender}>{area}</React.Profiler> : area}
      </I18nProvider>,
    ));
  }

  it.each(["toolCalling", "requesting", "memoryRecall"])("floats the %s cue over the composer, never in the transcript", async (phase) => {
    await render(chatAreaProps(phase));
    const transcript = host.querySelector("[data-testid=transcript]")!;
    const dock = host.querySelector("[data-testid=composer-dock]")!;
    expect(transcript.querySelector("[data-runtime-cue]")).toBeNull();
    const cue = dock.querySelector("[data-runtime-cue]");
    expect(cue).not.toBeNull();
    expect(cue!.getAttribute("role")).toBe("status");
  });

  it("shows no cue while the phase has none", async () => {
    await render(chatAreaProps("toolRunning"));
    expect(host.querySelector("[data-runtime-cue]")).toBeNull();
  });

  it("keeps a reasoning status in the transcript and suppresses the floating cue", async () => {
    liveStatus.reasoning = { kind: "encrypted" };
    await render(chatAreaProps("toolCalling"));
    expect(host.querySelector("[data-testid=transcript]")!.childElementCount).toBe(1);
    expect(host.querySelector("[data-runtime-cue]")).toBeNull();
  });

  it("commits each reasoning revision once and retains the final text for the overlay exit", async () => {
    vi.useFakeTimers();
    stubPopupResizeObserver();
    // reasoning keeps one canonical event identity as the stream appends text.
    // reasoning 在流式追加正文时保持同一个规范事件标识。
    const reasoning: ConversationEventSummary = {
      seq: 2, turn_id: 1, event_revision: 1, created_at_ms: 2, updated_at_ms: 2, visibility: "provider",
      event: { kind: "assistant_reasoning", api_call_id: 2, status: "streaming", text: "first" },
    };
    // commits counts the whole chat surface's commits, including any caused by popup state updates.
    // commits 统计整个会话界面的提交，包括浮层状态更新触发的提交。
    const commits = vi.fn<React.ProfilerOnRenderCallback>();
    // props keeps the workbench snapshot stable while only the reasoning event changes.
    // props 让工作台快照在只有思考事件变化时保持稳定。
    const props = chatAreaProps("requesting");
    eventWindow.events = [...initialEvents, reasoning];
    await render(props, commits);

    // liveBody stays mounted throughout streaming and exit to preserve the reader's viewport.
    // liveBody 在流式更新和退场期间保持挂载，以保留阅读者的视区。
    const liveBody = host.querySelector(".vk-think-float-content");
    // finalText and revision advance with every cumulative reasoning update.
    // finalText 与 revision 随每次累积思考更新递进。
    let finalText = "first";
    let revision = reasoning.event_revision;
    for (const fragment of Array.from({ length: 8 }, () => "\n\nReasoning paragraph. ".repeat(256))) {
      finalText += fragment;
      revision += 1;
      eventWindow.events = [...initialEvents, {
        ...reasoning, event_revision: revision,
        event: { kind: "assistant_reasoning", api_call_id: 2, status: "streaming", text: finalText },
      }];
      commits.mockClear();
      await render(props, commits);
      expect(commits).toHaveBeenCalledTimes(1);
      expect(liveBody?.textContent).toBe(finalText);
    }

    eventWindow.events = [...initialEvents, {
      ...reasoning, event_revision: revision + 1,
      event: { kind: "assistant_reasoning", api_call_id: 2, status: "completed", text: finalText },
    }];
    await render(props, commits);
    expect(host.querySelector(".vk-think-float-content")).toBe(liveBody);
    expect(liveBody?.textContent).toBe(finalText);
    await React.act(async () => vi.advanceTimersByTimeAsync(THINKING_OVERLAY_EXIT_MS * 2));
    expect(host.querySelector(".vk-think-float")).toBeNull();
  });

  it.each([
    {
      kind: "api_stats", api_call_id: 2, purpose: "main_generation", provider: "test", model: "test",
      usage: {}, status: "completed", continuation_eligibility: "forbidden", timing: {},
    },
    { kind: "turn_boundary", reason: "completed", outcome: "completed", purpose: "business" },
    { kind: "api_started", api_call_id: 3, purpose: "main_generation", request: { provider: "test", model: "test" } },
  ] satisfies ConversationEventBody[])("closes stale reasoning after $kind and stays closed on remount", async (boundary) => {
    vi.useFakeTimers();
    stubPopupResizeObserver();
    // reasoning is the live row whose terminal revision is deliberately omitted.
    // reasoning 是刻意省略终态修订的实时事件。
    const reasoning: ConversationEventSummary = {
      seq: 2, turn_id: 1, event_revision: 1, created_at_ms: 2, updated_at_ms: 2, visibility: "provider",
      event: { kind: "assistant_reasoning", api_call_id: 2, status: "streaming", text: "Reasoning in flight" },
    };
    eventWindow.events = [...initialEvents, reasoning];
    await render(chatAreaProps("requesting"));
    expect(host.querySelector(".vk-think-float")).not.toBeNull();

    eventWindow.events = [...eventWindow.events, {
      seq: 3, turn_id: reasoning.turn_id, event_revision: 1, created_at_ms: 3, updated_at_ms: 3,
      visibility: "provider", event: { kind: "assistant_text", api_call_id: 2, status: "completed", text: "Answer" },
    }, {
      seq: 4, turn_id: reasoning.turn_id, event_revision: 1, created_at_ms: 4, updated_at_ms: 4,
      visibility: boundary.kind === "api_started" ? "host_only" : "ui_only", event: boundary,
    }];
    await render(chatAreaProps("requesting"));
    await React.act(async () => vi.advanceTimersByTimeAsync(THINKING_OVERLAY_EXIT_MS * 2));
    expect(host.querySelector(".vk-think-float")).toBeNull();

    await React.act(async () => root.unmount());
    root = createRoot(host);
    await render(chatAreaProps("requesting"));
    expect(host.querySelector(".vk-think-float")).toBeNull();
  });

  it("keeps the ticking elapsed time out of the live region", async () => {
    await render(chatAreaProps("toolCalling"));
    const cue = host.querySelector("[data-runtime-cue]")!;
    // The live region announces only the phase text; every other child is hidden from assistive technology.
    // 实时区域只播报阶段文本；其余子节点都对辅助技术隐藏。
    const announced = [...cue.children].filter((child) => child.getAttribute("aria-hidden") !== "true");
    expect(announced).toHaveLength(1);
  });
});

it("keeps the floating status out of layout flow and click-through", () => {
  // rules are the kit's real top-level composer rules; viewport and container branches are excluded.
  // rules 是套件真实的输入区顶层规则，排除视口与容器分支。
  const load = createRequire(import.meta.url);
  const postcss = createRequire(load.resolve("vite"))("postcss");
  const rules = postcss.parse(readFileSync(resolve("src/ui-kit/styles/composer.css"), "utf8")).nodes
    .filter((node: { type: string }) => node.type === "rule");
  // declaration returns the last value the rule for selector declares for property.
  // declaration 返回该选择器规则为该属性声明的最后取值。
  const declaration = (selector: string, property: string): string | undefined => rules
    .filter((node: { selectors: string[] }) => node.selectors.includes(selector))
    .flatMap((node: { nodes: { prop: string; value: string }[] }) => node.nodes.filter((decl) => decl.prop === property))
    .map((decl: { value: string }) => decl.value)
    .at(-1);
  // The anchor takes no room: no height, and a bottom margin that cancels the dock gap it adds.
  // 锚点不占空间：没有高度，且底部外边距抵消它带来的停靠层间距。
  expect(declaration(".vk-dock-status", "height")).toBe("0");
  expect(declaration(".vk-dock-status", "margin-bottom")).toBe(declaration(".vk-dock", "gap")?.replace("* ", "* -"));
  expect(declaration(".vk-dock-status > .vk-runtime-cue", "position")).toBe("absolute");
  expect(declaration(".vk-dock-status", "pointer-events")).toBe("none");
});
