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

/** Display-only task messages decoded from a child root input rendered by the host inbox.
 * 从宿主 inbox 渲染的子会话根输入中解码的只读任务消息。 */
export interface ConversationAgentTaskInput {
  /** First envelope identity, bound by the root input action id.
   * 首个信封身份，由根输入动作身份绑定。 */
  eventId: string;
  /** Host-declared sender and recipient shared by every envelope.
   * 所有信封共享的宿主声明收发方。 */
  source: string;
  target: string;
  /** Unescaped literal bodies in delivery order.
   * 按投递顺序解转义后的原始正文。 */
  bodies: string[];
}

/** Decodes a root input whose text is only host task envelopes; anything else returns null.
 * 解码正文仅由宿主任务信封组成的根输入；其他情况返回空。
 * The inbox resume names its first envelope in the action id `agent-message:{event_id}:{attempt}`
 * (inter_agent_execution_id in vulcan-agent-runtime task_runtime/agent_control.rs); typed prompts never match.
 * inbox 续跑在动作身份 `agent-message:{event_id}:{attempt}` 中指名首个信封
 * （vulcan-agent-runtime task_runtime/agent_control.rs 的 inter_agent_execution_id）；人工输入不会匹配。 */
export function decodeAgentTaskInput(
  input: Extract<ConversationEventBody, { kind: "user_input" }>,
): ConversationAgentTaskInput | null {
  if (input.input_source !== "root" || !input.client_action_id.startsWith("agent-message:")
    || !input.text.startsWith("<vulcan_agent_message ")) return null;
  // Envelopes are joined by blank lines, so a synthetic root holds them; only whitespace may separate them.
  // 信封以空行连接，因此用合成根节点承载；信封之间只允许空白。
  const document = new DOMParser().parseFromString(`<envelopes>${input.text}</envelopes>`, "application/xml");
  if (document.doctype || document.querySelector("parsererror")) return null;
  const nodes = [...document.documentElement.childNodes];
  if (nodes.some((node) => node.nodeType !== Node.ELEMENT_NODE && (node.nodeType !== Node.TEXT_NODE || node.textContent?.trim()))) return null;
  const envelopes = nodes.filter((node): node is Element => node.nodeType === Node.ELEMENT_NODE);
  const first = envelopes[0];
  const eventId = first?.getAttribute("event_id");
  const source = first?.getAttribute("source");
  const target = first?.getAttribute("target");
  if (!first || !eventId?.trim() || !source?.trim() || !target?.trim()) return null;
  const valid = envelopes.every((envelope) => envelope.tagName === "vulcan_agent_message" && envelope.children.length === 0
    && envelope.getAttribute("version") === "1" && ["task_start", "task_steer"].includes(envelope.getAttribute("kind") ?? "")
    && envelope.getAttribute("source") === source && envelope.getAttribute("target") === target);
  // The attempt suffix keeps retries unique; it must be present but carries no display meaning.
  // 尝试后缀保证重试唯一；必须存在但不具展示含义。
  const prefix = `agent-message:${eventId}:`;
  if (!valid || !input.client_action_id.startsWith(prefix) || input.client_action_id.length === prefix.length) return null;
  return { eventId, source, target, bodies: envelopes.map((envelope) => envelope.textContent ?? "") };
}
