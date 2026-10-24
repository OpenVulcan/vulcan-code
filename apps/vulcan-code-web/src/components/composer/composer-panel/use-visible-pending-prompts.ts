import React from "react";
import type { ConversationEventSummary } from "@/service/conversation-event-window-store";
import { useSelectedConversationEventWindowSelector } from "@/state/conversation-event-window-react";
import type { PendingPromptSummary } from "@/types/composer-skills";

/** Separator for id lists; client action ids never contain control characters.
 * id 列表分隔符；客户端动作 id 绝不包含控制字符。 */
const ID_SEPARATOR = "\n";

/** Lists the inserted rows whose input already reached the conversation.
 * 列出输入已进入会话的已插入行。
 *
 * @param waitingIds ids of rows still shown as waiting for insertion.
 *                   仍显示为等待插入的行 id。
 * @param events conversation events of the selected window.
 *               选中窗口的会话事件。
 * @returns the streamed ids, joined so that equal results compare equal.
 *          已写入事件流的 id，拼接后使相同结果比较相等。 */
export function streamedInsertedPromptIds(
  waitingIds: ReadonlySet<string>,
  events: readonly ConversationEventSummary[],
): string {
  if (waitingIds.size === 0) return "";
  const streamed: string[] = [];
  for (const summary of events) {
    const event = summary.event;
    if (event.kind === "user_input" && waitingIds.has(event.client_action_id)) streamed.push(event.client_action_id);
  }
  return streamed.join(ID_SEPARATOR);
}

/** Keeps an inserted row until its prompt shows in the conversation, then hides it in the same frame.
 * 已插入行保留到其提示词出现在会话中，并在同一帧隐藏。
 *
 * The service drops such rows when the stream ends; inside a long tool loop the conversation shows
 * the prompt earlier, and the row must not linger beside it.
 * 服务在流结束时移除这类行；在较长的工具循环中会话更早显示该提示词，此时该行不得继续并存。
 *
 * @param entries service-owned queue rows.
 *                服务持有的队列行。
 * @returns the rows to display.
 *          需要显示的行。 */
export function useVisiblePendingPrompts(entries: PendingPromptSummary[]): PendingPromptSummary[] {
  const waitingKey = entries.filter((entry) => entry.state === "inserted").map((entry) => entry.id).join(ID_SEPARATOR);
  const select = React.useCallback(
    (snapshot: { events: readonly ConversationEventSummary[] }) =>
      streamedInsertedPromptIds(new Set(waitingKey === "" ? [] : waitingKey.split(ID_SEPARATOR)), snapshot.events),
    [waitingKey],
  );
  const streamedKey = useSelectedConversationEventWindowSelector(select);
  return React.useMemo(() => {
    if (streamedKey === "") return entries;
    const streamed = new Set(streamedKey.split(ID_SEPARATOR));
    return entries.filter((entry) => !(entry.state === "inserted" && streamed.has(entry.id)));
  }, [entries, streamedKey]);
}
