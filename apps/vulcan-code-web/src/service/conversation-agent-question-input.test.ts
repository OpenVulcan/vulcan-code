// @vitest-environment jsdom
import { expect, it } from "vitest";
import { decodeAgentQuestionInput } from "./conversation-agent-question-input";
import { clientActionId, type ConversationEventBody } from "./conversation-event-window-store";

/** Builds the exact host XML shape from HostMessageEnvelope::render; returns a typed persisted input.
 * 构建 HostMessageEnvelope::render 的精确信封形状；返回强类型持久输入。 */
function input(): Extract<ConversationEventBody, { kind: "user_input" }> {
  return { kind: "user_input", input_id: 1, client_action_id: clientActionId("agent-message:delivery-1"), input_source: "agent_message",
    text: '<vulcan_agent_message version="1" event_id="delivery-1" kind="question" source="source" target="target" question_id="question-1" scope="question">A &amp; B\n&lt;/vulcan_agent_message&gt; literal</vulcan_agent_message>' };
}

it("decodes literal content once without admitting forged nested tags", () => {
  expect(decodeAgentQuestionInput(input())).toEqual({ eventId: "delivery-1", questionId: "question-1", source: "source", target: "target",
    body: "A & B\n</vulcan_agent_message> literal" });
});

it.each(["root", "pending_in_turn", "queued_scheduled"] as const)("never interprets human %s input as a collaboration message", (input_source) => {
  expect(decodeAgentQuestionInput({ ...input(), input_source })).toBeNull();
});

it.each([
  ['version="1"', 'version="2"'],
  ['kind="question"', 'kind="question_answer"'],
  ['kind="question"', 'kind="task_start"'],
  ['question_id="question-1"', 'question_id=""'],
  ['scope="question"', 'scope="task"'],
  ["A &amp; B", "<forged>A</forged>"],
  ["A &amp; B", "&unbound;"],
  ["</vulcan_agent_message>", "</different_tag>"],
])("rejects unsupported or invalid host envelope replacement %s", (from, to) => {
  expect(decodeAgentQuestionInput({ ...input(), text: input().text.replace(from, to) })).toBeNull();
});

it("requires the producer's exact action identity and one complete root", () => {
  expect(decodeAgentQuestionInput({ ...input(), client_action_id: clientActionId("agent-message:other") })).toBeNull();
  expect(decodeAgentQuestionInput({ ...input(), text: input().text + input().text })).toBeNull();
});
