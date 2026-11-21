import { useMemo, useState } from "react";
import { ChevronRight, Terminal, Wrench, X } from "lucide-react";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import type { RequestContextAssistantResponse, RequestContextNode, RequestContextPromptOrigin, RequestContextRows, RequestContextToolPart } from "./request-context-dialog";
import { presentRequestNode, PROMPT_ORIGIN_KEYS, readableInitialPrompt } from "./request-context-presentation";
import { readRequestToolDeclarations, RequestToolSchemaPreview } from "./request-context-tool-schema";

// These are the system-level fields emitted by the supported wire encoders.
// 这些字段由支持的传输编码器用来承载系统级指令。
const SYSTEM_FIELDS = new Set(["instructions", "system", "systemInstruction", "system_instruction"]);
// These fields hold the chronologically ordered request messages.
// 这些字段承载有时序的请求消息。
const MESSAGE_FIELDS = new Set(["messages", "input", "contents", "questions", "baseline", "conversation"]);

/** Returns the original field path for the supplied node. */
/** 返回指定节点的原始字段路径。 */
function nodePath(node: RequestContextNode): string {
  return node.index === null ? node.field : `${node.field}[${node.index}]`;
}

/** Returns the exact tool names on a row without depending on its truncated original payload. */
/** 返回行上精确工具名，不依赖可能被截断的原始载荷。 */
function toolNames(node: RequestContextNode): string {
  return [...new Set(node.toolParts?.map((part) => part.name).filter(Boolean))].join(", ");
}

/** Returns whether the node belongs to the initial system and tools inspector. */
/** 判断节点是否属于初始系统提示词和工具检查区。 */
function isSystemNode(node: RequestContextNode): boolean {
  return SYSTEM_FIELDS.has(node.field) || node.role === "system" || node.role === "developer";
}

/** Shows a dense request-node list with one selected detail and a separate tool catalog. */
/** 展示紧凑的请求节点列表、一个所选详情及独立工具清单。 */
export function RequestContextInspector({ rows, response }: {
  rows: RequestContextRows;
  response: RequestContextAssistantResponse | null;
}) {
  const { t } = useI18n();
  // A new execution remounts this inspector so selection never crosses request boundaries.
  // 新执行重新挂载检查器，使选中状态不会跨越请求边界。
  const groups = useMemo(() => {
    // System updates that arrive after the first history message remain at their original position.
    // 在第一条历史消息之后到达的系统更新保留原位置。
    const firstMessage = rows.nodes.findIndex((node) => MESSAGE_FIELDS.has(node.field) && !isSystemNode(node));
    // Split only the initial prefix and the schema catalog from ordered messages.
    // 只从有序消息中提取初始前缀和 Schema 目录。
    const system = rows.nodes.filter((node, index) => (firstMessage === -1 || index < firstMessage) && isSystemNode(node));
    const tools = rows.nodes.filter((node) => node.field === "tools");
    const messages = rows.nodes.filter((node) => node.field !== "tools" && !system.includes(node));
    return { system, tools, messages };
  }, [rows.nodes]);
  // The initial system row is the natural first inspection target when the request has one.
  // 请求存在初始系统行时，它是默认检查对象。
  const [selected, setSelected] = useState<string | null>(() => groups.system.length || groups.tools.length || rows.toolsUnavailable ? "setup" : null);
  // Resolve the current selection against the displayed execution only.
  // 只从当前显示的执行中解析所选对象。
  const message = groups.messages.find((node) => nodePath(node) === selected);
  // The right pane resolves the same semantic type as the selected row.
  // 右侧详情从已选行解析相同的语义类型。
  const selectedMessage = message ? presentRequestNode(message) : undefined;
  // Resolve the same tool labels in the header that the row exposes in its summary.
  // 详情标题使用与列表摘要相同的工具名称。
  const selectedToolNames = message ? toolNames(message) : "";
  const setup = selected === "setup";
  const selectedResponse = selected === "response" ? response : null;
  // A continuation request may carry tools without repeating a system prompt.
  // 续接请求可能传入工具而无需重复提交系统提示词。
  const systemTitle = t(groups.system.length ? "requestContext.initialPrompt" : "skills.action.tools");
  return (
    <div className="vk-request-context-inspector" data-detail={Boolean(setup || message || selectedResponse)}>
      <div className="vk-request-context-list">
        {groups.system.length || groups.tools.length || rows.toolsUnavailable ? (
          <button className="vk-request-context-row" type="button" aria-pressed={setup} data-tone="system"
            onClick={() => setSelected("setup")}>
            <Terminal aria-hidden="true" />
            <strong>{systemTitle}</strong>
            {groups.tools.length > 0 ? <span className="vk-request-context-badge">{t("skills.action.tools")} {groups.tools.length}</span> : null}
            <ChevronRight aria-hidden="true" className="vk-request-context-row-arrow" />
          </button>
        ) : null}
        {groups.messages.map((node) => (
          <ContextRow key={nodePath(node)} node={node} selected={selected === nodePath(node)} onSelect={setSelected} />
        ))}
        {response ? (
          <button className="vk-request-context-row" type="button" aria-pressed={Boolean(selectedResponse)} data-tone="assistant"
            onClick={() => setSelected("response")}>
            <strong className="vk-request-context-badge">{t("requestContext.assistantResponse")}</strong>
            <span className="vk-request-context-preview">{response.content.slice(0, 256)}</span>
            <ChevronRight aria-hidden="true" className="vk-request-context-row-arrow" />
          </button>
        ) : null}
        {rows.nodeCount > rows.nodes.length ? <p className="vk-request-context-status">{t("requestContext.omitted")}</p> : null}
      </div>
      {setup || message || selectedResponse ? (
        <div className="vk-request-context-detail">
          <div className="vk-request-context-detail-header">
            <strong>{setup ? systemTitle : t(selectedMessage?.title ?? "requestContext.assistantResponse")}
              {selectedToolNames ? <code className="vk-request-context-tool-name">{selectedToolNames}</code> : null}
            </strong>
            <Button type="button" size="icon" variant="ghost" aria-label={t("conversation.detail.close")} onClick={() => setSelected(null)}>
              <X aria-hidden="true" />
            </Button>
          </div>
          {setup ? <SetupDetails system={groups.system} tools={groups.tools} origins={rows.promptOrigins ?? []}
            toolsUnavailable={rows.toolsUnavailable} /> : null}
          {message ? <NodeDetails key={nodePath(message)} node={message} /> : null}
          {selectedResponse ? <PreviewBody markdown={selectedResponse.content} raw={selectedResponse.content}
            truncated={selectedResponse.truncated} reasoning={selectedResponse.reasoning} /> : null}
        </div>
      ) : null}
    </div>
  );
}

/** Renders one compact selectable node and sends its original path to `onSelect`. */
/** 渲染一个紧凑的可选节点，并将原始路径交给 `onSelect`。 */
function ContextRow({ node, selected, onSelect }: {
  node: RequestContextNode;
  selected: boolean;
  onSelect: (path: string) => void;
}) {
  // Wire paths remain visible for matching the original provider payload.
  // 保持传输路径可见，便于对应供应商原始请求。
  const path = nodePath(node);
  const { t } = useI18n();
  // Derive the title and wrapper-free excerpt from the exact producer tag or role.
  // 从精确生产者标签或角色解析标题及无信封摘录。
  const presentation = useMemo(() => presentRequestNode(node), [node]);
  // Tool names are separate from the elided arguments and result preview.
  // 工具名称与省略后的参数及返回预览彼此独立。
  const names = toolNames(node);
  const preview = (presentation.body ?? node.toolParts?.map((part) => part.content).join(" · ") ?? node.reasoning ?? node.content)
    .slice(0, 256).replace(/\s+/g, " ");
  return (
    <button type="button" className="vk-request-context-row" aria-pressed={selected}
      data-tone={presentation.kind} onClick={() => onSelect(path)}>
      <span className="vk-request-context-badge">{t(presentation.kind === "injection" ? "requestContext.injection" : presentation.title)}</span>
      {presentation.kind === "injection" && presentation.title !== "requestContext.injection" ? <strong>{t(presentation.title)}</strong> : null}
      {names ? <code className="vk-request-context-tool-name">{names}</code> : null}
      <span className="vk-request-context-preview">{preview}</span>
      <ChevronRight aria-hidden="true" className="vk-request-context-row-arrow" />
    </button>
  );
}

/** Renders the selected node and makes its redacted raw payload available. */
/** 展示所选节点，并可查看其脱敏后的原始正文。 */
function NodeDetails({ node }: { node: RequestContextNode }) {
  const { t } = useI18n();
  // Tagged nodes show the host metadata separately from the Markdown body.
  // 带标签节点把宿主元数据与 Markdown 正文分开展示。
  const { envelope, kind, title, body } = presentRequestNode(node);
  return (
    <>
      {envelope ? (
        <dl className="vk-request-context-metadata">
          <div><dt>{t(kind === "injection" ? "requestContext.injection" : title)}</dt><dd><code>{envelope.tag}</code></dd></div>
          {envelope.attributes.map(([key, value]) => (
            <div key={key}><dt><code>{key}</code></dt><dd>{value}</dd></div>
          ))}
        </dl>
      ) : null}
      <PreviewBody markdown={body} raw={node.content} truncated={node.truncated} path={nodePath(node)}
        reasoning={node.reasoning} tools={node.toolParts} />
    </>
  );
}

/** Displays system instructions and tool definitions in independent tabs. */
/** 在独立标签页中展示系统提示词和工具定义。 */
function SetupDetails({ system, tools, origins, toolsUnavailable }: {
  system: RequestContextNode[];
  tools: RequestContextNode[];
  origins: RequestContextPromptOrigin[];
  /** Whether local tool resolution failed before producing a catalog. */
  /** 本地工具解析是否在生成目录前失败。 */
  toolsUnavailable?: boolean;
}) {
  const { t } = useI18n();
  const [showTools, setShowTools] = useState(system.length === 0);
  const [toolPath, setToolPath] = useState<string | null>(null);
  return (
    <>
      <div className="vk-request-context-tabs">
        {system.length ? <button type="button" aria-pressed={!showTools}
          onClick={() => setShowTools(false)}>{t("settings.modelExperience.systemPrompt")}</button> : null}
        {tools.length || toolsUnavailable ? <button type="button" aria-pressed={showTools}
          onClick={() => setShowTools(true)}>{t("skills.action.tools")}</button> : null}
      </div>
      {showTools ? (
        <div className="vk-request-context-detail-scroll">
          {toolsUnavailable ? <p className="vk-request-context-text">{t("requestContext.toolsUnavailable")}</p> : null}
          {tools.map((tool) => <ToolDefinition key={nodePath(tool)} node={tool} open={toolPath === nodePath(tool)} onToggle={setToolPath} />)}
        </div>
      ) : <InitialPromptDetails nodes={system} origins={origins} />}
    </>
  );
}

/** Renders a tool declaration and mounts its schema only when selected. */
/** 渲染工具声明，并仅在选中时挂载其 Schema。 */
function ToolDefinition({ node, open, onToggle }: {
  node: RequestContextNode;
  open: boolean;
  onToggle: (path: string | null) => void;
}) {
  // The row path remains stable for grouped provider declarations.
  // 对分组的供应商声明保留稳定的行路径。
  const path = nodePath(node);
  // Use the same decoded declaration in the summary and the structured preview.
  // 摘要与结构化预览共用同一份已解码声明。
  const declarations = useMemo(() => readRequestToolDeclarations(node.content), [node.content]);
  // Multiple declarations may share one provider node, while descriptions stay specific to a tool.
  // 多条声明可以共用一个供应商节点，说明仍属于各自的工具。
  const description = declarations.length === 1 ? declarations[0].description : undefined;
  return (
    <>
      <button type="button" className="vk-request-context-row" aria-expanded={open} data-tone="tool"
        onClick={() => onToggle(open ? null : path)}>
        <Wrench aria-hidden="true" />
        <code className="vk-request-context-catalog-tool-name">{declarations.length ? declarations.map((tool) => tool.name).join(", ") : path}</code>
        {description ? <span className="vk-request-context-preview">{description}</span> : null}
      </button>
      {open ? <ToolDefinitionDetails node={node} declarations={declarations} /> : null}
    </>
  );
}

/** Shows the tool declaration and lets the reader switch to its redacted raw source. */
/** 展示工具声明，并允许读者切换至脱敏后的原始内容。 */
function ToolDefinitionDetails({ node, declarations }: {
  /** The exact encoded node selected in the catalog. */
  /** 工具清单中选中的精确编码节点。 */
  node: RequestContextNode;
  /** Tool declarations decoded once by the owning row. */
  /** 由所属行一次解码得到的工具声明。 */
  declarations: ReturnType<typeof readRequestToolDeclarations>;
}) {
  const { t } = useI18n();
  // Collapsing the row resets its selected tab to the structured preview.
  // 收起该行后，选中标签恢复为结构化预览。
  const [showRaw, setShowRaw] = useState(false);
  return <>
    <div className="vk-request-context-tabs">
      <button type="button" aria-pressed={!showRaw} onClick={() => setShowRaw(false)}>{t("global.preview")}</button>
      <button type="button" aria-pressed={showRaw} onClick={() => setShowRaw(true)}>{t("requestContext.raw")}</button>
    </div>
    {showRaw ? <ContextText content={node.content} truncated={node.truncated} />
      : <RequestToolSchemaPreview declarations={declarations} truncated={node.truncated} />}
  </>;
}

/** Renders bounded text and marks truncation without interpreting markup. */
/** 渲染有界正文并标记截断，不解释正文中的标记。 */
function ContextText({ content, truncated }: RequestContextAssistantResponse) {
  const { t } = useI18n();
  return (
    <div className="vk-request-context-text">
      <pre>{content}</pre>
      {truncated ? <p>{t("requestContext.truncated")}</p> : null}
    </div>
  );
}

/** Renders the complete visible instruction prefix in order and exposes the supplied source labels. */
/** 按顺序渲染完整可见指令前缀，并展示服务提供的来源标签。 */
function InitialPromptDetails({ nodes, origins }: { nodes: RequestContextNode[]; origins: RequestContextPromptOrigin[] }) {
  const { t } = useI18n();
  // Provider encoding may merge instructions. Both views use the same ordered Markdown body.
  // 供应商编码可能合并指令，两个视图均使用相同的有序 Markdown 正文。
  const markdown = useMemo(() => readableInitialPrompt(nodes.map((node) => node.text ?? node.content).join("\n\n")), [nodes]);
  const raw = useMemo(() => nodes.map((node) => `${nodePath(node)}\n${node.content}`).join("\n\n"), [nodes]);
  return (
    <>
      {origins.length ? (
        <ol className="vk-request-context-origins" aria-label={t("requestContext.promptOrigins")}>
          {origins.map((origin, index) => (
            <li key={`${origin.kind}:${index}`}>
              <span>{Object.hasOwn(PROMPT_ORIGIN_KEYS, origin.kind) ? t(PROMPT_ORIGIN_KEYS[origin.kind]) : origin.kind}</span>
              {origin.tag ? <code>{origin.tag}</code> : null}
              {origin.source ? <small title={origin.source}>{origin.source}</small> : null}
            </li>
          ))}
        </ol>
      ) : null}
      <PreviewBody markdown={markdown} raw={raw} truncated={nodes.some((node) => node.truncated)} />
    </>
  );
}

/** Shows Markdown by default and keeps redacted raw content available in a separate tab. */
/** 默认展示 Markdown，并在独立标签页中保留脱敏后的原始内容。 */
function PreviewBody({ markdown, raw, truncated, path, reasoning, tools }: {
  markdown: string | null;
  raw: string;
  truncated: boolean;
  path?: string;
  /** Optional thinking text that precedes the main preview. */
  /** 位于主要预览之前的可选思考文本。 */
  reasoning?: string;
  /** Producer-defined tool blocks belonging to the same response node. */
  /** 属于同一响应节点的生产者定义工具块。 */
  tools?: RequestContextToolPart[];
}) {
  const { t } = useI18n();
  // The raw toggle is local to this exact selection.
  // 原文切换状态只属于此精确选择。
  const [showRaw, setShowRaw] = useState(false);
  // Tool rows can have a structured preview even without a message body.
  // 工具行即使没有消息正文，也可以提供结构化预览。
  const hasPreview = markdown !== null || Boolean(tools?.length) || Boolean(reasoning);
  return (
    <>
      {hasPreview ? (
        <div className="vk-request-context-tabs">
          <button type="button" aria-pressed={!showRaw} onClick={() => setShowRaw(false)}>{t("knowledge.reader.markdown")}</button>
          <button type="button" aria-pressed={showRaw} onClick={() => setShowRaw(true)}>{t("requestContext.raw")}</button>
        </div>
      ) : null}
      <div className="vk-request-context-detail-scroll">
        {reasoning ? (
          <details className="vk-request-context-reasoning">
            <summary>{t("conversation.block.thinking")}</summary>
            {showRaw ? <pre>{reasoning}</pre> : <MarkdownMessage content={reasoning} eager defaultCodeWrap />}
          </details>
        ) : null}
        {showRaw || !hasPreview ? <ContextText content={path ? `${path}\n${raw}` : raw} truncated={truncated} /> : (
          <div className="vk-request-context-text">
            {markdown ? <MarkdownMessage content={markdown} eager defaultCodeWrap /> : null}
            {tools?.map((part, index) => (
              <section key={index} className="vk-request-context-tool-part">
                <strong>{t(part.result ? "conversation.block.toolResult" : "conversation.block.toolCall")}</strong>
                {part.name ? <code className="vk-request-context-tool-name">{part.name}</code> : null}
                {part.content ? part.result
                  ? <MarkdownMessage content={part.content} eager defaultCodeWrap />
                  : <pre>{part.content}</pre> : null}
              </section>
            ))}
            {truncated ? <p>{t("requestContext.truncated")}</p> : null}
          </div>
        )}
      </div>
    </>
  );
}
