import { describe, expect, it } from "vitest";
import type { DedicatedLockedSpanV1 } from "@/service/conversation-event-window-client";
import type { ConversationEventSummary } from "@/service/conversation-event-window-store";
import {
  dedicatedEffectiveLockedSpans,
  dedicatedMemoryLocked,
  dedicatedTurnForgotten,
} from "@/service/dedicated-memory";

function memoryNote(seq: number, notice_kind: "memory_locked" | "memory_unlocked"): ConversationEventSummary {
  return {
    seq,
    turn_id: null,
    created_at_ms: seq,
    updated_at_ms: seq,
    event_revision: 1,
    visibility: "ui_only",
    event: { kind: "display_note", notice_kind, severity: "info", reason_code: `dedicated_${notice_kind}` },
  };
}

describe("dedicatedEffectiveLockedSpans", () => {
  it("opens and closes spans from notes that arrive after hydration", () => {
    const spans = dedicatedEffectiveLockedSpans([], [memoryNote(4, "memory_locked"), memoryNote(9, "memory_unlocked")]);
    expect(spans).toEqual([{ locked_at_seq: 4, unlocked_at_seq: 9 }]);
    expect(dedicatedMemoryLocked(spans)).toBe(false);
  });

  it("ignores notes already covered by the hydrated spans and never mutates them", () => {
    const base: DedicatedLockedSpanV1[] = [{ locked_at_seq: 4, unlocked_at_seq: null }];
    const spans = dedicatedEffectiveLockedSpans(base, [
      memoryNote(4, "memory_locked"),
      memoryNote(12, "memory_unlocked"),
      memoryNote(20, "memory_locked"),
    ]);
    expect(spans).toEqual([
      { locked_at_seq: 4, unlocked_at_seq: 12 },
      { locked_at_seq: 20, unlocked_at_seq: null },
    ]);
    expect(base).toEqual([{ locked_at_seq: 4, unlocked_at_seq: null }]);
    expect(dedicatedMemoryLocked(spans)).toBe(true);
  });

  it("drops notes that do not change the lock state", () => {
    const spans = dedicatedEffectiveLockedSpans([], [
      memoryNote(2, "memory_unlocked"),
      memoryNote(3, "memory_locked"),
      memoryNote(5, "memory_locked"),
    ]);
    expect(spans).toEqual([{ locked_at_seq: 3, unlocked_at_seq: null }]);
  });
});

describe("dedicatedTurnForgotten", () => {
  const spans: DedicatedLockedSpanV1[] = [
    { locked_at_seq: 10, unlocked_at_seq: 20 },
    { locked_at_seq: 30, unlocked_at_seq: null },
  ];

  it("forgets closed turns that start strictly inside a locked span", () => {
    expect(dedicatedTurnForgotten(spans, 11, false)).toBe(true);
    expect(dedicatedTurnForgotten(spans, 31, false)).toBe(true);
  });

  it("remembers turns outside the spans and the turn that is still open", () => {
    expect(dedicatedTurnForgotten(spans, 5, false)).toBe(false);
    expect(dedicatedTurnForgotten(spans, 20, false)).toBe(false);
    expect(dedicatedTurnForgotten(spans, 25, false)).toBe(false);
    expect(dedicatedTurnForgotten(spans, 31, true)).toBe(false);
  });
});
