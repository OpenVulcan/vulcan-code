// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { I18nProvider } from "@/i18n";
import {
  ConversationLiveStreamProvider,
  useConversationTailStatusKind,
} from "@/state/conversation-live-stream-context";
import { ConversationLiveStreamStore } from "@/state/conversation-live-stream-store";
import { ConversationTailStatus } from "@/components/conversation/chat-area/tail-status";
import type { WorkbenchDeltaEnvelope } from "@/types";

// generation numbers every published envelope, as the transport does.
// generation 为每条发布的信封编号，与传输层一致。
let generation = 0;

/** Publishes one reasoning-token update for the session and flushes it to subscribers.
 * 为会话发布一次思考 token 更新并通知订阅者。
 * store receives the envelope; reasoningTokens is the new count, or null to clear the status. */
function publishReasoning(store: ConversationLiveStreamStore, reasoningTokens: number | null) {
  generation += 1;
  const delta = {
    kind: "transientReasoningStatusChanged",
    payload: {
      conversationId: "session-1",
      status: reasoningTokens === null ? null : { kind: "encrypted", reasoningTokens },
    },
  } as WorkbenchDeltaEnvelope["delta"];
  store.applyEnvelope({ generation, delta });
  store.flushNotifications();
}

describe("transcript tail status scope", () => {
  let host: HTMLDivElement;
  let root: ReturnType<typeof createRoot>;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(async () => {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  });

  it("redraws only the tail line while the reasoning token count changes", async () => {
    const store = new ConversationLiveStreamStore();
    // ownerRenders counts renders of the component that decides whether a tail status exists, as ChatArea does.
    // ownerRenders 统计判断尾部状态是否存在的组件（如 ChatArea）的渲染次数。
    let ownerRenders = 0;
    function Owner() {
      ownerRenders += 1;
      const kind = useConversationTailStatusKind("session-1");
      return <div data-kind={kind ?? "none"}><ConversationTailStatus conversationId="session-1" /></div>;
    }
    await React.act(async () => root.render(
      <ConversationLiveStreamProvider store={store}>
        <I18nProvider initialLocale="en-US"><Owner /></I18nProvider>
      </ConversationLiveStreamProvider>,
    ));
    await React.act(async () => publishReasoning(store, 1));
    const rendersWithStatus = ownerRenders;
    const firstText = host.textContent;
    for (const tokens of [2, 3, 4]) {
      await React.act(async () => publishReasoning(store, tokens));
    }
    expect(ownerRenders).toBe(rendersWithStatus);
    expect(host.textContent).not.toBe(firstText);
    await React.act(async () => publishReasoning(store, null));
    expect(host.querySelector("[data-kind]")?.getAttribute("data-kind")).toBe("none");
  });
});
