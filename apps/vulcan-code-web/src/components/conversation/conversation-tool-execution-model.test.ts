import { describe, expect, it } from "vitest";
import {
  liveToolRequestProgress,
  toolExecutionDetailLoadEnabled,
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

  it("maps one leaf-store snapshot without changing durable request semantics", () => {
    expect(liveToolRequestProgress({
      requestStreamId: "request-1",
      toolCallId: "call-1",
      toolName: "terminal_session",
      state: "receiving",
      receivedBytes: 1_024,
      receivedCharacters: 1_000,
      requestRevision: 4,
      preview: { kind: "command" },
    })).toEqual({
      requestStreamId: "request-1",
      state: "receiving",
      receivedBytes: 1_024,
      receivedCharacters: 1_000,
      revision: 4,
      preview: { kind: "command" },
    });
  });

  it("permits detail hydration only for a terminal host-owned reference", () => {
    const detailRef = { recordId: "detail-1", seq: 4 };
    // toolCallRef proves the documented terminal lookup remains valid before a record id exists.
    // toolCallRef 证明在记录标识尚不存在时，已定义的终态查询路径仍然有效。
    const toolCallRef = { toolCallId: "call-1" };
    const receiving = {
      requestStreamId: "request-1",
      state: "receiving" as const,
      receivedCharacters: 10,
      receivedBytes: 10,
      revision: 2,
      preview: null,
    };

    expect(toolExecutionDetailLoadEnabled("running", null, detailRef)).toBe(false);
    expect(toolExecutionDetailLoadEnabled("completed", receiving, detailRef)).toBe(false);
    expect(toolExecutionDetailLoadEnabled("completed", null, null)).toBe(false);
    expect(toolExecutionDetailLoadEnabled("completed", null, detailRef)).toBe(true);
    expect(toolExecutionDetailLoadEnabled("completed", null, toolCallRef)).toBe(true);
    expect(toolExecutionDetailLoadEnabled("failed", null, detailRef)).toBe(true);
  });
});
