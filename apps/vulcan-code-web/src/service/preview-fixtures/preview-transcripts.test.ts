import { describe, expect, it } from "vitest";
import { ConversationEventWindowStore } from "@/service/conversation-event-window-store";
import { PREVIEW_TRANSCRIPT_SESSION_IDS, previewConversationWindow } from "./preview-transcripts";

// Preview transcripts must satisfy the same window invariants as service hydration, or the preview would show a
// surface no live session can produce.
// 预览转录必须满足与服务 hydration 相同的窗口不变量，否则预览会展示实时会话无法产生的界面。
describe("preview transcripts", () => {
  it.each(PREVIEW_TRANSCRIPT_SESSION_IDS)("hydrates %s into the event-window store", (sessionId) => {
    const window = previewConversationWindow(sessionId);
    const store = new ConversationEventWindowStore();
    store.hydrate(window);
    expect(store.getOwner()).toEqual({ session_id: sessionId, window_generation: window.window_generation });
    expect(store.size()).toBe(window.events.length);
  });

  it("returns an empty window for a session without a transcript", () => {
    const window = previewConversationWindow("preview-without-transcript");
    expect(window.events).toEqual([]);
    expect(() => new ConversationEventWindowStore().hydrate(window)).not.toThrow();
  });
});
