// @vitest-environment jsdom
// Behaviour of the tool chain card: routing, planned steps while running, and the finished list that keeps step order,
// marks each outcome and wraps the step's own tool card.
// 工具链卡的行为：路由、运行期间的计划步骤，以及保持步骤顺序、标记每个结果并包裹步骤自身工具卡的完成列表。
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { readToolCall } from "../tool-call";
import { resolveToolCard } from "../tool-call-card";
import { chainSteps, stepCall, ToolChainCard } from "./tool-chain-card";

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

const INPUT = {
  steps: [
    { id: "build", tool: "shell_command", arguments: { command: "cargo build" } },
    { id: "test", tool: "shell_command", arguments: { command: "cargo test" } },
    { tool: "file_read", if: "failure()", arguments: { path: "log.txt" } },
  ],
};

// chainEvent builds a tool_chain event, finished with the given host payload or still running.
// chainEvent 构建一条 tool_chain 事件：带给定宿主载荷的完成事件，或仍在运行的事件。
function chainEvent(payload: Record<string, unknown> | null): ToolCallEvent {
  return {
    api_call_id: 1,
    input: INPUT,
    kind: "tool_call",
    ...(payload ? { output: { host_result: { kind: "tool_chain", payload }, is_error: false, model_output: "report", tool_call_id: "call-1", tool_name: "tool_chain" } } : {}),
    request_stream_id: "stream-1",
    started_at_ms: 0,
    status: payload ? "completed" : "running",
    tool_call_id: 1,
    tool_name: "tool_chain",
  } as unknown as ToolCallEvent;
}

const PAYLOAD = {
  steps: [
    {
      index: 0, id: "build", tool: "shell_command", outcome: "success", exit_code: 0, detail: null,
      arguments: { command: "cargo build" },
      result: { model_output: "built", host_result: { kind: "command_run", payload: { command: "cargo build", exit_code: 0, status: "completed" } }, is_error: false, duration_ms: 5 },
    },
    {
      index: 1, id: "test", tool: "shell_command", outcome: "failure", exit_code: 1, detail: null,
      arguments: { command: "cargo test" },
      result: { model_output: "1 failed", host_result: null, is_error: true, duration_ms: 7 },
    },
    { index: 2, id: null, tool: "file_read", outcome: "skipped", exit_code: null, detail: "condition was false", arguments: { path: "log.txt" }, result: null },
  ],
};

// render mounts the chain card with its body open.
// render 挂载工具链卡并展开其正文。
function render(event: ToolCallEvent): void {
  const call = readToolCall(event);
  act(() => root.render(
    <I18nProvider initialLocale="en-US">
      <ToolChainCard call={call} family="run" onOpenChange={() => undefined} open />
    </I18nProvider>,
  ));
}

describe("tool chain card", () => {
  it("routes tool_chain calls to the chain card", () => {
    expect(resolveToolCard(readToolCall(chainEvent(null))).Card).toBe(ToolChainCard);
    expect(resolveToolCard(readToolCall(chainEvent(PAYLOAD))).Card).toBe(ToolChainCard);
  });

  it("lists the planned steps as pending while the chain runs", () => {
    const steps = chainSteps(readToolCall(chainEvent(null)));
    expect(steps.map((step) => [step.tool, step.outcome])).toEqual([
      ["shell_command", "pending"], ["shell_command", "pending"], ["file_read", "pending"],
    ]);
  });

  it("gives each step a call of its own that a direct call would have", () => {
    const parent = readToolCall(chainEvent(PAYLOAD));
    const [build, failed, skipped] = chainSteps(parent).map((step) => stepCall(parent, step));
    expect(build.name).toBe("shell_command");
    expect(build.state).toBe("done");
    expect(build.host?.kind).toBe("command_run");
    expect(failed.state).toBe("failed");
    expect(failed.isError).toBe(true);
    expect(skipped.state).toBe("stopped");
    expect(new Set([build.key, failed.key, skipped.key]).size).toBe(3);
  });

  it("renders the steps in order with their outcomes and nests the step cards that ran", () => {
    render(chainEvent(PAYLOAD));
    const rows = [...host.querySelectorAll<HTMLElement>(".vk-chain-step")];
    expect(rows.map((row) => row.dataset.outcome)).toEqual(["success", "failure", "skipped"]);
    expect(rows.map((row) => row.dataset.state)).toEqual(["done", "failed", undefined]);
    expect(rows[0].querySelector(".vk-tool")).not.toBeNull();
    expect(rows[1].querySelector(".vk-tool")).not.toBeNull();
    expect(rows[2].querySelector(".vk-tool")).toBeNull();
    expect(rows[2].querySelector(".vk-chain-step-tool")?.textContent).toBe("file_read");
  });
});
