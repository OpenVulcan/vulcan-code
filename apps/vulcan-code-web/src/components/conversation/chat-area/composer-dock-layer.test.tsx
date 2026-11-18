// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import {
  ChatAreaComposerDockLayer,
  composerDockLayerPropsEqual,
  type ChatAreaComposerDockLayerProps,
} from "./composer-dock-layer";
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import { ComposerDock } from "@/components/composer/composer-dock";
import { I18nProvider } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";
import { DEFAULT_THINKING_DISPLAY_MODE } from "../thinking-display";

// The projection test stops at the child boundary and never starts composer hooks or services.
// 投影测试止于子组件边界，绝不启动输入区钩子或服务。
vi.mock("@/components/composer/composer-dock", () => ({ ComposerDock: vi.fn(() => null), composerAttachmentMedia: {} }));

beforeEach(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true));
afterEach(() => vi.unstubAllGlobals());

/** Creates authoritative saved configuration without adding a visible transcript.
 * 创建权威的已保存配置，不添加可见转录。
 * No arguments; returns a detached typed snapshot used only by projection tests.
 * 无参数；返回仅用于投影测试的独立类型化快照。
 */
function configuredSnapshot(): WorkbenchSnapshot {
  // workbench contains only synthetic, detached configuration values.
  // workbench 仅包含合成、独立的配置值。
  const workbench = browserFallbackSnapshot();
  workbench.sessionVmmPolicy = { memoryEnabled: true, precheckMode: "disabled", precheckEnabled: false, memoryWritebackMode: "enabled", memoryWritebackEnabled: true };
  workbench.sessionPlanMode = { executionMode: "plan", promptInjected: false };
  workbench.sessionContextWindowSelection = { node: "compression-window-3" };
  workbench.sessionGoal = { sessionId: "preview", goalId: "audit", objective: "Saved before the first message", status: "paused",
    tokensUsed: 0, usageTokens: 0, apiCallCount: 0, toolCallCount: 0, inputTokens: 0, cachedInputTokens: 0,
    outputTokens: 0, reasoningTokens: 0, totalTokens: 0, timeUsedSeconds: 0, createdAtMs: 1, updatedAtMs: 1 };
  workbench.vmmMemory.globalDraft.backendMode = "native";
  workbench.vmmMemory.runtime.processReady = true;
  return workbench;
}

/** Invokes the pure wiring component and returns its actual child props.
 * 调用纯接线组件并返回它实际产生的子组件属性。
 * workbench is already locally projected; id selects the persisted owner and empty controls only layout.
 * workbench 已按本地选择投影；id 选择持久归属，empty 仅控制布局。
 */
function layerProps(workbench: WorkbenchSnapshot, id: string | null, empty: boolean, runtimeStatus: ReactNode = null): ChatAreaComposerDockLayerProps {
  // Only workbench is inspected here; all parent callbacks are forwarded without invocation.
  // 此处仅检查 workbench；所有父层回调只被转发，不会调用。
  const chatAreaProps = { workbench } as ChatAreaComposerDockLayerProps["chatAreaProps"];
  return { chatAreaProps, projectId: "preview", conversationId: id,
    isEmptyConversation: empty, startProjectName: "project", isConversationRunning: false, showScrollToBottom: false,
    todolistStatusHost: null, sessionTodolist: null,
    thinkingDisplayMode: DEFAULT_THINKING_DISPLAY_MODE, sessionReviewSummary: null, onComposerDockHeightChange: noop,
    runtimeStatus, onScrollToBottom: noop, onThinkingDisplayModeChange: noop, onSubmitPrompt: submit,
  };
}

// noop and submit are shared callbacks, so two prop sets built for the same scenario compare equal.
// noop 与 submit 是共享回调，使为同一场景构建的两组属性比较相等。
const noop = () => undefined;
const submit = async () => true;

/** Renders the layer for the test workspace, conversation, layout and status, then returns the composer's props.
 * 渲染指定测试工作区、会话、布局与状态的停靠层，并返回实际传给输入区的属性。
 */
async function project(workbench: WorkbenchSnapshot, id: string | null, empty: boolean, runtimeStatus: ReactNode = null) {
  // A real React root supplies the locale and memoization hooks used by the layer.
  // 使用真实 React 根节点为停靠层提供语言及记忆化钩子。
  const root = createRoot(document.createElement("div"));
  // The mock records the props forwarded to the child without starting composer services.
  // 模拟子组件记录收到的属性，无需启动输入区服务。
  const composer = vi.mocked(ComposerDock);
  composer.mockClear();
  try {
    await act(async () => root.render(
      <I18nProvider initialLocale="en-US">
        <ChatAreaComposerDockLayer {...layerProps(workbench, id, empty, runtimeStatus)} />
      </I18nProvider>,
    ));
    // Read the child props from the render that just committed.
    // 从刚提交的渲染中读取子组件属性。
    const call = composer.mock.lastCall;
    if (!call) throw new Error("The dock layer did not render the composer");
    return call[0];
  } finally {
    await act(async () => root.unmount());
  }
}

describe("composer dock authoritative settings", () => {
  it("skips Workbench deltas that touch only fields the composer does not read", () => {
    const workbench = configuredSnapshot();
    const before = layerProps(workbench, "preview", false);
    // unrelated replaces the run phase of every session and the command sessions, as a running turn does.
    // unrelated 替换每个会话的运行阶段与命令会话，与运行中的回合一致。
    const unrelated = { ...workbench, commandSessions: [], conversations: workbench.conversations.map((conversation) => ({ ...conversation, runPhase: "toolRunning" })) };
    expect(composerDockLayerPropsEqual(before, layerProps(unrelated, "preview", false))).toBe(true);
    // contextUsage is shown by the composer, so a new value must re-render it.
    // contextUsage 由输入区显示，因此新值必须使其重新渲染。
    const read = { ...workbench, contextUsage: { ...workbench.contextUsage } } as WorkbenchSnapshot;
    expect(composerDockLayerPropsEqual(before, layerProps(read, "preview", false))).toBe(false);
  });
  it("retains saved session settings when the durable session has no visible messages", async () => {
    const workbench = configuredSnapshot();
    const props = await project(workbench, "preview", true);
    expect(props.sessionVmmPolicy).toBe(workbench.sessionVmmPolicy);
    expect(props.sessionGoal).toBe(workbench.sessionGoal);
    expect(props.sessionPlanMode).toBe(workbench.sessionPlanMode);
    expect(props.sessionContextWindowSelection).toEqual({ node: "compression-window-3" });
  });
  it("does not project persisted configuration into an ownerless materialization frame", async () => {
    const props = await project(configuredSnapshot(), null, false);
    expect(props.sessionVmmPolicy).toBeNull();
    expect(props.sessionGoal).toBeNull();
    expect(props.sessionPlanMode).toBeNull();
    expect(props.sessionContextWindowSelection).toBeNull();
  });
  it("keeps native readiness process-wide and external health scoped to its exact selection", async () => {
    const workbench = configuredSnapshot();
    workbench.vmmMemory.selectedProjectId = "peer";
    workbench.vmmMemory.selectedConversationId = "peer";
    expect((await project(workbench, "preview", false)).vmmStatus).toBe("ready");
    workbench.vmmMemory.globalDraft.backendMode = "external_grpc";
    expect((await project(workbench, "preview", false)).vmmStatus).toBeUndefined();
    workbench.vmmMemory.selectedProjectId = "preview";
    workbench.vmmMemory.selectedConversationId = "preview";
    expect((await project(workbench, "preview", false)).vmmStatus).toBe("ready");
    workbench.vmmMemory.runtime.processReady = false;
    expect((await project(workbench, "preview", false)).vmmStatus).toBe("not_ready");
  });
  it("reports memory as not ready while a scope blocker stops it on a ready process", async () => {
    const workbench = configuredSnapshot();
    workbench.vmmMemory.runtime.scopeError = "vector rebuild required";
    expect((await project(workbench, "preview", false)).vmmStatus).toBe("not_ready");
  });
  it("hands the session runtime cue to the composer's floating status slot", async () => {
    // cue is an opaque element; the layer must pass the same node through unchanged.
    // cue 是不透明元素；该层必须原样传递同一节点。
    const cue = <span data-runtime-cue="" />;
    expect((await project(configuredSnapshot(), "preview", false, cue)).runtimeStatus).toBe(cue);
    expect((await project(configuredSnapshot(), "preview", false)).runtimeStatus).toBeNull();
  });
  it("does not treat missing session policy as enabled or an explicit disabled state", async () => {
    const workbench = configuredSnapshot();
    workbench.sessionVmmPolicy = null;
    expect((await project(workbench, "preview", false)).vmmAvailable).toBe(false);
    expect((await project(workbench, "preview", false)).vmmStatus).toBeUndefined();
    workbench.sessionVmmPolicy = { memoryEnabled: false, precheckMode: "inherit", precheckEnabled: false, memoryWritebackMode: "inherit", memoryWritebackEnabled: false };
    expect((await project(workbench, "preview", false)).vmmStatus).toBe("disabled");
  });
});
