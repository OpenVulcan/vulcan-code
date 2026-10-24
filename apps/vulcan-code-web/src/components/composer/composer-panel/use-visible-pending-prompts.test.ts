import { describe, expect, it } from "vitest";
import { clientActionId, type ConversationEventSummary } from "@/service/conversation-event-window-store";
import { streamedInsertedPromptIds } from "./use-visible-pending-prompts";

function userInput(seq: number, action: string): ConversationEventSummary {
  return {
    seq,
    turn_id: 1,
    created_at_ms: seq,
    updated_at_ms: seq,
    event_revision: 1,
    visibility: "provider",
    event: {
      kind: "user_input",
      client_action_id: clientActionId(action),
      input_id: seq,
      input_source: "pending_in_turn",
      text: "inserted",
    },
  };
}

describe("streamedInsertedPromptIds", () => {
  it("reports only waiting rows whose input reached the conversation", () => {
    const events = [userInput(1, "root-action"), userInput(2, "inserted-a")];
    expect(streamedInsertedPromptIds(new Set(["inserted-a", "inserted-b"]), events)).toBe("inserted-a");
    expect(streamedInsertedPromptIds(new Set(["inserted-b"]), events)).toBe("");
  });

  it("skips scanning when no row is waiting", () => {
    expect(streamedInsertedPromptIds(new Set(), [userInput(1, "inserted-a")])).toBe("");
  });
});
