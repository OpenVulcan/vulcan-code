import * as React from "react";
import { FileText, LogIn, LogOut } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { MarkdownMessage } from "./conversation-markdown";
import { v1ModelResult } from "./tool-presentations/v1-tool-domain-presentation";
import { ConversationModelParameters } from "./conversation-model-parameters";

// Control tools and file listings retain their dedicated body without payload tabs.
// 控制工具与文件列表保留专属正文，不展示载荷标签页。
const DIRECT_TOOL_IDS = new Set(["todolist", "get_goal", "create_goal", "update_goal", "submit_plan", "ask",
  "shell_command", "terminal_session", "runlua", "runlua_session", "spawn_agent", "send_message", "wait_agent", "wait_agents_result", "list_agents",
  "agent_orchestration_info", "subagent_feedback", "close_agent", "interrupt_agent", "debug_session_open",
  "debug_session_request", "debug_session_events", "debug_session_status", "debug_session_close", "file_edit", "apply_patch", "vulcan_file_list", "get_model_extension_parameters"]);

// Result names describe the returned form, not the invocation action.
// 结果名称描述返回形态，而非调用动作。
const RESULT_LABELS: Readonly<Record<string, TranslationKey>> = {
  vulcan_codekit_ast_tree: "toolDomain.tabs.astTree", vulcan_codekit_ast_detail: "toolDomain.tabs.astDetail", vulcan_codekit_rg: "toolDomain.search.results",
  vulcan_codekit_node_source: "toolDomain.tabs.nodeSource", vulcan_codekit_markdown_menu: "toolDomain.tabs.documentMenu",
  knowledge_search: "toolDomain.tabs.retrieval", knowledge_list: "toolDomain.knowledge.directory", knowledge_read: "toolDomain.tabs.knowledgeContent", knowledge_write: "toolDomain.knowledge.change",
  vmm_memory_search: "toolDomain.tabs.memoryResults", vmm_turn_details: "toolDomain.memory.turnDetails", runtime_config: "toolDomain.tabs.configDetails",
  codebase_retrieval: "toolDomain.tabs.codeRetrieval", context7_resolve_library_id: "toolDomain.tabs.library", context7_query_docs: "toolDomain.tabs.docContent",
  web_search: "toolDomain.search.results", web_extract: "toolDomain.extract.content",
  image_understand: "toolDomain.media.understanding", video_understand: "toolDomain.media.understanding", audio_understand: "toolDomain.media.understanding",
  image_generate: "toolDomain.generated", image_edit: "toolDomain.generated", video_generate: "toolDomain.generated", video_edit: "toolDomain.generated",
  video_extend: "toolDomain.generated", speech_synthesize: "toolDomain.generated", speech_transcribe: "toolDomain.tabs.transcription", music_generate: "toolDomain.generated", music_cover: "toolDomain.generated",
};

/** Classifies one canonical invocation using exact declared tool and file-action contracts.
 * 根据精确声明的工具与文件动作契约分类单次规范调用。
 * Parameter event owns the discriminator and optional changeset; returns direct, skill-source or tabbed rendering.
 * 参数 event 持有判别字段与可选变更集；返回直接、技能源码或标签页展示模式。 */
export function toolResultMode(event: ToolCallEvent): "direct" | "skill" | "tabs" {
  if (event.changeset || DIRECT_TOOL_IDS.has(event.tool_name) || event.tool_name === "vulcan_file_read") return "direct";
  if (event.tool_name === "file" || event.tool_name === "vulcan_codekit_repo_map") return "direct";
  if (event.tool_name === "skill") return "skill";
  return "tabs";
}

/** Renders one exact source in the shared code block with language, wrapping and copy controls.
 * 在共享代码块中展示精确源码，包含语言、换行与复制控件。
 * Parameters language and content define the source; returns a safely fenced Markdown component.
 * 参数 language 与 content 定义源码；返回安全围栏包裹的 Markdown 组件。 */
export function ToolPayloadSourceBlock(props: { language: "markdown" | "json"; content: string }) {
  // markdown fences exceed embedded backticks so nested code cannot escape the source block.
  // markdown 围栏长于嵌入的反引号，防止嵌套代码逸出源码块。
  const markdown = React.useMemo(() => {
    // length avoids spreading an unbounded number of matches into function arguments.
    // length 避免把无界数量的匹配展开为函数实参。
    const length = Array.from(props.content.matchAll(/`+/g)).reduce((size, match) => Math.max(size, match[0].length + 1), 3);
    // fence is syntax only and is never copied with the actual source.
    // fence 仅为语法，不随实际源码一起复制。
    const fence = "`".repeat(length);
    return `${fence}${props.language}\n${props.content}\n${fence}`;
  }, [props.content, props.language]);
  return <MarkdownMessage content={markdown} eager />;
}

/** Keeps specialized results intact while exposing exact input/output as peer tabs where appropriate.
 * 保留专属结果展示，并仅在适合时提供精确输入输出同级标签。
 * Parameters event and children own the canonical payload and specialized result; label optionally names its form.
 * 参数 event 与 children 持有规范载荷和专属结果；label 可指定返回形态名称。
 * Returns one dedicated body or three mutually exclusive result/input/output panels.
 * 返回一个专属正文或三个互斥的结果、输入、输出面板。 */
export function ConversationToolResultTabs(props: { event: ToolCallEvent; children?: React.ReactNode; label?: string }) {
  // mode is shared by branch and direct renderers so control tools cannot gain accidental raw details.
  // mode 由分支与直接渲染器共享，防止控制工具意外增加原始详情。
  const mode = toolResultMode(props.event);
  const { t } = useI18n();
  // result is decoded solely through the existing canonical model-output boundary.
  // result 仅通过既有规范模型输出边界解码。
  const result = v1ModelResult(props.event.output);
  if (props.event.tool_name === "get_model_extension_parameters") return <ConversationModelParameters event={props.event} />;
  if (mode === "direct") return <>{props.children}</>;
  if (mode === "skill") return <div className="model-extension-payload-body">{result === undefined ? props.children
    : <ToolPayloadSourceBlock language={typeof result === "string" ? "markdown" : "json"} content={typeof result === "string" ? result : JSON.stringify(result, null, 2)} />}</div>;
  // label defaults only for unknown contracts; native file actions use their declared discriminator.
  // label 仅对未知契约使用通用名称；原生文件动作使用其声明判别字段。
  const label = props.label ?? t(props.event.tool_name === "file" ? (props.event.input.action === "stat" ? "toolDomain.tabs.fileInfo" : "toolDomain.search.results") : RESULT_LABELS[props.event.tool_name] ?? "toolDomain.tabs.returnedInfo");
  return <Tabs className="model-extension-tool-payloads" defaultValue="result" key={props.event.tool_call_id}>
    <TabsList variant="line" aria-label={t("toolDomain.tabs.label")}>
      <TabsTrigger value="result" data-payload-tab="result"><FileText size={14} />{label}</TabsTrigger>
      <TabsTrigger value="input" data-payload-tab="input"><LogIn size={14} />{t("toolPresentation.payload.input")}</TabsTrigger>
      <TabsTrigger value="output" data-payload-tab="output"><LogOut size={14} />{t("toolPresentation.payload.output")}</TabsTrigger>
    </TabsList>
    <TabsContent value="result" className="model-extension-payload-body">{props.children !== undefined ? props.children : result === undefined ? <div role="status">{t("toolDomain.tabs.noResult")}</div>
      : <ToolPayloadSourceBlock language={typeof result === "string" ? "markdown" : "json"} content={typeof result === "string" ? result : JSON.stringify(result, null, 2)} />}</TabsContent>
    <TabsContent value="input" className="model-extension-payload-body"><ToolPayloadSourceBlock language="json" content={JSON.stringify(props.event.input, null, 2)} /></TabsContent>
    <TabsContent value="output" className="model-extension-payload-body">{props.event.output === undefined ? <div role="status">{t("toolDomain.tabs.noOutput")}</div>
      : <ToolPayloadSourceBlock language="json" content={JSON.stringify(props.event.output, null, 2)} />}</TabsContent>
  </Tabs>;
}
