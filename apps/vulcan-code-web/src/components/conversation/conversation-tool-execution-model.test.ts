import { describe, expect, it } from "vitest";
import {
  toolRequestProgress,
  toolRequestProgressLabel,
} from "./conversation-tool-execution-model";
import type { ToolExecutionConversationBlock } from "./conversation-tool-execution-types";

describe("conversation tool request progress", () => {
  it("uses the newest request revision and reports characters plus UTF-8 bytes", () => {
    const blocks = [
      {
        rawMetadata: {
          request_stream_id: "request-1",
          tool_display_state: "receiving",
          received_characters: 12,
          received_bytes: 16,
          request_revision: 1,
        },
      },
      {
        rawMetadata: {
          request_stream_id: "request-1",
          tool_display_state: "receiving",
          received_characters: 3_032,
          received_bytes: 4_096,
          request_revision: 2,
        },
      },
    ] as ToolExecutionConversationBlock[];

    const progress = toolRequestProgress(blocks);

    expect(progress).toMatchObject({
      requestStreamId: "request-1",
      state: "receiving",
      receivedCharacters: 3_032,
      receivedBytes: 4_096,
      revision: 2,
    });
    expect(toolRequestProgressLabel(progress!)).toBe(
      "Receiving data · 3,032 characters · 4.0 KiB",
    );
    expect(toolRequestProgressLabel(progress!)).not.toContain("Token");
  });

  it("does not replace an executing card with stale request construction state", () => {
    const blocks = [
      {
        rawMetadata: {
          request_stream_id: "request-1",
          tool_display_state: "executing",
          received_characters: 3_032,
          received_bytes: 4_096,
          request_revision: 3,
        },
      },
    ] as ToolExecutionConversationBlock[];

    expect(toolRequestProgress(blocks)).toBeNull();
  });
});
