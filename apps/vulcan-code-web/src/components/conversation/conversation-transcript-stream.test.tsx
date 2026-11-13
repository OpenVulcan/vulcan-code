// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { ConversationEventSummary } from "@/service/conversation-event-window-store";
import type { ConversationEventWindowSnapshotReader } from "@/state/conversation-event-window-react";
import { parseConversationBlocks } from "./conversation-transcript-parser";
import { ConversationTranscriptStreamContext, useConversationStreamingAnswerText, useConversationTranscriptSnapshot } from "./conversation-transcript-stream";

/** Builds a valid V1 reply revision with the exact event identity shared by the transcript and its leaf.
 * 构造有效 V1 回复修订，转录与叶子共用精确事件身份。
 * @param text reply content. 回复正文。
 * @param eventRevision revision of the same reply. 同一回复的修订号。
 * @param status canonical lifecycle state. 规范生命周期状态。
 * @returns a reply at sequence 20 in a paged window. 分页窗口中序号为 20 的回复。 */
function reply(text: string, eventRevision: number, status: "streaming" | "completed" = "streaming"): ConversationEventSummary {
  return { seq: 20, turn_id: 10, event_revision: eventRevision, visibility: "provider", created_at_ms: 10,
    updated_at_ms: 10 + eventRevision, event: { kind: "assistant_text", api_call_id: 5, status, text } };
}

it("updates the mounted Markdown leaf in place and reprojects on structure, sealing or owner changes", async () => {
  // Keep the synthetic canonical source and the observed structural renders local to this test.
  // 将合成规范来源与观测到的结构渲染限制在本测试内。
  let current: ConversationEventWindowControllerSnapshot = {
    owner: { session_id: "first", window_generation: 1 }, service_epoch: "service",
    memory_revision: 10, durable_revision: 10, events: [reply("", 1)], older_before_seq: 20, has_more: true,
    open_turn_id: 10, open_turn_start_seq: 20, session_config: null,
  };
  const listeners = new Set<() => void>();
  const read = () => current;
  const reader: ConversationEventWindowSnapshotReader = {
    getSnapshot: read,
    getServerSnapshot: read,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
  };
  let structureRenders = 0;

  /** Renders one parsed block using the production exact-event subscription.
   * 通过生产精确事件订阅渲染一个解析块。
   * @param props owner, projected body and optional exact event sequence. 归属、投影正文及可选精确序号。
   * @returns visible body for behavior assertions. 用于行为断言的可见正文。 */
  function Leaf(props: { conversationId: string; body: string; seq?: number }) {
    return <output>{useConversationStreamingAnswerText(props.conversationId, props.seq, props.body)}</output>;
  }

  /** Projects the reply using the same structural subscription as the selected transcript.
   * 使用与选中转录相同的结构订阅投影回复。
   * @returns parsed block kinds and the subscribed body. 解析块类型与订阅到的正文。 */
  function Transcript() {
    const snapshot = useConversationTranscriptSnapshot(reader, true);
    structureRenders += 1;
    const summary = snapshot.events[0];
    if (snapshot.owner === null || summary.event.kind !== "assistant_text") throw new Error("Missing fixture reply");
    const conversationId = snapshot.owner.session_id;
    const blocks = parseConversationBlocks({
      id: "reply", conversationId, role: "assistant", body: summary.event.text, createdAt: "10",
      metadata: { v1_event: true, v1_seq: summary.seq, stream_state: summary.event.status === "streaming" ? "active" : "sealed" },
    });
    return <ConversationTranscriptStreamContext.Provider value={{ reader, owner: snapshot.owner, serviceEpoch: snapshot.service_epoch }}>
      <div data-kinds={blocks.map((block) => block.kind).join(",")}>
        {blocks.map((block) => <Leaf key={block.id} conversationId={conversationId} body={block.body} seq={block.streamingAnswerSeq} />)}
      </div>
    </ConversationTranscriptStreamContext.Provider>;
  }

  /** Publishes one canonical snapshot and waits for React to commit it.
   * 发布一份规范快照并等待 React 提交。
   * @param next event or owner evolution to publish. 待发布的事件或归属演进。
   * @returns after React has flushed. React 完成刷新后返回。 */
  async function publish(next: Partial<ConversationEventWindowControllerSnapshot>) {
    await act(async () => {
      current = { ...current, ...next };
      for (const listener of Array.from(listeners)) listener();
    });
  }

  // Mount the real subscription path, then verify the same leaf handles subsequent text.
  // 挂载实际订阅路径，随后验证同一个叶子处理后续文字。
  const host = document.createElement("div");
  const root = createRoot(host);
  try {
    await act(async () => root.render(<Transcript />));
    const leaf = host.querySelector("output");
    const initialRenders = structureRenders;
    await publish({ events: [reply("hello\r\n", 2)], memory_revision: 11 });
    await publish({ events: [reply("hello\r\nworld", 3)], memory_revision: 12 });
    expect(structureRenders).toBe(initialRenders);
    expect(host.querySelector("output")).toBe(leaf);
    expect(leaf?.textContent).toBe("hello\nworld");

    await publish({ events: [reply("hello\r\nworld\n\n```diff\n-old\n+new\n```", 4)], memory_revision: 13 });
    expect(host.firstElementChild?.getAttribute("data-kinds")).toBe("markdown,diff");
    await publish({ events: [reply("completed reply", 5, "completed")], memory_revision: 14 });
    expect(host.textContent).toBe("completed reply");

    await publish({ owner: { session_id: "second", window_generation: 1 }, events: [reply("other session", 1)] });
    expect(host.textContent).toBe("other session");
    await publish({ owner: { session_id: "second", window_generation: 2 }, events: [reply("reconnected", 1)] });
    expect(host.textContent).toBe("reconnected");
  } finally {
    await act(async () => root.unmount());
  }
});
