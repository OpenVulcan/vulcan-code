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
});
