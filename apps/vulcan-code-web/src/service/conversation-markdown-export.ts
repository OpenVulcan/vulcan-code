import { createConversationEventWindowClient, type ConversationEventWindowRpc } from "./conversation-event-window-client";
import type { ConversationEventSummary } from "./conversation-event-window-store";
import { serviceClient } from "./webui-service-runtime";

// loadConversationEvents reads a conversation's whole history: the latest window, then every older page back to the
// start. Both reads are read-only on the service, so an open transcript of the same conversation is unaffected.
// loadConversationEvents 读取会话的完整历史：先读最近窗口，再逐页向前读到开头。两种读取在服务端均为只读，
// 因此同一会话已打开的转录不受影响。
export async function loadConversationEvents(
  sessionId: string,
  rpc: ConversationEventWindowRpc = createConversationEventWindowClient(serviceClient),
): Promise<ConversationEventSummary[]> {
  const latest = await rpc.hydrate(sessionId);
  const owner = { session_id: latest.session_id, window_generation: latest.window_generation };
  const pages: (readonly ConversationEventSummary[])[] = [latest.events];
  let cursor = latest.has_more ? latest.older_before_seq : null;
  while (cursor !== null) {
    const page = await rpc.page(owner, cursor);
    pages.unshift(page.events);
    const next = page.has_more ? page.older_before_seq : null;
    if (next !== null && next >= cursor) throw new Error("Conversation history paging did not move to older events.");
    cursor = next;
  }
  return pages.flat();
}

// ConversationMarkdownLabels names the two speakers in the exported text.
// ConversationMarkdownLabels 为导出文本中的两方发言者命名。
export interface ConversationMarkdownLabels {
  user: string;
  assistant: string;
}

// conversationMarkdown turns a conversation into Markdown: the title, then each run of messages from one speaker
// under that speaker's heading. Only what was said is kept; reasoning, tool calls and host-only events are left out.
// conversationMarkdown 将会话转为 Markdown：先是标题，然后把同一发言者的连续消息放在其标题下。只保留说出的内容；
// 思考、工具调用与仅宿主可见的事件均不包含。
export function conversationMarkdown(
  title: string,
  events: readonly ConversationEventSummary[],
  labels: ConversationMarkdownLabels,
): string {
  const parts = [`# ${title}`];
  let speaker: keyof ConversationMarkdownLabels | null = null;
  for (const { event, visibility } of events) {
    if (visibility === "host_only") continue;
    const role = event.kind === "user_input" ? "user" : event.kind === "assistant_text" ? "assistant" : null;
    if (role === null || !("text" in event)) continue;
    const text = event.text.trim();
    if (!text) continue;
    if (role !== speaker) {
      parts.push(`## ${labels[role]}`);
      speaker = role;
    }
    parts.push(text);
  }
  return `${parts.join("\n\n")}\n`;
}
