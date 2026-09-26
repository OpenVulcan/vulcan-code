import { describe, expect, it } from "vitest";

import {
  createConversationBranchPresentationProjectorV1,
  projectConversationBranchPresentationV1,
} from "./conversation-branch-presentation-v1";
import { clientActionId, type ConversationEventSummary } from "@/service/conversation-event-window-store";
import type { ConversationSessionResourceCatalogV1 } from "@/state/conversation-session-resources-v1";

// SESSION_ID owns every event in these streaming sequences.
// SESSION_ID 拥有这些流式序列中的全部事件。
const SESSION_ID = "session-incremental";

// RESOURCES is one stable catalog, as the resources hook returns while nothing changes.
// RESOURCES 是一份稳定目录，与资源 hook 在无变化时返回的一致。
const RESOURCES: ConversationSessionResourceCatalogV1 = {
  contentUrl: () => "",
  error: null,
  resources: new Map(),
  sessionId: SESSION_ID,
  status: "ready",
};

// t is one stable translator identity.
// t 是一个稳定的翻译函数身份。
const t = (key: string) => key;

// summary builds one V1 envelope; replacing an event means building a new object for the same seq.
// summary 构建一条 V1 envelope；替换事件即为同一 seq 构建新对象。
function summary(
  seq: number,
  turnId: number,
  event: ConversationEventSummary["event"],
  visibility: ConversationEventSummary["visibility"] = "provider",
): ConversationEventSummary {
  return { seq, turn_id: turnId, created_at_ms: 1_000 + seq, updated_at_ms: 1_000 + seq, event_revision: 1, visibility, event };
}

// apiStats builds one terminal API receipt.
// apiStats 构建一条终态 API 回执。
function apiStats(seq: number, turnId: number, apiCallId: number, completedAtMs: number): ConversationEventSummary {
  return summary(seq, turnId, {
    kind: "api_stats",
    api_call_id: apiCallId,
    purpose: "main_generation",
    provider: "openai",
    model: "gpt-v1",
    usage: { input_tokens: 20, output_tokens: 5, total_tokens: 25 },
    status: "completed",
    continuation_eligibility: "forbidden",
    timing: { first_event_at_ms: 1_000, completed_at_ms: completedAtMs },
  }, "ui_only");
}

// streamingWindows models the event window after each live delta: the store keeps every unchanged event object and
// replaces only the events a delta touched, so each window reuses the previous objects except the replaced ones.
// streamingWindows 模拟每次实时增量后的事件窗口：store 保留所有未变化的事件对象，只替换增量触及的事件，因此每个
// 窗口除被替换的事件外都复用上一窗口的对象。
function streamingWindows(): ConversationEventSummary[][] {
  const bucket = { api_call_count: 2, unknown_usage_api_call_count: 0, tool_call_count: 1, subagent_return_count: 0, usage: { output_tokens: 12 } };
  const emptyBucket = { api_call_count: 0, unknown_usage_api_call_count: 0, tool_call_count: 0, subagent_return_count: 0, usage: {} };
  const windows: ConversationEventSummary[][] = [];
  let current: ConversationEventSummary[] = [];
  // put replaces the event with the same seq, or appends a new one, and records the resulting window.
  // put 替换同一 seq 的事件或追加新事件，并记录得到的窗口。
  const put = (...next: ConversationEventSummary[]) => {
    const updated = [...current];
    for (const event of next) {
      const index = updated.findIndex((candidate) => candidate.seq === event.seq);
      if (index === -1) updated.push(event);
      else updated[index] = event;
    }
    current = updated;
    windows.push(current);
  };
  put(summary(1, 1, { kind: "user_input", client_action_id: clientActionId("incremental-root-1"), input_id: 1, input_source: "root", text: "检查实现" }));
  put(summary(2, 1, { kind: "assistant_reasoning", api_call_id: 1, text: "先", status: "streaming" }));
  put(summary(2, 1, { kind: "assistant_reasoning", api_call_id: 1, text: "先核对调用链", status: "streaming" }));
  put(summary(2, 1, { kind: "assistant_reasoning", api_call_id: 1, text: "先核对调用链。", status: "completed" }));
  put(summary(3, 1, { kind: "tool_call", api_call_id: 1, tool_call_id: 1, tool_name: "shell_command", input: { command: "rg chain" }, request_stream_id: "incremental-tool-1", status: "running" }));
  // The receipt of API 1 changes the reasoning duration without replacing the reasoning event itself.
  // API 1 的回执改变思考耗时，但不替换思考事件本身。
  put(apiStats(4, 1, 1, 1_450));
  put(summary(3, 1, { kind: "tool_call", api_call_id: 1, tool_call_id: 1, tool_name: "shell_command", input: { command: "rg chain" }, request_stream_id: "incremental-tool-1", output: { stdout: "one match" }, status: "completed" }));
  put(summary(5, 1, { kind: "assistant_text", api_call_id: 2, text: "已经", status: "streaming" }));
  put(summary(5, 1, { kind: "assistant_text", api_call_id: 2, text: "已经完成核对。", status: "streaming" }));
  put(summary(5, 1, { kind: "assistant_text", api_call_id: 2, text: "已经完成核对。", status: "completed" }), apiStats(6, 1, 2, 1_900));
  put(
    summary(7, 1, { kind: "turn_boundary", reason: "completed", outcome: "completed", purpose: "business" }),
    summary(8, 1, { kind: "turn_stats", start_seq: 1, end_seq: 8, outcome: "completed", duration_ms: 900, buckets: { direct: bucket, subagent: emptyBucket, total: bucket } }, "ui_only"),
  );
  put(summary(9, 2, { kind: "user_input", client_action_id: clientActionId("incremental-root-2"), input_id: 2, input_source: "root", text: "继续" }));
  put(summary(10, 2, { kind: "assistant_text", api_call_id: 3, text: "好", status: "streaming" }));
  // Editing the root input replaces it, which changes the root-input metadata of every event in its Turn.
  // 编辑根输入会替换它，从而改变其 Turn 内每条事件的根输入元数据。
  put(summary(9, 2, { kind: "user_input", client_action_id: clientActionId("incremental-root-2"), input_id: 2, input_source: "root", text: "继续检查" }));
  return windows;
}

describe("createConversationBranchPresentationProjectorV1", () => {
  // Every streamed window must project exactly as a from-scratch projection of the same window.
  // 每个流式窗口的投影必须与对同一窗口从零开始的投影完全一致。
  it("matches the full projection after every streaming delta", () => {
    const project = createConversationBranchPresentationProjectorV1();
    for (const events of streamingWindows()) {
      expect(project(SESSION_ID, events, RESOURCES, t)).toEqual(projectConversationBranchPresentationV1(SESSION_ID, events, RESOURCES, t));
    }
  });

  // Rows untouched by a delta keep their object identity, so memoized rows downstream skip rendering.
  // 未被增量触及的行保持对象身份，使下游已记忆化的行跳过渲染。
  it("reuses rows and messages whose events did not change", () => {
    const windows = streamingWindows();
    const project = createConversationBranchPresentationProjectorV1();
    // before is the window where the final answer is still streaming; after replaces only that answer.
    // before 是最终回答仍在流式输出的窗口；after 只替换该回答。
    const before = windows.find((events) => events.some((event) => event.event.kind === "assistant_text" && event.event.text === "已经"))!;
    const after = windows[windows.indexOf(before) + 1]!;
    const first = project(SESSION_ID, before, RESOURCES, t);
    const second = project(SESSION_ID, after, RESOURCES, t);

    const answerId = `v1:${after.find((event) => event.event.kind === "assistant_text")!.seq}`;
    for (const item of second.renderItems) {
      const prior = first.renderItems.find((candidate) => candidate.renderKey === item.renderKey);
      if (item.sourceMessages.some((message) => message.id === answerId)) {
        expect(item).not.toBe(prior);
      } else {
        expect(item).toBe(prior);
      }
    }
    expect(second.messages.find((message) => message.id === "v1:1")).toBe(first.messages.find((message) => message.id === "v1:1"));

    // Re-projecting an identical window returns the previous rows array.
    // 重新投影完全相同的窗口会返回上一行数组。
    expect(project(SESSION_ID, [...after], RESOURCES, t).renderItems).toBe(second.renderItems);
  });

  // A changed translator or resource catalog invalidates every cached projection.
  // 翻译函数或资源目录变化会使全部缓存投影失效。
  it("rebuilds everything when a shared input changes", () => {
    const windows = streamingWindows();
    const events = windows[windows.length - 1]!;
    const project = createConversationBranchPresentationProjectorV1();
    const first = project(SESSION_ID, events, RESOURCES, t);
    const otherT = (key: string) => `[${key}]`;
    const translated = project(SESSION_ID, events, RESOURCES, otherT);

    expect(translated).toEqual(projectConversationBranchPresentationV1(SESSION_ID, events, RESOURCES, otherT));
    expect(translated.renderItems[0]).not.toBe(first.renderItems[0]);
    const otherResources = { ...RESOURCES };
    expect(project(SESSION_ID, events, otherResources, otherT).messages[0]).not.toBe(translated.messages[0]);
  });
});
