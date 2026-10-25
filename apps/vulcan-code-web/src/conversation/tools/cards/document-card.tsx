// Document card for tools whose result is a document to read: skills, skill help, knowledge, library docs, code
// navigation, debug sessions, environment variables and session MCP connections. The head names what was looked up;
// the body is the returned Markdown.
// 结果为可读文档的工具卡：技能、技能帮助、知识库、库文档、代码导航、调试会话、环境变量与会话 MCP 连接。
// 标题说明查找了什么；正文为返回的 Markdown。
import * as React from "react";
import {
  AppWindow, Brain, BookOpen, BookText, Bug, Cable, Compass, FileCode2, FileSearch, FileText, FolderTree, KeyRound, Library,
  ListTree, Map as MapIcon, Plug, Puzzle, ScrollText, Settings2, TextSearch, type LucideIcon,
} from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { ToolFrame, type ToolCardProps, type ToolFamily } from "../tool-card";
import { listOf, numberOf, textOf, type ToolCall } from "../tool-call";
import { primaryArgument } from "./generic-card";
import { ErrorNote, MarkdownResult } from "../views/result-views";

// DocumentToolSpec fixes how one document tool is named and what its target is.
// DocumentToolSpec 固定一个文档工具的命名方式与目标。
interface DocumentToolSpec {
  // Purpose family.
  // 用途家族。
  family: ToolFamily;
  // Head icon.
  // 标题图标。
  icon: LucideIcon;
  // Action verb.
  // 动作词。
  verb: TranslationKey;
  // Reads the target from the exact arguments.
  // 从精确参数读取目标。
  target: (input: Record<string, unknown>) => string;
}

// field reads one string argument.
// field 读取一个字符串参数。
const field = (key: string) => (input: Record<string, unknown>) => textOf(input[key])?.trim() ?? "";

// firstOf reads the first present string argument among keys.
// firstOf 读取 keys 中第一个存在的字符串参数。
const firstOf = (...keys: string[]) => (input: Record<string, unknown>) => {
  for (const key of keys) {
    const value = textOf(input[key])?.trim();
    if (value) return value;
  }
  return "";
};

// queries joins the query list of a search call.
// queries 连接搜索调用的查询列表。
const queries = (input: Record<string, unknown>) => listOf(input.queries).filter((query) => typeof query === "string").join(" · ");

// joined joins the primitive items of one list argument.
// joined 连接一个列表参数中的原始值条目。
const joined = (key: string) => (input: Record<string, unknown>) =>
  listOf(input[key]).filter((item) => typeof item === "string" || typeof item === "number").join(" · ");

// firstAbstract names the first memory of a write and how many follow it.
// firstAbstract 标明写入的第一条记忆及其后的条数。
const firstAbstract = (input: Record<string, unknown>) => {
  const items = listOf(input.items);
  const first = textOf((items[0] as Record<string, unknown> | undefined)?.abstract)?.trim() ?? "";
  return items.length > 1 ? `${first} +${items.length - 1}` : first;
};

// lineRange appends an explicit line window to a document selector.
// lineRange 为文档选择器追加显式行窗口。
const lineRange = (input: Record<string, unknown>) => {
  const start = numberOf(input.startLine);
  const end = numberOf(input.endLine);
  return start !== null || end !== null ? ` L${start ?? 1}–${end ?? ""}` : "";
};

// variableNames names the variables one environment call sets or requests.
// variableNames 标明一次环境调用设置或索取的变量。
const variableNames = (input: Record<string, unknown>) => {
  const requested = listOf(input.variables)
    .map((item) => textOf((item as Record<string, unknown> | null)?.name)?.trim() ?? "")
    .filter(Boolean);
  return [field("action")(input), field("name")(input), ...requested].filter(Boolean).join(" · ");
};

// DOCUMENT_TOOLS is keyed by canonical tool name; only these tools use the document card.
// DOCUMENT_TOOLS 以规范工具名为键；只有这些工具使用文档卡。
const DOCUMENT_TOOLS: Readonly<Record<string, DocumentToolSpec>> = {
  skill: { family: "read", icon: Puzzle, target: field("name"), verb: "toolCard.doc.skill" },
  call_luaskill: { family: "run", icon: Puzzle, target: field("name"), verb: "toolCard.doc.callSkill" },
  luaskill_help: {
    family: "read",
    icon: BookOpen,
    target: (input) => field("name")(input) || [field("skill")(input), field("topic")(input)].filter(Boolean).join(" / "),
    verb: "toolCard.doc.skillHelp",
  },
  knowledge_search: { family: "search", icon: TextSearch, target: queries, verb: "toolCard.doc.knowledgeSearch" },
  knowledge_list: { family: "read", icon: Library, target: firstOf("path", "libraryId"), verb: "toolCard.doc.knowledgeList" },
  knowledge_read: {
    family: "read",
    icon: BookText,
    target: (input) => `${firstOf("path", "documentId")(input)}${lineRange(input)}`,
    verb: "toolCard.doc.knowledgeRead",
  },
  context7_resolve_library_id: { family: "web", icon: Library, target: field("libraryName"), verb: "toolCard.doc.libraryResolve" },
  context7_query_docs: {
    family: "web",
    icon: BookOpen,
    target: (input) => [field("libraryId")(input), field("query")(input)].filter(Boolean).join(" · "),
    verb: "toolCard.doc.libraryQuery",
  },
  debug_session_open: {
    family: "run",
    icon: Bug,
    target: (input) => {
      const target = typeof input.target === "object" && input.target !== null ? input.target as Record<string, unknown> : {};
      const address = textOf(target.websocket_url) ?? [textOf(target.host), numberOf(target.port)].filter((part) => part !== null).join(":");
      return [field("protocol")(input), address].filter(Boolean).join(" · ");
    },
    verb: "toolCard.doc.debugOpen",
  },
  debug_session_request: { family: "run", icon: Bug, target: firstOf("method", "command", "session_id"), verb: "toolCard.doc.debugRequest" },
  debug_session_events: { family: "run", icon: Bug, target: field("session_id"), verb: "toolCard.doc.debugEvents" },
  debug_session_status: { family: "run", icon: Bug, target: field("session_id"), verb: "toolCard.doc.debugStatus" },
  debug_session_close: { family: "run", icon: Bug, target: field("session_id"), verb: "toolCard.doc.debugClose" },
  vulcan_codekit_repo_map: { family: "read", icon: MapIcon, target: field("path"), verb: "toolCard.doc.repoMap" },
  vulcan_codekit_ast_tree: { family: "read", icon: ListTree, target: field("path"), verb: "toolCard.doc.astTree" },
  vulcan_codekit_ast_detail: { family: "read", icon: FileCode2, target: field("path"), verb: "toolCard.doc.astDetail" },
  vulcan_codekit_node_source: { family: "read", icon: FileCode2, target: field("path"), verb: "toolCard.doc.nodeSource" },
  vulcan_codekit_rg: { family: "search", icon: FileSearch, target: firstOf("pattern", "query", "path"), verb: "toolCard.doc.codeSearch" },
  vulcan_codekit_markdown_menu: {
    family: "read",
    icon: ScrollText,
    target: (input) => (textOf(input.path) ?? "").split(/\r?\n/).map((path) => path.trim()).filter(Boolean).join(" · "),
    verb: "toolCard.doc.markdownMenu",
  },
  vmm_turn_details: { family: "read", icon: Brain, target: joined("turnIds"), verb: "toolCard.doc.memoryTurns" },
  vmm_memory_write: { family: "edit", icon: Brain, target: firstAbstract, verb: "toolCard.doc.memoryWrite" },
  vmm_memory_delete: { family: "edit", icon: Brain, target: joined("memoryIds"), verb: "toolCard.doc.memoryDelete" },
  runtime_config: {
    family: "read",
    icon: Settings2,
    target: (input) => [field("action")(input), field("skill_id")(input)].filter(Boolean).join(" · "),
    verb: "toolCard.doc.runtimeConfig",
  },
  browser_list: { family: "web", icon: AppWindow, target: () => "", verb: "toolCard.doc.browserList" },
  browser_open: { family: "web", icon: AppWindow, target: () => "", verb: "toolCard.doc.browserOpen" },
  browser_select: { family: "web", icon: AppWindow, target: field("browserId"), verb: "toolCard.doc.browserSelect" },
  browser_close: { family: "web", icon: AppWindow, target: field("browserId"), verb: "toolCard.doc.browserClose" },
  declare_intent: { family: "plan", icon: Compass, target: field("intent"), verb: "toolCard.doc.declareIntent" },
  env: { family: "run", icon: KeyRound, target: variableNames, verb: "toolCard.doc.environment" },
  mcp_connect: {
    family: "run",
    icon: Plug,
    target: (input) => [field("action")(input), firstOf("name", "connection_id")(input)].filter(Boolean).join(" · "),
    verb: "toolCard.doc.mcpConnect",
  },
  mcp_tools: {
    family: "read",
    icon: Cable,
    target: (input) => [field("connection_id")(input), field("tool")(input)].filter(Boolean).join(" · "),
    verb: "toolCard.doc.mcpTools",
  },
  mcp_call: {
    family: "run",
    icon: Cable,
    target: (input) => [field("connection_id")(input), field("tool")(input)].filter(Boolean).join(" · "),
    verb: "toolCard.doc.mcpCall",
  },
  vulcan_file_list: { family: "read", icon: FolderTree, target: firstOf("path", "PWD"), verb: "toolCard.file.list" },
  vulcan_file_read: { family: "read", icon: FileText, target: field("path"), verb: "toolCard.file.read" },
};

// documentToolSpec returns the spec of one document tool, or null for any other tool.
// documentToolSpec 返回文档工具的规格；其他工具返回 null。
export function documentToolSpec(call: ToolCall): DocumentToolSpec | null {
  return Object.hasOwn(DOCUMENT_TOOLS, call.name) ? DOCUMENT_TOOLS[call.name] : null;
}

// SKILL_CONTENT unwraps the <skill_content> envelope a loaded skill arrives in.
// SKILL_CONTENT 解开已加载技能所在的 <skill_content> 封装。
const SKILL_CONTENT = /^\s*<skill_content\b[^>]*>\n?([\s\S]*?)\n?<\/skill_content>\s*$/;

// documentBody returns the readable Markdown of one result.
// documentBody 返回一条结果的可读 Markdown。
export function documentBody(output: string): string {
  return SKILL_CONTENT.exec(output)?.[1] ?? output;
}

/** Renders one document tool call.
 * 渲染一次文档工具调用。 */
export function DocumentCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const spec = documentToolSpec(call);
  const target = (spec?.target(call.input) || primaryArgument(call)).replace(/\s+/g, " ");
  const body = React.useMemo(() => (call.modelOutput ? documentBody(call.modelOutput) : ""), [call.modelOutput]);
  return (
    <ToolFrame
      {...props}
      icon={spec?.icon ?? BookOpen}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={spec ? t(spec.verb) : call.event.tool_name}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} /> : body.trim() ? <MarkdownResult text={body} /> : null}
    </ToolFrame>
  );
}
