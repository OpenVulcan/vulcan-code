import { describe, expect, it } from "vitest";
import { classifyConversationObservedScrollMovement } from "./conversation-scroll-controller";

describe("conversation scroll ownership ledger", () => {
  it("treats an unregistered native movement as reader ownership", () => {
    expect(classifyConversationObservedScrollMovement(400, 320, null)).toEqual({
      readerMoved: true,
      retainTarget: false,
    });
  });

  it("retains a smooth programmatic target while native motion approaches it", () => {
    expect(classifyConversationObservedScrollMovement(100, 260, 800)).toEqual({
      readerMoved: false,
      retainTarget: true,
    });
  });

  it("closes a programmatic target without transferring ownership when it is reached", () => {
    expect(classifyConversationObservedScrollMovement(760, 800, 800)).toEqual({
      readerMoved: false,
      retainTarget: false,
    });
  });

  it("transfers ownership when native motion moves away from an active target", () => {
    expect(classifyConversationObservedScrollMovement(500, 420, 900)).toEqual({
      readerMoved: true,
      retainTarget: false,
    });
  });
});
