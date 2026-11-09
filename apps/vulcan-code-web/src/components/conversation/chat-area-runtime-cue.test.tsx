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

  /** Renders ChatArea for one run phase; returns nothing, the DOM is read from host.
   * 按一个运行阶段渲染 ChatArea；无返回值，从 host 读取 DOM。 */
  async function render(runPhase: string) {
    await React.act(async () => root.render(
      <I18nProvider initialLocale="en-US"><ChatArea {...chatAreaProps(runPhase)} /></I18nProvider>,
    ));
  }

  it.each(["toolCalling", "requesting", "memoryRecall"])("floats the %s cue over the composer, never in the transcript", async (phase) => {
    await render(phase);
    const transcript = host.querySelector("[data-testid=transcript]")!;
    const dock = host.querySelector("[data-testid=composer-dock]")!;
    expect(transcript.querySelector("[data-runtime-cue]")).toBeNull();
    const cue = dock.querySelector("[data-runtime-cue]");
    expect(cue).not.toBeNull();
    expect(cue!.getAttribute("role")).toBe("status");
  });

  it("shows no cue while the phase has none", async () => {
    await render("toolRunning");
    expect(host.querySelector("[data-runtime-cue]")).toBeNull();
  });

  it("keeps a reasoning status in the transcript and suppresses the floating cue", async () => {
    liveStatus.reasoning = { kind: "encrypted" };
    await render("toolCalling");
    expect(host.querySelector("[data-testid=transcript]")!.childElementCount).toBe(1);
    expect(host.querySelector("[data-runtime-cue]")).toBeNull();
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
    vi.stubGlobal("ResizeObserver", class {
      // Accepts an observed element without delivering geometry notifications.
      // 接收被观察的元素，不发送几何变化通知。
      observe() {}
      // Releases the synthetic observer without retaining any elements.
      // 释放模拟观察器，不保留任何元素。
      disconnect() {}
    });
    // reasoning is the live row whose terminal revision is deliberately omitted.
    // reasoning 是刻意省略终态修订的实时事件。
    const reasoning: ConversationEventSummary = {
      seq: 2, turn_id: 1, event_revision: 1, created_at_ms: 2, updated_at_ms: 2, visibility: "provider",
      event: { kind: "assistant_reasoning", api_call_id: 2, status: "streaming", text: "Reasoning in flight" },
    };
    eventWindow.events = [...initialEvents, reasoning];
    await render("requesting");
    expect(host.querySelector(".vk-think-float")).not.toBeNull();

    eventWindow.events = [...eventWindow.events, {
      seq: 3, turn_id: reasoning.turn_id, event_revision: 1, created_at_ms: 3, updated_at_ms: 3,
      visibility: "provider", event: { kind: "assistant_text", api_call_id: 2, status: "completed", text: "Answer" },
    }, {
      seq: 4, turn_id: reasoning.turn_id, event_revision: 1, created_at_ms: 4, updated_at_ms: 4,
      visibility: boundary.kind === "api_started" ? "host_only" : "ui_only", event: boundary,
    }];
    await render("requesting");
    await React.act(async () => vi.advanceTimersByTimeAsync(THINKING_OVERLAY_EXIT_MS * 2));
    expect(host.querySelector(".vk-think-float")).toBeNull();

    await React.act(async () => root.unmount());
    root = createRoot(host);
    await render("requesting");
    expect(host.querySelector(".vk-think-float")).toBeNull();
  });

  it("keeps the ticking elapsed time out of the live region", async () => {
    await render("toolCalling");
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
