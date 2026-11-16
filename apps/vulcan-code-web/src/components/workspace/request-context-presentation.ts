import type { TranslationKey } from "@/i18n";
import type { RequestContextNode } from "./request-context-dialog";

// Producer keys are taken from V1 SystemContext and the prompt source contract.
// 生产者键取自 V1 SystemContext 与提示词来源契约。
const CONTEXT_KEYS: Readonly<Record<string, TranslationKey>> = {
  "dynamic.html_answers": "requestContext.answerMode",
  "dynamic.permission_mode": "requestContext.permissionMode",
  "dynamic.runtime_context": "requestContext.runtimeContext",
  "dynamic.tool_guidance": "requestContext.toolGuidance",
  "dynamic.provider_tools": "skills.action.tools",
  "dynamic.host_message_guidance": "requestContext.hostGuidance",
  "vmm_profile_delta": "requestContext.memoryProfile",
};

// Exact wrapper tags rendered by project rules, memory and skill producers.
// 项目规则、记忆和技能生产者渲染的精确信封标签。
const INSTRUCTION_TAGS = {
  vulcan_context_agent_md: "requestContext.projectRules",
  vmm_profile: "requestContext.memoryProfile",
  vmm_profile_delta: "requestContext.memoryProfile",
  long_term_memory: "requestContext.memoryProfile",
  skills_instructions: "requestContext.promptSkills",
} as const satisfies Record<string, TranslationKey>;

/** Readable body and metadata from one explicitly tagged injection. */
/** 从一个带有明确标签的注入节点取得的可读正文与元数据。 */
interface ContextInjection {
  /** Localized classification of this injection. */
  /** 此注入的本地化分类。 */
  title: TranslationKey;
  /** Wrapper and ordered attributes retained for diagnostics. */
  /** 保留用于诊断的信封标签及有序属性。 */
  tag: string;
  attributes: Array<[string, string]>;
  /** Decoded body with the wrapper removed. */
  /** 移除信封后的已解码正文。 */
  body: string;
}

/** Resolves the known sources in the initial prompt manifest to display labels. */
/** 将初始提示词清单中已知的来源映射到显示标签。 */
export const PROMPT_ORIGIN_KEYS: Readonly<Record<string, TranslationKey>> = {
  system_prompt: "settings.modelExperience.systemPrompt",
  developer_prompt: "requestContext.developerPrompt",
  memory_profile: "requestContext.memoryProfile",
  agents_md: "requestContext.projectRules",
  vulcan_md: "requestContext.projectRules",
  agent_persona: "requestContext.personality",
  output_style: "requestContext.outputStyle",
  prompt_skill_injection: "requestContext.promptSkills",
  plugin_instructions: "requestContext.pluginInstructions",
  baseline_hooks: "requestContext.baselineHook",
};

/** Decodes only the entity forms emitted by Vulcan's XML text and attribute writers. */
/** 仅解码 Vulcan 的 XML 正文与属性写入器生成的实体形式。 */
function decodeEnvelopeText(text: string): string {
  // One pass prevents a literal encoded sequence in the original body from being decoded twice.
  // 单次遍历避免对正文原有的实体序列重复解码。
  const entities: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };
  return text.replace(/&(amp|lt|gt|quot|apos);/g, (_, entity: string) => entities[entity]);
}

/** Parses a bounded envelope, including its metadata when the inspector clipped the end of the body. */
/** 解析有界信封，即使检查器截短了正文尾部也保留可用元数据。 */
function parseInjection(text: string, truncated: boolean): ContextInjection | undefined {
  // Host envelopes always begin with their tag and use double-quoted XML attributes.
  // 宿主信封总是从标签开始并使用双引号 XML 属性。
  const open = /^<([a-z_]+)((?: [a-z_]+="[^"<>]*")*)>/.exec(text);
  if (!open) return undefined;
  const [, tag, rawAttributes] = open;
  const attributes = Array.from(rawAttributes.matchAll(/ ([a-z_]+)="([^"]*)"/g), ([, key, value]): [string, string] => (
    [key, decodeEnvelopeText(value)]
  ));
  const fields = new Map(attributes);
  // Recognize the V1 contract and its producer key, not a language-dependent snippet of the body.
  // 识别 V1 契约及其生产者键，不根据依赖语言的正文片段猜测。
  const dynamic = tag === "vulcan_context_update" || tag === "vulcan_context_expired";
  let title: TranslationKey;
  if (dynamic) {
    const kind = tag === "vulcan_context_update" ? "context_update" : "context_expired";
    if (fields.get("version") !== "1" || fields.get("kind") !== kind || fields.get("target") !== "model") {
      return undefined;
    }
    const producerKey = /^v1-context-(?:update|expired)-\d+-(.+)$/.exec(fields.get("event_id") ?? "")?.[1];
    title = producerKey && Object.hasOwn(CONTEXT_KEYS, producerKey) ? CONTEXT_KEYS[producerKey] : "requestContext.injection";
  } else if (Object.hasOwn(INSTRUCTION_TAGS, tag)) {
    title = INSTRUCTION_TAGS[tag as keyof typeof INSTRUCTION_TAGS];
  } else {
    return undefined;
  }
  // The wrapper is peeled only when it owns the whole node or the node is explicitly truncated.
  // 仅在信封涵盖整个节点或节点已明确截断时去除信封。
  const close = `</${tag}>`;
  const body = text.slice(open[0].length).trimEnd();
  if (!body.endsWith(close) && !truncated) return undefined;
  const inner = body.endsWith(close) ? body.slice(0, -close.length).trim() : body.trimStart();
  return { tag, title, attributes, body: dynamic || tag === "vulcan_context_agent_md" ? decodeEnvelopeText(inner) : inner };
}

/** Resolves a semantic title, preview body, and optional injection metadata for one request row. */
/** 为一条请求行解析语义标题、预览正文及可选的注入元数据。 */
export function presentRequestNode(node: RequestContextNode): {
  title: TranslationKey;
  kind: "injection" | "user" | "assistant" | "tool" | "system" | "context";
  body: string | null;
  injection?: ContextInjection;
} {
  // Rust decodes bounded text independently of the bounded raw JSON representation.
  // Rust 对有界正文独立解码，使其不受有界原始 JSON 表示的截断影响。
  const text = node.text ?? null;
  const user = node.role === "user" || node.kind === "user_input";
  const injection = user && text ? parseInjection(text, node.truncated) : undefined;
  if (injection) return { title: injection.title, kind: "injection", body: injection.body, injection };
  if (node.toolParts?.length && !text?.trim()) {
    return {
      title: node.toolParts.every((part) => part.result) ? "conversation.block.toolResult" : "conversation.block.toolCall",
      kind: "tool",
      body: null,
    };
  }
  if (user) return { title: "conversation.export.user", kind: "user", body: text };
  if (node.kind === "reasoning" && node.reasoning) return { title: "conversation.block.thinking", kind: "assistant", body: null };
  if (node.role === "assistant" || node.role === "model" || node.kind === "assistant_text") {
    return { title: "requestContext.assistantResponse", kind: "assistant", body: text };
  }
  if (node.role === "tool" || node.kind === "tool_result" || node.kind === "function_call_output" || node.kind === "function_call") {
    return { title: "skills.action.tools", kind: "tool", body: text };
  }
  if (node.role === "system" || node.role === "developer" || node.kind === "system_text") {
    return { title: "settings.modelExperience.systemPrompt", kind: "system", body: text };
  }
  return { title: "requestContext.context", kind: "context", body: text };
}

/** Removes registered instruction wrappers in the combined Markdown preview, preserving text order. */
/** 在合并的 Markdown 预览中移除已登记的指令信封，并保留正文顺序。 */
export function readableInitialPrompt(text: string): string {
  // Wrapper boundaries are emitted on their own lines by the corresponding producers.
  // 各生产者在独立行输出信封边界。
  return text.replace(/^<(vulcan_context_agent_md|vmm_profile|long_term_memory|skills_instructions)>\r?\n([\s\S]*?)\r?\n<\/\1>[ \t]*\r?$/gm,
    (_, tag: string, body: string) => tag === "vulcan_context_agent_md" ? decodeEnvelopeText(body) : body,
  );
}
