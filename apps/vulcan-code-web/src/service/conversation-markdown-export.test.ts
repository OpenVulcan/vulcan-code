import { expect, it, vi } from "vitest";

vi.mock("./webui-service-runtime", () => ({ serviceClient: {} }));

import type { ConversationEventWindowRpc } from "./conversation-event-window-client";
import type { ConversationEventSummary } from "./conversation-event-window-store";
import { conversationMarkdown, loadConversationEvents } from "./conversation-markdown-export";

// summary builds one event summary with only the fields the export reads.
// summary 构建只含导出所读字段的事件摘要。
function summary(seq: number, event: Record<string, unknown>, visibility = "provider"): ConversationEventSummary {
  return { seq, visibility, event } as unknown as ConversationEventSummary;
}

const labels = { user: "User", assistant: "Assistant" };

it("keeps what each side said, grouped under one heading per run of messages", () => {
  const markdown = conversationMarkdown("Title", [
    summary(1, { kind: "user_input", text: "Question?" }),
    summary(2, { kind: "assistant_reasoning", text: "thinking" }),
    summary(3, { kind: "assistant_text", text: "Let me look." }),
    summary(4, { kind: "tool_call", tool_name: "read" }),
    summary(5, { kind: "assistant_text", text: "Answer." }),
    summary(6, { kind: "user_input", text: "hidden" }, "host_only"),
    summary(7, { kind: "assistant_text", text: "   " }),
    summary(8, { kind: "user_input", text: "Thanks" }),
  ], labels);
  expect(markdown).toBe("# Title\n\n## User\n\nQuestion?\n\n## Assistant\n\nLet me look.\n\nAnswer.\n\n## User\n\nThanks\n");
});

it("reads every older page back to the start, oldest first", async () => {
  const owner = { session_id: "s", window_generation: 3 };
  const page = vi.fn(async (_owner: unknown, beforeSeq: number) => beforeSeq === 20
    ? { ...owner, events: [summary(10, {})], has_more: true, older_before_seq: 10 }
    : { ...owner, events: [summary(1, {})], has_more: false, older_before_seq: null });
  const rpc = {
    hydrate: async () => ({ ...owner, events: [summary(20, {})], has_more: true, older_before_seq: 20 }),
    page,
  } as unknown as ConversationEventWindowRpc;
  const events = await loadConversationEvents("s", rpc);
  expect(events.map((event) => event.seq)).toEqual([1, 10, 20]);
  expect(page).toHaveBeenCalledWith(owner, 20);
});

it("stops with an error when a page does not move to older events", async () => {
  const owner = { session_id: "s", window_generation: 1 };
  const rpc = {
    hydrate: async () => ({ ...owner, events: [], has_more: true, older_before_seq: 5 }),
    page: async () => ({ ...owner, events: [], has_more: true, older_before_seq: 5 }),
  } as unknown as ConversationEventWindowRpc;
  await expect(loadConversationEvents("s", rpc)).rejects.toThrow();
});
