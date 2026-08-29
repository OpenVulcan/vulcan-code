import { describe, expect, it } from "vitest";
import {
  buildToolRawInvocationFallback,
  liveToolRequestProgress,
  toolExecutionDetailLoadEnabled,
  toolExecutionDisplayState,
  toolExecutionAllowsRawInvocation,
  toolExecutionSourceKind,
  toolExecutionSupportsDetailLookup,
  toolExecutionSummary,
  toolRequestProgress,
  toolRequestProgressForDisplay,
  toolRequestProgressLabel,
  toolRequestStreamId,
} from "./conversation-tool-execution-model";
import type { ToolExecutionConversationBlock } from "./conversation-tool-execution-types";
import { normalizeToolDisplayState } from "./conversation-tool-metadata";

describe("conversation tool request progress", () => {
  it("uses only the authoritative runtime source to expose raw invocation details", () => {
    // block builds one terminal record with an optional runtime-owned source key.
    // block 构建一条带可选运行时来源键的终态记录。
    const block = (toolSource?: string) => [{
      body: "",
      id: `source-${toolSource ?? "legacy"}`,
      kind: "tool-result" as const,
      rawMetadata: toolSource ? { tool_source: toolSource } : {},
      summary: "",
      title: "tool",
      toolName: "tool",
    }];

    expect(toolExecutionSourceKind(block("native"))).toBe("native");
    expect(toolExecutionAllowsRawInvocation(block("native"))).toBe(false);
    expect(toolExecutionAllowsRawInvocation(block("mcp"))).toBe(true);
    expect(toolExecutionAllowsRawInvocation(block("unregistered"))).toBe(true);
    expect(toolExecutionAllowsRawInvocation(block())).toBe(false);
  });

  it("keeps invalid and rejected pre-execution requests in the failed terminal state", () => {
    expect(normalizeToolDisplayState("invalid")).toBe("failed");
    expect(normalizeToolDisplayState("rejected")).toBe("failed");
    for (const status of ["invalid", "rejected"]) {
      const blocks = [{
        body: "Provider emitted invalid tool arguments.",
        id: `tool-${status}`,
        kind: "tool-call",
        rawMetadata: { tool_display_state: status },
        status,
        summary: "file_edit",
        title: "file_edit",
        toolName: "file_edit",
      }] as ToolExecutionConversationBlock[];

      expect(toolExecutionDisplayState(blocks, true)).toBe("failed");
    }
  });

  it("uses the newest request revision without exposing transport metrics", () => {
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
    expect(toolRequestProgressLabel(progress!)).toBe("Requesting · 3,032 characters · 4.0 KiB");
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

  it("keeps one stable request label after the argument document is assembled", () => {
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
    expect(toolRequestProgressLabel(progress!)).toBe("Requesting · 2,048 characters · 2.1 KiB");
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

  it("keeps the durable ready boundary visible until ToolStarted replaces it", () => {
    const ready = {
      requestStreamId: "request-ready",
      state: "ready" as const,
      receivedCharacters: 128,
      receivedBytes: 128,
      revision: 8,
      preview: { kind: "file_edit" },
    };

    expect(toolRequestProgressForDisplay(ready, "running")).toBe(ready);
    expect(toolRequestProgressForDisplay(ready, "completed")).toBeNull();
    expect(toolRequestProgressForDisplay({ ...ready, state: "receiving" }, "running")).toMatchObject({
      state: "receiving",
    });
  });

  it("retains the request identity after ToolStarted replaces the durable display state", () => {
    // blocks models the in-place metadata merge performed by the durable ToolStarted projection.
    // blocks 模拟持久 ToolStarted 投影执行的原地元数据合并。
    const blocks = [{
      rawMetadata: {
        request_stream_id: "request-running",
        request_revision: 9,
        tool_display_state: "running",
      },
    }] as ToolExecutionConversationBlock[];

    expect(toolRequestProgress(blocks)).toBeNull();
    expect(toolRequestStreamId(blocks)).toBe("request-running");
  });

  it("keeps Ready in the request phase until an authoritative ToolStarted event", () => {
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

    expect(toolExecutionSummary(blocks, true)).toBe("Tool call · file_edit · Requesting · 128 characters · 128 B");
  });

  it("lets an actual terminal result override a stale ready request block", () => {
    // blocks models a legacy row where request and result records were merged without replacing request metadata.
    // blocks 模拟请求记录与结果记录合并后未覆盖请求元数据的旧版行。
    const blocks = [{
      body: "",
      id: "ready-call",
      kind: "tool-call",
      rawMetadata: {
        request_stream_id: "request-ready",
        tool_display_state: "ready",
        received_characters: 64,
        received_bytes: 64,
        request_revision: 3,
      },
      status: "completed",
      summary: "knowledge_search",
      title: "knowledge_search",
      toolName: "knowledge_search",
    }, {
      body: "found",
      id: "terminal-result",
      kind: "tool-result",
      status: "completed",
      summary: "",
      title: "",
      toolName: "knowledge_search",
    }] as ToolExecutionConversationBlock[];

    expect(toolExecutionDisplayState(blocks, true)).toBe("completed");
    expect(toolRequestProgressForDisplay(toolRequestProgress(blocks), "completed")).toBeNull();
  });

  it("marks visible-record raw invocation data as an explicit legacy partial fallback", () => {
    const blocks = [{
      body: "",
      id: "call-record",
      kind: "tool-call",
      rawMetadata: {
        arguments: { query: "card lifecycle" },
        tool_call_id: "call-raw-1",
      },
      summary: "knowledge_search",
      title: "knowledge_search",
      toolCallId: "call-raw-1",
      toolName: "knowledge_search",
    }, {
      body: "Found one result",
      id: "result-record",
      kind: "tool-result",
      summary: "",
      title: "",
      toolCallId: "call-raw-1",
      toolName: "knowledge_search",
    }] as ToolExecutionConversationBlock[];

    expect(buildToolRawInvocationFallback(blocks, "completed")).toEqual({
      version: 1,
      completeness: "legacy_partial",
      callId: "call-raw-1",
      invocation: {
        tool: "knowledge_search",
        arguments: { query: "card lifecycle" },
      },
      execution: { status: "completed" },
      result: "Found one result",
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

  it("allows tool-call fallback only when a terminal result block exists", () => {
    // stoppedRequest contains identity but no evidence that a result/detail record was ever persisted.
    // stoppedRequest 包含调用身份，但没有任何结果或详情记录已持久化的证据。
    const stoppedRequest = [{
      body: "",
      id: "stopped-call",
      kind: "tool-call",
      summary: "knowledge_search",
      title: "knowledge_search",
      toolCallId: "call-stopped",
      toolName: "knowledge_search",
    }] as ToolExecutionConversationBlock[];
    // legacyTerminal adds the old ToolResultOutput shape that the service can resolve by toolCallId.
    // legacyTerminal 增加服务端可按 toolCallId 解析的旧版 ToolResultOutput 形状。
    const legacyTerminal = [...stoppedRequest, {
      body: "done",
      id: "legacy-result",
      kind: "tool-result",
      summary: "",
      title: "",
      toolCallId: "call-stopped",
      toolName: "knowledge_search",
    }] as ToolExecutionConversationBlock[];

    expect(toolExecutionSupportsDetailLookup(stoppedRequest)).toBe(false);
    expect(toolExecutionSupportsDetailLookup([{
      ...stoppedRequest[0],
      kind: "diff",
      rawMetadata: { tool_display_state: "ready" },
    }])).toBe(false);
    expect(toolExecutionSupportsDetailLookup(legacyTerminal)).toBe(true);
  });
});
