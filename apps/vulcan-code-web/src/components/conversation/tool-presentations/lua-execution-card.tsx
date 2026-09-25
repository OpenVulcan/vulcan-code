// Lua execution tool card for one-shot runs and Lua sessions.
// Lua 执行工具卡，覆盖一次性运行与 Lua 会话。
import * as React from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { hydrateLuaExecutionPresentation, type LuaExecutionPresentation } from "@/components/conversation/conversation-tool-presentations";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import { formatDateTime } from "@/lib/display-format";
import { ToolResultBody, type ToolRequestCardProgress, type ToolDetailLoader, activeToolStatusLabel } from "./tool-card-common";

// luaExecutionPresentationDetailCache avoids refetching full RunLua results after ordinary remounts.
// luaExecutionPresentationDetailCache 避免普通重新挂载后重复请求完整 RunLua 结果。
const luaExecutionPresentationDetailCache = new Map<string, LuaExecutionPresentation>();

// LuaExecutionPresentationCard renders one-shot and persistent RunLua actions with Lua-specific detail.
// LuaExecutionPresentationCard 使用 Lua 专用详情渲染一次性与持久 RunLua 动作。
export function LuaExecutionPresentationCard(props: {
  // Lua execution presentation model.
  // Lua 执行展示模型。
  presentation: LuaExecutionPresentation;
  // Whether this Lua execution card is expanded.
  // 当前 Lua 执行卡片是否展开。
  expanded: boolean;
  // Optional raw-detail action rendered inside expanded content.
  // 在展开内容内部渲染的可选原始详情操作。
  expandedFooter?: React.ReactNode;
  // Optional request stream state rendered inside this original card.
  // 在当前原始卡片内部渲染的可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for the complete Lua result.
  // 加载单条隐藏工具详情记录用于完整 Lua 结果。
  onLoadToolDetail?: ToolDetailLoader;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t, locale } = useI18n();
  const {
    expanded,
    onExpandedChange,
    onLoadToolDetail,
    presentation,
  } = props;
  const detailCacheKey = luaExecutionPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<LuaExecutionPresentation>(
    () => detailCacheKey ? luaExecutionPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  React.useEffect(() => {
    const cached = detailCacheKey ? luaExecutionPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);
  React.useEffect(() => {
    if (!expanded || !onLoadToolDetail || !presentation.detailRef || !detailCacheKey || luaExecutionPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
    void onLoadToolDetail(presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateLuaExecutionPresentation(presentation, payload);
        luaExecutionPresentationDetailCache.set(detailCacheKey, nextPresentation);
        setHydratedPresentation(nextPresentation);
      })
      .catch(() => {
        if (!cancelled) {
          setHydratedPresentation(presentation);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [detailCacheKey, expanded, onLoadToolDetail, presentation]);
  // title is the complete action title; only the Lua identity icon is prepended.
  // title 是完整动作标题；前方仅保留 Lua 身份图标。
  const title = luaExecutionActionLabel(hydratedPresentation, t);
  // status keeps request formation and actual execution visible without terminal prefixes.
  // status 在不添加终态前缀的前提下展示请求构造与真实执行状态。
  const status = activeToolStatusLabel(
    props.requestProgress,
    hydratedPresentation.state,
    t("toolPresentation.execution.running"),
  );
  // argumentsMarkdown renders structured args as a two-column Markdown table.
  // argumentsMarkdown 将结构化参数渲染为两列表格 Markdown。
  const argumentsMarkdown = luaExecutionArgumentsMarkdown(hydratedPresentation.arguments, t);
  // outputMarkdown strips model-facing envelopes and keeps only user-relevant results.
  // outputMarkdown 移除面向模型的外层结构，只保留与用户相关的结果。
  const outputMarkdown = luaExecutionOutputMarkdown(hydratedPresentation, t, locale);
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, onExpandedChange, presentation.id]);
  return (
    <div className={cn("tool-presentation-card lua-execution", expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="tool-presentation-summary"
        onClick={handleToggle}
        title={title}
        type="button"
      >
        <LuaToolIcon className="lua-execution-icon" />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">{title}</span>
        </span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`lua-execution:${presentation.id}`} expanded={expanded}>
        <ToolResultBody>
        <div className="tool-presentation-detail lua-execution-detail">
          {hydratedPresentation.code ? (
            <div className="lua-execution-section">
              <div className="lua-execution-section-title">{t("toolPresentation.lua.code")}</div>
              <MarkdownMessage content={markdownFencedCode("lua", hydratedPresentation.code)} eager />
            </div>
          ) : null}
          {argumentsMarkdown ? (
            <div className="lua-execution-section">
              <div className="lua-execution-section-title">{t("toolPresentation.lua.arguments")}</div>
              <MarkdownMessage content={argumentsMarkdown} eager />
            </div>
          ) : null}
          <div className="lua-execution-section lua-execution-output">
            <div className="lua-execution-section-title">{t("toolPresentation.lua.output")}</div>
            {outputMarkdown ? (
              <MarkdownMessage content={outputMarkdown} eager />
            ) : (
            <div className="markdown-tool-empty">{
              hydratedPresentation.state === "running"
                ? t("toolPresentation.execution.running")
                : t("toolPresentation.output.empty")
            }</div>
            )}
          </div>
          {props.expandedFooter}
        </div>
        </ToolResultBody>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// LuaToolIcon renders the compact Lua orbit mark without adding a status badge.
// LuaToolIcon 渲染紧凑的 Lua 轨道标志，不附加状态徽标。
function LuaToolIcon(props: {
  // Optional CSS class used by the conversation-card layout.
  // 会话卡片布局使用的可选 CSS 类名。
  className?: string;
}) {
  return (
    <svg
      aria-hidden="true"
      className={props.className}
      fill="none"
      focusable="false"
      viewBox="0 0 32 32"
    >
      <path
        d="M30 6a3.86 3.86 0 0 1-1.167 2.833 4.024 4.024 0 0 1-5.666 0A3.86 3.86 0 0 1 22 6a3.86 3.86 0 0 1 1.167-2.833 4.024 4.024 0 0 1 5.666 0A3.86 3.86 0 0 1 30 6m-9.208 5.208A10.6 10.6 0 0 0 13 8a10.6 10.6 0 0 0-7.792 3.208A10.6 10.6 0 0 0 2 19a10.6 10.6 0 0 0 3.208 7.792A10.6 10.6 0 0 0 13 30a10.6 10.6 0 0 0 7.792-3.208A10.6 10.6 0 0 0 24 19a10.6 10.6 0 0 0-3.208-7.792m-1.959 7.625a4.024 4.024 0 0 1-5.666 0 4.024 4.024 0 0 1 0-5.666 4.024 4.024 0 0 1 5.666 0 4.024 4.024 0 0 1 0 5.666"
        fill="currentColor"
      />
    </svg>
  );
}

// luaExecutionPresentationDetailCacheKey returns a stable cache key for one hidden Lua result.
// luaExecutionPresentationDetailCacheKey 返回单条隐藏 Lua 结果的稳定缓存键。
function luaExecutionPresentationDetailCacheKey(presentation: LuaExecutionPresentation): string | null {
  if (!presentation.detailRef) {
    return null;
  }
  return `${presentation.detailRef.recordId ?? ""}:${presentation.detailRef.seq ?? ""}:${presentation.detailRef.messageIndex ?? ""}`;
}

// luaExecutionActionLabel returns the complete localized public RunLua action title.
// luaExecutionActionLabel 返回完整的本地化公开 RunLua 动作标题。
function luaExecutionActionLabel(
  presentation: LuaExecutionPresentation,
  t: (key: TranslationKey) => string,
): string {
  let actionLabel: string;
  if (presentation.action === "invalid") actionLabel = t("toolPresentation.lua.action.invalid");
  else if (presentation.action === "list") actionLabel = t("toolPresentation.lua.action.list");
  else if (presentation.action === "start") actionLabel = t("toolPresentation.lua.action.start");
  else if (presentation.action === "status") actionLabel = t("toolPresentation.lua.action.status");
  else if (presentation.action === "close") actionLabel = t("toolPresentation.lua.action.close");
  else actionLabel = t("toolPresentation.lua.action.eval");
  // Only persistent actions have a verified session name; one-shot task labels are not session identities.
  // 只有持久动作具备已验证的会话名称；一次性任务标签不是会话身份。
  return presentation.mode === "persistent" && presentation.label
    ? `${actionLabel} · ${presentation.label}`
    : actionLabel;
}

// LuaMarkdownCodeBlock preserves one exact fenced block extracted from LuaSkills output.
// LuaMarkdownCodeBlock 保留从 LuaSkills 输出中提取的一段精确围栏代码块。
interface LuaMarkdownCodeBlock {
  // Declared Markdown fence language.
  // Markdown 围栏声明的语言。
  language: string;
  // Exact fenced content without delimiters.
  // 不含分隔符的精确围栏内容。
  content: string;
}

// luaExecutionArgumentsMarkdown renders exact structured arguments as a compact Markdown table.
// luaExecutionArgumentsMarkdown 将精确结构化参数渲染为紧凑 Markdown 表格。
function luaExecutionArgumentsMarkdown(
  argumentsValue: Record<string, unknown> | null,
  t: (key: TranslationKey) => string,
): string {
  if (!argumentsValue || Object.keys(argumentsValue).length === 0) {
    return "";
  }
  const rows = Object.entries(argumentsValue).map(([name, value]) => (
    `| ${markdownTableCell(name)} | ${markdownTableCell(JSON.stringify(value))} |`
  ));
  return [
    `| ${t("toolPresentation.lua.argumentName")} | ${t("toolPresentation.lua.argumentValue")} |`,
    "| --- | --- |",
    ...rows,
  ].join("\n");
}

// luaExecutionOutputMarkdown converts model-facing RunLua envelopes into user-facing detail only.
// luaExecutionOutputMarkdown 将面向模型的 RunLua 外层结构转换为仅供用户查看的详情。
function luaExecutionOutputMarkdown(
  presentation: LuaExecutionPresentation,
  t: (key: TranslationKey) => string,
  locale: string,
): string {
  const output = presentation.output.trim();
  if (!output) {
    return "";
  }
  return presentation.toolId === "runlua"
    ? luaOneShotOutputMarkdown(output)
    : luaSessionOutputMarkdown(presentation, output, t, locale);
}

// luaOneShotOutputMarkdown extracts only returned values or the exact error detail from LuaSkills Markdown.
// luaOneShotOutputMarkdown 仅从 LuaSkills Markdown 中提取返回值或精确错误详情。
function luaOneShotOutputMarkdown(output: string): string {
  if (output.startsWith("# Runtime Execution Error")) {
    const errorSection = luaMarkdownSection(output, "Error");
    const errorDetail = errorSection ? luaMarkdownCodeBlocks(errorSection)[0]?.content : null;
    return markdownFencedCode("text", errorDetail ?? output);
  }
  if (!output.startsWith("# Runtime Execution Result")) {
    return markdownFencedCode("text", luaToolFailureDetail(output));
  }
  const returnedSection = luaMarkdownSection(output, "Returned Values") ?? "";
  const returnedValues = luaMarkdownCodeBlocks(returnedSection).map((block) => {
    if (block.language !== "json") {
      return block.content;
    }
    try {
      return JSON.parse(block.content) as unknown;
    } catch {
      return block.content;
    }
  });
  const printedSection = luaMarkdownSection(output, "Printed Output");
  const printedOutput = printedSection ? luaMarkdownCodeBlocks(printedSection)[0]?.content : null;
  const returnedValue = returnedValues.length === 1 ? returnedValues[0] : returnedValues;
  const displayValue = printedOutput
    ? { printed_output: printedOutput, returned_values: returnedValue }
    : returnedValue;
  return markdownFencedCode("json", luaJsonDisplayText(displayValue));
}

// luaSessionOutputMarkdown renders each persistent lifecycle action from its exact JSON response contract.
// luaSessionOutputMarkdown 根据精确 JSON 响应契约渲染各个持久会话生命周期动作。
function luaSessionOutputMarkdown(
  presentation: LuaExecutionPresentation,
  output: string,
  t: (key: TranslationKey) => string,
  locale: string,
): string {
  const payload = luaJsonObject(output);
  if (!payload) {
    return markdownFencedCode("text", luaToolFailureDetail(output));
  }
  if (payload.ok !== true) {
    return markdownFencedCode("text", luaSessionErrorDetail(payload));
  }
  if (presentation.action === "eval") {
    if (!Object.hasOwn(payload, "result")) {
      return markdownFencedCode("text", "runlua_session eval response is missing `result`.");
    }
    return markdownFencedCode("json", luaJsonDisplayText(payload.result));
  }
  if (presentation.action === "list") {
    return luaSessionListMarkdown(payload, t, locale);
  }
  if (presentation.action === "close") {
    const sessionId = typeof payload.lease_id === "string" ? payload.lease_id : "-";
    return t("toolPresentation.lua.closeSucceeded").replace("{session}", sessionId);
  }
  const displayValue = presentation.action === "start"
    ? {
        name: presentation.label,
        session_id: payload.lease_id,
        generation: payload.generation,
        ttl_sec: payload.ttl_sec,
        expires_at_unix_ms: payload.expires_at_unix_ms,
      }
    : {
        session_id: payload.lease_id,
        active: payload.ok,
        ttl_sec: payload.ttl_sec,
        expires_at_unix_ms: payload.expires_at_unix_ms,
        closed: payload.closed,
        expired: payload.expired,
      };
  return markdownFencedCode("json", luaJsonDisplayText(displayValue));
}

// luaSessionListMarkdown renders only the user-relevant session identity and lifecycle fields.
// luaSessionListMarkdown 仅渲染与用户相关的会话身份和生命周期字段。
function luaSessionListMarkdown(
  payload: Record<string, unknown>,
  t: (key: TranslationKey) => string,
  locale: string,
): string {
  if (!Array.isArray(payload.sessions)) {
    return markdownFencedCode("text", "runlua_session list response is missing `sessions`.");
  }
  if (payload.sessions.length === 0) {
    return t("toolPresentation.lua.noSessions");
  }
  if (!payload.sessions.every((value) => value && typeof value === "object" && !Array.isArray(value))) {
    return markdownFencedCode("text", "runlua_session list response contains a non-object session.");
  }
  const rows = payload.sessions.map((sessionValue) => {
    const session = sessionValue as Record<string, unknown>;
    const ttl = typeof session.ttl_sec === "number" ? `${session.ttl_sec}s` : "-";
    const expiresAt = typeof session.expires_at_unix_ms === "number"
      ? formatDateTime(session.expires_at_unix_ms, locale) ?? "-"
      : "-";
    return `| ${markdownTableCell(session.name)} | ${markdownTableCell(session.session_id)} | ${markdownTableCell(luaSessionStateLabel(session, t))} | ${ttl} | ${markdownTableCell(expiresAt)} |`;
  });
  return [
    `| ${t("toolPresentation.lua.sessionName")} | ${t("toolPresentation.lua.sessionId")} | ${t("toolPresentation.lua.sessionState")} | ${t("toolPresentation.lua.sessionTtl")} | ${t("toolPresentation.lua.sessionExpiresAt")} |`,
    "| --- | --- | --- | --- | --- |",
    ...rows,
  ].join("\n");
}

// luaSessionStateLabel maps exact lease flags to one localized display state.
// luaSessionStateLabel 将精确租约标志映射为一个本地化展示状态。
function luaSessionStateLabel(
  session: Record<string, unknown>,
  t: (key: TranslationKey) => string,
): string {
  if (session.ok === true) return t("toolPresentation.lua.sessionActive");
  if (session.expired === true) return t("toolPresentation.lua.sessionExpired");
  if (session.closed === true) return t("toolPresentation.lua.sessionClosed");
  return t("toolPresentation.lua.sessionFailed");
}

// luaSessionErrorDetail returns the exact runtime error code and message without lifecycle metadata.
// luaSessionErrorDetail 返回精确运行时错误代码与消息，不包含生命周期元数据。
function luaSessionErrorDetail(payload: Record<string, unknown>): string {
  const errorCode = typeof payload.error_code === "string" ? payload.error_code : null;
  const message = typeof payload.message === "string" ? payload.message : null;
  if (errorCode && message) return `${errorCode}: ${message}`;
  if (message) return message;
  if (errorCode) return errorCode;
  return JSON.stringify(payload, null, 2);
}

// luaToolFailureDetail removes the exact host failure envelope and keeps its Reason text only.
// luaToolFailureDetail 移除精确宿主失败外层结构，仅保留 Reason 错误详情。
function luaToolFailureDetail(output: string): string {
  const match = output.match(
    /^Tool `[^`]+` failed\.\r?\nReason: ([\s\S]*?)\r?\nAdjust the arguments, narrow the scope, or switch to a more appropriate tool before continuing\.$/,
  );
  return match?.[1].trim() ?? output;
}

// luaMarkdownSection extracts one exact H2 section from canonical LuaSkills output.
// luaMarkdownSection 从规范 LuaSkills 输出中提取一个精确二级标题段落。
function luaMarkdownSection(markdown: string, heading: string): string | null {
  const marker = `## ${heading}`;
  const markerIndex = markdown.indexOf(marker);
  if (markerIndex < 0) {
    return null;
  }
  const contentStart = markerIndex + marker.length;
  const remaining = markdown.slice(contentStart).replace(/^\r?\n/, "");
  const nextHeadingIndex = remaining.search(/\r?\n## /);
  return (nextHeadingIndex < 0 ? remaining : remaining.slice(0, nextHeadingIndex)).trim();
}

// luaMarkdownCodeBlocks extracts exact fenced blocks from one canonical LuaSkills section.
// luaMarkdownCodeBlocks 从一个规范 LuaSkills 段落中提取精确围栏代码块。
function luaMarkdownCodeBlocks(section: string): LuaMarkdownCodeBlock[] {
  const blocks: LuaMarkdownCodeBlock[] = [];
  const pattern = /```([^\r\n]*)\r?\n([\s\S]*?)\r?\n```/g;
  for (const match of section.matchAll(pattern)) {
    blocks.push({ language: match[1].trim(), content: match[2] });
  }
  return blocks;
}

// luaJsonObject decodes the exact persistent-session JSON result when it is an object.
// luaJsonObject 解码精确持久会话 JSON 结果，并确保其为对象。
function luaJsonObject(output: string): Record<string, unknown> | null {
  try {
    const value = JSON.parse(output) as unknown;
    return value && typeof value === "object" && !Array.isArray(value)
      ? value as Record<string, unknown>
      : null;
  } catch {
    return null;
  }
}

// luaJsonDisplayText encodes one successful result as stable indented JSON.
// luaJsonDisplayText 将一个成功结果编码为稳定缩进 JSON。
function luaJsonDisplayText(value: unknown): string {
  return JSON.stringify(value, null, 2) ?? "null";
}

// markdownTableCell escapes one compact value for a GitHub-flavored Markdown table cell.
// markdownTableCell 转义一个紧凑值以用于 GitHub 风格 Markdown 表格单元格。
function markdownTableCell(value: unknown): string {
  if (value === null || value === undefined) {
    return "-";
  }
  return String(value).replace(/\|/g, "\\|").replace(/\r?\n/g, "<br />");
}

// markdownFencedCode wraps exact content with a fence longer than any embedded backtick run.
// markdownFencedCode 使用长于内容内任意反引号序列的围栏包裹精确内容。
function markdownFencedCode(language: "json" | "lua" | "text", content: string): string {
  const longestBacktickRun = Math.max(0, ...(content.match(/`+/g) ?? []).map((run) => run.length));
  const fence = "`".repeat(Math.max(3, longestBacktickRun + 1));
  return `${fence}${language}\n${content}\n${fence}`;
}
