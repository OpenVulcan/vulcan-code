import { describe, expect, it } from "vitest";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { resolveSubagentOrchestrationPresentation } from "./subagent-orchestration-presentation";

// collaborationBlock builds one exact native collaboration invocation and opaque generic output.
// collaborationBlock 构建一条精确原生协作调用与不透明通用输出。
function collaborationBlock(
  toolName: string,
  argumentsValue: Record<string, unknown>,
  output: string,
): ToolExecutionConversationBlock[] {
  return [{
    body: output,
    id: "collaboration-row",
    kind: "tool-result",
    rawMetadata: {
      arguments: argumentsValue,
      tool_name: toolName,
    },
    summary: output,
    title: toolName,
    toolCallId: "collaboration-call",
    toolName,
  }];
}

describe("native collaboration orchestration presentation", () => {
  it("distinguishes a question and preserves only exact arguments", () => {
    const presentation = resolveSubagentOrchestrationPresentation(
      collaborationBlock("ask_question", {
        target_session_id: "target-session",
        question: "需要确认哪个目录？",
      }, "{\"accepted\":true}"),
      "completed",
    );

    expect(presentation).toMatchObject({
      operation: "collaboration_question",
      prompt: "需要确认哪个目录？",
      collaborationArguments: {
        target_session_id: "target-session",
        question: "需要确认哪个目录？",
      },
      result: "{\"accepted\":true}",
    });
  });

  it("distinguishes queue direction from a question answer receipt", () => {
    const direction = resolveSubagentOrchestrationPresentation(
      collaborationBlock("send_session_message", {
        session_id: "target-session",
        message: "请先检查测试失败原因。",
        delivery_mode: "queue",
      }, "{\"delivery_id\":\"delivery-1\"}"),
      "completed",
    );
    const answer = resolveSubagentOrchestrationPresentation(
      collaborationBlock("reply_question", {
        question_id: "question-1",
        answer: "已确认，继续执行。",
        unable_to_answer: false,
      }, "{\"state\":\"answered\"}"),
      "completed",
    );

    expect(direction).toMatchObject({
      operation: "collaboration_steer",
      deliveryMode: "queue",
      queued: true,
      prompt: "请先检查测试失败原因。",
    });
    expect(answer).toMatchObject({
      operation: "collaboration_answer",
      prompt: "已确认，继续执行。",
      result: "{\"state\":\"answered\"}",
    });
  });

  it("renders phase notifications as a distinct non-question collaboration operation", () => {
    const presentation = resolveSubagentOrchestrationPresentation(
      collaborationBlock("send_notification", {
        target_session_id: "target-session",
        message: "阶段一已完成，开始验证。",
      }, "{\"accepted\":true}"),
      "completed",
    );

    expect(presentation).toMatchObject({
      operation: "collaboration_notification",
      prompt: "阶段一已完成，开始验证。",
      collaborationArguments: {
        target_session_id: "target-session",
        message: "阶段一已完成，开始验证。",
      },
      result: "{\"accepted\":true}",
    });
  });

  it("does not infer a service result field for session waits", () => {
    const presentation = resolveSubagentOrchestrationPresentation(
      collaborationBlock("wait_session", {
        session_id: "target-session",
        after_revision: 3,
        timeout_ms: 1000,
      }, "plain generic output"),
      "completed",
    );

    expect(presentation).toMatchObject({
      operation: "collaboration_session_wait",
      collaborationArguments: {
        session_id: "target-session",
        after_revision: 3,
        timeout_ms: 1000,
      },
      result: "plain generic output",
    });
    expect(presentation).not.toHaveProperty("timedOut", true);
  });

  it("retains existing wait-agent payload agents", () => {
    const presentation = resolveSubagentOrchestrationPresentation([{
      body: "",
      id: "wait-row",
      kind: "tool-result",
      rawMetadata: {
        host_result: {
          kind: "subagent_wait",
          payload: {
            agents: [{ task_name: "child", status: "completed" }],
            terminal_event_count: 1,
          },
        },
        tool_name: "wait_agent",
      },
      summary: "",
      title: "wait_agent",
      toolName: "wait_agent",
    }], "completed");

    expect(presentation?.agents).toHaveLength(1);
  });
});
