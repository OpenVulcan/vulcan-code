// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import type { ConversationScrollSnapshot } from "../chat-area-scroll";
import {
  CONVERSATION_ANCHOR_REVEAL_TOP_GAP_PX,
  useConversationScrollController,
  type ConversationScrollController,
} from "./conversation-scroll-controller";

// Reveal one prompt anchor through the production controller and release tail following.
// 通过生产控制器定位一个提示词锚点，并释放尾部跟随。
it("moves a mounted anchor near the viewport top and hands ownership to the reader", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // saved starts tail-owned so the reveal must publish an explicit ownership change.
  // saved 以尾部跟随开始，因此定位必须发布显式所有权变化。
  const saved = new Map<string, ConversationScrollSnapshot>([
    ["surface", { anchorKey: null, anchorTop: 0, scrollTop: 0, followTail: true }],
  ]);
  const onFollowTailChange = vi.fn();
  let controller: ConversationScrollController | null = null;
  // Harness mounts one scrollport with a prompt anchor whose geometry is supplied by the test.
  // Harness 挂载一个滚动视口，其中提示词锚点的几何由测试提供。
  function Harness() {
    const scrollRef = React.useRef<HTMLDivElement | null>(null);
    controller = useConversationScrollController({
      active: true,
      surfaceId: "surface",
      scrollRef,
      readSnapshot: (id) => saved.get(id) ?? null,
      writeSnapshot: (id, snapshot) => { saved.set(id, snapshot); },
      onFollowTailChange,
    });
    return (
      <div data-conversation-surface-id="surface" ref={(element) => {
        scrollRef.current = element;
        if (element) {
          Object.defineProperties(element, {
            scrollHeight: { configurable: true, value: 6000 },
            clientHeight: { configurable: true, value: 500 },
          });
          element.getBoundingClientRect = () => ({ top: 100 }) as DOMRect;
        }
      }}>
        <div className="conversation-scroll-content">
          <section data-conversation-turn-shell="turn-2" ref={(element) => {
            if (element) element.getBoundingClientRect = () => ({ top: -1400 }) as DOMRect;
          }} />
        </div>
      </div>
    );
  }
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  try {
    await React.act(async () => root.render(<Harness />));
    const scrollport = host.querySelector<HTMLElement>("[data-conversation-surface-id]")!;
    const anchor = host.querySelector<HTMLElement>("[data-conversation-turn-shell]")!;
    const startTop = scrollport.scrollTop;
    onFollowTailChange.mockClear();
    await React.act(async () => controller!.revealAnchor(anchor, "auto"));
    expect(scrollport.scrollTop).toBe(startTop + (-1400 - 100) - CONVERSATION_ANCHOR_REVEAL_TOP_GAP_PX);
    expect(onFollowTailChange).toHaveBeenCalledWith(false);
    expect(saved.get("surface")).toMatchObject({ followTail: false, scrollTop: scrollport.scrollTop });
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});
