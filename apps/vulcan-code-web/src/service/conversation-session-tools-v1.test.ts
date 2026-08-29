import { describe, expect, it } from "vitest";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { ConversationEventSummary, ToolPayload } from "@/service/conversation-event-window-store";
import {
  mergeConversationSessionTerminalsV1,
  selectConversationSessionTerminalsV1,
  selectConversationSessionTodolistV1,
} from "@/service/conversation-session-tools-v1";

/** Builds one atomic V1 window fixture for session-tool selector tests.
 * 为会话工具选择器测试构建一份原子 V1 窗口夹具。
 *
 * Parameters: events are the canonical events contained by the fixture.
 * 参数：events 是夹具包含的 canonical 事件。
 * Returns: a complete selected-session controller snapshot.
 * 返回值：一份完整的选中会话控制器快照。
 */
function snapshot(events: ConversationEventSummary[]): ConversationEventWindowControllerSnapshot {
  return {
    owner: { session_id: "session-v1", window_generation: 6 },
    service_epoch: "epoch-v1",
    memory_revision: 12,
    durable_revision: 12,
    events,
    older_before_seq: null,
    has_more: false,
    open_turn_id: null,
    open_turn_start_seq: null,
    session_config: null,
  };
}

/** Builds one completed V1 tool event fixture with an exact persisted output envelope.
 * 使用精确持久输出封装构建一条已完成 V1 工具事件夹具。
 *
 * Parameters: seq identifies the event, toolName selects the tool, output is its canonical payload, and externalTerminalId optionally binds a terminal reference.
 * 参数：seq 标识事件，toolName 选择工具，output 是其 canonical 载荷，externalTerminalId 可选绑定终端引用。
 * Returns: one complete conversation event summary.
 * 返回值：一条完整会话事件摘要。
 */
function completedTool(
  seq: number,
  toolName: string,
  output: ToolPayload,
  externalTerminalId?: string,
): ConversationEventSummary {
  return {
    seq,
    turn_id: 9,
    created_at_ms: seq * 100,
    updated_at_ms: seq * 100 + 7,
    event_revision: 3,
    visibility: "provider",
    event: {
      kind: "tool_call",
      api_call_id: seq,
      tool_call_id: seq + 1,
      request_stream_id: `request-${seq}`,
      tool_name: toolName,
      input: {},
      output,
      status: "completed",
      ...(externalTerminalId
        ? { external_refs: [{ kind: "terminal" as const, id: externalTerminalId }] }
        : {}),
    },
  };
}

describe("conversation session tools V1", () => {
  it("projects only the latest exact durable todolist", () => {
    const result = selectConversationSessionTodolistV1(snapshot([
      completedTool(4, "todolist", {
        host_result: {
          kind: "todolist",
          payload: {
            explanation: "first",
            items: [{ step: "step one", status: "in_progress" }],
          },
        },
      }),
      completedTool(8, "todolist", {
        host_result: {
          kind: "todolist",
          payload: {
            explanation: "latest",
            items: [
              { step: "step one", status: "completed" },
              { step: "step two", status: "in_progress" },
            ],
          },
        },
      }),
    ]));

    expect(result).toEqual({
      explanation: "latest",
      items: [
        { step: "step one", status: "completed" },
        { step: "step two", status: "in_progress" },
      ],
      completedCount: 1,
      totalCount: 2,
      updatedAt: "807",
    });
  });

  it("does not revive an older todolist when the latest matching payload is malformed", () => {
    const result = selectConversationSessionTodolistV1(snapshot([
      completedTool(4, "todolist", {
        host_result: {
          kind: "todolist",
          payload: { items: [{ step: "valid", status: "pending" }] },
        },
      }),
      completedTool(8, "todolist", {
        host_result: {
          kind: "todolist",
          payload: { items: [{ step: "invalid", status: "unknown" }] },
        },
      }),
    ]));

    expect(result).toBeNull();
  });

  it("restores the latest exact durable terminal snapshot", () => {
    const terminalId = "terminal-v1";
    const result = selectConversationSessionTerminalsV1(snapshot([
      completedTool(10, "terminal_session", {
        host_result: {
          kind: "terminal_session",
          payload: {
            terminal_session_id: terminalId,
            owner_session_id: "session-v1",
            command: "pwsh",
            status: "running",
            name: "dev shell",
            cwd: "D:/project",
            shell: "pwsh",
            backend: "pty",
            screen_frame_seq: 3,
            screen_size: { rows: 24, cols: 100 },
            screen_text: "ready",
            exit_code: null,
            failure_reason: null,
          },
        },
      }, terminalId),
      completedTool(12, "terminal_session", {
        host_result: {
          kind: "terminal_session",
          payload: {
            terminal_session_id: terminalId,
            owner_session_id: "session-v1",
            command: "pwsh",
            status: "running",
            name: "dev shell",
            cwd: "D:/project",
            shell: "pwsh",
            backend: "pty",
            screen_frame_seq: 5,
            screen_size: { rows: 30, cols: 120 },
            screen_text: "latest",
            exit_code: null,
            failure_reason: null,
          },
        },
      }, terminalId),
    ]));

    expect(result).toEqual([expect.objectContaining({
      id: terminalId,
      ownerSessionId: "session-v1",
      status: "running",
      preview: "latest",
      screenFrameSeq: 5,
      screenRows: 30,
      screenCols: 120,
    })]);
  });

  it("rejects a terminal payload whose local identity disagrees with the external reference", () => {
    const result = selectConversationSessionTerminalsV1(snapshot([
      completedTool(10, "terminal_session", {
        host_result: {
          kind: "terminal_session",
          payload: {
            terminal_session_id: "different-terminal",
            owner_session_id: "session-v1",
            command: "pwsh",
            status: "running",
          },
        },
      }, "terminal-v1"),
    ]));

    expect(result).toEqual([]);
  });

  it("does not revive an older terminal when the latest required payload shape is malformed", () => {
    const terminalId = "terminal-v1";
    const result = selectConversationSessionTerminalsV1(snapshot([
      completedTool(10, "terminal_session", {
        host_result: {
          kind: "terminal_session",
          payload: {
            terminal_session_id: terminalId,
            owner_session_id: "session-v1",
            command: "pwsh",
            status: "running",
            name: "dev shell",
            cwd: "D:/project",
            shell: "pwsh",
            backend: "pty",
            screen_frame_seq: 3,
            screen_size: { rows: 24, cols: 100 },
            screen_text: "ready",
            exit_code: null,
            failure_reason: null,
          },
        },
      }, terminalId),
      completedTool(12, "terminal_session", {
        host_result: {
          kind: "terminal_session",
          payload: {
            terminal_session_id: terminalId,
            owner_session_id: "session-v1",
            command: "pwsh",
            status: "running",
            name: "dev shell",
            cwd: "D:/project",
            shell: "pwsh",
            backend: "pty",
            screen_frame_seq: "not-a-sequence",
            screen_size: { rows: 24, cols: 100 },
            screen_text: "stale must not survive",
            exit_code: null,
            failure_reason: null,
          },
        },
      }, terminalId),
    ]));

    expect(result).toEqual([]);
  });

  it("overlays live runtime facts only on V1-owned terminal identities", () => {
    const merged = mergeConversationSessionTerminalsV1(
      [{ id: "terminal-v1", command: "pwsh", status: "running", preview: "durable", ownerSessionId: "session-v1" }],
      [
        { id: "terminal-v1", command: "pwsh", status: "running", preview: "live", ownerSessionId: "session-v1" },
        { id: "unowned-terminal", command: "cmd", status: "running", preview: "ignored", ownerSessionId: "session-v1" },
      ],
    );

    expect(merged).toEqual([
      { id: "terminal-v1", command: "pwsh", status: "running", preview: "live", ownerSessionId: "session-v1" },
    ]);
  });

  it("hides durable terminals that are absent, closed, lost, or owned by another live session", () => {
    const durable = [{
      id: "terminal-v1",
      command: "pwsh",
      status: "running",
      preview: "durable",
      ownerSessionId: "session-v1",
    }];

    expect(mergeConversationSessionTerminalsV1(durable, [])).toEqual([]);
    expect(mergeConversationSessionTerminalsV1(durable, [{
      ...durable[0],
      status: "completed",
      preview: "closed",
    }])).toEqual([]);
    expect(mergeConversationSessionTerminalsV1(durable, [{
      ...durable[0],
      lost: true,
      preview: "lost",
    }])).toEqual([]);
    expect(mergeConversationSessionTerminalsV1(durable, [{
      ...durable[0],
      ownerSessionId: "another-session",
      preview: "foreign",
    }])).toEqual([]);
  });
});
