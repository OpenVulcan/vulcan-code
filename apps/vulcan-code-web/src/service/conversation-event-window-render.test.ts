import { describe, expect, it } from "vitest";

import {
  ConversationEventBody,
  ConversationEventLivePatch,
  ConversationEventSummary,
  ConversationEventWindowBaseline,
  ConversationEventWindowStore,
  PersistentChangeSet,
} from "./conversation-event-window-store";

/** Stable session id used by the event-window fixture.
 * 事件窗口夹具使用的稳定会话 ID。 */
const SESSION_ID = "session-projection";

/** Builds one complete V1 summary for event-window tests.
 * 为事件窗口测试构建一条完整 V1 摘要。 */
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

/** Builds one revisioned sparse patch for store tests.
 * 为 store 测试构建一份带修订稀疏 patch。 */
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

/** Builds one exact serde-shaped collaboration question event from the Rust wire contract.
 * 按 Rust wire 契约构建一条精确 serde 形状的协作问题事件。 */
function collaborationQuestionCreatedFixture(state: "pending" | "answered" = "pending"): ConversationEventBody {
  return {
    kind: "collaboration",
    version: 1,
    operation_id: "operation-question-1",
    event_kind: state === "pending" ? "question_created" : "question_answered",
    after_state: {
      kind: "question",
      question: {
        operation_id: "operation-question-1",
        question_id: "question-1",
        delivery_id: "delivery-1",
        source: {
          session_id: "session-source",
          root_session_id: "session-source",
          project_id: "project-1",
          segment_generation: 1,
          turn_id: 7,
        },
        target: {
          session_id: "session-target",
          root_session_id: "session-target",
          project_id: "project-1",
          segment_generation: 1,
          task_id: "task-target",
        },
        body: "Which file owns this behavior?",
        execution_purpose: "in_turn_question",
        state,
        ...(state === "answered" ? {
          answer: {
            operation_id: "operation-answer-1",
            responder: {
              session_id: "session-target",
              root_session_id: "session-target",
              project_id: "project-1",
              segment_generation: 1,
              task_id: "task-target",
            },
            body: "The projection service owns it.",
            settled_at_ms: 120,
            delivery: {
              state: "delivered",
              delivery_revision: 1,
              kind: "wait_tool",
              last_operation_id: "operation-answer-delivery-1",
              updated_at_ms: 121,
            },
          },
        } : {}),
        created_at_ms: 100,
      },
      delivery: {
        delivery_id: "delivery-1",
        question_id: "question-1",
        operation_id: "operation-question-1",
        source: {
          session_id: "session-source",
          root_session_id: "session-source",
          project_id: "project-1",
          segment_generation: 1,
          turn_id: 7,
        },
        target: {
          session_id: "session-target",
          root_session_id: "session-target",
          project_id: "project-1",
          segment_generation: 1,
          task_id: "task-target",
        },
        state: "recorded",
        updated_at_ms: 101,
      },
    },
  };
}

describe("conversation event window rendering", () => {
  it("uses operation_id as the same-seq identity for collaboration revisions", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(batch([summary(1, collaborationQuestionCreatedFixture(), 101, 1, "host_only")]));
    const revisedEvent = collaborationQuestionCreatedFixture();
    store.applyLive(livePatch([summary(1, revisedEvent, 102, 2, "host_only")], 2));
    const rendered = store.render();
    expect(rendered).toHaveLength(1);
    expect(rendered[0].event).toMatchObject({
      kind: "collaboration",
      operation_id: "operation-question-1",
    });
    expect(rendered[0].event_revision).toBe(2);
    const conflictingEvent = { ...revisedEvent, operation_id: "operation-question-2" };
    expect(() => store.applyLive(livePatch([summary(1, conflictingEvent, 103, 3, "host_only")], 3)))
      .toThrow(/identity conflict/);
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

    const [rendered] = store.render();
    if (rendered?.event.kind !== "tool_call") throw new Error(`expected tool_call, got ${rendered?.event.kind}`);
    const item = { tool_call: rendered.event };

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

  it("reflects same-seq streaming updates without creating or reordering events", () => {
    const store = new ConversationEventWindowStore();
    store.hydrate(
      batch([
        summary(1, assistant("partial", "streaming"), 10),
        summary(2, tool(31, "running"), 10),
      ]),
    );

    const before = store.render();
    store.applyLive(
      livePatch([
        summary(1, assistant("complete", "completed"), 20, 2),
        summary(2, tool(31, "failed"), 20, 2),
      ]),
    );
    const after = store.render();

    expect(before.map((item) => item.seq)).toEqual([1, 2]);
    expect(after.map((item) => item.seq)).toEqual([1, 2]);
    expect(after[0].event).toMatchObject({ kind: "assistant_text", text: "complete", status: "completed" });
    expect(after[1].event).toMatchObject({ kind: "tool_call", status: "failed" });
  });

  it("renders equivalent events after restart hydration", () => {
    const liveStore = new ConversationEventWindowStore();
    liveStore.hydrate(
      batch([
        summary(1, reasoning("thinking")),
        summary(2, assistant("answer")),
      ]),
    );
    liveStore.applyLive(livePatch([summary(3, tool(31, "failed"), 20)], 2));
    const beforeRestart = liveStore.render();

    const restartedStore = new ConversationEventWindowStore();
    restartedStore.hydrate(batch(beforeRestart));

    expect(restartedStore.render()).toEqual(beforeRestart);
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

    const rendered = store.render();

    expect(rendered.map((item) => item.seq)).toEqual([1, 2, 3, 4]);
    expect(rendered[3].event).toMatchObject({ kind: "display_note", notice_kind: "compaction_completed" });
  });
});
