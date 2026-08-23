import { describe, expect, it } from "vitest";
import {
  liveToolRequestProgress,
  toolExecutionDetailLoadEnabled,
  toolExecutionSummary,
  toolRequestProgress,
  toolRequestProgressForDisplay,
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

    // progress is the newest receiving snapshot selected from this request stream.
    // progress 是从该请求流中选出的最新接收快照。
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

  it("closes the receiving presentation as soon as one tool argument document is assembled", () => {
    // blocks contains the exact per-tool structural boundary emitted before Provider completion.
    // blocks 包含 Provider 完成前针对单个工具发出的精确结构闭合边界。
    const blocks = [
      {
        rawMetadata: {
          request_stream_id: "request-assembled",
          tool_display_state: "arguments_assembled",
          received_characters: 2_048,
          received_bytes: 2_112,
          request_revision: 7,
        },
      },
    ] as ToolExecutionConversationBlock[];

    // progress is the independently closed visual state for this request stream.
    // progress 是该请求流独立闭合后的视觉状态。
    const progress = toolRequestProgress(blocks);

    expect(progress).toMatchObject({
      requestStreamId: "request-assembled",
      state: "arguments_assembled",
      receivedCharacters: 2_048,
      receivedBytes: 2_112,
      revision: 7,
    });
    expect(toolRequestProgressLabel(progress!)).toBe(
      "Arguments assembled · 2,048 characters · 2.1 KiB",
    );
    expect(toolRequestProgressLabel(progress!)).not.toContain("Receiving data");
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

  it("does not let the durable data-received marker override an executing tool", () => {
    const ready = {
      requestStreamId: "request-ready",
      state: "ready" as const,
      receivedCharacters: 128,
      receivedBytes: 128,
      revision: 8,
      preview: { kind: "file_edit" },
    };

    expect(toolRequestProgressForDisplay(ready, "running")).toBeNull();
    expect(toolRequestProgressForDisplay(ready, "completed")).toBeNull();
    expect(toolRequestProgressForDisplay({ ...ready, state: "receiving" }, "running")).toMatchObject({
      state: "receiving",
    });
  });

  it("uses the terminal execution state in the compact summary after data is received", () => {
    const blocks = [{
      body: "",
      id: "change-set",
      kind: "diff",
      rawMetadata: {
        request_stream_id: "request-ready",
        tool_display_state: "ready",
        received_characters: 128,
        received_bytes: 128,
        request_revision: 8,
      },
      status: "completed",
      summary: "",
      title: "file_edit",
      toolName: "file_edit",
    }] as ToolExecutionConversationBlock[];

    expect(toolExecutionSummary(blocks, true)).toBe("Tool call · file_edit · Completed");
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
