import { describe, expect, it } from "vitest";
import {
  classifyConversationObservedScrollMovement,
  conversationResizeCorrectionMode,
  conversationResizeOwnershipDecision,
  reconcileConversationNativeScrollClamp,
} from "./conversation-scroll-controller";

describe("conversation scroll ownership ledger", () => {
  it("keeps tail ownership when composer shrink precedes prompt insertion and the native scroll event", () => {
    // baseline acknowledges the browser clipping the former tail before the new prompt grows the range.
    // baseline 在新提示词增大范围前确认浏览器对原尾部位置的截断。
    const baseline = reconcileConversationNativeScrollClamp(823, 635, 635);
    expect(baseline).toBe(635);
    expect(classifyConversationObservedScrollMovement(baseline, 635, null)).toEqual({
      readerMoved: false,
      retainTarget: false,
    });
    // appendedBaseline must retain the acknowledged position when the delayed event sees the larger range.
    // appendedBaseline 在延迟事件看到增大后的范围时必须保留已确认的位置。
    const appendedBaseline = reconcileConversationNativeScrollClamp(baseline, 635, 776);
    expect(appendedBaseline).toBe(635);
  });

  it("does not hide actual reader movement during or after a shrinking layout", () => {
    expect(reconcileConversationNativeScrollClamp(823, 600, 635)).toBe(823);
    expect(reconcileConversationNativeScrollClamp(600, 550, 635)).toBe(600);
    // baseline is the clipped tail; any subsequent upward movement still belongs to the reader.
    // baseline 是截断后的尾部；随后任何向上移动仍属于阅读者。
    const baseline = reconcileConversationNativeScrollClamp(823, 635, 635);
    expect(classifyConversationObservedScrollMovement(baseline, 590, null).readerMoved).toBe(true);
  });

  it("acknowledges collapse to an unscrollable surface without inferring movement from growth", () => {
    expect(reconcileConversationNativeScrollClamp(240, 0, 0)).toBe(0);
    expect(reconcileConversationNativeScrollClamp(240, 240, 400)).toBe(240);
    expect(reconcileConversationNativeScrollClamp(240, 180.5, 181)).toBe(180.5);
  });

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

  it("does not restore reader-owned scroll position for ordinary aggregate reflow", () => {
    expect(conversationResizeCorrectionMode(false, false)).toBe("none");
  });

  it("limits aggregate reflow correction to tail ownership or one manual transaction", () => {
    expect(conversationResizeCorrectionMode(true, false)).toBe("tail");
    expect(conversationResizeCorrectionMode(false, true)).toBe("transaction");
    expect(conversationResizeCorrectionMode(true, true)).toBe("transaction");
  });

  it("restores tail ownership when collapse makes the native viewport reach the bottom", () => {
    expect(conversationResizeOwnershipDecision(false, true, true)).toEqual({
      adoptedTail: true,
      correctionMode: "tail",
    });
    expect(conversationResizeOwnershipDecision(false, false, true)).toEqual({
      adoptedTail: true,
      correctionMode: "tail",
    });
  });

  it("preserves reader ownership while resized content still extends below the viewport", () => {
    expect(conversationResizeOwnershipDecision(false, true, false)).toEqual({
      adoptedTail: false,
      correctionMode: "transaction",
    });
    expect(conversationResizeOwnershipDecision(false, false, false)).toEqual({
      adoptedTail: false,
      correctionMode: "none",
    });
  });
});
