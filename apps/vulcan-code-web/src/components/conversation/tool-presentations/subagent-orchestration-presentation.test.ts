import { describe, expect, it } from "vitest";
import type { ToolDetailPayload } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { hydrateSubagentOrchestrationPresentation, resolveSubagentOrchestrationPresentation } from "./subagent-orchestration-presentation";

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
  // Minimal host receipts preserve status while arbitrary JSON inside Markdown remains source content.
  // 最小宿主回执保留状态，同时 Markdown 内任意 JSON 保持为源码内容。
  it.each(["messages", "timeout", "no_pending_work", "needs_reply", "check_failed", "interrupted"])("reads Markdown wait status %s without parsing its body", (status) => {
    const body = '# Agent wait\n\n```json\n{"status":"not-the-envelope"}\n```';
    const blocks = collaborationBlock("wait_agents_result", { timeout_ms: 1000 }, body);
    blocks[0].rawMetadata = { ...blocks[0].rawMetadata as Record<string, unknown>, host_result: {
      kind: "agent_wait_ack_v1", payload: { format: "markdown", status, interrupted: status === "interrupted", reports: [], context_messages_pending: status === "messages" },
    } };
    const presentation = resolveSubagentOrchestrationPresentation(blocks, "completed");
    expect(presentation).toMatchObject({ result: body, resultFormat: "markdown", timedOut: status === "timeout",
      noActiveAgents: status === "no_pending_work", waitNeedsReply: status === "needs_reply", waitCheckFailed: status === "check_failed",
      interrupted: status === "interrupted" });
  });

  // The question ID joins live domain state; delayed details carry no duplicate answer body.
  // 问题身份关联实时领域状态；延迟详情不携带重复答案正文。
  it("hydrates a Markdown question receipt using the exact host identity", () => {
    const pending = resolveSubagentOrchestrationPresentation(collaborationBlock("ask_question", {
      target_session_id: "recipient", question: "Inspect the failure.",
    }, ""), "completed");
    expect(pending).not.toBeNull();
    const body = "# Question\n\nQuestion: q1\n";
    const hydrated = hydrateSubagentOrchestrationPresentation(pending!, {
      body, metadata: { host_result: { kind: "collaboration_receipt_v1", payload: { format: "markdown", question_id: "q1" } } },
    });
    expect(hydrated).toMatchObject({ questionId: "q1", result: body, resultFormat: "markdown", answerText: null });
  });
  // Unified waiting exposes meaningful inbox outcomes without raw protocol details.
  // 统一等待展示有意义的收件结果，不显示原始协议详情。
  it.each(["messages", "timeout", "no_pending_work", "needs_reply", "check_failed"])("recognizes unified inbox outcome %s", (status) => {
    const presentation = resolveSubagentOrchestrationPresentation(collaborationBlock("wait_agents_result", { timeout_ms: 1000 },
      JSON.stringify({ status, reports: [], context_messages_pending: status === "messages" })), "completed");
    expect(presentation).toMatchObject({ operation: "wait", timedOut: status === "timeout",
      noActiveAgents: status === "no_pending_work", terminalEventReceived: status === "messages", result: null,
      waitNeedsReply: status === "needs_reply", waitCheckFailed: status === "check_failed" });
  });
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

  it("shares a Markdown agent listing with only a header count, including delayed hydration", () => {
    const body = "# Open agents\n\nworker: completed\n\n```\n{\"result\":\"literal output\"}\n```";
    const host = { kind: "subagent_receipt_v1", payload: { format: "markdown", agent_count: 1 } };
    const presentation = resolveSubagentOrchestrationPresentation([{
      body, id: "listing", kind: "tool-result", rawMetadata: { host_result: host, tool_name: "list_agents" },
      summary: "", title: "list_agents", toolName: "list_agents",
    }], "completed");
    expect(presentation).toMatchObject({ agentsKnown: true, agents: [], agentCount: 1, result: body, resultFormat: "markdown" });
    expect(hydrateSubagentOrchestrationPresentation(presentation!, { body, metadata: { host_result: host } } as ToolDetailPayload))
      .toMatchObject({ agentsKnown: true, agents: [], agentCount: 1, result: body, resultFormat: "markdown" });
  });
});
