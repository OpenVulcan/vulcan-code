import * as React from "react";
import { ChevronDown } from "lucide-react";
import { TodoListResult } from "./conversation-todo-result";
import { Button } from "@/components/ui/button";
import { ConversationToolResultTabs, toolResultMode } from "./conversation-tool-result-tabs";
import { MarkdownMessage } from "./conversation-markdown";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import { domainObject, type V1ToolDomainPresentation, type V1ToolDomainSection } from "./tool-presentations/v1-tool-domain-presentation";
import type { ToolJsonValue } from "@/service/conversation-event-window-store";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import "@/styles/tool-domain.css";

// FIELD_LABEL_NAMES lists the exact protocol members with a translated label, without merging aliases or inferring ownership.
// FIELD_LABEL_NAMES 列出拥有翻译标签的精确协议成员，不合并别名或推测归属。
const FIELD_LABEL_NAMES = [
  "objective", "status", "step", "tokensUsed", "tokenBudget", "timeUsedSeconds", "skill_id", "skill_version",
  "complete", "missing_count", "invalid_count", "business_issue_count", "orphaned_count", "items", "key", "type",
  "title", "description", "required", "sensitive", "state", "constraints", "options", "validation_error",
  "missing", "invalid", "business_issues", "orphaned", "revision", "store_scope", "code", "message",
  "value", "path", "startLine", "endLine", "content", "source", "kind", "score",
  "query", "hits", "failures", "warnings", "libraryName", "relativePath", "headingPath", "document",
  "chunk", "targetType", "truncated", "libraryId", "library_id", "library_name", "documentId", "chunkId",
  "lineCount", "byteSize", "chunkCount", "sourceId", "folderPath", "query_index", "hit_count", "memory_id",
  "source_turn_id", "has_source_turn", "abstract", "details_preview", "category", "created_datetime", "retrieval", "turn_id",
  "user_question", "assistant_answer", "timeline", "detail", "previous_turn_ids", "next_turn_ids", "item_count", "deduped",
  "deleted_count", "not_found_count", "deleted_memory_ids", "not_found_memory_ids", "header", "question", "multiple", "answers",
  "answered", "label", "name", "model", "display_name", "characteristics", "input_modes", "resolution_policy",
  "duration_policy", "format_policy", "limitations", "args_schema", "properties", "default", "minimum", "maximum",
  "enum", "additionalProperties", "results", "url", "source_domain", "snippet", "rank", "error",
  "text", "resources", "searches", "extracts", "upstream_response_id", "session_id", "sessionId", "command_session_id",
  "protocol", "method", "command", "connected", "closed", "events", "result", "ok",
] as const;
const FIELD_LABEL_SET: ReadonlySet<string> = new Set(FIELD_LABEL_NAMES);

// STATUS_LABEL_NAMES keeps domain states distinct from the outer tool execution lifecycle.
// STATUS_LABEL_NAMES 将领域状态与外层工具执行生命周期区分。
const STATUS_LABEL_NAMES = [
  "pending", "in_progress", "completed", "active", "complete", "blocked", "unset", "missing",
  "default", "configured", "invalid", "failed",
] as const;
const STATUS_LABEL_SET: ReadonlySet<string> = new Set(STATUS_LABEL_NAMES);

/** Labels one protocol member, keeping unknown members verbatim.
 * 为一个协议成员提供标签，未知成员保持原样。
 * Parameters key and t select the member and active locale; returns its label or the key itself.
 * 参数 key 与 t 选择成员与当前语言；返回其标签或键本身。 */
function fieldLabel(key: string, t: (key: TranslationKey) => string): string {
  return FIELD_LABEL_SET.has(key) ? t(`toolDomain.field.${key as (typeof FIELD_LABEL_NAMES)[number]}`) : key;
}

/** Labels one explicit status value, keeping unknown states verbatim.
 * 为一个显式状态值提供标签，未知状态保持原样。
 * Parameters value and t select the state and active locale; returns its label or the value itself.
 * 参数 value 与 t 选择状态与当前语言；返回其标签或值本身。 */
function statusLabel(value: string, t: (key: TranslationKey) => string): string {
  return STATUS_LABEL_SET.has(value) ? t(`toolDomain.status.${value as (typeof STATUS_LABEL_NAMES)[number]}`) : value;
}

/** Renders exact JSON values as bounded, selectable fields with explicit null and empty states.
 * 将精确 JSON 值渲染为有界且可选择的字段，明确区分 null 与空集合。
 * Parameters value and field identify one result member; returns its read-only view.
 * 参数 value 与 field 标识一个结果成员；返回其只读视图。 */
function ToolResultValue(props: { value: ToolJsonValue; field?: string; depth?: number }) {
  // expanded controls only this long value; no tool action is attached.
  // expanded 只控制当前长值，不绑定任何工具动作。
  const [expanded, setExpanded] = React.useState(false);
  const { t, locale } = useI18n();
  // depth bounds eagerly rendered structure while keeping every branch available on demand.
  // depth 限制立即渲染的结构深度，同时保留每个分支按需查看的能力。
  const depth = props.depth ?? 0;
  // value is the exact persisted JSON member, not formatted data written back to the runtime.
  // value 是精确持久化 JSON 成员，不会将格式化数据回写运行时。
  const value = props.value;
  if (value === null) return <span className="v1-tool-domain-muted">-</span>;
  if (typeof value === "boolean") return <span>{value ? t("toolDomain.yes") : t("toolDomain.no")}</span>;
  if (typeof value === "number") return <span title={String(value)}>{Number.isInteger(value) ? value.toLocaleString(locale) : Number(value.toPrecision(6)).toString()}</span>;
  if (typeof value === "string") {
    // text labels only explicit status fields; arbitrary content remains untouched.
    // text 仅为显式状态字段提供标签；任意正文保持原样。
    const text = (props.field === "status" || props.field === "state") ? statusLabel(value, t) : value;
    if (text === "") return <span className="v1-tool-domain-muted">{t("toolDomain.emptyText")}</span>;
    // A source URL is clickable only for the exact url field and a web scheme.
    // 仅精确 url 字段且使用网页协议时，来源链接才可点击。
    if (props.field === "url" && /^https?:\/\//i.test(text)) return <a href={text} target="_blank" rel="noreferrer">{text}</a>;
    return <div className="v1-tool-domain-text"><span>{expanded ? text : text.slice(0, 900)}</span>{text.length > 900 ? <Button size="sm" variant="ghost" onClick={() => setExpanded(!expanded)}>{expanded ? t("toolDomain.collapseText") : formatMessage(t("toolDomain.expandText"), { count: text.length })}</Button> : null}</div>;
  }
  if (Array.isArray(value)) {
    if (value.length === 0) return <span className="v1-tool-domain-muted">{t("toolDomain.noItems")}</span>;
    return <div className="v1-tool-domain-list">
      {(expanded ? value : value.slice(0, 6)).map((entry, index) => <div className="v1-tool-domain-list-item" key={index}><ToolResultValue value={entry} depth={depth + 1} /></div>)}
      {value.length > 6 ? <Button size="sm" variant="ghost" onClick={() => setExpanded(!expanded)}>{expanded ? t("toolDomain.collapseList") : formatMessage(t("toolDomain.showAllItems"), { count: value.length })}</Button> : null}
    </div>;
  }
  // entries preserve all unknown members of dynamic tool contracts without guessing their semantics.
  // entries 保留动态工具契约的所有未知成员，不猜测其语义。
  const entries = Object.entries(value);
  if (entries.length === 0) return <span className="v1-tool-domain-muted">{t("toolDomain.noFields")}</span>;
  if (depth > 2 && !expanded) return <Button size="sm" variant="ghost" onClick={() => setExpanded(true)}>{formatMessage(t("toolDomain.showFields"), { count: entries.length })}</Button>;
  return <dl className="v1-tool-domain-fields">{entries.map(([key, entry]) => <div className="v1-tool-domain-field" key={key}><dt title={key}>{fieldLabel(key, t)}</dt><dd><ToolResultValue value={entry} field={key} depth={depth + 1} /></dd></div>)}</dl>;
}

/** Presents first-level domain results while retaining the existing Markdown renderer for declared text.
 * 展示首层领域结果，并对声明文本保留既有 Markdown 渲染器。
 * Parameter domain is a pure tool projection; returns a readable result body with explicit diagnostics.
 * 参数 domain 是纯工具投影；返回可读结果正文与显式诊断。 */
export function ConversationV1ToolDomainBody(props: { domain: V1ToolDomainPresentation }) {
  const { t } = useI18n();
  return <div className="v1-tool-domain" data-tool-domain-result>
    {props.domain.diagnostic ? <div className="v1-tool-domain-error" role="status">{props.domain.diagnostic}</div> : null}
    {props.domain.sections.map((section, index) => section.presentation === "configuration-package"
      ? <ConfigurationPackageResult key={`${section.title}:${index}`} section={section} />
      : section.presentation === "todo-list" ? <TodoListResult key={`${section.title}:${index}`} section={section} />
      : <section key={`${section.title}:${index}`} className="v1-tool-domain-section"><h4>{section.title}</h4>{section.markdown && typeof section.value === "string" ? <BoundedToolMarkdown text={section.value} /> : <ToolResultValue value={section.value} />}</section>)}
    {props.domain.sections.length === 0 && !props.domain.diagnostic ? <div className="v1-tool-domain-muted">{t("toolDomain.noResult")}</div> : null}
  </div>;
}


/** Shows package identity and completeness on one row, with declarations available on demand.
 * 将技能包身份与完整性显示在一行，按需展开声明。
 * Parameter section contains the exact redacted descriptor projection; returns its readable summary.
 * 参数 section 包含精确且已过滤配置值的声明投影；返回其可读摘要。 */
function ConfigurationPackageResult(props: { section: V1ToolDomainSection }) {
  // descriptor is owned by the configuration-package projection, never inferred from arbitrary tools.
  // descriptor 归 configuration-package 投影所有，不从任意工具推断。
  const descriptor = domainObject(props.section.value);
  // expanded mounts declaration fields only after a user request.
  // expanded 仅在用户请求后挂载声明字段。
  const [expanded, setExpanded] = React.useState(false);
  const { t } = useI18n();
  if (!descriptor) return <div className="v1-tool-domain-error">{t("toolDomain.config.invalidDescriptor")}</div>;
  // issueKeys are the four count fields defined by RuntimeSkillPackageConfigDescriptor.
  // issueKeys 是 RuntimeSkillPackageConfigDescriptor 定义的四个计数字段。
  const issueKeys = ["missing_count", "invalid_count", "business_issue_count", "orphaned_count"] as const;
  // issues shows only nonzero facts; full zero and missing distinctions remain in the tooltip and raw details.
  // issues 只显示非零事实；零值与缺失的完整区别保留在提示与原始详情。
  const issues = issueKeys.filter((key) => typeof descriptor[key] === "number" && descriptor[key] > 0);
  // countDetail does not invent zero for an absent count.
  // countDetail 不为缺失计数编造零值。
  const countDetail = issueKeys.map((key) => formatMessage(t("toolDomain.labelValue"), { label: fieldLabel(key, t), value: descriptor[key] === undefined ? "-" : String(descriptor[key]) })).join("\n");
  return <section className="v1-tool-config-package">
    <div className="v1-tool-config-heading"><strong>{props.section.title}</strong>
      {typeof descriptor.skill_version === "string" ? <span className="v1-tool-domain-muted">v{descriptor.skill_version}</span> : null}
      <span className="v1-tool-config-state" data-complete={typeof descriptor.complete === "boolean" ? descriptor.complete : undefined} title={countDetail}>{descriptor.complete === true ? t("toolDomain.config.complete") : descriptor.complete === false ? t("toolDomain.config.incomplete") : "-"}</span>
    </div>
    {issues.length > 0 ? <div className="v1-tool-config-issues">{issues.map((key) => <span key={key}>{fieldLabel(key, t)} {String(descriptor[key])}</span>)}</div> : null}
    {Array.isArray(descriptor.items) && descriptor.items.length > 0 ? <>
      <Button size="sm" variant="ghost" className="v1-tool-domain-raw-toggle" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>{formatMessage(t(expanded ? "toolDomain.config.hideItems" : "toolDomain.config.showItems"), { count: descriptor.items.length })}<ChevronDown aria-hidden="true" size={13} /></Button>
      {expanded ? <ToolResultValue value={descriptor.items} /> : null}
    </> : null}
  </section>;
}

/** Defers expensive Markdown work for long results until the user expands their body.
 * 对长结果延后昂贵的 Markdown 处理，直到用户展开正文。
 * Parameter text is exact declared Markdown; returns its bounded preview and full-content toggle.
 * 参数 text 是精确声明 Markdown；返回有界预览与全文开关。 */
function BoundedToolMarkdown(props: { text: string }) {
  // expanded is presentation-only and never mutates the persisted Markdown.
  // expanded 仅用于展示，绝不修改持久化 Markdown。
  const [expanded, setExpanded] = React.useState(false);
  const { t } = useI18n();
  return <div className="v1-tool-domain-markdown">{props.text.length <= 1600 || expanded ? <MarkdownMessage content={props.text} eager /> : <div className="v1-tool-domain-text">{props.text.slice(0, 900)}</div>}{props.text.length > 1600 ? <Button size="sm" variant="ghost" onClick={() => setExpanded(!expanded)}>{expanded ? t("toolDomain.collapseText") : formatMessage(t("toolDomain.expandText"), { count: props.text.length })}</Button> : null}</div>;
}

/** Renders an unknown tool's readable result and exact raw payloads as three peer tabs.
 * 将未知工具的可读结果与精确原始载荷渲染为三个同级标签页。
 * Parameters event and domain supply authoritative data; id isolates tab identity. Returns lazy, copyable detail panels.
 * 参数 event 与 domain 提供权威数据；id 隔离标签身份。返回按需挂载且可复制的详情面板。 */
export function ConversationV1GenericToolDetails(props: { event: ToolCallEvent; domain: V1ToolDomainPresentation; id: string }) {
  return <ConversationToolResultTabs event={props.event}>{toolResultMode(props.event) === "direct" ? <ConversationV1ToolDomainBody domain={props.domain} /> : undefined}</ConversationToolResultTabs>;
}
