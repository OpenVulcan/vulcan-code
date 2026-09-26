import { describe, expect, it } from "vitest";
import type { ToolExecutionConversationBlock } from "../conversation-tool-execution-types";
import { hydrateAskQuestionPresentation, resolveAskQuestionPresentation } from "./ask-question-presentation";

// The fixture includes surrogate pairs, separators and newlines inside one exact answer.
// 夹具在单个精确答案中包含代理对、分隔符和换行。
const answer = "唯一答案🦀, one\ntwo";
const body = `## User Answers\n\n- Scope: ${answer}\n\nContinue with these answers.`;
const receipt = { format: "markdown" };

// Builds the exact normalized Ask event metadata used by both live and historical cards.
// 构建实时与历史卡共同使用的精确规范化 Ask 事件元数据。
function blocks(output: string, host = false): ToolExecutionConversationBlock[] {
  return [{
    id: "ask", toolName: "ask", kind: "tool-result", title: "Ask", summary: "", body: output,
    rawMetadata: { arguments: { questions: [{ header: "Scope", question: "Continue?" }] },
      ...(host ? { host_result: { kind: "ask", payload: receipt } } : {}) },
  }];
}

describe("Ask shared answer body", () => {
  it("shares exact Unicode answers in immediate and delayed results", () => {
    const direct = resolveAskQuestionPresentation(blocks(body, true), "completed")!;
    const pending = resolveAskQuestionPresentation(blocks(""), "completed")!;
    const hydrated = hydrateAskQuestionPresentation(pending, {
      body, metadata: { host_result: { kind: "ask", payload: receipt } },
    });
    expect(direct.questions).toEqual(hydrated.questions);
    expect(direct.answerMarkdown).toBe(body);
    expect(hydrated.answerMarkdown).toBe(body);
    expect(direct.questions[0].question).toBe("Continue?");
  });
  it("renders the final redacted body without stale positional metadata", () => {
    const redacted = body.replace(answer, "[redacted]");
    expect(resolveAskQuestionPresentation(blocks(redacted, true), "completed")!.answerMarkdown).toBe(redacted);
  });
  it("retains the historical structured answer contract", () => {
    const input = blocks("");
    input[0].rawMetadata = { arguments: { questions: [{ header: "Scope", question: "Continue?" }] },
      host_result: { kind: "ask", payload: { answers: [["Yes"]] } } };
    expect(resolveAskQuestionPresentation(input, "completed")!.questions[0].answers).toEqual(["Yes"]);
  });
});
