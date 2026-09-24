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

/** Builds one exact serde-shaped source notification event from the Rust wire contract.
 * 按 Rust wire 契约构建一条精确 serde 形状的来源通知事件。 */
function collaborationNotificationSentFixture(): ConversationEventBody {
  return {
    kind: "collaboration",
    version: 1,
    operation_id: "operation-notification-1",
    event_kind: "notification_sent",
    after_state: {
      kind: "outbound_notification",
      notification: {
        notification_id: "notification-1",
        delivery_id: "delivery-notification-1",
        operation_id: "operation-notification-1",
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
        body: "The child agent reached the validation phase.",
        state: "recorded",
        created_at_ms: 110,
        updated_at_ms: 111,
      },
    },
  };
}

/** Returns one concrete tool display item after asserting its union branch.
 * 在断言联合类型分支后返回一个具体工具显示项。 */
function asTool(item: ReturnType<typeof projectConversationEventDisplay>[number]): ToolCallDisplayItem {
  if (item.kind !== "tool_call") throw new Error(`expected tool_call display item, got ${item.kind}`);
  return item;
}

describe("conversation V1 event display projection", () => {
  it.each(["child_task_reserved", "child_task_activated"] as const)(
    "retains %s in event data but omits its bookkeeping card from chat",
    (eventKind) => {
      /** Real child ownership is retained for navigation and runtime state.
       * 保留真实子任务归属，供导航及运行状态使用。 */
      const event: ConversationEventBody = {
        kind: "collaboration",
        version: 1,
        operation_id: eventKind,
        event_kind: eventKind,
        after_state: {
          kind: "child_task",
          task: {
            reservation_operation_id: "reserve-child",
            source_tool_name: "spawn_agent",
            execution_id: "execution-child",
            task_id: "child",
            parent_segment_generation: 1,
            parent_turn_id: 7,
            child: {
              session_id: "child",
              root_session_id: SESSION_ID,
              project_id: "project-1",
              segment_generation: 1,
              task_id: "child",
            },
            state: eventKind === "child_task_reserved" ? "reserved" : "active",
            reserved_at_ms: 1,
            non_success_result: null,
            non_success_projection: null,
          },
        },
      };
      /** Include a real agent notification to ensure meaningful content remains visible.
       * 加入真实代理通知，确保有意义的正文仍然可见。 */
      const projected = projectConversationEventDisplay([
        summary(1, event, 101, 1, "host_only"),
        summary(2, collaborationNotificationSentFixture(), 102, 1, "host_only"),
      ]);
      expect(projected).toHaveLength(2);
      expect(projectConversationTranscriptDisplay(projected)).toMatchObject([
        { kind: "collaboration", seq: 2 },
      ]);
    },
  );

  it("keeps the serde collaboration envelope strict and visible on the HostOnly timeline", () => {
    const event = collaborationQuestionCreatedFixture();
    const projected = projectConversationEventDisplay([summary(1, event, 101, 1, "host_only")]);
    expect(projected).toHaveLength(1);
    expect(projected[0]).toMatchObject({
      kind: "collaboration",
      collaboration: {
        kind: "collaboration",
        event_kind: "question_created",
        after_state: { kind: "question" },
      },
    });
    const nodes = projectConversationTranscriptDisplay(projected);
    expect(nodes).toHaveLength(1);
    expect(nodes[0]).toMatchObject({ kind: "collaboration", seq: 1 });
    if (nodes[0].kind !== "collaboration") throw new Error("expected collaboration node");
    expect(nodes[0].item.collaboration.after_state.kind).toBe("question");
    if (nodes[0].item.collaboration.after_state.kind !== "question") throw new Error("expected question state");
    expect(nodes[0].item.collaboration.after_state.question.target.session_id).toBe("session-target");
  });

  it("keeps the serde notification envelope as an independent HostOnly timeline node", () => {
    const event = collaborationNotificationSentFixture();
    const projected = projectConversationEventDisplay([summary(2, event, 102, 1, "host_only")]);
    expect(projected).toHaveLength(1);
    expect(projected[0]).toMatchObject({
      kind: "collaboration",
      collaboration: {
        kind: "collaboration",
        event_kind: "notification_sent",
        after_state: {
          kind: "outbound_notification",
          notification: {
            notification_id: "notification-1",
            body: "The child agent reached the validation phase.",
          },
        },
      },
    });
    const nodes = projectConversationTranscriptDisplay(projected);
    expect(nodes).toHaveLength(1);
    expect(nodes[0]).toMatchObject({ kind: "collaboration", seq: 2 });
  });

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

  // Only consecutive same-turn reasoning belongs to the final response; earlier tools and Turns remain intact.
  // 仅连续的同回合思考归属最终回复；之前的工具与其他回合保持完整。
  it("attaches trailing reasoning without crossing a tool or Turn boundary", () => {
    // events contain tool-owned reasoning followed by two final-owned reasoning segments.
    // events 包含工具所属思考，以及两段最终回复所属思考。
    const events = [summary(1, reasoning("tool reasoning")), summary(2, tool(1, "completed")),
      summary(3, { ...reasoning("final first"), api_call_id: 12 } as ConversationEventBody), summary(4, { ...reasoning("final second"), api_call_id: 12 } as ConversationEventBody), summary(5, { kind: "assistant_text", api_call_id: 12, text: "answer", status: "completed" })];
    // nodes use the production projector and preserve source sequence identities.
    // nodes 使用生产投影器并保留源序列身份。
    const nodes = projectConversationTranscriptDisplay(projectConversationEventDisplay(events));
    expect(nodes.map(node => node.kind)).toEqual(["process_group", "assistant_final"]);
    expect(nodes[0].kind === "process_group" && nodes[0].items.map(item => item.seq)).toEqual([1, 2]);
    expect(nodes[1].kind === "assistant_final" && nodes[1].leading_reasoning.map(item => item.seq)).toEqual([3, 4]);
    // direct requires no empty process group; different-turn reasoning must not disappear into the answer.
    // direct 不应产生空过程组；不同回合的思考不能被并入当前回答而消失。
    const direct = projectConversationTranscriptDisplay(projectConversationEventDisplay([summary(1, reasoning("first")), summary(2, assistant("answer"))]));
    expect(direct.map(node => node.kind)).toEqual(["assistant_final"]);
    const separate = projectConversationTranscriptDisplay(projectConversationEventDisplay([summary(1, reasoning("previous turn")), { ...summary(2, assistant("answer")), turn_id: 8 }]));
    expect(separate.map(node => node.kind)).toEqual(["process_group", "assistant_final"]);
    expect(separate[1].kind === "assistant_final" && separate[1].leading_reasoning).toEqual([]);
  });

  it("uses typed origin to hide the provider envelope while retaining the agent card and real user text", () => {
    // Identical text cannot confer host identity or hide a genuine human input.
    // 相同正文不能取得宿主身份，也不能隐藏真实的人类输入。
    const text = '<vulcan_agent_message kind="question">Who is asking?</vulcan_agent_message>';
    const nodes = projectConversationTranscriptDisplay(projectConversationEventDisplay([
      summary(1, {
        kind: "user_input", client_action_id: clientActionId("human-quote"),
        input_id: 1, input_source: "root", text,
      }),
      summary(2, collaborationQuestionCreatedFixture(), 10, 1, "host_only"),
      summary(3, {
        kind: "user_input", client_action_id: clientActionId("agent-message:delivery-1"),
        input_id: 3, input_source: "agent_message", text,
      }),
    ]));
    expect(nodes.map((node) => node.kind)).toEqual(["user_input", "collaboration"]);
    expect(nodes[0].seq).toBe(1);
    expect(nodes[1].seq).toBe(2);
  });

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
      summary(10, { kind: "turn_boundary", reason: "completed", outcome: "completed", purpose: "business" }),
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
