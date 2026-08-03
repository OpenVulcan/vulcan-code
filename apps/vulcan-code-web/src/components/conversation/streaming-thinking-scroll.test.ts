import { describe, expect, it } from "vitest";
import { nextStreamingThinkingTailFollow } from "./streaming-thinking-scroll";

describe("streaming thinking tail follow", () => {
  it("does not unlock when Markdown segmentation changes geometry without user input", () => {
    expect(nextStreamingThinkingTailFollow(true, 180, false)).toBe(true);
  });

  it("unlocks only for explicit manual scrolling away from the bottom", () => {
    expect(nextStreamingThinkingTailFollow(true, 180, true)).toBe(false);
  });

  it("restores locking when the viewport reaches the bottom again", () => {
    expect(nextStreamingThinkingTailFollow(false, 12, true)).toBe(true);
  });

  // The exact bottom tolerance restores following even when the user interaction flag is still active.
  // 精确的底部容差即使用户交互标记仍然存在，也应恢复尾部跟随。
  it("restores locking at the bottom tolerance boundary", () => {
    expect(nextStreamingThinkingTailFollow(false, 24, true)).toBe(true);
  });

  // A viewport that remains away from the tail must not regain following without explicit bottom contact.
  // 视口仍远离尾部时，未重新触底就不得恢复尾部跟随。
  it("keeps an unlocked viewport away from the tail", () => {
    expect(nextStreamingThinkingTailFollow(false, 25, false)).toBe(false);
  });
});
