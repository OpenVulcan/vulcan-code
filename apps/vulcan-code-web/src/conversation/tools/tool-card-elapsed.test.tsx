// @vitest-environment jsdom
// Behaviour of the tool-card call timer: it ticks until the call ends, then stays only for a slow call.
// 工具卡调用计时器的行为：调用结束前持续走动，结束后只在慢调用上保留。
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Wrench } from "lucide-react";
import { I18nProvider } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { ToolFrame } from "./tool-card";
import { formatToolDuration, readToolCall, SLOW_TOOL_CALL_MS } from "./tool-call";

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  vi.useFakeTimers();
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.useRealTimers();
});

// toolEvent builds a minimal tool-call event with the given lifecycle and timing.
// toolEvent 以给定生命周期与时间构建一条最小工具调用事件。
function toolEvent(
  status: ToolCallEvent["status"],
  startedAtMs: number,
  finishedAtMs?: number,
  userWaitMs?: number,
): ToolCallEvent {
  return {
    api_call_id: 1,
    input: {},
    kind: "tool_call",
    request_stream_id: "stream-1",
    started_at_ms: startedAtMs,
    ...(finishedAtMs === undefined ? {} : { finished_at_ms: finishedAtMs }),
    ...(userWaitMs === undefined ? {} : { user_wait_ms: userWaitMs }),
    status,
    tool_call_id: 1,
    tool_name: "image_generate",
  } as unknown as ToolCallEvent;
}

// renderCall mounts one frame for the call and returns the timer text, or null when no timer shows.
// renderCall 为该调用挂载一个框架，并返回计时文本；不显示计时器时返回 null。
function renderCall(event: ToolCallEvent): () => string | null {
  const call = readToolCall(event);
  act(() => root.render(
    <I18nProvider initialLocale="en-US">
      <ToolFrame call={call} family="media" icon={Wrench} onOpenChange={() => undefined} open={false} state={call.state} verb="tool" />
    </I18nProvider>,
  ));
  return () => host.querySelector(".vk-tool-elapsed")?.textContent ?? null;
}

it("ticks while the call runs", () => {
  vi.setSystemTime(10_000);
  const elapsed = renderCall(toolEvent("running", 10_000));
  expect(elapsed()).toBe(formatToolDuration(0));
  act(() => vi.advanceTimersByTime(3_500));
  expect(elapsed()).toBe(formatToolDuration(3_500));
  expect(host.querySelector(".vk-tool-elapsed[data-live]")).not.toBeNull();
});

it("hides the timer while the call waits on the user and never counts the wait", () => {
  vi.setSystemTime(10_000);
  expect(renderCall(toolEvent("waiting_user", 0))()).toBeNull();
  // Approved after 9 s of waiting: the live timer shows the 1 s of execution only.
  // 等待 9 秒后获批：实时计时只显示 1 秒执行时间。
  expect(renderCall(toolEvent("running", 0, undefined, 9_000))()).toBe(formatToolDuration(1_000));
  // An Ask answered after a long wait executed only briefly, so no duration stays.
  // 长时间等待后才回答的 Ask 执行时间很短，因此不保留耗时。
  expect(renderCall(toolEvent("completed", 0, 60_000, 59_900))()).toBeNull();
});

it("keeps the duration only for a slow finished call", () => {
  expect(renderCall(toolEvent("completed", 0, SLOW_TOOL_CALL_MS))()).toBeNull();
  const slow = renderCall(toolEvent("completed", 0, SLOW_TOOL_CALL_MS + 1_000));
  expect(slow()).toBe(formatToolDuration(SLOW_TOOL_CALL_MS + 1_000));
  expect(host.querySelector(".vk-tool-elapsed[data-live]")).toBeNull();
});
