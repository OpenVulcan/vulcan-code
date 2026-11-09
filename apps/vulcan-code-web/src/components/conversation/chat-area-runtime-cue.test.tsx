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
import {
  ConversationEventWindowExternalStoreProvider,
  createConversationEventWindowExternalStore,
} from "@/state/conversation-event-window-react";
import { THINKING_OVERLAY_EXIT_MS } from "@/components/conversation/chat-area/visual-state";
import type { ChatAreaProps } from "@/components/conversation/chat-area/types";
import type { TransientReasoningStatus } from "@/types";

// liveStatus lets each scenario choose which request-local status competes with the runtime cue.
// liveStatus 让每个场景选择与运行提示竞争的请求局部状态。
const liveStatus: { reasoning: TransientReasoningStatus | null } = { reasoning: null };

// dockRenders records the props ChatArea hands the composer layer on every render.
// dockRenders 记录 ChatArea 每次渲染时交给输入区层的属性。
const dockRenders: Record<string, unknown>[] = [];

// transcriptRenders counts transcript renders, which must follow every streamed frame.
// transcriptRenders 统计转录渲染次数，转录必须跟随每个流式帧。
let transcriptRenders = 0;

// The event window owns one visible user prompt for the selected session; no service is started.
// 事件窗口为选中会话持有一条可见用户提示；不启动任何服务。
let eventWindow: ConversationEventWindowControllerSnapshot = {
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
const initialOwner = eventWindow.owner;

// eventWindowCommands is the owner-bound command surface; like the registry's, it keeps one identity per owner.
// eventWindowCommands 是绑定归属的命令面；与 registry 一样，每个归属保持同一身份。
const eventWindowCommands = { rehydrate: async () => true, prependOlder: async () => true };

// eventWindowListeners receive controller notifications; the real external store relays them to React.
// eventWindowListeners 接收控制器通知；真实外部存储把它们转发给 React。
const eventWindowListeners = new Set<() => void>();

/** Replaces the controller snapshot's events and notifies subscribers, as one streamed delta does.
 * 替换控制器快照的事件并通知订阅者，如同一次流式增量。
 * events is the complete next event list; returns nothing. events 为完整的下一份事件列表；无返回值。 */
function publishEvents(events: ConversationEventSummary[]) {
  publishWindow({ ...eventWindow, events });
}

/** Replaces the whole controller snapshot and notifies subscribers, as an owner switch does.
 * 替换整个控制器快照并通知订阅者，如同一次归属切换。
 * next is the complete next snapshot; returns nothing. next 为完整的下一份快照；无返回值。 */
function publishWindow(next: ConversationEventWindowControllerSnapshot) {
  eventWindow = next;
  for (const listener of Array.from(eventWindowListeners)) listener();
}

// eventWindowStore is the production external store over a synthetic controller; each frame flushes on the next
// microtask, which act() drains, so one publication reaches React per streamed delta.
// eventWindowStore 是基于合成控制器的生产外部存储；每帧在下一个微任务刷新（由 act() 排空），使每个流式增量向 React 发布一次。
const eventWindowStore = createConversationEventWindowExternalStore({
  snapshot: () => eventWindow,
  subscribe: (listener) => {
    eventWindowListeners.add(listener);
    return () => eventWindowListeners.delete(listener);
  },
  commands: () => eventWindowCommands,
  status: () => ({ kind: "ready", session_id: "test-playwright" }),
}, (flush) => {
  let cancelled = false;
  queueMicrotask(() => {
    if (!cancelled) flush();
  });
  return () => {
    cancelled = true;
  };
});
vi.mock("@/state/conversation-live-stream-context", () => ({
  useConversationProviderRetryStatus: () => null,
  useConversationTransientReasoningStatus: () => liveStatus.reasoning,
  useConversationTailStatusKind: () => liveStatus.reasoning ? "transientReasoning" : null,
}));
// The transcript and composer stop at their slots: the transcript shows its tail status, the dock its floating status.
// 转录与输入区止于各自插槽：转录显示尾部状态，停靠层显示悬浮状态。
vi.mock("@/components/conversation/conversation-branch-workspace-v1", () => ({
  ConversationBranchWorkspaceV1: (props: { transientReasoningStatus?: React.ReactNode }) => {
    transcriptRenders += 1;
    return <section data-testid="transcript">{props.transientReasoningStatus}</section>;
  },
}));
vi.mock("@/components/conversation/chat-area/composer-dock-layer", () => ({
  ChatAreaComposerDockLayer: (props: { runtimeStatus?: React.ReactNode }) => {
    dockRenders.push(props);
    return <div data-testid="composer-dock">{props.runtimeStatus}</div>;
  },
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
    dockRenders.length = 0;
    transcriptRenders = 0;
  });

  afterEach(async () => {
    await React.act(async () => root.unmount());
    host.remove();
    eventWindow = { ...eventWindow, owner: initialOwner, events: initialEvents };
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
      <ConversationEventWindowExternalStoreProvider store={eventWindowStore}>
        <I18nProvider initialLocale="en-US">
          {onRender ? <React.Profiler id="chat" onRender={onRender}>{area}</React.Profiler> : area}
        </I18nProvider>
      </ConversationEventWindowExternalStoreProvider>,
    ));
  }

  /** Publishes one streamed delta through the external store and waits for React to commit it.
   * 通过外部存储发布一次流式增量，并等待 React 提交。
   * events is the complete next event list. events 为完整的下一份事件列表。 */
  async function stream(events: ConversationEventSummary[]) {
    await React.act(async () => publishEvents(events));
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
    eventWindow = { ...eventWindow, events: [...initialEvents, reasoning] };
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
      commits.mockClear();
      await stream([...initialEvents, {
        ...reasoning, event_revision: revision,
        event: { kind: "assistant_reasoning", api_call_id: 2, status: "streaming", text: finalText },
      }]);
      expect(commits).toHaveBeenCalledTimes(1);
      expect(liveBody?.textContent).toBe(finalText);
    }

    await stream([...initialEvents, {
      ...reasoning, event_revision: revision + 1,
      event: { kind: "assistant_reasoning", api_call_id: 2, status: "completed", text: finalText },
    }]);
    expect(host.querySelector(".vk-think-float-content")).toBe(liveBody);
    expect(liveBody?.textContent).toBe(finalText);
    await React.act(async () => vi.advanceTimersByTimeAsync(THINKING_OVERLAY_EXIT_MS * 2));
    expect(host.querySelector(".vk-think-float")).toBeNull();
  });

  it("re-renders only the transcript while reply text streams", async () => {
    // reply is one streaming answer row whose text grows in place.
    // reply 是一条正文原位增长的流式回答记录。
    const reply: ConversationEventSummary = {
      seq: 2, turn_id: 1, event_revision: 1, created_at_ms: 2, updated_at_ms: 2, visibility: "provider",
      event: { kind: "assistant_text", api_call_id: 2, status: "streaming", text: "first" },
    };
    eventWindow = { ...eventWindow, events: [...initialEvents, reply] };
    await render(chatAreaProps("requesting"));
    // dockBefore and transcriptBefore are the render counts of the frame preceding the deltas.
    // dockBefore 与 transcriptBefore 是增量之前那一帧的渲染次数。
    const dockBefore = dockRenders.length;
    const transcriptBefore = transcriptRenders;
    for (const [revision, text] of [[2, "first second"], [3, "first second third"]] as const) {
      await stream([...initialEvents, {
        ...reply, event_revision: revision,
        event: { kind: "assistant_text", api_call_id: 2, status: "streaming", text },
      }]);
    }
    expect(transcriptRenders - transcriptBefore).toBe(2);
    expect(dockRenders.length).toBe(dockBefore);
  });

  it("shows the selected session's reasoning when another session streamed the same row identity", async () => {
    stubPopupResizeObserver();
    // reasoningRow builds one streaming reasoning row whose seq and revision are identical in both sessions.
    // reasoningRow 构建一条流式思考行，其 seq 与修订号在两个会话中相同。
    const reasoningRow = (text: string): ConversationEventSummary => ({
      seq: 2, turn_id: 1, event_revision: 1, created_at_ms: 2, updated_at_ms: 2, visibility: "provider",
      event: { kind: "assistant_reasoning", api_call_id: 2, status: "streaming", text },
    });
    // switchedProps selects the second session, which is running in the same phase.
    // switchedProps 选中同样处于该阶段运行中的第二个会话。
    const switchedProps = chatAreaProps("requesting");
    switchedProps.workbench.conversations = switchedProps.workbench.conversations.map((conversation) => (
      conversation.id === "test-tools" ? { ...conversation, status: "Running", runPhase: "requesting" } : conversation
    ));
    switchedProps.workbench.selectedConversationId = "test-tools";
    eventWindow = { ...eventWindow, events: [...initialEvents, reasoningRow("Session A reasoning")] };
    await render(chatAreaProps("requesting"));
    expect(host.querySelector(".vk-think-float-content")?.textContent).toBe("Session A reasoning");

    await React.act(async () => publishWindow({
      ...eventWindow,
      owner: { session_id: "test-tools", window_generation: 1 },
      events: [...initialEvents, reasoningRow("Session B reasoning")],
    }));
    await render({ ...switchedProps, selectedConversationId: "test-tools" });
    expect(host.querySelector(".vk-think-float-content")?.textContent).toBe("Session B reasoning");
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
    eventWindow = { ...eventWindow, events: [...initialEvents, reasoning] };
    await render(chatAreaProps("requesting"));
    expect(host.querySelector(".vk-think-float")).not.toBeNull();

    await stream([...eventWindow.events, {
      seq: 3, turn_id: reasoning.turn_id, event_revision: 1, created_at_ms: 3, updated_at_ms: 3,
      visibility: "provider", event: { kind: "assistant_text", api_call_id: 2, status: "completed", text: "Answer" },
    }, {
      seq: 4, turn_id: reasoning.turn_id, event_revision: 1, created_at_ms: 4, updated_at_ms: 4,
      visibility: boundary.kind === "api_started" ? "host_only" : "ui_only", event: boundary,
    }]);
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
