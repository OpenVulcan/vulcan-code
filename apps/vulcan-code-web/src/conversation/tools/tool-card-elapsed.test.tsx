// @vitest-environment jsdom
// Behaviour of the tool-card call timer: it ticks until the call ends, then stays only for a slow call.
// 工具卡调用计时器的行为：调用结束前持续走动，结束后只在慢调用上保留。
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Wrench } from "lucide-react";
import { formatMessage, I18nProvider } from "@/i18n";
import { loadedLocaleCatalog } from "@/i18n-catalogs";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { ToolFrame } from "./tool-card";
import { ToolCallCard, ToolRequestCard } from "./tool-call-card";
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

// Mounts the command's real card with the active lifecycle and configured arguments.
// 使用当前生命周期与配置参数挂载真实命令卡。
it("shows the configured command timeout next to elapsed time only during the active call", () => {
  // A completed command still has its original input, so lifecycle must control label visibility.
  // 已结束的命令仍持有原始输入，因此标签是否显示由生命周期决定。
  const event: ToolCallEvent = {
    ...toolEvent("running", Date.now() - 4_000),
    tool_name: "shell_command",
    input: { command: "node --test", title: "Run tests", timeout_ms: 300_000 },
  };
  // Renders the given command event and reads its timeout label, or null when hidden.
  // 渲染指定命令事件并读取超时标签，标签隐藏时返回 null。
  const timeoutText = (commandEvent: ToolCallEvent) => {
    act(() => root.render(
      <I18nProvider initialLocale="en-US">
        <ToolCallCard event={commandEvent} onOpenChange={() => undefined} open={false} />
      </I18nProvider>,
    ));
    return host.querySelector(".vk-tool-timeout")?.textContent ?? null;
  };
  // Uses the catalog so translation wording can evolve independently of card placement.
  // 复用文案目录，让翻译措辞独立于卡片布局演进。
  const catalog = loadedLocaleCatalog("en-US")!;
  expect(timeoutText(event)).toBe(formatMessage(catalog["toolCard.shell.timeout"], {
    duration: formatToolDuration(300_000),
  }));
  expect(host.querySelector(".vk-tool-timeout")?.nextElementSibling?.textContent).toBe(formatToolDuration(4_000));
  expect(timeoutText({ ...event, input: { command: "node --test", title: "Run tests" } })).toBe(catalog["toolCard.shell.noTimeout"]);
  expect(timeoutText({ ...event, input: { ...event.input, timeout_ms: 0 } })).toBeNull();
  expect(timeoutText({ ...event, status: "waiting_user" })).toBeNull();
  expect(timeoutText({ ...event, status: "completed" })).toBeNull();
  act(() => root.render(
    <I18nProvider initialLocale="en-US">
      <ToolRequestCard
        onOpenChange={() => undefined}
        open={false}
        phase="receiving"
        preview={null}
        receivedBytes={10}
        requestStreamId="draft-command"
        state="running"
        toolName="shell_command"
      />
    </I18nProvider>,
  ));
  expect(host.querySelector(".vk-tool-timeout")).toBeNull();
});
