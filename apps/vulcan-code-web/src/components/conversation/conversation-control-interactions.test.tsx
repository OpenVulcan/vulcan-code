// @vitest-environment jsdom

import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { I18nProvider } from "@/i18n";
import { MarkdownMessage } from "./conversation-markdown";
import { MarkdownDiagramFrame } from "./conversation-diagram-frame";
import { PlanArtifactBlock } from "./conversation-plan-artifact";
import { ToolPresentationCard } from "./conversation-tool-presentation-card";
import type { GenericToolMarkdownPresentation } from "./conversation-tool-presentations";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ConversationGenerationDialogV1 } from "./conversation-generation-dialog-v1";
import { ConversationBranchWorkspaceV1 } from "./conversation-branch-workspace-v1";
import { ConversationLiveStreamProvider } from "@/state/conversation-live-stream-context";
import { ConversationLiveStreamStore } from "@/state/conversation-live-stream-store";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import { DEFAULT_THINKING_DISPLAY_MODE } from "./thinking-display";
import { ConversationSessionTerminalSection } from "./session-todolist-panel/terminal-section";

vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

// container and root mount the real controls without a network or native clipboard.
// container 和 root 挂载真实控件，不使用网络或原生剪贴板。
let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

/** Mounts actual controls with the production translation provider.
 * 使用生产翻译提供器挂载真实控件。 */
async function render(content: React.ReactNode): Promise<void> {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN">{content}</I18nProvider>));
}

/** Clicks the exact labelled control and drains its asynchronous state updates.
 * 点击精确标签的控件，并完成异步状态更新。 */
async function click(label: string): Promise<void> {
  // button must exist; missing controls fail the test rather than becoming no-op clicks.
  // button 必须存在；控件缺失时测试失败，而不是静默跳过点击。
  const button = container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
  expect(button).not.toBeNull();
  await act(async () => button!.click());
}

describe("conversation control rejection paths", () => {
  it("retains a rejected terminal stop confirmation and prevents duplicate requests", async () => {
    // rejectStop controls the request without starting or stopping a real command session.
    // rejectStop 控制请求，不启动或停止任何真实命令会话。
    let rejectStop!: (reason: Error) => void;
    // stop records the exact command-session identity and nullable owner from the actual row contract.
    // stop 记录真实行合同中的精确命令会话身份和可空所有者。
    const stop = vi.fn().mockImplementationOnce(() => new Promise<void>((_resolve, reject) => { rejectStop = reject; })).mockResolvedValue(undefined);
    await render(<TooltipProvider><ConversationSessionTerminalSection terminalSessions={[{ id: "audit-terminal", name: "Audit terminal", command: "echo audit", state: "running", label: "运行中", preview: "", exitCode: null, canStop: true }]} summariesById={new Map()} onStopCommandSession={stop} /></TooltipProvider>);
    await click("强制终止终端会话");
    // confirmButton belongs to the real destructive confirmation dialog.
    // confirmButton 属于真实破坏性确认弹窗。
    const confirmButton = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find((button) => button.textContent === "确认终止")!;
    await act(async () => { confirmButton.click(); confirmButton.click(); });
    expect(stop).toHaveBeenCalledTimes(1);
    expect(stop).toHaveBeenCalledWith("audit-terminal", null);
    await act(async () => rejectStop(new Error("stop denied")));
    expect(document.querySelector('[role="dialog"]')).not.toBeNull();
    expect(confirmButton.disabled).toBe(false);
    expect(toast.error).toHaveBeenCalledWith("操作失败", { description: "stop denied" });
    await act(async () => confirmButton.click());
    expect(stop).toHaveBeenCalledTimes(2);
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it("serializes generation confirmation and restores its exact draft after rejection", async () => {
    // rejectConfirmation controls the exact in-flight generation request.
    // rejectConfirmation 控制精确的进行中代次请求。
    let rejectConfirmation!: (reason: Error) => void;
    // confirm retains the trimmed fork title while the parent callback is pending.
    // confirm 在父回调等待期间保留去掉两端空白的分叉标题。
    const confirm = vi.fn(() => new Promise<void>((_resolve, reject) => { rejectConfirmation = reject; }));
    await render(<ConversationGenerationDialogV1 action={{ kind: "fork", title: " Audit child " }} submitting={false} onCancel={vi.fn()} onConfirm={confirm} />);
    // confirmButton lives in the real dialog portal rather than the local test container.
    // confirmButton 位于真实弹窗 portal，而不是本地测试容器。
    const confirmButton = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find((button) => button.textContent === "分叉会话")!;
    await act(async () => { confirmButton.click(); confirmButton.click(); });
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(confirm).toHaveBeenCalledWith("Audit child");
    expect(confirmButton.disabled).toBe(true);
    await act(async () => rejectConfirmation(new Error("generation denied")));
    expect(confirmButton.disabled).toBe(false);
    expect(document.querySelector<HTMLInputElement>('[role="dialog"] input')!.value).toBe(" Audit child ");
    expect(toast.error).toHaveBeenCalledWith("操作失败", { description: "generation denied" });
  });

  it("reports historical-page rejection and re-enables the same owner-bound load action", async () => {
    // snapshot follows the actual event-window controller contract and contains no invented events.
    // snapshot 遵循真实事件窗口控制器合同，不包含虚构事件。
    const snapshot: ConversationEventWindowControllerSnapshot = {
      owner: { session_id: "audit-pagination", window_generation: 1 }, service_epoch: "audit", memory_revision: 1,
      durable_revision: 1, events: [], older_before_seq: 2, has_more: true, open_turn_id: null,
      open_turn_start_seq: null, session_config: null,
    };
    // prependOlder fails once and then completes through the same owner-bound command.
    // prependOlder 首次失败，随后通过同一归属绑定命令完成。
    const prependOlder = vi.fn().mockRejectedValueOnce(new Error("history unavailable")).mockResolvedValue(true);
    await render(<TooltipProvider><ConversationLiveStreamProvider store={new ConversationLiveStreamStore()}>
      <ConversationBranchWorkspaceV1 sessionId="audit-pagination" conversation={{ id: "audit-pagination", title: "audit", status: "completed" }}
        snapshot={snapshot} commands={{ prependOlder, rehydrate: vi.fn() }} commandSessions={[]} initialExpandedState={null}
        onExpandedStateChange={vi.fn()} reasoningDisplayMode={DEFAULT_THINKING_DISPLAY_MODE} sessionRunning={false}
        onCopyText={vi.fn()} onOpenSubagentPanel={async () => true} onPreserveHistoryPrepend={(load) => load()}
        onScrollStateChange={vi.fn()} scrollRef={React.createRef<HTMLElement>()} />
    </ConversationLiveStreamProvider></TooltipProvider>);
    // loadButton is the real history control, which remains usable after a rejection.
    // loadButton 是真实历史控件，在拒绝后仍可使用。
    const loadButton = container.querySelector<HTMLButtonElement>(".conversation-history-load-button")!;
    await act(async () => loadButton.click());
    expect(toast.error).toHaveBeenCalledWith("操作失败", { description: "history unavailable" });
    expect(loadButton.disabled).toBe(false);
    await act(async () => loadButton.click());
    expect(prependOlder).toHaveBeenCalledTimes(2);
  });

  it("retains an old tool card after detail failure and rereads the exact stable identity", async () => {
    // presentation is a persisted legacy card with its original hidden-detail record identity.
    // presentation 是保留原始隐藏详情记录身份的持久化旧卡片。
    const presentation: GenericToolMarkdownPresentation = {
      id: "control-audit-old-card", kind: "generic_tool_markdown", markdown: "Retained summary",
      state: "completed", toolName: "custom_tool", detailRef: { sessionId: "audit-session", recordId: "audit-record" },
    };
    // loader rejects once without changing the tool result, then returns the complete original output.
    // loader 首次拒绝且不改变工具结果，随后返回完整原始输出。
    const loader = vi.fn().mockRejectedValueOnce(new Error("detail unavailable")).mockResolvedValue({ body: "Recovered full output" });
    await render(<TooltipProvider><ToolPresentationCard presentation={presentation} expanded requestProgress={null} detailLoadEnabled expandedToolPresentationIds={new Set()} onExpandedChange={vi.fn()} onLoadToolDetail={loader} /></TooltipProvider>);
    expect(container.textContent).toContain("Retained summary");
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("detail unavailable");
    await act(async () => Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "重新读取")!.click());
    expect(loader).toHaveBeenCalledTimes(2);
    expect(loader).toHaveBeenNthCalledWith(2, presentation.detailRef);
    expect(container.textContent).toContain("Recovered full output");
    expect(container.querySelector('[role="alert"]')).toBeNull();
  });

  it.each([false, "throw"])("does not report copied after a rejected legacy clipboard result %s", async (result) => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: undefined });
    Object.defineProperty(document, "execCommand", { configurable: true, value: vi.fn(() => {
      if (result === "throw") throw new Error("legacy clipboard denied");
      return false;
    }) });
    await render(<MarkdownMessage content={'```text\nretained raw source\n```'} eager />);
    await click("复制代码");
    expect(container.querySelector('[aria-label="已复制"]')).toBeNull();
    expect(document.querySelector("textarea")).toBeNull();
    expect(toast.error).toHaveBeenCalledWith("复制失败", expect.objectContaining({ description: expect.any(String) }));
  });

  it("reports the original diagram error and confirms only a later successful copy", async () => {
    // copySource rejects once, then resolves to exercise recovery on the same control.
    // copySource 首次拒绝，随后成功，以验证同一控件恢复。
    const copySource = vi.fn().mockRejectedValueOnce(new Error("diagram clipboard denied")).mockResolvedValue(undefined);
    await render(<MarkdownDiagramFrame alt="diagram" copySource={copySource} iconPath="diagram.svg" imageUrl={null} loading={false} title="SVG" />);
    await click("复制代码");
    expect(container.querySelector('[aria-label="已复制"]')).toBeNull();
    expect(toast.error).toHaveBeenCalledWith("复制失败", { description: "diagram clipboard denied" });
    await click("复制代码");
    expect(container.querySelector('[aria-label="已复制"]')).not.toBeNull();
  });

  it("blocks disabled keyboard decisions and serializes a failed plan submission before retry", async () => {
    // rejectDecision releases the exact first in-flight action after duplicate gestures are checked.
    // rejectDecision 在检查重复手势后释放精确的首个进行中动作。
    let rejectDecision!: (reason: Error) => void;
    // execute records decisions without changing any backend plan state.
    // execute 记录决策，不改变任何后端计划状态。
    const execute = vi.fn().mockImplementationOnce(() => new Promise<void>((_resolve, reject) => { rejectDecision = reject; })).mockResolvedValue(undefined);
    // props follows the actual ConversationPlanArtifact contract.
    // props 遵循真实 ConversationPlanArtifact 合同。
    const props = {
      artifact: { name: "Plan", path: "D:/audit/plan.md", content: "# Exact plan", updatedAt: "audit-1" },
      pending: true, disabled: true, onCopyText: vi.fn(), onExecute: execute, onSuggest: vi.fn(), onCancel: vi.fn(),
    };
    await render(<PlanArtifactBlock {...props} />);
    await act(async () => container.querySelector(".plan-artifact-confirmation")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
    expect(execute).not.toHaveBeenCalled();
    await render(<PlanArtifactBlock {...props} disabled={false} />);
    await act(async () => {
      // button is the real execute control, clicked twice before React publishes its disabled state.
      // button 是真实执行控件，在 React 发布禁用状态前连续点击两次。
      const button = container.querySelector<HTMLButtonElement>("#plan-artifact-action-execute")!;
      button.click();
      button.click();
      container.querySelector(".plan-artifact-confirmation")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    });
    expect(execute).toHaveBeenCalledTimes(1);
    expect(container.querySelector<HTMLTextAreaElement>("textarea")!.disabled).toBe(true);
    await act(async () => rejectDecision(new Error("plan submission denied")));
    expect(toast.error).toHaveBeenCalledWith("操作失败", { description: "plan submission denied" });
    await act(async () => container.querySelector<HTMLButtonElement>("#plan-artifact-action-execute")!.click());
    expect(execute).toHaveBeenCalledTimes(2);
  });
});
