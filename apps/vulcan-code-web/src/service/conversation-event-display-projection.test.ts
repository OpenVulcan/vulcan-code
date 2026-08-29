import { describe, expect, it } from "vitest";

import {
  clientActionId,
  ConversationEventBody,
  ConversationEventLivePatch,
  ConversationEventSummary,
  ConversationEventWindowBaseline,
  ConversationEventWindowStore,
  PersistentChangeSet,
} from "./conversation-event-window-store";
import {
  projectConversationEventDisplay,
  projectConversationTranscriptDisplay,
  ToolCallDisplayItem,
} from "./conversation-event-display-projection";

/** Stable session id used by the projection fixture.
 * 投影夹具使用的稳定会话 ID。 */
const SESSION_ID = "session-projection";

/** Builds one complete V1 summary for projection tests.
 * 为投影测试构建一条完整 V1 摘要。 */
function summary(
  seq: number,
  event: ConversationEventBody,
  updatedAtMs = 10,
  eventRevision = 1,
  visibility: ConversationEventSummary["visibility"] = "provider",
): ConversationEventSummary {
  return {
    seq,
    turn_id: 7,
    created_at_ms: 1,
    updated_at_ms: updatedAtMs,
    event_revision: eventRevision,
    visibility,
    event,
  };
}

/** Builds one owner-bound event-window batch.
 * 构建一批带归属的事件窗口数据。 */
function batch(
  events: readonly ConversationEventSummary[],
  generation = 1,
): ConversationEventWindowBaseline {
  const memoryRevision = events.reduce(
    (current, summaryValue) => Math.max(current, summaryValue.event_revision),
    0,
  );
  return {
    session_id: SESSION_ID,
    window_generation: generation,
    service_epoch: "service-epoch-1",
    memory_revision: memoryRevision,
    durable_revision: memoryRevision,
    events,
  };
}

/** Builds one revisioned sparse patch for projection-over-store tests.
 * 为经过 store 的投影测试构建一份带修订稀疏 patch。 */
function livePatch(
  events: readonly ConversationEventSummary[],
  revision = events.reduce(
    (current, summaryValue) => Math.max(current, summaryValue.event_revision),
    0,
  ),
): ConversationEventLivePatch {
  return {
    session_id: SESSION_ID,
    window_generation: 1,
    service_epoch: "service-epoch-1",
    base_memory_revision: revision - 1,
    revision,
    durability: "volatile",
    events,
  };
}

/** Builds one assistant reasoning event.
 * 构建一条 assistant 思考事件。 */
function reasoning(text: string, status: "streaming" | "completed" = "completed"): ConversationEventBody {
  return { kind: "assistant_reasoning", api_call_id: 11, text, status };
}

/** Builds one assistant Markdown event.
 * 构建一条 assistant Markdown 正文事件。 */
function assistant(text: string, status: "streaming" | "completed" = "completed"): ConversationEventBody {
  return { kind: "assistant_text", api_call_id: 11, text, status };
}

/** Builds one complete tool-call event with authoritative output and change-set.
 * 构建一条携带权威输出和 change-set 的完整工具调用事件。 */
function tool(
  toolCallId: number,
  status: "running" | "completed" | "failed",
  changeset?: PersistentChangeSet,
): ConversationEventBody {
  return {
    kind: "tool_call",
    api_call_id: 11,
    tool_call_id: toolCallId,
    tool_name: "file_edit",
    input: { path: "D:/project/README.md", content: "new" },
    request_stream_id: `display-tool-${toolCallId}`,
    output: status === "running" ? undefined : { result: status },
    changeset,
    status,
  };
}

/** Builds one persisted context-compaction marker.
 * 构建一条持久化上下文压缩标记。 */
function compactionMarker(): ConversationEventBody {
  return {
    kind: "display_note",
    notice_kind: "compaction_completed",
    severity: "info",
    reason_code: "context_compaction_completed",
  };
}

/** Returns one concrete tool display item after asserting its union branch.
 * 在断言联合类型分支后返回一个具体工具显示项。 */
function asTool(item: ReturnType<typeof projectConversationEventDisplay>[number]): ToolCallDisplayItem {
  if (item.kind !== "tool_call") throw new Error(`expected tool_call display item, got ${item.kind}`);
  return item;
}

describe("conversation V1 event display projection", () => {
  it("derives user, process, and final-answer hierarchy while hiding structural ledger facts", () => {
    // emptyBucket provides an exact zero-value terminal statistics bucket.
    // emptyBucket 提供一个精确零值终态统计桶。
    const emptyBucket = {
      api_call_count: 0,
      unknown_usage_api_call_count: 0,
      tool_call_count: 0,
      subagent_return_count: 0,
      usage: {},
    };
    const projected = projectConversationEventDisplay([
      summary(1, {
        kind: "user_input",
        client_action_id: clientActionId("projection-root"),
        input_id: 1,
        input_source: "root",
        text: "inspect",
      }),
      summary(2, {
        kind: "api_started",
        api_call_id: 11,
        purpose: "main_generation",
        request: { provider: "test-provider", model: "test-model" },
      }, 10, 1, "host_only"),
      summary(3, reasoning("reason")),
      summary(4, assistant("tool preamble")),
      summary(5, tool(31, "completed")),
      summary(6, {
        kind: "api_stats",
        api_call_id: 11,
        purpose: "main_generation",
        provider: "test-provider",
        model: "test-model",
        usage: { total_tokens: 5 },
        status: "completed",
        continuation_eligibility: "forbidden",
        timing: {},
      }, 10, 1, "ui_only"),
      summary(7, {
        kind: "api_started",
        api_call_id: 12,
        purpose: "main_generation",
        request: { provider: "test-provider", model: "test-model" },
      }, 10, 1, "host_only"),
      summary(8, { kind: "assistant_text", api_call_id: 12, text: "final answer", status: "completed" }),
      summary(9, {
        kind: "api_stats",
        api_call_id: 12,
        purpose: "main_generation",
        provider: "test-provider",
        model: "test-model",
        usage: { total_tokens: 9 },
        status: "completed",
        continuation_eligibility: "forbidden",
        timing: {},
      }, 10, 1, "ui_only"),
      summary(10, { kind: "turn_boundary", outcome: "completed" }),
      summary(11, {
        kind: "turn_stats",
        start_seq: 1,
        end_seq: 10,
        outcome: "completed",
        buckets: { direct: emptyBucket, subagent: emptyBucket, total: emptyBucket },
      }, 10, 1, "ui_only"),
    ]);

    const transcript = projectConversationTranscriptDisplay(projected);

    expect(transcript.map((node) => node.kind)).toEqual([
      "user_input",
      "process_group",
      "assistant_final",
      "turn_stats",
    ]);
    const process = transcript[1];
    expect(process.kind === "process_group" ? process.items.map((item) => item.seq) : []).toEqual([3, 4, 5]);
    const finalAnswer = transcript[2];
    expect(finalAnswer.kind === "assistant_final" ? finalAnswer.api_stats?.usage.total_tokens : null)
      .toBe(9);
    expect(finalAnswer.kind === "assistant_final" ? finalAnswer.root_input : null)
      .toEqual({ seq: 1, text: "inspect" });
    const turnStats = transcript[3];
    expect(turnStats.kind === "turn_stats" ? turnStats.item.event.buckets.total : null)
      .toEqual(emptyBucket);
  });

  it("never merges process items from two logical Turns", () => {
    const first = summary(1, reasoning("first turn"));
    const second = { ...summary(2, reasoning("second turn")), turn_id: 8 };

    const transcript = projectConversationTranscriptDisplay(
      projectConversationEventDisplay([first, second]),
    );

    expect(transcript.map((node) => [node.kind, node.turn_id, node.seq])).toEqual([
      ["process_group", 7, 1],
      ["process_group", 8, 2],
    ]);
  });

  it("projects the complete visibility sequence before the renderer hides host-only items", () => {
    // projected preserves every canonical seq before any visibility decision is applied.
    // projected 在应用任何可见性决策前保留每个 canonical seq。
    const projected = projectConversationEventDisplay([
      summary(
        1,
        {
          kind: "api_started",
          api_call_id: 11,
          purpose: "main_generation",
          request: { provider: "test-provider", model: "test-model" },
        },
        10,
        1,
        "host_only",
      ),
      summary(2, reasoning("thinking")),
      summary(3, tool(31, "failed")),
      summary(4, tool(32, "completed")),
      summary(
        5,
        {
          kind: "api_stats",
          api_call_id: 11,
          purpose: "main_generation",
          provider: "test-provider",
          model: "test-model",
          usage: {},
          status: "completed",
          continuation_eligibility: "forbidden",
          timing: {},
        },
        10,
        1,
        "ui_only",
      ),
    ]);

    expect(projected.map((item) => item.seq)).toEqual([1, 2, 3, 4, 5]);
    expect(projected.slice(1).map((item) => item.group.api_call_id)).toEqual([11, 11, 11, 11]);
    expect(projected.filter((item) => item.kind === "tool_call").map((item) => item.tool_call.status))
      .toEqual(["failed", "completed"]);
  });

  it("projects an exact request configuration change instead of a vague model notice", () => {
    const display = projectConversationEventDisplay([
      summary(
        1,
        {
          kind: "request_config_changed",
          previous: {
            provider: "openai",
            connection_id: "conn-1",
            model: "gpt-5",
            thinking_level: "high",
            temperature: 0.3,
          },
          current: {
            provider: "anthropic",
            connection_id: "conn-2",
            model: "claude-4",
            thinking_level: "max",
            temperature: 0.2,
          },
        },
        10,
        1,
        "ui_only",
      ),
    ]);

    expect(display).toEqual([
      expect.objectContaining({
        kind: "request_config_changed",
        visibility: "ui_only",
        group: { turn_id: 7, api_call_id: null },
        request_config_changed: {
          kind: "request_config_changed",
          previous: expect.objectContaining({ model: "gpt-5", thinking_level: "high" }),
          current: expect.objectContaining({ model: "claude-4", thinking_level: "max" }),
        },
      }),
    ]);
  });

  it("projects thinking, assistant Markdown, and tool strictly by seq", () => {
    const events = [
      summary(1, reasoning("thinking")),
      summary(2, assistant("**answer**")),
      summary(3, tool(31, "completed")),
    ];

    const display = projectConversationEventDisplay(events);

    expect(display.map((item) => [item.seq, item.kind])).toEqual([
      [1, "assistant_reasoning"],
      [2, "assistant_text"],
      [3, "tool_call"],
    ]);
    expect(display[1]).toMatchObject({ markdown: "**answer**" });
    expect(display.map((item) => item.group)).toEqual([
      { turn_id: 7, api_call_id: 11 },
      { turn_id: 7, api_call_id: 11 },
      { turn_id: 7, api_call_id: 11 },
    ]);
  });

  it("keeps failed and successful tools in factual seq order inside the original API group", () => {
    const failed = tool(31, "failed");
    const completed = tool(32, "completed");

    const display = projectConversationEventDisplay([
      summary(1, failed),
      summary(2, completed),
    ]);

    expect(display.map((item) => [item.seq, asTool(item).tool_call.status])).toEqual([
      [1, "failed"],
      [2, "completed"],
    ]);
    expect(display.map((item) => item.group.api_call_id)).toEqual([11, 11]);
  });

  it("takes tool status, output, and change-set from exactly one tool event", () => {
    const changeset: PersistentChangeSet = {
      mode: "applied",
      summary: "Updated README and removed obsolete content.",
      diagnostics: [
        {
          level: "warning",
          message: "Deleted content was retained as explicit head and tail snippets.",
        },
      ],
      files: [
        {
          change: "modify",
          path: "D:/project/README.md",
          before_text_format: {
            encoding: "utf-8",
            line_ending: "crlf",
            final_newline: true,
          },
          after_text_format: {
            encoding: "utf-8",
            line_ending: "lf",
            final_newline: true,
          },
          hunks: [
            {
              before: "old",
              after: "new",
              deleted: [{ line: 1, content: "old" }],
              inserted: [{ line: 1, content: "new" }],
            },
          ],
        },
        {
          change: "delete",
          path: "D:/project/obsolete.md",
          hunks: [],
          lifecycle_content: {
            mode: "truncated",
            head: "line 1\nline 2",
            tail: "line 207\nline 208",
            total_line_count: 208,
          },
          before_text_format: {
            encoding: "utf-8",
            line_ending: "lf",
            final_newline: true,
          },
        },
        {
          change: "create",
          path: "D:/project/new.md",
          hunks: [],
          lifecycle_content: {
            mode: "full",
            content: "line 1\nline 2",
            total_line_count: 2,
          },
          after_text_format: {
            encoding: "utf-8",
            line_ending: "lf",
            final_newline: false,
          },
        },
        {
          change: "delete",
          path: "D:/project/binary.dat",
          hunks: [],
          lifecycle_content: { mode: "omitted" },
        },
      ],
    };
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, tool(31, "running"), 10)]));
    store.applyLive(livePatch([summary(1, tool(31, "completed", changeset), 20, 2)]));

    const item = asTool(projectConversationEventDisplay(store.render())[0]);

    expect(item.tool_call.status).toBe("completed");
    expect(item.tool_call.output).toEqual({ result: "completed" });
    expect(item.tool_call.changeset).toBe(changeset);
    expect(item.tool_call.changeset?.summary).toBe("Updated README and removed obsolete content.");
    expect(item.tool_call.changeset?.diagnostics).toEqual([
      {
        level: "warning",
        message: "Deleted content was retained as explicit head and tail snippets.",
      },
    ]);
    expect(item.tool_call.changeset?.files[0]).toMatchObject({
      before_text_format: { encoding: "utf-8", line_ending: "crlf" },
      after_text_format: { encoding: "utf-8", line_ending: "lf" },
    });
    expect(item.tool_call.changeset?.files[1].lifecycle_content).toEqual({
      mode: "truncated",
      head: "line 1\nline 2",
      tail: "line 207\nline 208",
      total_line_count: 208,
    });
    expect(item.tool_call.changeset?.files[2].lifecycle_content).toEqual({
      mode: "full",
      content: "line 1\nline 2",
      total_line_count: 2,
    });
    expect(item.tool_call.changeset?.files[3].lifecycle_content).toEqual({
      mode: "omitted",
    });
  });

  it("reflects same-seq streaming updates without creating or reordering display items", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(
      batch([
        summary(1, assistant("partial", "streaming"), 10),
        summary(2, tool(31, "running"), 10),
      ]),
    );

    const before = projectConversationEventDisplay(store.render());
    store.applyLive(
      livePatch([
        summary(1, assistant("complete", "completed"), 20, 2),
        summary(2, tool(31, "failed"), 20, 2),
      ]),
    );
    const after = projectConversationEventDisplay(store.render());

    expect(before.map((item) => item.seq)).toEqual([1, 2]);
    expect(after.map((item) => item.seq)).toEqual([1, 2]);
    expect(after[0]).toMatchObject({ markdown: "complete", assistant: { status: "completed" } });
    expect(asTool(after[1]).tool_call.status).toBe("failed");
  });

  it("produces equivalent display output after restart hydration", () => {
    const liveStore = new ConversationEventWindowStore();
    liveStore.hydrate(
      batch([
        summary(1, reasoning("thinking")),
        summary(2, assistant("answer")),
      ]),
    );
    liveStore.applyLive(livePatch([summary(3, tool(31, "failed"), 20)], 2));
    const beforeRestart = projectConversationEventDisplay(liveStore.render());

    const restartedStore = new ConversationEventWindowStore();
    restartedStore.hydrate(batch(liveStore.render()));
    const afterRestart = projectConversationEventDisplay(restartedStore.render());

    expect(afterRestart).toEqual(beforeRestart);
  });

  it("retains loaded history when a compaction marker is appended", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(
      batch([
        summary(1, reasoning("thinking")),
        summary(2, assistant("answer")),
        summary(3, tool(31, "completed")),
      ]),
    );
    store.applyLive(livePatch([summary(4, compactionMarker(), 20)], 2));

    const display = projectConversationEventDisplay(store.render());

    expect(display.map((item) => item.seq)).toEqual([1, 2, 3, 4]);
    expect(display[3]).toMatchObject({
      kind: "event",
      event: { kind: "display_note", notice_kind: "compaction_completed" },
    });
  });

  it("rejects duplicate or gapped projection input instead of repairing it", () => {
    expect(() =>
      projectConversationEventDisplay([
        summary(1, assistant("one")),
        summary(1, assistant("duplicate")),
      ]),
    ).toThrow(/projection gap/);
    expect(() =>
      projectConversationEventDisplay([
        summary(1, assistant("one")),
        summary(3, tool(31, "completed")),
      ]),
    ).toThrow(/projection gap/);
    expect(() =>
      projectConversationEventDisplay([
        summary(1, assistant("one")),
        summary(3, tool(31, "completed")),
        summary(2, assistant("late")),
      ]),
    ).toThrow("conversation display projection gap: expected seq 2, got 3");
  });
});
