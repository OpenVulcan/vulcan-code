import type { ConversationEventBody } from "./conversation-event-window-store";

/** Display-only question decoded from the host-authored V1 XML envelope.
 * 从宿主生成的 V1 XML 信封解码的只读问题展示数据。 */
export interface ConversationAgentQuestionInput {
  /** Immutable envelope and question identities.
   * 不可变信封与问题身份。 */
  eventId: string;
  questionId: string;
  /** Host-declared sender and recipient, never inferred from prose.
   * 宿主声明的收发方，绝不从正文推断。 */
  source: string;
  target: string;
  /** Unescaped literal question body, rendered through the shared Markdown component.
   * 解转义后的原始问题正文，通过共享 Markdown 组件渲染。 */
  body: string;
}

/** Decodes only authenticated agent-message question inputs; unrelated or invalid envelopes return null.
 * 仅解码已标记为代理消息的问题输入；无关或无效信封返回空。
 * The wire contract is HostMessageEnvelope::render in vulcan-ai-contract/src/host_message.rs.
 * 线缆契约来自 vulcan-ai-contract/src/host_message.rs 的 HostMessageEnvelope::render。 */
export function decodeAgentQuestionInput(
  input: Extract<ConversationEventBody, { kind: "user_input" }>,
): ConversationAgentQuestionInput | null {
  if (input.input_source !== "agent_message" || !input.text.startsWith("<vulcan_agent_message ")) return null;
  // XML parses only a host envelope, never human/assistant text; reject markup or document declarations in its body.
  // XML 仅解析宿主信封，绝不解析人类或助手文本；拒绝正文中的标记及文档声明。
  const document = new DOMParser().parseFromString(input.text, "application/xml");
  const envelope = document.documentElement;
  if (document.doctype || document.querySelector("parsererror") || document.childNodes.length !== 1
    || envelope.tagName !== "vulcan_agent_message" || envelope.children.length !== 0
    || envelope.getAttribute("version") !== "1" || envelope.getAttribute("kind") !== "question"
    || envelope.getAttribute("scope") !== "question") return null;
  // Match the exact action identity produced by append_envelope before treating text as a question card.
  // 将文本视为问题卡之前，核对 append_envelope 生成的精确动作身份。
  const eventId = envelope.getAttribute("event_id");
  const questionId = envelope.getAttribute("question_id");
  const source = envelope.getAttribute("source");
  const target = envelope.getAttribute("target");
  if (!eventId?.trim() || !questionId?.trim() || !source?.trim() || !target?.trim()
    || input.client_action_id !== `agent-message:${eventId}`) return null;
  return { eventId, questionId, source, target, body: envelope.textContent ?? "" };
}
